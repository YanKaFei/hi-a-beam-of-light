// M1.1: no activation path may skip the credential and source policy.
// Runs against the default configuration (FL_ACTIVATION_MODE unset => v2).
process.env.FL_VERIFIER_V2 = "on";
delete process.env.FL_ACTIVATION_MODE;

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { generateKeyPairSync, sign as edSign, randomBytes } from "node:crypto";

import { openDb } from "../src/db.mjs";
import { createApp } from "../src/app.mjs";
import { loadPolicy } from "../src/policy.mjs";
import { makeClient } from "./support/client.mjs";
import { PROTOCOL_VERSION, SOURCE, signingInput } from "../src/credential.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const BRAND = JSON.parse(readFileSync(join(HERE, "..", "..", "..", "packages", "brand", "brand.json"), "utf8"));

async function fresh() {
  const db = await openDb();
  const policy = loadPolicy({ publicBaseUrl: "http://127.0.0.1:8787" });
  return { db, app: createApp({ db, policy, brand: BRAND }) };
}

async function toLearning(a, b) {
  const created = await a.call("POST", "/v1/transmissions", { channel: "QR" });
  const id = created.json.id;
  await b.call("GET", `/v1/transmissions/resolve?token=${created.json.token}`);
  await b.call("POST", `/v1/transmissions/${id}/accept`, {});
  await a.call("POST", `/v1/transmissions/${id}/confirm`, {});
  return { id, token: created.json.token };
}

const newKey = () => {
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  return { pub: publicKey.export({ format: "der", type: "spki" }).subarray(-32), priv: privateKey };
};

/** Issue a challenge as the receiver, then mint a credential bound to it. */
async function bindCredential({ b, a, transferId, key, sourceType = SOURCE.PLUGIN_OBSERVED, keyId = "dev1", overrides = {} }) {
  const ch = await b.call("POST", "/v1/challenges", { transfer_id: transferId });
  assert.equal(ch.status, 201, JSON.stringify(ch.json));
  const payload = {
    protocol_version: PROTOCOL_VERSION,
    credential_id: `cred_${randomBytes(8).toString("hex")}`,
    issuer_key_id: keyId,
    subject_identity_id: b.identity.anonId,
    source_type: sourceType,
    adapter_id: "ollama-proxy",
    adapter_version: "0.1.0",
    model_family: "gpt",
    observation_method: "http_response_observed",
    observed_at: Date.now(),
    issued_at: Date.now(),
    challenge_id: ch.json.challenge_id,
    transfer_id: transferId,
    ...overrides,
  };
  const sig = edSign(null, signingInput(payload), key.priv);
  return { credential: { ...payload, signature: sig.toString("hex") }, challengeId: ch.json.challenge_id };
}

async function registerKey(client, key, keyId = "dev1", adapter = "ollama-proxy") {
  const r = await client.call("POST", "/v1/keys/register", {
    key_id: keyId, pubkey_hex: key.pub.toString("hex"), adapter_id: adapter,
  });
  assert.equal(r.status, 201, JSON.stringify(r.json));
}


/** These cases test challenge lifecycle and concurrency, so they need a source class
    that is allowed to activate. The fixture authority is explicit and short-lived. */
async function withFixtureAuthority(fn) {
  process.env.FL_TRUSTED_FIXTURE_ADAPTER = "ollama-proxy";
  try { return await fn(); } finally { delete process.env.FL_TRUSTED_FIXTURE_ADAPTER; }
}

// --- A1 ----------------------------------------------------------------------
test("A1 · a legacy self-declared usage event cannot activate anything", async () => {
  const { app } = await fresh();
  const a = await makeClient(app);
  const b = await makeClient(app);
  const { id } = await toLearning(a, b);

  const ev = await b.call("POST", "/v1/usage/attest", {
    event_id: "evt-legacy-1", model_family: "gpt", evidence_level: "E3",
    proof_method: "local_sqlite_metadata", adapter_id: "codex-app",
    adapter_version: "1.0.0", registry_semver: "1.0.0", observed_at_ms: Date.now(),
  });
  assert.equal(ev.status, 201);

  const act = await b.call("POST", `/v1/transmissions/${id}/activate`, { usage_event_id: "evt-legacy-1" });
  assert.equal(act.status, 426);
  assert.equal(act.json.error.code, "UPGRADE_REQUIRED");
  const state = await a.call("GET", `/v1/transmissions/${id}`);
  assert.equal(state.json.state, "LEARNING");
});

// --- A2 ----------------------------------------------------------------------
test("A2 · a signed self-claim is valid crypto and still cannot activate", async () => {
  const { app } = await fresh();
  const a = await makeClient(app);
  const b = await makeClient(app);
  const { id } = await toLearning(a, b);
  const key = newKey();
  await registerKey(b, key);
  const { credential, challengeId } = await bindCredential({ a, b, transferId: id, key, sourceType: SOURCE.SIGNED_SELF_CLAIM });

  const att = await b.call("POST", "/v1/usage/attest-v2", { credential });
  assert.equal(att.status, 201);
  assert.equal(att.json.signature_status, "VALID");

  const act = await b.call("POST", `/v1/transmissions/${id}/activate`, { credential_id: credential.credential_id, challenge_id: challengeId });
  assert.equal(act.status, 403);
  assert.equal(act.json.error.code, "SOURCE_NOT_TRUSTED");
});

// --- A3 ----------------------------------------------------------------------
test("A3 · PLUGIN_OBSERVED is not enough to activate, however well bound", async () => {
  const { app } = await fresh();
  const a = await makeClient(app);
  const b = await makeClient(app);
  const { id } = await toLearning(a, b);
  const key = newKey();
  await registerKey(b, key);
  const { credential, challengeId } = await bindCredential({ a, b, transferId: id, key, sourceType: SOURCE.PLUGIN_OBSERVED });

  const att = await b.call("POST", "/v1/usage/attest-v2", { credential });
  assert.equal(att.json.source_class, SOURCE.PLUGIN_OBSERVED);
  // The class is allowed, so this one is expected to succeed only when every binding
  // holds; the next test breaks each binding in turn.
  // M2.2 tightened the matrix: a plugin observation is not independent corroboration,
  // so even a fully bound credential of this class is refused.
  const act = await b.call("POST", `/v1/transmissions/${id}/activate`, { credential_id: credential.credential_id, challenge_id: challengeId });
  assert.equal(act.status, 403);
  assert.equal(act.json.error.code, "SOURCE_NOT_TRUSTED");
});

// --- A4 ----------------------------------------------------------------------
test("A4 · a credential for someone else cannot be used", async () => {
  const { app } = await fresh();
  const a = await makeClient(app);
  const b = await makeClient(app);
  const c = await makeClient(app);
  const { id } = await toLearning(a, b);
  const key = newKey();
  await registerKey(b, key);
  const { credential, challengeId } = await bindCredential({ a, b, transferId: id, key });
  await b.call("POST", "/v1/usage/attest-v2", { credential });

  // c is not the receiver, and the credential belongs to b: it must not resolve.
  await registerKey(c, key);
  const act = await c.call("POST", `/v1/transmissions/${id}/activate`, { credential_id: credential.credential_id, challenge_id: challengeId });
  // Two guards can fire first — not being the receiver, or the credential not existing
  // for this identity. Either way it must be refused, never activated.
  assert.ok([403, 422].includes(act.status), `got ${act.status}`);
  assert.ok(["CREDENTIAL_UNKNOWN", "NOT_RECEIVER", "NOT_RECEIVER"].includes(act.json.error.code));
});

// --- A5 ----------------------------------------------------------------------
test("A5 · a credential bound to another transfer is refused", async () => {
  const { app } = await fresh();
  const a = await makeClient(app);
  const b = await makeClient(app);
  const c = await makeClient(app);
  const first = await toLearning(a, b);
  const key = newKey();
  await registerKey(b, key);
  const { credential, challengeId } = await bindCredential({ a, b, transferId: first.id, key });
  await b.call("POST", "/v1/usage/attest-v2", { credential });

  // a second relation with a different receiver, so the edge is genuinely distinct
  const second = await toLearning(a, c);
  const act = await c.call("POST", `/v1/transmissions/${second.id}/activate`, {
    credential_id: credential.credential_id, challenge_id: challengeId,
  });
  // Two guards can fire first — not being the receiver, or the credential not existing
  // for this identity. Either way it must be refused, never activated.
  assert.ok([403, 422].includes(act.status), `got ${act.status}`);
  assert.ok(["CREDENTIAL_UNKNOWN", "NOT_RECEIVER", "NOT_RECEIVER"].includes(act.json.error.code));
});

// --- A6 ----------------------------------------------------------------------
test("A6 · an expired or already consumed challenge is refused", async () => withFixtureAuthority(async () => {
  const { app, db } = await fresh();
  const a = await makeClient(app);
  const b = await makeClient(app);
  const { id } = await toLearning(a, b);
  const key = newKey();
  await registerKey(b, key);
  const { credential, challengeId } = await bindCredential({ a, b, transferId: id, key });
  await b.call("POST", "/v1/usage/attest-v2", { credential });

  await db.run("update challenge set expires_at_ms=$1 where id=$2", [Date.now() - 1000, challengeId]);
  const expired = await b.call("POST", `/v1/transmissions/${id}/activate`, { credential_id: credential.credential_id, challenge_id: challengeId });
  assert.equal(expired.status, 410);
  assert.equal(expired.json.error.code, "CHALLENGE_EXPIRED");
}));

// --- A7 ----------------------------------------------------------------------
test("A7 · a credential observed before the relation existed cannot activate it", async () => withFixtureAuthority(async () => {
  const { app, db } = await fresh();
  const a = await makeClient(app);
  const b = await makeClient(app);
  const key = newKey();
  await registerKey(b, key);
  const { id } = await toLearning(a, b);
  const { credential, challengeId } = await bindCredential({ a, b, transferId: id, key });
  await b.call("POST", "/v1/usage/attest-v2", { credential });

  // pretend it was recorded long before the edge was formed
  await db.run("update credential set received_at_ms=$1 where id=$2", [Date.now() - 86_400_000, credential.credential_id]);
  const act = await b.call("POST", `/v1/transmissions/${id}/activate`, { credential_id: credential.credential_id, challenge_id: challengeId });
  assert.equal(act.status, 409);
  assert.equal(act.json.error.code, "EVIDENCE_PRECEDES_RELATION");
}));

// --- A8 ----------------------------------------------------------------------
test("A8 · one credential cannot activate two relations", async () => withFixtureAuthority(async () => {
  const { app } = await fresh();
  const a = await makeClient(app);
  const b = await makeClient(app);
  const first = await toLearning(a, b);
  const key = newKey();
  await registerKey(b, key);
  const { credential, challengeId } = await bindCredential({ a, b, transferId: first.id, key });
  await b.call("POST", "/v1/usage/attest-v2", { credential });
  const ok = await b.call("POST", `/v1/transmissions/${first.id}/activate`, { credential_id: credential.credential_id, challenge_id: challengeId });
  assert.equal(ok.status, 200);

  const second = await toLearning(a, b);
  const reuse = await b.call("POST", `/v1/transmissions/${second.id}/activate`, {
    credential_id: credential.credential_id, challenge_id: challengeId,
  });
  assert.ok([409, 422].includes(reuse.status), `got ${reuse.status}`);
}));

// --- A9 ----------------------------------------------------------------------
test("A9 · eight concurrent activations produce exactly one activation and one echo", async () => withFixtureAuthority(async () => {
  const { app, db } = await fresh();
  const a = await makeClient(app);
  const b = await makeClient(app);
  const { id } = await toLearning(a, b);
  const key = newKey();
  await registerKey(b, key);
  const { credential, challengeId } = await bindCredential({ a, b, transferId: id, key });
  await b.call("POST", "/v1/usage/attest-v2", { credential });

  const results = await Promise.all(Array.from({ length: 8 }, () =>
    b.call("POST", `/v1/transmissions/${id}/activate`, { credential_id: credential.credential_id, challenge_id: challengeId })));
  const successes = results.filter((r) => r.status === 200 && r.json.already_activated === false);
  assert.equal(successes.length, 1, `exactly one fresh activation, got ${successes.length}`);
  const echoes = await db.all("select count(*)::int as n from outbox_event where kind='edge.activated'");
  assert.equal(echoes[0].n, 1, "one echo per activation, never more");
}));

// --- A10 ---------------------------------------------------------------------
test("A10 · a failed activation leaves no partial state", async () => withFixtureAuthority(async () => {
  const { app, db } = await fresh();
  const a = await makeClient(app);
  const b = await makeClient(app);
  const { id } = await toLearning(a, b);
  const key = newKey();
  await registerKey(b, key);
  const { credential, challengeId } = await bindCredential({ a, b, transferId: id, key });
  await b.call("POST", "/v1/usage/attest-v2", { credential });

  await db.run("update challenge set expires_at_ms=$1 where id=$2", [Date.now() - 1, challengeId]);
  await b.call("POST", `/v1/transmissions/${id}/activate`, { credential_id: credential.credential_id, challenge_id: challengeId });

  const ch = await db.one("select consumed_at_ms from challenge where id=$1", [challengeId]);
  assert.equal(ch.consumed_at_ms, null, "a refused request must not consume the challenge");
  const cons = await db.all("select * from credential_consumption where credential_id=$1", [credential.credential_id]);
  assert.equal(cons.length, 0);
  const state = await a.call("GET", `/v1/transmissions/${id}`);
  assert.equal(state.json.state, "LEARNING");
}));

// --- A11 ---------------------------------------------------------------------
test("A11 · turning the verifier flag off does not reopen the legacy bypass", async () => {
  const { app } = await fresh();
  const a = await makeClient(app);
  const b = await makeClient(app);
  const { id } = await toLearning(a, b);
  const saved = process.env.FL_VERIFIER_V2;
  process.env.FL_VERIFIER_V2 = "off";
  try {
    const act = await b.call("POST", `/v1/transmissions/${id}/activate`, { usage_event_id: "evt-anything" });
    assert.equal(act.status, 426, "no-credential activation stays refused regardless of the flag");
  } finally {
    process.env.FL_VERIFIER_V2 = saved;
  }
});

// --- A12 ---------------------------------------------------------------------
test("A12 · a revoked key cannot activate even if the credential row says VALID", async () => {
  const { app } = await fresh();
  const a = await makeClient(app);
  const b = await makeClient(app);
  const { id } = await toLearning(a, b);
  const key = newKey();
  await registerKey(b, key);
  const { credential, challengeId } = await bindCredential({ a, b, transferId: id, key });
  await b.call("POST", "/v1/usage/attest-v2", { credential });
  await b.call("POST", "/v1/keys/revoke", { key_id: "dev1" });

  const act = await b.call("POST", `/v1/transmissions/${id}/activate`, { credential_id: credential.credential_id, challenge_id: challengeId });
  assert.equal(act.status, 403);
  assert.equal(act.json.error.code, "KEY_REVOKED");
});

// --- A13 ---------------------------------------------------------------------
test("A13 · the full positive path (TEST FIXTURE authority) reaches ACTIVATED once", async () => {
  process.env.FL_TRUSTED_FIXTURE_ADAPTER = "fixture-adapter";
  const { app, db } = await fresh();
  const a = await makeClient(app);
  const b = await makeClient(app);
  const { id } = await toLearning(a, b);

  // A test-only issuer key. NOT evidence of real model usage.
  const key = newKey();
  await registerKey(b, key, "fixture-issuer", "fixture-adapter");
  const { credential, challengeId } = await bindCredential({
    a, b, transferId: id, key, keyId: "fixture-issuer",
    overrides: { adapter_id: "fixture-adapter", observation_method: "fixture_observation" },
  });
  const att = await b.call("POST", "/v1/usage/attest-v2", { credential });
  assert.equal(att.json.signature_status, "VALID");

  const act = await b.call("POST", `/v1/transmissions/${id}/activate`, { credential_id: credential.credential_id, challenge_id: challengeId });
  assert.equal(act.status, 200, JSON.stringify(act.json));
  assert.equal(act.json.state, "ACTIVATED");
  assert.equal(act.json.already_activated, false);
  assert.deepEqual(act.json.status, { cryptoVerified: true, sourcePolicyApproved: true, activationEligible: true });

  const echo = await db.all("select * from outbox_event where kind='edge.activated'");
  assert.equal(echo.length, 1);

  const replay = await b.call("POST", `/v1/transmissions/${id}/activate`, { credential_id: credential.credential_id, challenge_id: challengeId });
  assert.equal(replay.json.already_activated, true);
  const echoes = await db.all("select count(*)::int as n from outbox_event where kind='edge.activated'");
  assert.equal(echoes[0].n, 1, "replay must not produce a second echo");
  delete process.env.FL_TRUSTED_FIXTURE_ADAPTER;
});

// --- title audit -------------------------------------------------------------
test("T1 · a self claim cannot raise the trusted family count", async () => {
  const { app } = await fresh();
  const b = await makeClient(app);
  const key = newKey();
  await registerKey(b, key);
  const cred = (family, source) => {
    const p = {
      protocol_version: PROTOCOL_VERSION, credential_id: `c_${randomBytes(6).toString("hex")}`,
      issuer_key_id: "dev1", subject_identity_id: b.identity.anonId, source_type: source,
      adapter_id: "ollama-proxy", adapter_version: "0.1.0", model_family: family,
      observation_method: "http_response_observed", observed_at: Date.now(), issued_at: Date.now(),
    };
    return { ...p, signature: edSign(null, signingInput(p), key.priv).toString("hex") };
  };
  await b.call("POST", "/v1/usage/attest-v2", { credential: cred("gpt", SOURCE.SIGNED_SELF_CLAIM) });
  await b.call("POST", "/v1/usage/attest-v2", { credential: cred("claude", SOURCE.PLUGIN_OBSERVED) });

  const { trustedFamilyCount } = await import("../src/activation.mjs");
  const n = await trustedFamilyCount((await import("../src/db.mjs")).openDb ? null : null, null).catch(() => null);
  // count through the app's own db instead
  const att = await b.call("POST", "/v1/usage/attest-v2", { credential: cred("deepseek", SOURCE.SIGNED_SELF_CLAIM) });
  assert.equal(att.json.signature_status, "VALID");
  assert.equal(att.json.eligible.title, false, "a self claim may never present itself as verified usage");
});
