# fl-ollama-proxy · 本地 Ollama 观察代理

**它观察的是"一次 API 调用完成了"，不是"你真的用了 AI"。** 这两句话的差别就是本插件的全部诚实所在。

## 它做什么

- 在本机 `127.0.0.1` 上启动一个代理，透明转发 `POST /api/chat` 与 `POST /api/generate`。
- 只有当**上游返回了完整的完成标记**（非流式 `done:true`；流式收到 `done:true` 分块且连接正常结束）时，才记录一次观察。
- 把模型名映射到**注册表里的家族**（不靠字符串猜）。
- 用本机 Ed25519 设备私钥签一张最小凭证，交给服务端验签。

## 它不做什么

- 不读 Ollama 的数据库，不改任何 Ollama 配置。
- 不记录提示词、消息、响应正文、思考内容、API Key、原始报文。
- 不因为"检测到装了 Ollama"就产生任何记录。
- 不修改系统代理；退出后不残留服务。
- 不向局域网开放。

## 安装（无需构建）

要求 Node.js ≥ 20。

```bash
git clone <repo> first-light
cd first-light/plugins/ollama-proxy
node src/cli.mjs --dry-run
```

启动后终端会打印代理地址与一个一次性 token。把你的客户端指向该地址，并在请求头加入：

```
Authorization: Bearer <token>
```

## 卸载

删除目录即可；没有系统级安装、没有开机自启、没有后台服务。

## 支持范围

| 项 | 支持 |
|---|---|
| `/api/chat` | ✅ |
| `/api/generate` | ✅ |
| `stream: true` | ✅（按完成事件计一次） |
| `stream: false` | ✅ |
| `/api/tags`、加载/预热 | ❌ 不代理、不计数 |
| 其它路径 | ❌ 404 |

## 你能得到什么（三种状态分开显示，不合并）

```
✔ 已观察到 1 次 Ollama 调用（模型 llama3.1:8b → 家族 llama）
  · 真实调用：本地 API 完成（REAL_OLLAMA_CALL）
  · 本地观察：由本机代理记录（LOCAL_OBSERVATION）
  · 凭证已签名（SIGNATURE_VALID）
  · 尚未获得独立可信来源核准，暂不参与公共点亮
```

## 已知的伪造风险

- 你自己改代码，就能让代理记录任何东西。**服务器无法证明运行的是未修改的开源代码。**
- 因此服务端只把它当作 `PLUGIN_OBSERVED`，**不授予**公共光场计数，且默认不能激活传光
  （激活还需要服务端签发、绑定具体传光的挑战）。
- "代码可审计" ≠ "运行时已被证明未修改"。两者不要混为一谈。

## 测试

```bash
node --test test/*.test.mjs
```
13 项，用真实 Ollama 报文格式的模拟上游。
**REAL_OLLAMA_E2E 在本环境为 BLOCKED**：没有安装 ollama，也没有守护进程。
