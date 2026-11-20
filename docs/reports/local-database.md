# 本地数据库设计（已实现）

实现：`crates/light-store`（schema v1，迁移已实现并有测试）
服务端 schema 见 [03-data-model.md](../03-data-model.md)

---

## 1. 定位

本地库是一切体验的真源；服务端是可选副本。

**不变量**：本地库存在时，「我的光」完全成立——不需要登录、不需要网络、
不需要服务端同意。这对应产品文档 §1.3「可仅本地使用」。
测试 `the_app_is_fully_usable_without_any_network` 断言这一点。

```
用户使用 AI ──► 适配器产出证据 ──► 本地 countable_evidence ──► 光点变深（立即可见）
                                          │
                                          └─► outbox（可选上报，失败不影响本地）
```

---

## 2. 表结构（实际 DDL）

### 2.1 `local_identity`
```sql
create table local_identity (
    singleton     integer primary key check (singleton = 1),
    anon_id       text    not null,
    pubkey_hex    text    not null,
    key_epoch     integer not null default 1,
    state         text    not null default 'LOCAL_ONLY',
    created_at_ms integer not null
);
```
- 单行表（`singleton = 1` 约束），防止出现「两个身份」。
- **私钥不在此表**：私钥在 OS 凭据库。这里只存公钥与 epoch。
- `state` 取值对应身份生命周期（`LOCAL_ONLY` / `REGISTERED` / `REVOKED`）。

### 2.2 `countable_evidence` —— 唯一影响光阶的表
```sql
create table countable_evidence (
    family          text primary key,          -- ← 去重的核心
    event_id        text not null,
    level           integer not null check (level in (3,4)),
    proof_method    text not null,
    adapter_id      text not null,
    adapter_version text not null,
    registry_semver text not null,
    observed_at_ms  integer not null,
    upload_state    text not null default 'LOCAL_ONLY'
);
```
两个关键设计：

1. **`family` 是主键**，不是 `(event_id)`。
   ChatGPT 与 Codex 都用 GPT → 两条证据、同一个 family → 第二次写入是 no-op。
   测试 `one_family_counts_exactly_once` 断言「返回 false 且计数不变」。
2. **`check (level in (3,4))`**：E0/E1/E2 在存储层就写不进来。
   应用层若试图存入 E2，`record_countable_evidence` 直接返回错误。

### 2.3 `attested_family` —— E2 独立表
```sql
create table attested_family (
    family        text primary key,
    attested_at_ms integer not null
);
```
**为什么单独一张表**：如果 E2 与 E3 同表只靠 `level` 区分，
任何一个忘记过滤的查询都会把自主确认算进光阶。
分成两张表后，`verified_family_count()` 物理上查不到 E2。

测试 `e2_never_changes_the_light_level`：7 个 E2 家族 → 等级仍 `Dormant`、称号仍「寻光者」。

### 2.4 `observation` —— E0/E1，仅展示
```sql
create table observation (
    id text primary key, raw_model text, client_id text,
    level integer not null check (level between 0 and 1),
    adapter_id text not null, observed_at_ms integer not null
);
```
`check (level between 0 and 1)`：观察表永远无法承载 E2 及以上。

### 2.5 `received_glow` —— 来光
单行表，只影响外环。测试 `received_light_does_not_change_the_core`。

### 2.6 `local_edge` —— 传播关系
```sql
create table local_edge (
    edge_id text primary key,
    role text not null check (role in ('HELPER','HELPED')),
    peer_anon_id text not null,
    state text not null,
    visibility text not null default 'PRIVATE',
    created_at_ms integer not null,
    activated_at_ms integer
);
```
`visibility` 默认 `PRIVATE`：传播边默认不公开（产品文档 §12.2）。

### 2.7 `consent` —— 授权
```sql
create table consent (
    scope text primary key,
    state text not null check (state in ('GRANTED','DECLINED','REVOKED')),
    policy_version text not null,
    changed_at_ms integer not null
);
```
**缺行 = 未授权 = 不贡献**。测试 `consent_defaults_to_not_granted` 断言初始为 `None`，
符合产品文档 §5.4「默认不贡献」。

### 2.8 `outbox` —— 离线队列
```sql
create table outbox (
    id integer primary key autoincrement,
    kind text not null, payload text not null,
    created_at_ms integer not null,
    attempts integer not null default 0,
    uploaded_at_ms integer
);
```
上报失败不影响本地状态；`attempts` 支持退避。

---

## 3. 迁移机制

使用 SQLite 内建的 `PRAGMA user_version`：

```rust
let current: i64 = conn.query_row("pragma user_version", [], ...)?;
if current > SCHEMA_VERSION { return Err(StoreError::FutureSchema { .. }); }
if current < 1 { /* apply v1 in a transaction, set user_version = 1 */ }
```

- **幂等**：重复执行无副作用（测试 `migrations_are_idempotent` 连跑三次）。
- **向前不兼容时拒绝启动**：`FutureSchema` 错误，绝不降级破坏用户数据
  （测试 `future_schema_is_refused_rather_than_corrupted`）。
- 迁移在**事务内**执行，失败不留半套 schema。

---

## 4. 与服务端 schema 的对应关系

| 本地 | 服务端 | 说明 |
|---|---|---|
| `local_identity` | `identity` + `identity_key_epoch` | 服务端多存 epoch 历史用于验签 |
| `countable_evidence` | `usage_event` | 二者都有 `unique(identity,family)` 语义 |
| `attested_family` | `evidence_observation` | 服务端同样只用于展示 |
| `observation` | `evidence_observation` | 同上 |
| `local_edge` | `light_edge` + `transmission` | 服务端有状态机与双方确认 |
| `consent` | `consent` + `consent_history` | 服务端留变更历史 |
| `outbox` | — | 仅客户端 |

本地库刻意**不存**：凭证、原始模型行、token 数、路径、聊天内容。

---

## 5. 保留与删除（F1 已定义，F3 实现服务端侧）

| 数据 | 本地保留 | 删除入口 |
|---|---|---|
| `countable_evidence` | 永久（这是用户的成长记录） | 用户主动删除本机记录 |
| `observation` | 建议 90 天 | 同上 |
| `outbox` | 上报成功后清理 | 同上 |
| `attested_family` | 永久 | 同上 |

---

## 6. 性能与占用（F1 未实测，F2 必须实测）

本地库规模预期极小：家族 ≤ 数十、观测 ≤ 数千、边 ≤ 数十。
理论上 < 1 MB。**F2 必须在真机上确认**，并纳入产品文档 §14.3 的
「常驻内存 < 120 MB、CPU < 1%」验收。
