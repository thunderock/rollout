---
phase: 06-multi-node-distribution
plan: 02
subsystem: coordinator
tags: [coordinator, work-stealing, cas, dedup, ledger, dispatch-queue, ulid, redb]

# Dependency graph
requires:
  - phase: 06-multi-node-distribution
    plan: 00
    provides: "WorkItemRecord CAS state machine (try_claim/try_complete/try_repending + work_key), work/queue_items storage namespaces, Sim + CountingEmitter test support"
  - phase: 06-multi-node-distribution
    plan: 01
    provides: "StorageLease authority + authoritative epoch namespace (the steal/ledger epoch)"
provides:
  - "ledger::{enqueue, next_pending, dispatch} — queue_items ULID-ordered dispatch queue over WorkItemRecord"
  - "ledger::{backlog, busiest} — per-worker Running accounting for steal victim selection"
  - "steal::handle_steal_request + MAX_STEAL_BATCH — coordinator-mediated ceil(n/2)-capped CAS reassign"
  - "concurrent_ack_and_steal_no_double_execute (SC5) witness: stolen-then-reclaimed item never double-executes"
affects: [06-03-restart-replayer-spot-drain, 06-04-smoke-cli-pg-lane]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Two-step CAS reassign (try_repending then try_claim) over the SAME prior Running(victim) bytes — the dedup is expected-bytes drift, not a lock"
    - "Dispatch-queue mirror of InMemQueue: ULID-keyed queue_items rows, scan_bytes replays in insertion order"
    - "Deterministic content-addressed work_id (blake3 of payload) so dispatch + ack share one CAS key"

key-files:
  created:
    - crates/rollout-coordinator/src/ledger.rs
    - crates/rollout-coordinator/src/steal.rs
    - crates/rollout-coordinator/tests/steal_dedup.rs
  modified:
    - crates/rollout-coordinator/src/lib.rs
    - docs/specs/05-distribution.md

key-decisions:
  - "Reads happen on &dyn Storage, writes on the txn: StorageTxn is write-only (no get_bytes), so dispatch peeks the queue via scan_bytes then stages put/claim/delete in the caller's txn"
  - "Per-item txn in handle_steal_request: each try_repending+try_claim reassign is its own transaction so one lost CAS skips one item without aborting the whole batch"
  - "Lost re-claim rolls back the repend (abort) rather than stranding an item Pending — a later steal/dispatch retries it"
  - "busiest() tie-breaks on WorkerId so victim selection is deterministic"
  - "MAX_STEAL_BATCH=32 hardcoded (D-STEAL); not a config knob for v1.1"

requirements-completed: [DIST-02]

# Metrics
duration: 6min
completed: 2026-05-29
---

# Phase 6 Plan 02: Work Ledger + Stealing Summary

**Coordinator-mediated work-stealing on top of the `WorkItemRecord` CAS state machine: a `queue_items` ULID-ordered dispatch queue with content-addressed `work_id`s, per-worker `Running` backlog accounting that drives busiest-victim selection, and a `handle_steal_request` that reassigns `ceil(victim_backlog/2)` items (capped at `MAX_STEAL_BATCH=32`) from the busiest peer to an idle thief via a two-step `try_repending`→`try_claim` CAS — proven race-safe by the `concurrent_ack_and_steal_no_double_execute` witness (SC5, 100 iterations, Docker-free).**

## Performance

- **Duration:** ~6 min
- **Started:** 2026-05-29T15:51:52Z
- **Completed:** 2026-05-29T15:57:33Z
- **Tasks:** 3
- **Files modified:** 5 (3 created, 2 modified)

## Accomplishments
- **Dispatch queue (`ledger.rs`).** `enqueue` writes a `queue_items` row keyed by a fresh monotonic `Ulid` (`["q", <ulid>]`); `next_pending` / `dispatch` replay payloads in ULID (insertion) order via `scan_bytes` (mirrors `InMemQueue`). `dispatch` content-addresses the payload into a deterministic `work_id` (blake3, CORE-05), writes a `Pending` `WorkItemRecord`, `try_claim`s it for the worker, and on a winning CAS deletes the queue entry — leaving the entry intact if the claim loses so no work is dropped.
- **Backlog accounting.** `backlog(worker)` counts that worker's `Running` items in the `work` namespace; `busiest(exclude)` returns the peer with the most `Running` items (deterministic `WorkerId` tie-break), driving steal victim selection (D-STEAL-03).
- **Steal protocol (`steal.rs`).** `handle_steal_request(thief)` = idle-guard (D-STEAL-01: a non-idle thief is a no-op) → busiest victim → `n = min(victim_backlog.div_ceil(2), MAX_STEAL_BATCH)` (D-STEAL-02) → per-item two-step CAS reassign `try_repending(victim)` then `try_claim(thief)` (D-STEAL-04). A lost `try_repending` (victim acked first) skips the item; a lost re-claim rolls back the repend. `MAX_STEAL_BATCH = 32` is fixed for v1.1.
- **SC5 witness (`steal_dedup.rs`).** `concurrent_ack_and_steal_no_double_execute` races the victim's ack (`try_complete`, `Running→Done`) against the steal's `try_repending` (`Running→Pending`) over the SAME `Running(victim)` bytes, asserting `wins == 1` and a winner-consistent final state across 100 iterations on a 4-thread runtime; `final_state_consistent` asserts exactly one ledger row in exactly one state — never two `Running` owners, never double `Done`.
- **Spec.** `docs/specs/05-distribution.md` §5 now documents the coordinator-mediated steal + CAS-dedup invariant (trigger / victim / batch / reassign + the exactly-one-winner argument).

## Task Commits

1. **Task 1: Ledger dispatch queue + backlog accounting** - `4762760` (feat) — TDD; 3 inline tests (`queue_items_fifo_ulid_order`, `backlog_count_by_worker`, `dispatch_claims_pending`).
2. **Task 2: Steal protocol + CAS reassign + MAX_STEAL_BATCH** - `9fcff9a` (feat) — TDD; 3 inline tests (`steal_takes_ceil_half_capped`, `steal_only_when_local_empty`, `steal_reassigns_via_cas`).
3. **Task 3: concurrent_ack_and_steal_no_double_execute witness (SC5)** - `b8d606d` (test) — 2 tests + spec §5 dedup invariant.

**Plan metadata:** _(final commit)_ (docs: complete plan).

## Files Created/Modified
- `crates/rollout-coordinator/src/ledger.rs` - dispatch queue (enqueue/next_pending/dispatch) + backlog/busiest + 3 inline tests.
- `crates/rollout-coordinator/src/steal.rs` - `handle_steal_request` + `MAX_STEAL_BATCH` + 3 inline tests.
- `crates/rollout-coordinator/tests/steal_dedup.rs` - SC5 witness + `final_state_consistent` (`mod support;`).
- `crates/rollout-coordinator/src/lib.rs` - `pub mod ledger; pub mod steal;`.
- `docs/specs/05-distribution.md` - §5 coordinator-mediated steal + CAS dedup invariant.

## Decisions Made
- **Reads on `Storage`, writes on the txn.** `StorageTxn` is write-only (`put_bytes`/`delete`/`cas_bytes`/`commit`/`abort` — no `get_bytes`). `next_pending`/`dispatch` peek the queue via `storage.scan_bytes` (sorted on the ULID path segment to guard against backend ordering drift) and stage the `put`/`claim`/`delete` in the caller's transaction.
- **Per-item transaction in the steal loop.** Each `try_repending`+`try_claim` reassign is its own txn so one lost CAS skips one item rather than poisoning the whole batch; a lost re-claim aborts (rolls back the repend) so an item is never stranded `Pending`.
- **Deterministic `busiest` tie-break** on `WorkerId` so victim selection is stable across runs.
- **`MAX_STEAL_BATCH = 32` hardcoded** (D-STEAL — deferred config knob), documented in-code as "fixed for v1.1; revisit if a tuning need emerges."

## Deviations from Plan

None — plan executed exactly as written. The one structural shape the plan left to discretion (where the queue read happens, given `StorageTxn` is write-only) was handled by reading via `&dyn Storage` before staging writes in the txn; this matches the `InMemQueue` template and required no deviation from the plan's named functions, tests, or acceptance greps.

## Issues Encountered
Two trivial compile fixes during normal TDD iteration (not deviations): `StorageTxn` has no `get_bytes` (refactored `dispatch` to carry the payload out of the scan), and `work_key` was only used in `steal.rs` tests (moved its import into the test module to keep `-D warnings` clean). Both resolved inline before the task commit.

## Known Stubs
None. The steal protocol's transport wiring (the `steal_request` RPC on the existing Control/Work channels) is out of this plan's scope per CONTEXT — 06-04 wires the coordinator-side `handle_steal_request` to the proto surface. The dedup-critical logic (ledger, victim selection, CAS reassign, the SC5 race) is fully implemented and witnessed here.

## User Setup Required
None — all witnesses run Docker-free on the embedded redb path.

## Next Phase Readiness
- 06-03 (restart replayer + spot drain) can reuse `ledger::{backlog, busiest, next_pending}` for boot-time ledger replay and `dispatch` for re-enqueue; the `work`/`queue_items` namespaces and the `WorkItemRecord` CAS contract are stable.
- 06-04 (smoke + CLI + PG lane) wires `handle_steal_request` to the steal RPC on the mTLS transport and exercises the dispatch queue end-to-end in the 3-node smoke.

---
*Phase: 06-multi-node-distribution*
*Completed: 2026-05-29*

## Self-Check: PASSED

All 3 created files + spec edit + SUMMARY present; all 3 task commits (`4762760`, `9fcff9a`, `b8d606d`) found in git log. 19 coordinator tests green incl. SC5 `concurrent_ack_and_steal_no_double_execute` (100 iterations); dep-direction 14/14; clippy `-D warnings` clean.
