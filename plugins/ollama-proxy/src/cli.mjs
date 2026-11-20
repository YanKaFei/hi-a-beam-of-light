#!/usr/bin/env node
// fl-ollama-proxy — device-bound observer.
//
//   init                 create this machine's device key
//   pair                 open a pairing session and wait for the phone
//   status               show binding state
//   serve                start the proxy and submit signed observations
//   claim <transfer>     show the open challenge for a transfer
import { randomBytes, sign as edSign } from "node:crypto";
import { fileURLToPath } from "node:url";
import { loadDevice, createDevice, rememberSubject, signCredential, publicKeyHex } from "./device.mjs";
import { createProxy, listen } from "./proxy.mjs";
import { loadRegistry } from "./registry.mjs";
import { buildCredential } from "./credential.mjs";
import { pairingProofMessage } from "../../../services/api/src/pairing.mjs";

const [cmd, ...rest] = process.argv.slice(2);
const flag = (name, def) => {
  const i = rest.indexOf(`--${name}`);
  return i >= 0 && rest[i + 1] ? rest[i + 1] : def;
};
const SERVER = flag("server", process.env.FL_SERVER || "http://127.0.0.1:8787");

const json = async (path, init) => {
  const r = await fetch(`${SERVER}${path}`, init);
  const body = await r.json().catch(() => ({}));
  if (!r.ok) throw new Error(`${path} → ${r.status} ${JSON.stringify(body)}`);
  return body;
};

if (cmd === "init") {
  const d = createDevice();
  console.log(`设备密钥已创建：${d.deviceId}`);
  console.log(`公钥：${Buffer.from(d.publicKeyRaw).toString("hex")}`);
  console.log("私钥只保存在 ~/.firstlight/device.json（权限 600），不会上传。");
} else if (cmd === "pair") {
  const device = loadDevice();
  if (!device) { console.error("先运行 init"); process.exit(2); }
  await json("/v1/devices/register", {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ device_id: device.deviceId, pubkey_hex: publicKeyHex(device), adapter_id: "ollama-proxy" }),
  }).catch(() => {});
  const session = await json("/v1/pairing/sessions", {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ device_id: device.deviceId }),
  });
  console.log(`配对码：${session.code}`);
  console.log(`指纹：${session.fingerprint}（手机上应显示同一串）`);
  console.log(`有效期至：${new Date(session.expires_at_ms).toLocaleTimeString()}`);
  console.log("请在小程序中输入配对码并确认……");
  const deadline = Date.now() + 5 * 60_000;
  let state = "OFFERED";
  while (Date.now() < deadline) {
    const s = await json(`/v1/pairing/sessions/${session.session_id}/state`).catch(() => null);
    state = s?.state ?? "OFFERED";
    if (state === "APPROVED") break;
    if (state === "EXPIRED" || state === "CANCELLED") break;
    await new Promise((r) => setTimeout(r, 2000));
  }
  if (state !== "APPROVED") { console.error(`配对未完成（${state}）`); process.exit(3); }
  const proof = signPairingProof(device, session, session.code);
  const done = await json(`/v1/pairing/sessions/${session.session_id}/confirm`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ device_id: device.deviceId, code: session.code, signature_hex: proof }),
  });
  console.log(`已绑定。指纹 ${done.fingerprint}`);
  if (done.subject_anon_id) rememberSubject(device, done.subject_anon_id);
} else if (cmd === "status") {
  const device = loadDevice();
  if (!device) { console.error("尚未初始化"); process.exit(2); }
  const s = await json(`/v1/devices/${device.deviceId}`);
  console.log(JSON.stringify(s, null, 2));
} else if (cmd === "claim") {
  const transferId = rest[0];
  const device = loadDevice();
  const r = await json(`/v1/devices/${device.deviceId}/challenge`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ transfer_id: transferId }),
  });
  console.log(JSON.stringify(r, null, 2));
} else if (cmd === "serve") {
  const device = loadDevice();
  if (!device) { console.error("先运行 init 与 pair"); process.exit(2); }
  const registry = loadRegistry(fileURLToPath(new URL("../../../data/model-registry.v1.json", import.meta.url)));
  const token = flag("token", randomBytes(24).toString("hex"));
  const server = createProxy({
    upstream: flag("upstream", "http://127.0.0.1:11434"),
    registry, token,
    onObservation: async (obs) => {
      const payload = {
        protocol_version: "fl.usage.credential/v1",
        credential_id: `cred_${randomBytes(8).toString("hex")}`,
        issuer_key_id: device.deviceId,
        subject_identity_id: device.subjectAnonId,
        source_type: "PLUGIN_OBSERVED",
        adapter_id: obs.adapterId, adapter_version: obs.adapterVersion,
        model_family: obs.family, observation_method: "http_response_completed",
        observed_at: obs.at, issued_at: Date.now(),
      };
      const credential = { ...payload, signature: signCredential(device, payload) };
      console.log(`✔ 已观察到 1 次 Ollama 调用（${obs.model} → ${obs.family}）`);
      console.log(`  · 真实调用：${process.env.FL_MOCK_UPSTREAM ? "MOCK_UPSTREAM_COMPLETION" : "REAL_OLLAMA_COMPLETION?"}`);
      console.log("  · 本地观察：LOCAL_PLUGIN_OBSERVATION");
      console.log("  · 凭证已签名：SIGNATURE_VALID");
      const r = await json("/v1/usage/attest-device", {
        method: "POST", headers: { "content-type": "application/json" },
        body: JSON.stringify({ credential }),
      }).catch((e) => ({ error: String(e) }));
      if (r.error) console.log(`  · 服务端未接受：${r.error}`);
      else {
        console.log(`  · 服务端已接收：${r.signature_status} / ${r.source_class}`);
        console.log(`  · 是否具备激活资格：${r.eligible?.activate ? "是" : "否（来源未获独立核准）"}`);
      }
    },
    log: (e) => { if (process.env.FL_PROXY_DEBUG === "1") console.error(JSON.stringify(e)); },
  });
  const addr = await listen(server, Number(flag("port", "11435")));
  console.log(`代理已启动 http://127.0.0.1:${addr.port}  token=${token}`);
} else {
  console.log("用法: init | pair | status | claim <transfer_id> | serve");
}

function signPairingProof(device, session, code) {
  const msg = pairingProofMessage({ sessionId: session.session_id, code, deviceId: device.deviceId });
  return edSign(null, msg, device.privateKey).toString("hex");
}
