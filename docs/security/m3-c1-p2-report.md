# M3-C1-P2 · UI 配对闭环修复与验收

**结论：PAIRING_UI_E2E_PASS**

---

## 1. BAD_FIELD 的根本原因

**一行代码：`call()` 构造参数时把 `body` 漏掉了。**

```js
// 修复前
const args = { base, path, method, headers, fetchImpl };
if (typeof wx !== "undefined" && wx.request) return viaWx(args);   // data: undefined
```

`viaWx` 取的是 `args.body`，但 `args` 里没有这个键 → `wx.request({ data: undefined })`
→ 服务端收到空对象 → 缺 `code` → `BAD_FIELD`。

对照实验验证了这个判断：

| 调用方式 | 结果 |
|---|---|
| 页面按钮（走 `call()`） | `BAD_FIELD` |
| 直接 `wx.request({ data: { code } })` | `200 APPROVED` |

**唯一差异就是请求体有没有带上。**

修复位置：`apps/wechat-miniprogram/utils/api.js`
```js
const args = { base, path, method, headers, fetchImpl, body };
```

服务端 Schema **未改**——它是对的，是客户端把 body 丢了。

## 2. 纯 UI 配对（真实输入 + 真实点击）

新会话、新设备、持久化开发库。全程用自动化模拟**真实 UI 输入与点击**，
没有任何一次直接调用 `approve-by-code`。

| 步 | 结果 |
|---|---|
| CLI `init` / `pair` | ✅ 设备 `dev-f47233e53a`，新码，指纹 `F7B 732` |
| 页面**输入框输入** | ✅ 收到 9 字符（含连字符） |
| 页面**点击「确认配对」** | ✅ 200 → `APPROVED` |
| 页面显示待确认 | ✅ 「已确认，等待电脑端完成最后一步（F7B 732）」 |
| 双端指纹核对 | ✅ 与 CLI 一致 `F7B 732` |
| CLI 签名确认 | ✅ 「已绑定。指纹 F7B 732」 |
| 设备进入 ACTIVE | ✅ 页面自动轮询到 `ACTIVE` |
| 「我的光」显示 | ✅ **PAIRED_IDLE** /「已连接，尚未观察到使用」 |

截图：`ui-1-typed.png`（输入后）、`ui-2-approved.png`（APPROVED）、`ui-3-mine-paired.png`（PAIRED_IDLE）

## 3. UI 行为测试

| 用例 | 结果 |
|---|---|
| 空输入 | ✅ 「请输入配对码」 |
| 非法格式 | ✅ 「配对码是 8 位字母或数字」 |
| 过期配对码 | ✅ 「配对码已过期，请在电脑上重新生成」 |
| 重复点击 | ✅ `pending` 时按钮禁用 + 函数首行直接返回 |
| 网络错误 | ✅ 显示「网络不可用」并可重试；`NO_SESSION` 自动重新登录后重试一次 |
| 未签名确认不显示 ACTIVE | ✅ 停在 `APPROVED`，轮询到设备 ACTIVE 才转 `ACTIVE` |
| 切页返回仍已连接 | ✅ 切到光路再回「我的光」，仍是 `PAIRED_IDLE` + 同一设备 |
| 主体 ID 不变 | ✅ 全程 `fl1_dev_d4c00880` |

配对码按字符串处理：`normalize()` 去除非字母数字但保留前导零，绝不转 number。

## 4. 开发环境持久化

`serve.mjs` 已支持 `FL_DB_DIR`，本轮直接用上（**没有引入新数据库架构**）：

```bash
FL_DB_DIR=.tmpdeps/dev-db FL_VERIFIER_V2=on FL_DEV_ADAPTER=on node src/serve.mjs
```

效果：

- 服务重启不再丢会话/设备（本轮实测重启后仍为 ACTIVE）
- 重置测试状态 = 删除该目录
- 测试数据在 `.tmpdeps/dev-db`，与任何生产数据无交集

会话丢失（内存库时期）的问题已由持久化解决；代码中仍保留 `401 → 重新登录一次 → 重试` 的兜底。

## 5. `/v1/me/devices` 真实响应

```json
{"status":200,"body":{"devices":[
  {"device_id":"dev-f47233e53a","status":"ACTIVE","paired":true,
   "created_at_ms":1793705140398,"revoked_at_ms":null}]}}
```

## 6. 回归

| 套件 | 结果 |
|---|---|
| 服务端配对（m21） | 8/8 |
| 服务端凭证/激活/插件 | 38/38 |
| 插件代理 + 设备端到端 | 19/19 + 6/6 |
| 小程序逻辑 | 7/7 |
| 静态校验 | PASS（非编译） |

## 7. 状态

- `PAIRING_UI_E2E_PASS` ✅
- `REAL_WECHAT_AUTH_PENDING`（仍是 DEV_ADAPTER，页面如实标注）
- `REAL_DEVICE_PAIRING_PENDING`（模拟器，非真机）
- `REAL_OLLAMA_COMPLETION_BLOCKED`
- `PUBLIC_ACTIVATION_DISABLED`

未展示：完整配对码、token、长期私钥。配对码在报告中一律打码。
