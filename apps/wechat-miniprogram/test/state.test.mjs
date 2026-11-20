// The trust ladder and the API client, tested in node. No WeChat runtime is needed for
// the logic; the pages themselves are PENDING real 开发者工具 screenshots.
import { test } from "node:test";
import assert from "node:assert/strict";
import { levelOf, describe, LEVELS } from "../utils/state.js";
import { makeApi } from "../utils/api.js";
import { isConfigured } from "../utils/auth.js";

test("1 · no device means unpaired, with a next step", () => {
  const lv = levelOf({ device: null });
  assert.equal(lv, LEVELS.UNPAIRED);
  assert.match(describe(lv).next, /配对/);
});

test("2 · paired but no observation is its own state", () => {
  assert.equal(levelOf({ device: { paired: true } }), LEVELS.PAIRED_IDLE);
});

test("3 · a valid signature without source approval never reads as eligible", () => {
  const lv = levelOf({
    device: { paired: true },
    credential: { signature_status: "VALID", source_class: "PLUGIN_OBSERVED", eligible: { activate: false } },
  });
  assert.equal(lv, LEVELS.SIGNATURE_VALID);
  assert.notEqual(lv, LEVELS.ACTIVATION_ELIGIBLE);
});

test("4 · only the server can report ACTIVATED", () => {
  assert.equal(levelOf({ device: { paired: true }, transmission: { state: "ACTIVATED" } }), LEVELS.ACTIVATED);
  // a local clock or animation cannot produce it
  assert.notEqual(levelOf({ device: { paired: true } }), LEVELS.ACTIVATED);
});

test("5 · no label collapses into a boolean verified", () => {
  for (const k of Object.keys(LEVELS)) {
    assert.notEqual(k, "VERIFIED");
    assert.ok(describe(LEVELS[k]).title.length > 0);
  }
});

test("6 · the API client sends the session token and never a secret", async () => {
  const calls = [];
  const api = makeApi({
    base: "https://example.invalid",
    session: { token: "sess_123" },
    fetchImpl: async (url, init) => {
      calls.push({ url, headers: init.headers, body: init.body });
      return { status: 200, json: async () => ({ ok: true }) };
    },
  });
  await api.getDevice("dev-1");
  assert.equal(calls[0].url, "https://example.invalid/v1/devices/dev-1");
  assert.equal(calls[0].headers.authorization, "Bearer sess_123");
  assert.ok(!JSON.stringify(calls[0]).includes("AppSecret"));
});

test("7 · dev adapter is only used when the appid is a placeholder", () => {
  assert.equal(isConfigured("touristappid"), false);
  assert.equal(isConfigured("wx1234567890abcdef"), true);
  assert.equal(isConfigured(undefined), false);
});
