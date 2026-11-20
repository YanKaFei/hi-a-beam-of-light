# 参与这个仓库

谢谢你愿意把光传下去。这个仓库很小，但有几条不肯让步的判断；
动手之前，先读一条规则就够。

## 唯一一条硬规则

**证据边界是列级的。** 任何读取适配器 allowlist 之外列、或绕过 `adapter-sdk`
另开读取路径的改动，都不会被合并——不管它看起来多有用。
先读 `docs/adr/ADR-001` 与 `docs/adr/ADR-011`。

## 能帮上忙的地方

- **新想法 / 新场景**：开一个 issue，写清楚「你在什么时刻希望被点亮」。
  具体的场景胜过抽象的功能。
- **新适配器**：从 `crates/adapter-codex` 开始。声明你读的每一列，
  为禁读清单补负向测试，并记录一次真实数据实测。
- **界面**：改 `design/healing/healing.css`，然后跑
  `node render.mjs && python3 optimize.py`。四种语言的版式要对齐；
  不要手改生成的图片。
- **文档**：尤其欢迎指出「哪里写了结论却没有可复现的命令」。

## 提 PR 之前

```bash
cargo test
cd packages/protocol && node --test test/conformance.test.mjs
cd services/api && node --test test/*.test.mjs
```

写清楚你跑了什么、输出是什么。不能复现的改动，还不算改动。

## 语言

产品文案与文档有中文、英语、法语、日语四种。一个文件只用一种语言，
不要把译文混在同一份文件里。

## 许可

提交即表示你的贡献按本仓库的 MIT 许可发布。
