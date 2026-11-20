# M3-C2 报告：真实微信与真机验收

**结论：C2A 配置与安全回归完成；C2B（真实微信 / 真机）因缺少凭据与设备，保持 PENDING。**

---

## 1. 先修的缺陷：配对码长度

| 项 | 修复前 | 修复后 |
|---|---|---|
| 服务端生成 | `ABCD-EFGH`（9 字符含连字符） | **`ABCDEFGH`（恰好 8 位）** |
| 输入页提示 | `ABCD-EFGH` | `ABCDEFGH` |
| 输入归一化 | 去非字母数字、保留前导零 | 不变（仍接受用户输入连字符） |
| 服务端校验 | `normalizeCode` 后比较哈希 | 不变 |

**根因**：`makeCode()` 用 `slice(0,4) + "-" + slice(4)` 拼了个分隔符，
而协议写的是 8 位。测得的"输入 9 字符"就是它。

修复位置：`services/api/src/pairing.mjs`。服务端现在只发 8 位；
连字符仅作为输入侧的便利，协议不依赖它。实测新码长度 **8**。

## 2. C2A 准备结果

| 项 | 状态 |
|---|---|
| AppID | `wxf694ceb007f49764`（导入时你选择；**是否为第一束光专属项目，需你确认**） |
| AppSecret | **未提供** → 服务端 `FL_WECHAT_APP_ID/SECRET` 未设置，`/v1/auth/wechat-login` 返回 503 |
| AppSecret 注入方式 | 仅环境变量，代码与仓库中**无密钥**（已核对，`auth.js` 里只有一句注释） |
| HTTPS 后端 | **无证书/域名** → PENDING |
| 合法域名与备案 | PENDING（需自有域名 + 备案 + 微信后台配置） |
| 持久化隔离库 | ✅ `FL_DB_DIR=.tmpdeps/dev-db`，重启不丢 |
| 生产准入政策 | ✅ 与 M1.1 相同，未放宽 |
| DEV_ADAPTER 隔离 | ✅ 生产模式启动即拒绝（实测 `production isolation failed: FL_DEV_ADAPTER must not be enabled`） |
| 日志脱敏 | token/code/session_key 不打印；本轮报告与截图不含完整配对码 |

## 3. 安全回归

| 套件 | 结果 |
|---|---|
| 配对 / 设备 | **8/8** |
| 凭证 + 激活 + 插件 | **30/30** |
| 登录 / 会话 / 隔离 | **7/7** |
| 生产隔离（DEV_ADAPTER 注入） | ✅ 拒绝启动 |
| 平台证据（`PLUGIN_OBSERVED.activate=false`） | ✅ 未改 |

覆盖的断言：跨主体隔离（`?subject_id` 被忽略）、同 openid 稳定映射、配对码过期/重放/并发、
设备撤销后失效、退出后 token 立即失效、私钥不出设备、配对 ≠ 可信使用。

## 4. C2B：真实微信与真机 —— PENDING

**阻断项（按依赖顺序）**

1. **第一束光专属 AppID**：当前 `wxf694ceb007f49764` 来源需你确认归属；不能确定就不能声称"真实账号"。
2. **AppSecret**：未提供。必须由你在服务端环境注入，不进仓库、不进报告。
3. **HTTPS 域名 + 备案**：需要真实域名与证书；本轮未购买（按指示不得擅自付款）。
4. **微信后台 request 合法域名配置**：依赖第 3 项。
5. **真机**：本轮无可用设备。
6. **真实 Ollama**：`REAL_OLLAMA_COMPLETION` 仍 BLOCKED。

**因此以下全部保持 PENDING**：真机登录、真机扫码配对、手机界面截图、真实 HTTPS 联通。

## 5. 你提供以下内容后，我可以立即完成 C2B

```bash
# 服务端（不要把密钥写进仓库）
export FL_ENV=production
export FL_WECHAT_APP_ID=wx...
export FL_WECHAT_APP_SECRET=...      # 只在部署环境
export FL_PUBLIC_BASE_URL=https://你的域名
export FL_DB_DIR=/var/lib/firstlight
node src/serve.mjs
```

再加：域名解析到该机、证书就位、微信后台把 `https://你的域名` 加入 request 合法域名、
小程序侧把 `app.js` 的 `apiBase` 指向该域名。

## 6. 状态

- `PAIRING_UI_E2E_PASS`（上一轮，仍有效）
- `REAL_WECHAT_AUTH_PENDING`
- `REAL_DEVICE_PAIRING_PENDING`
- `REAL_OLLAMA_COMPLETION_BLOCKED`
- `PUBLIC_ACTIVATION_DISABLED`
- `PLUGIN_OBSERVED.activate = false`（未改）
