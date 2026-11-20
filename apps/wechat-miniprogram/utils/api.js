// Thin client over the existing contract. No business logic lives here.
//
// WeChat has no fetch; it has wx.request. Both paths return the same shape so the pages
// never need to know which one ran.

function viaWx({ base, path, method, headers, body }) {
  return new Promise((resolve) => {
    wx.request({
      url: base + path,
      method,
      header: headers,
      data: body,
      success: (r) => resolve({ status: r.statusCode, json: r.data || {} }),
      fail: (e) => resolve({
        status: 0,
        json: { error: { code: "NETWORK", message: String(e && e.errMsg ? e.errMsg : e) } },
      }),
    });
  });
}

async function viaFetch({ base, path, method, headers, body, fetchImpl }) {
  const r = await fetchImpl(base + path, {
    method, headers, body: body === undefined ? undefined : JSON.stringify(body),
  });
  const json = await r.json().catch(() => ({}));
  return { status: r.status, json };
}

function makeApi({ base, session, fetchImpl }) {
  async function call(method, path, body) {
    const headers = { "content-type": "application/json" };
    if (session && session.token) headers.authorization = `Bearer ${session.token}`;
    // body must travel with the request; dropping it made every POST an empty JSON
    const args = { base, path, method, headers, fetchImpl, body };
    if (typeof wx !== "undefined" && wx.request) return viaWx(args);
    if (fetchImpl) return viaFetch(args);
    return { status: 0, json: { error: { code: "NO_TRANSPORT", message: "no request implementation" } } };
  }
  return {
    devLogin: (user) => call("POST", "/v1/auth/dev-login", { user }),
    me: () => call("GET", "/v1/auth/me"),
    logout: () => call("POST", "/v1/auth/logout", {}),
    myObservations: () => call("GET", "/v1/me/observations"),
    myPaths: () => call("GET", "/v1/me/paths"),
    myDevices: () => call("GET", "/v1/me/devices"),
    getDevice: (id) => call("GET", `/v1/devices/${id}`),
    openPairing: (deviceId) => call("POST", "/v1/pairing/sessions", { device_id: deviceId }),
    approvePairing: (sid, code) => call("POST", `/v1/pairing/sessions/${sid}/approve`, { code }),
    approveByCode: (code) => call("POST", "/v1/pairing/approve-by-code", { code }),
    sessionState: (sid) => call("GET", `/v1/pairing/sessions/${sid}/state`),
    createTransfer: () => call("POST", "/v1/transmissions", { channel: "QR" }),
    resolveTransfer: (token) => call("GET", `/v1/transmissions/resolve?token=${token}`),
    acceptTransfer: (id) => call("POST", `/v1/transmissions/${id}/accept`, {}),
    confirmTransfer: (id) => call("POST", `/v1/transmissions/${id}/confirm`, {}),
    paths: () => call("GET", "/v1/me/paths"),
    revokeDevice: (id) => call("POST", `/v1/devices/${id}/revoke`, {}),
  };
}

module.exports = { makeApi };
