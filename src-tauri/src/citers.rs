//! "Who cites you": aggregates over the stored OpenAlex citing works (countries, institutions,
//! venues, fields, types, authors, self-citations, influential works).
//!
//! Every aggregate works on the same model: the distinct citing works (one work that cites several
//! of the user's articles counts once) that pass the period and the article filter. The
//! "exclude self-citations" switch removes self-citing works from every aggregate except the
//! self / co-author / external split. Percentages are left to the caller; the aggregates return
//! counts and how many works lack the field (`Cov`).
//!
//! Entities that are "new this year" (country, institution): their earliest citing work, over all
//! time, respecting the article filter and the self-citation switch but ignoring the period, is
//! dated in the current calendar year.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use anyhow::Result;
use rusqlite::Connection;
use serde::Serialize;

use crate::db;

/// A citing work counts as "influential" when it is cited more often than this.
pub const INFLUENTIAL_MIN: i64 = 100;

// ------------------------------------------------------------------ names

fn fold_char(c: char) -> Option<&'static str> {
    Some(match c {
        'ç' | 'ć' | 'č' => "c",
        'ğ' => "g",
        'ı' | 'í' | 'ì' | 'î' | 'ï' => "i",
        'ö' | 'ó' | 'ò' | 'ô' | 'õ' | 'ø' => "o",
        'ş' | 'š' | 'ś' => "s",
        'ü' | 'ú' | 'ù' | 'û' => "u",
        'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ā' => "a",
        'é' | 'è' | 'ê' | 'ë' | 'ē' | 'ě' => "e",
        'ñ' | 'ń' => "n",
        'ž' | 'ź' | 'ż' => "z",
        'ł' => "l",
        'ř' => "r",
        'ý' | 'ÿ' => "y",
        'ß' => "ss",
        _ => return None,
    })
}

/// Lower case, diacritics folded, everything but letters and digits turned into a space, runs of
/// spaces collapsed.
pub fn fold_name(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars().flat_map(char::to_lowercase) {
        if ('\u{300}'..='\u{36f}').contains(&c) {
            continue; // combining marks (the dot of a Turkish capital İ)
        }
        if let Some(f) = fold_char(c) {
            out.push_str(f);
        } else if c.is_alphanumeric() {
            out.push(c);
        } else {
            out.push(' ');
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn name_tokens(s: &str) -> Vec<String> {
    fold_name(s).split_whitespace().map(str::to_string).collect()
}

/// `surname|first initial` of a name given as `Ada Example`, `A. Example` or `AB Example`
/// (Scholar's author lists). `None` for a single word.
pub fn author_key(name: &str) -> Option<String> {
    let t = name_tokens(name);
    if t.len() < 2 {
        return None;
    }
    let initial = t[0].chars().next()?;
    Some(format!("{}|{}", t[t.len() - 1], initial))
}

/// Full-name match used for the self-citation fallback: the same words in any order, or the same
/// surname and the same given name (middle names ignored). Initials alone never match; a profile
/// name of one word never matches.
pub fn same_person(a: &str, b: &str) -> bool {
    let (ta, tb) = (name_tokens(a), name_tokens(b));
    if ta.len() < 2 || tb.len() < 2 {
        return false;
    }
    let (mut sa, mut sb) = (ta.clone(), tb.clone());
    sa.sort();
    sb.sort();
    if sa == sb {
        return true;
    }
    let long = |t: &Vec<String>| t[0].chars().count() > 1 && t[t.len() - 1].chars().count() > 1;
    long(&ta) && long(&tb) && ta[0] == tb[0] && ta[ta.len() - 1] == tb[tb.len() - 1]
}

// ------------------------------------------------------------------ model

/// What the classification needs to know about the user.
#[derive(Debug, Clone, Default)]
pub struct Ctx {
    /// ORCID iD from the settings (empty when not set).
    pub orcid: String,
    /// Profile name (Scholar), for the name fallback.
    pub name: String,
    /// `author_key`s of the co-authors of the user's own articles (from the Scholar author lists).
    pub coauthors: HashSet<String>,
    /// Current calendar year ("new this year").
    pub year: i32,
}

impl Ctx {
    /// Reads the profile name and the co-author keys from the database.
    pub fn load(conn: &Connection, orcid: &str, year: i32) -> Result<Ctx> {
        let name = db::latest_snapshot(conn)?.map(|s| s.name).unwrap_or_default();
        let mut st = conn.prepare("SELECT authors FROM articles")?;
        let lists = st.query_map([], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Ctx::new(orcid, &name, lists.iter().map(String::as_str), year))
    }

    /// `author_lists` are Scholar author strings (`AB Example, C Sample, ...`).
    pub fn new<'a>(orcid: &str, name: &str, author_lists: impl IntoIterator<Item = &'a str>, year: i32) -> Ctx {
        let me = author_key(name);
        let mut coauthors = HashSet::new();
        for list in author_lists {
            for one in list.split(',') {
                if let Some(k) = author_key(one) {
                    if Some(&k) != me.as_ref() {
                        coauthors.insert(k);
                    }
                }
            }
        }
        Ctx { orcid: orcid.trim().to_ascii_uppercase(), name: name.to_string(), coauthors, year }
    }

    pub fn name_usable(&self) -> bool {
        name_tokens(&self.name).len() >= 2
    }

    fn is_me(&self, a: &Author) -> Option<bool> {
        // Some(true): by ORCID, Some(false): by name, None: not the user.
        if !self.orcid.is_empty() && a.orcid == self.orcid {
            return Some(true);
        }
        let foreign_orcid = !a.orcid.is_empty() && !self.orcid.is_empty() && a.orcid != self.orcid;
        if !foreign_orcid && same_person(&a.name, &self.name) {
            return Some(false);
        }
        None
    }
}

/// One raw stored row (one article, one citing work).
#[derive(Debug, Clone, Default)]
pub struct Raw {
    pub article_key: String,
    pub article_title: String,
    pub citing_id: String,
    pub title: String,
    pub date: String,
    pub precision: String,
    pub venue: String,
    pub doi: String,
    pub url: String,
    pub authors: String,
    pub countries: String,
    pub institutions: String,
    pub authorships: String,
    pub author_orcids: String,
    pub topic: String,
    pub topic_field: String,
    pub topic_domain: String,
    pub work_type: String,
    pub language: String,
    pub oa_status: String,
    pub publisher: String,
    pub source_type: String,
    pub cited_by: Option<i64>,
    pub enriched: bool,
    /// OpenAlex source id of the venue (empty before the first 1.4.0 refresh).
    pub source_id: String,
    pub issn_list: String,
}

/// Every stored citing row.
pub fn load_raws(conn: &Connection) -> Result<Vec<Raw>> {
    let mut st = conn.prepare(
        "SELECT b.article_key, COALESCE(a.title,''), b.citing_id, b.citing_title, b.citing_date,
                b.date_precision, b.venue, b.doi, b.url, b.authors, b.countries,
                b.institutions, b.authorships, b.author_orcids, b.topic, b.topic_field,
                b.topic_domain, b.work_type, b.language, b.oa_status, b.publisher, b.source_type,
                b.cited_by, b.enriched, b.source_id, b.issn_list
         FROM backfill_citations b LEFT JOIN articles a ON a.article_key=b.article_key
         ORDER BY b.citing_date DESC, b.citing_id ASC, b.article_key ASC",
    )?;
    let rows = st.query_map([], |r| {
        Ok(Raw {
            article_key: r.get(0)?,
            article_title: r.get(1)?,
            citing_id: r.get(2)?,
            title: r.get(3)?,
            date: r.get(4)?,
            precision: r.get(5)?,
            venue: r.get(6)?,
            doi: r.get(7)?,
            url: r.get(8)?,
            authors: r.get(9)?,
            countries: r.get(10)?,
            institutions: r.get(11)?,
            authorships: r.get(12)?,
            author_orcids: r.get(13)?,
            topic: r.get(14)?,
            topic_field: r.get(15)?,
            topic_domain: r.get(16)?,
            work_type: r.get(17)?,
            language: r.get(18)?,
            oa_status: r.get(19)?,
            publisher: r.get(20)?,
            source_type: r.get(21)?,
            cited_by: r.get(22)?,
            enriched: r.get::<_, i64>(23)? != 0,
            source_id: r.get(24)?,
            issn_list: r.get(25)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Inst {
    pub key: String,
    pub name: String,
    pub cc: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Author {
    pub name: String,
    pub orcid: String,
    /// Keys of `Work::insts`.
    pub insts: Vec<String>,
}

/// How a citing work relates to the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    SelfOrcid,
    SelfName,
    Coauthor,
    External,
}

impl Kind {
    pub fn is_self(self) -> bool {
        matches!(self, Kind::SelfOrcid | Kind::SelfName)
    }
}

/// One distinct citing work.
#[derive(Debug, Clone)]
pub struct Work {
    pub id: String,
    pub title: String,
    pub date: String,
    /// First day the date names (`YYYY-MM-DD`).
    pub start: String,
    /// First day after the period the date names (exclusive).
    pub end: String,
    pub year: i32,
    pub venue: String,
    pub doi: String,
    pub url: String,
    pub countries: Vec<String>,
    pub insts: Vec<Inst>,
    pub authors: Vec<Author>,
    pub orcids: Vec<String>,
    pub topic: String,
    pub field: String,
    pub domain: String,
    pub work_type: String,
    pub language: String,
    pub oa: String,
    pub publisher: String,
    pub source_type: String,
    pub cited_by: Option<i64>,
    pub enriched: bool,
    /// OJP of the venue's journal.
    pub ojp: crate::ojp::Ojp,
    /// The user's articles it cites (key, title), in key order.
    pub articles: Vec<(String, String)>,
    pub kind: Kind,
}

fn start_of(date: &str, precision: &str) -> Option<String> {
    let s = match precision {
        "day" if date.len() >= 10 => date[..10].to_string(),
        "month" if date.len() >= 7 => format!("{}-01", &date[..7]),
        "year" if date.len() >= 4 => format!("{}-01-01", &date[..4]),
        _ => return None,
    };
    chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok()?;
    Some(s)
}

/// First day after the span a date names.
fn end_of(start: &str, precision: &str) -> String {
    let Ok(d) = chrono::NaiveDate::parse_from_str(start, "%Y-%m-%d") else {
        return start.to_string();
    };
    use chrono::Datelike;
    let next = match precision {
        "day" => d.succ_opt(),
        "month" => {
            if d.month() == 12 { chrono::NaiveDate::from_ymd_opt(d.year() + 1, 1, 1) } else { chrono::NaiveDate::from_ymd_opt(d.year(), d.month() + 1, 1) }
        }
        _ => chrono::NaiveDate::from_ymd_opt(d.year() + 1, 1, 1),
    };
    next.map(|n| n.format("%Y-%m-%d").to_string()).unwrap_or_else(|| start.to_string())
}

fn parse_insts(json: &str) -> Vec<Inst> {
    let Ok(v) = serde_json::from_str::<Vec<serde_json::Value>>(json) else {
        return Vec::new();
    };
    v.iter()
        .map(|x| {
            let s = |k: &str| x.get(k).and_then(|y| y.as_str()).unwrap_or("").to_string();
            Inst { key: s("i"), name: s("n"), cc: s("c") }
        })
        .filter(|i| !i.key.is_empty() || !i.name.is_empty())
        .collect()
}

/// Authors from the authorships JSON; for rows without it, the plain names of the authors string.
fn parse_authors(json: &str, plain: &str) -> Vec<Author> {
    if let Ok(v) = serde_json::from_str::<Vec<serde_json::Value>>(json) {
        let list: Vec<Author> = v
            .iter()
            .map(|x| Author {
                name: x.get("n").and_then(|y| y.as_str()).unwrap_or("").to_string(),
                orcid: x.get("o").and_then(|y| y.as_str()).unwrap_or("").to_string(),
                insts: x
                    .get("i")
                    .and_then(|y| y.as_array())
                    .map(|a| a.iter().filter_map(|k| k.as_str().map(str::to_string)).collect())
                    .unwrap_or_default(),
            })
            .collect();
        if !list.is_empty() {
            return list;
        }
    }
    plain
        .split(';')
        .map(str::trim)
        .filter(|n| !n.is_empty() && *n != "et al.")
        .map(|n| Author { name: n.to_string(), ..Author::default() })
        .collect()
}

fn classify(w: &Work, ctx: &Ctx) -> Kind {
    let mut by_name = false;
    let mut orcid = !ctx.orcid.is_empty() && w.orcids.contains(&ctx.orcid);
    for a in &w.authors {
        match ctx.is_me(a) {
            Some(true) => orcid = true,
            Some(false) => by_name = true,
            None => {}
        }
    }
    if orcid {
        Kind::SelfOrcid
    } else if by_name {
        Kind::SelfName
    } else if w.authors.iter().any(|a| author_key(&a.name).is_some_and(|k| ctx.coauthors.contains(&k))) {
        Kind::Coauthor
    } else {
        Kind::External
    }
}

fn split_list(s: &str, sep: char) -> Vec<String> {
    s.split(sep).map(str::trim).filter(|x| !x.is_empty()).map(str::to_string).collect()
}

/// Distinct citing works from the raw rows. `article` (a key, or empty for all) restricts the
/// rows first, so a work's `articles` lists only that article. Of the rows of one work the one
/// that was read with the new fields is used.
pub fn build_works(raws: &[Raw], article: &str, ctx: &Ctx) -> Vec<Work> {
    let mut order: Vec<&str> = Vec::new();
    let mut groups: HashMap<&str, Vec<&Raw>> = HashMap::new();
    for r in raws {
        groups
            .entry(r.citing_id.as_str())
            .or_insert_with(|| {
                order.push(r.citing_id.as_str());
                Vec::new()
            })
            .push(r);
    }
    let mut out = Vec::new();
    for id in order {
        let all_rows = &groups[id];
        // The article filter narrows the articles of a work, never the row its data comes from.
        let rows: Vec<&Raw> = all_rows.iter().copied().filter(|r| article.is_empty() || r.article_key == article).collect();
        if rows.is_empty() {
            continue;
        }
        let best = all_rows.iter().find(|r| r.enriched).unwrap_or(&all_rows[0]);
        let Some(start) = start_of(&best.date, &best.precision) else {
            continue;
        };
        let mut articles: Vec<(String, String)> = rows.iter().map(|r| (r.article_key.clone(), r.article_title.clone())).collect();
        articles.sort();
        articles.dedup_by(|a, b| a.0 == b.0);
        let mut orcids = split_list(&best.author_orcids, ',');
        let authors = parse_authors(&best.authorships, &best.authors);
        orcids.extend(authors.iter().filter(|a| !a.orcid.is_empty()).map(|a| a.orcid.clone()));
        orcids.sort();
        orcids.dedup();
        let mut w = Work {
            id: id.to_string(),
            title: best.title.clone(),
            date: best.date.clone(),
            year: start[..4].parse().unwrap_or(0),
            end: end_of(&start, &best.precision),
            start,
            venue: best.venue.trim().to_string(),
            doi: best.doi.clone(),
            url: best.url.clone(),
            countries: split_list(&best.countries, ','),
            insts: parse_insts(&best.institutions),
            authors,
            orcids,
            topic: best.topic.clone(),
            field: best.topic_field.clone(),
            domain: best.topic_domain.clone(),
            work_type: best.work_type.clone(),
            language: best.language.clone(),
            oa: best.oa_status.clone(),
            publisher: best.publisher.trim().to_string(),
            source_type: best.source_type.clone(),
            cited_by: best.cited_by,
            enriched: best.enriched,
            ojp: crate::ojp::lookup(
                all_rows.iter().map(|r| r.source_id.as_str()).find(|i| !i.is_empty()).unwrap_or(""),
                all_rows.iter().map(|r| r.issn_list.as_str()).find(|i| !i.is_empty()).unwrap_or(""),
                &best.source_type,
            ),
            articles,
            kind: Kind::External,
        };
        w.kind = classify(&w, ctx);
        out.push(w);
    }
    out
}

/// Period and article filter of the tab.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    /// First day (`YYYY-MM-DD`, inclusive); empty = no start.
    pub from: String,
    /// Last day (inclusive); empty = no end.
    pub to: String,
    /// Article key; empty = all articles.
    pub article: String,
    pub exclude_self: bool,
}

/// A work is in the period when the span its date names overlaps it (a work dated to a month or
/// a year only counts when that month / year touches the period), the same rule as the Citing
/// works list and the report.
fn in_period(w: &Work, f: &Filter) -> bool {
    (f.from.is_empty() || w.end.as_str() > f.from.as_str()) && (f.to.is_empty() || w.start.as_str() <= f.to.as_str())
}

/// The works of one filter setting.
pub struct Model {
    works: Vec<Work>,
    ctx: Ctx,
    filter: Filter,
    /// Distinct citing works of the whole database (no filter).
    pub n_all: usize,
    /// Per article: distinct citing works in the period, honouring the self-citation switch.
    article_counts: Vec<(String, String, usize)>,
}

impl Model {
    pub fn new(raws: &[Raw], ctx: Ctx, filter: Filter) -> Model {
        let all = build_works(raws, "", &ctx);
        let mut counts: BTreeMap<String, (String, usize)> = BTreeMap::new();
        for w in all.iter().filter(|w| in_period(w, &filter) && !(filter.exclude_self && w.kind.is_self())) {
            for (k, t) in &w.articles {
                counts.entry(k.clone()).or_insert_with(|| (t.clone(), 0)).1 += 1;
            }
        }
        let mut article_counts: Vec<(String, String, usize)> = counts.into_iter().map(|(k, (t, n))| (k, t, n)).collect();
        article_counts.sort_by(|a, b| b.2.cmp(&a.2).then(a.1.cmp(&b.1)).then(a.0.cmp(&b.0)));
        let n_all = all.len();
        let works = if filter.article.is_empty() { all } else { build_works(raws, &filter.article, &ctx) };
        Model { works, ctx, filter, n_all, article_counts }
    }

    /// Period + article filter, self-citations included.
    fn base(&self) -> Vec<&Work> {
        self.works.iter().filter(|w| in_period(w, &self.filter)).collect()
    }
    /// What every section except the split shows.
    fn shown(&self) -> Vec<&Work> {
        self.works
            .iter()
            .filter(|w| in_period(w, &self.filter) && !(self.filter.exclude_self && w.kind.is_self()))
            .collect()
    }
    /// Article filter and self-citation switch, no period (for "new this year").
    fn all_time(&self) -> Vec<&Work> {
        self.works.iter().filter(|w| !(self.filter.exclude_self && w.kind.is_self())).collect()
    }
}

// ------------------------------------------------------------------ shared shapes

/// Coverage of one field among the shown works.
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct Cov {
    /// Works the section looks at (the filtered count).
    pub n: usize,
    /// Works without the field.
    pub missing: usize,
    /// No work has the field and some were stored before the field existed: the next refresh
    /// fills it ("additional data needed" state).
    pub needs_data: bool,
}

fn cov(works: &[&Work], has: impl Fn(&Work) -> bool) -> Cov {
    let missing = works.iter().filter(|w| !has(w)).count();
    let any_has = works.len() > missing;
    let any_old = works.iter().any(|w| !w.enriched);
    Cov { n: works.len(), missing, needs_data: !works.is_empty() && !any_has && any_old }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Named {
    pub name: String,
    pub n: usize,
}

fn ranked(counts: HashMap<String, (String, usize)>) -> Vec<Named> {
    let mut v: Vec<Named> = counts.into_values().map(|(name, n)| Named { name, n }).collect();
    v.sort_by(|a, b| b.n.cmp(&a.n).then(a.name.cmp(&b.name)));
    v
}

fn count_by(works: &[&Work], f: impl Fn(&Work) -> String) -> Vec<Named> {
    let mut m: HashMap<String, (String, usize)> = HashMap::new();
    for w in works {
        let v = f(w);
        if !v.is_empty() {
            m.entry(v.to_lowercase()).or_insert_with(|| (v, 0)).1 += 1;
        }
    }
    ranked(m)
}

// ------------------------------------------------------------------ meta

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ArticleOpt {
    pub key: String,
    pub title: String,
    pub n: usize,
}

/// The filter bar: counts and the article dropdown.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Meta {
    /// The filtered count shown in the bar ("N atıf yapan yayın") and the denominator of every caption.
    pub n: usize,
    /// The same without the self-citation switch.
    pub n_with_self: usize,
    /// Distinct citing works stored, whatever the filter.
    pub n_all: usize,
    /// The user's articles that have citing works in the period, most first (period and
    /// self-citation switch applied, article filter not).
    pub articles: Vec<ArticleOpt>,
    pub orcid_set: bool,
    /// Whether the profile name can be used for the name fallback (two words or more).
    pub name_usable: bool,
    pub year: i32,
    /// Stored rows that still lack the 1.3.0 fields (reachable by the refresh).
    pub unenriched_rows: i64,
    /// Citing works known to be dated, per year: not needed; see the sections.
    pub total_rows: i64,
}

impl Model {
    pub fn meta(&self, unenriched_rows: i64, total_rows: i64) -> Meta {
        let base = self.base();
        Meta {
            n: self.shown().len(),
            n_with_self: base.len(),
            n_all: self.n_all,
            articles: self
                .article_counts
                .iter()
                .map(|(k, t, n)| ArticleOpt { key: k.clone(), title: t.clone(), n: *n })
                .collect(),
            orcid_set: !self.ctx.orcid.is_empty(),
            name_usable: self.ctx.name_usable(),
            year: self.ctx.year,
            unenriched_rows,
            total_rows,
        }
    }
}

// ------------------------------------------------------------------ entities (countries, institutions)

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EntityStat {
    /// Country code, or institution id (`I...`; `n:<name>` when OpenAlex gave no id).
    pub key: String,
    pub name: String,
    /// Country code of an institution (empty for a country entry).
    pub country: String,
    /// Citing works (a work counts once per entity).
    pub n: usize,
    /// First cited by a work of the current calendar year (see the module note).
    pub is_new: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct YearStat {
    pub year: i32,
    /// Distinct entities citing in that year.
    pub total: usize,
    /// Of them, the ones whose first citation is in that year.
    pub new: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct EntityReport {
    pub cov: Cov,
    /// Distinct entities in the period.
    pub k: usize,
    /// Of them, new this year.
    pub new_count: usize,
    /// All entities, most citing works first (the caller takes the top N).
    pub items: Vec<EntityStat>,
    /// The new ones, most citing works first.
    pub new_items: Vec<EntityStat>,
    /// One entry per year from the first to the last year of the period, years without citations included.
    pub years: Vec<YearStat>,
    /// The current calendar year.
    pub year: i32,
}

fn entity_report(
    all_time: &[&Work],
    period: &[&Work],
    year: i32,
    has: impl Fn(&Work) -> bool,
    ents: impl Fn(&Work) -> Vec<Inst>,
) -> EntityReport {
    let mut first: HashMap<String, i32> = HashMap::new();
    for w in all_time {
        for e in ents(w) {
            let f = first.entry(e.key).or_insert(w.year);
            *f = (*f).min(w.year);
        }
    }
    let mut counts: HashMap<String, (Inst, usize)> = HashMap::new();
    let mut per_year: BTreeMap<i32, BTreeSet<String>> = BTreeMap::new();
    for w in period {
        let list = ents(w);
        let mut seen = HashSet::new();
        for e in list {
            if !seen.insert(e.key.clone()) {
                continue;
            }
            per_year.entry(w.year).or_default().insert(e.key.clone());
            counts.entry(e.key.clone()).or_insert_with(|| (e, 0)).1 += 1;
        }
    }
    let stat = |(e, n): (Inst, usize)| {
        let is_new = first.get(&e.key).is_some_and(|y| *y == year);
        EntityStat { key: e.key, name: e.name, country: e.cc, n, is_new }
    };
    let mut items: Vec<EntityStat> = counts.into_values().map(stat).collect();
    items.sort_by(|a, b| b.n.cmp(&a.n).then(a.name.cmp(&b.name)).then(a.key.cmp(&b.key)));
    let new_items: Vec<EntityStat> = items.iter().filter(|e| e.is_new).cloned().collect();
    let mut years = Vec::new();
    if let (Some(lo), Some(hi)) = (per_year.keys().next().copied(), per_year.keys().next_back().copied()) {
        for y in lo..=hi {
            let set = per_year.get(&y);
            let total = set.map_or(0, BTreeSet::len);
            let new = set.map_or(0, |s| s.iter().filter(|k| first.get(*k) == Some(&y)).count());
            years.push(YearStat { year: y, total, new });
        }
    }
    EntityReport { cov: cov(period, has), k: items.len(), new_count: new_items.len(), items, new_items, years, year }
}

impl Model {
    pub fn countries(&self) -> EntityReport {
        let ents = |w: &Work| -> Vec<Inst> {
            w.countries.iter().map(|c| Inst { key: c.clone(), name: c.clone(), cc: String::new() }).collect()
        };
        entity_report(&self.all_time(), &self.shown(), self.ctx.year, |w| !w.countries.is_empty(), ents)
    }

    pub fn institutions(&self) -> EntityReport {
        entity_report(&self.all_time(), &self.shown(), self.ctx.year, |w| !w.insts.is_empty(), |w| w.insts.clone())
    }
}

// ------------------------------------------------------------------ venues and publishers

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct PublisherStat {
    pub name: String,
    pub n: usize,
    pub journal: usize,
    pub conference: usize,
    pub repository: usize,
    pub other: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct Venues {
    pub cov_venue: Cov,
    pub cov_publisher: Cov,
    pub cov_source_type: Cov,
    /// Distinct venues (case-insensitive).
    pub venue_k: usize,
    /// Top 30 venues.
    pub venues: Vec<Named>,
    pub publisher_k: usize,
    /// Top 10 publishers with the source types of their works.
    pub publishers: Vec<PublisherStat>,
    /// journal / conference / repository / other, over the works with a source type.
    pub source_types: Vec<Named>,
}

/// OpenAlex source types folded into the four the chart uses.
pub fn source_kind(t: &str) -> &'static str {
    match t.trim().to_ascii_lowercase().as_str() {
        "journal" => "journal",
        "conference" => "conference",
        "repository" => "repository",
        _ => "other",
    }
}

impl Model {
    pub fn venues(&self) -> Venues {
        let shown = self.shown();
        let venues = count_by(&shown, |w| w.venue.clone());
        let mut pubs: HashMap<String, PublisherStat> = HashMap::new();
        for w in shown.iter().filter(|w| !w.publisher.is_empty()) {
            let p = pubs
                .entry(w.publisher.to_lowercase())
                .or_insert_with(|| PublisherStat { name: w.publisher.clone(), ..PublisherStat::default() });
            p.n += 1;
            match source_kind(&w.source_type) {
                "journal" => p.journal += 1,
                "conference" => p.conference += 1,
                "repository" => p.repository += 1,
                _ => p.other += 1,
            }
        }
        let mut publishers: Vec<PublisherStat> = pubs.into_values().collect();
        publishers.sort_by(|a, b| b.n.cmp(&a.n).then(a.name.cmp(&b.name)));
        let publisher_k = publishers.len();
        publishers.truncate(10);
        let mut kinds: HashMap<String, (String, usize)> = HashMap::new();
        for w in shown.iter().filter(|w| !w.source_type.is_empty()) {
            let k = source_kind(&w.source_type);
            kinds.entry(k.to_string()).or_insert_with(|| (k.to_string(), 0)).1 += 1;
        }
        let venue_k = venues.len();
        Venues {
            cov_venue: cov(&shown, |w| !w.venue.is_empty()),
            cov_publisher: cov(&shown, |w| !w.publisher.is_empty()),
            cov_source_type: cov(&shown, |w| !w.source_type.is_empty()),
            venue_k,
            venues: venues.into_iter().take(30).collect(),
            publisher_k,
            publishers,
            source_types: ranked(kinds),
        }
    }
}

// ------------------------------------------------------------------ OJP

/// One venue of the OJP card.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct OjpVenue {
    pub name: String,
    pub n: usize,
    pub ojp: Option<u8>,
    pub ojp_state: crate::ojp::State,
    pub ojp_field: String,
    pub ojp_n: u32,
}

/// Citing works by the OJP of their journal.
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct OjpReport {
    /// Works the section looks at (the filtered count).
    pub n: usize,
    /// Works whose journal has an OJP.
    pub ranked: usize,
    /// Works per step: 0-9, 10-19, ... 90-100.
    pub hist: [usize; 10],
    /// Index of the fullest step (the higher one on a tie); `None` when nothing is ranked.
    pub mode: Option<usize>,
    /// In a journal that is known but not ranked (few counted articles, or no score in the data).
    pub none_few: usize,
    pub none_unscored: usize,
    /// In a journal missing from the data.
    pub none_absent: usize,
    /// Not in a journal.
    pub none_no_journal: usize,
    /// The 8 venues with most works among the works that are in a journal.
    pub venues: Vec<OjpVenue>,
}

impl Model {
    pub fn ojp(&self) -> OjpReport {
        use crate::ojp::State;
        let shown = self.shown();
        let mut r = OjpReport { n: shown.len(), ..OjpReport::default() };
        let mut venues: HashMap<String, OjpVenue> = HashMap::new();
        for w in &shown {
            match w.ojp.state {
                State::Ranked => {
                    if let Some(v) = w.ojp.ojp {
                        r.ranked += 1;
                        r.hist[crate::ojp::step(v)] += 1;
                    }
                }
                State::Few => r.none_few += 1,
                State::Unscored => r.none_unscored += 1,
                State::Absent => r.none_absent += 1,
                State::NoJournal => r.none_no_journal += 1,
            }
            if w.ojp.state != State::NoJournal && !w.venue.is_empty() {
                venues
                    .entry(w.venue.to_lowercase())
                    .and_modify(|v| {
                        v.n += 1;
                        // a venue seen first without a score takes the score of a later work of it
                        if v.ojp.is_none() && w.ojp.ojp.is_some() {
                            v.ojp = w.ojp.ojp;
                            v.ojp_state = w.ojp.state;
                            v.ojp_field = w.ojp.field.clone();
                            v.ojp_n = w.ojp.n;
                        }
                    })
                    .or_insert_with(|| OjpVenue {
                        name: w.venue.clone(),
                        n: 1,
                        ojp: w.ojp.ojp,
                        ojp_state: w.ojp.state,
                        ojp_field: w.ojp.field.clone(),
                        ojp_n: w.ojp.n,
                    });
            }
        }
        let top = r.hist.iter().copied().max().unwrap_or(0);
        r.mode = if top == 0 { None } else { r.hist.iter().rposition(|n| *n == top) };
        let mut v: Vec<OjpVenue> = venues.into_values().collect();
        v.sort_by(|a, b| b.n.cmp(&a.n).then(a.name.cmp(&b.name)));
        v.truncate(8);
        r.venues = v;
        r
    }
}

pub fn build_ojp(conn: &Connection, orcid: &str, year: i32, filter: Filter) -> Result<OjpReport> {
    Ok(model(conn, orcid, year, filter)?.ojp())
}

// ------------------------------------------------------------------ fields

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FieldNode {
    pub name: String,
    pub n: usize,
    /// The three most frequent topics of the field.
    pub topics: Vec<Named>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DomainNode {
    /// Empty when OpenAlex gave a field but no domain.
    pub name: String,
    pub n: usize,
    pub fields: Vec<FieldNode>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MatrixRow {
    pub key: String,
    pub title: String,
    /// Citing works of the article in the filtered set.
    pub n: usize,
    /// One count per column plus a last one for every other field.
    pub cells: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct Fields {
    /// Works without a field.
    pub cov: Cov,
    /// domain > field > top topics, largest first.
    pub domains: Vec<DomainNode>,
    /// All fields, most citing works first.
    pub fields: Vec<Named>,
    /// The top 6 fields (matrix columns; "other" is the implicit last one).
    pub matrix_columns: Vec<String>,
    /// The user's top 8 articles by citing works.
    pub matrix_rows: Vec<MatrixRow>,
}

/// Works of one field and the counts of its topics.
type FieldAcc = (usize, HashMap<String, (String, usize)>);

impl Model {
    pub fn fields(&self) -> Fields {
        let shown = self.shown();
        let with: Vec<&&Work> = shown.iter().filter(|w| !w.field.is_empty()).collect();
        let mut dom: BTreeMap<String, usize> = BTreeMap::new();
        let mut fld: BTreeMap<(String, String), FieldAcc> = BTreeMap::new();
        for w in &with {
            *dom.entry(w.domain.clone()).or_default() += 1;
            let e = fld.entry((w.domain.clone(), w.field.clone())).or_default();
            e.0 += 1;
            if !w.topic.is_empty() {
                e.1.entry(w.topic.to_lowercase()).or_insert_with(|| (w.topic.clone(), 0)).1 += 1;
            }
        }
        let mut domains: Vec<DomainNode> = dom
            .into_iter()
            .map(|(name, n)| {
                let mut fields: Vec<FieldNode> = fld
                    .iter()
                    .filter(|((d, _), _)| *d == name)
                    .map(|((_, f), (c, topics))| FieldNode {
                        name: f.clone(),
                        n: *c,
                        topics: ranked(topics.clone()).into_iter().take(3).collect(),
                    })
                    .collect();
                fields.sort_by(|a, b| b.n.cmp(&a.n).then(a.name.cmp(&b.name)));
                DomainNode { name, n, fields }
            })
            .collect();
        domains.sort_by(|a, b| b.n.cmp(&a.n).then(a.name.cmp(&b.name)));
        let fields = count_by(&shown, |w| w.field.clone());
        let columns: Vec<String> = fields.iter().take(6).map(|f| f.name.clone()).collect();
        let col_of = |f: &str| columns.iter().position(|c| c.eq_ignore_ascii_case(f)).unwrap_or(columns.len());
        // rows: top 8 articles of the filtered set (the same ranking as the dropdown, but over the shown works)
        let mut art: BTreeMap<&str, (&str, usize)> = BTreeMap::new();
        for w in &shown {
            for (k, t) in &w.articles {
                art.entry(k.as_str()).or_insert((t.as_str(), 0)).1 += 1;
            }
        }
        let mut order: Vec<(&str, &str, usize)> = art.into_iter().map(|(k, (t, n))| (k, t, n)).collect();
        order.sort_by(|a, b| b.2.cmp(&a.2).then(a.1.cmp(b.1)).then(a.0.cmp(b.0)));
        order.truncate(8);
        let mut matrix_rows: Vec<MatrixRow> = order
            .iter()
            .map(|(k, t, n)| MatrixRow { key: k.to_string(), title: t.to_string(), n: *n, cells: vec![0; columns.len() + 1] })
            .collect();
        for w in with {
            let c = col_of(&w.field);
            for (k, _) in &w.articles {
                if let Some(r) = matrix_rows.iter_mut().find(|r| r.key == *k) {
                    r.cells[c] += 1;
                }
            }
        }
        Fields { cov: cov(&shown, |w| !w.field.is_empty()), domains, fields, matrix_columns: columns, matrix_rows }
    }
}

// ------------------------------------------------------------------ types, language, open access

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct Types {
    pub cov_type: Cov,
    pub cov_language: Cov,
    pub cov_oa: Cov,
    /// OpenAlex type keys (`article`, `review`, `proceedings-article`, `book-chapter`, `dissertation`, `preprint`, ...).
    pub types: Vec<Named>,
    /// ISO 639-1 codes.
    pub languages: Vec<Named>,
    /// `gold`, `green`, `hybrid`, `bronze`, `diamond`, `closed`.
    pub oa: Vec<Named>,
}

impl Model {
    pub fn types(&self) -> Types {
        let shown = self.shown();
        Types {
            cov_type: cov(&shown, |w| !w.work_type.is_empty()),
            cov_language: cov(&shown, |w| !w.language.is_empty()),
            cov_oa: cov(&shown, |w| !w.oa.is_empty()),
            types: count_by(&shown, |w| w.work_type.clone()),
            languages: count_by(&shown, |w| w.language.clone()),
            oa: count_by(&shown, |w| w.oa.clone()),
        }
    }
}

// ------------------------------------------------------------------ who

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct Split {
    /// Works of the period and article filter; the split ignores the self-citation switch.
    pub total: usize,
    pub self_n: usize,
    /// Self-citations found by the ORCID iD / by the name fallback (they add up to `self_n`).
    pub by_orcid: usize,
    pub by_name: usize,
    pub coauthor: usize,
    pub external: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AuthorStat {
    pub name: String,
    pub orcid: String,
    /// Most frequent institution of the author across these works (empty when unknown).
    pub institution: String,
    pub n: usize,
    pub is_self: bool,
    pub is_coauthor: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct Who {
    pub split: Split,
    /// Over the works the filter bar counts (self-citation switch applied): `missing` = works with no
    /// author ORCID at all; `needs_data` = never fetched.
    pub cov_orcid: Cov,
    pub orcid_set: bool,
    pub name_usable: bool,
    /// The switch is on: the table has no row for the user.
    pub excluded_self: bool,
    /// Top 10 citing authors of the shown works.
    pub authors: Vec<AuthorStat>,
    /// Works that carry the author list used for the table.
    pub n_with_authors: usize,
}

impl Model {
    pub fn who(&self) -> Who {
        let base = self.base();
        let mut split = Split { total: base.len(), ..Split::default() };
        for w in &base {
            match w.kind {
                Kind::SelfOrcid => {
                    split.self_n += 1;
                    split.by_orcid += 1;
                }
                Kind::SelfName => {
                    split.self_n += 1;
                    split.by_name += 1;
                }
                Kind::Coauthor => split.coauthor += 1,
                Kind::External => split.external += 1,
            }
        }
        let shown = self.shown();
        struct Acc {
            name: String,
            orcid: String,
            n: usize,
            insts: HashMap<String, usize>,
        }
        let mut accs: HashMap<String, Acc> = HashMap::new();
        for w in &shown {
            let inst_name: HashMap<&str, &str> = w.insts.iter().map(|i| (i.key.as_str(), i.name.as_str())).collect();
            let mut seen = HashSet::new();
            for a in &w.authors {
                let key = if self.ctx.is_me(a).is_some() {
                    // ORCID- and name-matched self entries are one person.
                    "me".to_string()
                } else if !a.orcid.is_empty() {
                    format!("o:{}", a.orcid)
                } else {
                    let f = fold_name(&a.name);
                    if f.is_empty() {
                        continue;
                    }
                    format!("n:{f}")
                };
                if !seen.insert(key.clone()) {
                    continue;
                }
                let e = accs.entry(key).or_insert_with(|| Acc { name: a.name.clone(), orcid: a.orcid.clone(), n: 0, insts: HashMap::new() });
                e.n += 1;
                if e.name.is_empty() {
                    e.name = a.name.clone();
                }
                for i in &a.insts {
                    if let Some(n) = inst_name.get(i.as_str()).filter(|n| !n.is_empty()) {
                        *e.insts.entry(n.to_string()).or_default() += 1;
                    }
                }
            }
        }
        let mut authors: Vec<AuthorStat> = accs
            .into_values()
            .map(|a| {
                let probe = Author { name: a.name.clone(), orcid: a.orcid.clone(), insts: vec![] };
                let is_self = self.ctx.is_me(&probe).is_some();
                let is_coauthor = !is_self && author_key(&a.name).is_some_and(|k| self.ctx.coauthors.contains(&k));
                let mut insts: Vec<(String, usize)> = a.insts.into_iter().collect();
                insts.sort_by(|x, y| y.1.cmp(&x.1).then(x.0.cmp(&y.0)));
                AuthorStat {
                    name: a.name,
                    orcid: a.orcid,
                    institution: insts.into_iter().next().map(|x| x.0).unwrap_or_default(),
                    n: a.n,
                    is_self,
                    is_coauthor,
                }
            })
            .collect();
        authors.sort_by(|a, b| b.n.cmp(&a.n).then(a.name.cmp(&b.name)).then(a.orcid.cmp(&b.orcid)));
        authors.truncate(10);
        Who {
            split,
            cov_orcid: cov(&shown, |w| !w.orcids.is_empty()),
            orcid_set: !self.ctx.orcid.is_empty(),
            name_usable: self.ctx.name_usable(),
            excluded_self: self.filter.exclude_self,
            authors,
            n_with_authors: shown.iter().filter(|w| !w.authors.is_empty()).count(),
        }
    }
}

// ------------------------------------------------------------------ influential

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct InfluentialItem {
    pub citing_id: String,
    pub title: String,
    pub venue: String,
    pub year: i32,
    pub date: String,
    pub doi: String,
    pub url: String,
    pub cited_by: i64,
    /// The user's articles it cites (titles); the table shows the first.
    pub articles: Vec<String>,
    pub article_key: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct Influential {
    pub cov: Cov,
    pub threshold: i64,
    /// Works cited more than `threshold` times.
    pub over: usize,
    pub max: Option<i64>,
    /// Top 10 by the citing work's own citation count.
    pub items: Vec<InfluentialItem>,
}

impl Model {
    pub fn influential(&self) -> Influential {
        let shown = self.shown();
        let mut known: Vec<&&Work> = shown.iter().filter(|w| w.cited_by.is_some()).collect();
        known.sort_by(|a, b| {
            b.cited_by.cmp(&a.cited_by).then(b.start.cmp(&a.start)).then(a.id.cmp(&b.id))
        });
        let over = known.iter().filter(|w| w.cited_by.unwrap_or(0) > INFLUENTIAL_MIN).count();
        let max = known.first().and_then(|w| w.cited_by);
        let items = known
            .iter()
            .take(10)
            .map(|w| InfluentialItem {
                citing_id: w.id.clone(),
                title: w.title.clone(),
                venue: w.venue.clone(),
                year: w.year,
                date: w.date.clone(),
                doi: w.doi.clone(),
                url: w.url.clone(),
                cited_by: w.cited_by.unwrap_or(0),
                articles: w.articles.iter().map(|a| a.1.clone()).collect(),
                article_key: w.articles.first().map(|a| a.0.clone()).unwrap_or_default(),
            })
            .collect();
        Influential { cov: cov(&shown, |w| w.cited_by.is_some()), threshold: INFLUENTIAL_MIN, over, max, items }
    }
}

// ------------------------------------------------------------------ entry points

fn model(conn: &Connection, orcid: &str, year: i32, filter: Filter) -> Result<Model> {
    let ctx = Ctx::load(conn, orcid, year)?;
    Ok(Model::new(&load_raws(conn)?, ctx, filter))
}

pub fn build_meta(conn: &Connection, orcid: &str, year: i32, filter: Filter) -> Result<Meta> {
    let rows: i64 = conn.query_row("SELECT COUNT(*) FROM backfill_citations", [], |r| r.get(0))?;
    let old = db::unenriched_rows(conn)?;
    Ok(model(conn, orcid, year, filter)?.meta(old, rows))
}
pub fn build_countries(conn: &Connection, orcid: &str, year: i32, filter: Filter) -> Result<EntityReport> {
    Ok(model(conn, orcid, year, filter)?.countries())
}
pub fn build_institutions(conn: &Connection, orcid: &str, year: i32, filter: Filter) -> Result<EntityReport> {
    Ok(model(conn, orcid, year, filter)?.institutions())
}
pub fn build_venues(conn: &Connection, orcid: &str, year: i32, filter: Filter) -> Result<Venues> {
    Ok(model(conn, orcid, year, filter)?.venues())
}
pub fn build_fields(conn: &Connection, orcid: &str, year: i32, filter: Filter) -> Result<Fields> {
    Ok(model(conn, orcid, year, filter)?.fields())
}
pub fn build_types(conn: &Connection, orcid: &str, year: i32, filter: Filter) -> Result<Types> {
    Ok(model(conn, orcid, year, filter)?.types())
}
pub fn build_who(conn: &Connection, orcid: &str, year: i32, filter: Filter) -> Result<Who> {
    Ok(model(conn, orcid, year, filter)?.who())
}
pub fn build_influential(conn: &Connection, orcid: &str, year: i32, filter: Filter) -> Result<Influential> {
    Ok(model(conn, orcid, year, filter)?.influential())
}

/// Per citing work: (self-citation, co-author citation). For the list page and the Excel sheet.
pub fn flags(conn: &Connection, orcid: &str, year: i32) -> Result<HashMap<String, (bool, bool)>> {
    let ctx = Ctx::load(conn, orcid, year)?;
    Ok(build_works(&load_raws(conn)?, "", &ctx)
        .into_iter()
        .map(|w| (w.id, (w.kind.is_self(), w.kind == Kind::Coauthor)))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: &str = "0000-0002-1825-0097";

    fn ctx() -> Ctx {
        Ctx::new(ME, "Ada Example", ["AB Example, C Sample, D Partner, ..."], 2026)
    }

    fn auth(json: &str) -> String {
        json.to_string()
    }

    fn raw(article: &str, id: &str, date: &str) -> Raw {
        Raw {
            article_key: article.into(),
            article_title: format!("Article {article}"),
            citing_id: id.into(),
            title: format!("Citing {id}"),
            date: date.into(),
            precision: if date.len() == 10 { "day".into() } else if date.len() == 7 { "month".into() } else { "year".into() },
            enriched: true,
            ..Raw::default()
        }
    }

    fn filter() -> Filter {
        Filter::default()
    }

    /// Eight invented citing works covering every aggregate.
    fn sample() -> Vec<Raw> {
        let mut v = Vec::new();
        // W1: 2024, TR+US, institution I1 (TR) and I2 (US), self by ORCID
        v.push(Raw {
            countries: "TR,US".into(),
            institutions: r#"[{"i":"I1","n":"Alpha University","c":"TR"},{"i":"I2","n":"Beta Institute","c":"US"}]"#.into(),
            authorships: auth(&format!(r#"[{{"n":"A. Example","o":"{ME}","i":["I1"]}},{{"n":"Bo Sample","o":"","i":["I2"]}}]"#)),
            author_orcids: ME.into(),
            venue: "Journal of Invented Results".into(),
            publisher: "Invented Press".into(),
            source_type: "journal".into(),
            topic: "Remote sensing".into(),
            topic_field: "Earth Sciences".into(),
            topic_domain: "Physical Sciences".into(),
            work_type: "article".into(),
            language: "en".into(),
            oa_status: "gold".into(),
            cited_by: Some(150),
            ..raw("k1", "W1", "2024-03-05")
        });
        // W2: 2025, TR, institution I1, external, cites k1 and k2
        v.push(Raw {
            countries: "TR".into(),
            institutions: r#"[{"i":"I1","n":"Alpha University","c":"TR"}]"#.into(),
            authorships: auth(r#"[{"n":"Cem Other","o":"0000-0001-0000-0001","i":["I1"]}]"#),
            author_orcids: "0000-0001-0000-0001".into(),
            venue: "journal of invented results".into(),
            publisher: "Invented Press".into(),
            source_type: "journal".into(),
            topic: "Remote sensing".into(),
            topic_field: "Earth Sciences".into(),
            topic_domain: "Physical Sciences".into(),
            work_type: "article".into(),
            language: "tr".into(),
            oa_status: "green".into(),
            cited_by: Some(20),
            ..raw("k1", "W2", "2025-06-01")
        });
        v.push(Raw { ..raw("k2", "W2", "2025-06-01") }); // second article of the same work, not enriched row
        // W3: 2026, DE, new institution I3 (DE), co-author C. Sample, proceedings
        v.push(Raw {
            countries: "DE".into(),
            institutions: r#"[{"i":"I3","n":"Gamma Hochschule","c":"DE"}]"#.into(),
            authorships: auth(r#"[{"n":"Cara Sample","o":"","i":["I3"]}]"#),
            venue: "Conf on Invention".into(),
            publisher: "Gamma Verlag".into(),
            source_type: "conference".into(),
            topic: "Machine learning".into(),
            topic_field: "Computer Science".into(),
            topic_domain: "Physical Sciences".into(),
            work_type: "proceedings-article".into(),
            language: "de".into(),
            oa_status: "closed".into(),
            cited_by: Some(3),
            ..raw("k2", "W3", "2026-02-10")
        });
        // W4: 2026, FR, no institution id (name only), repository preprint
        v.push(Raw {
            countries: "FR".into(),
            institutions: r#"[{"i":"n:delta lab","n":"Delta Lab","c":"FR"}]"#.into(),
            authorships: auth(r#"[{"n":"Dan Nobody","o":"","i":["n:delta lab"]}]"#),
            venue: "Repo".into(),
            publisher: "Gamma Verlag".into(),
            source_type: "repository".into(),
            topic: "Machine learning".into(),
            topic_field: "Computer Science".into(),
            topic_domain: "Physical Sciences".into(),
            work_type: "preprint".into(),
            language: "en".into(),
            oa_status: "green".into(),
            cited_by: Some(0),
            ..raw("k1", "W4", "2026-04")
        });
        // W5: 2026, nothing known, enriched but empty, self by name only (no orcids), dated by year
        v.push(Raw {
            authorships: auth(r#"[{"n":"Ada Example","o":"","i":[]}]"#),
            ..raw("k2", "W5", "2026")
        });
        // W6: old row, never enriched, plain author string with the user's name
        v.push(Raw { enriched: false, authors: "Eve Foreign; Ada Example".into(), countries: "TR".into(), venue: "Old Journal".into(), ..raw("k1", "W6", "2023-01-01") });
        // W7: a namesake with another ORCID: external, not self
        v.push(Raw {
            authorships: auth(r#"[{"n":"Ada Example","o":"0000-0003-0000-0003","i":[]}]"#),
            author_orcids: "0000-0003-0000-0003".into(),
            work_type: "review".into(),
            language: "en".into(),
            oa_status: "bronze".into(),
            cited_by: Some(250),
            ..raw("k1", "W7", "2025-09-09")
        });
        v
    }

    fn model_of(f: Filter) -> Model {
        Model::new(&sample(), ctx(), f)
    }

    #[test]
    fn names_are_folded_and_compared() {
        assert_eq!(fold_name("İsmail Çağrı-Öztürk"), "ismail cagri ozturk");
        assert!(same_person("Ada Example", "Example, Ada") || same_person("Ada Example", "Example Ada"));
        assert!(same_person("Ada B. Example", "Ada Example"));
        assert!(!same_person("A. Example", "Ada Example"), "initials alone never match");
        assert!(!same_person("Ada", "Ada Example"));
        assert_eq!(author_key("AB Example").as_deref(), Some("example|a"));
        assert_eq!(author_key("Ada Example"), author_key("A. Example"));
        assert_eq!(author_key("Example"), None);
        let c = ctx();
        assert!(c.coauthors.contains("sample|c") && c.coauthors.contains("partner|d"));
        assert!(!c.coauthors.contains("example|a"), "the user is not their own co-author");
    }

    #[test]
    fn works_are_distinct_and_classified() {
        let m = model_of(filter());
        assert_eq!(m.n_all, 7);
        let kind = |id: &str| m.works.iter().find(|w| w.id == id).expect("work").kind;
        assert_eq!(kind("W1"), Kind::SelfOrcid);
        assert_eq!(kind("W2"), Kind::External);
        assert_eq!(kind("W3"), Kind::Coauthor);
        assert_eq!(kind("W5"), Kind::SelfName);
        assert_eq!(kind("W6"), Kind::SelfName, "name fallback also works for rows without authorships");
        assert_eq!(kind("W7"), Kind::External, "a namesake with a different ORCID is not a self-citation");
        let w2 = m.works.iter().find(|w| w.id == "W2").expect("w2");
        assert_eq!(w2.articles.len(), 2, "one work, two cited articles");
        assert_eq!(w2.cited_by, Some(20), "the enriched row is used");
    }

    #[test]
    fn period_and_article_filters_apply() {
        let mut f = filter();
        f.from = "2025-01-01".into();
        f.to = "2025-12-31".into();
        let m = model_of(f);
        assert_eq!(m.meta(0, 0).n, 2);
        let mut f = filter();
        f.article = "k2".into();
        let m = model_of(f);
        let ids: Vec<&str> = m.base().iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids.len(), 3, "W2, W3, W5 cite k2: {ids:?}");
        assert!(m.works.iter().all(|w| w.articles.len() == 1 && w.articles[0].0 == "k2"));
        // the dropdown counts ignore the article filter
        let counts: Vec<(String, usize)> = m.meta(0, 0).articles.iter().map(|a| (a.key.clone(), a.n)).collect();
        assert_eq!(counts, vec![("k1".to_string(), 5), ("k2".to_string(), 3)]);
    }

    #[test]
    fn exclude_self_removes_self_works_but_not_from_the_split() {
        let mut f = filter();
        f.exclude_self = true;
        let m = model_of(f);
        assert_eq!(m.shown().len(), 7 - 3, "W1, W5, W6 are self-citations");
        let who = m.who();
        assert_eq!(who.split.total, 7);
        assert_eq!(who.split.self_n, 3);
        assert!(who.excluded_self);
        assert!(!who.authors.iter().any(|a| a.is_self), "the user's own row is gone");
        let meta = m.meta(0, 0);
        assert_eq!((meta.n, meta.n_with_self), (4, 7));
    }

    #[test]
    fn countries_count_once_per_country_and_find_new_ones() {
        let r = model_of(filter()).countries();
        let n = |c: &str| r.items.iter().find(|e| e.key == c).map(|e| e.n);
        assert_eq!(n("TR"), Some(3), "W1, W2, W6");
        assert_eq!(n("US"), Some(1));
        assert_eq!(r.k, 4);
        assert_eq!(r.items[0].key, "TR");
        // new in 2026: DE and FR (first seen in 2026); TR and US are older
        let new: Vec<&str> = r.new_items.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(new, vec!["DE", "FR"]);
        assert!(r.items.iter().find(|e| e.key == "DE").expect("de").is_new);
        // works without a country: W3.. no: W5, W7 (W4 FR, W3 DE)
        assert_eq!((r.cov.n, r.cov.missing), (7, 2));
        assert!(!r.cov.needs_data);
        // years are contiguous, with the first-time countries counted per year
        let y = |year: i32| r.years.iter().find(|s| s.year == year).expect("year").clone();
        assert_eq!(r.years.first().map(|s| s.year), Some(2023));
        assert_eq!((y(2023).total, y(2023).new), (1, 1)); // TR
        assert_eq!((y(2024).total, y(2024).new), (2, 1)); // TR, US: US new
        assert_eq!((y(2025).total, y(2025).new), (1, 0));
        assert_eq!((y(2026).total, y(2026).new), (2, 2));
    }

    #[test]
    fn new_this_year_ignores_the_period_but_the_period_limits_the_counts() {
        let mut f = filter();
        f.from = "2025-01-01".into();
        let r = model_of(f).countries();
        // US first appeared in 2024: not new even though the period starts in 2025 and 2026 has no US
        assert!(r.items.iter().all(|e| e.key != "US"));
        assert_eq!(r.k, 3);
        assert!(r.new_items.iter().all(|e| e.key == "DE" || e.key == "FR"));
    }

    #[test]
    fn institutions_are_ranked_with_new_ones_marked() {
        let r = model_of(filter()).institutions();
        assert_eq!(r.items[0].name, "Alpha University");
        assert_eq!((r.items[0].n, r.items[0].country.as_str()), (2, "TR"));
        let new: Vec<&str> = r.new_items.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(new, vec!["Delta Lab", "Gamma Hochschule"], "ties by name; the id-less one is kept by name");
        assert_eq!(r.new_count, 2);
        assert_eq!(r.k, 4);
        assert_eq!(r.cov.n, 7);
        assert!(r.years.iter().any(|y| y.year == 2026 && y.new == 2));
    }

    #[test]
    fn venues_publishers_and_source_types() {
        let r = model_of(filter()).venues();
        assert_eq!(r.venues[0].name.to_lowercase(), "journal of invented results");
        assert_eq!(r.venues[0].n, 2, "case-insensitive");
        assert_eq!(r.venue_k, 4);
        assert_eq!((r.cov_venue.n, r.cov_venue.missing), (7, 2));
        let p = r.publishers.iter().find(|p| p.name == "Invented Press").expect("press");
        assert_eq!((p.name.as_str(), p.n, p.journal), ("Invented Press", 2, 2));
        let g = r.publishers.iter().find(|p| p.name == "Gamma Verlag").expect("gamma");
        assert_eq!((g.n, g.conference, g.repository), (2, 1, 1));
        let kinds: HashMap<&str, usize> = r.source_types.iter().map(|k| (k.name.as_str(), k.n)).collect();
        assert_eq!(kinds.get("journal"), Some(&2));
        assert_eq!(kinds.get("conference"), Some(&1));
        assert_eq!(kinds.get("repository"), Some(&1));
        assert_eq!(r.cov_source_type.missing, 3);
        assert_eq!(source_kind("ebook platform"), "other");
    }

    #[test]
    fn fields_hierarchy_and_matrix() {
        let r = model_of(filter()).fields();
        assert_eq!(r.cov.missing, 3, "W5, W6, W7 have no field");
        assert_eq!(r.domains.len(), 1);
        let d = &r.domains[0];
        assert_eq!((d.name.as_str(), d.n), ("Physical Sciences", 4));
        assert_eq!(d.fields[0].name, "Computer Science", "ties by name");
        assert_eq!(d.fields[0].topics[0].name, "Machine learning");
        assert_eq!(r.fields.len(), 2);
        assert_eq!(r.matrix_columns, vec!["Computer Science", "Earth Sciences"]);
        let row = |k: &str| r.matrix_rows.iter().find(|x| x.key == k).expect("row").clone();
        // k1 cites: W1 (Earth), W2 (Earth), W4 (CS), W6 (none), W7 (none)
        assert_eq!(row("k1").cells, vec![1, 2, 0]);
        // k2 cites: W2 (Earth), W3 (CS), W5 (none)
        assert_eq!(row("k2").cells, vec![1, 1, 0]);
        assert_eq!(row("k1").n, 5);
        // more than 6 fields: the rest goes to the last column
        let mut raws = Vec::new();
        for i in 0..8 {
            raws.push(Raw { topic_field: format!("Field {i}"), topic_domain: "D".into(), ..raw("k1", &format!("X{i}"), "2025-01-01") });
        }
        let r = Model::new(&raws, ctx(), filter()).fields();
        assert_eq!(r.matrix_columns.len(), 6);
        assert_eq!(r.matrix_rows[0].cells.len(), 7);
        assert_eq!(r.matrix_rows[0].cells[6], 2);
    }

    #[test]
    fn types_languages_and_open_access() {
        let r = model_of(filter()).types();
        let get = |v: &Vec<Named>, k: &str| v.iter().find(|x| x.name == k).map(|x| x.n);
        assert_eq!(get(&r.types, "article"), Some(2));
        assert_eq!(get(&r.types, "review"), Some(1));
        assert_eq!(r.cov_type.missing, 2, "W5 and W6");
        assert_eq!(get(&r.languages, "en"), Some(3));
        assert_eq!(get(&r.oa, "green"), Some(2));
        assert_eq!(get(&r.oa, "closed"), Some(1));
        assert_eq!(r.cov_oa.missing, 2);
        assert_eq!(r.types[0].name, "article");
    }

    #[test]
    fn who_split_authors_and_orcid_coverage() {
        let who = model_of(filter()).who();
        let s = &who.split;
        assert_eq!((s.total, s.self_n, s.by_orcid, s.by_name, s.coauthor, s.external), (7, 3, 1, 2, 1, 3));
        assert!(who.orcid_set && who.name_usable);
        // works without any author ORCID: W3, W4, W5, W6 (W1, W2, W7 have one)
        assert_eq!((who.cov_orcid.n, who.cov_orcid.missing), (7, 4));
        // the ORCID-matched and name-matched entries of the user are one row (W1 + W5 + W6)
        let mine: Vec<_> = who.authors.iter().filter(|a| a.is_self).collect();
        assert_eq!(mine.len(), 1, "one self row");
        assert_eq!(mine[0].n, 3);
        let cara = who.authors.iter().find(|a| a.name == "Cara Sample").expect("cara");
        assert!(cara.is_coauthor && !cara.is_self);
        assert_eq!(cara.institution, "Gamma Hochschule");
        assert!(who.authors.len() <= 10);
        // sorted by count, then name
        assert!(who.authors.windows(2).all(|w| w[0].n >= w[1].n));
    }

    #[test]
    fn self_detection_without_orcid_uses_names_only() {
        let c = Ctx::new("", "Ada Example", [""], 2026);
        let m = Model::new(&sample(), c, filter());
        let who = m.who();
        assert!(!who.orcid_set);
        assert_eq!(who.split.by_orcid, 0);
        // W1 has the author "A. Example": initials only do not match, so W1 is external here
        let kind = |id: &str| m.works.iter().find(|w| w.id == id).expect("work").kind;
        assert_eq!(kind("W1"), Kind::External);
        assert_eq!(kind("W5"), Kind::SelfName);
        assert_eq!(kind("W7"), Kind::SelfName, "without a user ORCID a different ORCID cannot rule the name out");
    }

    #[test]
    fn influential_works_are_ranked() {
        let r = model_of(filter()).influential();
        assert_eq!(r.threshold, 100);
        assert_eq!(r.over, 2, "W7 (250) and W1 (150)");
        assert_eq!(r.max, Some(250));
        assert_eq!(r.items[0].citing_id, "W7");
        assert_eq!(r.items[1].citing_id, "W1");
        assert_eq!(r.items[0].cited_by, 250);
        assert_eq!(r.items.len(), 5, "only works with a count");
        assert_eq!(r.cov.missing, 2, "W5 and W6 have no count");
        assert_eq!(r.items[0].articles, vec!["Article k1".to_string()]);
        let w2 = r.items.iter().find(|i| i.citing_id == "W2").expect("w2");
        assert_eq!(w2.articles.len(), 2);
    }

    #[test]
    fn not_fetched_yet_is_reported_per_section() {
        // only old rows: nothing enriched
        let raws: Vec<Raw> = (0..3).map(|i| Raw { enriched: false, countries: "TR".into(), ..raw("k1", &format!("O{i}"), "2024-05-05") }).collect();
        let m = Model::new(&raws, ctx(), filter());
        assert!(m.fields().cov.needs_data);
        assert!(m.types().cov_type.needs_data);
        assert!(m.influential().cov.needs_data);
        assert!(m.institutions().cov.needs_data);
        assert!(!m.countries().cov.needs_data, "countries were stored before 1.3.0");
        assert!(m.who().cov_orcid.needs_data);
        // enriched rows that simply lack the data are not "not fetched"
        let fresh: Vec<Raw> = (0..3).map(|i| raw("k1", &format!("N{i}"), "2024-05-05")).collect();
        let m = Model::new(&fresh, ctx(), filter());
        assert!(!m.fields().cov.needs_data);
        assert_eq!(m.fields().cov.missing, 3);
    }

    #[test]
    fn empty_database_gives_empty_aggregates() {
        let m = Model::new(&[], ctx(), filter());
        assert_eq!(m.meta(0, 0).n, 0);
        assert!(m.countries().items.is_empty() && m.countries().years.is_empty());
        assert!(m.institutions().items.is_empty());
        assert!(m.venues().venues.is_empty());
        assert!(m.fields().domains.is_empty());
        assert!(m.types().types.is_empty());
        assert_eq!(m.who().split.total, 0);
        assert!(m.influential().items.is_empty());
    }

    #[test]
    fn dates_without_precision_are_skipped_and_months_start_on_the_first() {
        assert_eq!(start_of("2024-03", "month").as_deref(), Some("2024-03-01"));
        assert_eq!(start_of("2024", "year").as_deref(), Some("2024-01-01"));
        assert_eq!(start_of("2024-13", "month"), None);
        assert_eq!(start_of("x", "day"), None);
    }

    // ---- storage round trip and migration (database level)

    use crate::db::{self, CitingRecord};

    fn temp_db(tag: &str) -> (std::path::PathBuf, Connection) {
        let dir = std::env::temp_dir().join(format!("sct-citers-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let c = db::open(&db::db_path_in(&dir)).expect("open");
        (dir, c)
    }

    #[test]
    fn migration_is_idempotent_and_marks_old_rows_unenriched() {
        let dir = std::env::temp_dir().join(format!("sct-citers-mig-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let path = db::db_path_in(&dir);
        {
            // a 1.2.0 database: the table without the new columns, one stored row
            let c = Connection::open(&path).expect("raw");
            c.execute_batch(
                "CREATE TABLE backfill_citations(article_key TEXT NOT NULL, source TEXT NOT NULL, citing_id TEXT NOT NULL,
                   citing_title TEXT NOT NULL DEFAULT '', citing_date TEXT NOT NULL, date_precision TEXT NOT NULL,
                   venue TEXT NOT NULL DEFAULT '', doi TEXT NOT NULL DEFAULT '', url TEXT NOT NULL DEFAULT '',
                   authors TEXT NOT NULL DEFAULT '', countries TEXT NOT NULL DEFAULT '', issn_list TEXT NOT NULL DEFAULT '',
                   PRIMARY KEY(article_key, citing_id));
                 INSERT INTO backfill_citations(article_key, source, citing_id, citing_title, citing_date, date_precision, countries)
                   VALUES ('k1','openalex','W1','Old','2024-01-02','day','TR');",
            )
            .expect("old");
        }
        let conn = db::open(&path).expect("first open migrates");
        let cols = |c: &Connection| -> Vec<String> {
            let mut st = c.prepare("PRAGMA table_info(backfill_citations)").expect("p");
            st.query_map([], |r| r.get::<_, String>(1)).expect("q").map(|x| x.expect("c")).collect()
        };
        let have = cols(&conn);
        for want in [
            "institutions", "authorships", "author_orcids", "topic", "topic_field", "topic_domain", "work_type",
            "language", "oa_status", "publisher", "source_type", "cited_by", "enriched", "source_id",
        ] {
            assert!(have.iter().any(|c| c == want), "{want} missing");
        }
        db::migrate(&conn).expect("second run is a no-op");
        assert_eq!(cols(&conn).len(), have.len());
        let (enriched, cited): (i64, Option<i64>) = conn
            .query_row("SELECT enriched, cited_by FROM backfill_citations WHERE citing_id='W1'", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .expect("row");
        assert_eq!((enriched, cited), (0, None), "old rows are not enriched and keep their data");
        let rows = load_raws(&conn).expect("raws");
        assert_eq!(rows[0].countries, "TR");
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn source_id_is_stored_and_old_rows_fall_back_to_issns() {
        let (dir, mut conn) = temp_db("ojpid");
        snapshot(&mut conn);
        // one old row (no source id, ISSN stored) and one refreshed row (source id, no ISSN)
        conn.execute(
            "INSERT INTO backfill_citations(article_key, source, citing_id, citing_title, citing_date, date_precision, issn_list, source_type, enriched, venue)
             VALUES ('k1','openalex','WOLD','Old','2024-01-02','day','11111111','journal',1,'Invented Journal A')",
            [],
        )
        .expect("old row");
        let rec = CitingRecord {
            citing_id: "WNEW".into(),
            citing_title: "New".into(),
            citing_date: "2025-05-05".into(),
            date_precision: "day".into(),
            source_type: "journal".into(),
            venue: "Invented Journal B".into(),
            source_id: "S2".into(),
            ..CitingRecord::default()
        };
        let before = db::citing_works(&conn).expect("rows");
        assert_eq!(before[0].source_id, "", "an old row has no source id");
        db::refresh_citing(&mut conn, "k1", "openalex", &[rec]).expect("refresh");
        let rows = db::citing_works(&conn).expect("rows");
        assert_eq!(rows[0].source_id, "S2");
        // no data file lookup is faked here: the real index is only asked for ids that cannot exist
        assert_eq!(rows[0].ojp_state, crate::ojp::State::Absent, "unknown id, no ISSN, a journal source");
        assert_eq!(db::unenriched_rows(&conn).expect("n"), 0, "a missing source id does not trigger a refresh");
        let raws = load_raws(&conn).expect("raws");
        assert_eq!(raws[0].source_id, "S2");
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ojp_report_counts_states_and_steps() {
        use crate::ojp::{Ojp, State};
        let ctx = Ctx::new("", "", [], 2026);
        let mut ws = build_works(&[raw("k1", "A", "2025-01-02"), raw("k1", "B", "2025-02-02"), raw("k1", "C", "2025-03-02"), raw("k1", "D", "2025-04-02"), raw("k1", "E", "2025-05-02"), raw("k1", "F", "2025-06-02")], "", &ctx);
        let set = |w: &mut Work, venue: &str, o: Ojp| {
            w.venue = venue.into();
            w.ojp = o;
        };
        let r = |v: u8| Ojp { ojp: Some(v), state: State::Ranked, field: "Medicine".into(), n: 100 };
        ws.sort_by(|a, b| a.id.cmp(&b.id));
        set(&mut ws[0], "Journal One", r(0));
        set(&mut ws[1], "journal one", r(0));
        set(&mut ws[2], "Journal Two", r(100));
        set(&mut ws[3], "Journal Three", Ojp { state: State::Few, ..Ojp::default() });
        set(&mut ws[4], "Journal Four", Ojp { state: State::Absent, ..Ojp::default() });
        set(&mut ws[5], "A Book", Ojp::default());
        let m = Model { works: ws, ctx, filter: Filter::default(), n_all: 6, article_counts: vec![] };
        let o = m.ojp();
        assert_eq!((o.n, o.ranked), (6, 3));
        assert_eq!((o.hist[0], o.hist[9]), (2, 1), "0 is step 0, 100 is step 9");
        assert_eq!(o.mode, Some(0));
        assert_eq!((o.none_few, o.none_unscored, o.none_absent, o.none_no_journal), (1, 0, 1, 1));
        assert_eq!(o.venues.len(), 4, "the book is not a venue");
        assert_eq!((o.venues[0].name.as_str(), o.venues[0].n, o.venues[0].ojp), ("Journal One", 2, Some(0)));
        assert!(OjpReport::default().mode.is_none());
    }

    fn snapshot(conn: &mut Connection) {
        conn.execute_batch(
            "INSERT INTO snapshots(fetched_at, profile_id, name, cit_all, cit_since, h_all, h_since, i10_all, i10_since, since_year)
               VALUES ('2026-11-01T00:00:00Z','P1','Ada Example',5,5,1,1,0,0,2021);
             INSERT INTO articles(article_key, title, authors, first_seen, last_seen)
               VALUES ('k1','Article k1','AB Example, C Sample','2026-11-01T00:00:00Z','2026-11-01T00:00:00Z');
             INSERT INTO article_citations(snapshot_id, article_key, citations) VALUES (1,'k1',5);
             INSERT INTO article_matches(article_key, source, external_id, matched_at) VALUES ('k1','openalex','W100','2026-11-01T00:00:00Z');",
        )
        .expect("seed");
    }

    #[test]
    fn stored_records_fill_the_new_columns_and_refresh_marks_them_enriched() {
        let (dir, mut conn) = temp_db("store");
        snapshot(&mut conn);
        // an old row first
        conn.execute(
            "INSERT INTO backfill_citations(article_key, source, citing_id, citing_title, citing_date, date_precision)
             VALUES ('k1','openalex','WOLD','Old','2024-01-02','day')",
            [],
        )
        .expect("old row");
        assert_eq!(db::unenriched_rows(&conn).expect("n"), 1);
        assert_eq!(db::refresh_targets(&conn).expect("t").len(), 1);
        let rec = CitingRecord {
            citing_id: "WNEW".into(),
            citing_title: "New".into(),
            citing_date: "2025-05-05".into(),
            date_precision: "day".into(),
            institutions: r#"[{"i":"I1","n":"Alpha University","c":"TR"}]"#.into(),
            authorships: format!(r#"[{{"n":"Ada Example","o":"{ME}","i":["I1"]}}]"#),
            author_orcids: ME.into(),
            topic: "Remote sensing".into(),
            topic_field: "Earth Sciences".into(),
            topic_domain: "Physical Sciences".into(),
            work_type: "article".into(),
            language: "en".into(),
            oa_status: "gold".into(),
            publisher: "Invented Press".into(),
            source_type: "journal".into(),
            cited_by: Some(42),
            ..CitingRecord::default()
        };
        // the refresh replaces the article's rows: the old row goes, the new one is enriched
        db::refresh_citing(&mut conn, "k1", "openalex", &[rec]).expect("refresh");
        assert_eq!(db::unenriched_rows(&conn).expect("n"), 0);
        let rows = db::citing_works(&conn).expect("rows");
        assert_eq!(rows.len(), 1);
        let r = &rows[0];
        assert_eq!((r.work_type.as_str(), r.language.as_str(), r.oa_status.as_str()), ("article", "en", "gold"));
        assert_eq!((r.topic_field.as_str(), r.topic_domain.as_str(), r.cited_by), ("Earth Sciences", "Physical Sciences", Some(42)));
        assert!(r.enriched);
        // and the aggregates see it
        let fl = flags(&conn, ME, 2026).expect("flags");
        assert_eq!(fl.get("WNEW"), Some(&(true, false)));
        let who = build_who(&conn, ME, 2026, filter()).expect("who");
        assert_eq!(who.split.by_orcid, 1);
        let meta = build_meta(&conn, ME, 2026, filter()).expect("meta");
        assert_eq!((meta.n, meta.n_all, meta.unenriched_rows, meta.total_rows), (1, 1, 0, 1));
        assert_eq!(build_influential(&conn, ME, 2026, filter()).expect("i").max, Some(42));
        assert_eq!(build_countries(&conn, ME, 2026, filter()).expect("c").cov.missing, 1);
        let ctx = Ctx::load(&conn, ME, 2026).expect("ctx");
        assert_eq!(ctx.name, "Ada Example");
        assert!(ctx.coauthors.contains("sample|c"));
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unenriched_rows_of_unreachable_articles_do_not_count() {
        let (dir, mut conn) = temp_db("unreach");
        snapshot(&mut conn);
        conn.execute(
            "INSERT INTO backfill_citations(article_key, source, citing_id, citing_title, citing_date, date_precision)
             VALUES ('gone','openalex','W1','x','2024-01-02','day')",
            [],
        )
        .expect("row");
        assert_eq!(db::unenriched_rows(&conn).expect("n"), 0, "no snapshot/match for that article");
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_article_filter_does_not_pick_a_stale_row_of_the_work() {
        let c = ctx();
        let all = build_works(&sample(), "", &c);
        let w2_all = all.iter().find(|w| w.id == "W2").expect("W2");
        assert!(w2_all.enriched);
        // W2 is also stored for k2 with a row that was never enriched: the data still comes from the k1 row
        let k2 = build_works(&sample(), "k2", &c);
        let w2 = k2.iter().find(|w| w.id == "W2").expect("W2 cites k2");
        assert!(w2.enriched);
        assert_eq!(w2.work_type, w2_all.work_type);
        assert_eq!(w2.kind, w2_all.kind);
        assert_eq!(w2.articles, vec![("k2".to_string(), "Article k2".to_string())]);
    }

    #[test]
    fn a_month_or_year_dated_work_counts_when_its_span_touches_the_period() {
        let f = Filter { from: "2026-10-26".into(), to: String::new(), ..Filter::default() };
        let ok = |d: &str| {
            let w = build_works(&[raw("k1", "X", d)], "", &ctx());
            in_period(&w[0], &f)
        };
        assert!(ok("2026-10"));
        assert!(ok("2026"));
        assert!(!ok("2026-09"));
        assert!(!ok("2026-10-25"));
        assert!(ok("2026-10-26"));
        assert_eq!(end_of("2026-12-01", "month"), "2027-01-01");
    }
}
