//! Historical backfill: past citation dates for every article from free sources.
//!
//! The profile owner's ORCID iD is the primary key to the right works: it resolves to an
//! OpenAlex author whose works form a small candidate set, and each Scholar article is
//! assigned to at most one of them (best title similarity, year within one). Articles outside
//! that set fall back to a global title search with a stricter threshold.
//!
//! For each article both OpenAlex and Semantic Scholar are then queried (Semantic Scholar by
//! DOI once one is known, otherwise by title) for the list of citing works. Whichever source
//! returns more dated citing records is stored; on a tie OpenAlex wins. Counts are never rescaled to Scholar's totals;
//! a record is dated by the citing work's own publication date.
//!
//! Nothing here talks to Google, and a failure here never touches the Scholar scheduler.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::time::Instant;

use anyhow::Result;
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::db::{self, CitingRecord};
use crate::i18n;
use crate::scheduler::lock;
use crate::secrets::{self, KeyKind, Keys};
use crate::settings::Settings;

pub const USER_AGENT: &str = concat!(
    "scholar-citation-tracker/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/bcankara/scholar-citation-tracker)"
);

const OPENALEX_WORKS: &str = "https://api.openalex.org/works";
const OPENALEX_AUTHORS: &str = "https://api.openalex.org/authors";
const S2_BASE: &str = "https://api.semanticscholar.org/graph/v1";
const OPENALEX_LABEL: &str = "OpenAlex";
const S2_LABEL: &str = "Semantic Scholar";
/// Minimum title similarity for the global title search fallback (Sørensen–Dice on word tokens).
pub const MIN_SCORE: f64 = 0.85;
/// Minimum title similarity inside the ORCID author set (a small, trusted candidate list).
pub const ORCID_MIN_SCORE: f64 = 0.80;
const OA_BACKOFF: &[u64] = &[5, 20, 60];
const S2_BACKOFF: &[u64] = &[10, 30, 60];
/// Safety cap on pages per article and source.
const MAX_PAGES: usize = 100;

// ------------------------------------------------------------------ matching

/// Lowercase, no diacritics, no punctuation, single spaces. HTML tags and a few entities
/// (OpenAlex titles can contain `<i>...</i>`) are removed first.
pub fn normalize_title(s: &str) -> String {
    let s = s
        .replace("&amp;", " ")
        .replace("&lt;", " ")
        .replace("&gt;", " ")
        .replace("&quot;", " ")
        .replace("&#39;", " ");
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '<' {
            let next = chars.get(i + 1).copied().unwrap_or(' ');
            if next.is_ascii_alphabetic() || next == '/' {
                if let Some(end) = chars[i..].iter().position(|&x| x == '>') {
                    out.push(' ');
                    i += end + 1;
                    continue;
                }
            }
        }
        for lower in c.to_lowercase() {
            fold_into(lower, &mut out);
        }
        i += 1;
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn fold_into(c: char, out: &mut String) {
    let base = match c {
        '\u{0300}'..='\u{036f}' => return,
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
        'ç' | 'ć' | 'č' => 'c',
        'ď' | 'đ' => 'd',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => 'e',
        'ğ' => 'g',
        'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' | 'ı' => 'i',
        'ł' => 'l',
        'ñ' | 'ń' | 'ň' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => 'o',
        'ř' => 'r',
        'ś' | 'š' | 'ş' | 'ș' => 's',
        'ť' | 'ţ' | 'ț' => 't',
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' | 'ų' => 'u',
        'ý' | 'ÿ' => 'y',
        'ź' | 'ż' | 'ž' => 'z',
        'ß' => {
            out.push_str("ss");
            return;
        }
        'æ' => {
            out.push_str("ae");
            return;
        }
        'œ' => {
            out.push_str("oe");
            return;
        }
        other => other,
    };
    if base.is_alphanumeric() {
        out.push(base);
    } else {
        out.push(' ');
    }
}

fn tokens(s: &str) -> HashSet<String> {
    normalize_title(s).split(' ').filter(|t| !t.is_empty()).map(str::to_string).collect()
}

/// Sørensen–Dice coefficient over the sets of normalized word tokens (0.0 to 1.0).
pub fn title_similarity(a: &str, b: &str) -> f64 {
    let (ta, tb) = (tokens(a), tokens(b));
    if ta.is_empty() || tb.is_empty() {
        return 0.0;
    }
    let common = ta.intersection(&tb).count() as f64;
    2.0 * common / (ta.len() + tb.len()) as f64
}

/// Percent-decodes `%XX` sequences; `None` when the result is not valid UTF-8.
fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = std::str::from_utf8(&b[i + 1..i + 3]).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Normalizes a DOI typed or pasted by the user: a bare DOI, `doi:10...` or any of the
/// `https://doi.org/...` / `https://dx.doi.org/...` forms. Returns the lower-case DOI, or
/// `None` when it does not have the shape `10.<registrant>/<suffix>` (registrant of 4 to 9
/// digits, optionally with `.digits` sub-parts; suffix without spaces or control characters).
pub fn normalize_doi(input: &str) -> Option<String> {
    let mut v = input.trim();
    let lower = v.to_ascii_lowercase();
    let mut from_url = false;
    for prefix in [
        "https://doi.org/",
        "http://doi.org/",
        "https://www.doi.org/",
        "https://dx.doi.org/",
        "http://dx.doi.org/",
        "doi.org/",
        "dx.doi.org/",
    ] {
        if lower.starts_with(prefix) {
            v = &v[prefix.len()..];
            from_url = true;
            break;
        }
    }
    if !from_url && lower.starts_with("doi:") {
        v = v["doi:".len()..].trim_start();
    }
    let mut v = v.trim();
    if from_url {
        if let Some(i) = v.find(['?', '#']) {
            v = &v[..i];
        }
    }
    let mut doi = if v.contains('%') {
        percent_decode(v)?
    } else {
        v.to_string()
    };
    doi = doi.trim().trim_end_matches('/').to_ascii_lowercase();
    if doi.len() > 200 {
        return None;
    }
    let rest = doi.strip_prefix("10.")?;
    let (registrant, suffix) = rest.split_once('/')?;
    let mut parts = registrant.split('.');
    let first = parts.next()?;
    if !(4..=9).contains(&first.len()) || !first.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if !parts.all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())) {
        return None;
    }
    if suffix.is_empty() || suffix.chars().any(|c| c.is_whitespace() || c.is_control() || c == '"') {
        return None;
    }
    Some(doi)
}

/// Year rule: within one year, or either year unknown.
pub fn year_ok(scholar: Option<i64>, candidate: Option<i64>) -> bool {
    match (scholar, candidate) {
        (Some(a), Some(b)) => (a - b).abs() <= 1,
        _ => true,
    }
}

/// A work found in an external source.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub id: String,
    pub doi: Option<String>,
    pub title: String,
    pub year: Option<i64>,
}

/// Best candidate with similarity >= `MIN_SCORE` and an acceptable year. Ties go to the
/// smaller year difference, then to the earlier candidate.
pub fn best_candidate<'a>(
    title: &str,
    year: Option<i64>,
    cands: &'a [Candidate],
) -> Option<(&'a Candidate, f64)> {
    let mut best: Option<(&Candidate, f64)> = None;
    for c in cands {
        let score = title_similarity(title, &c.title);
        if score < MIN_SCORE || !year_ok(year, c.year) {
            continue;
        }
        let better = match best {
            None => true,
            Some((b, bs)) => {
                score > bs + 1e-9
                    || ((score - bs).abs() <= 1e-9 && year_gap(year, c.year) < year_gap(year, b.year))
            }
        };
        if better {
            best = Some((c, score));
        }
    }
    best
}

/// `best_candidate` without the works the user ruled out for this article.
pub fn best_candidate_excluding(
    title: &str,
    year: Option<i64>,
    cands: &[Candidate],
    rejected: &HashSet<String>,
) -> Option<(Candidate, f64)> {
    let allowed: Vec<Candidate> = cands.iter().filter(|c| !rejected.contains(&c.id)).cloned().collect();
    best_candidate(title, year, &allowed).map(|(c, s)| (c.clone(), s))
}

fn year_gap(a: Option<i64>, b: Option<i64>) -> i64 {
    match (a, b) {
        (Some(a), Some(b)) => (a - b).abs(),
        _ => 0,
    }
}

// ------------------------------------------------------------------- parsing

/// Parses `YYYY-MM-DD`, `YYYY-MM` or `YYYY` and says how precise it is.
pub fn parse_date(s: &str) -> Option<(String, &'static str)> {
    let s = s.trim();
    let digits = |x: &str| !x.is_empty() && x.chars().all(|c| c.is_ascii_digit());
    match s.len() {
        10 if NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok() => Some((s.to_string(), "day")),
        7 if NaiveDate::parse_from_str(&format!("{s}-01"), "%Y-%m-%d").is_ok() => {
            Some((s.to_string(), "month"))
        }
        4 if digits(s) && (1000..=2200).contains(&s.parse::<i32>().unwrap_or(0)) => {
            Some((s.to_string(), "year"))
        }
        _ => None,
    }
}

/// Best available date: the full date when valid, otherwise the year.
fn dated(date: Option<&str>, year: Option<i64>) -> Option<(String, &'static str)> {
    date.and_then(parse_date)
        .or_else(|| year.and_then(|y| parse_date(&y.to_string())))
}

fn short_id(url: &str, prefix: &str) -> String {
    url.strip_prefix(prefix).unwrap_or(url).trim().to_string()
}

fn valid_openalex_id(id: &str) -> bool {
    id.len() > 1 && id.starts_with('W') && id[1..].chars().all(|c| c.is_ascii_digit())
}

fn valid_s2_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric())
}

#[derive(Deserialize)]
struct OaPage {
    #[serde(default)]
    results: Vec<OaWork>,
    #[serde(default)]
    meta: OaMeta,
}

#[derive(Deserialize, Default)]
struct OaMeta {
    next_cursor: Option<String>,
}

#[derive(Deserialize)]
struct OaWork {
    id: Option<String>,
    doi: Option<String>,
    title: Option<String>,
    publication_year: Option<i64>,
    publication_date: Option<String>,
    #[serde(default)]
    primary_location: Option<OaLocation>,
    #[serde(default)]
    authorships: Vec<OaAuthorship>,
}

#[derive(Deserialize, Default)]
struct OaLocation {
    landing_page_url: Option<String>,
    source: Option<OaSource>,
}

#[derive(Deserialize, Default)]
struct OaSource {
    display_name: Option<String>,
    issn_l: Option<String>,
    #[serde(default)]
    issn: Option<Vec<String>>,
}

#[derive(Deserialize, Default)]
struct OaAuthorship {
    author: Option<OaAuthorRef>,
    #[serde(default)]
    institutions: Vec<OaInstitution>,
}

#[derive(Deserialize, Default)]
struct OaAuthorRef {
    display_name: Option<String>,
}

#[derive(Deserialize, Default)]
struct OaInstitution {
    country_code: Option<String>,
}

/// ISSN as 8 characters without a hyphen, `X` upper case; `None` when it is not an ISSN.
pub fn normalize_issn(s: &str) -> Option<String> {
    let c: String = s.chars().filter(|c| !matches!(c, '-' | ' ' | '\u{a0}')).collect::<String>().to_ascii_uppercase();
    let ok = c.len() == 8
        && c.bytes().take(7).all(|b| b.is_ascii_digit())
        && c.as_bytes().get(7).is_some_and(|b| b.is_ascii_digit() || *b == b'X');
    ok.then_some(c)
}

/// Normalized, de-duplicated ISSNs (first occurrence order), comma-separated.
pub fn join_issns<I: IntoIterator<Item = String>>(items: I) -> String {
    let mut seen = HashSet::new();
    items
        .into_iter()
        .filter_map(|i| normalize_issn(&i))
        .filter(|i| seen.insert(i.clone()))
        .collect::<Vec<_>>()
        .join(",")
}

/// How many author names are kept in the authors string.
const MAX_AUTHORS: usize = 10;

/// `A; B; C` for up to ten names, then `; et al.`.
pub fn join_authors<I: IntoIterator<Item = String>>(names: I) -> String {
    let all: Vec<String> = names.into_iter().map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect();
    let mut out = all.iter().take(MAX_AUTHORS).cloned().collect::<Vec<_>>().join("; ");
    if all.len() > MAX_AUTHORS {
        out.push_str("; et al.");
    }
    out
}

/// Sorted, de-duplicated, comma-separated upper-case two-letter country codes.
pub fn join_countries<I: IntoIterator<Item = String>>(codes: I) -> String {
    let set: std::collections::BTreeSet<String> = codes
        .into_iter()
        .map(|c| c.trim().to_ascii_uppercase())
        .filter(|c| c.len() == 2 && c.chars().all(|x| x.is_ascii_alphabetic()))
        .collect();
    set.into_iter().collect::<Vec<_>>().join(",")
}

/// OpenAlex `works` page (search or filter) to candidates, plus the next cursor if there is one.
pub fn parse_oa_works(json: &str) -> Result<(Vec<Candidate>, Option<String>)> {
    let page: OaPage = serde_json::from_str(json)?;
    let empty = page.results.is_empty();
    let next = page.meta.next_cursor.filter(|c| !c.is_empty() && !empty);
    let cands = page
        .results
        .into_iter()
        .filter_map(|w| {
            let id = short_id(&w.id?, "https://openalex.org/");
            if !valid_openalex_id(&id) {
                return None;
            }
            Some(Candidate {
                id,
                doi: w.doi.map(|d| short_id(&d, "https://doi.org/")).filter(|d| !d.is_empty()),
                title: w.title?,
                year: w.publication_year,
            })
        })
        .collect();
    Ok((cands, next))
}

/// A single OpenAlex work (`works/doi:<doi>`): `None` when its id is unusable.
pub fn parse_oa_work(json: &str) -> Result<Option<Candidate>> {
    let w: OaWork = serde_json::from_str(json)?;
    let Some(id) = w.id.map(|i| short_id(&i, "https://openalex.org/")) else {
        return Ok(None);
    };
    if !valid_openalex_id(&id) {
        return Ok(None);
    }
    Ok(Some(Candidate {
        id,
        doi: w.doi.map(|d| short_id(&d, "https://doi.org/")).filter(|d| !d.is_empty()),
        title: w.title.unwrap_or_default(),
        year: w.publication_year,
    }))
}

/// OpenAlex `works?search=` result page to candidates.
pub fn parse_oa_search(json: &str) -> Result<Vec<Candidate>> {
    Ok(parse_oa_works(json)?.0)
}

#[derive(Deserialize)]
struct OaAuthor {
    id: Option<String>,
    works_count: Option<i64>,
}

/// OpenAlex `authors/orcid:<orcid>` response: author id (`A...`) and works_count.
pub fn parse_oa_author(json: &str) -> Result<(String, Option<i64>)> {
    let a: OaAuthor = serde_json::from_str(json)?;
    let id = short_id(&a.id.unwrap_or_default(), "https://openalex.org/");
    if id.len() < 2 || !id.starts_with('A') || !id[1..].chars().all(|c| c.is_ascii_digit()) {
        anyhow::bail!("unexpected author id: {id}");
    }
    Ok((id, a.works_count))
}

/// Works of the ORCID's OpenAlex author.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AuthorSet {
    pub author_id: Option<String>,
    pub works_count: Option<i64>,
    pub works: Vec<Candidate>,
}

/// Union by work id, first occurrence kept.
pub fn dedupe_candidates(cands: Vec<Candidate>) -> Vec<Candidate> {
    let mut seen = HashSet::new();
    cands.into_iter().filter(|c| seen.insert(c.id.clone())).collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Assignment {
    /// Index into the candidate list.
    pub cand: usize,
    pub score: f64,
}

/// One-to-one assignment of articles `(title, year)` to author-set candidates. All pairs with
/// similarity >= `ORCID_MIN_SCORE` and an acceptable year are taken greedily: highest score
/// first, then the smaller year difference, then input order. A candidate is used at most once;
/// `excluded` lists candidate ids that are already taken.
#[cfg(test)]
pub fn assign_candidates(
    articles: &[(String, Option<i64>)],
    cands: &[Candidate],
    excluded: &HashSet<String>,
) -> Vec<Option<Assignment>> {
    assign_candidates_ex(articles, cands, excluded, &[])
}

/// `assign_candidates` with a per-article list of ruled-out candidate ids (`rejected[i]` belongs
/// to `articles[i]`; a shorter slice means no exclusions for the rest).
pub fn assign_candidates_ex(
    articles: &[(String, Option<i64>)],
    cands: &[Candidate],
    excluded: &HashSet<String>,
    rejected: &[HashSet<String>],
) -> Vec<Option<Assignment>> {
    let mut pairs: Vec<(f64, i64, usize, usize)> = Vec::new();
    for (ai, (title, year)) in articles.iter().enumerate() {
        for (ci, c) in cands.iter().enumerate() {
            if excluded.contains(&c.id) || rejected.get(ai).is_some_and(|r| r.contains(&c.id)) {
                continue;
            }
            let score = title_similarity(title, &c.title);
            if score >= ORCID_MIN_SCORE - 1e-9 && year_ok(*year, c.year) {
                pairs.push((score, year_gap(*year, c.year), ai, ci));
            }
        }
    }
    pairs.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.cmp(&b.1))
            .then(a.2.cmp(&b.2))
            .then(a.3.cmp(&b.3))
    });
    let mut out: Vec<Option<Assignment>> = vec![None; articles.len()];
    let mut used: HashSet<usize> = HashSet::new();
    for (score, _, ai, ci) in pairs {
        if out[ai].is_none() && used.insert(ci) {
            out[ai] = Some(Assignment { cand: ci, score });
        }
    }
    out
}

/// How one article is looked up: the user's manual DOI beats every automatic rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// Resolve this (normalized) DOI in both sources; no title matching at all.
    Manual(String),
    /// ORCID author set, then title search; ruled-out works are excluded.
    Automatic,
}

pub fn plan_for(ov: Option<&db::Override>) -> Plan {
    match ov.and_then(|o| o.doi.clone()) {
        Some(doi) => Plan::Manual(doi),
        None => Plan::Automatic,
    }
}

/// Assignment of the targets to the ORCID author set under the user's overrides:
/// - an article with a manual DOI takes no part (it is resolved by DOI);
/// - a work whose DOI is another article's manual DOI is not available to anyone else;
/// - works ruled out for an article are never assigned to it.
///
/// The result has one entry per target, in order.
pub fn plan_assignments(
    targets: &[db::BackfillTarget],
    overrides: &std::collections::HashMap<String, db::Override>,
    cands: &[Candidate],
    excluded: &HashSet<String>,
) -> Vec<Option<Assignment>> {
    let manual: HashSet<String> = overrides.values().filter_map(|o| o.doi.clone()).collect();
    let mut taken = excluded.clone();
    for c in cands {
        if c.doi.as_ref().is_some_and(|d| manual.contains(&d.to_ascii_lowercase())) {
            taken.insert(c.id.clone());
        }
    }
    let auto: Vec<usize> = (0..targets.len())
        .filter(|i| plan_for(overrides.get(&targets[*i].key)) == Plan::Automatic)
        .collect();
    let wanted: Vec<(String, Option<i64>)> = auto
        .iter()
        .map(|i| (targets[*i].title.clone(), targets[*i].year))
        .collect();
    let rejected: Vec<HashSet<String>> = auto
        .iter()
        .map(|i| overrides.get(&targets[*i].key).map(db::Override::rejected_set).unwrap_or_default())
        .collect();
    let found = assign_candidates_ex(&wanted, cands, &taken, &rejected);
    let mut out: Vec<Option<Assignment>> = vec![None; targets.len()];
    for (slot, a) in auto.into_iter().zip(found) {
        out[slot] = a;
    }
    out
}

/// One `works?filter=cites:` page: dated citing works and the next cursor (if any).
pub fn parse_oa_citing(json: &str) -> Result<(Vec<CitingRecord>, Option<String>)> {
    let page: OaPage = serde_json::from_str(json)?;
    let empty = page.results.is_empty();
    let next = page.meta.next_cursor.filter(|c| !c.is_empty() && !empty);
    let records = page
        .results
        .into_iter()
        .filter_map(|w| {
            let id = short_id(&w.id?, "https://openalex.org/");
            let (date, precision) = dated(w.publication_date.as_deref(), w.publication_year)?;
            let loc = w.primary_location.unwrap_or_default();
            let src = loc.source.unwrap_or_default();
            let venue = src.display_name.clone().unwrap_or_default().trim().to_string();
            let issn_list = join_issns(src.issn_l.into_iter().chain(src.issn.into_iter().flatten()));
            let doi = w
                .doi
                .map(|d| short_id(&d, "https://doi.org/").to_ascii_lowercase())
                .unwrap_or_default();
            let url = loc.landing_page_url.unwrap_or_default();
            let authors = join_authors(
                w.authorships
                    .iter()
                    .filter_map(|a| a.author.as_ref().and_then(|x| x.display_name.clone())),
            );
            let countries = join_countries(
                w.authorships
                    .iter()
                    .flat_map(|a| a.institutions.iter().filter_map(|i| i.country_code.clone())),
            );
            Some(CitingRecord {
                citing_id: id,
                citing_title: w.title.unwrap_or_default(),
                citing_date: date,
                date_precision: precision.to_string(),
                venue,
                doi,
                url,
                authors,
                countries,
                issn_list,
            })
        })
        .collect();
    Ok((records, next))
}

#[derive(Deserialize)]
struct S2Match {
    #[serde(default)]
    data: Vec<S2Paper>,
}

#[derive(Deserialize)]
struct S2Paper {
    #[serde(rename = "paperId")]
    paper_id: Option<String>,
    title: Option<String>,
    year: Option<i64>,
    #[serde(rename = "publicationDate")]
    publication_date: Option<String>,
    #[serde(rename = "externalIds")]
    external_ids: Option<HashMap<String, serde_json::Value>>,
    venue: Option<String>,
    #[serde(rename = "publicationVenue")]
    publication_venue: Option<S2Venue>,
    #[serde(default)]
    authors: Option<Vec<S2Author>>,
}

#[derive(Deserialize)]
struct S2Venue {
    issn: Option<String>,
}

#[derive(Deserialize)]
struct S2Author {
    name: Option<String>,
}

#[derive(Deserialize)]
struct S2Citations {
    next: Option<i64>,
    #[serde(default)]
    data: Vec<S2CitingItem>,
}

#[derive(Deserialize)]
struct S2CitingItem {
    #[serde(rename = "citingPaper")]
    citing_paper: Option<S2Paper>,
}

/// Semantic Scholar `paper/search/match` response to candidates.
pub fn parse_s2_match(json: &str) -> Result<Vec<Candidate>> {
    let m: S2Match = serde_json::from_str(json)?;
    Ok(m.data
        .into_iter()
        .filter_map(|p| {
            let id = p.paper_id?;
            if !valid_s2_id(&id) {
                return None;
            }
            let doi = p
                .external_ids
                .as_ref()
                .and_then(|e| e.get("DOI"))
                .and_then(|v| v.as_str())
                .map(str::to_string);
            Some(Candidate {
                id,
                doi,
                title: p.title?,
                year: p.year,
            })
        })
        .collect())
}

/// Semantic Scholar `paper/DOI:<doi>` response (a single paper object).
pub fn parse_s2_paper(json: &str) -> Result<Option<Candidate>> {
    let p: S2Paper = serde_json::from_str(json)?;
    let Some(id) = p.paper_id.filter(|id| valid_s2_id(id)) else {
        return Ok(None);
    };
    let doi = p
        .external_ids
        .as_ref()
        .and_then(|e| e.get("DOI"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    Ok(Some(Candidate {
        id,
        doi,
        title: p.title.unwrap_or_default(),
        year: p.year,
    }))
}

/// One `paper/{id}/citations` page: dated citing works and the next offset (if any).
/// Without a `publicationDate` the `year` is used (year precision).
pub fn parse_s2_citations(json: &str) -> Result<(Vec<CitingRecord>, Option<i64>)> {
    let c: S2Citations = serde_json::from_str(json)?;
    let empty = c.data.is_empty();
    let next = c.next.filter(|_| !empty);
    let records = c
        .data
        .into_iter()
        .filter_map(|item| {
            let p = item.citing_paper?;
            let id = p.paper_id?;
            let (date, precision) = dated(p.publication_date.as_deref(), p.year)?;
            let doi = p
                .external_ids
                .as_ref()
                .and_then(|e| e.get("DOI"))
                .and_then(|v| v.as_str())
                .map(|d| d.trim().to_ascii_lowercase())
                .unwrap_or_default();
            let authors = join_authors(
                p.authors
                    .iter()
                    .flatten()
                    .filter_map(|a| a.name.clone()),
            );
            Some(CitingRecord {
                url: format!("https://www.semanticscholar.org/paper/{id}"),
                citing_id: id,
                citing_title: p.title.unwrap_or_default(),
                citing_date: date,
                date_precision: precision.to_string(),
                venue: p.venue.unwrap_or_default().trim().to_string(),
                doi,
                authors,
                countries: String::new(),
                issn_list: join_issns(p.publication_venue.and_then(|v| v.issn)),
            })
        })
        .collect();
    Ok((records, next))
}

fn dedupe(records: Vec<CitingRecord>) -> Vec<CitingRecord> {
    let mut seen = HashSet::new();
    records.into_iter().filter(|r| seen.insert(r.citing_id.clone())).collect()
}

// ----------------------------------------------------------------- selection

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    OpenAlex,
    SemanticScholar,
}

impl Source {
    fn as_str(self) -> &'static str {
        match self {
            Source::OpenAlex => "openalex",
            Source::SemanticScholar => "semanticscholar",
        }
    }
}

/// The source with more dated citing records wins; a tie goes to OpenAlex. `None` for a
/// source means it did not match (or could not be read completely).
pub fn choose_source(openalex: Option<usize>, s2: Option<usize>) -> Option<Source> {
    match (openalex, s2) {
        (Some(o), Some(s)) => Some(if s > o { Source::SemanticScholar } else { Source::OpenAlex }),
        (Some(_), None) => Some(Source::OpenAlex),
        (None, Some(_)) => Some(Source::SemanticScholar),
        (None, None) => None,
    }
}

/// How an article was tied to an external work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchMethod {
    /// Found in the works of the ORCID iD's OpenAlex author (Semantic Scholar: by that DOI).
    Orcid,
    /// Global title search with the strict threshold.
    TitleSearch,
    /// The user supplied the DOI; no title matching.
    ManualDoi,
}

impl MatchMethod {
    fn as_str(self) -> &'static str {
        match self {
            MatchMethod::Orcid => "orcid",
            MatchMethod::TitleSearch => "title_search",
            MatchMethod::ManualDoi => "manual_doi",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Work {
    pub id: String,
    pub doi: Option<String>,
    pub score: f64,
    pub method: MatchMethod,
}

/// What one source produced for one article.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SourceOutcome {
    /// The matched work, when the title matched.
    pub work: Option<Work>,
    /// All dated citing works; only set when every page was read.
    pub citing: Option<Vec<CitingRecord>>,
    /// Localized description of the failure that stopped this source, if any.
    pub error: Option<String>,
}

impl SourceOutcome {
    /// Number of dated citing records, or `None` when unmatched or incompletely read.
    fn usable_count(&self) -> Option<usize> {
        self.work.as_ref()?;
        self.citing.as_ref().map(Vec::len)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Matched {
        rec: Box<db::MatchRecord>,
        citing: Vec<CitingRecord>,
        /// Failure of the source that was not chosen (its count is then unknown).
        other_error: Option<String>,
    },
    /// Both sources answered and neither matched.
    Unmatched,
    /// Nothing usable and at least one source failed: retry later, store nothing.
    Failed(String),
}

/// Combines both sources' outcomes into the stored result.
pub fn decide(key: &str, oa: SourceOutcome, s2: SourceOutcome) -> Decision {
    let (oc, sc) = (oa.usable_count(), s2.usable_count());
    let Some(source) = choose_source(oc, sc) else {
        return match oa.error.or(s2.error) {
            Some(e) => Decision::Failed(e),
            None => Decision::Unmatched,
        };
    };
    let (chosen, other) = match source {
        Source::OpenAlex => (&oa, &s2),
        Source::SemanticScholar => (&s2, &oa),
    };
    let work = chosen.work.clone();
    let rec = db::MatchRecord {
        article_key: key.to_string(),
        source: Some(source.as_str().to_string()),
        external_id: work.as_ref().map(|w| w.id.clone()),
        doi: work.as_ref().and_then(|w| w.doi.clone()),
        score: work.as_ref().map(|w| w.score),
        match_method: work.as_ref().map(|w| w.method.as_str().to_string()),
        openalex_id: oa.work.as_ref().map(|w| w.id.clone()),
        s2_id: s2.work.as_ref().map(|w| w.id.clone()),
        openalex_count: oc.map(|n| n as i64),
        s2_count: sc.map(|n| n as i64),
    };
    let other_error = other.error.clone();
    let citing = match source {
        Source::OpenAlex => oa.citing,
        Source::SemanticScholar => s2.citing,
    }
    .unwrap_or_default();
    Decision::Matched {
        rec: Box::new(rec),
        citing,
        other_error,
    }
}

// ---------------------------------------------------------------------- HTTP

/// Spaces requests to one host. The gap can change while a run is active (a rejected API key
/// puts the host back on the keyless pace).
struct Pacer {
    gap_ms: std::sync::atomic::AtomicU64,
    last: tokio::sync::Mutex<Option<Instant>>,
}

impl Pacer {
    fn new(gap: Duration) -> Self {
        Pacer {
            gap_ms: std::sync::atomic::AtomicU64::new(gap.as_millis() as u64),
            last: tokio::sync::Mutex::new(None),
        }
    }

    fn set_gap(&self, gap: Duration) {
        self.gap_ms.store(gap.as_millis() as u64, Ordering::SeqCst);
    }

    async fn wait(&self) {
        let gap = Duration::from_millis(self.gap_ms.load(Ordering::SeqCst));
        let mut last = self.last.lock().await;
        if let Some(t) = *last {
            let elapsed = t.elapsed();
            if elapsed < gap {
                tokio::time::sleep(gap - elapsed).await;
            }
        }
        *last = Some(Instant::now());
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ApiError {
    RateLimited,
    Status(u16),
    Network(String),
    Parse(String),
    /// The user stopped the run.
    Cancelled,
}

impl ApiError {
    fn describe(&self, lang: &str) -> String {
        let text = match self {
            ApiError::RateLimited => i18n::tr(lang, "bf.err_rate").to_string(),
            ApiError::Status(c) => i18n::trf(lang, "bf.err_http", &[&c.to_string()]),
            ApiError::Network(m) => m.clone(),
            ApiError::Parse(m) => i18n::trf(lang, "bf.err_parse", &[m]),
            ApiError::Cancelled => i18n::tr(lang, "bf.err_cancelled").to_string(),
        };
        secrets::redact(&text, &[])
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Host {
    OpenAlex,
    Semantic,
}

impl Host {
    fn label(self) -> &'static str {
        match self {
            Host::OpenAlex => OPENALEX_LABEL,
            Host::Semantic => S2_LABEL,
        }
    }
}

/// What a request is for (the scan panel words it differently).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum What {
    /// Resolving the ORCID iD to an OpenAlex author.
    Author,
    /// The author's list of works.
    Works,
    /// Looking an article up (by DOI or by title).
    Lookup,
    /// The works that cite an article.
    Citing,
}

impl What {
    fn as_str(self) -> &'static str {
        match self {
            What::Author => "author",
            What::Works => "works",
            What::Lookup => "lookup",
            What::Citing => "citing",
        }
    }
}

/// A fine-grained step of a run, reported to the scan panel.
#[derive(Debug, Clone, Copy)]
pub enum Step<'a> {
    /// A request is about to be sent: page `page` of `what`, `records` collected so far.
    Request { source: &'a str, what: What, page: usize, records: usize },
    /// Waiting after HTTP 429 / 503: `secs` seconds left, retry number `attempt`.
    Wait { source: &'a str, secs: u64, attempt: usize },
}

type Say<'a> = &'a (dyn Fn(Step<'_>) + Sync);

/// What `Http::get` reports about one request.
#[derive(Clone, Copy)]
struct Req<'a> {
    say: Say<'a>,
    what: What,
    page: usize,
    records: usize,
}

/// HTTP client plus one pacer per host, and the optional API keys.
pub struct Http {
    client: reqwest::Client,
    oa: Pacer,
    s2: Pacer,
    keys: Mutex<Keys>,
    /// Keys the server refused (401 / 403); the host then continues without a key.
    rejected: Mutex<Vec<KeyKind>>,
    /// Set by the user's "Stop": requests end as soon as they notice it.
    cancel: Arc<AtomicBool>,
}

impl Http {
    pub fn new(keys: Keys) -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(40))
            .build()
            .map_err(|e| anyhow::anyhow!(secrets::redact(&e.without_url().to_string(), &[])))?;
        Ok(Http {
            client,
            oa: Pacer::new(secrets::oa_gap(keys.openalex.is_some())),
            s2: Pacer::new(secrets::s2_gap(keys.semantic.is_some())),
            keys: Mutex::new(keys),
            rejected: Mutex::new(Vec::new()),
            cancel: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Uses `flag` as the stop signal of this client.
    pub fn with_cancel(mut self, flag: Arc<AtomicBool>) -> Self {
        self.cancel = flag;
        self
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    /// Keys the servers refused during this run.
    pub fn rejected_keys(&self) -> Vec<KeyKind> {
        lock(&self.rejected).clone()
    }

    fn key_for(&self, host: Host) -> Option<String> {
        let k = lock(&self.keys);
        match host {
            Host::OpenAlex => k.openalex.clone(),
            Host::Semantic => k.semantic.clone(),
        }
    }

    /// Drops a refused key (once) and puts the host back on the keyless pace.
    fn drop_key(&self, host: Host) {
        let kind = match host {
            Host::OpenAlex => {
                lock(&self.keys).openalex = None;
                self.oa.set_gap(secrets::oa_gap(false));
                KeyKind::OpenAlex
            }
            Host::Semantic => {
                lock(&self.keys).semantic = None;
                self.s2.set_gap(secrets::s2_gap(false));
                KeyKind::SemanticScholar
            }
        };
        let mut rejected = lock(&self.rejected);
        if !rejected.contains(&kind) {
            rejected.push(kind);
        }
    }

    /// Sleeps `secs` seconds one second at a time, reporting the countdown and honouring "Stop".
    async fn wait_out(&self, host: Host, secs: u64, attempt: usize, say: Say<'_>) -> Result<(), ApiError> {
        let mut left = secs;
        while left > 0 {
            if self.is_cancelled() {
                return Err(ApiError::Cancelled);
            }
            say(Step::Wait { source: host.label(), secs: left, attempt });
            tokio::time::sleep(Duration::from_secs(1)).await;
            left -= 1;
        }
        if self.is_cancelled() {
            return Err(ApiError::Cancelled);
        }
        Ok(())
    }

    /// GET with pacing and 429/503 backoff. `Ok(None)` for HTTP 404. The key (if any) travels in
    /// a header, never in the URL; a 401 / 403 with a key retries once without it.
    async fn get(&self, host: Host, url: &str, query: &[(&str, &str)], req_info: Req<'_>) -> Result<Option<String>, ApiError> {
        let (pacer, backoff) = match host {
            Host::OpenAlex => (&self.oa, OA_BACKOFF),
            Host::Semantic => (&self.s2, S2_BACKOFF),
        };
        let mut attempt = 0usize;
        loop {
            if self.is_cancelled() {
                return Err(ApiError::Cancelled);
            }
            (req_info.say)(Step::Request {
                source: host.label(),
                what: req_info.what,
                page: req_info.page,
                records: req_info.records,
            });
            pacer.wait().await;
            if self.is_cancelled() {
                return Err(ApiError::Cancelled);
            }
            let key = self.key_for(host);
            let mut req = self.client.get(url).query(query);
            if let Some(k) = &key {
                let (name, value) = match host {
                    Host::OpenAlex => ("authorization", format!("Bearer {k}")),
                    Host::Semantic => ("x-api-key", k.clone()),
                };
                if let Ok(mut v) = reqwest::header::HeaderValue::from_str(&value) {
                    v.set_sensitive(true);
                    req = req.header(name, v);
                }
            }
            let resp = req
                .send()
                .await
                .map_err(|e| ApiError::Network(secrets::redact(&e.without_url().to_string(), &[])))?;
            let code = resp.status().as_u16();
            if code == 404 {
                return Ok(None);
            }
            if (code == 401 || code == 403) && key.is_some() {
                self.drop_key(host);
                continue;
            }
            if code == 429 || code == 503 {
                match backoff.get(attempt) {
                    Some(secs) => {
                        attempt += 1;
                        self.wait_out(host, *secs, attempt, req_info.say).await?;
                        continue;
                    }
                    None => return Err(ApiError::RateLimited),
                }
            }
            if !(200..300).contains(&code) {
                return Err(ApiError::Status(code));
            }
            let body = resp
                .text()
                .await
                .map_err(|e| ApiError::Network(secrets::redact(&e.without_url().to_string(), &[])))?;
            return Ok(Some(body));
        }
    }
}

/// Percent-encodes everything but unreserved characters and `/ : ( )` (enough for DOIs).
fn encode_path(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'/' | b':' | b'(' | b')') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Pages through `works?filter=<filter>` and returns every work (candidate fields only).
async fn oa_works_pages(http: &Http, filter: &str, say: Say<'_>, lang: &str) -> Result<Vec<Candidate>, String> {
    let mut all: Vec<Candidate> = Vec::new();
    let mut cursor = "*".to_string();
    for page in 1..=MAX_PAGES {
        let body = match http
            .get(
                Host::OpenAlex,
                OPENALEX_WORKS,
                &[
                    ("filter", filter),
                    ("per-page", "200"),
                    ("select", "id,doi,title,publication_year,cited_by_count"),
                    ("cursor", cursor.as_str()),
                ],
                Req { say, what: What::Works, page, records: all.len() },
            )
            .await
        {
            Ok(Some(b)) => b,
            Ok(None) => break,
            Err(e) => return Err(e.describe(lang)),
        };
        let (mut page, next) =
            parse_oa_works(&body).map_err(|e| ApiError::Parse(e.to_string()).describe(lang))?;
        all.append(&mut page);
        match next {
            Some(n) => cursor = n,
            None => break,
        }
    }
    Ok(all)
}

/// Resolves the ORCID iD to an OpenAlex author and collects that author's works: the union of
/// `author.orcid:<orcid>` and `authorships.author.id:<author id>` (OpenAlex often attaches the
/// ORCID to fewer works than the author record has).
async fn load_author_set(http: &Http, orcid: &str, say: Say<'_>, lang: &str) -> Result<AuthorSet, String> {
    let url = format!("{OPENALEX_AUTHORS}/orcid:{orcid}");
    let body = http
        .get(
            Host::OpenAlex,
            &url,
            &[("select", "id,works_count")],
            Req { say, what: What::Author, page: 1, records: 0 },
        )
        .await
        .map_err(|e| e.describe(lang))?;
    let mut set = AuthorSet::default();
    if let Some(body) = body {
        let (id, count) = parse_oa_author(&body).map_err(|e| ApiError::Parse(e.to_string()).describe(lang))?;
        set.author_id = Some(id);
        set.works_count = count;
    }
    let mut works = oa_works_pages(http, &format!("author.orcid:{orcid}"), say, lang).await?;
    if let Some(id) = &set.author_id {
        works.extend(oa_works_pages(http, &format!("authorships.author.id:{id}"), say, lang).await?);
    }
    set.works = dedupe_candidates(works);
    Ok(set)
}

/// All dated citing works of one OpenAlex work.
async fn oa_citing(http: &Http, work_id: &str, say: Say<'_>, lang: &str) -> Result<Vec<CitingRecord>, String> {
    let filter = format!("cites:{work_id}");
    let mut all: Vec<CitingRecord> = Vec::new();
    let mut cursor = "*".to_string();
    for page in 1..=MAX_PAGES {
        let body = match http
            .get(
                Host::OpenAlex,
                OPENALEX_WORKS,
                &[
                    ("filter", filter.as_str()),
                    ("per-page", "200"),
                    ("select", "id,doi,title,publication_date,publication_year,primary_location,authorships"),
                    ("cursor", cursor.as_str()),
                ],
                Req { say, what: What::Citing, page, records: all.len() },
            )
            .await
        {
            Ok(Some(b)) => b,
            Ok(None) => break,
            Err(e) => return Err(e.describe(lang)),
        };
        let (mut page, next) =
            parse_oa_citing(&body).map_err(|e| ApiError::Parse(e.to_string()).describe(lang))?;
        all.append(&mut page);
        match next {
            Some(n) => cursor = n,
            None => break,
        }
    }
    Ok(dedupe(all))
}

/// An article's match inside the ORCID author set.
#[derive(Debug, Clone, Copy)]
struct Assigned<'a> {
    cand: &'a Candidate,
    score: f64,
}

/// What both sources need to know about one article.
#[derive(Clone, Copy)]
struct Lookup<'a> {
    title: &'a str,
    year: Option<i64>,
    /// Works the user ruled out for this article (OpenAlex and Semantic Scholar ids).
    rejected: &'a HashSet<String>,
}

/// OpenAlex side for a manual DOI: the work is resolved by `works/doi:<doi>`, then its citing works.
async fn openalex_manual_flow(http: &Http, doi: &str, say: Say<'_>, lang: &str) -> SourceOutcome {
    let mut out = SourceOutcome::default();
    let url = format!("{OPENALEX_WORKS}/doi:{}", encode_path(doi));
    let body = match http
        .get(
            Host::OpenAlex,
            &url,
            &[("select", "id,doi,title,publication_year")],
            Req { say, what: What::Lookup, page: 1, records: 0 },
        )
        .await
    {
        Ok(Some(b)) => b,
        Ok(None) => return out,
        Err(e) => {
            out.error = Some(e.describe(lang));
            return out;
        }
    };
    let cand = match parse_oa_work(&body) {
        Ok(Some(c)) => c,
        Ok(None) => return out,
        Err(e) => {
            out.error = Some(ApiError::Parse(e.to_string()).describe(lang));
            return out;
        }
    };
    let id = cand.id.clone();
    out.work = Some(Work {
        id: cand.id,
        doi: Some(doi.to_string()),
        score: 1.0,
        method: MatchMethod::ManualDoi,
    });
    match oa_citing(http, &id, say, lang).await {
        Ok(c) => out.citing = Some(c),
        Err(e) => out.error = Some(e),
    }
    out
}

/// OpenAlex side of one article: the assigned work (ORCID set) or, failing that, a global
/// title search with the stricter threshold; then every dated citing work.
async fn openalex_flow(
    http: &Http,
    q: Lookup<'_>,
    assigned: Option<Assigned<'_>>,
    say: Say<'_>,
    lang: &str,
) -> SourceOutcome {
    let (title, year) = (q.title, q.year);
    let mut out = SourceOutcome::default();
    let work = match assigned {
        Some(a) => Work {
            id: a.cand.id.clone(),
            doi: a.cand.doi.clone(),
            score: a.score,
            method: MatchMethod::Orcid,
        },
        None => {
            let body = match http
                .get(
                    Host::OpenAlex,
                    OPENALEX_WORKS,
                    &[
                        ("search", title),
                        ("per-page", "5"),
                        ("select", "id,doi,title,publication_year,cited_by_count"),
                    ],
                    Req { say, what: What::Lookup, page: 1, records: 0 },
                )
                .await
            {
                Ok(Some(b)) => b,
                Ok(None) => return out,
                Err(e) => {
                    out.error = Some(e.describe(lang));
                    return out;
                }
            };
            let cands = match parse_oa_search(&body) {
                Ok(c) => c,
                Err(e) => {
                    out.error = Some(ApiError::Parse(e.to_string()).describe(lang));
                    return out;
                }
            };
            let Some((c, score)) = best_candidate_excluding(title, year, &cands, q.rejected) else {
                return out;
            };
            Work {
                id: c.id,
                doi: c.doi,
                score,
                method: MatchMethod::TitleSearch,
            }
        }
    };
    let id = work.id.clone();
    out.work = Some(work);
    match oa_citing(http, &id, say, lang).await {
        Ok(c) => out.citing = Some(c),
        Err(e) => out.error = Some(e),
    }
    out
}

/// Semantic Scholar side of one article. With a DOI (from the ORCID set) the paper is looked
/// up by `DOI:<doi>`; without one, or when the DOI is unknown there, by title match (strict).
async fn s2_flow(
    http: &Http,
    q: Lookup<'_>,
    doi_hint: Option<(&str, f64, MatchMethod)>,
    say: Say<'_>,
    lang: &str,
) -> SourceOutcome {
    let (title, year) = (q.title, q.year);
    let mut out = SourceOutcome::default();
    // A manual DOI is final: when Semantic Scholar does not know it, there is no title fallback.
    let manual = matches!(doi_hint, Some((_, _, MatchMethod::ManualDoi)));
    if let Some((doi, score, method)) = doi_hint {
        let url = format!("{S2_BASE}/paper/DOI:{}", encode_path(doi));
        match http
            .get(
                Host::Semantic,
                &url,
                &[("fields", "title,year,externalIds")],
                Req { say, what: What::Lookup, page: 1, records: 0 },
            )
            .await
        {
            Ok(Some(body)) => match parse_s2_paper(&body) {
                Ok(Some(c)) if !q.rejected.contains(&c.id) => {
                    out.work = Some(Work {
                        id: c.id,
                        doi: Some(doi.to_string()),
                        score,
                        method,
                    });
                }
                Ok(Some(_)) => {}
                Ok(None) => {}
                Err(e) => {
                    out.error = Some(ApiError::Parse(e.to_string()).describe(lang));
                    return out;
                }
            },
            Ok(None) => {}
            Err(e) => {
                out.error = Some(e.describe(lang));
                return out;
            }
        }
    }
    if out.work.is_none() && !manual {
        let url = format!("{S2_BASE}/paper/search/match");
        let body = match http
            .get(
                Host::Semantic,
                &url,
                &[("query", title), ("fields", "title,year,externalIds")],
                Req { say, what: What::Lookup, page: 1, records: 0 },
            )
            .await
        {
            Ok(Some(b)) => b,
            Ok(None) => return out,
            Err(e) => {
                out.error = Some(e.describe(lang));
                return out;
            }
        };
        let cands = match parse_s2_match(&body) {
            Ok(c) => c,
            Err(e) => {
                out.error = Some(ApiError::Parse(e.to_string()).describe(lang));
                return out;
            }
        };
        let Some((c, score)) = best_candidate_excluding(title, year, &cands, q.rejected) else {
            return out;
        };
        out.work = Some(Work {
            id: c.id,
            doi: c.doi,
            score,
            method: MatchMethod::TitleSearch,
        });
    }
    let Some(paper_id) = out.work.as_ref().map(|w| w.id.clone()) else {
        return out;
    };
    match s2_citing(http, &paper_id, say, lang).await {
        Ok(c) => out.citing = Some(c),
        Err(e) => out.error = Some(e),
    }
    out
}

/// All dated citing works of one Semantic Scholar paper.
async fn s2_citing(http: &Http, paper_id: &str, say: Say<'_>, lang: &str) -> Result<Vec<CitingRecord>, String> {
    let url = format!("{S2_BASE}/paper/{paper_id}/citations");
    let mut all: Vec<CitingRecord> = Vec::new();
    let mut offset = 0i64;
    for page in 1..=MAX_PAGES {
        let off = offset.to_string();
        let body = match http
            .get(
                Host::Semantic,
                &url,
                &[
                    ("fields", "title,publicationDate,year,venue,publicationVenue,externalIds,authors"),
                    ("limit", "100"),
                    ("offset", off.as_str()),
                ],
                Req { say, what: What::Citing, page, records: all.len() },
            )
            .await
        {
            Ok(Some(b)) => b,
            Ok(None) => break,
            Err(e) => return Err(e.describe(lang)),
        };
        let (mut page, next) =
            parse_s2_citations(&body).map_err(|e| ApiError::Parse(e.to_string()).describe(lang))?;
        all.append(&mut page);
        match next {
            Some(n) => offset = n,
            None => break,
        }
    }
    Ok(dedupe(all))
}

// -------------------------------------------------------------------- runner

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Only articles that have no match row yet.
    Missing,
    /// Every article of the latest snapshot (upsert).
    All,
    /// One article (after the user changed its DOI or rejected its match).
    Article(String),
    /// Daily light run: re-reads the citing lists of articles that are already matched, from the
    /// source that was chosen for them. No matching, no author set, no second source.
    Refresh,
}

impl Mode {
    /// Name used in the progress events ("missing", "all", "article", "refresh").
    pub fn as_str(&self) -> &'static str {
        match self {
            Mode::Missing => "missing",
            Mode::All => "all",
            Mode::Article(_) => "article",
            Mode::Refresh => "refresh",
        }
    }
}

/// One article of a run, as the scan panel lists it.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct ScanTarget {
    pub key: String,
    pub title: String,
}

/// A progress event. The same struct is the "current state" kept for the header pill and the
/// payload of the `backfill-progress` event. `event` says which fields are filled:
///
/// - `preparing`: the run was accepted, nothing is known yet.
/// - `start`: `targets` lists every article of the run.
/// - `request`: a request is being sent (`source`, `what`, `page`, `records`; `k`/`key` during
///   an article, `k == 0` while the author set is loaded).
/// - `backoff`: waiting after HTTP 429 / 503 (`source`, `wait_secs` left, `attempt`).
/// - `works`: the author's works were collected (`works`); `matching`: they were assigned to
///   articles (`works`, `records` = assigned).
/// - `article_start`, `article_done` (`oa_count`, `s2_count`, `chosen`, `method`, `records`),
///   `unmatched`, `error` (`message`).
/// - `finished`, `cancelled`, `failed`: the run ended (`running == false`, `summary`).
///
/// Every event also carries `done` (articles finished), `elapsed_secs` and `eta_secs`.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Progress {
    pub running: bool,
    pub k: usize,
    pub n: usize,
    pub source: String,
    pub title: String,
    pub event: String,
    pub mode: String,
    pub key: String,
    pub what: String,
    pub page: usize,
    pub records: usize,
    pub wait_secs: u64,
    pub attempt: usize,
    pub works: usize,
    pub oa_count: Option<i64>,
    pub s2_count: Option<i64>,
    pub chosen: String,
    pub method: String,
    pub message: String,
    pub done: usize,
    pub elapsed_secs: u64,
    pub eta_secs: Option<u64>,
    pub targets: Vec<ScanTarget>,
    pub summary: Option<RunSummary>,
}

/// Remaining seconds, extrapolated from the average time per finished article. `None` until
/// the first article is done.
pub fn eta_secs(done: usize, total: usize, elapsed_secs: u64) -> Option<u64> {
    if total > 0 && done >= total {
        return Some(0);
    }
    if done == 0 {
        return None;
    }
    Some(elapsed_secs * (total - done) as u64 / done as u64)
}

/// Builds the events of one run: fills in the mode, the counters, the elapsed time and the ETA.
struct Scan<'a> {
    on: &'a (dyn Fn(Progress) + Sync),
    mode: &'static str,
    started: Instant,
    n: usize,
    done: AtomicUsize,
    k: AtomicUsize,
}

impl<'a> Scan<'a> {
    fn new(on: &'a (dyn Fn(Progress) + Sync), mode: &'static str, n: usize) -> Self {
        Scan { on, mode, started: Instant::now(), n, done: AtomicUsize::new(0), k: AtomicUsize::new(0) }
    }

    fn emit(&self, event: &str, fill: impl FnOnce(&mut Progress)) {
        let elapsed = self.started.elapsed().as_secs();
        let done = self.done.load(Ordering::SeqCst);
        let mut p = Progress {
            running: true,
            k: self.k.load(Ordering::SeqCst),
            n: self.n,
            event: event.to_string(),
            mode: self.mode.to_string(),
            done,
            elapsed_secs: elapsed,
            eta_secs: eta_secs(done, self.n, elapsed),
            ..Progress::default()
        };
        fill(&mut p);
        (self.on)(p);
    }

    /// Translates a request / wait step of the HTTP layer into an event.
    fn step(&self, step: Step<'_>, key: &str, title: &str) {
        match step {
            Step::Request { source, what, page, records } => self.emit("request", |p| {
                p.source = source.to_string();
                p.what = what.as_str().to_string();
                p.page = page;
                p.records = records;
                p.key = key.to_string();
                p.title = title.to_string();
            }),
            Step::Wait { source, secs, attempt } => self.emit("backoff", |p| {
                p.source = source.to_string();
                p.wait_secs = secs;
                p.attempt = attempt;
                p.key = key.to_string();
                p.title = title.to_string();
            }),
        }
    }

    fn start(&self, targets: &[(String, String)]) {
        self.emit("start", |p| {
            p.targets = targets.iter().map(|(k, t)| ScanTarget { key: k.clone(), title: t.clone() }).collect();
        });
    }

    fn article_start(&self, k: usize, key: &str, title: &str) {
        self.k.store(k, Ordering::SeqCst);
        self.emit("article_start", |p| {
            p.key = key.to_string();
            p.title = title.to_string();
        });
    }

    /// Marks the article finished and reports how it ended (`event` is the terminal event).
    fn article_end(&self, event: &str, key: &str, title: &str, fill: impl FnOnce(&mut Progress)) {
        self.done.fetch_add(1, Ordering::SeqCst);
        self.emit(event, |p| {
            p.key = key.to_string();
            p.title = title.to_string();
            fill(p);
        });
    }
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct RunSummary {
    pub total: usize,
    pub matched: usize,
    pub unmatched: usize,
    pub failed: usize,
    pub records: usize,
    /// Articles matched inside the ORCID author set.
    pub by_orcid: usize,
    /// Citing works that were not stored before (empty on a first run and for single-article runs).
    pub new_total: usize,
    /// The newest few of them, for the notification.
    pub new_citing: Vec<NewCiting>,
    /// The user stopped the run before it was complete.
    pub cancelled: bool,
}

/// A newly found citing work, as the notification needs it.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct NewCiting {
    pub article: String,
    pub title: String,
    pub venue: String,
    pub date: String,
}

/// How many new citing works are kept for the notification.
const KEEP_NEW: usize = 6;

/// Adds `fresh` citing works of `article` to the summary (newest first, capped).
fn note_new(sum: &mut RunSummary, article: &str, fresh: Vec<CitingRecord>) {
    sum.new_total += fresh.len();
    for c in fresh {
        sum.new_citing.push(NewCiting {
            article: article.to_string(),
            title: c.citing_title,
            venue: c.venue,
            date: c.citing_date,
        });
    }
    sum.new_citing.sort_by(|a, b| b.date.cmp(&a.date).then(a.title.cmp(&b.title)));
    sum.new_citing.truncate(KEEP_NEW);
}

/// Looks up every target article in both sources, stores the result, and records the run.
/// The ORCID author set is loaded once per run; articles outside it fall back to title search.
/// `None` when there was nothing to do (no run row is written then). When the user stops the
/// run (`http.is_cancelled()`), the article in progress is dropped, the rest is skipped and the
/// run is recorded as `cancelled`.
pub async fn run_articles(
    db_path: &Path,
    mode: Mode,
    http: &Http,
    lang: &str,
    orcid: &str,
    on_progress: &(dyn Fn(Progress) + Sync),
) -> Result<Option<RunSummary>> {
    if mode == Mode::Refresh {
        return refresh_articles(db_path, http, lang, on_progress).await;
    }
    let mut conn = db::open(db_path)?;
    let targets = match &mode {
        Mode::Article(key) => db::backfill_target_for(&conn, key)?.into_iter().collect(),
        m => db::backfill_targets(&conn, *m == Mode::Missing)?,
    };
    if targets.is_empty() {
        return Ok(None);
    }
    let started = Utc::now();
    let run_id = db::start_run(&conn, started, targets.len())?;
    let n = targets.len();
    let scan = Scan::new(on_progress, mode.as_str(), n);
    scan.start(&targets.iter().map(|t| (t.key.clone(), t.title.clone())).collect::<Vec<_>>());
    let mut sum = RunSummary {
        total: n,
        ..RunSummary::default()
    };
    let mut gaps = 0usize;
    let mut first_error: Option<String> = None;
    let mut notes: Vec<String> = Vec::new();
    let mut cancelled = false;

    let overrides = db::all_overrides(&conn)?;
    // The author set is only needed for articles that are matched automatically.
    let needs_set = targets
        .iter()
        .any(|t| plan_for(overrides.get(&t.key)) == Plan::Automatic);

    // Step 1-2: the author's works, then one-to-one assignment of articles to them.
    let mut set = AuthorSet::default();
    if !orcid.is_empty() && needs_set {
        let say_set = |step: Step<'_>| scan.step(step, "", "");
        match load_author_set(http, orcid, &say_set, lang).await {
            Ok(s) => set = s,
            Err(e) => notes.push(i18n::trf(lang, "bf.msg_orcid_failed", &[&e])),
        }
        cancelled = http.is_cancelled();
        if !cancelled {
            scan.emit("works", |p| p.works = set.works.len());
        }
    }
    // In "missing" mode, works already taken by earlier matches stay taken.
    let excluded = match &mode {
        Mode::Missing => db::matched_openalex_ids(&conn)?,
        Mode::Article(key) => db::matched_openalex_ids_except(&conn, key)?,
        Mode::All | Mode::Refresh => HashSet::new(),
    };
    let assignments = plan_assignments(&targets, &overrides, &set.works, &excluded);
    sum.by_orcid = assignments.iter().flatten().count();
    if !cancelled {
        scan.emit("matching", |p| {
            p.works = set.works.len();
            p.records = sum.by_orcid;
        });
    }
    if needs_set && set.author_id.is_none() && set.works.is_empty() && !orcid.is_empty() && notes.is_empty() {
        notes.push(i18n::tr(lang, "bf.msg_orcid_none").to_string());
    } else if !set.works.is_empty() {
        notes.push(i18n::trf(
            lang,
            "bf.msg_orcid",
            &[&set.works.len().to_string(), &sum.by_orcid.to_string()],
        ));
    }

    // Step 3-6: both sources for each article.
    for (i, target) in targets.iter().enumerate() {
        if cancelled || http.is_cancelled() {
            cancelled = true;
            break;
        }
        let k = i + 1;
        let (key, title) = (target.key.clone(), target.title.clone());
        let say = |step: Step<'_>| scan.step(step, &key, &title);
        scan.article_start(k, &key, &title);
        let assigned = assignments[i].map(|a| Assigned {
            cand: &set.works[a.cand],
            score: a.score,
        });
        let ov = overrides.get(&target.key);
        let rejected = ov.map(db::Override::rejected_set).unwrap_or_default();
        let q = Lookup {
            title: &target.title,
            year: target.year,
            rejected: &rejected,
        };
        let plan = plan_for(ov);
        let (oa, s2) = match &plan {
            Plan::Manual(doi) => tokio::join!(
                openalex_manual_flow(http, doi, &say, lang),
                s2_flow(http, q, Some((doi.as_str(), 1.0, MatchMethod::ManualDoi)), &say, lang),
            ),
            Plan::Automatic => {
                let doi_hint = assigned
                    .and_then(|a| a.cand.doi.as_deref().map(|d| (d, a.score, MatchMethod::Orcid)));
                tokio::join!(
                    openalex_flow(http, q, assigned, &say, lang),
                    s2_flow(http, q, doi_hint, &say, lang),
                )
            }
        };
        if http.is_cancelled() {
            // The article in progress is dropped: a half-read citing list must not be stored.
            cancelled = true;
            break;
        }
        match decide(&target.key, oa, s2) {
            Decision::Matched {
                rec,
                citing,
                other_error,
            } => match db::store_backfill_article(&mut conn, &rec, Utc::now(), &citing) {
                Ok(fresh) => {
                    sum.matched += 1;
                    sum.records += citing.len();
                    // A DOI correction changes which work an article is tied to: not "new" citations.
                    if !matches!(mode, Mode::Article(_)) {
                        note_new(&mut sum, &target.title, fresh);
                    }
                    if other_error.is_some() {
                        gaps += 1;
                    }
                    scan.article_end("article_done", &key, &title, |p| {
                        p.oa_count = rec.openalex_count;
                        p.s2_count = rec.s2_count;
                        p.chosen = rec.source.clone().unwrap_or_default();
                        p.method = rec.match_method.clone().unwrap_or_default();
                        p.records = citing.len();
                        p.message = other_error.clone().unwrap_or_default();
                    });
                }
                Err(e) => {
                    sum.failed += 1;
                    let msg = format!("{e:#}");
                    first_error.get_or_insert_with(|| msg.clone());
                    scan.article_end("error", &key, &title, |p| p.message = secrets::redact(&msg, &[]));
                }
            },
            Decision::Unmatched => {
                // For a manual DOI the record keeps the DOI and the method, so the UI can say
                // that this DOI was not found instead of showing a plain "unmatched".
                let rec = match &plan {
                    Plan::Manual(doi) => db::MatchRecord {
                        article_key: target.key.clone(),
                        doi: Some(doi.clone()),
                        match_method: Some(MatchMethod::ManualDoi.as_str().to_string()),
                        ..db::MatchRecord::default()
                    },
                    Plan::Automatic => db::MatchRecord {
                        article_key: target.key.clone(),
                        ..db::MatchRecord::default()
                    },
                };
                match db::store_backfill_article(&mut conn, &rec, Utc::now(), &[]) {
                    Ok(_) => {
                        sum.unmatched += 1;
                        scan.article_end("unmatched", &key, &title, |_| {});
                    }
                    Err(e) => {
                        sum.failed += 1;
                        let msg = format!("{e:#}");
                        first_error.get_or_insert_with(|| msg.clone());
                        scan.article_end("error", &key, &title, |p| p.message = secrets::redact(&msg, &[]));
                    }
                }
            }
            Decision::Failed(msg) => {
                sum.failed += 1;
                first_error.get_or_insert_with(|| msg.clone());
                scan.article_end("error", &key, &title, |p| p.message = secrets::redact(&msg, &[]));
            }
        }
    }

    for kind in http.rejected_keys() {
        notes.push(i18n::tr(lang, key_rejected_msg(kind)).to_string());
    }
    sum.cancelled = cancelled;
    let status = if cancelled {
        "cancelled"
    } else if sum.failed == 0 {
        "ok"
    } else if sum.failed == n {
        "error"
    } else {
        "partial"
    };
    let mut message = if cancelled {
        i18n::trf(lang, "bf.msg_cancelled", &[&scan.done.load(Ordering::SeqCst).to_string(), &n.to_string()])
    } else {
        i18n::trf(
            lang,
            "bf.msg_done",
            &[&sum.matched.to_string(), &n.to_string(), &sum.records.to_string()],
        )
    };
    for note in notes {
        message.push(' ');
        message.push_str(&note);
    }
    if gaps > 0 {
        message.push(' ');
        message.push_str(&i18n::trf(lang, "bf.msg_gap", &[&gaps.to_string()]));
    }
    if sum.failed > 0 {
        message.push(' ');
        message.push_str(&i18n::trf(
            lang,
            "bf.msg_failed",
            &[&sum.failed.to_string(), first_error.as_deref().unwrap_or("?")],
        ));
    }
    db::finish_run(&conn, run_id, Utc::now(), status, (sum.matched, sum.records), &message)?;
    Ok(Some(sum))
}

fn key_rejected_msg(kind: KeyKind) -> &'static str {
    match kind {
        KeyKind::OpenAlex => "bf.msg_key_oa",
        KeyKind::SemanticScholar => "bf.msg_key_s2",
    }
}

/// The daily light run: for each matched article the citing list is read again from the
/// source chosen for it, using the stored work id. Nothing is matched again.
async fn refresh_articles(
    db_path: &Path,
    http: &Http,
    lang: &str,
    on_progress: &(dyn Fn(Progress) + Sync),
) -> Result<Option<RunSummary>> {
    let mut conn = db::open(db_path)?;
    let targets = db::refresh_targets(&conn)?;
    if targets.is_empty() {
        return Ok(None);
    }
    let n = targets.len();
    let run_id = db::start_run(&conn, Utc::now(), n)?;
    let scan = Scan::new(on_progress, Mode::Refresh.as_str(), n);
    scan.start(&targets.iter().map(|t| (t.key.clone(), t.title.clone())).collect::<Vec<_>>());
    let mut sum = RunSummary {
        total: n,
        ..RunSummary::default()
    };
    let mut first_error: Option<String> = None;
    let mut cancelled = false;
    for (i, target) in targets.iter().enumerate() {
        if http.is_cancelled() {
            cancelled = true;
            break;
        }
        let k = i + 1;
        let (key, title) = (target.key.clone(), target.title.clone());
        let say = |step: Step<'_>| scan.step(step, &key, &title);
        scan.article_start(k, &key, &title);
        let fetched = match target.source.as_str() {
            "openalex" => oa_citing(http, &target.external_id, &say, lang).await,
            "semanticscholar" => s2_citing(http, &target.external_id, &say, lang).await,
            _ => continue,
        };
        if http.is_cancelled() {
            cancelled = true;
            break;
        }
        match fetched {
            Ok(citing) => match db::refresh_citing(&mut conn, &target.key, &target.source, &citing) {
                Ok(fresh) => {
                    sum.matched += 1;
                    sum.records += citing.len();
                    note_new(&mut sum, &target.title, fresh);
                    scan.article_end("article_done", &key, &title, |p| {
                        let c = Some(citing.len() as i64);
                        if target.source == "openalex" {
                            p.oa_count = c;
                        } else {
                            p.s2_count = c;
                        }
                        p.chosen = target.source.clone();
                        p.method = "refresh".to_string();
                        p.records = citing.len();
                    });
                }
                Err(e) => {
                    sum.failed += 1;
                    let msg = format!("{e:#}");
                    first_error.get_or_insert_with(|| msg.clone());
                    scan.article_end("error", &key, &title, |p| p.message = secrets::redact(&msg, &[]));
                }
            },
            Err(msg) => {
                sum.failed += 1;
                first_error.get_or_insert_with(|| msg.clone());
                scan.article_end("error", &key, &title, |p| p.message = secrets::redact(&msg, &[]));
            }
        }
    }
    sum.cancelled = cancelled;
    let status = if cancelled {
        "cancelled"
    } else if sum.failed == 0 {
        "ok"
    } else if sum.failed == n {
        "error"
    } else {
        "partial"
    };
    let mut message = if cancelled {
        i18n::trf(lang, "bf.msg_cancelled", &[&scan.done.load(Ordering::SeqCst).to_string(), &n.to_string()])
    } else {
        i18n::trf(
            lang,
            "bf.msg_refresh",
            &[&sum.matched.to_string(), &n.to_string(), &sum.records.to_string(), &sum.new_total.to_string()],
        )
    };
    for kind in http.rejected_keys() {
        message.push(' ');
        message.push_str(i18n::tr(lang, key_rejected_msg(kind)));
    }
    if sum.failed > 0 {
        message.push(' ');
        message.push_str(&i18n::trf(
            lang,
            "bf.msg_failed",
            &[&sum.failed.to_string(), first_error.as_deref().unwrap_or("?")],
        ));
    }
    let message = secrets::redact(&message, &[]);
    db::finish_run(&conn, run_id, Utc::now(), status, (sum.matched, sum.records), &message)?;
    Ok(Some(sum))
}

/// Minimum time between two daily refreshes.
pub const REFRESH_EVERY_HOURS: i64 = 24;

/// Pure rule for the daily refresh: backfill is on, at least one article is matched, and the
/// last refresh (or any run that stored citing lists) is 24 hours old or there was none.
pub fn refresh_due(
    enabled: bool,
    last: Option<chrono::DateTime<Utc>>,
    now: chrono::DateTime<Utc>,
    has_matches: bool,
) -> bool {
    enabled && has_matches && last.is_none_or(|l| now - l >= chrono::Duration::hours(REFRESH_EVERY_HOURS))
}

/// Background loop: every ten minutes, starts the light refresh when it is due.
pub async fn refresh_loop(app: AppHandle) {
    tokio::time::sleep(Duration::from_secs(120)).await;
    loop {
        refresh_if_due(&app).await;
        tokio::time::sleep(Duration::from_secs(600)).await;
    }
}

async fn refresh_if_due(app: &AppHandle) {
    let Some(state) = app.try_state::<Arc<Shared>>() else {
        return;
    };
    let sh = state.inner().clone();
    let (enabled, path) = {
        let s = lock(&sh.settings);
        (
            s.backfill_enabled && !s.profile_id.is_empty() && !s.orcid.is_empty(),
            db::db_path_in(&s.data_path()),
        )
    };
    if !enabled || sh.is_running() || !path.exists() {
        return;
    }
    let due = tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<bool> {
        let conn = db::open(&path)?;
        let last = db::get_state(&conn, db::STATE_REFRESH)?.and_then(|s| db::parse_ts(&s));
        Ok(refresh_due(true, last, Utc::now(), db::has_refresh_targets(&conn)?))
    })
    .await;
    if matches!(due, Ok(Ok(true))) {
        start(app, &sh, Mode::Refresh);
    }
}

/// Shows the "new citing work" notification for the run (nothing when there is none).
fn notify_new_citing(app: &AppHandle, lang: &str, sum: &RunSummary) {
    use tauri_plugin_notification::NotificationExt;
    if sum.new_total == 0 {
        return;
    }
    let title = i18n::trf(lang, "notif.citing_title", &[&sum.new_total.to_string()]);
    let lines: Vec<String> = sum.new_citing.iter().take(2).map(|c| citing_line(lang, c)).collect();
    if let Err(e) = app.notification().builder().title(title).body(lines.join("\n")).show() {
        eprintln!("notification failed: {e}");
    }
}

/// "X dergisinde 'Başlık…' makalenize atıf yaptı" / English equivalent (venue optional).
pub fn citing_line(lang: &str, c: &NewCiting) -> String {
    let title = shorten_title(&c.title, 70);
    let t = if title.is_empty() { "?".to_string() } else { title };
    if c.venue.trim().is_empty() {
        i18n::trf(lang, "notif.citing_line_nv", &[&t])
    } else {
        i18n::trf(lang, "notif.citing_line", &[&t, c.venue.trim()])
    }
}

fn shorten_title(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{}…", cut.trim_end())
    }
}

// ------------------------------------------------------- app integration

/// What the scan panel needs to rebuild itself after a reload: every event since the run
/// started (the high-volume `request` / `backoff` ones are collapsed into `live`, the newest
/// per source).
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct ScanState {
    pub events: Vec<Progress>,
    pub live: Vec<Progress>,
}

/// Longest event history kept (the oldest events after the first are dropped).
const MAX_EVENTS: usize = 2000;

impl ScanState {
    /// Folds one event into the state.
    pub fn record(&mut self, p: &Progress) {
        match p.event.as_str() {
            "request" | "backoff" => {
                match self.live.iter_mut().find(|l| l.source == p.source) {
                    Some(l) => *l = p.clone(),
                    None => self.live.push(p.clone()),
                }
            }
            "start" | "preparing" => {
                self.events.clear();
                self.live.clear();
                self.events.push(p.clone());
            }
            _ => {
                self.live.clear();
                self.events.push(p.clone());
                if self.events.len() > MAX_EVENTS {
                    self.events.drain(1..self.events.len() - MAX_EVENTS + 1);
                }
            }
        }
    }
}

/// State shared between the commands, the scheduler hook and the background task.
pub struct Shared {
    running: AtomicBool,
    cancel: Arc<AtomicBool>,
    progress: Mutex<Progress>,
    scan: Mutex<ScanState>,
    settings: Arc<Mutex<Settings>>,
}

impl Shared {
    pub fn new(settings: Arc<Mutex<Settings>>) -> Arc<Self> {
        Arc::new(Shared {
            running: AtomicBool::new(false),
            cancel: Arc::new(AtomicBool::new(false)),
            progress: Mutex::new(Progress::default()),
            scan: Mutex::new(ScanState::default()),
            settings,
        })
    }

    pub fn progress(&self) -> Progress {
        lock(&self.progress).clone()
    }

    pub fn scan_state(&self) -> ScanState {
        lock(&self.scan).clone()
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Asks the running scan to stop after the current request. False when nothing is running.
    pub fn cancel(&self) -> bool {
        if self.is_running() {
            self.cancel.store(true, Ordering::SeqCst);
            true
        } else {
            false
        }
    }
}

/// Clears the running flag when the background task ends, even by a panic.
struct RunGuard(Arc<Shared>);

impl Drop for RunGuard {
    fn drop(&mut self) {
        *lock(&self.0.progress) = Progress::default();
        self.0.running.store(false, Ordering::SeqCst);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    Started,
    AlreadyRunning,
    /// Backfill needs the ORCID iD to find the author's works.
    NoOrcid,
}

/// Starts a background run unless one is already active or the ORCID iD is missing.
pub fn start(app: &AppHandle, sh: &Arc<Shared>, mode: Mode) -> Start {
    if lock(&sh.settings).orcid.is_empty() {
        return Start::NoOrcid;
    }
    if sh
        .running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Start::AlreadyRunning;
    }
    sh.cancel.store(false, Ordering::SeqCst);
    let guard = RunGuard(sh.clone());
    let app = app.clone();
    let sh = sh.clone();
    tauri::async_runtime::spawn(async move {
        let _guard = guard;
        let settings = lock(&sh.settings).clone();
        let lang = settings.language.clone();
        let path = db::db_path_in(&settings.data_path());
        let keys = tauri::async_runtime::spawn_blocking(secrets::load_keys).await.unwrap_or_default();
        let http = match Http::new(keys) {
            Ok(h) => h.with_cancel(sh.cancel.clone()),
            Err(e) => {
                eprintln!("backfill: HTTP client failed: {e:#}");
                return;
            }
        };
        let emitter = app.clone();
        let progress_sh = sh.clone();
        let on_progress = move |p: Progress| {
            *lock(&progress_sh.progress) = p.clone();
            lock(&progress_sh.scan).record(&p);
            if let Err(e) = emitter.emit("backfill-progress", &p) {
                eprintln!("backfill-progress event failed: {e}");
            }
        };
        let mode_name = mode.as_str();
        let began = Instant::now();
        on_progress(Progress {
            running: true,
            event: "preparing".into(),
            mode: mode_name.into(),
            ..Progress::default()
        });
        let stores_citing = !matches!(mode, Mode::Article(_));
        let result = run_articles(&path, mode, &http, &lang, &settings.orcid, &on_progress).await;
        if let Err(e) = &result {
            eprintln!("backfill failed: {}", secrets::redact(&format!("{e:#}"), &[]));
        }
        for kind in http.rejected_keys() {
            if secrets::first_warning(kind) {
                let _ = app.emit("api-key-rejected", kind.as_str());
            }
        }
        if let Ok(Some(sum)) = &result {
            if stores_citing && !sum.cancelled && sum.failed < sum.total {
                let p = path.clone();
                let _ = tauri::async_runtime::spawn_blocking(move || {
                    db::open(&p).and_then(|c| db::set_state(&c, db::STATE_REFRESH, &db::ts(Utc::now())))
                })
                .await;
            }
            if settings.notify && !sum.cancelled {
                notify_new_citing(&app, &lang, sum);
            }
        }
        // The last event: the run is over. The panel keeps its summary until the user closes it.
        let (event, summary, message) = match &result {
            Ok(Some(sum)) => (if sum.cancelled { "cancelled" } else { "finished" }, Some(sum.clone()), String::new()),
            Ok(None) => ("finished", None, String::new()),
            Err(e) => ("failed", None, secrets::redact(&format!("{e:#}"), &[])),
        };
        on_progress(Progress {
            running: false,
            event: event.into(),
            mode: mode_name.into(),
            n: summary.as_ref().map(|s| s.total).unwrap_or(0),
            done: summary.as_ref().map(|s| s.total).unwrap_or(0),
            elapsed_secs: began.elapsed().as_secs(),
            summary,
            message,
            ..Progress::default()
        });
        let _ = app.emit("backfill-updated", result.ok().flatten());
        let _ = app.emit("data-updated", ());
    });
    Start::Started
}

/// Debug aid (the `debug_scan_demo` command, debug builds only): plays a scripted scan with
/// invented articles through the real event path, so the scan panel can be tried without network.
pub fn demo(app: AppHandle, sh: Arc<Shared>) -> bool {
    if !cfg!(debug_assertions) {
        return false;
    }
    if sh.running.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        return false;
    }
    sh.cancel.store(false, Ordering::SeqCst);
    let guard = RunGuard(sh.clone());
    tauri::async_runtime::spawn(async move {
        let _guard = guard;
        let (emitter, psh) = (app.clone(), sh.clone());
        let on = move |p: Progress| {
            *lock(&psh.progress) = p.clone();
            lock(&psh.scan).record(&p);
            let _ = emitter.emit("backfill-progress", &p);
        };
        let names = [
            "Sample paper on invented networks",
            "A study of fictional rivers and their names in a very long title that must be cut off by the panel",
            "Notes on imaginary measurement",
            "Synthetic benchmarks for sample data",
            "Example methods for placeholder corpora",
            "Unmatched sample letter",
            "Demonstration of toy models",
            "Another invented article",
            "Placeholder proceedings paper",
            "Sample review of imaginary methods",
            "Short invented note",
        ];
        let targets: Vec<(String, String)> = names.iter().enumerate().map(|(i, t)| (format!("demo{i}"), t.to_string())).collect();
        let scan = Scan::new(&on, "all", targets.len());
        on(Progress { running: true, event: "preparing".into(), mode: "all".into(), ..Progress::default() });
        scan.start(&targets);
        let pause = |ms: u64| tokio::time::sleep(Duration::from_millis(ms));
        scan.step(Step::Request { source: OPENALEX_LABEL, what: What::Author, page: 1, records: 0 }, "", "");
        pause(700).await;
        scan.step(Step::Request { source: OPENALEX_LABEL, what: What::Works, page: 1, records: 0 }, "", "");
        pause(700).await;
        scan.emit("works", |p| p.works = 14);
        scan.emit("matching", |p| {
            p.works = 14;
            p.records = 9;
        });
        let (mut matched, mut unmatched, mut failed, mut records) = (0usize, 0usize, 0usize, 0usize);
        let mut cancelled = false;
        for (i, (key, title)) in targets.iter().enumerate() {
            if sh.cancel.load(Ordering::SeqCst) {
                cancelled = true;
                break;
            }
            scan.article_start(i + 1, key, title);
            for page in 1..=2usize {
                scan.step(Step::Request { source: OPENALEX_LABEL, what: What::Citing, page, records: (page - 1) * 40 }, key, title);
                scan.step(Step::Request { source: S2_LABEL, what: What::Citing, page, records: (page - 1) * 36 }, key, title);
                pause(450).await;
            }
            if i == 3 {
                for left in (1..=5u64).rev() {
                    if sh.cancel.load(Ordering::SeqCst) {
                        break;
                    }
                    scan.step(Step::Wait { source: S2_LABEL, secs: left, attempt: 1 }, key, title);
                    pause(1000).await;
                }
            }
            if sh.cancel.load(Ordering::SeqCst) {
                cancelled = true;
                break;
            }
            match i {
                5 => {
                    unmatched += 1;
                    scan.article_end("unmatched", key, title, |_| {});
                }
                7 => {
                    failed += 1;
                    scan.article_end("error", key, title, |p| p.message = "HTTP 500".into());
                }
                _ => {
                    matched += 1;
                    records += 40 - i * 2;
                    scan.article_end("article_done", key, title, |p| {
                        p.oa_count = Some((40 - i * 2) as i64);
                        p.s2_count = Some((36 - i * 2) as i64);
                        p.chosen = "openalex".into();
                        p.method = if i % 2 == 0 { "orcid" } else { "title_search" }.into();
                        p.records = 40 - i * 2;
                    });
                }
            }
        }
        let summary = RunSummary {
            total: targets.len(),
            matched,
            unmatched,
            failed,
            records,
            new_total: 3,
            cancelled,
            ..RunSummary::default()
        };
        on(Progress {
            running: false,
            event: if cancelled { "cancelled" } else { "finished" }.into(),
            mode: "all".into(),
            n: targets.len(),
            done: scan.done.load(Ordering::SeqCst),
            elapsed_secs: scan.started.elapsed().as_secs(),
            summary: Some(summary),
            ..Progress::default()
        });
    });
    true
}

/// Called by the scheduler after a successful snapshot: backfills new articles when enabled.
pub fn kick_after_snapshot(app: &AppHandle) {
    let Some(state) = app.try_state::<Arc<Shared>>() else {
        return;
    };
    let sh = state.inner().clone();
    let enabled = {
        let s = lock(&sh.settings);
        s.backfill_enabled && !s.profile_id.is_empty()
    };
    if enabled {
        start(app, &sh, Mode::Missing);
    }
}

/// Marks runs a previous session left open as interrupted.
pub fn recover_stale_runs(db_path: &Path, lang: &str) {
    if !db_path.exists() {
        return;
    }
    let result = db::open(db_path)
        .and_then(|c| db::mark_stale_runs(&c, Utc::now(), i18n::tr(lang, "bf.msg_interrupted")));
    if let Err(e) = result {
        eprintln!("backfill: could not mark stale runs: {e:#}");
    }
}

// --------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(id: &str, title: &str, year: Option<i64>) -> Candidate {
        Candidate {
            id: id.into(),
            doi: None,
            title: title.into(),
            year,
        }
    }

    #[test]
    fn normalization_strips_case_punctuation_diacritics_and_tags() {
        assert_eq!(normalize_title("  Hello,   WORLD!! "), "hello world");
        assert_eq!(normalize_title("Çağdaş Işık: Öğrenme Üzerine"), "cagdas isik ogrenme uzerine");
        assert_eq!(normalize_title("Déjà vu — naïve café"), "deja vu naive cafe");
        assert_eq!(normalize_title("Straße & Œuvre"), "strasse oeuvre");
        assert_eq!(normalize_title("Effects of <i>in situ</i> testing"), "effects of in situ testing");
        assert_eq!(normalize_title("a < b and c > d"), "a b and c d");
        assert_eq!(normalize_title("İstanbul"), "istanbul");
        assert_eq!(normalize_title("3D-printing (2019)"), "3d printing 2019");
        assert_eq!(normalize_title(""), "");
    }

    #[test]
    fn similarity_is_one_for_equal_titles_and_zero_for_unrelated_ones() {
        let a = "Estimating coastal sediment transport by a new framework and toolkit using satellite altimetry";
        assert!((title_similarity(a, a) - 1.0).abs() < 1e-9);
        assert!((title_similarity(a, &a.to_uppercase()) - 1.0).abs() < 1e-9);
        assert_eq!(title_similarity(a, "Deep learning for protein folding"), 0.0);
        assert_eq!(title_similarity("", a), 0.0);
        assert_eq!(title_similarity("", ""), 0.0);
        // punctuation and tags do not matter
        assert!(
            title_similarity(
                "Quillfern: Harmonizing the taxonomic ledger of herbarium and survey records",
                "Quillfern: harmonizing the <i>taxonomic</i> ledger of herbarium and survey records."
            ) > 0.99
        );
    }

    #[test]
    fn near_miss_titles_are_rejected() {
        let scholar = "Estimating coastal sediment transport by a new framework and toolkit using satellite altimetry";
        // a different paper with a very similar title: score stays below the threshold
        let near = "Estimating coastal sediment transport by a new framework using satellite imagery";
        let s = title_similarity(scholar, near);
        assert!(s > 0.7 && s < MIN_SCORE, "score {s}");
        assert!(best_candidate(scholar, Some(2019), &[cand("W1", near, Some(2019))]).is_none());

        // another near miss: same words, extra qualifier
        let a = "Machine learning for landslide susceptibility mapping";
        let b = "Machine learning for landslide susceptibility mapping: a systematic review of the literature";
        assert!(title_similarity(a, b) < MIN_SCORE);
        assert!(best_candidate(a, None, &[cand("W2", b, None)]).is_none());
    }

    #[test]
    fn accepts_minor_variants_and_enforces_the_year_rule() {
        let scholar = "Estimating coastal sediment transport by a new framework and toolkit using satellite altimetry";
        let published = "Estimating coastal sediment transport by a new framework and toolkit using satellite altimetry.";
        let ok = [cand("W1", published, Some(2019))];
        let (c, s) = best_candidate(scholar, Some(2019), &ok).expect("match");
        assert_eq!(c.id, "W1");
        assert!(s > 0.99);
        // within one year (online first vs issue year)
        assert!(best_candidate(scholar, Some(2020), &ok).is_some());
        // two years apart: rejected
        assert!(best_candidate(scholar, Some(2021), &ok).is_none());
        // Scholar year unknown, or candidate year unknown: accepted
        assert!(best_candidate(scholar, None, &ok).is_some());
        assert!(best_candidate(scholar, Some(2010), &[cand("W3", published, None)]).is_some());

        // one dropped word out of 13 still passes (dice 0.96)
        let dropped = "Estimating coastal sediment transport by a new framework and toolkit using satellite";
        assert!(best_candidate(scholar, None, &[cand("W4", dropped, None)]).is_some());
    }

    #[test]
    fn best_candidate_prefers_score_then_closer_year() {
        let t = "A study of citation dynamics in open science";
        let cands = vec![
            cand("W1", "A study of citation dynamics in open science today", Some(2020)),
            cand("W2", t, Some(2021)),
            cand("W3", t, Some(2020)),
        ];
        // W2 and W3 have a perfect score; W3 is closer to the Scholar year
        let (c, _) = best_candidate(t, Some(2020), &cands).expect("m");
        assert_eq!(c.id, "W3");
        // equal score and equal year gap: the earlier one stays
        let both = vec![cand("A", t, Some(2020)), cand("B", t, Some(2020))];
        assert_eq!(best_candidate(t, Some(2020), &both).expect("m").0.id, "A");
        assert!(best_candidate(t, Some(2020), &[]).is_none());
    }

    #[test]
    fn dates_are_parsed_with_their_precision() {
        assert_eq!(parse_date("2019-07-01"), Some(("2019-07-01".into(), "day")));
        assert_eq!(parse_date("2019-07"), Some(("2019-07".into(), "month")));
        assert_eq!(parse_date("2019"), Some(("2019".into(), "year")));
        assert_eq!(parse_date(" 2019-12-31 "), Some(("2019-12-31".into(), "day")));
        assert_eq!(parse_date("2019-13-01"), None);
        assert_eq!(parse_date("2019-02-30"), None);
        assert_eq!(parse_date("19-07-01"), None);
        assert_eq!(parse_date(""), None);
        assert_eq!(parse_date("abcd"), None);
        assert_eq!(dated(Some("bad"), Some(2020)), Some(("2020".into(), "year")));
        assert_eq!(dated(None, None), None);
    }

    const OA_SEARCH: &str = r#"{
      "meta": {"count": 3, "page": 1, "per_page": 5},
      "results": [
        {"id": "https://openalex.org/W2012345678", "doi": "https://doi.org/10.1234/example.2019.101916",
         "title": "Estimating coastal sediment transport by a new framework and toolkit using satellite altimetry",
         "publication_year": 2019, "cited_by_count": 41},
        {"id": "https://openalex.org/W111", "doi": null, "title": "Another paper", "publication_year": null, "cited_by_count": 0},
        {"id": "https://openalex.org/Wnotanid", "doi": null, "title": "Bad id", "publication_year": 2019},
        {"id": "https://openalex.org/W222", "doi": null, "title": null, "publication_year": 2019}
      ]
    }"#;

    #[test]
    fn parses_openalex_search_results() {
        let c = parse_oa_search(OA_SEARCH).expect("parse");
        assert_eq!(c.len(), 2); // the bad id and the untitled work are dropped
        assert_eq!(c[0].id, "W2012345678");
        assert_eq!(c[0].doi.as_deref(), Some("10.1234/example.2019.101916"));
        assert_eq!(c[0].year, Some(2019));
        assert_eq!(c[1].doi, None);
        assert_eq!(c[1].year, None);
        assert!(parse_oa_search("{}").expect("empty").is_empty());
        assert!(parse_oa_search("not json").is_err());
    }

    const OA_PAGE1: &str = r#"{
      "meta": {"count": 3, "per_page": 2, "next_cursor": "IlsxNzI0MDIwODAwMDAwLCAnaHR0cHM6Ly9vcGVuYWxleC5vcmcvVzEnXSI="},
      "results": [
        {"id": "https://openalex.org/W1", "doi": null, "title": "First <i>citing</i>", "publication_date": "2020-03-15", "publication_year": 2020},
        {"id": "https://openalex.org/W2", "doi": null, "title": "Second", "publication_date": "2021-12-01", "publication_year": 2021}
      ]
    }"#;
    const OA_PAGE2: &str = r#"{
      "meta": {"count": 3, "per_page": 2, "next_cursor": null},
      "results": [
        {"id": "https://openalex.org/W3", "title": "Date missing, year known", "publication_date": null, "publication_year": 2022},
        {"id": "https://openalex.org/W4", "title": "No date at all", "publication_date": null, "publication_year": null}
      ]
    }"#;

    #[test]
    fn parses_openalex_citing_pages_with_cursor() {
        let (recs, next) = parse_oa_citing(OA_PAGE1).expect("p1");
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].citing_id, "W1");
        assert_eq!((recs[0].citing_date.as_str(), recs[0].date_precision.as_str()), ("2020-03-15", "day"));
        assert_eq!(next.as_deref(), Some("IlsxNzI0MDIwODAwMDAwLCAnaHR0cHM6Ly9vcGVuYWxleC5vcmcvVzEnXSI="));

        let (recs, next) = parse_oa_citing(OA_PAGE2).expect("p2");
        assert_eq!(next, None);
        assert_eq!(recs.len(), 1); // the undated work is skipped
        assert_eq!(recs[0].citing_id, "W3");
        assert_eq!((recs[0].citing_date.as_str(), recs[0].date_precision.as_str()), ("2022", "year"));

        // a cursor on an empty page ends the loop
        let (recs, next) =
            parse_oa_citing(r#"{"meta":{"next_cursor":"abc"},"results":[]}"#).expect("empty");
        assert!(recs.is_empty());
        assert_eq!(next, None);
    }

    const OA_RICH: &str = r#"{"meta":{"next_cursor":null},"results":[
      {"id":"https://openalex.org/W9","doi":"https://doi.org/10.5555/Invented.2025.1","title":"An invented citing study",
       "publication_date":"2025-04-02","publication_year":2025,
       "primary_location":{"landing_page_url":"https://doi.org/10.5555/invented.2025.1","source":{"display_name":"Journal of Invented Results"}},
       "authorships":[
         {"author":{"display_name":"Ada Example"},"institutions":[{"country_code":"TR"},{"country_code":"US"}]},
         {"author":{"display_name":"Bo Sample"},"institutions":[{"country_code":"TR"}]},
         {"author":{"display_name":"Cy Nobody"},"institutions":[]}
       ]},
      {"id":"https://openalex.org/W10","title":"No location at all","publication_date":"2025-05-01","publication_year":2025,
       "primary_location":null,"authorships":[]}
    ]}"#;

    #[test]
    fn openalex_citing_keeps_venue_doi_authors_and_countries() {
        let (recs, _) = parse_oa_citing(OA_RICH).expect("rich");
        assert_eq!(recs.len(), 2);
        let r = &recs[0];
        assert_eq!(r.venue, "Journal of Invented Results");
        assert_eq!(r.doi, "10.5555/invented.2025.1");
        assert_eq!(r.url, "https://doi.org/10.5555/invented.2025.1");
        assert_eq!(r.authors, "Ada Example; Bo Sample; Cy Nobody");
        assert_eq!(r.countries, "TR,US");
        let bare = &recs[1];
        assert_eq!((bare.venue.as_str(), bare.doi.as_str(), bare.countries.as_str()), ("", "", ""));
    }

    #[test]
    fn semantic_scholar_citing_has_no_countries() {
        let json = r#"{"data":[{"citingPaper":{"paperId":"abc123","title":"Invented S2 work","year":2024,
          "publicationDate":"2024-02-03","venue":"Invented Proceedings","externalIds":{"DOI":"10.5555/S2.1"},
          "authors":[{"name":"Dee Example"},{"name":"Eve Sample"}]}}]}"#;
        let (recs, _) = parse_s2_citations(json).expect("s2");
        let r = &recs[0];
        assert_eq!((r.venue.as_str(), r.doi.as_str(), r.countries.as_str()), ("Invented Proceedings", "10.5555/s2.1", ""));
        assert_eq!(r.authors, "Dee Example; Eve Sample");
        assert_eq!(r.url, "https://www.semanticscholar.org/paper/abc123");
    }

    #[test]
    fn authors_and_countries_are_normalized() {
        let many: Vec<String> = (1..=12).map(|i| format!("Author {i}")).collect();
        let s = join_authors(many);
        assert!(s.starts_with("Author 1; Author 2;") && s.ends_with("Author 10; et al."), "{s}");
        assert_eq!(join_authors(vec![" ".to_string()]), "");
        assert_eq!(join_countries(["us", "TR", "tr", "x", "USA", ""].map(String::from)), "TR,US");
    }

    #[test]
    fn refresh_runs_daily_only_when_enabled_and_matched() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-22T10:00:00Z").expect("t").with_timezone(&Utc);
        let ago = |h: i64| Some(now - chrono::Duration::hours(h));
        assert!(refresh_due(true, None, now, true));
        assert!(refresh_due(true, ago(24), now, true));
        assert!(refresh_due(true, ago(30), now, true));
        assert!(!refresh_due(true, ago(23), now, true));
        assert!(!refresh_due(false, None, now, true));
        assert!(!refresh_due(true, None, now, false));
    }

    #[test]
    fn new_citing_notification_line_names_the_venue() {
        let c = NewCiting {
            article: "Mine".into(),
            title: "An invented citing study".into(),
            venue: "Journal of Invented Results".into(),
            date: "2025-04-02".into(),
        };
        assert_eq!(
            citing_line("tr", &c),
            "Journal of Invented Results dergisinde 'An invented citing study' makalenize atıf yaptı"
        );
        assert_eq!(citing_line("en", &c), "'An invented citing study' in Journal of Invented Results cited your article");
        let nv = NewCiting { venue: " ".into(), ..c.clone() };
        assert_eq!(citing_line("tr", &nv), "'An invented citing study' makalenize atıf yaptı");
        let long = NewCiting { title: "x".repeat(200), ..c };
        assert!(citing_line("en", &long).contains('…'));
    }

    #[test]
    fn new_citing_summary_is_newest_first_and_capped() {
        let mut sum = RunSummary::default();
        let mk = |id: &str, d: &str| CitingRecord {
            citing_id: id.into(),
            citing_title: format!("T{id}"),
            citing_date: d.into(),
            date_precision: "day".into(),
            ..CitingRecord::default()
        };
        note_new(&mut sum, "A", vec![mk("1", "2025-01-01"), mk("2", "2025-03-01")]);
        note_new(&mut sum, "B", (3..12).map(|i| mk(&i.to_string(), "2024-01-01")).collect());
        assert_eq!(sum.new_total, 11);
        assert_eq!(sum.new_citing.len(), KEEP_NEW);
        assert_eq!(sum.new_citing[0].date, "2025-03-01");
        assert_eq!(sum.new_citing[0].article, "A");
    }

    const S2_MATCH: &str = r#"{"data":[{"paperId":"0123456789abcdef0123456789abcdef01234567",
        "title":"Quillfern: Harmonizing the taxonomic ledger of herbarium and survey records",
        "year":2025,"externalIds":{"DOI":"10.1234/example.2025.102000","CorpusId":12345}}]}"#;

    #[test]
    fn parses_semantic_scholar_match() {
        let c = parse_s2_match(S2_MATCH).expect("parse");
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].id, "0123456789abcdef0123456789abcdef01234567");
        assert_eq!(c[0].doi.as_deref(), Some("10.1234/example.2025.102000"));
        assert_eq!(c[0].year, Some(2025));
        // the real "not found" answer is an HTTP 404 (handled as None); an empty list also works
        assert!(parse_s2_match(r#"{"data":[]}"#).expect("empty").is_empty());
        assert!(parse_s2_match(r#"{"error":"Title match not found"}"#).expect("err body").is_empty());
        // an external id that is not a plain number/string does not break parsing
        assert!(parse_s2_match(r#"{"data":[{"paperId":"abc","title":"T","year":null,"externalIds":null}]}"#)
            .expect("null ids")
            .first()
            .is_some_and(|c| c.doi.is_none()));
    }

    const S2_CITES1: &str = r#"{"offset":0,"next":2,"data":[
        {"citingPaper":{"paperId":"aaa111","title":"Has a full date","year":2023,"publicationDate":"2023-05-17","externalIds":{"DOI":"10.1/a"}}},
        {"citingPaper":{"paperId":"bbb222","title":"Date missing, year only","year":2024,"publicationDate":null,"externalIds":{}}}
      ]}"#;
    const S2_CITES2: &str = r#"{"offset":2,"data":[
        {"citingPaper":{"paperId":"ccc333","title":"No date and no year","year":null,"publicationDate":null}},
        {"citingPaper":{"paperId":null,"title":"No id","year":2020,"publicationDate":"2020-01-01"}},
        {"citingPaper":null},
        {"citingPaper":{"paperId":"ddd444","title":"Month only","year":2021,"publicationDate":"2021-08"}}
      ]}"#;

    #[test]
    fn parses_semantic_scholar_citations_with_missing_dates() {
        let (recs, next) = parse_s2_citations(S2_CITES1).expect("p1");
        assert_eq!(next, Some(2));
        assert_eq!(recs.len(), 2);
        assert_eq!((recs[0].citing_date.as_str(), recs[0].date_precision.as_str()), ("2023-05-17", "day"));
        // publicationDate missing: the year is used
        assert_eq!(recs[1].citing_id, "bbb222");
        assert_eq!((recs[1].citing_date.as_str(), recs[1].date_precision.as_str()), ("2024", "year"));

        let (recs, next) = parse_s2_citations(S2_CITES2).expect("p2");
        assert_eq!(next, None);
        // undated, id-less and null entries are skipped; the month-precision one stays
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].citing_id, "ddd444");
        assert_eq!(recs[0].date_precision, "month");
        assert!(parse_s2_citations("not json").is_err());
    }

    #[test]
    fn dedupe_keeps_the_first_of_each_id() {
        let mk = |id: &str, d: &str| CitingRecord {
            citing_id: id.into(),
            citing_title: String::new(),
            citing_date: d.into(),
            date_precision: "day".into(),
            ..CitingRecord::default()
        };
        let out = dedupe(vec![mk("a", "2020-01-01"), mk("b", "2020-01-02"), mk("a", "2021-01-01")]);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].citing_date, "2020-01-01");
    }

    #[test]
    fn source_choice_prefers_more_records_and_openalex_on_a_tie() {
        assert_eq!(choose_source(Some(40), Some(36)), Some(Source::OpenAlex));
        assert_eq!(choose_source(Some(36), Some(40)), Some(Source::SemanticScholar));
        // tie goes to OpenAlex, also at zero
        assert_eq!(choose_source(Some(12), Some(12)), Some(Source::OpenAlex));
        assert_eq!(choose_source(Some(0), Some(0)), Some(Source::OpenAlex));
        // a single matching source is used even with fewer (or no) records
        assert_eq!(choose_source(Some(0), None), Some(Source::OpenAlex));
        assert_eq!(choose_source(None, Some(0)), Some(Source::SemanticScholar));
        assert_eq!(choose_source(None, None), None);
    }

    fn rec(id: &str, date: &str, p: &str) -> CitingRecord {
        CitingRecord {
            citing_id: id.into(),
            citing_title: String::new(),
            citing_date: date.into(),
            date_precision: p.into(),
            ..CitingRecord::default()
        }
    }

    fn outcome(id: &str, n: usize) -> SourceOutcome {
        SourceOutcome {
            work: Some(Work {
                id: id.into(),
                doi: Some(format!("10.1/{id}")),
                score: 0.95,
                method: MatchMethod::Orcid,
            }),
            citing: Some((0..n).map(|i| rec(&format!("{id}-{i}"), "2024-01-01", "day")).collect()),
            error: None,
        }
    }

    #[test]
    fn decide_stores_the_larger_source_and_both_counts() {
        match decide("k1", outcome("W1", 40), outcome("s2id", 36)) {
            Decision::Matched { rec, citing, other_error } => {
                assert_eq!(rec.source.as_deref(), Some("openalex"));
                assert_eq!(rec.external_id.as_deref(), Some("W1"));
                assert_eq!(rec.doi.as_deref(), Some("10.1/W1"));
                assert_eq!((rec.openalex_count, rec.s2_count), (Some(40), Some(36)));
                assert_eq!((rec.openalex_id.as_deref(), rec.s2_id.as_deref()), (Some("W1"), Some("s2id")));
                assert_eq!(citing.len(), 40);
                assert!(other_error.is_none());
            }
            other => panic!("unexpected {other:?}"),
        }
        match decide("k1", outcome("W1", 5), outcome("s2id", 9)) {
            Decision::Matched { rec, citing, .. } => {
                assert_eq!(rec.source.as_deref(), Some("semanticscholar"));
                assert_eq!(rec.external_id.as_deref(), Some("s2id"));
                assert_eq!(citing.len(), 9);
                assert!(citing.iter().all(|c| c.citing_id.starts_with("s2id")));
            }
            other => panic!("unexpected {other:?}"),
        }
        // tie: OpenAlex
        match decide("k1", outcome("W1", 7), outcome("s2id", 7)) {
            Decision::Matched { rec, .. } => assert_eq!(rec.source.as_deref(), Some("openalex")),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn decide_handles_one_sided_matches_and_failures() {
        // only Semantic Scholar matched
        match decide("k1", SourceOutcome::default(), outcome("s2id", 3)) {
            Decision::Matched { rec, .. } => {
                assert_eq!(rec.source.as_deref(), Some("semanticscholar"));
                assert_eq!((rec.openalex_count, rec.s2_count), (None, Some(3)));
                assert_eq!(rec.openalex_id, None);
            }
            other => panic!("unexpected {other:?}"),
        }
        // Semantic Scholar rate-limited, OpenAlex fine: OpenAlex is used, the gap is reported
        let limited = SourceOutcome {
            work: Some(Work { id: "s2id".into(), doi: None, score: 0.9, method: MatchMethod::TitleSearch }),
            citing: None,
            error: Some("429".into()),
        };
        match decide("k1", outcome("W1", 2), limited.clone()) {
            Decision::Matched { rec, other_error, .. } => {
                assert_eq!(rec.source.as_deref(), Some("openalex"));
                assert_eq!(rec.s2_count, None);
                assert_eq!(rec.s2_id.as_deref(), Some("s2id"));
                assert_eq!(other_error.as_deref(), Some("429"));
            }
            other => panic!("unexpected {other:?}"),
        }
        // nothing usable and a source failed: retry later, store nothing
        assert_eq!(
            decide("k1", SourceOutcome::default(), limited),
            Decision::Failed("429".into())
        );
        // both answered, neither matched
        assert_eq!(decide("k1", SourceOutcome::default(), SourceOutcome::default()), Decision::Unmatched);
        // a matched source with zero citing works is still a match
        match decide("k1", outcome("W1", 0), SourceOutcome::default()) {
            Decision::Matched { rec, citing, .. } => {
                assert_eq!(rec.openalex_count, Some(0));
                assert!(citing.is_empty());
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    const OA_AUTHOR: &str = r#"{"id":"https://openalex.org/A5012345678","works_count":19}"#;

    #[test]
    fn parses_author_and_works_pages() {
        let (id, n) = parse_oa_author(OA_AUTHOR).expect("author");
        assert_eq!((id.as_str(), n), ("A5012345678", Some(19)));
        assert!(parse_oa_author(r#"{"id":"https://openalex.org/W1"}"#).is_err());
        assert!(parse_oa_author("{}").is_err());

        let (cands, next) = parse_oa_works(OA_SEARCH).expect("works");
        assert_eq!(cands.len(), 2);
        assert_eq!(next, None);
        let (cands, next) = parse_oa_works(OA_PAGE1).expect("works with cursor");
        assert_eq!(cands.len(), 2);
        assert!(next.is_some());
        // union by work id keeps the first occurrence
        let a = vec![cand("W1", "One", None), cand("W2", "Two", None)];
        let b = vec![cand("W2", "Two again", None), cand("W3", "Three", None)];
        let union = dedupe_candidates([a, b].concat());
        assert_eq!(union.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["W1", "W2", "W3"]);
        assert_eq!(union[1].title, "Two");
    }

    #[test]
    fn parses_a_semantic_scholar_paper_by_doi() {
        let c = parse_s2_paper(
            r#"{"paperId":"0123456789abcdef0123456789abcdef01234567","title":"T","year":2019,"externalIds":{"DOI":"10.1/x"}}"#,
        )
        .expect("parse")
        .expect("some");
        assert_eq!(c.doi.as_deref(), Some("10.1/x"));
        assert_eq!(c.year, Some(2019));
        assert!(parse_s2_paper(r#"{"paperId":null}"#).expect("null id").is_none());
        assert!(parse_s2_paper("nope").is_err());
        assert_eq!(encode_path("10.1234/example.2019.101916"), "10.1234/example.2019.101916");
        assert_eq!(encode_path("10.1002/(SICI)1097<x>#1"), "10.1002/(SICI)1097%3Cx%3E%231");
    }

    const QUILL_V1: &str = "Quillfern: Harmonizing the taxonomic ledger of herbarium and survey records";
    const QUILL_V2: &str = "Version 2 - Quillfern: harmonizing the taxonomic ledger of herbarium and survey records";

    fn arts(list: &[(&str, Option<i64>)]) -> Vec<(String, Option<i64>)> {
        list.iter().map(|(t, y)| (t.to_string(), *y)).collect()
    }

    #[test]
    fn orcid_set_keeps_near_duplicate_versions_apart() {
        // the real author set holds both the 2025 article and its 2026 "Version 2" record
        let cands = vec![
            cand("W-v2", QUILL_V2, Some(2026)),
            cand("W-v1", QUILL_V1, Some(2025)),
            cand("W-other", "Estimating coastal sediment transport by a new framework and toolkit using satellite altimetry", Some(2019)),
        ];
        let none = HashSet::new();

        // the pair is the dangerous kind: similar enough to pass the author-set threshold (0.80)
        // and even the global one (0.85), so only the one-to-one rule and the year keep them apart
        let sim = title_similarity(QUILL_V1, QUILL_V2);
        assert!((sim - 20.0 / 22.0).abs() < 1e-9 && sim > MIN_SCORE && sim > ORCID_MIN_SCORE, "{sim}");

        // Scholar lists the article once: it must get the 2025 record, never the 2026 one
        let one = assign_candidates(&arts(&[(QUILL_V1, Some(2025))]), &cands, &none);
        assert_eq!(one[0].expect("assigned").cand, 1);
        assert!((one[0].expect("assigned").score - 1.0).abs() < 1e-9);

        // Scholar lists both versions: each gets its own record, in either input order
        let both = assign_candidates(&arts(&[(QUILL_V2, Some(2026)), (QUILL_V1, Some(2025))]), &cands, &none);
        assert_eq!((both[0].expect("v2").cand, both[1].expect("v1").cand), (0, 1));
        let swapped = assign_candidates(&arts(&[(QUILL_V1, Some(2025)), (QUILL_V2, Some(2026))]), &cands, &none);
        assert_eq!((swapped[0].expect("v1").cand, swapped[1].expect("v2").cand), (1, 0));

        // each candidate is used at most once: a second identical Scholar entry stays unassigned
        // (or takes the weaker near-duplicate, but never the same record twice)
        let dup = assign_candidates(&arts(&[(QUILL_V1, Some(2025)), (QUILL_V1, Some(2025))]), &cands, &none);
        assert_eq!(dup[0].expect("first").cand, 1);
        assert_ne!(dup[1].map(|a| a.cand), Some(1));

        // identical titles on both records: the year breaks the tie
        let same = vec![cand("W-new", QUILL_V1, Some(2026)), cand("W-old", QUILL_V1, Some(2025))];
        let r = assign_candidates(&arts(&[(QUILL_V1, Some(2025))]), &same, &none);
        assert_eq!(r[0].expect("assigned").cand, 1);
        let r = assign_candidates(&arts(&[(QUILL_V1, Some(2026))]), &same, &none);
        assert_eq!(r[0].expect("assigned").cand, 0);

        // a candidate that is already taken (earlier run) is skipped
        let taken: HashSet<String> = ["W-v1".to_string()].into();
        let r = assign_candidates(&arts(&[(QUILL_V1, Some(2025))]), &cands, &taken);
        assert_ne!(r[0].map(|a| a.cand), Some(1));
    }

    #[test]
    fn orcid_set_uses_the_looser_threshold_but_still_checks_the_year() {
        let scholar = "Estimating coastal sediment transport by a new framework and toolkit using satellite altimetry";
        // dice 0.833: below the global-search threshold (0.85), accepted inside the author set (0.80)
        let near = "Estimating coastal sediment transport by a new framework using satellite imagery";
        let cands = vec![cand("W1", near, Some(2019))];
        let none = HashSet::new();
        let sim = title_similarity(scholar, near);
        assert!((sim - 20.0 / 24.0).abs() < 1e-9 && (ORCID_MIN_SCORE..MIN_SCORE).contains(&sim), "{sim}");
        assert!(best_candidate(scholar, Some(2019), &cands).is_none());
        let r = assign_candidates(&arts(&[(scholar, Some(2019))]), &cands, &none);
        assert!(r[0].is_some());
        // year more than one apart: rejected
        assert!(assign_candidates(&arts(&[(scholar, Some(2022))]), &cands, &none)[0].is_none());
        // Scholar year unknown: accepted
        assert!(assign_candidates(&arts(&[(scholar, None)]), &cands, &none)[0].is_some());
        // an unrelated work is not taken
        let far = vec![cand("W2", "Deep learning for protein folding", Some(2019))];
        assert!(assign_candidates(&arts(&[(scholar, Some(2019))]), &far, &none)[0].is_none());
        // empty candidate list
        assert_eq!(assign_candidates(&arts(&[(scholar, Some(2019))]), &[], &none), vec![None]);
    }

    #[test]
    fn doi_forms_are_normalized() {
        for input in [
            "10.1234/Example.2019.101916",
            "  10.1234/example.2019.101916 ",
            "doi:10.1234/example.2019.101916",
            "DOI: 10.1234/Example.2019.101916",
            "https://doi.org/10.1234/example.2019.101916",
            "http://dx.doi.org/10.1234/EXAMPLE.2019.101916",
            "https://www.doi.org/10.1234/example.2019.101916/",
            "doi.org/10.1234/example.2019.101916",
            "https://doi.org/10.1234/example.2019.101916?utm_source=x#frag",
            "https://doi.org/10.1234%2Fexample.2019.101916",
        ] {
            assert_eq!(normalize_doi(input).as_deref(), Some("10.1234/example.2019.101916"), "{input}");
        }
        // parentheses and sub-registrants are legitimate
        assert_eq!(normalize_doi("10.1002/(SICI)1097-4571").as_deref(), Some("10.1002/(sici)1097-4571"));
        assert_eq!(normalize_doi("10.1000.10/abc").as_deref(), Some("10.1000.10/abc"));
        // a query is only cut from URL forms; in a bare DOI it is part of the suffix
        assert_eq!(normalize_doi("10.1234/a?b").as_deref(), Some("10.1234/a?b"));
    }

    #[test]
    fn bad_dois_are_rejected() {
        for input in [
            "",
            "   ",
            "doi:",
            "10.1234",
            "10.1234/",
            "10.12/abc",           // registrant too short
            "10.1234567890/abc",   // registrant too long
            "10.12a4/abc",
            "10./abc",
            "10..5/abc",
            "11.1234/abc",
            "https://example.org/10.1234/abc",
            "10.1234/ab cd",
            "10.1234/ab\tcd",
            "see 10.1234/abc",
            "https://doi.org/",
            "10.1234/%ZZ",
            "10.1234/%FF%FE",      // invalid UTF-8 after decoding
        ] {
            assert_eq!(normalize_doi(input), None, "{input:?}");
        }
        assert_eq!(normalize_doi(&format!("10.1234/{}", "a".repeat(300))), None);
    }

    fn target(key: &str, title: &str, year: i64) -> db::BackfillTarget {
        db::BackfillTarget {
            key: key.into(),
            title: title.into(),
            year: Some(year),
        }
    }

    fn ov(doi: Option<&str>, rejected: &[&str]) -> db::Override {
        db::Override {
            doi: doi.map(str::to_string),
            rejected: rejected.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn manual_doi_beats_title_matching() {
        use std::collections::HashMap;
        let mut cands = vec![cand("W-v1", QUILL_V1, Some(2025)), cand("W-v2", QUILL_V2, Some(2026))];
        cands[0].doi = Some("10.1234/example.2025.1".into());
        let targets = vec![target("a", QUILL_V1, 2025)];
        let none = HashSet::new();

        // no override: the perfect title match is assigned
        let r = plan_assignments(&targets, &HashMap::new(), &cands, &none);
        assert_eq!(r[0].map(|a| a.cand), Some(0));

        // a manual DOI wins: the article takes no part in the title assignment at all,
        // even though a perfect title match exists
        let mut o = HashMap::new();
        o.insert("a".to_string(), ov(Some("10.1234/example.2025.99"), &[]));
        assert_eq!(plan_for(o.get("a")), Plan::Manual("10.1234/example.2025.99".into()));
        let r = plan_assignments(&targets, &o, &cands, &none);
        assert_eq!(r, vec![None]);

        // overrides without a DOI do not switch to the manual plan
        assert_eq!(plan_for(Some(&ov(None, &["W-v1"]))), Plan::Automatic);
        assert_eq!(plan_for(None), Plan::Automatic);
    }

    #[test]
    fn other_articles_cannot_take_a_work_claimed_by_a_manual_doi() {
        use std::collections::HashMap;
        let mut cands = vec![cand("W-v1", QUILL_V1, Some(2025))];
        cands[0].doi = Some("10.1234/example.2025.1".into());
        // article "b" has the DOI of that work set by hand; article "a" would match it by title
        let targets = vec![target("a", QUILL_V1, 2025), target("b", "Something else entirely", 2025)];
        let mut o = HashMap::new();
        o.insert("b".to_string(), ov(Some("10.1234/example.2025.1"), &[]));
        let r = plan_assignments(&targets, &o, &cands, &HashSet::new());
        assert_eq!(r, vec![None, None]);
    }

    #[test]
    fn rejected_works_are_excluded_for_that_article_only() {
        use std::collections::HashMap;
        let cands = vec![
            cand("W-v1", QUILL_V1, Some(2025)),
            cand("W-v2", QUILL_V2, Some(2026)),
        ];
        let targets = [target("a", QUILL_V1, 2025), target("b", QUILL_V1, 2025)];
        let mut o = HashMap::new();
        o.insert("a".to_string(), ov(None, &["W-v1"]));
        let r = plan_assignments(&targets[..1], &o, &cands, &HashSet::new());
        // the 2025 record is ruled out for "a"; the 2026 record is one year away and similar enough
        assert_eq!(r[0].map(|x| x.cand), Some(1));
        // "b" has no override and still gets the exact record
        let r = plan_assignments(&targets[1..], &o, &cands, &HashSet::new());
        assert_eq!(r[0].map(|x| x.cand), Some(0));
        // everything ruled out: unassigned
        o.insert("a".to_string(), ov(None, &["W-v1", "W-v2"]));
        let r = plan_assignments(&targets[..1], &o, &cands, &HashSet::new());
        assert_eq!(r, vec![None]);
    }

    #[test]
    fn rejected_works_are_excluded_from_title_search_candidates() {
        let cands = vec![cand("W1", QUILL_V1, Some(2025)), cand("W2", QUILL_V1, Some(2025))];
        let none = HashSet::new();
        assert_eq!(best_candidate_excluding(QUILL_V1, Some(2025), &cands, &none).expect("m").0.id, "W1");
        let rej: HashSet<String> = ["W1".to_string()].into();
        assert_eq!(best_candidate_excluding(QUILL_V1, Some(2025), &cands, &rej).expect("m").0.id, "W2");
        let all: HashSet<String> = ["W1".to_string(), "W2".to_string()].into();
        assert!(best_candidate_excluding(QUILL_V1, Some(2025), &cands, &all).is_none());
    }

    #[test]
    fn parses_a_single_openalex_work() {
        let c = parse_oa_work(
            r#"{"id":"https://openalex.org/W2012345678","doi":"https://doi.org/10.1234/example.2019.101916","title":"T","publication_year":2019}"#,
        )
        .expect("parse")
        .expect("some");
        assert_eq!((c.id.as_str(), c.doi.as_deref(), c.year), ("W2012345678", Some("10.1234/example.2019.101916"), Some(2019)));
        assert!(parse_oa_work(r#"{"id":"https://openalex.org/Wxyz"}"#).expect("bad id").is_none());
        assert!(parse_oa_work("{}").expect("no id").is_none());
        assert!(parse_oa_work("nope").is_err());
        assert_eq!(MatchMethod::ManualDoi.as_str(), "manual_doi");
    }

    #[test]
    fn match_method_is_stored_for_the_chosen_source() {
        let orcid_work = outcome("W1", 2); // method Orcid
        let mut title_work = outcome("s2id", 9);
        if let Some(w) = title_work.work.as_mut() {
            w.method = MatchMethod::TitleSearch;
        }
        match decide("k", orcid_work.clone(), title_work.clone()) {
            Decision::Matched { rec, .. } => {
                assert_eq!(rec.source.as_deref(), Some("semanticscholar"));
                assert_eq!(rec.match_method.as_deref(), Some("title_search"));
            }
            other => panic!("unexpected {other:?}"),
        }
        match decide("k", outcome("W1", 9), title_work) {
            Decision::Matched { rec, .. } => {
                assert_eq!(rec.source.as_deref(), Some("openalex"));
                assert_eq!(rec.match_method.as_deref(), Some("orcid"));
            }
            other => panic!("unexpected {other:?}"),
        }
        match decide("k", SourceOutcome::default(), SourceOutcome::default()) {
            Decision::Unmatched => {}
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn user_agent_has_no_personal_data() {
        assert!(USER_AGENT.starts_with("scholar-citation-tracker/"));
        assert!(USER_AGENT.contains("github.com/bcankara/scholar-citation-tracker"));
        assert!(!USER_AGENT.contains('@'));
    }

    #[tokio::test(start_paused = true)]
    async fn pacer_spaces_calls() {
        let p = Pacer::new(Duration::from_secs(3));
        let start = tokio::time::Instant::now();
        p.wait().await; // first call is immediate
        p.wait().await;
        p.wait().await;
        assert!(start.elapsed() >= Duration::from_secs(6));
    }

    // ------------------------------------------------------- scan progress and cancelling

    #[test]
    fn eta_is_extrapolated_from_finished_articles() {
        assert_eq!(eta_secs(0, 11, 30), None, "nothing finished yet");
        assert_eq!(eta_secs(1, 11, 10), Some(100));
        assert_eq!(eta_secs(5, 10, 50), Some(50));
        assert_eq!(eta_secs(10, 10, 99), Some(0));
        assert_eq!(eta_secs(12, 10, 99), Some(0), "never negative");
    }

    #[test]
    fn scan_events_carry_counters_elapsed_and_eta() {
        let seen: Mutex<Vec<Progress>> = Mutex::new(Vec::new());
        let on = |p: Progress| lock(&seen).push(p);
        let scan = Scan::new(&on, "all", 3);
        scan.start(&[("a".into(), "A".into()), ("b".into(), "B".into()), ("c".into(), "C".into())]);
        scan.article_start(1, "a", "A");
        scan.step(Step::Request { source: "OpenAlex", what: What::Citing, page: 2, records: 200 }, "a", "A");
        scan.step(Step::Wait { source: "Semantic Scholar", secs: 12, attempt: 1 }, "a", "A");
        scan.article_end("article_done", "a", "A", |p| p.records = 7);
        scan.article_start(2, "b", "B");
        scan.article_end("unmatched", "b", "B", |_| {});
        let ev = lock(&seen).clone();
        let names: Vec<&str> = ev.iter().map(|p| p.event.as_str()).collect();
        assert_eq!(names, ["start", "article_start", "request", "backoff", "article_done", "article_start", "unmatched"]);
        assert_eq!(ev[0].targets.len(), 3);
        assert!(ev.iter().all(|p| p.running && p.mode == "all" && p.n == 3));
        assert_eq!((ev[2].what.as_str(), ev[2].page, ev[2].records, ev[2].k), ("citing", 2, 200, 1));
        assert_eq!((ev[3].wait_secs, ev[3].source.as_str()), (12, "Semantic Scholar"));
        assert_eq!((ev[1].done, ev[4].done, ev[6].done), (0, 1, 2), "done counts finished articles");
        assert_eq!(ev[1].eta_secs, None, "no ETA before the first article is done");
        assert!(ev[6].eta_secs.is_some());
    }

    #[test]
    fn scan_state_collapses_transient_events_and_resets_on_start() {
        let mut st = ScanState::default();
        let ev = |event: &str, source: &str| Progress { event: event.into(), source: source.into(), ..Progress::default() };
        st.record(&ev("start", ""));
        st.record(&ev("article_start", ""));
        for _ in 0..50 {
            st.record(&ev("request", "OpenAlex"));
        }
        st.record(&ev("backoff", "Semantic Scholar"));
        assert_eq!(st.events.len(), 2, "requests are not kept in the history");
        assert_eq!(st.live.len(), 2, "newest per source");
        st.record(&ev("article_done", ""));
        assert!(st.live.is_empty() && st.events.len() == 3);
        st.record(&ev("start", ""));
        assert_eq!(st.events.len(), 1, "a new run starts clean");
    }

    #[tokio::test(start_paused = true)]
    async fn the_backoff_countdown_stops_when_cancelled() {
        let http = Http::new(Keys::default()).expect("http");
        let flag = Arc::new(AtomicBool::new(false));
        let http = Arc::new(http.with_cancel(flag.clone()));
        let seen: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::new()));
        let (h2, s2) = (http.clone(), seen.clone());
        let task = tokio::spawn(async move {
            let say = |step: Step<'_>| {
                if let Step::Wait { secs, .. } = step {
                    lock(&s2).push(secs);
                }
            };
            h2.wait_out(Host::Semantic, 60, 1, &say).await
        });
        tokio::time::sleep(Duration::from_millis(3500)).await;
        flag.store(true, Ordering::SeqCst);
        let r = task.await.expect("join");
        assert_eq!(r, Err(ApiError::Cancelled));
        let secs = lock(&seen).clone();
        assert_eq!(&secs[..3], &[60, 59, 58], "counts down once per second");
        assert!(secs.len() <= 5, "stopped long before the 60 s were over: {secs:?}");
    }

    #[tokio::test]
    async fn a_cancelled_client_sends_no_request() {
        let http = Http::new(Keys::default()).expect("http").with_cancel(Arc::new(AtomicBool::new(true)));
        let say = |_: Step<'_>| panic!("no request may be announced");
        let r = http
            .get(Host::OpenAlex, "http://127.0.0.1:9/never", &[], Req { say: &say, what: What::Citing, page: 1, records: 0 })
            .await;
        assert_eq!(r, Err(ApiError::Cancelled));
    }

    #[tokio::test]
    async fn a_cancelled_run_is_recorded_and_stores_nothing() {
        let dir = std::env::temp_dir().join(format!("scholartakip_cancel_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = db::db_path_in(&dir);
        {
            let mut conn = db::open(&path).expect("open");
            let art = |i: usize| crate::scholar::Article {
                key: format!("k{i}"),
                title: format!("Invented article {i}"),
                authors: String::new(),
                venue: String::new(),
                year: Some(2020),
                citations: 5,
                url: String::new(),
                cites_ids: String::new(),
            };
            let data = crate::scholar::ProfileData {
                name: "N".into(),
                stats: crate::scholar::Stats { since_year: 2021, ..crate::scholar::Stats::default() },
                articles: (0..3).map(art).collect(),
                yearly: vec![],
            };
            db::insert_snapshot(&mut conn, "P", Utc::now(), &data).expect("snapshot");
        }
        let http = Http::new(Keys::default()).expect("http").with_cancel(Arc::new(AtomicBool::new(true)));
        let seen: Mutex<Vec<Progress>> = Mutex::new(Vec::new());
        let on = |p: Progress| lock(&seen).push(p);
        let sum = run_articles(&path, Mode::All, &http, "en", "", &on).await.expect("run").expect("some");
        assert!(sum.cancelled);
        assert_eq!((sum.matched, sum.unmatched, sum.failed, sum.total), (0, 0, 0, 3));
        let conn = db::open(&path).expect("open");
        let run = db::backfill_summary(&conn).expect("summary").last_run.expect("run row");
        assert_eq!(run.status, "cancelled");
        assert!(run.message.contains("0 of 3"), "{}", run.message);
        let ev = lock(&seen).clone();
        assert_eq!(ev[0].event, "start");
        assert_eq!(ev[0].targets.len(), 3);
        assert!(ev.iter().all(|p| p.event != "article_done" && p.event != "error"));
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn issn_normalization() {
        assert_eq!(normalize_issn("1569-8432").as_deref(), Some("15698432"));
        assert_eq!(normalize_issn("1872826x").as_deref(), Some("1872826X"));
        assert_eq!(normalize_issn(" 0011 2233 ").as_deref(), Some("00112233"));
        assert_eq!(normalize_issn("1234567"), None);
        assert_eq!(normalize_issn("12345678X"), None);
        assert_eq!(normalize_issn("ABCD-EFGH"), None);
        assert_eq!(join_issns(["1569-8432".to_string(), "15698432".into(), "bad".into(), "1872-826x".into()]), "15698432,1872826X");
    }

    #[test]
    fn issns_are_read_from_both_sources() {
        let oa = r#"{"results":[{"id":"https://openalex.org/W1","title":"T","publication_date":"2024-05-06","publication_year":2024,
            "primary_location":{"landing_page_url":"https://x","source":{"display_name":"Journal X","issn_l":"1569-8432","issn":["1569-8432","1872-826x"]}},"authorships":[]},
            {"id":"https://openalex.org/W2","title":"U","publication_date":"2024-05-07","publication_year":2024,
            "primary_location":{"source":{"display_name":"No ISSN"}},"authorships":[]}],"meta":{"next_cursor":null}}"#;
        let (recs, _) = parse_oa_citing(oa).expect("oa");
        assert_eq!(recs[0].issn_list, "15698432,1872826X");
        assert_eq!(recs[1].issn_list, "");
        let s2 = r#"{"next":null,"data":[{"citingPaper":{"paperId":"abc123","title":"T","year":2024,"publicationDate":"2024-05-06",
            "publicationVenue":{"issn":"1234-5678"}}},{"citingPaper":{"paperId":"def456","title":"U","year":2024,"publicationVenue":null}}]}"#;
        let (recs, _) = parse_s2_citations(s2).expect("s2");
        assert_eq!(recs[0].issn_list, "12345678");
        assert_eq!(recs[1].issn_list, "");
    }

    /// Live check against OpenAlex and Semantic Scholar (never Google):
    /// `BACKFILL_ORCID=... BACKFILL_TITLES="Title one|Title two" cargo test live_backfill -- --ignored --nocapture`
    /// Takes a few minutes (Semantic Scholar is paced at about 3 s per call).
    #[tokio::test]
    #[ignore]
    async fn live_backfill() {
        let (Ok(titles), Ok(orcid)) = (std::env::var("BACKFILL_TITLES"), std::env::var("BACKFILL_ORCID")) else {
            eprintln!("set BACKFILL_ORCID and BACKFILL_TITLES (titles separated by '|') to run this test");
            return;
        };
        let dir = std::env::temp_dir().join(format!("scholartakip_live_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = db::db_path_in(&dir);
        {
            let mut conn = db::open(&path).expect("open");
            let articles: Vec<crate::scholar::Article> = titles
                .split('|')
                .enumerate()
                .map(|(i, t)| crate::scholar::Article {
                    key: format!("live{i}"),
                    title: t.trim().to_string(),
                    authors: String::new(),
                    venue: String::new(),
                    year: None,
                    citations: 10 - i as i64,
                    url: String::new(),
                    cites_ids: String::new(),
                })
                .collect();
            let data = crate::scholar::ProfileData {
                name: "Live".into(),
                stats: crate::scholar::Stats {
                    cit_all: 0,
                    cit_since: 0,
                    h_all: 0,
                    h_since: 0,
                    i10_all: 0,
                    i10_since: 0,
                    since_year: 2021,
                },
                articles,
                yearly: vec![],
            };
            db::insert_snapshot(&mut conn, "LIVE", Utc::now(), &data).expect("snapshot");
        }
        let http = Http::new(Keys::default()).expect("http");
        let log = |p: Progress| eprintln!("  [{}/{}] {}", p.k, p.n, p.source);
        let sum = run_articles(&path, Mode::All, &http, "en", &orcid, &log)
            .await
            .expect("run")
            .expect("some");
        eprintln!("summary: {sum:?}");
        let conn = db::open(&path).expect("open");
        for i in 0..sum.total {
            let a = db::article_backfill(&conn, &format!("live{i}")).expect("ab");
            eprintln!("live{i}: {:?} total={} year_only={}", a.match_info, a.total, a.year_only);
        }
        for g in ["day", "month", "year"] {
            let s = db::history_series(&conn, g).expect("series");
            eprintln!("{g}: {} periods, total {}, excluded {}", s.points.len(), s.total, s.excluded);
        }
        let run = db::backfill_summary(&conn).expect("summary").last_run.expect("run row");
        eprintln!("run: {run:?}");

        // a second pass is idempotent: no duplicate rows
        let before = db::backfill_summary(&conn).expect("s").records;
        drop(conn);
        run_articles(&path, Mode::All, &http, "en", &orcid, &log).await.expect("rerun");
        let conn = db::open(&path).expect("open");
        let after = db::backfill_summary(&conn).expect("s").records;
        eprintln!("records before/after re-run: {before}/{after}");
        assert_eq!(before, after);
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
