# 00 · 环境审计报告（F0）

审计时间：2026-10-31
审计对象：当前工作区 `/Users/coffee/Desktop/创造一切可能` 与所在主机
审计方式：只读探测 + 可重跑探针（见 `feasibility/`）
结论先行：**宿主环境足以支撑桌面端与 Web 端开工；iOS、Android、NFC 真机、微信上架四项 BLOCKED。**

---

## 1. 仓库审计：结论是「没有仓库」

这是本阶段最重要的一条事实，必须先讲清楚。

| 检查项 | 实测结果 |
|---|---|
| 工作区根目录是否为 git 仓库 | **否**（无 `.git`） |
| 是否存在名为 first-light / 第一束光 的现有工程 | **否**（全库检索 0 命中） |
| 根目录 `package.json` 内容 | 仅 `pdf-lib`、`pdfjs-dist`，属于历史报告生成用途，与本项目无关 |
| 根目录 `node_modules` | 仅上述两个依赖的传递树 |
| 子目录性质 | 40+ 个互不相关的项目/临时目录（ARTEX、flowback、SmartStaff-AI、剧本杀、报告构建等） |
| 现存 git 仓库 | 4 个，均为其他项目：`ascii-h3-director`、`flowback`、`SmartStaff-AI`、`文字人格系统` |

**判定：本项目是 greenfield。** 不存在「延续现有技术栈」的约束，也不存在需要兼容的历史代码。
因此「不要擅自替换技术栈」这一条在本项目中的含义是：
**不要为了套用工作区里其他项目的习惯（它们混用 Python/Node/无构建），而放弃产品文档 §11.2 的技术方向。**

⚠️ 同时必须提醒：工作区根目录是**共享草稿区**，不适合直接当产品仓库。
建议新建独立仓库目录并在其中 `git init`（见 [09 文档](09-phase0-next-actions.md) 决策 D-01）。

---

## 2. 工具链实测

| 组件 | 实测版本 | 状态 | 备注 |
|---|---|---|---|
| macOS | 27.0.1 (26A434), arm64 | ✅ | Apple Silicon；Windows 无法在本机验证 |
| Node.js | v24.19.0 | ✅ | 满足 Tauri 2 / Vite 8 要求 |
| npm | 11.17.0 | ✅ | registry 已指向 `registry.npmmirror.com` |
| pnpm | 12.4.1 | ✅ | 建议作为 monorepo 包管理器 |
| Rust / cargo | 1.98.1 (48a229cea) | ✅ | 仅装 `aarch64-apple-darwin` 一个 target |
| git | 2.50.1 (Apple Git-155) | ✅ | |
| SQLite | 3.54.0 | ✅ | 本地光点库直接可用 |
| Google Chrome | 已安装 | ✅ | journal-page / 截图类验证可用 |
| 微信开发者工具 | `wechatwebdevtools.app` 已安装 | ⚠️ | 工具在，但小程序需真实 AppID 才能联调 |
| Python | 3.9.6（系统） | ⚠️ | 系统自带，**无 pip**；`.venv312` 内亦无 pip。仅影响脚本类工作，不影响产品 |
| 磁盘 | 314 GiB 可用 | ✅ | |

### 2.1 缺失项（直接导致 BLOCKED）

| 缺失 | 影响 | 结论 |
|---|---|---|
| **Xcode**（仅有 Command Line Tools） | 无法编译 iOS App、Widget Extension、无法生成 AASA 关联文件、无法真机签名 | **F5-iOS = BLOCKED** |
| **Android SDK / NDK** | 无 `ANDROID_HOME`，无 adb，无法编译 APK | **F5-Android = BLOCKED** |
| **Docker / Podman** | 无法用容器跑 Postgres、无法跑 Supabase 本地栈 | 已用 PGlite 绕过（见 §4） |
| Homebrew | 无包管理器，安装 Postgres/ntfy 等需手动下载 | 降级为「不依赖 brew」的方案 |
| `gh` / `supabase` / `vercel` / `wrangler` CLI | 无法自动建仓库、无法一键部署 | 部署需人工或稍后补装 |
| NFC 读写硬件 + 实体标签 | 无法做 A08 真机触碰 | **F3-NFC = BLOCKED** |

---

## 3. 网络与依赖可达性（决定构建可行性）

| 目标 | 直连结果 | 可用替代 |
|---|---|---|
| `registry.npmjs.org` | **200** | 实际用 `registry.npmmirror.com`（已配置） |
| `crates.io` | **000（不可达）** | ✅ `rsproxy.cn` / `mirrors.tuna.tsinghua.edu.cn` / `mirrors.ustc.edu.cn` 均 200 |
| `github.com` | **200** | 可直接拉取 |

**结论：Rust 生态必须走镜像，否则 Tauri 完全无法构建。** 这是本项目的硬性构建前提，必须写进
`docs/install.md` 与 CI 配置，否则任何新成员/CI 机器都会构建失败。

---

## 4. 可行性探针（真跑，不是推断）

### 4.1 Tauri 2 + macOS 原生窗口层 —— ✅ 通过

文档 §11.2 推荐 Tauri，但「推荐」不等于「能构建」。实测：

```
依赖: tauri = "2" (features: tray-icon, image-png)
      objc2-app-kit = "0.3" (features: NSPanel, NSWindow, NSResponder)
解析: tauri 2.12.5 ✓
编译: Finished `dev` profile in 27.37s   BUILD_EXIT=0
```

这一步的意义：桌面光点需要的**透明置顶窗口 + 托盘 + 非激活面板**（`NSPanel`，避免点击光点抢焦点）
在 macOS 27 上真实可编译。`objc2-app-kit` 可用，意味着悬浮光点的系统级行为不必退回 Electron。

### 4.2 无 Docker 的 Postgres —— ✅ 通过（PGlite）

本机无 Docker、无 Postgres、无 psql。实测 `@electric-sql/pglite@0.5.13`：

```sql
create table evidence(id text primary key, family text, level int);
insert … on conflict do nothing;   -- 执行两次
-- 结果: PGLITE_OK {"c":2,"f":2}   唯一约束、事务、RETURNING 均正常
```

意义：**后端开发与自动化测试不需要等待任何基础设施**。测试跑真实 Postgres 语义，
生产部署仍用标准 Postgres（同一套 DDL），避免了「测试用内存 mock、上线才发现 SQL 不兼容」这类常见事故。

### 4.3 模型注册表归一化 —— ⚠️ 暴露产品级错误（见下节）

离线探针把真实观测到的模型字符串按注册表规则归一化（`bash feasibility/probes/run-registry-probe.sh`）：

```
distinct 取值（含空串）: 9
非空原始字符串:         8
naive 去重计数:         9  ← 会被判成「拓界者」(≥7)
真实家族数:             1  ← 正确答案「初明者」(1)
必须不计数:             2  ← codex-auto-review（任务标签）+ 空串
```

详见 [02 文档 G-02](02-requirements-traceability.md)。

---

## 5. 真实 AI 使用识别面（M2 的地基）

这一节是只读审计，**未读取任何对话正文、提示词、API Key 或 Cookie**，只枚举了路径、表名、列名。

### 5.1 本机现有识别目标

| 目标 | 实测 | 能力等级 |
|---|---|---|
| PATH 上的 `codex` / `claude` / `qwen` / `ollama` / `gemini` | **全部不存在** | — |
| `~/.codex/` | **存在，4.7 GB**，含 sqlite 账本 | ✅ 可达 E3 |
| `~/.cursor/` | 存在（仅 extensions + argv.json，无使用账本） | 最多 E0 |
| `~/.claude/` `~/.qwen/` `~/.ollama/` `~/.gemini/` | 不存在 | — |

> 注：`~/.cursor/extensions/` 内有 `saoudrizwan.claude-dev`（Cline）扩展，
> **暗示**曾通过 Cline 使用 Claude，但目录内无账本可核验 → 按规则只能是 E0，不得计入。

### 5.2 为什么这是 E3 的可行路径

`~/.codex/state_5.sqlite → threads` 表结构（仅列名）：

```
id, rollout_path, created_at, updated_at, source, model_provider, model,
tokens_used, sandbox_policy, approval_mode, archived, cli_version,
first_user_message, preview, title, …
```

- `model` / `model_provider` / `created_at` → **元数据，可作为 E3 证据**
- `first_user_message` / `preview` / `title` → **内容，永久禁读**
- `tokens_used` → 可读，但**不得用于任何评分、称号或上传**（否则违反 §1.5「不按 API 消费额评定人的价值」）

真实数据分布（仅 model 与计数）：

| model_provider | model | threads |
|---|---|---|
| openai | `codex-auto-review` | 203 |
| openai | `gpt-6-sol` | 47 |
| openai | `gpt-5.6-sol` | 30 |
| openai | `gpt-5.6-terra` | 29 |
| openai | `gpt-6.1-sol` | 21 |
| openai | `gpt-6-astra` | 12 |
| openai | `chatgpt-web/gpt-6-sol` | 2 |
| openai | `chatgpt-web/gpt-5.6-sol` | 1 |
| openai | *(空字符串)* | 2 |

### 5.3 三个必须写进协议的结论

1. **同一 SQLite 文件里元数据与内容混放** → 证据边界必须是列级 allowlist（G-01）。
2. **`codex-auto-review` 是任务标签，不是模型** → 注册表需要 `disposition: TASK_LABEL` 处置，而非强行归类（G-02）。
3. **`chatgpt-web/gpt-6-sol` 把客户端嵌进了模型名** → 证明 `client` 与 `model_family` 必须分列存储（§5.2 要求），
   且解析出来的是 `client=chatgpt-web, family=gpt`，不是新家族。

### 5.4 反面教材：禁读面

| 路径 | 大小 | 为什么禁读 |
|---|---|---|
| `thread_history_1.sqlite → thread_items.item_json` | 638 MB | 完整对话正文 |
| `logs_2.sqlite → logs.feedback_log_body` | 82 MB | 可能含用户输入片段 |
| `~/.codex/sessions/**/rollout-*.jsonl` | — | 完整会话回放 |

适配器对这些表只允许 `SELECT count(*)` 级别的存在性判断，**不得 `SELECT` 正文列**。

---

## 6. 环境层约束（写代码前必须知道）

本会话文件沙箱为 `workspace-write`，以下目录**只读**，会导致构建失败：

| 目录 | 现象 | 规避 |
|---|---|---|
| `~/.npm/_cacache` | `EPERM`，npm 全命令失败 | `export npm_config_cache=<workspace>/.tmpdeps/npm-cache` |
| `~/.cargo/registry` | `Operation not permitted (os error 1)` | `export CARGO_HOME=<workspace>/.tmpdeps/cargo-home` |

这不是产品缺陷，而是**开发环境配置项**。必须写进 `docs/install.md`，否则复现失败。
CI 与容器环境不受此限。

---

## 7. 审计结论

| 结论 | 依据 |
|---|---|
| 项目为 greenfield，需新建仓库 | §1 |
| 桌面端（macOS）可立即开工 | §2、§4.1 |
| Web 端可立即开工 | §2 Node/Chrome |
| 后端可立即开工，无需 Docker | §4.2 PGlite |
| M2 适配器至少有一条真实 E3 通路 | §5.2 |
| iOS / Android / NFC 真机 / 微信上架 四项 BLOCKED | §2.1 |
| Rust 构建必须配置 crates 镜像 | §3 |
| 「不读聊天正文」需升级为列级 allowlist 规范 | §5.3 |

### 本次审计留下的构建缓存

| 路径 | 大小 | 说明 |
|---|---|---|
| `.tmpdeps/cargo-home/` | 658 MB | Tauri 探针下载的 crate 源码缓存，F1/F2 开发可直接复用 |
| `.tmpdeps/npm-cache/` | 68 MB | npm 缓存（`~/.npm` 在沙箱外不可写，故重定向至此） |

两者都是纯缓存，可安全删除；删除后需重新下载才能构建。

下一步：进入 [01 技术实施总方案](01-architecture.md)。
