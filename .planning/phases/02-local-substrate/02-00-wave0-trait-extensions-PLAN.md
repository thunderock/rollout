---
phase: 02-local-substrate
plan: 00
type: execute
wave: 1
depends_on: []
files_modified:
  - crates/rollout-core/src/traits/storage.rs
  - crates/rollout-core/src/traits/plugin.rs
  - crates/rollout-core/src/traits/worker.rs
  - crates/rollout-core/src/traits/cloud.rs
  - crates/rollout-core/src/traits/observability.rs
  - crates/rollout-core/src/traits/mod.rs
  - crates/rollout-core/src/lib.rs
  - crates/rollout-core/Cargo.toml
  - crates/rollout-core/tests/dependency_direction.rs
  - crates/rollout-core/tests/trait_surface.rs
  - Cargo.toml
  - deny.toml
  - scripts/preflight.sh
  - docs/specs/01-core-runtime.md
  - docs/specs/03-plugin-system.md
  - docs/specs/04-storage-snapshots.md
  - docs/specs/06-cloud-layer.md
  - docs/book/src/SUMMARY.md
  - docs/book/src/substrate/index.md
  - .gitignore
autonomous: true
requirements: [SUBSTR-01, SUBSTR-02, SUBSTR-03, SUBSTR-04, DOCS-01, DOCS-02, DOCS-03]
must_haves:
  truths:
    - "Phase-2 crates can implement the Storage/StorageTxn surface (get/get_many/scan/watch/put/delete/cas/abort) without further trait churn."
    - "Phase-2 crates can implement the PluginHost surface (load/call/reload/unload) with PluginManifest + PluginHandle types."
    - "Phase-2 crates can implement Coordinator::heartbeat(Heartbeat) and the WorkerState enum."
    - "The workspace knows about all six Phase-2 crates and Phase-2 dependency pins are licence-passing under cargo deny."
    - "Dep-direction lint forbids rollout-transport → rollout-cloud-* and rollout-plugin-host → rollout-transport."
    - "scripts/preflight.sh detects missing protoc / python3 ≥ 3.11 and bails with a clear message."
    - "rollout-core defines an `EventEmitter` trait (spec 09 §2 event shape) so binaries can plug a stdout-JSON sink per D-OBSERVE-01 in plan 02-06."
  artifacts:
    - path: crates/rollout-core/src/traits/storage.rs
      provides: "Storage + StorageTxn extended surface, StorageKey/KeyRange/StorageEvent types"
      contains: "fn get<"
    - path: crates/rollout-core/src/traits/plugin.rs
      provides: "PluginHost::call/reload/unload + PluginManifest/PluginHandle/PluginDependencies"
      contains: "PluginHandle"
    - path: crates/rollout-core/src/traits/worker.rs
      provides: "Coordinator::heartbeat + Heartbeat + WorkerState"
      contains: "fn heartbeat"
    - path: crates/rollout-core/src/traits/cloud.rs
      provides: "ObjectStore::put returning ContentId, ComputeHint full surface, Queue ack/nack"
      contains: "ContentId"
    - path: crates/rollout-core/src/traits/observability.rs
      provides: "EventEmitter trait + Event / EventKind / Level types per spec 09 §2"
      contains: "pub trait EventEmitter"
    - path: Cargo.toml
      provides: "Six new workspace members + Phase-2 workspace.dependencies pins"
      contains: "rollout-proto"
    - path: scripts/preflight.sh
      provides: "Preflight check for protoc + python3 ≥ 3.11"
    - path: docs/book/src/substrate/index.md
      provides: "Substrate section landing page in mdBook"
  key_links:
    - from: crates/rollout-core/src/lib.rs
      to: "traits::*"
      via: "pub use"
      pattern: "PluginHandle|StorageKey|Heartbeat|WorkerState"
    - from: crates/rollout-core/tests/dependency_direction.rs
      to: "rollout-transport / rollout-cloud-* / rollout-plugin-host"
      via: "violation() rules"
      pattern: "rollout-transport"
    - from: Cargo.toml
      to: "six new crates"
      via: "[workspace] members"
      pattern: "rollout-(proto|storage|cloud-local|transport|plugin-host|coordinator)"
---

<objective>
Land all Wave-0 prerequisites in a single atomic plan so every downstream wave can compile without trait churn. This plan:

1. Extends the five `rollout-core` trait modules (`storage`, `plugin`, `worker`, `cloud`) to the spec surface that Phase 2 actually consumes.
2. Registers the six new Phase-2 crates as empty stubs in the workspace `Cargo.toml`.
3. Pins all Phase-2 dependency versions under `[workspace.dependencies]`.
4. Extends `dependency_direction.rs` with the two new invariants Phase 2 introduces.
5. Ships `scripts/preflight.sh` so contributors get a clear error when `protoc` / `python3 ≥ 3.11` is missing.
6. Updates the four affected specs (01/03/04/06) in the same PR per AGENTS.md §4 + §9.2.
7. Adds `docs/book/src/substrate/index.md` landing page so the mdBook section structure is in place.

Purpose: Research §"Critical Finding: Trait Surface Drift" — the Phase-1 trait stubs do not cover what spec 04/03/01 actually require. Without this Wave-0 atomic extension, every downstream crate (Storage impl, transport, plugin host, coordinator) would either drift from spec or accumulate trait-extension churn across multiple PRs.

Output: A workspace that **still compiles cleanly with no impls** (Phase 1 binary `rollout-cli` keeps working; new crates exist as empty `lib.rs` stubs that downstream plans flesh out), but with all the type surface Wave 1+ needs.
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
@$HOME/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/STATE.md
@.planning/ROADMAP.md
@.planning/REQUIREMENTS.md
@.planning/phases/02-local-substrate/02-CONTEXT.md
@.planning/phases/02-local-substrate/02-RESEARCH.md
@.planning/phases/02-local-substrate/02-VALIDATION.md
@AGENTS.md
@docs/specs/01-core-runtime.md
@docs/specs/03-plugin-system.md
@docs/specs/04-storage-snapshots.md
@docs/specs/05-distribution.md
@docs/specs/06-cloud-layer.md
@docs/specs/10-component-split.md
@docs/specs/11-config-schema.md
@crates/rollout-core/src/traits/storage.rs
@crates/rollout-core/src/traits/plugin.rs
@crates/rollout-core/src/traits/worker.rs
@crates/rollout-core/src/traits/cloud.rs
@crates/rollout-core/src/traits/mod.rs
@crates/rollout-core/src/lib.rs
@crates/rollout-core/src/ids.rs
@crates/rollout-core/src/errors.rs
@crates/rollout-core/tests/dependency_direction.rs
@crates/rollout-core/tests/trait_surface.rs
@Cargo.toml
@deny.toml

<interfaces>
<!-- Existing Phase-1 surface this plan extends. Executor MUST keep these stable. -->

From crates/rollout-core/src/ids.rs:
```rust
pub struct RunId(pub Ulid);
pub struct WorkerId(pub Ulid);
pub struct ContentId(pub [u8; 32]);
```

From crates/rollout-core/src/errors.rs (Phase 1):
```rust
pub enum CoreError {
  Recoverable(RecoverableError),
  Fatal(FatalError),
}
pub enum FatalError { ConfigInvalid(String), SchemaViolation(String), PluginContract(String), Internal(String) }
pub enum RecoverableError { Throttled { retry: RetryHint }, Transient { retry: RetryHint }, Preempted }
```

Existing stub surface (this plan extends, does not replace):
```rust
// storage.rs
pub trait Storage: Send + Sync {
  async fn begin(&self) -> Result<Box<dyn StorageTxn>, CoreError>;
  async fn ping(&self) -> Result<(), CoreError>;
}
pub trait StorageTxn: Send + Sync {
  async fn commit(self: Box<Self>) -> Result<(), CoreError>;
}

// plugin.rs
pub trait Plugin: Send + Sync { fn name(&self) -> &str; async fn validate(&self) -> Result<(), CoreError>; }
pub trait PluginHost: Send + Sync { async fn load(&self, name: &str) -> Result<(), CoreError>; }

// worker.rs
pub struct WorkerContext;
pub enum DrainReason { Cancelled, SnapshotRequest, Shutdown }
pub trait Worker: Send + Sync {
  fn id(&self) -> WorkerId;
  async fn run(&mut self, ctx: &WorkerContext) -> Result<(), CoreError>;
  async fn drain(&mut self, ctx: &WorkerContext, reason: DrainReason) -> Result<(), CoreError>;
  async fn shutdown(&mut self) -> Result<(), CoreError>;
}
pub trait Coordinator: Send + Sync {
  async fn register(&self, worker: WorkerId) -> Result<(), CoreError>;
  async fn deregister(&self, worker: WorkerId) -> Result<(), CoreError>;
}

// cloud.rs
pub trait ObjectStore { async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), CoreError>; async fn get(&self, key: &str) -> Result<Vec<u8>, CoreError>; }
pub trait SecretStore { async fn get(&self, name: &str) -> Result<String, CoreError>; }
pub trait ComputeHint { async fn instance_type(&self) -> Result<String, CoreError>; }
pub trait Queue { async fn enqueue(&self, payload: &[u8]) -> Result<(), CoreError>; async fn dequeue(&self) -> Result<Option<Vec<u8>>, CoreError>; }
```

Spec 04 §2 + RESEARCH "Wave 0 Gaps" target surface (this plan delivers):
```rust
// New types
pub struct StorageKey { pub namespace: smol_str::SmolStr, pub run_id: Option<RunId>, pub path: Vec<smol_str::SmolStr> }
pub struct KeyRange { pub prefix: StorageKey, pub limit: Option<usize> }
pub enum StorageEvent { Put { key: StorageKey }, Delete { key: StorageKey } }

// Extended Storage trait (object-safe; generics moved to free helpers OR boxed-bytes API)
pub trait Storage: Send + Sync {
  async fn begin(&self) -> Result<Box<dyn StorageTxn>, CoreError>;
  async fn get_bytes(&self, key: &StorageKey) -> Result<Option<Vec<u8>>, CoreError>;
  async fn get_many_bytes(&self, keys: &[StorageKey]) -> Result<Vec<Option<Vec<u8>>>, CoreError>;
  async fn scan_bytes(&self, range: KeyRange) -> Result<Vec<(StorageKey, Vec<u8>)>, CoreError>;
  async fn watch(&self, prefix: StorageKey) -> Result<tokio::sync::broadcast::Receiver<StorageEvent>, CoreError>;
  async fn ping(&self) -> Result<(), CoreError>;
}
pub trait StorageTxn: Send + Sync {
  async fn put_bytes(&mut self, key: StorageKey, value: Vec<u8>) -> Result<(), CoreError>;
  async fn delete(&mut self, key: StorageKey) -> Result<(), CoreError>;
  async fn cas_bytes(&mut self, key: StorageKey, expected: Option<Vec<u8>>, new: Option<Vec<u8>>) -> Result<bool, CoreError>;
  async fn commit(self: Box<Self>) -> Result<(), CoreError>;
  async fn abort(self: Box<Self>) -> Result<(), CoreError>;
}
```

Spec 03 §4-§5 surface:
```rust
pub struct PluginManifest { pub name: String, pub version: String, pub kind: PluginKind, pub trait_id: String, pub mode: PluginMode, pub runtime: RuntimeHints, pub entry: EntrySpec, pub config_schema_path: Option<String>, pub network_allowlist: Vec<String> }
pub enum PluginKind { EnvHarness, ToolHarness, EvalHarness, RewardModel, InferenceBackend, Storage, Queue, ObjectStore, Custom(String) }
pub enum PluginMode { Pyo3, Sidecar, RustCdylib }
pub struct RuntimeHints { pub python_min: Option<String>, pub gpu: bool, pub memory_mib: u64 }
pub enum EntrySpec { Cdylib { path: String, symbol: String }, Pyo3 { module: String, factory: String }, Sidecar { command: Vec<String>, protocol: SidecarProtocol, socket_template: String } }
pub enum SidecarProtocol { GrpcUds, FramedJsonUds }
pub struct PluginDependencies { /* injected at init() */ }
pub struct PluginHandle { pub id: PluginId, pub manifest: PluginManifest /* opaque mode-specific state in impls */ }
pub struct PluginId(pub String);

pub trait PluginHost: Send + Sync {
  async fn load(&self, manifest: PluginManifest) -> Result<PluginHandle, CoreError>;
  async fn call(&self, handle: &PluginHandle, method: &str, payload: Vec<u8>) -> Result<Vec<u8>, CoreError>;
  async fn reload(&self, handle: &PluginHandle, reason: &str) -> Result<(), CoreError>;
  async fn unload(&self, handle: PluginHandle) -> Result<(), CoreError>;
}
```

Spec 01 §2-§4 + spec 05 §6 surface (worker/coordinator):
```rust
pub struct Heartbeat {
  pub worker_id: WorkerId,
  pub run_id: RunId,
  pub state: WorkerState,
  pub due_at: std::time::SystemTime,
}
pub enum WorkerState { Init, Ready, Running, Draining }

pub trait Coordinator: Send + Sync {
  async fn register(&self, worker: WorkerId) -> Result<(), CoreError>;
  async fn deregister(&self, worker: WorkerId) -> Result<(), CoreError>;
  async fn heartbeat(&self, hb: Heartbeat) -> Result<(), CoreError>;
}

pub trait Worker: Send + Sync {
  fn id(&self) -> WorkerId;
  async fn init(&mut self, ctx: &WorkerContext) -> Result<(), CoreError>;
  async fn ready(&mut self) -> Result<(), CoreError>;
  async fn run(&mut self, ctx: &WorkerContext) -> Result<(), CoreError>;
  async fn drain(&mut self, ctx: &WorkerContext, reason: DrainReason) -> Result<(), CoreError>;
  async fn shutdown(&mut self) -> Result<(), CoreError>;
}
```

Spec 06 §3 surface (cloud):
```rust
pub struct PutHint { pub expected_size: Option<u64>, pub content_type: Option<String> }
pub trait ObjectStore: Send + Sync {
  async fn put_bytes(&self, bytes: Vec<u8>, hint: PutHint) -> Result<ContentId, CoreError>;
  async fn get_bytes(&self, id: &ContentId) -> Result<Vec<u8>, CoreError>;
  async fn exists(&self, id: &ContentId) -> Result<bool, CoreError>;
}
pub trait SecretStore: Send + Sync {
  async fn get(&self, name: &str) -> Result<String, CoreError>;
  async fn put(&self, name: &str, value: &str) -> Result<(), CoreError>;  // local impl returns Fatal(ConfigInvalid)
}
pub struct GpuInfo { pub vendor: String, pub model: String, pub memory_mib: u64 }
pub struct ComputeInventory { pub cpu_count: u32, pub memory_mib: u64, pub gpus: Vec<GpuInfo>, pub instance_type: Option<String> }
pub trait ComputeHint: Send + Sync {
  async fn inventory(&self) -> Result<ComputeInventory, CoreError>;
  async fn preemption_signal(&self) -> Result<Option<std::time::Duration>, CoreError>;
}
pub trait Queue: Send + Sync {
  async fn enqueue(&self, payload: Vec<u8>) -> Result<QueueItemId, CoreError>;
  async fn dequeue(&self) -> Result<Option<(QueueItemId, Vec<u8>)>, CoreError>;
  async fn ack(&self, id: QueueItemId) -> Result<(), CoreError>;
  async fn nack(&self, id: QueueItemId) -> Result<(), CoreError>;
}
pub struct QueueItemId(pub ulid::Ulid);
```
</interfaces>
</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: Extend rollout-core traits + types + dep-direction fixtures</name>
  <files>
    crates/rollout-core/src/traits/storage.rs,
    crates/rollout-core/src/traits/plugin.rs,
    crates/rollout-core/src/traits/worker.rs,
    crates/rollout-core/src/traits/cloud.rs,
    crates/rollout-core/src/traits/observability.rs,
    crates/rollout-core/src/traits/mod.rs,
    crates/rollout-core/src/lib.rs,
    crates/rollout-core/Cargo.toml,
    crates/rollout-core/tests/trait_surface.rs,
    crates/rollout-core/tests/dependency_direction.rs
  </files>
  <read_first>
    - crates/rollout-core/src/traits/storage.rs (the file you are extending)
    - crates/rollout-core/src/traits/plugin.rs
    - crates/rollout-core/src/traits/worker.rs
    - crates/rollout-core/src/traits/cloud.rs
    - crates/rollout-core/src/traits/mod.rs (re-exports)
    - crates/rollout-core/src/lib.rs (top-level re-exports)
    - crates/rollout-core/src/ids.rs (RunId/WorkerId/ContentId — DO NOT modify)
    - crates/rollout-core/src/errors.rs (CoreError taxonomy — DO NOT modify)
    - crates/rollout-core/tests/trait_surface.rs (existing trait-shape tests)
    - crates/rollout-core/tests/dependency_direction.rs (extend its rules)
    - docs/specs/04-storage-snapshots.md §2 (Storage trait — authoritative shape)
    - docs/specs/03-plugin-system.md §4-§5 (Plugin / PluginHost)
    - docs/specs/01-core-runtime.md §2 (Worker / Coordinator lifecycle)
    - docs/specs/06-cloud-layer.md §3 (ObjectStore / Queue / SecretStore / ComputeHint)
    - docs/specs/05-distribution.md §6 (Heartbeat / deadline-based health)
    - docs/specs/09-observability.md §2 (Event / EventKind / EventEmitter — the trait shape this plan lands)
    - .planning/phases/02-local-substrate/02-RESEARCH.md §"Critical Finding: Trait Surface Drift" and §"Wave 0 Gaps"
    - .planning/phases/01-core-foundations/01-CONTEXT.md (the original trait stub decisions — Phase-2 supersedes "trait definitions are not modified" per RESEARCH override)
  </read_first>
  <behavior>
    RED first (extend `tests/trait_surface.rs`):
    - Test `storage_trait_has_extended_surface`: at compile time, asserts `Storage: { begin, get_bytes, get_many_bytes, scan_bytes, watch, ping }` and `StorageTxn: { put_bytes, delete, cas_bytes, commit, abort }`. Use `fn _assert_object_safe(_: &dyn Storage) {}` pattern.
    - Test `plugin_host_has_extended_surface`: asserts `PluginHost: { load, call, reload, unload }`.
    - Test `coordinator_has_heartbeat`: asserts `Coordinator: { register, deregister, heartbeat }` and `Worker: { id, init, ready, run, drain, shutdown }`.
    - Test `cloud_traits_match_spec_06`: asserts `ObjectStore: { put_bytes, get_bytes, exists }`, `SecretStore: { get, put }`, `ComputeHint: { inventory, preemption_signal }`, `Queue: { enqueue, dequeue, ack, nack }`.
    - Test `new_types_exist`: type-name compile checks for `StorageKey`, `KeyRange`, `StorageEvent`, `PluginManifest`, `PluginHandle`, `PluginKind`, `PluginMode`, `Heartbeat`, `WorkerState`, `PutHint`, `ComputeInventory`, `GpuInfo`, `QueueItemId`.
    - Test `event_emitter_trait_exists`: asserts `EventEmitter: { emit }` is object-safe (`fn _assert_object_safe(_: &dyn EventEmitter) {}`); type-name checks for `Event`, `EventKind`, `Level`.

    Then GREEN (extend the four trait files) so all tests compile and pass.
  </behavior>
  <action>
    **Step 1 — add `smol_str` workspace pin.** Edit root `Cargo.toml` to add under `[workspace.dependencies]`:
    ```toml
    smol_str = "0.3"
    ```
    Edit `crates/rollout-core/Cargo.toml` to depend on `smol_str.workspace = true`. Add `tokio = { workspace = true, features = ["sync"] }` as a dep so `tokio::sync::broadcast::Receiver` can be named in the trait signatures. (Add `tokio` workspace pin in root `Cargo.toml` per the version table below if not present.)

    **Step 2 — extend `crates/rollout-core/src/traits/storage.rs`** per the `<interfaces>` Spec 04 §2 surface:
    - Keep `#[async_trait]` on both traits. Object-safe = no generic methods; use `Vec<u8>` payloads (downstream impls layer postcard on top per CONTEXT D-STO-04).
    - Add types: `StorageKey`, `KeyRange`, `StorageEvent` (all `Debug + Clone + Eq + Hash + Serialize + Deserialize`).
    - `Storage` keeps `begin` + `ping`; adds `get_bytes(&self, key: &StorageKey)`, `get_many_bytes(&self, keys: &[StorageKey])`, `scan_bytes(&self, range: KeyRange)` returning `Vec<(StorageKey, Vec<u8>)>` (NOT a stream — spec uses `BoxStream` but stream object-safety + dyn-async makes this awkward; Phase 2 uses owned `Vec` per RESEARCH Open Question 1 recommendation "extend only what Phase 2 needs"; document this as a Phase-2 simplification in the spec edit below).
    - `Storage::watch(&self, prefix: StorageKey) -> Result<tokio::sync::broadcast::Receiver<StorageEvent>, CoreError>` (in-process broadcast per CONTEXT D-STO-02).
    - `StorageTxn` adds `put_bytes`, `delete`, `cas_bytes`, `abort` per spec 04 §2.
    - Every `pub` item has a one-line `///` doc per AGENTS.md §9.3.

    **Step 3 — extend `crates/rollout-core/src/traits/plugin.rs`** per spec 03 §4-§5:
    - Add `PluginManifest`, `PluginKind`, `PluginMode`, `RuntimeHints`, `EntrySpec`, `SidecarProtocol`, `PluginDependencies` (empty struct in Phase 2 — extended in later phases), `PluginHandle { pub id: PluginId, pub manifest: PluginManifest }`, `PluginId(pub String)`.
    - `PluginHost`: `load(manifest) -> PluginHandle`, `call(&handle, method, Vec<u8>) -> Vec<u8>`, `reload(&handle, reason) -> ()`, `unload(handle) -> ()`. All `#[async_trait]`.
    - Keep `Plugin` trait minimal (name + validate); the full spec-03 §4 `Plugin` surface is for cdylib authors and lives in `rollout-plugin-host`'s ABI module (Plan 02-05).

    **Step 4 — extend `crates/rollout-core/src/traits/worker.rs`** per spec 01 §2:
    - Add `Heartbeat { worker_id, run_id, state, due_at: SystemTime }` and `WorkerState { Init, Ready, Running, Draining }`.
    - `Coordinator` adds `async fn heartbeat(&self, hb: Heartbeat) -> Result<(), CoreError>` (keeps `register` + `deregister`).
    - `Worker` adds `async fn init(&mut self, ctx: &WorkerContext)` and `async fn ready(&mut self)` BEFORE `run`. Keep `WorkerContext` as a unit struct for now (Phase 6 fleshes it out).
    - `DrainReason` keeps Phase-1 variants; no change needed.

    **Step 5b — create `crates/rollout-core/src/traits/observability.rs`** per spec 09 §2 (the contract D-OBSERVE-01 requires; the StdoutJsonEmitter impl lands in plan 02-06's coordinator binary):
    ```rust
    use async_trait::async_trait;
    use serde::{Deserialize, Serialize};

    /// Log level for `Event`. Mirrors `tracing::Level` ordering but is `Serialize`able.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Level { Trace, Debug, Info, Warn, Error }

    /// Discriminator for `Event` payloads (spec 09 §2).
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum EventKind {
        Log,
        Metric { name: smol_str::SmolStr, value: f64, unit: smol_str::SmolStr },
        Span { phase: SpanPhase },
        Domain { topic: smol_str::SmolStr },
    }

    /// Span lifecycle marker for `EventKind::Span`.
    #[derive(Debug, Clone, Copy, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum SpanPhase { Start, End }

    /// One observability event. `attrs` carries structured fields beyond the named columns.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Event {
        pub ts: std::time::SystemTime,
        pub kind: EventKind,
        pub level: Level,
        pub run_id: Option<crate::RunId>,
        pub worker_id: Option<crate::WorkerId>,
        pub trace_id: Option<String>,
        pub span_id:  Option<String>,
        pub plugin_id: Option<String>,
        pub algorithm: Option<String>,
        pub message: Option<String>,
        pub attrs: serde_json::Value,
    }

    /// Sink for structured observability events (spec 09 §2; D-OBSERVE-01).
    /// Plan 02-06 ships a `StdoutJsonEmitter` impl wired into the coordinator binary.
    #[async_trait]
    pub trait EventEmitter: Send + Sync {
        async fn emit(&self, event: Event) -> Result<(), crate::CoreError>;
    }
    ```

    Note: `serde_json` is already in `rollout-core`'s deps (Phase 1). Add it explicitly to `Cargo.toml` if not present.

    **Step 5 — extend `crates/rollout-core/src/traits/cloud.rs`** per spec 06 §3:
    - Add types: `PutHint`, `GpuInfo`, `ComputeInventory`, `QueueItemId(pub ulid::Ulid)`.
    - `ObjectStore`: replace string-keyed put/get with `put_bytes(Vec<u8>, PutHint) -> ContentId` and `get_bytes(&ContentId) -> Vec<u8>` + add `exists(&ContentId) -> bool`. Phase-1 string-keyed methods go away — there are NO existing impls so nothing breaks.
    - `SecretStore`: add `put(&self, name, value) -> Result<(), CoreError>` (local impl will return `Fatal(ConfigInvalid)` per D-LOCAL-03; cloud impls in Phase 5 actually write).
    - `ComputeHint`: replace `instance_type` with `inventory() -> ComputeInventory` (carries instance_type as `Option<String>`) and `preemption_signal() -> Option<Duration>`.
    - `Queue`: replace `enqueue/dequeue` blob API with `enqueue(Vec<u8>) -> QueueItemId`, `dequeue() -> Option<(QueueItemId, Vec<u8>)>`, `ack(QueueItemId)`, `nack(QueueItemId)`.

    **Step 6 — update `crates/rollout-core/src/traits/mod.rs` and `src/lib.rs`** re-exports so every new pub type/trait is reachable from `rollout_core::*`. Add `pub mod observability;` to `traits/mod.rs`. The `pub use` line in `lib.rs` needs to include `StorageKey, KeyRange, StorageEvent, PluginManifest, PluginKind, PluginMode, RuntimeHints, EntrySpec, SidecarProtocol, PluginDependencies, PluginHandle, PluginId, Heartbeat, WorkerState, PutHint, GpuInfo, ComputeInventory, QueueItemId, Event, EventKind, EventEmitter, Level, SpanPhase`.

    **Step 7 — `crates/rollout-core/tests/trait_surface.rs`** — add the RED tests described in `<behavior>` above. Use `fn _assert_dyn_safe<T: Storage + ?Sized>() {}` invocations to compile-test object safety; use `fn _assert_method_exists() { let _: fn(&dyn Storage, &StorageKey) -> _ = |s, k| Storage::get_bytes(s, k); }` patterns to assert method shape without instantiation.

    **Step 8 — extend `crates/rollout-core/tests/dependency_direction.rs`** (the file lints workspace dep edges). Add two new constants and two new rules:
    ```rust
    const TRANSPORT_CRATES: &[&str] = &["rollout-transport"];
    const PLUGIN_HOST_CRATES: &[&str] = &["rollout-plugin-host"];

    fn violation_transport_uses_cloud(pkg: &str, dep: &str) -> bool {
        TRANSPORT_CRATES.contains(&pkg) && CLOUD_CRATES.contains(&dep)
    }
    fn violation_plugin_host_uses_transport(pkg: &str, dep: &str) -> bool {
        PLUGIN_HOST_CRATES.contains(&pkg) && dep == "rollout-transport"
    }
    ```
    Wire both into the existing `algo_crates_do_not_depend_on_cloud_crates` test (rename to `dep_direction_invariants_hold` for clarity) and add **TWO** new fixture directories with the same `Cargo.toml`-only hand-rolled pattern Phase 1 uses:
    - `crates/rollout-core/tests/fixtures/violation_transport_cloud/Cargo.toml` (pkg = rollout-transport, dep = rollout-cloud-local)
    - `crates/rollout-core/tests/fixtures/violation_plugin_host_transport/Cargo.toml` (pkg = rollout-plugin-host, dep = rollout-transport)
    Add `deliberate_violation_*_detected` tests for each. Keep the existing `deliberate_violation_fixture_is_detected` test green by leaving the algo→cloud fixture untouched.

    **Constraints (the spec is the contract, AGENTS.md §4):**
    - Do NOT change `ids.rs`, `errors.rs`, `config/mod.rs`.
    - Object safety is mandatory — `dyn Storage` / `dyn StorageTxn` / `dyn PluginHost` / `dyn Coordinator` must compile. Move any generic typed-payload helpers (postcard wrappers) to FREE functions in `rollout_core::storage_helpers` if you create them at all (Phase 2 downstream crates can layer their own helpers).
    - The `missing_docs` workspace lint is `warn` (root `Cargo.toml` line 17). One-line `///` doc on every `pub` item to keep clippy green per DOCS-03.
    - The `unsafe_code = "forbid"` lint stays. No FFI here.
  </action>
  <verify>
    <automated>cargo test -p rollout-core --tests &amp;&amp; cargo clippy -p rollout-core --all-targets --all-features -- -D warnings &amp;&amp; RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" cargo doc -p rollout-core --no-deps --all-features</automated>
  </verify>
  <acceptance_criteria>
    - `cargo test -p rollout-core --test trait_surface` exits 0 with new RED tests now green
    - `cargo test -p rollout-core --test dependency_direction` exits 0; the test file contains `violation_transport_uses_cloud` and `violation_plugin_host_uses_transport`
    - `crates/rollout-core/src/traits/storage.rs` contains `pub struct StorageKey`, `pub enum StorageEvent`, `fn get_bytes`, `fn watch`, `fn cas_bytes`
    - `crates/rollout-core/src/traits/plugin.rs` contains `pub struct PluginManifest`, `pub struct PluginHandle`, `fn call`, `fn reload`, `fn unload`
    - `crates/rollout-core/src/traits/worker.rs` contains `pub struct Heartbeat`, `pub enum WorkerState`, `fn heartbeat`, `fn init`, `fn ready`
    - `crates/rollout-core/src/traits/cloud.rs` contains `pub struct ComputeInventory`, `pub struct QueueItemId`, `fn put_bytes`, `fn ack`, `fn nack`, `fn preemption_signal`
    - `crates/rollout-core/src/traits/observability.rs` contains `pub trait EventEmitter`, `pub struct Event`, `pub enum EventKind` (per spec 09 §2; D-OBSERVE-01 contract)
    - `crates/rollout-core/src/lib.rs` `pub use` line re-exports the new types (grep: `PluginHandle|StorageKey|Heartbeat|WorkerState|ComputeInventory`)
    - `crates/rollout-core/tests/fixtures/violation_transport_cloud/Cargo.toml` exists and parses
    - `crates/rollout-core/tests/fixtures/violation_plugin_host_transport/Cargo.toml` exists and parses
    - `cargo clippy -p rollout-core --all-targets --all-features -- -D warnings` exits 0
    - `cargo doc -p rollout-core --no-deps --all-features` produces zero warnings (RUSTDOCFLAGS not needed locally; CI applies them — see plan 02-07)
    - Doc + test touched in same commit (DOCS-02): trait files have `///` docs; new tests are in `tests/trait_surface.rs` and `tests/dependency_direction.rs`
  </acceptance_criteria>
  <done>
    rollout-core trait surface matches the subset of spec 01/03/04/06 that Phase 2 consumes; object-safety preserved; six dep-direction invariants pass; per-commit doc/test policy satisfied via inline `///` docs on every new pub item.
  </done>
</task>

<task type="auto">
  <name>Task 2: Register six new crates + workspace deps + preflight + spec updates + mdBook landing</name>
  <files>
    Cargo.toml,
    deny.toml,
    crates/rollout-proto/Cargo.toml,
    crates/rollout-proto/src/lib.rs,
    crates/rollout-storage/Cargo.toml,
    crates/rollout-storage/src/lib.rs,
    crates/rollout-cloud-local/Cargo.toml,
    crates/rollout-cloud-local/src/lib.rs,
    crates/rollout-transport/Cargo.toml,
    crates/rollout-transport/src/lib.rs,
    crates/rollout-plugin-host/Cargo.toml,
    crates/rollout-plugin-host/src/lib.rs,
    crates/rollout-coordinator/Cargo.toml,
    crates/rollout-coordinator/src/lib.rs,
    scripts/preflight.sh,
    .gitignore,
    docs/specs/01-core-runtime.md,
    docs/specs/03-plugin-system.md,
    docs/specs/04-storage-snapshots.md,
    docs/specs/06-cloud-layer.md,
    docs/book/src/SUMMARY.md,
    docs/book/src/substrate/index.md
  </files>
  <read_first>
    - Cargo.toml (root workspace — extend `members` and `[workspace.dependencies]`)
    - deny.toml (verify Phase-2 deps don't trip [bans] — none should; rustls is the explicit alt to openssl)
    - .gitignore (add `data/`)
    - Makefile (no edits here — preserved verbatim; plan 02-07 extends it)
    - .github/workflows/ci.yml (no edits here — preserved verbatim; plan 02-07 adds smoke job)
    - scripts/check-docs-tests-touched.sh (existing CI script — Phase 2 inherits)
    - docs/book/src/SUMMARY.md (add substrate section — preserve examples placeholder)
    - docs/book/src/introduction.md (style reference)
    - docs/book/src/architecture.md (style reference)
    - docs/specs/04-storage-snapshots.md §2 (annotate the Phase-2 simplification of `scan` returning `Vec` not `BoxStream`)
    - docs/specs/03-plugin-system.md §4 (note `PluginHost::call` uses `Vec<u8>` payloads in Phase 2; richer typed-payload helpers ship in later phases)
    - docs/specs/01-core-runtime.md §2 (note `Worker::init` / `ready` lifecycle hooks land in Phase 2; `WorkerContext` stays a unit struct until Phase 6)
    - docs/specs/06-cloud-layer.md §3 (note `Queue::ack/nack` + `ObjectStore::exists` + `ComputeHint::preemption_signal` shipped in Phase 2 core; impls in Phase 2 cloud-local, Phase 5 aws/gcp)
    - .planning/phases/02-local-substrate/02-RESEARCH.md §"Standard Stack" (the pin table — use those versions VERBATIM)
    - .planning/phases/02-local-substrate/02-RESEARCH.md §"Recommended Project Structure"
    - .planning/phases/02-local-substrate/02-RESEARCH.md §"Environment Availability" (preflight requirements)
  </read_first>
  <action>
    **Step 1 — extend root `Cargo.toml`:**

    Add to `[workspace] members`:
    ```toml
    "crates/rollout-proto",
    "crates/rollout-storage",
    "crates/rollout-cloud-local",
    "crates/rollout-transport",
    "crates/rollout-plugin-host",
    "crates/rollout-coordinator",
    ```

    Add to `[workspace.dependencies]` (VERBATIM from RESEARCH.md §"Standard Stack"):
    ```toml
    # Phase 2 — storage
    redb               = "2.5"
    postcard           = { version = "1.0", features = ["use-std"] }

    # Phase 2 — async runtime + observability extensions
    tokio              = { version = "1.40", features = ["rt-multi-thread", "macros", "sync", "time", "fs", "net", "process", "signal", "io-util"] }
    tokio-util         = { version = "0.7", features = ["io"] }
    tokio-stream       = { version = "0.1", features = ["sync", "net"] }
    tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
    humantime-serde    = "1.1"

    # Phase 2 — transport
    tonic              = { version = "0.14", features = ["tls-rustls", "transport"] }
    tonic-build        = "0.14"
    prost              = "0.13"
    prost-types        = "0.13"
    rustls             = { version = "0.23", default-features = false, features = ["ring", "std"] }
    rcgen              = "0.13"
    bytes              = "1.7"
    # NOTE: tonic-h3 / h3 / h3-quinn / quinn are NOT pinned in workspace.dependencies — they are
    # only added to rollout-transport behind the `quic` feature flag in plan 02-04 (EXPERIMENTAL).

    # Phase 2 — plugin host
    pyo3                 = { version = "0.28", features = ["auto-initialize", "abi3-py311"] }
    pyo3-async-runtimes  = { version = "0.28", features = ["tokio-runtime"] }
    libloading           = "0.8"

    # Phase 2 — cloud-local
    sysinfo            = { version = "0.33", default-features = false, features = ["system"] }
    nvml-wrapper       = { version = "0.11", optional = true }

    # Phase 2 — common
    toml               = "0.8"
    hex                = "0.4"

    # Phase 2 — dev/test
    tempfile           = "3.10"
    proptest           = "1.5"
    assert_cmd         = "2.0"
    predicates         = "3.1"
    ```

    **Step 2 — for EACH of the six new crates**, create `crates/<name>/Cargo.toml` and `crates/<name>/src/lib.rs` as **EMPTY STUBS** (downstream plans fill them in):

    Template `Cargo.toml`:
    ```toml
    [package]
    name = "rollout-<name>"
    version.workspace = true
    edition.workspace = true
    license.workspace = true
    rust-version.workspace = true
    repository.workspace = true

    [lints]
    workspace = true

    [dependencies]
    # Filled by plan 02-NN — Wave-0 stub only.
    rollout-core = { path = "../rollout-core" }
    ```

    Template `src/lib.rs` (crate-level `//!` doc satisfies DOCS-03):
    ```rust
    //! `rollout-<name>` — Phase-2 substrate crate; populated by plan 02-NN.
    //!
    //! Wave-0 stub: this file exists so the workspace compiles and downstream
    //! crates can declare a `path = "../rollout-<name>"` dependency. The real
    //! implementation lands in plan 02-NN.
    ```

    For `rollout-coordinator` ONLY, add a `[[bin]]` section to the Cargo.toml:
    ```toml
    [[bin]]
    name = "rollout-coordinator"
    path = "src/main.rs"
    ```
    and create `src/main.rs`:
    ```rust
    //! `rollout-coordinator` binary — populated by plan 02-06.
    fn main() {
        eprintln!("rollout-coordinator: not yet implemented (plan 02-06)");
        std::process::exit(2);
    }
    ```

    For `rollout-proto` ONLY, add a `build.rs` stub:
    ```rust
    //! build.rs — populated by plan 02-01 to invoke tonic-build on .proto files.
    fn main() {
        println!("cargo:rerun-if-changed=proto/");
    }
    ```
    Add `build-dependencies = { tonic-build.workspace = true }` to `crates/rollout-proto/Cargo.toml`.

    **Step 3 — `.gitignore`:** append `data/` on its own line, preserve existing entries (do NOT rewrite the file from scratch; ONLY append). Phase 1 already ignores `target/`, `node_modules/`, `graphify-out/`.

    **Step 4 — `scripts/preflight.sh`** (NEW, executable):
    ```bash
    #!/usr/bin/env bash
    # Preflight check for Phase-2 substrate. Run before `make smoke`.
    set -euo pipefail

    fail() { echo "preflight FAIL: $*" >&2; exit 1; }

    command -v cargo >/dev/null 2>&1 || fail "cargo not on PATH"
    command -v make  >/dev/null 2>&1 || fail "make not on PATH"
    command -v python3 >/dev/null 2>&1 || fail "python3 not on PATH (need >= 3.11 for sidecar sample)"

    PY_VER=$(python3 -c 'import sys; print(f"{sys.version_info[0]}.{sys.version_info[1]}")')
    PY_MAJ=${PY_VER%.*}; PY_MIN=${PY_VER#*.}
    if [ "$PY_MAJ" -lt 3 ] || { [ "$PY_MAJ" -eq 3 ] && [ "$PY_MIN" -lt 11 ]; }; then
        fail "python3 $PY_VER detected; need >= 3.11"
    fi

    # protoc is preferred but tonic-build vendors it for most targets.
    if ! command -v protoc >/dev/null 2>&1; then
        echo "preflight note: protoc not found on PATH; tonic-build bundles one but install protobuf-compiler if compilation fails" >&2
    fi

    echo "preflight OK: cargo $(cargo --version | awk '{print $2}'), python3 $PY_VER"
    ```
    `chmod +x scripts/preflight.sh`.

    **Step 5 — `docs/book/src/SUMMARY.md`** — extend (preserving examples placeholder for SHIP-03):
    ```markdown
    # Summary

    - [Introduction](./introduction.md)
    - [Architecture](./architecture.md)
    - [Substrate](./substrate/index.md)
    - [Examples](./examples/index.md)
    ```

    **Step 6 — `docs/book/src/substrate/index.md`** (NEW, ~40 lines):
    Title "Substrate (Phase 2)". Sections:
    - **What ships in Phase 2** (one-paragraph each on: rollout-proto, rollout-storage, rollout-cloud-local, rollout-transport, rollout-plugin-host, rollout-coordinator, smoke test).
    - **Plan-of-record vs stretch** — H/2 tonic + rustls is plan-of-record; QUIC via `tonic-h3` v0.0.x is feature-flagged EXPERIMENTAL.
    - **Trait surface** — links to spec 01/03/04/06.
    - **Per-crate chapters land in plan 02-07.** Placeholder TOC list.
    - **Preflight** — `bash scripts/preflight.sh` must pass before `make smoke`.

    **Step 7 — update the four specs in the same PR per AGENTS.md §4 + §9.2:**

    For each of `docs/specs/01-core-runtime.md`, `docs/specs/03-plugin-system.md`, `docs/specs/04-storage-snapshots.md`, `docs/specs/06-cloud-layer.md`, add a short **"Phase 2 implementation notes"** section near the top (after §1 Purpose):

    - **01-core-runtime.md:** "`Worker::init` and `Worker::ready` lifecycle hooks land in `rollout-core` in Phase 2; `WorkerContext` remains a unit struct until Phase 6 (multi-node distribution) needs richer state. `Coordinator::heartbeat(Heartbeat)` ships in Phase 2; `Coordinator::pull` / `submit` / `control` land in Phase 6."
    - **03-plugin-system.md:** "Phase 2 ships `PluginHost::call` with `Vec<u8>` payloads (postcard or JSON, plugin-defined). Typed-payload generic helpers may land in a later phase. The full `Plugin` cdylib trait surface lives in `rollout-plugin-host`'s ABI module — not in `rollout-core`."
    - **04-storage-snapshots.md:** "Phase 2's `Storage::scan_bytes` returns `Vec<(StorageKey, Vec<u8>)>` rather than the `BoxStream` shown in §2 — object-safety with `dyn Storage` + `async_trait` is incompatible with stream-returning methods on stable Rust. Streaming `scan` is deferred to a later phase that introduces a `StorageStream` newtype."
    - **06-cloud-layer.md:** "`Queue::ack` / `nack`, `ObjectStore::exists`, `ComputeHint::preemption_signal`, and `SecretStore::put` ship in `rollout-core` in Phase 2. Concrete impls land in `rollout-cloud-local` (Phase 2) and `rollout-cloud-aws` / `-gcp` (Phase 5)."

    **Step 8 — verify `cargo deny check` passes locally** (no source edits unless a Phase-2 dep trips a license). All pins selected in Step 1 are MIT/Apache-2.0 per RESEARCH §"Standard Stack" — `cargo deny` should pass without touching `deny.toml`. If a `[bans] multiple-versions = warn` complains, that's still `warn` not `deny` per Phase-1 decision (STATE.md). Do NOT edit `deny.toml` unless a true `deny` violation surfaces.
  </action>
  <verify>
    <automated>cargo build --workspace &amp;&amp; cargo test --workspace --tests &amp;&amp; cargo deny check &amp;&amp; bash scripts/preflight.sh &amp;&amp; cargo xtask schema-gen &amp;&amp; git diff --exit-code schemas/ python/</automated>
  </verify>
  <acceptance_criteria>
    - `Cargo.toml` `[workspace] members` contains all six new crate paths (grep for each: `crates/rollout-proto`, `crates/rollout-storage`, `crates/rollout-cloud-local`, `crates/rollout-transport`, `crates/rollout-plugin-host`, `crates/rollout-coordinator`)
    - `Cargo.toml` `[workspace.dependencies]` contains pins for `redb`, `tonic`, `tonic-build`, `prost`, `pyo3`, `pyo3-async-runtimes`, `libloading`, `rustls`, `rcgen`, `postcard`, `sysinfo`, `nvml-wrapper`, `tempfile`
    - `cargo build --workspace` exits 0
    - `cargo test --workspace --tests` exits 0 (Task 1's new tests pass; Phase-1 tests remain green)
    - `cargo deny check` exits 0 (no banned deps; all Phase-2 pins are MIT/Apache-2.0/etc per deny.toml allowlist)
    - `bash scripts/preflight.sh` exits 0 (python3 ≥ 3.11 confirmed on dev machine)
    - `scripts/preflight.sh` is executable (`-x` bit set)
    - `cargo xtask schema-gen && git diff --exit-code schemas/ python/` exits 0 (no schema drift introduced — Wave 0 changes touch traits, not config types)
    - `.gitignore` contains a line `data/`
    - `docs/book/src/SUMMARY.md` contains `[Substrate](./substrate/index.md)` AND still contains `[Examples](./examples/index.md)` (existing reservation preserved)
    - `docs/book/src/substrate/index.md` exists and `mdbook build docs/book` succeeds (validates link integrity)
    - `docs/specs/04-storage-snapshots.md` contains the string "Phase 2 implementation notes" or equivalent; similarly for specs 01, 03, 06
    - DOCS-02: this commit touches `docs/specs/*.md` AND `docs/book/src/substrate/index.md` AND code AND tests — same-commit doc + test policy satisfied
  </acceptance_criteria>
  <done>
    Workspace registers all six Phase-2 crates as empty stubs; Phase-2 dependency versions pinned in `[workspace.dependencies]`; preflight script gates `make smoke`; specs annotated per AGENTS.md §4; mdBook substrate section landing page exists; cargo deny + cargo build + cargo test --workspace + schema-drift all green.
  </done>
</task>

</tasks>

<verification>
End-to-end verification of Wave 0:
```bash
cargo build --workspace
cargo test --workspace --tests
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo deny check
cargo xtask schema-gen && git diff --exit-code schemas/ python/
mdbook build docs/book
bash scripts/preflight.sh
RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" \
  cargo doc --workspace --no-deps --all-features
```
All commands must exit 0.
</verification>

<success_criteria>
Wave 0 is complete when:
- rollout-core trait surface covers Phase-2 needs (Storage put/get/scan/watch/cas/abort; PluginHost call/reload/unload; Coordinator heartbeat; Worker init/ready; ObjectStore content-addressed; Queue ack/nack; ComputeHint inventory + preemption)
- Six new crates exist as stubs and compile
- Workspace dependency pins match RESEARCH §"Standard Stack" verbatim
- Dep-direction lint enforces the two new Phase-2 invariants
- Specs 01/03/04/06 carry "Phase 2 implementation notes" sections explaining the trait surface delivered
- mdBook builds; substrate landing page reachable from SUMMARY.md
- `bash scripts/preflight.sh` succeeds
- Phase-1 CI gates (rustdoc + clippy + schema-drift + cargo deny + dep-direction lint) remain green
</success_criteria>

<output>
After completion, create `.planning/phases/02-local-substrate/02-00-wave0-trait-extensions-SUMMARY.md` documenting:
- Final trait shapes landed (and any deviations from spec, with the same-PR spec edit cited)
- Workspace-dependencies pins added with versions
- Two new dep-direction fixture paths
- Decisions taken under "Claude's Discretion" (e.g., scan returning Vec not BoxStream; ObjectStore replacing string-keyed put/get)
- Any open questions surfaced for Wave 1 (e.g., whether `PluginDependencies` needs fleshing out for the in-tree cdylib sample)
</output>
