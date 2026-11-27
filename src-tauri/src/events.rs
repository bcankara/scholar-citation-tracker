//! Notification centre: the app records what happened (Scholar changes, new citing works, an
//! update, a health warning, the OpenAlex budget) in the database and tells the window, whether
//! or not the Windows / macOS notification for it is switched on.

use std::path::Path;

use chrono::Utc;
use tauri::{AppHandle, Emitter};

use crate::db;

/// Records one event with its own connection and announces it to the window. Blocking: call it
/// from a blocking context.
pub fn record(app: &AppHandle, db_path: &Path, kind: &str, uniq: &str, data: &serde_json::Value) {
    match db::open(db_path).and_then(|c| db::record_event(&c, Utc::now(), kind, uniq, data)) {
        Ok(true) => announce(app),
        Ok(false) => {}
        Err(e) => eprintln!("event not recorded: {e:#}"),
    }
}

/// Tells the window that the list changed (it reloads the badge and the panel).
pub fn announce(app: &AppHandle) {
    if let Err(e) = app.emit("events-changed", ()) {
        eprintln!("events-changed event failed: {e}");
    }
}

/// "Release notes" excerpt kept with an update event: the first lines, at most this many characters.
const NOTES_EXCERPT: usize = 400;

/// The first part of the release notes, cut at a line end where possible.
pub fn notes_excerpt(notes: &str) -> String {
    let notes = notes.trim();
    if notes.chars().count() <= NOTES_EXCERPT {
        return notes.to_string();
    }
    let cut: String = notes.chars().take(NOTES_EXCERPT).collect();
    let cut = match cut.rfind('\n') {
        Some(i) if i > NOTES_EXCERPT / 2 => cut[..i].to_string(),
        _ => cut,
    };
    format!("{}…", cut.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_notes_stay_whole_long_ones_are_cut_at_a_line() {
        assert_eq!(notes_excerpt("  one\ntwo \n"), "one\ntwo");
        let long = format!("{}\n{}", "a".repeat(300), "b".repeat(300));
        let cut = notes_excerpt(&long);
        assert!(cut.ends_with('…') && cut.chars().count() <= 301, "{cut}");
        assert!(!cut.contains('b'), "cut at the line end");
        let one_line = "x".repeat(900);
        assert_eq!(notes_excerpt(&one_line).chars().count(), 401);
    }
}
