# 预发布部署检查单（M3-C2A）

**本文只做检查与说明，不含任何真实密钥。** 需要你确认的只有"是否具备"，不是"把密钥给我"。

---

## 1. 需要你确认的五项（不需要提供明文）

| # | 需要确认 | 当前状态 |
|---|---|---|
| 1 | 是否拥有**第一束光专属**、可管理的正式小程序 AppID | ❓ 待确认（现有 `wxf694…` 来源未确认，我不推测归属） |
| 2 | AppSecret 是否**已安全配置在部署环境** | ❓ 待确认 |
| 3 | HTTPS 服务域名是否就绪（证书 + 解析） | ❌ 无 |
| 4 | 微信后台 request 合法域名是否已配置 | ❌ 无 |
| 5 | 是否具备真机测试条件 | ❌ 无 |

五项全部为"是"，才恢复 M3-C2B。

## 2. 最小环境变量清单

见仓库根 `.env.example`（无真实值）：

```
FL_ENV=production
FL_PORT=8787
FL_WECHAT_APP_ID=          # 正式 AppID
FL_WECHAT_APP_SECRET=      # 只在部署环境
FL_PUBLIC_BASE_URL=https://你的域名
FL_DB_DIR=/var/lib/firstlight
FL_VERIFIER_V2=on
# 生产环境严禁设置：FL_DEV_ADAPTER / FL_TRUSTED_FIXTURE_ADAPTER
```

**密钥注入方式**：只通过进程环境变量（systemd `EnvironmentFile`、容器 secret 或 shell export）。
不进源码、不进 `.env` 提交、不进日志、不进报告。

## 3. 密钥不泄漏的三道核对（本轮已执行）

| 检查 | 结果 |
|---|---|
| 工作区明文密钥赋值 | ✅ 未发现 |
| Git 历史是否含真实 secret | ✅ 仅含占位符（`export FL_WECHAT_APP_SECRET=...`），无真实值 |
| 客户端是否可能拿到 secret | ✅ 小程序代码无 AppSecret；`auth.js` 仅一句注释说明它不存在于设备 |
| 日志是否打印 token/code | ✅ 服务端不打印；报告与截图不含完整配对码 |

## 4. 数据库要求

| 项 | 要求 | 现状 |
|---|---|---|
| 持久化 | `FL_DB_DIR` 指向持久卷（PGlite 落盘目录） | ✅ 机制已支持，开发用 `.tmpdeps/dev-db` |
| 备份 | 每日快照该目录；恢复 = 覆盖目录后重启 | 未配置（部署时做） |
| 隔离 | 测试库与正式库不同目录 | ✅ |
| 迁移 | `001..005` 顺序执行，幂等 | ✅ 已在每次启动时执行 |

## 5. HTTPS 与访问控制

- 反向代理（nginx/Caddy）终止 TLS，转发到 `127.0.0.1:8787`。
- **只暴露 443**；`8787` 不对外开放。
- 证书可用 Let's Encrypt；到期自动续期。
- 建议加基础限流（登录与配对接口）。

## 6. 微信后台配置步骤（你操作）

1. 微信公众平台 → 开发 → 开发管理 → 开发设置
2. 记录 **AppID**；**重置/查看 AppSecret**（只显示一次，存进部署环境）
3. 「服务器域名」→ request 合法域名 → 添加 `https://你的域名`
4. 域名需 **ICP 备案**且为 HTTPS；配置后重启小程序项目生效
5. 小程序 `app.js` 的 `apiBase` 改为该域名

## 7. 部署

```bash
# 1) 准备目录与库
sudo mkdir -p /var/lib/firstlight && sudo chown $USER /var/lib/firstlight

# 2) 环境变量（示例，值由你填）
export FL_ENV=production FL_PORT=8787 \
       FL_WECHAT_APP_ID=... FL_WECHAT_APP_SECRET=... \
       FL_PUBLIC_BASE_URL=https://你的域名 \
       FL_DB_DIR=/var/lib/firstlight FL_VERIFIER_V2=on

# 3) 启动
cd services/api && node src/serve.mjs
```

启动时应看到：`firstlight api listening on http://127.0.0.1:8787`。
若出现 `production isolation failed`，说明误设了 DEV_ADAPTER 或缺少 AppID/AppSecret——**这是预期行为**。

## 8. 回滚

```bash
# 代码
git checkout <上一个已验收的 tag>
# 数据（如需回到某日快照）
systemctl stop firstlight
tar -xzf /backups/firstlight-YYYYMMDD.tar.gz -C /var/lib/firstlight
systemctl start firstlight
```

小程序侧回滚：开发者工具「版本管理」回退到上一个线上版本（若已发布）。

## 9. 发布阻断项（当前）

1. 正式 AppID 归属未确认
2. AppSecret 未配置
3. 无 HTTPS 域名与备案
4. 微信后台白名单未配置
5. 无真机

以上未满足前：`REAL_WECHAT_AUTH_PENDING`、`REAL_DEVICE_PAIRING_PENDING`，
且**不宣布 `REAL_DEVICE_PAIRING_PASS`**。
