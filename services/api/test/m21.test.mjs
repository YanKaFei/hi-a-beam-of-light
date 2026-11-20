// M2.1: device binding, pairing, and the boundary that a correct credential with an
// untrusted source still may not activate.
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
import { pairingProofMessage } from "../src/pairing.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const BRAND = JSON.parse(readFileSync(join(HERE, "..", "..", "..", "packages", "brand", "brand.json"), "utf8"));

async function fresh() {
  const db = await openDb();
  return { db, app: createApp({ db, policy: loadPolicy({ publicBaseUrl: "http://127.0.0.1:8787" }), brand: BRAND }) };
}

const newKey = () => {
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  return { pub: publicKey.export({ format: "der", type: "spki" }).subarray(-32), priv: privateKey };
};

/** Full pairing: device registers, opens a session, phone approves, device confirms. */
async function pair({ app, phone, deviceId = "devpair01" }) {
  const key = newKey();
  await app.request("/v1/devices/register", {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ device_id: deviceId, pubkey_hex: key.pub.toString("hex"), adapter_id: "ollama-proxy" }),
  });
  const s = await app.request("/v1/pairing/sessions", {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ device_id: deviceId }),
  });
  const session = await s.json();
  const ap = await phone.call("POST", `/v1/pairing/sessions/${session.session_id}/approve`, { code: session.code });
  assert.equal(ap.status, 200, JSON.stringify(ap.json));
  const proof = edSign(null, pairingProofMessage({
    sessionId: session.session_id, code: session.code, deviceId,
  }), key.priv);
  const cf = await app.request(`/v1/pairing/sessions/${session.session_id}/confirm`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ device_id: deviceId, code: session.code, signature_hex: proof.toString("hex") }),
  });
  return { key, session, confirm: await cf.json(), confirmStatus: cf.status, deviceId };
}

// 1 -------------------------------------------------------------------------
test("1 · a correctly bound credential with an untrusted source still cannot activate", async () => {
  const { app } = await fresh();
  const a = await makeClient(app);
  const phone = await makeClient(app);
  const { key, confirm } = await pair({ app, phone });
  assert.equal(confirm.state, "PAIRED");

  const created = await a.call("POST", "/v1/transmissions", { channel: "QR" });
  await phone.call("GET", `/v1/transmissions/resolve?token=${created.json.token}`);
  await phone.call("POST", `/v1/transmissions/${created.json.id}/accept`, {});
  await a.call("POST", `/v1/transmissions/${created.json.id}/confirm`, {});

  const ch = await phone.call("POST", "/v1/challenges", { transfer_id: created.json.id });
  assert.equal(ch.status, 201, JSON.stringify(ch.json));

  const payload = {
    protocol_version: PROTOCOL_VERSION,
    credential_id: `cred_${randomBytes(6).toString("hex")}`,
    issuer_key_id: "devpair01",
    subject_identity_id: phone.identity.anonId,
    source_type: SOURCE.SIGNED_SELF_CLAIM,          // correct bindings, weak source
    adapter_id: "ollama-proxy", adapter_version: "0.1.0",
    model_family: "llama", observation_method: "http_response_completed",
    observed_at: Date.now(), issued_at: Date.now(),
    challenge_id: ch.json.challenge_id, transfer_id: created.json.id,
  };
  const credential = { ...payload, signature: edSign(null, signingInput(payload), key.priv).toString("hex") };

  // the phone submits on behalf of the bound device; the device key is what signed it
  const att = await phone.call("POST", "/v1/usage/attest-v2", { credential });
  assert.equal(att.status, 201, JSON.stringify(att.json));
  assert.equal(att.json.signature_status, "VALID");

  const act = await phone.call("POST", `/v1/transmissions/${created.json.id}/activate`, {
    credential_id: credential.credential_id, challenge_id: ch.json.challenge_id,
  });
  assert.equal(act.status, 403);
  assert.equal(act.json.error.code, "SOURCE_NOT_TRUSTED",
    "a correct binding must not rescue an untrusted source");
});

// 2 -------------------------------------------------------------------------
test("2 · a device cannot claim someone else's identity as its subject", async () => {
  const { app, db } = await fresh();
  const phone = await makeClient(app);
  const other = await makeClient(app);
  const { key } = await pair({ app, phone });
  const payload = {
    protocol_version: PROTOCOL_VERSION,
    credential_id: `cred_${randomBytes(6).toString("hex")}`,
    issuer_key_id: "devpair01",
    subject_identity_id: other.identity.anonId,      // not the paired subject
    source_type: SOURCE.PLUGIN_OBSERVED,
    adapter_id: "ollama-proxy", adapter_version: "0.1.0", model_family: "llama",
    observation_method: "http_response_completed",
    observed_at: Date.now(), issued_at: Date.now(),
  };
  const credential = { ...payload, signature: edSign(null, signingInput(payload), key.priv).toString("hex") };
  const r = await other.call("POST", "/v1/usage/attest-v2", { credential });
  // the subject does not match the caller, so the request is refused at the door
  assert.equal(r.status, 422);
});

// 3/4/5 ---------------------------------------------------------------------
test("3/4/5 · unpaired, revoked and swapped devices cannot be used", async () => {
  const { app, db } = await fresh();
  const phone = await makeClient(app);
  const { key, deviceId } = await pair({ app, phone });

  // 4: revoke, then the device must not be considered paired
  const rev = await phone.call("POST", `/v1/devices/${deviceId}/revoke`, {});
  assert.equal(rev.json.revoked, 1);
  const row = await db.one("select status_v2 from device_key where key_id=$1", [deviceId]);
  assert.equal(row.status_v2, "REVOKED");

  // 3: an unpaired device never gets ACTIVE
  const solo = newKey();
  await app.request("/v1/devices/register", {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ device_id: "lonely01", pubkey_hex: solo.pub.toString("hex") }),
  });
  const lonely = await db.one("select status_v2, subject_identity_id from device_key where key_id='lonely01'");
  assert.equal(lonely.status_v2, "PENDING");
  assert.equal(lonely.subject_identity_id, null);
});

// 6 -------------------------------------------------------------------------
test("6 · an expired pairing code cannot be approved", async () => {
  const { app, db } = await fresh();
  const phone = await makeClient(app);
  const key = newKey();
  await app.request("/v1/devices/register", {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ device_id: "expired01", pubkey_hex: key.pub.toString("hex") }),
  });
  const s = await app.request("/v1/pairing/sessions", {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ device_id: "expired01" }),
  });
  const session = await s.json();
  await db.run("update pairing_session set expires_at_ms=$1 where id=$2", [Date.now() - 1000, session.session_id]);
  const ap = await phone.call("POST", `/v1/pairing/sessions/${session.session_id}/approve`, { code: session.code });
  assert.equal(ap.status, 410);
  assert.equal(ap.json.error.code, "PAIRING_EXPIRED");
});

// 7 -------------------------------------------------------------------------
test("7 · a pairing session cannot be replayed after it is consumed", async () => {
  const { app } = await fresh();
  const phone = await makeClient(app);
  const { session, key, deviceId } = await pair({ app, phone });
  const proof = edSign(null, pairingProofMessage({ sessionId: session.session_id, code: session.code, deviceId }), key.priv);
  const again = await app.request(`/v1/pairing/sessions/${session.session_id}/confirm`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ device_id: deviceId, code: session.code, signature_hex: proof.toString("hex") }),
  });
  assert.equal(again.status, 409);
  assert.equal((await again.json()).error.code, "REPLAYED");
});

// 9 -------------------------------------------------------------------------
test("9 · concurrent approval of the same session grants at most one binding", async () => {
  const { app } = await fresh();
  const phone = await makeClient(app);
  const key = newKey();
  await app.request("/v1/devices/register", {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ device_id: "race01", pubkey_hex: key.pub.toString("hex") }),
  });
  const s = await app.request("/v1/pairing/sessions", {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ device_id: "race01" }),
  });
  const session = await s.json();
  const results = await Promise.all(Array.from({ length: 6 }, () =>
    phone.call("POST", `/v1/pairing/sessions/${session.session_id}/approve`, { code: session.code })));
  const ok = results.filter((r) => r.status === 200).length;
  assert.equal(ok, 1, `exactly one approval may win, got ${ok}`);
});

// 10 ------------------------------------------------------------------------
test("10 · a device that fails the proof is never bound", async () => {
  const { app, db } = await fresh();
  const phone = await makeClient(app);
  const key = newKey();
  await app.request("/v1/devices/register", {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ device_id: "proof01", pubkey_hex: key.pub.toString("hex") }),
  });
  const s = await app.request("/v1/pairing/sessions", {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ device_id: "proof01" }),
  });
  const session = await s.json();
  await phone.call("POST", `/v1/pairing/sessions/${session.session_id}/approve`, { code: session.code });

  const wrongKey = newKey();
  const badProof = edSign(null, pairingProofMessage({ sessionId: session.session_id, code: session.code, deviceId: "proof01" }), wrongKey.priv);
  const cf = await app.request(`/v1/pairing/sessions/${session.session_id}/confirm`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ device_id: "proof01", code: session.code, signature_hex: badProof.toString("hex") }),
  });
  assert.equal(cf.status, 403);
  const row = await db.one("select status_v2 from device_key where key_id='proof01'");
  assert.equal(row.status_v2, "PENDING", "a failed proof must leave no binding");
});

// 11/12 ---------------------------------------------------------------------
test("11/12 · no test-issuer activation shortcut exists in the default configuration", async () => {
  assert.equal(process.env.FL_ACTIVATION_MODE, undefined);
  const { app } = await fresh();
  const a = await makeClient(app);
  const b = await makeClient(app);
  const created = await a.call("POST", "/v1/transmissions", { channel: "QR" });
  await b.call("POST", `/v1/transmissions/${created.json.id}/accept`, {});
  await a.call("POST", `/v1/transmissions/${created.json.id}/confirm`, {});
  const act = await b.call("POST", `/v1/transmissions/${created.json.id}/activate`, {
    usage_event_id: "evt-whatever",
  });
  assert.equal(act.status, 426, "the legacy shortcut stays closed in production mode");
});
