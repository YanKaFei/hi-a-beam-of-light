# F3.8-R2｜数据契约修复与真实界面验收

---

## 1. `undefined` 根因（有结构证据，不是猜测）

### 1.1 真实数据结构

Rust 侧 `scan.rs`：

```rust
pub struct FamilyView {
    pub family: String,        // 家族标识
    pub level: String,         // "E3" / "E1" / "E2"
    pub sessions: usize,
    pub raw_models: Vec<String>,
    pub countable: bool,       // 是否计入可信核验
}
```

`light_payload` 返回 `"families": Vec<FamilyView>`。

UI 读的是 **`f.name`** —— 这个字段**不存在**。运行中实测诊断输出：

```json
{ "type":"array", "length":1, "elementType":"object",
  "elementKeys":["countable","family","level","raw_models","sessions"],
  "rendererPath":"item.family -> vm.familyId -> .name" }
```

### 1.2 三个连带缺陷

1. **`families` 里混有不可计数条目**（E1/E2，`countable:false`）。原代码把它们与已核验的混在一起渲染。
2. **二进制里是旧 HTML**：只改 `ui/` 时 `tauri-build` 不重建，运行的应用一直在跑上一版。
   这就是"改了两次代码还是 undefined"的原因——第二次的修复根本没进二进制。
3. 修复后字段对上了，但 `undefined` 仍在 → 证明第 2 条是独立原因。

### 1.3 修复后实测

```
mine-1  初明者 1  invalid:false  rows:['gpt']
mine-2  启迪者 2  invalid:false  rows:['claude','gpt']
```

---

## 2. 正式数据契约

`apps/desktop/ui/families.js` —— Rust → 前端的唯一转换点，Rust 与 JS 共用一份定义：

| FamilyVM 字段 | 来源 | 说明 |
|---|---|---|
| `familyId` | `family` | 归一化标识 |
| `displayName` | `family` | 展示名（不读 `raw_models`，那是模型字符串） |
| `evidenceLevel` | `level` | 证据等级，独立维度 |
| `verificationStatus` | `countable === true` | `verified` / `discovered` |
| `countable` | `countable` | 严格 `=== true`，truthy 不算 |
| `sessions` | `sessions` | 仅 `Number.isFinite` 时保留 |

规则：
- `null` / `undefined` → 空列表，**不是错误**（还没扫描）
- 非数组、元素非对象、`family` 缺失/空白/含 `undefined` → **error 状态**，UI 显示「模型数据暂不可用」，**不伪造名字**
- 名称、证据等级、是否发现、是否可信是**四个独立维度**，不合并
- 称号仍由既有规则决定，E2 不升级可信称号

---

## 3. UI 资源构建依赖修复

**根因**：`build.rs` 只有 `tauri_build::build()`，没有声明对 `../ui` 的依赖，只改 HTML 不触发重编。

**修复**：

```rust
println!("cargo:rerun-if-changed=../ui");
```

**为什么不是字节比对**：Tauri 把嵌入资源压缩存储，`grep` 不到明文（实测 `ui-version` 在二进制里找不到，但 `families.js` 文件名能找到）。

**可核对的版本机制**：`panel.html` 带 `<meta name="ui-version" content="b8r2">`，
`__panelProbe` 返回 `uiVersion`，测试比对源码与运行值。实测运行中 `uiVersion: b8r2`。

**测试入口收敛**：删掉 `window.showView` 这个为测试加的全局别名，控制通道改为调用真实的
`show()`（本来就是全局函数）。生产页面不再多暴露一个入口。

---

## 4. 截图（全部来自真实 WebKit，`shot-mode` 调试激活模式）

| 文件 | 状态 | 数据来源 |
|---|---|---|
| `mine-0.png` | 我的光 · 0 个可信家族 | 真实（未扫描） |
| `mine-1.png` | 我的光 · 1 个家族 · 初明者 | **TEST 夹具**（gpt/codex） |
| `mine-2.png` | 我的光 · 2 个家族 · 启迪者 | **TEST 夹具**（gpt + claude） |
| `give-offline.png` | 传光 · 服务未连接 | 真实 |
| `give-waiting.png` | 传光 · 二维码有效 | 真实服务端 |
| `give-received.png` | 传光 · RECEIVED | 真实服务端 |
| `give-learning.png` | 传光 · LEARNING | 真实服务端 |
| `give-activated.png` | 传光 · ACTIVATED | 真实服务端 + **合成 E3 夹具** |
| `paths-empty.png` / `paths-data.png` | 光路 空 / 有记录 | 真实 |
| `echo-empty.png` / `echo-data.png` | 回响 空 / 有回响 | 真实 |
| `mine-0-640x480.png` | 最小窗口 | 真实 |

三态来自同一台机器两个实例 + 一个真实本地服务端：
`OFFERED → RECEIVED → LEARNING → ACTIVATED`，`complete` 返回 `kind=FIRST_LIGHT`。
**B 的 E3 证据来自夹具，不是真实模型调用。**

---

## 5. 无效文本自动化测试

`apps/desktop/tests/families_contract.test.mjs`（8 项）：
A 缺失不为错 · B 单家族 · C 双家族 · D 仅发现单独归类 · E 畸形输入报错 ·
F 归一化后不含无效文本 · G `countable` 必须严格 true · H **FamilyView 字段与 UI 读取路径一致**
（`scan.rs` 里没有 `name` 字段，这条测试直接挡住复发）。

`ui_resource.test.mjs`（2 项）：build.rs 依赖声明 · 运行时可核对的版本标记。

---

## 6. 全量回归

| 套件 | 结果 | 本轮 |
|---|---|---|
| Rust | 193 | **重跑全绿** |
| TypeScript 协议 | 12 | **重跑全绿** |
| 服务端 | 29 | **重跑全绿** |
| 扫描授权 | 5 | **重跑 5/5** |
| UI 契约 + 资源 + 选择器 | 19 | **重跑 19/19** |
| F2 桌面 E2E | 10 | **NOT_RUN** |
| F3.5 传光 E2E | 14 | **NOT_RUN**（三态已由本轮实拍覆盖，但正式套件未跑） |

---

## 7. CPU / RSS

两次测量，第一次是污染值——测量前我刚杀掉三个应用实例和一个服务端进程：

| | 第一次（污染） | **第二次（有效）** |
|---|---|---|
| 空闲 CPU | 0.318% | **0.011%** |
| 安静模式 CPU | 1.226% | **0.005%** |
| RSS | 116 → 126.7 MB | **113.2 MB，20s 增长 0.0 MB** |
| 残留进程 | 无 | 无 |

以第二次为准。第一次的高值与 10.6 MB 增长来自上一个实例的 WebKit 尚未回收，
不是本轮代码引入的回退——但这说明**测量前必须确保没有并发实例**，这一点我上一轮没做到。

---

## 8. 未完成

| 项 | 状态 |
|---|---|
| F2 / F3.5 正式 E2E | **NOT_RUN**（未跑，不沿用上轮 PASS） |
| 扫描权限"明确拒绝"截图 | 未拍 |
| 文本放大（系统字号） | 未做 |
| 新旧 UI 对比图 | 未生成 |
| 光点 | 未重写，未获视觉通过 |
| 非激活窗口 OS 首次点击 | **PENDING** |
| Keychain 成功路径 | **PENDING** |
| 真人可用性测试 | **USER_STUDY_PENDING** |

## 9. 修改文件

`apps/desktop/src-tauri/build.rs`（rerun-if-changed）
`apps/desktop/src-tauri/src/control.rs`（`showView` → `show`）
`apps/desktop/ui/panel.html`（契约归一化、诊断钩子、ui-version、去掉测试别名）
`apps/desktop/ui/families.js`（新增）
`apps/desktop/tests/families_contract.test.mjs`（新增）
`apps/desktop/tests/ui_resource.test.mjs`（新增）
