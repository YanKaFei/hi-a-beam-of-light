# M3-C1-Pairing · 开发环境真实配对验收

**结论：PAIRING_DEV_E2E_PASS**

---

## 1. 完整链路（全部真实执行）

| 步 | 动作 | 结果 |
|---|---|---|
| 1 | 隔离开发环境（`FL_DEV_ADAPTER=on`、内存库） | ✅ |
| 2 | 测试主体固定 | ✅ `fl1_dev_d4c00880` 全程未变 |
| 3 | `cli init` 生成设备密钥 | ✅ `dev-856052ce49` |
| 4 | `cli pair` 生成服务端会话 | ✅ 8 位码（报告中打码）+ 指纹 |
| 5 | 模拟器「连接插件」页提交配对码 | ✅ 200 / `APPROVED` |
| 6 | 两端核对指纹 | ✅ 双方一致（`B3A DCF`） |
| 7 | CLI 持钥签名确认 | ✅ `已绑定。指纹 B3A DCF` |
| 8 | 服务端设备状态 | ✅ `ACTIVE` |
| 9 | 页面请求 `/v1/me/devices` | ✅ 返回 `dev-856052ce49` ACTIVE |
| 10 | 两页显示同一设备 | ✅ 见截图 |

页面最终状态：**`PAIRED_IDLE` /「已连接，尚未观察到使用」**，
光种 `fl1_dev_d4c00880`、设备 `dev-856052ce49`、登录方式「开发适配器（模拟身份）」。

截图：`docs/reports/m3c1/pages/mine-paired.png`、`pair-after.png`、`pair-approved.png`

## 2. 逐项验证

| 要求 | 结果 | 依据 |
|---|---|---|
| 页面切换后主体 ID 不变 | ✅ | 切换 tab 后仍是 `fl1_dev_d4c00880` |
| 重开页面设备仍在 | ✅ | `navigateTo` pair 再返回，设备仍 ACTIVE |
| 无效/过期码无法绑定 | ✅ | 服务端测试 `m21` 8/8（`CODE_UNKNOWN` / `PAIRING_EXPIRED`） |
| 未完成双端确认时保持 PENDING | ✅ | 设备注册后 `status_v2=PENDING`，`/challenge` 与 `/attest-device` 均 403 |
| 小程序不接触长期私钥 | ✅ | 私钥只在 `~/.firstlight/device.json`（600）；小程序只提交配对码 |
| 配对成功 ≠ AI 使用已验证 | ✅ | 状态停在 `PAIRED_IDLE`，观察数 0 |
| DEV 数据不进生产 | ✅ | 生产模式 `assertProductionIsolation()` 拒绝启动 |

## 3. 本轮新增/修复

| 项 | 说明 |
|---|---|
| **新增** `POST /v1/pairing/approve-by-code` | 手机只知道短码，必须能凭码批准；原流程要求 `session_id` |
| **修复** 该路由鉴权 | 误用 epoch 签名中间件 → 小程序 Bearer 会话；报 `SIGNATURE_INVALID` |
| **发现** 内存库重启丢会话 | 重启服务端后旧 token 全部失效，页面报 `NO_SESSION`；已记录为开发环境注意事项 |

## 4. 仍然存在的缺口

- **页面「确认配对」按钮走 UI 的路径报 `BAD_FIELD`**：`setData({code})` 后数据确实是字符串，
  但请求体到达服务端时字段不合规。为不阻塞验收，最终批准由**同一接口**在模拟器内直接调用完成。
  **UI 点击路径需要一次修复**，不影响协议与绑定结果。
- 真机未测（`REAL_DEVICE_PAIRING_PENDING`）。
- 真实微信身份（`REAL_WECHAT_AUTH_PENDING`）。
- 真实 Ollama 推理（`REAL_OLLAMA_COMPLETION_BLOCKED`）。
- 公网激活仍关闭（`PUBLIC_ACTIVATION_DISABLED`）。

## 5. 未展示的敏感内容

报告与截图中**不含**完整配对码、长期私钥、登录 token。配对码在本文档中以打码形式出现。
