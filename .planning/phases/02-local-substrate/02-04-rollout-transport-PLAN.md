---
phase: 02-local-substrate
plan: 04
type: execute
wave: 3
depends_on: [02-00, 02-01]
files_modified:
  - crates/rollout-transport/Cargo.toml
  - crates/rollout-transport/src/lib.rs
  - crates/rollout-transport/src/config.rs
  - crates/rollout-transport/src/tls.rs
  - crates/rollout-transport/src/server.rs
  - crates/rollout-transport/src/client.rs
  - crates/rollout-transport/src/channels/mod.rs
  - crates/rollout-transport/src/channels/heartbeat.rs
  - crates/rollout-transport/src/channels/control.rs
  - crates/rollout-transport/src/channels/work.rs
  - crates/rollout-transport/src/health.rs
  - crates/rollout-transport/tests/tls_dev_ca.rs
  - crates/rollout-transport/tests/heartbeat.rs
  - crates/rollout-transport/tests/control_stream.rs
  - crates/rollout-transport/tests/config_invariants.rs
  - docs/book/src/substrate/transport.md
  - docs/book/src/SUMMARY.md
autonomous: true
requirements: [SUBSTR-02, DOCS-01, DOCS-02, DOCS-03]
must_haves:
  truths:
    - "rollout-transport ships HTTP/2 tonic + rustls 0.23 as the plan-of-record per RESEARCH (tonic-h3 is experimental)."
    - "mTLS auto-bootstrap: on first run, rcgen generates dev CA + server/client certs under ./data/tls/ (gitignored)."
    - "Three logical channels — Heartbeat (unary), Control (server-stream), Work (bidi) — share the same H/2 connection."
    - "Heartbeat deadline math: due_at = now + heartbeat_interval × 2; coordinator marks failed when elapsed > skew AND > coord_failure_timeout."
    - "Config invariants (worker_self_fence < coord_failure_timeout; clock_skew_budget < heartbeat_interval × 2) fail at plan-validate time, not runtime."
    - "QUIC is feature-flagged (`quic`); the default build does NOT pull in quinn/tonic-h3."
  artifacts:
    - path: crates/rollout-transport/src/tls.rs
      provides: "rcgen-based dev CA + per-worker cert generator with on-disk persistence"
      contains: "fn ensure_dev_ca"
    - path: crates/rollout-transport/src/server.rs
      provides: "Server::builder wiring with mTLS; cfg-switches H/2 vs QUIC"
      contains: "pub async fn serve"
    - path: crates/rollout-transport/src/channels/heartbeat.rs
      provides: "Server-side Heartbeat impl + due_at handling"
      contains: "impl Heartbeat for"
    - path: crates/rollout-transport/src/config.rs
      provides: "TransportConfig with cross-field invariants validated at plan time"
      contains: "fn validate_cross_fields"
  key_links:
    - from: crates/rollout-transport/src/server.rs
      to: rollout_proto::transport::v1
      via: "tonic-generated HeartbeatServer / ControlServer"
      pattern: "HeartbeatServer|ControlServer"
    - from: crates/rollout-transport/src/tls.rs
      to: "./data/tls/{ca,server,client}.{pem,key.pem}"
      via: "rcgen::generate_simple_self_signed"
      pattern: "rcgen"
    - from: crates/rollout-transport/src/config.rs
      to: "RunConfig (Phase 1) → TransportConfig"
      via: "schemars JsonSchema derive + validate_cross_fields"
      pattern: "validate_cross_fields"
---

<objective>
Implement `rollout-transport` — the **HTTP/2 tonic + rustls** plan-of-record per RESEARCH (tonic-h3 v0.0.5 is experimental and stays behind a `quic` Cargo feature). Wire three logical channels (Heartbeat unary, Control server-stream, Work bidi-stream) using the tonic-generated types from `rollout-proto`. mTLS auto-bootstraps via `rcgen` writing the dev CA + per-host certs under `./data/tls/`.

Purpose: SUBSTR-02 deliverable. The coordinator (plan 02-06) and worker (extended in plan 02-07 smoke) BOTH depend on this crate. The H/2-vs-QUIC swap path is preserved as a single Cargo feature flag toggle.

Output: `cargo test -p rollout-transport --tests` green for tls_dev_ca, heartbeat, control_stream, config_invariants. The Work bidi channel ships as a wired-but-stub `WorkService` (Phase 6 actually puts items through it).
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
@.planning/phases/02-local-substrate/02-01-rollout-proto-PLAN.md
@docs/specs/05-distribution.md
@crates/rollout-core/src/traits/worker.rs
@crates/rollout-proto/src/lib.rs
@crates/rollout-proto/proto/transport.proto
@Cargo.toml

<interfaces>
Consumed from rollout-proto (compiled by tonic-build in plan 02-01):
```rust
// rollout_proto::transport::v1
pub mod heartbeat_server { pub struct HeartbeatServer<T>(...); }
pub mod heartbeat_client { pub struct HeartbeatClient<T>; }
pub trait Heartbeat: Send + Sync + 'static {
    async fn beat(&self, r: tonic::Request<BeatRequest>) -> Result<tonic::Response<BeatResponse>, tonic::Status>;
}
pub struct BeatRequest { worker_id: String, due_at: Option<prost_types::Timestamp>, state: i32, run_id: String }
pub struct BeatResponse { acknowledged_at_drift: Option<prost_types::Duration>, pending_control: Option<ControlPush> }

pub mod control_server { pub struct ControlServer<T>; }
pub trait Control: Send + Sync + 'static { /* server-stream */ }

pub mod work_server { pub struct WorkServer<T>; }
pub trait Work: Send + Sync + 'static { /* bidi-stream */ }
```

Consumed from rollout-core:
```rust
pub struct Heartbeat /* domain type, NOT proto type */ { worker_id, run_id, state: WorkerState, due_at: SystemTime }
pub enum WorkerState { Init, Ready, Running, Draining }
#[async_trait] pub trait Coordinator { async fn heartbeat(&self, hb: Heartbeat) -> Result<(), CoreError>; }
```

NOTE on naming: there are TWO `Heartbeat` symbols. `rollout_core::Heartbeat` is the domain struct; `rollout_proto::transport::v1::Heartbeat` is the tonic-generated SERVICE trait. The transport crate converts between them in `channels/heartbeat.rs::HeartbeatServiceImpl`.

Versions (from plan 02-00 workspace pins):
- tonic = 0.14 with features ["tls-rustls", "transport"]
- rustls = 0.23 with features ["ring", "std"]
- rcgen = 0.13
- prost-types = 0.13
</interfaces>
</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: Cargo.toml + TLS dev-CA + TransportConfig with plan-time invariants</name>
  <files>
    crates/rollout-transport/Cargo.toml,
    crates/rollout-transport/src/lib.rs,
    crates/rollout-transport/src/config.rs,
    crates/rollout-transport/src/tls.rs,
    crates/rollout-transport/src/health.rs,
    crates/rollout-transport/tests/tls_dev_ca.rs,
    crates/rollout-transport/tests/config_invariants.rs
  </files>
  <read_first>
    - crates/rollout-transport/Cargo.toml (Wave-0 stub)
    - crates/rollout-transport/src/lib.rs (Wave-0 stub)
    - .planning/phases/02-local-substrate/02-CONTEXT.md D-TRANS-01..03 + D-TIME-01..02
    - .planning/phases/02-local-substrate/02-RESEARCH.md §"Transport stack" + §"Code Examples / mTLS server with rcgen-generated dev CA" + §"Pattern 5: deadline-based health"
    - docs/specs/05-distribution.md §3 (channels) + §6 (deadline-based health)
    - Cargo.toml workspace deps for tonic / rustls / rcgen / prost / prost-types / humantime-serde
  </read_first>
  <behavior>
    RED first:

    `tests/tls_dev_ca.rs`:
    - `ensure_dev_ca_creates_files`: `ensure_dev_ca(tmpdir)` writes `ca.pem`, `ca.key.pem`, and the key file has 0o600 perms (`Permissions::mode() & 0o777 == 0o600`).
    - `ensure_dev_ca_is_idempotent`: second call returns same bytes as first; no re-generation of the key.
    - `issue_server_cert_works`: from a generated CA, issue a server cert for "localhost"; verify the returned PEM parses via `rustls_pemfile::certs`.

    `tests/config_invariants.rs`:
    - `default_config_passes_validation`: `TransportConfig::default().validate_cross_fields()` returns Ok.
    - `self_fence_must_be_less_than_coord_failure`: build a config where `worker_self_fence_timeout = 6s` and `coordinator_failure_timeout = 5s`; validate_cross_fields returns `Err(...)` containing the string "split-brain prevention".
    - `clock_skew_must_be_less_than_2x_heartbeat`: set `clock_skew_budget = 2s` and `heartbeat_interval = 500ms`; validate returns Err containing "clock_skew_budget".
    - `defaults_match_d_time_01`: assert default heartbeat_interval=500ms, self_fence=4s, coord_failure=5s, clock_skew=250ms (from CONTEXT D-TIME-01).

    `tests/health.rs` (NEW module that's exported from lib.rs — can be a unit test instead of integration; if unit, drop the file from `tests/`):
    - `next_due_at_adds_two_intervals`
    - `is_failed_only_when_both_thresholds_passed`

    GREEN: implement.
  </behavior>
  <action>
    **Step 1 — `crates/rollout-transport/Cargo.toml`:**
    ```toml
    [package]
    name = "rollout-transport"
    version.workspace = true
    edition.workspace = true
    license.workspace = true
    rust-version.workspace = true
    repository.workspace = true

    [lints]
    workspace = true

    [features]
    default = ["h2"]
    # HTTP/2 plan-of-record. Always on by default.
    h2 = []
    # EXPERIMENTAL: gRPC-over-QUIC via tonic-h3 v0.0.x. Bidi-streaming not
    # documented as supported as of 2025-11-01. Do NOT use in production.
    # See RESEARCH §"Pitfall 2".
    quic = ["dep:quinn", "dep:tonic-h3", "dep:h3", "dep:h3-quinn"]

    [dependencies]
    rollout-core   = { path = "../rollout-core" }
    rollout-proto  = { path = "../rollout-proto" }
    async-trait    = { workspace = true }
    serde          = { workspace = true }
    schemars       = { workspace = true }
    thiserror      = { workspace = true }
    tracing        = { workspace = true }
    tokio          = { workspace = true }
    tokio-stream   = { workspace = true }
    humantime-serde = { workspace = true }
    tonic          = { workspace = true }
    prost          = { workspace = true }
    prost-types    = { workspace = true }
    rustls         = { workspace = true }
    rcgen          = { workspace = true }
    bytes          = { workspace = true }

    # quic stretch
    quinn      = { version = "0.11", optional = true }
    tonic-h3   = { version = "0.0.5", optional = true }
    h3         = { version = "0.0.6", optional = true }
    h3-quinn   = { version = "0.0.7", optional = true }

    [package.metadata.cargo-machete]
    ignored = ["quinn", "tonic-h3", "h3", "h3-quinn"]

    [dev-dependencies]
    tempfile = { workspace = true }
    tokio = { workspace = true, features = ["macros", "rt-multi-thread"] }
    rustls-pemfile = "2"
    ```

    **Step 2 — `src/lib.rs`:**
    ```rust
    //! `rollout-transport` — HTTP/2 tonic + rustls gRPC plane with mTLS by default.
    //!
    //! Three logical channels: Heartbeat (unary), Control (server-stream), Work (bidi).
    //! QUIC via `tonic-h3` is behind the `quic` Cargo feature; default build is H/2 only.
    //! See `docs/book/src/substrate/transport.md` for the plan-of-record rationale.
    #![forbid(unsafe_code)]

    pub mod channels;
    pub mod client;
    pub mod config;
    pub mod health;
    pub mod server;
    pub mod tls;

    pub use config::TransportConfig;
    ```

    **Step 3 — `src/config.rs`:**
    ```rust
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    use std::net::SocketAddr;
    use std::path::PathBuf;
    use std::time::Duration;

    /// Phase-2 transport config. Defaults match CONTEXT D-TIME-01.
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct TransportConfig {
        /// Address the coordinator binds. Default: 127.0.0.1:50051
        #[serde(default = "defaults::listen")]
        pub listen_addr: SocketAddr,
        /// Directory holding the dev CA + per-host certs (gitignored).
        #[serde(default = "defaults::tls_dir")]
        pub tls_dir: PathBuf,
        /// Heartbeat publish interval (D-TIME-01: 500ms).
        #[serde(default = "defaults::hb_interval", with = "humantime_serde")]
        pub heartbeat_interval: Duration,
        /// Worker self-fences after this many missed heartbeats (D-TIME-01: 4s).
        #[serde(default = "defaults::self_fence", with = "humantime_serde")]
        pub worker_self_fence_timeout: Duration,
        /// Coordinator marks worker failed after this elapsed past `due_at` (D-TIME-01: 5s).
        #[serde(default = "defaults::coord_timeout", with = "humantime_serde")]
        pub coordinator_failure_timeout: Duration,
        /// Allowed clock-skew between worker and coordinator (D-TIME-01: 250ms).
        #[serde(default = "defaults::skew", with = "humantime_serde")]
        pub clock_skew_budget: Duration,
    }

    impl Default for TransportConfig { fn default() -> Self { /* call defaults::* */ } }

    impl TransportConfig {
        /// Plan-time invariants. Run by `rollout plan` before any worker starts.
        ///
        /// Enforces:
        /// 1. `worker_self_fence_timeout < coordinator_failure_timeout` (split-brain prevention; spec 05 §6).
        /// 2. `clock_skew_budget < heartbeat_interval × 2`.
        pub fn validate_cross_fields(&self) -> Result<(), Vec<String>> {
            let mut errs = Vec::new();
            if self.worker_self_fence_timeout >= self.coordinator_failure_timeout {
                errs.push("transport.worker_self_fence_timeout must be strictly less than transport.coordinator_failure_timeout (split-brain prevention)".into());
            }
            if self.clock_skew_budget >= self.heartbeat_interval * 2 {
                errs.push("transport.clock_skew_budget must be less than 2 × transport.heartbeat_interval".into());
            }
            if errs.is_empty() { Ok(()) } else { Err(errs) }
        }
    }

    mod defaults {
        use super::*;
        pub fn listen() -> SocketAddr { "127.0.0.1:50051".parse().unwrap() }
        pub fn tls_dir() -> PathBuf { PathBuf::from("./data/tls") }
        pub fn hb_interval() -> Duration { Duration::from_millis(500) }
        pub fn self_fence() -> Duration { Duration::from_secs(4) }
        pub fn coord_timeout() -> Duration { Duration::from_secs(5) }
        pub fn skew() -> Duration { Duration::from_millis(250) }
    }
    ```

    **Step 4 — `src/health.rs`** — `next_due_at` + `is_failed` helpers per RESEARCH Pattern 5; unit-tested.

    **Step 5 — `src/tls.rs`** — `ensure_dev_ca(dir: &Path) -> Result<(Vec<u8>, Vec<u8>), CoreError>` and `issue_server_cert(ca_cert, ca_key, names: &[String]) -> Result<(Vec<u8>, Vec<u8>), CoreError>` per RESEARCH §"Code Examples / mTLS server". Use `rcgen 0.13` API:
    ```rust
    use rcgen::{Certificate, CertificateParams, IsCa, KeyPair, BasicConstraints};
    pub fn ensure_dev_ca(dir: &std::path::Path) -> Result<(Vec<u8>, Vec<u8>), rollout_core::CoreError> { ... }
    ```
    On macOS/Linux, write key files with 0o600 permissions (`std::os::unix::fs::PermissionsExt::set_mode`).

    **Step 6 — `tests/tls_dev_ca.rs` + `tests/config_invariants.rs`** per `<behavior>`.

    rcgen 0.13 API notes (RESEARCH §"Code Examples"):
    ```rust
    let mut params = CertificateParams::new(vec!["rollout-dev-ca".into()]).unwrap();
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let kp = KeyPair::generate().map_err(...)?;
    let cert = params.self_signed(&kp).map_err(...)?;
    let cert_pem = cert.pem();
    let key_pem = kp.serialize_pem();
    ```
    Verify this API against rcgen 0.13 docs at impl time — if the surface drifted between RESEARCH (2026-05-19) and exec time, adapt; the test is what proves correctness.
  </action>
  <verify>
    <automated>cargo build -p rollout-transport &amp;&amp; cargo test -p rollout-transport --test tls_dev_ca &amp;&amp; cargo test -p rollout-transport --test config_invariants &amp;&amp; cargo clippy -p rollout-transport --all-targets --features h2 -- -D warnings</automated>
  </verify>
  <acceptance_criteria>
    - `crates/rollout-transport/src/tls.rs` contains `pub fn ensure_dev_ca` and `pub fn issue_server_cert`
    - `crates/rollout-transport/src/config.rs` contains `pub struct TransportConfig` and `pub fn validate_cross_fields`
    - `crates/rollout-transport/Cargo.toml` `[features]` block has `default = ["h2"]` and `quic = [...]`; quinn/tonic-h3 are optional deps
    - `cargo test -p rollout-transport --test tls_dev_ca` exits 0 (3 tests pass)
    - `cargo test -p rollout-transport --test config_invariants` exits 0 (4 tests pass)
    - Generated `ca.key.pem` has 0o600 permissions on unix
    - `cargo build -p rollout-transport` (default features = H/2) does NOT pull in quinn or tonic-h3 — verify via `cargo tree -p rollout-transport | grep -E 'quinn|tonic-h3'` returns nothing
    - DOCS-02: tls.rs + config.rs have inline `///` docs; tests authored same commit
  </acceptance_criteria>
  <done>
    TLS dev-CA pipeline works; plan-time config invariants enforce split-brain prevention; QUIC is opt-in.
  </done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: Heartbeat/Control/Work channel servers + client builder + mdBook chapter</name>
  <files>
    crates/rollout-transport/src/channels/mod.rs,
    crates/rollout-transport/src/channels/heartbeat.rs,
    crates/rollout-transport/src/channels/control.rs,
    crates/rollout-transport/src/channels/work.rs,
    crates/rollout-transport/src/server.rs,
    crates/rollout-transport/src/client.rs,
    crates/rollout-transport/tests/heartbeat.rs,
    crates/rollout-transport/tests/control_stream.rs,
    docs/book/src/substrate/transport.md,
    docs/book/src/SUMMARY.md
  </files>
  <read_first>
    - crates/rollout-transport/src/lib.rs (Task 1 output — pub mod slots)
    - crates/rollout-proto/proto/transport.proto (Wave-1 proto)
    - crates/rollout-proto/src/lib.rs (Wave-1 — tonic::include_proto)
    - crates/rollout-core/src/traits/worker.rs (Heartbeat domain struct + WorkerState)
    - .planning/phases/02-local-substrate/02-RESEARCH.md §"Pattern 5: deadline-based health" + §"Pitfall 2: tonic-h3 v0.0.5 silently lacks bidi-streaming"
    - docs/specs/05-distribution.md §3 + §6
  </read_first>
  <behavior>
    RED first:

    `tests/heartbeat.rs`:
    - `heartbeat_unary_roundtrip`: spin up a HeartbeatServer on a randomly-bound port with a fake Coordinator that captures incoming heartbeats; HeartbeatClient sends one BeatRequest; server returns BeatResponse; the captured Heartbeat has the expected worker_id + due_at within 1ms.
    - `heartbeat_mtls_handshake_passes_with_dev_ca`: use `ensure_dev_ca + issue_server_cert + issue_client_cert` from Task 1; both server and client identities chain to the dev CA; the unary call succeeds. (If full mTLS proves to be > 60min, mark this `#[ignore]` and substitute a plaintext H/2 happy-path test that's enough for SUBSTR-02 acceptance.)
    - `heartbeat_due_at_round_trip_preserves_systemtime`: convert SystemTime → prost_types::Timestamp → BeatRequest, send, receive, convert back; equality within 1ms.

    `tests/control_stream.rs`:
    - `control_subscribe_receives_pushed_event`: client subscribes; server pushes one `ControlPush { drain: ... }`; client receives it.
    - `control_close_when_server_drops_sender`: client subscribes; server drops the tx side; client receives `Err(Status::cancelled)` or stream end.

    GREEN: implement.
  </behavior>
  <action>
    **Step 1 — `src/channels/mod.rs`:** re-export the three submodules + a `Channels` struct that bundles all three service implementations for use by `server.rs`.

    **Step 2 — `src/channels/heartbeat.rs`** — server impl:
    ```rust
    use async_trait::async_trait;
    use rollout_core::{Coordinator, Heartbeat as CoreHeartbeat, WorkerId, RunId, WorkerState};
    use rollout_proto::transport::v1::{
        heartbeat_server::Heartbeat as HeartbeatSvc,
        BeatRequest, BeatResponse,
        WorkerState as ProtoState,
    };
    use std::sync::Arc;

    /// Bridges the gRPC `Heartbeat` service to `rollout_core::Coordinator::heartbeat`.
    pub struct HeartbeatServiceImpl { coord: Arc<dyn Coordinator> }

    impl HeartbeatServiceImpl {
        pub fn new(coord: Arc<dyn Coordinator>) -> Self { Self { coord } }
    }

    #[async_trait]
    impl HeartbeatSvc for HeartbeatServiceImpl {
        async fn beat(&self, req: tonic::Request<BeatRequest>) -> Result<tonic::Response<BeatResponse>, tonic::Status> {
            let r = req.into_inner();
            let worker_id = r.worker_id.parse::<ulid::Ulid>()
                .map(WorkerId).map_err(|e| tonic::Status::invalid_argument(e.to_string()))?;
            let run_id = r.run_id.parse::<ulid::Ulid>()
                .map(RunId).map_err(|e| tonic::Status::invalid_argument(e.to_string()))?;
            let due_at = r.due_at.map(prost_to_system).unwrap_or_else(std::time::SystemTime::now);
            let state = proto_to_state(r.state);
            let hb = CoreHeartbeat { worker_id, run_id, state, due_at };
            self.coord.heartbeat(hb).await
                .map_err(|e| tonic::Status::internal(format!("{e:?}")))?;
            Ok(tonic::Response::new(BeatResponse { acknowledged_at_drift: None, pending_control: None }))
        }
    }

    fn proto_to_state(s: i32) -> WorkerState {
        match ProtoState::try_from(s) {
            Ok(ProtoState::Init)     => WorkerState::Init,
            Ok(ProtoState::Ready)    => WorkerState::Ready,
            Ok(ProtoState::Running)  => WorkerState::Running,
            Ok(ProtoState::Draining) => WorkerState::Draining,
            _ => WorkerState::Init,
        }
    }

    fn prost_to_system(t: prost_types::Timestamp) -> std::time::SystemTime {
        std::time::UNIX_EPOCH + std::time::Duration::new(t.seconds.max(0) as u64, t.nanos.max(0) as u32)
    }

    pub fn system_to_prost(t: std::time::SystemTime) -> prost_types::Timestamp {
        let d = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        prost_types::Timestamp { seconds: d.as_secs() as i64, nanos: d.subsec_nanos() as i32 }
    }
    ```

    **Step 3 — `src/channels/control.rs`** — Control server stream. Simple Phase-2 impl: per-worker mpsc::channel(64); server.subscribe takes worker_id; returns a stream wrapping `ReceiverStream`. The fan-out side (`push_drain(worker_id, ...)`) is exposed for the Coordinator (plan 02-06) to call. Phase-2 scope is just round-trip; actual drain orchestration is Phase 6.

    **Step 4 — `src/channels/work.rs`** — Work bidi stub. Implement `Work::stream` that echoes back to keep the type compiled and the integration smoke test (plan 02-07) happy. Document at top: "Phase 2 ships a wired stub; pull/submit semantics arrive in Phase 6 DIST-01..02."

    **Step 5 — `src/server.rs`** — `pub async fn serve(addr, tls_id, client_ca, channels) -> Result<(), CoreError>`:
    ```rust
    pub async fn serve(
        addr: SocketAddr,
        identity: tonic::transport::Identity,
        client_ca: tonic::transport::Certificate,
        hb: channels::HeartbeatServiceImpl,
        ctrl: channels::ControlServiceImpl,
        work: channels::WorkServiceImpl,
    ) -> Result<(), CoreError> {
        let tls = tonic::transport::ServerTlsConfig::new()
            .identity(identity).client_ca_root(client_ca);
        tonic::transport::Server::builder()
            .tls_config(tls).map_err(internal)?
            .add_service(rollout_proto::transport::v1::heartbeat_server::HeartbeatServer::new(hb))
            .add_service(rollout_proto::transport::v1::control_server::ControlServer::new(ctrl))
            .add_service(rollout_proto::transport::v1::work_server::WorkServer::new(work))
            .serve(addr).await.map_err(internal)
    }
    ```
    Behind `#[cfg(feature = "quic")]`, define a `serve_quic` variant that uses `tonic-h3` instead. EXPERIMENTAL feature documented inline.

    **Step 6 — `src/client.rs`** — `pub fn build_channel(addr, ca_pem, client_id, client_key) -> tonic::transport::Channel` wired through tonic 0.14's `Endpoint::from_shared(addr)?.tls_config(...)?.connect_lazy()`.

    **Step 7 — `tests/heartbeat.rs` and `tests/control_stream.rs`** per `<behavior>`. For the round-trip tests, the test fixture builds a tiny `FakeCoordinator { received: Arc<Mutex<Vec<CoreHeartbeat>>> }` that records every call. The test then asserts the received heartbeat matches what was sent.

    Bind to `127.0.0.1:0` and read the ephemeral port via `TcpListener::local_addr()` to avoid port races (see how tonic's own UDS+H/2 examples do this).

    **Step 8 — `docs/book/src/substrate/transport.md`** (NEW, ~120 lines):
    - **Plan-of-record: H/2 + rustls** — why (RESEARCH evidence: tonic-h3 0.0.5 experimental, hyperium/tonic#339 open).
    - **Three channels** with proto refs.
    - **mTLS auto-bootstrap** — `./data/tls/`; gitignored.
    - **Deadline-based health** — `due_at = now + 2 × heartbeat_interval`; coordinator scan formula.
    - **Config invariants** — split-brain prevention enforced at plan-time.
    - **QUIC feature flag** — EXPERIMENTAL; re-evaluate at Phase 6.
    - **Channel: Work** — Phase-2 stub; full pull/submit in Phase 6.

    **Step 9 — `docs/book/src/SUMMARY.md`** add transport chapter:
    ```markdown
    - [Substrate](./substrate/index.md)
      - [Proto crate](./substrate/proto.md)
      - [Storage](./substrate/storage.md)
      - [Cloud-local](./substrate/cloud-local.md)
      - [Transport](./substrate/transport.md)
    ```
  </action>
  <verify>
    <automated>cargo build -p rollout-transport --no-default-features --features h2 &amp;&amp; cargo test -p rollout-transport --test heartbeat &amp;&amp; cargo test -p rollout-transport --test control_stream &amp;&amp; cargo clippy -p rollout-transport --all-targets --features h2 -- -D warnings &amp;&amp; mdbook build docs/book</automated>
  </verify>
  <acceptance_criteria>
    - `crates/rollout-transport/src/channels/heartbeat.rs` contains `impl HeartbeatSvc for HeartbeatServiceImpl`
    - `crates/rollout-transport/src/server.rs` contains `pub async fn serve` that wires all three services
    - `cargo test -p rollout-transport --test heartbeat` exits 0 (≥2 tests pass; mTLS test may be `#[ignore]`)
    - `cargo test -p rollout-transport --test control_stream` exits 0 (2 tests pass)
    - `cargo build -p rollout-transport --no-default-features --features h2` exits 0
    - `cargo build -p rollout-transport --features quic` either succeeds OR fails with an EXPERIMENTAL-tagged compile error (acceptable in Phase 2; document the failure case in transport.md)
    - `docs/book/src/substrate/transport.md` exists; `mdbook build docs/book` exits 0
    - `docs/book/src/SUMMARY.md` references `./substrate/transport.md`
    - DOCS-02 satisfied (chapters + tests + inline docs all touch the commit)
  </acceptance_criteria>
  <done>
    SUBSTR-02 satisfied for the H/2 plan-of-record; QUIC is a documented opt-in feature; substrate/transport mdBook chapter ships.
  </done>
</task>

</tasks>

<verification>
```bash
cargo build -p rollout-transport
cargo test -p rollout-transport --tests
cargo clippy -p rollout-transport --all-targets --features h2 -- -D warnings
cargo doc -p rollout-transport --no-deps --features h2
mdbook build docs/book
```
All exit 0; QUIC feature does not affect default build.
</verification>

<success_criteria>
- SUBSTR-02 satisfied: H/2 tonic + rustls + mTLS + three channels working
- Deadline-based health helpers + plan-time invariants in place
- QUIC behind a feature flag, EXPERIMENTAL warning in docs
- Substrate/transport mdBook chapter ships
- Default `cargo build` does not pull QUIC deps
</success_criteria>

<output>
After completion, create `.planning/phases/02-local-substrate/02-04-rollout-transport-SUMMARY.md` documenting:
- Whether the QUIC feature actually compiled at execution time (tonic-h3 0.0.5 API surface may have drifted)
- mTLS test variant chosen (full mTLS roundtrip vs ignored)
- Decisions under "Claude's Discretion" (e.g., default listen port, ChannelBuilder signature)
- Open questions for plan 02-06 (Coordinator) — particularly how the Coordinator wires its &dyn Coordinator into the HeartbeatServiceImpl
</output>
