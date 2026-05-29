---
phase: 06-multi-node-distribution
plan: 03
type: execute
wave: 3
depends_on: ["06-00", "06-01", "06-02"]
files_modified:
  - crates/rollout-coordinator/src/run.rs
  - crates/rollout-coordinator/src/failure_scan.rs
  - crates/rollout-coordinator/src/drain.rs
  - crates/rollout-coordinator/src/lib.rs
  - crates/rollout-coordinator/tests/coord_restart.rs
  - crates/rollout-coordinator/tests/spot_drain.rs
  - crates/rollout-runtime-batch/src/worker.rs
  - .planning/REQUIREMENTS.md
  - .planning/ROADMAP.md
  - docs/specs/05-distribution.md
autonomous: true
requirements: [DIST-03, DIST-04]
must_haves:
  truths:
    - "A fresh coordinator boots by acquiring the lease, adopting the advanced epoch, and replaying the work ledger from Storage — reconstructing in-flight assignments without blindly requeuing Running items"
    - "Across a coordinator kill+restart, every work_id reaches Done exactly once — replayed acks are idempotent (try_complete on Done returns false)"
    - "On a spot-preemption signal a worker stops pulling, requeues in-flight items via nack, opportunistically snapshots TrainState if budget allows, deregisters, and exits within the drain deadline (60s AWS / 15s GCP)"
    - "REQUIREMENTS.md and ROADMAP.md both state BOTH the notice lead (120s/30s) AND the drain deadline (60s/15s) with the notice-vs-deadline distinction"
  artifacts:
    - path: "crates/rollout-coordinator/src/run.rs"
      provides: "stateless-replayer boot: lease acquire -> epoch adopt -> ledger replay -> serve"
      contains: "scan_bytes"
    - path: "crates/rollout-coordinator/src/drain.rs"
      provides: "spot-drain state machine (stop-pull -> nack -> snapshot? -> deregister -> exit)"
      contains: "preemption_signal"
    - path: "crates/rollout-coordinator/tests/coord_restart.rs"
      provides: "coord_restart_no_duplicates witness"
      contains: "coord_restart_no_duplicates"
    - path: "crates/rollout-coordinator/tests/spot_drain.rs"
      provides: "spot_drain_completes_within_lead_time witness"
      contains: "spot_drain_completes_within_lead_time"
  key_links:
    - from: "crates/rollout-coordinator/src/run.rs"
      to: "rollout_core::Storage::scan_bytes (work namespace)"
      via: "ledger replay on boot — reconstruct in-flight, requeue Pending, skip Done"
      pattern: "scan_bytes"
    - from: "crates/rollout-coordinator/src/drain.rs"
      to: "rollout_core::ComputeHint::preemption_signal + Queue::nack"
      via: "poll signal, nack in-flight, deregister"
      pattern: "preemption_signal|nack"
---

<objective>
Implement DIST-03 (coordinator-restart-from-storage via the stateless-replayer) and
DIST-04 (spot-preemption graceful drain). Wire the replayer boot order into `run.rs`
(lease acquire → adopt epoch → replay ledger → resume failure_scan → serve), land
`coord_restart_no_duplicates` (SC2), build the drain state machine on the existing
`ComputeHint::preemption_signal`, land `spot_drain_completes_within_lead_time` (SC3),
and reconcile the two spot numbers across REQUIREMENTS.md + ROADMAP (D-SPOT-04).

Purpose: restart is invisible to progress; spot preemption drains without data loss.
Output: replayer boot, drain state machine, two witnesses, doc reconciliation.
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
@.planning/phases/06-multi-node-distribution/06-02-SUMMARY.md

<interfaces>
From 06-01 — StorageLease + epoch:
```rust
StorageLease::new(storage, run_id).try_acquire(me, ttl) -> Option<LeaseRecord>;
epoch::current_epoch(storage, run_id) -> CoordEpoch;
```
From 06-00/06-02 — work_item + ledger:
```rust
work_item::{WorkItemRecord, WorkState::{Pending, Running, Done, Failed}, try_complete, try_repending};
ledger::enqueue/next_pending/dispatch;
```
From crates/rollout-core/src/traits/cloud.rs:
```rust
async fn preemption_signal(&self) -> Result<Option<Duration>, CoreError>;  // Some(120s) AWS / Some(30s) GCP
async fn nack(&self, id: QueueItemId) -> Result<(), CoreError>;             // lease nack -> back to Pending
```
From crates/rollout-coordinator/src/run.rs (existing boot to EXTEND, not rewrite):
```rust
pub async fn run(cfg: CoordinatorConfig) -> Result<(), CoreError>;  // opens Storage, TLS, CoordinatorImpl, failure_scan_loop
```
Replayer boot order (06-RESEARCH §5): acquire lease (else exit) -> adopt epoch -> scan `work` namespace -> Running=reconstruct (NO requeue), Pending=dispatch, Done/Failed=skip -> resume failure_scan -> serve.
Drain sequence (06-RESEARCH "Spot-Drain", D-SPOT-02): stop-pull -> nack in-flight -> opportunistic TrainState snapshot if budget -> deregister -> ack-exit, within deadline 60s/15s.
</interfaces>
</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: Stateless-replayer boot + coord_restart_no_duplicates (SC2)</name>
  <read_first>
    - crates/rollout-coordinator/src/run.rs (existing boot path — EXTEND with lease+replay; do NOT rewrite)
    - crates/rollout-coordinator/src/failure_scan.rs (resume after replay; stale-Running -> try_repending after coord_failure_timeout)
    - crates/rollout-cloud-local/src/queue.rs (InMemQueue::open scan-on-open — the ledger-replay template)
    - crates/rollout-coordinator/src/lease.rs + ledger.rs + work_item.rs (from 06-01/06-02)
    - crates/rollout-coordinator/tests/coord_restart.rs (create)
    - crates/rollout-coordinator/tests/support/mod.rs (Sim harness from 06-00)
    - .planning/phases/06-multi-node-distribution/06-RESEARCH.md §5 (stateless-replayer boot sequence) + §"Common Pitfalls" 4 (do NOT requeue Running on boot)
  </read_first>
  <behavior>
    - Test `replay_reconstructs_in_flight`: seed `work` namespace with Running(w1) X, Pending Y, Done Z; replayer boot reconstructs X as in-flight (NOT requeued), dispatches Y, skips Z. Assert X still Running(w1) after boot.
    - Test `coord_restart_no_duplicates` (SC2): Sim with 3 workers + N items; run to partial progress; "kill" coordinator (drop it); boot a fresh coordinator over the SAME storage; workers replay buffered acks; assert every work_id reaches Done EXACTLY once (Sim's no-duplicates scan helper). Include a replayed-ack-is-idempotent assertion: `try_complete` on an already-Done record returns false.
    - Test `replayer_adopts_advanced_epoch`: after a steal advanced epoch to 1, the fresh coordinator boots at epoch 1 and stamps it.
  </behavior>
  <action>
    Extend `crates/rollout-coordinator/src/run.rs` with a `replay_and_serve` boot order (RESEARCH §5), inserted AFTER storage open + TLS but BEFORE serving:
    1. `lease = StorageLease::new(storage, run_id).try_acquire(me, ttl=coordinator_failure_timeout)`; if `None` → another coord is live → return cleanly (spec 05 §8 loser exits).
    2. `epoch = lease.epoch` — adopt; stamp on all RPC responses (via 06-01 epoch helpers).
    3. Replay: `scan_bytes(namespace="work", run_id)`; for each WorkItemRecord — `Running{worker}` → reconstruct the in-flight assignment map (do NOT requeue — Pitfall 4); `Pending` → push to dispatch queue; `Done`/`Failed` → skip (idempotent).
    4. Resume `failure_scan_loop` (existing) — stale `Running` past `coord_failure_timeout` → `try_repending` (the only requeue path).
    5. Serve. Also add a lease-renew loop (renew at heartbeat cadence; on `renew()==false` → call `fence::fence_old_coordinator` then abort at binary edge).
    Make the boot order callable in-process by the Sim harness (extract `replay_and_serve(storage, run_id, ...)` as a testable fn separate from the socket-binding `run`). Keep the existing single-host path working.
    Create `crates/rollout-coordinator/tests/coord_restart.rs` (`mod support;`) with the 3 tests above. Add the lease-renew-loop module wiring to lib.rs if needed.
  </action>
  <verify>
    <automated>cargo test -p rollout-coordinator coord_restart_no_duplicates</automated>
  </verify>
  <acceptance_criteria>
    - `cargo test -p rollout-coordinator coord_restart_no_duplicates` exits 0 (the named SC2 witness).
    - `grep -q 'scan_bytes' crates/rollout-coordinator/src/run.rs` AND `grep -qi "do not requeue\|reconstruct\|in-flight" crates/rollout-coordinator/src/run.rs` (replayer reconstructs, does not blind-requeue).
    - `grep -q "try_acquire" crates/rollout-coordinator/src/run.rs` (lease-gated boot; loser exits).
    - `cargo test -p rollout-coordinator replay_reconstructs_in_flight` exits 0.
    - DOCS-02: same commit ships tests + rustdoc on the new boot fn.
  </acceptance_criteria>
  <done>Fresh coordinator boots via lease+replay, reconstructs in-flight without re-execution; coord_restart_no_duplicates (SC2) green — every work_id reaches Done exactly once across restart.</done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: Spot-drain state machine + spot_drain_completes_within_lead_time (SC3)</name>
  <read_first>
    - crates/rollout-coordinator/src/drain.rs (create)
    - crates/rollout-core/src/traits/cloud.rs (ComputeHint::preemption_signal, Queue::nack/extend_lease)
    - crates/rollout-cloud-aws/src/imds/mod.rs + crates/rollout-cloud-gcp/src/mds/mod.rs (120s/30s lead — signal source, consumed via the ComputeHint trait, NOT imported directly — coord ↛ cloud)
    - crates/rollout-coordinator/src/heartbeat.rs (deregister — already in heartbeat.rs:74)
    - crates/rollout-runtime-batch/src/worker.rs (worker run loop — wire stop-pull on drain)
    - crates/rollout-coordinator/tests/spot_drain.rs (create)
    - .planning/phases/06-multi-node-distribution/06-RESEARCH.md §"Spot-Drain Orchestration" (state machine + two-number discipline) + 06-CONTEXT.md D-SPOT-01/02/03
  </read_first>
  <behavior>
    - Test `spot_drain_completes_within_lead_time` (SC3): inject a mock ComputeHint returning `Some(lead)`; run `drain(deadline)`; assert the sequence stop-pull → nack-in-flight → (opportunistic snapshot) → deregister → exit completes and the measured wall-clock is within the deadline (use a compressed test deadline; assert ordering + completion, not a literal 60s sleep). Run for both AWS (deadline 60s) and GCP (deadline 15s) config variants.
    - Test `drain_requeues_in_flight`: in-flight items are nacked back to Pending; a surviving worker can re-claim them (try_claim succeeds post-nack).
    - Test `drain_snapshot_skipped_when_budget_low`: if remaining_budget < snapshot_cost estimate, the opportunistic TrainState snapshot is skipped (D-SPOT-03 TrainState-only; lost work is requeued, recomputed).
    - Test `drain_uses_two_numbers`: the drain targets the DEADLINE (60/15), distinct from the NOTICE lead (120/30) returned by preemption_signal.
  </behavior>
  <action>
    Create `crates/rollout-coordinator/src/drain.rs`. Define `pub struct DrainConfig { notice_lead: Duration, drain_deadline: Duration }` with `aws() -> {120s, 60s}` and `gcp() -> {30s, 15s}` constructors (D-SPOT-01 two numbers). Implement `pub async fn drain(hint: &dyn ComputeHint, queue: &dyn Queue, in_flight: &[QueueItemId], snapshotter: Option<&dyn Snapshotter>, cfg: DrainConfig, deregister: impl FnOnce) -> Result<(), CoreError>`:
    1. stop-pull: signal the worker run loop to leave `running` and refuse new pull/steal (set an `AtomicBool`/flag the worker checks).
    2. requeue: for each in-flight id, `queue.nack(id)` (lease nack → Pending).
    3. opportunistic: if `remaining_budget` allows → `snapshotter.snapshot(TrainState)` (D-SPOT-03 TrainState ONLY); else skip.
    4. deregister: call the deregister closure (wraps heartbeat.rs deregister).
    5. ack-exit: return Ok (binary edge exits 0).
    All within `cfg.drain_deadline` — use `tokio::time::timeout(cfg.drain_deadline, ...)`. The drain trigger (a poll loop on `preemption_signal()`) lives at the worker run-loop edge in `rollout-runtime-batch/src/worker.rs`; add the stop-pull flag + the poll there. Consume the signal ONLY via the `ComputeHint` trait (no `rollout-cloud-*` import in coordinator — preserves coord ↛ cloud).
    Create `crates/rollout-coordinator/tests/spot_drain.rs` with the 4 tests, using mock `ComputeHint`/`Queue`/`Snapshotter` impls (Sim harness + test mocks). Add `pub mod drain;` to lib.rs.
  </action>
  <verify>
    <automated>cargo test -p rollout-coordinator spot_drain_completes_within_lead_time</automated>
  </verify>
  <acceptance_criteria>
    - `cargo test -p rollout-coordinator spot_drain_completes_within_lead_time` exits 0 (the named SC3 witness).
    - `grep -q "preemption_signal" crates/rollout-coordinator/src/drain.rs` AND `grep -q "nack" crates/rollout-coordinator/src/drain.rs`.
    - `grep -q "60\|drain_deadline" crates/rollout-coordinator/src/drain.rs && grep -q "120\|notice_lead" crates/rollout-coordinator/src/drain.rs` (BOTH numbers present, D-SPOT-01).
    - `! grep -q "rollout_cloud_aws\|rollout_cloud_gcp\|aws_sdk\|google_cloud" crates/rollout-coordinator/src/drain.rs` (signal via ComputeHint trait only — coord ↛ cloud).
    - `cargo test -p rollout-core --test dependency_direction` green.
    - DOCS-02: same commit ships tests + rustdoc.
  </acceptance_criteria>
  <done>spot_drain_completes_within_lead_time (SC3) green: drain stops pull, nacks in-flight, opportunistically snapshots TrainState, deregisters, exits within 60s/15s; signal consumed via the ComputeHint trait (no cloud dep).</done>
</task>

<task type="auto">
  <name>Task 3: D-SPOT-04 doc reconciliation — notice lead vs drain deadline</name>
  <read_first>
    - .planning/REQUIREMENTS.md (DIST-04 line 65 — currently says "120s AWS / 30s GCP")
    - .planning/ROADMAP.md (Phase 6 SC3 line ~123 — currently says "AWS budget 60s / GCP 15s")
    - docs/specs/05-distribution.md (the drain section)
    - .planning/phases/06-multi-node-distribution/06-RESEARCH.md §"Common Pitfalls" 6 + §"Spot-Drain Orchestration" two-number discipline + 06-CONTEXT.md D-SPOT-04
  </read_first>
  <action>
    Reconcile the two spot numbers across all three docs so a reader can tell which is the test bound (D-SPOT-04, Pitfall 6). Edit:
    - `.planning/REQUIREMENTS.md` DIST-04: change "Budgets: 120s AWS / 30s GCP" to state BOTH — "Preemption-notice lead time: 120s AWS / 30s GCP (the cloud signal). Conservative drain deadline (what graceful drain completes within): 60s AWS / 15s GCP — the `spot_drain_completes_within_lead_time` test bound."
    - `.planning/ROADMAP.md` Phase 6 SC3: change "AWS budget 60s / GCP 15s" to the same two-number framing (notice lead 120/30, drain deadline 60/15).
    - `docs/specs/05-distribution.md` drain section: state both numbers + the notice-vs-deadline distinction, cross-referencing the witness.
    This is a docs-only task — no code. Per AGENTS.md §9.2 a docs-only commit satisfies the per-commit policy by touching `docs/`.
  </action>
  <verify>
    <automated>grep -q "drain deadline" .planning/REQUIREMENTS.md && grep -q "notice" .planning/ROADMAP.md && grep -qi "60s\|drain deadline" docs/specs/05-distribution.md</automated>
  </verify>
  <acceptance_criteria>
    - `grep -E "120s.*60s|notice.*deadline" .planning/REQUIREMENTS.md` succeeds (both numbers stated).
    - `grep -E "120|notice" .planning/ROADMAP.md` succeeds in the Phase 6 SC3 region.
    - `grep -qi "notice" docs/specs/05-distribution.md && grep -q "60s" docs/specs/05-distribution.md` (spec states both).
    - No code files changed in this commit (docs-only).
  </acceptance_criteria>
  <done>REQUIREMENTS.md, ROADMAP.md, and the distribution spec all state BOTH the notice lead (120/30) and the drain deadline (60/15) with the distinction explicit; the test bound (60/15) is unambiguous.</done>
</task>

</tasks>

<verification>
- `cargo test -p rollout-coordinator` green (coord_restart, spot_drain witnesses).
- `cargo test -p rollout-coordinator coord_restart_no_duplicates` exits 0 (SC2).
- `cargo test -p rollout-coordinator spot_drain_completes_within_lead_time` exits 0 (SC3).
- `cargo test -p rollout-core --test dependency_direction` green (coord ↛ cloud preserved through drain).
- `cargo test --workspace --tests` green (full suite after this wave).
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
</verification>

<success_criteria>
- DIST-03: stateless-replayer boot reconstructs in-flight from Storage; coord_restart_no_duplicates (SC2) green — restart invisible to progress, zero duplicate sample IDs.
- DIST-04: spot-drain state machine on ComputeHint::preemption_signal; spot_drain_completes_within_lead_time (SC3) green within 60s/15s; TrainState-only opportunistic snapshot.
- D-SPOT-04: docs reconciled — notice lead vs drain deadline stated in all three docs.
</success_criteria>

<output>
After completion, create `.planning/phases/06-multi-node-distribution/06-03-SUMMARY.md`
</output>
