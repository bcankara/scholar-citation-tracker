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
    range: "30",
    showSince: false,
    sort: { key: "citations", dir: -1 },
    query: "",
    open: new Set(),
    history: new Map(),
    page: "panel",
    bfInfo: null,
    bfProgress: null,
    hist: null,
    histGran: null,
    histRange: { day: "365", month: "12" },
    histYearBf: false,
    bf: new Map(),
    health: null,
    update: null,
    updating: false,
    version: "",
    doiKey: null,
    forecast: null,
    risers: null,
    risDays: 30,
    next: null,
    citing: [],
    citingGroups: [],
    citingLoaded: false,
    cw: { query: "", article: "", dir: -1, shown: 200 },
    dist: null,
    keys: null,
    keyMsg: { openalex: null, semanticscholar: null },
    firstDay: null,
  };

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
  async function loadSeries() { state.series = await safe(() => invoke("get_series"), []); renderCharts(); }
  async function loadArticles() { state.articles = await safe(() => invoke("get_articles"), []); renderArticles(); }
  async function loadLog() { state.log = await safe(() => invoke("get_log"), []); renderLog(); }
  async function loadBfInfo() {
    state.bfInfo = await safe(() => invoke("get_backfill_info"), state.bfInfo);
    if (state.bfInfo) state.bfProgress = state.bfInfo.progress;
    renderHeader();
    renderBackfill();
  }
  async function loadHealth() { state.health = await safe(() => invoke("get_health"), state.health); renderHealth(); }
  async function loadHistory() { state.hist = await safe(() => invoke("get_history"), state.hist); renderHistory(); }
  async function refreshAll() {
    state.history.clear();
    state.bf.clear();
    await Promise.all([
      loadOverview(), loadSeries(), loadArticles(), loadLog(), loadBfInfo(), loadHistory(), loadHealth(),
      loadForecast(), loadRisers(), loadNext(), loadCiting(), loadDist(),
    ]);
  }

  // ------------------------------------------------------------ language
  /** Switches the UI language and re-renders everything that has text. */
  function applyLang(lang) {
    window.I18N.setLang(lang);
    nf = new Intl.NumberFormat(loc());
    $("#setupLang").value = window.I18N.lang;
    $("#setLang").value = window.I18N.lang;
    renderHeader();
    renderPanel();
    renderArticles();
    renderLog();
    renderBackfill();
    renderHealth();
    renderUpdate();
    renderAbout();
    renderForecast();
    renderRisers();
    renderNext();
    renderCiting();
    renderKeys();
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

  async function installUpdate() {
    if (!state.update || state.updating) return;
    state.updating = true;
    $("#updateProgress").hidden = false;
    $("#updateBarFill").style.width = "0%";
    $("#updateText").textContent = t("update.downloading", { percent: "0%" });
    renderUpdate();
    try {
      await invoke("install_update");
      // On Windows the installer ends this process; if we are still here, restart explicitly.
      $("#updateText").textContent = t("update.installing");
      await window.__TAURI__.process.relaunch();
    } catch (e) {
      state.updating = false;
      renderUpdate();
      $("#updateText").textContent = t("update.failed", { error: String(e) });
    }
  }

  async function checkUpdatesManually() {
    const btn = $("#checkUpdateBtn");
    btn.disabled = true;
    $("#checkUpdateText").textContent = t("about.checking");
    try {
      const r = await invoke("check_for_updates");
      if (r.status === "available") { state.update = r.info; renderUpdate(); showInfo(t("about.available", { version: r.info.version }), false); }
      else if (r.status === "current") showInfo(t("about.current", { version: state.version }), false);
      else if (r.status === "not_found") showInfo(t("about.not_found"), false);
      else showInfo(t("about.error", { error: r.error || "?" }), true);
    } catch (e) {
      showInfo(t("about.error", { error: String(e) }), true);
    } finally {
      btn.disabled = false;
      $("#checkUpdateText").textContent = t("about.check");
    }
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
      $("#bfPill").title = `${t("bf.pill_title", { source: bp.source || "…" })} ${t("bf.progress_slow")}`;
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
  function kpiCard(label, value, small, sub, d) {
    return `<div class="card kpi">
      <div class="kpi-label">${esc(label)}</div>
      <div class="kpi-value">${nf.format(value)}${small !== null ? `<small>/ ${nf.format(small)}</small>` : ""}</div>
      <div class="kpi-sub">${esc(sub)}</div>
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
    if (none || !has) return;
    const s = ov.snapshot;
    const d = ov.deltas;
    const z = { today: 0, d7: 0, d30: 0 };
    const y = { year: s.since_year };
    $("#kpis").innerHTML = [
      kpiCard(t("kpi.total"), s.cit_all, null, t("kpi.total_sub"), d?.cit_all || z),
      kpiCard(t("kpi.since", y), s.cit_since, null, t("kpi.since_sub"), d?.cit_since || z),
      kpiCard(t("kpi.h"), s.h_all, s.h_since, t("kpi.both_sub", y), d?.h_all || z),
      kpiCard(t("kpi.i10"), s.i10_all, s.i10_since, t("kpi.both_sub", y), d?.i10_all || z),
    ].join("");
    $("#sinceToggleLabel").textContent = t("chart.since_toggle", y);
    renderCharts();
    renderForecast();
    renderRisers();
    renderNext();
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

  // ------------------------------------------------------------ charts
  function niceTicks(min, max, count) {
    if (max === min) { max += 1; min -= 1; }
    const raw = (max - min) / count;
    const mag = Math.pow(10, Math.floor(Math.log10(raw)));
    const norm = raw / mag;
    const step = (norm <= 1 ? 1 : norm <= 2 ? 2 : norm <= 5 ? 5 : 10) * mag;
    const start = Math.floor(min / step) * step;
    const end = Math.ceil(max / step) * step;
    const out = [];
    for (let v = start; v <= end + step / 2; v += step) out.push(Math.round(v * 1e6) / 1e6);
    return out;
  }

  const tip = $("#tooltip");
  function showTip(html, x, y) {
    tip.innerHTML = html;
    tip.hidden = false;
    const r = tip.getBoundingClientRect();
    let left = x + 14, top = y - r.height - 10;
    if (left + r.width > window.innerWidth - 8) left = x - r.width - 14;
    if (top < 8) top = y + 14;
    tip.style.left = Math.max(8, left) + "px";
    tip.style.top = top + "px";
  }
  const hideTip = () => { tip.hidden = true; };

  /** series: [{pts:[{t,v}], cls, name}] ; first series gets the area fill and the markers. */
  function lineChart(el, series, o = {}) {
    const w = el.clientWidth, h = el.clientHeight;
    const all = series.flatMap((s) => s.pts);
    if (!all.length || w < 60) { el.innerHTML = `<div class="chart-empty">${esc(t("chart.no_data"))}</div>`; return; }
    const m = { l: o.small ? 34 : 44, r: 16, t: 12, b: 26 };
    let tMin = Math.min(...all.map((p) => p.t)), tMax = Math.max(...all.map((p) => p.t));
    if (tMin === tMax) { tMin -= 12 * 3600e3; tMax += 12 * 3600e3; }
    const vMin = Math.min(...all.map((p) => p.v)), vMax = Math.max(...all.map((p) => p.v));
    const ticks = niceTicks(vMin, vMax, o.small ? 3 : 4);
    const y0 = ticks[0], y1 = ticks[ticks.length - 1];
    const X = (tv) => m.l + ((tv - tMin) / (tMax - tMin)) * (w - m.l - m.r);
    const Y = (v) => m.t + (1 - (v - y0) / (y1 - y0)) * (h - m.t - m.b);

    let svg = `<svg viewBox="0 0 ${w} ${h}" width="${w}" height="${h}" role="img">`;
    for (const tv of ticks) {
      svg += `<line class="grid-line" x1="${m.l}" x2="${w - m.r}" y1="${Y(tv)}" y2="${Y(tv)}"/>`;
      svg += `<text x="${m.l - 8}" y="${Y(tv) + 4}" text-anchor="end">${nf.format(tv)}</text>`;
    }
    const span = tMax - tMin;
    const nx = Math.max(2, Math.min(5, Math.floor((w - m.l - m.r) / 110)));
    for (let i = 0; i < nx; i++) {
      const tt = tMin + (span * i) / (nx - 1);
      const d = new Date(tt);
      const lab = span < 2 * 86400e3
        ? d.toLocaleTimeString(loc(), { hour: "2-digit", minute: "2-digit" })
        : d.toLocaleDateString(loc(), { day: "numeric", month: "short" });
      const anchor = i === 0 ? "start" : i === nx - 1 ? "end" : "middle";
      svg += `<text x="${X(tt)}" y="${h - 6}" text-anchor="${anchor}">${esc(lab)}</text>`;
    }
    series.forEach((s, idx) => {
      if (!s.pts.length) return;
      const d = s.pts.map((p, i) => `${i ? "L" : "M"}${X(p.t).toFixed(1)},${Y(p.v).toFixed(1)}`).join(" ");
      if (idx === 0 && s.pts.length > 1) {
        const base = h - m.b;
        svg += `<path class="area" d="${d} L${X(s.pts[s.pts.length - 1].t).toFixed(1)},${base} L${X(s.pts[0].t).toFixed(1)},${base} Z"/>`;
      }
      if (s.pts.length > 1) svg += `<path class="${s.cls === "alt" ? "line-alt" : "line-main"}" d="${d}"/>`;
    });
    const last = series[0].pts[series[0].pts.length - 1];
    svg += `<g id="hov" visibility="hidden"><line class="hover-line" y1="${m.t}" y2="${h - m.b}"/><circle class="dot-main" r="5"/></g>`;
    svg += `<circle class="dot-main" cx="${X(last.t)}" cy="${Y(last.v)}" r="${series[0].pts.length > 1 ? 3.5 : 5}"/>`;
    svg += `<rect class="hit" x="${m.l}" y="${m.t}" width="${w - m.l - m.r}" height="${h - m.t - m.b}" fill="transparent"/></svg>`;
    el.innerHTML = svg;

    const hit = $("rect.hit", el), hov = $("#hov", el);
    hit.addEventListener("mousemove", (ev) => {
      const rect = el.getBoundingClientRect();
      const px = ev.clientX - rect.left;
      let best = series[0].pts[0];
      for (const p of series[0].pts) if (Math.abs(X(p.t) - px) < Math.abs(X(best.t) - px)) best = p;
      hov.setAttribute("visibility", "visible");
      $("line", hov).setAttribute("x1", X(best.t)); $("line", hov).setAttribute("x2", X(best.t));
      const c = $("circle", hov); c.setAttribute("cx", X(best.t)); c.setAttribute("cy", Y(best.v));
      let html = `<div class="tt-t">${esc(fmtDateTime(new Date(best.t).toISOString()))}</div>`;
      for (const s of series) {
        const p = s.pts.reduce((a, b) => (Math.abs(b.t - best.t) < Math.abs(a.t - best.t) ? b : a), s.pts[0]);
        if (p) html += `<div><span class="tt-v">${nf.format(p.v)}</span> <span class="tt-t">${esc(s.name)}</span></div>`;
      }
      showTip(html, ev.clientX, ev.clientY);
    });
    hit.addEventListener("mouseleave", () => { hov.setAttribute("visibility", "hidden"); hideTip(); });
  }

  function barChart(el, data) {
    const w = el.clientWidth, h = el.clientHeight;
    if (!data.length || w < 60) { el.innerHTML = `<div class="chart-empty">${esc(t("chart.no_data"))}</div>`; return; }
    const m = { l: 40, r: 8, t: 22, b: 26 };
    const ticks = niceTicks(0, Math.max(...data.map((d) => d[1])), 4);
    const y1 = ticks[ticks.length - 1];
    const Y = (v) => m.t + (1 - v / y1) * (h - m.t - m.b);
    const band = (w - m.l - m.r) / data.length;
    const bw = Math.min(40, band * 0.62);
    let svg = `<svg viewBox="0 0 ${w} ${h}" width="${w}" height="${h}" role="img">`;
    for (const tv of ticks) {
      svg += `<line class="grid-line" x1="${m.l}" x2="${w - m.r}" y1="${Y(tv)}" y2="${Y(tv)}"/>`;
      svg += `<text x="${m.l - 8}" y="${Y(tv) + 4}" text-anchor="end">${nf.format(tv)}</text>`;
    }
    data.forEach(([year, count], i) => {
      const cx = m.l + band * (i + 0.5);
      const top = Y(count), base = Y(0);
      const bh = Math.max(count > 0 ? 2 : 0, base - top);
      svg += `<rect class="bar" x="${cx - bw / 2}" y="${base - bh}" width="${bw}" height="${bh}" rx="3" data-y="${year}" data-c="${count}"/>`;
      svg += `<text class="bar-label" x="${cx}" y="${base - bh - 6}" text-anchor="middle">${nf.format(count)}</text>`;
      svg += `<text x="${cx}" y="${h - 6}" text-anchor="middle">${year}</text>`;
    });
    svg += `</svg>`;
    el.innerHTML = svg;
    $$("rect.bar", el).forEach((r) => {
      r.addEventListener("mousemove", (ev) =>
        showTip(`<div class="tt-t">${r.dataset.y}</div><div><span class="tt-v">${nf.format(+r.dataset.c)}</span> <span class="tt-t">${esc(t("chart.series_cites"))}</span></div>`, ev.clientX, ev.clientY));
      r.addEventListener("mouseleave", hideTip);
    });
  }

  function seriesPoints(key) {
    const now = Date.now();
    const pts = state.series.map((p) => ({ t: new Date(p.t).getTime(), v: p[key] }));
    if (state.range === "all") return pts;
    const cutoff = now - Number(state.range) * 86400e3;
    const inRange = pts.filter((p) => p.t >= cutoff);
    const before = pts.filter((p) => p.t < cutoff).pop();
    return before ? [before, ...inRange] : inRange;
  }

  function renderCharts() {
    if (state.page !== "panel" || $("#panelData").hidden) return;
    $$("#rangeSeg button").forEach((b) => b.classList.toggle("on", b.dataset.range === state.range));
    const since = state.overview?.snapshot?.since_year;
    const series = [{ pts: seriesPoints("cit_all"), cls: "main", name: t("chart.series_total") }];
    if (state.showSince) series.push({ pts: seriesPoints("cit_since"), cls: "alt", name: t("chart.series_since", { year: since }) });
    if (!series[0].pts.length) { $("#lineChart").innerHTML = `<div class="chart-empty">${esc(t("chart.no_data_range"))}</div>`; }
    else lineChart($("#lineChart"), series);
    barChart($("#barChart"), state.overview?.yearly || []);
    renderHistory();
  }

  // ------------------------------------------------------------ history chart
  const DAY_MS = 86400e3;
  const pad2 = (n) => String(n).padStart(2, "0");
  const ymd = (ms) => { const d = new Date(ms); return `${d.getUTCFullYear()}-${pad2(d.getUTCMonth() + 1)}-${pad2(d.getUTCDate())}`; };
  const keyMs = (key) => { const [y, m = 1, d = 1] = key.split("-").map(Number); return Date.UTC(y, m - 1, d); };

  /** Continuous periods (zeros filled) from the first data point to today, with running totals. */
  function buildPeriods(points, gran) {
    if (!points.length) return [];
    const map = new Map(points);
    const first = keyMs(points[0][0]);
    const last = keyMs(points[points.length - 1][0]);
    const now = new Date();
    const end = Math.max(last, Date.UTC(now.getFullYear(), now.getMonth(), now.getDate()));
    const out = [];
    let cum = 0;
    const push = (key, tms) => { const v = map.get(key) || 0; cum += v; out.push({ key, t: tms, v, c: cum }); };
    if (gran === "day") {
      for (let tms = first; tms <= end; tms += DAY_MS) push(ymd(tms), tms);
    } else if (gran === "month") {
      let y = new Date(first).getUTCFullYear(), m = new Date(first).getUTCMonth();
      const ey = new Date(end).getUTCFullYear(), em = new Date(end).getUTCMonth();
      while (y < ey || (y === ey && m <= em)) {
        push(`${y}-${pad2(m + 1)}`, Date.UTC(y, m, 1));
        if (++m > 11) { m = 0; y++; }
      }
    } else {
      for (let y = new Date(first).getUTCFullYear(); y <= new Date(end).getUTCFullYear(); y++) push(String(y), Date.UTC(y, 0, 1));
    }
    return out;
  }

  function periodLabel(p, gran, long, small, withYear) {
    const d = new Date(p.t);
    if (gran === "year") return String(d.getUTCFullYear());
    if (gran === "month") return d.toLocaleDateString(loc(), { month: long ? "long" : "short", year: small ? "2-digit" : "numeric", timeZone: "UTC" });
    return d.toLocaleDateString(loc(), { day: "numeric", month: long ? "long" : "short", year: long ? "numeric" : withYear ? "2-digit" : undefined, timeZone: "UTC" });
  }

  /** Right-axis ticks lined up with the n grid intervals of the left axis. */
  function alignedTicks(max, n) {
    const raw = Math.max(max, 1) / n;
    const mag = Math.pow(10, Math.floor(Math.log10(raw)));
    const norm = raw / mag;
    const step = (norm <= 1 ? 1 : norm <= 2 ? 2 : norm <= 5 ? 5 : 10) * mag;
    return Array.from({ length: n + 1 }, (_, i) => Math.round(i * step * 1e6) / 1e6);
  }

  /** periods: [{key,t,v,c}] -> bars (new per period) and, optionally, a cumulative line (right axis). */
  function historyChart(el, periods, o = {}) {
    const w = el.clientWidth, h = el.clientHeight;
    if (!periods.length || w < 60) { el.innerHTML = `<div class="chart-empty">${esc(o.emptyText || t("chart.no_data"))}</div>`; return; }
    const cum = o.cumulative !== false;
    const m = { l: o.small ? 34 : 44, r: cum ? (o.small ? 34 : 52) : 12, t: 12, b: 26 };
    const ticks = niceTicks(0, Math.max(1, ...periods.map((p) => p.v)), o.small ? 3 : 4);
    const y1 = ticks[ticks.length - 1];
    const nInt = ticks.length - 1;
    const cTicks = cum ? alignedTicks(periods[periods.length - 1].c, nInt) : [];
    const c1 = cTicks.length ? cTicks[cTicks.length - 1] : 1;
    const pw = w - m.l - m.r;
    const band = pw / periods.length;
    const X = (i) => m.l + band * (i + 0.5);
    const Y = (v) => m.t + (1 - v / y1) * (h - m.t - m.b);
    const Yc = (v) => m.t + (1 - v / c1) * (h - m.t - m.b);
    const base = Y(0);

    let svg = `<svg viewBox="0 0 ${w} ${h}" width="${w}" height="${h}" role="img">`;
    ticks.forEach((tv, i) => {
      svg += `<line class="grid-line" x1="${m.l}" x2="${w - m.r}" y1="${Y(tv)}" y2="${Y(tv)}"/>`;
      svg += `<text x="${m.l - 8}" y="${Y(tv) + 4}" text-anchor="end">${nf.format(tv)}</text>`;
      if (cum) svg += `<text class="axis-r" x="${w - m.r + 8}" y="${Y(tv) + 4}" text-anchor="start">${nf.format(cTicks[i])}</text>`;
    });
    const bw = Math.max(1, Math.min(32, band * 0.72));
    periods.forEach((p, i) => {
      if (!p.v) return;
      const bh = Math.max(1.5, base - Y(p.v));
      svg += `<rect class="bar" x="${(X(i) - bw / 2).toFixed(2)}" y="${(base - bh).toFixed(2)}" width="${bw.toFixed(2)}" height="${bh.toFixed(2)}" rx="${bw > 6 ? 2 : 0}"/>`;
    });
    if (cum && periods.length > 1) {
      const d = periods.map((p, i) => `${i ? "L" : "M"}${X(i).toFixed(1)},${Yc(p.c).toFixed(1)}`).join(" ");
      svg += `<path class="line-cum" d="${d}"/>`;
    }
    const nx = Math.max(2, Math.min(7, Math.floor(pw / 96)));
    const shown = Math.min(nx, periods.length);
    const seen = new Set();
    for (let k = 0; k < shown; k++) {
      const i = shown === 1 ? 0 : Math.round((k * (periods.length - 1)) / (shown - 1));
      if (seen.has(i)) continue;
      seen.add(i);
      const anchor = i === 0 ? "start" : i === periods.length - 1 ? "end" : "middle";
      const x = anchor === "start" ? m.l : anchor === "end" ? w - m.r : X(i);
      svg += `<text x="${x}" y="${h - 6}" text-anchor="${anchor}">${esc(periodLabel(periods[i], o.gran, false, o.small, periods.length > 120))}</text>`;
    }
    svg += `<rect class="hist-hover" x="0" y="${m.t}" width="${Math.max(band, 1)}" height="${h - m.t - m.b}" visibility="hidden"/>`;
    svg += `<rect class="hit" x="${m.l}" y="${m.t}" width="${pw}" height="${h - m.t - m.b}" fill="transparent"/></svg>`;
    el.innerHTML = svg;

    const hit = $("rect.hit", el), hov = $("rect.hist-hover", el);
    hit.addEventListener("mousemove", (ev) => {
      const px = ev.clientX - el.getBoundingClientRect().left;
      const i = Math.max(0, Math.min(periods.length - 1, Math.floor((px - m.l) / band)));
      const p = periods[i];
      hov.setAttribute("x", m.l + band * i);
      hov.setAttribute("visibility", "visible");
      let html = `<div class="tt-t">${esc(periodLabel(p, o.gran, true, false))}</div>`;
      html += `<div><span class="tt-v">${nf.format(p.v)}</span> <span class="tt-t">${esc(t("hist.tt_new"))}</span></div>`;
      if (cum) html += `<div><span class="tt-v">${nf.format(p.c)}</span> <span class="tt-t">${esc(t("hist.tt_cum"))}</span></div>`;
      showTip(html, ev.clientX, ev.clientY);
    });
    hit.addEventListener("mouseleave", () => { hov.setAttribute("visibility", "hidden"); hideTip(); });
  }

  const HIST_RANGES = {
    day: [["30", "hist.d30"], ["90", "hist.d90"], ["365", "hist.m12"], ["all", "hist.all"]],
    month: [["12", "hist.m12"], ["36", "hist.y3"], ["all", "hist.all"]],
    year: [],
  };

  function renderHistory() {
    if (state.page !== "panel" || $("#panelData").hidden || !state.hist) return;
    const h = state.hist;
    if (state.histGran === null) state.histGran = h.month.total > 0 ? "month" : "year";
    const gran = state.histGran;
    $$("#histGran button").forEach((b) => b.classList.toggle("on", b.dataset.gran === gran));
    const ranges = HIST_RANGES[gran];
    const range = state.histRange[gran];
    const seg = $("#histRange");
    seg.hidden = !ranges.length;
    seg.innerHTML = ranges.map(([v, k]) => `<button data-range="${v}" class="${v === range ? "on" : ""}">${esc(t(k))}</button>`).join("");
    $("#histYearSrcWrap").hidden = gran !== "year";
    $("#histYearSrc").checked = state.histYearBf;

    const scholarYear = gran === "year" && !state.histYearBf;
    const series = h[gran];
    const points = scholarYear
      ? (state.overview?.yearly || []).map(([y, c]) => [String(y), c])
      : series.points;
    $("#histCaption").textContent = t(scholarYear ? "hist.src_scholar" : "hist.src_backfill");
    const note = $("#histNote");
    const showNote = !scholarYear && series.excluded > 0;
    note.hidden = !showNote;
    if (showNote) note.textContent = t("hist.note_excluded", { n: nf.format(series.excluded) });

    let periods = buildPeriods(points, gran);
    if (range !== "all" && ranges.length) periods = periods.slice(-Number(range));
    const running = !!state.bfProgress?.running;
    const emptyText = running ? t("hist.empty_running") : t("hist.empty");
    historyChart($("#histChart"), periods, { gran, cumulative: true, emptyText });
  }

  // ------------------------------------------------------------ articles
  function renderArticles() {
    const q = state.query.trim().toLocaleLowerCase(loc());
    let rows = state.articles.filter((a) => !q || `${a.title} ${a.authors} ${a.venue}`.toLocaleLowerCase(loc()).includes(q));
    const { key, dir } = state.sort;
    rows = rows.slice().sort((a, b) => {
      let r;
      if (key === "title") r = a.title.localeCompare(b.title, loc());
      else r = (a[key] ?? -1) - (b[key] ?? -1);
      return r * dir || a.title.localeCompare(b.title, loc());
    });
    $("#articleCount").textContent = t("articles.count", { n: nf.format(rows.length) });
    $$("#articleTable th[data-sort]").forEach((th) => {
      const old = $(".sort", th); if (old) old.remove();
      if (th.dataset.sort === key) th.insertAdjacentHTML("beforeend", `<span class="sort">${dir > 0 ? "" : ""}</span>`);
    });
    const body = $("#articleBody");
    if (!rows.length) {
      body.innerHTML = `<tr><td colspan="5" class="empty-row">${esc(state.articles.length ? t("articles.none_match") : t("articles.none"))}</td></tr>`;
      return;
    }
    body.innerHTML = rows.map((a) => {
      const open = state.open.has(a.key);
      return `<tr class="row ${open ? "open" : ""}" data-key="${esc(a.key)}">
        <td><div class="title-cell"><span class="chev"></span><div>
          <a href="#" class="title-link" data-url="${esc(a.url)}">${esc(a.title)}</a>${a.is_new ? `<span class="badge-new">${esc(t("articles.new"))}</span>` : ""}
          <div class="title-meta">${esc(a.venue)}</div></div></div></td>
        <td class="num">${a.year ?? "—"}</td>
        <td class="num"><b>${nf.format(a.citations)}</b></td>
        <td class="num">${chip("", a.today)}</td>
        <td class="num">${chip("", a.d7)}</td>
      </tr>${open ? `<tr class="detail" data-for="${esc(a.key)}"><td colspan="5"><div class="detail-grid">
        <div class="detail-col"><div class="detail-sub caption secondary">${esc(t("art.scholar_hist"))}</div><div class="chart-area d-main"></div></div>
        <div class="detail-col"><div class="detail-sub caption secondary">${esc(t("art.bf_title"))}</div><div class="chart-area d-bf"></div><div class="d-match caption"></div><div class="d-actions"></div></div>
        <div class="detail-col detail-wide"><div class="detail-sub caption secondary">${esc(t("cw.in_article"))} <span class="d-citing-n"></span></div><div class="d-citing"></div></div>
      </div></td></tr>` : ""}`;
    }).join("");
    state.open.forEach(drawDetail);
  }

  /** Source counts, how the match was made and links for one article's backfill record. */
  function matchHtml(b) {
    const m = b?.match;
    if (!m) return `<span class="secondary">${esc(t("art.bf_none"))}</span>`;
    const manual = m.match_method === "manual_doi";
    if (!m.source && manual) return `<span class="badge-none">${esc(t("art.bf_unmatched"))}</span> <span class="secondary">${esc(t("art.bf_doi_not_found"))}</span> <b>${esc(m.doi || "")}</b>`;
    if (!m.source) return `<span class="badge-none">${esc(t("art.bf_unmatched"))}</span> <span class="secondary">${esc(t("art.bf_unmatched_hint"))}</span>`;
    const c = (n) => (n === null || n === undefined ? "—" : nf.format(n));
    const chosen = m.source === "openalex" ? "OpenAlex" : "Semantic Scholar";
    const parts = [`<span class="bf-sources">OpenAlex <b>${c(m.openalex_count)}</b> · Semantic Scholar <b>${c(m.s2_count)}</b> → <b>${chosen}</b></span>`];
    const methodKey = manual ? "art.method_manual" : m.match_method === "orcid" ? "art.method_orcid" : "art.method_title";
    parts.push(`<span class="secondary">${esc(t(methodKey))}${m.score != null && !manual ? " · " + esc(t("art.bf_score", { s: m.score.toFixed(2) })) : ""}</span>`);
    if (b.override?.rejected > 0) parts.push(`<span class="secondary">${esc(t("art.rejected_n", { n: nf.format(b.override.rejected) }))}</span>`);
    const links = [];
    if (m.source === "openalex" && m.external_id) links.push([`https://openalex.org/${m.external_id}`, t("art.open_openalex")]);
    if (m.source === "semanticscholar" && m.external_id) links.push([`https://www.semanticscholar.org/paper/${m.external_id}`, t("art.open_s2")]);
    if (m.doi) links.push([`https://doi.org/${encodeURI(m.doi)}`, t("art.open_doi")]);
    for (const [url, label] of links) parts.push(`<a href="#" class="ext-link" data-url="${esc(url)}">${esc(label)}</a>`);
    if (b.year_only > 0) parts.push(`<span class="secondary">${esc(t("art.year_only", { n: nf.format(b.year_only) }))}</span>`);
    return parts.join(" ");
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
    const row = $(`tr.detail[data-for="${CSS.escape(key)}"]`);
    if (!row) return;
    const pts = hist.map(([ts, v]) => ({ t: new Date(ts).getTime(), v }));
    lineChart($(".d-main", row), [{ pts, cls: "main", name: t("chart.series_cites") }], { small: true });
    const monthly = bfd?.monthly || [];
    const periods = buildPeriods(monthly, "month").slice(-120);
    historyChart($(".d-bf", row), periods, { gran: "month", cumulative: false, small: true, emptyText: t("art.bf_empty") });
    $(".d-match", row).innerHTML = matchHtml(bfd);
    $(".d-actions", row).innerHTML = actionsHtml(key, bfd);
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

  /** DOI override actions of one article's detail row. */
  function actionsHtml(key, b) {
    const m = b?.match;
    const k = esc(key);
    const btn = (act, icon, label) => `<button class="btn" data-act="${act}" data-key="${k}"><span class="icon">${icon}</span><span>${esc(t(label))}</span></button>`;
    const out = [btn("doi", "&#xE70F;", "art.act_doi")];
    if (m?.source && m.match_method !== "manual_doi") out.push(btn("reject", "&#xE711;", "art.act_reject"));
    if (b?.override) out.push(btn("reset", "&#xE777;", "art.act_reset"));
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
    $("#doiInput").value = cached?.override?.doi || cached?.match?.doi || "";
    $("#doiError").hidden = true;
    $("#doiDialog").hidden = false;
    $("#doiInput").focus();
    $("#doiInput").select();
  }
  function closeDoiDialog() { $("#doiDialog").hidden = true; state.doiKey = null; }
  async function saveDoi(ev) {
    ev.preventDefault();
    const key = state.doiKey;
    const doi = $("#doiInput").value.trim();
    const err = $("#doiError");
    if (!doi) { err.textContent = t("art.err_doi_empty"); err.hidden = false; return; }
    $("#doiSave").disabled = true;
    try {
      const status = await invoke("set_article_doi", { key, doi });
      closeDoiDialog();
      overrideDone(status, key);
    } catch (e) {
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
    try { overrideDone(await invoke("reject_article_match", { key }), key); } catch (e) { toast(String(e), true); }
  }
  async function resetOverride(key) {
    try {
      const status = await invoke("reset_article_override", { key });
      overrideDone(status, key);
      toast(t("art.reset_done"), false);
    } catch (e) { toast(String(e), true); }
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
  const RUN_ST = { ok: "bf.st_ok", partial: "bf.st_partial", error: "bf.st_error", interrupted: "bf.st_interrupted", running: "bf.st_running" };

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
      const pct = p.n > 0 ? Math.round((p.k / p.n) * 100) : 0;
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
    try {
      const res = await invoke("set_backfill_enabled", { enabled });
      state.settings = res.settings;
      showBfStatus(res.status);
    } catch (e) {
      showInfo(String(e), true);
    }
    await loadBfInfo();
  }

  async function runBackfillNow() {
    try {
      showBfStatus(await invoke("backfill_run_now"));
    } catch (e) { showInfo(String(e), true); }
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
    await Promise.all([loadBfInfo(), loadHistory()]);
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

  // ------------------------------------------------------------ forecast, risers, h next step
  const pl = (n, key) => t(key + (n === 1 ? ".one" : ".many"));
  const shortText = (s, n) => (s.length > n ? s.slice(0, n - 1).trimEnd() + "…" : s);
  const monthLabel = (ym, long) => {
    const [y, m] = ym.split("-").map(Number);
    return new Date(Date.UTC(y, m - 1, 1)).toLocaleDateString(loc(), { month: long ? "long" : "short", year: long ? "numeric" : undefined, timeZone: "UTC" });
  };

  async function loadForecast() { state.forecast = await safe(() => invoke("get_forecast"), state.forecast); renderForecast(); }
  async function loadRisers() { state.risers = await safe(() => invoke("get_risers", { days: state.risDays }), state.risers); renderRisers(); }
  async function loadNext() { state.next = await safe(() => invoke("get_next_steps"), state.next); renderNext(); }

  function ensureForecastSkeleton() {
    const body = $("#fcBody");
    if ($("#fcGoal", body)) return;
    body.innerHTML = `
      <div class="fc-main">
        <div class="fc-stat"><div class="fc-big" id="fcYearEnd">—</div><div class="caption secondary" id="fcYearLabel"></div></div>
        <div class="fc-stat"><div class="fc-big" id="fcRate">—</div><div class="caption secondary" data-i18n="fc.per_month"></div></div>
        <div class="fc-stat"><div class="fc-big" id="fcLast12">—</div><div class="caption secondary" data-i18n="fc.last12"></div></div>
      </div>
      <div id="fcSpark" class="spark"></div>
      <div class="fc-goal">
        <label class="caption secondary" for="fcGoal" data-i18n="fc.goal_label"></label>
        <input id="fcGoal" class="input input-num" type="number" min="1" step="1" inputmode="numeric">
        <button id="fcGoalSave" class="btn"><span data-i18n="fc.goal_save"></span></button>
        <div id="fcGoalText" class="fc-goal-text"></div>
      </div>
      <div class="caption secondary fc-note" id="fcNote"></div>`;
    window.I18N.applyStatic(body);
    $("#fcGoalSave").addEventListener("click", saveGoal);
    $("#fcGoal").addEventListener("keydown", (e) => { if (e.key === "Enter") saveGoal(); });
  }

  /** Mini chart: one bar per month (the unfinished month is lighter, unknown months get a dot). */
  function sparkChart(el, months) {
    // Drawn while the page is still hidden the width reads 0; draw once it is laid out instead.
    if (!el.clientWidth) { requestAnimationFrame(() => { if (el.clientWidth) sparkChart(el, months); }); return; }
    const w = el.clientWidth, h = 92;
    const max = Math.max(1, ...months.map((m) => m.count ?? 0));
    const m = { l: 4, r: 4, t: 14, b: 18 };
    const band = (w - m.l - m.r) / months.length;
    const bw = Math.max(4, Math.min(28, band * 0.64));
    const base = h - m.b;
    const Y = (v) => base - (v / max) * (base - m.t);
    let svg = `<svg viewBox="0 0 ${w} ${h}" width="${w}" height="${h}" role="img" aria-label="${esc(t("fc.spark_label"))}"><line class="grid-line" x1="${m.l}" x2="${w - m.r}" y1="${base}" y2="${base}"/>`;
    const pts = [];
    months.forEach((mo, i) => {
      const cx = m.l + band * (i + 0.5);
      if (mo.count === null || mo.count === undefined) {
        svg += `<circle class="spark-na" cx="${cx.toFixed(1)}" cy="${base - 2}" r="2"/>`;
      } else {
        const bh = Math.max(mo.count > 0 ? 2 : 0, base - Y(mo.count));
        svg += `<rect class="bar${mo.partial ? " bar-part" : ""}" x="${(cx - bw / 2).toFixed(1)}" y="${(base - bh).toFixed(1)}" width="${bw.toFixed(1)}" height="${bh.toFixed(1)}" rx="2" data-ym="${mo.ym}" data-c="${mo.count}" data-s="${mo.source}"/>`;
        pts.push(`${cx.toFixed(1)},${Y(mo.count).toFixed(1)}`);
      }
      if (i === 0 || i === months.length - 1 || i === Math.floor(months.length / 2)) {
        const anchor = i === 0 ? "start" : i === months.length - 1 ? "end" : "middle";
        const x = anchor === "start" ? m.l : anchor === "end" ? w - m.r : cx;
        svg += `<text x="${x.toFixed(1)}" y="${h - 4}" text-anchor="${anchor}">${esc(monthLabel(mo.ym, false))}</text>`;
      }
    });
    if (pts.length > 1) svg += `<polyline class="spark-line" points="${pts.join(" ")}"/>`;
    svg += `</svg>`;
    el.innerHTML = svg;
    $$("rect.bar", el).forEach((r) => {
      r.addEventListener("mousemove", (ev) =>
        showTip(`<div class="tt-t">${esc(monthLabel(r.dataset.ym, true))}</div><div><span class="tt-v">${nf.format(+r.dataset.c)}</span> <span class="tt-t">${esc(t("chart.series_cites"))}</span></div><div class="tt-t">${esc(t("fc.src_" + r.dataset.s))}</div>`, ev.clientX, ev.clientY));
      r.addEventListener("mouseleave", hideTip);
    });
  }

  function renderForecast() {
    if (state.page !== "panel" || $("#panelData").hidden) return;
    ensureForecastSkeleton();
    const f = state.forecast;
    if (!f) return;
    const known = f.months.filter((x) => x.count !== null);
    $("#fcYearLabel").textContent = t("fc.year_end", { year: f.year });
    $("#fcYearEnd").textContent = f.year_end === null ? "—" : "≈ " + nf.format(f.year_end);
    $("#fcRate").textContent = f.per_month === null ? "—" : nf.format(Math.round(f.per_month * 10) / 10);
    $("#fcLast12").textContent = known.length ? nf.format(known.reduce((a, x) => a + x.count, 0)) : "—";
    const spark = $("#fcSpark");
    if (known.length) sparkChart(spark, f.months);
    else spark.innerHTML = `<div class="chart-empty">${esc(t("fc.no_data"))}</div>`;
    const input = $("#fcGoal");
    if (document.activeElement !== input) input.value = f.goal ?? "";
    const g = $("#fcGoalText");
    g.className = "fc-goal-text" + (f.goal_status === "reached" ? " pos" : "");
    if (f.goal_status === "none") g.textContent = t("fc.goal_none");
    else if (f.goal_status === "reached") g.textContent = t("fc.goal_reached", { goal: nf.format(f.goal) });
    else if (f.goal_status === "month") g.textContent = t("fc.goal_eta", { goal: nf.format(f.goal), date: monthLabel(f.goal_ym, true) });
    else g.textContent = t("fc.goal_unknown");
    const parts = [];
    if (f.source !== "none") parts.push(t("fc.note_src", { src: t("fc.src_" + f.source) }));
    if (f.basis) parts.push(t("fc.basis_" + f.basis));
    parts.push(t("fc.estimate_note"));
    $("#fcNote").textContent = parts.join(" ");
  }

  async function saveGoal() {
    const raw = $("#fcGoal").value.trim();
    const n = raw === "" ? null : Math.floor(Number(raw));
    if (n !== null && !(n > 0)) { toast(t("fc.goal_err"), true); return; }
    try {
      await invoke("set_goal", { goal: n });
      await loadForecast();
    } catch (e) { toast(String(e), true); }
  }

  function renderRisers() {
    if (state.page !== "panel" || $("#panelData").hidden) return;
    $$("#risSeg button").forEach((b) => b.classList.toggle("on", Number(b.dataset.days) === state.risDays));
    const r = state.risers;
    const body = $("#risBody");
    if (!r) { body.innerHTML = ""; return; }
    const max = Math.max(1, ...r.items.map((x) => x.gain));
    let html = "";
    if (!r.items.length) {
      html = `<div class="ris-empty secondary">${esc(t(r.basis === "none" ? "ris.no_history" : "ris.empty"))}</div>`;
    } else {
      html = `<ol class="ris-list">` + r.items.map((x) => `<li>
        <div class="ris-bar" style="width:${Math.max(4, Math.round((x.gain / max) * 100))}%"></div>
        <span class="ris-title" title="${esc(x.title)}">${esc(shortText(x.title, 90))}</span>
        <span class="chip pos"><b>+${nf.format(x.gain)}</b></span></li>`).join("") + `</ol>`;
    }
    const basis = r.basis === "snapshots" ? t("ris.basis_snapshots")
      : r.basis === "backfill" ? t("ris.basis_backfill")
      : r.basis === "snapshots_partial" ? t("ris.basis_partial", { days: r.covered_days })
      : "";
    body.innerHTML = html + (basis ? `<div class="caption secondary ris-basis">${esc(basis)}</div>` : "");
  }

  /** "h=5 for: 2 articles need 3 more citations" plus the closest articles. */
  function nextBlock(label, s) {
    let line;
    if (s.picks.length) {
      line = t("hn.line", {
        label, papers: nf.format(s.picks.length), paper_word: pl(s.picks.length, "unit.article"),
        total: nf.format(s.total_needed), cite_word: pl(s.total_needed, "unit.citation"),
      });
      if (s.new_papers > 0) line += " " + t("hn.plus_new", { n: nf.format(s.new_papers), paper_word: pl(s.new_papers, "unit.article") });
    } else {
      line = t("hn.new_only", { label, n: nf.format(s.new_papers), paper_word: pl(s.new_papers, "unit.article") });
    }
    const list = s.picks.length
      ? `<ul class="hn-list">${s.picks.map((p) => `<li><span class="hn-title" title="${esc(p.title)}">${esc(shortText(p.title, 80))}</span><span class="chip"><b>${nf.format(p.citations)}</b> → +${nf.format(p.needs)}</span></li>`).join("")}</ul>`
      : "";
    return `<div class="hn-block"><div class="hn-line">${esc(line)}</div>${list}</div>`;
  }

  function renderNext() {
    if (state.page !== "panel" || $("#panelData").hidden) return;
    const n = state.next;
    const el = $("#hNext");
    if (!n) { el.innerHTML = ""; return; }
    el.innerHTML = `<div class="hn-title-row body-strong">${esc(t("hn.title"))}</div>`
      + nextBlock(`h=${n.h.target}`, n.h) + nextBlock(`i10=${n.i10.target}`, n.i10);
  }

  // ------------------------------------------------------------ citing works page
  async function loadCiting() {
    state.citing = await safe(() => invoke("get_citing_works"), state.citing);
    state.citingLoaded = true;
    state.citingGroups = groupCiting(state.citing);
    renderCiting();
  }
  async function loadDist() { state.dist = await safe(() => invoke("get_distribution"), state.dist); renderDist(); }

  const citingUrl = (c) => (c.doi ? "https://doi.org/" + encodeURI(c.doi)
    : c.source === "openalex" ? "https://openalex.org/" + c.id
    : "https://www.semanticscholar.org/paper/" + c.id);

  /** One entry per citing work, with the list of my articles it cites. */
  function groupCiting(rows) {
    const map = new Map();
    for (const r of rows || []) {
      let g = map.get(r.citing_id);
      if (!g) {
        g = { id: r.citing_id, title: r.title, date: r.date, precision: r.precision, venue: r.venue, doi: r.doi, source: r.source, countries: r.countries, authors: r.authors, articles: [] };
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
  const sourceName = (s) => (s === "openalex" ? "OpenAlex" : "Semantic Scholar");

  function filteredCiting() {
    const q = state.cw.query.trim().toLocaleLowerCase(loc());
    const art = state.cw.article;
    let rows = (state.citingGroups || []).filter((g) =>
      (!art || g.articles.some((a) => a.key === art)) &&
      (!q || `${g.title} ${g.venue} ${g.authors} ${g.articles.map((a) => a.title).join(" ")}`.toLocaleLowerCase(loc()).includes(q)));
    const dir = state.cw.dir;
    rows = rows.slice().sort((a, b) => (a.date < b.date ? -1 : a.date > b.date ? 1 : 0) * dir || a.title.localeCompare(b.title, loc()));
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

  function renderCiting() {
    if (state.page !== "citing") return;
    const has = (state.citing || []).length > 0;
    $("#citingEmpty").hidden = has;
    $("#citingData").hidden = !has;
    renderDist();
    if (!has) return;
    fillArticleFilter();
    const th = $("#citingTable th[data-csort]");
    const old = $(".sort", th); if (old) old.remove();
    th.insertAdjacentHTML("beforeend", `<span class="sort">${state.cw.dir > 0 ? "&#xE70E;" : "&#xE70D;"}</span>`);
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
          <div class="title-meta">${esc(shortText(g.authors, 110))}</div>${countries ? `<div class="chips cw-countries">${countries}</div>` : ""}</td>
        <td>${esc(g.venue || "—")}</td>
        <td class="cw-arts">${arts}</td>
        <td><span class="src-badge">${esc(sourceName(g.source))}</span></td></tr>`;
    }).join("") : `<tr><td colspan="5" class="empty-row">${esc(t("cw.none_match"))}</td></tr>`;
    $("#citingMoreRow").hidden = rows.length <= shown.length;
  }

  function hbarChart(el, items, labelOf) {
    if (!items.length) { el.innerHTML = `<div class="chart-empty">${esc(t("chart.no_data"))}</div>`; return; }
    if (!el.clientWidth) { requestAnimationFrame(() => { if (el.clientWidth) hbarChart(el, items, labelOf); }); return; }
    const w = el.clientWidth, rowH = 26;
    const labelW = Math.round(Math.min(190, w * 0.46));
    const barMax = Math.max(40, w - labelW - 44);
    const max = Math.max(...items.map((i) => i[1]));
    const h = items.length * rowH + 4;
    let svg = `<svg viewBox="0 0 ${w} ${h}" width="${w}" height="${h}" role="img">`;
    items.forEach(([key, n], i) => {
      const y = i * rowH + 2;
      const label = labelOf(key);
      svg += `<text x="${labelW - 8}" y="${y + 17}" text-anchor="end"><title>${esc(label)}</title>${esc(shortText(label, Math.max(10, Math.floor(labelW / 7))))}</text>`;
      svg += `<rect class="bar" x="${labelW}" y="${y + 4}" width="${Math.max(2, (n / max) * barMax).toFixed(1)}" height="${rowH - 10}" rx="3"/>`;
      svg += `<text class="bar-label" x="${labelW + Math.max(2, (n / max) * barMax) + 6}" y="${y + 17}">${nf.format(n)}</text>`;
    });
    el.innerHTML = svg + "</svg>";
  }

  function renderDist() {
    if (state.page !== "citing" || $("#citingData").hidden) return;
    const d = state.dist;
    if (!d) return;
    const lang = window.I18N.lang;
    hbarChart($("#distCountry"), d.countries, (c) => `${window.COUNTRIES.name(c, lang)} (${c})`);
    hbarChart($("#distVenue"), d.venues, (v) => v);
    barChart($("#distYear"), d.years);
    const notes = [];
    if (d.with_country === 0) notes.push(t("dist.country_none"));
    else if (d.without_country > 0) notes.push(t("dist.country_missing", { n: nf.format(d.without_country), total: nf.format(d.total) }));
    if (d.without_venue > 0) notes.push(t("dist.venue_missing", { n: nf.format(d.without_venue) }));
    notes.push(t("dist.distinct"));
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
  }

  function renderReportPage() {
    if (!$("#repFrom").value) {
      setReportRange("12m");
      $("#repLang").value = window.I18N.lang;
    }
    const first = state.series[0]?.t;
    state.firstDay = first ? isoDay(new Date(first)) : null;
    $("#repCreate").disabled = noProfile() || !state.overview?.snapshot;
  }

  function reportInfo(text, err) {
    const bar = $("#reportInfo");
    bar.textContent = text;
    bar.className = "infobar" + (err ? " err" : "");
    bar.hidden = !text;
  }

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
        options: { from, to, lang: $("#repLang").value, include_citing: $("#repCiting").checked, include_charts: $("#repCharts").checked },
      });
      if (path) reportInfo(t("rep.done", { path }), false);
    } catch (e) {
      reportInfo(String(e), true);
    } finally {
      btn.disabled = false;
      $("#repCreateText").textContent = t("rep.create");
    }
  }

  // ------------------------------------------------------------ API keys
  const KEYS = {
    openalex: { input: "#keyOa", status: "#keyOaStatus", name: "OpenAlex", flag: "openalex" },
    semanticscholar: { input: "#keyS2", status: "#keyS2Status", name: "Semantic Scholar", flag: "semanticscholar" },
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
      const del = which === "openalex" ? $("#keyOaDel") : $("#keyS2Del");
      del.disabled = !stored;
    }
  }

  async function saveKey(which) {
    const k = KEYS[which];
    const key = $(k.input).value.trim();
    if (!key) { setKeyMsg(which, t("keys.err_empty"), "bad"); return; }
    try {
      state.keys = await invoke("save_api_key", { which, key });
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
    state.page = name;
    if (name === "settings") loadBfInfo();
    $$(".nav-item").forEach((b) => b.classList.toggle("selected", b.dataset.page === name));
    $$(".page").forEach((p) => { p.hidden = p.id !== "page-" + name; });
    hideTip();
    if (name === "panel") { renderCharts(); renderHistory(); renderForecast(); }
    if (name === "articles") loadArticles();
    if (name === "log") loadLog();
    if (name === "citing") { loadCiting(); loadDist(); }
    if (name === "report") renderReportPage();
    if (name === "settings") { $("#infoBar").hidden = true; fillSettings(); loadKeyStatus(); }
  }

  /** Opens the Citing works page filtered to one article. */
  function showCitingFor(key) {
    state.cw.article = key;
    state.cw.query = "";
    state.cw.shown = 200;
    $("#citingSearch").value = "";
    showPage("citing");
  }

  function wire() {
    $$(".nav-item").forEach((b) => b.addEventListener("click", () => showPage(b.dataset.page)));
    $("#refreshBtn").addEventListener("click", refreshNow);
    $("#emptyRefresh").addEventListener("click", refreshNow);
    $("#setupForm").addEventListener("submit", startSetup);
    $("#setupProfile").addEventListener("input", updateSetupValidity);
    $("#setupOrcid").addEventListener("input", updateSetupValidity);
    $("#setBackfill").addEventListener("change", (e) => setBackfillEnabled(e.target.checked));
    $("#bfRunBtn").addEventListener("click", runBackfillNow);
    $("#bfClearBtn").addEventListener("click", clearBackfill);
    $$("#histGran button").forEach((b) => b.addEventListener("click", () => { state.histGran = b.dataset.gran; renderHistory(); }));
    $("#histRange").addEventListener("click", (e) => {
      const b = e.target.closest("button[data-range]");
      if (!b) return;
      state.histRange[state.histGran] = b.dataset.range;
      renderHistory();
    });
    $("#histYearSrc").addEventListener("change", (e) => { state.histYearBf = e.target.checked; renderHistory(); });
    // Preview the language on the first-run card right away; it is saved with "Start".
    $("#setupLang").addEventListener("change", (e) => applyLang(e.target.value));
    $$("#rangeSeg button").forEach((b) => b.addEventListener("click", () => { state.range = b.dataset.range; renderCharts(); }));
    $("#sinceToggle").addEventListener("change", (e) => { state.showSince = e.target.checked; renderCharts(); });
    $("#searchBox").addEventListener("input", (e) => { state.query = e.target.value; renderArticles(); });
    $$("#articleTable th[data-sort]").forEach((th) => th.addEventListener("click", () => {
      const k = th.dataset.sort;
      state.sort = state.sort.key === k ? { key: k, dir: -state.sort.dir } : { key: k, dir: k === "title" ? 1 : -1 };
      renderArticles();
    }));
    $("#articleBody").addEventListener("click", (e) => {
      const act = e.target.closest("button[data-act]");
      if (act) {
        const k = act.dataset.key;
        if (act.dataset.act === "doi") openDoiDialog(k);
        else if (act.dataset.act === "reject") rejectMatch(k);
        else if (act.dataset.act === "reset") resetOverride(k);
        else if (act.dataset.act === "allciting") showCitingFor(k);
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
      if (state.open.has(k)) state.open.delete(k); else state.open.add(k);
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
    $("#healthOpenLog").addEventListener("click", () => showPage("log"));
    $$("#risSeg button").forEach((b) => b.addEventListener("click", () => { state.risDays = Number(b.dataset.days); loadRisers(); }));
    $("#citingSearch").addEventListener("input", (e) => { state.cw.query = e.target.value; state.cw.shown = 200; renderCiting(); });
    $("#citingArticle").addEventListener("change", (e) => { state.cw.article = e.target.value; state.cw.shown = 200; renderCiting(); });
    $("#citingMore").addEventListener("click", () => { state.cw.shown += 300; renderCiting(); });
    $("#citingTable th[data-csort]").addEventListener("click", () => { state.cw.dir = -state.cw.dir; renderCiting(); });
    $("#citingBody").addEventListener("click", (e) => {
      const link = e.target.closest("a[data-url]");
      if (!link) return;
      e.preventDefault();
      safe(() => invoke("open_url", { url: link.dataset.url }));
    });
    $$("#repPresets button").forEach((b) => b.addEventListener("click", () => setReportRange(b.dataset.preset)));
    $("#repCreate").addEventListener("click", createReport);
    $("#exportXlsx").addEventListener("click", exportXlsx);
    $("#keyOaSave").addEventListener("click", () => saveKey("openalex"));
    $("#keyOaTest").addEventListener("click", () => testKey("openalex"));
    $("#keyOaDel").addEventListener("click", () => removeKey("openalex"));
    $("#keyOaRequest").addEventListener("click", requestOaKey);
    $("#keyS2Save").addEventListener("click", () => saveKey("semanticscholar"));
    $("#keyS2Test").addEventListener("click", () => testKey("semanticscholar"));
    $("#keyS2Del").addEventListener("click", () => removeKey("semanticscholar"));
    $("#page-settings").addEventListener("click", (e) => {
      const link = e.target.closest("a[data-url]");
      if (!link) return;
      e.preventDefault();
      safe(() => invoke("open_url", { url: link.dataset.url }));
    });
    $$("[data-export]").forEach((b) => b.addEventListener("click", () => exportCsv(b.dataset.export)));
    let rz;
    const relayout = () => { clearTimeout(rz); rz = setTimeout(() => { renderCharts(); renderForecast(); renderDist(); state.open.forEach(drawDetail); }, 120); };
    window.addEventListener("resize", relayout);
    // The content area can change width without a window resize (nav width, scrollbar): redraw the charts.
    let lastW = 0;
    new ResizeObserver((entries) => {
      const w = Math.round(entries[0].contentRect.width);
      if (w !== lastW) { lastW = w; relayout(); }
    }).observe($(".content"));
    $("#articleBody").addEventListener("mouseover", (e) => { if (!e.target.closest("tr.detail")) hideTip(); });
  }

  async function init() {
    // Provisional language until the saved one is known (avoids empty labels).
    window.I18N.setLang(navigator.language);
    wire();
    if (await safe(() => invoke("backdrop_mica"), false)) document.documentElement.classList.add("mica");
    state.settings = await safe(() => invoke("get_settings"), null);
    applyLang(state.settings?.language || navigator.language);
    await listen("status", (e) => {
      const was = state.status?.state;
      state.status = e.payload;
      renderHeader();
      if (was && was !== "idle" && e.payload.state === "idle") loadHealth();
    });
    await listen("update-available", (e) => { state.update = e.payload; renderUpdate(); });
    await listen("update-progress", (e) => {
      const p = e.payload;
      if (p.done) { $("#updateText").textContent = t("update.installing"); $("#updateBarFill").style.width = "100%"; return; }
      const pct = p.total ? Math.min(100, Math.round((p.downloaded / p.total) * 100)) : null;
      $("#updateText").textContent = t("update.downloading", { percent: pct === null ? nf.format(Math.round(p.downloaded / 1024)) + " KB" : pct + "%" });
      if (pct !== null) $("#updateBarFill").style.width = pct + "%";
    });
    await listen("data-updated", refreshAll);
    await listen("backfill-progress", (e) => {
      const wasRunning = !!state.bfProgress?.running;
      state.bfProgress = e.payload;
      renderHeader();
      renderBackfill();
      if (wasRunning && !e.payload.running) loadBfInfo();
    });
    await listen("backfill-updated", async () => {
      state.bf.clear();
      await Promise.all([loadBfInfo(), loadHistory(), loadCiting(), loadDist(), loadForecast(), loadRisers(), loadNext()]);
      state.open.forEach(drawDetail);
    });
    await listen("api-key-rejected", (e) => {
      const k = KEYS[e.payload];
      toast(t("keys.rejected", { name: k ? k.name : String(e.payload) }), true);
    });
    state.version = await safe(() => invoke("app_version"), "");
    renderAbout();
    state.update = await safe(() => invoke("get_pending_update"), null);
    renderUpdate();
    await loadStatus();
    await refreshAll();
    $("#page-panel").hidden = false;
    setInterval(renderHeader, 30000);
  }
  init();
})();
