# JSON report schema

[简体中文](zh-CN/JSON_SCHEMA.md)

Every run writes `.assumezero/runs/<run-id>/report.json` conforming to [report-v1.schema.json](../schemas/report-v1.schema.json).

The path-normalized [demonstration report](demo/report-v1.example.json) is validated against that schema in CI. The same CI gate compiles both schemas and validates [examples/assumezero.toml](../examples/assumezero.toml) against the configuration schema after a lossless TOML-to-JSON conversion.

Stable top-level fields are:

```text
schema_version
tool_version
run_id
started_at
finished_at
platform
repository_fingerprint
configuration
command
baseline
baseline_status
scenarios
findings
budget
redaction_summary
workspace_integrity
```

`started_at` and `finished_at` are Unix epoch-second strings in schema v1. Durations are integer milliseconds.

`run_id` is a canonical 26-character ULID and a portable single path component. Repository and workspace fingerprints are opaque, version-dependent integrity evidence intended for before/after comparison within the same run; do not compare values across AssumeZero versions. `git_status_before` and `git_status_after` may contain legacy porcelain text from older v1 reports or a `sha256:` digest from hardened builds; new reports persist only the digest so repository path names do not re-enter evidence.

Environment-variable values are not fields anywhere in the schema. Recognized or configured sensitive CLI option values are replaced before the `command` field is constructed. `restored_names` contains variable names or redacted normalized `PATH` entries. Captured output and Oracle details are bounded where applicable and redacted before Report construction; captured output carries an `output_truncated` flag.

Consumers must:

- reject unsupported `schema_version` values;
- treat unknown enum values as a compatibility signal;
- distinguish scenario status from finding evidence level;
- avoid interpreting `SUSPECTED` as proven;
- avoid assuming a 1-minimal set is globally minimum.
