# M2.2 + M3-A 交付报告

---

## 1. 最重要的一条：我把准入矩阵收紧了

M2.1 的时候 `PLUGIN_OBSERVED` 在政策上**允许激活**，只是缺少挑战绑定。
E2E-1 一跑就暴露：正确绑定 + 正确 challenge + 插件观察 → **真的激活了**。

按本轮要求，这不允许。现在：

```
PLUGIN_OBSERVED.activate = false
```

开放插件是"观察者"，不是"独立佐证者"。唯一的例外是运维显式配置的
`FL_TRUSTED_FIXTURE_ADAPTER`（默认不设），且响应里会带 `FIXTURE_AUTHORITY` 原因码。

**默认配置下，正式激活没有任何可达路径。**

## 2. CLI 设备身份（不再有占位）

```
fl-ollama-proxy init    生成设备密钥（~/.firstlight/device.json，权限 600，不上传）
fl-ollama-proxy pair    开配对会话，显示 8 位码 + 双端指纹，轮询等待手机批准
fl-ollama-proxy status  查询绑定状态
fl-ollama-proxy claim <transfer_id>   查看当前可用的挑战
fl-ollama-proxy serve   启动代理，观察完成事件 → 签名 → 提交
```

- 私钥只在本机；小程序永远拿不到。
- 配对确认时设备对 `session_id + code + device_id` 签名，服务端验签后才绑定。
- 撤销后设备立即失去 claim 与提交能力。

## 3. 挑战领取

`POST /v1/devices/:deviceId/challenge` 只接受：已 ACTIVE 且绑定主体 = **该 transfer 的接收方**。
别人的 transfer → `NOT_YOUR_TRANSFER`。关系未就绪 → `RELATION_NOT_READY`。

## 4. 联合闭环（E2E-1，真实 HTTP + PGlite）

```
CLI 配对 → 手机批准 → 设备签名确认 → ACTIVE
  → 发起传光 → 接受 → 双方确认
  → 手机签发 challenge → 设备领取（同一 id）
  → 代理观察到完成 → 设备签名凭证 → /v1/usage/attest-device → 201 / VALID / PLUGIN_OBSERVED
  → 手机调用 activate → 403 SOURCE_NOT_TRUSTED
```

**这条负向结果才是本轮的核心证据**：链路每一步都真实跑通，最后一步仍然被政策挡住。

## 5. 测试

| 套件 | 结果 |
|---|---|
| 插件端到端（device-e2e） | **6/6** |
| 插件代理 | **19/19** |
| 服务端现代（凭证/激活/插件/配对） | **38/38** |
| 服务端旧验收 + FSM + 互操作 | **29/29** |
| 小程序逻辑（等级/客户端/登录） | **7/7** |
| Rust | **199** |
| 协议一致性 | 12 |

## 6. 小程序（M3-A）

原生实现（不引入 Taro）——维护成本最低，六页：我的光 / 连接插件 / 传光 / 光路 / 回响 / 设置。
继承 B+：暖瓷白 `#F6F7F4`、石墨 `#202B30`、赤陶 `#8A4F41` 仅用于主操作。

**可信阶梯五级分开显示**：`UNPAIRED → PAIRED_IDLE → LOCAL_OBSERVED → SIGNATURE_VALID →
SOURCE_APPROVED → ACTIVATION_ELIGIBLE → ACTIVATED`，没有任何 `VERIFIED` 布尔值。

- 登录：`wx.login` → 后端换会话；AppSecret / session_key 不在客户端。
- 当前无真实 AppID 与 HTTPS 域名 → **DEV_ADAPTER**，标记 `REAL_WECHAT_AUTH_PENDING`。
- 回响用 HTTPS 轮询，不用 SSE（小程序不支持 EventSource）。

## 7. 未完成 / 阻断

| 项 | 状态 |
|---|---|
| 微信开发者工具截图 | **PENDING**（本机未安装 devtools，未伪造） |
| 真机测试 | **PENDING** |
| REAL_WECHAT_AUTH | **PENDING**（无 AppID/域名） |
| REAL_OLLAMA_COMPLETION | **BLOCKED** |
| 后端 /v1/auth/* 登录端点 | **未实现**（小程序端已按契约调用，服务端未建） |
| 小程序实际请求服务端 | 未联调（无 devtools） |

## 8. 这一轮真正的变化

不是又多了一套协议，而是**第一次有一条能跑起来的完整链路**：
设备配对、挑战领取、签名提交、服务端拒绝激活，全部真实执行。
同时把 M2.1 里那个"插件观察可以激活"的口子关掉了。
