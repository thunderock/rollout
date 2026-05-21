#!/usr/bin/env bash
# Phase-3 end-to-end smoke. Runs `rollout infer batch` against
# examples/batch-tiny.toml. Gated on `ROLLOUT_VLLM_AVAILABLE=1`; skips with a
# clear message when unset so default public-runner CI stays green.
set -euo pipefail

if [[ "${ROLLOUT_VLLM_AVAILABLE:-0}" != "1" ]]; then
  echo "infer-smoke: skipped (ROLLOUT_VLLM_AVAILABLE != 1); see docs/book/src/inference/cpu-mode.md"
  exit 0
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

OUT_DIR="data/completions/batch-tiny"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

echo "infer-smoke: building rollout-cli --features vllm..."
cargo build -p rollout-cli --features vllm

echo "infer-smoke: running batch (timeout 300 s)..."
timeout 300 cargo run -p rollout-cli --features vllm -- \
  infer batch --config examples/batch-tiny.toml

OUT_FILE="$OUT_DIR/completions.jsonl"
if [[ ! -s "$OUT_FILE" ]]; then
  echo "infer-smoke: FAIL — output file $OUT_FILE missing or empty"
  exit 1
fi
N=$(wc -l < "$OUT_FILE")
if [[ "$N" -ne 4 ]]; then
  echo "infer-smoke: FAIL — expected 4 lines, got $N"
  exit 1
fi

python3 - <<PY
import json, sys
with open("$OUT_FILE") as f:
    for i, line in enumerate(f):
        row = json.loads(line)
        assert row.get("completion"), f"row {i} has empty completion"
print("infer-smoke: OK (4 completions)")
PY
