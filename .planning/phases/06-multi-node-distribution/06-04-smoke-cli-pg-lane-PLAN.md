---
phase: 06-multi-node-distribution
plan: 04
type: execute
wave: 4
depends_on: ["06-01", "06-02", "06-03"]
files_modified:
  - scripts/smoke-3node.sh
  - Makefile
  - crates/rollout-cli/src/worker.rs
  - crates/rollout-storage/tests/postgres_lease.rs
  - .github/workflows/ci.yml
  - docs/book/src/distribution/multi-node.md
  - docs/book/src/SUMMARY.md
autonomous: false
requirements: [DIST-01, DIST-02, DIST-03, DIST-04, DIST-05]
must_haves:
  truths:
    - "make smoke-3node-aws and make smoke-3node-gcp boot 1 coordinator + 3 workers (mock backend, no GPU), exchange heartbeats, dequeue+steal work, and report done within 30s"
    - "The Postgres single-row lease passes the same lease CAS acquire/renew/steal semantics as the embedded lease, in the postgres-integration CI lane"
    - "The multi-node distribution model (lease, epoch, steal, restart, drain) is documented in the mdBook docs site"
  artifacts:
    - path: "scripts/smoke-3node.sh"
      provides: "1 coordinator + 3 workers smoke driver (provider-parameterized, mock backend)"
      contains: "3"
    - path: "crates/rollout-storage/tests/postgres_lease.rs"
      provides: "Postgres lease CAS witness (postgres-integration lane)"
      contains: "lease"
    - path: "docs/book/src/distribution/multi-node.md"
      provides: "mdBook chapter: lease/epoch/steal/restart/drain"
      contains: "coordinator"
  key_links:
    - from: "scripts/smoke-3node.sh"
      to: "rollout-coordinator + rollout-cli worker"
      via: "boot coord + 3 workers over mTLS, assert run reports done in 30s"
      pattern: "coordinator|worker"
    - from: "crates/rollout-storage/tests/postgres_lease.rs"
      to: "StorageLease over Postgres backend"
      via: "same CAS lease semantics on the PG backend"
      pattern: "try_acquire|cas_bytes"
---

<objective>
Land the operator-facing and CI-lane closeout for Phase 6: the
`make smoke-3node-aws`/`-gcp` 1-coordinator + 3-worker smoke (mock backend, no GPU),
the Postgres-lease CI witness (proving D-LEASE-01's dual-backed lease on the real PG
path), and the mdBook multi-node chapter. The `--test-fence` abort subcommand is
already landed in 06-01 (where `fence.rs` lives, making `fence_aborts_within_5s` an
every-commit witness from wave 1); this plan only consumes it via the smoke + the
abort harness. Includes a human-verify checkpoint for the live-cloud smoke
(the every-commit witnesses already cover the logic Docker-free).

Purpose: prove the assembled system end-to-end and document it; close all 5 SCs.
Output: smoke driver + Make targets, PG lease lane, docs, CI wiring.
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
@.planning/phases/06-multi-node-distribution/06-01-SUMMARY.md
@.planning/phases/06-multi-node-distribution/06-02-SUMMARY.md
@.planning/phases/06-multi-node-distribution/06-03-SUMMARY.md

<interfaces>
From scripts/smoke.sh (Phase-2 template — copy the boot/teardown harness):
```bash
# Boots 1 coordinator + 2 workers over auto-generated dev CA; captures worker ULIDs;
# asserts an event in coord NDJSON log within a deadline. data/smoke/{coord,w1,w2}.db + logs.
```
From crates/rollout-cli/src/worker.rs:
```rust
pub struct WorkerConfig { run_id, coordinator_addr, coordinator_domain, storage }
// `rollout worker run --config <toml> [--worker-id <ulid>]`
```
From crates/rollout-coordinator/src/main.rs (the `--test-fence` subcommand already exists from 06-01):
```rust
enum Sub { Run { config: PathBuf }, TestFence { stale: u64, observed: u64 } }  // hidden; calls fence then std::process::abort (landed in 06-01 Task 3)
```
From crates/rollout-storage (Postgres backend, feature "postgres"; .sqlx/ offline cache exists):
```rust
// PostgresStorage implements cas_bytes via SELECT ... FOR UPDATE value-compare.
// StorageLease over Arc<dyn Storage> works unchanged on this backend (06-01).
```
CI postgres-integration lane (existing, ci.yml:258): `cargo test -p rollout-storage --features postgres --test postgres_integration -- --include-ignored --test-threads=1`.
</interfaces>
</context>

<tasks>

<task type="auto">
  <name>Task 1: 3-node smoke driver + Make targets (consumes the 06-01 --test-fence edge)</name>
  <read_first>
    - scripts/smoke.sh (Phase-2 boot/teardown harness — copy + extend to 3 workers + work dispatch)
    - Makefile (the `smoke:` target + .PHONY line + help block — add smoke-3node-aws/-gcp)
    - crates/rollout-cli/src/worker.rs (worker run loop — wire mock backend + steal-on-empty + drain poll)
    - crates/rollout-coordinator/src/main.rs (READ-ONLY: confirm the hidden `--test-fence` subcommand from 06-01 exists; do NOT re-add it)
    - crates/rollout-coordinator/src/drain.rs + steal.rs + run.rs (the assembled pieces the smoke exercises)
    - .planning/phases/06-multi-node-distribution/06-RESEARCH.md §"Validation Architecture" Phase gate + 06-VALIDATION.md Manual-Only Verifications
  </read_first>
  <action>
    Create `scripts/smoke-3node.sh` (provider-parameterized: `$1` = aws|gcp), copying the boot/teardown harness from `scripts/smoke.sh`:
    - Boot 1 coordinator (`rollout-coordinator run`) + 3 workers (`rollout worker run --worker-id <ulid>`) over an auto-generated dev CA, each with the `test-mock-backend` feature (no GPU, no vllm).
    - Seed N work items; assert the coordinator NDJSON log shows the run reaching `done` within 30s (ROADMAP SC1); assert at least one steal occurred (idle worker stole from a busy peer — grep a `steal` event).
    - For aws/gcp variants: gate the LIVE-cloud transport behind `ROLLOUT_SMOKE_CLOUD=1` (default = local mTLS transport so the script is runnable Docker-free for the wiring check); the real-cloud run is the operator/checkpoint path.
    The `--test-fence` subcommand is ALREADY landed in 06-01 Task 3 (in `crates/rollout-coordinator/src/main.rs`, calling `fence::fence_old_coordinator` then `std::process::abort()`); this plan does NOT add it — `tests/support/abort_harness.rs` already finds it. The smoke may invoke it (e.g. as a fault-injection step) but must not redefine it.
    Add Make targets `smoke-3node-aws` / `smoke-3node-gcp` (each `bash scripts/smoke-3node.sh {aws,gcp}`), update `.PHONY` + the `help:` block.
    Wire the worker run loop in `crates/rollout-cli/src/worker.rs` to: pull work, steal-on-empty (call coordinator steal RPC), and poll `preemption_signal` → `drain` (the 06-02/06-03 pieces).
  </action>
  <verify>
    <automated>test -x scripts/smoke-3node.sh && grep -q "smoke-3node-aws" Makefile && grep -q "smoke-3node-gcp" Makefile && grep -q "test-fence" crates/rollout-coordinator/src/main.rs</automated>
  </verify>
  <acceptance_criteria>
    - `test -x scripts/smoke-3node.sh` AND `grep -c "worker run" scripts/smoke-3node.sh` shows 3 worker boots (or a loop seeding 3).
    - `grep -q "smoke-3node-aws" Makefile && grep -q "smoke-3node-gcp" Makefile` (both targets + in .PHONY).
    - `grep -q "test-fence" crates/rollout-coordinator/src/main.rs` (the abort subcommand exists — landed in 06-01; this plan only consumes it, does not add it).
    - OPERATOR / MANUAL (per 06-VALIDATION.md Manual-Only; NOT an every-commit gate): `make smoke-3node-aws` then `make smoke-3node-gcp` each exit 0 — local-transport wiring run; reports done within 30s and observes a steal event. Run at the Task-4 checkpoint.
    - `cargo test -p rollout-coordinator fence_aborts_within_5s` still exits 0 (the abort harness uses the 06-01 `--test-fence` subcommand — regression check, not introduced here).
    - DOCS-02: same commit touches docs (chapter in Task 3 or a script header doc) or the worker.rs rustdoc + the smoke is a test.
  </acceptance_criteria>
  <done>make smoke-3node-{aws,gcp} boot 1 coord + 3 workers (mock backend), dequeue+steal work, report done within 30s on the local-transport wiring path; the script + Make targets + worker loop are wired, and the smoke consumes the 06-01 --test-fence edge without redefining it.</done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: Postgres-lease CI witness (D-LEASE-01 dual-backed lease on the PG path)</name>
  <read_first>
    - crates/rollout-storage/tests/postgres_integration.rs (existing PG-gated test style: #[ignore], --include-ignored, single-threaded)
    - crates/rollout-coordinator/src/lease.rs (StorageLease — the impl under test; works over any Arc<dyn Storage>)
    - crates/rollout-storage/src/postgres/ (PostgresStorage cas_bytes via SELECT ... FOR UPDATE)
    - .github/workflows/ci.yml lines ~258-286 (postgres-integration lane — add the new test invocation)
    - Makefile postgres-test target
    - .planning/phases/06-multi-node-distribution/06-RESEARCH.md §"DIST-03 Spike" §3 (PG DDL) + 06-CONTEXT.md D-LEASE-02 (PG lease in the postgres-integration lane)
  </read_first>
  <behavior>
    - Test `pg_lease_single_winner` (#[ignore], PG-gated): two `StorageLease::try_acquire` over a PostgresStorage against the same run — exactly one Some, one None (SC1 on the PG backend).
    - Test `pg_lease_steal_advances_epoch` (#[ignore]): acquire epoch 0 over PG, expire TTL, steal → epoch 1.
    - Test `pg_lease_renew_after_steal_fails` (#[ignore]): old holder's renew returns false after a PG-backed steal.
  </behavior>
  <action>
    Create `crates/rollout-storage/tests/postgres_lease.rs` (or co-locate in postgres_integration.rs if the harness requires it). Mark all tests `#[ignore]` + `#[tokio::test]`, run via `--include-ignored --test-threads=1` (matching the existing PG lane). Construct `StorageLease` over a `PostgresStorage` (DATABASE_URL from env, like postgres_integration.rs) and assert the same acquire/renew/steal semantics the embedded lease proves in 06-01 — confirming D-LEASE-01's "two impls for free over one StorageLease" claim.
    Note: rollout-storage cannot depend on rollout-coordinator (where StorageLease lives) without a dep edge. Resolve by either (a) testing the lease CAS semantics directly via `cas_bytes` on the PG backend in this crate (proving the primitive), and adding a `pg_lease` test in `crates/rollout-coordinator/tests/` gated on `DATABASE_URL` for the StorageLease wrapper; OR (b) keeping the StorageLease test in rollout-coordinator's test tree (which may dev-depend on rollout-storage features). Pick the option that respects the dep-direction lint — document the choice. Update `.github/workflows/ci.yml` postgres-integration lane + the `postgres-test` Makefile target to run the new test.
  </action>
  <verify>
    <automated>cargo test -p rollout-storage --features postgres --test postgres_lease -- --include-ignored --test-threads=1 2>&1 | grep -qE "test result: ok|0 passed; 0 failed; .* ignored" || cargo build -p rollout-storage --features postgres --tests</automated>
  </verify>
  <acceptance_criteria>
    - `test -f crates/rollout-storage/tests/postgres_lease.rs` (or the PG lease tests added to postgres_integration.rs — grep `pg_lease`).
    - `grep -q "pg_lease" crates/rollout-storage/tests/*.rs crates/rollout-coordinator/tests/*.rs` succeeds in at least one.
    - `grep -q "postgres_lease\|pg_lease" .github/workflows/ci.yml` (CI lane runs it).
    - `cargo build -p rollout-storage --features postgres --tests` exits 0 (compiles; live run is the PG-gated lane).
    - `cargo test -p rollout-core --test dependency_direction` green (no illegal dep edge introduced by the test placement choice).
    - DOCS-02: same commit ships the test (test-only commit is compliant).
  </acceptance_criteria>
  <done>Postgres single-row lease proves the same single-winner / monotonic-epoch / renew-after-steal-fails semantics as the embedded lease, wired into the postgres-integration CI lane (D-LEASE-01/02).</done>
</task>

<task type="auto">
  <name>Task 3: mdBook multi-node distribution chapter</name>
  <read_first>
    - docs/book/src/SUMMARY.md (add the new chapter under a Distribution heading)
    - docs/book/src/substrate/plugin-host.md (chapter style/format to match)
    - docs/specs/05-distribution.md (the spec the chapter narrates for users)
    - .planning/phases/06-multi-node-distribution/06-RESEARCH.md (the full design to document)
    - .planning/phases/06-multi-node-distribution/06-CONTEXT.md (locked decisions to reflect)
  </read_first>
  <action>
    Create `docs/book/src/distribution/multi-node.md` covering, for operators: the coordinator/worker model; the single-row CAS lease + monotonic epoch (one coordinator per run); work-stealing (idle steals ceil(backlog/2) from busiest peer, coordinator-mediated, MAX_STEAL_BATCH=32); coordinator restart (stateless-replayer, restart invisible to progress); spot-drain (notice lead 120/30 vs drain deadline 60/15, TrainState-only opportunistic snapshot); split-brain fencing (coord_epoch validation, self-fence + coordinator_fenced event + abort within 5s). Include the `make smoke-3node-aws`/`-gcp` operator recipe and the dep-direction note (coord ↛ cloud — preemption via the ComputeHint trait). Add the chapter to `docs/book/src/SUMMARY.md` under a "Distribution" heading.
  </action>
  <verify>
    <automated>test -f docs/book/src/distribution/multi-node.md && grep -q "multi-node" docs/book/src/SUMMARY.md && (cd docs/book 2>/dev/null && mdbook build 2>/dev/null; true)</automated>
  </verify>
  <acceptance_criteria>
    - `test -f docs/book/src/distribution/multi-node.md` AND `grep -qi "coordinator_fenced\|work-stealing\|stateless-replayer" docs/book/src/distribution/multi-node.md`.
    - `grep -q "multi-node" docs/book/src/SUMMARY.md` (linked in the ToC).
    - `grep -q "smoke-3node" docs/book/src/distribution/multi-node.md` (operator recipe documented).
    - `make docs` (mdbook build) succeeds with the new chapter (no broken links — DOCS-01/03).
    - DOCS-02: docs-only commit is compliant.
  </acceptance_criteria>
  <done>mdBook ships a multi-node distribution chapter documenting lease/epoch/steal/restart/drain/fencing + the 3-node operator smoke; the docs site builds clean.</done>
</task>

<task type="checkpoint:human-verify" gate="blocking">
  <name>Task 4 (checkpoint): Operator verification of the assembled multi-node runtime</name>
  <action>Pause for the operator to run the witness suite + local 3-node smoke (and optionally the live-cloud smoke + PG lease lane) and confirm the assembled runtime behaves per the success criteria. See how-to-verify for exact commands.</action>
  <what-built>
    The complete Phase-6 multi-node runtime: lease/epoch fencing, work-stealing, coordinator
    restart-from-storage, spot-drain. All four named witnesses (coord_restart_no_duplicates,
    concurrent_ack_and_steal_no_double_execute, split_brain_old_coord_self_fences,
    spot_drain_completes_within_lead_time) run Docker-free on every commit; the
    make smoke-3node-{aws,gcp} targets run the 1-coord + 3-worker topology.
  </what-built>
  <how-to-verify>
    1. Docker-free witnesses (every-commit, no creds): `cargo test -p rollout-coordinator` — confirm all four named witnesses pass.
    2. Local-transport smoke wiring: `make smoke-3node-aws` then `make smoke-3node-gcp` — confirm each reports the run done within 30s and shows a steal event.
    3. LIVE cloud (operator-only, requires real AWS/GCP creds + ~4 hosts, per 06-VALIDATION.md Manual-Only): set `ROLLOUT_SMOKE_CLOUD=1` + cloud creds, run `make smoke-3node-aws`; kill the coordinator process mid-run and confirm a fresh coordinator recovers and the run completes with zero duplicate sample IDs; trigger a mock spot-preemption on a worker and confirm graceful drain within 60s.
    4. (Optional, if PG available) `make postgres-test` — confirm the PG lease lane passes.
  </how-to-verify>
  <resume-signal>Type "approved" if the witnesses + local smoke pass (live-cloud is operator-optional), or describe issues.</resume-signal>
  <verify>Operator confirms `cargo test -p rollout-coordinator` (4 witnesses) + `make smoke-3node-aws`/`-gcp` pass; live-cloud optional.</verify>
  <done>Operator approves the assembled multi-node runtime; every-commit witnesses are the load-bearing gate, live-cloud smoke is operator-optional.</done>
</task>

</tasks>

<verification>
- `cargo test -p rollout-coordinator` green — all four named witnesses pass.
- Structural wiring check (every-commit): `test -x scripts/smoke-3node.sh && grep -q "smoke-3node-aws" Makefile && grep -q "test-fence" crates/rollout-coordinator/src/main.rs`.
- `make smoke-3node-aws` / `-gcp` exit 0 — operator/manual step per 06-VALIDATION.md (run at the Task-4 checkpoint, not on every commit).
- `cargo build -p rollout-storage --features postgres --tests` compiles the PG lease lane.
- `make docs` builds the new chapter clean (DOCS-01/03).
- `cargo test --workspace --tests` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test -p rollout-core --test dependency_direction` all green.
</verification>

<success_criteria>
- SC1 (DIST-01..05): make smoke-3node-aws/-gcp boot 1 coord + 3 workers, mock backend, no GPU, report done within 30s (operator/manual run at the checkpoint).
- D-LEASE-01/02: Postgres lease proven in the postgres-integration lane; embedded lease is the every-commit witness.
- SC4 abort: the `--test-fence` subcommand (landed in 06-01) backs the subprocess abort-within-5s witness; this plan only consumes it.
- mdBook multi-node chapter documents the full design; docs site builds clean.
- Operator-verify checkpoint confirms the assembled system (live-cloud optional, every-commit witnesses are the load-bearing gate).
</success_criteria>

<output>
After completion, create `.planning/phases/06-multi-node-distribution/06-04-SUMMARY.md`
</output>
</content>
</invoke>
