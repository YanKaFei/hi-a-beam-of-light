# ADR-005 · 移动端技术栈推迟到 F5

- **状态**：已采纳
- **决议来源**：D-06
- **论证背景**：[01-architecture.md](../01-architecture.md) §4 ADR-005

## 背景

F0 审计确认本机**没有 Xcode、没有 Android SDK/NDK**。
iOS Widget 与 NFC 读取本来就需要 Swift 原生代码；Android 悬浮窗需要原生权限流程。
且产品文档 §9 已承认 iOS 不支持任意第三方常驻悬浮。

## 决策

1. F0–F1 不对移动端技术栈做任何承诺。`apps/mobile/` 不创建可构建工程。
2. 但 **F1 必须完成平台无关的协议**：身份、点亮、传光、事件全部在
   `crates/light-core` + `packages/protocol` 中，与桌面端实现解耦。
3. 任何客户端只要说同一套协议即可接入；协议用共享夹具做跨实现验证。

## 反过来说，什么被禁止

- 禁止把业务规则写进桌面端 UI 或 Tauri command 层。
  规则必须在 `light-core`（纯函数）里，否则移动端要重写一遍逻辑 —— 这是
  「桌面实现绑死移动端」的具体形态，必须避免。
- 已用测试保证：`packages/protocol` 的 TypeScript 独立实现与 Rust 对同一批
  夹具给出相同答案（`test/conformance.test.mjs` + `tests/contract.rs`）。

## 重新考虑的条件

F5 启动前，需要真实设备、Xcode、Android SDK 与至少一次 NFC 真机实验。
在那之前任何技术栈承诺都无法验收。
