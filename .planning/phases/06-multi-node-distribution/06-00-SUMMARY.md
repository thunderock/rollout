---
phase: 06-multi-node-distribution
plan: 00
subsystem: infra
tags: [coordinator, lease, epoch, cas, work-stealing, test-harness, redb, postcard, tokio]

# Dependency graph
requires:
  - phase: 02-local-substrate
    provides: "rollout-core StorageTxn::cas_bytes, EmbeddedStorage, rollout-coordinator skeleton, EventEmitter"
  - phase: 03-inference-batch
    provides: "SampleRecord CAS state machine (the template WorkItemRecord mirrors)"
provides:
  - "CoordinatorLease trait + CoordEpoch + LeaseRecord in rollout-core (pure, no SDK)"
  - "WorkItemRecord CAS-on-state module (try_claim/try_complete/try_repending) in rollout-coordinator"
  - "Phase-6 storage namespaces registered in embedded redb (work/coordinator_lease/epoch/queue_items)"
  - "In-process Sim harness (1-coord + N-worker) + CountingEmitter over EmbeddedStorage"
  - "Subprocess abort harness (run_fence_subprocess) for the split_brain witness"
affects: [06-01-lease-epoch, 06-02-ledger-steal, 06-03-replayer-drain, distribution-witnesses]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Interface-first ordering: trait + harness shapes defined before downstream impls"
    - "Test-support tree via #[path = \"support/mod.rs\"] mod support; (subdir not auto-compiled)"
    - "Subprocess isolation for in-process-fatal std::process::abort() (CARGO_BIN_EXE_*)"

key-files:
  created:
    - crates/rollout-core/src/traits/lease.rs
    - crates/rollout-coordinator/src/work_item.rs
    - crates/rollout-coordinator/tests/support/mod.rs
    - crates/rollout-coordinator/tests/support/sim.rs
    - crates/rollout-coordinator/tests/support/abort_harness.rs
    - crates/rollout-coordinator/tests/support_smoke.rs
  modified:
    - crates/rollout-core/src/traits/mod.rs
    - crates/rollout-core/src/lib.rs
    - crates/rollout-coordinator/src/lib.rs
    - crates/rollout-storage/src/embedded/tables.rs
    - crates/rollout-storage/src/embedded/mod.rs

key-decisions:
  - "DUPLICATE the SampleRecord CAS shape into rollout-coordinator (no new shared crate / dep edge)"
  - "Register all four Phase-6 namespaces now (work/coordinator_lease/epoch/queue_items) so 06-01..03 need no storage edits"
  - "CoordinatorLease lives in rollout-core with zero SDK imports; single StorageLease impl deferred to 06-01"
  - "Subprocess abort harness over CARGO_BIN_EXE_rollout-coordinator; in-process witness asserts decision+event only"

patterns-established:
  - "CAS on exact prior bytes (postcard-encode the decoded input record as `expected`) — Pitfall 2"
  - "Object-safe async traits via #[async_trait], asserted by a `fn _assert(_: &dyn Trait)` compile test"
  - "CountingEmitter as an observability-only sink (no shared-state write) for fence assertions"

requirements-completed: [DIST-01, DIST-02, DIST-03, DIST-04, DIST-05]

# Metrics
duration: 8min
completed: 2026-05-29
---

# Phase 6 Plan 00: Wave-0 Test Infra + Lease Trait Summary

**CoordinatorLease trait + CoordEpoch/LeaseRecord (pure core, zero SDK leak), a duplicated WorkItemRecord CAS state machine with single-winner + idempotent-repending witnesses, and the in-process Sim + subprocess-abort test harnesses the four Phase-6 distribution witnesses build against.**

## Performance

- **Duration:** 8 min
- **Started:** 2026-05-29T15:31:42Z
- **Completed:** 2026-05-29T15:39:22Z
- **Tasks:** 3
- **Files modified:** 11 (6 created, 5 modified)

## Accomplishments
- `CoordinatorLease` trait + `CoordEpoch(u64)` + `LeaseRecord { holder, epoch, expires_at_ms }` land in `rollout-core` — object-safe, postcard-round-trip-stable, monotonically ordered epochs, no cloud SDK import (dep-direction lint stays 14/14).
- Shared `WorkItemRecord` CAS-on-state module (`Pending → Running → Done | Failed`) mirroring `SampleRecord`, with `try_claim`/`try_complete`/`try_repending` over `cas_bytes`; single-winner second-claim-loses and idempotent-repending-on-Done witnessed.
- Embedded redb backend now recognises the four Phase-6 namespaces (`work`, `coordinator_lease`, `epoch`, `queue_items`) so downstream lease/ledger/queue plans need no storage edits.
- In-process `Sim` harness (1-coordinator + N-worker over `EmbeddedStorage`) with seed/scan/`assert_all_done_exactly_once` dedup gate, a `CountingEmitter` topic-counting sink, and a `std::process::Command`-based abort harness for the split-brain `process::abort()` witness — all proven by a 3-test smoke binary.

## Task Commits

Each task was committed atomically:

1. **Task 1: CoordinatorLease trait + epoch types** - `f0814cf` (feat) — TDD; types-only impl made the 3 RED tests pass immediately (trait has no behavior to RED against).
2. **Task 2: WorkItemRecord CAS-on-state module** - `3883f75` (feat) — includes Rule 3 storage-namespace fix.
3. **Task 3: Sim + CountingEmitter + abort harness** - `596453d` (test).

**Plan metadata:** _(this commit)_ (docs: complete plan)

## Files Created/Modified
- `crates/rollout-core/src/traits/lease.rs` - `CoordEpoch`, `LeaseRecord`, `CoordinatorLease` trait + 3 inline tests.
- `crates/rollout-core/src/traits/mod.rs` - `pub mod lease;` + re-exports.
- `crates/rollout-core/src/lib.rs` - crate-root re-export of the lease types.
- `crates/rollout-coordinator/src/work_item.rs` - `WorkState`/`WorkItemRecord` + `work_key` + 3 CAS helpers + 3 inline tests.
- `crates/rollout-coordinator/src/lib.rs` - `pub mod work_item;`.
- `crates/rollout-storage/src/embedded/tables.rs` - 4 new `BytesTable` consts + `table_for` arms + `all_tables` bump to 12.
- `crates/rollout-storage/src/embedded/mod.rs` - `get_many_bytes` namespace match arms for the 4 new namespaces (+ latent `snapshots` gap).
- `crates/rollout-coordinator/tests/support/mod.rs` - `CountingEmitter` + module re-exports.
- `crates/rollout-coordinator/tests/support/sim.rs` - `Sim` harness + dedup assertion.
- `crates/rollout-coordinator/tests/support/abort_harness.rs` - `run_fence_subprocess` + `coordinator_bin()`.
- `crates/rollout-coordinator/tests/support_smoke.rs` - 3-test harness smoke.

## Decisions Made
- **Duplicate, don't extract.** Mirrored the `SampleRecord` CAS shape into `rollout-coordinator::work_item` rather than extracting a shared crate — avoids a new crate + a dependency-direction edge for ~80 lines, and the coordinator's `try_claim` deliberately drops the batch runtime's staleness-reclaim param (06-RESEARCH §4 "extract-vs-duplicate").
- **Register all Phase-6 namespaces up front** (`work`/`coordinator_lease`/`epoch`/`queue_items`) so plans 06-01..03 add no storage code.
- **Lease trait in core, impl deferred.** Defined the pure trait/types only; the single `StorageLease` over `Arc<dyn Storage>` lands in 06-01 (keeps `coord ↛ cloud` green and matches the existing `kv`-row design).
- **Abort isolation via subprocess.** `run_fence_subprocess` shells the compiled binary via `CARGO_BIN_EXE_rollout-coordinator`; the in-process witness will assert only the fence *decision* + single event (the actual `abort()` is fatal in-process — Pitfall 5).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Registered Phase-6 storage namespaces in the embedded backend**
- **Found during:** Task 2 (WorkItemRecord CAS module)
- **Issue:** `EmbeddedStorage` maps each `StorageKey.namespace` to a redb `TableDefinition` via an allowlist (`table_for`); the `work` namespace was unregistered, so every `try_claim`/`put_bytes` failed with `Fatal(ConfigInvalid { "unknown storage namespace: work" })` and all three Task-2 tests panicked.
- **Fix:** Added `T_COORD_LEASE`/`T_EPOCH`/`T_WORK`/`T_QUEUE_ITEMS` table consts, the matching `table_for` arms, bumped `all_tables()` to length 12, and added the four arms (plus the previously-missing `snapshots` arm) to the `get_many_bytes` static-namespace match in `embedded/mod.rs`.
- **Files modified:** `crates/rollout-storage/src/embedded/tables.rs`, `crates/rollout-storage/src/embedded/mod.rs`
- **Verification:** `cargo test -p rollout-coordinator --lib work_item` 3/3 green; `cargo test -p rollout-storage --tests` all green (no regression from the `all_tables` arity change).
- **Committed in:** `3883f75` (Task 2 commit)

**2. [Rule 1 - Bug] Backticked `work_id` in a sim.rs doc comment (clippy::doc_markdown)**
- **Found during:** Task 3 (Sim harness)
- **Issue:** `cargo clippy --all-targets -- -D warnings` flagged a bare `work_id` in the `sim.rs` module rustdoc as missing backticks, failing the merge-blocking lint.
- **Fix:** Wrapped `work_id` in backticks; the acceptance-grep phrase "Done exactly once" is preserved.
- **Files modified:** `crates/rollout-coordinator/tests/support/sim.rs`
- **Verification:** `cargo clippy -p rollout-coordinator --all-targets -- -D warnings` clean.
- **Committed in:** `596453d` (Task 3 commit)

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 bug)
**Impact on plan:** Both essential — the namespace registration is required for any Phase-6 storage write (and pre-empts identical breakage in 06-01..03), and the clippy fix is a merge gate. No scope creep; the StorageLease impl and witness bodies remain deferred to their planned plans.

## Issues Encountered
None beyond the auto-fixed deviations above.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- All Wave-0 contracts shipped: 06-01 (lease/epoch) can impl `StorageLease` against the trait; 06-02 (ledger/steal) can reuse `work_item`; 06-03 (replayer/drain + split_brain) can `mod support;` and drive `Sim` + `run_fence_subprocess`.
- 06-03 must add a hidden `--test-fence` subcommand to the coordinator binary (the abort-subprocess entrypoint) — the harness plumbing is ready, the entrypoint is not (documented in `abort_harness.rs`).
- The four witness tests in 06-VALIDATION.md (`coord_restart_no_duplicates`, `concurrent_ack_and_steal_no_double_execute`, `spot_drain_*`, `split_brain_old_coord_self_fences`) remain ⬜ pending — Wave 0 only builds the substrate they compile against.

---
*Phase: 06-multi-node-distribution*
*Completed: 2026-05-29*

## Self-Check: PASSED

All 6 created files present; SUMMARY present; all 3 task commits (`f0814cf`, `3883f75`, `596453d`) found in git log.
