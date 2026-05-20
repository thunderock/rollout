---
phase: 02-local-substrate
plan: 03
subsystem: substrate-cloud-local
tags: [rollout-cloud-local, object-store, queue, secret-store, compute-hint, fs-sharded, queue-replay, sysinfo, nvml, mdbook]

# Dependency graph
requires:
  - phase: 02-local-substrate
    provides: rollout-cloud-local stub + Wave-0 trait surface (ObjectStore content-addressed + Queue ack/nack + SecretStore + ComputeHint inventory) + sysinfo/nvml-wrapper workspace pins (plan 02-00); EmbeddedStorage backing the queue spill (plan 02-02)
provides:
  - "FsObjectStore — content-addressed two-level sharded FS impl of rollout_core::ObjectStore under <root>/<hex[0..2]>/<hex[2..4]>/<hex> with sibling <hex>.meta.json"
  - "InMemQueue — tokio::sync::Mutex<VecDeque<_>> hot path mirrored to Storage namespace cloudlocal_queue with full restart replay (open() scans + sorts by ULID + repopulates deque)"
  - "EnvSecretStore — read-only ROLLOUT_SECRET_<NAME> reader filtered through config-defined allowlist; put() always returns Fatal(ConfigInvalid)"
  - "LinuxComputeHint — /proc/cpuinfo + /proc/meminfo parsing with sysinfo fallback; optional NVML feature for GPU inventory (degrades to empty Vec on missing libnvml — never fails); instance_type from DMI product_name"
  - "MacosComputeHint — sysinfo cpu_count + total_memory; empty gpus; preemption_signal None"
  - "hints::for_current_platform() — Box<dyn ComputeHint> selector with compile_error! on unsupported targets"
  - "CloudLocalConfig — object_store_root + secret_allowlist (deny_unknown_fields + JsonSchema)"
  - "docs/book/src/substrate/cloud-local.md substrate chapter wired into SUMMARY.md"
affects: [02-06-rollout-coordinator, 02-07-smoke-and-docs]

# Tech tracking
tech-stack:
  added:
    - "rollout-cloud-local now depends on: rollout-core, rollout-storage, async-trait, serde, serde_json, schemars, thiserror, tracing, tokio, blake3, postcard, hex, ulid, sysinfo (0.33, system feature), smol_str (=0.3.2), nvml-wrapper (0.11, optional under `nvml` feature) — all via workspace = true"
    - "dev-deps: tempfile, tokio macros+rt-multi-thread"
  patterns:
    - "Content-addressed FS storage: ContentId::of(bytes) -> blake3 hex; path = root/hex[0..2]/hex[2..4]/hex; meta sidecar = <hex>.meta.json. tmp-then-rename for atomicity; idempotent on duplicate puts."
    - "Queue restart replay via Storage namespace partition: write-through on enqueue (txn.put_bytes inside Storage txn), Storage scan on open(), ULID lex sort recovers enqueue order, nack re-pushes to deque-front but leaves Storage entry intact so subsequent restart still replays."
    - "Three-way secret semantics: outside allowlist -> Fatal(ConfigInvalid); allowed-but-unset -> Recoverable(Transient, RetryHint::Never); put() always Fatal(ConfigInvalid) (read-only by design)."
    - "Compute-hint platform split via #[cfg(target_os = ...)] submodules; for_current_platform() returns Box<dyn ComputeHint>; tests gated by the same cfg so workspace cargo test --tests stays green on every host."
    - "Optional nvml feature with `[package.metadata.cargo-machete] ignored = [\"nvml-wrapper\"]` so the unused-deps CI job doesn't flag the gated dep."

key-files:
  created:
    - "crates/rollout-cloud-local/src/config.rs — CloudLocalConfig (object_store_root + secret_allowlist)"
    - "crates/rollout-cloud-local/src/object_store.rs — FsObjectStore + impl ObjectStore"
    - "crates/rollout-cloud-local/src/secrets.rs — EnvSecretStore + impl SecretStore"
    - "crates/rollout-cloud-local/src/queue.rs — InMemQueue + impl Queue with Storage spill"
    - "crates/rollout-cloud-local/src/hints/mod.rs — module wiring + for_current_platform()"
    - "crates/rollout-cloud-local/src/hints/linux.rs — LinuxComputeHint (#[cfg(target_os = \"linux\")])"
    - "crates/rollout-cloud-local/src/hints/macos.rs — MacosComputeHint (#[cfg(target_os = \"macos\")])"
    - "crates/rollout-cloud-local/tests/object_store.rs — 6 tests (round-trip, sharded layout, meta sidecar, exists, idempotency, missing -> fatal)"
    - "crates/rollout-cloud-local/tests/secrets.rs — 4 tests (allowlist read, outside-allowlist fatal, unset transient, put fatal)"
    - "crates/rollout-cloud-local/tests/queue_replay.rs — 5 tests (FIFO ULID order, nack-to-front, restart replay, ack removes from Storage, nack keeps in Storage)"
    - "crates/rollout-cloud-local/tests/hints_linux.rs — 3 tests gated #[cfg(linux)]"
    - "crates/rollout-cloud-local/tests/hints_macos.rs — 2 tests gated #[cfg(macos)]"
    - "docs/book/src/substrate/cloud-local.md — substrate chapter (~95 lines)"
  modified:
    - "crates/rollout-cloud-local/Cargo.toml — full dep set + `nvml` feature + cargo-machete ignore for nvml-wrapper"
    - "crates/rollout-cloud-local/src/lib.rs — crate-level //! doc + module wiring + re-exports (FsObjectStore, InMemQueue, EnvSecretStore, CloudLocalConfig)"
    - "docs/book/src/SUMMARY.md — nests [Cloud-local](./substrate/cloud-local.md) under Substrate"
    - "Cargo.lock — refreshed for sysinfo 0.33 + nvml-wrapper 0.11 + transitives"

key-decisions:
  - "[D-LOCAL-01] FsObjectStore: idempotent on duplicate-content puts (skips the blob write when the target file already exists). Meta sidecar is rewritten every put so the most recent content-type/created_at_ms wins — caller-friendly without leaking stale metadata."
  - "[D-LOCAL-01] Missing get_bytes() maps to Fatal(Internal(\"object not found: <hex>\")) NOT Recoverable. Rationale: a missing ContentId signals an upstream contract violation (caller asked for a hash that was never written) — retrying won't change the outcome. Documented inline in tests/object_store.rs."
  - "[D-LOCAL-02] Queue restart-replay order is reconstructed from ULID lex sort, not from a separate insertion-order column. ULIDs are k-sortable by construction so this recovers FIFO order across processes without extra state."
  - "[D-LOCAL-02] dequeue() pops only from the in-mem deque (does NOT delete from Storage). ack() commits the Storage delete; nack() leaves Storage untouched so a crash-between-dequeue-and-ack still replays correctly on next open(). This trades a tiny window of duplicate-delivery for crash safety — acceptable for at-least-once semantics."
  - "[D-LOCAL-03] Three-way semantics for SecretStore::get: outside allowlist => Fatal(ConfigInvalid); allowed-but-unset => Recoverable(Transient, RetryHint::Never); put() always Fatal(ConfigInvalid). The Recoverable choice for unset-allowed lets callers retry after an operator provisions the env var without restarting; the `RetryHint::Never` makes clear the caller should not auto-poll."
  - "[D-LOCAL-04] NVML feature default OFF (per CONTEXT + RESEARCH Pitfall 10). When ON, init failures degrade to empty gpus rather than erroring — never fails. cargo-machete ignore entry prevents CI's unused-deps job from flagging the optional dep."
  - "[D-LOCAL-04] Linux instance_type heuristic: read /sys/devices/virtual/dmi/id/product_name; trim + drop empty. Returns None on macOS-style hosts or LXC sandboxes where DMI is unavailable. Cloud-specific instance_type (EC2/GCE metadata) is Phase 5."
  - "[Claude] hints/for_current_platform() uses compile_error! on platforms outside Linux+macOS rather than runtime-erroring — gives a clearer signal at build time and matches the Phase-2-supports-Linux+macOS-only stance documented in the substrate landing page."
  - "[Claude] Tests use unique env-var suffixes per test (FOO_TEST1, FOO_TEST2, …) instead of pulling serial_test as a dev-dep. RESEARCH didn't list serial_test; per-test naming sidesteps cross-test env-mutation races without adding dependency surface."
  - "[Rule 1 - bug fix] Plan snippet's `RetryHint::after_seconds(0)` doesn't exist on the Phase-1 RetryHint enum (variants are Never, After(Duration), Backoff { base, max }). Used `RetryHint::Never` for the unset-allowed-secret case — matches the 'operator action needed, do not auto-retry' semantics the plan described."
  - "[Rule 1 - bug fix] FatalError variants are struct-form ({msg: String}) per Wave-0; plan snippets used the tuple form `FatalError::Internal(string)`. Fixed in object_store.rs and secrets.rs."
  - "[Rule 1 - bug fix] `std::env::set_var` inside test code does not require an `unsafe` block on Rust 1.88 edition 2021 (the 2024-edition deprecation hasn't landed for this toolchain). Removed the `unsafe { … }` wrapping the plan suggested — the workspace `#![forbid(unsafe_code)]` lint rejected it."

patterns-established:
  - "Crate-level optional dependency + cargo-machete ignore: re-introduce `optional = true` at the consumer site (rollout-cloud-local) under a feature flag (`nvml = [\"dep:nvml-wrapper\"]`), and add `[package.metadata.cargo-machete] ignored = [\"nvml-wrapper\"]` so the CI unused-deps job doesn't flag the gated dep."
  - "Restart-replay via Storage namespace partition: app-level Queue writes payloads through a single namespace (cloudlocal_queue here), uses ULID strings as path segments, scans on startup to rebuild in-memory state. Storage entry deletion is the ack semantic."
  - "Platform-gated submodules + parallel-gated tests: each platform-specific submodule guarded by #[cfg(target_os = \"…\")]; corresponding integration test file uses #![cfg(target_os = \"…\")] crate attribute so it compiles to an empty test binary on other hosts (workspace cargo test --tests stays green everywhere)."

deviations:
  - "[Rule 1 - bug] Plan snippet referenced `RetryHint::after_seconds(0)` which is not on the actual RetryHint enum. Substituted RetryHint::Never (the variant that matches the documented 'operator action needed, do not auto-retry' intent). Surfaced during initial secret-store implementation."
  - "[Rule 1 - bug] Plan snippet used tuple-form FatalError variants (`FatalError::Internal(\"…\")`); Wave-0 ships struct-form (`FatalError::Internal { msg }`). Fixed throughout object_store.rs + secrets.rs — same bug 02-02 already documented."
  - "[Rule 1 - bug] Plan suggested wrapping `std::env::set_var` in `unsafe { … }` inside test code. Rust 1.88 edition 2021 does not require unsafe for set_var, AND the workspace `lints.rust.unsafe_code = forbid` lint rejected the unsafe block. Removed the unsafe wrapper; tests compile and pass cleanly."
  - "[Rule 2 - missing critical functionality] Plan referred to a Cargo `[features]` block `nvml = [\"dep:nvml-wrapper\"]` and a `[package.metadata.cargo-machete] ignored` entry; the existing stub Cargo.toml had neither. Added both verbatim from the plan's Step 1 — RESEARCH Pitfall 10 demands the cargo-machete entry."

# Known stubs (none for this plan)
known_stubs: []

# Authentication gates / preflight notes
preflight_note: "None. cargo test -p rollout-cloud-local --tests runs hermetically; each test uses tempfile::TempDir for its own FS root or redb file, and EnvSecretStore tests use unique env-var suffixes per test. No system services required. The nvml feature requires libnvidia-ml at runtime to actually enumerate GPUs, but the feature is OFF by default and degrades cleanly to empty inventory when the library is absent."

requirements-completed: [SUBSTR-04, DOCS-02, DOCS-03]

# Metrics
duration: 7min
completed: 2026-05-20
---

# Phase 2 Plan 03: rollout-cloud-local Crate Summary

**One-liner:** Shipped the four Layer-1 substrate impls — `FsObjectStore` (content-addressed two-level sharded FS), `InMemQueue` (RAM hot path + Storage spill with full restart replay via ULID-sorted scan of `cloudlocal_queue/*`), `EnvSecretStore` (read-only `ROLLOUT_SECRET_<NAME>` allowlist with three-way fatal/transient/fatal-write semantics), and `ComputeHint` (Linux `/proc` + optional NVML feature; macOS `sysinfo` stub) — so the rest of the stack has a working ObjectStore/Queue/SecretStore/ComputeHint to target with zero cloud creds, gated by `cargo build/test/clippy/doc -p rollout-cloud-local --all-features` + workspace-wide `cargo test --workspace --tests` regression + `cargo deny check` + `mdbook build docs/book`.

## What landed

### Task 1 — FsObjectStore + EnvSecretStore + crate scaffolding

`crates/rollout-cloud-local/Cargo.toml` extended from Wave-0 stub to the full dep set (rollout-core + rollout-storage + async-trait + serde/serde_json + schemars + thiserror + tracing + tokio + blake3 + postcard + hex + ulid + sysinfo + smol_str + nvml-wrapper-optional-under-`nvml`-feature). `[package.metadata.cargo-machete] ignored = ["nvml-wrapper"]` keeps the CI unused-deps job quiet.

`src/config.rs` ships `CloudLocalConfig { object_store_root: PathBuf, secret_allowlist: Vec<String> }` with `#[serde(deny_unknown_fields)]` and `JsonSchema`. Default object store root is `./data/object-store`.

`src/object_store.rs` ships `FsObjectStore` with the two-level sharded layout (`<root>/<hex[0..2]>/<hex[2..4]>/<hex>`). `put_bytes` is idempotent on duplicate content (skips the blob write when the file exists; meta sidecar is always rewritten for the latest content-type+timestamp). Atomicity via tmp-then-rename. `get_bytes` on missing returns `Fatal(Internal("object not found: <hex>"))` — chosen because a missing `ContentId` indicates an upstream contract violation, not a transient I/O fault.

`src/secrets.rs` ships `EnvSecretStore` with three-way semantics:

| Condition                                       | Result                                          |
| ----------------------------------------------- | ----------------------------------------------- |
| `name` outside allowlist                        | `Err(Fatal(ConfigInvalid("…allowlist")))`       |
| `name` allowed, `ROLLOUT_SECRET_<name>` set     | `Ok(value)`                                     |
| `name` allowed, env var unset                   | `Err(Recoverable(Transient, RetryHint::Never))` |
| `put(name, value)` — always                     | `Err(Fatal(ConfigInvalid("…read-only")))`       |

`src/queue.rs` and `src/hints/mod.rs` ship as Task-1 stubs (private `_task2_stub` fn) so the lib.rs compiles; Task 2 fills them.

Tests: `tests/object_store.rs` (6) + `tests/secrets.rs` (4). All green. Clippy `-D warnings` clean. Tests use unique env-var suffixes (`FOO_TEST1`, `FOO_TEST2`, …) per test so no cross-test env-mutation races — avoids pulling `serial_test` as a dev-dep.

### Task 2 — InMemQueue restart replay + ComputeHint (Linux + macOS) + mdBook

`src/queue.rs` ships `InMemQueue { inner: Mutex<VecDeque<…>>, storage: Arc<dyn Storage> }`. Each `enqueue(payload)`:

1. Mint a ULID-based `QueueItemId`.
2. `storage.begin() -> txn.put_bytes(cloudlocal_queue/<ulid>, payload) -> txn.commit()`.
3. Push `(id, payload)` onto the in-mem deque.

`dequeue()` pops from the in-mem deque only (does NOT touch Storage). `ack(id)` deletes the Storage entry; `nack(id)` re-reads the payload from Storage and pushes to the deque front. This means a crash between dequeue and ack leaves the Storage entry intact, so the next `open()` will replay it — at-least-once semantics with a tiny duplicate-delivery window.

`open(storage)` scans `cloudlocal_queue/*` via `storage.scan_bytes`, parses each path segment as a ULID, and sorts the entries by ULID. ULIDs are k-sortable by construction, so this recovers FIFO enqueue order across restarts without an extra insertion-order column.

`src/hints/mod.rs` declares `for_current_platform() -> Box<dyn ComputeHint>` with platform-gated submodules. Compile-error on platforms outside Linux + macOS in Phase 2.

`src/hints/linux.rs` (`#[cfg(target_os = "linux")]`) parses `/proc/cpuinfo` (count `processor :` lines) and `/proc/meminfo` (`MemTotal: NNN kB`) with `sysinfo` fallback when those are unreadable. `instance_type` reads `/sys/devices/virtual/dmi/id/product_name`. GPU inventory is behind the `nvml` Cargo feature; default-off; if NVML init or `device_count` fails at runtime, returns empty `gpus` rather than erroring. `preemption_signal` returns `Ok(None)` — Phase 5 cloud impls hook real spot signals.

`src/hints/macos.rs` (`#[cfg(target_os = "macos")]`) returns sysinfo-derived `cpu_count + memory_mib`, empty `gpus`, `instance_type = None`, `preemption_signal = None`.

Tests: `tests/queue_replay.rs` (5) — `enqueue_dequeue_basic`, `nack_returns_to_front`, **`restart_replays_unacked_items`** (the load-bearing test: open storage, enqueue 3, dequeue 2 without ack, drop, reopen, expect 3 items back in ULID order), `ack_removes_from_storage`, `nack_keeps_in_storage_returns_to_queue`. `tests/hints_macos.rs` (2, `#[cfg(target_os = "macos")]` — runs on this macOS dev host). `tests/hints_linux.rs` (3, `#[cfg(target_os = "linux")]` — compile-to-empty on macOS; runs on Linux CI).

`docs/book/src/substrate/cloud-local.md` (~95 lines) covers what ships, what's deferred, FS layout diagram, queue restart semantics, secret allowlist matrix, GPU inventory feature gating, and the test table. Nested under Substrate in `docs/book/src/SUMMARY.md` (preserves the reserved Examples placeholder).

## End-to-end verification

All commands exit 0 on the macOS dev host:

```
cargo build  -p rollout-cloud-local --all-features
cargo test   -p rollout-cloud-local --tests          # 17 pass on macOS (object_store 6 + secrets 4 + queue_replay 5 + hints_macos 2)
cargo clippy -p rollout-cloud-local --all-targets --all-features -- -D warnings
cargo fmt    -p rollout-cloud-local -- --check
RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" \
  cargo doc -p rollout-cloud-local --no-deps --all-features
mdbook build docs/book
cargo deny check                                     # advisories ok, bans ok, licenses ok, sources ok
cargo test  --workspace --tests                      # full workspace green (no regressions)
```

On a Linux CI host, the three `tests/hints_linux.rs` tests additionally run; the `linux_gpu_inventory_via_nvml_when_available` test is `#[cfg(feature = "nvml")] #[ignore]`d so it only runs with `--ignored` on a GPU host.

## Deviations from Plan

### Rule-1 (auto-fix bug)

1. **`RetryHint::after_seconds(0)` does not exist on the Phase-1 RetryHint enum.** Plan snippet used this constructor; actual variants are `Never`, `After(Duration)`, `Backoff { base, max }`. Substituted `RetryHint::Never` for the unset-allowed-secret case — matches the documented intent ("operator action needed, do not auto-retry").

2. **FatalError variants are struct-form, not tuple-form.** Plan snippets used `FatalError::Internal("…")` and `FatalError::ConfigInvalid("…")` (Phase-1 shape); Wave-0 ships `{ msg }` struct variants. Same bug 02-02 already documented; fixed in `object_store.rs` and `secrets.rs`.

3. **`std::env::set_var` does NOT require `unsafe` on Rust 1.88 edition 2021.** Plan suggested wrapping the call in `unsafe { … }`; the workspace `lints.rust.unsafe_code = forbid` lint rejected it, AND the 2024-edition deprecation that motivates the wrapping hasn't landed for this toolchain. Removed the unsafe wrapper from `tests/secrets.rs`; tests compile + pass cleanly.

### Rule-2 (auto-add missing critical functionality)

1. **Cargo features block + cargo-machete ignore for `nvml-wrapper`.** Wave-0 stub Cargo.toml had neither. Added the `[features] nvml = ["dep:nvml-wrapper"]` block AND `[package.metadata.cargo-machete] ignored = ["nvml-wrapper"]` per RESEARCH Pitfall 10 — without the ignore entry the CI unused-deps job would have flagged the optional dep when its feature is off.

### Rule-4 (architectural)

None. All changes stayed within `crates/rollout-cloud-local/`; no spec edits, no trait surface changes, no workspace-wide config changes.

## Open Questions for Downstream Plans

- **Plan 02-06 (`rollout-coordinator`):** Does the coordinator need an `ObjectStore` handle in Phase 2? Likely no — the Phase-2 coordinator slice is register-worker + accept-heartbeat + deadline-scan, none of which write blobs. Defer to Phase 5 (cloud snapshot storage). If a future Phase-2 task does need it, `FsObjectStore::open(config.object_store_root)` is a one-liner.
- **Plan 02-07 (smoke + docs + CI):** The smoke test's worker config probably doesn't enable the `nvml` feature (no GPU on macOS / ubuntu-latest); confirm. If a Linux CI runner ever has libnvidia-ml installed, the `linux_gpu_inventory_via_nvml_when_available` `#[ignore]`d test can be opted in with `cargo test --features nvml -- --ignored`.
- **Plan 02-04 (`rollout-transport`):** No interaction with this crate (dep-direction lint enforces it). The `ComputeHint` may eventually feed the transport's listen-addr selection (e.g., bind to the GPU NUMA node) but that's Phase 5+.
- **At-least-once vs at-most-once Queue semantics:** The current `dequeue() -> drop -> reopen` flow has a tiny duplicate-delivery window (between dequeue and ack a crash will replay the item). Acceptable for the spirit-of-DIST-03 goal. If callers need at-most-once they need a lease + visibility-timeout pattern, which lands in Phase 6 with the real distribution work.

## Commits

| Task | Hash    | Subject                                                                       |
| ---- | ------- | ----------------------------------------------------------------------------- |
| 1    | 4b51a16 | feat(02-03): FsObjectStore + EnvSecretStore + cloud-local scaffolding         |
| 2    | 0a5f88e | feat(02-03): InMemQueue restart replay + ComputeHint (linux+macos) + mdBook   |

## Self-Check: PASSED

- `crates/rollout-cloud-local/src/config.rs` — FOUND
- `crates/rollout-cloud-local/src/object_store.rs` — FOUND (`pub struct FsObjectStore` + `impl ObjectStore`)
- `crates/rollout-cloud-local/src/secrets.rs` — FOUND (`pub struct EnvSecretStore` + `impl SecretStore`)
- `crates/rollout-cloud-local/src/queue.rs` — FOUND (`pub struct InMemQueue` + `impl Queue`)
- `crates/rollout-cloud-local/src/hints/mod.rs` — FOUND (`for_current_platform`)
- `crates/rollout-cloud-local/src/hints/linux.rs` — FOUND (`pub struct LinuxComputeHint`)
- `crates/rollout-cloud-local/src/hints/macos.rs` — FOUND (`pub struct MacosComputeHint`)
- `crates/rollout-cloud-local/tests/{object_store,secrets,queue_replay,hints_linux,hints_macos}.rs` — all FOUND
- `docs/book/src/substrate/cloud-local.md` — FOUND
- `docs/book/src/SUMMARY.md` — FOUND (`[Cloud-local](./substrate/cloud-local.md)` nested under Substrate)
- Commit `4b51a16` — FOUND in `git log --oneline -5`
- Commit `0a5f88e` — FOUND in `git log --oneline -5`
