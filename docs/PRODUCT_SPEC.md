# Product specification

## Product

AssumeZero tests what a project assumes about the machine it runs on. Version 0.1.0 accepts finite commands that exit within a configured timeout: tests, builds, type checks, linters, packaging, code generation, migrations checks, and one-shot scripts.

The evidence workflow is:

```text
stable baseline
→ controlled environment change
→ repeated confirmation
→ counterfactual restoration
→ supported 1-minimization
→ redacted evidence report
```

The tool does not claim to discover every hidden dependency or a unique root cause.

## Commands

```text
assumezero init [--force] [--path <path>]
assumezero doctor [-- <command>]
assumezero check [--profile quick|deep] [--dry-run] [--strict-output] [--shell] -- <command>
assumezero list-scenarios
assumezero explain <run-id>
assumezero report <run-id> --format markdown|json|junit
```

Global options are `--verbose`, `--quiet`, `--no-color`, `--json`, and `--config <path>`.

## Baseline

The default is two runs, each in a new project copy with the same command, source state, configuration, and original inherited environment. Every run must satisfy the oracle before attribution starts. Mixed accepted/rejected runs produce `BASELINE_UNSTABLE`; consistently rejected runs produce `BASELINE_FAILED`. `--strict-output` additionally requires identical redacted stdout/stderr summaries and exit codes.

Preparation commands execute separately inside each copy. Their failure is infrastructure failure, not evidence against a scenario.

## Deterministic oracle

The v0.1.0 oracle supports:

- accepted exit codes;
- required stdout substrings;
- forbidden stderr substrings;
- a required stdout regular expression, validated before execution;
- required files relative to the copied workspace;
- forbidden files relative to the copied workspace.

Absolute and parent-traversing oracle file paths are rejected. Each run records the actual exit code, timeout/interruption state, truncation state, and each oracle check.

## Workspaces

`working-tree` copies the current tree except configured exclusions. `.git` and `.assumezero` are excluded by default. Ordinary files are copied, not write-linked. Each baseline/scenario/recovery run gets a new copy.

`git-clean` uses Git's tracked-file list plus relative paths explicitly named in `workspace.include_untracked`. It does not copy `.git`, ignored dependencies, or build output by default. It generally needs preparation commands.

The default maximum copied size is 2 GiB. External symlinks are refused unless the user explicitly accepts their risk.

## Minimization

When `CLEAN_ENV` fails repeatedly and complete restoration succeeds repeatedly, candidates are original nonessential environment-variable names. `ddmin` searches for a recovery set. Values remain in memory, output is redacted before persistence, and only names are reported.

When `MINIMAL_PATH` fails repeatedly and complete restoration succeeds repeatedly, candidates are normalized, deduplicated, ordered nonessential original `PATH` entries. The top-level command is resolved before changing `PATH`. Home paths are rendered as `<HOME>` and system paths can become `<SYSTEM>`.

Results are explicitly verified as 1-minimal when budget permits. Budget exhaustion returns the current best set as `SUSPECTED`, not `PROVEN`.

## Reports and exit codes

JSON report schema v1 is always persisted. Markdown and JUnit XML are optional configured formats. JUnit maps scenarios to test cases.

| Code | Meaning |
|---:|---|
| 0 | No confirmed/proven finding in executed scenarios |
| 1 | At least one confirmed/proven finding |
| 2 | Baseline failed or was unstable |
| 3 | Configuration, argument, or command error |
| 4 | Internal AssumeZero error |
| 5 | User interruption |

Unsupported individual scenarios do not fail the check.
