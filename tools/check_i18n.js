"use strict";
// Checks the UI strings: TR and EN have the same keys, every key the UI code uses exists, and the period
// vocabulary has no abbreviations ("30G", "12A", "3Y"): ranges say "Son 30 gün" / "Last 30 days", grouping
// says "Günlük" / "Daily". Run: node tools/check_i18n.js (needs no packages).
const fs = require("fs");
const path = require("path");
const ui = path.join(__dirname, "..", "ui");
global.window = {};
new Function("window", fs.readFileSync(path.join(ui, "i18n.js"), "utf8"))(global.window);
const dict = window.I18N.dict;
let bad = 0;
const fail = (m) => { bad++; console.error("FAIL " + m); };

for (const k of Object.keys(dict.tr)) if (!(k in dict.en)) fail(`key only in tr: ${k}`);
for (const k of Object.keys(dict.en)) if (!(k in dict.tr)) fail(`key only in en: ${k}`);

const used = new Set();
for (const f of ["app.js", "charts.js", "citers.js", "citers-lib.js", "yearheat.js", "report.js", "index.html"]) {
  const text = fs.readFileSync(path.join(ui, f), "utf8");
  for (const m of text.matchAll(/\bt\(\s*["'`]([a-z0-9_.]+)["'`]/g)) used.add(m[1]);
  for (const m of text.matchAll(/data-i18n(?:-[a-z]+)?="([a-z0-9_.]+)"/g)) used.add(m[1]);
}
for (const k of used) {
  if (k.endsWith(".") || k.endsWith("_")) continue; // built at run time: "rep.c_" + id, "main.g_" + gran
  for (const l of ["tr", "en"]) if (!(k in dict[l])) fail(`missing ${l} key: ${k}`);
}
for (const id of ["trend", "period", "article", "index", "calendar", "dist", "citgeo", "citfields", "citself"]) for (const l of ["tr", "en"]) if (!dict[l]["rep.c_" + id]) fail(`missing ${l} rep.c_${id}`);
for (const g of ["day", "month", "year"]) for (const l of ["tr", "en"]) if (!dict[l]["main.g_" + g]) fail(`missing ${l} main.g_${g}`);

const abbr = /^\s*\d+\s?[A-Za-zÇĞİÖŞÜçğıöşü]\s*$/; // 30G, 12A, 3Y, 12M
for (const l of ["tr", "en"]) {
  for (const [k, v] of Object.entries(dict[l])) {
    if (abbr.test(v)) fail(`abbreviation in ${l} ${k}: "${v}"`);
    if (/^main\.r_/.test(k)) fail(`old range key still present: ${k}`);
  }
  for (const k of ["an.r_30d", "an.r_90d", "an.r_12m", "ris.d30", "ris.d90"]) {
    if (!/^(Son|Last) \d+ (gün|ay|days|months)$/.test(dict[l][k])) fail(`range label not spelled out: ${l} ${k} = "${dict[l][k]}"`);
  }
  for (const [k, want] of Object.entries({ "main.g_day": { tr: "Günlük", en: "Day" }, "main.g_month": { tr: "Aylık", en: "Month" }, "main.g_year": { tr: "Yıllık", en: "Year" } })) {
    if (dict[l][k] !== want[l]) fail(`grouping label ${l} ${k} = "${dict[l][k]}", expected "${want[l]}"`);
  }
}
// no range selector may hold a calendar-month label such as "30 gün" next to "Aylık"
console.log(`${Object.keys(dict.tr).length} keys per language, ${used.size} used in code, ${bad} problems`);
process.exit(bad ? 1 : 0);
