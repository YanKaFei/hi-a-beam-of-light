# M1 交付报告

完成标准：**系统不会把合法签名的虚假声明误判为可信 AI 使用。**
结论：**达成**，并有专门的测试（A6）与准入矩阵兜底。

---

## 1. 交付物对照

| # | 要求 | 位置 |
|---|---|---|
| 1 | 协议设计文档 | `docs/security/m1-credential-schema.md` |
| 2 | 威胁模型 | `docs/security/m1-threat-model.md` |
| 3 | 签名凭证 Schema | 同上 + `services/api/src/credential.mjs` |
| 4 | 公钥注册与撤销 | `POST /v1/keys/register`、`/v1/keys/revoke`（`FL_VERIFIER_V2`） |
| 5 | 服务端验证器 | `services/api/src/credential.mjs` + `POST /v1/usage/attest-v2` |
| 6 | 准入矩阵 | `docs/security/m1-admission-matrix.md` |
| 7 | 跨语言向量 | `packages/protocol/vectors/credential-v1.json`（Node 签名 / Rust 校验） |
| 8 | 攻击与重放测试 | `services/api/test/credential.test.mjs`（13 项） |
| 9 | 迁移与回滚 | `docs/security/m1-migration-rollback.md` + `002_credentials.sql` |
| 10 | 跨端身份决策 | `docs/architecture/identity-across-devices.md` |
| 11 | 小程序实时通信 | `docs/architecture/miniprogram-realtime.md` |
| 12 | 未解决的真实性限制 | 威胁模型 §3 |

## 2. 验收测试（13 项，全绿）

| 用例 | 断言 |
|---|---|
| A1 正确签名 | 201，`signature_status=VALID`，来源 PLUGIN_OBSERVED |
| A2 字段篡改（4 字段） | 一律不接受 |
| A3 非法家族名 | 422 `BAD_FIELD` |
| A4 未注册密钥 | 422 `KEY_UNKNOWN` |
| A5 已撤销密钥 | `signature_status=REVOKED`，不保存 |
| **A6 攻击者自签 GPT** | **签名 VALID，但 `activate=false`、`public_field=false`** |
| A7 谎报 source_type | 仍受挑战绑定约束 |
| B1 重放 | 幂等，不重复计数 |
| B2 并发 6 次 | 恰好 1 行插入 |
| B3 过期观察 | `CREDENTIAL_TOO_OLD` |
| B4 subject 不匹配 | 422 `SUBJECT_MISMATCH` |
| C1 规范化 | 键序稳定，拒绝浮点 |
| C2 未知字段 | 422 `UNKNOWN_FIELD`（含 `prompt_text`） |

## 3. 回归

| 套件 | 结果 |
|---|---|
| Rust | **199 全绿**（含 4 项跨语言向量） |
| 服务端（V2 开） | **42 全绿** |
| 协议一致性 | **12 全绿** |
| 服务端（V2 关，回滚） | **21 全绿** |

## 4. 本轮修掉的真问题

1. `002_credentials.sql` 没被 `db.mjs` 登记 → 新表不存在，全部测试 500。
2. `verifyCredential` 的 `limits` 未给默认值 → 任何调用都抛异常。
3. 非法凭证原本**会被写库**（只标 INVALID）→ 改为不入库：
   表里的无效行是给后来所有读者的陷阱。
4. 形状错误原本 500 → 改为 422，客户端错误不该是服务器故障。

## 5. 仍然存在的限制（不粉饰）

- **旧路径 `/v1/usage/attest` 仍接受客户端自述 E3/E4**，与新路径可信度不一致，建议 M2 前处理。
- **公钥注册不证明代码未改**，没有可信构建链路。
- **身份无法恢复**（只能撤销密钥）。
- **`INDEPENDENTLY_CORROBORATED` 没有达成路径** → 公共光场实际上被冻结，这是刻意选择。
- **挑战绑定尚未接到激活路径**：`/activate` 目前仍只检查关系状态，
  尚未强制要求"凭证 + 已消费挑战"。这是 M1 的已知缺口，列入 M1.1。

## 6. 下一步（需授权）

M1.1：把凭证资质接进 `/activate`，补"挑战消费 + 凭证一次性"的端到端测试。
M2：Ollama 插件（仅在 M1.1 通过后）。

微信小程序、Android、F4 均未启动。
