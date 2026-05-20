# Substrate (Phase 2)

Phase 2 lights up the **local substrate** — the layer that lets a worker start,
store state, talk to a peer, load a plugin, and shut down cleanly **without
touching any cloud**. The per-crate chapters land in plan 02-07; this page is
the section landing.

## What ships in Phase 2

- **`rollout-proto`** — owns `transport.proto` (heartbeat / control / work) and
  `plugin.proto` (sidecar). `tonic-build` runs in this crate's `build.rs`;
  every other crate consumes the generated code.
- **`rollout-storage`** — implements `Storage` + `StorageTxn` on top of
  [`redb`](https://crates.io/crates/redb). In-process `tokio::sync::broadcast`
  per-prefix for `watch()`. Always-fsync. Default path `./data/rollout.db`.
  `postcard` value encoding.
- **`rollout-cloud-local`** — implements `ObjectStore` (content-addressed
  sharded FS under `./data/object-store/`), `Queue` (RAM hot path + spill to
  `rollout-storage` for restart replay), `SecretStore` (env-var allowlist,
  read-only), and `ComputeHint` (Linux full via `/proc` + `nvml-wrapper`;
  macOS minimal stub via `sysinfo`).
- **`rollout-transport`** — implements gRPC with three logical channels
  (heartbeat / control / work). HTTP/2 + `rustls` is the plan-of-record; QUIC
  via `tonic-h3` is feature-flagged EXPERIMENTAL.
- **`rollout-plugin-host`** — implements `PluginHost` with all three modes
  wired: Rust cdylib, PyO3 in-process, Python sidecar (gRPC-over-UDS).
- **`rollout-coordinator`** — minimal binary that registers workers, accepts
  heartbeats, persists worker registry + heartbeat ledger to `Storage`, and
  marks workers failed via deadline-based scan.
- **Smoke test** — `make smoke` + `scripts/smoke.sh` spawn 1 coordinator + 2
  workers + 1 cdylib + 1 Python sidecar, kill `w1`, and assert deadline
  detection within `2 × heartbeat_interval`.

## Plan-of-record vs stretch

HTTP/2 over tonic + `rustls` is the **plan-of-record** transport. QUIC via
[`tonic-h3`](https://crates.io/crates/tonic-h3) v0.0.x is feature-flagged
EXPERIMENTAL (it is currently pre-1.0 and bidi-streaming is not documented as
production-ready). The same `.proto` schema covers both; the swap-to-QUIC is a
single-config change in a later phase.

## Trait surface

All trait contracts live in `rollout-core`. The Wave-0 extensions
(plan 02-00) align the `rollout-core` traits with the spec versions:

- `Storage` / `StorageTxn` — see [spec 04](../../specs/04-storage-snapshots.md)
  §2. Phase 2 simplifies `scan` to return owned `Vec` instead of `BoxStream`
  (object-safety + `async_trait` constraint).
- `PluginHost` — see [spec 03](../../specs/03-plugin-system.md) §4–§5. Phase 2
  uses `Vec<u8>` payloads in `call()`; richer typed-payload helpers ship in
  later phases.
- `Worker` / `Coordinator` — see [spec 01](../../specs/01-core-runtime.md) §2.
  `Worker::init` / `ready` land in Phase 2; `WorkerContext` stays a unit
  struct until Phase 6.
- `ObjectStore` / `Queue` / `SecretStore` / `ComputeHint` — see
  [spec 06](../../specs/06-cloud-layer.md) §3. `Queue::ack` / `nack`,
  `ObjectStore::exists`, `ComputeHint::preemption_signal`, and
  `SecretStore::put` ship in `rollout-core` in Phase 2.
- `EventEmitter` — see [spec 09](../../specs/09-observability.md) §2. The
  trait lives in `rollout-core` (plan 02-00); the `StdoutJsonEmitter` impl
  lands in `rollout-coordinator` (plan 02-06).

## Per-crate chapters

- [Proto crate](./proto.md)
- [Storage](./storage.md)
- [Cloud-local](./cloud-local.md)
- [Transport](./transport.md)
- [Plugin host](./plugin-host.md)
- [Python bridge](./python-bridge.md) — PyO3 + `pyo3-async-runtimes` pin rationale
- [Coordinator](./coordinator.md)
- [Smoke test](./smoke-test.md) — the `make smoke` SUBSTR-02 acceptance gate

## Preflight

Before running `make smoke`, run `bash scripts/preflight.sh`. The script
checks that `cargo`, `make`, and `python3 ≥ 3.11` are on `PATH` and notes
whether `protoc` is available (`tonic-build` bundles a copy when missing).
