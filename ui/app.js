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
    mode: "total",
    gran: null,
    range: { total: "all", day: "90", month: "12" },
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
    hist: null,
    bf: new Map(),
    health: null,
    update: null,
    updating: false,
    version: "",
    doiKey: null,
    doiOnly: false,
    doiBarHidden: false,
    forecast: null,
    risers: null,
    risDays: 30,
    next: null,
    citing: [],
    citingGroups: [],
    citingLoaded: false,
    cw: { query: "", article: "", dir: -1, shown: 200 },
    dist: null,
    recent: [],
    scan: null,
    scanOpen: false,
    scanUserStart: false,
    keys: null,
    keyMsg: { openalex: null },
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
  async function loadSeries() { state.series = await safe(() => invoke("get_series"), []); renderMain(); }
  async function loadArticles() { state.articles = await safe(() => invoke("get_articles"), []); renderArticles(); renderRecent(); renderScan(); }
  async function loadLog() { state.log = await safe(() => invoke("get_log"), []); renderLog(); }
  async function loadBfInfo() {
    state.bfInfo = await safe(() => invoke("get_backfill_info"), state.bfInfo);
    if (state.bfInfo) state.bfProgress = state.bfInfo.progress;
    renderHeader();
    renderBackfill();
  }
  async function loadHealth() { state.health = await safe(() => invoke("get_health"), state.health); renderHealth(); }
  async function loadHistory() { state.hist = await safe(() => invoke("get_history"), state.hist); renderMain(); }
  async function refreshAll() {
    state.history.clear();
    state.bf.clear();
    state.changes.clear();
    await Promise.all([
      loadOverview(), loadSeries(), loadArticles(), loadLog(), loadBfInfo(), loadHistory(), loadHealth(),
      loadForecast(), loadRisers(), loadNext(), loadCiting(), loadDist(), loadRecent(),
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
    renderRecent();
    renderScan();
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
    renderMain();
    renderRecent();
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

  /** series: [{pts:[{t,v}], cls, name}] ; the first series gets the area fill and the end marker.
   *  cls: "main" (solid accent), "bf" (dashed, lighter: estimated history), "alt" (dashed grey). */
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
        : span > 400 * 86400e3
          ? d.toLocaleDateString(loc(), { month: "short", year: "2-digit" })
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
      if (s.pts.length > 1) svg += `<path class="${s.cls === "alt" ? "line-alt" : s.cls === "bf" ? "line-bf" : "line-main"}" d="${d}"/>`;
    });
    const first = series[0].pts;
    const last = first[first.length - 1];
    svg += `<g id="hov" visibility="hidden"><line class="hover-line" y1="${m.t}" y2="${h - m.b}"/><circle class="dot-main" r="5"/></g>`;
    if (last) svg += `<circle class="dot-main" cx="${X(last.t)}" cy="${Y(last.v)}" r="${first.length > 1 ? 3.5 : 5}"/>`;
    svg += `<rect class="hit" x="${m.l}" y="${m.t}" width="${w - m.l - m.r}" height="${h - m.t - m.b}" fill="transparent"/></svg>`;
    el.innerHTML = svg;

    const flat = series.flatMap((s) => s.pts.map((p) => ({ p, s })));
    const hit = $("rect.hit", el), hov = $("#hov", el);
    hit.addEventListener("mousemove", (ev) => {
      const px = ev.clientX - el.getBoundingClientRect().left;
      let best = flat[0];
      for (const f of flat) if (Math.abs(X(f.p.t) - px) < Math.abs(X(best.p.t) - px)) best = f;
      hov.setAttribute("visibility", "visible");
      $("line", hov).setAttribute("x1", X(best.p.t)); $("line", hov).setAttribute("x2", X(best.p.t));
      const c = $("circle", hov); c.setAttribute("cx", X(best.p.t)); c.setAttribute("cy", Y(best.p.v));
      let html = `<div class="tt-t">${esc(fmtDateTime(new Date(best.p.t).toISOString()))}</div>`;
      html += `<div><span class="tt-v">${nf.format(best.p.v)}</span> <span class="tt-t">${esc(best.s.name)}</span></div>`;
      showTip(html, ev.clientX, ev.clientY);
    });
    hit.addEventListener("mouseleave", () => { hov.setAttribute("visibility", "hidden"); hideTip(); });
  }

  // ------------------------------------------------------------ period bars
  const DAY_MS = 86400e3;
  const pad2 = (n) => String(n).padStart(2, "0");
  const ymd = (ms) => { const d = new Date(ms); return `${d.getUTCFullYear()}-${pad2(d.getUTCMonth() + 1)}-${pad2(d.getUTCDate())}`; };
  const keyMs = (key) => { const [y, m = 1, d = 1] = key.split("-").map(Number); return Date.UTC(y, m - 1, d); };

  /** Continuous periods (zeros filled) from the first data point to today. */
  function buildPeriods(points, gran) {
    if (!points.length) return [];
    const map = new Map(points);
    const first = keyMs(points[0][0]);
    const last = keyMs(points[points.length - 1][0]);
    const now = new Date();
    const end = Math.max(last, Date.UTC(now.getFullYear(), now.getMonth(), now.getDate()));
    const out = [];
    const push = (key, tms) => { out.push({ key, t: tms, v: map.get(key) || 0 }); };
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

  /** periods: [{key,t,v}] -> one bar per period (new citations). One y-axis only. */
  function barPeriods(el, periods, o = {}) {
    const w = el.clientWidth, h = el.clientHeight;
    if (!periods.length || w < 60) { el.innerHTML = `<div class="chart-empty">${esc(o.emptyText || t("chart.no_data"))}</div>`; return; }
    const m = { l: o.small ? 34 : 44, r: 12, t: 12, b: 26 };
    const ticks = niceTicks(0, Math.max(1, ...periods.map((p) => p.v)), o.small ? 3 : 4);
    const y1 = ticks[ticks.length - 1];
    const pw = w - m.l - m.r;
    const band = pw / periods.length;
    const X = (i) => m.l + band * (i + 0.5);
    const Y = (v) => m.t + (1 - v / y1) * (h - m.t - m.b);
    const base = Y(0);

    let svg = `<svg viewBox="0 0 ${w} ${h}" width="${w}" height="${h}" role="img">`;
    for (const tv of ticks) {
      svg += `<line class="grid-line" x1="${m.l}" x2="${w - m.r}" y1="${Y(tv)}" y2="${Y(tv)}"/>`;
      svg += `<text x="${m.l - 8}" y="${Y(tv) + 4}" text-anchor="end">${nf.format(tv)}</text>`;
    }
    const bw = Math.max(1, Math.min(32, band * 0.72));
    periods.forEach((p, i) => {
      if (!p.v) return;
      const bh = Math.max(1.5, base - Y(p.v));
      svg += `<rect class="bar" x="${(X(i) - bw / 2).toFixed(2)}" y="${(base - bh).toFixed(2)}" width="${bw.toFixed(2)}" height="${bh.toFixed(2)}" rx="${bw > 6 ? 2 : 0}"/>`;
    });
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
      const html = `<div class="tt-t">${esc(periodLabel(p, o.gran, true, false))}</div>`
        + `<div><span class="tt-v">${nf.format(p.v)}</span> <span class="tt-t">${esc(t("main.tt_new"))}</span></div>`;
      showTip(html, ev.clientX, ev.clientY);
    });
    hit.addEventListener("mouseleave", () => { hov.setAttribute("visibility", "hidden"); hideTip(); });
  }

  // ------------------------------------------------------------ main chart: citation history
  /** Snapshot totals inside the chosen range (plus the point just before it, so the line starts at the edge). */
  function totalPoints(rangeKey) {
    const pts = state.series.map((p) => ({ t: new Date(p.t).getTime(), v: p.cit_all }));
    if (rangeKey === "all") return pts;
    const cutoff = Date.now() - Number(rangeKey) * DAY_MS;
    const inRange = pts.filter((p) => p.t >= cutoff);
    const before = pts.filter((p) => p.t < cutoff).pop();
    return before ? [before, ...inRange] : inRange;
  }

  /** Cumulative citing works of the historical lookup, one point at the end of each month, before the first snapshot. */
  function backfillTotalPoints(rangeKey) {
    const h = state.hist;
    const firstSnap = state.series[0] ? new Date(state.series[0].t).getTime() : null;
    if (!h || !h.month.points.length || firstSnap === null) return [];
    let cum = 0;
    let pts = h.month.points.map(([key, n]) => {
      cum += n;
      const [y, m] = key.split("-").map(Number);
      return { t: Date.UTC(y, m, 0, 23, 59), v: cum };
    }).filter((p) => p.t < firstSnap);
    if (rangeKey !== "all") {
      const cutoff = Date.now() - Number(rangeKey) * DAY_MS;
      pts = pts.filter((p) => p.t >= cutoff);
    }
    return pts;
  }

  const hasBackfill = () => !!state.hist && (state.hist.month.points.length > 0 || state.hist.day.points.length > 0);
  const PERIOD_RANGES = {
    day: [["30", "main.r_30d"], ["90", "main.r_90d"], ["365", "main.r_12m"], ["all", "main.r_all"]],
    month: [["12", "main.r_12m"], ["36", "main.r_3y"], ["all", "main.r_all"]],
    year: [],
  };
  const TOTAL_RANGES = [["30", "main.r_30d"], ["365", "main.r_12m"], ["all", "main.r_all"]];

  function renderMain() {
    if (state.page !== "panel" || $("#panelData").hidden) return;
    const bf = hasBackfill();
    if (state.gran === null) state.gran = bf && state.hist.month.points.length ? "month" : "year";
    if (!bf && state.gran !== "year") state.gran = "year";
    const mode = state.mode;
    const gran = state.gran;
    $$("#modeSeg button").forEach((b) => b.classList.toggle("on", b.dataset.mode === mode));
    $("#granSeg").hidden = mode !== "period";
    $$("#granSeg button").forEach((b) => {
      b.classList.toggle("on", b.dataset.gran === gran);
      const off = !bf && b.dataset.gran !== "year";
      b.disabled = off;
      b.title = off ? t("main.gran_disabled") : "";
    });
    const ranges = mode === "total" ? TOTAL_RANGES : PERIOD_RANGES[gran];
    const rkey = mode === "total" ? "total" : gran;
    const range = state.range[rkey];
    const seg = $("#rangeSeg");
    seg.hidden = !ranges.length;
    seg.innerHTML = ranges.map(([v, k]) => `<button data-range="${v}" class="${v === range ? "on" : ""}">${esc(t(k))}</button>`).join("");

    const chart = $("#mainChart"), legend = $("#mainLegend"), cap = $("#mainCaption");
    if (mode === "total") {
      const sch = totalPoints(range);
      const bfp = backfillTotalPoints(range);
      const series = [{ pts: sch, cls: "main", name: t("main.s_scholar") }];
      if (bfp.length) series.push({ pts: bfp, cls: "bf", name: t("main.s_backfill") });
      if (!sch.length) chart.innerHTML = `<div class="chart-empty">${esc(t("chart.no_data_range"))}</div>`;
      else lineChart(chart, series);
      legend.hidden = false;
      legend.innerHTML = `<span class="lg"><i class="sw sw-line-main"></i><span>${esc(t("main.s_scholar"))}</span></span>`
        + (bfp.length ? `<span class="lg"><i class="sw sw-line-bf"></i><span>${esc(t("main.s_backfill"))}</span></span>` : "");
      cap.textContent = t(bfp.length ? "main.cap_total_bf" : "main.cap_total");
      return;
    }
    legend.hidden = true;
    const scholarYear = gran === "year";
    const series = state.hist ? state.hist[gran] : null;
    const points = scholarYear ? (state.overview?.yearly || []).map(([y, c]) => [String(y), c]) : (series ? series.points : []);
    let periods = buildPeriods(points, gran);
    if (range !== "all" && ranges.length) periods = periods.slice(-Number(range));
    const running = !!state.bfProgress?.running;
    barPeriods(chart, periods, { gran, emptyText: running ? t("main.empty_running") : t("main.empty") });
    let text = t(scholarYear ? "main.cap_scholar_year" : "main.cap_backfill");
    if (!scholarYear && series && series.excluded > 0) text += " " + t("main.note_excluded", { n: nf.format(series.excluded) });
    cap.textContent = text;
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
          <div class="title-meta"><span class="venue">${esc(a.venue)}</span>${doi}</div></div></div></td>
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
      const pts = hist.map(([ts, v]) => ({ t: new Date(ts).getTime(), v }));
      lineChart($(".d-chart", row), [{ pts, cls: "main", name: t("chart.series_cites") }], { small: true });
    } else {
      barPeriods($(".d-chart", row), buildPeriods(monthly, "month").slice(-120), { gran: "month", small: true, emptyText: t("art.bf_empty") });
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
    state.doiOnly = true;
    state.doiBarHidden = false;
    showPage("articles");
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

  // ------------------------------------------------------------ recent citations
  async function loadRecent() { state.recent = await safe(() => invoke("get_recent_citations", { limit: 10 }), state.recent); renderRecent(); }

  /** "21 Kas" (the year is added when it is not the current one). */
  function fmtRecentDate(iso, precision) {
    if (precision === "month") return fmtCitingDate(iso, "month");
    const d = precision === "day" ? new Date(Date.UTC(...iso.split("-").map((x, i) => (i === 1 ? Number(x) - 1 : Number(x))))) : new Date(iso);
    if (isNaN(d)) return "—";
    const opt = precision === "day" ? { timeZone: "UTC" } : {};
    const sameYear = d.getFullYear() === new Date().getFullYear();
    return d.toLocaleDateString(loc(), { day: "numeric", month: "short", ...(sameYear ? {} : { year: "numeric" }), ...opt });
  }

  function recentRow(r) {
    const date = `<span class="rc-date">${esc(fmtRecentDate(r.date, r.kind === "citing" ? r.precision : ""))}</span>`;
    if (r.kind === "citing") {
      const url = r.doi ? doiUrl(r.doi) : r.url || "";
      const title = url
        ? `<a href="#" class="title-link rc-title" data-url="${esc(url)}">${esc(r.citing_title || "—")}</a>`
        : `<span class="rc-title body-strong">${esc(r.citing_title || "—")}</span>`;
      return `<li class="rc-row">${date}<div class="rc-main">${title}
        <div class="rc-sub secondary">${r.venue ? esc(r.venue) + " " : ""}<span class="rc-arrow">→</span> <span title="${esc(r.article_title)}">${esc(shortText(r.article_title, 70))}</span></div></div>
        <div class="rc-side"><span class="src-badge">OpenAlex</span></div></li>`;
    }
    const down = r.kind === "scholar_down";
    const link = r.cites_ids
      ? `<button type="button" class="btn btn-link" data-act="scholar" data-cites="${esc(r.cites_ids)}" title="${esc(t("scholar.tip"))}"><span>${esc(t("scholar.view"))}</span></button>` : "";
    return `<li class="rc-row ${down ? "rc-down" : ""}">${date}<div class="rc-main">
        <div class="rc-line"><span class="rc-title body-strong" title="${esc(r.article_title)}">${esc(r.article_title)}</span>${chip("", r.delta)}</div>
        ${down ? `<div class="rc-sub neg-text">${esc(t("recent.decreased"))} · ${nf.format(r.old)} → ${nf.format(r.new)}</div>` : `<div class="rc-sub secondary">${nf.format(r.old)} → ${nf.format(r.new)}</div>`}</div>
        <div class="rc-side"><span class="src-badge">Google Scholar</span>${link}</div></li>`;
  }

  function renderRecent() {
    if (state.page !== "panel" || $("#panelData").hidden) return;
    const el = $("#recentList");
    el.innerHTML = state.recent.length
      ? `<ul class="rc-list">${state.recent.map(recentRow).join("")}</ul>`
      : `<div class="ris-empty secondary">${esc(t("recent.empty"))}</div>`;
    const miss = missingDoi().length;
    const hint = $("#recentDoiHint");
    hint.hidden = miss === 0;
    if (miss) hint.innerHTML = `${esc(t("recent.doi_hint", { n: nf.format(miss) }))} <a href="#" class="ext-link" data-act="missing-doi">${esc(t("recent.doi_link"))}</a>`;
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
      const head = sc.end === "failed" ? t("scan.sum_failed", { message: sc.message }) : t(sc.end === "cancelled" ? "scan.sum_cancelled" : "scan.sum_head");
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
  const RUN_ST = { ok: "bf.st_ok", partial: "bf.st_partial", error: "bf.st_error", interrupted: "bf.st_interrupted", running: "bf.st_running", cancelled: "bf.st_cancelled" };

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
        ${chip("", x.gain)}</li>`).join("") + `</ol>`;
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

  const citingUrl = (c) => (c.doi ? "https://doi.org/" + encodeURI(c.doi) : "https://openalex.org/" + c.id);

  /** One entry per citing work, with the list of my articles it cites. */
  function groupCiting(rows) {
    const map = new Map();
    for (const r of rows || []) {
      let g = map.get(r.citing_id);
      if (!g) {
        g = { id: r.citing_id, title: r.title, date: r.date, precision: r.precision, venue: r.venue, doi: r.doi, countries: r.countries, authors: r.authors, articles: [] };
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
        <td class="cw-arts">${arts}</td></tr>`;
    }).join("") : `<tr><td colspan="4" class="empty-row">${esc(t("cw.none_match"))}</td></tr>`;
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
    if (name === "panel") { renderMain(); renderRecent(); renderForecast(); renderRisers(); renderNext(); }
    if (name === "articles") loadArticles();
    if (name !== "articles") $("#doiBar").hidden = true;
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
    $$("#modeSeg button").forEach((b) => b.addEventListener("click", () => { state.mode = b.dataset.mode; renderMain(); }));
    $$("#granSeg button").forEach((b) => b.addEventListener("click", () => { if (!b.disabled) { state.gran = b.dataset.gran; renderMain(); } }));
    $("#rangeSeg").addEventListener("click", (e) => {
      const b = e.target.closest("button[data-range]");
      if (!b) return;
      state.range[state.mode === "total" ? "total" : state.gran] = b.dataset.range;
      renderMain();
    });
    $("#recentAll").addEventListener("click", () => showPage("citing"));
    $("#recentCard").addEventListener("click", (e) => panelActions(e));
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
    $("#uninstallBtn").addEventListener("click", () => safe(() => invoke("open_url", { url: "ms-settings:appsfeatures" })));
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
    $("#page-settings").addEventListener("click", (e) => {
      const link = e.target.closest("a[data-url]");
      if (!link) return;
      e.preventDefault();
      safe(() => invoke("open_url", { url: link.dataset.url }));
    });
    $$("[data-export]").forEach((b) => b.addEventListener("click", () => exportCsv(b.dataset.export)));
    let rz;
    const relayout = () => { clearTimeout(rz); rz = setTimeout(() => { renderMain(); renderForecast(); renderDist(); state.open.forEach(drawDetail); }, 120); };
    window.addEventListener("resize", relayout);
    // The content area can change width without a window resize (nav width, scrollbar): redraw the charts.
    let lastW = 0;
    new ResizeObserver((entries) => {
      const w = Math.round(entries[0].contentRect.width);
      if (w !== lastW) { lastW = w; relayout(); }
    }).observe($(".content"));
    $("#articleBody").addEventListener("mouseover", (e) => { if (!e.target.closest("tr.detail")) hideTip(); });
  }

  /** Clicks on the "view on Scholar" buttons and the missing-DOI links inside the panel cards and the scan sheet. */
  function panelActions(e) {
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
      scanLogEvent(e.payload);
      if (wasRunning && !e.payload.running) loadBfInfo();
    });
    await listen("backfill-updated", async () => {
      state.bf.clear();
      await Promise.all([loadBfInfo(), loadHistory(), loadCiting(), loadDist(), loadForecast(), loadRisers(), loadNext(), loadRecent(), loadArticles()]);
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
    await hydrateScan();
    await loadStatus();
    await refreshAll();
    $("#page-panel").hidden = false;
    setInterval(renderHeader, 30000);
    setInterval(renderScanTime, 1000);
  }
  init();
})();
