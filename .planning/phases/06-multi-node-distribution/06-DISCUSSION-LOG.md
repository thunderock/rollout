# Phase 6: Multi-node distribution - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-29
**Phase:** 06-multi-node-distribution
**Areas discussed:** Self-fence mechanism, Restart-gap behavior, Work-stealing policy, Lease backend + smoke, Spot-drain budget

---

## Self-fence mechanism (DIST-05)

| Option | Description | Selected |
|--------|-------------|----------|
| Stop-IO + final event, then abort | Cease shared-state writes immediately, emit one `coordinator_fenced` event (no state write), then `std::process::abort` within 5s | ✓ |
| Immediate std::process::abort | Bare abort, no final event; rely on epoch rejection + CAS dedup | |
| Graceful drain, then abort | Loser finishes/flushes before abort — risks corrupting survivor's epoch | |

**User's choice:** Stop-IO + final event, then abort
**Notes:** Aligns with correct fencing theory — the loser must not mutate shared state after losing the lease.

---

## Restart-gap behavior (DIST-03)

| Option | Description | Selected |
|--------|-------------|----------|
| Keep running lease, re-sync on reconnect | Workers execute through the gap, buffer acks, coordinator replays ledger from storage on boot; self-fence if gap exceeds coord_failure_timeout | ✓ |
| Park until new coordinator recovers | Workers stop pulling and heartbeat-park until coordinator is back | |

**User's choice:** Keep running lease, re-sync on reconnect
**Notes:** Restart invisible to overall progress (roadmap SC2), with bounded safety via self-fence.

---

## Work-stealing policy (DIST-02)

| Option | Description | Selected |
|--------|-------------|----------|
| Empty-queue / half-of-victim / busiest | Steal when local queue empty; `ceil(victim_backlog/2)` capped; victim = busiest peer | ✓ |
| Below-watermark / fixed-N / any peer | Proactive low-watermark trigger; fixed batch; any victim | |
| Recommended defaults, but configurable | Ship recommended behavior as defaults with config knobs | |

**User's choice:** Empty-queue / half-of-victim / busiest
**Notes:** Configurable variant deferred to keep v1.1 surface minimal.

---

## Lease backend + smoke (DIST-01)

| Option | Description | Selected |
|--------|-------------|----------|
| Dual: embedded-lease for CI, Postgres for prod | Lease behind a trait; redb CAS lease for local/CI (Docker-free), Postgres single-row lease for prod | ✓ |
| Postgres-only lease | Implement only on Postgres; multi-node smoke + restart/split-brain tests require Docker | |

**User's choice:** Dual: embedded-lease for CI, Postgres for prod
**Notes:** Keeps the three named CI witnesses Docker-free on every commit; consistent with dual-storage convention.

---

## Spot-drain budget (DIST-04)

| Option | Description | Selected |
|--------|-------------|----------|
| Notice 120/30, drain deadline 60/15 | Real preemption notice = 120s AWS / 30s GCP; conservative drain deadline = 60s / 15s | ✓ |
| Use 60s / 15s as the only budget | Single hard budget; update REQ DIST-04 to match | |
| Use 120s / 30s as the only budget | Full provider notice as budget; no safety margin | |

**User's choice:** Notice 120/30, drain deadline 60/15
**Notes:** Resolves the REQ-vs-roadmap conflict; docs to be reconciled to state both numbers with the notice-vs-deadline distinction.

---

## Claude's Discretion

- Lease-trait method shape, TTL/renewal cadence, ledger schema, `coordinator_lease` DDL (DIST-03 spike during planning).
- Internal steal/epoch RPC additions within the existing 3-channel mTLS transport.
- Observability event taxonomy beyond `coordinator_fenced`.

## Deferred Ideas

- Configurable work-stealing knobs (deferred — minimal v1.1 surface).
- Process snapshots on spot-drain (SNAPSHOT-01, v1.2+).
- Raft/etcd coordinator consensus (rejected by roadmap — bespoke replayer instead).
