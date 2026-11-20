const { makeApi } = require("../../utils/api.js");
const { goHome } = require("../../utils/nav.js");
Page({
  goHome() { goHome(); },
  // custom navigation: reserve the real status bar height reported by the runtime
  __status() { const h = (wx.getSystemInfoSync().statusBarHeight || 20); this.setData({ statusBarHeight: h }); },
  data: {
    statusBarHeight: 44, device: null, anonId: "", revoked: false, loggedOut: false, expanded: false },
  async onShow() {
    this.__status();
    const app = getApp();
    try { await app.ensureSession(); } catch { return; }
    this.api = makeApi({ base: app.globalData.apiBase, session: app.globalData.session });
    const [me, d] = await Promise.all([this.api.me(), this.api.myDevices()]);
    this.setData({
      anonId: me.status === 200 ? me.json.anon_id : "",
      device: d.status === 200 ? (d.json.devices[0] || null) : null,
    });
  },
  toggle() { this.setData({ expanded: !this.data.expanded }); },
  async revoke() {
    if (!this.data.device) return;
    const r = await this.api.revokeDevice(this.data.device.device_id);
    this.setData({ revoked: r.status === 200, device: null });
  },
  async logout() {
    await this.api.logout();
    wx.removeStorageSync("fl_session");
    getApp().globalData.session = null;
    this.setData({ loggedOut: true });
  },
  back() { wx.navigateBack({ delta: 1 }); },
});
