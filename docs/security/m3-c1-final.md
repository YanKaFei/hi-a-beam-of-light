# M3-C1 · 最终运行验收

**结论：真实开发者工具导入、编译、六页运行、页面内访问隔离后端——全部达成。**

---

## 1. 验收证据

| 项 | 结果 | 证据 |
|---|---|---|
| 导入 | ✅ | 标题栏 `wechat-miniprogram` |
| 编译 | ✅ | 构建 78，**0 错误 / 0 警告** |
| 六页运行 | ✅ | `pages/{mine,pair,transfer,paths,echo,settings}.png` |
| 页面内访问后端 | ✅ | 页面显示 `fl1_dev_d4c00880`，来自 `/v1/auth/me` |
| 业务状态展示 | ✅ | 未配对 → `UNPAIRED` / 「还没有连接电脑」/「在电脑上运行插件并配对」 |
| 空状态 | ✅ | 「还没有观察到模型使用。通过插件代理调用一次即可。」 |
| 开发身份标注 | ✅ | 页面明写「开发适配器（模拟身份）」 |
| 无虚假数据 | ✅ | 设备 0、观察 0、无伪造光场数字 |

整屏对照：`docs/reports/m3c1/pages/all-six.png`

## 2. 运行中发现并修复的 3 个真实缺陷

| # | 缺陷 | 影响 | 修复 |
|---|---|---|---|
| A | 小程序不支持 `.mjs` | 六页全部模块加载失败 | `utils/*.mjs` → `*.js` + CommonJS |
| B | 小程序没有 `fetch` | 登录直接抛错，页面显示「登录失败」 | 增加 `wx.request` 传输层，`fetch` 仅作回退 |
| C | 自动化脚本变量遮蔽 | 截图路径被页面路径覆盖 | 重命名并改用 `mp.screenshot` |

**A 和 B 只有真跑才会暴露**——静态检查、单元测试、服务端测试全部通过，页面依然白屏。

## 3. 与后端的真实往返

```
页面 onShow
  → getApp().ensureSession()
  → POST /v1/auth/dev-login  → 201 {token, anon_id, note:"SIMULATED IDENTITY"}
  → GET  /v1/auth/me         → {anon_id}
  → GET  /v1/me/devices      → {devices: []}
  → GET  /v1/me/observations → {observations: []}
```

页面上的 `fl1_dev_d4c00880` 就是服务端生成的匿名主体，**不是前端常量**。

## 4. 状态判定

| 标记 | 值 |
|---|---|
| `WECHAT_DEVTOOLS_RUNTIME_PASS` | **达成**（导入 / 编译 / 六页 / 页面内 API / 业务状态） |
| 配对全链路（CLI + 手机确认） | 未在本轮跑（页面提示需先 `fl-ollama-proxy init`） |
| 真机 | **PENDING** |
| `REAL_WECHAT_AUTH` | **PENDING**（当前是 DEV_ADAPTER，页面已如实标注） |
| `REAL_OLLAMA_COMPLETION` | **BLOCKED** |

## 5. 需要说明的边界

- 本轮跑的是**测试号 AppID**（`wxf694ceb007f49764`）+ **DEV_ADAPTER 模拟身份**。
  截图里的「开发适配器（模拟身份）」就是这层标注，**不能当作真实微信身份验收**。
- 配对链路只验证了空状态与错误提示，没有走完「CLI 发码 → 手机确认 → ACTIVE」。
- 真机未测。

## 6. 代码改动（仅小程序端）

`utils/{state,api,auth}.js`（CommonJS + wx.request）、六页 require 路径、`test/state.test.mjs`、
`scripts/validate.mjs`。**未改**：业务逻辑、证据规则、激活政策、后端、数据库。
