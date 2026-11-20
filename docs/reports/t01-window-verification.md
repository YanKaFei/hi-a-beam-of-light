# T-01 最小窗口验收报告

日期：2026-10-31
设备：MacBook（Apple M2 Max）/ macOS 27.0.1 / 单显示器 3024×1964 @2x
构建：`firstlight-desktop` debug（`target/debug/firstlight-desktop`）
原始证据：`docs/reports/t01/t01-pre.json`、`t01-post.json`、
`T01-transparency-canvas-off.png`、`T01-transparency-canvas-on.png`

---

## 0. 结论

| 项目 | 结果 |
|---|---|
| **T-01 工程门槛** | **通过** |
| 通过项 | 10 |
| 失败项 | 0 |
| 无法验证项 | 1（多显示器：本机仅一个显示器） |

复现命令：

```bash
# 1. 构建
cargo build -p firstlight-desktop

# 2. 启动（测试通道仅在设置该环境变量时开启）
FIRSTLIGHT_STATE_DIR="$PWD/target/t01-state" \
FIRSTLIGHT_CONTROL_SOCKET="$PWD/target/t01-state/control.sock" \
  ./target/debug/firstlight-desktop &

# 3. 验证
.venv312/bin/python apps/desktop/tests/t01_verify.py \
  target/t01-state/control.sock --phase pre --out target/t01-pre.json

# 4. 重启恢复
#    (通过控制通道 quit → 重新启动 → 再跑 --phase post-restart)
```

---

## 1. 四项必须验证的能力

### 1.1 透明背景 —— PASS

**方法**：不隐藏窗口（隐藏会改变应用激活状态、导致整屏重绘），
而是只切换画布 `visibility`，让窗口本身状态完全不变。任何像素差异只可能来自光本身。

另外增加了**背景稳定性检测**：每轮采 4 帧（关闭-关闭-开启-开启），
只有「关闭两帧一致」且「开启两帧一致」时才采信结果，否则重试。
这是必要的——本机在使用中，第一版测量就因此出现过假失败。

实测：

```
changed = 4214 px
max_changed_radius = 48.3 px        （窗口半宽 132 px）
corner_changed_pixels = 0           （四角 12×12 全部未变）
edge_ring_changed_pixels = 0        （半径 >140px 的环带未变）
peak_channel_delta = 223 @ r=3.2px  （峰值在正中心）
```

**结论**：变化被完全约束在以中心为圆心、半径 48 px 的圆内，四角与边缘环带
一个像素都没变。**桌面上不存在矩形窗口背景**，只有光。

### 1.2 窗口置顶 —— PASS

实测 `NSWindow.level = 5`（普通窗口为 0，>0 即浮于其上）。
叠加证据来自 1.1：光点像素出现在普通应用窗口之上（截图中可见它覆盖在
用户窗口内容之上）。

### 1.3 点击穿透 —— PASS

macOS 决定「这个窗口要不要接收点击」的唯一依据是
`-[NSWindow ignoresMouseEvents]`。所以直接读这个属性，而不是模拟点击：

```
关闭穿透 → ignoresMouseEvents = False
开启穿透 → ignoresMouseEvents = True
再关闭   → ignoresMouseEvents = False
持久化   → settings.click_through = True（开启时）
```

**未做**：真实鼠标点击落在下层应用的端到端验证。原因见 §4。

### 1.4 非激活交互 —— PASS

判据不是「焦点有没有动」，而是「**本应用有没有成为前台应用**」。
用 `lsappinfo front` 客观测量：

```
accessory 策略：面板已显示=True，前台 Google Chrome → Google Chrome，本应用抢占焦点=False
regular  策略：面板已显示=True，前台 Google Chrome → Google Chrome，本应用抢占焦点=False
```

实现手段：窗口用 `orderFrontRegardless`（显示但不成为 key window），
随后 `NSApplication.deactivate`。应用激活策略设为 `accessory`（无 Dock 图标，
菜单栏常驻）。

---

## 2. 附加能力验收

| 项 | 结果 | 证据 |
|---|---|---|
| 光点可移动 | PASS | 移动到 (520,420)，实际 (520,420) |
| 位置可保存 | PASS | `desktop-state.json` 中 `position` 同步更新 |
| 重启后恢复 | PASS | 退出前 (740,560) → 重启后 (740,560) |
| 点击可打开控制界面 | PASS | 用与真实点击相同的 DOM 事件触发，`panel_visible` False→True |
| 穿透模式下可找回入口 | PASS | 穿透开启时通过菜单栏路径仍可打开面板 |
| 不残留阴影/标题栏 | PASS | `hasShadow=false`，`styleMask=0x8004`（无标题栏） |
| 空间切换行为 | PASS | `collectionBehavior=0b100010001`：canJoinAllSpaces + stationary + fullScreenAuxiliary |
| 多显示器与缩放 | **SKIP** | 本机仅 1 个显示器（scale=2.0） |

### 多显示器为什么是 SKIP 而不是 PASS

本机只有一块屏幕，无法做真机验证。放置算法（`geometry::place`）由 10 项单元测试
覆盖：负坐标副屏、拔掉副屏后回到主屏、跨屏时按重叠面积选择、轻微越界只做回推、
无显示器信息时保留原值。**但单元测试不等于真机验证**，因此标 SKIP 而非 PASS。

---

## 3. 过程中发现并修复的真实缺陷

这三个不是配置问题，是会在真实设备上崩溃的缺陷。

### 3.1 错误的 Objective-C selector 导致进程直接中止

第一版探针读取了 `isTitlebarAppearsTransparent`。该 selector 不存在
（正确名称是 `titlebarAppearsTransparent`），触发 ObjC 异常，
而 Rust 无法 unwind 外部异常：

```
fatal runtime error: Rust cannot catch foreign exceptions, aborting
```

修复：更正 selector，并启用 `objc2` 的 `exception` feature 作为兜底。

### 3.2 AppKit 不能在后台线程调用

控制通道在 socket 线程里读取窗口属性，AppKit 非线程安全，再次中止进程。
修复：所有 AppKit 访问统一经 `run_on_main_thread` 派发到主线程并等待结果
（`control.rs::on_main`）。这条现在是硬约束：**任何新增的 AppKit 调用都必须走它**。

### 3.3 视觉测量会被人为使用干扰

本机正在被使用，两次截图之间屏幕内容变化，导致「透明背景」测量出现假失败
（一次测得 `corner_changed=576`，即整个裁剪区都变了）。
修复：加入背景稳定性检测与重试，6 次仍不稳定则报 SKIP 而不是 FAIL。
**不做没有把握的判定。**

---

## 4. 明确未验证的部分（如实记录）

| 项 | 状态 | 原因 | 补做方式 |
|---|---|---|---|
| 真实鼠标拖拽 | 未自动验证 | 需要真实指针按下；无辅助功能权限无法注入 | 手动：按住光点拖动，松开后重启确认位置 |
| 点击穿透的端到端点击路由 | 未自动验证 | 同上。已改为读系统级开关（该开关即 macOS 的判定依据） | 手动：开启穿透后点击光点位置，确认下层应用响应 |
| 全屏应用 / Mission Control 观感 | 仅验证配置位 | 需要人为触发全屏与 Mission Control | 手动：进入全屏视频，确认光点仍可见或按设计隐藏 |
| 睡眠唤醒 | 未验证 | 需要真实睡眠周期 | 手动：合盖 1 分钟唤醒后确认光点位置与状态 |
| 多显示器 | SKIP | 无硬件 | 接第二块屏后重跑 `t01_verify.py` |

补充说明：本进程**没有**辅助功能（Accessibility）权限，也不应去获取——
那需要修改系统安全设置，属于本次授权范围之外。上述四项已改为人工步骤，
不伪造自动通过。

---

## 5. 冻结的窗口架构（后续阶段不得随意改动）

```
Tauri 2.12 窗口
  ├─ transparent: true          → isOpaque=false, backgroundColor alpha=0
  ├─ decorations: false         → styleMask 无标题栏，hasShadow=false
  ├─ alwaysOnTop: true          → NSWindow.level = 5
  ├─ visibleOnAllWorkspaces     → collectionBehavior |= canJoinAllSpaces | stationary | fullScreenAuxiliary
  ├─ macOSPrivateApi: true      → 透明合成所必需
  └─ 应用激活策略：accessory     → 无 Dock 图标；面板用 orderFrontRegardless 呈现

点击穿透：set_ignore_cursor_events(bool)     → -[NSWindow ignoresMouseEvents]
位置持久化：desktop-state.json（原子写入：临时文件 + rename）
```

**给后续阶段的约束**：
- 任何新的 AppKit 调用必须经 `on_main`。
- 不得为了效果在光点窗口上开启阴影或圆角容器——T01-1 证明当前是真正的透明。
- 窗口尺寸变化会改变 `WINDOW_SIZE` 常量与 T-01 的测量基线，需同步更新验证脚本。
