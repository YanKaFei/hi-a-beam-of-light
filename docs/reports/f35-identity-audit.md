# 光种身份与恢复：实现与安全审计（F3.5-A）

代码：`apps/desktop/src-tauri/src/identity_store.rs`、`identity.rs`
测试：24 项（存储 4、身份 11、加密原语由 light-core 覆盖）

---

## 1. IdentityStore：为什么先抽一层

```rust
pub trait IdentityStore: Send + Sync {
    fn load_seed(&self) -> Result<Option<[u8; 32]>, StoreError>;
    fn save_seed(&self, seed: &[u8; 32]) -> Result<(), StoreError>;
    fn delete_seed(&self) -> Result<(), StoreError>;
    fn backend(&self) -> Backend;
}
```

三个实现：

| 实现 | 用途 | 选择方式 |
|---|---|---|
| `KeychainStore` | 生产（macOS Keychain） | **默认** |
| `FileStore` | 本沙箱无法写 Keychain 时的开发后端，文件 0600 | 仅当 `FIRSTLIGHT_IDENTITY_BACKEND=file` |
| `MemoryStore` | 测试 | 仅当 `=memory` |

**不存在隐式回退**：默认就是 Keychain；它失败时不会"顺便"用文件后端顶上，
而是把错误抛给 UI。这一点是刻意的——静默降级会让用户在不知情的情况下
把私钥写到比预期弱的地方。

Windows 的 Credential Manager 后端**尚未实现**。接口已就位，但代码里没有假装
它可用（非 macOS 平台目前会走 FileStore 且 `backend()` 如实报告 `FileDev`）。

---

## 2. 四种失败模式的行为

| 情形 | 行为 | 测试 |
|---|---|---|
| 记录在、私钥在 | 正常加载，校验 anon_id 一致 | `a02_*` |
| 记录在、私钥**不在** | **硬错误**，绝不生成新身份 | `a03_record_without_seed_*` |
| 记录**不在**、私钥在 | 用私钥重建记录（升级/换机场景） | `a03_missing_record_*` |
| 凭据库拒绝访问 | 明确错误，不写任何记录 | `a04_*` |
| 记录与私钥的 anon_id 不一致 | 硬错误 `IdentityConflict` | `a_seed_with_a_mismatched_record_*` |

第三条尤其重要：**"记录丢了但钥匙还在"和"钥匙丢了但记录还在"是完全不同的两件事**。
前者可以恢复，后者一旦"顺手"生成新身份，用户全部历史就永久失去了。

---

## 3. 恢复机制：方案比较与选择

### 方案 A（已采用）：可恢复密钥 + 用户持有的恢复凭据
根种子编码为 `FL1-` + base32(seed ‖ checksum)，**256 位熵**。

- 优点：无需任何服务器即可恢复；跨平台同一套编码；离线可用；
  与"无需注册，但可以找回自己的光"的目标直接一致。
- 缺点：凭据即身份，泄露等于被完全冒充；用户必须自己保管。
- 缓解：轮换（签发新 epoch，旧 epoch 退役，F1/ADR-010 已实现）；
  恢复是签发新密钥的能力，不是复制私钥。

### 方案 B（未采用）：非导出密钥 + 已授权设备配对
- 优点：私钥永不出安全区，泄露面小。
- 缺点：**丢光全部设备就永久失去身份**；需要至少一台在线设备做中介；
  跨平台配对协议复杂，且在小程序/浏览器场景无法成立。
- 与本项目目标冲突：项目承诺"可以找回自己的光"，而不是"必须保管好设备"。

### 结论
采用 A。**但不是无保护的 A**：
- 恢复凭据必须 256 位熵起（禁止六位数字、禁止弱口令）；
- 不在任何地方自动上传；
- 面板只显示一次，不写日志、不进错误报告；
- 导入时校验 checksum，且覆盖已有身份需显式 `allow_replace`。

### 仍未解决的风险（如实记录）
恢复凭据一旦被他人看到（例如截屏分享），对方可以完全冒充该光种。
目前没有任何检测手段。可做但未做的缓解：导入后提示"上次使用时间与地区"，
让用户能发现异常。这需要服务端记录最小化登录轨迹，属于 F4 之前的决策。

---

## 4. UI 接触不到私钥

- `IdentityView` 只含 `anon_id` / 公钥 / epoch / 后端名 / 是否可签名，
  **没有种子字段**；测试 `the_identity_view_never_contains_the_seed` 断言
  JSON 中不出现种子十六进制串。
- `export_recovery` 是唯一把种子"以可读形式"交出去的路径，且只在用户点击时调用。
- 日志只打印 `anon_id`，从不打印种子（`RootIdentity` 的 `Debug` 在 F1 已脱敏）。

---

## 5. 审计结论

| 结论 | 依据 |
|---|---|
| 私钥落在 OS 凭据库（生产路径） | `KeychainStore` 为默认实现 |
| 失败时不静默降级、不覆盖历史 | §2 五种情形均有测试 |
| 恢复凭据熵足够 | 256 位种子 + 校验和 |
| 恢复凭据不上传 | 无任何网络代码路径 |
| UI 拿不到私钥 | 类型 + 测试 |
| Windows 后端未实现且未伪装 | 代码中如实标注 |

**未验证**：Keychain 的**成功**路径（本沙箱写入被拒）。
A01/A02 的持久化验证使用的是 `FileDev` 后端。这一点在验收矩阵中已标注。
