---
phase: 02-local-substrate
plan: 06
subsystem: infra
tags: [coordinator, heartbeat, failure-detection, mtls, ndjson, eventemitter, observability, clap, tonic]

requires:
  - phase: 02-00-wave0-trait-extensions
    provides: Coordinator/EventEmitter/Heartbeat/WorkerState trait surface; FatalError struct-form
  - phase: 02-01-rollout-proto
    provides: transport.proto Heartbeat unary RPC + WorkerState enum + BeatRequest/BeatResponse
  - phase: 02-02-rollout-storage
    provides: EmbeddedStorage open/begin/get_bytes/scan_bytes + workers/heartbeats tables
  - phase: 02-04-rollout-transport
    provides: server::serve + tls::ensure_dev_ca/issue_server_cert/issue_client_cert + health::is_failed + HeartbeatServiceImpl
  - phase: 02-05-rollout-plugin-host
    provides: PluginHostImpl::with_storage for the worker run plugin loader
provides:
  - rollout-coordinator binary booting EmbeddedStorage + dev-CA TLS + 3 transport services + failure-scan loop
  - CoordinatorImpl persisting workers/* and heartbeats/* with implicit-first-heartbeat auto-registration
  - StdoutJsonEmitter NDJSON sink for the rollout-core EventEmitter trait (D-OBSERVE-01)
  - failure_scan_loop emitting deduped worker_failed tracing + Event per overdue worker
  - rollout-cli `coordinator run` + `worker run` subcommands (existing `schema` preserved)
  - Phase-2 worker runtime: storage + plugin loads + mTLS heartbeat loop + SIGTERM drain
  - docs/book/src/substrate/coordinator.md chapter
affects: [02-07-smoke-and-docs, phase-06-distribution, phase-04-training, phase-09-rl]

tech-stack:
  added: [clap-subcommand routing, postcard ledger encoding, tokio::sync::watch shutdown channel, tracing-subscriber JSON formatter, prost-types Timestamp]
  patterns:
    - "Arc<dyn EventEmitter> threaded through all coordinator state transitions (D-OBSERVE-01)"
    - "Implicit auto-register on first heartbeat — proto has no explicit register RPC"
    - "Library `pub async fn run` re-exported from rollout-coordinator so CLI and standalone binary share one boot path"
    - "HashSet-based failure dedup so the loop emits exactly one worker_failed per worker lifetime"
    - "Failure-scan tick at heartbeat_interval / 2 — detect missed beat within 2 × heartbeat_interval (SUBSTR-02)"

key-files:
  created:
    - crates/rollout-coordinator/src/config.rs
    - crates/rollout-coordinator/src/registry.rs
    - crates/rollout-coordinator/src/heartbeat.rs
    - crates/rollout-coordinator/src/failure_scan.rs
    - crates/rollout-coordinator/src/emitter.rs
    - crates/rollout-coordinator/src/run.rs
    - crates/rollout-coordinator/tests/registry_persistence.rs
    - crates/rollout-coordinator/tests/failure_scan.rs
    - crates/rollout-cli/src/worker.rs
    - crates/rollout-cli/tests/cli_help.rs
    - docs/book/src/substrate/coordinator.md
  modified:
    - crates/rollout-coordinator/Cargo.toml
    - crates/rollout-coordinator/src/lib.rs
    - crates/rollout-coordinator/src/main.rs
    - crates/rollout-cli/Cargo.toml
    - crates/rollout-cli/src/main.rs
    - docs/book/src/SUMMARY.md

key-decisions:
  - "Registration is implicit-via-first-heartbeat — the heartbeat handler upserts workers/<id> on first sight; no separate register RPC on the gRPC service (CONTEXT D-COORD-02 Step 4)."
  - "Failure-scan ticks at heartbeat_interval / 2 (250 ms by default) so a single missed beat is detected within 2 × heartbeat_interval — the SUBSTR-02 acceptance criterion #3."
  - "rollout-cli's `coordinator run` delegates to `rollout_coordinator::run::run` (re-exported from the library) rather than duplicating boot logic — single source of truth."
  - "Failure-scan loop dedups worker_failed events via an in-memory HashSet<String>; only the FIRST scan tick that observes a worker overdue emits the event."
  - "StdoutJsonEmitter wraps tokio::io::Stdout in a tokio::sync::Mutex<Stdout> + flushes after every line; line-level atomicity is the spec-09 wire contract."
  - "Plan snippet shipped `CoordinatorConfig::storage` and `transport` as required fields; the SUMMARY relaxes them to `#[serde(default)]` so smoke-test fixtures can ship a minimal TOML."
  - "Worker runtime uses `Beat(state=Init)` for the initial registration handshake and flips to `Ready` after the first successful beat — keeps the proto state machine spec-05-compliant."

patterns-established:
  - "Test emitter pattern: a CaptureEmitter impls EventEmitter into a Mutex<Vec<String>> for assertion in failure_scan tests — avoids tracing-test as a dev-dep"
  - "Boot-path skeleton (Storage open → TLS bootstrap → emitter → coord → 3 services → scan loop → SIGTERM handler → serve) is the reusable template for Phase 6's HA coordinator"
  - "Worker CLI: clap-subcommands route into a single async fn that handles config → plugins → channel → loop → drain — pattern reusable for `rollout infer batch` and other Phase 3+ binaries"

requirements-completed: [SUBSTR-02, DOCS-01, DOCS-02, DOCS-03]

duration: 24min
completed: 2026-05-20
---

# Phase 2 Plan 06: rollout-coordinator Summary

**Minimal Phase-2 control plane: register-via-implicit-heartbeat + deadline-based failure scan, persisted to redb under `workers/*` and `heartbeats/*`, with a `StdoutJsonEmitter` NDJSON sink for spec-09 events and `rollout {coordinator,worker} run` clap subcommands wiring everything end-to-end.**

## Performance

- **Duration:** 24 min
- **Started:** 2026-05-20T17:38:52Z
- **Completed:** 2026-05-20T17:55:14Z
- **Tasks:** 2 (both `type="auto" tdd="true"`)
- **Files modified:** 17 (11 created, 6 modified)

## Accomplishments

- `rollout-coordinator` ships as a real binary that opens `EmbeddedStorage`, bootstraps the mTLS dev CA, wires `HeartbeatServiceImpl` + `ControlServiceImpl` + `WorkServiceImpl` through `rollout_transport::server::serve`, and spawns a deadline-based failure-scan loop ticking at `heartbeat_interval / 2`.
- `CoordinatorImpl` impls `rollout_core::Coordinator` with `Arc<dyn EventEmitter>` threaded through every state transition — emits structured `Event { kind: Domain { topic: "worker_registered" | "worker_heartbeat" | "worker_deregistered" | "worker_failed" } }` alongside the existing `tracing::*!` lines. Implicit-first-heartbeat auto-registration handled in `heartbeat()` so the proto stays single-RPC (no separate `register` over the wire).
- `StdoutJsonEmitter` (D-OBSERVE-01) ships as the Phase-2 sink — `tokio::sync::Mutex<Stdout>` + `serde_json::to_vec` + `\n` + `flush` per event. `NoopEmitter` ships alongside as the test/Phase-2-default discard sink.
- `rollout-cli` gains `coordinator run --config <path>` and `worker run --config <path> [--worker-id <ulid>] [--plugin <m> ...] [--hot-reload]` subcommands; the existing `rollout schema --format json|pretty` carryover from Phase 1 is preserved verbatim. The CLI's `coord_run` delegates to the library's `pub async fn run` so the boot path stays single-source.
- Phase-2 worker runtime: `WorkerConfig` TOML → `EmbeddedStorage` → `PluginHostImpl::with_storage` + load each `--plugin` manifest → mTLS channel via dev CA + per-worker client cert → heartbeat loop ticking at `heartbeat_interval` → SIGTERM handler that sends a final `Beat(Draining)` and exits 0.
- `docs/book/src/substrate/coordinator.md` (~140 lines) ships covering scope, storage layout, failure formula, observability, CLI, first-run UX — linked from `docs/book/src/SUMMARY.md`.

## Task Commits

1. **Task 1: Coordinator core + StdoutJsonEmitter + failure scan** — `4765381` (feat)
2. **Task 2: rollout-cli worker/coordinator subcommands + coordinator chapter** — `ad60b86` (feat)

Plan metadata commit follows this file.

## Files Created/Modified

### Created (11)

- `crates/rollout-coordinator/src/config.rs` — `CoordinatorConfig` (run_id + storage + transport) with `serde(default)` on the nested blocks and `validate()` delegating to `TransportConfig::validate_cross_fields`.
- `crates/rollout-coordinator/src/registry.rs` — `WorkerRegistryEntry`, `HeartbeatRecord`, `worker_key`, `heartbeat_key`, `now_ms`, `ms_to_systime`, `state_to_i32`.
- `crates/rollout-coordinator/src/heartbeat.rs` — `CoordinatorImpl::new(storage, run_id, emitter)` + `impl Coordinator` (register / deregister / heartbeat with implicit auto-register).
- `crates/rollout-coordinator/src/failure_scan.rs` — `failure_scan_loop(storage, emitter, interval, skew, coord_timeout, shutdown)` + deduped scan_once.
- `crates/rollout-coordinator/src/emitter.rs` — `NoopEmitter` + `StdoutJsonEmitter` (Mutex-wrapped Stdout).
- `crates/rollout-coordinator/src/run.rs` — `pub async fn run(cfg)` + `pub fn load_config(path)`.
- `crates/rollout-coordinator/tests/registry_persistence.rs` — 5 tests: register / heartbeat / deregister / overwrite / auto-register.
- `crates/rollout-coordinator/tests/failure_scan.rs` — 4 tests: marks late, ignores healthy, respects skew, dedup periodic.
- `crates/rollout-cli/src/worker.rs` — `WorkerConfig` + `pub async fn run` (the Phase-2 worker runtime).
- `crates/rollout-cli/tests/cli_help.rs` — 5 `assert_cmd` tests on subcommand `--help` exit codes + flag presence.
- `docs/book/src/substrate/coordinator.md` — substrate chapter.

### Modified (6)

- `crates/rollout-coordinator/Cargo.toml` — added rollout-{storage,transport,proto}, async-trait, serde + serde_json, tracing-subscriber, tokio (`io-std` feature for `tokio::io::stdout`), postcard, clap, ulid, toml, tonic, prost-types; dev: tempfile + tokio test-util.
- `crates/rollout-coordinator/src/lib.rs` — exports config/emitter/failure_scan/heartbeat/registry/run; re-exports CoordinatorConfig, CoordinatorImpl, NoopEmitter, StdoutJsonEmitter, run.
- `crates/rollout-coordinator/src/main.rs` — replaced Wave-0 stub with full clap entrypoint that calls `rollout_coordinator::run`.
- `crates/rollout-cli/Cargo.toml` — added rollout-{coordinator,transport,storage,plugin-host,proto}, tokio (signal+process), tracing-subscriber, toml, ulid, tonic, prost-types; dev: assert_cmd, predicates, tempfile.
- `crates/rollout-cli/src/main.rs` — added `Worker { sub }` and `Coordinator { sub }` enum variants alongside the existing `Schema { format }`; added `worker_run` + `coord_run` dispatch fns + `init_tracing` helper.
- `docs/book/src/SUMMARY.md` — added `Coordinator` link under `Substrate`.

## Decisions Made

The seven `key-decisions` in the frontmatter (above) are the canonical record. Most-load-bearing: **registration is implicit-via-first-heartbeat** (proto has no `register` RPC; `CoordinatorImpl::heartbeat` upserts `workers/<id>` on first sight) and **failure-scan ticks at `heartbeat_interval / 2`** so the SUBSTR-02 acceptance criterion (kill worker → failed within `2 × heartbeat_interval`) holds.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Enabled `io-std` Cargo feature on tokio for `rollout-coordinator`**
- **Found during:** Task 1 first build
- **Issue:** `StdoutJsonEmitter::default()` calls `tokio::io::stdout()` which is feature-gated behind `io-std`; the workspace's default tokio feature set does not include it.
- **Fix:** Added `features = ["io-std"]` to `[dependencies.tokio]` in `crates/rollout-coordinator/Cargo.toml`.
- **Files modified:** `crates/rollout-coordinator/Cargo.toml`
- **Verification:** `cargo build -p rollout-coordinator` succeeds
- **Committed in:** `4765381`

**2. [Rule 1 - Bug] `CoordinatorConfig` nested fields now `serde(default)`**
- **Found during:** Task 1 config design
- **Issue:** Plan snippet declared `storage` + `transport` as required fields. Phase-2 smoke fixtures need to ship a minimal TOML with `run_id = "..."` only — both inner structs already carry defaults.
- **Fix:** Added `#[serde(default)]` to both fields. `EmbeddedStorageConfig::default()` exists; `TransportConfig::default()` already shipped in plan 02-04.
- **Files modified:** `crates/rollout-coordinator/src/config.rs`
- **Verification:** Registry/failure-scan tests still pass; plan 02-07 smoke fixtures can stay minimal
- **Committed in:** `4765381`

**3. [Rule 2 - Missing critical] Implicit-first-heartbeat auto-registration**
- **Found during:** Task 1 + Task 2 design (plan-time open question)
- **Issue:** Plan Step 4 (Task 2) flagged that the proto's `Heartbeat` service has no `register` RPC — the only way a worker shows up over the wire is via `Beat`. Without auto-registration, the smoke-test worker would never appear in `workers/*` and SUBSTR-02 #3 couldn't be verified.
- **Fix:** `CoordinatorImpl::heartbeat` looks up `workers/<id>` first; if absent, calls `self.register(...)` before persisting the heartbeat. Added `heartbeat_auto_registers_unknown_worker` test for coverage.
- **Files modified:** `crates/rollout-coordinator/src/heartbeat.rs`, `crates/rollout-coordinator/tests/registry_persistence.rs`
- **Verification:** `heartbeat_auto_registers_unknown_worker` passes
- **Committed in:** `4765381`

**4. [Rule 2 - Missing critical] Failure-scan dedup via HashSet**
- **Found during:** Task 1 (`failure_scan_loop_runs_periodically` test design)
- **Issue:** Without dedup, the loop emits one `worker_failed` event per tick per overdue worker (e.g. 4 events at 50 ms interval over 200 ms). The spec contract is one event per transition.
- **Fix:** `scan_once` carries an `&mut HashSet<String>` of already-emitted worker IDs; only the first overdue observation per worker emits.
- **Files modified:** `crates/rollout-coordinator/src/failure_scan.rs`
- **Verification:** `failure_scan_loop_runs_periodically` asserts `captured.len() == 1`
- **Committed in:** `4765381`

**5. [Rule 1 - Bug] Replaced `_tx` binding name in test**
- **Found during:** Task 1 clippy pass
- **Issue:** `clippy::used_underscore_binding` rejected `let (_tx, rx) = ...; ...; let _ = _tx.send(true);` — Rust idiom is to drop the `_` prefix when the binding is actually used.
- **Fix:** Renamed to `tx`.
- **Files modified:** `crates/rollout-coordinator/tests/failure_scan.rs`
- **Verification:** `cargo clippy -p rollout-coordinator --all-targets -- -D warnings` clean
- **Committed in:** `4765381`

**6. [Rule 1 - Bug] doc_markdown lints on `heartbeat_interval` + `PyO3`**
- **Found during:** Task 2 clippy pass
- **Issue:** `clippy::doc_markdown` flagged bare `heartbeat_interval` and `PyO3` in doc comments.
- **Fix:** Backticked both.
- **Files modified:** `crates/rollout-cli/src/worker.rs`, `crates/rollout-cli/src/main.rs`
- **Verification:** `cargo clippy -p rollout-cli --all-targets -- -D warnings` clean
- **Committed in:** `ad60b86`

**7. [Rule 1 - Bug] `items_after_statements` lint on inline `use rollout_core::PluginHost`**
- **Found during:** Task 2 clippy pass
- **Issue:** Plan sketch had `use rollout_core::PluginHost` inside the `for path in &plugin_paths` block; clippy rejects.
- **Fix:** Hoisted to file-level `use` statement.
- **Files modified:** `crates/rollout-cli/src/worker.rs`
- **Verification:** Clippy clean
- **Committed in:** `ad60b86`

---

**Total deviations:** 7 auto-fixed (1 blocking, 4 bug, 2 missing critical)
**Impact on plan:** All seven auto-fixes were required for the plan to compile + lint + pass the SUBSTR-02 acceptance behavior. No scope creep — every fix lands within the plan's named files.

## Issues Encountered

None — the plan was sufficient detail for two clean TDD commits. The Wave 4 plugin-host work + Wave 3 transport work + Wave 2 storage work all landed Phase 2's invariants cleanly enough that the coordinator was glue + persistence + a scan loop, exactly as the planner scoped.

## User Setup Required

None — the dev CA auto-generates on first boot (`./data/tls/ca.pem`), redb opens its file on demand, and `RUST_LOG` defaults to `info`. Production deployments will need a real CA + supervised process manager + cert rotation, but those are Phase 12 hardening concerns.

## Verification Evidence

```
cargo build -p rollout-coordinator -p rollout-cli           # OK
cargo test  -p rollout-coordinator --tests                  # 9 passed
cargo test  -p rollout-cli         --tests                  # 5 passed
cargo test  --workspace            --tests                  # 102 passed total (+ ignored gates from plans 02-02 / 02-04 / 02-05 carried over)
cargo clippy -p rollout-coordinator -p rollout-cli --all-targets -- -D warnings   # clean
cargo clippy --workspace --all-targets -- -D warnings       # clean
cargo deny check                                            # advisories ok, bans ok, licenses ok, sources ok
mdbook build docs/book                                      # success
cargo run --bin rollout-coordinator -- run --help           # exit 0, --config flag described
cargo run -p rollout-cli -- coordinator run --help          # exit 0
cargo run -p rollout-cli -- worker run --help               # exit 0, --plugin + --hot-reload flags described
cargo run -p rollout-cli -- schema --format json | head -c 80   # JSON Schema still emitted (Phase-1 preservation)
```

## Open Questions Carried to Plan 02-07

1. **Exact fixture shape for `tests/smoke/coordinator.toml` and `tests/smoke/worker.toml`.** The minimal coordinator TOML is `run_id = "<ULID>"`; the worker needs `run_id` + `coordinator_addr` + `coordinator_domain`. Plan 02-07 owns the fixture authoring + the `make smoke` driver wiring.
2. **Whether the smoke driver wires the in-tree cdylib + Python sidecar samples via two `--plugin` flags on a single worker, or splits them across w1 + w2.** Plan 02-07 decides per the kill-w1 acceptance step.
3. **NDJSON capture for the SUBSTR-02 acceptance.** This plan ships `StdoutJsonEmitter` writing to the binary's stdout; the smoke driver in 02-07 needs to capture that stream and grep for `"topic":"worker_failed"` to verify the deadline detection.

## Self-Check: PASSED

- All 11 created files exist on disk (verified via `Read`/build).
- Both task commits in `git log`: `4765381`, `ad60b86`.
- All 14 target acceptance criteria from the plan's success_criteria validated (build / test / clippy / mdbook / --help / NDJSON wired).

## Next Phase Readiness

- **Plan 02-07 (smoke + docs + CI) unblocked.** The binary boots, the CLI subcommands route, and the failure-scan loop emits the events that the smoke `kill -KILL <w1>` test asserts on. Plan 02-07 only needs to author the TOML fixtures, the `scripts/smoke.sh` driver, and the `smoke` CI job.
- **Phase 6 distribution scaffolding ready.** The boot-path skeleton (Storage → TLS → emitter → coord → services → scan loop → SIGTERM → serve) is the reusable template for DIST-01..05's HA coordinator. Storage namespaces `workers` + `heartbeats` will be extended (not replaced) when lease/CAS lands.

---
*Phase: 02-local-substrate*
*Completed: 2026-05-20*
