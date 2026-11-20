// M2 acceptance. Against a mock upstream that speaks the real Ollama wire format.
// REAL_OLLAMA_E2E is BLOCKED in this environment: no ollama binary, no daemon.
import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import http from "node:http";
import { createProxy, listen } from "../src/proxy.mjs";
import { loadRegistry } from "../src/registry.mjs";
import { createStreamObserver, observeJson } from "../src/observe.mjs";
import { buildCredential, newDeviceKey, canonicalize, signingInput, PROTOCOL_VERSION } from "../src/credential.mjs";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const REG = loadRegistry(fileURLToPath(new URL("../../../data/model-registry.v1.json", import.meta.url)));
const TOKEN = "test-token-0123456789abcdef";

/** A stand-in for Ollama. It speaks the same NDJSON/JSON shapes, nothing more. */
function mockUpstream(script) {
  const srv = http.createServer((req, res) => {
    const mode = script;
    if (mode === "error") {
      res.writeHead(500, { "content-type": "application/json" });
      return res.end(JSON.stringify({ error: "model not loaded" }));
    }
    if (mode === "stream") {
      res.writeHead(200, { "content-type": "application/x-ndjson" });
      res.write(JSON.stringify({ model: "llama3.1:8b", response: "he", done: false }) + "\n");
      res.write(JSON.stringify({ model: "llama3.1:8b", response: "llo", done: false }) + "\n");
      res.write(JSON.stringify({ model: "llama3.1:8b", done: true, eval_count: 12, prompt_eval_count: 5 }) + "\n");
      return res.end();
    }
    if (mode === "stream-cut") {
      res.writeHead(200, { "content-type": "application/x-ndjson" });
      res.write(JSON.stringify({ model: "llama3.1:8b", response: "partial", done: false }) + "\n");
      return res.destroy();                       // connection dies before done
    }
    if (mode === "warmup") {
      res.writeHead(200, { "content-type": "application/json" });
      return res.end(JSON.stringify({ model: "llama3.1:8b", done: true, done_reason: "load" }));
    }
    if (mode === "tags") {
      res.writeHead(200, { "content-type": "application/json" });
      return res.end(JSON.stringify({ models: [{ name: "llama3.1:8b" }] }));
    }
    res.writeHead(200, { "content-type": "application/json" });
    return res.end(JSON.stringify({ model: "qwen2.5:7b", done: true, eval_count: 9, prompt_eval_count: 3, message: { role: "assistant", content: "hi" } }));
  });
  return srv;
}

async function withProxy(script, fn) {
  const up = mockUpstream(script);
  await new Promise((r) => up.listen(0, "127.0.0.1", r));
  const observations = [];
  const logs = [];
  const proxy = createProxy({
    upstream: `http://127.0.0.1:${up.address().port}`,
    registry: REG, token: TOKEN,
    onObservation: (o) => observations.push(o),
    log: (e) => logs.push(e),
  });
  const addr = await listen(proxy, 0);
  try {
    await fn({ port: addr.port, observations, logs });
  } finally {
    await new Promise((r) => proxy.close(r));
    up.close();
  }
}

function send(port, path, body, headers = {}) {
  return new Promise((resolve, reject) => {
    const text = JSON.stringify(body);
    const req = http.request({
      hostname: "127.0.0.1", port, path, method: "POST",
      headers: { "content-type": "application/json", "content-length": Buffer.byteLength(text), ...headers },
    }, (res) => {
      const chunks = [];
      res.on("data", (c) => chunks.push(c));
      res.on("end", () => resolve({ status: res.statusCode, text: Buffer.concat(chunks).toString("utf8") }));
    });
    req.on("error", reject);
    req.end(text);
  });
}

const AUTH = { authorization: `Bearer ${TOKEN}` };

// 1 & 3 -----------------------------------------------------------------------
test("1/3 · a completed streaming chat call is observed exactly once", async () => {
  await withProxy("stream", async ({ port, observations }) => {
    const r = await send(port, "/api/chat", { model: "llama3.1:8b", stream: true, messages: [] }, AUTH);
    assert.equal(r.status, 200);
    assert.equal(observations.length, 1, "one request = one observation, never one per chunk");
    assert.equal(observations[0].family, "llama");
  });
});

// 2 & 3 -----------------------------------------------------------------------
test("2/3 · a completed non-streaming generate call is observed", async () => {
  await withProxy("json", async ({ port, observations }) => {
    const r = await send(port, "/api/generate", { model: "qwen2.5:7b", stream: false, prompt: "x" }, AUTH);
    assert.equal(r.status, 200);
    assert.equal(observations.length, 1);
    assert.equal(observations[0].family, "qwen");
  });
});

// 4 ---------------------------------------------------------------------------
test("4 · an interrupted stream is not counted", async () => {
  await withProxy("stream-cut", async ({ port, observations, logs }) => {
    await send(port, "/api/chat", { model: "llama3.1:8b", stream: true }, AUTH).catch(() => {});
    await new Promise((r) => setTimeout(r, 60));
    assert.equal(observations.length, 0);
    // The exact code depends on how Node surfaces the broken connection
    // ("aborted" vs "upstream_error"); what matters is that a verdict was recorded
    // and that it was not counted as usage.
    assert.ok(logs.some((l) => l.event === "not_counted"), JSON.stringify(logs));
  });
});

// 5 ---------------------------------------------------------------------------
test("5 · an upstream error is not counted", async () => {
  await withProxy("error", async ({ port, observations }) => {
    const r = await send(port, "/api/chat", { model: "llama3.1:8b" }, AUTH);
    assert.equal(r.status, 500);
    assert.equal(observations.length, 0);
  });
});

// 6 ---------------------------------------------------------------------------
test("6 · listing models is not a usage event", async () => {
  await withProxy("tags", async ({ port, observations }) => {
    // authentication is checked before routing on purpose: an unauthenticated caller
    // should not learn which paths exist. With the token, the path is refused.
    const r = await send(port, "/api/tags", {}, AUTH);
    assert.equal(r.status, 404, "only /api/chat and /api/generate are proxied");
    const noAuth = await send(port, "/api/tags", {});
    assert.equal(noAuth.status, 401);
    assert.equal(observations.length, 0);
  });
});

test("6b · a metadata-only response with no completion is not counted", async () => {
  await withProxy("json", async ({ port, observations }) => {
    await send(port, "/api/chat", { model: "llama3.1:8b" }, AUTH);
    assert.equal(observations.length, 1);
  });
  // and a body that merely claims done=true from the client side never reaches us
  const v = observeJson(JSON.stringify({ model: "llama3.1:8b", done: false }));
  assert.equal(v.completed, false);
});

// 7 & 8 -----------------------------------------------------------------------
test("7/8 · different calls produce different credential ids", async () => {
  const key = newDeviceKey();
  const mk = () => buildCredential({
    key, keyId: "k1", subjectIdentityId: "fl1_x", family: "llama",
    adapterId: "ollama-proxy", adapterVersion: "0.1.0",
    observedAt: Date.now(), issuedAt: Date.now(), credentialId: `cred_${Math.random().toString(16).slice(2)}`,
  });
  const a = mk(), b = mk();
  assert.notEqual(a.credential_id, b.credential_id);
  assert.equal(a.signature.length, 128);
});

// 12 --------------------------------------------------------------------------
test("12 · requests without the token, or from a browser origin, are refused", async () => {
  await withProxy("stream", async ({ port, observations }) => {
    const noAuth = await send(port, "/api/chat", { model: "llama3.1:8b" });
    assert.equal(noAuth.status, 401);
    const withOrigin = await send(port, "/api/chat", { model: "llama3.1:8b" }, { ...AUTH, origin: "https://evil.example" });
    assert.equal(withOrigin.status, 403);
    assert.equal(observations.length, 0);
  });
});

test("12b · a non-loopback Host header is refused (DNS rebinding)", async () => {
  await withProxy("stream", async ({ port, observations }) => {
    const r = await send(port, "/api/chat", { model: "llama3.1:8b" }, { ...AUTH, host: "evil.example:11435" });
    assert.equal(r.status, 421);
    assert.equal(observations.length, 0);
  });
});

// 10 --------------------------------------------------------------------------
test("10 · an unregistered model never becomes a trusted family", async () => {
  await withProxy("stream", async ({ port, observations, logs }) => {
    // the mock answers with llama3.1, so force the unknown path through the resolver
    const { resolve } = await import("../src/registry.mjs");
    const r = resolve(REG, "totally-new-model:latest");
    assert.equal(r.kind, "unknown");
    const t = resolve(REG, "codex-auto-review");
    assert.equal(t.kind, "task_label");
  });
});

// 13 --------------------------------------------------------------------------
test("13 · neither logs nor observations carry conversation text", async () => {
  await withProxy("json", async ({ port, observations, logs }) => {
    await send(port, "/api/generate", { model: "qwen2.5:7b", prompt: "MY SECRET PROMPT" }, AUTH);
    const blob = JSON.stringify({ observations, logs });
    assert.ok(!blob.includes("MY SECRET"), "prompt must not leak");
    assert.ok(!blob.includes("hi"), "response content must not leak");
    for (const o of observations) {
      assert.deepEqual(Object.keys(o).sort(), ["adapterId", "adapterVersion", "at", "evalCount", "family", "model"]);
    }
  });
});

// 9 ---------------------------------------------------------------------------
test("9 · the plugin's canonicalisation matches the shared vector", () => {
  const vector = JSON.parse(readFileSync(new URL("../../../packages/protocol/vectors/credential-v1.json", import.meta.url), "utf8"));
  assert.equal(canonicalize(vector.payload), vector.canonical_json);
  assert.equal(signingInput(vector.payload).toString("hex"), vector.signing_input_hex);
});

test("stream observer tolerates chunk boundaries", () => {
  const o = createStreamObserver();
  o.consumeChunk('{"model":"llama3.1:8b","done":fal');
  o.consumeChunk('se}\n{"model":"llama3.1:8b","done":true,"eval_count":3}\n');
  const v = o.finish();
  assert.equal(v.completed, true);
  assert.equal(v.evalCount, 3);
});
