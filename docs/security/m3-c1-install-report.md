# M3-C1 · 新版安装与运行结果

---

## 1. 备份（已完成并校验）

| 项 | 结果 |
|---|---|
| 位置 | `~/firstlight-backups/2026-11-03`（权限 700，仓库之外） |
| 旧 App | `app-1.06.2412050.app`，1,073,708 KB，**与源大小一致** |
| 配置目录 | `profile`，923,320 KB，**与源大小一致** |
| 备份签名 | `codesign -v` **通过** |

## 2. 新版

| 项 | 值 |
|---|---|
| 版本 | **2.02.2609240**（2026/10/26 稳定版） |
| 来源 | `devtools.wxqcloud.qq.com.cn`（官方文档页给出的稳定版链接） |
| 包大小 | 249 MB，HTTP 200 |
| SHA-256 | `46e429fe…9ae27d55` |
| 包签名 | **Developer ID Installer: Tencent Technology (Shanghai) Co., Ltd (FN2V63AD2J)**，**Apple 公证通过** |
| 安装后校验 | `codesign -v` 通过；`spctl` = **accepted / Notarized Developer ID** |

## 3. GUI：**成功显示**

关键结论：**旧版 1.06 的 NW.js 壳在 macOS 27 上确实不建窗，2.02 的 Electron 壳正常。**

截图：`docs/reports/m3c1/ide-window-visible.png`（整屏）与 `ide-window-crop.png`（窗口）。
窗口内容为项目列表页：小程序 / 小游戏 / 多端应用 / 代码片段 / 公众号网页，右上角「导入 / 管理」。

## 4. CLI 通路

- 新版 CLI 首次调用提示「服务端口已关闭」。
- 通过设置项 `enableServicePort` 开启（等同「设置 → 安全设置 → 服务端口」），**未修改 macOS 安全设置**。
- 开启后 CLI 与 IDE 正常通信。

## 5. 编译：**未达成**

CLI 导入报错：

```
Error: 不存在此 AppID 请检查后重新输入 (code 10)
  at exports.openProject ...
```

原因：`project.config.json` 里是 `touristappid`（测试号），**CLI 不接受它**。
`cli open --help` 显示 `--appid` 在提供 `--project` 时会被忽略，无法用它覆盖。

**唯一可行路径**：在 GUI 里点「导入」→ 选择目录 → AppID 选「测试号」。
这一步需要鼠标点击，我无法完成。

## 6. 结论

| 项 | 状态 |
|---|---|
| 备份完成 | ✅ |
| 新版安装与签名 | ✅ |
| GUI 可见 | ✅ **（这是本轮最大进展）** |
| 项目导入 | ❌ 需要 GUI 点击 |
| 编译 | ❌ |
| 六页运行 | ❌ |
| 真实截图（工具内） | 仅有窗口截图，无编译界面 |

`WECHAT_DEVTOOLS_RUNTIME_PASS` **未达成**，阻断点已从「不建窗」变为「需要一次鼠标点击导入」。

## 7. 回滚能力

```
pkill -f wechatwebdevtools
rm -rf /Applications/wechatwebdevtools.app
cp -R ~/firstlight-backups/2026-11-03/app-1.06.2412050.app /Applications/wechatwebdevtools.app
```
配置目录未删除，新版沿用了同一目录。

## 8. 需要你做的（约 1 分钟）

1. 开发者工具窗口现在应该是开着的（项目列表页）。
2. 点右上「导入」→ 目录选 `~/Desktop/创造一切可能/first-light/apps/wechat-miniprogram`
3. AppID 选「测试号」→ 导入
4. 它会自动编译，把模拟器界面截图给我。

服务端（我先启动好即可联调）：
```bash
cd ~/Desktop/创造一切可能/first-light/services/api
FL_VERIFIER_V2=on FL_DEV_ADAPTER=on node src/serve.mjs
```
