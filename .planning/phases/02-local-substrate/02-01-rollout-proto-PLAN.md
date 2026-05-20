---
phase: 02-local-substrate
plan: 01
type: execute
wave: 2
depends_on: [02-00]
files_modified:
  - crates/rollout-proto/Cargo.toml
  - crates/rollout-proto/build.rs
  - crates/rollout-proto/src/lib.rs
  - crates/rollout-proto/proto/transport.proto
  - crates/rollout-proto/proto/plugin.proto
  - crates/rollout-proto/tests/codegen.rs
  - Makefile
  - xtask/Cargo.toml
  - xtask/src/main.rs
  - python/rollout/_proto/__init__.py
  - python/rollout/_proto/.gitkeep
  - docs/book/src/substrate/proto.md
  - docs/book/src/SUMMARY.md
autonomous: true
requirements: [SUBSTR-02, SUBSTR-03, DOCS-01, DOCS-02, DOCS-03]
must_haves:
  truths:
    - "rollout-proto compiles transport.proto and plugin.proto via tonic-build at workspace build time."
    - "Heartbeat / Control / Work / Plugin service stubs are reachable as Rust types from downstream crates via tonic::include_proto!."
    - "`make protos` regenerates Python sidecar stubs into python/rollout/_proto/ deterministically."
    - "Phase-2 substrate mdBook chapter on the proto crate exists."
  artifacts:
    - path: crates/rollout-proto/proto/transport.proto
      provides: "Heartbeat (unary), Control (server-stream), Work (bidi) service definitions"
      contains: "service Heartbeat"
    - path: crates/rollout-proto/proto/plugin.proto
      provides: "Plugin sidecar service: Init/Preflight/Call/Reload/Shutdown"
      contains: "service Plugin"
    - path: crates/rollout-proto/build.rs
      provides: "tonic-build invocation that runs on cargo build"
      contains: "tonic_build"
    - path: crates/rollout-proto/src/lib.rs
      provides: "tonic::include_proto!-driven module exports"
      contains: "include_proto"
  key_links:
    - from: crates/rollout-proto/build.rs
      to: proto/*.proto
      via: tonic_build::configure().compile_protos()
      pattern: "compile_protos"
    - from: Makefile
      to: xtask
      via: "make protos -> cargo xtask gen-protos"
      pattern: "gen-protos"
---

<objective>
Land the `rollout-proto` crate: a single home for `transport.proto` (Heartbeat/Control/Work) and `plugin.proto` (sidecar) per CONTEXT D-PROTO-01. The crate runs `tonic-build` in its `build.rs` so every downstream crate (`rollout-transport`, `rollout-plugin-host`, `rollout-coordinator`) gets the same generated Rust types. Python sidecar stubs are generated via `make protos` (one-shot, committed) so the in-tree Python sample does not require `pip install grpcio` at test time.

Purpose: Centralize wire-format ownership in ONE crate so the H/2 ↔ QUIC swap (CONTEXT D-TRANS-01 fallback) and the sidecar IPC protocol share the same source-of-truth `.proto` files.

Output: `crates/rollout-proto/` builds cleanly, exposes Rust modules `rollout_proto::transport::v1::*` and `rollout_proto::plugin::v1::*`, and `make protos` regenerates `python/rollout/_proto/*.py` byte-deterministically (committed).
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
@docs/specs/03-plugin-system.md
@docs/specs/05-distribution.md
@Cargo.toml
@Makefile
@xtask/Cargo.toml
@xtask/src/main.rs
@crates/rollout-proto/Cargo.toml
@crates/rollout-proto/src/lib.rs
@crates/rollout-proto/build.rs

<interfaces>
Generated Rust types downstream crates will consume (from `tonic::include_proto!`):

```rust
// rollout-proto::transport::v1
pub mod heartbeat_server {  // tonic-generated
    pub struct HeartbeatServer<T: Heartbeat>(/* ... */);
}
pub mod heartbeat_client {
    pub struct HeartbeatClient<T> { /* ... */ }
}
pub trait Heartbeat: Send + Sync + 'static {
    async fn beat(&self, request: tonic::Request<BeatRequest>) -> Result<tonic::Response<BeatResponse>, tonic::Status>;
}
pub struct BeatRequest { worker_id: String, due_at: Option<prost_types::Timestamp>, state: i32, run_id: String }
pub struct BeatResponse { acknowledged_at_drift: Option<prost_types::Duration>, pending_control: Option<ControlPush> }
pub enum WorkerState { Unspecified=0, Init=1, Ready=2, Running=3, Draining=4 }

pub mod control_server { /* ControlServer<T: Control> */ }
pub trait Control: Send + Sync + 'static {
    type SubscribeStream: tokio_stream::Stream<Item = Result<ControlPush, tonic::Status>> + Send + 'static;
    async fn subscribe(&self, request: tonic::Request<ControlSubscribeRequest>) -> Result<tonic::Response<Self::SubscribeStream>, tonic::Status>;
}

pub mod work_server { /* WorkServer<T: Work> */ }
pub trait Work: Send + Sync + 'static {
    type StreamStream: tokio_stream::Stream<Item = Result<WorkDown, tonic::Status>> + Send + 'static;
    async fn stream(&self, request: tonic::Request<tonic::Streaming<WorkUp>>) -> Result<tonic::Response<Self::StreamStream>, tonic::Status>;
}

// rollout-proto::plugin::v1
pub trait Plugin: Send + Sync + 'static {
    async fn init(&self, r: tonic::Request<InitRequest>) -> Result<tonic::Response<InitResponse>, tonic::Status>;
    async fn preflight(&self, r: tonic::Request<PreflightRequest>) -> Result<tonic::Response<PreflightResponse>, tonic::Status>;
    async fn call(&self, r: tonic::Request<CallRequest>) -> Result<tonic::Response<CallResponse>, tonic::Status>;
    async fn reload(&self, r: tonic::Request<ReloadRequest>) -> Result<tonic::Response<ReloadResponse>, tonic::Status>;
    async fn shutdown(&self, r: tonic::Request<ShutdownRequest>) -> Result<tonic::Response<ShutdownResponse>, tonic::Status>;
}
```
</interfaces>
</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: Write transport.proto + plugin.proto + tonic-build wiring</name>
  <files>
    crates/rollout-proto/Cargo.toml,
    crates/rollout-proto/build.rs,
    crates/rollout-proto/src/lib.rs,
    crates/rollout-proto/proto/transport.proto,
    crates/rollout-proto/proto/plugin.proto,
    crates/rollout-proto/tests/codegen.rs
  </files>
  <read_first>
    - crates/rollout-proto/Cargo.toml (Wave-0 stub — extend, don't rewrite)
    - crates/rollout-proto/build.rs (Wave-0 stub)
    - crates/rollout-proto/src/lib.rs (Wave-0 stub)
    - .planning/phases/02-local-substrate/02-RESEARCH.md §"Transport proto (sketch)" and §"Sidecar gRPC proto (sketch)" — proto contents are AUTHORITATIVE
    - .planning/phases/02-local-substrate/02-CONTEXT.md §"D-TRANS-03" (three channels) and §"D-PROTO-01"
    - docs/specs/05-distribution.md §3 (transport channels)
    - docs/specs/03-plugin-system.md §3 (sidecar protocol)
    - Cargo.toml (workspace.dependencies — `tonic`, `tonic-build`, `prost`, `prost-types` already pinned by plan 02-00)
  </read_first>
  <behavior>
    RED first (`tests/codegen.rs`):
    - `transport_v1_types_present`: assert `rollout_proto::transport::v1::BeatRequest` and `BeatResponse` are constructible (`Default::default()` works); assert `WorkerState::Ready as i32 == 2`.
    - `transport_v1_services_present`: assert `rollout_proto::transport::v1::heartbeat_server::HeartbeatServer::<()>` type name is resolvable (compile-only check via PhantomData trick — `fn _x() { let _: std::marker::PhantomData<rollout_proto::transport::v1::heartbeat_server::HeartbeatServer<()>> = std::marker::PhantomData; }`).
    - `plugin_v1_types_present`: same pattern for `rollout_proto::plugin::v1::InitRequest`, `CallRequest`, `CallResponse`, `plugin_server::PluginServer<()>`.
    - `proto_files_exist`: filesystem check that `proto/transport.proto` and `proto/plugin.proto` are non-empty.
    Then GREEN: write the proto files and build.rs so all tests compile and pass.
  </behavior>
  <action>
    **Step 1 — `crates/rollout-proto/Cargo.toml`:**
    ```toml
    [package]
    name = "rollout-proto"
    version.workspace = true
    edition.workspace = true
    license.workspace = true
    rust-version.workspace = true
    repository.workspace = true

    [lints]
    workspace = true

    [dependencies]
    tonic        = { workspace = true }
    prost        = { workspace = true }
    prost-types  = { workspace = true }
    tokio-stream = { workspace = true }

    [build-dependencies]
    tonic-build  = { workspace = true }
    ```

    **Step 2 — `crates/rollout-proto/build.rs`:**
    ```rust
    //! Compile transport.proto + plugin.proto via tonic-build.
    //! Generated code lands in OUT_DIR; src/lib.rs picks it up via tonic::include_proto!.
    fn main() -> Result<(), Box<dyn std::error::Error>> {
        println!("cargo:rerun-if-changed=proto/transport.proto");
        println!("cargo:rerun-if-changed=proto/plugin.proto");
        tonic_build::configure()
            .build_client(true)
            .build_server(true)
            .compile_protos(
                &["proto/transport.proto", "proto/plugin.proto"],
                &["proto"],
            )?;
        Ok(())
    }
    ```

    **Step 3 — `crates/rollout-proto/proto/transport.proto`:** copy VERBATIM from RESEARCH.md §"Transport proto (sketch)". Package = `rollout.transport.v1`. Services: `Heartbeat` (unary `Beat`), `Control` (server-stream `Subscribe`), `Work` (bidi `Stream`).

    **Step 4 — `crates/rollout-proto/proto/plugin.proto`:** copy VERBATIM from RESEARCH.md §"Sidecar gRPC proto (sketch)". Package = `rollout.plugin.v1`. Service: `Plugin` with `Init`/`Preflight`/`Call`/`Reload`/`Shutdown` rpcs.

    **Step 5 — `crates/rollout-proto/src/lib.rs`:**
    ```rust
    //! Generated gRPC types for `rollout-transport` (heartbeat/control/work) and
    //! the Python sidecar protocol consumed by `rollout-plugin-host`.
    //!
    //! Source-of-truth `.proto` files live under `proto/`; `tonic-build` compiles
    //! them at build time. Per CONTEXT D-PROTO-01 this crate is the ONLY place
    //! `tonic-build` runs in the workspace.
    #![forbid(unsafe_code)]
    #![allow(missing_docs)] // generated code — tonic-build doesn't emit rustdoc per item.
    #![allow(clippy::pedantic)] // generated code may not satisfy pedantic lints.

    /// gRPC transport: Heartbeat (unary), Control (server-stream), Work (bidi).
    pub mod transport {
        /// v1 wire format.
        pub mod v1 {
            tonic::include_proto!("rollout.transport.v1");
        }
    }

    /// gRPC Plugin sidecar protocol.
    pub mod plugin {
        /// v1 wire format.
        pub mod v1 {
            tonic::include_proto!("rollout.plugin.v1");
        }
    }
    ```

    **Step 6 — `crates/rollout-proto/tests/codegen.rs`:** implement the RED tests from `<behavior>`.

    **Constraints:**
    - Do NOT depend on `rollout-core`. `rollout-proto` is purely wire types.
    - `#![allow(missing_docs)]` is OK at this crate-only because tonic-build's generated code doesn't carry rustdoc per item (verified by Phase-1 schema-drift work — same pattern). The crate-level `//!` doc is what `rustdoc::missing_crate_level_docs` checks.
  </action>
  <verify>
    <automated>cargo build -p rollout-proto &amp;&amp; cargo test -p rollout-proto --tests &amp;&amp; cargo clippy -p rollout-proto --all-targets -- -D warnings &amp;&amp; cargo doc -p rollout-proto --no-deps 2>&amp;1 | grep -v "warning:" | tail -5</automated>
  </verify>
  <acceptance_criteria>
    - `crates/rollout-proto/proto/transport.proto` exists and contains `service Heartbeat` AND `service Control` AND `service Work`
    - `crates/rollout-proto/proto/plugin.proto` exists and contains `service Plugin`
    - `crates/rollout-proto/build.rs` contains `tonic_build::configure().compile_protos(`
    - `crates/rollout-proto/src/lib.rs` contains `tonic::include_proto!("rollout.transport.v1")` AND `tonic::include_proto!("rollout.plugin.v1")`
    - `cargo build -p rollout-proto` exits 0
    - `cargo test -p rollout-proto --tests` exits 0 (codegen.rs tests pass)
    - `cargo clippy -p rollout-proto --all-targets -- -D warnings` exits 0
    - DOCS-02 same-commit policy: tests/codegen.rs (test), src/lib.rs crate-level `//!` doc (inline doc), proto/*.proto (the wire docs themselves) all touched in the same commit
  </acceptance_criteria>
  <done>
    rollout-proto compiles; tonic-build runs in build.rs only; generated types are reachable from downstream crates via `rollout_proto::transport::v1::*` and `rollout_proto::plugin::v1::*`.
  </done>
</task>

<task type="auto">
  <name>Task 2: xtask gen-protos + Makefile `protos` target + Python stub harness + substrate mdBook chapter</name>
  <files>
    xtask/Cargo.toml,
    xtask/src/main.rs,
    Makefile,
    python/rollout/_proto/__init__.py,
    python/rollout/_proto/.gitkeep,
    docs/book/src/substrate/proto.md,
    docs/book/src/SUMMARY.md
  </files>
  <read_first>
    - xtask/src/main.rs (existing Phase-1 commands: schema-gen)
    - xtask/Cargo.toml
    - Makefile (preserve all existing targets verbatim)
    - .planning/phases/02-local-substrate/02-CONTEXT.md (D-PROTO-01 — Python stubs committed)
    - .planning/phases/02-local-substrate/02-RESEARCH.md §"Python sidecar IPC: avoid pip" (stdlib framing decision)
    - docs/book/src/substrate/index.md (landing page from plan 02-00)
    - docs/book/src/SUMMARY.md (extend with proto chapter)
  </read_first>
  <action>
    **Step 1 — Decide Python-stub strategy.** Per CONTEXT D-PROTO-01 + RESEARCH Pitfall 9: the in-tree sample sidecar uses **stdlib framing** (no `pip install grpcio` required); the committed `python/rollout/_proto/` directory is a **placeholder** for users who want real gRPC stubs (they can opt into running `make protos` with a separate dev-only Python venv that has `grpcio-tools` installed). Phase 2 does NOT require generating real `_pb2.py` files in CI.

    **Step 2 — `xtask gen-protos`:** Add a new clap subcommand to `xtask/src/main.rs`:
    ```rust
    /// Generate Python protobuf stubs for `rollout-proto` into `python/rollout/_proto/`.
    /// Requires `grpcio-tools` on the dev machine; this is opt-in (not run in CI).
    GenProtos {
        /// Where to write generated files.
        #[arg(long, default_value = "python/rollout/_proto")]
        out_dir: PathBuf,
    }
    ```
    Implementation:
    1. Find `crates/rollout-proto/proto/*.proto`.
    2. Shell out to `python3 -m grpc_tools.protoc --proto_path=crates/rollout-proto/proto --python_out=<out_dir> --grpc_python_out=<out_dir> transport.proto plugin.proto`.
    3. If `grpc_tools` is not installed, print a helpful message: `xtask gen-protos requires 'pip install grpcio-tools'. The in-tree Python sample sidecar uses stdlib framing and does NOT need this; only run gen-protos if you are authoring a Python plugin that uses real gRPC.` and exit 0 (NOT an error).
    4. Otherwise, run the subprocess; on success, ensure `__init__.py` exists in the out_dir.

    **Step 3 — `xtask/Cargo.toml`:** no new deps required (uses existing `clap` + `std::process::Command`).

    **Step 4 — `Makefile`:** ADD `protos` target (preserve all existing targets verbatim — do NOT rewrite the file):
    ```makefile
    .PHONY: lint test build check schema-gen validate-schema docs graphify protos help

    # ... existing targets unchanged ...

    protos:
    	cargo xtask gen-protos

    help:
    	@echo "lint             cargo fmt --check + clippy -D warnings"
    	@echo "test             cargo test --workspace --tests"
    	@echo "build            cargo build --workspace"
    	@echo "check            lint + test"
    	@echo "schema-gen       regenerate schemas/rollout.schema.json + python stubs"
    	@echo "validate-schema  meta-validate the JSON Schema (requires check-jsonschema)"
    	@echo "docs             mdbook build + cargo doc --workspace --no-deps --all-features"
    	@echo "graphify         build codebase knowledge graph via graphify-ts (out: graphify-out/)"
    	@echo "protos           regenerate python/rollout/_proto/ (requires grpcio-tools; opt-in)"
    ```
    Note: `make smoke` lands in plan 02-07. Do NOT add it here.

    **Step 5 — `python/rollout/_proto/`:** create an empty `__init__.py` with a single docstring `"""Generated gRPC stubs (opt-in; populated by `make protos`)."""` and a `.gitkeep` to ensure the directory tracks even when empty.

    **Step 6 — `docs/book/src/substrate/proto.md`** (NEW, ~80 lines):
    - **Why a dedicated crate** — CONTEXT D-PROTO-01; single tonic-build invocation site; downstream crates depend on `rollout-proto`, not on .proto regeneration.
    - **Wire-format ownership** — transport.proto vs plugin.proto.
    - **Three channels** — Heartbeat (unary), Control (server-stream), Work (bidi). Multiplexed on one connection.
    - **Python stubs** — stdlib framing is the in-tree plan-of-record; `make protos` is opt-in for users who want real gRPC.
    - **Versioning** — `.v1` package suffix; bumps live in a future spec edit.
    - **Cross-link to spec 05 §3** and spec 03 §3.3.

    **Step 7 — `docs/book/src/SUMMARY.md`** extend (preserve existing entries):
    ```markdown
    # Summary

    - [Introduction](./introduction.md)
    - [Architecture](./architecture.md)
    - [Substrate](./substrate/index.md)
      - [Proto crate](./substrate/proto.md)
    - [Examples](./examples/index.md)
    ```
  </action>
  <verify>
    <automated>cargo xtask gen-protos --help &amp;&amp; make -n protos &amp;&amp; mdbook build docs/book &amp;&amp; test -f python/rollout/_proto/__init__.py &amp;&amp; grep -q "protos" Makefile</automated>
  </verify>
  <acceptance_criteria>
    - `cargo xtask gen-protos --help` exits 0 and prints the new subcommand
    - `cargo xtask gen-protos` exits 0 even when `grpcio-tools` is NOT installed (prints helpful message, returns 0)
    - `make -n protos` parses without error and shows `cargo xtask gen-protos`
    - All existing `make` targets remain (`lint`, `test`, `build`, `check`, `schema-gen`, `validate-schema`, `docs`, `graphify`, `help`) — grep each
    - `python/rollout/_proto/__init__.py` exists with the placeholder docstring
    - `docs/book/src/substrate/proto.md` exists and `mdbook build docs/book` exits 0
    - `docs/book/src/SUMMARY.md` references `./substrate/proto.md`
    - DOCS-02: this commit touches a code file (`xtask/src/main.rs`) AND `docs/book/src/substrate/proto.md` AND a test (`cargo xtask gen-protos --help` is exercised at acceptance; if execute-plan requires a test file, add `crates/rollout-proto/tests/proto_files_present.rs` asserting the two .proto paths exist and contain `service Heartbeat` / `service Plugin`)
  </acceptance_criteria>
  <done>
    `make protos` wired to `xtask gen-protos`; Python stub directory exists with placeholder; substrate proto chapter renders; existing Makefile / mdBook entries preserved.
  </done>
</task>

</tasks>

<verification>
```bash
cargo build -p rollout-proto
cargo test -p rollout-proto --tests
cargo xtask gen-protos --help
make -n protos
mdbook build docs/book
cargo doc -p rollout-proto --no-deps
```
All exit 0; mdBook builds without broken links.
</verification>

<success_criteria>
- `rollout-proto` compiles and exposes `rollout_proto::transport::v1::*` and `rollout_proto::plugin::v1::*` for downstream crates
- `.proto` files are the single source of truth for transport + plugin wire formats
- `make protos` is wired (opt-in; degrades cleanly without `grpcio-tools`)
- Substrate mdBook chapter for the proto crate ships
</success_criteria>

<output>
After completion, create `.planning/phases/02-local-substrate/02-01-rollout-proto-SUMMARY.md` documenting:
- Final proto package names and service names (transport.v1 + plugin.v1)
- Any deviations from RESEARCH §"Transport proto / Sidecar gRPC proto" sketches
- Stub regeneration path (`make protos` opt-in vs CI-enforced)
- Open questions for plan 02-04 (transport) and 02-05 (plugin host) — particularly whether Work channel bidi works under H/2 tonic
</output>
