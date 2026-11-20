# ADR-004 · 实时光场下行使用自建 SSE

- **状态**：已采纳，协议常量与事件结构已实现（`crates/light-core/src/protocol.rs`）
- **决议来源**：D-07
- **论证背景**：[01-architecture.md](../01-architecture.md) §2.4、[04-api-contract.md](../04-api-contract.md) §5.7

## 背景

V1.0 需要「远方有人点亮 → 本机收到光脉」。这是**服务端 → 客户端单向事件**。

## 决策

自建 SSE（Server-Sent Events），不使用 WebSocket，不使用 Supabase Realtime。

协议必须定义（均已落地为常量与结构）：

| 项 | 取值 / 位置 |
|---|---|
| 事件 ID | `FieldEvent.id`，单调递增，兼作 SSE `id:` 字段 |
| 断线重连 | 客户端回传 `Last-Event-ID`；服务端在 `StreamHello.resumed_from` 回显 |
| 去重 | 事件 ID 幂等；已见 ID 直接丢弃 |
| 授权 | 匿名可看（仅密度档）；带签名可看区域明细 |
| 心跳 | `SSE_HEARTBEAT_MS = 15s` |
| 速率限制 | `SSE_MAX_EVENTS_PER_MINUTE = 2`（spec §14.3） |
| 重放边界 | `SSE_REPLAY_WINDOW_MS = 300s`；超出返回 `410 + resync`，**不得把历史当作实时重播**（spec §8.5） |

## 明确禁止

- **不得把 SSE 连接数直称为真实人数。** 在线数只能表述为「在线实例数」，
  且必须与「节点数」分开呈现（`FieldSummary` 用四个独立 `FieldMetric` 强制这一点，
  结构上不存在一个可以合并的 `total` 字段）。

## 未来扩容（独立记录，不在 V1.0 承诺内）

若单实例连接数成为瓶颈：先做多实例 + Postgres `LISTEN/NOTIFY` 扇出，
再考虑边缘运行时。WebSocket 仅在出现真正的双向需求时才重新评估。

## 后果

- 无第三方依赖，F1–F4 不被外部账号阻塞。
- SSE 是纯文本协议，便于审计与抓包验证；代价是单向下行足够用、双向能力缺失。
