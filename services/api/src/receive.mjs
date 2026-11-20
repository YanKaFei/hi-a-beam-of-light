// The receive page.
//
// Opened by scanning a QR code or tapping an NFC tag. It is a real page with a
// real accept flow, not a screenshot: if the browser supports Ed25519 in
// WebCrypto, it creates an ephemeral anonymous identity and accepts the light.
// Otherwise it says so plainly instead of pretending.
//
// The page never receives the token's secret meaning beyond what the token
// itself is, and it never displays the sender's identity — only an anonymous
// label.

export function renderReceivePage({
  brand = {},
  state = "INVALID",
  token = "",
  transmissionId = "",
  initiatorLabel = "",
  expiresAtMs = 0,
  baseUrl = "",
  stateLabelOverride = "",
  noteOverride = "",
}) {
  const name = brand.public?.nameZh ?? "第一束光";
  const title = brand.ui?.receivePageTitle ?? "你收到了一束光";
  const tagline = brand.taglines?.invitation ?? "把一束光传给别人。";
  const stateLabel = {
    INVALID: "这个入口已失效",
    CREATED: "光正在准备",
    OFFERED: "这束光正在等着你",
    RECEIVED: "已有人在接收这束光",
    ACCEPTED: "双方已确认",
    LEARNING: "已确认，等待点亮",
    ACTIVATED: "这束光已经点亮",
    ACKNOWLEDGED: "这束光已经点亮",
    CANCELLED: "发送者取消了这束光",
    EXPIRED: "这束光已经过期",
    REJECTED: "这束光被拒绝了",
    REVOKED: "这束光已被撤回",
  }[state] ?? "未知状态";

  const actionable = state === "CREATED" || state === "OFFERED";
  const expiresIso = expiresAtMs ? new Date(expiresAtMs).toISOString() : "";

  return `<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
<meta name="referrer" content="no-referrer" />
<title>${escapeHtml(title)} · ${escapeHtml(name)}</title>
<style>
  :root {
    --night:#0B1425; --mist:#53657D; --ivory:#F2E7D0;
    --coral:#F1A99D; --crimson:#DF4655; --deep:#8D102E; --mint:#9FD8B4;
  }
  *{box-sizing:border-box}
  html,body{height:100%}
  body{
    margin:0; display:grid; place-items:center; padding:28px 22px;
    font-family:-apple-system,"PingFang SC",system-ui,sans-serif; color:var(--ivory);
    background:
      radial-gradient(120% 70% at 50% -10%, rgba(83,101,125,.42) 0%, rgba(11,20,37,0) 60%),
      radial-gradient(90% 60% at 50% 112%, rgba(141,16,46,.30) 0%, rgba(11,20,37,0) 70%),
      var(--night);
    min-height:100%;
  }
  .card{width:100%; max-width:380px; text-align:center}
  .light{
    width:96px; height:96px; margin:0 auto 26px; border-radius:50%;
    background: radial-gradient(circle at 45% 40%, #FFB4AA 0%, var(--crimson) 46%, var(--deep) 100%);
    box-shadow: 0 0 46px 12px rgba(223,70,85,.32);
    animation: breathe 7s ease-in-out infinite;
  }
  @keyframes breathe{0%,100%{transform:scale(.94);opacity:.9}50%{transform:scale(1);opacity:1}}
  @media (prefers-reduced-motion: reduce){ .light{animation:none} }
  h1{font-size:19px; font-weight:600; letter-spacing:.08em; margin:0 0 10px}
  .state{font-size:13px; color:var(--coral); letter-spacing:.06em; margin-bottom:6px}
  .from{font-size:11.5px; color:var(--mist); margin-bottom:22px}
  p.note{font-size:12px; color:var(--mist); line-height:1.8; margin:18px 0 0}
  button{
    width:100%; padding:13px 16px; margin-top:10px;
    border-radius:12px; border:1px solid rgba(242,231,208,.18);
    background:rgba(242,231,208,.07); color:var(--ivory);
    font-size:13.5px; letter-spacing:.06em; font-family:inherit; cursor:pointer;
  }
  button:hover{background:rgba(242,231,208,.13)}
  button.primary{border-color:rgba(223,70,85,.45); background:rgba(223,70,85,.16)}
  button:disabled{opacity:.45;cursor:default}
  .ok{color:var(--mint)}
  .warn{color:var(--coral)}
  .foot{font-size:10.5px; color:var(--mist); margin-top:26px; line-height:1.8}
  code{color:var(--coral); font-size:10.5px}
</style>
</head>
<body>
  <main class="card">
    <div class="light" id="light"></div>
    <h1>${escapeHtml(title)}</h1>
    <div class="state" id="state">${escapeHtml(stateLabelOverride || stateLabel)}</div>
    ${initiatorLabel ? `<div class="from">来自 ${escapeHtml(initiatorLabel)}</div>` : ""}

    <div id="actions">
      ${actionable
        ? `<button class="primary" id="accept">接收这束光</button>`
        : `<button disabled>${escapeHtml(stateLabel)}</button>`}
    </div>

    <p class="note" id="note">${
      noteOverride ? escapeHtml(noteOverride) : actionable
        ? "接收不需要手机号、邮箱或实名。你会在本设备上得到一个匿名光种。<br/>收到光不等于学会 AI：只有你真正使用了 AI 之后，这束光才会被点亮。"
        : escapeHtml(stateLabel)
    }</p>

    <div class="foot">
      ${name} · ${escapeHtml(tagline)}<br/>
      <span id="detail"></span>
    </div>
  </main>

<script>
const TOKEN = ${JSON.stringify(token)};
const TX_ID = ${JSON.stringify(transmissionId)};
const EXPIRES = ${JSON.stringify(expiresIso)};
const BASE = ${JSON.stringify(baseUrl)};

const $ = (id) => document.getElementById(id);

function setState(text, cls) {
  const el = $("state");
  el.textContent = text;
  el.className = "state " + (cls || "");
}

function b64url(bytes) {
  let s = "";
  for (const b of new Uint8Array(bytes)) s += String.fromCharCode(b);
  return btoa(s).replace(/\\+/g, "-").replace(/\\//g, "_").replace(/=+$/, "");
}
function hex(bytes) {
  return [...new Uint8Array(bytes)].map((b) => b.toString(16).padStart(2, "0")).join("");
}
async function sha256Hex(text) {
  const d = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return hex(d);
}

// Derive an anonymous id exactly as the Rust core does.
async function anonIdFrom(rootPubBytes) {
  const domain = new TextEncoder().encode("firstlight/anon-id/v1");
  const buf = new Uint8Array(domain.length + rootPubBytes.length);
  buf.set(domain, 0);
  buf.set(rootPubBytes, domain.length);
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", buf));
  return "fl1_" + hex(digest.slice(0, 16));
}

async function ed25519Pair(seed) {
  const pkcs8 = new Uint8Array([
    0x30,0x2e,0x02,0x01,0x00,0x30,0x05,0x06,0x03,0x2b,0x65,0x70,0x04,0x22,0x04,0x20,
    ...seed,
  ]);
  return crypto.subtle.importKey("pkcs8", pkcs8, { name: "Ed25519" }, true, ["sign"]);
}
async function rawPublic(privateKey) {
  // Export SPKI and strip the 12-byte header.
  const spki = new Uint8Array(await crypto.subtle.exportKey("spki", privateKey));
  return spki.slice(12);
}
async function sha256Bytes(bytes) {
  return new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
}

async function signedFetch(method, path, body, identity) {
  const text = body ? JSON.stringify(body) : "";
  const ts = Date.now();
  const nonce = b64url(crypto.getRandomValues(new Uint8Array(16)));
  const canonical = ["FL1", method, path, await sha256Hex(text), String(ts), nonce].join("\\n");
  const sig = new Uint8Array(await crypto.subtle.sign("Ed25519", identity.key, new TextEncoder().encode(canonical)));
  const res = await fetch(BASE + path, {
    method,
    headers: {
      "content-type": "application/json",
      "x-fl-identity": identity.anonId,
      "x-fl-epoch": String(identity.epoch),
      "x-fl-timestamp": String(ts),
      "x-fl-nonce": nonce,
      "x-fl-signature": hex(sig),
    },
    body: text || undefined,
  });
  return { status: res.status, json: await res.json().catch(() => ({})) };
}

async function makeIdentity() {
  if (!crypto.subtle || !(await supportsEd25519())) {
    throw new Error("unsupported");
  }
  const seed = crypto.getRandomValues(new Uint8Array(32));
  const rootKey = await ed25519Pair(seed);
  const rootPub = await rawPublic(rootKey);
  const anonId = await anonIdFrom(rootPub);

  // Epoch key derived the same way the core does: sha256(domain ‖ seed ‖ epochLE)
  const domain = new TextEncoder().encode("firstlight/epoch-key/v1");
  const buf = new Uint8Array(domain.length + 32 + 4);
  buf.set(domain, 0); buf.set(seed, domain.length);
  new DataView(buf.buffer).setUint32(domain.length + 32, 1, true);
  const epochSeed = await sha256Bytes(buf);
  const epochKey = await ed25519Pair(epochSeed);
  const epochPub = await rawPublic(epochKey);

  // Epoch certificate: root signs (domain ‖ rootPub ‖ epochLE ‖ epochPub)
  const certDomain = new TextEncoder().encode("firstlight/epoch-cert/v1");
  const cb = new Uint8Array(certDomain.length + 32 + 4 + 32);
  cb.set(certDomain, 0);
  cb.set(rootPub, certDomain.length);
  new DataView(cb.buffer).setUint32(certDomain.length + 32, 1, true);
  cb.set(epochPub, certDomain.length + 36);
  const certSig = new Uint8Array(await crypto.subtle.sign("Ed25519", rootKey, cb));

  return {
    anonId,
    epoch: 1,
    key: epochKey,
    cert: {
      epoch: 1,
      root_pubkey_hex: hex(rootPub),
      epoch_pubkey_hex: hex(epochPub),
      signature_hex: hex(certSig),
    },
  };
}

let ed25519Support = null;
async function supportsEd25519() {
  if (ed25519Support !== null) return ed25519Support;
  try {
    await crypto.subtle.generateKey({ name: "Ed25519" }, true, ["sign", "verify"]);
    ed25519Support = true;
  } catch {
    ed25519Support = false;
  }
  return ed25519Support;
}

const acceptBtn = $("accept");
if (acceptBtn) {
  acceptBtn.addEventListener("click", async () => {
    acceptBtn.disabled = true;
    try {
      setState("正在接收…");
      const identity = await makeIdentity();
      const reg = await fetch(BASE + "/v1/identities/register", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ anon_id: identity.anonId, cert: identity.cert }),
      });
      if (!reg.ok) throw new Error("register " + reg.status);

      const res = await signedFetch(
        "POST",
        "/v1/transmissions/" + TX_ID + "/accept",
        {},
        identity,
      );
      if (res.status >= 400) {
        throw new Error(res.json?.error?.code || String(res.status));
      }
      setState("已收到这束光", "ok");
      $("note").innerHTML =
        "光现在是你的了，但它还没有点亮。<br/>当你真正开始使用 AI，这束光才会真正亮起。";
      $("actions").innerHTML = "";
      $("detail").textContent = "你的匿名光种：" + identity.anonId.slice(0, 12) + "…";
    } catch (e) {
      const unsupported = String(e.message).includes("unsupported");
      setState(unsupported ? "此浏览器不支持匿名接收" : "接收失败", "warn");
      $("note").innerHTML = unsupported
        ? "请在安装了《第一束光》的设备上打开这个链接完成接收。"
        : "请稍后重试，或请发送者重新生成一个邀请。";
      acceptBtn.disabled = false;
    }
  });
}

if (EXPIRES) {
  $("detail").textContent = "有效期至 " + new Date(EXPIRES).toLocaleTimeString();
}
</script>
</body>
</html>`;
}

function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, (c) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
  }[c]));
}
