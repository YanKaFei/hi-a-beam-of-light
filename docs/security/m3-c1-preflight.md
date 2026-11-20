# M3-C1 新版兼容性测试 · 只读准备

**未安装任何软件。** 本轮只列出事实、备份方案与恢复步骤，等你的安装授权。

---

## 1. 旧版事实

| 项 | 值 |
|---|---|
| 路径 | `/Applications/wechatwebdevtools.app` |
| 版本 | **1.06.2412050** |
| 构建 | **4240.111** |
| Bundle ID | `com.tencent.webplusdevtools` |
| 签名主体 | **Tencent Technology (Shanghai) Co., Ltd (FN2V63AD2J)** |
| 签名时间 | 2025-01-14 |

签名链完整（Tencent → Developer ID CA → Apple Root CA），**不是第三方改包**。

## 2. 配置目录

| 项 | 值 |
|---|---|
| 路径 | `~/Library/Application Support/微信开发者工具` |
| 大小 | **908 MB** |
| 内容 | 登录态、项目索引、插件、缓存 |

**它含登录凭据，属于敏感数据**：不进 Git、不进报告、不贴聊天记录。
`.gitignore` 已加入 `.backups/`。

## 3. 磁盘

- 根卷可用：**294 GiB**（5% 已用）
- 工作区同卷，备份空间充足。

## 4. 备份方案（**尚未执行**）

```
# 1) 旧 App（约 1 GB）
cp -R "/Applications/wechatwebdevtools.app" \
      "/Users/coffee/Desktop/创造一切可能/.backups/wechat-devtools/app-1.06.2412050.app"

# 2) 配置目录（908 MB，含登录态，保持私有权限）
cp -R "$HOME/Library/Application Support/微信开发者工具" \
      "/Users/coffee/Desktop/创造一切可能/.backups/wechat-devtools/profile-$(date +%Y%m%d)"
chmod -R go-rwx "/Users/coffee/Desktop/创造一切可能/.backups"
```

`.backups/` 已在 `.gitignore` 中，不会被提交。

## 5. 恢复原环境（三步）

```
# 1) 退出所有 IDE 进程
pkill -f wechatdevtools

# 2) 旧 App 放回原位（覆盖新版）
rm -rf "/Applications/wechatwebdevtools.app"
cp -R ".backups/wechat-devtools/app-1.06.2412050.app" "/Applications/wechatwebdevtools.app"

# 3) 配置目录按需回滚
rm -rf "$HOME/Library/Application Support/微信开发者工具"
cp -R ".backups/wechat-devtools/profile-<日期>" "$HOME/Library/Application Support/微信开发者工具"
```

**不删除旧版本、不删除登录状态**——备份先做，升级后原目录仍会被新版复用。

## 6. 新版获取（未执行）

- 唯一允许来源：`https://developers.weixin.qq.com/miniprogram/dev/devtools/download.html`
- 该页面是 JS 渲染的，我用静态请求只拿到 69 KB 壳，**没有解析到版本号**，
  因此不在本报告里写"最新是 X.Y.Z"——我没验证。
- 需要 macOS / Apple Silicon 版本；下载后**你会看到官方签名**，可自行核对。
- **不关闭 Gatekeeper、不扩大权限、不用第三方镜像。**

## 7. 2.x 的差异（重要）

新版 CLI 的启动参数与运行机制可能不同，**旧命令不能照搬**：

- 旧版：`cli open --project <dir>` + `~/Library/.../Default/.cli` 握手文件
- 新版：需要重新读它的 `cli --help`，不要假设路径与参数相同

因此即使安装成功，我也会**先读新 CLI 的帮助**再动手，而不是复用旧命令。

## 8. 停止条件（预先声明）

- 新版能出窗口 → 停止旧版排障，继续 M3-C1 页面与 API 验收。
- 新版仍不出窗口 → 记录实际版本与系统日志，**标记本机 `IDE_RUNTIME_BLOCKED`**，
  建议换一台受控 Mac 验收，不再试其他历史版本。

## 9. 需要你的一个决定

> **是否授权我执行 §4 的备份 + 从官方页面安装新版开发者工具？**

在你答复之前，我不复制、不下载、不安装。

## 10. 安全状态（未变）

`PLUGIN_OBSERVED.activate=false` · 生产拒绝 DEV_ADAPTER 与夹具授权 ·
旧接口不恢复自述激活 · 公共光场关闭 · `REAL_OLLAMA_COMPLETION` **BLOCKED** ·
`WECHAT_DEVTOOLS_RUNTIME_PENDING` · `REAL_WECHAT_AUTH_PENDING`
