# 05 · 事件状态机

> 状态机的价值不在于画出状态名，而在于**每个转移都写清：谁能触发、判据是什么、副作用是什么、超时怎么办、重复调用怎么办。**
> 文档 §7.2 只给了状态名，本文件补全全部判据——这部分需你审核确认。

---

## 1. 传光会话状态机（核心）

### 1.1 状态图

```
                        ┌──────────┐
                        │ CREATED  │  发起方生成会话（token 仅返回一次）
                        └────┬─────┘
             30s 内未展示    │  展示二维码/NFC/链接
             ┌───────────────┴──────────────┐
             ▼                              ▼
        ┌─────────┐                   ┌──────────┐
        │ EXPIRED │◄──────10min───────│ OFFERED  │
        └─────────┘                   └────┬─────┘
                                            │ 接收方打开 /t/{token}
                                       ┌────▼─────┐
                                       │ RECEIVED │◄── 二维码/ NFC / 深链 / 近场
                                       └────┬─────┘
              接收方拒绝                    │ 双方确认（接收方 accept + 发起方 confirm）
        ┌────────────┐                     ▼
        │  REJECTED  │◄──────────────┌──────────┐
        └────────────┘   5min 未确认  │ ACCEPTED │
                                      └────┬─────┘
                                           │ 建立 light_edge
                                      ┌────▼─────┐
                                      │ LEARNING │  等待受助者首次可信使用
                                      └────┬─────┘
                                           │ 受助者首个 E3/E4 事件（晚于边创建）
                                      ┌────▼─────┐
                                      │ACTIVATED │  受助者内核点亮
                                      └────┬─────┘
                                           │ 帮助者客户端收到回响
                                      ┌────▼──────┐
                                      │ACKNOWLEDGED│
                                      └────┬──────┘
                                           │ 任一方撤回
                                           ▼
                                      ┌─────────┐
                                      │ REVOKED │
                                      └─────────┘

     CANCELLED：发起方在 CREATED/OFFERED 阶段取消
```

### 1.2 转移表

| # | 从 | 事件 | 触发者 | 判据 | 副作用 | 超时 |
|---|---|---|---|---|---|---|
| T1 | — | `create` | 发起方 | 已注册、当日配额未满 | 写 `transmission(CREATED)`，存 `token_hash`，返回明文令牌一次 | 30s 内必须 `offer` |
| T2 | CREATED | `offer` | 发起方 | 已生成二维码/写标签 | `state=OFFERED` | 10min 后 `expire` |
| T3 | OFFERED | `open(token)` | 接收方 | token 哈希命中、未过期、未使用 | 写 `transmission_party(RECEIVER)`；`state=RECEIVED` | 5min 内需双向确认 |
| T4 | RECEIVED | `accept` | 接收方 | 状态=RECEIVED，`version` 匹配 | 记接收方 `confirmed_at` + 签名 | — |
| T5 | RECEIVED | `confirm` | 发起方 | 接收方已 accept | 记发起方 `confirmed_at` + 签名；**两者齐备**才转 ACCEPTED | — |
| T6 | RECEIVED | `reject` | 接收方 | 状态=RECEIVED | `state=REJECTED`；不建边；不播成功动画 | — |
| T7 | CREATED/OFFERED/RECEIVED | `expire` | 系统 | `now() > expires_at` | `state=EXPIRED`；清 `token_hash` | — |
| T8 | CREATED/OFFERED | `cancel` | 发起方 | 未被 RECEIVED | `state=CANCELLED` | — |
| T9 | ACCEPTED | `form_edge` | 系统 | 双方确认齐备（T4∧T5） | 建 `light_edge(FORMED)`；`state=LEARNING` | 同事务 |
| T10 | LEARNING | `activate` | 接收方 | 见 §1.3 五条校验 | 建 `usage_event`（若为该家族首个）；`light_edge=ACTIVATED`；`outbox(edge.activated)` | 无超时 |
| T11 | ACTIVATED | `acknowledge` | 帮助者客户端 | 收到 `edge.activated` 事件 | `acknowledged_at=now()`；`state=ACKNOWLEDGED`；播一次回响 | 可离线延迟，重连后补 |
| T12 | ACCEPTED/LEARNING/ACTIVATED/ACKNOWLEDGED | `revoke` | 任一方 | — | `light_edge=REVOKED`；`state=REVOKED`；**不删除 usage_event**（那是受助者自己的光） | — |

### 1.3 `activate` 的五条校验（缺一即 409）

1. 调用者 = 该会话 `RECEIVER`。
2. 会话 `state = 'LEARNING'`。
3. `usage_event` 存在、属于该 `identity_id`、`evidence_level ∈ {3,4}`。
4. `usage_event.received_at > light_edge.created_at` ← **防「用历史使用冒充新激活」**（A09/A10 的关键）。
5. `light_edge.state = 'FORMED'`（未被并发激活过）。

### 1.4 并发与幂等（[G-07](02-requirements-traceability.md) 的补全）

| 场景 | 行为 |
|---|---|
| 双端同时 `accept`+`confirm` | `UPDATE … WHERE version = ?` 乐观锁，失败方得 `409 STATE_CONFLICT` 并返回当前状态（不损坏） |
| 重复 `accept` | 返回 200 与当前状态（幂等，不报错） |
| 重复 `activate` | 返回 200 且 `already_activated: true`，**不重复计数** |
| `expire` 与 `open` 竞争 | 以数据库时间 `now()` 为准；`open` 若在 `expires_at` 之后即 `410` |
| `revoke` 后重放 `activate` | `409`，不复活边 |
| 帮助者长期离线 | `ACKNOWLEDGED` 可无限期延迟；回响在重连后一次性补发（不重复） |

**重要**：`REVOKED` **不撤销**受助者已经获得的 `usage_event`。
理由：受助者的光是他自己用出来的，不因关系撤回而消失——这是「光不因分享而减少」在状态机上的体现。

---

## 2. 证据等级生命周期（§5.3）

### 2.1 单调晋升

```
E0 已发现 ──► E1 已连接 ──► E2 自主确认 ──► E3 本机可核验 ──► E4 受支持的调用回执
   │             │              │                │                    │
   └─────────────┴──────────────┴────────────────┴────────────────────┘
                       同一 (identity, family) 只保留最高等级
```

| 转移 | 判据 | 是否影响主色 | 是否进全球统计 |
|---|---|---|---|
| → E0 | 应用/配置/服务路径存在 | 否 | 否 |
| → E1 | 存在登录态或合法连接配置 | 否 | 否 |
| → E2 | 用户亲自勾选「我已使用过」 | **否**（仅侧栏显示） | 否 |
| → E3 | 适配器输出合规元数据事件 | ✅ | ✅ |
| → E4 | 集成方返回签名回执 | ✅ | ✅ |

**硬规则**：`E2 → E3` 是**替换**而非累加：同一家族一旦到 E3，界面显示「已验证」，
但「自主确认」这条记录仍保留在 `evidence_observation`，供用户回看自己的历史。

**禁止降级**，唯一例外：适配器被用户关闭扫描 → 该家族的 E3 **不降级**（已经是核验过的事实），
只是不再更新 `last_verified_at`。界面可标注「最后核验于 X 天前」。

### 2.2 等级与称号的关系（这是我建议的硬规则，[G-05](02-requirements-traceability.md)）

| 用途 | 使用的等级集合 |
|---|---|
| 光点主色深浅 | {E3, E4} |
| 知光称号 | {E3, E4} |
| 全球可信统计 | {E3, E4} |
| 「自主确认 N」徽标 | {E2} |
| 「已发现/已连接」列表 | {E0, E1} |

**E2 永远不会单独把主色推深**。这是 `usage_event` 的 CHECK 约束（[03 §4](03-data-model.md)）在数据库层的保证。

---

## 3. 同意状态机（§12.2）

```
        ┌────────┐   用户授权    ┌──────────┐
        │ ABSENT │──────────────►│ GRANTED  │
        │ (无行)  │               └────┬─────┘
        └───┬────┘                    │ 撤回
            │ 用户拒绝                 ▼
            └──────────────►  ┌──────────┐
                              │ REVOKED  │─── 重新授权 ──► GRANTED
                              └──────────┘
```

| 转移 | 触发 | 副作用 |
|---|---|---|
| ABSENT → GRANTED | 用户在设置里开启「参与世界光场」 | 写 `consent`；开始心跳与事件上报 |
| ABSENT → DECLINED | 用户明确拒绝 | 写 `consent(DECLINED)`；不上报 |
| GRANTED → REVOKED | 用户关闭 | 停止上报；**触发 `/privacy/revoke` 删除可关联事件并重算聚合** |
| REVOKED → GRANTED | 用户重新开启 | 从头开始计数（不恢复已删数据） |

**两个独立 scope**：`GLOBAL_FIELD_CONTRIBUTION`（贡献统计）与 `PUBLIC_LIGHT_PATH`（公开光路）。
必须能「只观看、不贡献」——这是 §12.2 的明确要求，因此**观看不需要任何 consent 记录**。

---

## 4. 身份生命周期（M1 / [G-10](02-requirements-traceability.md)）

```
 NEW ──► LOCAL_ONLY ──► REGISTERED ──► ROTATED ──► REGISTERED(新 epoch)
                              │
                              └──► REVOKED
```

| 状态 | 含义 | 备注 |
|---|---|---|
| `NEW` | 首次启动，正在生成密钥 | 瞬时 |
| `LOCAL_ONLY` | 光种已生成，从未联网 | 完整可用（离线原则） |
| `REGISTERED` | 公钥已登记，可参与光场 | 需用户授权 |
| `ROTATED` | 旧 epoch 已作废，新 epoch 生效 | 恢复短语只用于**签发新密钥**，不复制旧私钥 |
| `REVOKED` | 用户主动销毁 | 服务端保留匿名计数，删除可关联数据 |

**恢复设计的硬约束**：恢复凭据（12/24 词）**不是**私钥本身，而是「有权为该 `anon_id` 登记新 epoch」的凭据。
因此：
- 泄露恢复短语的风险从「永久冒充」降为「可被轮换/吊销」。
- `identity_key_epoch.retired_at` 一旦写入，旧 epoch 签名一律 401。

---

## 5. 光点状态派生（不是状态机，是纯函数）

```ts
// crates/light-core 与 packages/visual-system 共用同一组 fixtures 对齐
level(familyCount) =
  familyCount === 0        → 'DORMANT'    // 沉睡 灰雾光
  familyCount === 1        → 'FAINT'      // 微明 浅珊瑚红
  familyCount <= 3         → 'GROWING'    // 生长 柔赤红
  familyCount <= 6         → 'BLAZING'    // 炽热 饱和绯红
  else                     → 'DEEP'       // 深红 暗宝石红 + 暖红外辉

titleKnowledge(familyCount) =           // 知光称号
  0 → '寻光者' | 1 → '初明者' | 2..3 → '启迪者' | 4..6 → '探索者' | ≥7 → '拓界者'

titleTransmission(activatedNodes) =      // 传光称号
  0 → '持光者' | 1 → '启蒙者' | 3..9 → '引光者' | 10..29 → '播光者'
  | 30..99 → '传灯者' | ≥100 → '燎原者'
```

**边界的定义**：阈值为「达到即进入」，`2..3` 含 3，`3..9` 含 9。这一条必须写进 fixtures，
否则 TS 与 Rust 两侧对 `count=3` 会给出不同称号（传光侧 3 是「引光者」的起点，知光侧 3 是「启迪者」的终点）。

**不变量**：
- 来光（收到别人的光）**只影响外环**，不进入 `familyCount`。
- 分享**不减少** `familyCount`。
- `attestedCount`（E2）**不参与**上面任何函数。

---

## 6. 五秒仪式时序（§7.6）与状态机的对应

| 时间 | 视觉 | 必须已发生的服务端状态 |
|---|---|---|
| 0.0–0.6s | 两光点靠近、同步微颤 | `state=RECEIVED`（T3 已达成） |
| 0.6–1.2s | 外环共鸣、轻雾回收 | 双方确认齐备（T4∧T5） |
| 1.2–2.0s | 深红析出光粒，内核不减少 | `ACCEPTED`（T9 已建边） |
| 2.0–3.5s | 光粒弧线迁移 | 边已持久化 |
| 3.5–5.0s | 接收方外环亮起，帮助者收到光波 | `LEARNING` 已写入 |

**关键约束**（§7.6 末段）：动画的每一段都必须**挂在真实服务端状态之后**。
不满足前置状态时：
- 播放「等待确认」的中间态，而不是成功动画；
- 失败/离线/拒绝一律不播成功动画（A07 的验收点）。

这条必须在实现时做成**门控函数**而不是散落的 `if`：

```ts
const canPlay = (phase: RitualPhase, s: TransmissionState) => PHASE_REQUIRES[phase](s);
// 'resonance'  → s in {RECEIVED, ACCEPTED, LEARNING, ...}
// 'emit'       → s in {ACCEPTED, LEARNING, ...}
// 'arrive'     → s in {LEARNING, ACTIVATED, ...}
// 'echo'       → s === 'ACTIVATED' || s === 'ACKNOWLEDGED'
```

---

## 7. 状态机测试清单（F1 必做）

| 用例 | 断言 |
|---|---|
| 正常主路径 | CREATED→…→ACKNOWLEDGED，每步只发生一次副作用 |
| 未双向确认 | 停在 RECEIVED，**无边**，无成功动画（A07） |
| 过期后扫码 | 410，无任何写入 |
| 拒绝 | REJECTED，不建边，不计入任何称号 |
| 重复 activate | 只一条 usage_event，`already_activated` 幂等 |
| 历史使用冒充 | `received_at < edge.created_at` → 409（A09） |
| 并发双确认 | 一方 409，状态一致 |
| 撤回后重放 | 不复活、不复活计数 |
| 撤回不影响受助者主色 | `usage_event` 仍在（T12 说明） |
| 阈值边界 | count=2/3、3/9、10/29、30/99、99/100 两侧称号正确 |
