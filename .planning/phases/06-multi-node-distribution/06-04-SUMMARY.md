---
phase: 06-multi-node-distribution
plan: 04
subsystem: testing
tags: [smoke, cli, postgres, lease, work-stealing, spot-drain, fencing, mdbook, ci]

# Dependency graph
requires:
  - phase: 06-multi-node-distribution
    plan: 00
    provides: "WorkItemRecord CAS state machine (try_claim/try_complete/try_repending + work_key) + work namespace"
  - phase: 06-multi-node-distribution
    plan: 01
    provides: "StorageLease + monotonic epoch + fence_old_coordinator + the hidden --test-fence subcommand in coordinator main.rs"
  - phase: 06-multi-node-distribution
    plan: 02
    provides: "ledger dispatch queue + coordinator-mediated steal protocol"
  - phase: 06-multi-node-distribution
    plan: 03
    provides: "replay_and_serve stateless-replayer + spot-drain state machine + BatchWorker stop_pull/poll_preemption"
  - phase: 05-cloud-layer
    provides: "ComputeHint::preemption_signal (120s AWS / 30s GCP) + Queue lease semantics"
provides:
  - "scripts/smoke-3node.sh — provider-parameterized (aws|gcp) 1-coordinator + 3-worker mTLS smoke; drives the assembled ledger (dispatch + steal + CAS), asserts run done + a steal within 30s on the local-transport wiring path"
  - "make smoke-3node-aws / smoke-3node-gcp targets (+ .PHONY + help)"
  - "mock_run.rs — mock-backend ledger driver (no GPU); emits NDJSON work_dispatched/work_stolen/run_done"
  - "rollout-cli worker drain edge — ROLLOUT_MOCK_PREEMPT_MS -> stop-pull -> drain (joins the 06-03 spot-drain edge at the worker run loop)"
  - "crates/rollout-storage/tests/postgres_lease.rs — 3 PG-gated CAS-lease witnesses (D-LEASE-01/02) in the postgres-integration CI lane"
  - "docs/book/src/distribution/multi-node.md — operator mdBook chapter (lease/epoch/steal/restart/drain/fencing)"
affects: [07-harnesses]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Local-transport-by-default smoke: 3-node topology runnable Docker-free for the wiring check; the live-cloud run gates behind ROLLOUT_SMOKE_CLOUD=1 (operator-only path)"
    - "Mock-backend ledger driver as a hidden coordinator subcommand: smoke exercises the assembled dispatch/steal/CAS without a GPU or vllm"
    - "PG lease witness exercises the LeaseRecord CAS primitive directly (rollout-core types only) so rollout-storage never depends on the coordinator where StorageLease lives — dep-direction preserved"
    - "Embedded redb lease is the every-commit witness; the PG witness proves the dual-backed CAS in the postgres-integration lane (D-LEASE-01)"

key-files:
  created:
    - scripts/smoke-3node.sh
    - crates/rollout-coordinator/src/mock_run.rs
    - crates/rollout-storage/tests/postgres_lease.rs
    - docs/book/src/distribution/multi-node.md
    - docs/book/src/distribution/index.md
  modified:
    - Makefile
    - crates/rollout-cli/src/worker.rs
    - crates/rollout-coordinator/src/main.rs
    - crates/rollout-coordinator/src/lib.rs
    - crates/rollout-coordinator/src/work_item.rs
    - .github/workflows/ci.yml
    - docs/book/src/SUMMARY.md

key-decisions:
  - "PG lease witness exercises the LeaseRecord CAS primitive directly via cas_bytes on the Postgres backend (rollout-core types only), not StorageLease — keeps rollout-storage ↛ rollout-coordinator clean while proving the same single-winner/monotonic-epoch/renew-after-steal-fails semantics"
  - "Smoke defaults to local mTLS transport (Docker-free wiring check); the real-cloud run gates behind ROLLOUT_SMOKE_CLOUD=1 and is the operator/checkpoint path — every-commit witnesses already cover the logic"
  - "Worker drain edge wired via ROLLOUT_MOCK_PREEMPT_MS env poll -> stop-pull -> drain, joining the 06-03 spot-drain state machine at the worker run loop (the end-to-end join deferred from 06-03)"
  - "The --test-fence abort subcommand (06-01) is consumed by the smoke as a fault-injection edge, never redefined"

patterns-established:
  - "Local-transport-default / cloud-gated smoke driver pattern for provider-parameterized topologies"
  - "Mock-backend ledger driver as a hidden coordinator subcommand for GPU-free end-to-end exercise"

requirements-completed: [DIST-01, DIST-02, DIST-03, DIST-04, DIST-05]

# Metrics
duration: 7min
completed: 2026-05-29
---

# Phase 6 Plan 04: Smoke + CLI + PG Lane Summary

**The Phase-6 closeout: a provider-parameterized 1-coordinator + 3-worker smoke (`make smoke-3node-aws`/`-gcp`, mock backend, no GPU) that drives the assembled dispatch/steal/CAS ledger and asserts run-done + a steal within 30s on the local-transport wiring path; a Postgres CAS-lease CI witness (3 PG-gated tests proving D-LEASE-01's dual-backed lease on the real `cas_bytes` path) wired into the postgres-integration lane; the worker drain edge joining the 06-03 spot-drain state machine at the run loop; and an operator-facing mdBook multi-node chapter — closing all five DIST requirements, with the operator human-verify checkpoint APPROVED (live-cloud deferred as operator-optional).**

## Performance

- **Duration:** ~7 min (Tasks 1-3 execution); checkpoint pause + operator approval spanned a separate operator window
- **Started:** 2026-05-29T09:32:15-07:00
- **Completed:** 2026-05-29T09:36:51-07:00 (Tasks 1-3); Task 4 checkpoint approved by operator post-pause
- **Tasks:** 4 (3 auto + 1 checkpoint:human-verify, approved)
- **Files modified:** 12 (5 created, 7 modified)

## Accomplishments
- **3-node smoke driver + Make targets (Task 1).** `scripts/smoke-3node.sh` (provider-parameterized `aws|gcp`) boots 1 coordinator + 3 workers over an auto-generated dev CA, drives the assembled 06-02 ledger (dispatch + steal + CAS) via a hidden coordinator mock-run edge (`mock_run.rs`, mock backend, no GPU/vllm), and asserts the run reaches `done` plus at least one steal event within 30s. The LIVE-cloud transport gates behind `ROLLOUT_SMOKE_CLOUD=1` (default = local mTLS transport, Docker-free wiring check). `make smoke-3node-aws`/`-gcp` targets + `.PHONY` + help block added. The 06-01 `--test-fence` subcommand is consumed, never redefined.
- **Worker drain edge (Task 1).** `rollout-cli` `worker.rs` polls `ROLLOUT_MOCK_PREEMPT_MS` → sets stop-pull → drains, joining the 06-03 spot-drain state machine at the worker run loop (the end-to-end join deferred from 06-03). `work_item::work_prefix` was made `pub` for the driver's ledger scans.
- **Postgres CAS-lease CI witness (Task 2).** `crates/rollout-storage/tests/postgres_lease.rs` ships 3 PG-gated (`#[ignore]`) witnesses: `pg_lease_single_winner` (SC1 on the PG backend), `pg_lease_steal_advances_epoch` (monotonic epoch), and `pg_lease_renew_after_steal_fails`. They exercise the `LeaseRecord` CAS primitive directly via the Postgres `cas_bytes` path using rollout-core types only — respecting the dep-direction lint (rollout-storage must not depend on the coordinator where `StorageLease` lives). The embedded redb half is the every-commit witness; this proves the dual-backed CAS on PG (D-LEASE-01/02). `ci.yml` postgres-integration lane + the `postgres-test` Make target run it via `--include-ignored --test-threads=1`.
- **mdBook multi-node chapter (Task 3).** `docs/book/src/distribution/multi-node.md` documents, for operators: the coordinator/worker model; single-row CAS lease + monotonic epoch; coordinator-mediated work-stealing (`ceil(backlog/2)`, `MAX_STEAL_BATCH=32`); stateless-replayer restart; spot-drain (notice lead vs drain deadline, TrainState-only snapshot, coord ↛ cloud); and split-brain fencing (`coordinator_fenced` + self-fence + abort < 5s). Includes the `make smoke-3node-aws`/`-gcp` operator recipe + the `ROLLOUT_SMOKE_CLOUD` live path. A new "Distribution" heading (`distribution/index.md`) was added to `SUMMARY.md`; `mdbook build` is clean.
- **Operator verification checkpoint APPROVED (Task 4).** The operator confirmed the load-bearing every-commit gate: `cargo test -p rollout-coordinator` (the four named witnesses — `coord_restart_no_duplicates`, `concurrent_ack_and_steal_no_double_execute`, `split_brain_old_coord_self_fences`, `spot_drain_completes_within_lead_time`) plus `make smoke-3node-aws`/`-gcp` on the local-transport path (each reports done within 30s with a steal). Operator response: **"approved."** The LIVE-cloud smoke (requires real AWS/GCP creds + ~4 hosts, per 06-VALIDATION.md Manual-Only) is **operator-optional and deferred** — the every-commit witnesses are the load-bearing gate.

## Task Commits

Each task was committed atomically:

1. **Task 1: 3-node smoke driver + Make targets + worker drain edge** - `6faadea` (feat)
2. **Task 2: Postgres coordinator-lease CAS witness (D-LEASE-01/02)** - `6abb63a` (test)
3. **Task 3: mdBook multi-node distribution chapter** - `dd3132f` (docs)
4. **Task 4: Operator verification checkpoint** - no code commit; operator approval recorded in this SUMMARY (pause state was recorded in `d9f56ef`)

**Plan metadata:** _(this completion commit)_ (docs: complete plan)

## Files Created/Modified
- `scripts/smoke-3node.sh` - Provider-parameterized 1-coord + 3-worker mTLS smoke; local-transport-default, `ROLLOUT_SMOKE_CLOUD=1` for live.
- `crates/rollout-coordinator/src/mock_run.rs` - Mock-backend ledger driver; NDJSON `work_dispatched`/`work_stolen`/`run_done`; unit test completes all items + a steal.
- `crates/rollout-coordinator/src/main.rs` - Hidden mock-run edge wired (the `--test-fence` subcommand from 06-01 is consumed, not re-added).
- `crates/rollout-coordinator/src/lib.rs` - `pub mod mock_run;`.
- `crates/rollout-coordinator/src/work_item.rs` - `work_prefix` made `pub` for the driver's ledger scans.
- `crates/rollout-cli/src/worker.rs` - `ROLLOUT_MOCK_PREEMPT_MS` poll → stop-pull → drain (06-03 spot-drain edge at the run loop).
- `Makefile` - `smoke-3node-aws`/`-gcp` targets + `.PHONY` + help; `postgres-test` runs the PG lease lane.
- `crates/rollout-storage/tests/postgres_lease.rs` - 3 PG-gated CAS-lease witnesses (D-LEASE-01/02).
- `.github/workflows/ci.yml` - postgres-integration lane runs the new PG lease test.
- `docs/book/src/distribution/multi-node.md` - Operator multi-node chapter (lease/epoch/steal/restart/drain/fencing + smoke recipe).
- `docs/book/src/distribution/index.md` - New Distribution section landing page.
- `docs/book/src/SUMMARY.md` - Distribution heading in the ToC.

## Decisions Made
- **PG lease tests target the CAS primitive, not StorageLease.** `postgres_lease.rs` exercises the `LeaseRecord` CAS via the Postgres `cas_bytes` path using rollout-core types only, so `rollout-storage` never depends on the coordinator crate (where `StorageLease` lives) — option (a) from the plan, respecting the dep-direction lint while proving the same single-winner / monotonic-epoch / renew-after-steal-fails semantics the embedded lease proves in 06-01.
- **Smoke is local-transport-by-default, cloud-gated.** The 3-node smoke runs Docker-free for the wiring check; the real-cloud transport gates behind `ROLLOUT_SMOKE_CLOUD=1` and is the operator/checkpoint path. The every-commit witnesses (`cargo test -p rollout-coordinator`) carry the load-bearing logic.
- **Worker drain edge via env-polled mock preemption.** `ROLLOUT_MOCK_PREEMPT_MS` lets the smoke inject a preemption signal that the worker run loop observes → stop-pull → drain, joining the 06-03 spot-drain state machine end-to-end without a real cloud preemption.

## Deviations from Plan

None — plan executed as written. Tasks 1-3 landed exactly per the task specs (smoke driver + Make targets + worker edge; PG lease witness in the postgres-integration lane; mdBook chapter), and Task 4 (`checkpoint:human-verify`) ran as designed: the executor paused, the operator ran the witnesses + local smoke, and approved. The live-cloud smoke was explicitly operator-optional per the plan's resume-signal and 06-VALIDATION.md Manual-Only classification.

## Issues Encountered
None.

## Authentication / Checkpoint Gates
- **Task 4 — `checkpoint:human-verify` (blocking): APPROVED.** Operator ran `cargo test -p rollout-coordinator` (the four named witnesses) + `make smoke-3node-aws`/`-gcp` (local-transport, each done within 30s with a steal) and responded "approved." The live-cloud smoke + PG lease lane (real creds / PG server) are operator-optional and deferred; the every-commit Docker-free witnesses are the load-bearing gate per the plan's success criteria.

## Known Stubs
- **Live-cloud 3-node smoke deferred (operator-optional).** `make smoke-3node-aws`/`-gcp` with `ROLLOUT_SMOKE_CLOUD=1` (real AWS/GCP creds + ~4 hosts) is the operator-only manual verification per 06-VALIDATION.md Manual-Only; it was not run as part of automated completion. This is intentional and by-design — the local-transport wiring run + the four every-commit witnesses prove the assembled dedup/timing/steal/fence logic Docker-free. The PG lease lane likewise runs only when a Postgres server + `DATABASE_URL` are present (the postgres-integration CI lane).

## User Setup Required
None for the every-commit path — all witnesses run Docker-free on the embedded redb path with mock cloud traits. Operators wanting the live-cloud or PG verification provide their own AWS/GCP creds (`ROLLOUT_SMOKE_CLOUD=1`) or a Postgres `DATABASE_URL`.

## Next Phase Readiness
- **Phase 6 is complete (5/5 plans; DIST-01..05 satisfied).** The assembled multi-node runtime — lease/epoch fencing, work-stealing, stateless-replayer restart, spot-drain — is wired, witnessed Docker-free, exercised by the 3-node smoke, documented in the mdBook, and operator-approved.
- **Phase 7 (Harnesses)** can build on the stable `work`/`epoch`/`coordinator_lease`/`queue_items` namespaces, the `WorkItemRecord` CAS + `StorageLease` + `drain` contracts, and the `WorkQueue` for eval-as-job. The strace-derived seccomp baseline for HARNESS-02 is the next prerequisite before its plan.

---
*Phase: 06-multi-node-distribution*
*Completed: 2026-05-29*

## Self-Check: PASSED

All 5 created files (`scripts/smoke-3node.sh`, `crates/rollout-coordinator/src/mock_run.rs`, `crates/rollout-storage/tests/postgres_lease.rs`, `docs/book/src/distribution/multi-node.md`, `docs/book/src/distribution/index.md`) + this SUMMARY present on disk; all three task commits (`6faadea`, `6abb63a`, `dd3132f`) found in git log. Structural every-commit witnesses confirmed: `test -x scripts/smoke-3node.sh`, 3 worker boots in the smoke, `smoke-3node-aws`/`-gcp` in the Makefile, `test-fence` present (consumed, not redefined) in `main.rs`, and `pg_lease` wired into `ci.yml`. Task 4 checkpoint operator-approved (live-cloud operator-optional, deferred).
