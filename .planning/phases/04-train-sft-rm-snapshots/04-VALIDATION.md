---
phase: 4
slug: train-sft-rm-snapshots
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-05-21
---

# Phase 4 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` (Rust 1.84+ workspace) + pytest where Python-side fixtures land (under `python/rollout/backends/vllm/tests/`) |
| **Config file** | Workspace `Cargo.toml` test config; per-crate `tests/` directories; `pytest.ini` if Python tests added (Phase 4 mostly Rust-driven via MockBackend) |
| **Quick run command** | `cargo test -p <crate>` for the crate just edited |
| **Full suite command** | `make test` (mirrors CI `test` job) + `make postgres-test` (testcontainers integration) |
| **Estimated runtime** | ~30 s for `cargo test --workspace` (excl. `#[ignore]` live tests); +30 s for `postgres-test` (testcontainers boot + integration) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p <crate>` for the crate just edited
- **After every plan wave:** Run `cargo test --workspace --all-features` (excluding `#[ignore]` gates) + `make postgres-test` if Postgres-touching wave
- **Before `/gsd:verify-work`:** Full suite must be green including `make train-smoke` (gated on `ROLLOUT_TRANSFORMERS_AVAILABLE=1` for local verification; CI default fires only the MockBackend path)
- **Max feedback latency:** ~30 s for unit/integration; +30 s for postgres-integration job

---

## Per-Task Verification Map

> Filled in by the planner during PLAN.md generation. Each task lists its `<automated>` verify command + `<test_file>` reference.

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| _Planner fills_ | _NN_ | _N_ | _TRAIN-NN_ | unit / integration / smoke / lint | _command_ | ✅ / ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

**Expected coverage shape (per 04-RESEARCH.md §Validation Architecture):**

- **TRAIN-01 (rollout-algo-sft):** unit (`crates/rollout-algo-sft/tests/snapshot_resume.rs`, packing + assistant-mask unit tests); smoke (`make train-smoke` — gated)
- **TRAIN-02 (rollout-algo-rm):** unit (`crates/rollout-algo-rm/tests/bradley_terry_loss.rs`, content-addressed checkpoint test); smoke (gated)
- **TRAIN-03 (training-state snapshots):** unit (`crates/rollout-snapshots/tests/`, `crates/rollout-algo-sft/tests/snapshot_resume.rs` byte-compare assertion — load-bearing); CPU bit-identical resume (every CI build); CUDA bit-identical (gated `#[ignore]`)
- **TRAIN-04 (Postgres backend):** integration (`crates/rollout-storage/tests/postgres_integration.rs` via testcontainers Postgres 16 — default-fire CI job); CAS atomicity, LISTEN/NOTIFY watch, migration application, connection pool reuse

---

## Wave 0 Requirements

- [ ] `crates/rollout-core/src/traits/algorithm.rs` — extended `PolicyAlgorithm` trait (spec 02 §2 surface) — REQ TRAIN-01/02/03
- [ ] `crates/rollout-core/src/traits/backend.rs` — `TrainableBackend: InferenceBackend` (D-TRAIN-PATH-01) — REQ TRAIN-01/02
- [ ] `crates/rollout-core/src/traits/snapshot.rs` (or `traits/storage.rs`) — `Snapshotter` trait (spec 04 §5.2) — REQ TRAIN-03
- [ ] `crates/rollout-core/src/traits/storage.rs` — add `watch_stream() -> BoxStream<StorageEvent>` parallel method — REQ TRAIN-04 (PgListener cross-process)
- [ ] `crates/rollout-core/src/config/` — ~15 supporting types (Snapshot, SnapshotKind, SnapshotRequest, SnapshotFilter, PrunePolicy, RetentionPolicy, SnapshotPolicy, PeriodicPolicy, RestoreTarget, AlgoDependencies, AlgoContext, OptimizerSettings, OptimizerKind, LrSchedule, TrainingBudget, DatasetRef, PackingPolicy, PackingKind, LossScope, MaskSpec, SftSettings, RmSettings, RmHeadKind, TrainBatch, LossOutput, GradHandle)
- [ ] `crates/rollout-algo-sft/` — NEW crate skeleton + `tests/snapshot_resume.rs` stub
- [ ] `crates/rollout-algo-rm/` — NEW crate skeleton + `tests/bradley_terry_loss.rs` stub
- [ ] `crates/rollout-snapshots/` — NEW crate skeleton + `tests/` round-trip stub
- [ ] `crates/rollout-runtime-batch/src/mock_backend.rs` — `TrainableBackend` impl for MockBackend (deterministic SGD on `ndarray::Array1<f32>`)
- [ ] `crates/rollout-backend-vllm/Cargo.toml` — add `train` feature
- [ ] `crates/rollout-storage/Cargo.toml` — add `postgres` feature
- [ ] `database/migrations/0001_init.sql`, `0002_snapshots.sql` — kv + snapshots + events tables (spec 04 §3.2)
- [ ] `.sqlx/` workspace-level offline data directory — committed
- [ ] `crates/rollout-core/tests/dependency_direction.rs` — invariants #7/#8/#9 + fixture violations
- [ ] `Cargo.toml` (workspace) — register 3 new crates + sqlx 0.8 + tar workspace deps

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Cross-machine bit-identical resume on CUDA | TRAIN-03 | Requires identical SM + cuDNN versions across machines; cannot be exercised by single-runner CI | Run snapshot on machine A (CUDA), restore on machine B (CUDA, same SM + cuDNN), assert weight checksum match — best-effort per spec 04 §5.3 |
| `rollout train sft --config examples/sft-tiny.toml` end-to-end on real Qwen2.5-0.5B-Instruct (CPU) | TRAIN-01 (exit criterion) | Requires HF transformers + accelerate Python install (~2 GB); gated `#[ignore]` in CI; runs locally via `make train-smoke` with `ROLLOUT_TRANSFORMERS_AVAILABLE=1` | `ROLLOUT_TRANSFORMERS_AVAILABLE=1 make train-smoke`; expect completion in <5 min on M-series CPU; verify final snapshot persisted to `<artifact_dir>/snapshots/` |
| `rollout train rm` on real model (CPU) | TRAIN-02 | Same as above | `ROLLOUT_TRANSFORMERS_AVAILABLE=1 cargo run -p rollout-cli --features vllm,train -- train rm --config examples/rm-tiny.toml` |
| Postgres `PgListener` cross-process watch fan-out under real network conditions | TRAIN-04 | testcontainers covers single-host case; multi-host network-partition behavior cannot be exercised in CI; relevant for Phase 5 cloud testing | Document in `docs/book/src/training/postgres-backend.md` §Watch behavior; revisit during Phase 5 cloud integration |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references (3 new crates, 2 feature flags, ~15 supporting types, 3 new lint invariants, sqlx offline data)
- [ ] No watch-mode flags (`cargo test`, not `cargo watch`)
- [ ] Feedback latency < 60 s per task (workspace test) and < 90 s per wave (workspace + postgres-integration)
- [ ] `nyquist_compliant: true` set in frontmatter once planner has filled the verification map

**Approval:** pending
