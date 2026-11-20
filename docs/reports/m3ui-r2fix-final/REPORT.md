# R2-FIX-FINAL · 自动呼吸生命周期验收

## 0. 根因（不是 stopLight，是异常）

线上状态一直是 `draws:1, t:0, raf:false, want:true`。
我先怀疑是某个 `stopLight()` 掐掉的，但复现证明不是：

```js
p.ctx = null; p.startLight();
→ threwOnStart: "Cannot read properties of null (reading 'clearRect')"
→ afterNullCtx: { draws: 1, t: 0, raf: false }     // 与线上完全一致
p.ctx = saved; p.startLight();
→ afterCtxBack: { draws: 9, t: 290, raf: true }    // 正常
```

**真因**：`onShow` 同步调用 `startLight()`，但 `renderLight()` 是异步取 canvas 的，
第一个 `tick` 时 `this.ctx` 还是 `null` → `clearRect` 抛错 → 循环在排下一次定时器之前就死了。
`__wantLight` 一直是 true，所以"看起来像被停掉"。

**修复**（三处）：

1. `tick` 里 `if (!this.ctx) { this.raf = setTimeout(tick, 80); return; }` —— 不抛，等画布。
2. `ensureCanvas` 回调里：`if (this.__wantLight && !this.raf) this.startLight();` —— 画布到位才起循环。
3. `__gen` 世代号：`onHide/onUnload` 递增；600ms 自愈回调用 `gen === this.__gen` 校验，
   过期的延迟回调不会复活已隐藏或已销毁的页面。

## 1. AUTO_START（无任何手动调用）

| 时刻 | draws | t (ms) | raf | gen |
|---|---|---|---|---|
| 0ms | 86 | 3121 | true | 1 |
| 1000ms | 112 | 4054 | true | 1 |
| 3000ms | 168 | 6059 | true | 1 |
| 7000ms | 279 | 10069 | true | 1 |

`reLaunch` 后从未调用 `startLight()`，循环自行开始。

## 2. 生命周期

| 动作 | draws | raf | gen | want |
|---|---|---|---|---|
| 自动运行 1s | 126 → 155 | true | 1 | true |
| **onHide** | 155（冻结） | **false** | 2 | false |
| onHide 后 1.3s | **155（仍未动）** | false | 2 | false |
| **onShow** | 164 | **true** | 3 | true |
| onShow 后 1.2s | 198 | true | 3 | true |

- 隐藏后**没有自愈重启**（1.3s 内 draws 完全不变）。
- 显示后恢复，且**只有一条循环**。

## 3. 单循环验证

绘制速率 **28.3 帧/秒**（间隔 33ms）。
若是两条循环叠加，速率会接近 57/s。实测 28.3 → **SINGLE_RAF 通过**。
循环用 wall-clock 累计 `t`，掉帧不会让呼吸变慢。

## 4. 动态强度（真实取证）

| 帧 | 最大差 | 不同像素 | 环区平均差 |
|---|---|---|---|
| frame-1 | 4 | 118,691 | 1.05 |
| frame-3 | 5 | 108,056 | 1.03 |
| frame-5 | 4 | 118,682 | 1.05 |

`breathing.gif`：6 帧真实截图，间隔 900ms。

**动态存在，但幅度只有 4–5 级（环区平均 1.0 左右）。**
在手机屏幕上这基本不可感知。这不是取证问题，是艺术参数问题——
按指示本轮不动光学参数，所以只如实记录。

## 5. 状态

| 项 | 状态 |
|---|---|
| AUTO_START | **PASS** |
| SINGLE_RAF | **PASS**（28.3 fps ≈ 一条 33ms 循环） |
| LIFECYCLE_CLEANUP | **PASS**（hide 停且不复起，show 恰好一条，gen 拦过期回调） |
| MOTION_EXECUTION | **PASS**（帧间差异非零，118k 像素） |
| MOTION_PERCEPTIBILITY | **FAIL**（幅度 4–5 级，肉眼难辨） |
| VISUAL_APPROVED | **PENDING** |

## 6. 未改

光学参数（FIELD / FILAMENT / ENERGY / MOTION）一行未动；
未改业务、配对、证据、激活政策；`PLUGIN_OBSERVED.activate=false` 保持。
