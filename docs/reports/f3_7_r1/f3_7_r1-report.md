# F3.7-R1｜空间布局诊断与修复

## 1. 真实原因（不是调参问题）

用真实 WebKit 测量 `我的光`（720×900 旧默认）后，数据直接指出原因：

```
section.h   = 765
lightStage  = y 65,  h 132
title       = y 199, h 39
primary     = y 282, h 41
rows        = y 358, h 97      → 内容结束于 455
slackPx     = 411              → 全部落在内容之后
justify-content = "normal"     → 居中从未生效
```

**根因：CSS 选择器写错了。**

section 的 id 是 `v-mine`，而样式里写的是 `#mine`。因此：

- `#mine{padding-top:…}` 空转
- `#mine::before / ::after{flex:1}` 空转（这两条本意是上下弹性占位）
- 之前那次 `justify-content:center` 也挂在同一个错误选择器上

`computed justify-content` 返回 `normal`，是这一判断的直接证据——不是被 auto margin 抵消，
也不是被固定高度拉伸，而是**规则根本没有匹配到元素**。

同时 `.rows{margin-top:auto}` 在上一轮被我移除，底部也没人托住，于是 411px 空白全堆在下半页。

## 2. 最小修复

| 改动 | 内容 |
|---|---|
| 选择器 | `#mine` → `#v-mine`，并启用 `justify-content:center` |
| 内容节奏 | `.rows` 改为常规块（`padding:22px 0 8px`），不再用 auto margin 撑底 |
| 滚动 | `section` 加 `overflow-y:auto`，内容超出时滚动而非裁切 |
| 窗口 | 默认 **720×560**，最小 **640×480** |

### 修复后实测（同一套度量）

```
h=480  viewport 720×480  section.h=370  slack= 34  justify=center
h=560  viewport 720×560  section.h=450  slack= 98  justify=center
h=900  viewport 720×900  section.h=765  slack=413  justify=center
```

居中在三种高度下都生效。560 是内容与留白最平衡的一档：内容约 350px，上下各约 50px 呼吸。
900 时剩余 413px 会被均分成上下留白——即方案 B，但那是「更大的窗口装同样的内容」。

## 3. 两种方案

**方案 A（采用）**：720×560 紧凑窗口。内容自然居中，无需滚动，无空洞。
**方案 B**：720×900，同样的居中规则下变为三段式呼吸。数据上成立（slack 413 均分），
但窗口比内容大一倍，对一个常驻工具没有收益。

**推荐 A**。理由：四个页面的内容量都在 300–420px 区间，560 能容纳且不留空洞；
拉高到 900 只是把空白做大，不增加信息。

窗口规格建议：默认 720×560，最小 640×480，最大不设（用户可拉高，布局按居中/滚动自适应）。

## 4. 未完成：本轮真实截图缺失

**没有拿到修复后的真实截图。** 原因是窗口排序问题：面板是非激活呈现
（`orderFrontRegardless` + `deactivateApp`），在缩放/切换视图后会被用户当前的应用
（微信）盖住，`screencapture` 抓到的是覆盖窗口。

我删掉了这六张错误截图，没有把它们当成证据。上一轮 720×900 的四页截图仍然有效，
可作为「修改前」。

**复现方式**（需要有人的时候执行，或面板获得焦点后立刻拍）：
```bash
FIRSTLIGHT_STATE_DIR=$PWD/target/x FIRSTLIGHT_CONTROL_SOCKET=$PWD/target/x/c.sock \
FIRSTLIGHT_IDENTITY_BACKEND=file ./target/debug/firstlight-desktop &
# 点光点打开面板 → 等 1 秒 → ⌘⇧4 截图
```

## 5. 回归与性能

| 项 | 值 |
|---|---|
| Rust | 193 全绿 |
| 空闲 CPU | 0.031% 均值 / 0.080% 峰值 |
| 安静模式 CPU | 0.112% 均值 / 0.32% 峰值 |
| RSS | 113.3 MB，20s 增长 0.0 MB |

安静模式升到 0.112%：本轮给 section 加了 `overflow-y:auto`，WebKit 在可滚动容器上
会维持额外的合成层。仍在 1% 门槛内，但比上一轮的 0.015% 高一个量级，**这是回退**，
记录在案。若需恢复，可只在内容溢出时启用滚动。

## 6. 冻结与保留

未修改：paintBead、五档颜色语义、RootIdentity、Keychain、E3/E4、扫描授权、状态机、API/SSE。
未安装任何设计插件。

仍为 PENDING（不因本轮而升级）：
- OS 层非激活窗口首次点击
- Keychain 成功路径
- 真人可用性测试（USER_STUDY_PENDING）
