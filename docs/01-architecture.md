# 01 · V1.0 技术实施总方案

> 本文件回答三件事：用什么、为什么、模块之间怎么咬合。
> 所有选型都以 [00 环境审计](00-environment-audit.md) 的实测结果为前提，不以「社区流行」为依据。

---

## 1. 架构总览

```
┌──────────────────────────────────────────────────────────────────────────┐
│ 客户端层 (Clients)                                                        │
│                                                                          │
│  ┌────────────┐ ┌────────────┐ ┌──────────┐ ┌─────────┐ ┌──────────────┐ │
│  │ Desktop    │ │ Web (PWA)  │ │ iOS App  │ │ Android │ │ 微信小程序    │ │
│  │ mac/Win    │ │ 免安装接收  │ │ BLOCKED  │ │ BLOCKED │ │ 免注册观看    │ │
│  │ Tauri 2    │ │ React+Vite │ │ 待定     │ │ 待定    │ │ 原生 WXML    │ │
│  └─────┬──────┘ └─────┬──────┘ └────┬─────┘ └────┬────┘ └──────┬───────┘ │
│        │              │             │            │             │         │
│  ┌─────┴──────────────┴─────────────┴────────────┴─────────────┴──────┐  │
│  │ packages/protocol  —— 唯一事实来源（Zod schema → JSON Schema）      │  │
│  │ packages/model-registry —— 版本化家族字典（数据，非代码）           │  │
│  │ packages/visual-system —— 光点/光场渲染规则与设计 token             │  │
│  └──────────────────────────────┬────────────────────────────────────┘  │
└─────────────────────────────────┼───────────────────────────────────────┘
                                  │ HTTPS / Ed25519 签名 / SSE
┌─────────────────────────────────┼───────────────────────────────────────┐
│ 服务层 (services/)               ▼                                       │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐                   │
│  │ api (Hono)   │  │ realtime     │  │ aggregation  │                   │
│  │ 鉴权/幂等/限流│  │ SSE + NOTIFY │  │ k-匿名/合并   │                   │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘                   │
│         └─────────────────┴─────────────────┘                           │
│                    ┌──────────────────┐                                  │
│                    │ PostgreSQL       │  生产：标准 PG 16+               │
│                    │ 测试：PGlite     │  同一套 DDL                      │
│                    └──────────────────┘                                  │
└──────────────────────────────────────────────────────────────────────────┘
```

**分层不变量**：协议层不知道 UI 存在；服务层不知道渲染存在；适配器层不知道网络存在。

---

## 2. 技术选型：方案对比与理由

### 2.1 桌面端

| 方案 | 内存基线 | 透明置顶/非激活面板 | 本地扫描能力 | 结论 |
|---|---|---|---|---|
| **Tauri 2 + Rust** | 40–80 MB | ✅ 实测可编译（`tauri 2.12.5` + `objc2-app-kit`） | ✅ Rust 可安全只读打开他方 SQLite | **采用** |
| Electron | 180–320 MB | ⚠️ 可做但窗口笨重 | ✅ Node 可读 | 否决：直接违反 §14.3 的 <120 MB SLO |
| SwiftUI 原生 (macOS) | 30–60 MB | ✅ 最原生 | ✅ | 否决：Windows 需重写一遍，违背「统一协议 + 各自原生」的成本约束 |
| Wails (Go) | 50–90 MB | ✅ | ✅ | 否决：Go 在移动端复用更差，且团队/生态比 Rust 薄 |

**采用 Tauri 2**。核心理由不是「轻」，而是三条硬约束同时被满足：
SLO 内存、透明置顶窗口、以及能在同一套 Rust 代码里安全地做**只读 SQLite 证据采集**。

> 已实测：`tauri` 2.12.5 + `objc2-app-kit` 0.3 在 macOS 27 / arm64 上
> `cargo build` 通过，27.37s。这不是预期，是结果。

### 2.2 桌面 UI 框架

| 方案 | 理由 | 结论 |
|---|---|---|
| **React 19 + Vite 8** | 19.3.1 / 8.3.5 实测可用；Canvas/WebGL 示例与库生态最厚；Tauri 官方模板良好 | **采用** |
| Svelte 5 | 运行时更小，但光点/光场这类长驻动画的差异不明显（都是 Canvas 驱动） | 备选 |
| 原生 DOM + 手写 | 沉浸层四个视图的状态管理会很快失控 | 否决 |

关键判断：沉浸层主要是 **Canvas 渲染 + 少量 UI 壳**，框架差异对性能影响远小于渲染策略。
因此选影响「可维护性与招人」更大的 React。

### 2.3 协议定义（最重要的一项）

六端、三种语言（TS / Rust / Swift 或 Kotlin），最大风险是**协议漂移**。

| 方案 | 优点 | 缺点 | 结论 |
|---|---|---|---|
| **Zod 为源 → 导出 JSON Schema** | TS 类型自动推导；运行时可校验；JSON Schema 可作为跨语言契约与开源资产 | Rust/Swift 需手工镜像 | **采用** |
| Rust 为源 → 生成 TS | 单一权威 | TS 端开发者体验差，Web/小程序端无法直连 | 否决 |
| Protobuf | 强跨语言 | 对开源阅读者不友好；光路/光场是文档型数据，不适合强 schema 二进制 | 否决（可未来用于心跳批量） |
| JSON Schema 为源 | 语言中立 | 需要成熟的 TS 生成器链，运行时校验要另配 | 备选 |

**采用「Zod 为源」**，并加一道**契约夹具测试**：`packages/protocol/fixtures/*.json`
同时被 TS 测试与 Rust 测试读取，任一侧 schema 漂移则 CI 红。
这是把「文档写了」变成「机器保证」的唯一低成本手段。

### 2.4 后端

| 方案 | 结论 | 理由 |
|---|---|---|
| **Hono 4 + Node 24 + PostgreSQL 16+** | **采用** | 4.13.14 实测；体积小、TS 一流、中间件模型适合签名鉴权；未来可平移到边缘运行时 |
| Fastify | 备选 | 生态成熟，但类型体验与本项目「schema 即契约」的取向略逊 |
| Supabase（含 Realtime） | **暂不采用** | 需要外部账号与项目创建（当前无 CLI、无凭据）→ 会成为 F1 的人为阻塞；且把核心聚合逻辑绑到第三方 |
| 自建实时（SSE + LISTEN/NOTIFY） | **采用** | 文档 §11.2 允许「其他可验证的实时服务」；SSE 单向、够用、可缓存、易审计 |

实时通道结论：光脉是**服务端 → 客户端单向事件**，SSE 完全够用。WebSocket 的双向能力本项目用不上，
反而增加攻击面与运维成本。

### 2.5 本地存储与密钥

| 需求 | 方案 | 理由 |
|---|---|---|
| 本地光点库 | **rusqlite**（Rust 侧直连 SQLite 3.54） | 适配器与本地库同进程同语言；避免 IPC 往返与插件抽象泄漏 |
| 私钥存储 | **`keyring` crate** → macOS Keychain / Windows Credential Manager | 满足 §5.4「不进全局统计库、日志与传光载荷」 |
| 身份算法 | **Ed25519**（`ed25519-dalek`） | 短签名、公钥派生匿名 ID、无证书链负担 |
| 他方应用证据读取 | `rusqlite` + `SQLITE_OPEN_READ_ONLY` + URI `immutable=1` | 不锁用户数据库，不写回 |

> `immutable=1` 是审计中发现的关键细节：本机 `thread_history_1.sqlite`（638 MB）在常规模式下
> 无法打开（被应用持有）。只读 + immutable 是唯一不干扰用户、且可重复的打开方式。

### 2.6 视觉渲染

| 层 | 方案 | 理由 |
|---|---|---|
| 桌面悬浮光点 | Canvas2D + 极小窗口（约 120×120 逻辑 px） | 每帧只画光核+光晕，2D 足够且内存最省 |
| 沉浸层光核 | Canvas2D + 离屏画布 | 光晕用径向渐变 + 曝光叠加即可，不需要 WebGL |
| 全球光场 | WebGL（仅当节点数 > 3000 才启用），否则 Canvas2D | 渐进增强，低端机退化 SVG/CSS |
| 降动效模式 | 全部动画走统一 `motion` 令牌，`prefers-reduced-motion` + 应用内开关双通道 | §4.4 硬要求 |

**否决「一上来 WebGL」**：桌面常驻窗口的 SLO 是 CPU <1%、内存 <120 MB，
WebGL 上下文的长驻成本与 GPU 唤醒都会破坏这个目标。光场是**打开时才存在**的视图，才配得上 WebGL。

---

## 3. 模块依赖关系（严格单向）

```
                    ┌────────────────────────────┐
                    │  packages/protocol         │  ← 无依赖（纯 schema + 编解码）
                    └────────────┬───────────────┘
                                 │
              ┌──────────────────┼──────────────────┐
              ▼                  ▼                  ▼
   ┌──────────────────┐ ┌────────────────┐ ┌──────────────────┐
   │ packages/        │ │ packages/      │ │ crates/          │
   │ model-registry   │ │ visual-system  │ │ light-core       │
   │ (数据+归一化)     │ │ (设计 token)   │ │ (身份/证据/状态) │
   └────────┬─────────┘ └────────┬───────┘ └────────┬─────────┘
            │                    │                  │
            │                    │        ┌─────────┴──────────┐
            │                    │        ▼                    ▼
            │                    │  ┌───────────────┐  ┌────────────────┐
            │                    │  │ crates/       │  │ crates/        │
            │                    │  │ local-detector│  │ adapter-sdk    │
            │                    │  │ (扫描调度)     │  │ (allowlist 契约)│
            │                    │  └───────┬───────┘  └────────┬───────┘
            │                    │          │                   │
            │                    │          ▼                   ▼
            │                    │      ┌───────────────────────────┐
            │                    │      │ adapters/{codex,qwen-code,│
            │                    │      │ ollama,custom-api,...}    │
            │                    │      └─────────────┬─────────────┘
            ▼                    ▼                    ▼
   ┌──────────────────────────────────────────────────────────┐
   │  apps/desktop (Tauri: 前端 UI + Rust command 层)          │
   │  apps/web                                                 │
   └──────────────────────────┬───────────────────────────────┘
                              ▼
   ┌──────────────────────────────────────────────────────────┐
   │  services/api → services/aggregation → services/realtime  │
   └──────────────────────────────────────────────────────────┘
```

**禁止的依赖方向**（CI 静态检查）：

1. `protocol` 不得 import 任何 UI 或服务端代码。
2. `adapters/*` 不得联网、不得 import `services/*`（适配器只产出本地事件）。
3. `visual-system` 不得 import 业务状态。
4. `light-core` 保持纯函数：给定相同输入产出相同光点状态（可测性前提）。
5. 服务层不得 import `crates/*`（避免把客户端逻辑泄漏进服务端）。

---

## 4. 关键架构决策记录（ADR）

### ADR-001 · 适配器证据边界是**列级 allowlist**
**背景**：文档 §5.4、§12.3 禁止读取聊天正文，但实测 Codex 的元数据与正文在同一个 SQLite 里。
**决策**：每个适配器声明 `AdapterManifest`，显式列出 `allow` 列与 `deny` 列；
`adapter-sdk` 包装所有查询，凡触达未声明列的调用在 debug 构建 panic、CI 直接失败。
**后果**：新增适配器必须写清单；换来的是「禁读」从口号变成可执行约束。→ 见 [02 §G-01](02-requirements-traceability.md)

### ADR-002 · 模型家族**只查表**，不做字符串推断
**背景**：实测 8 个字符串仅 1 个真实家族；`codex-auto-review` 是任务标签。
**决策**：注册表是带 `disposition` 的数据文件（`FAMILY` / `TASK_LABEL` / `UNKNOWN`）；
匹配按显式别名从长到短优先；未命中一律 `UNKNOWN` 且**不计数、不猜**。
**后果**：注册表成为需要版本化维护的资产（这正是文档 §16 要求的）。

### ADR-003 · 生产 Postgres，测试/本地 PGlite，同一 DDL
**背景**：本机无 Docker。
**决策**：迁移脚本只写标准 PG SQL；CI 与本地用 PGlite 执行同一份迁移。
**后果**：不允许使用 PG 扩展（如 PostGIS）；地区用自建网格编码，见 [03 文档](03-data-model.md)。

### ADR-004 · 实时用 SSE，不用 WebSocket / Supabase
见 §2.4。**后果**：光脉必须设计为幂等、可重放、带 `Last-Event-ID`。

### ADR-005 · 桌面先做，移动端技术栈**不在 F0 承诺**
**背景**：无 Xcode、无 Android SDK；iOS Widget 与 NFC 本来就需要 Swift 原生代码。
**决策**：F0 明确「Tauri 能否复用为移动外壳」为**待验证**，不写进 V1.0 承诺；
协议先行，任何客户端只要说同一套协议即可接入。
**后果**：避免为了「六端」而做出无法验收的架构承诺。

### ADR-006 · 品牌名不进代码标识
**背景**：§3.1 明确正式品牌未定，不得固化。
**决策**：代码标识统一用中性代号 `firstlight`；用户可见品牌名走 `packages/visual-system/brand.json` 单一配置。
**后果**：改名只需改一处 + 换图标，不触碰 bundle id 之外的逻辑。

---

## 5. 目录与工程结构（建议落地形态）

```
first-light/
  apps/
    desktop/                 # Tauri 2
      src/                   # React 19 前端
      src-tauri/             # Rust: commands, tray, floating window
      src-tauri/src/windows/light_point.rs
    web/                     # React + Vite（免安装接收页 + 光场观看）
    wechat-mini/             # 原生小程序
    mobile/                  # F5 再定，先占位不写码
  services/
    api/                     # Hono
    realtime/                # SSE
    aggregation/             # 区域聚合 / k-匿名 / 抑制
  crates/
    light-core/              # 身份、证据评级、光点状态、状态机
    local-detector/          # 扫描调度、路径白名单
    adapter-sdk/             # AdapterManifest、列级 allowlist 执行器
  adapters/
    codex/  qwen-code/  ollama/  custom-api/
  packages/
    protocol/                # Zod → JSON Schema + fixtures
    model-registry/          # registry 数据 + 归一化函数
    visual-system/           # token、色阶、动效曲线、brand.json
  hardware/nfc-tag/          # NDEF 写入模板与制作指引
  docs/                      # 本目录
  tests/e2e/
```

> ⚠️ 与文档 §11.3 的差异：文档把 `apps/mobile` 列为 V1.0 目录。
> 本方案保留占位但**不在 F0 创建可构建工程**，理由见 ADR-005。这是自觉的偏离，已标注。

---

## 6. 数据流（一次真实点亮的完整路径）

```
1. 本地扫描      local-detector 发现 ~/.codex/state_5.sqlite
                     │  (AdapterManifest: allow=[id,model,model_provider,created_at])
2. 证据生成      adapters/codex → UsageEvidence{level:E3, family:gpt, event_id}
                     │  (light-core 用唯一键 identity+family 去重 → 只点亮一次)
3. 本地持久化    本地 SQLite 写入 evidence 与 light_state；界面立即变深（离线也成立）
4. 上报（可选）  仅在用户已授权贡献时：POST /v1/usage/attest（签名 + Idempotency-Key）
5. 聚合          服务端写 usage_event（unique(identity,family)）→ 触发 NOTIFY
6. 光脉广播      realtime 过滤/合并/限频（≤2 次/分钟）→ SSE 给订阅者
7. 区域聚合      aggregation 维护 region_aggregate，低于阈值(20) 的平台份合并到上级
```

**关键不变量**：第 3 步之后，用户看到的一切在**离线**状态下已经完整成立。
联网只影响公共光场，不影响「我的光」——这是 §1.3「可仅本地使用」的架构级保证。

---

## 7. 与产品文档的自觉偏离（必须先说清）

| 文档原文 | 本方案 | 理由 |
|---|---|---|
| §11.2 `Mobile: Tauri 2 或原生壳` | F0 不承诺，推迟至 F5 决策 | 无 Xcode/Android SDK，且 Widget/NFC 需原生；先承诺等于造假 |
| §11.2 `Realtime: Supabase 或其他` | 自建 SSE | 避免外部账号阻塞 F1；单向事件够用 |
| §11.3 目录含 `apps/mobile/` | 仅占位 | 同上 |
| §6.1 知光称号按「已验证家族数」 | 明确 **E2 自主确认不计入知光称号** | §5.3 规定 E2 不进全球可信数；称号若计入会自相矛盾 → 需你确认（[D-03](09-phase0-next-actions.md)） |

---

## 8. 部署拓扑（V1.0 建议）

```
                    Cloudflare / CDN（静态 Web + 接收页）
                              │
        ┌─────────────────────┼──────────────────────┐
        ▼                     ▼                      ▼
   api.example.com      field.example.com      events.example.com
   (Hono, 2 实例)       (聚合读取, 可缓存)      (SSE, 长连接)
        └─────────────────────┴──────────────────────┘
                              ▼
                     PostgreSQL 16（主 + 只读副本）
                              ▼
                     对象存储（导出包，短期保留）
```

对外必需域名清单与「BLOCKED 依赖」见 [08 文档](08-risks-dependencies.md)。
