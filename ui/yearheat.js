"use strict";
/* Analysis > Calendar: the year heat map for all years at once. One row per year (newest first), 53 week columns by
   7 weekday rows, square cells that shrink to fit (6..14 px), one colour scale for every year. Plain DOM: only the
   days with citations are buttons. Clicking a day reports it through `onPick`. */
(() => {
  const DAY = 86400e3;
  const GUT = 44;
  const pad2 = (n) => String(n).padStart(2, "0");
  const iso = (ms) => { const d = new Date(ms); return `${d.getUTCFullYear()}-${pad2(d.getUTCMonth() + 1)}-${pad2(d.getUTCDate())}`; };
  const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
  const mondayIndex = (ms) => (new Date(ms).getUTCDay() + 6) % 7; // Monday = 0

  /** Cell size in px for a container width: clamp(floor((avail - 44 - 52 * gap) / 53), 6, 14); the gap is 1 px for cells under 9 px. */
  function cellSize(avail, cols = 53) {
    const calc = (gap) => Math.floor((avail - GUT - (cols - 1) * gap) / cols);
    let gap = 2;
    let cs = Math.max(6, Math.min(14, calc(gap)));
    if (cs < 9) { gap = 1; cs = Math.max(6, Math.min(14, calc(gap))); }
    return { cs, gap };
  }

  /** spec: { rows: [{ label, sub, start, end }] (ms, UTC midnights, start is the first day shown), days: Map(iso -> n),
              selected, onPick(iso), tailFrom (iso | ""), today (iso), t, loc, palette: {seq[5], cell}, emptyText } */
  function mount(el, spec) {
    if (el._yhDispose) el._yhDispose();
    const { t, loc } = spec;
    const rows = spec.rows;
    if (!rows.length) { el.innerHTML = `<div class="chart-empty yh-empty">${esc(spec.emptyText)}</div>`; return; }
    let max = 0;
    for (const r of rows) for (let ms = r.start; ms <= r.end; ms += DAY) max = Math.max(max, spec.days.get(iso(ms)) || 0);
    const level = (n) => (n <= 0 ? 0 : max <= 5 ? Math.min(5, n) : Math.min(5, Math.ceil((n * 5) / max)));
    const todayMs = spec.today ? Date.parse(spec.today + "T00:00:00Z") : Infinity;
    const cols = Math.max(53, ...rows.map((r) => Math.floor((mondayIndex(r.start) + Math.round((r.end - r.start) / DAY)) / 7) + 1));

    const monthLabel = (ms) => new Date(ms).toLocaleDateString(loc, { month: "short", timeZone: "UTC" });
    let html = `<div class="yh-in" style="--cols:${cols}">`;
    // month labels: once, above the first row
    const first = rows[0];
    const off0 = mondayIndex(first.start);
    let months = "";
    for (let ms = first.start; ms <= first.end; ms += DAY) {
      const d = new Date(ms);
      if (d.getUTCDate() !== 1) continue;
      const c = Math.floor((off0 + Math.round((ms - first.start) / DAY)) / 7);
      if (c > cols - 3 && ms - first.start < 20 * DAY) continue;
      months += `<span style="grid-column:${c + 1} / span 4">${esc(monthLabel(ms))}</span>`;
    }
    html += `<div class="yh-mrow"><span class="yh-gut"></span><div class="yh-months">${months}</div></div>`;
    for (const r of rows) {
      const off = mondayIndex(r.start);
      let cells = "";
      for (let ms = r.start; ms <= r.end; ms += DAY) {
        if (ms > todayMs) break;
        const i = Math.round((ms - r.start) / DAY) + off;
        const col = Math.floor(i / 7) + 1, row = (i % 7) + 1;
        const d = iso(ms);
        const n = spec.days.get(d) || 0;
        const tail = spec.tailFrom && d >= spec.tailFrom ? " tail" : "";
        const sel = spec.selected === d ? " sel" : "";
        const pos = `style="grid-column:${col};grid-row:${row}"`;
        cells += n > 0
          ? `<button type="button" class="yh-c l${level(n)}${tail}${sel}" ${pos} data-d="${d}" data-n="${n}" aria-label="${esc(t("cal.heat_aria", { date: d, n }))}"></button>`
          : `<i class="yh-c${tail}" ${pos} data-d="${d}" data-n="0"></i>`;
      }
      html += `<div class="yh-row"><div class="yh-gut"><b>${esc(r.label)}</b><span>${esc(r.sub)}</span></div><div class="yh-right"><div class="yh-grid">${cells}</div>${r.caption ? `<div class="yh-rowcap">${esc(r.caption)}</div>` : ""}</div></div>`;
    }
    html += `<div class="yh-legend" aria-hidden="true"><span>${esc(t("cal.heat_less"))}</span><i class="yh-c"></i><i class="yh-c l1"></i><i class="yh-c l2"></i><i class="yh-c l3"></i><i class="yh-c l4"></i><i class="yh-c l5"></i><span>${esc(t("cal.heat_more"))}</span></div></div><div class="yh-tip" hidden></div>`;
    el.innerHTML = html;
    el.classList.add("yh");
    const pal = spec.palette;
    pal.seq.forEach((c, i) => el.style.setProperty(`--yh${i + 1}`, c));
    el.style.setProperty("--yh0", pal.cell);

    const size = () => {
      const { cs, gap } = cellSize(el.clientWidth, cols);
      el.style.setProperty("--cs", cs + "px");
      el.style.setProperty("--gap", gap + "px");
    };
    size();
    const ro = new ResizeObserver(size);
    ro.observe(el);

    const tip = el.querySelector(".yh-tip");
    const longDate = (d) => new Date(Date.parse(d + "T00:00:00Z")).toLocaleDateString(loc, { weekday: "long", day: "numeric", month: "long", year: "numeric", timeZone: "UTC" });
    const over = (e) => {
      const c = e.target.closest("[data-d]");
      if (!c || !c.classList.contains("yh-c")) { tip.hidden = true; return; }
      tip.innerHTML = `<div class="tt-h">${esc(longDate(c.dataset.d))}</div><div>${esc(t("an.cal_works"))}: <b>${esc(new Intl.NumberFormat(loc).format(Number(c.dataset.n)))}</b></div>${c.classList.contains("tail") ? `<div class="tt-s">${esc(t("an.tail_tip"))}</div>` : ""}`;
      tip.hidden = false;
      const b = el.getBoundingClientRect();
      let x = e.clientX - b.left + 12, y = e.clientY - b.top + 14;
      const tw = tip.offsetWidth;
      if (x + tw > b.width) x = Math.max(0, e.clientX - b.left - tw - 12);
      tip.style.transform = `translate(${x}px, ${y}px)`;
    };
    const click = (e) => { const c = e.target.closest("button[data-d]"); if (c && spec.onPick) spec.onPick(c.dataset.d); };
    el.addEventListener("mousemove", over);
    const leave = () => { tip.hidden = true; };
    el.addEventListener("mouseleave", leave);
    el.addEventListener("click", click);
    el._yhDispose = () => { ro.disconnect(); el.removeEventListener("mousemove", over); el.removeEventListener("mouseleave", leave); el.removeEventListener("click", click); el._yhDispose = null; };
  }

  function dispose(el) { if (el._yhDispose) el._yhDispose(); }

  window.YearHeat = { mount, dispose, cellSize, iso, DAY };
})();
