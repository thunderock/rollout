#!/usr/bin/env bash
# check-schema.sh — meta-validate the rollout JSON Schema using check-jsonschema.
# Used by `make validate-schema` and CI (plan 06).
set -euo pipefail

OUT="${TMPDIR:-/tmp}/rollout-schema-test.json"
cargo run --quiet -p rollout-cli -- schema --format json > "${OUT}"
check-jsonschema --check-metaschema "${OUT}"
echo "schema OK: ${OUT}"
