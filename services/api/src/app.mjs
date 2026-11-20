// FirstLight transmission API.
//
// Scope (F3): anonymous identity, one-time invitations, mutual confirmation,
// evidence-linked activation, SSE echoes, personal light paths, consent/revocation.
//
// Two invariants are structural rather than procedural:
//   1. RECEIVED never counts as a successful transmission. Only ACTIVATED edges
//      with a countable usage event that post-dates the relation can count.
//   2. The server never stores or accepts chat content, prompts, API keys, token
//      counts, or file paths. Request bodies reject unknown fields.

import { Hono } from "hono";
import { randomBytes } from "node:crypto";
import { SESSION_TTL_MS, productionMode, tokenHash, newToken, code2Session, assertProductionIsolation } from "./wechat.mjs";
import { PROTOCOL_VERSION, SOURCE, SIG, POLICY, verifyCredential, payloadHash, mayActivate } from "./credential.mjs";
import { PAIRING_TTL_MS, makeCode, codeHash, pairingFingerprint, verifyPairingProof, PairingError, subjectForDeviceKey } from "./pairing.mjs";
import { evaluateActivation, consumeActivationGrants, ActivationDenied, trustedFamilyCount } from "./activation.mjs";
import { streamSSE } from "hono/streaming";
import {
  ApiError,
  anonIdFromRootPubkey,
  authenticate,
  sha256Hex,
  verifyEpochCertificate,
} from "./auth.mjs";
import { uuid, secureToken } from "./db.mjs";
import { applyEvent, isTerminal } from "./fsm.mjs";
import { renderReceivePage } from "./receive.mjs";
import qrcode from "qrcode-generator";
import { loadPolicy } from "./policy.mjs";

const nowMs = () => Date.now();

/** Bearer session -> identity. Every mini-program route goes through this. */
async function sessionFromHeaders(db, authz, nowMs) {
  if (!authz || !authz.startsWith("Bearer ")) {
    const e = new Error("missing session"); e.code = "NO_SESSION"; e.status = 401; throw e;
  }
  const row = await db.one("select * from auth_session where token_hash=$1", [tokenHash(authz.slice(7))]);
  if (!row) { const e = new Error("unknown session"); e.code = "NO_SESSION"; e.status = 401; throw e; }
  if (row.revoked_at_ms) { const e = new Error("session revoked"); e.code = "SESSION_REVOKED"; e.status = 401; throw e; }
  if (Number(row.expires_at_ms) <= nowMs) { const e = new Error("session expired"); e.code = "SESSION_EXPIRED"; e.status = 401; throw e; }
  const ident = await db.one("select anon_id from identity where id=$1", [row.identity_id]);
  return { identityId: row.identity_id, anonId: ident?.anon_id, expiresAt: Number(row.expires_at_ms) };
}
function createHashShort(s) { return shift(s); }
function shift(s) {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i += 1) { h ^= s.charCodeAt(i); h = Math.imul(h, 0x01000193) >>> 0; }
  return h.toString(16).padStart(8, "0");
}



/** Reject bodies that carry fields we must never accept (F3 §8). */
const FORBIDDEN_FIELDS = [
  "chat", "chats", "messages", "message", "prompt", "prompts", "text", "content",
  "api_key", "apiKey", "authorization", "token", "tokens", "tokens_used",
  "db_path", "database", "sqlite", "file", "files", "path", "paths",
];

function assertNoForbiddenFields(body) {
  const found = Object.keys(body ?? {}).filter((k) => FORBIDDEN_FIELDS.includes(k));
  if (found.length) {
    throw new ApiError(400, "INVALID_SCHEMA", `forbidden field(s): ${found.join(", ")}`);
  }
}

function requireFields(body, fields) {
  const missing = fields.filter((f) => body?.[f] === undefined || body?.[f] === null);
  if (missing.length) {
    throw new ApiError(400, "INVALID_SCHEMA", `missing field(s): ${missing.join(", ")}`);
  }
}

/** Only E3/E4 are countable. E2 and below are display-only (ADR-009). */
const COUNTABLE_LEVELS = new Set(["E3", "E4"]);

export function createApp({ db, policy = loadPolicy(), brand = {} }) {
  const app = new Hono();

  app.onError((err, c) => {
    if (!(err instanceof ApiError) && !(err instanceof ActivationDenied) && err.status && err.code) {
      return c.json({ error: { code: err.code, message: err.message, retryable: false } }, err.status);
    }
    if (err instanceof ActivationDenied) {
      return c.json({ error: { code: err.code, message: err.message, retryable: false } },
        err.status ?? 403);
    }
    if (err instanceof ApiError) {
      return c.json(
        { error: { code: err.code, message: err.message, retryable: false, ...err.extra } },
        err.status,
      );
    }
    console.error("unhandled:", err?.message);
    return c.json(
      { error: { code: "INTERNAL", message: "internal error", retryable: true } },
      500,
    );
  });

  // ---- authenticated request helper ------------------------------------
  async function auth(c, handler) {
    const bodyText = await c.req.text();
    let body = {};
    if (bodyText) {
      try {
        body = JSON.parse(bodyText);
      } catch {
        throw new ApiError(400, "INVALID_SCHEMA", "body is not valid JSON");
      }
    }
    const url = new URL(c.req.url);
    const caller = await authenticate(db, {
      method: c.req.method,
      path: url.pathname,
      bodySha256Hex: sha256Hex(bodyText),
      headers: c.req.header(),
      nowMs: nowMs(),
    });
    return handler({ caller, body, bodyText, c });
  }

  /** Idempotent mutation wrapper: same key + same body returns the first result. */
  async function idempotent(caller, endpoint, bodyText, c, fn) {
    const key = c.req.header("idempotency-key");
    if (!key) return fn();
    const requestHash = sha256Hex(bodyText);
    const existing = await db.one("select * from idempotency_key where key = $1", [key]);
    if (existing) {
      if (existing.identity_id !== caller.identityId || existing.request_hash !== requestHash) {
        throw new ApiError(409, "IDEMPOTENCY_CONFLICT", "same key used with a different request");
      }
      return c.json(JSON.parse(existing.response ?? "{}"), existing.status_code ?? 200);
    }
    const result = await fn();
    const payload = await result.clone().text();
    await db.run(
      `insert into idempotency_key
        (key, identity_id, endpoint, request_hash, status_code, response, created_at_ms, expires_at_ms)
       values ($1,$2,$3,$4,$5,$6,$7,$8)`,
      [key, caller.identityId, endpoint, requestHash, result.status, payload,
       nowMs(), nowMs() + 24 * 3600 * 1000],
    );
    return result;
  }

  // ---- identity ---------------------------------------------------------

  app.post("/v1/identities/register", async (c) => {
    const body = await c.req.json().catch(() => ({}));
    assertNoForbiddenFields(body);
    requireFields(body, ["anon_id", "cert"]);
    const { anon_id: anonId, cert } = body;

    if (!verifyEpochCertificate(anonId, cert)) {
      throw new ApiError(401, "CERT_INVALID", "epoch certificate does not verify for this anon id");
    }

    const existing = await db.one("select id from identity where anon_id = $1", [anonId]);
    if (existing) {
      // Re-registering is how a new device joins or a key is rotated.
      await db.run(
        `insert into identity_key_epoch (identity_id, epoch, pubkey, created_at_ms)
         values ($1,$2,$3,$4) on conflict (identity_id, epoch) do nothing`,
        [existing.id, cert.epoch, cert.epoch_pubkey_hex, nowMs()],
      );
      return c.json({ ok: true, identity_id: existing.id, registered: false });
    }

    const id = uuid();
    await db.tx(async (tx) => {
      await tx.run(
        `insert into identity (id, anon_id, root_pubkey, status, created_at_ms)
         values ($1,$2,$3,'ACTIVE',$4)`,
        [id, anonId, cert.root_pubkey_hex, nowMs()],
      );
      await tx.run(
        `insert into identity_key_epoch (identity_id, epoch, pubkey, created_at_ms)
         values ($1,$2,$3,$4)`,
        [id, cert.epoch, cert.epoch_pubkey_hex, nowMs()],
      );
    });
    return c.json({ ok: true, identity_id: id, registered: true }, 201);
  });

  // ---- invitations ------------------------------------------------------

  app.post("/v1/transmissions", async (c) =>
    auth(c, async ({ caller, body, bodyText, c: ctx }) =>
      idempotent(caller, "transmissions.create", bodyText, ctx, async () => {
        assertNoForbiddenFields(body);
        const channel = body.channel ?? "QR";
        if (!["QR", "NFC", "LINK", "NEARBY"].includes(channel)) {
          throw new ApiError(400, "INVALID_SCHEMA", "unknown channel");
        }

        const since = nowMs() - 24 * 3600 * 1000;
        const [{ count }] = await db.all(
          `select count(*)::int as count from transmission
            where initiator_id = $1 and created_at_ms > $2`,
          [caller.identityId, since],
        );
        if (count >= policy.maxInvitationsPerDay) {
          throw new ApiError(429, "RATE_LIMITED", "daily invitation limit reached");
        }

        const id = uuid();
        // The token is shown once and never stored: only its hash is kept.
        const token = secureToken(32);
        const tokenHash = sha256Hex(token);
        const ttl = Math.min(
          Math.max(Number(body.ttl_seconds ?? 0) * 1000 || policy.invitationTtlMs, 60_000),
          policy.invitationTtlMs,
        );

        // Generating the QR *is* offering the light, so the session starts at
        // OFFERED. CREATED remains reachable only in the state machine, which
        // keeps OPEN a meaningful "someone actually opened it" event.
        await db.run(
          `insert into transmission
            (id, initiator_id, token_hash, channel, state, version, created_at_ms, updated_at_ms, expires_at_ms)
           values ($1,$2,$3,$4,'OFFERED',1,$5,$6,$7)`,
          [id, caller.identityId, tokenHash, channel, nowMs(), nowMs(), nowMs() + ttl],
        );
        await db.run(
          `insert into transmission_party (transmission_id, role, identity_id)
           values ($1,'INITIATOR',$2)`,
          [id, caller.identityId],
        );

        const receivePath = (brand.urls?.receivePathTemplate ?? "/t/{token}").replace("{token}", token);
        return ctx.json(
          {
            id,
            token,
            receive_url: `${policy.publicBaseUrl}${receivePath}`,
            nfc_payload: `${policy.publicBaseUrl}${receivePath}`,
            expires_at_ms: nowMs() + ttl,
            state: "OFFERED",
          },
          201,
        );
      }),
    ),
  );

  app.post("/v1/transmissions/:id/offer", async (c) =>
    auth(c, async ({ caller, c: ctx }) => {
      const tx = await loadTransmission(c.req.param("id"));
      requireInitiator(tx, caller);
      return transition(tx, "OFFER", caller, ctx);
    }),
  );

  // ---- public receive page ---------------------------------------------

  app.get("/t/:token", async (c) => {
    const token = c.req.param("token");
    const tx = await db.one(
      `select t.*, p.identity_id as initiator_id,
              i.anon_id as initiator_anon
         from transmission t
         join transmission_party p on p.transmission_id = t.id and p.role = 'INITIATOR'
         join identity i on i.id = p.identity_id
        where t.token_hash = $1`,
      [sha256Hex(token)],
    );
    if (!tx) return c.html(renderReceivePage({ brand, state: "INVALID" }), 404);
    return c.html(
      renderReceivePage({
        brand,
        state: tx.state,
        token,
        transmissionId: tx.id,
        initiatorLabel: shortAnon(tx.initiator_anon),
        expiresAtMs: Number(tx.expires_at_ms),
        baseUrl: policy.publicBaseUrl,
      }),
    );
  });

  /**
   * The QR image for an invitation.
   *
   * Rendered on demand from the invite URL. It is a picture of a URL and nothing
   * more: scanning it gets the receiver to the receive page, not to a proof of
   * anything.
   */
  app.get("/t/:token/qr.svg", async (c) => {
    const token = c.req.param("token");
    const tx = await db.one("select id, state, expires_at_ms from transmission where token_hash=$1", [
      sha256Hex(token),
    ]);
    if (!tx) return c.text("not found", 404);
    const url = `${policy.publicBaseUrl}${(brand.urls?.receivePathTemplate ?? "/t/{token}").replace("{token}", token)}`;
    const svg = renderQrSvg(url);
    c.header("content-type", "image/svg+xml");
    c.header("cache-control", "no-store");
    return c.body(svg);
  });

  /** JSON resolution used by the receive page and by tests. */
  app.get("/v1/transmissions/resolve", async (c) => {
    const token = c.req.query("token") ?? "";
    const tx = await db.one("select * from transmission where token_hash = $1", [sha256Hex(token)]);
    if (!tx) throw new ApiError(404, "TRANSMISSION_NOT_FOUND", "invitation not found");
    return c.json({
      id: tx.id,
      state: tx.state,
      expires_at_ms: Number(tx.expires_at_ms),
      expired: Number(tx.expires_at_ms) < nowMs(),
    });
  });

  /**
   * NFC 光石 entry.
   *
   * A tag holds only a public URL with a public tag id. Touching it must never
   * create or claim anything: this route is a *lookup*. If the owner has no
   * active invitation open right now, the visitor sees a waiting page and no
   * session, identity or relation is created (F3 §6).
   */
  app.get("/n/:tagId", async (c) => {
    const tagId = c.req.param("tagId");
    // A tag may point at the owner's most recent open invitation, but only if
    // the owner explicitly opened one; otherwise nothing happens.
    const open = await db.one(
      `select t.id, t.state, t.expires_at_ms
         from transmission t
         join transmission_party p on p.transmission_id = t.id and p.role = 'INITIATOR'
         join identity i on i.id = p.identity_id
        where i.anon_id = $1
          and t.state in ('CREATED','OFFERED','RECEIVED')
          and t.expires_at_ms > $2
        order by t.created_at_ms desc limit 1`,
      [tagId, nowMs()],
    );
    if (!open) {
      return c.html(
        renderReceivePage({
          brand,
          state: "CREATED",
          stateLabelOverride: "这块光石现在没有开启的传光",
          noteOverride:
            "光石本身只是一个公开入口，不包含任何身份或权限。等主人开启一次传光后，再触碰它。",
        }),
      );
    }
    return c.html(
      renderReceivePage({
        brand,
        state: open.state,
        transmissionId: open.id,
        expiresAtMs: Number(open.expires_at_ms),
        baseUrl: policy.publicBaseUrl,
      }),
    );
  });

  // ---- confirmation flow -----------------------------------------------

  app.post("/v1/transmissions/:id/accept", async (c) =>
    auth(c, async ({ caller, body, bodyText, c: ctx }) =>
      idempotent(caller, "transmissions.accept", bodyText, ctx, async () => {
        const tx = await loadTransmission(c.req.param("id"));
        if (tx.initiator_id === caller.identityId) {
          throw new ApiError(403, "SELF_TRANSMISSION", "you cannot receive your own light");
        }
        // Opening the invitation is a real event: the receiver looked at it.
        let current = tx;
        if (current.state === "CREATED") {
          await transition(current, "OFFER", caller, null, { silent: true });
          current = await loadTransmission(tx.id);
        }
        if (current.state === "OFFERED") {
          await transition(current, "OPEN", caller, null, { silent: true });
          current = await loadTransmission(tx.id);
        }
        const fresh = current;
        // The receiver is bound to this session exactly once.
        const existing = await db.one(
          `select identity_id from transmission_party where transmission_id=$1 and role='RECEIVER'`,
          [fresh.id],
        );
        if (existing?.identity_id && existing.identity_id !== caller.identityId) {
          throw new ApiError(409, "SESSION_TAKEN", "this invitation was already claimed");
        }
        await db.run(
          `insert into transmission_party (transmission_id, role, identity_id, confirmed_at_ms)
           values ($1,'RECEIVER',$2,$3)
           on conflict (transmission_id, role) do update
             set identity_id = excluded.identity_id, confirmed_at_ms = excluded.confirmed_at_ms`,
          [fresh.id, caller.identityId, nowMs()],
        );
        return transition(fresh, "ACCEPT", caller, ctx);
      }),
    ),
  );

  app.post("/v1/transmissions/:id/confirm", async (c) =>
    auth(c, async ({ caller, c: ctx }) => {
      const tx = await loadTransmission(c.req.param("id"));
      requireInitiator(tx, caller);
      // Confirming is the sender's second act: it only means something after the
      // receiver has already accepted.
      const receiver = await db.one(
        `select identity_id, confirmed_at_ms from transmission_party
          where transmission_id=$1 and role='RECEIVER'`,
        [tx.id],
      );
      if (!receiver?.confirmed_at_ms) {
        throw new ApiError(409, "STATE_CONFLICT", "receiver has not accepted yet");
      }
      return transition(tx, "CONFIRM", caller, ctx);
    }),
  );

  app.post("/v1/transmissions/:id/reject", async (c) =>
    auth(c, async ({ caller, c: ctx }) => {
      const tx = await loadTransmission(c.req.param("id"));
      requireReceiver(tx, caller);
      return transition(tx, "REJECT", caller, ctx);
    }),
  );

  app.post("/v1/transmissions/:id/cancel", async (c) =>
    auth(c, async ({ caller, c: ctx }) => {
      const tx = await loadTransmission(c.req.param("id"));
      requireInitiator(tx, caller);
      return transition(tx, "CANCEL", caller, ctx);
    }),
  );

  app.post("/v1/transmissions/:id/revoke", async (c) =>
    auth(c, async ({ caller, c: ctx }) => {
      const tx = await loadTransmission(c.req.param("id"));
      if (tx.initiator_id !== caller.identityId) {
        const receiver = await db.one(
          `select identity_id from transmission_party where transmission_id=$1 and role='RECEIVER'`,
          [tx.id],
        );
        if (receiver?.identity_id !== caller.identityId) {
          throw new ApiError(403, "FORBIDDEN", "not a party to this session");
        }
      }
      return transition(tx, "REVOKE", caller, ctx);
    }),
  );

  app.get("/v1/transmissions/:id", async (c) =>
    auth(c, async ({ caller, c: ctx }) => {
      const tx = await loadTransmission(c.req.param("id"));
      const parties = await db.all(
        "select role, identity_id, confirmed_at_ms from transmission_party where transmission_id=$1",
        [tx.id],
      );
      if (!parties.some((p) => p.identity_id === caller.identityId)) {
        throw new ApiError(404, "TRANSMISSION_NOT_FOUND", "not found");
      }
      return ctx.json({
        id: tx.id,
        state: tx.state,
        version: Number(tx.version),
        expires_at_ms: Number(tx.expires_at_ms),
        parties: parties.map((p) => ({ role: p.role, confirmed: Boolean(p.confirmed_at_ms) })),
      });
    }),
  );

  // ---- evidence ---------------------------------------------------------

  app.post("/v1/usage/attest", async (c) =>
    auth(c, async ({ caller, body, bodyText, c: ctx }) =>
      idempotent(caller, "usage.attest", bodyText, ctx, async () => {
        assertNoForbiddenFields(body);
        requireFields(body, ["event_id", "model_family", "evidence_level", "proof_method",
                             "adapter_id", "observed_at_ms"]);

        const level = String(body.evidence_level).toUpperCase();
        if (!COUNTABLE_LEVELS.has(level)) {
          throw new ApiError(422, "NOT_COUNTABLE", "only E3/E4 can be attested as countable");
        }
        if (!/^[a-z0-9][a-z0-9_-]{0,40}$/.test(body.model_family)) {
          throw new ApiError(422, "FAMILY_UNKNOWN", "model_family must be a registry slug");
        }

        const hourAgo = nowMs() - 3600 * 1000;
        const [{ count }] = await db.all(
          "select count(*)::int as count from usage_event where identity_id=$1 and received_at_ms>$2",
          [caller.identityId, hourAgo],
        );
        if (count >= policy.maxAttestPerHour) {
          throw new ApiError(429, "RATE_LIMITED", "attestation rate limit reached");
        }

        const legacyClass = process.env.FL_ACTIVATION_MODE === "legacy-fixture"
          ? "TEST_FIXTURE" : "UNSIGNED_CLAIM";
        const inserted = await db.run(
          `insert into usage_event
            (id, identity_id, model_family, evidence_level, proof_method, adapter_id,
             adapter_version, registry_semver, observed_at_ms, received_at_ms, source_class)
           values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
           on conflict (identity_id, model_family) do nothing`,
          [body.event_id, caller.identityId, body.model_family,
           level === "E4" ? 4 : 3, body.proof_method, body.adapter_id,
           body.adapter_version ?? "unknown", body.registry_semver ?? "unknown",
           Number(body.observed_at_ms), nowMs(), legacyClass],
        );
        const counted = inserted.rowCount > 0;
        const [{ count: familyCount }] = await db.all(
          "select count(*)::int as count from usage_event where identity_id=$1",
          [caller.identityId],
        );

        return ctx.json(
          { counted, verified_families: familyCount, level: level },
          counted ? 201 : 200,
        );
      }),
    ),
  );

  // ---- activation: the only path that makes a transmission successful ---

  /**
   * The pre-M1.1 activation, kept only so fixture-driven tests can still exercise the
   * FSM end to end. It is reachable only when FL_ACTIVATION_MODE=legacy-fixture AND the
   * usage row is labelled TEST_FIXTURE. It is not a rollback path: with the default
   * mode this function cannot be reached at all.
   */
  async function legacyActivateFixture(c, tx, caller, body, ctx) {
    const receiver = await db.one(
      `select identity_id from transmission_party where transmission_id=$1 and role='RECEIVER'`,
      [tx.id],
    );
    if (receiver?.identity_id !== caller.identityId) {
      throw new ApiError(403, "FORBIDDEN", "only the receiver can activate");
    }
    if (tx.state === "ACTIVATED" || tx.state === "ACKNOWLEDGED") {
      return ctx.json({ ok: true, already_activated: true, state: tx.state, source: "TEST_FIXTURE" });
    }
    const evidence = await db.one(
      "select * from usage_event where id=$1 and identity_id=$2",
      [body.usage_event_id, caller.identityId],
    );
    if (!evidence) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "no such usage event for this identity");
    const edge = await db.one("select * from light_edge where transmission_id=$1", [tx.id]);
    if (!edge) throw new ApiError(409, "STATE_CONFLICT", "no relation exists for this session");
    if (Number(evidence.received_at_ms) <= Number(edge.created_at_ms)) {
      throw new ApiError(409, "EVIDENCE_PRECEDES_RELATION", "this usage happened before the light was shared");
    }
    const kind = await classifyKind(edge, caller.identityId, evidence.model_family);
    const outcome = await transition(tx, "ACTIVATE", caller, ctx, {
      activationEvidenceOk: true, returnOutcome: true,
    });
    await db.run(
      `update light_edge set state='ACTIVATED', activated_at_ms=$2, kind=$3 where id=$1`,
      [edge.id, nowMs(), kind],
    );
    await db.run(
      `insert into outbox_event (kind, identity_id, payload, created_at_ms)
       values ('edge.activated', $1, $2, $3)`,
      [edge.helper_id,
       JSON.stringify({ edge_id: edge.id, kind, message: "你传出的一束光，已经被点亮。" }),
       nowMs()],
    );
    return ctx.json({ ok: true, state: outcome.state, kind, already_activated: false, source: "TEST_FIXTURE" });
  }

  app.post("/v1/transmissions/:id/activate", async (c) =>
    auth(c, async ({ caller, body, bodyText, c: ctx }) =>
      idempotent(caller, "transmissions.activate", bodyText, ctx, async () => {
        assertNoForbiddenFields(body);
        const tx = await loadTransmission(c.req.param("id"));

        // Legacy self-declared evidence may not activate anything anymore. The escape
        // hatch exists only for fixture-driven tests and must never be a production
        // rollback: FL_ACTIVATION_MODE=legacy-fixture is explicit, and any other value
        // keeps the signed-credential requirement.
        const mode = process.env.FL_ACTIVATION_MODE || "v2";
        if (!body.credential_id) {
          if (mode === "legacy-fixture" && body.usage_event_id) {
            const ev = await db.one(
              "select source_class from usage_event where id=$1 and identity_id=$2",
              [body.usage_event_id, caller.identityId],
            );
            if (ev && ev.source_class === "TEST_FIXTURE") {
              return legacyActivateFixture(c, tx, caller, body, ctx);
            }
          }
          throw new ApiError(426, "UPGRADE_REQUIRED",
            "activation now requires a signed credential; unsigned usage events cannot activate");
        }

        const decision = await evaluateActivation({
          db,
          transmission: tx,
          callerIdentityId: caller.identityId,
          callerAnonId: caller.anonId,
          body,
          nowMs: nowMs(),
          windowMs: policy.activationWindowMs,
        });

        if (decision.already) {
          return ctx.json({ ok: true, already_activated: true, state: tx.state });
        }

        const kind = await classifyKind(decision.edge, caller.identityId, decision.credential.model_family);

        // One transaction: consume both grants, move the relation, write exactly one echo.
        const out = await db.tx(async () => {
          await consumeActivationGrants(db, {
            challengeId: body.challenge_id,
            credentialId: body.credential_id,
            transmissionId: tx.id,
            nowMs: nowMs(),
          });
          const outcome = await transition(tx, "ACTIVATE", caller, ctx, {
            activationEvidenceOk: true,
            returnOutcome: true,
          });
          await db.run(
            `update light_edge set state='ACTIVATED', activated_at_ms=$2, kind=$3 where id=$1`,
            [decision.edge.id, nowMs(), kind],
          );
          await db.run(
            `insert into outbox_event (kind, identity_id, payload, created_at_ms)
             values ('edge.activated', $1, $2, $3)`,
            [decision.edge.helper_id,
             JSON.stringify({ edge_id: decision.edge.id, kind, message: "你传出的一束光，已经被点亮。" }),
             nowMs()],
          );
          return outcome;
        });

        return ctx.json({
          ok: true,
          state: out.state,
          kind,
          already_activated: false,
          status: decision.status,
        });
      }),
    ),
  );

  app.get("/v1/light/paths", async (c) =>
    auth(c, async ({ caller, c: ctx }) => {
      const outgoing = await db.all(
        `select e.*, t.state as transmission_state
           from light_edge e join transmission t on t.id = e.transmission_id
          where e.helper_id = $1 order by e.created_at_ms desc limit 100`,
        [caller.identityId],
      );
      const incoming = await db.all(
        `select e.*, t.state as transmission_state
           from light_edge e join transmission t on t.id = e.transmission_id
          where e.helped_id = $1 order by e.created_at_ms desc limit 100`,
        [caller.identityId],
      );
      const echoes = await db.all(
        "select id, payload, created_at_ms from outbox_event where identity_id=$1 order by id desc limit 50",
        [caller.identityId],
      );
      const [{ count: transmitted }] = await db.all(
        `select count(*)::int as count from light_edge
          where helper_id=$1 and state='ACTIVATED'
            and kind in ('FIRST_LIGHT','NEW_DISCOVERY')`,
        [caller.identityId],
      );
      return ctx.json({
        transmitted_nodes: transmitted,
        outgoing: outgoing.map(edgeView),
        incoming: incoming.map(edgeView),
        echoes: echoes.map((e) => ({
          id: Number(e.id),
          at_ms: Number(e.created_at_ms),
          message: safeParse(e.payload)?.message ?? "回响",
          read: false,
        })),
      });
    }),
  );


  // ---- M1: signed usage credentials (feature-flagged) ----------------------
  // The flag exists so this path can be rolled back without a schema change.
  const V2 = () => process.env.FL_VERIFIER_V2 === "on";
  const ctxJson = (c, obj, status) => c.json(obj, status);

  app.post("/v1/keys/register", async (c) =>
    auth(c, async ({ caller, body, c: ctx }) => {
      if (!V2()) throw new ApiError(404, "NOT_ENABLED", "verifier v2 disabled");
      requireFields(body, ["key_id", "pubkey_hex"]);
      if (!/^[0-9a-f]{64}$/.test(body.pubkey_hex)) {
        throw new ApiError(422, "BAD_FIELD", "pubkey must be 32 bytes hex");
      }
      await db.run(
        `insert into device_key (id, identity_id, key_id, pubkey_hex, adapter_id, scope, created_at_ms)
         values ($1,$2,$3,$4,$5,$6,$7)
         on conflict (identity_id, key_id) do nothing`,
        [`dk_${caller.identityId}_${body.key_id}`, caller.identityId, body.key_id,
         body.pubkey_hex, body.adapter_id ?? null, body.scope === "any" ? "any" : "usage", nowMs()],
      );
      return ctx.json({ key_id: body.key_id, status: "ACTIVE" }, 201);
    }),
  );

  app.post("/v1/keys/revoke", async (c) =>
    auth(c, async ({ caller, body, c: ctx }) => {
      if (!V2()) throw new ApiError(404, "NOT_ENABLED", "verifier v2 disabled");
      requireFields(body, ["key_id"]);
      const n = await db.run(
        `update device_key set status='REVOKED', revoked_at_ms=$1
          where identity_id=$2 and key_id=$3 and status='ACTIVE'`,
        [nowMs(), caller.identityId, body.key_id],
      );
      return ctx.json({ revoked: n.rowCount });
    }),
  );

  /** One-time challenge, bound to an identity, a transfer and an expiry. */
  app.post("/v1/challenges", async (c) =>
    auth(c, async ({ caller, body, c: ctx }) => {
      if (!V2()) throw new ApiError(404, "NOT_ENABLED", "verifier v2 disabled");
      requireFields(body, ["transfer_id"]);
      const t = await db.one(
        `select t.id, p.identity_id from transmission t
           join transmission_party p on p.transmission_id = t.id and p.role='RECEIVER'
          where t.id=$1`,
        [body.transfer_id],
      );
      if (!t) throw new ApiError(404, "NOT_FOUND", "unknown transfer");
      if (t.identity_id !== caller.identityId) {
        throw new ApiError(403, "FORBIDDEN", "only the receiver may request a usage challenge");
      }
      const id = `ch_${sha256Hex(`${caller.identityId}:${body.transfer_id}:${nowMs()}:${Math.random()}`).slice(0, 24)}`;
      const nonce = sha256Hex(`${id}:nonce`).slice(0, 32);
      await db.run(
        `insert into challenge (id, identity_id, purpose, transfer_id, peer_identity_id, nonce, issued_at_ms, expires_at_ms)
         values ($1,$2,'usage',$3,$4,$5,$6,$7)`,
        [id, caller.identityId, body.transfer_id, t.identity_id, nonce, nowMs(), nowMs() + 15 * 60_000],
      );
      return ctx.json({ challenge_id: id, nonce, expires_at_ms: nowMs() + 15 * 60_000 }, 201);
    }),
  );

  /** The signed attestation path. Separate from /v1/usage/attest on purpose. */
  app.post("/v1/usage/attest-v2", async (c) =>
    auth(c, async ({ caller, body, c: ctx }) => {
      if (!V2()) throw new ApiError(404, "NOT_ENABLED", "verifier v2 disabled");
      const payload = body.credential;
      const key = await db.one(
        "select pubkey_hex, status, adapter_id from device_key where identity_id=$1 and key_id=$2",
        [caller.identityId, payload?.issuer_key_id],
      );
      if (!key) throw new ApiError(422, "KEY_UNKNOWN", "issuer key not registered");

      let verdict;
      try {
        verdict = verifyCredential({
        payload,
        publicKeyHex: key.pubkey_hex,
          keyStatus: key.status,
          nowMs: nowMs(),
        });
      } catch (e) {
        // Shape violations are a 422, not a 500: the caller sent something the protocol
        // does not define, and that is an answerable client error.
        if (e.code) throw new ApiError(422, "BAD_CREDENTIAL", e.message);
        throw e;
      }

      if (verdict.body.subject_identity_id !== caller.anonId) {
        throw new ApiError(422, "SUBJECT_MISMATCH", "subject does not match caller");
      }
      // A credential that does not verify is not stored at all: an invalid row in the
      // table is a trap for every later reader.
      if (verdict.signatureStatus !== SIG.VALID) {
        return ctx.json({
          accepted: false, signature_status: verdict.signatureStatus,
          source_class: verdict.sourceClass, reasons: verdict.reasons,
          eligible: { title: false, activate: false, public_field: false },
        }, 200);
      }
      const stored = await db.run(
        `insert into credential (id, identity_id, key_id, protocol_version, source_type, source_class,
             adapter_id, adapter_version, model_family, observation_method, observed_at_ms,
             issued_at_ms, transfer_id, challenge_id, signature_status, payload_sha256, received_at_ms)
         values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)
         on conflict (identity_id, id) do nothing`,
        [verdict.body.credential_id, caller.identityId, verdict.body.issuer_key_id,
         verdict.body.protocol_version, verdict.body.source_type, verdict.sourceClass,
         verdict.body.adapter_id, verdict.body.adapter_version, verdict.body.model_family,
         verdict.body.observation_method, verdict.body.observed_at, verdict.body.issued_at,
         verdict.body.transfer_id ?? null, verdict.body.challenge_id ?? null,
         verdict.signatureStatus, payloadHash(verdict.body), nowMs()],
      );

      // A valid signature on a self-claim is still a self-claim: it may be stored and
      // shown, but it is not activatable and does not enter the public field.
      const pol = POLICY[verdict.sourceClass] ?? POLICY[SOURCE.UNSIGNED_CLAIM];
      return ctx.json({
        accepted: stored.rowCount > 0,
        duplicate: stored.rowCount === 0,
        signature_status: verdict.signatureStatus,
        source_class: verdict.sourceClass,
        reasons: verdict.reasons,
        eligible: { title: pol.title && verdict.signatureStatus === SIG.VALID,
                    activate: pol.activate && verdict.signatureStatus === SIG.VALID,
                    public_field: pol.publicField && verdict.signatureStatus === SIG.VALID },
      }, stored.rowCount > 0 ? 201 : 200);
    }),
  );


  // ---- M2.1: device identity + two-sided pairing ---------------------------

  /** Step 0: a device key announces itself. No subject yet — it belongs to nobody. */
  app.post("/v1/devices/register", async (c) => {
    const body = await c.req.json().catch(() => ({}));
    const deviceId = body.device_id;
    const pubkey = body.pubkey_hex;
    if (typeof deviceId !== "string" || !/^[A-Za-z0-9_-]{6,64}$/.test(deviceId)) {
      throw new ApiError(422, "BAD_FIELD", "device_id required");
    }
    if (typeof pubkey !== "string" || !/^[0-9a-f]{64}$/.test(pubkey)) {
      throw new ApiError(422, "BAD_FIELD", "pubkey must be 32 bytes hex");
    }
    await db.run(
      `insert into device_key (id, identity_id, key_id, pubkey_hex, adapter_id, scope, status, created_at_ms, status_v2)
       values ($1, null, $2, $3, $4, 'usage', 'ACTIVE', $5, 'PENDING')
       on conflict (id) do nothing`,
      [`dev_${deviceId}`, deviceId, pubkey, body.adapter_id ?? null, nowMs()],
    );
    return c.json({ device_id: deviceId, status: "PENDING" }, 201);
  });

  /** Step 1: the device opens a pairing session and shows a short code. */
  app.post("/v1/pairing/sessions", async (c) => {
    const body = await c.req.json().catch(() => ({}));
    const device = await db.one("select * from device_key where key_id=$1", [body.device_id]);
    if (!device) throw new ApiError(404, "DEVICE_UNKNOWN", "register the device first");
    if (device.status_v2 === "REVOKED") throw new ApiError(403, "DEVICE_REVOKED", "device revoked");

    const id = `pair_${randomBytes(9).toString("hex")}`;
    const code = makeCode();
    const expires = nowMs() + PAIRING_TTL_MS;
    await db.run(
      `insert into pairing_session (id, device_id, device_pubkey, code_hash, state, created_at_ms, expires_at_ms)
       values ($1,$2,$3,$4,'OFFERED',$5,$6)`,
      [id, device.id, device.pubkey_hex, codeHash(code), nowMs(), expires],
    );
    return c.json({
      session_id: id, code, expires_at_ms: expires,
      fingerprint: pairingFingerprint({ sessionId: id, devicePubkeyHex: device.pubkey_hex }),
    }, 201);
  });


  /** The phone only ever knows the short code, so approval is by code. */
  app.post("/v1/pairing/approve-by-code", async (c) => {
    // The mini program authenticates with a business session, not an epoch signature.
    const sess = await sessionFromHeaders(db, c.req.header("authorization"), nowMs());
    const body = await c.req.json().catch(() => ({}));
    const code = body.code;
    if (typeof code !== "string") throw new ApiError(422, "BAD_FIELD", "code required");
    const open = await db.one(
      `select * from pairing_session where code_hash=$1 and state='OFFERED'
        order by created_at_ms desc limit 1`,
      [codeHash(code)],
    );
    if (!open) throw new ApiError(404, "CODE_UNKNOWN", "no open pairing for this code");
    if (Number(open.expires_at_ms) <= nowMs()) {
      await db.run("update pairing_session set state='EXPIRED' where id=$1", [open.id]);
      throw new ApiError(410, "PAIRING_EXPIRED", "pairing code expired");
    }
    const updated = await db.run(
      `update pairing_session set state='APPROVED', subject_identity_id=$1, approved_at_ms=$2
        where id=$3 and state='OFFERED' returning id`,
      [sess.identityId, nowMs(), open.id],
    );
    if (updated.rowCount !== 1) throw new ApiError(409, "STATE_CONFLICT", "already approved");
    return c.json({
      session_id: open.id, state: "APPROVED",
      fingerprint: pairingFingerprint({ sessionId: open.id, devicePubkeyHex: open.device_pubkey }),
    });
  });

  /** The device polls the session while the user types the code on their phone. */
  app.get("/v1/pairing/sessions/:id/state", async (c) => {
    const session = await db.one("select state, expires_at_ms from pairing_session where id=$1", [c.req.param("id")]);
    if (!session) throw new ApiError(404, "NOT_FOUND", "no such pairing session");
    const expired = Number(session.expires_at_ms) <= nowMs() && session.state === "OFFERED";
    return c.json({ state: expired ? "EXPIRED" : session.state });
  });

  /** Step 2: the phone (an authenticated identity) approves with the code it was shown. */
  app.post("/v1/pairing/sessions/:id/approve", async (c) =>
    auth(c, async ({ caller, body, c: ctx }) => {
      const session = await db.one("select * from pairing_session where id=$1", [c.req.param("id")]);
      if (!session) throw new ApiError(404, "NOT_FOUND", "no such pairing session");
      if (session.state !== "OFFERED") throw new ApiError(409, "STATE_CONFLICT", `session is ${session.state}`);
      if (Number(session.expires_at_ms) <= nowMs()) {
        await db.run("update pairing_session set state='EXPIRED' where id=$1", [session.id]);
        throw new ApiError(410, "PAIRING_EXPIRED", "pairing code expired");
      }
      if (codeHash(body.code) !== session.code_hash) {
        throw new ApiError(403, "CODE_MISMATCH", "pairing code does not match");
      }
      const updated = await db.run(
        `update pairing_session set state='APPROVED', subject_identity_id=$1, approved_at_ms=$2
          where id=$3 and state='OFFERED' returning id`,
        [caller.identityId, nowMs(), session.id],
      );
      if (updated.rowCount !== 1) throw new ApiError(409, "STATE_CONFLICT", "already approved");
      return ctx.json({
        session_id: session.id, state: "APPROVED",
        fingerprint: pairingFingerprint({ sessionId: session.id, devicePubkeyHex: session.device_pubkey }),
      });
    }),
  );

  /** Step 3: the device proves key possession over this very session, and is bound. */
  app.post("/v1/pairing/sessions/:id/confirm", async (c) => {
    const body = await c.req.json().catch(() => ({}));
    const session = await db.one("select * from pairing_session where id=$1", [c.req.param("id")]);
    if (!session) throw new ApiError(404, "NOT_FOUND", "no such pairing session");
    if (session.state === "CONSUMED") throw new ApiError(409, "REPLAYED", "this pairing was already used");
    if (session.state !== "APPROVED") throw new ApiError(409, "NOT_APPROVED", "the phone has not approved yet");
    if (!verifyPairingProof({ session, code: body.code, deviceId: body.device_id, signatureHex: body.signature_hex })) {
      throw new ApiError(403, "PROOF_INVALID", "device did not prove possession of its key");
    }
    await db.tx(async () => {
      const consumed = await db.run(
        "update pairing_session set state='CONSUMED', consumed_at_ms=$1 where id=$2 and state='APPROVED' returning id",
        [nowMs(), session.id],
      );
      if (consumed.rowCount !== 1) throw new ApiError(409, "REPLAYED", "pairing consumed concurrently");
      await db.run(
        `update device_key set subject_identity_id=$1, status_v2='ACTIVE', identity_id=$1
          where id=$2 and status_v2 <> 'REVOKED'`,
        [session.subject_identity_id, session.device_id],
      );
    });
    const subject = await db.one("select anon_id from identity where id=$1", [session.subject_identity_id]);
    return c.json({
      state: "PAIRED",
      subject_anon_id: subject?.anon_id ?? null,
      fingerprint: pairingFingerprint({ sessionId: session.id, devicePubkeyHex: session.device_pubkey }),
    });
  });

  /** Unpairing is an explicit, authenticated act by the subject. */
  app.post("/v1/devices/:deviceId/revoke", async (c) =>
    auth(c, async ({ caller, c: ctx }) => {
      const n = await db.run(
        `update device_key set status_v2='REVOKED', status='REVOKED', revoked_at_ms=$1
          where key_id=$2 and identity_id=$3 and status_v2='ACTIVE'`,
        [nowMs(), c.req.param("deviceId"), caller.identityId],
      );
      return ctx.json({ revoked: n.rowCount });
    }),
  );


  // ---- M2.2: device-signed endpoints (the plugin has no identity keys) -----

  /** A bound device reports its own state. Proof is a signature over a nonce. */
  app.get("/v1/devices/:deviceId", async (c) => {
    const device = await db.one("select * from device_key where key_id=$1", [c.req.param("deviceId")]);
    if (!device) throw new ApiError(404, "DEVICE_UNKNOWN", "unknown device");
    const identity = device.subject_identity_id
      ? await db.one("select anon_id from identity where id=$1", [device.subject_identity_id])
      : null;
    return c.json({
      device_id: device.key_id,
      status: device.status_v2,
      paired: device.status_v2 === "ACTIVE",
      subject_anon_id: identity?.anon_id ?? null,
    });
  });

  /** Challenge claim, restricted to the bound device of this very receiver. */
  app.post("/v1/devices/:deviceId/challenge", async (c) => {
    const body = await c.req.json().catch(() => ({}));
    const device = await db.one("select * from device_key where key_id=$1", [c.req.param("deviceId")]);
    if (!device) throw new ApiError(404, "DEVICE_UNKNOWN", "unknown device");
    if (device.status_v2 !== "ACTIVE" || !device.subject_identity_id) {
      throw new ApiError(403, "DEVICE_NOT_PAIRED", "device is not bound to a subject");
    }
    const transferId = body.transfer_id;
    if (typeof transferId !== "string") throw new ApiError(422, "BAD_FIELD", "transfer_id required");

    // the receiver of this transfer must be the subject this device is bound to
    const receiver = await db.one(
      "select identity_id from transmission_party where transmission_id=$1 and role='RECEIVER'",
      [transferId],
    );
    if (!receiver) throw new ApiError(404, "NOT_FOUND", "unknown transfer");
    if (receiver.identity_id !== device.subject_identity_id) {
      throw new ApiError(403, "NOT_YOUR_TRANSFER", "this transfer's receiver is not bound to this device");
    }
    const edge = await db.one("select state from light_edge where transmission_id=$1", [transferId]);
    if (!edge || !["FORMED", "LEARNING"].includes(edge.state)) {
      throw new ApiError(409, "RELATION_NOT_READY", "the relation is not ready for usage evidence");
    }
    const open = await db.one(
      `select id, nonce, expires_at_ms from challenge
        where transfer_id=$1 and peer_identity_id=$2 and purpose='usage'
          and consumed_at_ms is null and expires_at_ms > $3
        order by issued_at_ms desc limit 1`,
      [transferId, device.subject_identity_id, nowMs()],
    );
    if (!open) throw new ApiError(404, "NO_OPEN_CHALLENGE", "request one from the phone first");
    return c.json({ challenge_id: open.id, nonce: open.nonce, expires_at_ms: Number(open.expires_at_ms) });
  });

  /** A device submits a credential it signed on behalf of its bound subject. */
  app.post("/v1/usage/attest-device", async (c) => {
    const body = await c.req.json().catch(() => ({}));
    const credential = body.credential;
    if (!credential || typeof credential !== "object") {
      throw new ApiError(422, "BAD_FIELD", "credential required");
    }
    const device = await db.one("select * from device_key where key_id=$1", [credential.issuer_key_id]);
    if (!device) throw new ApiError(422, "KEY_UNKNOWN", "issuer key not registered");
    if (device.status_v2 !== "ACTIVE" || !device.subject_identity_id) {
      throw new ApiError(403, "DEVICE_NOT_PAIRED", "device is not bound to a subject");
    }
    if (credential.subject_identity_id) {
      const subject = await db.one("select anon_id from identity where id=$1", [device.subject_identity_id]);
      if (!subject || subject.anon_id !== credential.subject_identity_id) {
        throw new ApiError(422, "SUBJECT_MISMATCH", "credential names a subject this device is not bound to");
      }
    }

    const verdict = verifyCredential({
      payload: credential, publicKeyHex: device.pubkey_hex, keyStatus: "ACTIVE", nowMs: nowMs(),
    });
    if (verdict.signatureStatus !== SIG.VALID) {
      return c.json({
        accepted: false, signature_status: verdict.signatureStatus,
        source_class: verdict.sourceClass, reasons: verdict.reasons,
      }, 200);
    }
    const stored = await db.run(
      `insert into credential (id, identity_id, key_id, protocol_version, source_type, source_class,
           adapter_id, adapter_version, model_family, observation_method, observed_at_ms,
           issued_at_ms, transfer_id, challenge_id, signature_status, payload_sha256, received_at_ms)
       values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)
       on conflict (identity_id, id) do nothing`,
      [verdict.body.credential_id, device.subject_identity_id, verdict.body.issuer_key_id,
       verdict.body.protocol_version, verdict.body.source_type, verdict.sourceClass,
       verdict.body.adapter_id, verdict.body.adapter_version, verdict.body.model_family,
       verdict.body.observation_method, verdict.body.observed_at, verdict.body.issued_at,
       verdict.body.transfer_id ?? null, verdict.body.challenge_id ?? null,
       verdict.signatureStatus, payloadHash(verdict.body), nowMs()],
    );
    const pol = POLICY[verdict.sourceClass] ?? POLICY[SOURCE.UNSIGNED_CLAIM];
    return ctxJson(c, {
      accepted: stored.rowCount > 0,
      duplicate: stored.rowCount === 0,
      signature_status: verdict.signatureStatus,
      source_class: verdict.sourceClass,
      reasons: verdict.reasons,
      eligible: { title: pol.title, activate: pol.activate, public_field: pol.publicField },
    }, stored.rowCount > 0 ? 201 : 200);
  });


  // ---- M3-B: mini program login -------------------------------------------

  /** Development adapter. Refused outright in production. */
  app.post("/v1/auth/dev-login", async (c) => {
    if (productionMode() || process.env.FL_DEV_ADAPTER !== "on") {
      throw new ApiError(404, "NOT_ENABLED", "dev login is not available");
    }
    const body = await c.req.json().catch(() => ({}));
    const user = String(body.user || "dev-user").slice(0, 64);
    const anonId = `fl1_dev_${createHashShort(user)}`;
    let identity = await db.one("select id from identity where anon_id=$1", [anonId]);
    if (!identity) {
      const id = `id_${randomBytes(8).toString("hex")}`;
      await db.run(
        "insert into identity (id, anon_id, root_pubkey, status, created_at_ms) values ($1,$2,$3,'ACTIVE',$4)",
        [id, anonId, "0".repeat(64), nowMs()],
      );
      identity = { id };
    }
    const token = newToken();
    await db.run(
      `insert into auth_session (id, identity_id, token_hash, auth_method, created_at_ms, expires_at_ms)
       values ($1,$2,$3,'DEV_ADAPTER',$4,$5)`,
      [`sess_${randomBytes(9).toString("hex")}`, identity.id, tokenHash(token), nowMs(), nowMs() + SESSION_TTL_MS],
    );
    return c.json({
      mode: "DEV_ADAPTER", token, expires_at_ms: nowMs() + SESSION_TTL_MS,
      anon_id: anonId, note: "SIMULATED IDENTITY — not a real WeChat account",
    }, 201);
  });

  /** Real login: wx.login code in, business session out. */
  app.post("/v1/auth/wechat-login", async (c) => {
    const body = await c.req.json().catch(() => ({}));
    const code = body.code;
    if (typeof code !== "string" || code.length < 8) throw new ApiError(422, "BAD_FIELD", "code required");
    const appId = process.env.FL_WECHAT_APP_ID;
    const appSecret = process.env.FL_WECHAT_APP_SECRET;
    if (!appId || !appSecret) throw new ApiError(503, "WECHAT_NOT_CONFIGURED", "AppID/AppSecret not configured");

    let openId;
    try {
      ({ openId } = await code2Session(code, { appId, appSecret, fetchImpl: fetch }));
    } catch (e) {
      throw new ApiError(401, "WECHAT_CODE_INVALID", e.message);
    }
    // one code, one use: the unique constraint plus this table make replay fail
    try {
      await db.run("insert into wechat_code (code_hash, consumed_at_ms) values ($1,$2)",
        [createHashShort(code), nowMs()]);
    } catch (e) {
      // only a uniqueness violation means replay; anything else is a real failure
      if (/duplicate key|unique/i.test(String(e.message))) {
        throw new ApiError(409, "CODE_REPLAYED", "this login code was already used");
      }
      throw e;
    }

    let subject = await db.one("select * from wechat_subject where app_id=$1 and open_id=$2", [appId, openId]);
    if (!subject) {
      const anonId = `fl1_wx_${createHashShort(`${appId}:${openId}`)}`;
      const id = `id_${randomBytes(8).toString("hex")}`;
      await db.run(
        "insert into identity (id, anon_id, root_pubkey, status, created_at_ms) values ($1,$2,$3,'ACTIVE',$4)",
        [id, anonId, "0".repeat(64), nowMs()],
      );
      await db.run(
        "insert into wechat_subject (id, app_id, open_id, identity_id, created_at_ms) values ($1,$2,$3,$4,$5)",
        [`wx_${randomBytes(8).toString("hex")}`, appId, openId, id, nowMs()],
      );
      subject = { identity_id: id };
    }
    const token = newToken();
    await db.run(
      `insert into auth_session (id, identity_id, token_hash, auth_method, created_at_ms, expires_at_ms)
       values ($1,$2,$3,'WECHAT',$4,$5)`,
      [`sess_${randomBytes(9).toString("hex")}`, subject.identity_id, tokenHash(token), nowMs(), nowMs() + SESSION_TTL_MS],
    );
    return c.json({
      mode: "WECHAT", token, expires_at_ms: nowMs() + SESSION_TTL_MS,
      anon_id: (await db.one("select anon_id from identity where id=$1", [subject.identity_id])).anon_id,
    }, 201);
  });

  app.get("/v1/auth/me", async (c) => {
    const s = await sessionFromHeaders(db, c.req.header("authorization"), nowMs());
    return c.json({ anon_id: s.anonId, identity_id: s.identityId, expires_at_ms: s.expiresAt });
  });

  app.post("/v1/auth/logout", async (c) => {
    const authz = c.req.header("authorization");
    const token = authz?.startsWith("Bearer ") ? authz.slice(7) : null;
    if (token) {
      await db.run("update auth_session set revoked_at_ms=$1 where token_hash=$2 and revoked_at_ms is null",
        [nowMs(), tokenHash(token)]);
    }
    return c.json({ ok: true });
  });


  // ---- M3-B: session-scoped reads (the mini program never passes a subject id) ----

  /** Every credential this subject owns, with its trust facts kept separate. */
  app.get("/v1/me/observations", async (c) => {
    const s = await sessionFromHeaders(db, c.req.header("authorization"), nowMs());
    const rows = await db.all(
      `select id, model_family, source_class, signature_status, adapter_id, observed_at_ms, transfer_id, challenge_id
         from credential where identity_id=$1 order by received_at_ms desc limit 50`,
      [s.identityId],
    );
    return c.json({
      anon_id: s.anonId,
      observations: rows.map((r) => ({
        id: r.id, model_family: r.model_family, source_class: r.source_class,
        signature_status: r.signature_status, adapter_id: r.adapter_id,
        observed_at_ms: Number(r.observed_at_ms),
        bound_to_transfer: Boolean(r.transfer_id),
        eligible: {
          title: r.source_class === "PLUGIN_OBSERVED" || r.source_class === "INDEPENDENTLY_CORROBORATED",
          activate: r.source_class === "INDEPENDENTLY_CORROBORATED",
          public_field: false,
        },
      })),
    });
  });

  /** The mini program's own view of its relations. Scoped by session, never by query. */
  app.get("/v1/me/paths", async (c) => {
    const s = await sessionFromHeaders(db, c.req.header("authorization"), nowMs());
    const outgoing = await db.all(
      `select e.transmission_id, e.state, e.kind, e.created_at_ms
         from light_edge e join transmission t on t.id=e.transmission_id
        where e.helper_id=$1 order by e.created_at_ms desc limit 50`, [s.identityId]);
    const incoming = await db.all(
      `select e.transmission_id, e.state, e.kind, e.created_at_ms
         from light_edge e join transmission t on t.id=e.transmission_id
        where e.helped_id=$1 order by e.created_at_ms desc limit 50`, [s.identityId]);
    const echoes = await db.all(
      "select id, payload, created_at_ms from outbox_event where identity_id=$1 order by id desc limit 50",
      [s.identityId]);
    return c.json({
      outgoing: outgoing.map(view), incoming: incoming.map(view),
      echoes: echoes.map((e) => ({ id: Number(e.id), at_ms: Number(e.created_at_ms) })),
    });
    function view(e) {
      return { transmission_id: e.transmission_id, state: e.state, kind: e.kind, at_ms: Number(e.created_at_ms) };
    }
  });

  /** Devices bound to the signed-in subject. */
  app.get("/v1/me/devices", async (c) => {
    const s = await sessionFromHeaders(db, c.req.header("authorization"), nowMs());
    const rows = await db.all(
      `select key_id, status_v2, created_at_ms, revoked_at_ms
         from device_key where subject_identity_id=$1 order by created_at_ms desc`, [s.identityId]);
    return c.json({
      devices: rows.map((r) => ({
        device_id: r.key_id, status: r.status_v2, paired: r.status_v2 === "ACTIVE",
        created_at_ms: Number(r.created_at_ms), revoked_at_ms: r.revoked_at_ms ? Number(r.revoked_at_ms) : null,
      })),
    });
  });

  /** Public view of explicitly published edges. Used by T12. */
  app.get("/v1/light/public/:anonId", async (c) => {
    const anonId = c.req.param("anonId");
    const identity = await db.one("select id from identity where anon_id=$1", [anonId]);
    if (!identity) return c.json({ edges: [] });
    const consentRow = await db.one(
      "select state from consent where identity_id=$1 and scope='PUBLIC_LIGHT_PATH'",
      [identity.id],
    );
    if (consentRow?.state !== "GRANTED") return c.json({ edges: [], consent: "not_granted" });
    const edges = await db.all(
      `select kind, state, created_at_ms from light_edge
        where helper_id=$1 and visibility='ANONYMOUS_PUBLIC' and state<>'REVOKED'
        order by created_at_ms desc limit 50`,
      [identity.id],
    );
    return c.json({
      edges: edges.map((e) => ({ kind: e.kind, state: e.state, at_ms: Number(e.created_at_ms) })),
      consent: "granted",
    });
  });

  // ---- privacy ----------------------------------------------------------

  app.post("/v1/privacy/consent", async (c) =>
    auth(c, async ({ caller, body, c: ctx }) => {
      requireFields(body, ["scope", "state"]);
      if (!["GLOBAL_FIELD_CONTRIBUTION", "PUBLIC_LIGHT_PATH"].includes(body.scope)) {
        throw new ApiError(400, "INVALID_SCHEMA", "unknown scope");
      }
      if (!["GRANTED", "DECLINED", "REVOKED"].includes(body.state)) {
        throw new ApiError(400, "INVALID_SCHEMA", "unknown state");
      }
      await db.run(
        `insert into consent (identity_id, scope, state, policy_version, changed_at_ms)
         values ($1,$2,$3,$4,$5)
         on conflict (identity_id, scope) do update
           set state=excluded.state, policy_version=excluded.policy_version,
               changed_at_ms=excluded.changed_at_ms`,
        [caller.identityId, body.scope, body.state, body.policy_version ?? "v1", nowMs()],
      );
      if (body.state !== "GRANTED") {
        // Revoking publication must take effect immediately, not on next write.
        await db.run(
          `update light_edge set visibility='PRIVATE' where helper_id=$1`,
          [caller.identityId],
        );
      }
      return ctx.json({ ok: true, scope: body.scope, state: body.state });
    }),
  );

  app.post("/v1/privacy/revoke", async (c) =>
    auth(c, async ({ caller, body, c: ctx }) => {
      const mode = body.mode ?? "LINKABLE_ONLY";
      let deletedUsage = 0;
      let revokedEdges = 0;
      await db.tx(async (tx) => {
        if (mode === "LINKABLE_ONLY" || mode === "ALL") {
          const r = await tx.run("delete from usage_event where identity_id=$1", [caller.identityId]);
          deletedUsage = r.rowCount;
        }
        if (mode === "ALL") {
          const r2 = await tx.run(
            `update light_edge set state='REVOKED', revoked_at_ms=$2
              where helper_id=$1 or helped_id=$1`,
            [caller.identityId, nowMs()],
          );
          revokedEdges = r2.rowCount;
          await tx.run(
            `update transmission set state='REVOKED', updated_at_ms=$2
              where id in (select transmission_id from transmission_party where identity_id=$1)
                and state not in ('CANCELLED','EXPIRED','REJECTED')`,
            [caller.identityId, nowMs()],
          );
        }
        await tx.run(
          "insert into audit_log (identity_id, action, detail, at_ms) values ($1,'privacy.revoke',$2,$3)",
          [caller.identityId, mode, nowMs()],
        );
      });
      return ctx.json({
        ok: true,
        mode,
        deleted_usage_events: deletedUsage,
        revoked_edges: revokedEdges,
        irreversible_note:
          "已经生成的匿名汇总（区域计数）不可逆，但不再包含任何可关联到你的事件。",
      });
    }),
  );

  // ---- SSE: real-time echoes -------------------------------------------

  app.get("/v1/events/stream", async (c) => {
    const url = new URL(c.req.url);
    const caller = await authenticate(db, {
      method: "GET",
      path: url.pathname,
      bodySha256Hex: sha256Hex(""),
      headers: c.req.header(),
      nowMs: nowMs(),
    });

    const lastEventId = Number(c.req.header("last-event-id") ?? c.req.query("lastEventId") ?? 0);
    const oldest = await db.one("select min(id) as id from outbox_event where identity_id=$1", [caller.identityId]);
    const oldestId = oldest?.id ? Number(oldest.id) : null;

    return streamSSE(c, async (stream) => {
      let cursor = Number.isFinite(lastEventId) ? lastEventId : 0;
      let sent = 0;
      const startedAt = nowMs();

      await stream.writeSSE({
        event: "hello",
        data: JSON.stringify({
          protocol_version: "v1",
          resumed_from: cursor || null,
          heartbeat_ms: policy.sseHeartbeatMs,
          replay_window_ms: policy.sseReplayWindowMs,
          rate_limit_per_minute: policy.sseMaxEventsPerMinute,
          oldest_event_id: oldestId,
        }),
      });

      let lastBeat = nowMs();
      let closed = false;
      stream.onAbort(() => { closed = true; });

      while (!closed) {
        const rows = await db.all(
          `select id, kind, payload, created_at_ms from outbox_event
            where identity_id=$1 and id > $2 order by id asc limit 20`,
          [caller.identityId, cursor],
        );
        for (const row of rows) {
          const id = Number(row.id);
          // Expired events must not be replayed as if they were live.
          if (nowMs() - Number(row.created_at_ms) > policy.sseReplayWindowMs && cursor === 0) {
            cursor = id;
            continue;
          }
          if (sent >= policy.sseMaxEventsPerMinute) break;
          await stream.writeSSE({
            id: String(id),
            event: row.kind,
            data: row.payload,
          });
          await db.run("update outbox_event set delivered_at_ms=$2 where id=$1", [id, nowMs()]);
          cursor = id;
          sent += 1;
        }

        if (nowMs() - lastBeat > policy.sseHeartbeatMs) {
          await stream.writeSSE({ event: "heartbeat", data: String(nowMs()) });
          lastBeat = nowMs();
        }
        // Close idle streams so a test client can finish; production would rely
        // on the client reconnecting with Last-Event-ID.
        if (nowMs() - startedAt > 20_000) break;
        await stream.sleep(300);
      }
    });
  });

  // ---- field summary stub (F4 compatibility, deliberately minimal) ------

  app.get("/v1/field/summary", async (c) => {
    const [{ count }] = await db.all(
      "select count(distinct helper_id)::int as count from light_edge where state='ACTIVATED'",
    );
    return c.json({
      generated_at_ms: nowMs(),
      cache_seconds: 60,
      metrics: {
        total_activated_nodes: {
          value: count,
          window: "lifetime",
          note: "已激活的匿名节点数，不是自然人数",
        },
        online_instances: { value: 0, window: "90s", note: "F4 实现" },
        new_activations: { value: 0, window: "24h", note: "F4 实现" },
        helper_nodes: { value: count, window: "lifetime", note: "至少帮助过一个节点完成激活" },
      },
      methodology_url: `${policy.publicBaseUrl}/methodology`,
      note: "F3 仅提供兼容形状，真实聚合在 F4。",
    });
  });

  // ---- internals --------------------------------------------------------

  async function loadTransmission(id) {
    const tx = await db.one(
      `select t.*,
              (select identity_id from transmission_party p
                where p.transmission_id=t.id and p.role='RECEIVER') as receiver_id,
              (select confirmed_at_ms from transmission_party p
                where p.transmission_id=t.id and p.role='RECEIVER') as receiver_confirmed,
              (select confirmed_at_ms from transmission_party p
                where p.transmission_id=t.id and p.role='INITIATOR') as initiator_confirmed,
              (select identity_id from transmission_party p
                where p.transmission_id=t.id and p.role='INITIATOR') as initiator_id
         from transmission t where t.id=$1`,
      [id],
    );
    if (!tx) throw new ApiError(404, "TRANSMISSION_NOT_FOUND", "session not found");
    return tx;
  }

  function requireInitiator(tx, caller) {
    if (tx.initiator_id !== caller.identityId) {
      throw new ApiError(403, "FORBIDDEN", "not the initiator");
    }
  }

  function requireReceiver(tx, caller) {
    if (tx.receiver_id !== caller.identityId) {
      throw new ApiError(403, "FORBIDDEN", "not the receiver");
    }
  }

  function requireParty(tx, caller) {
    if (tx.initiator_id !== caller.identityId && tx.receiver_id !== caller.identityId) {
      throw new ApiError(403, "FORBIDDEN", "not a party to this session");
    }
  }

  /**
   * Single transition entry point. Every mutation goes through the shared state
   * machine, so no endpoint can invent its own notion of "success".
   */
  async function transition(tx, event, caller, c, options = {}) {
    const session = {
      state: tx.state,
      initiatorConfirmed: Boolean(tx.initiator_confirmed),
      receiverConfirmed: Boolean(tx.receiver_confirmed),
      version: Number(tx.version),
    };
    const outcome = applyEvent(session, event, {
      nowMs: nowMs(),
      expiresAtMs: Number(tx.expires_at_ms),
      activationEvidenceOk: options.activationEvidenceOk ?? false,
    });

    if (!outcome.ok) {
      if (outcome.error === "Expired") {
        await db.run("update transmission set state='EXPIRED', updated_at_ms=$2 where id=$1",
          [tx.id, nowMs()]);
        throw new ApiError(410, "TRANSMISSION_EXPIRED", "session expired");
      }
      if (outcome.error === "ActivationEvidenceMissing") {
        throw new ApiError(409, "EVIDENCE_PENDING", "no countable evidence for this activation");
      }
      if (options.silent) return null;
      throw new ApiError(409, "STATE_CONFLICT", `cannot ${event} while ${tx.state}`);
    }

    // Optimistic concurrency: two devices confirming at once cannot both win.
    const updated = await db.run(
      `update transmission set state=$3, version=version+1, updated_at_ms=$4
        where id=$1 and version=$2`,
      [tx.id, Number(tx.version), outcome.state, nowMs()],
    );
    if (updated.rowCount === 0) {
      throw new ApiError(409, "STATE_CONFLICT", "concurrent modification");
    }

    await db.run(
      `insert into transmission_log (transmission_id, event, from_state, to_state, actor_id, at_ms)
       values ($1,$2,$3,$4,$5,$6)`,
      [tx.id, event, tx.state, outcome.state, caller?.identityId ?? null, nowMs()],
    );

    // Forming the relation is the moment the edge is born.
    if (outcome.effects.includes("EdgeFormed")) {
      const receiver = await db.one(
        "select identity_id from transmission_party where transmission_id=$1 and role='RECEIVER'",
        [tx.id],
      );
      const kind = await classifyAtFormation(caller?.identityId ?? tx.initiator_id, receiver.identity_id);
      await db.run(
        `insert into light_edge
          (id, transmission_id, helper_id, helped_id, kind, state, visibility, created_at_ms)
         values ($1,$2,$3,$4,$5,'FORMED','PRIVATE',$6)
         on conflict (transmission_id) do nothing`,
        [uuid(), tx.id, caller?.identityId ?? tx.initiator_id, receiver.identity_id, kind, nowMs()],
      );
    }
    if (outcome.effects.includes("EdgeRevoked")) {
      await db.run(
        "update light_edge set state='REVOKED', revoked_at_ms=$2 where transmission_id=$1",
        [tx.id, nowMs()],
      );
    }

    if (options.silent) return null;
    if (options.returnOutcome) return outcome;
    return c.json({
      id: tx.id,
      state: outcome.state,
      version: outcome.version,
      effects: outcome.effects,
      replayed: outcome.replayed,
    });
  }

  /**
   * Decide the propagation kind when the relation forms.
   *
   * FIRST_LIGHT  — the receiver had no countable usage at all before this.
   * NEW_DISCOVERY— the receiver already used AI, so this is about a new family.
   * RESONANCE    — two already-lit nodes meeting with nothing new pending.
   *
   * FIRST_LIGHT says "the first AI use this light seed confirmed". It is not a
   * claim about the person's real-world history.
   */
  async function classifyAtFormation(helperId, helpedId) {
    const [{ count: helpedBefore }] = await db.all(
      "select count(*)::int as count from usage_event where identity_id=$1",
      [helpedId],
    );
    if (helpedBefore === 0) return "FIRST_LIGHT";
    const [{ count: overlap }] = await db.all(
      `select count(*)::int as count from usage_event a
         join usage_event b on a.model_family = b.model_family
        where a.identity_id=$1 and b.identity_id=$2`,
      [helperId, helpedId],
    );
    // Both already had countable usage in common: meeting, not teaching.
    if (overlap > 0) return "RESONANCE";
    return "NEW_DISCOVERY";
  }

  /** Refine the kind at activation: a genuinely new family upgrades RESONANCE. */
  async function classifyKind(edge, helpedId, family) {
    const prior = await db.one(
      `select 1 from usage_event where identity_id=$1 and model_family=$2
        and received_at_ms <= $3`,
      [helpedId, family, Number(edge.created_at_ms)],
    );
    if (prior) return "RESONANCE";
    return edge.kind === "FIRST_LIGHT" ? "FIRST_LIGHT" : "NEW_DISCOVERY";
  }

  return app;
}

/**
 * Render a URL as an SVG QR code.
 *
 * Error correction M keeps the code readable on a screen while staying small
 * enough to stay crisp at the size the panel shows it.
 */
function renderQrSvg(url) {
  const qr = qrcode(0, "M");
  qr.addData(url);
  qr.make();
  const count = qr.getModuleCount();
  const quiet = 2;
  const total = count + quiet * 2;
  let path = "";
  for (let row = 0; row < count; row += 1) {
    for (let col = 0; col < count; col += 1) {
      if (qr.isDark(row, col)) {
        path += `M${col + quiet} ${row + quiet}h1v1h-1z`;
      }
    }
  }
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${total} ${total}" width="264" height="264" shape-rendering="crispEdges">
<rect width="${total}" height="${total}" fill="#0B1425"/>
<path d="${path}" fill="#F2E7D0"/>
</svg>`;
}

function edgeView(e) {
  return {
    id: e.id,
    kind: e.kind,
    state: e.state,
    visibility: e.visibility,
    created_at_ms: Number(e.created_at_ms),
    activated_at_ms: e.activated_at_ms ? Number(e.activated_at_ms) : null,
    acknowledged_at_ms: e.acknowledged_at_ms ? Number(e.acknowledged_at_ms) : null,
    transmission_state: e.transmission_state,
  };
}

function shortAnon(anonId) {
  if (!anonId) return "一位匿名的持光者";
  return `一位匿名的持光者（${anonId.slice(0, 9)}…）`;
}

function safeParse(text) {
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

export { isTerminal };
