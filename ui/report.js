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

  // ------------------------------------------------------------ charts (inline SVG)
  /** Round tick values covering [min, max]. */
  function niceTicks(min, max, count) {
    if (max === min) { max += 1; min = Math.max(0, min - 1); }
    const raw = (max - min) / count;
    const mag = Math.pow(10, Math.floor(Math.log10(raw)));
    const norm = raw / mag;
    const step = (norm <= 1 ? 1 : norm <= 2 ? 2 : norm <= 5 ? 5 : 10) * mag;
    const out = [];
    for (let v = Math.floor(min / step) * step; v <= Math.ceil(max / step) * step + step / 2; v += step) out.push(Math.round(v * 1e6) / 1e6);
    return out;
  }

  function lineChart(points) {
    const W = 680, H = 220, m = { l: 46, r: 14, t: 12, b: 26 };
    const ts = points.map((p) => new Date(p[0]).getTime());
    const vs = points.map((p) => p[1]);
    const vMin = Math.min(...vs), vMax = Math.max(...vs);
    const ticks = niceTicks(vMin, vMax, 4);
    const y0 = ticks[0], y1 = ticks[ticks.length - 1];
    const tMin = Math.min(...ts), tMax = Math.max(...ts);
    const X = (tv) => m.l + (tMax === tMin ? 0.5 : (tv - tMin) / (tMax - tMin)) * (W - m.l - m.r);
    const Y = (v) => m.t + (1 - (v - y0) / (y1 - y0 || 1)) * (H - m.t - m.b);
    let svg = `<svg viewBox="0 0 ${W} ${H}" role="img">`;
    for (const tv of ticks) {
      svg += `<line class="grid" x1="${m.l}" x2="${W - m.r}" y1="${Y(tv).toFixed(1)}" y2="${Y(tv).toFixed(1)}"/>`;
      svg += `<text x="${m.l - 6}" y="${(Y(tv) + 3).toFixed(1)}" text-anchor="end">${nf.format(tv)}</text>`;
    }
    const d1 = points.map((p, i) => `${i ? "L" : "M"}${X(ts[i]).toFixed(1)},${Y(p[1]).toFixed(1)}`).join(" ");
    const base = H - m.b;
    svg += `<path class="area" d="${d1} L${X(ts[ts.length - 1]).toFixed(1)},${base} L${X(ts[0]).toFixed(1)},${base} Z"/>`;
    svg += `<path class="line" d="${d1}"/>`;
    points.forEach((p, i) => { svg += `<circle class="dot" cx="${X(ts[i]).toFixed(1)}" cy="${Y(p[1]).toFixed(1)}" r="2.2"/>`; });
    const labelFmt = (tv) => new Date(tv).toLocaleDateString(loc, { day: "numeric", month: "short", year: "2-digit" });
    svg += `<text x="${m.l}" y="${H - 8}" text-anchor="start">${esc(labelFmt(tMin))}</text>`;
    if (tMax !== tMin) svg += `<text x="${W - m.r}" y="${H - 8}" text-anchor="end">${esc(labelFmt(tMax))}</text>`;
    return svg + "</svg>";
  }

  function barChart(data) {
    const W = 680, H = 210, m = { l: 46, r: 14, t: 22, b: 26 };
    const ticks = niceTicks(0, Math.max(1, ...data.map((x) => x[1])), 4);
    const y1 = ticks[ticks.length - 1];
    const Y = (v) => m.t + (1 - v / (y1 || 1)) * (H - m.t - m.b);
    const band = (W - m.l - m.r) / data.length;
    const bw = Math.min(34, band * 0.62);
    let svg = `<svg viewBox="0 0 ${W} ${H}" role="img">`;
    for (const tv of ticks) {
      svg += `<line class="grid" x1="${m.l}" x2="${W - m.r}" y1="${Y(tv).toFixed(1)}" y2="${Y(tv).toFixed(1)}"/>`;
      svg += `<text x="${m.l - 6}" y="${(Y(tv) + 3).toFixed(1)}" text-anchor="end">${nf.format(tv)}</text>`;
    }
    data.forEach(([label, v], i) => {
      const cx = m.l + band * (i + 0.5);
      const bh = Math.max(v > 0 ? 1.5 : 0, Y(0) - Y(v));
      svg += `<rect class="bar" x="${(cx - bw / 2).toFixed(1)}" y="${(Y(0) - bh).toFixed(1)}" width="${bw.toFixed(1)}" height="${bh.toFixed(1)}" rx="2"/>`;
      svg += `<text class="val" x="${cx.toFixed(1)}" y="${(Y(0) - bh - 4).toFixed(1)}" text-anchor="middle">${nf.format(v)}</text>`;
      svg += `<text x="${cx.toFixed(1)}" y="${H - 8}" text-anchor="middle">${esc(label)}</text>`;
    });
    return svg + "</svg>";
  }

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

  let charts = "";
  if (d.include_charts) {
    const few = `<div class="chart-empty">${esc(t("doc.chart_few"))}</div>`;
    charts = `<section>
      <h2>${esc(t("doc.charts"))}</h2>
      <div class="chart"><h3>${esc(t("doc.chart_time"))}</h3>${d.series.length >= 2 ? lineChart(d.series) : few}</div>
      <div class="chart"><h3>${esc(t("doc.chart_year"))}</h3>${d.yearly.length ? barChart(d.yearly.map(([y, c]) => [String(y), c])) : few}</div>
    </section>`;
  }

  let citing = "";
  if (d.include_citing) {
    const shown = d.citing.slice(0, MAX_CITING);
    const crow = shown.map((c) => `<tr><td class="date">${esc(fmtPeriod(c.date))}</td><td class="title">${esc(c.title || "—")}</td><td>${esc(c.venue || "—")}</td><td>${esc(c.article)}</td></tr>`).join("");
    citing = `<section>
      <h2>${esc(t("doc.citing"))}</h2>
      ${d.citing_total
        ? `<p class="small muted">${esc(t("doc.citing_intro", { n: nf.format(d.citing_total) }))}${d.citing.length > MAX_CITING ? " " + esc(t("doc.truncated", { n: nf.format(MAX_CITING) })) : ""}</p>
           <table class="citing"><thead><tr><th>${esc(t("doc.col_date"))}</th><th>${esc(t("doc.col_ctitle"))}</th><th>${esc(t("doc.col_venue"))}</th><th>${esc(t("doc.col_article"))}</th></tr></thead><tbody>${crow}</tbody></table>`
        : `<div class="chart-empty">${esc(t("doc.citing_none"))}</div>`}
    </section>`;
  }

  const method = `<section class="method">
    <h2>${esc(t("doc.method"))}</h2>
    <p>${esc(t("doc.method_scholar"))}</p>
    <p>${esc(t("doc.method_citing"))}</p>
    <p>${esc(t("doc.method_diff"))}</p>
  </section>`;

  // Own footer (no browser header/footer): custom text left, "Page n/total" right.
  const q = (x) => JSON.stringify(x);
  const st = document.createElement("style");
  st.textContent = `@page { @bottom-left { content: ${q(t("doc.footer"))}; font: 8pt "Segoe UI", sans-serif; color: #7a7a7a; }
    @bottom-right { content: ${q(t("doc.page") + " ")} counter(page) "/" counter(pages); font: 8pt "Segoe UI", sans-serif; color: #7a7a7a; } }`;
  document.head.appendChild(st);
  document.getElementById("doc").innerHTML = cover + summary + articles + charts + citing + method;
  document.title = `${t("doc.title")} ${d.from} – ${d.to}`;
  if (document.fonts && document.fonts.ready) await document.fonts.ready;
  await invoke("report_ready");
})().catch((e) => {
  console.error(e);
  document.body.textContent = "Report error: " + e;
});
