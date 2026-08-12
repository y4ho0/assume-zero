# Changelog

[简体中文](CHANGELOG.zh-CN.md)

All notable changes are documented here. The format follows Keep a Changelog, and versions follow Semantic Versioning.

## [Unreleased]

### Added

- Complete Simplified Chinese documentation with bidirectional navigation and automated coverage/link checks.

### Security

- Reject workspace paths that escape through intermediate symlinks, unsafe workspace names, destination ancestors, or file-Oracle symlinks.
- Reject report run-ID traversal and report-root/output symlink escapes; stage report files before publication and apply the same 64 MiB limit to generated and loaded JSON reports.
- Preflight workspace byte and entry budgets before destination mutation, preserve Git path bytes on Unix, and stream source fingerprinting.
- Frame raw platform path bytes in source fingerprints and persist Git status as a SHA-256 digest; integrity hashes are version-specific rather than cross-version identifiers.
- Redact sensitive CLI option values consistently in command displays, verbose output, JSON, Markdown, Oracle details, and persisted evidence.
- Bound `deep_path_length` to 240 bytes so generated workspace names remain one portable component; configurations above that limit now fail validation.

## [0.1.0] - 2026-07-27

### Added

- Strict versioned TOML configuration and machine-readable schemas.
- Stable baseline gating with optional strict output.
- Seven quick scenarios and three deep scenarios with platform-aware skip behavior.
- Environment-variable and ordered `PATH` recovery using verified 1-minimal `ddmin`.
- Working-tree and Git-clean copied workspaces with size and symlink protections.
- Direct process execution, timeouts, interruption, bounded output, and deterministic oracles.
- In-memory secret/path redaction and JSON, Markdown, terminal, and JUnit reports.
- `init`, `doctor`, `check`, `list-scenarios`, `explain`, and `report` commands.
- Unit, integration, end-to-end, integrity, privacy, timeout, and fixture tests.

[Unreleased]: https://github.com/y4ho0/assume-zero/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/y4ho0/assume-zero/releases/tag/v0.1.0
