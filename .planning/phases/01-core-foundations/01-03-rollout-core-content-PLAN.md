---
phase: 01-core-foundations
plan: 03
type: execute
wave: 2
depends_on: ['01-core-foundations/01']
files_modified:
  - crates/rollout-core/src/lib.rs
  - crates/rollout-core/src/ids.rs
  - crates/rollout-core/src/errors.rs
  - crates/rollout-core/src/traits/mod.rs
  - crates/rollout-core/src/traits/algorithm.rs
  - crates/rollout-core/src/traits/worker.rs
  - crates/rollout-core/src/traits/plugin.rs
  - crates/rollout-core/src/traits/harness.rs
  - crates/rollout-core/src/traits/backend.rs
  - crates/rollout-core/src/traits/storage.rs
  - crates/rollout-core/src/traits/cloud.rs
  - crates/rollout-core/src/traits/clock.rs
  - crates/rollout-core/src/config/mod.rs
  - crates/rollout-core/src/config/defaults.rs
  - crates/rollout-core/tests/trait_surface.rs
  - crates/rollout-core/tests/error_taxonomy.rs
  - crates/rollout-core/tests/id_types.rs
autonomous: true
requirements: [CORE-01, CORE-03, CORE-05, DOCS-03]

must_haves:
  truths:
    - "All 19 traits from CORE-01 are public from `rollout-core` and Send + Sync where required"
    - "CoreError = Recoverable | Fatal with #[from] propagation per CORE-03"
    - "RunId/WorkerId/ContentId implement Serialize + Deserialize + Display + FromStr per CORE-05"
    - "ContentId::of(data) is deterministic (blake3) and equal for equal inputs"
    - "RunConfig with schema_version: u32 derives JsonSchema + Serialize + Deserialize + deny_unknown_fields"
    - "`cargo test -p rollout-core` passes (all unit + integration tests green)"
    - "`cargo doc -p rollout-core --no-deps --all-features` passes under the §9.3 RUSTDOCFLAGS (DOCS-03)"
  artifacts:
    - path: "crates/rollout-core/src/ids.rs"
      provides: "RunId, WorkerId, ContentId"
      contains: "pub struct RunId"
    - path: "crates/rollout-core/src/errors.rs"
      provides: "CoreError + RecoverableError + FatalError + RetryHint"
      contains: "pub enum CoreError"
    - path: "crates/rollout-core/src/traits/mod.rs"
      provides: "All 19 trait modules re-exported"
      contains: "pub use"
    - path: "crates/rollout-core/src/config/mod.rs"
      provides: "RunConfig type tree with JsonSchema derive"
      contains: "pub struct RunConfig"
    - path: "crates/rollout-core/tests/trait_surface.rs"
      provides: "Compile-time assertions that all 19 traits exist and are object-safe where intended"
    - path: "crates/rollout-core/tests/error_taxonomy.rs"
      provides: "CoreError variants + #[from] propagation tests"
    - path: "crates/rollout-core/tests/id_types.rs"
      provides: "ID round-trip + serde + ContentId determinism tests"
  key_links:
    - from: "crates/rollout-core/src/lib.rs"
      to: "all sub-modules (traits, errors, ids, config)"
      via: "pub mod / pub use"
      pattern: "pub (mod|use)"
    - from: "crates/rollout-core/src/errors.rs"
      to: "thiserror"
      via: "derive macro"
      pattern: "#\\[derive\\(.*Error.*\\)\\]"
    - from: "crates/rollout-core/src/config/mod.rs"
      to: "schemars JsonSchema derive"
      via: "derive macro"
      pattern: "JsonSchema"
---

<objective>
Populate `rollout-core` with the actual trait surface (all 19 traits from CORE-01), error taxonomy (`CoreError` per CORE-03), ID types (`RunId`/`WorkerId`/`ContentId` per CORE-05), and the `RunConfig` type tree with `JsonSchema` derives. Wave 0 test files are written FIRST and must fail before implementation, then pass once implementation lands (RED → GREEN within each task). Additionally seed crate-level + per-`pub`-item doc comments so DOCS-03 (rustdoc gate in Plan 06) passes for `rollout-core`.

Purpose: This is the **type/trait contract** that every downstream plan and phase depends on. Without it, Plan 04 has nothing to schema-gen, Plan 05 has nothing to lint, and Phases 2+ have no traits to implement. DOCS-03 is a standing rule (AGENTS.md §9.3) — every published crate needs a crate-level `//!` and one-line `///` on every public item, starting now.
Output: `cargo test -p rollout-core` passes; the trait surface, error enum, ID types, and `RunConfig` are all public, documented, and compile cleanly under `clippy -D warnings` AND under the rustdoc-gate `RUSTDOCFLAGS`.
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
@$HOME/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/STATE.md
@.planning/ROADMAP.md
@.planning/REQUIREMENTS.md
@.planning/phases/01-core-foundations/01-CONTEXT.md
@.planning/phases/01-core-foundations/01-RESEARCH.md
@.planning/phases/01-core-foundations/01-VALIDATION.md
@AGENTS.md
@ARCHITECTURE.md
@docs/specs/01-core-runtime.md
@docs/specs/11-config-schema.md
@docs/design-principles.md
@.planning/phases/01-core-foundations/01-PLAN-01-SUMMARY.md
@Cargo.toml
@crates/rollout-core/Cargo.toml
@crates/rollout-core/src/lib.rs

<interfaces>
<!-- Exact trait names from CORE-01 (CONTEXT.md D-CRATE-02): -->
<!-- PolicyAlgorithm, Worker, Coordinator, Scheduler, Plugin, EnvHarness, ToolHarness, EvalHarness, -->
<!-- RewardModel, InferenceBackend, Storage, StorageTxn, Queue, ObjectStore, SecretStore, -->
<!-- ComputeHint, Snapshotter, PluginHost, Clock. (19 total.) -->

<!-- Module layout (RESEARCH.md §Architecture Patterns → Recommended rollout-core Structure): -->
<!-- traits/algorithm.rs: PolicyAlgorithm -->
<!-- traits/worker.rs:    Worker, Coordinator, Scheduler -->
<!-- traits/plugin.rs:    Plugin, PluginHost -->
<!-- traits/harness.rs:   EnvHarness, ToolHarness, EvalHarness, RewardModel -->
<!-- traits/backend.rs:   InferenceBackend -->
<!-- traits/storage.rs:   Storage, StorageTxn, Snapshotter -->
<!-- traits/cloud.rs:     ObjectStore, SecretStore, ComputeHint, Queue -->
<!-- traits/clock.rs:     Clock (sync; no async_trait) -->

<!-- CoreError shape (RESEARCH.md §Pattern 3 + CONTEXT.md D-ERR-01): -->
pub enum CoreError {
    #[error("recoverable: {0}")] Recoverable(#[from] RecoverableError),
    #[error("fatal: {0}")] Fatal(#[from] FatalError),
}
pub enum RecoverableError { Throttled { hint: RetryHint }, Transient { msg: String, hint: RetryHint }, Preempted { hint: RetryHint } }
pub enum FatalError { ConfigInvalid { msg: String }, SchemaViolation { msg: String }, PluginContract { plugin: String, msg: String }, Internal { msg: String } }
pub enum RetryHint { Never, After(Duration), Backoff { base: Duration, max: Duration } }

<!-- ID shape (RESEARCH.md §Pattern 4 + CONTEXT.md D-ID-01): -->
pub struct RunId(pub Ulid);     // serde transparent, Display, FromStr (via ulid::Ulid)
pub struct WorkerId(pub Ulid);  // same shape
pub struct ContentId(pub [u8; 32]);  // blake3; impl ContentId::of(&[u8]) -> Self
</interfaces>
</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: Wave 0 test scaffolds (RED) + ID types + error taxonomy implementation (GREEN)</name>
  <files>crates/rollout-core/tests/id_types.rs, crates/rollout-core/tests/error_taxonomy.rs, crates/rollout-core/src/ids.rs, crates/rollout-core/src/errors.rs, crates/rollout-core/src/lib.rs</files>
  <read_first>
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-RESEARCH.md (§Pattern 3 CoreError + §Pattern 4 ID types + §Anti-Patterns)
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-VALIDATION.md (Wave 0 list: tests/error_taxonomy.rs, tests/id_types.rs)
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-CONTEXT.md (D-ERR-01, D-ID-01)
    - /Users/ashutosh/personal/rollout/AGENTS.md (principle 8 — no Box<dyn Error>; §8 house style; §9.3 rustdoc gate)
    - /Users/ashutosh/personal/rollout/docs/specs/01-core-runtime.md §2 (RunId/WorkerId field shape)
    - existing /Users/ashutosh/personal/rollout/crates/rollout-core/src/lib.rs (from plan 01)
  </read_first>
  <behavior>
    Tests (must be written first; RED before impl):
    - `id_types::run_id_display_from_str_roundtrip` — `let r = RunId(Ulid::new()); assert_eq!(RunId::from_str(&r.to_string()).unwrap(), r);`
    - `id_types::worker_id_display_from_str_roundtrip` — same shape for WorkerId
    - `id_types::content_id_determinism` — `assert_eq!(ContentId::of(b"data"), ContentId::of(b"data"))` and `assert_ne!(ContentId::of(b"data"), ContentId::of(b"other"))`
    - `id_types::run_id_serde_json` — round-trip via `serde_json::to_string` + `from_str::<RunId>`; serialized form is a string (transparent), not an object
    - `id_types::content_id_known_vector` — `ContentId::of(b"")` equals the blake3 hash of the empty input (constant: blake3("") = af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262)
    - `error_taxonomy::variants_exist` — pattern-match `CoreError::Recoverable(_)` and `CoreError::Fatal(_)` compile
    - `error_taxonomy::from_propagation` — `fn f() -> Result<(), CoreError> { Err(RecoverableError::Preempted { hint: RetryHint::Never })?; Ok(()) }` compiles and returns `Err(CoreError::Recoverable(_))`
    - `error_taxonomy::display_formats_recoverable_and_fatal` — `format!("{}", CoreError::Recoverable(RecoverableError::Throttled { hint: RetryHint::Never }))` starts with `"recoverable: "`
    - `error_taxonomy::not_serializable` — (compile-only assertion via `static_assertions` OR a doc comment noting CoreError must not derive Serialize per RESEARCH.md Anti-Patterns); concretely: there is NO `#[derive(Serialize)]` on `CoreError` (grep verifies)
  </behavior>
  <action>
1. **RED — write tests first**:
   a. Create `/Users/ashutosh/personal/rollout/crates/rollout-core/tests/id_types.rs` with the 5 test functions listed in `<behavior>`. Use `use rollout_core::{RunId, WorkerId, ContentId};` and `use std::str::FromStr;`. The blake3-empty-input vector is `af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262` — write it as a `const EMPTY_BLAKE3: [u8; 32] = hex_literal::hex!("...")` OR (no extra dep) decode by hand: write the 32 bytes as a `[u8; 32]` literal. Prefer the literal-array approach to avoid pulling in `hex-literal`.
   b. Create `/Users/ashutosh/personal/rollout/crates/rollout-core/tests/error_taxonomy.rs` with the 4 test functions listed in `<behavior>`. Use `use rollout_core::{CoreError, RecoverableError, FatalError, RetryHint};`.
   c. Confirm RED: `cd /Users/ashutosh/personal/rollout && cargo test -p rollout-core 2>&1 | tail -20` shows compile errors referencing missing types `RunId`, `CoreError`, etc.

2. **GREEN — implement `crates/rollout-core/src/ids.rs`** per RESEARCH.md §Pattern 4. Add one-line `///` doc comments on every `pub` item (AGENTS.md §9.3):
   ```rust
   //! Run / worker / content ID types.
   use serde::{Deserialize, Serialize};
   use std::fmt;
   use std::str::FromStr;
   use ulid::Ulid;

   /// ULID-based identifier for a single run.
   #[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize)]
   #[serde(transparent)]
   pub struct RunId(pub Ulid);

   impl fmt::Display for RunId { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { self.0.fmt(f) } }
   impl FromStr for RunId { type Err = ulid::DecodeError; fn from_str(s: &str) -> Result<Self, Self::Err> { Ok(Self(s.parse()?)) } }

   // Same shape for WorkerId.

   /// blake3 content hash; equality implies content equality.
   #[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize)]
   pub struct ContentId(pub [u8; 32]);
   impl ContentId {
       /// Compute the blake3 hash of `data`.
       pub fn of(data: &[u8]) -> Self { Self(*blake3::hash(data).as_bytes()) }
   }
   impl fmt::Display for ContentId {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           for b in &self.0 { write!(f, "{b:02x}")?; }
           Ok(())
       }
   }
   impl FromStr for ContentId {
       type Err = String;
       fn from_str(s: &str) -> Result<Self, Self::Err> {
           if s.len() != 64 { return Err(format!("ContentId: expected 64 hex chars, got {}", s.len())); }
           let mut out = [0u8; 32];
           for i in 0..32 {
               out[i] = u8::from_str_radix(&s[i*2..i*2+2], 16).map_err(|e| e.to_string())?;
           }
           Ok(Self(out))
       }
   }
   ```

3. **GREEN — implement `crates/rollout-core/src/errors.rs`** per RESEARCH.md §Pattern 3 — EXACT shape from `<interfaces>` above. Do NOT add `#[derive(Serialize)]` on any error type (Anti-Pattern 4). Crate-level `//!` plus one-line `///` on every `pub` enum / variant required for DOCS-03 (AGENTS.md §9.3).

4. **GREEN — update `crates/rollout-core/src/lib.rs`** to declare and re-export. The crate-level `//!` is REQUIRED for the rustdoc gate (AGENTS.md §9.3):
   ```rust
   //! Core trait surface, types, errors, and config schema for the rollout framework.
   //! See AGENTS.md §2 (principles) and docs/specs/01-core-runtime.md.
   #![forbid(unsafe_code)]
   pub mod ids;
   pub mod errors;
   pub use ids::{RunId, WorkerId, ContentId};
   pub use errors::{CoreError, RecoverableError, FatalError, RetryHint};
   ```

5. Confirm GREEN: `cd /Users/ashutosh/personal/rollout && cargo test -p rollout-core --tests --no-fail-fast 2>&1 | tail -30` — id_types and error_taxonomy tests all pass.

6. Confirm DOCS-03 gate passes:
   ```bash
   cd /Users/ashutosh/personal/rollout && \
     RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" \
     cargo doc -p rollout-core --no-deps --all-features
   ```
   Must exit 0.
  </action>
  <acceptance_criteria>
    - `test -f /Users/ashutosh/personal/rollout/crates/rollout-core/src/ids.rs`
    - `test -f /Users/ashutosh/personal/rollout/crates/rollout-core/src/errors.rs`
    - `test -f /Users/ashutosh/personal/rollout/crates/rollout-core/tests/id_types.rs`
    - `test -f /Users/ashutosh/personal/rollout/crates/rollout-core/tests/error_taxonomy.rs`
    - `head -1 /Users/ashutosh/personal/rollout/crates/rollout-core/src/lib.rs | grep -q '^//!'`
    - `grep -q 'pub struct RunId' /Users/ashutosh/personal/rollout/crates/rollout-core/src/ids.rs`
    - `grep -q 'pub struct WorkerId' /Users/ashutosh/personal/rollout/crates/rollout-core/src/ids.rs`
    - `grep -q 'pub struct ContentId' /Users/ashutosh/personal/rollout/crates/rollout-core/src/ids.rs`
    - `grep -q 'pub enum CoreError' /Users/ashutosh/personal/rollout/crates/rollout-core/src/errors.rs`
    - `grep -q 'pub enum RecoverableError' /Users/ashutosh/personal/rollout/crates/rollout-core/src/errors.rs`
    - `grep -q 'pub enum FatalError' /Users/ashutosh/personal/rollout/crates/rollout-core/src/errors.rs`
    - `grep -q 'pub enum RetryHint' /Users/ashutosh/personal/rollout/crates/rollout-core/src/errors.rs`
    - `! grep -q 'derive.*Serialize.*CoreError\|CoreError.*derive.*Serialize' /Users/ashutosh/personal/rollout/crates/rollout-core/src/errors.rs` (no Serialize on CoreError — Anti-Pattern 4)
    - `grep -q '#\[from\]' /Users/ashutosh/personal/rollout/crates/rollout-core/src/errors.rs`
    - `cd /Users/ashutosh/personal/rollout && cargo test -p rollout-core --test id_types 2>&1 | grep -qE 'test result: ok\. [5-9]+ passed'`
    - `cd /Users/ashutosh/personal/rollout && cargo test -p rollout-core --test error_taxonomy 2>&1 | grep -qE 'test result: ok\. [4-9]+ passed'`
  </acceptance_criteria>
  <verify>
    <automated>cd /Users/ashutosh/personal/rollout && cargo test -p rollout-core --test id_types && cargo test -p rollout-core --test error_taxonomy</automated>
  </verify>
  <done>CORE-03 (error taxonomy) and CORE-05 (IDs) implemented with passing Wave 0 tests; lib.rs carries the crate-level `//!` required by DOCS-03. Maps to 01-VALIDATION.md rows `id_roundtrip`, `content_id_determinism`, `id_serde`, `error_taxonomy`, `error_from_propagation`.</done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: All 19 traits + trait_surface compile test (CORE-01)</name>
  <files>crates/rollout-core/src/traits/mod.rs, crates/rollout-core/src/traits/algorithm.rs, crates/rollout-core/src/traits/worker.rs, crates/rollout-core/src/traits/plugin.rs, crates/rollout-core/src/traits/harness.rs, crates/rollout-core/src/traits/backend.rs, crates/rollout-core/src/traits/storage.rs, crates/rollout-core/src/traits/cloud.rs, crates/rollout-core/src/traits/clock.rs, crates/rollout-core/src/lib.rs, crates/rollout-core/tests/trait_surface.rs</files>
  <read_first>
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-RESEARCH.md (§Pattern 2 async-trait + §Architecture Patterns → Recommended rollout-core Structure)
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-CONTEXT.md (D-CRATE-02 — 19 trait names + module placement)
    - /Users/ashutosh/personal/rollout/AGENTS.md §9.3 (rustdoc gate — every `pub` item needs `///`)
    - /Users/ashutosh/personal/rollout/docs/specs/01-core-runtime.md §2 (Worker, Coordinator, Scheduler shapes)
    - /Users/ashutosh/personal/rollout/docs/specs/02-algorithms.md (PolicyAlgorithm) — read if file exists
    - /Users/ashutosh/personal/rollout/docs/specs/03-plugin-system.md (Plugin, PluginHost) — read if file exists
    - /Users/ashutosh/personal/rollout/docs/specs/04-storage-snapshots.md (Storage, StorageTxn, Snapshotter) — read if file exists
    - /Users/ashutosh/personal/rollout/docs/specs/05-distribution.md (Queue, Scheduler) — read if file exists
    - /Users/ashutosh/personal/rollout/docs/specs/06-cloud-layer.md (ObjectStore, SecretStore, ComputeHint) — read if file exists
    - /Users/ashutosh/personal/rollout/docs/specs/07-harnesses.md (EnvHarness, ToolHarness, EvalHarness) — read if file exists
    - existing crates/rollout-core/src/lib.rs from Task 1
  </read_first>
  <behavior>
    Tests in tests/trait_surface.rs (compile-only, no runtime behaviour):
    - For each of the 19 traits, a `fn _assert_<trait>_object_safe()` (or compile-time use) that constructs `let _: Option<std::sync::Arc<dyn rollout_core::TraitName>> = None;` for the I/O traits (Worker, Coordinator, Scheduler, Plugin, PluginHost, EnvHarness, ToolHarness, EvalHarness, RewardModel, InferenceBackend, Storage, StorageTxn, Queue, ObjectStore, SecretStore, ComputeHint, Snapshotter, PolicyAlgorithm — 18 dyn-compatible). For Clock (the one sync trait), also construct `Arc<dyn Clock>` to assert object-safety.
    - For Send + Sync: `fn _assert_send_sync<T: Send + Sync + ?Sized>() {} let _ = _assert_send_sync::<dyn rollout_core::Worker>;` — repeat for the 19 traits.
    - One `#[test] fn trait_surface_counts_19() { /* doc-only assertion */ }` that does nothing but exists so `cargo test --test trait_surface` reports a passing test (rather than zero tests).
  </behavior>
  <action>
0. **Pre-step — inspect `docs/specs/01-core-runtime.md` §2** for the `WorkerContext<'_>` and `DrainReason` types referenced in the `Worker` trait. **Decision (Claude's Discretion):** introduce stub aliases in `src/traits/worker.rs` so the trait surface compiles with the spec's intended signatures:

   ```rust
   /// Phase 1 stub; full type lands in Phase 2 (runtime substrate).
   pub struct WorkerContext;

   /// Phase 1 stub; full type lands in Phase 2 (runtime substrate).
   pub enum DrainReason { Cancelled, SnapshotRequest, Shutdown }
   ```

   Document the choice in this plan's SUMMARY: "Phase 1 introduces minimal stub types for WorkerContext + DrainReason to keep Worker trait spec-shaped; full types arrive in Phase 2 (runtime substrate)." The `Worker` trait body below uses `&WorkerContext<'_>` and `DrainReason` against these stubs so the spec signature is preserved.

1. **RED — write `crates/rollout-core/tests/trait_surface.rs` first**, importing all 19 traits via `use rollout_core::*;` and asserting Arc<dyn Trait> + Send + Sync as described in `<behavior>`. Confirm compile error (traits not yet declared).

2. **GREEN — implement each trait module** with SKELETAL trait definitions (1–3 methods each is sufficient for Phase 1 — the surface, not the impls; CONTEXT.md §specifics: "All 19 named traits must compile in `rollout-core` after Phase 1, even if some are skeletal").

   Use `#[async_trait::async_trait]` on all I/O traits (RESEARCH.md §Pattern 2 — required for `dyn Trait`). EXCEPTION: `Clock` is sync.

   **DOCS-03 rule (AGENTS.md §9.3):** every `pub trait` and every `pub` method needs at least a one-line `///` doc comment. Each module file gets a `//!` header line.

   Module → trait mapping (FROM D-CRATE-02 + RESEARCH.md):
   - `algorithm.rs` → `PolicyAlgorithm`
   - `worker.rs` → `Worker`, `Coordinator`, `Scheduler`
   - `plugin.rs` → `Plugin`, `PluginHost`
   - `harness.rs` → `EnvHarness`, `ToolHarness`, `EvalHarness`, `RewardModel`
   - `backend.rs` → `InferenceBackend`
   - `storage.rs` → `Storage`, `StorageTxn`, `Snapshotter`
   - `cloud.rs` → `ObjectStore`, `SecretStore`, `ComputeHint`, `Queue`
   - `clock.rs` → `Clock`

   Example minimum body (worker.rs):
   ```rust
   //! Worker / coordinator / scheduler traits.
   use async_trait::async_trait;
   use crate::{CoreError, RunId, WorkerId};

   /// A unit of work that produces rollouts or processes them.
   #[async_trait]
   pub trait Worker: Send + Sync {
       /// Stable identity for routing and observability.
       fn id(&self) -> WorkerId;
       /// Drive the worker to completion.
       async fn run(&mut self) -> Result<(), CoreError>;
       /// Cooperative shutdown.
       async fn shutdown(&mut self) -> Result<(), CoreError>;
   }

   /// Run-wide control plane.
   #[async_trait]
   pub trait Coordinator: Send + Sync {
       /// Register a worker with this run.
       async fn register(&self, worker: WorkerId) -> Result<(), CoreError>;
   }

   /// Assigns work to workers.
   #[async_trait]
   pub trait Scheduler: Send + Sync {
       /// Assign a run to the next available slot.
       async fn assign(&self, run: RunId) -> Result<(), CoreError>;
   }
   ```

   Clock (sync — RESEARCH.md §Pattern 2 exception):
   ```rust
   //! Clock trait (sync; injectable for deterministic tests).
   /// Monotonic clock.
   pub trait Clock: Send + Sync {
       /// Monotonic nanoseconds since an unspecified epoch.
       fn now_nanos(&self) -> u128;
   }
   ```

   For each trait: `Send + Sync` bound; skeletal method count 1–3; method signatures return `Result<_, CoreError>`. Doc comment ≤ 1 line above each trait and each method per AGENTS.md §8 (`why`, not `what`).

3. **GREEN — write `crates/rollout-core/src/traits/mod.rs`** declaring each sub-module and re-exporting:
   ```rust
   //! All 19 trait modules.
   pub mod algorithm; pub mod worker; pub mod plugin; pub mod harness;
   pub mod backend;  pub mod storage; pub mod cloud;  pub mod clock;
   pub use algorithm::PolicyAlgorithm;
   pub use worker::{Worker, Coordinator, Scheduler};
   pub use plugin::{Plugin, PluginHost};
   pub use harness::{EnvHarness, ToolHarness, EvalHarness, RewardModel};
   pub use backend::InferenceBackend;
   pub use storage::{Storage, StorageTxn, Snapshotter};
   pub use cloud::{ObjectStore, SecretStore, ComputeHint, Queue};
   pub use clock::Clock;
   ```

4. **GREEN — update `crates/rollout-core/src/lib.rs`** to add `pub mod traits;` and `pub use traits::*;`.

5. Confirm GREEN: `cd /Users/ashutosh/personal/rollout && cargo test -p rollout-core --test trait_surface 2>&1 | tail -10` shows `test result: ok`.

6. Run clippy: `cd /Users/ashutosh/personal/rollout && cargo clippy -p rollout-core --all-targets -- -D warnings`. Fix any lint warnings (likely `missing_docs` since lib.rs forbids it via workspace lint `missing_docs = "warn"` — add `///` doc comments where needed, but DO NOT add multi-paragraph docstrings (per AGENTS.md §8 and user CLAUDE.md "Comments: be succinct, one short line max").

7. Confirm DOCS-03 gate passes after traits land:
   ```bash
   cd /Users/ashutosh/personal/rollout && \
     RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" \
     cargo doc -p rollout-core --no-deps --all-features
   ```
   Must exit 0.
  </action>
  <acceptance_criteria>
    - All 8 trait module files exist under `/Users/ashutosh/personal/rollout/crates/rollout-core/src/traits/`
    - `grep -q 'pub trait PolicyAlgorithm' /Users/ashutosh/personal/rollout/crates/rollout-core/src/traits/algorithm.rs`
    - `grep -qE 'pub trait Worker(\s*:|\s*\{)' /Users/ashutosh/personal/rollout/crates/rollout-core/src/traits/worker.rs`
    - `grep -qE 'pub trait Coordinator(\s*:|\s*\{)' /Users/ashutosh/personal/rollout/crates/rollout-core/src/traits/worker.rs`
    - `grep -qE 'pub trait Scheduler(\s*:|\s*\{)' /Users/ashutosh/personal/rollout/crates/rollout-core/src/traits/worker.rs`
    - `grep -qE 'pub trait Plugin(\s*:|\s*\{)' /Users/ashutosh/personal/rollout/crates/rollout-core/src/traits/plugin.rs`
    - `grep -qE 'pub trait PluginHost(\s*:|\s*\{)' /Users/ashutosh/personal/rollout/crates/rollout-core/src/traits/plugin.rs`
    - `grep -qE 'pub trait EnvHarness(\s*:|\s*\{)' /Users/ashutosh/personal/rollout/crates/rollout-core/src/traits/harness.rs`
    - `grep -qE 'pub trait ToolHarness(\s*:|\s*\{)' /Users/ashutosh/personal/rollout/crates/rollout-core/src/traits/harness.rs`
    - `grep -qE 'pub trait EvalHarness(\s*:|\s*\{)' /Users/ashutosh/personal/rollout/crates/rollout-core/src/traits/harness.rs`
    - `grep -qE 'pub trait RewardModel(\s*:|\s*\{)' /Users/ashutosh/personal/rollout/crates/rollout-core/src/traits/harness.rs`
    - `grep -qE 'pub trait InferenceBackend(\s*:|\s*\{)' /Users/ashutosh/personal/rollout/crates/rollout-core/src/traits/backend.rs`
    - `grep -qE 'pub trait Storage(\s*:|\s*\{)' /Users/ashutosh/personal/rollout/crates/rollout-core/src/traits/storage.rs`
    - `grep -qE 'pub trait StorageTxn(\s*:|\s*\{)' /Users/ashutosh/personal/rollout/crates/rollout-core/src/traits/storage.rs`
    - `grep -qE 'pub trait Snapshotter(\s*:|\s*\{)' /Users/ashutosh/personal/rollout/crates/rollout-core/src/traits/storage.rs`
    - `grep -qE 'pub trait ObjectStore(\s*:|\s*\{)' /Users/ashutosh/personal/rollout/crates/rollout-core/src/traits/cloud.rs`
    - `grep -qE 'pub trait SecretStore(\s*:|\s*\{)' /Users/ashutosh/personal/rollout/crates/rollout-core/src/traits/cloud.rs`
    - `grep -qE 'pub trait ComputeHint(\s*:|\s*\{)' /Users/ashutosh/personal/rollout/crates/rollout-core/src/traits/cloud.rs`
    - `grep -qE 'pub trait Queue(\s*:|\s*\{)' /Users/ashutosh/personal/rollout/crates/rollout-core/src/traits/cloud.rs`
    - `grep -qE 'pub trait Clock(\s*:|\s*\{)' /Users/ashutosh/personal/rollout/crates/rollout-core/src/traits/clock.rs`
    - `grep -c 'pub trait' /Users/ashutosh/personal/rollout/crates/rollout-core/src/traits/*.rs | awk -F: '{s+=$2} END {exit !(s==19)}'` (exactly 19 `pub trait` declarations across the 8 files)
    - `grep -q 'async_trait' /Users/ashutosh/personal/rollout/crates/rollout-core/src/traits/worker.rs`
    - `cd /Users/ashutosh/personal/rollout && cargo test -p rollout-core --test trait_surface 2>&1 | grep -q 'test result: ok'`
    - `cd /Users/ashutosh/personal/rollout && cargo clippy -p rollout-core --all-targets -- -D warnings` exits 0
  </acceptance_criteria>
  <verify>
    <automated>cd /Users/ashutosh/personal/rollout && cargo test -p rollout-core --test trait_surface && cargo clippy -p rollout-core --all-targets -- -D warnings && [ $(grep -c 'pub trait' crates/rollout-core/src/traits/*.rs | awk -F: '{s+=$2} END {print s}') -eq 19 ]</automated>
  </verify>
  <done>CORE-01 trait surface complete: all 19 traits declared, exported, object-safe where required, compiling under clippy -D warnings AND under the §9.3 RUSTDOCFLAGS. Maps to 01-VALIDATION.md row `trait_surface`.</done>
</task>

<task type="auto" tdd="true">
  <name>Task 3: RunConfig type tree with JsonSchema + deny_unknown_fields</name>
  <files>crates/rollout-core/src/config/mod.rs, crates/rollout-core/src/config/defaults.rs, crates/rollout-core/src/lib.rs</files>
  <read_first>
    - /Users/ashutosh/personal/rollout/docs/specs/11-config-schema.md §1–§8 (config rules, anatomy, tagged unions, defaults policy)
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-RESEARCH.md (§Pattern 1 schemars derive + §Open Questions Q2 schema_version range)
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-CONTEXT.md (D-CFG-01)
    - /Users/ashutosh/personal/rollout/AGENTS.md §9.3 (every `pub` item gets a `///`)
    - /Users/ashutosh/personal/rollout/docs/specs/01-core-runtime.md §7 (RuntimeSettings shape)
    - existing /Users/ashutosh/personal/rollout/crates/rollout-core/src/lib.rs
  </read_first>
  <behavior>
    A type tree such that `schemars::schema_for!(RunConfig)` produces a JSON Schema 2020-12 document that:
    - Has `$defs/RunConfig` (or top-level) with `additionalProperties: false`
    - Has a required `schema_version: integer` (constraint `1 <= x <= 1` in Phase 1)
    - References sub-configs as `$ref` entries
    The drift-test in Plan 04 will validate determinism. This task only needs to:
    - Compile and pass `cargo test -p rollout-core` (no new dedicated config-test file required in this task — RunConfig presence is verified by grep + the Plan 04 schema-gen pipeline)
  </behavior>
  <action>
1. Implement `/Users/ashutosh/personal/rollout/crates/rollout-core/src/config/defaults.rs` with one pure helper function so the `defaults` module exists (D-CFG-01 + spec 11 §8):
   ```rust
   //! Default values for config fields. Pure functions only.
   /// Default schema version for new configs.
   pub fn schema_version() -> u32 { 1 }
   ```

2. Implement `/Users/ashutosh/personal/rollout/crates/rollout-core/src/config/mod.rs` with the minimum tree needed for Phase 1 schema-gen — keep it small but real (spec 11 §4). Add `///` doc comments on every `pub` struct / enum / variant / field per AGENTS.md §9.3:
   ```rust
   //! Run configuration. Single source of truth for the config schema.
   use schemars::JsonSchema;
   use serde::{Deserialize, Serialize};

   pub mod defaults;

   /// Top-level run configuration.
   #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
   #[serde(deny_unknown_fields)]
   pub struct RunConfig {
       /// Schema version. The framework refuses configs with version > 1.
       #[serde(default = "defaults::schema_version")]
       #[schemars(range(min = 1, max = 1))]
       pub schema_version: u32,

       /// Free-form metadata about the run; persisted in storage but not used by the framework.
       #[serde(default)]
       pub run: RunMetadata,

       /// Storage backend selection.
       pub storage: StorageConfig,
       /// Algorithm + its settings.
       pub algorithm: AlgorithmConfig,
   }

   /// Free-form run metadata.
   #[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
   #[serde(deny_unknown_fields)]
   pub struct RunMetadata {
       /// Human-readable run name.
       #[serde(default)]
       pub name: Option<String>,
       /// Free-form tags.
       #[serde(default)]
       pub tags: Vec<String>,
   }

   /// Storage backend selection.
   #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
   #[serde(deny_unknown_fields, tag = "backend", rename_all = "snake_case")]
   pub enum StorageConfig {
       /// Embedded local KV (sled/redb — choice in Phase 2).
       Embedded {
           /// Filesystem path for the embedded DB.
           path: String,
       },
       /// Postgres URL (deferred to Phase 4 / TRAIN-04).
       Postgres {
           /// Postgres connection URL.
           url: String,
       },
   }

   /// Algorithm selection.
   #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
   #[serde(deny_unknown_fields, tag = "kind", rename_all = "snake_case")]
   pub enum AlgorithmConfig {
       /// Supervised fine-tuning.
       Sft(SftSettings),
       /// Proximal policy optimization.
       Ppo(PpoSettings),
   }

   /// SFT settings.
   #[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
   #[serde(deny_unknown_fields)]
   pub struct SftSettings {
       /// Learning rate override.
       #[serde(default)]
       pub learning_rate: Option<f64>,
   }

   /// PPO settings.
   #[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
   #[serde(deny_unknown_fields)]
   pub struct PpoSettings {
       /// Initial KL coefficient.
       #[serde(default)]
       pub kl_coef_init: Option<f64>,
   }
   ```

   Rationale: keep the type tree small — Phase 1 only needs to prove the pipeline works (CONTEXT.md §specifics: "Python stub generation can be lightweight in Phase 1"). Future phases extend `RunConfig`.

3. Update `/Users/ashutosh/personal/rollout/crates/rollout-core/src/lib.rs`:
   ```rust
   pub mod config;
   pub use config::RunConfig;
   ```

4. Verify schemars `range(min = 1, max = 1)` attribute compiles under schemars 1.2.1. If RESEARCH.md Open Question Q2 turns out to require a different syntax (e.g., `#[schemars(schema_with = ...)]`), fall back to: drop the attribute and add an `impl RunConfig { pub fn validate_schema_version(&self) -> Result<(), FatalError> { ... } }` method instead. **Do not block the task on this** — schemars 1.2.1 supports `range` per the schemars 1.x changelog; if it fails to compile, document in the SUMMARY and use the fallback.

5. Confirm:
   ```bash
   cd /Users/ashutosh/personal/rollout && cargo test -p rollout-core
   cd /Users/ashutosh/personal/rollout && cargo clippy -p rollout-core --all-targets -- -D warnings
   cd /Users/ashutosh/personal/rollout && \
     RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" \
     cargo doc -p rollout-core --no-deps --all-features
   ```
   All three must pass.
  </action>
  <acceptance_criteria>
    - `test -f /Users/ashutosh/personal/rollout/crates/rollout-core/src/config/mod.rs`
    - `test -f /Users/ashutosh/personal/rollout/crates/rollout-core/src/config/defaults.rs`
    - `grep -q 'pub struct RunConfig' /Users/ashutosh/personal/rollout/crates/rollout-core/src/config/mod.rs`
    - `grep -q 'JsonSchema' /Users/ashutosh/personal/rollout/crates/rollout-core/src/config/mod.rs`
    - `grep -q 'deny_unknown_fields' /Users/ashutosh/personal/rollout/crates/rollout-core/src/config/mod.rs`
    - `grep -q 'schema_version' /Users/ashutosh/personal/rollout/crates/rollout-core/src/config/mod.rs`
    - `grep -q 'tag = "kind"' /Users/ashutosh/personal/rollout/crates/rollout-core/src/config/mod.rs`
    - `grep -q 'fn schema_version' /Users/ashutosh/personal/rollout/crates/rollout-core/src/config/defaults.rs`
    - `grep -q 'pub use config::RunConfig' /Users/ashutosh/personal/rollout/crates/rollout-core/src/lib.rs`
    - `cd /Users/ashutosh/personal/rollout && cargo test -p rollout-core 2>&1 | grep -qE 'test result: ok'`
    - `cd /Users/ashutosh/personal/rollout && cargo clippy -p rollout-core --all-targets -- -D warnings` exits 0
    - `cd /Users/ashutosh/personal/rollout && cargo build -p rollout-core` exits 0 (downstream plans depend on this)
    - `cd /Users/ashutosh/personal/rollout && RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" cargo doc -p rollout-core --no-deps --all-features` exits 0 (DOCS-03 gate)
  </acceptance_criteria>
  <verify>
    <automated>cd /Users/ashutosh/personal/rollout && cargo test -p rollout-core && cargo clippy -p rollout-core --all-targets -- -D warnings && RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" cargo doc -p rollout-core --no-deps --all-features</automated>
  </verify>
  <done>CORE-04 foundation in place — `RunConfig` is the single-source-of-truth type that Plan 04's xtask schema-gen will consume. Drift-test wiring lands in Plan 04. DOCS-03 gate passes for `rollout-core`. Maps to 01-VALIDATION.md row CORE-04 (schema generation — pipeline implementation Plan 04, but the source type lives here).</done>
</task>

</tasks>

<verification>
- `cd /Users/ashutosh/personal/rollout && cargo test -p rollout-core` passes (all unit + integration tests)
- `cd /Users/ashutosh/personal/rollout && cargo clippy -p rollout-core --all-targets -- -D warnings` exits 0
- `cd /Users/ashutosh/personal/rollout && cargo build --workspace` still passes (no breakage in cli/xtask)
- 19 `pub trait` declarations across `src/traits/*.rs` (counted via grep)
- No `Serialize` derive on `CoreError` (Anti-Pattern 4)
- `cargo doc -p rollout-core --no-deps --all-features` passes under §9.3 RUSTDOCFLAGS (DOCS-03 gate)
</verification>

<success_criteria>
- All 19 traits public from `rollout-core` (CORE-01).
- `CoreError = Recoverable | Fatal` with `RetryHint` and `#[from]` propagation (CORE-03).
- `RunId`, `WorkerId`, `ContentId` with Serialize + Deserialize + Display + FromStr; blake3-based content-addressing (CORE-05).
- `RunConfig` with `JsonSchema + Serialize + Deserialize + deny_unknown_fields` (sets up CORE-04 for Plan 04).
- Wave 0 test files (trait_surface, error_taxonomy, id_types) green.
- Crate-level `//!` and one-line `///` on every `pub` item — DOCS-03 gate passes for `rollout-core`.
</success_criteria>

<output>
After completion, create `.planning/phases/01-core-foundations/01-PLAN-03-SUMMARY.md` documenting:
- Final trait module structure
- Any deviations from research recommendations (e.g., fallback for schemars `range` attribute if it didn't compile)
- Decisions made under Claude's Discretion (e.g., whether `Clock` was sync — yes, per RESEARCH.md §Pattern 2 exception; module layout)
- Test file inventory + pass counts
- Confirmation that the §9.3 rustdoc gate passes locally for `rollout-core`
</output>
