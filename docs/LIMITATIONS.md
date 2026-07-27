# Known limitations

[简体中文](zh-CN/LIMITATIONS.md)

Version 0.1.0 intentionally supports only finite, non-interactive commands.

- Workspace copies are isolation from direct relative source writes, not a security sandbox.
- Absolute-path and deliberate out-of-workspace writes cannot be prevented.
- Timeout/interruption termination targets the direct child; complete descendant-tree termination is best effort.
- Only Unicode environment variables available through Rust's portable string API participate in v0.1.0 minimization.
- `EMPTY_HOME` can confirm dependence on home-level state but does not trace the specific file.
- Cache redirection covers a conservative known-variable list and cannot prove a cache was actually read.
- `TZ=UTC` is a process-level best-effort setting, not an operating-system timezone change.
- `LOCALE_C` is skipped if the locale cannot be discovered reliably.
- Deep paths are bounded and do not probe beyond operating-system limits.
- A 1-minimal result is not a globally minimum or unique causal explanation.
- Minimization assumes sufficiently stable behavior and can stop with a `SUSPECTED` current-best result when budget is exhausted.
- Pairwise scenario reduction is documented for a future release and is not enabled in v0.1.0.
- Shell mode is explicitly trusted-input only.
- Log redaction can miss encoded, transformed, fragmented, or unfamiliar secrets.
- The tool does not trace arbitrary filesystem access, syscalls, network faults, databases, services, or containers.
- No crates.io package, telemetry, account system, cloud backend, or Marketplace Action is provided.

Platform capability is reported at runtime by `assumezero doctor` and in scenario skip statuses.
