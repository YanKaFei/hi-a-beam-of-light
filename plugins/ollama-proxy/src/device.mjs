// Device identity for the CLI. The private key never leaves this machine and is never
// sent to the server, the phone, or any log.

import { readFileSync, writeFileSync, existsSync, mkdirSync, chmodSync } from "node:fs";
import { dirname, join } from "node:path";
import { homedir } from "node:os";
import { createPrivateKey, createPublicKey, sign as edSign, randomBytes } from "node:crypto";
import { newDeviceKey, signingInput } from "./credential.mjs";

const DEFAULT_DIR = join(homedir(), ".firstlight");
const FILE = "device.json";

export function devicePath(dir = DEFAULT_DIR) { return join(dir, FILE); }

export function loadDevice(dir = DEFAULT_DIR) {
  const p = devicePath(dir);
  if (!existsSync(p)) return null;
  const raw = JSON.parse(readFileSync(p, "utf8"));
  return {
    deviceId: raw.device_id,
    publicKeyRaw: Buffer.from(raw.pubkey_hex, "hex"),
    privateKey: createPrivateKey({ key: Buffer.from(raw.private_key_pkcs8_hex, "hex"), format: "der", type: "pkcs8" }),
    createdAt: raw.created_at_ms,
    subjectAnonId: raw.subject_anon_id ?? null,
  };
}

export function createDevice(dir = DEFAULT_DIR, deviceId) {
  const id = deviceId || `dev-${randomBytes(5).toString("hex")}`;
  const key = newDeviceKey();
  const pkcs8 = key.privateKey.export({ format: "der", type: "pkcs8" });
  mkdirSync(dir, { recursive: true, mode: 0o700 });
  const path = devicePath(dir);
  writeFileSync(path, JSON.stringify({
    device_id: id,
    pubkey_hex: key.publicKeyRaw.toString("hex"),
    private_key_pkcs8_hex: pkcs8.toString("hex"),
    created_at_ms: Date.now(),
  }, null, 2), { mode: 0o600 });
  try { chmodSync(path, 0o600); } catch { /* best effort on exotic filesystems */ }
  return { deviceId: id, publicKeyRaw: key.publicKeyRaw, privateKey: key.privateKey, createdAt: Date.now(), subjectAnonId: null };
}

export function rememberSubject(device, anonId, dir = DEFAULT_DIR) {
  const p = devicePath(dir);
  const raw = JSON.parse(readFileSync(p, "utf8"));
  raw.subject_anon_id = anonId;
  writeFileSync(p, JSON.stringify(raw, null, 2), { mode: 0o600 });
}

export function signCredential(device, payload) {
  return edSign(null, signingInput(payload), device.privateKey).toString("hex");
}

export function publicKeyHex(device) {
  return createPublicKey(device.privateKey).export({ format: "der", type: "spki" }).subarray(-32).toString("hex");
}
