# M1 凭证 Schema（`fl.usage.credential/v1`）

## 1. 字段

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `protocol_version` | string | 是 | 必须等于 `fl.usage.credential/v1` |
| `credential_id` | string | 是 | ≤128，`[A-Za-z0-9_.:#-]`，全局唯一 |
| `issuer_key_id` | string | 是 | 必须是该身份已注册的 `device_key.key_id` |
| `subject_identity_id` | string | 是 | 必须等于调用者 anon_id |
| `source_type` | enum | 是 | 四种来源之一 |
| `adapter_id` | string | 是 | `^[a-z0-9][a-z0-9._-]{0,63}$` |
| `adapter_version` | string | 是 | ≤32 |
| `model_family` | string | 是 | 注册表 slug，`^[a-z0-9][a-z0-9_-]{0,40}$` |
| `observation_method` | string | 是 | ≤64，如 `http_response_observed` |
| `observed_at` | integer | 是 | 毫秒，不得晚于 `issued_at` + 时差 |
| `issued_at` | integer | 是 | 毫秒，距今 ≤10 分钟 |
| `challenge_id` | string | 否 | 传光相关证据必填 |
| `transfer_id` | string | 否 | 仅传光相关证据 |
| `signature` | string | 是 | 128 位 hex（Ed25519） |

**禁止出现**：提示词、响应正文、聊天记录、第三方 API Key、恢复种子。
未知字段直接 422 `UNKNOWN_FIELD`——静默忽略会隐藏 bug。

## 2. 规范化与签名

- 规范化：RFC 8785 JCS 的**受限子集**（只允许 string / integer / boolean / null，禁止浮点与数组）。
  这是明确限制，不是"实现了 JCS"。
- 签名输入：`"firstlight/usage-credential/v1\n" + canonical_json`。
  域分隔符让凭证签名不能冒充 epoch 证书或其它业务签名。
- 算法：Ed25519，通过 Node `crypto` 与 Rust `ed25519-dalek`，不自实现。

## 3. 版本与兼容

- 版本号在字段里，服务端遇到未知版本直接拒绝（422），不做猜测式兼容。
- 新增可选字段 → 升次版本并更新 conformance 向量；删除/改语义 → 升主版本。
- 旧的无签名 `usage_event` 行保持原样，不会被回填成 v1 凭证。

## 4. 跨语言向量

`packages/protocol/vectors/credential-v1.json`：固定私钥 → 固定 payload → canonical bytes → 签名。

- Node：`services/api/test/credential.test.mjs` 校验 canonical 与签名
- Rust：`crates/light-core/tests/credential_vector.rs` 校验同一段字节上的签名、域分隔、键序
- **Rust 目前没有 JCS 实现**，它验证的是"签名覆盖了双方约定的字节"，不是"Rust 能重算字节"。
  这个差别写在测试文件头，避免被当成完整互操作。
