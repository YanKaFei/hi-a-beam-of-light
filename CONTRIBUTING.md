# Contributing

Thank you for wanting to pass the light on. This project is small and opinionated;
the fastest way in is to understand one rule before writing code.

## The one rule

**The evidence boundary is column-level.** A contribution that reads a column
outside an adapter's allowlist, or adds a path around `adapter-sdk`, will not be
merged — however useful it looks. Read `docs/adr/ADR-001` and `docs/adr/ADR-011`
first.

## Ways to help

- **New idea / new scene** — open an issue and describe the moment you wished
  had been lit for you. Concrete beats abstract.
- **New adapter** — start from `crates/adapter-codex`. Declare every column you
  read, add negative tests for the deny list, and record a real-data measurement.
- **Interface** — edit `design/healing/healing.css`, then
  `node render.mjs && python3 optimize.py`. Keep the four language layouts
  aligned; do not hand-edit the generated images.
- **Documentation** — corrections are welcome, especially where a claim lacks a
  reproducible command.

## Before you open a pull request

```bash
cargo test
cd packages/protocol && node --test test/conformance.test.mjs
cd services/api && node --test test/*.test.mjs
```

State what you ran and what it printed. A change that cannot be reproduced is
not yet a change.

## Language

Product copy and documentation exist in Chinese, English, French and Japanese.
Keep each file in one language; do not mix translations inside a file.

## Licence

By contributing you agree that your contribution is released under the MIT
licence of this repository.
