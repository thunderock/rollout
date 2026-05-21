# Phase 4: SFT + reward-model training + training-state snapshots — Context

**Gathered:** 2026-05-21
**Status:** Ready for planning
**Source:** Synthesized from `/gsd:discuss-phase 4` Q&A + ROADMAP.md §"Phase 4" + REQUIREMENTS.md TRAIN-01..04 + AGENTS.md §9 + docs/specs/02-algorithms.md §2/§6/§7 + docs/specs/04-storage-snapshots.md §3.2/§5/§6 + docs/specs/08-cli.md §2.5 + Phase 1/2/3 CONTEXT.md.

<domain>
## Phase Boundary

Phase 4 delivers the **first end-to-end training story**: supervised fine-tuning + Bradley-Terry reward-model training + bit-identical-resume training-state snapshots + the Postgres `Storage` backend alongside the existing embedded one. This is the precursor to RL (Phase 9) — proves the training loop, snapshot system, and metadata store.

Five new crates ship plus one Wave-0 trait extension to `rollout-core`:

- **`rollout-algo-sft`** — `PolicyAlgorithm` impl for supervised fine-tuning with sample-packing, loss-on-assistant masking, JSONL data loader (TRAIN-01).
- **`rollout-algo-rm`** — `PolicyAlgorithm` impl for Bradley-Terry reward-model training with pairwise loss; outputs a content-addressed checkpoint registered in the run's artifact index (TRAIN-02).
- **`rollout-snapshots`** — `Snapshotter` trait impl. Phase 4 ships the **`TrainState`** snapshot kind only (Buffer/Process/EpisodicMemory variants enumerated in the enum but their impls land in Phases 9/11/8). Snapshot blobs go to `FsObjectStore`; metadata rows go to `Storage` (TRAIN-03).
- **`rollout-backend-vllm` extension** (no new crate; in-place feature addition) — gain a new `train` Cargo feature that pulls HF transformers + accelerate + FSDP/DDP via PyO3 and impls `TrainableBackend` (sibling trait of `InferenceBackend`). vLLM stays the inference path; transformers + accelerate is the training path. Reuses the Phase-3 dedicated Python OS thread infrastructure.
- **`rollout-storage` extension** (no new crate; new `postgres` Cargo feature + new module) — Postgres `Storage` impl on `sqlx 0.8` with `PgListener`-backed `watch()`; migrations under `database/migrations/` via the `sqlx::migrate!()` macro; testcontainers-driven CI integration job (TRAIN-04).
- **`rollout-cli`** gains the `train sft` and `train rm` subcommands.

**Out of scope (explicit):**

- Buffer / Process / EpisodicMemory snapshot kinds — Phases 9 / 11 / 8.
- PPO / GRPO / DPO / IPO / KTO — Phases 9 / 10. (Phase 9 ships `Buffer` snapshots when it needs rollout-buffer persistence.)
- HuggingFace datasets Hub integration (`datasets.load_dataset("squad")`) — Phase 7 (`HARNESS-*` eval harnesses bring it). Phase 4 supports `DatasetRef::JsonlPath` only.
- Cloud object stores (S3 / GCS) for snapshot blobs — Phase 5 (`CLOUD-03`). Phase 4 snapshot blobs land in `FsObjectStore`.
- Runtime backend selection via TOML — Phase 8. Phase 4 keeps the Phase-3 Cargo-feature pattern (`--features vllm` / `--features test-mock-backend`).
- Online inference / tool calling — Phase 8.
- Training perf vs HF TRL / DeepSpeed reference — ROADMAP risk callout. Phase 4 skips kernel-level optimization; relies on accelerate + FSDP defaults.

</domain>

<decisions>
## Implementation Decisions

### Training execution path (backend architecture)

- **D-TRAIN-PATH-01** — **Sibling `TrainableBackend: InferenceBackend` trait** in `rollout-core`. Methods (verified against spec 02 §2; researcher refines):
  ```rust
  #[async_trait]
  pub trait TrainableBackend: InferenceBackend {
      async fn set_train_mode(&mut self, enabled: bool) -> Result<(), CoreError>;
      async fn forward_with_loss(
          &self,
          batch: &TrainBatch,
          loss_scope: LossScope,
      ) -> Result<LossOutput, CoreError>;
      async fn optimizer_step(&mut self, grads: GradHandle, opt: &OptimizerSettings) -> Result<(), CoreError>;
      async fn save_weights(&self) -> Result<ContentId, CoreError>;
      async fn load_weights(&mut self, weights_id: &ContentId) -> Result<(), CoreError>;
  }
  ```
  Backends opt-in by impl'ing both `InferenceBackend` and `TrainableBackend`.
- **D-TRAIN-PATH-02** — **In-crate via a new `train` Cargo feature on `rollout-backend-vllm`** (NOT a new crate). With `train` ON, `VllmBackend` impls both traits: inference path uses vLLM AsyncLLMEngine (Phase 3 code); training path uses HF transformers + accelerate (new code) via the SAME dedicated Python OS thread infrastructure (`rollout-py-vllm-<engine_id>`). Both code paths share the PyO3 thread; only one mode is active per run (selected by `set_train_mode`).
- **D-TRAIN-PATH-03** — **HF transformers + accelerate + FSDP** (with DDP fallback for single-device CPU runs). `accelerate.Accelerator()` wraps the model + optimizer + LR scheduler + data loader. FSDP for multi-GPU; DDP single-process when only one device is visible. The `disable_log_stats=True` + `disable_log_requests=True` Phase-3 pattern applies to the inference path; training path uses `accelerate` log control.
- **D-TRAIN-PATH-04** — **Single `Arc<dyn TrainableBackend>` slot in `AlgoDependencies`**. Since TrainableBackend extends InferenceBackend, the algorithm gets both inference and training methods through one slot. Phase 9 PPO actor calls inference methods on this Arc; learner calls training methods. If the actor and learner need distinct backends in Phase 9, add a second slot then.
- **D-TRAIN-PATH-05** — **`MockBackend` (from 03-02, behind `test-mock-backend` feature) extends with `TrainableBackend` impl** so SFT/RM integration tests run deterministically on every CI build with no HF transformers installed. Fake weights as `ndarray::Array1<f32>`; fake `forward_with_loss` returns `loss = 0.5` + zero grads; `optimizer_step` is plain SGD against the fake weights. ~10 ms per step; full epoch runs in a second.
- **D-TRAIN-PATH-06** — **Cargo-feature backend selection** (same pattern as Phase 3). Production: `cargo build -p rollout-cli --features vllm,train` enables real backend with both modes. Tests: `--features test-mock-backend` swaps in extended `MockBackend`. Runtime backend selection deferred to Phase 8 (`INFER-01`) when SGLang/TGI arrive.

### Wave 0 trait surgery (mirrors 02-00 / 03-00 pattern)

- **D-WAVE0-01** — **Full Wave 0 surgery in 2 tasks.** Task 1: trait surface (extend `PolicyAlgorithm` to spec 02 §2 surface; add `Snapshotter` trait per spec 04 §5.2; add `TrainableBackend` trait per D-TRAIN-PATH-01; add ~15 supporting types listed below). Task 2: register 3 new crates (`rollout-algo-sft`, `rollout-algo-rm`, `rollout-snapshots`); add `rollout-backend-vllm` `train` feature gate; add `rollout-storage` `postgres` feature gate; add `sqlx 0.8` + accelerate-related workspace deps; arch-lint invariant additions; spec edits (02 §2a, 04 §5a, 08 §2.5 verify).
- **D-WAVE0-02** — **PolicyAlgorithm full surface** per spec 02 §2: `id()`, `Settings: DeserializeOwned + Serialize + JsonSchema + Send + Sync + 'static`, `from_settings(settings, deps)`, `required_roles()`, `validate_plan(plan)`, `run(ctx)`, `snapshot_save()`, `snapshot_restore(snapshot)`. Same `#[non_exhaustive]` hygiene as Phase 3's `SamplingParams`.
- **D-WAVE0-03** — **Supporting types in Wave 0** (in `rollout-core::config` unless noted):
  - `Snapshot`, `SnapshotKind` (enum: `TrainState`, `Buffer`, `Process`, `EpisodicMemory` — only `TrainState` impls live; rest enumerated)
  - `SnapshotPart`, `RestoreTarget` (enum: `SameRun`, `Fork { new_run_id }`, `Worker { worker_id }`)
  - `SnapshotRequest`, `SnapshotFilter`, `PrunePolicy`, `RetentionPolicy`, `SnapshotPolicy`, `PeriodicPolicy`
  - `AlgoDependencies { backend: Arc<dyn TrainableBackend>, storage: Arc<dyn Storage>, object: Arc<dyn ObjectStore>, snapshots: Arc<dyn Snapshotter>, events: Arc<dyn EventEmitter> }`
  - `AlgoContext<'a> { plan: &'a Plan, worker: WorkerId, cancel: CancellationToken, clock: &'a dyn Clock }`
  - `Plan` (placeholder for now; Phase 6 `rollout plan` expands it)
  - `OptimizerSettings { kind: OptimizerKind, lr, weight_decay, betas, eps, warmup_steps, schedule: LrSchedule }`
  - `OptimizerKind` enum (AdamW, Adam, SGD, ...); `LrSchedule` enum (Constant, Linear, Cosine, ...)
  - `TrainingBudget { max_steps, max_tokens, max_walltime }`
  - `ModelRef` is already in `rollout-core` from Phase 3
  - `DatasetRef { JsonlPath(PathBuf), Other(SmolStr) }` — `Other` placeholder for Phase 7 HF datasets variant
  - `PackingPolicy { kind: PackingKind, max_seq_len }`; `PackingKind` enum (`Concat`, `Bucketed`, `Off`)
  - `LossScope` enum (`AssistantOnly`, `Full`, `Custom(MaskSpec)`); `MaskSpec` (placeholder)
  - `SftSettings { base_model, optimizer, budget, dataset, packing, loss_on, minibatch_size, gradient_accumulation }`
  - `RmSettings { base_model, optimizer, budget, dataset, head, minibatch_size }`; `RmHeadKind` enum (`BradleyTerry`, `PairwiseLogistic`)
  - `TrainBatch`, `LossOutput`, `GradHandle` for the `TrainableBackend` interface (opaque handle types so backends can pass internal references without round-tripping through Rust on every step)
  - `WorkerRole` already added in Phase 3; verify `LearnerWorker` variant exists or add it
- **D-WAVE0-04** — **Algorithm crates:** `rollout-algo-sft` and `rollout-algo-rm` as separate crates (matches spec 02 naming + SHIP-01 publishability). Each owns one `PolicyAlgorithm` impl + its `Settings` type's full schema implementation.
- **D-WAVE0-05** — **`rollout-snapshots` standalone crate.** Impls the `Snapshotter` trait against an injected `Arc<dyn Storage>` (for metadata rows) + `Arc<dyn ObjectStore>` (for blobs). Phase 4 only impls the `TrainState` kind; other variants return `Fatal { kind: PluginContract, msg: "Phase N: <kind>" }`.

### Snapshot determinism (TRAIN-03 critical)

- **D-DETERM-01** — **Determinism stack**: `accelerate.Accelerator.save_state(dir)` + `load_state(dir)` captures model + optimizer + LR scheduler + RNG (torch/numpy/python) + dataloader cursor. We additionally set `torch.use_deterministic_algorithms(True)` + `CUBLAS_WORKSPACE_CONFIG=:4096:8` (env-set at Python thread startup) + `torch.set_float32_matmul_precision("highest")` for CUDA bit-identicality. CPU runs are bit-identical unconditionally; CUDA runs are bit-identical IFF the same GPU SM + same cuDNN version. **Cross-machine resume is documented as best-effort** (spec 04 §5.3 already states this).
- **D-DETERM-02** — **One tar per snapshot, one `ContentId`**. After `accelerate.save_state(dir)`, we `tar` the directory (deterministic ordering by name; no compression to keep hashing stable across machines), blake3-hash the tar, and write to `FsObjectStore` at the resulting `ContentId`. Restore: fetch the tar, blake3-verify, extract to a tempdir, `accelerate.load_state(tempdir)`. Loses per-component dedup but simpler. Phase 5 may revisit if S3 storage costs warrant per-blob dedup.
- **D-DETERM-03** — **MockBackend-driven deterministic-resume test** (mirrors Phase 3's `restart_no_duplicates` pattern). `crates/rollout-algo-sft/tests/snapshot_resume.rs`: 10 SGD steps against fake weights with a fixed RNG seed; snapshot at step 5; restart with `--resume <snapshot_id>`; run 5 more steps; **byte-compare the final weights against a non-interrupted run's step-10 weights**. Test runs on every CI build (no GPU, no HF transformers required); ~1 s wall-clock. The full HF-transformers + Qwen2.5-0.5B-Instruct equivalent test exists too but gated `#[ignore]` unless `ROLLOUT_TRANSFORMERS_AVAILABLE=1` (parallel to Phase 3's `ROLLOUT_VLLM_AVAILABLE` gate).
- **D-DETERM-04** — **Default `SnapshotPolicy`**: `on_completion: true`, `periodic: Some(PeriodicPolicy { interval_steps: 500, interval_tokens: None, interval_walltime: None, kinds: vec![SnapshotKind::TrainState] })`, `on_preemption: true`. Retention: `keep_last: 3`, `keep_labeled: true`, `max_age: None`. SIGTERM hook triggers an opportunistic snapshot before exit (mirrors Phase 2's worker graceful drain).
- **D-DETERM-05** — **Algorithm internal state via `serde_json::Value`**. `PolicyAlgorithm::snapshot_save` returns a `Snapshot::with_meta(algorithm_id, serde_json::Value)`. The algorithm packs its own incremental state (curriculum cursor, schedule overrides, anything not captured by accelerate.save_state) into a JSON object. `snapshot_restore` receives the same Value back. Simplest; flexible; framework owns step/RNG/optimizer via accelerate, algorithm owns "extras."

### Postgres backend (TRAIN-04) + dataset loader

- **D-PG-01** — **`sqlx 0.8`** as the Postgres client. Compile-time-checked SQL queries (`sqlx::query!()` macro). Built-in async connection pool. `PgListener` for LISTEN/NOTIFY. `sqlx::migrate!()` macro embeds SQL migration files at compile time. Workspace deps: `sqlx = { version = "0.8", default-features = false, features = ["runtime-tokio", "postgres", "macros", "migrate", "json", "chrono", "uuid"] }`. We commit a `sqlx-data.json` so offline builds (`SQLX_OFFLINE=true`) pass without a live Postgres.
- **D-PG-02** — **Migrations under `database/migrations/`** (the Phase-1-reserved directory). Numbered `.sql` files (`0001_init.sql`, `0002_snapshots.sql`, ...). `PostgresStorage::new(url, pool_size)` runs `sqlx::migrate!("../../database/migrations").run(&pool)` at construction. CI testcontainers job runs the same. Production: `rollout cloud doctor --provider postgres` (Phase 5) verifies migrations applied OR users run `sqlx-cli` manually.
- **D-PG-03** — **`watch()` via `PgListener`**: storage writes also `pg_notify('rollout_watch_<namespace>', <postcard-encoded-key-suffix>)`. `watch(prefix)` opens a `PgListener` listening on the matching channel(s), filters notifications by prefix, returns a `BoxStream<StorageEvent>`. **Cross-process watch works** (unlike Phase 2's in-process broadcast for the embedded backend) — this is the main capability the Postgres backend unlocks.
- **D-PG-04** — **Testcontainers CI integration in DEFAULT CI** (not opt-in). New `postgres-integration` job in `.github/workflows/ci.yml`: `needs: test`; spins up testcontainers-modules Postgres 16; runs `cargo test -p rollout-storage --features postgres --test postgres_integration -- --include-ignored`. Verifies CRUD round-trip, CAS atomicity, LISTEN/NOTIFY watch fan-out, migration application, connection pool reuse. Adds the 15th CI job. Default public-runner CI fires this because Docker is available on `ubuntu-latest`.
- **D-PG-05** — **Schema (Phase 4 subset)** per spec 04 §3.2: one generic `kv(namespace TEXT, run_id UUID, path TEXT[], value BYTEA, version BIGINT, updated_at TIMESTAMPTZ)` with `PRIMARY KEY (namespace, run_id, path)`; plus specialized `runs`, `snapshots`, `events` tables. Phase 4 ships only `kv` + `snapshots` + `events`. `runs`/`workers`/`heartbeats`/`work_items` defer to Phase 6 when multi-node coordinator state grows beyond what `kv` can hold.
- **D-DATA-01** — **`DatasetRef::JsonlPath(PathBuf)` is the only Phase-4 variant.** Schema: SFT reads `{"prompt": String, "completion": String}` per line (one chat turn) or `{"messages": [{role, content}, ...]}` (multi-turn chat). RM reads `{"prompt": String, "chosen": String, "rejected": String}` per line. `DatasetRef::Other(SmolStr)` is enumerated for forward-compat; reading it returns `Fatal(ConfigInvalid)` in Phase 4. HF datasets Hub integration defers to Phase 7 (`HARNESS-*`).
- **D-DATA-02** — **Tokenization on the Python side** via the same HF tokenizer as the model. The `[base_model] tokenizer = "..."` override (already in Phase 3 `ModelRef`) controls which tokenizer to load. Packing implemented in Python via a custom collator: concatenate tokenized samples up to `packing.max_seq_len` with EOS separators. `LossScope::AssistantOnly` masks loss to assistant-role spans via the chat template's role markers (HF tokenizer's `apply_chat_template` + role-id tracking).
- **D-DATA-03** — **Test model: `Qwen/Qwen2.5-0.5B-Instruct`** (same as Phase 3). Apache-2.0, CPU-runnable in tiny batches in <60 s, vLLM-supported (so inference smoke still works after the train-feature extension). ROADMAP says "1B model" as guidance; 500M serves the exit criterion better (faster CI; Apple Silicon CPU friendly).

### Cross-cutting (inherited from Phase 1/2/3)

- **DOCS-01..03 standing rules** apply to every Phase-4 commit. New mdBook chapters under `docs/book/src/training/` (parallel to `inference/` from Phase 3). Subsections: index, sft, rm, snapshots, postgres-backend, determinism, cli. Rustdoc gate passes; per-commit doc/test policy enforced.
- **Architecture-lint** invariants. Phase 3 ended at 6. Phase 4 adds:
  - #7: `rollout-algo-*` crates may NOT depend on `rollout-cloud-*` (algorithms are cloud-agnostic per spec 10).
  - #8: `rollout-algo-*` crates may NOT depend on `rollout-transport` (algorithms speak to peers through `AlgoDependencies`, not direct transport).
  - #9: `rollout-snapshots` may NOT depend on `rollout-algo-*` (the snapshots crate is consumed by algorithms, not the other way around).
- **PyO3 + Python**: tokenizer ownership shifts vs Phase 3. Phase 3 said "backend owns tokenizer; algorithms never see token IDs." Phase 4 inherits that: SFT/RM call `TrainableBackend::forward_with_loss(batch)` where `batch` carries tokenized inputs prepared by the BACKEND (which knows the tokenizer). The dataset loader passes raw text + the backend tokenizes inside the Python thread. Phase 7 may introduce a first-class tokenizer trait if HARNESS-* needs it.
- **HF_TOKEN handling**: identical to Phase 3. `ROLLOUT_SECRET_HF_TOKEN` is read at plan time via the env-var allowlist from 02-03; passed to the Python thread at startup BEFORE `import transformers` (the same Pitfall-10 contract as `import vllm`).
- **CUDA detection**: explicit `torch.cuda.is_available()` probe (Pitfall 9 inherited from Phase 3). Training path on CPU: documented in `docs/book/src/training/cpu-mode.md`; expected throughput ~0.1–1 token/sec for a 0.5B model on M-series silicon; smoke tests use a 4-sample dataset and 2 SGD steps.
- **Conventional commits**: `feat(04-NN): ...`, `docs(04-NN): ...`, `fix(04-NN): ...`. Per-plan commits atomic. DOCS-02 enforced.

### Claude's Discretion (defer to research / planner)

- Crate organization within `rollout-snapshots`: do we have a `kind/train_state.rs` module from day one (for Phase 9 extension) or inline?
- accelerate version pin (likely `>=1.0,<2.0`).
- HF transformers version pin (likely `>=4.45,<5.0` — must support Qwen2.5 architecture).
- FSDP vs DDP heuristic: probe device count at runtime, default DDP single-device, FSDP multi-device.
- Loss-masking exact implementation for `LossScope::AssistantOnly` — depends on Qwen2.5's chat template token ids.
- `Plan` type definition — Phase 4 ships a minimal placeholder; Phase 6 expands.
- `GradHandle` shape — opaque newtype around a Python-side reference? Or just a marker carrying training-step number?
- Whether the `sqlx-data.json` lives under `crates/rollout-storage/.sqlx/` (sqlx convention) or in a project-level `database/.sqlx/`.
- Postgres `runs` / `workers` table schemas (deferred to Phase 6 per D-PG-05; placeholder migrations may or may not land in Phase 4).
- mdBook chapter file naming (matching `docs/book/src/training/` Phase-4 surface).

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Roadmap & requirements
- `ROADMAP.md` §"Phase 4 — SFT + reward-model training + training-state snapshots" — goal, includes, exit criteria, risks
- `.planning/REQUIREMENTS.md` — TRAIN-01..04
- `.planning/ROADMAP.md` — phase → requirement mapping

### Architectural source-of-truth
- `AGENTS.md` — principles #1 async-native, #2 batching first-class, #3 plan-time validation, #4 single-source-of-truth config, #5 deadline-based health, #7 every plugin testable locally, #10 observability not optional
- `AGENTS.md` §9 standing rules — DOCS-01/02/03 apply to every commit; §9.4 v1-example commitment (Phase 4 lands the SFT building block; the polished recipe arrives Phase 9 → Phase 12)

### Phase-4 canonical specs
- `docs/specs/02-algorithms.md` §2 — PolicyAlgorithm trait + AlgoDependencies + AlgoContext + ModelRef + OptimizerSettings + TrainingBudget
- `docs/specs/02-algorithms.md` §6 — SFT (rollout-algo-sft) — SftSettings, LossScope, lifecycle, failure modes
- `docs/specs/02-algorithms.md` §7 — Reward model (rollout-algo-rm) — RmSettings, RmHeadKind
- `docs/specs/02-algorithms.md` §11 Open questions — tokenizer ownership, content-addressing weights
- `docs/specs/04-storage-snapshots.md` §3.2 — Postgres backend schema + properties
- `docs/specs/04-storage-snapshots.md` §3.3 — Storage selection (TOML config; same trait, same operations)
- `docs/specs/04-storage-snapshots.md` §5 — Snapshot common shape, Snapshotter trait, TrainState kind
- `docs/specs/04-storage-snapshots.md` §6 — SnapshotPolicy + PeriodicPolicy + RetentionPolicy
- `docs/specs/04-storage-snapshots.md` §7 — Restore semantics (SameRun, Fork, Worker)
- `docs/specs/08-cli.md` §2 — CLI command surface (rollout train, rollout snapshot)
- `docs/specs/08-cli.md` §3 — Config file conventions
- `docs/specs/10-component-split.md` — dep-direction; algorithm crates are Layer 3, depend on rollout-core + nothing cloud/transport
- `docs/specs/11-config-schema.md` — single-source-of-truth config; new `[sft]`/`[rm]`/`[storage.postgres]`/`[snapshot]` blocks

### Prior phase context (decisions inherited)
- `.planning/phases/03-inference-batch/03-CONTEXT.md` — D-VLLM-01..05 (PyO3 dedicated thread pattern), D-BACKEND-01..05 (InferenceBackend extension shape), D-RESUME-01..05 (sample-state CAS pattern that snapshot persistence mirrors)
- `.planning/phases/03-inference-batch/03-RESEARCH.md` — Pitfall 9 (explicit torch.cuda.is_available()), Pitfall 10 (env-write before import), `pyo3_async_runtimes::tokio::run_until_complete` bridge
- `.planning/phases/03-inference-batch/03-00-wave0-trait-extensions-SUMMARY.md` — Wave 0 trait-extension pattern Phase 4 mirrors
- `.planning/phases/02-local-substrate/02-02-rollout-storage-SUMMARY.md` — EmbeddedStorage CAS API + `infer` namespace registration pattern (Phase 4 adds `train`, `snapshots` namespaces)
- `.planning/phases/02-local-substrate/02-03-rollout-cloud-local-SUMMARY.md` — FsObjectStore content-addressed sharded layout (Phase 4 reuses for snapshot blobs)

### External library docs (researcher reads)
- HuggingFace `accelerate` docs — Accelerator.save_state/load_state surface
- HuggingFace `transformers` docs — AutoModelForCausalLM + tokenizer + chat template
- `sqlx 0.8` docs — connection pool, PgListener, migrate macro, offline mode
- `testcontainers-modules` Rust crate — Postgres 16 image usage
- `safetensors` (file format) — referenced by accelerate but not directly used by rollout in Phase 4

### Repo state Phase 4 modifies or extends
- `crates/rollout-core/src/traits/algorithm.rs` — extend PolicyAlgorithm (Wave 0)
- `crates/rollout-core/src/traits/backend.rs` — add TrainableBackend (Wave 0)
- `crates/rollout-core/src/traits/storage.rs` — add Snapshotter (Wave 0) — note: snapshotter could live in its own `traits/snapshot.rs` module
- `crates/rollout-core/src/config/` — many new types (Wave 0)
- `crates/rollout-backend-vllm/` — `train` Cargo feature; new `src/train.rs` (or similar) hosting the TrainableBackend impl
- `crates/rollout-storage/` — `postgres` Cargo feature; new `src/postgres/` module
- `crates/rollout-runtime-batch/src/mock_backend.rs` — extend with TrainableBackend impl
- `crates/rollout-cli/src/main.rs` — add `train sft` + `train rm` subcommands
- `crates/rollout-algo-sft/` — NEW crate
- `crates/rollout-algo-rm/` — NEW crate
- `crates/rollout-snapshots/` — NEW crate
- `database/migrations/0001_init.sql`, `0002_snapshots.sql` — NEW
- `Cargo.toml` (workspace) — 3 new members; sqlx + tar workspace deps
- `deny.toml` — verify new transitive deps pass
- `Makefile` — add `train-smoke`, `postgres-test` targets
- `.github/workflows/ci.yml` — add `postgres-integration` job + optional `train-smoke` job
- `docs/book/src/SUMMARY.md` + `docs/book/src/training/` — NEW section
- `examples/sft-tiny.toml`, `examples/rm-tiny.toml`, `examples/sft-tiny.jsonl`, `examples/rm-tiny.jsonl` — NEW

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **`InferenceBackend` trait + `VllmBackend`** — extended in Phase 3. Phase 4's `TrainableBackend` extends `InferenceBackend`; `VllmBackend` gains a `train` feature gate.
- **`MockBackend`** (Phase 3, behind `test-mock-backend` feature) — Phase 4 extends it with `TrainableBackend` impl for deterministic-resume CI tests.
- **`EmbeddedStorage` with CAS** (Phase 2) — Phase 4's Postgres impl matches the same trait; algorithms write snapshot metadata via `Storage::cas_bytes` regardless of backend.
- **`FsObjectStore` content-addressed sharded layout** (Phase 2) — Phase 4 reuses for snapshot blob storage.
- **`rollout-runtime-batch::EventEmitter` wiring + `StdoutJsonEmitter`** (Phase 2/3) — Phase 4 emits `train_step`, `snapshot_saved`, `snapshot_restored`, `train_completed` events.
- **PyO3 dedicated Python OS thread** (Phase 2 plugin-host; Phase 3 vllm-backend) — Phase 4 training path reuses the same thread inside `VllmBackend`; the thread already does `Python::attach` + pyo3-async-runtimes bridging.
- **`pyo3_async_runtimes::tokio::run_until_complete`** (Phase 3 03-03 SUMMARY) — the working asyncio bridge.

### Established Patterns
- **Wave 0 trait surgery** — same pattern as 02-00 + 03-00. Two tasks: trait surface first, then crate registrations + spec edits.
- **Cargo feature gates for heavy Python deps** (Phase 3 `vllm` feature) — Phase 4 adds `train` (transformers + accelerate) on `rollout-backend-vllm`, `postgres` (sqlx) on `rollout-storage`.
- **CI strategy**: default public-runner CI stays green without GPU/HF-transformers; live tests gated `#[ignore]` unless `ROLLOUT_*_AVAILABLE=1`; testcontainers Postgres is the exception — it runs in default CI because Docker is available.
- **MockBackend pattern** — Phase 3 proved its value; Phase 4 extends it for training; Phase 9 PPO will extend it again for actor/learner symmetry.
- **DOCS-02 per-commit policy** — every Phase-4 plan commits docs + tests + code in the same diff.

### Integration Points
- **`crates/rollout-storage/src/lib.rs`** — add a `Storage` trait re-export for `Backend` selection; modify `EmbeddedStorage::new` to be one variant of a higher-level `Storage::new(StorageConfig)` factory that picks embedded or postgres.
- **`crates/rollout-storage/src/embedded/tables.rs`** — register `train` and `snapshots` namespaces (mirrors Phase 3's `infer` registration).
- **`crates/rollout-cli/src/main.rs`** — add `Cmd::Train(TrainCmd)` variant; `TrainCmd::Sft(TrainSftArgs)` and `Rm(TrainRmArgs)` subcommands. Existing `infer batch`, `coordinator run`, `worker run`, `schema` subcommands untouched.
- **`crates/rollout-core/tests/dependency_direction.rs`** — extend with invariants #7/#8/#9 + fixture violations.
- **`.github/workflows/ci.yml`** — add `postgres-integration` job (default fire) + optional `train-smoke` job (gated on `ROLLOUT_TRANSFORMERS_AVAILABLE=1`). Existing 14 jobs untouched.
- **`Makefile`** — add `train-smoke`, `postgres-test` targets. Preserve existing.
- **`docs/book/src/SUMMARY.md`** — add `Training` top-level section.
- **`scripts/`** — new `scripts/train-smoke.sh` mirrors `scripts/infer-smoke.sh` pattern.

</code_context>

<specifics>
## Specific Ideas

- **First training story matters.** Phase 4 is the first time rollout trains a model end-to-end. The smoke test should be `rollout train sft --config examples/sft-tiny.toml` and it should finish on CPU in under 5 minutes with a 4-sample dataset and 2 SGD steps against Qwen2.5-0.5B-Instruct.
- **Snapshot-resume test is the load-bearing proof** for TRAIN-03 — it's the SFT analog of Phase 3's `restart_no_duplicates`. MockBackend-driven, deterministic, runs on every CI build in ~1 s.
- **Postgres testcontainers in default CI** is the load-bearing proof for TRAIN-04. Adds the 15th CI job. Docker is available on `ubuntu-latest` so the public-runner cost is just the test runtime (~30 s for Postgres boot + ~10 s for the integration tests).
- **DDP single-device default + FSDP multi-device probe.** accelerate auto-selects based on visible devices; we document the heuristic but don't hardcode it.
- **`sqlx-data.json` committed.** Without it, `cargo build` against rollout-storage would require a live Postgres at build time. Commit it; CI verifies it's in sync via `cargo sqlx prepare --check`.
- **HF transformers / accelerate version pins live in `python/rollout/backends/vllm/pyproject.toml` (or a Phase-4-equivalent)**. Document the pin range in the mdBook chapter so users know what to `pip install`.
- **`rollout snapshot list` / `show` / `prune` CLI** (spec 08 §2.5) ride on the same `Snapshotter` trait. Phase 4 ships `snapshot list` + `snapshot show`; `prune` may slip to Phase 9 if scope tight.
- **Cross-machine restore documented as best-effort.** Spec 04 §5.3 already says this; the mdBook chapter spells out CUDA-determinism caveats.
- **`examples/sft-tiny.toml`** lives at the repo root next to `examples/batch-tiny.toml` from Phase 3.

</specifics>

<deferred>
## Deferred Ideas

- **Streaming generation / online inference** — Phase 8 (`INFER-01..02`).
- **PPO / GRPO / DPO / IPO / KTO** — Phases 9 (`RL-*`), 10 (`OFFLINE-*`).
- **Buffer snapshot kind** — Phase 9 (`RL-03`); enumerated but unimplemented in Phase 4.
- **Process (CRIU-style) snapshot kind** — Phase 11 (`SNAPSHOT-01`); enumerated but unimplemented.
- **Episodic-memory snapshot kind** — Phase 8 (`INFER-03`); enumerated but unimplemented.
- **HF datasets Hub integration** — Phase 7 (`HARNESS-*`); `DatasetRef::Other` placeholder lands in Phase 4.
- **S3 / GCS object stores for snapshot blobs** — Phase 5 (`CLOUD-03`); Phase 4 uses `FsObjectStore`.
- **`runs` / `workers` / `heartbeats` / `work_items` Postgres tables** — Phase 6 (`DIST-01..05`); Phase 4 ships only `kv`, `snapshots`, `events`.
- **First-class tokenizer trait** — Phase 7 if HARNESS-* needs it.
- **Reader/writer worker split** — Phase 6.
- **Runtime backend selection (TOML-driven)** — Phase 8.
- **trl Trainer integration / HF model upload after training** — out of v1 scope (post-1.0).
- **DeepSpeed integration** — out of v1 scope (FSDP via accelerate is the v1 stack).
- **`rollout snapshot prune` CLI** — may slip from Phase 4 to Phase 9 if scope tight.
- **Speculative decoding / prefix caching tuning** — Phase 9+ perf work.
- **Cross-machine bit-identical resume on CUDA** — documented as best-effort; never a v1 guarantee.

</deferred>

---

*Phase: 04-train-sft-rm-snapshots*
*Context gathered: 2026-05-21 via `/gsd:discuss-phase 4`*
