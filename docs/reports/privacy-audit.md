# 隐私审计报告（F1）

状态：**TESTED**（21 项专项测试 + 适配器端到端断言）
审计对象：`crates/adapter-sdk`、`crates/adapter-codex`、`crates/light-core`、`crates/light-store`
决策记录：[ADR-001](../adr/ADR-001-adapter-column-allowlist.md)、[ADR-011](../adr/ADR-011-evidence-gates.md)、[ADR-012](../adr/ADR-012-wal-read-only.md)

---

## 1. 授权书 §2.2 的九条要求，逐条核对

| # | 要求 | 落地方式 | 测试证据 | 结论 |
|---|---|---|---|---|
| 1 | 读取使用只读连接 | `SQLITE_OPEN_READ_ONLY` + `PRAGMA query_only=1`；WAL 场景降级为 `query_only`（仍不可写） | `connection_is_read_only`、`query_only_mode_still_cannot_write` | ✅ |
| 2 | 查询必须显式列出允许字段 | 由 `RowsSpec.columns` 拼装，无通配；`Connection` 私有无访问器 | `select_star_is_structurally_impossible` | ✅ |
| 3 | 禁止 `SELECT *` | SQL 由代码生成，星号只可能出现在 `count(*)`；另有 `has_no_bare_star` 断言 | 同上 | ✅ |
| 4 | 禁止失败后自动扩大扫描 | 无重试路径；文件缺失即失败；拒绝只记录不重试 | `a_refusal_never_widens_the_scan`、`opening_a_missing_file_fails_without_searching_for_alternatives` | ✅ |
| 5 | 不上传本机原始数据库内容 | 上报类型 `CountableEvidence` 只含 8 个字段（无原始行、无路径、无 token） | `countable_evidence_serializes_without_sensitive_fields`、`scan_report_leaks_no_content_and_no_raw_session_ids` | ✅ |
| 6 | 调试日志不得含敏感字段值 | 审计只记录表名与列名；`GuardedDb`/`RootIdentity` 手写 `Debug` | `audit_records_names_never_values` | ✅ |
| 7 | 敏感列自动化负向测试 | 21 项测试，覆盖内容/凭据/位置/路径/消费量 5 类 | 见 §2 | ✅ |
| 8 | 适配器只能通过受审接口取证据 | 唯一入口 `GuardedDb`；`adapter-codex` 不持有 `rusqlite::Connection` | `adapter_touches_only_allowlisted_columns` | ✅ |
| 9 | 限时、限量、增量，避免影响使用 | `max_rows=20000`、`timeout_ms=15000`，超限即失败；单次 group_count 批量处理 | `row_budget_stops_a_runaway_scan`、`group_count_answers_per_session_item_counts` | ✅ |
| 10 | 未授权不得扫私人目录或读聊天文件 | 路径由调用方给出，适配器不搜索、不遍历目录；manifest 白名单 3 张表 | `manifest_cannot_allow_conversation_columns` 等 | ✅ |

---

## 2. 全局禁读清单（任何 manifest 都不可覆盖）

`adapter-sdk::GLOBAL_DENY_COLUMNS`，按禁止原因分组：

### 2.1 对话内容（产品文档 §12.3）
`first_user_message` `item_json` `message_json` `messages` `preview`
`feedback_log_body` `prompt` `prompts` `text` `content` `body`
`conversation` `transcript` `title`

### 2.2 凭据与密钥
`api_key` `apikey` `authorization` `token` `access_token` `refresh_token`
`secret` `password` `cookie` `cookies`

### 2.3 个人联系与精确位置
`email` `phone` `address` `latitude` `longitude`

### 2.4 暴露用户项目的文件系统信息
`rollout_path` `file_path` `cwd` `agent_path` `git_origin_url`

### 2.5 消费量计数（产品文档 §1.5）
`tokens_used` `tokens` `cost` `usage_cost`

> 第 5 类是本项目主动加严的一类：产品文档只禁止"按消费额评定人的价值"，
> 我们直接**禁止读取该字段**。省掉一列读取，就少一处泄漏面。

---

## 3. 三层防御

```
① manifest 校验（加载时）   ← 违反全局禁读清单的 manifest 根本无法构造
        ↓
② 查询期列校验（每次）      ← 未在 allow 中的列被拒绝并留痕
        ↓
③ SQL 生成约束（结构性）    ← 星号无法表达、标识符经过白名单比对
```

三层都是「不可能」而非「不应该」：

- 第 ① 层：`manifest.validate()` 在 `GuardedDb::open` 第一步执行，
  非法 manifest 连连接都拿不到。
- 第 ② 层：`check_column` 在拼 SQL **之前**执行，越界直接返回 `GloballyForbidden`。
- 第 ③ 层：`Connection` 是 `GuardedDb` 的私有字段，无任何方法返回它或接受原始 SQL。

---

## 4. 负向测试清单（21 项）

| 测试 | 尝试的攻击 | 结果 |
|---|---|---|
| `manifest_cannot_allow_conversation_columns` | 8 个内容列逐个尝试加入 manifest | 全部拒绝 |
| `manifest_cannot_allow_consumption_counters` | `tokens_used` 等 4 个 | 全部拒绝 |
| `manifest_cannot_allow_secrets_or_contact_data` | 密钥/邮箱/经纬度 7 个 | 全部拒绝 |
| `manifest_cannot_allow_filesystem_detail` | `cwd` 等 4 个 | 全部拒绝 |
| `manifest_rejects_contradiction_and_duplicates` | allow/deny 自相矛盾、重复表、零预算 | 全部拒绝 |
| `invalid_identifiers_are_rejected` | 列名注入 `id; drop table threads` | 拒绝 |
| `reading_a_sensitive_column_is_refused_and_recorded` | 直接 SELECT 5 个敏感列 | 拒绝 + 留痕 5 条 |
| `reading_a_column_outside_the_allowlist_is_refused` | 读取未声明列、未声明表 | 拒绝 |
| `filtering_on_a_forbidden_column_is_refused` | 把敏感列放进 WHERE | 拒绝 |
| `select_star_is_structurally_impossible` | 构造 `SELECT *` | 无法表达 |
| `audit_records_names_never_values` | 检查审计与 Debug 是否泄漏 6 种敏感值 | 未泄漏 |
| `a_refusal_never_widens_the_scan` | 拒绝后是否重试更大范围 | 无重试 |
| `row_budget_stops_a_runaway_scan` | 请求超过预算 | 失败而非部分返回 |
| `zero_limit_is_rejected` | limit=0 | 拒绝 |
| `opening_a_missing_file_fails_without_searching_for_alternatives` | 文件不存在 | 失败，错误信息不含路径 |
| `connection_is_read_only` | 检查 query_only | 已开启 |
| `query_only_mode_still_cannot_write` | 在 query_only 连接上 INSERT | SQLite 拒绝 |
| `live_wal_database_reports_the_strategy_it_used` | 活跃 WAL 库 | 如实上报模式 |
| `adapter_touches_only_allowlisted_columns` | 真实扫描后核对触达列 | 6 列全部在声明内 |
| `scan_report_leaks_no_content_and_no_raw_session_ids` | 检查报告 JSON | 无内容、无路径、无原始 id |
| `only_three_tables_and_seven_columns_are_reachable` | 数据面扩张检测 | 3 表 7 列被锁定 |

---

## 5. 已知残留风险（诚实记录）

| 风险 | 严重度 | 说明与缓解 |
|---|---|---|
| **内存中的值会短暂存在** | 中 | 读取允许列后，其值进入进程内存。缓解：允许列本身不含正文；不做任何落盘缓存；审计不记录值 |
| **`item_type` 是自由文本列** | 低 | 理论上客户端可把内容塞进该列。缓解：适配器对它做**集合成员判断**（`IN` 白名单），不读取展示，且只统计计数 |
| **`PRAGMA query_only` 可能被后续代码关闭** | 低 | 本 crate 内无任何关闭路径；`forbid(unsafe_code)`；连接私有。若有新代码需要关闭，必须改本 crate 并通过评审 |
| **WAL 兼容模式失去 OS 层只读** | 中 | 该模式仅在严格只读失败时启用；仍由 `query_only` 保证不可写；`OpenMode` 上报给用户可查 |
| **SQLite 版本差异** | 低 | 使用 bundled SQLite，版本随二进制固定 |
| **`row_budget` 触发的失败可能让用户看不到光点** | 低 | 属可用性而非隐私问题；表现为「暂时无法校验」而非错误数字 |

**没有发现的泄漏路径**：审计、错误信息、Debug 输出、扫描报告 JSON、
上报结构体均已断言不含内容/路径/凭据/消费量。

---

## 6. 审计结论

| 结论 | 依据 |
|---|---|
| 敏感列在结构上不可达，而非仅在约定上禁读 | §3 三层防御 |
| 「不读聊天正文」已从承诺变为可执行约束 | 21 项负向测试 |
| 上报数据不含任何原始数据库内容 | `CountableEvidence` 字段表 + JSON 断言 |
| 失败不扩大范围，超限不返回部分结果 | 拒绝路径测试 |
| 数据面（3 表 7 列）被测试锁定 | `only_three_tables_and_seven_columns_are_reachable` |

**遗留**：服务端接收端（F3）需要做对应的入库校验（`CHECK evidence_level between 3 and 4`
已在 [03-data-model](../03-data-model.md) 定义），F1 未实现服务端。
