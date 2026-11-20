// M1 acceptance: a valid signature must never be mistaken for a true statement.
import { test } from "node:test";
import assert from "node:assert/strict";
import { createApp } from "../src/app.mjs";
import { openDb } from "../src/db.mjs";
import { makeClient } from "./support/client.mjs";
import { generateKeyPairSync, sign as edSign, randomBytes } from "node:crypto";
import {
  PROTOCOL_VERSION, SOURCE, SIG, canonicalize, signingInput, verifyCredential, validateShape,
} from "../src/credential.mjs";

process.env.FL_VERIFIER_V2 = "on";

async function setup() {
  const db = await openDb();
  const app = createApp({ db, nowMs: () => Date.now() });
  const alice = await makeClient(app);
  const bob = await makeClient(app);
  return { db, app, alice, bob };
}

function newDeviceKey() {
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  const raw = publicKey.export({ format: "der", type: "spki" }).subarray(-32);
  return { pub: raw, priv: privateKey };
}

function makeCredential(key, overrides = {}) {
  const base = {
    protocol_version: PROTOCOL_VERSION,
    credential_id: `cred_${randomBytes(8).toString("hex")}`,
    issuer_key_id: "dev1",
    subject_identity_id: "fl1_placeholder",
    source_type: SOURCE.PLUGIN_OBSERVED,
    adapter_id: "ollama-proxy",
    adapter_version: "0.1.0",
    model_family: "gpt",
    observation_method: "http_response_observed",
    observed_at: Date.now(),
    issued_at: Date.now(),
    ...overrides,
  };
  const sig = edSign(null, signingInput(base), key.priv);
  return { ...base, signature: sig.toString("hex") };
}

test("A1 · a correctly signed credential is accepted", async () => {
  const { app, alice } = await setup();
  const key = newDeviceKey();
  await alice.call("POST", "/v1/keys/register", { key_id: "dev1", pubkey_hex: key.pub.toString("hex"), adapter_id: "ollama-proxy" });
  const cred = makeCredential(key, { subject_identity_id: alice.identity.anonId });
  const r = await alice.call("POST", "/v1/usage/attest-v2", { credential: cred });
  assert.equal(r.status, 201);
  assert.equal(r.json.signature_status, SIG.VALID);
  assert.equal(r.json.source_class, SOURCE.PLUGIN_OBSERVED);
});

test("A2 · tampering with any signed field is rejected", async () => {
  const { app, alice } = await setup();
  const key = newDeviceKey();
  await alice.call("POST", "/v1/keys/register", { key_id: "dev1", pubkey_hex: key.pub.toString("hex") });
  for (const field of ["model_family", "source_type", "observed_at", "adapter_id"]) {
    const cred = makeCredential(key, { subject_identity_id: alice.identity.anonId });
    cred[field] = field === "observed_at" ? cred[field] + 1 : cred[field] + "x";
    const r = await alice.call("POST", "/v1/usage/attest-v2", { credential: cred });
    const rejected = r.status === 422 || r.json.signature_status === SIG.INVALID;
    assert.ok(rejected, `${field} must not be accepted`);
    assert.notEqual(r.json.accepted, true);
  }
});

test("A3 · an unknown model family fails shape validation", () => {
  const key = newDeviceKey();
  const cred = makeCredential(key, { model_family: "not a slug!" });
  assert.throws(() => validateShape(cred), /BAD_FIELD/);
});

test("A4 · an unregistered key is refused before any crypto", async () => {
  const { app, alice } = await setup();
  const key = newDeviceKey();
  const cred = makeCredential(key, { subject_identity_id: alice.identity.anonId });
  const r = await alice.call("POST", "/v1/usage/attest-v2", { credential: cred });
  assert.equal(r.status, 422);
  assert.equal(r.json.error.code, "KEY_UNKNOWN");
});

test("A5 · a revoked key cannot sign anything", async () => {
  const { app, alice } = await setup();
  const key = newDeviceKey();
  await alice.call("POST", "/v1/keys/register", { key_id: "dev1", pubkey_hex: key.pub.toString("hex") });
  await alice.call("POST", "/v1/keys/revoke", { key_id: "dev1" });
  const cred = makeCredential(key, { subject_identity_id: alice.identity.anonId });
  const r = await alice.call("POST", "/v1/usage/attest-v2", { credential: cred });
  assert.equal(r.json.signature_status, SIG.REVOKED);
  assert.equal(r.json.accepted, false);
});

test("A6 · the counterexample: attacker's own key signs a GPT claim", async () => {
  const ctx = await setup();
  const { app, alice: mallory } = ctx;
  const key = newDeviceKey();                       // attacker-generated, never audited
  await mallory.call("POST", "/v1/keys/register", { key_id: "evil", pubkey_hex: key.pub.toString("hex") });
  const cred = makeCredential(key, {
    issuer_key_id: "evil",                          // must match the registered id
    subject_identity_id: mallory.identity.anonId,
    source_type: SOURCE.SIGNED_SELF_CLAIM,          // honest about being a self claim
    model_family: "gpt",
  });
  const r = await mallory.call("POST", "/v1/usage/attest-v2", { credential: cred });
  // cryptographic integrity holds...
  assert.equal(r.json.signature_status, SIG.VALID);
  assert.equal(r.json.accepted, true);
  // ...and it still may not activate a transmission on the open network
  assert.equal(r.json.eligible.activate, false);
  assert.equal(r.json.eligible.public_field, false);
});

test("A7 · a self claim that lies about its source_type still never activates", async () => {
  const { app, alice } = await setup();
  const key = newDeviceKey();
  await alice.call("POST", "/v1/keys/register", { key_id: "dev1", pubkey_hex: key.pub.toString("hex") });
  // claiming PLUGIN_OBSERVED with no challenge and no binding is still unbound
  const cred = makeCredential(key, { subject_identity_id: alice.identity.anonId, source_type: SOURCE.PLUGIN_OBSERVED });
  const r = await alice.call("POST", "/v1/usage/attest-v2", { credential: cred });
  // M2.2: the class is no longer activatable at policy level at all — a plugin
  // observation is not independent corroboration, however it labels itself.
  assert.equal(r.json.eligible.activate, false);
});

test("B1 · replaying the same credential is idempotent, not double counted", async () => {
  const { app, alice } = await setup();
  const key = newDeviceKey();
  await alice.call("POST", "/v1/keys/register", { key_id: "dev1", pubkey_hex: key.pub.toString("hex") });
  const cred = makeCredential(key, { subject_identity_id: alice.identity.anonId });
  const first = await alice.call("POST", "/v1/usage/attest-v2", { credential: cred });
  const second = await alice.call("POST", "/v1/usage/attest-v2", { credential: cred });
  assert.equal(first.json.accepted, true);
  assert.equal(second.json.duplicate, true);
});

test("B2 · concurrent identical submissions insert exactly one row", async () => {
  const { app, alice, db } = await setup();
  const key = newDeviceKey();
  await alice.call("POST", "/v1/keys/register", { key_id: "dev1", pubkey_hex: key.pub.toString("hex") });
  const cred = makeCredential(key, { subject_identity_id: alice.identity.anonId });
  const results = await Promise.all(
    Array.from({ length: 6 }, () => alice.call("POST", "/v1/usage/attest-v2", { credential: cred })),
  );
  const accepted = results.filter((r) => r.json.accepted).length;
  assert.equal(accepted, 1, "exactly one insert may win");
  const rows = await db.all("select count(*)::int as n from credential where id=$1", [cred.credential_id]);
  assert.equal(rows[0].n, 1);
});

test("B3 · an old observation cannot be dressed up as fresh", async () => {
  const key = newDeviceKey();
  const old = Date.now() - 24 * 3600 * 1000;
  const cred = makeCredential(key, { observed_at: old, issued_at: old });
  const verdict = verifyCredential({
    payload: cred, publicKeyHex: key.pub.toString("hex"), keyStatus: "ACTIVE", nowMs: Date.now(),
  });
  assert.equal(verdict.signatureStatus, SIG.VALID);
  assert.ok(verdict.reasons.includes("CREDENTIAL_TOO_OLD"));
});

test("B4 · subject mismatch is refused", async () => {
  const { app, alice, bob } = await setup();
  const key = newDeviceKey();
  await alice.call("POST", "/v1/keys/register", { key_id: "dev1", pubkey_hex: key.pub.toString("hex") });
  const cred = makeCredential(key, { subject_identity_id: bob.identity.anonId });
  const r = await alice.call("POST", "/v1/usage/attest-v2", { credential: cred });
  assert.equal(r.status, 422);
  assert.equal(r.json.error.code, "SUBJECT_MISMATCH");
});

test("C1 · canonical form is stable and rejects floats", () => {
  assert.equal(canonicalize({ b: 1, a: "x" }), '{"a":"x","b":1}');
  assert.throws(() => canonicalize({ a: 1.5 }), /JCS_SUBSET/);
});

test("C2 · unknown fields are rejected, not silently dropped", () => {
  const key = newDeviceKey();
  const cred = makeCredential(key, { prompt_text: "secret" });
  assert.throws(() => validateShape(cred), /UNKNOWN_FIELD/);
});
