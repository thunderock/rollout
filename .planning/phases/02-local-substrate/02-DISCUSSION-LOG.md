# Phase 2: Local substrate — Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in `02-CONTEXT.md` — this log preserves the alternatives considered.

**Date:** 2026-05-19
**Phase:** 02-local-substrate
**Areas discussed (round 1):** Storage backend; Plugin host scope; Transport + smoke-test slice; cloud-local + heartbeat constants
**Areas discussed (round 2):** Wave breakdown; Observability; .proto file ownership; Plugin sandboxing depth

---

## Storage backend

### Q1 — Embedded KV choice

| Option | Description | Selected |
|---|---|---|
| Pick redb now | Spec already calls it preferred; single-file MVCC, copy-on-write, no compaction stalls; actively maintained. Skip the benchmark. | ✓ |
| Pick sled now | More mature/battle-tested in OSS; risk of compaction surprises in long runs; 1.0 stability flagged by maintainer. | |
| Benchmark both, pick on data | Write throughput, read latency, recovery after kill -9, on-disk size, watch() implementability. Decide at end of Wave 1. | |
| Claude's discretion | Likely redb after short investigation; open to flipping on blocker. | |

**User's choice:** Pick redb now
**Notes:** Aligns with spec 04 §3.1 stated preference; saves the benchmark cost.

### Q2 — `watch()` semantics on the embedded backend

| Option | Description | Selected |
|---|---|---|
| In-process broadcast channel | `tokio::sync::broadcast` per prefix-tree; commit hooks fan out events. Works because Phase 2 is single-process. Documents the limitation. | ✓ |
| Background polling | Spawn a polling task that diffs against a cached snapshot. Cross-process capable; coarser latency, more CPU. | |
| Return NotSupported for embedded; Postgres-only watch | Phase 2 coordinator wires the heartbeat receiver directly into an in-process subscriber list. | |
| Claude's discretion | Pick during implementation based on what the heartbeat receiver actually needs. | |

**User's choice:** In-process broadcast channel
**Notes:** Documented in CONTEXT.md as in-process-only; cross-process watch arrives with Postgres in Phase 4.

### Q3 — Durability + on-disk layout

| Option | Description | Selected |
|---|---|---|
| Always fsync, path from config | fsync on every commit; default `./data/rollout.db`; overridable. Crash-consistent; higher write latency (acceptable, perf bar is GPU). | ✓ |
| Group-commit batching | Configurable `fsync_mode = 'always' \| 'batched { ... }' \| 'never'`. Trades durability window for throughput. | |
| OS-buffered, fsync on close + on snapshot | Fastest writes, weakest durability. Suitable if snapshots cover the gap. | |
| Claude's discretion | Default to always-fsync but expose the knob. | |

**User's choice:** Always fsync, path from config
**Notes:** Config key: `[storage.embedded] path = ...`. Heavy tests can dial down later via the config surface if needed.

### Q4 — Value encoding

| Option | Description | Selected |
|---|---|---|
| postcard | Compact binary serde; schemafull, deterministic; serde-native; forward-compatible. | ✓ |
| serde_json | Human-readable on disk; trivial compatibility with future Postgres `jsonb`. Larger, slower. | |
| bincode 2.x | Fastest binary; brittle to type changes; would need version tags per stored type. | |
| Claude's discretion | Pick based on hot-path stress. | |

**User's choice:** postcard

---

## Plugin host

### Q1 — Loading modes shipped in Phase 2

| Option | Description | Selected |
|---|---|---|
| All three modes (cdylib + PyO3 + sidecar) | Aligns with spec 03; gives Phase 3 (vLLM) a proven path. Per-mode loaders are small. Samples: 1 cdylib + 1 Python sidecar. | ✓ |
| Two modes: PyO3 + sidecar | Matches ROADMAP Includes wording; defers cdylib to Phase 3. | |
| Two modes: PyO3 + cdylib | Matches exit-criterion wording; defers sidecar to Phase 7. Loses hot-reload + crash-isolation now. | |
| Claude's discretion | Investigate cost and pick smallest set satisfying both ROADMAP + exit-criterion. | |

**User's choice:** All three modes
**Notes:** Phase 3 (vLLM via PyO3) and Phase 7 (tool harness via sidecar sandbox) both benefit from a working three-mode host.

### Q2 — PyO3 ↔ Tokio strategy

| Option | Description | Selected |
|---|---|---|
| pyo3-async-runtimes + dedicated Python OS thread | Per-worker Python thread owns the interpreter; Python coroutines bridge to Tokio via the `tokio` feature. | ✓ |
| spawn_blocking + sync-Python-only | Forbid async plugins; every call is `spawn_blocking(Python::with_gil(...))`. | |
| Sub-interpreter per call (PEP 684) | Bleeding edge; PyO3 sub-interpreter support partial. | |
| Claude's discretion | Pick during research. | |

**User's choice:** pyo3-async-runtimes + dedicated Python OS thread per worker
**Notes:** Pin both `pyo3` and `pyo3-async-runtimes` versions early per ROADMAP risk callout.

### Q3 — Python sample install path

| Option | Description | Selected |
|---|---|---|
| Vendored under `python/examples/`, run via `python -m` | Pure-Python module; sidecar launches with whichever python3 is on PATH. Zero install step. | ✓ |
| Workspace `rollout-plugin-py` via `maturin develop` | Cleaner story for Phase 9 real plugins; adds an install step to dev/CI. | |
| Embed sample as string literal | Always in sync with test; loses readability. | |
| Claude's discretion | Pick after loader discovery code shapes up. | |

**User's choice:** `python/examples/sample_sidecar/` + `python/examples/sample_inproc/`, launched via `python -m`
**Notes:** Stdlib-only sample to honor AGENTS.md "every plugin testable locally without extra deps."

### Q4 — Hot-reload scope

| Option | Description | Selected |
|---|---|---|
| Full hot-reload for PyO3 + sidecar | Drain-in-flight, reload, resume. cdylib unsupported per spec. Honors SUBSTR-03 literally. | ✓ |
| Load + call + unload only; defer hot-reload | `reload()` returns Recoverable(Transient) "not yet implemented." | |
| Sidecar-only hot-reload (defer PyO3 reload) | Sidecar respawn is mechanically simpler; PyO3 importlib.reload has edge cases. | |
| Claude's discretion | Pick based on wave structure. | |

**User's choice:** Full hot-reload (PyO3 + sidecar)
**Notes:** Gated behind a `dev` feature or `--hot-reload` flag. cdylib reload returns a typed Fatal(PluginContract) "unsupported".

---

## Transport + smoke-test slice

### Q1 — gRPC-over-QUIC stack

| Option | Description | Selected |
|---|---|---|
| tonic over h3/quinn | Keeps spec-05 semantics + gRPC compatibility. Researcher MUST validate tonic-h3 bidi-streaming. | ✓ |
| Ship HTTP/2 tonic in Phase 2; swap to QUIC later | Same proto schema; QUIC swap = one config change in a later phase. | |
| Custom framing over quinn (no tonic) | Hand-rolled framer; smallest dep surface; more plumbing we own. | |
| Claude's discretion | Pick after researcher evaluates tonic-h3. | |

**User's choice:** tonic over h3/quinn
**Notes:** **Documented fallback**: if `tonic-h3` is not production-ready for the Work channel's bidi streaming, ship HTTP/2 tonic in Phase 2 with identical proto. Researcher validates maturity before planner commits.

### Q2 — TLS posture

| Option | Description | Selected |
|---|---|---|
| mTLS by default, dev CA auto-generated | Per-run self-signed CA + cert/key under `./data/tls/`; matches spec 05 "TLS by default; mTLS in production." | ✓ |
| Plaintext over UDS (loopback), TLS only for non-local | Faster local dev; TLS code less exercised. | |
| Plaintext TCP by default for Phase 2 | Skip TLS entirely until Phase 6. | |
| Claude's discretion | Pick based on rustls/tonic plumbing cost. | |

**User's choice:** mTLS by default with auto-generated dev CA
**Notes:** First-run UX: CLI prints "Generated dev CA at ./data/tls/ca.pem" and proceeds. `data/tls/` gitignored.

### Q3 — Does Phase 2 ship a coordinator binary?

| Option | Description | Selected |
|---|---|---|
| Minimal `rollout-coordinator` crate | Registers workers, accepts heartbeats, persists to Storage, deadline-based failure scan. No scheduling/lease/work-stealing. | ✓ |
| Heartbeat receiver baked into `rollout-cli` test binary | Phase 6 introduces `rollout-coordinator` as a real crate; slight rework. | |
| Reinterpret exit criterion: two workers, no coordinator | Worker↔worker heartbeats; cheats spec 05 §3. | |
| Claude's discretion | Pick after planner sizes the coordinator crate. | |

**User's choice:** Minimal `rollout-coordinator` crate
**Notes:** Phase 2 ships 6 crates total (proto + storage + transport + plugin-host + cloud-local + coordinator). Lease/CAS/scheduling all stay Phase 6.

### Q4 — Smoke-test shape

| Option | Description | Selected |
|---|---|---|
| CLI script: 1 coord + 2 workers, load plugins, kill w1, assert detection | `make smoke` + `scripts/smoke.sh` + `tests/smoke/*.toml` configs. | ✓ |
| Rust integration test under `crates/rollout-cli/tests/smoke.rs` | Pure `#[tokio::test]` spawning subprocesses; runs on `cargo test --workspace`. | |
| Both: cargo-test for CI + bash script for humans | Slight duplication; clearest UX. | |
| Claude's discretion | Pick based on wave structure. | |

**User's choice:** CLI script (`make smoke` + `scripts/smoke.sh`)
**Notes:** Wired into the CI `test` job. Returns non-zero on assertion failure.

---

## cloud-local + heartbeat constants

### Q1 — Heartbeat / timeout default constants

| Option | Description | Selected |
|---|---|---|
| Conservative: 500ms / 4s / 5s / 250ms | Detection in ≤1s; self-fence (4s) < coord-failure (5s); skew margin > one heartbeat. | ✓ |
| Aggressive: 200ms / 1.5s / 2s / 100ms | Faster detection; may flap on loaded CI runners. | |
| Loose: 2s / 15s / 20s / 1s | Quiet wire, slower detection. Safer for noisy clouds. | |
| Claude's discretion | Pick during implementation. | |

**User's choice:** Conservative defaults
**Notes:** Invariants `self_fence < coord_failure` and `skew < 2× heartbeat` enforced at config-validate time.

### Q2 — In-mem queue durability across restart

| Option | Description | Selected |
|---|---|---|
| Spill to Storage; recover on restart | Hot path in RAM; durability rides on Storage (redb). Matches spirit of DIST-03. | ✓ |
| Pure RAM, lose on restart | Literal reading of spec 06; cheapest. | |
| Spill to FS (jsonl append-only) | No `rollout-storage` dep; cleaner boundary; higher fsync cost. | |
| Claude's discretion | Pick after coordinator restart story shapes up. | |

**User's choice:** Spill to Storage under `cloudlocal/queue/<id>`
**Notes:** Postcard-encoded; restart replays unacknowledged items.

### Q3 — ComputeHint platform support

| Option | Description | Selected |
|---|---|---|
| Linux full + macOS minimal stub | Linux reads /proc + nvml; macOS uses `sysinfo`. Honors AGENTS.md local-test rule on Mac dev boxes. | ✓ |
| Linux-only; macOS returns Recoverable(Transient) | Compiles on macOS but every call errors; forces Linux for local runs. | |
| Full Linux + macOS + Windows | Symmetric stubs; rollout isn't a Windows target — value marginal. | |
| Claude's discretion | Pick based on local-test rule. | |

**User's choice:** Linux full + macOS minimal stub via `sysinfo`
**Notes:** `nvml-wrapper` feature-gated; `#[cfg(target_os = "linux")]` gates Linux-only integration tests.

### Q4 — fs object store + SecretStore + BlockStore bundle

| Option | Description | Selected |
|---|---|---|
| Content-addressed FS object store; env-var SecretStore; skip BlockStore | Blobs under `./data/object-store/<sha[0..2]>/<sha[2..4]>/<sha>` with `.meta`; env-var allowlist; BlockStore optional trait left unimplemented. | ✓ |
| Flat FS object store; env-var SecretStore; minimal BlockStore | Same SecretStore; flat blob layout; minimal BlockStore returning fs path. | |
| Run-scoped object store; env-vars + ~/.rollout/secrets.toml fallback; skip BlockStore | Per-run deletion; SecretStore TOML fallback. | |
| Claude's discretion | Pick based on what later phases inherit. | |

**User's choice:** Content-addressed sharded FS + env-var allowlist SecretStore + skip BlockStore
**Notes:** SecretStore.put() returns Fatal(ConfigInvalid) — read-only by design.

---

## Wave breakdown / sequencing

### Q1 — Wave structure for 5 (later 6) crates

| Option | Description | Selected |
|---|---|---|
| 4 waves, mostly parallel | W1: storage + cloud-local (later: + proto); W2: transport; W3: plugin-host + coordinator; W4: smoke + docs + CI. | ✓ |
| Sequential, one crate per wave | 5+ waves; easiest deviation handling. | |
| Big-bang: trait skeletons W1, impls W2, smoke W3 | Risky: 5 parallel streams in W2 modifying shared assumptions. | |
| Claude's discretion (planner decides) | Let RESEARCH.md drive wave shape. | |

**User's choice:** 4 waves, mostly parallel
**Notes:** After the `rollout-proto` crate was added in the .proto-ownership question, Wave 1 expands to 3 parallel streams: proto + storage + cloud-local.

---

## Observability

### Q1 — Spec 09 coverage in Phase 2

| Option | Description | Selected |
|---|---|---|
| Tracing skeleton + structured events on critical paths | Workspace tracing; spans on public async fns; EventEmitter with stdout JSON sink. | ✓ |
| Minimal: `tracing` only | No EventEmitter impl until later phase. | |
| Full spec 09 (EventEmitter + sinks + smoke-test assertions) | Most upfront work; highest fidelity. | |
| Claude's discretion | Pick based on planner capacity. | |

**User's choice:** Tracing skeleton + structured events on critical paths
**Notes:** Critical events: worker_registered, worker_heartbeat, worker_failed, plugin_loaded, plugin_reloaded, plugin_call, plugin_call_error. RunId+WorkerId propagate as span fields. EventEmitter trait implemented; sink = stdout JSON in Phase 2.

---

## .proto file ownership

### Q1 — Where do .proto files live?

| Option | Description | Selected |
|---|---|---|
| Dedicated `rollout-proto` crate | Owns transport.proto + plugin.proto; build.rs runs tonic-build. transport, plugin-host, and external sidecar plugins all depend on it. | ✓ |
| Inline in rollout-transport + duplicate for sidecar | Two build.rs setups; risk of drift. | |
| Inline in rollout-transport; plugin-host depends on transport | Wrong layer; sidecar runs over UDS not QUIC. | |
| Claude's discretion | Pick during research. | |

**User's choice:** Dedicated `rollout-proto` crate
**Notes:** Python stubs generated via `make protos` (one-shot, committed). Crate count grows to 6 for Phase 2.

---

## Plugin sandboxing depth

### Q1 — Spec 03 §10 enforcement scope

| Option | Description | Selected |
|---|---|---|
| Network allowlist only; cgroups + seccomp deferred to Phase 7 | Host-side egress proxy enforces `[network]` block (default-deny). Real isolation deferred to when adversarial harnesses arrive. | ✓ |
| Full enforcement (cgroups + seccomp + network + fs) | Significant Linux code (cgroupfs + libseccomp); high value for Phase 7. | |
| Stub everything; declare PluginSandbox trait + no-op impl | Smallest scope; least security. | |
| Claude's discretion | Pick based on plugin-host crate size. | |

**User's choice:** Network allowlist only; defer cgroups + seccomp + FD limits + fs write restrictions to Phase 7
**Notes:** TODOs with tracking comments reference Phase 7 (HARNESS-02 brings untrusted code-exec).

---

## Claude's Discretion (areas the user explicitly punted)

- Specific `pyo3` / `pyo3-async-runtimes` version pins (research picks the known-good pair).
- Manifest format details — TOML keys, plugin discovery search-paths precedence (spec 03 §8).
- Minimum supported Python version (recommendation: 3.11+ for stdlib `tomllib` + PyO3 abi3 stability).
- PyO3 abi3 strategy.
- cdylib plugin-abi shim crate naming (likely `rollout-plugin-abi`).
- `nvml-wrapper` vs hand-rolled FFI for GPU inventory.
- Internal redb table layout (table-per-namespace vs single kv table).
- Specific `tonic-h3` (or successor) crate choice.
- mdBook section structure for substrate docs.

## Deferred Ideas

(Captured in `02-CONTEXT.md` `<deferred>` section — every deferred capability is mapped to a downstream phase.)
