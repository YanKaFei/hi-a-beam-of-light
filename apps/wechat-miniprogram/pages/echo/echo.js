const { makeApi } = require("../../utils/api.js");
const { goHome } = require("../../utils/nav.js");
Page({
  goHome() { goHome(); },
  // custom navigation: reserve the real status bar height reported by the runtime
  __status() { const h = (wx.getSystemInfoSync().statusBarHeight || 20); this.setData({ statusBarHeight: h }); },
  data: {
    statusBarHeight: 44, echoes: [], err: "", loaded: false },
  async onShow() {
    this.__status();
    const app = getApp();
    try { await app.ensureSession(); } catch { this.setData({ err: "登录失败" }); return; }
    this.api = makeApi({ base: app.globalData.apiBase, session: app.globalData.session });
    await this.load();
    this.timer = setInterval(() => this.load(), 15000);
  },
  onHide() { if (this.timer) clearInterval(this.timer); },
  onUnload() { if (this.timer) clearInterval(this.timer); },
  async load() {
    const r = await this.api.myPaths().catch(() => null);
    if (!r || r.status !== 200) { this.setData({ err: "暂时无法读取回响", loaded: true }); return; }
    const fmt = (ms) => { const d = new Date(ms); const p = (n) => String(n).padStart(2, "0");
      return `${d.getFullYear()} / ${p(d.getMonth()+1)} / ${p(d.getDate())}  ${p(d.getHours())}:${p(d.getMinutes())}`; };
    this.setData({ echoes: (r.json.echoes || []).map((e) => ({ ...e, when: fmt(e.at_ms) })),
                   err: "", loaded: true });
  },
});
