# F3 API 与事件协议

服务端：`services/api`（Hono + PGlite/Postgres）
线格式契约：[`packages/protocol/schemas/`](../../packages/protocol/schemas)（11 个 JSON Schema，F1 冻结）
状态机夹具：[`packages/protocol/fixtures/transmission_fsm.json`](../../packages/protocol/fixtures/transmission_fsm.json)（220 例，Rust 生成）

---

## 1. 认证：Ed25519 签名请求

无 Cookie、无会话 token。每个请求自带签名。

| 请求头 | 内容 |
|---|---|
| `X-FL-Identity` | `anon_id`（`fl1_` + 32 hex） |
| `X-FL-Epoch` | 密钥轮次 |
| `X-FL-Timestamp` | Unix 毫秒 |
| `X-FL-Nonce` | 16 字节随机，base64url |
| `X-FL-Signature` | Ed25519 签名，hex |
| `Idempotency-Key` | 仅写操作必需 |

签名原文（`\n` 分隔，无尾随换行）：

```
FL1
<METHOD>
<PATH（不含 query）>
<sha256_hex(body)>
<timestamp>
<nonce>
```

### 校验顺序（顺序本身是安全属性）
1. 时间窗 ±120 s → 否则 `CLOCK_SKEW`
2. `anon_id` 存在且状态 ACTIVE
3. 该 epoch 公钥存在且未退役
4. **验签**
5. **nonce 一次性**写入（主键冲突即 `NONCE_REPLAYED`）

nonce 在验签**之后**才写入：否则攻击者可以用伪造请求烧掉受害者的 nonce。

### 身份注册
`POST /v1/identities/register`

```json
{ "anon_id": "fl1_…", "cert": {
    "epoch": 1,
    "root_pubkey_hex": "…64 hex…",
    "epoch_pubkey_hex": "…64 hex…",
    "signature_hex": "…128 hex…" } }
```

服务端验证：
1. `sha256("firstlight/anon-id/v1" ‖ root_pubkey)[..16] == anon_id 的后 32 hex`
2. root 对 `"firstlight/epoch-cert/v1" ‖ root_pubkey ‖ epoch_le32 ‖ epoch_pubkey` 的签名有效
3. `epoch_pubkey ≠ root_pubkey`

证书自包含，所以多设备恢复与密钥轮换都只需再注册一个 epoch（`T11b`）。

---

## 2. 端点

| 方法 | 路径 | 鉴权 | 幂等 | 说明 |
|---|---|---|---|---|
| POST | `/v1/identities/register` | 无（靠证书） | 天然 | 注册光种 / 追加 epoch |
| POST | `/v1/transmissions` | 签名 | ✓ | 生成一次性邀请 |
| POST | `/v1/transmissions/{id}/offer` | 签名（发起方） | — | 显式标记已展示 |
| GET | `/t/{token}` | 无 | — | 接收页（HTML） |
| GET | `/t/{token}/qr.svg` | 无 | — | 邀请二维码（SVG） |
| GET | `/v1/transmissions/resolve?token=` | 无 | — | 令牌换状态（供页面与测试） |
| POST | `/v1/transmissions/{id}/accept` | 签名（接收方） | ✓ | 接收方确认 |
| POST | `/v1/transmissions/{id}/confirm` | 签名（发起方） | ✓ | 发起方确认 |
| POST | `/v1/transmissions/{id}/reject` | 签名（接收方） | — | 拒绝 |
| POST | `/v1/transmissions/{id}/cancel` | 签名（发起方） | — | 取消 |
| POST | `/v1/transmissions/{id}/activate` | 签名（接收方） | ✓ | 用证据点亮 |
| POST | `/v1/transmissions/{id}/revoke` | 签名（任一方） | — | 撤回关系 |
| GET | `/v1/transmissions/{id}` | 签名（当事方） | — | 查询状态 |
| POST | `/v1/usage/attest` | 签名 | ✓ | 提交 E3/E4 证据 |
| GET | `/v1/light/paths` | 签名 | — | 我的光路与回响 |
| GET | `/v1/light/public/{anonId}` | 无 | — | 已授权的公开传播边 |
| GET | `/v1/events/stream` | 签名 | — | SSE 回响 |
| POST | `/v1/privacy/consent` | 签名 | — | 授权范围 |
| POST | `/v1/privacy/revoke` | 签名 | — | 删除可关联数据 |
| GET | `/n/{tagId}` | 无 | — | NFC 光石入口（只读查找） |
| GET | `/v1/field/summary` | 无 | — | F4 兼容占位 |

### 错误模型

```json
{ "error": { "code": "STATE_CONFLICT", "message": "…", "retryable": false } }
```

| HTTP | code |
|---|---|
| 400 | `INVALID_SCHEMA`（含**禁止字段**命中） |
| 401 | `SIGNATURE_INVALID` / `NONCE_REPLAYED` / `CLOCK_SKEW` / `EPOCH_RETIRED` |
| 403 | `FORBIDDEN` / `SELF_TRANSMISSION` / `IDENTITY_REVOKED` |
| 404 | `TRANSMISSION_NOT_FOUND` / `EVIDENCE_NOT_FOUND` |
| 409 | `STATE_CONFLICT` / `SESSION_TAKEN` / `EVIDENCE_PENDING` / `EVIDENCE_PRECEDES_RELATION` |
| 410 | `TRANSMISSION_EXPIRED` |
| 422 | `NOT_COUNTABLE`（E0–E2）/ `FAMILY_UNKNOWN` |
| 429 | `RATE_LIMITED` |

---

## 3. 传光状态机

### 3.1 状态

`CREATED → OFFERED → RECEIVED → (ACCEPTED) → LEARNING → ACTIVATED → ACKNOWLEDGED`
终止：`CANCELLED` / `EXPIRED` / `REJECTED` / `REVOKED`

### 3.2 与 F1 的审计结论

**没有另建一套。** F1 的实现 (`crates/light-core/src/transmission.rs`) 被逐条移植到
`services/api/src/fsm.mjs`，并由 220 条夹具（Rust 生成）双向锁定。
任何一侧改动而另一侧未跟进，测试立刻失败。

一处**语义澄清**（不是新状态）：
F1 的 `ACCEPTED` 只在转移内部瞬时存在——双方确认齐备时，同一转移内即建立 `light_edge`
并进入 `LEARNING`。因此外部可观察的状态序列是
`RECEIVED → LEARNING`，而"已接受"这一事实由 `transmission_party.confirmed_at_ms`
与 `EdgeFormed` 副作用独立表达。这满足授权书"双方确认必须独立表示"的要求。

### 3.3 授权书要求的事实 → 数据载体

| 必须独立表达的事实 | 载体 |
|---|---|
| 邀请创建 | `transmission` 行（state=OFFERED） |
| 对方打开接收页 | `transmission_log` 的 `OPEN` 记录 + state=RECEIVED |
| 双方确认 | `transmission_party.confirmed_at_ms`（INITIATOR / RECEIVER 各一行） |
| 来光到达 | `light_edge` state=`FORMED` |
| AI 使用发生 | `usage_event` 行（E3/E4） |
| 传播激活 | `light_edge` state=`ACTIVATED` + `activated_at_ms` |
| 回响已投递/待投递 | `outbox_event.delivered_at_ms` 为空 = 待投递 |

### 3.4 边界与异常

| 情形 | 行为 |
|---|---|
| 重复提交 | `Idempotency-Key` 命中 → 返回首次结果 |
| 重复确认 | 状态机 replays（`replayed: true`），不新增关系 |
| 并发确认 | 乐观锁 `where version = ?`，落败方 `STATE_CONFLICT` |
| 过期后 accept | `410 TRANSMISSION_EXPIRED` |
| 自我传光 | `403 SELF_TRANSMISSION` |
| 会话被他人抢领 | `409 SESSION_TAKEN` |
| 无证据激活 | `404 EVIDENCE_NOT_FOUND` |
| 证据早于关系 | `409 EVIDENCE_PRECEDES_RELATION` |
| 重复激活 | `already_activated: true`，不重复计数、不重复回响 |

---

## 4. 证据声明协议

`POST /v1/usage/attest`

```json
{
  "event_id": "01J8Z3K9…",
  "model_family": "gpt",
  "evidence_level": "E3",
  "proof_method": "local_sqlite_metadata",
  "adapter_id": "codex-app",
  "adapter_version": "1.0.0",
  "registry_semver": "1.0.0",
  "observed_at_ms": 1761868800000
}
```

服务端**不接受**也**不存储**：原始数据库、聊天内容、提示词、API Key、token 数、文件路径。
命中任一禁止字段 → `400 INVALID_SCHEMA`（测试 T14）。

规则：
- `evidence_level` 只接受 `E3`/`E4`，其它 → `422 NOT_COUNTABLE`
- `model_family` 必须是注册表 slug 形状；未登记字符串不得伪装成家族
- 同一 `(identity, family)` 只计一次（数据库唯一约束）
- **E3 是客户端自证，不等于第三方签名的 E4。** 二者在 `usage_event.evidence_level` 中分开存储，
  报告与界面上也分开呈现；没有任何代码把 E3 升级成 E4。

---

## 5. SSE 事件协议

`GET /v1/events/stream`，签名鉴权。

```
event: hello
data: {"protocol_version":"v1","resumed_from":null,"heartbeat_ms":15000,
       "replay_window_ms":300000,"rate_limit_per_minute":30,"oldest_event_id":1}

id: 1
event: edge.activated
data: {"edge_id":"…","kind":"FIRST_LIGHT","message":"你传出的一束光，已经被点亮。"}

event: heartbeat
data: 1761868815000
```

| 要求 | 实现 |
|---|---|
| 事件唯一 ID | `outbox_event.id`（bigserial，单调），同时作为 SSE `id:` |
| 授权订阅 | 签名鉴权；只投递 `identity_id` 匹配的事件 |
| 断线重连 | 客户端回 `Last-Event-ID` 或 `?lastEventId=` |
| 心跳 | 15 s（`FL_SSE_HEARTBEAT_MS`） |
| 去重 | 事件 ID 单调，客户端按 ID 去重；服务端 `delivered_at_ms` 记录 |
| 过期边界 | 超过 `replay_window_ms` 的旧事件不当作实时重播 |
| 速率限制 | 每流 ≤30 条/分钟；超出跳过而非积压 |
| 未在线用户 | 恢复后 `GET /v1/light/paths` 的 `echoes` 数组给出未读回响 |

**回响内容克制**：只有一句「你传出的一束光，已经被点亮。」+ 关系类型 + edge id。
不含接收者 anon id、位置、模型使用细节（测试 T7 断言无 identity 泄漏）。

---

## 6. 传播类型

| 类型 | 判定时机 | 含义 | 计入传光称号 |
|---|---|---|---|
| `FIRST_LIGHT` | 关系建立时接收方**没有任何**可计数使用 | 该光种首次确认的 AI 使用 | 是（默认） |
| `NEW_DISCOVERY` | 关系建立时接收方已有使用 | 传光后首次确认**新家族** | 可配置（默认是） |
| `RESONANCE` | 双方此前已有相同家族的已验证使用 | 两个已点亮的人相遇 | **否**（永远） |

- `FIRST_LIGHT` **不声称**知道某人在现实中是否用过 AI，只说明该光种首次确认。
- 同一 `(helper, helped)` 受数据库唯一约束保护，同一接收节点不会被重复奖励。
- `countNewDiscoveryAsTransmission` 是独立配置项（`FL_COUNT_NEW_DISCOVERY`）。

---

## 7. 幂等与唯一约束

| 事件 | 保证 |
|---|---|
| 邀请创建 | 每日上限 + 令牌 32 字节随机，`token_hash` 唯一 |
| 接受/确认/激活 | `Idempotency-Key` + 状态机 replay + `version` 乐观锁 |
| 使用证据 | `unique(identity_id, model_family)` |
| 传播关系 | `unique(helper_id, helped_id)` + `unique(transmission_id)` |
| 回响 | 每次激活恰好一条 outbox 行（激活本身幂等） |
| 签名重放 | `request_nonce` 主键 |
