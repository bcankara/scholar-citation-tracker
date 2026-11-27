"use strict";
(() => {
  const { invoke } = window.__TAURI__.core;
  const { listen } = window.__TAURI__.event;
  const { t } = window.I18N;

  const $ = (s, r = document) => r.querySelector(s);
  const $$ = (s, r = document) => Array.from(r.querySelectorAll(s));
  const loc = () => window.I18N.locale;
  let nf = new Intl.NumberFormat(loc());
  const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));

  const state = {
    overview: null,
    series: [],
    articles: [],
    log: [],
    status: null,
    settings: null,
    dirPending: null,
    trend: null,
    budget: null,
    budgetDismissed: "",
    visit: null,
    analysis: null,
    recentAll: false,
    an: { range: "12m", from: "", to: "", tab: "calendar", gran: "month" },
    cal: { month: "", sel: null, focus: "", source: "all", article: "", days: new Map(), key: "", detail: [], detailKey: "", shown: 120, year: null, yearDays: new Map(), yearKey: "", undatedOpen: false },
    sort: { key: "citations", dir: -1 },
    query: "",
    open: new Set(),
    history: new Map(),
    changes: new Map(),
    changesAll: new Set(),
    artView: new Map(),
    page: "panel",
    bfInfo: null,
    bfProgress: null,
    bf: new Map(),
    health: null,
    update: null,
    updating: false,
    version: "",
    doiKey: null,
    doiOnly: false,
    doiBarHidden: false,
    risers: null,
    risDays: 30,
    next: null,
    citing: [],
    citingGroups: [],
    citingLoaded: false,
    cw: { query: "", article: "", dir: -1, shown: 200, country: "", venue: "", date: "", period: null, type: "", oa: "", field: "", noSelf: false, sort: "date" },
    dist: null,
    recent: [],
    scan: null,
    scanOpen: false,
    scanUserStart: false,
    keys: null,
    keyMsg: { openalex: null },
    firstDay: null,
    nc: { events: [], unread: 0, open: false, seen: new Set() },
    nav: { stack: [], keep: false, depth: 0, tok: 0 },
  };

  state.cal.month = new Date().getFullYear() + "-" + String(new Date().getMonth() + 1).padStart(2, "0");

  const noProfile = () => !state.settings?.profile_id;

  /** Accepts a bare profile id or a profile URL and returns the id. */
  function extractProfileId(input) {
    const v = String(input || "").trim();
    const m = v.match(/[?&;]user=([^&#;\s]*)/i);
    if (m) {
      try { return decodeURIComponent(m[1]).trim(); } catch { return m[1].trim(); }
    }
    return v;
  }
  const validProfileId = (id) => /^[A-Za-z0-9_-]{1,32}$/.test(id);

  /** Accepts an ORCID iD or its https://orcid.org/ address; returns the dashed form. */
  function extractOrcid(input) {
    const v = String(input || "").trim().replace(/^https?:\/\/(www\.)?orcid\.org\//i, "").replace(/^orcid\.org\//i, "").replace(/\/+$/, "");
    const compact = v.replace(/[-\s]/g, "");
    if (/^[0-9A-Za-z]{16}$/.test(compact)) {
      const c = compact.toUpperCase();
      return `${c.slice(0, 4)}-${c.slice(4, 8)}-${c.slice(8, 12)}-${c.slice(12)}`;
    }
    return v;
  }
  /** Format dddd-dddd-dddd-dddc plus the ISO 7064 mod 11-2 check character. */
  function validOrcid(id) {
    if (!/^\d{4}-\d{4}-\d{4}-\d{3}[\dX]$/.test(id)) return false;
    let total = 0;
    for (const ch of id.replace(/-/g, "").slice(0, 15)) total = (total + Number(ch)) * 2;
    const r = (12 - (total % 11)) % 11;
    return id.slice(-1) === (r === 10 ? "X" : String(r));
  }
  const needOrcid = () => !!state.settings?.profile_id && !state.settings?.orcid;

  // ------------------------------------------------------------ formatting
  const fmtDateTime = (iso) => {
    if (!iso) return "—";
    const d = new Date(iso);
    if (isNaN(d)) return "—";
    return d.toLocaleString(loc(), { day: "numeric", month: "long", year: "numeric", hour: "2-digit", minute: "2-digit" });
  };
  const fmtShortDateTime = (iso) => {
    const d = new Date(iso);
    return d.toLocaleString(loc(), { day: "2-digit", month: "2-digit", year: "numeric", hour: "2-digit", minute: "2-digit", second: "2-digit" });
  };
  const fmtTime = (iso) => new Date(iso).toLocaleTimeString(loc(), { hour: "2-digit", minute: "2-digit" });
  const signed = (n) => (n > 0 ? "+" + nf.format(n) : n < 0 ? "−" + nf.format(-n) : "0");
  const chip = (label, n) =>
    `<span class="chip ${n > 0 ? "pos" : n < 0 ? "neg" : ""}">${label ? esc(label) + " " : ""}<b>${signed(n)}</b></span>`;

  // ------------------------------------------------------------ data loading
  async function safe(fn, fallback) {
    try { return await fn(); } catch (e) { console.error(e); return fallback; }
  }
  async function loadStatus() { state.status = await safe(() => invoke("get_status"), state.status); renderHeader(); }
  async function loadOverview() { state.overview = await safe(() => invoke("get_overview"), state.overview); renderHeader(); renderPanel(); }
  async function loadSeries() { state.series = await safe(() => invoke("get_series"), []); }
  async function loadArticles() { state.articles = await safe(() => invoke("get_articles"), []); renderArticles(); renderRecent(); renderScan(); renderVisit(); }
  async function loadLog() { state.log = await safe(() => invoke("get_log"), []); renderLog(); }
  async function loadBfInfo() {
    state.bfInfo = await safe(() => invoke("get_backfill_info"), state.bfInfo);
    if (state.bfInfo) state.bfProgress = state.bfInfo.progress;
    renderHeader();
    renderBackfill();
  }
  async function loadHealth() { state.health = await safe(() => invoke("get_health"), state.health); renderHealth(); }
  async function loadTrend() { state.trend = await safe(() => invoke("get_trend"), state.trend); renderMini(); }
  async function loadBudget() { state.budget = await safe(() => invoke("get_budget_pause"), null); renderBudget(); }
  async function loadVisit() { state.visit = await safe(() => invoke("get_visit_summary"), state.visit); renderVisit(); }
  async function loadAnalysis() {
    state.analysis = await safe(() => invoke("get_analysis"), state.analysis);
    calInvalidate();
    renderAnalysis();
    renderReportPreview();
  }
  async function refreshAll() {
    state.history.clear();
    state.bf.clear();
    state.changes.clear();
    const jobs = [
      loadOverview(), loadSeries(), loadArticles(), loadLog(), loadBfInfo(), loadHealth(),
      loadNext(), loadCiting(), loadDist(), loadRecent(), loadTrend(), loadVisit(), loadBudget(),
    ];
    if (state.page === "analysis" || state.page === "report" || state.analysis) jobs.push(loadAnalysis());
    if (state.page === "analysis") jobs.push(loadRisers());
    await Promise.all(jobs);
  }

  // ------------------------------------------------------------ language
  /** Switches the UI language and re-renders everything that has text. */
  function applyLang(lang) {
    window.I18N.setLang(lang);
    nf = new Intl.NumberFormat(loc());
    $("#setupLang").value = window.I18N.lang;
    $("#setLang").value = window.I18N.lang;
    $("#setTheme").value = window.SCTTheme.pref;
    renderHeader();
    renderPanel();
    renderArticles();
    renderLog();
    renderBackfill();
    renderHealth();
    renderUpdate();
    renderAbout();
    renderAboutResult();
    renderAnalysis();
    renderCiting();
    renderHk();
    renderKeys();
    renderRecent();
    renderScan();
    renderBudget();
    renderReportPage();
    renderReportPreview();
    renderNc();
    renderBack();
    state.open.forEach(drawDetail);
  }

  // ------------------------------------------------------------ toast, health, updates
  let toastTimer;
  function toast(text, err) {
    const el = $("#toast");
    el.textContent = text;
    el.className = "toast" + (err ? " err" : "");
    el.hidden = false;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => { el.hidden = true; }, 6000);
  }

  function renderHealth() {
    const h = state.health;
    const show = !noProfile() && !!h?.unhealthy;
    $("#healthBar").hidden = !show;
    if (show) $("#healthText").textContent = t("health.text", { time: h.last_ok ? fmtDateTime(h.last_ok) : "—" });
  }

  function renderUpdate() {
    const u = state.update;
    $("#updateBar").hidden = !u;
    if (!u) return;
    if (!state.updating) {
      $("#updateText").textContent = t("update.ready", { version: u.version });
      $("#updateProgress").hidden = true;
    }
    const notes = $("#updateNotes");
    notes.textContent = u.notes || "";
    $("#updateNotesWrap").hidden = !u.notes;
    $("#updateNow").disabled = state.updating;
    $("#updateLater").disabled = state.updating;
  }

  /** Progress of the update download: the top bar and the About card show the same text. */
  function updateProgressUi(text, pct) {
    $("#updateText").textContent = text;
    $("#aboutResultText").textContent = text;
    $("#updateProgress").hidden = false;
    $("#aboutProgress").hidden = false;
    if (pct !== null && pct !== undefined) {
      $("#updateBarFill").style.width = pct + "%";
      $("#aboutBarFill").style.width = pct + "%";
    }
  }

  async function installUpdate() {
    if (!state.update || state.updating) return;
    state.updating = true;
    updateProgressUi(t("update.downloading", { percent: "0%" }), 0);
    renderUpdate();
    renderAboutResult();
    try {
      await invoke("install_update");
      // On Windows the installer ends this process; on macOS the app bundle is replaced in
      // place. If we are still here, restart explicitly.
      updateProgressUi(t("update.installing"), 100);
      await window.__TAURI__.process.relaunch();
    } catch (e) {
      state.updating = false;
      state.aboutRes = { kind: "failed", text: t("update.failed", { error: String(e) }) };
      renderUpdate();
      $("#updateText").textContent = t("update.failed", { error: String(e) });
      renderAboutResult();
    }
  }

  /** The result of "Check for updates" inside the About card, right under the button. */
  function renderAboutResult() {
    const r = state.aboutRes;
    const box = $("#aboutResult");
    box.hidden = !r;
    if (!r) return;
    const u = state.update;
    const avail = r.kind === "available" && !!u;
    box.className = "about-result " + (r.kind === "error" || r.kind === "failed" ? "err" : avail ? "avail" : "ok");
    $("#aboutResultIcon").textContent = r.kind === "error" || r.kind === "failed" ? "" : avail ? "" : "";
    if (!state.updating) {
      $("#aboutResultText").textContent = avail ? t("about.available_here", { version: u.version }) : r.text;
      $("#aboutProgress").hidden = true;
    }
    $("#aboutNotes").textContent = avail ? u.notes || "" : "";
    $("#aboutNotesWrap").hidden = !(avail && u.notes);
    $("#aboutInstall").hidden = !avail;
    $("#aboutInstall").disabled = state.updating;
  }

  async function checkUpdatesManually() {
    const btn = $("#checkUpdateBtn");
    btn.disabled = true;
    $("#checkUpdateText").textContent = t("about.checking");
    state.aboutRes = { kind: "checking", text: t("about.checking") };
    renderAboutResult();
    let msg, err = false, dialog = false;
    try {
      const r = await invoke("check_for_updates");
      if (r.status === "available") {
        state.update = r.info; renderUpdate();
        state.aboutRes = { kind: "available", text: "" };
        msg = t("about.available_toast", { version: r.info.version });
        showUpdateDialog(r.info);
        dialog = true; // the dialog says it; no toast on top of it
      } else if (r.status === "current") {
        state.aboutRes = { kind: "current", text: t("about.current", { version: state.version }) };
        msg = state.aboutRes.text;
      } else if (r.status === "not_found") {
        state.aboutRes = { kind: "not_found", text: t("about.not_found") };
        msg = state.aboutRes.text;
      } else {
        err = true;
        state.aboutRes = { kind: "error", text: t("about.error", { error: r.error || "?" }) };
        msg = state.aboutRes.text;
      }
    } catch (e) {
      err = true;
      state.aboutRes = { kind: "error", text: t("about.error", { error: String(e) }) };
      msg = state.aboutRes.text;
    } finally {
      btn.disabled = false;
      $("#checkUpdateText").textContent = t("about.check");
    }
    renderAboutResult();
    if (!dialog) toast(msg, err);
    $("#aboutResult").scrollIntoView({ block: "nearest", behavior: matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth" });
  }

  function renderAbout() {
    $("#aboutTitle").textContent = t("about.title", { version: state.version || "…" });
  }

  // ------------------------------------------------------------ header
  function renderHeader() {
    const ov = state.overview;
    const st = state.status;
    const none = noProfile() || st?.state === "no_profile";
    $("#profileName").textContent = ov?.snapshot?.name || t("app.name");
    $("#lastUpdate").textContent = ov?.snapshot ? fmtDateTime(ov.snapshot.fetched_at) : "—";
    $("#nextRun").textContent = !none && st?.next_run ? fmtDateTime(st.next_run) : "—";

    const pill = $("#statusPill");
    const text = $("#statusText");
    let cls = "pill", label = t("status.idle");
    if (none) {
      label = t("status.no_profile");
    } else if (st) {
      if (st.state === "running") {
        cls = "pill pill-running";
        label = t("status.running", { attempt: st.attempt, max: st.max_attempts });
      } else if (st.state === "waiting_retry") {
        cls = "pill pill-warn";
        label = t("status.waiting_retry", { error: st.last_error || t("status.unknown_error"), time: st.retry_at ? fmtTime(st.retry_at) : "" });
      } else if (st.last_error) {
        cls = "pill pill-error";
        label = t("status.error", { error: st.last_error });
      }
    }
    pill.className = cls;
    text.textContent = label;
    pill.title = label;

    const bp = state.bfProgress;
    $("#bfPill").hidden = !bp?.running;
    if (bp?.running) {
      $("#bfPillText").textContent = bp.k > 0 ? t("bf.pill", { k: bp.k, n: bp.n }) : t("bf.pill_start");
      $("#bfPill").title = t("bf.pill_title");
    }

    const running = st?.state === "running";
    for (const b of [$("#refreshBtn"), $("#emptyRefresh")]) {
      b.disabled = running || none;
      const ic = $(".icon", b);
      ic.classList.toggle("spin", running);
    }
  }

  async function refreshNow() {
    const ok = await safe(() => invoke("refresh_now"), false);
    if (!ok) await loadStatus();
  }

  // ------------------------------------------------------------ panel
  const DAY_MS = 86400e3;
  const pad2 = (n) => String(n).padStart(2, "0");
  const pl = (n, key) => t(key + (n === 1 ? ".one" : ".many"));
  const shortText = (s, n) => (s.length > n ? s.slice(0, n - 1).trimEnd() + "…" : s);
  const monthLabel = (ym, long) => {
    const [y, m] = ym.split("-").map(Number);
    return new Date(Date.UTC(y, m - 1, 1)).toLocaleDateString(loc(), { month: long ? "long" : "short", year: long ? "numeric" : undefined, timeZone: "UTC" });
  };
  const hideTip = () => { const el = $("#tooltip"); if (el) el.hidden = true; };
  const srcName = (s) => (s === "openalex" ? "OpenAlex" : String(s || ""));

  /** Turkish dative suffix of a number as it is spoken: 7'ye, 8'e, 9'a, 6'ya, 20'ye, 10'a. */
  function trDative(n) {
    n = Math.abs(Math.trunc(n));
    const digit = ["a", "e", "ye", "e", "e", "e", "ya", "ye", "e", "a"];
    const tens = ["", "a", "ye", "a", "a", "ye", "a", "e", "e", "a"];
    let s;
    if (n === 0) s = digit[0];
    else if (n % 100 === 0) s = "e";
    else if (n % 10 === 0) s = tens[(n / 10) % 10];
    else s = digit[n % 10];
    return "'" + s;
  }

  /** "h=9 için 3 atıf": what the next step of an index needs (empty when unknown). */
  function nextLine(label, s) {
    if (!s) return "";
    if (s.picks.length) return t("kpi.next", { label, n: nf.format(s.total_needed), unit: pl(s.total_needed, "unit.citation") });
    return t("kpi.next_new", { label, n: nf.format(s.new_papers), unit: pl(s.new_papers, "unit.article") });
  }

  function kpiCard(label, value, small, sub, d, next) {
    return `<div class="card kpi">
      <div class="kpi-label">${esc(label)}</div>
      <div class="kpi-value">${nf.format(value)}${small !== null ? `<small>/ ${nf.format(small)}</small>` : ""}</div>
      <div class="kpi-sub">${esc(sub)}</div>
      <div class="kpi-next">${esc(next || "")}</div>
      <div class="chips">${chip(t("chip.today"), d.today)}${chip(t("chip.d7"), d.d7)}${chip(t("chip.d30"), d.d30)}</div>
    </div>`;
  }

  function renderPanel() {
    const ov = state.overview;
    const has = !!ov?.snapshot;
    const none = noProfile();
    const orcidOnly = !none && needOrcid();
    $("#setupCard").hidden = !(none || orcidOnly);
    if (none || orcidOnly) renderSetup(orcidOnly);
    $("#emptyState").hidden = none || has || ov === null;
    $("#panelData").hidden = none || !has;
    renderVisit();
    if (none || !has) { Charts.disposeIn($("#miniCard")); return; }
    const s = ov.snapshot;
    const d = ov.deltas;
    const z = { today: 0, d7: 0, d30: 0 };
    const y = { year: s.since_year };
    const n = state.next;
    $("#kpis").innerHTML = [
      kpiCard(t("kpi.total"), s.cit_all, null, t("kpi.total_sub"), d?.cit_all || z, ""),
      kpiCard(t("kpi.since", y), s.cit_since, null, t("kpi.since_sub"), d?.cit_since || z, ""),
      kpiCard(t("kpi.h"), s.h_all, s.h_since, t("kpi.both_sub", y), d?.h_all || z, n ? nextLine(`h=${n.h.target}`, n.h) : ""),
      kpiCard(t("kpi.i10"), s.i10_all, s.i10_since, t("kpi.both_sub", y), d?.i10_all || z, n ? nextLine(`i10=${n.i10.target}`, n.i10) : ""),
    ].join("");
    renderMini();
    renderRecent();
  }

  // ------------------------------------------------------------ "since your last visit" sentence
  /** A date like "12 Kasım" (the year is added when it is not the current one). */
  function fmtDayMonth(iso) {
    const d = new Date(iso);
    if (isNaN(d)) return "—";
    return d.toLocaleDateString(loc(), { day: "numeric", month: "long", ...(d.getFullYear() === new Date().getFullYear() ? {} : { year: "numeric" }) });
  }

  /** The sentence as HTML: priority (a) index threshold, (b) new citations and the top article, (c) a net
   *  decrease, (d) closeness to the next h; otherwise the calm state. The article and the h phrase are links. */
  function visitHtml(s) {
    const link = (go, text, extra) => `<button type="button" class="vs-link" data-go="${go}"${extra || ""}>${esc(text)}</button>`;
    const art = (x) => link("article", shortText(x.title, 80), ` data-key="${esc(x.key)}"`);
    const tr = window.I18N.lang === "tr";
    switch (s.kind) {
      case "first":
        return esc(t("vs.first", { n: nf.format(s.total), h: s.h }));
      case "threshold": {
        const phrase = t(s.metric === "h" ? "vs.h_phrase" : "vs.i10_phrase", { value: s.value, sfx: tr ? trDative(s.value) : "" });
        return t("vs.threshold", { phrase: link("indices", phrase) });
      }
      case "gain": {
        const unit = pl(s.net, "unit.citation");
        if (s.top && s.top.gain >= s.net) return t("vs.gain_one", { n: nf.format(s.net), unit, article: art(s.top) });
        if (s.top) return t("vs.gain", { n: nf.format(s.net), unit, article: art(s.top), k: nf.format(s.top.gain) });
        return esc(t("vs.gain_plain", { n: nf.format(s.net), unit }));
      }
      case "loss":
        return esc(t("vs.loss", { n: nf.format(-s.net), unit: pl(-s.net, "unit.citation") }));
      case "closeness": {
        const n = s.next_h_needs;
        return t("vs.closeness", { target: link("indices", `h=${s.next_h_target}`), n: nf.format(n), unit: pl(n, "unit.citation") });
      }
      default:
        return esc(s.last_gain_at ? t("vs.calm_with", { date: fmtDayMonth(s.last_gain_at) }) : t("vs.calm"));
    }
  }

  function renderVisit() {
    const el = $("#visitSummary");
    const s = state.visit;
    const show = state.page === "panel" && !!s && !noProfile() && !needOrcid() && !!state.overview?.snapshot;
    el.hidden = !show;
    if (show) el.innerHTML = visitHtml(s);
  }

  /** Opens one article's detail row on the Articles page. */
  function openArticle(key) {
    go(() => { state.open.add(key); showPage("articles"); });
    setTimeout(() => {
      const row = $(`#articleBody tr.row[data-key="${CSS.escape(key)}"]`);
      if (row) row.scrollIntoView({ block: "center", behavior: "smooth" });
    }, 200);
  }
  function openAnalysis(tab) {
    go(() => {
      if (tab) state.an.tab = tab;
      showPage("analysis");
    });
  }

  // ------------------------------------------------------------ panel: compact citation history
  function renderMini() {
    if (state.page !== "panel" || $("#panelData").hidden) return;
    const el = $("#miniChart");
    const tr = state.trend;
    const c = Charts.ctx({ mini: true, forecast: false, goal: false, range: { from: Date.now() - 365 * DAY_MS, to: null } });
    const built = tr && tr.has_data ? Charts.build.trend({ trend: tr }, c) : null;
    Charts.mount(el, built, { emptyText: t("chart.no_data") });
    $("#miniCaption").textContent = tr && tr.has_data ? t(tr.hist.length ? "mini.cap_merged" : "mini.cap") : "";
  }

  /** First-run card (profile + ORCID + language) or, for older settings, the ORCID-only variant. */
  function renderSetup(orcidOnly) {
    $("#setupCard").classList.toggle("setup-compact", orcidOnly);
    $("#setupTitle").textContent = t(orcidOnly ? "setup.orcid_only_title" : "setup.title");
    $("#setupIntro").textContent = t(orcidOnly ? "setup.orcid_only_intro" : "setup.intro");
    $("#setupProfileField").hidden = orcidOnly;
    $("#setupLangField").hidden = orcidOnly;
    $("#startBtn .icon").innerHTML = orcidOnly ? "&#xE74E;" : "&#xE768;";
    $("#startBtn span:not(.icon)").textContent = t(orcidOnly ? "setup.orcid_save" : "setup.start");
    updateSetupValidity();
  }
  function updateSetupValidity() {
    const orcidOnly = !noProfile() && needOrcid();
    const orcid = extractOrcid($("#setupOrcid").value);
    const orcidOk = validOrcid(orcid);
    const idOk = orcidOnly || validProfileId(extractProfileId($("#setupProfile").value));
    const err = $("#setupOrcidError");
    const showErr = $("#setupOrcid").value.trim() !== "" && !orcidOk;
    err.textContent = showErr ? t("setup.err_orcid_invalid") : "";
    err.hidden = !showErr;
    $("#startBtn").disabled = !(orcidOk && idOk);
  }

  // ------------------------------------------------------------ articles
  const scholarUrl = (ids) => `https://scholar.google.com/scholar?oi=bibs&hl=${window.I18N.lang}&cites=${encodeURIComponent(ids)}&scisbd=1`;
  const doiUrl = (doi) => "https://doi.org/" + encodeURI(doi);
  const missingDoi = () => state.articles.filter((a) => !a.doi);

  function renderDoiBar() {
    const miss = missingDoi().length;
    if (state.doiOnly && miss === 0) state.doiOnly = false;
    const show = state.page === "articles" && miss > 0 && !state.doiBarHidden;
    $("#doiBar").hidden = !show;
    $("#doiOnly").checked = state.doiOnly;
    if (show) $("#doiBarText").textContent = t(miss === 1 ? "doibar.text.one" : "doibar.text.many", { n: nf.format(miss) });
  }

  /** "Scholar 59 · identified 40 · not yet identified 19" with a thin two-part bar (only once citing works exist). */
  function idLine(a) {
    if (!state.articles.some((x) => x.identified > 0)) return "";
    const unidentified = Math.max(0, a.citations - a.identified);
    const pct = a.citations > 0 ? Math.min(100, Math.round((a.identified / a.citations) * 100)) : 0;
    return `<div class="idbar-line"><span class="idbar" aria-hidden="true"><i style="width:${pct}%"></i></span><span class="idbar-text">${esc(t("gap.line", { s: nf.format(a.citations), i: nf.format(a.identified), u: nf.format(unidentified) }))}</span></div>`;
  }

  function renderArticles() {
    const q = state.query.trim().toLocaleLowerCase(loc());
    let rows = state.articles.filter((a) => (!q || `${a.title} ${a.authors} ${a.venue}`.toLocaleLowerCase(loc()).includes(q)) && (!state.doiOnly || !a.doi));
    const { key, dir } = state.sort;
    rows = rows.slice().sort((a, b) => {
      let r;
      if (key === "title") r = a.title.localeCompare(b.title, loc());
      else r = (a[key] ?? -1) - (b[key] ?? -1);
      return r * dir || a.title.localeCompare(b.title, loc());
    });
    renderDoiBar();
    $("#articleCount").textContent = t("articles.count", { n: nf.format(rows.length) });
    $$("#articleTable th[data-sort]").forEach((th) => {
      const old = $(".sort", th); if (old) old.remove();
      if (th.dataset.sort === key) th.insertAdjacentHTML("beforeend", `<span class="sort">${dir > 0 ? "&#xE70E;" : "&#xE70D;"}</span>`);
    });
    const body = $("#articleBody");
    Charts.disposeIn(body);
    if (!rows.length) {
      body.innerHTML = `<tr><td colspan="5" class="empty-row">${esc(state.articles.length ? t("articles.none_match") : t("articles.none"))}</td></tr>`;
      return;
    }
    body.innerHTML = rows.map((a) => {
      const open = state.open.has(a.key);
      const doi = a.doi
        ? `<a href="#" class="doi-chip" data-url="${esc(doiUrl(a.doi))}" title="${esc(a.doi)}">DOI</a>`
        : `<button type="button" class="btn btn-outline doi-add" data-act="doi" data-key="${esc(a.key)}">${esc(t("art.add_doi"))}</button>`;
      return `<tr class="row ${open ? "open" : ""}" data-key="${esc(a.key)}">
        <td><div class="title-cell"><span class="chev"></span><div class="title-body">
          <a href="#" class="title-link" data-url="${esc(a.url)}">${esc(a.title)}</a>${a.is_new ? `<span class="badge-new">${esc(t("articles.new"))}</span>` : ""}
          <div class="title-meta"><span class="venue">${esc(a.venue)}</span>${doi}</div>${idLine(a)}</div></div></td>
        <td class="num">${a.year ?? "—"}</td>
        <td class="num"><b>${nf.format(a.citations)}</b></td>
        <td class="num">${chip("", a.today)}</td>
        <td class="num">${chip("", a.d7)}</td>
      </tr>${open ? `<tr class="detail" data-for="${esc(a.key)}"><td colspan="5"><div class="detail-grid">
        <div class="detail-col">
          <div class="seg d-seg" role="group"><button data-view="scholar" data-key="${esc(a.key)}">${esc(t("art.v_scholar"))}</button><button data-view="bf" data-key="${esc(a.key)}">${esc(t("art.v_bf"))}</button></div>
          <div class="chart-area d-chart"></div>
          <div class="d-match caption"></div><div class="d-actions"></div></div>
        <div class="detail-col"><div class="detail-sub caption secondary">${esc(t("art.changes_title"))}</div><div class="d-sum caption"></div><div class="d-changes"></div></div>
        <div class="detail-col detail-wide"><div class="detail-sub caption secondary">${esc(t("cw.in_article"))} <span class="d-citing-n"></span></div><div class="d-citing"></div></div>
      </div></td></tr>` : ""}`;
    }).join("");
    state.open.forEach(drawDetail);
  }

  /** OpenAlex count, how the match was made and links for one article's backfill record. */
  function matchHtml(b) {
    const m = b?.match;
    if (!m) return `<span class="secondary">${esc(t("art.bf_none"))}</span>`;
    const manual = m.match_method === "manual_doi";
    if (!m.source && manual) return `<span class="badge-none">${esc(t("art.bf_unmatched"))}</span> <span class="secondary">${esc(t("art.bf_doi_not_found"))}</span> <b>${esc(m.doi || "")}</b>`;
    if (!m.source) return `<span class="badge-none">${esc(t("art.bf_unmatched"))}</span> <span class="secondary">${esc(t("art.bf_unmatched_hint"))}</span>`;
    const c = (n) => (n === null || n === undefined ? "—" : nf.format(n));
    const parts = [`<span class="bf-sources">OpenAlex <b>${c(m.openalex_count)}</b></span>`];
    const methodKey = manual ? "art.method_manual" : m.match_method === "orcid" ? "art.method_orcid" : "art.method_title";
    parts.push(`<span class="secondary">${esc(t(methodKey))}${m.score != null && !manual ? " · " + esc(t("art.bf_score", { s: m.score.toFixed(2) })) : ""}</span>`);
    if (b.override?.rejected > 0) parts.push(`<span class="secondary">${esc(t("art.rejected_n", { n: nf.format(b.override.rejected) }))}</span>`);
    const links = [];
    if (m.source === "openalex" && m.external_id) links.push([`https://openalex.org/${m.external_id}`, t("art.open_openalex")]);
    if (m.doi) links.push([doiUrl(m.doi), t("art.open_doi")]);
    for (const [url, label] of links) parts.push(`<a href="#" class="ext-link" data-url="${esc(url)}">${esc(label)}</a>`);
    if (b.year_only > 0) parts.push(`<span class="secondary">${esc(t("art.year_only", { n: nf.format(b.year_only) }))}</span>`);
    return parts.join(" ");
  }

  /** Last-30-days line and the change list (newest first, collapsed after five). */
  function changesHtml(key, ch) {
    const items = ch?.items || [];
    if (!items.length) return [`<span class="secondary">${esc(t("art.changes_none"))}</span>`, ""];
    const s = ch.last30;
    const sum = (s.up || s.down)
      ? t("art.sum30", { up: "+" + nf.format(s.up), down: "−" + nf.format(s.down), net: signed(s.net) })
      : t("art.sum30_none");
    const all = state.changesAll.has(key);
    const shown = all ? items : items.slice(0, 5);
    let list = `<ul class="ch-list">` + shown.map((c) => `<li><span class="ch-date">${esc(fmtShortDateTime(c.at))}</span><span class="ch-vals">${nf.format(c.old)} → ${nf.format(c.new)}</span>${chip("", c.delta)}</li>`).join("") + `</ul>`;
    if (items.length > 5) {
      list += `<button type="button" class="btn btn-link" data-act="morechanges" data-key="${esc(key)}"><span>${esc(all ? t("art.changes_less") : t("art.changes_more", { n: nf.format(items.length - 5) }))}</span></button>`;
    }
    return [esc(sum), list];
  }

  async function drawDetail(key) {
    const tr = $(`tr.detail[data-for="${CSS.escape(key)}"]`);
    if (!tr) return;
    let hist = state.history.get(key);
    if (!hist) {
      hist = await safe(() => invoke("get_article_history", { key }), []);
      state.history.set(key, hist);
    }
    let bfd = state.bf.get(key);
    if (!bfd) {
      bfd = await safe(() => invoke("get_article_backfill", { key }), null);
      if (bfd) state.bf.set(key, bfd);
    }
    let ch = state.changes.get(key);
    if (!ch) {
      ch = await safe(() => invoke("get_article_changes", { key }), null);
      if (ch) state.changes.set(key, ch);
    }
    const row = $(`tr.detail[data-for="${CSS.escape(key)}"]`);
    if (!row) return;
    const monthly = bfd?.monthly || [];
    let view = state.artView.get(key);
    if (!view) view = hist.length >= 2 || !monthly.length ? "scholar" : "bf";
    $$(".d-seg button", row).forEach((b) => b.classList.toggle("on", b.dataset.view === view));
    if (view === "scholar") {
      const pts = hist.map(([ts, v]) => [new Date(ts).getTime(), v]);
      Charts.mount($(".d-chart", row), Charts.build.articleLine(pts, Charts.ctx()), { emptyText: t("chart.no_data") });
    } else {
      Charts.mount($(".d-chart", row), Charts.build.articleMonthly(monthly, Charts.ctx()), { emptyText: t("art.bf_empty") });
    }
    $(".d-match", row).innerHTML = matchHtml(bfd);
    const art = state.articles.find((a) => a.key === key);
    $(".d-actions", row).innerHTML = actionsHtml(key, bfd, art);
    const [sum, list] = changesHtml(key, ch);
    $(".d-sum", row).innerHTML = sum;
    $(".d-changes", row).innerHTML = list;
    if (!state.citingLoaded) await loadCiting();
    const again = $(`tr.detail[data-for="${CSS.escape(key)}"]`);
    if (again) drawDetailCiting(again, key);
  }

  /** The citing works of one article, newest first (the first 25; the rest on the Citing works page). */
  function drawDetailCiting(row, key) {
    const list = (state.citingGroups || [])
      .filter((g) => g.articles.some((a) => a.key === key))
      .sort((a, b) => (a.date < b.date ? 1 : a.date > b.date ? -1 : 0));
    $(".d-citing-n", row).textContent = list.length ? `(${nf.format(list.length)})` : "";
    const el = $(".d-citing", row);
    if (!list.length) { el.innerHTML = `<div class="caption secondary">${esc(t("cw.none_for_article"))}</div>`; return; }
    const items = list.slice(0, 25).map((g) => `<li><span class="cw-d">${esc(fmtCitingDate(g.date, g.precision))}</span>
      <a href="#" class="title-link" data-url="${esc(citingUrl(g))}">${esc(g.title || "—")}</a>
      <span class="secondary">${esc(g.venue ? " · " + g.venue : "")}</span></li>`).join("");
    const more = list.length > 25
      ? `<button class="btn" data-act="allciting" data-key="${esc(key)}"><span>${esc(t("cw.show_all", { n: nf.format(list.length) }))}</span></button>` : "";
    el.innerHTML = `<ul class="d-citing-list">${items}</ul>${more}`;
  }

  /** DOI override actions and the Scholar link of one article's detail row. */
  function actionsHtml(key, b, art) {
    const m = b?.match;
    const k = esc(key);
    const btn = (act, icon, label) => `<button class="btn" data-act="${act}" data-key="${k}"><span class="icon">${icon}</span><span>${esc(t(label))}</span></button>`;
    const out = [btn("doi", "&#xE70F;", "art.act_doi")];
    if (m?.source && m.match_method !== "manual_doi") out.push(btn("reject", "&#xE711;", "art.act_reject"));
    if (b?.override) out.push(btn("reset", "&#xE777;", "art.act_reset"));
    if (art?.cites_ids) out.push(`<button class="btn" data-act="scholar" data-cites="${esc(art.cites_ids)}" title="${esc(t("scholar.tip"))}"><span class="icon">&#xE8A7;</span><span>${esc(t("scholar.view"))}</span></button>`);
    return out.join("");
  }

  const OVERRIDE_RESULT = { started: "art.relooking", no_orcid: "bf.need_orcid", running: "bf.already" };
  function overrideDone(status, key) {
    toast(t(OVERRIDE_RESULT[status] || "art.relooking"), false);
    state.bf.delete(key);
    drawDetail(key);
  }

  function openDoiDialog(key) {
    const art = state.articles.find((a) => a.key === key);
    const cached = state.bf.get(key);
    state.doiKey = key;
    $("#doiDlgArticle").textContent = art?.title || "";
    $("#doiInput").value = cached?.override?.doi || cached?.match?.doi || art?.doi || "";
    $("#doiError").hidden = true;
    $("#doiPaste").hidden = !navigator.clipboard?.readText;
    $("#doiDialog").hidden = false;
    $("#doiInput").focus();
    $("#doiInput").select();
  }
  function closeDoiDialog() { $("#doiDialog").hidden = true; state.doiKey = null; }
  async function pasteDoi() {
    try {
      const text = (await navigator.clipboard.readText()).trim();
      if (text) { $("#doiInput").value = text; $("#doiError").hidden = true; }
    } catch { /* clipboard access was refused: the user can type or paste with Ctrl+V */ }
    $("#doiInput").focus();
  }
  async function saveDoi(ev) {
    ev.preventDefault();
    const key = state.doiKey;
    const doi = $("#doiInput").value.trim();
    const err = $("#doiError");
    if (!doi) { err.textContent = t("art.err_doi_empty"); err.hidden = false; return; }
    $("#doiSave").disabled = true;
    state.scanUserStart = true;
    try {
      const status = await invoke("set_article_doi", { key, doi });
      if (status !== "started") state.scanUserStart = false;
      closeDoiDialog();
      overrideDone(status, key);
    } catch (e) {
      state.scanUserStart = false;
      err.textContent = String(e);
      err.hidden = false;
    } finally {
      $("#doiSave").disabled = false;
    }
  }
  async function rejectMatch(key) {
    const ok = await window.__TAURI__.dialog.ask(t("art.reject_confirm"), {
      title: t("art.reject_title"), kind: "warning", okLabel: t("art.reject_ok"), cancelLabel: t("dialog.cancel"),
    });
    if (!ok) return;
    state.scanUserStart = true;
    try {
      const status = await invoke("reject_article_match", { key });
      if (status !== "started") state.scanUserStart = false;
      overrideDone(status, key);
    } catch (e) { state.scanUserStart = false; toast(String(e), true); }
  }
  async function resetOverride(key) {
    state.scanUserStart = true;
    try {
      const status = await invoke("reset_article_override", { key });
      if (status !== "started") state.scanUserStart = false;
      overrideDone(status, key);
      toast(t("art.reset_done"), false);
    } catch (e) { state.scanUserStart = false; toast(String(e), true); }
  }

  /** Opens the Articles page filtered to the articles that have no DOI yet. */
  function showMissingDoi() {
    go(() => {
      state.doiOnly = true;
      state.doiBarHidden = false;
      showPage("articles");
    });
  }

  // ------------------------------------------------------------ log
  const STATUS_KEY = { ok: "log.st_ok", blocked: "log.st_blocked", network: "log.st_network", parse: "log.st_parse" };
  function renderLog() {
    const body = $("#logBody");
    if (!state.log.length) { body.innerHTML = `<tr><td colspan="4" class="empty-row">${esc(t("log.none"))}</td></tr>`; return; }
    body.innerHTML = state.log.map((r) => `<tr>
      <td class="num" style="text-align:left">${esc(fmtShortDateTime(r.attempted_at))}</td>
      <td class="num">${r.attempt_no}</td>
      <td><span class="st st-${esc(r.status)}">${esc(STATUS_KEY[r.status] ? t(STATUS_KEY[r.status]) : r.status)}</span></td>
      <td class="msg">${esc(r.message)}</td></tr>`).join("");
  }

  // ------------------------------------------------------------ recent changes
  const RECENT_SHORT = 6;
  const RECENT_ALL = 40;
  async function loadRecent() {
    state.recent = await safe(() => invoke("get_recent_citations", { limit: state.recentAll ? RECENT_ALL : RECENT_SHORT }), state.recent);
    renderRecent();
  }

  const isoLocalDay = (d) => `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;
  /** The calendar day a recent row belongs to (Scholar rows: local day of the check; citing works: their own date). */
  function recentDay(r) {
    if (r.kind === "citing") return r.precision === "day" ? r.date : r.date.length === 7 ? r.date + "-01" : r.date.slice(0, 10);
    return isoLocalDay(new Date(r.date));
  }
  /** "Bugün" / "Dün" / "21 Kasım" for a `YYYY-MM-DD` day. */
  function recentHeading(day) {
    const [y, m, d] = day.split("-").map(Number);
    const dt = new Date(y, m - 1, d);
    const today = new Date();
    const yest = new Date(today.getFullYear(), today.getMonth(), today.getDate() - 1);
    const text = dt.toLocaleDateString(loc(), { weekday: "long", day: "numeric", month: "long", ...(y === today.getFullYear() ? {} : { year: "numeric" }) });
    if (isoLocalDay(today) === day) return `${t("recent.today")} · ${text}`;
    if (isoLocalDay(yest) === day) return `${t("recent.yesterday")} · ${text}`;
    return text;
  }

  function recentRow(r) {
    if (r.kind === "citing") {
      const url = r.doi ? doiUrl(r.doi) : r.url || "";
      const title = url
        ? `<a href="#" class="title-link rc-title" data-url="${esc(url)}">${esc(r.citing_title || "—")}</a>`
        : `<span class="rc-title body-strong">${esc(r.citing_title || "—")}</span>`;
      return `<li class="rc-row rc2"><div class="rc-main">${title}
        <div class="rc-sub secondary">${r.venue ? esc(r.venue) + " " : ""}<span class="rc-arrow">→</span> <span title="${esc(r.article_title)}">${esc(shortText(r.article_title, 70))}</span> · <span class="src-badge">OpenAlex</span></div></div></li>`;
    }
    const down = r.kind === "scholar_down";
    const open = r.cites_ids
      ? `<button type="button" class="rc-open" data-act="scholar" data-cites="${esc(r.cites_ids)}" title="${esc(t("scholar.tip"))}" aria-label="${esc(t("scholar.view"))}"><span class="icon">&#xE8A7;</span></button>` : "";
    return `<li class="rc-row rc2 ${down ? "rc-down" : ""}"><div class="rc-main">
        <div class="rc-line"><a href="#" class="rc-title body-strong title-link" data-open-article="${esc(r.article_key)}" title="${esc(r.article_title)}">${esc(r.article_title)}</a>${chip("", r.delta)}</div>
        ${down ? `<div class="rc-sub neg-text">${esc(t("recent.decreased"))} · ${nf.format(r.old)} → ${nf.format(r.new)}</div>` : `<div class="rc-sub secondary">${nf.format(r.old)} → ${nf.format(r.new)}</div>`}</div>
        <div class="rc-side">${open}</div></li>`;
  }

  function renderRecent() {
    if (state.page !== "panel" || $("#panelData").hidden) return;
    const el = $("#recentList");
    if (!state.recent.length) {
      el.innerHTML = `<div class="ris-empty secondary">${esc(t("recent.empty"))}</div>`;
    } else {
      let html = "";
      let cur = "";
      for (const r of state.recent) {
        const day = recentDay(r);
        if (day !== cur) {
          if (cur) html += "</ul>";
          cur = day;
          html += `<div class="rc-group-head"><button type="button" class="rc-group-btn" data-day="${day}" title="${esc(t("recent.open_day"))}">${esc(recentHeading(day))}</button></div><ul class="rc-list">`;
        }
        html += recentRow(r);
      }
      el.innerHTML = html + "</ul>";
    }
    $("#recentAllText").textContent = t(state.recentAll ? "recent.less" : "recent.all");
    $("#recentAll").hidden = !state.recent.length || (!state.recentAll && state.recent.length < RECENT_SHORT);
    const miss = missingDoi().length;
    const hint = $("#recentDoiHint");
    hint.hidden = miss === 0;
    if (miss) hint.innerHTML = `${esc(t("recent.doi_hint", { n: nf.format(miss) }))} <a href="#" class="ext-link" data-act="missing-doi">${esc(t("recent.doi_link"))}</a>`;
  }

  // ------------------------------------------------------------ OpenAlex daily budget notice
  /** "'te" / "'de" / "'ta" / "'da" for a clock time as it is spoken (03:00 -> 'te, 01:00 -> 'de, 06:00 -> 'da). */
  function trLocative(hh, mm) {
    const n = mm !== 0 ? mm : hh;
    const unit = { 1: "de", 2: "de", 3: "te", 4: "te", 5: "te", 6: "da", 7: "de", 8: "de", 9: "da" };
    const tens = { 0: "da", 1: "da", 2: "de", 3: "da", 4: "ta", 5: "de" };
    return "'" + (n % 10 ? unit[n % 10] : tens[n / 10]);
  }
  /** Local clock time of an instant plus the Turkish suffix when the language is Turkish. */
  function clockOf(d) {
    const text = d.toLocaleTimeString(loc(), { hour: "2-digit", minute: "2-digit" });
    return { time: text, sfx: window.I18N.lang === "tr" ? trLocative(d.getHours(), d.getMinutes()) : "" };
  }
  function tzLabel(d) {
    if (window.I18N.lang === "tr" && d.getTimezoneOffset() === -180) return "TSİ";
    const part = new Intl.DateTimeFormat(loc(), { timeZoneName: "short" }).formatToParts(d).find((x) => x.type === "timeZoneName");
    return part ? part.value : "";
  }
  /** The active pause (its resume time still ahead), or null. */
  const activeBudget = () => (state.budget && new Date(state.budget.resume_at).getTime() > Date.now() ? state.budget : null);

  function budgetInfo(b) {
    const resume = clockOf(new Date(b.resume_at));
    const reset = new Date(b.resets_at);
    const r = clockOf(reset);
    return {
      main: b.has_key ? t("budget.key", { time: resume.time, sfx: resume.sfx }) : t("budget.keyless"),
      line: t("budget.reset", { time: r.time, sfx: r.sfx, tz: tzLabel(reset) }),
    };
  }
  function budgetButtons(b) {
    const btn = (act, label, icon) => `<button type="button" class="btn" data-budget="${act}"><span class="icon">${icon}</span><span>${esc(t(label))}</span></button>`;
    return (b.has_key ? "" : btn("key", "budget.get_key", "&#xE8A7;") + btn("mail", "budget.mail", "&#xE715;")) + btn("settings", "budget.settings", "&#xE713;");
  }

  function renderBudget() {
    const b = activeBudget();
    const info = b ? budgetInfo(b) : null;
    // dismissible bar on Panel, Analysis and Citing works
    const show = !!b && ["panel", "analysis", "citing"].includes(state.page) && state.budgetDismissed !== b.resume_at;
    $("#budgetBar").hidden = !show;
    if (show) {
      $("#budgetText").textContent = info.main;
      $("#budgetReset").textContent = info.line;
      $("#budgetBtns").innerHTML = budgetButtons(b);
    }
    // scan side panel
    const sp = $("#scanBudget");
    sp.hidden = !b;
    if (b) sp.innerHTML = `<div class="body-strong">${esc(info.main)}</div><div class="bn-line">${esc(info.line)}</div><div class="sbtns">${budgetButtons(b)}</div>`;
    // Settings > API keys card
    const kb = $("#keyBudget");
    kb.hidden = !b;
    if (b) kb.textContent = info.main + " " + info.line;
  }

  function budgetAction(act) {
    if (act === "key") safe(() => invoke("open_url", { url: "https://openalex.org/" }));
    else if (act === "settings" || act === "mail") {
      go(() => {
        closeScan();
        showPage("settings");
        const target = act === "mail" ? $("#keyOaEmail") : $("#keyOa");
        setTimeout(() => { target.scrollIntoView({ block: "center" }); target.focus(); }, 100);
      });
    }
  }

  // ------------------------------------------------------------ historical scan panel
  const newScan = () => ({
    running: false, mode: "", n: 0, k: 0, done: 0, elapsed: 0, eta: null, recvAt: 0,
    articles: new Map(), live: new Map(), log: [], summary: null, end: "", message: "", stopping: false, lastWait: "",
  });

  const fmtDur = (s) => {
    s = Math.max(0, Math.round(s));
    const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60), x = s % 60;
    return h ? `${h}:${pad2(m)}:${pad2(x)}` : `${m}:${pad2(x)}`;
  };
  const clockTime = () => new Date().toLocaleTimeString(loc(), { hour: "2-digit", minute: "2-digit", second: "2-digit" });

  /** Plain-language text of one live step (a request or a rate-limit wait). */
  function liveText(e) {
    const p = { src: "OpenAlex", from: t("scan.from_oa"), page: e.page, records: nf.format(e.records || 0), secs: e.wait_secs, attempt: e.attempt };
    if (e.event === "backoff") return t("scan.wait", p);
    if (e.what === "author") return t("scan.author", p);
    if (e.what === "works") return t("scan.works", p);
    if (e.what === "lookup") return t("scan.lookup", p);
    return t("scan.citing", p);
  }

  function pushLog(text) {
    const sc = state.scan;
    sc.log.push({ time: clockTime(), text });
    if (sc.log.length > 30) sc.log.splice(0, sc.log.length - 30);
  }

  /** Folds one `backfill-progress` event into the scan model. `replay` suppresses side effects. */
  function applyScan(e, replay) {
    let sc = state.scan;
    if (e.event === "preparing") { sc = state.scan = newScan(); sc.running = true; sc.mode = e.mode; }
    sc.mode = e.mode || sc.mode;
    sc.n = e.n || sc.n;
    sc.k = e.k || sc.k;
    sc.done = e.done ?? sc.done;
    sc.elapsed = e.elapsed_secs || 0;
    sc.eta = e.eta_secs ?? null;
    sc.recvAt = Date.now();
    const art = (key) => sc.articles.get(key);
    switch (e.event) {
      case "preparing": pushLog(t("scan.log_prepare")); break;
      case "start":
        sc.running = true;
        sc.articles = new Map((e.targets || []).map((x) => [x.key, { title: x.title, status: "queued" }]));
        pushLog(t("scan.log_start", { n: nf.format(sc.n) }));
        if (!replay && state.scanUserStart && sc.mode !== "refresh") openScan();
        state.scanUserStart = false;
        break;
      case "request": case "backoff":
        sc.live.set(e.source, e);
        if (e.event === "backoff" && sc.lastWait !== `${e.source}${e.attempt}`) {
          sc.lastWait = `${e.source}${e.attempt}`;
          pushLog(t("scan.log_wait", { src: srcName(e.source), secs: e.wait_secs, attempt: e.attempt }));
        }
        break;
      case "works": pushLog(t("scan.log_works", { n: nf.format(e.works) })); break;
      case "matching": pushLog(t("scan.log_matching", { works: nf.format(e.works), n: nf.format(e.records) })); break;
      case "article_start":
        sc.live.clear();
        if (art(e.key)) art(e.key).status = "running";
        pushLog(t("scan.log_article", { k: e.k, n: sc.n, title: shortText(e.title, 60) }));
        break;
      case "article_done": case "unmatched": case "error": {
        sc.live.clear();
        const a = art(e.key);
        if (a) {
          a.status = e.event === "article_done" ? "done" : e.event === "unmatched" ? "unmatched" : "error";
          Object.assign(a, { oa: e.oa_count, method: e.method, records: e.records, message: e.message });
        }
        pushLog(t(`scan.log_${e.event}`, { title: shortText(e.title, 50), n: nf.format(e.records || 0), message: e.message || "" }));
        break;
      }
      case "finished": case "cancelled": case "failed":
        sc.running = false;
        sc.stopping = false;
        sc.live.clear();
        sc.end = e.event;
        sc.summary = e.summary || null;
        sc.message = e.message || "";
        if (e.event === "failed") pushLog(t("scan.log_failed", { message: e.message || "" }));
        else pushLog(t(e.event === "cancelled" ? "scan.log_cancelled" : "scan.log_finished"));
        break;
      default: break;
    }
    state.bfProgress = e;
  }

  const STATUS_ICON = { queued: "", running: "", done: "&#xE73E;", unmatched: "&#xE7BA;", error: "&#xE711;" };

  function articleResult(a) {
    if (a.status === "queued") return t("scan.st_queued");
    if (a.status === "running") return t("scan.st_running");
    if (a.status === "unmatched") return t("scan.st_unmatched");
    if (a.status === "error") return t("scan.st_error", { message: shortText(a.message || "?", 80) });
    const c = (n) => (n === null || n === undefined ? "—" : nf.format(n));
    return `OpenAlex ${c(a.oa)}`;
  }

  function renderScanTime() {
    const sc = state.scan;
    if (!sc.running) return;
    const extra = (Date.now() - sc.recvAt) / 1000;
    const el = sc.elapsed + extra;
    const eta = sc.eta === null ? t("scan.eta_pending") : t("scan.eta", { time: fmtDur(Math.max(0, sc.eta - extra)) });
    $("#scanTime").textContent = `${t("scan.elapsed", { time: fmtDur(el) })} · ${eta}`;
  }

  function renderScan() {
    const sc = state.scan;
    const panel = $("#scanPanel");
    panel.classList.toggle("open", state.scanOpen);
    panel.setAttribute("aria-hidden", state.scanOpen ? "false" : "true");
    if (!state.scanOpen) return;
    const running = sc.running;
    const idle = !running && !sc.end;
    $("#scanRun").hidden = !running && !!sc.end;
    $("#scanCount").textContent = sc.n ? t("scan.count", { k: nf.format(running ? Math.max(sc.k, 1) : sc.done), n: nf.format(sc.n) }) : t(idle ? "scan.idle" : "scan.preparing");
    $("#scanBar").style.width = sc.n ? Math.round((sc.done / sc.n) * 100) + "%" : "0%";
    $("#scanBar").classList.toggle("indet", running && !sc.n);
    if (running) renderScanTime(); else $("#scanTime").textContent = "";
    const live = Array.from(sc.live.values());
    $("#scanNow").innerHTML = running
      ? (live.length ? live.map((e) => `<div class="scan-step ${e.event === "backoff" ? "wait" : ""}">${esc(liveText(e))}</div>`).join("") : `<div class="scan-step">${esc(t("scan.preparing"))}</div>`)
      : "";
    // summary
    const sum = $("#scanSummary");
    sum.hidden = running || !sc.end;
    if (!running && sc.end) {
      const s = sc.summary;
      const head = sc.end === "failed" ? t("scan.sum_failed", { message: sc.message })
        : s && s.paused_budget ? t("scan.sum_paused") : t(sc.end === "cancelled" ? "scan.sum_cancelled" : "scan.sum_head");
      let body = "";
      if (s) {
        body = `<div>${esc(t("scan.sum_line", { matched: nf.format(s.matched), total: nf.format(s.total), records: nf.format(s.records), added: nf.format(s.new_total), time: fmtDur(state.bfProgress?.elapsed_secs || sc.elapsed) }))}</div>`;
      } else if (sc.end === "finished") body = `<div>${esc(t("scan.sum_nothing"))}</div>`;
      const miss = missingDoi().length;
      if (miss > 0 && sc.end !== "failed") body += `<div class="scan-doi">${esc(t("scan.sum_doi", { n: nf.format(miss) }))} <a href="#" class="ext-link" data-act="missing-doi">${esc(t("scan.sum_doi_link"))}</a></div>`;
      sum.innerHTML = `<div class="body-strong">${esc(head)}</div>${body}`;
    }
    // per article
    $("#scanList").innerHTML = Array.from(sc.articles.entries()).map(([key, a]) => `<li class="scan-item st-${a.status}" data-key="${esc(key)}">
        <span class="scan-ic">${a.status === "running" ? `<i class="ring"></i>` : a.status === "queued" ? `<i class="circle"></i>` : `<span class="icon">${STATUS_ICON[a.status]}</span>`}</span>
        <div class="scan-txt"><div class="scan-title" title="${esc(a.title)}">${esc(a.title)}</div><div class="caption secondary scan-res">${esc(articleResult(a))}</div></div></li>`).join("");
    $("#scanLog").innerHTML = sc.log.slice().reverse().map((l) => `<li><span class="log-t">${esc(l.time)}</span> ${esc(l.text)}</li>`).join("");
    const stop = $("#scanStop");
    stop.hidden = !running;
    stop.disabled = sc.stopping;
    $("#scanStopText").textContent = t(sc.stopping ? "scan.stopping" : "scan.stop");
  }

  function openScan() { state.scanOpen = true; renderScan(); $("#scanClose").focus({ preventScroll: true }); }
  function closeScan() { state.scanOpen = false; renderScan(); }
  async function stopScan() {
    state.scan.stopping = true;
    renderScan();
    await safe(() => invoke("cancel_backfill"), false);
  }
  /** Rebuilds the model from the events the backend kept (after a reload or when the panel is reopened). */
  async function hydrateScan() {
    const st = await safe(() => invoke("get_scan_state"), null);
    if (!st || !st.events.length) return;
    state.scan = newScan();
    for (const e of st.events) applyScan(e, true);
    for (const e of st.live) applyScan(e, true);
    state.scan.live = new Map(st.live.map((e) => [e.source, e]));
    state.bfProgress = state.scan.running ? st.live[0] || st.events[st.events.length - 1] : state.bfProgress;
  }
  function scanLogEvent(e) {
    applyScan(e, false);
    renderHeader();
    renderBackfill();
    renderScan();
  }

  // ------------------------------------------------------------ settings
  function showInfo(text, err) {
    const bar = $("#infoBar");
    bar.textContent = text;
    bar.className = "infobar" + (err ? " err" : "");
    bar.hidden = false;
  }
  function fillSettings() {
    const s = state.settings;
    if (!s) return;
    $("#setProfile").value = s.profile_id;
    $("#setOrcid").value = s.orcid || "";
    $("#setBackfill").checked = !!s.backfill_enabled;
    $("#setInterval").value = s.interval_hours;
    $("#setLang").value = window.I18N.lang;
    $("#setTheme").value = window.SCTTheme.pref;
    $("#setBackdrop").checked = s.backdrop !== false;
    $("#setAutostart").checked = s.autostart;
    $("#setNotify").checked = s.notify;
    $("#setWeekly").checked = s.weekly_summary !== false;
    $("#setAutoUpdate").checked = s.auto_update !== false;
    $("#setDirText").textContent = state.dirPending ?? s.data_dir;
  }
  async function saveSettings() {
    const profileId = extractProfileId($("#setProfile").value);
    $("#setProfile").value = profileId;
    const orcid = extractOrcid($("#setOrcid").value);
    $("#setOrcid").value = orcid;
    if (!orcid) { showInfo(t("setup.err_orcid_empty"), true); return; }
    if (!validOrcid(orcid)) { showInfo(t("setup.err_orcid_invalid"), true); return; }
    const s = {
      profile_id: profileId,
      orcid,
      backfill_enabled: !!state.settings.backfill_enabled,
      language: $("#setLang").value,
      theme: window.SCTTheme.pref,
      data_dir: state.dirPending ?? state.settings.data_dir,
      interval_hours: Math.max(1, Math.min(24, parseInt($("#setInterval").value, 10) || 3)),
      autostart: $("#setAutostart").checked,
      notify: $("#setNotify").checked,
      weekly_summary: $("#setWeekly").checked,
      auto_update: $("#setAutoUpdate").checked,
    };
    const hadProfile = !!state.settings.profile_id;
    const profileChanged = s.profile_id !== state.settings.profile_id;
    const needsConfirm = profileChanged && hadProfile;
    const oldOrcid = state.settings.orcid;
    const orcidChanged = !needsConfirm && !!oldOrcid && s.orcid !== oldOrcid;
    if (needsConfirm) {
      const ok = await window.__TAURI__.dialog.ask(
        t("dialog.message", { old: state.settings.profile_id, new: s.profile_id }),
        { title: t("dialog.title"), kind: "warning", okLabel: t("dialog.ok"), cancelLabel: t("dialog.cancel") }
      );
      if (!ok) { $("#setProfile").value = state.settings.profile_id; return; }
    } else if (orcidChanged) {
      const ok = await window.__TAURI__.dialog.ask(
        t("dialog.orcid_message", { old: oldOrcid, new: s.orcid }),
        { title: t("dialog.orcid_title"), kind: "warning", okLabel: t("dialog.orcid_ok"), cancelLabel: t("dialog.cancel") }
      );
      if (!ok) { $("#setOrcid").value = oldOrcid; return; }
    }
    try {
      const res = await invoke("save_settings", { settings: s, clearData: needsConfirm });
      state.settings = res.settings;
      state.dirPending = null;
      applyLang(res.settings.language);
      fillSettings();
      showInfo(res.message, false);
      // first profile entered in Settings: fetch right away
      if (!hadProfile && res.settings.profile_id) await refreshNow();
      await Promise.all([loadStatus(), refreshAll()]);
      // a confirmed profile change wiped everything: let the user choose about the history again
      if (needsConfirm || (!hadProfile && res.settings.profile_id)) await offerBackfill();
    } catch (e) {
      showInfo(String(e), true);
    }
  }
  async function exportCsv(kind) {
    try {
      const path = await invoke("export_csv", { kind });
      if (path) showInfo(t("settings.exported", { path }), false);
    } catch (e) { showInfo(String(e), true); }
  }

  // ------------------------------------------------------------ historical data
  const RUN_ST = { paused_budget: "bf.st_paused_budget", ok: "bf.st_ok", partial: "bf.st_partial", error: "bf.st_error", interrupted: "bf.st_interrupted", running: "bf.st_running", cancelled: "bf.st_cancelled" };

  function renderBackfill() {
    const info = state.bfInfo;
    const p = state.bfProgress;
    const running = !!p?.running;
    $("#setBackfill").checked = !!state.settings?.backfill_enabled;
    const sum = info?.summary;
    const last = sum?.last_run;
    let line = t("bf.status_never");
    if (last) {
      line = t("bf.status_line", {
        time: fmtDateTime(last.finished_at || last.started_at),
        status: t(RUN_ST[last.status] || "bf.st_ok"),
        matched: nf.format(sum.matched),
        total: nf.format(sum.total_articles),
        records: nf.format(sum.records),
      });
    }
    $("#bfStatusLine").textContent = line;
    const extra = [];
    if (sum && (sum.unmatched > 0 || sum.pending > 0)) {
      extra.push(t("bf.status_extra", { unmatched: nf.format(sum.unmatched), pending: nf.format(sum.pending) }));
    }
    if (info && !info.orcid_set) extra.push(t("bf.need_orcid"));
    $("#bfStatusExtra").textContent = extra.join(" · ");
    const msg = $("#bfStatusMsg");
    msg.textContent = last?.message || "";
    msg.classList.toggle("bf-err", !!last && (last.status === "error" || last.status === "partial"));

    $("#bfProgress").hidden = !running;
    if (running) {
      const pct = p.n > 0 ? Math.round(((p.done || 0) / p.n) * 100) : 0;
      $("#bfBarFill").style.width = pct + "%";
      $("#bfProgressText").textContent = p.k > 0
        ? t("bf.progress", { k: p.k, n: p.n, source: p.source || "…" })
        : t("bf.progress_author");
    }
    $("#bfRunBtn").disabled = running || (info ? !info.orcid_set : false);
    $("#bfClearBtn").disabled = running;
    $("#setBackfill").disabled = false;
  }

  const BF_STATUS_TEXT = { started: ["bf.started", false], running: ["bf.already", false], no_orcid: ["bf.need_orcid", true], no_data: ["bf.no_data", true] };
  function showBfStatus(status) {
    const e = BF_STATUS_TEXT[status];
    if (e) showInfo(t(e[0]), e[1]);
  }

  async function setBackfillEnabled(enabled) {
    state.scanUserStart = enabled;
    try {
      const res = await invoke("set_backfill_enabled", { enabled });
      state.settings = res.settings;
      if (res.status !== "started") state.scanUserStart = false;
      showBfStatus(res.status);
    } catch (e) {
      state.scanUserStart = false;
      showInfo(String(e), true);
    }
    await loadBfInfo();
  }

  async function runBackfillNow() {
    state.scanUserStart = true;
    try {
      const status = await invoke("backfill_run_now");
      if (status !== "started") state.scanUserStart = false;
      showBfStatus(status);
    } catch (e) { state.scanUserStart = false; showInfo(String(e), true); }
    await loadBfInfo();
  }

  async function clearBackfill() {
    const ok = await window.__TAURI__.dialog.ask(t("bf.clear_confirm"), {
      title: t("bf.clear_title"), kind: "warning", okLabel: t("bf.clear_ok"), cancelLabel: t("dialog.cancel"),
    });
    if (!ok) return;
    try {
      await invoke("clear_backfill_data");
      showInfo(t("bf.cleared"), false);
    } catch (e) { showInfo(String(e), true); }
    state.bf.clear();
    await Promise.all([loadBfInfo(), loadTrend(), state.analysis ? loadAnalysis() : null]);
    state.open.forEach(drawDetail);
  }

  /** Fluent choice dialog: resolves true for "look up history too", false for "start from today". */
  function askBackfill() {
    return new Promise((resolve) => {
      const dlg = $("#bfDialog");
      const yes = $("#bfDlgYes"), no = $("#bfDlgNo");
      const done = (v) => {
        dlg.hidden = true;
        yes.onclick = no.onclick = null;
        document.removeEventListener("keydown", onKey, true);
        resolve(v);
      };
      const onKey = (e) => { if (e.key === "Escape") { e.preventDefault(); done(false); } };
      document.addEventListener("keydown", onKey, true);
      yes.onclick = () => done(true);
      no.onclick = () => done(false);
      dlg.hidden = false;
      yes.focus();
    });
  }
  async function offerBackfill() {
    const yes = await askBackfill();
    await setBackfillEnabled(yes);
  }

  // ------------------------------------------------------------ first-run setup
  function setupError(text) {
    const el = $("#setupError");
    el.textContent = text || "";
    el.hidden = !text;
  }
  async function startSetup(ev) {
    ev.preventDefault();
    setupError("");
    const orcidOnly = !noProfile() && needOrcid();
    const orcid = extractOrcid($("#setupOrcid").value);
    if (!orcidOnly) {
      const id = extractProfileId($("#setupProfile").value);
      if (!id) { setupError(t("setup.err_empty")); return; }
      if (!validProfileId(id)) { setupError(t("setup.err_invalid")); return; }
    }
    if (!orcid) { setupError(t("setup.err_orcid_empty")); return; }
    if (!validOrcid(orcid)) { setupError(t("setup.err_orcid_invalid")); return; }
    const btn = $("#startBtn");
    btn.disabled = true;
    try {
      const s = orcidOnly
        ? { ...state.settings, orcid }
        : { ...state.settings, profile_id: extractProfileId($("#setupProfile").value), orcid, language: $("#setupLang").value };
      const res = await invoke("save_settings", { settings: s, clearData: false });
      state.settings = res.settings;
      applyLang(res.settings.language);
      if (!orcidOnly) await refreshNow();
      await Promise.all([loadStatus(), refreshAll()]);
      $("#setupOrcid").value = "";
      await offerBackfill();
    } catch (e) {
      setupError(String(e));
    } finally {
      btn.disabled = false;
      updateSetupValidity();
    }
  }

  // ------------------------------------------------------------ Analysis page
  async function loadRisers() { state.risers = await safe(() => invoke("get_risers", { days: state.risDays }), state.risers); renderRisersChart(); }
  async function loadNext() { state.next = await safe(() => invoke("get_next_steps"), state.next); renderPanel(); renderNextLists(); }

  /** The period selector of the page as {from, to} in ms (null: open end). A custom end in the future means "until today". */
  function anRange() {
    const r = state.an;
    const now = Date.now();
    if (r.range === "30d") return { from: now - 30 * DAY_MS, to: null };
    if (r.range === "90d") return { from: now - 90 * DAY_MS, to: null };
    if (r.range === "12m") return { from: now - 365 * DAY_MS, to: null };
    if (r.range === "all") return { from: null, to: null };
    const from = r.from ? new Date(r.from + "T00:00:00").getTime() : null;
    let to = r.to ? new Date(r.to + "T23:59:59").getTime() : null;
    if (to !== null && to >= now - DAY_MS) to = null;
    return { from, to };
  }

  const hasHistory = () => !!state.analysis && state.analysis.history_year.total > 0;
  /** Why a chart that depends on the historical lookup is empty. */
  const needHistoryText = () => t(state.settings?.backfill_enabled ? "an.no_history" : "an.need_backfill");

  function renderAnalysis() {
    if (state.page !== "analysis") return;
    const a = state.analysis;
    const has = !!a && a.trend.has_data;
    $("#analysisEmpty").hidden = has || a === null;
    $("#analysisData").hidden = !has;
    if (!has) { Charts.disposeIn($("#page-analysis")); return; }
    $$("#anRange button").forEach((b) => b.classList.toggle("on", b.dataset.range === state.an.range));
    $("#anCustom").hidden = state.an.range !== "custom";
    $("#anFrom").value = state.an.from;
    $("#anTo").value = state.an.to;
    $$("#anTabs .tab").forEach((tb) => {
      const on = tb.dataset.tab === state.an.tab;
      tb.setAttribute("aria-selected", on ? "true" : "false");
      tb.tabIndex = on ? 0 : -1;
    });
    $$(".an-tab").forEach((p) => {
      const on = p.id === "anTab-" + state.an.tab;
      if (!on) Charts.disposeIn(p);
      p.hidden = !on;
    });
    Citers.mountRange(state.an.tab === "citers");
    if (state.an.tab !== "citers") Citers.leave();
    if (state.an.tab !== "calendar") window.YearHeat.dispose($("#yearHeat"));
    if (!state.citingLoaded) loadCiting();
    ({ trend: renderTrendTab, articles: renderArticlesTab, indices: renderIndicesTab, calendar: renderCalendarTab, citers: () => Citers.render() }[state.an.tab])();
  }

  // ---- trend tab
  function renderTrendTab() {
    const a = state.analysis;
    const tr = a.trend;
    const range = anRange();
    const c = Charts.ctx({ range });
    const built = Charts.build.trend(a, c);
    Charts.mount($("#chTrend"), built, { emptyText: t("chart.no_data_range") });
    const parts = [t(tr.hist.length ? "trend.cap_hist" : "trend.cap_scholar")];
    if (tr.merge && tr.merge.year_only > 0 && tr.hist.length) parts.push(t("trend.cap_year_only", { n: nf.format(tr.merge.year_only) }));
    if (tr.forecast_hidden) {
      const [have, need] = tr.forecast_hidden;
      parts.push(t("trend.fc_hidden", { need, have, left: Math.max(1, need - have) }));
      if (tr.oa_per_month != null && tr.oa_latest) parts.push(t("trend.oa_rate", { n: nf.format(Math.round(tr.oa_per_month * 10) / 10), date: fmtCitingDate(tr.oa_latest, "day") }));
    } else if (tr.forecast) {
      parts.push(t(tr.forecast.thin ? "trend.fc_thin" : "trend.fc_basis"));
    }
    $("#trendCaption").textContent = parts.join(" ");
    const f = tr.forecast;
    const yearEnd = f
      ? `≈ ${nf.format(f.mid)} <span class="fc-sub">(${nf.format(f.low)}–${nf.format(f.high)})</span>`
      : "—";
    const last12Note = tr.last12 !== null && tr.last12_days < 360 ? ` <span class="fc-sub">(${esc(t("trend.last_days", { n: tr.last12_days }))})</span>` : "";
    $("#trendStats").innerHTML = `
      <div class="fc-stat"><div class="fc-big">${yearEnd}</div><div class="caption secondary">${esc(t("fc.year_end", { year: tr.year }))}</div></div>
      <div class="fc-stat"><div class="fc-big">${tr.monthly_avg === null ? "—" : nf.format(Math.round(tr.monthly_avg * 10) / 10)}</div><div class="caption secondary">${esc(t("fc.per_month"))}</div></div>
      <div class="fc-stat"><div class="fc-big">${tr.last12 === null ? "—" : nf.format(tr.last12)}${last12Note}</div><div class="caption secondary">${esc(t("fc.last12"))}</div></div>`;
    const input = $("#fcGoal");
    if (document.activeElement !== input) input.value = tr.goal ?? "";
    const g = $("#fcGoalText");
    g.className = "fc-goal-text" + (tr.goal_status === "reached" ? " pos" : "");
    if (tr.goal_status === "none") g.textContent = t("fc.goal_none");
    else if (tr.goal_status === "reached") g.textContent = t("fc.goal_reached", { goal: nf.format(tr.goal) });
    else if (tr.goal_status === "month") g.textContent = t("fc.goal_eta", { goal: nf.format(tr.goal), date: monthLabel(tr.goal_ym, true) });
    else g.textContent = t("fc.goal_unknown");
    renderPeriodChart();
  }

  async function saveGoal() {
    const raw = $("#fcGoal").value.trim();
    const n = raw === "" ? null : Math.floor(Number(raw));
    if (n !== null && !(n > 0)) { toast(t("fc.goal_err"), true); return; }
    try {
      await invoke("set_goal", { goal: n });
      await loadAnalysis();
    } catch (e) { toast(String(e), true); }
  }

  function renderPeriodChart() {
    const a = state.analysis;
    const hist = hasHistory();
    if (!hist && state.an.gran !== "year") state.an.gran = "year";
    $$("#anGran button").forEach((b) => {
      const off = !hist && b.dataset.gran !== "year";
      b.classList.toggle("on", b.dataset.gran === state.an.gran);
      b.disabled = off;
      b.title = off ? t("main.gran_disabled") : "";
    });
    const c = Charts.ctx({ range: anRange(), gran: state.an.gran, onPick: pickPeriod, oaLatest: a.trend.oa_latest });
    const built = Charts.build.period(a, c);
    Charts.mount($("#chPeriod"), built, { emptyText: hist ? t("chart.no_data_range") : needHistoryText() });
    const cap = $("#periodCaption");
    if (!built) { cap.textContent = ""; return; }
    let text = t(built.meta.source === "scholar" ? "an.cap_period_scholar" : "an.cap_period");
    if (built.meta.source === "openalex" && built.meta.excluded > 0 && state.an.gran !== "year") text += " " + t("main.note_excluded", { n: nf.format(built.meta.excluded) });
    if (built.meta.source === "openalex") {
      if (built.meta.tail > 0) text += " " + t("an.lag_tail_note", { date: fmtDayMonth(a.trend.oa_latest) });
      text += " " + lagHint();
    }
    cap.textContent = text;
  }

  /** "OpenAlex's newest citing work: 14 Oct 2026 - new papers are usually added weeks late." */
  function lagHint() {
    const d = state.analysis?.trend.oa_latest;
    return d ? t("ris.lag", { date: fmtCitingDate(d, "day") }) : t("ris.lag_none");
  }

  /** A bar was clicked: select that period in the calendar (a day, a whole month or a whole year). */
  function pickPeriod(per, gran) {
    const cal = state.cal;
    if (gran === "day") { openCalendarDay(per.key); return; }
    const first = gran === "year" ? `${per.key}-01-01` : `${per.key}-01`;
    const last = gran === "year" ? `${per.key}-12-31` : monthRange(per.key)[1];
    cal.month = first.slice(0, 7);
    cal.sel = { from: first, to: last, whole: true };
    cal.focus = first;
    cal.year = null;
    cal.shown = 120;
    state.an.tab = "calendar";
    renderAnalysis();
    const card = $("#calDetail");
    if (card) card.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }

  // ---- articles tab
  function renderArticlesTab() {
    renderGapList();
    $$("#risSeg button").forEach((b) => b.classList.toggle("on", Number(b.dataset.days) === state.risDays));
    if (!state.risers) loadRisers(); else renderRisersChart();
    const a = state.analysis;
    const c = Charts.ctx({ range: anRange() });
    const built = Charts.build.stacked(a, c);
    Charts.mount($("#chStacked"), built, { emptyText: hasHistory() ? t("chart.no_data_range") : needHistoryText() });
    const cap = $("#stackedCaption");
    if (!built) { cap.textContent = ""; return; }
    let text = t("an.cap_stacked");
    if (a.stacked.year_only > 0) text += " " + t("an.cap_stacked_year", { n: nf.format(a.stacked.year_only) });
    cap.textContent = text;
  }

  function renderRisersChart() {
    if (state.page !== "analysis" || state.an.tab !== "articles") return;
    const r = state.risers;
    const days = state.risDays;
    const el = $("#chRisers"), elOa = $("#chRisersOa");
    const fit = (box, n) => { box.style.height = n ? Math.max(120, n * 38 + 20) + "px" : ""; };
    const lag = (r && r.oa_latest) ? t("ris.lag", { date: fmtCitingDate(r.oa_latest, "day") }) : t("ris.lag_none");
    const onPick = (x) => openArticle(x.key);
    if (!r) { Charts.mount(el, null); $("#risEmpty").hidden = true; return; }
    const snapItems = r.items.length > 0;
    const oaItems = r.oa_items.length > 0;
    // Scholar block: what the snapshots say, with the days they cover
    let head = "";
    if (r.basis === "snapshots") head = t("ris.snap_full", { days, total: signed(r.snap_total) });
    else if (r.basis === "snapshots_partial") head = t("ris.snap_partial", { covered: r.covered_days, total: signed(r.snap_total) });
    $("#risScholarHead").textContent = head;
    $("#risScholarHead").hidden = !head;
    fit(el, snapItems ? r.items.length : 0);
    el.hidden = !snapItems;
    Charts.mount(el, snapItems ? Charts.build.risers(r.items, Charts.ctx({ onPick })) : null);
    $("#risCaption").textContent = r.basis === "none" ? t("ris.snap_none") : snapItems ? "" : t("ris.snap_empty");
    // OpenAlex block, apart and always labelled; shown when Scholar cannot cover the window or has nothing to say
    const showOa = oaItems && (r.basis !== "snapshots" || !snapItems);
    $("#risOa").hidden = !showOa;
    $("#risOaHead").textContent = t("ris.oa_head");
    fit(elOa, showOa ? r.oa_items.length : 0);
    Charts.mount(elOa, showOa ? Charts.build.risers(r.oa_items, Charts.ctx({ onPick })) : null);
    // the lag hint wherever OpenAlex dates are used (or would be)
    $("#risLag").hidden = !(showOa || r.basis !== "snapshots" || (!snapItems && !oaItems));
    $("#risLag").textContent = lag;
    // never just "nothing": say why and what to try
    const empty = !snapItems && !oaItems;
    const why = [];
    if (empty) {
      why.push(r.basis === "none" ? t("ris.why_scholar_none") : r.basis === "snapshots_partial" ? t("ris.why_scholar", { covered: r.covered_days }) : t("ris.why_scholar_flat", { days }));
      why.push(r.oa_latest ? t("ris.why_oa", { date: fmtCitingDate(r.oa_latest, "day") }) : t("ris.why_oa_none"));
    }
    const box = $("#risEmpty");
    box.hidden = !empty;
    if (empty) box.textContent = t("ris.empty_reason", { why: why.join("; ") }) + " " + t(days === 30 ? "ris.try_wider" : "ris.try_later");
  }

  // ---- indices tab
  /** "h=5 for: 2 articles need 3 more citations" plus every article still below the threshold, closest first. */
  function nextColumn(label, s) {
    let line;
    if (s.picks.length) {
      line = t("hn.line", { label, papers: nf.format(s.picks.length), paper_word: pl(s.picks.length, "unit.article"), total: nf.format(s.total_needed), cite_word: pl(s.total_needed, "unit.citation") });
      if (s.new_papers > 0) line += " " + t("hn.plus_new", { n: nf.format(s.new_papers), paper_word: pl(s.new_papers, "unit.article") });
    } else {
      line = t("hn.new_only", { label, n: nf.format(s.new_papers), paper_word: pl(s.new_papers, "unit.article") });
    }
    const picks = new Set(s.picks.map((p) => p.key));
    const list = s.candidates.length
      ? `<ul class="cand-list">${s.candidates.map((p) => `<li class="${picks.has(p.key) ? "pick" : ""}"><a href="#" class="cand-t title-link" data-open-article="${esc(p.key)}" title="${esc(p.title)}">${esc(p.title)}</a><span class="chip"><b>${nf.format(p.citations)}</b> → +${nf.format(p.needs)}</span></li>`).join("")}</ul>`
      : "";
    return `<div class="next-col"><h3>${esc(line)}</h3><div class="caption secondary">${esc(t("an.next_hint", { th: s.threshold }))}</div>${list}</div>`;
  }

  function renderNextLists() {
    if (state.page !== "analysis" || state.an.tab !== "indices") return;
    const n = state.analysis?.next;
    $("#nextLists").innerHTML = n ? nextColumn(`h=${n.h.target}`, n.h) + nextColumn(`i10=${n.i10.target}`, n.i10) : "";
  }

  function renderIndicesTab() {
    const a = state.analysis;
    const c = Charts.ctx({ range: anRange() });
    const few = a.indices.points.length < 2;
    const built = few ? null : Charts.build.indices(a, c);
    const checks = state.overview?.snapshot_count || 0;
    Charts.mount($("#chIndex"), built, { emptyText: t("an.idx_need", { n: Math.max(1, 2 - checks) }) });
    $("#indexCaption").textContent = built ? t("an.cap_index") : "";
    renderNextLists();
  }

  // ---- calendar tab: the main way to explore citations (which article was cited on which day)
  /** Date -> {n, titles} of the citing works that were published on that day (for the year heat map tooltip). */
  let dayInfoFor = null, dayInfoCache = null;
  function dayInfo() {
    if (dayInfoFor === state.citingGroups && dayInfoCache) return dayInfoCache;
    const map = new Map();
    for (const g of state.citingGroups || []) {
      if (g.precision !== "day") continue;
      const e = map.get(g.date) || map.set(g.date, { n: 0, titles: [] }).get(g.date);
      e.n++;
      e.titles.push(g.title || "—");
    }
    dayInfoFor = state.citingGroups;
    dayInfoCache = map;
    return map;
  }

  /** [start, end) of the period a citing work's date names (a month or a year when the day is unknown). */
  function dateSpan(date, precision) {
    const [y, m, d] = date.split("-").map(Number);
    if (precision === "day") { const s = Date.UTC(y, m - 1, d); return [s, s + DAY_MS]; }
    if (precision === "month") return [Date.UTC(y, m - 1, 1), Date.UTC(y, m, 1)];
    return [Date.UTC(y, 0, 1), Date.UTC(y + 1, 0, 1)];
  }
  const spanTouches = (g, from, to) => {
    const [s, e] = dateSpan(g.date, g.precision);
    return (from == null || e > from) && (to == null || s <= to);
  };

  const keyMsOf = (iso) => Charts.util.keyMs(iso);
  const monthRange = (ym) => {
    const [y, m] = ym.split("-").map(Number);
    return [`${ym}-01`, `${ym}-${pad2(new Date(Date.UTC(y, m, 0)).getUTCDate())}`];
  };
  const addDays = (iso, n) => {
    const d = new Date(keyMsOf(iso) + n * DAY_MS);
    return `${d.getUTCFullYear()}-${pad2(d.getUTCMonth() + 1)}-${pad2(d.getUTCDate())}`;
  };
  const longDay = (iso) => new Date(keyMsOf(iso)).toLocaleDateString(loc(), { weekday: "long", day: "numeric", month: "long", year: "numeric", timeZone: "UTC" });
  const shortDay = (iso) => new Date(keyMsOf(iso)).toLocaleDateString(loc(), { day: "numeric", month: "short", year: "numeric", timeZone: "UTC" });
  const todayIso = () => isoDay(new Date());
  const calSig = () => `${state.cal.source}|${state.cal.article}`;

  /** Forget what was fetched for the calendar (data changed): it is fetched again when shown. */
  function calInvalidate() { Object.assign(state.cal, { key: "", detailKey: "", yearKey: "" }); if (window.Citers) window.Citers.invalidate(); }

  function renderCalendarTab() {
    const cal = state.cal;
    $$("#calSource button").forEach((b) => b.classList.toggle("on", b.dataset.source === cal.source));
    const sel = $("#calArticle");
    const arts = state.articles.slice().sort((a, b) => a.title.localeCompare(b.title, loc()));
    sel.innerHTML = `<option value="">${esc(t("cw.all_articles"))}</option>` + arts.map((a) => `<option value="${esc(a.key)}">${esc(shortText(a.title, 70))}</option>`).join("");
    sel.value = cal.article;
    const [y, m] = cal.month.split("-").map(Number);
    $("#mcMonth").innerHTML = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11].map((i) =>
      `<option value="${i + 1}">${esc(new Date(Date.UTC(2023, i, 1)).toLocaleDateString(loc(), { month: "long", timeZone: "UTC" }))}</option>`).join("");
    $("#mcMonth").value = String(m);
    if (document.activeElement !== $("#mcYear")) $("#mcYear").value = String(y);
    renderMonthGrid();
    loadCalMonth();
    renderDetail();
    loadDetail();
    renderUndatedBtn();
    renderYearHeat();
    loadYear();
    renderCalTrend();
  }

  async function loadCalMonth() {
    const cal = state.cal;
    const key = `${cal.month}|${calSig()}`;
    if (cal.key === key) return;
    const [from, to] = monthRange(cal.month);
    const res = await safe(() => invoke("get_calendar_days", { from, to, source: cal.source, article: cal.article }), null);
    if (!res || key !== `${state.cal.month}|${calSig()}`) return;
    cal.key = key;
    cal.days = new Map(res.days.map((d) => [d.date, d]));
    renderMonthGrid();
  }

  /** The month as a grid of day buttons (WinUI CalendarView style): intensity by count, markers per source. */
  function renderMonthGrid(focusAfter) {
    if (state.page !== "analysis" || state.an.tab !== "calendar") return;
    const cal = state.cal;
    const grid = $("#mcGrid");
    const [y, m] = cal.month.split("-").map(Number);
    const weekStart = window.I18N.lang === "tr" ? 1 : 0;
    const lead = (new Date(Date.UTC(y, m - 1, 1)).getUTCDay() - weekStart + 7) % 7;
    const dim = new Date(Date.UTC(y, m, 0)).getUTCDate();
    const today = todayIso();
    const oaLatest = state.analysis?.trend.oa_latest;
    const focus = cal.focus && cal.focus.startsWith(cal.month) ? cal.focus : today.startsWith(cal.month) ? today : `${cal.month}-01`;
    const s = cal.sel;
    let html = `<div class="mc-row" role="row" style="display:contents">`;
    for (let i = 0; i < 7; i++) {
      const dt = new Date(Date.UTC(2023, 0, 1 + ((i + weekStart) % 7)));
      html += `<div class="mc-wd" role="columnheader">${esc(dt.toLocaleDateString(loc(), { weekday: "short", timeZone: "UTC" }))}</div>`;
    }
    html += "</div>";
    const total = Math.ceil((lead + dim) / 7) * 7;
    for (let i = 0; i < total; i++) {
      if (i % 7 === 0) html += `<div class="mc-row" role="row" style="display:contents">`;
      const day = i - lead + 1;
      if (day < 1 || day > dim) {
        html += `<span class="mc-day empty" role="gridcell" aria-hidden="true"></span>`;
      } else {
        const date = `${cal.month}-${pad2(day)}`;
        const info = cal.days.get(date);
        const up = info?.scholar_up || 0, down = info?.scholar_down || 0, oa = info?.openalex || 0;
        const n = up + oa;
        const lvl = n >= 5 ? 4 : n >= 3 ? 3 : n === 2 ? 2 : n === 1 ? 1 : 0;
        const tail = !!oaLatest && date > oaLatest && date <= today && cal.source !== "scholar";
        const isSel = s && (date === s.from || date === s.to);
        const inRange = s && !isSel && date > s.from && date < s.to;
        const parts = [];
        if (up) parts.push(t("cal.aria_scholar_up", { n: up }));
        if (down) parts.push(t("cal.aria_scholar_down", { n: down }));
        if (oa) parts.push(t("cal.aria_oa", { n: oa }));
        if (!parts.length) parts.push(t("cal.aria_none"));
        if (tail) parts.push(t("an.tail_label"));
        html += `<button type="button" role="gridcell" class="mc-day${lvl ? " i" + lvl : ""}${date === today ? " today" : ""}${isSel ? " sel" : ""}${inRange ? " inrange" : ""}${tail ? " tail" : ""}" data-date="${date}" tabindex="${date === focus ? 0 : -1}" aria-selected="${isSel || inRange ? "true" : "false"}" aria-label="${esc(longDay(date) + " — " + parts.join(", "))}">
          <span class="mc-n">${day}</span>
          <span class="mc-dots">${up ? '<i class="dot sc"></i>' : ""}${oa ? '<i class="dot oa"></i>' : ""}${down ? '<i class="dot dn"></i>' : ""}</span>
          ${n ? `<span class="mc-badge">${nf.format(n)}</span>` : ""}</button>`;
      }
      if (i % 7 === 6) html += "</div>";
    }
    grid.innerHTML = html;
    grid.setAttribute("aria-label", new Date(Date.UTC(y, m - 1, 1)).toLocaleDateString(loc(), { month: "long", year: "numeric", timeZone: "UTC" }));
    const lag = $("#mcLag");
    lag.hidden = cal.source === "scholar";
    lag.textContent = lagHint() + (oaLatest ? " " + t("cal.lag_tail") : "");
    if (focusAfter) { const b = grid.querySelector('.mc-day[tabindex="0"]'); if (b) b.focus(); }
  }

  /** Moves the keyboard focus to a day (changing the month when needed). */
  function moveFocus(iso) {
    const cal = state.cal;
    const month = iso.slice(0, 7);
    cal.focus = iso;
    if (month !== cal.month) { cal.month = month; cal.year = null; renderCalendarTab(); }
    renderMonthGrid(true);
  }

  function onGridKey(e) {
    const b = e.target.closest(".mc-day[data-date]");
    if (!b) return;
    const d = b.dataset.date;
    const weekStart = window.I18N.lang === "tr" ? 1 : 0;
    const dow = (new Date(keyMsOf(d)).getUTCDay() - weekStart + 7) % 7;
    let n = null;
    if (e.key === "ArrowLeft") n = addDays(d, -1);
    else if (e.key === "ArrowRight") n = addDays(d, 1);
    else if (e.key === "ArrowUp") n = addDays(d, -7);
    else if (e.key === "ArrowDown") n = addDays(d, 7);
    else if (e.key === "Home") n = addDays(d, -dow);
    else if (e.key === "End") n = addDays(d, 6 - dow);
    else if (e.key === "PageUp" || e.key === "PageDown") {
      const [y, m, dd] = d.split("-").map(Number);
      const t0 = new Date(Date.UTC(y, m - 1 + (e.key === "PageUp" ? -1 : 1), 1));
      const last = new Date(Date.UTC(t0.getUTCFullYear(), t0.getUTCMonth() + 1, 0)).getUTCDate();
      n = `${t0.getUTCFullYear()}-${pad2(t0.getUTCMonth() + 1)}-${pad2(Math.min(dd, last))}`;
    }
    if (n) { e.preventDefault(); moveFocus(n); }
  }

  function selectDay(date, extend) {
    const cal = state.cal;
    const s = cal.sel;
    cal.sel = extend && s && !s.whole ? { from: date < s.from ? date : s.from, to: date > s.to ? date : s.to } : { from: date, to: date };
    cal.focus = date;
    cal.shown = 120;
    renderMonthGrid();
    renderDetail();
    loadDetail();
    renderYearHeat();
  }

  function selectWholeMonth() {
    const [from, to] = monthRange(state.cal.month);
    state.cal.sel = { from, to, whole: true };
    state.cal.shown = 120;
    renderMonthGrid();
    renderDetail();
    loadDetail();
  }

  function changeMonth(delta) {
    const cal = state.cal;
    const [y, m] = cal.month.split("-").map(Number);
    const d = new Date(Date.UTC(y, m - 1 + delta, 1));
    cal.month = `${d.getUTCFullYear()}-${pad2(d.getUTCMonth() + 1)}`;
    cal.focus = "";
    cal.year = null;
    renderCalendarTab();
  }

  /** Opens Analysis > Calendar on one day. */
  function openCalendarDay(day) {
    go(() => {
      const cal = state.cal;
      cal.month = day.slice(0, 7);
      cal.sel = { from: day, to: day };
      cal.focus = day;
      cal.year = null;
      cal.shown = 120;
      state.an.tab = "calendar";
      if (state.page === "analysis") renderAnalysis(); else showPage("analysis");
    });
  }

  // ---- day detail
  async function loadDetail() {
    const cal = state.cal;
    const s = cal.sel;
    if (!s) return;
    const key = `${s.from}|${s.to}|${calSig()}`;
    if (cal.detailKey === key) return;
    const res = await safe(() => invoke("get_day_detail", { from: s.from, to: s.to, source: cal.source, article: cal.article }), null);
    const now = state.cal.sel;
    if (!res || !now || key !== `${now.from}|${now.to}|${calSig()}`) return;
    cal.detail = res;
    cal.detailKey = key;
    renderDetail();
  }

  function scholarItem(e) {
    const down = e.delta < 0;
    const link = e.cites_ids
      ? `<button type="button" class="rc-open" data-act="scholar" data-cites="${esc(e.cites_ids)}" title="${esc(t("scholar.tip"))}" aria-label="${esc(t("scholar.view"))}"><span class="icon">&#xE8A7;</span></button>` : "";
    let nested = "";
    if (!down) {
      const maybe = e.probable.map((w) => `<div class="cd-maybe"><b>${esc(t("cal.maybe"))}</b> <a href="#" class="title-link" data-url="${esc(citingUrl({ doi: w.doi, id: w.citing_id }))}">${esc(w.title || "—")}</a>${w.venue ? " · " + esc(w.venue) : ""}</div>`).join("");
      const rest = e.delta - e.probable.length;
      nested = e.probable.length
        ? `<div class="cd-nested">${maybe}${rest > 0 ? `<div class="cd-none">${esc(t("cal.some_unknown", { n: rest }))}</div>` : ""}</div>`
        : `<div class="cd-none">${esc(t("cal.no_detail"))}</div>`;
    }
    return `<div class="cd-item"><i class="dot ${down ? "dn" : "sc"}"></i><div class="cd-main">
      <div class="cd-line"><a href="#" class="title-link" data-open-article="${esc(e.article_key)}">${esc(e.title)}</a>${chip("", e.delta)}<span class="secondary ${down ? "cd-down" : ""}">${nf.format(e.old)} → ${nf.format(e.new)}</span></div>${nested}</div>${link}</div>`;
  }

  function oaItem(w) {
    const arts = w.articles.map((a) => `<a href="#" class="title-link" data-open-article="${esc(a.key)}" title="${esc(a.title)}">${esc(shortText(a.title, 60))}</a>`).join(", ");
    return `<div class="cd-item"><i class="dot oa"></i><div class="cd-main"><a href="#" class="title-link" data-url="${esc(citingUrl({ doi: w.doi, id: w.citing_id }))}">${esc(w.title || "—")}</a>
      <div class="tl-sub">${w.venue ? esc(w.venue) + " " : ""}<span class="tl-arrow">→</span> ${arts}</div></div></div>`;
  }

  function renderDetail() {
    const cal = state.cal;
    const s = cal.sel;
    const body = $("#cdBody");
    $("#cdCiting").hidden = !s || cal.source === "scholar";
    $("#cdMoreRow").hidden = true;
    if (!s) {
      $("#cdTitle").textContent = t("cal.pick_title");
      body.innerHTML = `<div class="cd-empty">${esc(t("cal.pick_hint"))}</div>`;
      return;
    }
    const single = s.from === s.to;
    $("#cdTitle").textContent = single ? longDay(s.from)
      : s.whole ? monthLabel(s.from.slice(0, 7), true) : `${shortDay(s.from)} – ${shortDay(s.to)}`;
    if (cal.detailKey !== `${s.from}|${s.to}|${calSig()}`) { body.innerHTML = ""; return; }
    const days = cal.detail.slice().sort((a, b) => (a.date < b.date ? 1 : -1));
    const up = days.reduce((a, d) => a + d.scholar.filter((e) => e.delta > 0).reduce((x, e) => x + e.delta, 0), 0);
    const down = days.reduce((a, d) => a + d.scholar.filter((e) => e.delta < 0).reduce((x, e) => x - e.delta, 0), 0);
    const works = days.reduce((a, d) => a + d.openalex.length, 0);
    const oaLatest = state.analysis?.trend.oa_latest;
    let html = "";
    if (days.length) {
      const bits = [];
      if (cal.source !== "openalex") bits.push(t("cal.sum_scholar", { up: "+" + nf.format(up), down: down ? " / −" + nf.format(down) : "" }));
      if (cal.source !== "scholar") bits.push(t("cal.sum_oa", { n: nf.format(works) }));
      html += `<div class="cd-summary">${esc(bits.join(" · "))}</div>`;
    }
    let shown = 0, cut = false;
    for (const d of days) {
      if (shown >= cal.shown) { cut = true; break; }
      html += `<div class="cd-day">${single ? "" : `<div class="rc-group-head">${esc(longDay(d.date))}</div>`}`;
      if (d.scholar.length) html += `<div class="cd-sub">${esc(t("cal.sh_scholar"))}</div>` + d.scholar.map(scholarItem).join("");
      if (d.openalex.length) html += `<div class="cd-sub">${esc(t("cal.sh_oa"))}</div>` + d.openalex.map(oaItem).join("");
      html += "</div>";
      shown += d.scholar.length + d.openalex.length;
    }
    if (!days.length) {
      const lagging = oaLatest && s.to > oaLatest && cal.source !== "scholar";
      html = `<div class="cd-empty">${esc(t("cal.none_day"))}${lagging ? `<div class="cd-none">${esc(t("cal.none_lag", { date: fmtCitingDate(oaLatest, "day") }))}</div>` : ""}</div>`;
    }
    body.innerHTML = html;
    $("#cdMoreRow").hidden = !cut;
  }

  function openCitingFromDetail() {
    const s = state.cal.sel;
    if (!s) return;
    go(() => {
      Object.assign(state.cw, { article: state.cal.article, country: "", venue: "", query: "", shown: 200, date: "", period: null, type: "", oa: "", field: "", noSelf: false, sort: "date" });
      if (s.from === s.to) state.cw.date = s.from;
      else state.cw.period = { from: keyMsOf(s.from), to: keyMsOf(s.to) + DAY_MS - 1, label: s.whole ? monthLabel(s.from.slice(0, 7), true) : `${shortDay(s.from)} – ${shortDay(s.to)}` };
      $("#citingSearch").value = "";
      showPage("citing");
    });
  }

  // ---- works that only have a month or a year: never on a day
  const undatedGroups = () => (state.citingGroups || []).filter((g) => g.precision !== "day" && (!state.cal.article || g.articles.some((a) => a.key === state.cal.article)));

  function renderUndatedBtn() {
    const n = state.cal.source === "scholar" ? 0 : undatedGroups().length;
    $("#calUndatedBtn").hidden = n === 0;
    $("#calUndatedText").textContent = t("cal.undated", { n: nf.format(n) });
    if (n === 0) state.cal.undatedOpen = false;
    renderUndatedList();
  }

  function renderUndatedList() {
    const card = $("#calUndatedCard");
    card.hidden = !state.cal.undatedOpen;
    if (!state.cal.undatedOpen) return;
    const list = undatedGroups().sort((a, b) => (a.date < b.date ? 1 : a.date > b.date ? -1 : 0));
    const mo = list.filter((g) => g.precision === "month").length;
    $("#cuTitle").textContent = t("cal.undated", { n: nf.format(list.length) });
    $("#cuNote").textContent = t("an.undated_note", { m: nf.format(mo), y: nf.format(list.length - mo) });
    $("#cuList").innerHTML = list.slice(0, 300).map((g) => `<div class="tl-day"><div class="tl-daylabel">${esc(fmtCitingDate(g.date, g.precision))}</div><div class="tl-items">${tlItem(g)}</div></div>`).join("");
  }

  function tlItem(g) {
    const arts = g.articles.map((x) => `<a href="#" class="title-link" data-open-article="${esc(x.key)}" title="${esc(x.title)}">${esc(shortText(x.title, 64))}</a>`).join(", ");
    return `<div class="tl-item"><a href="#" class="title-link" data-url="${esc(citingUrl(g))}">${esc(g.title || "—")}</a>
      <div class="tl-sub">${g.venue ? esc(g.venue) + " " : ""}<span class="tl-arrow">→</span> ${arts}</div></div>`;
  }

  // ---- year heat map: every year with data at once (one row per year), cells shrink to fit
  const mondayIdx = (ms) => (new Date(ms).getUTCDay() + 6) % 7;
  const heatMonthYear = (ms) => new Date(ms).toLocaleDateString(loc(), { month: "short", year: "numeric", timeZone: "UTC" });

  /** What the period selector asks the year heat map to show: the rolling 53 weeks, all years, or the years the range covers. */
  function heatPlan() {
    const today = todayIso();
    const curY = Number(today.slice(0, 4));
    const r = state.an.range;
    if (r === "12m") {
      const end = keyMsOf(today);
      const start = end - mondayIdx(end) * DAY_MS - 52 * 7 * DAY_MS;
      return { mode: "roll", start, end, y0: new Date(start).getUTCFullYear(), y1: curY, clip: null };
    }
    if (r === "all") {
      const snapY = state.series[0]?.t ? new Date(state.series[0].t).getFullYear() : curY;
      return { mode: "years", y0: Math.min(state.analysis?.calendar.years[0] ?? curY, snapY, curY), y1: curY, clip: null, trim: true };
    }
    const a = anRange();
    const from = a.from == null ? Date.now() - 365 * DAY_MS : a.from;
    return { mode: "years", y0: new Date(from).getFullYear(), y1: new Date(a.to ?? Date.now()).getFullYear(), clip: { from: isoDay(new Date(from)), to: a.to == null ? today : isoDay(new Date(a.to)) }, trim: false };
  }
  const heatKey = (p) => `${p.mode}|${p.y0}|${p.y1}|${calSig()}`;

  async function loadYear() {
    const cal = state.cal;
    if (!state.analysis) return;
    const plan = heatPlan();
    const key = heatKey(plan);
    if (cal.yearKey === key) return;
    const res = await safe(() => invoke("get_calendar_days", { from: `${plan.y0}-01-01`, to: `${plan.y1}-12-31`, source: cal.source, article: cal.article }), null);
    if (!res || key !== heatKey(heatPlan())) return;
    cal.yearKey = key;
    cal.yearDays = new Map(res.days.map((d) => [d.date, d.scholar_up + d.openalex]).filter((d) => d[1] > 0));
    renderYearHeat();
  }

  function renderYearHeat() {
    if (state.page !== "analysis" || state.an.tab !== "calendar" || !state.analysis) return;
    const cal = state.cal;
    const plan = heatPlan();
    const el = $("#yearHeat");
    if (cal.yearKey !== heatKey(plan)) { return; }
    let days = cal.yearDays;
    if (plan.clip) days = new Map(Array.from(days).filter(([d]) => d >= plan.clip.from && d <= plan.clip.to));
    const sumIn = (a, b) => { let n = 0; for (const [d, c] of days) if (d >= a && d <= b) n += c; return n; };
    let rows = [];
    if (plan.mode === "roll") {
      const a = isoDay0(plan.start), b = isoDay0(plan.end);
      rows = [{ label: t("cal.heat_roll"), sub: nf.format(sumIn(a, b)), caption: `${heatMonthYear(plan.start)} – ${heatMonthYear(plan.end)}`, start: plan.start, end: plan.end }];
    } else {
      let y0 = plan.y0, y1 = plan.y1;
      if (plan.trim) {
        const ys = Array.from(days.keys()).map((d) => Number(d.slice(0, 4)));
        if (ys.length) { y0 = Math.min(...ys); y1 = Math.max(...ys); } else { y0 = 1; y1 = 0; }
      }
      for (let y = y1; y >= y0; y--) rows.push({ label: String(y), sub: nf.format(sumIn(`${y}-01-01`, `${y}-12-31`)), start: Date.UTC(y, 0, 1), end: Date.UTC(y, 11, 31) });
    }
    if (!days.size) rows = [];
    const pal = window.CitersLib.palette(Charts.ctx());
    const oaLatest = state.analysis.trend.oa_latest;
    const tailFrom = cal.source !== "scholar" && oaLatest && oaLatest.length >= 10 ? addDays(oaLatest.slice(0, 10), 1) : "";
    window.YearHeat.mount(el, {
      rows, days, selected: cal.sel && cal.sel.from === cal.sel.to ? cal.sel.from : "", today: todayIso(), tailFrom, t, loc: loc(),
      palette: { seq: pal.seq, cell: pal.cell }, emptyText: t("cal.heat_empty"),
      onPick: (d) => { selectDay(d); cal.month = d.slice(0, 7); renderCalendarTab(); const m = $("#calDetail"); if (m) m.scrollIntoView({ block: "nearest", behavior: matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth" }); },
    });
    let text = t("cal.cap_year");
    if (tailFrom && rows.length && tailFrom <= todayIso()) text += " " + t("an.lag_tail_note", { date: fmtDayMonth(oaLatest) });
    $("#calCaption").textContent = text;
  }
  const isoDay0 = (ms) => Charts.util.ymd(ms);

  function renderCalTrend() {
    const a = state.analysis;
    const c = Charts.ctx({ range: anRange() });
    Charts.mount($("#chCalTrend"), Charts.build.trend(a, c), { emptyText: t("chart.no_data_range") });
    $("#calTrendCaption").textContent = t(a.trend.hist.length ? "trend.cap_hist" : "trend.cap_scholar");
  }

  /** Scholar count against the citing works identified so far, per article. */
  function gapParts(a) {
    const unidentified = Math.max(0, a.citations - a.identified);
    return { unidentified, pct: a.citations > 0 ? Math.min(100, Math.round((a.identified / a.citations) * 100)) : 0 };
  }
  function renderGapList() {
    const el = $("#gapList");
    if (!el) return;
    const rows = state.articles.filter((a) => a.citations > 0).sort((a, b) => gapParts(b).unidentified - gapParts(a).unidentified || b.citations - a.citations).slice(0, 14);
    el.innerHTML = rows.length ? rows.map((a) => {
      const g = gapParts(a);
      return `<div class="gap-row"><div class="gap-title" title="${esc(a.title)}"><a href="#" class="title-link" data-open-article="${esc(a.key)}">${esc(a.title)}</a></div>
        <span class="idbar" aria-hidden="true"><i style="width:${g.pct}%"></i></span>
        <div class="gap-nums">${esc(t("gap.line", { s: nf.format(a.citations), i: nf.format(a.identified), u: nf.format(g.unidentified) }))}</div></div>`;
    }).join("") : `<div class="an-note">${esc(t("chart.no_data"))}</div>`;
  }

  /** "Bu dönemi raporla": the Report page, filled with the period of this page. */
  function reportThisPeriod() {
    const r = anRange();
    const first = state.series[0]?.t ? new Date(state.series[0].t).getTime() : Date.now() - 365 * DAY_MS;
    const hist = state.analysis?.trend.hist[0]?.[0];
    $("#repFrom").value = isoDay(new Date(r.from ?? (hist ?? first)));
    $("#repTo").value = isoDay(new Date(r.to ?? Date.now()));
    $("#repLang").value = window.I18N.lang;
    go(() => showPage("report"));
  }

  // ------------------------------------------------------------ citing works page
  async function loadCiting() {
    state.citing = await safe(() => invoke("get_citing_works"), state.citing);
    state.citingLoaded = true;
    state.citingGroups = groupCiting(state.citing);
    renderCiting();
    if (state.page === "analysis") renderAnalysis();
    renderReportPreview();
  }
  async function loadDist() { state.dist = await safe(() => invoke("get_distribution"), state.dist); renderDist(); }

  const citingUrl = (c) => (c.doi ? "https://doi.org/" + encodeURI(c.doi) : "https://openalex.org/" + c.id);

  /** One entry per citing work, with the list of my articles it cites. */
  function groupCiting(rows) {
    const map = new Map();
    for (const r of rows || []) {
      let g = map.get(r.citing_id);
      if (!g) {
        g = {
          id: r.citing_id, title: r.title, date: r.date, precision: r.precision, venue: r.venue, doi: r.doi, countries: r.countries, authors: r.authors, articles: [],
          type: r.work_type || "", language: r.language || "", oa: r.oa_status || "", field: r.topic_field || "", domain: r.topic_domain || "", cited: r.cited_by ?? null,
          self: !!r.self_cite, co: !!r.coauthor_cite, url: r.url || "",
          ojp: r.ojp ?? null, ojp_state: r.ojp_state || "no_journal", ojp_field: r.ojp_field || "", ojp_n: r.ojp_n || 0,
        };
        map.set(r.citing_id, g);
      }
      if (!g.articles.some((a) => a.key === r.article_key)) g.articles.push({ key: r.article_key, title: r.article_title });
    }
    return Array.from(map.values());
  }

  function fmtCitingDate(date, precision) {
    if (precision === "year") return date;
    const [y, m, d] = date.split("-").map(Number);
    const dt = new Date(Date.UTC(y, (m || 1) - 1, d || 1));
    return dt.toLocaleDateString(loc(), precision === "month"
      ? { month: "short", year: "numeric", timeZone: "UTC" }
      : { day: "numeric", month: "short", year: "numeric", timeZone: "UTC" });
  }

  const L = window.CitersLib;
  const sameText = (a, b) => String(a || "").trim().toLocaleLowerCase(loc()) === String(b || "").trim().toLocaleLowerCase(loc());

  function filteredCiting() {
    const cw = state.cw;
    const q = cw.query.trim().toLocaleLowerCase(loc());
    let rows = (state.citingGroups || []).filter((g) =>
      (!cw.article || g.articles.some((a) => a.key === cw.article)) &&
      (!cw.country || (g.countries || "").split(",").map((c) => c.trim()).includes(cw.country)) &&
      (!cw.venue || sameText(g.venue, cw.venue)) &&
      (!cw.date || (g.precision === "day" && g.date === cw.date)) &&
      (!cw.period || spanTouches(g, cw.period.from, cw.period.to)) &&
      (!cw.type || (L.names.typeKey(g.type) || g.type) === cw.type) &&
      (!cw.oa || L.names.oaKey(g.oa) === cw.oa) &&
      (!cw.field || g.field === cw.field) &&
      (!cw.noSelf || !g.self) &&
      (!q || `${g.title} ${g.venue} ${g.authors} ${g.articles.map((a) => a.title).join(" ")}`.toLocaleLowerCase(loc()).includes(q)));
    const dir = cw.dir;
    if (cw.sort === "ojp") {
      // rows without a score are always last, in both directions; ties by date, newest first
      const sc = (g) => (g.ojp_state === "ranked" && g.ojp != null ? g.ojp : null);
      rows = rows.slice().sort((a, b) => {
        const x = sc(a), y = sc(b);
        if (x == null && y == null) return a.date < b.date ? 1 : a.date > b.date ? -1 : 0;
        if (x == null) return 1;
        if (y == null) return -1;
        return (x - y) * dir || (a.date < b.date ? 1 : a.date > b.date ? -1 : 0);
      });
    } else if (cw.sort === "cited") rows = rows.slice().sort((a, b) => (b.cited ?? -1) - (a.cited ?? -1) || (a.date < b.date ? 1 : -1));
    else rows = rows.slice().sort((a, b) => (a.date < b.date ? -1 : a.date > b.date ? 1 : 0) * dir || a.title.localeCompare(b.title, loc()));
    return rows;
  }

  function fillArticleFilter() {
    const sel = $("#citingArticle");
    const seen = new Map();
    for (const r of state.citing || []) if (!seen.has(r.article_key)) seen.set(r.article_key, r.article_title);
    const items = Array.from(seen.entries()).sort((a, b) => a[1].localeCompare(b[1], loc()));
    sel.innerHTML = `<option value="">${esc(t("cw.all_articles"))}</option>` + items.map(([k, title]) => `<option value="${esc(k)}">${esc(shortText(title, 70))}</option>`).join("");
    if (state.cw.article && !seen.has(state.cw.article)) state.cw.article = "";
    sel.value = state.cw.article;
  }

  /** Removable chips of the active filters (country / venue from the charts, date / period from the calendar). */
  /** Options of the new list filters (type, open access, field) from what the stored citing works carry. */
  function fillCitingFilters() {
    const groups = state.citingGroups || [];
    const types = new Map(), fields = new Set(), oas = new Set();
    for (const g of groups) {
      const k = L.names.typeKey(g.type) || g.type;
      if (k) types.set(k, L.names.typeName(g.type));
      if (g.field) fields.add(g.field);
      if (g.oa) oas.add(L.names.oaKey(g.oa));
    }
    const fill = (sel, allKey, entries, cur) => {
      const el = $(sel);
      el.innerHTML = `<option value="">${esc(t(allKey))}</option>` + entries.map(([v, label]) => `<option value="${esc(v)}">${esc(label)}</option>`).join("");
      el.value = entries.some(([v]) => v === cur) ? cur : "";
    };
    fill("#citingType", "cw.f_all_types", Array.from(types).sort((a, b) => a[1].localeCompare(b[1], loc())), state.cw.type);
    const order = ["gold", "green", "hybrid", "bronze", "closed"];
    fill("#citingOa", "cw.f_all_oa", order.filter((k) => oas.has(k)).map((k) => [k, t("ci.oa_" + k)]), state.cw.oa);
    fill("#citingField", "cw.f_all_fields", Array.from(fields).map((f) => [f, L.names.fieldName(f)]).sort((a, b) => a[1].localeCompare(b[1], loc())), state.cw.field);
    $("#citingNoSelf").checked = state.cw.noSelf;
    $("#citingFilters").hidden = !(types.size || fields.size || oas.size);
  }
  function syncCitingToolbar() { if (state.page === "citing") fillCitingFilters(); }

  function renderCitingChips() {
    const cw = state.cw;
    const lang = window.I18N.lang;
    const chips = [];
    if (cw.country) chips.push(["country", t("cw.chip_country", { name: window.COUNTRIES.name(cw.country, lang) })]);
    if (cw.venue) chips.push(["venue", t("cw.chip_venue", { name: shortText(cw.venue, 60) })]);
    if (cw.date) chips.push(["date", t("cw.chip_date", { date: fmtCitingDate(cw.date, "day") })]);
    if (cw.period) chips.push(["period", t("an.chip_period", { label: cw.period.label })]);
    if (cw.type) chips.push(["type", t("cw.chip_type", { name: L.names.typeName(cw.type) })]);
    if (cw.oa) chips.push(["oa", t("cw.chip_oa", { name: t("ci.oa_" + cw.oa) })]);
    if (cw.field) chips.push(["field", t("cw.chip_field", { name: L.names.fieldName(cw.field) })]);
    if (cw.noSelf) chips.push(["noSelf", t("cw.chip_self")]);
    if (cw.sort === "cited") chips.push(["sort", t("cw.chip_sort")]);
    if (cw.sort === "ojp") chips.push(["sort", t("cw.chip_sort_ojp")]);
    const box = $("#citingChips");
    box.hidden = !chips.length;
    box.innerHTML = chips.map(([k, label]) => `<span class="fchip"><span>${esc(label)}</span><button type="button" data-clear="${k}" aria-label="${esc(t("cw.chip_remove"))}" title="${esc(t("cw.chip_remove"))}"><span class="icon">&#xE711;</span></button></span>`).join("")
      + (chips.length > 1 ? `<button type="button" class="btn btn-link" data-clear="all"><span>${esc(t("cw.chip_clear"))}</span></button>` : "");
  }

  function renderCiting() {
    if (state.page !== "citing") return;
    const has = (state.citing || []).length > 0;
    $("#citingEmpty").hidden = has;
    $("#citingData").hidden = !has;
    renderDist();
    if (!has) return;
    fillArticleFilter();
    fillCitingFilters();
    renderCitingChips();
    $$("#citingTable th[data-csort]").forEach((th) => {
      const old = $(".sort", th); if (old) old.remove();
      const active = (state.cw.sort === "ojp" || state.cw.sort === "date") && state.cw.sort === th.dataset.csort;
      th.setAttribute("aria-sort", active ? (state.cw.dir > 0 ? "ascending" : "descending") : "none");
      if (active) th.insertAdjacentHTML("beforeend", `<span class="sort">${state.cw.dir > 0 ? "&#xE70E;" : "&#xE70D;"}</span>`);
    });
    renderOjpIntro();
    const rows = filteredCiting();
    const total = (state.citingGroups || []).length;
    $("#citingCount").textContent = t("cw.count", { n: nf.format(rows.length), total: nf.format(total) });
    const shown = rows.slice(0, state.cw.shown);
    const lang = window.I18N.lang;
    $("#citingBody").innerHTML = shown.length ? shown.map((g) => {
      const arts = g.articles.slice(0, 2).map((a) => `<div class="cw-art" title="${esc(a.title)}">${esc(shortText(a.title, 60))}</div>`).join("")
        + (g.articles.length > 2 ? `<div class="caption secondary">${esc(t("cw.more_articles", { n: g.articles.length - 2 }))}</div>` : "");
      const countries = g.countries ? g.countries.split(",").map((c) => `<span class="chip" title="${esc(window.COUNTRIES.name(c, lang))}">${esc(c)}</span>`).join("") : "";
      return `<tr>
        <td class="num cw-date" style="text-align:left">${esc(fmtCitingDate(g.date, g.precision))}</td>
        <td><a href="#" class="title-link" data-url="${esc(citingUrl(g))}">${esc(g.title || "—")}</a>
          <div class="title-meta">${esc(shortText(g.authors, 110))}${g.cited != null ? ` · ${esc((g.cited === 1 ? t("cw.cited_meta.one", { n: nf.format(g.cited) }) : t("cw.cited_meta.many", { n: nf.format(g.cited) })))}` : ""}</div>${countries ? `<div class="chips cw-countries">${countries}</div>` : ""}</td>
        <td>${esc(g.venue || "—")}</td>
        <td class="col-ojp">${L.ojp.badge(g)}</td>
        <td class="cw-arts">${arts}</td></tr>`;
    }).join("") : `<tr><td colspan="5" class="empty-row">${esc(t("cw.none_match"))}</td></tr>`;
    $("#citingMoreRow").hidden = rows.length <= shown.length;
  }

  /** Label of the period selector of the Analysis page (for the chip on the list page). */
  function anPeriodLabel() {
    const r = state.an.range;
    if (r === "custom") return `${state.an.from || "…"} – ${state.an.to || "…"}`;
    return t("an.r_" + r);
  }
  /** UTC midnight (ms) of the local calendar day of a timestamp: the Citing works list compares UTC day spans, like the backend. */
  const utcDayMs = (ms) => keyMsOf(isoDay(new Date(ms)));
  /** "view in list": the Citing works list with the period, article and self-citation state of the Analysis tab. */
  function openCitersList(o) {
    const r = anRange();
    go(() => {
      Object.assign(state.cw, {
        article: o.article || "", query: "", shown: 200, country: "", venue: "", date: "", type: "", oa: "", field: "",
        noSelf: !!o.excludeSelf, sort: o.sort === "cited" ? "cited" : o.sort === "ojp" ? "ojp" : "date", dir: -1,
        period: r.from == null && r.to == null ? null : { from: r.from == null ? null : utcDayMs(r.from), to: r.to == null ? null : utcDayMs(r.to), label: anPeriodLabel() },
      });
      $("#citingSearch").value = "";
      showPage("citing");
    });
  }
  /** Range of the Analysis period selector as local days for the get_citers_* commands (empty = open). */
  function anRangeDays() {
    const r = anRange();
    return { from: r.from == null ? "" : isoDay(new Date(r.from)), to: r.to == null ? "" : isoDay(new Date(r.to)) };
  }

  function setCitingFilter(key, value) {
    state.cw[key] = state.cw[key] === value ? "" : value;
    state.cw.shown = 200;
    renderCiting();
  }

  function renderDist() {
    if (state.page !== "citing" || $("#citingData").hidden) return;
    const d = state.dist;
    if (!d) return;
    const lang = window.I18N.lang;
    const draw = (sel, items, c) => {
      const el = $(sel);
      el.style.height = Math.max(120, items.length * 28 + 16) + "px";
      Charts.mount(el, Charts.build.bars(items, c), { emptyText: t("chart.no_data") });
    };
    draw("#distCountry", d.countries, Charts.ctx({
      labelOf: (k) => `${window.COUNTRIES.name(k, lang)} (${k})`, total: d.with_country, active: state.cw.country,
      onPick: (k) => setCitingFilter("country", k), labelWidth: 150,
    }));
    draw("#distVenue", d.venues, Charts.ctx({
      labelOf: (k) => k, total: d.total - d.without_venue, active: state.cw.venue,
      onPick: (k) => setCitingFilter("venue", k), labelWidth: 170,
    }));
    const notes = [];
    if (d.with_country === 0) notes.push(t("dist.country_none"));
    else if (d.without_country > 0) notes.push(t("dist.country_missing", { n: nf.format(d.without_country), total: nf.format(d.total) }));
    if (d.without_venue > 0) notes.push(t("dist.venue_missing", { n: nf.format(d.without_venue) }));
    notes.push(t("dist.distinct"));
    notes.push(t("dist.click_hint"));
    $("#distCaption").textContent = notes.join(" ");
  }

  // ------------------------------------------------------------ report page
  const isoDay = (d) => `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;

  function setReportRange(preset) {
    const now = new Date();
    const to = isoDay(now);
    let from;
    if (preset === "year") from = `${now.getFullYear()}-01-01`;
    else if (preset === "all") from = state.firstDay || `${now.getFullYear() - 1}-01-01`;
    else { const d = new Date(now); d.setFullYear(d.getFullYear() - 1); d.setDate(d.getDate() + 1); from = isoDay(d); }
    $("#repFrom").value = from;
    $("#repTo").value = to;
    renderReportPreview();
  }

  function renderReportPage() {
    const first = state.series[0]?.t;
    state.firstDay = first ? isoDay(new Date(first)) : null;
    if (!$("#repFrom").value) {
      setReportRange("12m");
      $("#repLang").value = window.I18N.lang;
    }
    $("#repCreate").disabled = noProfile() || !state.overview?.snapshot;
  }

  function reportInfo(text, err) {
    const bar = $("#reportInfo");
    bar.textContent = text;
    bar.className = "infobar" + (err ? " err" : "");
    bar.hidden = !text;
  }

  const reportCharts = () => $$("#repCharts input[data-chart], #repCiters input[data-chart]").filter((i) => i.checked).map((i) => i.dataset.chart);

  async function createReport() {
    const from = $("#repFrom").value, to = $("#repTo").value;
    if (!from || !to) { reportInfo(t("rep.err_dates"), true); return; }
    if (from > to) { reportInfo(t("rep.err_order"), true); return; }
    const btn = $("#repCreate");
    btn.disabled = true;
    $("#repCreateText").textContent = t("rep.creating");
    reportInfo("", false);
    try {
      const path = await invoke("create_report_pdf", {
        options: { from, to, lang: $("#repLang").value, include_citing: $("#repCiting").checked, charts: reportCharts() },
      });
      if (path) reportInfo(t("rep.done", { path }), false);
    } catch (e) {
      reportInfo(String(e), true);
    } finally {
      btn.disabled = false;
      $("#repCreateText").textContent = t("rep.create");
    }
  }

  /** The range of the report inputs as {from, to} ms (a range that ends today stays open). */
  function reportRangeMs() {
    const f = $("#repFrom").value, e = $("#repTo").value;
    const from = f ? new Date(f + "T00:00:00").getTime() : null;
    let to = e ? new Date(e + "T23:59:59").getTime() : null;
    if (to !== null && to >= Date.now() - DAY_MS) to = null;
    return { from, to };
  }

  /** Country / venue counts of the citing works that touch the range (each work once), like the backend does for the PDF. */
  function distOf(groups) {
    const countries = new Map(), venues = new Map();
    let withCountry = 0;
    for (const g of groups) {
      const codes = new Set((g.countries || "").split(",").map((c) => c.trim()).filter((c) => c.length === 2));
      if (codes.size) withCountry++;
      for (const c of codes) countries.set(c, (countries.get(c) || 0) + 1);
      const v = (g.venue || "").trim();
      if (v) { const k = v.toLowerCase(); const e = venues.get(k) || venues.set(k, [v, 0]).get(k); e[1]++; }
    }
    const top = (arr) => arr.sort((a, b) => b[1] - a[1] || String(a[0]).localeCompare(String(b[0]))).slice(0, 10);
    return { countries: top(Array.from(countries.entries())), venues: top(Array.from(venues.values())), withCountry, total: groups.length };
  }

  /** Small live preview of the PDF: the same chart builders as the report, drawn as thumbnails. */
  let previewTimer = 0;
  function scheduleReportPreview() { clearTimeout(previewTimer); previewTimer = setTimeout(renderReportPreview, 150); }
  async function renderReportPreview() {
    if (state.page !== "report") return;
    const grid = $("#repPreviewGrid");
    if (!state.analysis) { await loadAnalysis(); if (!state.analysis) return; }
    if (!state.citingLoaded) await loadCiting();
    const a = state.analysis;
    const range = reportRangeMs();
    const charts = reportCharts();
    Charts.disposeIn(grid);
    const groups = (state.citingGroups || []).filter((g) => spanTouches(g, range.from, range.to));
    $("#repPreviewSummary").textContent = t("rep.preview_sum", { charts: charts.length, n: nf.format(groups.length) });
    grid.innerHTML = "";
    const gran = Charts.util.autoGran(range.from ?? state.analysis.trend.snaps[0][0], range.to ?? Date.now());
    const endYear = new Date(range.to ?? Date.now()).getFullYear();
    const base = { range };
    const tiles = {
      trend: () => Charts.build.trend(a, Charts.ctx({ ...base, mini: true })),
      period: () => Charts.build.period(a, Charts.ctx({ ...base, gran, oaLatest: a.trend.oa_latest })),
      article: () => Charts.build.stacked(a, Charts.ctx(base)),
      index: () => Charts.build.indices(a, Charts.ctx(base)),
      calendar: () => Charts.build.calendar(a, Charts.ctx({ ...base, year: Math.min(Math.max(endYear, a.calendar.years[0] ?? endYear), a.calendar.years[a.calendar.years.length - 1] ?? endYear) })),
      dist: () => {
        const d = distOf(groups);
        const lang = window.I18N.lang;
        return Charts.build.bars(d.countries, Charts.ctx({ labelOf: (k) => window.COUNTRIES.name(k, lang), total: d.withCountry, print: true }));
      },
    };
    for (const id of charts) {
      if (id.startsWith("cit")) {
        const tile = document.createElement("div");
        tile.className = "prev-tile";
        tile.innerHTML = `<div class="prev-t">${esc(t("rep.c_" + id))}</div><div class="prev-chart"><div class="prev-empty">${esc(t("rep.preview_cit"))}</div></div>`;
        grid.appendChild(tile);
        continue;
      }
      const tile = document.createElement("div");
      tile.className = "prev-tile";
      const box = document.createElement("div");
      box.className = "prev-chart" + (id === "calendar" ? " prev-cal" : "");
      tile.innerHTML = `<div class="prev-t">${esc(t("rep.c_" + id))}</div>`;
      tile.appendChild(box);
      grid.appendChild(tile);
      const built = Charts.thumb(tiles[id]());
      if (built) Charts.mount(box, built);
      else box.innerHTML = `<div class="prev-empty">${esc(t("rep.preview_none"))}</div>`;
    }
    if (!charts.length) grid.innerHTML = `<div class="prev-empty">${esc(t("rep.preview_empty"))}</div>`;
  }

  // ------------------------------------------------------------ API keys
  const KEYS = {
    openalex: { input: "#keyOa", status: "#keyOaStatus", name: "OpenAlex", flag: "openalex" },
  };
  async function loadKeyStatus() { state.keys = await safe(() => invoke("api_key_status"), state.keys); renderKeys(); }

  function setKeyMsg(which, text, cls) { state.keyMsg[which] = text ? { text, cls: cls || "" } : null; renderKeys(); }

  function renderKeys() {
    for (const [which, k] of Object.entries(KEYS)) {
      const stored = !!state.keys?.[k.flag];
      const el = $(k.status);
      const msg = state.keyMsg[which];
      el.textContent = msg ? msg.text : t(stored ? "keys.stored" : "keys.none");
      el.className = "caption key-status" + (msg?.cls ? " " + msg.cls : stored ? " ok" : " secondary");
      $(k.input).placeholder = t(stored ? "keys.placeholder_stored" : "keys.placeholder");
      $("#keyOaDel").disabled = !stored;
    }
  }

  async function saveKey(which) {
    const k = KEYS[which];
    const key = $(k.input).value.trim();
    if (!key) { setKeyMsg(which, t("keys.err_empty"), "bad"); return; }
    try {
      state.keys = await invoke("save_api_key", { which, key });
      loadBudget();
      $(k.input).value = "";
      setKeyMsg(which, t("keys.saved"), "ok");
    } catch (e) { setKeyMsg(which, String(e), "bad"); }
  }

  async function testKey(which) {
    const k = KEYS[which];
    const typed = $(k.input).value.trim();
    setKeyMsg(which, t("keys.testing"), "secondary");
    try {
      const r = await invoke("test_api_key", { which, key: typed || null });
      if (r.status === "ok") setKeyMsg(which, t("keys.test_ok"), "ok");
      else if (r.status === "invalid") setKeyMsg(which, t("keys.test_invalid"), "bad");
      else setKeyMsg(which, t("keys.test_unreachable", { detail: r.detail || "?" }), "warn");
    } catch (e) { setKeyMsg(which, String(e), "bad"); }
  }

  async function removeKey(which) {
    try {
      state.keys = await invoke("delete_api_key", { which });
      loadBudget();
      $(KEYS[which].input).value = "";
      setKeyMsg(which, t("keys.removed"), "ok");
    } catch (e) { setKeyMsg(which, String(e), "bad"); }
  }

  async function requestOaKey() {
    const email = $("#keyOaEmail").value.trim();
    if (!email) { setKeyMsg("openalex", t("keys.err_email"), "bad"); return; }
    const btn = $("#keyOaRequest");
    btn.disabled = true;
    try {
      await invoke("request_openalex_key", { email });
      loadBudget();
      $("#keyOaEmail").value = "";
      await loadKeyStatus();
      setKeyMsg("openalex", t("keys.requested"), "ok");
    } catch (e) { setKeyMsg("openalex", String(e), "bad"); }
    finally { btn.disabled = false; }
  }

  async function exportXlsx() {
    try {
      const path = await invoke("export_xlsx");
      if (path) showInfo(t("settings.exported", { path }), false);
    } catch (e) { showInfo(String(e), true); }
  }

  // ------------------------------------------------------------ navigation / wiring
  function showPage(name) {
    const prev = state.page;
    // only a cross-page link (go) keeps the way back; the sidebar and everything else starts afresh
    if (!state.nav.keep) state.nav.stack.length = 0;
    state.page = name;
    // charts are created per page and disposed when it is left
    if (prev !== name) Charts.disposeIn($("#page-" + prev));
    if (name === "settings") loadBfInfo();
    if (name === "credits" && prev !== "credits") state.credFrom = prev === "help" ? "help" : "settings";
    $$(".nav-item").forEach((b) => b.classList.toggle("selected", b.dataset.page === name || (name === "credits" && b.dataset.page === "settings")));
    $$(".page").forEach((p) => { p.hidden = p.id !== "page-" + name; });
    hideTip();
    renderVisit();
    renderBudget();
    if (name === "panel") { renderMini(); renderRecent(); }
    if (name === "articles") loadArticles();
    if (name !== "articles") $("#doiBar").hidden = true;
    if (name === "log") loadLog();
    if (name === "citing") { loadCiting(); loadDist(); }
    if (name === "analysis") { renderAnalysis(); loadAnalysis(); loadRisers(); }
    if (name === "report") { renderReportPage(); renderReportPreview(); }
    if (name === "settings") { $("#infoBar").hidden = true; fillSettings(); loadKeyStatus(); }
    if (name === "help") renderHelp();
    if (name === "credits") loadCredits();
    renderBack();
  }

  // ------------------------------------------------------------ back navigation
  const NAV_MAX = 20;
  const NAV_KEYS = { panel: "nav.panel", articles: "nav.articles", citing: "nav.citing", log: "nav.log", report: "nav.report", settings: "nav.settings", help: "nav.help", analysis: "nav.analysis" };
  const navSig = () => state.page + (state.page === "analysis" ? ":" + state.an.tab : "");
  function navName(e) {
    const base = NAV_KEYS[e.page] ? t(NAV_KEYS[e.page]) : e.page;
    return e.page === "analysis" ? base + " › " + t("an.tab_" + e.an.tab) : base;
  }
  /** What "back" has to restore: the page, the Analysis tab / period, the filters and the scroll position. */
  function navSnap() {
    const sc = $(".content"), c = state.cal;
    return {
      page: state.page, an: { ...state.an }, cw: { ...state.cw }, sort: { ...state.sort }, query: state.query, doiOnly: state.doiOnly,
      open: [...state.open],
      cal: { month: c.month, sel: c.sel ? { ...c.sel } : null, focus: c.focus, source: c.source, article: c.article, year: c.year, shown: c.shown, undatedOpen: c.undatedOpen },
      scroll: sc ? sc.scrollTop : 0,
    };
  }
  /** Runs a cross-page link. When it lands on another page (or Analysis tab) the place left is kept for the back button. */
  function go(fn) {
    const nav = state.nav;
    if (nav.depth > 0) { fn(); return; }
    const before = navSnap(), sig = navSig();
    nav.depth++;
    nav.keep = true;
    try { fn(); } finally { nav.depth--; nav.keep = false; }
    if (navSig() !== sig) {
      nav.stack.push(before);
      if (nav.stack.length > NAV_MAX) nav.stack.shift();
      const sc = $(".content");
      if (sc) sc.scrollTo({ top: 0, behavior: "instant" });
    }
    renderBack();
  }
  function renderBack() {
    const top = state.nav.stack[state.nav.stack.length - 1];
    $("#backBar").hidden = !top || state.page === "credits";
    if (top) $("#backText").textContent = t("nav.back", { where: navName(top) });
  }
  function navBack() {
    const nav = state.nav;
    const e = nav.stack.pop();
    if (!e) return false;
    Object.assign(state.an, e.an);
    Object.assign(state.cw, e.cw);
    Object.assign(state.cal, e.cal);
    state.sort = e.sort;
    state.query = e.query;
    state.doiOnly = e.doiOnly;
    state.open.clear();
    e.open.forEach((k) => state.open.add(k));
    $("#searchBox").value = e.query;
    $("#citingSearch").value = e.cw.query || "";
    nav.keep = true;
    try { showPage(e.page); } finally { nav.keep = false; }
    if (e.page === "analysis") renderAnalysis();
    renderBack();
    // the pages fill in asynchronously: put the scroll position back until the person scrolls on their own
    const sc = $(".content"), tok = ++nav.tok;
    const restore = () => { if (sc && nav.tok === tok) sc.scrollTo({ top: e.scroll, behavior: "instant" }); };
    restore();
    [80, 300, 700].forEach((ms) => setTimeout(restore, ms));
    return true;
  }

  // ------------------------------------------------------------ notification centre
  const NC_ICON = { scholar: "\uE77B", citing: "\uE716", update: "\uE895", health: "\uE7BA", budget: "\uE916" };

  async function loadEvents() {
    const r = await safe(() => invoke("get_events"), null);
    if (!r) return;
    const nc = state.nc;
    nc.events = r.events;
    nc.unread = r.unread;
    if (nc.open) {
      // the list is in front of the person: what arrives meanwhile is read as soon as it is shown
      r.events.forEach((e) => { if (!e.read) nc.seen.add(e.id); });
      if (r.unread) { nc.unread = 0; await safe(() => invoke("mark_events_read"), null); }
    }
    renderNc();
  }

  /** Local day of a timestamp as the group heading: Today, Yesterday or the date. */
  function ncDayLabel(iso) {
    const d = new Date(iso), now = new Date();
    const key = isoDay(d), today = isoDay(now);
    const y = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1);
    if (key === today) return t("nc.today");
    if (key === isoDay(y)) return t("nc.yesterday");
    return d.toLocaleDateString(loc(), { day: "numeric", month: "long", year: "numeric" });
  }

  const ncLine = (key, nav, inner, cls) => key
    ? `<button type="button" class="nc-line${cls ? " " + cls : ""}" data-nc="${nav}" data-key="${esc(key)}">${inner}</button>`
    : `<div class="nc-line${cls ? " " + cls : ""}">${inner}</div>`;

  /** One event as { icon, warn, title, sub, nav, lines } for the list. */
  function ncDescribe(e) {
    const d = e.data || {};
    switch (e.kind) {
      case "scholar": {
        const up = d.up || [], down = d.down || [], nw = d.new || [];
        const upSum = up.reduce((a, x) => a + x.d, 0) + nw.reduce((a, x) => a + (x.n || 0), 0);
        const downSum = -down.reduce((a, x) => a + x.d, 0);
        const total = nf.format(d.total);
        let title;
        if (d.delta > 0) title = t("nc.sch_up", { n: nf.format(d.delta), total });
        else if (upSum > 0) title = t("nc.sch_up", { n: nf.format(upSum), total });
        else if (nw.length) title = t("nc.sch_new", { total });
        else if (d.delta < 0) title = t("nc.sch_down", { n: nf.format(-d.delta), total });
        else if (downSum > 0) title = t("nc.sch_down", { n: nf.format(downSum), total });
        else title = t("nc.sch_flat", { total });
        const lines = [
          ...up.map((x) => ncLine(x.k, "article", `<span class="nc-lt">${esc(x.t)}</span>${chip("", x.d)}`)),
          ...nw.map((x) => ncLine(x.k, "article", `<span class="nc-lt">${esc(t("nc.new_article", { title: x.t }))}</span>${x.n > 0 ? chip("", x.n) : ""}`)),
          ...down.map((x) => ncLine(x.k, "article", `<span class="nc-lt">${esc(x.t)}</span>${chip("", x.d)}`)),
        ];
        return { icon: NC_ICON.scholar, title, nav: "articles", lines };
      }
      case "citing": {
        const items = d.items || [];
        const lines = items.map((x) => ncLine(x.k, "citing", `<span class="nc-lt">${esc(x.t || "?")}</span><span class="nc-ls">${esc([x.v, t("nc.cites", { article: x.a })].filter(Boolean).join(" · "))}</span>`, "stack"));
        if (d.total > items.length) lines.push(`<div class="nc-more">${esc(t("nc.citing_more", { n: nf.format(d.total - items.length) }))}</div>`);
        return { icon: NC_ICON.citing, title: t("nc.citing", { n: nf.format(d.total) }), nav: "citing", lines };
      }
      case "update": return { icon: NC_ICON.update, title: t("nc.update", { version: d.version || "" }), sub: t("nc.update_sub"), nav: "update", lines: [] };
      case "health": return { icon: NC_ICON.health, warn: true, title: t("nc.health"), sub: t("nc.health_sub"), nav: "health", lines: [] };
      case "budget": return { icon: NC_ICON.budget, warn: true, title: t("nc.budget"), sub: t("nc.budget_sub"), nav: "budget", lines: [] };
      default: return null;
    }
  }

  function renderNc() {
    const nc = state.nc, n = nc.unread;
    const badge = $("#ncBadge");
    badge.hidden = n <= 0;
    badge.textContent = n > 99 ? "99+" : String(n);
    const btn = $("#ncBtn");
    btn.setAttribute("aria-label", n > 0 ? t("nc.bell_unread", { n }) : t("nc.bell"));
    btn.setAttribute("aria-expanded", nc.open ? "true" : "false");
    $("#ncPanel").hidden = !nc.open;
    if (!nc.open) return;
    $("#ncClear").disabled = !nc.events.length;
    let html = "", day = "";
    for (const e of nc.events) {
      const v = ncDescribe(e);
      if (!v) continue;
      const label = ncDayLabel(e.at);
      if (label !== day) { day = label; html += `<div class="nc-day" role="heading" aria-level="3">${esc(label)}</div>`; }
      html += `<div class="nc-ev${nc.seen.has(e.id) ? " unread" : ""}"><button type="button" class="nc-hd" data-nc="${v.nav}"><span class="icon nc-ic${v.warn ? " warn" : ""}">${v.icon}</span><span class="nc-tx"><span class="nc-t">${esc(v.title)}</span>${v.sub ? `<span class="nc-s">${esc(v.sub)}</span>` : ""}</span><span class="nc-time">${esc(fmtTime(e.at))}</span></button>${v.lines.length ? `<div class="nc-lines">${v.lines.join("")}</div>` : ""}</div>`;
    }
    $("#ncList").innerHTML = html || `<div class="nc-empty">${esc(t("nc.empty"))}</div>`;
  }

  async function openNc() {
    const nc = state.nc;
    nc.open = true;
    nc.seen = new Set(nc.events.filter((e) => !e.read).map((e) => e.id));
    renderNc();
    await loadEvents();
    const first = $("#ncList button");
    (first || $("#ncPanel")).focus();
  }
  function closeNc(returnFocus) {
    const nc = state.nc;
    if (!nc.open) return;
    nc.open = false;
    nc.seen = new Set();
    renderNc();
    if (returnFocus) $("#ncBtn").focus();
  }
  /** Where an item leads: the article row, the Citing works list, Settings > About, the Fetch log, the API key card. */
  function ncGo(what, key) {
    closeNc(false);
    go(() => {
      if (what === "article") openArticle(key);
      else if (what === "articles") showPage("articles");
      else if (what === "citing") showCitingFor(key || "");
      else if (what === "update") {
        showPage("settings");
        setTimeout(() => { const el = $("#aboutTitle"); if (el) el.scrollIntoView({ block: "center" }); }, 150);
      } else if (what === "health") showPage("log");
      else if (what === "budget") budgetAction("settings");
    });
  }

  // ------------------------------------------------------------ update found: dialog
  let updDlgOpener = null;
  /** The first lines of the release notes, plain text, cut at a line end. */
  function notesExcerpt(notes) {
    const plain = String(notes || "").replace(/\r/g, "").replace(/^#{1,6}\s*/gm, "").replace(/\*\*/g, "").trim();
    if (plain.length <= 600) return plain;
    const cut = plain.slice(0, 600), i = cut.lastIndexOf("\n");
    return (i > 300 ? cut.slice(0, i) : cut).trimEnd() + "…";
  }
  function showUpdateDialog(u) {
    if (!u) return;
    $("#updDlgTitle").textContent = t("update.dlg_title", { version: u.version });
    const notes = notesExcerpt(u.notes);
    $("#updDlgNotes").textContent = notes;
    $("#updDlgNotesWrap").hidden = !notes;
    updDlgOpener = document.activeElement && document.activeElement !== document.body ? document.activeElement : $("#checkUpdateBtn");
    $("#updDialog").hidden = false;
    $("#updDlgNow").focus();
  }
  function closeUpdateDialog() {
    $("#updDialog").hidden = true;
    if (updDlgOpener && updDlgOpener.focus) updDlgOpener.focus();
    updDlgOpener = null;
  }

  // ------------------------------------------------------------ OJP intro bar, Help and Credits pages
  const OJP_INTRO_KEY = "sct.ojp.intro_seen";
  const ojpIntroSeen = () => { try { return localStorage.getItem(OJP_INTRO_KEY) === "1"; } catch { return false; } };
  function renderOjpIntro() {
    const bar = $("#ojpIntroBar");
    if (bar) bar.hidden = state.page !== "citing" || ojpIntroSeen() || state.ojpIntroClosed;
  }
  function closeOjpIntro() {
    state.ojpIntroClosed = true;
    try { localStorage.setItem(OJP_INTRO_KEY, "1"); } catch { /* private window */ }
    renderOjpIntro();
  }
  /** Opens the Help page and scrolls to one of its blocks. */
  function openHelp(anchor) {
    go(() => showPage("help"));
    const el = anchor ? document.getElementById(anchor) : null;
    const sc = $(".content");
    if (el && sc) requestAnimationFrame(() => { sc.scrollTo({ top: sc.scrollTop + el.getBoundingClientRect().top - sc.getBoundingClientRect().top - 8, behavior: "auto" }); });
    else if (sc) sc.scrollTo({ top: 0, behavior: "auto" });
  }
  /** Fills every [data-hk] text (strings with {placeholders} from the OJP data edition) and the Help examples. */
  function renderHk() {
    const v = Object.assign(L.ojp.vars(), { version: state.version || "" });
    $$("[data-hk]").forEach((el) => { el.textContent = t(el.dataset.hk, v); });
    const ex = $("#helpOjpEx");
    if (ex) {
      ex.innerHTML = [12, 48, 76, 97].map((n) => `<span>${L.ojp.sample(n)}</span>`).join("")
        + `<span><span class="ojp ojp-lg ojp-none" aria-hidden="true">NA</span>${esc(t("help.ojp_ex_none"))}</span>`
        + `<span><span class="ojp-dash" aria-hidden="true">NA</span>${esc(t("help.ojp_ex_absent"))}</span>`;
    }
    $("#credAppT").textContent = t("cred.app_t", v);
    $("#credOjpT").textContent = t("cred.ojp_t", v);
    const doi = $("#credOjpDoi");
    if (doi) { doi.dataset.url = "https://doi.org/" + (v.dataset_doi || ""); doi.textContent = "DOI " + (v.dataset_doi || ""); }
    if (state.cred) renderCredRust();
  }
  function renderHelp() { renderHk(); }

  async function fetchText(url) {
    const r = await fetch(url);
    if (!r.ok) throw new Error(url + " " + r.status);
    return r.text();
  }
  /** Lazy: the Rust crate list is read the first time the Credits page opens. */
  async function loadCredits() {
    renderHk();
    if (!state.credCopy) {
      try {
        const line = (await fetchText("licenses/app-MIT.txt")).split(/\r?\n/).find((l) => /^copyright/i.test(l.trim()));
        state.credCopy = line ? line.trim() : " ";
      } catch { state.credCopy = " "; }
    }
    $("#credAppCopy").textContent = state.credCopy;
    if (!state.cred) {
      try { state.cred = await (await fetch("licenses/rust-crates.json")).json(); } catch { state.cred = null; $("#credRustD").textContent = t("cred.load_fail"); return; }
      // The file carries every shipped platform's crates; this build was compiled from one of them.
      const plat = state.platform === "macos" ? "macos" : "windows";
      state.cred.crates = state.cred.crates.filter((x) => !x.platforms || x.platforms.includes(plat));
      renderCredRust();
    }
  }
  function renderCredRust() {
    const c = state.cred;
    if (!c) return;
    $("#credRustD").textContent = t("cred.rust_d", { n: nf.format(c.crates.length) });
    const q = $("#credRustSearch").value.trim().toLowerCase();
    const list = c.crates.map((x, i) => [x, i]).filter(([x]) => !q || `${x.name} ${x.license}`.toLowerCase().includes(q));
    $("#credRustCount").textContent = q ? `${nf.format(list.length)} / ${nf.format(c.crates.length)}` : "";
    $("#credRustTable tbody").innerHTML = list.length ? list.map(([x, i]) => {
      const name = x.repository && /^https:\/\//.test(x.repository) ? `<a href="#" class="ext-link" data-url="${esc(x.repository)}">${esc(x.name)}</a>` : esc(x.name);
      return `<tr><td>${name}</td><td>${esc(x.version)}</td><td>${esc(x.license || "—")}</td><td><button type="button" class="btn btn-link" data-crate="${i}">${esc(t("cred.view_licence"))}</button></td></tr>`;
    }).join("") : `<tr><td colspan="4" class="empty-row">${esc(t("cred.rust_none"))}</td></tr>`;
  }
  let licOpener = null;
  function showLic(title, meta, text, opener) {
    licOpener = opener || null;
    $("#licTitle").textContent = title;
    $("#licMeta").textContent = meta || "";
    $("#licText").textContent = text;
    $("#licText").scrollTop = 0;
    $("#licDialog").hidden = false;
    $("#licClose").focus();
  }
  function closeLic() {
    $("#licDialog").hidden = true;
    if (licOpener && licOpener.isConnected) licOpener.focus();
    licOpener = null;
  }
  const LIC_FILES = {
    app: ["MIT", "licenses/app-MIT.txt"], icons: ["Fluent UI System Icons (MIT)", "licenses/fluentui-system-icons-MIT.txt"], ne: ["Natural Earth", "vendor/LICENSE-natural-earth.txt"],
    echarts: ["Apache ECharts", "vendor/LICENSE-echarts.txt"], "echarts-notice": ["Apache ECharts", "vendor/NOTICE-echarts.txt"], "echarts-d3": ["d3 (BSD-3-Clause, in ECharts)", "vendor/LICENSE-d3.txt"],
  };
  async function openLicense(kind, opener) {
    try {
      if (kind === "ojp") { showLic(t("cred.ojp_t", L.ojp.vars()), "CC BY 4.0", await invoke("get_ojp_notice"), opener); return; }
      const f = LIC_FILES[kind];
      if (f) showLic(f[0], f[1], await fetchText(f[1]), opener);
    } catch { toast(t("cred.load_fail"), true); }
  }
  async function openCrateLicense(i, opener) {
    const x = state.cred && state.cred.crates[i];
    if (!x) return;
    const meta = `${x.version} · ${x.license || "—"}`;
    if (!x.texts.length) { showLic(x.name, meta, t("cred.rust_notext", { license: x.license || "—" }) + (x.repository ? "\n" + x.repository : ""), opener); return; }
    try {
      if (!state.credTexts) state.credTexts = await (await fetch("licenses/rust-license-texts.json")).json();
      const parts = x.texts.map((f) => (x.texts.length > 1 ? t("cred.file_sep", { file: f.file }) + "\n\n" : "") + (state.credTexts[f.id] || ""));
      showLic(x.name, meta, parts.join("\n\n"), opener);
    } catch { toast(t("cred.load_fail"), true); }
  }

  /** Opens the Citing works page filtered to one article. */
  function showCitingFor(key) {
    go(() => {
      Object.assign(state.cw, { article: key, query: "", shown: 200, country: "", venue: "", date: "", period: null, type: "", oa: "", field: "", noSelf: false, sort: "date" });
      $("#citingSearch").value = "";
      showPage("citing");
    });
  }

  function clearFilter(box, st, kinds) {
    box.addEventListener("click", (e) => {
      const b = e.target.closest("button[data-clear]");
      if (!b) return;
      const k = b.dataset.clear;
      if (k === "all") { st.country = ""; st.venue = ""; st.date = ""; st.period = null; if (kinds === "cw") Object.assign(st, { type: "", oa: "", field: "", noSelf: false, sort: "date", dir: -1 }); }
      else if (k === "period") st.period = null;
      else if (k === "noSelf") st.noSelf = false;
      else if (k === "sort") { st.sort = "date"; st.dir = -1; }
      else st[k] = "";
      if (kinds === "cw") { state.cw.shown = 200; renderCiting(); }
      else { state.an.tlShown = 100; renderCalendarTab(); }
    });
  }

  function onTabKey(e) {
    const tabs = $$("#anTabs .tab");
    const i = tabs.findIndex((x) => x.dataset.tab === state.an.tab);
    let n = -1;
    if (e.key === "ArrowRight") n = (i + 1) % tabs.length;
    else if (e.key === "ArrowLeft") n = (i - 1 + tabs.length) % tabs.length;
    else if (e.key === "Home") n = 0;
    else if (e.key === "End") n = tabs.length - 1;
    if (n < 0) return;
    e.preventDefault();
    state.an.tab = tabs[n].dataset.tab;
    renderAnalysis();
    tabs[n].focus();
  }

  /** Translucent window background on/off: applied and saved at once; the page background follows. */
  async function chooseBackdrop(on) {
    const active = await safe(() => invoke("set_backdrop", { enabled: on }), null);
    if (active === null) { $("#setBackdrop").checked = !on; return; }
    document.documentElement.classList.toggle("mica", active);
    if (state.settings) state.settings.backdrop = on;
  }

  /** Applies and saves the theme at once (the Save button is not needed): page colours, charts, native title bar. */
  async function chooseTheme(value) {
    window.SCTTheme.set(value);
    const pref = window.SCTTheme.pref;
    $("#setTheme").value = pref;
    const res = await safe(() => invoke("set_theme", { theme: pref }), null);
    if (res && state.settings) state.settings.theme = res;
  }

  /** Re-draws the charts of the visible page (colour scheme change). */
  function redrawCharts() {
    if (state.page === "panel") renderMini();
    else if (state.page === "analysis") renderAnalysis();
    else if (state.page === "citing") renderDist();
    else if (state.page === "report") renderReportPreview();
    else if (state.page === "articles") state.open.forEach(drawDetail);
  }

  function wire() {
    $$(".nav-item").forEach((b) => b.addEventListener("click", () => showPage(b.dataset.page)));
    // back navigation: the button, the mouse back button, Alt+Left
    $("#backBtn").addEventListener("click", navBack);
    window.addEventListener("mouseup", (e) => { if (e.button === 3) { e.preventDefault(); navBack(); } });
    window.addEventListener("auxclick", (e) => { if (e.button === 3 || e.button === 4) e.preventDefault(); });
    ["wheel", "pointerdown", "keydown"].forEach((ev) => window.addEventListener(ev, () => { state.nav.tok++; }, { passive: true, capture: true }));
    document.addEventListener("keydown", (e) => {
      if (e.key !== "ArrowLeft" || !e.altKey || e.ctrlKey || e.shiftKey || e.metaKey || !state.nav.stack.length) return;
      const a = document.activeElement;
      if (a && (/^(INPUT|TEXTAREA|SELECT)$/.test(a.tagName) || a.isContentEditable)) return;
      if ($(".modal:not([hidden])")) return;
      e.preventDefault();
      navBack();
    });
    // notification centre
    $("#ncBtn").addEventListener("click", () => { if (state.nc.open) closeNc(true); else openNc(); });
    $("#ncClear").addEventListener("click", async () => { await safe(() => invoke("clear_events"), null); await loadEvents(); (state.nc.open ? $("#ncPanel") : $("#ncBtn")).focus(); });
    $("#ncList").addEventListener("click", (e) => {
      const b = e.target.closest("[data-nc]");
      if (b) ncGo(b.dataset.nc, b.dataset.key);
    });
    document.addEventListener("mousedown", (e) => { if (state.nc.open && !e.target.closest(".nc-wrap")) closeNc(false); });
    $(".nc-wrap").addEventListener("focusout", (e) => { if (state.nc.open && e.relatedTarget && !e.relatedTarget.closest(".nc-wrap")) closeNc(false); });
    document.addEventListener("keydown", (e) => {
      if (e.key === "Escape" && state.nc.open && $("#updDialog").hidden) { e.preventDefault(); closeNc(true); }
    });
    // update dialog
    $("#updDlgLater").addEventListener("click", closeUpdateDialog);
    $("#updDlgNow").addEventListener("click", () => { closeUpdateDialog(); installUpdate(); });
    $("#updDialog").addEventListener("click", (e) => { if (e.target === $("#updDialog")) closeUpdateDialog(); });
    document.addEventListener("keydown", (e) => {
      if ($("#updDialog").hidden) return;
      if (e.key === "Escape") { e.preventDefault(); closeUpdateDialog(); return; }
      if (e.key === "Tab") {
        const f = [$("#updDlgNotes"), $("#updDlgLater"), $("#updDlgNow")].filter((x) => x.offsetParent !== null);
        const i = f.indexOf(document.activeElement);
        e.preventDefault();
        f[e.shiftKey ? (i <= 0 ? f.length - 1 : i - 1) : (i + 1) % f.length].focus();
      }
    });
    $("#refreshBtn").addEventListener("click", refreshNow);
    $("#emptyRefresh").addEventListener("click", refreshNow);
    $("#setupForm").addEventListener("submit", startSetup);
    $("#setupProfile").addEventListener("input", updateSetupValidity);
    $("#setupOrcid").addEventListener("input", updateSetupValidity);
    $("#setBackfill").addEventListener("change", (e) => setBackfillEnabled(e.target.checked));
    $("#bfRunBtn").addEventListener("click", runBackfillNow);
    $("#bfClearBtn").addEventListener("click", clearBackfill);
    // panel
    $("#recentAll").addEventListener("click", async () => { state.recentAll = !state.recentAll; await loadRecent(); });
    $("#recentCard").addEventListener("click", (e) => panelActions(e));
    $("#miniOpen").addEventListener("click", () => { state.an.range = "12m"; openAnalysis("trend"); });
    $("#visitSummary").addEventListener("click", (e) => {
      const b = e.target.closest("[data-go]");
      if (!b) return;
      if (b.dataset.go === "article") openArticle(b.dataset.key);
      else openAnalysis("indices");
    });
    // analysis
    $("#anRange").addEventListener("click", (e) => {
      const b = e.target.closest("button[data-range]");
      if (!b) return;
      state.an.range = b.dataset.range;
      if (state.an.range === "custom" && !state.an.from) {
        const to = new Date(), from = new Date(to.getTime() - 365 * DAY_MS);
        state.an.from = isoDay(from);
        state.an.to = isoDay(to);
      }
      renderAnalysis();
    });
    $("#anFrom").addEventListener("change", (e) => { state.an.from = e.target.value; renderAnalysis(); });
    $("#anTo").addEventListener("change", (e) => { state.an.to = e.target.value; renderAnalysis(); });
    $("#anTabs").addEventListener("click", (e) => {
      const b = e.target.closest(".tab");
      if (!b) return;
      state.an.tab = b.dataset.tab;
      renderAnalysis();
    });
    $("#anTabs").addEventListener("keydown", onTabKey);
    $("#anGran").addEventListener("click", (e) => {
      const b = e.target.closest("button[data-gran]");
      if (!b || b.disabled) return;
      state.an.gran = b.dataset.gran;
      renderPeriodChart();
    });
    $$("#risSeg button").forEach((b) => b.addEventListener("click", () => { state.risDays = Number(b.dataset.days); loadRisers(); renderArticlesTab(); }));
    $("#fcGoalSave").addEventListener("click", saveGoal);
    $("#fcGoal").addEventListener("keydown", (e) => { if (e.key === "Enter") saveGoal(); });
    $("#anReport").addEventListener("click", reportThisPeriod);
    window.Citers.init({
      invoke, range: anRangeDays, openList: openCitersList, refresh: refreshNow, openHelp,
      resetRange: () => { state.an.range = "all"; renderAnalysis(); },
      articleTitle: (k) => (state.articles.find((a) => a.key === k) || {}).title || "",
    });
    // calendar
    $("#calSource").addEventListener("click", (e) => {
      const b = e.target.closest("button[data-source]");
      if (!b) return;
      state.cal.source = b.dataset.source;
      renderCalendarTab();
    });
    $("#calArticle").addEventListener("change", (e) => { state.cal.article = e.target.value; renderCalendarTab(); });
    $("#mcPrev").addEventListener("click", () => changeMonth(-1));
    $("#mcNext").addEventListener("click", () => changeMonth(1));
    $("#mcMonth").addEventListener("change", (e) => { const y = state.cal.month.slice(0, 4); state.cal.month = `${y}-${pad2(Number(e.target.value))}`; state.cal.year = null; renderCalendarTab(); });
    $("#mcYear").addEventListener("change", (e) => {
      const y = Math.max(1990, Math.min(2100, parseInt(e.target.value, 10) || new Date().getFullYear()));
      state.cal.month = `${y}-${state.cal.month.slice(5, 7)}`;
      state.cal.year = null;
      renderCalendarTab();
    });
    $("#mcToday").addEventListener("click", () => openCalendarDay(todayIso()));
    $("#mcWhole").addEventListener("click", selectWholeMonth);
    $("#mcGrid").addEventListener("click", (e) => {
      const b = e.target.closest(".mc-day[data-date]");
      if (b) selectDay(b.dataset.date, e.shiftKey);
    });
    $("#mcGrid").addEventListener("keydown", onGridKey);
    $("#cdCiting").addEventListener("click", openCitingFromDetail);
    $("#cdMore").addEventListener("click", () => { state.cal.shown += 120; renderDetail(); });
    $("#calDetail").addEventListener("click", (e) => panelActions(e));
    $("#calUndatedBtn").addEventListener("click", () => { state.cal.undatedOpen = !state.cal.undatedOpen; renderUndatedList(); if (state.cal.undatedOpen) $("#calUndatedCard").scrollIntoView({ block: "nearest", behavior: "smooth" }); });
    $("#cuClose").addEventListener("click", () => { state.cal.undatedOpen = false; renderUndatedList(); });
    $("#recentCal").addEventListener("click", () => openAnalysis("calendar"));
    clearFilter($("#citingChips"), state.cw, "cw");
    $("#page-analysis").addEventListener("click", (e) => {
      const link = e.target.closest("a[data-url]");
      if (link) { e.preventDefault(); safe(() => invoke("open_url", { url: link.dataset.url })); return; }
      const art = e.target.closest("a[data-open-article]");
      if (art) { e.preventDefault(); openArticle(art.dataset.openArticle); }
    });
    // scan panel, DOI bar
    $("#budgetClose").addEventListener("click", () => { if (state.budget) state.budgetDismissed = state.budget.resume_at; renderBudget(); });
    document.addEventListener("click", (e) => {
      const b = e.target.closest("[data-budget]");
      if (b) budgetAction(b.dataset.budget);
    });
    $("#bfPill").addEventListener("click", async () => { if (!state.scan.running && !state.scan.end) await hydrateScan(); openScan(); });
    $("#scanClose").addEventListener("click", closeScan);
    $("#bfScanBtn").addEventListener("click", async () => { if (!state.scan.running && !state.scan.end) await hydrateScan(); openScan(); });
    $("#scanStop").addEventListener("click", stopScan);
    $("#scanPanel").addEventListener("click", (e) => panelActions(e));
    $("#scanPanel").addEventListener("keydown", (e) => { if (e.key === "Escape") closeScan(); });
    $("#doiOnly").addEventListener("change", (e) => { state.doiOnly = e.target.checked; renderArticles(); });
    $("#doiBarClose").addEventListener("click", () => { state.doiBarHidden = true; renderDoiBar(); });
    $("#doiPaste").addEventListener("click", pasteDoi);
    // Preview the language on the first-run card right away; it is saved with "Start".
    $("#setupLang").addEventListener("change", (e) => applyLang(e.target.value));
    $("#searchBox").addEventListener("input", (e) => { state.query = e.target.value; renderArticles(); });
    $$("#articleTable th[data-sort]").forEach((th) => th.addEventListener("click", () => {
      const k = th.dataset.sort;
      state.sort = state.sort.key === k ? { key: k, dir: -state.sort.dir } : { key: k, dir: k === "title" ? 1 : -1 };
      renderArticles();
    }));
    $("#articleBody").addEventListener("click", (e) => {
      const view = e.target.closest("button[data-view]");
      if (view) { state.artView.set(view.dataset.key, view.dataset.view); drawDetail(view.dataset.key); return; }
      const act = e.target.closest("button[data-act]");
      if (act) {
        const k = act.dataset.key;
        if (act.dataset.act === "doi") openDoiDialog(k);
        else if (act.dataset.act === "reject") rejectMatch(k);
        else if (act.dataset.act === "reset") resetOverride(k);
        else if (act.dataset.act === "allciting") showCitingFor(k);
        else if (act.dataset.act === "scholar") safe(() => invoke("open_url", { url: scholarUrl(act.dataset.cites) }));
        else if (act.dataset.act === "morechanges") {
          if (state.changesAll.has(k)) state.changesAll.delete(k); else state.changesAll.add(k);
          drawDetail(k);
        }
        return;
      }
      const link = e.target.closest("a[data-url]");
      if (link) {
        e.preventDefault();
        safe(() => invoke("open_url", { url: link.dataset.url }));
        return;
      }
      const row = e.target.closest("tr.row");
      if (!row) return;
      const k = row.dataset.key;
      if (state.open.has(k)) { state.open.delete(k); } else state.open.add(k);
      renderArticles();
    });
    $("#pickDirBtn").addEventListener("click", async () => {
      const dir = await safe(() => invoke("pick_folder"), null);
      if (dir) { state.dirPending = dir; fillSettings(); showInfo(t("settings.dir_selected"), false); }
    });
    $("#openDirBtn").addEventListener("click", () => safe(() => invoke("open_data_folder")));
    $("#saveBtn").addEventListener("click", saveSettings);
    $("#doiForm").addEventListener("submit", saveDoi);
    $("#doiCancel").addEventListener("click", closeDoiDialog);
    $("#doiDialog").addEventListener("keydown", (e) => { if (e.key === "Escape") closeDoiDialog(); });
    $("#updateNow").addEventListener("click", installUpdate);
    $("#updateLater").addEventListener("click", () => { $("#updateBar").hidden = true; });
    $("#checkUpdateBtn").addEventListener("click", checkUpdatesManually);
    $("#aboutInstall").addEventListener("click", installUpdate);
    // "Installed apps" is a Windows page. Elsewhere there is nothing to open: the hint text says
    // how to remove the app, so the button itself goes away.
    if (state.platform === "windows") {
      $("#uninstallBtn").addEventListener("click", () => safe(() => invoke("open_url", { url: "ms-settings:appsfeatures" })));
    } else {
      $("#uninstallBtn").hidden = true;
    }
    $("#healthOpenLog").addEventListener("click", () => go(() => showPage("log")));
    // citing works
    $("#citingSearch").addEventListener("input", (e) => { state.cw.query = e.target.value; state.cw.shown = 200; renderCiting(); });
    $("#citingArticle").addEventListener("change", (e) => { state.cw.article = e.target.value; state.cw.shown = 200; renderCiting(); });
    for (const [sel, key] of [["#citingType", "type"], ["#citingOa", "oa"], ["#citingField", "field"]]) $(sel).addEventListener("change", (e) => { state.cw[key] = e.target.value; state.cw.shown = 200; renderCiting(); });
    $("#citingNoSelf").addEventListener("change", (e) => { state.cw.noSelf = e.target.checked; state.cw.shown = 200; renderCiting(); });
    $("#citingMore").addEventListener("click", () => { state.cw.shown += 300; renderCiting(); });
    $$("#citingTable th[data-csort]").forEach((th) => th.addEventListener("keydown", (e) => {
      if ((e.key === "Enter" || e.key === " ") && e.target === th) { e.preventDefault(); th.click(); }
    }));
    $$("#citingTable th[data-csort]").forEach((th) => th.addEventListener("click", (e) => {
      if (e.target.closest(".th-help")) return;
      const k = th.dataset.csort;
      if (state.cw.sort === k) state.cw.dir = -state.cw.dir; else { state.cw.sort = k; state.cw.dir = -1; }
      state.cw.shown = 200;
      renderCiting();
    }));
    $("#citingOjpHelp").addEventListener("click", (e) => { e.stopPropagation(); openHelp("help-ojp"); });
    $("#ojpIntroMore").addEventListener("click", () => { closeOjpIntro(); openHelp("help-ojp"); });
    $("#ojpIntroClose").addEventListener("click", closeOjpIntro);
    $("#helpCredits").addEventListener("click", () => showPage("credits"));
    $("#aboutCredits").addEventListener("click", (e) => { e.preventDefault(); showPage("credits"); });
    $("#credBack").addEventListener("click", () => showPage(state.credFrom || "settings"));
    $("#credRustSearch").addEventListener("input", renderCredRust);
    $("#page-credits").addEventListener("click", (e) => {
      const link = e.target.closest("a[data-url]");
      if (link) { e.preventDefault(); safe(() => invoke("open_url", { url: link.dataset.url })); return; }
      const lic = e.target.closest("button[data-lic]");
      if (lic) { openLicense(lic.dataset.lic, lic); return; }
      const cr = e.target.closest("button[data-crate]");
      if (cr) openCrateLicense(Number(cr.dataset.crate), cr);
    });
    $("#licClose").addEventListener("click", closeLic);
    $("#licDialog").addEventListener("click", (e) => { if (e.target === $("#licDialog")) closeLic(); });
    document.addEventListener("keydown", (e) => {
      if ($("#licDialog").hidden) return;
      if (e.key === "Escape") { e.preventDefault(); closeLic(); return; }
      if (e.key === "Tab") {
        const f = [$("#licText"), $("#licClose")];
        const i = f.indexOf(document.activeElement);
        e.preventDefault();
        f[e.shiftKey ? (i <= 0 ? f.length - 1 : i - 1) : (i + 1) % f.length].focus();
      }
    });
    $("#page-help").addEventListener("click", (e) => {
      const link = e.target.closest("a[data-url]");
      if (link) { e.preventDefault(); safe(() => invoke("open_url", { url: link.dataset.url })); }
    });
    $("#citingBody").addEventListener("click", (e) => {
      const link = e.target.closest("a[data-url]");
      if (!link) return;
      e.preventDefault();
      safe(() => invoke("open_url", { url: link.dataset.url }));
    });
    // report
    $$("#repPresets button").forEach((b) => b.addEventListener("click", () => setReportRange(b.dataset.preset)));
    $("#repCreate").addEventListener("click", createReport);
    $("#repCharts").addEventListener("change", scheduleReportPreview);
    $("#repCiters").addEventListener("change", scheduleReportPreview);
    $("#repFrom").addEventListener("change", scheduleReportPreview);
    $("#repTo").addEventListener("change", scheduleReportPreview);
    $("#exportXlsx").addEventListener("click", exportXlsx);
    $("#keyOaSave").addEventListener("click", () => saveKey("openalex"));
    $("#keyOaTest").addEventListener("click", () => testKey("openalex"));
    $("#keyOaDel").addEventListener("click", () => removeKey("openalex"));
    $("#keyOaRequest").addEventListener("click", requestOaKey);
    $("#page-settings").addEventListener("click", (e) => {
      const link = e.target.closest("a[data-url]");
      if (!link) return;
      e.preventDefault();
      safe(() => invoke("open_url", { url: link.dataset.url }));
    });
    $$("[data-export]").forEach((b) => b.addEventListener("click", () => exportCsv(b.dataset.export)));
    // charts follow the colour theme (Settings, or the OS in "system"); their size follows their box (ResizeObserver in charts.js)
    window.SCTTheme.onChange(redrawCharts);
    $("#setTheme").addEventListener("change", (e) => chooseTheme(e.target.value));
    $("#setBackdrop").addEventListener("change", (e) => chooseBackdrop(e.target.checked));
    $("#articleBody").addEventListener("mouseover", (e) => { if (!e.target.closest("tr.detail")) hideTip(); });
  }

  /** Clicks on the "view on Scholar" buttons, article links and the missing-DOI links inside the panel cards and the scan sheet. */
  function panelActions(e) {
    const dayBtn = e.target.closest("[data-day]");
    if (dayBtn) { e.preventDefault(); openCalendarDay(dayBtn.dataset.day); return; }
    const art = e.target.closest("a[data-open-article]");
    if (art) { e.preventDefault(); openArticle(art.dataset.openArticle); return; }
    const a = e.target.closest("[data-act]");
    if (!a) {
      const link = e.target.closest("a[data-url]");
      if (link) { e.preventDefault(); safe(() => invoke("open_url", { url: link.dataset.url })); }
      return;
    }
    e.preventDefault();
    if (a.dataset.act === "scholar") safe(() => invoke("open_url", { url: scholarUrl(a.dataset.cites) }));
    else if (a.dataset.act === "missing-doi") { closeScan(); showMissingDoi(); }
  }

  async function init() {
    state.scan = newScan();
    // The platform wording ("Keychain" or "Credential Manager", "at login" or "when Windows
    // starts") has to be in place before the first label is rendered.
    state.platform = await safe(() => invoke("platform"), "windows");
    window.I18N.setPlatform(state.platform);
    // Provisional language until the saved one is known (avoids empty labels).
    window.I18N.setLang(navigator.language);
    wire();
    if (await safe(() => invoke("backdrop_mica"), false)) document.documentElement.classList.add("mica");
    // the switch is only offered where the system really has a translucent backdrop (Windows 11, macOS)
    $("#backdropCard").hidden = !(await safe(() => invoke("backdrop_supported"), false));
    state.settings = await safe(() => invoke("get_settings"), null);
    applyLang(state.settings?.language || navigator.language);
    // settings.json is the source of truth; the localStorage copy only prevents a first-frame flash
    if (state.settings?.theme) window.SCTTheme.set(state.settings.theme);
    await listen("status", (e) => {
      const was = state.status?.state;
      state.status = e.payload;
      renderHeader();
      if (was && was !== "idle" && e.payload.state === "idle") loadHealth();
    });
    await listen("update-available", (e) => { state.update = e.payload; renderUpdate(); });
    await listen("events-changed", loadEvents);
    await listen("update-progress", (e) => {
      const p = e.payload;
      if (p.done) { updateProgressUi(t("update.installing"), 100); return; }
      const pct = p.total ? Math.min(100, Math.round((p.downloaded / p.total) * 100)) : null;
      updateProgressUi(t("update.downloading", { percent: pct === null ? nf.format(Math.round(p.downloaded / 1024)) + " KB" : pct + "%" }), pct);
    });
    await listen("data-updated", refreshAll);
    await listen("openalex-budget", (e) => { state.budget = e.payload; state.budgetDismissed = ""; renderBudget(); });
    await listen("backfill-progress", (e) => {
      const wasRunning = !!state.bfProgress?.running;
      scanLogEvent(e.payload);
      if (wasRunning && !e.payload.running) loadBfInfo();
    });
    await listen("backfill-updated", async () => {
      state.bf.clear();
      const jobs = [loadBfInfo(), loadTrend(), loadCiting(), loadDist(), loadNext(), loadRecent(), loadArticles(), loadVisit()];
      if (state.page === "analysis" || state.page === "report" || state.analysis) jobs.push(loadAnalysis());
      if (state.page === "analysis") jobs.push(loadRisers());
      await Promise.all(jobs);
      state.open.forEach(drawDetail);
    });
    await listen("api-key-rejected", (e) => {
      const k = KEYS[e.payload];
      toast(t("keys.rejected", { name: k ? k.name : String(e.payload) }), true);
    });
    state.version = await safe(() => invoke("app_version"), "");
    state.ojpMeta = await safe(() => invoke("get_ojp_meta"), null);
    L.ojp.setMeta(state.ojpMeta);
    renderAbout();
    state.update = await safe(() => invoke("get_pending_update"), null);
    renderUpdate();
    await hydrateScan();
    await loadStatus();
    loadEvents();
    await refreshAll();
    $("#page-panel").hidden = false;
    setInterval(renderHeader, 30000);
    // when the resume time passes the notice goes away (and the data is asked for again)
    setInterval(() => { if (state.budget && !activeBudget()) { state.budget = null; renderBudget(); } }, 30000);
    setInterval(renderScanTime, 1000);
  }
  init();
})();
