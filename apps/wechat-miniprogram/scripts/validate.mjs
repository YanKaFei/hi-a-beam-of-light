#!/usr/bin/env node
// Static structural validation. NOT the devtools compiler, and never to be reported as
// a successful compile. It only catches what would waste a human's time.
import { readFileSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const problems = [];
const ok = [];
const readJSON = (p) => { try { return JSON.parse(readFileSync(p, "utf8")); } catch (e) { problems.push(`${p}: ${e.message}`); return null; } };

const app = readJSON(join(ROOT, "app.json"));
if (app) {
  for (const page of app.pages ?? []) {
    const base = join(ROOT, page);
    for (const ext of [".js", ".json", ".wxml"]) if (!existsSync(base + ext)) problems.push(`missing ${page}${ext}`);
    if (existsSync(base + ".json")) readJSON(base + ".json");
    const wxml = existsSync(base + ".wxml") ? readFileSync(base + ".wxml", "utf8") : "";
    const js = existsSync(base + ".js") ? readFileSync(base + ".js", "utf8") : "";
    for (const m of wxml.matchAll(/bind(?:tap|input|change|confirm|submit)="([A-Za-z0-9_]+)"/g)) {
      if (!new RegExp(`\\b${m[1]}\\s*\\(`).test(js)) problems.push(`${page}: handler ${m[1]} not defined`);
    }
    if (/\{\{\s*undefined\s*\}\}/.test(wxml)) problems.push(`${page}: literal undefined in WXML`);
  }
  for (const item of app.tabBar?.list ?? []) if (!(app.pages ?? []).includes(item.pagePath)) problems.push(`tabBar page not in pages: ${item.pagePath}`);
  ok.push(`pages: ${(app.pages ?? []).length}`);
}
for (const f of ["app.js", "app.wxss", "sitemap.json", "project.config.json"]) if (!existsSync(join(ROOT, f))) problems.push(`missing ${f}`);
// Only real assignments count: a comment saying "no AppSecret here" is not a leak.
const stripComments = (src) => src.replace(/\/\*[\s\S]*?\*\//g, "").replace(/(^|[^:])\/\/.*$/gm, "$1");
for (const f of ["app.js", "utils/api.js", "utils/auth.js", "pages/mine/mine.js", "pages/settings/settings.js"]) {
  const src = stripComments(readFileSync(join(ROOT, f), "utf8"));
  for (const banned of [/app_?secret/i, /session_?key/i, /private_?key/i]) {
    if (banned.test(src)) problems.push(`${f}: ${banned} appears outside a comment`);
  }
}
ok.push("no secrets in client code");
if (problems.length) { console.error("STATIC_VALIDATION_FAIL"); for (const p of problems) console.error("  -", p); process.exit(1); }
console.log("STATIC_VALIDATION_PASS (not a devtools compile)");
for (const o of ok) console.log("  ·", o);
