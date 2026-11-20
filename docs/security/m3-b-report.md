# M3-B 交付报告：真实登录、生产隔离与页面接线

---

## 1. 安全审计（先做，不删数据）

| 项 | 结论 |
|---|---|
| 生产准入矩阵是否已同步 `PLUGIN_OBSERVED.activate=false` | **是**（`credential.mjs:33`） |
| 激活入口是否都用服务端最终政策 | **是**，唯一入口 `evaluateActivation` |
| 开关是否恢复旧行为 | **否**：`FL_ACTIVATION_MODE` 只提供测试夹具路径，且要求 `TEST_FIXTURE` 标记 |
| 历史数据中是否有错误政策下的激活 | **本仓库无生产库**；需在部署环境执行 §6 的审计 SQL，**本轮未删改任何数据** |
| `FL_TRUSTED_FIXTURE_ADAPTER` 是否可能进生产 | 会——`assertProductionIsolation()` 现在会在生产模式下直接**拒绝启动** |

## 2. 微信登录模块

```
POST /v1/auth/wechat-login   {code} → {token, expires_at_ms, anon_id}
POST /v1/auth/dev-login      {user} → 仅 FL_DEV_ADAPTER=on 且非生产
GET  /v1/auth/me             会话自检
POST /v1/auth/logout         立即撤销
```

- AppSecret 只在服务端；`code2Session` 的 `session_key` / `openid` **从不返回**。
- 登录 code 单次消费：独立 `wechat_code` 表，主键冲突**恰好**等于重放（已测）。
- 同一 `AppID+OpenID` 稳定映射同一匿名主体（已测）。
- 会话是随机 token，库中只存 SHA-256；撤销立即生效。
- 不依赖昵称/头像/手机号；OpenID 不出现在任何对外响应里。

## 3. 开发/生产物理隔离

`assertProductionIsolation()` 在 `FL_ENV=production` 时要求：

- 不得设置 `FL_DEV_ADAPTER=on`
- 不得设置 `FL_TRUSTED_FIXTURE_ADAPTER`
- 必须有 `FL_WECHAT_APP_ID` 与 `FL_WECHAT_APP_SECRET`

任一不满足 → **抛错拒绝启动**（L6 测试覆盖三种违规配置）。

## 4. 会话作用域

新增 `/v1/me/observations`、`/v1/me/paths`、`/v1/me/devices`——**全部只看会话身份，不接受任何 subject 参数**。
L2 测试显式传 `?subject_id=` 别人的 id，返回的仍是自己的数据。

## 5. 六页接线

| 页 | 数据来源 |
|---|---|
| 我的光 | `/v1/auth/me` + `/v1/me/devices` + `/v1/me/observations` |
| 连接插件 | `/v1/pairing/sessions` + `approve` + `state` |
| 传光 | `/v1/transmissions` 系列 |
| 光路 | `/v1/me/paths` |
| 回响 | `/v1/me/paths` + 前台 15s 轮询（`onHide` 清除） |
| 设置 | `/v1/me/devices` + 撤销 + 退出 |

可信等级仍分七级显示；`eligible.activate` 恒为 `false`，页面文案直说"未获独立核准"。

## 6. 部署环境需要执行的审计 SQL（未执行）

```sql
-- 是否存在通过旧政策（PLUGIN_OBSERVED 可激活）产生的激活
select e.transmission_id, e.activated_at_ms, c.source_class
  from light_edge e
  join credential c on c.transfer_id = e.transmission_id
 where e.state='ACTIVATED' and c.source_class = 'PLUGIN_OBSERVED';
```
本仓库没有生产数据库，**没有执行也没有删除任何记录**。

## 7. 微信开发者工具：尝试了，但**没有拿到运行证据**

- 工具已安装（`/Applications/wechatwebdevtools.app`）。
- CLI 需要写 `~/Library/Application Support/微信开发者工具/.../.cli`，被文件沙箱拒绝；
  按你的授权以更宽权限重试后，CLI 进程启动，但**窗口始终没有出现**（截屏确认前台仍是别的应用）。
- 因此：**没有截图、没有编译结果、没有真机数据。**
  这三项全部标 `PENDING`，我不拿"进程起来了"冒充"跑通了"。

## 8. 测试

| 套件 | 结果 |
|---|---|
| 登录/会话/隔离（m3b） | **7/7** |
| 服务端现代（凭证/激活/插件/配对/m3b） | **44/44** |
| 服务端旧验收 + FSM + 互操作 | 29/29 |
| 插件（代理 + 设备端到端） | 19/19 + 6/6 |
| 小程序逻辑 | 7/7 |
| Rust | 199 |
| 协议 | 12 |

## 9. 发布阻断项

| 项 | 状态 |
|---|---|
| 微信开发者工具截图/编译 | **PENDING** |
| 真机测试 | **PENDING** |
| REAL_WECHAT_AUTH | **PENDING**（无 AppID/AppSecret） |
| REAL_OLLAMA_COMPLETION | **BLOCKED** |
| 生产库审计 | 未执行（无生产库） |

## 10. M3-C 真机验收清单

1. 合法 AppID + AppSecret（服务端环境变量）
2. 已备案 HTTPS 域名，配置请求域名白名单
3. `FL_ENV=production`，确认启动时拒绝 DEV_ADAPTER
4. 微信开发者工具导入本工程，编译六页
5. 用真实 `wx.login` 走一次登录，确认 `/v1/auth/me` 返回
6. 真机扫码配对，核对双端指纹
7. 传光全流程，确认 `SOURCE_NOT_TRUSTED` 文案正确
