//! Optional OpenAlex API key: storage in the Windows Credential Manager,
//! request pacing that depends on whether a key is present, redaction of anything that could
//! contain a key, and the two small key calls (test a key, request an OpenAlex key by e-mail).
//!
//! A key never goes into `settings.json`, the database, a URL, a log line or an error message.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;

use crate::backfill::USER_AGENT;

const SERVICE: &str = "scholar-citation-tracker";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    OpenAlex,
}

impl KeyKind {
    /// Parses the name the UI sends.
    pub fn parse(s: &str) -> Option<KeyKind> {
        match s {
            "openalex" => Some(KeyKind::OpenAlex),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            KeyKind::OpenAlex => "openalex",
        }
    }

    fn account(self) -> &'static str {
        match self {
            KeyKind::OpenAlex => "openalex-api-key",
        }
    }
}

// ------------------------------------------------------------------- pacing

pub const OA_GAP_KEYLESS_MS: u64 = 1100;
/// A free OpenAlex key allows 30 requests per second; the app still stays far below that.
pub const OA_GAP_KEYED_MS: u64 = 400;

pub fn oa_gap(has_key: bool) -> Duration {
    Duration::from_millis(if has_key { OA_GAP_KEYED_MS } else { OA_GAP_KEYLESS_MS })
}

// ---------------------------------------------------------------- redaction

/// Keys seen in this session; `redact` hides them wherever they would show up.
static KNOWN: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Remembers a key so `redact` can hide it (keys shorter than 6 characters are ignored).
pub fn register(key: &str) {
    let key = key.trim();
    if key.len() < 6 {
        return;
    }
    let mut known = KNOWN.lock().unwrap_or_else(|e| e.into_inner());
    if !known.iter().any(|k| k == key) {
        known.push(key.to_string());
    }
}

/// Keys the user was already warned about in this session (the warning shows once).
static WARNED: Mutex<Vec<KeyKind>> = Mutex::new(Vec::new());

/// True the first time a rejected `kind` is reported in this session; saving a new key resets it.
pub fn first_warning(kind: KeyKind) -> bool {
    let mut w = WARNED.lock().unwrap_or_else(|e| e.into_inner());
    if w.contains(&kind) {
        false
    } else {
        w.push(kind);
        true
    }
}

fn clear_warning(kind: KeyKind) {
    WARNED.lock().unwrap_or_else(|e| e.into_inner()).retain(|k| *k != kind);
}

const MASK: &str = "[redacted]";

/// Replaces the value after a marker such as `api_key=` up to the next separator.
fn scrub_after(text: &str, marker: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut pos = 0;
    while let Some(i) = lower[pos..].find(marker) {
        let start = pos + i + marker.len();
        out.push_str(&text[pos..start]);
        let rest = &text[start..];
        let skip = rest.len() - rest.trim_start_matches(' ').len();
        out.push_str(&rest[..skip]);
        let value = &rest[skip..];
        let end = value
            .find(|c: char| c.is_whitespace() || matches!(c, '&' | '"' | '\'' | ',' | ';' | ')' | '<'))
            .unwrap_or(value.len());
        if end > 0 {
            out.push_str(MASK);
        }
        pos = start + skip + end;
    }
    out.push_str(&text[pos..]);
    out
}

/// Removes every API key from `text`: the keys given in `extra`, the keys registered in this
/// session, and anything that follows `api_key=`, or `Bearer `.
pub fn redact(text: &str, extra: &[&str]) -> String {
    let mut out = text.to_string();
    let known: Vec<String> = KNOWN.lock().unwrap_or_else(|e| e.into_inner()).clone();
    for k in extra.iter().copied().chain(known.iter().map(String::as_str)) {
        let k = k.trim();
        if k.len() >= 6 {
            out = out.replace(k, MASK);
        }
    }
    for marker in ["api_key=", "api-key=", "apikey=", "bearer "] {
        out = scrub_after(&out, marker);
    }
    out
}

// ------------------------------------------------------------------ storage

fn entry(kind: KeyKind) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, kind.account()).map_err(|e| redact(&e.to_string(), &[]))
}

/// The stored key, or `None` when there is none (or the store cannot be read).
pub fn load(kind: KeyKind) -> Option<String> {
    let key = entry(kind).ok()?.get_password().ok()?;
    let key = key.trim().to_string();
    if key.is_empty() {
        return None;
    }
    register(&key);
    Some(key)
}

pub fn store(kind: KeyKind, key: &str) -> Result<(), String> {
    let key = key.trim();
    entry(kind)?.set_password(key).map_err(|e| redact(&e.to_string(), &[key]))?;
    register(key);
    clear_warning(kind);
    Ok(())
}

/// Removes the stored key; a missing entry is not an error.
pub fn delete(kind: KeyKind) -> Result<(), String> {
    match entry(kind)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(redact(&e.to_string(), &[])),
    }
}

/// One-time clean-up: versions before 1.1.1 could keep a key for a service that is no longer
/// used. Removes that Credential Manager entry; a missing entry or an unreadable store is ignored.
pub fn delete_legacy_semantic_key() {
    if let Ok(e) = keyring::Entry::new(SERVICE, "semantic-scholar-api-key") {
        let _ = e.delete_credential();
    }
}

/// Which keys are stored (never the keys themselves).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct KeyStatus {
    pub openalex: bool,
}

pub fn status() -> KeyStatus {
    KeyStatus {
        openalex: load(KeyKind::OpenAlex).is_some(),
    }
}

/// Keys loaded for one backfill run.
#[derive(Debug, Clone, Default)]
pub struct Keys {
    pub openalex: Option<String>,
}

pub fn load_keys() -> Keys {
    Keys {
        openalex: load(KeyKind::OpenAlex),
    }
}

/// A plausible key: printable ASCII without spaces, 8 to 200 characters.
pub fn plausible_key(key: &str) -> bool {
    let k = key.trim();
    (8..=200).contains(&k.len()) && k.chars().all(|c| c.is_ascii_graphic())
}

// --------------------------------------------------------------- key calls

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TestResult {
    /// "ok" | "invalid" | "unreachable"
    pub status: &'static str,
    pub detail: String,
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(25))
        .build()
        .map_err(|e| redact(&e.without_url().to_string(), &[]))
}

/// One cheap request with the key: 200 is "ok", 401 / 403 is "invalid".
pub async fn test_key(kind: KeyKind, key: &str) -> TestResult {
    let key = key.trim().to_string();
    register(&key);
    let client = match client() {
        Ok(c) => c,
        Err(e) => return TestResult { status: "unreachable", detail: e },
    };
    let KeyKind::OpenAlex = kind;
    let mut value = match reqwest::header::HeaderValue::from_str(&format!("Bearer {key}")) {
        Ok(v) => v,
        Err(_) => return TestResult { status: "invalid", detail: String::new() },
    };
    value.set_sensitive(true);
    let req = client
        .get("https://api.openalex.org/works?per-page=1&select=id")
        .header(reqwest::header::AUTHORIZATION, value);
    match req.send().await {
        Ok(resp) => match resp.status().as_u16() {
            200..=299 => TestResult { status: "ok", detail: String::new() },
            401 | 403 => TestResult { status: "invalid", detail: String::new() },
            code => TestResult { status: "unreachable", detail: format!("HTTP {code}") },
        },
        Err(e) => TestResult {
            status: "unreachable",
            detail: redact(&e.without_url().to_string(), &[&key]),
        },
    }
}

/// Minimal e-mail shape check (the address only goes to OpenAlex, on an explicit click).
pub fn plausible_email(email: &str) -> bool {
    let e = email.trim();
    if e.len() > 254 || e.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    match e.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.') && !domain.contains('@')
        }
        None => false,
    }
}

/// What OpenAlex answers to `POST /api-keys`.
#[derive(serde::Deserialize)]
struct KeyReply {
    api_key: Option<String>,
    status: Option<String>,
}

/// Requests a free OpenAlex key for `email`. The key is stored in the Credential Manager; the
/// address is used for this one request only and is neither stored nor logged. Returns the
/// activation status OpenAlex reports (normally "pending_activation").
pub async fn request_openalex_key(email: &str) -> Result<String, String> {
    if !plausible_email(email) {
        return Err("email".into());
    }
    let client = client()?;
    let body = serde_json::json!({ "email": email.trim() }).to_string();
    let resp = client
        .post("https://api.openalex.org/api-keys")
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body)
        .send()
        .await
        .map_err(|e| redact(&e.without_url().to_string(), &[]))?;
    let code = resp.status().as_u16();
    if !(200..300).contains(&code) {
        return Err(format!("HTTP {code}"));
    }
    let text = resp.text().await.map_err(|e| redact(&e.without_url().to_string(), &[]))?;
    let reply: KeyReply = serde_json::from_str(&text).map_err(|_| "unexpected reply".to_string())?;
    let key = reply.api_key.map(|k| k.trim().to_string()).unwrap_or_default();
    if !plausible_key(&key) {
        return Err("unexpected reply".into());
    }
    store(KeyKind::OpenAlex, &key)?;
    Ok(reply.status.unwrap_or_else(|| "pending_activation".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pacing_depends_on_the_key() {
        assert_eq!(oa_gap(false), Duration::from_millis(1100));
        assert!(oa_gap(true) < oa_gap(false));
        // a key never makes the app faster than the documented limits
        assert!(oa_gap(true) >= Duration::from_millis(100));
    }

    #[test]
    fn redact_hides_given_and_marked_keys() {
        let key = "sk_test_ABCDEF123456";
        let t = format!("request failed for https://x.org/?api_key={key}&per-page=1 using {key}");
        let r = redact(&t, &[key]);
        assert!(!r.contains(key), "{r}");
        assert!(r.contains("per-page=1"));

        let r = redact("Authorization: Bearer abcdef123456789 rejected", &[]);
        assert!(!r.contains("abcdef123456789"), "{r}");
        assert!(r.contains("rejected"));

        // nothing to hide: unchanged
        assert_eq!(redact("HTTP 429", &[]), "HTTP 429");
        // short strings are not treated as keys
        assert_eq!(redact("abc", &["abc"]), "abc");
    }

    #[test]
    fn redact_hides_registered_keys() {
        register("registered-key-0001");
        let r = redact("boom registered-key-0001 boom", &[]);
        assert_eq!(r, "boom [redacted] boom");
    }

    #[test]
    fn key_and_email_shape_checks() {
        assert!(plausible_key("abcdefgh1234"));
        assert!(!plausible_key("short"));
        assert!(!plausible_key("has space in it 1234"));
        assert!(plausible_email("name@example.org"));
        assert!(!plausible_email("name@"));
        assert!(!plausible_email("name example@x.org"));
        assert!(!plausible_email("@x.org"));
        assert!(!plausible_email("a@b"));
    }

    #[test]
    fn a_rejected_key_is_reported_once_until_a_new_key_is_saved() {
        assert!(first_warning(KeyKind::OpenAlex));
        assert!(!first_warning(KeyKind::OpenAlex));
        clear_warning(KeyKind::OpenAlex);
        assert!(first_warning(KeyKind::OpenAlex));
    }

    #[test]
    fn key_kind_round_trip() {
        let k = KeyKind::OpenAlex;
        assert_eq!(KeyKind::parse(k.as_str()), Some(k));
        assert_eq!(KeyKind::parse("x"), None);
    }
}
