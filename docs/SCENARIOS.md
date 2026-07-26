# Scenarios

Every scenario has a stable ID, description, quick/deep profile membership, platform capability rule, and one of five statuses.

## Quick

- `AZ-S001 EMPTY_HOME`: redirects the applicable home, profile, config, and data variables to an empty directory. It never lists or copies the real home. v0.1.0 does not trace a specific missing file.
- `AZ-S002 EMPTY_CACHE`: redirects `XDG_CACHE_HOME`, `npm_config_cache`, `PIP_CACHE_DIR`, `UV_CACHE_DIR`, and `GRADLE_USER_HOME`. It never deletes real caches and does not guess Maven cache options.
- `AZ-S003 CLEAN_ENV`: preserves platform essentials plus `[environment].preserve`. Stable failure, full recovery, and a completed `ddmin` can produce a `PROVEN` variable-name set.
- `AZ-S005 SPACE_WORKDIR`: copies into `AssumeZero Test Workspace/project copy`.
- `AZ-S006 UNICODE_WORKDIR`: copies into `项目-测试-Δ`; inability to create or use the path is `SKIPPED_UNSUPPORTED`.
- `AZ-S007 DEEP_WORKDIR`: uses a configurable, safely bounded target length and does not deliberately exceed documented OS limits.
- `AZ-S008 REDIRECTED_TEMP`: redirects `TMP`, `TEMP`, and `TMPDIR` to a scenario-owned directory.

## Deep

- `AZ-S004 MINIMAL_PATH`: retains the resolved top-level program directory, essential system paths, and explicit preserved entries. Stable recovery supports ordered `PATH`-entry `ddmin`.
- `AZ-S009 TIMEZONE_UTC`: sets process-level `TZ=UTC` on supported Unix-like platforms. This is best effort and is not an OS-wide timezone change.
- `AZ-S010 LOCALE_C`: sets `LC_ALL=C` and `LANG=C` only when the C/POSIX locale is discoverable. It is skipped on unsupported platforms.

## Statuses

- `PASS`: the oracle remained accepted.
- `FAIL`: the changed condition repeatedly caused rejection.
- `SKIPPED_UNSUPPORTED`: the platform could not reliably enable the condition.
- `INCONCLUSIVE`: instability or budget limits prevented attribution.
- `INFRASTRUCTURE_ERROR`: setup/copy/execution infrastructure failed.

Pairwise scenario execution is not enabled in v0.1.0; see [LIMITATIONS.md](LIMITATIONS.md).

