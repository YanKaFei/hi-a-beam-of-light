# R2-FIX 结项 + R5.2 准备：坐标系与图层对齐

## 一、生命周期最终检查

| 测试 | 结果 |
|---|---|
| Canvas 未就绪不抛未捕获异常 | **PASS**：`threw: null`，tick 改为等待而非绘制 |
| 画布永不就绪时停止重试 | **PASS**：`__retry` 到达 27 后 `raf:false`，不再调度 |
| 隐藏后异步回调不启动动画 | **PASS**：`onHide` 后 1.3s `draws` 完全冻结 |
| 销毁后释放 | **PASS**：`destroyed:true, ctx:null, raf:false, drawsFrozen:true` |
| `__gen` 拦住过期初始化/自愈 | **PASS**：`ensureCanvas(cb, genAtRequest)` 与 `gen === this.__gen` 双校验 |
| 无重复 RAF | **PASS**：28.3 帧/秒 ≈ 单条 33ms 循环（双循环会接近 57/s） |

`ROOT_CAUSE = ASYNC_CANVAS_INITIALIZATION_RACE` 已记录。

## 二、R5.2 渲染结构（已落地）

新增 `utils/lightframe.js` —— **唯一的坐标来源**：

```
frameOf(width, height) -> { cx, cy, R, halo, core, offset }
```

- `R = min(w,h) * 0.30`
- `cx = w/2`，`cy = h*0.34`
- 内场 `0.92R`，大晕 `0.50 × 短边`，偏移 `(+0.05R, -0.035R)`
- `backing(frame, dpr)`：DPR 上限 3，避免 3x 画布的填充开销

`light.js` 不再自己算几何，改为接收 `opts.frame`；
页面在取到画布后调用一次 `frameOf()`，画布与基底共用同一份。

## 三、对齐验证：抓到一处真实错位

我加了一个开发态对齐层（基底图 + canvas 十字 + R 圆），实测：

| 测量 | 结果 |
|---|---|
| 修前：基底可见场中心 vs 契约点 | dy **42.3 CSS px**（基底整体偏高） |
| 根因 | `.light-wrap` 没有 `position: relative`，绝对定位的基底跑到了外层包含块 |
| 修复 | 给 `.light-wrap` 加 `position: relative` |
| 修后 | dy **15.5 CSS px**（同一测量口径） |

**但 15.5 这个数字不可信**：光环中心 `cy=115`，大晕半径 168 → 顶部被画布裁掉，
场质心天然被拉高；亮丝质心又被页面上其它亮像素污染。
所以**对齐尚未被证明收敛到 ±2px**。

正确做法（下一步、很小）：在基底 PNG 里烘焙一个已知位置的标记点，
截图后只找这个标记与 canvas 十字，做点对点比较。本轮没做，不拿污染数据充数。

## 四、结论与状态

| 项 | 状态 |
|---|---|
| AUTO_START | PASS |
| SINGLE_RAF | PASS |
| LIFECYCLE_CLEANUP | PASS |
| MOTION_EXECUTION | PASS |
| MOTION_PERCEPTIBILITY | **FAIL**（4–5 级，肉眼难辨） |
| R52_FRAME_CONTRACT | **PASS**（单一坐标来源，两层共用） |
| R52_ALIGNMENT | **FAIL / 未收敛**（42px 已修，残余未证实） |
| VISUAL_APPROVED | **PENDING** |

## 五、边界

- B2-1 光学参数（FIELD / FILAMENT / ENERGY / MOTION）**已冻结**，本轮未动。
- 未改插件观察、签名、来源核验、激活、传光规则。
- 生产仍为 `PAIRED_IDLE`（1 台设备 / 0 条观察），暖金仅 VISUAL_FIXTURE。
- 未复用 B2-2 的旧 PNG 作为最终资产；本轮基底只是对齐验证用的临时件。
