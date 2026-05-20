---
phase: 02-local-substrate
plan: 01
subsystem: substrate-proto
tags: [rollout-proto, tonic, tonic-build, prost, grpc, protobuf, transport, plugin, sidecar, mdbook, xtask, makefile]

# Dependency graph
requires:
  - phase: 02-local-substrate
    provides: rollout-proto stub crate + workspace tonic/prost/tonic-build pins (plan 02-00 Wave 0)
provides:
  - "rollout_proto::transport::v1 module with Heartbeat (unary), Control (server-stream), Work (bidi) services + BeatRequest/BeatResponse/WorkerState/ControlPush messages per spec 05 §3"
  - "rollout_proto::plugin::v1 module with Plugin sidecar service (Init/Preflight/Call/Reload/Shutdown) per spec 03 §4"
  - "build.rs invoking tonic-prost-build with vendored protoc — single tonic-build site in the workspace per D-PROTO-01"
  - "`cargo xtask gen-protos [--out-dir PATH]` for opt-in Python gRPC stub regeneration"
  - "`make protos` Makefile target wiring gen-protos into the standard build vocabulary"
  - "docs/book/src/substrate/proto.md mdBook chapter explaining the crate, the three transport channels, the sidecar protocol, and the stdlib-vs-real-gRPC Python sample story"
  - "python/rollout/_proto/ placeholder dir (with __init__.py + .gitkeep) for opt-in real gRPC stubs"
affects: [02-04-rollout-transport, 02-05-rollout-plugin-host, 02-06-rollout-coordinator]

# Tech tracking
tech-stack:
  added:
    - "tonic-prost 0.14 + tonic-prost-build 0.14 — tonic 0.14 split protobuf codegen out of tonic-build; the configure().compile_protos() API moved here"
    - "protoc-bin-vendored 3.0 — vendored protoc binary avoids `brew install protobuf` on dev machines (also documented in scripts/preflight.sh fallback notes)"
  patterns:
    - "Single tonic-build invocation per workspace (D-PROTO-01) — every other crate consumes `rollout_proto::*` instead of running its own codegen"
    - "Opt-in Python stub regen: xtask exits 0 (not error) when grpcio-tools is missing, with a helpful message pointing to the stdlib-framing alternative"
    - "Vendored protoc set via `std::env::set_var(\"PROTOC\", ...)` only if not already overridden — lets CI / dev override with a system protoc when needed"

key-files:
  created:
    - "crates/rollout-proto/proto/transport.proto — Heartbeat/Control/Work services per spec 05 §3 + RESEARCH.md sketch"
    - "crates/rollout-proto/proto/plugin.proto — Plugin sidecar service per spec 03 §3.3 + §4 + RESEARCH.md sketch"
    - "crates/rollout-proto/tests/codegen.rs — 4 compile-shape tests covering transport/plugin types + server stubs + proto-file presence"
    - "crates/rollout-proto/tests/proto_files_present.rs — 2 DOCS-02 partner tests asserting service/rpc/package strings in the .proto files"
    - "xtask/src/gen_protos.rs — clap-less subcommand handler with --help, --out-dir, graceful grpc_tools-missing fallback"
    - "docs/book/src/substrate/proto.md — substrate chapter on the proto crate"
    - "python/rollout/_proto/__init__.py + .gitkeep — opt-in real-gRPC stub placeholder"
  modified:
    - "Cargo.toml — tonic feature set fixed (tls-ring + transport + server + channel + router); prost/prost-types bumped 0.13 -> 0.14 (tonic 0.14 alignment); new pins for tonic-prost, tonic-prost-build, protoc-bin-vendored"
    - "Cargo.lock — refreshed for prost 0.14 + tonic-prost 0.14 transitives"
    - "crates/rollout-proto/Cargo.toml — concrete dep set (tonic, tonic-prost, prost, prost-types, tokio-stream) + build-deps (tonic-build, tonic-prost-build, protoc-bin-vendored)"
    - "crates/rollout-proto/build.rs — wired tonic_prost_build::configure().compile_protos() with vendored PROTOC env var"
    - "crates/rollout-proto/src/lib.rs — exposes transport::v1 and plugin::v1 via tonic::include_proto!"
    - "Makefile — added `protos` target + help entry; all 9 existing targets preserved verbatim"
    - "xtask/src/main.rs — registered gen-protos subcommand; updated usage string"
    - "docs/book/src/SUMMARY.md — nested Proto crate entry under Substrate (Examples placeholder preserved)"
    - "crates/rollout-core/src/lib.rs + traits/mod.rs + traits/storage.rs + 3 tests — fmt-clean drift fixes surfaced by `cargo fmt --all`"

key-decisions:
  - "[Rule 1 — bug] tonic 0.14 feature `tls-rustls` does not exist; the correct feature name is `tls-ring` (rustls backend). Plan instruction copied the older 0.10/0.12 feature name from RESEARCH.md; workspace pin updated."
  - "[Rule 1 — bug] tonic 0.14 requires prost 0.14 (workspace had prost 0.13 from plan 02-00). Bumped prost + prost-types in workspace.dependencies; no downstream consumer existed yet so no cascade."
  - "[Rule 1 — bug] tonic 0.14 split protobuf codegen into a separate `tonic-prost-build` crate; the legacy `tonic_build::configure().compile_protos()` API does not exist on 0.14. Build.rs uses `tonic_prost_build::configure()` instead. Added `tonic-prost` runtime dep so generated code's `tonic_prost::Codec` references resolve."
  - "[Rule 2 — missing critical functionality] tonic-build 0.14 / prost-build no longer bundle protoc. Added `protoc-bin-vendored 3.0` as a build-dep and set PROTOC env var if not already overridden — keeps the build hermetic + matches RESEARCH.md §Environment fallback note."
  - "[D-PROTO-01] Single tonic-build invocation: `rollout-proto/build.rs` is the only site; no other crate compiles .proto files."
  - "[AGENTS.md §7 / RESEARCH Pitfall 9] In-tree Python sample sidecar uses stdlib length-prefixed JSON framing — `make protos` stays opt-in. The xtask exits 0 (not error) when grpcio-tools is missing."
  - "[Style] Generated tonic code carries no per-item rustdoc; lib.rs uses `#![allow(missing_docs)]` at the module boundary + `#![allow(clippy::pedantic)]` + `#![allow(clippy::all)]` to keep the workspace-wide `missing_docs = warn` policy happy without spamming the generated code."

patterns-established:
  - "tonic 0.14 workspace pattern: tonic + tonic-prost (runtime) + tonic-prost-build + protoc-bin-vendored (build); features = ['tls-ring','transport','server','channel','router']; runtime generated code referenced via `tonic::include_proto!`"
  - "Opt-in xtask subcommand pattern: detect required external tool, print a helpful message + exit 0 when missing rather than treating absence as a build failure"

deviations:
  - "[Rule 1 — bug] Workspace tonic feature was set to `tls-rustls` (does not exist on 0.14). Changed to `tls-ring + transport + server + channel + router`."
  - "[Rule 1 — bug] Workspace prost pin was 0.13; tonic 0.14 needs prost 0.14. Bumped + locked."
  - "[Rule 1 — bug] tonic 0.14 split protobuf codegen out of tonic-build; legacy `tonic_build::configure()` is gone. Switched to `tonic-prost-build`."
  - "[Rule 2 — missing critical functionality] Added `protoc-bin-vendored` build-dep so `cargo build -p rollout-proto` works on a clean machine without `brew install protobuf` / `apt install protobuf-compiler`."
  - "[Rule 1 — formatting drift] `cargo fmt --all` revealed pre-existing drift in 6 rollout-core files committed by plan 02-00 (re-ordering inside `pub use traits::{...}` blocks + minor wrap fixes). Included in the Task 2 commit so `make lint` stays green."

# Known stubs (intentional — populated by downstream plans)
known_stubs:
  - "python/rollout/_proto/__init__.py is a placeholder with a single docstring — real `*_pb2.py` / `*_pb2_grpc.py` are opt-in via `make protos` (plan-of-record is stdlib framing per AGENTS.md §7)"
  - "transport.proto's `service Work` carries forward-compatible message types but full pull/submit semantics land in Phase 6 (DIST-01..02) per spec 05 §3 + RESEARCH.md"

# Authentication gates / preflight notes
preflight_note: "None. `cargo build -p rollout-proto` runs hermetically thanks to protoc-bin-vendored; no system protobuf-compiler required. `make protos` degrades gracefully when grpcio-tools is missing."

requirements-completed: [SUBSTR-02, SUBSTR-03, DOCS-01, DOCS-02, DOCS-03]

# Metrics
duration: 35min
completed: 2026-05-20
---

# Phase 2 Plan 01: rollout-proto Crate Summary

**One-liner:** Wired `rollout-proto` as the single workspace site for tonic-prost codegen, shipping `transport.proto` (Heartbeat unary / Control server-stream / Work bidi per spec 05 §3) and `plugin.proto` (Plugin sidecar Init/Preflight/Call/Reload/Shutdown per spec 03 §4); added `cargo xtask gen-protos` + `make protos` as an opt-in Python-stub regen path; documented the crate in `docs/book/src/substrate/proto.md`; gated by `cargo build/test/clippy/doc -p rollout-proto`, `make -n protos`, `mdbook build`, `cargo deny check`.

## What landed

### Task 1 — Proto schema + tonic-prost-build wiring

`crates/rollout-proto/proto/transport.proto` defines:

- `service Heartbeat` with unary `Beat(BeatRequest) -> BeatResponse`
- `service Control` with server-streaming `Subscribe(ControlSubscribeRequest) -> stream ControlPush`
- `service Work` with bidi `Stream(stream WorkUp) -> stream WorkDown`
- Messages: `BeatRequest { worker_id, due_at, state, run_id }`, `BeatResponse { acknowledged_at_drift, pending_control }`, `ControlPush` oneof (drain / snapshot / cancel), `WorkUp` / `WorkDown` oneofs
- `enum WorkerState { Unspecified=0, Init=1, Ready=2, Running=3, Draining=4 }`
- Package: `rollout.transport.v1`

`crates/rollout-proto/proto/plugin.proto` defines:

- `service Plugin` with five RPCs: `Init`, `Preflight`, `Call`, `Reload`, `Shutdown`
- Messages: `InitRequest { plugin_id, config: google.protobuf.Struct }` + `InitResponse { version }`, empty Preflight messages, `CallRequest { method, payload: bytes }` + `CallResponse { payload: bytes, error }`, Reload + Shutdown with `reason` / `grace_secs`
- Package: `rollout.plugin.v1`

`crates/rollout-proto/build.rs` invokes `tonic_prost_build::configure().build_client(true).build_server(true).compile_protos(...)` with vendored protoc + `rerun-if-changed` for both .proto files.

`crates/rollout-proto/src/lib.rs` exposes:

```rust
pub mod transport { pub mod v1 { tonic::include_proto!("rollout.transport.v1"); } }
pub mod plugin    { pub mod v1 { tonic::include_proto!("rollout.plugin.v1"); } }
```

`crates/rollout-proto/tests/codegen.rs` ships 4 compile-shape tests:

1. `transport_v1_types_present` — `BeatRequest::default()` constructs; `WorkerState::Ready as i32 == 2`; all 5 enum variants present.
2. `transport_v1_services_present` — `heartbeat_server::HeartbeatServer<()>`, `control_server::ControlServer<()>`, `work_server::WorkServer<()>` resolve via PhantomData.
3. `plugin_v1_types_present` — `InitRequest`, `CallRequest`, `CallResponse` default-construct; `plugin_server::PluginServer<()>` resolves.
4. `proto_files_exist` — `include_str!()` checks that both .proto files contain the canonical service definitions.

### Task 2 — xtask gen-protos + Makefile + mdBook chapter + Python placeholder

- `xtask/src/gen_protos.rs` adds a new subcommand wired into `xtask/src/main.rs`. Supports `--help`, `--out-dir <PATH>` (default `python/rollout/_proto`), and `--out-dir=PATH`. Detects `grpcio-tools` via `python3 -c "import grpc_tools.protoc"`; on missing prints a helpful message and exits 0. On present, runs `python3 -m grpc_tools.protoc --proto_path=... --python_out=... --grpc_python_out=... transport.proto plugin.proto` and writes `__init__.py` if absent.
- `Makefile` adds `protos: cargo xtask gen-protos`. All 9 existing targets (lint, test, build, check, schema-gen, validate-schema, docs, graphify, help) preserved verbatim; `make help` gains a `protos` line.
- `python/rollout/_proto/` directory with `__init__.py` (placeholder docstring) and `.gitkeep`.
- `docs/book/src/substrate/proto.md` (~95 lines) covers: why a dedicated crate (D-PROTO-01), wire-format ownership table, three transport channels table (Heartbeat / Control / Work with RPC kind, cadence, purpose), sidecar protocol list, Python stub opt-in story (stdlib framing as plan-of-record), versioning, cross-refs to specs 03 + 05 + CONTEXT.
- `docs/book/src/SUMMARY.md` nests `[Proto crate](./substrate/proto.md)` under `[Substrate](./substrate/index.md)`; reserved `[Examples]` placeholder preserved.
- `crates/rollout-proto/tests/proto_files_present.rs` adds 2 DOCS-02 partner tests asserting `service Heartbeat / Control / Work`, `service Plugin`, `package rollout.transport.v1`, `package rollout.plugin.v1`, and all 5 Plugin RPC verbs are present in the `.proto` files.

## End-to-end verification

All commands exit 0:

```
cargo build -p rollout-proto
cargo test -p rollout-proto --tests          # 4 codegen.rs + 2 proto_files_present.rs = 6 tests
cargo clippy --workspace --all-targets -- -D warnings
cargo doc -p rollout-proto --no-deps
RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" \
  cargo doc -p rollout-proto --no-deps
cargo xtask gen-protos --help
cargo xtask gen-protos                       # exit 0 (grpcio-tools missing -> helpful message)
make -n protos
mdbook build docs/book
cargo fmt --all -- --check
cargo deny check                             # advisories ok, bans ok, licenses ok, sources ok
cargo build --workspace                      # all 7 substrate crates compile
cargo test --workspace --tests               # all phase-1 + phase-2 tests green
```

## Deviations from Plan

### Rule-1 (auto-fix bug) deviations

1. **tonic `tls-rustls` feature does not exist on 0.14.** Plan instruction (copied from RESEARCH.md) said `features = ["tls-rustls", "transport"]`. tonic 0.14's actual feature names are `tls-ring`, `tls-aws-lc`, `tls-native-roots`, `tls-webpki-roots`. Switched to `["tls-ring", "transport", "server", "channel", "router"]` in workspace.dependencies. Surfaced by `cargo build -p rollout-proto` failing immediately with `tonic does not have that feature`.

2. **tonic 0.14 requires prost 0.14, workspace pinned prost 0.13.** Build surfaced 130 `Codec`-related trait-mismatch errors because tonic-prost's `ProstCodec<T>` requires `T: prost::Message` from prost 0.14 but the workspace-pinned prost was 0.13. Bumped prost + prost-types in `[workspace.dependencies]` to 0.14; no downstream consumer existed yet so no cascade.

3. **tonic 0.14 moved protobuf codegen out of tonic-build.** Plan referenced `tonic_build::configure().compile_protos()`. On tonic 0.14 this API lives in `tonic-prost-build` (tonic-build itself is now a low-level codegen kernel). Switched `build.rs` to `tonic_prost_build::configure()`. Added `tonic-prost = "0.14"` as a runtime dep because generated code references `tonic_prost::Codec`.

4. **`cargo fmt --all` surfaced pre-existing fmt drift in 6 `rollout-core` files committed by plan 02-00.** Drift was in `pub use traits::{...}` block ordering + minor wrap fixes in `lib.rs`, `traits/mod.rs`, `traits/storage.rs`, `tests/dependency_direction.rs`, `tests/error_taxonomy.rs`, `tests/trait_surface.rs`. Included in the Task 2 commit so `make lint` stays green; this is scope-boundary edge (drift surfaced because our task ran a workspace-wide `cargo fmt`).

### Rule-2 (auto-add missing critical functionality) deviations

1. **`tonic-build` 0.14 / `prost-build` no longer bundle protoc.** Without protoc on `PATH`, `cargo build -p rollout-proto` fails with `Could not find protoc`. Added `protoc-bin-vendored 3.0` as a build-dep and set the `PROTOC` env var in `build.rs` if not already overridden. This matches RESEARCH.md §Environment fallback note about `protoc-bin-vendored`. Plan's `<action>` step list did not anticipate this; the addition keeps the build hermetic on dev machines without `brew install protobuf`.

### Rule-4 (architectural) deviations

None. All changes stayed within the rollout-proto crate scope.

## Open Questions for Downstream Plans

- **Plan 02-04 (rollout-transport):** Confirm that tonic 0.14's HTTP/2 `Server::builder()` + `tls-ring` rustls features compose cleanly with the generated `heartbeat_server::HeartbeatServer` / `control_server::ControlServer` / `work_server::WorkServer`. The `work_server::WorkServer<()>` PhantomData check in codegen.rs only proves the type exists; bidi-streaming under load is the plan-02-04 verification.
- **Plan 02-04:** Decide whether to gate QUIC behind a Cargo feature in `rollout-transport` itself or in a downstream consumer (e.g. `rollout-cli`). RESEARCH.md recommends the former; the proto crate is QUIC-agnostic.
- **Plan 02-05 (rollout-plugin-host):** The generated `plugin_server::PluginServer` is reachable; the sidecar host needs to decide whether to use it directly (for sidecar plugins that DO opt into real gRPC via `make protos`) or stay on stdlib framing (the in-tree sample's plan-of-record). Suggested: support both via a `SidecarProtocol` enum dispatch that already exists in `rollout-core` (`GrpcUds` vs `FramedJsonUds`).
- **Plan 02-06 (rollout-coordinator):** Confirm `BeatRequest::due_at` (a `Option<prost_types::Timestamp>`) round-trips cleanly with the `Heartbeat { due_at: SystemTime }` struct in `rollout-core`. The conversion is `From<SystemTime> for prost_types::Timestamp` (already in prost-types 0.14); the coordinator's heartbeat-service shim will need to handle the None case (set to "now").

## Commits

| Task | Hash    | Subject                                                                       |
| ---- | ------- | ----------------------------------------------------------------------------- |
| 1    | f45e91e | feat(02-01): wire rollout-proto transport.proto + plugin.proto via tonic-build |
| 2    | 376039a | feat(02-01): wire `make protos` + xtask gen-protos + substrate proto chapter   |

## Self-Check: PASSED

- crates/rollout-proto/proto/transport.proto — FOUND
- crates/rollout-proto/proto/plugin.proto — FOUND
- crates/rollout-proto/build.rs — FOUND (tonic_prost_build wired)
- crates/rollout-proto/src/lib.rs — FOUND (transport::v1 + plugin::v1)
- crates/rollout-proto/tests/codegen.rs — FOUND (4 tests, all pass)
- crates/rollout-proto/tests/proto_files_present.rs — FOUND (2 tests, all pass)
- xtask/src/gen_protos.rs — FOUND
- xtask/src/main.rs — FOUND (gen-protos subcommand registered)
- Makefile — FOUND (`protos` target + all 9 existing targets preserved)
- python/rollout/_proto/__init__.py — FOUND
- python/rollout/_proto/.gitkeep — FOUND
- docs/book/src/substrate/proto.md — FOUND
- docs/book/src/SUMMARY.md — FOUND (Proto crate entry nested under Substrate)
- Commit f45e91e — FOUND in `git log --oneline -10`
- Commit 376039a — FOUND in `git log --oneline -10`
