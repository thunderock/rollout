#!/usr/bin/env bash
# Phase-4 train smoke driver. Runs unconditionally in CPU mode on default
# public runners (CPU torch + transformers + accelerate).
# Exercises the full SFT path against Qwen/Qwen2.5-0.5B-Instruct on CPU.
# Expected wall-clock: ~3-5 minutes on M-series CPU; longer on x86 CPU runners.
# Mirrors scripts/infer-smoke.sh shape.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$REPO_ROOT"

# The transformers/vLLM backend embeds Python and does `import rollout`; the
# `python/rollout` package has no pyproject (not pip-installable), so put it on
# PYTHONPATH for the embedded interpreter.
export PYTHONPATH="$REPO_ROOT/python${PYTHONPATH:+:$PYTHONPATH}"

WORK_DIR="$(mktemp -d -t rollout-train-smoke-XXXXXX)"
trap 'rm -rf "$WORK_DIR"' EXIT
echo "train-smoke: work dir $WORK_DIR"

# 1. Dry-run validation first (cheap, no Python deps).
# Invokes `rollout train sft --config <toml> --dry-run` via `cargo run -p rollout-cli`.
echo "train-smoke: ==> Step 1: dry-run validation"
cargo run -p rollout-cli --features train --quiet -- \
  train sft \
  --config "$REPO_ROOT/examples/sft-tiny.toml" \
  --dry-run

# 2. Live SFT run. Override storage path + dataset path to land in WORK_DIR
#    so repeated runs don't collide on the repo-local ./data path.
echo "train-smoke: ==> Step 2: live SFT run against Qwen/Qwen2.5-0.5B-Instruct (CPU)"
cp "$REPO_ROOT/examples/sft-tiny.jsonl" "$WORK_DIR/"
sed \
  -e "s|./data/sft-tiny.db|$WORK_DIR/sft-tiny.db|" \
  -e "s|examples/sft-tiny.jsonl|$WORK_DIR/sft-tiny.jsonl|" \
  "$REPO_ROOT/examples/sft-tiny.toml" > "$WORK_DIR/sft-tiny.toml"

timeout 600 cargo run -p rollout-cli --features train --quiet -- \
  train sft \
  --config "$WORK_DIR/sft-tiny.toml"

# 3. Exercise the snapshot list command. Phase-4 SFT does not auto-snapshot on
#    completion from the CLI path (snapshot policy ride-along lands in Phase 9);
#    accept empty results here.
echo "train-smoke: ==> Step 3: list snapshots"
cargo run -p rollout-cli --features train --quiet -- \
  snapshot list \
  --storage-path "$WORK_DIR/sft-tiny.db" \
  --object-path "$WORK_DIR/object-store" \
  || echo "train-smoke: snapshot list returned non-zero (acceptable if no snapshots saved)"

echo "train-smoke: OK"
