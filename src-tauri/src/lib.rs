//! Scholar Citation Tracker: personal Google Scholar citation tracker (Tauri 2).

mod backfill;
mod citers;
mod db;
mod export;
mod i18n;
mod insights;
mod ojp;
mod pdf;
mod report;
mod scheduler;
mod scholar;
mod secrets;
mod settings;
mod updater;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{Datelike, Local, Timelike, Utc};
use rusqlite::Connection;
use serde::Serialize;
use settings::Settings;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::Wry;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use scheduler::lock;

/// True when the Mica backdrop was applied (page background must then be transparent).
static MICA_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[tauri::command]
fn backdrop_mica() -> bool {
    MICA_ACTIVE.load(std::sync::atomic::Ordering::SeqCst)
}

#[derive(Clone)]
struct AppState {
    settings: Arc<Mutex<Settings>>,
    config_path: Arc<PathBuf>,
    default_dir: Arc<PathBuf>,
    sched: Arc<scheduler::Shared>,
}

/// Tray menu items kept so their texts can be updated when the language changes.
struct TrayItems {
    open: MenuItem<Wry>,
    refresh: MenuItem<Wry>,
    folder: MenuItem<Wry>,
    quit: MenuItem<Wry>,
}

type CmdResult<T> = Result<T, String>;

fn lang_of(state: &AppState) -> String {
    lock(&state.settings).language.clone()
}

/// Updates everything native that shows text: tray menu, tooltip and window title.
fn apply_language(app: &AppHandle, state: &AppState) {
    let (lang, has_profile) = {
        let s = lock(&state.settings);
        (s.language.clone(), !s.profile_id.is_empty())
    };
    if let Some(items) = app.try_state::<TrayItems>() {
        let results = [
            items.open.set_text(i18n::tr(&lang, "tray.open")),
            items.refresh.set_text(i18n::tr(&lang, "tray.refresh")),
            items.folder.set_text(i18n::tr(&lang, "tray.folder")),
            items.quit.set_text(i18n::tr(&lang, "tray.quit")),
            items.refresh.set_enabled(has_profile),
        ];
        for r in results {
            if let Err(e) = r {
                eprintln!("tray menu update failed: {e}");
            }
        }
    }
    if let Some(w) = app.get_webview_window("main") {
        if let Err(e) = w.set_title(i18n::product_name(&lang)) {
            eprintln!("window title update failed: {e}");
        }
    }
    refresh_tooltip(app, state);
}

fn data_dir(state: &AppState) -> PathBuf {
    lock(&state.settings).data_path()
}

/// Runs a blocking DB closure off the async runtime.
async fn with_db<T, F>(state: &AppState, f: F) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce(&mut Connection) -> anyhow::Result<T> + Send + 'static,
{
    let path = db::db_path_in(&data_dir(state));
    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<T> {
        let mut conn = db::open(&path)?;
        f(&mut conn)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))
}

/// Remembers when the window was last seen: when it is shown (the time it was last hidden becomes the
/// baseline of the "since your last visit" summary) and when it is hidden.
fn record_visit(app: &AppHandle, begin: bool) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    if lock(&state.settings).profile_id.is_empty() {
        return;
    }
    let path = db::db_path_in(&data_dir(&state));
    let result = db::open(&path).and_then(|conn| {
        if begin {
            insights::begin_visit(&conn, Utc::now())
        } else {
            insights::end_visit(&conn, Utc::now())
        }
    });
    if let Err(e) = result {
        eprintln!("visit record failed: {e:#}");
    }
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        // a window that is already on screen is not a new visit
        let seen = w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false);
        if !seen {
            record_visit(app, true);
        }
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn open_folder(app: &AppHandle, dir: &Path) -> CmdResult<()> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

fn apply_autostart(app: &AppHandle, enable: bool) {
    let mgr = app.autolaunch();
    let result = if enable {
        mgr.enable()
    } else if mgr.is_enabled().unwrap_or(false) {
        mgr.disable()
    } else {
        Ok(())
    };
    if let Err(e) = result {
        eprintln!("autostart update failed: {e}");
    }
}

// ------------------------------------------------------------------ commands

#[derive(Serialize)]
struct Overview {
    snapshot: Option<db::Snapshot>,
    deltas: Option<db::Deltas>,
    yearly: Vec<(i32, i64)>,
    snapshot_count: i64,
}

#[tauri::command]
async fn get_overview(state: State<'_, AppState>) -> CmdResult<Overview> {
    with_db(&state, |conn| {
        let now = Utc::now();
        let today = db::local_midnight_utc(Local::now());
        let snapshot = db::latest_snapshot(conn)?;
        let yearly = match &snapshot {
            Some(s) => db::yearly_for(conn, s.id)?,
            None => Vec::new(),
        };
        Ok(Overview {
            deltas: db::deltas(conn, today, now)?,
            yearly,
            snapshot_count: db::snapshot_count(conn)?,
            snapshot,
        })
    })
    .await
}

#[tauri::command]
async fn get_series(state: State<'_, AppState>) -> CmdResult<Vec<db::SeriesPoint>> {
    with_db(&state, |conn| db::time_series(conn)).await
}

#[tauri::command]
async fn get_articles(state: State<'_, AppState>) -> CmdResult<Vec<db::ArticleRow>> {
    with_db(&state, |conn| {
        db::articles_overview(conn, db::local_midnight_utc(Local::now()), Utc::now())
    })
    .await
}

#[tauri::command]
async fn get_article_history(
    state: State<'_, AppState>,
    key: String,
) -> CmdResult<Vec<(String, i64)>> {
    with_db(&state, move |conn| db::article_history(conn, &key)).await
}

#[tauri::command]
async fn get_article_changes(state: State<'_, AppState>, key: String) -> CmdResult<db::ArticleChanges> {
    with_db(&state, move |conn| db::article_changes(conn, &key, Utc::now())).await
}

#[tauri::command]
async fn get_recent_citations(state: State<'_, AppState>, limit: Option<usize>) -> CmdResult<Vec<insights::RecentItem>> {
    let limit = limit.unwrap_or(10).clamp(1, 100);
    with_db(&state, move |conn| insights::build_recent(conn, limit)).await
}

#[tauri::command]
async fn get_log(state: State<'_, AppState>) -> CmdResult<Vec<db::LogRow>> {
    with_db(&state, |conn| db::recent_log(conn, 200)).await
}

#[tauri::command]
fn get_status(state: State<'_, AppState>) -> scheduler::StatusInfo {
    state.sched.status()
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    lock(&state.settings).clone()
}

#[tauri::command]
fn refresh_now(state: State<'_, AppState>) -> bool {
    state.sched.trigger_now()
}

// ------------------------------------------------------- historical backfill

type BackfillState<'a> = State<'a, Arc<backfill::Shared>>;

#[derive(Serialize)]
struct BackfillInfo {
    enabled: bool,
    orcid_set: bool,
    summary: db::BackfillSummary,
    progress: backfill::Progress,
}

#[tauri::command]
async fn get_backfill_info(state: State<'_, AppState>, bf: BackfillState<'_>) -> CmdResult<BackfillInfo> {
    let (enabled, orcid_set) = {
        let s = lock(&state.settings);
        (s.backfill_enabled, !s.orcid.is_empty())
    };
    let summary = with_db(&state, |conn| db::backfill_summary(conn)).await?;
    Ok(BackfillInfo {
        enabled,
        orcid_set,
        summary,
        progress: bf.progress(),
    })
}

/// Asks the running scan to stop after the current request.
#[tauri::command]
fn cancel_backfill(bf: BackfillState<'_>) -> bool {
    bf.cancel()
}

/// Everything the scan panel needs to rebuild itself (after a reload or when it is reopened).
#[tauri::command]
fn get_scan_state(bf: BackfillState<'_>) -> backfill::ScanState {
    bf.scan_state()
}

/// Debug builds only: plays a scripted scan with invented articles (no network).
#[tauri::command]
async fn debug_scan_demo(app: AppHandle, bf: BackfillState<'_>) -> CmdResult<bool> {
    if !cfg!(debug_assertions) {
        return Err("debug builds only".into());
    }
    Ok(backfill::demo(app, bf.inner().clone()))
}

fn start_status(result: backfill::Start) -> &'static str {
    match result {
        backfill::Start::Started => "started",
        backfill::Start::AlreadyRunning => "running",
        backfill::Start::NoOrcid => "no_orcid",
    }
}

/// Manual run over every article. Answers "started", "running", "no_orcid" or "no_data".
#[tauri::command]
async fn backfill_run_now(app: AppHandle, state: State<'_, AppState>, bf: BackfillState<'_>) -> CmdResult<String> {
    let has_snapshot = with_db(&state, |conn| Ok(db::latest_snapshot(conn)?.is_some())).await?;
    if !has_snapshot {
        return Ok("no_data".into());
    }
    Ok(start_status(backfill::start_as(&app, &bf, backfill::Mode::All, true)).into())
}

#[derive(Serialize)]
struct EnabledResult {
    settings: Settings,
    /// "idle", "started", "running" or "no_orcid"
    status: String,
}

/// Switches the backfill option and saves it right away. Turning it on looks up the articles
/// that have no match yet (all of them the first time) when a snapshot already exists.
#[tauri::command]
async fn set_backfill_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    bf: BackfillState<'_>,
    enabled: bool,
) -> CmdResult<EnabledResult> {
    let new = {
        let mut s = lock(&state.settings);
        s.backfill_enabled = enabled;
        s.clone()
    };
    settings::save(&state.config_path, &new).map_err(|e| format!("{e:#}"))?;
    let mut status = "idle";
    if enabled {
        let has_snapshot = with_db(&state, |conn| Ok(db::latest_snapshot(conn)?.is_some())).await?;
        if new.orcid.is_empty() {
            status = "no_orcid";
        } else if has_snapshot {
            status = start_status(backfill::start_as(&app, &bf, backfill::Mode::Missing, true));
        }
    }
    Ok(EnabledResult {
        settings: new,
        status: status.into(),
    })
}

#[tauri::command]
async fn clear_backfill_data(app: AppHandle, state: State<'_, AppState>, bf: BackfillState<'_>) -> CmdResult<()> {
    if bf.is_running() {
        return Err(i18n::tr(&lang_of(&state), "bf.err_running").into());
    }
    with_db(&state, db::clear_backfill).await?;
    let _ = app.emit("backfill-updated", ());
    let _ = app.emit("data-updated", ());
    Ok(())
}

#[tauri::command]
async fn get_article_backfill(state: State<'_, AppState>, key: String) -> CmdResult<db::ArticleBackfill> {
    with_db(&state, move |conn| db::article_backfill(conn, &key)).await
}

// ------------------------------------------------- manual DOI / "not this one"

/// Answer of the override commands: "started", "running" or "no_orcid" (the single-article
/// lookup only starts with an ORCID iD and when no other lookup is active).
fn override_refuse_if_running(state: &AppState, bf: &backfill::Shared) -> CmdResult<()> {
    if bf.is_running() {
        return Err(i18n::tr(&lang_of(state), "bf.err_running").into());
    }
    Ok(())
}

/// Sets a manual DOI for one article and looks that article up again right away.
#[tauri::command]
async fn set_article_doi(
    app: AppHandle,
    state: State<'_, AppState>,
    bf: BackfillState<'_>,
    key: String,
    doi: String,
) -> CmdResult<String> {
    let Some(doi) = backfill::normalize_doi(&doi) else {
        return Err(i18n::tr(&lang_of(&state), "art.err_doi").into());
    };
    override_refuse_if_running(&state, &bf)?;
    let k = key.clone();
    with_db(&state, move |conn| db::set_override_doi(conn, &k, Some(&doi), Utc::now())).await?;
    let _ = app.emit("backfill-updated", ());
    Ok(start_status(backfill::start_as(&app, &bf, backfill::Mode::Article(key), true)).into())
}

/// "Not this one": rules the current match out for this article and looks it up again.
#[tauri::command]
async fn reject_article_match(
    app: AppHandle,
    state: State<'_, AppState>,
    bf: BackfillState<'_>,
    key: String,
) -> CmdResult<String> {
    override_refuse_if_running(&state, &bf)?;
    let k = key.clone();
    with_db(&state, move |conn| {
        let Some(m) = db::article_backfill(conn, &k)?.match_info else {
            return Ok(());
        };
        let ids: Vec<String> = [m.external_id, m.openalex_id].into_iter().flatten().collect();
        db::add_rejected(conn, &k, &ids, Utc::now())
    })
    .await?;
    let _ = app.emit("backfill-updated", ());
    Ok(start_status(backfill::start_as(&app, &bf, backfill::Mode::Article(key), true)).into())
}

/// Removes the manual DOI and the rejected matches of one article and matches it automatically again.
#[tauri::command]
async fn reset_article_override(
    app: AppHandle,
    state: State<'_, AppState>,
    bf: BackfillState<'_>,
    key: String,
) -> CmdResult<String> {
    override_refuse_if_running(&state, &bf)?;
    let k = key.clone();
    with_db(&state, move |conn| db::reset_override(conn, &k)).await?;
    let _ = app.emit("backfill-updated", ());
    Ok(start_status(backfill::start_as(&app, &bf, backfill::Mode::Article(key), true)).into())
}

#[tauri::command]
async fn get_health(state: State<'_, AppState>) -> CmdResult<scheduler::HealthInfo> {
    with_db(&state, |conn| scheduler::current_health(conn)).await
}

#[derive(Serialize)]
struct SaveResult {
    settings: Settings,
    message: String,
}

#[tauri::command]
async fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    bf: BackfillState<'_>,
    settings: Settings,
    clear_data: Option<bool>,
) -> CmdResult<SaveResult> {
    let mut new = settings;
    new.normalize(&state.default_dir);
    let old = lock(&state.settings).clone();
    // Messages use the language being saved, so the reply matches what the UI shows next.
    let lang = new.language.clone();
    let m = |key: &str| i18n::tr(&lang, key).to_string();

    if new.profile_id.is_empty() {
        if !old.profile_id.is_empty() {
            return Err(m("save.err_profile_empty"));
        }
    } else if !settings::valid_profile_id(&new.profile_id) {
        return Err(m("save.err_profile_chars"));
    }
    if !new.orcid.is_empty() && !settings::valid_orcid(&new.orcid) {
        return Err(m("save.err_orcid_invalid"));
    }
    let profile_changed = new.profile_id != old.profile_id;
    // The ORCID is required next to a profile: when one is set or changed, and once one is stored.
    // Settings files from before the field existed may stay without it until the user adds it.
    if !new.profile_id.is_empty() && new.orcid.is_empty() && (profile_changed || !old.orcid.is_empty()) {
        return Err(m("save.err_orcid_empty"));
    }
    // Setting the first profile (from empty) needs no confirmation: there is nothing to delete.
    let needs_clear = profile_changed && !old.profile_id.is_empty();
    if needs_clear {
        if !clear_data.unwrap_or(false) {
            return Err(m("save.err_confirm"));
        }
        if state.sched.status().state == "running" {
            return Err(m("save.err_running"));
        }
    }
    let orcid_changed = !needs_clear && !old.orcid.is_empty() && new.orcid != old.orcid;
    if (needs_clear || orcid_changed) && bf.is_running() {
        return Err(m("save.err_bf_running"));
    }
    if needs_clear {
        // The user chooses again (in the dialog that follows) whether to look up the history.
        new.backfill_enabled = false;
    }
    let mut notes: Vec<String> = Vec::new();

    if new.data_dir != old.data_dir {
        let (o, n) = (old.data_path(), new.data_path());
        let outcome = tauri::async_runtime::spawn_blocking(move || db::switch_data_dir(&o, &n))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| i18n::trf(&lang, "save.err_dir", &[&format!("{e:#}")]))?;
        notes.push(match outcome {
            db::SwitchOutcome::Copied => m("save.dir_copied"),
            db::SwitchOutcome::UsedExisting => m("save.dir_existing"),
            db::SwitchOutcome::EmptyStart => m("save.dir_empty"),
            db::SwitchOutcome::Unchanged => String::new(),
        });
    }
    if needs_clear {
        let path = db::db_path_in(&new.data_path());
        tauri::async_runtime::spawn_blocking(move || db::open(&path).and_then(|mut c| db::clear_all(&mut c)))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| i18n::trf(&lang, "save.err_clear", &[&format!("{e:#}")]))?;
        notes.push(m("save.profile_changed"));
    }
    if orcid_changed {
        // Matches were made through the old ORCID works; they would be wrong now.
        let path = db::db_path_in(&new.data_path());
        tauri::async_runtime::spawn_blocking(move || db::open(&path).and_then(|mut c| db::clear_backfill(&mut c)))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| i18n::trf(&lang, "save.err_clear", &[&format!("{e:#}")]))?;
        notes.push(m("save.orcid_changed"));
    }

    settings::save(&state.config_path, &new).map_err(|e| format!("{e:#}"))?;
    // Autostart is only registered once a profile exists (see setup()).
    apply_autostart(&app, new.autostart && !new.profile_id.is_empty());
    *lock(&state.settings) = new.clone();
    state.sched.reschedule();
    if new.backfill_enabled && !new.orcid.is_empty() && (!old.backfill_enabled || old.orcid != new.orcid) {
        backfill::start(&app, &bf, backfill::Mode::Missing);
    }
    apply_language(&app, &state);
    if let Err(e) = app.emit("data-updated", ()) {
        eprintln!("data-updated event failed: {e}");
    }
    let mut message = m("save.saved");
    for n in notes.into_iter().filter(|n| !n.is_empty()) {
        message.push(' ');
        message.push_str(&n);
    }
    Ok(SaveResult {
        settings: new,
        message,
    })
}

fn refresh_tooltip(app: &AppHandle, state: &AppState) {
    let path = db::db_path_in(&data_dir(state));
    let total = if path.exists() {
        db::open(&path)
            .and_then(|c| db::latest_snapshot(&c))
            .ok()
            .flatten()
            .map(|s| s.cit_all)
    } else {
        None
    };
    scheduler::set_tray_tooltip(app, &lang_of(state), total);
}

#[tauri::command]
async fn pick_folder(app: AppHandle) -> Option<String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_folder(move |p| {
        let _ = tx.send(p);
    });
    rx.await
        .ok()
        .flatten()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
fn open_data_folder(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    open_folder(&app, &data_dir(&state))
}

/// Pages the app may open in the default browser. The app itself never fetches any of them;
/// in particular Scholar's cited-by search is only opened for the user.
fn url_allowed(url: &str) -> bool {
    const ALLOWED: [&str; 11] = [
        "https://github.com/bcankara/scholar-citation-tracker",
        "https://scholar.google.com/citations",
        "https://scholar.google.com/scholar",
        "https://openalex.org/",
        "https://doi.org/",
        "https://orcid.org/",
        "https://docs.openalex.org/",
        "https://creativecommons.org/licenses/by/4.0/",
        "https://www.sqlite.org/copyright.html",
        // Repositories of the Rust libraries listed on the Credits page.
        "https://github.com/",
        "https://gitlab.com/",
    ];
    // Windows' Installed apps page, for the "Uninstall..." button (the app never uninstalls itself).
    const EXACT: [&str; 1] = ["ms-settings:appsfeatures"];
    ALLOWED.iter().any(|p| url.starts_with(p)) || EXACT.contains(&url)
}

#[tauri::command]
fn open_url(app: AppHandle, state: State<'_, AppState>, url: String) -> CmdResult<()> {
    if !url_allowed(&url) {
        return Err(i18n::tr(&lang_of(&state), "cmd.err_open_url").into());
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn export_csv(
    app: AppHandle,
    state: State<'_, AppState>,
    kind: String,
) -> CmdResult<Option<String>> {
    let lang = lang_of(&state);
    let snapshots = match kind.as_str() {
        "snapshots" => true,
        "articles" => false,
        _ => return Err(i18n::tr(&lang, "cmd.err_export_kind").into()),
    };
    let default_name = i18n::tr(&lang, if snapshots { "cmd.csv_snapshots" } else { "cmd.csv_articles" });
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .add_filter("CSV", &["csv"])
        .set_file_name(default_name)
        .save_file(move |p| {
            let _ = tx.send(p);
        });
    let Some(path) = rx.await.ok().flatten().and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    let target = path.clone();
    with_db(&state, move |conn| {
        let file = std::io::BufWriter::new(std::fs::File::create(&target)?);
        if snapshots {
            db::write_snapshots_csv(conn, file, &lang)
        } else {
            db::write_articles_csv(conn, file, &lang)
        }
    })
    .await?;
    Ok(Some(path.to_string_lossy().to_string()))
}


// ------------------------------------------------- citing works and insights

/// All citing works for the list page; `self_cite` / `coauthor_cite` follow the user's ORCID iD and profile name.
#[tauri::command]
async fn get_citing_works(state: State<'_, AppState>) -> CmdResult<Vec<db::CitingRow>> {
    let orcid = lock(&state.settings).orcid.clone();
    with_db(&state, move |conn| {
        let mut rows = db::citing_works(conn)?;
        let flags = citers::flags(conn, &orcid, Local::now().year())?;
        for r in &mut rows {
            if let Some((s, c)) = flags.get(&r.citing_id) {
                r.self_cite = *s;
                r.coauthor_cite = *c;
            }
        }
        Ok(rows)
    })
    .await
}

/// The period / article filter of the "Citing works" tab of the Analysis page: `from` / `to` are
/// local days (`YYYY-MM-DD`, inclusive, empty = open), `article` an article key or empty.
fn citers_filter(from: String, to: String, article: String, exclude_self: bool) -> Result<citers::Filter, String> {
    for d in [&from, &to] {
        if !d.is_empty() && !valid_day(d) {
            return Err("invalid date".into());
        }
    }
    Ok(citers::Filter { from, to, article, exclude_self })
}

macro_rules! citers_command {
    ($(#[$m:meta])* $name:ident, $build:path, $ret:ty) => {
        $(#[$m])*
        #[tauri::command]
        async fn $name(state: State<'_, AppState>, from: String, to: String, article: String, exclude_self: bool) -> CmdResult<$ret> {
            let filter = citers_filter(from, to, article, exclude_self)?;
            let orcid = lock(&state.settings).orcid.clone();
            with_db(&state, move |conn| $build(conn, &orcid, Local::now().year(), filter)).await
        }
    };
}

citers_command!(
    /// Filter bar of the tab: filtered count, article dropdown, ORCID / enrichment state.
    get_citers_meta, citers::build_meta, citers::Meta
);
citers_command!(
    /// Countries: all countries with counts, the year-by-year first-time series, the new ones.
    get_citers_countries, citers::build_countries, citers::EntityReport
);
citers_command!(
    /// Institutions: all institutions with counts and country, the new ones, the year-by-year series.
    get_citers_institutions, citers::build_institutions, citers::EntityReport
);
citers_command!(
    /// Venues, publishers (with source types) and the source-type split.
    get_citers_venues, citers::build_venues, citers::Venues
);
citers_command!(
    /// Domain > field > topics tree and the article x field matrix.
    get_citers_fields, citers::build_fields, citers::Fields
);
citers_command!(
    /// Work types, languages and open-access status.
    get_citers_types, citers::build_types, citers::Types
);
citers_command!(
    /// Self / co-author / external split and the top citing authors.
    get_citers_who, citers::build_who, citers::Who
);
citers_command!(
    /// The most cited citing works.
    get_citers_influential, citers::build_influential, citers::Influential
);

citers_command!(
    /// Citing works by the OJP of their journal: histogram, coverage and the top 8 journals.
    get_citers_ojp, citers::build_ojp, citers::OjpReport
);

/// Constants of the OJP indicator (edition, window, thresholds, counts, dataset version and DOI).
#[tauri::command]
fn get_ojp_meta() -> ojp::Meta {
    ojp::meta()
}

/// Attribution and statement of changes of the OJP data (CC BY 4.0), plain text.
#[tauri::command]
fn get_ojp_notice() -> &'static str {
    ojp::NOTICE
}

/// The merged trend line (history scaled to the first snapshot, snapshots, forecast).
#[tauri::command]
async fn get_trend(state: State<'_, AppState>) -> CmdResult<insights::Trend> {
    with_db(&state, |conn| insights::build_trend(conn, Utc::now(), Local::now().date_naive())).await
}

/// Everything the Analysis page draws, from one place.
#[tauri::command]
async fn get_analysis(state: State<'_, AppState>) -> CmdResult<insights::Analysis> {
    with_db(&state, |conn| insights::build_analysis(conn, Utc::now(), Local::now().date_naive(), None)).await
}

fn valid_day(s: &str) -> bool {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
}

/// Per-day aggregates of both sources for the calendar (`from` .. `to` inclusive, local days).
#[tauri::command]
async fn get_calendar_days(state: State<'_, AppState>, from: String, to: String, source: String, article: String) -> CmdResult<insights::CalendarDays> {
    if !valid_day(&from) || !valid_day(&to) {
        return Err("invalid date".into());
    }
    with_db(&state, move |conn| insights::build_calendar_days(conn, &from, &to, &source, &article)).await
}

/// Everything of a day or a range: Scholar changes (with probable OpenAlex works) and citing works.
#[tauri::command]
async fn get_day_detail(state: State<'_, AppState>, from: String, to: String, source: String, article: String) -> CmdResult<Vec<insights::DayDetail>> {
    if !valid_day(&from) || !valid_day(&to) {
        return Err("invalid date".into());
    }
    with_db(&state, move |conn| insights::build_day_detail(conn, &from, &to, &source, &article)).await
}

/// The active OpenAlex daily-budget pause (resume time still ahead), if any; `has_key` follows the key stored now.
#[tauri::command]
async fn get_budget_pause(state: State<'_, AppState>) -> CmdResult<Option<backfill::BudgetPause>> {
    let has_key = tauri::async_runtime::spawn_blocking(secrets::load_keys).await.map(|k| k.openalex.is_some()).unwrap_or(false);
    with_db(&state, move |conn| {
        Ok(backfill::pause_active(conn, Utc::now())?.map(|mut p| {
            p.has_key = has_key;
            p
        }))
    })
    .await
}

/// "Since your last visit": the facts of the one-sentence summary on the Panel.
#[tauri::command]
async fn get_visit_summary(state: State<'_, AppState>) -> CmdResult<Option<insights::VisitSummary>> {
    with_db(&state, |conn| insights::build_visit_summary(conn)).await
}

/// Stores the citation goal (`None` or a non-positive number removes it).
#[tauri::command]
async fn set_goal(state: State<'_, AppState>, goal: Option<i64>) -> CmdResult<()> {
    let lang = lang_of(&state);
    if goal.is_some_and(|g| g > 1_000_000_000) {
        return Err(i18n::tr(&lang, "cmd.err_goal").into());
    }
    with_db(&state, move |conn| match goal.filter(|g| *g > 0) {
        Some(g) => db::set_state(conn, db::STATE_GOAL, &g.to_string()),
        None => db::delete_state(conn, db::STATE_GOAL),
    })
    .await
}

#[tauri::command]
async fn get_risers(state: State<'_, AppState>, days: i64) -> CmdResult<insights::Risers> {
    let days = if days >= 90 { 90 } else { 30 };
    with_db(&state, move |conn| insights::build_risers(conn, Utc::now(), days)).await
}

#[tauri::command]
async fn get_next_steps(state: State<'_, AppState>) -> CmdResult<Option<insights::NextSteps>> {
    with_db(&state, |conn| insights::build_next_steps(conn)).await
}

#[tauri::command]
async fn get_distribution(state: State<'_, AppState>) -> CmdResult<insights::Distribution> {
    with_db(&state, |conn| insights::build_distribution(conn)).await
}

// ----------------------------------------------------------- weekly summary

/// Monday summary: once per ISO week, on the first check at or after 09:00 local time.
async fn weekly_check(app: &AppHandle, state: &AppState) {
    use chrono::Weekday;
    let (enabled, lang, path) = {
        let s = lock(&state.settings);
        (
            s.notify && s.weekly_summary && !s.profile_id.is_empty(),
            s.language.clone(),
            db::db_path_in(&s.data_path()),
        )
    };
    let now_local = Local::now().naive_local();
    // Cheap exit before touching the database: the rule only ever fires on Mondays from 09:00.
    if !enabled || now_local.weekday() != Weekday::Mon || now_local.hour() < 9 || !path.exists() {
        return;
    }
    let found = tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<Option<insights::WeekSummary>> {
        let conn = db::open(&path)?;
        let last = db::get_state(&conn, db::STATE_WEEKLY)?;
        let summary = insights::build_week_summary(&conn, Utc::now())?;
        Ok(match insights::weekly_decision(now_local, last.as_deref(), true, summary) {
            insights::Weekly::Show { week, summary } => {
                db::set_state(&conn, db::STATE_WEEKLY, &week)?;
                Some(summary)
            }
            insights::Weekly::Skip(_) => None,
        })
    })
    .await;
    match found {
        Ok(Ok(Some(sum))) => {
            use tauri_plugin_notification::NotificationExt;
            let body = match &sum.top {
                Some((title, gain)) => i18n::trf(
                    &lang,
                    "notif.weekly",
                    &[&sum.gain.to_string(), &db::shorten(title, 60), &gain.to_string()],
                ),
                None => i18n::trf(&lang, "notif.weekly_plain", &[&sum.gain.to_string()]),
            };
            if let Err(e) = app.notification().builder().title(i18n::product_name(&lang)).body(body).show() {
                eprintln!("notification failed: {e}");
            }
        }
        Ok(Ok(None)) => {}
        Ok(Err(e)) => eprintln!("weekly summary failed: {e:#}"),
        Err(e) => eprintln!("weekly summary task failed: {e}"),
    }
}

async fn weekly_loop(app: AppHandle, state: AppState) {
    tokio::time::sleep(std::time::Duration::from_secs(45)).await;
    loop {
        weekly_check(&app, &state).await;
        tokio::time::sleep(std::time::Duration::from_secs(600)).await;
    }
}

// ------------------------------------------------------------------ API keys

fn key_kind(state: &AppState, which: &str) -> CmdResult<secrets::KeyKind> {
    secrets::KeyKind::parse(which).ok_or_else(|| i18n::tr(&lang_of(state), "key.err_name").to_string())
}

#[tauri::command]
async fn api_key_status() -> CmdResult<secrets::KeyStatus> {
    tauri::async_runtime::spawn_blocking(secrets::status)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_api_key(state: State<'_, AppState>, which: String, key: String) -> CmdResult<secrets::KeyStatus> {
    let kind = key_kind(&state, &which)?;
    let lang = lang_of(&state);
    if !secrets::plausible_key(&key) {
        return Err(i18n::tr(&lang, "key.err_format").into());
    }
    let status = tauri::async_runtime::spawn_blocking(move || {
        secrets::store(kind, &key).map_err(|e| i18n::trf(&lang, "key.err_store", &[&e]))?;
        Ok::<_, String>(secrets::status())
    })
    .await
    .map_err(|e| e.to_string())??;
    // a new key has a budget of its own: a pause caused by the old (keyless) budget ends
    let _ = with_db(&state, |conn| backfill::clear_pause(conn)).await;
    Ok(status)
}

#[tauri::command]
async fn delete_api_key(state: State<'_, AppState>, which: String) -> CmdResult<secrets::KeyStatus> {
    let kind = key_kind(&state, &which)?;
    let lang = lang_of(&state);
    tauri::async_runtime::spawn_blocking(move || {
        secrets::delete(kind).map_err(|e| i18n::trf(&lang, "key.err_store", &[&e]))?;
        Ok(secrets::status())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// One cheap request with the typed key, or with the stored key when `key` is empty.
#[tauri::command]
async fn test_api_key(state: State<'_, AppState>, which: String, key: Option<String>) -> CmdResult<secrets::TestResult> {
    let kind = key_kind(&state, &which)?;
    let lang = lang_of(&state);
    let typed = key.map(|k| k.trim().to_string()).filter(|k| !k.is_empty());
    let key = match typed {
        Some(k) => k,
        None => tauri::async_runtime::spawn_blocking(move || secrets::load(kind))
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| i18n::tr(&lang, "key.err_nokey").to_string())?,
    };
    if !secrets::plausible_key(&key) {
        return Ok(secrets::TestResult { status: "invalid", detail: String::new() });
    }
    Ok(secrets::test_key(kind, &key).await)
}

/// Asks OpenAlex for a free key by e-mail. The address is sent once, to OpenAlex, and not kept.
#[tauri::command]
async fn request_openalex_key(state: State<'_, AppState>, email: String) -> CmdResult<String> {
    let lang = lang_of(&state);
    if !secrets::plausible_email(&email) {
        return Err(i18n::tr(&lang, "key.err_email").into());
    }
    secrets::request_openalex_key(&email)
        .await
        .map_err(|e| i18n::trf(&lang, "key.err_request", &[&e]))
}

// ------------------------------------------------------- Excel and PDF export

async fn pick_save_path(app: &AppHandle, filter: (&str, &[&str]), default_name: &str) -> Option<PathBuf> {
    // Debug builds only: lets a scripted UI test save into a folder without the native dialog.
    #[cfg(debug_assertions)]
    if let Ok(dir) = std::env::var("SCT_TEST_SAVE_DIR") {
        return Some(PathBuf::from(dir).join(default_name));
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .add_filter(filter.0, filter.1)
        .set_file_name(default_name)
        .save_file(move |p| {
            let _ = tx.send(p);
        });
    rx.await.ok().flatten().and_then(|p| p.into_path().ok())
}

#[tauri::command]
async fn export_xlsx(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Option<String>> {
    let lang = lang_of(&state);
    let Some(path) = pick_save_path(&app, ("Excel", &["xlsx"]), i18n::tr(&lang, "cmd.xlsx_name")).await else {
        return Ok(None);
    };
    let target = path.clone();
    let orcid = lock(&state.settings).orcid.clone();
    with_db(&state, move |conn| export::write_xlsx(conn, &target, &lang, &orcid)).await?;
    Ok(Some(path.to_string_lossy().to_string()))
}

#[tauri::command]
fn report_payload(job: State<'_, Arc<report::Job>>) -> Option<String> {
    lock(&job.payload).clone()
}

#[tauri::command]
fn report_ready(job: State<'_, Arc<report::Job>>) {
    if let Some(tx) = lock(&job.ready).take() {
        let _ = tx.send(());
    }
}

/// Builds the report data, asks where to save, and prints the report page to a PDF.
#[tauri::command]
async fn create_report_pdf(
    app: AppHandle,
    state: State<'_, AppState>,
    job: State<'_, Arc<report::Job>>,
    options: report::ReportOptions,
) -> CmdResult<Option<String>> {
    let ui_lang = lang_of(&state);
    let lang = i18n::normalize_lang(&options.lang).to_string();
    let orcid = lock(&state.settings).orcid.clone();
    let generated = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let opts = options.clone();
    let built = with_db(&state, move |conn| Ok(report::build(conn, &opts, &orcid, &generated))).await?;
    let data = match built {
        Ok(d) => d,
        Err(report::ReportError::Range) => return Err(i18n::tr(&ui_lang, "cmd.err_report_range").into()),
        Err(report::ReportError::NoData) => return Err(i18n::tr(&ui_lang, "cmd.err_report_nodata").into()),
        Err(report::ReportError::Other(e)) => return Err(e),
    };
    let json = serde_json::to_string(&data).map_err(|e| e.to_string())?;
    // Debug builds only (UI tests cannot drive the native save dialog): SCT_TEST_PDF_PATH names the file.
    #[cfg(debug_assertions)]
    let forced = std::env::var("SCT_TEST_PDF_PATH").ok().filter(|p| !p.is_empty()).map(PathBuf::from);
    #[cfg(not(debug_assertions))]
    let forced: Option<PathBuf> = None;
    let path = match forced {
        Some(p) => p,
        None => match pick_save_path(&app, ("PDF", &["pdf"]), i18n::tr(&lang, "cmd.pdf_name")).await {
            Some(p) => p,
            None => return Ok(None),
        },
    };
    pdf::print_report(&app, job.inner(), json, &path)
        .await
        .map_err(|e| i18n::trf(&ui_lang, "cmd.err_report_print", &[&secrets::redact(&e, &[])]))?;
    Ok(Some(path.to_string_lossy().to_string()))
}

// --------------------------------------------------------------- entry point

/// `--once PROFILE_ID_OR_URL`: one live fetch that prints parsed totals (smoke test).
fn run_once() {
    let arg = std::env::args()
        .skip_while(|a| a != "--once")
        .nth(1)
        .filter(|a| !a.starts_with("--"));
    let Some(arg) = arg else {
        eprintln!("usage: scholar-citation-tracker --once <profile id or profile URL>");
        return;
    };
    let profile_id = settings::extract_profile_id(&arg);
    if !settings::valid_profile_id(&profile_id) {
        eprintln!("invalid profile id: {profile_id}");
        return;
    }
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("runtime error: {e}");
            return;
        }
    };
    match rt.block_on(scholar::fetch_profile(&profile_id)) {
        Ok(p) => {
            println!("name: {}", p.name);
            println!(
                "citations: {} / {} (since {})",
                p.stats.cit_all, p.stats.cit_since, p.stats.since_year
            );
            println!("h-index: {} / {}", p.stats.h_all, p.stats.h_since);
            println!("i10-index: {} / {}", p.stats.i10_all, p.stats.i10_since);
            println!("articles: {}", p.articles.len());
            println!("yearly: {:?}", p.yearly);
        }
        Err(e) => eprintln!("fetch failed [{}]: {}", e.status(), e.message(i18n::EN)),
    }
}

pub fn run() {
    if std::env::args().any(|a| a == "--once") {
        run_once();
        return;
    }
    let hidden = std::env::args().any(|a| a == "--hidden");

    let result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(move |app| {
            let handle = app.handle().clone();
            let config_dir = app.path().app_config_dir()?;
            let docs = app
                .path()
                .document_dir()
                .or_else(|_| app.path().home_dir())?;
            let default_dir = docs.join("ScholarTakip");
            let config_path = config_dir.join("settings.json");

            let loaded = settings::load(&config_path, &default_dir);
            if !config_path.exists() {
                if let Err(e) = settings::save(&config_path, &loaded) {
                    eprintln!("settings save failed: {e:#}");
                }
            }
            // Do not register autostart before the user has chosen a profile.
            if !loaded.profile_id.is_empty() {
                apply_autostart(&handle, loaded.autostart);
            }
            let lang = loaded.language.clone();
            let no_profile = loaded.profile_id.is_empty();

            let shared_settings = Arc::new(Mutex::new(loaded));
            let sched = scheduler::Shared::new(shared_settings.clone());
            let bf_shared = backfill::Shared::new(shared_settings.clone());
            let data_path = lock(&shared_settings).data_path();
            backfill::recover_stale_runs(&db::db_path_in(&data_path), &lang);
            backfill::drop_legacy_source(&db::db_path_in(&data_path));
            app.manage(bf_shared);
            app.manage(Arc::new(report::Job::default()));
            app.manage(Arc::new(updater::Pending::default()));
            let state = AppState {
                settings: shared_settings,
                config_path: Arc::new(config_path),
                default_dir: Arc::new(default_dir),
                sched: sched.clone(),
            };
            app.manage(state.clone());

            // Tray
            let open_i = MenuItem::with_id(app, "open", i18n::tr(&lang, "tray.open"), true, None::<&str>)?;
            let refresh_i = MenuItem::with_id(
                app,
                "refresh",
                i18n::tr(&lang, "tray.refresh"),
                !no_profile,
                None::<&str>,
            )?;
            let folder_i =
                MenuItem::with_id(app, "folder", i18n::tr(&lang, "tray.folder"), true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", i18n::tr(&lang, "tray.quit"), true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(app, &[&open_i, &refresh_i, &folder_i, &sep, &quit_i])?;
            app.manage(TrayItems {
                open: open_i,
                refresh: refresh_i,
                folder: folder_i,
                quit: quit_i,
            });
            let icon = app
                .default_window_icon()
                .cloned()
                .ok_or("application icon not found")?;
            let menu_state = state.clone();
            TrayIconBuilder::with_id("main")
                .icon(icon)
                .tooltip(i18n::product_name(&lang))
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "open" => show_main(app),
                    "refresh" => {
                        menu_state.sched.trigger_now();
                    }
                    "folder" => {
                        if let Err(e) = open_folder(app, &data_dir(&menu_state)) {
                            eprintln!("open folder failed: {e}");
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main(tray.app_handle());
                    }
                })
                .build(app)?;
            apply_language(&handle, &state);

            if let Some(w) = app.get_webview_window("main") {
                use tauri::window::{Effect, EffectsBuilder};
                match w.set_effects(EffectsBuilder::new().effect(Effect::Mica).build()) {
                    Ok(()) => MICA_ACTIVE.store(true, std::sync::atomic::Ordering::SeqCst),
                    Err(e) => eprintln!("mica unavailable: {e}"),
                }
            }

            // Without a profile the window always opens, even for an autostart (--hidden) launch.
            if !hidden || no_profile {
                show_main(&handle);
            }

            // Resume articles an earlier session left unmatched (no-op when backfill is off).
            let kick_app = handle.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(20)).await;
                backfill::kick_after_snapshot(&kick_app);
            });
            tauri::async_runtime::spawn(updater::run_auto(handle.clone(), state.settings.clone()));
            tauri::async_runtime::spawn(backfill::refresh_loop(handle.clone()));
            tauri::async_runtime::spawn(backfill::budget_resume_loop(handle.clone()));
            tauri::async_runtime::spawn(weekly_loop(handle.clone(), state.clone()));
            tauri::async_runtime::spawn(scheduler::run(handle, sched));
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
                record_visit(window.app_handle(), false);
            }
        })
        .invoke_handler(tauri::generate_handler![
            backdrop_mica,
            get_overview,
            get_series,
            get_articles,
            get_article_history,
            get_article_changes,
            get_recent_citations,
            cancel_backfill,
            get_scan_state,
            debug_scan_demo,
            get_log,
            get_status,
            get_settings,
            save_settings,
            refresh_now,
            get_backfill_info,
            backfill_run_now,
            set_backfill_enabled,
            clear_backfill_data,
            get_article_backfill,
            pick_folder,
            open_data_folder,
            open_url,
            export_csv,
            set_article_doi,
            reject_article_match,
            reset_article_override,
            get_health,
            get_citing_works,
            get_citers_meta,
            get_citers_countries,
            get_citers_institutions,
            get_citers_venues,
            get_citers_fields,
            get_citers_types,
            get_citers_who,
            get_citers_influential,
            get_citers_ojp,
            get_ojp_meta,
            get_ojp_notice,
            get_trend,
            get_analysis,
            get_visit_summary,
            get_calendar_days,
            get_budget_pause,
            get_day_detail,
            set_goal,
            get_risers,
            get_next_steps,
            get_distribution,
            export_xlsx,
            create_report_pdf,
            report_payload,
            report_ready,
            api_key_status,
            save_api_key,
            delete_api_key,
            test_api_key,
            request_openalex_key,
            updater::check_for_updates,
            updater::get_pending_update,
            updater::install_update,
            updater::app_version,
        ])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("application failed to start: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::url_allowed;

    #[test]
    fn open_url_allow_list() {
        assert!(url_allowed("https://scholar.google.com/scholar?oi=bibs&hl=en&cites=1,2&scisbd=1"));
        assert!(url_allowed("https://scholar.google.com/citations?view_op=view_citation&user=x"));
        assert!(url_allowed("https://doi.org/10.1234/abc"));
        assert!(url_allowed("https://creativecommons.org/licenses/by/4.0/"));
        assert!(url_allowed("https://www.sqlite.org/copyright.html"));
        assert!(url_allowed("https://github.com/serde-rs/serde"));
        assert!(!url_allowed("https://creativecommons.org/"));
        assert!(!url_allowed("https://scholar.google.com/"));
        assert!(!url_allowed("http://scholar.google.com/scholar?x=1"));
        assert!(!url_allowed("javascript:alert(1)"));
        assert!(url_allowed("ms-settings:appsfeatures"));
        assert!(!url_allowed("ms-settings:privacy"));
        assert!(!url_allowed("ms-settings:appsfeatures?x=1"));
    }
}
