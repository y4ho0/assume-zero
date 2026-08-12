# Security and privacy model

[简体中文](zh-CN/SECURITY_MODEL.md)

## Boundary

AssumeZero executes user-provided commands with the current user's privileges inside copied workspaces. The copies protect the source workspace from ordinary relative writes by the command. This is not a sandbox, container, virtual machine, privilege boundary, or safe way to execute untrusted code.

A tested or preparation command can intentionally access the network, home directory, credentials, services, devices, or arbitrary absolute paths that the current user can access. AssumeZero itself does not initiate network requests during checks and sends no telemetry.

## Source protection

- The tested command's working directory is always a fresh copy.
- Baseline, scenario, recovery, and minimization executions do not reuse modified copies.
- Ordinary files are byte-copied, never writable hard-linked.
- Repository roots and eligible sources are resolved before copying; ordinary sources reached through an intermediate symlink are refused.
- Workspace names are one path component, destinations remain under a fresh temporary root, and symlinks are created only after ordinary entries.
- Byte and entry budgets are checked during a read-only preflight before the destination project directory is created.
- `.git` and prior `.assumezero` evidence are excluded by default.
- Symlink targets are resolved for containment without reading target file contents; external targets are refused unless explicitly allowed.
- Source fingerprint and Git-status evidence are compared before report persistence.
- Git-status evidence is persisted as a SHA-256 digest rather than raw path output. Report run IDs are one normal path component; `.assumezero`, `runs`, run directories, and report files are checked to reject symlink escapes, and generated files are staged before publication. Every generated report artifact is capped at 64 MiB, and loaded JSON reports use the same read limit. Loaded reports are structurally revalidated before rendering; unsupported versions, unknown fields, and invalid constrained metadata are rejected.
- `.assumezero` is deliberate tool metadata and is excluded from source-content integrity claims.

Commands can still deliberately write outside their working directory. Do not test untrusted projects or commands.

## Process execution

Arguments are passed directly to the process API. `--shell` is explicit and warns that the system shell will parse the script. Each process has a per-command timeout and bounded captured output. On interruption or timeout, AssumeZero attempts to terminate the direct child and clean temporary directories. Complete descendant termination cannot be guaranteed across platforms.

## Environment values

Environment values are held only in process memory for execution, recovery, and exact-value redaction. Report structures contain names—not values. Sensitive names include token, secret, password, API/access/private-key, authentication, and credential patterns. Semantic long CLI options such as `--token`, `--password`, and `--api-key` are also recognized. Ambiguous short options are redacted only when named in `report.sensitive_options`, for example `["-p"]`; configured single-character short options recognize separate, equals-sign, and attached values.

Home-path redaction is mandatory in configuration v1: `report.redact_home = false` is rejected rather than weakening this privacy boundary.

Before any output summary is written, redaction replaces:

- exact values of sensitive-named inherited variables;
- exact values supplied to recognized or configured sensitive CLI options;
- Bearer tokens;
- GitHub token shapes;
- AWS access key IDs;
- JWT shapes;
- private-key headers;
- common database connection strings;
- home (`HOME` and `USERPROFILE` where present), project, and scenario-temporary paths.

Redaction has unavoidable false-positive and false-negative risk. A tested command that transforms or fragments a secret can evade exact matching. Use test credentials and inspect evidence before sharing it.

Verbose output is emitted only after bounded capture and redaction, rather than streamed before the redaction boundary. Raw CLI values still exist in process memory and the operating system's process argument view while the command runs. A single opaque `--shell` script is refused because its option semantics cannot be parsed reliably enough to redact CLI-only values from output evidence; use structured arguments or recognized environment variables instead. Because report v1 has no trusted Shell provenance, `explain` and regeneration conservatively refuse every saved `sh -c` or Windows `/D /S /C` wrapper, even when its command field was previously masked. Do not place real credentials directly in command arguments.

## Reporting a vulnerability

Follow [SECURITY.md](../SECURITY.md). Never include real credentials in an issue or report artifact.
