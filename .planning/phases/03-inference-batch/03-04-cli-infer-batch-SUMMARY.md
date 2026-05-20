---
phase: 03-inference-batch
plan: 04
subsystem: cli-infer-batch
tags: [rollout-cli, infer-batch, clap-subcommand, run-id-lifecycle, dry-run, run-pool, glob-inputs, batch-coordinator, batch-worker, mdbook, vllm-feature, test-mock-backend-feature]

# Dependency graph
requires:
  - phase: 02-local-substrate
    provides: "EmbeddedStorage (02-02), FsObjectStore + InMemQueue (02-03), EnvSecretStore (02-03)"
  - phase: 03-inference-batch
    provides: "InferBatchConfig + BatchCoordinator + BatchWorker + MockBackend re-exports from 03-02; VllmBackend::with_secret_token + content-addressed model_id from 03-03"
provides:
  - "`rollout infer batch --config <toml> [--resume <run_id>] [--workers N] [--dry-run]` subcommand: full clap surface mounted on the existing Infer command group on rollout-cli/src/main.rs"
  - "Backend selection helper `select_backend(&ModelRef, Option<String>) -> Arc<dyn InferenceBackend>`: precedence (1) `test-mock-backend` feature + ROLLOUT_TEST_MOCK_BACKEND=1 → MockBackend, (2) `vllm` feature → VllmBackend::with_secret_token, (3) neither → Fatal(ConfigInvalid). All three feature combinations build clean."
  - "`run_id` lifecycle (BLOCKER 6): --resume <id> > <output.dir>/run-id (single-line UTF-8 ULID) > freshly minted RunId(Ulid::new()) persisted atomically via tempfile + rename. Plan 03-05's restart_no_duplicates test reads the run-id file between phases."
  - "`run_pool(args, cfg, backend, inputs, workers)` implementation: creates output dir + EmbeddedStorage + FsObjectStore + InMemQueue, resolves run_id, runs BatchCoordinator::scan_and_enqueue, spawns workers via tokio::task::JoinSet, awaits drain, calls collect_done_records, emits completions.jsonl sorted by input_idx via rollout_runtime_batch::write_jsonl."
  - "`glob_inputs(pattern)`: literal-path fallback when pattern contains no glob meta-chars; otherwise glob::glob(pattern) sorted lexicographically; reads each file via rollout_runtime_batch::read_jsonl; assigns deterministic input_idx by file order + line order."
  - "Plan-time validation: deny_unknown_fields (via the runtime crate's InferBatchConfig schema), sampling.stream rejection with `\"Phase 8\"` substring, sampling.max_tokens > 0, workers.count >= 1, input.glob non-empty, glob resolves to >= 1 file."
  - "`--dry-run` short-circuits before backend construction: parses TOML, validates schema, probes input glob (reads JSONL fully), reads ROLLOUT_SECRET_HF_TOKEN best-effort for gated prefixes (meta-llama/, mistralai/), prints `dry-run OK: model=… inputs=… workers=…` to stdout. Works on builds with NO backend feature."
  - "Two CLI Cargo features: `vllm = [\"rollout-backend-vllm/vllm\"]` (production) and `test-mock-backend = [\"rollout-runtime-batch/test-mock-backend\"]` (plan 03-05 restart test). Independent — both can be enabled simultaneously."
  - "infer_config::load_from_file: thin TOML loader returning rollout_runtime_batch::InferBatchConfig (the schema lives upstream per WARN-5). Maps I/O + parse errors to Fatal(ConfigInvalid)."
  - "Tracing wiring honors RUST_LOG via the pre-existing init_tracing() from main.rs; structured events on `tracing` for run_id, enqueued, total, completed, and per-worker spawn lines."
  - "mdBook chapter docs/book/src/inference/cli.md (~140 lines): invocation, TOML schema reference, JSONL contracts, run_id lifecycle table, dry-run semantics, backend selection table, observability, exit codes, CPU-mode caveat. Linked from docs/book/src/SUMMARY.md under Inference."
  - "5 dry-run integration tests (infer_dry_run.rs): happy-path exit 0, streaming-rejection with `Phase 8` in stderr, workers.count=0 rejection, unknown-toml-field rejection (deny_unknown_fields surface), no-matching-input-files rejection. Plus 2 help-parses tests (cli_help.rs): `infer batch --help` lists all 4 flags; `infer --help` lists the `batch` subcommand. Plus 2 unit tests for the RFC3339-ish UTC formatter (epoch + 2024-01-01)."
affects: [03-05-smoke-docs-bench]

# Tech tracking
tech-stack:
  added:
    - "glob 0.3 (new CLI dep) — `glob::glob(pattern)` for the `[input].glob` field; literal-path bypass when no meta-chars are present so single-file configs don't need wildcards."
    - "Direct path deps from rollout-cli to rollout-runtime-batch, rollout-backend-vllm, rollout-cloud-local. Validated against rollout-core/tests/dependency_direction.rs — rollout-cli is not in ALGO_AND_ABOVE so the dep-direction invariants stay green (7/7 tests pass)."
    - "async-trait workspace dep added to rollout-cli (re-used elsewhere; harmless duplicate)."
    - "tokio::task::JoinSet for the worker pool — composes naturally with Arc-shared substrate deps + a bounded join loop."
  patterns:
    - "Backend selection via #[cfg(feature = ...)] chains with explicit early-returns under `test-mock-backend` and a final `#[cfg(not(feature = \"vllm\"))]` fast-fail. The `cfg(not(feature = \"vllm\"))` branch is the only one that doesn't `return` — it produces the final expression of the function. This avoids the `if-without-else` error E0317 when only `test-mock-backend` is enabled."
    - "Per-run `<output.dir>/run-id` file as the source-of-truth for implicit resume. Atomic write (tempfile + rename) so a crash between mint and persist never leaves a corrupt half-written file. Plan 03-05 reads this file between SIGKILL and restart to obtain the ULID for the explicit --resume override."
    - "RFC3339-ish UTC formatter without chrono/jiff/time deps — Howard Hinnant's civil_from_days algorithm in 12 lines + a small wrapper. Sufficient for `generated_at` JSONL output field where downstream consumers parse with their own tooling. Unit-tested for epoch + 2024-01-01."
    - "Async function `select_backend` carries `#[allow(clippy::unused_async)]` because under default-features (no backend) it doesn't `await` anything. Removing `async` would force the call-site to fork its await — keeping it async preserves a uniform interface across all feature combinations."

key-files:
  created:
    - "crates/rollout-cli/src/infer.rs (~350 lines): InferCmd/InferSub/BatchArgs clap structs; run_infer_batch entry; validate(); glob_inputs(); read_hf_token_if_required(); select_backend(); run_pool(); resolve_run_id(); format_unix_ms() + civil_from_days(); unit tests for the formatter."
    - "crates/rollout-cli/src/infer_config.rs: thin TOML loader using rollout_runtime_batch::InferBatchConfig. Maps I/O + serde errors to Fatal(ConfigInvalid). 22 lines."
    - "crates/rollout-cli/tests/infer_dry_run.rs (~190 lines): 5 dry-run tests covering happy-path, streaming-rejection, workers=0 rejection, unknown-field rejection, missing-input rejection."
    - "docs/book/src/inference/cli.md (~140 lines): full CLI chapter with TOML schema reference, run_id lifecycle, dry-run semantics, backend selection, observability."
  modified:
    - "crates/rollout-cli/Cargo.toml: added [features] block (default/vllm/test-mock-backend); added rollout-runtime-batch + rollout-backend-vllm + rollout-cloud-local path deps; added glob 0.3 + async-trait workspace deps."
    - "crates/rollout-cli/src/main.rs: declared mod infer + mod infer_config; added Cmd::Infer(infer::InferCmd) variant; added infer_dispatch(cmd) helper that boots Tokio + runs `infer::run_infer_batch(&args).await`."
    - "crates/rollout-cli/tests/cli_help.rs: added infer_batch_help_parses() (asserts all 4 flags on stdout) and infer_top_level_help_lists_subcommand() (asserts `batch` is listed)."
    - "docs/book/src/SUMMARY.md: nested cli.md under Inference after batch-runtime.md (DOCS-01 trio)."
    - "Cargo.lock: glob v0.3.3 picked up as a new transitive."

key-decisions:
  - "[Plan rationale] InferBatchConfig stays in rollout-runtime-batch (per WARN-5 from plan 03-02). The CLI only adds a 22-line TOML loader wrapping `toml::from_str(text)` — no schema duplication, no upstream Cargo direction inversion. Schema-gen + Phase-4 callers consume the same single source of truth."
  - "[Plan rationale per BLOCKER 6] Three-tier run_id resolution (--resume > file > mint). The file is single-line UTF-8 Crockford ULID, written via tempfile + rename for atomicity. Implicit re-resume on the same `output.dir` is the dominant ergonomic — running the same command twice picks up where it left off without any extra flag."
  - "[Plan rationale per BLOCKER 4] glob_inputs uses literal-path fallback (no glob expansion) when `pattern` contains no `*` or `?`. Configs that point at a single file don't need to escape glob metacharacters; configs that use a wildcard get sorted lexicographic file order. Within-file ordering is preserved by JSONL line order; the resulting input_idx is the global deterministic ordinal."
  - "[Claude / Rule 1 - select_backend cfg flow] First implementation had `#[cfg(feature = \"test-mock-backend\")] { if env == 1 { ... return Ok(...); } }` followed by `#[cfg(feature = \"vllm\")] { ... return Ok(...); }` followed by `#[cfg(not(any(...)))]` else. Under `--features test-mock-backend` ALONE (no vllm), the function fell off the end without a return → E0317. Fix: restructured to bare `if` (no `{}` block) under `#[cfg(feature = \"test-mock-backend\")]`, made the vllm branch the final expression block returning Ok directly, and switched the fast-fail branch to `#[cfg(not(feature = \"vllm\"))]` which co-exists with test-mock-backend without conflict. All three feature combinations now compile."
  - "[Claude / Rule 1 - cast_sign_loss + similar_names in civil_from_days] Howard Hinnant's algorithm naturally uses single-letter math names (y/m/d/z/doy/doe/yoe/mp). Renamed binding scope to year_civil/year_final/month/day at the public-API boundary (the return tuple) while preserving the algorithm's standard form with explicit `#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation, clippy::similar_names)]` on the helper function. The cast `as i32`/`as u32` is correct: `civil_from_days` is only ever called with `days >= 0` (Unix epoch lower bound)."
  - "[Plan rationale] No `chrono`/`jiff`/`time` dep — they all carry a non-trivial transitive surface, and the JSONL `generated_at` field only needs to round-trip as an RFC3339-shaped UTC string. The 30-line Howard-Hinnant implementation is unit-tested and is the same shape Phase-4 callers will use if they add their own timestamp emitters before a real time-crate is workspace-pinned."
  - "[Plan rationale] `Arc<dyn InferenceBackend>` is NOT shut down explicitly at run_pool exit. Backend.shutdown() requires &mut, and the Arc may not be uniquely owned (workers cloned it). VllmBackend's Drop impl in engine.rs handles best-effort cleanup; MockBackend is a no-op. Future work: wrap behind Mutex<Arc<dyn ...>> if explicit shutdown becomes load-bearing."

patterns-established:
  - "CLI-side `[features]` block as the production knob. `--features vllm` selects the live engine path at build time; `--features test-mock-backend` is a parallel build that swaps the backend at runtime via env-var. This pattern composes with the runtime crate's own `test-mock-backend` feature without requiring the runtime to know about the CLI."
  - "Implicit-resume via per-output-dir state file (`<output.dir>/run-id`) — the same convention will likely apply to Phase 4 training runs (`<output.dir>/run-id` + `<output.dir>/snapshot-id`) and Phase 6 multi-node runs."
  - "TOML loader as a 22-line thin wrapper, schema lives upstream. Phase-4 (algorithm configs) and Phase-6 (multi-host configs) will likely follow the same shape: 1 module per `cargo run --schema`-producing config type, all in the owning runtime/algo crate, with rollout-cli supplying only a deserialization wrapper."

# Known stubs
known_stubs:
  - "Backend `shutdown()` is not called from run_pool — relies on VllmBackend's Drop best-effort cleanup. Acceptable for Phase 3 (process exits after batch completes); revisit if long-lived CLI processes need explicit teardown (likely Phase 8 online-inference)."
  - "`finish_reason` in the JSONL output row is hard-coded to `\"stop\"`. The MockBackend's Completion type doesn't carry a finish reason via the public crate API; plumbing the real per-sample finish_reason from VllmBackend through `try_complete` (storing it on `SampleState::Done`) is Phase 3 deferred — plan 03-05 (smoke + bench) may pick it up, or it lands when streaming surfaces in Phase 8 (INFER-01)."
  - "`completion_tokens` / `prompt_tokens` fields from Completion are not surfaced in the JSONL output. The schema lives in `rollout_runtime_batch::JsonlOutput` already; the wiring is one Result.pop()-and-store away if a future plan needs it for cost accounting."
  - "HF_TOKEN allowlist is hard-coded to two prefixes (meta-llama/, mistralai/). A future config-driven allowlist would replace `read_hf_token_if_required` with a SecretStore lookup. For Phase 3 this hits the documented gated-model paths and stays out of EnvSecretStore's allowlist enforcement."

# Authentication gates / preflight notes
preflight_note: "No new external service auth required for the default-features test suite. The full `cargo test -p rollout-cli --tests` runs without Python / vllm / HF_TOKEN — every test exercises the dry-run path. Plan 03-05's live smoke + restart_no_duplicates tests will require ROLLOUT_VLLM_AVAILABLE=1 (for the vllm path) and ROLLOUT_TEST_MOCK_BACKEND=1 (for the mock path with `--features test-mock-backend`)."

requirements-completed: [BACKEND-02, DOCS-01, DOCS-02, DOCS-03]

# Metrics
duration: 8min
completed: 2026-05-20
---

# Phase 3 Plan 04: cli-infer-batch Summary

**One-liner:** Ships `rollout infer batch --config <toml> [--resume <id>] [--workers N] [--dry-run]` — full clap surface, three-tier `run_id` lifecycle (--resume > `<output.dir>/run-id` file > minted ULID), backend selection across `vllm` and `test-mock-backend` Cargo features, `run_pool` wiring `BatchCoordinator` + N × `BatchWorker` against `Arc<dyn InferenceBackend>` over `EmbeddedStorage` + `FsObjectStore` + `InMemQueue`, deterministic JSONL output sorted by `input_idx`, plan-time validation rejecting streaming + zero-workers + unknown-field + empty-input configs, `--dry-run` short-circuit that never touches the backend, mdBook CLI chapter, and 9 new tests (5 dry-run + 2 help-parses + 2 unit). All three feature combinations build clean; clippy clean across default / `--features vllm` / `--all-features`.

## Performance

- **Duration:** ~8 min
- **Started:** 2026-05-20T23:29:00Z
- **Completed:** 2026-05-20T23:36:20Z
- **Tasks:** 1 (TDD: fixtures + tests + implementation shipped together)
- **Files modified:** 9 (4 created + 5 modified including Cargo.lock)

## Accomplishments

### Task 1: `rollout infer batch` clap surface + TOML config + dry-run + run_pool

- **Subcommand mounted:** `rollout infer batch` with `--config <PATH>`, `--resume <RUN_ID>`, `--workers <N>`, `--dry-run` flags. `rollout infer --help` lists the `batch` subcommand; `rollout infer batch --help` shows all four flags.
- **Backend selection:** runtime precedence (1) `--features test-mock-backend` + `ROLLOUT_TEST_MOCK_BACKEND=1` → `MockBackend(50ms)`, (2) `--features vllm` → `VllmBackend::with_secret_token(engine_id, hf_token)`, (3) neither → `Fatal(ConfigInvalid)` at full-run; dry-run still works.
- **`run_id` lifecycle (BLOCKER 6):** three-tier resolution (--resume > `<output.dir>/run-id` > mint + persist via tempfile + rename). Plan 03-05's restart_no_duplicates test consumes the file between Phase A and Phase B.
- **`run_pool` (BLOCKER 4 — concrete impl):** creates output dir → opens `EmbeddedStorage` at `<dir>/rollout.db` + `FsObjectStore` at `<dir>/object-store` + `InMemQueue::open(storage)` (Phase-2 namespace `cloudlocal_queue`) → resolves run_id → constructs `BatchCoordinator::new(.., run_id)` → `scan_and_enqueue(inputs, backend.model_id(), &cfg.sampling)` → spawns N `BatchWorker::run_loop()` via `tokio::task::JoinSet` (each with its own `WorkerId(Ulid::new())`) → awaits all → `collect_done_records()` → emits `<dir>/completions.jsonl` sorted by `input_idx` via `rollout_runtime_batch::write_jsonl`.
- **`glob_inputs`:** literal-path fallback when no `*`/`?` in pattern; otherwise `glob::glob(pattern)` sorted lex; reads each file with `rollout_runtime_batch::read_jsonl`; assigns global `input_idx` across files in deterministic order.
- **Plan-time validation:** `serde(deny_unknown_fields)` (inherited from the runtime schema) + explicit checks for `sampling.stream == false` (`Phase 8` error msg), `sampling.max_tokens > 0`, `workers.count >= 1`, `input.glob` non-empty, and ≥1 input file matched.
- **`--dry-run`:** validates → resolves glob (reads JSONL fully) → reads `ROLLOUT_SECRET_HF_TOKEN` best-effort for known-gated prefixes → prints `dry-run OK: model=… inputs=… workers=…` → exits 0. Backend never constructed.
- **CLI Cargo features:** `vllm = [\"rollout-backend-vllm/vllm\"]` for production live engine; `test-mock-backend = [\"rollout-runtime-batch/test-mock-backend\"]` for plan 03-05 restart test. Independent; both can be enabled.
- **mdBook chapter** `docs/book/src/inference/cli.md` (~140 lines): TOML schema, JSONL contracts, `run_id` lifecycle table, `--dry-run` semantics, backend selection table, observability, exit codes, CPU-mode caveat. Linked from `docs/book/src/SUMMARY.md`.
- **9 new tests:** `infer_dry_run::dry_run_happy_path_exits_zero` + `dry_run_rejects_streaming_sampling` + `dry_run_rejects_zero_workers` + `dry_run_rejects_unknown_toml_field` + `dry_run_rejects_missing_input_files` (5 dry-run integration tests via `assert_cmd`); `cli_help::infer_batch_help_parses` + `infer_top_level_help_lists_subcommand` (2 help-parses); `infer::tests::format_unix_ms_handles_epoch` + `format_unix_ms_handles_known_date` (2 unit tests).

## Task Commits

1. **Task 1** — `69a22fa` (`feat`): `feat(03-04): rollout infer batch CLI + run_id lifecycle + run_pool wiring`

## Files Created/Modified

### Created (4)
- `crates/rollout-cli/src/infer.rs`
- `crates/rollout-cli/src/infer_config.rs`
- `crates/rollout-cli/tests/infer_dry_run.rs`
- `docs/book/src/inference/cli.md`

### Modified (5)
- `crates/rollout-cli/Cargo.toml` (features + path deps + glob + async-trait)
- `crates/rollout-cli/src/main.rs` (mod + Cmd::Infer + infer_dispatch helper)
- `crates/rollout-cli/tests/cli_help.rs` (+2 tests for infer help)
- `docs/book/src/SUMMARY.md` (linked cli.md under Inference)
- `Cargo.lock` (glob v0.3.3 added)

## Decisions Made

- **Schema stays upstream (WARN-5 invariant from 03-02):** `InferBatchConfig` is owned by `rollout-runtime-batch`; the CLI only contributes a 22-line TOML loader. Phase-4 algorithm configs + Phase-6 multi-host configs will follow the same convention.
- **Three-tier `run_id` resolution (BLOCKER 6):** explicit `--resume <id>` > `<output.dir>/run-id` file > minted ULID. The file is single-line UTF-8 Crockford; atomic via tempfile + rename. Implicit re-resume on the same `output.dir` is the dominant ergonomic for restart loops.
- **No chrono/jiff/time dep:** 30-line Howard Hinnant `civil_from_days` produces the RFC3339-shaped `generated_at` string. Two unit tests lock the format.
- **No explicit backend shutdown:** `Arc<dyn InferenceBackend>` may not be uniquely owned at run_pool exit; rely on `VllmBackend::Drop` best-effort cleanup. Revisit if long-lived processes need explicit teardown (likely Phase 8 online-inference).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `select_backend` cfg-chain fell off the end under `--features test-mock-backend` alone (E0317).**
- **Found during:** Task 1 (`cargo build -p rollout-cli --features test-mock-backend`).
- **Issue:** First implementation used `#[cfg(feature = "test-mock-backend")] { ... return ... }` followed by `#[cfg(feature = "vllm")] { ... return ... }` followed by `#[cfg(not(any(feature = "vllm", feature = "test-mock-backend")))]` fast-fail. With ONLY `test-mock-backend` enabled (no vllm), all three blocks could compile away to a single inner `if env == "1" { return ... }` with no else → function falls off end without a Result. E0317 "if may be missing an else clause".
- **Fix:** Restructured: the `test-mock-backend` branch is now a bare `if` (without enclosing `{}`) so its early-return is unconditional when present; the `vllm` branch became the function's final expression block (returning Ok directly); the fast-fail uses `#[cfg(not(feature = "vllm"))]` which co-exists with `test-mock-backend` cleanly. All three feature combinations now compile.
- **Files modified:** `crates/rollout-cli/src/infer.rs`.
- **Verification:** `cargo build -p rollout-cli` (default), `cargo build -p rollout-cli --features vllm`, `cargo build -p rollout-cli --features test-mock-backend` all exit 0.
- **Committed in:** 69a22fa.

**2. [Rule 1 - Bug] `ObjectStore::get_bytes` not in scope.**
- **Found during:** Task 1 (first `cargo build -p rollout-cli`).
- **Issue:** `run_pool` called `object_store.get_bytes(&completion_blob)` but the `ObjectStore` trait wasn't imported, so the method wasn't visible on the `Arc<FsObjectStore>` value.
- **Fix:** Added `ObjectStore` to the `use rollout_core::{...}` import.
- **Files modified:** `crates/rollout-cli/src/infer.rs`.
- **Verification:** `cargo build -p rollout-cli` exits 0.
- **Committed in:** 69a22fa.

**3. [Rule 1 - Clippy] Five pedantic-clippy lints under `-D warnings`.**
- **Found during:** Task 1 (`cargo clippy -p rollout-cli --all-targets --features vllm -- -D warnings`).
- **Issue:** (a) `clippy::doc_markdown` on `BatchCoordinator` + `N × BatchWorker` in the module `//!` doc; (b) `clippy::needless_return` on the final `vllm` branch's explicit `return`; (c) `clippy::cast_sign_loss` + `clippy::cast_possible_truncation` on `as i32`/`as u32` in `civil_from_days`; (d) `clippy::many_single_char_names` (≥5 single-letter bindings); (e) `clippy::similar_names` between `doe` and `doy`.
- **Fix:** (a) wrapped in backticks; (b) dropped the `return` and made the branch a final expression; (c)+(d)+(e) renamed the public-facing bindings to descriptive names (`year_civil`, `year_final`, `month`, `day`, `hour`, `minute`, `second`, `unix_ms`) and added a function-level `#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation, clippy::similar_names)]` to preserve the Howard-Hinnant algorithm's standard inner-variable form; also added function-level `#[allow(clippy::unused_async)]` on `select_backend` because it doesn't `await` under default-features.
- **Files modified:** `crates/rollout-cli/src/infer.rs`.
- **Verification:** `cargo clippy -p rollout-cli --all-targets -- -D warnings`, `cargo clippy -p rollout-cli --all-targets --features vllm -- -D warnings`, `cargo clippy -p rollout-cli --all-targets --all-features -- -D warnings`, and `cargo clippy --workspace --all-targets -- -D warnings` all exit 0.
- **Committed in:** 69a22fa.

### Rule-4 (architectural) deviations

None. All work stayed within the plan's Task 1 scope.

## End-to-end Verification

All commands exited 0:

```
cargo build -p rollout-cli                                                      # default features
cargo build -p rollout-cli --features vllm
cargo build -p rollout-cli --features test-mock-backend
cargo build -p rollout-cli --all-features
cargo test  -p rollout-cli --tests                                              # 14 tests across 3 test files + 2 unit
cargo clippy -p rollout-cli --all-targets -- -D warnings
cargo clippy -p rollout-cli --all-targets --features vllm -- -D warnings
cargo clippy -p rollout-cli --all-targets --all-features -- -D warnings
cargo clippy --workspace --all-targets -- -D warnings                           # no regressions
cargo test  --workspace --tests                                                 # no regressions
cargo test -p rollout-core --test dependency_direction                          # 7/7 invariants hold
RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" \
    cargo doc -p rollout-cli --no-deps --all-features                           # rustdoc gate clean
mdbook build docs/book                                                          # cli.md renders
cargo deny check                                                                # advisories/bans/licenses/sources OK
cargo run -p rollout-cli -- infer batch --help                                  # shows --config / --resume / --workers / --dry-run
BASE_SHA=HEAD~1 HEAD_SHA=HEAD bash scripts/check-docs-tests-touched.sh          # DOCS-02 satisfied
```

## User Setup Required

None for the default-features build (no Python / vllm dependency). The full CLI dry-run path (`cargo test -p rollout-cli --tests`) runs with stdlib + tempfile + tokio + assert_cmd — zero Python.

For the live `--features vllm` path:
- `pip install "vllm>=0.10,<0.22"` (Linux ± CUDA, or macOS via Docker per RESEARCH Pitfall 3).
- For gated HuggingFace models: `export ROLLOUT_SECRET_HF_TOKEN=...`.

## Next Phase Readiness

- **Plan 03-05 (Wave 4) ready:** the `rollout infer batch` subcommand is the load-bearing surface for the smoke test (`scripts/infer-smoke.sh` invokes `rollout infer batch --config examples/batch-tiny.toml`). The `test-mock-backend` feature is the load-bearing surface for the `restart_no_duplicates` integration test (which reads `<output.dir>/run-id` between SIGKILL and restart). Both ingredients are in this commit.
- **Deferred to plan 03-05:** `examples/batch-tiny.toml` config, `scripts/infer-smoke.sh` driver, criterion bench wiring (the bench file exists in plan 03-03; 03-05 adds the perf-ratio measurement script), CI `infer-smoke` job behind `ROLLOUT_VLLM_AVAILABLE=1`.
- **Open question (deferred to Phase 4):** `finish_reason` plumbing from `VllmBackend::Completion` through `SampleState::Done` to `JsonlOutput.finish_reason`. Currently hard-coded to `"stop"`. Worth surfacing when training algorithms (Phase 4) start caring about EOS-vs-length-vs-stop distinctions.

## Self-Check: PASSED

- `crates/rollout-cli/src/infer.rs` — FOUND
  - `pub struct InferCmd` ✓
  - `pub enum InferSub { Batch(BatchArgs) }` ✓
  - `pub struct BatchArgs { config, resume, workers, dry_run }` ✓
  - `pub async fn run_infer_batch` ✓
  - `Phase 8` substring (streaming-rejection error msg) ✓
  - `workers.count must be >= 1` substring ✓
  - `pub async fn glob_inputs` ✓
  - `async fn run_pool` ✓
  - `async fn resolve_run_id` ✓
  - `select_backend` + cfg-gated VllmBackend / MockBackend branches ✓
- `crates/rollout-cli/src/infer_config.rs` — FOUND
  - `pub fn load_from_file` ✓
  - `use rollout_runtime_batch::InferBatchConfig` ✓ (schema lives upstream per WARN-5)
- `crates/rollout-cli/src/main.rs` — `mod infer;` + `Cmd::Infer(infer::InferCmd)` ✓
- `crates/rollout-cli/Cargo.toml` — `vllm`/`test-mock-backend` features ✓; new path deps to rollout-runtime-batch + rollout-backend-vllm + rollout-cloud-local ✓; `glob = "0.3"` ✓
- `crates/rollout-cli/tests/cli_help.rs` — `infer_batch_help_parses` + `infer_top_level_help_lists_subcommand` ✓
- `crates/rollout-cli/tests/infer_dry_run.rs` — FOUND (5 tests)
- `docs/book/src/inference/cli.md` — FOUND
- `docs/book/src/SUMMARY.md` — contains `inference/cli.md` ✓
- Commit `69a22fa` — present in `git log --oneline -5`

---
*Phase: 03-inference-batch*
*Completed: 2026-05-20*
