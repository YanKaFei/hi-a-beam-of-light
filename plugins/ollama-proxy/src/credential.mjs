// Signed usage credential, byte-identical to services/api/src/credential.mjs.
// Kept as a separate copy so the plugin can be installed without the server checkout;
// test/conformance.test.mjs pins it against the shared vector so the copies cannot drift.

import { createHash, sign as edSign, generateKeyPairSync } from "node:crypto";

export const PROTOCOL_VERSION = "fl.usage.credential/v1";
const DOMAIN = "firstlight/usage-credential/v1";
const REQUIRED = [
  "protocol_version", "credential_id", "issuer_key_id", "subject_identity_id",
  "source_type", "adapter_id", "adapter_version", "model_family",
  "observation_method", "observed_at", "issued_at",
];
const OPTIONAL = ["challenge_id", "transfer_id"];

export function canonicalize(obj) {
  const keys = Object.keys(obj).sort();
  const parts = [];
  for (const k of keys) {
    const v = obj[k];
    if (v === null || v === undefined) continue;
    if (typeof v === "string") parts.push(JSON.stringify(k) + ":" + JSON.stringify(v));
    else if (typeof v === "number" && Number.isInteger(v)) parts.push(JSON.stringify(k) + ":" + String(v));
    else if (typeof v === "boolean") parts.push(JSON.stringify(k) + ":" + (v ? "true" : "false"));
    else throw new Error(`JCS_SUBSET: unsupported ${typeof v}`);
  }
  return "{" + parts.join(",") + "}";
}

export function signingInput(payload) {
  const body = {};
  for (const k of REQUIRED.concat(OPTIONAL)) {
    if (payload[k] !== undefined && payload[k] !== null) body[k] = payload[k];
  }
  return Buffer.from(DOMAIN + "\n" + canonicalize(body), "utf8");
}

export function newDeviceKey() {
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  return { publicKeyRaw: publicKey.export({ format: "der", type: "spki" }).subarray(-32), privateKey };
}

export function buildCredential({ key, keyId, subjectIdentityId, family, adapterId, adapterVersion, observedAt, issuedAt, challengeId, transferId, credentialId }) {
  const payload = {
    protocol_version: PROTOCOL_VERSION,
    credential_id: credentialId,
    issuer_key_id: keyId,
    subject_identity_id: subjectIdentityId,
    source_type: "PLUGIN_OBSERVED",
    adapter_id: adapterId,
    adapter_version: adapterVersion,
    model_family: family,
    observation_method: "http_response_completed",
    observed_at: observedAt,
    issued_at: issuedAt,
  };
  if (challengeId) payload.challenge_id = challengeId;
  if (transferId) payload.transfer_id = transferId;
  const signature = edSign(null, signingInput(payload), key.privateKey).toString("hex");
  return { ...payload, signature };
}

export function sha256Hex(buf) {
  return createHash("sha256").update(buf).digest("hex");
}
