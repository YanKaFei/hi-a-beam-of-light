// Device identity + two-sided pairing (M2.1).
//
// A device key proves possession of a private key. It does not, by itself, belong to
// anyone. The binding to a 光种 subject is created only by a two-sided act: the phone
// (an authenticated identity) approves a short code, then the device proves it holds the
// key by signing the session. Neither side alone can attach a device to a subject.

import { createHash, randomBytes } from "node:crypto";
import { verifySignature, sha256Hex } from "./auth.mjs";

export const PAIRING_TTL_MS = 5 * 60_000;
const CODE_ALPHABET = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";   // no look-alikes

/**
 * Eight characters, no separator. The protocol says 8 and the server should emit exactly
 * that; a hyphen is a display convenience the input side tolerates, never something the
 * server depends on.
 */
export function makeCode(len = 8) {
  let out = "";
  const bytes = randomBytes(len);
  for (let i = 0; i < len; i += 1) out += CODE_ALPHABET[bytes[i] % CODE_ALPHABET.length];
  return out;
}

export const normalizeCode = (c) => String(c || "").toUpperCase().replace(/[^A-Z0-9]/g, "");
export const codeHash = (c) => sha256Hex(normalizeCode(c));

/** The exact bytes a device signs to prove it holds its key for this session. */
export function pairingProofMessage({ sessionId, code, deviceId }) {
  return Buffer.from(`firstlight/pairing/v1\n${sessionId}\n${deviceId}\n${normalizeCode(code)}`, "utf8");
}

/** A short human-checkable fingerprint both screens can show. */
export function pairingFingerprint({ sessionId, devicePubkeyHex }) {
  const h = createHash("sha256").update(sessionId).update(devicePubkeyHex).digest("hex");
  return h.slice(0, 6).toUpperCase().match(/.{1,3}/g).join(" ");
}

export function verifyPairingProof({ session, code, deviceId, signatureHex }) {
  const msg = pairingProofMessage({ sessionId: session.id, code, deviceId });
  return verifySignature(Buffer.from(session.device_pubkey, "hex"), msg, signatureHex);
}

export class PairingError extends Error {
  constructor(code, message, status = 400) {
    super(message || code);
    this.code = code;
    this.status = status;
  }
}

/** Resolve which subject a credential may claim, given its issuer key. */
export async function subjectForDeviceKey(db, { keyId, subjectClaimed }) {
  const row = await db.one(
    "select subject_identity_id, status_v2 from device_key where key_id=$1 order by created_at_ms desc limit 1",
    [keyId],
  );
  if (!row) throw new PairingError("KEY_UNKNOWN", "issuer key is not registered", 422);
  if (row.status_v2 !== "ACTIVE") {
    throw new PairingError("DEVICE_NOT_PAIRED", "device is not bound to a subject", 403);
  }
  if (!row.subject_identity_id) throw new PairingError("DEVICE_NOT_PAIRED", "no subject binding", 403);
  if (subjectClaimed && row.subject_identity_id !== subjectClaimed) {
    // the credential may not name someone else's subject
    return { subjectIdentityId: row.subject_identity_id, mismatched: true };
  }
  return { subjectIdentityId: row.subject_identity_id, mismatched: false };
}
