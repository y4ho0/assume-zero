# Security and privacy model

[简体中文](zh-CN/SECURITY_MODEL.md)

## Boundary

AssumeZero executes user-provided commands with the current user's privileges inside copied workspaces. The copies protect the source workspace from ordinary relative writes by the command. This is not a sandbox, container, virtual machine, privilege boundary, or safe way to execute untrusted code.

A tested or preparation command can intentionally access the network, home directory, credentials, services, devices, or arbitrary absolute paths that the current user can access. AssumeZero itself does not initiate network requests during checks and sends no telemetry.

## Source protection

- The tested command's working directory is always a fresh copy.
- Baseline, scenario, recovery, and minimization executions do not reuse modified copies.
- Ordinary files are byte-copied, never writable hard-linked.
- `.git` and prior `.assumezero` evidence are excluded by default.
- External symlinks are refused without reading their targets unless explicitly allowed.
- Source fingerprint and Git-status evidence are compared before report persistence.
- `.assumezero` is deliberate tool metadata and is excluded from source-content integrity claims.

Commands can still deliberately write outside their working directory. Do not test untrusted projects or commands.

## Process execution

Arguments are passed directly to the process API. `--shell` is explicit and warns that the system shell will parse the script. Each process has a per-command timeout and bounded captured output. On interruption or timeout, AssumeZero attempts to terminate the direct child and clean temporary directories. Complete descendant termination cannot be guaranteed across platforms.

## Environment values

Environment values are held only in process memory for execution, recovery, and exact-value redaction. Report structures contain names—not values. Sensitive names include token, secret, password, API/access/private-key, authentication, and credential patterns.

Before any output summary is written, redaction replaces:

- exact values of sensitive-named inherited variables;
- Bearer tokens;
- GitHub token shapes;
- AWS access key IDs;
- JWT shapes;
- private-key headers;
- common database connection strings;
- home, project, and scenario-temporary paths.

Redaction has unavoidable false-positive and false-negative risk. A tested command that transforms or fragments a secret can evade exact matching. Use test credentials and inspect evidence before sharing it.

## Reporting a vulnerability

Follow [SECURITY.md](../SECURITY.md). Never include real credentials in an issue or report artifact.
