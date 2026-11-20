// Login. In production this exchanges wx.login's code for a server session; the
// AppSecret and session_key never exist on the device.
function isConfigured(appid) { return Boolean(appid) && appid !== "touristappid"; }

async function login({ appid, apiBase, fetchImpl, wxLogin, devUserId }) {
  const f = fetchImpl;
  if (!isConfigured(appid)) {
    // Development adapter: no WeChat call, a local identity, clearly labelled.
    const r = await f(`${apiBase}/v1/auth/dev-login`, {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ user: devUserId || "dev-user" }),
    });
    return { mode: "DEV_ADAPTER", status: r.status, session: await r.json().catch(() => ({})) };
  }
  const { code } = await wxLogin();
  const r = await f(`${apiBase}/v1/auth/wechat-login`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ code }),
  });
  return { mode: "WECHAT", status: r.status, session: await r.json().catch(() => ({})) };
}

module.exports = { isConfigured, login };
