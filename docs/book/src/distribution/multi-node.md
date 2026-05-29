# Multi-node distribution

rollout runs a single training/inference run across many machines from one
**coordinator**. This chapter is the operator's view of how that works: the
lease that keeps exactly one coordinator alive, the work-stealing that keeps
workers busy, how a coordinator restart is invisible to progress, and how a spot
preemption drains gracefully. The implementation lives in `rollout-coordinator`;
the contract is [spec 05 — Distribution](../../../specs/05-distribution.md).

## Coordinator + worker model

A run has **exactly one coordinator** at any time and N workers. The coordinator
owns the work ledger and brokers all coordination; workers never talk
peer-to-peer (one mutual-TLS edge per worker instead of N²). Workers long-poll
the coordinator for work, run it on their backend (vLLM in production, a mock
backend in the smoke), and submit content-addressed results. Submission is
idempotent on the content-addressed `WorkItem.id`, so a retried item never
produces duplicate output.

## Single-row CAS lease + monotonic epoch

"Exactly one coordinator per run" is enforced by a **single-row compare-and-swap
lease** (`StorageLease`) over the `coordinator_lease` storage row. One impl rides
the dual-backed `StorageTxn::cas_bytes`, so the SAME lease semantics hold on both
the embedded redb backend and Postgres (D-LEASE-01) — the Postgres path is proven
in the `postgres-integration` CI lane (`postgres_lease.rs`).

- **Acquire** — a fresh coordinator CASes the empty row to `epoch = 0`.
- **Steal-on-expiry** — if the lease TTL has passed, a new coordinator CASes the
  exact prior (expired) bytes to `epoch + 1`. The epoch is **monotonic**: it only
  ever advances.
- **Renew** — the incumbent heartbeats by CASing its own bytes forward, keeping
  the epoch constant. A renew that finds the epoch has advanced under it returns
  `false` — the incumbent has been fenced.

The lease TTL equals `coordinator_failure_timeout`; the renew cadence is strictly
shorter than the TTL. Every winning claim stamps the authoritative `epoch` row in
the **same** transaction, so the lease epoch and the ledger epoch never diverge.

## Work-stealing

When the global queue is empty but a peer is still busy, an **idle** worker steals
work — coordinator-mediated, never peer-to-peer
(`rollout-coordinator::{ledger, steal}`):

- **Trigger** — a worker steals only when its local backlog drains to empty
  (`backlog(thief) == 0`); a non-idle thief's request is a no-op.
- **Victim** — the coordinator picks the **busiest peer** by `Running` backlog.
- **Batch** — it reassigns `ceil(victim_backlog / 2)` items, capped at
  `MAX_STEAL_BATCH = 32`.
- **Reassign** — each item is moved via a two-step CAS over the same prior
  `Running(victim)` bytes: `try_repending` (`Running → Pending`) then `try_claim`
  (`Pending → Running(thief)`).

**Stolen-then-reclaimed items never double-execute.** If the victim's ack races
the steal, both CAS the same expected bytes and exactly one wins — the loser sees
stale bytes and is dropped. This single-winner property is witnessed every commit,
Docker-free, by `concurrent_ack_and_steal_no_double_execute`.

## Coordinator restart (stateless replayer)

The coordinator holds **no run state in memory that it cannot rebuild from
storage**. On boot it: wins the lease, adopts the advanced epoch, then
`scan_bytes`es the `work` ledger and reconstructs its in-flight assignment map:

- `Running{worker}` rows are reconstructed **in-flight** — NOT requeued, because
  the worker may still hold the item (only the failure-scan stale path re-pends
  after `coordinator_failure_timeout`);
- `Pending` rows go back onto the dispatch queue;
- `Done` / `Failed` rows are terminal and skipped — replayed acks are idempotent.

So a coordinator restart is **invisible to progress**: a fresh coordinator boots
over the same storage and the run completes with zero duplicate sample IDs. This
is witnessed every commit by `coord_restart_no_duplicates` (SC2).

## Spot-drain (graceful preemption)

When a worker's cloud reports a spot preemption, the drain state machine
(`rollout-coordinator::drain`) runs within a conservative deadline: set a
stop-pull flag → nack each in-flight item (back to `Pending` for another worker)
→ opportunistically snapshot **TrainState only** if the remaining budget covers
the cost → deregister → exit cleanly.

Two numbers, kept distinct (D-SPOT-01/04):

| Provider | Notice lead (cloud gives) | Drain deadline (we target) |
| --- | --- | --- |
| AWS | 120s | 60s |
| GCP | 30s | 15s |

The notice lead is what the cloud promises; the drain deadline is the budget the
state machine targets, leaving margin. The preemption signal is consumed **only**
through the `ComputeHint` trait — the coordinator never imports a cloud SDK
(coord ↛ cloud; the dependency-direction lint enforces it). Witnessed every commit
by `spot_drain_completes_within_lead_time` (SC3), for both AWS and GCP.

## Split-brain fencing

If a coordinator is deposed (its lease was stolen and the epoch advanced), it
**self-fences**: it emits exactly one `coordinator_fenced` observability event
(never a shared-store write) and then `std::process::abort()`s within 5s. Workers
reject any RPC response carrying a stale `coord_epoch` (`EpochGuard`), so a deposed
coordinator's late replies can never corrupt the run. The abort edge is the hidden
`rollout-coordinator test-fence` subcommand, driven by the SC4 subprocess witness
`fence_aborts_within_5s`.

## Operator recipe: the 3-node smoke

`make smoke-3node-aws` / `make smoke-3node-gcp` boot 1 coordinator + 3 workers
(mock backend, no GPU, no Docker) over an auto-generated dev CA, drive the work
ledger through dispatch + a real steal, and assert the run reports `done` within
30s with a steal observed:

```bash
make smoke-3node-aws      # local-transport wiring run (free, Docker-free)
make smoke-3node-gcp
```

By default the smoke runs over the **local mTLS transport** so it is reproducible
on a free runner. To run the same topology over the real cloud transport (operator
path; needs real AWS/GCP credentials and ~4 hosts):

```bash
ROLLOUT_SMOKE_CLOUD=1 make smoke-3node-aws
```

The live-cloud run is the operator-optional path; the every-commit named witnesses
(`coord_restart_no_duplicates`, `concurrent_ack_and_steal_no_double_execute`,
`split_brain_old_coord_self_fences`, `spot_drain_completes_within_lead_time`) are
the load-bearing gate and run Docker-free on every commit:

```bash
cargo test -p rollout-coordinator
```
