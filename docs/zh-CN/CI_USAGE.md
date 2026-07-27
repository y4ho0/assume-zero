# CI 使用

[English](../CI_USAGE.md) | [中文文档索引](README.md)

v0.1.0 使用普通 GitHub Actions 步骤，不是 Marketplace Action。

```yaml
name: AssumeZero

on:
  pull_request:
  workflow_dispatch:

permissions:
  contents: read

jobs:
  assumptions:
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
        with:
          persist-credentials: false

      - uses: dtolnay/rust-toolchain@4cda84d5c5c54efe2404f9d843567869ab1699d4 # stable snapshot 2026-07

      - name: Install AssumeZero
        run: |
          cargo install \
            --git https://github.com/y4ho0/assume-zero \
            --tag v0.1.0 \
            --locked

      - name: Check hidden assumptions
        run: assumezero check --config assumezero.toml
```

请选择有限命令，并根据仓库规模配置合理的总预算。退出码 1 表示发现 `CONFIRMED`/`PROVEN` 证据，退出码 2 表示基线无法支持归因。只有在检查脱敏输出后，才应把 `.assumezero/runs` 保存为 CI 产物。

虽然 AssumeZero 本身在检查期间不会主动访问网络，但准备命令和被测命令可能访问网络。
