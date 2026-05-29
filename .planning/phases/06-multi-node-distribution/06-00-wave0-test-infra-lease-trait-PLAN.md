---
phase: 06-multi-node-distribution
plan: 00
type: execute
wave: 0
depends_on: []
files_modified:
  - crates/rollout-core/src/traits/lease.rs
  - crates/rollout-core/src/traits/mod.rs
  - crates/rollout-core/src/lib.rs
  - crates/rollout-coordinator/src/lib.rs
  - crates/rollout-coordinator/src/work_item.rs
  - crates/rollout-coordinator/tests/support/mod.rs
  - crates/rollout-coordinator/tests/support/sim.rs
  - crates/rollout-coordinator/tests/support/abort_harness.rs
  - crates/rollout-coordinator/Cargo.toml
autonomous: true
requirements: [DIST-01, DIST-02, DIST-03, DIST-04, DIST-05]
must_haves:
  truths:
    - "The CoordinatorLease trait + CoordEpoch/LeaseRecord types compile in rollout-core with no cloud SDK dependency"
    - "A shared WorkItemRecord CAS-on-state module exists with try_claim/try_complete/try_repending and a round-trip property test"
    - "An in-process 1-coordinator + N-worker simulation harness exists that the four witness tests can drive over EmbeddedStorage"
    - "A subprocess abort harness exists so split_brain's std::process::abort() can be exercised without killing the test runner"
  artifacts:
    - path: "crates/rollout-core/src/traits/lease.rs"
      provides: "CoordEpoch, LeaseRecord, CoordinatorLease trait"
      contains: "trait CoordinatorLease"
    - path: "crates/rollout-coordinator/src/work_item.rs"
      provides: "WorkItemRecord CAS state machine (mirror of SampleRecord)"
      contains: "fn try_claim"
    - path: "crates/rollout-coordinator/tests/support/sim.rs"
      provides: "in-process coordinator + N-worker simulation harness"
      contains: "struct"
    - path: "crates/rollout-coordinator/tests/support/abort_harness.rs"
      provides: "fork-a-subprocess helper for the abort witness"
      contains: "Command"
  key_links:
    - from: "crates/rollout-coordinator/src/work_item.rs"
      to: "rollout_core::StorageTxn::cas_bytes"
      via: "try_claim/try_complete CAS calls"
      pattern: "cas_bytes"
    - from: "crates/rollout-core/src/traits/lease.rs"
      to: "rollout_core::ids (WorkerId)"
      via: "LeaseRecord.holder field"
      pattern: "WorkerId"
---

<objective>
Establish the Wave-0 contracts and test scaffolding that every other Phase-6 plan
builds against: the `CoordinatorLease` trait (pure core, no SDK), the shared
`WorkItemRecord` CAS-on-state module, and the in-process simulation + subprocess
abort harnesses that the four named witnesses require (per 06-VALIDATION.md Wave-0
Requirements).

This is the DIST-03 architecture spike landing as code: define the interface
contracts BEFORE implementation so downstream plans (06-01 lease/epoch, 06-02
ledger/steal, 06-03 replayer/drain) receive types, not a scavenger hunt.

Purpose: Interface-first ordering — all four witness tests (in 06-01..03) compile
against the harness + trait shapes defined here.
Output: lease trait, shared CAS module, sim harness, abort harness, test-support tree.
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
@.planning/phases/06-multi-node-distribution/06-VALIDATION.md

<interfaces>
<!-- Contracts the executor must respect. Extracted from the codebase. -->

From crates/rollout-core/src/traits/storage.rs (StorageTxn — the CAS arbiter):
```rust
#[async_trait] pub trait Storage: Send + Sync {
    async fn begin(&self) -> Result<Box<dyn StorageTxn>, CoreError>;
    async fn get_bytes(&self, key: &StorageKey) -> Result<Option<Vec<u8>>, CoreError>;
    async fn scan_bytes(&self, range: KeyRange) -> Result<Vec<(StorageKey, Vec<u8>)>, CoreError>;
}
#[async_trait] pub trait StorageTxn: Send + Sync {
    async fn put_bytes(&mut self, key: StorageKey, value: Vec<u8>) -> Result<(), CoreError>;
    async fn cas_bytes(&mut self, key: StorageKey,
        expected: Option<Vec<u8>>, new: Option<Vec<u8>>) -> Result<bool, CoreError>;
}
pub struct StorageKey { pub namespace: SmolStr, pub run_id: Option<RunId>, pub path: Vec<String> }
```

From crates/rollout-core/src/ids.rs:
```rust
pub struct RunId(/* Ulid */);   pub struct WorkerId(/* Ulid */);   pub struct ContentId(/* blake3 */);
```

From crates/rollout-runtime-batch/src/state.rs (THE template to mirror — copy the shape):
```rust
pub enum SampleState { Pending, Running { worker_id, started_at_ms }, Done { .. }, Failed { .. } }
pub struct SampleRecord { pub id: ContentId, pub state: SampleState, /* .. */ }
pub fn sample_key(run_id: &RunId, sample_id: &ContentId) -> StorageKey;
pub async fn try_claim(txn, run_id, record, worker_id) -> Result<bool, CoreError>;   // CAS Pending->Running
pub async fn try_complete(txn, run_id, record, ...) -> Result<bool, CoreError>;       // CAS Running->Done
pub async fn try_repending(txn, run_id, record) -> Result<bool, CoreError>;           // CAS Running->Pending
```

From crates/rollout-core/src/traits/observability.rs:
```rust
pub enum EventKind { /* .. */ Domain { topic: SmolStr } }
pub struct Event { pub kind: EventKind, pub level: Level, pub run_id: Option<RunId>,
                   pub worker_id: Option<WorkerId>, /* .. */ }
#[async_trait] pub trait EventEmitter: Send + Sync { async fn emit(&self, event: Event) -> Result<(), CoreError>; }
```
</interfaces>
</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: Define the CoordinatorLease trait + epoch types in rollout-core</name>
  <read_first>
    - crates/rollout-core/src/traits/lease.rs (does not exist yet — create)
    - crates/rollout-core/src/traits/mod.rs (add `pub mod lease;` + re-exports)
    - crates/rollout-core/src/lib.rs (confirm trait re-export convention)
    - crates/rollout-core/src/ids.rs (WorkerId, RunId — Serialize/Deserialize derives to match)
    - .planning/phases/06-multi-node-distribution/06-RESEARCH.md §"DIST-03 Architecture Spike" subsection 1 (the proposed trait shape — implement it verbatim)
  </read_first>
  <behavior>
    - Test 1 (postcard round-trip): encode then decode a `LeaseRecord { holder, epoch: CoordEpoch(7), expires_at_ms }` yields an equal value (Pitfall 2: encode/decode stability).
    - Test 2 (epoch ordering): `CoordEpoch(0) < CoordEpoch(1)` and `Ord`/`Eq` derives hold.
    - Test 3 (no-impl-yet): the trait is object-safe — `fn _assert(_: &dyn CoordinatorLease) {}` compiles.
  </behavior>
  <action>
    Create `crates/rollout-core/src/traits/lease.rs` with NO cloud/SDK imports (keeps coord ↛ cloud + public-api-cloud-leak green). Define exactly:
    - `pub struct CoordEpoch(pub u64)` deriving `Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize` (+ Hash).
    - `pub struct LeaseRecord { pub holder: WorkerId, pub epoch: CoordEpoch, pub expires_at_ms: u128 }` deriving `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`.
    - `#[async_trait] pub trait CoordinatorLease: Send + Sync` with three methods exactly as the spike §1: `async fn try_acquire(&self, me: WorkerId, ttl: Duration) -> Result<Option<LeaseRecord>, CoreError>`, `async fn renew(&self, held: &LeaseRecord, ttl: Duration) -> Result<bool, CoreError>`, `async fn current(&self) -> Result<Option<LeaseRecord>, CoreError>`.
    Add `pub mod lease;` to `traits/mod.rs` and re-export `CoordEpoch, LeaseRecord, CoordinatorLease`. Crate-level + per-item rustdoc (DOCS-03). Put the 3 unit tests in a `#[cfg(test)] mod tests` in the same file.
    Do NOT implement the trait here — implementation is `StorageLease` in 06-01.
  </action>
  <verify>
    <automated>cargo test -p rollout-core --lib lease && cargo doc -p rollout-core --no-deps 2>&1 | grep -qv "missing"</automated>
  </verify>
  <acceptance_criteria>
    - `grep -q "pub trait CoordinatorLease" crates/rollout-core/src/traits/lease.rs` succeeds.
    - `cargo test -p rollout-core --lib lease` exits 0 (3 tests pass).
    - `cargo build -p rollout-core` exits 0; `cargo public-api -p rollout-core --simplified 2>/dev/null | grep -iE "aws|gcp|s3|sqs" ` returns no new SDK symbols (re-run scripts/check-public-api-cloud-leak.sh stays green).
    - Same commit touches inline rustdoc (DOCS-02 satisfied by doc comments on every pub item).
  </acceptance_criteria>
  <done>CoordEpoch + LeaseRecord + CoordinatorLease trait compile in rollout-core, object-safe, round-trip-stable, zero SDK leakage.</done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: Shared WorkItemRecord CAS-on-state module in rollout-coordinator</name>
  <read_first>
    - crates/rollout-runtime-batch/src/state.rs (THE template — mirror SampleState/SampleRecord/try_claim/try_complete/try_repending)
    - crates/rollout-runtime-batch/tests/cas_state_machine.rs (single-winner test pattern to replicate)
    - crates/rollout-coordinator/src/lib.rs (add `pub mod work_item;`)
    - crates/rollout-coordinator/src/work_item.rs (create)
    - .planning/phases/06-multi-node-distribution/06-RESEARCH.md §4 "Storage namespaces" (the `work` namespace key layout: `["item", <work_id>]`)
  </read_first>
  <behavior>
    - Test 1 (round trip): Pending -> try_claim(worker) -> Running -> try_complete -> Done, each CAS returns true; final scan reads Done once.
    - Test 2 (second claim loses): two try_claim against the same Pending record; exactly one returns true (single-winner property, mirror cas_state_machine.rs).
    - Test 3 (repending idempotent): try_repending on an already-Done record returns false, state unchanged.
  </behavior>
  <action>
    Create `crates/rollout-coordinator/src/work_item.rs`. Per RESEARCH §4, decide extract-vs-duplicate: DUPLICATE the shape from `rollout-runtime-batch::state` into the coordinator crate (avoids a new shared crate + a dep-direction edge; documented as intentional). Define:
    - `pub enum WorkState { Pending, Running { worker_id: WorkerId, started_at_ms: u128 }, Done { result_id: ContentId }, Failed { reason: String } }` (Serialize/Deserialize).
    - `pub struct WorkItemRecord { pub id: ContentId, pub state: WorkState }` (Serialize/Deserialize, Eq).
    - `pub fn work_key(run_id: &RunId, work_id: &ContentId) -> StorageKey` with `namespace="work"`, `path=vec!["item".into(), hex(work_id)]`.
    - `pub async fn try_claim(txn, run_id, &record, worker_id, now_ms) -> Result<bool, CoreError>` — CAS Pending->Running on exact prior bytes (postcard-encode the input record as `expected`, identical to state.rs).
    - `pub async fn try_complete(txn, run_id, &record, result_id) -> Result<bool, CoreError>` — CAS Running->Done.
    - `pub async fn try_repending(txn, run_id, &record) -> Result<bool, CoreError>` — CAS Running->Pending.
    Add `pub mod work_item;` to coordinator lib.rs. Put the 3 unit tests inline (`#[tokio::test]`, EmbeddedStorage over tempfile). Per-item rustdoc.
  </action>
  <verify>
    <automated>cargo test -p rollout-coordinator --lib work_item</automated>
  </verify>
  <acceptance_criteria>
    - `grep -q "pub async fn try_claim" crates/rollout-coordinator/src/work_item.rs` succeeds.
    - `grep -q 'namespace.*work' crates/rollout-coordinator/src/work_item.rs` confirms the `work` namespace.
    - `cargo test -p rollout-coordinator --lib work_item` exits 0 (3 tests; the second-claim-loses test asserts exactly one `true`).
    - DOCS-02: same commit adds tests (the inline `mod tests`).
  </acceptance_criteria>
  <done>WorkItemRecord CAS state machine compiles with single-winner + idempotent-repending witnessed; downstream steal/ledger code can reuse it.</done>
</task>

<task type="auto">
  <name>Task 3: In-process simulation harness + subprocess abort harness (test support)</name>
  <read_first>
    - crates/rollout-coordinator/tests/failure_scan.rs (existing test style: EmbeddedStorage over tempfile, NoopEmitter)
    - crates/rollout-coordinator/src/emitter.rs (NoopEmitter / StdoutJsonEmitter for a counting emitter)
    - crates/rollout-coordinator/src/main.rs (the binary entrypoint the abort-harness subprocess will invoke)
    - crates/rollout-coordinator/Cargo.toml (add `[[test]]` harness if needed; tempfile is already a dev-dep)
    - .planning/phases/06-multi-node-distribution/06-RESEARCH.md §6 (test skeleton: decision-vs-abort split) + 06-VALIDATION.md Wave-0 Requirements
  </read_first>
  <action>
    Create the shared test-support tree under `crates/rollout-coordinator/tests/support/`:
    - `mod.rs`: re-exports `sim`, `abort_harness`, and a `CountingEmitter` (an `EventEmitter` that records counts per `Domain { topic }` in an `Arc<Mutex<HashMap<String,usize>>>`, exposes `count(&self, topic) -> usize`). NO shared-state write — observability sink only (supports D-FENCE-02 assertion).
    - `sim.rs`: an in-process harness `struct Sim { storage: Arc<dyn Storage>, run_id: RunId, _tmp: TempDir }` with `Sim::new(num_workers) -> Self` (opens EmbeddedStorage over a tempdir), `spawn_worker(id) -> WorkerHandle`, and helpers to seed N `WorkItemRecord::Pending` items into the `work` namespace and to assert "every work_id reaches Done exactly once" (scan `work` namespace, assert each record `Done`, no duplicate ids). This is the substrate for coord_restart_no_duplicates, concurrent_ack_and_steal, spot_drain.
    - `abort_harness.rs`: `fn run_fence_subprocess(args) -> std::process::Output` that `std::process::Command`s the compiled `rollout-coordinator` binary (or a dedicated `--test-fence` hidden subcommand) in a child process so an actual `std::process::abort()` exits the CHILD (asserting non-zero/SIGABRT exit + wall-clock < 5s) without killing the test runner. Document WHY (06-RESEARCH §6 + Pitfall 5: abort is in-process-fatal).
    Add a one-line smoke test `tests/support_smoke.rs` that builds `Sim::new(3)`, seeds 2 items, asserts the scan helper sees 2 Pending — proves the harness compiles and runs.
  </action>
  <verify>
    <automated>cargo test -p rollout-coordinator --test support_smoke</automated>
  </verify>
  <acceptance_criteria>
    - `test -f crates/rollout-coordinator/tests/support/sim.rs && test -f crates/rollout-coordinator/tests/support/abort_harness.rs` succeeds.
    - `grep -q "std::process::Command" crates/rollout-coordinator/tests/support/abort_harness.rs` confirms the fork-based abort harness.
    - `grep -q "CountingEmitter" crates/rollout-coordinator/tests/support/mod.rs` succeeds.
    - `cargo test -p rollout-coordinator --test support_smoke` exits 0.
    - `grep -q "Done exactly once" crates/rollout-coordinator/tests/support/sim.rs` (the no-duplicates assertion helper exists, comment or fn name).
  </acceptance_criteria>
  <done>Sim harness + CountingEmitter + subprocess abort harness compile and run; the four witness plans can `mod support;` and drive them.</done>
</task>

</tasks>

<verification>
- `cargo test -p rollout-core --lib lease` green (trait + types).
- `cargo test -p rollout-coordinator --lib work_item` green (CAS state machine).
- `cargo test -p rollout-coordinator --test support_smoke` green (harness).
- `cargo clippy -p rollout-core -p rollout-coordinator --all-targets -- -D warnings` clean.
- `cargo test -p rollout-core --test dependency_direction` still green (coord ↛ cloud, no new SDK dep on core).
</verification>

<success_criteria>
- CoordinatorLease trait + CoordEpoch/LeaseRecord land in rollout-core with zero SDK leakage (DIST-01 contract).
- Shared WorkItemRecord CAS module with single-winner + idempotent witnesses (DIST-02 dedup primitive).
- In-process Sim harness + CountingEmitter + subprocess abort harness exist (the Wave-0 infra for all 4 named witnesses).
- All Wave-0 items in 06-VALIDATION.md checked off; downstream plans compile against these contracts.
</success_criteria>

<output>
After completion, create `.planning/phases/06-multi-node-distribution/06-00-SUMMARY.md`
</output>
