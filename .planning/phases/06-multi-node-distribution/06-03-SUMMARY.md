---
phase: 06-multi-node-distribution
plan: 03
subsystem: coordinator
tags: [coordinator, stateless-replayer, restart, spot-drain, preemption, lease, epoch, nack, snapshot]

# Dependency graph
requires:
  - phase: 06-multi-node-distribution
    plan: 00
    provides: "WorkItemRecord CAS state machine (try_claim/try_complete/try_repending + work_key), work namespace, Sim harness + assert_all_done_exactly_once"
  - phase: 06-multi-node-distribution
    plan: 01
    provides: "StorageLease (try_acquire/renew/current) + monotonic epoch + epoch::current_epoch + fence_old_coordinator"
  - phase: 06-multi-node-distribution
    plan: 02
    provides: "ledger dispatch queue + steal protocol (the in-flight ledger the replayer reconstructs)"
  - phase: 05-cloud-layer
    provides: "ComputeHint::preemption_signal (120s AWS / 30s GCP) + Queue::nack"
provides:
  - "run::replay_and_serve — lease-gated stateless-replayer boot (acquire -> adopt epoch -> scan_bytes work ledger -> reconstruct in-flight without requeue -> ReplayState)"
  - "run::spawn_lease_renew_loop — renew at heartbeat cadence; lost renew -> fence_old_coordinator + std::process::abort"
  - "drain::{DrainConfig (aws/gcp two-number), SnapshotPlan, drain, StopPullFlag} — spot-drain state machine on ComputeHint::preemption_signal (coord ↛ cloud)"
  - "BatchWorker stop_pull flag + poll_preemption + run-loop stop-pull observance"
  - "coord_restart_no_duplicates (SC2) + spot_drain_completes_within_lead_time (SC3) witnesses"
affects: [06-04-smoke-cli-pg-lane]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Stateless replayer: reconstruct in-flight from Storage on boot, never persist the assignment map in memory across restart"
    - "Reconstruct-not-requeue: Running rows are reconstructed in-flight on boot; only the failure_scan stale path re-pends (Pitfall 4)"
    - "Two-number spot discipline: notice lead (cloud signal) vs drain deadline (test bound) as distinct DrainConfig fields"
    - "Drain orchestrator consumes the preemption signal via the ComputeHint trait only — no rollout-cloud-* import (coord ↛ cloud)"
    - "Struct-of-params (SnapshotPlan) to keep drain() under the clippy too-many-arguments bound"

key-files:
  created:
    - crates/rollout-coordinator/src/drain.rs
    - crates/rollout-coordinator/tests/coord_restart.rs
    - crates/rollout-coordinator/tests/spot_drain.rs
  modified:
    - crates/rollout-coordinator/src/run.rs
    - crates/rollout-coordinator/src/lib.rs
    - crates/rollout-coordinator/Cargo.toml
    - crates/rollout-runtime-batch/src/worker.rs
    - .planning/REQUIREMENTS.md
    - .planning/ROADMAP.md
    - docs/specs/05-distribution.md

key-decisions:
  - "replay_and_serve returns ReplayState (epoch + in_flight map + pending list + terminal count) as the boot-decision half, separable from socket binding so the Sim harness drives it in-process"
  - "Lost-renew self-fence wired into run() via spawn_lease_renew_loop; reuses 06-01 fence_old_coordinator (event-only, then abort) — no new fence logic"
  - "Drain stop-pull flag lives in rollout-runtime-batch as a plain Arc<AtomicBool> (not the coordinator's StopPullFlag type) to preserve runtime-batch ↛ coordinator dep direction; coordinator exposes an identical StopPullFlag alias for its own call sites"
  - "SnapshotPlan struct bundles snapshotter/budget/cost/run_id/algorithm_id so drain() stays within the clippy arg-count gate"
  - "Cargo.lock chrono dev-dep committed as a separate chore so the D-SPOT-04 commit stays docs-only (no code files)"

requirements-completed: [DIST-03, DIST-04]

# Metrics
duration: 15min
completed: 2026-05-29
---

# Phase 6 Plan 03: Restart Replayer + Spot Drain Summary

**Stateless-replayer coordinator boot (`replay_and_serve`: win the lease, adopt the advanced epoch, `scan_bytes` the work ledger and reconstruct in-flight assignments WITHOUT blindly requeuing) plus a lease-renew/self-fence loop, and a `ComputeHint`-driven spot-drain state machine (`drain`: stop-pull → nack in-flight → opportunistic TrainState snapshot → deregister → exit, within the conservative 60s/15s drain deadline) — proven by `coord_restart_no_duplicates` (SC2, every work_id Done exactly once across a kill+restart with idempotent replayed acks) and `spot_drain_completes_within_lead_time` (SC3, AWS + GCP), with the notice-lead-vs-drain-deadline two-number distinction reconciled across REQUIREMENTS / ROADMAP / spec 05 (D-SPOT-04).**

## Performance

- **Duration:** ~15 min
- **Started:** 2026-05-29T16:04:41Z
- **Completed:** 2026-05-29T16:20:13Z
- **Tasks:** 3
- **Files modified:** 11 (3 created, 8 modified)

## Accomplishments
- **Stateless-replayer boot (DIST-03).** `run::replay_and_serve` factors the lease-gated boot decision (06-RESEARCH §5) out of socket binding: `try_acquire` (loser exits cleanly per spec 05 §8) → adopt the advanced `epoch` → `scan_bytes(namespace="work")`, building a `ReplayState { epoch, in_flight, pending, terminal }`. `Running` rows are reconstructed into the in-flight map but NOT requeued (Pitfall 4 — the worker may still hold them; only the `failure_scan` stale path re-pends); `Pending` rows are collected for dispatch; `Done`/`Failed` are skipped (idempotent). `run()` wires this before serving and adds `spawn_lease_renew_loop` (renew at heartbeat cadence; a lost renew, i.e. the epoch advanced under us, fires `fence_old_coordinator` then `std::process::abort()` — D-FENCE-01..03).
- **`coord_restart_no_duplicates` (SC2).** Sim with 3 workers + 12 items runs to partial progress (half acked, half left Running with their acks buffered); the first replayer's state is dropped ("kill"); a fresh replayer boots over the SAME storage (stateless — nothing held across the gap); the buffered acks replay as CAS `Running→Done` keyed on the deterministic `work_id` (idempotent — a second replay of an already-`Done` ack returns `false`); `assert_all_done_exactly_once` confirms every work_id reached `Done` once. Plus `replay_reconstructs_in_flight` (Running reconstructed, NOT requeued; X still Running(w1) on disk after boot) and `replayer_adopts_advanced_epoch` (boots at the stolen-advanced epoch and stamps it).
- **Spot-drain state machine (DIST-04).** `drain::DrainConfig` carries the two numbers as distinct fields — `aws()` = notice 120s / deadline 60s, `gcp()` = notice 30s / deadline 15s (D-SPOT-01). `drain(hint, queue, in_flight, SnapshotPlan, cfg, stop_pull, deregister)` runs the D-SPOT-02 sequence inside `tokio::time::timeout(cfg.drain_deadline, …)`: set the stop-pull flag → `queue.nack` each in-flight id (lease nack → Pending) → opportunistic `TrainState` snapshot iff a snapshotter is present and `remaining_budget >= snapshot_cost` (D-SPOT-03, TrainState ONLY) → run the `deregister` closure → return Ok (binary edge exits 0). The preemption signal is read ONLY through the `ComputeHint` trait — no `rollout-cloud-*` import (coord ↛ cloud preserved, dep-direction lint 14/14).
- **Worker drain edge.** `BatchWorker` gains a shared `stop_pull: Arc<AtomicBool>`, a `stop_pull_flag()` accessor, and `poll_preemption(&dyn ComputeHint)` (sets stop-pull on a `Some` lead); the `run_loop` checks the flag at the top of each cycle and leaves `running` so the drain orchestrator can requeue + exit. The flag is a plain core `AtomicBool` (not the coordinator's `StopPullFlag` alias) so `rollout-runtime-batch ↛ rollout-coordinator` stays clean.
- **`spot_drain_completes_within_lead_time` (SC3).** Mock `ComputeHint`/`Queue`/`Snapshotter`; asserts ordering + completion within a compressed deadline for BOTH AWS and GCP variants. Plus `drain_requeues_in_flight` (every in-flight nacked), `drain_snapshot_skipped_when_budget_low` (budget < cost → no save, work still nacked), and `drain_uses_two_numbers` (notice lead != drain deadline).
- **D-SPOT-04 doc reconciliation.** REQUIREMENTS.md DIST-04, ROADMAP.md SC3, and `docs/specs/05-distribution.md` now all state BOTH the notice lead (120/30) AND the drain deadline (60/15) with the distinction explicit, naming `spot_drain_completes_within_lead_time` as the test bound.

## Task Commits

1. **Task 1: Stateless-replayer boot + coord_restart_no_duplicates (SC2)** - `d9cfcf9` (feat) — `replay_and_serve` + `spawn_lease_renew_loop` in run.rs; 3 witnesses.
2. **Task 2: Spot-drain state machine + spot_drain_completes_within_lead_time (SC3)** - `e38c45e` (feat) — drain.rs + worker.rs stop-pull; 4 witnesses; plus `815ed9d` (chore) for the Cargo.lock chrono dev-dep.
3. **Task 3: D-SPOT-04 doc reconciliation** - `8eee940` (docs) — three docs reconciled, docs-only.

**Plan metadata:** _(final commit)_ (docs: complete plan).

## Files Created/Modified
- `crates/rollout-coordinator/src/run.rs` - `ReplayState`, `work_prefix`, `replay_and_serve`, `spawn_lease_renew_loop`; replayer wired into `run()`.
- `crates/rollout-coordinator/src/drain.rs` - `DrainConfig` (aws/gcp), `SnapshotPlan`, `StopPullFlag`, `drain()` + a two-number unit test.
- `crates/rollout-coordinator/src/lib.rs` - `pub mod drain;`.
- `crates/rollout-coordinator/Cargo.toml` - `chrono` dev-dependency (Snapshot mock).
- `crates/rollout-coordinator/tests/coord_restart.rs` - SC2 witness + 2 supporting tests.
- `crates/rollout-coordinator/tests/spot_drain.rs` - SC3 witness + 3 supporting tests + mock ComputeHint/Queue/Snapshotter.
- `crates/rollout-runtime-batch/src/worker.rs` - `stop_pull` flag, `stop_pull_flag()`, `poll_preemption`, run-loop stop-pull check.
- `.planning/REQUIREMENTS.md`, `.planning/ROADMAP.md`, `docs/specs/05-distribution.md` - two-number reconciliation (D-SPOT-04).

## Decisions Made
- **`ReplayState` as the boot-decision return.** `replay_and_serve` returns the reconstructed `ReplayState` (epoch + in-flight map + pending list + terminal count) so the Sim harness can assert the reconstruct-without-requeue invariant without binding a socket; `run()` consumes it and proceeds to serve.
- **Reuse 06-01 fence, don't reinvent.** The lost-renew path calls the existing `fence_old_coordinator` (event-only, then `abort`) — the renew loop is the only new code; the fence decision/abort split from 06-01 is untouched.
- **Stop-pull flag in core types, not coordinator's.** The worker's flag is `Arc<AtomicBool>` from std, keeping `rollout-runtime-batch ↛ rollout-coordinator`; the coordinator exposes a parallel `StopPullFlag` alias for its own ergonomics. The two are structurally identical and interoperate at the smoke wiring (06-04).
- **`SnapshotPlan` struct.** Bundling snapshotter + budget + cost + run_id + algorithm_id keeps `drain()` within the clippy `too_many_arguments` gate (was 11 args) without hiding the semantics.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `SnapshotPlan` extraction to satisfy the clippy arg-count gate**
- **Found during:** Task 2
- **Issue:** The plan's literal `drain(...)` signature has 11 parameters; `clippy::too_many_arguments` (`-D warnings`, a merge gate) caps at 7.
- **Fix:** Bundled the five snapshot-related params (`snapshotter`, `remaining_budget`, `snapshot_cost`, `run_id`, `algorithm_id`) into a `SnapshotPlan<'_>` struct. Behaviour and the named witnesses are unchanged; the signature is documented.
- **Files modified:** `crates/rollout-coordinator/src/drain.rs`, `crates/rollout-coordinator/tests/spot_drain.rs`
- **Verification:** `cargo clippy --workspace --all-targets -- -D warnings` clean; all 4 spot_drain tests green.
- **Committed in:** `e38c45e` (Task 2 commit)

**2. [Rule 3 - Blocking] Cargo.lock chrono dev-dep committed separately to keep the D-SPOT-04 commit docs-only**
- **Found during:** Task 3 (commit boundary)
- **Issue:** The spot_drain mock `Snapshotter` builds `Snapshot.created_at: DateTime<Utc>`, requiring a `chrono` dev-dep on `rollout-coordinator`; the resulting `Cargo.lock` change was an uncommitted leftover when reaching the docs-only Task 3, which the acceptance criterion requires to touch no code files.
- **Fix:** Committed `Cargo.lock` as a `chore(06-03)` tied to Task 2's dependency wiring before the docs commit, so `8eee940` is cleanly docs-only.
- **Files modified:** `Cargo.lock`, `crates/rollout-coordinator/Cargo.toml` (Cargo.toml was in the Task 2 commit)
- **Verification:** Task 3 commit `8eee940` touches only `.planning/*` + `docs/specs/05-distribution.md`.
- **Committed in:** `815ed9d` (chore)

Several trivial clippy fixes during normal TDD iteration (not deviations): `CoordEpoch` has no `Default` (constructed `ReplayState` fields explicitly), `items_after_statements` (moved a `use` to the top), `too_many_lines` on `run()` (extracted `spawn_lease_renew_loop`), and three `doc_markdown` backtick additions on `TrainState`. All resolved inline before the task commits.

---

**Total deviations:** 2 auto-fixed (both blocking — clippy arg-count gate + commit-boundary hygiene).
**Impact on plan:** Cosmetic/structural only. The plan's named functions (`replay_and_serve`, `drain`), witnesses (`coord_restart_no_duplicates`, `spot_drain_completes_within_lead_time`), and every acceptance grep are satisfied verbatim; `SnapshotPlan` is a parameter grouping, not a behaviour change.

## Issues Encountered
None beyond the auto-fixed deviations above.

## Known Stubs
- **Drain ↔ worker wiring not yet end-to-end.** `BatchWorker::poll_preemption` sets the stop-pull flag and `drain()` runs the full requeue/snapshot/deregister sequence, but the two are not yet joined at a live worker run-loop edge in a binary (the worker run loop is the batch runtime's; the coordinator's `drain` takes the `in_flight` ids + `deregister` closure as inputs). The end-to-end join (a worker polling its `ComputeHint`, gathering its in-flight ids, and invoking `drain`) lands in 06-04's 3-node smoke. This is intentional per CONTEXT (transport/smoke wiring is 06-04 scope); the dedup-critical and timing-critical logic — the drain state machine, the two-number budget, the stop-pull observance, and the idempotent nack/requeue — is fully implemented and witnessed here.
- **Epoch proto field still deferred to 06-04** (carried from 06-01) — `replay_and_serve` adopts and the lease stamps the epoch, but the proto field carrying it to workers lands with the smoke wiring.

## User Setup Required
None — all witnesses run Docker-free on the embedded redb path with mock cloud traits.

## Next Phase Readiness
- 06-04 (smoke + CLI + PG lane) can: drive `replay_and_serve` across a real coordinator kill+restart in the 3-node smoke; join `BatchWorker::poll_preemption` → gather in-flight → `drain` at the worker run-loop edge; wire the epoch proto field; exercise the `--test-fence` abort edge; and run the Postgres lease through the `postgres-integration` lane. The `work`/`epoch`/`coordinator_lease`/`queue_items` namespaces and the `WorkItemRecord` CAS + `StorageLease` + `drain` contracts are all stable.

---
*Phase: 06-multi-node-distribution*
*Completed: 2026-05-29*

## Self-Check: PASSED

All 3 created files + SUMMARY present; all task commits (`d9cfcf9`, `e38c45e`, `815ed9d`, `8eee940`) found in git log. SC2 `coord_restart_no_duplicates` + SC3 `spot_drain_completes_within_lead_time` exit 0; dep-direction 14/14; `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo test --workspace --tests` all green (0 failures).
