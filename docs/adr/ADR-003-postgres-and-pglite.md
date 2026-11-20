# ADR-003 · 生产 Postgres，测试/本地 PGlite，同一套 DDL

- **状态**：已采纳（F1 未涉及服务端实现）
- **决议来源**：F0 环境审计（无 Docker / 无 Postgres / 无 psql）
- **论证背景**：[01-architecture.md](../01-architecture.md) §4 ADR-003、[03-data-model.md](../03-data-model.md)

## 决策

- 生产：标准 PostgreSQL 16+。
- 测试与本地：PGlite（WASM Postgres），执行**同一份**迁移 SQL。
- 因此禁止使用任何 PG 扩展（含 PostGIS）——地理只用行政区码，不存坐标。

## 后果

- 后端开发与 CI 不依赖任何容器或外部服务（F0 已实测 PGlite 支持唯一约束、事务、`ON CONFLICT`）。
- 代价：失去扩展能力。这反过来强化了隐私设计——不存经纬度就不需要 PostGIS。

## 重新考虑的条件

出现必须用扩展的查询需求时（如真正的球面距离聚合），需重新评估，
并优先考虑「粗粒度网格 + 计数」这类不依赖扩展的方案。
