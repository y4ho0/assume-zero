# CI usage

Version 0.1.0 uses ordinary GitHub Actions steps; it is not a Marketplace Action.

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

Use a finite command and a total budget appropriate for the repository. An exit code of 1 means a confirmed/proven finding, while 2 means the baseline could not support attribution. Store `.assumezero/runs` as a CI artifact only after reviewing its redacted output.

Preparation and tested commands may use the network even though AssumeZero itself does not initiate network access during a check.
