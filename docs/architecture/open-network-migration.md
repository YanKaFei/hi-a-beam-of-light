# 第一束光 · Open Network 迁移评估

状态：**评估，未动代码**。桌面 App 冻结但未删除；无 schema、无数据、无身份改动。
文中每条"可复用"都指向仓库里真实存在的文件；标 **需确认** 的是我没验证的。

---

## 1. 资产复用清单（按可复用程度排序）

### 1.1 可直接复用（服务端，跨端不变）

| 资产 | 位置 | 说明 |
|---|---|---|
| 传光服务 | `services/api/src/app.mjs` | 20 条路由，全部与客户端类型无关 |
| 状态机 | `services/api/src/fsm.mjs` | 11 个状态：`CREATED / OFFERED / RECEIVED / ACCEPTED / LEARNING / ACTIVATED / ACKNOWLEDGED / CANCELLED / EXPIRED / REJECTED / REVOKED` |
| 回响推送 | `GET /v1/events/stream` | SSE，小程序**不支持** EventSource，需改轮询或长轮询（见 §3.4） |
| 隐私接口 | `/v1/privacy/consent`、`/revoke` | 授权与撤回已存在 |
| 匿名光场 | `GET /v1/field/summary` | 现成的聚合口径 |

### 1.2 需移植（协议规则，跨语言重写）

| 资产 | 原位置 | 迁移方式 |
|---|---|---|
| 证据等级 E0–E4 | `crates/light-core/src/evidence.rs` | 规则本身保留；TS/Kotlin 各自实现，用同一套 conformance 测试验证 |
| 身份与纪元证书 | `crates/light-core/src/identity.rs` | RootIdentity + epoch 证书。小程序/Android 需要用 Ed25519（小程序无内置，需纯 JS 实现或后端代签）**需确认** |
| 协议一致性测试 | `packages/protocol/test/conformance.test.mjs` | **这是最重要的资产**：12 项，任何新端必须先跑通它 |
| 适配器契约 | `crates/adapter-sdk/src/lib.rs` | 表级白名单 + 列级黑名单 + `is_globally_denied`，开源插件的规范来源 |

### 1.3 不再开发（保留、不删）

`apps/desktop`（Tauri，8281 行）、`ui/light-engine.js`（paintBead）、`ui/aperture-engine.js`（A2/L3）、
`design/f39*`（光点研究）、`tests/f2_e2e.py`、`f3_5_e2e.py`、`shot-mode` 截图链路。

---

## 2. 必须先说的一件事：现在的证据是"客户端自述"

`POST /v1/usage/attest`（`app.mjs:441`）实际校验的是：

- `evidence_level` 必须是 E3/E4；
- `model_family` 必须是注册表里的 slug；
- 每小时速率限制；
- `adapter_id` / `proof_method` 只做**格式**校验。

**服务端并不验证"这件事真的发生过"。** 桌面端可信是因为适配器跑在用户自己机器上、读本地库；
一旦开放成"任何人都能写插件"，这条信任链就变成**自述即证据**。

这与第四阶段"不得以客户端自行宣告的模型使用作为高可信证据"直接冲突。

**结论：开放网络的第一件事不是写小程序，是给 attest 加一个可离线验证的凭证格式。**
建议形态（待你确认）：插件用设备纪元密钥签 `{model_family, event_id, observed_at, adapter_id}`，
服务端验签 + 去重 + 速率限制。验签只能证明"同一个密钥声称"，不能证明"真的用过"——
这一点必须在产品文案里承认，不能包装成"已验证的真人使用"。

---

## 3. 三端功能边界

| | WeChat 小程序 | Android | Plugin（桌面/CLI） |
|---|---|---|---|
| 身份 | 展示、二维码承载 | 展示 + 本地安全存储 | 本地密钥存储 |
| 采集证据 | **不能**扫描其他应用 | **不能**用应用启动统计冒充 | **唯一**的合法证据来源 |
| 传光 | 发起、扫码、确认、光路、回响 | 同左 | 无 UI，只输出凭证 |
| 光场 | 展示 | 展示 | 无 |
| 动画 | 轻量 canvas | 原生 | 无 |

三条硬约束（写死，不进代码就进评审清单）：

1. 微信小程序**没有**跨应用使用统计能力，任何"自动点亮"都是假的。
2. Android 的 `UsageStatsManager` 只能看到包名与前台时长——**不是模型调用**，最多作为 E1 信号。
3. 只有插件能在用户机器上观察到真实的模型交互。

由此推出：**证据只能由插件产生，再由用户把凭证带到手机上。**
这不是缺陷，是这个架构的必然形状。

---

## 4. 共享协议与身份关联

### 4.1 协议版本
`packages/protocol` 已有 `rules.mjs` + 12 项 conformance。建议升为 `protocol/v1`：
版本号进 `usage_event.registry_semver`（字段已存在），客户端上报自己实现的版本，服务端拒绝过旧版本。

### 4.2 身份关联
RootIdentity 是**每端各自持有**还是**由主设备签发**，必须现在定，因为改起来最贵。两个方案：

- **A 一设备一身份**（推荐）：小程序生成自己的匿名身份，与插件身份**不合并**。
  传光时用邀请码关联，而不是合并身份。优点：不需要跨端私钥传输，隐私边界最干净。
- **B 主设备签发子身份**：插件为主身份，手机端拿派生凭证。优点：一个光种跨端统一；
  代价：私钥/种子必须从桌面传到手机，违背"身份密钥由用户设备安全管理"。

**建议 A**，并在文档里明确：光种是"一台设备上的一个匿名身份"，不是"一个人"。

### 4.3 传光流程（复用现有 FSM，不改）
```
A 小程序 → POST /v1/transmissions → 得 token → 生成二维码
B 扫码   → POST /v1/transmissions/:id/accept → RECEIVED
A 确认   → /confirm → LEARNING
B 插件出证据 → /usage/attest + /activate → ACTIVATED
A 收到   → SSE / 轮询 → ACKNOWLEDGED
```
其中 RECEIVED ≠ LEARNING ≠ ACTIVATED 的分界已经在 `fsm.mjs` 里，**不用重写**。

---

## 5. 首个开源插件目标：Ollama（推荐），Codex 次之

| 目标 | 依据 | 风险 |
|---|---|---|
| **Ollama** | 本地 HTTP API（`adapter-ollama/src/http.rs` 已有），用户自带模型，无厂商 ToS 冲突，跨平台 | 默认不留调用日志，需插件在代理层观察 |
| Codex | `adapter-codex` 已能读本地 SQLite（`state_*.sqlite`） | 表结构随版本变；且必须处理"内部任务标签不是模型"（`rejected_task_label` 已有） |

**建议首发 Ollama**：插件作为用户与 Ollama 之间的代理，自己记录请求（模型名、时间、token 数），
不读别人的数据库，也不碰提示词正文。这正好满足"不上传提示词"。

**不做的**：不读 Claude/ChatGPT 的私有缓存，不做浏览器扩展抓包。

---

## 6. 小程序 MVP（5 页，不更多）

| 页 | 内容 | 依赖 |
|---|---|---|
| 我的光 | 称号、可信家族数、光种 ID 缩写 | `GET /v1/light/public/:anonId` |
| 传光 | 生成二维码、有效期 | `POST /v1/transmissions` |
| 确认 | 显示对方光种、接受/拒绝 | `accept` / `reject` |
| 光路 | 已激活 / 待激活 / 我收到的 | `GET /v1/light/paths` |
| 回响 | 事件流 | SSE 不可用 → 30s 轮询 |

**技术约束（需以微信官方文档复核）**：
- 个人主体可注册小程序，但部分类目与能力受限；
- 需要 HTTPS 域名并完成备案；
- 小程序**不支持** `EventSource`，SSE 必须降级为轮询；
- 小程序 canvas 与桌面 canvas 差异大，光点渲染要重写（不能直接复用 `light-engine.js`）。

---

## 7. Android 可行性

- 目标 API：`UsageStatsManager`（包名/时长）+ 可选的 Accessibility（**不建议**，审核与隐私风险高）。
- 能做的：展示、扫码、传光、光路、回响、本地密钥（`EncryptedSharedPreferences` 或 Keystore）。
- **不能做的**：把"某个 App 被打开 3 次"当成模型使用证据。
- 工作量估算（粗）：身份 + 网络 + 5 页 ≈ 3–4 周；光点渲染复用设计参数，不复用 JS。

---

## 8. 服务器与费用（估算，非报价）

| 项 | 估计 | 备注 |
|---|---|---|
| 云服务器 2C2G | ¥50–80/月 | Node + Postgres 同机可跑 |
| 域名 + 备案 | ¥60/年 | 小程序要求 HTTPS |
| 微信小程序认证 | ¥300/年（企业主体） | 个人主体可免，但能力受限 **需确认** |
| 合计（年） | **约 ¥1,300–1,600** | 不含人力 |

现在代码里 `db.mjs` 用 `$1` 占位符，**Postgres 专用**；若换 SQLite 需改适配层。

---

## 9. 里程碑与风险

| 阶段 | 交付 | 判定标准 |
|---|---|---|
| M1 | attest 凭证格式 + 验签 | 伪造凭证被拒；同凭证重放不重复计数 |
| M2 | Ollama 开源插件 | 真实本地调用产出 E3，且不含提示词 |
| M3 | 小程序 5 页 | 扫码 → 双方确认 → 回响，全程无假状态 |
| M4 | Android | 展示与传光可用 |
| M5 | 光场 | 聚合口径匿名 |

**主要风险**

1. **证据可信度被开源稀释**（最大）。缓解：验签 + 速率 + 明确的"自述不等于真实"文案。
2. **微信审核**：涉及"传播""关系"类内容需类目合规，可能被要求整改。
3. **协议分叉**：三端各自实现 E 规则，若不复用 conformance 测试必然漂移。
4. **身份碎片化**：一设备一身份会让"我的光"在多台设备上不一致——这是设计选择，不是 bug，但必须提前说。

## 10. 我不建议现在做的事

- 不重写 Core（`light-core` 的规则已被 193 项测试覆盖）。
- 不新建三套工程骨架（先 M1+M2）。
- 不动数据库 schema。
- 不把桌面 UI 的视觉资产直接搬进小程序（canvas 差异大）。

---

## 附：本评估的证据来源

`services/api/src/{app.mjs,fsm.mjs,db.mjs}`、`crates/light-core/src/{evidence.rs,identity.rs}`、
`crates/adapter-sdk/src/lib.rs`、`crates/adapter-ollama/src/http.rs`、
`packages/protocol/src/rules.mjs`、`packages/protocol/test/conformance.test.mjs`。
未运行任何测试本轮的结论均来自代码阅读。
