# Security policy

[简体中文](SECURITY.zh-CN.md)

## Supported version

The latest `0.1.x` source on the default branch receives security fixes during initial development.

## Report privately

Do not open a public issue containing exploit details, credentials, private paths, or unredacted AssumeZero artifacts. Use GitHub's private security-advisory reporting for `y4ho0/assume-zero` when available.

Include:

- AssumeZero version and commit;
- operating system and architecture;
- the smallest trusted fixture that reproduces the issue;
- whether source files changed;
- which report fields or output paths are affected;
- redacted reproduction steps.

Do not send real tokens or environment-variable values. Replace them with clearly invalid test values.

## Scope

Security issues include source-workspace writes caused by AssumeZero's copy mechanism, unsafe symlink traversal, persisted inherited environment values, command argument injection in non-shell mode, and misleading security-boundary behavior.

A trusted tested command deliberately reading or writing resources available to the current user is within the documented non-sandbox boundary, though documentation gaps are still welcome.
