---
phase: 01-core-foundations
plan: 03
subsystem: core
tags: [rust, trait-surface, error-taxonomy, ids, jsonschema, schemars, blake3, ulid, async-trait, tdd]

requires:
  - "Cargo workspace skeleton (plan 01-01)"
  - "workspace.dependencies pins for serde/schemars/thiserror/async-trait/tracing/ulid/blake3 (plan 01-01)"
provides:
  - "All 19 traits from CORE-01 (PolicyAlgorithm, Worker, Coordinator, Scheduler, Plugin, PluginHost, EnvHarness, ToolHarness, EvalHarness, RewardModel, InferenceBackend, Storage, StorageTxn, Snapshotter, ObjectStore, SecretStore, ComputeHint, Queue, Clock)"
  - "CoreError taxonomy: Recoverable | Fatal + RetryHint with #[from] propagation (CORE-03)"
  - "ID types: RunId(Ulid), WorkerId(Ulid), ContentId(blake3 [u8; 32]) (CORE-05)"
  - "RunConfig type tree with JsonSchema + deny_unknown_fields (foundation for CORE-04)"
  - "Stub types WorkerContext + DrainReason (Phase 1 placeholders; full types in Phase 2)"
affects:
  - 01-04-schema-gen-pipeline (consumes RunConfig for schema_for!)
  - 01-05-dep-direction-and-deny (architecture-lint test depends on rollout-core)
  - Phase 2+ (every downstream crate consumes these traits/types)

tech-stack:
  added: []
  patterns:
    - "Two-level error enum: outer CoreError = Recoverable | Fatal with #[from], inner enums name leaf variants (RESEARCH Pattern 3)"
    - "Module-per-trait-family under traits/ (RESEARCH Architecture Patterns)"
    - "#[async_trait] for all I/O traits (object-safe), sync for Clock (RESEARCH Pattern 2 + exception)"
    - "#[serde(transparent)] on ULID newtypes for string-form serialization"
    - "ContentId stores raw [u8; 32] (not blake3::Hash) to avoid blake3 serde feature dep"
    - "Tagged enums (#[serde(tag = ...)]) for StorageConfig + AlgorithmConfig — schemars emits oneOf with discriminator"

key-files:
  created:
    - crates/rollout-core/src/ids.rs
    - crates/rollout-core/src/errors.rs
    - crates/rollout-core/src/traits/mod.rs
    - crates/rollout-core/src/traits/algorithm.rs
    - crates/rollout-core/src/traits/worker.rs
    - crates/rollout-core/src/traits/plugin.rs
    - crates/rollout-core/src/traits/harness.rs
    - crates/rollout-core/src/traits/backend.rs
    - crates/rollout-core/src/traits/storage.rs
    - crates/rollout-core/src/traits/cloud.rs
    - crates/rollout-core/src/traits/clock.rs
    - crates/rollout-core/src/config/mod.rs
    - crates/rollout-core/src/config/defaults.rs
    - crates/rollout-core/tests/id_types.rs
    - crates/rollout-core/tests/error_taxonomy.rs
    - crates/rollout-core/tests/trait_surface.rs
  modified:
    - crates/rollout-core/src/lib.rs

key-decisions:
  - "Clock kept sync (no async_trait) per RESEARCH Pattern 2 exception — clocks have no I/O"
  - "WorkerContext + DrainReason added as Phase 1 stub types in traits/worker.rs to preserve spec-shaped Worker signatures; full types arrive in Phase 2 (runtime substrate)"
  - "Worker trait uses &WorkerContext (no lifetime) matching docs/specs/01-core-runtime.md §2, not the plan's &WorkerContext<'_> sketch"
  - "schemars 1.2.1 #[schemars(range(min = 1, max = 1))] compiled without fallback — RESEARCH Open Question Q2 resolved positively"
  - "ContentId stores raw [u8; 32] rather than blake3::Hash to avoid leaking the blake3 serde feature dependency (RESEARCH Pattern 4)"
  - "All ID newtypes use #[serde(transparent)] so JSON form is a string, not an object"
  - "Test trait_surface.rs uses compile-time fn references (not #[test] bodies) for object-safety + Send+Sync assertions; one marker #[test] satisfies cargo test runner"
  - "not_serializable test uses include_bytes! self-inspection of errors.rs source instead of nightly negative trait bounds"

patterns-established:
  - "RED → GREEN within each TDD task: write tests/<feature>.rs first, confirm compile error, then implement"
  - "Per-pub-item one-line /// doc comments (AGENTS.md §9.3 / DOCS-03), no multi-paragraph docstrings"
  - "Module re-exports flow: src/<mod>/mod.rs -> pub mod <leaf>; pub use leaf::*; then lib.rs flat re-exports the surface"

requirements-completed: [CORE-01, CORE-03, CORE-05, DOCS-03]
requirements-foundation: [CORE-04]

duration: 5min
completed: 2026-05-19
---

# Phase 01 Plan 03: rollout-core content — Summary

**Trait surface (19 traits), error taxonomy (CoreError + RetryHint), ID types (RunId/WorkerId/ContentId), and RunConfig type tree all landed in rollout-core via TDD; rollout-core now compiles, tests, clippies, and rustdoc-gates cleanly.**

## Performance

- **Duration:** ~5 min (287s wall, three tasks, single-process)
- **Started:** 2026-05-19T22:35:43Z
- **Completed:** 2026-05-19T22:40:30Z
- **Tasks:** 3
- **Files created:** 16 (15 new files + 1 modified lib.rs)

## Accomplishments

- **CORE-05 (IDs):** `RunId(Ulid)`, `WorkerId(Ulid)`, `ContentId([u8; 32])`. All implement `Serialize + Deserialize + Display + FromStr`. `ContentId::of(b"")` matches the well-known blake3-empty vector.
- **CORE-03 (Errors):** `CoreError = Recoverable | Fatal` with `#[from]` propagation. `RecoverableError = Throttled | Transient | Preempted` (each carries `RetryHint`). `FatalError = ConfigInvalid | SchemaViolation | PluginContract | Internal`. `RetryHint = Never | After(Duration) | Backoff { base, max }`. No `Serialize` derive (Anti-Pattern 4 enforced by both grep + a runtime test that scans `errors.rs` source).
- **CORE-01 (Traits):** All 19 traits public from `rollout-core`, `Send + Sync`, object-safe (verified at compile time in `tests/trait_surface.rs`). Module layout matches RESEARCH §Architecture Patterns:
  - `algorithm.rs` → `PolicyAlgorithm`
  - `worker.rs` → `Worker`, `Coordinator`, `Scheduler` (+ stub `WorkerContext`, `DrainReason`)
  - `plugin.rs` → `Plugin`, `PluginHost`
  - `harness.rs` → `EnvHarness`, `ToolHarness`, `EvalHarness`, `RewardModel`
  - `backend.rs` → `InferenceBackend`
  - `storage.rs` → `Storage`, `StorageTxn`, `Snapshotter`
  - `cloud.rs` → `ObjectStore`, `SecretStore`, `ComputeHint`, `Queue`
  - `clock.rs` → `Clock` (sync)
- **CORE-04 foundation:** `RunConfig` with `schema_version: u32` (range 1..=1 via `#[schemars(range(...))]`), `RunMetadata`, tagged `StorageConfig` (`backend = embedded | postgres`), tagged `AlgorithmConfig` (`kind = sft | ppo`), `SftSettings`, `PpoSettings`. All derive `JsonSchema + Serialize + Deserialize + #[serde(deny_unknown_fields)]`. Plan 01-04 will consume this via `schemars::schema_for!(RunConfig)`.
- **DOCS-03:** Crate-level `//!` doc on `rollout-core` + one-line `///` on every `pub` item. `RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" cargo doc -p rollout-core --no-deps --all-features` exits 0.

## Task Commits

1. **Task 1: Wave 0 RED tests + ids.rs + errors.rs GREEN** — `87143f1` (feat)
2. **Task 2: All 19 traits + trait_surface.rs compile test** — `ee41907` (feat)
3. **Task 3: RunConfig type tree with JsonSchema** — `13cb09b` (feat)

## Files Created/Modified

| Path | Role |
|---|---|
| `crates/rollout-core/src/lib.rs` | Module declarations + flat re-exports of full surface |
| `crates/rollout-core/src/ids.rs` | `RunId`, `WorkerId`, `ContentId` + Display/FromStr/serde |
| `crates/rollout-core/src/errors.rs` | `CoreError`, `RecoverableError`, `FatalError`, `RetryHint` |
| `crates/rollout-core/src/traits/{mod,algorithm,worker,plugin,harness,backend,storage,cloud,clock}.rs` | 19-trait surface |
| `crates/rollout-core/src/config/{mod,defaults}.rs` | `RunConfig` tree + `defaults::schema_version()` |
| `crates/rollout-core/tests/id_types.rs` | 5 tests: Display/FromStr round-trip × 2, determinism, serde-json, known-vector |
| `crates/rollout-core/tests/error_taxonomy.rs` | 4 tests: variants_exist, from_propagation, display_formats, not_serializable |
| `crates/rollout-core/tests/trait_surface.rs` | Compile-time `Arc<dyn Trait>` + `Send + Sync` assertions × 19 + marker `#[test]` |

## Decisions Made

1. **Clock stays sync (no async_trait)** — RESEARCH §Pattern 2 explicitly carves Clock out; clocks have no I/O. All other I/O traits use `#[async_trait]` for `dyn`-compatibility.
2. **`WorkerContext` + `DrainReason` introduced as Phase 1 stubs in `traits/worker.rs`** — the spec (`docs/specs/01-core-runtime.md §2`) names these types in `Worker`'s signatures, but their full definitions belong to the runtime substrate (Phase 2). Stubs preserve the spec-shaped signatures so Phase 2 can replace them without changing trait method shapes.
3. **`&WorkerContext` (no lifetime) instead of the plan's `&WorkerContext<'_>` sketch** — matches the actual spec text in `docs/specs/01-core-runtime.md §2`. The lifetime parameter was an artifact of the planner reasoning about future generic contexts; not required in Phase 1.
4. **schemars `range(min = 1, max = 1)` works on schemars 1.2.1 without fallback** — RESEARCH Open Question Q2 resolved positively. No need for the planned fallback (a manual `validate_schema_version()` method).
5. **`ContentId` wraps `[u8; 32]`, not `blake3::Hash`** — avoids needing the `blake3` `serde` feature, keeps the dep surface minimal.
6. **`not_serializable` test scans `errors.rs` source via `include_bytes!`** — stable-Rust workaround for negative trait bounds. Belt-and-braces with the plan's grep check.
7. **Test functions in `trait_surface.rs` are not `#[test]`-annotated** — they're compile-time existence checks (object safety + Send+Sync). A single `#[test] fn trait_surface_counts_19` provides the runner-visible pass.
8. **Underscore-prefix on test helper fns removed after clippy `used_underscore_items` lint** — renamed `_assert_send_sync` → `assert_send_sync` and similar, no semantic change.

## Deviations from Plan

1. **[Rule 1 — Bug] Clippy `used_underscore_items` lint required renaming test helpers.** Plan's example code used `_assert_send_sync` and `_algorithm`/`_worker`/... fn names. With workspace `[lints.clippy] pedantic = warn` + `-D warnings`, calling underscore-prefixed items is rejected. Renamed to non-underscored names; behavior identical. Fixed inline in Task 2; no commit-on-commit churn.
2. **[Rule 1 — Bug] Clippy `doc_markdown` lint flagged `CoreError` (no backticks) in `tests/error_taxonomy.rs` `//!` line.** Wrapped in backticks. Fixed inline in Task 1.
3. **[Rule 2 — Critical] `not_serializable` test as planned used a blanket trait impl that asserted nothing.** Replaced with a source-inspection test using `include_bytes!("../src/errors.rs")` — catches `#[derive(...Serialize...)]` lines and `use serde` imports directly. This is the only stable-Rust mechanism for "must NOT impl X" without nightly negative bounds. Fixed inline in Task 1.

All three deviations are minor implementation polish driven by the workspace's strict lint posture (pedantic clippy + missing_docs + rustdoc gate). No architectural shifts.

## Test File Inventory + Pass Counts

| Test file | Tests | Result |
|---|---|---|
| `tests/id_types.rs` | 5 | ✅ all pass |
| `tests/error_taxonomy.rs` | 4 | ✅ all pass |
| `tests/trait_surface.rs` | 1 (marker; compile-time checks otherwise) | ✅ pass |
| **Total** | **10** | **10 / 10 ✅** |

Plus 0 unit tests inside crate (`#[cfg(test)] mod tests` — none authored in Phase 1 per plan).

## §9.3 Rustdoc Gate Confirmation

```bash
$ RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" \
    cargo doc -p rollout-core --no-deps --all-features
 Documenting rollout-core v0.1.0 (/Users/ashutosh/personal/rollout/crates/rollout-core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.45s
   Generated /Users/ashutosh/personal/rollout/target/doc/rollout_core/index.html
```

Exit 0. DOCS-03 gate passes for `rollout-core`.

## Issues Encountered

None blocking. Two clippy-pedantic surprises (underscore-item-use + doc-markdown) and one stable-Rust limitation (negative trait bounds) — all resolved inline within the originating task. No checkpoints, no auth gates.

## User Setup Required

None.

## Next Phase Readiness

- **Ready for plan 01-04 (schema-gen pipeline):** `RunConfig` is fully derived; `schemars::schema_for!(rollout_core::RunConfig)` will produce a valid JSON Schema 2020-12 document on first call. Plan 04 replaces the xtask stub.
- **Ready for plan 01-05 (dep-direction lint):** `rollout-core` depends only on `serde`, `serde_json`, `schemars`, `thiserror`, `async-trait`, `tracing`, `ulid`, `blake3` — all permissive licenses, no cloud SDKs. The architecture-lint test in plan 05 will baseline against this clean surface.
- **Phase 2 (runtime substrate):** Stub `WorkerContext` and `DrainReason` need to be replaced with real types when Phase 2 lands. Replace at the `src/traits/worker.rs` site; the `Worker` trait signature does not need to change.

No blockers, no concerns.

---
*Phase: 01-core-foundations*
*Completed: 2026-05-19*

## Self-Check: PASSED

Files verified (17/17):
- FOUND: crates/rollout-core/src/ids.rs
- FOUND: crates/rollout-core/src/errors.rs
- FOUND: crates/rollout-core/src/traits/mod.rs
- FOUND: crates/rollout-core/src/traits/algorithm.rs
- FOUND: crates/rollout-core/src/traits/worker.rs
- FOUND: crates/rollout-core/src/traits/plugin.rs
- FOUND: crates/rollout-core/src/traits/harness.rs
- FOUND: crates/rollout-core/src/traits/backend.rs
- FOUND: crates/rollout-core/src/traits/storage.rs
- FOUND: crates/rollout-core/src/traits/cloud.rs
- FOUND: crates/rollout-core/src/traits/clock.rs
- FOUND: crates/rollout-core/src/config/mod.rs
- FOUND: crates/rollout-core/src/config/defaults.rs
- FOUND: crates/rollout-core/tests/id_types.rs
- FOUND: crates/rollout-core/tests/error_taxonomy.rs
- FOUND: crates/rollout-core/tests/trait_surface.rs
- FOUND: .planning/phases/01-core-foundations/01-03-rollout-core-content-SUMMARY.md

Commits verified (3/3):
- FOUND: 87143f1 (Task 1 — ids + errors + TDD tests)
- FOUND: ee41907 (Task 2 — 19 traits + trait_surface test)
- FOUND: 13cb09b (Task 3 — RunConfig + JsonSchema)
