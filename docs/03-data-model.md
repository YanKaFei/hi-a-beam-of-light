# 03 · 数据库架构

> 目标：让「分享不损耗」「撤回可兑现」「聚合不可逆但可解释」这三条承诺在表结构上成立，
> 而不是靠业务代码自觉。

设计前提（来自 [00 审计](00-environment-audit.md)）：
生产 = PostgreSQL 16+；测试与本地 = PGlite（**同一份迁移 SQL**，实测可跑）；
因此**不使用任何 PG 扩展**（含 PostGIS）——地理不存坐标，只存行政区码。

---

## 1. ER 总览

```
identity ──1:N── identity_key_epoch
   │
   ├──1:N── usage_event        (可计数事实，唯一键 identity+family)   ← 影响主色
   ├──1:N── evidence_observation (不可计数观察，可删)                  ← E0/E1/E2
   ├──1:N── transmission       (作为发起者)
   ├──1:N── light_edge         (作为 helper / helped)
   ├──1:N── consent
   └──1:N── presence           (短时在线，聚合后可删)

registry_version ──1:N── model_family ──1:N── model_alias
                                                  │
usage_event.model_family ─────────────────────────┘

transmission ──1:1── light_edge          (一次会话最多产出一条传播边)
transmission ──1:N── transmission_party  (发起者/接收者各自的确认签名)

region (层级树) ──1:N── region_aggregate (窗口化计数，无 identity)
field_snapshot (对外发布口径快照)

nonce / idempotency_key / outbox_event / audit_log  (基础设施)
```

---

## 2. 身份（M1）

```sql
create table identity (
  id              uuid primary key default gen_random_uuid(),
  anon_id         text        not null unique,        -- base32(blake3(pubkey)[0..16])
  pubkey          bytea       not null unique,        -- Ed25519, 32 bytes
  key_epoch       int         not null default 1,
  status          text        not null default 'ACTIVE'
                    check (status in ('ACTIVE','REVOKED')),
  -- 地区：仅粗粒度行政区码，永不存经纬度
  region_code     text        references region(code) on delete set null,
  region_source   text        check (region_source in ('user_selected','ip_coarse')),
  created_at      timestamptz not null default now(),
  registered_at   timestamptz,                        -- 首次与服务端交互
  revoked_at      timestamptz
);

create table identity_key_epoch (
  identity_id  uuid not null references identity(id) on delete cascade,
  epoch        int  not null,
  pubkey       bytea not null,
  created_at   timestamptz not null default now(),
  retired_at   timestamptz,                           -- 迁移后旧 epoch 作废
  primary key (identity_id, epoch)
);
```

**为什么 `anon_id` 与 `pubkey` 都存**：`anon_id` 用于对外展示与索引（不可逆哈希），
`pubkey` 用于验签。二者不能互相推导，避免拿到 anon_id 就能验证某人签名。

**为什么地区在 identity 上**：地区是"人"的属性（用户主动选择），不是事件的属性。
事件不携带地点，天然满足「不展示个人定位点」。

---

## 3. 模型注册表（M2）—— 数据，不是代码

```sql
create table registry_version (
  id          serial primary key,
  semver      text not null unique,
  applied_at  timestamptz not null default now(),
  changelog   text not null
);

create table model_family (
  slug            text primary key,                   -- 'gpt','claude','deepseek','qwen','gemini','ollama-local'
  display_name    text not null,
  provider        text not null,
  registry_id     int  not null references registry_version(id),
  introduced_at   timestamptz not null default now(),
  deprecated_at   timestamptz
);

create table model_alias (
  id            serial primary key,
  raw_pattern   text not null,                        -- 精确字符串或前缀模式
  family_slug   text references model_family(slug),   -- disposition=FAMILY 时必填
  disposition   text not null
                  check (disposition in ('FAMILY','TASK_LABEL','UNKNOWN')),
  match_kind    text not null check (match_kind in ('EXACT','PREFIX','CLIENT_PREFIXED')),
  priority      int  not null default 100,            -- 数值越小越先匹配；长别名优先级更高
  registry_id   int  not null references registry_version(id),
  note          text,
  unique (raw_pattern, registry_id)
);
create index on model_alias (priority, length(raw_pattern) desc);
```

**硬规则（对应 [G-02](02-requirements-traceability.md)）**：

1. `disposition='FAMILY'` 才允许写入 `usage_event`。
2. `TASK_LABEL`（如 `codex-auto-review`）永不计数，仅可作为「已发现」的中性展示。
3. 未命中任何别名 → 归一化函数返回 `UNKNOWN`，**调用方不得自行推断**。
4. `match_kind='CLIENT_PREFIXED'` 用于 `chatgpt-web/gpt-6-sol` 这类把客户端嵌进模型名的源，
   解析为 `client=chatgpt-web` + 其余部分再走别名表。

---

## 4. 使用证据（M2 / §5.3）

两级分离是本设计的核心：**能计数的事实** 与 **只能展示的观察** 是两张表。

```sql
-- 4.1 可计数事实：影响主色与全球统计
create table usage_event (
  id              uuid primary key,                   -- 客户端生成，天然幂等
  identity_id     uuid not null references identity(id) on delete cascade,
  model_family    text not null references model_family(slug),
  evidence_level  smallint not null check (evidence_level between 3 and 4),  -- 只收 E3/E4
  proof_method    text not null
                    check (proof_method in
                      ('local_sqlite_metadata','client_receipt_api',
                       'hook_callback','signed_adapter_report')),
  adapter_id      text not null,
  adapter_version text not null,
  observed_at     timestamptz not null,               -- 证据发生时间（客户端时钟）
  received_at     timestamptz not null default now(), -- 服务端接收时间
  registry_semver text not null,                      -- 归一化时使用的注册表版本
  -- 去重：同一光种、同一家族，全局只存在一条
  unique (identity_id, model_family)
);
create index on usage_event (model_family, received_at);

-- 4.2 不可计数观察：E0/E1/E2，只用于本机展示，永不进入全球统计
create table evidence_observation (
  id            uuid primary key,
  identity_id   uuid not null references identity(id) on delete cascade,
  raw_model     text,                                 -- 原始字符串，可为空
  model_family  text references model_family(slug),   -- 可能为 UNKNOWN(NULL)
  client_id     text,
  evidence_level smallint not null check (evidence_level between 0 and 2),
  adapter_id    text not null,
  observed_at   timestamptz not null,
  received_at   timestamptz not null default now()
);
create index on evidence_observation (identity_id);
```

**注意 `evidence_level between 3 and 4` 这个 CHECK**：它让「E2 混进可信统计」在数据库层就不可能发生，
而不是靠服务端代码记得过滤。这是把 §5.3 变成不可绕过的约束。

**`tokens_used` 不出现在任何表里**（[G-15](02-requirements-traceability.md)）——
消费量既不参与计算也不进入服务端。

---

## 5. 传光（M4）

```sql
create table transmission (
  id                uuid primary key default gen_random_uuid(),
  initiator_id      uuid not null references identity(id) on delete cascade,
  token_hash        bytea not null unique,            -- 只存短时令牌哈希，明文不落库
  channel           text not null check (channel in ('qr','nfc','link','nearby')),
  state             text not null default 'CREATED'
                      check (state in ('CREATED','OFFERED','RECEIVED','ACCEPTED',
                                       'LEARNING','ACTIVATED','ACKNOWLEDGED',
                                       'CANCELLED','EXPIRED','REJECTED','REVOKED')),
  version           int  not null default 1,          -- 乐观并发：防双端同时确认打架
  expires_at        timestamptz not null,
  created_at        timestamptz not null default now(),
  updated_at        timestamptz not null default now(),
  check (expires_at > created_at)
);
create index on transmission (initiator_id, state);
create index on transmission (expires_at) where state not in ('CANCELLED','EXPIRED','REJECTED','REVOKED');

create table transmission_party (
  transmission_id uuid not null references transmission(id) on delete cascade,
  role            text not null check (role in ('INITIATOR','RECEIVER')),
  identity_id     uuid references identity(id),        -- RECEIVER 在扫码前可为空
  confirmed_at    timestamptz,
  signature       bytea,                               -- 对会话摘要的签名
  primary key (transmission_id, role)
);

-- 传播关系：会话成功后的一等公民
create table light_edge (
  id              uuid primary key default gen_random_uuid(),
  transmission_id uuid not null unique references transmission(id),
  helper_id       uuid not null references identity(id) on delete cascade,
  helped_id       uuid not null references identity(id) on delete cascade,
  state           text not null check (state in ('FORMED','ACTIVATED','REVOKED')),
  visibility      text not null default 'PRIVATE'
                    check (visibility in ('PRIVATE','ANONYMOUS_PUBLIC')),
  created_at      timestamptz not null default now(),
  activated_at    timestamptz,                         -- 受助者首次可信使用
  acknowledged_at timestamptz,                         -- helper 收到回响
  revoked_at      timestamptz,
  -- 同一对 helper/helped 只保留一条有效边（G-06 的去重基础）
  unique (helper_id, helped_id)
);
create index on light_edge (helper_id, state);
create index on light_edge (helped_id, state);
```

**`unique (helper_id, helped_id)`** 解决两件事：同一对用户重复触碰不叠加称号；
`helper_id` 的行数就是「成功激活节点数」的直接来源，称号计算不需要复杂去重。

---

## 6. 地区与聚合（M5 / §8）

```sql
create table region (
  code        text primary key,                        -- 'CN' | 'CN-ZJ' | 'CN-ZJ-HZ'
  level       text not null check (level in ('COUNTRY','ADMIN1','CITY')),
  parent_code text references region(code),
  display_name text not null
);

create table region_aggregate (
  region_code        text   not null references region(code),
  window_start       timestamptz not null,
  window_end         timestamptz not null,
  active_nodes       int    not null default 0,        -- 累计有效点亮节点（自愿参与）
  new_activations    int    not null default 0,        -- 窗口内新增激活事件
  online_instances   int    not null default 0,        -- 短时心跳去重实例（≠自然人）
  contributing_nodes int    not null default 0,        -- 用于 k-匿名判定
  suppressed         boolean not null default false,   -- 低于阈值 → 展示上级
  display_region     text references region(code),     -- 实际对外展示的区码
  computed_at        timestamptz not null default now(),
  primary key (region_code, window_start, window_end)
);

create table field_snapshot (
  id            serial primary key,
  generated_at  timestamptz not null default now(),
  payload       jsonb not null,                        -- 对外发布口径的完整快照
  methodology   text not null,                         -- 口径说明文本（必须随数据发布）
  cache_seconds int not null default 60
);
```

**聚合表里没有任何 identity 列**——这是 [G-09](02-requirements-traceability.md) 的物理保证：
即使管理员想「从聚合反查个人」，表结构上做不到。

**k-匿名阈值 20**（§8.3）：`contributing_nodes < 20` 时 `suppressed=true`，
`display_region` 指向上级；对外响应只返回 `suppressed=false` 或用上级替代。
阈值放在配置表，不硬编码。

---

## 7. 同意与撤回（§12.2）

```sql
create table consent (
  identity_id  uuid not null references identity(id) on delete cascade,
  scope        text not null
                 check (scope in ('GLOBAL_FIELD_CONTRIBUTION',
                                  'PUBLIC_LIGHT_PATH')),
  state        text not null check (state in ('GRANTED','DECLINED','REVOKED')),
  policy_version text not null,
  changed_at   timestamptz not null default now(),
  primary key (identity_id, scope)
);

-- 每次同意变更留痕，用于「可解释」
create table consent_history (
  id          bigserial primary key,
  identity_id uuid not null references identity(id) on delete cascade,
  scope       text not null,
  from_state  text,
  to_state    text not null,
  policy_version text not null,
  at          timestamptz not null default now()
);
```

**默认值语义**：表里没有行 = 从未授权 = `DECLINED`。
这满足 §5.4「询问是否加入匿名全球光场（默认不贡献）」。

---

## 8. 基础设施表

```sql
-- 防重放：签名请求的 nonce 只可用一次
create table nonce (
  identity_id uuid not null references identity(id) on delete cascade,
  nonce       bytea not null,
  seen_at     timestamptz not null default now(),
  expires_at  timestamptz not null,
  primary key (identity_id, nonce)
);
create index on nonce (expires_at);

-- 幂等：同一 Idempotency-Key 返回首次结果
create table idempotency_key (
  key          text primary key,
  identity_id  uuid not null references identity(id) on delete cascade,
  endpoint     text not null,
  request_hash bytea not null,
  response     jsonb,
  status_code  int,
  created_at   timestamptz not null default now(),
  expires_at   timestamptz not null
);

-- 领域事件：SSE / NOTIFY 的可靠来源（先写库，再广播）
create table outbox_event (
  id          bigserial primary key,
  kind        text not null,          -- 'usage.attested' | 'edge.formed' | 'edge.activated'
  identity_id uuid,                   -- 可空：聚合类事件无主体
  region_code text,
  payload     jsonb not null,
  created_at  timestamptz not null default now(),
  published_at timestamptz
);
create index on outbox_event (published_at) where published_at is null;

-- 隐私相关操作的审计（不含内容，只含动作）
create table audit_log (
  id          bigserial primary key,
  identity_id uuid,
  action      text not null,          -- 'consent.grant' | 'consent.revoke' | 'identity.revoke' | 'export'
  detail      jsonb,
  at          timestamptz not null default now()
);
```

---

## 9. 本地 SQLite（客户端，离线可用）

本地库是一切体验的真源；服务端是可选副本。

```sql
create table local_identity (
  singleton   integer primary key check (singleton = 1),
  anon_id     text not null,
  pubkey      blob not null,
  key_epoch   integer not null default 1,
  created_at  integer not null
);  -- 私钥不在此表：存 OS 凭据库

create table local_evidence (
  id              text primary key,          -- = 上报用 event_id
  family_slug     text not null,
  raw_model       text,
  client_id       text,
  evidence_level  integer not null,          -- 0..4
  proof_method    text,
  adapter_id      text not null,
  observed_at     integer not null,
  upload_state    text not null default 'LOCAL_ONLY'
                    check (upload_state in ('LOCAL_ONLY','QUEUED','UPLOADED','REJECTED'))
);

create table local_light_state (
  singleton     integer primary key check (singleton = 1),
  family_count  integer not null default 0,  -- 仅 E3/E4
  attested_count integer not null default 0, -- E2 单独计
  received_glow integer not null default 0,  -- 来光影响外环
  updated_at    integer not null
);

create table local_edge (
  edge_id      text primary key,
  role         text not null check (role in ('HELPER','HELPED')),
  peer_anon_id text not null,
  state        text not null,
  visibility   text not null default 'PRIVATE',
  created_at   integer not null,
  activated_at integer
);

create table local_consent (
  scope   text primary key,
  state   text not null,
  policy_version text not null,
  changed_at integer not null
);

create table outbox (
  id         integer primary key autoincrement,
  kind       text not null,
  payload    text not null,
  created_at integer not null,
  attempts   integer not null default 0,
  next_try_at integer
);
```

`local_light_state` 里 **`family_count` 与 `attested_count` 分开**，
正是 [G-05](02-requirements-traceability.md)「E2 不计入主色但可见」的落地。

---

## 10. 保留期与撤回语义（把 §12.2 变成操作）

| 数据 | 保留 | 撤回时 |
|---|---|---|
| `usage_event` | 直至撤回 | **硬删除** |
| `evidence_observation` | 90 天 | 硬删除 |
| `light_edge` | 直至撤回 | 硬删除；对方侧仅保留「一条匿名已撤回边」 |
| `transmission` | 30 天后仅留状态与时间（去 token_hash） | 立即清 `token_hash` |
| `nonce` / `idempotency_key` | 24 小时 | 自然过期 |
| `region_aggregate` | 永久 | **不可逆**，但**重算**受影响窗口 |
| `audit_log` | 365 天 | 保留动作记录（不含内容），用于证明「确实删了」 |

**撤回流程**：删原始事实 → 标记受影响 `(region, window)` 为待重算 → 重算 → 返回
「已删除 N 条个人事件；历史聚合快照不可逆」这一明确文案（对应 A12）。

---

## 11. 一致性约束自查（必须在测试中覆盖）

| 约束 | 落地手段 |
|---|---|
| E2 不进可信统计 | `usage_event.evidence_level` CHECK 3..4 |
| 同家族只计一次 | `unique (identity_id, model_family)` |
| 同一对用户只一条边 | `unique (helper_id, helped_id)` |
| 会话不可复用 | `transmission.token_hash unique` + 状态机 |
| 双端并发确认 | `transmission.version` 乐观锁 |
| 聚合无法反查个人 | `region_aggregate` 无 identity 列 |
| 不存精确位置 | 全库无 lat/lng 列，只有 `region_code` |
