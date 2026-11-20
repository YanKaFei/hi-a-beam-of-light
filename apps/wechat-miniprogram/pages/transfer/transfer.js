const { makeApi } = require("../../utils/api.js");
const { goHome } = require("../../utils/nav.js");
const COPY = {
  idle:      { title: "把一束光，交给另一个人", body: "传光不会减少你的光。对方接收后，你们之间会出现一条光路。", action: "生成邀请" },
  created:   { title: "邀请已生成", body: "邀请已生成；小程序分享和接收功能还在准备中。", action: "返回" },
  received:  { title: "对方已经接收", body: "等待双方确认后，关系才会建立。", action: "确认传光" },
  learning:  { title: "关系已建立", body: "等对方产生真实的 AI 使用后，才会点亮。", action: "查看光路" },
};
Page({
  goHome() { goHome(); },
  // custom navigation: reserve the real status bar height reported by the runtime
  __status() { const h = (wx.getSystemInfoSync().statusBarHeight || 20); this.setData({ statusBarHeight: h }); },
  data: {
    statusBarHeight: 44, stage: "idle", copy: COPY.idle, err: "", token: "" },
  async onShow() {
    this.__status();
    const app = getApp();
    try { await app.ensureSession(); } catch { this.setData({ err: "登录失败" }); return; }
    this.api = makeApi({ base: app.globalData.apiBase, session: app.globalData.session });
  },
  async act() {
    if (!this.api) return;
    if (this.data.stage === "idle") {
      const r = await this.api.createTransfer();
      if (r.status === 201) this.setData({ stage: "created", token: r.json.token, copy: COPY.created });
      else this.setData({ err: "暂时无法生成邀请：" + ((r.json.error && r.json.error.code) || r.status) });
    } else {
      this.setData({ stage: "idle", copy: COPY.idle });
    }
  },
  goPaths() { wx.navigateTo({ url: "/pages/paths/paths" }); },
});
