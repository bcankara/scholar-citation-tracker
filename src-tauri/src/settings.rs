//! User settings stored as JSON in the Tauri app config directory.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::i18n;

pub const THEME_SYSTEM: &str = "system";

/// Maps any stored value to "system", "light" or "dark"; unknown values mean "system".
pub fn normalize_theme(theme: &str) -> &'static str {
    match theme.trim().to_ascii_lowercase().as_str() {
        "light" => "light",
        "dark" => "dark",
        _ => THEME_SYSTEM,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Empty until the user sets a profile (first-run setup).
    pub profile_id: String,
    /// UI / backend language: "tr" or "en".
    pub language: String,
    /// Colour theme: "system" (follow the OS, the default), "light" or "dark".
    pub theme: String,
    /// Translucent window background (Mica on Windows 11, vibrancy on macOS). On by default;
    /// only has an effect where the system supports it.
    pub backdrop: bool,
    pub data_dir: String,
    pub interval_hours: u32,
    pub autostart: bool,
    pub notify: bool,
    /// Historical backfill (OpenAlex). Off for settings files without the field.
    pub backfill_enabled: bool,
    /// ORCID iD of the profile owner, `0000-0000-0000-0000` form. Empty in settings files
    /// written before the field existed; only the historical backfill needs it.
    pub orcid: String,
    /// Check for app updates in the background (after start and then daily).
    pub auto_update: bool,
    /// Monday summary notification ("+N citations this week"). Needs `notify`.
    pub weekly_summary: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            profile_id: String::new(),
            language: i18n::system_lang().to_string(),
            theme: THEME_SYSTEM.to_string(),
            backdrop: true,
            data_dir: String::new(),
            interval_hours: 3,
            autostart: true,
            notify: true,
            backfill_enabled: false,
            orcid: String::new(),
            auto_update: true,
            weekly_summary: true,
        }
    }
}

impl Settings {
    /// Clamps values and fills in defaults. `default_dir` is used when `data_dir` is empty.
    pub fn normalize(&mut self, default_dir: &Path) {
        self.profile_id = extract_profile_id(&self.profile_id);
        self.orcid = extract_orcid(&self.orcid);
        self.language = match self.language.trim().to_ascii_lowercase().as_str() {
            "tr" => i18n::TR.to_string(),
            "en" => i18n::EN.to_string(),
            _ => i18n::system_lang().to_string(),
        };
        self.theme = normalize_theme(&self.theme).to_string();
        self.interval_hours = self.interval_hours.clamp(1, 24);
        self.data_dir = self.data_dir.trim().to_string();
        if self.data_dir.is_empty() {
            self.data_dir = default_dir.to_string_lossy().to_string();
        }
    }

    pub fn data_path(&self) -> PathBuf {
        PathBuf::from(&self.data_dir)
    }
}

/// Accepts a bare profile id or a profile URL and returns the id.
/// `https://scholar.google.com/citations?user=XXXX&hl=en` yields `XXXX`.
pub fn extract_profile_id(input: &str) -> String {
    let input = input.trim();
    for (idx, _) in input.match_indices("user=") {
        let preceded_ok = idx > 0 && matches!(input.as_bytes()[idx - 1], b'?' | b'&' | b';');
        if preceded_ok {
            let rest = &input[idx + "user=".len()..];
            let end = rest.find(['&', '#', ';']).unwrap_or(rest.len());
            return rest[..end].trim().to_string();
        }
    }
    input.to_string()
}

/// A Scholar profile id contains only letters, digits, '-' and '_'.
pub fn valid_profile_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 32
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Accepts an ORCID iD in any common spelling (`0000-0002-1825-0097`, `0000000218250097`,
/// `https://orcid.org/0000-0002-1825-0097`) and returns the canonical dashed form with an
/// upper-case check character. Input that is not 16 characters long is returned trimmed.
pub fn extract_orcid(input: &str) -> String {
    let mut v = input.trim();
    let lower = v.to_ascii_lowercase();
    for prefix in ["https://orcid.org/", "http://orcid.org/", "orcid.org/", "https://www.orcid.org/"] {
        if lower.starts_with(prefix) {
            v = v[prefix.len()..].trim();
            break;
        }
    }
    let v = v.trim_end_matches('/');
    let compact: String = v.chars().filter(|c| *c != '-' && !c.is_whitespace()).collect();
    if compact.chars().count() == 16 && compact.chars().all(|c| c.is_ascii_alphanumeric()) {
        let c = compact.to_ascii_uppercase();
        return format!("{}-{}-{}-{}", &c[0..4], &c[4..8], &c[8..12], &c[12..16]);
    }
    v.to_string()
}

/// ISO 7064 mod 11-2 check character for the first 15 digits of an ORCID iD.
fn orcid_check_char(digits15: &str) -> Option<char> {
    let mut total: u32 = 0;
    for c in digits15.chars() {
        total = (total + c.to_digit(10)?) * 2;
    }
    let result = (12 - total % 11) % 11;
    Some(if result == 10 {
        'X'
    } else {
        char::from_digit(result, 10)?
    })
}

/// True for `dddd-dddd-dddd-dddc` with a correct ISO 7064 mod 11-2 check character.
pub fn valid_orcid(orcid: &str) -> bool {
    let b = orcid.as_bytes();
    if b.len() != 19 || b[4] != b'-' || b[9] != b'-' || b[14] != b'-' {
        return false;
    }
    let compact: String = orcid.chars().filter(|c| *c != '-').collect();
    let (digits, check) = compact.split_at(15);
    if !digits.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    check.chars().next().is_some() && orcid_check_char(digits) == check.chars().next()
}

/// Loads settings; a missing or corrupt file yields defaults.
pub fn load(path: &Path, default_dir: &Path) -> Settings {
    let mut s = std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str::<Settings>(&t).ok())
        .unwrap_or_default();
    s.normalize(default_dir);
    s
}

pub fn save(path: &Path, settings: &Settings) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("could not create folder: {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(settings)?;
    std::fs::write(path, json).with_context(|| format!("could not write settings: {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_clamping() {
        let dir = PathBuf::from("C:\\Docs\\ScholarTakip");
        let mut s = Settings::default();
        s.normalize(&dir);
        assert_eq!(s.profile_id, "");
        assert!(s.language == "tr" || s.language == "en");
        assert_eq!(s.interval_hours, 3);
        assert!(s.autostart && s.notify && s.auto_update && s.weekly_summary);
        assert_eq!(s.data_dir, "C:\\Docs\\ScholarTakip");

        s.interval_hours = 0;
        s.normalize(&dir);
        assert_eq!(s.interval_hours, 1);
        s.interval_hours = 99;
        s.normalize(&dir);
        assert_eq!(s.interval_hours, 24);
    }

    #[test]
    fn partial_json_gets_defaults_and_roundtrips() {
        let base = std::env::temp_dir().join(format!("scholartakip_set_{}", std::process::id()));
        let file = base.join("settings.json");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("mk");
        std::fs::write(&file, "{\"interval_hours\": 6}").expect("w");
        let s = load(&file, &base);
        assert_eq!(s.interval_hours, 6);
        assert_eq!(s.profile_id, "");
        assert!(s.language == "tr" || s.language == "en");
        // a settings file without the weekly switch keeps the default (on)
        assert!(s.weekly_summary);

        let mut changed = s.clone();
        changed.notify = false;
        save(&file, &changed).expect("save");
        assert_eq!(load(&file, &base), changed);

        std::fs::write(&file, "not json").expect("w");
        assert_eq!(load(&file, &base).interval_hours, 3);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn theme_defaults_to_system_and_roundtrips() {
        let dir = PathBuf::from("Tracker");
        assert_eq!(Settings::default().theme, "system");
        // a settings file written before the field existed loads as "system"
        let old: Settings = serde_json::from_str("{\"profile_id\":\"AbC\"}").expect("parse");
        assert_eq!(old.theme, "system");
        for (input, want) in [("dark", "dark"), (" Light ", "light"), ("DARK", "dark"), ("", "system"), ("sepia", "system")] {
            let mut s = Settings { theme: input.into(), ..Settings::default() };
            s.normalize(&dir);
            assert_eq!(s.theme, want, "{input:?}");
        }
        let base = std::env::temp_dir().join(format!("scholartakip_theme_{}", std::process::id()));
        let file = base.join("settings.json");
        let _ = std::fs::remove_dir_all(&base);
        let mut s = Settings { theme: "dark".into(), ..Settings::default() };
        s.normalize(&base);
        save(&file, &s).expect("save");
        assert_eq!(load(&file, &base).theme, "dark");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn backdrop_defaults_to_on_and_roundtrips() {
        assert!(Settings::default().backdrop);
        let old: Settings = serde_json::from_str("{\"profile_id\":\"AbC\"}").expect("parse");
        assert!(old.backdrop);
        let off: Settings = serde_json::from_str("{\"backdrop\":false}").expect("parse");
        assert!(!off.backdrop);
        let json = serde_json::to_string(&off).expect("ser");
        assert!(!serde_json::from_str::<Settings>(&json).expect("de").backdrop);
    }

    #[test]
    fn profile_id_validation() {
        assert!(valid_profile_id("AbCdEf123456"));
        assert!(!valid_profile_id(""));
        assert!(!valid_profile_id("a b"));
        assert!(!valid_profile_id("x&y=1"));
    }

    #[test]
    fn language_is_normalized_and_old_files_get_a_default() {
        let dir = PathBuf::from("Tracker");
        let mut s = Settings {
            language: "TR".into(),
            ..Settings::default()
        };
        s.normalize(&dir);
        assert_eq!(s.language, "tr");
        s.language = "en".into();
        s.normalize(&dir);
        assert_eq!(s.language, "en");
        s.language = "klingon".into();
        s.normalize(&dir);
        assert_eq!(s.language, i18n::system_lang());

        // a settings file written before the field existed
        let old: Settings = serde_json::from_str("{\"profile_id\":\"AbC\"}").expect("parse");
        assert_eq!(old.language, i18n::system_lang());
    }

    #[test]
    fn orcid_forms_are_normalized() {
        for input in [
            "0000-0002-1825-0097",
            "  https://orcid.org/0000-0002-1825-0097 ",
            "http://orcid.org/0000-0002-1825-0097/",
            "orcid.org/0000-0002-1825-0097",
            "0000000218250097",
            "0000 0002 1825 0097",
        ] {
            assert_eq!(extract_orcid(input), "0000-0002-1825-0097", "{input}");
        }
        assert_eq!(extract_orcid("0000-0002-1694-233x"), "0000-0002-1694-233X");
        assert_eq!(extract_orcid(""), "");
        assert_eq!(extract_orcid("abc"), "abc");
    }

    #[test]
    fn orcid_checksum_is_enforced() {
        assert!(valid_orcid("0000-0002-1825-0097")); // ORCID's own sample iD
        assert!(valid_orcid("0000-0002-1694-233X")); // check character X
        assert!(!valid_orcid("0000-0002-1825-0098")); // wrong check digit
        assert!(!valid_orcid("0000-0002-1694-2330"));
        assert!(!valid_orcid("0000-0002-1825-009")); // too short
        assert!(!valid_orcid("0000000218250097")); // not normalized (no dashes)
        assert!(!valid_orcid("0000-0002-1825-00X7"));
        assert!(!valid_orcid(""));
        assert!(valid_orcid(&extract_orcid("https://orcid.org/0000-0002-1825-0097")));
    }

    #[test]
    fn old_settings_files_have_no_orcid_and_no_backfill() {
        let old: Settings = serde_json::from_str("{\"profile_id\":\"AbC\"}").expect("parse");
        assert_eq!(old.orcid, "");
        assert!(!old.backfill_enabled);
        let mut s = Settings { orcid: "https://orcid.org/0000-0002-1825-0097".into(), ..Settings::default() };
        s.normalize(&PathBuf::from("D"));
        assert_eq!(s.orcid, "0000-0002-1825-0097");
    }

    #[test]
    fn profile_id_is_extracted_from_urls() {
        assert_eq!(extract_profile_id("  AbCdEf123456 "), "AbCdEf123456");
        assert_eq!(
            extract_profile_id("https://scholar.google.com/citations?user=AbCdEf123456&hl=en"),
            "AbCdEf123456"
        );
        assert_eq!(
            extract_profile_id("https://scholar.google.com/citations?hl=tr&user=AbC-d_1#x"),
            "AbC-d_1"
        );
        assert_eq!(extract_profile_id("scholar.google.com/citations?user=Zz9"), "Zz9");
        assert_eq!(extract_profile_id("https://example.com/?xuser=1"), "https://example.com/?xuser=1");
        assert_eq!(extract_profile_id(""), "");
    }
}
