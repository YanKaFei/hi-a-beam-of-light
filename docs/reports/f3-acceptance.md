# F3 开发报告与验收

日期：2026-10-31
范围：传光服务端、动态二维码与接收页、双方确认、证据激活、SSE 回响、NFC 入口语义、隐私与反滥用

---

## 1. 交付清单与状态

| # | 交付物 | 位置 | 状态 |
|---|---|---|---|
| 1 | API 与事件协议文档 | [f3-api-protocol.md](f3-api-protocol.md) | **完成** |
| 2 | 传光状态机（含 F1 审计） | 同上 §3 + `services/api/src/fsm.mjs` | **TESTED** |
| 3 | 数据库迁移与约束 | `services/api/migrations/001_init.sql` | **TESTED** |
| 4 | 动态二维码与接收页 | `src/app.mjs`（`/t/:token`、`/t/:token/qr.svg`） | **IMPLEMENTED / TESTED（服务端）** |
| 5 | 实时回响（SSE） | `src/app.mjs` `/v1/events/stream` | **TESTED** |
| 6 | NFC 兼容方案 | `/n/:tagId` + §6 | **IMPLEMENTED / TESTED（软件层）**；真机 **HARDWARE_PENDING** |
| 7 | 匿名身份与防重放 | `src/auth.mjs` | **TESTED** + 跨语言 interop |
| 8 | 隐私、撤回、反滥用 | §5 + [安全报告](f3-security-report.md) | **TESTED** |
| 9 | 个人光路（服务端） | `GET /v1/light/paths` | **TESTED** |
| 10 | 个人光路（桌面端界面） | — | **NOT STARTED（见 §7）** |
| 11 | 品牌单点配置 | `packages/brand/brand.json` | **完成** |
| 12 | 测试通道剥离 | Cargo feature | **VERIFIED** |

---

## 2. 测试总量与结果

| 套件 | 数量 | 结果 |
|---|---|---|
| Rust（F1+F2 回归） | 163 | 全绿 |
| `packages/protocol` 跨语言一致性 | 12 | 全绿 |
| `services/api` 状态机夹具 | 4（覆盖 220 条转移） | 全绿 |
| `services/api` Rust↔TS 互操作 | 4 | 全绿 |
| `services/api` F3 验收 T1–T15 | 21 | 全绿 |
| **合计** | **204** | **全绿** |

复现：

```bash
# 服务端（含状态机、互操作、验收）
cd services/api && node --test test/*.test.mjs

# 协议一致性
cd packages/protocol && node --test test/conformance.test.mjs

# Rust 全量
cargo test

# 真实 HTTP 冒烟（可选）
FL_PORT=8791 FL_PUBLIC_BASE_URL=http://127.0.0.1:8791 node src/serve.mjs
```

---

## 3. 验收用例 T1–T15 逐条结果

| ID | 用例 | 结果 | 证据 |
|---|---|---|---|
| T1 | A 发起动态二维码邀请 | **PASS** | 201；令牌 ≥32 字节；库中只有哈希 |
| T1b | 邀请渲染出可扫的二维码 | **PASS** | `/t/:token/qr.svg` 返回 SVG，未知令牌 404 |
| T2 | B 通过接收页 / 独立客户端接收 | **PASS** | 页面 200 且含接收按钮；令牌可解析为 OFFERED |
| T3 | 双方分别确认 | **PASS** | 单方确认停在 RECEIVED；双方齐备才建边 |
| T4 | B 收到来光但仍不计成功 | **PASS** | `RECEIVED`/`LEARNING` 时 A 的 `transmitted_nodes = 0` |
| T5 | 无证据不能激活 | **PASS** | 404 `EVIDENCE_NOT_FOUND`，状态保持 LEARNING |
| T6 | 合格且晚于关系的使用完成激活 | **PASS** | 200；`kind = FIRST_LIGHT`；计数 0→1 |
| T6b | 早于关系的使用不能激活 | **PASS** | 409 `EVIDENCE_PRECEDES_RELATION` |
| T7 | A 通过 SSE 收到一次回响 | **PASS** | 流中含 `event: edge.activated`，且带 `id:` |
| T8 | 断线重连不重复计数/回响 | **PASS** | 重复激活后 outbox 仍 1 条、边仍 1 条 |
| T9 | 重复使用、过期、自传、并发被拒 | **PASS** | `SESSION_TAKEN` / `SELF_TRANSMISSION` / replay |
| T9b | 过期邀请明确报告且不可接受 | **PASS** | `expired: true`；accept → 410 |
| T10 | NFC 无有效会话时不认领用户 | **PASS** | 页面提示"没有开启的传光"；0 会话 0 关系 |
| T11 | 恢复同一光种不重复计数 | **PASS** | 同根种子 → 同 anon id；identity 仍 2 行 |
| T11b | 第二 epoch 可注册使用 | **PASS** | 轮换后仍同一身份 |
| T12 | 撤回后公开接口不再暴露关系 | **PASS** | 公开边 1 → 0 |
| T12b | 撤回删除可关联证据 | **PASS** | usage 清空、边撤销、返回不可逆说明 |
| T13 | 离线本地光点可用，恢复后安全同步 | **PASS（F2 已测）** | F2 端到端 H/I：无服务器时本地光点照常工作 |
| T14 | 不存在可读正文/Key/私有文件的路径 | **PASS** | 6 类禁止字段被 400；schema 无对应列 |
| T15 | 正式 release 不含可启用的测试通道 | **PASS** | `strings` 对比：默认构建 ABSENT，feature 构建 PRESENT |

### 环境不具备、明确未验证的
| 项 | 状态 | 原因 |
|---|---|---|
| 两台真实设备各持一份 App 联调 | **BLOCKED** | 需 iOS/Android（无 Xcode/SDK），且桌面端尚未接入服务端（§7） |
| 真实公网 HTTPS 接收页 | **BLOCKED** | 未购买域名、未部署（授权要求另行批准） |
| 实体 NFC 光石触碰 | **HARDWARE_PENDING** | 无标签、无读写器 |
| 接收页在真实浏览器中完成接收 | **PARTIAL** | 页面与流程已实现；本机 headless Chrome 被沙箱 crashpad 写入限制阻断，未能截图取证。服务端路径已由 T2 证明 |
| 多代光路可视化 | 未开始 | 数据库已支持（`light_edge` 自关联可扩展），F3 不做 |

---

## 4. 关键技术决策

### 4.1 状态机不重写，只移植并锁定
F1 的状态机在 Rust 里已经实现并测过。服务端用 TypeScript，因此做了**逐条移植**，
并用 Rust 生成的 **220 条转移夹具**双向锁定。
`services/api/test/fsm.test.mjs` 会跑完所有 (状态 × 事件 × 有无证据) 组合。
这样"服务端偷偷放宽 RECEIVED→ACTIVATED"这种改动一定会失败。

### 4.2 一处语义澄清
F1 的 `ACCEPTED` 只在转移内部瞬时存在：双方确认齐备的同一转移内即建边并进入 `LEARNING`。
外部可观察序列是 `RECEIVED → LEARNING`；"已接受"这一事实由
`transmission_party.confirmed_at_ms` + `EdgeFormed` 副作用独立表达。
授权书要求"验证双方确认"——满足；但不引入一个可能与 F1 冲突的持久状态。

### 4.3 服务端不接受 `activated=true` 这类字段
激活必须提交 `usage_event_id`，服务端自己查库核对：
归属、可计数等级、**时间晚于关系建立**、边处于 FORMEd。
客户端无法用一句布尔值让传光成功。

### 4.4 传播类型在关系建立时判定，在激活时校正
`FIRST_LIGHT` / `NEW_DISCOVERY` / `RESONANCE` 三态。
`RESONANCE` 永不计数，从而避免"两个已点亮的人互相碰一下"被反复奖励。

### 4.5 密码学跨语言一致性有真凭据
`crates/light-core/examples/emit_signed_request.rs` 用 Rust 生成一个真实签名请求，
`services/api/test/rust-interop.test.mjs` 用 TS 验证它。
这排除了"两套实现各自自洽但互不兼容"的经典陷阱。

---

## 5. 数据库迁移与约束

文件：`services/api/migrations/001_init.sql`（12 张表）。

结构性的约束（不是业务代码的自觉）：

| 约束 | 作用 |
|---|---|
| `usage_event` 唯一 `(identity_id, model_family)` | 同家族永不重复计数 |
| `usage_event` `check (evidence_level in (3,4))` | E0–E2 在数据库层进不来 |
| `light_edge` 唯一 `(helper_id, helped_id)` | 同一对节点不重复获奖 |
| `light_edge` `check (helper_id <> helped_id)` | 自我传光不可能入库 |
| `light_edge` 唯一 `transmission_id` | 一次会话最多一条边 |
| `transmission` `check (expires_at_ms > created_at_ms)` | 无负 TTL |
| `transmission.token_hash` 唯一 | 令牌不可复用 |
| `request_nonce` 主键 | 重放不可能 |
| `transmission_log` 全量记录 | 状态轨迹可审计 |

**PGlite 与生产 Postgres 执行同一份迁移**（ADR-003），测试不用内存 mock。

---

## 6. 本次新增的工程纪律

1. **`brand.json` 单点配置**：名称、标语、URL 路径模板、NFC 限制都在这里；
   业务代码不出现品牌字符串。
2. **测试通道改为 Cargo feature**：`--features test-control` 才编入，
   正式构建里连字符串都不存在（已验证）。
3. **跨语言夹具成为默认做法**：标题阶梯、证据判定、状态机、签名请求，
   四处都有 Rust/TS 双向锁定。

---

## 7. 明确未完成：桌面端接入服务端（个人光路 UI）

这是本阶段最大的一处缺口，必须说清楚，而不是用一份好看的文档掩盖。

**状态：NOT STARTED。**

原因（按依赖顺序）：
1. F2 的桌面端**从未创建持久光种**——F1 设计了 `RootIdentity`，F2 只做了窗口与视觉，
   身份持久化（OS Keychain）被推迟。
2. 没有光种就无法签名请求，也就无法调用 `/v1/light/paths`。
3. 在这之上才是：传光 UI（生成邀请、显示二维码、确认）、光路列表、回响波纹。

**这意味着**：F3 的传光闭环目前在"服务端 + Web 接收页 + 测试客户端"层面完整可用，
但**桌面 App 还不能发起一次传光**。用 F2 的状态口径，这一项是 `PLANNED`，不是 `INTEGRATED`。

**建议的 F3.5 顺序**（每步都能独立验收）：
1. 桌面端生成并持久化光种（Keychain + 恢复码导出）——2 天量级
2. 桌面端签名请求 + `POST /v1/identities/register`——半天
3. 生成邀请 + 面板显示二维码（可用服务端 `/t/:token/qr.svg`）——1 天
4. 双方确认 UI + 光路列表——1–2 天
5. 回响波纹（复用 F2 的 `far_pulse` 令牌与事件通道）——半天

**为什么不在本轮硬做**：身份持久化牵涉 Keychain 权限与恢复码流程，属于 F2 遗留的技术债，
草率实现会比不做更危险（一个丢失的光种等于丢失全部历史）。

---

## 8. 回归确认

| 检查 | 结果 |
|---|---|
| F1/F2 全部测试 | 163 项全绿 |
| 协议一致性 | 12 项全绿 |
| F2 桌面端构建（默认 feature） | 通过，无警告阻断 |
| 测试通道剥离 | 已验证 |
| 空闲 CPU 基线 | 未因 F3 变化（F3 不触碰桌面渲染路径） |
| 设计令牌唯一来源 | 未新增任何硬编码视觉常量 |
| 未引入高频 JS 动画循环 | 确认：F3 未改动光点渲染循环 |
| 其他项目未被修改 | 确认：仅 `first-light/` 内有改动 |

---

## 9. 结论

服务器端与协议层已经**真的能用**：一次传光从邀请、接收、双方确认、证据提交、
激活到回响，每一步都有测试，且"RECEIVED 不算成功""证据必须晚于关系""重复不双计"
这三条最容易被放松的规则都被结构性约束住了。

但 **桌面 App 还不能发起传光**，因为 F2 没有做光种持久化。这是下一轮要补的第一件事，
不是可选项——没有它，这条链路的最后一公里仍然是断的。
