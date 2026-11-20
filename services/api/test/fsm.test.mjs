// The server's state machine must be byte-for-byte behaviourally identical to
// the Rust one. Both are checked against the same fixture.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { applyEvent } from "../src/fsm.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const FIXTURE = JSON.parse(
  readFileSync(join(HERE, "..", "..", "..", "packages", "protocol", "fixtures", "transmission_fsm.json"), "utf8"),
);

test("every fixture case matches the ported machine", () => {
  let checked = 0;
  for (const c of FIXTURE.cases) {
    const session = {
      state: c.from,
      initiatorConfirmed: false,
      receiverConfirmed: false,
      version: 1,
    };
    // The Rust fixture primes the confirmation that the event implies.
    if (c.from === "RECEIVED" && c.event === "ACCEPT") session.initiatorConfirmed = true;
    if (c.from === "RECEIVED" && c.event === "CONFIRM") session.receiverConfirmed = true;

    const out = applyEvent(session, c.event, {
      nowMs: 1_000_100,
      expiresAtMs: 1_600_000,
      activationEvidenceOk: c.evidence,
    });

    const label = `${c.from} --${c.event}-- evidence=${c.evidence}`;
    if (c.result === "ok") {
      assert.equal(out.ok, true, `${label}: expected ok, got ${JSON.stringify(out)}`);
      assert.equal(out.state, c.next, `${label}: state`);
      assert.deepEqual(out.effects, c.effects, `${label}: effects`);
      assert.equal(out.replayed, c.replayed, `${label}: replayed`);
    } else {
      assert.equal(out.ok, false, `${label}: expected error ${c.error}, got ${JSON.stringify(out)}`);
      assert.equal(out.error, c.error, `${label}: error kind`);
    }
    checked += 1;
  }
  assert.equal(checked, FIXTURE.cases.length);
  assert.ok(checked >= 200, "the fixture should cover every state/event pair");
});

test("RECEIVED can never jump to ACTIVATED", () => {
  const session = { state: "RECEIVED", initiatorConfirmed: true, receiverConfirmed: true, version: 1 };
  const out = applyEvent(session, "ACTIVATE", { nowMs: 0, expiresAtMs: 1e9, activationEvidenceOk: true });
  assert.equal(out.ok, false);
  assert.equal(out.error, "IllegalTransition");
});

test("both confirmations are required before a relation exists", () => {
  const ctx = { nowMs: 0, expiresAtMs: 1e9, activationEvidenceOk: false };
  const session = { state: "RECEIVED", initiatorConfirmed: false, receiverConfirmed: false, version: 1 };
  const afterAccept = applyEvent(session, "ACCEPT", ctx);
  assert.equal(afterAccept.state, "RECEIVED", "receiver alone must not form a relation");
  const afterConfirm = applyEvent(session, "CONFIRM", ctx);
  assert.equal(afterConfirm.state, "LEARNING");
  assert.deepEqual(afterConfirm.effects, ["EdgeFormed", "TokenInvalidated"]);
});

test("activation requires evidence", () => {
  const session = { state: "LEARNING", initiatorConfirmed: true, receiverConfirmed: true, version: 3 };
  const denied = applyEvent(session, "ACTIVATE", { nowMs: 0, expiresAtMs: 1e9, activationEvidenceOk: false });
  assert.equal(denied.error, "ActivationEvidenceMissing");
  const allowed = applyEvent(session, "ACTIVATE", { nowMs: 0, expiresAtMs: 1e9, activationEvidenceOk: true });
  assert.equal(allowed.state, "ACTIVATED");
  assert.deepEqual(allowed.effects, ["EdgeActivated"]);
});
