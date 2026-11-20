# 模块接口文档（F1）

> 所有接口都在 `crates/` 中实现并有测试覆盖。纯核心（`light-core`）不含 I/O；
> 需要外部世界的部分被隔离在 `adapter-sdk`（读他方数据库）与 `light-store`（本地库）里。

依赖方向（严格单向，见 [01-architecture](../01-architecture.md) §3）：

```
light-core ──► adapter-sdk ──► adapter-codex
     │                            ▲
     └────────────────────────────┘
     └──► light-store
```

`light-core` 不依赖任何其他 crate。`adapter-*` 不依赖 `light-store`，反之亦然。

---

## 1. `light-core`

### 1.1 `codec` —— 无依赖编码

```rust
pub fn hex_encode(bytes: &[u8]) -> String
pub fn base32_encode(data: &[u8]) -> String
pub fn base32_decode(s: &str) -> Option<Vec<u8>>
```
自实现而非引第三方：这是稳定线格式的一部分，不应随依赖升级漂移。

### 1.2 `ids` —— 匿名标识

```rust
pub fn anon_id_from_root_pubkey(root_pubkey: &[u8]) -> String   // "fl1_" + 32 hex
pub fn is_valid_anon_id(candidate: &str) -> bool                // 结构校验，非证明
```

### 1.3 `identity` —— 身份与密钥

```rust
pub struct RootIdentity;                       // 不实现 Clone；Debug 恒定脱敏
impl RootIdentity {
    pub fn generate() -> Self;
    pub fn from_seed(seed: [u8; 32]) -> Self;  // 测试与恢复用
    pub fn anon_id(&self) -> String;           // 跨设备、跨 epoch 稳定
    pub fn epoch_public_key(&self, epoch: u32) -> VerifyingKey;
    pub fn epoch_certificate(&self, epoch: u32) -> EpochCertificate;
    pub fn recovery_code(&self) -> RecoveryCode;
    pub fn sign_with_epoch(&self, epoch: u32, msg: &[u8]) -> Signature;
}

pub struct RecoveryCode;
impl RecoveryCode {
    pub fn as_str(&self) -> &str;
    pub fn to_identity(&self) -> Result<RootIdentity, CoreError>;   // 校验和
}

pub fn verify_epoch_certificate(anon_id: &str, cert: &EpochCertificate) -> Result<(), CoreError>;
```
`verify_epoch_certificate` 是服务端唯一需要的身份校验入口：证书自包含。

### 1.4 `registry` —— 模型家族字典

```rust
pub enum Disposition { Family { family: String }, TaskLabel { reason: String }, Unknown }
pub struct Normalization { raw, family, client, disposition, matched_rule, registry_semver }

impl ModelRegistry {
    pub fn bundled() -> Self;
    pub fn from_json(json: &str) -> Result<Self, CoreError>;   // 加载即校验
    pub fn normalize(&self, raw_model: &str) -> Normalization; // 永不猜
    pub fn family(&self, slug: &str) -> Option<&Family>;
    pub fn semver(&self) -> &str;
}
```

### 1.5 `evidence` —— 证据等级与双闸门

```rust
pub enum EvidenceLevel { E0, E1, E2, E3, E4 }
impl EvidenceLevel { pub fn is_countable(self) -> bool; }   // 仅 E3/E4

pub struct ResponseProof { output_items: u32, completed_turns: u32, output_item_types: Vec<String> }
impl ResponseProof { pub fn proves_call(&self) -> bool; }   // 只看 output_items > 0

pub enum EvidenceDecision {
    Countable   { family, level, normalization },
    Observable  { level, reason, normalization },
    Rejected    { reason, normalization },
}

impl EvidencePolicy {
    pub fn v1() -> Self;
    pub fn evaluate(&self, raw_model: &str, proof: &ResponseProof) -> EvidenceDecision;
    pub fn count_verified_families<'a, I>(&self, it: I) -> BTreeSet<String>;
    pub fn classifies_as_output_item(&self, item_type: &str) -> bool;
}
```

### 1.6 `light` —— 光点状态与双称号

```rust
pub fn light_level(verified_families: u32) -> LightLevel;
pub fn knowledge_title(verified_families: u32) -> KnowledgeTitle;
pub fn transmission_title(activated_nodes: u32) -> TransmissionTitle;
pub fn palette(level: LightLevel) -> Palette;

pub fn compute_light_state(
    verified_families: u32,   // 唯一影响 level / knowledge_title 的输入
    attested_families: u32,   // E2，仅展示
    received_glow: u32,       // 来光，仅外环
    activated_nodes: u32,     // 传光称号
) -> LightState;
```
阈值表 `KNOWLEDGE_LADDER` / `TRANSMISSION_LADDER` 是唯一事实来源，
且被导出到共享夹具，由另一语言独立实现对照。

### 1.7 `transmission` —— 会话状态机

```rust
pub struct Session { state, initiator_confirmed, receiver_confirmed, version, expires_at_ms }
impl Session {
    pub fn new(expires_at_ms: i64) -> Self;
    pub fn apply(&mut self, event: SessionEvent, ctx: &Context) -> Result<Transition, FsmError>;
}

pub struct Context { now_ms, expires_at_ms, activation_evidence_ok }
pub struct Transition { state, effects: Vec<Effect>, replayed: bool }
pub enum Effect { TokenInvalidated, EdgeFormed, EdgeActivated, EchoQueued, EdgeRevoked }
```
纯函数：不读时钟、不写库。`activation_evidence_ok` 由调用方（服务端）独立核验后传入。

### 1.8 `node` —— 节点记账

```rust
pub struct NodeBook;
impl NodeBook {
    pub fn attach_device(&mut self, device_id: &str, anon_id: &str) -> Result<(), NodeBookError>;
    pub fn node_count(&self) -> usize;          // 去重后的节点数
    pub fn device_count(&self) -> usize;
    pub fn record_activation(&mut self, helper: &str, helped: &str);
    pub fn activated_node_count(&self, helper: &str) -> u32;
}
```
**没有**「合并两个光种为同一人」的 API，因为该证据不存在。

### 1.9 `protocol` —— 线格式与契约

```rust
pub const PROTOCOL_VERSION: &str = "v1";
pub const SSE_HEARTBEAT_MS / SSE_MAX_EVENTS_PER_MINUTE / SSE_REPLAY_WINDOW_MS / CLOCK_SKEW_MS;

pub struct UsageAttestRequest;      // deny_unknown_fields
pub struct UsageAttestResponse;
pub struct TransmissionCreateRequest / Response;
pub struct TransmissionActivateRequest / TransmissionStateResponse;
pub struct FieldMetric / FieldSummary;     // 四个指标各自带 window + note
pub struct RegionEntry / FieldRegionsResponse;
pub struct FieldEvent / StreamHello;

pub fn exported_schemas() -> BTreeMap<String, serde_json::Value>;
```
所有请求结构 `deny_unknown_fields`：多余字段是**反序列化错误**，
所以「夹带 token 数 / 提示词」在类型层面不可能成功（有测试）。

---

## 2. `adapter-sdk` —— 唯一受控的数据访问层

```rust
pub struct AdapterManifest {
    id, version, platforms, tables: Vec<TablePolicy>,
    evidence_capability, max_rows, timeout_ms,
}
impl AdapterManifest { pub fn validate(&self) -> Result<(), ManifestError>; }

pub struct TablePolicy { table, allow: Vec<String>, deny: Vec<String> }
pub const GLOBAL_DENY_COLUMNS: &[&str];   // 任何 manifest 都不可覆盖

pub struct GuardedDb;   // 内部 Connection 私有，无访问器
impl GuardedDb {
    pub fn open(path: &Path, manifest, options: OpenOptions) -> Result<Self, GuardError>;
    pub fn open_readonly(...) -> ...;    // 严格只读
    pub fn open_compatible(...) -> ...;  // 允许 WAL 兼容模式（仍然不可写）
    pub fn open_mode(&self) -> OpenMode; // 如实上报用了哪种策略
    pub fn rows(&mut self, RowsSpec) -> Result<Vec<Row>, GuardError>;
    pub fn count(&mut self, table, Option<Filter>) -> Result<i64, GuardError>;
    pub fn group_count(&mut self, GroupCountSpec) -> Result<Vec<(Value, i64)>, GuardError>;
    pub fn audit(&self) -> &QueryAudit;  // 只含列名，不含值
}

pub enum OpenMode { ReadOnly, QueryOnly }
pub enum GuardError { Manifest, TableNotAllowed, ColumnNotAllowed, GloballyForbidden,
                      RowBudgetExceeded, TimeBudgetExceeded, Open, Sql, InvalidLimit }
```

设计约束：
- **没有**任何接受原始 SQL 的入口。
- 拒绝后不重试、不放宽（`a_refusal_never_widens_the_scan` 测试）。
- 行数/时间预算用尽即失败——因为**部分扫描会给出错误的光点等级**（ADR-012）。

---

## 3. `adapter-codex`

```rust
pub fn manifest() -> AdapterManifest;      // 3 表 7 列，测试锁定
pub struct CodexPaths { state_db: PathBuf, history_db: Option<PathBuf> }
pub fn scan(paths: &CodexPaths, policy: &EvidencePolicy) -> Result<ScanReport, ScanError>;

pub struct ScanReport {
    adapter_id, adapter_version, registry_semver,
    state_open_mode: OpenMode, history_open_mode: Option<OpenMode>,
    verified_families: Vec<String>,
    counts: ScanCounts,
    sessions: Vec<SessionDecision>,        // session_ref 是摘要，不是原始 id
    audit: QueryAudit,                     // 不序列化
}
```

---

## 4. `light-store` —— 本地 SQLite

```rust
pub const SCHEMA_VERSION: i64 = 1;

pub struct LocalStore;
impl LocalStore {
    pub fn open(path: &Path) -> Result<Self>;
    pub fn in_memory() -> Result<Self>;
    pub fn set_identity(&mut self, anon_id, pubkey_hex, key_epoch, now_ms) -> Result<()>;
    pub fn record_countable_evidence(&mut self, &CountableEvidence) -> Result<bool>; // false = 已计过
    pub fn verified_family_count(&self) -> Result<u32>;
    pub fn record_attested_family(&mut self, family, now_ms) -> Result<()>;          // E2，另一张表
    pub fn light_state(&self, activated_nodes: u32) -> Result<LightState>;
    pub fn enqueue(&mut self, kind, payload, now_ms) -> Result<i64>;
    pub fn pending(&self, limit) -> Result<Vec<OutboxRow>>;
    pub fn set_consent / consent_state;    // 缺省 = 未授权（不贡献）
}
```
