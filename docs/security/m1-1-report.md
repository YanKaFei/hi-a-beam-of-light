# M1.1 交付报告：唯一激活决策

目标：**消除所有能跳过凭证与来源政策的激活路径。**
结论：**达成。** 但正式网络的可信激活来源仍然只有测试夹具（见 §6）。

---

## 1. 激活路径审计（改前）

| 入口 | 能否产生 ACTIVATED | 结论 |
|---|---|---|
| `POST /v1/transmissions/:id/activate`（`app.mjs:533`） | **是，唯一入口** | 但接受任意 `usage_event_id` |
| `POST /v1/usage/attest` | 否 | 只写记录 |
| `POST /v1/usage/attest-v2` | 否 | 只写凭证 |
| `complete_transfer`（桌面端调用 activate） | 通过上面同一入口 | 无捷径 |
| FSM `ACTIVATE` | 只能被 `:id/activate` 调用（`grep '"ACTIVATE"'` 仅一处） | 无第二调用方 |
| SSE / outbox | 事后通知，不改变状态 | 无捷径 |

**审计结论：只有一条激活入口，但它接受自述证据。这就是绕过点。**

## 2. 唯一决策函数

`services/api/src/activation.mjs → evaluateActivation()`，十个条件顺序执行：
关系状态 → 接收方身份 → 凭证存在 → 签名 VALID → 发行密钥 ACTIVE →
来源政策 → 凭证绑定本 transfer → 挑战绑定（本 transfer / 本接收方 / 未过期 / 未消费）→
凭证未被其他关系消费 → 证据晚于关系建立。

返回三个**分开**的布尔值，不合并：

```
cryptoVerified       密码学完整性
sourcePolicyApproved 来源政策允许
activationEligible   全部绑定成立
```
四个状态在响应里独立可见：`CRYPTO_VERIFIED` / `SOURCE_POLICY_APPROVED` /
`ACTIVATION_ELIGIBLE` / `ACTIVATED`（后者是 FSM 状态）。

## 3. 挑战生命周期

`ISSUED → CONSUMED`（`RESERVED` 保留位，当前未使用）。
创建时绑定 `transfer_id` + 接收方身份 + `purpose='usage'` + 服务端 15 分钟有效期。
消费是单语句原子更新：

```sql
update challenge set state='CONSUMED', consumed_at_ms=$1
 where id=? and consumed_at_ms is null returning id
```
`rowCount !== 1` 即并发失败，抛 `CHALLENGE_RACE`。
挑战消耗、凭证消耗、FSM 激活、light_edge 更新、outbox 写入**在同一个 `db.tx` 内**。

## 4. 旧接口封闭

- `activate` 不再接受无凭证请求 → `426 UPGRADE_REQUIRED`（无论 feature flag 开关）。
- 旧 `/v1/usage/attest` 写入的行标记 `source_class='UNSIGNED_CLAIM'`，
  永远不参与激活、不参与可信称号。
- 保留一条**测试专用**路径：`FL_ACTIVATION_MODE=legacy-fixture` 且行被标记 `TEST_FIXTURE`
  时才可走旧逻辑。它用于让既有 F3 验收套件继续验证 FSM，**不是生产回滚开关**。
- **关闭 `FL_VERIFIER_V2` 不会恢复不可信激活**（A11 测试覆盖）。

## 5. 攻击测试（14 项，全绿）

A1 旧自述激活 → 426；A2 合法自签 → 403 SOURCE_NOT_TRUSTED；
A3 自签谎报 PLUGIN_OBSERVED → 绑定齐全时才通过（其余绑定被逐一否证）；
A4 跨身份使用 → 403/422；A5 跨 transfer → 422；A6 挑战过期 → 410；
A7 关系前证据 → 409；A8 一凭证多关系 → 拒绝；A9 八并发 → **恰好 1 次激活、1 条回响**；
A10 失败不留部分状态（挑战未消费、无消费记录、关系仍 LEARNING）；
A11 flag 关闭不恢复绕过；A12 已撤销密钥 → 403 KEY_REVOKED；
A13 **完整正向闭环**（TEST FIXTURE 签发者）→ ACTIVATED + 1 条回响 + 重放不重复计数；
T1 自签不能提升可信家族数 / `eligible.title=false`。

## 6. 仍无法证明的事

- **A3/A13 的正向通过依赖测试夹具签发者**，不是真实 AI 使用证据。
  生产环境目前**没有任何**独立可信来源，因此正式网络的
  `INDEPENDENTLY_CORROBORATED` 仍为空，公共光场不增长。
- 签名与挑战只能证明"这条声明在某个时刻由某把密钥签发"，不能证明模型真的运行过。
- 注册公钥不证明插件代码未被修改。

## 7. 回归

| 套件 | 结果 |
|---|---|
| M1.1 攻击/并发 | **14/14** |
| M1 凭证 | **13/13** |
| 旧验收 + FSM + Rust 互操作（fixture 模式） | **29/29** |
| Rust | **199 全绿** |
| 协议一致性 | 12 全绿 |

## 8. 本轮修掉的缺陷

1. `ActivationDenied` 未被错误处理器识别 → 全部 500；已映射为各自状态码。
2. 挑战绑定用了错误的参与方（可能绑到发起方）→ 改为只认 RECEIVER，并校验调用者。
3. 已激活关系重放返回 STATE_CONFLICT → 改为幂等 `already_activated`，
   且不产生第二条回响。
4. T5 断言旧错误码 404，与新行为 426 不符 → 更新断言并注明语义变化（不是掩盖）。

## 9. 未做

M2 插件、小程序、Android、F4 均未启动。
`RESERVED` 状态未使用（当前直接 ISSUED→CONSUMED）。
事务性 outbox 未改造（沿用既有写入，已包含在同一个事务里）。
