# M3-C1 阻断记录：WECHAT_DEVTOOLS_GUI_NOT_VISIBLE

状态：**只读诊断完成，等待人工接管。** 未再启动 IDE，未改权限，未动项目代码。

---

## 1. 只读核对结果

| 项 | 实测值 |
|---|---|
| 应用版本 | **1.06.2412050** |
| 构建号 | **4240.111** |
| 日志中的 `1.06.2412050` | **确实是应用版本**（与 Info.plist 一致，不是另一套构建号） |
| macOS | **27.0.1**（26A434） |
| 架构 | **arm64** |
| 当前进程 | 无（已按指示停止启动） |
| 窗口创建失败信息 | **日志中没有** |

## 2. 日志里没有的东西，比有的更重要

- `launch.log`：只有启动链路（daemon → 签名 → GPU 开启 → fork nw 进程 4907），
  **没有任何 `window` / `createWindow` / `fail` / `error` 记录**。
- `report.log`：同样没有窗口相关错误。
- `stderr.log`：有 `ssl_client_socket_impl ... handshake failed, net_error -100`（网络侧），
  以及我调用 AppleScript 时产生的 `OSACopyScriptingDefinition` 报错（与本问题无关）。

**结论：IDE 没记录"窗口创建失败"，所以不能断言它尝试过创建窗口。**

## 3. 一个可检验的假设（不是结论）

应用版本 **1.06.2412050**（2024 年 12 月的构建）运行在 **macOS 27.0.1** 上。
NW.js 壳在远超其发布年代的系统上无法创建窗口，是一个**合理但未经证实**的解释。

要证实或证伪，需要在同一台机器上换一个已知较新的 IDE 版本，或在一台较旧 macOS 上跑同一版本。
**本轮两件事都没做**（未升级、未换机）。

## 4. 官方更新路径（未执行）

- 下载页：`https://developers.weixin.qq.com/miniprogram/dev/devtools/download.html`
- 稳定版 macOS（Apple Silicon）安装包
- 覆盖安装即可，**项目列表与配置保存在** `~/Library/Application Support/微信开发者工具/`，
  不会被覆盖安装删除；项目本体在仓库里，与 IDE 无关。
- 若需回退：保留旧 `.app` 副本即可（当前未做备份，升级前建议先复制一份）。

**我没有升级，也没有重装**——按你的决定，等授权。

## 5. 仍需你在真实桌面完成（3 步）

1. Dock / Mission Control 里找一下是否已有开发者工具窗口（可能在其他 Space）。
2. 手动打开并导入：`~/Desktop/创造一切可能/first-light/apps/wechat-miniprogram`
3. 点编译，把窗口或报错截图发我。

如果**手动启动也不出窗口**，按你的规则归类为「本机开发工具环境故障」，
后续建议在另一台受控 Mac 或更新版本的官方工具上验证，我不再重复同一命令。

## 6. 状态

- `STATIC_VALIDATION_PASS`
- `WECHAT_DEVTOOLS_RUNTIME_PENDING`
- `REAL_WECHAT_AUTH_PENDING`
- `REAL_OLLAMA_COMPLETION_BLOCKED`

安全门槛全部不变：`PLUGIN_OBSERVED.activate=false`、生产拒绝 DEV_ADAPTER 与夹具授权、
旧接口不恢复自述激活、公共光场关闭。
