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
