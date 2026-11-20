# M3-UI-R4.3 · 跳过遮挡与末帧修正

---

## 1. 跳过按钮与标题重叠

**原因**：`revealing` 只改了遮罩背景色，跳过按钮仍在左上角不透明存在。
2240ms 时首页「第一束光」正好出现在同一区域，两个东西叠在一起。

**修复**：`.ignition-skip.hide { opacity: 0; pointer-events: none; }`，
`transition: opacity 180ms`。
`revealing` 一置真（2080ms）就淡出，比标题显现早约 100ms，且淡出后不再接收点击。

- 2080ms 之前：跳过完全可用
- 2080ms 起：淡出，180ms 后不可见不可点
- 标题显现时：左上只剩「第一束光」

截图证据：`reveal/r2080.png`（跳过已淡出）、`reveal/r2240.png`（标题显现，无重叠）

## 2. 末帧

上一版 GIF 的末帧其实取自 2360ms——仍在转场中，且停留了 1100ms。

**这次的做法**：
跑一次**真实播放**（不走取帧钩子），等 `ignitionDone` 之后再等 900ms，
确认 `overlay:false, hidden:false, revealing:false` 之后才截图。

```
post-run state: {"hidden":false,"overlay":false,"revealing":false}
```

该帧即 GIF 的最终停留帧，停留 900ms。

**没有做任何图像编辑**：没有手动擦掉跳过按钮，没有补画 TabBar。

## 3. 新 GIF

`ignition-r43.gif`：35 帧时间轴（0–2040ms）+ 5 帧渐显（2080–2400ms）+ 真实末帧。

帧序与时长：
0–2040 每 60ms；渐显段 70ms；末帧 500/500/900ms 停留。

## 4. 工程回归（重测）

| 项 | 结果 |
|---|---|
| 帧数 | **291** |
| 结束后遮罩 | 已移除 |
| 切 Tab 返回 | 不重播 |
| 跳过 | 生效 |
| 首页 API | **200 / 1 台设备** |

## 5. 仍 PENDING

- 真机帧率与安全区
- 真实前后台切换
- 原生 hide/show 在真机的延迟

## 6. 状态

`speed` / `palette` / `disLocal` 与光轨结构**均未改**。
**VISUAL_APPROVED 不宣布**，等你看这一版。
