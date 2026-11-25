//! Numbers behind the panel and the Analysis page: the merged trend line and its forecast, fastest
//! risers, per-article stacks, the calendar, the h / i10 history and next steps, the "since your last
//! visit" summary, the distribution of citing works and the weekly summary rule. The maths is pure and tested;
//! the `build_*` functions only gather the inputs from the database.

use std::collections::HashMap;

use anyhow::Result;
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, Timelike, Utc, Weekday};
use rusqlite::Connection;
use serde::Serialize;

use crate::db;

// ------------------------------------------------------------------ levels

/// Total at the last snapshot taken at or before `t`; `None` before the first snapshot.
pub fn level_at(points: &[(DateTime<Utc>, i64)], t: DateTime<Utc>) -> Option<i64> {
    points.iter().rev().find(|(pt, _)| *pt <= t).map(|(_, v)| *v)
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

pub fn month_key(y: i32, m: u32) -> String {
    format!("{y:04}-{m:02}")
}

// ------------------------------------------------- trend: one continuous line

/// The forecast stays hidden until the snapshots cover at least this many days.
pub const FORECAST_MIN_DAYS: i64 = 60;
/// Length of one growth window.
const WINDOW_DAYS: i64 = 30;
/// Growth windows looked at (newest first).
const MAX_WINDOWS: usize = 6;
const MS_DAY: i64 = 86_400_000;

/// The historical (OpenAlex) cumulative curve before the first Scholar snapshot: raw counts,
/// one point per month end, a zero origin and a last point at the snapshot instant.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HistCurve {
    /// (unix ms, cumulative citing records).
    pub points: Vec<(i64, i64)>,
    /// Cumulative count at the snapshot instant (the last point).
    pub raw_last: i64,
    /// Records dated by year only: they cannot be placed on the curve.
    pub year_only: usize,
}

fn ms_of(d: NaiveDate, h: u32, m: u32, s: u32) -> i64 {
    d.and_hms_opt(h, m, s).map(|n| n.and_utc().timestamp_millis()).unwrap_or(0)
}

fn month_end(y: i32, m: u32) -> NaiveDate {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    NaiveDate::from_ymd_opt(ny, nm, 1).and_then(|d| d.pred_opt()).unwrap_or_default()
}

/// Cumulative curve of the citing works dated before `first` (the first Scholar snapshot).
/// `records` are (date, precision) pairs, one per (article, citing work). A day-dated record
/// counts when it lies before the snapshot's day; a month-dated one when its month is before the
/// snapshot's month; year-only records are counted separately and never placed.
pub fn historical_curve(records: &[(String, String)], first: DateTime<Utc>) -> HistCurve {
    let first_day = first.date_naive();
    let first_month = month_key(first_day.year(), first_day.month());
    let first_day_s = first_day.format("%Y-%m-%d").to_string();
    let mut by_month: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    let mut tail = 0i64;
    let mut year_only = 0usize;
    for (date, precision) in records {
        match precision.as_str() {
            "day" if date.len() >= 10 => {
                if date.as_str() >= first_day_s.as_str() {
                    continue;
                }
                if date[..7] < *first_month.as_str() {
                    *by_month.entry(date[..7].to_string()).or_insert(0) += 1;
                } else {
                    tail += 1;
                }
            }
            "month" if date.len() >= 7 => {
                if date[..7] < *first_month.as_str() {
                    *by_month.entry(date[..7].to_string()).or_insert(0) += 1;
                }
            }
            "year" => year_only += 1,
            _ => {}
        }
    }
    let mut c = HistCurve { year_only, ..HistCurve::default() };
    let Some(earliest) = by_month.keys().next().cloned() else {
        if tail > 0 {
            c.points.push((first.timestamp_millis() - MS_DAY, 0));
            c.points.push((first.timestamp_millis(), tail));
            c.raw_last = tail;
        }
        return c;
    };
    let ey: i32 = earliest[0..4].parse().unwrap_or(2000);
    let em: u32 = earliest[5..7].parse().unwrap_or(1);
    if let Some(origin) = NaiveDate::from_ymd_opt(ey, em, 1) {
        c.points.push((ms_of(origin, 0, 0, 0), 0));
    }
    let mut cum = 0i64;
    for (key, n) in &by_month {
        cum += n;
        let (y, m): (i32, u32) = (key[0..4].parse().unwrap_or(2000), key[5..7].parse().unwrap_or(1));
        c.points.push((ms_of(month_end(y, m), 23, 59, 59), cum));
    }
    cum += tail;
    c.points.push((first.timestamp_millis(), cum));
    c.raw_last = cum;
    c
}

/// Scales the curve so its last point equals `target` (the first Scholar snapshot) and the shape is
/// kept: every value is multiplied by the same factor. Empty when the curve is empty.
pub fn scale_curve(curve: &HistCurve, target: i64) -> (Vec<(i64, f64)>, f64) {
    if curve.raw_last <= 0 || curve.points.is_empty() {
        return (Vec::new(), 1.0);
    }
    let k = target as f64 / curve.raw_last as f64;
    let n = curve.points.len();
    let pts = curve
        .points
        .iter()
        .enumerate()
        .map(|(i, (t, v))| {
            let val = if i + 1 == n { target as f64 } else { (*v as f64 * k * 100.0).round() / 100.0 };
            (*t, val)
        })
        .collect();
    (pts, k)
}

/// Linear interpolation of a time-ordered line at `t`; clamped to the first / last value.
pub fn line_value_at(points: &[(i64, f64)], t: i64) -> Option<f64> {
    let (first, last) = (points.first()?, points.last()?);
    if t <= first.0 {
        return Some(first.1);
    }
    if t >= last.0 {
        return Some(last.1);
    }
    let i = points.partition_point(|p| p.0 <= t);
    let (a, b) = (points[i - 1], points[i]);
    if b.0 == a.0 {
        return Some(b.1);
    }
    Some(a.1 + (b.1 - a.1) * ((t - a.0) as f64 / (b.0 - a.0) as f64))
}

/// Growth per day in consecutive 30-day windows ending at the newest snapshot, newest first. A window needs a
/// snapshot at or before its start, so the first snapshot is only ever a baseline: the level it
/// brings in (everything Scholar already counted on the first day) is never counted as growth.
pub fn growth_windows(points: &[(DateTime<Utc>, i64)], now: DateTime<Utc>) -> Vec<f64> {
    let mut out = Vec::new();
    // anchored at the newest snapshot: a stretch with no checks (the app was off) is not zero growth
    let Some(anchor) = points.last().map(|p| p.0.min(now)) else {
        return out;
    };
    for i in 0..MAX_WINDOWS as i64 {
        let end = anchor - Duration::days(i * WINDOW_DAYS);
        let start = end - Duration::days(WINDOW_DAYS);
        let (Some(a), Some(b)) = (level_at(points, start), level_at(points, end)) else {
            break;
        };
        out.push((b - a) as f64 / WINDOW_DAYS as f64);
    }
    out
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ForecastBand {
    /// Where the dashed forecast starts: now, at the latest total.
    pub start_t: i64,
    pub start_v: i64,
    /// Year end (UTC) and the estimates there.
    pub end_t: i64,
    pub mid: i64,
    pub low: i64,
    pub high: i64,
    pub per_day: f64,
    pub low_per_day: f64,
    pub high_per_day: f64,
    /// Growth windows behind the estimate.
    pub windows: usize,
    /// Fewer than three windows: the range is wider.
    pub thin: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ForecastOutcome {
    Shown(ForecastBand),
    /// Not enough history yet: (days covered, days needed).
    Hidden { have: i64, need: i64 },
}

/// Linear forecast to the end of `today`'s year with an uncertainty range. Hidden while the
/// snapshots cover fewer than [`FORECAST_MIN_DAYS`] days.
pub fn forecast_band(points: &[(DateTime<Utc>, i64)], now: DateTime<Utc>, today: NaiveDate) -> ForecastOutcome {
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return ForecastOutcome::Hidden { have: 0, need: FORECAST_MIN_DAYS };
    };
    let have = (last.0 - first.0).num_days().max(0);
    let windows = growth_windows(points, now);
    if have < FORECAST_MIN_DAYS || windows.is_empty() {
        return ForecastOutcome::Hidden { have, need: FORECAST_MIN_DAYS };
    }
    let recent = &windows[..windows.len().min(3)];
    let central = (recent.iter().sum::<f64>() / recent.len() as f64).max(0.0);
    let mean_all = windows.iter().sum::<f64>() / windows.len() as f64;
    let sd = (windows.iter().map(|w| (w - mean_all).powi(2)).sum::<f64>() / windows.len() as f64).sqrt();
    let thin = windows.len() < 3;
    let spread = sd.max(central * if thin { 0.5 } else { 0.35 });
    let low = (central - spread).max(0.0);
    let high = central + spread;
    let current = last.1;
    let end_day = NaiveDate::from_ymd_opt(today.year(), 12, 31).unwrap_or(today);
    ForecastOutcome::Shown(ForecastBand {
        start_t: now.timestamp_millis(),
        start_v: current,
        end_t: ms_of(end_day, 23, 59, 59),
        mid: project_total(current, central, today, end_day),
        low: project_total(current, low, today, end_day),
        high: project_total(current, high, today, end_day),
        per_day: central,
        low_per_day: low,
        high_per_day: high,
        windows: windows.len(),
        thin,
    })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Merge {
    /// The first Scholar snapshot (unix ms) and its value: where the two sources meet.
    pub t: i64,
    pub value: i64,
    /// Raw cumulative count of the historical curve at that instant, and the factor it was scaled by.
    pub raw_last: i64,
    pub k: f64,
    /// Records of the historical lookup that are not on the curve (year-only dates).
    pub year_only: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Trend {
    pub has_data: bool,
    /// Scaled historical cumulative curve, ending at the merge point.
    pub hist: Vec<(i64, f64)>,
    /// Scholar snapshots (unix ms, total). The first one is the merge point.
    pub snaps: Vec<(i64, i64)>,
    pub merge: Option<Merge>,
    pub forecast: Option<ForecastBand>,
    /// `(days covered, days needed)` while the forecast is hidden.
    pub forecast_hidden: Option<(i64, i64)>,
    pub year: i32,
    /// Citations gained in the last 12 months on the merged line, and the days that really cover.
    pub last12: Option<i64>,
    pub last12_days: i64,
    /// Average per month over the span above.
    pub monthly_avg: Option<f64>,
    pub goal: Option<i64>,
    /// "none" | "reached" | "month" | "unknown"
    pub goal_status: String,
    pub goal_ym: Option<String>,
    /// Newest day-dated citing work (OpenAlex indexes new papers with a lag of weeks).
    pub oa_latest: Option<String>,
    /// OpenAlex citing works per month, over complete months it already covers (the lag tail is excluded).
    pub oa_per_month: Option<f64>,
}

fn empty_trend(today: NaiveDate, goal: Option<i64>) -> Trend {
    Trend {
        has_data: false,
        hist: Vec::new(),
        snaps: Vec::new(),
        merge: None,
        forecast: None,
        forecast_hidden: None,
        year: today.year(),
        last12: None,
        last12_days: 0,
        monthly_avg: None,
        goal,
        goal_status: "none".into(),
        goal_ym: None,
        oa_latest: None,
        oa_per_month: None,
    }
}

/// Pure core of [`build_trend`]. `records` are the citing works as (date, precision).
pub fn trend_from(
    points: &[(DateTime<Utc>, i64)],
    records: &[(String, String)],
    goal: Option<i64>,
    now: DateTime<Utc>,
    today: NaiveDate,
) -> Trend {
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return empty_trend(today, goal);
    };
    let curve = historical_curve(records, first.0);
    let (hist, k) = scale_curve(&curve, first.1);
    let merge = Some(Merge {
        t: first.0.timestamp_millis(),
        value: first.1,
        raw_last: curve.raw_last,
        k,
        year_only: curve.year_only,
    });
    let snaps: Vec<(i64, i64)> = points.iter().map(|(t, v)| (t.timestamp_millis(), *v)).collect();

    // the merged line: the scaled history up to the first snapshot, then the snapshots
    let mut line: Vec<(i64, f64)> = hist.iter().copied().filter(|p| p.0 < snaps[0].0).collect();
    line.extend(snaps.iter().map(|(t, v)| (*t, *v as f64)));
    let now_ms = now.timestamp_millis();
    let a_ms = now_ms - 365 * MS_DAY;
    let (last12, last12_days, monthly_avg) = match (line_value_at(&line, now_ms), line_value_at(&line, a_ms)) {
        (Some(b), Some(a)) => {
            let span_ms = now_ms - line[0].0.max(a_ms);
            let days = (span_ms / MS_DAY).clamp(0, 365);
            let gained = (b - a).round() as i64;
            (Some(gained), days, if days >= 28 { Some(gained as f64 / (days as f64 / 30.4375)) } else { None })
        }
        _ => (None, 0, None),
    };

    let outcome = forecast_band(points, now, today);
    let (forecast, forecast_hidden) = match outcome {
        ForecastOutcome::Shown(b) => (Some(b), None),
        ForecastOutcome::Hidden { have, need } => (None, Some((have, need))),
    };
    let current = last.1;
    let (goal_status, goal_ym) = match goal {
        None => ("none", None),
        Some(g) => match goal_eta(current, g, forecast.as_ref().map(|f| f.per_day), today) {
            GoalEta::Reached => ("reached", None),
            GoalEta::Month(y, m) => ("month", Some(month_key(y, m))),
            GoalEta::Unknown => ("unknown", None),
        },
    };
    Trend {
        has_data: true,
        hist,
        snaps,
        merge,
        forecast,
        forecast_hidden,
        year: today.year(),
        last12,
        last12_days,
        monthly_avg,
        goal,
        goal_status: goal_status.into(),
        goal_ym,
        oa_latest: None,
        oa_per_month: None,
    }
}

pub fn build_trend(conn: &Connection, now: DateTime<Utc>, today: NaiveDate) -> Result<Trend> {
    let series = db::time_series(conn)?;
    let points: Vec<(DateTime<Utc>, i64)> =
        series.iter().filter_map(|p| db::parse_ts(&p.t).map(|t| (t, p.cit_all))).collect();
    let records: Vec<(String, String)> =
        db::citing_works(conn)?.into_iter().map(|c| (c.date, c.precision)).collect();
    let goal = db::get_state(conn, db::STATE_GOAL)?.and_then(|s| s.parse::<i64>().ok()).filter(|g| *g > 0);
    let mut tr = trend_from(&points, &records, goal, now, today);
    tr.oa_latest = db::latest_citing_day(conn)?;
    tr.oa_per_month = oa_monthly_rate(&db::history_series(conn, "month")?.points, tr.oa_latest.as_deref(), 6);
    Ok(tr)
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
    /// How far the Scholar snapshot history reaches: "snapshots" (the whole window), "snapshots_partial"
    /// (less than the window: `covered_days`) or "none" (no earlier snapshot to compare with).
    pub basis: String,
    /// Days the snapshot history covers, at most `days`.
    pub covered_days: i64,
    /// Net change of the total over those days (Scholar).
    pub snap_total: i64,
    /// Articles that gained most according to the Scholar snapshots.
    pub items: Vec<Riser>,
    /// Articles with the most citing works dated inside the window (OpenAlex), listed apart from the Scholar ones.
    pub oa_items: Vec<Riser>,
    /// Newest day-dated citing work in the database: OpenAlex adds new papers with a lag of weeks.
    pub oa_latest: Option<String>,
}

/// The snapshot a window is compared with: its article counts, the days between it and the newest snapshot
/// (at most the window) and the change of the total.
pub struct RiserRef {
    pub map: HashMap<String, i64>,
    pub covered_days: i64,
    pub total_change: i64,
    pub complete: bool,
}

/// Pure core of [`build_risers`]: Scholar gains first (whole or partial window), OpenAlex counts apart.
pub fn risers_from(
    days: i64,
    latest: &[(String, String, i64)],
    reference: Option<RiserRef>,
    oa_counts: &HashMap<String, i64>,
    oa_latest: Option<String>,
) -> Risers {
    let oa_items = top_risers(latest, &|k, _| oa_counts.get(k).copied().unwrap_or(0), 5);
    match reference {
        Some(r) => Risers {
            days,
            basis: if r.complete { "snapshots" } else { "snapshots_partial" }.into(),
            covered_days: r.covered_days,
            snap_total: r.total_change,
            items: top_risers(latest, &|k, c| c - r.map.get(k).copied().unwrap_or(0), 5),
            oa_items,
            oa_latest,
        },
        None => Risers { days, basis: "none".into(), covered_days: 0, snap_total: 0, items: Vec::new(), oa_items, oa_latest },
    }
}

pub fn build_risers(conn: &Connection, now: DateTime<Utc>, days: i64) -> Result<Risers> {
    let latest = db::latest_article_citations(conn)?;
    let oa_latest = db::latest_citing_day(conn)?;
    if latest.is_empty() {
        return Ok(risers_from(days, &latest, None, &HashMap::new(), oa_latest));
    }
    let cutoff = now - Duration::days(days);
    let counts = db::backfill_counts_since(conn, cutoff.date_naive())?;
    let newest = db::latest_snapshot(conn)?;
    let reference = match (db::snapshot_at_or_before(conn, cutoff)?, newest) {
        (Some(r), Some(n)) => Some(RiserRef { map: db::article_map_at(conn, r.id)?, covered_days: days, total_change: n.cit_all - r.cit_all, complete: true }),
        (None, Some(n)) => match db::earliest_snapshot(conn)? {
            Some(f) if f.id != n.id => {
                let covered = match (db::parse_ts(&f.fetched_at), db::parse_ts(&n.fetched_at)) {
                    (Some(a), Some(b)) => (b - a).num_days().max(0),
                    _ => 0,
                };
                Some(RiserRef { map: db::article_map_at(conn, f.id)?, covered_days: covered, total_change: n.cit_all - f.cit_all, complete: false })
            }
            _ => None,
        },
        _ => None,
    };
    Ok(risers_from(days, &latest, reference, &counts, oa_latest))
}

/// Average citing works per month (OpenAlex) over the last `n` complete months that OpenAlex already
/// covers. The months after the newest citing work, and that month itself, are the indexing lag: they are
/// never counted as zero. `months` are (`YYYY-MM`, count); gaps inside the covered span count as zero.
pub fn oa_monthly_rate(months: &[(String, i64)], latest_day: Option<&str>, n: usize) -> Option<f64> {
    let latest = latest_day?;
    if latest.len() < 7 || months.is_empty() || n == 0 {
        return None;
    }
    let (ly, lm): (i32, u32) = (latest[0..4].parse().ok()?, latest[5..7].parse().ok()?);
    // last month fully covered: the one before the newest citing work's month
    let (mut y, mut m) = if lm == 1 { (ly - 1, 12) } else { (ly, lm - 1) };
    let first = months.first()?.0.as_str();
    let map: HashMap<&str, i64> = months.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    let (mut sum, mut cnt) = (0i64, 0usize);
    while cnt < n {
        let key = month_key(y, m);
        if key.as_str() < first {
            break;
        }
        sum += map.get(key.as_str()).copied().unwrap_or(0);
        cnt += 1;
        if m == 1 {
            y -= 1;
            m = 12;
        } else {
            m -= 1;
        }
    }
    (cnt > 0).then(|| sum as f64 / cnt as f64)
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
    /// Every article still below the threshold, closest first (at most [`MAX_CANDIDATES`]).
    pub candidates: Vec<Pick>,
}

/// Length of the full candidate list of a next step.
pub const MAX_CANDIDATES: usize = 12;

fn next_step(arts: &[(String, String, i64)], current: i64, threshold: i64, need_papers: usize) -> NextStep {
    let mut below: Vec<&(String, String, i64)> = arts.iter().filter(|a| a.2 < threshold).collect();
    below.sort_by(|a, b| b.2.cmp(&a.2).then(a.1.cmp(&b.1)));
    let picks: Vec<Pick> = below
        .iter()
        .take(need_papers)
        .map(|a| Pick { key: a.0.clone(), title: a.1.clone(), citations: a.2, needs: threshold - a.2 })
        .collect();
    let total_needed = picks.iter().map(|p| p.needs).sum();
    let candidates: Vec<Pick> = below
        .iter()
        .take(MAX_CANDIDATES)
        .map(|a| Pick { key: a.0.clone(), title: a.1.clone(), citations: a.2, needs: threshold - a.2 })
        .collect();
    NextStep {
        candidates,
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
/// through OpenAlex.
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

// ------------------------------------------------------------ per article, stacked

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StackSeries {
    pub key: String,
    pub title: String,
    /// Citing works per month, aligned with `Stacked::months`.
    pub counts: Vec<i64>,
    pub total: i64,
}

/// Citing works per month, stacked by the article they cite: the top articles one by one, the
/// rest together as "other". Dates are those of the citing works (OpenAlex).
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct Stacked {
    /// `YYYY-MM`, continuous from the first to the last month with data.
    pub months: Vec<String>,
    pub series: Vec<StackSeries>,
    /// The articles beyond the top ones, added up per month (empty when there are none).
    pub other: Vec<i64>,
    pub other_articles: usize,
    /// Records dated by year only (they have no month).
    pub year_only: usize,
    /// Records placed on the chart.
    pub placed: usize,
}

fn ym_next(key: &str) -> String {
    let (y, m): (i32, u32) = (key[0..4].parse().unwrap_or(2000), key[5..7].parse().unwrap_or(1));
    if m == 12 {
        month_key(y + 1, 1)
    } else {
        month_key(y, m + 1)
    }
}

pub fn stacked_by_article(rows: &[db::CitingRow], top_n: usize) -> Stacked {
    let mut out = Stacked::default();
    // (article key, title) -> month -> count
    let mut per: HashMap<String, (String, std::collections::BTreeMap<String, i64>)> = HashMap::new();
    let (mut lo, mut hi): (Option<String>, Option<String>) = (None, None);
    for r in rows {
        if r.precision == "year" || r.date.len() < 7 {
            out.year_only += 1;
            continue;
        }
        let m = r.date[..7].to_string();
        out.placed += 1;
        if lo.as_ref().is_none_or(|l| m < *l) {
            lo = Some(m.clone());
        }
        if hi.as_ref().is_none_or(|h| m > *h) {
            hi = Some(m.clone());
        }
        let e = per.entry(r.article_key.clone()).or_insert_with(|| (r.article_title.clone(), Default::default()));
        *e.1.entry(m).or_insert(0) += 1;
    }
    let (Some(lo), Some(hi)) = (lo, hi) else {
        return out;
    };
    let mut cur = lo;
    while cur <= hi && out.months.len() < 1200 {
        out.months.push(cur.clone());
        cur = ym_next(&cur);
    }
    let mut arts: Vec<(String, String, std::collections::BTreeMap<String, i64>, i64)> = per
        .into_iter()
        .map(|(k, (t, m))| {
            let total = m.values().sum();
            (k, t, m, total)
        })
        .collect();
    arts.sort_by(|a, b| b.3.cmp(&a.3).then(a.1.cmp(&b.1)).then(a.0.cmp(&b.0)));
    let n = out.months.len();
    let mut other = vec![0i64; n];
    for (i, (k, t, m, total)) in arts.into_iter().enumerate() {
        let counts: Vec<i64> = out.months.iter().map(|ym| m.get(ym).copied().unwrap_or(0)).collect();
        if i < top_n {
            out.series.push(StackSeries { key: k, title: t, counts, total });
        } else {
            out.other_articles += 1;
            for (j, c) in counts.iter().enumerate() {
                other[j] += c;
            }
        }
    }
    if out.other_articles > 0 {
        out.other = other;
    }
    out
}

// ------------------------------------------------------------------ calendar

/// Citing works by publication day. Works dated by month or year only are counted apart: they
/// never land on a day of the calendar.
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct Calendar {
    /// (`YYYY-MM-DD`, distinct citing works that day), ascending.
    pub days: Vec<(String, i64)>,
    /// Years that have at least one dated day, ascending.
    pub years: Vec<i32>,
    pub month_only: usize,
    pub year_only: usize,
    /// Distinct citing works (dated or not).
    pub works: usize,
    pub max: i64,
}

pub fn calendar(rows: &[db::CitingRow]) -> Calendar {
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut days: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    let mut c = Calendar::default();
    for r in rows {
        if !seen.insert(r.citing_id.as_str()) {
            continue;
        }
        c.works += 1;
        match r.precision.as_str() {
            "day" if r.date.len() >= 10 => *days.entry(r.date[..10].to_string()).or_insert(0) += 1,
            "month" => c.month_only += 1,
            _ => c.year_only += 1,
        }
    }
    let mut years: Vec<i32> = days.keys().filter_map(|d| d[0..4].parse().ok()).collect();
    years.dedup();
    c.years = years;
    c.max = days.values().copied().max().unwrap_or(0);
    c.days = days.into_iter().collect();
    c
}

// -------------------------------------------------------------- h / i10 history

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct IndexPoint {
    pub t: String,
    pub h: i64,
    pub i10: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct IndexChange {
    pub t: String,
    /// "h" | "i10"
    pub metric: String,
    pub from: i64,
    pub to: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct IndexHistory {
    /// The first snapshot, every snapshot where h or i10 differs from the one before, and the last.
    pub points: Vec<IndexPoint>,
    pub changes: Vec<IndexChange>,
}

/// h-index and i10-index (all time, as Scholar reports them) over the snapshots.
pub fn index_history(series: &[db::SeriesPoint]) -> IndexHistory {
    let mut out = IndexHistory::default();
    let n = series.len();
    for (i, p) in series.iter().enumerate() {
        let prev = if i > 0 { Some(&series[i - 1]) } else { None };
        let changed = prev.is_some_and(|q| q.h_all != p.h_all || q.i10_all != p.i10_all);
        if i == 0 || changed || i + 1 == n {
            out.points.push(IndexPoint { t: p.t.clone(), h: p.h_all, i10: p.i10_all });
        }
        if let Some(q) = prev {
            if q.h_all != p.h_all {
                out.changes.push(IndexChange { t: p.t.clone(), metric: "h".into(), from: q.h_all, to: p.h_all });
            }
            if q.i10_all != p.i10_all {
                out.changes.push(IndexChange { t: p.t.clone(), metric: "i10".into(), from: q.i10_all, to: p.i10_all });
            }
        }
    }
    out
}

// ------------------------------------------------------------- visit summary

pub const STATE_LAST_VISIT: &str = "last_visit_at";
pub const STATE_VISIT_BASELINE: &str = "visit_baseline_at";

/// The window is shown: what the summary compares against is the time it was last seen (when it was
/// hidden), and the current moment becomes the new "last visit".
pub fn begin_visit(conn: &Connection, now: DateTime<Utc>) -> Result<()> {
    match db::get_state(conn, STATE_LAST_VISIT)?.filter(|s| !s.is_empty()) {
        Some(prev) => db::set_state(conn, STATE_VISIT_BASELINE, &prev)?,
        None => db::delete_state(conn, STATE_VISIT_BASELINE)?,
    }
    db::set_state(conn, STATE_LAST_VISIT, &db::ts(now))
}

/// The window is hidden: the visit ends now.
pub fn end_visit(conn: &Connection, now: DateTime<Utc>) -> Result<()> {
    db::set_state(conn, STATE_LAST_VISIT, &db::ts(now))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapView {
    pub cit: i64,
    pub h: i64,
    pub i10: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TopArticle {
    pub key: String,
    pub title: String,
    pub gain: i64,
}

#[derive(Debug, Clone)]
pub struct VisitInput {
    /// The snapshot the last visit saw; `None` without a previous visit.
    pub baseline: Option<SnapView>,
    pub latest: SnapView,
    pub top: Option<TopArticle>,
    /// Target of the next h and the citations still missing (`None`: needs new articles).
    pub next_h_target: i64,
    pub next_h_needs: Option<i64>,
    pub last_gain_at: Option<String>,
    pub baseline_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct VisitSummary {
    /// "first" | "threshold" | "gain" | "loss" | "closeness" | "calm"
    pub kind: String,
    pub baseline_at: Option<String>,
    pub total: i64,
    /// Net change in total citations since the baseline.
    pub net: i64,
    pub h: i64,
    pub i10: i64,
    /// "h" | "i10" for kind "threshold".
    pub metric: Option<String>,
    /// The new value of that index.
    pub value: i64,
    pub top: Option<TopArticle>,
    pub next_h_target: i64,
    pub next_h_needs: Option<i64>,
    pub last_gain_at: Option<String>,
}

/// A closeness hint is only worth a sentence when this few citations are missing.
const CLOSE_NEEDS: i64 = 3;

/// One sentence's worth of facts, in priority order: (a) an index crossed a threshold, (b) new
/// citations and the article behind most of them, (c) a net decrease, (d) closeness to the next
/// h; otherwise the calm state (nothing changed).
pub fn summarize_visit(i: &VisitInput) -> VisitSummary {
    let mut s = VisitSummary {
        kind: "calm".into(),
        baseline_at: i.baseline_at.clone(),
        total: i.latest.cit,
        net: 0,
        h: i.latest.h,
        i10: i.latest.i10,
        metric: None,
        value: 0,
        top: None,
        next_h_target: i.next_h_target,
        next_h_needs: i.next_h_needs,
        last_gain_at: i.last_gain_at.clone(),
    };
    let Some(b) = i.baseline else {
        s.kind = "first".into();
        return s;
    };
    s.net = i.latest.cit - b.cit;
    if i.latest.h > b.h {
        s.kind = "threshold".into();
        s.metric = Some("h".into());
        s.value = i.latest.h;
    } else if i.latest.i10 > b.i10 {
        s.kind = "threshold".into();
        s.metric = Some("i10".into());
        s.value = i.latest.i10;
    } else if s.net > 0 {
        s.kind = "gain".into();
        s.top = i.top.clone();
    } else if s.net < 0 {
        s.kind = "loss".into();
    } else if i.next_h_needs.is_some_and(|n| n <= CLOSE_NEEDS) {
        s.kind = "closeness".into();
    }
    s
}

/// Time of the last snapshot whose total was higher than the one before.
pub fn last_gain_time(series: &[db::SeriesPoint]) -> Option<String> {
    series.windows(2).rev().find(|w| w[1].cit_all > w[0].cit_all).map(|w| w[1].t.clone())
}

pub fn build_visit_summary(conn: &Connection) -> Result<Option<VisitSummary>> {
    let Some(latest) = db::latest_snapshot(conn)? else {
        return Ok(None);
    };
    let baseline_at = db::get_state(conn, STATE_VISIT_BASELINE)?.filter(|s| !s.is_empty());
    let view = |s: &db::Snapshot| SnapView { cit: s.cit_all, h: s.h_all, i10: s.i10_all };
    // The snapshot the last visit saw. A visit that began before any snapshot existed uses the
    // first snapshot: its level is what Scholar already counted, not growth.
    let base_snap = match baseline_at.as_deref().and_then(db::parse_ts) {
        Some(t) => match db::snapshot_at_or_before(conn, t)? {
            Some(s) => Some(s),
            None => db::earliest_snapshot(conn)?,
        },
        None => None,
    };
    let arts = db::latest_article_citations(conn)?;
    let next = h_next_step(&arts);
    let top = match &base_snap {
        Some(b) if b.id != latest.id => {
            let map = db::article_map_at(conn, b.id)?;
            top_risers(&arts, &|k, c| c - map.get(k).copied().unwrap_or(0), 1)
                .into_iter()
                .next()
                .map(|r| TopArticle { key: r.key, title: r.title, gain: r.gain })
        }
        _ => None,
    };
    let series = db::time_series(conn)?;
    let input = VisitInput {
        baseline: base_snap.as_ref().map(view),
        latest: view(&latest),
        top,
        next_h_target: next.target,
        next_h_needs: if next.new_papers == 0 { Some(next.total_needed) } else { None },
        last_gain_at: last_gain_time(&series),
        baseline_at,
    };
    Ok(Some(summarize_visit(&input)))
}

// ------------------------------------------------------------------- analysis

/// Everything the Analysis page and the PDF report draw, from one place so both show the same numbers.
#[derive(Debug, Clone, Serialize)]
pub struct Analysis {
    pub trend: Trend,
    pub history_day: db::HistorySeries,
    pub history_month: db::HistorySeries,
    pub history_year: db::HistorySeries,
    /// Scholar's own citations per year (latest snapshot).
    pub yearly: Vec<(i32, i64)>,
    pub stacked: Stacked,
    pub indices: IndexHistory,
    pub next: Option<NextSteps>,
    pub calendar: Calendar,
    pub dist: Distribution,
}

/// `range`: only the distribution of citing works is cut to it (the charts of the page are cut by
/// the page itself, with the same range).
pub fn build_analysis(
    conn: &Connection,
    now: DateTime<Utc>,
    today: NaiveDate,
    range: Option<(NaiveDate, NaiveDate)>,
) -> Result<Analysis> {
    let rows = db::citing_works(conn)?;
    let yearly = match db::latest_snapshot(conn)? {
        Some(s) => db::yearly_for(conn, s.id)?,
        None => Vec::new(),
    };
    let dist_rows = db::distinct_citing_works(conn)?;
    let dist = match range {
        Some((from, to)) => distribution(
            &dist_rows
                .into_iter()
                .filter(|r| {
                    let precision = match r.date.len() {
                        10.. => "day",
                        7..=9 => "month",
                        _ => "year",
                    };
                    crate::report::in_range(&r.date, precision, from, to)
                })
                .collect::<Vec<_>>(),
        ),
        None => distribution(&dist_rows),
    };
    Ok(Analysis {
        trend: build_trend(conn, now, today)?,
        history_day: db::history_series(conn, "day")?,
        history_month: db::history_series(conn, "month")?,
        history_year: db::history_series(conn, "year")?,
        yearly,
        stacked: stacked_by_article(&rows, 5),
        indices: index_history(&db::time_series(conn)?),
        next: build_next_steps(conn)?,
        calendar: calendar(&rows),
        dist,
    })
}

// ------------------------------------------------------- calendar: days and details

/// What happened on one calendar day (local time): the changes Scholar showed that day and the citing
/// works (OpenAlex) whose exact publication date it is.
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct DayAgg {
    pub date: String,
    /// Citations Scholar added that day (sum of the increases).
    pub scholar_up: i64,
    /// Citations Scholar took away that day (sum of the decreases, as a positive number).
    pub scholar_down: i64,
    pub scholar_events: usize,
    /// Distinct citing works published that day.
    pub openalex: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct CalendarDays {
    /// Days with any activity inside the range, ascending.
    pub days: Vec<DayAgg>,
    /// Citing works dated by month or year only that touch the range: never put on a day.
    pub month_only: usize,
    pub year_only: usize,
}

fn wants_scholar(source: &str) -> bool {
    source != "openalex"
}
fn wants_openalex(source: &str) -> bool {
    source != "scholar"
}
fn article_ok(filter: &str, key: &str) -> bool {
    filter.is_empty() || filter == key
}

/// Per-day aggregates of both sources inside `[from, to]` (`YYYY-MM-DD`, inclusive). `source` is "all",
/// "scholar" or "openalex"; `article` an article key or empty. `to_day` turns a stored UTC timestamp into
/// the local calendar day. Scholar changes come from consecutive snapshots, so the very first snapshot
/// (everything Scholar already counted) is never a day on which anything was "cited".
pub fn calendar_days(
    changes: &[db::ScholarChange],
    rows: &[db::CitingRow],
    from: &str,
    to: &str,
    source: &str,
    article: &str,
    to_day: &dyn Fn(&str) -> String,
) -> CalendarDays {
    let mut map: std::collections::BTreeMap<String, DayAgg> = std::collections::BTreeMap::new();
    let mut out = CalendarDays::default();
    if wants_scholar(source) {
        for c in changes.iter().filter(|c| article_ok(article, &c.article_key)) {
            let day = to_day(&c.at);
            if day.as_str() < from || day.as_str() > to {
                continue;
            }
            let e = map.entry(day.clone()).or_insert_with(|| DayAgg { date: day, ..DayAgg::default() });
            if c.delta > 0 {
                e.scholar_up += c.delta;
            } else {
                e.scholar_down += -c.delta;
            }
            e.scholar_events += 1;
        }
    }
    if wants_openalex(source) {
        let mut per_day: HashMap<String, std::collections::HashSet<&str>> = HashMap::new();
        let (mut mo, mut yo): (std::collections::HashSet<&str>, std::collections::HashSet<&str>) = Default::default();
        for r in rows.iter().filter(|r| article_ok(article, &r.article_key)) {
            match r.precision.as_str() {
                "day" if r.date.len() >= 10 => {
                    let d = &r.date[..10];
                    if d >= from && d <= to {
                        per_day.entry(d.to_string()).or_default().insert(r.citing_id.as_str());
                    }
                }
                "month" if r.date.len() >= 7 => {
                    if r.date[..7] >= from[..7.min(from.len())] && r.date[..7] <= to[..7.min(to.len())] {
                        mo.insert(r.citing_id.as_str());
                    }
                }
                "year" if r.date.len() >= 4 && r.date[..4] >= from[..4.min(from.len())] && r.date[..4] <= to[..4.min(to.len())] => {
                    yo.insert(r.citing_id.as_str());
                }
                _ => {}
            }
        }
        for (day, set) in per_day {
            let e = map.entry(day.clone()).or_insert_with(|| DayAgg { date: day, ..DayAgg::default() });
            e.openalex = set.len();
        }
        out.month_only = mo.len();
        out.year_only = yo.len();
    }
    out.days = map.into_values().collect();
    out
}

/// A Scholar increase is linked to citing works published this long before / after it (days).
pub const MATCH_BEFORE_DAYS: i64 = 45;
pub const MATCH_AFTER_DAYS: i64 = 15;

/// An increase Scholar showed: `n` new citations of one article on one day.
#[derive(Debug, Clone, PartialEq)]
pub struct Ev {
    pub article: String,
    pub day: NaiveDate,
    pub n: i64,
}

/// A citing work with an exact publication day.
#[derive(Debug, Clone, PartialEq)]
pub struct Wk {
    pub article: String,
    pub id: String,
    pub day: NaiveDate,
}

/// Probable attribution of Scholar increases to citing works: for each article, a work is a candidate for
/// an event when its publication date lies within `-MATCH_BEFORE_DAYS ..= +MATCH_AFTER_DAYS` of the event
/// day (OpenAlex indexes it later, so it can be dated before the event). Candidate pairs are taken closest
/// first (ties: earlier event, then work id), every work at most once and every event at most `n` times.
/// Returns, per event index, the indices of its works. A guess, never a certainty; decreases are not events.
pub fn attribute_events(events: &[Ev], works: &[Wk]) -> HashMap<usize, Vec<usize>> {
    let mut pairs: Vec<(i64, NaiveDate, usize, &str, usize)> = Vec::new();
    for (ei, e) in events.iter().enumerate().filter(|(_, e)| e.n > 0) {
        for (wi, w) in works.iter().enumerate() {
            if w.article != e.article {
                continue;
            }
            let off = (w.day - e.day).num_days();
            if (-MATCH_BEFORE_DAYS..=MATCH_AFTER_DAYS).contains(&off) {
                pairs.push((off.abs(), e.day, ei, w.id.as_str(), wi));
            }
        }
    }
    pairs.sort();
    let mut taken = vec![false; works.len()];
    let mut left: Vec<i64> = events.iter().map(|e| e.n).collect();
    let mut out: HashMap<usize, Vec<usize>> = HashMap::new();
    for (_, _, ei, _, wi) in pairs {
        if taken[wi] || left[ei] <= 0 {
            continue;
        }
        taken[wi] = true;
        left[ei] -= 1;
        out.entry(ei).or_default().push(wi);
    }
    for v in out.values_mut() {
        v.sort_unstable();
    }
    out
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ArtRef {
    pub key: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct OaWork {
    pub citing_id: String,
    pub title: String,
    pub venue: String,
    pub doi: String,
    pub url: String,
    pub date: String,
    pub articles: Vec<ArtRef>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ScholarEvent {
    pub article_key: String,
    pub title: String,
    pub cites_ids: String,
    pub at: String,
    pub old: i64,
    pub new: i64,
    pub delta: i64,
    /// Citing works that probably are these new citations ("probable": a guess from the dates).
    pub probable: Vec<OaWork>,
    pub confidence: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct DayDetail {
    pub date: String,
    pub scholar: Vec<ScholarEvent>,
    pub openalex: Vec<OaWork>,
}

fn oa_work(r: &db::CitingRow, all: &[&db::CitingRow]) -> OaWork {
    OaWork {
        citing_id: r.citing_id.clone(),
        title: r.title.clone(),
        venue: r.venue.clone(),
        doi: r.doi.clone(),
        url: r.url.clone(),
        date: r.date.clone(),
        articles: all.iter().map(|x| ArtRef { key: x.article_key.clone(), title: x.article_title.clone() }).collect(),
    }
}

/// Everything of both sources inside `[from, to]`, grouped by day (ascending), with the probable
/// OpenAlex works behind each Scholar increase. Links are derived every time from the data as it is now,
/// so they fill themselves as OpenAlex catches up.
pub fn day_detail(
    changes: &[db::ScholarChange],
    rows: &[db::CitingRow],
    from: &str,
    to: &str,
    source: &str,
    article: &str,
    to_day: &dyn Fn(&str) -> String,
) -> Vec<DayDetail> {
    let parse = |s: &str| NaiveDate::parse_from_str(&s[..10.min(s.len())], "%Y-%m-%d").ok();
    // works with an exact day, grouped per citing work (the same work may cite several of the articles)
    let mut by_id: std::collections::BTreeMap<&str, Vec<&db::CitingRow>> = std::collections::BTreeMap::new();
    for r in rows.iter().filter(|r| r.precision == "day" && r.date.len() >= 10) {
        by_id.entry(r.citing_id.as_str()).or_default().push(r);
    }
    // attribution over all increases and all (article, work) pairs, never limited by the filters
    let mut events: Vec<Ev> = Vec::new();
    let mut ev_change: Vec<usize> = Vec::new();
    for (ci, c) in changes.iter().enumerate().filter(|(_, c)| c.delta > 0) {
        if let Some(day) = parse(&to_day(&c.at)) {
            events.push(Ev { article: c.article_key.clone(), day, n: c.delta });
            ev_change.push(ci);
        }
    }
    let mut works: Vec<Wk> = Vec::new();
    let mut work_row: Vec<&db::CitingRow> = Vec::new();
    for r in rows.iter().filter(|r| r.precision == "day" && r.date.len() >= 10) {
        if let Some(day) = parse(&r.date) {
            works.push(Wk { article: r.article_key.clone(), id: r.citing_id.clone(), day });
            work_row.push(r);
        }
    }
    let links = attribute_events(&events, &works);
    let mut link_of: HashMap<usize, Vec<OaWork>> = HashMap::new();
    for (ei, wis) in links {
        let items = wis.into_iter().map(|wi| oa_work(work_row[wi], &[work_row[wi]])).collect();
        link_of.insert(ev_change[ei], items);
    }

    let mut days: std::collections::BTreeMap<String, DayDetail> = std::collections::BTreeMap::new();
    if wants_scholar(source) {
        for (ci, c) in changes.iter().enumerate().filter(|(_, c)| article_ok(article, &c.article_key)) {
            let day = to_day(&c.at);
            if day.as_str() < from || day.as_str() > to {
                continue;
            }
            let d = days.entry(day.clone()).or_insert_with(|| DayDetail { date: day, ..DayDetail::default() });
            d.scholar.push(ScholarEvent {
                article_key: c.article_key.clone(),
                title: c.title.clone(),
                cites_ids: c.cites_ids.clone(),
                at: c.at.clone(),
                old: c.old,
                new: c.new,
                delta: c.delta,
                probable: link_of.remove(&ci).unwrap_or_default(),
                confidence: "probable".into(),
            });
        }
    }
    if wants_openalex(source) {
        let mut seen: std::collections::HashSet<(String, &str)> = std::collections::HashSet::new();
        for (id, group) in &by_id {
            let group: Vec<&db::CitingRow> = group.iter().copied().filter(|r| article_ok(article, &r.article_key)).collect();
            let Some(first) = group.first() else { continue };
            let day = first.date[..10].to_string();
            if day.as_str() < from || day.as_str() > to || !seen.insert((day.clone(), *id)) {
                continue;
            }
            let d = days.entry(day.clone()).or_insert_with(|| DayDetail { date: day, ..DayDetail::default() });
            d.openalex.push(oa_work(first, &group));
        }
    }
    for d in days.values_mut() {
        d.scholar.sort_by(|a, b| b.delta.cmp(&a.delta).then(a.title.cmp(&b.title)));
        d.openalex.sort_by(|a, b| a.title.cmp(&b.title).then(a.citing_id.cmp(&b.citing_id)));
    }
    days.into_values().collect()
}

/// Local calendar day of a stored UTC timestamp.
pub fn local_day(ts: &str) -> String {
    db::parse_ts(ts)
        .map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

pub fn build_calendar_days(conn: &Connection, from: &str, to: &str, source: &str, article: &str) -> Result<CalendarDays> {
    Ok(calendar_days(&db::scholar_changes(conn)?, &db::citing_works(conn)?, from, to, source, article, &local_day))
}

pub fn build_day_detail(conn: &Connection, from: &str, to: &str, source: &str, article: &str) -> Result<Vec<DayDetail>> {
    Ok(day_detail(&db::scholar_changes(conn)?, &db::citing_works(conn)?, from, to, source, article, &local_day))
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

    fn art(k: &str, c: i64) -> (String, String, i64) {
        (k.to_string(), format!("Title {k}"), c)
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
            ..Default::default()
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

    // ------------------------------------------------------------ test helpers

    fn sp(tm: &str, cit: i64, h: i64, i10: i64) -> db::SeriesPoint {
        db::SeriesPoint { t: tm.into(), cit_all: cit, cit_since: cit, h_all: h, h_since: h, i10_all: i10, i10_since: i10 }
    }

    fn rec(date: &str, precision: &str) -> (String, String) {
        (date.to_string(), precision.to_string())
    }

    fn crow(article: &str, id: &str, date: &str, precision: &str) -> db::CitingRow {
        db::CitingRow {
            article_key: article.into(),
            article_title: format!("Article {article}"),
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
            ..Default::default()
        }
    }

    /// Snapshots every `step` days from `start`, totals `v0 + per_day * days`.
    fn line(start: &str, n: i64, step: i64, v0: i64, per_day: f64) -> Vec<(DateTime<Utc>, i64)> {
        (0..n)
            .map(|i| {
                let tm = t(start) + Duration::days(i * step);
                (tm, v0 + (per_day * (i * step) as f64).round() as i64)
            })
            .collect()
    }

    // ------------------------------------------------ trend: merge scaling

    #[test]
    fn merge_scales_the_history_to_the_first_snapshot_and_keeps_its_shape() {
        let first = t("2026-03-15T10:00:00Z");
        let records = vec![
            rec("2025-11-05", "day"),
            rec("2025-12-20", "day"),
            rec("2025-12-21", "day"),
            rec("2026-01", "month"),
            rec("2026-02-10", "day"),
            rec("2026-02-11", "day"),
            rec("2026-03-02", "day"), // same month as the snapshot, before its day: counts
            rec("2026-03-20", "day"), // after the snapshot: not history
            rec("2026-03", "month"),  // the snapshot's own month is ambiguous: left out
            rec("2024", "year"),      // year only: never placed
        ];
        let curve = historical_curve(&records, first);
        assert_eq!(curve.year_only, 1);
        assert_eq!(curve.raw_last, 7);
        assert_eq!(curve.points.first().map(|p| p.1), Some(0), "starts from a zero origin");
        assert_eq!(curve.points.last().map(|p| p.0), Some(first.timestamp_millis()));
        let (scaled, k) = scale_curve(&curve, 100);
        assert!((k - 100.0 / 7.0).abs() < 1e-9);
        // no gap and no jump: the last point is exactly the first snapshot value
        assert_eq!(scaled.last().map(|p| p.1), Some(100.0));
        // proportional: every raw ratio is kept
        for (i, (_, v)) in scaled.iter().enumerate() {
            let raw = curve.points[i].1 as f64;
            assert!((v - raw / 7.0 * 100.0).abs() < 0.01, "point {i}: {v} vs {raw}");
        }
        // the curve never decreases
        assert!(scaled.windows(2).all(|w| w[1].1 >= w[0].1));
        // scaling to the raw value itself changes nothing
        let (same, _) = scale_curve(&curve, 7);
        assert_eq!(same.iter().map(|p| p.1.round() as i64).collect::<Vec<_>>(), curve.points.iter().map(|p| p.1).collect::<Vec<_>>());
    }

    #[test]
    fn merge_without_usable_history_draws_no_curve() {
        let first = t("2026-03-15T10:00:00Z");
        let curve = historical_curve(&[rec("2025", "year"), rec("2026-03-20", "day")], first);
        assert_eq!(curve.raw_last, 0);
        assert!(scale_curve(&curve, 100).0.is_empty());
        let none = historical_curve(&[], first);
        assert!(none.points.is_empty() && none.year_only == 0);
        // only same-month days: a short two-point curve
        let tail = historical_curve(&[rec("2026-03-01", "day"), rec("2026-03-05", "day")], first);
        assert_eq!(tail.raw_last, 2);
        assert_eq!(tail.points.len(), 2);
    }

    #[test]
    fn line_value_interpolates_and_clamps() {
        let pts = vec![(1000i64, 10.0), (2000, 20.0), (4000, 40.0)];
        assert_eq!(line_value_at(&pts, 500), Some(10.0));
        assert_eq!(line_value_at(&pts, 1500), Some(15.0));
        assert_eq!(line_value_at(&pts, 3000), Some(30.0));
        assert_eq!(line_value_at(&pts, 9000), Some(40.0));
        assert_eq!(line_value_at(&[], 1), None);
    }

    #[test]
    fn trend_is_one_continuous_line() {
        let snaps = line("2026-02-01T08:00:00Z", 30, 5, 400, 0.5); // 145 days
        let now = snaps.last().expect("last").0;
        let records: Vec<(String, String)> =
            (0..300).map(|i| rec(&format!("2025-{:02}-{:02}", 1 + i % 12, 1 + i % 28), "day")).collect();
        let tr = trend_from(&snaps, &records, Some(1000), now, now.date_naive());
        assert!(tr.has_data);
        let merge = tr.merge.as_ref().expect("merge");
        assert_eq!(merge.value, 400);
        assert_eq!(tr.hist.last().map(|p| p.1), Some(400.0));
        assert_eq!(tr.hist.last().map(|p| p.0), Some(tr.snaps[0].0), "history ends where the snapshots start");
        assert_eq!(tr.snaps[0], (merge.t, 400));
        assert!(tr.forecast.is_some() && tr.forecast_hidden.is_none());
        // last 12 months: the merged line from the start of the history to now
        let last12 = tr.last12.expect("last12");
        assert_eq!(tr.last12_days, 365);
        let total = 400 + (0.5 * 145.0_f64).round() as i64;
        assert!(last12 > 72 && last12 < total, "{last12} of {total}");
        assert!(tr.monthly_avg.is_some());
        assert_eq!(tr.goal_status, "month");
    }

    // ------------------------------------------------ forecast: range and hiding rule

    #[test]
    fn forecast_is_hidden_until_sixty_days_of_snapshots() {
        let now = t("2026-11-22T12:00:00Z");
        for days in [0, 10, 59] {
            let pts = vec![(now - Duration::days(days), 100), (now, 100 + days)];
            let r = forecast_band(&pts, now, d("2026-11-22"));
            assert_eq!(r, ForecastOutcome::Hidden { have: days, need: FORECAST_MIN_DAYS }, "{days} days");
        }
        assert!(matches!(forecast_band(&[], now, d("2026-11-22")), ForecastOutcome::Hidden { have: 0, .. }));
        let enough = line("2026-09-20T12:00:00Z", 12, 6, 100, 1.0); // 66 days
        assert!(matches!(forecast_band(&enough, now, d("2026-11-22")), ForecastOutcome::Shown(_)));
        // the trend carries the hiding note instead of a band
        let tr = trend_from(&line("2026-11-01T12:00:00Z", 5, 5, 100, 1.0), &[], None, now, d("2026-11-22"));
        assert!(tr.forecast.is_none());
        assert_eq!(tr.forecast_hidden, Some((20, FORECAST_MIN_DAYS)));
        assert!(tr.hist.is_empty());
    }

    #[test]
    fn forecast_gives_an_ordered_range_that_widens_with_noise() {
        let now = t("2026-11-22T12:00:00Z");
        let steady = line("2026-05-01T12:00:00Z", 60, 3, 100, 2.0); // 2 per day, 177 days
        let ForecastOutcome::Shown(b) = forecast_band(&steady, now, d("2026-11-22")) else { panic!("hidden") };
        assert!((b.per_day - 2.0).abs() < 0.2, "{}", b.per_day);
        assert!(b.low <= b.mid && b.mid <= b.high, "{b:?}");
        assert!(!b.thin && b.windows >= 5);
        // 39 days left (23 Nov .. 31 Dec) at about 2 per day
        let cur = steady.last().expect("last").1;
        assert!((b.mid - (cur + 78)).abs() <= 8, "{} vs {}", b.mid, cur + 78);
        assert_eq!(b.start_v, cur);
        // a bursty history widens the range
        let mut bursty = steady.clone();
        for (i, p) in bursty.iter_mut().enumerate() {
            p.1 += if (i / 10) % 2 == 0 { 0 } else { (i as i64 % 10) * 15 };
        }
        let ForecastOutcome::Shown(w) = forecast_band(&bursty, now, d("2026-11-22")) else { panic!("hidden") };
        assert!(w.high - w.low > b.high - b.low, "{} vs {}", w.high - w.low, b.high - b.low);
        // thin data (about two windows) is flagged and gets a wide range
        let thin = line("2026-09-15T12:00:00Z", 15, 5, 50, 1.0);
        let ForecastOutcome::Shown(tb) = forecast_band(&thin, now, d("2026-11-22")) else { panic!("hidden") };
        assert!(tb.thin && tb.windows < 3);
        assert!(tb.high - tb.low > 0 && tb.low <= tb.mid && tb.mid <= tb.high);
        // a falling total never forecasts below the current total
        let falling = line("2026-05-01T12:00:00Z", 30, 6, 500, -1.0);
        let ForecastOutcome::Shown(f) = forecast_band(&falling, now, d("2026-11-22")) else { panic!("hidden") };
        assert!(f.low >= f.start_v && f.mid >= f.start_v);
    }

    #[test]
    fn the_first_scan_never_counts_as_growth() {
        // The first snapshot already carries Scholar's whole history (50 000): that level is a
        // baseline only. The growth windows see 1 citation per day, nothing else.
        let now = t("2026-11-22T12:00:00Z");
        let pts = line("2026-08-01T12:00:00Z", 28, 5, 50_000, 1.0);
        let w = growth_windows(&pts, now);
        assert!(!w.is_empty());
        assert!(w.iter().all(|r| (r - 1.0).abs() < 0.2), "{w:?}");
        // a window that would start before the first snapshot is dropped, not guessed
        let short = line("2026-11-01T12:00:00Z", 5, 5, 50_000, 1.0);
        assert!(growth_windows(&short, now).is_empty());
        // and through the forecast: about one per day, not thousands
        let ForecastOutcome::Shown(b) = forecast_band(&pts, now, d("2026-11-22")) else { panic!("hidden") };
        assert!(b.per_day < 2.0, "{}", b.per_day);
    }

    // ------------------------------------------------------- visit summary

    fn sv(cit: i64, h: i64, i10: i64) -> SnapView {
        SnapView { cit, h, i10 }
    }

    fn vin(baseline: Option<SnapView>, latest: SnapView) -> VisitInput {
        VisitInput {
            baseline,
            latest,
            top: Some(TopArticle { key: "a".into(), title: "Alpha".into(), gain: 3 }),
            next_h_target: latest.h + 1,
            next_h_needs: Some(5),
            last_gain_at: Some("2026-11-12T10:00:00Z".into()),
            baseline_at: Some("2026-11-20T10:00:00Z".into()),
        }
    }

    #[test]
    fn visit_summary_templates_follow_the_priority_order() {
        // (a) an index threshold wins over new citations, h before i10
        let s = summarize_visit(&vin(Some(sv(100, 6, 3)), sv(110, 7, 4)));
        assert_eq!((s.kind.as_str(), s.metric.as_deref(), s.value), ("threshold", Some("h"), 7));
        let s = summarize_visit(&vin(Some(sv(100, 7, 3)), sv(104, 7, 4)));
        assert_eq!((s.kind.as_str(), s.metric.as_deref(), s.value), ("threshold", Some("i10"), 4));
        // (b) new citations with the top contributing article
        let s = summarize_visit(&vin(Some(sv(100, 7, 4)), sv(104, 7, 4)));
        assert_eq!((s.kind.as_str(), s.net), ("gain", 4));
        assert_eq!(s.top.as_ref().map(|t| t.title.as_str()), Some("Alpha"));
        // (c) a net decrease, stated neutrally (no top article)
        let s = summarize_visit(&vin(Some(sv(100, 7, 4)), sv(98, 7, 4)));
        assert_eq!((s.kind.as_str(), s.net), ("loss", -2));
        assert!(s.top.is_none());
        // a lower h is a decrease, not a threshold
        let s = summarize_visit(&vin(Some(sv(100, 8, 4)), sv(99, 7, 4)));
        assert_eq!(s.kind, "loss");
        // (d) nothing changed but the next h is close
        let mut close = vin(Some(sv(100, 7, 4)), sv(100, 7, 4));
        close.next_h_needs = Some(3);
        assert_eq!(summarize_visit(&close).kind, "closeness");
        // calm: nothing changed and the next h is far; the last rise is kept for the sentence
        let calm = summarize_visit(&vin(Some(sv(100, 7, 4)), sv(100, 7, 4)));
        assert_eq!(calm.kind, "calm");
        assert_eq!(calm.net, 0);
        assert_eq!(calm.last_gain_at.as_deref(), Some("2026-11-12T10:00:00Z"));
        // needing new articles is never "close"
        let mut far = vin(Some(sv(100, 7, 4)), sv(100, 7, 4));
        far.next_h_needs = None;
        assert_eq!(summarize_visit(&far).kind, "calm");
        // no previous visit
        assert_eq!(summarize_visit(&vin(None, sv(100, 7, 4))).kind, "first");
    }

    #[test]
    fn visit_summary_compares_against_the_snapshot_of_the_last_visit() {
        use crate::scholar::{Article, ProfileData, Stats};
        let dir = std::env::temp_dir().join(format!("sct-visit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let mut conn = db::open(&dir.join("v.db")).expect("db");
        let art = |k: &str, title: &str, c: i64| Article {
            key: k.into(),
            title: title.into(),
            authors: String::new(),
            venue: String::new(),
            year: Some(2020),
            citations: c,
            url: String::new(),
            cites_ids: String::new(),
        };
        let mk = |a: i64, b: i64, h: i64| ProfileData {
            name: "P".into(),
            stats: Stats { cit_all: a + b, cit_since: a + b, h_all: h, h_since: h, i10_all: 1, i10_since: 1, since_year: 2021 },
            articles: vec![art("a", "Alpha", a), art("b", "Beta", b)],
            yearly: vec![],
        };
        db::insert_snapshot(&mut conn, "P", t("2026-11-10T10:00:00Z"), &mk(10, 5, 4)).expect("s1");
        db::insert_snapshot(&mut conn, "P", t("2026-11-18T10:00:00Z"), &mk(12, 5, 4)).expect("s2");
        db::insert_snapshot(&mut conn, "P", t("2026-11-21T10:00:00Z"), &mk(15, 6, 5)).expect("s3");

        // never visited: "first"
        assert_eq!(build_visit_summary(&conn).expect("sum").expect("some").kind, "first");

        // first show, then a later show after s2: s3 is new since then, and h crossed 5
        begin_visit(&conn, t("2026-11-15T09:00:00Z")).expect("begin");
        end_visit(&conn, t("2026-11-19T09:00:00Z")).expect("end");
        begin_visit(&conn, t("2026-11-22T09:00:00Z")).expect("begin 2");
        let s = build_visit_summary(&conn).expect("sum").expect("some");
        assert_eq!(s.baseline_at.as_deref(), Some("2026-11-19T09:00:00Z"));
        assert_eq!((s.kind.as_str(), s.metric.as_deref(), s.value), ("threshold", Some("h"), 5));
        assert_eq!(s.net, 21 - 17);

        // a visit that started before any snapshot uses the first snapshot as the baseline
        db::set_state(&conn, STATE_VISIT_BASELINE, "2026-11-01T00:00:00Z").expect("state");
        let s = build_visit_summary(&conn).expect("sum").expect("some");
        assert_eq!(s.net, 21 - 15);

        // after the latest snapshot: nothing new, with the time of the last rise
        db::set_state(&conn, STATE_VISIT_BASELINE, "2026-11-21T12:00:00Z").expect("state");
        let s = build_visit_summary(&conn).expect("sum").expect("some");
        assert_eq!(s.net, 0);
        assert!(s.kind == "calm" || s.kind == "closeness");
        assert_eq!(s.last_gain_at.as_deref(), Some("2026-11-21T10:00:00Z"));

        // plain gain names the article behind most of it
        db::insert_snapshot(&mut conn, "P", t("2026-11-22T10:00:00Z"), &mk(19, 6, 5)).expect("s4");
        let s = build_visit_summary(&conn).expect("sum").expect("some");
        assert_eq!(s.kind, "gain");
        assert_eq!(s.top.as_ref().map(|t| (t.title.as_str(), t.gain)), Some(("Alpha", 4)));
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn last_gain_is_the_latest_rise() {
        let s = vec![
            sp("2026-11-01T00:00:00Z", 10, 1, 0),
            sp("2026-11-05T00:00:00Z", 12, 1, 0),
            sp("2026-11-08T00:00:00Z", 12, 1, 0),
            sp("2026-11-09T00:00:00Z", 11, 1, 0),
        ];
        assert_eq!(last_gain_time(&s).as_deref(), Some("2026-11-05T00:00:00Z"));
        assert_eq!(last_gain_time(&s[..1]), None);
    }

    // ------------------------------------------- stacked, calendar, indices

    #[test]
    fn stacked_keeps_the_top_articles_and_folds_the_rest_into_other() {
        let mut rows = Vec::new();
        for (art, n) in [("a", 5), ("b", 4), ("c", 3), ("d", 2), ("e", 2), ("f", 1), ("g", 1)] {
            for i in 0..n {
                rows.push(crow(art, &format!("{art}{i}"), &format!("2026-0{}-10", 1 + (i % 3)), "day"));
            }
        }
        rows.push(crow("a", "ay", "2026", "year"));
        rows.push(crow("a", "am", "2026-05", "month"));
        let s = stacked_by_article(&rows, 5);
        assert_eq!(s.months, vec!["2026-01", "2026-02", "2026-03", "2026-04", "2026-05"], "continuous months");
        assert_eq!(s.series.len(), 5);
        assert_eq!(s.series[0].key, "a");
        assert_eq!(s.series[0].total, 6, "month-dated records count, year-only ones do not");
        assert_eq!(s.year_only, 1);
        assert_eq!(s.placed, rows.len() - 1);
        assert_eq!(s.other_articles, 2);
        assert_eq!(s.other.iter().sum::<i64>(), 2);
        // every column adds up to the placed records
        let col: i64 = s.series.iter().map(|x| x.counts.iter().sum::<i64>()).sum::<i64>() + s.other.iter().sum::<i64>();
        assert_eq!(col as usize, s.placed);
        // with few articles there is no "other"
        let few = stacked_by_article(&rows[..9], 5);
        assert!(few.other.is_empty() && few.other_articles == 0);
        assert!(stacked_by_article(&[], 5).months.is_empty());
    }

    #[test]
    fn calendar_counts_distinct_works_on_their_day_and_keeps_coarse_dates_apart() {
        let rows = vec![
            crow("a", "W1", "2026-03-05", "day"),
            crow("b", "W1", "2026-03-05", "day"), // the same work cites two articles
            crow("a", "W2", "2026-03-05", "day"),
            crow("a", "W3", "2025-12-31", "day"),
            crow("a", "W4", "2026-03", "month"),
            crow("a", "W5", "2026", "year"),
            crow("a", "W6", "2025", "year"),
        ];
        let c = calendar(&rows);
        assert_eq!(c.days, vec![("2025-12-31".to_string(), 1), ("2026-03-05".to_string(), 2)]);
        assert_eq!(c.years, vec![2025, 2026]);
        assert_eq!((c.month_only, c.year_only, c.works, c.max), (1, 2, 6, 2));
        // no coarse record lands on January 1st or the 1st of a month
        assert!(c.days.iter().all(|(d, _)| d != "2026-01-01" && d != "2026-03-01"));
        assert_eq!(calendar(&[]), Calendar::default());
    }

    #[test]
    fn index_history_keeps_the_steps() {
        let s = vec![
            sp("2026-01-01T00:00:00Z", 100, 5, 2),
            sp("2026-01-04T00:00:00Z", 104, 5, 2),
            sp("2026-01-07T00:00:00Z", 110, 6, 2),
            sp("2026-01-10T00:00:00Z", 111, 6, 3),
            sp("2026-01-13T00:00:00Z", 112, 6, 3),
            sp("2026-01-16T00:00:00Z", 113, 6, 3),
        ];
        let h = index_history(&s);
        assert_eq!(
            h.points.iter().map(|p| (p.t.as_str(), p.h, p.i10)).collect::<Vec<_>>(),
            vec![
                ("2026-01-01T00:00:00Z", 5, 2),
                ("2026-01-07T00:00:00Z", 6, 2),
                ("2026-01-10T00:00:00Z", 6, 3),
                ("2026-01-16T00:00:00Z", 6, 3),
            ]
        );
        assert_eq!(h.changes.len(), 2);
        assert_eq!((h.changes[0].metric.as_str(), h.changes[0].from, h.changes[0].to), ("h", 5, 6));
        assert_eq!((h.changes[1].metric.as_str(), h.changes[1].from, h.changes[1].to), ("i10", 2, 3));
        assert_eq!(index_history(&[]), IndexHistory::default());
        assert_eq!(index_history(&s[..1]).points.len(), 1);
    }

    #[test]
    fn next_steps_list_every_candidate_closest_first() {
        let arts = vec![art("a", 25), art("b", 10), art("c", 8), art("d", 6), art("e", 3), art("f", 0)];
        let s = i10_next_step(&arts);
        let keys: Vec<(&str, i64)> = s.candidates.iter().map(|p| (p.key.as_str(), p.needs)).collect();
        assert_eq!(keys, vec![("c", 2), ("d", 4), ("e", 7), ("f", 10)]);
        assert_eq!(s.picks.len(), 1);
        // the h list is capped
        let many: Vec<(String, String, i64)> = (0..30).map(|i| art(&format!("k{i:02}"), 1)).collect();
        assert_eq!(h_next_step(&many).candidates.len(), MAX_CANDIDATES);
    }

    #[test]
    fn analysis_builds_from_an_empty_and_a_filled_database() {
        use crate::scholar::{Article, ProfileData, Stats};
        let dir = std::env::temp_dir().join(format!("sct-analysis-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let mut conn = db::open(&dir.join("a.db")).expect("db");
        let now = t("2026-11-22T12:00:00Z");
        let a = build_analysis(&conn, now, d("2026-11-22"), None).expect("empty");
        assert!(!a.trend.has_data && a.stacked.months.is_empty() && a.calendar.days.is_empty() && a.next.is_none());
        let data = ProfileData {
            name: "P".into(),
            stats: Stats { cit_all: 12, cit_since: 12, h_all: 2, h_since: 2, i10_all: 1, i10_since: 1, since_year: 2021 },
            articles: vec![Article {
                key: "a".into(),
                title: "Alpha".into(),
                authors: String::new(),
                venue: String::new(),
                year: Some(2020),
                citations: 12,
                url: String::new(),
                cites_ids: String::new(),
            }],
            yearly: vec![(2026, 12)],
        };
        db::insert_snapshot(&mut conn, "P", t("2026-11-01T10:00:00Z"), &data).expect("snap");
        let a = build_analysis(&conn, now, d("2026-11-22"), Some((d("2026-01-01"), d("2026-12-31")))).expect("filled");
        assert!(a.trend.has_data);
        assert_eq!(a.trend.snaps.len(), 1);
        assert!(a.trend.forecast.is_none());
        assert_eq!(a.indices.points.len(), 1);
        assert_eq!(a.yearly, vec![(2026, 12)]);
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ------------------------------------------------ risers: basis and OpenAlex lag

    fn rref(complete: bool, covered: i64, total: i64, base: &[(&str, i64)]) -> Option<RiserRef> {
        Some(RiserRef { map: base.iter().map(|(k, v)| (k.to_string(), *v)).collect(), covered_days: covered, total_change: total, complete })
    }

    #[test]
    fn risers_pick_the_scholar_basis_and_keep_openalex_apart() {
        let latest = vec![art("a", 20), art("b", 10), art("c", 5)];
        let oa: HashMap<String, i64> = [("c".to_string(), 4), ("b".to_string(), 1)].into_iter().collect();
        // the snapshot history covers the whole window
        let r = risers_from(30, &latest, rref(true, 30, 6, &[("a", 17), ("b", 9), ("c", 5)]), &oa, Some("2026-10-14".into()));
        assert_eq!((r.basis.as_str(), r.covered_days, r.snap_total), ("snapshots", 30, 6));
        assert_eq!(r.items.iter().map(|x| (x.key.as_str(), x.gain)).collect::<Vec<_>>(), vec![("a", 3), ("b", 1)]);
        // OpenAlex items are listed separately and never mixed into the Scholar ones
        assert_eq!(r.oa_items.iter().map(|x| (x.key.as_str(), x.gain)).collect::<Vec<_>>(), vec![("c", 4), ("b", 1)]);
        assert_eq!(r.oa_latest.as_deref(), Some("2026-10-14"));
        // the history covers only three days: partial, with what it covers
        let p = risers_from(30, &latest, rref(false, 3, 5, &[("a", 18), ("b", 9), ("c", 4)]), &HashMap::new(), None);
        assert_eq!((p.basis.as_str(), p.covered_days, p.snap_total), ("snapshots_partial", 3, 5));
        assert_eq!(p.items.len(), 3);
        assert!(p.oa_items.is_empty());
        // no earlier snapshot at all: no Scholar items, OpenAlex items still there
        let n = risers_from(30, &latest, None, &oa, Some("2026-10-14".into()));
        assert_eq!((n.basis.as_str(), n.covered_days), ("none", 0));
        assert!(n.items.is_empty() && !n.oa_items.is_empty());
        // empty with a reason: neither source has anything, the newest OpenAlex day is still reported
        let e = risers_from(30, &latest, None, &HashMap::new(), Some("2026-10-14".into()));
        assert!(e.items.is_empty() && e.oa_items.is_empty());
        assert_eq!(e.oa_latest.as_deref(), Some("2026-10-14"));
    }

    #[test]
    fn risers_build_from_a_database_with_a_short_history() {
        use crate::scholar::{Article, ProfileData, Stats};
        let dir = std::env::temp_dir().join(format!("sct-risers-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let mut conn = db::open(&dir.join("r.db")).expect("db");
        let mk = |a: i64| ProfileData {
            name: "P".into(),
            stats: Stats { cit_all: a, cit_since: a, h_all: 1, h_since: 1, i10_all: 1, i10_since: 1, since_year: 2021 },
            articles: vec![Article { key: "a".into(), title: "Alpha".into(), authors: String::new(), venue: String::new(), year: Some(2020), citations: a, url: String::new(), cites_ids: String::new() }],
            yearly: vec![],
        };
        let now = t("2026-11-22T12:00:00Z");
        db::insert_snapshot(&mut conn, "P", now - Duration::days(3), &mk(10)).expect("s1");
        db::insert_snapshot(&mut conn, "P", now - Duration::hours(1), &mk(15)).expect("s2");
        let r = build_risers(&conn, now, 30).expect("risers");
        assert_eq!((r.basis.as_str(), r.covered_days, r.snap_total), ("snapshots_partial", 2, 5));
        assert_eq!(r.items.first().map(|x| x.gain), Some(5));
        assert_eq!(r.oa_latest, None);
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_openalex_lag_tail_is_never_counted_as_zero_months() {
        let months: Vec<(String, i64)> = [("2026-03", 6), ("2026-04", 4), ("2026-05", 5), ("2026-06", 3), ("2026-07", 6), ("2026-08", 6)]
            .iter()
            .map(|(k, v)| (k.to_string(), *v))
            .collect();
        // newest citing work on 14 Oct: Sep and Oct are the lag; the covered months are up to Sep 30? no:
        // the month of the newest work (Oct) and everything after it are excluded, September counts as covered
        // only if OpenAlex has reached October, so it is a (real) zero month here
        let r = oa_monthly_rate(&months, Some("2026-10-14"), 6).expect("rate");
        assert!((r - (4.0 + 5.0 + 3.0 + 6.0 + 6.0 + 0.0) / 6.0).abs() < 1e-9, "{r}");
        // if OpenAlex stops in August, the empty September / October / November are lag, not zeros
        let lag = oa_monthly_rate(&months, Some("2026-08-20"), 6).expect("rate");
        assert!((lag - (6.0 + 4.0 + 5.0 + 3.0 + 6.0) / 5.0).abs() < 1e-9, "{lag}");
        // never reaches back before the first month with data
        assert_eq!(oa_monthly_rate(&months, Some("2026-04-02"), 6), Some(6.0));
        assert_eq!(oa_monthly_rate(&months, None, 6), None);
        assert_eq!(oa_monthly_rate(&[], Some("2026-10-14"), 6), None);
        // January wraps to December of the year before
        assert_eq!(oa_monthly_rate(&[("2025-12".to_string(), 8)], Some("2026-01-05"), 3), Some(8.0));
    }

    // ------------------------------------------------ calendar days, details, attribution

    fn utc_day(ts: &str) -> String {
        ts[..10].to_string()
    }

    fn sch(key: &str, at: &str, old: i64, new: i64) -> db::ScholarChange {
        db::ScholarChange { article_key: key.into(), title: format!("Article {key}"), cites_ids: "1".into(), at: at.into(), old, new, delta: new - old }
    }

    #[test]
    fn calendar_days_count_both_sources_decreases_and_leave_undated_works_off_the_grid() {
        let changes = vec![
            sch("a", "2026-11-05T10:00:00Z", 3, 5),
            sch("b", "2026-11-05T11:00:00Z", 9, 8), // a decrease
            sch("b", "2026-11-09T10:00:00Z", 8, 7), // a day with only a decrease
            sch("a", "2026-10-30T10:00:00Z", 2, 3), // outside the range
        ];
        let rows = vec![
            crow("a", "W1", "2026-11-05", "day"),
            crow("b", "W1", "2026-11-05", "day"), // same work, two articles: one work
            crow("a", "W2", "2026-11-12", "day"),
            crow("a", "W3", "2026-11", "month"),
            crow("a", "W4", "2026", "year"),
            crow("a", "W5", "2025", "year"),
        ];
        let c = calendar_days(&changes, &rows, "2026-11-01", "2026-11-30", "all", "", &utc_day);
        let by: HashMap<&str, &DayAgg> = c.days.iter().map(|d| (d.date.as_str(), d)).collect();
        assert_eq!(c.days.len(), 3);
        let d5 = by["2026-11-05"];
        assert_eq!((d5.scholar_up, d5.scholar_down, d5.scholar_events, d5.openalex), (2, 1, 2, 1));
        let d9 = by["2026-11-09"];
        assert_eq!((d9.scholar_up, d9.scholar_down, d9.openalex), (0, 1, 0), "only a decrease");
        assert_eq!(by["2026-11-12"].openalex, 1);
        // the month- and year-only works never get a day; they are counted apart
        assert_eq!((c.month_only, c.year_only), (1, 1));
        assert!(!by.contains_key("2026-11-01") && !by.contains_key("2026-01-01"));
        // sources and the article filter
        let sc = calendar_days(&changes, &rows, "2026-11-01", "2026-11-30", "scholar", "", &utc_day);
        assert!(sc.days.iter().all(|d| d.openalex == 0) && (sc.month_only, sc.year_only) == (0, 0));
        let oa = calendar_days(&changes, &rows, "2026-11-01", "2026-11-30", "openalex", "", &utc_day);
        assert!(oa.days.iter().all(|d| d.scholar_events == 0) && oa.days.len() == 2);
        let only_b = calendar_days(&changes, &rows, "2026-11-01", "2026-11-30", "all", "b", &utc_day);
        assert_eq!(only_b.days.iter().map(|d| d.date.as_str()).collect::<Vec<_>>(), vec!["2026-11-05", "2026-11-09"]);
        // local time: a shifted day mapping moves a late-evening change to the next day
        let shifted = calendar_days(&[sch("a", "2026-11-05T22:30:00Z", 1, 2)], &[], "2026-11-01", "2026-11-30", "all", "", &|ts: &str| {
            db::parse_ts(ts).map(|t| (t + Duration::hours(3)).format("%Y-%m-%d").to_string()).unwrap_or_default()
        });
        assert_eq!(shifted.days[0].date, "2026-11-06");
    }

    #[test]
    fn the_first_snapshot_day_is_not_a_day_of_citations() {
        use crate::scholar::{Article, ProfileData, Stats};
        let dir = std::env::temp_dir().join(format!("sct-calfirst-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let mut conn = db::open(&dir.join("c.db")).expect("db");
        let mk = |a: i64| ProfileData {
            name: "P".into(),
            stats: Stats { cit_all: a, cit_since: a, h_all: 1, h_since: 1, i10_all: 1, i10_since: 1, since_year: 2021 },
            articles: vec![Article { key: "a".into(), title: "Alpha".into(), authors: String::new(), venue: String::new(), year: Some(2020), citations: a, url: String::new(), cites_ids: String::new() }],
            yearly: vec![],
        };
        db::insert_snapshot(&mut conn, "P", t("2026-11-01T10:00:00Z"), &mk(500)).expect("first scan: the whole profile");
        db::insert_snapshot(&mut conn, "P", t("2026-11-04T10:00:00Z"), &mk(503)).expect("second");
        let c = calendar_days(&db::scholar_changes(&conn).expect("ch"), &[], "2026-10-01", "2026-11-30", "all", "", &utc_day);
        assert_eq!(c.days.len(), 1, "{:?}", c.days);
        assert_eq!((c.days[0].date.as_str(), c.days[0].scholar_up), ("2026-11-04", 3));
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn dday(s: &str) -> NaiveDate {
        d(s)
    }

    #[test]
    fn attribution_uses_the_window_and_is_one_to_one() {
        let events = vec![
            Ev { article: "a".into(), day: dday("2026-11-10"), n: 2 },
            Ev { article: "a".into(), day: dday("2026-11-20"), n: 1 },
            Ev { article: "b".into(), day: dday("2026-11-10"), n: 1 },
        ];
        let w = |a: &str, id: &str, day: &str| Wk { article: a.into(), id: id.into(), day: dday(day) };
        let works = vec![
            w("a", "W1", "2026-11-09"),  // 1 day before event 0
            w("a", "W2", "2026-10-01"),  // 40 days before event 0: inside the window
            w("a", "W3", "2026-09-20"),  // 51 days before: outside
            w("a", "W4", "2026-11-26"),  // 16 days after event 0: outside; 6 days after event 1: inside
            w("b", "W5", "2026-11-12"),  // 2 days after the b event
            w("c", "W6", "2026-11-10"),  // another article: never
        ];
        let m = attribute_events(&events, &works);
        // closest pairs first: W1 -> event 0, W2 -> event 0 (40 days), W4 -> event 1 (6 days)
        assert_eq!(m.get(&0), Some(&vec![0, 1]));
        assert_eq!(m.get(&1), Some(&vec![3]));
        assert_eq!(m.get(&2), Some(&vec![4]));
        // one to one: no work twice, no event over its n
        let mut used: Vec<usize> = m.values().flatten().copied().collect();
        used.sort_unstable();
        let before = used.len();
        used.dedup();
        assert_eq!(before, used.len());
        assert!(!used.contains(&2) && !used.contains(&5), "outside the window / other article");
        // a work shared by two events goes to the closer one only
        let two = vec![Ev { article: "a".into(), day: dday("2026-11-10"), n: 1 }, Ev { article: "a".into(), day: dday("2026-11-20"), n: 1 }];
        let one_work = vec![w("a", "W9", "2026-11-18")];
        let m2 = attribute_events(&two, &one_work);
        assert_eq!(m2.len(), 1);
        assert_eq!(m2.get(&1), Some(&vec![0]));
        // deterministic: same input, same answer (recomputation is idempotent)
        assert_eq!(attribute_events(&events, &works), m);
        // an event with n = 0 or a decrease-sized event takes nothing
        let none = vec![Ev { article: "a".into(), day: dday("2026-11-10"), n: 0 }];
        assert!(attribute_events(&none, &works).is_empty());
    }

    #[test]
    fn day_detail_links_increases_to_probable_works_and_never_decreases() {
        let changes = vec![
            sch("a", "2026-11-10T10:00:00Z", 3, 5),  // +2
            sch("a", "2026-11-12T10:00:00Z", 5, 4),  // -1: a decrease, never matched
            sch("a", "2026-11-14T10:00:00Z", 4, 5),  // +1 with no work around
        ];
        let rows = vec![
            crow("a", "W1", "2026-11-09", "day"),
            crow("a", "W2", "2026-11-11", "day"),
            crow("a", "W3", "2026-11-12", "day"), // would fit the decrease day best, still not matched to it
        ];
        let det = day_detail(&changes, &rows, "2026-11-01", "2026-11-30", "all", "", &utc_day);
        let day = |s: &str| det.iter().find(|d| d.date == s).expect("day");
        let up = &day("2026-11-10").scholar[0];
        assert_eq!(up.delta, 2);
        assert_eq!(up.probable.iter().map(|w| w.citing_id.as_str()).collect::<Vec<_>>(), vec!["W1", "W2"]);
        assert_eq!(up.confidence, "probable");
        let down = &day("2026-11-12").scholar[0];
        assert!(down.delta < 0 && down.probable.is_empty(), "decreases are never matched");
        assert_eq!(day("2026-11-12").openalex.len(), 1);
        // W3 is free, so the later +1 takes it (within 15 days after / 45 before)
        let last = &day("2026-11-14").scholar[0];
        assert_eq!(last.probable.iter().map(|w| w.citing_id.as_str()).collect::<Vec<_>>(), vec!["W3"]);
        // the openalex source alone has no Scholar rows; the scholar source alone no works
        let oa = day_detail(&changes, &rows, "2026-11-01", "2026-11-30", "openalex", "", &utc_day);
        assert!(oa.iter().all(|d| d.scholar.is_empty()) && oa.len() == 3);
        let sc = day_detail(&changes, &rows, "2026-11-01", "2026-11-30", "scholar", "", &utc_day);
        assert!(sc.iter().all(|d| d.openalex.is_empty()));
        // an article filter keeps the attribution of the others out of the way
        let none = day_detail(&changes, &rows, "2026-11-01", "2026-11-30", "all", "zzz", &utc_day);
        assert!(none.is_empty());
        // range: a single day
        let one = day_detail(&changes, &rows, "2026-11-10", "2026-11-10", "all", "", &utc_day);
        assert_eq!(one.len(), 1);
    }
}
