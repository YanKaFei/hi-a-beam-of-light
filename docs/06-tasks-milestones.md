# 06 · 任务分解、里程碑与追踪表

> 状态口径见 [README](../README.md#状态口径)。任务升状态必须附证据（命令 + 输出 / 真机记录）。
> **当前全表除 F0 完成项外，一律 `PLANNED`。**

---

## 1. 里程碑映射（消除文档与总控指令的命名冲突，[G-12](02-requirements-traceability.md)）

| 本方案 | 产品文档 §13.2 | 交付核心 | 出口条件（可验收） |
|---|---|---|---|
| **F0** 审计与设计 | F0 视觉与协议冻结 | 本目录全部文档 | 设计冻结；7 项决策确认 |
| **F1** 统一核心协议 | （并入 F0） | `crates/light-core`、`packages/protocol`、`model-registry` | 协议测试全绿；E2/E3 隔离被测试证明 |
| **F2** 桌面端 | F1 本地光点 | macOS 悬浮光点 + 扫描 + 本地光界面 | A01–A05 在 macOS 上 VERIFIED |
| **F3** 传光闭环 | F2 传播闭环 + F3 NFC | Web 接收、二维码、NFC 入口、双向确认、激活、回响 | A06–A10、A13 VERIFIED（**A08 NFC 需硬件**） |
| **F4** 全球光场 | F4 区域光场 | 聚合、k-匿名、SSE 光脉、可视化 | A11、A15 VERIFIED |
| **F5** 移动端与小程序 | F5 全平台入口 | iOS / Android / 微信小程序 | 兼容矩阵通过；**当前 BLOCKED** |
| **F6** 发布验收 | GA V1.0 | 安全审计、性能、隐私撤回、安装包、文档 | A12、A14 VERIFIED；开源 README |

> 顺序上的一个自觉调整：文档把 NFC 放在传播闭环之后（F3）。
> 本方案把 **NFC 入口的"协议与写入模板"** 留在 F3，但**把真机验收独立标为 BLOCKED**，
> 不让二维码兜底通过验证的闭环被 NFC 硬件卡住。这是为了让 F3 可以真正结束。

---

## 2. 任务表

优先级：**P0** 不做就无法验收 V1.0｜**P1** 显著影响体验/扩展性｜**P2** 优化项

### F1 统一核心协议（可立即开工，无外部依赖）

| ID | 任务 | 模块 | P | 依赖 | 证据要求 | 状态 |
|---|---|---|---|---|---|---|
| F1-01 | monorepo 骨架：pnpm workspaces + Cargo workspace + TS 配置 | 根 | P0 | — | `pnpm -r build` 通过 | PLANNED |
| F1-02 | `packages/protocol`：Zod schema（身份/证据/会话/边/区聚合） | protocol | P0 | F1-01 | `vitest` 覆盖每个 schema | PLANNED |
| F1-03 | 导出 JSON Schema + `fixtures/*.json` | protocol | P0 | F1-02 | schema 与 fixture 双向校验 | PLANNED |
| F1-04 | `crates/light-core`：Ed25519 身份生成、anon_id 派生 | light-core | P0 | F1-01 | 单测：同种子同结果 | PLANNED |
| F1-05 | 密钥轮换 / 恢复凭据签发（[G-10](02-requirements-traceability.md)） | light-core | P0 | F1-04 | 轮换后旧 epoch 验签失败 | PLANNED |
| F1-06 | 证据分级与晋升规则（E0–E4，E2 隔离） | light-core | P0 | F1-04 | 表的每个转移一个测试 | PLANNED |
| F1-07 | 光点状态派生纯函数（色阶 + 双称号 + 边界值） | light-core | P0 | — | 边界 2/3、3/9、99/100 fixture | PLANNED |
| F1-08 | `model-registry`：数据文件 + 归一化函数 + 版本 | registry | P0 | F1-02 | 真实样本：8 字符串 → 1 家族 | PLANNED |
| F1-09 | `disposition` 处理（TASK_LABEL / UNKNOWN 不计数） | registry | P0 | F1-08 | `codex-auto-review` 不产生家族 | PLANNED |
| F1-10 | 传光状态机实现（纯函数转移表） | light-core | P0 | F1-02 | [05 §7](05-state-machines.md) 全部用例 | PLANNED |
| F1-11 | 契约夹具双向测试（TS ↔ Rust） | protocol | P0 | F1-03,F1-04 | CI 中两侧读同一 fixtures | PLANNED |
| F1-12 | `adapter-sdk`：AdapterManifest + 列级 allowlist 执行器（[G-01](02-requirements-traceability.md)） | adapter-sdk | P0 | F1-01 | 越界列在 debug 下 panic | PLANNED |

### F2 桌面端

| ID | 任务 | 模块 | P | 依赖 | 证据要求 | 状态 |
|---|---|---|---|---|---|---|
| F2-01 | Tauri 2 应用骨架 + mac 构建（含 crates 镜像配置） | desktop | P0 | F1-01 | `cargo tauri build` 产出 .app | PLANNED |
| F2-02 | 透明置顶悬浮窗 + 点击穿透 + 托盘 + 拖动 | desktop | P0 | F2-01 | 真机录屏 + 内存实测（T-01） | PLANNED |
| F2-03 | 非激活面板（不抢焦点，`NSPanel`） | desktop | P0 | F2-02 | 点击光点时前台 App 不失焦 | PLANNED |
| F2-04 | 悬浮光点渲染（呼吸/光晕/Reduce Motion） | visual-system | P0 | F2-02 | CPU<1%、内存<120MB 实测 | PLANNED |
| F2-05 | 本地 SQLite（本地库 schema + 迁移） | desktop | P0 | F1-04 | 迁移可重入 | PLANNED |
| F2-06 | 扫描调度器 + 路径白名单 + 用户授权门 | local-detector | P0 | F1-12 | 未授权时零文件访问 | PLANNED |
| F2-07 | `adapters/codex`（只读 immutable 打开，只读 allow 列） | adapters | P0 | F1-12 | A02/A03/A04 通过 | PLANNED |
| F2-08 | `adapters/custom-api`（OpenAI-compatible 配置发现，仅 E1） | adapters | P1 | F1-12 | A05：配置成功不加深主色 | PLANNED |
| F2-09 | `adapters/ollama`（本地模型标识 → 家族，T-06） | adapters | P1 | F1-12 | 安装后实测 | PLANNED |
| F2-10 | 沉浸层「我的光」界面（光核 + 双称号 + 证据列表 + 隐私状态） | desktop | P0 | F2-04 | 3 秒内理解自身状态（§10.2） | PLANNED |
| F2-11 | 设置页：扫描范围、联网授权、动效、数据管理 | desktop | P0 | F2-05 | 每项开关有实际效果 | PLANNED |
| F2-12 | 安静模式 / 全屏检测 / 开机启动 | desktop | P1 | F2-02 | 全屏时不打扰 | PLANNED |
| F2-13 | Windows 构建与透明窗验证 | desktop | P0 | F2-02 | **BLOCKED：无 Windows 硬件** | BLOCKED |

### F3 真实传光闭环

| ID | 任务 | 模块 | P | 依赖 | 证据要求 | 状态 |
|---|---|---|---|---|---|---|
| F3-01 | `services/api` 骨架 + 签名鉴权 + nonce 防重放 | api | P0 | F1-02 | 重放用例 401 | PLANNED |
| F3-02 | Postgres 迁移 + PGlite 测试夹具 | api | P0 | F3-01 | 同一迁移在 PGlite 跑通 | PLANNED |
| F3-03 | `/identities/*`、`/usage/attest`（含幂等与去重） | api | P0 | F3-02 | 连发 10 次只 +1 | PLANNED |
| F3-04 | `/transmissions` 全套（创建/接收/确认/取消/拒绝/过期） | api | P0 | F3-02 | 状态机全用例 | PLANNED |
| F3-05 | `/transmissions/{id}/activate` 五条校验 | api | P0 | F3-04 | A09/A10 通过 | PLANNED |
| F3-06 | Web 接收页 `apps/web`（无 App、无注册、可拒绝） | web | P0 | F3-04 | 手机浏览器实测（A06） | PLANNED |
| F3-07 | 动态二维码生成与轮换（限时失效） | desktop+web | P0 | F3-04 | 过期后扫码 410 | PLANNED |
| F3-08 | 五秒仪式动效 + 门控函数（[05 §6](05-state-machines.md)） | visual-system | P0 | F3-05 | 拒绝/失败不播成功（A07） | PLANNED |
| F3-09 | 回响送达与一次性补发 | desktop+api | P0 | F3-05 | 离线重连后仅一次 | PLANNED |
| F3-10 | 光路视图（出/入/代际/激活状态/回响历史） | desktop | P0 | F3-09 | 默认只看自己的边 | PLANNED |
| F3-11 | NFC NDEF 写入模板 + 制作指引 | hardware | P1 | F3-04 | 手机触碰打开接收页 | **BLOCKED：无标签/读卡硬件** |
| F3-12 | 近场（BLE/Nearby）可行性验证 | mobile | P2 | — | **BLOCKED：移动端** | BLOCKED |

### F4 全球光场

| ID | 任务 | 模块 | P | 依赖 | 证据要求 | 状态 |
|---|---|---|---|---|---|---|
| F4-01 | `services/aggregation`：区域聚合 + k-匿名(20) + 抑制 | aggregation | P0 | F3-02 | A15 通过 | PLANNED |
| F4-02 | `services/realtime`：SSE + NOTIFY + 限频合并 | realtime | P0 | F3-02 | ≤2 条/分钟 压测 | PLANNED |
| F4-03 | `/field/summary`（三口径分离 + 方法说明） | api | P0 | F4-01 | 三指标不可合并 | PLANNED |
| F4-04 | `/field/regions`（离散密度档，无坐标） | api | P0 | F4-01 | 响应无 lat/lng | PLANNED |
| F4-05 | 光场可视化（Canvas2D，>3000 节点才 WebGL） | visual-system | P0 | F4-04 | 低端设备降级 | PLANNED |
| F4-06 | 空状态设计（真实无数据时优雅且诚实） | desktop+web | P0 | F4-05 | 零数据不造假 | PLANNED |
| F4-07 | 区域切换视图 + 静观模式（无身份观看） | desktop+web | P1 | F4-04 | 无身份可看 | PLANNED |
| F4-08 | 心跳 `/presence/heartbeat` + 在线实例去重 | api | P1 | F3-02 | 在线数 ≠ 自然人数标注 | PLANNED |

### F5 移动端与小程序（整体 BLOCKED）

| ID | 任务 | 模块 | P | 依赖 | 阻塞 |
|---|---|---|---|---|---|
| F5-01 | iOS App 骨架 + Widget Extension | mobile | P0 | — | **无 Xcode** |
| F5-02 | Widget 刷新预算实测（T-03、A14） | mobile | P0 | F5-01 | **无 Xcode** |
| F5-03 | iOS NFC 读取（Core NFC） | mobile | P0 | F5-01 | **无 Xcode + 无标签** |
| F5-04 | Android App + 悬浮窗权限流程 | mobile | P0 | — | **无 SDK/NDK** |
| F5-05 | 跨设备身份恢复（配对/恢复短语） | mobile+core | P0 | F1-05 | 依赖 F5-01 |
| F5-06 | 微信小程序：免注册观看 + 扫码接收 + 传光 | wechat | P0 | F3-04 | **无 AppID** |
| F5-07 | 小程序降级矩阵实测（G-14） | wechat | P0 | F5-06 | **无 AppID** |

### F6 发布验收

| ID | 任务 | P | 证据 |
|---|---|---|---|
| F6-01 | 全平台端到端测试（A01–A15） | P0 | 测试报告 |
| F6-02 | 安全审计（签名、重放、越权、依赖漏洞） | P0 | 审计报告 |
| F6-03 | 性能验收（CPU/内存/冷启动/光脉压力） | P0 | 实测曲线 |
| F6-04 | 隐私撤回全链路测试（A12） | P0 | 撤回前后对比 |
| F6-05 | 无障碍验收（低动态/静音/键盘/对比度） | P0 | 辅助功能测试记录 |
| F6-06 | 安装包与签名（macOS 公证 / Windows 签名） | P0 | 可安装产物 |
| F6-07 | 开源 README / 贡献指南 / 适配器提交规范 | P0 | 仓库文档 |
| F6-08 | 合规材料（备案、隐私政策、数据处理说明） | P0 | **需运营主体** |

---

## 3. 验收用例 → 任务映射

| 用例 | 覆盖任务 | 平台可验证性 |
|---|---|---|
| A01 新用户不注册启动，生成光种显示沉睡 | F1-04, F2-01, F2-04, F2-05 | macOS ✅ |
| A02 扫描发现 Codex 只显示「已发现」 | F2-06, F2-07 | macOS ✅ |
| A03 GPT 由两客户端使用只计一次 | F1-08, F2-07, F3-03 | macOS ✅ |
| A04 客户端内调 DeepSeek 归 DeepSeek 家族 | F1-08, F2-07 | 需真实 DeepSeek 调用 |
| A05 配置 API 未成功调用 → 已连接不加深 | F2-08 | macOS ✅ |
| A06 A 生成二维码，B 无 App 扫码可拒绝 | F3-04, F3-06, F3-07 | ✅ 需第二台设备 |
| A07 未双向确认不建关系不播成功 | F3-04, F3-08 | ✅ |
| A08 iPhone/Android 读 NFC 打开安全 URL | F3-11 | ❌ **BLOCKED** |
| A09 收到光但未使用 → 不触发启蒙计数 | F3-05 | ✅ |
| A10 后续首次激活 → 内亮点亮 + 回响只一次 | F3-05, F3-09 | ✅ |
| A11 拒绝贡献 → 本地可用、不汇报 | F4-01, `consent` | ✅ |
| A12 撤回并删除可关联事件 | F6-04 | ✅ |
| A13 弱网/重复提交/伪造会话不重复计数 | F3-01, F3-03 | ✅ |
| A14 iOS Widget 不能即时刷新时不伪称实时 | F5-02 | ❌ **BLOCKED** |
| A15 小地区不足阈值合并到上级 | F4-01 | ✅ |

**结论：15 个验收用例中 11 个可在当前环境完成，4 个 BLOCKED（A04 部分、A08、A14、以及 iOS/Android 相关）。**

---

## 4. 关键路径

```
F1-02/04 (协议+身份)
   └─► F1-08/10 (注册表+状态机)
         └─► F2-01/02 (Tauri 悬浮窗)
               └─► F2-06/07 (扫描+codex 适配器)   ← A01–A05 达成，第一个「可演示的真产品」
                     └─► F3-01/04 (API+会话)
                           └─► F3-06/07 (Web 接收+二维码)
                                 └─► F3-05/09 (激活+回响)  ← A06–A13 达成，V1.0 主路径闭环
                                       └─► F4-01/05 (聚合+光场)
```

**第一个有意义的中途交付点 = F2 结束**：那时已经有一颗真实变深的桌面光点，
不依赖任何服务端、不依赖任何账号。这本身就是产品宣言的可运行证明。
