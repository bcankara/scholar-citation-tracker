//! Numbers behind the panel cards: forecast, fastest risers, the next h / i10 step, the
//! distribution of citing works and the weekly summary rule. The maths is pure and tested;
//! the `build_*` functions only gather the inputs from the database.

use std::collections::HashMap;

use anyhow::Result;
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, Timelike, Utc, Weekday};
use rusqlite::Connection;
use serde::Serialize;

use crate::db;

/// Mean month length, for turning a per-day rate into a per-month figure.
const DAYS_PER_MONTH: f64 = 30.4375;

// ------------------------------------------------------------------ months

/// `(year, month)` of the last `n` calendar months ending with the month of `today`, oldest first.
pub fn last_months(today: NaiveDate, n: usize) -> Vec<(i32, u32)> {
    let mut out = Vec::with_capacity(n);
    let (mut y, mut m) = (today.year(), today.month());
    for _ in 0..n {
        out.push((y, m));
        if m == 1 {
            y -= 1;
            m = 12;
        } else {
            m -= 1;
        }
    }
    out.reverse();
    out
}

pub fn month_key(y: i32, m: u32) -> String {
    format!("{y:04}-{m:02}")
}

fn next_month(y: i32, m: u32) -> (i32, u32) {
    if m == 12 {
        (y + 1, 1)
    } else {
        (y, m + 1)
    }
}

pub fn days_in_month(y: i32, m: u32) -> i64 {
    let (ny, nm) = next_month(y, m);
    match (NaiveDate::from_ymd_opt(y, m, 1), NaiveDate::from_ymd_opt(ny, nm, 1)) {
        (Some(a), Some(b)) => (b - a).num_days(),
        _ => 30,
    }
}

/// Total at the last snapshot taken at or before `t`; `None` before the first snapshot.
pub fn level_at(points: &[(DateTime<Utc>, i64)], t: DateTime<Utc>) -> Option<i64> {
    points.iter().rev().find(|(pt, _)| *pt <= t).map(|(_, v)| *v)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MonthCount {
    pub ym: String,
    /// Citations gained in the month; `None` when neither source can say.
    pub count: Option<i64>,
    /// "snapshots" | "backfill" | "none"
    pub source: &'static str,
    /// The month is not over yet.
    pub partial: bool,
}

/// Citations per month from the snapshot totals: the total at the month's end minus the total at
/// its start. Needs a snapshot at or before the month's start; otherwise `None`.
/// `boundary(y, m)` is the instant the month starts (local midnight in the app).
pub fn monthly_from_snapshots(
    points: &[(DateTime<Utc>, i64)],
    months: &[(i32, u32)],
    now: DateTime<Utc>,
    boundary: &dyn Fn(i32, u32) -> DateTime<Utc>,
) -> Vec<Option<i64>> {
    months
        .iter()
        .map(|&(y, m)| {
            let start = boundary(y, m);
            let (ny, nm) = next_month(y, m);
            let end = boundary(ny, nm).min(now);
            let a = level_at(points, start)?;
            let b = level_at(points, end)?;
            Some((b - a).max(0))
        })
        .collect()
}

/// Merges snapshot-derived and backfill monthly counts: snapshots first, backfill where the
/// snapshots cannot say (the backfill counts every month of its range, a missing one is 0).
pub fn merge_months(
    months: &[(i32, u32)],
    today: NaiveDate,
    snap: &[Option<i64>],
    backfill: &HashMap<String, i64>,
    backfill_range: Option<(String, String)>,
) -> Vec<MonthCount> {
    let current = month_key(today.year(), today.month());
    months
        .iter()
        .enumerate()
        .map(|(i, &(y, m))| {
            let ym = month_key(y, m);
            let partial = ym == current;
            if let Some(Some(v)) = snap.get(i) {
                return MonthCount { ym, count: Some(*v), source: "snapshots", partial };
            }
            let in_range = backfill_range.as_ref().is_some_and(|(a, b)| ym >= *a && ym <= *b);
            if in_range {
                let v = backfill.get(&ym).copied().unwrap_or(0);
                return MonthCount { ym, count: Some(v), source: "backfill", partial };
            }
            MonthCount { ym, count: None, source: "none", partial }
        })
        .collect()
}

/// How the daily rate was estimated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Basis {
    /// Average of the last (up to) three complete months with data.
    RecentMonths,
    /// The current, unfinished month (at least three days in).
    CurrentMonth,
    /// Whole span of the snapshot history (at least three days).
    SnapshotSpan,
}

impl Basis {
    pub fn as_str(self) -> &'static str {
        match self {
            Basis::RecentMonths => "recent_months",
            Basis::CurrentMonth => "current_month",
            Basis::SnapshotSpan => "snapshot_span",
        }
    }
}

/// Citations per day over the whole snapshot history (needs two snapshots at least three days apart).
pub fn span_rate(points: &[(DateTime<Utc>, i64)]) -> Option<f64> {
    let (first, last) = (points.first()?, points.last()?);
    let days = (last.0 - first.0).num_seconds() as f64 / 86400.0;
    if days < 3.0 {
        return None;
    }
    Some((last.1 - first.1).max(0) as f64 / days)
}

/// Simple linear rate (citations per day). Prefers the last three complete months with data,
/// then the current month, then the span of the snapshot history.
pub fn estimate_rate(months: &[MonthCount], today: NaiveDate, span: Option<f64>) -> Option<(f64, Basis)> {
    let complete: Vec<&MonthCount> = months.iter().filter(|m| !m.partial && m.count.is_some()).collect();
    if !complete.is_empty() {
        let recent = &complete[complete.len().saturating_sub(3)..];
        let (mut sum, mut days) = (0i64, 0i64);
        for m in recent {
            let (y, mo) = (m.ym[0..4].parse().unwrap_or(2000), m.ym[5..7].parse().unwrap_or(1));
            sum += m.count.unwrap_or(0);
            days += days_in_month(y, mo);
        }
        if days > 0 {
            return Some((sum as f64 / days as f64, Basis::RecentMonths));
        }
    }
    if let Some(cur) = months.iter().find(|m| m.partial && m.count.is_some()) {
        let elapsed = i64::from(today.day());
        if elapsed >= 3 {
            return Some((cur.count.unwrap_or(0) as f64 / elapsed as f64, Basis::CurrentMonth));
        }
    }
    span.map(|r| (r, Basis::SnapshotSpan))
}

/// Estimated total on `to`: today's total plus the daily rate times the days left.
pub fn project_total(current: i64, per_day: f64, from: NaiveDate, to: NaiveDate) -> i64 {
    let days = (to - from).num_days().max(0) as f64;
    current + (per_day.max(0.0) * days).round() as i64
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalEta {
    Reached,
    /// Month (year, month) in which the goal is expected to be reached.
    Month(i32, u32),
    /// No usable rate, or further away than 30 years.
    Unknown,
}

pub fn goal_eta(current: i64, goal: i64, per_day: Option<f64>, today: NaiveDate) -> GoalEta {
    if current >= goal {
        return GoalEta::Reached;
    }
    let Some(rate) = per_day.filter(|r| r.is_finite() && *r > 0.0) else {
        return GoalEta::Unknown;
    };
    let days = ((goal - current) as f64 / rate).ceil();
    if days > 365.0 * 30.0 {
        return GoalEta::Unknown;
    }
    match today.checked_add_signed(Duration::days(days as i64)) {
        Some(d) => GoalEta::Month(d.year(), d.month()),
        None => GoalEta::Unknown,
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Forecast {
    pub has_data: bool,
    /// "snapshots" | "backfill" | "mixed" | "none"
    pub source: String,
    pub current: i64,
    pub months: Vec<MonthCount>,
    pub per_month: Option<f64>,
    pub basis: Option<String>,
    pub year: i32,
    pub year_end: Option<i64>,
    pub goal: Option<i64>,
    /// "none" | "reached" | "month" | "unknown"
    pub goal_status: String,
    pub goal_ym: Option<String>,
}

pub fn build_forecast(
    conn: &Connection,
    now: DateTime<Utc>,
    today: NaiveDate,
    boundary: &dyn Fn(i32, u32) -> DateTime<Utc>,
) -> Result<Forecast> {
    let series = db::time_series(conn)?;
    let points: Vec<(DateTime<Utc>, i64)> = series
        .iter()
        .filter_map(|p| db::parse_ts(&p.t).map(|t| (t, p.cit_all)))
        .collect();
    let goal = db::get_state(conn, db::STATE_GOAL)?
        .and_then(|s| s.parse::<i64>().ok())
        .filter(|g| *g > 0);
    let Some(current) = points.last().map(|p| p.1) else {
        return Ok(Forecast {
            has_data: false,
            source: "none".into(),
            current: 0,
            months: Vec::new(),
            per_month: None,
            basis: None,
            year: today.year(),
            year_end: None,
            goal,
            goal_status: "none".into(),
            goal_ym: None,
        });
    };
    let months = last_months(today, 12);
    let snap = monthly_from_snapshots(&points, &months, now, boundary);
    let bf = db::history_series(conn, "month")?;
    let bf_map: HashMap<String, i64> = bf.points.iter().cloned().collect();
    let range = bf
        .points
        .first()
        .map(|f| (f.0.clone(), month_key(today.year(), today.month())));
    let merged = merge_months(&months, today, &snap, &bf_map, range);
    let used: std::collections::BTreeSet<&str> = merged.iter().filter(|m| m.count.is_some()).map(|m| m.source).collect();
    let source = match (used.contains("snapshots"), used.contains("backfill")) {
        (true, true) => "mixed",
        (true, false) => "snapshots",
        (false, true) => "backfill",
        _ => "none",
    };
    let est = estimate_rate(&merged, today, span_rate(&points));
    let year_end_date = NaiveDate::from_ymd_opt(today.year(), 12, 31).unwrap_or(today);
    let year_end = est.map(|(r, _)| project_total(current, r, today, year_end_date));
    let (goal_status, goal_ym) = match goal {
        None => ("none", None),
        Some(g) => match goal_eta(current, g, est.map(|e| e.0), today) {
            GoalEta::Reached => ("reached", None),
            GoalEta::Month(y, m) => ("month", Some(month_key(y, m))),
            GoalEta::Unknown => ("unknown", None),
        },
    };
    Ok(Forecast {
        has_data: true,
        source: source.into(),
        current,
        months: merged,
        per_month: est.map(|(r, _)| r * DAYS_PER_MONTH),
        basis: est.map(|(_, b)| b.as_str().to_string()),
        year: today.year(),
        year_end,
        goal,
        goal_status: goal_status.into(),
        goal_ym,
    })
}

// ----------------------------------------------------------- fastest risers

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Riser {
    pub key: String,
    pub title: String,
    pub citations: i64,
    pub gain: i64,
}

/// The `n` articles with the largest positive gain (ties: more citations first, then title).
pub fn top_risers(latest: &[(String, String, i64)], gain_of: &dyn Fn(&str, i64) -> i64, n: usize) -> Vec<Riser> {
    let mut v: Vec<Riser> = latest
        .iter()
        .map(|(k, t, c)| Riser { key: k.clone(), title: t.clone(), citations: *c, gain: gain_of(k, *c) })
        .filter(|r| r.gain > 0)
        .collect();
    v.sort_by(|a, b| b.gain.cmp(&a.gain).then(b.citations.cmp(&a.citations)).then(a.title.cmp(&b.title)));
    v.truncate(n);
    v
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Risers {
    pub days: i64,
    /// "snapshots" | "backfill" | "snapshots_partial" | "none"
    pub basis: String,
    /// Days the snapshot history really covers (for "snapshots_partial").
    pub covered_days: i64,
    pub items: Vec<Riser>,
}

pub fn build_risers(conn: &Connection, now: DateTime<Utc>, days: i64) -> Result<Risers> {
    let latest = db::latest_article_citations(conn)?;
    let none = |basis: &str| Risers { days, basis: basis.into(), covered_days: 0, items: Vec::new() };
    if latest.is_empty() {
        return Ok(none("none"));
    }
    let cutoff = now - Duration::days(days);
    if let Some(reference) = db::snapshot_at_or_before(conn, cutoff)? {
        let map = db::article_map_at(conn, reference.id)?;
        let items = top_risers(&latest, &|k, c| c - map.get(k).copied().unwrap_or(0), 5);
        return Ok(Risers { days, basis: "snapshots".into(), covered_days: days, items });
    }
    let since = cutoff.date_naive();
    let counts = db::backfill_counts_since(conn, since)?;
    if !counts.is_empty() {
        let items = top_risers(&latest, &|k, _| counts.get(k).copied().unwrap_or(0), 5);
        return Ok(Risers { days, basis: "backfill".into(), covered_days: days, items });
    }
    // Not enough history for the window: use what the snapshots have, and say so.
    if let (Some(first), Some(last)) = (db::earliest_snapshot(conn)?, db::latest_snapshot(conn)?) {
        if first.id != last.id {
            let map = db::article_map_at(conn, first.id)?;
            let covered = match (db::parse_ts(&first.fetched_at), db::parse_ts(&last.fetched_at)) {
                (Some(a), Some(b)) => (b - a).num_days(),
                _ => 0,
            };
            let items = top_risers(&latest, &|k, c| c - map.get(k).copied().unwrap_or(0), 5);
            return Ok(Risers { days, basis: "snapshots_partial".into(), covered_days: covered, items });
        }
    }
    Ok(none("none"))
}

// ------------------------------------------------------------ h / i10 steps

pub fn h_index(citations: &[i64]) -> i64 {
    let mut c: Vec<i64> = citations.to_vec();
    c.sort_unstable_by(|a, b| b.cmp(a));
    c.iter().enumerate().take_while(|(i, v)| **v > *i as i64).count() as i64
}

pub fn i10_index(citations: &[i64]) -> i64 {
    citations.iter().filter(|c| **c >= 10).count() as i64
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Pick {
    pub key: String,
    pub title: String,
    pub citations: i64,
    /// Citations still missing to reach the threshold.
    pub needs: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct NextStep {
    pub current: i64,
    pub target: i64,
    /// Citations an article needs to count toward the target.
    pub threshold: i64,
    /// How many articles must reach the threshold in total (not yet there).
    pub need_papers: usize,
    /// The closest articles, most cited first.
    pub picks: Vec<Pick>,
    /// Sum of the missing citations over `picks`.
    pub total_needed: i64,
    /// Articles missing besides the picks: there are not enough articles at all.
    pub new_papers: usize,
}

fn next_step(arts: &[(String, String, i64)], current: i64, threshold: i64, need_papers: usize) -> NextStep {
    let mut below: Vec<&(String, String, i64)> = arts.iter().filter(|a| a.2 < threshold).collect();
    below.sort_by(|a, b| b.2.cmp(&a.2).then(a.1.cmp(&b.1)));
    let picks: Vec<Pick> = below
        .iter()
        .take(need_papers)
        .map(|a| Pick { key: a.0.clone(), title: a.1.clone(), citations: a.2, needs: threshold - a.2 })
        .collect();
    let total_needed = picks.iter().map(|p| p.needs).sum();
    NextStep {
        current,
        target: 0,
        threshold,
        need_papers,
        new_papers: need_papers.saturating_sub(picks.len()),
        picks,
        total_needed,
    }
}

/// What the next h needs: h+1 articles with at least h+1 citations each.
pub fn h_next_step(arts: &[(String, String, i64)]) -> NextStep {
    let cites: Vec<i64> = arts.iter().map(|a| a.2).collect();
    let h = h_index(&cites);
    let target = h + 1;
    let already = cites.iter().filter(|c| **c >= target).count() as i64;
    let need = (target - already).max(1) as usize;
    NextStep { target, ..next_step(arts, h, target, need) }
}

/// What the next i10 needs: one more article with at least 10 citations.
pub fn i10_next_step(arts: &[(String, String, i64)]) -> NextStep {
    let cites: Vec<i64> = arts.iter().map(|a| a.2).collect();
    let i10 = i10_index(&cites);
    NextStep { target: i10 + 1, ..next_step(arts, i10, 10, 1) }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct NextSteps {
    pub h: NextStep,
    pub i10: NextStep,
}

pub fn build_next_steps(conn: &Connection) -> Result<Option<NextSteps>> {
    let arts = db::latest_article_citations(conn)?;
    if arts.is_empty() {
        return Ok(None);
    }
    Ok(Some(NextSteps { h: h_next_step(&arts), i10: i10_next_step(&arts) }))
}

// ------------------------------------------------------------- distribution

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct Distribution {
    /// Distinct citing works.
    pub total: usize,
    pub countries: Vec<(String, i64)>,
    pub with_country: usize,
    pub without_country: usize,
    pub venues: Vec<(String, i64)>,
    pub without_venue: usize,
}

fn top(counts: HashMap<String, (String, i64)>, n: usize) -> Vec<(String, i64)> {
    let mut v: Vec<(String, i64)> = counts.into_values().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v.truncate(n);
    v
}

/// Counts distinct citing works by author country (a work counts once per country), by venue
/// (case-insensitive). Top 10 for country and venue.
pub fn distribution(rows: &[db::DistRow]) -> Distribution {
    let mut countries: HashMap<String, (String, i64)> = HashMap::new();
    let mut venues: HashMap<String, (String, i64)> = HashMap::new();
    let mut d = Distribution { total: rows.len(), ..Distribution::default() };
    for r in rows {
        let codes: std::collections::BTreeSet<&str> = r.countries.split(',').map(str::trim).filter(|c| c.len() == 2).collect();
        if codes.is_empty() {
            d.without_country += 1;
        } else {
            d.with_country += 1;
            for c in codes {
                countries.entry(c.to_string()).or_insert_with(|| (c.to_string(), 0)).1 += 1;
            }
        }
        let venue = r.venue.trim();
        if venue.is_empty() {
            d.without_venue += 1;
        } else {
            venues.entry(venue.to_lowercase()).or_insert_with(|| (venue.to_string(), 0)).1 += 1;
        }
    }
    d.countries = top(countries, 10);
    d.venues = top(venues, 10);
    d
}

pub fn build_distribution(conn: &Connection) -> Result<Distribution> {
    Ok(distribution(&db::distinct_citing_works(conn)?))
}

// ------------------------------------------------------------ recent citations

/// One row of the "Recent citations" card: a change Scholar showed, or a citing work found
/// through OpenAlex / Semantic Scholar.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct RecentItem {
    /// "scholar_up" | "scholar_down" | "citing"
    pub kind: String,
    /// Timestamp (Scholar changes) or `YYYY-MM-DD` / `YYYY-MM` (citing works).
    pub date: String,
    pub precision: String,
    pub article_key: String,
    pub article_title: String,
    pub cites_ids: String,
    /// Signed change for Scholar rows, 0 for citing works.
    pub delta: i64,
    pub old: i64,
    pub new: i64,
    pub citing_title: String,
    pub venue: String,
    pub source: String,
    pub doi: String,
    pub url: String,
    pub citing_id: String,
}

/// Day a row is filed under, for ordering: the first ten characters of a timestamp, a month
/// becomes its first day.
fn day_key(date: &str) -> String {
    match date.len() {
        0..=4 => format!("{date:0<4}-01-01"),
        5..=7 => format!("{date}-01"),
        _ => date[..10.min(date.len())].to_string(),
    }
}

/// Newest `limit` rows of both kinds, newest first. Citing works dated by year only are left
/// out (they cannot be called recent); Scholar rows sort before citing works of the same day.
pub fn merge_recent(changes: Vec<db::ScholarChange>, citing: Vec<db::CitingRow>, limit: usize) -> Vec<RecentItem> {
    let mut rows: Vec<(String, u8, String, RecentItem)> = Vec::new();
    for c in changes {
        rows.push((
            day_key(&c.at),
            0,
            c.at.clone(),
            RecentItem {
                kind: if c.delta > 0 { "scholar_up" } else { "scholar_down" }.into(),
                date: c.at,
                article_key: c.article_key,
                article_title: c.title,
                cites_ids: c.cites_ids,
                delta: c.delta,
                old: c.old,
                new: c.new,
                source: "scholar".into(),
                ..RecentItem::default()
            },
        ));
    }
    for c in citing.into_iter().filter(|c| c.precision != "year") {
        rows.push((
            day_key(&c.date),
            1,
            c.citing_id.clone(),
            RecentItem {
                kind: "citing".into(),
                date: c.date,
                precision: c.precision,
                article_key: c.article_key,
                article_title: c.article_title,
                citing_title: c.title,
                venue: c.venue,
                source: c.source,
                doi: c.doi,
                url: c.url,
                citing_id: c.citing_id,
                ..RecentItem::default()
            },
        ));
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(b.2.cmp(&a.2)));
    rows.into_iter().take(limit).map(|r| r.3).collect()
}

pub fn build_recent(conn: &Connection, limit: usize) -> Result<Vec<RecentItem>> {
    Ok(merge_recent(db::scholar_changes(conn)?, db::citing_works(conn)?, limit))
}

// ----------------------------------------------------------- weekly summary

/// `2026-W47` for the ISO week of `d`.
pub fn iso_week_key(d: NaiveDate) -> String {
    let w = d.iso_week();
    format!("{}-W{:02}", w.year(), w.week())
}

#[derive(Debug, Clone, PartialEq)]
pub struct WeekSummary {
    pub gain: i64,
    /// Article with the largest gain and that gain.
    pub top: Option<(String, i64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Weekly {
    /// Show this summary and remember the week key.
    Show { week: String, summary: WeekSummary },
    Skip(&'static str),
}

/// Decides whether the weekly summary is shown now: only on Monday, from 09:00 local time, once
/// per ISO week, when the feature is on, and when the week has data (a positive gain).
pub fn weekly_decision(
    now_local: NaiveDateTime,
    last_week: Option<&str>,
    enabled: bool,
    summary: Option<WeekSummary>,
) -> Weekly {
    if !enabled {
        return Weekly::Skip("disabled");
    }
    if now_local.weekday() != Weekday::Mon {
        return Weekly::Skip("not_monday");
    }
    if now_local.hour() < 9 {
        return Weekly::Skip("too_early");
    }
    let week = iso_week_key(now_local.date());
    if last_week == Some(week.as_str()) {
        return Weekly::Skip("already_sent");
    }
    match summary {
        Some(s) if s.gain > 0 => Weekly::Show { week, summary: s },
        _ => Weekly::Skip("no_data"),
    }
}

/// Gain of the last seven days and the article that gained most. `None` without data in the
/// week: fewer than two snapshots, or the newest one older than seven days.
pub fn build_week_summary(conn: &Connection, now: DateTime<Utc>) -> Result<Option<WeekSummary>> {
    let Some(latest) = db::latest_snapshot(conn)? else {
        return Ok(None);
    };
    let fresh = db::parse_ts(&latest.fetched_at).is_some_and(|t| now - t < Duration::days(7));
    if !fresh {
        return Ok(None);
    }
    let reference = match db::snapshot_at_or_before(conn, now - Duration::days(7))? {
        Some(s) => s,
        None => match db::earliest_snapshot(conn)? {
            Some(s) => s,
            None => return Ok(None),
        },
    };
    if reference.id == latest.id {
        return Ok(None);
    }
    let map = db::article_map_at(conn, reference.id)?;
    let arts = db::latest_article_citations(conn)?;
    let top = top_risers(&arts, &|k, c| c - map.get(k).copied().unwrap_or(0), 1)
        .into_iter()
        .next()
        .map(|r| (r.title, r.gain));
    Ok(Some(WeekSummary { gain: latest.cit_all - reference.cit_all, top }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("date")
    }

    fn t(s: &str) -> DateTime<Utc> {
        db::parse_ts(s).expect("ts")
    }

    fn utc_boundary(y: i32, m: u32) -> DateTime<Utc> {
        t(&format!("{y:04}-{m:02}-01T00:00:00Z"))
    }

    fn art(k: &str, c: i64) -> (String, String, i64) {
        (k.to_string(), format!("Title {k}"), c)
    }

    // ---------------------------------------------------------------- months

    #[test]
    fn last_months_ends_with_the_current_month_and_crosses_years() {
        let m = last_months(d("2026-02-14"), 4);
        assert_eq!(m, vec![(2025, 11), (2025, 12), (2026, 1), (2026, 2)]);
        assert_eq!(last_months(d("2026-11-22"), 12).len(), 12);
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2026, 12), 31);
    }

    #[test]
    fn snapshot_months_need_a_baseline() {
        let pts = vec![
            (t("2026-09-10T10:00:00Z"), 100),
            (t("2026-10-05T10:00:00Z"), 110),
            (t("2026-10-31T10:00:00Z"), 130),
            (t("2026-11-15T10:00:00Z"), 135),
        ];
        let months = last_months(d("2026-11-20"), 3); // Sep, Oct, Nov
        let r = monthly_from_snapshots(&pts, &months, t("2026-11-20T00:00:00Z"), &utc_boundary);
        // September: first snapshot is inside the month: no baseline
        assert_eq!(r[0], None);
        // October: 100 at Oct 1 (Sep 10 snapshot) to 130 at Nov 1 (Oct 31 snapshot)
        assert_eq!(r[1], Some(30));
        // November (partial): 130 at Nov 1 to 135 now
        assert_eq!(r[2], Some(5));
    }

    #[test]
    fn merge_prefers_snapshots_and_falls_back_to_backfill() {
        let months = last_months(d("2026-11-20"), 4); // Aug..Nov
        let snap = vec![None, None, Some(30), Some(5)];
        let mut bf = HashMap::new();
        bf.insert("2026-08".to_string(), 7);
        bf.insert("2026-10".to_string(), 99); // ignored: snapshots win
        let m = merge_months(&months, d("2026-11-20"), &snap, &bf, Some(("2026-07".into(), "2026-11".into())));
        assert_eq!((m[0].count, m[0].source), (Some(7), "backfill"));
        assert_eq!((m[1].count, m[1].source), (Some(0), "backfill")); // inside the backfill range, none that month
        assert_eq!((m[2].count, m[2].source), (Some(30), "snapshots"));
        assert!(m[3].partial && m[3].count == Some(5));
        // no backfill and no snapshots: unknown
        let m = merge_months(&months, d("2026-11-20"), &[None, None, None, None], &HashMap::new(), None);
        assert!(m.iter().all(|x| x.count.is_none() && x.source == "none"));
    }

    // ---------------------------------------------------------------- forecast

    fn mc(ym: &str, c: Option<i64>, partial: bool) -> MonthCount {
        MonthCount { ym: ym.into(), count: c, source: if c.is_some() { "snapshots" } else { "none" }, partial }
    }

    #[test]
    fn rate_uses_the_last_three_complete_months() {
        let months = vec![
            mc("2026-07", Some(1000), false), // too old: only the last three count
            mc("2026-08", Some(31), false),
            mc("2026-09", Some(30), false),
            mc("2026-10", Some(31), false),
            mc("2026-11", Some(5), true),
        ];
        let (rate, basis) = estimate_rate(&months, d("2026-11-20"), None).expect("rate");
        assert_eq!(basis, Basis::RecentMonths);
        assert!((rate - 92.0 / 92.0).abs() < 1e-9, "{rate}"); // 92 citations over 92 days
    }

    #[test]
    fn rate_falls_back_to_current_month_then_span() {
        let only_current = vec![mc("2026-10", None, false), mc("2026-11", Some(10), true)];
        let (r, b) = estimate_rate(&only_current, d("2026-11-20"), Some(9.0)).expect("rate");
        assert_eq!(b, Basis::CurrentMonth);
        assert!((r - 0.5).abs() < 1e-9);
        // too early in the month for the current-month estimate: span rate
        let (r, b) = estimate_rate(&only_current, d("2026-11-02"), Some(0.25)).expect("rate");
        assert_eq!((b, r), (Basis::SnapshotSpan, 0.25));
        assert!(estimate_rate(&[mc("2026-11", None, true)], d("2026-11-20"), None).is_none());
    }

    #[test]
    fn span_rate_needs_three_days() {
        let a = (t("2026-11-01T00:00:00Z"), 100);
        assert_eq!(span_rate(&[a]), None);
        assert_eq!(span_rate(&[a, (t("2026-11-02T00:00:00Z"), 110)]), None);
        let r = span_rate(&[a, (t("2026-11-11T00:00:00Z"), 150)]).expect("rate");
        assert!((r - 5.0).abs() < 1e-9);
    }

    #[test]
    fn year_end_projection_is_linear() {
        // 2 citations per day, 40 days left in the year
        assert_eq!(project_total(500, 2.0, d("2026-11-21"), d("2026-12-31")), 580);
        assert_eq!(project_total(500, 0.0, d("2026-11-21"), d("2026-12-31")), 500);
        // a negative rate never lowers the total; a past target adds nothing
        assert_eq!(project_total(500, -3.0, d("2026-11-21"), d("2026-12-31")), 500);
        assert_eq!(project_total(500, 2.0, d("2026-12-31"), d("2026-12-31")), 500);
        // rounding: 0.4 per day over 10 days = 4
        assert_eq!(project_total(10, 0.4, d("2026-12-21"), d("2026-12-31")), 14);
    }

    #[test]
    fn goal_eta_cases() {
        let today = d("2026-11-22");
        assert_eq!(goal_eta(1000, 1000, Some(1.0), today), GoalEta::Reached);
        assert_eq!(goal_eta(1200, 1000, None, today), GoalEta::Reached);
        assert_eq!(goal_eta(900, 1000, None, today), GoalEta::Unknown);
        assert_eq!(goal_eta(900, 1000, Some(0.0), today), GoalEta::Unknown);
        // 100 missing at 1 per day: 100 days after 22 Nov 2026 is 2 Mar 2027
        assert_eq!(goal_eta(900, 1000, Some(1.0), today), GoalEta::Month(2027, 3));
        // 31 days at 1 per day: 23 Dec 2026
        assert_eq!(goal_eta(969, 1000, Some(1.0), today), GoalEta::Month(2026, 12));
        // fractional rates round the day count up
        assert_eq!(goal_eta(999, 1000, Some(0.5), d("2026-12-30")), GoalEta::Month(2027, 1));
        // further away than 30 years
        assert_eq!(goal_eta(0, 1_000_000, Some(0.01), today), GoalEta::Unknown);
    }

    // ----------------------------------------------------------- h / i10

    #[test]
    fn h_and_i10_index() {
        assert_eq!(h_index(&[]), 0);
        assert_eq!(h_index(&[0, 0]), 0);
        assert_eq!(h_index(&[10, 8, 5, 4, 3]), 4);
        assert_eq!(h_index(&[3, 3, 3]), 3);
        assert_eq!(h_index(&[100]), 1);
        assert_eq!(i10_index(&[10, 9, 11, 100, 0]), 3);
    }

    #[test]
    fn h_next_step_picks_the_closest_articles() {
        // h = 4 (10, 8, 5, 4 count); the next h = 5 needs 5 papers with >= 5
        let arts = vec![art("a", 10), art("b", 8), art("c", 5), art("d", 4), art("e", 3), art("f", 1)];
        let s = h_next_step(&arts);
        assert_eq!((s.current, s.target, s.threshold), (4, 5, 5));
        // three articles have >= 5 already: two more are needed (d needs 1, e needs 2)
        assert_eq!(s.need_papers, 2);
        assert_eq!(s.picks.iter().map(|p| (p.key.as_str(), p.needs)).collect::<Vec<_>>(), vec![("d", 1), ("e", 2)]);
        assert_eq!(s.total_needed, 3);
        assert_eq!(s.new_papers, 0);
    }

    #[test]
    fn h_next_step_with_too_few_articles_asks_for_new_ones() {
        // h = 2; h = 3 needs three articles with >= 3 citations but there are only two
        let arts = vec![art("a", 5), art("b", 4)];
        let s = h_next_step(&arts);
        assert_eq!((s.current, s.target), (2, 3));
        assert_eq!(s.need_papers, 1);
        assert!(s.picks.is_empty());
        assert_eq!(s.new_papers, 1);
        // no articles at all
        let s = h_next_step(&[]);
        assert_eq!((s.current, s.target, s.need_papers, s.new_papers), (0, 1, 1, 1));
    }

    #[test]
    fn h_next_step_ties_prefer_the_alphabetically_first_title() {
        let arts = vec![art("a", 6), art("b", 6), art("c", 6), art("d", 3), art("e", 3)];
        // h = 3; h = 4: three articles have >= 4, one more needed: d and e both need 1 -> "Title d"
        let s = h_next_step(&arts);
        assert_eq!(s.picks.len(), 1);
        assert_eq!(s.picks[0].key, "d");
        assert_eq!(s.picks[0].needs, 1);
    }

    #[test]
    fn i10_next_step_needs_one_more_article() {
        let arts = vec![art("a", 25), art("b", 10), art("c", 8), art("d", 3)];
        let s = i10_next_step(&arts);
        assert_eq!((s.current, s.target, s.threshold, s.need_papers), (2, 3, 10, 1));
        assert_eq!(s.picks.len(), 1);
        assert_eq!((s.picks[0].key.as_str(), s.picks[0].needs), ("c", 2));
        assert_eq!(s.total_needed, 2);
        // every article is already at 10: a new article is needed
        let all = vec![art("a", 25), art("b", 10)];
        let s = i10_next_step(&all);
        assert!(s.picks.is_empty());
        assert_eq!(s.new_papers, 1);
    }

    // ----------------------------------------------------------- risers

    #[test]
    fn risers_are_sorted_by_gain_and_limited() {
        let latest = vec![art("a", 50), art("b", 40), art("c", 30), art("d", 20), art("e", 10), art("f", 5), art("g", 1)];
        let refs: HashMap<&str, i64> = [("a", 49), ("b", 30), ("c", 25), ("d", 20), ("e", 8), ("f", 0)].into_iter().collect();
        let r = top_risers(&latest, &|k, c| c - refs.get(k).copied().unwrap_or(0), 3);
        assert_eq!(r.iter().map(|x| (x.key.as_str(), x.gain)).collect::<Vec<_>>(), vec![("b", 10), ("c", 5), ("f", 5)]);
        // d did not move, g is new with 1 citation
        let all = top_risers(&latest, &|k, c| c - refs.get(k).copied().unwrap_or(0), 10);
        assert!(all.iter().all(|x| x.gain > 0));
        assert!(all.iter().any(|x| x.key == "g"));
        assert!(!all.iter().any(|x| x.key == "d"));
    }

    // ------------------------------------------------------ distribution

    fn dr(venue: &str, date: &str, countries: &str) -> db::DistRow {
        db::DistRow { venue: venue.into(), date: date.into(), countries: countries.into(), source: "openalex".into() }
    }

    #[test]
    fn distribution_counts_works_once_per_country_and_venue() {
        let rows = vec![
            dr("Journal A", "2024-03-01", "TR,US"),
            dr("journal a", "2024-05", "TR"),
            dr("Journal B", "2025", ""),
            dr("", "2025-01-02", "DE"),
        ];
        let d = distribution(&rows);
        assert_eq!(d.total, 4);
        assert_eq!(d.countries, vec![("TR".to_string(), 2), ("DE".to_string(), 1), ("US".to_string(), 1)]);
        assert_eq!((d.with_country, d.without_country), (3, 1));
        assert_eq!(d.venues, vec![("Journal A".to_string(), 2), ("Journal B".to_string(), 1)]);
        assert_eq!(d.without_venue, 1);
    }

    fn sc(key: &str, at: &str, old: i64, new: i64) -> db::ScholarChange {
        db::ScholarChange {
            article_key: key.into(),
            title: format!("Article {key}"),
            cites_ids: "1".into(),
            at: at.into(),
            old,
            new,
            delta: new - old,
        }
    }

    fn cr(id: &str, date: &str, precision: &str) -> db::CitingRow {
        db::CitingRow {
            article_key: "k1".into(),
            article_title: "Article k1".into(),
            source: "openalex".into(),
            citing_id: id.into(),
            title: format!("Citing {id}"),
            date: date.into(),
            precision: precision.into(),
            venue: "V".into(),
            doi: String::new(),
            url: String::new(),
            authors: String::new(),
            countries: String::new(),
            issn_list: String::new(),
        }
    }

    #[test]
    fn recent_merges_both_sources_in_time_order_with_both_directions() {
        let changes = vec![
            sc("a", "2026-11-21T09:00:00Z", 3, 5),
            sc("b", "2026-11-18T09:00:00Z", 7, 6),
            sc("c", "2026-10-01T09:00:00Z", 1, 2),
        ];
        let citing = vec![cr("W1", "2026-11-20", "day"), cr("W2", "2026-11", "month"), cr("W3", "2025", "year")];
        let r = merge_recent(changes, citing, 10);
        let kinds: Vec<(&str, i64)> = r.iter().map(|i| (i.kind.as_str(), i.delta)).collect();
        assert_eq!(
            kinds,
            vec![("scholar_up", 2), ("citing", 0), ("scholar_down", -1), ("citing", 0), ("scholar_up", 1)],
            "year-only citing works are left out; the month-dated one files under the 1st"
        );
        assert_eq!(r[1].citing_id, "W1");
        assert_eq!(r[3].citing_id, "W2");
    }

    #[test]
    fn recent_is_capped() {
        let changes: Vec<db::ScholarChange> = (1..=12).map(|d| sc("a", &format!("2026-11-{d:02}T09:00:00Z"), d, d + 1)).collect();
        let r = merge_recent(changes, Vec::new(), 10);
        assert_eq!(r.len(), 10);
        assert!(r[0].date.starts_with("2026-11-12"));
    }

    #[test]
    fn distribution_keeps_only_the_top_ten() {
        let rows: Vec<db::DistRow> = (0..15)
            .map(|i| dr(&format!("Venue {i:02}"), "2024-01-01", &format!("{}{}", char::from(b'A' + i as u8), "X")))
            .collect();
        let d = distribution(&rows);
        assert_eq!(d.venues.len(), 10);
        assert_eq!(d.countries.len(), 10);
        assert_eq!(d.total, 15);
    }

    // ----------------------------------------------------------- weekly

    fn ndt(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").expect("datetime")
    }

    fn summary(gain: i64) -> Option<WeekSummary> {
        Some(WeekSummary { gain, top: Some(("Title a".into(), 2)) })
    }

    #[test]
    fn weekly_shows_on_monday_from_nine() {
        // 2026-11-23 is a Monday
        assert_eq!(iso_week_key(d("2026-11-23")), "2026-W48");
        let show = weekly_decision(ndt("2026-11-23 09:00"), None, true, summary(4));
        assert!(matches!(show, Weekly::Show { ref week, .. } if week == "2026-W48"), "{show:?}");
        assert_eq!(weekly_decision(ndt("2026-11-23 08:59"), None, true, summary(4)), Weekly::Skip("too_early"));
        assert!(matches!(weekly_decision(ndt("2026-11-23 23:30"), None, true, summary(4)), Weekly::Show { .. }));
    }

    #[test]
    fn weekly_is_once_per_week_and_only_on_mondays() {
        assert_eq!(weekly_decision(ndt("2026-11-24 10:00"), None, true, summary(4)), Weekly::Skip("not_monday"));
        assert_eq!(weekly_decision(ndt("2026-11-22 10:00"), None, true, summary(4)), Weekly::Skip("not_monday"));
        assert_eq!(
            weekly_decision(ndt("2026-11-23 12:00"), Some("2026-W48"), true, summary(4)),
            Weekly::Skip("already_sent")
        );
        // a week later it shows again
        assert!(matches!(
            weekly_decision(ndt("2026-11-30 09:05"), Some("2026-W48"), true, summary(4)),
            Weekly::Show { .. }
        ));
    }

    #[test]
    fn weekly_respects_the_switch_and_empty_weeks() {
        assert_eq!(weekly_decision(ndt("2026-11-23 10:00"), None, false, summary(4)), Weekly::Skip("disabled"));
        assert_eq!(weekly_decision(ndt("2026-11-23 10:00"), None, true, None), Weekly::Skip("no_data"));
        assert_eq!(weekly_decision(ndt("2026-11-23 10:00"), None, true, summary(0)), Weekly::Skip("no_data"));
    }

    #[test]
    fn iso_week_keys_follow_iso_8601_at_year_borders() {
        assert_eq!(iso_week_key(d("2026-12-28")), "2026-W53");
        assert_eq!(iso_week_key(d("2027-01-03")), "2026-W53");
        assert_eq!(iso_week_key(d("2027-01-04")), "2027-W01");
        assert_eq!(iso_week_key(d("2024-12-30")), "2025-W01");
    }
}
