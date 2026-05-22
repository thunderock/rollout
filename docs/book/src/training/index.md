# Training

Phase 4 lands the first end-to-end training story: supervised fine-tuning + Bradley-Terry reward-model training + bit-identical-resume training-state snapshots + the Postgres `Storage` backend.

## What's here

- [SFT (Supervised Fine-Tuning)](./sft.md) — `rollout-algo-sft`; TRAIN-01.
- [RM (Reward Model)](./rm.md) — `rollout-algo-rm` with Bradley-Terry pairwise loss; TRAIN-02.
- [Snapshots](./snapshots.md) — `rollout-snapshots`, `SnapshotKind::TrainState`, tar + blake3 + restore; TRAIN-03.
- [Postgres backend](./postgres-backend.md) — `rollout-storage[postgres]`; testcontainers CI; TRAIN-04.
- [Determinism](./determinism.md) — `accelerate.save_state` + CUDA / CPU caveats.
- [CLI](./cli.md) — `rollout train sft|rm` and `rollout snapshot list|show|prune`.
- [CPU mode](./cpu-mode.md) — what to expect on macOS / Apple Silicon development boxes.

## Quickstart

```bash
# Dry-run validation (works without Python deps).
cargo run -p rollout-cli -- train sft --config examples/sft-tiny.toml --dry-run

# Live run (requires transformers + accelerate + torch; ~3-5 min CPU on M-series).
pip install 'transformers>=4.45,<5.0' 'accelerate>=1.0,<2.0' 'torch>=2.1,<3.0'
ROLLOUT_TRANSFORMERS_AVAILABLE=1 make train-smoke
```

## Phase 4 exit criteria

| Criterion | Where it's proven |
|-----------|-------------------|
| `rollout train sft --config examples/sft-tiny.toml` completes on a small model | `make train-smoke` (gated on `ROLLOUT_TRANSFORMERS_AVAILABLE=1`) |
| Snapshot + restart produces bit-identical weights for next K steps | `crates/rollout-algo-sft/tests/snapshot_resume.rs` (default-fire) + `crates/rollout-backend-vllm/tests/snapshot_resume_live.rs` (gated) |
| Postgres backend CI-tested via containerized integration test | `crates/rollout-storage/tests/postgres_integration.rs` via the `postgres-integration` CI job |

## What's NOT here (deferred)

- PPO / GRPO / DPO / IPO / KTO — Phases 9 / 10.
- Buffer / Process / EpisodicMemory snapshot kinds — Phases 9 / 11 / 8.
- Cloud object stores for snapshot blobs — Phase 5.
- HuggingFace datasets Hub integration — Phase 7.
- Multi-node distributed training — Phase 6.
