//! Google Scholar profile fetching and HTML parsing.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::LazyLock;
use std::time::Duration;

use scraper::{ElementRef, Html, Selector};
use serde::Serialize;

use crate::i18n;

pub const BASE_URL: &str = "https://scholar.google.com";
pub const PAGE_SIZE: usize = 100;
pub const MAX_PAGES: usize = 10;

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
(KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36";

/// Detail of a failure: either a translatable message key (+ argument) or raw text.
#[derive(Debug, Clone, PartialEq)]
pub struct Detail {
    key: &'static str,
    arg: String,
}

impl Detail {
    fn raw(text: impl Into<String>) -> Self {
        Detail {
            key: "",
            arg: text.into(),
        }
    }

    fn keyed(key: &'static str, arg: impl Into<String>) -> Self {
        Detail {
            key,
            arg: arg.into(),
        }
    }

    fn render(&self, lang: &str) -> String {
        if self.key.is_empty() {
            self.arg.clone()
        } else {
            i18n::trf(lang, self.key, &[&self.arg])
        }
    }
}

/// Classified failure of a fetch attempt.
#[derive(Debug, Clone, PartialEq)]
pub enum FetchError {
    /// Connect / timeout / DNS problems.
    Network(Detail),
    /// Google refused to serve the page (429/403, captcha, "unusual traffic").
    Blocked(Detail),
    /// The page was served but did not look like a profile page.
    Parse(Detail),
}

impl FetchError {
    /// Value stored in `fetch_log.status`.
    pub fn status(&self) -> &'static str {
        match self {
            FetchError::Network(_) => "network",
            FetchError::Blocked(_) => "blocked",
            FetchError::Parse(_) => "parse",
        }
    }

    fn detail(&self) -> &Detail {
        match self {
            FetchError::Network(d) | FetchError::Blocked(d) | FetchError::Parse(d) => d,
        }
    }

    /// Message in the given language, without the category prefix.
    pub fn message(&self, lang: &str) -> String {
        self.detail().render(lang)
    }

    /// Localized text for the UI / log: "<category>: <detail>".
    pub fn user_message(&self, lang: &str) -> String {
        let prefix = match self {
            FetchError::Network(_) => "err.network",
            FetchError::Blocked(_) => "err.blocked",
            FetchError::Parse(_) => "err.parse",
        };
        format!("{}: {}", i18n::tr(lang, prefix), self.message(lang))
    }
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.user_message(i18n::EN))
    }
}

impl std::error::Error for FetchError {}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Article {
    pub key: String,
    pub title: String,
    pub authors: String,
    pub venue: String,
    pub year: Option<i32>,
    pub citations: i64,
    pub url: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Stats {
    pub cit_all: i64,
    pub cit_since: i64,
    pub h_all: i64,
    pub h_since: i64,
    pub i10_all: i64,
    pub i10_since: i64,
    pub since_year: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProfileData {
    pub name: String,
    pub stats: Stats,
    pub articles: Vec<Article>,
    /// (year, citations) sorted by year ascending.
    pub yearly: Vec<(i32, i64)>,
}

macro_rules! selector {
    ($name:ident, $css:expr) => {
        static $name: LazyLock<Selector> =
            LazyLock::new(|| Selector::parse($css).expect("valid static CSS selector"));
    };
}

selector!(SEL_NAME, "#gsc_prf_in");
selector!(SEL_STATS_TABLE, "#gsc_rsb_st");
selector!(SEL_STATS_HEAD, "th.gsc_rsb_sth");
selector!(SEL_STATS_ROW, "tbody tr");
selector!(SEL_STATS_LABEL, "td.gsc_rsb_sc1 a");
selector!(SEL_STATS_VALUE, "td.gsc_rsb_std");
selector!(SEL_ROW, "tr.gsc_a_tr");
selector!(SEL_TITLE, "a.gsc_a_at");
selector!(SEL_GRAY, "div.gs_gray");
selector!(SEL_CITES, "a.gsc_a_ac");
selector!(SEL_YEAR, "span.gsc_a_h");
selector!(SEL_HIST_LABEL, "span.gsc_g_t");
selector!(SEL_HIST_BAR, "a.gsc_g_a");
selector!(SEL_HIST_COUNT, "span.gsc_g_al");

fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn text_of(el: &ElementRef) -> String {
    collapse_ws(&el.text().collect::<String>())
}

/// Text of the element's direct text nodes only (child elements are skipped).
fn own_text(el: &ElementRef) -> String {
    let mut out = String::new();
    for child in el.children() {
        if let Some(t) = child.value().as_text() {
            out.push_str(t);
        }
    }
    collapse_ws(&out)
}

fn digits_to_i64(s: &str) -> Option<i64> {
    let d: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    if d.is_empty() {
        None
    } else {
        d.parse().ok()
    }
}

/// Extracts the 4-digit year from a header such as "Since 2021".
fn year_in(s: &str) -> Option<i32> {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            if i - start == 4 {
                return chars[start..i].iter().collect::<String>().parse().ok();
            }
        } else {
            i += 1;
        }
    }
    None
}

/// Value of `right:NNpx` in an inline style.
fn style_right(style: &str) -> Option<i32> {
    let idx = style.find("right:")?;
    let rest = &style[idx + "right:".len()..];
    let num: String = rest
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '-')
        .collect();
    num.parse().ok()
}

/// Full `citation_for_view` value (e.g. `PROFILEID:ARTICLEID`) from an article href.
fn citation_key(href: &str) -> Option<String> {
    let idx = href.find("citation_for_view=")?;
    let rest = &href[idx + "citation_for_view=".len()..];
    let end = rest.find(['&', '#']).unwrap_or(rest.len());
    let key = rest[..end].replace("%3A", ":").replace("%3a", ":");
    if key.is_empty() {
        None
    } else {
        Some(key)
    }
}

/// Parses all article rows found in the document.
pub fn parse_articles(doc: &Html) -> Vec<Article> {
    let mut out = Vec::new();
    for tr in doc.select(&SEL_ROW) {
        let Some(a) = tr.select(&SEL_TITLE).next() else {
            continue;
        };
        let href = a.value().attr("href").unwrap_or_default();
        let Some(key) = citation_key(href) else {
            continue;
        };
        let grays: Vec<ElementRef> = tr.select(&SEL_GRAY).collect();
        let authors = grays.first().map(text_of).unwrap_or_default();
        let venue = grays
            .get(1)
            .map(own_text)
            .unwrap_or_default()
            .trim_end_matches([',', ' '])
            .to_string();
        let citations = tr
            .select(&SEL_CITES)
            .next()
            .and_then(|e| digits_to_i64(&text_of(&e)))
            .unwrap_or(0);
        let year = tr
            .select(&SEL_YEAR)
            .next()
            .and_then(|e| text_of(&e).parse::<i32>().ok());
        let url = if href.starts_with("http") {
            href.to_string()
        } else {
            format!("{BASE_URL}{href}")
        };
        out.push(Article {
            key,
            title: text_of(&a),
            authors,
            venue,
            year,
            citations,
            url,
        });
    }
    out
}

fn parse_stats(doc: &Html) -> Result<Stats, FetchError> {
    let table = doc
        .select(&SEL_STATS_TABLE)
        .next()
        .ok_or_else(|| FetchError::Parse(Detail::keyed("err.d.no_stats_table", "")))?;

    let heads: Vec<String> = table.select(&SEL_STATS_HEAD).map(|e| text_of(&e)).collect();
    let since_year = heads
        .get(2)
        .and_then(|h| year_in(h))
        .or_else(|| heads.iter().find_map(|h| year_in(h)))
        .ok_or_else(|| FetchError::Parse(Detail::keyed("err.d.no_since", "")))?;

    let mut stats = Stats {
        since_year,
        ..Stats::default()
    };
    let mut found = 0;
    for row in table.select(&SEL_STATS_ROW) {
        let Some(label) = row.select(&SEL_STATS_LABEL).next().map(|e| text_of(&e)) else {
            continue;
        };
        let vals: Vec<i64> = row
            .select(&SEL_STATS_VALUE)
            .map(|e| digits_to_i64(&text_of(&e)).unwrap_or(0))
            .collect();
        if vals.len() < 2 {
            continue;
        }
        match label.to_lowercase().as_str() {
            "citations" => {
                stats.cit_all = vals[0];
                stats.cit_since = vals[1];
            }
            "h-index" => {
                stats.h_all = vals[0];
                stats.h_since = vals[1];
            }
            "i10-index" => {
                stats.i10_all = vals[0];
                stats.i10_since = vals[1];
            }
            _ => continue,
        }
        found += 1;
    }
    if found < 3 {
        return Err(FetchError::Parse(Detail::keyed("err.d.missing_rows", "")));
    }
    Ok(stats)
}

fn parse_yearly(doc: &Html) -> Vec<(i32, i64)> {
    let mut labels: Vec<(i32, i32)> = Vec::new(); // (year, right offset)
    let mut seen = HashSet::new();
    for span in doc.select(&SEL_HIST_LABEL) {
        let Some(right) = span.value().attr("style").and_then(style_right) else {
            continue;
        };
        let Ok(year) = text_of(&span).parse::<i32>() else {
            continue;
        };
        if seen.insert(year) {
            labels.push((year, right));
        }
    }
    let mut bars: HashMap<i32, i64> = HashMap::new();
    for a in doc.select(&SEL_HIST_BAR) {
        let Some(right) = a.value().attr("style").and_then(style_right) else {
            continue;
        };
        let count = a
            .select(&SEL_HIST_COUNT)
            .next()
            .and_then(|e| digits_to_i64(&text_of(&e)))
            .unwrap_or(0);
        bars.insert(right, count);
    }
    let mut out: Vec<(i32, i64)> = labels
        .into_iter()
        .map(|(year, right)| (year, bars.get(&(right + 5)).copied().unwrap_or(0)))
        .collect();
    out.sort_by_key(|(y, _)| *y);
    out
}

/// Parses the first profile page (name, stats, articles, histogram).
pub fn parse_profile(html: &str) -> Result<ProfileData, FetchError> {
    let doc = Html::parse_document(html);
    let stats = parse_stats(&doc)?;
    let name = doc
        .select(&SEL_NAME)
        .next()
        .map(|e| text_of(&e))
        .unwrap_or_default();
    Ok(ProfileData {
        name,
        stats,
        articles: parse_articles(&doc),
        yearly: parse_yearly(&doc),
    })
}

/// Decides whether a response means "blocked" or a generic HTTP failure.
pub fn classify_response(status: u16, final_url: &str, body: &str) -> Result<(), FetchError> {
    if status == 429 || status == 403 {
        return Err(FetchError::Blocked(Detail::raw(format!("HTTP {status}"))));
    }
    if final_url.contains("sorry") {
        return Err(FetchError::Blocked(Detail::keyed("err.d.redirect", final_url)));
    }
    let lower = body.to_lowercase();
    for marker in ["gs_captcha", "unusual traffic", "/sorry/"] {
        if lower.contains(marker) {
            return Err(FetchError::Blocked(Detail::keyed("err.d.marker", marker)));
        }
    }
    if !(200..300).contains(&status) {
        return Err(FetchError::Network(Detail::raw(format!("HTTP {status}"))));
    }
    Ok(())
}

pub fn page_url(profile_id: &str, cstart: usize) -> String {
    format!("{BASE_URL}/citations?user={profile_id}&hl=en&cstart={cstart}&pagesize={PAGE_SIZE}")
}

fn build_client() -> Result<reqwest::Client, FetchError> {
    use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE};
    let mut headers = HeaderMap::new();
    headers.insert(
        ACCEPT,
        HeaderValue::from_static(
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
        ),
    );
    headers.insert(
        ACCEPT_LANGUAGE,
        HeaderValue::from_static("en-US,en;q=0.9"),
    );
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .default_headers(headers)
        .cookie_store(true)
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| FetchError::Network(Detail::keyed("err.d.client", e.to_string())))
}

async fn get_page(client: &reqwest::Client, url: &str) -> Result<String, FetchError> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| FetchError::Network(Detail::raw(e.to_string())))?;
    let status = resp.status().as_u16();
    let final_url = resp.url().to_string();
    let body = resp
        .text()
        .await
        .map_err(|e| FetchError::Network(Detail::raw(e.to_string())))?;
    classify_response(status, &final_url, &body)?;
    Ok(body)
}

/// Pseudo-random delay (3000..=6000 ms) without an extra dependency.
fn random_delay() -> Duration {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut h = RandomState::new().build_hasher();
    h.write_u64(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0),
    );
    Duration::from_millis(3000 + h.finish() % 3001)
}

/// Guards against a page that parsed "successfully" but lost its article list (for example
/// after a Scholar layout change). Both cases are reported as a parse error so no snapshot
/// is stored:
/// - the profile shows citations but no article was read, or
/// - the article count fell by more than half compared with the previous snapshot while the
///   citation total did not drop.
///
/// `previous` is `(citation total, article count)` of the latest stored snapshot.
pub fn validate_article_list(data: &ProfileData, previous: Option<(i64, usize)>) -> Result<(), FetchError> {
    let now = data.articles.len();
    if data.stats.cit_all > 0 && now == 0 {
        return Err(FetchError::Parse(Detail::keyed("err.d.no_articles", "")));
    }
    if let Some((prev_cit, prev_n)) = previous {
        if prev_n > 0 && now * 2 < prev_n && data.stats.cit_all >= prev_cit {
            return Err(FetchError::Parse(Detail::keyed(
                "err.d.articles_dropped",
                format!("{prev_n} → {now}"),
            )));
        }
    }
    Ok(())
}

/// Fetches the complete profile (all article pages) with one cookie-enabled client.
pub async fn fetch_profile(profile_id: &str) -> Result<ProfileData, FetchError> {
    let client = build_client()?;
    let mut data: Option<ProfileData> = None;
    for page in 0..MAX_PAGES {
        let html = get_page(&client, &page_url(profile_id, page * PAGE_SIZE)).await?;
        let rows_on_page;
        match data.as_mut() {
            None => {
                let first = parse_profile(&html)?;
                rows_on_page = first.articles.len();
                data = Some(first);
            }
            Some(d) => {
                let articles = parse_articles(&Html::parse_document(&html));
                rows_on_page = articles.len();
                let known: HashSet<String> = d.articles.iter().map(|a| a.key.clone()).collect();
                d.articles
                    .extend(articles.into_iter().filter(|a| !known.contains(&a.key)));
            }
        }
        if rows_on_page < PAGE_SIZE {
            break;
        }
        tokio::time::sleep(random_delay()).await;
    }
    data.ok_or_else(|| FetchError::Parse(Detail::keyed("err.d.empty", "")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hand-written page that mirrors the structure of a real profile page
    /// (same ids and classes). Names and titles are made up.
    const SAMPLE: &str = concat!(
        r#"<!doctype html><html><head><title>Jane Doe - Google Scholar</title></head><body>"#,
        r#"<div id="gsc_prf_in">Jane Doe</div>"#,
        r#"<div class="gsc_rsb_s gsc_prf_pnl" id="gsc_rsb_cit" role="region"><h3 class="gsc_rsb_header"><span class="gsc_rsb_title">Cited by</span></h3>"#,
        r#"<table id="gsc_rsb_st"><thead><tr><th class="gsc_rsb_sth"></th><th class="gsc_rsb_sth">All</th><th class="gsc_rsb_sth">Since 2021</th></tr></thead><tbody>"#,
        r#"<tr><td class="gsc_rsb_sc1"><a href="javascript:void(0)" class="gsc_rsb_f gs_ibl" title="x">Citations</a></td><td class="gsc_rsb_std">1,100</td><td class="gsc_rsb_std">60</td></tr>"#,
        r#"<tr><td class="gsc_rsb_sc1"><a href="javascript:void(0)" class="gsc_rsb_f gs_ibl" title="x">h-index</a></td><td class="gsc_rsb_std">5</td><td class="gsc_rsb_std">4</td></tr>"#,
        r#"<tr><td class="gsc_rsb_sc1"><a href="javascript:void(0)" class="gsc_rsb_f gs_ibl" title="x">i10-index</a></td><td class="gsc_rsb_std">3</td><td class="gsc_rsb_std">2</td></tr>"#,
        r#"</tbody></table>"#,
        r#"<div class="gsc_md_hist_w"><div class="gsc_md_hist_b">"#,
        r#"<span class="gsc_g_t" style="right:99px">2022</span><span class="gsc_g_t" style="right:67px">2023</span><span class="gsc_g_t" style="right:35px">2024</span><span class="gsc_g_t" style="right:3px">2025</span>"#,
        r#"<a href="javascript:void(0)" class="gsc_g_a" style="right:104px;top:150px;height:10px;z-index:3"><span class="gsc_g_al">5</span></a>"#,
        r#"<a href="javascript:void(0)" class="gsc_g_a" style="right:40px;top:100px;height:60px;z-index:2"><span class="gsc_g_al">25</span></a>"#,
        r#"<a href="javascript:void(0)" class="gsc_g_a" style="right:8px;top:12px;height:148px;z-index:1"><span class="gsc_g_al">70</span></a>"#,
        r#"</div></div></div>"#,
        r#"<table id="gsc_a_t"><tbody id="gsc_a_b">"#,
        // article A: normal row with venue and year
        r#"<tr class="gsc_a_tr"><td class="gsc_a_t"><a href="/citations?view_op=view_citation&amp;hl=en&amp;user=SAMPLEID1&amp;citation_for_view=SAMPLEID1:AAA111" class="gsc_a_at">Sample Paper A</a><div class="gs_gray">J Doe, A Roe</div><div class="gs_gray">Journal of Sample Studies 12, 34-56<span class="gs_oph">, 2019</span></div></td><td class="gsc_a_c"><a href="https://scholar.google.com/scholar?oi=bibs&amp;hl=en&amp;cites=1,2" class="gsc_a_ac gs_ibl">40</a></td><td class="gsc_a_y"><span class="gsc_a_h gsc_a_hc gs_ibl">2019</span></td></tr>"#,
        // article B: empty citations cell
        r#"<tr class="gsc_a_tr"><td class="gsc_a_t"><a href="/citations?view_op=view_citation&amp;hl=en&amp;user=SAMPLEID1&amp;citation_for_view=SAMPLEID1:BBB222" class="gsc_a_at">Sample Paper B</a><div class="gs_gray">J Doe</div><div class="gs_gray">Sample Conference 2024<span class="gs_oph">, 2024</span></div></td><td class="gsc_a_c"><a href="" class="gsc_a_ac gs_ibl"></a></td><td class="gsc_a_y"><span class="gsc_a_h gsc_a_hc gs_ibl">2024</span></td></tr>"#,
        // article C: empty year and empty venue line
        r#"<tr class="gsc_a_tr"><td class="gsc_a_t"><a href="/citations?view_op=view_citation&amp;hl=en&amp;user=SAMPLEID1&amp;citation_for_view=SAMPLEID1:CCC333" class="gsc_a_at">Sample Paper C</a><div class="gs_gray">A Roe, J Doe</div><div class="gs_gray"></div></td><td class="gsc_a_c"><a href="https://scholar.google.com/scholar?oi=bibs&amp;hl=en&amp;cites=3" class="gsc_a_ac gs_ibl">60</a><span class="gsc_a_m"><a href="javascript:void(0)" class="gsc_a_am" data-eid="SAMPLEID1:CCC333">*</a></span></td><td class="gsc_a_y"><span class="gsc_a_h gsc_a_hc gs_ibl"></span></td></tr>"#,
        r#"</tbody></table></body></html>"#,
    );

    #[test]
    fn sample_name_and_stats() {
        let p = parse_profile(SAMPLE).expect("sample parses");
        assert_eq!(p.name, "Jane Doe");
        let s = &p.stats;
        assert_eq!((s.cit_all, s.cit_since), (1100, 60));
        assert_eq!((s.h_all, s.h_since), (5, 4));
        assert_eq!((s.i10_all, s.i10_since), (3, 2));
        assert_eq!(s.since_year, 2021);
    }

    #[test]
    fn sample_articles() {
        let p = parse_profile(SAMPLE).expect("sample parses");
        assert_eq!(p.articles.len(), 3);

        let a = &p.articles[0];
        assert_eq!(a.title, "Sample Paper A");
        assert_eq!(a.key, "SAMPLEID1:AAA111");
        assert_eq!(a.authors, "J Doe, A Roe");
        assert_eq!(a.venue, "Journal of Sample Studies 12, 34-56");
        assert_eq!(a.citations, 40);
        assert_eq!(a.year, Some(2019));
        assert!(a.url.starts_with("https://scholar.google.com/citations?view_op=view_citation&hl=en"));
        assert!(!a.url.contains("&amp;"));

        // empty citations cell counts as zero
        let b = &p.articles[1];
        assert_eq!(b.title, "Sample Paper B");
        assert_eq!(b.citations, 0);
        assert_eq!(b.year, Some(2024));
        assert_eq!(b.venue, "Sample Conference 2024");

        // empty year and empty venue line; the "*" marker next to the count is ignored
        let c = &p.articles[2];
        assert_eq!(c.title, "Sample Paper C");
        assert_eq!(c.citations, 60);
        assert_eq!(c.year, None);
        assert_eq!(c.venue, "");
        assert_eq!(c.authors, "A Roe, J Doe");

        let keys: HashSet<_> = p.articles.iter().map(|a| a.key.clone()).collect();
        assert_eq!(keys.len(), 3);
        assert_eq!(p.articles.iter().map(|a| a.citations).sum::<i64>(), 100);
    }

    #[test]
    fn sample_yearly_histogram_with_missing_bar() {
        let p = parse_profile(SAMPLE).expect("sample parses");
        // 2023 has a label but no bar: it is reported as zero
        assert_eq!(p.yearly, vec![(2022, 5), (2023, 0), (2024, 25), (2025, 70)]);
    }

    #[test]
    fn sample_is_not_classified_as_blocked() {
        assert_eq!(
            classify_response(200, "https://scholar.google.com/citations?user=x", SAMPLE),
            Ok(())
        );
    }

    #[test]
    fn classifies_blocked_responses() {
        assert!(matches!(classify_response(429, "u", ""), Err(FetchError::Blocked(_))));
        assert!(matches!(classify_response(403, "u", ""), Err(FetchError::Blocked(_))));
        assert!(matches!(
            classify_response(200, "https://www.google.com/sorry/index?continue=x", ""),
            Err(FetchError::Blocked(_))
        ));
        assert!(matches!(
            classify_response(200, "u", "<div id=\"gs_captcha_ccl\">"),
            Err(FetchError::Blocked(_))
        ));
        assert!(matches!(
            classify_response(200, "u", "Our systems have detected unusual traffic"),
            Err(FetchError::Blocked(_))
        ));
        assert!(matches!(classify_response(503, "u", "x"), Err(FetchError::Network(_))));
    }

    #[test]
    fn non_profile_page_is_parse_error() {
        let err = parse_profile("<html><body>hello</body></html>").unwrap_err();
        assert!(matches!(err, FetchError::Parse(_)));
        assert_eq!(err.status(), "parse");
    }

    #[test]
    fn error_messages_are_localized() {
        let err = parse_profile("<html><body>hello</body></html>").unwrap_err();
        assert_eq!(err.user_message("en"), "Page could not be parsed: #gsc_rsb_st table not found");
        assert_eq!(err.user_message("tr"), "Sayfa çözümlenemedi: #gsc_rsb_st tablosu bulunamadı");

        let blocked = classify_response(429, "u", "").unwrap_err();
        assert_eq!(blocked.user_message("en"), "Google blocked access: HTTP 429");
        assert_eq!(blocked.user_message("tr"), "Google erişimi engelledi: HTTP 429");

        let marker = classify_response(200, "u", "unusual traffic").unwrap_err();
        assert_eq!(marker.user_message("tr"), "Google erişimi engelledi: sayfada 'unusual traffic' ifadesi var");
        assert_eq!(marker.user_message("en"), "Google blocked access: page contains 'unusual traffic'");
    }

    #[test]
    fn helper_parsers() {
        assert_eq!(year_in("Since 2021"), Some(2021));
        assert_eq!(year_in("All"), None);
        assert_eq!(style_right("right:264px;top:158px"), Some(264));
        assert_eq!(digits_to_i64("1,234"), Some(1234));
        assert_eq!(digits_to_i64(""), None);
        assert_eq!(
            citation_key("/citations?view_op=view_citation&hl=en&user=U&citation_for_view=U%3AXYZ&x=1"),
            Some("U:XYZ".to_string())
        );
    }

    #[test]
    fn random_delay_is_in_range() {
        for _ in 0..50 {
            let d = random_delay();
            assert!(d >= Duration::from_secs(3) && d <= Duration::from_secs(6));
        }
    }

    fn data_with(cit_all: i64, n_articles: usize) -> ProfileData {
        let mut p = parse_profile(SAMPLE).expect("sample parses");
        p.stats.cit_all = cit_all;
        p.articles = (0..n_articles)
            .map(|i| Article {
                key: format!("k{i}"),
                title: format!("Title {i}"),
                authors: String::new(),
                venue: String::new(),
                year: None,
                citations: 1,
                url: String::new(),
            })
            .collect();
        p
    }

    #[test]
    fn empty_article_list_with_citations_is_a_parse_error() {
        let err = validate_article_list(&data_with(120, 0), None).expect_err("must fail");
        assert_eq!(err.status(), "parse");
        assert!(err.user_message("tr").contains("makale listesi okunamadı"), "{}", err.user_message("tr"));
        assert!(err.user_message("en").contains("article list could not be read"), "{}", err.user_message("en"));
        // also with history: still the same error
        assert!(validate_article_list(&data_with(120, 0), Some((100, 10))).is_err());
        // a profile with no citations and no articles is legitimate
        assert!(validate_article_list(&data_with(0, 0), None).is_ok());
        assert!(validate_article_list(&data_with(0, 0), Some((0, 0))).is_ok());
    }

    #[test]
    fn large_article_drop_without_citation_drop_is_a_parse_error() {
        // 10 -> 4 is a 60 % drop and citations rose: refuse
        let err = validate_article_list(&data_with(105, 4), Some((100, 10))).expect_err("must fail");
        assert_eq!(err.status(), "parse");
        assert!(err.message("en").contains("10 → 4"), "{}", err.message("en"));
        // citations equal: still refused
        assert!(validate_article_list(&data_with(100, 4), Some((100, 10))).is_err());
        // exactly half is allowed ("more than 50 %" is needed)
        assert!(validate_article_list(&data_with(105, 5), Some((100, 10))).is_ok());
        // a drop together with a citation drop is a real change
        assert!(validate_article_list(&data_with(60, 4), Some((100, 10))).is_ok());
        // growth and first snapshot are fine
        assert!(validate_article_list(&data_with(105, 12), Some((100, 10))).is_ok());
        assert!(validate_article_list(&data_with(105, 12), None).is_ok());
        // previous snapshot with no articles never triggers the drop rule
        assert!(validate_article_list(&data_with(5, 1), Some((5, 0))).is_ok());
    }

    // ---- optional checks against a locally saved real profile page -------------
    // The file is not part of the repository (see .gitignore). When it is absent
    // these tests print a note and return.

    fn local_fixture() -> Option<String> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("profile.html");
        match std::fs::read(&path) {
            Ok(bytes) => Some(String::from_utf8_lossy(&bytes).into_owned()),
            Err(_) => {
                println!("note: {} not found; skipping local fixture test", path.display());
                None
            }
        }
    }

    #[test]
    fn local_fixture_parses_consistently() {
        let Some(html) = local_fixture() else { return };
        let p = parse_profile(&html).expect("fixture parses");
        assert!(!p.name.is_empty());
        assert!(p.stats.since_year > 2000);
        assert!(p.stats.cit_all >= p.stats.cit_since);
        assert!(!p.articles.is_empty());
        let keys: HashSet<_> = p.articles.iter().map(|a| a.key.clone()).collect();
        assert_eq!(keys.len(), p.articles.len(), "article keys must be unique");
        assert!(p.articles.iter().all(|a| !a.title.is_empty() && a.url.starts_with(BASE_URL)));
        assert!(p.articles.iter().all(|a| !a.url.contains("&amp;")));
        assert!(!p.yearly.is_empty());
        assert!(p.yearly.windows(2).all(|w| w[0].0 < w[1].0), "years ascend");
        assert!(p.yearly.iter().map(|(_, c)| c).sum::<i64>() <= p.stats.cit_all);
    }

    #[test]
    fn local_fixture_is_not_classified_as_blocked() {
        let Some(html) = local_fixture() else { return };
        assert_eq!(
            classify_response(200, "https://scholar.google.com/citations?user=x", &html),
            Ok(())
        );
    }
}
