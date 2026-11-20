# 第一束光 · 微信小程序（MVP）

原生小程序，无构建步骤。**未在微信开发者工具中运行过**（本机未安装），
页面截图与真机验证均为 PENDING。

## 运行

1. 微信开发者工具 → 导入本目录
2. AppID 选「测试号」→ 会以 `touristappid` 启动，登录走 **DEV_ADAPTER**
3. 服务端：`cd ../../services/api && FL_VERIFIER_V2=on node src/serve.mjs`
4. 修改 `app.js` 里的 `apiBase`

## 真实登录

需要合法 AppID 与 HTTPS 域名。当前 `touristappid` 下**不会**调用 `wx.login`，
也不会声称拿到了真实微信身份（`REAL_WECHAT_AUTH_PENDING`）。

后端需实现：`POST /v1/auth/wechat-login {code}` → 会话；`POST /v1/auth/dev-login`（仅开发）。

## 页面

我的光 / 连接插件 / 传光 / 光路 / 回响 / 设置。

## 可信等级

`utils/state.mjs` 把状态分成七级，UI 逐级显示，**没有单一的 VERIFIED 标记**。
`node --test test/state.test.mjs` 可在无微信环境下验证这套逻辑。
