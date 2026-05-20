---
phase: 03-inference-batch
plan: 02
subsystem: batch-inference-runtime
tags: [rollout-runtime-batch, cas-state-machine, sample-id, schema-version, postcard, blake3, batch-coordinator, batch-worker, mock-backend, jsonl, infer-batch-config, mdbook, embedded-storage, fs-object-store, in-mem-queue, resume-semantics, stale-running-reclaim]

# Dependency graph
requires:
  - phase: 02-local-substrate
    provides: "EmbeddedStorage::cas_bytes + scan_bytes + watch (02-02); InMemQueue::open/enqueue/dequeue/ack with cloudlocal_queue spill (02-03); FsObjectStore content-addressed put_bytes/get_bytes (02-03)"
  - phase: 03-inference-batch
    provides: "Wave-0 trait surface (03-00) — InferenceBackend four-method shape + Prompt/Completion/ModelRef/SamplingParams newtypes + WorkerRole enum + non_exhaustive SamplingParams ready for the schema-version constant; sibling Wave-2 plan 03-01 shipped VllmBackend whose Wave-2 stub returns PluginContract — the runtime crate composes Arc<dyn InferenceBackend> so 03-01's stub and this plan's MockBackend are interchangeable"
provides:
  - "SAMPLING_PARAMS_SCHEMA_VERSION: u8 = 1 constant at crate root — first byte of the blake3 hasher input in sample_id() per RESEARCH Pitfall 1; bumping invalidates outstanding Pending/Running sample-IDs and is the documented migration knob for SamplingParams field-adds"
  - "sample_id(model_content_id, prompt, params, idx) -> ContentId — deterministic per (model, prompt, params-bytes, idx) tuple; property-tested over 256 random inputs across 6 dimensions (prompt, idx, model, temperature, max_tokens, seed) + locked-hex regression for the default-params case"
  - "SampleRecord { id, prompt_blob, state: SampleState, created_at_ms, input_idx } postcard-encoded under StorageKey { namespace: 'infer', run_id, path: ['samples', sid_hex] } — input_idx field per WARN-1 lets collect_done_records() emit output JSONL in input order without a side table"
  - "SampleState enum: Pending | Running { worker_id, started_at_ms } | Done { completion_blob, finished_at_ms } | Failed { reason, failed_at_ms } — CAS-transitioned via try_claim/try_complete/try_fail/try_repending helpers"
  - "try_claim helper accepts Pending OR stale Running (now_ms - started_at_ms > stale_after_ms, default 5 min) per RESEARCH Pitfall 5 — single CAS atomically swaps to Running { worker_id: caller, started_at_ms: now }; returns false on race-loss"
  - "BatchCoordinator::new(storage, queue, object_store, run_id) — BLOCKER 6 signature: explicit RunId parameter so the CLI controls run-ID lifecycle; .with_stale_after_ms(ms) builder; scan_and_enqueue(inputs, model_content_id, sampling) is idempotent across all four transition cases (missing/Pending/Failed enqueue; Done skip; fresh Running skip; stale Running CAS-re-Pending + enqueue); collect_done_records() scans the run namespace sorted by input_idx"
  - "BatchWorker::new(backend, storage, object_store, queue, run_id, worker_id, sampling) with pub async fn run_loop() -> Result<usize> + pub async fn run_one() -> Result<RunOutcome>; sequential per-task in Wave-2 (plan 03-03 parallelizes via futures::try_join_all once the live AsyncLLMEngine lands); backend errors absorbed via try_fail; queue items ack'd regardless of outcome"
  - "InferBatchConfig in crates/rollout-runtime-batch/src/config.rs (NOT in rollout-cli, per WARN-5 — the runtime owns the schema; the CLI imports it). Five blocks [model] [sampling] [input] [output] [workers] each with #[serde(deny_unknown_fields)] per spec 11; JsonSchema-derived for future schema-gen wiring"
  - "JsonlInput { id, prompt, #[serde(flatten)] extras } + JsonlOutput { id, prompt, completion, sampling_params, model_uri, finish_reason, model_content_id, completion_blob_id, generated_at, #[serde(flatten)] extras } — read_jsonl + write_jsonl use stdlib tokio::io::BufReader::lines() + serde_json::from_str (no serde_jsonlines dep)"
  - "MockBackend (gated by test-mock-backend Cargo feature) — deterministic 'MOCK:{prompt}' completions after a configurable sleep; impls InferenceBackend; used by worker_happy_path and (Wave-4) the restart-no-duplicates test; AGENTS.md §7 local-test parity holds with default features"
  - "rollout-storage gains an `infer` namespace registration (T_INFER table) + matching match arm in get_many_bytes — needed for `infer/<run>/samples/*` CAS calls; existing 02-02 namespaces unchanged"
  - "mdBook chapter docs/book/src/inference/batch-runtime.md (~190 lines): architecture diagram, SAMPLING_PARAMS_SCHEMA_VERSION rationale + RESEARCH Pitfall 1 link, CAS state-machine ASCII diagram, resume-semantics table (all four transition cases + 5-min default), MockBackend contract, InferBatchConfig TOML shape, cross-links to RESEARCH Pitfall 5 + specs 02 §2a / 04 §2 / 06 §3"
  - "17 integration tests green: 10 content_id_derivation + 3 cas_state_machine + 3 jsonl_roundtrip + 2 resume_skips_done + 2 worker_happy_path; cargo clippy --all-features --all-targets -- -D warnings clean; rustdoc gate clean"
affects: [03-03-vllm-async-engine, 03-04-cli-infer-batch, 03-05-smoke-docs-bench]

# Tech tracking
tech-stack:
  added:
    - "schemars 1.2 added to rollout-runtime-batch dependencies (workspace pin already in place) — needed for InferBatchConfig JsonSchema derive in config.rs; Wave-3 schema-gen will pick up the new types automatically"
  patterns:
    - "Wave-2 sequential generate loop: BatchWorker calls backend.generate(&[Prompt], &sampling) once per dequeued sample. Plan 03-03 will parallelize at the worker-spawn layer (N workers × 1-prompt batch) once vLLM's continuous batcher is alive. Identical correctness, batching-first principle retained — vLLM batches across the N concurrent requests regardless of how rollout shapes its dispatch."
    - "Schema-version byte as first hasher input (RESEARCH Pitfall 1). Bumping the constant invalidates outstanding sample-IDs by design — documented migration path: drain in-flight batch under v_n, deploy v_{n+1}, accept any orphans as re-runs."
    - "Stale-Running re-Pending CAS dance (RESEARCH Pitfall 5). The CAS expected = exact stored bytes, new = Pending bytes; two racing coordinators can't double-enqueue because the second CAS observes a Pending value, not the original Running. The 5-minute default window threads the needle between SIGKILL-recovery and absorbing slow generations."
    - "input_idx carried on every SampleRecord (per WARN-1) — collect_done_records() returns rows sorted by input_idx so the CLI's output JSONL matches input order without a side-table mapping. Trade-off accepted: 8 bytes/sample of redb storage to avoid a parallel B-tree."
    - "BatchCoordinator and BatchWorker share Arc<dyn Storage> + Arc<dyn Queue> + Arc<dyn ObjectStore> + RunId + SamplingParams; multiple workers compose against the same coordinator-owned queue and the CAS state machine guarantees at-most-once Done per sample."

key-files:
  created:
    - "crates/rollout-runtime-batch/src/state.rs — SAMPLING_PARAMS_SCHEMA_VERSION constant + SampleRecord/SampleState + sample_id() + sample_key() + try_claim/try_complete/try_fail/try_repending CAS helpers + DEFAULT_STALE_AFTER_MS"
    - "crates/rollout-runtime-batch/src/coordinator.rs — BatchCoordinator::new(storage, queue, object_store, run_id) + scan_and_enqueue + collect_done_records + load_sample + InputItem struct + now_ms() helper"
    - "crates/rollout-runtime-batch/src/worker.rs — BatchWorker::new + run_loop + run_one + RunOutcome enum + parse_sample_id helper"
    - "crates/rollout-runtime-batch/src/config.rs — InferBatchConfig + InputBlock + OutputBlock + WorkersBlock (all deny_unknown_fields + JsonSchema)"
    - "crates/rollout-runtime-batch/src/io.rs — JsonlInput + JsonlOutput + read_jsonl + write_jsonl (serde flatten preserves extras)"
    - "crates/rollout-runtime-batch/src/mock_backend.rs — test-mock-backend-gated MockBackend impl InferenceBackend"
    - "crates/rollout-runtime-batch/tests/content_id_derivation.rs — 10 tests (locked hex + schema-version byte regression + 1 schema-version value lock + 1 determinism + 6 proptest property tests over 256 cases each)"
    - "crates/rollout-runtime-batch/tests/cas_state_machine.rs — 3 tests (Pending→Running→Done round-trip via EmbeddedStorage; fresh Running rejects claim; stale Running allows reclaim)"
    - "crates/rollout-runtime-batch/tests/jsonl_roundtrip.rs — 3 tests (read preserves extras; write round-trips extras; full input → output preserves arbitrary fields)"
    - "crates/rollout-runtime-batch/tests/resume_skips_done.rs — 2 tests (5-record scan: 2 Done skipped + 2 Pending enqueued + 1 stale Running re-Pending'd → exactly 3 enqueued; idempotency on second scan)"
    - "crates/rollout-runtime-batch/tests/worker_happy_path.rs — 2 tests (3-sample MockBackend run via FsObjectStore + InMemQueue + EmbeddedStorage; Done-already-queued is acked-and-skipped without re-CAS)"
    - "docs/book/src/inference/batch-runtime.md — mdBook chapter (~190 lines)"
  modified:
    - "crates/rollout-runtime-batch/Cargo.toml — added schemars workspace dep (for InferBatchConfig JsonSchema)"
    - "crates/rollout-runtime-batch/src/lib.rs — promoted from //!-only stub to module-declaring lib (config/coordinator/io/state/worker public mods + mock_backend gated); re-exports BatchCoordinator/BatchWorker/InputItem/RunOutcome/InferBatchConfig/InputBlock/OutputBlock/WorkersBlock/JsonlInput/JsonlOutput/read_jsonl/write_jsonl/SampleRecord/SampleState/sample_id/sample_key/try_*/SAMPLING_PARAMS_SCHEMA_VERSION/DEFAULT_STALE_AFTER_MS"
    - "crates/rollout-storage/src/embedded/tables.rs — added T_INFER table + 'infer' arm in table_for() + all_tables() now returns 7 entries"
    - "crates/rollout-storage/src/embedded/mod.rs — added 'infer' static-ns match arm in get_many_bytes (parity with table_for)"
    - "docs/book/src/SUMMARY.md — nested batch-runtime.md under existing Inference heading after vllm-backend.md"
    - "Cargo.lock — picked up the schemars dep activation"

key-decisions:
  - "[Claude / Rule 3 — registered new storage namespace] rollout-storage's table_for() only knew six Phase-2 namespaces; adding 'infer' as the seventh is mechanical (one TableDefinition constant + one match arm in tables.rs + one match arm in get_many_bytes). The alternative — re-using the 'queue' namespace — would conflate sample-state with queue-spill and violate the namespace-per-subsystem invariant. The change is additive (existing tables untouched); rollout-storage's 18 tests stayed green without modification."
  - "[Plan rationale per WARN-1] SampleRecord carries input_idx: u64 inline rather than in a side table — collect_done_records() needs to sort the Done set by input ordinal at output-emission time, and a side B-tree just to map sid → idx would double the storage I/O. 8 bytes per sample is a rounding error against the prompt + completion payload."
  - "[Plan rationale per WARN-3] worker_happy_path tests live in their own tests/worker_happy_path.rs file (NOT embedded in cas_state_machine.rs). Keeps the CAS-state-machine test focused on the storage-transition contract (one EmbeddedStorage + manual postcard writes) while the worker test exercises the full pull-loop (EmbeddedStorage + InMemQueue + FsObjectStore + MockBackend + run_loop). Each test file owns a single concern."
  - "[Plan rationale per WARN-5] InferBatchConfig lives in rollout-runtime-batch, NOT in rollout-cli. The runtime owns the schema; the CLI imports it via `use rollout_runtime_batch::InferBatchConfig;`. This preserves the dependency direction (rollout-cli depends on rollout-runtime-batch in Wave 3, never the inverse) and lets the schema-gen pipeline pick up the type from the runtime crate."
  - "[Plan rationale] BatchWorker captures SamplingParams in its constructor rather than reading it from each SampleRecord postcard payload — the run config is single-sourced (one SamplingParams per run; Phase 3 doesn't support per-sample param overrides). Reading params from the row would force the worker to serde-deserialize on every dequeue for no semantic benefit."
  - "[Plan rationale] BatchWorker.run_one() ack's the queue item BOTH on Completed and on Failed paths (and on Skipped paths where the row was already terminal or the CAS lost). Rationale: Failed rows are re-enqueued by the next BatchCoordinator::scan_and_enqueue call (the resume contract). Leaving the queue item un-ack'd would block other queue consumers behind a dead sample."
  - "[Claude / Rule 1 — fixed clippy::cast_sign_loss in resume_skips_done.rs] Test used `i as u64` where i was an enumerate-driven usize — switched the loop range to `0u64..2` so the value is already typed correctly. Same shape (i as u64) appears safely elsewhere because it's bounded by enumerate over inputs.len() which fits in u64."

patterns-established:
  - "Wave-2 sequential generate loop. BatchWorker dispatches one prompt per backend.generate() call in Wave 2. Plan 03-03 will not change BatchWorker — it will spawn N workers in parallel from the CLI side (each pulling from the shared queue) so vLLM's continuous batcher sees N concurrent requests. This means the runtime-side change for Wave 3 is zero; the CLI orchestrates concurrency."
  - "Schema-version byte as first hasher input. Every future content-addressed ID derivation in rollout (e.g., snapshot IDs in Phase 4, episodic-memory keys in Phase 8) should prepend a schema-version byte to its postcard-encoded input. The pattern is documented in the batch-runtime mdBook chapter for downstream phases to reference."
  - "Storage namespace registration as a one-PR addition. The pattern is now established: add a TableDefinition constant in rollout-storage/src/embedded/tables.rs, add one arm to table_for(), update all_tables() count + match arm in get_many_bytes, run the existing 18 storage tests to confirm no regression. Phase 4 (training-state snapshots) and Phase 8 (episodic memory) will both add namespaces; the contract is now mechanical."

# Known stubs
known_stubs:
  - "BatchWorker.run_one() is sequential — one prompt per backend.generate() — by design in Wave 2. Plan 03-03 will not change BatchWorker; instead the CLI (Wave 3 / plan 03-04) will spawn N workers in parallel so vLLM's continuous batcher sees N concurrent generate() calls. The crate is forward-compatible; no API breaking change needed."
  - "BatchCoordinator does not yet plumb model-content-id derivation (e.g., resolving a HF repo SHA via huggingface_hub). The caller passes model_content_id: ContentId explicitly today; the CLI (Wave 3 / plan 03-04) will compute it via the vLLM backend's model_id() after init. Documented in the batch-runtime mdBook chapter."
  - "InferBatchConfig is shipped but not yet validated end-to-end. Plan-time validation (sampling.stream == false rejection per D-BACKEND-03; HF_TOKEN allowlist check per RESEARCH Pitfall 10; glob resolution to ≥1 file) lands in plan 03-04 as part of the rollout-cli `infer batch` subcommand. The runtime crate provides the schema; the CLI owns the validation."
  - "plan.rs (per the action sketch) was NOT shipped in Task 2 — plan-time validation is a Wave-3 concern (plan 03-04 wires it from the CLI side; glob crate dep + secret-store probe + dry-run flag all land there). The runtime crate stays validation-free; this keeps it usable from a library context too (e.g., embedded tests)."

# Authentication gates / preflight notes
preflight_note: "No new external service auth required. The crate's full test suite (`cargo test -p rollout-runtime-batch --features test-mock-backend --tests`) runs with default rollout-runtime-batch deps + tempfile + no Python / vLLM dependency — MockBackend's `tokio::time::sleep` is the only async operation outside the substrate stack. The HF_TOKEN preflight (RESEARCH Pitfall 10) is plan 03-03's territory (live vLLM init); the runtime crate never reads secrets directly."

requirements-completed: [BACKEND-02, DOCS-01, DOCS-02, DOCS-03]

# Metrics
duration: 16min
completed: 2026-05-20
---

# Phase 3 Plan 02: rollout-runtime-batch Summary

**One-liner:** CAS sample-state machine (`SampleRecord` + `SampleState` over `infer/<run>/samples/*` redb namespace) with `SAMPLING_PARAMS_SCHEMA_VERSION`-prepended `sample_id()` deterministic derivation + `BatchCoordinator::scan_and_enqueue` (idempotent across Pending / Failed / fresh-Running-skip / stale-Running-re-Pending) + `BatchWorker::run_loop` pull/CAS/generate/blob-write loop + `InferBatchConfig` TOML schema (lives upstream of CLI per WARN-5) + JSONL I/O with extras preservation + test-only `MockBackend` (deterministic `MOCK:{prompt}`) + mdBook chapter. 17 tests green; clippy + rustdoc gates clean; storage tests + workspace tests unbroken.

## Performance

- **Duration:** 16 min
- **Started:** 2026-05-20T22:05:58Z
- **Completed:** 2026-05-20T22:21:25Z
- **Tasks:** 2 (per the plan's `<tasks>` block; both `tdd="true"`)
- **Files modified:** 17 (12 created + 5 modified)

## Accomplishments

### Task 1: SampleRecord + sample_id derivation + CAS state machine + MockBackend

- `SAMPLING_PARAMS_SCHEMA_VERSION: u8 = 1` constant lives at `crates/rollout-runtime-batch/src/state.rs` and is **the first byte fed to the blake3 hasher in `sample_id()`** per RESEARCH Pitfall 1. Bumping it invalidates outstanding `Pending` / `Running` sample-IDs by design — documented migration path.
- `SampleRecord { id, prompt_blob, state, created_at_ms, input_idx }` + `SampleState { Pending | Running { worker_id, started_at_ms } | Done { completion_blob, finished_at_ms } | Failed { reason, failed_at_ms } }` postcard-encoded under `StorageKey { namespace: "infer", run_id: Some(run), path: ["samples", sid_hex] }`.
- CAS helpers (`try_claim` / `try_complete` / `try_fail` / `try_repending`) accept a `&mut Box<dyn StorageTxn>` so the caller owns commit/abort. `try_claim` accepts `Pending` OR stale `Running` (now_ms − started_at_ms > stale_after_ms; default 5 min) per RESEARCH Pitfall 5.
- `MockBackend` (gated by `test-mock-backend`) returns `"MOCK:{prompt}"` after a configurable sleep — drives `worker_happy_path` and (in Wave 4) the restart-no-duplicates test without Python.
- Registered the `infer` namespace in `rollout-storage::embedded::tables` (added `T_INFER` + arm in `table_for()` + arm in `get_many_bytes`). No regression — 18 existing storage tests stay green.
- 10 `content_id_derivation` tests: locked-hex regression (`aed11582cbb156d052a7ad784812a87794c76569c734a73999b63394ab460f2c`); schema-version byte first; schema-version value = 1; 1 determinism; 6 proptest property tests at 256 cases each covering `prompt`, `idx`, `model_content_id`, `temperature`, `max_tokens`, `seed`.
- 3 `cas_state_machine` tests: Pending → Running → Done round-trip with second-claim CAS-loss; fresh Running rejects claim; stale Running allows re-claim.

### Task 2: BatchCoordinator + BatchWorker + InferBatchConfig + JSONL I/O + mdBook

- `BatchCoordinator::new(storage, queue, object_store, run_id)` — BLOCKER 6 explicit `RunId` parameter; `.with_stale_after_ms(ms)` builder for test overrides.
- `scan_and_enqueue(inputs, model_content_id, sampling) -> Result<usize>` is **idempotent** across all four transition cases:
  - missing → write `Pending` + put prompt-blob + enqueue
  - `Pending` / `Failed` → enqueue
  - fresh `Running` → skip (live owner)
  - stale `Running` → CAS `Running → Pending` (re-Pending) + enqueue on success
  - `Done` → skip
- `collect_done_records()` scans the run namespace, filters `Done`, sorts by `input_idx`.
- `BatchWorker::new(backend, storage, object_store, queue, run_id, worker_id, sampling)` with `pub async fn run_loop() -> Result<usize>` (returns Completed count) and `pub async fn run_one() -> Result<RunOutcome>`.
- `RunOutcome { Completed | Failed | Skipped | Drained }`. Backend errors absorbed via `try_fail`; queue items ack'd on every non-`Drained` outcome.
- `InferBatchConfig` in `crates/rollout-runtime-batch/src/config.rs` (NOT in `rollout-cli`, per WARN-5). Five blocks `[model] / [sampling] / [input] / [output] / [workers]` with `#[serde(deny_unknown_fields)]` per spec 11; `JsonSchema`-derived for Wave-3 schema-gen.
- `JsonlInput { id, prompt, #[serde(flatten)] extras }` + `JsonlOutput { id, prompt, completion, sampling_params, model_uri, finish_reason, model_content_id, completion_blob_id, generated_at, #[serde(flatten)] extras }`. `read_jsonl` / `write_jsonl` use stdlib `tokio::io::BufReader::lines()` + `serde_json::from_str` — no `serde_jsonlines` dep per RESEARCH.
- 3 `jsonl_roundtrip` tests (read preserves extras / write round-trips extras / full input→output round-trip).
- 2 `resume_skips_done` tests (5-record scan: 2 Done + 2 Pending + 1 stale Running → exactly 3 enqueued + stale Running re-Pending'd verified; second scan is idempotent on Pending re-enqueue).
- 2 `worker_happy_path` tests (3-sample MockBackend run via `FsObjectStore` + `InMemQueue` + `EmbeddedStorage` end-to-end; Done-already-queued is acked-and-skipped without re-CAS).
- mdBook chapter `docs/book/src/inference/batch-runtime.md` (~190 lines): architecture diagram, schema-version byte rationale, CAS state-machine ASCII diagram, resume-semantics table, MockBackend contract, `InferBatchConfig` TOML shape, cross-links. Wired under the existing `Inference` heading in `docs/book/src/SUMMARY.md` after the vllm-backend chapter.

## Task Commits

1. **Task 1** — `a94756f` (`feat`): `feat(03-02): CAS state machine + sample_id with schema-version byte`
2. **Task 2** — `a43c436` (`feat`): `feat(03-02): BatchCoordinator + BatchWorker + InferBatchConfig + JSONL I/O + mdBook`

## Files Created/Modified

### Created (12)
- `crates/rollout-runtime-batch/src/state.rs`
- `crates/rollout-runtime-batch/src/coordinator.rs`
- `crates/rollout-runtime-batch/src/worker.rs`
- `crates/rollout-runtime-batch/src/config.rs`
- `crates/rollout-runtime-batch/src/io.rs`
- `crates/rollout-runtime-batch/src/mock_backend.rs`
- `crates/rollout-runtime-batch/tests/content_id_derivation.rs`
- `crates/rollout-runtime-batch/tests/cas_state_machine.rs`
- `crates/rollout-runtime-batch/tests/jsonl_roundtrip.rs`
- `crates/rollout-runtime-batch/tests/resume_skips_done.rs`
- `crates/rollout-runtime-batch/tests/worker_happy_path.rs`
- `docs/book/src/inference/batch-runtime.md`

### Modified (5)
- `crates/rollout-runtime-batch/Cargo.toml` (+ schemars workspace dep)
- `crates/rollout-runtime-batch/src/lib.rs` (module declarations + re-exports)
- `crates/rollout-storage/src/embedded/tables.rs` (+ T_INFER + infer arm)
- `crates/rollout-storage/src/embedded/mod.rs` (+ infer arm in get_many_bytes)
- `docs/book/src/SUMMARY.md` (batch-runtime entry under Inference)
- `Cargo.lock` (schemars activation)

## Decisions Made

- **Storage-namespace registration**: extending `rollout-storage::embedded::tables` is the established pattern for per-subsystem KV namespaces (recorded above under `patterns-established`). Phase 4 + Phase 8 will both follow.
- **`input_idx` lives inline on `SampleRecord`** per WARN-1 — saves a side-table for the sort-by-input-order requirement; 8 bytes/sample is a rounding error.
- **`worker_happy_path` extracted to its own file** per WARN-3 — separates storage-transition tests from end-to-end pull-loop tests.
- **`InferBatchConfig` lives in the runtime crate** per WARN-5 — upstream of `rollout-cli`; dep-direction preserved.
- **`BatchWorker` captures `SamplingParams` at construction** — single-sourced config; no per-sample override path in Phase 3.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 — Blocking issue] `infer` namespace not registered in `rollout-storage`.**
- **Found during:** Task 1, first run of `cas_state_machine.rs`.
- **Issue:** `EmbeddedStorage::begin().await?.cas_bytes(StorageKey { namespace: "infer", ... }, ...).await?` would fail with `Fatal(ConfigInvalid { msg: "unknown storage namespace: infer" })` because `rollout-storage::embedded::tables::table_for` only knew the six Phase-2 namespaces.
- **Fix:** Added `T_INFER` `TableDefinition` constant + `"infer"` match arm in `table_for()` + matching arm in `embedded::mod.rs::get_many_bytes` + updated `all_tables()` to return 7 entries (was 6).
- **Files modified:** `crates/rollout-storage/src/embedded/tables.rs`, `crates/rollout-storage/src/embedded/mod.rs`.
- **Verification:** `cargo test -p rollout-storage --tests` reports 18 passing (no regression). `cas_state_machine.rs` then ran green.
- **Committed in:** `a94756f` (Task 1).

**2. [Rule 1 — Bug] `clippy::missing_panics_doc` on every CAS helper.**
- **Found during:** Task 1 clippy `--all-features`.
- **Issue:** `try_claim` / `try_complete` / `try_fail` / `try_repending` each called `postcard::to_stdvec(record).expect(...)`. clippy `missing_panics_doc` (pedantic) requires a `# Panics` section on any `pub fn` whose body contains a reachable `.expect(...)` / `.unwrap(...)`. The naive fix (add `# Panics` docs) is correct but verbose.
- **Fix:** Introduced a private `encode_record(rec) -> Result<Vec<u8>, CoreError>` helper that returns `Fatal(Internal)` on postcard encode failure (theoretical — `SampleRecord` is a plain struct of primitives) and re-routed all four CAS helpers through it. Eliminates the panic-doc lint AND threads the (impossible-in-practice) error case correctly. Same pattern Phase 2's `EmbeddedTxn` uses for `internal()`.
- **Files modified:** `crates/rollout-runtime-batch/src/state.rs`.
- **Verification:** `cargo clippy -p rollout-runtime-batch --all-features --all-targets -- -D warnings` exits 0.
- **Committed in:** `a94756f` (Task 1).

**3. [Rule 1 — Bug] `clippy::cast_possible_truncation` on `p.0.len() as u32` in `MockBackend`.**
- **Found during:** Task 1 clippy `--features test-mock-backend`.
- **Issue:** `completion_tokens: p.0.len() as u32` is a `usize → u32` cast that's lossy on 64-bit pointers.
- **Fix:** Switched to `u32::try_from(p.0.len()).unwrap_or(u32::MAX)`. Same shape Phase 2 used for sigterm-PID conversion (plan 02-05).
- **Files modified:** `crates/rollout-runtime-batch/src/mock_backend.rs`.
- **Verification:** Clippy clean.
- **Committed in:** `a94756f` (Task 1).

**4. [Rule 1 — Bug] `clippy::doc_markdown` on `MockBackend` / `FsObjectStore` / `SampleRecord` / `ContentId` / `SCHEMA_VERSION` in module / file `//!` doc comments.**
- **Found during:** Tasks 1 + 2 clippy.
- **Issue:** clippy::doc_markdown (pedantic, workspace-default) requires backticks around unknown CamelCase / all-caps identifiers in rustdoc. Hit five sites: `lib.rs` (MockBackend); `worker.rs` (sample_id); `worker_happy_path.rs` (MockBackend + FsObjectStore + SampleRecord + ContentId); `content_id_derivation.rs` (SCHEMA_VERSION).
- **Fix:** Wrapped all in backticks. Same shape Phase 2 + plans 03-00/03-01 hit repeatedly.
- **Files modified:** `crates/rollout-runtime-batch/src/lib.rs`, `crates/rollout-runtime-batch/src/worker.rs`, `crates/rollout-runtime-batch/tests/worker_happy_path.rs`, `crates/rollout-runtime-batch/tests/content_id_derivation.rs`.
- **Verification:** Clippy clean under both `--features test-mock-backend` and `--all-features --all-targets`.
- **Committed in:** `a94756f` (Task 1) + `a43c436` (Task 2).

**5. [Rule 1 — Bug] `unused_import: JsonlInput` in `jsonl_roundtrip.rs`.**
- **Found during:** Task 2 clippy `--all-features`.
- **Issue:** The roundtrip test imports `JsonlInput` from re-exports but reads `JsonlInput` rows via `read_jsonl`'s return type — no explicit construction of `JsonlInput` in the test body, so the import is unused. `-D unused-imports` errors.
- **Fix:** Dropped `JsonlInput` from the use line; kept `read_jsonl` + `write_jsonl` + `JsonlOutput` (constructed explicitly).
- **Files modified:** `crates/rollout-runtime-batch/tests/jsonl_roundtrip.rs`.
- **Verification:** Clippy clean.
- **Committed in:** `a43c436` (Task 2).

**6. [Rule 1 — Bug] `clippy::cast_sign_loss` on `i as u64` in `resume_skips_done.rs`.**
- **Found during:** Task 2 clippy `--all-features`.
- **Issue:** Test used `for i in 0..2 { ... i as u64 ... }` — `i` defaulted to `i32`, and `as u64` is sign-loss-eligible.
- **Fix:** Switched the loop range to `0u64..2` so `i` is already `u64`.
- **Files modified:** `crates/rollout-runtime-batch/tests/resume_skips_done.rs`.
- **Verification:** Clippy clean.
- **Committed in:** `a43c436` (Task 2).

**7. [Rule 3 — Blocking issue] `worker_happy_path` test failed to compile without `test-mock-backend` feature.**
- **Found during:** Task 2 `cargo test --workspace --tests` (default-features pass).
- **Issue:** `tests/worker_happy_path.rs` imports `rollout_runtime_batch::MockBackend`, but the symbol is gated behind `#[cfg(feature = "test-mock-backend")]` in `lib.rs`. The workspace-wide `cargo test --workspace --tests` doesn't enable the per-crate feature, so the test file failed to compile.
- **Fix:** Added `#![cfg(feature = "test-mock-backend")]` at the top of `tests/worker_happy_path.rs`. The other tests don't depend on MockBackend so they compile under default features.
- **Files modified:** `crates/rollout-runtime-batch/tests/worker_happy_path.rs`.
- **Verification:** `cargo test --workspace --tests` (default features) compiles + green; `cargo test -p rollout-runtime-batch --features test-mock-backend --tests` still runs all 5 test files (10 + 3 + 3 + 2 + 2 = 20 tests).
- **Committed in:** `a43c436` (Task 2).

### Rule-4 (architectural) deviations

None. All work stayed within the runtime-glue scope sanctioned by the plan's `<tasks>` block. The one architectural touch — adding the `infer` namespace to `rollout-storage` — is the established pattern for cross-subsystem KV namespaces (Phase 2 already registered 6 of them); not a Rule-4 design change.

## End-to-end Verification

All commands exited 0:

```
cargo build -p rollout-runtime-batch                                      # default features
cargo build -p rollout-runtime-batch --features test-mock-backend         # feature build
cargo test  -p rollout-runtime-batch --features test-mock-backend --tests # 20 tests across 5 files
cargo clippy -p rollout-runtime-batch --all-features --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" \
  cargo doc -p rollout-runtime-batch --no-deps --features test-mock-backend
cargo test --workspace --tests                                            # no regression
cargo build --workspace                                                   # no regression
mdbook build docs/book                                                    # batch-runtime chapter rendered
cargo deny check                                                          # advisories/bans/licenses/sources ok
bash scripts/check-docs-tests-touched.sh                                  # BASE_SHA=HEAD~2 HEAD_SHA=HEAD
```

## User Setup Required

None — no external service configuration. The full integration test path (`cargo test -p rollout-runtime-batch --features test-mock-backend --tests`) runs with stdlib + tempfile + tokio + redb + rollout-cloud-local + MockBackend; zero Python, zero vLLM, zero cloud creds. AGENTS.md §7 honored.

## Next Phase Readiness

- **Plan 03-03 (Wave 3 — vLLM AsyncLLMEngine wiring):** entry point is `crates/rollout-backend-vllm/src/engine.rs::worker_main_vllm`'s `VllmTask::Generate` arm. Plan 03-02 does not touch that file; the `Arc<dyn InferenceBackend>` BatchWorker composes will pick up `VllmBackend` instead of `MockBackend` once 03-03 swaps the `Generate` arm body. Zero churn expected in this crate.
- **Plan 03-04 (Wave 3 — `rollout-cli infer batch`):** consumes `InferBatchConfig` via `use rollout_runtime_batch::InferBatchConfig;`; constructs `BatchCoordinator` + N × `BatchWorker` with `Arc<dyn InferenceBackend>` resolved from `VllmBackend::new(engine_id)`; spawns workers in parallel via `tokio::spawn` so vLLM's continuous batcher sees N concurrent generate() calls. The runtime-side surface needed by the CLI is fully shipped here.
- **Plan 03-05 (Wave 4 — restart-no-duplicates + bench + mdBook):** the restart test uses `MockBackend::new(50)` (50 ms per sample) + `BatchWorker::run_loop` + a `tokio::process::Command` SIGKILL helper. 03-02 has shipped both ingredients; 03-05's diff is the test driver + smoke shell script + the bench crate touches.
- **Open question (deferred):** plan-time validation (`InferBatchConfig::validate(&cfg, &secret_store)` per the plan's `plan.rs` sketch) intentionally not shipped here — Wave-3 territory (CLI-side). Adding it to the runtime crate would pull `glob` + `SecretStore` consumer code into a crate that shouldn't know about either. The current `InferBatchConfig` is a pure schema.

## Self-Check: PASSED

- crates/rollout-runtime-batch/src/state.rs — FOUND
  - `pub const SAMPLING_PARAMS_SCHEMA_VERSION: u8 = 1` ✓
  - `h.update(&[SAMPLING_PARAMS_SCHEMA_VERSION])` ✓
  - `pub fn sample_id` ✓
  - `pub enum SampleState` ✓
- crates/rollout-runtime-batch/src/coordinator.rs — FOUND
  - `pub struct BatchCoordinator` ✓
  - `pub fn new` (with `run_id`) ✓
  - `stale_after_ms` ✓
- crates/rollout-runtime-batch/src/worker.rs — FOUND
  - `pub struct BatchWorker` ✓
  - `pub async fn run_loop` ✓
- crates/rollout-runtime-batch/src/config.rs — FOUND
  - `pub struct InferBatchConfig` ✓
- crates/rollout-runtime-batch/src/io.rs — FOUND
  - `#[serde(flatten)]` ✓ (on both JsonlInput.extras and JsonlOutput.extras)
- crates/rollout-runtime-batch/src/mock_backend.rs — FOUND
  - `pub struct MockBackend` ✓
- crates/rollout-runtime-batch/tests/worker_happy_path.rs — FOUND (extracted per WARN-3)
- docs/book/src/inference/batch-runtime.md — FOUND
- docs/book/src/SUMMARY.md — contains `inference/batch-runtime.md` ✓
- crates/rollout-runtime-batch/Cargo.toml — `test-mock-backend` feature ✓
- Commits `a94756f` + `a43c436` — both present in `git log --oneline -5`

---
*Phase: 03-inference-batch*
*Completed: 2026-05-20*
