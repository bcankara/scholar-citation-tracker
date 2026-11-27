//! Background scheduler: periodic fetch cycles with retry rules.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::Notify;

use crate::settings::Settings;
use crate::{db, i18n, scholar};

/// Total attempts per cycle: the first one plus three retries.
pub const MAX_ATTEMPTS: u32 = 4;
const RETRY_WAIT_MINUTES: i64 = 5;
const STARTUP_DELAY_SECS: i64 = 30;
const RESCHEDULE_DELAY_SECS: i64 = 5;
/// Wall-clock polling period of the main loop.
const POLL_SECS: u64 = 60;

pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Pure scheduling rule. The next cycle is `last_cycle + interval`; with no
/// history, or when that moment has already passed, it runs after `delay`.
pub fn compute_next_run(
    last_cycle: Option<DateTime<Utc>>,
    interval_hours: u32,
    now: DateTime<Utc>,
    delay: ChronoDuration,
) -> DateTime<Utc> {
    let interval = ChronoDuration::hours(i64::from(interval_hours.clamp(1, 24)));
    match last_cycle {
        Some(last) => {
            let next = last + interval;
            if next <= now {
                now + delay
            } else {
                next
            }
        }
        None => now + delay,
    }
}

/// No successful fetch for this long (while attempts were made) counts as a health problem.
pub const HEALTH_WINDOW_HOURS: i64 = 48;
/// Minimum gap between two health notifications.
pub const HEALTH_REPEAT_HOURS: i64 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Health {
    /// No `ok` for 48 h although at least one attempt was made in that window.
    pub unhealthy: bool,
    /// A Windows notification should be shown now (once per incident, at most once per 24 h).
    pub notify: bool,
}

/// Start of the current outage: the last success, or the first attempt when there never was one.
pub fn incident_start(times: &db::FetchTimes) -> Option<DateTime<Utc>> {
    times.last_ok.or(times.first_attempt)
}

/// Pure health rule. `last_notified` is the stored time of the previous health notification.
///
/// - unhealthy: the last success (or, with none, the first attempt) is at least 48 h old and
///   an attempt was made within the last 48 h. A long pause with no attempts (app closed) is
///   not a Scholar problem and does not count.
/// - notify: unhealthy, no notification yet for this incident (none stored after the incident
///   start), and the previous one is at least 24 h ago.
pub fn health_decision(
    now: DateTime<Utc>,
    times: &db::FetchTimes,
    last_notified: Option<DateTime<Utc>>,
) -> Health {
    let window = ChronoDuration::hours(HEALTH_WINDOW_HOURS);
    let Some(start) = incident_start(times) else {
        return Health { unhealthy: false, notify: false };
    };
    let attempted_in_window = times.last_attempt.is_some_and(|a| now - a < window);
    let unhealthy = now - start >= window && attempted_in_window;
    let notify = unhealthy
        && last_notified.is_none_or(|n| {
            n < start && now - n >= ChronoDuration::hours(HEALTH_REPEAT_HOURS)
        });
    Health { unhealthy, notify }
}

#[derive(Debug, Clone, Serialize)]
pub struct HealthInfo {
    pub unhealthy: bool,
    /// Last successful fetch (UTC), if any.
    pub last_ok: Option<String>,
}

/// Current health from the database, for the UI banner.
pub fn current_health(conn: &rusqlite::Connection) -> anyhow::Result<HealthInfo> {
    let times = db::fetch_times(conn)?;
    let h = health_decision(Utc::now(), &times, None);
    Ok(HealthInfo {
        unhealthy: h.unhealthy,
        last_ok: times.last_ok.map(db::ts),
    })
}

/// After a cycle: shows the health notification when the rule says so and remembers it.
async fn check_health(app: &AppHandle, sh: &Shared) {
    let settings = sh.settings();
    let path = sh.db_path();
    let now = Utc::now();
    let decision = blocking(move || -> anyhow::Result<bool> {
        let conn = db::open(&path)?;
        let times = db::fetch_times(&conn)?;
        let notified = db::get_state(&conn, db::STATE_HEALTH_NOTIFIED)?.and_then(|s| db::parse_ts(&s));
        let h = health_decision(now, &times, notified);
        if h.notify {
            // Remember it even when notifications are switched off, so enabling them later
            // does not fire for an old incident.
            db::set_state(&conn, db::STATE_HEALTH_NOTIFIED, &db::ts(now))?;
            // the notification centre always records it
            db::record_event(&conn, now, "health", &db::ts(now), &serde_json::json!({}))?;
        }
        Ok(h.notify)
    })
    .await;
    if matches!(decision, Some(Ok(true))) {
        crate::events::announce(app);
    }
    match decision {
        Some(Ok(true)) if settings.notify => {
            let lang = settings.language.clone();
            show_notification(app, i18n::product_name(&lang), i18n::tr(&lang, "notif.health"));
        }
        Some(Ok(_)) | None => {}
        Some(Err(e)) => eprintln!("health check failed: {e:#}"),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusInfo {
    /// "idle" | "running" | "waiting_retry" | "no_profile"
    pub state: String,
    pub attempt: u32,
    pub max_attempts: u32,
    pub retry_at: Option<String>,
    pub last_error: Option<String>,
    pub next_run: Option<String>,
}

impl Default for StatusInfo {
    fn default() -> Self {
        StatusInfo {
            state: "idle".into(),
            attempt: 0,
            max_attempts: MAX_ATTEMPTS,
            retry_at: None,
            last_error: None,
            next_run: None,
        }
    }
}

/// State shared between the scheduler task, tray and commands.
pub struct Shared {
    status: Mutex<StatusInfo>,
    manual: AtomicBool,
    reschedule: AtomicBool,
    wake: Notify,
    settings: Arc<Mutex<Settings>>,
}

impl Shared {
    pub fn new(settings: Arc<Mutex<Settings>>) -> Arc<Self> {
        Arc::new(Shared {
            status: Mutex::new(StatusInfo::default()),
            manual: AtomicBool::new(false),
            reschedule: AtomicBool::new(false),
            wake: Notify::new(),
            settings,
        })
    }

    pub fn status(&self) -> StatusInfo {
        lock(&self.status).clone()
    }

    /// Requests an immediate cycle. Returns false while an attempt is in progress.
    pub fn trigger_now(&self) -> bool {
        if lock(&self.status).state == "running" || self.settings().profile_id.is_empty() {
            return false;
        }
        self.manual.store(true, Ordering::SeqCst);
        self.wake.notify_one();
        true
    }

    /// Asks the loop to recompute the next run time (e.g. after an interval change).
    pub fn reschedule(&self) {
        self.reschedule.store(true, Ordering::SeqCst);
        self.wake.notify_one();
    }

    fn update(&self, app: &AppHandle, f: impl FnOnce(&mut StatusInfo)) {
        let snapshot = {
            let mut st = lock(&self.status);
            f(&mut st);
            st.clone()
        };
        if let Err(e) = app.emit("status", &snapshot) {
            eprintln!("status event failed: {e}");
        }
    }

    fn settings(&self) -> Settings {
        lock(&self.settings).clone()
    }

    fn db_path(&self) -> PathBuf {
        db::db_path_in(&self.settings().data_path())
    }
}

async fn blocking<T, F>(f: F) -> Option<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    match tauri::async_runtime::spawn_blocking(f).await {
        Ok(v) => Some(v),
        Err(e) => {
            eprintln!("blocking task failed: {e}");
            None
        }
    }
}

async fn read_last_cycle(path: PathBuf) -> Option<DateTime<Utc>> {
    blocking(move || {
        db::open(&path)
            .and_then(|c| db::last_cycle_start(&c))
            .unwrap_or_else(|e| {
                eprintln!("last cycle read failed: {e:#}");
                None
            })
    })
    .await
    .flatten()
}

/// Writes one fetch_log row. Failures go to stderr only.
async fn log_attempt(path: PathBuf, cycle: String, attempt: u32, status: &'static str, message: String) {
    let attempted = db::ts(Utc::now());
    let r = blocking(move || {
        db::open(&path).and_then(|c| db::log_attempt(&c, &cycle, &attempted, attempt, status, &message))
    })
    .await;
    match r {
        Some(Ok(())) => {}
        Some(Err(e)) => eprintln!("fetch_log write failed: {e:#}"),
        None => {}
    }
}

struct Stored {
    change: Option<db::Change>,
    total: i64,
    articles: usize,
}

fn store_snapshot(path: &std::path::Path, profile_id: &str, data: &scholar::ProfileData) -> anyhow::Result<Stored> {
    let mut conn = db::open(path)?;
    let id = db::insert_snapshot(&mut conn, profile_id, Utc::now(), data)?;
    let change = db::change_since_previous(&conn, id)?;
    Ok(Stored {
        change,
        total: data.stats.cit_all,
        articles: data.articles.len(),
    })
}

pub fn set_tray_tooltip(app: &AppHandle, lang: &str, total: Option<i64>) {
    let product = i18n::product_name(lang);
    let text = match total {
        Some(t) => i18n::trf(lang, "tray.tooltip_total", &[product, &t.to_string()]),
        None => product.to_string(),
    };
    if let Some(tray) = app.tray_by_id("main") {
        if let Err(e) = tray.set_tooltip(Some(text)) {
            eprintln!("tray tooltip failed: {e}");
        }
    }
}

fn show_notification(app: &AppHandle, title: &str, body: &str) {
    use tauri_plugin_notification::NotificationExt;
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        eprintln!("notification failed: {e}");
    }
}

/// One attempt: fetch, store, notify. Err carries (log status, localized message).
async fn attempt(app: &AppHandle, sh: &Shared) -> Result<String, (&'static str, String)> {
    let settings = sh.settings();
    let lang = settings.language.clone();
    let data = scholar::fetch_profile(&settings.profile_id)
        .await
        .map_err(|e| (e.status(), e.user_message(&lang)))?;
    let path = settings.data_path();
    let db_file = db::db_path_in(&path);
    let pid = settings.profile_id.clone();
    let data_for_db = data.clone();
    // A page that lost its article list must not become a snapshot (see validate_article_list).
    let prev_file = db_file.clone();
    let previous = blocking(move || {
        db::open(&prev_file)
            .and_then(|c| db::latest_summary(&c))
            .unwrap_or_else(|e| {
                eprintln!("previous snapshot read failed: {e:#}");
                None
            })
    })
    .await
    .flatten();
    scholar::validate_article_list(&data, previous).map_err(|e| (e.status(), e.user_message(&lang)))?;
    let stored = blocking(move || store_snapshot(&db_file, &pid, &data_for_db))
        .await
        .ok_or(("parse", i18n::tr(&lang, "log.store_crashed").to_string()))?
        .map_err(|e| ("parse", i18n::trf(&lang, "log.store_failed", &[&format!("{e:#}")])))?;

    set_tray_tooltip(app, &lang, Some(stored.total));
    if let Some(data) = stored.change.as_ref().and_then(db::scholar_event_data) {
        // the notification centre records every change, whatever the "notify" setting says
        let (app_ev, ev_path) = (app.clone(), path.clone());
        let _ = blocking(move || crate::events::record(&app_ev, &db::db_path_in(&ev_path), "scholar", "", &data)).await;
    }
    if settings.notify {
        if let Some((title, body)) = stored
            .change
            .as_ref()
            .and_then(|c| db::notification_text(c, &lang))
        {
            show_notification(app, &title, &body);
        }
    }
    if let Err(e) = app.emit("data-updated", ()) {
        eprintln!("data-updated event failed: {e}");
    }
    // Historical backfill runs on its own task; nothing it does can fail this attempt.
    crate::backfill::kick_after_snapshot(app);
    Ok(i18n::trf(
        &lang,
        "log.ok",
        &[&stored.total.to_string(), &stored.articles.to_string()],
    ))
}

/// Waits until `deadline` using short wall-clock checks. A manual trigger ends the wait early.
async fn wait_until(sh: &Shared, deadline: DateTime<Utc>) {
    loop {
        if sh.manual.swap(false, Ordering::SeqCst) {
            return;
        }
        let remaining = deadline - Utc::now();
        if remaining <= ChronoDuration::zero() {
            return;
        }
        let step = remaining
            .to_std()
            .unwrap_or(Duration::from_secs(15))
            .min(Duration::from_secs(15));
        tokio::select! {
            _ = tokio::time::sleep(step) => {}
            _ = sh.wake.notified() => {}
        }
    }
}

async fn run_cycle(app: &AppHandle, sh: &Shared) {
    let started = Utc::now();
    let cycle_ts = db::ts(started);
    let mut last_error: Option<String> = None;
    let mut succeeded = false;

    for attempt_no in 1..=MAX_ATTEMPTS {
        sh.update(app, |s| {
            s.state = "running".into();
            s.attempt = attempt_no;
            s.retry_at = None;
        });
        let result = attempt(app, sh).await;
        let path = sh.db_path();
        match result {
            Ok(msg) => {
                log_attempt(path, cycle_ts.clone(), attempt_no, "ok", msg).await;
                succeeded = true;
                break;
            }
            Err((status, msg)) => {
                log_attempt(path, cycle_ts.clone(), attempt_no, status, msg.clone()).await;
                last_error = Some(msg);
            }
        }
        if attempt_no < MAX_ATTEMPTS {
            let retry_at = Utc::now() + ChronoDuration::minutes(RETRY_WAIT_MINUTES);
            let err = last_error.clone();
            sh.update(app, |s| {
                s.state = "waiting_retry".into();
                s.retry_at = Some(db::ts(retry_at));
                s.last_error = err;
            });
            wait_until(sh, retry_at).await;
        }
    }

    sh.manual.store(false, Ordering::SeqCst);
    check_health(app, sh).await;
    let interval = sh.settings().interval_hours;
    let next = compute_next_run(
        Some(started),
        interval,
        Utc::now(),
        ChronoDuration::seconds(STARTUP_DELAY_SECS),
    );
    sh.update(app, |s| {
        s.state = "idle".into();
        s.retry_at = None;
        s.next_run = Some(db::ts(next));
        s.last_error = if succeeded { None } else { last_error };
    });
}

fn set_no_profile(sh: &Shared, app: &AppHandle) {
    sh.update(app, |s| {
        s.state = "no_profile".into();
        s.attempt = 0;
        s.retry_at = None;
        s.next_run = None;
        s.last_error = None;
    });
}

/// Main scheduler loop. Never returns; errors are logged, never propagated.
/// While no profile is configured nothing is fetched and nothing is logged.
pub async fn run(app: AppHandle, sh: Arc<Shared>) {
    let mut next = Utc::now();
    let mut no_profile = sh.settings().profile_id.is_empty();
    if no_profile {
        set_no_profile(&sh, &app);
    } else {
        let last = read_last_cycle(sh.db_path()).await;
        next = compute_next_run(
            last,
            sh.settings().interval_hours,
            Utc::now(),
            ChronoDuration::seconds(STARTUP_DELAY_SECS),
        );
        sh.update(&app, |s| s.next_run = Some(db::ts(next)));
    }

    loop {
        if sh.settings().profile_id.is_empty() {
            if !no_profile {
                no_profile = true;
                set_no_profile(&sh, &app);
            }
            sh.manual.store(false, Ordering::SeqCst);
            sh.reschedule.store(false, Ordering::SeqCst);
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(POLL_SECS)) => {}
                _ = sh.wake.notified() => {}
            }
            continue;
        }
        if no_profile {
            // A profile was just set: leave the idle state and recompute the schedule.
            no_profile = false;
            sh.reschedule.store(true, Ordering::SeqCst);
            sh.update(&app, |s| s.state = "idle".into());
        }

        if sh.reschedule.swap(false, Ordering::SeqCst) {
            let last = read_last_cycle(sh.db_path()).await;
            next = compute_next_run(
                last,
                sh.settings().interval_hours,
                Utc::now(),
                ChronoDuration::seconds(RESCHEDULE_DELAY_SECS),
            );
            sh.update(&app, |s| s.next_run = Some(db::ts(next)));
        }

        let manual = sh.manual.swap(false, Ordering::SeqCst);
        if manual || Utc::now() >= next {
            run_cycle(&app, &sh).await;
            next = sh
                .status()
                .next_run
                .and_then(|s| db::parse_ts(&s))
                .unwrap_or_else(|| Utc::now() + ChronoDuration::hours(1));
            continue;
        }

        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(POLL_SECS)) => {}
            _ = sh.wake.notified() => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> DateTime<Utc> {
        db::parse_ts(s).expect("ts")
    }

    #[test]
    fn no_history_runs_after_delay() {
        let now = t("2026-11-22T10:00:00Z");
        let n = compute_next_run(None, 3, now, ChronoDuration::seconds(30));
        assert_eq!(n, now + ChronoDuration::seconds(30));
    }

    #[test]
    fn recent_cycle_waits_for_interval() {
        let now = t("2026-11-22T10:00:00Z");
        let last = t("2026-11-22T08:30:00Z");
        let n = compute_next_run(Some(last), 3, now, ChronoDuration::seconds(30));
        assert_eq!(n, t("2026-11-22T11:30:00Z"));
    }

    #[test]
    fn overdue_cycle_runs_after_delay() {
        let now = t("2026-11-22T10:00:00Z");
        let last = t("2026-11-22T06:00:00Z");
        let n = compute_next_run(Some(last), 3, now, ChronoDuration::seconds(30));
        assert_eq!(n, now + ChronoDuration::seconds(30));
        // exactly due counts as overdue
        let last = t("2026-11-22T07:00:00Z");
        let n = compute_next_run(Some(last), 3, now, ChronoDuration::seconds(30));
        assert_eq!(n, now + ChronoDuration::seconds(30));
    }

    #[test]
    fn interval_is_clamped() {
        let now = t("2026-11-22T10:00:00Z");
        let last = t("2026-11-22T09:30:00Z");
        assert_eq!(
            compute_next_run(Some(last), 0, now, ChronoDuration::seconds(5)),
            t("2026-11-22T10:30:00Z")
        );
        assert_eq!(
            compute_next_run(Some(last), 1000, now, ChronoDuration::seconds(5)),
            t("2026-11-23T09:30:00Z")
        );
    }

    fn times(ok: Option<&str>, first: Option<&str>, last: Option<&str>) -> db::FetchTimes {
        db::FetchTimes {
            last_ok: ok.map(t),
            first_attempt: first.map(t),
            last_attempt: last.map(t),
        }
    }

    #[test]
    fn health_is_fine_with_a_recent_success_or_no_history() {
        let now = t("2026-11-22T12:00:00Z");
        // nothing logged yet
        assert_eq!(health_decision(now, &times(None, None, None), None), Health { unhealthy: false, notify: false });
        // success 47 h ago, failures since
        let tm = times(Some("2026-11-20T13:00:00Z"), Some("2026-11-01T00:00:00Z"), Some("2026-11-22T11:50:00Z"));
        assert!(!health_decision(now, &tm, None).unhealthy);
        // never succeeded but the first attempt is only 5 h old
        let tm = times(None, Some("2026-11-22T07:00:00Z"), Some("2026-11-22T11:50:00Z"));
        assert!(!health_decision(now, &tm, None).unhealthy);
    }

    #[test]
    fn health_needs_48_hours_and_an_attempt_in_the_window() {
        let now = t("2026-11-22T12:00:00Z");
        // exactly 48 h since the last ok, attempts continue: unhealthy
        let tm = times(Some("2026-11-20T12:00:00Z"), Some("2026-11-01T00:00:00Z"), Some("2026-11-22T11:50:00Z"));
        assert_eq!(health_decision(now, &tm, None), Health { unhealthy: true, notify: true });
        // app was closed for days: the last attempt is older than the window, so no alarm
        let tm = times(Some("2026-11-10T12:00:00Z"), Some("2026-11-01T00:00:00Z"), Some("2026-11-12T11:50:00Z"));
        assert_eq!(health_decision(now, &tm, None), Health { unhealthy: false, notify: false });
        // never succeeded, failing for 3 days
        let tm = times(None, Some("2026-11-19T10:00:00Z"), Some("2026-11-22T11:50:00Z"));
        assert!(health_decision(now, &tm, None).unhealthy);
    }

    #[test]
    fn health_notification_is_once_per_incident_and_not_within_24_hours() {
        let now = t("2026-11-22T12:00:00Z");
        let tm = times(Some("2026-11-19T12:00:00Z"), Some("2026-11-01T00:00:00Z"), Some("2026-11-22T11:50:00Z"));
        // not notified yet: notify
        assert!(health_decision(now, &tm, None).notify);
        // notified during this incident (after the last success): never again
        let during = Some(t("2026-11-21T10:00:00Z"));
        assert_eq!(health_decision(now, &tm, during), Health { unhealthy: true, notify: false });
        // a notification from an older incident (before the last success) allows a new one
        let older = Some(t("2026-11-10T10:00:00Z"));
        assert!(health_decision(now, &tm, older).notify);
        // healthy again after a success: not unhealthy, no notification
        let recovered = times(Some("2026-11-22T11:00:00Z"), Some("2026-11-01T00:00:00Z"), Some("2026-11-22T11:50:00Z"));
        assert_eq!(health_decision(now, &recovered, older), Health { unhealthy: false, notify: false });
    }

    #[test]
    fn interval_change_reschedules_from_last_cycle() {
        let now = t("2026-11-22T10:00:00Z");
        let last = t("2026-11-22T09:00:00Z");
        let a = compute_next_run(Some(last), 3, now, ChronoDuration::seconds(5));
        let b = compute_next_run(Some(last), 1, now, ChronoDuration::seconds(5));
        assert_eq!(a, t("2026-11-22T12:00:00Z"));
        assert_eq!(b, now + ChronoDuration::seconds(5));
    }
}
