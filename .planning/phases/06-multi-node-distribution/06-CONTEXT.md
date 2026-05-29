# Phase 6: Multi-node distribution - Context

**Gathered:** 2026-05-29
**Status:** Ready for planning

<domain>
## Phase Boundary

Turn the in-tree single-host control plane (Phase 2 shipped register / deregister / heartbeat into Storage + deadline-based failure scan) into a **real multi-host distributed runtime**: a work-stealing pull queue, coordinator-restart-from-storage, spot-preemption graceful drain, and split-brain fencing. Scope = DIST-01..05.

In scope:
- DIST-01 — Coordinator with persistent state in `Storage` (namespaces `work`, `epoch`, `queue_items`); one coordinator per run; lease-based exclusion.
- DIST-02 — Work-stealing pull queue; CAS-on-state dedup.
- DIST-03 — Coordinator-restart-from-storage (bespoke storage-backed stateless-replayer, no Raft/etcd).
- DIST-04 — Spot-preemption signal handler → graceful drain.
- DIST-05 — Split-brain prevention via epoch fencing.

Out of scope (explicit): process snapshots (SNAPSHOT-01, v1.2+ — DIST-04 falls back to TrainState only); RL training; any new algorithm. New capabilities belong to other phases.
</domain>

<decisions>
## Implementation Decisions

### Split-brain self-fence (DIST-05)
- **D-FENCE-01:** When the old coordinator detects its lease was stolen (it lost the epoch race), it MUST **stop all shared-state I/O immediately** — a loser must never mutate shared state after losing the lease (the survivor now owns the epoch). Correct fencing semantics, not graceful drain.
- **D-FENCE-02:** Before aborting, emit exactly ONE `coordinator_fenced` observability event (tracing + `Event`), which performs **no shared-state write** (observability sink only). This keeps the fence diagnosable in post-mortems.
- **D-FENCE-03:** Then `std::process::abort()` within the 5s bound (per roadmap SC4). No best-effort flush of in-flight work by the loser (rejected — risks corrupting the survivor's epoch).
- **D-FENCE-04:** Workers validate `coord_epoch` on **every** RPC and reject responses tagged with a stale epoch. `worker_self_fence_timeout < coordinator_failure_timeout` (reuse Phase 2 deadline-based health: 4s self-fence < 5s coord-failure).

### Coordinator restart-gap behavior (DIST-03)
- **D-RESTART-01:** While the coordinator is dead/restarting, workers **keep executing their current lease and re-sync on reconnect** — restart is invisible to overall progress (roadmap SC2). Workers buffer acks during the gap and replay them when the fresh coordinator is back.
- **D-RESTART-02:** The fresh coordinator boots by **replaying the assignment ledger + fence epoch from Storage** (stateless-replayer pattern) — it reconstructs in-flight assignments rather than reassigning blindly.
- **D-RESTART-03:** Bounded safety — if the gap exceeds `coordinator_failure_timeout`, workers self-fence per D-FENCE-04 and stop. Workers continue only within their lease/coord-failure window.

### Work-stealing policy (DIST-02)
- **D-STEAL-01:** Trigger — a worker steals only when its **local queue drains to empty** (reactive, low churn).
- **D-STEAL-02:** Batch size — steal `ceil(victim_backlog / 2)`, capped at a max batch size.
- **D-STEAL-03:** Victim selection — steal from the **busiest peer by backlog**, coordinator-mediated.
- **D-STEAL-04:** Dedup — CAS-on-state collapses duplicate acks/steals so a stolen-then-reclaimed item never double-executes (roadmap SC5: `concurrent_ack_and_steal_no_double_execute`).
- These are fixed behaviors for v1.1 (not config knobs) — the "configurable" variant was considered and deferred to avoid premature surface area. Revisit if tuning need emerges.

### Lease backend + test strategy (DIST-01)
- **D-LEASE-01:** The single-row CAS lease is **abstracted behind a trait** with two impls: **embedded redb-backed lease** (default, for local dev / CI / `make smoke`) and **Postgres single-row lease** (real multi-host prod). Mirrors the Phase 4 dual-storage (embedded + Postgres) pattern.
- **D-LEASE-02:** The 3-node smoke + `coord_restart_no_duplicates` + `split_brain_old_coord_self_fences` run **Docker-free on every commit** via the embedded-lease path (in-process simulation / multi-process on one host). Postgres lease exercised in the existing `postgres-integration`-style CI lane. Consistent with the project's Docker-free-default testing convention.

### Spot-drain budgets (DIST-04)
- **D-SPOT-01:** Distinguish two numbers. **Preemption-notice lead time** (the real cloud signal) = **120s AWS / 30s GCP**. **Conservative drain deadline** (what graceful drain is designed to complete within, leaving margin before forced reclaim) = **60s AWS / 15s GCP**.
- **D-SPOT-02:** Drain sequence: stop-pull → finish-or-requeue in-flight via lease nack → opportunistic TrainState snapshot if budget allows → deregister cleanly → ack-exit. The `spot_drain_completes_within_lead_time` test asserts completion within the **60s/15s** deadline.
- **D-SPOT-03:** v1.1 opportunistic snapshot = TrainState only (process-snapshot path defers to SNAPSHOT-01, v1.2+).
- **D-SPOT-04:** Reconcile the docs — update REQ DIST-04 (currently "120s/30s") and the roadmap (currently "60s/15s") to state BOTH numbers with the notice-vs-deadline distinction.

### Claude's Discretion
- Exact lease-trait method shape, TTL/renewal cadence (within the Phase-2 deadline-based health bounds), ledger schema layout, and the `coordinator_lease` table DDL — to be designed in the DIST-03 architecture spike during planning.
- Internal RPC/proto additions for steal + epoch-tagged responses (within the existing 3-channel mTLS transport).
- Observability event taxonomy beyond `coordinator_fenced`.
</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase scope + requirements
- `.planning/ROADMAP.md` — Phase 6 goal, the mandatory DIST-03 architecture-spike note, and the 5 success criteria (incl. the named CI tests).
- `.planning/REQUIREMENTS.md` — DIST-01..05 acceptance criteria + traceability table.

### Distribution + storage specs
- `docs/specs/05-distribution.md` — the distribution spec (coordinator/worker model, lease, fencing, heartbeats). Primary spec for this phase.
- `docs/specs/04-storage-snapshots.md` — Storage namespaces + snapshot contract (DIST-01 `work`/`epoch`/`queue_items`; DIST-04 opportunistic TrainState snapshot).
- `docs/specs/09-observability.md` — Event/EventKind taxonomy (for the `coordinator_fenced` event, D-FENCE-02).

### Established patterns to reuse
- `crates/rollout-coordinator/src/{heartbeat,registry,failure_scan,run}.rs` — Phase-2 control-plane scaffolding to extend (NOT rewrite). `failure_scan.rs` already implements deadline-based health; DIST-05 fencing builds on it.
- `crates/rollout-transport/src/health.rs` (`is_failed`, `next_due_at`) + `TransportConfig::validate_cross_fields` — deadline-based health timing (500ms hb / 4s self-fence / 5s coord-failure / 250ms skew) and the `self_fence < coord_failure` split-brain invariant.
- `crates/rollout-core/src/traits/cloud.rs` — `Queue::dequeue_with_lease`/`extend_lease`, `LeaseToken`, `ComputeHint::preemption_signal` (Phase 5 surfaces this phase consumes).
- `crates/rollout-cloud-local/src/queue.rs` — `InMemQueue` + Storage spill (the embedded-lease impl should follow this storage-mirroring pattern).
- `crates/rollout-cloud-{aws,gcp}/src/imds|mds` — preemption-signal sources for DIST-04.
- `crates/rollout-runtime-batch` — CAS sample-state machine + the `restart_no_duplicates` witness pattern (template for `coord_restart_no_duplicates` + `concurrent_ack_and_steal_no_double_execute`).
- `crates/rollout-storage/src/postgres/` — Postgres lease impl home (single-row lease); embedded lease in `rollout-storage` / `rollout-cloud-local`.
</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **`rollout-coordinator` (Phase 2):** `CoordinatorImpl`, `registry` (HeartbeatRecord), `failure_scan_loop`, `run()` boot path, `CoordinatorConfig`. Extend with lease acquisition/renewal, epoch management, work ledger, steal mediation.
- **Deadline-based health (`rollout-transport::health`):** reuse timing + `is_failed`/`next_due_at`; DIST-05 fencing maps onto the existing `self_fence < coord_failure` invariant.
- **Queue lease surface (`rollout-core::traits::cloud`):** `dequeue_with_lease`/`extend_lease`/`LeaseToken` already exist (default impls); cloud impls landed in Phase 5.
- **CAS dedup pattern (`rollout-runtime-batch`):** the content-addressed state-machine + `restart_no_duplicates` test is the direct template for the two dedup CI witnesses.

### Established Patterns
- Dual storage (embedded redb default + Postgres feature-gated) — the lease follows the same dual-impl shape (D-LEASE-01).
- Docker-free-default tests; live/heavy paths gated to dedicated CI jobs (D-LEASE-02).
- Plan-time `validate_cross_fields` for timing invariants — extend for lease/epoch timing.

### Integration Points
- `rollout-cli` `coordinator run` subcommand delegates to `rollout_coordinator::run` — multi-node smoke entry (`make smoke-3node-aws`/`-gcp`) wires here.
- mTLS 3-channel transport — steal RPCs + epoch-tagged responses ride the existing Control/Work channels.
</code_context>

<specifics>
## Specific Ideas

- DIST-03 architecture spike (do FIRST in planning): write the `coordinator_lease` table schema + the `split_brain_old_coord_self_fences` test skeleton before committing to the PR plan (per ROADMAP).
- Named CI witnesses required (every commit, in-process/embedded): `coord_restart_no_duplicates`, `concurrent_ack_and_steal_no_double_execute`, `split_brain_old_coord_self_fences`, `spot_drain_completes_within_lead_time`.
- Operator smokes: `make smoke-3node-aws` and `-gcp` (1 coordinator + 3 workers, mock backend, no GPU, real cloud transport).
</specifics>

<deferred>
## Deferred Ideas

- **Configurable work-stealing knobs** (trigger threshold / max batch / victim strategy as config fields) — considered in D-STEAL; deferred to keep v1.1 surface minimal. Revisit if tuning need emerges.
- **Process snapshots on spot-drain** (CRIU-style) — SNAPSHOT-01, v1.2+. DIST-04 uses TrainState snapshot only in v1.1.
- **Raft/etcd-based coordinator consensus** — explicitly rejected by the roadmap (bespoke storage-backed replayer instead).

None of the above are in Phase 6 scope.
</deferred>

---

*Phase: 06-multi-node-distribution*
*Context gathered: 2026-05-29*
