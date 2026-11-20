# F1 完成状态与阻断清单

日期：2026-10-31
范围：授权书 §三 的 12 项任务 + §四 的 6 个验收场景

---

## 1. 十二项任务的最终状态

| # | 任务 | 状态 | 证据 |
|---|---|---|---|
| 1 | Git 仓库初始化与工程基础结构 | **IMPLEMENTED** | `first-light/.git`；Cargo workspace 4 crates + `packages/protocol`；`.gitignore` 排除数据库与密钥 |
| 2 | ADR 决策记录 | **IMPLEMENTED** | `docs/adr/` 12 份 ADR + 索引，D-01…D-07 全部正式化 |
| 3 | Model Registry 与家族归一化 | **TESTED** | `registry.rs` + `model-registry-report.md` |
| 4 | 别名 / 未知 / 内部任务标签过滤 | **TESTED** | `task_label_is_never_a_family`、`unknown_strings_are_not_guessed` |
| 5 | E0–E4 证据模型与判定规则 | **TESTED** | `evidence.rs` + `evidence-report.md` |
| 6 | 列级受控数据访问接口 | **TESTED** | `adapter-sdk` 21 项隐私测试 + `privacy-audit.md` |
| 7 | 本地匿名光种身份模型 | **TESTED** | `identity.rs`（根种子 / epoch 证书 / 恢复码）+ `node.rs` |
| 8 | 个人光点状态与颜色计算 | **TESTED** | `light.rs`（色阶 + palette + 亮度与深浅分离） |
| 9 | 知光 / 传光双称号规则引擎 | **TESTED** | `KNOWLEDGE_LADDER` / `TRANSMISSION_LADDER`，边界值全覆盖 |
| 10 | 跨平台事件协议与传光状态机 | **TESTED** | `protocol.rs`（11 个 JSON Schema）+ `transmission.rs`（状态机全转移） |
| 11 | SQLite 数据模型与迁移 | **TESTED** | `light-store` 14 项测试 + `local-database.md` |
| 12 | 单元 / 集成 / 隐私负向测试 | **TESTED** | 134 项全绿，`test-execution.md` |

**没有 `VERIFIED` 项**：F1 无真机 UI，按状态口径不得升级。
唯一在真实数据上验证过的是 `adapter-codex`（见 §3），其证据形态为
「TESTED + 真实数据实测」，仍不标 VERIFIED，因为真机集成（F2）尚未发生。

---

## 2. 六个验收场景

| 场景 | 要求 | 结果 | 证据 |
|---|---|---|---|
| **A** 模型归一化 | 9 个原始取值 → 1 个 GPT 家族，任务标签不算模型 | ✅ 通过（合成数据 + 真实数据双重复核） | `f0_samples_collapse_to_exactly_one_family`；真实扫描 `VERIFIED FAMILIES: 1` |
| **B** 隐私 | 敏感字段无法被适配器读到，有测试证据 | ✅ 通过 | 21 项测试；`privacy-audit.md` |
| **C** 称号 | 0/1/2–3/4–6/≥7 正确；E2 不触发升级 | ✅ 通过 | `knowledge_ladder_boundaries`、`e2_never_deepens_the_core`、`e2_never_changes_the_light_level` |
| **D** 传光 | 邀请→确认→激活→重复激活拒绝→撤回→回响 | ✅ 通过 | `transmission.rs` 13 项测试；无使用证据不得计为成功 |
| **E** 多设备身份 | 同光种多设备不重复计节点 | ✅ 通过 | `same_seed_on_many_devices_is_one_node` 等 6 项 |
| **F** 隐私与网络 | 完全离线运行，不上传任何个人数据 | ✅ 通过 | 核心 crate 无任何 HTTP 依赖；`the_app_is_fully_usable_without_any_network` |

---

## 3. 真实数据实测（关键证据）

在真实安装的只读快照上运行 `adapter-codex`：

```
sessions seen     : 347
countable         : 140      ← 通过双闸门
observable        : 2        ← 建了会话没调用，被拦下
task-label rejects: 203      ← guardian 子代理，被拦下
empty-model       : 2
VERIFIED FAMILIES : 1  (gpt)
columns touched   : id, item_type, model, model_provider, status, thread_id
refusals recorded : 0
```

复现方式：
```bash
# 1. 复制到工作区（沙箱不允许写 ~/.codex 的 -shm，见 ADR-012）
mkdir -p target/snapshot
cp ~/.codex/state_5.sqlite target/snapshot/state.sqlite
cp ~/.codex/thread_history_1.sqlite target/snapshot/history.sqlite
# 2. 扫描
cargo run -p adapter-codex --example scan_local -- \
  target/snapshot/state.sqlite target/snapshot/history.sqlite
# 3. 清理（快照绝不入库，.gitignore 已排除 *.sqlite）
rm -rf target/snapshot
```

---

## 4. 阻断清单（BLOCKED）

以下不是 F1 的欠账，而是需要外部条件才能推进的事项。

| 项目 | 状态 | 缺什么 | 影响 |
|---|---|---|---|
| iOS App / Widget / NFC 读取 | **BLOCKED** | Xcode 16+ | F5-01…F5-03，A08、A14 |
| Android App / 悬浮窗 | **BLOCKED** | Android SDK/NDK | F5-04，A08 |
| 微信小程序联调 | **BLOCKED** | 小程序 AppID | F5-06、F5-07 |
| NFC 真机触光 | **BLOCKED** | NTAG215/216 标签 + 读写器 + 真机 | F3-11，A08 |
| Windows 构建验证 | **BLOCKED** | Windows 机器 | F2-13，T-02 |
| 服务端部署 | **BLOCKED** | 云账号 / 域名 / 证书 | F3、F4 |
| 第三方签名回执（E4） | **PLANNED / 外部依赖** | 第三方客户端愿意签名 | ADR-008 通路 2 |

---

## 5. 未能完成但属 F1 范围内的偏差

| 项 | 说明 | 处理 |
|---|---|---|
| `brand.json` 单点配置 | ADR-006 要求用户可见品牌文字单点管理，F1 只做到"不写死"，未建配置文件 | 移入 F2（需要真实 UI 才有意义） |
| 服务端 API | F1 只定义契约（JSON Schema），未实现 Hono 服务 | F3，符合里程碑划分 |
| 真实 UI | 无 | F2 |

---

## 6. 对原计划的调整（需要你知悉）

1. **新增 ADR-012（WAL 只读策略）**：原计划没有这一条，是实测逼出来的。
   活着的 WAL 数据库无法严格只读打开，必须有两级策略并如实上报。
2. **`tokens_used` 从"可读但不用于评分"改为"完全不读"**：
   实测证明不读它也能得到相同结果，因此把 D-04 的最小列原则再收紧一档。
   这一改动记录在 ADR-011。
3. **本地验证明改用快照副本**：沙箱禁止写 `~/.codex` 的 `-shm`，
   因此在工作区内做只读快照后验证，验证后删除。数据未入库。

---

## 7. 下一步（F2）的前提

F1 已把「规则」与「隐私边界」做完并测住。F2 的第一件事不是画界面，
而是**在真机上确认 Tauri 透明窗口能否满足产品形态**（T-01）。
理由：这是唯一一个"如果不可行，整个桌面形态要重做"的风险点，
必须第一天验证，而不是最后一天。

详见 [f2-preparation.md](f2-preparation.md)。
