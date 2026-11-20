# M3 小程序契约（设计稿，未实现）

## 可信等级分层（绝不合并成一个 VERIFIED）

| 等级 | 文案 | 能否激活 |
|---|---|---|
| 未配对 | 「这台电脑还没有与你的光种关联」 | 否 |
| 已配对未观察 | 「已关联，尚未观察到模型使用」 | 否 |
| 本地观察 | 「本机观察到 1 次调用（本地记录）」 | 否 |
| 已签名 | 「记录已签名，服务器已接收」 | 否 |
| 来源政策通过 | 「该记录符合来源政策」 | 视绑定 |
| 独立佐证 | **当前不可能出现** | 是 |

## 六页与状态

| 页 | 主内容 | 空态 |
|---|---|---|
| 我的光 | 称号、可信家族数、设备列表 | 「先安装插件并配对」 |
| 配对 | 输入 8 位配对码 / 扫码 | 「在电脑上启动插件」 |
| 本地观察 | 按时间列出已签名的观察 | 「配对后，插件会在这里留下记录」 |
| 传光 | 生成二维码、接受他人 | 「服务未连接」 |
| 光路 | 已激活 / 待激活 / 我收到的 | 「还没有传过光」 |
| 回响 | 短句 + 历史 | 「还没有回响」 |

## API 契约（沿用现有，不新增）

```
POST /v1/devices/register                {device_id, pubkey_hex}
POST /v1/pairing/sessions                {device_id} → {session_id, code, fingerprint}
POST /v1/pairing/sessions/:id/approve    {code}          (身份鉴权)
POST /v1/pairing/sessions/:id/confirm    {device_id, code, signature_hex}
POST /v1/devices/:deviceId/revoke        (身份鉴权)

POST /v1/transmissions                   发起
POST /v1/transmissions/:id/accept|confirm
POST /v1/challenges                      {transfer_id} → {challenge_id, nonce, expires_at_ms}
POST /v1/usage/attest-v2                 {credential}
POST /v1/transmissions/:id/activate      {credential_id, challenge_id}
GET  /v1/light/paths | /v1/events/stream
```

## 安全说明

小程序**没有**与本机插件同等的密钥托管能力。配对中它是被认证的一方（已有身份 + 服务端会话），
不是密钥持有方；这一点必须写进产品文案，不能暗示两端安全等级相同。

## 未决

- 配对码在手机上如何输入（扫码 vs 手动）。
- 回响实时性：小程序不支持 SSE，需 WS 或轮询（M1 评估文档已给方案）。
- 未配对时的"我的光"是否显示称号（建议显示，但标注"仅本机"）。
