"use strict";
/* Chart builders, text builders and grammar helpers of the "Citing works" tab (Analysis) and of the PDF report.
   Pure functions of the data that the backend returns (get_citers_*): no DOM access outside the elements they are given.
   Colours come from the CSS tokens of the page; the orange token is reserved for "new" (and the self share of the split). */
(() => {
  const E = window.echarts;
  const C = window.Charts;
  const I = window.I18N;
  const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
  const trunc = (s, n) => { s = String(s ?? ""); return s.length > n ? s.slice(0, n - 1).trimEnd() + "…" : s; };
  const pct = (n, den) => (den > 0 ? Math.min(100, Math.round((n / den) * 100)) : 0);
  const sum = (a, f = (x) => x) => a.reduce((s, x) => s + f(x), 0);

  // ------------------------------------------------------------ grammar (Turkish suffixes after numbers and names)
  // Possessive suffix of the last spoken word of a number: 12 (on iki) -> "si", 40 (kırk) -> "ı".
  const POSS_DIGIT = ["ı", "i", "si", "ü", "ü", "i", "sı", "si", "i", "u"]; // sıfır bir iki üç dört beş altı yedi sekiz dokuz
  const POSS_TENS = [null, "u", "si", "u", "ı", "si", "ı", "i", "i", "ı"]; // on yirmi otuz kırk elli altmış yetmiş seksen doksan
  function possSuffix(n) {
    n = Math.abs(Math.round(n));
    if (n === 0) return "ı";
    if (n % 1000 === 0) return "i"; // bin
    if (n % 100 === 0) return "ü"; // yüz
    if (n % 10 === 0) return POSS_TENS[(n / 10) % 10];
    return POSS_DIGIT[n % 10];
  }
  const isTr = () => I.lang === "tr";
  const nfmt = (n) => new Intl.NumberFormat(I.locale).format(n);
  /** "%12'si" (TR) / "12%" (EN) */
  const pctPoss = (n) => (isTr() ? `%${n}'${possSuffix(n)}` : `${n}%`);
  /** "13'ünde" (TR, in the 13th) / "13" (EN) */
  const locNum = (n) => {
    if (!isTr()) return nfmt(n);
    const p = possSuffix(n);
    return `${nfmt(n)}'${p}${/[iü]/.test(p.slice(-1)) ? "nde" : "nda"}`;
  };
  /** "6'sı" (TR) / "6" (EN) */
  const numPoss = (n) => (isTr() ? `${nfmt(n)}'${possSuffix(n)}` : nfmt(n));
  /** Turkish ablative of a (Turkish) place name: Almanya'dan, Çin'den, Birleşik Krallık'tan. */
  function abl(name) {
    const w = String(name).trim();
    const low = w.toLocaleLowerCase("tr");
    if (/leri$/.test(low)) return w + "'nden";
    if (/ları$/.test(low)) return w + "'ndan";
    if (/sahili$/.test(low)) return w + "'nden";
    let last = "";
    for (const ch of low) if ("aeıioöuü".includes(ch)) last = ch;
    const back = "aıou".includes(last);
    const end = low[low.length - 1];
    if ("aeıioöuü".includes(end)) return `${w}'d${back ? "an" : "en"}`;
    return `${w}'${"çfhkpsşt".includes(end) ? "t" : "d"}${back ? "an" : "en"}`;
  }
  /** "A ve B'dan" (TR) / "A and B" (EN) */
  function joinFrom(names) {
    const t = I.t;
    if (names.length === 1) return isTr() ? abl(names[0]) : names[0];
    const [a, b] = names;
    return isTr() ? `${a} ${t("ci.and")} ${abl(b)}` : `${a} ${t("ci.and")} ${b}`;
  }
  const joinAnd = (list) => (list.length < 2 ? list.join("") : list.slice(0, -1).join(", ") + " " + I.t("ci.and") + " " + list[list.length - 1]);

  // ------------------------------------------------------------ names
  const countryName = (iso) => window.COUNTRIES.name(iso, I.lang);
  const slug = (n) => String(n).toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
  function fieldName(name) {
    if (!name) return I.t("ci.other");
    const k = "fld." + slug(name);
    return I.dict[I.lang][k] ? I.t(k) : name;
  }
  function typeKey(raw) {
    const r = String(raw || "").toLowerCase();
    if (r === "article") return "article";
    if (r === "review") return "review";
    if (r === "proceedings-article" || r === "proceedings" || r === "conference-paper") return "proceedings";
    if (r === "book-chapter" || r === "book-section" || r === "chapter") return "chapter";
    if (r === "dissertation" || r === "thesis") return "dissertation";
    if (r === "preprint") return "preprint";
    if (r === "book") return "book";
    return "";
  }
  function typeName(raw) {
    const k = typeKey(raw);
    if (k) return I.t("ci.ty_" + k);
    const s = String(raw || "").replace(/-/g, " ");
    return s ? s.charAt(0).toUpperCase() + s.slice(1) : I.t("ci.other");
  }
  const oaKey = (raw) => { const r = String(raw || "").toLowerCase(); return r === "diamond" ? "gold" : r; };
  function langName(code) {
    try { const n = new Intl.DisplayNames([I.locale], { type: "language" }).of(code); if (n && n !== code) return n.charAt(0).toLocaleUpperCase(I.locale) + n.slice(1); } catch { /* unknown code */ }
    return String(code).toUpperCase();
  }

  // ------------------------------------------------------------ colours
  function parseColor(s) {
    s = String(s).trim();
    let m = /^#([0-9a-f]{6})$/i.exec(s);
    if (m) { const n = parseInt(m[1], 16); return [(n >> 16) & 255, (n >> 8) & 255, n & 255, 1]; }
    m = /^#([0-9a-f]{3})$/i.exec(s);
    if (m) { const h = m[1]; return [parseInt(h[0] + h[0], 16), parseInt(h[1] + h[1], 16), parseInt(h[2] + h[2], 16), 1]; }
    m = /^rgba?\(([^)]+)\)/.exec(s);
    if (m) { const p = m[1].split(",").map((x) => parseFloat(x)); return [p[0], p[1], p[2], p[3] ?? 1]; }
    return [128, 128, 128, 1];
  }
  const hex = (r, g, b) => "#" + [r, g, b].map((v) => Math.round(Math.max(0, Math.min(255, v))).toString(16).padStart(2, "0")).join("");
  /** `a` over `b` with weight w for a */
  function mix(a, b, w) {
    const x = parseColor(a), y = parseColor(b);
    return hex(x[0] * w + y[0] * (1 - w), x[1] * w + y[1] * (1 - w), x[2] * w + y[2] * (1 - w));
  }
  const css = (name, fb) => getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fb;
  /** Palette of the tab: sequential blue (5 steps), categories (no orange), orange for "new", grey for "other". */
  function palette(c) {
    const tk = c.tk;
    return {
      accent: tk.accent,
      orange: tk.cat[3],
      purple: tk.cat[2],
      grey: tk.cat[5],
      cat: [tk.cat[0], tk.cat[1], tk.cat[2], tk.cat[4], css("--chart-c7", "#c239b3")],
      seq: [0.2, 0.4, 0.6, 0.82, 1].map((a) => mix(tk.accent, tk.surface, a)),
      cell: tk.cell,
      surface: tk.surface,
      onAccent: css("--on-accent", "#ffffff"),
    };
  }
  /** Five (or fewer) value bins on the sequential scale: [{min, max, color, label}] */
  function bins(max, p) {
    if (!(max >= 1)) return [];
    let edges;
    if (max <= 5) edges = Array.from({ length: max }, (_, i) => [i + 1, i + 1]);
    else {
      const step = Math.ceil(max / 5);
      edges = [];
      for (let i = 0; i < 5; i++) { const lo = i * step + 1, hi = Math.min(max, (i + 1) * step); if (lo <= max) edges.push([lo, hi]); }
    }
    const k = edges.length;
    return edges.map(([lo, hi], i) => ({
      min: lo, max: hi, color: p.seq[k === 1 ? 3 : Math.round((i * 4) / (k - 1))], label: lo === hi ? String(lo) : `${lo}–${hi}`,
    }));
  }
  const binFor = (b, n) => b.find((x) => n >= x.min && n <= x.max);

  // ------------------------------------------------------------ shared option pieces
  function baseOpt(c, grid) {
    const tk = c.tk;
    return {
      animation: c.motion, animationDuration: 600, animationDurationUpdate: 250, animationEasing: "cubicOut",
      textStyle: { fontFamily: tk.font, color: tk.text2 },
      grid: Object.assign({ left: 4, right: 40, top: 4, bottom: 4, containLabel: true }, grid),
      tooltip: {
        trigger: "item", confine: true, backgroundColor: tk.pop, borderColor: tk.stroke, borderWidth: 1, padding: [8, 12],
        textStyle: { color: tk.text, fontSize: 12, fontFamily: tk.font },
        extraCssText: "border-radius:8px;box-shadow:0 4px 12px rgba(0,0,0,0.14);max-width:320px;white-space:normal;",
      },
    };
  }
  const axisTxt = (c, extra) => Object.assign({ color: c.tk.text2, fontSize: 12, fontFamily: c.tk.font }, extra);
  const dot = (color) => `<i style="display:inline-block;width:8px;height:8px;border-radius:50%;background:${color};flex:none"></i>`;
  const tipRow = (c, color, label, value) => `<div style="display:flex;align-items:center;gap:8px;margin-top:2px">${dot(color)}<span style="flex:1;color:${c.tk.text2}">${esc(label)}</span><b style="font-variant-numeric:tabular-nums">${esc(value)}</b></div>`;
  const tipLine = (color, text) => `<div style="display:flex;align-items:center;gap:8px;margin-top:2px">${dot(color)}<span>${esc(text)}</span></div>`;
  const tipHead = (c, text) => `<div style="font-weight:600;margin-bottom:4px;max-width:300px">${esc(text)}</div>`;

  /** Horizontal bars, top N. items: [{name, n, isNew?, tip?}]. New entities are orange (legend "previously citing" / `newLabel`). */
  function hbars(items, c, o = {}) {
    if (!items || !items.length) return null;
    const { tk, nf, t } = c;
    const p = palette(c);
    const hasNew = items.some((x) => x.isNew);
    const mk = (isNew) => items.map((x) => (!!x.isNew === isNew ? x.n : null));
    const lw = o.labelWidth || 150;
    const series = (name, color, data) => ({
      name, type: "bar", stack: "t", cursor: "default", data, barMaxWidth: 18, itemStyle: { color, borderRadius: [0, 3, 3, 0] },
      label: { show: true, position: "right", color: tk.text2, fontWeight: 600, formatter: (q) => (q.value == null ? "" : nf.format(q.value)) },
      emphasis: { itemStyle: { opacity: 0.85 } }, silent: false,
    });
    const option = Object.assign(baseOpt(c, { top: hasNew ? 26 : 4 }), {
      xAxis: { type: "value", show: false },
      yAxis: {
        type: "category", inverse: true, data: items.map((x) => x.name), axisTick: { show: false }, axisLine: { show: false },
        axisLabel: axisTxt(c, { width: lw, overflow: "truncate", formatter: (v) => trunc(v, 28) }),
      },
      series: [series(o.prevLabel || t("ci.lg_prev"), p.accent, mk(false))].concat(hasNew ? [series(o.newLabel || t("ci.lg_inst_new"), p.orange, mk(true))] : []),
    });
    if (hasNew) option.legend = { top: 0, right: 8, itemWidth: 10, itemHeight: 10, icon: "roundRect", textStyle: { color: tk.text2, fontSize: 12, fontFamily: tk.font } };
    option.tooltip.formatter = (q) => {
      const it = items[q.dataIndex];
      return tipHead(c, it.name) + tipLine(it.isNew ? p.orange : p.accent, t("ci.unit_works", { n: nf.format(it.n) }))
        + (it.isNew ? `<div style="color:${tk.text3};margin-top:2px">${esc(t("ci.tt_new"))}</div>` : "") + (it.tip ? `<div style="color:${tk.text3};margin-top:2px">${esc(it.tip)}</div>` : "");
    };
    return { option };
  }

  /** Countries per year: previously citing (accent) + first time that year (orange) on top. years: [{year,total,new}] */
  function yearsBars(years, c) {
    if (!years || !years.length) return null;
    const { tk, nf, t } = c;
    const p = palette(c);
    const option = Object.assign(baseOpt(c, { top: 28, right: 12, bottom: 4 }), {
      xAxis: { type: "category", data: years.map((y) => String(y.year)), axisTick: { show: false }, axisLine: { lineStyle: { color: tk.grid } }, axisLabel: axisTxt(c, { color: tk.text3 }) },
      yAxis: { type: "value", minInterval: 1, axisLine: { show: false }, axisTick: { show: false }, axisLabel: axisTxt(c, { color: tk.text3 }), splitLine: { lineStyle: { color: tk.grid } } },
      legend: { top: 0, right: 8, itemWidth: 10, itemHeight: 10, icon: "roundRect", textStyle: { color: tk.text2, fontSize: 12, fontFamily: tk.font } },
      series: [
        { name: t("ci.lg_prev"), type: "bar", stack: "y", cursor: "default", barMaxWidth: 48, data: years.map((y) => y.total - y.new), itemStyle: { color: p.accent } },
        { name: t("ci.lg_first"), type: "bar", stack: "y", cursor: "default", barMaxWidth: 48, data: years.map((y) => y.new), itemStyle: { color: p.orange, borderRadius: [3, 3, 0, 0] } },
      ],
    });
    option.tooltip.trigger = "axis";
    option.tooltip.axisPointer = { type: "shadow" };
    option.tooltip.formatter = (ps) => {
      const y = years[ps[0].dataIndex];
      return tipHead(c, String(y.year)) + tipLine(p.accent, t("ci.tt_year_countries", { n: nf.format(y.total) })) + (y.new ? tipLine(p.orange, t("ci.tt_year_first", { n: nf.format(y.new) })) : "");
    };
    return { option };
  }

  /** Top publishers as stacked horizontal bars by source type. items: [{name,n,journal,conference,repository,other}] */
  function publishers(items, c, o = {}) {
    if (!items || !items.length) return null;
    const { tk, nf, t } = c;
    const p = palette(c);
    const parts = [["journal", p.cat[0]], ["conference", p.cat[1]], ["repository", p.cat[2]], ["other", p.grey]];
    const lw = o.labelWidth || 150;
    const option = Object.assign(baseOpt(c, { top: 28 }), {
      xAxis: { type: "value", show: false },
      yAxis: { type: "category", inverse: true, data: items.map((x) => x.name), axisTick: { show: false }, axisLine: { show: false }, axisLabel: axisTxt(c, { width: lw, overflow: "truncate", formatter: (v) => trunc(v, 28) }) },
      legend: { top: 0, right: 8, itemWidth: 10, itemHeight: 10, icon: "roundRect", data: parts.map(([k]) => t("ci.st_" + k)), textStyle: { color: tk.text2, fontSize: 12, fontFamily: tk.font } },
      series: parts.map(([k, col]) => ({ name: t("ci.st_" + k), type: "bar", stack: "p", cursor: "default", barMaxWidth: 18, data: items.map((x) => x[k] || 0), itemStyle: { color: col }, emphasis: { itemStyle: { opacity: 0.85 } } }))
        .concat([{ name: "total", type: "bar", stack: "p", data: items.map(() => 0), silent: true, itemStyle: { color: "transparent" }, tooltip: { show: false }, label: { show: true, position: "right", color: tk.text2, fontWeight: 600, formatter: (q) => nf.format(items[q.dataIndex].n) } }]),
    });
    option.tooltip.trigger = "axis";
    option.tooltip.axisPointer = { type: "shadow" };
    option.tooltip.formatter = (ps) => {
      const it = items[ps[0].dataIndex];
      return tipHead(c, it.name) + parts.filter(([k]) => it[k] > 0).map(([k, col]) => tipRow(c, col, t("ci.st_" + k), nf.format(it[k]))).join("") + tipLine(tk.text3, t("ci.unit_works", { n: nf.format(it.n) }));
    };
    return { option };
  }

  /** Publication x field heatmap. rows: [{title, cells[]}], cols: names (the last is "other"). */
  function matrix(rows, cols, c) {
    if (!rows || !rows.length || !cols || !cols.length) return null;
    const { tk, nf, t } = c;
    const p = palette(c);
    const max = Math.max(1, ...rows.map((r) => Math.max(0, ...r.cells)));
    const b = bins(max, p);
    const data = [];
    rows.forEach((r, y) => r.cells.forEach((n, x) => {
      const bin = binFor(b, n);
      data.push({
        value: [x, y, n],
        itemStyle: { color: bin ? bin.color : p.cell, borderColor: p.surface, borderWidth: 2, borderRadius: 2 },
        label: { show: n > 0, color: bin && b.indexOf(bin) >= 3 ? p.onAccent : tk.text, fontSize: 11, fontWeight: 600, formatter: String(n) },
      });
    }));
    const option = Object.assign(baseOpt(c, { top: 4, right: 8, bottom: 4, left: 4 }), {
      xAxis: { type: "category", position: "top", data: cols, splitArea: { show: false }, axisTick: { show: false }, axisLine: { show: false }, axisLabel: axisTxt(c, { interval: 0, rotate: 32, fontSize: 11, formatter: (v) => trunc(v, 16) }) },
      yAxis: { type: "category", inverse: true, data: rows.map((r) => r.title), axisTick: { show: false }, axisLine: { show: false }, axisLabel: axisTxt(c, { width: c.labelWidth || 140, overflow: "truncate", formatter: (v) => trunc(v, 28) }) },
      series: [{ type: "heatmap", cursor: "default", data, label: { show: true }, emphasis: { itemStyle: { borderColor: tk.text, borderWidth: 1 } }, progressive: 0 }],
    });
    option.tooltip.formatter = (q) => {
      const [x, y, n] = q.value;
      return tipHead(c, rows[y].title) + tipLine(n > 0 ? p.accent : tk.text3, `${cols[x]}: ${t("ci.unit_works", { n: nf.format(n) })}`);
    };
    return { option };
  }

  /** World map; items: [{iso, n}]. The map "sctWorld" must be registered (registerWorld). */
  function worldMap(items, c) {
    const { tk, nf, t } = c;
    const p = palette(c);
    const max = Math.max(1, ...items.map((x) => x.n));
    const b = bins(max, p);
    const option = {
      animation: c.motion, animationDuration: 600, textStyle: { fontFamily: tk.font, color: tk.text2 },
      tooltip: {
        trigger: "item", confine: true, backgroundColor: tk.pop, borderColor: tk.stroke, borderWidth: 1, padding: [6, 10],
        textStyle: { color: tk.text, fontSize: 12, fontFamily: tk.font }, extraCssText: "border-radius:8px;box-shadow:0 4px 12px rgba(0,0,0,0.14);",
        formatter: (q) => esc(t("ci.tt_works", { name: countryName(q.name), n: nf.format(Number.isFinite(q.value) ? q.value : 0) })),
      },
      visualMap: {
        type: "piecewise", pieces: b.map(({ min, max: mx, color, label }) => ({ min, max: mx, color, label })), orient: "horizontal", left: 4, bottom: 2,
        itemWidth: 12, itemHeight: 10, itemGap: 8, itemSymbol: "rect", textStyle: { color: tk.text2, fontSize: 11, fontFamily: tk.font }, seriesIndex: 0,
      },
      series: [{
        type: "map", cursor: "default", map: "sctWorld", nameProperty: "iso", roam: false, selectedMode: false, left: 0, right: 0, top: 4, bottom: 22,
        itemStyle: { areaColor: p.cell, borderColor: p.surface, borderWidth: 0.6 },
        emphasis: { label: { show: false }, itemStyle: { areaColor: undefined, borderColor: tk.text, borderWidth: 1 } },
        label: { show: false },
        data: items.map((x) => ({ name: x.iso, value: x.n })),
      }],
    };
    return { option };
  }

  let worldPromise = null;
  /** Loads vendor/world.geo.json once and registers it with ECharts. Rejects when it cannot be read. */
  function registerWorld() {
    if (E.getMap && E.getMap("sctWorld")) return Promise.resolve();
    if (!worldPromise) {
      worldPromise = fetch("vendor/world.geo.json").then((r) => { if (!r.ok) throw new Error("world map " + r.status); return r.json(); })
        .then((g) => { E.registerMap("sctWorld", g); }).catch((e) => { worldPromise = null; throw e; });
    }
    return worldPromise;
  }

  // ------------------------------------------------------------ 100% stacked bar (DOM, prints cleanly)
  /** parts: [{label, n, color}] -> html of the bar and its legend with percentages directly under it. */
  function stackBar(parts, o = {}) {
    const total = sum(parts, (x) => x.n);
    if (!total) return "";
    const live = parts.filter((x) => x.n > 0);
    const seg = live.map((x) => {
      const p = (x.n / total) * 100;
      return `<i class="sb-seg" style="width:${p}%;background:${x.color}" title="${esc(`${x.label}: ${nfmt(x.n)} (${Math.round(p)}%)`)}"></i>`;
    }).join("");
    const leg = live.map((x) => `<li><i style="background:${x.color}"></i><span>${esc(x.label)}</span><b>${Math.round((x.n / total) * 100)}%</b></li>`).join("");
    return `<div class="sb${o.big ? " sb-big" : ""}" role="img" aria-label="${esc(live.map((x) => `${x.label} ${Math.round((x.n / total) * 100)}%`).join(", "))}">${seg}</div><ul class="sb-legend">${leg}</ul>`;
  }

  // ------------------------------------------------------------ treemap (own squarified layout: labels only where they fit)
  function squarify(vals, x, y, w, h) {
    const res = [];
    let rem = vals.slice();
    let total = sum(rem, (r) => r.v);
    while (rem.length && total > 0 && w > 0 && h > 0) {
      const side = Math.min(w, h);
      const k = (w * h) / total;
      const worst = (row, s) => {
        const a = s * k;
        let mx = 0, mn = Infinity;
        for (const r of row) { mx = Math.max(mx, r.v); mn = Math.min(mn, r.v); }
        return Math.max((side * side * mx * k) / (a * a), (a * a) / (side * side * mn * k));
      };
      let row = [rem[0]], s = rem[0].v, j = 1;
      while (j < rem.length) {
        const nr = row.concat(rem[j]);
        if (worst(nr, s + rem[j].v) <= worst(row, s)) { row = nr; s += rem[j].v; j++; } else break;
      }
      const a = s * k;
      if (w >= h) {
        const cw = a / h; let cy = y;
        for (const r of row) { const rh = (r.v * k) / cw; res.push({ ...r, x, y: cy, w: cw, h: rh }); cy += rh; }
        x += cw; w -= cw;
      } else {
        const rh = a / w; let cx = x;
        for (const r of row) { const rw = (r.v * k) / rh; res.push({ ...r, x: cx, y, w: rw, h: rh }); cx += rw; }
        y += rh; h -= rh;
      }
      total -= s; rem = rem.slice(row.length);
    }
    return res;
  }

  let measureCtx = null;
  function textWidth(text, font) {
    if (!measureCtx) measureCtx = document.createElement("canvas").getContext("2d");
    measureCtx.font = font;
    return measureCtx.measureText(text).width;
  }

  /** The text, or a prefix with an ellipsis when it is too wide; "" when not even a few letters fit. */
  function fitText(text, maxW, font) {
    if (maxW <= 0) return "";
    if (textWidth(text, font) <= maxW) return text;
    let lo = 0, hi = text.length;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (textWidth(text.slice(0, mid).trimEnd() + "…", font) <= maxW) lo = mid; else hi = mid - 1;
    }
    return lo >= 3 ? text.slice(0, lo).trimEnd() + "…" : "";
  }

  /** Domain > field tree with tiles under 4% merged into "other" per domain. Returns {nodes (domains with children), total}. */
  function treemapModel(domains) {
    const total = sum(domains, (d) => d.n);
    const out = [];
    for (const d of domains) {
      const kids = [];
      let other = 0; const otherNames = [];
      for (const f of d.fields) {
        if (total > 0 && f.n / total < 0.04) { other += f.n; otherNames.push(f.name); } else kids.push({ name: f.name, label: fieldName(f.name), v: f.n, topics: f.topics || [] });
      }
      if (other > 0) kids.push({ name: "", label: I.t("ci.other"), v: other, topics: [], merged: otherNames, other: true });
      kids.sort((a, b) => (a.other ? 1 : b.other ? -1 : b.v - a.v));
      out.push({ name: d.name, label: d.name ? fieldName(d.name) : I.t("ci.other"), v: d.n, kids });
    }
    out.sort((a, b) => b.v - a.v);
    return { nodes: out, total };
  }

  /** The treemap as an SVG string for a w x h box. Returns {svg, tiles} (tiles for the tooltip). */
  function treemapSvg(domains, w, h, c) {
    const model = treemapModel(domains);
    if (!model.total || w < 40 || h < 40) return { svg: "", tiles: [] };
    const p = palette(c);
    const font = `11px ${c.tk.font}`;
    const fontB = `600 12px ${c.tk.font}`;
    const ff = c.tk.font.replace(/"/g, "'");
    const cssF = `11px ${ff}`, cssB = `600 12px ${ff}`;
    const tiles = [];
    let svg = `<svg class="tm" xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}" role="img">`;
    const doms = squarify(model.nodes, 0, 0, w, h);
    doms.forEach((d, di) => {
      const base = d.name ? p.cat[di % p.cat.length] : p.grey;
      const head = d.h >= 44 && d.w >= 70 ? 20 : 0;
      svg += `<rect x="${d.x + 0.5}" y="${d.y + 0.5}" width="${Math.max(0, d.w - 1)}" height="${Math.max(0, d.h - 1)}" rx="3" fill="${base}"/>`;
      if (head) {
        const lab = d.label;
        if (textWidth(lab, fontB) <= d.w - 12) svg += `<text x="${d.x + 7}" y="${d.y + 14.5}" style="font:${cssB};fill:${p.onAccent}">${esc(lab)}</text>`;
      }
      const inner = { x: d.x + 2, y: d.y + head + (head ? 0 : 2), w: d.w - 4, h: d.h - head - (head ? 2 : 4) };
      const kids = squarify(d.kids, inner.x, inner.y, Math.max(0, inner.w), Math.max(0, inner.h));
      for (const k of kids) {
        const fill = mix(base, p.surface, 0.8);
        const idx = tiles.length;
        tiles.push({ domain: d.label, label: k.label, n: k.v, topics: k.topics, merged: k.merged, x: k.x, y: k.y, w: k.w, h: k.h });
        svg += `<g class="tm-tile" data-i="${idx}"><rect x="${k.x + 1}" y="${k.y + 1}" width="${Math.max(0, k.w - 2)}" height="${Math.max(0, k.h - 2)}" rx="2" fill="${fill}"/>`;
        const shown = k.h - 2 >= 18 ? fitText(k.label, k.w - 2 - 10, font) : "";
        if (shown) {
          svg += `<text x="${k.x + 6}" y="${k.y + 15}" style="font:${cssF};fill:${p.onAccent}">${esc(shown)}</text>`;
          const share = `${Math.round((k.v / model.total) * 100)}%`;
          if (k.h - 2 >= 34 && k.w - 2 >= textWidth(share, font) + 10) svg += `<text x="${k.x + 6}" y="${k.y + 29}" style="font:${cssF};fill:${p.onAccent};opacity:0.85">${esc(share)}</text>`;
        }
        svg += "</g>";
      }
    });
    return { svg: svg + "</svg>", tiles, total: model.total };
  }

  /** Draws the treemap into `el` (resizes with it) with a hover tooltip. Returns a dispose function. */
  function mountTreemap(el, domains, c, o = {}) {
    let tip = null;
    const draw = () => {
      const w = Math.floor(el.clientWidth), h = Math.floor(el.clientHeight);
      const r = treemapSvg(domains, w, h, o.ctx ? o.ctx() : c);
      el.innerHTML = r.svg;
      el._tiles = r.tiles;
      el._total = r.total || 0;
      if (!tip) { tip = document.createElement("div"); tip.className = "tm-tip"; tip.hidden = true; }
      el.appendChild(tip);
    };
    const show = (e) => {
      const g = e.target.closest(".tm-tile");
      if (!g) { tip.hidden = true; return; }
      const t = el._tiles[Number(g.dataset.i)];
      if (!t) return;
      const cc = o.ctx ? o.ctx() : c;
      let html = `<div class="tt-h">${esc(t.label)}</div><div class="tt-s">${esc(t.domain)}</div><div>${esc(cc.t("ci.tt_field", { n: cc.nf.format(t.n), p: Math.round((t.n / el._total) * 100) + "%" }))}</div>`;
      const extra = t.merged && t.merged.length ? t.merged.map(fieldName) : t.topics.map((x) => x.name);
      if (extra.length) html += `<div class="tt-s">${esc(t.merged ? "" : cc.t("ci.topics") + ": ")}${esc(extra.slice(0, 3).join(", "))}${t.merged && t.merged.length > 3 ? "…" : ""}</div>`;
      tip.innerHTML = html;
      tip.hidden = false;
      const b = el.getBoundingClientRect();
      let x = e.clientX - b.left + 12, y = e.clientY - b.top + 12;
      const tw = tip.offsetWidth, th = tip.offsetHeight;
      if (x + tw > b.width) x = Math.max(0, e.clientX - b.left - tw - 12);
      if (y + th > b.height) y = Math.max(0, e.clientY - b.top - th - 12);
      tip.style.transform = `translate(${x}px, ${y}px)`;
    };
    el.addEventListener("mousemove", show);
    el.addEventListener("mouseleave", () => { if (tip) tip.hidden = true; });
    let raf = 0;
    const ro = new ResizeObserver(() => { cancelAnimationFrame(raf); raf = requestAnimationFrame(draw); });
    ro.observe(el);
    draw();
    return () => { ro.disconnect(); cancelAnimationFrame(raf); el.removeEventListener("mousemove", show); el.innerHTML = ""; };
  }

  // ------------------------------------------------------------ captions and answer sentences
  /** Missing-data caption. parts: [{what: "ci.w_country", missing}], n = the filtered count. */
  function caption(n, parts) {
    const t = I.t;
    const some = parts.filter((p) => p.missing > 0);
    if (!some.length) return t("ci.cap_complete", { n: nfmt(n), list: joinAnd(parts.map((p) => t(p.what))) });
    return t("ci.cap_missing", { n: nfmt(n), list: some.map((p) => t("ci.cap_item", { x: locNum(p.missing), what: t(p.what) })).join(", ") });
  }
  /** t() with the parameters escaped; the template itself may carry <b>. */
  const tb = (key, params) => I.t(key, Object.fromEntries(Object.entries(params).map(([k, v]) => [k, k.endsWith("_raw") ? v : esc(v)])));

  const sentences = {
    countries(rep) {
      const items = rep.items;
      if (!items.length) return "";
      const den = rep.cov.n - rep.cov.missing;
      const names = items.slice(0, 2).map((x) => countryName(x.key));
      const newc = rep.new_count > 0 ? I.t("ci.a_countries_new", { m: nfmt(rep.new_count) }) : "";
      if (items.length === 1) return I.t("ci.a_countries_1", { names: esc(names[0]), newc });
      const share = pct(items[0].n + items[1].n, den);
      return I.t("ci.a_countries", { p: esc(pctPoss(share)), names: esc(joinFrom(names)), k: nfmt(rep.k), newc });
    },
    institutions(rep) {
      const it = rep.items[0];
      if (!it) return "";
      if (rep.new_count > 0) return tb("ci.a_institutions", { name: it.name, n: nfmt(it.n), k: nfmt(rep.k), m: numPoss(rep.new_count) });
      return tb("ci.a_institutions_0", { name: it.name, n: nfmt(it.n), k: nfmt(rep.k) });
    },
    venues(v) {
      const parts = [];
      const st = Object.fromEntries(v.source_types.map((x) => [x.name, x.n]));
      const stTotal = sum(v.source_types, (x) => x.n);
      if (stTotal > 0) parts.push(tb("ci.a_venues_types", { pj: pctPoss(pct(st.journal || 0, stTotal)), pc: pctPoss(pct(st.conference || 0, stTotal)), pr: pctPoss(pct(st.repository || 0, stTotal)) }));
      const pubs = v.publishers.slice(0, 2);
      const den = v.cov_publisher.n - v.cov_publisher.missing;
      const plain = (v) => (isTr() ? `%${v}` : `${v}%`);
      if (pubs.length === 2 && den > 0) parts.push(I.t("ci.a_venues_pub", { names: esc(pubs.map((x) => x.name).join(` ${I.t("ci.and")} `)), p: esc(plain(pct(pubs[0].n + pubs[1].n, den))) }));
      else if (pubs.length === 1 && den > 0) parts.push(I.t("ci.a_venues_pub", { names: esc(pubs[0].name), p: esc(plain(pct(pubs[0].n, den))) }));
      return parts.join("; ") + (parts.length ? "." : "");
    },
    fields(f) {
      const top = f.fields;
      if (!top.length) return "";
      const n = (x) => esc(fieldName(x.name));
      if (top.length === 1) return I.t("ci.a_fields_1", { f1: n(top[0]) });
      const third = top[2] ? I.t("ci.a_fields_third", { f3: n(top[2]) }) : "";
      return I.t("ci.a_fields", { f1: n(top[0]), f2: n(top[1]), third });
    },
    types(ty) {
      const parts = [];
      const tt = sum(ty.types, (x) => x.n);
      if (tt > 0) parts.push(tb("ci.a_types_type", { p: pctPoss(pct(ty.types.filter((x) => typeKey(x.name) === "article").reduce((s, x) => s + x.n, 0), tt)) }));
      const ot = sum(ty.oa, (x) => x.n);
      if (ot > 0) parts.push(tb("ci.a_types_oa", { p: pctPoss(pct(ty.oa.filter((x) => oaKey(x.name) !== "closed").reduce((s, x) => s + x.n, 0), ot)) }));
      const lt = sum(ty.languages, (x) => x.n);
      if (lt > 0) parts.push(tb("ci.a_types_lang", { p: pctPoss(pct(ty.languages.filter((x) => x.name.toLowerCase() === "en").reduce((s, x) => s + x.n, 0), lt)) }));
      return parts.length ? parts[0] + (parts[1] ? "; " + parts[1] : "") + (parts[2] ? ", " + parts[2] : "") + "." : "";
    },
    who(w) {
      const s = w.split;
      return tb("ci.a_who", { p1: pctPoss(pct(s.self_n, s.total)), p2: pctPoss(pct(s.coauthor, s.total)), p3: pctPoss(pct(s.external, s.total)) });
    },
    influential(inf) {
      if (inf.max == null) return "";
      return inf.over > 0 ? tb("ci.a_infl", { k: nfmt(inf.over), n: nfmt(inf.max) }) : tb("ci.a_infl_0", { n: nfmt(inf.max) });
    },
  };

  /** The three 100% bars of "Publication types": [{title, parts}] (top 5 types, five OA classes, top 3 languages; the rest in grey "other"). */
  function typeBars(ty, c) {
    const p = palette(c);
    const t = I.t;
    const group = (list, limit, label, colors) => {
      const top = list.slice(0, limit);
      const rest = sum(list.slice(limit), (x) => x.n);
      const parts = top.map((x, i) => ({ label: label(x.name), n: x.n, color: colors[i % colors.length] }));
      if (rest > 0) parts.push({ label: t("ci.other"), n: rest, color: p.grey });
      return parts;
    };
    // types: fold raw keys that share a label (conference papers) before ranking
    const folded = new Map();
    for (const x of ty.types) { const k = typeKey(x.name) || x.name; folded.set(k, (folded.get(k) || 0) + x.n); }
    const types = Array.from(folded, ([name, n]) => ({ name, n })).sort((a, b) => b.n - a.n);
    const oaMap = new Map();
    for (const x of ty.oa) { const k = oaKey(x.name); oaMap.set(k, (oaMap.get(k) || 0) + x.n); }
    const oaParts = ["gold", "green", "hybrid", "bronze", "closed"].filter((k) => oaMap.get(k)).map((k, i) => ({ label: t("ci.oa_" + k), n: oaMap.get(k), color: k === "closed" ? p.grey : p.cat[i % p.cat.length] }));
    const oaOther = sum(Array.from(oaMap).filter(([k]) => !["gold", "green", "hybrid", "bronze", "closed"].includes(k)), ([, n]) => n);
    if (oaOther > 0) oaParts.push({ label: t("ci.other"), n: oaOther, color: p.grey });
    return [
      { key: "type", title: t("ci.t_type"), parts: group(types, 5, (k) => typeName(k), p.cat), n: sum(types, (x) => x.n) },
      { key: "oa", title: t("ci.t_oa"), parts: oaParts, n: sum(oaParts, (x) => x.n) },
      { key: "lang", title: t("ci.t_lang"), parts: group(ty.languages, 3, (k) => langName(k), p.cat), n: sum(ty.languages, (x) => x.n) },
    ];
  }

  // ------------------------------------------------------------ OJP (Open Journal Prestige) gauge
  // Rows carry { ojp, ojp_state, ojp_field, ojp_n }; the data edition and thresholds come from get_ojp_meta (set once by the page).
  const ojp = {
    meta: null,
    setMeta(m) { ojp.meta = m || null; },
    /** Score 0..100 -> colour step 0..9 (steps by tens, the last one holds 90-100). */
    step: (score) => Math.max(0, Math.min(9, Math.floor(score / 10))),
    vars() {
      const m = ojp.meta || {};
      return { edition: m.edition ?? "", cite_year: m.cite_year ?? "", from: m.window_from ?? "", to: m.window_to ?? "", min: m.min_pubs ?? "", min_field: m.min_field ?? "",
        journals: m.journals != null ? nfmt(m.journals) : "", fields: m.fields ?? "", dataset_version: m.dataset_version ?? "", dataset_doi: m.dataset_doi ?? "" };
    },
    lines(row) {
      const t = I.t, v = ojp.vars();
      if (!row) return [];
      if (row.ojp_state === "ranked" && row.ojp != null) {
        return [t("ojp.tip_score", { score: row.ojp }), t("ojp.tip_field", { field: fieldName(row.ojp_field), n: nfmt(row.ojp_n || 0) }), t("ojp.tip_rank", v), t("ojp.tip_not")];
      }
      if (["few", "unscored", "absent"].includes(row.ojp_state)) return [t("ojp.none_" + row.ojp_state, v), t("ojp.tip_not")];
      return [];
    },
    tip: (row) => ojp.lines(row).join("\n"),
    /** HTML of the gauge; "" when the work is not in a journal. */
    badge(row) {
      if (!row) return "";
      const ln = ojp.lines(row);
      if (!ln.length) return "";
      const title = esc(ln.join("\n")), aria = esc(ln.map((s) => s.replace(/\.$/, "")).join(". "));
      if (row.ojp_state === "ranked" && row.ojp != null) return `<span class="ojp ojp-b${ojp.step(row.ojp)}" role="img" aria-label="${aria}" title="${title}">${esc(row.ojp)}</span>`;
      if (row.ojp_state === "absent") return `<span class="ojp-dash" role="img" aria-label="${aria}" title="${title}">NA</span>`;
      return `<span class="ojp ojp-none" role="img" aria-label="${aria}" title="${title}">NA</span>`;
    },
    /** Example badge for the Help page (large, with the meter along the bottom edge). */
    sample: (score) => `<span class="ojp ojp-lg ojp-b${ojp.step(score)}" style="--ojp-v:${score}" role="img" aria-label="OJP ${score} / 100">${score}</span>`,
    range: (i) => (i >= 9 ? "90–100" : `${i * 10}–${i * 10 + 9}`),
    /** Sentence of the Analysis card and the report: coverage and the fullest range only (no average on purpose). */
    answer: (o) => (o && o.ranked ? I.t("ci.ojp_answer", { n: nfmt(o.n), ranked: nfmt(o.ranked), range: ojp.range(o.mode), k: nfmt(o.hist[o.mode]) }) : ""),
  };

  /** Citing works per OJP step (10 bars filled with the gauge colours). hist: [n x 10] */
  function ojpHist(hist, c) {
    const total = sum(hist);
    if (!total) return null;
    const { tk, nf, t } = c;
    const labels = hist.map((_, i) => ojp.range(i));
    const option = Object.assign(baseOpt(c, { top: 22, right: 12, bottom: 4 }), {
      xAxis: { type: "category", data: labels, axisTick: { show: false }, axisLine: { lineStyle: { color: tk.grid } }, axisLabel: axisTxt(c, { color: tk.text3, interval: 0, fontSize: 11 }) },
      yAxis: { type: "value", minInterval: 1, axisLine: { show: false }, axisTick: { show: false }, axisLabel: axisTxt(c, { color: tk.text3 }), splitLine: { lineStyle: { color: tk.grid } } },
      series: [{
        type: "bar", cursor: "default", barMaxWidth: 34,
        data: hist.map((v, i) => ({ value: v, itemStyle: { color: css("--ojp-" + i, "#cccccc"), borderColor: tk.text3, borderWidth: 1, borderRadius: [3, 3, 0, 0] } })),
        label: { show: true, position: "top", color: tk.text2, fontWeight: 600, formatter: (q) => (q.value ? nf.format(q.value) : "") },
        emphasis: { itemStyle: { opacity: 0.85 } },
      }],
    });
    option.tooltip.formatter = (q) => tipHead(c, t("ci.ojp_tt", { range: labels[q.dataIndex], k: nf.format(hist[q.dataIndex]), pct: pct(hist[q.dataIndex], total) }));
    return { option };
  }

  window.CitersLib = {
    esc, trunc, pct, sum, palette, bins, mix, ojp, ojpHist, hbars, yearsBars, publishers, matrix, worldMap, registerWorld, stackBar,
    treemapSvg, mountTreemap, treemapModel, caption, sentences, typeBars,
    grammar: { possSuffix, pctPoss, locNum, numPoss, abl, joinFrom },
    names: { countryName, fieldName, typeName, typeKey, oaKey, langName },
  };
})();
