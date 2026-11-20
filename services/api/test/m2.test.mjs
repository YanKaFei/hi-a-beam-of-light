// M2: a credential produced by the local proxy is stored, but standing alone it can
// never activate a transmission — that is M1.1's binding doing its job.
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
import { PROTOCOL_VERSION, signingInput } from "../src/credential.mjs";
import { buildCredential, newDeviceKey } from "../../../plugins/ollama-proxy/src/credential.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const BRAND = JSON.parse(readFileSync(join(HERE, "..", "..", "..", "packages", "brand", "brand.json"), "utf8"));

async function fresh() {
  const db = await openDb();
  return { db, app: createApp({ db, policy: loadPolicy({ publicBaseUrl: "http://127.0.0.1:8787" }), brand: BRAND }) };
}

test("M2-1 · a proxy-shaped standalone credential is stored as a self-attested observation", async () => {
  const { app, db } = await fresh();
  const b = await makeClient(app);
  const key = newDeviceKey();
  await b.call("POST", "/v1/keys/register", {
    key_id: "ollama-proxy-1", pubkey_hex: key.publicKeyRaw.toString("hex"), adapter_id: "ollama-proxy",
  });
  const credential = buildCredential({
    key, keyId: "ollama-proxy-1", subjectIdentityId: b.identity.anonId, family: "llama",
    adapterId: "ollama-proxy", adapterVersion: "0.1.0",
    observedAt: Date.now(), issuedAt: Date.now(),
    credentialId: `cred_${randomBytes(8).toString("hex")}`,
  });
  const r = await b.call("POST", "/v1/usage/attest-v2", { credential });
  assert.equal(r.status, 201);
  assert.equal(r.json.signature_status, "VALID");
  assert.equal(r.json.source_class, "PLUGIN_OBSERVED");
  const row = await db.one("select transfer_id, challenge_id from credential where id=$1", [credential.credential_id]);
  assert.equal(row.transfer_id, null, "a standalone observation names no transfer");
});

test("M2-2 · a standalone observation cannot activate a transmission", async () => {
  const { app } = await fresh();
  const a = await makeClient(app);
  const b = await makeClient(app);
  const key = newDeviceKey();
  await b.call("POST", "/v1/keys/register", {
    key_id: "ollama-proxy-1", pubkey_hex: key.publicKeyRaw.toString("hex"),
  });
  const credential = buildCredential({
    key, keyId: "ollama-proxy-1", subjectIdentityId: b.identity.anonId, family: "llama",
    adapterId: "ollama-proxy", adapterVersion: "0.1.0",
    observedAt: Date.now(), issuedAt: Date.now(),
    credentialId: `cred_${randomBytes(8).toString("hex")}`,
  });
  await b.call("POST", "/v1/usage/attest-v2", { credential });

  const created = await a.call("POST", "/v1/transmissions", { channel: "QR" });
  await b.call("POST", `/v1/transmissions/${created.json.id}/accept`, {});
  await a.call("POST", `/v1/transmissions/${created.json.id}/confirm`, {});

  const act = await b.call("POST", `/v1/transmissions/${created.json.id}/activate`, {
    credential_id: credential.credential_id, challenge_id: "ch_none",
  });
  // the source-policy refusal now fires before the binding checks
  assert.ok([403, 409, 422].includes(act.status), `got ${act.status}`);
  assert.notEqual(act.json.state, "ACTIVATED");
});

test("M2-3 · the plugin's own signer produces credentials the server accepts", async () => {
  const { app } = await fresh();
  const b = await makeClient(app);
  const key = newDeviceKey();
  await b.call("POST", "/v1/keys/register", { key_id: "k", pubkey_hex: key.publicKeyRaw.toString("hex") });
  const mk = (family) => buildCredential({
    key, keyId: "k", subjectIdentityId: b.identity.anonId, family,
    adapterId: "ollama-proxy", adapterVersion: "0.1.0",
    observedAt: Date.now(), issuedAt: Date.now(),
    credentialId: `cred_${randomBytes(8).toString("hex")}`,
  });
  for (const family of ["llama", "qwen", "deepseek"]) {
    const r = await b.call("POST", "/v1/usage/attest-v2", { credential: mk(family) });
    assert.equal(r.status, 201, `${family}: ${JSON.stringify(r.json)}`);
  }
});
