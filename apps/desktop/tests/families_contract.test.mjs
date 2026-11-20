// The contract between the Rust payload and the model rows, plus the rule that no
// invalid text may ever reach the DOM.
import { test } from "node:test";
import assert from "node:assert/strict";
import { createRequire } from "node:module";
const require = createRequire(import.meta.url);
const { normalize, INVALID } = require("../ui/families.js");

const FV = (o) => ({ raw_models: [], sessions: 1, ...o });

test("A · absent families is empty, not an error", () => {
  for (const v of [null, undefined]) {
    const r = normalize(v);
    assert.deepEqual(r.verified, []);
    assert.equal(r.error, false);
  }
});

test("B · one countable family maps to a verified row", () => {
  const r = normalize([FV({ family: "gpt", level: "E3", countable: true })]);
  assert.equal(r.verified.length, 1);
  assert.equal(r.verified[0].familyId, "gpt");
  assert.equal(r.verified[0].verificationStatus, "verified");
  assert.equal(r.verified[0].evidenceLevel, "E3");
});

test("C · two countable families stay two rows", () => {
  const r = normalize([
    FV({ family: "gpt", countable: true }), FV({ family: "claude", countable: true }),
  ]);
  assert.deepEqual(r.verified.map((f) => f.familyId).sort(), ["claude", "gpt"]);
});

test("D · discovered-only families are separated, never counted as verified", () => {
  const r = normalize([FV({ family: "deepseek", level: "E1", countable: false })]);
  assert.equal(r.verified.length, 0);
  assert.equal(r.discovered.length, 1);
  assert.equal(r.discovered[0].verificationStatus, "discovered");
});

test("E · malformed entries raise the error flag instead of inventing a name", () => {
  const cases = [
    ["gpt"],                                  // string element: old broken shape
    [{ level: "E3", countable: true }],       // no family field
    [{ family: null }], [{ family: "  " }],
    [{ family: "undefined" }],                // poisoned string
    "just-a-string", 42, {}, [[]],
  ];
  for (const c of cases) {
    const r = normalize(c);
    assert.equal(r.error, true, `expected error for ${JSON.stringify(c)}`);
    assert.equal(r.verified.length, 0);
    assert.equal(r.discovered.length, 0);
  }
});

test("F · nothing normalised can render invalid text", () => {
  const r = normalize([FV({ family: "gpt", countable: true }), FV({ family: "x", countable: false })]);
  for (const f of [...r.verified, ...r.discovered]) {
    assert.ok(!INVALID.test(f.displayName), f.displayName);
    assert.equal(typeof f.displayName, "string");
  }
});

test("G · countable must be exactly true, not truthy", () => {
  const r = normalize([FV({ family: "gpt", countable: 1 })]);
  assert.equal(r.verified.length, 0);
  assert.equal(r.discovered.length, 1);
});

test("H · the field the UI reads is the field Rust writes", async () => {
  const { readFileSync } = await import("node:fs");
  const rs = readFileSync(new URL("../src-tauri/src/scan.rs", import.meta.url), "utf8");
  const m = rs.match(/pub struct FamilyView \{([\s\S]*?)\}/);
  assert.ok(m, "FamilyView not found");
  const fields = [...m[1].matchAll(/pub\s+(\w+)\s*:/g)].map((x) => x[1]);
  for (const f of ["family", "level", "sessions", "countable"]) {
    assert.ok(fields.includes(f), `FamilyView must expose ${f}`);
  }
  assert.ok(!fields.includes("name"), "FamilyView has no `name`; the UI must not read one");
});
