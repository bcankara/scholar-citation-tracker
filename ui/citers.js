"use strict";
/* Analysis > "Citing works" tab: one card per category (countries, institutions, venues, fields, types, who, influential).
   The data comes from the get_citers_* commands; the period selector of the page, the article filter and the
   "exclude self-citations" switch apply to the whole tab. No chart is clickable; the only navigation is the small
   "view in list" link of each card. app.js hands in what it owns through `Citers.init(bridge)`. */
(() => {
  const L = window.CitersLib;
  const I = window.I18N;
  const { esc, trunc, pct, sum } = L;
  const t = (k, p) => I.t(k, p);
  const $ = (s, r = document) => r.querySelector(s);
  const $$ = (s, r = document) => Array.from(r.querySelectorAll(s));
  const nfmt = (n) => new Intl.NumberFormat(I.locale).format(n);

  const SECTIONS = ["countries", "institutions", "venues", "fields", "types", "who", "influential"];
  const LS_KEY = "sct.citers.exclude_self";
  const readExclude = () => { try { return localStorage.getItem(LS_KEY) === "1"; } catch { return false; } };
  const saveExclude = (v) => { try { localStorage.setItem(LS_KEY, v ? "1" : "0"); } catch { /* private window */ } };

  let B = null; // bridge from app.js
  const S = { article: "", exclude: readExclude(), data: null, key: "", seq: 0, titles: new Map(), disposers: [], active: false, error: "" };

  const reducedMotion = () => matchMedia("(prefers-reduced-motion: reduce)").matches;

  function filter() {
    const r = B.range();
    return { from: r.from, to: r.to, article: S.article, excludeSelf: S.exclude };
  }
  const NAMES = ["get_citers_meta", "get_citers_countries", "get_citers_institutions", "get_citers_venues", "get_citers_fields", "get_citers_types", "get_citers_who", "get_citers_influential"];

  async function load(f, key) {
    const seq = ++S.seq;
    S.key = key;
    S.error = "";
    let res;
    try {
      res = await Promise.all(NAMES.map((n) => B.invoke(n, f)));
    } catch (e) {
      if (seq !== S.seq) return;
      S.data = null;
      S.error = String(e);
      draw();
      return;
    }
    if (seq !== S.seq) return;
    const [meta, countries, institutions, venues, fields, types, who, influential] = res;
    for (const a of meta.articles) S.titles.set(a.key, a.title);
    S.data = { meta, countries, institutions, venues, fields, types, who, influential };
    draw();
  }

  /** Called whenever the Analysis page renders this tab (and on language / data changes). */
  function render() {
    if (!B) return;
    S.active = true;
    $("#ciExclude").checked = S.exclude;
    const f = filter();
    const key = JSON.stringify(f);
    if (S.key === key && (S.data || S.error)) { draw(); return; }
    if (S.key !== key) $("#ciCount").textContent = "";
    load(f, key);
  }
  function invalidate() { S.key = ""; }
  function leave() { S.active = false; clearCharts(); }

  function clearCharts() {
    for (const d of S.disposers.splice(0)) { try { d(); } catch { /* gone */ } }
    window.Charts.disposeIn($("#ciSections"));
  }

  // ------------------------------------------------------------ pieces
  const section = (id, answer, body, cap) => `<section class="card ci-card" id="ci-${id}" data-sec="${id}">
    <div class="ci-head"><h2 class="ci-title">${esc(t("ci.h_" + id))}</h2><button type="button" class="ci-view" data-view="${id}">${esc(t("ci.view_list"))}</button></div>
    ${answer ? `<p class="ci-answer">${answer}</p>` : ""}${body}${cap ? `<p class="ci-cap">${esc(cap)}</p>` : ""}</section>`;
  const needsBody = (fieldKey) => `<div class="ci-needs"><p>${esc(t("ci.needs_data", { field: t(fieldKey) }))}</p><button type="button" class="btn ci-refresh">${esc(t("ci.refresh"))}</button></div>`;
  const slotNeeds = (fieldKey) => `<div class="ci-slot-msg"><span>${esc(t("ci.slot_needs_data", { field: t(fieldKey) }))}</span><button type="button" class="btn ci-refresh">${esc(t("ci.refresh"))}</button></div>`;
  const slotMsg = (text) => `<div class="ci-slot-msg"><span>${esc(text)}</span></div>`;
  const col = (title, inner, cls = "") => `<div class="ci-col ${cls}"><h3 class="ci-sub">${esc(title)}</h3>${inner}</div>`;
  const nameOf = (iso) => L.names.countryName(iso);

  function narrow() { return $("#ciSections").clientWidth < 600; }
  const labelW = () => (($("#ciSections").clientWidth || 800) < 1000 ? 140 : 160);

  // ------------------------------------------------------------ sections
  function buildCountries(d, jobs) {
    const rep = d.countries;
    if (rep.cov.needs_data) return section("countries", "", needsBody("ci.w_country"), L.caption(rep.cov.n, [{ what: "ci.w_country", missing: rep.cov.missing }]));
    if (!rep.items.length) return section("countries", "", slotMsg(t("ci.no_data")), L.caption(rep.cov.n, [{ what: "ci.w_country", missing: rep.cov.missing }]));
    const top = rep.items.slice(0, 10).map((x) => ({ name: nameOf(x.key), n: x.n }));
    const first = rep.new_items.slice(0, 12).map((x) => `<span class="chip ci-chip-new">${esc(nameOf(x.key))}</span>`).join("")
      + (rep.new_items.length > 12 ? `<span class="chip">${esc(t("ci.more_n", { n: rep.new_items.length - 12 }))}</span>` : "");
    const body = `<div class="ci-row ci-r32" id="ciGeoRow">
        ${col(t("ci.t_world"), `<div class="ci-ch ci-map" id="ciMap"></div>`, "ci-col-map")}
        ${col(t("ci.t_top_countries"), `<div class="ci-ch ci-h340" id="ciTopC"></div>`)}
      </div>
      ${rep.years.length ? col(t("ci.t_years_countries"), `<div class="ci-ch ci-h200" id="ciYearsC"></div>`, "ci-years") : ""}
      ${rep.new_items.length ? `<div class="ci-first"><span class="ci-first-l">${esc(t("ci.first_this_year"))}</span>${first}</div>` : ""}`;
    jobs.push(() => {
      const c = window.Charts.ctx({ labelWidth: labelW() });
      window.Charts.mount($("#ciTopC"), L.hbars(top, c, { labelWidth: labelW() }), { emptyText: t("ci.no_data") });
      if (rep.years.length) window.Charts.mount($("#ciYearsC"), L.yearsBars(rep.years, c));
      const mapEl = $("#ciMap");
      L.registerWorld().then(() => {
        if (!mapEl.isConnected) return;
        window.Charts.mount(mapEl, L.worldMap(rep.items.map((x) => ({ iso: x.key, n: x.n })), window.Charts.ctx()));
      }).catch(() => {
        if (!mapEl.isConnected) return;
        // the message stays in the map slot; the top 10 chart takes the full row
        mapEl.className = "ci-info ci-map-fail";
        mapEl.textContent = t("ci.map_fail");
        $("#ciGeoRow").classList.add("ci-r-full");
        const sub = mapEl.closest(".ci-col").querySelector(".ci-sub");
        if (sub) sub.hidden = true;
      });
    });
    return section("countries", L.sentences.countries(rep), body, L.caption(rep.cov.n, [{ what: "ci.w_country", missing: rep.cov.missing }]));
  }

  function buildInstitutions(d, jobs) {
    const rep = d.institutions;
    if (rep.cov.needs_data) return section("institutions", "", needsBody("ci.w_institution"), L.caption(rep.cov.n, [{ what: "ci.w_institution", missing: rep.cov.missing }]));
    if (!rep.items.length) return section("institutions", "", slotMsg(t("ci.no_data")), L.caption(rep.cov.n, [{ what: "ci.w_institution", missing: rep.cov.missing }]));
    const top = rep.items.slice(0, 10).map((x) => ({ name: x.name, n: x.n, isNew: x.is_new, tip: x.country ? nameOf(x.country) : "" }));
    const rows = rep.new_items.slice(0, 8).map((x) => `<tr><td title="${esc(x.name)}">${esc(trunc(x.name, 40))}</td><td>${esc(x.country ? nameOf(x.country) : "—")}</td><td class="num">${nfmt(x.n)}</td></tr>`).join("");
    const more = rep.new_items.length > 8 ? `<div class="ci-more">${esc(t("ci.inst_more", { n: rep.new_items.length - 8 }))}</div>` : "";
    const hasNew = rep.new_items.length > 0;
    const partial = d.meta.unenriched_rows > 0 ? `<p class="ci-info ci-partial">${esc(t("ci.partial_inst"))}</p>` : "";
    const body = `${partial}<div class="ci-row ${hasNew ? "ci-r32" : "ci-r-full"}">
        ${col(t("ci.t_top_inst"), `<div class="ci-ch ci-h320" id="ciTopI"></div>`)}
        ${hasNew ? col(t("ci.t_new_inst"), `<table class="ci-table ci-t-inst"><thead><tr><th>${esc(t("ci.col_inst"))}</th><th>${esc(t("ci.col_country"))}</th><th class="num">${esc(t("ci.col_n"))}</th></tr></thead><tbody>${rows}</tbody></table>${more}`) : ""}
      </div>`;
    jobs.push(() => window.Charts.mount($("#ciTopI"), L.hbars(top, window.Charts.ctx(), { labelWidth: labelW() }), { emptyText: t("ci.no_data") }));
    return section("institutions", L.sentences.institutions(rep), body, L.caption(rep.cov.n, [{ what: "ci.w_institution", missing: rep.cov.missing }]));
  }

  function buildVenues(d, jobs) {
    const v = d.venues;
    const venues = v.venues.slice(0, 8).map((x) => ({ name: x.name, n: x.n }));
    const pubs = v.publishers.slice(0, 5);
    const left = venues.length ? `<div class="ci-ch ci-h300" id="ciVenues"></div>` : slotMsg(t("ci.no_data"));
    const right = v.cov_publisher.needs_data ? slotNeeds("ci.w_publisher") : pubs.length ? `<div class="ci-ch ci-h300" id="ciPubs"></div>` : slotMsg(t("ci.no_data"));
    const body = `<div class="ci-row ci-r11">${col(t("ci.t_top_venues"), left)}${col(t("ci.t_publishers"), right)}</div>`;
    jobs.push(() => {
      const c = window.Charts.ctx();
      if (venues.length) window.Charts.mount($("#ciVenues"), L.hbars(venues, c, { labelWidth: labelW() }));
      if (pubs.length && !v.cov_publisher.needs_data) window.Charts.mount($("#ciPubs"), L.publishers(pubs, c, { labelWidth: labelW() }));
    });
    return section("venues", L.sentences.venues(v), body, L.caption(v.cov_venue.n, [{ what: "ci.w_venue", missing: v.cov_venue.missing }, { what: "ci.w_publisher", missing: v.cov_publisher.missing }]));
  }

  function buildFields(d, jobs) {
    const f = d.fields;
    if (f.cov.needs_data) return section("fields", "", needsBody("ci.w_topic"), L.caption(f.cov.n, [{ what: "ci.w_topic", missing: f.cov.missing }]));
    if (!f.domains.length) return section("fields", "", slotMsg(t("ci.no_data")), L.caption(f.cov.n, [{ what: "ci.w_topic", missing: f.cov.missing }]));
    const single = !!S.article;
    const showMatrix = !single && f.matrix_rows.length > 0;
    const body = `<div class="ci-row ${showMatrix ? "ci-r11" : "ci-r-full"}">
        ${col(t("ci.t_field_dist"), `<div class="ci-ch ci-h320 ci-tm" id="ciTreemap"></div>`)}
        ${showMatrix ? col(t("ci.t_matrix"), `<div class="ci-ch ci-h320" id="ciMatrix"></div>`) : ""}
      </div>`;
    jobs.push(() => {
      const c = window.Charts.ctx();
      S.disposers.push(L.mountTreemap($("#ciTreemap"), f.domains, c, { ctx: () => window.Charts.ctx() }));
      if (showMatrix) {
        const width = Math.max(...f.matrix_rows.map((r) => r.cells.length));
        const cols = f.matrix_columns.map((x) => L.names.fieldName(x));
        while (cols.length < width - 1) cols.push("");
        cols.push(t("ci.other"));
        const rows = f.matrix_rows.map((r) => ({ title: r.title, cells: r.cells.concat(Array(Math.max(0, width - r.cells.length)).fill(0)) }));
        window.Charts.mount($("#ciMatrix"), L.matrix(rows, cols.slice(0, width), window.Charts.ctx({ labelWidth: labelW() })), { emptyText: t("ci.no_data") });
      }
    });
    return section("fields", L.sentences.fields(f), body, L.caption(f.cov.n, [{ what: "ci.w_topic", missing: f.cov.missing }]));
  }

  function buildTypes(d) {
    const ty = d.types;
    const cap = L.caption(ty.cov_type.n, [{ what: "ci.w_type", missing: ty.cov_type.missing }, { what: "ci.w_oa", missing: ty.cov_oa.missing }, { what: "ci.w_language", missing: ty.cov_language.missing }]);
    if (ty.cov_type.needs_data && ty.cov_oa.needs_data && ty.cov_language.needs_data) return section("types", "", needsBody("ci.w_type"), cap);
    const bars = L.typeBars(ty, window.Charts.ctx());
    const body = `<div class="ci-sbars">${bars.map((b) => `<div class="ci-sbar"><div class="ci-sbar-l">${esc(b.title)}</div><div class="ci-sbar-b">${b.n > 0 ? L.stackBar(b.parts, { big: true }) : `<div class="ci-slot-msg">${esc(t("ci.no_data"))}</div>`}</div></div>`).join("")}</div>`;
    return section("types", L.sentences.types(ty), body, cap);
  }

  function buildWho(d) {
    const w = d.who;
    const s = w.split;
    const p = L.palette(window.Charts.ctx());
    const parts = [
      { key: "self", label: t("ci.f_self"), n: s.self_n, color: p.orange },
      { key: "co", label: t("ci.f_co"), n: s.coauthor, color: p.purple },
      { key: "ext", label: t("ci.f_ext"), n: s.external, color: p.accent },
    ];
    const figs = parts.map((x) => `<div class="ci-fig"><div class="ci-fig-n" style="color:${x.color}">${s.total ? Math.round((x.n / s.total) * 100) : 0}%</div><div class="ci-fig-c">${esc(nfmt(x.n))} · ${esc(x.label)}</div></div>`).join("");
    const rows = w.authors.slice(0, 10).map((a) => {
      const badge = a.is_self ? `<span class="ci-badge ci-badge-self">${esc(t("ci.badge_self"))}</span>` : a.is_coauthor ? `<span class="ci-badge ci-badge-co">${esc(t("ci.badge_co"))}</span>` : "";
      return `<tr><td title="${esc(a.name)}">${esc(trunc(a.name, 34))}${badge}</td><td title="${esc(a.institution)}">${esc(a.institution ? trunc(a.institution, 28) : "—")}</td><td class="num">${nfmt(a.n)}</td></tr>`;
    }).join("");
    const left = `${w.orcid_set ? "" : `<p class="ci-info">${esc(t("ci.no_orcid"))}</p>`}
      <div class="ci-figs">${figs}</div>${L.stackBar(parts, { big: true })}${S.exclude ? `<p class="ci-info ci-note">${esc(t("ci.note_excl"))}</p>` : ""}`;
    const right = w.authors.length ? `<table class="ci-table ci-t-auth"><thead><tr><th>${esc(t("ci.col_author"))}</th><th>${esc(t("ci.col_inst"))}</th><th class="num">${esc(t("ci.col_n"))}</th></tr></thead><tbody>${rows}</tbody></table>` : slotMsg(t("ci.empty_table"));
    const g = L.grammar;
    const orcNeeds = w.cov_orcid.needs_data;
    const orc = orcNeeds ? "" : w.cov_orcid.missing > 0 ? t("ci.cap_orcid_none", { n: nfmt(w.cov_orcid.n), x: g.locNum(w.cov_orcid.missing) }) : t("ci.cap_orcid_all", { n: nfmt(w.cov_orcid.n) });
    const cap = t("ci.cap_who", { a: g.numPoss(s.by_orcid), b: g.numPoss(s.by_name), orc }).trim();
    const needs = orcNeeds ? `<div class="ci-needs">${slotNeeds("ci.w_orcid")}</div>` : "";
    return section("who", s.total ? L.sentences.who(w) : "", `<div class="ci-row ci-r23">${col(t("ci.t_split"), left)}${col(t("ci.t_authors"), right)}</div>${needs}`, cap);
  }

  function buildInfluential(d) {
    const inf = d.influential;
    if (inf.cov.needs_data) return section("influential", "", needsBody("ci.w_cited"), L.caption(inf.cov.n, [{ what: "ci.w_cited", missing: inf.cov.missing }]));
    const rows = inf.items.map((x) => {
      const art = x.articles[0] || "";
      const more = x.articles.length > 1 ? ` <span class="secondary">+${x.articles.length - 1}</span>` : "";
      return `<tr><td class="ci-t-title"><a href="#" class="title-link" data-url="${esc(x.url || (x.doi ? "https://doi.org/" + x.doi : ""))}" title="${esc(x.title)}">${esc(trunc(x.title || "—", 80))}</a></td>
        <td class="ci-t-venue" title="${esc(x.venue)}">${esc(x.venue ? trunc(x.venue, 36) : "—")}</td><td class="num ci-t-year">${esc(x.year || "—")}</td>
        <td title="${esc(x.articles.join(" · "))}">${esc(trunc(art, 40))}${more}</td><td class="num"><b>${nfmt(x.cited_by)}</b></td></tr>`;
    }).join("");
    const body = inf.items.length ? `<div class="ci-table-wrap"><table class="ci-table ci-infl"><thead><tr><th>${esc(t("ci.col_cw"))}</th><th class="ci-t-venue">${esc(t("ci.col_venue"))}</th><th class="num ci-t-year">${esc(t("ci.col_year"))}</th><th>${esc(t("ci.col_article"))}</th><th class="num">${esc(t("ci.col_cited"))}</th></tr></thead><tbody>${rows}</tbody></table></div>` : slotMsg(t("ci.empty_table"));
    return section("influential", L.sentences.influential(inf), body, L.caption(inf.cov.n, [{ what: "ci.w_cited", missing: inf.cov.missing }]));
  }

  // ------------------------------------------------------------ drawing
  function fillFilters(meta) {
    const sel = $("#ciArticle");
    const list = meta.articles.slice();
    if (S.article && !list.some((a) => a.key === S.article)) list.push({ key: S.article, title: S.titles.get(S.article) || B.articleTitle(S.article) || S.article, n: 0 });
    sel.innerHTML = `<option value="">${esc(t("ci.all_articles", { n: nfmt(meta.articles.length) }))}</option>`
      + list.map((a) => `<option value="${esc(a.key)}">${esc(trunc(a.title, 70))} (${nfmt(a.n)})</option>`).join("");
    sel.value = S.article;
    $("#ciCount").textContent = meta.n === 1 ? t("ci.count_1") : t("ci.count", { n: nfmt(meta.n) });
  }

  function showMsg(html) {
    const box = $("#ciMsg");
    box.innerHTML = html;
    box.hidden = !html;
  }

  function draw() {
    clearCharts();
    const host = $("#ciSections");
    host.innerHTML = "";
    const d = S.data;
    if (!d) {
      $("#ciIndex").hidden = true;
      showMsg(S.error ? `<p class="ci-err">${esc(S.error)}</p>` : "");
      return;
    }
    fillFilters(d.meta);
    if (d.meta.n_all === 0) {
      $("#ciIndex").hidden = true;
      showMsg(`<div class="empty-icon icon">&#xE716;</div><p>${esc(t("ci.none_all"))}</p><button type="button" class="btn btn-primary ci-refresh">${esc(t("ci.refresh"))}</button>`);
      return;
    }
    if (d.meta.n === 0) {
      $("#ciIndex").hidden = true;
      showMsg(`<p>${esc(t("ci.none_filter"))}</p><button type="button" class="btn btn-link ci-reset">${esc(t("ci.reset"))}</button>`);
      return;
    }
    showMsg("");
    $("#ciIndex").hidden = false;
    const jobs = [];
    host.innerHTML = [buildCountries(d, jobs), buildInstitutions(d, jobs), buildVenues(d, jobs), buildFields(d, jobs), buildTypes(d), buildWho(d), buildInfluential(d)].join("");
    for (const j of jobs) { try { j(); } catch (e) { console.error(e); } }
    spy();
  }

  // ------------------------------------------------------------ category index: scroll to a card, mark the one in view
  const stickyBottom = () => { const el = $("#ciSticky"); return el ? el.getBoundingClientRect().bottom : 0; };
  function spy() {
    if (!S.active) return;
    const cards = $$("#ciSections .ci-card");
    if (!cards.length) return;
    const line = stickyBottom() + 24;
    let cur = cards[0].dataset.sec;
    for (const c of cards) if (c.getBoundingClientRect().top <= line) cur = c.dataset.sec;
    const sc = $(".content");
    if (sc && sc.scrollTop + sc.clientHeight >= sc.scrollHeight - 4) cur = cards[cards.length - 1].dataset.sec;
    $$("#ciIndex button").forEach((b) => { const on = b.dataset.sec === cur; b.classList.toggle("on", on); if (on) b.setAttribute("aria-current", "true"); else b.removeAttribute("aria-current"); });
  }
  function goTo(sec) {
    const el = $("#ci-" + sec);
    if (!el) return;
    const sc = $(".content");
    const scr = sc.getBoundingClientRect();
    const top = sc.scrollTop + (el.getBoundingClientRect().top - scr.top) - (stickyBottom() - scr.top) - 8;
    sc.scrollTo({ top, behavior: reducedMotion() ? "auto" : "smooth" });
  }

  /** The range selector of the page lives in the filter row of this tab while it is shown. */
  function mountRange(on) {
    const seg = $("#anRange"), custom = $("#anCustom");
    const slot = $("#ciRangeSlot");
    if (!seg || !slot) return;
    if (on) { if (seg.parentElement !== slot) slot.append(seg, custom); } else if (seg.parentElement === slot) {
      const spacer = $(".an-head .an-spacer");
      spacer.before(seg, custom);
    }
  }

  function init(bridge) {
    B = bridge;
    $("#ciArticle").addEventListener("change", (e) => { S.article = e.target.value; render(); });
    $("#ciExclude").addEventListener("change", (e) => { S.exclude = e.target.checked; saveExclude(S.exclude); render(); });
    $("#ciIndex").addEventListener("click", (e) => { const b = e.target.closest("button[data-sec]"); if (b) goTo(b.dataset.sec); });
    $("#anTab-citers").addEventListener("click", (e) => {
      const v = e.target.closest(".ci-view");
      if (v) { B.openList({ article: S.article, excludeSelf: S.exclude, sort: v.dataset.view === "influential" ? "cited" : "" }); return; }
      if (e.target.closest(".ci-refresh")) { B.refresh(); return; }
      if (e.target.closest(".ci-reset")) { S.article = ""; S.exclude = false; saveExclude(false); B.resetRange(); }
    });
    const sc = $(".content");
    let raf = 0;
    sc.addEventListener("scroll", () => { if (!S.active) return; cancelAnimationFrame(raf); raf = requestAnimationFrame(spy); }, { passive: true });
    window.addEventListener("resize", () => { if (S.active) spy(); });
  }

  window.Citers = { init, render, invalidate, leave, mountRange, filter: () => ({ article: S.article, excludeSelf: S.exclude }) };
})();
