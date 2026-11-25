//! Excel export: one .xlsx with the summary history, the latest article table, the long-format
//! article history, the citing works and the yearly counts. Sheet names and headers follow the
//! UI language; dates are real Excel dates; header rows are bold and filled, frozen and filtered.

use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Local, NaiveDate, Utc};
use rusqlite::Connection;
use rust_xlsxwriter::{Color, ExcelDateTime, Format, FormatAlign, FormatBorder, Workbook, Worksheet};

use crate::{citers, db, i18n};

const HEADER_FILL: u32 = 0x00D9E6F5;

fn header_format() -> Format {
    Format::new()
        .set_bold()
        .set_background_color(Color::RGB(HEADER_FILL))
        .set_border_bottom(FormatBorder::Thin)
        .set_align(FormatAlign::Left)
}

/// A UTC timestamp as local wall-clock time, the way the Excel user reads it.
fn local_excel(ts: &str) -> Option<ExcelDateTime> {
    let t: DateTime<Utc> = db::parse_ts(ts)?;
    let local = t.with_timezone(&Local).naive_local();
    ExcelDateTime::parse_from_str(&local.format("%Y-%m-%dT%H:%M:%S").to_string()).ok()
}

/// Date of a citing work: the first day of the month / year when the source gives no more.
/// Returns the Excel date and the number format that shows only what is known.
fn citing_excel(date: &str, precision: &str) -> Option<(ExcelDateTime, &'static str)> {
    let (full, fmt) = match precision {
        "day" => (date.to_string(), "yyyy-mm-dd"),
        "month" => (format!("{date}-01"), "yyyy-mm"),
        _ => (format!("{date}-01-01"), "yyyy"),
    };
    NaiveDate::parse_from_str(&full, "%Y-%m-%d").ok()?;
    ExcelDateTime::parse_from_str(&full).ok().map(|d| (d, fmt))
}

/// The "who cites you" cells of one citing work, columns 10..=22 of the citing sheet (column 19, the
/// citing work's own citation count, is a number written by the caller and is empty here).
/// Unknown values stay empty; institutions and ORCID iDs are separated by `; `.
fn citing_extra(c: &db::CitingRow, is_self: bool, is_co: bool, yes: &str, no: &str) -> Vec<String> {
    let inst: Vec<String> = serde_json::from_str::<Vec<serde_json::Value>>(&c.institutions)
        .unwrap_or_default()
        .iter()
        .filter_map(|i| i.get("n").and_then(|n| n.as_str()).filter(|n| !n.is_empty()).map(str::to_string))
        .collect();
    let list = |s: &str| s.split(',').map(str::trim).filter(|x| !x.is_empty()).collect::<Vec<_>>().join("; ");
    let flag = |b: bool| if c.enriched || b { (if b { yes } else { no }).to_string() } else { String::new() };
    vec![
        c.work_type.clone(),
        c.language.clone(),
        c.oa_status.clone(),
        c.topic_domain.clone(),
        c.topic_field.clone(),
        c.topic.clone(),
        inst.join("; "),
        c.publisher.clone(),
        c.source_type.clone(),
        String::new(),
        flag(is_self),
        flag(is_co),
        list(&c.author_orcids),
    ]
}

/// Country codes as `TR; US`, the same separator as the other list cells.
fn countries_list(s: &str) -> String {
    s.split(',').map(str::trim).filter(|x| !x.is_empty()).collect::<Vec<_>>().join("; ")
}

fn write_headers(ws: &mut Worksheet, headers: &[&str], widths: &[f64], rows: u32) -> Result<()> {
    let hf = header_format();
    for (c, h) in headers.iter().enumerate() {
        ws.write_string_with_format(0, c as u16, *h, &hf)?;
        ws.set_column_width(c as u16, widths.get(c).copied().unwrap_or(14.0))?;
    }
    ws.set_freeze_panes(1, 0)?;
    ws.autofilter(0, 0, rows.max(1), headers.len().saturating_sub(1) as u16)?;
    Ok(())
}

/// Writes the workbook to `path`.
/// `orcid` is the user's ORCID iD (settings), used to flag self-citations.
pub fn write_xlsx(conn: &Connection, path: &Path, lang: &str, orcid: &str) -> Result<()> {
    let tr = |k: &str| i18n::tr(lang, k);
    let mut wb = Workbook::new();
    let dt_fmt = Format::new().set_num_format("yyyy-mm-dd hh:mm");

    // ---- summary: the snapshot history
    {
        let ws = wb.add_worksheet();
        ws.set_name(tr("xl.sheet.summary"))?;
        let headers: Vec<&str> = (0..10).map(|i| tr(&format!("csv.s.{i}"))).collect();
        let mut st = conn.prepare(
            "SELECT fetched_at, profile_id, name, cit_all, cit_since, h_all, h_since, i10_all, i10_since, since_year
             FROM snapshots ORDER BY fetched_at, id",
        )?;
        let mut rows = st.query([])?;
        let mut n = 0u32;
        while let Some(r) = rows.next()? {
            n += 1;
            let at: String = r.get(0)?;
            match local_excel(&at) {
                Some(d) => ws.write_datetime_with_format(n, 0, &d, &dt_fmt)?,
                None => ws.write_string(n, 0, &at)?,
            };
            ws.write_string(n, 1, r.get::<_, String>(1)?)?;
            ws.write_string(n, 2, r.get::<_, String>(2)?)?;
            for c in 3..10usize {
                ws.write_number(n, c as u16, r.get::<_, i64>(c)? as f64)?;
            }
        }
        write_headers(ws, &headers, &[18.0, 18.0, 26.0, 14.0, 20.0, 12.0, 20.0, 12.0, 20.0, 12.0], n)?;
    }

    // ---- articles: latest snapshot with today / 7 / 30 day changes
    {
        let ws = wb.add_worksheet();
        ws.set_name(tr("xl.sheet.articles"))?;
        let headers: Vec<&str> = (0..8).map(|i| tr(&format!("xl.a.{i}"))).collect();
        let rows = db::articles_overview(conn, db::local_midnight_utc(Local::now()), Utc::now())?;
        for (i, a) in rows.iter().enumerate() {
            let n = i as u32 + 1;
            ws.write_string(n, 0, &a.title)?;
            if let Some(y) = a.year {
                ws.write_number(n, 1, y as f64)?;
            }
            ws.write_string(n, 2, &a.venue)?;
            ws.write_number(n, 3, a.citations as f64)?;
            ws.write_number(n, 4, a.today as f64)?;
            ws.write_number(n, 5, a.d7 as f64)?;
            ws.write_number(n, 6, a.d30 as f64)?;
            ws.write_string(n, 7, &a.url)?;
        }
        write_headers(ws, &headers, &[70.0, 8.0, 34.0, 10.0, 10.0, 10.0, 10.0, 40.0], rows.len() as u32)?;
    }

    // ---- article history (long format)
    {
        let ws = wb.add_worksheet();
        ws.set_name(tr("xl.sheet.history"))?;
        let headers: Vec<&str> = (0..4).map(|i| tr(&format!("csv.a.{i}"))).collect();
        let mut st = conn.prepare(
            "SELECT s.fetched_at, c.article_key, a.title, c.citations
             FROM article_citations c
             JOIN snapshots s ON s.id=c.snapshot_id
             JOIN articles a ON a.article_key=c.article_key
             ORDER BY s.fetched_at, s.id, a.title",
        )?;
        let mut rows = st.query([])?;
        let mut n = 0u32;
        while let Some(r) = rows.next()? {
            n += 1;
            let at: String = r.get(0)?;
            match local_excel(&at) {
                Some(d) => ws.write_datetime_with_format(n, 0, &d, &dt_fmt)?,
                None => ws.write_string(n, 0, &at)?,
            };
            ws.write_string(n, 1, r.get::<_, String>(1)?)?;
            ws.write_string(n, 2, r.get::<_, String>(2)?)?;
            ws.write_number(n, 3, r.get::<_, i64>(3)? as f64)?;
        }
        write_headers(ws, &headers, &[18.0, 16.0, 70.0, 10.0], n)?;
    }

    // ---- citing works
    {
        let ws = wb.add_worksheet();
        ws.set_name(tr("xl.sheet.citing"))?;
        let rows = db::citing_works(conn)?;
        let flags = citers::flags(conn, orcid, Local::now().year())?;
        let headers: Vec<&str> = (0..23).map(|i| tr(&format!("xl.c.{i}"))).collect();
        for (i, c) in rows.iter().enumerate() {
            let n = i as u32 + 1;
            match citing_excel(&c.date, &c.precision) {
                Some((d, fmt)) => ws.write_datetime_with_format(n, 0, &d, &Format::new().set_num_format(fmt).set_align(FormatAlign::Left))?,
                None => ws.write_string(n, 0, &c.date)?,
            };
            let prec_key = match c.precision.as_str() {
                "day" => "xl.prec.day",
                "month" => "xl.prec.month",
                _ => "xl.prec.year",
            };
            ws.write_string(n, 1, tr(prec_key))?;
            ws.write_string(n, 2, &c.title)?;
            ws.write_string(n, 3, &c.venue)?;
            ws.write_string(n, 4, &c.doi)?;
            ws.write_string(n, 5, &c.url)?;
            ws.write_string(n, 6, &c.authors)?;
            ws.write_string(n, 7, countries_list(&c.countries))?;
            ws.write_string(n, 8, &c.article_title)?;
            ws.write_string(n, 9, &c.citing_id)?;
            // who cites you; empty cells where OpenAlex (or an unrefreshed row) has nothing
            let (is_self, is_co) = flags.get(&c.citing_id).copied().unwrap_or((false, false));
            for (col, v) in citing_extra(c, is_self, is_co, tr("xl.yes"), tr("xl.no")).iter().enumerate() {
                if !v.is_empty() {
                    ws.write_string(n, 10 + col as u16, v)?;
                }
            }
            if let Some(cb) = c.cited_by {
                ws.write_number(n, 19, cb as f64)?;
            }
        }
        write_headers(
            ws,
            &headers,
            &[
                12.0, 14.0, 70.0, 34.0, 28.0, 40.0, 40.0, 12.0, 60.0, 22.0, 18.0, 8.0, 16.0, 20.0, 24.0, 30.0, 40.0, 26.0,
                14.0, 10.0, 10.0, 12.0, 40.0,
            ],
            rows.len() as u32,
        )?;
    }

    // ---- yearly: Scholar's histogram next to the citing works by year
    {
        let ws = wb.add_worksheet();
        ws.set_name(tr("xl.sheet.yearly"))?;
        let headers: Vec<&str> = (0..3).map(|i| tr(&format!("xl.y.{i}"))).collect();
        let scholar: std::collections::BTreeMap<i32, i64> = match db::latest_snapshot(conn)? {
            Some(s) => db::yearly_for(conn, s.id)?.into_iter().collect(),
            None => Default::default(),
        };
        let citing: std::collections::BTreeMap<i32, i64> = db::history_series(conn, "year")?
            .points
            .into_iter()
            .filter_map(|(y, n)| y.parse::<i32>().ok().map(|y| (y, n)))
            .collect();
        let mut years: Vec<i32> = scholar.keys().chain(citing.keys()).copied().collect();
        years.sort_unstable();
        years.dedup();
        for (i, y) in years.iter().enumerate() {
            let n = i as u32 + 1;
            ws.write_number(n, 0, *y as f64)?;
            if let Some(v) = scholar.get(y) {
                ws.write_number(n, 1, *v as f64)?;
            }
            if let Some(v) = citing.get(y) {
                ws.write_number(n, 2, *v as f64)?;
            }
        }
        write_headers(ws, &headers, &[8.0, 20.0, 40.0], years.len() as u32)?;
    }

    wb.save(path).with_context(|| format!("could not write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn citing_dates_keep_their_precision() {
        let (d, f) = citing_excel("2024-03-05", "day").expect("day");
        assert_eq!(f, "yyyy-mm-dd");
        let _ = d;
        assert_eq!(citing_excel("2024-03", "month").expect("month").1, "yyyy-mm");
        assert_eq!(citing_excel("2024", "year").expect("year").1, "yyyy");
        assert!(citing_excel("2024-13", "month").is_none());
        assert!(citing_excel("garbage", "day").is_none());
    }

    #[test]
    fn who_cites_cells_are_empty_when_unknown() {
        let mut c = db::CitingRow::default();
        let e = citing_extra(&c, false, false, "Yes", "No");
        assert_eq!(e.len(), 13);
        assert!(e.iter().all(String::is_empty), "an unrefreshed row has no placeholders: {e:?}");
        c.enriched = true;
        c.work_type = "article".into();
        c.institutions = r#"[{"i":"I1","n":"Alpha University","c":"TR"},{"i":"I2","n":"Beta Institute","c":"US"}]"#.into();
        c.countries = "TR,US".into();
        c.author_orcids = "0000-0002-1825-0097,0000-0001-0000-0001".into();
        let e = citing_extra(&c, true, false, "Yes", "No");
        assert_eq!(e[0], "article");
        assert_eq!(e[6], "Alpha University; Beta Institute");
        assert_eq!(countries_list(&c.countries), "TR; US");
        assert_eq!((e[10].as_str(), e[11].as_str()), ("Yes", "No"));
        assert_eq!(e[12], "0000-0002-1825-0097; 0000-0001-0000-0001");
    }

    #[test]
    fn sheet_names_fit_excel_limits() {
        for lang in [i18n::TR, i18n::EN] {
            for k in ["xl.sheet.summary", "xl.sheet.articles", "xl.sheet.history", "xl.sheet.citing", "xl.sheet.yearly"] {
                let name = i18n::tr(lang, k);
                assert!(name.chars().count() <= 31, "{name}");
                assert!(!name.contains(['[', ']', ':', '*', '?', '/', '\\']), "{name}");
            }
        }
    }

    #[test]
    fn workbook_is_written_for_an_empty_database() {
        let conn = Connection::open_in_memory().expect("db");
        conn.execute_batch("PRAGMA foreign_keys=OFF;").expect("pragma");
        let dir = std::env::temp_dir().join(format!("sct-xlsx-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("out.xlsx");
        let db_path = dir.join("t.db");
        let c = db::open(&db_path).expect("open");
        write_xlsx(&c, &path, "tr", "").expect("xlsx");
        assert!(std::fs::metadata(&path).map(|m| m.len() > 1000).unwrap_or(false));
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
