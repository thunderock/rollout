---
phase: 02-local-substrate
plan: 00
subsystem: substrate-core
tags: [rollout-core, traits, trait-extensions, storage, plugin-host, coordinator, observability, event-emitter, workspace, dep-direction, mdbook, specs, preflight]

# Dependency graph
requires:
  - phase: 01-core-foundations
    provides: rollout-core trait stubs + dep-direction integration test + deny.toml + mdBook scaffold (plans 01-03, 01-05, 01-07)
provides:
  - "Storage / StorageTxn trait surface aligned with spec 04 §2 (get_bytes / get_many_bytes / scan_bytes / watch / put_bytes / delete / cas_bytes / abort) + StorageKey/KeyRange/StorageEvent types"
  - "PluginHost trait surface aligned with spec 03 §4-§5 (load(PluginManifest) -> PluginHandle, call, reload, unload) + PluginManifest/PluginHandle/PluginKind/PluginMode/EntrySpec/SidecarProtocol/RuntimeHints/PluginDependencies/PluginId"
  - "Worker::init/ready lifecycle hooks + Coordinator::heartbeat per spec 01 §2 + spec 05 §6"
  - "Heartbeat struct + WorkerState enum (Init/Ready/Running/Draining)"
  - "ObjectStore content-addressed put_bytes/get_bytes/exists + PutHint (spec 06 §3)"
  - "Queue ack/nack with QueueItemId(Ulid) handles"
  - "ComputeHint inventory() / preemption_signal() + ComputeInventory/GpuInfo"
  - "SecretStore::put surface (local impl returns Fatal(ConfigInvalid))"
  - "EventEmitter trait + Event/EventKind/Level/SpanPhase types per spec 09 §2 (D-OBSERVE-01)"
  - "Six workspace-registered Phase-2 crate stubs (rollout-proto/storage/cloud-local/transport/plugin-host/coordinator)"
  - "Phase-2 workspace dependency pin table (redb 2.5, tonic 0.14, prost 0.13, pyo3 0.28, pyo3-async-runtimes 0.28, libloading 0.8, rustls 0.23, rcgen 0.13, postcard 1.0, sysinfo 0.33, nvml-wrapper 0.11, plus tokio extensions and dev/test deps)"
  - "Two new dep-direction lint invariants (rollout-transport ↛ rollout-cloud-*; rollout-plugin-host ↛ rollout-transport) + their Cargo.toml fixtures"
  - "scripts/preflight.sh gating make smoke on cargo + make + python3 ≥ 3.11"
  - "docs/book/src/substrate/index.md landing page + SUMMARY.md wire-up"
  - "Phase 2 implementation notes added to specs 01/03/04/06"
affects: [02-01-rollout-proto, 02-02-rollout-storage, 02-03-rollout-cloud-local, 02-04-rollout-transport, 02-05-rollout-plugin-host, 02-06-rollout-coordinator, 02-07-smoke-and-docs]

# Tech tracking
tech-stack:
  added:
    - "smol_str =0.3.2 (with serde feature) — SmolStr keys in StorageKey (pinned to 0.3.2 because 0.3.4+ require Rust 1.89, workspace pins 1.88)"
    - "tokio 1.40 (sync feature in rollout-core for broadcast::Receiver) — Phase-2 workspace pin"
    - "redb 2.5, postcard 1.0, tonic 0.14, tonic-build 0.14, prost 0.13, prost-types 0.13, rustls 0.23 (ring+std), rcgen 0.13, bytes 1.7"
    - "pyo3 0.28 (abi3-py311), pyo3-async-runtimes 0.28 (tokio-runtime), libloading 0.8"
    - "sysinfo 0.33 (system feature), nvml-wrapper 0.11"
    - "tokio-util 0.7 (io), tokio-stream 0.1 (sync+net), tracing-subscriber 0.3 (env-filter+json), humantime-serde 1.1"
    - "toml 0.8, hex 0.4"
    - "tempfile 3.10, proptest 1.5, assert_cmd 2.0, predicates 3.1 (dev-deps)"
  patterns:
    - "Trait surface kept object-safe via Vec<u8> payloads — generic typed-payload methods (get<T>/put<T>/cas<T>) deferred to free helpers in downstream crates"
    - "`scan_bytes` returns owned Vec<(StorageKey, Vec<u8>)> rather than BoxStream — async_trait + dyn-Storage cannot return streams on stable Rust (documented in spec 04 §1a)"
    - "Every new pub item carries a one-line /// doc to keep DOCS-03 rustdoc gate green"
    - "Each Phase-2 stub crate uses `rollout-core = { path = \"../rollout-core\", version = \"0.1\" }` so cargo-deny's no-wildcard rule stays satisfied"
    - "Two-fixture dep-direction lint pattern from Phase 1 extended: hand-rolled Cargo.toml under tests/fixtures/ + non-workspace-member + tolerant TOML extraction"

key-files:
  created:
    - "crates/rollout-core/src/traits/observability.rs — EventEmitter + Event/EventKind/Level/SpanPhase per spec 09 §2 (D-OBSERVE-01)"
    - "crates/rollout-core/tests/fixtures/violation_transport_cloud/Cargo.toml + src/lib.rs — fixture for rollout-transport → rollout-cloud-local invariant"
    - "crates/rollout-core/tests/fixtures/violation_plugin_host_transport/Cargo.toml + src/lib.rs — fixture for rollout-plugin-host → rollout-transport invariant"
    - "crates/rollout-proto/{Cargo.toml,build.rs,src/lib.rs} — Wave-1 stub"
    - "crates/rollout-storage/{Cargo.toml,src/lib.rs} — Wave-1 stub"
    - "crates/rollout-cloud-local/{Cargo.toml,src/lib.rs} — Wave-1 stub"
    - "crates/rollout-transport/{Cargo.toml,src/lib.rs} — Wave-2 stub"
    - "crates/rollout-plugin-host/{Cargo.toml,src/lib.rs} — Wave-3 stub"
    - "crates/rollout-coordinator/{Cargo.toml,src/lib.rs,src/main.rs} — Wave-3 stub + binary placeholder"
    - "scripts/preflight.sh — preflight gate for make smoke"
    - "docs/book/src/substrate/index.md — Phase-2 substrate landing page"
  modified:
    - "Cargo.toml — added 6 workspace members + Phase-2 workspace.dependencies pin table"
    - "crates/rollout-core/Cargo.toml — added smol_str + tokio deps"
    - "crates/rollout-core/src/lib.rs — re-exports for new Phase-2 types/traits"
    - "crates/rollout-core/src/traits/mod.rs — registered observability module + new re-exports"
    - "crates/rollout-core/src/traits/storage.rs — Phase-2 surface"
    - "crates/rollout-core/src/traits/plugin.rs — Phase-2 surface"
    - "crates/rollout-core/src/traits/worker.rs — Phase-2 surface"
    - "crates/rollout-core/src/traits/cloud.rs — Phase-2 surface"
    - "crates/rollout-core/tests/trait_surface.rs — added 8 Phase-2 RED-now-GREEN tests + event_emitter object-safety check"
    - "crates/rollout-core/tests/dependency_direction.rs — added 2 new invariants + 2 new fixture-detection tests"
    - "docs/specs/01-core-runtime.md — Phase 2 implementation notes §1a"
    - "docs/specs/03-plugin-system.md — Phase 2 implementation notes §1a"
    - "docs/specs/04-storage-snapshots.md — Phase 2 implementation notes §1a (scan returns Vec not BoxStream)"
    - "docs/specs/06-cloud-layer.md — Phase 2 implementation notes §1a"
    - "docs/book/src/SUMMARY.md — added Substrate section entry (preserved Examples placeholder)"

key-decisions:
  - "[Claude] smol_str pinned to =0.3.2 (not 0.3 floating) because 0.3.4+ raise MSRV to 1.89 and workspace rust-toolchain is 1.88.0"
  - "[Claude] nvml-wrapper added at workspace level without `optional = true` (cargo rejects optional workspace.dependencies); per-crate optionality will be re-introduced at the consumer site (rollout-cloud-local) in plan 02-03"
  - "[Spec → AGENTS.md §4] Storage::scan_bytes returns Vec, not BoxStream — async_trait + object-safety constraint; documented in spec 04 §1a Phase 2 implementation notes"
  - "[Spec → AGENTS.md §4] PluginHost methods use Vec<u8> payloads (not generic Req/Res); typed-payload helpers deferred"
  - "[Spec → AGENTS.md §4] ObjectStore Phase-1 string-keyed put/get had no impls and is replaced by content-addressed put_bytes/get_bytes/exists; documented in spec 06 §1a"
  - "[Spec → AGENTS.md §4] ComputeHint::instance_type folded into ComputeInventory.instance_type; the single inventory() call replaces 4–5 Phase-1 spec methods (region/zone/instance_type/gpu_inventory)"
  - "[D-OBSERVE-01] EventEmitter trait lands in rollout-core in Phase 2 (per plan revision iter 1); stdout-JSON impl lives in plan 02-06"
  - "[CLAUDE.md] Comments kept terse — one-line /// docs per AGENTS.md §9.3, no multi-paragraph rustdoc on stub crates"
  - "[Plan rationale] Six new crates registered as empty stubs with `path = ../rollout-core, version = 0.1` to satisfy cargo-deny's no-wildcard rule (Phase-1 pattern from rollout-cli)"

patterns-established:
  - "Workspace stub crate pattern: Cargo.toml with workspace package keys + [lints] workspace = true + rollout-core path+version dep; src/lib.rs with crate-level //! doc citing the upcoming plan"
  - "Phase-N implementation notes section as `§1a` in each spec — keeps the Phase-1 spec body authoritative while letting Phase-N annotate deviations without re-numbering"
  - "Dep-direction fixtures live as non-workspace-member Cargo.toml under tests/fixtures/violation_<edge>/ — cargo's tests/ auto-discovery skips them because there's no main.rs"

deviations:
  - "[Rule 2 — missing critical functionality] Added `[lints] workspace = true` to each stub crate Cargo.toml — required for the workspace missing_docs warning to apply uniformly per AGENTS.md §9.3 DOCS-03; the plan task description didn't spell this out but workspace-wide doc-policy demands it."
  - "[Rule 1 — bug fix] Plan instruction said `nvml-wrapper = { version = '0.11', optional = true }` but cargo rejects `optional = true` on workspace.dependencies. Removed the optional flag at workspace level; downstream crates (rollout-cloud-local) re-introduce optionality at the consumer site in plan 02-03."
  - "[Rule 1 — bug fix] Pinned smol_str to =0.3.2 instead of `0.3` (floating) because newer 0.3.4+ require Rust 1.89 and the workspace pins 1.88.0 in rust-toolchain.toml. Plan didn't anticipate the MSRV bump on the latest 0.3 patch."

# Known stubs (intentional — populated by downstream plans)
known_stubs:
  - "crates/rollout-{proto,storage,cloud-local,transport,plugin-host,coordinator}/src/lib.rs are crate-level //! stubs awaiting plans 02-01..02-06 — intentional per the Wave-0 plan rationale"
  - "crates/rollout-proto/build.rs is a stub print-only build.rs — plan 02-01 wires tonic-build"
  - "crates/rollout-coordinator/src/main.rs prints 'not yet implemented' and exits 2 — plan 02-06 ships the real binary"

# Authentication gates / preflight notes
preflight_note: "Local dev machine has python 3.10.14 selected via pyenv; preflight.sh correctly rejects this and accepts when PYENV_VERSION=3.11.12 is set. CI on macos-14/ubuntu-latest uses Python 3.11 per Phase-1 setup-python action. Documented in the substrate landing page."

requirements-completed: [SUBSTR-01, SUBSTR-02, SUBSTR-03, SUBSTR-04, DOCS-02, DOCS-03]

# Metrics
duration: 25min
completed: 2026-05-20
---

# Phase 2 Plan 00: Wave-0 Trait Extensions Summary

**One-liner:** Extended `rollout-core` to the Phase-2 spec surface, registered six Wave-1+ crate stubs in the workspace, pinned the full Phase-2 dependency stack, added two dep-direction lint invariants, shipped `EventEmitter` per spec 09 §2, and annotated specs 01/03/04/06 with Phase-2 implementation notes — all gated by `cargo build --workspace`, `cargo test --workspace`, `cargo clippy -D warnings`, `cargo deny check`, `cargo xtask schema-gen` drift-check, `mdbook build`, and `cargo doc` rustdoc-gate.

## What landed

### Task 1: rollout-core trait surface extensions + dep-direction fixtures

**Storage / StorageTxn** (spec 04 §2):

- `StorageKey { namespace: SmolStr, run_id: Option<RunId>, path: Vec<SmolStr> }`
- `KeyRange { prefix: StorageKey, limit: Option<usize> }`
- `StorageEvent::{Put, Delete}`
- `Storage::{begin, get_bytes, get_many_bytes, scan_bytes, watch, ping}` (object-safe; `scan_bytes` returns `Vec<(StorageKey, Vec<u8>)>` rather than `BoxStream` per the Phase 2 simplification documented in spec 04 §1a)
- `StorageTxn::{put_bytes, delete, cas_bytes, commit, abort}`
- `watch()` returns `tokio::sync::broadcast::Receiver<StorageEvent>` (in-process broadcast per D-STO-02)

**PluginHost** (spec 03 §4–§5):

- `PluginManifest { name, version, kind, trait_id, mode, runtime, entry, config_schema_path, network_allowlist }`
- `PluginKind::{EnvHarness, ToolHarness, EvalHarness, RewardModel, InferenceBackend, Storage, Queue, ObjectStore, Custom(String)}`
- `PluginMode::{Pyo3, Sidecar, RustCdylib}`
- `EntrySpec::{Cdylib, Pyo3, Sidecar}` + `SidecarProtocol::{GrpcUds, FramedJsonUds}`
- `RuntimeHints { python_min, gpu, memory_mib }`
- `PluginHandle { id: PluginId, manifest: PluginManifest }`
- `PluginId(String)`, `PluginDependencies` (Phase-2 empty struct; later phases extend)
- `PluginHost::{load(PluginManifest)->PluginHandle, call, reload, unload}` — all `async_trait`, `Vec<u8>` payloads

**Worker / Coordinator** (spec 01 §2 + spec 05 §6):

- `Heartbeat { worker_id, run_id, state, due_at: SystemTime }`
- `WorkerState::{Init, Ready, Running, Draining}`
- `Worker` adds `init(ctx)` and `ready()` BEFORE `run`
- `Coordinator::heartbeat(Heartbeat) -> Result<(), CoreError>`

**Cloud** (spec 06 §3):

- `PutHint { expected_size, content_type }`
- `GpuInfo { vendor, model, memory_mib }`
- `ComputeInventory { cpu_count, memory_mib, gpus, instance_type }`
- `QueueItemId(ulid::Ulid)`
- `ObjectStore::{put_bytes(Vec<u8>, PutHint)->ContentId, get_bytes(&ContentId), exists(&ContentId)}`
- `SecretStore::{get, put}` (local backend's `put` returns `Fatal(ConfigInvalid)` per D-LOCAL-03)
- `ComputeHint::{inventory()->ComputeInventory, preemption_signal()->Option<Duration>}`
- `Queue::{enqueue(Vec<u8>)->QueueItemId, dequeue()->Option<(QueueItemId, Vec<u8>)>, ack(QueueItemId), nack(QueueItemId)}`

**Observability** (spec 09 §2; D-OBSERVE-01):

- New `traits/observability.rs` module
- `Event { ts, kind, level, run_id, worker_id, trace_id, span_id, plugin_id, algorithm, message, attrs }`
- `EventKind::{Log, Metric{name,value,unit}, Span{phase}, Domain{topic}}`
- `Level::{Trace, Debug, Info, Warn, Error}`, `SpanPhase::{Start, End}`
- `EventEmitter::emit(&self, Event) -> Result<(), CoreError>` — object-safe via `async_trait`

**Tests:**

- `tests/trait_surface.rs` grew from the Phase-1 19-trait surface check to 9 new compile-shape tests (`storage_trait_has_extended_surface`, `storage_txn_has_extended_surface`, `plugin_host_has_extended_surface`, `coordinator_has_heartbeat`, `worker_has_lifecycle_hooks`, `cloud_traits_match_spec_06`, `new_types_exist`, `event_emitter_trait_exists`, `trait_surface_counts_19`).
- `tests/dependency_direction.rs` renamed positive scan to `dep_direction_invariants_hold` and gained two new violation predicates + two new fixture-detection tests; the original `deliberate_violation_fixture_is_detected` stays green.

### Task 2: Workspace registration + preflight + spec edits

- Six new crate directories with stub `Cargo.toml` + `src/lib.rs`. `rollout-coordinator` adds `[[bin]]` + `src/main.rs` placeholder; `rollout-proto` adds a `build.rs` stub.
- `[workspace] members` extended with all six (kept alphabetical-by-letter Phase-1 ordering broken only because the plan dictates the explicit list).
- `[workspace.dependencies]` extended with the full Phase-2 pin table from RESEARCH.md §"Standard Stack": `redb 2.5`, `postcard 1.0`, `tonic 0.14` + `tonic-build 0.14` + `prost 0.13` + `prost-types 0.13`, `pyo3 0.28` (abi3-py311) + `pyo3-async-runtimes 0.28` + `libloading 0.8`, `rustls 0.23` + `rcgen 0.13` + `bytes 1.7`, `sysinfo 0.33` + `nvml-wrapper 0.11`, `tokio-util 0.7` + `tokio-stream 0.1`, `tracing-subscriber 0.3`, `humantime-serde 1.1`, `toml 0.8`, `hex 0.4`, `tempfile 3.10`, `proptest 1.5`, `assert_cmd 2.0`, `predicates 3.1`.
- `scripts/preflight.sh` ships executable. Verifies `cargo`, `make`, `python3 ≥ 3.11`; warns (non-fatal) if `protoc` is missing since tonic-build vendors it.
- `docs/book/src/SUMMARY.md` adds `[Substrate](./substrate/index.md)` between Architecture and the reserved Examples placeholder.
- `docs/book/src/substrate/index.md` is a ~60-line landing page covering what ships in Phase 2, plan-of-record vs stretch, trait surface cross-links, per-crate chapter placeholders (filled in plan 02-07), and the preflight requirement.
- Specs 01/03/04/06 each gain a `## 1a. Phase 2 implementation notes` section explaining the trait extensions and Phase-2 simplifications.
- `.gitignore` already had `data/` — no change required.

## End-to-end verification

All commands exit 0:

```
cargo build --workspace
cargo test --workspace --tests           # 9 trait_surface tests + 4 dep_direction tests + Phase-1 tests
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo deny check                         # advisories ok, bans ok, licenses ok, sources ok
cargo xtask schema-gen && git diff --exit-code schemas/ python/
mdbook build docs/book
RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" \
  cargo doc --workspace --no-deps --all-features
```

Preflight: `bash scripts/preflight.sh` exits 0 when `python3 ≥ 3.11` is selected via pyenv (`PYENV_VERSION=3.11.12 bash scripts/preflight.sh` confirmed). The local default pyenv version is 3.10.14, which the script correctly rejects with `preflight FAIL: python3 3.10 detected; need >= 3.11` — this is by design and matches CONTEXT D-PLUGIN-03 (Python 3.11+ for sidecar samples).

## Deviations from Plan

### Auto-fixed issues

1. **[Rule 1 — bug] `nvml-wrapper = { ..., optional = true }` rejected by cargo at workspace level.**
   - Found during: Task 2 build verification.
   - Issue: cargo errors with "nvml-wrapper is optional, but workspace dependencies cannot be optional."
   - Fix: removed `optional = true` from the workspace pin. Plan 02-03 (`rollout-cloud-local`) will mark `nvml-wrapper` optional at the crate level under a `linux-gpu` feature.
   - Files modified: `Cargo.toml`.
   - Commit: 5e15893 (Task 2).

2. **[Rule 1 — bug] `smol_str = "0.3"` pulled in 0.3.6 which requires Rust 1.89.**
   - Found during: Task 1 build verification.
   - Issue: `error: rustc 1.88.0 is not supported by the following package: smol_str@0.3.6 requires rustc 1.89`. Workspace `rust-toolchain.toml` pins 1.88.0; 0.3.5 / 0.3.4 also require 1.89; 0.3.3 is yanked.
   - Fix: pinned to `smol_str = "=0.3.2"` (exact version with serde feature) — the highest 0.3.x patch that doesn't bump MSRV.
   - Files modified: `Cargo.toml`.
   - Commit: 91c9733 (Task 1) updated to use `=0.3.2`.

3. **[Rule 2 — missing critical functionality] Each stub `Cargo.toml` was missing `[lints] workspace = true`.**
   - Found during: drafting the six stubs.
   - Issue: without `[lints] workspace = true`, the workspace-wide `missing_docs = "warn"` lint wouldn't apply uniformly to the stub crates, risking a DOCS-03 rustdoc-gate failure when those crates grow real code in downstream plans.
   - Fix: added `[lints] workspace = true` to every stub Cargo.toml.
   - Files modified: 6 × `crates/rollout-<name>/Cargo.toml`.

4. **[Rule 2 — missing critical functionality] Plan instruction implied a wildcard `rollout-core = { path = "../rollout-core" }` dep on each stub.**
   - Found during: `cargo deny check` after Task 2.
   - Issue: cargo-deny flagged 6 `wildcard` errors (`allow-wildcard-paths` does not apply to public crates per crates.io rules).
   - Fix: appended `version = "0.1"` to each stub's `rollout-core` dep — matches the Phase-1 pattern from `rollout-cli`.
   - Files modified: 6 × `crates/rollout-<name>/Cargo.toml`.
   - Commit: 5e15893 (Task 2).

5. **[Rule 1 — bug] `cargo clippy --workspace -- -D warnings` rejected three doc_markdown issues.**
   - Found during: Task 1 + Task 2 clippy runs.
   - Issue: `clippy::doc_markdown` (in pedantic group) wants `PyO3`, `EventEmitter`, and `snake_case` in backticks.
   - Fix: added backticks in three doc strings (`traits/mod.rs`, `traits/observability.rs`, `traits/plugin.rs`, `rollout-plugin-host/src/lib.rs`).
   - Files modified: as listed.

6. **[Rule 1 — bug] `tests/trait_surface.rs` initial complex `fn`-pointer signatures triggered E0106 lifetime errors.**
   - Found during: Task 1 test build.
   - Issue: function-pointer types over `&dyn Trait` closures need explicit lifetime introduction; the for<'a>-elaboration triggered E0106 on the second-argument lifetimes.
   - Fix: replaced the function-pointer style with plain `fn _shape(...)` helpers — same compile-shape coverage, no lifetime gymnastics. Added three `#![allow(clippy::...)]` attributes (`let_underscore_future`, `too_many_arguments`, `needless_pass_by_value`) because the test pattern legitimately violates those lints without a runtime concern.
   - Files modified: `crates/rollout-core/tests/trait_surface.rs`.

### Rule-4 (architectural) deviations

None. All changes stayed within the trait-extension scope sanctioned by RESEARCH.md §"Critical Finding: Trait Surface Drift".

### Open questions surfaced for Wave 1

- **`PluginDependencies` is a `#[derive(Default)] Debug` struct with no fields in Phase 2.** Plan 02-05 (rollout-plugin-host) needs to decide whether the in-tree cdylib sample receives a populated `PluginDependencies` at `init()` or whether that injection deferred to Phase 7 (when the tool harness lands and untrusted-code isolation matters).
- **`scan_bytes` returns `Vec`, not a stream.** Plan 02-02 (rollout-storage) should validate this is acceptable for the in-process watch/scan hot paths; if downstream plans run into memory pressure on large scans, plan 02-07 may need to introduce a `StorageStream` newtype.
- **`EventEmitter::emit` takes `Event` by value.** Plan 02-06's `StdoutJsonEmitter` will need a `Mutex<BufWriter<Stdout>>` to serialize concurrent emits without dropping ordering — flag for that plan's RESEARCH pass.

## Commits

| Task | Hash    | Subject                                                                       |
| ---- | ------- | ----------------------------------------------------------------------------- |
| 1    | 91c9733 | feat(02-00): extend rollout-core traits to Phase-2 spec surface               |
| 2    | 5e15893 | feat(02-00): register Phase-2 crate stubs + workspace deps + preflight        |

## Self-Check: PASSED

- crates/rollout-core/src/traits/observability.rs — FOUND
- crates/rollout-core/tests/fixtures/violation_transport_cloud/Cargo.toml — FOUND
- crates/rollout-core/tests/fixtures/violation_plugin_host_transport/Cargo.toml — FOUND
- 6 × crates/rollout-<name>/Cargo.toml — FOUND
- scripts/preflight.sh — FOUND (executable bit set)
- docs/book/src/substrate/index.md — FOUND
- Commits 91c9733 + 5e15893 — both present in `git log --oneline -5`
