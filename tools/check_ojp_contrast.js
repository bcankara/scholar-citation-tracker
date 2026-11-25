"use strict";
// Recomputes the WCAG contrast of the OJP gauge (number against badge fill) from ui/app.css and ui/report.css:
// 10 steps x 2 themes in the app, 10 steps in the report. Fails below 4.5:1. Run: node tools/check_ojp_contrast.js
const fs = require("fs");
const path = require("path");
const ui = path.join(__dirname, "..", "ui");
const lum = (hex) => {
  const c = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
};
const ratio = (a, b) => { const x = lum(a), y = lum(b); return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05); };
const grab = (block) => {
  const fills = [];
  for (let i = 0; i < 10; i++) { const m = block.match(new RegExp(`--ojp-${i}:\\s*(#[0-9a-fA-F]{6})`)); if (!m) throw new Error("missing --ojp-" + i); fills.push(m[1]); }
  const a = block.match(/--ojp-ink-a:\s*(#[0-9a-fA-F]{6})/)[1];
  const b = block.match(/--ojp-ink-b:\s*(#[0-9a-fA-F]{6})/)[1];
  return { fills, a, b };
};
let bad = 0, n = 0;
const check = (label, { fills, a, b }, aSteps) => {
  fills.forEach((f, i) => {
    const ink = aSteps.includes(i) ? a : b;
    const r = ratio(f, ink);
    n++;
    if (r < 4.5) { bad++; console.error(`FAIL ${label} step ${i}: ${f} on ${ink} = ${r.toFixed(2)}`); }
  });
};
const css = fs.readFileSync(path.join(ui, "app.css"), "utf8");
const start = css.indexOf("--ojp-0:");
const block = css.slice(css.lastIndexOf(":root", start), css.indexOf(".ojp {", start));
const [light, dark] = block.split("@media (prefers-color-scheme: dark)");
check("app light", grab(light), [0, 1, 2, 3, 4, 5, 6, 7]);
check("app dark", grab(dark), [0, 1, 2]);
const rep = fs.readFileSync(path.join(ui, "report.css"), "utf8");
check("report", grab(rep.slice(rep.indexOf("--ojp-0:") - 40, rep.indexOf(".ojp {"))), [0, 1, 2, 3, 4, 5, 6, 7]);
console.log(`${n} pairs checked, ${bad} below 4.5:1`);
process.exit(bad ? 1 : 0);
