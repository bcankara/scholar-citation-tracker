"use strict";
/* Builds the citation report from the JSON that the app produces (command `report_payload`),
   then tells Rust it is ready so WebView2 can print the page to PDF (src-tauri/src/pdf.rs). */
(async () => {
  const { invoke } = window.__TAURI__.core;
  const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
  const MAX_CITING = 1500;

  const raw = await invoke("report_payload");
  if (!raw) throw new Error("no report data");
  const d = JSON.parse(raw);
  window.I18N.setLang(d.lang);
  if (window.CitersLib && window.CitersLib.ojp) window.CitersLib.ojp.setMeta(d.ojp_meta);
  const { t } = window.I18N;
  const loc = window.I18N.locale;
  const nf = new Intl.NumberFormat(loc);
  const signed = (n) => (n > 0 ? "+" + nf.format(n) : n < 0 ? "−" + nf.format(-n) : "0");
  const cls = (n) => (n > 0 ? "pos" : n < 0 ? "neg" : "");

  /** `YYYY-MM-DD`, `YYYY-MM` or `YYYY` as text. */
  function fmtPeriod(s) {
    const [y, m, day] = s.split("-").map(Number);
    if (!m) return String(y);
    if (!day) return new Date(Date.UTC(y, m - 1, 1)).toLocaleDateString(loc, { month: "short", year: "numeric", timeZone: "UTC" });
    return new Date(Date.UTC(y, m - 1, day)).toLocaleDateString(loc, { day: "numeric", month: "short", year: "numeric", timeZone: "UTC" });
  }
  const fmtDayLong = (s) => {
    const [y, m, day] = s.split("-").map(Number);
    return new Date(Date.UTC(y, m - 1, day)).toLocaleDateString(loc, { day: "numeric", month: "long", year: "numeric", timeZone: "UTC" });
  };
  const fmtInstant = (iso) => new Date(iso).toLocaleDateString(loc, { day: "numeric", month: "long", year: "numeric" });
  const fmtGenerated = (s) => {
    const [dd, tt] = s.split(" ");
    const [y, m, day] = dd.split("-").map(Number);
    const [h, mi] = (tt || "00:00").split(":").map(Number);
    return new Date(y, m - 1, day, h, mi).toLocaleString(loc, { day: "numeric", month: "long", year: "numeric", hour: "2-digit", minute: "2-digit" });
  };

  // ------------------------------------------------------------ sections
  const s = d.summary;
  const range = `${fmtDayLong(d.from)} – ${fmtDayLong(d.to)}`;

  const cover = `<section class="cover">
    <div class="eyebrow">Scholar Citation Tracker</div>
    <div class="bar"></div>
    <h1>${esc(t("doc.title"))}</h1>
    <div class="sub">${esc(t("doc.subtitle"))}</div>
    <dl>
      <dt>${esc(t("doc.profile"))}</dt><dd>${esc(d.profile_name || "—")}</dd>
      <dt>${esc(t("doc.orcid"))}</dt><dd>${esc(d.orcid || "—")}</dd>
      <dt>${esc(t("doc.range"))}</dt><dd>${esc(range)}</dd>
      <dt>${esc(t("doc.generated"))}</dt><dd>${esc(fmtGenerated(d.generated))}</dd>
    </dl>
  </section>`;

  const hasBase = !!s.start_at;
  const metric = (label, value, unit, c) => `<div class="metric"><div class="label">${esc(label)}</div><div class="value ${c || ""}">${esc(value)}</div>${unit ? `<div class="unit">${esc(unit)}</div>` : ""}</div>`;
  let baselineNote;
  if (!hasBase) baselineNote = t("doc.baseline_none");
  else if (s.baseline_is_first) baselineNote = t("doc.baseline_first", { date: fmtInstant(s.start_at) });
  else baselineNote = t("doc.baseline", { date: fmtInstant(s.start_at) });
  const summary = `<section>
    <h2>${esc(t("doc.summary"))}</h2>
    <div class="metrics">
      ${metric(t("doc.m_total"), nf.format(s.cit_all))}
      ${metric(t("doc.m_since", { year: s.since_year }), nf.format(s.cit_since))}
      ${metric(t("doc.m_h"), nf.format(s.h_all), hasBase && s.change_h ? signed(s.change_h) : "", cls(s.change_h))}
      ${metric(t("doc.m_i10"), nf.format(s.i10_all), hasBase && s.change_i10 ? signed(s.change_i10) : "", cls(s.change_i10))}
      ${metric(t("doc.m_change"), hasBase ? signed(s.change_all) : "—", t("doc.change_unit"), hasBase ? cls(s.change_all) : "")}
    </div>
    <p class="small muted">${esc(t("doc.as_of", { date: fmtInstant(s.end_at) }))} ${esc(baselineNote)}</p>
  </section>`;

  const rows = d.articles.map((a) => `<tr><td class="title">${esc(a.title)}</td><td class="num">${a.year ?? "—"}</td>
      <td class="num"><b>${nf.format(a.citations)}</b></td><td class="num ${cls(a.gain)}">${hasBase ? signed(a.gain) : "—"}</td></tr>`).join("");
  const articles = `<section>
    <h2>${esc(t("doc.articles"))}</h2>
    <table><thead><tr><th>${esc(t("doc.col_title"))}</th><th class="num">${esc(t("doc.col_year"))}</th><th class="num">${esc(t("doc.col_cites"))}</th><th class="num">${esc(t("doc.col_gain"))}</th></tr></thead>
    <tbody>${rows}</tbody></table>
  </section>`;

  // ------------------------------------------------------------ charts (the same builders as the app window)
  /** Chart boxes to fill after the markup is in the page: [{id, build, height}] (drawn as SVG, vector in the PDF). */
  const chartJobs = [];
  const box = (id, height, title) => `<div class="chart"><h3>${esc(title)}</h3><div class="chart-box" id="${id}" style="width:${CHART_W}px;height:${height}px"></div>`;
  const CHART_W = 670;
  const now = Date.now();
  const rng = {
    from: new Date(d.from + "T00:00:00").getTime(),
    to: (() => { const e = new Date(d.to + "T23:59:59").getTime(); return e >= now - 86400e3 ? null : e; })(),
  };
  const A = d.analysis;
  let charts = "";
  if (d.charts && d.charts.length && A) {
    const cap = (text) => `<p class="small muted cap">${esc(text)}</p>`;
    const none = `<div class="chart-empty">${esc(t("doc.chart_few"))}</div>`;
    let n = 0;
    const add = (height, title, build, caption) => {
      const id = "ch" + n++;
      chartJobs.push({ id, build });
      return box(id, height, title) + (caption ? cap(caption) : "") + "</div>";
    };
    const tr = A.trend;
    const parts = [];
    for (const id of d.charts) {
      if (id === "trend") {
        let c = t(tr.hist.length ? "trend.cap_hist" : "trend.cap_scholar");
        if (tr.forecast_hidden) c += " " + t("trend.fc_hidden", { need: tr.forecast_hidden[1], have: tr.forecast_hidden[0], left: Math.max(1, tr.forecast_hidden[1] - tr.forecast_hidden[0]) });
        else if (tr.forecast) c += " " + t(tr.forecast.thin ? "trend.fc_thin" : "trend.fc_basis");
        const f = tr.forecast;
        const stats = `<div class="metrics three">
          ${metric(t("fc.year_end", { year: tr.year }), f ? `≈ ${nf.format(f.mid)}` : "—", f ? `${nf.format(f.low)}–${nf.format(f.high)}` : "")}
          ${metric(t("fc.per_month"), tr.monthly_avg === null ? "—" : nf.format(Math.round(tr.monthly_avg * 10) / 10))}
          ${metric(t("fc.last12"), tr.last12 === null ? "—" : nf.format(tr.last12), tr.last12 !== null && tr.last12_days < 360 ? t("trend.last_days", { n: tr.last12_days }) : "")}</div>`;
        parts.push(`<h2>${esc(t("an.q_trend"))}</h2>` + add(270, t("doc.ch_trend"), () => Charts.build.trend(A, Charts.ctx({ print: true, range: rng })), c) + stats);
      } else if (id === "period") {
        const gran = Charts.util.autoGran(rng.from, rng.to ?? now);
        parts.push(`<h2>${esc(t("an.q_period"))}</h2>` + add(230, t("main.g_" + gran), () => Charts.build.period(A, Charts.ctx({ print: true, range: rng, gran, oaLatest: tr.oa_latest })), t("an.src_citing") + ". " + (tr.oa_latest ? t("ris.lag", { date: fmtDayLong(tr.oa_latest) }) + " " + t("an.lag_tail_note", { date: fmtDayLong(tr.oa_latest) }) : t("ris.lag_none"))));
      } else if (id === "article") {
        let c = t("an.cap_stacked");
        if (A.stacked.year_only > 0) c += " " + t("an.cap_stacked_year", { n: nf.format(A.stacked.year_only) });
        parts.push(`<h2>${esc(t("an.q_stacked"))}</h2>` + add(280, t("doc.ch_article"), () => Charts.build.stacked(A, Charts.ctx({ print: true, range: rng })), c));
      } else if (id === "index") {
        parts.push(`<h2>${esc(t("an.q_indices"))}</h2>` + add(230, t("doc.ch_index"), () => Charts.build.indices(A, Charts.ctx({ print: true, range: rng })), t("an.cap_index")));
      } else if (id === "calendar") {
        const years = A.calendar.years.filter((y) => y >= new Date(rng.from).getFullYear() && y <= new Date(rng.to ?? now).getFullYear());
        const shown = years.slice(-3);
        const c = t("an.cap_cal", { n: nf.format(A.calendar.days.filter(([day]) => shown.some((y) => day.startsWith(String(y)))).length) })
          + (A.calendar.month_only + A.calendar.year_only > 0 ? " " + t("an.cap_cal_undated", { n: nf.format(A.calendar.month_only + A.calendar.year_only) }) : "");
        parts.push(`<h2>${esc(t("an.q_calendar"))}</h2>` + (shown.length
          ? shown.map((y, i) => add(190, String(y), () => Charts.build.calendar(A, Charts.ctx({ print: true, range: rng, year: y, oaLatest: tr.oa_latest })), i === shown.length - 1 ? c : "")).join("")
          : none));
      } else if (id === "dist") {
        const lang = window.I18N.lang;
        const dd = A.dist;
        const notes = [t("doc.dist_range"), t("dist.distinct")];
        if (dd.without_country > 0 && dd.with_country > 0) notes.push(t("dist.country_missing", { n: nf.format(dd.without_country), total: nf.format(dd.total) }));
        parts.push(`<h2>${esc(t("doc.ch_dist"))}</h2>`
          + add(Math.max(120, dd.countries.length * 24 + 40), t("dist.country"), () => Charts.build.bars(dd.countries, Charts.ctx({ print: true, labelOf: (k) => `${window.COUNTRIES.name(k, lang)} (${k})`, total: dd.with_country, labelWidth: 170 })), "")
          + add(Math.max(120, dd.venues.length * 24 + 40), t("dist.venue"), () => Charts.build.bars(dd.venues, Charts.ctx({ print: true, labelOf: (k) => k, total: dd.total - dd.without_venue, labelWidth: 250 })), notes.join(" ")));
      }
    }
    charts = `<section class="page-start">${parts.join("")}</section>`;
  }

  // ------------------------------------------------------------ optional citing-works sections (the report's own period)
  let citSections = "";
  if (d.citers) {
    const CI = window.CitersLib;
    const cc = d.citers;
    const pc = () => Charts.ctx({ print: true });
    const capP = (text) => `<p class="small muted cap">${esc(text)}</p>`;
    const none = `<div class="chart-empty">${esc(t("doc.ci_none"))}</div>`;
    let k = 0;
    const cbox = (height, title, build) => {
      const id = "cit" + k++;
      chartJobs.push({ id, build });
      return `<div class="chart"><h3>${esc(title)}</h3><div class="chart-box" id="${id}" style="width:${CHART_W}px;height:${height}px"></div></div>`;
    };
    const parts = [];
    if (cc.countries && cc.institutions) {
      const cr = cc.countries, ir = cc.institutions;
      let h = `<h2>${esc(t("doc.ci_geo"))}</h2>`;
      if (!cr.items.length && !ir.items.length) h += none;
      else {
        if (cr.items.length) {
          const top = cr.items.slice(0, 10).map((x) => ({ name: CI.names.countryName(x.key), n: x.n }));
          h += `<p>${CI.sentences.countries(cr)}</p>` + cbox(Math.max(120, top.length * 24 + 24), t("ci.t_top_countries"), () => CI.hbars(top, Charts.ctx({ print: true }), { labelWidth: 170 }))
            + capP(CI.caption(cr.cov.n, [{ what: "ci.w_country", missing: cr.cov.missing }]));
        }
        if (ir.items.length) {
          const rows = ir.items.slice(0, 10).map((x) => `<tr><td>${esc(x.name)}${x.is_new ? ` <b class="pos">· ${esc(t("doc.ci_new"))}</b>` : ""}</td><td>${esc(x.country ? CI.names.countryName(x.country) : "—")}</td><td class="num"><b>${nf.format(x.n)}</b></td></tr>`).join("");
          h += `<p>${CI.sentences.institutions(ir)}</p><div class="chart"><h3>${esc(t("ci.t_top_inst"))}</h3><table><thead><tr><th>${esc(t("ci.col_inst"))}</th><th>${esc(t("ci.col_country"))}</th><th class="num">${esc(t("ci.col_n"))}</th></tr></thead><tbody>${rows}</tbody></table></div>`
            + capP(CI.caption(ir.cov.n, [{ what: "ci.w_institution", missing: ir.cov.missing }]));
        }
      }
      parts.push(h);
    }
    if (cc.ojp) {
      const o = cc.ojp;
      let h = `<h2>${esc(t("doc.ci_ojp"))}</h2>`;
      if (!o.ranked) h += none;
      else {
        const rows = o.venues.map((x) => `<tr><td>${esc(x.name)}</td><td class="ojpc">${CI.ojp.badge(x) || "—"}</td><td class="num"><b>${nf.format(x.n)}</b></td></tr>`).join("");
        h += `<p>${CI.ojp.answer(o)}</p>` + cbox(220, t("ci.t_ojp_hist"), () => CI.ojpHist(o.hist, Charts.ctx({ print: true })))
          + (o.venues.length ? `<div class="chart"><h3>${esc(t("ci.t_ojp_venues"))}</h3><table class="ojp-tbl"><thead><tr><th>${esc(t("ci.col_journal"))}</th><th class="ojpc">OJP</th><th class="num">${esc(t("ci.col_n"))}</th></tr></thead><tbody>${rows}</tbody></table></div>` : "");
      }
      h += capP(t("ci.ojp_cap", { n: nf.format(o.n), a: nf.format(o.none_few + o.none_unscored), b: nf.format(o.none_absent), c: nf.format(o.none_no_journal), edition: CI.ojp.vars().edition }));
      parts.push(h);
    }
    if (cc.fields) {
      const f = cc.fields;
      let h = `<h2>${esc(t("doc.ci_fields"))}</h2>`;
      if (!f.domains.length) h += none;
      else {
        const tm = CI.treemapSvg(f.domains, CHART_W, 300, pc());
        const den = Math.max(1, f.cov.n - f.cov.missing);
        const rows = f.fields.slice(0, 10).map((x) => `<tr><td>${esc(CI.names.fieldName(x.name))}</td><td class="num"><b>${nf.format(x.n)}</b></td><td class="num">${Math.round((x.n / den) * 100)}%</td></tr>`).join("");
        h += `<p>${CI.sentences.fields(f)}</p><div class="chart"><h3>${esc(t("ci.t_field_dist"))}</h3>${tm.svg}</div>`
          + `<div class="chart"><table><thead><tr><th>${esc(t("doc.ci_field_col"))}</th><th class="num">${esc(t("ci.col_n"))}</th><th class="num">${esc(t("doc.ci_share"))}</th></tr></thead><tbody>${rows}</tbody></table></div>`
          + capP(CI.caption(f.cov.n, [{ what: "ci.w_topic", missing: f.cov.missing }]));
      }
      parts.push(h);
    }
    if (cc.who) {
      const w = cc.who, sp = w.split;
      let h = `<h2>${esc(t("doc.ci_self"))}</h2>`;
      if (!sp.total) h += none;
      else {
        const pal = CI.palette(pc());
        const bars = [
          { label: t("ci.f_self"), n: sp.self_n, color: pal.orange },
          { label: t("ci.f_co"), n: sp.coauthor, color: pal.purple },
          { label: t("ci.f_ext"), n: sp.external, color: pal.accent },
        ];
        const figs = bars.map((x) => `<div class="ci-fig"><div class="ci-fig-n" style="color:${x.color}">${Math.round((x.n / sp.total) * 100)}%</div><div class="ci-fig-c">${nf.format(x.n)} · ${esc(x.label)}</div></div>`).join("");
        const rows = w.authors.slice(0, 10).map((a) => `<tr><td>${esc(a.name)}${a.is_self ? ` <b class="pos">· ${esc(t("ci.badge_self"))}</b>` : a.is_coauthor ? ` <span class="muted">· ${esc(t("ci.badge_co"))}</span>` : ""}</td><td>${esc(a.institution || "—")}</td><td class="num"><b>${nf.format(a.n)}</b></td></tr>`).join("");
        h += `<p>${CI.sentences.who(w)}</p><div class="ci-figs">${figs}</div>${CI.stackBar(bars, { big: true })}`
          + (w.authors.length ? `<div class="chart"><h3>${esc(t("ci.t_authors"))}</h3><table><thead><tr><th>${esc(t("ci.col_author"))}</th><th>${esc(t("ci.col_inst"))}</th><th class="num">${esc(t("ci.col_n"))}</th></tr></thead><tbody>${rows}</tbody></table></div>` : "")
          + `<p class="small muted cap">${esc(t("doc.ci_method", { a: nf.format(sp.by_orcid), b: nf.format(sp.by_name) }))}${w.orcid_set ? "" : " " + esc(t("ci.no_orcid"))}</p>`;
      }
      parts.push(h);
    }
    citSections = parts.map((x) => `<section class="page-start">${x}</section>`).join("");
  }

  let citing = "";
  if (d.include_citing) {
    const shown = d.citing.slice(0, MAX_CITING);
    const crow = shown.map((c) => `<tr><td class="date">${esc(fmtPeriod(c.date))}</td><td class="title">${esc(c.title || "—")}</td><td>${esc(c.venue || "—")}</td><td class="col-ojp">${window.CitersLib.ojp.badge(c)}</td><td>${esc(c.article)}</td></tr>`).join("");
    citing = `<section>
      <h2>${esc(t("doc.citing"))}</h2>
      ${d.citing_total
        ? `<p class="small muted">${esc(t("doc.citing_intro", { n: nf.format(d.citing_total) }))}${d.citing.length > MAX_CITING ? " " + esc(t("doc.truncated", { n: nf.format(MAX_CITING) })) : ""}</p>
           <table class="citing"><thead><tr><th>${esc(t("doc.col_date"))}</th><th>${esc(t("doc.col_ctitle"))}</th><th>${esc(t("doc.col_venue"))}</th><th class="col-ojp">${esc(t("doc.col_ojp"))}</th><th>${esc(t("doc.col_article"))}</th></tr></thead><tbody>${crow}</tbody></table>`
        : `<div class="chart-empty">${esc(t("doc.citing_none"))}</div>`}
    </section>`;
  }

  const method = `<section class="method">
    <h2>${esc(t("doc.method"))}</h2>
    <p>${esc(t("doc.method_scholar"))}</p>
    <p>${esc(t("doc.method_citing"))}</p>
    <p>${esc(t("doc.method_diff"))}</p>
    ${d.charts && d.charts.length ? `<p>${esc(t("doc.method_charts"))}</p>` : ""}
    ${d.include_citing || (d.citers && d.citers.ojp) ? `<p>${esc(t("doc.method_ojp", window.CitersLib.ojp.vars()))}</p>` : ""}
  </section>`;

  // Own footer (no browser header/footer): custom text left, "Page n/total" right.
  const q = (x) => JSON.stringify(x);
  const st = document.createElement("style");
  st.textContent = `@page { @bottom-left { content: ${q(t("doc.footer"))}; font: 8pt "Segoe UI", sans-serif; color: #7a7a7a; }
    @bottom-right { content: ${q(t("doc.page") + " ")} counter(page) "/" counter(pages); font: 8pt "Segoe UI", sans-serif; color: #7a7a7a; } }`;
  document.head.appendChild(st);
  document.getElementById("doc").innerHTML = cover + summary + articles + charts + citSections + citing + method;
  for (const job of chartJobs) {
    const el = document.getElementById(job.id);
    const built = job.build();
    if (built) {
      const chart = Charts.mount(el, built, { renderer: "svg", width: CHART_W, height: parseInt(el.style.height, 10) });
      try { chart.getZr().refreshImmediately(); } catch { /* painted on the next frame instead */ }
    } else {
      el.innerHTML = `<div class="chart-empty">${esc(t("doc.chart_few"))}</div>`;
      el.style.height = "auto";
    }
  }
  await new Promise((r) => setTimeout(r, 250));
  document.title = `${t("doc.title")} ${d.from} – ${d.to}`;
  if (document.fonts && document.fonts.ready) await document.fonts.ready;
  await invoke("report_ready");
})().catch((e) => {
  console.error(e);
  document.body.textContent = "Report error: " + e;
});
