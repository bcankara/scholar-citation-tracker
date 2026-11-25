//! SQLite storage: snapshots, articles, history, fetch log and UI query helpers.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Local, SecondsFormat, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;

use crate::i18n;
use crate::scholar::ProfileData;

pub const DB_FILE: &str = "scholar_takip.db";

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS snapshots(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    fetched_at TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    name TEXT NOT NULL,
    cit_all INTEGER NOT NULL,
    cit_since INTEGER NOT NULL,
    h_all INTEGER NOT NULL,
    h_since INTEGER NOT NULL,
    i10_all INTEGER NOT NULL,
    i10_since INTEGER NOT NULL,
    since_year INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_snapshots_fetched ON snapshots(fetched_at);
CREATE TABLE IF NOT EXISTS articles(
    article_key TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    authors TEXT NOT NULL DEFAULT '',
    venue TEXT NOT NULL DEFAULT '',
    year INTEGER,
    url TEXT NOT NULL DEFAULT '',
    first_seen TEXT NOT NULL,
    last_seen TEXT NOT NULL,
    cites_ids TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS article_citations(
    snapshot_id INTEGER NOT NULL,
    article_key TEXT NOT NULL,
    citations INTEGER NOT NULL,
    PRIMARY KEY(snapshot_id, article_key)
);
CREATE TABLE IF NOT EXISTS yearly_citations(
    snapshot_id INTEGER NOT NULL,
    year INTEGER NOT NULL,
    count INTEGER NOT NULL,
    PRIMARY KEY(snapshot_id, year)
);
CREATE TABLE IF NOT EXISTS fetch_log(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    cycle_started_at TEXT NOT NULL,
    attempted_at TEXT NOT NULL,
    attempt_no INTEGER NOT NULL,
    status TEXT NOT NULL,
    message TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS article_matches(
    article_key TEXT PRIMARY KEY,
    source TEXT,
    external_id TEXT,
    doi TEXT,
    score REAL,
    match_method TEXT,
    matched_at TEXT NOT NULL,
    openalex_id TEXT,
    s2_id TEXT, -- s2_id and s2_count: kept for old databases, no longer written
    openalex_count INTEGER,
    s2_count INTEGER
);
CREATE TABLE IF NOT EXISTS backfill_citations(
    article_key TEXT NOT NULL,
    source TEXT NOT NULL,
    citing_id TEXT NOT NULL,
    citing_title TEXT NOT NULL DEFAULT '',
    citing_date TEXT NOT NULL,
    date_precision TEXT NOT NULL CHECK(date_precision IN ('day','month','year')),
    venue TEXT NOT NULL DEFAULT '',
    doi TEXT NOT NULL DEFAULT '',
    url TEXT NOT NULL DEFAULT '',
    authors TEXT NOT NULL DEFAULT '',
    countries TEXT NOT NULL DEFAULT '',
    issn_list TEXT NOT NULL DEFAULT '',
    institutions TEXT NOT NULL DEFAULT '',
    authorships TEXT NOT NULL DEFAULT '',
    author_orcids TEXT NOT NULL DEFAULT '',
    topic TEXT NOT NULL DEFAULT '',
    topic_field TEXT NOT NULL DEFAULT '',
    topic_domain TEXT NOT NULL DEFAULT '',
    work_type TEXT NOT NULL DEFAULT '',
    language TEXT NOT NULL DEFAULT '',
    oa_status TEXT NOT NULL DEFAULT '',
    publisher TEXT NOT NULL DEFAULT '',
    source_type TEXT NOT NULL DEFAULT '',
    cited_by INTEGER,
    enriched INTEGER NOT NULL DEFAULT 0,
    source_id TEXT NOT NULL DEFAULT '',
    PRIMARY KEY(article_key, citing_id)
);
CREATE INDEX IF NOT EXISTS idx_backfill_date ON backfill_citations(citing_date);
CREATE TABLE IF NOT EXISTS backfill_runs(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    status TEXT NOT NULL,
    matched INTEGER NOT NULL DEFAULT 0,
    total INTEGER NOT NULL DEFAULT 0,
    records INTEGER NOT NULL DEFAULT 0,
    message TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS article_overrides(
    article_key TEXT PRIMARY KEY,
    doi TEXT,
    rejected_external_ids TEXT NOT NULL DEFAULT '[]',
    updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS app_state(
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
";

/// Canonical timestamp format. Fixed width UTC, so string order equals time order.
pub fn ts(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

/// Start of the current local day expressed in UTC.
pub fn local_midnight_utc(now: DateTime<Local>) -> DateTime<Utc> {
    let fallback = now.with_timezone(&Utc) - Duration::hours(24);
    let Some(naive) = now.date_naive().and_hms_opt(0, 0, 0) else {
        return fallback;
    };
    Local
        .from_local_datetime(&naive)
        .earliest()
        .or_else(|| Local.from_local_datetime(&naive).latest())
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or(fallback)
}

pub fn db_path_in(dir: &Path) -> PathBuf {
    dir.join(DB_FILE)
}

/// Serializes database initialization: several UI commands open a brand-new database at the
/// same moment, and switching it to WAL mode needs the file to itself.
static OPEN_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Opens (creating if needed) the database in WAL mode and ensures the schema.
pub fn open(path: &Path) -> Result<Connection> {
    let _init = OPEN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("could not create folder: {}", parent.display()))?;
    }
    let conn = Connection::open(path)
        .with_context(|| format!("could not open database: {}", path.display()))?;
    conn.busy_timeout(std::time::Duration::from_secs(10))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.execute_batch(SCHEMA)?;
    migrate(&conn)?;
    Ok(conn)
}

/// Columns added after a table first existed: (table, column, definition). Databases created
/// with the older column set get them here (idempotent: a column is only added when missing).
const EXTRA_COLUMNS: [(&str, &str, &str); 21] = [
    ("backfill_citations", "venue", "TEXT NOT NULL DEFAULT ''"),
    ("backfill_citations", "doi", "TEXT NOT NULL DEFAULT ''"),
    ("backfill_citations", "url", "TEXT NOT NULL DEFAULT ''"),
    ("backfill_citations", "authors", "TEXT NOT NULL DEFAULT ''"),
    ("backfill_citations", "countries", "TEXT NOT NULL DEFAULT ''"),
    ("backfill_citations", "issn_list", "TEXT NOT NULL DEFAULT ''"),
    ("articles", "cites_ids", "TEXT NOT NULL DEFAULT ''"),
    // 1.3.0: who cites you (OpenAlex citing-work details). `enriched` is 1 once a row was written
    // from a response that carried these fields, so rows that are empty because OpenAlex has
    // no data can be told apart from old rows that were never asked.
    ("backfill_citations", "institutions", "TEXT NOT NULL DEFAULT ''"),
    ("backfill_citations", "authorships", "TEXT NOT NULL DEFAULT ''"),
    ("backfill_citations", "author_orcids", "TEXT NOT NULL DEFAULT ''"),
    ("backfill_citations", "topic", "TEXT NOT NULL DEFAULT ''"),
    ("backfill_citations", "topic_field", "TEXT NOT NULL DEFAULT ''"),
    ("backfill_citations", "topic_domain", "TEXT NOT NULL DEFAULT ''"),
    ("backfill_citations", "work_type", "TEXT NOT NULL DEFAULT ''"),
    ("backfill_citations", "language", "TEXT NOT NULL DEFAULT ''"),
    ("backfill_citations", "oa_status", "TEXT NOT NULL DEFAULT ''"),
    ("backfill_citations", "publisher", "TEXT NOT NULL DEFAULT ''"),
    ("backfill_citations", "source_type", "TEXT NOT NULL DEFAULT ''"),
    ("backfill_citations", "cited_by", "INTEGER"),
    ("backfill_citations", "enriched", "INTEGER NOT NULL DEFAULT 0"),
    // 1.4.0: OpenAlex source id (S...) of the venue, the first key of the OJP lookup. Rows stored
    // before get it at the next refresh; until then the ISSNs match.
    ("backfill_citations", "source_id", "TEXT NOT NULL DEFAULT ''"),
];

pub fn migrate(conn: &Connection) -> Result<()> {
    for (table, col, def) in EXTRA_COLUMNS {
        let mut st = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let have: Vec<String> = st
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if have.is_empty() {
            continue; // the table does not exist (yet)
        }
        if !have.iter().any(|c| c == col) {
            conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {col} {def}"))?;
        }
    }
    Ok(())
}

/// Deletes every stored record (used when the tracked profile changes). Manual overrides and
/// the small app state go too: they belong to the previous profile's articles.
pub fn clear_all(conn: &mut Connection) -> Result<()> {
    let tx = conn.transaction()?;
    tx.execute_batch(
        "DELETE FROM article_citations;
         DELETE FROM yearly_citations;
         DELETE FROM snapshots;
         DELETE FROM articles;
         DELETE FROM fetch_log;
         DELETE FROM article_matches;
         DELETE FROM backfill_citations;
         DELETE FROM backfill_runs;
         DELETE FROM article_overrides;
         DELETE FROM app_state;",
    )?;
    tx.commit()?;
    conn.execute_batch("VACUUM;")?;
    Ok(())
}

/// Deletes only the historical backfill data (matches, citing records, runs).
/// `article_overrides` are kept on purpose: they are the user's own corrections (a DOI, or
/// works that were ruled out), not computed data, so the next run must still honour them.
/// Only `clear_all` (profile change) removes them.
pub fn clear_backfill(conn: &mut Connection) -> Result<()> {
    let tx = conn.transaction()?;
    tx.execute_batch(
        "DELETE FROM article_matches;
         DELETE FROM backfill_citations;
         DELETE FROM backfill_runs;
         DELETE FROM app_state WHERE key='last_citing_refresh';",
    )?;
    tx.commit()?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Snapshot {
    pub id: i64,
    pub fetched_at: String,
    pub profile_id: String,
    pub name: String,
    pub cit_all: i64,
    pub cit_since: i64,
    pub h_all: i64,
    pub h_since: i64,
    pub i10_all: i64,
    pub i10_since: i64,
    pub since_year: i64,
}

const SNAP_COLS: &str =
    "id, fetched_at, profile_id, name, cit_all, cit_since, h_all, h_since, i10_all, i10_since, since_year";

fn snap_from_row(r: &Row) -> rusqlite::Result<Snapshot> {
    Ok(Snapshot {
        id: r.get(0)?,
        fetched_at: r.get(1)?,
        profile_id: r.get(2)?,
        name: r.get(3)?,
        cit_all: r.get(4)?,
        cit_since: r.get(5)?,
        h_all: r.get(6)?,
        h_since: r.get(7)?,
        i10_all: r.get(8)?,
        i10_since: r.get(9)?,
        since_year: r.get(10)?,
    })
}

/// Inserts a complete snapshot in a single transaction and returns its id.
pub fn insert_snapshot(
    conn: &mut Connection,
    profile_id: &str,
    fetched_at: DateTime<Utc>,
    data: &ProfileData,
) -> Result<i64> {
    let when = ts(fetched_at);
    let tx = conn.transaction()?;
    let s = &data.stats;
    tx.execute(
        "INSERT INTO snapshots(fetched_at, profile_id, name, cit_all, cit_since, h_all, h_since, i10_all, i10_since, since_year)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        params![
            when, profile_id, data.name, s.cit_all, s.cit_since, s.h_all, s.h_since, s.i10_all,
            s.i10_since, s.since_year
        ],
    )?;
    let sid = tx.last_insert_rowid();
    {
        let mut upsert = tx.prepare(
            "INSERT INTO articles(article_key, title, authors, venue, year, url, first_seen, last_seen, cites_ids)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?7,?8)
             ON CONFLICT(article_key) DO UPDATE SET
               title=excluded.title, authors=excluded.authors, venue=excluded.venue,
               year=excluded.year, url=excluded.url, last_seen=excluded.last_seen,
               cites_ids=CASE WHEN excluded.cites_ids<>'' THEN excluded.cites_ids ELSE articles.cites_ids END",
        )?;
        let mut cit = tx.prepare(
            "INSERT OR REPLACE INTO article_citations(snapshot_id, article_key, citations) VALUES (?1,?2,?3)",
        )?;
        for a in &data.articles {
            upsert.execute(params![a.key, a.title, a.authors, a.venue, a.year, a.url, when, a.cites_ids])?;
            cit.execute(params![sid, a.key, a.citations])?;
        }
        let mut yr = tx.prepare(
            "INSERT OR REPLACE INTO yearly_citations(snapshot_id, year, count) VALUES (?1,?2,?3)",
        )?;
        for (year, count) in &data.yearly {
            yr.execute(params![sid, year, count])?;
        }
    }
    tx.commit()?;
    Ok(sid)
}

pub fn latest_snapshot(conn: &Connection) -> Result<Option<Snapshot>> {
    Ok(conn
        .query_row(
            &format!("SELECT {SNAP_COLS} FROM snapshots ORDER BY fetched_at DESC, id DESC LIMIT 1"),
            [],
            snap_from_row,
        )
        .optional()?)
}

pub fn earliest_snapshot(conn: &Connection) -> Result<Option<Snapshot>> {
    Ok(conn
        .query_row(
            &format!("SELECT {SNAP_COLS} FROM snapshots ORDER BY fetched_at ASC, id ASC LIMIT 1"),
            [],
            snap_from_row,
        )
        .optional()?)
}

pub fn snapshot_count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get(0))?)
}

/// Last snapshot strictly before `before`; falls back to the earliest snapshot.
fn reference_snapshot(conn: &Connection, before: DateTime<Utc>) -> Result<Option<Snapshot>> {
    let found = conn
        .query_row(
            &format!(
                "SELECT {SNAP_COLS} FROM snapshots WHERE fetched_at < ?1 ORDER BY fetched_at DESC, id DESC LIMIT 1"
            ),
            params![ts(before)],
            snap_from_row,
        )
        .optional()?;
    match found {
        Some(s) => Ok(Some(s)),
        None => earliest_snapshot(conn),
    }
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Delta {
    pub today: i64,
    pub d7: i64,
    pub d30: i64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Deltas {
    pub cit_all: Delta,
    pub cit_since: Delta,
    pub h_all: Delta,
    pub h_since: Delta,
    pub i10_all: Delta,
    pub i10_since: Delta,
}

/// Deltas of the latest snapshot versus reference snapshots:
/// before local midnight today (`today_start`), 7 days ago and 30 days ago.
pub fn deltas(
    conn: &Connection,
    today_start: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<Option<Deltas>> {
    let Some(latest) = latest_snapshot(conn)? else {
        return Ok(None);
    };
    let r_today = reference_snapshot(conn, today_start)?.unwrap_or_else(|| latest.clone());
    let r7 = reference_snapshot(conn, now - Duration::days(7))?.unwrap_or_else(|| latest.clone());
    let r30 = reference_snapshot(conn, now - Duration::days(30))?.unwrap_or_else(|| latest.clone());
    let d = |f: fn(&Snapshot) -> i64| Delta {
        today: f(&latest) - f(&r_today),
        d7: f(&latest) - f(&r7),
        d30: f(&latest) - f(&r30),
    };
    Ok(Some(Deltas {
        cit_all: d(|s| s.cit_all),
        cit_since: d(|s| s.cit_since),
        h_all: d(|s| s.h_all),
        h_since: d(|s| s.h_since),
        i10_all: d(|s| s.i10_all),
        i10_since: d(|s| s.i10_since),
    }))
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SeriesPoint {
    pub t: String,
    pub cit_all: i64,
    pub cit_since: i64,
    pub h_all: i64,
    pub h_since: i64,
    pub i10_all: i64,
    pub i10_since: i64,
}

pub fn time_series(conn: &Connection) -> Result<Vec<SeriesPoint>> {
    let mut st = conn.prepare(
        "SELECT fetched_at, cit_all, cit_since, h_all, h_since, i10_all, i10_since
         FROM snapshots ORDER BY fetched_at ASC, id ASC",
    )?;
    let rows = st.query_map([], |r| {
        Ok(SeriesPoint {
            t: r.get(0)?,
            cit_all: r.get(1)?,
            cit_since: r.get(2)?,
            h_all: r.get(3)?,
            h_since: r.get(4)?,
            i10_all: r.get(5)?,
            i10_since: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn yearly_for(conn: &Connection, snapshot_id: i64) -> Result<Vec<(i32, i64)>> {
    let mut st =
        conn.prepare("SELECT year, count FROM yearly_citations WHERE snapshot_id=?1 ORDER BY year")?;
    let rows = st.query_map(params![snapshot_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ArticleRow {
    pub key: String,
    pub title: String,
    pub authors: String,
    pub venue: String,
    pub year: Option<i64>,
    pub url: String,
    pub citations: i64,
    pub today: i64,
    pub d7: i64,
    pub d30: i64,
    pub is_new: bool,
    /// DOI of the article: manual override, else the matched work's DOI; `None` when unknown.
    pub doi: Option<String>,
    /// Comma-separated `cites=` ids of the Scholar profile page (for the "view on Scholar" link).
    pub cites_ids: String,
    /// Citing works identified for the article (OpenAlex); Scholar usually counts more and earlier.
    pub identified: i64,
}

/// Articles of the latest snapshot with deltas and the "new" flag.
pub fn articles_overview(
    conn: &Connection,
    today_start: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<Vec<ArticleRow>> {
    let Some(latest) = latest_snapshot(conn)? else {
        return Ok(Vec::new());
    };
    let first = earliest_snapshot(conn)?;
    let r_today = reference_snapshot(conn, today_start)?.map(|s| s.id);
    let r7 = reference_snapshot(conn, now - Duration::days(7))?.map(|s| s.id);
    let r30 = reference_snapshot(conn, now - Duration::days(30))?.map(|s| s.id);
    let new_cutoff = ts(now - Duration::days(7));
    let first_at = first.map(|s| s.fetched_at).unwrap_or_default();

    let mut st = conn.prepare(
        "SELECT a.article_key, a.title, a.authors, a.venue, a.year, a.url, a.first_seen, c.citations,
                (SELECT citations FROM article_citations WHERE snapshot_id=?2 AND article_key=a.article_key),
                (SELECT citations FROM article_citations WHERE snapshot_id=?3 AND article_key=a.article_key),
                (SELECT citations FROM article_citations WHERE snapshot_id=?4 AND article_key=a.article_key),
                COALESCE(NULLIF((SELECT doi FROM article_overrides WHERE article_key=a.article_key),''),
                         NULLIF((SELECT doi FROM article_matches WHERE article_key=a.article_key),'')),
                a.cites_ids,
                (SELECT COUNT(DISTINCT citing_id) FROM backfill_citations b WHERE b.article_key=a.article_key)
         FROM article_citations c JOIN articles a ON a.article_key=c.article_key
         WHERE c.snapshot_id=?1
         ORDER BY c.citations DESC, a.title ASC",
    )?;
    let rows = st.query_map(params![latest.id, r_today, r7, r30], |r| {
        let first_seen: String = r.get(6)?;
        let citations: i64 = r.get(7)?;
        let prev_today: Option<i64> = r.get(8)?;
        let prev_7: Option<i64> = r.get(9)?;
        let prev_30: Option<i64> = r.get(10)?;
        let is_new = first_seen > first_at && first_seen >= new_cutoff;
        Ok(ArticleRow {
            key: r.get(0)?,
            title: r.get(1)?,
            authors: r.get(2)?,
            venue: r.get(3)?,
            year: r.get(4)?,
            url: r.get(5)?,
            citations,
            today: citations - prev_today.unwrap_or(0),
            d7: citations - prev_7.unwrap_or(0),
            d30: citations - prev_30.unwrap_or(0),
            is_new,
            doi: r.get(11)?,
            cites_ids: r.get(12)?,
            identified: r.get(13)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn article_history(conn: &Connection, key: &str) -> Result<Vec<(String, i64)>> {
    let mut st = conn.prepare(
        "SELECT s.fetched_at, c.citations FROM article_citations c
         JOIN snapshots s ON s.id=c.snapshot_id
         WHERE c.article_key=?1 ORDER BY s.fetched_at ASC, s.id ASC",
    )?;
    let rows = st.query_map(params![key], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

// ------------------------------------------------- Scholar-detected changes

/// One snapshot at which an article's Scholar count differed from the snapshot before it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CitationChange {
    pub at: String,
    pub old: i64,
    pub new: i64,
    pub delta: i64,
}

/// Changes between consecutive points of one article's history (ascending by time). The first
/// point has nothing before it, so it never counts. Result: newest first.
pub fn extract_changes(history: &[(String, i64)]) -> Vec<CitationChange> {
    let mut out: Vec<CitationChange> = history
        .windows(2)
        .filter(|w| w[0].1 != w[1].1)
        .map(|w| CitationChange {
            at: w[1].0.clone(),
            old: w[0].1,
            new: w[1].1,
            delta: w[1].1 - w[0].1,
        })
        .collect();
    out.reverse();
    out
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct ChangeSummary {
    /// Sum of the increases (positive).
    pub up: i64,
    /// Sum of the decreases (positive magnitude).
    pub down: i64,
    pub net: i64,
}

/// Increases, decreases and net change of the changes at or after `since` (timestamp string).
pub fn summarize_changes(changes: &[CitationChange], since: &str) -> ChangeSummary {
    let mut s = ChangeSummary::default();
    for c in changes.iter().filter(|c| c.at.as_str() >= since) {
        if c.delta > 0 {
            s.up += c.delta;
        } else {
            s.down += -c.delta;
        }
    }
    s.net = s.up - s.down;
    s
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ArticleChanges {
    pub items: Vec<CitationChange>,
    pub last30: ChangeSummary,
}

pub fn article_changes(conn: &Connection, key: &str, now: DateTime<Utc>) -> Result<ArticleChanges> {
    let items = extract_changes(&article_history(conn, key)?);
    let last30 = summarize_changes(&items, &ts(now - Duration::days(30)));
    Ok(ArticleChanges { items, last30 })
}

/// A change of one article, with what the UI needs to show and link it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ScholarChange {
    pub article_key: String,
    pub title: String,
    pub cites_ids: String,
    pub at: String,
    pub old: i64,
    pub new: i64,
    pub delta: i64,
}

/// Every Scholar-detected change of every article, newest first.
pub fn scholar_changes(conn: &Connection) -> Result<Vec<ScholarChange>> {
    let mut st = conn.prepare(
        "SELECT c.article_key, a.title, a.cites_ids, s.fetched_at, c.citations
         FROM article_citations c
         JOIN snapshots s ON s.id=c.snapshot_id
         JOIN articles a ON a.article_key=c.article_key
         ORDER BY c.article_key ASC, s.fetched_at ASC, s.id ASC",
    )?;
    let rows = st
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut out = Vec::new();
    let mut i = 0;
    while i < rows.len() {
        let mut j = i;
        while j < rows.len() && rows[j].0 == rows[i].0 {
            j += 1;
        }
        let hist: Vec<(String, i64)> = rows[i..j].iter().map(|r| (r.3.clone(), r.4)).collect();
        for c in extract_changes(&hist) {
            out.push(ScholarChange {
                article_key: rows[i].0.clone(),
                title: rows[i].1.clone(),
                cites_ids: rows[i].2.clone(),
                at: c.at,
                old: c.old,
                new: c.new,
                delta: c.delta,
            });
        }
        i = j;
    }
    out.sort_by(|a, b| b.at.cmp(&a.at).then(a.title.cmp(&b.title)));
    Ok(out)
}

// ---------------------------------------------------------------- fetch log

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LogRow {
    pub id: i64,
    pub cycle_started_at: String,
    pub attempted_at: String,
    pub attempt_no: i64,
    pub status: String,
    pub message: String,
}

pub fn log_attempt(
    conn: &Connection,
    cycle_started_at: &str,
    attempted_at: &str,
    attempt_no: u32,
    status: &str,
    message: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO fetch_log(cycle_started_at, attempted_at, attempt_no, status, message) VALUES (?1,?2,?3,?4,?5)",
        params![cycle_started_at, attempted_at, attempt_no, status, message],
    )?;
    Ok(())
}

pub fn recent_log(conn: &Connection, limit: i64) -> Result<Vec<LogRow>> {
    let mut st = conn.prepare(
        "SELECT id, cycle_started_at, attempted_at, attempt_no, status, message
         FROM fetch_log ORDER BY id DESC LIMIT ?1",
    )?;
    let rows = st.query_map(params![limit], |r| {
        Ok(LogRow {
            id: r.get(0)?,
            cycle_started_at: r.get(1)?,
            attempted_at: r.get(2)?,
            attempt_no: r.get(3)?,
            status: r.get(4)?,
            message: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Start time of the most recent cycle recorded in the log.
pub fn last_cycle_start(conn: &Connection) -> Result<Option<DateTime<Utc>>> {
    let s: Option<String> = conn
        .query_row("SELECT MAX(cycle_started_at) FROM fetch_log", [], |r| r.get(0))
        .optional()?
        .flatten();
    Ok(s.and_then(|s| parse_ts(&s)))
}

// ------------------------------------------------------------ change summary

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Change {
    pub total: i64,
    pub total_delta: i64,
    /// (title, increase) for articles whose count went up, largest first.
    pub increases: Vec<(String, i64)>,
    /// (title, decrease as a positive number) for articles whose count went down. Never notified.
    pub decreases: Vec<(String, i64)>,
    /// Titles of articles first seen in this snapshot.
    pub new_articles: Vec<String>,
}

/// Compares a snapshot with the one before it. `None` for the very first snapshot.
pub fn change_since_previous(conn: &Connection, snapshot_id: i64) -> Result<Option<Change>> {
    let prev: Option<Snapshot> = conn
        .query_row(
            &format!("SELECT {SNAP_COLS} FROM snapshots WHERE id < ?1 ORDER BY id DESC LIMIT 1"),
            params![snapshot_id],
            snap_from_row,
        )
        .optional()?;
    let Some(prev) = prev else {
        return Ok(None);
    };
    let cur: Snapshot = conn.query_row(
        &format!("SELECT {SNAP_COLS} FROM snapshots WHERE id = ?1"),
        params![snapshot_id],
        snap_from_row,
    )?;
    let mut st = conn.prepare(
        "SELECT a.title, c.citations, o.citations
         FROM article_citations c
         JOIN articles a ON a.article_key=c.article_key
         LEFT JOIN article_citations o ON o.article_key=c.article_key AND o.snapshot_id=?2
         WHERE c.snapshot_id=?1",
    )?;
    let rows = st.query_map(params![snapshot_id, prev.id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, Option<i64>>(2)?,
        ))
    })?;
    let mut change = Change {
        total: cur.cit_all,
        total_delta: cur.cit_all - prev.cit_all,
        ..Change::default()
    };
    for row in rows {
        let (title, now, before) = row?;
        match before {
            None => {
                change.new_articles.push(title.clone());
                if now > 0 {
                    change.increases.push((title, now));
                }
            }
            Some(b) if now > b => change.increases.push((title, now - b)),
            Some(b) if now < b => change.decreases.push((title, b - now)),
            _ => {}
        }
    }
    change.increases.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    change.decreases.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    change.new_articles.sort();
    Ok(Some(change))
}

pub fn shorten(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{}…", cut.trim_end())
    }
}

/// Builds the notification (title, body) in `lang`. `None` when nothing changed.
pub fn notification_text(c: &Change, lang: &str) -> Option<(String, String)> {
    let increase_sum: i64 = c.increases.iter().map(|(_, d)| d).sum();
    if c.total_delta <= 0 && increase_sum <= 0 && c.new_articles.is_empty() {
        return None;
    }
    let total = c.total.to_string();
    let title = if c.total_delta > 0 {
        i18n::trf(lang, "notif.new_citations", &[&c.total_delta.to_string(), &total])
    } else if increase_sum > 0 {
        i18n::trf(lang, "notif.new_citations", &[&increase_sum.to_string(), &total])
    } else {
        i18n::trf(lang, "notif.new_article_title", &[&total])
    };
    let mut lines: Vec<String> = c
        .increases
        .iter()
        .map(|(t, d)| format!("{} +{}", shorten(t, 45), d))
        .collect();
    for t in &c.new_articles {
        if !c.increases.iter().any(|(it, _)| it == t) {
            lines.push(i18n::trf(lang, "notif.new_article_line", &[&shorten(t, 45)]));
        }
    }
    lines.truncate(3);
    Some((title, lines.join("
")))
}

// ------------------------------------------------------- historical backfill

/// Outcome of matching one Scholar article against OpenAlex.
/// `source == None` means "searched, not matched".
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct MatchRecord {
    pub article_key: String,
    pub source: Option<String>,
    pub external_id: Option<String>,
    pub doi: Option<String>,
    pub score: Option<f64>,
    /// "orcid" | "title_search" | "manual_doi"; `None` for unmatched articles without a manual DOI.
    pub match_method: Option<String>,
    pub openalex_id: Option<String>,
    pub openalex_count: Option<i64>,
}

/// One citing work with the date it is filed under.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CitingRecord {
    pub citing_id: String,
    pub citing_title: String,
    /// `YYYY-MM-DD`, `YYYY-MM` or `YYYY`, matching `date_precision`.
    pub citing_date: String,
    /// "day" | "month" | "year"
    pub date_precision: String,
    /// Journal / venue name of the citing work (empty when unknown).
    pub venue: String,
    /// Bare DOI of the citing work (empty when unknown).
    pub doi: String,
    pub url: String,
    /// Authors, `; `-separated (first ten, then "et al.").
    pub authors: String,
    /// ISO country codes of the authors' institutions, sorted, comma-separated. Only OpenAlex has them.
    pub countries: String,
    /// Normalized ISSNs of the venue (8 characters, no hyphen), comma-separated.
    pub issn_list: String,
    /// Distinct institutions of all authors, JSON: `[{"i":"I123","n":"Name","c":"TR"}]` (empty when unknown).
    pub institutions: String,
    /// Authorships, JSON: `[{"n":"Ada Example","o":"0000-0002-1825-0097","i":["I123"]}]` (at most
    /// `MAX_AUTHORSHIPS`; `o` is the bare ORCID iD, `i` the institution ids).
    pub authorships: String,
    /// Bare ORCID iDs of the authors, comma-separated.
    pub author_orcids: String,
    /// Primary topic, its field and its domain (empty when unknown).
    pub topic: String,
    pub topic_field: String,
    pub topic_domain: String,
    /// OpenAlex work type: article, review, preprint, book-chapter, dissertation, ...
    pub work_type: String,
    /// ISO 639-1 language code (empty when unknown).
    pub language: String,
    /// gold, green, hybrid, bronze, diamond, closed (empty when unknown).
    pub oa_status: String,
    /// Host organization of the venue (publisher).
    pub publisher: String,
    /// OpenAlex source type: journal, conference, repository, ... (empty when unknown).
    pub source_type: String,
    /// How often the citing work itself is cited (`None` when unknown).
    pub cited_by: Option<i64>,
    /// OpenAlex source id of the venue (`S...`), empty when unknown.
    pub source_id: String,
}

/// Inserts the citing rows of one article (the caller has removed the old ones).
fn insert_citing_rows(tx: &rusqlite::Transaction, key: &str, source: &str, citing: &[CitingRecord]) -> Result<()> {
    let mut ins = tx.prepare(
        "INSERT OR REPLACE INTO backfill_citations
           (article_key, source, citing_id, citing_title, citing_date, date_precision,
            venue, doi, url, authors, countries, issn_list,
            institutions, authorships, author_orcids, topic, topic_field, topic_domain, work_type,
            language, oa_status, publisher, source_type, cited_by, enriched, source_id)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,1,?25)",
    )?;
    for c in citing {
        ins.execute(params![
            key,
            source,
            c.citing_id,
            c.citing_title,
            c.citing_date,
            c.date_precision,
            c.venue,
            c.doi,
            c.url,
            c.authors,
            c.countries,
            c.issn_list,
            c.institutions,
            c.authorships,
            c.author_orcids,
            c.topic,
            c.topic_field,
            c.topic_domain,
            c.work_type,
            c.language,
            c.oa_status,
            c.publisher,
            c.source_type,
            c.cited_by,
            c.source_id
        ])?;
    }
    Ok(())
}

fn stored_citing_ids(tx: &rusqlite::Transaction, key: &str) -> Result<std::collections::HashSet<String>> {
    let mut st = tx.prepare("SELECT citing_id FROM backfill_citations WHERE article_key=?1")?;
    let rows = st.query_map(params![key], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<std::collections::HashSet<_>>>()?)
}

/// Citing works that were not stored before. Nothing counts as new when the article had no
/// earlier result from the same source (first run, or the source changed): everything would be.
fn new_citing(
    prev_source: Option<&str>,
    source: Option<&str>,
    before: &std::collections::HashSet<String>,
    citing: &[CitingRecord],
) -> Vec<CitingRecord> {
    match (prev_source, source) {
        (Some(p), Some(s)) if p == s => citing.iter().filter(|c| !before.contains(&c.citing_id)).cloned().collect(),
        _ => Vec::new(),
    }
}

/// Replaces everything stored for one article (idempotent): old citing rows are removed,
/// the match row is upserted and the new citing rows are inserted, in one transaction.
/// Returns the citing works that were not stored before (see `new_citing`).
pub fn store_backfill_article(
    conn: &mut Connection,
    rec: &MatchRecord,
    matched_at: DateTime<Utc>,
    citing: &[CitingRecord],
) -> Result<Vec<CitingRecord>> {
    let tx = conn.transaction()?;
    let prev_source: Option<String> = tx
        .query_row(
            "SELECT source FROM article_matches WHERE article_key=?1",
            params![rec.article_key],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten();
    let before = stored_citing_ids(&tx, &rec.article_key)?;
    tx.execute(
        "DELETE FROM backfill_citations WHERE article_key=?1",
        params![rec.article_key],
    )?;
    tx.execute(
        "INSERT INTO article_matches(article_key, source, external_id, doi, score, match_method,
                                     matched_at, openalex_id, openalex_count)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
         ON CONFLICT(article_key) DO UPDATE SET
           source=excluded.source, external_id=excluded.external_id, doi=excluded.doi,
           score=excluded.score, match_method=excluded.match_method, matched_at=excluded.matched_at,
           openalex_id=excluded.openalex_id, openalex_count=excluded.openalex_count",
        params![
            rec.article_key,
            rec.source,
            rec.external_id,
            rec.doi,
            rec.score,
            rec.match_method,
            ts(matched_at),
            rec.openalex_id,
            rec.openalex_count
        ],
    )?;
    if let Some(source) = &rec.source {
        insert_citing_rows(&tx, &rec.article_key, source, citing)?;
    }
    let fresh = new_citing(prev_source.as_deref(), rec.source.as_deref(), &before, citing);
    tx.commit()?;
    Ok(fresh)
}

/// An already matched article whose citing list the daily refresh re-reads.
#[derive(Debug, Clone, PartialEq)]
pub struct RefreshTarget {
    pub key: String,
    pub title: String,
    /// Always "openalex" (older databases may still hold rows from a source that is no longer used).
    pub source: String,
    /// Id of the work in that source.
    pub external_id: String,
}

/// Matched articles of the latest snapshot (with the id used in the chosen source).
pub fn refresh_targets(conn: &Connection) -> Result<Vec<RefreshTarget>> {
    let Some(latest) = latest_snapshot(conn)? else {
        return Ok(Vec::new());
    };
    let mut st = conn.prepare(
        "SELECT a.article_key, a.title, m.source, m.external_id
         FROM article_citations c
         JOIN articles a ON a.article_key=c.article_key
         JOIN article_matches m ON m.article_key=a.article_key
         WHERE c.snapshot_id=?1 AND m.source='openalex' AND m.external_id IS NOT NULL
         ORDER BY EXISTS(SELECT 1 FROM backfill_citations b
                         WHERE b.article_key=a.article_key AND b.enriched=0) DESC,
                  c.citations DESC, a.title ASC",
    )?;
    let rows = st.query_map(params![latest.id], |r| {
        Ok(RefreshTarget {
            key: r.get(0)?,
            title: r.get(1)?,
            source: r.get(2)?,
            external_id: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Daily refresh: replaces the citing rows of one matched article and keeps its match row
/// (only the count of the used source is updated). Returns the citing works new to the database.
pub fn refresh_citing(
    conn: &mut Connection,
    key: &str,
    source: &str,
    citing: &[CitingRecord],
) -> Result<Vec<CitingRecord>> {
    let tx = conn.transaction()?;
    let before = stored_citing_ids(&tx, key)?;
    tx.execute("DELETE FROM backfill_citations WHERE article_key=?1", params![key])?;
    insert_citing_rows(&tx, key, source, citing)?;
    tx.execute(
        "UPDATE article_matches SET openalex_count=?2 WHERE article_key=?1",
        params![key, citing.len() as i64],
    )?;
    let fresh = new_citing(Some(source), Some(source), &before, citing);
    tx.commit()?;
    Ok(fresh)
}

/// Stored citing rows that were written before 1.3.0 and still lack the "who cites you" fields.
/// The daily refresh fills them (articles with such rows are read first). Only rows the refresh
/// can reach count: articles matched in OpenAlex that are in the latest snapshot.
pub fn unenriched_rows(conn: &Connection) -> Result<i64> {
    let Some(latest) = latest_snapshot(conn)? else {
        return Ok(0);
    };
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM backfill_citations b
         WHERE b.enriched=0
           AND b.article_key IN (SELECT article_key FROM article_citations WHERE snapshot_id=?1)
           AND b.article_key IN (SELECT article_key FROM article_matches
                                 WHERE source='openalex' AND external_id IS NOT NULL)",
        params![latest.id],
        |r| r.get(0),
    )?)
}

/// True when at least one article is matched (the refresh has work to do).
pub fn has_refresh_targets(conn: &Connection) -> Result<bool> {
    Ok(!refresh_targets(conn)?.is_empty())
}

/// An article of the latest snapshot that backfill should look up.
#[derive(Debug, Clone, PartialEq)]
pub struct BackfillTarget {
    pub key: String,
    pub title: String,
    pub year: Option<i64>,
}

/// Articles of the latest snapshot, most cited first. With `only_missing`, articles that
/// already have a match row (matched or not) are skipped.
pub fn backfill_targets(conn: &Connection, only_missing: bool) -> Result<Vec<BackfillTarget>> {
    let Some(latest) = latest_snapshot(conn)? else {
        return Ok(Vec::new());
    };
    let filter = if only_missing {
        "AND NOT EXISTS (SELECT 1 FROM article_matches m WHERE m.article_key=a.article_key)"
    } else {
        ""
    };
    let mut st = conn.prepare(&format!(
        "SELECT a.article_key, a.title, a.year
         FROM article_citations c JOIN articles a ON a.article_key=c.article_key
         WHERE c.snapshot_id=?1 {filter}
         ORDER BY c.citations DESC, a.title ASC"
    ))?;
    let rows = st.query_map(params![latest.id], |r| {
        Ok(BackfillTarget {
            key: r.get(0)?,
            title: r.get(1)?,
            year: r.get(2)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// OpenAlex work ids already tied to an article (so a work is never given to two articles).
pub fn matched_openalex_ids(conn: &Connection) -> Result<std::collections::HashSet<String>> {
    let mut st = conn.prepare("SELECT openalex_id FROM article_matches WHERE openalex_id IS NOT NULL")?;
    let rows = st.query_map([], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<std::collections::HashSet<_>>>()?)
}

/// Matched ids of every article except `key` (single-article re-run).
pub fn matched_openalex_ids_except(conn: &Connection, key: &str) -> Result<std::collections::HashSet<String>> {
    let mut st = conn.prepare(
        "SELECT openalex_id FROM article_matches WHERE openalex_id IS NOT NULL AND article_key<>?1",
    )?;
    let rows = st.query_map(params![key], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<std::collections::HashSet<_>>>()?)
}

/// One article of the latest snapshot (for a single-article re-run).
pub fn backfill_target_for(conn: &Connection, key: &str) -> Result<Option<BackfillTarget>> {
    let Some(latest) = latest_snapshot(conn)? else {
        return Ok(None);
    };
    Ok(conn
        .query_row(
            "SELECT a.article_key, a.title, a.year
             FROM article_citations c JOIN articles a ON a.article_key=c.article_key
             WHERE c.snapshot_id=?1 AND a.article_key=?2",
            params![latest.id, key],
            |r| {
                Ok(BackfillTarget {
                    key: r.get(0)?,
                    title: r.get(1)?,
                    year: r.get(2)?,
                })
            },
        )
        .optional()?)
}

// ------------------------------------------------------- manual overrides

/// The user's corrections for one article's historical match.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Override {
    /// Normalized DOI that replaces automatic matching.
    pub doi: Option<String>,
    /// External work ids (OpenAlex `W...`) ruled out as matches.
    pub rejected: Vec<String>,
}

impl Override {
    pub fn rejected_set(&self) -> std::collections::HashSet<String> {
        self.rejected.iter().cloned().collect()
    }
}

/// What the UI needs to know about an override.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct OverrideInfo {
    pub doi: Option<String>,
    pub rejected: usize,
}

fn override_from_row(doi: Option<String>, rejected_json: String) -> Override {
    Override {
        doi,
        rejected: serde_json::from_str::<Vec<String>>(&rejected_json).unwrap_or_default(),
    }
}

pub fn get_override(conn: &Connection, key: &str) -> Result<Option<Override>> {
    Ok(conn
        .query_row(
            "SELECT doi, rejected_external_ids FROM article_overrides WHERE article_key=?1",
            params![key],
            |r| Ok(override_from_row(r.get(0)?, r.get(1)?)),
        )
        .optional()?)
}

pub fn all_overrides(conn: &Connection) -> Result<std::collections::HashMap<String, Override>> {
    let mut st = conn.prepare("SELECT article_key, doi, rejected_external_ids FROM article_overrides")?;
    let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, override_from_row(r.get(1)?, r.get(2)?))))?;
    Ok(rows.collect::<rusqlite::Result<std::collections::HashMap<_, _>>>()?)
}

/// Writes the override row, or removes it when it holds nothing.
fn put_override(conn: &Connection, key: &str, o: &Override, now: DateTime<Utc>) -> Result<()> {
    if o.doi.is_none() && o.rejected.is_empty() {
        conn.execute("DELETE FROM article_overrides WHERE article_key=?1", params![key])?;
        return Ok(());
    }
    conn.execute(
        "INSERT INTO article_overrides(article_key, doi, rejected_external_ids, updated_at)
         VALUES (?1,?2,?3,?4)
         ON CONFLICT(article_key) DO UPDATE SET
           doi=excluded.doi, rejected_external_ids=excluded.rejected_external_ids,
           updated_at=excluded.updated_at",
        params![key, o.doi, serde_json::to_string(&o.rejected)?, ts(now)],
    )?;
    Ok(())
}

/// Sets (or, with `None`, removes) the manual DOI; rejected ids are kept.
pub fn set_override_doi(conn: &Connection, key: &str, doi: Option<&str>, now: DateTime<Utc>) -> Result<()> {
    let mut o = get_override(conn, key)?.unwrap_or_default();
    o.doi = doi.map(str::to_string);
    put_override(conn, key, &o, now)
}

/// Adds work ids to the article's rejected list (no duplicates, empty ids ignored).
pub fn add_rejected(conn: &Connection, key: &str, ids: &[String], now: DateTime<Utc>) -> Result<()> {
    let mut o = get_override(conn, key)?.unwrap_or_default();
    for id in ids.iter().filter(|i| !i.is_empty()) {
        if !o.rejected.contains(id) {
            o.rejected.push(id.clone());
        }
    }
    put_override(conn, key, &o, now)
}

/// Back to fully automatic matching for this article.
pub fn reset_override(conn: &Connection, key: &str) -> Result<()> {
    conn.execute("DELETE FROM article_overrides WHERE article_key=?1", params![key])?;
    Ok(())
}

// -------------------------------------------------------------- app state

pub fn get_state(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row("SELECT value FROM app_state WHERE key=?1", params![key], |r| r.get(0))
        .optional()?)
}

pub fn set_state(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO app_state(key, value) VALUES (?1,?2)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn delete_state(conn: &Connection, key: &str) -> Result<()> {
    conn.execute("DELETE FROM app_state WHERE key=?1", params![key])?;
    Ok(())
}

pub const STATE_HEALTH_NOTIFIED: &str = "last_health_notification";

// ------------------------------------------------------------ fetch health

/// Timestamps from `fetch_log` that the health rule needs.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FetchTimes {
    pub last_ok: Option<DateTime<Utc>>,
    pub first_attempt: Option<DateTime<Utc>>,
    pub last_attempt: Option<DateTime<Utc>>,
}

pub fn fetch_times(conn: &Connection) -> Result<FetchTimes> {
    let (ok, first, last): (Option<String>, Option<String>, Option<String>) = conn.query_row(
        "SELECT MAX(CASE WHEN status='ok' THEN attempted_at END), MIN(attempted_at), MAX(attempted_at)
         FROM fetch_log",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    Ok(FetchTimes {
        last_ok: ok.and_then(|s| parse_ts(&s)),
        first_attempt: first.and_then(|s| parse_ts(&s)),
        last_attempt: last.and_then(|s| parse_ts(&s)),
    })
}

/// Citation total and article count of the latest stored snapshot.
pub fn latest_summary(conn: &Connection) -> Result<Option<(i64, usize)>> {
    let Some(s) = latest_snapshot(conn)? else {
        return Ok(None);
    };
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM article_citations WHERE snapshot_id=?1",
        params![s.id],
        |r| r.get(0),
    )?;
    Ok(Some((s.cit_all, n as usize)))
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RunRow {
    pub id: i64,
    pub started_at: String,
    pub finished_at: Option<String>,
    /// "running" | "ok" | "partial" | "error" | "interrupted"
    pub status: String,
    pub matched: i64,
    pub total: i64,
    pub records: i64,
    pub message: String,
}

pub fn start_run(conn: &Connection, started: DateTime<Utc>, total: usize) -> Result<i64> {
    conn.execute(
        "INSERT INTO backfill_runs(started_at, status, total) VALUES (?1,'running',?2)",
        params![ts(started), total as i64],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn finish_run(
    conn: &Connection,
    id: i64,
    finished: DateTime<Utc>,
    status: &str,
    counts: (usize, usize),
    message: &str,
) -> Result<()> {
    let (matched, records) = counts;
    conn.execute(
        "UPDATE backfill_runs SET finished_at=?2, status=?3, matched=?4, records=?5, message=?6 WHERE id=?1",
        params![id, ts(finished), status, matched as i64, records as i64, message],
    )?;
    Ok(())
}

/// Runs left in "running" by a closed app are marked "interrupted".
pub fn mark_stale_runs(conn: &Connection, now: DateTime<Utc>, message: &str) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE backfill_runs SET status='interrupted', finished_at=?1, message=?2 WHERE status='running'",
        params![ts(now), message],
    )?)
}

fn run_from_row(r: &Row) -> rusqlite::Result<RunRow> {
    Ok(RunRow {
        id: r.get(0)?,
        started_at: r.get(1)?,
        finished_at: r.get(2)?,
        status: r.get(3)?,
        matched: r.get(4)?,
        total: r.get(5)?,
        records: r.get(6)?,
        message: r.get(7)?,
    })
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BackfillSummary {
    /// Articles of the latest snapshot.
    pub total_articles: i64,
    pub matched: i64,
    pub unmatched: i64,
    /// Articles of the latest snapshot without any match row yet.
    pub pending: i64,
    /// Stored citing records.
    pub records: i64,
    pub last_run: Option<RunRow>,
}

pub fn backfill_summary(conn: &Connection) -> Result<BackfillSummary> {
    let latest = latest_snapshot(conn)?;
    let sid = latest.map(|s| s.id).unwrap_or(-1);
    let total_articles: i64 = conn.query_row(
        "SELECT COUNT(*) FROM article_citations WHERE snapshot_id=?1",
        params![sid],
        |r| r.get(0),
    )?;
    let (matched, unmatched): (i64, i64) = conn.query_row(
        "SELECT COALESCE(SUM(m.source IS NOT NULL),0), COALESCE(SUM(m.source IS NULL),0)
         FROM article_citations c JOIN article_matches m ON m.article_key=c.article_key
         WHERE c.snapshot_id=?1",
        params![sid],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let records: i64 = conn.query_row("SELECT COUNT(*) FROM backfill_citations", [], |r| r.get(0))?;
    let last_run = conn
        .query_row(
            "SELECT id, started_at, finished_at, status, matched, total, records, message
             FROM backfill_runs ORDER BY id DESC LIMIT 1",
            [],
            run_from_row,
        )
        .optional()?;
    Ok(BackfillSummary {
        total_articles,
        matched,
        unmatched,
        pending: total_articles - matched - unmatched,
        records,
        last_run,
    })
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HistorySeries {
    pub granularity: String,
    /// (period key, new citations) in ascending order. Keys are `YYYY-MM-DD`, `YYYY-MM` or `YYYY`.
    pub points: Vec<(String, i64)>,
    /// All stored citing records.
    pub total: i64,
    /// Records that cannot be placed at this granularity (too coarse a date).
    pub excluded: i64,
}

/// New citations per period over all articles, one count per (article, citing work).
/// Day uses day-precision records only; month uses day and month precision; year uses all.
pub fn history_series(conn: &Connection, granularity: &str) -> Result<HistorySeries> {
    let (expr, filter, gran) = match granularity {
        "day" => ("citing_date", "WHERE date_precision='day'", "day"),
        "month" => (
            "substr(citing_date,1,7)",
            "WHERE date_precision IN ('day','month')",
            "month",
        ),
        _ => ("substr(citing_date,1,4)", "", "year"),
    };
    let mut st = conn.prepare(&format!(
        "SELECT {expr} AS k, COUNT(*) FROM backfill_citations {filter} GROUP BY k ORDER BY k ASC"
    ))?;
    let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    let points = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    let total: i64 = conn.query_row("SELECT COUNT(*) FROM backfill_citations", [], |r| r.get(0))?;
    let included: i64 = points.iter().map(|p| p.1).sum();
    Ok(HistorySeries {
        granularity: gran.to_string(),
        points,
        total,
        excluded: total - included,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct ArticleBackfill {
    #[serde(rename = "match")]
    pub match_info: Option<MatchRecord>,
    /// The user's manual DOI / rejected matches for this article, if any.
    #[serde(rename = "override")]
    pub override_info: Option<OverrideInfo>,
    /// (`YYYY-MM`, count) from day and month precision records.
    pub monthly: Vec<(String, i64)>,
    /// Records with a year-only date (not in `monthly`).
    pub year_only: i64,
    pub total: i64,
}

pub fn article_backfill(conn: &Connection, key: &str) -> Result<ArticleBackfill> {
    let match_info = conn
        .query_row(
            "SELECT article_key, source, external_id, doi, score, match_method, openalex_id,
                    openalex_count
             FROM article_matches WHERE article_key=?1",
            params![key],
            |r| {
                Ok(MatchRecord {
                    article_key: r.get(0)?,
                    source: r.get(1)?,
                    external_id: r.get(2)?,
                    doi: r.get(3)?,
                    score: r.get(4)?,
                    match_method: r.get(5)?,
                    openalex_id: r.get(6)?,
                    openalex_count: r.get(7)?,
                })
            },
        )
        .optional()?;
    let mut st = conn.prepare(
        "SELECT substr(citing_date,1,7) AS k, COUNT(*) FROM backfill_citations
         WHERE article_key=?1 AND date_precision IN ('day','month') GROUP BY k ORDER BY k ASC",
    )?;
    let rows = st.query_map(params![key], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    let monthly = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM backfill_citations WHERE article_key=?1",
        params![key],
        |r| r.get(0),
    )?;
    let in_months: i64 = monthly.iter().map(|m| m.1).sum();
    let override_info = get_override(conn, key)?.map(|o| OverrideInfo {
        doi: o.doi,
        rejected: o.rejected.len(),
    });
    Ok(ArticleBackfill {
        match_info,
        override_info,
        monthly,
        year_only: total - in_months,
        total,
    })
}

// ------------------------------------------------- citing works and insights

pub const STATE_GOAL: &str = "goal_citations";
pub const STATE_WEEKLY: &str = "weekly_summary_week";
pub const STATE_REFRESH: &str = "last_citing_refresh";
/// Last refresh run that tried to fill older rows: `timestamp|unenriched before|unenriched after`.
pub const STATE_ENRICH_TRY: &str = "last_enrich_attempt";
/// Set once the 1.1.1 clean-up (matches made with the retired second source) has run.
pub const STATE_LEGACY_SOURCE_DROPPED: &str = "legacy_source_dropped_v111";

/// 1.1.1 migration: articles whose stored match came from the source that was dropped lose
/// their citing rows and their match row, so the next backfill matches them again with
/// OpenAlex. Manual DOI / rejected-work overrides live in their own table and are kept.
/// Runs once (recorded in `app_state`); returns the number of articles reset.
pub fn drop_legacy_source_matches(conn: &mut Connection) -> Result<usize> {
    if get_state(conn, STATE_LEGACY_SOURCE_DROPPED)?.is_some() {
        return Ok(0);
    }
    let tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM backfill_citations WHERE article_key IN
           (SELECT article_key FROM article_matches WHERE source='semanticscholar')
         OR source='semanticscholar'",
        [],
    )?;
    let n = tx.execute("DELETE FROM article_matches WHERE source='semanticscholar'", [])?;
    tx.execute(
        "INSERT INTO app_state(key, value) VALUES (?1,?2)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![STATE_LEGACY_SOURCE_DROPPED, ts(Utc::now())],
    )?;
    tx.commit()?;
    Ok(n)
}

/// One stored citing work of one article.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct CitingRow {
    pub article_key: String,
    pub article_title: String,
    pub source: String,
    pub citing_id: String,
    pub title: String,
    pub date: String,
    pub precision: String,
    pub venue: String,
    pub doi: String,
    pub url: String,
    pub authors: String,
    pub countries: String,
    pub issn_list: String,
    pub institutions: String,
    pub author_orcids: String,
    pub topic: String,
    pub topic_field: String,
    pub topic_domain: String,
    pub work_type: String,
    pub language: String,
    pub oa_status: String,
    pub publisher: String,
    pub source_type: String,
    pub cited_by: Option<i64>,
    /// False for rows stored before 1.3.0 that the next refresh has not filled yet.
    pub enriched: bool,
    /// Set by the command (needs the user's ORCID and name): the citing work is a self-citation.
    pub self_cite: bool,
    /// Set by the command: not a self-citation, but an author is a co-author of one of the user's works.
    pub coauthor_cite: bool,
    #[serde(skip)]
    pub source_id: String,
    /// OJP of the venue's journal (0-100), `None` when it has no score.
    pub ojp: Option<u8>,
    /// ranked | few | unscored | absent | no_journal
    pub ojp_state: crate::ojp::State,
    /// Field of the OJP ranking (English, as in the data), empty without a score.
    pub ojp_field: String,
    /// Ranked journals of that field.
    pub ojp_n: u32,
}

/// Every stored citing work, newest first (one row per article and citing work).
pub fn citing_works(conn: &Connection) -> Result<Vec<CitingRow>> {
    let mut st = conn.prepare(
        "SELECT b.article_key, COALESCE(a.title,''), b.source, b.citing_id, b.citing_title, b.citing_date,
                b.date_precision, b.venue, b.doi, b.url, b.authors, b.countries, b.issn_list,
                b.institutions, b.author_orcids, b.topic, b.topic_field, b.topic_domain, b.work_type,
                b.language, b.oa_status, b.publisher, b.source_type, b.cited_by, b.enriched, b.source_id
         FROM backfill_citations b LEFT JOIN articles a ON a.article_key=b.article_key
         ORDER BY b.citing_date DESC, b.citing_id ASC",
    )?;
    let rows = st.query_map([], |r| {
        Ok(CitingRow {
            article_key: r.get(0)?,
            article_title: r.get(1)?,
            source: r.get(2)?,
            citing_id: r.get(3)?,
            title: r.get(4)?,
            date: r.get(5)?,
            precision: r.get(6)?,
            venue: r.get(7)?,
            doi: r.get(8)?,
            url: r.get(9)?,
            authors: r.get(10)?,
            countries: r.get(11)?,
            issn_list: r.get(12)?,
            institutions: r.get(13)?,
            author_orcids: r.get(14)?,
            topic: r.get(15)?,
            topic_field: r.get(16)?,
            topic_domain: r.get(17)?,
            work_type: r.get(18)?,
            language: r.get(19)?,
            oa_status: r.get(20)?,
            publisher: r.get(21)?,
            source_type: r.get(22)?,
            cited_by: r.get(23)?,
            enriched: r.get::<_, i64>(24)? != 0,
            self_cite: false,
            coauthor_cite: false,
            source_id: r.get(25)?,
            ojp: None,
            ojp_state: crate::ojp::State::NoJournal,
            ojp_field: String::new(),
            ojp_n: 0,
        })
    })?;
    let mut rows = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    for r in &mut rows {
        r.set_ojp(&crate::ojp::lookup(&r.source_id, &r.issn_list, &r.source_type));
    }
    Ok(rows)
}

impl CitingRow {
    /// Copies the OJP facts of the row's journal.
    pub fn set_ojp(&mut self, o: &crate::ojp::Ojp) {
        self.ojp = o.ojp;
        self.ojp_state = o.state;
        self.ojp_field = o.field.clone();
        self.ojp_n = o.n;
    }
}

/// Last snapshot taken at or before `t` (no fallback).
pub fn snapshot_at_or_before(conn: &Connection, t: DateTime<Utc>) -> Result<Option<Snapshot>> {
    Ok(conn
        .query_row(
            &format!("SELECT {SNAP_COLS} FROM snapshots WHERE fetched_at <= ?1 ORDER BY fetched_at DESC, id DESC LIMIT 1"),
            params![ts(t)],
            snap_from_row,
        )
        .optional()?)
}

/// Citations of every article in one snapshot.
pub fn article_map_at(conn: &Connection, snapshot_id: i64) -> Result<std::collections::HashMap<String, i64>> {
    let mut st = conn.prepare("SELECT article_key, citations FROM article_citations WHERE snapshot_id=?1")?;
    let rows = st.query_map(params![snapshot_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<std::collections::HashMap<_, _>>>()?)
}

/// (key, title, citations) of the latest snapshot, most cited first.
pub fn latest_article_citations(conn: &Connection) -> Result<Vec<(String, String, i64)>> {
    let Some(latest) = latest_snapshot(conn)? else {
        return Ok(Vec::new());
    };
    article_citations_in(conn, latest.id)
}

/// (key, title, citations) of one snapshot, most cited first.
pub fn article_citations_in(conn: &Connection, snapshot_id: i64) -> Result<Vec<(String, String, i64)>> {
    let mut st = conn.prepare(
        "SELECT a.article_key, a.title, c.citations
         FROM article_citations c JOIN articles a ON a.article_key=c.article_key
         WHERE c.snapshot_id=?1 ORDER BY c.citations DESC, a.title ASC",
    )?;
    let rows = st.query_map(params![snapshot_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Citing works per article dated on or after `since` (day precision; month precision when the
/// whole month lies after it).
pub fn backfill_counts_since(conn: &Connection, since: chrono::NaiveDate) -> Result<std::collections::HashMap<String, i64>> {
    let day = since.format("%Y-%m-%d").to_string();
    let month = since.format("%Y-%m").to_string();
    let mut st = conn.prepare(
        "SELECT article_key, COUNT(*) FROM backfill_citations
         WHERE (date_precision='day' AND citing_date >= ?1) OR (date_precision='month' AND citing_date > ?2)
         GROUP BY article_key",
    )?;
    let rows = st.query_map(params![day, month], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<std::collections::HashMap<_, _>>>()?)
}

/// Newest citing work dated to the day (`YYYY-MM-DD`). OpenAlex adds new papers with a lag of weeks, so
/// this is where its data really ends.
pub fn latest_citing_day(conn: &Connection) -> Result<Option<String>> {
    Ok(conn.query_row("SELECT MAX(citing_date) FROM backfill_citations WHERE date_precision='day'", [], |r| r.get(0))?)
}

/// One distinct citing work (a work that cites several articles counts once).
#[derive(Debug, Clone, PartialEq)]
pub struct DistRow {
    pub venue: String,
    pub date: String,
    pub countries: String,
    pub source: String,
}

pub fn distinct_citing_works(conn: &Connection) -> Result<Vec<DistRow>> {
    let mut st = conn.prepare(
        "SELECT MAX(venue), MIN(citing_date), MAX(countries), MIN(source) FROM backfill_citations GROUP BY citing_id",
    )?;
    let rows = st.query_map([], |r| {
        Ok(DistRow {
            venue: r.get(0)?,
            date: r.get(1)?,
            countries: r.get(2)?,
            source: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Start of a local calendar day expressed in UTC.
pub fn local_day_start_utc(day: chrono::NaiveDate) -> DateTime<Utc> {
    let fallback = Utc.from_utc_datetime(&day.and_hms_opt(0, 0, 0).unwrap_or_default());
    let Some(naive) = day.and_hms_opt(0, 0, 0) else {
        return fallback;
    };
    Local
        .from_local_datetime(&naive)
        .earliest()
        .or_else(|| Local.from_local_datetime(&naive).latest())
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or(fallback)
}


// ------------------------------------------------------------ data folder

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchOutcome {
    Unchanged,
    Copied,
    UsedExisting,
    /// The old folder had no database, so there was nothing to copy.
    EmptyStart,
}

/// Prepares `new_dir` for use. Copies the current DB there when the target has none.
pub fn switch_data_dir(old_dir: &Path, new_dir: &Path) -> Result<SwitchOutcome> {
    if old_dir == new_dir {
        return Ok(SwitchOutcome::Unchanged);
    }
    std::fs::create_dir_all(new_dir)
        .with_context(|| format!("could not create folder: {}", new_dir.display()))?;
    let new_db = db_path_in(new_dir);
    if new_db.exists() {
        return Ok(SwitchOutcome::UsedExisting);
    }
    let old_db = db_path_in(old_dir);
    if !old_db.exists() {
        return Ok(SwitchOutcome::EmptyStart);
    }
    let conn = open(&old_db)?;
    let target = new_db.to_string_lossy().to_string();
    conn.execute("VACUUM INTO ?1", params![target])
        .context("could not copy database")?;
    Ok(SwitchOutcome::Copied)
}

// ------------------------------------------------------------------- export

/// Writes the UTF-8 BOM followed by CSV rows (one row per snapshot).
pub fn write_snapshots_csv<W: Write>(conn: &Connection, mut w: W, lang: &str) -> Result<()> {
    w.write_all(&[0xEF, 0xBB, 0xBF])?;
    let mut csv = csv::Writer::from_writer(w);
    csv.write_record((0..10).map(|i| i18n::tr(lang, &format!("csv.s.{i}"))))?;
    let mut st = conn.prepare(
        "SELECT fetched_at, profile_id, name, cit_all, cit_since, h_all, h_since, i10_all, i10_since, since_year
         FROM snapshots ORDER BY fetched_at, id",
    )?;
    let mut rows = st.query([])?;
    while let Some(r) = rows.next()? {
        let mut rec: Vec<String> = Vec::with_capacity(10);
        rec.push(r.get::<_, String>(0)?);
        rec.push(r.get::<_, String>(1)?);
        rec.push(r.get::<_, String>(2)?);
        for i in 3..10 {
            rec.push(r.get::<_, i64>(i)?.to_string());
        }
        csv.write_record(&rec)?;
    }
    csv.flush()?;
    Ok(())
}

/// Writes the UTF-8 BOM followed by the long-format article history.
pub fn write_articles_csv<W: Write>(conn: &Connection, mut w: W, lang: &str) -> Result<()> {
    w.write_all(&[0xEF, 0xBB, 0xBF])?;
    let mut csv = csv::Writer::from_writer(w);
    csv.write_record((0..4).map(|i| i18n::tr(lang, &format!("csv.a.{i}"))))?;
    let mut st = conn.prepare(
        "SELECT s.fetched_at, c.article_key, a.title, c.citations
         FROM article_citations c
         JOIN snapshots s ON s.id=c.snapshot_id
         JOIN articles a ON a.article_key=c.article_key
         ORDER BY s.fetched_at, s.id, a.title",
    )?;
    let mut rows = st.query([])?;
    while let Some(r) = rows.next()? {
        csv.write_record([
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?.to_string(),
        ])?;
    }
    csv.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scholar::{Article, Stats};

    fn art(key: &str, title: &str, cites: i64) -> Article {
        Article {
            key: key.into(),
            title: title.into(),
            authors: "A".into(),
            venue: "V".into(),
            year: Some(2020),
            citations: cites,
            url: format!("https://scholar.google.com/{key}"),
            cites_ids: format!("{}", cites + 100),
        }
    }

    fn profile(articles: Vec<Article>, h: i64) -> ProfileData {
        let total: i64 = articles.iter().map(|a| a.citations).sum();
        ProfileData {
            name: "Test".into(),
            stats: Stats {
                cit_all: total,
                cit_since: total,
                h_all: h,
                h_since: h,
                i10_all: 1,
                i10_since: 1,
                since_year: 2021,
            },
            articles,
            yearly: vec![(2025, 3), (2026, total - 3)],
        }
    }

    fn mem() -> Connection {
        let conn = Connection::open_in_memory().expect("memory db");
        conn.execute_batch(SCHEMA).expect("schema");
        conn
    }

    fn t(s: &str) -> DateTime<Utc> {
        parse_ts(s).expect("valid ts")
    }

    #[test]
    fn clear_all_removes_every_record() {
        let mut conn = mem();
        insert_snapshot(&mut conn, "P", t("2026-11-01T10:00:00Z"), &profile(vec![art("k1", "One", 10)], 1))
            .expect("insert");
        log_attempt(&conn, "c", "c", 1, "ok", "m").expect("log");
        store_backfill_article(
            &mut conn,
            &mrec("k1", Some("openalex")),
            t("2026-11-01T11:00:00Z"),
            &[cite("W1", "2024-01-05", "day")],
        )
        .expect("bf");
        start_run(&conn, t("2026-11-01T11:00:00Z"), 1).expect("run");
        set_override_doi(&conn, "k1", Some("10.1234/x"), t("2026-11-01T12:00:00Z")).expect("ov");
        set_state(&conn, STATE_HEALTH_NOTIFIED, "2026-11-01T12:00:00Z").expect("state");
        clear_all(&mut conn).expect("clear");
        for table in [
            "snapshots",
            "articles",
            "article_citations",
            "yearly_citations",
            "fetch_log",
            "article_matches",
            "backfill_citations",
            "backfill_runs",
            "article_overrides",
            "app_state",
        ] {
            let n: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .expect("count");
            assert_eq!(n, 0, "{table} not empty");
        }
        assert!(latest_snapshot(&conn).expect("q").is_none());
        assert!(last_cycle_start(&conn).expect("q").is_none());
    }

    #[test]
    fn insert_and_read_back() {
        let mut conn = mem();
        let id = insert_snapshot(
            &mut conn,
            "P",
            t("2026-11-01T10:00:00Z"),
            &profile(vec![art("k1", "One", 10), art("k2", "Two", 5)], 2),
        )
        .expect("insert");
        let s = latest_snapshot(&conn).expect("q").expect("some");
        assert_eq!(s.id, id);
        assert_eq!(s.cit_all, 15);
        assert_eq!(s.fetched_at, "2026-11-01T10:00:00Z");
        assert_eq!(yearly_for(&conn, id).expect("y"), vec![(2025, 3), (2026, 12)]);
        assert_eq!(snapshot_count(&conn).expect("c"), 1);
    }

    #[test]
    fn article_upsert_updates_title_and_keeps_first_seen() {
        let mut conn = mem();
        insert_snapshot(&mut conn, "P", t("2026-11-01T10:00:00Z"), &profile(vec![art("k1", "Old", 1)], 1))
            .expect("ins1");
        insert_snapshot(&mut conn, "P", t("2026-11-02T10:00:00Z"), &profile(vec![art("k1", "New", 2)], 1))
            .expect("ins2");
        let (title, first, last): (String, String, String) = conn
            .query_row(
                "SELECT title, first_seen, last_seen FROM articles WHERE article_key='k1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .expect("row");
        assert_eq!(title, "New");
        assert_eq!(first, "2026-11-01T10:00:00Z");
        assert_eq!(last, "2026-11-02T10:00:00Z");
        let hist = article_history(&conn, "k1").expect("hist");
        assert_eq!(hist.len(), 2);
        assert_eq!(hist[1].1, 2);
    }

    #[test]
    fn deltas_use_reference_snapshots_and_fallback() {
        let mut conn = mem();
        let now = t("2026-11-22T12:00:00Z");
        let midnight = t("2026-11-22T00:00:00Z");
        assert!(deltas(&conn, midnight, now).expect("d").is_none());

        // single snapshot: everything falls back to itself
        insert_snapshot(&mut conn, "P", t("2026-10-01T10:00:00Z"), &profile(vec![art("k1", "One", 100)], 3))
            .expect("i");
        let d = deltas(&conn, midnight, now).expect("d").expect("some");
        assert_eq!(d.cit_all, Delta { today: 0, d7: 0, d30: 0 });

        insert_snapshot(&mut conn, "P", t("2026-11-10T10:00:00Z"), &profile(vec![art("k1", "One", 110)], 3))
            .expect("i");
        insert_snapshot(&mut conn, "P", t("2026-11-21T23:00:00Z"), &profile(vec![art("k1", "One", 115)], 4))
            .expect("i");
        insert_snapshot(&mut conn, "P", t("2026-11-22T09:00:00Z"), &profile(vec![art("k1", "One", 118)], 4))
            .expect("i");
        let d = deltas(&conn, midnight, now).expect("d").expect("some");
        // today: vs. 11-21 23:00 (115); 7d: vs. 11-10 (110, before 11-15); 30d: vs. 10-01 (100, before 10-23)
        assert_eq!(d.cit_all, Delta { today: 3, d7: 8, d30: 18 });
        assert_eq!(d.h_all, Delta { today: 0, d7: 1, d30: 1 });
    }

    #[test]
    fn deltas_fall_back_to_earliest_when_no_older_data() {
        let mut conn = mem();
        insert_snapshot(&mut conn, "P", t("2026-11-22T08:00:00Z"), &profile(vec![art("k1", "One", 10)], 1))
            .expect("i");
        insert_snapshot(&mut conn, "P", t("2026-11-22T11:00:00Z"), &profile(vec![art("k1", "One", 12)], 1))
            .expect("i");
        let d = deltas(&conn, t("2026-11-22T00:00:00Z"), t("2026-11-22T12:00:00Z"))
            .expect("d")
            .expect("some");
        assert_eq!(d.cit_all, Delta { today: 2, d7: 2, d30: 2 });
    }

    #[test]
    fn first_import_is_not_new_but_later_articles_are() {
        let mut conn = mem();
        let now = t("2026-11-22T12:00:00Z");
        let midnight = t("2026-11-22T00:00:00Z");
        insert_snapshot(
            &mut conn,
            "P",
            t("2026-11-20T10:00:00Z"),
            &profile(vec![art("k1", "One", 10), art("k2", "Two", 5)], 1),
        )
        .expect("i");
        let rows = articles_overview(&conn, midnight, now).expect("rows");
        assert!(rows.iter().all(|r| !r.is_new));

        insert_snapshot(
            &mut conn,
            "P",
            t("2026-11-22T10:00:00Z"),
            &profile(vec![art("k1", "One", 12), art("k2", "Two", 5), art("k3", "Three", 1)], 1),
        )
        .expect("i");
        let rows = articles_overview(&conn, midnight, now).expect("rows");
        let by = |k: &str| rows.iter().find(|r| r.key == k).expect("row").clone();
        assert!(by("k3").is_new);
        assert!(!by("k1").is_new);
        assert_eq!(by("k1").today, 2);
        assert_eq!(by("k1").d7, 2);
        assert_eq!(by("k3").today, 1);
        assert_eq!(by("k2").today, 0);
        assert_eq!(rows[0].key, "k1"); // sorted by citations desc
    }

    #[test]
    fn new_flag_expires_after_seven_days() {
        let mut conn = mem();
        insert_snapshot(&mut conn, "P", t("2026-11-01T10:00:00Z"), &profile(vec![art("k1", "One", 1)], 1))
            .expect("i");
        insert_snapshot(
            &mut conn,
            "P",
            t("2026-11-05T10:00:00Z"),
            &profile(vec![art("k1", "One", 1), art("k2", "Two", 1)], 1),
        )
        .expect("i");
        let rows = articles_overview(&conn, t("2026-11-22T00:00:00Z"), t("2026-11-22T12:00:00Z")).expect("r");
        assert!(rows.iter().all(|r| !r.is_new));
    }

    #[test]
    fn log_and_last_cycle() {
        let conn = mem();
        assert!(last_cycle_start(&conn).expect("l").is_none());
        log_attempt(&conn, "2026-11-22T10:00:00Z", "2026-11-22T10:00:01Z", 1, "network", "x").expect("log");
        log_attempt(&conn, "2026-11-22T10:00:00Z", "2026-11-22T10:05:01Z", 2, "ok", "y").expect("log");
        log_attempt(&conn, "2026-11-22T13:00:00Z", "2026-11-22T13:00:01Z", 1, "ok", "z").expect("log");
        assert_eq!(
            last_cycle_start(&conn).expect("l"),
            Some(t("2026-11-22T13:00:00Z"))
        );
        let rows = recent_log(&conn, 200).expect("rows");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].message, "z");
    }

    #[test]
    fn change_summary_and_notification() {
        let mut conn = mem();
        let first = insert_snapshot(
            &mut conn,
            "P",
            t("2026-11-20T10:00:00Z"),
            &profile(vec![art("k1", "One", 10), art("k2", "Two", 5)], 1),
        )
        .expect("i");
        assert!(change_since_previous(&conn, first).expect("c").is_none());

        let second = insert_snapshot(
            &mut conn,
            "P",
            t("2026-11-20T13:00:00Z"),
            &profile(vec![art("k1", "One", 11), art("k2", "Two", 6), art("k3", "Three", 0)], 1),
        )
        .expect("i");
        let c = change_since_previous(&conn, second).expect("c").expect("some");
        assert_eq!(c.total, 17);
        assert_eq!(c.total_delta, 2);
        assert_eq!(c.new_articles, vec!["Three".to_string()]);
        let (title, body) = notification_text(&c, "tr").expect("text");
        assert_eq!(title, "+2 yeni atıf (toplam 17)");
        assert!(body.contains("One +1"));
        assert!(body.contains("Two +1"));
        assert!(body.contains("Yeni makale: Three"));
        let (title, body) = notification_text(&c, "en").expect("text");
        assert_eq!(title, "+2 new citations (total 17)");
        assert!(body.contains("New article: Three"));

        let third = insert_snapshot(
            &mut conn,
            "P",
            t("2026-11-20T16:00:00Z"),
            &profile(vec![art("k1", "One", 11), art("k2", "Two", 6), art("k3", "Three", 0)], 1),
        )
        .expect("i");
        let c = change_since_previous(&conn, third).expect("c").expect("some");
        assert!(notification_text(&c, "en").is_none());
    }

    #[test]
    fn notification_limits_lines_and_shortens_titles() {
        let c = Change {
            total: 100,
            total_delta: 4,
            increases: (1..=5)
                .map(|i| (format!("Title number {i} {}", "x".repeat(60)), 1))
                .collect(),
            decreases: vec![],
            new_articles: vec![],
        };
        let (_, body) = notification_text(&c, "en").expect("text");
        assert_eq!(body.lines().count(), 3);
        assert!(body.lines().all(|l| l.contains('…') && l.ends_with("+1")));
    }

    #[test]
    fn extract_changes_in_both_directions_skips_the_first_point() {
        let h = |v: &[(&str, i64)]| v.iter().map(|(a, b)| (a.to_string(), *b)).collect::<Vec<_>>();
        let c = extract_changes(&h(&[("t1", 5), ("t2", 5), ("t3", 7), ("t4", 6), ("t5", 6)]));
        assert_eq!(c.len(), 2, "unchanged points make no change; the first point has nothing before it");
        assert_eq!((c[0].at.as_str(), c[0].old, c[0].new, c[0].delta), ("t4", 7, 6, -1), "newest first");
        assert_eq!((c[1].at.as_str(), c[1].old, c[1].new, c[1].delta), ("t3", 5, 7, 2));
        assert!(extract_changes(&h(&[("t1", 9)])).is_empty());
        assert!(extract_changes(&[]).is_empty());
        // a citation removed and later returning shows as a down and an up
        let c = extract_changes(&h(&[("a", 3), ("b", 2), ("c", 3)]));
        assert_eq!(c.iter().map(|x| x.delta).collect::<Vec<_>>(), vec![1, -1]);
    }

    #[test]
    fn change_summary_counts_ups_downs_and_net_inside_the_window() {
        let ch = |at: &str, old: i64, new: i64| CitationChange { at: at.into(), old, new, delta: new - old };
        let list = vec![
            ch("2026-11-20T10:00:00Z", 5, 7),
            ch("2026-11-10T10:00:00Z", 7, 6),
            ch("2026-11-05T10:00:00Z", 6, 9),
            ch("2026-09-01T10:00:00Z", 1, 0), // outside the window
        ];
        let s = summarize_changes(&list, "2026-10-23T00:00:00Z");
        assert_eq!((s.up, s.down, s.net), (5, 1, 4));
        let all = summarize_changes(&list, "");
        assert_eq!((all.up, all.down, all.net), (5, 2, 3));
        assert_eq!(summarize_changes(&[], ""), ChangeSummary::default());
    }

    #[test]
    fn scholar_changes_from_snapshots_ignore_the_first_snapshot_and_new_articles() {
        let mut conn = mem();
        insert_snapshot(&mut conn, "P", t("2026-11-01T10:00:00Z"), &profile(vec![art("k1", "One", 10), art("k2", "Two", 5)], 1)).expect("s1");
        insert_snapshot(&mut conn, "P", t("2026-11-02T10:00:00Z"), &profile(vec![art("k1", "One", 12), art("k2", "Two", 4)], 1)).expect("second");
        // k3 shows up for the first time with 3 citations: that is not an increase
        insert_snapshot(&mut conn, "P", t("2026-11-03T10:00:00Z"), &profile(vec![art("k1", "One", 12), art("k2", "Two", 4), art("k3", "Three", 3)], 1)).expect("s3");
        insert_snapshot(&mut conn, "P", t("2026-11-04T10:00:00Z"), &profile(vec![art("k1", "One", 11), art("k2", "Two", 4), art("k3", "Three", 4)], 1)).expect("s4");
        let ch = scholar_changes(&conn).expect("changes");
        let view: Vec<(&str, i64)> = ch.iter().map(|c| (c.article_key.as_str(), c.delta)).collect();
        assert_eq!(view, vec![("k1", -1), ("k3", 1), ("k1", 2), ("k2", -1)], "newest first, both directions");
        assert!(ch.iter().all(|c| c.cites_ids.parse::<i64>().is_ok()), "ids travel with the change");
        let one = article_changes(&conn, "k1", t("2026-11-10T10:00:00Z")).expect("one");
        assert_eq!(one.items.len(), 2);
        assert_eq!((one.last30.up, one.last30.down, one.last30.net), (2, 1, 1));
    }

    #[test]
    fn decreases_are_recorded_but_never_notified_alone() {
        let mut conn = mem();
        insert_snapshot(&mut conn, "P", t("2026-11-20T10:00:00Z"), &profile(vec![art("k1", "One", 10), art("k2", "Two", 5)], 1)).expect("s1");
        let down = insert_snapshot(&mut conn, "P", t("2026-11-20T13:00:00Z"), &profile(vec![art("k1", "One", 8), art("k2", "Two", 5)], 1)).expect("second");
        let c = change_since_previous(&conn, down).expect("c").expect("some");
        assert_eq!(c.total_delta, -2);
        assert_eq!(c.decreases, vec![("One".to_string(), 2)]);
        assert!(c.increases.is_empty());
        assert!(notification_text(&c, "tr").is_none() && notification_text(&c, "en").is_none());
        // one article up and another down in the same snapshot: the net, and only the risen article
        let both = insert_snapshot(&mut conn, "P", t("2026-11-20T16:00:00Z"), &profile(vec![art("k1", "One", 7), art("k2", "Two", 9)], 1)).expect("s3");
        let c = change_since_previous(&conn, both).expect("c").expect("some");
        assert_eq!((c.total_delta, c.increases.len(), c.decreases.len()), (3, 1, 1));
        let (title, body) = notification_text(&c, "en").expect("text");
        assert_eq!(title, "+3 new citations (total 16)");
        assert!(body.contains("Two +4") && !body.contains("One"), "{body}");
    }

    #[test]
    fn article_rows_carry_the_doi_and_scholar_ids() {
        let mut conn = mem();
        insert_snapshot(&mut conn, "P", t("2026-11-01T10:00:00Z"), &profile(vec![art("k1", "One", 10), art("k2", "Two", 5), art("k3", "Three", 1)], 1)).expect("s");
        // k1: matched work with a DOI; k2: manual DOI wins; k3: nothing known
        store_backfill_article(
            &mut conn,
            &MatchRecord { article_key: "k1".into(), source: Some("openalex".into()), external_id: Some("W1".into()), doi: Some("10.1/auto".into()), ..MatchRecord::default() },
            t("2026-11-02T10:00:00Z"),
            &[],
        )
        .expect("m1");
        store_backfill_article(
            &mut conn,
            &MatchRecord { article_key: "k2".into(), source: Some("openalex".into()), external_id: Some("W2".into()), doi: Some("10.1/auto2".into()), ..MatchRecord::default() },
            t("2026-11-02T10:00:00Z"),
            &[],
        )
        .expect("m2");
        set_override_doi(&conn, "k2", Some("10.9/manual"), t("2026-11-03T10:00:00Z")).expect("ov");
        let rows = articles_overview(&conn, t("2026-11-05T00:00:00Z"), t("2026-11-05T12:00:00Z")).expect("rows");
        let by = |k: &str| rows.iter().find(|r| r.key == k).expect("row");
        assert_eq!(by("k1").doi.as_deref(), Some("10.1/auto"));
        assert_eq!(by("k2").doi.as_deref(), Some("10.9/manual"));
        assert_eq!(by("k3").doi, None);
        assert_eq!(by("k1").cites_ids, "110");
    }

    #[test]
    fn citing_rows_keep_the_issn_list() {
        let mut conn = mem();
        insert_snapshot(&mut conn, "P", t("2026-11-01T10:00:00Z"), &profile(vec![art("k1", "One", 10)], 1)).expect("s");
        let with_issn = CitingRecord { issn_list: "15698432,1872826X".into(), ..cite("W1", "2026-10-01", "day") };
        store_backfill_article(&mut conn, &mrec("k1", Some("openalex")), t("2026-11-02T10:00:00Z"), &[with_issn, cite("W2", "2026-10-02", "day")]).expect("bf");
        let rows = citing_works(&conn).expect("c");
        assert_eq!(rows.iter().find(|c| c.citing_id == "W1").expect("w1").issn_list, "15698432,1872826X");
        assert_eq!(rows.iter().find(|c| c.citing_id == "W2").expect("w2").issn_list, "");
    }

    #[test]
    fn data_dir_switch_copies_or_reuses() {
        let base = std::env::temp_dir().join(format!("scholartakip_test_{}", std::process::id()));
        let old = base.join("old");
        let new = base.join("new");
        let existing = base.join("existing");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&old).expect("mk");
        {
            let mut conn = open(&db_path_in(&old)).expect("open");
            insert_snapshot(&mut conn, "P", t("2026-11-01T10:00:00Z"), &profile(vec![art("k1", "One", 3)], 1))
                .expect("ins");
        }
        assert_eq!(switch_data_dir(&old, &old).expect("s"), SwitchOutcome::Unchanged);
        assert_eq!(switch_data_dir(&old, &new).expect("s"), SwitchOutcome::Copied);
        let conn = open(&db_path_in(&new)).expect("open new");
        assert_eq!(snapshot_count(&conn).expect("c"), 1);
        drop(conn);

        // existing DB in target is kept untouched
        std::fs::create_dir_all(&existing).expect("mk");
        drop(open(&db_path_in(&existing)).expect("open"));
        assert_eq!(switch_data_dir(&old, &existing).expect("s"), SwitchOutcome::UsedExisting);
        let conn = open(&db_path_in(&existing)).expect("open");
        assert_eq!(snapshot_count(&conn).expect("c"), 0);
        drop(conn);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn csv_exports_have_bom_and_rows() {
        let mut conn = mem();
        insert_snapshot(
            &mut conn,
            "P",
            t("2026-11-01T10:00:00Z"),
            &profile(vec![art("k1", "Başlık, virgüllü", 3)], 1),
        )
        .expect("ins");
        let mut buf = Vec::new();
        write_snapshots_csv(&conn, &mut buf, "en").expect("csv");
        assert_eq!(&buf[..3], &[0xEF, 0xBB, 0xBF]);
        let text = String::from_utf8(buf[3..].to_vec()).expect("utf8");
        assert_eq!(text.lines().count(), 2);
        assert!(text.starts_with("Fetched at,Profile ID"));

        let mut buf_tr = Vec::new();
        write_snapshots_csv(&conn, &mut buf_tr, "tr").expect("csv");
        let text_tr = String::from_utf8(buf_tr[3..].to_vec()).expect("utf8");
        assert!(text_tr.starts_with("Zaman,Profil kimliği"));

        let mut buf = Vec::new();
        write_articles_csv(&conn, &mut buf, "tr").expect("csv");
        assert_eq!(&buf[..3], &[0xEF, 0xBB, 0xBF]);
        let text = String::from_utf8(buf[3..].to_vec()).expect("utf8");
        assert!(text.starts_with("Zaman,Makale anahtarı,Başlık,Atıf"));
        assert!(text.contains("\"Başlık, virgüllü\""));
        assert!(text.contains(",3"));
    }

    #[test]
    fn local_midnight_is_not_in_the_future_and_within_a_day() {
        let now = Local::now();
        let m = local_midnight_utc(now);
        let n = now.with_timezone(&Utc);
        assert!(m <= n);
        assert!(n - m < Duration::hours(25));
    }

    #[test]
    fn wal_mode_is_enabled() {
        let dir = std::env::temp_dir().join(format!("scholartakip_wal_{}", std::process::id()));
        let conn = open(&db_path_in(&dir)).expect("open");
        let mode: String = conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .expect("pragma");
        assert_eq!(mode.to_lowercase(), "wal");
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }


    // ------------------------------------------------------ backfill tests

    fn mrec(key: &str, source: Option<&str>) -> MatchRecord {
        MatchRecord {
            article_key: key.into(),
            source: source.map(str::to_string),
            external_id: source.map(|_| "W1".to_string()),
            doi: Some("10.1/x".into()),
            score: source.map(|_| 0.97),
            match_method: source.map(|_| "orcid".to_string()),
            openalex_id: Some("W1".into()),
            openalex_count: source.map(|_| 3),
        }
    }

    fn cite(id: &str, date: &str, precision: &str) -> CitingRecord {
        CitingRecord {
            citing_id: id.into(),
            citing_title: format!("Citing {id}"),
            citing_date: date.into(),
            date_precision: precision.into(),
            ..CitingRecord::default()
        }
    }

    fn count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .expect("count")
    }

    #[test]
    fn backfill_store_is_idempotent_and_replaces_per_article() {
        let mut conn = mem();
        let rec = mrec("k1", Some("openalex"));
        let cites = vec![
            cite("W10", "2024-01-05", "day"),
            cite("W11", "2024-01-20", "day"),
            cite("W12", "2024-03", "month"),
        ];
        store_backfill_article(&mut conn, &rec, t("2026-11-22T10:00:00Z"), &cites).expect("1");
        store_backfill_article(&mut conn, &rec, t("2026-11-22T11:00:00Z"), &cites).expect("2");
        assert_eq!(count(&conn, "backfill_citations"), 3);
        assert_eq!(count(&conn, "article_matches"), 1);

        // a re-run that returns different citing works replaces the old rows
        let mut rec2 = mrec("k1", Some("other"));
        rec2.external_id = Some("abc".into());
        store_backfill_article(&mut conn, &rec2, t("2026-11-22T12:00:00Z"), &[cite("p1", "2025", "year")]).expect("3");
        assert_eq!(count(&conn, "backfill_citations"), 1);
        let (src, at): (String, String) = conn
            .query_row("SELECT source, matched_at FROM article_matches WHERE article_key='k1'", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .expect("row");
        assert_eq!(src, "other");
        assert_eq!(at, "2026-11-22T12:00:00Z");

        // unmatched: NULL source, no citing rows
        store_backfill_article(&mut conn, &mrec("k2", None), t("2026-11-22T12:00:00Z"), &[cite("zzz", "2020-01-01", "day")])
            .expect("4");
        assert_eq!(count(&conn, "backfill_citations"), 1);
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM article_matches WHERE source IS NULL", [], |r| r.get(0))
            .expect("n");
        assert_eq!(n, 1);
    }

    #[test]
    fn legacy_source_matches_are_reset_once_and_overrides_are_kept() {
        let mut conn = mem();
        let legacy = |k: &str| MatchRecord { source: Some("semanticscholar".into()), ..mrec(k, Some("openalex")) };
        store_backfill_article(&mut conn, &mrec("a", Some("openalex")), t("2026-11-01T10:00:00Z"), &[cite("W1", "2024-01-05", "day")]).expect("a");
        store_backfill_article(&mut conn, &legacy("b"), t("2026-11-01T10:00:00Z"), &[cite("p1", "2024-02-05", "day"), cite("p2", "2024-03", "month")]).expect("b");
        store_backfill_article(&mut conn, &mrec("c", None), t("2026-11-01T10:00:00Z"), &[]).expect("c");
        set_override_doi(&conn, "b", Some("10.1/manual"), t("2026-11-01T10:00:00Z")).expect("ov");

        assert_eq!(drop_legacy_source_matches(&mut conn).expect("run"), 1);
        // the openalex match, its citing rows and the unmatched row stay
        assert_eq!(count(&conn, "article_matches"), 2);
        assert_eq!(count(&conn, "backfill_citations"), 1);
        let gone: i64 = conn
            .query_row("SELECT COUNT(*) FROM article_matches WHERE article_key='b'", [], |r| r.get(0))
            .expect("n");
        assert_eq!(gone, 0);
        // the manual DOI survives, so the next run matches the article by it
        assert_eq!(all_overrides(&conn).expect("ov").get("b").and_then(|o| o.doi.clone()).as_deref(), Some("10.1/manual"));
        assert!(get_state(&conn, STATE_LEGACY_SOURCE_DROPPED).expect("state").is_some());

        // runs once: a later match from that source name is left alone
        store_backfill_article(&mut conn, &legacy("d"), t("2026-11-02T10:00:00Z"), &[cite("p3", "2024-05-05", "day")]).expect("d");
        assert_eq!(drop_legacy_source_matches(&mut conn).expect("again"), 0);
        assert_eq!(count(&conn, "article_matches"), 3);
    }

    #[test]
    fn history_aggregates_day_month_year_without_double_counting() {
        let mut conn = mem();
        // the same citing id under two different articles counts once per article
        store_backfill_article(
            &mut conn,
            &mrec("k1", Some("openalex")),
            t("2026-11-22T10:00:00Z"),
            &[
                cite("W1", "2024-01-05", "day"),
                cite("W1", "2024-01-05", "day"), // duplicate within one article collapses (PK)
                cite("W2", "2024-01-05", "day"),
                cite("W3", "2024-01-31", "day"),
                cite("W4", "2024-02", "month"),
                cite("W5", "2023", "year"),
            ],
        )
        .expect("k1");
        store_backfill_article(
            &mut conn,
            &mrec("k2", Some("openalex")),
            t("2026-11-22T10:00:00Z"),
            &[cite("W1", "2024-01-05", "day"), cite("W6", "2025-06-10", "day")],
        )
        .expect("k2");

        let day = history_series(&conn, "day").expect("day");
        assert_eq!(
            day.points,
            vec![
                ("2024-01-05".to_string(), 3), // W1 (k1) + W2 (k1) + W1 (k2)
                ("2024-01-31".to_string(), 1),
                ("2025-06-10".to_string(), 1),
            ]
        );
        assert_eq!(day.total, 7);
        assert_eq!(day.excluded, 2); // month- and year-precision records

        let month = history_series(&conn, "month").expect("month");
        assert_eq!(
            month.points,
            vec![
                ("2024-01".to_string(), 4),
                ("2024-02".to_string(), 1),
                ("2025-06".to_string(), 1),
            ]
        );
        assert_eq!(month.excluded, 1);

        let year = history_series(&conn, "year").expect("year");
        assert_eq!(
            year.points,
            vec![("2023".to_string(), 1), ("2024".to_string(), 5), ("2025".to_string(), 1)]
        );
        assert_eq!(year.excluded, 0);

        let a = article_backfill(&conn, "k1").expect("a");
        assert_eq!(a.monthly, vec![("2024-01".to_string(), 3), ("2024-02".to_string(), 1)]);
        assert_eq!(a.year_only, 1);
        assert_eq!(a.total, 5);
        let m = a.match_info.expect("m");
        assert_eq!(m.openalex_count, Some(3));
        assert_eq!(m.match_method.as_deref(), Some("orcid"));
        assert_eq!(
            matched_openalex_ids(&conn).expect("ids"),
            std::collections::HashSet::from(["W1".to_string()])
        );
        assert!(article_backfill(&conn, "nope").expect("none").match_info.is_none());
    }

    #[test]
    fn clear_backfill_empties_only_backfill_tables() {
        let mut conn = mem();
        insert_snapshot(&mut conn, "P", t("2026-11-01T10:00:00Z"), &profile(vec![art("k1", "One", 10)], 1))
            .expect("insert");
        store_backfill_article(
            &mut conn,
            &mrec("k1", Some("openalex")),
            t("2026-11-22T10:00:00Z"),
            &[cite("W1", "2024-01-05", "day")],
        )
        .expect("bf");
        let run = start_run(&conn, t("2026-11-22T10:00:00Z"), 1).expect("run");
        finish_run(&conn, run, t("2026-11-22T10:01:00Z"), "ok", (1, 1), "m").expect("fin");
        clear_backfill(&mut conn).expect("clear");
        for table in ["article_matches", "backfill_citations", "backfill_runs"] {
            assert_eq!(count(&conn, table), 0, "{table}");
        }
        assert_eq!(count(&conn, "snapshots"), 1);
        assert_eq!(count(&conn, "articles"), 1);
    }

    #[test]
    fn migrate_adds_the_citing_columns_to_an_old_table_once() {
        let conn = Connection::open_in_memory().expect("db");
        conn.execute_batch(
            "CREATE TABLE backfill_citations(article_key TEXT NOT NULL, source TEXT NOT NULL, citing_id TEXT NOT NULL,
               citing_title TEXT NOT NULL DEFAULT '', citing_date TEXT NOT NULL, date_precision TEXT NOT NULL,
               PRIMARY KEY(article_key, citing_id));
             INSERT INTO backfill_citations VALUES('k','openalex','W1','Old title','2020-01-01','day');",
        )
        .expect("old table");
        migrate(&conn).expect("first");
        migrate(&conn).expect("second (idempotent)");
        let (venue, countries): (String, String) = conn
            .query_row("SELECT venue, countries FROM backfill_citations WHERE citing_id='W1'", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .expect("old row still readable");
        assert_eq!((venue.as_str(), countries.as_str()), ("", ""));
        let cols: i64 = conn
            .query_row("SELECT COUNT(*) FROM pragma_table_info('backfill_citations')", [], |r| r.get(0))
            .expect("cols");
        assert_eq!(cols, 12 + 14, "the 1.2.0 table plus the 1.3.0 columns and the 1.4.0 source_id");
        let issn: String = conn
            .query_row("SELECT issn_list FROM backfill_citations WHERE citing_id='W1'", [], |r| r.get(0))
            .expect("issn column");
        assert_eq!(issn, "");
    }

    fn rich(id: &str, date: &str, venue: &str) -> CitingRecord {
        CitingRecord { venue: venue.into(), countries: "TR,US".into(), doi: format!("10.5555/{id}"), ..cite(id, date, "day") }
    }

    #[test]
    fn citing_details_round_trip() {
        let mut conn = mem();
        insert_snapshot(&mut conn, "P", t("2026-11-01T10:00:00Z"), &profile(vec![art("k1", "One", 10)], 1)).expect("s");
        store_backfill_article(&mut conn, &mrec("k1", Some("openalex")), t("2026-11-02T10:00:00Z"), &[rich("W1", "2024-01-05", "Venue A")])
            .expect("bf");
        let rows = citing_works(&conn).expect("rows");
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].venue.as_str(), rows[0].countries.as_str(), rows[0].article_title.as_str()), ("Venue A", "TR,US", "One"));
        assert_eq!(rows[0].doi, "10.5555/W1");
    }

    #[test]
    fn only_works_stored_after_a_first_result_are_new() {
        let mut conn = mem();
        insert_snapshot(&mut conn, "P", t("2026-11-01T10:00:00Z"), &profile(vec![art("k1", "One", 10)], 1)).expect("s");
        let first = store_backfill_article(
            &mut conn,
            &mrec("k1", Some("openalex")),
            t("2026-11-02T10:00:00Z"),
            &[rich("W1", "2024-01-05", "V"), rich("W2", "2024-02-05", "V")],
        )
        .expect("first");
        assert!(first.is_empty(), "the first run reports nothing as new");
        // a full re-run with one more work
        let again = store_backfill_article(
            &mut conn,
            &mrec("k1", Some("openalex")),
            t("2026-11-03T10:00:00Z"),
            &[rich("W1", "2024-01-05", "V"), rich("W2", "2024-02-05", "V"), rich("W3", "2025-03-01", "V")],
        )
        .expect("again");
        assert_eq!(again.iter().map(|c| c.citing_id.as_str()).collect::<Vec<_>>(), vec!["W3"]);
        // the daily refresh finds one more and keeps the match row
        let fresh = refresh_citing(
            &mut conn,
            "k1",
            "openalex",
            &[rich("W1", "2024-01-05", "V"), rich("W2", "2024-02-05", "V"), rich("W3", "2025-03-01", "V"), rich("W4", "2025-04-01", "V")],
        )
        .expect("refresh");
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].citing_id, "W4");
        assert_eq!(count(&conn, "backfill_citations"), 4);
        let m = article_backfill(&conn, "k1").expect("ab").match_info.expect("match kept");
        assert_eq!(m.openalex_count, Some(4));
        assert_eq!(m.source.as_deref(), Some("openalex"));
        // nothing new on an identical refresh
        assert!(refresh_citing(&mut conn, "k1", "openalex", &[rich("W1", "2024-01-05", "V")]).expect("r").is_empty());
        assert_eq!(count(&conn, "backfill_citations"), 1);
        // switching source (first result from the other source): not reported as new
        let switched = store_backfill_article(
            &mut conn,
            &MatchRecord { source: Some("other".into()), ..mrec("k1", Some("openalex")) },
            t("2026-11-04T10:00:00Z"),
            &[rich("abc", "2024-01-05", "V")],
        )
        .expect("switch");
        assert!(switched.is_empty());
    }

    #[test]
    fn refresh_targets_are_the_matched_articles() {
        let mut conn = mem();
        insert_snapshot(
            &mut conn,
            "P",
            t("2026-11-01T10:00:00Z"),
            &profile(vec![art("k1", "One", 10), art("k2", "Two", 5), art("k3", "Three", 1)], 1),
        )
        .expect("s");
        store_backfill_article(&mut conn, &mrec("k1", Some("openalex")), t("2026-11-02T10:00:00Z"), &[]).expect("m1");
        store_backfill_article(&mut conn, &mrec("k2", None), t("2026-11-02T10:00:00Z"), &[]).expect("unmatched");
        let targets = refresh_targets(&conn).expect("targets");
        assert_eq!(targets.len(), 1);
        assert_eq!((targets[0].key.as_str(), targets[0].source.as_str(), targets[0].external_id.as_str()), ("k1", "openalex", "W1"));
        assert!(has_refresh_targets(&conn).expect("has"));
    }

    #[test]
    fn clear_backfill_also_forgets_the_refresh_time() {
        let mut conn = mem();
        set_state(&conn, STATE_REFRESH, "2026-11-22T10:00:00Z").expect("st");
        set_state(&conn, STATE_GOAL, "500").expect("goal");
        clear_backfill(&mut conn).expect("clear");
        assert!(get_state(&conn, STATE_REFRESH).expect("g").is_none());
        // the user's goal is not backfill data
        assert_eq!(get_state(&conn, STATE_GOAL).expect("g").as_deref(), Some("500"));
        clear_all(&mut conn).expect("all");
        assert!(get_state(&conn, STATE_GOAL).expect("g").is_none());
    }

    #[test]
    fn overrides_roundtrip_and_survive_clear_backfill_but_not_clear_all() {
        let mut conn = mem();
        let now = t("2026-11-22T10:00:00Z");
        assert!(get_override(&conn, "k1").expect("g").is_none());
        set_override_doi(&conn, "k1", Some("10.1234/x"), now).expect("doi");
        add_rejected(&conn, "k1", &["W1".into(), "abc".into(), "W1".into(), String::new()], now).expect("rej");
        add_rejected(&conn, "k2", &["W9".into()], now).expect("rej2");
        let o = get_override(&conn, "k1").expect("g").expect("some");
        assert_eq!(o.doi.as_deref(), Some("10.1234/x"));
        assert_eq!(o.rejected, vec!["W1".to_string(), "abc".to_string()]);
        assert_eq!(all_overrides(&conn).expect("all").len(), 2);

        // replacing the DOI keeps the rejected list; removing both rows empties the override
        set_override_doi(&conn, "k1", Some("10.1234/y"), now).expect("doi2");
        let o = get_override(&conn, "k1").expect("g").expect("some");
        assert_eq!((o.doi.as_deref(), o.rejected.len()), (Some("10.1234/y"), 2));
        set_override_doi(&conn, "k2", None, now).expect("clear doi");
        assert_eq!(get_override(&conn, "k2").expect("g").expect("some").rejected, vec!["W9".to_string()]);

        // the info the UI gets
        let info = article_backfill(&conn, "k1").expect("ab").override_info.expect("info");
        assert_eq!((info.doi.as_deref(), info.rejected), (Some("10.1234/y"), 2));
        assert!(article_backfill(&conn, "zzz").expect("ab").override_info.is_none());

        // clear_backfill keeps the overrides, clear_all removes them
        clear_backfill(&mut conn).expect("cb");
        assert_eq!(count(&conn, "article_overrides"), 2);
        reset_override(&conn, "k2").expect("reset");
        assert!(get_override(&conn, "k2").expect("g").is_none());
        clear_all(&mut conn).expect("ca");
        assert_eq!(count(&conn, "article_overrides"), 0);
    }

    #[test]
    fn single_article_target_and_other_matched_ids() {
        let mut conn = mem();
        insert_snapshot(
            &mut conn,
            "P",
            t("2026-11-01T10:00:00Z"),
            &profile(vec![art("k1", "One", 10), art("k2", "Two", 5)], 1),
        )
        .expect("insert");
        assert_eq!(backfill_target_for(&conn, "k2").expect("t").expect("some").title, "Two");
        assert!(backfill_target_for(&conn, "nope").expect("t").is_none());
        let mut a = mrec("k1", Some("openalex"));
        a.openalex_id = Some("W10".into());
        let mut b = mrec("k2", Some("openalex"));
        b.openalex_id = Some("W20".into());
        store_backfill_article(&mut conn, &a, t("2026-11-22T10:00:00Z"), &[]).expect("a");
        store_backfill_article(&mut conn, &b, t("2026-11-22T10:00:00Z"), &[]).expect("b");
        assert_eq!(
            matched_openalex_ids_except(&conn, "k1").expect("ids"),
            std::collections::HashSet::from(["W20".to_string()])
        );
    }

    #[test]
    fn app_state_and_fetch_times() {
        let conn = mem();
        assert_eq!(get_state(&conn, "x").expect("g"), None);
        set_state(&conn, "x", "1").expect("s");
        set_state(&conn, "x", "2").expect("second");
        assert_eq!(get_state(&conn, "x").expect("g").as_deref(), Some("2"));

        assert_eq!(fetch_times(&conn).expect("ft"), FetchTimes::default());
        log_attempt(&conn, "c1", "2026-11-20T10:00:00Z", 1, "network", "x").expect("l");
        log_attempt(&conn, "c1", "2026-11-20T10:05:00Z", 2, "ok", "x").expect("l");
        log_attempt(&conn, "c2", "2026-11-22T10:00:00Z", 1, "blocked", "x").expect("l");
        let ft = fetch_times(&conn).expect("ft");
        assert_eq!(ft.first_attempt, Some(t("2026-11-20T10:00:00Z")));
        assert_eq!(ft.last_ok, Some(t("2026-11-20T10:05:00Z")));
        assert_eq!(ft.last_attempt, Some(t("2026-11-22T10:00:00Z")));
    }

    #[test]
    fn backfill_targets_summary_and_runs() {
        let mut conn = mem();
        assert!(backfill_targets(&conn, false).expect("t").is_empty());
        insert_snapshot(
            &mut conn,
            "P",
            t("2026-11-01T10:00:00Z"),
            &profile(vec![art("k1", "One", 10), art("k2", "Two", 5), art("k3", "Three", 1)], 1),
        )
        .expect("insert");
        let all = backfill_targets(&conn, true).expect("t");
        assert_eq!(all.iter().map(|x| x.key.as_str()).collect::<Vec<_>>(), ["k1", "k2", "k3"]);

        store_backfill_article(&mut conn, &mrec("k1", Some("openalex")), t("2026-11-22T10:00:00Z"), &[cite("W1", "2024-01-05", "day")])
            .expect("k1");
        store_backfill_article(&mut conn, &mrec("k2", None), t("2026-11-22T10:00:00Z"), &[]).expect("k2");
        // matched and unmatched articles are both skipped in "missing" mode, but not in "all" mode
        let missing = backfill_targets(&conn, true).expect("t");
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].key, "k3");
        assert_eq!(backfill_targets(&conn, false).expect("t").len(), 3);

        let s = backfill_summary(&conn).expect("s");
        assert_eq!((s.total_articles, s.matched, s.unmatched, s.pending, s.records), (3, 1, 1, 1, 1));
        assert!(s.last_run.is_none());

        let run = start_run(&conn, t("2026-11-22T10:00:00Z"), 3).expect("run");
        assert_eq!(mark_stale_runs(&conn, t("2026-11-22T10:05:00Z"), "stale").expect("stale"), 1);
        let s = backfill_summary(&conn).expect("s");
        let last = s.last_run.expect("run");
        assert_eq!((last.id, last.status.as_str(), last.message.as_str()), (run, "interrupted", "stale"));
        let run2 = start_run(&conn, t("2026-11-22T11:00:00Z"), 1).expect("run2");
        finish_run(&conn, run2, t("2026-11-22T11:01:00Z"), "partial", (1, 7), "done").expect("fin");
        let last = backfill_summary(&conn).expect("s").last_run.expect("run");
        assert_eq!((last.status.as_str(), last.matched, last.records, last.total), ("partial", 1, 7, 1));
    }

    #[test]
    fn open_adds_backfill_tables_to_an_existing_database() {
        let dir = std::env::temp_dir().join(format!("scholartakip_mig_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mk");
        {
            // a version 1.0.0 database has none of the backfill tables
            let conn = open(&db_path_in(&dir)).expect("first");
            conn.execute_batch(
                "DROP TABLE article_matches; DROP TABLE backfill_citations; DROP TABLE backfill_runs;
                 DROP TABLE article_overrides; DROP TABLE app_state;",
            )
            .expect("old");
        }
        let conn = open(&db_path_in(&dir)).expect("open");
        assert_eq!(count(&conn, "article_matches"), 0);
        assert_eq!(count(&conn, "backfill_citations"), 0);
        assert_eq!(count(&conn, "backfill_runs"), 0);
        assert_eq!(count(&conn, "article_overrides"), 0);
        assert_eq!(count(&conn, "app_state"), 0);
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Developer helper: `DEMO_DIR=... cargo test seed_demo -- --ignored` fills a demo database.
    #[test]
    #[ignore]
    fn seed_demo() {
        let Ok(dir) = std::env::var("DEMO_DIR") else { return };
        let mut conn = open(&db_path_in(Path::new(&dir))).expect("open");
        let now = Utc::now();
        for i in 0..40i64 {
            let at = now - Duration::days(40 - i) + Duration::hours(1);
            let mut arts = vec![art("k1", "Sample Paper A", 40 + i / 2)];
            arts.push(art("k2", "Sample Paper B", 10 + i));
            if i > 30 {
                arts.push(art("k3", "Sample Paper C", i - 30));
            }
            insert_snapshot(&mut conn, "P", at, &profile(arts, 4)).expect("ins");
        }
        let c = ts(now - Duration::minutes(10));
        log_attempt(&conn, &c, &c, 1, "network", "Network error: demo").expect("log");
        log_attempt(&conn, &c, &c, 2, "ok", "Total citations: 135, 11 articles").expect("log");

        // historical data for the UI check: two matched articles and one unmatched
        for (key, n, source, oa, method) in [
            ("k1", 60i64, "openalex", Some(60i64), "orcid"),
            ("k2", 25, "openalex", Some(25), "title_search"),
        ] {
            let cites: Vec<CitingRecord> = (0..n)
                .map(|j| {
                    let days = (j * j * 7 + j * 13) % 420;
                    let date = now - Duration::days(days);
                    let (d, p) = if j % 17 == 0 {
                        (date.format("%Y").to_string(), "year")
                    } else if j % 11 == 0 {
                        (date.format("%Y-%m").to_string(), "month")
                    } else {
                        (date.format("%Y-%m-%d").to_string(), "day")
                    };
                    cite(&format!("{key}-{j}"), &d, p)
                })
                .collect();
            let mut rec = mrec(key, Some(source));
            rec.match_method = Some(method.to_string());
            rec.external_id = Some(format!("W20123456{key}"));
            rec.openalex_count = oa;
            store_backfill_article(&mut conn, &rec, now, &cites).expect("bf");
        }
        store_backfill_article(&mut conn, &mrec("k3", None), now, &[]).expect("bf");
        let run = start_run(&conn, now - Duration::minutes(5), 3).expect("run");
        finish_run(
            &conn,
            run,
            now - Duration::minutes(4),
            "ok",
            (2, 85),
            "2 of 3 articles matched, 85 citing records. 15 works found in OpenAlex through the ORCID iD; 1 articles matched from that set.",
        )
        .expect("fin");
    }
}
