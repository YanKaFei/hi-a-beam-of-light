// Test client: a faithful stand-in for the desktop/mobile client.
//
// It implements the exact key derivation and request signing the Rust core does,
// so the server is tested against the real protocol rather than a shortcut.
// A Rust-produced signature is additionally verified in `rust-interop.test.mjs`.

import {
  createHash,
  createPrivateKey,
  createPublicKey,
  generateKeyPairSync,
  randomBytes,
  sign as cryptoSign,
} from "node:crypto";

const SPKI_PREFIX = Buffer.from("302a300506032b6570032100", "hex");
const PKCS8_PREFIX = Buffer.from("302e020100300506032b657004220420", "hex");

export function sha256(buf) {
  return createHash("sha256").update(buf).digest();
}
export function hex(buf) {
  return Buffer.from(buf).toString("hex");
}

function keyFromSeed(seed) {
  return createPrivateKey({
    key: Buffer.concat([PKCS8_PREFIX, seed]),
    format: "der",
    type: "pkcs8",
  });
}
function rawPublicOf(privateKey) {
  const spki = createPublicKey(privateKey).export({ type: "spki", format: "der" });
  return spki.subarray(12);
}
function edSign(privateKey, message) {
  return cryptoSign(null, Buffer.from(message), privateKey);
}

/** The same anon id derivation as `light-core::ids`. */
export function anonIdFromRootPubkey(rootPub) {
  const digest = sha256(Buffer.concat([Buffer.from("firstlight/anon-id/v1"), rootPub]));
  return `fl1_${digest.subarray(0, 16).toString("hex")}`;
}

/**
 * Create an anonymous light seed.
 *
 * @param {Buffer} [rootSeed] pass a seed to simulate restoring the same seed on
 *   a second device; the anon id will be identical.
 */
export function createIdentity(rootSeed = randomBytes(32)) {
  const rootKey = keyFromSeed(rootSeed);
  const rootPub = rawPublicOf(rootKey);
  const anonId = anonIdFromRootPubkey(rootPub);
  return { rootSeed, rootKey, rootPub, anonId, epoch: 0 };
}

/** Build the epoch certificate the server checks before trusting a key. */
export function epochCertificate(identity, epoch = 1) {
  const seed = sha256(
    Buffer.concat([
      Buffer.from("firstlight/epoch-key/v1"),
      identity.rootSeed,
      (() => {
        const b = Buffer.alloc(4);
        b.writeUInt32LE(epoch);
        return b;
      })(),
    ]),
  );
  const epochKey = keyFromSeed(seed);
  const epochPub = rawPublicOf(epochKey);

  const epochLe = Buffer.alloc(4);
  epochLe.writeUInt32LE(epoch);
  const message = Buffer.concat([
    Buffer.from("firstlight/epoch-cert/v1"),
    identity.rootPub,
    epochLe,
    epochPub,
  ]);
  const signature = edSign(identity.rootKey, message);

  return {
    key: epochKey,
    pub: epochPub,
    cert: {
      epoch,
      root_pubkey_hex: hex(identity.rootPub),
      epoch_pubkey_hex: hex(epochPub),
      signature_hex: hex(signature),
    },
  };
}

/** A registered, signing client. */
export async function makeClient(app, { rootSeed } = {}) {
  const identity = createIdentity(rootSeed);
  const { key, cert } = epochCertificate(identity, 1);

  const reg = await app.request("/v1/identities/register", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ anon_id: identity.anonId, cert }),
  });
  if (!reg.ok) throw new Error(`register failed: ${reg.status} ${await reg.text()}`);

  let counter = 0;
  async function call(method, path, body = null, extraHeaders = {}) {
    const text = body === null ? "" : JSON.stringify(body);
    const ts = Date.now();
    counter += 1;
    const nonce = `n${counter}-${randomBytes(8).toString("hex")}`;
    const digest = hex(sha256(Buffer.from(text)));
    const canonical = ["FL1", method, path, digest, String(ts), nonce].join("\n");
    const signature = edSign(key, canonical);

    const headers = {
      "content-type": "application/json",
      "x-fl-identity": identity.anonId,
      "x-fl-epoch": "1",
      "x-fl-timestamp": String(ts),
      "x-fl-nonce": nonce,
      "x-fl-signature": hex(signature),
      ...extraHeaders,
    };
    if (method !== "GET" && method !== "HEAD") {
      headers["idempotency-key"] = `idem-${counter}-${randomBytes(6).toString("hex")}`;
    }
    const res = await app.request(path, { method, headers, body: text || undefined });
    const json = await res.json().catch(() => null);
    return { status: res.status, json, res };
  }

  /** Build signature headers for an arbitrary request (used for SSE tests). */
  async function signedHeaders(method, path, text = "") {
    const ts = Date.now();
    counter += 1;
    const nonce = `h${counter}-${randomBytes(8).toString("hex")}`;
    const digest = hex(sha256(Buffer.from(text)));
    const canonical = ["FL1", method, path, digest, String(ts), nonce].join("\n");
    const signature = edSign(key, canonical);
    return {
      headers: {
        "x-fl-identity": identity.anonId,
        "x-fl-epoch": "1",
        "x-fl-timestamp": String(ts),
        "x-fl-nonce": nonce,
        "x-fl-signature": hex(signature),
      },
    };
  }

  function appRequest(path, init) {
    return app.request(path, init);
  }

  return { identity, cert, call, epochKey: key, signedHeaders, appRequest, app };
}
