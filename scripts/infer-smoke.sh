#!/usr/bin/env bash
# Phase-3 end-to-end smoke. Runs `rollout infer batch` against
# examples/batch-tiny.toml. Always runs in CPU mode on default public runners
# via the `vllm-cpu` wheel (engine.py's torch.cuda.is_available() probe selects
# device="cpu"); see docs/book/src/inference/cpu-mode.md.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# The vLLM backend embeds Python and does `import rollout`; the `python/rollout`
# package has no pyproject (not pip-installable), so put it on PYTHONPATH for the
# embedded interpreter.
export PYTHONPATH="$REPO_ROOT/python${PYTHONPATH:+:$PYTHONPATH}"

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
