# Known limitations

[简体中文](zh-CN/LIMITATIONS.md)

Version 0.1.0 intentionally supports only finite, non-interactive commands.

- Workspace copies are isolation from direct relative source writes, not a security sandbox.
- Canonicalization, preflight, and source rechecks narrow filesystem race windows but do not provide capability-relative, atomic protection against a repository being modified concurrently during copy.
- `workspace.max_entries` also bounds full-source fingerprint collection; only `.git` and `.assumezero` are excluded from that count, and stored fingerprint path names have a 64 MiB aggregate limit.
- Absolute-path and deliberate out-of-workspace writes cannot be prevented.
- Timeout/interruption termination targets the direct child; complete descendant-tree termination is best effort.
- Disposable workspace cleanup is best effort after execution; a surviving descendant process or operating-system file lock can leave a temporary copy behind, especially on Windows.
- Only Unicode environment variables available through Rust's portable string API participate in v0.1.0 minimization.
- Very short exact secret values can cause broad false-positive masking. When `-p` is configured as sensitive, every longer `-p...` token is treated as an attached value.
- Literal redaction is limited to 2,048 rules and 256 KiB of rule text; exceeding either limit fails closed by suppressing affected free-text evidence.
- `EMPTY_HOME` can confirm dependence on home-level state but does not trace the specific file.
- Cache redirection covers a conservative known-variable list and cannot prove a cache was actually read.
- `TZ=UTC` is a process-level best-effort setting, not an operating-system timezone change.
- `LOCALE_C` is skipped if the locale cannot be discovered reliably.
- The `DEEP_WORKDIR` path-length probe uses one bounded component (maximum 240 ASCII bytes); it does not measure arbitrary directory nesting or probe beyond common component limits.
- A 1-minimal result is not a globally minimum or unique causal explanation.
- Minimization assumes sufficiently stable behavior and can stop with a `SUSPECTED` current-best result when budget is exhausted.
- Pairwise scenario reduction is documented for a future release and is not enabled in v0.1.0.
- Shell mode is explicitly trusted-input only.
- Log redaction can miss encoded, transformed, fragmented, or unfamiliar secrets.
- Raw secrets passed as command arguments remain visible to the operating system while the process runs. Single-string opaque shell scripts are refused; transformed values and undeclared short-option meanings in structured commands can still evade exact redaction.
- Regenerating an older report reapplies current built-in long-option redaction, but historical custom/short-option semantics cannot be reconstructed. Inspect historical `.assumezero/runs` content before regenerating or sharing it.
- Report v1 has no trusted Shell provenance, so `explain` and regeneration reject every saved `sh -c` or Windows `/D /S /C` wrapper, including direct commands that were not created with `--shell`.
- Human-readable renderers expose C0/C1 control characters as visible code points, but Unicode bidirectional formatting characters are not normalized in v0.1.0.
- The tool does not trace arbitrary filesystem access, syscalls, network faults, databases, services, or containers.
- No crates.io package, telemetry, account system, cloud backend, or Marketplace Action is provided.

Platform capability is reported at runtime by `assumezero doctor` and in scenario skip statuses.
