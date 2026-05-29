---
phase: 6
slug: multi-node-distribution
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-05-29
---

# Phase 6 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Detailed witness→signal mapping in `06-RESEARCH.md` § Validation Architecture.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `cargo test` (workspace) |
| **Config file** | none — workspace Cargo.toml |
| **Quick run command** | `cargo test -p rollout-coordinator --tests` |
| **Full suite command** | `cargo test --workspace --tests` |
| **Estimated runtime** | ~60–120 seconds (Docker-free, embedded lease + mock backend) |

---

## Sampling Rate

- **After every task commit:** Run the quick command for the touched crate
- **After every plan wave:** Run `cargo test --workspace --tests`
- **Before `/gsd:verify-work`:** Full suite green + `cargo clippy --workspace --all-targets -- -D warnings`
- **Max feedback latency:** ~120 seconds

---

## Per-Task Verification Map

> Populated by the planner per task. Phase-level witnesses below are the load-bearing acceptance gates (every commit, in-process, Docker-free via the embedded lease).

| Witness | Requirement(s) | Test Type | Automated Command | Status |
|---------|----------------|-----------|-------------------|--------|
| `coord_restart_no_duplicates` | DIST-03 | integration (in-proc sim) | `cargo test -p rollout-coordinator coord_restart_no_duplicates` | ⬜ pending |
| `concurrent_ack_and_steal_no_double_execute` | DIST-02 | integration | `cargo test -p rollout-coordinator concurrent_ack_and_steal_no_double_execute` | ⬜ pending |
| `split_brain_old_coord_self_fences` | DIST-05 | integration (subprocess for abort) | `cargo test -p rollout-coordinator split_brain_old_coord_self_fences` | ⬜ pending |
| `spot_drain_completes_within_lead_time` | DIST-04 | integration | `cargo test -p rollout-coordinator spot_drain_completes_within_lead_time` | ⬜ pending |
| lease CAS acquire/renew/steal | DIST-01 | unit | `cargo test -p rollout-storage lease` | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `StorageLease` trait + embedded redb CAS impl test stub (DIST-01)
- [ ] `WorkItemRecord` CAS-on-state module (extracted/shared from rollout-runtime-batch) test stub (DIST-02)
- [ ] In-process 1-coord + N-worker simulation harness (shared test support) for the four witnesses
- [ ] Subprocess abort harness for `split_brain_old_coord_self_fences` SC4 (in-process abort kills the test runner — must fork)

*Existing infrastructure (cargo test, cas_bytes on both backends, CAS state machine, preemption_signal) covers the rest.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| `make smoke-3node-aws` / `-gcp` against real cloud | DIST-01..05 (SC1) | Needs real AWS/GCP creds + 4 hosts; not free-runner reproducible | Operator runs the smoke per `docs/book` cloud chapter; 1 coord + 3 workers report `done` within 30s |
| Real coordinator kill mid-run on cloud | DIST-03 (SC2 operator path) | Needs the live 3-node cloud topology | Kill coordinator process; confirm fresh coord recovers + run completes (the in-proc `coord_restart_no_duplicates` is the every-commit witness) |
| Real spot-preemption signal on a cloud worker | DIST-04 (SC3 operator path) | Needs real IMDS/MDS preemption event | Trigger mock preemption; confirm drain within 60s/15s (the in-proc `spot_drain_completes_within_lead_time` is the every-commit witness) |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 120s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
