// Anonymous identity and request authentication.
//
// The scheme is the one fixed in F1 (ADR-010):
//
//   root seed ──► root keypair ──► anon_id = "fl1_" + sha256(root_pub)[:16]
//                     │
//                     └── signs an epoch certificate ──► epoch keypair (signs requests)
//
// Rotation and multi-device recovery both work because the anon_id is derived
// from the *root* key, not from whichever signing key is current.
//
// What this proves and what it does not is spelled out in
// docs/reports/f3-security-limits.md. In short: possession of a key, not
// personhood.

import { createPublicKey, verify as cryptoVerify, createHash, timingSafeEqual } from "node:crypto";

/** DER prefix for a raw Ed25519 public key wrapped as SPKI. */
const SPKI_PREFIX = Buffer.from("302a300506032b6570032100", "hex");
const CERT_DOMAIN = "firstlight/epoch-cert/v1";
const ANON_DOMAIN = "firstlight/anon-id/v1";

export const CLOCK_SKEW_MS = 120_000;

export class ApiError extends Error {
  constructor(status, code, message, extra = {}) {
    super(message);
    this.status = status;
    this.code = code;
    this.extra = extra;
  }
}

export function sha256Hex(data) {
  return createHash("sha256").update(data).digest("hex");
}

export function publicKeyFromRaw(raw32) {
  const raw = Buffer.isBuffer(raw32) ? raw32 : Buffer.from(raw32, "hex");
  if (raw.length !== 32) throw new ApiError(400, "BAD_PUBKEY", "public key must be 32 bytes");
  return createPublicKey({
    key: Buffer.concat([SPKI_PREFIX, raw]),
    format: "der",
    type: "spki",
  });
}

export function verifySignature(publicKeyRaw, message, signatureHex) {
  try {
    const key = publicKeyFromRaw(publicKeyRaw);
    const sig = Buffer.from(signatureHex, "hex");
    if (sig.length !== 64) return false;
    return cryptoVerify(null, Buffer.from(message), key, sig);
  } catch {
    return false;
  }
}

export function anonIdFromRootPubkey(rootPubRaw) {
  const raw = Buffer.isBuffer(rootPubRaw) ? rootPubRaw : Buffer.from(rootPubRaw, "hex");
  const digest = createHash("sha256").update(Buffer.from(ANON_DOMAIN)).update(raw).digest();
  return `fl1_${digest.subarray(0, 16).toString("hex")}`;
}

/** The exact bytes the root key signs when authorising an epoch key. */
export function epochBindingMessage(rootPubRaw, epoch, epochPubRaw) {
  const root = Buffer.isBuffer(rootPubRaw) ? rootPubRaw : Buffer.from(rootPubRaw, "hex");
  const ek = Buffer.isBuffer(epochPubRaw) ? epochPubRaw : Buffer.from(epochPubRaw, "hex");
  const e = Buffer.alloc(4);
  e.writeUInt32LE(epoch >>> 0);
  return Buffer.concat([Buffer.from(CERT_DOMAIN), root, e, ek]);
}

/**
 * Verify an epoch certificate against the claimed anon id.
 * Self-contained: the server needs nothing but the anon id and the certificate.
 */
export function verifyEpochCertificate(anonId, cert) {
  try {
    const root = Buffer.from(cert.root_pubkey_hex, "hex");
    const epochPub = Buffer.from(cert.epoch_pubkey_hex, "hex");
    if (root.length !== 32 || epochPub.length !== 32) return false;
    if (anonIdFromRootPubkey(root) !== anonId) return false;
    if (root.equals(epochPub)) return false;
    if (!Number.isInteger(cert.epoch) || cert.epoch < 1) return false;
    const msg = epochBindingMessage(root, cert.epoch, epochPub);
    return verifySignature(root, msg, cert.signature_hex);
  } catch {
    return false;
  }
}

/** The canonical string a client signs. Matches docs/04-api-contract.md. */
export function canonicalRequest({ method, path, bodySha256Hex, timestamp, nonce }) {
  return ["FL1", method.toUpperCase(), path, bodySha256Hex, String(timestamp), nonce].join("\n");
}

export function safeEqualHex(a, b) {
  const ba = Buffer.from(String(a), "hex");
  const bb = Buffer.from(String(b), "hex");
  if (ba.length !== bb.length || ba.length === 0) return false;
  return timingSafeEqual(ba, bb);
}

/**
 * Authenticate one request.
 *
 * Order matters: time window, then key lookup, then signature, then nonce.
 * A replayed nonce is rejected *after* the signature check, so an attacker
 * cannot burn a victim's nonce with forged traffic.
 */
export async function authenticate(db, { method, path, bodySha256Hex, headers, nowMs }) {
  const anonId = headers["x-fl-identity"];
  const epoch = Number(headers["x-fl-epoch"]);
  const timestamp = Number(headers["x-fl-timestamp"]);
  const nonce = headers["x-fl-nonce"];
  const signature = headers["x-fl-signature"];

  if (!anonId || !nonce || !signature || !Number.isFinite(epoch) || !Number.isFinite(timestamp)) {
    throw new ApiError(401, "SIGNATURE_INVALID", "missing authentication headers");
  }
  if (Math.abs(nowMs - timestamp) > CLOCK_SKEW_MS) {
    throw new ApiError(401, "CLOCK_SKEW", "request timestamp outside allowed window");
  }

  const identity = await db.one(
    "select id, anon_id, status from identity where anon_id = $1",
    [anonId],
  );
  if (!identity) throw new ApiError(401, "SIGNATURE_INVALID", "unknown identity");
  if (identity.status !== "ACTIVE") throw new ApiError(403, "IDENTITY_REVOKED", "identity revoked");

  const keyRow = await db.one(
    `select pubkey, retired_at_ms from identity_key_epoch
      where identity_id = $1 and epoch = $2`,
    [identity.id, epoch],
  );
  if (!keyRow) throw new ApiError(401, "SIGNATURE_INVALID", "unknown key epoch");
  if (keyRow.retired_at_ms) throw new ApiError(401, "EPOCH_RETIRED", "key epoch retired");

  const canonical = canonicalRequest({
    method,
    path,
    bodySha256Hex,
    timestamp,
    nonce,
  });
  if (!verifySignature(keyRow.pubkey, canonical, signature)) {
    throw new ApiError(401, "SIGNATURE_INVALID", "signature does not verify");
  }

  // Nonce is single-use. The primary key makes the check atomic under concurrency.
  try {
    await db.run(
      `insert into request_nonce (identity_id, nonce, seen_at_ms, expires_at_ms)
       values ($1, $2, $3, $4)`,
      [identity.id, nonce, nowMs, nowMs + CLOCK_SKEW_MS * 2],
    );
  } catch {
    throw new ApiError(401, "NONCE_REPLAYED", "nonce already used");
  }

  return { identityId: identity.id, anonId: identity.anon_id, epoch };
}
