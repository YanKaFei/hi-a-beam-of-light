# M3-UI-R4.1 · 沉浸式开场收口

---

## 1. TabBar 隐藏

用官方 `wx.hideTabBar` / `wx.showTabBar`，未重构导航。

| 步 | 实现 |
|---|---|
| 1 | 进入 Ignition 前先 `hideTabBar({animation:false})` |
| 2 | **hide 成功后**才显示遮罩并开始动画（失败也放行，见 §3） |
| 3 | 动画结束 → `restoreTabBar()` → 再移除遮罩 |
| 4 | 跳过 → 同一条 `endIgnition()` 路径 |
| 5 | `onHide` / `onUnload` 均调用 `restoreTabBar()` |

**竞争保护**：用 `__tabGen` 世代号。
每次 hide/show 都递增；hide 的回调只有在**世代未变**时才把 `__tabHidden` 置真。
结束后 `restoreTabBar()` 先递增世代、再 show——迟到的 hide 回调因此无法把 bar 再藏起来。

**失败降级**：hide 有 240ms 看门狗，接口异常或超时都照常进入动画，
只是 TabBar 仍可见（不阻塞开场，不留遮罩）。

## 2. 跳过按钮

| 项 | 值 |
|---|---|
| 位置 | 左上，`left:12px` + 运行时 `statusBarHeight` |
| 尺寸 | **≥44×44**（`min-width/min-height:44px`） |
| 视觉 | 石墨灰文字、无底、无边框、无彩色 |
| 出现时机 | 遮罩出现即显示，不等动画结束 |
| 点击 | `catchtap`，不穿透到底层页面 |

放在左上是为了避开右侧胶囊（capsule）与底部 TabBar。

## 3. 实测结果

| 项 | 结果 |
|---|---|
| 总时长 | **2462 ms** |
| 帧数 | **290** |
| 动画中 TabBar | **已隐藏**（`__tabHidden: true`，截图证实） |
| 跳过之后 | **已恢复**（`__tabHidden: false`，overlay:false） |
| 切 Tab 返回 | 不重播 |
| 首页 API | **200 / 1 台设备** |
| 结束遮罩 | 已移除 |

证据：`tabbar-board.png`（前 / 中 / 后三态）、`skip-detail.png`、`hidden-{700,1300,1750}.png`

## 4. 未验证（PENDING）

1. **真机帧率与手感**：模拟器 118fps 不代表真机。
2. **中途切后台**：代码上 `onHide` 会恢复 TabBar、看门狗会收尾，
   但本轮**没有真实触发**前后台切换来验证，属未测。
3. **hideTabBar 在真机上是否与自定义导航冲突**：只在模拟器验证。
4. 低性能设备的实际收尾时间：未测。

## 5. 验收

- **ENGINEERING_PASS**：启动、跳过、导航恢复、异常放行、API 不阻塞 —— 均已实测。
- **VISUAL_APPROVED**：**不宣布**。按你的要求，需你亲自看动画后判断。

## 6. 边界

未改：speed / palette / disLocal 艺术参数、光轨数量、B 版形态、
业务逻辑、配对、证据政策、其他五页 UI。
