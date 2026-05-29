---
phase: 06-multi-node-distribution
plan: 02
type: execute
wave: 2
depends_on: ["06-00", "06-01"]
files_modified:
  - crates/rollout-coordinator/src/ledger.rs
  - crates/rollout-coordinator/src/steal.rs
  - crates/rollout-coordinator/src/lib.rs
  - crates/rollout-coordinator/tests/steal_dedup.rs
  - docs/specs/05-distribution.md
autonomous: true
requirements: [DIST-02]
must_haves:
  truths:
    - "An idle worker (local queue empty) steals ceil(victim_backlog/2) items capped at MAX_STEAL_BATCH from the busiest peer, coordinator-mediated"
    - "A stolen-then-reclaimed work item never double-executes: a concurrent ack and steal racing the same Running record have exactly one CAS win"
    - "The pending (unassigned) queue is ULID-ordered in the queue_items namespace and dispatched in order"
  artifacts:
    - path: "crates/rollout-coordinator/src/ledger.rs"
      provides: "queue_items dispatch queue + per-worker backlog accounting over WorkItemRecord"
      contains: "queue_items"
    - path: "crates/rollout-coordinator/src/steal.rs"
      provides: "victim selection + ceil(n/2) cap + CAS reassign (try_repending then try_claim)"
      contains: "MAX_STEAL_BATCH"
    - path: "crates/rollout-coordinator/tests/steal_dedup.rs"
      provides: "concurrent_ack_and_steal_no_double_execute witness"
      contains: "concurrent_ack_and_steal_no_double_execute"
  key_links:
    - from: "crates/rollout-coordinator/src/steal.rs"
      to: "crates/rollout-coordinator/src/work_item.rs"
      via: "try_repending(victim) then try_claim(thief) CAS reassign"
      pattern: "try_repending|try_claim"
    - from: "crates/rollout-coordinator/src/ledger.rs"
      to: "rollout_core::Storage::scan_bytes"
      via: "queue_items ULID-ordered scan (InMemQueue template)"
      pattern: "scan_bytes"
---

<objective>
Implement DIST-02: the work-stealing pull queue with CAS-on-state dedup. Build the
coordinator-mediated steal protocol (idle worker → coordinator picks busiest victim
→ reassign ceil(backlog/2) capped items via CAS) on top of the `WorkItemRecord`
module (06-00) and the epoch authority (06-01). Land
`concurrent_ack_and_steal_no_double_execute` — the witness that a stolen-then-reclaimed
item never double-executes (SC5).

Purpose: idle workers steal from busy peers without double-execution — the core
distributed-work guarantee.
Output: ledger dispatch queue, steal protocol, steal_dedup witness.
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
@$HOME/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/PROJECT.md
@.planning/ROADMAP.md
@.planning/STATE.md
@.planning/phases/06-multi-node-distribution/06-RESEARCH.md
@.planning/phases/06-multi-node-distribution/06-CONTEXT.md
@.planning/phases/06-multi-node-distribution/06-00-SUMMARY.md
@.planning/phases/06-multi-node-distribution/06-01-SUMMARY.md

<interfaces>
From 06-00 — crates/rollout-coordinator/src/work_item.rs:
```rust
pub enum WorkState { Pending, Running { worker_id, started_at_ms }, Done { result_id }, Failed { reason } }
pub struct WorkItemRecord { pub id: ContentId, pub state: WorkState }
pub fn work_key(run_id: &RunId, work_id: &ContentId) -> StorageKey;  // namespace="work"
pub async fn try_claim(txn, run_id, &record, worker_id, now_ms) -> Result<bool, CoreError>;   // Pending->Running
pub async fn try_complete(txn, run_id, &record, result_id) -> Result<bool, CoreError>;        // Running->Done
pub async fn try_repending(txn, run_id, &record) -> Result<bool, CoreError>;                  // Running->Pending
```

From crates/rollout-cloud-local/src/queue.rs (ULID-ordered queue_items template):
```rust
// InMemQueue mirrors RAM queue into a Storage namespace; scan-on-open replays in ULID order.
```

Steal protocol (06-RESEARCH "Work-Stealing + CAS Dedup", D-STEAL-01..04):
- D-STEAL-01: steal only when local queue drains to empty.
- D-STEAL-02: n = min(ceil(victim_backlog/2), MAX_STEAL_BATCH).
- D-STEAL-03: victim = busiest peer by backlog, coordinator-mediated (no peer-to-peer).
- D-STEAL-04: reassign each item via try_repending(victim) then try_claim(thief); CAS collapses races.
</interfaces>
</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: Ledger dispatch queue (queue_items) + per-worker backlog accounting</name>
  <read_first>
    - crates/rollout-coordinator/src/ledger.rs (create)
    - crates/rollout-coordinator/src/work_item.rs (from 06-00 — WorkItemRecord + work_key + CAS fns)
    - crates/rollout-cloud-local/src/queue.rs (InMemQueue: ULID-ordered scan-on-open spill/replay — the template)
    - crates/rollout-coordinator/src/lib.rs (add `pub mod ledger;`)
    - .planning/phases/06-multi-node-distribution/06-RESEARCH.md §4 (queue_items namespace: `["q", <ulid>]`, payload bytes, ULID-ordered)
  </read_first>
  <behavior>
    - Test `queue_items_fifo_ulid_order`: enqueue 3 payloads; dispatch returns them in ULID (insertion) order.
    - Test `backlog_count_by_worker`: with 2 Running items on worker A and 1 on worker B (in the `work` namespace), `backlog(A)==2`, `backlog(B)==1`, `busiest()==A`.
    - Test `dispatch_claims_pending`: dispatch to worker pops a queue_items entry and CAS-creates a `WorkItemRecord::Pending` then `try_claim` → Running(worker).
  </behavior>
  <action>
    Create `crates/rollout-coordinator/src/ledger.rs`:
    - `enqueue(txn, run_id, payload) -> Result<(), CoreError>`: put a `queue_items` row at `StorageKey { namespace:"queue_items", run_id, path: vec!["q".into(), Ulid::new().to_string()] }` (ULID monotonic ordering, mirror InMemQueue).
    - `next_pending(storage, run_id) -> Result<Option<(ContentId, Vec<u8>)>, CoreError>`: scan `queue_items` namespace (ULID-ordered via scan_bytes), return the lowest-keyed entry.
    - `dispatch(txn, run_id, worker_id) -> Result<Option<ContentId>, CoreError>`: pop next pending → derive deterministic `work_id` (ContentId = blake3 of payload, content-addressed per CORE-05) → write WorkItemRecord::Pending → `try_claim(worker)` → on success delete the queue_items entry.
    - `backlog(storage, run_id, worker_id) -> Result<usize, CoreError>` and `busiest(storage, run_id, exclude) -> Result<Option<(WorkerId, usize)>, CoreError>`: scan the `work` namespace, count `Running { worker_id }` per worker.
    Reuse `work_item::{try_claim, work_key}`. Add `pub mod ledger;` to lib.rs. Tests inline (EmbeddedStorage over tempfile). Per-item rustdoc.
  </action>
  <verify>
    <automated>cargo test -p rollout-coordinator ledger</automated>
  </verify>
  <acceptance_criteria>
    - `grep -q '"queue_items"' crates/rollout-coordinator/src/ledger.rs` AND `grep -q "fn busiest" crates/rollout-coordinator/src/ledger.rs`.
    - `grep -q "scan_bytes" crates/rollout-coordinator/src/ledger.rs` (ULID-ordered scan).
    - `cargo test -p rollout-coordinator ledger` exits 0 (3 tests incl. `backlog_count_by_worker`).
    - DOCS-02: same commit ships inline tests + rustdoc.
  </acceptance_criteria>
  <done>queue_items ULID-ordered dispatch queue + per-worker backlog/busiest accounting compile and pass; steal can ask "who is busiest".</done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: Coordinator-mediated steal protocol + CAS reassign + MAX_STEAL_BATCH</name>
  <read_first>
    - crates/rollout-coordinator/src/steal.rs (create)
    - crates/rollout-coordinator/src/ledger.rs (from Task 1 — busiest/backlog)
    - crates/rollout-coordinator/src/work_item.rs (try_repending + try_claim — the CAS reassign primitives)
    - crates/rollout-coordinator/src/lib.rs (add `pub mod steal;`)
    - .planning/phases/06-multi-node-distribution/06-RESEARCH.md §"Work-Stealing + CAS Dedup" (steal protocol pseudocode + MAX_STEAL_BATCH) + 06-CONTEXT.md D-STEAL-01..04
  </read_first>
  <behavior>
    - Test `steal_takes_ceil_half_capped`: victim has 7 Running items → steal returns ceil(7/2)=4 (or MAX_STEAL_BATCH if smaller); victim with 100 items → capped at MAX_STEAL_BATCH (32).
    - Test `steal_only_when_local_empty`: a steal_request from a worker with non-empty local backlog is rejected/no-op (D-STEAL-01).
    - Test `steal_reassigns_via_cas`: after a successful steal of item X, X's WorkItemRecord is Running(thief), not Running(victim).
  </behavior>
  <action>
    Create `crates/rollout-coordinator/src/steal.rs`:
    - `pub const MAX_STEAL_BATCH: usize = 32;` documented "fixed for v1.1 (D-STEAL); revisit if tuning need emerges" (NOT a config knob — deferred per CONTEXT).
    - `pub async fn handle_steal_request(storage, run_id, thief: WorkerId) -> Result<Vec<ContentId>, CoreError>`: (1) guard — only proceed if thief's local backlog is empty (D-STEAL-01); (2) `victim = ledger::busiest(exclude=thief)` (D-STEAL-03); (3) `n = min((victim_backlog + 1) / 2, MAX_STEAL_BATCH)` (D-STEAL-02 ceil); (4) for the first n of victim's Running items: in one txn, `try_repending(item)` (Running(victim)->Pending) then `try_claim(item, thief)` (Pending->Running(thief)) — CAS-on-exact-bytes (D-STEAL-04). If `try_repending` returns false (victim acked first), skip the item (no double-execute). Return the work_ids actually reassigned.
    Add `pub mod steal;` to lib.rs. Tests inline. Per-item rustdoc documenting why CAS collapse is sound (RESEARCH §"Why this is idempotent").
  </action>
  <verify>
    <automated>cargo test -p rollout-coordinator steal</automated>
  </verify>
  <acceptance_criteria>
    - `grep -q "MAX_STEAL_BATCH" crates/rollout-coordinator/src/steal.rs` AND `grep -q "32" crates/rollout-coordinator/src/steal.rs`.
    - `grep -q "try_repending" crates/rollout-coordinator/src/steal.rs && grep -q "try_claim" crates/rollout-coordinator/src/steal.rs` (CAS reassign).
    - `grep -q "+ 1) / 2\|div_ceil\|ceil" crates/rollout-coordinator/src/steal.rs` (ceil(n/2)).
    - `cargo test -p rollout-coordinator steal` exits 0 (incl. `steal_takes_ceil_half_capped`).
    - DOCS-02: inline tests + rustdoc in same commit.
  </acceptance_criteria>
  <done>Coordinator-mediated steal reassigns ceil(backlog/2)-capped items via CAS from the busiest peer to an idle worker; non-empty thieves and lost-race items are skipped.</done>
</task>

<task type="auto" tdd="true">
  <name>Task 3: concurrent_ack_and_steal_no_double_execute witness (SC5)</name>
  <read_first>
    - crates/rollout-coordinator/tests/steal_dedup.rs (create)
    - crates/rollout-runtime-batch/tests/cas_state_machine.rs (the single-winner race-test pattern: spawn two tasks, assert exactly one CAS true)
    - crates/rollout-coordinator/src/steal.rs + work_item.rs (the CAS primitives under test)
    - crates/rollout-coordinator/tests/support/mod.rs (Sim harness from 06-00)
    - .planning/phases/06-multi-node-distribution/06-RESEARCH.md §"Work-Stealing + CAS Dedup" (the ack-vs-steal race analysis: both CAS against Running(victim) bytes; exactly one wins)
    - docs/specs/05-distribution.md §5 (steal protocol — update if behavior differs)
  </read_first>
  <behavior>
    - Test `concurrent_ack_and_steal_no_double_execute` (SC5): seed one item X as Running(victim); spawn task A = victim's ack (`try_complete` Running->Done) and task B = steal's `try_repending` (Running->Pending), both racing the same `expected=Running(victim)` bytes; assert exactly one of {A,B} returns `true`; assert the final state is reached once (if ack won → Done, steal no-op; if steal won → Pending then re-claimed, victim's late ack dropped). Run it ~100 iterations to shake out the race.
    - Test `final_state_consistent`: after the race, scanning the `work` namespace shows item X in exactly one terminal/assigned state — never two Running owners, never double Done.
  </behavior>
  <action>
    Create `crates/rollout-coordinator/tests/steal_dedup.rs` (`mod support;`). Implement `concurrent_ack_and_steal_no_double_execute` as a direct adaptation of `cas_state_machine.rs::pending_to_running_to_done_round_trip`'s single-winner assertion: use the Sim harness to seed Running(victim), `tokio::join!` an ack task and a steal task over the SAME prior record bytes, count CAS `true` returns, assert == 1, loop 100×. Document the two outcomes (ack-wins / steal-wins) inline. Update `docs/specs/05-distribution.md` §5 with the steal-vs-ack dedup invariant if not already covered by 06-01's doc edit.
  </action>
  <verify>
    <automated>cargo test -p rollout-coordinator concurrent_ack_and_steal_no_double_execute</automated>
  </verify>
  <acceptance_criteria>
    - `cargo test -p rollout-coordinator concurrent_ack_and_steal_no_double_execute` exits 0 (the named SC5 witness from ROADMAP + VALIDATION map).
    - `grep -q "exactly one\|== 1\|assert_eq!(wins, 1" crates/rollout-coordinator/tests/steal_dedup.rs` (single-winner assertion).
    - `grep -q "tokio::join\|spawn" crates/rollout-coordinator/tests/steal_dedup.rs` (genuine concurrency, not sequential).
    - DOCS-02: same commit updates docs/specs/05-distribution.md.
  </acceptance_criteria>
  <done>concurrent_ack_and_steal_no_double_execute green on every commit: a stolen-then-reclaimed item never double-executes; CAS collapses the ack/steal race to exactly one winner.</done>
</task>

</tasks>

<verification>
- `cargo test -p rollout-coordinator` green (ledger, steal, steal_dedup).
- `cargo test -p rollout-coordinator concurrent_ack_and_steal_no_double_execute` exits 0 (SC5).
- `cargo test -p rollout-core --test dependency_direction` green (coord ↛ cloud).
- `cargo clippy -p rollout-coordinator --all-targets -- -D warnings` clean.
</verification>

<success_criteria>
- DIST-02: idle workers steal ceil(backlog/2)-capped batches from the busiest peer, coordinator-mediated, via CAS reassign.
- `concurrent_ack_and_steal_no_double_execute` (SC5) green every commit, Docker-free — no double-execution under the ack/steal race.
- queue_items dispatch is ULID-ordered; backlog accounting drives victim selection.
</success_criteria>

<output>
After completion, create `.planning/phases/06-multi-node-distribution/06-02-SUMMARY.md`
</output>
