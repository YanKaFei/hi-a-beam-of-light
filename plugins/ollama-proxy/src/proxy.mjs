// Local loopback proxy for Ollama.
//
// Security model, stated plainly:
//   * it binds 127.0.0.1 only, and rejects any request whose Host is not loopback
//     (DNS-rebinding guard)
//   * it requires a bearer token; a web page cannot set that header without a preflight,
//     and requests carrying an Origin header are refused outright
//   * CORS is NOT the boundary here — the token and the Origin refusal are
//   * body bytes are forwarded and parsed, never persisted

import http from "node:http";
import { createStreamObserver, observeJson } from "./observe.mjs";
import { resolve } from "./registry.mjs";

const OBSERVED_PATHS = new Set(["/api/chat", "/api/generate"]);
const LOOPBACK_HOSTS = new Set(["127.0.0.1", "localhost", "[::1]"]);

export function createProxy({
  upstream = "http://127.0.0.1:11434",
  registry, token, adapterId = "ollama-proxy", adapterVersion = "0.1.0",
  onObservation, log = () => {}, maxBytes = 8 * 1024 * 1024,
}) {
  if (!token || token.length < 16) throw new Error("a token of at least 16 chars is required");

  const server = http.createServer(async (req, res) => {
    const host = (req.headers.host || "").split(":")[0];
    if (!LOOPBACK_HOSTS.has(host)) {
      res.writeHead(421, { "content-type": "application/json" });
      return res.end(JSON.stringify({ error: "loopback_only" }));
    }
    if (req.headers.origin) {
      res.writeHead(403, { "content-type": "application/json" });
      return res.end(JSON.stringify({ error: "browser_origin_refused" }));
    }
    if (req.headers.authorization !== `Bearer ${token}`) {
      res.writeHead(401, { "content-type": "application/json" });
      return res.end(JSON.stringify({ error: "unauthorized" }));
    }
    if (req.method !== "POST" || !OBSERVED_PATHS.has(new URL(req.url, "http://x").pathname)) {
      res.writeHead(404, { "content-type": "application/json" });
      return res.end(JSON.stringify({ error: "unsupported_path" }));
    }

    // collect the request body (needed to forward it); never stored
    const chunks = [];
    let size = 0;
    for await (const c of req) {
      size += c.length;
      if (size > maxBytes) {
        res.writeHead(413, { "content-type": "application/json" });
        return res.end(JSON.stringify({ error: "request_too_large" }));
      }
      chunks.push(c);
    }
    const body = Buffer.concat(chunks);
    let wantsStream = false;
    try { wantsStream = JSON.parse(body.toString("utf8"))?.stream === true; } catch { /* forward as-is */ }

    const target = new URL(req.url, upstream);
    const upstreamRes = await new Promise((resolveP) => {
      const r = http.request({
        hostname: target.hostname, port: target.port, path: target.pathname + target.search,
        method: "POST", headers: { "content-type": "application/json", "content-length": body.length },
      }, resolveP);
      r.on("error", () => resolveP(null));
      r.end(body);
    });
    if (!upstreamRes) {
      // The upstream never produced a response. That is a failed call, and it must be
      // recorded as such rather than disappearing.
      log({ event: "not_counted", reason: "upstream_unreachable" });
      res.writeHead(502, { "content-type": "application/json" });
      return res.end(JSON.stringify({ error: "upstream_unreachable" }));
    }

    res.writeHead(upstreamRes.statusCode ?? 502, {
      "content-type": upstreamRes.headers["content-type"] ?? "application/json",
    });

    const observer = createStreamObserver();
    const collected = [];
    let aborted = false;
    let settled = false;
    const settle = (reason) => {
      if (settled) return;
      settled = true;
      log({ event: "not_counted", reason });
    };
    req.on("aborted", () => { aborted = true; });
    // Last line of defence: whenever this response closes without a verdict, record one.
    res.on("close", () => { if (!settled) { aborted = true; settle("connection_closed"); } });

    upstreamRes.on("data", (chunk) => {
      res.write(chunk);
      if (wantsStream) observer.consumeChunk(chunk.toString("utf8"));
      else if (collected.length < 512) collected.push(chunk);
    });

    // Exactly one verdict per request, however the upstream ends. A dropped connection
    // must still be recorded, otherwise the call vanishes with no explanation.
    upstreamRes.on("aborted", () => { if (!res.writableEnded) res.end(); settle("aborted"); });
    upstreamRes.on("error", () => { if (!res.writableEnded) res.end(); settle("upstream_error"); });
    upstreamRes.on("close", () => { if (!upstreamRes.complete) settle("aborted"); });

    upstreamRes.on("end", () => {
      res.end();
      settled = true;
      if ((upstreamRes.statusCode ?? 500) >= 400) {
        log({ event: "not_counted", reason: "upstream_status", status: upstreamRes.statusCode });
        return;
      }
      const verdict = wantsStream ? observer.finish({ aborted }) : observeJson(Buffer.concat(collected).toString("utf8"));
      if (!verdict.completed) {
        log({ event: "not_counted", reason: verdict.reason });
        return;
      }
      const mapped = resolve(registry, verdict.model);
      if (mapped.kind !== "family") {
        log({ event: "not_counted", reason: `family_${mapped.kind}` });
        return;
      }
      // One observation per request, never per streamed chunk.
      onObservation?.({
        model: verdict.model, family: mapped.family, adapterId, adapterVersion,
        evalCount: verdict.evalCount ?? null, at: Date.now(),
      });
      log({ event: "observed", family: mapped.family });
    });
  });

  return server;
}

export function listen(server, port = 11435) {
  return new Promise((resolveP, reject) => {
    server.once("error", reject);
    server.listen(port, "127.0.0.1", () => resolveP(server.address()));
  });
}
