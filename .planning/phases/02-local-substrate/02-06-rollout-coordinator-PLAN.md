---
phase: 02-local-substrate
plan: 06
type: execute
wave: 5
depends_on: [02-00, 02-01, 02-02, 02-03, 02-04, 02-05]
files_modified:
  - crates/rollout-coordinator/Cargo.toml
  - crates/rollout-coordinator/src/lib.rs
  - crates/rollout-coordinator/src/main.rs
  - crates/rollout-coordinator/src/config.rs
  - crates/rollout-coordinator/src/registry.rs
  - crates/rollout-coordinator/src/heartbeat.rs
  - crates/rollout-coordinator/src/failure_scan.rs
  - crates/rollout-coordinator/src/emitter.rs
  - crates/rollout-coordinator/tests/registry_persistence.rs
  - crates/rollout-coordinator/tests/failure_scan.rs
  - crates/rollout-cli/src/main.rs
  - crates/rollout-cli/Cargo.toml
  - docs/book/src/substrate/coordinator.md
  - docs/book/src/SUMMARY.md
autonomous: true
requirements: [SUBSTR-02, DOCS-01, DOCS-02, DOCS-03]
must_haves:
  truths:
    - "rollout-coordinator binary `rollout-coordinator run --config <path>` boots, opens Storage, opens TLS transport, and starts the failure-scan loop."
    - "Coordinator persists worker registry to Storage namespace `workers` and heartbeat ledger to `heartbeats`."
    - "Failure-scan task marks workers failed when now > due_at + coordinator_failure_timeout AND elapsed > clock_skew_budget."
    - "rollout-cli gains `worker run` and `coordinator run` subcommands (the coordinator subcommand routes to the coordinator binary OR runs the same library entry point in-process)."
    - "Phase-2 coordinator is explicitly minimal — no work distribution, no lease/CAS, no multi-coordinator (Phase 6)."
    - "Coordinator binary instantiates a `StdoutJsonEmitter` impl of `rollout_core::EventEmitter` (D-OBSERVE-01); emits one structured `Event` per `worker_registered` / `worker_heartbeat` / `worker_failed` to stdout as NDJSON, in addition to `tracing-subscriber` JSON output."
  artifacts:
    - path: crates/rollout-coordinator/src/registry.rs
      provides: "WorkerRegistry: tracks workers by ID with last-seen heartbeat; persists to Storage"
      contains: "pub struct WorkerRegistry"
    - path: crates/rollout-coordinator/src/heartbeat.rs
      provides: "CoordinatorImpl: impl rollout_core::Coordinator persisted via Storage"
      contains: "impl Coordinator for"
    - path: crates/rollout-coordinator/src/failure_scan.rs
      provides: "Periodic task scanning heartbeats for deadline violations"
      contains: "pub async fn failure_scan_loop"
    - path: crates/rollout-coordinator/src/emitter.rs
      provides: "StdoutJsonEmitter: impl rollout_core::EventEmitter writing one NDJSON line per event to stdout"
      contains: "impl EventEmitter for StdoutJsonEmitter"
    - path: crates/rollout-coordinator/src/main.rs
      provides: "Coordinator binary entrypoint"
      contains: "tonic::transport::Server"
    - path: crates/rollout-cli/src/main.rs
      provides: "`rollout worker run` + `rollout coordinator run` clap subcommands"
      contains: "Worker { .. } | Coordinator { .. }"
  key_links:
    - from: crates/rollout-coordinator/src/heartbeat.rs
      to: rollout_storage::EmbeddedStorage
      via: "Arc<dyn Storage>"
      pattern: "Arc<dyn Storage>"
    - from: crates/rollout-coordinator/src/main.rs
      to: rollout_transport::server::serve
      via: "wires HeartbeatServiceImpl(coord) into the H/2 transport"
      pattern: "HeartbeatServiceImpl"
    - from: crates/rollout-cli/src/main.rs
      to: rollout_coordinator::run
      via: "coordinator subcommand dispatch"
      pattern: "Coordinator"
---

<objective>
Implement `rollout-coordinator` — the minimal control-plane binary per CONTEXT D-COORD-01..02 + SUBSTR-02 acceptance criterion #3 ("kill a worker, observe coordinator marks it failed within `2 × heartbeat_interval`").

**In scope:**
- Register-worker, accept-heartbeat, persist worker registry + heartbeat ledger to Storage (namespaces: `workers`, `heartbeats`).
- Deadline-based failure scan: a periodic task watching the `heartbeats/*` Storage prefix; emits `worker_failed` events when the deadline-based formula triggers.
- `rollout-coordinator` binary wires:
  1. Storage (EmbeddedStorage at config path).
  2. Transport (HeartbeatServiceImpl + ControlServiceImpl + stub WorkServiceImpl).
  3. Failure-scan task.
- `rollout-cli` gains `worker run` and `coordinator run` clap subcommands.

**Out of scope (Phase 6 `DIST-01..05`):**
- Work distribution, work-stealing.
- Coordinator lease/CAS (HA).
- Multi-coordinator handoff.
- Coordinator-restart-from-storage 4-node integration test.

Output: Coordinator binary boots, accepts heartbeats from a (test) worker, persists state, and surfaces deadline-detected failures via tracing events.
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
@$HOME/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/STATE.md
@.planning/phases/02-local-substrate/02-CONTEXT.md
@.planning/phases/02-local-substrate/02-RESEARCH.md
@.planning/phases/02-local-substrate/02-VALIDATION.md
@.planning/phases/02-local-substrate/02-00-wave0-trait-extensions-PLAN.md
@.planning/phases/02-local-substrate/02-02-rollout-storage-PLAN.md
@.planning/phases/02-local-substrate/02-04-rollout-transport-PLAN.md
@docs/specs/01-core-runtime.md
@docs/specs/05-distribution.md
@crates/rollout-core/src/traits/worker.rs
@crates/rollout-cli/src/main.rs
@Cargo.toml

<interfaces>
Trait surface (post-Wave-0):
```rust
pub struct Heartbeat { worker_id: WorkerId, run_id: RunId, state: WorkerState, due_at: SystemTime }
pub enum WorkerState { Init, Ready, Running, Draining }
#[async_trait] pub trait Coordinator {
    async fn register(&self, worker: WorkerId) -> Result<(), CoreError>;
    async fn deregister(&self, worker: WorkerId) -> Result<(), CoreError>;
    async fn heartbeat(&self, hb: Heartbeat) -> Result<(), CoreError>;
}
```

Storage (post-Wave-0) used:
```rust
trait Storage {
    async fn begin(&self) -> Result<Box<dyn StorageTxn>, CoreError>;
    async fn watch(&self, prefix: StorageKey) -> Result<broadcast::Receiver<StorageEvent>, CoreError>;
    async fn scan_bytes(&self, range: KeyRange) -> Result<Vec<(StorageKey, Vec<u8>)>, CoreError>;
}
```

Transport health helpers (plan 02-04):
```rust
pub fn next_due_at(now: SystemTime, hb_interval: Duration) -> SystemTime;
pub fn is_failed(now: SystemTime, due_at: SystemTime, skew: Duration, coord_timeout: Duration) -> bool;
```

Worker-registry storage layout:
- Namespace `workers`, path `[<worker_id>]` → postcard(WorkerRegistryEntry)
- Namespace `heartbeats`, path `[<worker_id>]` → postcard(HeartbeatRecord { run_id, state, due_at, received_at })

Failure events go to tracing (`tracing::warn!(target: "coordinator", worker_id = %wid, "worker_failed")`). Phase 2 does NOT persist a separate "failure" event log. **Per CONTEXT D-OBSERVE-01**: this plan also ships a `StdoutJsonEmitter` impl of `rollout_core::EventEmitter` (the trait lands in plan 02-00); the coordinator binary (Task 2) instantiates it and is the wire-in point for spec 09's stdout-NDJSON sink.
</interfaces>
</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: Coordinator core — registry + heartbeat impl + failure-scan loop + Storage persistence tests</name>
  <files>
    crates/rollout-coordinator/Cargo.toml,
    crates/rollout-coordinator/src/lib.rs,
    crates/rollout-coordinator/src/config.rs,
    crates/rollout-coordinator/src/registry.rs,
    crates/rollout-coordinator/src/heartbeat.rs,
    crates/rollout-coordinator/src/failure_scan.rs,
    crates/rollout-coordinator/tests/registry_persistence.rs,
    crates/rollout-coordinator/tests/failure_scan.rs
  </files>
  <read_first>
    - crates/rollout-coordinator/Cargo.toml (Wave-0 stub)
    - crates/rollout-coordinator/src/lib.rs (Wave-0 stub)
    - crates/rollout-coordinator/src/main.rs (Wave-0 stub binary)
    - crates/rollout-core/src/traits/worker.rs (post-Wave-0)
    - crates/rollout-core/src/traits/observability.rs (post-Wave-0; defines EventEmitter / Event / EventKind)
    - crates/rollout-storage/src/lib.rs (EmbeddedStorage)
    - crates/rollout-transport/src/health.rs (next_due_at / is_failed)
    - docs/specs/01-core-runtime.md (coordinator lifecycle)
    - docs/specs/05-distribution.md §6 (deadline-based health)
    - .planning/phases/02-local-substrate/02-CONTEXT.md D-COORD-01..02 + D-TIME-01
    - .planning/phases/02-local-substrate/02-RESEARCH.md §"Pattern 5: deadline-based health"
    - .planning/phases/02-local-substrate/02-RESEARCH.md §"Pitfall 7: tracing spans + spawn_blocking"
  </read_first>
  <behavior>
    RED first:

    `tests/registry_persistence.rs`:
    - `register_persists_worker_to_storage`: build CoordinatorImpl with EmbeddedStorage in tempdir; register(w1); reopen Storage; entry exists in namespace "workers" with key path [w1.to_string()].
    - `heartbeat_persists_to_storage`: register w1; send Heartbeat; reopen Storage; entry exists in "heartbeats/<w1>" with the expected due_at + state (postcard-decoded).
    - `deregister_removes_from_storage`: register; deregister; entry absent.
    - `heartbeat_updates_existing_ledger_entry`: send 2 heartbeats; only the latest is stored (overwrite, not append — Phase 2 doesn't keep history; Phase 6 DIST-* may).

    `tests/failure_scan.rs`:
    - `failure_scan_marks_late_workers`: register w1; insert a heartbeat with due_at = now - 10s (artificially overdue); run one iteration of the scanner; assert tracing captured `worker_failed` event for w1 (use `tracing-test` or capture events into a custom Layer in the test setup). Phase-2 scope: emit tracing::warn — no persistent "failed" status on the worker (it's read off the heartbeat ledger by checking is_failed at scan time).
    - `failure_scan_does_not_mark_healthy_workers`: insert heartbeat with due_at = now + 10s; one scan iteration; no failed event.
    - `failure_scan_respects_skew_budget`: heartbeat overdue by less than skew; not marked failed (the AND-of-both-thresholds invariant).
    - `failure_scan_loop_runs_periodically`: spawn loop with interval=50ms; insert overdue heartbeat; within 200ms see the warning emitted.

    GREEN: implement.
  </behavior>
  <action>
    **Step 1 — `crates/rollout-coordinator/Cargo.toml`:**
    ```toml
    [package]
    name = "rollout-coordinator"
    version.workspace = true
    edition.workspace = true
    license.workspace = true
    rust-version.workspace = true
    repository.workspace = true

    [lints]
    workspace = true

    [[bin]]
    name = "rollout-coordinator"
    path = "src/main.rs"

    [dependencies]
    rollout-core      = { path = "../rollout-core" }
    rollout-storage   = { path = "../rollout-storage" }
    rollout-transport = { path = "../rollout-transport" }
    async-trait       = { workspace = true }
    serde             = { workspace = true }
    serde_json        = { workspace = true }
    schemars          = { workspace = true }
    thiserror         = { workspace = true }
    tracing           = { workspace = true }
    tracing-subscriber = { workspace = true }
    tokio             = { workspace = true, features = ["rt-multi-thread", "macros", "signal", "time", "sync"] }
    smol_str          = { workspace = true }
    postcard          = { workspace = true }
    clap              = { workspace = true }
    ulid              = { workspace = true }

    [dev-dependencies]
    tempfile = { workspace = true }
    tokio = { workspace = true, features = ["macros", "rt-multi-thread", "test-util"] }
    tracing-subscriber = { workspace = true, features = ["env-filter", "json"] }
    ```

    **Step 2 — `src/config.rs`:**
    ```rust
    use rollout_storage::EmbeddedStorageConfig;
    use rollout_transport::TransportConfig;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    /// Coordinator run configuration (Phase-2 minimal).
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct CoordinatorConfig {
        /// Run ID this coordinator serves. ULID format.
        pub run_id: String,
        /// Embedded storage location.
        pub storage: EmbeddedStorageConfig,
        /// Transport (listen addr + TLS dir + heartbeat timings).
        pub transport: TransportConfig,
    }

    impl CoordinatorConfig {
        /// Validate cross-field invariants at plan-time (delegates to TransportConfig).
        pub fn validate(&self) -> Result<(), Vec<String>> { self.transport.validate_cross_fields() }
    }
    ```

    **Step 3 — `src/registry.rs`:**
    ```rust
    use rollout_core::{CoreError, FatalError, RunId, StorageKey, WorkerId, WorkerState};
    use serde::{Deserialize, Serialize};
    use std::time::SystemTime;

    /// Persisted worker registry entry.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct WorkerRegistryEntry {
        pub worker_id: String,
        pub run_id:    String,
        pub registered_at: u128,    // ms since epoch
    }

    /// Persisted heartbeat ledger entry.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct HeartbeatRecord {
        pub worker_id: String,
        pub run_id:    String,
        pub state:     i32,        // WorkerState as i32
        pub due_at_ms: u128,
        pub received_at_ms: u128,
    }

    pub fn worker_key(id: WorkerId) -> StorageKey {
        StorageKey {
            namespace: smol_str::SmolStr::new("workers"),
            run_id: None,
            path: vec![smol_str::SmolStr::new(id.0.to_string())],
        }
    }
    pub fn heartbeat_key(id: WorkerId) -> StorageKey {
        StorageKey {
            namespace: smol_str::SmolStr::new("heartbeats"),
            run_id: None,
            path: vec![smol_str::SmolStr::new(id.0.to_string())],
        }
    }
    pub fn now_ms() -> u128 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()
    }
    pub fn ms_to_systime(ms: u128) -> SystemTime { std::time::UNIX_EPOCH + std::time::Duration::from_millis(ms as u64) }
    ```

    **Step 4 — `src/heartbeat.rs`:**
    ```rust
    use async_trait::async_trait;
    use rollout_core::{Coordinator, CoreError, Heartbeat, Storage, StorageTxn, WorkerId};
    use std::sync::Arc;

    /// Persisted-state Coordinator implementation. Phase 2 scope only:
    /// register / deregister / heartbeat into Storage. Work pull / submit / lease
    /// land in Phase 6 (DIST-01..05).
    pub struct CoordinatorImpl {
        storage: Arc<dyn Storage>,
        run_id: rollout_core::RunId,
        emitter: Arc<dyn rollout_core::EventEmitter>,
    }

    impl CoordinatorImpl {
        /// `emitter` lands a structured spec-09 `Event` per state transition (D-OBSERVE-01).
        /// Tests use `NoopEmitter`; the binary in Task 2 plugs in `StdoutJsonEmitter`.
        pub fn new(
            storage: Arc<dyn Storage>,
            run_id: rollout_core::RunId,
            emitter: Arc<dyn rollout_core::EventEmitter>,
        ) -> Self { Self { storage, run_id, emitter } }
    }

    #[async_trait]
    impl Coordinator for CoordinatorImpl {
        async fn register(&self, worker: WorkerId) -> Result<(), CoreError> {
            let entry = crate::registry::WorkerRegistryEntry {
                worker_id: worker.0.to_string(),
                run_id: self.run_id.0.to_string(),
                registered_at: crate::registry::now_ms(),
            };
            let bytes = postcard::to_allocvec(&entry).map_err(internal)?;
            let mut txn = self.storage.begin().await?;
            txn.put_bytes(crate::registry::worker_key(worker), bytes).await?;
            txn.commit().await?;
            tracing::info!(target: "coordinator", worker_id = %worker.0, "worker_registered");
            Ok(())
        }

        async fn deregister(&self, worker: WorkerId) -> Result<(), CoreError> {
            let mut txn = self.storage.begin().await?;
            txn.delete(crate::registry::worker_key(worker)).await?;
            txn.delete(crate::registry::heartbeat_key(worker)).await?;
            txn.commit().await?;
            tracing::info!(target: "coordinator", worker_id = %worker.0, "worker_deregistered");
            Ok(())
        }

        async fn heartbeat(&self, hb: Heartbeat) -> Result<(), CoreError> {
            let rec = crate::registry::HeartbeatRecord {
                worker_id: hb.worker_id.0.to_string(),
                run_id: hb.run_id.0.to_string(),
                state: state_to_i32(hb.state),
                due_at_ms: hb.due_at.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0),
                received_at_ms: crate::registry::now_ms(),
            };
            let bytes = postcard::to_allocvec(&rec).map_err(internal)?;
            let mut txn = self.storage.begin().await?;
            txn.put_bytes(crate::registry::heartbeat_key(hb.worker_id), bytes).await?;
            txn.commit().await?;
            tracing::trace!(target: "coordinator", worker_id = %hb.worker_id.0, "worker_heartbeat");
            Ok(())
        }
    }

    fn internal<E: std::fmt::Display>(e: E) -> CoreError { CoreError::Fatal(rollout_core::FatalError::Internal(e.to_string())) }
    fn state_to_i32(s: rollout_core::WorkerState) -> i32 { match s { rollout_core::WorkerState::Init=>1, rollout_core::WorkerState::Ready=>2, rollout_core::WorkerState::Running=>3, rollout_core::WorkerState::Draining=>4 } }
    ```

    **Step 5 — `src/failure_scan.rs`:**
    ```rust
    use rollout_core::{Storage, StorageKey, KeyRange};
    use std::sync::Arc;
    use std::time::{Duration, SystemTime};
    use rollout_transport::health::is_failed;

    /// Periodic task: every `interval` reads namespace `heartbeats`, decodes,
    /// and emits `worker_failed` tracing events for any entry whose `due_at`
    /// is past the failure thresholds.
    pub async fn failure_scan_loop(
        storage: Arc<dyn Storage>,
        interval: Duration,
        skew: Duration,
        coord_timeout: Duration,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) {
        let mut ticker = tokio::time::interval(interval);
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if let Err(e) = scan_once(&storage, skew, coord_timeout).await {
                        tracing::warn!(target: "coordinator", error = %format!("{e:?}"), "failure_scan_error");
                    }
                }
                Ok(_) = shutdown.changed() => {
                    if *shutdown.borrow() { break; }
                }
            }
        }
    }

    async fn scan_once(storage: &Arc<dyn Storage>, skew: Duration, coord_timeout: Duration) -> Result<(), rollout_core::CoreError> {
        let prefix = StorageKey { namespace: smol_str::SmolStr::new("heartbeats"), run_id: None, path: vec![] };
        let now = SystemTime::now();
        for (key, bytes) in storage.scan_bytes(KeyRange { prefix, limit: None }).await? {
            let Ok(rec) = postcard::from_bytes::<crate::registry::HeartbeatRecord>(&bytes) else { continue };
            let due = crate::registry::ms_to_systime(rec.due_at_ms);
            if is_failed(now, due, skew, coord_timeout) {
                tracing::warn!(target: "coordinator", worker_id = %rec.worker_id, due_at_ms = rec.due_at_ms, "worker_failed");
            }
        }
        Ok(())
    }
    ```

    **Step 6 — `src/lib.rs`:**
    ```rust
    //! `rollout-coordinator` — Phase-2 minimal control plane.
    //!
    //! Scope: register / deregister / heartbeat into Storage + deadline-based
    //! failure scan. Out of scope: work distribution, lease/CAS, multi-coordinator
    //! handoff (all Phase 6 DIST-01..05).
    #![forbid(unsafe_code)]

    pub mod config;
    pub mod emitter;
    pub mod failure_scan;
    pub mod heartbeat;
    pub mod registry;

    pub use config::CoordinatorConfig;
    pub use emitter::{NoopEmitter, StdoutJsonEmitter};
    pub use heartbeat::CoordinatorImpl;
    ```
    The full `emitter.rs` body lands in Task 2 (Step 4b); Task 1 lands a minimal `NoopEmitter` stub so unit tests can construct CoordinatorImpl. `StdoutJsonEmitter` is added in Task 2 — until then, `pub use emitter::{NoopEmitter};` only.

    **Step 7 — RED tests** per `<behavior>`. Use `tracing-subscriber::test_writer` or capture events via a custom Layer that stores `worker_failed` events into a `Mutex<Vec<String>>`. (`tracing-test` crate works too; add as dev-dep if used.) Tests build CoordinatorImpl with `Arc::new(NoopEmitter::default())` for the emitter slot — a minimal NoopEmitter (returns `Ok(())`) lives in `src/emitter.rs` (Task 1 lands the Noop stub; Task 2 adds the real StdoutJsonEmitter).

    **Step 8 — `src/emitter.rs` Noop stub** (Task 1 minimum; Task 2 extends with `StdoutJsonEmitter`):
    ```rust
    use async_trait::async_trait;
    use rollout_core::{CoreError, Event, EventEmitter};

    /// Discards events. Used by tests and as a Phase-2 default.
    #[derive(Default)]
    pub struct NoopEmitter;

    #[async_trait]
    impl EventEmitter for NoopEmitter {
        async fn emit(&self, _event: Event) -> Result<(), CoreError> { Ok(()) }
    }
    ```
  </action>
  <verify>
    <automated>cargo test -p rollout-coordinator --test registry_persistence &amp;&amp; cargo test -p rollout-coordinator --test failure_scan &amp;&amp; cargo clippy -p rollout-coordinator --all-targets -- -D warnings</automated>
  </verify>
  <acceptance_criteria>
    - `crates/rollout-coordinator/src/emitter.rs` declares `pub struct NoopEmitter` impl-ing `rollout_core::EventEmitter`
    - `crates/rollout-coordinator/src/registry.rs` declares `WorkerRegistryEntry`, `HeartbeatRecord`, `worker_key`, `heartbeat_key`
    - `crates/rollout-coordinator/src/heartbeat.rs` contains `impl Coordinator for CoordinatorImpl`
    - `crates/rollout-coordinator/src/failure_scan.rs` contains `pub async fn failure_scan_loop`
    - `cargo test -p rollout-coordinator --test registry_persistence` exits 0 (4 tests pass)
    - `cargo test -p rollout-coordinator --test failure_scan` exits 0 (4 tests pass)
    - `cargo clippy -p rollout-coordinator --all-targets -- -D warnings` exits 0
    - DOCS-02 satisfied
  </acceptance_criteria>
  <done>
    Coordinator library impl works with real Storage and deadline-based failure scan; tracing events fire correctly.
  </done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: Coordinator binary + rollout-cli worker/coordinator subcommands + mdBook chapter</name>
  <files>
    crates/rollout-coordinator/src/main.rs,
    crates/rollout-coordinator/src/emitter.rs,
    crates/rollout-coordinator/src/lib.rs,
    crates/rollout-cli/src/main.rs,
    crates/rollout-cli/Cargo.toml,
    docs/book/src/substrate/coordinator.md,
    docs/book/src/SUMMARY.md
  </files>
  <read_first>
    - crates/rollout-coordinator/src/lib.rs (Task 1 output)
    - crates/rollout-coordinator/src/main.rs (Wave-0 stub — replace)
    - crates/rollout-cli/src/main.rs (Phase 1 — has `schema` subcommand; preserve it)
    - crates/rollout-cli/Cargo.toml
    - crates/rollout-transport/src/server.rs (transport::serve to call)
    - crates/rollout-transport/src/tls.rs (ensure_dev_ca + issue_server_cert)
    - .planning/phases/02-local-substrate/02-CONTEXT.md (the smoke-script-needs-binary requirements)
    - .planning/phases/02-local-substrate/02-RESEARCH.md §"Smoke-test script shape" — main.rs must satisfy the expectations the smoke script has of CLI flags
  </read_first>
  <behavior>
    Integration check (no separate test file required for this task; verified via smoke in plan 02-07):
    - `rollout-coordinator run --config tests/smoke/coordinator.toml` boots, prints "Generated dev CA at <path>" on first run (per CONTEXT first-run UX), and listens.
    - `rollout coordinator run` and `rollout worker run` clap subcommands parse and route correctly.
    - `--help` for each subcommand prints meaningful text.

    Phase 2 keeps `rollout worker run` SIMPLE: it accepts `--worker-id`, `--config`, optional `--plugin <path>` (repeated), and an optional `--hot-reload` flag. The actual full Worker loop is Phase 2 plumbing — the smoke test (plan 02-07) defines what behavior is required: register with coordinator, publish heartbeat every 500ms, load each plugin, await SIGTERM.

    Add a `tests/cli_help.rs` integration test using `assert_cmd`:
    - `coordinator_run_help_works`: `assert_cmd::Command::cargo_bin("rollout").args(["coordinator","run","--help"]).assert().success()`.
    - `worker_run_help_works`: same for `worker run`.
  </behavior>
  <action>
    **Step 1 — `crates/rollout-coordinator/src/main.rs`** — REPLACE Wave-0 stub:
    ```rust
    //! `rollout-coordinator` binary: minimal Phase-2 control plane.
    #![forbid(unsafe_code)]
    use clap::Parser;
    use rollout_coordinator::{CoordinatorConfig, CoordinatorImpl};
    use std::path::PathBuf;
    use std::sync::Arc;

    #[derive(Parser)]
    #[command(name = "rollout-coordinator", version)]
    struct Cli {
        #[command(subcommand)]
        cmd: Sub,
    }

    #[derive(clap::Subcommand)]
    enum Sub {
        /// Boot the coordinator from a TOML config file.
        Run {
            /// Path to coordinator TOML config.
            #[arg(long)]
            config: PathBuf,
        },
    }

    #[tokio::main]
    async fn main() -> std::process::ExitCode {
        tracing_subscriber::fmt().with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        ).json().init();
        let cli = Cli::parse();
        match cli.cmd {
            Sub::Run { config } => match run(config).await {
                Ok(_) => std::process::ExitCode::SUCCESS,
                Err(e) => { tracing::error!(error = ?e, "coordinator exited with error"); std::process::ExitCode::from(2) }
            }
        }
    }

    async fn run(config_path: PathBuf) -> Result<(), rollout_core::CoreError> {
        let raw = std::fs::read_to_string(&config_path).map_err(internal)?;
        let config: CoordinatorConfig = toml::from_str(&raw).map_err(internal)?;
        config.validate().map_err(|errs| internal(format!("config invalid: {errs:?}")))?;

        // 1. Storage
        let storage = Arc::new(rollout_storage::EmbeddedStorage::open(&config.storage.path).await?);

        // 2. TLS dev CA
        let (ca_cert, ca_key) = rollout_transport::tls::ensure_dev_ca(&config.transport.tls_dir)?;
        eprintln!("Generated dev CA at {}", config.transport.tls_dir.join("ca.pem").display());
        let (srv_cert, srv_key) = rollout_transport::tls::issue_server_cert(&ca_cert, &ca_key, &["localhost".into(), "127.0.0.1".into()])?;

        // 3. Coordinator + transport wiring (D-OBSERVE-01: StdoutJsonEmitter wired here)
        let run_id_ulid: ulid::Ulid = config.run_id.parse().map_err(internal)?;
        let emitter: Arc<dyn rollout_core::EventEmitter> =
            Arc::new(rollout_coordinator::StdoutJsonEmitter::default());
        let coord = Arc::new(CoordinatorImpl::new(storage.clone(), rollout_core::RunId(run_id_ulid), emitter));
        let hb_svc = rollout_transport::channels::HeartbeatServiceImpl::new(coord.clone() as Arc<dyn rollout_core::Coordinator>);
        let ctrl_svc = rollout_transport::channels::ControlServiceImpl::new();
        let work_svc = rollout_transport::channels::WorkServiceImpl::new();

        // 4. Failure-scan loop
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let storage_clone = storage.clone();
        let interval = config.transport.heartbeat_interval;
        let skew = config.transport.clock_skew_budget;
        let coord_timeout = config.transport.coordinator_failure_timeout;
        tokio::spawn(rollout_coordinator::failure_scan::failure_scan_loop(storage_clone, interval, skew, coord_timeout, shutdown_rx));

        // 5. SIGTERM handler
        tokio::spawn(async move {
            if let Ok(mut sig) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                sig.recv().await;
                let _ = shutdown_tx.send(true);
            }
        });

        // 6. Serve
        let identity = tonic::transport::Identity::from_pem(srv_cert, srv_key);
        let client_ca = tonic::transport::Certificate::from_pem(ca_cert);
        rollout_transport::server::serve(config.transport.listen_addr, identity, client_ca, hb_svc, ctrl_svc, work_svc).await
    }

    fn internal<E: std::fmt::Display>(e: E) -> rollout_core::CoreError {
        rollout_core::CoreError::Fatal(rollout_core::FatalError::Internal(e.to_string()))
    }
    ```

    **Step 2 — `crates/rollout-cli/Cargo.toml`** — add deps (preserve existing):
    ```toml
    [dependencies]
    rollout-core         = { path = "../rollout-core" }
    rollout-coordinator  = { path = "../rollout-coordinator" }
    rollout-transport    = { path = "../rollout-transport" }
    rollout-storage      = { path = "../rollout-storage" }
    rollout-plugin-host  = { path = "../rollout-plugin-host" }
    rollout-cloud-local  = { path = "../rollout-cloud-local" }
    rollout-proto        = { path = "../rollout-proto" }
    clap                 = { workspace = true }
    serde_json           = { workspace = true }
    schemars             = { workspace = true }
    serde                = { workspace = true }
    tokio                = { workspace = true, features = ["rt-multi-thread", "macros", "signal", "process"] }
    tracing              = { workspace = true }
    tracing-subscriber   = { workspace = true }
    toml                 = { workspace = true }
    ulid                 = { workspace = true }
    tonic                = { workspace = true }
    prost-types          = { workspace = true }
    humantime            = "2"     # for pretty error messages if needed

    [dev-dependencies]
    assert_cmd  = { workspace = true }
    predicates  = { workspace = true }
    tempfile    = { workspace = true }
    ```
    NOTE: `rollout-cli` depending on `rollout-cloud-local` is fine — CLI is an application layer, not algorithm-layer (dep-direction lint forbids `rollout-algo-*` from depending on `rollout-cloud-*`; `rollout-cli` isn't in that list).

    **Step 3 — `crates/rollout-cli/src/main.rs`** — extend with subcommands, PRESERVING the existing `schema` subcommand verbatim:
    ```rust
    //! rollout CLI binary. Phase-2 adds `worker run` + `coordinator run`.
    #![forbid(unsafe_code)]
    use clap::{Parser, Subcommand, ValueEnum};
    use rollout_core::config::RunConfig;
    use std::process::ExitCode;
    use std::path::PathBuf;

    #[derive(Parser)]
    #[command(name = "rollout", version)]
    struct Cli {
        #[command(subcommand)]
        cmd: Cmd,
    }

    #[derive(Subcommand)]
    enum Cmd {
        /// Print the JSON Schema for the run config.
        Schema {
            #[arg(long, value_enum, default_value_t = SchemaFormat::Json)]
            format: SchemaFormat,
        },
        /// Worker process: register with coordinator + emit heartbeats + load plugins.
        Worker {
            #[command(subcommand)]
            sub: WorkerSub,
        },
        /// Coordinator process (Phase-2 minimal control plane).
        Coordinator {
            #[command(subcommand)]
            sub: CoordSub,
        },
    }

    #[derive(Subcommand)] enum WorkerSub { Run(WorkerRunArgs) }
    #[derive(Subcommand)] enum CoordSub  { Run(CoordRunArgs) }

    #[derive(clap::Args)]
    struct WorkerRunArgs {
        /// Path to worker TOML config.
        #[arg(long)] config: PathBuf,
        /// Worker ID. ULID. If omitted, one is generated.
        #[arg(long)] worker_id: Option<String>,
        /// Plugin manifest path(s) to load. Repeated.
        #[arg(long = "plugin")] plugins: Vec<PathBuf>,
        /// Enable hot-reload for PyO3 + sidecar plugins.
        #[arg(long)] hot_reload: bool,
    }
    #[derive(clap::Args)]
    struct CoordRunArgs {
        /// Path to coordinator TOML config.
        #[arg(long)] config: PathBuf,
    }

    #[derive(Copy, Clone, ValueEnum)]
    enum SchemaFormat { Json, Pretty }

    fn main() -> ExitCode {
        let cli = Cli::parse();
        match cli.cmd {
            Cmd::Schema { format } => schema(format),
            Cmd::Coordinator { sub: CoordSub::Run(a) } => coord_run(a),
            Cmd::Worker { sub: WorkerSub::Run(a) } => worker_run(a),
        }
    }

    fn schema(format: SchemaFormat) -> ExitCode {
        // ... existing Phase-1 body verbatim ...
        let schema = schemars::schema_for!(RunConfig);
        let out = match format {
            SchemaFormat::Json => serde_json::to_string(&schema),
            SchemaFormat::Pretty => serde_json::to_string_pretty(&schema),
        };
        match out {
            Ok(s) => { println!("{s}"); ExitCode::SUCCESS }
            Err(e) => { eprintln!("schema serialize failed: {e}"); ExitCode::from(2) }
        }
    }

    fn coord_run(args: CoordRunArgs) -> ExitCode {
        // Spawn a tokio runtime + call the same logic as the rollout-coordinator binary.
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        rt.block_on(async move {
            let raw = match std::fs::read_to_string(&args.config) { Ok(s) => s, Err(e) => { eprintln!("read config: {e}"); return ExitCode::from(2); } };
            let cfg: rollout_coordinator::CoordinatorConfig = match toml::from_str(&raw) { Ok(c) => c, Err(e) => { eprintln!("parse: {e}"); return ExitCode::from(2); } };
            // For simplicity, delegate to a `pub async fn run` exposed from rollout-coordinator's lib.
            // ... call rollout_coordinator::run(cfg).await ...
            ExitCode::SUCCESS
        })
    }

    fn worker_run(args: WorkerRunArgs) -> ExitCode {
        // Phase-2 worker loop: open Storage, build PluginHostImpl, register with coord, emit heartbeats every interval, load plugins, await SIGTERM.
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        rt.block_on(async move {
            // ... see Step 4 for the worker body sketch ...
            ExitCode::SUCCESS
        })
    }
    ```

    **Step 4 — Worker body sketch (Phase-2 minimum):**
    1. Read `args.config` as `WorkerConfig { run_id, coordinator_addr, transport: TransportConfig, plugin_dir? }`.
    2. worker_id = `args.worker_id` parsed-or-generated.
    3. Build a `tonic::transport::Channel` to the coordinator using the dev CA + per-worker mTLS cert (issued via `rollout_transport::tls::issue_client_cert`).
    4. Build a `HeartbeatClient<Channel>` and call `register` (via Coordinator gRPC service if exposed; OR via direct in-process Arc<dyn Coordinator> if running embedded — for Phase 2 smoke, real over-the-wire is the test). NOTE: spec 05 / proto-defined services don't include a `register` RPC — registration happens via the first heartbeat. Refactor: skip explicit register; the first heartbeat with state=Init is the registration signal. The CoordinatorImpl handles "heartbeat for unknown worker" by silently auto-registering (extend CoordinatorImpl::heartbeat to write a workers/<id> entry on first sight). Update Task 1's test if needed.

       Alternative simpler: keep `register` in the Rust trait but DON'T expose it over gRPC; the over-the-wire path uses heartbeats as both registration and liveness. CoordinatorImpl::heartbeat checks workers/<id> existence and upserts.

    5. Build `PluginHostImpl::with_storage(storage)`; for each `--plugin` arg, parse manifest TOML, call `host.load(manifest)`.
    6. Spawn a heartbeat loop: every `heartbeat_interval`, send `BeatRequest { worker_id, due_at: now + 2*hb_interval, state, run_id }`.
    7. Await SIGTERM; on receipt, send a final heartbeat with state=Draining; unload plugins; exit 0.

    Acceptable to leave the worker body partially-stubbed for this plan IF the smoke test (plan 02-07) defines the missing pieces — but the smoke test is the SUBSTR-02 acceptance gate, so the worker MUST publish heartbeats correctly. Implement the loop fully.

    **Step 4b — `crates/rollout-coordinator/src/emitter.rs`** (NEW) — the StdoutJsonEmitter impl D-OBSERVE-01 requires:
    ```rust
    use async_trait::async_trait;
    use rollout_core::{CoreError, Event, EventEmitter, FatalError};
    use tokio::io::AsyncWriteExt;
    use tokio::sync::Mutex;

    /// One-NDJSON-line-per-event emitter on stdout. Locks an internal mutex so
    /// concurrent emitters don't interleave bytes within a line.
    pub struct StdoutJsonEmitter { inner: Mutex<tokio::io::Stdout> }

    impl Default for StdoutJsonEmitter {
        fn default() -> Self { Self { inner: Mutex::new(tokio::io::stdout()) } }
    }

    #[async_trait]
    impl EventEmitter for StdoutJsonEmitter {
        async fn emit(&self, event: Event) -> Result<(), CoreError> {
            let mut line = serde_json::to_vec(&event)
                .map_err(|e| CoreError::Fatal(FatalError::Internal(format!("event serialize: {e}"))))?;
            line.push(b'\n');
            let mut out = self.inner.lock().await;
            out.write_all(&line).await
                .map_err(|e| CoreError::Fatal(FatalError::Internal(format!("stdout write: {e}"))))?;
            Ok(())
        }
    }
    ```
    Re-export from `src/lib.rs`: `pub mod emitter; pub use emitter::StdoutJsonEmitter;`.

    **Step 4c — wire StdoutJsonEmitter in `src/main.rs` `run()`** between Storage open and transport serve:
    - Build `let emitter: std::sync::Arc<dyn rollout_core::EventEmitter> = std::sync::Arc::new(StdoutJsonEmitter::default());`.
    - Pass `emitter` into `CoordinatorImpl::new(...)` (extend the constructor signature) so `register` / `deregister` / `heartbeat` each call `emitter.emit(Event { ... })` alongside the existing `tracing::*!` lines.
    - Update Task 1's CoordinatorImpl to take an `emitter: Arc<dyn EventEmitter>` field and emit a `Domain { topic: "worker_registered" }`-shaped `Event` on register, `Domain { topic: "worker_heartbeat" }` on heartbeat, and (from `failure_scan_loop`) `Domain { topic: "worker_failed" }` when the deadline trips. The tracing lines stay as before; this is additive.
    - For tests in Task 1, pass `Arc::new(NoopEmitter::default())` (define a `pub struct NoopEmitter` in `emitter.rs` that returns `Ok(())` — useful for unit tests too). Task 1's existing `<acceptance_criteria>` and tests stay valid; the executor needs to extend the constructor call sites only.

        **Step 5 — `docs/book/src/substrate/coordinator.md`** (NEW, ~80 lines):
    - **Phase-2 scope** — heartbeat receiver + worker registry + deadline scan. NOT work distribution or lease.
    - **Deferred to Phase 6** — DIST-01..05.
    - **Storage layout** — namespaces `workers` and `heartbeats`.
    - **Failure detection formula** — `is_failed = elapsed > skew AND elapsed > coord_timeout`.
    - **CLI** — `rollout coordinator run --config <path>` and `rollout worker run --config <path> --plugin <manifest>`.
    - **First-run UX** — TLS dev CA auto-generated.

    **Step 6 — `docs/book/src/SUMMARY.md`** extend:
    ```markdown
    - [Substrate](./substrate/index.md)
      - [Proto crate](./substrate/proto.md)
      - [Storage](./substrate/storage.md)
      - [Cloud-local](./substrate/cloud-local.md)
      - [Transport](./substrate/transport.md)
      - [Plugin host](./substrate/plugin-host.md)
      - [Python bridge](./substrate/python-bridge.md)
      - [Coordinator](./substrate/coordinator.md)
    ```

    **Step 7 — tests/cli_help.rs:** assert_cmd-based help tests per `<behavior>`.
  </action>
  <verify>
    <automated>cargo build -p rollout-coordinator -p rollout-cli &amp;&amp; cargo test -p rollout-cli --tests &amp;&amp; cargo test -p rollout-coordinator --tests &amp;&amp; cargo clippy -p rollout-coordinator -p rollout-cli --all-targets -- -D warnings &amp;&amp; mdbook build docs/book</automated>
  </verify>
  <acceptance_criteria>
    - `crates/rollout-coordinator/src/main.rs` contains `tonic::transport::Server` wiring through `rollout_transport::server::serve`
    - `crates/rollout-cli/src/main.rs` contains `Worker { sub }` and `Coordinator { sub }` enum variants; the existing `Schema` variant is preserved
    - `cargo build -p rollout-coordinator --bin rollout-coordinator` exits 0
    - `cargo build -p rollout-cli` exits 0
    - `cargo test -p rollout-cli --tests` exits 0 (cli_help tests pass)
    - `cargo run --bin rollout-coordinator -- run --help` exits 0 and prints flag descriptions
    - `cargo run -p rollout-cli -- coordinator run --help` exits 0
    - `cargo run -p rollout-cli -- worker run --help` exits 0
    - `crates/rollout-coordinator/src/emitter.rs` contains `impl EventEmitter for StdoutJsonEmitter` and `pub struct NoopEmitter`
    - The coordinator binary `run()` constructs an `Arc<dyn rollout_core::EventEmitter>` and passes it into `CoordinatorImpl::new`
    - Running `cargo run --bin rollout-coordinator -- run --config tests/smoke/coordinator.toml` (smoke setup) writes at least one NDJSON line containing `"topic":"worker_heartbeat"` to stdout after a worker beats (validated implicitly by the smoke script in plan 02-07; not a standalone test here)
    - `docs/book/src/substrate/coordinator.md` exists; `mdbook build docs/book` exits 0
    - `docs/book/src/SUMMARY.md` references `./substrate/coordinator.md`
    - DOCS-02 satisfied
  </acceptance_criteria>
  <done>
    Coordinator binary boots end-to-end (Storage open + TLS dev CA + transport.serve + failure-scan loop); rollout-cli has worker/coordinator subcommands; substrate coordinator mdBook chapter ships.
  </done>
</task>

</tasks>

<verification>
```bash
cargo build -p rollout-coordinator -p rollout-cli
cargo test -p rollout-coordinator --tests
cargo test -p rollout-cli --tests
cargo clippy -p rollout-coordinator -p rollout-cli --all-targets -- -D warnings
cargo doc -p rollout-coordinator --no-deps --all-features
cargo run --bin rollout-coordinator -- run --help
cargo run -p rollout-cli -- coordinator run --help
cargo run -p rollout-cli -- worker run --help
cargo run -p rollout-cli -- schema --format json | head -c 200
mdbook build docs/book
```
All exit 0; the existing Phase-1 `rollout schema` subcommand still works.
</verification>

<success_criteria>
- Coordinator binary boots from a TOML config and serves the three transport channels
- Worker registry + heartbeat ledger persist in Storage namespaces `workers` and `heartbeats`
- Deadline-based failure scan emits `worker_failed` tracing events
- rollout-cli gains `worker run` + `coordinator run` while keeping `schema`
- Substrate coordinator mdBook chapter ships
- Phase-2 explicitly scopes out work distribution / lease / multi-coordinator (Phase 6)
- Coordinator binary instantiates `StdoutJsonEmitter` and emits NDJSON events (D-OBSERVE-01); CoordinatorImpl takes a generic `Arc<dyn EventEmitter>` so non-stdout backends drop in without code change in later phases
</success_criteria>

<output>
After completion, create `.planning/phases/02-local-substrate/02-06-rollout-coordinator-SUMMARY.md` documenting:
- Whether registration is explicit-RPC or implicit-via-first-heartbeat (decision taken at exec time)
- Failure-scan interval choice (using `heartbeat_interval` as scan tick rate, per Step 1 plumbing)
- How rollout-cli's `coord_run` delegates to rollout-coordinator (re-export `pub async fn run` from the library vs duplicated logic)
- Open questions for plan 02-07 (smoke test) — exact worker.toml + coordinator.toml fixtures the smoke script consumes
</output>
