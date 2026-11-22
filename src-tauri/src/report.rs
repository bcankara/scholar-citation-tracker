//! PDF citation report: the data (this file) and the printing (`pdf`). The report page
//! (`ui/report.html`) builds the document from the JSON made here, in a hidden window, and
//! WebView2 prints that window to an A4 PDF.

use std::collections::HashMap;
use std::sync::Mutex;

use chrono::{Duration, NaiveDate};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db;

#[derive(Debug, Clone, Deserialize)]
pub struct ReportOptions {
    pub from: String,
    pub to: String,
    pub lang: String,
    pub include_citing: bool,
    pub include_charts: bool,
}

#[derive(Debug)]
pub enum ReportError {
    /// Start after end, or an unreadable date.
    Range,
    /// No snapshot at or before the end of the range.
    NoData,
    Other(String),
}

impl From<anyhow::Error> for ReportError {
    fn from(e: anyhow::Error) -> Self {
        ReportError::Other(format!("{e:#}"))
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Summary {
    pub cit_all: i64,
    pub cit_since: i64,
    pub since_year: i64,
    pub h_all: i64,
    pub i10_all: i64,
    /// Changes between the baseline snapshot and the last snapshot of the range.
    pub change_all: i64,
    pub change_since: i64,
    pub change_h: i64,
    pub change_i10: i64,
    /// Time of the snapshot the figures belong to.
    pub end_at: String,
    /// Time of the baseline snapshot (`None` when there was none).
    pub start_at: Option<String>,
    /// No snapshot before the range: the first snapshot is the baseline and the change is partial.
    pub baseline_is_first: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ReportArticle {
    pub title: String,
    pub year: Option<i64>,
    pub citations: i64,
    pub gain: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ReportCiting {
    pub date: String,
    pub title: String,
    pub venue: String,
    pub article: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReportData {
    pub lang: String,
    pub profile_name: String,
    pub orcid: String,
    pub from: String,
    pub to: String,
    pub generated: String,
    pub include_citing: bool,
    pub include_charts: bool,
    pub summary: Summary,
    pub articles: Vec<ReportArticle>,
    /// (timestamp, total citations) of the snapshots inside the range, baseline first.
    pub series: Vec<(String, i64)>,
    pub yearly: Vec<(i32, i64)>,
    pub citing: Vec<ReportCiting>,
    pub citing_total: usize,
}

pub fn parse_day(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok()
}

/// A citing work belongs to the range when the period its date names overlaps it
/// (a work dated only by month or year counts when that month / year touches the range).
pub fn in_range(date: &str, precision: &str, from: NaiveDate, to: NaiveDate) -> bool {
    let (from_s, to_s) = (from.format("%Y-%m-%d").to_string(), to.format("%Y-%m-%d").to_string());
    match precision {
        "day" => date >= from_s.as_str() && date <= to_s.as_str(),
        "month" => date >= &from_s[..7] && date <= &to_s[..7],
        _ => date >= &from_s[..4] && date <= &to_s[..4],
    }
}

pub fn build(
    conn: &Connection,
    opts: &ReportOptions,
    orcid: &str,
    generated: &str,
) -> Result<ReportData, ReportError> {
    let (Some(from), Some(to)) = (parse_day(&opts.from), parse_day(&opts.to)) else {
        return Err(ReportError::Range);
    };
    if from > to {
        return Err(ReportError::Range);
    }
    let start = db::local_day_start_utc(from);
    let end = db::local_day_start_utc(to + Duration::days(1));
    // Last snapshot of the range; a snapshot taken exactly at the next midnight is not part of it.
    let end_snap = db::snapshot_at_or_before(conn, end - Duration::seconds(1))?.ok_or(ReportError::NoData)?;
    let before = db::snapshot_at_or_before(conn, start - Duration::seconds(1))?;
    let (base, baseline_is_first) = match before {
        Some(s) => (Some(s), false),
        None => {
            let first = db::earliest_snapshot(conn)?.filter(|f| f.id != end_snap.id);
            (first, true)
        }
    };
    let base_map: HashMap<String, i64> = match &base {
        Some(b) => db::article_map_at(conn, b.id)?,
        None => HashMap::new(),
    };
    let articles = db::article_citations_in(conn, end_snap.id)?;
    let years: HashMap<String, Option<i64>> = {
        let mut st = conn.prepare("SELECT article_key, year FROM articles").map_err(anyhow::Error::from)?;
        let rows = st
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?)))
            .map_err(anyhow::Error::from)?;
        rows.collect::<rusqlite::Result<HashMap<_, _>>>().map_err(anyhow::Error::from)?
    };
    let report_articles: Vec<ReportArticle> = articles
        .iter()
        .map(|(k, t, c)| ReportArticle {
            title: t.clone(),
            year: years.get(k).copied().flatten(),
            citations: *c,
            gain: if base.is_some() { c - base_map.get(k).copied().unwrap_or(0) } else { 0 },
        })
        .collect();

    let (start_s, end_s) = (db::ts(start), db::ts(end));
    let mut series: Vec<(String, i64)> = Vec::new();
    if let Some(b) = &base {
        series.push((b.fetched_at.clone(), b.cit_all));
    }
    for p in db::time_series(conn)? {
        if p.t >= start_s && p.t < end_s && base.as_ref().is_none_or(|b| p.t > b.fetched_at) {
            series.push((p.t, p.cit_all));
        }
    }
    let b = base.as_ref();
    let summary = Summary {
        cit_all: end_snap.cit_all,
        cit_since: end_snap.cit_since,
        since_year: end_snap.since_year,
        h_all: end_snap.h_all,
        i10_all: end_snap.i10_all,
        change_all: b.map_or(0, |b| end_snap.cit_all - b.cit_all),
        change_since: b.map_or(0, |b| end_snap.cit_since - b.cit_since),
        change_h: b.map_or(0, |b| end_snap.h_all - b.h_all),
        change_i10: b.map_or(0, |b| end_snap.i10_all - b.i10_all),
        end_at: end_snap.fetched_at.clone(),
        start_at: b.map(|b| b.fetched_at.clone()),
        baseline_is_first,
    };

    let mut citing: Vec<ReportCiting> = Vec::new();
    let mut citing_total = 0usize;
    if opts.include_citing {
        for c in db::citing_works(conn)? {
            if in_range(&c.date, &c.precision, from, to) {
                citing_total += 1;
                citing.push(ReportCiting { date: c.date, title: c.title, venue: c.venue, article: c.article_title });
            }
        }
    }
    Ok(ReportData {
        lang: opts.lang.clone(),
        profile_name: end_snap.name.clone(),
        orcid: orcid.to_string(),
        from: opts.from.trim().to_string(),
        to: opts.to.trim().to_string(),
        generated: generated.to_string(),
        include_citing: opts.include_citing,
        include_charts: opts.include_charts,
        summary,
        articles: report_articles,
        series,
        yearly: db::yearly_for(conn, end_snap.id)?,
        citing,
        citing_total,
    })
}

/// The report job: the JSON the report window asks for and the signal that it has rendered.
#[derive(Default)]
pub struct Job {
    pub payload: Mutex<Option<String>>,
    pub ready: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scholar::{Article, ProfileData, Stats};
    use chrono::{DateTime, Utc};

    fn day(s: &str) -> NaiveDate {
        parse_day(s).expect("day")
    }

    #[test]
    fn citing_dates_overlap_the_range_by_their_precision() {
        let (a, b) = (day("2026-03-15"), day("2026-06-10"));
        assert!(in_range("2026-03-15", "day", a, b));
        assert!(in_range("2026-06-10", "day", a, b));
        assert!(!in_range("2026-03-14", "day", a, b));
        assert!(!in_range("2026-06-11", "day", a, b));
        // months and years that touch the range count
        assert!(in_range("2026-03", "month", a, b));
        assert!(in_range("2026-06", "month", a, b));
        assert!(!in_range("2026-02", "month", a, b));
        assert!(!in_range("2026-07", "month", a, b));
        assert!(in_range("2026", "year", a, b));
        assert!(!in_range("2025", "year", a, b));
    }

    fn art(key: &str, title: &str, c: i64) -> Article {
        Article {
            key: key.into(),
            title: title.into(),
            authors: "A. Author".into(),
            venue: "Journal".into(),
            year: Some(2020),
            citations: c,
            url: String::new(),
        }
    }

    fn data(total_a: i64, total_b: i64, h: i64) -> ProfileData {
        let total = total_a + total_b;
        ProfileData {
            name: "Test Person".into(),
            stats: Stats { cit_all: total, cit_since: total, h_all: h, h_since: h, i10_all: 1, i10_since: 1, since_year: 2021 },
            articles: vec![art("a", "Alpha study", total_a), art("b", "Beta study", total_b)],
            yearly: vec![(2025, 3), (2026, total - 3)],
        }
    }

    #[test]
    fn report_uses_snapshots_inside_and_before_the_range() {
        let dir = std::env::temp_dir().join(format!("sct-report-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let mut conn = db::open(&dir.join("r.db")).expect("db");
        let at = |s: &str| -> DateTime<Utc> { db::parse_ts(s).expect("ts") };
        db::insert_snapshot(&mut conn, "P", at("2026-01-10T12:00:00Z"), &data(10, 5, 2)).expect("s1");
        db::insert_snapshot(&mut conn, "P", at("2026-03-10T12:00:00Z"), &data(14, 6, 3)).expect("s2");
        db::insert_snapshot(&mut conn, "P", at("2026-05-10T12:00:00Z"), &data(20, 6, 3)).expect("s3");
        db::insert_snapshot(&mut conn, "P", at("2026-09-10T12:00:00Z"), &data(99, 6, 4)).expect("s4");
        let opts = ReportOptions {
            from: "2026-02-01".into(),
            to: "2026-06-30".into(),
            lang: "en".into(),
            include_citing: true,
            include_charts: true,
        };
        let r = build(&conn, &opts, "0000-0002-1825-0097", "2026-11-22 10:00").expect("report");
        // the range ends before snapshot 4; the baseline is snapshot 1 (before the range)
        assert_eq!(r.summary.cit_all, 26);
        assert_eq!(r.summary.change_all, 26 - 15);
        assert_eq!(r.summary.change_h, 1);
        assert!(!r.summary.baseline_is_first);
        let alpha = r.articles.iter().find(|a| a.title == "Alpha study").expect("alpha");
        assert_eq!((alpha.citations, alpha.gain), (20, 10));
        // baseline + the two snapshots inside the range
        assert_eq!(r.series.len(), 3);
        assert_eq!(r.series[0].1, 15);

        // a range that starts before every snapshot: the first snapshot is the baseline
        let early = ReportOptions { from: "2025-01-01".into(), to: "2026-04-01".into(), ..opts.clone() };
        let r2 = build(&conn, &early, "", "x").expect("report");
        assert!(r2.summary.baseline_is_first);
        assert_eq!(r2.summary.cit_all, 20);
        assert_eq!(r2.summary.change_all, 20 - 15);

        // nothing at or before the end
        let none = ReportOptions { from: "2024-01-01".into(), to: "2024-02-01".into(), ..opts.clone() };
        assert!(matches!(build(&conn, &none, "", "x"), Err(ReportError::NoData)));
        // reversed range
        let rev = ReportOptions { from: "2026-06-30".into(), to: "2026-02-01".into(), ..opts };
        assert!(matches!(build(&conn, &rev, "", "x"), Err(ReportError::Range)));
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
