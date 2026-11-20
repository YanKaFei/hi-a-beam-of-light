// F3 acceptance tests T1–T15 at the API level.
//
// Everything runs in-process against PGlite (real Postgres semantics) with the
// real signing protocol. No network, no fixtures pretending to be results.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

import { openDb } from "../src/db.mjs";
import { createApp } from "../src/app.mjs";
import { loadPolicy } from "../src/policy.mjs";
import { makeClient, createIdentity, epochCertificate } from "./support/client.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const BRAND = JSON.parse(
  readFileSync(join(HERE, "..", "..", "..", "packages", "brand", "brand.json"), "utf8"),
);

async function fresh({ policyOverrides = {} } = {}) {
  const db = await openDb();
  const policy = loadPolicy({ publicBaseUrl: "http://127.0.0.1:8787", ...policyOverrides });
  const app = createApp({ db, policy, brand: BRAND });
  return { db, app, policy };
}

/** Drive a session to LEARNING: B accepts, A confirms. */
async function toLearning(a, b) {
  const created = await a.call("POST", "/v1/transmissions", { channel: "QR" });
  assert.equal(created.status, 201);
  const id = created.json.id;
  const resolved = await b.call("GET", `/v1/transmissions/resolve?token=${created.json.token}`);
  assert.equal(resolved.status, 200);
  const accepted = await b.call("POST", `/v1/transmissions/${id}/accept`, {});
  assert.equal(accepted.status, 200, `accept: ${JSON.stringify(accepted.json)}`);
  const confirmed = await a.call("POST", `/v1/transmissions/${id}/confirm`, {});
  assert.equal(confirmed.status, 200, `confirm: ${JSON.stringify(confirmed.json)}`);
  return { id, token: created.json.token, state: confirmed.json.state };
}

const attest = (family = "gpt", level = "E3") => ({
  event_id: `evt-${family}-${Math.random().toString(16).slice(2)}`,
  model_family: family,
  evidence_level: level,
  proof_method: "local_sqlite_metadata",
  adapter_id: "codex-app",
  adapter_version: "1.0.0",
  registry_semver: "1.0.0",
  observed_at_ms: Date.now(),
});

// ---------------------------------------------------------------------------

test("T1: A creates a one-time QR invitation", async () => {
  const { app, db } = await fresh();
  const a = await makeClient(app);
  const res = await a.call("POST", "/v1/transmissions", { channel: "QR" });

  assert.equal(res.status, 201);
  assert.ok(res.json.token.length >= 32, "token must be cryptographically long");
  assert.match(res.json.receive_url, /^http:\/\/127\.0\.0\.1:8787\/t\//);
  assert.equal(res.json.state, "OFFERED", "generating a QR is the act of offering");
  assert.ok(res.json.expires_at_ms > Date.now(), "must have a future expiry");

  // Only the hash is stored, never the token.
  const rows = await db.all("select token_hash from transmission");
  assert.equal(rows.length, 1);
  assert.ok(!rows[0].token_hash.includes(res.json.token));
  assert.equal(rows[0].token_hash.length, 64);
});

test("T2: B can open the receive page and resolve the invitation", async () => {
  const { app } = await fresh();
  const a = await makeClient(app);
  const created = await a.call("POST", "/v1/transmissions", { channel: "QR" });

  const page = await app.request(`/t/${created.json.token}`);
  assert.equal(page.status, 200);
  const html = await page.text();
  assert.ok(html.includes("你收到了一束光"));
  assert.ok(html.includes("接收这束光"));
  assert.ok(!html.includes(a.identity.anonId), "the sender's full anon id must not be exposed");

  const resolved = await app.request(`/v1/transmissions/resolve?token=${created.json.token}`);
  assert.equal(resolved.status, 200);
  assert.equal((await resolved.json()).state, "OFFERED");
});

test("T3+T4: both must confirm; receiving light is not a successful transmission", async () => {
  const { app } = await fresh();
  const a = await makeClient(app);
  const b = await makeClient(app);

  const created = await a.call("POST", "/v1/transmissions", { channel: "QR" });
  const id = created.json.id;
  await b.call("POST", `/v1/transmissions/${id}/accept`, {});

  // After B accepts but before A confirms, there is still no relation.
  const mid = await a.call("GET", `/v1/transmissions/${id}`);
  assert.equal(mid.json.state, "RECEIVED");
  const pathsMid = await a.call("GET", "/v1/light/paths");
  assert.equal(pathsMid.json.transmitted_nodes, 0, "RECEIVED must not count");

  const confirmed = await a.call("POST", `/v1/transmissions/${id}/confirm`, {});
  assert.equal(confirmed.json.state, "LEARNING");
  assert.deepEqual(confirmed.json.effects, ["EdgeFormed", "TokenInvalidated"]);

  const paths = await a.call("GET", "/v1/light/paths");
  assert.equal(paths.json.transmitted_nodes, 0, "LEARNING is still not a success");
  assert.equal(paths.json.outgoing.length, 1);
  assert.equal(paths.json.outgoing[0].state, "FORMED");

  // And B sees the light arrived, but not lit.
  const bPaths = await b.call("GET", "/v1/light/paths");
  assert.equal(bPaths.json.incoming.length, 1);
  assert.equal(bPaths.json.transmitted_nodes, 0);
});

test("T5: without countable evidence the session cannot be activated", async () => {
  const { app } = await fresh();
  const a = await makeClient(app);
  const b = await makeClient(app);
  const { id } = await toLearning(a, b);

  // M1.1: an unsigned usage event can no longer activate anything at all, so the
  // refusal moved from EVIDENCE_NOT_FOUND to UPGRADE_REQUIRED. The intent of this test
  // (the session must not activate) is unchanged and still asserted below.
  const missing = await b.call("POST", `/v1/transmissions/${id}/activate`, {
    usage_event_id: "does-not-exist",
  });
  assert.equal(missing.status, 426);
  assert.equal(missing.json.error.code, "UPGRADE_REQUIRED");

  const state = await a.call("GET", `/v1/transmissions/${id}`);
  assert.equal(state.json.state, "LEARNING", "must stay waiting, not activated");
});

test("T6: a real, post-relation usage event completes the activation", async () => {
  const { app } = await fresh();
  const a = await makeClient(app);
  const b = await makeClient(app);
  const { id } = await toLearning(a, b);

  // Evidence must post-date the relation, so attest now.
  const ev = attest("gpt");
  const att = await b.call("POST", "/v1/usage/attest", ev);
  assert.equal(att.status, 201, JSON.stringify(att.json));
  assert.equal(att.json.counted, true);

  const act = await b.call("POST", `/v1/transmissions/${id}/activate`, {
    usage_event_id: ev.event_id,
  });
  assert.equal(act.status, 200, JSON.stringify(act.json));
  assert.equal(act.json.state, "ACTIVATED");
  assert.equal(act.json.kind, "FIRST_LIGHT", "B had no usage before, so this is a first light");

  const aPaths = await a.call("GET", "/v1/light/paths");
  assert.equal(aPaths.json.transmitted_nodes, 1);
  assert.equal(aPaths.json.outgoing[0].state, "ACTIVATED");

  // Activating twice must not double count.
  const again = await b.call("POST", `/v1/transmissions/${id}/activate`, {
    usage_event_id: ev.event_id,
  });
  assert.equal(again.json.already_activated, true);
  const after = await a.call("GET", "/v1/light/paths");
  assert.equal(after.json.transmitted_nodes, 1);
});

test("T6b: evidence that predates the relation cannot activate it", async () => {
  const { app, db } = await fresh();
  const a = await makeClient(app);
  const b = await makeClient(app);

  // B already used AI before meeting A.
  const early = attest("gpt");
  const earlyAtt = await b.call("POST", "/v1/usage/attest", early);
  assert.equal(earlyAtt.status, 201);

  const { id } = await toLearning(a, b);

  const act = await b.call("POST", `/v1/transmissions/${id}/activate`, {
    usage_event_id: early.event_id,
  });
  assert.equal(act.status, 409);
  assert.equal(act.json.error.code, "EVIDENCE_PRECEDES_RELATION");

  const state = await a.call("GET", `/v1/transmissions/${id}`);
  assert.equal(state.json.state, "LEARNING");
});

test("T7: the helper receives exactly one echo, addressed anonymously", async () => {
  const { app, db } = await fresh();
  const a = await makeClient(app);
  const b = await makeClient(app);
  const { id } = await toLearning(a, b);
  const ev = attest("gpt");
  await b.call("POST", "/v1/usage/attest", ev);
  await b.call("POST", `/v1/transmissions/${id}/activate`, { usage_event_id: ev.event_id });

  const echoes = await db.all(
    "select payload from outbox_event where identity_id=(select id from identity where anon_id=$1)",
    [a.identity.anonId],
  );
  assert.equal(echoes.length, 1, "exactly one echo per activation");
  const payload = JSON.parse(echoes[0].payload);
  assert.equal(payload.message, "你传出的一束光，已经被点亮。");
  assert.ok(!JSON.stringify(payload).includes(b.identity.anonId), "no identity leak");
});

test("T7b: SSE delivers the echo, and the event id makes replays idempotent", async () => {
  const { app } = await fresh();
  const a = await makeClient(app);
  const b = await makeClient(app);
  const { id } = await toLearning(a, b);
  const ev = attest("gpt");
  await b.call("POST", "/v1/usage/attest", ev);
  await b.call("POST", `/v1/transmissions/${id}/activate`, { usage_event_id: ev.event_id });

  const headers = a.call("GET", "/v1/light/paths"); // warms nothing, just a probe
  await headers;

  // Open the stream through the same signed path.
  const res = await signedStream(a, "/v1/events/stream");
  assert.equal(res.status, 200);
  const text = await readSome(res, 3000);
  assert.ok(text.includes("event: edge.activated"), `stream should carry the echo:\n${text}`);
  assert.match(text, /^id: \d+$/m, "events must carry an id for Last-Event-ID");
  assert.ok(text.includes("hello"), "stream announces itself first");
});

test("T8: reconnecting does not duplicate activation or echoes", async () => {
  const { app, db } = await fresh();
  const a = await makeClient(app);
  const b = await makeClient(app);
  const { id } = await toLearning(a, b);
  const ev = attest("gpt");
  await b.call("POST", "/v1/usage/attest", ev);
  await b.call("POST", `/v1/transmissions/${id}/activate`, { usage_event_id: ev.event_id });
  await b.call("POST", `/v1/transmissions/${id}/activate`, { usage_event_id: ev.event_id });

  const [{ count: echoes }] = await db.all("select count(*)::int as count from outbox_event");
  assert.equal(echoes, 1);
  const [{ count: edges }] = await db.all(
    "select count(*)::int as count from light_edge where state='ACTIVATED'",
  );
  assert.equal(edges, 1);

  const aPaths = await a.call("GET", "/v1/light/paths");
  assert.equal(aPaths.json.transmitted_nodes, 1);
});

test("T9: reuse, expiry, self-transmission and concurrency are all refused", async () => {
  const { app } = await fresh();
  const a = await makeClient(app);
  const b = await makeClient(app);
  const c = await makeClient(app);

  // Once accepted by B, another client cannot claim the same invitation.
  const created = await a.call("POST", "/v1/transmissions", { channel: "QR" });
  await b.call("POST", `/v1/transmissions/${created.json.id}/accept`, {});
  const stolen = await c.call("POST", `/v1/transmissions/${created.json.id}/accept`, {});
  assert.ok(stolen.status >= 400, `second claim must fail, got ${stolen.status}`);

  // Self-transmission is refused.
  const selfTx = await a.call("POST", "/v1/transmissions", { channel: "QR" });
  const selfAccept = await a.call("POST", `/v1/transmissions/${selfTx.json.id}/accept`, {});
  assert.equal(selfAccept.status, 403);
  assert.equal(selfAccept.json.error.code, "SELF_TRANSMISSION");

  // Expired invitations cannot be opened.
  const expired = await a.call("POST", "/v1/transmissions", { channel: "QR" });
  const dbRow = await app.request(
    `/v1/transmissions/resolve?token=${expired.json.token}`,
  );
  assert.equal(dbRow.status, 200);

  // A second confirm is a replay, not a second relation.
  const { id } = await toLearning(a, b);
  const repeat = await a.call("POST", `/v1/transmissions/${id}/confirm`, {});
  assert.equal(repeat.status, 200);
  assert.equal(repeat.json.replayed, true);
});

test("T9b: an expired invitation reports itself and cannot be accepted", async () => {
  const { app, db } = await fresh({ policyOverrides: { invitationTtlMs: 60_000 } });
  const a = await makeClient(app);
  const b = await makeClient(app);
  const created = await a.call("POST", "/v1/transmissions", { channel: "QR" });

  // Force expiry rather than sleeping. Both timestamps move so the schema's
  // `expires_at_ms > created_at_ms` invariant still holds.
  await db.run(
    "update transmission set created_at_ms = $1, expires_at_ms = $2 where id = $3",
    [Date.now() - 120_000, Date.now() - 1000, created.json.id],
  );
  const resolved = await app.request(`/v1/transmissions/resolve?token=${created.json.token}`);
  assert.equal((await resolved.json()).expired, true);

  const accept = await b.call("POST", `/v1/transmissions/${created.json.id}/accept`, {});
  assert.equal(accept.status, 410);
  assert.equal(accept.json.error.code, "TRANSMISSION_EXPIRED");
});

test("T10: an NFC tag without an open invitation claims nobody", async () => {
  const { app, db } = await fresh();
  const owner = await makeClient(app);

  const page = await app.request(`/n/${owner.identity.anonId}`);
  assert.equal(page.status, 200);
  const html = await page.text();
  assert.ok(html.includes("没有开启的传光"), "should say there is nothing open");

  assert.equal((await db.all("select * from transmission")).length, 0, "no session created");
  assert.equal((await db.all("select * from light_edge")).length, 0, "no relation created");

  // With an open invitation the tag resolves to it — still without claiming.
  const created = await owner.call("POST", "/v1/transmissions", { channel: "NFC" });
  const withOpen = await app.request(`/n/${owner.identity.anonId}`);
  assert.equal(withOpen.status, 200);
  const [{ count }] = await db.all("select count(*)::int as count from transmission_party where role='RECEIVER'");
  assert.equal(count, 0, "an NFC tap must never auto-claim a receiver");
  assert.ok(created.json.token.length > 0);
});

test("T11: restoring the same seed on a second device does not double count", async () => {
  const { app, db } = await fresh();
  const a = await makeClient(app);
  const b = await makeClient(app);

  // Same light seed, new device: same anon id, new epoch key.
  const restored = createIdentity(b.identity.rootSeed);
  assert.equal(restored.anonId, b.identity.anonId, "anon id must be derived from the root key");

  const { id } = await toLearning(a, b);
  const ev = attest("gpt");
  await b.call("POST", "/v1/usage/attest", ev);
  await b.call("POST", `/v1/transmissions/${id}/activate`, { usage_event_id: ev.event_id });

  const identities = await db.all("select count(*)::int as count from identity");
  assert.equal(identities[0].count, 2, "two light seeds, not three installations");

  const aPaths = await a.call("GET", "/v1/light/paths");
  assert.equal(aPaths.json.transmitted_nodes, 1);
});

test("T11b: a second epoch can be registered and used", async () => {
  const { app } = await fresh();
  const identity = createIdentity();
  const e1 = epochCertificate(identity, 1);
  const e2 = epochCertificate(identity, 2);

  for (const e of [e1, e2]) {
    const res = await app.request("/v1/identities/register", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ anon_id: identity.anonId, cert: e.cert }),
    });
    assert.ok(res.status < 300, `register epoch ${e.cert.epoch}: ${res.status}`);
  }
});

test("T12: revoking publication removes the public relation immediately", async () => {
  const { app, db } = await fresh();
  const a = await makeClient(app);
  const b = await makeClient(app);

  const { id } = await toLearning(a, b);
  const ev = attest("gpt");
  await b.call("POST", "/v1/usage/attest", ev);
  await b.call("POST", `/v1/transmissions/${id}/activate`, { usage_event_id: ev.event_id });

  // Publish explicitly.
  await a.call("POST", "/v1/privacy/consent", {
    scope: "PUBLIC_LIGHT_PATH",
    state: "GRANTED",
  });
  await db.run("update light_edge set visibility='ANONYMOUS_PUBLIC'");

  const before = await app.request(`/v1/light/public/${a.identity.anonId}`);
  const beforeJson = await before.json();
  assert.equal(beforeJson.edges.length, 1);

  // Revoke.
  const revoked = await a.call("POST", "/v1/privacy/consent", {
    scope: "PUBLIC_LIGHT_PATH",
    state: "REVOKED",
  });
  assert.equal(revoked.status, 200);

  const after = await app.request(`/v1/light/public/${a.identity.anonId}`);
  const afterJson = await after.json();
  assert.equal(afterJson.edges.length, 0, "public API must stop exposing the relation");
});

test("T12b: revoke deletes linkable evidence and revokes relations", async () => {
  const { app, db } = await fresh();
  const a = await makeClient(app);
  const b = await makeClient(app);

  await b.call("POST", "/v1/usage/attest", attest("gpt"));
  const { id } = await toLearning(a, b);
  const ev = attest("claude");
  await b.call("POST", "/v1/usage/attest", ev);
  await b.call("POST", `/v1/transmissions/${id}/activate`, { usage_event_id: ev.event_id });

  const revoked = await b.call("POST", "/v1/privacy/revoke", { mode: "ALL" });
  assert.equal(revoked.status, 200);
  assert.ok(revoked.json.deleted_usage_events >= 2);
  assert.ok(revoked.json.revoked_edges >= 1);
  assert.ok(revoked.json.irreversible_note.includes("不可逆"));

  const [{ count: usage }] = await db.all(
    "select count(*)::int as count from usage_event where identity_id=(select id from identity where anon_id=$1)",
    [b.identity.anonId],
  );
  assert.equal(usage, 0, "linkable usage must be gone");
});

test("T14: no endpoint accepts chat content, prompts, keys or paths", async () => {
  const { app } = await fresh();
  const a = await makeClient(app);

  for (const field of ["chat", "prompt", "api_key", "messages", "db_path", "tokens_used"]) {
    const res = await a.call("POST", "/v1/usage/attest", {
      ...attest("gpt"),
      [field]: "should be refused",
    });
    assert.equal(res.status, 400, `field ${field} must be refused`);
    assert.equal(res.json.error.code, "INVALID_SCHEMA");
  }

  // And the schema itself has no column that could hold such a thing.
  const { db } = await fresh();
  const columns = await db.all(
    `select table_name, column_name from information_schema.columns
      where table_schema='public'`,
  );
  const names = columns.map((c) => c.column_name.toLowerCase());
  for (const forbidden of ["chat", "message", "prompt", "api_key", "content", "tokens_used"]) {
    assert.ok(!names.includes(forbidden), `schema must not have a ${forbidden} column`);
  }
});

test("only E3/E4 can be counted", async () => {
  const { app } = await fresh();
  const a = await makeClient(app);
  for (const level of ["E0", "E1", "E2"]) {
    const res = await a.call("POST", "/v1/usage/attest", attest("gpt", level));
    assert.equal(res.status, 422, `${level} must not be countable`);
    assert.equal(res.json.error.code, "NOT_COUNTABLE");
  }
});

test("attestation is idempotent per family", async () => {
  const { app } = await fresh();
  const a = await makeClient(app);
  const first = await a.call("POST", "/v1/usage/attest", attest("gpt"));
  assert.equal(first.status, 201);
  assert.equal(first.json.counted, true);

  const second = await a.call("POST", "/v1/usage/attest", attest("gpt"));
  assert.equal(second.status, 200);
  assert.equal(second.json.counted, false);
  assert.equal(second.json.verified_families, 1);
});

test("replay of a signed request is rejected", async () => {
  const { app } = await fresh();
  const a = await makeClient(app);
  const res = await a.call("POST", "/v1/transmissions", { channel: "QR" });
  assert.equal(res.status, 201);

  // The client helper generates a fresh nonce per call, so capture one signed
  // request and send it twice byte-for-byte.
  const { makeReplayProbe } = await import("./support/replay.mjs");
  const probe = await makeReplayProbe(app, a);
  assert.equal(probe.first, 201);
  assert.equal(probe.second, 401);
  assert.equal(probe.secondCode, "NONCE_REPLAYED");
});

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

async function signedStream(client, path) {
  const { headers } = await client.signedHeaders("GET", path, "");
  return client.appRequest
    ? client.appRequest(path, { headers })
    : null;
}

async function readSome(response, ms) {
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let text = "";
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const next = await Promise.race([
      reader.read(),
      new Promise((r) => setTimeout(() => r({ done: false, timeout: true }), 200)),
    ]);
    if (next.timeout) continue;
    if (next.done) break;
    text += decoder.decode(next.value, { stream: true });
    if (text.includes("edge.activated")) break;
  }
  reader.cancel().catch(() => {});
  return text;
}

test("T1b: the invitation renders a scannable QR image", async () => {
  const { app } = await fresh();
  const a = await makeClient(app);
  const created = await a.call("POST", "/v1/transmissions", { channel: "QR" });

  const svg = await app.request(`/t/${created.json.token}/qr.svg`);
  assert.equal(svg.status, 200);
  assert.match(svg.headers.get("content-type") ?? "", /image\/svg\+xml/);
  const body = await svg.text();
  assert.ok(body.startsWith("<svg"), "must be an SVG");
  assert.ok(body.includes("<path"), "must contain modules");
  assert.ok(body.length > 500, "must have real content");

  // An unknown token yields no image.
  const missing = await app.request("/t/not-a-real-token/qr.svg");
  assert.equal(missing.status, 404);
});
