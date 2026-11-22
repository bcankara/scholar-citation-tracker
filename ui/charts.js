"use strict";
/* Charts: Apache ECharts (ui/vendor/echarts.min.js, Apache-2.0) set up in the Fluent look.
   One set of builders feeds both the app window and the PDF report (report.js), so the numbers match.
   Every builder takes the data from the backend (`get_analysis` / `get_trend`) and a context from
   `Charts.ctx()` and returns `{ option, click?, yFor?, meta? }`, or null when there is nothing to draw.
   Colours come from the CSS tokens of the page (light / dark), the language from window.I18N. */
(() => {
  const E = window.echarts;
  const DAY = 86400e3;
  const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
  const pad2 = (n) => String(n).padStart(2, "0");
  const shortText = (s, n) => (s.length > n ? s.slice(0, n - 1).trimEnd() + "…" : s);

  // ------------------------------------------------------------ tokens and context
  function tokens() {
    const cs = getComputedStyle(document.documentElement);
    const g = (n, fb) => cs.getPropertyValue(n).trim() || fb;
    return {
      font: g("--font-text", '"Segoe UI", sans-serif'),
      accent: g("--accent", "#0067c0"),
      text: g("--text", "#1a1a1a"),
      text2: g("--text-2", "rgba(0,0,0,0.62)"),
      text3: g("--text-3", "rgba(0,0,0,0.45)"),
      grid: g("--grid", "rgba(0,0,0,0.07)"),
      pop: g("--pop-bg", "#fcfcfc"),
      stroke: g("--layer-stroke", "rgba(0,0,0,0.06)"),
      pos: g("--pos", "#0f7b0f"),
      neg: g("--neg", "#c42b1c"),
      hist: g("--chart-hist", "#6a9fd0"),
      fc: g("--chart-fc", "#8764b8"),
      surface: g("--chart-surface", "#fafafa"),
      cell: g("--chart-cell", "#ececec"),
      cat: [1, 2, 3, 4, 5, 6].map((i) => g("--chart-c" + i, ["#0067c0", "#038387", "#8764b8", "#ca5010", "#107c10", "#8a8886"][i - 1])),
    };
  }

  /** `#rrggbb` or `rgb[a](...)` with another alpha. */
  function alpha(c, a) {
    c = String(c).trim();
    let m = /^#([0-9a-f]{6})$/i.exec(c);
    if (m) {
      const n = parseInt(m[1], 16);
      return `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${a})`;
    }
    m = /^rgba?\(([^)]+)\)/.exec(c);
    if (m) {
      const p = m[1].split(",").map((s) => s.trim());
      return `rgba(${p[0]},${p[1]},${p[2]},${a})`;
    }
    return c;
  }

  /** Context of one drawing: language, number / date formats, colours, motion. `o.print`: the PDF (no animation). */
  function ctx(o = {}) {
    const loc = window.I18N.locale;
    return Object.assign({
      t: window.I18N.t,
      loc,
      lang: window.I18N.lang,
      nf: new Intl.NumberFormat(loc),
      tk: tokens(),
      motion: !o.print && !matchMedia("(prefers-reduced-motion: reduce)").matches,
      now: Date.now(),
      range: { from: null, to: null },
    }, o);
  }

  const signed = (c, n) => (n > 0 ? "+" + c.nf.format(n) : n < 0 ? "−" + c.nf.format(-n) : "0");
  const dateLong = (c, ms) => new Date(ms).toLocaleDateString(c.loc, { day: "numeric", month: "long", year: "numeric" });
  const dateTime = (c, ms) => new Date(ms).toLocaleString(c.loc, { day: "numeric", month: "long", year: "numeric", hour: "2-digit", minute: "2-digit" });
  const keyMs = (key) => { const [y, m = 1, d = 1] = key.split("-").map(Number); return Date.UTC(y, m - 1, d); };
  const ymd = (ms) => { const d = new Date(ms); return `${d.getUTCFullYear()}-${pad2(d.getUTCMonth() + 1)}-${pad2(d.getUTCDate())}`; };
  const utcLong = (c, key) => {
    const parts = key.split("-").map(Number);
    const opt = parts.length === 1 ? { year: "numeric" } : parts.length === 2 ? { month: "long", year: "numeric" } : { day: "numeric", month: "long", year: "numeric" };
    return new Date(keyMs(key)).toLocaleDateString(c.loc, { ...opt, timeZone: "UTC" });
  };
  const monthShort = (c, key, withYear) => new Date(keyMs(key)).toLocaleDateString(c.loc, { month: "short", ...(withYear ? { year: "2-digit" } : {}), timeZone: "UTC" });

  // ------------------------------------------------------------ pieces shared by the builders
  function niceStep(raw) {
    const mag = Math.pow(10, Math.floor(Math.log10(raw || 1)));
    const n = raw / mag;
    return (n <= 1 ? 1 : n <= 2 ? 2 : n <= 5 ? 5 : 10) * mag;
  }
  /** Round axis bounds around [min, max] with about four intervals. */
  function niceBounds(min, max, intervals = 4) {
    if (!(max > min)) { max = min + 1; }
    const step = niceStep((max - min) / intervals);
    return { min: Math.floor(min / step) * step, max: Math.ceil(max / step) * step, interval: step };
  }

  function base(c, opt = {}) {
    const tk = c.tk;
    return {
      animation: c.motion,
      animationDuration: 800,
      animationDurationUpdate: 300,
      animationEasing: "cubicOut",
      textStyle: { fontFamily: tk.font, color: tk.text2 },
      grid: Object.assign({ left: 4, right: 16, top: 12, bottom: 4, containLabel: true }, opt.grid),
      tooltip: Object.assign({
        trigger: "axis",
        confine: true,
        backgroundColor: tk.pop,
        borderColor: tk.stroke,
        borderWidth: 1,
        padding: [8, 12],
        textStyle: { color: tk.text, fontSize: 12, fontFamily: tk.font },
        extraCssText: "border-radius:8px;box-shadow:0 4px 12px rgba(0,0,0,0.14);",
        axisPointer: { type: "line", lineStyle: { color: tk.text3, type: "dashed" } },
      }, opt.tooltip),
    };
  }
  const axisText = (c) => ({ color: c.tk.text3, fontSize: 12, fontFamily: c.tk.font });
  const valueAxis = (c, extra) => Object.assign({
    type: "value",
    axisLine: { show: false },
    axisTick: { show: false },
    axisLabel: Object.assign({ formatter: (v) => c.nf.format(v) }, axisText(c)),
    splitLine: { lineStyle: { color: c.tk.grid } },
    minInterval: 1,
  }, extra);
  const dot = (color) => `<i style="display:inline-block;width:8px;height:8px;border-radius:50%;background:${color};flex:none"></i>`;
  const tipHead = (c, text) => `<div style="color:${c.tk.text2};margin-bottom:4px">${esc(text)}</div>`;
  const tipRow = (c, color, label, value, note) =>
    `<div style="display:flex;align-items:center;gap:8px;margin-top:2px">${dot(color)}<span style="flex:1;color:${c.tk.text2}">${esc(label)}</span><b style="font-size:14px;font-variant-numeric:tabular-nums">${esc(value)}</b></div>`
    + (note ? `<div style="margin:0 0 2px 16px;color:${c.tk.text3};font-size:11px">${note}</div>` : "");
  const deltaHtml = (c, n) => `<span style="color:${n > 0 ? c.tk.pos : n < 0 ? c.tk.neg : c.tk.text3};font-weight:600">${esc(signed(c, n))}</span>`;
  const areaGradient = (color, a0, a1 = 0) => new E.graphic.LinearGradient(0, 0, 0, 1, [{ offset: 0, color: alpha(color, a0) }, { offset: 1, color: alpha(color, a1) }]);

  // ------------------------------------------------------------ periods (bars per day / month / year)
  /** Continuous periods (zeros filled) from the first key to `endMs`. */
  function buildPeriods(points, gran, endMs) {
    if (!points.length) return [];
    const map = new Map(points);
    const first = keyMs(points[0][0]);
    const end = Math.max(keyMs(points[points.length - 1][0]), endMs);
    const out = [];
    const push = (key, t, t2) => out.push({ key, t, t2, v: map.get(key) || 0 });
    if (gran === "day") {
      for (let t = first; t <= end; t += DAY) push(ymd(t), t, t + DAY);
    } else if (gran === "month") {
      let y = new Date(first).getUTCFullYear(), m = new Date(first).getUTCMonth();
      const ey = new Date(end).getUTCFullYear(), em = new Date(end).getUTCMonth();
      while (y < ey || (y === ey && m <= em)) {
        push(`${y}-${pad2(m + 1)}`, Date.UTC(y, m, 1), Date.UTC(y, m + 1, 1));
        if (++m > 11) { m = 0; y++; }
      }
    } else {
      for (let y = new Date(first).getUTCFullYear(); y <= new Date(end).getUTCFullYear(); y++) push(String(y), Date.UTC(y, 0, 1), Date.UTC(y + 1, 0, 1));
    }
    return out;
  }
  /** Keeps the periods that overlap the range. */
  function clipPeriods(periods, range) {
    const { from, to } = range || {};
    return periods.filter((p) => (from == null || p.t2 > from) && (to == null || p.t <= to));
  }
  const todayUtc = (c) => { const d = new Date(c.now); return Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()); };

  /** Which data feeds the period chart: the citing works (OpenAlex), or Scholar's own yearly histogram. */
  function periodSource(data, gran) {
    const h = { day: data.history_day, month: data.history_month, year: data.history_year }[gran];
    const hasHist = data.history_year && data.history_year.total > 0;
    if (hasHist) return { source: "openalex", points: h.points, excluded: h.excluded };
    if (gran === "year") return { source: "scholar", points: (data.yearly || []).map(([y, n]) => [String(y), n]), excluded: 0 };
    return { source: "none", points: [], excluded: 0 };
  }

  function periodLabel(c, p, gran, long) {
    const d = new Date(p.t);
    if (gran === "year") return String(d.getUTCFullYear());
    if (gran === "month") return d.toLocaleDateString(c.loc, { month: long ? "long" : "short", year: long ? "numeric" : "2-digit", timeZone: "UTC" });
    return d.toLocaleDateString(c.loc, { day: "numeric", month: long ? "long" : "short", year: long ? "numeric" : "2-digit", timeZone: "UTC" });
  }

  /** Light diagonal hatching for "OpenAlex has not indexed this yet" (a flat tint in the PDF). */
  function hatch(c) {
    if (c.print) return alpha(c.tk.text3, 0.14);
    const cv = document.createElement("canvas");
    cv.width = cv.height = 8;
    const g = cv.getContext("2d");
    g.strokeStyle = alpha(c.tk.text3, 0.55);
    g.lineWidth = 1;
    g.beginPath();
    g.moveTo(0, 8);
    g.lineTo(8, 0);
    g.stroke();
    return { image: cv, repeat: "repeat" };
  }
  /** First instant (ms) after OpenAlex's newest day-dated citing work, at the given grouping; null without data. */
  function tailStart(oaLatest, gran) {
    if (!oaLatest || oaLatest.length < 10) return null;
    const [y, m, d] = oaLatest.split("-").map(Number);
    if (gran === "day") return Date.UTC(y, m - 1, d) + DAY;
    if (gran === "month") return Date.UTC(y, m, 1);
    return Date.UTC(y + 1, 0, 1);
  }

  // ------------------------------------------------------------ builders
  /** "How are my citations growing?" One cumulative line (history scaled to the first Scholar value, then
   *  the Scholar checks), a forecast to the end of the year with a range, an optional goal line. */
  function trend(data, c) {
    const tr = data && data.trend;
    if (!tr || !tr.has_data) return null;
    const { tk, t, nf } = c;
    const hist = tr.hist.map((p) => [p[0], p[1]]);
    const snaps = tr.snaps.map((p) => [p[0], p[1]]);
    const fc = tr.forecast;
    const to = c.range.to;
    const fcShown = !!fc && c.forecast !== false && (to == null || to >= c.now - 2 * DAY);
    const firstT = hist.length ? hist[0][0] : snaps[0][0];
    const lastT = snaps[snaps.length - 1][0];
    const endT = fcShown ? Math.max(fc.end_t, lastT) : lastT;
    let vFrom = c.range.from == null ? firstT : Math.max(c.range.from, firstT);
    let vTo = to == null || fcShown ? endT : Math.min(to, endT);
    if (!(vTo > vFrom)) { vFrom = firstT; vTo = Math.max(endT, firstT + DAY); }
    const line = hist.filter((p) => !snaps.length || p[0] < snaps[0][0]).concat(snaps);
    const at = (x) => {
      if (x <= line[0][0]) return line[0][1];
      if (x >= line[line.length - 1][0]) return line[line.length - 1][1];
      let i = 1;
      while (line[i][0] < x) i++;
      const [a, b] = [line[i - 1], line[i]];
      return a[1] + ((b[1] - a[1]) * (x - a[0])) / (b[0] - a[0] || 1);
    };
    const goal = tr.goal;
    let goalDrawn = false;
    const yFor = (a, b) => {
      const ys = [at(a), at(b)];
      for (const p of line) if (p[0] > a && p[0] < b) ys.push(p[1]);
      if (fcShown && a < fc.end_t) {
        const f = (v) => fc.start_v + ((v - fc.start_v) * (Math.min(b, fc.end_t) - fc.start_t)) / (fc.end_t - fc.start_t || 1);
        if (b > fc.start_t) ys.push(f(fc.high), f(fc.low));
      }
      const lo = Math.min(...ys), hi = Math.max(...ys);
      goalDrawn = !!goal && c.goal !== false && goal <= hi * 1.6 && goal >= lo * 0.2;
      if (goalDrawn) ys.push(goal);
      const mn = Math.min(...ys), mx = Math.max(...ys);
      const pad = (mx - mn) * 0.06 || 1;
      return niceBounds(Math.max(0, mn - pad), mx + pad);
    };
    const yb = yFor(vFrom, vTo);
    const histName = t("trend.s_hist");
    const snapName = t("trend.s_scholar");
    const fcName = t("trend.s_forecast");
    const merge = tr.merge;
    const marks = [];
    if (hist.length && merge && !c.mini) {
      marks.push({ xAxis: merge.t, label: { formatter: t("trend.merge_label"), color: tk.text2, position: "insideEndTop", fontSize: 11 }, lineStyle: { color: tk.text3, type: "dashed", width: 1 } });
    }
    if (goalDrawn && !c.mini) {
      marks.push({ yAxis: goal, label: { formatter: t("trend.goal_label", { n: nf.format(goal) }), color: tk.pos, position: "insideStartTop", fontSize: 11 }, lineStyle: { color: tk.pos, type: "dotted", width: 1.5 } });
    }
    const series = [];
    if (hist.length) {
      series.push({
        id: "hist", name: histName, type: "line", data: hist, showSymbol: false, symbol: "circle", symbolSize: 6, z: 2,
        lineStyle: { width: 2, color: tk.hist }, itemStyle: { color: tk.hist }, areaStyle: { color: areaGradient(tk.hist, 0.12) },
        emphasis: { focus: "none" },
      });
    }
    series.push({
      id: "snap", name: snapName, type: "line", data: snaps, showSymbol: false, symbol: "circle", symbolSize: 7, z: 3,
      lineStyle: { width: 2.5, color: tk.accent }, itemStyle: { color: tk.accent }, areaStyle: { color: areaGradient(tk.accent, 0.2) },
      emphasis: { focus: "none" },
      markLine: marks.length ? { silent: true, symbol: "none", data: marks, animation: false } : undefined,
    });
    if (fcShown) {
      series.push({
        id: "fc", name: fcName, type: "line", z: 4, data: [[fc.start_t, fc.start_v], [fc.end_t, fc.mid]], showSymbol: false,
        lineStyle: { width: 2.5, color: tk.fc, type: "dashed" }, itemStyle: { color: tk.fc }, emphasis: { focus: "none" },
        markPoint: c.mini ? undefined : {
          silent: true, symbol: "circle", symbolSize: 9, itemStyle: { color: tk.fc },
          data: [{
            coord: [fc.end_t, fc.mid],
            label: {
              show: true, position: "left", offset: [-4, -14], color: tk.fc, fontWeight: 600, fontSize: 12,
              formatter: t("trend.fc_end", { n: nf.format(fc.mid), month: new Date(fc.end_t).toLocaleDateString(c.loc, { month: "long", year: "numeric", timeZone: "UTC" }) }),
            },
          }],
        },
      });
      series.push({
        id: "band", name: fcName, type: "custom", z: 1, silent: true, data: [[fc.start_t, fc.start_v, fc.end_t, fc.low, fc.high]],
        tooltip: { show: false },
        renderItem: (params, api) => {
          const a = api.coord([api.value(0), api.value(1)]);
          const hi = api.coord([api.value(2), api.value(4)]);
          const lo = api.coord([api.value(2), api.value(3)]);
          return { type: "polygon", shape: { points: [a, hi, lo] }, style: { fill: alpha(tk.fc, 0.16) } };
        },
        encode: { x: [0, 2], y: [1, 3, 4] },
      });
    }
    const names = [histName, snapName].filter((n, i) => (i === 0 ? hist.length : true));
    if (fcShown) names.push(fcName);
    const zoom = !c.mini && !c.print;
    const option = Object.assign(base(c, { grid: { left: 4, right: c.mini ? 8 : 18, top: c.mini ? 8 : 44, bottom: zoom ? 44 : 4 } }), {
      legend: c.mini ? { show: false } : {
        top: 0, left: 0, data: names, icon: "roundRect", itemWidth: 16, itemHeight: 3, itemGap: 18,
        textStyle: { color: tk.text2, fontSize: 12, fontFamily: tk.font },
        inactiveColor: tk.text3,
      },
      xAxis: {
        type: "time", min: zoom ? firstT : vFrom, max: zoom ? endT : vTo,
        axisLine: { lineStyle: { color: tk.grid } }, axisTick: { show: false },
        axisLabel: Object.assign({ hideOverlap: true, formatter: { year: "{yyyy}", month: "{MMM}", day: "{d} {MMM}" } }, axisText(c)),
        splitLine: { show: false },
        axisPointer: { snap: true },
      },
      yAxis: valueAxis(c, { min: yb.min, max: yb.max, interval: yb.interval, minInterval: undefined }),
      dataZoom: zoom ? [
        { type: "inside", filterMode: "none", startValue: vFrom, endValue: vTo, zoomOnMouseWheel: "ctrl" },
        { type: "slider", filterMode: "none", startValue: vFrom, endValue: vTo, height: 20, bottom: 6, borderColor: "transparent", backgroundColor: tk.grid, fillerColor: alpha(tk.accent, 0.15), handleSize: 14, textStyle: { color: tk.text3 }, dataBackground: { lineStyle: { color: tk.text3, opacity: 0.4 }, areaStyle: { color: tk.grid } } },
      ] : undefined,
      series,
    });
    option.tooltip.axisPointer = { type: "cross", snap: true, lineStyle: { color: tk.text3, type: "dashed" }, crossStyle: { color: tk.text3, type: "dashed" }, label: { show: false } };
    option.tooltip.formatter = (params) => {
      const ps = params.filter((p) => p.seriesId !== "band" && p.value);
      if (!ps.length) return "";
      const x = params[0].axisValue;
      const near = ps.filter((p) => Math.abs(p.value[0] - x) < 3600e3);
      const use = near.length ? near : [ps[0]];
      let html = "";
      for (const p of use) {
        const [px, py] = p.value;
        if (p.seriesId === "hist") {
          const arr = hist, prev = arr[p.dataIndex - 1];
          html += (html ? "" : tipHead(c, utcLong(c, ymd(px))))
            + tipRow(c, tk.hist, histName, nf.format(Math.round(py)),
              (prev ? deltaHtml(c, Math.round(py - prev[1])) + " · " : "") + esc(t("trend.src_hist")) + (merge ? "<br>" + esc(t("trend.scaled", { k: (merge.k).toFixed(2) })) : ""));
        } else if (p.seriesId === "snap") {
          const prev = snaps[p.dataIndex - 1];
          html += (html ? "" : tipHead(c, dateTime(c, px)))
            + tipRow(c, tk.accent, snapName, nf.format(py), (prev ? deltaHtml(c, py - prev[1]) + " · " : "") + esc(t("trend.src_scholar")));
        } else if (p.seriesId === "fc") {
          const isEnd = p.dataIndex === 1;
          html += (html ? "" : tipHead(c, dateLong(c, px)))
            + tipRow(c, tk.fc, fcName, "≈ " + nf.format(py), isEnd ? esc(t("trend.range", { low: nf.format(fc.low), high: nf.format(fc.high) })) + "<br>" + esc(t("trend.src_fc")) : esc(t("trend.src_fc")));
        }
      }
      return html;
    };
    return { option, yFor: zoom ? (a, b) => { const r = yFor(a, b); return r; } : undefined, meta: { fcShown, from: vFrom, to: vTo } };
  }

  /** "Which period brought how many citations?" Bars per day / month / year; clicking one opens its dates. */
  function period(data, c) {
    const { tk, t, nf } = c;
    const gran = c.gran || "month";
    const src = periodSource(data, gran);
    let periods = clipPeriods(buildPeriods(src.points, gran, c.range.to != null ? c.range.to : todayUtc(c)), c.range);
    if (!periods.length) return null;
    const labels = periods.map((p) => periodLabel(c, p, gran, false));
    const zoom = !c.print && !c.mini && periods.length > (gran === "day" ? 90 : 48);
    const clickable = src.source === "openalex" && !c.print;
    const ts = src.source === "openalex" ? tailStart(c.oaLatest, gran) : null;
    const tailIdx = ts == null ? -1 : periods.findIndex((p) => p.t >= ts);
    const option = Object.assign(base(c, { grid: { left: 4, right: 12, top: 12, bottom: zoom ? 44 : 4 } }), {
      xAxis: {
        type: "category", data: labels, axisTick: { show: false }, axisLine: { lineStyle: { color: tk.grid } },
        axisLabel: Object.assign({ hideOverlap: true, interval: "auto" }, axisText(c)),
      },
      yAxis: valueAxis(c),
      dataZoom: zoom ? [
        { type: "inside", startValue: Math.max(0, periods.length - (gran === "day" ? 90 : 48)), endValue: periods.length - 1, zoomOnMouseWheel: "ctrl" },
        { type: "slider", startValue: Math.max(0, periods.length - (gran === "day" ? 90 : 48)), endValue: periods.length - 1, height: 20, bottom: 6, borderColor: "transparent", backgroundColor: tk.grid, fillerColor: alpha(tk.accent, 0.15), handleSize: 14, textStyle: { color: tk.text3 } },
      ] : undefined,
      series: [{
        type: "bar", data: periods.map((p) => ({ value: p.v, key: p.key })), barMaxWidth: 28, cursor: clickable ? "pointer" : "default",
        itemStyle: { color: tk.accent, borderRadius: [3, 3, 0, 0] }, emphasis: { itemStyle: { color: tk.accent, opacity: 0.8 } },
        animationDelay: (i) => Math.min(i * 6, 400),
        markArea: tailIdx < 0 ? undefined : {
          silent: true, data: [[{ xAxis: tailIdx }, { xAxis: periods.length - 1 }]],
          itemStyle: { color: hatch(c) },
          label: { show: periods.length - tailIdx > 2, formatter: t("an.tail_label"), position: "insideTop", color: tk.text3, fontSize: 11 },
        },
      }],
    });
    option.tooltip.formatter = (ps) => {
      const p = ps[0];
      const per = periods[p.dataIndex];
      if (tailIdx >= 0 && p.dataIndex >= tailIdx) return tipHead(c, periodLabel(c, per, gran, true)) + `<div style="max-width:260px;white-space:normal;color:${tk.text2}">${esc(t("an.tail_tip"))}</div>`;
      return tipHead(c, periodLabel(c, per, gran, true))
        + tipRow(c, tk.accent, t("main.tt_new"), nf.format(per.v), esc(t(src.source === "scholar" ? "an.src_scholar_year" : "an.src_citing")) + (clickable && per.v > 0 ? "<br>" + esc(t("an.click_dates")) : ""));
    };
    return {
      option, meta: { source: src.source, excluded: src.excluded, n: periods.length, clickable, tail: tailIdx >= 0 ? periods.length - tailIdx : 0 },
      click: clickable ? (p) => { const per = periods[p.dataIndex]; if (per && per.v > 0) c.onPick && c.onPick(per, gran); } : undefined,
    };
  }

  /** "Which articles do my citations come from?" Citing works per month, stacked by cited article. */
  function stacked(data, c) {
    const s = data && data.stacked;
    if (!s || !s.months.length) return null;
    const { tk, t, nf } = c;
    const idx = s.months.map((m, i) => i).filter((i) => {
      const ms = keyMs(s.months[i]);
      const end = Date.UTC(new Date(ms).getUTCFullYear(), new Date(ms).getUTCMonth() + 1, 1);
      return (c.range.from == null || end > c.range.from) && (c.range.to == null || ms <= c.range.to);
    });
    if (!idx.length) return null;
    const months = idx.map((i) => s.months[i]);
    const wide = months.length > 18;
    const rows = s.series.map((x, i) => ({ name: shortText(x.title, 46), full: x.title, data: idx.map((j) => x.counts[j]), color: tk.cat[i] }));
    if (s.other && s.other.length) rows.push({ name: t("an.other_articles", { n: s.other_articles }), full: t("an.other_articles", { n: s.other_articles }), data: idx.map((j) => s.other[j]), color: tk.cat[5] });
    const zoom = !c.print && !c.mini && months.length > 36;
    const option = Object.assign(base(c, { grid: { left: 4, right: 12, top: 12, bottom: zoom ? 80 : 40 } }), {
      legend: { type: c.print ? "plain" : "scroll", bottom: zoom ? 38 : 0, left: 0, icon: "roundRect", itemWidth: 12, itemHeight: 8, itemGap: 14, textStyle: { color: tk.text2, fontSize: 12, fontFamily: tk.font }, inactiveColor: tk.text3, pageIconColor: tk.text2, pageTextStyle: { color: tk.text2 } },
      xAxis: { type: "category", data: months.map((m) => monthShort(c, m, wide || new Date(keyMs(m)).getUTCMonth() === 0)), axisTick: { show: false }, axisLine: { lineStyle: { color: tk.grid } }, axisLabel: Object.assign({ hideOverlap: true }, axisText(c)) },
      yAxis: valueAxis(c),
      dataZoom: zoom ? [
        { type: "inside", startValue: Math.max(0, months.length - 36), endValue: months.length - 1, zoomOnMouseWheel: "ctrl" },
        { type: "slider", startValue: Math.max(0, months.length - 36), endValue: months.length - 1, height: 20, bottom: 6, borderColor: "transparent", backgroundColor: tk.grid, fillerColor: alpha(tk.accent, 0.15), handleSize: 14, textStyle: { color: tk.text3 } },
      ] : undefined,
      series: rows.map((r, i) => ({
        name: r.name, type: "bar", stack: "all", data: r.data, barMaxWidth: 26, itemStyle: { color: r.color, borderRadius: i === rows.length - 1 ? [3, 3, 0, 0] : 0 },
        emphasis: { focus: "series" }, animationDelay: (k) => Math.min(k * 8, 500),
      })),
    });
    option.tooltip.formatter = (ps) => {
      const i = ps[0].dataIndex;
      const total = rows.reduce((a, r) => a + r.data[i], 0);
      let html = tipHead(c, utcLong(c, months[i]));
      for (const p of ps) {
        const r = rows[p.seriesIndex];
        if (r.data[i] > 0) html += tipRow(c, r.color, shortText(r.full, 56), nf.format(r.data[i]));
      }
      return html + `<div style="margin-top:6px;padding-top:4px;border-top:1px solid ${tk.stroke};display:flex;justify-content:space-between"><span style="color:${tk.text2}">${esc(t("an.total"))}</span><b>${nf.format(total)}</b></div>`
        + `<div style="margin-top:2px;color:${tk.text3};font-size:11px">${esc(t("an.src_citing"))}</div>`;
    };
    return { option, meta: { rows: rows.length, months: months.length } };
  }

  /** "Which articles rose fastest?" Horizontal bars of the gain, click opens the article. */
  function risers(r, c) {
    const items = Array.isArray(r) ? r : r && r.items;
    if (!items || !items.length) return null;
    const { tk, t, nf } = c;
    const option = Object.assign(base(c, { grid: { left: 4, right: 40, top: 6, bottom: 4 } }), {
      xAxis: valueAxis(c, { minInterval: 1 }),
      yAxis: {
        type: "category", inverse: true, data: items.map((x) => shortText(x.title, c.print ? 70 : 42)), axisTick: { show: false }, axisLine: { show: false },
        axisLabel: Object.assign({ width: c.print ? 300 : 230, overflow: "truncate" }, axisText(c), { color: tk.text2 }),
      },
      series: [{
        type: "bar", data: items.map((x) => ({ value: x.gain, key: x.key })), barMaxWidth: 20, cursor: c.print ? "default" : "pointer",
        itemStyle: { color: tk.accent, borderRadius: [0, 3, 3, 0] }, emphasis: { itemStyle: { opacity: 0.8 } },
        label: { show: true, position: "right", color: tk.text2, fontWeight: 600, formatter: (p) => "+" + nf.format(p.value) },
        animationDelay: (i) => i * 60,
      }],
    });
    option.tooltip.trigger = "item";
    option.tooltip.formatter = (p) => {
      const x = items[p.dataIndex];
      return tipHead(c, x.title) + tipRow(c, tk.accent, t("an.gain"), "+" + nf.format(x.gain), esc(t("an.now_cites", { n: nf.format(x.citations) })) + (c.print ? "" : "<br>" + esc(t("an.click_article"))));
    };
    return { option, click: c.print ? undefined : (p) => { const x = items[p.dataIndex]; if (x && c.onPick) c.onPick(x); } };
  }

  /** "How did h and i10 change?" Step lines over the Scholar checks. */
  function indices(data, c) {
    const ih = data && data.indices;
    if (!ih || !ih.points.length) return null;
    const { tk, t } = c;
    const pts = ih.points.map((p) => ({ t: new Date(p.t).getTime(), h: p.h, i10: p.i10 }));
    const last = pts[pts.length - 1];
    const endT = Math.max(c.now, last.t);
    const hs = pts.map((p) => [p.t, p.h]).concat([[endT, last.h]]);
    const is = pts.map((p) => [p.t, p.i10]).concat([[endT, last.i10]]);
    const ch = new Map();
    for (const x of ih.changes) { const k = new Date(x.t).getTime(); (ch.get(k) || ch.set(k, []).get(k)).push(x); }
    const firstT = pts[0].t;
    const from = c.range.from == null ? firstT : Math.max(c.range.from, firstT);
    const to = c.range.to == null ? endT : Math.min(c.range.to, endT);
    const vmax = Math.max(...pts.map((p) => Math.max(p.h, p.i10)), 1);
    const yb = niceBounds(0, vmax * 1.1, 4);
    const mk = (id, name, data, color) => ({
      id, name, type: "line", step: "end", data, showSymbol: true, symbol: "circle", symbolSize: 6, lineStyle: { width: 2.5, color }, itemStyle: { color },
      emphasis: { focus: "none" }, areaStyle: { color: areaGradient(color, 0.1) },
    });
    const option = Object.assign(base(c, { grid: { left: 4, right: 16, top: 40, bottom: 4 } }), {
      legend: { top: 0, left: 0, icon: "roundRect", itemWidth: 16, itemHeight: 3, itemGap: 18, textStyle: { color: tk.text2, fontSize: 12, fontFamily: tk.font }, inactiveColor: tk.text3 },
      xAxis: { type: "time", min: from, max: to, axisLine: { lineStyle: { color: tk.grid } }, axisTick: { show: false }, axisLabel: Object.assign({ hideOverlap: true, formatter: { year: "{yyyy}", month: "{MMM}", day: "{d} {MMM}" } }, axisText(c)), splitLine: { show: false } },
      yAxis: valueAxis(c, { min: 0, max: yb.max, interval: yb.interval }),
      series: [mk("h", "h", hs, tk.accent), mk("i10", "i10", is, tk.cat[3])],
    });
    option.tooltip.axisPointer = { type: "cross", snap: true, lineStyle: { color: tk.text3, type: "dashed" }, crossStyle: { color: tk.text3, type: "dashed" }, label: { show: false } };
    option.tooltip.formatter = (ps) => {
      if (!ps.length) return "";
      const x = ps[0].value[0];
      let html = tipHead(c, dateTime(c, x));
      for (const p of ps) html += tipRow(c, p.color, p.seriesName === "h" ? t("kpi.h") : t("kpi.i10"), String(p.value[1]));
      const changes = ch.get(x) || [];
      for (const k of changes) html += `<div style="margin-top:4px;font-weight:600;color:${tk.pos}">${esc(t("an.idx_change", { metric: k.metric === "h" ? "h" : "i10", from: k.from, to: k.to }))}</div>`;
      return html + `<div style="margin-top:4px;color:${tk.text3};font-size:11px">${esc(t("an.src_scholar_check"))}</div>`;
    };
    return { option };
  }

  /** "On which days did the citations come?" GitHub-style heatmap of one year. `c.year`, `c.days` Map(date -> {n, titles}). */
  function calendar(data, c) {
    const cal = data && data.calendar;
    if (!cal) return null;
    const { tk, t, nf } = c;
    const year = c.year;
    const days = new Map((c.days || cal.days).map(([d, n]) => [d, n]));
    const inRange = (d) => {
      const ms = keyMs(d);
      return (c.range.from == null || ms + DAY > c.range.from) && (c.range.to == null || ms <= c.range.to);
    };
    const pts = [];
    for (const [d, n] of days) {
      if (!d.startsWith(String(year)) || !inRange(d)) continue;
      const item = { value: [d, n] };
      if (c.selected === d) item.itemStyle = { borderColor: tk.accent, borderWidth: 2 };
      pts.push(item);
    }
    const tailMs = tailStart(c.oaLatest, "day");
    const tail = [];
    if (tailMs != null && !c.mini) {
      const yEnd = Math.min(Date.UTC(year, 11, 31), keyMs(ymd(c.now)));
      for (let ms = Math.max(tailMs, Date.UTC(year, 0, 1)); ms <= yEnd; ms += DAY) {
        const d = ymd(ms);
        if (inRange(d)) tail.push({ value: [d, 0], itemStyle: { color: hatch(c), borderColor: tk.surface, borderWidth: 3 } });
      }
    }
    const ramp = [0.22, 0.4, 0.6, 0.82, 1].map((a) => alpha(tk.accent, a));
    const pieces = [
      { min: 1, max: 1, color: ramp[0], label: "1" },
      { min: 2, max: 2, color: ramp[1], label: "2" },
      { min: 3, max: 4, color: ramp[2], label: "3–4" },
      { min: 5, max: 9, color: ramp[3], label: "5–9" },
      { min: 10, color: ramp[4], label: "10+" },
    ];
    const sun = Date.UTC(2023, 0, 1); // a Sunday
    const dayNames = [0, 1, 2, 3, 4, 5, 6].map((i) => new Date(sun + i * DAY).toLocaleDateString(c.loc, { weekday: "short", timeZone: "UTC" }));
    const monthNames = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11].map((m) => new Date(Date.UTC(2023, m, 1)).toLocaleDateString(c.loc, { month: "short", timeZone: "UTC" }));
    const option = Object.assign(base(c, { tooltip: { trigger: "item" } }), {
      visualMap: {
        type: "piecewise", pieces, orient: "horizontal", right: 4, bottom: 0, itemWidth: 12, itemHeight: 12, itemGap: 6, itemSymbol: "roundRect",
        textStyle: { color: tk.text2, fontSize: 11, fontFamily: tk.font }, outOfRange: { color: tk.cell }, show: !c.mini, seriesIndex: 0,
      },
      calendar: {
        top: 24, left: 34, right: 6, bottom: 34, cellSize: ["auto", c.mini ? 11 : 15], range: String(year),
        itemStyle: { color: tk.cell, borderColor: tk.surface, borderWidth: 3 }, splitLine: { show: false }, yearLabel: { show: false },
        dayLabel: { firstDay: 1, nameMap: dayNames, color: tk.text3, fontSize: 11, fontFamily: tk.font, margin: 8 },
        monthLabel: { nameMap: monthNames, color: tk.text3, fontSize: 11, fontFamily: tk.font, margin: 8 },
      },
      series: [{
        type: "heatmap", coordinateSystem: "calendar", data: pts, cursor: c.print ? "default" : "pointer",
        itemStyle: { borderRadius: 2, borderColor: tk.surface, borderWidth: 3 },
        emphasis: { itemStyle: { borderColor: tk.text, borderWidth: 1 } },
      }].concat(tail.length ? [{ type: "heatmap", coordinateSystem: "calendar", data: tail, silent: false, itemStyle: { borderRadius: 2 }, emphasis: { itemStyle: { borderColor: tk.text, borderWidth: 1 } } }] : []),
    });
    option.tooltip.formatter = (p) => {
      const [d, n] = p.value;
      if (p.seriesIndex === 1) return tipHead(c, utcLong(c, d)) + `<div style="max-width:260px;white-space:normal;color:${tk.text2}">${esc(t("an.tail_tip"))}</div>`;
      const info = c.dayInfo && c.dayInfo.get(d);
      const titles = info && info.titles ? info.titles.slice(0, 3).map((x) => `<div style="margin-left:16px;color:${tk.text3};font-size:11px;max-width:300px;white-space:normal">• ${esc(shortText(x, 90))}</div>`).join("") : "";
      return tipHead(c, utcLong(c, d)) + tipRow(c, tk.accent, t("an.cal_works"), nf.format(n), esc(t("an.src_citing"))) + titles
        + (c.print ? "" : `<div style="margin-top:4px;color:${tk.text3};font-size:11px">${esc(t("an.click_day"))}</div>`);
    };
    return { option, click: c.print ? undefined : (p) => { if (p.seriesIndex === 0 && p.value && c.onPick) c.onPick(p.value[0]); }, meta: { year, days: pts.length, tail: tail.length } };
  }

  /** Top countries / venues as horizontal bars; click filters the list. `items`: [[key, n]], `c.labelOf`, `c.total`, `c.active`. */
  function bars(items, c) {
    if (!items || !items.length) return null;
    const { tk, t, nf } = c;
    const lab = (k) => c.labelOf(k);
    const total = c.total || items.reduce((a, x) => a + x[1], 0);
    const option = Object.assign(base(c, { grid: { left: 4, right: 38, top: 4, bottom: 4 } }), {
      xAxis: valueAxis(c, { show: false }),
      yAxis: {
        type: "category", inverse: true, data: items.map(([k]) => k), axisTick: { show: false }, axisLine: { show: false },
        axisLabel: Object.assign({ width: c.labelWidth || 150, overflow: "truncate", formatter: (k) => lab(k) }, axisText(c), { color: tk.text2 }),
      },
      series: [{
        type: "bar", cursor: c.print ? "default" : "pointer", barMaxWidth: 16,
        data: items.map(([k, n]) => ({
          value: n, key: k,
          itemStyle: { color: c.active && String(c.active).toLowerCase() !== String(k).toLowerCase() ? alpha(tk.accent, 0.35) : tk.accent, borderRadius: [0, 3, 3, 0] },
        })),
        label: { show: true, position: "right", color: tk.text2, fontWeight: 600, formatter: (p) => nf.format(p.value) },
        emphasis: { itemStyle: { opacity: 0.8 } }, animationDelay: (i) => i * 40,
      }],
    });
    option.tooltip.trigger = "item";
    option.tooltip.formatter = (p) => {
      const [k, n] = items[p.dataIndex];
      return tipHead(c, lab(k)) + tipRow(c, tk.accent, t("an.cal_works"), nf.format(n), esc(t("dist.share", { p: total ? Math.round((n / total) * 100) : 0 })) + (c.print ? "" : "<br>" + esc(t("dist.click_filter"))));
    };
    return { option, click: c.print ? undefined : (p) => { const it = items[p.dataIndex]; if (it && c.onPick) c.onPick(it[0]); } };
  }

  /** Small line of one article's Scholar counts (Articles page). */
  function articleLine(points, c) {
    if (!points.length) return null;
    const { tk, t, nf } = c;
    const vals = points.map((p) => p[1]);
    const yb = niceBounds(Math.min(...vals), Math.max(...vals), 3);
    const option = Object.assign(base(c, { grid: { left: 4, right: 12, top: 10, bottom: 4 } }), {
      xAxis: { type: "time", axisLine: { lineStyle: { color: tk.grid } }, axisTick: { show: false }, axisLabel: Object.assign({ hideOverlap: true }, axisText(c)), splitLine: { show: false } },
      yAxis: valueAxis(c, { min: yb.min, max: yb.max, interval: yb.interval }),
      series: [{
        type: "line", data: points, showSymbol: points.length < 40, symbolSize: 6, lineStyle: { width: 2.5, color: tk.accent }, itemStyle: { color: tk.accent },
        areaStyle: { color: areaGradient(tk.accent, 0.2) }, emphasis: { focus: "none" },
      }],
    });
    option.tooltip.formatter = (ps) => {
      const p = ps[0];
      const prev = points[p.dataIndex - 1];
      return tipHead(c, dateTime(c, p.value[0])) + tipRow(c, tk.accent, t("chart.series_cites"), nf.format(p.value[1]), prev ? deltaHtml(c, p.value[1] - prev[1]) : "");
    };
    return { option };
  }

  /** Monthly citing works of one article (Articles page). `monthly`: [[YYYY-MM, n]]. */
  function articleMonthly(monthly, c) {
    const { tk, t, nf } = c;
    const periods = buildPeriods(monthly, "month", todayUtc(c)).slice(-120);
    if (!periods.length) return null;
    const option = Object.assign(base(c, { grid: { left: 4, right: 12, top: 10, bottom: 4 } }), {
      xAxis: { type: "category", data: periods.map((p) => periodLabel(c, p, "month", false)), axisTick: { show: false }, axisLine: { lineStyle: { color: tk.grid } }, axisLabel: Object.assign({ hideOverlap: true }, axisText(c)) },
      yAxis: valueAxis(c),
      series: [{ type: "bar", data: periods.map((p) => p.v), barMaxWidth: 20, itemStyle: { color: tk.accent, borderRadius: [3, 3, 0, 0] }, emphasis: { itemStyle: { opacity: 0.8 } } }],
    });
    option.tooltip.formatter = (ps) => {
      const p = periods[ps[0].dataIndex];
      return tipHead(c, periodLabel(c, p, "month", true)) + tipRow(c, tk.accent, t("main.tt_new"), nf.format(p.v), esc(t("an.src_citing")));
    };
    return { option };
  }

  /** A builder result as a tiny picture (no axes text, legend, zoom or tooltip) for the report preview. */
  function thumb(built) {
    if (!built) return null;
    const o = built.option;
    o.animation = false;
    o.legend = { show: false };
    o.dataZoom = undefined;
    o.tooltip = { show: false };
    if (o.visualMap) o.visualMap.show = false;
    const quiet = (a) => {
      for (const x of Array.isArray(a) ? a : a ? [a] : []) {
        x.axisLabel = Object.assign(x.axisLabel || {}, { show: false });
        x.splitLine = { show: false };
        x.axisLine = { show: false };
      }
    };
    quiet(o.xAxis);
    quiet(o.yAxis);
    o.grid = Object.assign(o.grid || {}, { left: 2, right: 2, top: 4, bottom: 2, containLabel: false });
    for (const s of o.series || []) {
      s.markLine = undefined;
      s.markPoint = undefined;
      s.markArea = undefined;
      s.label = { show: false };
      s.animation = false;
    }
    if (o.calendar) {
      Object.assign(o.calendar, { dayLabel: { show: false }, monthLabel: { show: false }, top: 4, left: 2, right: 2, bottom: 2, cellSize: ["auto", "auto"] });
    }
    return { option: o };
  }

  /** Granularity of the period bars for a range: days for a quarter or less, months up to three years, else years. */
  function autoGran(fromMs, toMs) {
    const days = (toMs - fromMs) / DAY;
    return days <= 100 ? "day" : days <= 3 * 366 ? "month" : "year";
  }

  // ------------------------------------------------------------ mounting
  const reg = new Map(); // element -> { chart, ro }

  function dispose(el) {
    const r = reg.get(el);
    if (!r) return;
    r.ro.disconnect();
    try { r.chart.dispose(); } catch { /* already gone */ }
    reg.delete(el);
  }
  /** Disposes every chart inside `root` (call before the markup is replaced or a tab is left). */
  function disposeIn(root) {
    for (const el of Array.from(reg.keys())) if (el === root || root.contains(el) || !el.isConnected) dispose(el);
  }
  const live = () => reg.size;

  /** Draws `built` (a builder result) into `el`. null clears the element and shows `emptyText` when given. */
  function mount(el, built, o = {}) {
    dispose(el);
    if (!built) {
      el.innerHTML = o.emptyText ? `<div class="chart-empty">${esc(o.emptyText)}</div>` : "";
      return null;
    }
    el.innerHTML = "";
    const chart = E.init(el, null, { renderer: o.renderer || "canvas", width: o.width, height: o.height });
    chart.setOption(built.option);
    if (built.click) chart.on("click", built.click);
    if (built.yFor) {
      chart.on("datazoom", () => {
        const dz = chart.getOption().dataZoom && chart.getOption().dataZoom[0];
        if (!dz || dz.startValue == null) return;
        const b = built.yFor(dz.startValue, dz.endValue);
        chart.setOption({ yAxis: { min: b.min, max: b.max, interval: b.interval } });
      });
    }
    let raf = 0;
    const ro = new ResizeObserver(() => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => { if (el.clientWidth > 0 && el.clientHeight > 0) chart.resize({ animation: { duration: 0 } }); });
    });
    ro.observe(el);
    reg.set(el, { chart, ro });
    return chart;
  }

  window.Charts = {
    ctx, tokens, mount, dispose, disposeIn, live, thumb,
    build: { trend, period, stacked, risers, indices, calendar, bars, articleLine, articleMonthly },
    util: { buildPeriods, clipPeriods, periodSource, keyMs, ymd, utcLong, shortText, niceBounds, autoGran },
  };
})();
