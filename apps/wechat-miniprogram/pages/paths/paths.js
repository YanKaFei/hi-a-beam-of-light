const { makeApi } = require("../../utils/api.js");
const { goHome } = require("../../utils/nav.js");
Page({
  goHome() { goHome(); },
  // custom navigation: reserve the real status bar height reported by the runtime
  __status() { const h = (wx.getSystemInfoSync().statusBarHeight || 20); this.setData({ statusBarHeight: h }); },
  data: {
    statusBarHeight: 44, out: [], inc: [], err: "", loaded: false },
  async onShow() {
    this.__status();
    const app = getApp();
    try { await app.ensureSession(); } catch { this.setData({ err: "登录失败" }); return; }
    const api = makeApi({ base: app.globalData.apiBase, session: app.globalData.session });
    const r = await api.myPaths().catch(() => null);
    if (!r || r.status !== 200) { this.setData({ err: "暂时无法读取光路", loaded: true }); return; }
    this.setData({
      out: (r.json.outgoing || []).map((e) => ({ ...e, label: e.state === "ACTIVATED" ? "已激活" : "待激活" })),
      inc: r.json.incoming || [], err: "", loaded: true,
    });
  },
  back() { wx.navigateBack({ delta: 1 }); },
});
