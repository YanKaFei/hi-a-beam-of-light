# ADR-010 · Identity / Device / Node 的定义与密钥模型

- **状态**：已采纳，已实现（`crates/light-core/src/identity.rs`、`node.rs`），有测试
- **决议来源**：D-02、F1 授权 §2.3
- **日期**：2026-10-31

## 1. 正式定义

| 术语 | 定义 | 在代码中的对应 |
|---|---|---|
| **Identity（光种身份）** | 本地生成的根种子（32 字节），其派生公钥的哈希即匿名 ID | `RootIdentity` |
| **Device（设备）** | 一次安装 / 一个运行实例，可替换、可恢复 | `NodeBook` 的 `device_id` |
| **Node（匿名节点）** | 一个参与网络的匿名光种身份 | `anon_id`，`NodeBook::node_count()` |
| **Model Family（模型家族）** | 注册表去重后的家族 | `registry::Normalization.family` |
| **Ignition Event（有效点亮）** | 某节点的某个已验证家族**首次**被计入 | `countable_evidence` 的 `unique(family)` |
| **Transmission（传光关系）** | 两个节点间一次经双方确认的帮助关系 | `Session` 状态机 |
| **Activation（激活）** | 接收者在关系建立后完成首次**已验证**使用 | `SessionEvent::Activate` + `Effect::EdgeActivated` |

## 2. 三条硬规则

1. **同一光种恢复到多台设备 → 仍然只算 1 个 Node。**
   因为 `anon_id` 由根公钥派生，与设备无关。测试：3 台设备同一光种 → `node_count == 1`。
2. **不同光种永不合并为一个人。**
   `NodeBook` **没有**任何"合并"API（刻意的：能力不存在，就不需要纪律）。
   测试：两个光种 → 2 个节点。
3. **公共统计不得把节点数称为真实自然人数。**
   对外只表述「激活节点」。类型层面不提供 `person_count`。

## 3. 密钥模型

```
root seed (32B, OS keychain)
   ├─ anon_id     = "fl1_" + hex(SHA-256(root_pubkey)[..16])
   ├─ root key    = Ed25519(root seed)                     ← 签名授权
   └─ epoch key_n = Ed25519(SHA-256("firstlight/epoch-key/v1" ‖ seed ‖ n))  ← 请求签名
```

### 为什么匿名 ID 不直接等于公钥哈希
因为要**可轮换**。若 `anon_id = hash(epoch_pubkey)`，轮换密钥就会换身份，
多设备也会变成多个节点。用根公钥派生则两者都成立。

### 恢复码
`FL1-` + base32(seed ‖ checksum)。校验和防手误。

### 轮换与吊销（解决 F0 缺口 G-10）
`EpochCertificate` 自包含：`{epoch, root_pubkey, epoch_pubkey, root_signature}`。
服务端只需 `anon_id` 即可验证：

1. `hash(root_pubkey) == anon_id`
2. root 签名有效
3. `epoch_pubkey != root_pubkey`

因此恢复码泄漏的危害从「永久冒充」降为「可被轮换作废」：
签发新 epoch、把旧 epoch 标记 `retired_at`，旧签名一律拒绝。

## 4. 明确不做

- **不使用设备指纹**（产品文档 §9 表注、§5.4）。
- 不做「同一人」推断：两个光种之间不存在可信的"同人"证据，就不合并。
- 不在日志、错误信息、`Debug` 输出中打印种子。`RootIdentity` 手写 `Debug`，
  种子字段恒为 `<redacted>`（有测试覆盖）。

## 5. 后果

- 多设备体验成立，且不会虚增节点数。
- 代价：恢复码是单点秘密。已通过轮换机制与控制台吊销流程缓解，
  且**恢复码不用于日常签名**，只为签发 epoch 密钥。
