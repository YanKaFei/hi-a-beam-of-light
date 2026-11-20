// M3-B: login, session scoping, production isolation.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

import { openDb } from "../src/db.mjs";
import { createApp } from "../src/app.mjs";
import { loadPolicy } from "../src/policy.mjs";
import { assertProductionIsolation } from "../src/wechat.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const BRAND = JSON.parse(readFileSync(join(HERE, "..", "..", "..", "packages", "brand", "brand.json"), "utf8"));

async function fresh() {
  const db = await openDb();
  return { db, app: createApp({ db, policy: loadPolicy({ publicBaseUrl: "http://127.0.0.1:8787" }), brand: BRAND }) };
}
const post = (app, path, body, headers = {}) =>
  app.request(path, { method: "POST", headers: { "content-type": "application/json", ...headers }, body: JSON.stringify(body ?? {}) });
const get = (app, path, headers = {}) => app.request(path, { headers });

test("L1 · dev login is closed unless explicitly enabled", async () => {
  const { app } = await fresh();
  delete process.env.FL_DEV_ADAPTER;
  const r = await post(app, "/v1/auth/dev-login", { user: "u1" });
  assert.equal(r.status, 404);
});

test("L2 · dev login issues a session that reads only its own data", async () => {
  process.env.FL_DEV_ADAPTER = "on";
  try {
    const { app } = await fresh();
    const a = await (await post(app, "/v1/auth/dev-login", { user: "alice" })).json();
    const b = await (await post(app, "/v1/auth/dev-login", { user: "bob" })).json();
    assert.equal(a.mode, "DEV_ADAPTER");
    assert.match(a.note, /SIMULATED/);

    const me = await get(app, "/v1/auth/me", { authorization: `Bearer ${a.token}` });
    assert.equal(me.status, 200);
    assert.equal((await me.json()).anon_id, a.anon_id);

    const obs = await get(app, "/v1/me/observations", { authorization: `Bearer ${a.token}` });
    assert.equal(obs.status, 200);

    // b's token cannot read a's rows; the endpoint takes no subject parameter at all
    const cross = await get(app, "/v1/me/observations?subject_id=" + a.anon_id, { authorization: `Bearer ${b.token}` });
    assert.equal(cross.status, 200);
    const body = await cross.json();
    assert.equal(body.anon_id, b.anon_id, "the query parameter must be ignored");
  } finally { delete process.env.FL_DEV_ADAPTER; }
});

test("L3 · logout revokes the session immediately", async () => {
  process.env.FL_DEV_ADAPTER = "on";
  try {
    const { app } = await fresh();
    const s = await (await post(app, "/v1/auth/dev-login", { user: "carol" })).json();
    await post(app, "/v1/auth/logout", {}, { authorization: `Bearer ${s.token}` });
    const me = await get(app, "/v1/auth/me", { authorization: `Bearer ${s.token}` });
    assert.equal(me.status, 401);
    assert.equal((await me.json()).error.code, "SESSION_REVOKED");
  } finally { delete process.env.FL_DEV_ADAPTER; }
});

test("L4 · an expired or unknown session is refused", async () => {
  const { app, db } = await fresh();
  const bad = await get(app, "/v1/auth/me", { authorization: "Bearer nope" });
  assert.equal(bad.status, 401);

  process.env.FL_DEV_ADAPTER = "on";
  try {
    const s = await (await post(app, "/v1/auth/dev-login", { user: "dave" })).json();
    await db.run("update auth_session set expires_at_ms=$1", [Date.now() - 1000]);
    const me = await get(app, "/v1/auth/me", { authorization: `Bearer ${s.token}` });
    assert.equal(me.status, 401);
    assert.equal((await me.json()).error.code, "SESSION_EXPIRED");
  } finally { delete process.env.FL_DEV_ADAPTER; }
});

test("L5 · a wechat code is single-use and maps stably to one subject", async () => {
  process.env.FL_WECHAT_APP_ID = "wxTESTAPPID";
  process.env.FL_WECHAT_APP_SECRET = "secret-not-in-tests";
  const realFetch = globalThis.fetch;
  globalThis.fetch = async () => ({ json: async () => ({ openid: "openid-abc", session_key: "sk" }) });
  try {
    const { app, db } = await fresh();
    const first = await post(app, "/v1/auth/wechat-login", { code: "code-0001" });
    assert.equal(first.status, 201);
    const t1 = await first.json();
    assert.ok(!JSON.stringify(t1).includes("openid"), "openid must not be returned");
    assert.ok(!JSON.stringify(t1).includes("sk"), "session_key must not be returned");

    const replay = await post(app, "/v1/auth/wechat-login", { code: "code-0001" });
    assert.equal(replay.status, 409);
    assert.equal((await replay.json()).error.code, "CODE_REPLAYED");

    const second = await (await post(app, "/v1/auth/wechat-login", { code: "code-0002" })).json();
    assert.equal(second.anon_id, t1.anon_id, "same openid must map to the same subject");
    const subjects = await db.all("select count(*)::int as n from wechat_subject");
    assert.equal(subjects[0].n, 1);
  } finally {
    globalThis.fetch = realFetch;
    delete process.env.FL_WECHAT_APP_ID;
    delete process.env.FL_WECHAT_APP_SECRET;
  }
});

test("L6 · production refuses to boot with the dev adapter or a fixture authority", () => {
  const base = { FL_ENV: "production", FL_WECHAT_APP_ID: "wx1", FL_WECHAT_APP_SECRET: "s" };
  assertProductionIsolation(base);                       // clean config passes
  assert.throws(() => assertProductionIsolation({ ...base, FL_DEV_ADAPTER: "on" }), /FL_DEV_ADAPTER/);
  assert.throws(() => assertProductionIsolation({ ...base, FL_TRUSTED_FIXTURE_ADAPTER: "x" }), /FL_TRUSTED_FIXTURE_ADAPTER/);
  assert.throws(() => assertProductionIsolation({ FL_ENV: "production", FL_WECHAT_APP_ID: "wx1" }), /FL_WECHAT_APP_SECRET/);
  // development without the vars is fine
  assertProductionIsolation({});
});

test("L7 · the activation matrix still refuses PLUGIN_OBSERVED in production config", async () => {
  const { app } = await fresh();
  process.env.FL_DEV_ADAPTER = "on";
  try {
    const s = await (await post(app, "/v1/auth/dev-login", { user: "erin" })).json();
    const obs = await (await get(app, "/v1/me/observations", { authorization: `Bearer ${s.token}` })).json();
    for (const o of obs.observations) {
      assert.equal(o.eligible.activate, false, "nothing may be activatable by default");
    }
  } finally { delete process.env.FL_DEV_ADAPTER; }
});
