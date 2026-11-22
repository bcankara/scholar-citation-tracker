//! Backend strings: tray, notifications, status/error messages, CSV headers.
//!
//! One table of `(key, Turkish, English)`. Placeholders are `{0}`, `{1}`, ...

pub const TR: &str = "tr";
pub const EN: &str = "en";

/// `"tr"` or `"en"` (anything else becomes English).
pub fn normalize_lang(lang: &str) -> &'static str {
    if lang.trim().eq_ignore_ascii_case(TR) {
        TR
    } else {
        EN
    }
}

/// Language from an OS locale tag such as `tr-TR`, `en-US` or `tr_TR`.
pub fn lang_from_locale(locale: Option<&str>) -> &'static str {
    match locale {
        Some(l) if l.trim().to_ascii_lowercase().starts_with("tr") => TR,
        _ => EN,
    }
}

/// Language of the Windows UI (Turkish when the OS language starts with "tr").
pub fn system_lang() -> &'static str {
    lang_from_locale(sys_locale::get_locale().as_deref())
}

pub const PRODUCT_TR: &str = "Scholar Atıf Takip";
pub const PRODUCT_EN: &str = "Scholar Citation Tracker";

pub fn product_name(lang: &str) -> &'static str {
    if normalize_lang(lang) == TR {
        PRODUCT_TR
    } else {
        PRODUCT_EN
    }
}

pub const TABLE: &[(&str, &str, &str)] = &[
    // tray
    ("tray.open", "Paneli aç", "Open dashboard"),
    ("tray.refresh", "Şimdi güncelle", "Refresh now"),
    ("tray.folder", "Veri klasörünü aç", "Open data folder"),
    ("tray.quit", "Çıkış", "Quit"),
    ("tray.tooltip_total", "{0} — Toplam atıf: {1}", "{0} — Total citations: {1}"),
    // notifications
    ("notif.new_citations", "+{0} yeni atıf (toplam {1})", "+{0} new citations (total {1})"),
    ("notif.new_article_title", "Yeni makale eklendi (toplam {0})", "New article added (total {0})"),
    ("notif.new_article_line", "Yeni makale: {0}", "New article: {0}"),
    (
        "notif.health",
        "Scholar 2 gündür okunamıyor — Kayıt günlüğüne bakın",
        "Scholar has been unreadable for 2 days — see the Fetch log",
    ),
    // scheduler / fetch log
    ("log.ok", "Toplam atıf: {0}, {1} makale", "Total citations: {0}, {1} articles"),
    ("log.store_crashed", "Kayıt hatası: arka plan görevi çöktü", "Storage error: background task crashed"),
    ("log.store_failed", "Kayıt hatası: {0}", "Storage error: {0}"),
    // fetch errors
    ("err.network", "Ağ hatası", "Network error"),
    ("err.blocked", "Google erişimi engelledi", "Google blocked access"),
    ("err.parse", "Sayfa çözümlenemedi", "Page could not be parsed"),
    ("err.d.no_stats_table", "#gsc_rsb_st tablosu bulunamadı", "#gsc_rsb_st table not found"),
    ("err.d.no_since", "'Since YYYY' başlığı bulunamadı", "'Since YYYY' header not found"),
    (
        "err.d.missing_rows",
        "Atıf / h-indeksi / i10-indeksi satırları eksik",
        "Citations / h-index / i10-index rows are missing",
    ),
    (
        "err.d.no_articles",
        "makale listesi okunamadı — Scholar sayfa yapısı değişmiş olabilir",
        "article list could not be read — Scholar's page layout may have changed",
    ),
    (
        "err.d.articles_dropped",
        "makale sayısı beklenmedik biçimde düştü ({0}) — Scholar sayfa yapısı değişmiş olabilir",
        "article count dropped unexpectedly ({0}) — Scholar's page layout may have changed",
    ),
    ("err.d.redirect", "yönlendirme: {0}", "redirected to: {0}"),
    ("err.d.marker", "sayfada '{0}' ifadesi var", "page contains '{0}'"),
    ("err.d.client", "istemci oluşturulamadı: {0}", "could not create HTTP client: {0}"),
    ("err.d.empty", "boş yanıt", "empty response"),
    // status
    ("status.no_profile", "Profil ayarlanmadı", "Profile not set"),
    // save_settings
    ("save.saved", "Ayarlar kaydedildi.", "Settings saved."),
    (
        "save.err_profile_chars",
        "Profil kimliği yalnızca harf, rakam, '-' ve '_' içerebilir.",
        "The profile ID may only contain letters, digits, '-' and '_'.",
    ),
    (
        "save.err_profile_empty",
        "Profil kimliği boş bırakılamaz.",
        "The profile ID cannot be empty.",
    ),
    (
        "save.err_confirm",
        "Profil değişikliği için eski kayıtların silinmesini onaylamanız gerekir.",
        "Changing the profile requires confirming that the old records will be deleted.",
    ),
    (
        "save.err_running",
        "Şu anda güncelleme yapılıyor; profil bitince değiştirilebilir.",
        "An update is in progress; the profile can be changed when it finishes.",
    ),
    ("save.err_dir", "Veri klasörü değiştirilemedi: {0}", "Could not change the data folder: {0}"),
    ("save.err_clear", "Eski kayıtlar silinemedi: {0}", "Could not delete the old records: {0}"),
    (
        "save.dir_copied",
        "Mevcut veritabanı yeni klasöre kopyalandı ve yeni klasöre geçildi.",
        "The existing database was copied to the new folder and the app switched to it.",
    ),
    (
        "save.dir_existing",
        "Yeni klasörde zaten bir veritabanı vardı; kopyalama yapılmadı, o veritabanına geçildi.",
        "The new folder already had a database; nothing was copied and the app switched to it.",
    ),
    (
        "save.dir_empty",
        "Yeni klasörde boş bir veritabanı ile başlandı.",
        "Started with an empty database in the new folder.",
    ),
    (
        "save.profile_changed",
        "Profil değişti; önceki profilin tüm kayıtları silindi. Yeni profil birazdan çekilecek.",
        "The profile changed; all records of the previous profile were deleted. The new profile will be fetched shortly.",
    ),
    // commands
    (
        "cmd.err_open_url",
        "Yalnızca Google Scholar, OpenAlex, DOI ve proje (GitHub) bağlantıları açılabilir.",
        "Only Google Scholar, OpenAlex, DOI and project (GitHub) links can be opened.",
    ),
    (
        "art.err_doi",
        "DOI geçersiz. 10.xxxx/... biçiminde, doi:... ya da https://doi.org/... olarak girin.",
        "The DOI is not valid. Enter it as 10.xxxx/..., doi:... or https://doi.org/...",
    ),
    // ORCID (save_settings)
    (
        "save.err_orcid_invalid",
        "ORCID kimliği geçersiz. 0000-0002-1825-0097 biçiminde ya da https://orcid.org/... adresi olarak girin; son hane denetim hanesidir.",
        "The ORCID iD is not valid. Enter it as 0000-0002-1825-0097 or as an https://orcid.org/... address; the last character is a check digit.",
    ),
    (
        "save.err_orcid_empty",
        "ORCID kimliği gereklidir.",
        "The ORCID iD is required.",
    ),
    (
        "save.orcid_changed",
        "ORCID değişti; önceki ORCID ile eşleştirilmiş geçmiş veriler silindi.",
        "The ORCID changed; the historical data matched with the previous ORCID was deleted.",
    ),
    (
        "save.err_bf_running",
        "Geçmiş veri araştırması sürüyor; bitince bu değişiklik yapılabilir.",
        "A historical data lookup is in progress; this change can be made when it finishes.",
    ),
    // historical backfill
    ("bf.err_running", "Geçmiş veri araştırması sürüyor.", "A historical data lookup is already in progress."),
    ("bf.err_rate", "istek sınırı aşıldı (HTTP 429)", "rate limit reached (HTTP 429)"),
    ("bf.err_http", "HTTP {0}", "HTTP {0}"),
    ("bf.err_budget", "OpenAlex günlük kullanım sınırı doldu", "OpenAlex daily usage limit reached"),
    (
        "bf.msg_paused_budget",
        "OpenAlex günlük kullanım sınırı doldu; tarama duraklatıldı ({0}/{1} makale tamamlandı) ve sınır sıfırlanınca kendiliğinden devam edecek.",
        "The OpenAlex daily usage limit is spent; the scan is paused ({0} of {1} articles completed) and continues by itself after the reset.",
    ),
    ("notif.budget_title", "OpenAlex günlük sınırı doldu", "OpenAlex daily limit reached"),
    (
        "notif.budget_body",
        "Tarama duraklatıldı ve sınır sıfırlanınca devam edecek. Ücretsiz bir OpenAlex anahtarı sınırı 10 kat artırır.",
        "The scan is paused and continues after the reset. A free OpenAlex key raises the limit tenfold.",
    ),
    (
        "notif.budget_body_key",
        "OpenAlex anahtarınızın bugünkü bütçesi doldu; tarama sınır sıfırlanınca devam edecek.",
        "Your OpenAlex key's budget for today is spent; the scan continues after the reset.",
    ),
    ("bf.err_parse", "yanıt çözümlenemedi: {0}", "response could not be parsed: {0}"),
    ("bf.err_cancelled", "durduruldu", "stopped"),
    (
        "bf.msg_cancelled",
        "Tarama durduruldu: {0}/{1} makale tamamlandı.",
        "Scan stopped: {0} of {1} articles completed.",
    ),
    (
        "bf.msg_done",
        "{0} / {1} makale eşleşti, {2} atıf kaydı.",
        "{0} of {1} articles matched, {2} citing records.",
    ),
    (
        "bf.msg_orcid",
        "ORCID ile OpenAlex'te {0} yayın bulundu; {1} makale bu kümeden eşleşti.",
        "{0} works found in OpenAlex through the ORCID iD; {1} articles matched from that set.",
    ),
    (
        "bf.msg_orcid_none",
        "ORCID için OpenAlex'te yayın bulunamadı; başlık araması kullanıldı.",
        "No works found in OpenAlex for the ORCID iD; title search was used.",
    ),
    (
        "bf.msg_orcid_failed",
        "ORCID yayın listesi alınamadı ({0}); başlık araması kullanıldı.",
        "The ORCID works list could not be read ({0}); title search was used.",
    ),
    (
        "bf.msg_failed",
        "{0} makale işlenemedi (sonra yeniden denenecek): {1}",
        "{0} articles could not be processed (they will be retried): {1}",
    ),
    (
        "bf.msg_interrupted",
        "Uygulama kapandığı için yarım kaldı.",
        "Interrupted because the app was closed.",
    ),
    // new citing works, weekly summary
    (
        "notif.citing_title",
        "{0} yeni atıf yapan yayın bulundu",
        "{0} new citing works found",
    ),
    (
        "notif.citing_line",
        "{1} dergisinde '{0}' makalenize atıf yaptı",
        "'{0}' in {1} cited your article",
    ),
    ("notif.citing_line_nv", "'{0}' makalenize atıf yaptı", "'{0}' cited your article"),
    (
        "notif.weekly",
        "Bu hafta +{0} atıf; en çok: {1} (+{2})",
        "This week +{0} citations; most: {1} (+{2})",
    ),
    ("notif.weekly_plain", "Bu hafta +{0} atıf", "This week +{0} citations"),
    // citing-list refresh and API keys
    (
        "bf.msg_refresh",
        "Atıf listeleri yenilendi: {0} / {1} makale, {2} atıf kaydı ({3} yeni).",
        "Citing lists refreshed: {0} of {1} articles, {2} citing records ({3} new).",
    ),
    (
        "bf.msg_key_oa",
        "OpenAlex anahtarı reddedildi; anahtarsız devam edildi. Ayarlar'dan anahtarı kontrol edin.",
        "The OpenAlex key was rejected; continued without it. Check the key in Settings.",
    ),
    ("key.err_name", "Bilinmeyen anahtar türü.", "Unknown key type."),
    (
        "key.err_format",
        "Anahtar geçersiz görünüyor (8-200 karakter, boşluksuz).",
        "The key does not look valid (8-200 characters, no spaces).",
    ),
    ("key.err_email", "E-posta adresi geçersiz.", "The e-mail address is not valid."),
    (
        "key.err_store",
        "Anahtar Windows Kimlik Bilgileri Yöneticisi'ne yazılamadı: {0}",
        "The key could not be saved to the Windows Credential Manager: {0}",
    ),
    ("key.err_request", "Anahtar istenemedi: {0}", "The key could not be requested: {0}"),
    (
        "key.err_nokey",
        "Sınanacak anahtar yok: önce bir anahtar girin ya da kaydedin.",
        "There is no key to test: enter or save a key first.",
    ),
    // report and export
    ("cmd.xlsx_name", "scholar_atif_takip.xlsx", "scholar_citation_tracker.xlsx"),
    ("cmd.pdf_name", "scholar_atif_raporu.pdf", "scholar_citation_report.pdf"),
    (
        "cmd.err_report_range",
        "Tarih aralığı geçersiz: başlangıç, bitişten sonra olamaz.",
        "The date range is not valid: the start cannot be after the end.",
    ),
    (
        "cmd.err_report_nodata",
        "Seçilen aralıkta ya da öncesinde kayıtlı veri yok.",
        "There is no stored data in or before the selected range.",
    ),
    (
        "cmd.err_report_print",
        "PDF oluşturulamadı: {0}",
        "The PDF could not be created: {0}",
    ),
    ("cmd.err_goal", "Hedef pozitif bir tam sayı olmalıdır.", "The goal must be a positive whole number."),
    // Excel workbook (sheet names must stay within 31 characters)
    ("xl.sheet.summary", "Özet", "Summary"),
    ("xl.sheet.articles", "Makaleler", "Articles"),
    ("xl.sheet.history", "Makale geçmişi", "Article history"),
    ("xl.sheet.citing", "Atıf yapanlar", "Citing works"),
    ("xl.sheet.yearly", "Yıllık", "Yearly"),
    ("xl.a.0", "Başlık", "Title"),
    ("xl.a.1", "Yıl", "Year"),
    ("xl.a.2", "Dergi / yayın yeri", "Venue"),
    ("xl.a.3", "Atıf", "Citations"),
    ("xl.a.4", "Bugün", "Today"),
    ("xl.a.5", "7 gün", "7 days"),
    ("xl.a.6", "30 gün", "30 days"),
    ("xl.a.7", "Scholar bağlantısı", "Scholar link"),
    ("xl.c.0", "Tarih", "Date"),
    ("xl.c.1", "Tarih duyarlığı", "Date precision"),
    ("xl.c.2", "Başlık", "Title"),
    ("xl.c.3", "Dergi / yayın yeri", "Venue"),
    ("xl.c.4", "DOI", "DOI"),
    ("xl.c.5", "Bağlantı", "Link"),
    ("xl.c.6", "Yazarlar", "Authors"),
    ("xl.c.7", "Ülkeler", "Countries"),
    ("xl.c.8", "Atıf yapılan makale", "Cited article"),
    ("xl.c.9", "Kayıt kimliği", "Record ID"),
    ("xl.y.0", "Yıl", "Year"),
    ("xl.y.1", "Scholar atıf sayısı", "Scholar citations"),
    ("xl.y.2", "Atıf yapan yayın (yayın tarihine göre)", "Citing works (by publication date)"),
    ("xl.prec.day", "gün", "day"),
    ("xl.prec.month", "ay", "month"),
    ("xl.prec.year", "yıl", "year"),
    ("cmd.err_export_kind", "Bilinmeyen dışa aktarma türü.", "Unknown export type."),
    ("cmd.csv_snapshots", "scholar_ozet_gecmisi.csv", "scholar_summary_history.csv"),
    ("cmd.csv_articles", "scholar_makale_gecmisi.csv", "scholar_article_history.csv"),
    // csv headers (snapshots, 10 columns)
    ("csv.s.0", "Zaman", "Fetched at"),
    ("csv.s.1", "Profil kimliği", "Profile ID"),
    ("csv.s.2", "Ad", "Name"),
    ("csv.s.3", "Toplam atıf", "Total citations"),
    ("csv.s.4", "Başlangıç yılından beri atıf", "Citations since start year"),
    ("csv.s.5", "h-indeksi", "h-index"),
    ("csv.s.6", "h-indeksi (başlangıç yılından beri)", "h-index (since start year)"),
    ("csv.s.7", "i10-indeksi", "i10-index"),
    ("csv.s.8", "i10-indeksi (başlangıç yılından beri)", "i10-index (since start year)"),
    ("csv.s.9", "Başlangıç yılı", "Start year"),
    // csv headers (articles, 4 columns)
    ("csv.a.0", "Zaman", "Fetched at"),
    ("csv.a.1", "Makale anahtarı", "Article key"),
    ("csv.a.2", "Başlık", "Title"),
    ("csv.a.3", "Atıf", "Citations"),
];

/// Looks up `key`; an unknown key returns "?" (a test guards against that).
pub fn tr(lang: &str, key: &str) -> &'static str {
    let turkish = normalize_lang(lang) == TR;
    for (k, t, e) in TABLE {
        if *k == key {
            return if turkish { t } else { e };
        }
    }
    "?"
}

/// Replaces `{0}`, `{1}`, ... with `args`.
pub fn fmt(template: &str, args: &[&str]) -> String {
    let mut out = template.to_string();
    for (i, a) in args.iter().enumerate() {
        out = out.replace(&format!("{{{i}}}"), a);
    }
    out
}

/// `tr` followed by `fmt`.
pub fn trf(lang: &str, key: &str, args: &[&str]) -> String {
    fmt(tr(lang, key), args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_key_is_unique_and_filled_in_both_languages() {
        let mut seen = HashSet::new();
        for (k, t, e) in TABLE {
            assert!(seen.insert(*k), "duplicate key {k}");
            assert!(!t.is_empty() && !e.is_empty(), "empty text for {k}");
            assert_ne!(tr(TR, k), "?");
            assert_eq!(tr(TR, k), *t);
            assert_eq!(tr(EN, k), *e);
            // the same placeholders must exist in both languages
            for i in 0..3 {
                let p = format!("{{{i}}}");
                assert_eq!(t.contains(&p), e.contains(&p), "placeholder {p} differs in {k}");
            }
        }
        assert_eq!(tr("en", "no.such.key"), "?");
    }

    #[test]
    fn locale_mapping() {
        assert_eq!(lang_from_locale(Some("tr-TR")), "tr");
        assert_eq!(lang_from_locale(Some("tr_TR")), "tr");
        assert_eq!(lang_from_locale(Some("TR")), "tr");
        assert_eq!(lang_from_locale(Some("en-US")), "en");
        assert_eq!(lang_from_locale(Some("de-DE")), "en");
        assert_eq!(lang_from_locale(None), "en");
        assert_eq!(normalize_lang("xx"), "en");
        assert_eq!(normalize_lang(" TR "), "tr");
    }

    #[test]
    fn fmt_replaces_placeholders() {
        assert_eq!(trf("en", "log.ok", &["10", "3"]), "Total citations: 10, 3 articles");
        assert_eq!(trf("tr", "log.ok", &["10", "3"]), "Toplam atıf: 10, 3 makale");
    }
}
