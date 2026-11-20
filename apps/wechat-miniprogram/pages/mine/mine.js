const { levelOf, describe } = require("../../utils/state.js");
const { makeApi } = require("../../utils/api.js");
const light = require("../../utils/light.js");
const lightframe = require("../../utils/lightframe.js");
const { makeObserve } = require("../../utils/observe.js");
const opening = require("../../utils/opening.js");

Page({
  data: {
    statusBarHeight: 44,
    showIntro: false, introStage: "", introCopy: "", introKind: "", introOpacity: 0, skipVisible: false,
    level: "UNPAIRED", statusLine: "", observing: false, narrative: [], result: null,
    showDetail: false, deviceLabel: "", observations: [], firstRun: true, lightBase: "",
  },

  onLoad() {
    const sys = wx.getSystemInfoSync();
    this.setData({ statusBarHeight: sys.statusBarHeight || 20 });
    this.W = sys.windowWidth;
    this.H = sys.windowHeight;
  },

  async onShow() {
    const app = getApp();
    try { await app.ensureSession(); } catch (e) {
      this.setData({ statusLine: "无法连接观察服务" });
    }
    this.api = makeApi({ base: app.globalData.apiBase, session: app.globalData.session });
    this.observeCtl = makeObserve({ api: this.api });
    this.maybePlayIntro();
    this.syncBase();
    this.renderLight();
    this.__gen = (this.__gen || 0) + 1;
    this.__wantLight = true;
    this.startLight();            // the resting light breathes; without this the frame never changes
    // startup can hide/show the page once; restart if the loop was cancelled in that window
    const gen = this.__gen;
    setTimeout(() => { if (this.__wantLight && gen === this.__gen && !this.raf) this.startLight(); }, 600);
    this.refreshFacts();
  },

  onHide() { this.__gen = (this.__gen || 0) + 1; this.__wantLight = false; this.stopLight(); },
  onUnload() {
    this.__destroyed = true;
    this.__gen = (this.__gen || 0) + 1;
    this.__wantLight = false;
    this.stopLight();
    this.endIntro();
    this.ctx = null; this.canvasNode = null;      // drop the node; nothing may redraw after this
  },

  // ---------- facts ----------
  async refreshFacts() {
    try {
      const [devs, obs] = await Promise.all([this.api.myDevices(), this.api.myObservations()]);
      const devices = devs.status === 200 ? (devs.json.devices || []) : [];
      const observations = obs.status === 200 ? (obs.json.observations || []) : [];
      const device = devices.find((d) => d.paired) || null;
      const level = levelOf({ device, credential: observations[0] || null });
      this.setData({
        level,
        statusLine: describe(level).next,
        title: describe(level).title,
        deviceLabel: device ? device.device_id : "",
        observations,
      });
      this.renderLight();
    } catch (e) { /* keep the last honest state */ }
  },

  // ---------- light ----------
  ensureCanvas(cb, genAtRequest) {
    if (this.ctx) { cb(this.ctx); return; }
    wx.createSelectorQuery().in(this).select("#light").fields({ node: true, size: true }).exec((res) => {
      const hit = res && res[0];
      if (!hit || !hit.node) return;
      const dpr = (wx.getSystemInfoSync().pixelRatio) || 2;
      hit.node.width = hit.width * dpr;
      hit.node.height = hit.height * dpr;
      const ctx = hit.node.getContext("2d");
      ctx.scale(dpr, dpr);
      this.frame = lightframe.frameOf(hit.width, hit.height);
      if (getApp().globalData.alignTest) {
        this.setData({ alignTest: true, frameW: hit.width, frameH: hit.height });
      }
      this.canvasNode = hit.node;
      this.ctx = ctx; this.Wc = hit.width; this.Hc = hit.height;
      cb(ctx);
      // the canvas is the precondition for animation; starting earlier threw on clearRect
      const gen = (genAtRequest === undefined) ? this.__gen : genAtRequest;
      if (this.__wantLight && !this.raf && !this.__destroyed && gen === this.__gen) this.startLight();
    });
  },

  renderLight() {
    const gen = this.__gen;
    this.ensureCanvas(() => {
      light.draw(this.ctx, this.Wc, this.Hc, this.data.level, this.t0 || 0, { frame: this.frame,
        lift: this.lift || 0, scan: this.scan || 0, variant: this.variant(), fixture: this.fixture(),
      });
    });
  },

  variant() { const a = getApp(); return (a.globalData && a.globalData.lightVariant) || "B2-1"; },
  fixture() { const a = getApp(); return (a.globalData && a.globalData.lightFixture) || null; },
  syncBase() {
    const v = this.variant();
    const fix = this.fixture();
    let src = "";
    if (v === "HYBRID") src = "/assets/light-base-art.png";
    if (v === "B2-2") src = "/assets/light-base-stone.png";
    if (src !== this.data.lightBase) this.setData({ lightBase: src });
  },
  startLight() {
    if (this.raf) return;
    this.__wantLight = true;
    this.__lastTick = Date.now();
    const tick = () => {
      if (this.data.showIntro) {            // nothing else is allowed on screen during the opening
        if (this.ctx) this.ctx.clearRect(0, 0, this.Wc, this.Hc);
        this.raf = setTimeout(tick, 120);
        return;
      }
      if (!this.ctx) {
        this.__retry = (this.__retry || 0) + 1;
        if (this.__retry > 25 || !this.__wantLight || this.__destroyed) {
          this.raf = null;                        // give up quietly; the page still works
          return;
        }
        this.raf = setTimeout(tick, 80);
        return;
      }
      this.__retry = 0;
      const now = Date.now();
      const dt = Math.min(120, now - (this.__lastTick || now));
      this.__lastTick = now;
      this.t0 = (this.t0 || 0) + dt;          // wall-clock driven, not assumed 33ms
      this.__draws = (this.__draws || 0) + 1;
      if (this.scan > 0) this.scan = Math.max(0, this.scan - 0.01);
      if (this.lift > 0) this.lift = Math.max(0, this.lift - 0.02);
      light.draw(this.ctx, this.Wc, this.Hc, this.data.level, this.t0, { frame: this.frame, lift: this.lift || 0, scan: this.scan || 0, variant: this.variant(), fixture: this.fixture() });
      if (getApp().globalData.alignTest && this.frame) {
        const f = this.frame;
        this.ctx.save();
        const dot = (x, y, c) => { this.ctx.fillStyle = c; this.ctx.beginPath(); this.ctx.arc(x, y, 3, 0, Math.PI * 2); this.ctx.fill(); };
        dot(f.cx, f.cy, "rgba(255,0,0,1)");          // centre
        dot(f.cx + f.R, f.cy, "rgba(0,160,0,1)");    // right edge
        dot(f.cx, f.cy - f.R, "rgba(0,0,255,1)");    // top edge
        this.ctx.restore();
      }
      this.raf = setTimeout(tick, 33);
    };
    tick();
  },
  stopLight() { if (this.raf) { clearTimeout(this.raf); this.raf = null; } this.__lastTick = 0; },

  // ---------- observe ----------
  async onTapLight() {
    if (this.data.observing) return;                 // one run at a time, no request storms
    this.lift = 1; this.scan = 1;
    this.setData({ observing: true, narrative: [], result: null, showDetail: false });
    this.startLight();

    const res = await this.observeCtl.run({
      onStep: ({ stage, text, tone }) => {
        const line = { text, tone, stage };
        const keep = this.data.narrative.slice(-2);
        this.setData({ narrative: keep.concat([line]) });
      },
    });

    if (res.cancelled) { this.setData({ observing: false }); return; }

    if (res.error) {
      this.scan = 0; this.lift = 0;
      this.setData({ observing: false, result: { kind: "error", text: "无法完成观察，请重试" } });
      this.renderLight();
      return;
    }

    const device = res.devices[0] || null;
    const latest = res.latest;
    const level = levelOf({ device, credential: latest });
    this.setData({
      observing: false, level, observations: res.observations,
      deviceLabel: device ? device.device_id : "",
      statusLine: describe(level).next, title: describe(level).title,
      result: { kind: latest ? "observed" : (device ? "idle" : "empty"), level },
    });
    this.scan = 0;
    this.startLight();                                // keep breathing at the settled state
  },

  cancelObserve() {
    if (this.observeCtl) this.observeCtl.cancel();
    this.setData({ observing: false, result: null });
    this.scan = 0; this.renderLight();
  },

  // ---------- brand intro ----------
  maybePlayIntro() {
    const app = getApp();
    if (app.globalData.introDone || wx.getStorageSync("fl_intro_seen")) return;
    this.setData({ showIntro: true, introStage: "ignition", skipVisible: true });
    wx.nextTick(() => this.runIntro());
  },

  runIntro() {
    wx.createSelectorQuery().in(this).select("#intro").fields({ node: true, size: true }).exec((res) => {
      const hit = res && res[0];
      if (!hit || !hit.node) { this.endIntro(); return; }
      const canvas = hit.node;
      const dpr = (wx.getSystemInfoSync().pixelRatio) || 2;
      canvas.width = hit.width * dpr; canvas.height = hit.height * dpr;
      const ctx = canvas.getContext("2d");
      ctx.scale(dpr, dpr);

      const t0 = Date.now();
      let alive = true, lastKey = "", lastOpacity = -1;
      const tick = () => {
        if (!alive || this.__destroyed) return;
        const ms = Date.now() - t0;
        opening.draw(ctx, hit.width, hit.height, ms);
        const cue = opening.cueAt(ms);
        const key = cue ? cue.kind : "";
        const alpha = Math.round((cue ? opening.opacityAt(ms, cue) : 0) * 20) / 20;
        if (key !== lastKey || alpha !== lastOpacity) {
          this.setData({ introCopy: cue ? cue.text : "", introKind: key, introOpacity: alpha });
          lastKey = key; lastOpacity = alpha;
        }
        if (ms >= opening.DURATION - 700 && this.data.introStage !== "reveal") this.setData({ introStage: "reveal" });
        if (ms >= opening.DURATION) { this.endIntro(); return; }
        this.rafIntro = setTimeout(tick, 33);
      };
      this.__introAlive = () => { alive = false; };
      this.__introWatchdog = setTimeout(() => this.endIntro(), opening.DURATION + 800);
      tick();
    });
  },

  skipIntro() { getApp().globalData.introSkipped = true; this.endIntro(); },

  endIntro() {
    if (this.__introAlive) { this.__introAlive(); this.__introAlive = null; }
    if (this.__introWatchdog) { clearTimeout(this.__introWatchdog); this.__introWatchdog = null; }
    if (this.rafIntro) { clearTimeout(this.rafIntro); this.rafIntro = null; }
    getApp().globalData.introDone = true;
    wx.setStorageSync("fl_intro_seen", true);
    if (this.data.showIntro) this.setData({ showIntro: false, introStage: "" });
    if (!this.__destroyed) { this.__wantLight = true; this.startLight(); }
  },

  // ---------- navigation ----------
  goPair() { wx.navigateTo({ url: "/pages/pair/pair" }); },
  goTransfer() { wx.navigateTo({ url: "/pages/transfer/transfer" }); },
  goEcho() { wx.navigateTo({ url: "/pages/echo/echo" }); },
  goMore() { wx.navigateTo({ url: "/pages/paths/paths" }); },
  goSettings() { wx.navigateTo({ url: "/pages/settings/settings" }); },
  toggleDetail() { this.setData({ showDetail: !this.data.showDetail }); },
});
