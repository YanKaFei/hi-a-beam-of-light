// Cross-platform conformance test.
//
// The Rust reference implementation writes these fixtures
// (`cargo run -p light-core --example emit_contract`) and validates them again in
// `crates/light-core/tests/contract.rs`. This suite independently proves that a
// second implementation of the same rules produces identical answers from the
// same inputs. Drift fails one side or the other.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

import {
  KNOWLEDGE_LADDER,
  TRANSMISSION_LADDER,
  computeLightState,
  evaluateEvidence,
  knowledgeTitle,
  lightLevel,
  loadRegistry,
  normalize,
  provesCall,
  transmissionTitle,
} from "../src/rules.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const PKG = join(HERE, "..");
const FIXTURES = join(PKG, "fixtures");
const SCHEMAS = join(PKG, "schemas");

const readFixture = (name) => JSON.parse(readFileSync(join(FIXTURES, name), "utf8"));

test("schemas are present and forbid unknown properties where it matters", () => {
  const files = readdirSync(SCHEMAS).filter((f) => f.endsWith(".schema.json"));
  assert.ok(files.length >= 11, `expected >= 11 schemas, found ${files.length}`);

  const attest = JSON.parse(readFileSync(join(SCHEMAS, "usage_attest_request.schema.json"), "utf8"));
  assert.equal(attest.additionalProperties, false, "attest request must reject unknown fields");

  // The attest schema must not be able to carry consumption counters.
  const serialized = JSON.stringify(attest);
  for (const forbidden of ["tokens_used", "prompt", "api_key"]) {
    assert.ok(!serialized.includes(forbidden), `schema must not mention ${forbidden}`);
  }
});

test("knowledge ladder matches the fixture for every count", () => {
  const fixture = readFixture("titles.json");
  assert.equal(fixture.knowledge.length, 13);
  for (const row of fixture.knowledge) {
    const got = knowledgeTitle(row.families);
    assert.equal(got.slug, row.slug, `families=${row.families}`);
    assert.equal(got.zh, row.zh, `families=${row.families}`);
  }
});

test("transmission ladder matches the fixture including thresholds with no tier of their own", () => {
  const fixture = readFixture("titles.json");
  for (const row of fixture.transmission) {
    const got = transmissionTitle(row.nodes);
    assert.equal(got.slug, row.slug, `nodes=${row.nodes}`);
    assert.equal(got.zh, row.zh, `nodes=${row.nodes}`);
  }
  // 2 is between the 1 and 3 thresholds and must stay at the lower tier.
  assert.equal(transmissionTitle(2).slug, "kindler");
});

test("light level matches the fixture", () => {
  const fixture = readFixture("titles.json");
  for (const row of fixture.levels) {
    assert.equal(lightLevel(row.families), row.level, `families=${row.families}`);
  }
});

test("ladders are strictly increasing and start at zero", () => {
  for (const ladder of [KNOWLEDGE_LADDER, TRANSMISSION_LADDER]) {
    assert.equal(ladder[0][0], 0);
    for (let i = 1; i < ladder.length; i += 1) {
      assert.ok(ladder[i][0] > ladder[i - 1][0], "thresholds must increase");
    }
  }
});

test("registry normalization matches the fixture and never guesses", () => {
  const fixture = readFixture("registry_normalization.json");
  const registry = loadRegistry();
  assert.equal(registry.semver, fixture.registry_semver);

  for (const c of fixture.cases) {
    const got = normalize(registry, c.raw);
    assert.equal(got.disposition.kind, c.disposition.kind, `raw=${JSON.stringify(c.raw)}`);
    assert.equal(got.family, c.family, `raw=${JSON.stringify(c.raw)}`);
    assert.equal(got.client, c.client, `raw=${JSON.stringify(c.raw)}`);
  }
});

test("the F0 sample set collapses to exactly one family", () => {
  // Acceptance scenario A, checked from the TypeScript side as well.
  const registry = loadRegistry();
  const raws = [
    "gpt-6-sol",
    "gpt-5.6-sol",
    "gpt-5.6-terra",
    "gpt-6.1-sol",
    "gpt-6-astra",
    "chatgpt-web/gpt-6-sol",
    "chatgpt-web/gpt-5.6-sol",
    "codex-auto-review",
    "",
  ];
  const families = new Set();
  for (const raw of raws) {
    const n = normalize(registry, raw);
    if (n.disposition.kind === "family") families.add(n.family);
  }
  assert.equal(raws.length, 9);
  assert.equal(families.size, 1);
  assert.ok(families.has("gpt"));
});

test("evidence decisions match the fixture", () => {
  const fixture = readFixture("evidence_decisions.json");
  const registry = loadRegistry();

  const decisionName = (result) => result.decision;

  for (const c of fixture.cases) {
    const proof = { output_items: c.output_items, completed_turns: c.completed_turns };
    const got = evaluateEvidence(registry, c.raw, proof);
    assert.equal(decisionName(got), c.decision, `raw=${JSON.stringify(c.raw)}`);

    if (c.decision === "countable") {
      assert.equal(got.family, c.detail.family);
      assert.equal(got.level, c.detail.level);
    }
    if (c.decision === "observable") {
      assert.equal(got.level, c.detail.level);
    }
  }
});

test("completed turns alone never prove a call", () => {
  const registry = loadRegistry();
  assert.equal(provesCall({ output_items: 0, completed_turns: 99 }), false);
  const got = evaluateEvidence(registry, "gpt-6-sol", { output_items: 0, completed_turns: 99 });
  assert.equal(got.decision, "observable");
});

test("output item taxonomy does not overlap the non-proof list", () => {
  const fixture = readFixture("evidence_decisions.json");
  const output = new Set(fixture.policy.model_output_item_types);
  for (const t of fixture.policy.non_proof_item_types) {
    assert.ok(!output.has(t), `${t} appears in both taxonomies`);
  }
  for (const t of ["userMessage", "imageView", "sleep", "contextCompaction"]) {
    assert.ok(fixture.policy.non_proof_item_types.includes(t), `${t} must be excluded`);
  }
});

test("E2 and received light never change the core", () => {
  const fixture = readFixture("light_state.json");
  for (const c of fixture.cases) {
    const got = computeLightState({
      verified: c.verified_families,
      attested: c.attested_families,
      received: c.received_glow,
      nodes: c.activated_nodes,
    });
    assert.equal(got.level, c.expected.level, JSON.stringify(c));
    assert.equal(got.knowledge_title, c.expected.knowledge_title, JSON.stringify(c));
    assert.equal(got.transmission_title, c.expected.transmission_title, JSON.stringify(c));
  }
});

test("a node with 7 attested but 0 verified families stays a seeker", () => {
  const got = computeLightState({ verified: 0, attested: 7, received: 0, nodes: 0 });
  assert.equal(got.level, "DORMANT");
  assert.equal(got.knowledge_title, "seeker");
  assert.equal(got.attested_families, 7);
});
