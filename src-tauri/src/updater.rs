//! In-app updates through the Tauri updater plugin.
//!
//! The app checks the GitHub release feed (`latest.json`) about a minute after start and then
//! once a day (when "check automatically" is on). Background failures are silent; only the
//! manual check from Settings reports its result. Installing downloads the signed installer,
//! verifies the signature with the public key in `tauri.conf.json` and starts it in passive
//! mode, which closes the app and restarts it when done.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::scheduler::lock;
use crate::settings::Settings;

/// First automatic check after start.
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(60);
/// Period of the automatic checks.
const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// Update found by the last check, waiting for the user to install it.
#[derive(Default)]
pub struct Pending {
    update: Mutex<Option<Update>>,
    installing: AtomicBool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct UpdateInfo {
    pub version: String,
    pub current: String,
    /// Release notes (the release body), if any.
    pub notes: Option<String>,
    pub date: Option<String>,
}

fn info_of(u: &Update) -> UpdateInfo {
    UpdateInfo {
        version: u.version.clone(),
        current: u.current_version.clone(),
        notes: u.body.clone().filter(|b| !b.trim().is_empty()),
        date: u.date.map(|d| d.to_string()),
    }
}

/// Result of a check, as the UI needs it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CheckResult {
    /// "available" | "current" | "not_found" | "error"
    pub status: &'static str,
    pub info: Option<UpdateInfo>,
    pub error: Option<String>,
}

/// Maps a failed check to a result. A missing release feed (no release published yet, HTTP 404)
/// is not an error worth alarming about.
fn failure(e: &tauri_plugin_updater::Error) -> CheckResult {
    let status = if matches!(e, tauri_plugin_updater::Error::ReleaseNotFound) {
        "not_found"
    } else {
        "error"
    };
    CheckResult {
        status,
        info: None,
        error: Some(e.to_string()),
    }
}

/// Asks the feed for a newer version. A found update is kept for `install_update` and announced
/// with the `update-available` event.
pub async fn check(app: &AppHandle) -> CheckResult {
    let updater = match app.updater() {
        Ok(u) => u,
        Err(e) => return failure(&e),
    };
    let pending = app.state::<Arc<Pending>>();
    match updater.check().await {
        Ok(Some(update)) => {
            let info = info_of(&update);
            *lock(&pending.update) = Some(update);
            if let Err(e) = app.emit("update-available", &info) {
                eprintln!("update-available event failed: {e}");
            }
            CheckResult {
                status: "available",
                info: Some(info),
                error: None,
            }
        }
        Ok(None) => {
            *lock(&pending.update) = None;
            CheckResult {
                status: "current",
                info: None,
                error: None,
            }
        }
        Err(e) => failure(&e),
    }
}

/// Background loop: first check after a minute, then daily. Never reports failures.
pub async fn run_auto(app: AppHandle, settings: Arc<Mutex<Settings>>) {
    tokio::time::sleep(FIRST_CHECK_DELAY).await;
    loop {
        if lock(&settings).auto_update {
            let r = check(&app).await;
            if r.status == "error" {
                eprintln!("automatic update check failed: {}", r.error.unwrap_or_default());
            }
        }
        tokio::time::sleep(CHECK_EVERY).await;
    }
}

#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> CheckResult {
    check(&app).await
}

#[tauri::command]
pub fn get_pending_update(pending: State<'_, Arc<Pending>>) -> Option<UpdateInfo> {
    lock(&pending.update).as_ref().map(info_of)
}

#[derive(Clone, Serialize)]
struct Progress {
    downloaded: u64,
    total: Option<u64>,
    /// True once the download finished and the installer is about to start.
    done: bool,
}

/// Downloads and installs the pending update, emitting `update-progress`. On Windows the
/// installer takes over and ends this process, so this normally does not return on success.
#[tauri::command]
pub async fn install_update(app: AppHandle, pending: State<'_, Arc<Pending>>) -> Result<(), String> {
    let Some(update) = lock(&pending.update).clone() else {
        return Err("no update pending".to_string());
    };
    if pending.installing.swap(true, Ordering::SeqCst) {
        return Err("update already in progress".to_string());
    }
    let mut downloaded: u64 = 0;
    let progress_app = app.clone();
    let done_app = app.clone();
    let result = update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = progress_app.emit(
                    "update-progress",
                    Progress {
                        downloaded,
                        total,
                        done: false,
                    },
                );
            },
            move || {
                let _ = done_app.emit(
                    "update-progress",
                    Progress {
                        downloaded: 0,
                        total: None,
                        done: true,
                    },
                );
            },
        )
        .await;
    pending.installing.store(false, Ordering::SeqCst);
    result.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}
