# Spec 05 — Distribution

This spec covers how `rollout` runs on more than one machine: transport, work distribution, work-stealing, fault tolerance, and coordinator recovery.

## 1. Purpose

Multi-node distribution must be a property of the framework, not an extension. v1 supports clusters of 2–N nodes from a single coordinator with the following guarantees:

- **Liveness:** if at least one healthy worker remains and the coordinator is alive (or recoverable), the run makes progress.
- **No double-execution under retry:** content addressing makes retried work idempotent; the coordinator never produces duplicate output for the same input.
- **Bounded recovery time:** a single worker failure costs at most `heartbeat_lease + clock_skew_budget` of detection latency; a coordinator restart costs at most `coordinator_recovery_budget` (default: 30 seconds).
- **Split-brain prevention:** worker self-fence timeout < coordinator failure timeout.

## 2. Topology

```
            ┌──────────────────────────────┐
            │  Coordinator (1 per run)     │
            │  - owns Plan, Schedule       │
            │  - owns Queue + Worker reg.  │
            │  - state persisted to Storage│
            └──┬───────────────┬──────────┘
               │ heartbeats    │ control events
   ┌───────────┴──┐         ┌──┴──────────┐
   │              │         │             │
   ▼              ▼         ▼             ▼
 ┌───────┐    ┌───────┐  ┌───────┐    ┌───────┐
 │Actor 1│    │Actor 2│  │Learn 1│    │ ...   │
 └───┬───┘    └───┬───┘  └───┬───┘    └───────┘
     │            │          │
     ▼            ▼          ▼
  ┌──────────────────────────────┐
  │  Object store (S3/GCS/local) │
  └──────────────────────────────┘
```

A run has exactly one coordinator at any time. Multiple coordinators is a future capability and explicitly out of v1 scope.

## 3. Transport

`rollout-transport` is the inter-process transport library. Implementation: **gRPC over QUIC**. Properties:

- TLS by default; mutual TLS in production.
- Multiplexing without head-of-line blocking (QUIC's main benefit over HTTP/2).
- Built-in deadlines, retries, connection migration.
- Streaming for bulk data; unary for control.

### Channels

Three logical channels between coordinator and worker, multiplexed over one QUIC connection:

1. **Heartbeat channel** (unary, frequent). Worker → coordinator. Short timeout. Never blocked by other traffic.
2. **Control channel** (server-streaming). Coordinator → worker. Drain requests, snapshot requests, cancellation, config reloads.
3. **Work channel** (bidirectional streaming). Worker pulls work, submits results. The bulk of traffic.

For intra-node worker ↔ sidecar communication, the same gRPC schema runs over a UNIX domain socket.

### Bulk data

Trajectories, weights, and snapshots are **not** sent over gRPC. They are written to the object store; the message carries only the `ContentId`. This keeps the control plane small and lets the data plane scale independently.

## 4. Work distribution

Two patterns. The choice is per-role, configured at plan time.

### 4.1 Pull-based (high throughput)

Used by: batch inference, rollout collection, dataset loading.

- The coordinator owns a queue of `WorkItem`s.
- Workers in `running` state long-poll `coordinator.pull(worker, budget)`.
- Budget is a tuple of `(max_items, max_tokens, max_walltime)`. The coordinator returns up to the budget.
- Submission is via `coordinator.submit(worker, results)`. Idempotent on content-addressed `WorkItem.id`.

**Why pull, not push:** workers know their own capacity; the coordinator does not. Pull naturally backpressures.

### 4.2 Push-based (low latency)

Used by: online inference serving.

- A balancer (in front of inference workers) routes incoming requests directly to workers.
- Sticky-by-session for tool-using agents (per-session state lives on one worker).
- Health checks come from the same deadline-based heartbeats; an unhealthy worker is removed from the balancer's pool.

## 5. Work-stealing

When the global queue is empty but some workers still have in-flight batches, idle workers can **steal** work from a busy peer.

### Protocol

```
idle Worker A:                                busy Worker B:
   ──pull(coordinator)──▶  (empty)
   ──steal_request(coordinator)──▶
                              ──forwarded_steal_request──▶
                                                       (B examines local backlog)
   ◀──── stolen items ──────────────────────────────── B yields N items
   (A processes; submits results normally)
```

The coordinator is the broker; workers do not talk peer-to-peer directly. This simplifies the security model (one mutual-TLS edge instead of N²) and keeps coordination logic in one place.

Steal requests are bounded: at most `K` items per steal; a worker can refuse if it's near its `WorkBudget` limit.

### Coordinator-mediated steal + CAS dedup (DIST-02)

The v1.1 implementation (`rollout-coordinator::{ledger, steal}`) is coordinator-mediated and reactive:

- **Trigger (D-STEAL-01).** A worker steals only when its local queue drains to empty (`ledger::backlog(thief) == 0`). A non-idle thief's request is a no-op.
- **Victim (D-STEAL-03).** The coordinator picks the **busiest peer by `Running` backlog** (`ledger::busiest`, excluding the thief). Workers never talk peer-to-peer.
- **Batch (D-STEAL-02).** `n = min(ceil(victim_backlog / 2), MAX_STEAL_BATCH)`. `MAX_STEAL_BATCH = 32` is a fixed const for v1.1 (not a config knob).
- **Reassign (D-STEAL-04).** For each of the first `n` of the victim's `Running` items, the coordinator runs a two-step CAS over the SAME prior `Running(victim)` bytes: `try_repending` (`Running(victim) → Pending`) then `try_claim` (`Pending → Running(thief)`).

**Stolen-then-reclaimed items never double-execute.** The dedup is the CAS expected-bytes drift of the `WorkItemRecord` state machine. When the victim's ack (`try_complete`, `Running(victim) → Done`) races the steal's `try_repending` (`Running(victim) → Pending`), both CAS against the same `expected` bytes and **exactly one wins**:

- ack wins → item is `Done`; the steal's `try_repending` sees stale `expected`, returns `false`, the item is skipped (never stolen, never re-run);
- steal wins → item is `Pending` and reassigned to the thief; the victim's late ack sees stale `expected`, returns `false`, and its result is dropped (the victim never committed a terminal transition).

This is the same single-winner property as the batch runtime's `cas_state_machine.rs`, witnessed every commit by `concurrent_ack_and_steal_no_double_execute` (SC5, Docker-free, in-process over `EmbeddedStorage`).

The pending (unassigned) dispatch queue lives in the `queue_items` namespace, keyed `["q", <ulid>]`; entries dispatch in ULID (insertion) order, mirroring `InMemQueue`. `dispatch` content-addresses the payload into a deterministic `work_id` (blake3, CORE-05), writes a `Pending` `WorkItemRecord`, and `try_claim`s it for the worker.

## 6. Fault tolerance

### Worker failure

- **Detection:** deadline-based. If `now > worker.next_heartbeat_due_at + clock_skew_budget`, the worker is marked failed.
- **Recovery:** in-flight work for that worker is moved back to the queue. Idempotent submission ensures no double-counting.
- **Bounded retries per work item.** A work item that fails on `max_retries` workers is moved to a dead-letter queue and surfaces a `WorkItemPoison` error.

### Coordinator failure

- **State persistence:** the coordinator persists all state to `Storage` continuously. Queue head, worker registry, heartbeat ledger, in-flight assignments — all durable.
- **Restart:** a new coordinator process picks up state from `Storage`. Workers' control-channel long polls time out; they retry; the new coordinator answers.
- **Lease:** the coordinator holds a CAS-based lease on `runs/<id>/coordinator_lease`. The lease has a TTL. A would-be new coordinator must observe the lease expired before claiming it. This prevents two coordinators from operating simultaneously.

#### Lease / epoch / fence model (DIST-01 / DIST-05)

The lease is a **single-row CAS** over `StorageTxn::cas_bytes` (one impl, `StorageLease`, serving both the embedded redb and Postgres backends — `cas_bytes` is already dual-backed). It is not a new storage primitive.

- **Single-row CAS.** The `coordinator_lease` namespace holds exactly one row per run (`LeaseRecord { holder, epoch, expires_at_ms }`). `try_acquire` is one CAS on the exact prior bytes: a fresh row claims `epoch = 0`; an **expired** row is stolen with `epoch = prev.epoch + 1`. Two coordinators racing an expired lease both CAS against the same `expected` bytes — exactly one wins; the loser's `expected` is stale and the CAS returns `false` (spec §8 "exactly one wins; loser exits").
- **Monotonic epoch on steal; constant on renew.** A successful steal advances the epoch by one inside the same CAS that claims the lease, so a new coordinator is always strictly higher than the one it deposed. `renew` (the incumbent's heartbeat on the lease, cadence = `heartbeat_interval`, TTL = `coordinator_failure_timeout`) keeps the epoch constant. The same CAS writes the authoritative `epoch` namespace row, so lease-epoch and ledger-epoch never diverge.
- **Epoch stamped on every RPC; workers reject stale epochs.** The coordinator stamps its `coord_epoch` on heartbeat / control / work responses. Each worker keeps the highest epoch seen (`EpochGuard`) and rejects any response tagged below it as coming from a deposed coordinator (D-FENCE-04). Combined with the plan-time `worker_self_fence_timeout (4s) < coordinator_failure_timeout (5s)` invariant, a partitioned worker stops being authoritative before a new coordinator can take over.
- **Self-fence on lost renew.** When the old coordinator's `renew()` returns `false` (its `expected` bytes are stale because the epoch advanced), it knows it lost. It (1) stops all shared-state I/O immediately — writes nothing (D-FENCE-01); (2) emits **exactly one** `coordinator_fenced` observability event through the `EventEmitter` (an observability sink only — never `storage`/`cas_bytes`), flushed synchronously (D-FENCE-02 / Pitfall 5); then (3) `std::process::abort()`s within the 5s bound (D-FENCE-03). No graceful flush — that would race the survivor's epoch.
- **`--test-fence` abort edge.** The coordinator binary carries a hidden `--test-fence <stale> <observed>` subcommand that runs the real `fence_old_coordinator` decision then `std::process::abort()`. It exists so the SC4 subprocess witness (`fence_aborts_within_5s`) can exercise the actual abort in a child process without killing the test runner; the in-process witness (`split_brain_old_coord_self_fences`) asserts the decision + single-event + no-write properties.

### Network partition

- A worker that cannot reach the coordinator publishes its next heartbeat to **storage directly** (storage is multi-AZ in production; the partition is typically transport-level, not storage-level).
- If even storage is unreachable: the worker **self-fences** after `worker_self_fence_timeout` and stops processing.
- **Invariant:** `worker_self_fence_timeout < coordinator_failure_timeout`. The worker stops being authoritative before the coordinator declares it dead.

### Spot / preemption

- Cloud preemption notifications (AWS spot ITN, GCP preemption) are caught by the worker.
- On notification: trigger an opportunistic **process snapshot** (spec 04). If process snapshot succeeds, the worker's state can be resumed on a new node.
- If process snapshot fails: rely on TrainState + Buffer snapshots. Lost work since the last snapshot is requeued.

## 7. Resource hints and scheduling

The scheduler (spec 01) consumes:

- Worker resource declarations (GPUs, RAM, network egress) from the algorithm's `required_roles`.
- Node resource availability from cloud `ComputeHint` impls (spec 06).
- Affinity/anti-affinity from plugin manifests (spec 03).

Constraints (v1):

- A worker requiring `n` GPUs gets `n` GPUs **exclusively**. No fractional sharing in v1.
- Workers exchanging high-bandwidth data prefer same-zone placement when zone metadata is available.
- Co-location of actor + harness sidecar on the same node is the default; opt out with `placement.allow_remote_harness = true`.

## 8. Failure modes

| Failure | Detection | Recovery |
|---|---|---|
| Worker dies | deadline-based heartbeat scan | requeue in-flight; if recurring, mark plan failed |
| Coordinator dies | worker control long-poll times out | new coordinator instance claims lease, picks up state from storage |
| Both coordinators race for lease | CAS on lease key in storage | exactly one wins; loser exits |
| Network partition (worker isolated) | worker self-fence | when partition heals, worker re-registers |
| All workers die simultaneously | coordinator observes empty fleet | plan marked failed; alarm emitted |
| Storage outage | coordinator + workers fail health probe | run drains; on restore, run resumes from last persisted state |
| Object store outage | snapshot / trajectory write fails | retry per `RetryHint`; persistent failure drains run |
| Clock skew exceeds budget | coordinator metric | alarm; do not mark workers failed; human intervention |

## 9. Test contract

- **Unit:** scheduler placement against synthetic fleets.
- **Integration (local multi-process):** 1 coordinator + N workers on one host, IPC via local QUIC.
- **Integration (multi-host):** small cluster (2 nodes) via testcontainers.
- **Chaos (CI nightly):** N workers, random kills, network drops between random pairs, clock jitter via NTP simulation. The run must either complete successfully or fail with a typed error — never deadlock or duplicate output.
- **Split-brain:** force a network partition that lets a worker think it's alive while the coordinator marks it dead. Verify worker self-fences before the coordinator's mark.
- **Coordinator restart:** kill the coordinator mid-run; restart. Verify all in-flight work is accounted for, no duplicates emitted.

## 10. Open questions

- **Multi-coordinator (HA):** post-v1. v1 is single-coordinator with fast restart.
- **Encrypted object-store traffic:** assumed yes via cloud SDKs' defaults; document the contract per cloud.
- **NCCL-aware scheduling:** v2. v1 scheduler does bin-packing + zone preference, no NCCL topology awareness.
- **Cross-zone vs intra-zone learner-actor placement:** measure in Phase 6 with a real cluster; pick default based on data.
- **Queue ordering:** FIFO default. Priority queue support deferred.
