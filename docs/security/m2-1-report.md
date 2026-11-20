# M2.1 交付报告：设备身份、跨端配对与挑战桥接

**结论：配对协议与身份绑定通过 8 项测试；正式激活仍然关闭；真实 Ollama 仍 BLOCKED。**

---

## 1. 证据语义（修正 M2 的表述）

| 标签 | 本环境状态 |
|---|---|
| `MOCK_UPSTREAM_COMPLETION` | ✅ 13 项插件测试 |
| `REAL_OLLAMA_COMPLETION` | **BLOCKED**（无 ollama、无守护进程，未下载模型） |
| `LOCAL_PLUGIN_OBSERVATION` | ✅ |
| `SIGNATURE_VALID` | ✅ |
| `SOURCE_POLICY_APPROVED` | ✅（政策允许 PLUGIN_OBSERVED 类别） |
| `ACTIVATION_ELIGIBLE` | ❌ **默认配置下仍不成立**（见 §5） |

M2 报告里"通过 13 项测试"指的是模拟上游，**不是真实推理**。这里更正。

## 2. 设备身份

一次性设计，不引入第二套身份系统：

```
设备密钥对（Ed25519，本机生成，私钥不出机器）
  → POST /v1/devices/register      登记公钥，状态 PENDING，无主体
  → POST /v1/pairing/sessions      服务端发短时配对码（5 分钟）
  → POST /v1/pairing/sessions/:id/approve   手机端（已登录身份）用码批准
  → POST /v1/pairing/sessions/:id/confirm   设备对会话签名，证明持有私钥
  → 绑定：device_key.subject_identity_id = 批准者身份，状态 ACTIVE
```

- 配对码只存哈希；`ABCD-EFGH` 去掉易混字符。
- **双端指纹**：`pairingFingerprint()` 由 `session_id + 设备公钥` 派生，两端显示同一串，用于人工核对。
- 二维码/配对码**不含任何长期私钥**。
- 撤销：`POST /v1/devices/:deviceId/revoke`，只有主体本人可调用。

## 3. 挑战桥接

传光激活挑战仍由 `/v1/challenges` 签发（绑定 transfer + 接收方 + `purpose='usage'` + 15 分钟）。
插件在配对后以绑定主体身份持有设备密钥，签出的凭证 `issuer_key_id` 指向该设备。

**关键边界没有放宽**：凭证正确绑定 transfer 与 challenge，但来源仍是
`SIGNED_SELF_CLAIM` 时，服务端依然返回 `403 SOURCE_NOT_TRUSTED`。测试 1 就是这个断言。

## 4. 新增测试（8 项，全绿）

| # | 断言 |
|---|---|
| 1 | 正确 transfer + 正确 challenge + 未可信来源 → **仍禁止激活** |
| 2 | 设备不能把别人的身份当作 subject → 422 |
| 3/4/5 | 未配对设备保持 PENDING；撤销后 `status_v2=REVOKED` |
| 6 | 过期配对码 → 410 `PAIRING_EXPIRED` |
| 7 | 已消费的配对会话重放 → 409 `REPLAYED` |
| 9 | 6 个并发批准 → 恰好 1 个成功 |
| 10 | 签名证明错误 → 403，且设备仍为 PENDING（无部分绑定） |
| 11/12 | 默认配置下不存在遗留激活捷径（426） |

## 5. 为什么正式激活仍然关闭

准入矩阵没有为"M2 插件产生了真实观察"开例外：

- `PLUGIN_OBSERVED` 在政策上允许激活，但**激活还需要**挑战绑定 + 凭证绑定 + 关系状态，
  且**没有独立佐证来源**时，公共光场仍为 0。
- `INDEPENDENTLY_CORROBORATED` 依然没有达成路径。
- 我在测试 1 里刻意构造了"绑定全对、来源不够"的情形，服务端拒绝——这是本轮最重要的负向证据。

## 6. 本轮修掉的缺陷

1. `device_key.identity_id` 是 NOT NULL，而配对前设备本无主体 → 注册直接失败。已放宽。
2. `app.mjs` 缺 `randomBytes` 导入 → 配对会话创建静默 500。
3. 配对码长度校验把短 ID 挡在外面（测试用 ID 太短，已改；规则本身保留）。

## 7. 未解决 / 阻断项

| 项 | 状态 |
|---|---|
| REAL_OLLAMA_COMPLETION | **BLOCKED**（环境无 ollama） |
| 插件 CLI 的 subject 绑定 | 仍用占位，需接配对流程（工具已就绪，未接线） |
| 挑战下发到插件 | 服务端已签发，插件端尚未实现领取与注入 |
| 手机端（小程序）实现 | 未开始，仅有契约 |
| 发布包签名与校验 | 未做 |
| 设备密钥丢失后的恢复 | 未设计 |

## 8. M3 契约（不在本轮实现）

见 `docs/architecture/m3-miniprogram-contract.md`：六页状态文案已按可信等级分层，
没有单一 VERIFIED 标记；未配对/未观察/未获批准各有明确空态与下一步。

## 9. 回归

服务端现代套件 **38/38**（凭证 13 + M1.1 14 + M2 3 + M2.1 8）、
插件 **13/13**、Rust **199**、协议 12（本轮未改动，沿用）。
