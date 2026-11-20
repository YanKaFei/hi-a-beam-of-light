# 04 · API 契约（v1）

> 契约原则：**匿名可看、签名才能写、每个写操作都可重放而不重复计数。**
> 所有响应必须可被第三方复现验证——这是开源协议的前提。

- Base：`https://api.<domain>/v1`
- 传输：HTTPS/TLS 1.3；JSON（UTF-8）；时间一律 RFC 3339 UTC
- 版本策略：路径内 `v1` 冻结；破坏性变更走 `/v2`，`v1` 至少并存 12 个月

---

## 1. 鉴权：Ed25519 请求签名

无 Cookie、无 Bearer token、无会话。每个请求自带签名。

| 请求头 | 内容 |
|---|---|
| `X-FL-Identity` | `anon_id`（base32） |
| `X-FL-Epoch` | 密钥轮次 |
| `X-FL-Timestamp` | Unix 毫秒 |
| `X-FL-Nonce` | 16 字节随机，base64url |
| `X-FL-Signature` | base64url(Ed25519) |

签名原文（`\n` 分隔，无尾随换行）：

```
FL1
<METHOD>
<PATH_WITHOUT_QUERY>
<sha256_hex(body)>
<timestamp>
<nonce>
```

**校验顺序**：时间窗 ±120s → nonce 未用过（写入 `nonce` 表）→ 取 `identity_key_epoch` 对应公钥验签。
任一步失败返回 `401`，且 **nonce 一旦见过即拒绝**（防重放）。

> 为什么不用 JWT：JWT 需要密钥分发与吊销列表，而本项目每个客户端本来就持有长期私钥；
> 直接签名更少状态、更少被偷令牌的风险。

---

## 2. 幂等

所有产生副作用的 `POST` **必须**携带 `Idempotency-Key: <uuid>`。

- 同 key + 同 `request_hash` → 返回首次结果（含原 status code）
- 同 key + 不同 body → `409 IDEMPOTENCY_CONFLICT`
- 保留 24 小时

---

## 3. 错误模型

```json
{ "error": { "code": "TRANSMISSION_EXPIRED", "message": "会话已过期",
             "retryable": false, "request_id": "req_01H..." } }
```

| HTTP | code | 场景 |
|---|---|---|
| 400 | `INVALID_SCHEMA` | 不符合协议 schema |
| 401 | `SIGNATURE_INVALID` / `NONCE_REPLAYED` / `CLOCK_SKEW` | 鉴权失败 |
| 403 | `CONSENT_REQUIRED` | 未授权贡献却调用写接口 |
| 404 | `TRANSMISSION_NOT_FOUND` | 会话不存在或已失效（**不区分二者**，防枚举） |
| 409 | `STATE_CONFLICT` | 状态机不允许的转移（含并发确认） |
| 409 | `IDEMPOTENCY_CONFLICT` | 同 key 不同体 |
| 410 | `TRANSMISSION_EXPIRED` | 超时后访问 |
| 422 | `FAMILY_UNKNOWN` | 写入了 `UNKNOWN` 家族 |
| 429 | `RATE_LIMITED` | 见 §6 |

---

## 4. 端点总表

| # | 方法 | 路径 | 鉴权 | 幂等 | 用途 |
|---|---|---|---|---|---|
| 1 | POST | `/identities/challenge` | 无 | — | 取挑战 nonce（注册前） |
| 2 | POST | `/identities/register` | 挑战签名 | 必需 | 注册公钥 |
| 3 | POST | `/identities/rotate` | 签名 | 必需 | 迁移/轮换密钥（G-10） |
| 4 | POST | `/usage/attest` | 签名 | 必需 | 提交 E3/E4 证据 |
| 5 | POST | `/usage/observe` | 签名 | 必需 | 提交 E0–E2 观察（不入统计） |
| 6 | POST | `/transmissions` | 签名 | 必需 | 发起传光会话 |
| 7 | GET | `/transmissions/{id}` | 签名（发起者） | — | 查会话状态 |
| 8 | GET | `/t/{token}` | 无 | — | 接收页入口（扫码/NFC） |
| 9 | POST | `/transmissions/{id}/accept` | 签名 | 必需 | 接收方确认 |
| 10 | POST | `/transmissions/{id}/confirm` | 签名（发起者） | 必需 | 发起方确认交付 |
| 11 | POST | `/transmissions/{id}/reject` | 签名 | 必需 | 接收方拒绝 |
| 12 | POST | `/transmissions/{id}/cancel` | 签名（发起者） | 必需 | 发起方取消 |
| 13 | POST | `/transmissions/{id}/activate` | 签名（接收方） | 必需 | 上报首次可信使用 |
| 14 | POST | `/transmissions/{id}/revoke` | 签名（任一方） | 必需 | 撤回传播关系 |
| 15 | GET | `/light/me` | 签名 | — | 我的光点状态与双称号 |
| 16 | GET | `/light/paths` | 签名 | — | 我的光路（出/入/代际/回响） |
| 17 | GET | `/field/regions` | **无** | — | 区域聚合（免注册观看） |
| 18 | GET | `/field/summary` | **无** | — | 三类口径数字 + 方法说明 |
| 19 | GET | `/events/stream` | 可选 | — | SSE 光脉 |
| 20 | POST | `/presence/heartbeat` | 签名 | 必需 | 在线心跳（参与贡献时） |
| 21 | POST | `/privacy/consent` | 签名 | 必需 | 授权/拒绝/撤回 |
| 22 | GET | `/privacy/export` | 签名 | — | 导出我的全部数据 |
| 23 | POST | `/privacy/revoke` | 签名 | 必需 | 删除可关联数据 |
| 24 | GET | `/registry/latest` | **无** | — | 分发模型注册表（带签名） |

**#17 #18 #19 #24 无需身份**——直接兑现 §2.2「不创建账户即可查看聚合光场」。

---

## 5. 关键端点详述

### 5.1 POST `/usage/attest` —— 整个产品最敏感的写接口

请求：

```json
{
  "event_id": "01J8Z3K9...",
  "model_family": "gpt",
  "evidence_level": 3,
  "proof_method": "local_sqlite_metadata",
  "adapter_id": "codex-app",
  "adapter_version": "1.0.0",
  "registry_semver": "2026.11.1",
  "observed_at": "2026-10-31T09:12:00Z",
  "client_id": "codex"
}
```

服务端行为：

1. 校验 `model_family` 存在于注册表且 `disposition='FAMILY'`，否则 `422 FAMILY_UNKNOWN`。
2. `insert ... on conflict (identity_id, model_family) do nothing`。
3. **命中冲突 = 已点亮过** → 返回 `200 { "counted": false, "family_count": N }`，不报错、不重复计数。
4. 新插入 → `201 { "counted": true, "family_count": N, "light_level": "微明", "title": "初明者" }`。
5. 写 `outbox_event(kind='usage.attested')`。

响应里直接返回**新的称号与光阶**，让客户端不必自行计算（避免两端算法漂移）。
客户端本地仍保留自己的计算，用于离线态，二者由 fixtures 对齐。

> 服务端**不接收** `tokens_used`、调用次数、消费金额、提示词、客户端路径。
> 请求体 schema 用 `additionalProperties: false` 强制这一条（[G-15](02-requirements-traceability.md)）。

### 5.2 POST `/transmissions` —— 发起传光

```json
{ "channel": "qr", "ttl_seconds": 600 }
```

```json
{ "id": "…", "token": "urlsafe-32B",       // 仅此一次返回明文
  "receive_url": "https://fl.example.com/t/<token>",
  "nfc_payload": "https://fl.example.com/t/<token>",
  "expires_at": "2026-10-31T09:22:00Z" }
```

- 明文令牌**只在创建响应里出现一次**，库里只存 `sha256(token)`。
- `nfc_payload` 就是写进 NDEF 标签的字符串（§7.5：标签只含公开 HTTPS 入口，无密钥无隐私）。

### 5.3 GET `/t/{token}` —— 接收页（无 App 也能开）

- 200 → 返回最小页面数据：发起者匿名代号、会话剩余时间、`state`。
- 不返回发起者任何身份信息、地区或历史。
- 令牌一次性；`ACCEPTED` 之后再访问返回 `410`。

### 5.4 POST `/transmissions/{id}/activate`

```json
{ "usage_event_id": "01J8Z3K9..." }
```

服务端校验（缺一不可）：

1. 调用者是该会话的 `RECEIVER`。
2. 会话 `state='LEARNING'`。
3. 该 `usage_event` 存在、属于该 identity、且 `evidence_level ∈ {3,4}`。
4. 该 event 的 `received_at` **晚于** `light_edge.created_at`（防止用历史使用冒充激活）。
5. `light_edge` 尚未 `ACTIVATED`。

→ `light_edge.state='ACTIVATED'`，`activated_at=now()`，写 `outbox(edge.activated)`。
**重复调用返回 200 且 `already_activated: true`，不重复计数**（A10「只计一次」）。

### 5.5 GET `/field/summary` —— 三类数字必须分开

```json
{
  "generated_at": "2026-10-31T09:20:00Z",
  "cache_seconds": 60,
  "metrics": {
    "total_activated_nodes": { "value": 1284, "window": "lifetime",
      "note": "已自愿参与且至少一次可信激活的唯一匿名节点" },
    "online_instances": { "value": 37, "window": "90s",
      "note": "短时心跳去重实例数，不等同于自然人" },
    "new_activations_today": { "value": 12, "window": "2026-10-31T00:00:00Z/PT24H",
      "note": "受助者首次激活并建立双方认可传播关系的次数" },
    "helper_nodes": { "value": 213, "note": "至少帮助 1 个节点完成激活的匿名节点数" }
  },
  "methodology_url": "https://fl.example.com/methodology",
  "suppressed_regions": 14
}
```

**字段级强制**：三个指标**不得合并成一个「总点数」**（§8.2）。每个指标必须带 `window` 与 `note`；
`window` 与 `note` 为必填，缺任一则视为接口违约，由客户端渲染失败而非静默省略。

### 5.6 GET `/field/regions`

```
GET /field/regions?level=COUNTRY&window=7d
```

```json
{ "level": "COUNTRY", "window": {"start":"…","end":"…"},
  "regions": [
    { "code": "CN", "name": "中国", "active_nodes": 412,
      "new_activations": 9, "density_band": "high", "suppressed": false,
      "children_suppressed": 6 }
  ],
  "k_threshold": 20 }
```

- **永不返回**经纬度、城市以下粒度、个人点。
- `density_band` 是为可视化准备的离散档位（`none/low/mid/high`），不是精确坐标——
  这样即使接口被滥用也无法还原真实分布。
- `suppressed=true` 的地区直接不出现在数组里，其计数并入上级。

### 5.7 GET `/events/stream`（SSE）

```
GET /events/stream?since=<event_id>&region=CN
Accept: text/event-stream
```

```
id: 84213
event: pulse
data: {"kind":"far_light","region":"CN-ZJ","band":"mid","at":"…"}
retry: 30000
```

规则（对应 [G-17](02-requirements-traceability.md)）：

- **服务端限频**：单连接 ≤2 条/分钟；超出的合并为一条 `kind:"aggregate"`，附 `merged_count`。
- **不得**把历史事件重播成新事件（§8.5）。`since` 超出保留窗口时返回 `410` + `resync: true`，
  客户端重新拉 `/field/summary` 而不是假装实时。
- 匿名连接（无签名）只收 `band` 级信息，不收具体区码以外的事件明细。

---

## 6. 限流

| 端点 | 每 identity | 每 IP | 备注 |
|---|---|---|---|
| `/usage/attest` | 30/时 | 60/时 | 正常用户远低于此 |
| `/transmissions` | 20/日 | 40/日 | 反刷量（[G-11](02-requirements-traceability.md)） |
| `/transmissions/*/activate` | 10/日 | 20/日 | 冷却 24h 后才计入称号 |
| `/identities/register` | — | 5/日 | 防批量造身份 |
| `/field/*` | — | 120/分 | 可缓存 |
| `/events/stream` | — | 2 并发 | |

超限返回 `429` + `Retry-After`。**限流不得泄露他人存在性**（404 与 429 的取舍需保持一致语义）。

---

## 7. 注册表分发（§5.2「必须可配置、版本化」的落地）

`GET /registry/latest` →

```json
{ "semver": "2026.11.1",
  "issued_at": "2026-10-31T00:00:00Z",
  "families": [ {"slug":"gpt","provider":"openai","display":"GPT"} ],
  "aliases":  [ {"raw":"gpt-6-sol","family":"gpt","disposition":"FAMILY","match":"EXACT"},
                {"raw":"codex-auto-review","family":null,"disposition":"TASK_LABEL","match":"EXACT"} ],
  "signature": "ed25519:…" }
```

- 客户端用**内置固定的公钥**验签；验签失败则继续用内置版本，绝不信任未签名数据。
- 这样修正「新模型名没被识别」不需要发版，只需要发布注册表。
- 但**禁止**远程关闭/开启某个家族的能力——注册表只描述映射，不承载策略。

---

## 8. 契约测试要求

| 测试 | 内容 |
|---|---|
| Schema 一致性 | `packages/protocol/fixtures/*.json` 同时被 TS 与 Rust 测试读取 |
| 幂等重放 | 同 Idempotency-Key 连发 10 次 → 计数只 +1 |
| 重放攻击 | 复用 nonce → 401 `NONCE_REPLAYED` |
| 并发确认 | 双端同时 accept → 一成一败（409），状态不损坏 |
| 历史冒充激活 | 用 `received_at` 早于边的 E3 事件调用 activate → 409 |
| 隐私字段 | 对每个写接口注入 `tokens_used`/`prompt` → 400 `INVALID_SCHEMA` |
| 聚合泄露 | `/field/regions` 响应中不得出现 lat/lng 字段 |
