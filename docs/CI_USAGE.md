# CI usage

Version 0.1.0 uses ordinary GitHub Actions steps; it is not a Marketplace Action.

```yaml
name: AssumeZero

on:
  pull_request:
  workflow_dispatch:

jobs:
  assumptions:
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v7

      - uses: dtolnay/rust-toolchain@stable

      - name: Install AssumeZero
        run: cargo install --git https://github.com/y4ho0/assume-zero --locked

      - name: Check hidden assumptions
        run: assumezero check --config assumezero.toml
```

Use a finite command and a total budget appropriate for the repository. An exit code of 1 means a confirmed/proven finding, while 2 means the baseline could not support attribution. Store `.assumezero/runs` as a CI artifact only after reviewing its redacted output.

Preparation and tested commands may use the network even though AssumeZero itself does not initiate network access during a check.
