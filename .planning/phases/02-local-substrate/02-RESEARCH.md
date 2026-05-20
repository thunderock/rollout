# Phase 2: Local substrate — Research

**Researched:** 2026-05-19
**Domain:** Embedded KV storage (redb), gRPC-over-QUIC transport (tonic + h3 + quinn), plugin host (PyO3 + cdylib + sidecar), local cloud impls (FS object store, in-mem queue, env-var secrets, /proc-based compute hints), minimal coordinator with deadline-based health.
**Confidence:** HIGH on storage, plugin-host, cloud-local, observability; **MEDIUM-LOW on transport** (tonic-h3 is v0.0.5 "experimental"); HIGH on tooling versions.

## Summary

Phase 2 lights up Layer 1 (cloud-local) + Layer 2 (storage + transport) + a slice of Layer 3 (plugin-host) and a thin coordinator that proves deadline-based health. Six new crates (`rollout-proto`, `rollout-storage`, `rollout-cloud-local`, `rollout-transport`, `rollout-plugin-host`, `rollout-coordinator`) integrate through the existing `rollout-core` trait surface, with `rollout-cli` gaining `worker run` and `coordinator run` subcommands. A `make smoke` / `scripts/smoke.sh` E2E gate launches 1 coordinator + 2 workers + 1 cdylib + 1 Python sidecar plugin, kills w1, and asserts deadline-detection inside `2 × heartbeat_interval`.

Two findings are load-bearing for planning:

1. **`tonic-h3` (gRPC-over-QUIC) is v0.0.5 "experimental" (last release 2025-11-01).** Bidirectional streaming is not explicitly documented as supported; the upstream `tonic/h3` integration is tracked in `hyperium/tonic#339` and has no merge ETA. **CONTEXT.md's documented fallback (HTTP/2 tonic) MUST be the default plan-of-record**, with QUIC as a stretch goal behind a feature flag. The fallback uses standard `tonic::transport::Server::builder().tls_config()` with `rustls`.

2. **`rollout-core` traits in the working tree are Phase 1 stubs, not the full spec surface.** Spec 04's `Storage` has `get`/`get_many`/`scan`/`watch`/`begin`/`ping`; the actual `crates/rollout-core/src/traits/storage.rs` only has `begin` + `ping`. Same for `Plugin`/`PluginHost`/`Coordinator`. CONTEXT.md says "trait definitions are not modified in Phase 2" — that's not true in practice. Per AGENTS.md §4 ("spec is the contract — if spec is wrong, fix the spec in the same PR"), Phase 2 either (a) extends the core traits to match spec, or (b) extends spec to match the stub. **The planner MUST allocate a Wave-0 task to land the trait extensions in `rollout-core` before Wave 1 starts** — otherwise none of the six new crates can implement against a usable surface.

**Primary recommendation:** Plan-of-record is **HTTP/2 tonic + redb 2.x + pyo3 0.28 + pyo3-async-runtimes 0.28 + rcgen for dev CA + libloading 0.8 for cdylib + tonic UDS for sidecar IPC**. QUIC is a feature-flagged opt-in. Trait extensions in `rollout-core` land in Wave 0 before any crate scaffolding.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Storage (`rollout-storage`):**
- **D-STO-01** — Embedded backend = **redb** (per spec 04 §3.1 "preferred"; single-file MVCC, copy-on-write, no compaction stalls). The async `Storage` trait wraps the sync redb API via `tokio::task::spawn_blocking`.
- **D-STO-02** — `watch()` implemented via `tokio::sync::broadcast` per key-prefix. Commit hooks fan events out to subscribers. Documented as **in-process only**; cross-process watch arrives with the Postgres backend in Phase 4.
- **D-STO-03** — Durability mode: **always fsync** on commit. Default path `./data/rollout.db`, overridable via `[storage.embedded] path = ...`.
- **D-STO-04** — Value encoding: **postcard**. Compact, schemafull, deterministic, serde-native.

**Plugin host (`rollout-plugin-host`):**
- **D-PLUGIN-01** — All three modes ship as wired loaders: Rust cdylib, PyO3 in-process, Python sidecar (gRPC-over-UDS).
- **D-PLUGIN-02** — PyO3 ↔ Tokio bridge: `pyo3-async-runtimes` + dedicated Python OS thread per worker. Pin both `pyo3` and `pyo3-async-runtimes` versions early.
- **D-PLUGIN-03** — In-tree plugin samples under `python/examples/sample_sidecar/` and `python/examples/sample_inproc/`. Sidecar launches via `python -m sample_sidecar`. **No `maturin develop` step in `cargo test`.**
- **D-PLUGIN-04** — Full hot-reload for PyO3 (`importlib.reload`) + sidecar (SIGTERM + respawn). Rust cdylib reload returns `Fatal(PluginContract)` "unsupported". Hot-reload gated behind `dev` feature or `--hot-reload` flag.

**Transport (`rollout-transport`):**
- **D-TRANS-01** — gRPC stack = **tonic + h3 + quinn**. Researcher MUST validate `tonic-h3` bidi-streaming maturity. **Documented fallback:** HTTP/2 tonic with the same proto schema; swap-to-QUIC is a single-config change.
- **D-TRANS-02** — TLS = mTLS by default. On first run, CLI generates per-run self-signed CA + cert/key under `./data/tls/` (gitignored).
- **D-TRANS-03** — Three logical channels: heartbeat (unary), control (server-streaming), work (bidi). Multiplexed over one QUIC connection (or one H/2 connection in fallback mode).

**Coordinator slice (`rollout-coordinator`):**
- **D-COORD-01** — `rollout-coordinator` ships as a real crate with a binary. Surface: register-worker, accept-heartbeat, persist worker registry + heartbeat ledger to Storage, deadline-based failure scan. Out of scope: work distribution, work-stealing, lease/CAS, multi-coordinator (all Phase 6 `DIST-01..05`).
- **D-COORD-02** — Smoke test = `make smoke` → `scripts/smoke.sh` spawning 1 coord + 2 workers + 1 cdylib + 1 Python sidecar, kill w1, assert failure detection within `2 × heartbeat_interval`. Wired into CI `test` job.

**cloud-local (`rollout-cloud-local`):**
- **D-LOCAL-01** — `ObjectStore`: content-addressed sharded FS layout `./data/object-store/<sha[0..2]>/<sha[2..4]>/<sha>` + sibling `.meta` JSON.
- **D-LOCAL-02** — `Queue` hot path is `tokio::sync::Mutex<VecDeque<_>>`; mirrors to Storage `cloudlocal/queue/<id>` (postcard) for restart replay.
- **D-LOCAL-03** — `SecretStore` reads `ROLLOUT_SECRET_<KEY>` env vars filtered through config allowlist. `put()` returns `Fatal(ConfigInvalid)` — read-only by design.
- **D-LOCAL-04** — `ComputeHint`: Linux full (`/proc/cpuinfo`, `/proc/meminfo`, optional `nvml-wrapper`); macOS minimal stub via `sysinfo`. Linux-only integration tests gated by `#[cfg(target_os = "linux")]`.
- **D-LOCAL-05** — `BlockStore` **skipped**; declared in `rollout-core` but not implemented in `rollout-cloud-local`.

**Heartbeat / timing defaults (D-TIME-01..02):**
- `heartbeat_interval = 500 ms`, `worker_self_fence_timeout = 4 s`, `coordinator_failure_timeout = 5 s`, `clock_skew_budget = 250 ms`.
- Invariants enforced at config-validate time: `worker_self_fence_timeout < coordinator_failure_timeout`, `clock_skew_budget < heartbeat_interval × 2`. Configs that violate fail at `rollout plan`, never at runtime.

**Cross-crate plumbing:**
- **D-PROTO-01** — Dedicated `rollout-proto` crate owns `transport.proto` + `plugin.proto`. `tonic-build` runs in its `build.rs`. Python stubs generated via `make protos` (committed to repo).
- **D-OBSERVE-01** — `tracing` skeleton workspace-wide. Library crates emit spans/events only; binary crates configure subscriber (default = `tracing-subscriber` + `EnvFilter`, `RUST_LOG`-driven). Critical events list given. `EventEmitter` (spec 09) implemented with stdout JSON sink.
- **D-SANDBOX-01** — Sidecar sandboxing in Phase 2: **network allowlist only**. cgroups + seccomp + FD limits + fs write restrictions left as TODOs referencing Phase 7.

**Wave breakdown (planner reference):**
- W1 (parallel, 3 streams): `rollout-proto` · `rollout-storage` · `rollout-cloud-local` — no cross-deps among the three.
- W2: `rollout-transport` — depends on `rollout-proto`.
- W3 (parallel, 2 streams): `rollout-plugin-host` · `rollout-coordinator` — both depend on proto + storage + transport; they don't depend on each other.
- W4: smoke test + mdBook chapters + CI wiring.

### Claude's Discretion

- Specific `pyo3` + `pyo3-async-runtimes` version pins (pick a known-good pair, comment in `Cargo.toml`).
- Manifest format details — TOML keys, discovery search-paths precedence (spec 03 §8).
- Minimum supported Python version (recommendation: 3.11+ for stdlib `tomllib` + PyO3 abi3 stability).
- PyO3 abi3 strategy — yes/no; if yes, which Python minor.
- cdylib plugin-abi shim crate naming and exported C symbols (likely `rollout-plugin-abi`).
- `nvml-wrapper` vs hand-rolled FFI for GPU inventory.
- Internal redb table layout — table-per-namespace vs single `kv(namespace, key, value)`.
- Specific `tonic-h3` (or successor) crate choice — pick most-maintained at implementation time.
- mdBook section structure for substrate docs.

### Deferred Ideas (OUT OF SCOPE)

- **Postgres `Storage` backend** — Phase 4 (`TRAIN-04`).
- **Real cloud impls** (`rollout-cloud-aws`, `rollout-cloud-gcp`) — Phase 5 (`CLOUD-01`, `CLOUD-02`).
- **Object-store-backed snapshots replacing local-fs** — Phase 5 (`CLOUD-03`).
- **Work distribution, work-stealing, coordinator lease/CAS, multi-node restart-from-storage test** — Phase 6 (`DIST-01..05`).
- **Process snapshots (CRIU-style)** — Phase 11 (`SNAPSHOT-01`).
- **Sidecar full sandbox** (cgroups + seccomp + FD limits + fs write restrictions) — Phase 7 (when `HARNESS-02` brings untrusted code-exec).
- **Multi-coordinator HA + lease handoff** — post-v1.
- **`BlockStore` impl** — opt-in for clouds that need it; not Phase 2.
- **Encrypted object-store traffic** — cloud SDK defaults in Phase 5.
- **NCCL-aware scheduling** — v2.
- **PyO3 sub-interpreter strategy (PEP 684)** — revisit when stable.
- **Cap'n Proto for sidecar IPC** — gRPC over UDS in v1.
- **Cross-process embedded `watch()`** — Postgres backend in Phase 4.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| **SUBSTR-01** | Embedded KV `Storage` backend (sled or redb; chosen Phase 2 after benchmark) | Decision pre-locked: **redb 2.x** (CONTEXT D-STO-01). Research §"Storage stack" + §"Code Examples / redb table layout" + §"Pitfall — redb post-commit hook". |
| **SUBSTR-02** | gRPC-over-QUIC `rollout-transport` with deadline-based heartbeats and three logical channels | Research §"Transport stack" — **HTTP/2 tonic is the plan-of-record**; QUIC behind a feature flag (tonic-h3 v0.0.5 is "experimental"). §"Code Examples / mTLS server", §"Pitfall — split-brain", §"Heartbeat deadline math". |
| **SUBSTR-03** | `rollout-plugin-host` supporting PyO3 in-process + subprocess RPC sidecar, with hot-reload in dev | Research §"Plugin host stack" — pyo3 0.28 + pyo3-async-runtimes 0.28; libloading 0.8 for cdylib; tonic-over-UDS for sidecar. §"Code Examples / sidecar UDS", §"Pitfall — PyO3 GIL + Tokio". |
| **SUBSTR-04** | `rollout-cloud-local`: FS object store + in-mem queue + env-var secrets + /proc compute hints | Research §"cloud-local stack" — sysinfo 0.33, nvml-wrapper 0.11, content-addressed sharded FS layout. §"Code Examples / sharded FS layout". |
| **DOCS-01..03** (cross-cutting, from AGENTS.md §9) | Per-commit doc/test policy, rustdoc gate, docs-build | All inherited from Phase 1 CI; Phase 2 must NOT break the 11 existing jobs. Phase 2 adds `smoke` job + substrate chapters under `docs/book/src/substrate/`. |
</phase_requirements>

## Project Constraints (from CLAUDE.md and AGENTS.md §9)

**From `~/.claude/CLAUDE.md` (user global):**
- Comments: succinct, one short line max, only when WHY is non-obvious. No multi-paragraph docstrings unless asked.
- Linting/formatting: use the project's existing rules. Discovery order: Makefile → justfile → `.github/workflows/*.yml` → `pre-commit-config.yaml` / `pyproject.toml` / `package.json`. Run the project's command verbatim (`make lint`, etc.). The repo uses **`make lint`** = `cargo fmt --all -- --check` + `cargo clippy --all-targets --all-features -- -D warnings`.

**From `AGENTS.md` §9 (every Phase 2 commit):**
- **§9.1 DOCS-01** — mdBook at `docs/book/`; CI builds on PR; deploys to Pages on push to `main`. Phase 2 adds substrate chapters; must NOT break the deploy.
- **§9.2 DOCS-02** — Every code-touching commit must also touch docs/, tests/, or inline doc comments. `[skip-docs-check]` only for bootstrap/mechanical. Phase 2 task acceptance criteria MUST explicitly mention "doc + test touched in same commit."
- **§9.3 DOCS-03** — `cargo doc --workspace --no-deps --all-features` with `RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs"`. Every new crate needs a `//!` crate-level doc comment. Every `pub` item needs at least a one-line doc.
- **§9.5** — Plan tasks declare doc+test updated in `<acceptance_criteria>`. CI changes must keep `docs-build`/`docs-deploy`/`rustdoc-check`/`docs-test-policy` healthy — no skipping, no `continue-on-error`.
- **§9.6** — `graphify-ts` available locally (`npx graphify-ts generate . --directed --svg`). Use before refactors and before planning a phase that touches multiple crates.

**From `AGENTS.md` core principles relevant to Phase 2:**
- #1 Async-native end-to-end: no blocking I/O on async hot paths. Storage wraps sync redb via `spawn_blocking`.
- #4 Single source of truth: new config blocks (`[storage]`, `[transport]`, `[plugins]`, `[cloud.local]`) are Rust types in each crate's `config` module; the schema pipeline picks them up automatically. `xtask schema-gen` MUST regenerate cleanly after Phase 2 changes.
- #5 Deadline-based health: heartbeat carries `due_at`, not interval; coordinator scans deadlines, not polls.
- #7 Every plugin locally testable: the Python sidecar sample MUST run via `python -m sample_sidecar` with stdlib only (no `pip install` in `cargo test` path).
- #8 Hot reload for plugins, not core: PyO3 + sidecar reload; cdylib intentionally unsupported.
- #9 Layered cloud abstraction: `rollout-transport` does NOT depend on `rollout-cloud-*`; `rollout-plugin-host`'s sidecar IPC uses UDS via `rollout-proto`, NOT the QUIC transport.
- #10 Observability not optional: every public op emits a structured event with run/worker IDs.

## Critical Finding: Trait Surface Drift

**Discovered:** The `rollout-core` traits committed to the working tree are Phase-1 stubs, much smaller than the spec versions in `docs/specs/04-storage-snapshots.md` and `docs/specs/03-plugin-system.md`.

Examples:

| Spec says (spec 04 §2) | Actual `crates/rollout-core/src/traits/storage.rs` |
|------------------------|---------------------------------------------------|
| `Storage::begin` + `get` + `get_many` + `scan` + `watch` + `ping` | `Storage::begin` + `ping` only |
| `StorageTxn::put` + `delete` + `cas` + `commit` + `abort` | `StorageTxn::commit` only |

| Spec says (spec 03 §5) | Actual `crates/rollout-core/src/traits/plugin.rs` |
|------------------------|--------------------------------------------------|
| `PluginHost::load` (with manifest path + config) + `call<Req, Res>` + `reload` + `unload` | `PluginHost::load(&str)` only |

| Spec says (spec 01 §2) | Actual `crates/rollout-core/src/traits/worker.rs` |
|------------------------|--------------------------------------------------|
| `Coordinator::heartbeat` + `pull` + `submit` + `control` | `Coordinator::register` + `deregister` only |
| `Worker::init` + `ready` + `run` + `drain` + `shutdown` | `Worker::run` + `drain` + `shutdown` only |

**Implication:** CONTEXT.md says "trait definitions are not modified in Phase 2; Phase 2 imports them from `rollout-core`." That statement is WRONG in practice — none of the six new Phase-2 crates can implement against a usable surface without trait extensions.

**Recommendation for planner:** Add a **Wave 0 task** to `rollout-core` that:

1. Extends `Storage` / `StorageTxn` / `Plugin` / `PluginHost` / `Worker` / `Coordinator` to the spec shape (or as much of it as Phase 2 actually uses — `watch`, `put`/`get`/`scan`/`cas`, `Heartbeat` struct, `PluginManifest`, `PluginHandle`).
2. Updates `docs/specs/01,03,04` in the same PR if spec needs adjustment (AGENTS.md §4).
3. Re-runs `cargo xtask schema-gen` so any new config types flow through.
4. Updates `docs/book/src/` rustdoc xrefs to match.

Without this Wave-0 task, Phase 2 will either (a) silently drift from spec, (b) generate type errors throughout the new crates, or (c) accumulate trait-extension churn across multiple PRs. None of these are acceptable under AGENTS.md §9 (per-commit docs + tests policy: the spec is documentation).

## Standard Stack

### Core dependencies (Phase 2 additions, all license-passing under deny.toml)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `redb` | **2.5+** (current stable 2.5.x; v4.1.0 latest but breaking changes — pin 2.5.x for now) | Embedded KV (spec 04 §3.1) | Pure-Rust, MVCC, single-file, no openssl, no compaction stalls. MIT/Apache-2.0. |
| `tokio` | `1.40+` (workspace pin) | Async runtime | Standard. Use `rt-multi-thread` for binaries, `rt` for libs. |
| `tokio-util` | `0.7+` | UnixListenerStream, CancellationToken | Required for tonic-over-UDS. |
| `tonic` | **0.14.x** | gRPC framework (HTTP/2 plan-of-record) | Latest stable; `tls-rustls` feature gives mTLS via `rustls`. |
| `tonic-build` | `0.14.x` (matches `tonic`) | proto codegen | Runs in `rollout-proto/build.rs`. |
| `prost` | `0.13.x` | protobuf encoding | tonic dep; pin to align. |
| `prost-types` | `0.13.x` | Timestamp / Duration messages | For `Heartbeat.due_at`. |
| `pyo3` | **0.28** | Python ↔ Rust FFI | Latest stable; abi3 features stable. Pin to 0.28.x. |
| `pyo3-async-runtimes` | **0.28** (matches `pyo3`) | Tokio ↔ asyncio bridge | Successor to `pyo3-asyncio`; supports pyo3 0.28; MSRV 1.83. |
| `libloading` | **0.8.x** | Dynamic library load for cdylib plugins | Standard; no FFI footguns. |
| `rustls` | **0.23.x** | TLS impl | Pure-Rust; openssl banned by deny.toml. |
| `rcgen` | **0.13.x** | Self-signed dev CA + cert/key generation | Pure-Rust; one-line API. |
| `postcard` | **1.0.10+** | Value encoding for storage | No-std, schemafull, serde-native, deterministic. |
| `sysinfo` | **0.33+** | macOS ComputeHint fallback | Cross-platform CPU/RAM; MIT. |
| `nvml-wrapper` | **0.11+** | Linux GPU inventory (feature-gated) | NVIDIA Management Library wrapper; MIT/Apache-2.0; depends on `libloading`. |
| `tracing-subscriber` | `0.3.18+` | Subscriber for binary crates | Standard partner to `tracing` (already workspace-pinned). |

**Verification commands** (run before pinning in `Cargo.toml`):
```bash
cargo search redb --limit 1
cargo search tonic --limit 1
cargo search pyo3 --limit 1
cargo search pyo3-async-runtimes --limit 1
cargo search rcgen --limit 1
cargo search postcard --limit 1
cargo search sysinfo --limit 1
cargo search nvml-wrapper --limit 1
cargo search libloading --limit 1
cargo search tonic-h3 --limit 1   # for the feature-flagged QUIC path
```

### Supporting dependencies

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `tokio-stream` | `0.1.16+` | `UnixListenerStream`, `BroadcastStream` | UDS server + storage `watch()` impl. |
| `bytes` | `1.7+` | Zero-copy byte buffers | tonic transport already depends on it; pin to align. |
| `humantime-serde` | `1.1+` | Parse `500ms`, `5s` from TOML | Heartbeat/timeout config fields. |
| `smol_str` | `0.3+` | `SmolStr` keys (already on workspace via `rollout-core` if used) | Storage key namespaces. |
| `tempfile` | `3.10+` (dev) | Test fixtures for FS object store | Unit + integration tests. |
| `proptest` | `1.5+` (dev) | Property tests for postcard round-trip + scheduler | Optional but worth it. |
| `assert_cmd` + `predicates` | `2.0+` (dev) | CLI integration tests | `worker run` / `coordinator run` exit codes. |

### Feature-flagged QUIC (stretch — DO NOT block Phase 2 on this)

| Library | Version | Purpose | Maturity |
|---------|---------|---------|----------|
| `tonic-h3` | **0.0.5** (latest, 2025-11-01) | gRPC over HTTP/3 over QUIC | **Experimental.** Last release ~6mo old. Bidi-streaming not documented. |
| `h3` | `0.0.6+` | HTTP/3 protocol | Pre-1.0; companion to tonic-h3. |
| `h3-quinn` | `0.0.7+` | h3 ↔ quinn adapter | Pre-1.0. |
| `quinn` | `0.11.x` | QUIC | Mature; rustls-integrated. |

**Plan-of-record:** ship HTTP/2 tonic + rustls in Phase 2. Wire `rollout-transport` so the underlying `Server::builder()` is selected by a `quic` Cargo feature; default off. Document the swap path. Re-evaluate at Phase 6 (when multi-node distribution actually exercises HoL-blocking).

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `redb` | `sled` | sled is more mature but has compaction stalls in long-running workloads (spec 04 §3.1 explicitly prefers redb). |
| `redb` | `rocksdb` | C++ dep, openssl-sys risk, not pure-Rust. Banned by deny.toml. |
| HTTP/2 tonic | `tonic-h3` | tonic-h3 v0.0.5 is "experimental" — production risk. Defer to a later phase. |
| `rustls` | `openssl` / `native-tls` | Banned by deny.toml ([bans] openssl, openssl-sys). |
| `rcgen` | `openssl req -x509 ...` | Banned. |
| `nvml-wrapper` | hand-rolled `libloading` FFI to libnvidia-ml.so | nvml-wrapper saves ~500 LOC, well-tested, same `libloading` underneath. License OK. |
| `sysinfo` | `mach`/`mach2` direct | sysinfo is cross-platform; mach is macOS-only and lower-level. Use sysinfo. |
| `postcard` | `bincode` / `messagepack` / `serde_json` | postcard is deterministic + schemafull + compact. bincode has a v1/v2 split causing confusion. serde_json bloats storage. |
| `grpclib` (Python sidecar) | `grpcio` | `grpcio` has fork/subprocess gotchas; `grpclib` is pure-Python asyncio. **But:** sidecar sample MUST NOT require pip install — see §"Python sidecar IPC: avoid pip". |
| `tonic` UDS for sidecar | Cap'n Proto over UDS | Out of scope per spec 03 §12. gRPC for tooling ubiquity. |
| `tokio::sync::broadcast` per-prefix watch | crossbeam channel + custom router | tokio::sync::broadcast is already in the Tokio dep graph; per-prefix Arc<Mutex<HashMap<Prefix, broadcast::Sender>>> is ~50 LOC. |

**Installation block (additions to workspace `Cargo.toml`):**

```toml
[workspace.dependencies]
# (existing pins from Phase 1: serde, serde_json, schemars, thiserror, async-trait, tracing, ulid, blake3, clap, cargo_metadata)

# Phase 2 — storage
redb           = "2.5"
postcard       = { version = "1.0", features = ["use-std"] }
smol_str       = "0.3"

# Phase 2 — async runtime + observability
tokio          = { version = "1.40", features = ["rt-multi-thread", "macros", "sync", "time", "fs", "net", "process", "signal", "io-util"] }
tokio-util     = { version = "0.7", features = ["io"] }
tokio-stream   = { version = "0.1", features = ["sync", "net"] }
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
humantime-serde    = "1.1"

# Phase 2 — transport
tonic          = { version = "0.14", features = ["tls-rustls", "transport"] }
tonic-build    = "0.14"
prost          = "0.13"
prost-types    = "0.13"
rustls         = { version = "0.23", default-features = false, features = ["ring", "std"] }
rcgen          = "0.13"
bytes          = "1.7"

# Phase 2 — plugin host
pyo3                 = { version = "0.28", features = ["auto-initialize", "abi3-py311"] }
pyo3-async-runtimes  = { version = "0.28", features = ["tokio-runtime"] }
libloading           = "0.8"

# Phase 2 — cloud-local
sysinfo        = { version = "0.33", default-features = false, features = ["system"] }
nvml-wrapper   = { version = "0.11", optional = true }

# Phase 2 — dev/test
tempfile       = "3.10"
proptest       = "1.5"
assert_cmd     = "2.0"
predicates     = "3.1"
```

## Architecture Patterns

### Recommended Project Structure

```
crates/
├── rollout-core/              (existing, Wave 0 extends traits)
├── rollout-cli/               (existing, Phase 2 adds `worker run` + `coordinator run`)
├── rollout-proto/             (NEW — Wave 1, parallel stream 1)
│   ├── Cargo.toml             (build-dep: tonic-build)
│   ├── build.rs               (runs tonic-build on .proto files)
│   ├── proto/
│   │   ├── transport.proto    (Heartbeat, Control, Work services)
│   │   └── plugin.proto       (Plugin sidecar service)
│   └── src/
│       └── lib.rs             (includes generated code via tonic::include_proto!)
├── rollout-storage/           (NEW — Wave 1, parallel stream 2)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs             (crate-level //! doc per DOCS-03)
│       ├── config.rs          (StorageConfig with schemars derives)
│       ├── embedded/
│       │   ├── mod.rs         (struct EmbeddedStorage, impl Storage)
│       │   ├── txn.rs         (impl StorageTxn)
│       │   ├── tables.rs      (TableDefinition consts — see Pattern 1)
│       │   ├── watch.rs       (per-prefix broadcast router)
│       │   └── encoding.rs    (postcard helpers)
│       └── tests/
│           ├── crud.rs        (put/get/delete/scan)
│           ├── txn.rs         (commit/abort, CAS)
│           ├── watch.rs       (broadcast fan-out)
│           └── crash_safety.rs (fsync verify; SIGKILL mid-commit)
├── rollout-cloud-local/       (NEW — Wave 1, parallel stream 3)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── config.rs          (CloudLocalConfig)
│       ├── object_store.rs    (FsObjectStore, content-addressed sharded layout)
│       ├── queue.rs           (InMemQueue with Storage spill)
│       ├── secrets.rs         (EnvSecretStore, read-only allowlist)
│       ├── hints/
│       │   ├── mod.rs
│       │   ├── linux.rs       (#[cfg(target_os = "linux")] /proc + optional nvml)
│       │   └── macos.rs       (#[cfg(target_os = "macos")] sysinfo)
│       └── tests/...
├── rollout-transport/         (NEW — Wave 2; depends on rollout-proto)
│   ├── Cargo.toml             (features: `quic` (default off), `h2` (default on))
│   └── src/
│       ├── lib.rs
│       ├── config.rs          (TransportConfig — listen_addr, tls, channels)
│       ├── tls.rs             (rcgen-based dev CA generation)
│       ├── server.rs          (Server::builder() wiring; cfg() switches QUIC/H2)
│       ├── client.rs          (Channel builder; same cfg() pattern)
│       ├── channels/
│       │   ├── heartbeat.rs   (unary HeartbeatService)
│       │   ├── control.rs     (server-stream ControlService)
│       │   └── work.rs        (bidi WorkService — guarded behind feature in QUIC mode)
│       └── tests/
│           ├── tls_dev_ca.rs  (rcgen → trust roundtrip)
│           ├── heartbeat.rs   (round-trip + deadline check)
│           └── h2_smoke.rs    (server↑/client↑/echo/server↓)
├── rollout-plugin-host/       (NEW — Wave 3, parallel stream 1)
│   ├── Cargo.toml             (features: `pyo3` (default), `sidecar` (default), `cdylib` (default), `dev-hot-reload`)
│   └── src/
│       ├── lib.rs
│       ├── config.rs          (PluginsConfig, PluginManifestPath list)
│       ├── manifest.rs        (PluginManifest TOML schema, validate())
│       ├── handle.rs          (PluginHandle = enum { Cdylib(..), PyO3(..), Sidecar(..) })
│       ├── host.rs            (impl PluginHost, dispatch on handle kind)
│       ├── modes/
│       │   ├── mod.rs
│       │   ├── cdylib.rs      (libloading-based loader; rollout-plugin-abi shim)
│       │   ├── pyo3.rs        (pyo3 + pyo3-async-runtimes; dedicated OS thread)
│       │   └── sidecar.rs     (spawn child + tonic-over-UDS client; respawn-on-crash)
│       └── tests/...
├── rollout-plugin-abi/        (NEW or inlined — see §"libloading cdylib safety")
│   └── (versioned C-ABI shim; minimal exposed symbols)
├── rollout-coordinator/       (NEW — Wave 3, parallel stream 2)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── main.rs            (bin: `rollout-coordinator`; reuses `rollout-cli`-style clap)
│       ├── config.rs
│       ├── registry.rs        (worker registry persisted to Storage)
│       ├── heartbeat.rs       (impl HeartbeatService; deadline scanner task)
│       ├── failure_scan.rs    (periodic task: now > due_at + clock_skew → mark failed)
│       └── tests/...
└── xtask/                     (existing, Phase 2 may add `xtask gen-protos` for Python)

python/
├── rollout/                   (existing)
└── examples/                  (NEW)
    ├── sample_inproc/
    │   ├── __init__.py
    │   └── plugin.py          (PyO3 in-tree sample; stdlib only)
    └── sample_sidecar/
        ├── __init__.py
        ├── __main__.py        (python -m sample_sidecar entry)
        └── server.py          (length-prefixed framing over UDS — see §"Python sidecar IPC: avoid pip")

scripts/
├── check-docs-tests-touched.sh (existing)
└── smoke.sh                   (NEW — see §"Smoke test shape")

tests/smoke/                   (NEW — fixtures for the smoke test)
├── coordinator.toml
├── worker.toml
└── plugins/
    └── rust_cdylib_sample/    (NEW — tiny cdylib plugin source + Cargo.toml)

docs/book/src/substrate/       (NEW — mdBook chapters; preserves docs/book/src/examples/ reservation)
├── index.md
├── storage.md
├── transport.md
├── plugin-host.md
├── python-bridge.md           (pyo3 + pyo3-async-runtimes pin rationale; AGENTS.md §9.2)
├── cloud-local.md
└── smoke-test.md
```

### Pattern 1: redb table-per-namespace + `StorageKey` encoding

redb is pure-Rust with `const TableDefinition<K, V>` slots. The natural mapping of spec 04's `StorageKey { namespace, run_id, path }` is **one redb table per namespace**, with keys encoded as a structured byte string.

```rust
// Source: https://docs.rs/redb/latest/redb/struct.TableDefinition.html
//         https://github.com/cberner/redb (design.md)
use redb::{Database, TableDefinition};

// Declare tables at the top of each module that opens them.
const RUNS:       TableDefinition<&[u8], &[u8]> = TableDefinition::new("runs");
const WORKERS:    TableDefinition<&[u8], &[u8]> = TableDefinition::new("workers");
const HEARTBEATS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("heartbeats");
const QUEUE:      TableDefinition<&[u8], &[u8]> = TableDefinition::new("queue");
const PLUGINS:    TableDefinition<&[u8], &[u8]> = TableDefinition::new("plugins");
const CLOUDLOCAL: TableDefinition<&[u8], &[u8]> = TableDefinition::new("cloudlocal_queue");

// Key encoding: <ulid_bytes(run_id)?> || varint(path.len()) || path[0].as_bytes() || ...
// Use postcard for the key payload too — same wire format everywhere.
fn encode_key(key: &StorageKey) -> Vec<u8> {
    postcard::to_allocvec(&key.path).expect("infallible: in-memory")
    // run_id is encoded as the leading bytes for prefix scans by run.
}
```

**Why table-per-namespace, not single `kv(namespace, key, value)`:**
- redb opens a table inside a transaction in O(log n) over the table tree; opening more tables doesn't change per-key cost. (Source: redb design doc — "data tree(s) (per one table)".)
- Per-namespace tables let scans target only the relevant namespace without prefix filtering on namespace bytes.
- Drift-resistant: adding a new namespace = adding a `const`, not a migration.
- One-table `kv` would force every `scan` to filter by leading namespace bytes — strictly worse for `watch()` on `heartbeats/*` (the hot path).

Trade: more tables ⇒ slightly larger metadata B-tree. For ~10 namespaces, negligible.

### Pattern 2: redb's "no post-commit hook" — write-through `watch()`

redb has **no built-in post-commit callback** (verified against redb design doc + WriteTransaction docs). The `watch()` impl must wrap commit explicitly:

```rust
// Source: spec 04 §2 + tokio::sync::broadcast docs.
//
// EmbeddedStorage owns:
//   db:    Arc<Database>,
//   watch: Arc<WatchRouter>,    // per-prefix tokio::sync::broadcast::Sender<StorageEvent>
//
// Every put/delete/cas inside a Txn records (prefix, event) into a local Vec.
// On commit() the Vec is drained and fanned out to the watch router AFTER the
// redb commit succeeds — drop the events if redb commit returns Err.

pub struct EmbeddedTxn {
    redb_txn: redb::WriteTransaction<'static>,  // boxed via Arc<Database>
    pending:  Vec<StorageEvent>,
    watch:    Arc<WatchRouter>,
}

#[async_trait]
impl StorageTxn for EmbeddedTxn {
    async fn commit(self: Box<Self>) -> Result<(), CoreError> {
        let Self { redb_txn, pending, watch } = *self;
        // commit on a blocking pool so we don't stall the async runtime
        let commit_result = tokio::task::spawn_blocking(move || redb_txn.commit())
            .await
            .map_err(|e| CoreError::Fatal(FatalError::Internal(e.to_string())))?;
        commit_result.map_err(|e| CoreError::Fatal(FatalError::Internal(e.to_string())))?;
        // ONLY notify after durability is confirmed (fsync inside redb commit).
        for evt in pending { watch.publish(evt); }
        Ok(())
    }
}
```

**Why fan out only after commit succeeds:** subscribers must never observe writes that were rolled back. This pattern matches what Postgres LISTEN/NOTIFY does (notifications run only after commit).

### Pattern 3: PyO3 ↔ Tokio bridge with dedicated OS thread

```rust
// Source: https://github.com/PyO3/pyo3-async-runtimes (README, README v0.28).
// Pattern: each worker spawns a dedicated Python OS thread that owns the
// interpreter; the Tokio runtime stays on the main pool. Calls into Python
// hop to the Python thread via a channel; Python coroutines awaited via
// pyo3_async_runtimes::tokio::into_future.

use pyo3::prelude::*;
use pyo3_async_runtimes::tokio::future_into_py;

pub struct Pyo3PluginHost {
    py_tx: tokio::sync::mpsc::Sender<PyTask>,
}

enum PyTask {
    Call { method: String, args: serde_json::Value, reply: oneshot::Sender<Result<serde_json::Value, CoreError>> },
    Reload { reply: oneshot::Sender<Result<(), CoreError>> },
    Shutdown,
}

impl Pyo3PluginHost {
    pub fn new(modules_dir: PathBuf) -> PyResult<Self> {
        let (py_tx, mut py_rx) = tokio::sync::mpsc::channel(64);
        // Dedicated OS thread; pyo3-async-runtimes::tokio::main can't be used
        // because we need the main Tokio runtime to stay free.
        std::thread::Builder::new()
            .name("rollout-py".into())
            .spawn(move || {
                pyo3::prepare_freethreaded_python();
                // Initialize the Tokio runtime *inside* Python for asyncio bridging.
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                rt.block_on(async move {
                    while let Some(task) = py_rx.recv().await {
                        // dispatch...
                    }
                });
            })?;
        Ok(Self { py_tx })
    }
}
```

**Key constraints:**
- pyo3 0.28 + pyo3-async-runtimes 0.28 MUST be pinned in lockstep. Mismatched majors break at link time.
- `abi3-py311` feature on pyo3 = single wheel works on Python 3.11+. Use this if you want to avoid per-Python-minor builds.
- The Python thread MUST be initialized before any `Python::with_gil` calls. Use `pyo3::prepare_freethreaded_python()` once at process start.
- Heavy-CPU Python code should release the GIL (`Py::allow_threads`) per spec 03 §3.2.

### Pattern 4: tonic over UDS for sidecar IPC

```rust
// Source: tonic docs, hyperium/tonic#136, hyperium/tonic#856, tonic UDS example.
use tonic::transport::Server;
use tokio::net::UnixListener;
use tokio_stream::wrappers::UnixListenerStream;
use std::os::unix::fs::PermissionsExt;

pub async fn serve_sidecar_uds(svc: PluginServer<impl Plugin>, path: PathBuf) -> Result<(), CoreError> {
    let _ = std::fs::remove_file(&path);  // idempotent
    let uds = UnixListener::bind(&path)?;
    // Lock the socket to the current user — sidecar IPC is intra-user.
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    let stream = UnixListenerStream::new(uds);
    Server::builder()
        .add_service(svc)
        .serve_with_incoming(stream)
        .await
        .map_err(|e| CoreError::Fatal(FatalError::Internal(e.to_string())))
}
```

**Permissions:** chmod 600. Sidecar IPC is single-user; do NOT widen.

**Path placement:** `./data/sidecars/<plugin_name>-<pid>.sock`. Cleanup on shutdown; tolerate stale socket on respawn (remove + rebind).

**Client side:** tonic 0.14's `Endpoint::try_from("http://[::]:50051")` for UDS uses `tower::service_fn` + manual `tokio::net::UnixStream::connect`. The example in hyperium/tonic/examples/src/uds is canonical.

### Pattern 5: deadline-based health (heartbeat invariants)

```rust
// Source: spec 01 §4, spec 05 §6.
// CONTEXT.md timing defaults:
//   heartbeat_interval = 500ms
//   worker_self_fence_timeout = 4s
//   coordinator_failure_timeout = 5s
//   clock_skew_budget = 250ms
//
// Worker publishes due_at = now() + heartbeat_interval × 2 (one-period headroom).
// Coordinator scans at interval = heartbeat_interval; marks failed when
//   now > due_at + clock_skew_budget AND now > due_at + coordinator_failure_timeout.

pub fn next_due_at(now: SystemTime, hb_interval: Duration) -> SystemTime {
    now + hb_interval * 2     // one period of slack
}

pub fn is_failed(now: SystemTime, due_at: SystemTime, skew: Duration, coord_timeout: Duration) -> bool {
    let elapsed_past_due = now.duration_since(due_at).unwrap_or(Duration::ZERO);
    elapsed_past_due > skew && elapsed_past_due > coord_timeout
}
```

**Plan-time invariant enforcement (per CONTEXT D-TIME-02):**

```rust
impl TransportConfig {
    pub fn validate_cross_fields(&self) -> Result<(), Vec<ConfigViolation>> {
        let mut errs = Vec::new();
        if self.worker_self_fence_timeout >= self.coordinator_failure_timeout {
            errs.push(ConfigViolation::new(
                "transport.worker_self_fence_timeout",
                "must be strictly less than transport.coordinator_failure_timeout (split-brain prevention)"
            ));
        }
        if self.clock_skew_budget >= self.heartbeat_interval * 2 {
            errs.push(ConfigViolation::new(
                "transport.clock_skew_budget",
                "must be less than 2 * transport.heartbeat_interval"
            ));
        }
        if errs.is_empty() { Ok(()) } else { Err(errs) }
    }
}
```

These checks are invoked by `rollout plan` (Phase 1's CLI) before any worker starts.

### Anti-Patterns to Avoid

- **`async fn` inside `libloading` cdylib boundary.** abi_stable explicitly does not support async over the C-ABI. Sync calls only across the cdylib edge; the host wraps them in `spawn_blocking` if needed.
- **Polling instead of deadlines.** If you find yourself writing `tokio::time::interval` + checking `last_seen.elapsed() > timeout`, you're polling. Use `next_heartbeat_due_at` from the heartbeat payload directly.
- **Importing `tonic` from `rollout-plugin-host`.** Sidecar IPC uses UDS via `rollout-proto`, NOT the QUIC/HTTP-2 transport from `rollout-transport`. Adding a `rollout-transport` dep to `rollout-plugin-host` would violate dependency-direction-lint and the layered architecture (AGENTS.md §9).
- **`tokio::sync::Mutex` across `.await` on critical hot paths.** Use `parking_lot::Mutex` for non-async-held locks or design lock-free.
- **PyO3 calls outside the dedicated Python OS thread.** Crashes are deterministic and confusing. Always hop via the channel.
- **Hand-rolled CA + signing with `openssl` CLI.** Banned by deny.toml. Use `rcgen`.
- **Trusting the `--hot-reload` flag in production.** Per spec 03 §7, prod runs ignore it with a warning. Implement the warning.
- **Implementing `BlockStore`.** Skipped per D-LOCAL-05. Don't get clever.
- **Adding `aws-sdk-*` or `google-cloud-*` to ANY Phase-2 crate.** Banned by deny.toml + arch-lint. Only Layer-1 cloud crates can.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Self-signed CA + cert/key generation | OpenSSL CLI / hand-rolled X.509 | `rcgen 0.13` | One function call: `generate_simple_self_signed(vec!["localhost"])`. Pure Rust. |
| GPU inventory via NVIDIA libs | `dlopen("libnvidia-ml.so")` + custom FFI | `nvml-wrapper 0.11` (Linux-only feature) | Already uses `libloading`; +1500 LOC saved; well-tested. |
| Cross-platform CPU/RAM stats | `/proc` parsers + `sysctlbyname` calls | `sysinfo 0.33` | Apple-app-store-friendly feature; macOS + Linux; MIT. |
| Plugin manifest TOML parsing | hand-rolled toml + schema validate | `toml 0.8` + `schemars`-derived `JsonSchema` | Inherits the single-source-of-truth rule (spec 11). |
| Length-prefixed framing over UDS | hand-rolled `read_exact(4) ; read_exact(n)` | `tonic` over `UnixListenerStream` | Get HTTP/2 multiplexing + deadlines + tracing-traceparent propagation for free. |
| Async wrappers around sync redb | spawn-and-block-and-forget | `tokio::task::spawn_blocking` | redb is sync-by-design; spawn_blocking is the documented Tokio bridge. |
| Per-prefix pub/sub | locks + condvars | `tokio::sync::broadcast` + `Arc<DashMap<Prefix, Sender<_>>>` | ~50 LOC; subscriber late-join semantics already correct. |
| Content-addressed sharded blob layout | hand-rolled `dirhash` schemes | `blake3` hex + `[0..2]/[2..4]/full` (already standard) | `blake3` is workspace-pinned from Phase 1; AGENTS pattern. |
| TLS server config | rustls raw API | `tonic::transport::ServerTlsConfig` | Tonic 0.14 wraps rustls; mTLS = `.identity(...)` + `.client_ca_root(...)`. |
| Shell argument parsing in `smoke.sh` | hand-rolled | `bash` + `set -euo pipefail` + `trap` for cleanup | Standard; CI-friendly exit codes. |

**Key insight:** every "I could just write 50 lines of Rust" in this list has bitten teams in production. Use the libraries.

## Runtime State Inventory

> Phase 2 is a greenfield substrate phase, not a rename/refactor. **No existing runtime state** (no Mem0, no n8n, no Task Scheduler, no SOPS keys referencing renamed identifiers, no egg-info artifacts). The new `./data/` directory is fresh — it has no stored data to migrate.

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | None — verified by `find . -name '*.db' -o -name '*.sqlite' -o -name '*.json' | grep -v node_modules | grep -v target` returning only schema artifacts. | None. |
| Live service config | None — no n8n, no Datadog, no Tailscale, no external services configured for this repo. | None. |
| OS-registered state | None — no Task Scheduler / launchd / systemd / pm2 / cron entries reference `rollout`. | None. |
| Secrets/env vars | `ROLLOUT_SECRET_*` env-var convention is **introduced** in Phase 2 (D-LOCAL-03). No prior usage to migrate. | Define the allowlist in `[cloud.local.secrets.allowlist]` config block. |
| Build artifacts / installed packages | `target/` from Phase 1 builds; no global installs reference `rollout` (no `pip install -e .`, no `cargo install rollout-cli` yet). | None. Phase 12 (`SHIP-01..02`) handles publish. |

## Environment Availability

**Verified on the dev machine** (the user's macOS environment is the primary target; CI uses macos-14 + ubuntu-latest):

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `cargo` / `rustc` | All Rust crates | ✓ | 1.88.0 (rust-toolchain.toml) | — |
| `make` | `make smoke`, `make protos`, `make lint`, `make test` | ✓ | system | — |
| `mdbook` | `make docs` (already passing in CI) | ✓ | 0.4.40 in CI / 0.4.52 local | Skip docs build (NOT acceptable — Phase 1 requires green). |
| `python3` ≥ 3.11 | Python sidecar sample, pyo3-async-runtimes, schema-gen pipeline | TBD — verify with `python3 --version`. CI uses 3.11. | pin via `python_min = "3.11"` in plugin manifest | If unavailable, sidecar tests skip with a typed `Skipped(NoPython)` (NOT viable for SUBSTR-03 exit criterion). |
| `protoc` | tonic-build / proto compilation | TBD — many devs lack it. **tonic-build 0.14 vendors `protoc`** for most targets, so usually OK. | bundled `protoc` via `protoc-bin-vendored` or check `protoc --version` | Without bundled protoc: `brew install protobuf` / `apt install protobuf-compiler`. Document in `docs/book/src/substrate/index.md`. |
| `libnvidia-ml.so.1` | Linux GPU inventory (optional `nvml-wrapper` feature) | ✗ on macOS, varies on Linux | NVIDIA driver runtime | `nvml-wrapper` is feature-gated; missing libnvidia-ml = empty inventory, never fails (per D-LOCAL-04). |
| `datamodel-code-generator==0.57.0` | Schema-gen pipeline (already wired Phase 1) | TBD; CI installs it | pinned | If missing locally: `pip install datamodel-code-generator==0.57.0`. Phase 1 schema-drift CI job handles drift. |
| `check-jsonschema==0.37.2` | `make validate-schema` (already wired Phase 1) | TBD locally; CI installs it | pinned | If missing locally: `pip install check-jsonschema`. |
| `convco` | commitlint CI job (Phase 1) | TBD locally; CI installs from GitHub release | pinned | Local devs use `git commit -m "feat(...): ..."` discipline; CI is the gate. |
| `node` / `npx` / `graphify-ts` | `make graphify` (local dev only, AGENTS.md §9.6) | ✓ (root `package.json` declares it) | npm install at root | `npx` resolves; no CI dependency. |
| Linux kernel ≥ 3.17 for `MFD_CLOEXEC` etc. | nothing in Phase 2 requires it; cgroups/seccomp are Phase 7 | — | — | — |

**Missing dependencies with no fallback:**
- None blocking. The Python sidecar sample blocks SUBSTR-03's exit criterion ("loads one Python plugin"), so `python3 ≥ 3.11` MUST be available. The smoke test must early-exit with a clear error message if not.

**Missing dependencies with fallback:**
- `libnvidia-ml.so.1` — graceful empty GPU inventory.
- `protoc` — `tonic-build` bundles it for most platforms via `protoc-bin-vendored`; explicit install only needed on unusual targets.

**Recommendation:** add `scripts/preflight.sh` to verify `python3 >= 3.11 && cargo && make` and bail with a friendly error before `make smoke` does anything destructive.

## Common Pitfalls

### Pitfall 1: redb has no post-commit hook
**What goes wrong:** Subscribers via `tokio::sync::broadcast` observe writes that later get rolled back.
**Why it happens:** redb's `WriteTransaction::commit()` returns Result but exposes no callback; abort just drops.
**How to avoid:** Buffer pending events in the transaction; publish ONLY after `commit()` returns `Ok`. See Pattern 2.
**Warning signs:** Tests that intermittently see "extra" events; subscribers observing keys that aren't actually in the DB on re-open.

### Pitfall 2: tonic-h3 v0.0.5 silently lacks bidi-streaming
**What goes wrong:** SUBSTR-02's Work channel (bidi) compiles but hangs or errors at runtime.
**Why it happens:** `tonic-h3` (latest 0.0.5, 2025-11-01) is explicitly "experimental"; the upstream `hyperium/tonic#339` issue (gRPC HTTP/3) is still open.
**How to avoid:** Default `rollout-transport` to HTTP/2 tonic (plan-of-record). Make QUIC a feature flag. Document the swap path.
**Warning signs:** "Stream reset" errors under load; tests that pass single-call but fail bidi.

### Pitfall 3: PyO3 + Tokio runtime entanglement
**What goes wrong:** Tokio worker threads call into Python with the GIL held → other Tokio tasks block; or Python's asyncio loop runs on the wrong thread.
**Why it happens:** `pyo3-async-runtimes` requires a Tokio runtime initialized inside Python's thread; mixing default-runtime Tokio with PyO3 in random tasks leaks GIL.
**How to avoid:** **Dedicated Python OS thread per worker** (CONTEXT D-PLUGIN-02). All Python calls hop via channel. See Pattern 3.
**Warning signs:** Sporadic deadlocks under load; tests pass single-call but hang on concurrent calls.

### Pitfall 4: cdylib hot reload appears to work, then segfaults
**What goes wrong:** The host `dlclose`s a cdylib, drops `Box<dyn Plugin>` pointers; another Tokio task is still holding one. UB.
**Why it happens:** Rust has no stable ABI; even matching toolchains can't safely unload a library still referenced.
**How to avoid:** Return `Fatal(PluginContract("cdylib reload unsupported"))` from `PluginHost::reload` for cdylib handles. **DO NOT** attempt it. (Already in CONTEXT D-PLUGIN-04.)
**Warning signs:** Tests that look fine pass; production hits a segfault under reload after hours.

### Pitfall 5: Smoke-test PID file races
**What goes wrong:** `kill -KILL <w1_pid>` runs against a stale PID from a previous run because cleanup didn't happen.
**Why it happens:** Bash trap not set; CI runs reuse `./data/`; pre-existing `*.pid` files mislead.
**How to avoid:** `set -euo pipefail` + `trap cleanup EXIT` + `rm -rf data/` at script start. Capture PIDs from `$!` immediately after `&`. Time-bound `wait` with `timeout`.
**Warning signs:** "No such process" or smoke test passes by accident; CI is flaky.

### Pitfall 6: dev CA generated per-run breaks reproducibility tests
**What goes wrong:** Integration tests that depend on a stable cert thumbprint fail every run.
**Why it happens:** `rcgen::generate_simple_self_signed` produces a new key every call.
**How to avoid:** Generate once, persist to `./data/tls/ca.pem` + `./data/tls/ca.key.pem` + per-worker `worker-N.{pem,key.pem}`. Tests that need stable certs use `tempdir`. Production never reuses dev CAs (this is a dev/CI feature).
**Warning signs:** Tests that pass twice in a row but fail once on a clean machine.

### Pitfall 7: `tracing` `Instrument` futures lose `RunId` span field across `spawn_blocking`
**What goes wrong:** Storage operations don't carry `run_id` in logs because `spawn_blocking` runs outside the calling span.
**Why it happens:** `tracing` spans are task-local; `spawn_blocking` creates a new task.
**How to avoid:** Use `tracing::Span::current().in_scope()` inside the closure, or capture the span explicitly: `let span = tracing::Span::current(); spawn_blocking(move || span.in_scope(|| ...))`.
**Warning signs:** Logs missing `run_id` selectively on storage ops; observability spec 09 violations.

### Pitfall 8: `serde(deny_unknown_fields)` + `#[serde(default)]` interact subtly with new config fields
**What goes wrong:** Adding a new field to `StorageConfig` breaks every existing config file in tests.
**Why it happens:** `deny_unknown_fields` is correct (per spec 11), but new fields need `#[serde(default)]` until they propagate.
**How to avoid:** Every new config field in Phase 2 either (a) has `#[serde(default = "defaults::...")]` OR (b) is added in a coordinated PR that updates all `tests/smoke/*.toml` fixtures.
**Warning signs:** Phase 2 PR breaks Phase-1 smoke tests with "unknown field" errors.

### Pitfall 9: Python sidecar pulls in gRPC via `pip` and breaks AGENTS.md §7
**What goes wrong:** The sample sidecar plugin `import grpc` requires `pip install grpcio` to run, which violates "every plugin testable locally without ... external services."
**Why it happens:** Default reach for "Python gRPC."
**How to avoid:** Use **hand-rolled length-prefixed framing over UDS** in the in-tree sample (stdlib only), OR ship `grpclib`-based code with prebuilt stubs committed to the repo. **Plan-of-record:** stdlib `socket` + length-prefixed JSON. Move to grpclib only if framing complexity grows. See §"Python sidecar IPC: avoid pip."
**Warning signs:** `make smoke` works on the author's machine but fails on a fresh checkout with "ModuleNotFoundError: No module named 'grpc'".

### Pitfall 10: `cargo machete` (existing CI job) flags optional features as unused
**What goes wrong:** Phase 2 adds `nvml-wrapper` as an optional dep; cargo-machete sees no usage and fails CI.
**Why it happens:** Optional deps behind `cfg(target_os)` aren't always picked up by machete.
**How to avoid:** Use `[target.'cfg(target_os = "linux")'.dependencies]` rather than a feature flag if possible; OR add to `[package.metadata.cargo-machete] ignored = ["nvml-wrapper"]`.
**Warning signs:** PR passes locally but `unused-deps` CI job fails.

## Code Examples

### redb table-per-namespace + postcard value encoding

```rust
// Source: docs.rs/redb (2.x), spec 04 §2, CONTEXT D-STO-04.
use redb::{Database, ReadableTable, TableDefinition};
use serde::{de::DeserializeOwned, Serialize};
use std::path::Path;

pub const T_HEARTBEATS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("heartbeats");

pub struct EmbeddedStorage {
    db:    Arc<Database>,
    watch: Arc<WatchRouter>,
}

impl EmbeddedStorage {
    pub async fn open(path: &Path) -> Result<Self, CoreError> {
        let path = path.to_path_buf();
        let db = tokio::task::spawn_blocking(move || Database::create(path))
            .await
            .map_err(|e| internal(e))?
            .map_err(|e| internal(e))?;
        Ok(Self { db: Arc::new(db), watch: Arc::new(WatchRouter::default()) })
    }
}

pub fn put<V: Serialize>(
    txn: &redb::WriteTransaction,
    table_def: TableDefinition<&[u8], &[u8]>,
    key: &[u8],
    value: &V,
) -> Result<(), CoreError> {
    let mut table = txn.open_table(table_def).map_err(internal)?;
    let bytes = postcard::to_allocvec(value).map_err(internal)?;
    table.insert(key, bytes.as_slice()).map_err(internal)?;
    Ok(())
}

pub fn get<V: DeserializeOwned>(
    txn: &redb::ReadTransaction,
    table_def: TableDefinition<&[u8], &[u8]>,
    key: &[u8],
) -> Result<Option<V>, CoreError> {
    let table = txn.open_table(table_def).map_err(internal)?;
    let g = table.get(key).map_err(internal)?;
    Ok(g.map(|v| postcard::from_bytes(v.value()).map_err(internal)).transpose()?)
}
```

### mTLS server with rcgen-generated dev CA

```rust
// Source: rcgen docs.rs/rcgen + tonic 0.14 ServerTlsConfig.
use rcgen::{Certificate, CertificateParams, IsCa, KeyPair, BasicConstraints};
use tonic::transport::{Identity, Server, ServerTlsConfig};

pub fn ensure_dev_ca(dir: &Path) -> Result<(Vec<u8>, Vec<u8>), CoreError> {
    let ca_pem  = dir.join("ca.pem");
    let ca_key  = dir.join("ca.key.pem");
    if ca_pem.exists() && ca_key.exists() {
        return Ok((std::fs::read(&ca_pem)?, std::fs::read(&ca_key)?));
    }
    std::fs::create_dir_all(dir)?;
    let mut params = CertificateParams::new(vec!["rollout-dev-ca".into()]).unwrap();
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let kp = KeyPair::generate().map_err(internal)?;
    let cert = params.self_signed(&kp).map_err(internal)?;
    let cert_pem = cert.pem();
    let key_pem  = kp.serialize_pem();
    std::fs::write(&ca_pem, &cert_pem)?;
    std::fs::write(&ca_key, &key_pem)?;
    std::fs::set_permissions(&ca_key, std::fs::Permissions::from_mode(0o600))?;
    Ok((cert_pem.into_bytes(), key_pem.into_bytes()))
}

pub async fn serve_h2_mtls(
    addr: SocketAddr,
    server_cert: Vec<u8>,
    server_key:  Vec<u8>,
    client_ca:   Vec<u8>,
    heartbeat:   HeartbeatServer<impl Heartbeat>,
) -> Result<(), CoreError> {
    let identity   = Identity::from_pem(server_cert, server_key);
    let client_ca  = tonic::transport::Certificate::from_pem(client_ca);
    let tls        = ServerTlsConfig::new().identity(identity).client_ca_root(client_ca);
    Server::builder()
        .tls_config(tls).map_err(internal)?
        .add_service(heartbeat)
        .serve(addr)
        .await
        .map_err(internal)
}
```

### Content-addressed sharded FS object store

```rust
// Source: CONTEXT D-LOCAL-01.
pub async fn put(&self, mut body: impl AsyncRead + Unpin + Send, hint: PutHint) -> Result<ContentId, CoreError> {
    let mut hasher = blake3::Hasher::new();
    let mut buf    = Vec::with_capacity(64 * 1024);
    let mut tmp    = tempfile::NamedTempFile::new_in(&self.root)?;
    let mut writer = tokio::io::BufWriter::new(tokio::fs::File::from_std(tmp.reopen()?));
    loop {
        let n = body.read_buf(&mut buf).await?;
        if n == 0 { break; }
        hasher.update(&buf[..n]);
        writer.write_all(&buf[..n]).await?;
        buf.clear();
    }
    writer.flush().await?;
    writer.into_inner().sync_all().await?;     // fsync content
    let cid = ContentId(hasher.finalize().into());
    let hex = hex::encode(cid.0);
    let dir = self.root.join(&hex[0..2]).join(&hex[2..4]);
    tokio::fs::create_dir_all(&dir).await?;
    let final_path = dir.join(&hex);
    tokio::fs::rename(tmp.path(), &final_path).await?;
    tokio::fs::write(final_path.with_extension("meta.json"), serde_json::to_vec(&ObjectMeta {
        size: hint.expected_size, content_type: hint.content_type, created_at: now(),
    })?).await?;
    Ok(cid)
}
```

### Smoke-test script shape

```bash
#!/usr/bin/env bash
# scripts/smoke.sh — SUBSTR-02/03/04 acceptance gate.
set -euo pipefail

ROOT=$(git rev-parse --show-toplevel)
cd "$ROOT"

DATA_DIR="${ROOT}/data/smoke"
LOGS_DIR="${ROOT}/data/smoke/logs"
rm -rf "$DATA_DIR"
mkdir -p "$LOGS_DIR"

# Cleanup on any exit
PIDS=()
cleanup() {
    set +e
    for pid in "${PIDS[@]:-}"; do kill -TERM "$pid" 2>/dev/null; done
    sleep 0.5
    for pid in "${PIDS[@]:-}"; do kill -KILL "$pid" 2>/dev/null; done
}
trap cleanup EXIT INT TERM

# Build prerequisites
cargo build -p rollout-cli -p rollout-coordinator --release
cargo build -p rollout-cli --release --features cdylib-sample
# (Or: build the sample cdylib explicitly into tests/smoke/plugins/)

# Start coordinator
target/release/rollout-coordinator run --config tests/smoke/coordinator.toml \
    >"$LOGS_DIR/coord.log" 2>&1 &
COORD_PID=$!
PIDS+=("$COORD_PID")

# Wait until coordinator's heartbeat port is up
for i in {1..50}; do
    nc -z 127.0.0.1 50051 && break || sleep 0.1
done

# Spawn two workers, each loading 1 cdylib + 1 Python sidecar plugin
target/release/rollout worker run --config tests/smoke/worker.toml \
    --worker-id w1 \
    --plugin tests/smoke/plugins/rust_cdylib_sample.so \
    --plugin python/examples/sample_sidecar \
    >"$LOGS_DIR/w1.log" 2>&1 &
W1_PID=$!
PIDS+=("$W1_PID")

target/release/rollout worker run --config tests/smoke/worker.toml \
    --worker-id w2 \
    --plugin tests/smoke/plugins/rust_cdylib_sample.so \
    --plugin python/examples/sample_sidecar \
    >"$LOGS_DIR/w2.log" 2>&1 &
W2_PID=$!
PIDS+=("$W2_PID")

# Wait for heartbeat-stable: both workers logged "worker_registered"
for i in {1..50}; do
    if grep -q "worker_registered" "$LOGS_DIR/w1.log" && \
       grep -q "worker_registered" "$LOGS_DIR/w2.log"; then
        break
    fi
    sleep 0.1
done

# Kill w1 hard
kill -KILL "$W1_PID"
PIDS=("${PIDS[@]/$W1_PID}")    # remove from cleanup list

# heartbeat_interval=500ms, coord_failure_timeout=5s — wait 2x heartbeat_interval = 1s
# allow 6s total (failure_timeout + skew)
deadline=$(( $(date +%s) + 8 ))
while [ "$(date +%s)" -lt "$deadline" ]; do
    if grep -q "worker_failed.*w1" "$LOGS_DIR/coord.log"; then
        echo "smoke: PASS — coordinator detected w1 failure"
        exit 0
    fi
    sleep 0.2
done

echo "smoke: FAIL — coordinator did not detect w1 failure within 8s"
echo "--- coord.log ---"; tail -40 "$LOGS_DIR/coord.log"
echo "--- w2.log ---";    tail -20 "$LOGS_DIR/w2.log"
exit 1
```

### Plugin manifest TOML (spec 03 §2 — Phase 2 minimum)

```toml
# rollout-plugin.toml — committed alongside each plugin.
[plugin]
name    = "sample-sidecar"
version = "0.1.0"
kind    = "custom"                        # one of: env-harness | tool-harness | eval-harness |
                                          #         reward-model | inference-backend |
                                          #         storage | queue | object-store | custom
trait   = "rollout_core::Plugin"          # the trait the plugin implements
mode    = "sidecar"                       # one of: pyo3 | sidecar | rust-cdylib

[runtime]
python_min = "3.11"                       # only for pyo3 / sidecar
gpu        = false
memory_mib = 256

[entry]
# sidecar:
command  = ["python3", "-m", "sample_sidecar"]
protocol = "grpc-uds"                     # tonic over UDS in Phase 2
socket   = "./data/sidecars/{plugin_name}-{pid}.sock"

# pyo3 (when mode = "pyo3"):
# module  = "sample_inproc.plugin"
# factory = "create_plugin"

# rust-cdylib (when mode = "rust-cdylib"):
# cdylib  = "libsample_cdylib.so"
# symbol  = "rollout_plugin_factory"

[config]
schema = "schema.json"                    # JSON Schema for the plugin's config block

[network]                                 # CONTEXT D-SANDBOX-01: enforced as allowlist in Phase 2
egress_allowed = []
```

### Sidecar gRPC proto (sketch — `rollout-proto/proto/plugin.proto`)

```proto
// Source: spec 03 §3.3, §4, §7.
syntax = "proto3";
package rollout.plugin.v1;

import "google/protobuf/struct.proto";

service Plugin {
  // One-time init. Called once per worker per plugin instance.
  rpc Init(InitRequest) returns (InitResponse);

  // Optional preflight. Called after Init, before Run.
  rpc Preflight(PreflightRequest) returns (PreflightResponse);

  // Generic call. method is the typed entry point name; payload is opaque JSON
  // (Phase 2 uses serde_json::Value); deps are injected via dial-back UDS.
  rpc Call(CallRequest) returns (CallResponse);

  // Hot reload signal (host-side initiated; sidecar can refuse).
  rpc Reload(ReloadRequest) returns (ReloadResponse);

  // Cleanup. Sidecar exits cleanly after returning.
  rpc Shutdown(ShutdownRequest) returns (ShutdownResponse);
}

message InitRequest {
  string plugin_id = 1;
  google.protobuf.Struct config = 2;
}
message InitResponse  { string version = 1; }

message PreflightRequest  {}
message PreflightResponse {}

message CallRequest {
  string method = 1;
  bytes  payload = 2;                // postcard or json bytes; plugin-defined
}
message CallResponse {
  bytes  payload = 1;
  string error   = 2;                // empty on success
}

message ReloadRequest  { string reason = 1; }
message ReloadResponse {}

message ShutdownRequest  { string reason = 1; uint32 grace_secs = 2; }
message ShutdownResponse {}
```

### Transport proto (sketch — `rollout-proto/proto/transport.proto`)

```proto
syntax = "proto3";
package rollout.transport.v1;

import "google/protobuf/timestamp.proto";
import "google/protobuf/duration.proto";

// Heartbeat — unary, frequent (every heartbeat_interval = 500ms).
service Heartbeat {
  rpc Beat(BeatRequest) returns (BeatResponse);
}

message BeatRequest {
  string worker_id = 1;
  google.protobuf.Timestamp due_at = 2;          // worker's next deadline
  WorkerState state = 3;
  string run_id = 4;
}

message BeatResponse {
  google.protobuf.Duration acknowledged_at_drift = 1;     // for clock-skew alarm
  optional ControlPush pending_control = 2;               // optional fast-path
}

enum WorkerState {
  WORKER_STATE_UNSPECIFIED = 0;
  WORKER_STATE_INIT        = 1;
  WORKER_STATE_READY       = 2;
  WORKER_STATE_RUNNING     = 3;
  WORKER_STATE_DRAINING    = 4;
}

// Control — server-streaming. Coordinator pushes drain/snapshot/cancel.
service Control {
  rpc Subscribe(ControlSubscribeRequest) returns (stream ControlPush);
}
message ControlSubscribeRequest { string worker_id = 1; string run_id = 2; }
message ControlPush {
  oneof event {
    DrainRequest      drain = 1;
    SnapshotRequest   snapshot = 2;
    CancelRequest     cancel = 3;
  }
}
message DrainRequest    { google.protobuf.Duration deadline = 1; }
message SnapshotRequest { string snapshot_kind = 1; }
message CancelRequest   { string reason = 1; }

// Work — bidirectional streaming. Phase 2 ships a stub service definition;
// pull/submit semantics are Phase 6 (DIST-01..02). Defined here so the proto
// is forward-compatible.
service Work {
  rpc Stream(stream WorkUp) returns (stream WorkDown);
}
message WorkUp   { oneof up   { string ready = 1; bytes result = 2; } }
message WorkDown { oneof down { bytes  item  = 1; string heartbeat = 2; } }
```

### Python sidecar IPC: avoid pip

```python
# python/examples/sample_sidecar/__main__.py
# Stdlib-only sidecar. Length-prefixed JSON over UDS.
# Plan-of-record per AGENTS.md §7 — every plugin testable locally without external services.
#
# In Phase 2 we ship the sample with stdlib framing because:
#   - grpcio breaks fork() (see grpc/grpc#13235 — known)
#   - pip-installing grpclib violates AGENTS.md §7 for the SAMPLE
#   - User plugins are free to use grpcio/grpclib in their own venv
import socket, struct, json, sys, os

def serve(sock_path: str) -> None:
    if os.path.exists(sock_path): os.remove(sock_path)
    srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    srv.bind(sock_path); srv.listen(1); os.chmod(sock_path, 0o600)
    conn, _ = srv.accept()
    try:
        while True:
            hdr = conn.recv(4)
            if not hdr: break
            (n,) = struct.unpack(">I", hdr)
            payload = b""
            while len(payload) < n:
                chunk = conn.recv(n - len(payload))
                if not chunk: return
                payload += chunk
            req  = json.loads(payload)
            resp = handle(req)
            out  = json.dumps(resp).encode()
            conn.sendall(struct.pack(">I", len(out))); conn.sendall(out)
    finally:
        conn.close(); srv.close(); os.remove(sock_path)

def handle(req: dict) -> dict:
    if req.get("method") == "Init":     return {"version": "0.1.0"}
    if req.get("method") == "Shutdown": sys.exit(0)
    return {"error": f"unknown method: {req.get('method')!r}"}

if __name__ == "__main__":
    serve(sys.argv[1])
```

**Note for planner:** the host side (`rollout-plugin-host`) speaks tonic gRPC to non-sample sidecars, but for the **in-tree sample** the host uses an alternate "framed-json" code path (see CONTEXT D-PLUGIN-03 "no maturin develop step in cargo test"). The actual gRPC proto stubs are still generated and committed to `python/examples/sample_sidecar/_pb/` for users who want them; the sample's own `__main__.py` is stdlib-only.

**Alternative if framed-json proves brittle:** generate Python stubs for `plugin.proto` via `python3 -m grpc_tools.protoc` once (`make protos`), commit the generated code under `python/rollout/_proto/`, and ship the sample sidecar using `grpclib` (pure-Python asyncio gRPC). This requires `grpclib` in a dev-only `python/requirements-dev.txt` but does NOT pollute the cargo test path.

## State of the Art

| Old Approach | Current Approach (2026) | When Changed | Impact |
|--------------|-------------------------|--------------|--------|
| `pyo3-asyncio` (awestlake87) | **`pyo3-async-runtimes`** (PyO3 org) | Fork ~2024; org-blessed 2025 | New crate name. Pin both crates in lockstep. |
| `tonic 0.10/0.11` with `prost 0.11/0.12` | **`tonic 0.14` + `prost 0.13`** | tonic 0.14 (2025+) | `tls-rustls` feature gates rustls; `tonic-build` API minor breaks. |
| `rustls 0.21/0.22` | **`rustls 0.23`** | 2024-2025 | `ring` is now opt-in via feature; `aws-lc-rs` is the new default crypto backend. Pin `ring` feature to keep current. |
| `sled` for embedded KV | **`redb`** | redb hit 1.0 in 2024; 2.x in 2025 | Pure-Rust, MVCC, no compaction stalls. |
| OpenSSL for X.509 | **`rcgen 0.13`** | 2024-2025 | Pure-Rust; banned by deny.toml otherwise. |
| `serde_json` for storage payloads | **`postcard 1.x`** | 1.0 stable since 2023 | Compact, deterministic, schemafull. |

**Deprecated/outdated (avoid):**
- `pyo3-asyncio` (use `pyo3-async-runtimes`).
- `bincode 1.x` (use postcard or bincode 2.x; postcard preferred for determinism).
- `openssl` / `openssl-sys` (banned).
- `sled` (use redb per spec 04 §3.1).
- `rocksdb` (C++ dep; banned by spirit of deny.toml).

## Open Questions

1. **rollout-core trait extension scope** — How much of spec 04/03/01 trait surface lands in Phase 2 vs. waits for later phases?
   - What we know: Phase 1 left stubs (verified — see "Critical Finding"); CONTEXT.md says "not modified in Phase 2" (wrong in practice).
   - What's unclear: whether to extend the full spec surface (more churn now, less later) or only what Phase 2 strictly needs (`Storage::get/put/scan/watch`, `StorageTxn::put/delete/cas/commit/abort`, `Coordinator::heartbeat`, `PluginHost::call`, `Heartbeat` struct).
   - Recommendation: **extend only what Phase 2 needs**, but in a single Wave-0 task that lands atomically. Document the gaps explicitly in `docs/specs/01,03,04` as "Phase X trait extension" markers.

2. **`tonic-h3` re-evaluation cadence** — When do we revisit QUIC?
   - What we know: tonic-h3 0.0.5 is "experimental" (verified 2026-05-19); hyperium/tonic#339 still open.
   - What's unclear: whether QUIC is needed for Phase 6 multi-node (HoL blocking matters more at scale) or can wait to Phase 12.
   - Recommendation: re-evaluate at Phase 6 plan-time; until then, document the feature flag.

3. **rollout-plugin-abi crate vs. inlined module** — Spec 10 §1 declares `rollout-plugin-abi` as a separate internal crate (#21).
   - What we know: spec wants it as a crate; current workspace doesn't have it.
   - What's unclear: whether the C-ABI shim is actually used by external plugin authors in Phase 2 (probably not — they're all in-tree).
   - Recommendation: ship as an **internal module** of `rollout-plugin-host` in Phase 2 (`src/modes/abi.rs`); promote to a separate crate when an external Rust cdylib author appears (likely Phase 7 or later). Document the deferral in `docs/specs/10-component-split.md`.

4. **abi3 strategy for pyo3** — Pin to `abi3-py311`?
   - What we know: pyo3 0.28 supports abi3. Minimum Python 3.9 in pyo3-async-runtimes; 3.11 needed for stdlib `tomllib` in our manifest parsing.
   - Recommendation: yes — `abi3-py311`. Single wheel, smaller CI matrix.

5. **`grpclib` vs stdlib framing for the in-tree Python sidecar sample** — see Pitfall 9 and §"Python sidecar IPC: avoid pip."
   - Recommendation: **stdlib framing** (~30 LOC); ship grpclib only if the next-phase plugin (`rollout-backend-vllm`, Phase 3) needs richer wire format.

6. **redb durability — `Durability::Immediate` everywhere or `Durability::None` for low-priority writes?**
   - What we know: CONTEXT D-STO-03 says "always fsync." `Durability::Immediate` is the redb default.
   - What's unclear: whether `cloudlocal/queue/<id>` mirrors are a hot enough path to drop to `Durability::None` and accept restart-replay loss.
   - Recommendation: ship with `Immediate` everywhere in Phase 2; benchmark in Phase 6 if hot-path becomes a bottleneck.

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | `cargo test` (Rust integration + unit) + `bash` smoke test + `pytest` for Python samples |
| Config file | Workspace `Cargo.toml` + per-crate `tests/`; `scripts/smoke.sh`; `python/examples/sample_*/test_*.py` (if any) |
| Quick run command | `cargo test --workspace --tests` (existing, already in `make test`) |
| Full suite command | `make check` (lint + test) + `make smoke` (new in Phase 2) + `make docs` |
| Smoke command | `make smoke` → `scripts/smoke.sh` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| **SUBSTR-01** | Storage `put/get` round-trip | unit | `cargo test -p rollout-storage --test crud` | ❌ Wave 1 |
| SUBSTR-01 | Storage transaction commit/abort | unit | `cargo test -p rollout-storage --test txn` | ❌ Wave 1 |
| SUBSTR-01 | Storage `watch()` broadcast fan-out | integration | `cargo test -p rollout-storage --test watch` | ❌ Wave 1 |
| SUBSTR-01 | Storage fsync durability (SIGKILL mid-write) | integration | `cargo test -p rollout-storage --test crash_safety -- --ignored` (CI-skipped on macOS if flaky) | ❌ Wave 1 |
| SUBSTR-01 | redb table-per-namespace open-many | unit | `cargo test -p rollout-storage --test tables` | ❌ Wave 1 |
| **SUBSTR-02** | rcgen dev CA + mTLS handshake | integration | `cargo test -p rollout-transport --test tls_dev_ca` | ❌ Wave 2 |
| SUBSTR-02 | Heartbeat unary round-trip | integration | `cargo test -p rollout-transport --test heartbeat` | ❌ Wave 2 |
| SUBSTR-02 | Control server-stream subscribe | integration | `cargo test -p rollout-transport --test control_stream` | ❌ Wave 2 |
| SUBSTR-02 | Deadline detection: kill worker → coord marks failed within 2× hb_interval | integration (smoke) | `make smoke` | ❌ Wave 4 |
| SUBSTR-02 | Plan-time invariants (self_fence < coord_timeout) | unit | `cargo test -p rollout-transport --test config_invariants` | ❌ Wave 2 |
| **SUBSTR-03** | Manifest parse + validate | unit | `cargo test -p rollout-plugin-host --test manifest` | ❌ Wave 3 |
| SUBSTR-03 | Load cdylib + call + unload | integration | `cargo test -p rollout-plugin-host --test cdylib_load -- --ignored` (Linux+macOS) | ❌ Wave 3 |
| SUBSTR-03 | Load PyO3 in-process + call | integration | `cargo test -p rollout-plugin-host --test pyo3_load` | ❌ Wave 3 |
| SUBSTR-03 | Spawn sidecar + call + shutdown | integration | `cargo test -p rollout-plugin-host --test sidecar_load` | ❌ Wave 3 |
| SUBSTR-03 | Hot-reload PyO3 mid-call (dev feature) | integration | `cargo test -p rollout-plugin-host --features dev-hot-reload --test reload_pyo3` | ❌ Wave 3 |
| SUBSTR-03 | Hot-reload sidecar (SIGTERM + respawn) | integration | `cargo test -p rollout-plugin-host --features dev-hot-reload --test reload_sidecar` | ❌ Wave 3 |
| SUBSTR-03 | Cdylib reload returns Fatal(PluginContract) | unit | `cargo test -p rollout-plugin-host --test reload_cdylib_unsupported` | ❌ Wave 3 |
| SUBSTR-03 | Smoke: load 1 cdylib + 1 Python sidecar per worker | integration (smoke) | `make smoke` | ❌ Wave 4 |
| **SUBSTR-04** | FS object store sharded-layout put/get | unit | `cargo test -p rollout-cloud-local --test object_store` | ❌ Wave 1 |
| SUBSTR-04 | In-mem queue with Storage spill replay after restart | integration | `cargo test -p rollout-cloud-local --test queue_replay` | ❌ Wave 1 |
| SUBSTR-04 | SecretStore env-var allowlist | unit | `cargo test -p rollout-cloud-local --test secrets` | ❌ Wave 1 |
| SUBSTR-04 | SecretStore put() returns Fatal(ConfigInvalid) | unit | `cargo test -p rollout-cloud-local --test secrets` | ❌ Wave 1 |
| SUBSTR-04 | ComputeHint Linux /proc parsing | integration `#[cfg(linux)]` | `cargo test -p rollout-cloud-local --test hints_linux` | ❌ Wave 1 |
| SUBSTR-04 | ComputeHint macOS sysinfo stub | integration `#[cfg(macos)]` | `cargo test -p rollout-cloud-local --test hints_macos` | ❌ Wave 1 |
| **DOCS-01..03** | Substrate mdBook chapters render + crate-level //! docs present | CI | `cargo doc --workspace --no-deps --all-features` + `mdbook build docs/book` | ✓ (existing CI jobs `rustdoc-check` + `docs-build`) |
| DOCS-02 | Every commit touches docs/tests | CI | `scripts/check-docs-tests-touched.sh` (existing) | ✓ |
| Cross-crate | Plugin host calls Storage to persist a plugin manifest | integration | `cargo test -p rollout-plugin-host --test storage_integration` | ❌ Wave 3 |
| Cross-crate | Coordinator persists worker registry to Storage | integration | `cargo test -p rollout-coordinator --test registry_persistence` | ❌ Wave 3 |
| Cross-crate | Worker → transport → coordinator heartbeat flow | integration (smoke) | `make smoke` | ❌ Wave 4 |
| Architecture | New crates pass dep-direction lint | CI | `cargo test -p rollout-core --test dependency_direction` (existing, extended) | ✓ (existing job; extend fixture) |
| Architecture | `rollout-transport` does NOT depend on `rollout-cloud-*` | unit | extension to `dependency_direction.rs` | ❌ Wave 0 |
| Architecture | `rollout-plugin-host` does NOT depend on `rollout-transport` | unit | extension to `dependency_direction.rs` | ❌ Wave 0 |
| Schema | New `[storage]` / `[transport]` / `[plugins]` / `[cloud.local]` blocks regenerate cleanly | CI | `cargo xtask schema-gen && git diff --exit-code` (existing job) | ✓ (existing; extend fixture configs) |

### Sampling Rate

- **Per task commit:** `cargo test -p <touched-crate> --tests` (≤ 30s typical; Phase 2's per-crate test surface is small).
- **Per wave merge:** `make check` (lint + workspace test) + `make smoke` for waves W3 and W4.
- **Phase gate:** `make check && make smoke && make docs` all green; CI runs same on PR; `/gsd:verify-work` confirms.

### Wave 0 Gaps

- [ ] **`crates/rollout-core/src/traits/storage.rs`** — extend `Storage` with `get`/`get_many`/`scan`/`watch`; extend `StorageTxn` with `put`/`delete`/`cas`/`abort`. Add `StorageKey { namespace, run_id, path }`, `KeyRange`, `StorageEvent`. Covers REQ SUBSTR-01.
- [ ] **`crates/rollout-core/src/traits/plugin.rs`** — extend `PluginHost` with `call<Req,Res>` / `reload` / `unload`; add `PluginHandle`, `PluginManifest`, `PluginDependencies`. Covers REQ SUBSTR-03.
- [ ] **`crates/rollout-core/src/traits/worker.rs`** — extend `Coordinator` with `heartbeat(Heartbeat)`; extend `Worker` with `init` / `ready` lifecycle hooks; add `Heartbeat`, `WorkerState`. Covers REQ SUBSTR-02.
- [ ] **`crates/rollout-core/src/traits/cloud.rs`** — verify/extend `ObjectStore` / `Queue` / `SecretStore` / `ComputeHint` to spec 06 §3. Covers REQ SUBSTR-04.
- [ ] **`crates/rollout-core/src/config/`** — add `StorageConfig`, `TransportConfig`, `PluginsConfig`, `CloudLocalConfig` modules with `JsonSchema` derives. Wire them into `RunConfig`.
- [ ] **`docs/specs/01,03,04,06`** — update specs in same PR if any extension differs from current spec text (AGENTS.md §4).
- [ ] **`crates/rollout-core/tests/dependency_direction.rs`** — add fixtures for new invariants (`rollout-transport` ↛ `rollout-cloud-*`; `rollout-plugin-host` ↛ `rollout-transport`).
- [ ] **Workspace `Cargo.toml`** — register six new crates (`rollout-proto`, `rollout-storage`, `rollout-cloud-local`, `rollout-transport`, `rollout-plugin-host`, `rollout-coordinator`).
- [ ] **Framework install:** `protoc` (or rely on tonic-build's bundled vendored protoc). Verify with `protoc --version` in `scripts/preflight.sh`.

*(Wave 0 = before any Wave 1 stream begins. These gaps would block downstream waves if missing.)*

## Sources

### Primary (HIGH confidence)
- [rollout repo `docs/specs/03-plugin-system.md`](docs/specs/03-plugin-system.md) — manifest schema, host trait, hot reload, security.
- [rollout repo `docs/specs/04-storage-snapshots.md`](docs/specs/04-storage-snapshots.md) — Storage trait, redb selection rationale.
- [rollout repo `docs/specs/05-distribution.md`](docs/specs/05-distribution.md) — three channels, deadline-based health, fault tolerance.
- [rollout repo `docs/specs/06-cloud-layer.md`](docs/specs/06-cloud-layer.md) — ObjectStore/Queue/SecretStore/ComputeHint contracts.
- [rollout repo `docs/specs/01-core-runtime.md`](docs/specs/01-core-runtime.md) — Worker/Coordinator/Heartbeat/lifecycle.
- [rollout repo `docs/specs/09-observability.md`](docs/specs/09-observability.md) — EventEmitter, structured events.
- [rollout repo `docs/specs/10-component-split.md`](docs/specs/10-component-split.md) — crate map and dep graph.
- [rollout repo `docs/specs/11-config-schema.md`](docs/specs/11-config-schema.md) — single source of truth rules.
- [rollout repo `AGENTS.md`](AGENTS.md) — north-star principles + §9 standing rules.
- [rollout repo `crates/rollout-core/src/traits/*.rs`](crates/rollout-core/src/traits/) — actual Phase-1 trait surface (the "Critical Finding" source).
- [redb crates.io / docs.rs](https://docs.rs/redb) — TableDefinition API, design.md (multi-table, no post-commit hook).
- [pyo3-async-runtimes README on GitHub](https://github.com/PyO3/pyo3-async-runtimes/blob/main/README.md) — version 0.28, Python 3.9+ support, tokio + async-std runtimes, MSRV 1.83.
- [tonic 0.14 docs.rs](https://docs.rs/tonic/latest/tonic/transport/index.html) — Server::builder().tls_config(), ServerTlsConfig, Identity::from_pem, client_ca_root.
- [tonic UDS examples + hyperium/tonic#136, #826, #856](https://github.com/hyperium/tonic/issues/856) — UnixListenerStream pattern.
- [rcgen GitHub](https://github.com/rustls/rcgen) — generate_simple_self_signed, CertificateParams::self_signed.
- [nvml-wrapper crates.io](https://crates.io/crates/nvml-wrapper/dependencies) — MIT/Apache-2.0, libloading-based.

### Secondary (MEDIUM confidence)
- [tonic-h3 GitHub](https://github.com/youyuanwu/tonic-h3) — v0.0.5 "experimental", 2025-11-01, MIT.
- [hyperium/tonic#339 (gRPC HTTP/3 support)](https://github.com/hyperium/tonic/issues/339) — open issue tracking native h3 support.
- [postcard crates.io + spec](https://docs.rs/postcard/latest/postcard/) — wire format stable since 1.0, schemafull.
- [sysinfo crates.io](https://crates.io/crates/sysinfo) — cross-platform, macOS-friendly.
- [tracing-subscriber EnvFilter docs](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html) — RUST_LOG-driven filtering.
- [libloading crates.io + nullderef.com blog series](https://nullderef.com/blog/plugin-dynload/) — cdylib + libloading patterns; abi_stable for richer ABI; async over C-ABI is unsupported.
- [grpclib README](https://grpclib.readthedocs.io/en/latest/) — pure-Python asyncio gRPC for sidecar fallback.

### Tertiary (LOW confidence — flag for validation at implementation time)
- Exact `tonic-h3` bidi-streaming support — could not verify in v0.0.5 README; the recommended action is to prototype and fall back to H/2 on any signal of regression. Treat HTTP/2 as the plan-of-record (see Pitfall 2).
- Exact runtime cost of redb table-per-namespace at ~10 tables vs single-table — no production benchmark found; verify with a quick `criterion` bench in Wave 1 if it becomes a smell.
- Whether tonic-build 0.14 bundles `protoc` on all developer platforms — verify in `scripts/preflight.sh`; install manually if missing.

## Metadata

**Confidence breakdown:**
- Standard stack (storage, cloud-local, plugin-host, observability): **HIGH** — direct doc citation + verified against the working tree.
- Transport (HTTP/2 plan-of-record): **HIGH** — tonic 0.14 mTLS is well-trodden.
- Transport (QUIC stretch): **LOW** — tonic-h3 v0.0.5 experimental; recommended deferral.
- Architecture patterns (redb tables, broadcast watch, dedicated PyO3 thread, UDS sidecar): **HIGH** — patterns confirmed against authoritative sources.
- Pitfalls: **HIGH** — derived from spec text + common community traps + direct verification of redb/tonic-h3 capabilities.
- Trait extension scope: **MEDIUM** — derived from inspecting the working tree, but exact extension subset is a planner decision.

**Research date:** 2026-05-19
**Valid until:** ~2026-06-19 (30 days for stable parts: redb, postcard, rcgen, sysinfo, libloading). **7 days** for tonic-h3 maturity reassessment (fast-moving).
