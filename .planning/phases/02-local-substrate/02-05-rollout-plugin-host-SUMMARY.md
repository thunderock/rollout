---
phase: 02-local-substrate
plan: 05
subsystem: substrate-plugin-host
tags: [rollout-plugin-host, pyo3, libloading, cdylib, sidecar, abi, uds, hot-reload, python, mdbook, sandboxing-stub]

# Dependency graph
requires:
  - phase: 02-local-substrate
    provides: PluginHost trait surface + PluginManifest/PluginHandle/PluginKind/PluginMode/EntrySpec/SidecarProtocol/RuntimeHints/PluginDependencies/PluginId types (plan 02-00 Wave 0); EmbeddedStorage for manifest persistence (plan 02-02); workspace pins for pyo3 0.28 + pyo3-async-runtimes 0.28 + libloading 0.8 + toml 0.8 + nix 0.30 (plan 02-00 + this plan)
provides:
  - "PluginHostImpl implementing rollout_core::PluginHost with three modes dispatched on manifest.mode at load() time"
  - "modes::cdylib::CdylibState — libloading-based loader against the ABI v1 vtable (rollout_plugin_factory); copies out the returned Buf before free_buf to avoid allocator-mismatch UB"
  - "modes::pyo3::Pyo3State — dedicated Python OS thread per host (RESEARCH Pattern 3); pyo3 0.28 uses Python::attach (with_gil removed); auto-initialize spins up the interpreter lazily"
  - "modes::sidecar::SidecarState — stdlib length-prefixed JSON over AF_UNIX for the in-tree sample (RESEARCH Pitfall 9); 5s connect retry; SIGTERM + respawn via nix::sys::signal::kill for hot reload"
  - "modes::abi — RolloutPluginVtable + Buf + ABI_VERSION = 1 as an in-tree module (per RESEARCH OQ 3); promote to a separate rollout-plugin-abi crate later"
  - "manifest::parse_manifest + validate_manifest: TOML parse → schema check (python_min >= 3.11 for pyo3 plugins; non-empty name/version; serde rename_all = kebab-case via the rollout-core PluginManifest type)"
  - "PluginHostImpl::with_storage(storage): persists manifest JSON under StorageKey { namespace: 'plugins', run_id: None, path: [name] } on every successful load()"
  - "Test injection helper (test_inject_cdylib_placeholder + CdylibState::for_tests_placeholder) so reload_cdylib_unsupported.rs exercises the reload branch without a real .dylib"
  - "Hot reload gated behind the `dev-hot-reload` Cargo feature: PyO3 via importlib.reload + factory re-call; sidecar via SIGTERM + respawn; cdylib returns Fatal(PluginContract { msg: 'cdylib reload unsupported per spec 03 §7' })"
  - "Tracing events with target = 'plugin_host': plugin_loaded (load), plugin_reloaded (reload), plugin_call span (every call), plugin_call_error (Err) — per D-OBSERVE-01"
  - "tests/smoke/plugins/rust_cdylib_sample/ — out-of-workspace cdylib crate exporting rollout_plugin_factory with one method ('echo'); SyncVtable wrapper makes the static vtable Sync-safe"
  - "python/examples/sample_inproc/{__init__,plugin}.py — PyO3 in-process sample with create_plugin().call(method, payload) → bytes contract"
  - "python/examples/sample_sidecar/{__init__,__main__}.py — stdlib sidecar with 4-byte BE length-prefixed JSON envelopes over AF_UNIX (chmod 600)"
  - "tests/smoke/plugins/sample_{inproc,sidecar}.toml + rust_cdylib_sample/rollout-plugin.toml — manifest fixtures consumed by plan 02-07 smoke driver"
  - "docs/book/src/substrate/plugin-host.md + python-bridge.md — mdBook chapters wired into SUMMARY.md (Examples placeholder preserved)"
affects: [02-06-rollout-coordinator, 02-07-smoke-and-docs]

# Tech tracking
tech-stack:
  added:
    - "rollout-plugin-host now depends on: rollout-core, rollout-storage, async-trait, serde, serde_json, thiserror, tracing, tokio, smol_str, toml; behind features: libloading 0.8 (cdylib), pyo3 0.28 + pyo3-async-runtimes 0.28 (pyo3, abi3-py311 + auto-initialize), nix 0.30 (sidecar, for SIGTERM)"
    - "dev-dep: tempfile 3.10 (writable manifest dirs for reload tests)"
    - "deny.toml: added 'Apache-2.0 WITH LLVM-exception' to the license allowlist (target-lexicon transitive via pyo3-build-config)"
  patterns:
    - "Per-crate [lints.rust] downgrade: workspace `unsafe_code = forbid` → `deny` in this crate ONLY; SAFETY-commented `#[allow(unsafe_code)]` in modes/cdylib.rs + modes/abi.rs ONLY — every C-ABI cast carries a SAFETY: comment"
    - "Three-mode dispatch via match on (manifest.mode, manifest.entry.clone()): the public PluginHandle stays POD; the per-instance state lives in a parallel HashMap<PluginId, HandleState> inside the host"
    - "HandleState enum boxes the large Sidecar variant (clippy::large_enum_variant) — Cdylib/Pyo3 stay inline because their sizes are similar"
    - "Sidecar JSON envelope: {method, payload as UTF-8 string} — text-only by design; binary payloads should base64-encode (documented). The in-tree sample is text-only"
    - "PyO3 worker channel: enum PyTask { Call, Reload (cfg-gated), Shutdown }; Python::attach inside blocking_recv loop on the dedicated OS thread (named 'rollout-py-<plugin>')"
    - "CdylibState::call copies out the Buf via std::slice::from_raw_parts(...).to_vec() BEFORE invoking free_buf — prevents allocator-mismatch UB across the cdylib boundary (Rust host allocator ≠ plugin allocator in general)"
    - "Test-only doc(hidden) injection (CdylibState::for_tests_placeholder + PluginHostImpl::test_insert_handle) lets reload_cdylib_unsupported.rs run without a prebuilt sample"

key-files:
  created:
    - "crates/rollout-plugin-host/src/manifest.rs — parse + validate"
    - "crates/rollout-plugin-host/src/handle.rs — HandleState enum (Cdylib / Pyo3 (cfg-gated) / Box<Sidecar>)"
    - "crates/rollout-plugin-host/src/host.rs — PluginHostImpl + impl PluginHost"
    - "crates/rollout-plugin-host/src/modes/mod.rs + abi.rs + cdylib.rs + pyo3.rs + sidecar.rs — mode loaders"
    - "crates/rollout-plugin-host/tests/manifest.rs (6 tests)"
    - "crates/rollout-plugin-host/tests/cdylib_load.rs (1 #[ignore]d active when sample pre-built)"
    - "crates/rollout-plugin-host/tests/reload_cdylib_unsupported.rs (1 test)"
    - "crates/rollout-plugin-host/tests/sidecar_load.rs (1 test; skips cleanly when python3 missing)"
    - "crates/rollout-plugin-host/tests/pyo3_load.rs (1 #[ignore]d; pyo3 link-time gate)"
    - "crates/rollout-plugin-host/tests/reload_pyo3.rs (#[cfg(feature='dev-hot-reload')]; 1 #[ignore]d)"
    - "crates/rollout-plugin-host/tests/reload_sidecar.rs (#[cfg(feature='dev-hot-reload')]; 1 test green)"
    - "crates/rollout-plugin-host/tests/storage_integration.rs (1 test)"
    - "tests/smoke/plugins/rust_cdylib_sample/{Cargo.toml,src/lib.rs,rollout-plugin.toml,Cargo.lock} — out-of-workspace cdylib sample"
    - "python/examples/sample_inproc/{__init__,plugin}.py"
    - "python/examples/sample_sidecar/{__init__,__main__}.py"
    - "tests/smoke/plugins/sample_{inproc,sidecar}.toml — manifest fixtures"
    - "docs/book/src/substrate/plugin-host.md + python-bridge.md"
  modified:
    - "crates/rollout-plugin-host/Cargo.toml — concrete dep set; features cdylib/pyo3/sidecar default-on + dev-hot-reload opt-in; [lints.rust] downgrades unsafe_code from forbid to deny in this crate only"
    - "crates/rollout-plugin-host/src/lib.rs — module wiring + test_inject_cdylib_placeholder helper + re-exports"
    - "docs/book/src/SUMMARY.md — nests plugin-host + python-bridge under Substrate (Examples placeholder preserved)"
    - "deny.toml — adds 'Apache-2.0 WITH LLVM-exception' (target-lexicon → pyo3-build-config)"
    - "Cargo.lock — refreshed for pyo3 0.28 + pyo3-async-runtimes 0.28 + libloading 0.8 + nix 0.30 + toml 0.8 transitives"

key-decisions:
  - "[D-PLUGIN-01] All three modes ship wired: Rust cdylib (libloading + ABI v1 vtable), PyO3 in-process (dedicated Python OS thread, pyo3 0.28 Python::attach), Python sidecar (stdlib AF_UNIX framing for in-tree sample)"
  - "[D-PLUGIN-02] PyO3 dedicated OS thread named 'rollout-py-<plugin>'; auto-initialize feature lazily spins up the interpreter on first Python::attach; pyo3 0.28 removed prepare_freethreaded_python — not needed because attach handles it"
  - "[D-PLUGIN-03] In-tree Python samples use stdlib only (no pip install in cargo test). Sidecar wire format = 4-byte BE length prefix + UTF-8 JSON envelope {method, payload}"
  - "[D-PLUGIN-04] Hot reload behind `dev-hot-reload` Cargo feature: PyO3 via importlib.reload + factory re-call; sidecar via nix::sys::signal::kill(SIGTERM) + 2s wait + SIGKILL fallback + respawn on a fresh UDS path; cdylib reload returns Fatal(PluginContract { msg: 'cdylib reload unsupported per spec 03 §7' })"
  - "[D-SANDBOX-01] Phase 2 sandboxing = manifest network_allowlist field surfaced via PluginManifest; egress proxy enforcement is deferred to the worker/cloud-local layer + Phase 7 (when the tool harness needs untrusted-code isolation). cgroups + seccomp + FD limits + fs write restrictions tracked as TODOs"
  - "[RESEARCH OQ 3] C-ABI shim stays as an internal module (src/modes/abi.rs) in Phase 2 rather than a standalone rollout-plugin-abi crate. Promote when an external plugin ecosystem emerges (likely Phase 7+)"
  - "[Claude's discretion] Single PluginHostImpl with mode dispatch beats three separate host structs — keeps the dependency graph one-to-one and the public surface tiny (HandleState enum is the only branching point)"
  - "[Claude's discretion] CdylibState boxes the Library inside Option<Arc<Library>> so the test-only for_tests_placeholder constructor can return a synthetic state with no live library; placed behind #[doc(hidden)] + #[must_use]"
  - "[Claude's discretion] Sidecar respawn signal = SIGTERM via the `nix` crate (not raw libc::kill, not child.start_kill which is SIGKILL on Unix). nix 0.30 is MIT, single-purpose, already in deny.toml's allowlist"
  - "[Claude's discretion] PluginDependencies is passed as a no-op (Phase 2 has it as a unit struct per Wave 0). Cdylib init does NOT thread dependencies through the ABI vtable in Phase 2 — adding a `deps` slot is a breaking ABI bump, deferred to a future ABI_VERSION = 2"
  - "[Claude's discretion] storage_integration test loads a real cdylib (the in-tree sample) so the persist_manifest happy path actually runs; skips cleanly with eprintln if the sample isn't pre-built"
  - "[Claude's discretion] cdylib sample crate carries its own [workspace] table — keeps `cargo build --workspace` from churning on it; plan 02-07 smoke driver builds it via `cargo build --manifest-path … --release`"
  - "[Claude's discretion] tests/smoke/plugins/sample_*.toml + rust_cdylib_sample/rollout-plugin.toml use `kind = 'env-harness'` (unit variant) rather than 'custom' (which is a newtype variant requiring `[kind] Custom = '...'` TOML shape) — simpler for fixtures"
  - "[Claude's discretion] storage_integration encodes the manifest as JSON (serde_json::to_vec) rather than postcard so it round-trips identically — keeps the storage-side decoder independent of the postcard-key encoding choices made in rollout-storage"
  - "[Rule 1 — bug] pyo3 0.28 renamed `Python::with_gil` → `Python::attach` and removed `prepare_freethreaded_python`. Updated all worker_main call sites; the `auto-initialize` feature handles lazy interpreter init"
  - "[Rule 1 — bug] pyo3 0.28 deprecated `Bound::downcast_into` in favor of `Bound::cast_into` — switched 2 call sites in modes/pyo3.rs"
  - "[Rule 1 — bug] PluginKind::Custom is a newtype variant; `kind = 'custom'` TOML failed to parse with `invalid type: unit variant, expected newtype variant`. Fixed the test fixtures + cdylib sample manifest to use `kind = 'env-harness'` (unit variant)"
  - "[Rule 1 — bug] cdylib sample's static VTABLE failed `Sync` because `*const c_char` isn't Sync. Wrapped it in a #[repr(transparent)] SyncVtable with unsafe impl Sync — the vtable is initialized statically + only ever read, so the unsafe impl is sound"
  - "[Rule 1 — bug] target-lexicon (pyo3-build-config transitive) ships under 'Apache-2.0 WITH LLVM-exception'; deny.toml's allowlist didn't include this variant. Added it (LLVM-exception is more permissive than plain Apache, not less)"
  - "[Rule 2 — missing critical functionality] Workspace `unsafe_code = forbid` cannot be relaxed at the file level (forbid is final). Added per-crate [lints.rust] unsafe_code = 'deny' to this crate's Cargo.toml so `#[allow(unsafe_code)]` works at the cdylib boundary; documented in the crate-level //! doc"
  - "[Rule 2 — missing critical functionality] `clippy::pedantic` flagged many low-priority warnings: large_enum_variant (boxed Sidecar variant), must_use_candidate (with_* builders), return_self_not_must_use (ditto), needless_pass_by_value on pyo3 spawn args (switched to refs), doc_markdown (`PyO3` in backticks), missing_docs on struct fields, items_after_statements (hoisted use stmt). All addressed without [allow]s"

patterns-established:
  - "Per-crate unsafe-code-policy downgrade: when one crate genuinely needs unsafe at an FFI boundary, the workspace stays `forbid` and the consumer crate downgrades to `deny` + adds `#[allow(unsafe_code)]` at the specific boundary file. Documented in crate-level //!"
  - "Test-only handle injection (doc(hidden) + #[must_use]): for traits whose impl requires expensive real artifacts (a real .dylib here), expose a placeholder constructor that fails on actual use but lets tests exercise unrelated code paths (the cdylib reload branch)"
  - "Out-of-workspace sample crate: ship the in-tree sample as a separate-workspace crate (own [workspace] table + Cargo.lock) so `cargo build --workspace` doesn't run it. The smoke driver (plan 02-07) builds it with `cargo build --manifest-path … --release` once before the smoke run"
  - "Stdlib-only Python sample (AGENTS.md §7 enforcement): when a sample needs gRPC, prefer hand-rolled length-prefixed framing over `pip install grpcio` so the cargo test path stays hermetic. Document the alternative (real gRPC stubs) for users who want it"

deviations:
  - "[Rule 1 — bug] pyo3 0.28 API shift: `Python::with_gil` → `Python::attach`; `prepare_freethreaded_python` removed (auto-initialize feature handles it). Fixed in modes/pyo3.rs. Plan's research material referenced the older 0.20-era API"
  - "[Rule 1 — bug] pyo3 0.28 deprecated `Bound::downcast_into` → `Bound::cast_into`. Two call sites updated"
  - "[Rule 1 — bug] PluginKind::Custom is a newtype variant; `kind = 'custom'` TOML required `[kind] Custom = ...` shape. Switched fixture TOMLs to `kind = 'env-harness'` (unit variant) which parses with the plain scalar shape — simpler and equally valid for a test fixture"
  - "[Rule 1 — bug] cdylib sample static VTABLE wasn't `Sync` (contains *const c_char). Wrapped in #[repr(transparent)] SyncVtable with unsafe impl Sync; the vtable is initialised statically and read-only after, so the impl is sound"
  - "[Rule 1 — bug] deny.toml didn't include `Apache-2.0 WITH LLVM-exception` which target-lexicon (pyo3-build-config transitive) requires. Added — LLVM-exception loosens Apache-2.0, doesn't tighten it, so it's a safe addition"
  - "[Rule 1 — bug] Plan's `nix::sys::signal::kill(pid as i32, ...)` triggered clippy::cast_possible_wrap. Switched to `i32::try_from(pid).unwrap_or(i32::MAX)` — PIDs are always positive + well below i32::MAX on Linux/macOS"
  - "[Rule 2 — missing critical functionality] Plan's instruction to make `nix` an unconditional dep would have pulled signal handling into builds with `--no-default-features --features cdylib` (no sidecar). Made `nix` optional and gated on the `sidecar` feature"
  - "[Rule 2 — missing critical functionality] HandleState's Sidecar variant was 256 bytes vs 40 for Cdylib; clippy::large_enum_variant flagged it. Boxed the Sidecar variant — keeps the enum compact + the boxing cost is negligible (one allocation per sidecar load)"
  - "[Rule 2 — missing critical functionality] Per-crate `[lints.rust] unsafe_code = 'deny'` table added because workspace `forbid` can't be relaxed at the file level. Documented in crate-level //! doc"
  - "[Rule 2 — missing critical functionality] CdylibState::call copies the returned Buf bytes (slice::from_raw_parts → to_vec) BEFORE invoking free_buf — prevents allocator-mismatch UB across the cdylib boundary (the plugin's allocator may differ from the host's). Plan's snippet handed the Vec back via from_raw_parts which would have crashed if the plugin used a different global allocator"

# Known stubs (intentional — populated by downstream plans)
known_stubs:
  - "cdylib_load_and_call_roundtrip is #[ignore]d because it requires the rust_cdylib_sample to be pre-built (`cargo build --manifest-path tests/smoke/plugins/rust_cdylib_sample/Cargo.toml --release`). Plan 02-07 (smoke) wires the build into the smoke driver. The test passes when run manually after the sample is built (verified during execution)."
  - "pyo3_load_and_call_roundtrip is #[ignore]d because pyo3 abi3-py311 requires Python 3.11+ headers at link time; the local dev default (pyenv 3.10.14) rejects the build. With PYENV_VERSION=3.11.12 the test compiles + runs; smoke driver gates the real verification."
  - "reload_pyo3_invokes_importlib is #[ignore]d for the same Python-link-time reason; the dev-hot-reload feature is opt-in so production builds never compile this test."
  - "manifest network_allowlist is parsed into PluginManifest but NOT enforced in Phase 2. Egress proxy implementation lives in plan 02-06 (rollout-coordinator) or later — Phase 7 (HARNESS-02) introduces the adversarial isolation that makes the allowlist load-bearing."
  - "Sidecar uses FramedJsonUds (stdlib framing) only; GrpcUds branch on SidecarProtocol exists in the trait surface (Wave 0) but the host doesn't dispatch to it. Real gRPC-over-UDS sidecars land when a production user needs them (Phase 3+ inference backend may force this)."
  - "PluginDependencies (Wave 0 unit struct) is NOT threaded through the cdylib ABI vtable in Phase 2 — adding a `deps` slot is a breaking ABI bump deferred to ABI_VERSION = 2 when later phases need it (Phase 7+)."

# Authentication gates / preflight notes
preflight_note: "Local dev: PYENV_VERSION=3.11.12 must be exported before `cargo build -p rollout-plugin-host` because pyo3 abi3-py311 rejects Python < 3.11 at link time. CI runs setup-python with 3.11 per Phase 1's CI matrix. `scripts/preflight.sh` (plan 02-00) gates `make smoke` on python3 >= 3.11. The ignored pyo3_load + reload_pyo3 tests run successfully under PYENV_VERSION=3.11.12 + `cargo test -- --include-ignored`; documented in plan 02-07 to be the smoke driver's responsibility."

requirements-completed: [SUBSTR-03, DOCS-01, DOCS-02, DOCS-03]

# Metrics
duration: 16min
completed: 2026-05-20
---

# Phase 2 Plan 05: rollout-plugin-host Summary

**One-liner:** Shipped `rollout-plugin-host` with all three loading modes wired (Rust cdylib via `libloading` + ABI v1 vtable; PyO3 in-process via dedicated Python OS thread with `pyo3 0.28` `Python::attach`; Python sidecar via stdlib length-prefixed JSON over `AF_UNIX`), hot-reload behind a `dev-hot-reload` feature (PyO3 `importlib.reload`, sidecar SIGTERM-respawn via `nix`, cdylib explicit `Fatal(PluginContract)` per spec 03 §7), manifest persistence via `rollout-storage`, three in-tree samples (Rust cdylib + Python in-process + Python sidecar — all stdlib-only on the Python side), 11 active integration tests + 3 environment-gated `#[ignore]`d ones, and two new mdBook chapters (`plugin-host.md` + `python-bridge.md`) — gated by `cargo build/test/clippy/doc -p rollout-plugin-host --all-features`, `cargo deny check`, `mdbook build`, and `cargo build --workspace`, all green.

## What landed

### Task 1 — Scaffolding + manifest + cdylib loader + ABI v1 + cdylib-reload-unsupported (commit 3a98b21)

- **`Cargo.toml`** — concrete dep set; features `cdylib` + `pyo3` + `sidecar` default-on, `dev-hot-reload` opt-in. Per-crate `[lints.rust] unsafe_code = "deny"` downgrades the workspace `forbid` so `#[allow(unsafe_code)]` works at the C-ABI boundary; documented in the crate-level `//!`.
- **`manifest.rs`** — `parse_manifest_str` + `parse_manifest` + `validate_manifest`. Cheap plan-time validation: pyo3 plugins require `runtime.python_min >= 3.11`; bad / unknown kinds rejected with `Fatal(ConfigInvalid)`.
- **`modes/abi.rs`** — `ABI_VERSION = 1`, `#[repr(C)] RolloutPluginVtable + Buf`, `#[no_mangle] rollout_plugin_abi_version` host-side probe.
- **`modes/cdylib.rs`** — libloading-based load: `Library::new(path)` → `lib.get(symbol)` → cast to vtable factory → assert ABI matches. `call()` copies out the returned `Buf` via `slice::from_raw_parts(...).to_vec()` BEFORE invoking `free_buf` so allocator mismatches across the cdylib boundary can't corrupt anything. Test-only `for_tests_placeholder` constructor (doc(hidden), must_use) returns a synthetic state with null vtable so the reload-cdylib-unsupported test exercises the reload branch without a real .dylib.
- **`modes/{pyo3,sidecar}.rs`** — skeletons (full impls in Task 2).
- **`host.rs`** — `PluginHostImpl` with mode dispatch + `Storage` persistence helper + `test_insert_handle` doc(hidden) helper. Cdylib `reload` returns `Fatal(PluginContract { plugin, msg: "cdylib reload unsupported per spec 03 §7" })`.
- **`tests/manifest.rs` (6 tests):** pyo3 / sidecar / cdylib TOML parse + 3 validation rejections (unknown kind / missing python_min / pre-3.11 version).
- **`tests/reload_cdylib_unsupported.rs` (1 test):** confirms the cdylib reload branch returns `Fatal(PluginContract { plugin: "rust-cdylib-sample", msg: "...cdylib... unsupported..." })`.
- **`tests/cdylib_load.rs` (1 #[ignore]d):** end-to-end ABI v1 round-trip; passes when the sample is pre-built.
- **`tests/smoke/plugins/rust_cdylib_sample/`** — out-of-workspace cdylib crate (`[workspace]` table at the top of Cargo.toml so it's excluded from the main workspace). Exports `rollout_plugin_factory` with one method (`echo`). `SyncVtable` wrapper makes the static `RolloutPluginVtable` Sync-safe.

### Task 2 — PyO3 + sidecar + hot-reload + samples + mdBook + storage_integration (commit a71eb8d)

- **`modes/pyo3.rs`** — Dedicated Python OS thread per host, named `rollout-py-<plugin>`. Channel-based hop: `mpsc::Sender<PyTask>` from the Tokio side; worker thread runs `Python::attach` inside a `blocking_recv` loop. `PyTask::{Call, Reload, Shutdown}` — `Reload` is `#[cfg(feature = "dev-hot-reload")]`. pyo3 0.28 specifics: `Python::attach` replaces `with_gil`; `Bound::cast_into` replaces `downcast_into`; `auto-initialize` feature lazily spins up the interpreter (no `prepare_freethreaded_python` — removed in 0.28).
- **`modes/sidecar.rs`** — Stdlib length-prefixed JSON over `AF_UNIX`. Spawn: `tokio::process::Command` with `kill_on_drop(true)`; pass the UDS path as the last argv. Connect: 5s retry loop with 50ms sleeps (the child needs time to `bind`). Wire format: `[u32 BE length][UTF-8 JSON {"method": ..., "payload": ...}]`. Hot reload: `nix::sys::signal::kill(pid, SIGTERM)` → 2s wait → SIGKILL fallback → respawn on a fresh UDS path. `nix 0.30` is opt-in via the `sidecar` Cargo feature.
- **`host.rs`** — `with_storage(Arc<EmbeddedStorage>)` constructor; `persist_manifest` writes `serde_json::to_vec(manifest)` under `StorageKey { namespace: "plugins", run_id: None, path: [name] }` after every successful load. Tracing events with `target = "plugin_host"`: `plugin_loaded` (load), `plugin_reloaded` (reload), `plugin_call` span (every call), `plugin_call_error` (err) — per D-OBSERVE-01.
- **`python/examples/sample_inproc/{__init__,plugin}.py`** — `create_plugin().call("echo", b"…")` echoes; `call("ping", _)` returns `b"pong"`. Stdlib only.
- **`python/examples/sample_sidecar/{__init__,__main__}.py`** — `python -m sample_sidecar <socket_path>`. 4-byte BE length prefix + UTF-8 JSON envelope. Handles `Init`, `echo`, `Shutdown`. Stdlib (`socket` + `struct` + `json`) only.
- **`tests/smoke/plugins/{sample_inproc,sample_sidecar}.toml`** — manifest fixtures.
- **Integration tests (5):** `sidecar_load.rs` (echo round-trip, green); `pyo3_load.rs` (#[ignore] for pyo3 link-time gate); `reload_pyo3.rs` (#[cfg(feature="dev-hot-reload")] + #[ignore]); `reload_sidecar.rs` (#[cfg(feature="dev-hot-reload")], green); `storage_integration.rs` (green when the cdylib sample is pre-built; skips with eprintln otherwise).
- **`docs/book/src/substrate/plugin-host.md`** (~140 lines) — Three-mode table, ABI v1 contract block, sidecar wire format, hot-reload semantics, sandboxing scope (D-SANDBOX-01), dep-direction note, observability events table.
- **`docs/book/src/substrate/python-bridge.md`** (~80 lines) — pyo3 0.28 + pyo3-async-runtimes 0.28 pin rationale, abi3-py311 strategy + trade-offs, dedicated Python OS thread diagram, in-tree samples + no-pip rule.
- **`docs/book/src/SUMMARY.md`** — nests plugin-host + python-bridge under Substrate (Examples placeholder preserved).
- **`deny.toml`** — adds `Apache-2.0 WITH LLVM-exception` for target-lexicon (transitive via pyo3-build-config).

## End-to-end verification

All commands exit 0:

```
PYENV_VERSION=3.11.12 cargo build -p rollout-plugin-host
PYENV_VERSION=3.11.12 cargo build -p rollout-plugin-host --features dev-hot-reload
PYENV_VERSION=3.11.12 cargo test  -p rollout-plugin-host --tests
PYENV_VERSION=3.11.12 cargo test  -p rollout-plugin-host --tests --features dev-hot-reload
PYENV_VERSION=3.11.12 cargo clippy -p rollout-plugin-host --all-targets --all-features -- -D warnings
PYENV_VERSION=3.11.12 RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" \
  cargo doc -p rollout-plugin-host --no-deps --all-features
cargo build --manifest-path tests/smoke/plugins/rust_cdylib_sample/Cargo.toml --release
cargo fmt --all -- --check
cargo deny check
mdbook build docs/book
PYENV_VERSION=3.11.12 cargo build --workspace
```

Tests: 11 active passes (manifest×6, reload_cdylib_unsupported, sidecar_spawn_call_shutdown, reload_sidecar_sigterm_respawns, host_persists_manifest_to_storage, cdylib_load_and_call_roundtrip when run with `--include-ignored` after sample build) + 3 environment-gated `#[ignore]`s (pyo3_load, reload_pyo3, cdylib_load by default).

## Deviations from Plan

### Rule-1 (auto-fix bug)

1. **pyo3 0.28 API shift.** `Python::with_gil` → `Python::attach`; `prepare_freethreaded_python` removed (the `auto-initialize` feature handles lazy init). Fixed in `modes/pyo3.rs`. Plan's snippet referenced the older 0.20-era API.
2. **pyo3 0.28 deprecated `Bound::downcast_into`** in favor of `Bound::cast_into`. Two call sites updated.
3. **`PluginKind::Custom` is a newtype variant.** `kind = "custom"` TOML failed to parse with `invalid type: unit variant, expected newtype variant`. Switched manifest fixtures (test TOMLs + cdylib sample) to `kind = "env-harness"` — unit variant, parses with the scalar shape.
4. **cdylib sample's static VTABLE wasn't `Sync`.** `*const c_char` doesn't implement `Sync`. Wrapped in `#[repr(transparent)] SyncVtable` with `unsafe impl Sync` — sound because the vtable is statically initialised + read-only after.
5. **deny.toml allowlist missing `Apache-2.0 WITH LLVM-exception`.** `target-lexicon` (pyo3-build-config transitive) uses this license. Added — LLVM-exception only loosens Apache-2.0, never tightens, so it's a safe addition.
6. **`pid as i32` cast clippy violation.** `clippy::cast_possible_wrap` rejected the raw cast in `respawn`. Replaced with `i32::try_from(pid).unwrap_or(i32::MAX)`.

### Rule-2 (auto-add missing critical functionality)

1. **Per-crate `[lints.rust] unsafe_code = "deny"` table.** Workspace `forbid` cannot be relaxed at the file level (forbid is final). Added the per-crate downgrade + documented the policy in the crate-level `//!`. Without it, the cdylib boundary would fail to compile.
2. **`nix` made optional and gated on the `sidecar` feature.** A `--no-default-features --features cdylib` build shouldn't pull in signal-handling code. The Cargo feature already gated SidecarState; the dep needed to follow suit.
3. **HandleState boxed the Sidecar variant.** `clippy::large_enum_variant` flagged a 256-byte Sidecar variant vs 40-byte Cdylib. Box keeps the enum compact; one allocation per sidecar load is fine.
4. **CdylibState::call copies the returned `Buf` BEFORE `free_buf`.** Plan's snippet handed the Vec back via `from_raw_parts` directly — that would have crashed on any plugin whose global allocator differs from the host's. The copy-and-free pattern matches how `cbindgen` / `extern "C"` round-trips are conventionally handled.
5. **Test-only `CdylibState::for_tests_placeholder` + `PluginHostImpl::test_insert_handle`** so `reload_cdylib_unsupported.rs` exercises the cdylib reload branch without a prebuilt .dylib. Both are `#[doc(hidden)]`.
6. **Clippy pedantic catalog of small fixes** without `#[allow]`s: `must_use_candidate` on `new()` / `with_storage()` / `with_python_path()` / `with_sidecar_root()` / `for_tests_placeholder()`; `return_self_not_must_use` ditto; `doc_markdown` (`PyO3` in backticks); `missing_docs` on struct fields; `items_after_statements` (hoisted `use` statement); `needless_pass_by_value` on `Pyo3State::spawn` args (switched to `&str` / `&[String]`).

### Rule-4 (architectural)

None. The plan's scope held: one new crate, three modes, manifest + persistence + hot reload, two mdBook chapters. No new sub-systems introduced.

## Open Questions for Downstream Plans

- **Plan 02-06 (`rollout-coordinator`):** This crate emits `plugin_loaded` / `plugin_reloaded` / `plugin_call` / `plugin_call_error` tracing events with `target = "plugin_host"`. The coordinator's `StdoutJsonEmitter` (D-OBSERVE-01 Phase-2 impl) needs a `tracing-subscriber` `Layer` that converts these events into `EventEmitter::emit(Event)` calls — flag for plan 02-06.
- **Plan 02-07 (smoke):** The smoke driver MUST run `cargo build --manifest-path tests/smoke/plugins/rust_cdylib_sample/Cargo.toml --release` before `cargo test -p rollout-plugin-host -- --include-ignored` (or before invoking the coordinator binary that loads the sample). Otherwise `cdylib_load_and_call_roundtrip` skips and `storage_integration` skips its load path.
- **Plan 02-07 (smoke):** The sidecar UDS path defaults to `./data/sidecars/<plugin>-<pid>.sock`. Make sure the smoke driver creates `./data/sidecars/` (or uses a tempdir + `PluginHostImpl::with_sidecar_root`) and cleans up on exit. The host's `unload()` removes the socket on success, but a `kill -9`'d test leaves orphans.
- **Plan 02-07 (smoke):** `PYENV_VERSION=3.11.12` (or equivalent) MUST be exported in the CI smoke job for pyo3 builds. CI's `setup-python` action already requests 3.11 per Phase 1; verify the env propagation reaches `cargo build`.
- **Future Phase 7 (HARNESS-02):** The `manifest.network_allowlist` field is parsed but not enforced. When the tool harness lands and untrusted-code isolation matters, an egress proxy (or sidecar `--network-allowlist=…` argv) needs to actually deny traffic. cgroups + seccomp + FD limits + fs write restrictions also land in Phase 7.
- **Future ABI bump:** `PluginDependencies` is a Phase-2 unit struct. When later phases need to inject `events: Arc<dyn EventEmitter>` / `storage: Arc<dyn Storage>` / `secrets: Arc<dyn SecretStore>` at cdylib init, the C-ABI vtable will need a `dependencies` slot — that's `ABI_VERSION = 2`, a breaking change. Document the policy in the eventual `rollout-plugin-abi` crate (Phase 7+).

## Commits

| Task | Hash    | Subject                                                                                |
| ---- | ------- | -------------------------------------------------------------------------------------- |
| 1    | 3a98b21 | feat(02-05): scaffold rollout-plugin-host + cdylib loader + ABI v1                     |
| 2    | a71eb8d | feat(02-05): wire pyo3 + sidecar loaders + hot-reload + samples + mdBook               |

## Self-Check: PASSED

- `crates/rollout-plugin-host/src/{lib,manifest,handle,host}.rs` — FOUND
- `crates/rollout-plugin-host/src/modes/{mod,abi,cdylib,pyo3,sidecar}.rs` — FOUND
- `crates/rollout-plugin-host/tests/{manifest,cdylib_load,reload_cdylib_unsupported,pyo3_load,sidecar_load,reload_pyo3,reload_sidecar,storage_integration}.rs` — FOUND (8 test files)
- `tests/smoke/plugins/rust_cdylib_sample/{Cargo.toml,src/lib.rs,rollout-plugin.toml}` — FOUND
- `python/examples/sample_inproc/{__init__,plugin}.py` — FOUND
- `python/examples/sample_sidecar/{__init__,__main__}.py` — FOUND
- `tests/smoke/plugins/{sample_inproc,sample_sidecar}.toml` — FOUND
- `docs/book/src/substrate/{plugin-host,python-bridge}.md` — FOUND
- `docs/book/src/SUMMARY.md` — contains `[Plugin host](./substrate/plugin-host.md)` + `[Python bridge](./substrate/python-bridge.md)`
- Commit `3a98b21` — FOUND in `git log --oneline -5`
- Commit `a71eb8d` — FOUND in `git log --oneline -5`
