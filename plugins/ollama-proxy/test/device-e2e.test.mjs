// M2.2 end-to-end: CLI device identity -> pairing -> challenge claim -> signed
// observation -> server verdict. Runs a real HTTP server over PGlite.
process.env.FL_VERIFIER_V2 = "on";
delete process.env.FL_ACTIVATION_MODE;

import { test } from "node:test";
import assert from "node:assert/strict";
import http from "node:http";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { generateKeyPairSync, sign as edSign, randomBytes } from "node:crypto";

import { openDb } from "../../../services/api/src/db.mjs";
import { createApp } from "../../../services/api/src/app.mjs";
import { loadPolicy } from "../../../services/api/src/policy.mjs";
import { makeClient } from "../../../services/api/test/support/client.mjs";
import { pairingProofMessage } from "../../../services/api/src/pairing.mjs";
import { createProxy } from "../src/proxy.mjs";
import { loadRegistry } from "../src/registry.mjs";
import { buildCredential } from "../src/credential.mjs";

const REG = loadRegistry(fileURLToPath(new URL("../../../data/model-registry.v1.json", import.meta.url)));
const BRAND = JSON.parse(readFileSync(new URL("../../../packages/brand/brand.json", import.meta.url), "utf8"));

async function startServer() {
  const db = await openDb();
  const app = createApp({ db, policy: loadPolicy({ publicBaseUrl: "http://127.0.0.1:8787" }), brand: BRAND });
  const srv = http.createServer(async (req, res) => {
    const chunks = [];
    for await (const c of req) chunks.push(c);
    const body = chunks.length ? Buffer.concat(chunks) : undefined;
    const r = await app.request(req.url, {
      method: req.method, headers: req.headers,
      body: req.method === "GET" || req.method === "HEAD" ? undefined : body,
    });
    res.writeHead(r.status, { "content-type": req.headers.accept ?? "application/json" });
    res.end(await r.text());
  });
  await new Promise((r) => srv.listen(0, "127.0.0.1", r));
  return { db, app, base: `http://127.0.0.1:${srv.address().port}`, close: () => new Promise((r) => srv.close(r)) };
}

const api = (base) => ({
  get: async (p) => (await fetch(base + p)).json(),
  post: async (p, body) => {
    const r = await fetch(base + p, {
      method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body ?? {}),
    });
    return { status: r.status, json: await r.json().catch(() => ({})) };
  },
});

const deviceKey = () => {
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  return { pub: publicKey.export({ format: "der", type: "spki" }).subarray(-32), priv: privateKey };
};

async function fullPairing({ base, phone, deviceId = "clidev01" }) {
  const key = deviceKey();
  const h = api(base);
  await h.post("/v1/devices/register", { device_id: deviceId, pubkey_hex: key.pub.toString("hex") });
  const s = await h.post("/v1/pairing/sessions", { device_id: deviceId });
  assert.equal(s.status, 201, JSON.stringify(s.json));
  const approve = await phone.call("POST", `/v1/pairing/sessions/${s.json.session_id}/approve`, { code: s.json.code });
  assert.equal(approve.status, 200, JSON.stringify(approve.json));
  const proof = edSign(null, pairingProofMessage({ sessionId: s.json.session_id, code: s.json.code, deviceId }), key.priv);
  const confirm = await h.post(`/v1/pairing/sessions/${s.json.session_id}/confirm`, {
    device_id: deviceId, code: s.json.code, signature_hex: proof.toString("hex"),
  });
  return { key, deviceId, session: s.json, confirm };
}

test("E2E-1 · pairing then claim then signed observation: server stores it, activation stays blocked", async () => {
  const srv = await startServer();
  try {
    const phone = await makeClient(srv.app);
    const helper = await makeClient(srv.app);
    const { key, deviceId } = await fullPairing({ base: srv.base, phone });
    const h = api(srv.base);

    // phone state after pairing
    const st = await h.get(`/v1/devices/${deviceId}`);
    assert.equal(st.status, "ACTIVE");
    assert.equal(st.paired, true);

    // a transfer from helper to the paired phone
    const created = await helper.call("POST", "/v1/transmissions", { channel: "QR" });
    await phone.call("GET", `/v1/transmissions/resolve?token=${created.json.token}`);
    await phone.call("POST", `/v1/transmissions/${created.json.id}/accept`, {});
    await helper.call("POST", `/v1/transmissions/${created.json.id}/confirm`, {});

    // the phone issues the usage challenge, the device claims it
    const ch = await phone.call("POST", "/v1/challenges", { transfer_id: created.json.id });
    assert.equal(ch.status, 201);
    const claim = await h.post(`/v1/devices/${deviceId}/challenge`, { transfer_id: created.json.id });
    assert.equal(claim.status, 200, JSON.stringify(claim.json));
    assert.equal(claim.json.challenge_id, ch.json.challenge_id);

    // observe a completion through the proxy, sign, submit as the device
    const credential = buildCredential({
      key: { privateKey: key.priv }, keyId: deviceId,
      subjectIdentityId: phone.identity.anonId,
      family: "llama", adapterId: "ollama-proxy", adapterVersion: "0.1.0",
      observedAt: Date.now(), issuedAt: Date.now(),
      credentialId: `cred_${randomBytes(6).toString("hex")}`,
      challengeId: claim.json.challenge_id, transferId: created.json.id,
    });
    const att = await h.post("/v1/usage/attest-device", { credential });
    assert.equal(att.status, 201, JSON.stringify(att.json));
    assert.equal(att.json.signature_status, "VALID");
    assert.equal(att.json.source_class, "PLUGIN_OBSERVED");

    // the phone activates; the source policy must still refuse
    const act = await phone.call("POST", `/v1/transmissions/${created.json.id}/activate`, {
      credential_id: credential.credential_id, challenge_id: claim.json.challenge_id,
    });
    assert.equal(act.status, 403);
    assert.equal(act.json.error.code, "SOURCE_NOT_TRUSTED");
  } finally { await srv.close(); }
});

test("E2E-2 · a device cannot claim a challenge for a transfer it does not own", async () => {
  const srv = await startServer();
  try {
    const phone = await makeClient(srv.app);
    const other = await makeClient(srv.app);
    const helper = await makeClient(srv.app);
    const { deviceId } = await fullPairing({ base: srv.base, phone });
    const h = api(srv.base);

    // a transfer whose receiver is someone else
    const created = await helper.call("POST", "/v1/transmissions", { channel: "QR" });
    await other.call("GET", `/v1/transmissions/resolve?token=${created.json.token}`);
    await other.call("POST", `/v1/transmissions/${created.json.id}/accept`, {});
    await helper.call("POST", `/v1/transmissions/${created.json.id}/confirm`, {});

    const claim = await h.post(`/v1/devices/${deviceId}/challenge`, { transfer_id: created.json.id });
    assert.equal(claim.status, 403);
    assert.equal(claim.json.error.code, "NOT_YOUR_TRANSFER");
  } finally { await srv.close(); }
});

test("E2E-3 · an unpaired device cannot claim challenges or submit", async () => {
  const srv = await startServer();
  try {
    const h = api(srv.base);
    const key = deviceKey();
    await h.post("/v1/devices/register", { device_id: "unpaired01", pubkey_hex: key.pub.toString("hex") });
    const claim = await h.post("/v1/devices/unpaired01/challenge", { transfer_id: "t1" });
    assert.equal(claim.status, 403);
    assert.equal(claim.json.error.code, "DEVICE_NOT_PAIRED");

    const att = await h.post("/v1/usage/attest-device", { credential: { issuer_key_id: "unpaired01" } });
    assert.equal(att.status, 403);
  } finally { await srv.close(); }
});

test("E2E-4 · a revoked device loses both claim and submit", async () => {
  const srv = await startServer();
  try {
    const phone = await makeClient(srv.app);
    const { deviceId, key } = await fullPairing({ base: srv.base, phone });
    const h = api(srv.base);
    await phone.call("POST", `/v1/devices/${deviceId}/revoke`, {});
    const claim = await h.post(`/v1/devices/${deviceId}/challenge`, { transfer_id: "x" });
    assert.equal(claim.status, 403);
    const att = await h.post("/v1/usage/attest-device", {
      credential: buildCredential({
        key: { privateKey: key.priv }, keyId: deviceId, subjectIdentityId: phone.identity.anonId,
        family: "llama", adapterId: "ollama-proxy", adapterVersion: "0.1.0",
        observedAt: Date.now(), issuedAt: Date.now(), credentialId: `cred_${randomBytes(6).toString("hex")}`,
      }),
    });
    assert.equal(att.status, 403);
  } finally { await srv.close(); }
});

test("E2E-5 · a credential naming another subject is refused", async () => {
  const srv = await startServer();
  try {
    const phone = await makeClient(srv.app);
    const other = await makeClient(srv.app);
    const { deviceId, key } = await fullPairing({ base: srv.base, phone });
    const h = api(srv.base);
    const att = await h.post("/v1/usage/attest-device", {
      credential: buildCredential({
        key: { privateKey: key.priv }, keyId: deviceId, subjectIdentityId: other.identity.anonId,
        family: "llama", adapterId: "ollama-proxy", adapterVersion: "0.1.0",
        observedAt: Date.now(), issuedAt: Date.now(), credentialId: `cred_${randomBytes(6).toString("hex")}`,
      }),
    });
    assert.equal(att.status, 422);
    assert.equal(att.json.error.code, "SUBJECT_MISMATCH");
  } finally { await srv.close(); }
});

test("E2E-6 · the proxy only emits one observation per completed call", async () => {
  const up = http.createServer((req, res) => {
    res.writeHead(200, { "content-type": "application/x-ndjson" });
    for (const part of [{ done: false }, { done: false }, { done: true, eval_count: 2 }]) {
      res.write(JSON.stringify({ model: "llama3.1:8b", ...part }) + "\n");
    }
    res.end();
  });
  await new Promise((r) => up.listen(0, "127.0.0.1", r));
  const seen = [];
  const proxy = createProxy({
    upstream: `http://127.0.0.1:${up.address().port}`, registry: REG,
    token: "t".repeat(24), onObservation: (o) => seen.push(o), log: () => {},
  });
  const addr = await new Promise((r) => { proxy.listen(0, "127.0.0.1", () => r(proxy.address())); });
  await new Promise((resolve, reject) => {
    const req = http.request({
      hostname: "127.0.0.1", port: addr.port, path: "/api/chat", method: "POST",
      headers: { "content-type": "application/json", authorization: `Bearer ${"t".repeat(24)}` },
    }, (res) => { res.resume(); res.on("end", resolve); });
    req.on("error", reject);
    req.end(JSON.stringify({ model: "llama3.1:8b", stream: true }));
  });
  await new Promise((r) => setTimeout(r, 50));
  assert.equal(seen.length, 1);
  assert.equal(seen[0].family, "llama");
  await new Promise((r) => proxy.close(r));
  up.close();
});
