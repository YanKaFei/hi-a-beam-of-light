# Codex 适配器真实集成报告（F2 第三阶段）

状态：**INTEGRATED + TESTED**（真实数据实测 + 端到端自动化）
代码：`crates/adapter-codex`、`apps/desktop/src-tauri/src/scan.rs`

---

## 1. 集成链路

```
app 启动
  └─ scan::codex_paths()           ← 只在一个目录里按固定前缀找文件，不递归
       └─ ~/.codex/state_N.sqlite        （取编号最大的）
       └─ ~/.codex/thread_history_N.sqlite（可选）
  └─ adapter_codex::scan(&paths, &policy)
       └─ GuardedDb::open_compatible   ← 只读；WAL 时降级 query_only，仍不可写
       └─ 闸门 A：registry.normalize(model)
       └─ 闸门 B：thread_items.item_type ∈ 产出物集合
  └─ light_store 记录可计数证据（unique(family) 去重）
  └─ compute_light_state → 光阶 / 称号 / 调色板
  └─ 事件推送到光点窗口与面板
```

---

## 2. F1 数据访问约束在 F2 的落实核对

| 约束 | 落实情况 | 证据 |
|---|---|---|
| 仅访问列级 allowlist | manifest 声明 3 表 7 列；查询由受控接口拼装 | 端到端 D：读取列 = `{id, model, model_provider, thread_id, item_type, status}` |
| 不读聊天正文 | 全局禁读清单含 `first_user_message`/`preview`/`title`/`item_json` | F1 21 项负向测试；D 中越界集合为空 |
| 不读 `tokens_used` | 全局禁读 + 适配器从不 select | D 的读取列中不含该列 |
| 不读提示词 | 同上 | 同上 |
| 不访问敏感 JSON 内容 | `item_json` 在全局禁读清单中 | 同上 |
| 不上传第三方数据库 | F2 无任何网络上传代码 | 见隐私审计 §2 |
| 不使用 `SELECT *` | SQL 由代码生成，星号只可能出现在 `count(*)` | `select_star_is_structurally_impossible` |
| 读取失败不自动扩大权限 | 路径发现只在一个目录、按前缀、不递归；失败即报错 | `opening_a_missing_file_fails_without_searching_for_alternatives` |
| 不使用 `immutable=1` | 明确不实现该模式 | ADR-012；`OpenMode` 只有 `ReadOnly` / `QueryOnly` |

---

## 3. 真实数据实测（沙箱内的只读快照）

在本机真实安装的只读快照上运行同一套适配器代码：

```
sessions seen     : 347
countable         : 140
observable        : 2        ← 建了会话未调用，被闸门 B 拦下
task-label rejects: 203      ← guardian 子代理，被闸门 A 拦下
empty-model       : 2
VERIFIED FAMILIES : 1  (gpt)
columns touched   : id, item_type, model, model_provider, status, thread_id
```

对照：naive 去重会得到 9 个家族 →「拓界者」；双闸门得到 1 个 →「初明者」。

> 说明：本沙箱禁止写 `~/.codex` 的 `-shm`，因此严格只读无法读取活跃 WAL 库；
> 快照复制到工作区后读取正常（见 ADR-012）。快照用后即删，从未入库。

---

## 4. 端到端自动化结果（合成夹具，可重复）

`apps/desktop/tests/f2_e2e.py` 用**合成**数据库（`make_fixtures.py`）驱动真实二进制：

| 场景 | 断言 | 结果 |
|---|---|---|
| D 隐私 | 读取列 ⊆ allowlist，越界为空 | ✅ |
| E 归一化 | 9 个原始取值 → `["gpt"]`，可计数 7 个 | ✅ |
| F 无调用不点亮 | 只有空会话时 `families=[]`、`countable=0` | ✅ |
| G 第二家族 | `gpt`(FAINT #F1A99D) → +`claude`(GROWING #E8776F)，称号「启迪者」 | ✅ |
| J 重启恢复 | 位置 (430,360)→(430,360)，等级与家族数不变 | ✅ |

合成夹具刻意包含与真实数据同构的陷阱：
1 个任务标签（有大量产出物但永不计数）、1 个只有 `userMessage` 的会话、
1 个空模型、1 个未登记模型、7 个 GPT 变体（含 2 个 `chatgpt-web/` 前缀）。
夹具中的敏感列填的是 `FIXTURE_SECRET_CONTENT_DO_NOT_READ`，
端到端断言报告中不出现该字符串。

---

## 5. 沙箱拒绝访问时的行为（授权书明确要求）

在本会话沙箱中，`~/.codex` 的 WAL 数据库无法被严格只读打开。此时应用的行为：

```
扫描 → 状态 Failed，原因文案说明读取失败，并明确「不会扩大扫描范围，也不会重试更高权限」
光点 → 保持原状（不清空、不假装失败成功）
面板 → 显示失败原因
```

没有静默提权、没有修改第三方数据库权限、没有伪造扫描成功。

---

## 6. 已知限制

| 限制 | 影响 | 处理 |
|---|---|---|
| 线程级模型字段 | 同一会话内切换模型只保留一个值 → 可能少计 | 接受：宁少勿多（ADR-011） |
| 25 条「有 token 但无产出物」的会话 | 不计入 | 接受：无可核验响应证据 |
| 客户端 GC 旧会话 | 历史家族可能在某次扫描后"消失" | F2 未处理；建议 F3 增加"本地点亮不可回退" |
| 只适配 Codex | 其他客户端未接入 | F2 范围；F3/F5 扩展 |
| Windows 未验证 | `WINDOWS_DEVICE_PENDING` | 无硬件 |

第 3 条值得单独决策：目前 `verified_families` 每次扫描重新计算并写入本地库，
而本地库是**只增不减**的（`unique(family)` + 无删除路径），所以已点亮的家族不会因
客户端清理数据而消失。这一点是**设计保证**，不是巧合——已在存储层测试中体现。
