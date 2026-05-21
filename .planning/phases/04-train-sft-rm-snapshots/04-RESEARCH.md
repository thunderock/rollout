# Phase 4: SFT + reward-model training + training-state snapshots — Research

**Researched:** 2026-05-21
**Domain:** Rust async + PyO3 training path (HF transformers + accelerate), bit-identical training-state snapshots, sqlx 0.8 Postgres storage backend with LISTEN/NOTIFY.
**Confidence:** HIGH on PyO3 reuse + sqlx + snapshot determinism stack; MEDIUM on Qwen2.5 chat-template assistant mask (chat template lacks `{% generation %}` markers — needs token-id-level fallback); MEDIUM on accelerate FSDP/DDP heuristic API surface (Accelerator() auto-selects; we just probe device count).

---

## Phase Goal

Phase 4 delivers the first end-to-end training story: SFT + Bradley-Terry RM + bit-identical-resume training-state snapshots + the Postgres `Storage` backend alongside embedded. Three new crates ship (`rollout-algo-sft`, `rollout-algo-rm`, `rollout-snapshots`), two crate extensions via Cargo features (`rollout-backend-vllm[train]` adds HF transformers + accelerate + FSDP/DDP through the existing PyO3 thread; `rollout-storage[postgres]` adds sqlx 0.8 + PgListener), one Wave-0 trait surgery (extend `PolicyAlgorithm` per spec 02 §2, add `TrainableBackend: InferenceBackend` per D-TRAIN-PATH-01, replace the existing 2-method `Snapshotter` placeholder with the spec 04 §5.2 4-method shape, plus ~15 supporting types), and CLI subcommands `train sft` / `train rm` / `snapshot list` / `snapshot show`.

## Phase Boundary

In scope: TRAIN-01..04 only. Out: PPO/GRPO (Phase 9), DPO/IPO/KTO (Phase 10), Buffer / Process / EpisodicMemory snapshot kinds (Phases 9 / 11 / 8), cloud object stores for snapshot blobs (Phase 5), HF datasets Hub integration (Phase 7), runtime backend selection (Phase 8), `runs`/`workers`/`work_items` Postgres tables (Phase 6 — Phase 4 ships only `kv` + `snapshots` + `events`), `rollout snapshot prune` CLI may slip to Phase 9 if scope is tight.

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Training execution path:**
- **D-TRAIN-PATH-01** — Sibling `TrainableBackend: InferenceBackend` trait in `rollout-core`. Methods: `set_train_mode(enabled)`, `forward_with_loss(batch, loss_scope)`, `optimizer_step(grads, opt)`, `save_weights() -> ContentId`, `load_weights(weights_id)`.
- **D-TRAIN-PATH-02** — In-crate via new `train` Cargo feature on `rollout-backend-vllm` (NOT a new crate). `VllmBackend` impls both traits; same dedicated Python OS thread (`rollout-py-vllm-<engine_id>`); only one mode active per run (selected by `set_train_mode`).
- **D-TRAIN-PATH-03** — HF transformers + accelerate + FSDP (with DDP fallback for single-device CPU runs). `accelerate.Accelerator()` wraps model + optimizer + LR scheduler + data loader.
- **D-TRAIN-PATH-04** — Single `Arc<dyn TrainableBackend>` slot in `AlgoDependencies`. (Actor/learner split deferred to Phase 9.)
- **D-TRAIN-PATH-05** — `MockBackend` (behind `test-mock-backend`) extends with `TrainableBackend` impl. Fake weights as `ndarray::Array1<f32>`; `forward_with_loss` returns loss=0.5 + zero grads; `optimizer_step` is plain SGD against the fake weights.
- **D-TRAIN-PATH-06** — Cargo-feature backend selection. Production: `--features vllm,train`. Tests: `--features test-mock-backend`.

**Wave 0 trait surgery (mirrors 02-00 / 03-00):**
- **D-WAVE0-01** — Full Wave 0 surgery in 2 tasks. Task 1: trait surface (extend `PolicyAlgorithm`; replace `Snapshotter` placeholder with spec 04 §5.2 shape; add `TrainableBackend`; ~15 supporting types). Task 2: register 3 new crates; `train` feature on `rollout-backend-vllm`; `postgres` feature on `rollout-storage`; add sqlx 0.8 + accelerate-related workspace deps; arch-lint invariants #7/#8/#9; spec edits (02 §2a, 04 §5a, 08 §2.5 verify).
- **D-WAVE0-02** — `PolicyAlgorithm` full surface per spec 02 §2: `id()`, `type Settings`, `from_settings`, `required_roles()`, `validate_plan(plan)`, `run(ctx)`, `snapshot_save()`, `snapshot_restore(snapshot)`. Same `#[non_exhaustive]` hygiene as Phase 3's `SamplingParams`.
- **D-WAVE0-03** — Supporting types in Wave 0: `Snapshot`, `SnapshotKind { TrainState, Buffer, Process, EpisodicMemory }` (only `TrainState` impls live), `SnapshotPart`, `RestoreTarget { SameRun, Fork { new_run_id }, Worker { worker_id } }`, `SnapshotRequest`, `SnapshotFilter`, `PrunePolicy`, `RetentionPolicy`, `SnapshotPolicy`, `PeriodicPolicy`, `AlgoDependencies { backend, storage, object, snapshots, events }`, `AlgoContext<'a> { plan, worker, cancel, clock }`, `Plan` (placeholder), `OptimizerSettings`, `OptimizerKind`, `LrSchedule`, `TrainingBudget`, `DatasetRef { JsonlPath(PathBuf), Other(SmolStr) }`, `PackingPolicy`, `PackingKind { Concat, Bucketed, Off }`, `LossScope { AssistantOnly, Full, Custom(MaskSpec) }`, `MaskSpec`, `SftSettings`, `RmSettings`, `RmHeadKind { BradleyTerry, PairwiseLogistic }`, `TrainBatch`, `LossOutput`, `GradHandle`, verify `WorkerRole::LearnerWorker` exists or add.
- **D-WAVE0-04** — Algorithm crates: `rollout-algo-sft` and `rollout-algo-rm` as separate crates.
- **D-WAVE0-05** — `rollout-snapshots` standalone crate. Impls `Snapshotter` against injected `Arc<dyn Storage>` (metadata) + `Arc<dyn ObjectStore>` (blobs). Only `TrainState` kind in Phase 4; others return `Fatal { PluginContract, msg: "Phase N: <kind>" }`.

**Snapshot determinism (TRAIN-03):**
- **D-DETERM-01** — Determinism stack: `accelerate.Accelerator.save_state(dir)` + `load_state(dir)` captures model + optimizer + LR scheduler + RNG + dataloader cursor. Additionally `torch.use_deterministic_algorithms(True)` + `CUBLAS_WORKSPACE_CONFIG=:4096:8` (env-set at Python thread startup) + `torch.set_float32_matmul_precision("highest")`. CPU runs bit-identical unconditionally; CUDA bit-identical IFF same GPU SM + same cuDNN. Cross-machine resume = best-effort.
- **D-DETERM-02** — One tar per snapshot, one `ContentId`. After `accelerate.save_state(dir)`, tar the directory (deterministic ordering by name; no compression), blake3-hash the tar, write to `FsObjectStore`. Restore: fetch tar, blake3-verify, extract to tempdir, `accelerate.load_state(tempdir)`.
- **D-DETERM-03** — `crates/rollout-algo-sft/tests/snapshot_resume.rs`: 10 SGD steps against fake weights with fixed RNG seed; snapshot at step 5; restart with `--resume <snapshot_id>`; run 5 more; byte-compare final weights against non-interrupted run's step-10 weights. Runs every CI build via MockBackend (no GPU, no HF transformers); ~1 s. Full HF-transformers + Qwen2.5-0.5B-Instruct equivalent gated `#[ignore]` unless `ROLLOUT_TRANSFORMERS_AVAILABLE=1`.
- **D-DETERM-04** — Default `SnapshotPolicy`: `on_completion: true`, `periodic: PeriodicPolicy { interval_steps: 500, ..None, kinds: [TrainState] }`, `on_preemption: true`, `retention: { keep_last: 3, keep_labeled: true, max_age: None }`. SIGTERM hook triggers opportunistic snapshot before exit.
- **D-DETERM-05** — Algorithm internal state via `serde_json::Value`. `PolicyAlgorithm::snapshot_save` returns `Snapshot::with_meta(algorithm_id, serde_json::Value)`. Algorithm packs its own incremental state (curriculum cursor, schedule overrides) into JSON.

**Postgres backend (TRAIN-04) + dataset loader:**
- **D-PG-01** — sqlx 0.8 as Postgres client. Compile-time-checked SQL (`sqlx::query!()`); built-in async pool; `PgListener` for LISTEN/NOTIFY; `sqlx::migrate!()` embeds SQL migrations. Workspace dep: `sqlx = { version = "0.8", default-features = false, features = ["runtime-tokio", "postgres", "macros", "migrate", "json", "chrono", "uuid"] }`. Commit `sqlx-data.json` (offline mode).
- **D-PG-02** — Migrations under `database/migrations/`. Numbered `.sql` files (`0001_init.sql`, `0002_snapshots.sql`). `PostgresStorage::new(url, pool_size)` runs `sqlx::migrate!("../../database/migrations").run(&pool)` at construction.
- **D-PG-03** — `watch()` via `PgListener`: storage writes also `pg_notify('rollout_watch_<namespace>', <postcard-encoded-key-suffix>)`. Cross-process watch works.
- **D-PG-04** — testcontainers CI integration in DEFAULT CI (not opt-in). New `postgres-integration` job in `.github/workflows/ci.yml`: `needs: test`; spins up testcontainers-modules Postgres 16; runs `cargo test -p rollout-storage --features postgres --test postgres_integration -- --include-ignored`.
- **D-PG-05** — Schema (Phase 4 subset): one generic `kv(namespace TEXT, run_id UUID, path TEXT[], value BYTEA, version BIGINT, updated_at TIMESTAMPTZ)` with PK `(namespace, run_id, path)`; plus `snapshots` + `events` tables. `runs`/`workers`/`heartbeats`/`work_items` defer to Phase 6.
- **D-DATA-01** — `DatasetRef::JsonlPath(PathBuf)` is the only Phase-4 variant. SFT schema: `{"prompt", "completion"}` OR `{"messages": [{role, content}, ...]}`. RM schema: `{"prompt", "chosen", "rejected"}`. `DatasetRef::Other(SmolStr)` enumerated for forward-compat; reading returns `Fatal(ConfigInvalid)` in Phase 4.
- **D-DATA-02** — Tokenization Python-side via the model's HF tokenizer. Packing in Python via custom collator. `LossScope::AssistantOnly` masks via chat template role markers.
- **D-DATA-03** — Test model: `Qwen/Qwen2.5-0.5B-Instruct`. Apache-2.0, CPU-runnable, vLLM-supported.

### Claude's Discretion

- Crate organization within `rollout-snapshots`: `kind/train_state.rs` module from day one vs. inline?
- accelerate version pin (likely `>=1.0,<2.0`).
- HF transformers version pin (likely `>=4.45,<5.0` — must support Qwen2.5).
- FSDP vs DDP heuristic: probe device count at runtime.
- Loss-masking exact implementation for `LossScope::AssistantOnly` — depends on Qwen2.5's chat template token IDs.
- `Plan` type definition — Phase 4 minimal placeholder; Phase 6 expands.
- `GradHandle` shape — opaque Python-side reference vs. marker.
- `sqlx-data.json` location (crate-level vs. project-level).
- Postgres `runs`/`workers` placeholders in Phase 4? (No — defer to Phase 6.)
- mdBook chapter file naming under `docs/book/src/training/`.
- Whether `rollout snapshot prune` ships in Phase 4 or slips to Phase 9.

### Deferred Ideas (OUT OF SCOPE)

- Streaming generation / online inference — Phase 8 (`INFER-01..02`).
- PPO / GRPO / DPO / IPO / KTO — Phases 9, 10.
- Buffer / Process / EpisodicMemory snapshot kinds — Phases 9 / 11 / 8.
- HF datasets Hub integration — Phase 7.
- S3 / GCS object stores for snapshot blobs — Phase 5.
- `runs` / `workers` / `heartbeats` / `work_items` Postgres tables — Phase 6.
- First-class tokenizer trait — Phase 7 if HARNESS-* needs.
- Reader/writer worker split — Phase 6.
- Runtime backend selection (TOML-driven) — Phase 8.
- trl Trainer / HF model upload after training — post-v1.
- DeepSpeed integration — post-v1.
- Cross-machine bit-identical resume on CUDA — best-effort, never a v1 guarantee.
- `rollout snapshot prune` CLI may slip from Phase 4 to Phase 9.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| TRAIN-01 | `rollout-algo-sft`: SFT with packing, loss-on-assistant masking, JSONL data loader | §1 PyO3 training thread shape; §7 Qwen2.5 chat template mask; §8 packing collator |
| TRAIN-02 | `rollout-algo-rm`: Bradley-Terry pairwise loss; CA checkpoint | §6 storage trait + factory; loss math in spec 02 §7 |
| TRAIN-03 | Training-state snapshots — weights + optimizer + LR cursor + step + RNG + algo state; bit-identical restore | §2 accelerate.save_state determinism stack; §3 MockBackend snapshot_resume.rs; §4 rollout-snapshots crate API |
| TRAIN-04 | Postgres `Storage` backend alongside embedded; identical trait API; CI via containerized Postgres | §5 sqlx 0.8 + migrations + PgListener; §6 Storage factory; §12 CI integration |
</phase_requirements>

## Project Constraints (from AGENTS.md §9 standing rules)

- **DOCS-01..03 per-commit doc + test policy.** Every Phase-4 commit that modifies code under `crates/` MUST also touch `docs/` content, inline rustdoc / Python docstrings, or tests. CI job `docs-test-policy` enforces.
- **Rustdoc gate.** `cargo doc --workspace --no-deps --all-features` with `RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs"` must stay green.
- **Conventional commits.** `feat(04-NN): ...`, `docs(04-NN): ...`, `fix(04-NN): ...`. Per-plan atomic.
- **No `unwrap()` on worker hot paths** (AGENTS.md §4 Never list).
- **No direct `aws-sdk-*` / `google-cloud-*` calls** outside `rollout-cloud-*` crates (dep-direction lint).
- **Layered cloud abstraction** — `rollout-algo-*` may NOT depend on `rollout-cloud-*` or `rollout-transport`; `rollout-snapshots` may NOT depend on `rollout-algo-*`.
- **Async-native end-to-end.** No blocking I/O on async hot paths (the PyO3 thread is the ONE allowed boundary, and only because the dedicated thread bounds the blocking surface).
- **Single source of truth for config.** Rust types are authoritative. `cargo xtask schema-gen` regenerates JSON Schema + Python stubs; CI fails on drift.
- **Workspace lints.** `pedantic = "warn"`, `unsafe_code = "forbid"` (except `rollout-plugin-abi`), `clippy::all = "warn"`.
- **MSRV 1.88.0.**

---

## Summary

Phase 4 builds atop the Phase-3 PyO3 substrate. The training path reuses the dedicated Python OS thread (`rollout-py-vllm-<engine_id>`) and the asyncio↔Tokio bridge (`pyo3_async_runtimes::tokio::run_until_complete` — verified by `pyo3_bridge_smoke.rs` to release the GIL across await). The new `train` Cargo feature on `rollout-backend-vllm` flips the thread between vLLM inference and an `accelerate.Accelerator`-wrapped HF transformers training loop via `set_train_mode`. Determinism rides on `accelerate.save_state` (model + optimizer + LR scheduler + RNG + optionally dataloader cursor) plus a fixed determinism preamble (`torch.use_deterministic_algorithms(True)` + `CUBLAS_WORKSPACE_CONFIG=:4096:8` + `PYTHONHASHSEED` + `cudnn.deterministic=True`), tar'd into one blob hashed with blake3, persisted in `FsObjectStore`.

The Postgres backend is a sqlx-0.8 implementation with a single `kv` table + two specialized tables (`snapshots`, `events`), migrations embedded via `sqlx::migrate!()`, `PgListener`-backed `watch()` over `pg_notify`, and testcontainers-modules CI integration on `ubuntu-latest` (Docker is available so no opt-in gate). The biggest single risk in the Postgres slice is that the existing `Storage::watch()` trait returns `tokio::sync::broadcast::Receiver<StorageEvent>`, which doesn't naturally bridge to a cross-process listener — Phase 4 must change the return type to a `BoxStream<StorageEvent>` OR add a parallel `watch_stream()` method on the trait (recommend the latter to keep the Phase-2 broadcast surface intact for the embedded backend).

The Wave-0 trait surgery is the load-bearing change: the current `Snapshotter` trait in `rollout-core::traits::storage` is a 2-method placeholder (`save(key, bytes)`/`load(key)`), incompatible with spec 04 §5.2's 4-method shape. Replace it wholesale; the only existing consumer is the unimplemented `AlgoDependencies::snapshots` slot, so the blast radius is contained.

**Primary recommendation:** ship the Wave-0 trait surgery (Tasks 04-00-a + 04-00-b) first; then ship 04-01 (`rollout-algo-sft` skeleton + MockBackend `TrainableBackend` impl + `snapshot_resume.rs` byte-compare test) and 04-02 (`rollout-snapshots` crate + tar+blake3 round-trip test) in parallel; then 04-03 (`rollout-storage[postgres]` + sqlx + migrations + testcontainers integration test); then 04-04 (`rollout-algo-rm` + Bradley-Terry loss test) parallel with 04-05 (`rollout-backend-vllm[train]` + accelerate.Accelerator integration + Qwen2.5 SFT smoke); then 04-06 (CLI `train sft` / `train rm` / `snapshot list` / `snapshot show`); finish with 04-07 (examples + scripts + mdBook chapters + CI postgres-integration job).

---

## Standard Stack

### Core (verified via Context7 / WebSearch / official docs)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `sqlx` | `0.8` (workspace dep) | Postgres async client + migrations + PgListener | Compile-time-checked SQL; PgListener auto-reconnect; `sqlx::migrate!()` embeds SQL; first-class Postgres LISTEN/NOTIFY support |
| `testcontainers` | `0.23.x` | Docker container orchestration for CI | Phase 4 CI integration must spin Postgres 16 on `ubuntu-latest`; testcontainers is the standard Rust testcontainers binding |
| `testcontainers-modules` | `0.11.x` (postgres feature) | Pre-built Postgres image module | One-liner `postgres::Postgres::default().start()`; supports `with_init_sql()` |
| `tar` | `0.4` | Deterministic tar archive build for snapshots | One tar per snapshot; deterministic ordering (`--sort=name --owner=0 --group=0 --numeric-owner --mtime=@0`); no compression |
| `blake3` | `1.8` (already workspace dep) | Hash for `ContentId` of snapshot tar | Already used for sample-IDs in Phase 3; reuse for snapshot blob CAS keys |
| `ndarray` | `0.16` | Fake-weights backing for MockBackend `TrainableBackend` | Test-only; `Array1<f32>` with `ndarray::Zip` for plain SGD step |
| `serde_json` | `1` (already workspace dep) | `Snapshot::meta: serde_json::Value` for algorithm internal state (D-DETERM-05) | Already in workspace |

**Python-side libraries (gated by `train` Cargo feature; never linked at build time; users `pip install` separately):**

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `transformers` | `>=4.45,<5.0` | HF model loader + tokenizer + chat template | Min `4.37` supports Qwen2.5; we pin `>=4.45` for stable `apply_chat_template(return_assistant_tokens_mask=True)` support |
| `accelerate` | `>=1.0,<2.0` | Distributed training wrapper (FSDP + DDP + DeepSpeed unified API) + save_state/load_state | The "blessed" v1 distributed stack (spec 02 §11 names it); accelerate 1.0 stabilized the `Accelerator.save_state(dir)` + `load_state(dir)` surface |
| `torch` | `>=2.1,<3.0` | Underlying tensor + autograd | Required by transformers + accelerate; CUBLAS_WORKSPACE_CONFIG=:4096:8 contract verified against torch ≥1.12 |
| `safetensors` | (transitive) | Model shard format | Used by accelerate.save_state under the hood; we don't depend on it directly |
| `torchdata` | `>=0.8` (optional) | `use_stateful_dataloader=True` for sampler-position persistence | OPTIONAL — without it, dataloader cursor is not in accelerate.save_state and we restore from step counter instead |
| `huggingface_hub` | (transitive) | Model download + `model_info().sha` | Already used in Phase 3 vllm path |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `sqlx-cli` | `0.8` | `cargo sqlx prepare` for offline mode | Local dev + CI offline-mode check. NOT a workspace dep; install via `cargo install sqlx-cli`. |
| `chrono` | already in sqlx feature | `DateTime<Utc>` for `Snapshot.created_at` (spec 04 §5.1) | Already pulled via `sqlx` `chrono` feature |
| `uuid` | already in sqlx feature | Postgres `UUID` ↔ `RunId` mapping | `RunId` is ULID-encoded as UUID at the Postgres boundary |
| `tokio` | `1.40` (already workspace) | async runtime | Already pinned |
| `pyo3` | `0.28` (already workspace) | `Python::attach` + GIL | Already pinned |
| `pyo3-async-runtimes` | `0.28` (already workspace, `tokio-runtime` feature) | `run_until_complete` bridge | Already verified by `pyo3_bridge_smoke.rs` |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `sqlx` | `tokio-postgres` + `deadpool-postgres` | sqlx wins for compile-time SQL checks + integrated migrations + first-class PgListener. tokio-postgres has lower-level control; not needed here. |
| `testcontainers-modules` Postgres | docker-compose + manual cleanup | testcontainers handles container teardown per-test; less flaky in CI. |
| `safetensors` direct manipulation | rely on accelerate.save_state | We don't unpack the safetensors; accelerate writes them inside the directory we tar. Simpler. |
| One tar per part (model, optimizer, RNG) | One tar per snapshot | D-DETERM-02 locks one tar per snapshot; loses dedup but simpler. Revisit Phase 5 if S3 costs warrant. |
| Compressed tar (zstd) | Uncompressed tar | D-DETERM-02 says no compression for hash stability across machines. zstd output can drift across versions/threads. |
| `accelerate.save_model()` (model-only) | `accelerate.save_state(dir)` (full state) | Only `save_state` captures optimizer + LR + RNG. Required by TRAIN-03. |
| trl SFTTrainer | direct accelerate.Accelerator | trl wraps accelerate but adds opinions (e.g., chat-template handling). We control packing + masking explicitly; trl integration is post-v1. |
| `transformers.Trainer` | direct accelerate.Accelerator | Trainer wraps accelerate but its loop is non-extensible enough that snapshot/RNG control is harder. accelerate.Accelerator gives us the loop. |

**Installation:**

```bash
# Rust (Phase 4 adds to workspace Cargo.toml):
# (no install step — `cargo build` picks them up from workspace.dependencies)

# Python (training-mode dev box, NOT default CI):
pip install 'transformers>=4.45,<5.0' 'accelerate>=1.0,<2.0' 'torch>=2.1,<3.0' torchdata
# Optional, for token-id mask validation tests:
pip install 'huggingface_hub>=0.24'
```

**Version verification** (run before locking versions in Cargo.toml):

```bash
cargo search sqlx | head -3                        # confirm 0.8.x latest
cargo search testcontainers-modules | head -3      # confirm 0.11.x latest
cargo search tar | head -3                         # confirm 0.4.x latest
cargo search ndarray | head -3                     # confirm 0.16.x latest
pip index versions accelerate                      # confirm 1.x latest
pip index versions transformers                    # confirm 4.45+ latest
```

---

## Architecture Patterns

### Recommended Project Structure

```
crates/
├── rollout-core/
│   └── src/
│       ├── traits/
│       │   ├── backend.rs       # extend with TrainableBackend (sibling of InferenceBackend)
│       │   ├── algorithm.rs     # REWRITE: PolicyAlgorithm full surface per spec 02 §2
│       │   ├── snapshot.rs      # NEW: Snapshotter trait per spec 04 §5.2 (moved out of storage.rs)
│       │   └── storage.rs       # leave Storage/StorageTxn; remove placeholder Snapshotter
│       └── config/
│           └── mod.rs           # add OptimizerSettings, TrainingBudget, DatasetRef, etc.
├── rollout-backend-vllm/
│   ├── Cargo.toml               # add `train` Cargo feature
│   └── src/
│       ├── lib.rs               # gate `train` module on feature
│       ├── backend.rs           # impl TrainableBackend when feature on
│       ├── engine.rs            # extend VllmTask with SetTrainMode / ForwardWithLoss / OptimizerStep / SaveWeights / LoadWeights
│       └── train.rs             # NEW: Python-side glue (accelerate.Accelerator wrapper)
├── rollout-storage/
│   ├── Cargo.toml               # add `postgres` Cargo feature
│   └── src/
│       ├── lib.rs               # Storage factory: enum StorageConfig → Arc<dyn Storage>
│       ├── embedded/            # untouched
│       └── postgres/            # NEW (feature-gated)
│           ├── mod.rs           # PostgresStorage + PostgresTxn
│           ├── listener.rs      # PgListener-backed watch_stream()
│           └── migrations.rs    # sqlx::migrate!() macro embed
├── rollout-algo-sft/            # NEW crate
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs               # SftAlgo : PolicyAlgorithm; SftSettings
│   │   ├── data.rs              # JsonlPath loader
│   │   ├── pack.rs              # Concat / Bucketed Python collator dispatch
│   │   └── loss_mask.rs         # LossScope dispatch (calls into Python tokenizer)
│   └── tests/
│       ├── snapshot_resume.rs   # LOAD-BEARING: byte-compare bit-identical resume
│       └── happy_path.rs        # 2-step SFT with MockBackend
├── rollout-algo-rm/             # NEW crate
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs               # RmAlgo : PolicyAlgorithm; RmSettings
│   │   └── loss.rs              # Bradley-Terry pairwise loss math
│   └── tests/
│       ├── snapshot_resume.rs   # parallel to SFT
│       └── pairwise_loss.rs     # unit math
├── rollout-snapshots/           # NEW crate
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs               # SnapshotterImpl : Snapshotter
│       ├── kind/
│       │   └── train_state.rs   # one tar per snapshot; tar build + blake3 hash + extract
│       ├── tar_build.rs         # deterministic tar packer (--sort=name --owner=0 --group=0 --numeric-owner --mtime=@0)
│       └── policy.rs            # SnapshotPolicy / PeriodicPolicy / RetentionPolicy enforcement
└── rollout-cli/
    └── src/
        └── main.rs              # add Cmd::Train(TrainCmd) and Cmd::Snapshot(SnapshotCmd)

database/
└── migrations/                  # NEW (Phase 1 reserved the directory)
    ├── 0001_init.sql            # kv table
    └── 0002_snapshots.sql       # snapshots + events tables

examples/
├── sft-tiny.toml                # NEW
├── sft-tiny.jsonl               # NEW (4 messages)
├── rm-tiny.toml                 # NEW
└── rm-tiny.jsonl                # NEW (4 pairs)

scripts/
├── train-smoke.sh               # NEW — mirrors infer-smoke.sh
└── postgres-test.sh             # NEW (optional helper) — testcontainers spin + integration tests

docs/book/src/
├── SUMMARY.md                   # add Training section after Inference
└── training/                    # NEW
    ├── index.md
    ├── sft.md
    ├── rm.md
    ├── snapshots.md
    ├── postgres-backend.md
    ├── determinism.md
    ├── cli.md
    └── cpu-mode.md
```

### Pattern 1: PyO3 training-thread mode switch (extends Phase 3 dedicated-thread)

**What:** The existing `rollout-py-vllm-<engine_id>` thread (Phase 3 `engine.rs`) gains 5 new `VllmTask` variants. Inference variants (`Init`, `Generate`, `Shutdown`) stay unchanged. Training variants land behind `#[cfg(feature = "train")]`:

```rust
// crates/rollout-backend-vllm/src/engine.rs (extend)
#[allow(clippy::large_enum_variant, dead_code)]
pub(crate) enum VllmTask {
    // Phase 3 (unchanged):
    Init { model: ModelRef, reply: oneshot::Sender<Result<String, CoreError>> },
    Generate { prompt: String, params: SamplingParams, request_id: String,
               reply: oneshot::Sender<Result<Completion, CoreError>> },
    Shutdown,

    // Phase 4 (gated):
    #[cfg(feature = "train")]
    SetTrainMode { enabled: bool, reply: oneshot::Sender<Result<(), CoreError>> },
    #[cfg(feature = "train")]
    ForwardWithLoss { batch: TrainBatch, loss_scope: LossScope,
                      reply: oneshot::Sender<Result<LossOutput, CoreError>> },
    #[cfg(feature = "train")]
    OptimizerStep { grads: GradHandle, opt: OptimizerSettings,
                    reply: oneshot::Sender<Result<(), CoreError>> },
    #[cfg(feature = "train")]
    SaveWeights { reply: oneshot::Sender<Result<ContentId, CoreError>> },
    #[cfg(feature = "train")]
    LoadWeights { weights_id: ContentId, reply: oneshot::Sender<Result<(), CoreError>> },
}
```

The worker `set_train_mode(enabled)` triggers a destroy-old/construct-new on the Python side:
- `enabled=true` and we currently have an `AsyncLLMEngine`: explicitly `del engine; gc.collect(); torch.cuda.empty_cache()`, then construct `accelerate.Accelerator()` + `AutoModelForCausalLM.from_pretrained(...)` + optimizer.
- `enabled=false` and we currently have an `Accelerator`: explicitly `del accelerator; del model; del optimizer; gc.collect(); torch.cuda.empty_cache()`, then construct fresh `AsyncLLMEngine`.

This avoids holding two large models in CUDA memory simultaneously. Phase 4 ships only the training-side construction (the inference→train and train→inference paths in Phase 4 only fire on startup, since algos pick one mode per run; the bidirectional flips are Phase 9 PPO).

**When to use:** Whenever the algorithm calls `backend.set_train_mode(true)` at the start of its `run()`. Default Phase 4 path is training-only — the inference engine never spins up.

**Crash recovery:** if the training thread panics Python-side (e.g., OOM), the panic propagates to the `tokio::sync::oneshot` reply channel as a closed-channel error. The Rust side maps closed-channel + thread-no-longer-running into `CoreError::Fatal { kind: PluginContract, msg: "training thread crashed: <message>" }`. The thread is restarted only by tearing down the whole `VllmBackend` and constructing a fresh one (no per-call restart — too much state to reconstruct).

### Pattern 2: forward_with_loss await semantics

**What:** Same shape as Phase 3 `run_generate` (asyncio↔Tokio bridge via `pyo3_async_runtimes::tokio::run_until_complete`). But the Python-side `forward_with_loss` is SYNC, not async — there's no I/O wait inside (it's a CUDA kernel sequence). So we can call it directly on the Python thread with the GIL released via `py.detach(|| ...)`:

```rust
// crates/rollout-backend-vllm/src/train.rs
#[cfg(feature = "train")]
fn run_forward_with_loss(
    module: &pyo3::Py<pyo3::types::PyModule>,
    batch: &TrainBatch,
    loss_scope: &LossScope,
) -> Result<LossOutput, CoreError> {
    Python::attach(|py| -> Result<LossOutput, CoreError> {
        // py.detach drops the GIL while the underlying CUDA call runs;
        // re-acquires before we touch Python objects.
        let result_dict = py.detach(|| {
            Python::attach(|py| {
                let m = module.bind(py);
                let kwargs = build_forward_kwargs(py, batch, loss_scope)?;
                m.call_method("forward_with_loss", (), Some(&kwargs))
                    .map(|r| r.unbind())
            })
        })?;

        // Extract loss + grad handle (opaque PyObject)
        let bound = result_dict.bind(py);
        let loss: f32 = bound.get_item("loss")?.extract()?;
        let grad_handle = GradHandle::new(bound.get_item("grad_handle")?.unbind());
        let n_tokens: u32 = bound.get_item("n_tokens")?.extract()?;
        Ok(LossOutput { loss, grad_handle, n_tokens })
    })
}
```

**`forward_with_loss` is sync because the actual CUDA work doesn't yield**. Releasing the GIL via `py.detach` is the correct pattern for the heavy kernel block; we do NOT use `run_until_complete` here (no asyncio coroutine to await).

`optimizer_step` follows the same pattern — sync Python call wrapped in `py.detach()`.

**GIL release during `optimizer_step`:** mandatory — the AdamW step is ~5-30 ms of CUDA kernels, and other Tokio tasks need the GIL released so they can make progress (especially if a parallel `EventEmitter` writes to stdout from a different task). Always `py.detach(|| { ... })` around the inner `call_method`.

**`GradHandle` shape:** Opaque newtype around `pyo3::Py<pyo3::PyAny>` that holds a reference to a Python-side dict `{model: <model_ref>, accelerator: <accelerator_ref>}`. The Rust side never inspects it — it's a marker passed back into `optimizer_step` so the Python side knows which model to step. Defining it this way keeps the GIL transactional: every method that takes/returns a `GradHandle` is `&self` on the Python thread; the actual gradients never leave Python memory.

```rust
// crates/rollout-core/src/traits/backend.rs (Wave 0 addition)
/// Opaque handle to gradients computed by `forward_with_loss`.
///
/// Python-side ref kept alive by the backend; not inspected from Rust.
/// Lifetime: must be passed to `optimizer_step` in the same `run` epoch;
/// dropped automatically when its source `LossOutput` is dropped.
#[derive(Debug)]
pub struct GradHandle {
    inner: pyo3::Py<pyo3::PyAny>,  // only valid under `--features train`
}
```

For builds without the `train` feature (e.g., MockBackend builds), GradHandle is a marker:

```rust
#[cfg(not(feature = "train"))]
#[derive(Debug, Default)]
pub struct GradHandle {
    pub step: u64,  // MockBackend tracks step number; real backend ignores
}
```

### Pattern 3: accelerate.save_state determinism preamble

**What:** The Python-side training module sets the determinism preamble exactly once at thread startup, BEFORE `import torch`. Order matters: torch reads `CUBLAS_WORKSPACE_CONFIG` at import time.

```python
# python/rollout/backends/vllm/train.py
import os
# MUST be set BEFORE import torch (see torch.use_deterministic_algorithms docs)
os.environ.setdefault("CUBLAS_WORKSPACE_CONFIG", ":4096:8")
os.environ.setdefault("PYTHONHASHSEED", "0")

import torch
import random
import numpy as np
from accelerate import Accelerator
from accelerate.utils import set_seed
from transformers import AutoModelForCausalLM, AutoTokenizer

def init_train(model_uri: str, seed: int = 42) -> dict:
    """Construct an Accelerator-wrapped model. Called once on set_train_mode(True)."""
    # 1. RNG seeds (set_seed seeds python, numpy, torch, cuda)
    set_seed(seed)

    # 2. Deterministic torch flags
    torch.use_deterministic_algorithms(True, warn_only=False)
    torch.backends.cudnn.deterministic = True
    torch.backends.cudnn.benchmark = False  # benchmark=True is non-deterministic
    torch.set_float32_matmul_precision("highest")

    # 3. CUDA probe (Phase 3 Pitfall 9 — explicit, not device="auto")
    has_cuda = torch.cuda.is_available()
    device_count = torch.cuda.device_count() if has_cuda else 0

    # 4. FSDP / DDP / single-device heuristic
    #    accelerate auto-selects; we just gate which kwargs we hand it.
    accelerator_kwargs = {}
    if device_count >= 2:
        # Multi-GPU: FSDP via accelerate
        from accelerate.utils import FullyShardedDataParallelPlugin
        accelerator_kwargs["fsdp_plugin"] = FullyShardedDataParallelPlugin(
            # accelerate 1.0+ defaults; tune in Phase 9
        )
    # device_count == 1 → DDP (single process)
    # device_count == 0 → plain accelerate, CPU
    accelerator = Accelerator(**accelerator_kwargs)

    # 5. Load model + tokenizer
    tokenizer = AutoTokenizer.from_pretrained(model_uri)
    model = AutoModelForCausalLM.from_pretrained(model_uri)

    # 6. Construct optimizer (defaults; will be reconfigured by set_optimizer call)
    optimizer = torch.optim.AdamW(model.parameters(), lr=1e-5)
    model, optimizer = accelerator.prepare(model, optimizer)

    return {
        "accelerator": accelerator,
        "model": model,
        "optimizer": optimizer,
        "tokenizer": tokenizer,
        "has_cuda": has_cuda,
        "device_count": device_count,
        "step": 0,
    }
```

**What `accelerate.Accelerator.save_state(dir)` actually writes (verified against accelerate 1.x docs):**

| State | Captured? | Notes |
|-------|-----------|-------|
| Model weights (sharded under FSDP) | YES | `pytorch_model.bin` or `model.safetensors` shards |
| Optimizer state | YES | `optimizer.bin` |
| LR scheduler state | YES | `scheduler.bin` (only if registered via `accelerator.register_for_checkpointing`) |
| GradScaler (mixed precision) | YES | `scaler.pt` |
| RNG state (python.random + numpy + torch.cpu + torch.cuda) | YES | `random_states_*.pkl` per process |
| Custom registered objects | YES IF registered | `accelerator.register_for_checkpointing(my_obj)` |
| DataLoader sampler position | YES ONLY IF `use_stateful_dataloader=True` (requires torchdata>=0.8) | Otherwise NOT captured; we restore via step counter |
| Algorithm-internal state (curriculum cursor, etc.) | NO | We pack into `Snapshot.meta: serde_json::Value` per D-DETERM-05 |
| Step counter | NO | Caller's responsibility (we track in `Snapshot.meta`) |

**Recommendation:** In Phase 4, use `DataLoaderConfiguration(use_stateful_dataloader=True)` when torchdata is installed; otherwise fall back to step-based restoration (replay first N batches deterministically, skip them). Document the divergence.

**Tar reproducibility:** verified against GNU tar + bsdtar (macOS). Use `tar::Builder` (Rust crate) — has a `mode(HeaderMode::Deterministic)` that sets owner/group/mtime to zero. Reproducible bytes across machines IF:
- Source files are byte-identical (accelerate output IS byte-identical when seeded + deterministic flags set + same device topology).
- Tar entry order is `sort=name` (deterministic).
- No compression.
- All metadata zeroed (mtime, uid, gid, mode bits to 0o644 file / 0o755 dir).

```rust
// crates/rollout-snapshots/src/tar_build.rs
pub fn build_deterministic_tar(src_dir: &Path) -> Result<Vec<u8>, CoreError> {
    let mut entries: Vec<PathBuf> = walkdir::WalkDir::new(src_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .map(|e| e.into_path())
        .collect();
    entries.sort();  // sort=name

    let mut buf = Vec::new();
    let mut tar = tar::Builder::new(&mut buf);
    tar.mode(tar::HeaderMode::Deterministic);  // mtime=0, uid=gid=0
    for path in entries {
        if path.is_file() {
            let rel = path.strip_prefix(src_dir).unwrap();
            tar.append_path_with_name(&path, rel)?;
        }
    }
    tar.finish()?;
    drop(tar);
    Ok(buf)
}
```

### Anti-Patterns to Avoid

- **Holding both vLLM engine + Accelerator in CUDA memory.** Always destroy one before constructing the other (`del engine; gc.collect(); torch.cuda.empty_cache()`). Otherwise OOM on any non-trivial model.
- **Skipping `torch.backends.cudnn.benchmark = False`.** With `benchmark=True`, cuDNN picks the fastest kernel per shape — non-deterministic. Must be False for bit-identical resume.
- **Tar with compression.** zstd output drifts across versions; gzip embeds mtime in header. Use no compression; rely on blake3 hash for integrity.
- **`from_pretrained` from disk every restore.** accelerate.load_state restores into the already-constructed model; we don't re-download or re-instantiate from the HF Hub.
- **Mixing `serde_json::Value` into the tar.** `Snapshot.meta` is a separate Storage row, NOT inside the tar. Putting it in the tar would invalidate the hash on each metadata update.
- **PgListener payload > 8000 bytes.** Postgres NOTIFY payload limit is 8000 bytes. We send `<postcard-encoded-key-suffix>` (typically <200 bytes); if a longer-than-suffix payload ever materializes, the storage layer must split or fall back to polling the row.
- **Channel name > 63 bytes.** Postgres LISTEN/NOTIFY channel identifier limit is 63 bytes. `rollout_watch_<namespace>` with `namespace = "infer"` is fine; if a future namespace exceeds it, hash the namespace.
- **`SQLX_OFFLINE=true` set in `.env`.** Breaks `cargo sqlx prepare --workspace` (known sqlx issue #3836). Set via `SQLX_OFFLINE=true cargo build` invocation OR `[env]` section of `.cargo/config.toml`, NOT `.env`.
- **Calling `accelerator.prepare()` more than once on the same model.** Wraps the model in a DDP/FSDP wrapper each time; on restore, unwrap before re-prepare.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Distributed training (FSDP / DDP) | Custom NCCL all-reduce | `accelerate.Accelerator` | accelerate handles process group bootstrap, gradient sync, mixed-precision scaling. NCCL hand-rolling is months of work. |
| Postgres LISTEN/NOTIFY | tokio-postgres manual subscription | `sqlx::postgres::PgListener` | PgListener auto-reconnects on connection loss + re-subscribes to channels; the auto-reconnect logic alone is 100+ LoC of edge cases. |
| Postgres connection pool | Hand-rolled | `sqlx::PgPool` | Built in. `PgPoolOptions::new().max_connections(16).connect(url)`. |
| SQL migrations | Custom version table + apply loop | `sqlx::migrate!("../../database/migrations")` | Compile-time embedded; runtime checksum verification; idempotent. |
| Tar archive build | hand-rolled USTAR writer | `tar` crate with `HeaderMode::Deterministic` | Deterministic mode handles mtime/uid/gid zeroing. |
| HF chat template application | Parse roles + concatenate | `tokenizer.apply_chat_template(messages, tokenize=True, return_tensors="pt")` | HF maintains per-model templates; reinventing is wrong (each model formats differently). |
| Loss masking on assistant tokens | Manual span tracking | `apply_chat_template(..., return_assistant_tokens_mask=True)` IF template supports `{% generation %}` markers | **Critical caveat:** Qwen2.5 chat template does NOT have `{% generation %}` markers (HF issue #34172). Phase 4 must either override the template OR fall back to token-id-level masking. |
| Bradley-Terry pairwise loss | From scratch | `torch.nn.functional.logsigmoid(r_chosen - r_rejected).neg().mean()` | One line; canonical form. |
| Snapshot diff / metadata index | Custom schema | spec 04 §5.1 `Snapshot` shape | Already specified. |
| ULID ↔ Postgres UUID round-trip | hand-rolled | `ulid::Ulid::to_bytes() -> [u8; 16]; uuid::Uuid::from_bytes` | One-line conversion; ULID is UUID-shaped. |
| Determinism env-var orchestration | Per-call setting | One-time preamble before `import torch` | Setting CUBLAS_WORKSPACE_CONFIG after torch import is silently ignored. |

**Key insight:** the training path is 95% gluing well-established tools together (accelerate, transformers, sqlx, tar, blake3). The remaining 5% — Qwen2.5 chat-template loss mask, the PyO3 mode-switch, the determinism preamble ordering — is where bugs hide.

---

## Runtime State Inventory

> Phase 4 is a greenfield phase (new crates, new feature gates). No rename / refactor of existing identifiers is required. **Stored data:** None — verified by inspection of CONTEXT.md (no rename mentioned). **Live service config:** None — verified by checking n8n / Datadog absence in repo. **OS-registered state:** None — no Task Scheduler / pm2 / systemd integrations in scope. **Secrets/env vars:** Phase 4 ADDS new env-var conventions (`ROLLOUT_TRANSFORMERS_AVAILABLE`, `CUBLAS_WORKSPACE_CONFIG`, `PYTHONHASHSEED`, `DATABASE_URL`, `SQLX_OFFLINE`, `ROLLOUT_SECRET_HF_TOKEN`) but does not RENAME any existing ones. **Build artifacts:** None to migrate.

---

## Common Pitfalls

### Pitfall 1: Qwen2.5 chat template does NOT support `assistant_tokens_mask` natively

**What goes wrong:** Calling `tokenizer.apply_chat_template(messages, return_assistant_tokens_mask=True)` against Qwen2.5-0.5B-Instruct silently returns an all-zeros mask (or no mask) because the chat template lacks `{% generation %}` / `{% endgeneration %}` Jinja markers. The training loop computes loss on every token (Full scope) instead of AssistantOnly — wrong behavior, no error message.

**Why it happens:** HF added `return_assistant_tokens_mask` to `apply_chat_template` in transformers ~4.37, but it requires the template to wrap assistant content in `{% generation %}{{ content }}{% endgeneration %}`. Qwen team did not include these markers in Qwen2.5 (HF issue #34172). Qwen3 added them.

**How to avoid:** Two viable paths:

1. **Override the chat template at load time** (recommended for Phase 4):
   ```python
   GENERATION_MARKED_QWEN25_TEMPLATE = """{%- for message in messages %}{%- if message.role == 'system' %}<|im_start|>system\n{{ message.content }}<|im_end|>\n{%- elif message.role == 'user' %}<|im_start|>user\n{{ message.content }}<|im_end|>\n{%- elif message.role == 'assistant' %}<|im_start|>assistant\n{% generation %}{{ message.content }}<|im_end|>{% endgeneration %}\n{%- endif %}{%- endfor %}"""
   tokenizer.chat_template = GENERATION_MARKED_QWEN25_TEMPLATE
   ```
2. **Token-id-level mask** (fallback): tokenize twice — once with assistant content, once without — and diff the token positions to derive the mask. Slower and more brittle.

**Warning signs:** loss curves are unusually slow to drop (model is being penalized for prompt-token reproduction); `samples_skipped` counter is 0 when it should reflect masked-out prompt tokens.

**Verification test (Phase 4 ships):** `tests/qwen25_assistant_mask.rs` (Python integration): apply chat template to `[{role:"user","content":"Hi"}, {role:"assistant","content":"Hello"}]`; assert mask is `1` only on the "Hello" tokens (+ EOS), `0` elsewhere.

### Pitfall 2: `CUBLAS_WORKSPACE_CONFIG` must be set BEFORE `import torch`

**What goes wrong:** Setting `os.environ["CUBLAS_WORKSPACE_CONFIG"] = ":4096:8"` in the Python module AFTER `import torch` has no effect — torch reads cuBLAS workspace config once at import. Result: `torch.use_deterministic_algorithms(True)` raises `RuntimeError: Deterministic behavior was enabled with either torch.use_deterministic_algorithms(True) or at::Context::setDeterministicAlgorithms(true), but this operation is not deterministic because it uses CuBLAS and you have CUDA >= 10.2`.

**Why it happens:** Same root cause as Phase 3 Pitfall 10 (HF_TOKEN must be set before `import vllm`). The Python-side `train.py` module's first lines are the env-setters, before any `import torch`. The Rust side enforces this by writing env-vars on the Python thread before calling `py.import("rollout.backends.vllm.train")`.

**How to avoid:** mirror the Phase-3 pattern. In Rust's `worker_main_train`:

```rust
Python::attach(|py| {
    let os = py.import("os")?;
    let environ: Bound<'_, PyDict> = os.getattr("environ")?.cast_into()?;
    environ.set_item("CUBLAS_WORKSPACE_CONFIG", ":4096:8")?;
    environ.set_item("PYTHONHASHSEED", "0")?;
    if let Some(token) = &secret_token {
        environ.set_item("HF_TOKEN", token)?;
    }
    // NOW we can import — torch will see the env on first import.
    let module = py.import("rollout.backends.vllm.train")?;
    Ok(module.unbind())
})
```

**Warning signs:** intermittent `RuntimeError: ...not deterministic because it uses CuBLAS...` on CUDA boxes; bit-identical-resume test passes on CPU but fails on CUDA.

### Pitfall 3: `Accelerator.save_state` does NOT capture dataloader cursor unless `use_stateful_dataloader=True`

**What goes wrong:** After a snapshot at step 5, restore loads model + optimizer + RNG correctly — but the dataloader starts from sample 0 again. Step 6 sees the wrong batch; weights diverge from the non-interrupted run.

**Why it happens:** PyTorch's default `DataLoader` has no `state_dict()`. accelerate's stateful-dataloader path is opt-in and requires `torchdata>=0.8`.

**How to avoid:** Two paths:

1. **Recommended:** `DataLoaderConfiguration(use_stateful_dataloader=True)` when constructing the Accelerator, and require `torchdata>=0.8` in `pyproject.toml`. accelerate will then include `dataloader.bin` in `save_state` output.
2. **Fallback:** track step counter in `Snapshot.meta` (the `serde_json::Value`); on restore, replay first N batches deterministically before starting the loop. Works because the RNG is restored, so the sampler reproduces the same shuffle order.

Phase 4 default: use path 1 IF `torchdata` is importable, else path 2 with a documented warning event.

**Warning signs:** `snapshot_resume.rs` test failure on the HF-transformers-backed path (the MockBackend path doesn't exercise dataloader state — that's an `#[ignore]`'d test).

### Pitfall 4: sqlx `cargo sqlx prepare --workspace` breaks with `.env`-sourced `SQLX_OFFLINE`

**What goes wrong:** Setting `SQLX_OFFLINE=true` in a `.env` file (so that `cargo build` doesn't try to connect to a DB at compile time) also affects `cargo sqlx prepare --workspace`, which then refuses to query the DB to regenerate `.sqlx/` cache.

**Why it happens:** sqlx-cli reads `.env` first thing. Known issue: launchbadge/sqlx#3836.

**How to avoid:** do NOT put `SQLX_OFFLINE=true` in `.env`. Instead, add it to `.cargo/config.toml`:

```toml
# .cargo/config.toml
[env]
SQLX_OFFLINE = "true"
```

OR set it inline for the build only: `SQLX_OFFLINE=true cargo build`. When running `cargo sqlx prepare`, override with `SQLX_OFFLINE=false cargo sqlx prepare --workspace -- --features postgres`.

**Warning signs:** CI fails with `error: set DATABASE_URL to use query macros online, or run cargo sqlx prepare to update the query cache` even though the cache exists.

### Pitfall 5: `pg_notify` payload truncation at 8000 bytes

**What goes wrong:** A storage write triggers `pg_notify('rollout_watch_kv', <payload>)`. If the payload exceeds 8000 bytes, Postgres truncates without warning. The listener receives a malformed notification, fails to deserialize, and silently drops the event.

**Why it happens:** Postgres caps NOTIFY payloads at 8000 bytes (compiled-in `NAMEDATALEN` constant).

**How to avoid:** the payload is `<postcard-encoded-key-suffix>` — typically <200 bytes. Validate at the storage-layer boundary: if `payload.len() > 8000`, fall back to sending just the namespace + a "see kv table" marker; the listener then issues a follow-up SELECT to fetch the changed key. Phase 4 ships the guard rail; the fallback path is exercised by an integration test that writes a synthetic long-suffix row.

**Warning signs:** `watch()` consumers miss notifications under high-cardinality key prefixes.

### Pitfall 6: testcontainers Postgres image boot time + readiness probe

**What goes wrong:** Test starts container; immediately tries `PgPool::connect(url)`; gets connection refused because Postgres is still booting (typically 5-15 s on a CI runner). Flaky tests.

**Why it happens:** testcontainers-rs `start()` returns when Docker reports "container running", not when the application inside is ready to accept connections.

**How to avoid:** wrap connection attempts in a retry loop with backoff (5-attempt × 2-second backoff is sufficient). testcontainers-modules has `wait_for(WaitFor::message_in_stdout("database system is ready to accept connections"))` — use it on the Postgres module to gate `start()` correctly. Also: in CI, set `min_connections=0, max_connections=4, acquire_timeout=Duration::from_secs(30)` on PgPool so the first acquire waits long enough.

**Warning signs:** `postgres-integration` job fails ~10-20% of runs with `connection refused` or `relation "kv" does not exist`.

### Pitfall 7: `accelerate.Accelerator()` constructed twice in one process

**What goes wrong:** A snapshot restore path that re-imports `train.py` (or re-calls `init_train`) constructs a second `Accelerator()` in the same process. accelerate's global state (process group, distributed env) is in a partially-initialized state; subsequent `prepare()` calls hang or raise.

**Why it happens:** accelerate maintains a global `AcceleratorState` singleton. Constructing twice is supported only if the first one is explicitly torn down via `AcceleratorState._reset_state()` (private API) OR by exiting the process.

**How to avoid:** the Python module caches the Accelerator instance globally; `set_train_mode(True)` is idempotent (returns the existing Accelerator if already constructed). Restore loads weights INTO the existing Accelerator via `accelerator.load_state(dir)`. If a fundamental config change is needed (e.g., different fsdp_plugin), error out with `Fatal(PluginContract, msg: "Accelerator reconfiguration requires process restart")`.

**Warning signs:** the second training step hangs indefinitely; CUDA shows zero utilization.

### Pitfall 8: `torch.backends.cudnn.benchmark=True` is the silent determinism killer

**What goes wrong:** All deterministic flags set; resume test still fails on CUDA. Investigation reveals cuDNN benchmark mode is on (it's PyTorch's default in some versions), which picks the fastest kernel for each input shape at runtime — non-deterministically when multiple equivalent kernels exist.

**Why it happens:** `cudnn.benchmark` defaults to False in modern torch, but legacy code or some HF model loaders flip it on. `torch.use_deterministic_algorithms(True)` does NOT disable it.

**How to avoid:** explicit `torch.backends.cudnn.benchmark = False` in the determinism preamble, AFTER `torch.use_deterministic_algorithms(True)`. Combine with `cudnn.deterministic = True` for double-coverage.

**Warning signs:** CUDA resume test produces weights that differ in the last few decimal places; CPU resume test passes.

### Pitfall 9: tar `HeaderMode::Deterministic` doesn't zero file mode bits

**What goes wrong:** Tar files produced on macOS and Linux differ in the `mode` field of headers because the Rust `tar` crate's `HeaderMode::Deterministic` zeros uid/gid/mtime but NOT mode (file permissions). Same-bytes input, different blake3 hash across platforms.

**Why it happens:** `HeaderMode::Deterministic` is conservative; preserves mode so executables stay executable on extract.

**How to avoid:** after `set_metadata_deterministic()`, also explicitly call `header.set_mode(0o644)` (or 0o755 for directories) before appending each entry. The `rollout-snapshots::tar_build::build_deterministic_tar` function handles this:

```rust
for path in entries {
    let mut header = tar::Header::new_gnu();
    let meta = std::fs::metadata(&path)?;
    header.set_size(meta.len());
    header.set_mode(if meta.is_dir() { 0o755 } else { 0o644 });
    header.set_mtime(0);
    header.set_uid(0);
    header.set_gid(0);
    header.set_cksum();
    let rel = path.strip_prefix(src_dir).unwrap();
    let mut file = std::fs::File::open(&path)?;
    tar.append_data(&mut header, rel, &mut file)?;
}
```

**Warning signs:** snapshot_resume.rs passes locally on macOS but fails the CI Linux job (or vice versa) with hash mismatch.

### Pitfall 10: `register_for_checkpointing` is required for the LR scheduler

**What goes wrong:** `accelerator.save_state(dir)` skips the LR scheduler unless explicitly registered. Restore loses the warmup-cosine cursor; LR jumps back to start.

**Why it happens:** Per accelerate issue #976: `save_state` only saves what's been "prepared" through Accelerator or "registered" via `register_for_checkpointing`. Custom schedulers fall outside the automatic capture path.

**How to avoid:** after constructing the scheduler, register it:

```python
scheduler = torch.optim.lr_scheduler.CosineAnnealingLR(optimizer, ...)
accelerator.register_for_checkpointing(scheduler)
```

OR (preferred) prepare it explicitly:

```python
scheduler = accelerator.prepare(scheduler)
```

Phase 4 always uses `accelerator.prepare()` on the scheduler to keep coverage uniform.

**Warning signs:** restored LR is wildly different from the pre-snapshot LR; loss spikes on the first post-restore step.

---

## Code Examples

### Trait surface (Wave 0 — `rollout-core/src/traits/backend.rs`)

```rust
// SIBLING trait of InferenceBackend. Same `Send + Sync`.
#[async_trait]
pub trait TrainableBackend: InferenceBackend {
    /// Switch this backend between inference and training modes.
    /// Idempotent: calling with the same value is a no-op.
    async fn set_train_mode(&mut self, enabled: bool) -> Result<(), CoreError>;

    /// Compute forward + loss for a training batch.
    /// Returns the loss value + an opaque GradHandle for `optimizer_step`.
    async fn forward_with_loss(
        &self,
        batch: &TrainBatch,
        loss_scope: &LossScope,
    ) -> Result<LossOutput, CoreError>;

    /// Apply gradients accumulated in `grads` with `opt` settings.
    async fn optimizer_step(
        &mut self,
        grads: GradHandle,
        opt: &OptimizerSettings,
    ) -> Result<(), CoreError>;

    /// Persist current weights as a content-addressed blob; returns the ID.
    /// Caller is responsible for storing the ID in the snapshot metadata.
    async fn save_weights(&self) -> Result<ContentId, CoreError>;

    /// Restore weights from a previously-saved blob.
    /// May fail if the blob is incompatible with the loaded model architecture.
    async fn load_weights(&mut self, weights_id: &ContentId) -> Result<(), CoreError>;
}
```

### Trait surface (Wave 0 — `rollout-core/src/traits/algorithm.rs`)

Full rewrite per spec 02 §2:

```rust
#[async_trait]
pub trait PolicyAlgorithm: Send + Sync {
    fn id() -> AlgorithmId where Self: Sized;

    type Settings: DeserializeOwned + Serialize + JsonSchema + Send + Sync + 'static;

    fn from_settings(settings: Self::Settings, deps: AlgoDependencies)
        -> Result<Self, CoreError>
    where Self: Sized;

    fn required_roles(&self) -> Vec<WorkerRole>;
    fn validate_plan(&self, plan: &Plan) -> Result<(), Vec<ConfigViolation>>;

    async fn run(&mut self, ctx: &AlgoContext<'_>) -> Result<RunOutcome, CoreError>;

    async fn snapshot_save(&self) -> Result<Snapshot, CoreError>;
    async fn snapshot_restore(&mut self, snapshot: Snapshot) -> Result<(), CoreError>;
}

pub struct AlgoDependencies {
    pub backend:   Arc<dyn TrainableBackend>,
    pub storage:   Arc<dyn Storage>,
    pub object:    Arc<dyn ObjectStore>,
    pub snapshots: Arc<dyn Snapshotter>,
    pub events:    Arc<dyn EventEmitter>,
}

pub struct AlgoContext<'a> {
    pub plan:    &'a Plan,
    pub worker:  WorkerId,
    pub cancel:  tokio_util::sync::CancellationToken,
    pub clock:   &'a dyn Clock,
}
```

### Snapshotter trait (Wave 0 — `rollout-core/src/traits/snapshot.rs`)

Replaces the placeholder in `traits/storage.rs`. Per spec 04 §5.2:

```rust
#[async_trait]
pub trait Snapshotter: Send + Sync {
    async fn save(&self, request: SnapshotRequest) -> Result<Snapshot, CoreError>;
    async fn restore(&self, id: &SnapshotId, target: RestoreTarget) -> Result<(), CoreError>;
    async fn list(&self, filter: SnapshotFilter) -> Result<Vec<Snapshot>, CoreError>;
    async fn prune(&self, policy: PrunePolicy) -> Result<u64, CoreError>;
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Snapshot {
    pub id:         SnapshotId,            // ContentId-wrapped (blake3 of canonical metadata)
    pub kind:       SnapshotKind,
    pub run_id:     RunId,
    pub created_at: DateTime<Utc>,
    pub label:      Option<SmolStr>,
    pub parts:      Vec<SnapshotPart>,
    pub algorithm_id: AlgorithmId,
    pub meta:       serde_json::Value,     // algorithm-internal state (D-DETERM-05)
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SnapshotPart {
    pub role:    SmolStr,                  // e.g., "tar", "weights"
    pub content: ContentId,
    pub size:    u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotKind {
    TrainState,
    Buffer,
    Process,
    EpisodicMemory,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum RestoreTarget {
    SameRun,
    Fork { new_run_id: RunId },
    Worker { worker_id: WorkerId },
}
```

### MockBackend `TrainableBackend` impl + snapshot_resume.rs (load-bearing test for TRAIN-03)

```rust
// crates/rollout-runtime-batch/src/mock_backend.rs (extend, behind test-mock-backend)
use ndarray::Array1;
use std::sync::Mutex;

pub struct MockBackend {
    model_id: ContentId,
    delay: Duration,
    // training state — only relevant under `test-mock-backend` + `train` features
    weights: Mutex<Array1<f32>>,
    step: Mutex<u64>,
    seed: u64,
}

#[async_trait]
impl TrainableBackend for MockBackend {
    async fn set_train_mode(&mut self, _enabled: bool) -> Result<(), CoreError> { Ok(()) }

    async fn forward_with_loss(&self, batch: &TrainBatch, _: &LossScope)
        -> Result<LossOutput, CoreError>
    {
        let step = *self.step.lock().unwrap();
        Ok(LossOutput {
            loss: 0.5,
            grad_handle: GradHandle { step: step + 1 },
            n_tokens: batch.n_tokens(),
        })
    }

    async fn optimizer_step(&mut self, grads: GradHandle, opt: &OptimizerSettings)
        -> Result<(), CoreError>
    {
        // Plain deterministic SGD against fake weights:
        //   weights -= lr * (seed + step)  (predictable per-step delta)
        let mut weights = self.weights.lock().unwrap();
        let mut step = self.step.lock().unwrap();
        let delta = (self.seed + grads.step) as f32 * opt.lr as f32;
        for w in weights.iter_mut() { *w -= delta; }
        *step = grads.step;
        Ok(())
    }

    async fn save_weights(&self) -> Result<ContentId, CoreError> {
        let weights = self.weights.lock().unwrap();
        let bytes = postcard::to_stdvec(&*weights)?;
        Ok(ContentId::of(&bytes))
    }

    async fn load_weights(&mut self, weights_id: &ContentId) -> Result<(), CoreError> {
        // For tests: storage layer holds the actual bytes; we just verify identity.
        Ok(())
    }
}
```

```rust
// crates/rollout-algo-sft/tests/snapshot_resume.rs
//! TRAIN-03 LOAD-BEARING PROOF. Mirrors Phase 3's restart_no_duplicates pattern.
//! Runs on every CI build with no GPU / no HF transformers.

use rollout_algo_sft::{SftAlgo, SftSettings};
use rollout_runtime_batch::MockBackend;
use std::sync::Arc;

#[tokio::test]
async fn bit_identical_resume_at_step_5() {
    // Run A: 10 SGD steps uninterrupted with seed=42
    let backend_a = Arc::new(MockBackend::new_train(42));
    let mut algo_a = SftAlgo::from_settings(test_settings(), test_deps(backend_a.clone())).unwrap();
    for _ in 0..10 { algo_a.step_once().await.unwrap(); }
    let weights_a = backend_a.weights_snapshot();

    // Run B: 5 steps, snapshot, restart, 5 more steps
    let backend_b1 = Arc::new(MockBackend::new_train(42));
    let mut algo_b1 = SftAlgo::from_settings(test_settings(), test_deps(backend_b1.clone())).unwrap();
    for _ in 0..5 { algo_b1.step_once().await.unwrap(); }
    let snapshot = algo_b1.snapshot_save().await.unwrap();
    drop(algo_b1); drop(backend_b1);

    let backend_b2 = Arc::new(MockBackend::new_train(42));
    let mut algo_b2 = SftAlgo::from_settings(test_settings(), test_deps(backend_b2.clone())).unwrap();
    algo_b2.snapshot_restore(snapshot).await.unwrap();
    for _ in 0..5 { algo_b2.step_once().await.unwrap(); }
    let weights_b = backend_b2.weights_snapshot();

    // Byte-compare — this is the TRAIN-03 exit criterion.
    assert_eq!(weights_a, weights_b, "bit-identical resume failed at step 5");
}
```

The RM analog (`crates/rollout-algo-rm/tests/snapshot_resume.rs`) follows the same pattern; same MockBackend; same byte-compare assertion. The only difference is the algorithm wrapper (`RmAlgo` instead of `SftAlgo`) and the input batch shape (preference pairs instead of single sequences).

### tar + blake3 round-trip (rollout-snapshots)

```rust
// crates/rollout-snapshots/src/kind/train_state.rs
pub async fn save_train_state(
    &self,
    request: SnapshotRequest,
    accelerate_dir: &Path,
) -> Result<Snapshot, CoreError> {
    // 1. Build deterministic tar (deterministic mode + explicit mode bits).
    let tar_bytes = tokio::task::spawn_blocking({
        let src = accelerate_dir.to_path_buf();
        move || build_deterministic_tar(&src)
    }).await.map_err(internal)??;

    // 2. Hash + write to object store.
    let content_id = ContentId::of(&tar_bytes);
    let size = tar_bytes.len() as u64;
    self.object.put_bytes(&content_id, &tar_bytes).await?;

    // 3. Build Snapshot metadata.
    let snapshot = Snapshot {
        id: SnapshotId::from(content_id),
        kind: SnapshotKind::TrainState,
        run_id: request.run_id,
        created_at: chrono::Utc::now(),
        label: request.label,
        parts: vec![SnapshotPart { role: "tar".into(), content: content_id, size }],
        algorithm_id: request.algorithm_id,
        meta: request.meta,
    };

    // 4. Persist metadata row to Storage under namespace="snapshots".
    let mut txn = self.storage.begin().await?;
    let key = snapshot_key(&snapshot.run_id, &snapshot.id);
    let value = postcard::to_stdvec(&snapshot).map_err(internal)?;
    txn.put_bytes(key, value).await?;
    txn.commit().await?;

    Ok(snapshot)
}
```

### Postgres `PostgresStorage` skeleton (TRAIN-04)

```rust
// crates/rollout-storage/src/postgres/mod.rs
#[cfg(feature = "postgres")]
pub struct PostgresStorage {
    pool: PgPool,
    watch_pool: PgPool,  // separate pool so listeners don't starve writers
}

impl PostgresStorage {
    pub async fn new(url: &str, pool_size: u32) -> Result<Self, CoreError> {
        let pool = PgPoolOptions::new()
            .min_connections(0)
            .max_connections(pool_size)
            .acquire_timeout(Duration::from_secs(30))
            .connect(url)
            .await
            .map_err(transient)?;
        sqlx::migrate!("../../database/migrations")
            .run(&pool)
            .await
            .map_err(|e| fatal_config(&format!("migration failed: {e}")))?;
        let watch_pool = PgPoolOptions::new()
            .max_connections(4)
            .connect(url)
            .await
            .map_err(transient)?;
        Ok(Self { pool, watch_pool })
    }
}

#[async_trait]
impl Storage for PostgresStorage {
    async fn get_bytes(&self, key: &StorageKey) -> Result<Option<Vec<u8>>, CoreError> {
        let path_arr: Vec<&str> = key.path.iter().map(SmolStr::as_str).collect();
        let row = sqlx::query!(
            "SELECT value FROM kv WHERE namespace = $1 AND run_id = $2 AND path = $3",
            key.namespace.as_str(),
            key.run_id.map(ulid_to_uuid),
            &path_arr as &[&str],
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(transient)?;
        Ok(row.map(|r| r.value))
    }
    // get_many_bytes / scan_bytes / watch / ping … similar.
}
```

`watch()` returns the Phase-2 `broadcast::Receiver` for the embedded backend; for Postgres, we add a parallel `watch_stream(prefix) -> BoxStream<StorageEvent>` method on the trait that the embedded backend implements by bridging its broadcast channel into a stream. Both backends thus expose both surfaces; consumers that need cross-process notification call `watch_stream()`.

### CAS in Postgres

```rust
async fn cas_bytes(&mut self, key: StorageKey, expected: Option<Vec<u8>>, new: Option<Vec<u8>>)
    -> Result<bool, CoreError>
{
    let path_arr: Vec<&str> = key.path.iter().map(SmolStr::as_str).collect();
    let run_uuid = key.run_id.map(ulid_to_uuid);
    match (expected, new) {
        (Some(exp), Some(new_val)) => {
            // Update if existing value matches.
            let res = sqlx::query!(
                "UPDATE kv SET value = $4, version = version + 1, updated_at = now() \
                 WHERE namespace = $1 AND run_id = $2 AND path = $3 AND value = $5",
                key.namespace.as_str(), run_uuid, &path_arr as &[&str], new_val, exp,
            ).execute(&mut **self).await.map_err(transient)?;
            Ok(res.rows_affected() == 1)
        }
        (None, Some(new_val)) => {
            // Insert-if-absent.
            let res = sqlx::query!(
                "INSERT INTO kv (namespace, run_id, path, value, version, updated_at) \
                 VALUES ($1, $2, $3, $4, 0, now()) \
                 ON CONFLICT (namespace, run_id, path) DO NOTHING",
                key.namespace.as_str(), run_uuid, &path_arr as &[&str], new_val,
            ).execute(&mut **self).await.map_err(transient)?;
            Ok(res.rows_affected() == 1)
        }
        (Some(exp), None) => {
            // Delete if existing matches.
            let res = sqlx::query!(
                "DELETE FROM kv WHERE namespace = $1 AND run_id = $2 AND path = $3 AND value = $4",
                key.namespace.as_str(), run_uuid, &path_arr as &[&str], exp,
            ).execute(&mut **self).await.map_err(transient)?;
            Ok(res.rows_affected() == 1)
        }
        (None, None) => Ok(true),  // no-op
    }
}
```

### Migrations (`database/migrations/0001_init.sql`)

```sql
-- Phase 4 (TRAIN-04): Postgres Storage backend, kv table.
-- Mirrors EmbeddedStorage namespace semantics so the Storage trait works identically.

CREATE TABLE kv (
    namespace   TEXT NOT NULL,
    run_id      UUID,                       -- ULID-as-UUID; NULL for global rows
    path        TEXT[] NOT NULL,
    value       BYTEA NOT NULL,
    version     BIGINT NOT NULL DEFAULT 0,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (namespace, run_id, path)
);

CREATE INDEX kv_namespace_run_idx ON kv (namespace, run_id);
CREATE INDEX kv_updated_at_idx ON kv (updated_at);

-- LISTEN/NOTIFY trigger: emit a notify on every kv mutation. Channel name
-- is `rollout_watch_<namespace>` (max 63 chars; "rollout_watch_" = 14 chars,
-- leaves 49 for namespace — comfortable).
CREATE OR REPLACE FUNCTION rollout_kv_notify() RETURNS trigger AS $$
DECLARE
    channel TEXT;
    payload TEXT;
BEGIN
    channel := 'rollout_watch_' || COALESCE(NEW.namespace, OLD.namespace);
    -- Payload = run_id::text || '|' || array_to_string(path, '/')
    -- Length-guarded at 8000 bytes (Pitfall 5); should be impossible in practice.
    payload := COALESCE(NEW.run_id::text, OLD.run_id::text, '') || '|' ||
               array_to_string(COALESCE(NEW.path, OLD.path), '/');
    payload := substring(payload, 1, 7999);
    PERFORM pg_notify(channel, payload);
    RETURN COALESCE(NEW, OLD);
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER kv_notify_trg
    AFTER INSERT OR UPDATE OR DELETE ON kv
    FOR EACH ROW EXECUTE FUNCTION rollout_kv_notify();
```

### Migrations (`database/migrations/0002_snapshots.sql`)

```sql
-- Phase 4 (TRAIN-03): snapshot metadata + structured events.

CREATE TABLE snapshots (
    id              UUID PRIMARY KEY,                 -- SnapshotId (blake3-derived; ContentId-shaped)
    run_id          UUID NOT NULL,
    kind            TEXT NOT NULL,                    -- 'train_state' | 'buffer' | 'process' | 'episodic_memory'
    algorithm_id    TEXT NOT NULL,
    label           TEXT,
    parts_json      JSONB NOT NULL,                   -- SnapshotPart[] serialized
    meta            JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX snapshots_run_idx       ON snapshots (run_id);
CREATE INDEX snapshots_kind_idx      ON snapshots (kind);
CREATE INDEX snapshots_label_idx     ON snapshots (label) WHERE label IS NOT NULL;
CREATE INDEX snapshots_created_idx   ON snapshots (created_at DESC);

CREATE TABLE events (
    id              BIGSERIAL PRIMARY KEY,
    run_id          UUID NOT NULL,
    worker_id       UUID,
    ts              TIMESTAMPTZ NOT NULL DEFAULT now(),
    kind            TEXT NOT NULL,                    -- spec 09 EventKind discriminant
    level           SMALLINT NOT NULL,                -- 0=trace 1=debug 2=info 3=warn 4=error
    payload         JSONB NOT NULL
);
CREATE INDEX events_run_ts_idx ON events (run_id, ts DESC);
CREATE INDEX events_kind_idx   ON events (kind);
```

### Example CLI surface (`rollout-cli/src/main.rs` extension)

```rust
#[derive(clap::Subcommand)]
enum Cmd {
    /// Phase 1 — schema dump.
    Schema(SchemaArgs),
    /// Phase 2 — coordinator/worker daemons.
    Coordinator { #[command(subcommand)] action: CoordinatorAction },
    Worker      { #[command(subcommand)] action: WorkerAction },
    /// Phase 3 — batch inference.
    Infer       { #[command(subcommand)] action: InferAction },
    /// Phase 4 NEW: training.
    Train       { #[command(subcommand)] action: TrainAction },
    /// Phase 4 NEW: snapshot ops.
    Snapshot    { #[command(subcommand)] action: SnapshotAction },
}

#[derive(clap::Subcommand)]
enum TrainAction {
    /// Supervised fine-tuning.
    Sft(TrainSftArgs),
    /// Reward-model training (Bradley-Terry).
    Rm(TrainRmArgs),
}

#[derive(clap::Args)]
struct TrainSftArgs {
    /// Path to SFT TOML config.
    #[arg(long)]
    config: PathBuf,
    /// Resume from this snapshot ID (overrides any auto-discovery).
    #[arg(long)]
    resume: Option<String>,
    /// Dry-run: validate config + load dataset, exit without training.
    #[arg(long)]
    dry_run: bool,
}

#[derive(clap::Subcommand)]
enum SnapshotAction {
    /// List snapshots for a run.
    List(SnapshotListArgs),
    /// Show details for one snapshot.
    Show(SnapshotShowArgs),
    // Phase 4 may also ship: Prune(SnapshotPruneArgs). Otherwise defer to Phase 9.
}
```

### `examples/sft-tiny.toml`

```toml
schema_version = 1

[run]
name = "sft-tiny-smoke"

[storage]
backend = "embedded"
[storage.embedded]
path = "./data/sft-tiny.db"

[algorithm]
kind = "sft"

[algorithm.sft]
minibatch_size = 1
gradient_accumulation = 1

[algorithm.sft.base_model]
uri = "Qwen/Qwen2.5-0.5B-Instruct"

[algorithm.sft.optimizer]
kind = "adamw"
lr = 1e-5
weight_decay = 0.0
betas = [0.9, 0.999]
eps = 1e-8
warmup_steps = 0
schedule = "constant"

[algorithm.sft.budget]
max_steps = 2

[algorithm.sft.dataset]
kind = "jsonl_path"
path = "examples/sft-tiny.jsonl"

[algorithm.sft.packing]
kind = "concat"
max_seq_len = 512

[algorithm.sft.loss_on]
kind = "assistant_only"

[snapshots]
on_completion = true
on_preemption = true
[snapshots.periodic]
interval_steps = 500
kinds = ["train_state"]
[snapshots.retention]
keep_last = 3
keep_labeled = true
```

### `examples/sft-tiny.jsonl`

```jsonl
{"messages": [{"role": "user", "content": "What is 2+2?"}, {"role": "assistant", "content": "2+2 equals 4."}]}
{"messages": [{"role": "user", "content": "Capital of France?"}, {"role": "assistant", "content": "Paris."}]}
{"messages": [{"role": "user", "content": "Largest planet?"}, {"role": "assistant", "content": "Jupiter."}]}
{"messages": [{"role": "user", "content": "Boiling point of water at sea level in Celsius?"}, {"role": "assistant", "content": "100 degrees Celsius."}]}
```

### Architecture-lint additions (Wave 0)

```rust
// crates/rollout-core/tests/dependency_direction.rs (extend)

const ALGO_CRATES: &[&str] = &["rollout-algo-sft", "rollout-algo-rm"];

// Invariant #7: algo-* may NOT depend on rollout-cloud-*
#[test]
fn algo_crates_do_not_depend_on_cloud() {
    for crate_name in ALGO_CRATES {
        for dep in dependencies_of(crate_name) {
            assert!(
                !dep.starts_with("rollout-cloud-"),
                "{crate_name} depends on {dep} — algorithm crates must stay cloud-agnostic",
            );
        }
    }
}

// Invariant #8: algo-* may NOT depend on rollout-transport
#[test]
fn algo_crates_do_not_depend_on_transport() {
    for crate_name in ALGO_CRATES {
        for dep in dependencies_of(crate_name) {
            assert!(
                dep != "rollout-transport",
                "{crate_name} depends on rollout-transport — speak via AlgoDependencies",
            );
        }
    }
}

// Invariant #9: rollout-snapshots may NOT depend on rollout-algo-*
#[test]
fn snapshots_does_not_depend_on_algo() {
    for dep in dependencies_of("rollout-snapshots") {
        assert!(
            !dep.starts_with("rollout-algo-"),
            "rollout-snapshots depends on {dep} — snapshots is CONSUMED by algos",
        );
    }
}
```

Add corresponding fixtures `tests/fixtures/violation_algo_uses_cloud/`, `violation_algo_uses_transport/`, `violation_snapshots_uses_algo/`.

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Manual FSDP via `torch.distributed` | `accelerate.Accelerator(fsdp_plugin=...)` | accelerate 0.20+, stable 1.0 | One Accelerator API covers FSDP + DDP + DeepSpeed |
| `LLM.generate()` sync vLLM | `AsyncLLMEngine` (Phase 3) | vLLM 0.4+ | Continuous batching for free; bridge via pyo3-async-runtimes |
| `tokio-postgres` + manual pool | `sqlx 0.8` + `PgPool` + `PgListener` | sqlx 0.7→0.8 | Compile-time SQL checks + auto-reconnect listener |
| `tokenizer.encode(role + content + role + content)` manually | `tokenizer.apply_chat_template(messages, return_tensors="pt")` | transformers 4.34+ | Per-model template; tokenizer.chat_template overridable |
| Hand-rolled offline mode hooks | `cargo sqlx prepare` → `.sqlx/` cache + `SQLX_OFFLINE=true` | sqlx 0.6→0.7 | Builds without DB; CI verifies cache via `cargo sqlx prepare --check` |
| `trl.SFTTrainer.train()` (opinionated) | direct `accelerate.Accelerator` loop | always our preference | We control packing + masking + snapshot integration explicitly |
| `pickle` for snapshots | `safetensors` shards + JSON metadata (inside accelerate.save_state output) | safetensors 0.3+ | Memory-safe; no arbitrary-code-execution on load |
| `tar -czf snapshot.tar.gz` (zstd) | uncompressed tar (Pitfall 9) | this phase | Compressed output drifts cross-platform; blake3 hash must be stable |

**Deprecated / outdated (DO NOT USE):**
- `huggingface_hub.snapshot_download` for storing run-side snapshots — that's for hub-side; we use FsObjectStore.
- `trl.RewardTrainer` — opinionated; we hand-roll the Bradley-Terry loss (one line).
- `DeepSpeed` integration — out of v1 scope (FSDP via accelerate is the v1 stack).
- `accelerate.utils.broadcast_object_list` for snapshot — incorrect mental model; use `save_state` + `load_state`.

---

## Open Questions

1. **`rollout-snapshots` crate layout: `kind/train_state.rs` from day one?**
   - What we know: spec 04 §5 enumerates 4 snapshot kinds; Phase 4 ships TrainState only.
   - What's unclear: whether the per-kind module split lands now or in Phase 9 (when Buffer is added).
   - **Recommendation:** ship `kind/train_state.rs` from day one. Cheap to add a module; expensive to move code later. The `Snapshotter` impl dispatches on `SnapshotKind` to `kind::train_state::save(...)` etc.; Phase 9 adds `kind::buffer::save(...)` without touching the dispatcher.

2. **`accelerate` version pin range?**
   - What we know: accelerate 1.0 stabilized `save_state`/`load_state` semantics; 1.9 (Aug 2025) is current.
   - What's unclear: whether to pin `>=1.0,<2.0` (broad) or `>=1.5,<2.0` (safer if older 1.x has bugs).
   - **Recommendation:** start with `>=1.0,<2.0` in docs (`pip install` line) + `>=1.5` in CI's actual install for fewer bug surfaces. Leave the lower bound at 1.0 for users.

3. **`HF transformers` version pin range?**
   - What we know: min `4.37` supports Qwen2.5; `4.45+` has stable `return_assistant_tokens_mask`.
   - What's unclear: upper bound (5.0? 6.0?). transformers 4.54+ has FSDP2 regressions per HF issue #39977.
   - **Recommendation:** `>=4.45,<5.0` in docs + CI; document the 4.54 regression as a known issue if anyone hits it.

4. **`GradHandle` shape — opaque PyObject vs marker?**
   - What we know: D-TRAIN-PATH-04 says "opaque newtype" — sufficient.
   - **Recommendation:** opaque `pyo3::Py<pyo3::PyAny>` under `--features train`; marker `{ step: u64 }` otherwise. Resolved above (Pattern 2). Codifies in `rollout-core/src/traits/backend.rs`.

5. **`sqlx-data.json` location — workspace-level or per-crate?**
   - What we know: sqlx 0.8 supports both. `--workspace` flag → workspace root `.sqlx/`. Per-crate publishing has known issues (sqlx#3644).
   - **Recommendation:** workspace-level `.sqlx/` at repo root (committed). CI runs `cargo sqlx prepare --workspace --check`. Documented in `docs/book/src/training/postgres-backend.md`.

6. **`rollout snapshot prune` — Phase 4 or Phase 9?**
   - What we know: spec 08 §2.5 lists `prune` as a CLI surface; D-TRAIN-PATH spec is silent.
   - **Recommendation:** ship `prune` in Phase 4 — implementation is straightforward against the same `Snapshotter::prune` trait method. Risk-reward favors landing it now so Phase 9 doesn't need to revisit the CLI layer.

7. **mdBook chapter file naming under `docs/book/src/training/`?**
   - **Recommendation:** match the focus-areas: `index.md`, `sft.md`, `rm.md`, `snapshots.md`, `postgres-backend.md`, `determinism.md`, `cli.md`, `cpu-mode.md`. Insertion in SUMMARY.md immediately after the `Inference` section.

8. **What about the `Storage::watch()` trait return type — `broadcast::Receiver` vs. `BoxStream`?**
   - What we know: existing trait returns `tokio::sync::broadcast::Receiver<StorageEvent>` (Phase 2 in-process broadcast). PgListener returns a stream — different shape.
   - **Recommendation:** ADD a parallel `Storage::watch_stream(&self, prefix: StorageKey) -> Result<BoxStream<'static, StorageEvent>, CoreError>` method to the trait. EmbeddedStorage implements it by wrapping its broadcast receiver in `BroadcastStream`. PostgresStorage implements it natively via PgListener. Consumers that want cross-process notification call `watch_stream()`. Phase 2's `watch()` stays untouched to avoid blast radius. Both methods are part of the trait.

9. **PostgresStorage runtime config — pool sizes, timeouts?**
   - **Recommendation:**
     - Production: `min_connections=1, max_connections=16, acquire_timeout=30s, idle_timeout=10m`.
     - Watch pool: `min=0, max=4` (separate from write pool to avoid contention).
     - CI testcontainers: `min=0, max=4, acquire_timeout=30s` (boot can be slow).

---

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Docker | testcontainers Postgres in CI + local | ✓ | 29.5.1 | none — CI uses ubuntu-latest which has Docker pre-installed |
| Postgres 16 | `postgres-integration` CI job + local manual testing | ✓ (homebrew) | 16.14 (local); image via testcontainers in CI | testcontainers spins it anyway; local install optional |
| Python 3.11+ | PyO3 abi3-py311 link (already from Phase 2) | ✓ via pyenv | 3.10.14 local default (must use 3.11.12 for vllm builds — `PYENV_VERSION=3.11.12`) | none — abi3-py311 hard requirement |
| `transformers>=4.45` | `train` feature live tests | ✗ in default env | — | gate live tests on `ROLLOUT_TRANSFORMERS_AVAILABLE=1`; MockBackend tests run without it |
| `accelerate>=1.0` | `train` feature live tests | ✗ in default env | — | same gate as above |
| `torch>=2.1` | `train` feature live tests | ✗ in default env | — | same gate as above |
| `torchdata>=0.8` | OPTIONAL: stateful dataloader for snapshot resume | ✗ in default env | — | fallback to step-based replay (Pitfall 3) |
| `sqlx-cli` | local `cargo sqlx prepare` regeneration | not pre-installed | — | install via `cargo install sqlx-cli --version 0.8 --features postgres,rustls` |
| `Qwen/Qwen2.5-0.5B-Instruct` model weights | live `train-smoke.sh` (`ROLLOUT_TRANSFORMERS_AVAILABLE=1`) | downloaded on first run via `huggingface_hub` | — | needs network on first run; subsequent runs hit `~/.cache/huggingface/hub/` |

**Missing dependencies with no fallback:**
- None blocking. testcontainers Postgres is the only external service and it bootstraps itself.

**Missing dependencies with fallback:**
- All Python training-mode deps fall back to MockBackend path; default CI never installs them. The `train-smoke` job gates on `vars.ROLLOUT_TRANSFORMERS_AVAILABLE == '1'` (mirrors Phase 3's `ROLLOUT_VLLM_AVAILABLE`); default fork CI runs of public PRs skip it.

---

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust: `cargo test`; Python integration: `pytest` (not yet introduced in workspace, deferred to harness phases) |
| Config file | Workspace `Cargo.toml`; per-crate `Cargo.toml` test deps |
| Quick run command | `cargo test -p rollout-algo-sft --tests` |
| Per-feature commands | `cargo test -p rollout-storage --features postgres --test postgres_integration -- --include-ignored` |
| Full suite command | `cargo test --workspace --tests` (default features) |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| TRAIN-01 | `rollout-algo-sft::SftAlgo` impls `PolicyAlgorithm`; runs end-to-end against MockBackend; loss decreases | integration | `cargo test -p rollout-algo-sft --test happy_path` | ❌ Wave 0 + Plan 04-01 |
| TRAIN-01 | JSONL data loader parses `{prompt, completion}` AND `{messages: [...]}` | unit | `cargo test -p rollout-algo-sft --test data_loader` | ❌ Plan 04-01 |
| TRAIN-01 | Packing (`Concat`) produces sequences ≤ `max_seq_len` with EOS separators | Python integration | `cargo test -p rollout-backend-vllm --features train,test-mock-backend --test packing -- --ignored` (gated `ROLLOUT_TRANSFORMERS_AVAILABLE=1`) | ❌ Plan 04-05 |
| TRAIN-01 | `LossScope::AssistantOnly` masks loss to assistant spans on Qwen2.5 | Python integration | same gate | ❌ Plan 04-05 |
| TRAIN-02 | `rollout-algo-rm::RmAlgo` impls `PolicyAlgorithm`; Bradley-Terry pairwise loss math | unit | `cargo test -p rollout-algo-rm --test pairwise_loss` | ❌ Plan 04-04 |
| TRAIN-02 | RM output is content-addressed; round-trip through `save_weights` + `load_weights` produces identical `ContentId` | integration | `cargo test -p rollout-algo-rm --test checkpoint_roundtrip` | ❌ Plan 04-04 |
| TRAIN-03 | Bit-identical resume at step 5 of 10 (MockBackend) | integration | `cargo test -p rollout-algo-sft --test snapshot_resume` | ❌ Plan 04-01 (LOAD-BEARING) |
| TRAIN-03 | Same for RM | integration | `cargo test -p rollout-algo-rm --test snapshot_resume` | ❌ Plan 04-04 |
| TRAIN-03 | Bit-identical resume with HF transformers + Qwen2.5-0.5B-Instruct (CPU) | integration `#[ignore]` | `ROLLOUT_TRANSFORMERS_AVAILABLE=1 cargo test -p rollout-backend-vllm --features train --test snapshot_resume_live -- --ignored` | ❌ Plan 04-05 |
| TRAIN-03 | tar build is byte-identical across runs (determinism preamble proof) | unit | `cargo test -p rollout-snapshots --test deterministic_tar` | ❌ Plan 04-02 |
| TRAIN-03 | `Snapshotter::save` + `restore` round-trip via FsObjectStore + EmbeddedStorage | integration | `cargo test -p rollout-snapshots --test save_restore_roundtrip` | ❌ Plan 04-02 |
| TRAIN-04 | `PostgresStorage` impls `Storage` (CRUD + CAS + scan + watch) | integration | `cargo test -p rollout-storage --features postgres --test postgres_integration -- --include-ignored` | ❌ Plan 04-03 |
| TRAIN-04 | Migrations apply idempotently (run twice, no error) | integration | same target, `test idempotent_migrations` | ❌ Plan 04-03 |
| TRAIN-04 | `PgListener`-backed `watch_stream()` delivers notifications cross-process | integration | same target, `test pg_listener_cross_process` | ❌ Plan 04-03 |
| TRAIN-04 | `sqlx-data.json` (workspace `.sqlx/`) is in sync (offline build passes) | CI lint | `SQLX_OFFLINE=false cargo sqlx prepare --workspace --check -- --features postgres` | ❌ Plan 04-03 |
| Cross-cutting | Architecture-lint invariants #7/#8/#9 hold | unit | `cargo test -p rollout-core --test dependency_direction` | partial — extend in Plan 04-00-b |
| Cross-cutting | mdBook builds; rustdoc gate green | CI | `mdbook build docs/book && cargo doc --workspace --no-deps --all-features` | ❌ Plan 04-07 |
| Cross-cutting | Schema codegen drift-free | CI | `cargo xtask schema-gen && git diff --exit-code schemas/ python/` | extends existing Plan 04-00-a |
| Cross-cutting | `examples/sft-tiny.toml` validates + dry-runs | integration | `cargo test -p rollout-cli --test train_dry_run` | ❌ Plan 04-06 |
| Cross-cutting | `scripts/train-smoke.sh` ends-to-end on Qwen2.5-0.5B-Instruct CPU | live smoke | `make train-smoke` (gated `ROLLOUT_TRANSFORMERS_AVAILABLE=1`) | ❌ Plan 04-07 |

### Sampling Rate

- **Per task commit:** `cargo test -p <touched_crate> --tests` (plus the docs-test-policy check).
- **Per wave merge:** `cargo test --workspace --tests` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo clippy --workspace --all-targets --all-features -- -D warnings` + `cargo deny check` + `mdbook build docs/book`.
- **Phase gate:** Full suite green + `cargo test -p rollout-storage --features postgres --test postgres_integration -- --include-ignored` (testcontainers; takes ~30 s) + `make train-smoke` (gated; live HF transformers) before `/gsd:verify-work`.

### Wave 0 Gaps (test infrastructure to create before implementation)

- [ ] `crates/rollout-algo-sft/tests/snapshot_resume.rs` — TRAIN-03 LOAD-BEARING. Covers TRAIN-03 (MockBackend path).
- [ ] `crates/rollout-algo-rm/tests/snapshot_resume.rs` — covers TRAIN-03 (RM variant).
- [ ] `crates/rollout-algo-sft/tests/happy_path.rs` — TRAIN-01 end-to-end.
- [ ] `crates/rollout-algo-rm/tests/pairwise_loss.rs` — TRAIN-02 loss math.
- [ ] `crates/rollout-snapshots/tests/save_restore_roundtrip.rs` — TRAIN-03 (tar+blake3 path).
- [ ] `crates/rollout-snapshots/tests/deterministic_tar.rs` — Pitfall 9 byte-stability.
- [ ] `crates/rollout-storage/tests/postgres_integration.rs` — TRAIN-04 (gated by `#[cfg(feature = "postgres")]`).
- [ ] `crates/rollout-core/tests/fixtures/violation_algo_uses_cloud/` + 2 more — arch-lint #7/#8/#9.
- [ ] `crates/rollout-cli/tests/train_dry_run.rs` — CLI surface validation.
- [ ] `scripts/train-smoke.sh` — mirrors `scripts/infer-smoke.sh`.

### Wave 0 Framework Install (must exist before any task runs)

- Workspace `Cargo.toml`: add `sqlx`, `testcontainers`, `testcontainers-modules`, `tar`, `ndarray`, `walkdir` workspace deps.
- Per-crate `Cargo.toml`: add `train` feature on `rollout-backend-vllm`; `postgres` feature on `rollout-storage`.
- `database/migrations/0001_init.sql` + `0002_snapshots.sql` files (Wave 0 stubs; SQL bodies finalized in Plan 04-03).
- `.cargo/config.toml`: `[env] SQLX_OFFLINE = "true"` (Pitfall 4 prevention).

---

## Risks + Mitigations

| Risk | Severity | Likelihood | Mitigation |
|------|----------|------------|------------|
| PyO3 thread mode-switch leaves stale CUDA tensors (OOM) | HIGH | MEDIUM | Explicit `del; gc.collect(); torch.cuda.empty_cache()` on every mode switch; Phase 4 mode switches only on startup (single direction). |
| FSDP-determinism is NOT bit-identical on multi-GPU | HIGH | HIGH | Document as best-effort in `docs/book/src/training/determinism.md`. Phase-4 CI is CPU only (0 GPUs → no distributed wrapper). CUDA bit-identical gated `#[ignore]`. |
| `sqlx-data.json` drift if schema changes without re-running `cargo sqlx prepare` | MEDIUM | MEDIUM | CI runs `cargo sqlx prepare --workspace --check`; fail PR if drift. Document in `docs/book/src/training/postgres-backend.md`. |
| testcontainers boot time exceeds CI step timeout on slow runners | MEDIUM | LOW | testcontainers-modules `WaitFor::message_in_stdout` gate; retry loop on first PgPool acquire. Budget pad of 60 s before declaring failure. |
| Qwen2.5 chat-template stability (tokenizer version pin) | MEDIUM | LOW | Pin tokenizer version transitively via transformers pin. Override the chat_template explicitly with the `{% generation %}`-marked variant (Pitfall 1). |
| `accelerate.save_state` doesn't capture dataloader cursor without torchdata | MEDIUM | MEDIUM | If torchdata available, use `use_stateful_dataloader=True`; else fall back to step-based replay (Pitfall 3). Test exercise BOTH paths. |
| Postgres LISTEN/NOTIFY payload truncation at 8000 bytes | LOW | LOW | trigger function caps payload at 7999; integration test exercises a long-suffix row to verify graceful fallback. |
| `Snapshotter` trait replacement breaks downstream consumers | LOW | LOW | Only consumer is the unimplemented `AlgoDependencies::snapshots` slot. No external users. |
| HF Hub rate-limit during CI on first model download | LOW | LOW | Gate live tests behind `ROLLOUT_TRANSFORMERS_AVAILABLE=1`; populated CI environments cache via `~/.cache/huggingface/hub/`. |
| `Storage::watch()` ↔ `watch_stream()` consumer confusion | LOW | LOW | Document both methods in the trait rustdoc; example in `docs/book/src/substrate/storage.md`. |
| Workspace-level `.sqlx/` cache conflicts with per-crate publish | LOW | LOW | sqlx 0.8 supports both; we stay workspace-level until SHIP-01. Revisit if `cargo publish` flakes. |

---

## Sources

### Primary (HIGH confidence)
- `docs/specs/02-algorithms.md` §§ 2, 6, 7, 11 — PolicyAlgorithm trait, SFT, RM, open questions
- `docs/specs/04-storage-snapshots.md` §§ 3.2, 3.3, 5, 6, 7, 9 — Postgres schema, storage selection, Snapshotter trait, policy, restore semantics, failure modes
- `docs/specs/08-cli.md` §§ 2, 3 — CLI command surface, config file conventions
- `docs/specs/10-component-split.md` — dep direction; algorithm crates Layer 3
- `docs/specs/11-config-schema.md` — single-source-of-truth config + tagged unions
- `AGENTS.md` §9 — DOCS-01/02/03 per-commit policy
- `.planning/phases/03-inference-batch/03-RESEARCH.md` Pitfalls 9 + 10 — explicit CUDA probe + env-write-before-import
- `.planning/phases/03-inference-batch/03-SUMMARY` files — `pyo3_async_runtimes::tokio::run_until_complete` bridge, dedicated Python OS thread
- Existing source: `crates/rollout-backend-vllm/src/engine.rs` (Phase 3 Pitfall 10 implementation reference)
- Existing source: `crates/rollout-runtime-batch/src/mock_backend.rs` (MockBackend pattern to extend)
- Existing source: `crates/rollout-storage/src/embedded/tables.rs` (namespace registration pattern)
- Existing source: `crates/rollout-cli/tests/restart_no_duplicates.rs` (the test pattern `snapshot_resume.rs` mirrors)

### Secondary (MEDIUM confidence — WebSearch verified against official docs)
- HuggingFace Accelerate docs — `Accelerator.save_state` / `load_state` semantics — https://huggingface.co/docs/accelerate/en/usage_guides/checkpoint
- HuggingFace Accelerate `Accelerator` API reference — https://huggingface.co/docs/accelerate/en/package_reference/accelerator
- HuggingFace transformers `apply_chat_template` `return_assistant_tokens_mask` — https://github.com/huggingface/transformers/issues/34172 (Qwen2.5 lacks `{% generation %}`)
- sqlx 0.8 `PgListener` API — https://docs.rs/sqlx/latest/sqlx/postgres/struct.PgListener.html
- sqlx 0.8 LISTEN example — https://github.com/launchbadge/sqlx/blob/main/examples/postgres/listen/src/main.rs
- sqlx offline mode docs — https://deepwiki.com/launchbadge/sqlx/8.3-offline-mode-(prepare-command)
- sqlx `.env` + `SQLX_OFFLINE` known issue — https://github.com/launchbadge/sqlx/issues/3836
- testcontainers-modules Postgres docs — https://docs.rs/testcontainers-modules/latest/testcontainers_modules/postgres/index.html
- testcontainers-rs community modules — https://github.com/testcontainers/testcontainers-rs-modules-community
- PyTorch `torch.use_deterministic_algorithms` docs — https://docs.pytorch.org/docs/stable/generated/torch.use_deterministic_algorithms.html
- HF accelerate FSDP guide — https://github.com/huggingface/accelerate/blob/main/docs/source/usage_guides/fsdp.md
- HF accelerate save_state custom-method issue — https://github.com/huggingface/accelerate/issues/976
- Qwen/Qwen2.5-0.5B-Instruct min transformers — https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct (requires transformers ≥ 4.37)

### Tertiary (LOW confidence — single source / not yet verified by running)
- Specific accelerate 1.x → 2.x deprecation timeline (no official announcement found; treat `<2.0` upper bound as defensive)
- transformers 4.54+ FSDP2 regression (HF issue #39977) — pinning `<4.55` recommended for FSDP-heavy paths; Phase 4 CPU paths are unaffected
- `assistant_tokens_mask` exact Jinja contract — verified by issue #34172 but no canonical doc; if Qwen 4.x finally adds it, simplify Pitfall 1 path

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — sqlx 0.8, testcontainers-modules, tar, ndarray are all well-known versions.
- Architecture (PyO3 mode switch + asyncio bridge): HIGH — directly extends Phase 3's verified pattern.
- Determinism stack: HIGH on CPU; MEDIUM on CUDA — CPU is bit-identical unconditionally; CUDA carries the same-SM caveat already documented in spec 04 §5.3.
- Postgres backend: HIGH — sqlx 0.8 + PgListener + migrate macro are battle-tested.
- Qwen2.5 chat template loss mask: MEDIUM — Qwen2.5 lacks `{% generation %}` markers; we override the template (Pitfall 1).
- `sqlx-data.json` location decision: MEDIUM — workspace-level is the documented recommendation; per-crate has known issues.
- accelerate / transformers / torch version pins: MEDIUM — recommended ranges based on issue tracker hints; final pins land in Plan 04-05.
- CLI surface (`rollout train sft` / `rollout snapshot list`): HIGH — directly mirrors Phase 3's `rollout infer batch` clap derive surface.

**Research date:** 2026-05-21
**Valid until:** 2026-06-21 (30 days; HF transformers / accelerate / sqlx all release on monthly-to-quarterly cadence; pins above are defensive but should be re-verified at plan-execution time).
