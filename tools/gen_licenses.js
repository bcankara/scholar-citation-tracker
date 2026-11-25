#!/usr/bin/env node
// Generates the licence data of the Credits page from Cargo.lock (no npm dependency).
//
//   node tools/gen_licenses.js          write ui/licenses/{rust-crates.json, rust-license-texts.json, app-MIT.txt}
//   node tools/gen_licenses.js --check  regenerate in memory; exit 1 when a file differs byte for byte
//                                       or Cargo.lock changed since the files were written
//
// Only crates that are compiled into the app are listed: the normal-dependency closure of the root
// package (dev- and build-only crates are not in the shipped binary). The closure is resolved once per
// shipped target and the lists are merged, so a crate that only one platform compiles is still listed,
// with the platforms it belongs to in its "platforms" field; the Credits page shows the ones the running
// binary was built from. Output is deterministic: sorted keys and arrays, LF line endings, no timestamps,
// no absolute paths.

"use strict";
const fs = require("fs");
const path = require("path");
const crypto = require("crypto");
const { spawnSync } = require("child_process");

const ROOT = path.resolve(__dirname, "..");
const TAURI = path.join(ROOT, "src-tauri");
const OUT = path.join(ROOT, "ui", "licenses");
// Every target a release ships. Both Apple targets resolve to the same crates today, but asking for
// each one keeps the list right if that stops being true.
const TARGETS = [
  { target: "x86_64-pc-windows-msvc", platform: "windows" },
  { target: "aarch64-apple-darwin", platform: "macos" },
  { target: "x86_64-apple-darwin", platform: "macos" },
];
const LICENCE_FILE = /^(LICEN[SC]E|COPYING|COPYRIGHT|NOTICE|UNLICEN[SC]E)([-._].*)?$/i;

const sha256 = (buf) => crypto.createHash("sha256").update(buf).digest("hex");
const norm = (text) => text.replace(/\r\n?/g, "\n").replace(/[ \t]+$/gm, "").replace(/\s+$/, "") + "\n";

function cargoBin() {
  const home = process.env.USERPROFILE || process.env.HOME || "";
  const exe = path.join(home, ".cargo", "bin", process.platform === "win32" ? "cargo.exe" : "cargo");
  return fs.existsSync(exe) ? exe : "cargo";
}

function metadata(target) {
  const args = ["metadata", "--format-version", "1", "--locked", "--offline", "--filter-platform", target];
  const r = spawnSync(cargoBin(), args, { cwd: TAURI, encoding: "utf8", maxBuffer: 256 * 1024 * 1024 });
  if (r.status !== 0) {
    console.error(`cargo metadata failed for ${target}:\n` + (r.stderr || r.error));
    process.exit(2);
  }
  return JSON.parse(r.stdout);
}

function shipped(meta) {
  const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
  const seen = new Set();
  const stack = [meta.resolve.root];
  while (stack.length) {
    const id = stack.pop();
    if (seen.has(id)) continue;
    seen.add(id);
    for (const d of nodes.get(id).deps) {
      if (d.dep_kinds.some((k) => k.kind === null)) stack.push(d.pkg);
    }
  }
  seen.delete(meta.resolve.root);
  return meta.packages.filter((p) => seen.has(p.id));
}

function textsOf(pkg) {
  const dir = path.dirname(pkg.manifest_path);
  const files = fs.readdirSync(dir, { withFileTypes: true })
    .filter((e) => e.isFile() && LICENCE_FILE.test(e.name))
    .map((e) => e.name)
    .sort();
  return files.map((file) => {
    const text = norm(fs.readFileSync(path.join(dir, file), "utf8"));
    return { file, id: sha256(Buffer.from(text, "utf8")).slice(0, 16), text };
  });
}

// Packages that ship without a licence file but declare an SPDX licence whose standard text (no per-project
// copyright line) is carried by another package. The borrowed file is named in the output ("textFrom").
const BORROW = { selectors: { from: "cssparser", spdx: "MPL-2.0" } };

// Packages whose published crate omits the licence file. The upstream repository's licence files were saved
// once (read-only text, fetched as text through the GitHub contents API) under ui/licenses/ and are used as
// their texts. "name" is the file name shown on the Credits page, "url" the upstream file (pinned to a commit
// or tag where the upstream has one), recorded per text in the output as "source". An override applies only
// while the crate still declares exactly the SPDX expression given here.
const WEBVIEW2_RS = [{ file: "webview2-rs-MIT.txt", name: "LICENSE", url: "https://github.com/wravery/webview2-rs/blob/main/LICENSE" }];
// madsmtm/objc2: objc2, block2, objc2-foundation and objc2-encode are MIT; the other crates of the repository
// are Zlib OR Apache-2.0 OR MIT (the repository's LICENSE.md). The texts were added upstream in ee9a7ada2131.
const OBJC2_AT = "https://github.com/madsmtm/objc2/blob/ee9a7ada2131f5944b8750428e265c15632f2a19/";
const OBJC2_MIT = [{ file: "objc2-MIT.txt", name: "LICENSE-MIT.txt", url: OBJC2_AT + "LICENSE-MIT.txt" }];
const OBJC2_TRIO = [
  { file: "objc2-Apache-2.0.txt", name: "LICENSE-APACHE.txt", url: OBJC2_AT + "LICENSE-APACHE.txt" },
  ...OBJC2_MIT,
  { file: "objc2-Zlib.txt", name: "LICENSE-ZLIB.txt", url: OBJC2_AT + "LICENSE-ZLIB.txt" },
];
const TRIO = "Zlib OR Apache-2.0 OR MIT";
const OVERRIDE = {
  "webview2-com": { spdx: "MIT", files: WEBVIEW2_RS },
  "webview2-com-macros": { spdx: "MIT", files: WEBVIEW2_RS },
  "webview2-com-sys": { spdx: "MIT", files: WEBVIEW2_RS },
  "alloc-stdlib": { spdx: "BSD-3-Clause", files: [{ file: "alloc-stdlib-BSD-3-Clause.txt", name: "LICENSE", url: "https://github.com/dropbox/rust-alloc-no-stdlib/blob/master/LICENSE" }] },
  "objc2": { spdx: "MIT", files: OBJC2_MIT },
  "block2": { spdx: "MIT", files: OBJC2_MIT },
  "objc2-encode": { spdx: "MIT", files: OBJC2_MIT },
  "objc2-foundation": { spdx: "MIT", files: OBJC2_MIT },
  "dispatch2": { spdx: TRIO, files: OBJC2_TRIO },
  "objc2-app-kit": { spdx: TRIO, files: OBJC2_TRIO },
  "objc2-core-foundation": { spdx: TRIO, files: OBJC2_TRIO },
  "objc2-core-graphics": { spdx: TRIO, files: OBJC2_TRIO },
  "objc2-exception-helper": { spdx: TRIO, files: OBJC2_TRIO },
  "objc2-io-surface": { spdx: TRIO, files: OBJC2_TRIO },
  "objc2-osa-kit": { spdx: TRIO, files: OBJC2_TRIO },
  "objc2-quartz-core": { spdx: TRIO, files: OBJC2_TRIO },
  "objc2-security": { spdx: TRIO, files: OBJC2_TRIO },
  "objc2-service-management": { spdx: TRIO, files: OBJC2_TRIO },
  "objc2-web-kit": { spdx: TRIO, files: OBJC2_TRIO },
  "mac-notification-sys": {
    spdx: "MIT/Apache-2.0",
    files: [
      { file: "mac-notification-sys-Apache-2.0.txt", name: "LICENSE-Apache", url: "https://github.com/h4llow3En/mac-notification-sys/blob/v0.6.15/LICENSE-Apache" },
      { file: "mac-notification-sys-MIT.txt", name: "LICENSE-MIT", url: "https://github.com/h4llow3En/mac-notification-sys/blob/v0.6.15/LICENSE-MIT" },
    ],
  },
};

function sortedObject(o) {
  return Object.fromEntries(Object.keys(o).sort().map((k) => [k, o[k]]));
}

function build() {
  // name + version -> the package and the platforms whose build includes it
  const merged = new Map();
  for (const { target, platform } of TARGETS) {
    for (const p of shipped(metadata(target))) {
      const key = `${p.name} ${p.version}`;
      const entry = merged.get(key) || { pkg: p, platforms: new Set() };
      entry.platforms.add(platform);
      merged.set(key, entry);
    }
  }
  const entries = [...merged.values()].sort((a, b) => {
    const x = a.pkg, y = b.pkg;
    return x.name < y.name ? -1 : x.name > y.name ? 1 : x.version < y.version ? -1 : x.version > y.version ? 1 : 0;
  });
  const pkgs = entries.map((e) => e.pkg);
  const platformsOf = new Map(entries.map((e) => [`${e.pkg.name} ${e.pkg.version}`, [...e.platforms].sort()]));
  const texts = {};
  let noText = 0;
  const crates = pkgs.map((p) => {
    let t = textsOf(p);
    let textFrom;
    const b = BORROW[p.name];
    if (!t.length && b && p.license === b.spdx) {
      const src = pkgs.find((q) => q.name === b.from && q.license === b.spdx);
      if (src) { t = textsOf(src); textFrom = src.name; }
    }
    const o = OVERRIDE[p.name];
    if (!t.length && o && p.license === o.spdx) {
      t = o.files.map((f) => {
        const text = norm(fs.readFileSync(path.join(OUT, f.file), "utf8"));
        return { file: f.name, id: sha256(Buffer.from(text, "utf8")).slice(0, 16), text, source: f.url };
      });
    }
    if (!t.length) noText++;
    for (const x of t) texts[x.id] = x.text;
    return {
      name: p.name,
      version: p.version,
      license: p.license || (p.license_file ? "see licence file" : ""),
      repository: p.repository || "",
      platforms: platformsOf.get(`${p.name} ${p.version}`),
      texts: t.map((x) => ({ file: x.file, id: x.id, ...(x.source ? { source: x.source } : {}) })),
      ...(textFrom ? { textFrom } : {}),
    };
  });
  // LF-normalised, so a CRLF checkout (core.autocrlf on the Windows CI runner) hashes the same
  const lock = Buffer.from(fs.readFileSync(path.join(TAURI, "Cargo.lock"), "utf8").replace(/\r\n/g, "\n"), "utf8");
  const crateJson = JSON.stringify({ cargo_lock_sha256: sha256(lock), targets: TARGETS.map((t) => t.target), crates }, null, 1) + "\n";
  const textJson = JSON.stringify(sortedObject(texts), null, 1) + "\n";
  const mit = norm(fs.readFileSync(path.join(ROOT, "LICENSE"), "utf8"));
  return {
    files: { "rust-crates.json": crateJson, "rust-license-texts.json": textJson, "app-MIT.txt": mit },
    summary: { crates: crates.length, withoutLicenceFile: noText, texts: Object.keys(texts).length },
  };
}

const { files, summary } = build();
if (process.argv.includes("--check")) {
  let bad = 0;
  for (const [name, content] of Object.entries(files)) {
    const p = path.join(OUT, name);
    const have = fs.existsSync(p) ? fs.readFileSync(p, "utf8").replace(/\r\n/g, "\n") : null;
    if (have !== content) {
      console.error(`${name}: out of date (run: node tools/gen_licenses.js)`);
      bad++;
    }
  }
  if (bad) process.exit(1);
  console.log(`licence files are up to date (${summary.crates} crates)`);
} else {
  fs.mkdirSync(OUT, { recursive: true });
  for (const [name, content] of Object.entries(files)) {
    fs.writeFileSync(path.join(OUT, name), content, { encoding: "utf8" });
    console.log(`${name}: ${Buffer.byteLength(content)} bytes`);
  }
  console.log(`crates: ${summary.crates}, without a licence file: ${summary.withoutLicenceFile}, distinct texts: ${summary.texts}`);
}
