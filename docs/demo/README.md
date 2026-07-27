# Verified fixture demonstrations

These excerpts were produced by the Rust helper fixture on 2026-07-27 on macOS/aarch64. Paths are normalized and fixture secret values are omitted. Every tested command ran in disposable copies.

## Hidden environment variable

```text
Baseline:
  STABLE — 2/2 accepted

AZ-F001
  Scenario: AZ-S003
  Evidence: PROVEN

  Restoring only ASSUMEZERO_DEMO_TOKEN made the command pass repeatedly.
  Values stayed in memory and were not persisted.
  The set is 1-minimal, not guaranteed globally minimum.

Source workspace unchanged: yes
Exit code: 1
```

Evidence chain:

```text
baseline accepted 2/2
→ CLEAN_ENV rejected 2/2
→ complete environment restoration accepted repeatedly
→ ddmin
→ ASSUMEZERO_DEMO_TOKEN accepted repeatedly
→ explicit 1-minimal verification
```

The obviously invalid fixture value was searched across `.assumezero` after the run and was absent.

## Hidden child tool on PATH

```text
Baseline:
  STABLE — 2/2 accepted

AZ-F001
  Scenario: AZ-S004
  Evidence: PROVEN

  Restoring only <DEMO_BIN> made the command pass repeatedly.
  The ordered entry set is 1-minimal, not guaranteed globally minimum.

Source workspace unchanged: yes
Exit code: 1
```

The top-level fixture executable resolved before `PATH` changed. It invoked `az-demo-child`, which existed only in `<DEMO_BIN>`.

## Empty home

```text
Baseline:
  STABLE — 2/2 accepted

AZ-F001
  Scenario: AZ-S001
  Evidence: CONFIRMED

  The scenario failure repeated, but v0.1.0 does not trace a more
  specific underlying file or condition for this scenario.

Source workspace unchanged: yes
Exit code: 1
```

The baseline used a temporary fake home containing a fixture config. `EMPTY_HOME` redirected applicable variables to a newly created empty directory. No real home content was listed, read, or copied.

## Workspace path containing spaces

```text
Baseline:
  STABLE — 2/2 accepted

AZ-F001
  Scenario: AZ-S005
  Evidence: CONFIRMED

  The command has a repeatable dependency exposed by SPACE_WORKDIR.

Source workspace unchanged: yes
Exit code: 1
```

The fixture deliberately rejected a current directory containing spaces. The failure repeated in independent copies.

## Unstable baseline

```text
Baseline:
  BASELINE_UNSTABLE — 1/2 accepted

Scenarios:
  0 executed

Source workspace unchanged: yes
Exit code: 2
```

The deterministic flaky fixture alternated exit codes `0` and `1` through state outside the source project. AssumeZero stopped before environment attribution.

## Machine-readable summary

See [evidence-summary.json](evidence-summary.json) for the compact evidence index and [report-v1.example.json](report-v1.example.json) for the structurally complete, path-normalized environment-variable report. CI validates the latter against `schemas/report-v1.schema.json` with a JSON Schema 2020-12 validator.
