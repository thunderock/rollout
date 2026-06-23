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

# vLLM-CPU on public runners: run the V1 engine core in-process (the multiproc
# WorkerProc subprocess crashes on CI) and give the CPU backend a KV-cache budget.
# engine.py also sets enforce_eager on CPU to skip torch.compile/inductor.
export VLLM_ENABLE_V1_MULTIPROCESSING="${VLLM_ENABLE_V1_MULTIPROCESSING:-0}"
export VLLM_CPU_KVCACHE_SPACE="${VLLM_CPU_KVCACHE_SPACE:-4}"

OUT_DIR="data/completions/batch-tiny"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

echo "infer-smoke: building rollout-cli --features vllm..."
cargo build -p rollout-cli --features vllm

# 300s was too tight on a cold HF cache: model download + CPU vLLM init + 4
# prompts overran it (CI exit 124). The build above is already done, so this
# budget is download+inference only; 1200s fits well inside the 50m job cap.
echo "infer-smoke: running batch (timeout 1200 s)..."
timeout 1200 cargo run -p rollout-cli --features vllm -- \
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
