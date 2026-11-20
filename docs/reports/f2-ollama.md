# Ollama 适配器测试报告（F2 第四阶段）

状态：**IMPLEMENTED + TESTED（对本地模拟服务）｜真实模型运行 = BLOCKED**
代码：`crates/adapter-ollama`（13 项测试）

---

## 1. 结论先行

| 授权书要求（§4 验证内容） | 结果 | 说明 |
|---|---|---|
| 1. 能识别 Ollama 服务存在 | ✅ TESTED | `probe()` 调 `/api/version` |
| 2. 能识别已安装模型，但**不把安装视为使用** | ✅ TESTED | `/api/tags` 只产出 E0/E1；结构上无法变成证据 |
| 3. 可以执行一次本地真实推理 | ⚠️ **BLOCKED** | 本机未安装 Ollama，且模型仓库不可达 |
| 4. 能产生与模型家族对应的可验证使用记录 | ✅ TESTED（模拟服务） | 生成 E4 回执，含模型名/家族/回执 ID |
| 5. 调用发生在受控流程之外不得凭推测认定 | ✅ TESTED | 无任何"推测已使用"路径 |
| 6. 能同时识别 GPT 与 Qwen 两个家族 | ✅ TESTED | 见 §5，2 家族 → 生长 / 启迪者 |
| 7. 不重复计数 | ✅ TESTED | 存储层 `unique(family)` |
| 8. 服务离线/扫描失败界面不崩溃 | ✅ TESTED | `probe()` 永不返回错误，只返回 `reachable: false` |

---

## 2. 为什么第 3 项是 BLOCKED

授权书允许在满足条件时安装一个小模型（目标 ≤1.5GB）。实测网络：

| 端点 | 结果 |
|---|---|
| `https://ollama.com` | 200 |
| `https://ollama.com/download/Ollama-darwin.zip` | 307（可重定向） |
| `https://registry.ollama.ai/v2/` | **000（不可达）** |
| `https://registry.ollama.ai/v2/library/qwen2.5/manifests/0.5b` | **000（不可达）** |

模型分发仓库不可达 → **无法 pull 任何模型**（包括 `qwen2.5:0.5b`）。
即使安装 Ollama 二进制，也没有可调用的模型，因此第 3 项无法真机验证。

**没有绕过**：未使用代理、未下载第三方镜像模型、未伪造模型。

另外：本机未安装 Ollama（`command -v ollama` 无结果，无 `~/.ollama`）。
按"优先复用本机现有模型"的要求，这里也不存在可复用的模型，因此**连安装步骤都没有必要进行**。

---

## 3. 证据分级的落地方式（这一点必须说清）

Ollama 与 Codex 是**两类不同的证据源**：

| 信号 | 能证明什么 | 等级 | 能否点亮 |
|---|---|---|---|
| `/api/version` 有响应 | 服务已安装并运行 | E0 | ❌ |
| `/api/tags` 列出模型 | 模型可用 | E1 | ❌ |
| **我们执行了一次调用并拿到响应** | 真的发生了一次推理 | **E4** | ✅ |

**关键设计**：`CountableEvidence` 唯一的构造路径是
`receipt_to_evidence(&InferenceReceipt, ...)`，而 `InferenceReceipt` 只能由
`generate()` 返回。`ServiceInfo`（服务与模型清单）与证据类型之间**没有转换函数**。
测试 `installed_models_never_become_evidence` 断言了这一点。

这直接对应授权书第 5 条：调用发生在本产品控制的流程之外，不得凭推测认定已使用。
本适配器**根本不存在**"推测"的代码路径。

---

## 4. 一次调用如何变成 E4 回执

```rust
generate(base, model, prompt, registry)
  → 若 model 无法归入已知家族 → 立即拒绝（UnknownFamily），不发出请求
  → POST /api/generate { stream:false, num_predict:64 }
  → 解析 response / eval_count / created_at
  → InferenceReceipt { request_id, model, family, prompt_chars, response_chars,
                       eval_count, created_at_ms, response }
  → receipt_to_evidence(...) → CountableEvidence { level: E4,
                              proof_method: ClientReceiptApi }
```

- `request_id` 由 `SHA-256(model ‖ prompt ‖ created_at)` 前 8 字节派生：同一请求重放会得到同一 ID。
- `num_predict: 64`：验证性调用保持廉价，不长时间占用 CPU。
- **回答只回给用户本人，绝不上传**（F2 无任何网络上传路径）。
- 未知家族在**发请求之前**就拒绝：避免"调用了但无法归属"的灰色状态。

---

## 5. 两个家族共存（要求 6）

测试 `two_families_can_coexist`：GPT（来自 Codex，E3）+ Qwen（来自 Ollama，E4）
→ 已验证家族数 2 → 光阶 `Growing` → 知光称号 `启迪者`。

这与端到端测试 G（FAINT → GROWING）同一条规则路径，只是证据来源不同。

---

## 6. 传输层为什么不引入 HTTP 依赖

`src/http.rs` 是一个约 180 行的手写 HTTP/1.1 客户端，理由是：

1. 只连 loopback，不需要 TLS、连接池、重定向策略；
2. 引入完整 HTTP 栈会显著扩大依赖面与构建时间，对本地调用没有收益；
3. 明确拒绝一切非 loopback 主机与 `https://`（有测试 `refuses_non_loopback_and_https`）。

已覆盖 `Content-Length` 与 `chunked` 两种响应体（各有测试）。

---

## 7. 未验证 / 待办

| 项 | 状态 | 依赖 |
|---|---|---|
| 真实模型的一次推理与回执 | **BLOCKED** | 模型仓库可达（或本机已有模型） |
| 面板中的「试一次」入口 | 未接入 UI | 需先有一个可调用模型才有意义 |
| 真实模型的家族识别（`qwen2.5:0.5b` 等） | 逻辑已测（mock 返回同名） | 真机 |
| Ollama 服务版本差异 | 未测 | 真机 |

**给下一步的建议**：一旦模型仓库可用，用
```bash
cargo run -p adapter-ollama --example probe -- http://127.0.0.1:11434
```
（示例程序尚未写）跑一次，即可把第 3 项从 BLOCKED 变为 VERIFIED。
所需下载量：Ollama 二进制约 30–50MB，`qwen2.5:0.5b` 约 400MB，合计约 450MB，在授权额度内。

---

## 8. 资源占用与测试文件

| 项 | 值 |
|---|---|
| 测试运行时下载 | **0 字节**（全部使用本地模拟服务） |
| 留下的测试文件 | 无（模拟服务在内存中的随机端口，进程结束即消失） |
| 磁盘占用 | 0 |

因此"是否卸载测试模型"这一问题在本轮不产生任何对象，无需你决定。
