//! OJP (Open Journal Prestige): a 0-100 within-field percentile score for journals.
//!
//! The data (`data/ojp/ojp-2024.csv.gz`, built by `data/ojp/build_ojp.py` from the pinned Opindx
//! release) is compiled into the app and parsed once, on first use. Nothing is downloaded.
//! A citing work is matched by its OpenAlex source id first, then by the ISSNs stored with it.

use std::collections::HashMap;
use std::io::Read;
use std::sync::OnceLock;

use anyhow::{Context, Result};
use serde::Serialize;

const DATA_GZ: &[u8] = include_bytes!("../data/ojp/ojp-2024.csv.gz");
/// Attribution and statement of changes (CC BY 4.0), shown on the Credits page.
pub const NOTICE: &str = include_str!("../data/ojp/NOTICE.txt");

pub const EDITION: i32 = 2024;
/// Year of the citations the edition counts.
pub const CITE_YEAR: i32 = 2024;
/// Publication window of the counted articles.
pub const WINDOW_FROM: i32 = 2019;
pub const WINDOW_TO: i32 = 2023;
/// Counted articles a journal needs for a score.
pub const MIN_PUBS: u32 = 20;
/// Ranked journals a field needs for scores.
pub const MIN_FIELD: u32 = 20;
pub const DATASET_VERSION: &str = "2026-09-v5";
pub const DATASET_DOI: &str = "10.5281/zenodo.23599869";

/// Why a citing work has (or has not) an OJP.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// The journal is in the data and has a score.
    Ranked,
    /// In the data, but fewer than [`MIN_PUBS`] counted articles.
    Few,
    /// In the data, but no score (ANS undefined, field unknown, or too small a field).
    Unscored,
    /// The work has an ISSN or a journal source, and no journal in the data matches.
    Absent,
    /// Not in a journal (book, preprint, thesis, unknown).
    #[default]
    NoJournal,
}

/// The OJP facts of one citing work.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Ojp {
    /// 0..=100, `None` when the journal has no score.
    pub ojp: Option<u8>,
    pub state: State,
    /// Field as in the data (English OpenAlex field); empty when unknown.
    pub field: String,
    /// Ranked journals of that field (0 when the journal has no score).
    pub n: u32,
}

/// Constants every UI string uses.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Meta {
    pub edition: i32,
    pub cite_year: i32,
    pub window_from: i32,
    pub window_to: i32,
    pub min_pubs: u32,
    pub min_field: u32,
    /// Journals that have a score.
    pub journals: usize,
    /// Fields with scores.
    pub fields: usize,
    pub dataset_version: &'static str,
    pub dataset_doi: &'static str,
}

#[derive(Debug, Clone)]
struct Entry {
    ojp: Option<u8>,
    field: String,
    n: u32,
    /// Kept for the test that recomputes every score from it.
    #[allow(dead_code)]
    ans: Option<f64>,
    reason: State,
}

/// The parsed data file.
pub struct Index {
    entries: Vec<Entry>,
    by_id: HashMap<String, usize>,
    by_issn: HashMap<String, usize>,
    ranked: usize,
    fields: usize,
}

/// (Used by the tests: the data file is built by `build_ojp.py`.)
/// `100 * (less + equal/2) / n`, rounded half up, in integer arithmetic (the formula of
/// `build_ojp.py`). `less` = journals of the field with a lower score, `equal` = journals with the same
/// score (the journal itself included), `n` = ranked journals of the field.
#[cfg(test)]
pub fn percentile(less: u32, equal: u32, n: u32) -> u8 {
    let (l, e, n) = (u64::from(less), u64::from(equal), u64::from(n.max(1)));
    ((100 * (2 * l + e) + n) / (2 * n)).min(100) as u8
}

/// Step of the colour scale: 0..=9 (0-9, 10-19, ... 90-100).
pub fn step(score: u8) -> usize {
    usize::from(score / 10).min(9)
}

fn short_source_id(id: &str) -> &str {
    id.trim().rsplit('/').next().unwrap_or("")
}

impl Index {
    /// Parses the CSV (plain text, not gzipped).
    pub fn parse<R: Read>(r: R) -> Result<Index> {
        let mut rdr = csv::ReaderBuilder::new().flexible(true).from_reader(r);
        let head = rdr.headers()?.clone();
        let col = |name: &str| head.iter().position(|h| h == name).with_context(|| format!("column {name} missing"));
        let (c_id, c_l, c_issns, c_field, c_ans, c_ojp, c_n, c_reason) = (
            col("openalex_id")?,
            col("issn_l")?,
            col("issns")?,
            col("field")?,
            col("ans")?,
            col("ojp")?,
            col("n")?,
            col("reason")?,
        );
        let mut idx = Index { entries: Vec::new(), by_id: HashMap::new(), by_issn: HashMap::new(), ranked: 0, fields: 0 };
        let mut fields = std::collections::HashSet::new();
        for rec in rdr.records() {
            let rec = rec?;
            let get = |i: usize| rec.get(i).unwrap_or("").trim();
            let ojp = get(c_ojp).parse::<u8>().ok().filter(|v| *v <= 100);
            let reason = if get(c_reason) == "few" { State::Few } else { State::Unscored };
            let entry = Entry { ojp, field: get(c_field).to_string(), n: get(c_n).parse().unwrap_or(0), ans: get(c_ans).parse().ok(), reason };
            if ojp.is_some() {
                idx.ranked += 1;
                fields.insert(entry.field.clone());
            }
            let i = idx.entries.len();
            let scored = ojp.is_some();
            idx.entries.push(entry);
            let id = short_source_id(get(c_id));
            if !id.is_empty() {
                idx.by_id.entry(id.to_string()).or_insert(i);
            }
            for raw in std::iter::once(get(c_l)).chain(get(c_issns).split(';')) {
                let Some(issn) = crate::backfill::normalize_issn(raw) else { continue };
                match idx.by_issn.get(&issn) {
                    // an ISSN shared by two journals goes to the scored one
                    Some(old) if idx.entries[*old].ojp.is_some() || !scored => {}
                    _ => {
                        idx.by_issn.insert(issn, i);
                    }
                }
            }
        }
        idx.fields = fields.len();
        Ok(idx)
    }

    fn entry_ojp(e: &Entry) -> Ojp {
        match e.ojp {
            Some(v) => Ojp { ojp: Some(v), state: State::Ranked, field: e.field.clone(), n: e.n },
            None => Ojp { ojp: None, state: e.reason, field: e.field.clone(), n: 0 },
        }
    }

    /// OJP of a citing work. `source_id` is the OpenAlex source id (`S...`, empty for rows stored
    /// before 1.4.0), `issn_list` the stored comma-separated normalised ISSNs (ISSN-L first),
    /// `source_type` the OpenAlex source type.
    pub fn lookup(&self, source_id: &str, issn_list: &str, source_type: &str) -> Ojp {
        let id = short_source_id(source_id);
        if !id.is_empty() {
            if let Some(i) = self.by_id.get(id) {
                return Self::entry_ojp(&self.entries[*i]);
            }
        }
        let mut has_issn = false;
        for raw in issn_list.split(',') {
            let Some(issn) = crate::backfill::normalize_issn(raw) else { continue };
            has_issn = true;
            if let Some(i) = self.by_issn.get(&issn) {
                return Self::entry_ojp(&self.entries[*i]);
            }
        }
        let state = if has_issn || source_type.trim().eq_ignore_ascii_case("journal") { State::Absent } else { State::NoJournal };
        Ojp { state, ..Ojp::default() }
    }

    pub fn meta(&self) -> Meta {
        Meta {
            edition: EDITION,
            cite_year: CITE_YEAR,
            window_from: WINDOW_FROM,
            window_to: WINDOW_TO,
            min_pubs: MIN_PUBS,
            min_field: MIN_FIELD,
            journals: self.ranked,
            fields: self.fields,
            dataset_version: DATASET_VERSION,
            dataset_doi: DATASET_DOI,
        }
    }
}

/// The bundled data, parsed once.
pub fn index() -> &'static Index {
    static IDX: OnceLock<Index> = OnceLock::new();
    IDX.get_or_init(|| {
        let gz = flate2::read::GzDecoder::new(DATA_GZ);
        Index::parse(gz).expect("the bundled OJP data is valid")
    })
}

pub fn lookup(source_id: &str, issn_list: &str, source_type: &str) -> Ojp {
    index().lookup(source_id, issn_list, source_type)
}

pub fn meta() -> Meta {
    index().meta()
}

/// Turkish names of the 25 fields (the English name is the one in the data); same wording as the UI.
const FIELDS_TR: [(&str, &str); 25] = [
    ("Agricultural and Biological Sciences", "Tarım ve Biyoloji Bilimleri"),
    ("Arts and Humanities", "Sanat ve Beşeri Bilimler"),
    ("Biochemistry, Genetics and Molecular Biology", "Biyokimya, Genetik ve Moleküler Biyoloji"),
    ("Business, Management and Accounting", "İşletme, Yönetim ve Muhasebe"),
    ("Chemistry", "Kimya"),
    ("Computer Science", "Bilgisayar Bilimi"),
    ("Decision Sciences", "Karar Bilimleri"),
    ("Dentistry", "Diş Hekimliği"),
    ("Earth and Planetary Sciences", "Yer ve Gezegen Bilimleri"),
    ("Economics, Econometrics and Finance", "Ekonomi, Ekonometri ve Finans"),
    ("Energy", "Enerji"),
    ("Engineering", "Mühendislik"),
    ("Environmental Science", "Çevre Bilimi"),
    ("Health Professions", "Sağlık Meslekleri"),
    ("Immunology and Microbiology", "İmmünoloji ve Mikrobiyoloji"),
    ("Materials Science", "Malzeme Bilimi"),
    ("Mathematics", "Matematik"),
    ("Medicine", "Tıp"),
    ("Neuroscience", "Nörobilim"),
    ("Nursing", "Hemşirelik"),
    ("Pharmacology, Toxicology and Pharmaceutics", "Farmakoloji, Toksikoloji ve Eczacılık"),
    ("Physics and Astronomy", "Fizik ve Astronomi"),
    ("Psychology", "Psikoloji"),
    ("Social Sciences", "Sosyal Bilimler"),
    ("Veterinary", "Veterinerlik"),
];

/// Field name in the language of the export (unknown names stay as they are).
pub fn field_name(lang: &str, name: &str) -> String {
    if crate::i18n::normalize_lang(lang) == crate::i18n::TR {
        if let Some((_, t)) = FIELDS_TR.iter().find(|(e, _)| *e == name) {
            return (*t).to_string();
        }
    }
    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "openalex_id,issn_l,issns,title,field,ans,ojp,n,edition,variant,reason\n";

    fn fx() -> Index {
        let rows = [
            "S1,1111-1111,1111-1111;2222-2222,Invented Journal A,Medicine,2.0,100,3,2024,oa_raw,",
            "S2,3333-3333,3333-3333,Invented Journal B,Medicine,1.0,50,3,2024,oa_raw,",
            "S3,4444-4444,4444-4444;5555-555X,Invented Journal C,Medicine,0.5,17,3,2024,oa_raw,",
            "S4,6666-6666,6666-6666,Invented Tiny,Medicine,0.1,,,2024,oa_raw,few",
            "S5,,7777-7777,Invented NoScore,,,,,2024,oa_raw,unscored",
        ];
        Index::parse(format!("{HEAD}{}\n", rows.join("\n")).as_bytes()).expect("fixture")
    }

    #[test]
    fn percentile_edge_cases() {
        // one journal alone in a field: its own mid-rank, 50
        assert_eq!(percentile(0, 1, 1), 50);
        // all equal: 50 for everybody
        assert_eq!(percentile(0, 20, 20), 50);
        // lowest and highest of a field of 20 distinct values (2.5 and 97.5 round half up)
        assert_eq!(percentile(0, 1, 20), 3);
        assert_eq!(percentile(19, 1, 20), 98);
        // two-way tie at the bottom
        assert_eq!(percentile(0, 2, 20), 5);
        // never above 100, even with nonsense input; n = 0 does not divide by zero
        assert_eq!(percentile(50, 50, 20), 100);
        assert_eq!(percentile(0, 0, 0), 0);
        assert_eq!((step(0), step(9), step(10), step(76), step(99), step(100)), (0, 0, 1, 7, 9, 9));
    }

    #[test]
    fn id_match_comes_first_and_issns_are_the_fallback() {
        let i = fx();
        let by_id = i.lookup("https://openalex.org/S2", "11111111", "journal");
        assert_eq!((by_id.ojp, by_id.state, by_id.field.as_str(), by_id.n), (Some(50), State::Ranked, "Medicine", 3));
        // no id (old row): ISSNs in stored order; a later ISSN of the same journal also matches
        assert_eq!(i.lookup("", "99999999,22222222", "journal").ojp, Some(100));
        // the check character X is case-insensitive
        assert_eq!(i.lookup("", "5555555x", "").ojp, Some(17));
        // an unknown id falls back to the ISSN; formatted ISSNs are read too
        assert_eq!(i.lookup("S404", "33333333", "journal").ojp, Some(50));
        assert_eq!(i.lookup("", "3333-3333", "").ojp, Some(50));
    }

    #[test]
    fn five_states() {
        let i = fx();
        assert_eq!(i.lookup("S1", "", "").state, State::Ranked);
        let few = i.lookup("S4", "", "journal");
        assert_eq!((few.state, few.ojp, few.n), (State::Few, None, 0));
        let un = i.lookup("", "77777777", "journal");
        assert_eq!((un.state, un.ojp), (State::Unscored, None));
        assert_eq!(i.lookup("S404", "00000000", "journal").state, State::Absent);
        assert_eq!(i.lookup("", "", "journal").state, State::Absent, "journal source without ISSN");
        assert_eq!(i.lookup("", "", "").state, State::NoJournal);
        assert_eq!(i.lookup("S404", "", "repository").state, State::NoJournal);
        assert_eq!(i.lookup("", "garbage", "conference").state, State::NoJournal);
    }

    #[test]
    fn shared_issn_goes_to_the_scored_journal() {
        let csv = format!(
            "{HEAD}S1,1111-1111,1111-1111,Invented Unscored,,,,,2024,oa_raw,few\nS2,2222-2222,2222-2222;1111-1111,Invented Scored,Energy,1.0,60,25,2024,oa_raw,\n"
        );
        let i = Index::parse(csv.as_bytes()).expect("csv");
        assert_eq!(i.lookup("", "11111111", "").ojp, Some(60));
        assert_eq!((i.meta().journals, i.meta().fields), (1, 1));
    }

    #[test]
    fn missing_column_is_an_error() {
        assert!(Index::parse("openalex_id,title\nS1,x\n".as_bytes()).is_err());
    }

    #[test]
    fn bundled_data_matches_its_constants_and_the_formula() {
        let idx = index();
        let m = idx.meta();
        assert_eq!((m.journals, m.fields), (85_948, 25), "update the README and NOTICE with a new build");
        assert_eq!(idx.entries.len(), 111_087);
        // every score is the percentile of its ANS inside its field
        let mut by_field: HashMap<&str, Vec<f64>> = HashMap::new();
        for e in idx.entries.iter().filter(|e| e.ojp.is_some()) {
            by_field.entry(e.field.as_str()).or_default().push(e.ans.expect("ranked rows have an ANS"));
        }
        for v in by_field.values_mut() {
            v.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
        }
        for e in idx.entries.iter().filter(|e| e.ojp.is_some()) {
            let v = &by_field[e.field.as_str()];
            let ans = e.ans.expect("ans");
            let less = v.partition_point(|x| *x < ans) as u32;
            let le = v.partition_point(|x| *x <= ans) as u32;
            assert_eq!(e.n as usize, v.len(), "{}", e.field);
            assert!(e.n >= MIN_FIELD);
            assert_eq!(e.ojp, Some(percentile(less, le - less, e.n)), "{}", e.field);
        }
        assert!(idx.entries.iter().all(|e| e.ojp.is_some() || e.n == 0));
    }

    #[test]
    fn field_names_are_localised_and_cover_every_field() {
        assert_eq!(field_name("tr", "Medicine"), "Tıp");
        assert_eq!(field_name("en", "Medicine"), "Medicine");
        assert_eq!(field_name("tr", "Something New"), "Something New");
        let mut seen = std::collections::HashSet::new();
        for e in index().entries.iter().filter(|e| e.ojp.is_some()) {
            if seen.insert(e.field.clone()) {
                assert_ne!(field_name("tr", &e.field), e.field, "no Turkish name for {}", e.field);
            }
        }
    }

    #[test]
    fn notice_carries_the_attribution() {
        for s in [DATASET_DOI, DATASET_VERSION, "CC BY 4.0", "Weitzel", "did not review or endorse"] {
            assert!(NOTICE.contains(s), "{s}");
        }
    }
}
