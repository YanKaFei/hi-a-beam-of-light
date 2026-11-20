// Selector integrity. The F3.7-R1 bug was a CSS rule pointing at `#mine` while the
// section is `#v-mine`: the rule matched nothing and every layout assertion in the
// stylesheet was silently dead. This test fails when a layout-critical selector has
// no matching element, and it refuses to accept a broader selector as a substitute.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const HERE = dirname(fileURLToPath(import.meta.url));
const HTML = readFileSync(join(HERE, "..", "ui", "panel.html"), "utf8");

const IDS = HTML.match(new RegExp('id="([a-z0-9-]+)"', "g")) || [];
const PRESENT = new Set(IDS.map((m) => m.slice(4, -1)));

const REQUIRED = {
  "我的光主区域": "v-mine",
  "传光主区域": "v-give",
  "光路主区域": "v-paths",
  "回响主区域": "v-echo",
  "导航栏": null,          // checked structurally below
  "主操作按钮": "m-primary",
  // The B+ panel no longer hosts the light: it lives on the desktop overlay,
  // which is verified by the overlay probe and the bead-render tests.
};
for (const [label, id] of Object.entries(REQUIRED)) {
  if (!id) continue;
  test(`布局选择器存在：${label} (#${id})`, () => {
    assert.ok(PRESENT.has(id), `缺少元素 #${id}（${label}）`);
  });
}

test("导航栏存在且含四个入口", () => {
  const m = HTML.match(/<nav[^>]*>([\s\S]*?)<\/nav>/);
  assert.ok(m, "缺少 <nav>");
  const labels = ["我的光", "传光", "光路", "回响"];
  for (const l of labels) assert.ok(m[1].includes(l), `导航缺少「${l}」`);
});

test("每个 section 的 id 都以 v- 开头，避免样式与 DOM 命名漂移", () => {
  const sections = [...HTML.matchAll(/<section[^>]*id="([a-z0-9-]+)"/g)].map((m) => m[1]);
  assert.equal(sections.length, 4);
  for (const s of sections) assert.match(s, /^v-/, `section id ${s} 不以 v- 开头`);
});

test("样式表里出现的 #id 选择器，必须能匹配到真实元素", () => {
  const css = HTML.slice(HTML.indexOf("<style>"), HTML.indexOf("</style>"));
  // `#fff` is a colour, not a selector: drop tokens that are pure hex.
  const raw = [...css.matchAll(/#([a-z][a-z0-9-]*)/g)].map((m) => m[1]);
  const ids = new Set(raw.filter((t) => !/^[0-9a-f]{3,8}$/.test(t)));
  const missing = [...ids].filter((id) => !PRESENT.has(id));
  assert.deepEqual(missing, [], `样式引用了不存在的 id: ${missing.join(", ")}`);
});
