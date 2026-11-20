# M3-C1 接续验收报告（解锁后）

**结论：仍未达成 WECHAT_DEVTOOLS_RUNTIME_PASS。阻断点已从"锁屏"推进到"IDE 不呈现窗口"。**

---

## 1. 本轮实际推进

| 步骤 | 结果 |
|---|---|
| 确认屏幕状态 | 已解锁（截图为 Chrome + DSH 界面） |
| 启动开发者工具 | `open -a` → **成功**，`launch.log` 记录 `fork nw process success: -> 4907 1.06.2412050` |
| 签名校验 | `[CheckSignature] success` |
| GPU | `settingJSON.General.enableGPU == true` |
| IDE 窗口 | **不可见/不可截取**（三张截屏均只有 Chrome / 微信 / VS Code） |
| CLI 导入项目 | `EPERM ... /.cli`（我的 shell 在沙箱内，无法写 App 目录；按指示不再扩大权限） |
| AppleScript 自动化 | **权限违例 -10004**（未授予辅助功能权限） |

## 2. 第一个可复现阻断点

> **`wechatdevtools` 进程存在，但在当前会话中不产生任何可截取的窗口。**

佐证：

- `pgrep` 有进程（PID 4907），启动参数完整；
- `launch.log` 显示 nw 进程 fork 成功、签名通过；
- 三张全屏截图（11:41、11:42）均无 IDE 窗口；
- `osascript` 查询窗口被系统拒绝（-10004），无法进一步判断窗口是否存在于其他 Space 或被隐藏；
- `stderr.log` 反复出现 `ssl_client_socket_impl: handshake failed, net_error -100`——IDE 的网络请求失败，可能是窗口停在无法渲染的状态。

**不是** CLI 参数问题，**不是** 工程问题，**不是** 锁屏问题。

## 3. 六页编译结果

**未编译。** 因此：

- 不能声称"无编译错误"；
- 不能声称"六页可运行"；
- 六页真实截图：**无**。

## 4. 静态校验（不是编译）

```
STATIC_VALIDATION_PASS (not a devtools compile)
  · pages: 6
  · no secrets in client code
```

仅证明文件齐备、JSON 可解析、WXML 事件绑定都有对应函数、无密钥赋值。

## 5. 需要人工执行（约 3 分钟）

1. 手动双击打开「微信开发者工具」，确认窗口出现（若同样不出现，说明是本机 IDE 安装/网络问题）。
2. 首次打开可能需要微信扫码登录——这是 IDE 自身的授权，我无法代做。
3. 导入项目：`~/Desktop/创造一切可能/first-light/apps/wechat-miniprogram`
4. AppID 选「测试号」，「不校验合法域名」仅在开发时勾选。
5. 服务端：
   ```bash
   cd ~/Desktop/创造一切可能/first-light/services/api
   FL_VERIFIER_V2=on FL_DEV_ADAPTER=on node src/serve.mjs
   ```
6. 编译后逐页检查，并把工具窗口截图发我。

## 6. 若要让我继续自动验收，需要其一

- 授予「辅助功能」权限（让 AppleScript 能读窗口），或
- 允许 CLI 写入 `~/Library/Application Support/微信开发者工具`（即上次那类一次性放宽），或
- 你手动完成导入后保持窗口可见，我直接截屏。

三者都不做的话，C1 只能停在 PENDING。

## 7. 安全门槛（未动）

`PLUGIN_OBSERVED.activate=false`、生产拒绝 DEV_ADAPTER 与夹具授权、旧接口不恢复自述激活、
公共光场关闭、真实 Ollama **BLOCKED**、生产库审计 **PENDING**。
DEV_ADAPTER 的任何成功都不会被记为 REAL_WECHAT_AUTH。

## 8. C2

真实 AppID/Secret、HTTPS 域名、真机：**全部缺失/未执行**，保持 PENDING。
