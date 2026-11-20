# M3-C1 · 真实运行验收报告

---

## 1. 已经真实达成的

| 项 | 证据 |
|---|---|
| 新版 IDE GUI | 2.02.2609240 Stable，窗口可见 |
| 项目导入 | 标题栏 `wechat-miniprogram` |
| **编译** | **构建 78，错误 0 / 警告 0** |
| **六页运行** | **六页全部成功渲染并截图**（`docs/reports/m3c1/pages/*.png`） |
| IDE 自动化 | `cli auto` 端口 9420，`miniprogram-automator` 连通 |
| 模拟器 → 后端 | **实测 201**：`wx.request` 直连 `127.0.0.1:8787/v1/auth/dev-login`，返回 token |

截图目录：mine / pair / transfer / paths / echo / settings 共 **6 张**。
「我的光」页已按 B+ 设计渲染：暖瓷白、石墨字、赤陶按钮、可信状态行。

## 2. 运行中抓到的两个真实缺陷（都已修）

### 缺陷 A：`.mjs` 不被支持

```
module 'utils/api.mjs.js' is not defined
```

微信小程序只认 `.js` + CommonJS。已把 `utils/{state,api,auth}.mjs` 改为 `.js` 并改 `module.exports`，
六页与 `app.js` 的 require 全部更新。

### 缺陷 B：小程序没有 `fetch`

页面调用 `fetch(...)` 直接抛错，`mine` 页显示「登录失败：」。
已改为：有 `wx.request` 时走 `wx.request`，否则回退 `fetch`。
模拟器内实测：`status 201`，返回 `DEV_ADAPTER` token 与 `SIMULATED IDENTITY` 标记。

**这两个缺陷只有真机/模拟器运行才会暴露，静态检查与单元测试都发现不了。**

## 3. 当前还差一步

页面跑的仍是**修复前的包**：

- `mine` 页数据仍是 `err: "登录失败："`
- 服务端日志 **0 条请求**
- 构建计数停在 78

原因：IDE 尚未把 `utils/api.js` 的新代码编译进运行包（`compileHotReLoad` 没有触发重建）。

**在开发者工具里再按一次 ⌘B**，之后我能立刻验证：
`dev-login → /v1/auth/me → /v1/me/devices → /v1/me/observations` 的真实往返，
以及配对与 `SOURCE_NOT_TRUSTED` 的界面文案。

## 4. 判定

| 项 | 状态 |
|---|---|
| 编译 | ✅ |
| 六页渲染 | ✅ |
| 六页截图 | ✅ |
| 访问后端（页面内） | ❌ 待最后一次编译 |
| 配对 / 来源不足文案 | ❌ 未验证 |
| 真机 | PENDING |
| `WECHAT_DEVTOOLS_RUNTIME_PASS` | **未达成**（差 API 往返这一步） |

## 5. 已修的代码缺陷清单

1. `utils/*.mjs` → `utils/*.js`（CommonJS），7 处 require 更新
2. `utils/api.js` 增加 `wx.request` 传输层
3. 单测 `test/state.test.mjs` 改为 CommonJS（7/7 通过）
4. `scripts/validate.mjs` 同步路径

未改：业务逻辑、证据规则、激活政策、后端。
