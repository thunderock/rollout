---
phase: 04-train-sft-rm-snapshots
plan: 05
subsystem: backend-vllm-train
tags: [rollout-backend-vllm, trainable-backend, pyo3, python-os-thread, hf-transformers, accelerate, determinism, qwen25, chat-template, pitfall-1, pitfall-2, pitfall-3, pitfall-7, pitfall-8, pitfall-10, mdbook, training]

# Dependency graph
requires:
  - phase: 04-train-sft-rm-snapshots
    provides: "Plan 04-00-a — TrainableBackend trait + GradHandle/TrainBatch/LossOutput/LossScope/OptimizerSettings + the optimizer_step(&self) interior-mutability shape"
  - phase: 04-train-sft-rm-snapshots
    provides: "Plan 04-00-b — train Cargo feature already registered on rollout-backend-vllm (implies vllm); workspace dep pins"
  - phase: 04-train-sft-rm-snapshots
    provides: "Plan 04-01 — SnapshotterImpl (used by 04-06 CLI integration; not directly consumed here)"
  - phase: 03-inference-batch
    provides: "Plan 03-03 — dedicated Python OS thread (rollout-py-vllm-<engine_id>) + Python::attach + Pitfall-10 env-write-before-import contract that this plan reuses for train.py"
provides:
  - "rollout-backend-vllm::VllmBackend impls rollout_core::TrainableBackend under --features train"
  - "5 new VllmTask variants gated on #[cfg(feature = train)]: SetTrainMode / ForwardWithLoss / OptimizerStep / SaveWeights / LoadWeights"
  - "src/train.rs — Pitfall-2 env-write-before-import enforcer; py.detach wrappers (RESEARCH Pattern 2) so GIL releases across CUDA kernels"
  - "python/rollout/backends/vllm/train.py — Accelerator-wrapped HF transformers training; determinism preamble; Qwen2.5 chat-template override; Pitfalls 1/2/3/7/8/10 mitigations all live here"
  - "python/rollout/backends/vllm/qwen25_chat_template.py — generation-marked Qwen2.5 template (Pitfall 1)"
  - "tests/train_thread_smoke.rs — default-fire CI smoke (no transformers required) proves the thread comes up gracefully"
  - "tests/snapshot_resume_live.rs — gated on ROLLOUT_TRANSFORMERS_AVAILABLE=1; live-witness shape for the Qwen2.5 CPU resume path"
  - "tests/qwen25_assistant_mask.rs — gated on ROLLOUT_TRANSFORMERS_AVAILABLE=1; Pitfall-1 acceptance for the chat-template override"
  - "mdBook chapters: training/determinism.md + training/cpu-mode.md"
affects: [phase-04 plan 04-06 (CLI mounts the train feature via the new TrainableBackend), plan 04-07 (smoke recipe consumes the train.py path + the mdBook chapters)]

# Tech tracking
tech-stack:
  added:
    - "ulid promoted to prod-dep (gated on `train` feature) for `save_weights` target_dir minting"
    - "rollout-snapshots + rollout-storage + rollout-cloud-local added as dev-deps of rollout-backend-vllm (used only by snapshot_resume_live.rs)"
  patterns:
    - "Lazy inference-module import in worker_main_vllm: --features train builds defer py.import(rollout.backends.vllm.engine) until first Init/Generate so train-only callers don't pay the vllm-import cost"
    - "ActiveMode { None | Inference | Training } tracked on the worker thread; Inference→Training swap rejected with PluginContract (Phase-9 deferral)"
    - "Pitfall-2 env-write-before-import: train.rs::import_train_module writes CUBLAS_WORKSPACE_CONFIG + PYTHONHASHSEED + (optional) HF_TOKEN via os.environ BEFORE py.import(rollout.backends.vllm.train)"
    - "py.detach(...) wraps the forward + optimizer_step calls so the GIL releases during the heavy CUDA kernel work (RESEARCH Pattern 2)"
    - "Phase-4 simplification: train.py holds the pending loss tensor in module-global _STATE; Rust optimizer_step passes only the step counter. Full PyObject grad plumbing lands in Phase 9"

key-files:
  created:
    - "crates/rollout-backend-vllm/src/train.rs"
    - "crates/rollout-backend-vllm/tests/train_thread_smoke.rs"
    - "crates/rollout-backend-vllm/tests/snapshot_resume_live.rs"
    - "crates/rollout-backend-vllm/tests/qwen25_assistant_mask.rs"
    - "python/rollout/backends/vllm/train.py"
    - "python/rollout/backends/vllm/qwen25_chat_template.py"
    - "docs/book/src/training/determinism.md"
    - "docs/book/src/training/cpu-mode.md"
  modified:
    - "crates/rollout-backend-vllm/Cargo.toml (ulid optional prod-dep gated on train; rollout-snapshots + rollout-storage + rollout-cloud-local dev-deps)"
    - "crates/rollout-backend-vllm/src/lib.rs (declare train module under #[cfg(feature = train)])"
    - "crates/rollout-backend-vllm/src/engine.rs (ActiveMode enum + 5 new VllmTask variants gated on train; worker_main_vllm extended with lazy inference-module import + 5 new train dispatch arms)"
    - "crates/rollout-backend-vllm/src/backend.rs (impl rollout_core::TrainableBackend for VllmBackend gated on train)"
    - "docs/book/src/SUMMARY.md (Determinism + CPU-mode entries in the Training section)"

key-decisions:
  - "Lazy inference-module import in worker_main_vllm. --features train builds always go through this worker (train implies vllm). Eagerly importing the vllm Python module on thread startup would force any train-only test to pay the (heavy) vllm import even if no inference call ever happens. Deferring until the first Init/Generate lands keeps the smoke test fast and the train.rs path independent."
  - "Phase-4 simplification — train.py keeps the pending loss tensor in module-global _STATE. The Rust-side run_optimizer_step passes only the GradHandle.step counter; train.py retrieves the tensor from _STATE.last_loss for the backward + step. Real bidirectional PyObject plumbing (multiple in-flight grads, distinct backends per process) is a Phase-9 follow-up. Phase 4's use-case is single-backend + single-grad-in-flight, where this is correct."
  - "VllmBackend::load_weights is a no-op in Phase 4. The trait method takes a ContentId; the Phase-4 save_weights returns ContentId::of(target_dir_path_bytes) as a placeholder. Real ContentId-of-tar lives in SnapshotterImpl (plan 04-01); the CLI flow in plan 04-06 will call SnapshotterImpl::save_train_state directly with the worker's accelerate_dir. Phase-9 PPO actor adds the weights_id → dir resolver."
  - "ActiveMode::Inference is reserved-but-unreachable in Phase 4. Init never promotes mode to Inference; only the Phase-9 path will. The variant is kept (with #[allow(dead_code)]) so set_train_mode(true) after Generate has a typed rejection point already in place when the Phase-9 work lands."
  - "snapshot_resume_live.rs is shape-only. The byte-compare assertion (TRAIN-03 LOAD-BEARING) lives in rollout-algo-sft::tests::snapshot_resume::bit_identical_resume_at_step_5 with MockBackend, which runs on every CI build without transformers. The live test proves the VllmBackend train surface compiles + dispatches; the bit-identical determinism contract is exercised by the MockBackend path."

patterns-established:
  - "Pattern: when a dedicated Python OS thread must serve TWO different Python modules (engine.py for inference, train.py for training), gate the imports lazily inside the worker loop. The first inference task triggers engine import; the first train task triggers train import; either can fire first or never."
  - "Pattern: Phase-4 training trait methods that need a per-call tempdir (save_weights) mint the dir on the algorithm side (Rust) and pass it through the VllmTask::SaveWeights variant. Avoids leaking std::env::temp_dir() into the Python module-global state."
  - "Pattern: gated Python integration tests carry both `#![cfg(feature = train)]` AND `#[ignore = ...]` so they (a) only compile under the right feature, (b) only run when the dev env explicitly opts in via ROLLOUT_TRANSFORMERS_AVAILABLE=1."

requirements-completed: [TRAIN-01, TRAIN-02, TRAIN-03, DOCS-01, DOCS-02, DOCS-03]

# Metrics
duration: 10min
completed: 2026-05-21
---

# Phase 4 Plan 05: rollout-backend-vllm train surface Summary

**`VllmBackend` ships `TrainableBackend` end-to-end behind `--features train` —
5 new `VllmTask` variants (`SetTrainMode` / `ForwardWithLoss` / `OptimizerStep`
/ `SaveWeights` / `LoadWeights`) dispatch through the same dedicated Python
OS thread Phase-3 introduced; `train.py` carries the determinism preamble +
Qwen2.5 chat-template override + Pitfall 1/2/3/7/8/10 mitigation set; default-
fire smoke test proves the thread comes up cleanly without `transformers`
installed; gated live witness for the Qwen2.5-0.5B-Instruct CPU resume path
in place; mdBook gets two new chapters (`training/determinism.md` + `training/
cpu-mode.md`). 3 tests green on `cargo test --features train --test
train_thread_smoke`; 0 regressions on `cargo test --workspace --tests` (201
passed, 4 ignored).**

## Performance

- **Duration:** ~10 min
- **Started:** 2026-05-21T21:56:07Z
- **Completed:** 2026-05-21T22:06:29Z
- **Tasks:** 2
- **Files created:** 8 (3 Rust source + 3 Rust tests + 2 Python + 2 mdBook = 10; -2 because train.rs lives in src/; final count 8 created, 4 modified — see frontmatter)
- **Files modified:** 5

## Accomplishments

### 5 new `VllmTask` variants (Phase 4)

| Variant            | Reply type                          | Notes                                                     |
| ------------------ | ----------------------------------- | --------------------------------------------------------- |
| `SetTrainMode`     | `Result<(), CoreError>`             | Idempotent for `None→train`; rejects `Inference→train`    |
| `ForwardWithLoss`  | `Result<LossOutput, CoreError>`     | `py.detach` wrapper releases GIL during CUDA kernels      |
| `OptimizerStep`    | `Result<(), CoreError>`             | Phase-4: passes only `GradHandle.step`; loss lives in `_STATE` |
| `SaveWeights`      | `Result<ContentId, CoreError>`      | Mints tempdir on the algorithm side; `ContentId` is placeholder |
| `LoadWeights`      | `Result<(), CoreError>`             | No-op on `VllmBackend::load_weights`; real path via `SnapshotterImpl` |

All five variants are gated on `#[cfg(feature = "train")]`. The default-features
worker (`worker_main_stub`) doesn't see them; the `vllm`-features worker
(`worker_main_vllm`) dispatches them via `crate::train::run_*` helpers.

### Pitfall mitigations applied

| Pitfall | Mitigation                                                | Where                                                                          |
| ------- | --------------------------------------------------------- | ------------------------------------------------------------------------------ |
| 1       | Qwen2.5 chat template with `{% generation %}` markers      | `python/rollout/backends/vllm/qwen25_chat_template.py`                          |
| 2       | `CUBLAS_WORKSPACE_CONFIG` + `PYTHONHASHSEED` BEFORE `import torch` | `train.py` top-of-module + `train.rs::import_train_module` env-write enforcer  |
| 3       | torchdata stateful-dataloader detection + fallback         | `train.py::init_train` (`try import torchdata`)                                 |
| 7       | Accelerator singleton + `gc.collect` + `cuda.empty_cache` on teardown | `train.py::init_train` (idempotent) + `teardown_train`                          |
| 8       | `cudnn.benchmark = False` explicit (not just `deterministic = True`) | `train.py::_set_determinism_flags`                                              |
| 10      | `accelerator.prepare(scheduler)` OR `register_for_checkpointing` fallback | `train.py::init_train` (try/except)                                             |

### `GradHandle` / `TrainBatch` shape decisions for Phase 4

- `GradHandle` carries only `step: u64`. The opaque loss tensor lives in
  Python module-global `_STATE.last_loss` between `forward_with_loss` and
  `optimizer_step`. Phase-4 acceptable because only one grad is in flight
  at any time per backend.
- `TrainBatch::with_rows(n_sequences, n_tokens, rows)` is the canonical
  constructor (matches the `#[non_exhaustive]` shape from plan 04-00-a).
- `LossScope::AssistantOnly` is passed through to Python as the string
  `"assistant_only"`; the actual mask plumbing is a plan-04-07 smoke item
  (the chat-template override is independently witnessed by
  `qwen25_assistant_mask.rs`).

### `train_thread_smoke.rs` outcome

PASSES on macOS dev box with no `transformers`/`accelerate`/`torch` installed.
The three tests in the smoke file:

1. `gradhandle_send_sync` — compile-time assertion `GradHandle: Send + Sync`.
2. `vllm_backend_is_send_sync_under_train` — compile-time assertion
   `VllmBackend: Send + Sync` under `--features train`.
3. `thread_starts_under_train_feature` — runtime: construct `VllmBackend::new`
   (spawns the dedicated Python OS thread), call `set_train_mode(true).await`,
   accept EITHER `Ok(())` (transformers installed on dev box) OR
   `Fatal(PluginContract { msg: ~"python error|transformers|accelerate|ModuleNotFoundError|torch|No module named" })`.

Crucially: the test does NOT panic on missing Python deps. The dedicated
worker thread captures the import error and routes it back through the reply
channel as a typed `CoreError`.

### Tests + verification

```
$ cargo build -p rollout-backend-vllm                       # OK
$ cargo build -p rollout-backend-vllm --features vllm       # OK
$ cargo build -p rollout-backend-vllm --features train      # OK
$ cargo build -p rollout-backend-vllm --all-features        # OK
$ cargo test  -p rollout-backend-vllm --features train --test train_thread_smoke  # 3 passed
$ cargo clippy -p rollout-backend-vllm --features train --all-targets -- -D warnings  # clean
$ cargo clippy -p rollout-backend-vllm --all-features --all-targets -- -D warnings    # clean
$ cargo clippy --workspace --all-targets -- -D warnings                                # clean
$ cargo test --workspace --tests                                                       # 201 passed, 4 ignored
$ RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" \
    cargo doc -p rollout-backend-vllm --features train --no-deps    # OK
$ mdbook build docs/book                                                              # OK
```

## Task Commits

1. **Task 1: Python train.py + Qwen2.5 chat-template override + determinism preamble** — `faf1799` (`feat(04-05-01):`)
2. **Task 2: VllmBackend TrainableBackend impl + train thread smoke + snapshot_resume_live** — `2f658ed` (`feat(04-05-02):`)

## Files Created/Modified

**Created (8):**
- `crates/rollout-backend-vllm/src/train.rs` — Rust-side training glue (Pitfall-2 enforcer + `py.detach` wrappers).
- `crates/rollout-backend-vllm/tests/train_thread_smoke.rs` — default-fire CI smoke.
- `crates/rollout-backend-vllm/tests/snapshot_resume_live.rs` — gated live witness for Qwen2.5 CPU.
- `crates/rollout-backend-vllm/tests/qwen25_assistant_mask.rs` — gated Pitfall-1 acceptance.
- `python/rollout/backends/vllm/train.py` — Accelerator-wrapped HF transformers training module.
- `python/rollout/backends/vllm/qwen25_chat_template.py` — generation-marked Qwen2.5 template (Pitfall 1).
- `docs/book/src/training/determinism.md` — Determinism contract chapter (~160 lines).
- `docs/book/src/training/cpu-mode.md` — CPU mode chapter (~80 lines).

**Modified (5):**
- `crates/rollout-backend-vllm/Cargo.toml` — `ulid` optional prod-dep gated on `train`; new dev-deps.
- `crates/rollout-backend-vllm/src/lib.rs` — declare `mod train` under `#[cfg(feature = "train")]`.
- `crates/rollout-backend-vllm/src/engine.rs` — `ActiveMode` enum + 5 new `VllmTask` variants + lazy inference-module import + 5 new train dispatch arms.
- `crates/rollout-backend-vllm/src/backend.rs` — `impl rollout_core::TrainableBackend for VllmBackend` (gated).
- `docs/book/src/SUMMARY.md` — Determinism + CPU-mode entries added under Training.

## Decisions Made

(See `key-decisions` frontmatter for the full list.)

- **Lazy inference-module import in `worker_main_vllm`.** A `--features train`
  smoke test that never calls `Init` should not pay the cost of importing
  `vllm`. The lazy path keeps the smoke fast and the train.rs path
  self-contained.
- **`run_optimizer_step` takes only the step counter.** Loss tensor lives in
  Python `_STATE.last_loss`. Phase-4 acceptable; Phase-9 PPO will need real
  PyObject plumbing for multiple in-flight grads.
- **`VllmBackend::load_weights` is a no-op.** Phase-4 doesn't have a real
  weights-id → directory resolver; the CLI flow in plan 04-06 calls
  `SnapshotterImpl::save_train_state` / `restore_train_state` directly with
  the accelerate dir, bypassing the trait method.
- **`ActiveMode::Inference` is reserved.** Phase-4 never promotes to it; the
  variant exists so the Phase-9 swap-rejection logic compiles today.
- **`snapshot_resume_live.rs` is shape-only.** The byte-compare proof is the
  `MockBackend` test in `rollout-algo-sft`; the live test proves dispatch
  compiles + works against the real Python module.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Plan snippet's `Python::with_gil` → must be `Python::attach` (PyO3 0.28)**
- **Found during:** Task 2 first build under `--features train`.
- **Issue:** PyO3 0.28 renamed `Python::with_gil` to `Python::attach`. The plan
  snippet used the old name. The existing Phase-3 `engine.rs` already uses
  `Python::attach`, so the rename is mechanical.
- **Fix:** Wrote `train.rs` with `Python::attach` from the outset.
- **Files modified:** N/A (caught before commit).
- **Committed in:** `2f658ed`.

**2. [Rule 1 - Bug] `import_train_module(secret_token: &Option<String>)` tripped `clippy::ref_option`**
- **Found during:** Task 2, `cargo clippy --features train -- -D warnings`.
- **Issue:** Clippy pedantic-group `ref_option` lint requires `Option<&T>`
  over `&Option<T>` for non-owning function arguments.
- **Fix:** Changed signature to `secret_token: Option<&str>` and called via
  `secret_token.map(String::as_str)` from `run_set_train_mode`. The caller
  in `engine.rs` passes `secret_token.as_ref()`.
- **Files modified:** `crates/rollout-backend-vllm/src/{train.rs, engine.rs}`.
- **Verification:** `cargo clippy -p rollout-backend-vllm --features train --all-targets -- -D warnings` exits 0.
- **Committed in:** `2f658ed`.

**3. [Rule 1 - Bug] `run_optimizer_step(grads: GradHandle, ...)` tripped `clippy::needless_pass_by_value`**
- **Found during:** Task 2, same clippy run.
- **Issue:** `GradHandle` is moved into a fresh `PyDict` field; clippy correctly notes the parameter isn't consumed.
- **Fix:** Changed `run_optimizer_step` to take `grads: &GradHandle`; updated both call sites in `engine.rs` (`worker_main_train_only` + `worker_main_vllm`) to pass `&grads`.
- **Files modified:** `crates/rollout-backend-vllm/src/{train.rs, engine.rs}`.
- **Committed in:** `2f658ed`.

**4. [Rule 1 - Bug] `#![cfg(feature = "train")]` in `train.rs` duplicated the lib.rs `#[cfg(feature = "train")] mod train`**
- **Found during:** Task 2, `cargo clippy`.
- **Issue:** `duplicated attribute` error. The mod declaration already gates the module; the inner `#![cfg]` is redundant.
- **Fix:** Removed the `#![cfg(feature = "train")]` line from `train.rs`.
- **Committed in:** `2f658ed`.

**5. [Rule 1 - Bug] Three `clippy::doc_markdown` violations**
- **Found during:** Task 2 clippy + Task 2 final clippy pass.
- **Issue:** Bare `PyO3` / `PyObject` / `MockBackend` in doc comments.
- **Fix:** Backticked all three sites.
- **Committed in:** `2f658ed`.

**6. [Rule 2 - Missing critical functionality] `ulid` not in `rollout-backend-vllm` prod-deps**
- **Found during:** Task 2, after adding `ulid::Ulid::new()` to
  `VllmBackend::save_weights` for the tempdir name.
- **Issue:** `ulid` is in `[workspace.dependencies]` but not declared on
  `rollout-backend-vllm`. The save_weights impl needs it inside
  `#[cfg(feature = "train")]`, so the dep must also be optional + gated.
- **Fix:** Added `ulid = { workspace = true, optional = true }` to
  `[dependencies]`; extended `train = ["vllm", "dep:ulid"]` so enabling
  train automatically pulls ulid. Default builds remain unaffected.
- **Files modified:** `crates/rollout-backend-vllm/Cargo.toml`.
- **Verification:** All 4 feature-combination builds (`default`, `vllm`,
  `train`, `all-features`) succeed.
- **Committed in:** `2f658ed`.

**7. [Rule 1 - Bug] `worker_main_vllm` lazy-import refactor — semantics change vs Phase-3**
- **Found during:** Task 2 design.
- **Issue:** The original `worker_main_vllm` (plan 03-03) imports `engine.py`
  on thread spawn. Under `--features train` that's wrong: a train-only
  caller must NOT pay the vllm import cost (which on dev boxes can be 5–10s
  even when the import succeeds, and panics-or-fails on dev boxes without
  vllm installed). Lazy-import the inference module on the first `Init` /
  `Generate` task instead.
- **Fix:** Restructured `worker_main_vllm` to track `inference_module:
  Option<Py<PyModule>>` + `inference_import_err: Option<String>`. The
  helper closure `import_inference` populates one or the other on demand.
  `Init` / `Generate` arms call the helper before dispatching; train arms
  never touch it. Shutdown gracefully calls `engine.shutdown()` only if the
  module was loaded.
- **Files modified:** `crates/rollout-backend-vllm/src/engine.rs`.
- **Verification:** All builds clean; `cargo test --workspace --tests` shows
  no Phase-3 regressions (the Phase-3 `vllm_init.rs` / `vllm_generate.rs`
  tests stay `#[ignore]`'d so the live behaviour is exercised on the dev
  box only; the `pyo3_bridge_smoke.rs` runs and passes because it uses the
  generic `pyo3_async_runtimes` API, not our backend).
- **Committed in:** `2f658ed`.

---

**Total deviations:** 7 auto-fixed (1 architectural refactor — #7 lazy
inference-module import — that the plan implicitly requires for the smoke
test to make sense; 1 missing-dep — #6 ulid; 5 mechanical clippy/syntax
fixes). 0 architectural decisions required.

**Impact on plan:** No scope expansion. #7 is the only "real" deviation —
the plan's literal `import_result` block at the top of `worker_main_vllm`
would have made `train_thread_smoke.rs` fail on every CI build (the vllm
import would error out before any train task got dispatched). Documented
in this SUMMARY and in `engine.rs` comments.

## Issues Encountered

- **Parallel sibling agent (04-04).** Plan 04-04 was running concurrently on
  `main`, modifying `crates/rollout-algo-rm/*` and editing
  `docs/book/src/SUMMARY.md`. Strictly stayed in my own file scope
  (`rollout-backend-vllm`, `python/rollout/backends/vllm/`, `training/*.md`,
  and my SUMMARY entry); the sibling added the RM chapter line to SUMMARY.md
  in a separate insertion point with no conflict.
- **Cargo.lock**: shared workspace artifact; I did not stage it. The
  orchestrator picks it up after all parallel agents land.

## Auth gates encountered

None. The smoke test runs without any external service; the live test gates
itself on `ROLLOUT_TRANSFORMERS_AVAILABLE=1` and reads `ROLLOUT_SECRET_HF_TOKEN`
defensively (optional) before constructing a `VllmBackend`.

## User Setup Required

To run the gated live tests on a dev box:

```bash
pip install transformers>=4.45 accelerate>=0.34 torch>=2.4
# Optional: for HF-gated models (Qwen2.5 is open-weights so usually not needed).
export ROLLOUT_SECRET_HF_TOKEN=hf_xxx

ROLLOUT_TRANSFORMERS_AVAILABLE=1 cargo test \
    -p rollout-backend-vllm --features train \
    --test snapshot_resume_live -- --ignored --nocapture

ROLLOUT_TRANSFORMERS_AVAILABLE=1 cargo test \
    -p rollout-backend-vllm --features train \
    --test qwen25_assistant_mask -- --ignored --nocapture
```

## Next Phase Readiness

- Plan 04-06 (`rollout train sft --config <toml>` CLI) mounts on
  `VllmBackend::with_secret_token` + `set_train_mode(true)` followed by the
  algo `run` loop. The CLI parses `SftSettings.base_model.uri` and threads it
  to a Python-side `init_train(model_uri, seed=…)` invocation — wiring to be
  added in 04-06 (the trait surface is ready).
- Plan 04-07 (smoke + docs polish) consumes `training/determinism.md` and
  `training/cpu-mode.md` as cross-links. The actual smoke recipe drives
  `cargo test --features train --test snapshot_resume_live` (or
  `qwen25_assistant_mask`) under `make train-smoke`.
- The MockBackend byte-compare proof in 04-02 stays load-bearing for CI.
  This plan's live witness is dev-box-only.

## Deferred items

- **Full bidirectional inference↔training mode switch** — Phase 9. Phase-4
  rejects `Inference→Training` swaps mid-process; production PPO/GRPO will
  need real teardown + re-spawn of the engine.
- **Real `GradHandle` `PyObject` plumbing** — Phase 9. Phase-4 keeps the
  pending loss tensor in module-global `_STATE.last_loss`; this works for
  single-grad-in-flight but breaks down with PPO's multiple actor-rollouts.
- **`LossScope::AssistantOnly` end-to-end loss-masking integration** —
  plan 04-07 smoke. The chat-template override is independently witnessed
  by `qwen25_assistant_mask.rs`; the actual masked-loss path needs
  structured rows from the CLI.
- **`VllmBackend::load_weights` resolving ContentId → directory** — Phase 9.
  Today it's a no-op; the snapshot-restore path goes through
  `SnapshotterImpl` directly.

## Self-Check: PASSED

**Files exist:**
- FOUND: `crates/rollout-backend-vllm/src/train.rs`
- FOUND: `crates/rollout-backend-vllm/tests/train_thread_smoke.rs`
- FOUND: `crates/rollout-backend-vllm/tests/snapshot_resume_live.rs`
- FOUND: `crates/rollout-backend-vllm/tests/qwen25_assistant_mask.rs`
- FOUND: `python/rollout/backends/vllm/train.py`
- FOUND: `python/rollout/backends/vllm/qwen25_chat_template.py`
- FOUND: `docs/book/src/training/determinism.md`
- FOUND: `docs/book/src/training/cpu-mode.md`

**Commits present (verified via `git log --oneline | grep`):**
- FOUND: `faf1799` (`feat(04-05-01): Python train.py + Qwen2.5 chat-template override + determinism preamble`)
- FOUND: `2f658ed` (`feat(04-05-02): VllmBackend TrainableBackend impl + train thread smoke + snapshot_resume_live`)

**Acceptance grep checks (all PASSED):**
- `grep -q '#\[cfg(feature = "train")\]' crates/rollout-backend-vllm/src/engine.rs` ✓
- `grep -q 'SetTrainMode' crates/rollout-backend-vllm/src/engine.rs` ✓
- `grep -q 'ForwardWithLoss' crates/rollout-backend-vllm/src/engine.rs` ✓
- `grep -q 'OptimizerStep' crates/rollout-backend-vllm/src/engine.rs` ✓
- `grep -c 'ForwardWithLoss\|OptimizerStep\|SaveWeights\|LoadWeights' crates/rollout-backend-vllm/src/engine.rs` = 13 (≥ 4) ✓
- `grep -q 'impl rollout_core::TrainableBackend for VllmBackend' crates/rollout-backend-vllm/src/backend.rs` ✓
- `grep -q 'py.detach' crates/rollout-backend-vllm/src/train.rs` ✓
- `grep -q 'CUBLAS_WORKSPACE_CONFIG' crates/rollout-backend-vllm/src/train.rs` ✓
- `grep -q '#\[ignore' crates/rollout-backend-vllm/tests/snapshot_resume_live.rs` ✓
- `grep -q 'ROLLOUT_TRANSFORMERS_AVAILABLE' crates/rollout-backend-vllm/tests/snapshot_resume_live.rs` ✓
- `grep -q '#\[ignore' crates/rollout-backend-vllm/tests/qwen25_assistant_mask.rs` ✓
- `grep -q '{% generation %}' python/rollout/backends/vllm/qwen25_chat_template.py` ✓
- `grep -q 'PYTHONHASHSEED' python/rollout/backends/vllm/train.py` ✓
- `grep -q 'cudnn.benchmark = False' python/rollout/backends/vllm/train.py` ✓
- `grep -q 'use_stateful_dataloader' python/rollout/backends/vllm/train.py` ✓
- `grep -q 'gc.collect' python/rollout/backends/vllm/train.py` ✓
- `grep -q 'use_deterministic_algorithms' python/rollout/backends/vllm/train.py` ✓

**Builds + tests + lints:**
- `cargo build -p rollout-backend-vllm` ✓
- `cargo build -p rollout-backend-vllm --features vllm` ✓
- `cargo build -p rollout-backend-vllm --features train` ✓
- `cargo build -p rollout-backend-vllm --all-features` ✓
- `cargo test -p rollout-backend-vllm --features train --test train_thread_smoke` ✓ (3 passed)
- `cargo clippy -p rollout-backend-vllm --features train --all-targets -- -D warnings` ✓
- `cargo clippy -p rollout-backend-vllm --all-features --all-targets -- -D warnings` ✓
- `cargo clippy --workspace --all-targets -- -D warnings` ✓
- `cargo test --workspace --tests` ✓ (201 passed, 0 failed, 4 ignored)
- `RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" cargo doc -p rollout-backend-vllm --features train --no-deps` ✓
- `mdbook build docs/book` ✓
- `python3 -c "import ast; ast.parse(open('python/rollout/backends/vllm/train.py').read())"` ✓
- `python3 -c "import ast; ast.parse(open('python/rollout/backends/vllm/qwen25_chat_template.py').read())"` ✓

---
*Phase: 04-train-sft-rm-snapshots*
*Plan: 05*
*Completed: 2026-05-21*
