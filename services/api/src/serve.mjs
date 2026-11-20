// HTTP entry point.
//
// Reads configuration from the environment, opens the database, and serves the
// API. No domain, no TLS, no public deployment: this is for local runs and for
// the automated suite. See docs/reports/f3-deployment-prep.md.

import { readFile } from "node:fs/promises";
import { createServer } from "node:http";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { openDb } from "./db.mjs";
import { createApp } from "./app.mjs";
import { loadPolicy } from "./policy.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const PORT = Number(process.env.FL_PORT ?? 8787);
const HOST = process.env.FL_HOST ?? "127.0.0.1";

const brand = JSON.parse(
  await readFile(join(HERE, "..", "..", "..", "packages", "brand", "brand.json"), "utf8"),
);

const db = await openDb({ dataDir: process.env.FL_DB_DIR ?? null });
const app = createApp({ db, policy: loadPolicy(), brand });

function toRequest(req, body) {
  const url = `http://${req.headers.host ?? `${HOST}:${PORT}`}${req.url}`;
  const headers = new Headers();
  for (const [k, v] of Object.entries(req.headers)) {
    if (Array.isArray(v)) v.forEach((x) => headers.append(k, x));
    else if (v !== undefined) headers.set(k, String(v));
  }
  return new Request(url, {
    method: req.method,
    headers,
    body: ["GET", "HEAD"].includes(req.method) ? undefined : body,
  });
}

const server = createServer(async (req, res) => {
  const chunks = [];
  for await (const chunk of req) chunks.push(chunk);
  const body = Buffer.concat(chunks);

  const response = await app.fetch(toRequest(req, body));
  res.statusCode = response.status;
  for (const [k, v] of response.headers) res.setHeader(k, v);
  if (response.body) {
    const reader = response.body.getReader();
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      res.write(Buffer.from(value));
    }
  }
  res.end();
});

server.listen(PORT, HOST, () => {
  console.log(`firstlight api listening on http://${HOST}:${PORT}`);
  console.log(`public base url: ${process.env.FL_PUBLIC_BASE_URL ?? `http://${HOST}:${PORT}`}`);
});
