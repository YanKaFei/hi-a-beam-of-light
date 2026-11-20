# F2 技术准备清单

F1 已完成核心规则与隐私边界。F2 要做的是**让它成为一颗真的光点**。
本文件只列「开工前必须知道的事」，不是任务表（任务表见 [06-tasks-milestones](../06-tasks-milestones.md)）

---

## 1. 第一天就做：T-01 真机形态验证（最高优先）

**在写任何界面之前**，用最小可运行窗口确认四件事。任何一件不成立，
整个桌面形态都要重做，所以必须最先验证。

| 验证点 | 判据 | 若失败的退路 |
|---|---|---|
| 透明 + 无边框窗口 | 背景真正透明，无标题栏、无白底 | 只做托盘图标 + 菜单（体验降级但可用） |
| 始终置顶 | 覆盖普通窗口；全屏应用中的表现需记录 | 仅在桌面层置顶 |
| 点击穿透（安静模式） | `ignore_cursor_events` 生效，下方应用可操作 | 提供"收起为托盘"替代 |
| **非激活面板**（点击光点不抢焦点） | 点击光点后，原来的前台 App 仍是前台 | 用 `NSPanel` + `nonactivatingPanel`；若仍失败，改为托盘菜单触发 |

已确认的前提：`tauri 2.12.5` + `objc2-app-kit 0.3` 在 macOS 27 / arm64 上
`cargo build` 通过（F0 实测 27.37s）。

### 关键依赖与 feature
```toml
tauri = { version = "2", features = ["tray-icon", "image-png"] }
objc2-app-kit = { version = "0.3", features = ["NSPanel", "NSWindow", "NSResponder"] }
tauri-plugin-autostart     # 开机启动
tauri-plugin-single-instance
```

### 窗口配置要点（`tauri.conf.json`）
```json
{
  "transparent": true,
  "decorations": false,
  "alwaysOnTop": true,
  "skipTaskbar": true,
  "resizable": false,
  "shadow": false,
  "width": 120, "height": 120
}
```
逻辑尺寸约 120×120，内核 10–14 px、光晕 40–80 px（产品文档 §4.2）。

---

## 2. 必须先弄清的平台权限风险

| 风险 | 说明 | 何时验证 |
|---|---|---|
| **TCC / 全盘访问权限** | 部分客户端的数据库位于受保护目录（`~/Library/Containers/…`）。读取这些路径可能触发系统授权弹窗或直接被拒。 | F2-06 实现扫描调度时，用真实客户端路径逐个试 |
| 拒绝授权后的降级 | 必须显示「未获得访问权限，无法校验」而不是静默无光点 | 与 T-01 同期 |
| 开机启动 | 沙盒与公证签名影响 | F2-12 |
| 公证签名 | 分发需要 Apple Developer（当前缺） | F6 |

**注意**：`~/.codex` 这类家目录下的路径不受 TCC 保护（F1 已实测可读）。
受保护的通常是 `~/Library/Containers/<bundle-id>/` 下的沙盒应用数据。
这个差异决定了首批适配器能覆盖哪些客户端。

---

## 3. 性能预算（SLO 来自产品文档 §14.3）

| 指标 | 目标 | 测量方式 |
|---|---|---|
| 空闲 CPU | 平均 < 1% | `Activity Monitor` / `ps -o %cpu` 连续采样 5 分钟 |
| 常驻内存 | 尽量 < 120 MB | `ps -o rss` |
| 帧率 | 呼吸动画 30 fps 足够；降动效时 0 fps | 手动观察 + 开发者工具 |
| 冷启动到光点可见 | < 2 s（建议值，未在文档中规定） | 打点 |

**渲染策略已定**：悬浮光点用 Canvas2D（只用径向渐变 + 加色混合），
**不引入 WebGL**。理由：长驻窗口的 GPU 上下文成本会破坏 CPU/内存 SLO。
WebGL 只留给「打开后才存在」的全球光场。

---

## 4. 可以直接复用的 F1 产物

| F1 产物 | F2 怎么用 |
|---|---|
| `light-core::compute_light_state` | 前端通过 Tauri command 拿 `LightState`，**不要在前端重算** |
| `light-core::palette` | 色阶 token 直接喂给 Canvas |
| `adapter-codex::scan` | 挂到扫描调度器上，产出 `ScanReport` |
| `adapter-sdk::GuardedDb` | 新适配器一律经它读取 |
| `light-store::LocalStore` | 本地库直接接 |
| `packages/protocol/schemas/*.json` | 前端类型可直接由 schema 生成 |

**禁止**：把业务规则写进 Tauri command 或 React 组件（违反 ADR-005）。
command 层的职责只有「搬运」，不做判断。

---

## 5. 适配器接入顺序建议

| 顺序 | 适配器 | 理由 |
|---|---|---|
| 1 | `codex`（已完成） | 唯一已验证 E3 通路 |
| 2 | `ollama` | 本地模型、无账号；需先在本机装一个模型实测（F0 T-06 未验证） |
| 3 | `custom-api`（OpenAI-compatible） | 只到 E1（配置存在），**不得**因为"填了 key"而点亮 |
| 4 | `claude` / `qwen-code` | 需先确认客户端是否有可用的产出物元数据；没有就停在 E0/E1 |

**每个新适配器必须**：写 `AdapterManifest`、更新 `only_three_tables_and_seven_columns_are_reachable`
这类数据面测试、并在 PR 里说明为什么需要每一列。

---

## 6. F2 开工前需要你确认的两件事

1. **T-01 的验收方式**：允许我做一个最小可运行窗口（不接真实数据，
   只显示一颗静态光点）来验证透明/置顶/穿透/非激活吗？这是唯一能在 F2 早期
   排除"形态不可行"风险的办法。
2. **本机是否允许创建一个测试用 Ollama 模型**（约 2–4 GB 磁盘）
   用于验证适配器 2？若不希望安装，该适配器顺延到 F2 后期，用合成数据先写测试。

---

## 7. 明确不在 F2 做的事

- 不做服务端（F3）
- 不做传光 UI 的完整闭环（F3）
- 不做全球光场（F4）
- 不做移动端（F5，BLOCKED）
- 不做品牌命名与视觉资产定稿（需你决策）
