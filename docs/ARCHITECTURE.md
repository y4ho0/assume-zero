# Architecture

[简体中文](zh-CN/ARCHITECTURE.md)

AssumeZero is a Rust library plus a thin CLI binary. The main modules are:

- `cli`: command selection, configuration precedence, diagnostics, and exit mapping;
- `config`: strict versioned TOML parsing and validation;
- `engine`: baseline gate, scenario orchestration, recovery experiments, budgets, and evidence levels;
- `workspace`: per-run copied workspaces, size limits, exclusions, Git-clean mode, and symlink policy;
- `runner`: direct process spawning, bounded stdout/stderr capture, timeouts, and best-effort interruption;
- `oracle`: deterministic run acceptance;
- `scenarios`: stable IDs and platform-aware environment/path plans;
- `minimize`: budget-aware `ddmin` with explicit 1-minimal verification;
- `redaction`: in-memory exact-value and pattern redaction plus path replacement;
- `report`: terminal, JSON, Markdown, and JUnit output;
- `fingerprint`: source-content and Git-status integrity evidence;
- `platform`: command lookup, path handling, essential environment, and platform facts.

## Execution flow

1. Parse and validate configuration, rejecting unknown fields.
2. Select the command, with tokens after `--` taking precedence.
3. Resolve the top-level executable before any `PATH` change.
4. Fingerprint source files and record Git status.
5. Run independent baseline copies.
6. Stop attribution unless all baseline runs satisfy the oracle.
7. Run each selected scenario in a fresh copy.
8. Repeat changed-condition failures.
9. For supported scenarios, restore and 1-minimize names/ordered entries.
10. Fingerprint and compare source state.
11. Redact in memory and write report schema v1.

The process runner receives an executable and argument vector; it does not concatenate a shell string. Explicit `--shell` converts the user script into platform shell arguments only after displaying the security warning.

## Cross-platform design

Platform-specific code is isolated behind compile-time branches. Windows preserves required process variables and recognizes `PATHEXT`, `.cmd`, and `.bat`; Unix checks executable bits. Path lists use the host separator and preserve order. Unsupported locale/timezone capabilities are skipped rather than treated as passing.
