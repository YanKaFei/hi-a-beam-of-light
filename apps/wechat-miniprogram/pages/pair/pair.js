const { makeApi } = require("../../utils/api.js");
const { goHome } = require("../../utils/nav.js");
const CODE_LEN = 8;
const normalize = (v) => String(v || "").toUpperCase().replace(/[^A-Z0-9]/g, "");

Page({
  goHome() { goHome(); },
  // custom navigation: reserve the real status bar height reported by the runtime
  __status() { const h = (wx.getSystemInfoSync().statusBarHeight || 20); this.setData({ statusBarHeight: h }); },
  data: {
    statusBarHeight: 44, code: "", fingerprint: "", phase: "", msg: "", err: "", pending: false, step: 1 },
  async onShow() {
    this.__status();
    const app = getApp();
    try { await app.ensureSession(); } catch { this.setData({ err: "登录失败，请重试" }); return; }
    this.api = makeApi({ base: app.globalData.apiBase, session: app.globalData.session });
    const active = await this.refreshDevice();
    if (active) this.setData({ step: 3 });
  },
  onInput(e) {
    const raw = e && e.detail ? e.detail.value : "";
    this.setData({ code: raw, err: "" });
  },
  async confirm() {
    if (this.data.pending) return;
    if (!this.api) { this.setData({ err: "登录失败，请重试" }); return; }
    const raw = this.data.code;
    if (!String(raw).trim()) { this.setData({ err: "请输入配对码" }); return; }
    if (normalize(raw).length !== CODE_LEN) { this.setData({ err: "配对码是 8 位字母或数字" }); return; }
    this.setData({ pending: true, err: "", msg: "" });
    let r = await this.api.approveByCode(String(raw));
    if (r.status === 401) {
      const app = getApp();
      wx.removeStorageSync("fl_session"); app.globalData.session = null;
      await app.ensureSession();
      this.api = makeApi({ base: app.globalData.apiBase, session: app.globalData.session });
      r = await this.api.approveByCode(String(raw));
    }
    if (r.status === 200) {
      this.setData({ phase: "APPROVED", step: 2, fingerprint: r.json.fingerprint || "",
        msg: "已在手机上确认。核对两端指纹一致后，在电脑上完成最后一步。" });
      this.waitForActive();
    } else {
      const c = r.json && r.json.error && r.json.error.code;
      this.setData({ err: c === "CODE_UNKNOWN" ? "配对码无效或已被使用"
        : c === "PAIRING_EXPIRED" ? "配对码已过期，请在电脑上重新生成"
        : c === "NETWORK" ? "网络不可用，请检查后重试"
        : "配对失败：" + (c || r.status) });
    }
    this.setData({ pending: false });
  },
  async waitForActive() {
    this.stopPoll();
    this.timer = setInterval(async () => {
      const ok = await this.refreshDevice();
      if (ok) this.stopPoll();
    }, 2000);
  },
  stopPoll() { if (this.timer) { clearInterval(this.timer); this.timer = null; } },
  onUnload() { this.stopPoll(); },
  onHide() { this.stopPoll(); },
  async refreshDevice() {
    const r = await this.api.myDevices().catch(() => null);
    const dev = r && r.status === 200 ? (r.json.devices || []).find((d) => d.status === "ACTIVE") : null;
    if (dev) { this.setData({ phase: "ACTIVE", step: 3, msg: "已连接：" + dev.device_id }); return true; }
    return false;
  },
  back() { wx.navigateBack({ delta: 1 }); },
});
