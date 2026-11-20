# F3 安全与隐私测试报告

代码：`services/api/src/auth.mjs`、`app.mjs`、`migrations/001_init.sql`
测试：`services/api/test/*.test.mjs`（29 项，全绿）

---

## 1. 信任模型：能证明什么，不能证明什么

这一节必须写在最前面，因为它决定了所有"已激活"数字的真实含义。

### 1.1 能证明
- 某个**光种**（`anon_id`，由根公钥派生）用其当前 epoch 密钥签发了这个请求。
- 该请求没被篡改（签名覆盖方法、路径、body 哈希、时间戳、nonce）。
- 该请求没被重放（nonce 一次性，主键约束）。
- 该光种的两个 epoch 属于同一根密钥（证书链），因此多设备不重复计数。
- 该光种在某时刻提交了一条 E3/E4 使用证据，且该证据**晚于**传光关系建立。

### 1.2 不能证明
| 不能证明 | 原因 | 影响与缓解 |
|---|---|---|
| 光种背后是**唯一自然人** | 匿名公钥模型不绑定身份 | 对外只说"激活节点"，不说"人数"；软性反刷量见 §4 |
| 客户端**未被篡改** | E3 是客户端自证 | E3 与 E4 严格分列；服务端不把 E3 当第三方证明 |
| 该 AI 使用是**第一次**发生 | 只看得到本系统可见的证据 | 文案只说"该光种首次确认的 AI 使用" |
| 证据对应的模型家族判断正确 | 归一化在客户端 | 注册表版本随事件上报；服务端校验 slug 形状；争议时可复核 |

**结论**：系统能保证"某个匿名光种确实完成了一次可核验的、发生在传光之后的使用"，
不能保证"某个真人在现实中被教会了"。所有对外表述必须保持这个精度。

---

## 2. 授权书 §7 的要求逐条核对

| 要求 | 实现 | 证据 |
|---|---|---|
| 无手机号/邮箱/实名 | 注册只需证书，无任何 PII 字段 | schema 无对应列（T14） |
| 双方各自独立光种 | `identity` 两行，边两端必不同 | `check (helper_id <> helped_id)` |
| 传光必须双方确认 | `RECEIVED` 只有一方确认不建边 | T3+T4 |
| 自我传光无效 | `403 SELF_TRANSMISSION` | T9 |
| 同光种多设备不重复 | `anon_id` 由根公钥派生 | T11 |
| 一次邀请不能无限领取 | `SESSION_TAKEN` + 状态机 | T9 |
| 重复安装不得无限刷传播 | `unique(helper_id, helped_id)` + 每日邀请上限 | §4 |
| 状态转换有事务保护 | `db.tx` + `version` 乐观锁 | T3/T9 |
| 不接收 Key/提示词/正文 | 禁止字段名单 + 拒绝未知字段 | T14 |

---

## 3. 数据面：库里到底存了什么

`schema` 全表核对（`information_schema.columns`）：

| 表 | 关键列 | 是否含隐私内容 |
|---|---|---|
| `identity` | `anon_id`, `root_pubkey`, `status` | 公钥与匿名 ID，无 PII |
| `identity_key_epoch` | `epoch`, `pubkey` | 公钥 |
| `request_nonce` | `nonce`, 时间 | 随机数 |
| `idempotency_key` | `key`, `request_hash`, `response` | 请求哈希与响应体（不含敏感内容，因为请求本身不含） |
| `transmission` | `token_hash`, `state`, `version`, `expires_at_ms` | **只有令牌哈希**，无明文 |
| `transmission_party` | `identity_id`, `confirmed_at_ms` | 谁确认了 |
| `usage_event` | `model_family`, `evidence_level` | **家族名**（公开模型名），无聊天 |
| `light_edge` | `kind`, `state`, `visibility` | 关系元数据 |
| `outbox_event` | `payload`（一句话回响） | 无接收者身份 |
| `consent` | `scope`, `state` | 授权状态 |
| `audit_log` | `action`, `detail` | 动作名，无内容 |
| `transmission_log` | `event`, `from/to_state`, `actor_id` | 状态轨迹 |

**测试 T14 断言**：schema 中不存在名为 `chat`/`message`/`prompt`/`api_key`/`content`/`tokens_used` 的列，
且这些字段出现在请求体里会被 `400 INVALID_SCHEMA` 拒绝。

---

## 4. 反滥用机制

| 机制 | 参数 | 位置 |
|---|---|---|
| 每日邀请上限 | 20 / 光种 / 24h | `FL_MAX_INVITATIONS_PER_DAY` |
| 证据提交上限 | 30 / 光种 / 小时 | `FL_MAX_ATTEST_PER_HOUR` |
| 邀请 TTL | 60 s – 8 min（可配，默认 8 min） | `FL_INVITATION_TTL_MS` |
| 令牌熵 | 32 字节 CSPRNG（base64url） | `secureToken()` |
| 令牌存储 | 仅 SHA-256 哈希 | `transmission.token_hash` |
| nonce 一次性 | 主键 | `request_nonce` |
| 家族去重 | 唯一约束 | `usage_event` |
| 关系去重 | 唯一约束 | `light_edge` |
| SSE 限速 | 30 条/流/分钟 | `FL_SSE_MAX_EVENTS_PER_MINUTE` |
| 时钟偏移 | ±120 s | `FL_CLOCK_SKEW_MS` |

### 仍然敞开的滥用面（如实记录）
匿名模型下，一个人可以用多个光种互相"传光"来虚增数字。当前缓解是软的：

- 同一 `(helper, helped)` 只计一次；
- 邀请与证据提交有日/时上限；
- `RESONANCE` 不计数，纯互刷的"老朋友"不会得分；
- 但**跨光种串通无法被密码学阻止**。

建议 F4/F6 增加：同源 IP 段聚类降权、新光种的激活计入称号前 24h 冷却。
这些都需要真实数据分布才能定阈值，现在拍板等于猜。

---

## 5. 隐私权利的落实

| 权利 | 端点 | 效果 | 测试 |
|---|---|---|---|
| 只观看不贡献 | 无（观看不需要身份） | 公开接口无需鉴权 | T2 |
| 撤回公开展示 | `POST /v1/privacy/consent`（REVOKED） | 立即把可见边改为 PRIVATE；公开接口返回空 | T12 |
| 删除可关联记录 | `POST /v1/privacy/revoke`（ALL） | 删除 usage_event、撤销边与会话；返回不可逆说明 | T12b |
| 撤销未完成会话 | 同上 | `state='REVOKED'` | T12b |
| 不可逆汇总说明 | 响应字段 `irreversible_note` | 明确告知区域汇总不可逆 | T12b |

**没有"用户已撤回但关系仍可反查"的路径**：公开接口先查 consent，未 GRANTED 直接返回空
（T12 断言撤回前后从 1 条边变为 0 条）。

---

## 6. 与 F1/F2 隐私边界的连续性

| F1/F2 保证 | F3 是否破坏 |
|---|---|
| 不读聊天正文 | 未破坏；服务端无相关列，请求无相关字段 |
| 不读 `tokens_used` | 未破坏；禁止字段名单包含它 |
| 列级 allowlist | 未破坏；F3 不触碰本机数据库 |
| 不上传原始数据库 | 未破坏；上报只有 8 个字段 |
| E2 不计入可信 | 未破坏；服务端只接受 E3/E4 |
| 失败不提权 | 未破坏；F3 无系统权限操作 |

---

## 7. 传输与部署安全现状

| 项 | 状态 |
|---|---|
| HTTPS | **未部署**。本地验证走 http loopback（授权允许） |
| TLS 证书 | 无（BLOCKED：需域名） |
| 公网暴露 | **无**。服务只在 127.0.0.1 监听 |
| 数据库凭据 | 本地 PGlite，无凭据 |
| 日志 | 只记录错误消息，不记录请求体 |
| 速率限制 | 应用层，未接入网关 |

**明确声明**：当前服务不是生产形态，不得对公网发布。见
[f3-deployment-prep](f3-deployment-prep.md)。

---

## 8. 测试通道剥离（T15）

F2 的测试控制通道已改为 Cargo feature：

```toml
[features]
default = []
test-control = []
```

验证方法（可复跑）：

```bash
cargo build -p firstlight-desktop
strings target/debug/firstlight-desktop | grep -c FIRSTLIGHT_CONTROL_SOCKET   # → 0

cargo build -p firstlight-desktop --features test-control
strings target/debug/firstlight-desktop | grep -c FIRSTLIGHT_CONTROL_SOCKET   # → 1
```

结果：正式构建中 **ABSENT**，测试构建中 PRESENT。F3 的自动化仍使用测试构建。

---

## 9. 结论

| 结论 | 依据 |
|---|---|
| 服务端不存、不接收任何聊天内容/Key/正文 | T14 + schema 审计 |
| RECEIVED 不计为成功 | T3+T4 |
| 无证据不能激活，且证据必须晚于关系 | T5 / T6b |
| 重复与并发不双计数 | T8 / T9 / 唯一约束 |
| 撤回后不可再公开反查 | T12 |
| Rust 与 TS 的密码学实现一致 | 4 项 interop 测试（含真实 Rust 签名） |
| 信任模型边界已明确表述 | §1 |
| 测试通道不出现在正式构建 | §8 |

**最强的一条限制**：本报告只能证明"匿名光种层面的可信"，不能证明"自然人数"。
任何对外数字都必须维持这个措辞精度。
