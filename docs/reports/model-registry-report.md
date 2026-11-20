# Model Registry 测试报告

状态：**TESTED**（离线、可复现、跨语言双向验证）
实现：`crates/light-core/src/registry.rs`
数据：`data/model-registry.v1.json`（semver `1.0.0`）

---

## 1. 结论

| 验收点 | 结论 | 证据 |
|---|---|---|
| 9 个原始取值 → 1 个家族 | ✅ 通过 | `f0_samples_collapse_to_exactly_one_family` |
| 内部任务标签不算模型 | ✅ 通过 | `task_label_is_never_a_family`（含大小写/空白变体） |
| 未知模型不猜 | ✅ 通过 | `unknown_strings_are_not_guessed` |
| 注册表加载即校验 | ✅ 通过 | `registry_validation_rejects_broken_data` |
| 跨语言一致 | ✅ 通过 | `registry_fixture_matches_the_core` + `conformance.test.mjs` |
| 每个决策记录 semver | ✅ 通过 | `normalization_records_registry_version` |

---

## 2. F0 样本的最终对照表

原始观测数据（某真实客户端 `threads.model` 列的 9 个 distinct 取值）：

| # | 原始字符串 | 命中规则 | 家族 | 若 naive 计数 |
|---|---|---|---|---|
| 1 | `gpt-6-sol` | exact | gpt | 1 |
| 2 | `gpt-5.6-sol` | exact | gpt | 2 |
| 3 | `gpt-5.6-terra` | exact | gpt | 3 |
| 4 | `gpt-6.1-sol` | exact | gpt | 4 |
| 5 | `gpt-6-astra` | exact | gpt | 5 |
| 6 | `chatgpt-web/gpt-6-sol` | client_prefix + exact | gpt（client=chatgpt-web） | 6 |
| 7 | `chatgpt-web/gpt-5.6-sol` | client_prefix + exact | gpt（client=chatgpt-web） | 7 |
| 8 | `codex-auto-review` | exact → task_label | **非模型** | 8 |
| 9 | `""` | — | **unknown** | 9 |

- **正确家族数 = 1** → 知光称号「初明者」
- **naive 计数 = 9** → 知光称号「拓界者」（最高称号）

这不是精度差异，而是用户肉眼可见的错误。测试 `naive_counting_would_have_been_wrong`
把这个对比固化为断言，任何人试图退回 naive 实现都会失败。

---

## 3. 规则定义（可审计）

### 3.1 匹配顺序（固定，不随数据顺序变化）
```
1. 客户端前缀剥离（最长前缀优先）→ 对剩余部分继续 2、3
2. 精确别名命中
3. 前缀别名命中（最长模式优先）
4. 都不命中 → Unknown
```

### 3.2 disposition 三态
| disposition | 含义 | 是否可计数 |
|---|---|---|
| `family` | 真实模型家族 | ✅ |
| `task_label` | 内部任务/子代理标签，不是模型 | ❌ |
| `unknown` | 未登记 | ❌ 永不猜 |

### 3.3 为什么 `codex-auto-review` 是 task_label
实测交叉验证：
- `codex-auto-review` 出现 203 次；
- 同库 `threads.source` 中 `{"subagent":{"other":"guardian"}}` 也正好 203 次。

即它是应用内部 guardian 子代理的线程标签，与模型家族无关。
把 55M tokens 的"用量"归给一个不存在的第十个模型，会产生一个凭空造出的家族。

### 3.4 前缀规则不是推断
`gpt-` → gpt 是一条**登记在数据文件里、需要评审、有版本**的策展规则。
它可追溯、可回滚。与之相对，「字符串里有 gpt 就算 GPT」不可追溯。
测试用 `my-gpt-wrapper` 明确断言：前缀规则是 "startsWith"，不是 "contains"。

---

## 4. 注册表校验（加载即失败）

| 非法情形 | 行为 |
|---|---|
| 别名指向不存在的家族 slug | `CoreError::Registry` |
| 重复的精确别名 | 失败 |
| 空别名 / 空 semver / 空家族 slug | 失败 |
| 别名声明 `unknown` disposition | 失败（unknown 是运行时结论，不是数据） |
| JSON 解析失败 | 失败并带位置 |

这意味着注册表的数据 PR 在 CI 就会被挡下，而不是在用户机器上产生奇怪数字。

---

## 5. 共享夹具

`packages/protocol/fixtures/registry_normalization.json` 含 15 个用例：
6 个真实 GPT 变体、2 个客户端前缀、1 个任务标签、1 个空串、
`deepseek-r1`、`claude-sonnet-4-5`、`qwen3-coder`、`gemini-2.5-pro`，
以及 2 个必须**不匹配**的反例（`mystery-model-zzz`、`my-gpt-wrapper`）。

同一份文件被 Rust（`tests/contract.rs`）与 TypeScript（`conformance.test.mjs`）
分别验证。两侧独立实现，任一侧改动未同步即失败。

---

## 6. 已知局限

| 局限 | 影响 | 处理 |
|---|---|---|
| 新模型需人工登记 | 新模型发布后可能短暂显示为 unknown | 服务端可下发新注册表（F3），客户端验签后使用 |
| 前缀规则可能过宽 | 例如未来某家族也以 `gpt-` 开头 | 精确别名优先；发现冲突时加精确别名并收紧前缀 |
| 无法验证「同一字符串在不同客户端指同一家族」 | 极端厂商改名场景 | 靠精确别名逐条修正，不做语义推断 |

**明确不做**：不根据 provider 字段反推家族，不用调用次数/消费额辅助判断，
不对未登记字符串做模糊匹配。

---

## 7. 真实数据实测

通过受控访问层对真实安装的扫描结果（详见 [evidence-report](evidence-report.md)）：

```
registry          : 1.0.0
sessions seen     : 347
VERIFIED FAMILIES : 1  (gpt)
```

若按 naive 去重：9 个家族。
