# Phase 2: Local substrate — Context

**Gathered:** 2026-05-19
**Status:** Ready for planning
**Source:** Synthesized from `/gsd:discuss-phase 2` Q&A + `ROADMAP.md` Phase 2 + `.planning/REQUIREMENTS.md` SUBSTR-01..04 + `AGENTS.md` §9 + `docs/specs/03..06,09` + `.planning/phases/01-core-foundations/01-CONTEXT.md`

<domain>
## Phase Boundary

Phase 2 builds the **local substrate** — the layer that lets a worker start, store state, talk to a peer, load a plugin, and shut down cleanly **without touching any cloud**. Six crates ship:

- **`rollout-proto`** — owns `transport.proto` (heartbeat / control / work) + `plugin.proto` (sidecar). `tonic-build` runs here; other crates consume the generated code.
- **`rollout-storage`** — implements `Storage` + `StorageTxn` on top of **redb**. In-process `tokio::sync::broadcast` per-prefix for `watch()`. Always-fsync. Default path `./data/rollout.db`. postcard value encoding.
- **`rollout-transport`** — implements gRPC-over-QUIC via **tonic + h3 + quinn** with three logical channels (heartbeat / control / work). mTLS by default with auto-generated per-run dev CA under `./data/tls/`. **Conditional fallback:** if `tonic-h3` (or equivalent) is not production-ready for bidi-streaming, ship HTTP/2 tonic in Phase 2 — same proto schema, swap-to-QUIC in a later phase.
- **`rollout-plugin-host`** — implements `PluginHost` with **all three** modes wired: Rust cdylib, PyO3 in-process, Python sidecar (gRPC-over-UDS). PyO3 strategy: `pyo3-async-runtimes` + dedicated Python OS thread per worker. Full hot-reload for PyO3 + sidecar; cdylib reload explicitly unsupported per spec 03 §7.
- **`rollout-cloud-local`** — implements `ObjectStore` (content-addressed sharded FS under `./data/object-store/`), `Queue` (RAM hot path + spill to `rollout-storage` for restart replay), `SecretStore` (env-var allowlist, read-only), `ComputeHint` (Linux full via `/proc` + nvml; macOS minimal stub via `sysinfo`). `BlockStore` skipped — optional per spec.
- **`rollout-coordinator`** — minimal binary that registers workers, accepts heartbeats, persists worker registry + heartbeat ledger to Storage, and marks workers failed via deadline-based scan. No work distribution, no work-stealing, no lease (Phase 6 scope).

Plus: **smoke test** (`make smoke` + `scripts/smoke.sh`) spawning 1 coordinator + 2 workers, loading 1 cdylib + 1 Python sidecar plugin, killing w1, and asserting deadline-based detection within `2 × heartbeat_interval`.

**Out of scope (explicit):**

- Postgres `Storage` backend — Phase 4 (`TRAIN-04`).
- Real cloud impls (S3, SQS, GCS, Pub/Sub, Secrets Manager) — Phase 5 (`CLOUD-01`, `CLOUD-02`).
- Work-stealing, lease-based coordinator HA, multi-node restart-from-storage test — Phase 6 (`DIST-01..05`).
- Inference backend, training algorithms, harnesses — Phases 3+.
- Process snapshots (CRIU), episodic-memory snapshots — Phases 11, 8.
- Sidecar sandbox enforcement beyond network allowlist (cgroups + seccomp + FD limits + fs write restrictions) — Phase 7 (when tool harness lands and adversarial isolation matters).

</domain>

<decisions>
## Implementation Decisions

### Storage (`rollout-storage`)

- **D-STO-01** — Embedded backend = **redb** (per spec 04 §3.1 "preferred"; single-file MVCC, copy-on-write, no compaction stalls). The async `Storage` trait wraps the sync redb API via `tokio::task::spawn_blocking`.
- **D-STO-02** — `watch()` implemented via `tokio::sync::broadcast` per key-prefix. Commit hooks fan events out to subscribers. Documented as **in-process only**; cross-process watch arrives with the Postgres backend in Phase 4.
- **D-STO-03** — Durability mode: **always fsync** on commit. Default path `./data/rollout.db`, overridable via `[storage.embedded] path = ...`.
- **D-STO-04** — Value encoding: **postcard**. Compact, schemafull, deterministic, serde-native.

### Plugin host (`rollout-plugin-host`)

- **D-PLUGIN-01** — All three modes ship as wired loaders: **Rust cdylib**, **PyO3 in-process**, **Python sidecar (gRPC-over-UDS)**. Phase 3 (vLLM via PyO3) and Phase 7 (sidecar sandbox for tool harness) inherit a working host.
- **D-PLUGIN-02** — PyO3 ↔ Tokio bridge: **`pyo3-async-runtimes`** + a dedicated Python OS thread per worker. **Pin both `pyo3` and `pyo3-async-runtimes` versions early** (ROADMAP risk callout). Python coroutines schedule onto Tokio via the crate's `tokio` feature.
- **D-PLUGIN-03** — In-tree plugin samples live under `python/examples/sample_sidecar/` and `python/examples/sample_inproc/`. Sidecar launches via `python -m sample_sidecar` using whichever `python3` is on PATH. **No `maturin develop` step in `cargo test`** — keeps the AGENTS.md "every plugin testable locally without cloud creds / GPUs / extra system deps" rule clean.
- **D-PLUGIN-04** — **Full hot-reload** ships in Phase 2 for PyO3 (`importlib.reload`) and sidecar (SIGTERM + respawn). Rust cdylib reload returns a typed `Fatal(PluginContract)` "unsupported" error per spec 03 §7. Drain-in-flight semantics implemented; hot-reload gated behind a `dev` feature or `--hot-reload` CLI flag.

### Transport (`rollout-transport`)

- **D-TRANS-01** — gRPC stack = **tonic + h3 + quinn**. **Researcher MUST validate** that `tonic-h3` (or the most-maintained equivalent) supports bidi-streaming for the Work channel against spec 05 §3. **Documented fallback:** if QUIC is not production-ready, ship **HTTP/2 tonic** with the same proto schema; the swap-to-QUIC then becomes a single-config change in a later phase.
- **D-TRANS-02** — TLS = **mTLS by default**. On first run, the CLI generates a per-run self-signed CA + cert/key under `./data/tls/` (gitignored). No manual openssl steps required.
- **D-TRANS-03** — Three logical channels per spec 05 §3: heartbeat (unary, frequent), control (server-streaming), work (bidirectional streaming). Multiplexed over one QUIC connection (or one H/2 connection in fallback mode).

### Coordinator slice (`rollout-coordinator`)

- **D-COORD-01** — Phase 2 ships **`rollout-coordinator`** as a real crate with a binary. Surface: register-worker, accept-heartbeat, persist worker registry + heartbeat ledger to Storage, deadline-based failure scan. **Explicitly out of scope:** work distribution, work-stealing, lease/CAS, multi-coordinator. All of those land in Phase 6 (`DIST-01..05`).
- **D-COORD-02** — Smoke test = `make smoke` shells to `scripts/smoke.sh`, which:
  1. Spawns `rollout coordinator run --config tests/smoke/coordinator.toml &`.
  2. Spawns 2× `rollout worker run --config tests/smoke/worker.toml --worker-id w{1,2} &`.
  3. Loads `tests/smoke/plugins/rust_cdylib_sample.so` + `python/examples/sample_sidecar/` via worker CLI plugin flags.
  4. After heartbeat-stable, `kill -KILL <w1_pid>`.
  5. Asserts the coordinator marks w1 failed within `2 × heartbeat_interval`.
  6. Returns non-zero on assertion failure.
  Wired into the CI `test` job and `make test` target.

### cloud-local (`rollout-cloud-local`)

- **D-LOCAL-01** — `ObjectStore` writes content-addressed blobs under `./data/object-store/<sha256[0..2]>/<sha256[2..4]>/<sha256>` (two-level shard) with a sibling `.meta` JSON for content-type + size + created_at.
- **D-LOCAL-02** — `Queue` hot path is `tokio::sync::Mutex<VecDeque<_>>`; every enqueue/ack/nack mirrors to Storage under `cloudlocal/queue/<id>` (postcard). On restart, the queue replays unacknowledged items. Honors the spirit of DIST-03 ("coordinator restart from storage") for the local backend even though DIST-03 itself is Phase 6.
- **D-LOCAL-03** — `SecretStore` reads `ROLLOUT_SECRET_<KEY>` env vars filtered through a config-defined allowlist (per spec 03 §10 "no env inheritance beyond allowlist"). `put()` returns `Fatal(ConfigInvalid)` — the local secret store is **read-only by design**.
- **D-LOCAL-04** — `ComputeHint`:
  - **Linux:** full impl (`/proc/cpuinfo`, `/proc/meminfo`; GPU via `nvml-wrapper` feature-gated; missing NVML = empty inventory, never fails).
  - **macOS:** minimal stub via `sysinfo`; `gpu_inventory()` returns empty; `preemption_signal()` returns `None`.
  - Linux-only integration tests gated by `#[cfg(target_os = "linux")]`.
- **D-LOCAL-05** — `BlockStore` **skipped** in Phase 2 (optional per spec 06 §2). Trait stays declared in `rollout-core`; `rollout-cloud-local` does not implement it.

### Heartbeat / timing defaults

- **D-TIME-01** — Default constants (v1, until Phase 6 tunes them with real cluster data):
  - `heartbeat_interval = 500 ms`
  - `worker_self_fence_timeout = 4 s`
  - `coordinator_failure_timeout = 5 s`
  - `clock_skew_budget = 250 ms`
- **D-TIME-02** — Invariants enforced at **config-validate time** (principle #3, plan-time validation):
  - `worker_self_fence_timeout < coordinator_failure_timeout` (spec 05 §6 split-brain prevention)
  - `clock_skew_budget < heartbeat_interval × 2`
  Configs that violate fail at `rollout plan`, never at runtime.

### Cross-crate plumbing

- **D-PROTO-01** — Dedicated **`rollout-proto`** crate owns `transport.proto` + `plugin.proto`. `tonic-build` runs in its `build.rs`. `rollout-transport`, `rollout-plugin-host`, and external Rust sidecar plugin authors all depend on `rollout-proto`. Python stubs generated via `make protos` (one-shot; committed to repo so sidecar samples don't require a build step).
- **D-OBSERVE-01** — `tracing` skeleton wired workspace-wide. Library crates emit spans/events only; binary crates configure the subscriber (default = `tracing-subscriber` + `EnvFilter`, driven by `RUST_LOG`). Critical events: `worker_registered`, `worker_heartbeat`, `worker_failed`, `plugin_loaded`, `plugin_reloaded`, `plugin_call`, `plugin_call_error`. `RunId` + `WorkerId` propagate as span fields. `EventEmitter` trait from spec 09 implemented with a **stdout JSON sink** in Phase 2; richer backends (file, cloud) defer to later phases.
- **D-SANDBOX-01** — Sidecar sandboxing in Phase 2: **network allowlist only** (host-side egress proxy enforces the manifest's `[network]` block; default-deny). cgroups + seccomp + FD limits + fs write restrictions left as TODOs with tracking comments referencing **Phase 7** (when the tool harness lands and untrusted-code isolation becomes load-bearing).

### Wave breakdown (planner reference; planner owns the final structure)

- **Wave 1 (parallel, 3 streams):** `rollout-proto` · `rollout-storage` · `rollout-cloud-local` — no cross-deps among the three.
- **Wave 2:** `rollout-transport` — depends on `rollout-proto`.
- **Wave 3 (parallel, 2 streams):** `rollout-plugin-host` · `rollout-coordinator` — both depend on `rollout-proto` + `rollout-storage` + `rollout-transport`; they don't depend on each other.
- **Wave 4:** smoke test + mdBook chapters under `docs/book/src/substrate/` + CI wiring (extend existing 11-job workflow with a `smoke` job; tighten `architecture-lint` to cover the new crates).

### Claude's Discretion

- Specific `pyo3` and `pyo3-async-runtimes` version pins (researcher picks a known-good pair; pin in `Cargo.toml` with a comment).
- Manifest format details — TOML keys, discovery search-paths precedence (spec 03 §8).
- Minimum supported Python version (recommendation: 3.11+ for stdlib `tomllib` + PyO3 abi3 stability).
- PyO3 abi3 strategy — yes/no; if yes, which Python minor.
- cdylib plugin-abi shim crate naming and exported C symbols (likely `rollout-plugin-abi`).
- `nvml-wrapper` vs hand-rolled FFI for GPU inventory.
- Internal redb table layout — table-per-namespace vs single `kv(namespace, key, value)`.
- Specific `tonic-h3` (or successor) crate choice — pick the most-maintained option at implementation time.
- mdBook section structure for substrate docs.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Roadmap & requirements

- `ROADMAP.md` §"Phase 2 — Local substrate" — narrative goal, includes, exit criteria, risks (authoritative for scope)
- `.planning/REQUIREMENTS.md` — `SUBSTR-01..04` (authoritative for what must ship)
- `.planning/ROADMAP.md` — phase → requirement mapping

### Architectural source-of-truth

- `AGENTS.md` — all 10 north-star principles. Phase-2-load-bearing: #1 async-native end-to-end, #5 deadline-based health, #7 every plugin locally testable, #8 hot reload for plugins, #9 layered cloud abstraction, #10 observability not optional
- `AGENTS.md` §9 — standing rules (DOCS-01, DOCS-02, DOCS-03 + v1-example commitment; per-commit doc/test policy applies to every Phase 2 commit)
- `ARCHITECTURE.md` — layered architecture, Layer 1 (substrate) contents

### Phase-2 canonical specs (implementation contracts)

- `docs/specs/03-plugin-system.md` — `Plugin`, `PluginHost` traits; §2 manifest; §3 loading modes; §5 host trait; §7 hot reload; §10 security; §11 host test contract
- `docs/specs/04-storage-snapshots.md` §1–§3 — `Storage`, `StorageTxn` traits; embedded-backend properties; selection rules
- `docs/specs/05-distribution.md` §3, §6 — transport channels, deadline-based health, fault tolerance
- `docs/specs/06-cloud-layer.md` — `ObjectStore` / `Queue` / `SecretStore` / `ComputeHint` traits and the `rollout-cloud-local` contract
- `docs/specs/09-observability.md` — `EventEmitter` trait + structured event shape
- `docs/specs/01-core-runtime.md` — `Worker`, `Coordinator`, `WorkerContext`, `Heartbeat`, lifecycle state machine
- `docs/specs/10-component-split.md` — dependency-direction rule for the new crates
- `docs/specs/11-config-schema.md` — single-source-of-truth config; new `[storage]`, `[transport]`, `[plugins]`, `[cloud.local]` blocks must follow these rules

### Prior phase context

- `.planning/phases/01-core-foundations/01-CONTEXT.md` — Phase 1 decisions Phase 2 inherits (Makefile shape, CI jobs, schema-gen pipeline, mdBook layout, dependency-direction lint, conventional-commits + per-commit doc/test policy)

### Repo state Phase 2 modifies or extends

- `crates/rollout-core/` — Phase 2 implements its trait surface; does not modify it
- `crates/rollout-cli/` — Phase 2 adds `worker run` and `coordinator run` subcommands
- `xtask/` — Phase 2 may add `xtask gen-protos` for Python sidecar stubs
- `Makefile` — Phase 2 adds `smoke` and `protos` targets; preserves all existing targets
- `.github/workflows/ci.yml` — Phase 2 adds a `smoke` job and tightens `architecture-lint`; never skips or `continue-on-error`s the existing 11 jobs
- `docs/book/src/SUMMARY.md` — Phase 2 adds a substrate section; preserves the reserved `docs/book/src/examples/` placeholder
- `.gitignore` — Phase 2 adds `data/` (runtime state, including `data/tls/`, `data/object-store/`, `data/rollout.db`); preserves existing entries

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets

- **`rollout-core` trait surface** is complete and compiles. Phase 2 imports `Storage`, `StorageTxn`, `Queue`, `ObjectStore`, `SecretStore`, `ComputeHint`, `PluginHost`, `CoreError`, `RetryHint`, `RunId`, `WorkerId`, `ContentId`, `StorageKey`, `KeyRange` from there. Trait definitions are not modified in Phase 2.
- **Single-source config pipeline** (`xtask schema-gen`) already enforces drift. New config blocks (`[storage]`, `[transport]`, `[plugins]`, `[cloud.local]`) are Rust types in each substrate crate's `config` module; the pipeline picks them up automatically.
- **Error taxonomy** is locked: substrate impls map their internal errors into `Recoverable { Throttled, Transient, Preempted }` ∪ `Fatal { ConfigInvalid, SchemaViolation, PluginContract, Internal }`, each with a `RetryHint`.
- **CI workflow (11 jobs)** is live: `lint`, `test`, `deny`, `commitlint`, `schema-drift`, `architecture-lint`, `unused-deps`, `rustdoc-check`, `docs-build`, `docs-deploy`, `docs-test-policy`. Phase 2 must not skip or `continue-on-error` any of them.
- **mdBook docs site** at `docs/book/`; Phase 2 adds chapters under `docs/book/src/substrate/`.

### Established Patterns

- **Cargo workspace** with `crates/*` + `xtask` + `python/*` lanes already declared.
- **`cargo deny`** in `deny.toml` — Phase 2's new transitive deps (`redb`, `tonic`, `h3`, `quinn`, `pyo3`, `pyo3-async-runtimes`, `libloading`, `nvml-wrapper`, `sysinfo`, `postcard`, `rustls`) must pass advisories + licenses + bans (no `openssl-sys`).
- **Architecture-lint test** in `rollout-core/tests/architecture.rs` — Phase 2 adds invariants: algorithm crates (none yet) cannot depend on `rollout-cloud-*`; `rollout-transport` does not depend on `rollout-cloud-*`; `rollout-plugin-host` does not depend on `rollout-transport` for sidecar IPC (sidecar uses UDS via `rollout-proto`, not the QUIC transport).
- **`graphify-ts`** dev-graph regeneration on demand (AGENTS.md §9.6) — use before refactors to eyeball dependency-direction.
- **Conventional commits** + `convco` lint; per-commit doc/test policy (DOCS-02). Every commit modifying `crates/`, `python/`, or `xtask/` must also touch docs or tests in the same diff.

### Integration Points

- `xtask` — add `xtask gen-protos` (or `make protos`) for Python sidecar stubs.
- `Makefile` — extend with `smoke` + `protos`; preserve all existing targets.
- `.github/workflows/ci.yml` — add (a) `smoke` job on `ubuntu-latest`, (b) extend `architecture-lint` invariants. Do NOT touch the existing 11 jobs.
- `.gitignore` — add `data/` (runtime state); preserve `graphify-out/`, `node_modules/`, `target/`.
- `crates/rollout-cli` — add `worker run` + `coordinator run` subcommands; existing `rollout schema` stays untouched.
- `docs/book/src/SUMMARY.md` — add substrate section; existing `examples/` placeholder remains for SHIP-03 (Phase 4+).

</code_context>

<specifics>
## Specific Ideas

- **Spec is the contract.** Match spec 03/04/05/06 trait shapes verbatim. If a spec is wrong, fix it in the same PR (AGENTS.md §4).
- **mTLS bootstrap UX matters.** On first run, the CLI prints "Generated dev CA at ./data/tls/ca.pem" and proceeds — no manual openssl steps.
- **The smoke test is the proof.** If `scripts/smoke.sh` doesn't pass on a clean checkout with only `cargo`, `make`, and `python3` on PATH, Phase 2 is not done.
- **`pyo3` + `pyo3-async-runtimes` version pin is load-bearing.** Pick a known-good pair after researching crates.io publish history + GitHub issue activity. Document the pin in `Cargo.toml` and in `docs/book/src/substrate/python-bridge.md`.
- **Python sample plugins must be `python -m`-runnable with stdlib only.** No third-party `pip install` for the in-tree sample. If a sample needs gRPC, generate Python stubs into the sample directory; reach for `grpcio` only if hand-rolled length-prefixed framing isn't viable.
- **Falling back to HTTP/2 tonic must be a single-config swap.** Same `.proto`, same service traits — only `Server::builder()` wiring differs. Plan for this so the swap is a one-PR change if `tonic-h3` regresses or stalls.
- **redb table-per-namespace** is the likely shape (matches the spec's `StorageKey { namespace, run_id, path }` model). Researcher should verify by reading redb's table-management API.

</specifics>

<deferred>
## Deferred Ideas

- **Postgres `Storage` backend** — Phase 4 (`TRAIN-04`).
- **Real cloud impls** (`rollout-cloud-aws`, `rollout-cloud-gcp`) — Phase 5 (`CLOUD-01`, `CLOUD-02`).
- **Object-store-backed snapshots replacing local-fs** — Phase 5 (`CLOUD-03`).
- **Work distribution, work-stealing, coordinator lease/CAS, multi-node restart-from-storage test** — Phase 6 (`DIST-01..05`).
- **Process snapshots (CRIU-style)** — Phase 11 (`SNAPSHOT-01`).
- **Sidecar full sandbox** (cgroups + seccomp + FD limits + fs write restrictions) — Phase 7 (when `HARNESS-02` brings untrusted code-exec).
- **Multi-coordinator HA + lease handoff** — post-v1 (spec 05 §10).
- **`BlockStore` impl** — opt-in for clouds that need it; not Phase 2.
- **Encrypted object-store traffic** — assumed via cloud SDK defaults in Phase 5.
- **NCCL-aware scheduling** — v2 (spec 05 §10).
- **PyO3 sub-interpreter strategy (PEP 684)** — revisit when PyO3 ships stable sub-interpreter support; v1 uses single-interpreter-per-worker.
- **Cap'n Proto for sidecar IPC** — gRPC over UDS in v1 (spec 03 §12).
- **Cross-process embedded `watch()`** — Postgres backend in Phase 4 covers cross-process; embedded stays in-process-only.

</deferred>

---

*Phase: 02-local-substrate*
*Context gathered: 2026-05-19 via `/gsd:discuss-phase 2`*
