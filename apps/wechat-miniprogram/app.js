// No secret lives here. The business session is a random token from the server.
const { makeApi } = require("./utils/api.js");

App({
  globalData: { apiBase: "http://127.0.0.1:8787", session: null, loginError: "" },
  onLaunch() {
    const cached = wx.getStorageSync("fl_session");
    if (cached && cached.expires_at_ms > Date.now()) this.globalData.session = cached;
  },
  async ensureSession() {
    if (this.globalData.session) return this.globalData.session;
    const api = makeApi({ base: this.globalData.apiBase, session: null });
    // Development adapter only. In production the server refuses this route.
    const r = await api.devLogin(wx.getStorageSync("fl_dev_user") || "dev-user");
    if (r.status !== 201) {
      this.globalData.loginError = r.json.error?.code || String(r.status);
      throw new Error(this.globalData.loginError);
    }
    this.globalData.session = { token: r.json.token, expires_at_ms: r.json.expires_at_ms, anon_id: r.json.anon_id };
    wx.setStorageSync("fl_session", this.globalData.session);
    return this.globalData.session;
  },
});
