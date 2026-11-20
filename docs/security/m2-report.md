# M2 交付报告：Ollama 开源观察插件

**结论：插件完成并通过 13 项测试；真实 Ollama 端到端为 BLOCKED（环境无 ollama）。**

---

## 1. 状态区分（不合并成一个绿标）

一次成功观察会同时产生四个**独立**事实：

| 状态 | 含义 | 谁能判定 |
|---|---|---|
| `REAL_OLLAMA_CALL` | 上游真的返回了完整响应 | 代理观察 |
| `LOCAL_OBSERVATION` | 由本机代理记录 | 代理 |
| `SIGNATURE_VALID` | 凭证签名完整 | 服务端验签 |
| `SOURCE_POLICY_APPROVED` | 来源类别被政策允许 | 服务端政策 |
| `ACTIVATION_ELIGIBLE` | 满足全部绑定 | 服务端（需挑战） |

用户看到的是四行说明，不是"已验证"。

## 2. 插件架构

```
plugins/ollama-proxy/
  src/registry.mjs    模型名 → 注册家族（精确别名 → 最长前缀，绝不子串猜）
  src/observe.mjs     NDJSON 增量解析，只输出 {done, model, counters}
  src/credential.mjs  JCS 子集 + Ed25519 签名（与服务端字节一致，向量固定）
  src/proxy.mjs       回环代理 + token + Origin/Host 校验 + 单次判定
  src/cli.mjs         显式启动，退出即无残留
```

## 3. 测试（13/13）

真实 Ollama 报文格式的模拟上游覆盖：流式成功、非流式成功、流中断、上游报错、
`/api/tags` 不代理、token 缺失、Origin 伪造、Host 伪造（rebinding）、
未注册模型、日志无正文、分块边界、凭证 id 唯一、规范化向量一致。

**REAL_OLLAMA_E2E = BLOCKED**：本机未安装 ollama、无守护进程，未擅自下载模型。

## 4. 服务端集成（3/3）

- 插件形状的独立凭证 → `201`，`signature_status=VALID`，`source_class=PLUGIN_OBSERVED`，
  `transfer_id=null`。
- **同一张凭证无法激活任何传光**（`TRANSFER_MISMATCH`）——M1.1 的绑定在起作用。
- 插件自己的签名器产出的凭证被服务端接受（llama / qwen / deepseek）。

## 5. 隐私

代理转发时必然接触正文（已如实写进 PRIVACY.md）。
**日志、凭证、磁盘、上传均不含正文**，测试 13 断言了这一点。

## 6. 本轮修掉的缺陷

1. 路径含中文时 `new URL().pathname` 未解码 → 注册表读不到（插件根本无法启动）。
2. **上游断流/不可达时代理完全不记录判定** —— 调用凭空消失。现在每个请求保证恰好一条判定。
3. 未授权路径返回码暴露路径存在性 → 鉴权先于路由，并加测试。

## 7. 仍然不能证明的事

- **服务器无法证明运行的是未修改的开源代码**（"可审计" ≠ "未篡改"）。
- 代理看到的是"API 完成了"，不是"模型真的做了推理"。
- 因此正式环境仍不授予可信称号、不触发激活、不计入公共光场。

## 8. M3 之前仍需解决

1. 插件与身份的绑定（当前 CLI 的 `subject_identity_id` 还是占位）。
2. 传光场景下的挑战下发：谁在扫码后签发、如何送达插件。
3. 插件分发与完整性：签名发布包、版本兼容矩阵。
4. 小程序侧展示"已观察但未核准"的措辞与位置。
