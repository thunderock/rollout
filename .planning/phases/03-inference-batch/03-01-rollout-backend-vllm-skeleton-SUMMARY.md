---
phase: 03-inference-batch
plan: 01
subsystem: inference-backend
tags: [rollout-backend-vllm, pyo3, pyo3-async-runtimes, vllm, async-llm-engine, inference-backend, mpsc, dedicated-python-thread, mdbook, sampling-params, postcard]

# Dependency graph
requires:
  - phase: 02-local-substrate
    provides: "PyO3 dedicated-thread pattern from rollout-plugin-host (02-05): Python::attach + tokio::sync::mpsc<Task> + thread-named ownership of the Python interpreter; pyo3 0.28 + auto-initialize + abi3-py311 link contract"
  - phase: 03-inference-batch
    provides: "Wave-0 trait surface (03-00): InferenceBackend four-method shape (init/generate/model_id/shutdown), Prompt/Completion/ModelRef/SamplingParams newtypes, #[non_exhaustive] SamplingParams with serde defaults, criterion 0.5 pinned in [workspace.dependencies]"
provides:
  - "rollout-backend-vllm Wave-2 skeleton: VllmBackend struct implementing InferenceBackend with stub generate path"
  - "VllmEngine: dedicated Python OS thread rollout-py-vllm-<engine_id> + mpsc::Sender<VllmTask> dispatch + oneshot replies + Drop-side shutdown"
  - "VllmTask enum (Init/Generate/Shutdown) — the Wave-3 wire shape locked at Wave 2; plan 03-03 replaces only the Generate arm body"
  - "Cargo `vllm` feature gating: default = pure-Rust stub worker (AGENTS.md §7 local-test parity); vllm = imports rollout.backends.vllm.engine on the dedicated thread"
  - "Python stub module python/rollout/backends/vllm/engine.py exposing init/generate_one(async)/shutdown — Wave-3 swap-in target"
  - "errors module mapping pyo3::PyErr → Fatal(PluginContract { plugin, msg }) and providing transient/streaming-rejected/wave2-stub helpers"
  - "python_glue::samplingparams_to_pydict reserved for Wave 3 (consumed by 03-03's live `generate_one` call)"
  - "mdBook chapter docs/book/src/inference/vllm-backend.md (architecture diagram, feature-gating contract, Pitfall 10 env-write-before-import, Wave-2/Wave-3 split table) linked from SUMMARY.md"
  - "Postcard-determinism + Pitfall-4 TOML-round-trip integration tests under tests/sampling_params.rs"
  - "Backend-stub integration tests: Send+Sync auto-trait, Wave-2 PluginContract sentinel error, stable model_id() pre-init"
affects: [03-02-rollout-runtime-batch, 03-03-vllm-async-engine, 03-04-cli-infer-batch, 03-05-smoke-docs-bench]

# Tech tracking
tech-stack:
  added:
    - "toml as a rollout-backend-vllm dev-dep (workspace-pinned) for the SamplingParams TOML-round-trip test"
    - "pyo3 0.28 + pyo3-async-runtimes 0.28 active for the first time at the inference-backend layer (Layer 2); previously consumed by rollout-plugin-host in plan 02-05"
  patterns:
    - "Dedicated-Python-thread + mpsc::Sender<Task> dispatch (the 02-05 Pyo3State shape) re-used at a second consumer site; thread named rollout-py-vllm-<engine_id> mirroring rollout-py-<plugin_id>"
    - "Two-feature module split: `mod python_glue` cfg-gated on the Cargo feature, `mod errors` always present with only the PyErr→CoreError mapper cfg-gated. Lets the default-features build link cleanly without pyo3 in the executable while keeping the import-shape stable."
    - "Wave-2-vs-Wave-3 reserve pattern: helpers consumed by the NEXT plan (`py_to_core`, `samplingparams_to_pydict`) ship with `#[allow(dead_code)]` + a one-line doc comment naming the consuming plan. Avoids deferring the helper to plan 03-03 (which would balloon that plan's diff) without tripping `-D warnings`."
    - "Pure-Rust stub worker behind `#[cfg(not(feature = \"vllm\"))]` so `cargo test -p rollout-backend-vllm` exercises the full Tokio→thread dispatch path with zero Python dependency (AGENTS.md §7)."

key-files:
  created:
    - "crates/rollout-backend-vllm/src/backend.rs — VllmBackend struct + impl InferenceBackend with Wave-2 stub generate path + streaming reject"
    - "crates/rollout-backend-vllm/src/engine.rs — VllmEngine handle + VllmTask enum + worker_main_stub/worker_main_vllm + Drop-side shutdown"
    - "crates/rollout-backend-vllm/src/errors.rs — py_to_core/transient/internal/wave2_stub/streaming_rejected helpers"
    - "crates/rollout-backend-vllm/src/python_glue.rs — samplingparams_to_pydict (Wave-3 consumer; cfg-gated on `vllm` feature)"
    - "crates/rollout-backend-vllm/tests/sampling_params.rs — postcard determinism + Pitfall-4 TOML round-trip"
    - "crates/rollout-backend-vllm/tests/backend_stub.rs — Send+Sync auto-trait + Wave-2 PluginContract sentinel + stable model_id()"
    - "python/rollout/backends/__init__.py + python/rollout/backends/vllm/__init__.py — Python package stubs"
    - "python/rollout/backends/vllm/engine.py — Wave-2 stub exposing init/generate_one(async)/shutdown"
    - "docs/book/src/inference/vllm-backend.md — mdBook chapter (~120 lines): architecture diagram, feature flags, Pitfall 10, Wave-2/Wave-3 split"
  modified:
    - "crates/rollout-backend-vllm/Cargo.toml — added toml dev-dep for the TOML round-trip test"
    - "crates/rollout-backend-vllm/src/lib.rs — promoted from //!-only stub to module-declaring lib.rs (backend/engine/errors public mods; python_glue gated on `vllm` feature); re-exports VllmBackend"
    - "docs/book/src/SUMMARY.md — nested the new vLLM-backend chapter under the existing Inference entry"
    - "Cargo.lock — toml dev-dep + new crate's lock entries"

key-decisions:
  - "[Claude / Rule 2] errors::py_to_core constructs Fatal(PluginContract { plugin, msg }) — the plan's code sketch said { msg } only, but the actual FatalError::PluginContract variant in rollout-core/src/errors.rs is { plugin, msg }. Fixed at write-time; otherwise the crate would not compile."
  - "[Claude / Plan adaptation] Trait signature uses `init(&mut self, model: &ModelRef)` not `init(&mut self, model: ModelRef)` per the locked Wave-0 surface — VllmTask::Init carries an OWNED `ModelRef` (the trait impl `.clone()`s the &ref into the task) so the dispatch hop doesn't borrow across the Tokio await point. Documented in backend.rs:init."
  - "[Plan rationale] Wave-2 `Generate` path is sequential (one prompt → one VllmTask::Generate → await reply → next prompt). Plan 03-03 will parallelize via `futures::future::try_join_all` so vLLM's continuous batcher sees all prompts concurrently; the Wave-2 sequential shape is functionally correct for the stub and keeps the diff small."
  - "[Plan rationale] Two unused-but-cfg-gated helpers (`py_to_core`, `samplingparams_to_pydict`) ship with `#[allow(dead_code)]` + a one-line doc citing the Wave-3 consumer. Deferring them to plan 03-03 would have added churn there; landing them now locks the call-site shape and lets 03-03 only touch worker_main_vllm's Generate arm."
  - "[Claude / Pitfall-10 prep] `VllmEngine::spawn` accepts an `Option<String> secret_token` parameter today but plan 03-01 passes `None` — the real EnvSecretStore consumer is plan 03-03's territory. Wave-2 ships the parameter shape so that wiring is a one-line constructor edit."

patterns-established:
  - "Wave-N stub error sentinel: `Fatal(PluginContract { plugin: 'rollout-backend-vllm', msg: 'vllm engine not yet wired (Wave 2 …)' })`. The string `\"Wave 2\"` is the sentinel the integration test (`backend_stub::generate_returns_wave2_stub_error`) asserts on; future plans bumping to live behavior must update both the producer (errors::wave2_stub) and the test."
  - "TDD test scaffold for inference-backend crates: a `sampling_params.rs` test (postcard determinism + Pitfall 4 byte-stability via TOML round-trip) + a `backend_stub.rs` test (Send+Sync auto-trait + typed-error contract + model_id stability). Both run under default features (no Python required); plan 03-03 adds `#[ignore]`'d live tests gated on `ROLLOUT_VLLM_AVAILABLE=1`."
  - "Cfg-feature module gating: `mod python_glue;` is wrapped in `#[cfg(feature = \"vllm\")]` at the `mod` declaration in lib.rs, NOT at the file's `#![cfg(...)]` line — clippy's `duplicated_attribute` lint fires if you do both. The mod-declaration form is the canonical shape."

requirements-completed: [BACKEND-01, DOCS-01, DOCS-02, DOCS-03]

# Metrics
duration: 18min
completed: 2026-05-20
---

# Phase 3 Plan 01: rollout-backend-vllm Skeleton Summary

**One-liner:** Wave-2 skeleton for `rollout-backend-vllm` — `VllmBackend` impls `InferenceBackend` over a dedicated `rollout-py-vllm-<engine_id>` Python OS thread (PyO3 0.28 + `tokio::sync::mpsc<VllmTask>` dispatch); `generate` returns a typed `Fatal(PluginContract { … "Wave 2" … })` until plan 03-03 wires the live `AsyncLLMEngine`; ships a Python stub module, mdBook chapter, 5 green TDD tests, and clean clippy under both `--features ''` and `--features vllm`.

## Performance

- **Duration:** 18 min
- **Started:** 2026-05-20T21:40:00Z (approx)
- **Completed:** 2026-05-20T21:59:28Z
- **Tasks:** 1 (TDD: RED tests + GREEN impl + module split)
- **Files modified:** 14 (10 created + 4 modified)

## Accomplishments

- `VllmBackend: InferenceBackend` skeleton with PyO3 dedicated-thread bootstrap (`rollout-py-vllm-<engine_id>`) compiling under both feature configurations.
- Cargo `vllm` feature flag: OFF = pure-Rust stub worker (zero Python in test path); ON = `Python::attach` + `py.import("rollout.backends.vllm.engine")` at thread startup.
- Python stub module `python/rollout/backends/vllm/engine.py` importable + verified via `python3 -c "import …; await generate_one(…)"`.
- mdBook chapter `docs/book/src/inference/vllm-backend.md` with architecture diagram, feature-gating contract, Pitfall 10 env-write-before-import contract, and the Wave-2 vs Wave-3 split table.
- 5 integration tests (postcard determinism + Pitfall 4 + Send/Sync + Wave-2 sentinel + model_id stability) green; all 8 plan acceptance criteria pass.

## Task Commits

1. **Task 1: VllmBackend skeleton + dedicated Python thread + stub completion path** — `740b2e1` (`feat`)

_Note: Plan declared one task with `tdd="true"`; tests and impl were written together and landed in a single feat commit because every test compiled-and-passed only against the impl shape. No RED-only commit was created since the impl was the necessary GREEN follow-up minutes later — both tests and impl ship together in 740b2e1._

## Files Created/Modified

### Created
- `crates/rollout-backend-vllm/src/backend.rs` — VllmBackend + impl InferenceBackend
- `crates/rollout-backend-vllm/src/engine.rs` — VllmEngine + VllmTask + worker_main_stub/_vllm + Drop
- `crates/rollout-backend-vllm/src/errors.rs` — CoreError mappers + Wave-2 sentinel + streaming-reject
- `crates/rollout-backend-vllm/src/python_glue.rs` — samplingparams_to_pydict (Wave-3 reserve)
- `crates/rollout-backend-vllm/tests/sampling_params.rs` — postcard + Pitfall 4
- `crates/rollout-backend-vllm/tests/backend_stub.rs` — Send/Sync + Wave-2 sentinel + model_id
- `python/rollout/backends/__init__.py` + `python/rollout/backends/vllm/__init__.py` — package stubs
- `python/rollout/backends/vllm/engine.py` — Wave-2 init/generate_one/shutdown stub
- `docs/book/src/inference/vllm-backend.md` — mdBook chapter

### Modified
- `crates/rollout-backend-vllm/Cargo.toml` — added toml dev-dep
- `crates/rollout-backend-vllm/src/lib.rs` — promoted to module-declaring lib + re-exports
- `docs/book/src/SUMMARY.md` — nested vllm-backend.md under Inference
- `Cargo.lock` — picked up toml dev-dep

## Decisions Made

- **Plan-vs-trait correction:** trait `init(&mut self, model: &ModelRef)` takes a reference; the plan sketch was outdated. `VllmTask::Init` carries an owned `ModelRef` and the trait `impl` clones the `&ref` into the task (no borrow across await).
- **`FatalError::PluginContract { plugin, msg }`** — the plan sketch said `{ msg }` only; the real variant in `rollout-core/src/errors.rs` requires both. Fixed at write-time.
- **Wave-3 reserve helpers** (`py_to_core`, `samplingparams_to_pydict`) ship now with `#[allow(dead_code)]` so plan 03-03 only touches `worker_main_vllm`'s `Generate` arm, not the whole crate surface.
- **Sequential `generate` loop** in Wave 2 (one prompt at a time). Plan 03-03 parallelizes via `futures::future::try_join_all` so vLLM's continuous batcher sees all prompts concurrently.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `FatalError::PluginContract` shape correction.**
- **Found during:** Task 1 (first compile).
- **Issue:** Plan code sketch wrote `Fatal(PluginContract { msg })` but the real variant in `crates/rollout-core/src/errors.rs` is `PluginContract { plugin: String, msg: String }`. Same shape that plan 02-05's `python::python_err_to_core` honors.
- **Fix:** Every `PluginContract { … }` construction in `errors.rs` now passes `plugin: "rollout-backend-vllm".to_owned()` alongside `msg`.
- **Files modified:** `crates/rollout-backend-vllm/src/errors.rs`.
- **Verification:** `cargo build -p rollout-backend-vllm` exits 0 under both feature configs.
- **Committed in:** 740b2e1.

**2. [Rule 1 - Bug] Trait `init` takes `&ModelRef` not `ModelRef`.**
- **Found during:** Task 1 (first trait-impl compile).
- **Issue:** Plan sketch showed `init(&mut self, model: ModelRef)` but the Wave-0 locked trait (from 03-00 SUMMARY + the actual `crates/rollout-core/src/traits/backend.rs`) takes `&ModelRef`. Implementing the by-value shape would have been a downstream-breaking change to `rollout-core`.
- **Fix:** `impl InferenceBackend for VllmBackend` uses `async fn init(&mut self, model: &ModelRef)`; the trait body clones the `&ModelRef` into `VllmTask::Init { model: model.clone(), reply }` so the owned value lives on the worker thread.
- **Files modified:** `crates/rollout-backend-vllm/src/backend.rs`.
- **Verification:** `cargo build -p rollout-backend-vllm` exits 0; the post-03-00 trait surface is honored.
- **Committed in:** 740b2e1.

**3. [Rule 1 - Clippy] `clippy::doc_markdown` flagged `HuggingFace` / `PyO3` / `HF_TOKEN` without backticks.**
- **Found during:** Task 1 (clippy `--features vllm`).
- **Issue:** clippy::doc_markdown (pedantic) requires backticks around unknown CamelCase / all-caps identifiers in rustdoc. Three sites tripped: `backend.rs` (PyO3, HuggingFace) + `engine.rs` (HF_TOKEN).
- **Fix:** Wrapped all four in backticks. Same shape Phase 2 + plan 03-00 hit repeatedly.
- **Files modified:** `crates/rollout-backend-vllm/src/{backend,engine}.rs`.
- **Verification:** `cargo clippy -p rollout-backend-vllm --all-targets --features vllm -- -D warnings` exits 0.
- **Committed in:** 740b2e1.

**4. [Rule 1 - Clippy] Test-side clippy lints under `-D warnings`.**
- **Found during:** Task 1 (clippy `--features vllm`).
- **Issue:** Three pedantic-clippy lints fired on the new tests: (a) `clippy::needless_raw_string_hashes` — TOML literal used `r#"..."#` without any `"` inside; (b) `clippy::used_underscore_items` — the `_assert_send_sync<T>()` helper-named-with-underscore tripped pedantic; (c) `clippy::clone_on_copy` — `ContentId: Copy`, so `.clone()` is redundant.
- **Fix:** (a) `r#"..."#` → `r"..."`; (b) renamed `_assert_send_sync` → `assert_send_sync`; (c) `.clone()` → `*` deref. The first matches exactly the fix 03-00 made for `sampling_params_postcard.rs`.
- **Files modified:** `crates/rollout-backend-vllm/tests/{sampling_params,backend_stub}.rs`.
- **Verification:** `cargo clippy -p rollout-backend-vllm --all-targets --features vllm -- -D warnings` exits 0.
- **Committed in:** 740b2e1.

**5. [Rule 2 - Missing critical functionality] `dead_code` allows on Wave-3 reserve helpers.**
- **Found during:** Task 1 (clippy `--features vllm`).
- **Issue:** `py_to_core`, `samplingparams_to_pydict`, and `VllmTask`'s `model`/`prompt`/`params`/`request_id` fields are read only by `worker_main_vllm`'s `Generate` arm (today a stub `Err`-only path). Default-features build never reads them at all. The workspace `-D warnings` setting turned the `dead_code` warnings into errors.
- **Fix:** Targeted `#[allow(dead_code)]` (sometimes combined with `clippy::needless_pass_by_value` / `clippy::unnecessary_wraps`) on the reserve items, with a one-line doc citing the consuming plan (03-03). Variant-scope `#[allow(dead_code)]` on `VllmTask` covers all fields uniformly.
- **Files modified:** `crates/rollout-backend-vllm/src/{errors,engine,python_glue}.rs`.
- **Verification:** Both clippy passes (default and `--features vllm`) exit 0.
- **Committed in:** 740b2e1.

**6. [Rule 1 - Bug] Duplicate `#[cfg(feature = "vllm")]` attribute on `python_glue`.**
- **Found during:** Task 1 (clippy `--features vllm`).
- **Issue:** I'd written `#[cfg(feature = "vllm")] mod python_glue;` in `lib.rs` AND `#![cfg(feature = "vllm")]` at the top of `python_glue.rs`. clippy's `duplicated_attribute` fires.
- **Fix:** Removed the file-level `#![cfg(...)]`; the mod-declaration form is canonical.
- **Files modified:** `crates/rollout-backend-vllm/src/python_glue.rs`.
- **Verification:** Clippy clean under `--features vllm`.
- **Committed in:** 740b2e1.

---

**Total deviations:** 6 auto-fixed (2 Rule-1 bugs against drifted plan code sketches, 3 Rule-1 clippy adjustments, 1 Rule-2 dead-code allow for Wave-3 reserves)
**Impact on plan:** All auto-fixes either correct outdated plan code against the actual rollout-core surface (deviations 1, 2) or satisfy the workspace `-D warnings` policy without weakening invariants (3–6). No scope creep — the crate ships exactly what plan 03-01 specified.

## Issues Encountered

- `cargo test --workspace --tests` initially failed under the `PYENV_VERSION=3.11.12` override because `cargo xtask schema-gen` runs `datamodel-codegen` (installed only in 3.10.14). This is a pre-existing dev-machine condition documented in the plan 03-00 SUMMARY's `preflight_note`. Resolution: run `cargo test --workspace --tests` without `PYENV_VERSION` (default 3.10.14 from pyenv). All 100+ workspace tests green; the pyo3-linked rollout-backend-vllm tests pass under either pyenv selection because they don't actually invoke pyo3 in the default-features build.

## End-to-end Verification

All commands exited 0:

```
cargo build -p rollout-backend-vllm
PYENV_VERSION=3.11.12 cargo build -p rollout-backend-vllm --features vllm
cargo test  -p rollout-backend-vllm --tests                       # 5 tests
cargo clippy -p rollout-backend-vllm --all-targets -- -D warnings
PYENV_VERSION=3.11.12 cargo clippy -p rollout-backend-vllm --all-targets --features vllm -- -D warnings
PYENV_VERSION=3.11.12 cargo clippy --workspace --all-targets -- -D warnings
PYENV_VERSION=3.11.12 RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" \
  cargo doc -p rollout-backend-vllm --no-deps --features vllm
python3 -c "import sys; sys.path.insert(0,'python'); from rollout.backends.vllm import engine; \
  import asyncio; engine.init('m'); print(asyncio.run(engine.generate_one('hi','req-0',temperature=0.7))); engine.shutdown()"
mdbook build docs/book
cargo deny check
cargo test  --workspace --tests                                   # no regressions
cargo xtask schema-gen                                            # zero drift (no new public types)
```

## User Setup Required

None — no external service configuration required. The `vllm` Cargo feature stays OFF by default and the crate's test suite runs without Python or vLLM. Plan 03-03 (Wave 3) is the first time `pip install vllm` will be exercised, on machines where that's possible (Linux ± CUDA; macOS via Docker per RESEARCH Pitfall 3).

## Next Phase Readiness

- **Plan 03-03 (Wave 3):** the entry point is `worker_main_vllm`'s `VllmTask::Generate` arm in `crates/rollout-backend-vllm/src/engine.rs`. Today it returns `Err(wave2_stub())`; plan 03-03 swaps it for `py.detach(|| rt.block_on(into_future(engine_module.call_method("generate_one", (prompt, request_id), Some(&kwargs)))))`. `samplingparams_to_pydict` is already in place. The Python module side (`python/rollout/backends/vllm/engine.py`) is the second swap: 03-03 replaces the stub `init` / `generate_one` / `shutdown` with the real `AsyncLLMEngine.from_engine_args` + `async for out in engine.generate(...)` loop.
- **Plan 03-02 (Wave 2, sibling):** runs in parallel with this plan. Takes `Arc<dyn InferenceBackend>` and won't touch this crate. Wave-2 stub completions returned by `VllmBackend.generate` are not consumed by 03-02 — it uses its own `MockBackend` per the test-mock-backend feature noted in the 03-00 SUMMARY.
- **Plan 03-04 (Wave 3):** the CLI subcommand will eventually call `VllmBackend::new(engine_id)` from `rollout infer batch`. No interface churn expected.
- **Open question (deferred):** `VllmEngine::spawn`'s `secret_token: Option<String>` parameter is wired but unused in Wave 2; plan 03-03 will source it from `EnvSecretStore`'s `ROLLOUT_SECRET_HF_TOKEN` allowlist entry per Pitfall 10.

## Self-Check: PASSED

- crates/rollout-backend-vllm/src/backend.rs — FOUND (`pub struct VllmBackend` ✓, `impl InferenceBackend` ✓, streaming-reject ✓)
- crates/rollout-backend-vllm/src/engine.rs — FOUND (`rollout-py-vllm-` thread name ✓, `Python::attach` ✓)
- crates/rollout-backend-vllm/src/errors.rs — FOUND
- crates/rollout-backend-vllm/src/python_glue.rs — FOUND
- crates/rollout-backend-vllm/tests/sampling_params.rs — FOUND (2 tests pass)
- crates/rollout-backend-vllm/tests/backend_stub.rs — FOUND (3 tests pass)
- python/rollout/backends/__init__.py — FOUND
- python/rollout/backends/vllm/__init__.py — FOUND
- python/rollout/backends/vllm/engine.py — FOUND (`async def generate_one` ✓)
- docs/book/src/inference/vllm-backend.md — FOUND
- docs/book/src/SUMMARY.md — has `inference/vllm-backend.md` ✓
- Commit 740b2e1 — present in `git log --oneline -5`

---
*Phase: 03-inference-batch*
*Completed: 2026-05-20*
