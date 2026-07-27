# Contributing

[简体中文](CONTRIBUTING.zh-CN.md)

Thank you for improving AssumeZero.

## Before a change

Open an issue for substantial behavior or schema changes. Preserve stable scenario IDs, exit codes, privacy rules, and report compatibility. New claims need automated evidence.

## Development

Rust 1.80 or newer is required.

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
cargo build --locked --release
```

Tests must use obviously invalid fixture credentials and temporary fake homes/caches. Never read a developer's real home content. A scenario or recovery test must run the tested command in a copied workspace and verify source integrity.

Do not remove a useful failing cross-platform test to make CI green. Mark genuinely unsupported capabilities as skipped with an explanation.

## Pull requests

Describe the user-visible change, security/privacy effect, schema compatibility, and commands used to verify it. Keep commits focused and do not include `target`, `.assumezero`, `.env`, credentials, or local absolute paths.

Participation is governed by [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
