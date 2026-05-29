---
phase: quick-260528-w1p
plan: 01
subsystem: ci
tags: [ci, smoke-tests, vllm-cpu, inference, training, docs]
requires: []
provides:
  - "Always-on infer-smoke + train-smoke CI jobs on free public runners"
  - "vllm-cpu wheel install path (no CUDA wheel) for infer-smoke"
  - "pip + HuggingFace caching on both smoke jobs"
affects:
  - .github/workflows/ci.yml
  - scripts/infer-smoke.sh
  - scripts/train-smoke.sh
  - docs/book/src/inference/cpu-mode.md
tech-stack:
  added: ["vllm-cpu>=0.17 (CPU PyPI wheel)"]
  patterns: ["actions/cache@v4 for ~/.cache/huggingface", "setup-python cache: pip"]
key-files:
  created: []
  modified:
    - .github/workflows/ci.yml
    - scripts/infer-smoke.sh
    - scripts/train-smoke.sh
    - docs/book/src/inference/cpu-mode.md
decisions:
  - "infer-smoke installs vllm-cpu>=0.17 (101MB) rather than the ~10GB CUDA vllm wheel; CPU autodetect handled by engine.py torch.cuda.is_available() probe"
  - "No backend code change needed — engine.py already selects device=cpu"
requirements: [BACKEND-02, DOCS-02]
metrics:
  duration: "~3 min"
  completed: "2026-05-29"
  tasks: 3
  files: 4
---

# Phase quick-260528-w1p Plan 01: Always-on free CPU smoke jobs Summary

Made the `infer-smoke` and `train-smoke` GitHub Actions jobs always-on and free on standard 4-vCPU `ubuntu-latest` public runners by removing their `ROLLOUT_*_AVAILABLE` gates, swapping `infer-smoke` to the ~101MB `vllm-cpu` wheel, and adding pip + HuggingFace caching — with a matching CPU-mode doc refresh.

## What was done

- **Task 1 (fix):** Removed the `ROLLOUT_VLLM_AVAILABLE`/`ROLLOUT_TRANSFORMERS_AVAILABLE` self-skip early-exit blocks from `scripts/infer-smoke.sh` and `scripts/train-smoke.sh`; updated top comments to state unconditional CPU-mode operation. Build/run/assert bodies, `set -euo pipefail`, and the trap/mktemp scaffolding preserved verbatim. Commit `599fd7e`.
- **Task 2 (ci):** Ungated both jobs (`if:` removed), changed `infer-smoke` install from `vllm>=0.10,<0.22` to `vllm-cpu>=0.17` (step renamed "Install vllm-cpu (CPU wheel)"), added `cache: pip` to both `setup-python` steps, added `actions/cache@v4` for `~/.cache/huggingface` to both jobs, removed the now-dead `ROLLOUT_*_AVAILABLE` env overrides from the run steps, and set `infer-smoke` `timeout-minutes: 20`. Job header comments refreshed. Commit `14060e8`.
- **Task 3 (docs):** Rewrote the `## CI posture` section of `docs/book/src/inference/cpu-mode.md` to describe the always-on free CPU smoke posture (vllm-cpu wheel, HF caching, MockBackend proofs unchanged, local-dev install hint) and updated the failure-modes row to point the CPU/CI path at `vllm-cpu>=0.17`. Commit `99823d6`.

## Deviations from Plan

None - plan executed exactly as written.

## Verification

All local validations pass:

- `bash -n scripts/infer-smoke.sh` and `bash -n scripts/train-smoke.sh` — clean; no `ROLLOUT_*_AVAILABLE` references remain.
- `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"` — parses; neither smoke job has an `if:` gate; `infer-smoke` installs `vllm-cpu>=0.17` (not the CUDA wheel); both jobs carry `cache: pip` + a `~/.cache/huggingface` cache step; the run-step `ROLLOUT_*_AVAILABLE` env overrides are gone.
- `cargo build --workspace` — succeeds (no Rust changes; tree intact).
- `mdbook build docs/book` — succeeds.
- `shellcheck` / `actionlint` — not installed locally (skipped per plan; the YAML check above substitutes).

## Known Risk

`vllm-cpu`'s import surface is asserted (per the feasibility spike) to match `vllm` / `AsyncLLMEngine`, but this cannot be validated locally — no `vllm-cpu` install is available in the dev/CI-prep environment. **The first CI run on this branch is the real confirmation.** The `engine.py` try/except already tolerates the `>=0.22` top-level-alias drop, so a partial-import mismatch surfaces as a clean engine-init error rather than a silent failure.

## §9.2 doc+test policy

The code/CI changes (Tasks 1-2 touch only `scripts/` and `.github/`, not `crates/`/`python/`/`xtask/`, so §9.2 does not bind those commits). The accompanying doc refresh (Task 3, `docs/book/...`) keeps the documentation in sync with the CI behavior change, satisfying the spirit of the per-commit doc policy for this change set.

## Commits

- `599fd7e` fix(quick-260528-w1p-01): drop ROLLOUT_*_AVAILABLE self-skip from smoke scripts
- `14060e8` ci(quick-260528-w1p-01): ungate smoke jobs, swap to vllm-cpu, add pip+HF caching
- `99823d6` docs(quick-260528-w1p-01): refresh cpu-mode CI posture for always-on free smoke

## Self-Check: PASSED

All 4 modified/created files present; all 3 task commits found in git history.
