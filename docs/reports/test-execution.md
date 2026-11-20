# 自动化测试执行记录（F1）

执行日期：2026-10-31
执行环境：macOS 27.0.1 / arm64, Rust 1.98.1, Node v24.19.0
原始输出：`docs/reports/logs/rust-tests.log`、`docs/reports/logs/ts-tests.log`

## 1. 复现命令

```bash
# Rust 全量（122 项）
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_HOME="$PWD/.tmpdeps/cargo-home"      # 见 00-environment-audit.md §6
cargo test

# 跨语言契约（12 项）
cd packages/protocol && node --test test/conformance.test.mjs
```

两条命令都**不需要网络、不需要数据库服务、不需要任何账号**（验收场景 F）。

## 2. 结果

| 套件 | 位置 | 结果 |
|---|---|---|
| `light-core` 单元测试 | `crates/light-core/src/**` | **69 passed, 0 failed** |
| `light-core` 契约测试 | `crates/light-core/tests/contract.rs` | **6 passed, 0 failed** |
| `adapter-sdk` 隐私与守卫测试 | `crates/adapter-sdk/src/tests.rs` | **21 passed, 0 failed** |
| `adapter-codex` 适配器测试 | `crates/adapter-codex/src/tests.rs` | **12 passed, 0 failed** |
| `light-store` 存储测试 | `crates/light-store/src/tests.rs` | **14 passed, 0 failed** |
| **Rust 合计** | | **122 passed, 0 failed** |
| `packages/protocol` 跨语言一致性 | `test/conformance.test.mjs` | **12 passed, 0 failed** |
| **总计** | | **134 passed, 0 failed** |

退出码：`cargo test` → 0；`node --test` → 0。

## 3. 关键测试点与它们防的具体错误

### 3.1 模型归一化（验收 A）
| 测试 | 防的错误 |
|---|---|
| `f0_samples_collapse_to_exactly_one_family` | 9 个原始取值必须收敛到 1 个家族 |
| `naive_counting_would_have_been_wrong` | 固化「naive 计数会错」这个事实，防止有人改回去 |
| `task_label_is_never_a_family` | `codex-auto-review` 不得成为家族（含大小写/空白变体） |
| `unknown_strings_are_not_guessed` | `gptq`、`claude3`、`my-gpt-wrapper` 不得被猜成家族 |
| `registry_validation_rejects_broken_data` | 指向不存在家族 / 重复别名 / 空 semver / 非法 disposition 全部加载失败 |
| `client_prefix_is_extracted_not_invented` | `chatgpt-web/gpt-6-sol` → client + family，不产生新家族 |

### 3.2 证据判定（验收 A/C）
| 测试 | 防的错误 |
|---|---|
| `family_plus_output_is_countable` | 正例：家族 + 产出物 → E3 |
| `family_without_output_is_observable_not_countable` | **建了会话没调用** → 只可观察，不点亮 |
| `completed_turns_alone_do_not_prove_a_call` | 完成 turn 但不是充分条件 |
| `task_label_is_rejected_regardless_of_output` | 任务标签即使有 999 条产出也不计数 |
| `e2_never_deepens_the_core` | E2 不改变等级与称号 |
| `received_light_only_affects_the_ring` | 来光不影响内核 |
| `sharing_never_reduces_the_sender` | 分享不扣减 |
| `deep_core_keeps_a_warm_halo` | 深红仍有暖光外晕（亮度与深浅分离） |

### 3.3 隐私（验收 B）
| 测试 | 防的错误 |
|---|---|
| `manifest_cannot_allow_conversation_columns` | 8 个内容列（`first_user_message`/`preview`/`title`/`item_json`…）无法被任何 manifest 放行 |
| `manifest_cannot_allow_consumption_counters` | `tokens_used`/`cost` 全局禁读 |
| `manifest_cannot_allow_secrets_or_contact_data` | 密钥、令牌、邮箱、经纬度禁读 |
| `manifest_cannot_allow_filesystem_detail` | `cwd`/`rollout_path`/`git_origin_url` 禁读 |
| `reading_a_sensitive_column_is_refused_and_recorded` | 5 次越界读取全部被拒且留痕 |
| `reading_a_column_outside_the_allowlist_is_refused` | 未声明列不可达 |
| `filtering_on_a_forbidden_column_is_refused` | **WHERE 子句也不能夹带** |
| `select_star_is_structurally_impossible` | 生成的 SQL 中星号只可能出现在 `count(*)` |
| `audit_records_names_never_values` | 审计只含列名；6 种敏感串均不出现在审计与 Debug 输出中 |
| `a_refusal_never_widens_the_scan` | 拒绝后不自动扩大扫描范围 |
| `row_budget_stops_a_runaway_scan` | 超预算即失败（不返回部分结果） |
| `opening_a_missing_file_fails_without_searching_for_alternatives` | 文件缺失就是失败，不猜路径 |
| `connection_is_read_only` / `query_only_mode_still_cannot_write` | 两种打开模式都无法写 |
| `scan_report_leaks_no_content_and_no_raw_session_ids` | 报告不含内容、路径、原始会话 ID |
| `adapter_touches_only_allowlisted_columns` | 真实扫描中触达的列 ⊆ 声明 |
| `only_three_tables_and_seven_columns_are_reachable` | 数据面扩张必须改测试 → 强制评审 |

### 3.4 传光状态机（验收 D）
| 测试 | 防的错误 |
|---|---|
| `single_confirmation_does_not_form_a_relation` | 单方确认不是关系 |
| `both_confirmations_form_the_edge_once` | 边只形成一次 |
| `received_alone_can_never_activate` | 收到光不能直接激活 |
| `activation_without_evidence_is_refused` | 无使用证据不能激活 |
| `repeated_activation_is_idempotent_and_not_double_counted` | 重复激活不双计数 |
| `repeated_acknowledge_queues_only_one_echo` | 回响只一次 |
| `rejection_forms_no_relation_and_emits_no_success` | 拒绝不产生成功副作用 |
| `expiry_blocks_late_events` | 过期后事件被拒 |
| `revoke_after_activation_keeps_the_relational_history_but_ends_it` | 撤回关系但不撤销受助者已得的光 |
| `terminal_states_accept_nothing` | 终态不再转移 |
| `version_increments_only_on_real_transitions` | 重放不递增版本（乐观锁基础） |

### 3.5 多设备身份（验收 E）
| 测试 | 防的错误 |
|---|---|
| `same_seed_on_many_devices_is_one_node` | 3 设备 1 节点 |
| `different_seeds_are_never_merged` | 不同光种不合并 |
| `a_device_cannot_silently_switch_identity` | 设备不能悄悄换身份污染数据 |
| `restoring_a_seed_does_not_inflate_activation_counts` | 恢复后重复激活不虚增 |
| `anon_id_is_stable_across_epochs` | 轮换密钥不换身份 |
| `certificate_verifies_and_rejects_wrong_identity` | 证书不能张冠李戴 |
| `recovery_roundtrip_preserves_identity_and_epoch_keys` | 恢复后身份与签名能力一致 |
| `recovery_code_rejects_corruption` | 恢复码校验和有效 |

### 3.6 存储与离线（验收 C/F）
| 测试 | 防的错误 |
|---|---|
| `one_family_counts_exactly_once` | `unique(family)` 使多客户端同家族只计一次 |
| `e2_never_changes_the_light_level` | 7 个 E2 家族 → 仍是寻光者 |
| `non_countable_evidence_is_refused` | 存储层拒收 E2 作为可计数证据 |
| `the_app_is_fully_usable_without_any_network` | 本地库自足 |
| `consent_defaults_to_not_granted` | 缺省不贡献 |
| `future_schema_is_refused_rather_than_corrupted` | 未来版本不降级破坏数据 |

### 3.7 跨语言一致性
`crates/light-core/tests/contract.rs` 与 `packages/protocol/test/conformance.test.mjs`
读取**同一批** `packages/protocol/fixtures/*.json`，分别由 Rust 与 TypeScript
两套独立实现验证。任一侧改了阈值而另一侧没改，必有一条测试失败。

`committed_schemas_match_the_core` 还会重新生成 JSON Schema 并与仓库中的文件比对，
防止 schema 与类型脱节。

## 4. Rust 与 TypeScript 结果一致性

| 检查 | Rust | TypeScript |
|---|---|---|
| 知光称号 0–12 | ✅ 69 项套件内 | ✅ fixture 逐条 |
| 传光称号 0/1/2/3/9/10/29/30/99/100 | ✅ | ✅ |
| 光阶 0/1/2/3/4/6/7 | ✅ | ✅ |
| E2 不影响等级 | ✅ | ✅ |
| 双闸门证据判定（8 个用例） | ✅ | ✅ |
| 归一化（15 个用例，含未知与任务标签） | ✅ | ✅ |

## 5. 未覆盖 / 未执行的部分（诚实记录）

| 项目 | 状态 | 原因 |
|---|---|---|
| 服务端 API 集成测试 | `PLANNED` | F1 范围不含服务端实现 |
| 真机 UI 测试 | `BLOCKED` | F2 未开始，无 Tauri 应用 |
| iOS / Android / 小程序测试 | `BLOCKED` | 无 Xcode / 无 SDK / 无 AppID |
| NFC 真机测试 | `BLOCKED` | 无标签、无读卡硬件 |
| 并发压力测试 | `PLANNED` | 属服务端，F3 |
| 真实数据库直连扫描 | 已在**快照副本**上完成 | 沙箱禁止写 `~/.codex` 的 `-shm`；见 ADR-012 |

最后一项不是缺陷：真实数据的验证确实做了，只是通过工作区内的只读快照完成，
且快照在验证后已删除、从未入库。
