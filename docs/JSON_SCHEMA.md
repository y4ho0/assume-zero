# JSON report schema

Every run writes `.assumezero/runs/<run-id>/report.json` conforming to [report-v1.schema.json](../schemas/report-v1.schema.json).

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

Environment-variable values are not fields anywhere in the schema. `restored_names` contains variable names or redacted normalized `PATH` entries. Captured output is a bounded, redacted summary and carries an `output_truncated` flag.

Consumers must:

- reject unsupported `schema_version` values;
- treat unknown enum values as a compatibility signal;
- distinguish scenario status from finding evidence level;
- avoid interpreting `SUSPECTED` as proven;
- avoid assuming a 1-minimal set is globally minimum.

