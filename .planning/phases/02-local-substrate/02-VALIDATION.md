---
phase: 02
slug: local-substrate
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-05-19
---

# Phase 02 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Source-of-truth: `02-RESEARCH.md` §"Validation Architecture".

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` (Rust unit + integration) + `bash` (smoke test) + `pytest`/stdlib (Python samples) |
| **Config file** | Workspace `Cargo.toml`; per-crate `tests/`; `scripts/smoke.sh`; `python/examples/sample_*/test_*.py` (optional) |
| **Quick run command** | `cargo test --workspace --tests` (already in `make test`) |
| **Full suite command** | `make check && make smoke && make docs` |
| **Smoke command** | `make smoke` → `scripts/smoke.sh` |
| **Estimated runtime** | quick ~20–60 s · full + smoke ~3–5 min on CI |

---

## Sampling Rate

- **After every task commit:** `cargo test -p <touched-crate> --tests` (≤30 s typical)
- **After every plan wave:** `make check` (lint + workspace test); `make smoke` for waves W3 + W4
- **Before `/gsd:verify-work`:** `make check && make smoke && make docs` all green
- **Max feedback latency:** ~60 s per task, ~5 min per wave

---

## Per-Task Verification Map

| Req | Behavior | Test Type | Automated Command | File Exists | Wave |
|---|---|---|---|---|---|
| **SUBSTR-01** | Storage `put`/`get` round-trip | unit | `cargo test -p rollout-storage --test crud` | ❌ W0/W1 | W1 |
| SUBSTR-01 | Storage transaction commit / abort | unit | `cargo test -p rollout-storage --test txn` | ❌ W1 | W1 |
| SUBSTR-01 | Storage `watch()` broadcast fan-out | integration | `cargo test -p rollout-storage --test watch` | ❌ W1 | W1 |
| SUBSTR-01 | fsync durability (SIGKILL mid-write) | integration | `cargo test -p rollout-storage --test crash_safety -- --ignored` (CI Linux only) | ❌ W1 | W1 |
| SUBSTR-01 | redb table-per-namespace open-many | unit | `cargo test -p rollout-storage --test tables` | ❌ W1 | W1 |
| **SUBSTR-02** | rcgen dev CA + mTLS handshake | integration | `cargo test -p rollout-transport --test tls_dev_ca` | ❌ W2 | W2 |
| SUBSTR-02 | Heartbeat unary round-trip | integration | `cargo test -p rollout-transport --test heartbeat` | ❌ W2 | W2 |
| SUBSTR-02 | Control server-stream subscribe | integration | `cargo test -p rollout-transport --test control_stream` | ❌ W2 | W2 |
| SUBSTR-02 | Plan-time invariants (`self_fence < coord_failure`; `skew < 2× hb`) | unit | `cargo test -p rollout-transport --test config_invariants` | ❌ W2 | W2 |
| SUBSTR-02 | Deadline detection: kill worker → coord marks failed within 2× hb_interval | integration (smoke) | `make smoke` | ❌ W4 | W4 |
| **SUBSTR-03** | Manifest TOML parse + validate | unit | `cargo test -p rollout-plugin-host --test manifest` | ❌ W3 | W3 |
| SUBSTR-03 | Load cdylib + call + unload | integration | `cargo test -p rollout-plugin-host --test cdylib_load -- --ignored` | ❌ W3 | W3 |
| SUBSTR-03 | Load PyO3 in-process + call | integration | `cargo test -p rollout-plugin-host --test pyo3_load` | ❌ W3 | W3 |
| SUBSTR-03 | Spawn sidecar + call + shutdown | integration | `cargo test -p rollout-plugin-host --test sidecar_load` | ❌ W3 | W3 |
| SUBSTR-03 | Hot-reload PyO3 (dev feature) | integration | `cargo test -p rollout-plugin-host --features dev-hot-reload --test reload_pyo3` | ❌ W3 | W3 |
| SUBSTR-03 | Hot-reload sidecar (SIGTERM + respawn) | integration | `cargo test -p rollout-plugin-host --features dev-hot-reload --test reload_sidecar` | ❌ W3 | W3 |
| SUBSTR-03 | Cdylib reload returns `Fatal(PluginContract)` | unit | `cargo test -p rollout-plugin-host --test reload_cdylib_unsupported` | ❌ W3 | W3 |
| SUBSTR-03 | Smoke: load 1 cdylib + 1 Python sidecar per worker | integration (smoke) | `make smoke` | ❌ W4 | W4 |
| **SUBSTR-04** | FS object store sharded-layout put / get | unit | `cargo test -p rollout-cloud-local --test object_store` | ❌ W1 | W1 |
| SUBSTR-04 | In-mem queue with Storage spill + restart replay | integration | `cargo test -p rollout-cloud-local --test queue_replay` | ❌ W1 | W1 |
| SUBSTR-04 | SecretStore env-var allowlist (read) | unit | `cargo test -p rollout-cloud-local --test secrets` | ❌ W1 | W1 |
| SUBSTR-04 | SecretStore `put()` returns `Fatal(ConfigInvalid)` | unit | `cargo test -p rollout-cloud-local --test secrets` | ❌ W1 | W1 |
| SUBSTR-04 | ComputeHint Linux `/proc` parsing | integration `#[cfg(linux)]` | `cargo test -p rollout-cloud-local --test hints_linux` | ❌ W1 | W1 |
| SUBSTR-04 | ComputeHint macOS sysinfo stub | integration `#[cfg(macos)]` | `cargo test -p rollout-cloud-local --test hints_macos` | ❌ W1 | W1 |
| **DOCS-01..03** | Substrate mdBook chapters + crate-level `//!` docs | CI | `cargo doc --workspace --no-deps --all-features` + `mdbook build docs/book` | ✓ existing | W4 |
| DOCS-02 | Every commit touches docs/tests | CI | `scripts/check-docs-tests-touched.sh` | ✓ existing | every wave |
| **Cross-crate** | Plugin host persists manifest via Storage | integration | `cargo test -p rollout-plugin-host --test storage_integration` | ❌ W3 | W3 |
| Cross-crate | Coordinator persists worker registry via Storage | integration | `cargo test -p rollout-coordinator --test registry_persistence` | ❌ W3 | W3 |
| Cross-crate | Worker → transport → coordinator heartbeat flow | integration (smoke) | `make smoke` | ❌ W4 | W4 |
| **Architecture** | Dep-direction lint covers new crates | CI | `cargo test -p rollout-core --test dependency_direction` (extended) | ✓ existing + W0 ext | W0 |
| Architecture | `rollout-transport` ↛ `rollout-cloud-*` | unit | extension to `dependency_direction.rs` | ❌ W0 | W0 |
| Architecture | `rollout-plugin-host` ↛ `rollout-transport` | unit | extension to `dependency_direction.rs` | ❌ W0 | W0 |
| **Schema** | New `[storage]` / `[transport]` / `[plugins]` / `[cloud.local]` config blocks regenerate cleanly | CI | `cargo xtask schema-gen && git diff --exit-code` | ✓ existing + fixtures | W0/W1 |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

**Wave 0 = before any Wave 1 stream begins. These gaps block downstream waves if missing.**

- [ ] **`crates/rollout-core/src/traits/storage.rs`** — extend `Storage` with `get` / `get_many` / `scan` / `watch`; extend `StorageTxn` with `put` / `delete` / `cas` / `abort`. Add `StorageKey { namespace, run_id, path }`, `KeyRange`, `StorageEvent`. Covers SUBSTR-01.
- [ ] **`crates/rollout-core/src/traits/plugin.rs`** — extend `PluginHost` with `call<Req,Res>` / `reload` / `unload`; add `PluginHandle`, `PluginManifest`, `PluginDependencies`. Covers SUBSTR-03.
- [ ] **`crates/rollout-core/src/traits/worker.rs`** — extend `Coordinator` with `heartbeat(Heartbeat)`; extend `Worker` with `init` / `ready` lifecycle hooks; add `Heartbeat`, `WorkerState`. Covers SUBSTR-02.
- [ ] **`crates/rollout-core/src/traits/cloud.rs`** — verify/extend `ObjectStore` / `Queue` / `SecretStore` / `ComputeHint` to spec 06 §3. Covers SUBSTR-04.
- [ ] **`crates/rollout-core/src/config/`** — add `StorageConfig`, `TransportConfig`, `PluginsConfig`, `CloudLocalConfig` modules with `JsonSchema` derives. Wire into `RunConfig`.
- [ ] **`docs/specs/01,03,04,06`** — update specs in the same PR if any extension differs from current text (AGENTS.md §4).
- [ ] **`crates/rollout-core/tests/dependency_direction.rs`** — add fixtures for new invariants (`rollout-transport` ↛ `rollout-cloud-*`; `rollout-plugin-host` ↛ `rollout-transport`).
- [ ] **Workspace `Cargo.toml`** — register six new crates: `rollout-proto`, `rollout-storage`, `rollout-cloud-local`, `rollout-transport`, `rollout-plugin-host`, `rollout-coordinator`.
- [ ] **Framework install (`scripts/preflight.sh`):** confirm `protoc` (or rely on tonic-build's vendored protoc). Verify with `protoc --version`.

*Critical Finding from RESEARCH.md §"Critical Finding: Trait Surface Drift": the existing `rollout-core` traits are Phase-1 stubs. CONTEXT.md said "trait definitions are not modified in Phase 2" — research disagrees. Wave 0 closes the gap.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|---|---|---|---|
| First-run UX: CLI prints "Generated dev CA at ./data/tls/ca.pem" | D-TRANS-02 | Side effect of TTY output formatting; not load-bearing for correctness | Run `rm -rf data/ && cargo run -p rollout-cli -- coordinator run --config tests/smoke/coordinator.toml &`; observe stderr/stdout contains the line; cleanup. |
| GPU inventory on a NVIDIA host | D-LOCAL-04 | CI runners do not have GPUs | Run `cargo test -p rollout-cloud-local --features nvml --test hints_linux_gpu -- --ignored` on a host with NVML installed. |
| Hot-reload UX: SIGTERM + respawn observable in logs | D-PLUGIN-04 | Best validated by watching live logs | Run `cargo run -p rollout-cli -- worker run --hot-reload --config ...`; modify Python sidecar source; confirm reload event in stderr. |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references (trait extensions, dep-direction fixtures, six crate registrations, preflight)
- [ ] No watch-mode flags
- [ ] Feedback latency < 60 s per task / 5 min per wave
- [ ] `nyquist_compliant: true` set in frontmatter (after planner wires every task to an automated command above)

**Approval:** pending
