---
phase: 06-multi-node-distribution
plan: 01
type: execute
wave: 1
depends_on: ["06-00"]
files_modified:
  - crates/rollout-coordinator/src/lease.rs
  - crates/rollout-coordinator/src/epoch.rs
  - crates/rollout-coordinator/src/fence.rs
  - crates/rollout-coordinator/src/main.rs
  - crates/rollout-coordinator/src/config.rs
  - crates/rollout-coordinator/src/heartbeat.rs
  - crates/rollout-coordinator/src/lib.rs
  - crates/rollout-coordinator/tests/lease.rs
  - crates/rollout-coordinator/tests/split_brain.rs
  - database/migrations/0003_coordinator_lease.sql
  - docs/specs/05-distribution.md
autonomous: true
requirements: [DIST-01, DIST-05]
must_haves:
  truths:
    - "Exactly one of two racing coordinators acquires the single-row lease; the other gets None (one coordinator per run)"
    - "Stealing an expired lease advances the epoch monotonically (epoch+1); renewing keeps the epoch constant"
    - "An old coordinator whose lease was stolen detects it (renew returns false), emits exactly one coordinator_fenced event with no shared-state write, and the decision is to abort"
    - "Workers reject any RPC response tagged with an epoch lower than the highest epoch they have seen"
  artifacts:
    - path: "crates/rollout-coordinator/src/lease.rs"
      provides: "StorageLease: CoordinatorLease impl over Arc<dyn Storage> (embedded + Postgres)"
      contains: "impl CoordinatorLease for StorageLease"
    - path: "crates/rollout-coordinator/src/epoch.rs"
      provides: "epoch read/advance + RPC epoch-stamping + stale-epoch rejection helpers"
      contains: "CoordEpoch"
    - path: "crates/rollout-coordinator/src/fence.rs"
      provides: "fence_old_coordinator() decision fn + coordinator_fenced emit (no kv write)"
      contains: "coordinator_fenced"
    - path: "crates/rollout-coordinator/src/main.rs"
      provides: "hidden --test-fence subcommand: calls fence_old_coordinator then std::process::abort (abort-witness edge)"
      contains: "test-fence"
    - path: "crates/rollout-coordinator/tests/split_brain.rs"
      provides: "split_brain_old_coord_self_fences witness"
      contains: "split_brain_old_coord_self_fences"
  key_links:
    - from: "crates/rollout-coordinator/src/lease.rs"
      to: "rollout_core::StorageTxn::cas_bytes"
      via: "try_acquire/renew single-row CAS on coordinator_lease namespace"
      pattern: "cas_bytes"
    - from: "crates/rollout-coordinator/src/fence.rs"
      to: "rollout_core::EventEmitter"
      via: "emit Domain { topic: coordinator_fenced } — observability sink only"
      pattern: "coordinator_fenced"
    - from: "crates/rollout-coordinator/src/main.rs"
      to: "crates/rollout-coordinator/src/fence.rs"
      via: "--test-fence subcommand calls fence_old_coordinator then std::process::abort"
      pattern: "test-fence"
    - from: "crates/rollout-coordinator/src/heartbeat.rs"
      to: "crates/rollout-coordinator/src/epoch.rs"
      via: "stamp coord_epoch on every RPC response"
      pattern: "epoch"
---

<objective>
Implement DIST-01 (lease-based single-coordinator exclusion + epoch in Storage)
and DIST-05 (split-brain fencing). Build the `StorageLease` impl of the
`CoordinatorLease` trait (from 06-00) over `Arc<dyn Storage>` — one impl serving
BOTH embedded redb and Postgres because `cas_bytes` is already dual-backed
(D-LEASE-01 satisfied without two impls). Add epoch stamping/validation and the
self-fence decision path, landing the `split_brain_old_coord_self_fences` witness.

Purpose: the lease + epoch are the authority primitive every later plan depends on.
Output: StorageLease, epoch helpers, fence decision, the `--test-fence` abort edge, two witnesses (lease CAS + split_brain) including the subprocess abort-within-5s witness.
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

<interfaces>
From 06-00 — crates/rollout-core/src/traits/lease.rs:
```rust
pub struct CoordEpoch(pub u64);
pub struct LeaseRecord { pub holder: WorkerId, pub epoch: CoordEpoch, pub expires_at_ms: u128 }
#[async_trait] pub trait CoordinatorLease: Send + Sync {
    async fn try_acquire(&self, me: WorkerId, ttl: Duration) -> Result<Option<LeaseRecord>, CoreError>;
    async fn renew(&self, held: &LeaseRecord, ttl: Duration) -> Result<bool, CoreError>;
    async fn current(&self) -> Result<Option<LeaseRecord>, CoreError>;
}
```

From crates/rollout-transport/src/config.rs (timing source of truth — reuse, do not redefine):
```rust
heartbeat_interval = 500ms;  worker_self_fence_timeout = 4s;
coordinator_failure_timeout = 5s;  clock_skew_budget = 250ms;
TransportConfig::validate_cross_fields() // enforces self_fence < coord_failure
```

From crates/rollout-coordinator/src/main.rs (the binary entrypoint to extend — add a hidden subcommand):
```rust
enum Sub { Run { config: PathBuf } }  // add hidden `--test-fence <stale> <observed>` subcommand
```

From crates/rollout-coordinator/src/failure_scan.rs (emit template to mirror for coordinator_fenced):
```rust
emitter.emit(Event { kind: EventKind::Domain { topic: "worker_failed".into() },
    level: Level::Error, run_id: Some(run_id), worker_id: Some(wid), .. }).await;
```
</interfaces>
</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: StorageLease — CAS acquire/renew/steal with monotonic epoch</name>
  <read_first>
    - crates/rollout-core/src/traits/lease.rs (the trait to implement, from 06-00)
    - crates/rollout-runtime-batch/src/state.rs (try_claim: the exact CAS-on-prior-bytes pattern to copy)
    - crates/rollout-cloud-local/src/queue.rs (InMemQueue scan-on-open: the Storage-mirroring pattern)
    - crates/rollout-coordinator/src/lib.rs (add `pub mod lease; pub mod epoch;`)
    - .planning/phases/06-multi-node-distribution/06-RESEARCH.md §"DIST-03 Architecture Spike" subsections 2 (acquire/steal/renew algorithm) + §"Common Pitfalls" 1,2,3 (epoch confusion, encode drift, clock skew)
  </read_first>
  <behavior>
    - Test `lease_exclusion_single_winner` (SC1): two `try_acquire` against a fresh lease — exactly one returns `Some`, the other `None`; `current()` shows one holder.
    - Test `steal_advances_epoch`: acquire at epoch 0, let TTL expire, second `try_acquire` returns `Some` with `epoch == 1` (monotonic advance, D-FENCE-05 / Pitfall 1).
    - Test `renew_keeps_epoch`: incumbent `renew(held, ttl)` returns true and `current().epoch` is unchanged.
    - Test `renew_after_steal_fails`: after a steal advances the epoch, the old holder's `renew(stale_held)` returns `false`.
    - Test `lease_record_roundtrip` (Pitfall 2): postcard encode/decode of LeaseRecord is byte-stable across re-encode.
  </behavior>
  <action>
    Create `crates/rollout-coordinator/src/lease.rs` with `pub struct StorageLease { storage: Arc<dyn Storage>, run_id: RunId }` and `StorageLease::new(storage, run_id)`. Implement `CoordinatorLease`:
    - `lease_key(run_id)` = `StorageKey { namespace: SmolStr::new_static("coordinator_lease"), run_id: Some(run_id), path: vec![] }` (single row).
    - `try_acquire(me, ttl)`: per RESEARCH §2 algorithm — `get_bytes(lease_key)`; if `None` → CAS(expected=None, new=encode(LeaseRecord{me, epoch:0, expires_at:now+ttl})); if `Some(r)` and `now > r.expires_at_ms` → CAS(expected=Some(encode(r)), new=encode(LeaseRecord{me, epoch:r.epoch+1, ...})) [MONOTONIC]; if `Some(_)` live → return `Ok(None)`. CAS on EXACT prior bytes read back (Pitfall 2). Also write the `epoch` namespace row (`StorageKey namespace="epoch"`) inside the SAME txn so lease-epoch == ledger-epoch (RESEARCH §4).
    - `renew(held, ttl)`: CAS(expected=Some(encode(held)), new=encode(LeaseRecord{held.holder, held.epoch /*SAME*/, expires_at:now+ttl})) → returns false iff epoch advanced under us (Pitfall 1).
    - `current()`: `get_bytes(lease_key).map(decode)`.
    - Use a clock: accept an injectable `now_ms()` (or `rollout_core::Clock`) so tests can compress TTL to 50ms (RESEARCH §6 uses 50ms ttl). Wall-clock via the existing Clock trait.
    Put the 5 unit tests inline OR in `crates/rollout-coordinator/tests/lease.rs` (EmbeddedStorage over tempfile). Per-item rustdoc. Add `pub mod lease;` to lib.rs.
  </action>
  <verify>
    <automated>cargo test -p rollout-coordinator lease</automated>
  </verify>
  <acceptance_criteria>
    - `grep -q "impl CoordinatorLease for StorageLease" crates/rollout-coordinator/src/lease.rs` succeeds.
    - `grep -q "coordinator_lease" crates/rollout-coordinator/src/lease.rs` AND `grep -q '"epoch"' crates/rollout-coordinator/src/lease.rs` (both namespaces written).
    - `grep -q "epoch.0 + 1\|epoch: CoordEpoch(.*+ 1\|r.epoch.*+ 1" crates/rollout-coordinator/src/lease.rs` (monotonic advance on steal).
    - `cargo test -p rollout-coordinator lease` exits 0 (all 5 tests incl. `lease_exclusion_single_winner` SC1 witness).
    - `cargo test -p rollout-core --test dependency_direction` green (StorageLease depends only on rollout-core::Storage, never cloud).
  </acceptance_criteria>
  <done>StorageLease implements single-winner CAS lease with monotonic-on-steal / constant-on-renew epoch over the dual-backed Storage trait; SC1 witnessed.</done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: Epoch stamping + stale-epoch rejection + lease timing validation</name>
  <read_first>
    - crates/rollout-coordinator/src/epoch.rs (create)
    - crates/rollout-coordinator/src/heartbeat.rs (CoordinatorImpl — extend RPC responses to carry coord_epoch)
    - crates/rollout-coordinator/src/config.rs (CoordinatorConfig::validate — extend cross-field checks for lease TTL)
    - crates/rollout-transport/src/config.rs (validate_cross_fields: self_fence(4s) < coord_failure(5s) — the invariant to reuse)
    - .planning/phases/06-multi-node-distribution/06-RESEARCH.md §"Epoch Fencing Correctness" (the 5-point invariant chain) + §"DIST-03 Spike" subsection 2 TTL/renewal cadence
  </read_first>
  <behavior>
    - Test `worker_rejects_stale_epoch` (D-FENCE-04): an `EpochGuard` seeded with seen_max=CoordEpoch(2) rejects a response tagged CoordEpoch(1) (returns Err/false) and accepts CoordEpoch(2) and CoordEpoch(3) (updating seen_max to 3).
    - Test `lease_ttl_equals_coord_failure`: a `validate()` on a config where lease TTL != coordinator_failure_timeout is rejected (or a helper asserts ttl defaults to coord_failure 5s, renew cadence to heartbeat 500ms).
    - Test `seen_max_is_monotonic`: feeding 3,1,2,5 leaves seen_max=5; only 5 (and the first 3) were accepted as advances.
  </behavior>
  <action>
    Create `crates/rollout-coordinator/src/epoch.rs`:
    - `pub fn current_epoch(storage, run_id) -> Result<CoordEpoch, CoreError>` reading the `epoch` namespace row (default `CoordEpoch(0)` if absent).
    - `pub struct EpochGuard { seen_max: CoordEpoch }` (worker-side) with `accept(&mut self, resp_epoch: CoordEpoch) -> Result<(), CoreError>`: reject (`Err(Recoverable(Transient))` or a typed StaleEpoch error) if `resp_epoch < self.seen_max`; else update `seen_max = max(seen_max, resp_epoch)` and accept. This is point 3 of the invariant chain.
    - A `stamp_epoch(resp, epoch)` helper (or extend heartbeat response structs) so heartbeat/control/work responses carry the coordinator's `coord_epoch` (point 2). Wire `CoordinatorImpl` in heartbeat.rs to include its current epoch on responses (the proto field addition is Claude's discretion per CONTEXT; if proto regen is heavy, carry epoch as a response field in the existing message and note the proto task for 06-04 smoke wiring).
    Extend `CoordinatorConfig::validate()` in config.rs to assert lease timing: ttl == coordinator_failure_timeout (5s default) and renew cadence == heartbeat_interval (500ms) lie within the transport bounds; reuse `TransportConfig::validate_cross_fields` (do NOT re-derive timing). Add a lease-TTL cross-field message.
    Add `pub mod epoch;` to lib.rs. Tests inline. Per-item rustdoc.
  </action>
  <verify>
    <automated>cargo test -p rollout-coordinator epoch</automated>
  </verify>
  <acceptance_criteria>
    - `grep -q "struct EpochGuard" crates/rollout-coordinator/src/epoch.rs` AND `grep -q "fn accept" crates/rollout-coordinator/src/epoch.rs`.
    - `grep -q "seen_max\|resp_epoch < " crates/rollout-coordinator/src/epoch.rs` (stale-epoch rejection logic present).
    - `cargo test -p rollout-coordinator epoch` exits 0 (incl. `worker_rejects_stale_epoch`).
    - `grep -q "coordinator_failure_timeout" crates/rollout-coordinator/src/config.rs` (lease-TTL validation reuses the transport bound).
    - DOCS-02: same commit ships inline tests + rustdoc.
  </acceptance_criteria>
  <done>Coordinator stamps coord_epoch on responses; workers reject stale epochs via EpochGuard; lease TTL/cadence validated against the existing transport timing bounds.</done>
</task>

<task type="auto" tdd="true">
  <name>Task 3: Self-fence decision + coordinator_fenced event + --test-fence abort edge + split_brain witness + Postgres DDL</name>
  <read_first>
    - crates/rollout-coordinator/src/fence.rs (create)
    - crates/rollout-coordinator/src/main.rs (the binary entrypoint — add the hidden `--test-fence` subcommand that calls fence then std::process::abort)
    - crates/rollout-coordinator/src/failure_scan.rs (the worker_failed emit pattern to mirror — emit through EventEmitter, NOT storage)
    - crates/rollout-coordinator/src/emitter.rs (StdoutJsonEmitter; CountingEmitter is in tests/support from 06-00)
    - crates/rollout-coordinator/tests/split_brain.rs (create — the skeleton is in RESEARCH §6, copy it)
    - crates/rollout-coordinator/tests/support/abort_harness.rs (from 06-00 — subprocess abort; it invokes the `--test-fence` subcommand)
    - .planning/phases/06-multi-node-distribution/06-RESEARCH.md §6 (test skeleton) + §"Epoch Fencing Correctness" (D-FENCE-01/02/03 + Pitfall 5) + §3 (Postgres DDL)
    - docs/specs/05-distribution.md §8 (exactly-one-winner; loser exits — update if behavior differs)
  </read_first>
  <behavior>
    - Test `split_brain_old_coord_self_fences` (SC4, in-process decision): A acquires epoch 0; TTL expires; B steals → epoch 1; A's `renew(stale)` returns false; calling `fence_old_coordinator(&CountingEmitter, stale_epoch, observed_epoch)` emits EXACTLY ONE `coordinator_fenced` event (assert `emitter.count("coordinator_fenced") == 1`) and returns `FenceDecision::Abort`; assert the `epoch`/lease row still reads B's epoch=1 (A wrote nothing — D-FENCE-01).
    - Test `fence_writes_no_shared_state`: snapshot the lease row bytes before fence, call fence, assert bytes unchanged (observability sink only).
    - Subprocess test `fence_aborts_within_5s` (via abort_harness): the child process runs `rollout-coordinator --test-fence <stale> <observed>` (the subcommand created in this task), which takes the real `std::process::abort()` path; the child exits non-zero (SIGABRT) within 5s wall-clock.
  </behavior>
  <action>
    Create `crates/rollout-coordinator/src/fence.rs`:
    - `pub enum FenceDecision { Abort }` (open for future variants).
    - `pub async fn fence_old_coordinator(emitter: &dyn EventEmitter, stale: CoordEpoch, observed: CoordEpoch) -> FenceDecision`: (1) emit EXACTLY ONE `Event { kind: Domain { topic: "coordinator_fenced" }, level: Error, run_id, worker_id: Some(coord_id), attrs: {stale_epoch, observed_epoch} }` through the emitter — mirror failure_scan.rs `worker_failed`; (2) flush synchronously BEFORE returning (Pitfall 5); (3) make NO `storage.begin()/put_bytes` call (D-FENCE-01/02); (4) return `FenceDecision::Abort`.
    Add the hidden `--test-fence <stale> <observed>` subcommand to `crates/rollout-coordinator/src/main.rs` (extend the `Sub` enum): it calls `fence::fence_old_coordinator` and then `std::process::abort()` (D-FENCE-03). This is the binary-edge that performs the ACTUAL abort so the in-process witness never aborts the test runner; `tests/support/abort_harness.rs` (06-00) invokes this subcommand for the SC4 subprocess abort. Keep it hidden (e.g. `#[command(hide = true)]` or an undocumented arg) so it does not appear in operator help.
    Create `crates/rollout-coordinator/tests/split_brain.rs` from the RESEARCH §6 skeleton: drive StorageLease + CountingEmitter for the in-process decision witness; add the subprocess `fence_aborts_within_5s` test using `tests/support/abort_harness.rs` (which spawns the `--test-fence` subcommand).
    Create `database/migrations/0003_coordinator_lease.sql` = the optional typed table from RESEARCH §3 (additive; documented as specialization — the generic-kv StorageLease is the canonical path). Include the conditional-UPDATE CAS comment block.
    Update `docs/specs/05-distribution.md`: document the lease/epoch/fence model (single-row CAS, monotonic epoch on steal, self-fence on lost renew, exactly-one coordinator_fenced event, the `--test-fence` abort edge). Wire `pub mod fence;` into lib.rs.
  </action>
  <verify>
    <automated>cargo test -p rollout-coordinator split_brain_old_coord_self_fences</automated>
  </verify>
  <acceptance_criteria>
    - `cargo test -p rollout-coordinator split_brain_old_coord_self_fences` exits 0 (SC4 in-process witness — the named test from ROADMAP SC4 + VALIDATION map).
    - `grep -q "coordinator_fenced" crates/rollout-coordinator/src/fence.rs` AND the test asserts `count("coordinator_fenced") == 1`.
    - `! grep -q "storage.begin\|put_bytes\|cas_bytes" crates/rollout-coordinator/src/fence.rs` (loser writes NO shared state — D-FENCE-01).
    - `grep -q "test-fence" crates/rollout-coordinator/src/main.rs && grep -q "process::abort" crates/rollout-coordinator/src/main.rs` (the abort edge lives at the binary).
    - `test -f database/migrations/0003_coordinator_lease.sql` AND `grep -q "expires_at < now()" database/migrations/0003_coordinator_lease.sql` (CAS steal-on-expiry).
    - `cargo test -p rollout-coordinator fence_aborts_within_5s` exits 0 (subprocess SIGABRT within 5s — the abort harness finds the `--test-fence` subcommand created here).
    - DOCS-02: same commit updates docs/specs/05-distribution.md.
  </acceptance_criteria>
  <done>split_brain_old_coord_self_fences green: old coord emits exactly one coordinator_fenced event, writes no shared state, decides Abort; the `--test-fence` binary subcommand + subprocess witness prove abort within 5s on every commit; optional Postgres DDL landed.</done>
</task>

</tasks>

<verification>
- `cargo test -p rollout-coordinator` green (lease, epoch, split_brain witnesses).
- `cargo test -p rollout-coordinator split_brain_old_coord_self_fences` exits 0 (SC4).
- `cargo test -p rollout-coordinator fence_aborts_within_5s` exits 0 (subprocess abort-within-5s witness, backed by the `--test-fence` subcommand landed in this plan).
- `cargo test -p rollout-coordinator lease_exclusion_single_winner` exits 0 (SC1).
- `cargo test -p rollout-core --test dependency_direction` green (coord ↛ cloud preserved).
- `cargo clippy -p rollout-coordinator --all-targets -- -D warnings` clean.
</verification>

<success_criteria>
- DIST-01: single-row CAS lease (StorageLease) gives exactly-one-coordinator-per-run; `work`/`epoch`/`coordinator_lease` namespaces in Storage.
- DIST-05: epoch monotonic on steal, stamped on every RPC, stale-epoch rejected by workers; old coord self-fences with exactly one coordinator_fenced event + no shared-state write + abort within 5s (proven by the `--test-fence` subprocess witness).
- Named witnesses `lease_exclusion_single_winner` (SC1), `split_brain_old_coord_self_fences` (SC4), and `fence_aborts_within_5s` green on every commit, Docker-free.
</success_criteria>

<output>
After completion, create `.planning/phases/06-multi-node-distribution/06-01-SUMMARY.md`
</output>
</content>
</invoke>
