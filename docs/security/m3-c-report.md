# M3-C 报告：开发者工具运行验收（C1）

---

## 1. 结论先说

**WECHAT_DEVTOOLS_RUNTIME_PASS 未达成。**
阻断点不是 CLI，也不是工程本身，而是：**这台机器的 macOS 图形会话处于锁屏状态**。

截图证据：登录窗口（用户 `coffee` + 密码输入框）。锁屏下任何应用窗口都不会渲染到可截取画面。

## 2. 逐层诊断（按你要求的 A–E 五项）

| 层 | 结果 | 证据 |
|---|---|---|
| A 主进程是否启动 | **是** | `pgrep` 显示 `wechatdevtools ... --user-data-dir=... --app-session-id=CUTSrvmDTz`（PID 1451） |
| B GUI 是否进入图形会话 | **部分** | `launchctl managername` = `Aqua`；但截屏为锁屏，**窗口不可见/不可截取** |
| C 项目是否导入 | **否** | CLI 的 `.cli` 握手文件已生成（端口 3799），但项目打开命令没有返回结果 |
| D 模拟器/编译器是否启动 | **否** | 无编译产物、无 devtools 日志 |
| E WXML/WXSS/JS 编译错误 | **未知** | 未进入编译阶段，因此不能声称"无错误" |

## 3. 已经确认的事实（不夸大）

- 用 `open -a` 启动会让主进程真正起来（之前 CLI 直接调用时根本没拉起 GUI）。
- `.cli` 文件由 App 自己创建，端口可读。
- 显示器休眠时截屏全黑；`caffeinate -u` 可唤醒，唤醒后看到的是**锁屏**。
- 解锁需要用户密码，我不尝试、也不绕过。

## 4. 替代验证：静态结构校验（明确不是编译）

新增 `scripts/validate.mjs`，检查：六页文件齐备、页面 JSON 可解析、WXML 中绑定的事件在页面里存在、
tabBar 指向的页面都在 `pages` 里、客户端代码里没有真正的密钥赋值（忽略注释）。

```
STATIC_VALIDATION_PASS (not a devtools compile)
  · pages: 6
  · no secrets in client code
```

**它不能替代开发者工具编译**，只能证明"人打开时不会立刻踩空"。

## 5. 需要你手动执行（约 3 分钟）

1. 解锁 Mac（这是唯一阻断点）。
2. 打开「微信开发者工具」→ 导入项目 → 目录选
   `~/Desktop/创造一切可能/first-light/apps/wechat-miniprogram`
3. AppID 选「测试号」；「不校验合法域名」仅限开发。
4. 启动服务端：
   ```bash
   cd ~/Desktop/创造一切可能/first-light/services/api
   FL_VERIFIER_V2=on FL_DEV_ADAPTER=on node src/serve.mjs
   ```
5. 编译后逐页检查：我的光 / 连接插件 / 传光 / 光路 / 回响 / 设置。
6. 把开发者工具窗口截屏给我，我据此修 UI 与联调问题。

## 6. C2（真实微信 + 真机）状态

| 项 | 状态 |
|---|---|
| 真实 AppID / AppSecret | **缺失** |
| HTTPS 域名与白名单 | **缺失** |
| 真机 | **未执行** |
| REAL_WECHAT_AUTH | **PENDING** |

## 7. 安全门槛（未放宽）

- `PLUGIN_OBSERVED.activate=false` 保持不变。
- 生产启动拒绝 DEV_ADAPTER 与夹具授权（`assertProductionIsolation`）。
- 旧接口不恢复自述激活。
- 公共光场仍不接受未获独立核准的记录。
- 真实 Ollama 推理：**BLOCKED**。
- 生产库历史审计：**PENDING**（本仓库无生产库）。

## 8. 本轮修改

- `apps/wechat-miniprogram/scripts/validate.mjs`（新增，静态校验）
- 小程序六页接线（上一轮已完成，本轮未改界面）

## 9. 判断

C1 的失败是**环境型**（锁屏），不是代码型。在解锁之前，任何"编译成功"的说法都不成立。
我停在这里，等解锁后拿到第一张真实工具截图再继续。
