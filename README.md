# AssumeZero

> Test what your project assumes about the machine it runs on.

AssumeZero treats the development environment as a finite test input. It runs a command that normally succeeds in fresh project copies, changes one supported condition at a time, repeats failures, and—where supported—restores a 1-minimal set of environment names or `PATH` entries.

```console
$ assumezero check -- cargo test
Final command: cargo test
Configuration source: built-in defaults

AssumeZero completed.

Baseline:
  STABLE — 2/2 accepted

Scenarios:
  7 passed
  0 failed
  0 skipped
  0 inconclusive/infrastructure

Secret values persisted: no environment values are report fields
Source workspace unchanged: yes
```

## What problem it addresses

A test, build, lint, package, or code-generation command can work on one machine because of state that the repository never declared: a user-level config, a populated cache, an environment variable, a globally installed child tool, a forgiving path, or a particular temporary directory. AssumeZero collects controlled counterfactual evidence about those conditions.

It is not a `.env` validator, dependency-version checker, development-container manager, static analyzer, secret scanner, syscall fuzzer, or reproducible-artifact comparator.

## Install

AssumeZero v0.1.0 requires Rust 1.80 or newer:

```bash
cargo install --git https://github.com/y4ho0/assume-zero --locked
```

No crates.io package is published for v0.1.0.

## 30-second start

Run a finite command that normally exits on its own:

```bash
cd your-project
assumezero doctor -- npm test
assumezero check -- npm test
```

Other examples:

```bash
assumezero check -- pytest
assumezero check -- mvn test
assumezero check -- cargo test
assumezero check --profile deep -- cargo test
```

AssumeZero always prints the final command it selected. Arguments after `--` override `[run].command` from the configuration file.

## Scenarios

| ID | Name | Quick | What changes |
|---|---|:---:|---|
| AZ-S001 | `EMPTY_HOME` | ✓ | Redirects applicable home/config/data variables to an empty directory |
| AZ-S002 | `EMPTY_CACHE` | ✓ | Redirects supported cache variables to empty directories |
| AZ-S003 | `CLEAN_ENV` | ✓ | Keeps platform essentials and the configured allowlist |
| AZ-S004 | `MINIMAL_PATH` | deep | Keeps the top-level command directory, system essentials, and explicit entries |
| AZ-S005 | `SPACE_WORKDIR` | ✓ | Uses a copied path containing multiple spaces |
| AZ-S006 | `UNICODE_WORKDIR` | ✓ | Uses a copied path containing Unicode |
| AZ-S007 | `DEEP_WORKDIR` | ✓ | Uses a safely bounded deep path |
| AZ-S008 | `REDIRECTED_TEMP` | ✓ | Redirects `TMP`, `TEMP`, and `TMPDIR` |
| AZ-S009 | `TIMEZONE_UTC` | deep | Sets process-level `TZ=UTC` on supported platforms; best effort |
| AZ-S010 | `LOCALE_C` | deep | Sets `LANG=C` and `LC_ALL=C` when the locale exists |

Statuses are `PASS`, `FAIL`, `SKIPPED_UNSUPPORTED`, `INCONCLUSIVE`, and `INFRASTRUCTURE_ERROR`. A skipped platform capability does not fail the whole check.

## Evidence levels

- `PROVEN`: stable baseline, repeated changed-condition failure, repeated recovery, completed 1-minimization, and no infrastructure error.
- `CONFIRMED`: stable baseline and repeated changed-condition failure, but no more specific recoverable variable/path was established.
- `SUSPECTED`: useful but incomplete or budget-limited evidence.
- `INCONCLUSIVE`: reliable attribution was not possible.
- `SKIPPED`: unsupported or explicitly excluded.

“1-minimal” means removing any one reported item breaks the observed recovery. It does not mean a mathematically unique or globally smallest explanation.

## Configuration

Create `assumezero.toml` without overwriting an existing file:

```bash
assumezero init
```

Use `assumezero init --force` only when replacement is intentional. A compact example:

```toml
version = 1

[run]
command = ["cargo", "test"]
prepare = []
timeout_seconds = 300
baseline_runs = 2
confirm_failures = 2

[workspace]
mode = "working-tree"
max_size_mib = 2048
exclude = [".git", ".assumezero"]
include_untracked = []

[oracle]
kind = "exit-code"
accepted_exit_codes = [0]

[scenarios]
profile = "quick"

[budget]
max_total_runs = 40
max_total_seconds = 1800

[report]
formats = ["terminal", "json", "markdown"]
```

Unknown fields fail validation. Output-text, regular-expression, required-file, and forbidden-file oracle conditions are documented in [PRODUCT_SPEC.md](docs/PRODUCT_SPEC.md). The machine-readable format is [config-v1.schema.json](schemas/config-v1.schema.json).

## Reports

Every check saves redacted JSON evidence:

```text
.assumezero/runs/<run-id>/report.json
```

Configured Markdown and JUnit XML reports are stored beside it. Regenerate formats with:

```bash
assumezero report <run-id> --format markdown
assumezero report <run-id> --format json
assumezero report <run-id> --format junit
assumezero explain <run-id>
```

Exit codes are stable: `0` no confirmed finding, `1` at least one `CONFIRMED`/`PROVEN` finding, `2` baseline failed/unstable, `3` command/configuration error, `4` internal error, and `5` user interruption. `SUSPECTED` does not cause code 1 unless `--suspected-is-failure` is used.

Verified fixture transcripts for hidden environment variables, hidden child tools, empty home state, paths containing spaces, and unstable baselines are in [docs/demo](docs/demo/README.md).

## Privacy

AssumeZero itself does not upload files, call external APIs, send telemetry, inspect the contents of the real home directory, or persist environment-variable values. Sensitive environment values are used only in memory to redact command output before it is written. Reports contain names, presence/classification metadata, and redacted output summaries.

User-provided preparation and tested commands can still access the network and other resources available to the current user. Redaction is defense in depth; pattern matching can have both false positives and false negatives.

## Security boundary

> AssumeZero executes user-provided commands with the current user's privileges inside copied workspaces. It is an isolation mechanism for protecting the source workspace, not a security sandbox for untrusted code.

The tested command never has the source project as its working directory, but it retains the current user's operating-system privileges. It can deliberately reach outside its copied workspace. Do not use AssumeZero to run untrusted code. Shell parsing is disabled unless `--shell` is explicitly selected, in which case a warning is shown.

External symlinks are refused by default without reading their targets. Process-tree termination on timeout or interruption is best effort and cannot be guaranteed on every platform.

## Platform support

The implementation and CI workflow target Windows, macOS, and Linux. Current verified support is recorded from actual CI runs in the repository's checks; see [LIMITATIONS.md](docs/LIMITATIONS.md) for capability-specific differences. A platform is not described as verified until its workflow job succeeds.

## Known limitations

v0.1.0 only supports finite, non-interactive commands. It does not provide OS virtualization, network or database fault injection, syscall/file-access tracing, automatic source fixes, cloud accounts, telemetry, package publication, or exhaustive scenario combinations. `EMPTY_HOME` confirms the condition but does not identify a specific home file. Timezone changes are process-level and best effort.

See [LIMITATIONS.md](docs/LIMITATIONS.md) for the complete list.

## Roadmap

The next candidates are evidence-preserving pairwise scenario reduction, stronger cross-platform descendant-process control, optional file-access evidence with an explicit privacy boundary, cached copy acceleration, and additional deterministic oracles. See [ROADMAP.md](docs/ROADMAP.md).

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md), the [security model](docs/SECURITY_MODEL.md), and the [Code of Conduct](CODE_OF_CONDUCT.md). Bug reports should include redacted evidence, platform details, and the exact AssumeZero version—never secrets.

## License

[MIT](LICENSE)
