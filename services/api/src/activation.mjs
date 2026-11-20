// The single activation decision (M1.1).
//
// Every route that can produce ACTIVATED, FIRST_LIGHT or a trusted family count goes
// through here. The three properties are returned separately on purpose:
//
//   cryptoVerified        the bytes were signed by the key they claim
//   sourcePolicyApproved  that source class is allowed to activate, by policy
//   activationEligible    every binding (relation, subject, challenge, freshness) holds
//
// Callers must not collapse them into one "verified" boolean.

import { verifyCredential, mayActivate, SIG, SOURCE, POLICY } from "./credential.mjs";

const TRUSTED_FOR_TITLE = new Set([SOURCE.PLUGIN_OBSERVED, SOURCE.INDEPENDENTLY_CORROBORATED]);

export class ActivationDenied extends Error {
  constructor(code, message, extra = {}) {
    super(message || code);
    this.code = code;
    this.extra = extra;
    this.status = extra.status ?? 403;
  }
}

/** Families that may raise the trusted title: only properly signed, policy-approved ones. */
export async function trustedFamilyCount(db, identityId) {
  const rows = await db.all(
    `select distinct model_family from credential
      where identity_id=$1 and signature_status='VALID'
        and source_class in ('PLUGIN_OBSERVED','INDEPENDENTLY_CORROBORATED')`,
    [identityId],
  );
  return rows.length;
}

/**
 * Decide whether one transmission may be activated by this caller with this credential.
 * Throws ActivationDenied with a code; never returns a partially-true verdict.
 */
export async function evaluateActivation({
  db, transmission, callerIdentityId, callerAnonId, body, nowMs, windowMs = 24 * 3600 * 1000,
}) {
  const reasons = [];

  // 1. relation must be in a state that can activate
  const edge = await db.one("select * from light_edge where transmission_id=$1", [transmission.id]);
  if (!edge) throw new ActivationDenied("NO_RELATION", "no relation exists for this session", { status: 409 });
  if (edge.state === "ACTIVATED") {
    // Idempotent replay: the same relation already lit up once. No second count, no
    // second echo — the caller just gets told it is done.
    return { already: true, edge };
  }
  if (!["FORMED", "LEARNING"].includes(edge.state)) {
    throw new ActivationDenied("RELATION_NOT_READY", `relation is ${edge.state}`, { status: 409 });
  }

  // 2. only the receiver may activate
  const receiver = await db.one(
    "select identity_id from transmission_party where transmission_id=$1 and role='RECEIVER'",
    [transmission.id],
  );
  if (!receiver || receiver.identity_id !== callerIdentityId) {
    throw new ActivationDenied("NOT_RECEIVER", "only the receiver can activate");
  }

  const credentialId = body?.credential_id;
  const challengeId = body?.challenge_id;
  if (!credentialId) throw new ActivationDenied("CREDENTIAL_REQUIRED", "activation requires a signed credential", { status: 400 });
  if (!challengeId) throw new ActivationDenied("CHALLENGE_REQUIRED", "activation requires a challenge", { status: 400 });

  // 3. the credential must exist for this identity
  const row = await db.one(
    "select * from credential where id=$1 and identity_id=$2",
    [credentialId, callerIdentityId],
  );
  if (!row) throw new ActivationDenied("CREDENTIAL_UNKNOWN", "no such credential for this identity", { status: 422 });

  // 4. signature must be valid
  const cryptoVerified = row.signature_status === SIG.VALID;
  if (!cryptoVerified) {
    throw new ActivationDenied("CRYPTO_INVALID", `signature is ${row.signature_status}`, { status: 422 });
  }

  // 5. the issuing key must still be usable — revocation beats a stored VALID
  const key = await db.one(
    "select status from device_key where identity_id=$1 and key_id=$2",
    [callerIdentityId, row.key_id],
  );
  if (!key || key.status !== "ACTIVE") {
    throw new ActivationDenied("KEY_REVOKED", "issuing key is revoked or unknown", { status: 403 });
  }

  // 6. the receiver's own claim of source_type is not trusted: re-derive from policy
  let sourceClass = row.source_class;
  // A fixture adapter may stand in for an independent corroborator, but only when the
  // operator names it explicitly. Unset (the default) means no such path exists.
  const fixture = process.env.FL_TRUSTED_FIXTURE_ADAPTER;
  if (fixture && row.adapter_id === fixture) {
    sourceClass = "INDEPENDENTLY_CORROBORATED";
    reasons.push("FIXTURE_AUTHORITY");
  }
  const sourcePolicyApproved = mayActivate(sourceClass);
  if (!sourcePolicyApproved) {
    throw new ActivationDenied("SOURCE_NOT_TRUSTED", `source class ${sourceClass} cannot activate a transmission`, { status: 403 });
  }

  // 7. the credential must name this very transmission
  if (row.transfer_id !== transmission.id) {
    throw new ActivationDenied("TRANSFER_MISMATCH", "credential belongs to another transmission", { status: 422 });
  }

  // 8. challenge must be open, bound to the same transfer and receiver, and unexpired
  const challenge = await db.one(
    `select * from challenge
      where id=$1 and transfer_id=$2 and peer_identity_id=$3 and purpose='usage'`,
    [challengeId, transmission.id, callerIdentityId],
  );
  if (!challenge) throw new ActivationDenied("CHALLENGE_MISMATCH", "challenge does not belong to this transmission", { status: 422 });
  if (challenge.consumed_at_ms) throw new ActivationDenied("CHALLENGE_CONSUMED", "challenge already used", { status: 409 });
  if (Number(challenge.expires_at_ms) <= nowMs) {
    throw new ActivationDenied("CHALLENGE_EXPIRED", "challenge expired", { status: 410 });
  }
  if (row.challenge_id !== challengeId) {
    throw new ActivationDenied("CHALLENGE_UNBOUND", "credential was not issued against this challenge", { status: 422 });
  }

  // 9. a credential may be spent on exactly one relation
  const spent = await db.one("select transmission_id from credential_consumption where credential_id=$1", [credentialId]);
  if (spent) {
    if (spent.transmission_id === transmission.id) return { already: true, edge, credential: row, challenge };
    throw new ActivationDenied("CREDENTIAL_REUSED", "credential already consumed by another relation", { status: 409 });
  }

  // 10. evidence must post-date the relation — an old observation cannot be carried over
  if (Number(row.received_at_ms) <= Number(edge.created_at_ms)) {
    throw new ActivationDenied("EVIDENCE_PRECEDES_RELATION", "this usage happened before the light was shared", { status: 409 });
  }
  if (Number(row.received_at_ms) < Number(edge.created_at_ms) - windowMs) {
    throw new ActivationDenied("EVIDENCE_TOO_OLD", "evidence outside the activation window", { status: 409 });
  }

  return {
    already: false,
    edge,
    credential: row,
    challenge,
    status: { cryptoVerified, sourcePolicyApproved, activationEligible: true },
  };
}

/**
 * Consume the challenge and the credential atomically. Both are single-winner updates:
 * the WHERE clause is the lock, so two concurrent requests cannot both succeed.
 */
export async function consumeActivationGrants(db, { challengeId, credentialId, transmissionId, nowMs }) {
  const ch = await db.run(
    `update challenge set state='CONSUMED', consumed_at_ms=$1
      where id=$2 and consumed_at_ms is null returning id`,
    [nowMs, challengeId],
  );
  if (ch.rowCount !== 1) throw new ActivationDenied("CHALLENGE_RACE", "challenge consumed concurrently", { status: 409 });

  try {
    await db.run(
      `insert into credential_consumption (credential_id, transmission_id, consumed_at_ms)
       values ($1,$2,$3)`,
      [credentialId, transmissionId, nowMs],
    );
  } catch {
    throw new ActivationDenied("CREDENTIAL_REUSED", "credential already consumed by another relation", { status: 409 });
  }
}

export { TRUSTED_FOR_TITLE };
