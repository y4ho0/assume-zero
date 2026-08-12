#!/usr/bin/env bash
set -euo pipefail

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
validation_root="$(mktemp -d)"
trap 'rm -rf "$validation_root"' EXIT

python3 - "$repository_root/examples/assumezero.toml" "$validation_root/assumezero.json" <<'PY'
import json
import pathlib
import sys
import tomllib

source = pathlib.Path(sys.argv[1])
destination = pathlib.Path(sys.argv[2])
with source.open("rb") as handle:
    parsed = tomllib.load(handle)
destination.write_text(json.dumps(parsed), encoding="utf-8")
PY

npx --yes ajv-cli@5.0.0 compile \
  --spec=draft2020 \
  -s "$repository_root/schemas/config-v1.schema.json"
npx --yes ajv-cli@5.0.0 compile \
  --spec=draft2020 \
  -s "$repository_root/schemas/report-v1.schema.json"
npx --yes ajv-cli@5.0.0 validate \
  --spec=draft2020 \
  -s "$repository_root/schemas/config-v1.schema.json" \
  -d "$validation_root/assumezero.json"
npx --yes ajv-cli@5.0.0 validate \
  --spec=draft2020 \
  -s "$repository_root/schemas/report-v1.schema.json" \
  -d "$repository_root/docs/demo/report-v1.example.json"

python3 - \
  "$repository_root/docs/demo/report-v1.example.json" \
  "$validation_root/invalid-report.json" \
  "$validation_root/invalid-nested-report.json" \
  "$validation_root/invalid-config.json" <<'PY'
import json
import pathlib
import sys

source = pathlib.Path(sys.argv[1])
report = json.loads(source.read_text(encoding="utf-8"))

overflow = dict(report)
overflow["run_id"] = "Z0000000000000000000000000"
pathlib.Path(sys.argv[2]).write_text(json.dumps(overflow), encoding="utf-8")

nested = json.loads(source.read_text(encoding="utf-8"))
nested["configuration"]["unknown_field"] = True
pathlib.Path(sys.argv[3]).write_text(json.dumps(nested), encoding="utf-8")

invalid_config = {"version": 1, "report": {"redact_home": False}}
pathlib.Path(sys.argv[4]).write_text(json.dumps(invalid_config), encoding="utf-8")
PY

if npx --yes ajv-cli@5.0.0 validate \
  --spec=draft2020 \
  -s "$repository_root/schemas/report-v1.schema.json" \
  -d "$validation_root/invalid-report.json" >/dev/null 2>&1; then
  echo "invalid overflow ULID unexpectedly passed report schema" >&2
  exit 1
fi

if npx --yes ajv-cli@5.0.0 validate \
  --spec=draft2020 \
  -s "$repository_root/schemas/report-v1.schema.json" \
  -d "$validation_root/invalid-nested-report.json" >/dev/null 2>&1; then
  echo "nested unknown report field unexpectedly passed report schema" >&2
  exit 1
fi

if npx --yes ajv-cli@5.0.0 validate \
  --spec=draft2020 \
  -s "$repository_root/schemas/config-v1.schema.json" \
  -d "$validation_root/invalid-config.json" >/dev/null 2>&1; then
  echo "redact_home=false unexpectedly passed configuration schema" >&2
  exit 1
fi
