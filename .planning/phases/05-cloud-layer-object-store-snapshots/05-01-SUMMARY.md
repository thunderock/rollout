---
phase: 05-cloud-layer-object-store-snapshots
plan: 01
subsystem: database
tags: [postgres, redb, sqlx, storage, proptest, testcontainers, validation]

# Dependency graph
requires:
  - phase: 04-train-sft-rm-snapshots
    provides: "PostgresStorage backend + testcontainers integration-test harness (#[ignore] D-PG-04 pattern)"
  - phase: 01-core-foundations
    provides: "StorageKey type + CoreError/FatalError::ConfigInvalid taxonomy"
provides:
  - "StorageKey::validate_for_postgres() — printable-ASCII (0x20-0x7E) path guard returning Fatal(ConfigInvalid)"
  - "Every Postgres CRUD/scan entry-point validates the key before SQL (get/get_many/scan/put/delete/cas)"
  - "256-case redb↔Postgres scan_bytes parity proptest wired into the postgres-integration CI job"
  - "storage.md note documenting the constraint + hex::encode escape hatch for binary IDs"
affects: [06-multi-node-distribution, phase-6-work-namespaces, queue_items, epoch]

# Tech tracking
tech-stack:
  added: [proptest (rollout-storage dev-dep), hex (rollout-core + rollout-storage dev-dep)]
  patterns:
    - "Validity guard at the trait boundary: reject un-round-trippable keys at every backend entry-point, no SQL change"
    - "Cross-backend parity proptest with one shared container + per-case bucket isolation"

key-files:
  created:
    - crates/rollout-storage/tests/postgres_scan_bytes_parity.rs
  modified:
    - crates/rollout-core/src/traits/storage.rs
    - crates/rollout-storage/src/postgres/mod.rs
    - crates/rollout-storage/tests/postgres_integration.rs
    - docs/book/src/substrate/storage.md
    - .github/workflows/ci.yml
    - crates/rollout-core/Cargo.toml
    - crates/rollout-storage/Cargo.toml

key-decisions:
  - "Approach 1 (validity guard + hex-encoding) — no schema migration; SQL untouched so no .sqlx cache regeneration needed"
  - "namespace stays SmolStr (validated UTF-8 by type); only path components are byte-checked"
  - "Parity proptest fixes namespace to a registered one (snapshots) because embedded table_for rejects unknown namespaces"
  - "Sort scan results via a projected tuple key — StorageKey deliberately not made Ord to avoid widening the core API"

patterns-established:
  - "Pattern: every Postgres Storage/StorageTxn method calls key.validate_for_postgres()? as its first line"
  - "Pattern: cross-backend parity proptests reuse a single testcontainer and isolate cases by a random bucket path segment"

requirements-completed: [DOCS-01, DOCS-02, DOCS-03]

# Metrics
duration: ~25min
completed: 2026-05-28
---

# Phase 5 Plan 01: Precursor A — Postgres scan_bytes Validity Guard Summary

**`StorageKey::validate_for_postgres` rejects non-printable-ASCII path bytes at every Postgres CRUD/scan entry-point, closing the v1.0 latent `scan_bytes` wildcard-parity divergence (PITFALLS.md §17) with a 256-case redb↔Postgres proptest witness — no schema migration.**

## Performance

- **Duration:** ~25 min
- **Started:** 2026-05-28
- **Completed:** 2026-05-28
- **Tasks:** 3
- **Files modified:** 7 (1 created, 6 modified)

## Accomplishments
- `StorageKey::validate_for_postgres()` guard on the core type with 6 unit tests + rustdoc that instructs hex-encoding binary IDs.
- All six Postgres entry-points (`get_bytes`, `get_many_bytes`, `scan_bytes`, `put_bytes`, `delete`, `cas_bytes`) validate the key before any SQL runs.
- 256-case proptest proving byte-identical `scan_bytes` results between redb and Postgres over printable-ASCII inputs, wired into the `postgres-integration` CI job.
- mdBook `substrate/storage.md` documents the ASCII-printable path constraint and the `hex::encode` escape hatch.

## Task Commits

Each task was committed atomically:

1. **Task 1: Add `StorageKey::validate_for_postgres` validity guard** - `bba1d50` (feat)
2. **Task 2: Wire `validate_for_postgres` into Postgres backend CRUD + scan** - `99795bd` (feat)
3. **Task 3: Proptest parity witness — redb vs Postgres scan_bytes** - `ffc30b0` (test)

_TDD note: tests and implementation landed together per task because the new method/tests live in the same files; each task's tests were run green before commit._

## Files Created/Modified
- `crates/rollout-core/src/traits/storage.rs` - Added `impl StorageKey { validate_for_postgres }` + struct-level Postgres-constraint rustdoc + 6 unit tests.
- `crates/rollout-storage/src/postgres/mod.rs` - `key.validate_for_postgres()?` as the first line of all six CRUD/scan methods.
- `crates/rollout-storage/tests/postgres_integration.rs` - 3 new `#[ignore]` integration tests (reject prefix, reject put + no-row, ASCII round-trip).
- `crates/rollout-storage/tests/postgres_scan_bytes_parity.rs` - New 256-case redb↔Postgres parity proptest (shared container, per-case bucket isolation).
- `docs/book/src/substrate/storage.md` - Postgres path-constraint section (ASCII-printable + `hex::encode`).
- `.github/workflows/ci.yml` - New `postgres-integration` step running the parity proptest with `--include-ignored`.
- `crates/rollout-core/Cargo.toml`, `crates/rollout-storage/Cargo.toml` - `hex` + `proptest` dev-deps.

## Decisions Made
- **Approach 1 over a schema migration:** SQL strings are untouched; the guard ensures only ASCII-printable bytes reach `path[1:array_length($3,1)] = $3`, so there is nothing to migrate.
- **No `.sqlx/` regeneration:** the crate runs runtime-checked `sqlx::query` (not the `query!` macro) and has no offline cache; `SQLX_OFFLINE=true cargo check` passes unchanged.
- **`StorageKey` not made `Ord`:** the parity test sorts via a projected `(namespace, run_id_bytes, path_strings, value)` tuple to avoid widening the core type's API surface.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Adapted to real `StorageKey` / error shapes**
- **Found during:** Task 1
- **Issue:** Plan sketch assumed `path: Vec<String>`, `namespace: &'static str`, and `FatalError::ConfigInvalid(String)` (tuple). The real types are `path: Vec<SmolStr>`, `namespace: SmolStr`, and `ConfigInvalid { msg: String }` (struct variant).
- **Fix:** Implemented the guard against the real types; iterate `SmolStr::as_bytes()`; construct the struct-variant error. Behavior identical to the plan's intent.
- **Files modified:** crates/rollout-core/src/traits/storage.rs
- **Verification:** 6 unit tests pass; rustdoc clean with deny flags.
- **Committed in:** `bba1d50`

**2. [Rule 3 - Blocking] Added missing dev-dependencies**
- **Found during:** Task 1 & Task 3
- **Issue:** `hex` was not a dev-dep of `rollout-core`; neither `hex` nor `proptest` were dev-deps of `rollout-storage` (plan assumed Phase 4 added them).
- **Fix:** Added `hex` (both crates) and `proptest` (rollout-storage) from existing workspace pins.
- **Files modified:** crates/rollout-core/Cargo.toml, crates/rollout-storage/Cargo.toml, Cargo.lock
- **Verification:** Tests compile and pass.
- **Committed in:** `bba1d50`, `ffc30b0`

**3. [Rule 1 - Bug] Parity proptest namespace must be backend-registered**
- **Found during:** Task 3
- **Issue:** Plan sketch generated random `namespace in "[a-z]{3,8}"`, but the embedded backend's `table_for` rejects any namespace not in its fixed set, so every case would fail on the redb side.
- **Fix:** Fixed the namespace to `snapshots` (registered in both backends) and isolated cases with a random `bucket` path segment instead.
- **Files modified:** crates/rollout-storage/tests/postgres_scan_bytes_parity.rs
- **Verification:** Test compiles; `#[ignore]` so default `cargo test` stays green; will run in CI on a Docker runner.
- **Committed in:** `ffc30b0`

**4. [Rule 3 - Blocking] Real backend API (no `txn_owned` / `connect` / `migrate`)**
- **Found during:** Task 3
- **Issue:** Plan sketch used `txn_owned(...)`, `PostgresStorage::connect`, `pg.migrate()`, and `EmbeddedStorage::open(dir).unwrap()` (sync). The real API uses `begin()`-based txns, `PostgresStorage::new(url, pool)` (migrations inside `new`), and async `EmbeddedStorage::open(path)`.
- **Fix:** Rewrote the proptest against the real API: shared Tokio runtime + one container started once via `OnceLock`, writes via `begin()/put_bytes/commit`, readiness retry loop matching the Phase-4 pattern.
- **Files modified:** crates/rollout-storage/tests/postgres_scan_bytes_parity.rs
- **Verification:** Compiles with `--no-run`; `#[ignore]` skip confirmed locally.
- **Committed in:** `ffc30b0`

---

**Total deviations:** 4 auto-fixed (3 blocking, 1 bug). All driven by the plan sketch being written against assumed APIs that differ from the real v1.0 code. No scope creep — the load-bearing behavior (validity guard + parity assertion) is exactly as specified.

**Note on plan `files_modified`:** the plan listed `crates/rollout-storage/.sqlx/` — that directory does not exist (this crate uses runtime-checked queries) and no offline cache was needed because no SQL changed. `crates/rollout-core/src/traits/mod.rs` was listed for Task 1 but needed no edit (`pub use ...::StorageKey` already present).

## Issues Encountered
- `StorageKey` is not `Ord`, so the proptest's `.sort()` did not compile — resolved by sorting on a projected comparable tuple.
- Docker is unavailable on the dev machine, so the 3 integration tests + parity proptest could not be executed locally; they compile clean (`--no-run`) and are `#[ignore]`'d per the established Phase-4 D-PG-04 pattern, running in the `postgres-integration` CI job.

## User Setup Required
None - no external service configuration required. The Docker-gated tests run automatically in the `postgres-integration` CI job.

## Next Phase Readiness
- Phase 6 multi-node namespaces (`work/`, `epoch/`, `queue_items/`) can now safely use the Postgres backend: any binary ID in a path component is rejected at construction unless hex-encoded, and parity with redb is witnessed.
- Standalone, independently revertable change against `main`. No blockers.

## Self-Check: PASSED

- All 5 listed key files exist on disk (1 created, 4 modified verified).
- All 3 task commits exist in git history (`bba1d50`, `99795bd`, `ffc30b0`).

---
*Phase: 05-cloud-layer-object-store-snapshots*
*Completed: 2026-05-28*
