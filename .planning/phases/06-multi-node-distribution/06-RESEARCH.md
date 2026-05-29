# Phase 6: Multi-node distribution - Research

**Researched:** 2026-05-29
**Domain:** Distributed coordination — lease-based exclusion, epoch fencing, work-stealing, spot-drain
**Confidence:** HIGH (grounded in repo Phase-2/4/5 scaffolding; minimal external surface)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Split-brain self-fence (DIST-05):**
- **D-FENCE-01:** Old coordinator that detects its lease was stolen MUST stop all shared-state I/O immediately. A loser never mutates shared state after losing the lease.
- **D-FENCE-02:** Before aborting, emit exactly ONE `coordinator_fenced` observability event (tracing + `Event`) that performs **no shared-state write** (observability sink only).
- **D-FENCE-03:** Then `std::process::abort()` within the 5s bound (roadmap SC4). NO best-effort flush of in-flight work by the loser.
- **D-FENCE-04:** Workers validate `coord_epoch` on **every** RPC and reject stale-epoch responses. `worker_self_fence_timeout < coordinator_failure_timeout` (reuse Phase-2: 4s self-fence < 5s coord-failure).

**Coordinator restart-gap (DIST-03):**
- **D-RESTART-01:** While coordinator is dead/restarting, workers keep executing their current lease and re-sync on reconnect. Workers buffer acks during the gap and replay on reconnect.
- **D-RESTART-02:** Fresh coordinator boots by replaying the assignment ledger + fence epoch from Storage (stateless-replayer). Reconstructs in-flight assignments, does not reassign blindly.
- **D-RESTART-03:** Bounded safety — if gap exceeds `coordinator_failure_timeout`, workers self-fence (D-FENCE-04) and stop.

**Work-stealing (DIST-02):**
- **D-STEAL-01:** Steal only when local queue drains to empty (reactive).
- **D-STEAL-02:** Steal `ceil(victim_backlog / 2)`, capped at a max batch size.
- **D-STEAL-03:** Victim = busiest peer by backlog, coordinator-mediated.
- **D-STEAL-04:** CAS-on-state collapses duplicate acks/steals; stolen-then-reclaimed item never double-executes.
- These are FIXED for v1.1 (not config knobs).

**Lease backend + tests (DIST-01):**
- **D-LEASE-01:** Single-row CAS lease behind a trait with two impls: embedded redb-backed lease (default, CI/`make smoke`) and Postgres single-row lease (prod). Mirrors Phase-4 dual-storage.
- **D-LEASE-02:** 3-node smoke + `coord_restart_no_duplicates` + `split_brain_old_coord_self_fences` run **Docker-free on every commit** via the embedded path. Postgres lease in the `postgres-integration` CI lane.

**Spot-drain budgets (DIST-04):**
- **D-SPOT-01:** Notice lead time = 120s AWS / 30s GCP. Conservative drain deadline = 60s AWS / 15s GCP.
- **D-SPOT-02:** Drain: stop-pull → requeue in-flight via lease nack → opportunistic TrainState snapshot if budget allows → deregister → ack-exit. `spot_drain_completes_within_lead_time` asserts completion within 60s/15s.
- **D-SPOT-03:** v1.1 opportunistic snapshot = TrainState only.
- **D-SPOT-04:** Reconcile docs — REQ DIST-04 + roadmap to state BOTH numbers (notice-vs-deadline).

### Claude's Discretion
- Exact lease-trait method shape, TTL/renewal cadence (within Phase-2 deadline bounds), ledger schema layout, `coordinator_lease` table DDL — designed in the DIST-03 spike below.
- Internal RPC/proto additions for steal + epoch-tagged responses (within the existing 3-channel mTLS transport).
- Observability event taxonomy beyond `coordinator_fenced`.

### Deferred Ideas (OUT OF SCOPE)
- Configurable work-stealing knobs (deferred from D-STEAL).
- Process snapshots on spot-drain (CRIU) — SNAPSHOT-01, v1.2+. v1.1 = TrainState only.
- Raft/etcd coordinator consensus — explicitly rejected (bespoke storage-backed replayer instead).
- Multi-coordinator HA — post-v1.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| DIST-01 | Coordinator state in Storage (`work`/`epoch`/`queue_items`); one coord per run; lease-based exclusion | Lease trait over `StorageTxn::cas_bytes` (already on both backends); DDL = existing `kv` row, no new table |
| DIST-02 | Work-stealing pull queue; CAS dedup; `concurrent_ack_and_steal_no_double_execute` | Reuse `rollout-runtime-batch` CAS state machine (`try_claim`/`try_complete`); coordinator-mediated steal |
| DIST-03 | Coordinator-restart-from-storage; stateless-replayer; `coord_restart_no_duplicates` | Ledger replay from `work`/`epoch` namespaces; DIST-03 spike below |
| DIST-04 | Spot-preemption handler → graceful drain; 60s/15s deadline | `ComputeHint::preemption_signal` already returns lead (120s AWS/30s GCP); drain state machine |
| DIST-05 | Split-brain fencing; epoch validation every RPC; old coord self-aborts; `split_brain_old_coord_self_fences` | Epoch monotonic in `epoch` namespace; `validate_cross_fields` invariant already enforced |
</phase_requirements>

## Summary

Phase 6 is **assembly over invention**. Every hard primitive already exists in the tree:

- **`StorageTxn::cas_bytes(key, expected, new) -> bool`** is implemented identically on both the embedded redb backend (`embedded/txn.rs:122`) and Postgres (`postgres/mod.rs:242`, value-compare under `SELECT … FOR UPDATE`). A "single-row CAS lease" is literally one CAS row in the existing `kv` table. **No new lease table is needed** — DIST-01's `coordinator_lease` is a `StorageKey { namespace: "coordinator_lease", run_id, path: [] }` row. The two-impl requirement (D-LEASE-01) is satisfied for free because `cas_bytes` is already dual-backed.
- **CAS-on-state dedup** (DIST-02/D-STEAL-04) is the exact pattern in `rollout-runtime-batch::state` (`try_claim`/`try_complete`/`try_repending`, witnessed by `cas_state_machine.rs` + `resume_skips_done.rs`). Stolen-then-reclaimed idempotency is the same `try_claim` race the batch runtime already proves.
- **Deadline-based health** (`rollout-transport::health::is_failed`/`next_due_at`) and the **`self_fence < coord_failure` invariant** (`TransportConfig::validate_cross_fields`, enforced at plan time) already encode the split-brain timing guarantee (DIST-05/D-FENCE-04). 4s self-fence < 5s coord-failure ships in Phase-2 defaults.
- **`ComputeHint::preemption_signal()`** already returns `Some(lead)` — 120s AWS (`imds/mod.rs:72`), 30s GCP (`mds/mod.rs:24` `GCE_PREEMPT_LEAD`). DIST-04 only needs the *drain state machine* on top of an existing signal source.
- The **queue spill/replay pattern** (`rollout-cloud-local::queue::InMemQueue::open`, scan-namespace-on-open) is the template for ledger replay and the embedded lease.

The genuinely **new** work is: (1) an epoch counter + epoch-tagging on RPC responses; (2) a `Lease` trait wrapping `cas_bytes` with TTL/expiry semantics; (3) the work-ledger schema + stateless-replayer boot path; (4) coordinator-mediated steal RPCs; (5) the drain state machine; (6) the four named in-process witnesses.

**Primary recommendation:** Do the DIST-03 spike first (lease trait + `coordinator_lease` row schema + `split_brain_old_coord_self_fences` skeleton), then build everything on `cas_bytes` and the existing `try_claim` CAS machine. Do NOT introduce a new storage table, a new consensus library, or a new health-timing model — they all exist.

## DIST-03 Architecture Spike (PRIORITY)

This discharges the roadmap's mandatory pre-plan spike: a concrete `coordinator_lease` schema (Postgres DDL + embedded redb single-row CAS equivalent) and the `split_brain_old_coord_self_fences` test skeleton.

### 1. The `Lease` trait (Claude's discretion — proposed shape)

The lease is **not** a new storage primitive; it is a thin TTL/epoch wrapper over `cas_bytes`. Define it in `rollout-core::traits` (no cloud dep — keeps coord ↛ cloud lint green) and back it with a single Storage-row impl that works on both backends.

```rust
// rollout-core::traits::lease (NEW — pure trait, no SDK dependency)
/// Monotonic epoch stamped on every coordinator authority claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CoordEpoch(pub u64);

/// Durable lease record (postcard value of the single `coordinator_lease` row).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseRecord {
    pub holder: WorkerId,        // coordinator instance id (ULID)
    pub epoch: CoordEpoch,       // monotonic; advances on every successful takeover
    pub expires_at_ms: u128,     // wall-clock deadline; renew before this
}

#[async_trait]
pub trait CoordinatorLease: Send + Sync {
    /// Try to acquire (fresh) or steal (expired) the lease. Returns the new
    /// LeaseRecord with epoch advanced iff this caller won the CAS.
    async fn try_acquire(&self, me: WorkerId, ttl: Duration) -> Result<Option<LeaseRecord>, CoreError>;
    /// Renew an owned lease: CAS(expected=current_held, new=extended). Returns
    /// Ok(false) if we no longer hold it (someone stole it -> caller must fence).
    async fn renew(&self, held: &LeaseRecord, ttl: Duration) -> Result<bool, CoreError>;
    /// Read current lease without mutating (for replayer boot + workers).
    async fn current(&self) -> Result<Option<LeaseRecord>, CoreError>;
}
```

Single impl `StorageLease` (in `rollout-coordinator`, generic over `Arc<dyn Storage>`) — works on embedded AND Postgres because both implement `cas_bytes`. This is how D-LEASE-01's "two impls" requirement is met **without writing two impls**: one `StorageLease` over the already-dual-backed `Storage` trait. (If a reviewer insists on a literal Postgres-specific `coordinator_lease` table, see §3 — but the generic-row approach is strongly recommended and matches the existing `kv` design.)

### 2. The `coordinator_lease` storage row (embedded redb single-row CAS)

```rust
fn lease_key(run_id: RunId) -> StorageKey {
    StorageKey {
        namespace: SmolStr::new_static("coordinator_lease"),
        run_id: Some(run_id),
        path: vec![],              // exactly one row per run -> "single-row" lease
    }
}
```

**acquire / steal-on-expiry algorithm** (one CAS, no read-modify-write race):

```text
try_acquire(me, ttl):
    cur_bytes = storage.get_bytes(lease_key)          # observe
    cur = cur_bytes.map(postcard::decode)
    now = now_ms()
    match cur:
      None:                                            # never held
          next = LeaseRecord { me, epoch=0, expires_at=now+ttl }
          won = cas_bytes(lease_key, expected=None, new=Some(encode(next)))
      Some(r) if now > r.expires_at_ms:                # expired -> steal
          next = LeaseRecord { me, epoch=r.epoch+1, expires_at=now+ttl }   # MONOTONIC ADVANCE
          won = cas_bytes(lease_key, expected=Some(encode(r)), new=Some(encode(next)))
      Some(_):                                          # live, not ours
          return Ok(None)
    return won ? Some(next) : None                      # lost the CAS race -> retry/back off
```

The CAS `expected` is the **exact prior bytes** — identical to `try_claim` in the batch runtime. If two would-be coordinators race on an expired lease, exactly one CAS sees its `expected` still current; the loser's `expected` is stale and `cas_bytes` returns `false` (spec 05 §8 "exactly one wins; loser exits").

**renew** (the incumbent's heartbeat on the lease):

```text
renew(held, ttl):
    next = LeaseRecord { held.holder, held.epoch, expires_at=now_ms()+ttl }  # SAME epoch
    return cas_bytes(lease_key, expected=Some(encode(held)), new=Some(encode(next)))
    # returns false iff someone advanced the epoch under us -> we were fenced
```

**TTL / renewal cadence** (within Phase-2 deadline bounds, Claude's discretion):
- `ttl = coordinator_failure_timeout` (5s) — a dead coordinator's lease expires exactly when workers would declare it failed.
- renew cadence = `heartbeat_interval` (500ms) — 10 renewals per TTL window; one missed renewal is not fatal (matches `next_due_at` 2× slack philosophy).
- A new coordinator must observe `now > expires_at_ms` before stealing — i.e. wait out the full `coordinator_failure_timeout`. This is the lower bound on `coordinator_recovery_budget` (spec 05 §1, default 30s).

### 3. Postgres DDL (prod path — only if a dedicated table is mandated)

The generic-`kv`-row approach in §2 already runs on Postgres unchanged. If a reviewer requires a typed table for queryability (spec 04 §4.2 lists specialized tables), this is the minimal additive migration `database/migrations/0003_coordinator_lease.sql`:

```sql
-- DIST-01: single-row-per-run coordinator lease (CAS exclusion + monotonic epoch).
-- Optional specialization; the generic kv row is the canonical path.
CREATE TABLE coordinator_lease (
    run_id        UUID PRIMARY KEY,          -- exactly one lease row per run
    holder        UUID NOT NULL,             -- coordinator instance ULID-as-UUID
    epoch         BIGINT NOT NULL,           -- monotonic; +1 on every steal
    expires_at    TIMESTAMPTZ NOT NULL,      -- renew before this; steal after
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- CAS acquire/steal is a single conditional UPDATE (atomic, no app-level race):
--   UPDATE coordinator_lease
--      SET holder=$me, epoch=epoch+1, expires_at=now()+$ttl, updated_at=now()
--    WHERE run_id=$run AND expires_at < now()        -- steal only if expired
--   RETURNING epoch;                                  -- NULL rows affected => lost race
-- Initial claim: INSERT ... ON CONFLICT (run_id) DO NOTHING RETURNING epoch;
-- Renew (incumbent):
--   UPDATE ... SET expires_at=now()+$ttl WHERE run_id=$run AND holder=$me AND epoch=$held_epoch;
--   0 rows affected => we were fenced (epoch advanced under us).
```

Both forms (`kv` CAS and the conditional `UPDATE … WHERE expires_at < now()`) give the same guarantee: monotonic epoch, exactly-one-winner. **Recommendation: ship the generic-`kv` `StorageLease` (§2) for v1.1; defer the typed table unless a query need emerges.**

### 4. Storage namespaces (DIST-01) and the work ledger

Per spec 04 + CONTEXT, four run-scoped namespaces (all on the generic `kv` path, no new tables):

| Namespace | Key path | Value (postcard) | Purpose |
|-----------|----------|------------------|---------|
| `coordinator_lease` | `[]` | `LeaseRecord` | single-row CAS lease + epoch |
| `epoch` | `[]` | `CoordEpoch` | authoritative current epoch (== lease epoch; replayer reads this) |
| `work` | `["item", <work_id>]` | `WorkItemRecord` (CAS state machine, mirror of `SampleRecord`) | assignment ledger; the durable in-flight map |
| `queue_items` | `["q", <ulid>]` | payload bytes | pending (unassigned) queue, ULID-ordered like `InMemQueue` |

`WorkItemRecord` mirrors `SampleRecord` exactly — `state: Pending | Running { worker_id, started_at_ms } | Done { … } | Failed { … }`. Reuse `try_claim`/`try_complete`/`try_repending` verbatim (or generalize them out of `rollout-runtime-batch` into a shared CAS-state module). The `epoch` row is written by the same CAS that advances the lease, so lease-epoch and ledger-epoch never diverge.

### 5. Stateless-replayer boot (D-RESTART-02)

A fresh coordinator does NOT reassign blindly. Boot sequence:

```text
coordinator boot:
  1. lease = try_acquire(me, ttl)              # must win the lease (wait out TTL if held)
     if None: another coord is live -> exit (spec 05 §8 loser exits)
  2. epoch = lease.epoch                        # adopt the advanced epoch; stamp on all RPCs
  3. replay ledger: scan_bytes(namespace="work", run_id)   # template: InMemQueue::open
        for each WorkItemRecord:
            Running{worker_id, started_at_ms} -> reconstruct in-flight assignment map
                                                  (do NOT requeue yet; worker may still hold it)
            Pending                            -> push to dispatch queue
            Done/Failed                        -> terminal; skip (idempotent, no re-execute)
  4. resume failure_scan_loop                   # stale Running -> try_repending after coord_failure_timeout
  5. serve; workers' control long-polls time out, retry, hit the fresh coord, re-sync acks
```

Workers buffer acks during the gap (D-RESTART-01) and replay them; because acks are CAS `Running -> Done` keyed on the deterministic `work_id`, a replayed ack is idempotent (`try_complete` on an already-`Done` row returns `false` harmlessly). **This is the mechanism behind `coord_restart_no_duplicates`.**

### 6. `split_brain_old_coord_self_fences` test skeleton (Docker-free, embedded)

```rust
// crates/rollout-coordinator/tests/split_brain.rs
//! DIST-05 witness: an old coordinator whose lease was stolen self-fences
//! (emits exactly one coordinator_fenced event, then aborts) and never writes
//! shared state after losing the epoch. Runs in-process over EmbeddedStorage.

use rollout_core::{RunId, WorkerId, Storage};
use rollout_storage::EmbeddedStorage;
use std::sync::Arc;
use std::time::Duration;
use ulid::Ulid;

#[tokio::test]
async fn split_brain_old_coord_self_fences() {
    let tmp = tempfile::tempdir().unwrap();
    let storage: Arc<dyn Storage> =
        Arc::new(EmbeddedStorage::open(tmp.path().join("rollout.redb")).await.unwrap());
    let run = RunId(Ulid::new());
    let ttl = Duration::from_millis(50);          // compressed clock for the test
    let lease = StorageLease::new(storage.clone(), run);

    // 1. Old coordinator A acquires the lease at epoch 0.
    let coord_a = WorkerId(Ulid::new());
    let held_a = lease.try_acquire(coord_a, ttl).await.unwrap().expect("A wins");
    assert_eq!(held_a.epoch.0, 0);

    // 2. A's lease expires (it stalled / GC paused). Simulate by advancing clock.
    tokio::time::sleep(ttl + Duration::from_millis(10)).await;

    // 3. New coordinator B steals the lease -> epoch advances to 1.
    let coord_b = WorkerId(Ulid::new());
    let held_b = lease.try_acquire(coord_b, ttl).await.unwrap().expect("B steals");
    assert_eq!(held_b.epoch.0, 1, "steal MUST advance epoch monotonically");

    // 4. A wakes up and tries to renew its STALE lease -> CAS fails (it was fenced).
    let renewed = lease.renew(&held_a, ttl).await.unwrap();
    assert!(!renewed, "old coordinator must NOT be able to renew after epoch advance");

    // 5. WITNESS: on renew()==false, A's fence routine emits exactly one
    //    coordinator_fenced event with NO shared-state write, then would abort.
    //    Assert the event sink saw exactly one coordinator_fenced and the
    //    shared-state (epoch row) still reads epoch=1 (B's), untouched by A.
    let fenced = CountingEmitter::default();
    fence_old_coordinator(&fenced, held_a.epoch /* stale */, held_b.epoch /* observed */).await;
    assert_eq!(fenced.count("coordinator_fenced"), 1, "exactly one fence event (D-FENCE-02)");

    // 6. Shared state is intact: epoch row is still B's (A wrote nothing).
    let cur = lease.current().await.unwrap().unwrap();
    assert_eq!(cur.holder, coord_b);
    assert_eq!(cur.epoch.0, 1);

    // NOTE: the real fence calls std::process::abort() (D-FENCE-03). The unit
    // witness asserts the *decision* + *event* path; the abort itself is
    // exercised by a #[should_panic]-style subprocess harness in the 3-node smoke,
    // NOT in-process (abort would kill the test runner).
}
```

**Key test-design constraint:** `std::process::abort()` cannot run in-process (it kills the test binary). Split the fence into a pure decision function `fence_old_coordinator(...) -> FenceDecision` that (a) emits the event and (b) returns "abort" — the in-process witness asserts the decision + single-event + no-write properties; a subprocess in `make smoke-3node-*` asserts the actual abort + 5s bound (SC4).

## Epoch Fencing Correctness (DIST-05)

**The invariant chain (why two live coordinators are impossible):**

1. **Monotonic epoch on takeover.** Every successful steal does `epoch = prev.epoch + 1` inside the *same CAS* that claims the lease (§2). A new coordinator is always strictly higher.
2. **Epoch stamped on every RPC response.** The coordinator tags heartbeat/control/work responses with its `coord_epoch` (an added proto field on the existing 3 channels — Claude's discretion, "internal RPC/proto additions"). Workers store the highest epoch seen.
3. **Workers reject stale-epoch responses (D-FENCE-04).** Any response with `epoch < seen_max` is rejected; the worker treats the sender as a deposed coordinator.
4. **Timing guarantee (already enforced).** `worker_self_fence_timeout (4s) < coordinator_failure_timeout (5s)` — enforced at plan time by `TransportConfig::validate_cross_fields` (`config.rs:62`). A partitioned worker stops being authoritative (4s) before the coordinator would declare it dead (5s) and before the lease TTL (5s) lets a new coord take over. So when the new coordinator (epoch N+1) starts answering, the old one's workers have already self-fenced or will reject epoch N.
5. **Loser self-fences (D-FENCE-01/02/03).** When the old coordinator's `renew()` returns `false` (its `expected` bytes are stale because epoch advanced), it KNOWS it lost. It must: stop all shared-state I/O immediately → emit ONE `coordinator_fenced` event (observability sink only, no `kv` write) → `std::process::abort()` within 5s. No graceful flush (rejected — would race the survivor's epoch).

**`coordinator_fenced` event shape** (extends the spec-09 taxonomy, Claude's discretion):

```rust
Event {
    kind: EventKind::Domain { topic: SmolStr::new("coordinator_fenced") },
    level: Level::Error,
    run_id: Some(run_id),
    worker_id: Some(coord_instance_id),
    attrs: json!({ "stale_epoch": old.0, "observed_epoch": new.0 }),
    // ...
}
```

Mirrors the `worker_failed` emit in `failure_scan.rs:84`. Critically it goes through the `EventEmitter` (stdout/OTLP sink), **not** through `storage.begin()/put_bytes` — the loser writes nothing to shared state.

## Work-Stealing + CAS Dedup (DIST-02)

**Steal protocol (coordinator-mediated, spec 05 §5):** workers never talk peer-to-peer.

```text
idle worker A (local queue empty, D-STEAL-01):
    A --steal_request(epoch)--> coordinator
    coordinator: pick victim = busiest peer by backlog (D-STEAL-03)
                 n = min(ceil(victim_backlog/2), MAX_STEAL_BATCH)   (D-STEAL-02)
                 reassign n WorkItemRecords from victim to A via CAS:
                    for each item in victim's Running set:
                        try_repending(item)            # Running(victim) -> Pending  [CAS]
                        try_claim(item, A)             # Pending -> Running(A)        [CAS]
    coordinator --stolen items + epoch--> A
    A processes; submits results via normal ack (try_complete)
```

**Why this is idempotent (D-STEAL-04 / `concurrent_ack_and_steal_no_double_execute`):** the dedup is the **CAS expected-bytes drift** already proven in `cas_state_machine.rs`. Consider the race: victim finishes item X (acks `Running(victim) -> Done`) at the same instant the coordinator tries to steal X (`Running(victim) -> Pending`). Both CAS against `expected = Running(victim)` bytes; **exactly one wins**:

- If victim's ack wins → item is `Done`; the steal's `try_repending` sees stale `expected`, returns `false`, X is not stolen. No double-execute.
- If steal wins → X is `Pending`, reassigned to A; victim's late ack sees stale `expected`, returns `false`, victim's result is dropped (it never finished a committed claim). No double-execute.

This is the same single-winner property as `cas_state_machine.rs::pending_to_running_to_done_round_trip` ("second claim must lose"). The witness `concurrent_ack_and_steal_no_double_execute` is a direct adaptation: spawn an ack task and a steal task racing the same `Running` record, assert exactly one CAS returns `true` and the final state is reached once.

**`MAX_STEAL_BATCH`** is a fixed const (D-STEAL: not configurable for v1.1). Suggest a conservative const (e.g. 32) defined alongside the steal logic, documented as "fixed; revisit if tuning need emerges."

## Spot-Drain Orchestration (DIST-04)

**Signal source already exists.** `ComputeHint::preemption_signal() -> Result<Option<Duration>, _>` returns `Some(lead)`:
- AWS: `Some(Duration::from_secs(120))` on `/latest/meta-data/spot/instance-action` (`imds/mod.rs:72`).
- GCP: `Some(GCE_PREEMPT_LEAD = 30s)` on `instance/preempted == "TRUE"` (`mds/mod.rs:24,113`).

DIST-04 adds only the **drain state machine** that polls `preemption_signal()` and, on `Some`, runs:

```text
drain(deadline):     # deadline = 60s AWS / 15s GCP (conservative; D-SPOT-01)
    1. stop-pull        : worker leaves `running`, refuses new pull/steal
    2. requeue in-flight: for each in-flight item, queue.nack(id)  (lease nack -> back to Pending)
                          (uses Queue::nack / extend_lease surface already in cloud.rs)
    3. opportunistic    : if remaining_budget allows -> snapshot TrainState (D-SPOT-03, TrainState ONLY)
       snapshot           else skip (lost work since last snapshot is requeued -> recomputed)
    4. deregister       : coordinator.deregister(worker)  (already in heartbeat.rs:74)
    5. ack-exit         : clean process exit (exit 0)
    must complete within `deadline` (60s/15s), leaving margin before forced reclaim (120s/30s)
```

**Two-number discipline (D-SPOT-01/04):** the *notice lead* (120/30) is what the cloud gives; the *drain deadline* (60/15) is the budget the state machine targets, leaving 60s/15s of margin. The witness `spot_drain_completes_within_lead_time` asserts the whole sequence completes within the **deadline** (60/15), not the lead. **Doc reconciliation (D-SPOT-04):** REQ DIST-04 currently says "120s/30s" and the roadmap says "60s/15s" — update both to state BOTH numbers with the notice-vs-deadline distinction. This is a planned task, not just research.

**Idempotent requeue:** `nack` returns the item to `Pending` via CAS; if the worker dies before exit, the failure_scan stale-`Running` path (`try_repending` after `coord_failure_timeout`) requeues it anyway. Either path is safe — no double-execute because the next claimant goes through `try_claim`.

## Standard Stack

No new external libraries. Everything is in-tree.

| Component | Source (in-repo) | Role in Phase 6 |
|-----------|------------------|-----------------|
| `StorageTxn::cas_bytes` | `rollout-core::traits::storage` (impls: `embedded/txn.rs`, `postgres/mod.rs`) | lease CAS + work-ledger CAS, both backends |
| CAS state machine | `rollout-runtime-batch::state` (`try_claim`/`try_complete`/`try_repending`) | work-item dedup; generalize for `WorkItemRecord` |
| `health::is_failed` / `next_due_at` | `rollout-transport::health` | lease TTL expiry == coord-failure detection |
| `TransportConfig::validate_cross_fields` | `rollout-transport::config` | `self_fence < coord_failure` plan-time invariant (extend for lease timing) |
| `ComputeHint::preemption_signal` | `rollout-cloud-{aws,gcp}` | spot-drain trigger (signal already wired) |
| `Queue` nack/extend_lease/`InMemQueue` | `rollout-core::traits::cloud`, `rollout-cloud-local::queue` | drain requeue + spill/replay template |
| `EventEmitter` | `rollout-coordinator::emitter` (`StdoutJsonEmitter`) | `coordinator_fenced` emit (no shared-state write) |
| `CoordinatorImpl` + `failure_scan_loop` + `run()` | `rollout-coordinator` | EXTEND with lease/epoch/ledger/steal; do not rewrite |

**Version verification:** N/A — no new crate dependencies. The only crates touched (`ulid`, `postcard`, `blake3`, `tokio`, `async-trait`, `sqlx`) are already pinned in the workspace and load-bearing in shipped Phase 2/4/5 code.

## Architecture Patterns

### Recommended placement

```text
rollout-core/src/traits/lease.rs      # NEW: CoordEpoch, LeaseRecord, CoordinatorLease trait (no SDK dep)
rollout-coordinator/src/
  ├── lease.rs                         # NEW: StorageLease (generic over Arc<dyn Storage>; both backends)
  ├── epoch.rs                         # NEW: epoch read/advance + RPC stamping helpers
  ├── ledger.rs                        # NEW: WorkItemRecord + replay (reuse runtime-batch CAS, or share it)
  ├── steal.rs                         # NEW: victim selection + ceil(n/2) cap + CAS reassign
  ├── drain.rs                         # NEW: spot-drain state machine
  ├── fence.rs                         # NEW: fence_old_coordinator() decision + coordinator_fenced emit
  ├── heartbeat.rs                     # EXTEND: stamp epoch on responses; lease-aware register
  ├── failure_scan.rs                  # EXTEND: stale-Running -> try_repending; lease renew loop
  └── run.rs                           # EXTEND: lease acquire -> replay -> serve boot order
rollout-coordinator/tests/
  ├── split_brain.rs                   # split_brain_old_coord_self_fences
  ├── coord_restart.rs                 # coord_restart_no_duplicates
  ├── steal_dedup.rs                   # concurrent_ack_and_steal_no_double_execute
  └── spot_drain.rs                    # spot_drain_completes_within_lead_time
```

### Pattern: dependency-direction safety (CORE-02 / AGENTS.md §9)

The `CoordinatorLease` trait lives in `rollout-core` (no SDK). `StorageLease` lives in `rollout-coordinator` and depends only on `rollout-core::Storage` — never on `rollout-cloud-*`. The preemption signal is consumed via the `ComputeHint` trait (core), with the concrete AWS/GCP impl injected at the binary edge. **`coord ↛ cloud` stays green** because the coordinator only ever sees core traits. Verify with `cargo deny check` (CORE-02).

### Anti-patterns to avoid

- **New consensus library (etcd/raft).** Explicitly rejected by the roadmap. The CAS lease + stateless replayer IS the design.
- **New `coordinator_lease` table as the primary path.** The generic `kv` CAS row already works on both backends; a typed table is optional specialization only.
- **Read-modify-write the lease without CAS.** Always `cas_bytes(expected=exact prior bytes, …)`. A bare `put_bytes` reintroduces the split-brain race.
- **Graceful flush by the fenced coordinator.** D-FENCE-03 rejects it. The loser writes nothing and aborts.
- **Testing `std::process::abort()` in-process.** It kills the test runner; split decision from abort (see test skeleton §6).

## Don't Hand-Roll

| Problem | Don't build | Use instead | Why |
|---------|-------------|-------------|-----|
| Single-winner lease acquisition | bespoke lock/spinlock | `StorageTxn::cas_bytes` | atomic on both redb + Postgres (`FOR UPDATE`); already tested |
| Work-item dedup / idempotent steal | new dedup index | `try_claim`/`try_complete` CAS state machine | proven in `cas_state_machine.rs`; same single-winner property |
| Failure detection / lease TTL | custom timers | `health::is_failed` + `next_due_at` (2× slack) | matches Phase-2 deadline model; one timing source of truth |
| Restart queue recovery | custom WAL | scan-namespace-on-open (`InMemQueue::open`) | ULID-ordered replay already shipped |
| Preemption detection | poll IMDS/MDS yourself | `ComputeHint::preemption_signal()` | wired in Phase 5; returns the lead duration directly |
| Coordinator consensus | etcd/raft/zookeeper | CAS lease + stateless replayer | roadmap-mandated; single-coordinator model |

**Key insight:** every "distributed systems hard part" in this phase reduces to an existing `cas_bytes` call. The phase is wiring, not algorithm design.

## Common Pitfalls

### Pitfall 1: Lease renew vs steal epoch confusion
**What goes wrong:** `renew` accidentally advances the epoch, or `steal` reuses the old epoch.
**Avoid:** `renew` keeps `epoch` constant (same holder); only `try_acquire`-on-expiry does `epoch+1`. The test asserts `renew` after a steal returns `false` (you were fenced) and never mutates the row.

### Pitfall 2: CAS expected-bytes drift from serialization nondeterminism
**What goes wrong:** postcard re-encode of "the same" `LeaseRecord` produces different bytes, so CAS `expected` never matches.
**Avoid:** CAS on the **exact bytes read back** from storage (as `try_claim` does — it re-encodes the *input* record it was handed, which is the decoded prior value). Keep `LeaseRecord` fields stable; don't include wall-clock fields that change between read and compare beyond `expires_at_ms` (which IS the value being swapped). Property-test encode/decode round-trip stability.

### Pitfall 3: Clock-skew letting two coordinators both think the lease is expired
**What goes wrong:** worker A's clock is ahead; it steals while incumbent B still thinks it holds a valid lease.
**Avoid:** the CAS itself is the arbiter — even if both think it's expired, only one CAS wins (the other's `expected` is stale). Skew only affects *when* a steal is attempted, never *whether two win*. The `clock_skew_budget` (250ms) is already validated against `heartbeat_interval × 2`.

### Pitfall 4: Replayer requeuing in-flight work that a live worker still holds
**What goes wrong:** fresh coordinator sees `Running{worker}` and immediately requeues, causing double-execution.
**Avoid:** D-RESTART-02 — reconstruct the assignment map, do NOT requeue `Running` items on boot. Only the failure_scan path requeues a `Running` item after `coord_failure_timeout` of staleness (via `try_repending`), and the re-claim goes through `try_claim` so it's idempotent.

### Pitfall 5: `process::abort()` skips destructors and flushes
**What goes wrong:** abort is intentionally violent — no `Drop`, no buffer flush. That's correct for fencing (D-FENCE-03) but the `coordinator_fenced` event must be emitted (and ideally flushed to stdout/OTLP) BEFORE the abort, or it's lost.
**Avoid:** emit + flush the single event synchronously, then abort. Document that the emit path must not touch shared `kv` state.

### Pitfall 6: Doc drift on the two spot numbers (D-SPOT-04)
**What goes wrong:** REQ says 120s/30s, roadmap says 60s/15s; a reader can't tell which is the test bound.
**Avoid:** ship a doc-reconciliation task updating BOTH to state notice-lead (120/30) AND drain-deadline (60/15). The witness asserts the deadline (60/15).

## Validation Architecture

> nyquist_validation is enabled (`config.json` workflow.nyquist_validation = true).

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[tokio::test]` (workspace standard) + `tempfile` for embedded storage |
| Config file | none (cargo test); CI lanes in `.github/workflows/*.yml` |
| Quick run command | `cargo test -p rollout-coordinator` |
| Full suite command | `cargo test --workspace` (embedded/Docker-free) + `postgres-integration` lane for PG lease |

### Success Criterion → Witness Test → Observable Signal
| Roadmap SC | Behavior | Witness (in-process, Docker-free) | Observable signal |
|------------|----------|-----------------------------------|-------------------|
| SC1 (lease exclusion, DIST-01) | one coordinator per run | `lease_exclusion_single_winner` (two `try_acquire` race → one `Some`, one `None`) | CAS returns `true` for exactly one caller; `current()` shows one holder |
| SC2 (restart invisible, DIST-03) | kill+restart emits no duplicates | `coord_restart_no_duplicates` | every `work_id` reaches `Done` exactly once; replayed acks return `false` from `try_complete` |
| SC3 (work-steal dedup, DIST-02) | stolen-then-reclaimed never double-runs | `concurrent_ack_and_steal_no_double_execute` | exactly one of {ack CAS, steal CAS} returns `true`; final state reached once |
| SC4 (split-brain fence, DIST-05) | old coord self-fences ≤5s | `split_brain_old_coord_self_fences` (in-proc decision) + subprocess abort in smoke | exactly ONE `coordinator_fenced` event; no `kv` write by loser; abort within 5s (smoke) |
| SC5 (spot-drain, DIST-04) | drain completes within deadline | `spot_drain_completes_within_lead_time` | sequence stop-pull→requeue→snapshot?→dereg→exit completes < 60s (AWS) / 15s (GCP) |

### Sampling Rate
- **Per task commit:** `cargo test -p rollout-coordinator` (all 5 witnesses, embedded, < 30s)
- **Per wave merge:** `cargo test --workspace` + `cargo deny check` (coord ↛ cloud) + `cargo sqlx prepare --check`
- **Phase gate:** full suite green + `postgres-integration` lane (PG lease) + `make smoke-3node-aws`/`-gcp` (operator, real cloud transport, mock backend, no GPU) before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `crates/rollout-coordinator/tests/split_brain.rs` — SC4 (skeleton in spike §6)
- [ ] `crates/rollout-coordinator/tests/coord_restart.rs` — SC2
- [ ] `crates/rollout-coordinator/tests/steal_dedup.rs` — SC3
- [ ] `crates/rollout-coordinator/tests/spot_drain.rs` — SC5
- [ ] `crates/rollout-coordinator/tests/lease.rs` — SC1 + Pitfall-2 encode/decode round-trip property
- [ ] Subprocess abort harness for SC4's `std::process::abort()` (smoke-only; cannot run in-process)
- [ ] `make smoke-3node-aws` / `make smoke-3node-gcp` Makefile targets (1 coord + 3 workers, mock backend)
- [ ] Framework install: none — `#[tokio::test]` + `tempfile` already workspace deps

## Environment Availability

| Dependency | Required by | Available | Version | Fallback |
|------------|-------------|-----------|---------|----------|
| Rust toolchain | all | ✓ (workspace MSRV 1.88; bump-to-1.91 spike is a Phase-5 precursor) | — | — |
| `cargo deny` | CORE-02 dep-direction lint | assumed ✓ (CI) | — | — |
| `sqlx` CLI / offline cache | PG lease lane | ✓ (`.sqlx/` cache in repo) | — | generic-`kv` lease needs no new SQL |
| Postgres 16 | `postgres-integration` lane only | gated (Docker) | 16 | embedded redb path runs all 4 witnesses Docker-free (D-LEASE-02) |
| AWS/GCP live cloud | `make smoke-3node-*` (operator) | not in CI | — | embedded transport smoke; live smoke is operator-run |

**No blocking missing dependencies.** Every per-commit witness runs Docker-free on the embedded path. Postgres and live cloud are gated lanes with the embedded path as the always-available fallback.

## Sources

### Primary (HIGH confidence — repo source of truth)
- `crates/rollout-core/src/traits/storage.rs:97-143` — `Storage` / `StorageTxn::cas_bytes` surface
- `crates/rollout-storage/src/{embedded/txn.rs:122,postgres/mod.rs:242}` — dual CAS impls (value-compare, `FOR UPDATE`)
- `crates/rollout-runtime-batch/src/state.rs` + `tests/cas_state_machine.rs` — CAS state-machine + single-winner witness template
- `crates/rollout-transport/src/{health.rs,config.rs:60-80}` — deadline health + `self_fence < coord_failure` invariant
- `crates/rollout-core/src/traits/cloud.rs:142-188` — `ComputeHint::preemption_signal`, `Queue` nack/extend_lease
- `crates/rollout-cloud-{aws/src/imds/mod.rs:72,gcp/src/mds/mod.rs:24,113}` — 120s/30s preemption leads
- `crates/rollout-cloud-local/src/queue.rs` — scan-on-open spill/replay template
- `crates/rollout-coordinator/src/{heartbeat.rs,failure_scan.rs,run.rs,registry.rs}` — Phase-2 scaffolding to extend
- `database/migrations/*.sql` — `kv` table DDL (`PRIMARY KEY (namespace, run_id, path)`)
- `docs/specs/05-distribution.md` §5-8 — steal protocol, coordinator failure, lease, exactly-one-winner
- `docs/specs/04-storage-snapshots.md` — namespaces, specialized tables, TrainState-only snapshot
- `docs/specs/09-observability.md` — `EventKind::Domain { topic }` shape for `coordinator_fenced`
- `.planning/phases/06-multi-node-distribution/06-CONTEXT.md` — locked decisions

### Secondary / Tertiary
- None — no external web sources were required; the phase is fully grounded in in-tree primitives.

## Metadata

**Confidence breakdown:**
- DIST-03 spike (lease + replay + test skeleton): HIGH — all primitives verified in source
- Epoch fencing: HIGH — invariant already enforced by `validate_cross_fields`
- Work-steal dedup: HIGH — direct reuse of proven CAS state machine
- Spot-drain: HIGH — signal source verified; only the state machine is new
- DDL: HIGH for generic-`kv`; MEDIUM for the optional typed table (untested, additive)

**Research date:** 2026-05-29
**Valid until:** ~2026-06-28 (stable; in-tree, no fast-moving external deps)
