# ADR-011 · E3 需要「归属」与「发生」双闸门

- **状态**：已采纳，已实现，已在真实数据上实测
- **决议来源**：F1 授权 §2.1（E3 实际使用语义验证）
- **日期**：2026-10-31

## 背景：一个具体的、可证伪的问题

授权书要求证明：`threads.model` 是否必然对应一次真实模型调用？

F1 期间在某真实客户端上直接测量，结论是**否**：

| 观察 | 数值 |
|---|---|
| `threads` 行数 | 347 |
| 其中 `tokens_used = 0` 且无任何模型输出 | **78** |
| 其中 `model` 有值但无调用证据 | **2**（都是「建了会话、选了模型、没跑」） |

也就是说：**`model` 字段在会话创建时就被写入**，早于任何模型调用。
若直接按 `model` 计数，会把从未发生的使用报成已发生。

更糟的是第二个发现：`codex-auto-review` 出现 203 次，
但它是应用内部 `guardian` 子代理的任务标签，**不是模型**。
naive 去重会把 1 个真实家族算成 9 个，直接把用户顶到最高称号。

## 决策

一次会话要成为 E3 证据，必须同时通过两个**独立**闸门：

### 闸门 A —— 归属（attribution）
`model` 字符串在注册表中命中且 `disposition = FAMILY`。
- 任务标签（`codex-auto-review`）→ 拒绝
- 未登记字符串 → `UNKNOWN` → 拒绝，不猜
- 空字符串 → 拒绝
- 内嵌客户端前缀（`chatgpt-web/gpt-6-sol`）→ 拆出 client，余下走别名表

### 闸门 B —— 发生（occurrence）
该会话必须至少有一条**模型产出物**记录。

判定依据是 `thread_items.item_type`——一个枚举式**标签列**，
读出它不涉及任何消息内容：

- 计入证明：`agentMessage` `reasoning` `commandExecution` `mcpToolCall`
  `webSearch` `fileChange` `imageGeneration` `collabAgentToolCall` `subAgentActivity`
- 明确排除：`userMessage`（用户自己输入）、`imageView`（用户在看图）、
  `sleep`（定时器）、`contextCompaction`（harness 记账）

### 关键取舍：`completed_turns` 不作为充分条件
`thread_turns.status = 'completed'` 只作**佐证**，不单独成立。
理由：一个 turn 可以在没有模型产出物的情况下被标记完成。
宁可更严，也不放宽。

### 关键取舍：完全不读 `tokens_used`
实测对比两种规则：

| 规则 | 证明的会话数 | 家族数 |
|---|---|---|
| A：`tokens_used>0 AND (产出物 OR 完成 turn)` | 140 | 1 |
| B：`产出物 > 0` | 140 | 1 |

两者结果完全相同。既然不读消费数字也能得到同样答案，就不读——
少读一列，就少一分泄漏面，也彻底消除「按消费额排名」的可能性（§1.5）。

## 实测结果（真实数据，通过受控访问层）

```
sessions seen     : 347
countable         : 140
observable (no op): 2      ← 建了会话没调用，被闸门 B 拦下
task-label rejects: 203    ← guardian 子代理，被闸门 A 拦下
empty-model       : 2
VERIFIED FAMILIES : 1  (gpt)
columns touched   : id, item_type, model, model_provider, status, thread_id
refusals recorded : 0
```

naive 计数会得到 9 个家族 →「拓界者」；双闸门得到 1 个家族 →「初明者」。

## 被放弃的替代方案

| 方案 | 为什么放弃 |
|---|---|
| 只按 `model` 计数 | 会把 78 个空会话与 203 个任务标签当使用，已证伪 |
| 用 `tokens_used > 0` 作为唯一证据 | 是消费量，语义上接近「按花费评定」，且不比产出物更准 |
| 读 rollout JSONL 的 `session_meta` 行 | 该文件含完整对话，超出「不读正文」边界 |
| 用失败/中断的 turn 也算 | 失败的调用不是使用 |

## 后果

- 适配器必须能证明「模型产出了东西」，不能只证明「配置里有模型」。
- 某些客户端可能**没有任何**可用的产出物元数据 → 该客户端只能到 E0/E1。
  这是可接受的：**少点亮一个人，好过错误点亮一个人。**
- 服务端 `usage_event` 表用 `CHECK (evidence_level between 3 and 4)` 二次兜底，
  E0–E2 在数据库层面就进不了可信统计。

## 重新考虑的条件

只有在某个客户端的元数据模型发生变化（例如提供真正的调用回执 E4）时，
才需要重新评估闸门 B 的信号选择。**任何时候都不得为了让数字更好看而放宽闸门。**
