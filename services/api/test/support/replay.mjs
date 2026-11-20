// Replay probe: send the exact same signed request twice.
//
// The server must accept the first and reject the second as NONCE_REPLAYED.
// This is the anti-replay evidence required by F3 §7.

import { createHash, createPrivateKey, createPublicKey, randomBytes, sign } from "node:crypto";

const PKCS8_PREFIX = Buffer.from("302e020100300506032b657004220420", "hex");
const SPKI_PREFIX = Buffer.from("302a300506032b6570032100", "hex");
const sha256 = (b) => createHash("sha256").update(b).digest();
const hex = (b) => Buffer.from(b).toString("hex");

export async function makeReplayProbe(app, client) {
  // Rebuild the signing key from the identity's epoch certificate is not needed:
  // we reuse the client's call once, capture what it sent, then resend verbatim.
  const body = JSON.stringify({ channel: "QR" });
  const path = "/v1/transmissions";

  // Ask the client to sign, but do not send yet, by intercepting its headers.
  const captured = await client.signedHeaders("POST", path, body);
  const headers = { ...captured.headers, "content-type": "application/json", "idempotency-key": `replay-${randomBytes(6).toString("hex")}` };

  const first = await app.request(path, { method: "POST", headers, body });
  const second = await app.request(path, { method: "POST", headers, body });

  const secondJson = await second.json().catch(() => ({}));
  return {
    first: first.status,
    second: second.status,
    secondCode: secondJson?.error?.code,
  };
}
