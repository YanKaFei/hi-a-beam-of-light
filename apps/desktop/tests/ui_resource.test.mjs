// Proves the embedded UI is the one in source, without a hand-maintained version
// string. build.rs hashes exactly the files it embeds and writes the digest to
// target/ui-hash.txt; this test recomputes it the same way.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, existsSync, readdirSync, statSync } from "node:fs";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";
import { dirname, join, relative } from "node:path";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = join(HERE, "..", "..", "..");
const UI = join(ROOT, "apps", "desktop", "ui");
const HASH_FILE = join(ROOT, "target", "ui-hash.txt");

function walk(dir, out = []) {
  for (const e of readdirSync(dir)) {
    const p = join(dir, e);
    if (statSync(p).isDirectory()) walk(p, out); else out.push(p);
  }
  return out;
}

function digest() {
  const h = createHash("sha256");
  for (const f of walk(UI).sort()) {
    h.update(relative(UI, f));
    h.update(Buffer.from([0]));
    h.update(readFileSync(f));
    h.update(Buffer.from([0]));
  }
  return h.digest("hex");
}

test("build.rs declares a dependency on every embedded UI file", () => {
  const rs = readFileSync(join(ROOT, "apps", "desktop", "src-tauri", "build.rs"), "utf8");
  assert.match(rs, /rerun-if-changed=\{\}/, "must emit one line per file");
  assert.match(rs, /collect\(/, "must walk the UI directory");
});

test("the built binary's UI digest equals the digest of current sources", {
  skip: !existsSync(HASH_FILE) && "no build hash yet",
}, () => {
  const built = readFileSync(HASH_FILE, "utf8").trim();
  assert.equal(digest(), built,
    "the embedded UI differs from source — rebuild before trusting any screenshot");
});

test("panel.html loads the shared contract module", () => {
  const html = readFileSync(join(UI, "panel.html"), "utf8");
  assert.match(html, /<script src="families\.js">/);
});
