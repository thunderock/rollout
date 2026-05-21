# Phase 4: SFT + reward-model training + training-state snapshots — Discussion Log

> **Audit trail only.** Decisions are captured in `04-CONTEXT.md`.

**Date:** 2026-05-21
**Phase:** 04-train-sft-rm-snapshots
**Mode:** `/gsd:discuss-phase 4` (interactive; user invoked without `--auto`)
**Areas discussed:** Training execution path · Wave 0 trait surgery · Snapshot determinism · Postgres backend + dataset loader

---

## Area 1 — Training execution path (6 questions)

| Q | Selected (recommended) | Alternatives not chosen |
|---|---|---|
| Training trait shape | **Sibling `TrainableBackend: InferenceBackend` trait** | Extend InferenceBackend; new rollout-backend-hf-train crate |
| Where does training impl live | **`rollout-backend-vllm` with new `train` Cargo feature** (in-crate; reuses Phase-3 PyO3 thread) | New crate `rollout-backend-hf-train`; combine into one `rollout-backend-hf` |
| Underlying Python stack | **HF transformers + accelerate + FSDP (DDP fallback)** | Pure PyTorch DDP; HF trl Trainer |
| AlgoDependencies slot | **Single `Arc<dyn TrainableBackend>`** (trait hierarchy covers both inference + train) | Two slots; downcast-at-runtime |
| MockBackend TrainableBackend impl | **Yes — extend MockBackend so SFT/RM tests run on every CI build without HF transformers** | Inference-only mock + #[ignore] gate; separate MockTrainableBackend |
| Backend selection | **Cargo-feature gates (`--features vllm,train` / `--features test-mock-backend`)** | Runtime TOML `[backend] kind`; per-algorithm backend block |

## Area 2 — Wave 0 trait surgery (3 questions)

| Q | Selected (recommended) | Alternatives not chosen |
|---|---|---|
| Wave 0 scope | **Full surgery: PolicyAlgorithm + Snapshotter + TrainableBackend + ~15 supporting types** | Minimal (defer Snapshotter); add RunConfig variant discriminator |
| Algorithm crate layout | **Separate `rollout-algo-sft` + `rollout-algo-rm` crates** | Single `rollout-algos` with features; single `rollout-train` host crate |
| Snapshots crate | **Standalone `rollout-snapshots`; ships TrainState kind only in Phase 4** | Module inside rollout-storage; module inside rollout-runtime-batch |

## Area 3 — Snapshot determinism (3 questions)

| Q | Selected (recommended) | Alternatives not chosen |
|---|---|---|
| Determinism stack | **accelerate.save_state/load_state + torch.use_deterministic_algorithms(True) + fixed seeds across libs** | Hand-roll each piece; trust HF Trainer checkpoint format |
| Snapshot blob layout | **One tar per snapshot, one ContentId** (loses per-component dedup; simpler restore) | Multi-blob with SnapshotPart roles; safetensors + JSON sidecar |
| Resume test design | **MockBackend-driven; byte-equal weights at step N+K vs non-interrupted run (runs every CI build)** | Real HF transformers + Qwen2.5-0.5B-Instruct (slow CI); both (belt-and-suspenders) |
| Snapshot policy defaults | **on_completion + every 500 steps + on_preemption (SIGTERM); keep_last=3** | on_completion only; every 100 steps; user-config-only |
| Algorithm internal state | **Single `serde_json::Value` blob + algorithm_id tag** | Typed associated type; mixed (framework owns step/RNG, algo owns extras as Value) |

## Area 4 — Postgres + dataset loader (5 questions)

| Q | Selected (recommended) | Alternatives not chosen |
|---|---|---|
| Postgres client | **sqlx 0.8** (compile-time-checked SQL; PgListener; sqlx::migrate macro) | tokio-postgres + deadpool; diesel + diesel-async |
| Migrations | **sqlx migrate; .sql files under database/migrations/** | Hand-rolled migration runner; refinery / dbmate |
| watch() + CI | **LISTEN/NOTIFY via PgListener; testcontainers Postgres 16 in DEFAULT CI** | Triggers + polling; opt-in like infer-smoke |
| Dataset loader | **JSONL-only in Phase 4; HF datasets Hub deferred to Phase 7 (HARNESS-*)** | JSONL + HF datasets; JSONL + Parquet |
| Test model | **Qwen/Qwen2.5-0.5B-Instruct** (same as Phase 3; Apache-2.0; CPU-runnable) | Qwen/Qwen2.5-1.5B-Instruct; TinyLlama/TinyLlama-1.1B-Chat-v1.0 |

---

## Notes for User Review

- **In-crate `train` feature on rollout-backend-vllm** is a big crate. With both features ON, it pulls vllm + transformers + accelerate + torch (huge dep tree). The Cargo-feature gating keeps non-Phase-4 builds small. If this gets unwieldy in Phase 9, split into rollout-backend-vllm-infer + rollout-backend-vllm-train then.
- **MockBackend extension to TrainableBackend** is non-trivial — fake optimizer SGD against ndarray::Array1<f32> weights. Worth it because deterministic-resume CI test on every PR is a load-bearing exit-criterion proof.
- **sqlx + testcontainers in default CI** adds a 15th CI job that depends on Docker. ubuntu-latest already has Docker, so cost is the test runtime (~40 s end-to-end). Trade-off: stronger TRAIN-04 acceptance vs slightly slower CI.
- **Cross-machine bit-identical CUDA resume is documented as best-effort.** Same software stack (cuDNN, torch version) + same GPU SM = bit-identical. Different GPUs (e.g., A100 → H100) are NOT bit-identical by design. Phase 4 mdBook chapter spells this out.
- **HF transformers + accelerate pulls a lot of Python stuff.** Users of rollout will already have these for vLLM (Phase 3) — adding them is incremental, not net-new.

## Claude's Discretion (deferred to research/planner)

- Crate organization within rollout-snapshots
- accelerate / transformers version pins
- FSDP-vs-DDP heuristic
- Loss-masking implementation for AssistantOnly (depends on Qwen2.5 chat template tokens)
- Plan type definition (Phase 4 ships a minimal placeholder)
- GradHandle shape (opaque newtype)
- sqlx-data.json location
- Postgres runs/workers/heartbeats table schemas (deferred to Phase 6)
- mdBook chapter file naming

## Deferred Ideas (captured in CONTEXT.md `<deferred>`)

Streaming generation (Phase 8), PPO/GRPO/DPO/IPO/KTO (Phases 9/10), Buffer/Process/EpisodicMemory snapshot kinds (Phases 9/11/8), HF datasets Hub integration (Phase 7), S3/GCS object stores (Phase 5), runs/workers Postgres tables (Phase 6), first-class tokenizer trait (Phase 7), reader/writer worker split (Phase 6), runtime backend selection (Phase 8), trl Trainer / HF model upload (post-1.0), DeepSpeed (post-1.0), snapshot prune CLI (may slip to Phase 9), speculative decoding tuning (Phase 9+), cross-machine bit-identical CUDA resume (never a v1 guarantee).
