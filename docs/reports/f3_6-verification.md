# F3.6 验收报告｜INK-LIGHT REFORM

日期：2026-11-01
范围：扫描授权 P0 修复 → 视觉令牌 → 光点四层材质 → 四屏重构 → 性能与回归
状态口径：PLANNED / IMPLEMENTED / TESTED / INTEGRATED / VERIFIED / BLOCKED

---

## 0. 一句话

**P0 已修复并被负向测试证明；光点与四个界面已按「当代水墨微光」重建并读真实状态。**
但真机截图 **BLOCKED**——执行时这台机器停在登录窗口（锁屏），所有应用窗口都被遮住。
我没有用设计稿或合成图冒充运行截图，见 §7。

---

## 1. 阶段 A｜P0 扫描授权绕过

### 1.1 问题
F3.5 的 `complete_transfer` 里写着 `scan::run(&dir, settings.scan_enabled || true)`。
恒真。用户关掉扫描，传光结束时仍然读本机客户端数据库。权限开关是装饰品。

### 1.2 修复
| 改动 | 文件 |
|---|---|
| 单一授权入口 `scan::authorize(scan_enabled, user_confirmed)` | `scan.rs` |
| 进程级读取计数 `executions()`，在真正触碰磁盘处自增 | `scan.rs` |
| `set_scan_enabled` 不再触发扫描（拨开关 ≠ 同意这一刻扫描） | `main.rs` |
| 新增 `scan_now(confirmed)`，显式手势单独承载 | `main.rs` |
| `complete_transfer` **完全不扫描**，只读已记录的可用证据 | `main.rs` |
| 控制通道新增 `scan_stats` / `scan_now` / `dry_run` | `control.rs` |

判定规则：**开关打开 且 本次显式确认**，两者缺一不可。

### 1.3 测试

Rust（`scan.rs::tests`，5 项）：
- `scan_disabled_blocks_transfer_scan`
- `scan_requires_explicit_consent`
- `permission_revoked_blocks_next_scan`
- `a_denied_scan_never_counts_as_an_execution`
- `denial_messages_are_actionable`

集成（`tests/f3_6_scan_auth.py`，**5/5 PASS**）——断言的是**真的读了几次**，不是返回了什么消息：

```
scan_disabled_blocks_transfer_scan      PASS  executions 0→0
scan_requires_explicit_consent          PASS  executions 0→0
confirmed_scan_runs_exactly_once        PASS  executions 0→1
permission_revoked_blocks_next_scan     PASS  executions 1→1
activation_never_scans                  PASS  executions 1→1
```

复现：
```bash
cargo test -p firstlight-desktop
cargo build -p firstlight-desktop --features test-control
python3 apps/desktop/tests/f3_6_scan_auth.py
```

状态：**IMPLEMENTED / TESTED / VERIFIED**

---

## 2. 阶段 A｜视觉令牌

`packages/visual-system/tokens.json` 从 1.0.0 升到 **2.0.0**，**原位扩展**，没有第二份运行时令牌。

| 新增/替换 | 内容 |
|---|---|
| `colors` | 墨蓝黑 `#0A0F16`、纸 `#101821`、象牙白 `#F1E9DC`、朱砂系五档 + 每档独立暖晕 |
| `inkLight` | 四层参数：core / bloom / veil / pulse（含边缘谐波、模糊、斑驳、呼吸幅度） |
| `levels` | 五档几何全部重算：核心 5.0–6.8px、墨 13–25px、光幕 20–42px |
| `type` / `layout` / `surfaces` / `paper` | 排版角色、8px 节奏、语义表面色、纸纹参数 |
| `motion` | 全部时长改为规范区间（呼吸 9.5–12s、点亮 1.4–1.9s、页面 300ms） |

Rust 侧 `light.rs` 常量同步：`palette_matches_visual_tokens` 仍为唯一护栏，改一边另一边立刻红。
**深红档的外晕透明度从 0.44 提到最高 0.34 + 半径最大**，保证「越高级不会越暗」。

状态：**IMPLEMENTED / TESTED**

---

## 3. 阶段 B｜光点四层材质

### 3.1 结构
| 层 | 实现 | 关键决策 |
|---|---|---|
| L1 朱砂核心 | 64 点低频谐波闭合路径 + 径向渐变 + 暖白小高光 | **不规则**，因为完美的圆看起来是渲染出来的 |
| L2 湿墨浸润 | 7 次偏移落笔，bake 时整层 `blur(0.42×R)` | 平涂无论轮廓多不规则都像贴纸，墨必须有软边 |
| L3 雾色光幕 | 加色径向渐变 | 透明桌面上唯一在任何壁纸下都成立的一层 |
| L4 呼吸余韵 | CSS 只作用于 veil 图层 | **核心不呼吸**，只有周围的空气在动 |

### 3.2 一张关键的结构图
F2 把整块 sprite 一起做 alpha 呼吸 → 核心跟着变暗 → 读起来是「调光的 LED」。
现在拆成两张画布：`#core` 静止，`#veil` 呼吸。这是 F3.6 最重要的一处改动。

### 3.3 单一引擎，两个宿主
`ui/light-engine.js` 抽出 `specOf / paintVeil / paintCore / blobPath`。
桌面浮层与面板都调用它——**没有第二套光点引擎**，这是任务书 §06 的硬要求。

### 3.4 已验证
- `__flProbe()` 报告 `layerCount: 4`、等级、色值、半径、`lastError: null`
- 画布像素导出（`window.__flWantPng`）：真实渲染结果，透明底
- 真实扫描 1 个家族 → `FAINT`，核心 `#F1A99D`，光幕 26px

状态：**IMPLEMENTED / TESTED**；视觉观感见 §7（真机截图 BLOCKED）

---

## 4. 阶段 C｜四屏重构

| 屏 | 结构 | 真实数据来源 |
|---|---|---|
| 我的光 | 光在上、称号其下、单列信息、hairline 分隔 | `light_status`（称号/家族数/等级） |
| 传光 | 光居中、一句标题、纸块邀请、一行状态、一个文字动作 | `transfer_snapshot` + `create_invitation` |
| 光路 | 三个数字 + 垂直墨线时间轴，不是社交图谱 | `light_paths` |
| 回响 | 极大留白，中央一句话 | `light_paths().echoes` |

去掉了：卡片、徽标 pill、并排统计块、按钮框层级。
保留并强化：留白、hairline、宋体标题、底栏四词导航（24px 指示线）。

### 4.1 状态机没有被动画篡改
- `RECEIVED` 只改外环与一句「光已经抵达，等待双方确认」；
- `ACTIVATED` 才把句子换成「这束光已经点亮」；
- 两者在 `transfer.rs::Phase` 里是不同的枚举值，`received_is_not_activated` 测试锁住；
- 面板只读 `transfer_snapshot`，不自己推断阶段。

### 4.2 机读验证（`panel_probe`）
```
ready: True   view: mine
title: 初明者    sub: 1 个可信模型家族 · 微明
verified: 1 个家族   scan: 自动扫描已关闭
lights: ['mine', 'give', 'echo']   navCurrent: ['我的光']
[give]  giveTitle=把一束光传给别人
[paths] pathsSub=传光服务未连接。断网不影响你自己的光。
[echo]  echoTitle=还没有回响。
```
四个视图都渲染、都读真实状态、导航 `aria-current` 正确。

状态：**IMPLEMENTED / TESTED**

---

## 5. 阶段 D｜无障碍

| 项 | 实现 |
|---|---|
| 桌面光点语义 | `role="img"` + 动态 `aria-label`：「我的光，初明者，已核验 1 个模型家族」 |
| 焦点 | 所有按钮 `:focus-visible` 珊瑚色描边，偏移 3px |
| 键盘 | Tab 顺序自然；`Escape` 关闭设置层；关闭后焦点回到触发按钮 |
| 命中区 | 光点整个窗口就是热区（132×132），核心不是唯一可点区域 |
| 动态播报 | 扫描结果写入 `aria-live="polite"`，只播一次，不逐帧 |
| 降低动态 | `prefers-reduced-motion` 关闭呼吸、浮动、页面位移与所有过渡 |
| 不靠颜色区分 | 等级同时有中文名、家族数、文字状态 |

状态：**IMPLEMENTED / TESTED（结构）**；真实对比度采样 **PENDING**（需要非锁屏环境）

---

## 6. 阶段 E｜性能

同机同方法（应用自测 `getrusage` + `proc_pidinfo`），`target/f36-perf.json`：

| 指标 | F3.5 | **F3.6** | 目标 | 判定 |
|---|---|---|---|---|
| 空闲（动画中）CPU 均值 | 0.048% | **0.053%** | ≤0.10% | PASS |
| 空闲 CPU 峰值 | 0.074% | **0.077%** | ≤1% | PASS |
| 安静模式 CPU | 0.033% | **0.018%** | — | 改善 |
| RSS | 113.2 MB | **112.9 MB** | ≤120 MB | PASS |
| 30 s 稳定性 | -3.4 MB | **0.0 MB** | 无增长 | PASS |
| 扫描耗时 | 2 ms | **2 ms** | — | PASS |
| 退出残留进程 | 无 | **无** | — | PASS |

常驻帧循环：仅用水墨烘焙位图 + CSS 合成器；空闲零 JS RAF（`loopRunning: false`）。

状态：**TESTED / VERIFIED（数值）**

---

## 7. 真机截图：BLOCKED

### 7.1 事实
执行截图时这台 Mac 停在**登录窗口**（锁屏）。`screencapture` 返回的画面是锁屏而不是桌面，
所有应用窗口都在锁屏之后。这不是权限问题，也不是我没有尝试：

- `caffeinate -u` 唤醒屏幕 → 仍是登录窗口
- `screencapture -x` 多次 → 得到的都是锁屏
- 尝试等待再拍 → 同一画面

**因此没有一张真实桌面/面板截图。** 我不会用设计稿、渲染图或概念图充当运行截图。

### 7.2 已交付的替代证据（这些是真实渲染，不是设计稿）
| 文件 | 性质 |
|---|---|
| `docs/reports/f3_6/light-study.png` | **应用画布导出的真实渲染**（264×264，透明底），同一张图分别叠在深墨与浅纸上 |
| `docs/reports/f3_6/light-DORMANT-canvas.png` | 上图的原始透明 PNG |
| `docs/reports/evidence/f36-scan-auth.json` | 扫描授权 5/5 的机读结果 |
| `docs/reports/evidence/f36-perf.json` | CPU/RSS 原始测量 |
| `docs/reports/evidence/f36-desktop-e2e.json` | 两实例传光闭环 14/14 |

这张图说明的是设计里的那个物理取舍：**同一张渲染，深底只见光，浅底连墨也显出来**。
它不是两种设计，是水与墨在同一种材质上的两种底色。

### 7.3 解锁后可立即复现的人工步骤
```bash
cargo build -p firstlight-desktop --features test-control
FIRSTLIGHT_STATE_DIR=$PWD/target/shot \
FIRSTLIGHT_CONTROL_SOCKET=$PWD/target/shot/control.sock \
FIRSTLIGHT_CODEX_DIR=$PWD/target/fixtures-single \
FIRSTLIGHT_IDENTITY_BACKEND=file ./target/debug/firstlight-desktop &
# 1) 点击光点打开面板 → 我的光
# 2) 底栏切换 传光 / 光路 / 回响 各截一张
# 3) 设置 → 扫描一次（走真实授权路径）
# 4) ⌘⇧4 或 python3 apps/desktop/tests/f3_6_shots.py --capture
```
状态：**BLOCKED / ENVIRONMENT（锁屏）**

---

## 8. 回归

| 套件 | 数量 | 结果 |
|---|---|---|
| Rust（light-core / adapters / store / desktop） | **193** | 全绿 |
| 协议一致性 | 12 | 全绿 |
| 服务端（状态机 + 互操作 + T1–T15） | 29 | 全绿 |
| F3.6 扫描授权（新增） | **5** | 全绿 |
| 桌面端到端（两实例传光闭环） | 14 | 全绿 |
| F2 端到端 | 10 | **9 PASS / 1 BLOCKED**（见 §9.1） |

F3.6 新增 **9** 项自动化断言（5 Rust + 5 集成中 4 项为新增语义），并把 F3.5 的 E2E
更新为「接收者先显式扫描，激活只消费已证明的证据」——这是 P0 修复的必然结果，不是放宽。

---

## 9. 未完成与限制（如实记录）

### 9.1 F2 端到端：9/10，用例 J 不通过
`J 重启后位置与本地状态正确恢复`：期望 (430,360)，实际 (1446,916)（等于默认居中放置）。
等级与家族数都正确恢复，只有窗口位置没有。

**最可能的原因同样是锁屏**：窗口服务器在会话锁定时不保证接受 `set_position`，
重放时读到的是被系统改写的坐标。F3.5 在解锁会话下此项通过。
我没有把它标记为 PASS，也没有改测试去迎合结果——解锁后必须重跑确认。

若解锁后仍失败，则说明我本轮改动（面板窗口尺寸）影响了位置恢复，需要单独修。

| 项 | 状态 | 原因 |
|---|---|---|
| 真实 macOS 桌面/面板截图 | **BLOCKED** | 机器锁屏 |
| 五档光点像素对比图 | **PARTIAL** | 只有 DORMANT 一档的可靠导出；FAINT 抓取时探针返回了上一帧（已删除，未当作证据）；3–5 档需要 4+/7+ 家族夹具 |
| 对比度实测（半透明叠加后采样） | **PENDING** | 需要非锁屏环境截屏 |
| 传光关键帧录屏 | **BLOCKED** | 同上 |
| 浅色壁纸 / 非 Retina / 小窗检查 | **PENDING** | 同上 |
| 世界光场（F4） | **未触碰** | 授权边界 |

---

## 10. 与任务书 Gate 的逐条对照

| Gate | 判定 |
|---|---|
| P0 授权绕过修复 + 负向测试 | ✅ 5/5，计数证明 |
| 至少一个真实可安装 macOS UI 构建 | ✅ debug 构建可运行（未做 notarize 安装包） |
| 光核四层小尺寸清晰、无霓虹感 | ✅ 画布导出可判；真机观感待解锁复核 |
| 四页读现有业务真实状态 | ✅ panel_probe 证据 |
| 状态机与证据规则无回归 | ✅ 193+12+29+5+14 全绿 |
| 性能无明显恶化 | ✅ 0.053% / 112.9 MB |
| 真机证据与设计稿明确区分 | ✅ §7 显式标注 |

---

## 11. 下一步建议

1. **解锁后 15 分钟内**补齐截图与录屏（§7.3 有完整步骤），这是唯一阻断发布证据的事项。
2. 增加 4 家族与 7 家族夹具，补齐三档光点的像素对比。
3. 对比度在真实叠加后采样，得出确切比值而不是估计。
4. F4 仍未启动。

**我不建议在截图缺失的情况下宣布 F3.6 完成**——视觉项目的验收标准里，真机画面不是可选项。
