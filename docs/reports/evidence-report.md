# Evidence 判定报告（E3 语义验证）

状态：**TESTED + 在真实数据上实测**
实现：`crates/light-core/src/evidence.rs`、`crates/adapter-codex/`
决策记录：[ADR-011](../adr/ADR-011-evidence-gates.md)

---

## 1. 授权书提出的六个问题，逐个回答

### Q1 · `threads.model` 是否必然对应实际发生过的模型调用？

**否。** 实测证据：

| 分组 | 会话数 |
|---|---|
| `model` 有值 且 有模型产出物 | 244 |
| `model` 有值 但 无任何产出物、无 token | **78** |
| `model` 有值 但 无任何产出物（含 token 情况另计） | 见下 |
| `model` 为空 | 2 |
| 合计 | 347 |

其中 78 个会话 `tokens_used = 0`、`completed_turns = 0`、`agent items = 0`，
却已经写入了 `model`。结论：**`model` 在会话创建时即写入**，早于调用。

### Q2 · 是否可能只是创建会话时的预设模型？

**是，而且这就是那 78 条的本质。** 交叉验证结果：

| tokens>0 | 有完成 turn | 有产出物 | 会话数 |
|---|---|---|---|
| Y | Y | Y | 239 |
| Y | N | Y | 5 |
| Y | N | N | 25 |
| N | N | N | **78** |

最后一行正是「建了会话、选了模型、什么都没跑」。

### Q3 · 是否存在没有实际调用却产生 model 记录的情况？

**存在，且在最终判定集里也抓到了 2 条。** 这两个会话：

```
model = chatgpt-web/gpt-5.6-sol   tokens_used=0  completed_turns=0  agent_items=0
model = gpt-6-sol                 tokens_used=0  completed_turns=0  agent_items=0
```

若没有闸门 B，它们会被当成两次真实使用。现在它们被判定为
`Observable / SessionCreatedButNoModelOutput` —— 界面上可见「建过会话」，
但**不点亮内核、不进统计**。

### Q4 · 历史记录、模型切换、内部标签是否导致错误计数？

三类问题都存在，处理方式不同：

| 问题 | 实测 | 处理 |
|---|---|---|
| **内部标签**（最严重） | `codex-auto-review` 203 条，对应 `source={"subagent":{"other":"guardian"}}` 203 条 | 注册表 `task_label` 处置，**永不计数** |
| **历史/回填记录** | 无法从元数据区分「当时真跑过」与「后来导入」 | 要求产出物存在；导入但无产出物的记录不计数（保守） |
| **模型切换** | `threads.model` 是线程级字段，同一线程内换模型只保留一个值 | **必然少计，不会多计**。一个线程只可能贡献一个家族，且是它最后记录的那个。方向是安全的 |

模型切换的正确处理方式（F3 若需要）：不用线程级字段，改用逐 turn 的模型信息；
当前客户端没有该字段，因此无法更精确——这一点如实记录，不用推断填补。

### Q5 · `model_provider` 与 `model` 归一化后能否准确得到家族？

**能，但 provider 不参与判定。** 实测：

- 347 条记录的 `model_provider` **全部** 为 `openai`。
- 因此在当前数据上 provider 没有任何区分力。
- 注册表只依据 `model` 字符串；provider 仅作**信息性交叉核对**
  （`ScanCounts.provider_disagreements`），不作为家族来源。

理由：若用 provider 推断家族，就等于「按厂商名猜模型」，这正是 F0 G-02 禁止的推断。
宁可少一个信号，也不要一个会猜的信号。

### Q6 · 不读正文的前提下，是否还有其他安全元数据能验证调用发生？

**有，这是本次 F1 最有价值的发现。**

`thread_items.item_type` 是一个**枚举式标签列**，读取它不涉及任何消息内容。
它的取值分布（实测）：

```
reasoning 18720 | commandExecution 15724 | imageView 3830 | agentMessage 3644
fileChange 2428 | mcpToolCall 1146 | userMessage 1061 | webSearch 974
contextCompaction 178 | imageGeneration 121 | sleep 55 | subAgentActivity 6 | collabAgentToolCall 4
```

据此定义的产出物判定：

| 计入（能证明模型产出） | 排除（不能证明） |
|---|---|
| `agentMessage` `reasoning` `commandExecution` `mcpToolCall` `webSearch` `fileChange` `imageGeneration` `collabAgentToolCall` `subAgentActivity` | `userMessage`（用户输入）`imageView`（用户看图）`sleep`（定时器）`contextCompaction`（harness 记账） |

---

## 2. 最终规则

```
E3 = 闸门 A（归属） AND 闸门 B（发生）

闸门 A：registry.normalize(model).disposition == FAMILY
闸门 B：thread_items 中存在 item_type ∈ 产出物集合 的记录
```

`completed_turns` 只作佐证；`tokens_used` **完全不读**。

### 为什么不读 tokens_used —— 有数据支撑
两种规则的实测对比：

| 规则 | 证明的会话 | 家族数 |
|---|---|---|
| A：`tokens>0 AND (产出物 OR 完成 turn)` | 140 | 1 |
| B：`产出物 > 0` | 140 | 1 |

结果完全相同 → 选择不读消费数字的那一条。少读一列就少一分泄漏面，
也彻底消除「按消费额排名」的可能（产品文档 §1.5）。

---

## 3. 正向与负向测试

### 3.1 正向（必须计数）
| 用例 | 断言 |
|---|---|
| 家族 + 产出物 | `Countable / E3` |
| 客户端前缀家族 + 产出物 | 归属为 gpt，不新增家族 |
| 多会话同家族 | 家族数仍为 1 |

### 3.2 负向（必须不计数）
| 用例 | 期望判定 | 实现测试 |
|---|---|---|
| 有 model、无产出物 | Observable（E1） | `family_without_output_is_observable_not_countable` |
| 有 model、只有 `userMessage` | Observable | `model_without_output_is_only_observable` |
| 有 model、只有 `completed` turn | Observable | `completed_turns_alone_do_not_prove_a_call` |
| 任务标签（含 999 条产出） | Rejected | `task_label_is_rejected_regardless_of_output` |
| 未知模型 | Rejected | `empty_and_unknown_are_rejected` |
| 空字符串 | Rejected | 同上 |
| 历史库缺失 | 全部 Observable | `missing_history_database_downgrades_to_observable` |
| 状态库缺失 | 报错（不猜路径） | `missing_state_database_is_an_error_not_a_widened_search` |

---

## 4. 真实数据实测输出

通过受控访问层（`GuardedDb`）在真实安装的只读快照上执行：

```
adapter           : codex-app v1.0.0
registry          : 1.0.0
sessions seen     : 347
countable         : 140
observable (no op): 2
task-label rejects: 203
unknown-family    : 0
empty-model       : 2
VERIFIED FAMILIES : 1  (gpt)
columns touched   : id, item_type, model, model_provider, status, thread_id
refusals recorded : 0
```

对照：
- **双闸门结果 = 1 个家族 →「初明者」**
- **naive 计数 = 9 个家族 →「拓界者」**

同一个用户、同一台机器、同一份数据，两种算法给出两个完全不同的身份。

---

## 5. 本次判定对产品的影响（必须说清）

1. **可能少点亮。** 那 25 条「有 token 消耗但无产出物元数据」的会话被排除。
   它们很可能真的被用过（tokens>0），但没有可核验的响应证据。
   我们选择不计——**宁可少点亮一个人，好过错误点亮一个人。**
2. **某些客户端可能永远到不了 E3**，如果它不记录产出物类型。
   这些客户端只能停在 E0/E1，界面如实显示「已发现」，不假装点亮。
3. **闸门只会变严，不会为数字放松。** 任何放宽都必须走新的 ADR。

---

## 6. 未验证 / 待验证

| 项 | 状态 | 说明 |
|---|---|---|
| 其他客户端是否有类似产出物元数据 | `PLANNED` | F2 逐适配器验证（Claude / Qwen / Ollama） |
| 逐 turn 模型归属 | 不可用 | 当前客户端无此字段，不做推断 |
| E4 调用回执的验签流程 | `PLANNED` | F3 服务端实现 |
| 会话被 GC 后的历史证据 | 未知 | 若客户端清理老数据，历史家族可能「消失」；需在 F2 决定是否本地固化已确认家族 |
