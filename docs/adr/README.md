# 架构决策记录（ADR）

每个 ADR 只记录一件事：**在什么约束下、选了什么、放弃了什么、什么时候该重新考虑。**
决策一旦落地，代码与测试引用的就是 ADR 编号，而不是散落在评论里的理由。

| ADR | 标题 | 对应决议 | 状态 |
|---|---|---|---|
| [ADR-001](ADR-001-adapter-column-allowlist.md) | 适配器证据边界必须是列级 allowlist | D-04 | 已采纳，已实现并有测试 |
| [ADR-002](ADR-002-registry-lookup-only.md) | 模型家族只查表，绝不字符串推断 | F0 G-02 | 已采纳，已实现并有测试 |
| [ADR-003](ADR-003-postgres-and-pglite.md) | 生产 Postgres，测试/本地 PGlite，同一 DDL | F0 | 已采纳（F1 未涉及服务端） |
| [ADR-004](ADR-004-sse-realtime.md) | 实时下行用自建 SSE | D-07 | 已采纳，协议已定义 |
| [ADR-005](ADR-005-mobile-stack-deferred.md) | 移动端技术栈推迟到 F5 | D-06 | 已采纳 |
| [ADR-006](ADR-006-neutral-codename.md) | 代码用中性代号 `firstlight` | D-05 | 已采纳 |
| [ADR-007](ADR-007-repository-location.md) | 仓库位于 `first-light/`，独立 git | D-01 | 已采纳，已执行 |
| [ADR-008](ADR-008-identity-recovery-inheritance.md) | 光种恢复继承 + 跨平台验证接口 | D-02 | 已采纳，已实现并有测试 |
| [ADR-009](ADR-009-e2-not-counted.md) | E2 自主确认不计入可信称号 | D-03 | 已采纳，已实现并有测试 |
| [ADR-010](ADR-010-node-identity.md) | Identity / Device / Node 的定义与密钥模型 | D-02 | 已采纳，已实现并有测试 |
| [ADR-011](ADR-011-evidence-gates.md) | E3 需要「归属」+「发生」双闸门 | F1 §2.1 | 已采纳，已实现并实测 |
| [ADR-012](ADR-012-wal-read-only.md) | 活跃 WAL 数据库的只读打开策略 | F1 实测发现 | 已采纳，已实现并有测试 |

## 编号约定

- ADR-001 至 ADR-006 源于 F0 审计，详细论证在 [01-architecture.md](../01-architecture.md) §4；
  本目录中的文件是正式收编，不重复全部论证。
- ADR-007 至 ADR-012 源于 F1 授权与实测，是本目录的正文。
- 代码引用 ADR 的方式：`// see docs/adr/ADR-011-evidence-gates.md`。

## 什么情况下要新增 ADR

- 改变数据结构的语义（例如新增一个证据等级）
- 改变「什么算真实使用」的判定
- 改变隐私边界（可读范围、可上传字段）
- 引入一个外部依赖，且它承载了不可替换的职责
