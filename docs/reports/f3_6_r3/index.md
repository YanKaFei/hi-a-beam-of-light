# F3.6-R3 视觉验收索引

日期：2026-11-01
全部截图取自运行中的应用；未使用设计稿、静态页或合成图。

---

## 一、四页浏览（统一 720×900 窗口、统一字体）

| # | 文件 | 说明 |
|---|---|---|
| 1 | [`page-mine.png`](page-mine.png) | 我的光 |
| 2 | [`page-give.png`](page-give.png) | 传光 |
| 3 | [`page-paths.png`](page-paths.png) | 光路（零记录空状态） |
| 4 | [`page-echo.png`](page-echo.png) | 回响 |

## 二、光点与材质

| # | 文件 | 说明 |
|---|---|---|
| 5 | [`compare-faint-colour.png`](compare-faint-colour.png) | **FAINT 颜色修订**：原版 `#F1A99D` vs 批准 `#D68B83`，深墨 / 浅纸各一行，同一 R3 引擎 |
| 6 | [`compare-paper-wet.png`](compare-paper-wet.png) | **浅纸湿中湿**：R2 → R3，同一颜色 |
| 7 | [`light-on-dark.png`](light-on-dark.png) | 光点压在自己面板（深色）上 |
| 8 | [`light-on-paper.png`](light-on-paper.png) | 光点在真实亮壁纸上 |
| 9 | [`both-windows.png`](both-windows.png) | 光点与面板同屏的真实桌面 |

## 三、传光三态（两个真实实例）

两个独立进程、独立 state 目录、独立私钥、同一个本地服务端（`127.0.0.1:8801`）。

| # | 文件 | 状态来源 |
|---|---|---|
| 10 | [`1-invite.png`](1-invite.png) | A 点击「发出一束光」→ 服务端返回 `OFFERED` |
| 11 | [`2-received.png`](2-received.png) | B 用真实签名 `accept` → A 查询得 `RECEIVED` |
| 12 | [`3-learning.png`](3-learning.png) | A `confirm` → `LEARNING`（边已建立，尚无使用证据） |
| 13 | [`4-activated.png`](4-activated.png) | B 提交 E3 后 `activate` → `ACTIVATED`，`kind=FIRST_LIGHT` |
| 14 | [`5-echo.png`](5-echo.png) | A 收到 SSE 回响 |
| 15 | [`6-paths.png`](6-paths.png) | A 的光路页显示已点亮 |

> **证据性质：B 的 E3 使用证据来自 `make_fixtures.py` 生成的 Codex 夹具，**
> **不是真实模型调用。** 签名、时间窗、去重、"证据必须晚于关系"全部为真实服务端校验，
> 但证据源头是合成的。凡引用这三态，必须同时引用这句话。
>
> 服务端实际返回：`{'already_activated': False, 'kind': 'FIRST_LIGHT', 'ok': True, 'state': 'ACTIVATED'}`

## 四、回归与性能

| # | 数据 | 值 |
|---|---|---|
| 16 | Rust | **193 全绿** |
| 17 | 协议一致性 | **12 全绿** |
| 18 | 服务端 | **29 全绿** |
| 19 | F2 桌面 E2E | **10/10** |
| 20 | F3.5 传光 E2E | **14/14** |
| 21 | 扫描授权 | **5/5**（读取计数：拒绝时 0） |
| 22 | 空闲 CPU（动画中） | **0.021%** 均值 / 0.030% 峰值 |
| 23 | 安静模式 CPU | **0.021%** 均值 |
| 24 | RSS | **113.0 MB**，20s 增长 **0.0 MB** |
| 25 | 默认构建含测试通道 | **0 处**（`--features test-control` 构建为 1 处） |

原始数据：`docs/reports/evidence/f36r3-*.json`、`f2-e2e.json`、`f35-e2e.json`、`f36-scan-auth.json`。

## 五、尚未解决

见 [f3_6_r3-report.md](f3_6_r3-report.md) 第 5 节。
