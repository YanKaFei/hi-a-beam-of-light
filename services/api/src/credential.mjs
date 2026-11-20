// Signed usage credentials (M1).
//
// A valid signature proves *who said it*, never *that it happened*. Everything in this
// file is built to keep those two statements apart: cryptographic integrity is recorded
// as `signature_status`, while whether the claim may influence anything is decided by
// `source_class` and POLICY. A self-signed claim can be perfectly valid and still be
// barred from activation.

import { verifySignature, sha256Hex } from "./auth.mjs";

export const PROTOCOL_VERSION = "fl.usage.credential/v1";
const DOMAIN = "firstlight/usage-credential/v1";

/** Source categories. These do NOT replace E0–E4; they sit beside them. */
export const SOURCE = {
  UNSIGNED_CLAIM: "UNSIGNED_CLAIM",
  SIGNED_SELF_CLAIM: "SIGNED_SELF_CLAIM",
  PLUGIN_OBSERVED: "PLUGIN_OBSERVED",
  INDEPENDENTLY_CORROBORATED: "INDEPENDENTLY_CORROBORATED",
};

export const SIG = { VALID: "VALID", INVALID: "INVALID", UNKNOWN: "UNKNOWN", REVOKED: "REVOKED" };

/**
 * Admission matrix. Each column answers one question, and they are deliberately
 * different questions — a row can be "yes" for personal display and "no" for activation.
 */
export const POLICY = {
  [SOURCE.UNSIGNED_CLAIM]:        { store: true,  title: false, activate: false, publicField: false },
  [SOURCE.SIGNED_SELF_CLAIM]:     { store: true,  title: false, activate: false, publicField: false },
  // M2.2: an open plugin observes; it does not corroborate. Activation requires a
  // source that is independent of the claimant, which nothing produces yet.
  [SOURCE.PLUGIN_OBSERVED]:       { store: true,  title: true,  activate: false, publicField: false },
  [SOURCE.INDEPENDENTLY_CORROBORATED]: { store: true, title: true, activate: true, publicField: true },
};

export function mayActivate(sourceClass) {
  return Boolean(POLICY[sourceClass]?.activate);
}
export function mayCountTitle(sourceClass) {
  return Boolean(POLICY[sourceClass]?.title);
}

const REQUIRED = [
  "protocol_version", "credential_id", "issuer_key_id", "subject_identity_id",
  "source_type", "adapter_id", "adapter_version", "model_family",
  "observation_method", "observed_at", "issued_at",
];
const OPTIONAL = ["challenge_id", "transfer_id"];
const MAX_LEN = 128;

/**
 * RFC 8785 (JCS) canonicalisation, restricted to what this schema allows: strings and
 * integers only, no floats, no arrays. The restriction is stated so nobody assumes the
 * full standard is implemented.
 */
export function canonicalize(obj) {
  const keys = Object.keys(obj).sort();
  for (const k of keys) {
    const v = obj[k];
    if (v === null) continue;
    if (typeof v === "string") continue;
    if (typeof v === "number") {
      if (!Number.isInteger(v)) throw new Error("JCS_SUBSET: non-integer number");
      continue;
    }
    if (typeof v === "boolean") continue;
    throw new Error(`JCS_SUBSET: unsupported type ${typeof v}`);
  }
  return "{" + keys.map((k) => JSON.stringify(k) + ":" + (obj[k] === null ? "null" : JSON.stringify(obj[k]))).join(",") + "}";
}

/** The exact bytes an issuer signs: a domain-separated tag, then the canonical form. */
export function signingInput(payload) {
  const body = {};
  for (const k of REQUIRED.concat(OPTIONAL)) if (payload[k] !== undefined && payload[k] !== null) body[k] = payload[k];
  return Buffer.from(DOMAIN + "\n" + canonicalize(body), "utf8");
}

export class CredentialError extends Error {
  constructor(code, message) { super(`${code}: ${message || code}`); this.code = code; }
}

const RULES = {
  protocol_version: (v) => v === PROTOCOL_VERSION,
  credential_id: (v) => isId(v),
  issuer_key_id: (v) => isId(v),
  subject_identity_id: (v) => isId(v),
  source_type: (v) => Object.values(SOURCE).includes(v),
  adapter_id: (v) => /^[a-z0-9][a-z0-9._-]{0,63}$/.test(v),
  adapter_version: (v) => /^[0-9A-Za-z][0-9A-Za-z.+_-]{0,31}$/.test(v),
  model_family: (v) => /^[a-z0-9][a-z0-9_-]{0,40}$/.test(v),
  observation_method: (v) => /^[a-z0-9][a-z0-9._-]{0,63}$/.test(v),
  observed_at: (v) => Number.isInteger(v) && v > 0,
  issued_at: (v) => Number.isInteger(v) && v > 0,
  challenge_id: (v) => isId(v),
  transfer_id: (v) => isId(v),
};

function isId(v) {
  return typeof v === "string" && v.length > 0 && v.length <= MAX_LEN &&
    /^[A-Za-z0-9_.:#-]+$/.test(v);
}

/** Field-level validation. Unknown fields are rejected, not ignored: silence hides bugs. */
export function validateShape(payload) {
  if (!payload || typeof payload !== "object" || Array.isArray(payload)) {
    throw new CredentialError("BAD_SHAPE", "payload must be an object");
  }
  const sig = payload.signature;
  const body = { ...payload };
  delete body.signature;

  for (const key of Object.keys(body)) {
    if (!REQUIRED.includes(key) && !OPTIONAL.includes(key)) {
      throw new CredentialError("UNKNOWN_FIELD", `unknown field ${key}`);
    }
  }
  for (const key of REQUIRED) {
    if (body[key] === undefined) throw new CredentialError("MISSING_FIELD", `missing ${key}`);
    if (!RULES[key](body[key])) throw new CredentialError("BAD_FIELD", `invalid ${key}`);
  }
  for (const key of OPTIONAL) {
    if (body[key] !== undefined && !RULES[key](body[key])) {
      throw new CredentialError("BAD_FIELD", `invalid ${key}`);
    }
  }
  if (typeof sig !== "string" || !/^[0-9a-f]{128}$/.test(sig)) {
    throw new CredentialError("BAD_SIGNATURE_FORMAT", "signature must be 64 bytes hex");
  }
  return { body, signature: sig };
}

/**
 * Verify one credential against a registered key. Returns a verdict object; it never
 * throws for a bad signature, because "signed but wrong" is a normal outcome the caller
 * must be able to record.
 */
export function verifyCredential({ payload, publicKeyHex, keyStatus, nowMs, limits = {} }) {
  const { body, signature } = validateShape(payload);
  const result = { body, signatureStatus: SIG.INVALID, sourceClass: SOURCE.UNSIGNED_CLAIM, reasons: [] };

  if (keyStatus !== "ACTIVE") {
    result.signatureStatus = SIG.REVOKED;
    result.reasons.push("KEY_NOT_ACTIVE");
    return result;
  }
  const ok = verifySignature(Buffer.from(publicKeyHex, "hex"), signingInput(body), signature);
  if (!ok) {
    result.reasons.push("SIGNATURE_INVALID");
    return result;
  }
  result.signatureStatus = SIG.VALID;

  // A valid signature does not upgrade the source. The class is what the issuer claims.
  result.sourceClass = body.source_type;

  const skew = limits.clockSkewMs ?? 120_000;
  const maxAge = limits.maxCredentialAgeMs ?? 10 * 60_000;
  if (body.observed_at > nowMs + skew) result.reasons.push("OBSERVED_IN_FUTURE");
  if (body.issued_at > nowMs + skew) result.reasons.push("ISSUED_IN_FUTURE");
  if (nowMs - body.issued_at > maxAge) result.reasons.push("CREDENTIAL_TOO_OLD");
  if (body.observed_at > body.issued_at + skew) result.reasons.push("OBSERVED_AFTER_ISSUE");
  return result;
}

export function payloadHash(body) {
  return sha256Hex(canonicalize(body));
}
