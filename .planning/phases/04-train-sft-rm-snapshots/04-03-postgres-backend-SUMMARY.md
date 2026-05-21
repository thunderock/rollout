---
phase: 04-train-sft-rm-snapshots
plan: 03
subsystem: storage-postgres
tags: [postgres, sqlx, pglistener, listen-notify, watch-stream, testcontainers, train-04, migrations, rollout-storage]

# Dependency graph
requires:
  - phase: 04-train-sft-rm-snapshots
    provides: "Storage::watch_stream method on trait + rollout-storage `postgres` Cargo feature scaffolding + sqlx/uuid/testcontainers workspace pins (04-00-a + 04-00-b)"
  - phase: 02-local-substrate
    provides: "Storage / StorageTxn / StorageKey / KeyRange / StorageEvent trait + types, EmbeddedStorage broadcast watch baseline"
provides:
  - "PostgresStorage impl Storage (begin / get_bytes / get_many_bytes / scan_bytes / watch=reject / watch_stream / ping)"
  - "PostgresTxn impl StorageTxn (put_bytes / delete / cas_bytes / commit / abort) via sqlx::Transaction"
  - "PgListener-backed watch_stream emitting StorageEvent::Put filtered by run_id + path prefix"
  - "database/migrations/0001_init.sql (kv + LISTEN/NOTIFY trigger; payload <= 7999 bytes per Pitfall 5)"
  - "database/migrations/0002_snapshots.sql (snapshots + events tables for plans 04-01 + observability)"
  - "testcontainers Postgres 16 integration test suite (6 tests, all #[ignore]'d to keep workspace cargo test Docker-free)"
  - "postgres-integration CI job (15th workflow job; ubuntu-latest; runs --include-ignored)"
  - "Makefile postgres-test target (docker info preflight + same invocation as CI)"
  - "docs/book/src/training/postgres-backend.md (~180 lines) covering schema, LISTEN/NOTIFY contract, trait surface, migrations, offline mode, pool sizing, testcontainers, limitations"
affects: [04-01 (snapshots metadata writer can target the snapshots table), 04-05 (TrainableBackend has a shared cross-process store available), Phase 6 (DIST-01..04 multi-node coord uses postgres-shared watch_stream)]

# Tech tracking
tech-stack:
  added:
    - "sqlx 0.8 (pool + migrate + Postgres driver) wired via runtime-checked sqlx::query (not query! macro yet)"
    - "async-stream 0.3 (workspace dep) for the PgListener loop"
    - "ulid added as optional dep on rollout-storage (already at workspace level) for RunId<->Uuid round-trip"
    - "testcontainers 0.23 + testcontainers-modules 0.11 (dev-dep; postgres feature)"
    - "tracing-subscriber dev-dep on rollout-storage (test debugging)"
  patterns:
    - "Feature-gated optional backend: `postgres` feature with `dep:sqlx/uuid/ulid/async-stream` references; default build remains sqlx-free"
    - "Migration directory points UP from the crate (`sqlx::migrate!(\"../../database/migrations\")`) so the migration set is single-source for the workspace"
    - "Postgres txn pattern: `Option<sqlx::Transaction<'static, Postgres>>` taken on commit, `&mut **tx` deref for query execution"
    - "RunId<->Uuid bridge: `Uuid::from_bytes(ulid.to_bytes())` and back; 16-byte representation is byte-identical"
    - "Pitfall 6 (testcontainers): 30-attempt × 2s retry loop on PostgresStorage::new to wait for PG readiness"
    - "Pitfall 5 (pg_notify): trigger function caps payload at 7999 via substring(payload, 1, 7999)"
    - "watch returns Fatal(PluginContract) (in-process broadcast unsupported on postgres); cross-process callers use watch_stream"
    - "Test isolation: #[ignore = \"requires Docker / testcontainers\"] on every integration test; CI opts in via --include-ignored"

key-files:
  created:
    - "crates/rollout-storage/src/postgres/mod.rs (PostgresStorage + PostgresTxn + impls)"
    - "crates/rollout-storage/src/postgres/listener.rs (pg_watch_stream + parse_payload)"
    - "crates/rollout-storage/src/postgres/migrations.rs (workflow docstring)"
    - "crates/rollout-storage/tests/postgres_integration.rs (6 #[ignore]'d integration tests)"
    - "database/migrations/0001_init.sql"
    - "database/migrations/0002_snapshots.sql"
    - "docs/book/src/training/postgres-backend.md"
    - ".sqlx/.gitkeep"
    - ".planning/phases/04-train-sft-rm-snapshots/04-03-postgres-backend-SUMMARY.md"
  modified:
    - "crates/rollout-storage/Cargo.toml (postgres feature gains dep:async-stream + dep:ulid; testcontainers + testcontainers-modules + tracing-subscriber dev-deps; docs.rs all-features = true)"
    - "crates/rollout-storage/src/lib.rs (feature-gated postgres module + PostgresStorage re-export)"
    - "Cargo.toml (workspace dep async-stream = 0.3)"
    - ".github/workflows/ci.yml (new postgres-integration job)"
    - "Makefile (postgres-test + train-smoke targets, help)"
    - "docs/book/src/SUMMARY.md (Postgres backend chapter entry — merged with parallel 04-01-02's Training section commit)"

key-decisions:
  - "Use runtime sqlx::query (not query! macro) so `cargo build -p rollout-storage --features postgres` succeeds in offline mode without a pre-populated .sqlx/ cache. The cache dir is reserved (.gitkeep) for a future switch once the SQL surface stabilizes."
  - "Storage::watch on Postgres returns Fatal(PluginContract) rather than implementing a synthetic broadcast — broadcast is fundamentally in-process. Cross-process callers MUST use watch_stream."
  - "RunId<->Uuid round-trip via raw 16-byte representation, NOT via string parsing. `Uuid::from_bytes(ulid.to_bytes())` and `Ulid::from_bytes(*uuid.as_bytes())` are exact inverses on the binary form."
  - "Two-pool design: primary write pool (caller-sized, 30s acquire, 10min idle) + dedicated 4-conn watch pool. PgListener holds a connection for the lifetime of the stream; isolating it prevents listener churn from starving writes."
  - "pg_notify payload format `<run_id_uuid_or_empty>|<path_parts_joined_by_slash>` truncated at 7999 bytes via substring() in the trigger (Pitfall 5; pg_notify caps at 8000). Phase 9 may extend the payload with a +/- byte to distinguish Put vs Delete."
  - "All testcontainers integration tests are #[ignore]'d so default macOS dev loop (no Docker) stays green; CI opts in via --include-ignored. --test-threads=1 because each test spins a fresh container."

patterns-established:
  - "Pattern: Feature-gated optional dep set declared via `dep:<crate>` references in the feature list AND `optional = true` on each dep. Keeps the default-build path free of the optional-only transitives."
  - "Pattern: PgListener wrapped in async_stream::stream! { loop { match recv { Ok => yield, Err => log+continue }}}. PgListener handles reconnect internally; the loop just needs to keep recv-ing."
  - "Pattern: testcontainers tests use `#[ignore = \"requires Docker / testcontainers\"]` per-test rather than a file-level cfg gate, so individual tests can be exercised by name during debugging."

requirements-completed: [TRAIN-04, DOCS-01, DOCS-02, DOCS-03]

# Metrics
duration: 10min
completed: 2026-05-21
---

# Phase 4 Plan 03: Postgres backend Summary

**PostgresStorage + PostgresTxn impl Storage/StorageTxn over sqlx 0.8 (runtime-checked queries; offline-mode clean) + PgListener-backed watch_stream + 2 migrations (kv + LISTEN/NOTIFY trigger with 7999-byte payload cap, snapshots+events) + 6-test testcontainers integration suite (all #[ignore]'d) + new postgres-integration CI job + Makefile postgres-test target + 180-line mdBook chapter.**

## Performance

- **Duration:** ~10 min
- **Tasks:** 2 (commits: f18a7fd, e708142)
- **Files created:** 9
- **Files modified:** 6

## Accomplishments

- `PostgresStorage::new(url, pool_size)` opens write + watch pools, runs `sqlx::migrate!()` against `database/migrations/`, exposes the full `Storage` trait.
- `Storage::watch_stream` on Postgres delivers `StorageEvent::Put` notifications via `PgListener` over channel `rollout_watch_<namespace>`. Filters by `prefix.run_id` + `prefix.path` before yielding.
- `Storage::watch` returns `Fatal(PluginContract)` — broadcast is in-process only; cross-process callers use `watch_stream`. Documented in the mdBook chapter.
- `PostgresTxn` implements CAS atomicity through a `SELECT ... FOR UPDATE` followed by a conditional `INSERT ... ON CONFLICT DO UPDATE` (or `DELETE` when `new = None`).
- Two migrations land under `database/migrations/`:
  - `0001_init.sql` — `kv` table (namespace + UUID run_id + TEXT[] path + BYTEA value + version + updated_at) + row-level `rollout_kv_notify` trigger emitting `pg_notify(channel, payload)` with `substring(payload, 1, 7999)` per Pitfall 5.
  - `0002_snapshots.sql` — `snapshots` (UUID PK + run_id + kind + algorithm_id + parts_json + meta jsonb + label) + `events` (BIGSERIAL + run_id + worker_id + ts + kind + level + payload jsonb) tables with indexes.
- testcontainers Postgres 16 integration test suite (6 tests; all `#[ignore]`'d): crud_round_trip, cas_atomicity, watch_stream_delivers_events, migrations_are_idempotent, pool_reuse_handles_many_writes, scan_returns_matching_prefix. Retry loop in `new_storage_with_retry` handles Pitfall 6 readiness lag.
- New `postgres-integration` CI job (15th workflow job): ubuntu-latest, `needs: test`, runs `cargo check -p rollout-storage --features postgres` then the test with `--include-ignored --test-threads=1`.
- `Makefile`: new `postgres-test` target (docker info preflight + same invocation as CI) + `train-smoke` placeholder (lands in plan 04-07).
- `docs/book/src/training/postgres-backend.md` (~180 lines): schema, LISTEN/NOTIFY contract, trait surface, migrations workflow, offline mode + `.sqlx`, pool sizing, testcontainers CI integration, limitations (Put-only events, Phase 9 may extend).
- Default `cargo build -p rollout-storage` is unchanged (postgres feature off); `cargo build -p rollout-storage --features postgres` is offline-mode clean (no live DB needed for the build).

## PostgresStorage code surface

| Method                                  | Impl                                                                                                            |
| --------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| `Storage::begin`                        | `self.pool.begin()` → `Box<PostgresTxn>`                                                                        |
| `Storage::get_bytes`                    | `SELECT value FROM kv WHERE namespace = ... AND run_id IS NOT DISTINCT FROM ... AND path = ...`                |
| `Storage::get_many_bytes`               | Sequential point reads (batching deferred)                                                                      |
| `Storage::scan_bytes`                   | `SELECT ... FROM kv WHERE namespace = $1 AND run_id IS NOT DISTINCT FROM $2 AND path[1:n] = $3 LIMIT $4`        |
| `Storage::watch`                        | `Err(Fatal(PluginContract { plugin: "PostgresStorage", msg: "use watch_stream" }))`                             |
| `Storage::watch_stream`                 | `listener::pg_watch_stream(&self.watch_pool, prefix)` — PgListener LISTEN + async_stream + payload parse        |
| `Storage::ping`                         | `SELECT 1`                                                                                                      |
| `StorageTxn::put_bytes`                 | `INSERT ... ON CONFLICT DO UPDATE SET value, version+1, updated_at`                                             |
| `StorageTxn::delete`                    | `DELETE FROM kv WHERE namespace = ... AND run_id IS NOT DISTINCT FROM ... AND path = ...`                       |
| `StorageTxn::cas_bytes`                 | `SELECT FOR UPDATE` → compare to `expected` → branch on `new = Some / None` for insert-update / delete / no-op  |
| `StorageTxn::commit / abort`            | `tx.commit() / tx.rollback()` via the `Option<Transaction>` take-pattern                                         |

## PgListener watch_stream behaviour

- LISTEN channel: `rollout_watch_<namespace>` (max 63 chars; `rollout_watch_` = 14, leaving 49 for the namespace).
- Payload format: `<run_id_uuid_or_empty>|<path_parts_joined_by_slash>`, truncated to 7999 bytes by the trigger.
- Filter rule: when `prefix.run_id.is_some()`, the event's `run_id` must match exactly; the event's path must start with `prefix.path`.
- Emitted variant: `StorageEvent::Put` for ALL notifications. Put vs Delete is not distinguished in the payload — Phase 9 deferral documented in the chapter.
- Reconnect: `PgListener::recv` handles connection drops internally; the wrapper loop logs the failure and continues.

## testcontainers test coverage

| Test                                  | Verifies                                                                                                        |
| ------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| `crud_round_trip`                     | put → get returns bytes; delete → get returns None                                                              |
| `cas_atomicity`                       | 4-step sequence: cas(None, Some) ok / cas(None, Some) fail / cas(Some(v1), Some(v2)) ok / cas(Some(v1), Some(v3)) fail |
| `watch_stream_delivers_events`        | Listener subscribed before commit → receives StorageEvent::Put with matching namespace + path within 10s        |
| `migrations_are_idempotent`           | Two PostgresStorage::new calls against the same DB don't error (sqlx::migrate is idempotent)                    |
| `pool_reuse_handles_many_writes`      | 50 sequential commits succeed against pool_size=4                                                               |
| `scan_returns_matching_prefix`        | 3 writes under one run_id, prefix scan returns exactly 3 rows                                                   |

## CI job shape

```yaml
postgres-integration:
  runs-on: ubuntu-latest
  needs: test
  timeout-minutes: 15
  steps:
    - checkout
    - rust-toolchain@1.88.0
    - rust-cache (key: ci-postgres-integration)
    - cargo check -p rollout-storage --features postgres   # offline-mode build verification
    - cargo test -p rollout-storage --features postgres --test postgres_integration \
        -- --include-ignored --test-threads=1
```

`needs: test` ensures we don't burn ubuntu minutes on an obvious build break; `--test-threads=1` because each test spawns a Postgres container.

## Pitfall 4-6 mitigations applied

| Pitfall                                | Mitigation                                                                                                                |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| #4 SQLX_OFFLINE in .env vs config.toml | Lives in `.cargo/config.toml [env]` (set by plan 04-00-b); confirmed unchanged by this plan                              |
| #5 pg_notify payload cap               | Trigger function caps with `substring(payload, 1, 7999)` (8000 - 1)                                                       |
| #6 testcontainers readiness lag        | `new_storage_with_retry` loops up to 30 × 2s before declaring the container dead                                          |

## Offline-mode confirmation

- `cargo build -p rollout-storage --features postgres` — **passes** in offline mode without a live DB. SQL statements go through runtime-checked `sqlx::query` rather than `query!` macros; the `.sqlx/` cache dir is reserved but empty (`.gitkeep` only).
- `cargo check -p rollout-storage --features postgres` — passes (CI's first step).
- `cargo test -p rollout-storage --features postgres --tests` — 6 ignored postgres tests + 13 default-feature tests pass (no Docker required).

## Decisions Made

(See `key-decisions` frontmatter for the full list; highlights below.)

- **Runtime-checked SQL, not `query!` macros (yet).** Avoids the chicken-and-egg of needing a live DB to regenerate `.sqlx/` before the very first commit. Switching to compile-time `query!` lands once the SQL surface stabilizes (likely Phase 5 or 6 when CAS patterns settle).
- **`watch` on Postgres is intentionally unsupported.** Returning `Fatal(PluginContract)` is clearer than synthesizing an in-process broadcast on top of `PgListener` — the trait makes the contract explicit and the chapter documents it.
- **Two-pool architecture.** Watch consumers must not starve writes; a 4-conn dedicated watch pool isolates them.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 — Bug] `StorageTxn::commit` / `abort` signatures don't match `&mut self`.**
- **Found during:** Task 1, `cargo build --features postgres`.
- **Issue:** The trait declares `async fn commit(self: Box<Self>)` (consumes the box) but the plan's literal sketch wrote `&mut self`. `E0195: lifetime parameters or bounds do not match`.
- **Fix:** Updated both `commit` and `abort` on `PostgresTxn` to `mut self: Box<Self>` matching the trait. The `Option<Transaction>` take pattern still works.
- **Files modified:** `crates/rollout-storage/src/postgres/mod.rs`.
- **Verification:** `cargo build -p rollout-storage --features postgres` succeeds; CAS round-trip in the integration test will exercise commit on a live DB.
- **Committed in:** `f18a7fd` (Task 1).

**2. [Rule 3 — Blocking] `ulid` crate not in `rollout-storage` deps.**
- **Found during:** Task 1, listener/mod.rs use `ulid::Ulid::from_bytes`.
- **Issue:** RunId is `pub struct RunId(pub Ulid)` where Ulid lives in the `ulid` crate. Going from a Postgres UUID back to a RunId requires importing `ulid::Ulid` directly — but `rollout-storage` doesn't depend on `ulid` (it gets RunId through `rollout-core` re-exports). `E0433: failed to resolve: use of unresolved module or unlinked crate ulid`.
- **Fix:** Added `ulid = { workspace = true, optional = true }` to `rollout-storage/Cargo.toml` under `[dependencies]` and `"dep:ulid"` to the `postgres` feature.
- **Files modified:** `crates/rollout-storage/Cargo.toml`.
- **Verification:** `cargo build -p rollout-storage --features postgres` exits 0.
- **Committed in:** `f18a7fd`.

**3. [Rule 1 — Bug] Used `sqlx::query!` macros in the plan's sketch — would have required a live DB.**
- **Found during:** Task 1, considering the .sqlx/ cache situation.
- **Issue:** The plan's <action> uses `sqlx::query!` macros, which require either a live `DATABASE_URL` connection at compile time OR a pre-populated `.sqlx/` cache. The plan ships `.sqlx/.gitkeep` only — no cache files. Building under either route fails on the first developer.
- **Fix:** Switched all SQL to runtime-checked `sqlx::query("...")` with `.bind()` chains. Documented at module top: "SQL is run via runtime-checked `sqlx::query` rather than the compile-time `query!` macro so the crate builds in offline mode without a pre-populated `.sqlx/` cache." The mdBook chapter notes the future migration once the schema stabilizes.
- **Files modified:** `crates/rollout-storage/src/postgres/mod.rs`, `docs/book/src/training/postgres-backend.md`.
- **Verification:** `cargo build -p rollout-storage --features postgres` succeeds in offline mode without a live DB.
- **Committed in:** `f18a7fd` (code) + `e708142` (chapter).

**4. [Rule 1 — Bug] Clippy lints across the postgres module.**
- **Found during:** Task 1, `cargo clippy -p rollout-storage --all-targets --features postgres -- -D warnings`.
- **Issue:** 10 clippy errors:
  - 1 × `duplicated_attribute` — both `lib.rs` and `mod.rs` carried `#![cfg(feature = "postgres")]` (the lib.rs `#[cfg]` on the module declaration is sufficient; remove the file-level inner attribute).
  - 5 × `redundant_closure_for_method_calls` — `|s| s.to_string()` → `SmolStr::to_string`.
  - 3 × `doc_markdown` — `PgListener` → `` `PgListener` ``, `run_id` → `` `run_id` ``, `Storage::watch` → `` [`Storage::watch`] ``.
  - 1 × `needless_pass_by_value` on the `transient` helper — by-value matches all call sites under `?` propagation; added a targeted `#[allow(clippy::needless_pass_by_value)]`.
- **Fix:** Applied each fix per the suggestion above.
- **Files modified:** `crates/rollout-storage/src/postgres/mod.rs`, `crates/rollout-storage/src/postgres/listener.rs`.
- **Verification:** `cargo clippy -p rollout-storage --all-targets --features postgres -- -D warnings` exits 0.
- **Committed in:** `f18a7fd` (Task 1, applied alongside the surface).

**5. [Rule 3 — Concurrent change] `docs/book/src/SUMMARY.md` raced with parallel agent 04-01-02.**
- **Found during:** Task 2, after writing the postgres-backend.md chapter.
- **Issue:** Plan 04-01-02 was executing in parallel and added a Training section to `SUMMARY.md` between my read and my edit. My Edit landed on the post-parallel-write state and added `Postgres backend` under their `Training` heading; the parallel agent's subsequent commit captured both lines together. By the time I went to stage my changes, SUMMARY.md was already clean (their commit `acd47d0` carried both entries).
- **Fix:** No action needed — the parallel agent's commit included my edit. Verified the final SUMMARY.md carries both `Snapshots` and `Postgres backend` under `Training`.
- **Files modified:** (none in my commit — parallel commit captured the file)
- **Verification:** `grep training/postgres-backend.md docs/book/src/SUMMARY.md` passes.
- **Committed in:** `acd47d0` (parallel agent's commit; my commit `e708142` had no SUMMARY.md diff).

---

**Total deviations:** 5 auto-fixed (3 bugs + 1 blocking + 1 parallel-coordination). 0 architectural decisions required.

**Impact on plan:** The runtime-vs-compile-time-SQL deviation (#3) is the most consequential — it eliminates the need for a pre-commit `cargo sqlx prepare` step against a one-off Postgres, makes the plan trivially reproducible on a fresh checkout, and is documented in the chapter as a Phase-4 trade-off with a forward pointer to switching back to `query!` macros once the schema stabilizes.

## Issues Encountered

- **Parallel-agent race on `docs/book/src/SUMMARY.md`.** Documented above as Deviation #5. The parallel agent's commit (`acd47d0`) absorbed both their training/snapshots line AND my training/postgres-backend line. No data loss; both entries live in the file.

## User Setup Required

None — no external service configuration. Developers who want to exercise the integration tests locally need Docker running and invoke `make postgres-test`.

## Next Phase Readiness

Plan 04-03 is complete. Downstream plans can now:

- Use `PostgresStorage` as the cross-process backend (Phase 6 multi-node coordination, Phase 4 + 5 snapshots metadata writer).
- Subscribe to cross-process events via `watch_stream` (PgListener-backed).
- Run the testcontainers integration suite under CI on every PR (default-fire on ubuntu-latest).

No blockers for Wave 2's siblings (04-01 snapshots, 04-02 algo-sft skeleton) — Wave 2 plans land on top of the same 04-00-a/04-00-b trait surface and don't depend on the postgres impl. Wave 3+ (04-04 algo-rm, 04-05 backend-vllm-train, 04-06 cli-train-snapshot, 04-07 examples-docs-smoke) can mount on top of Wave 2.

## Self-Check: PASSED

**Files present:**
- FOUND: `crates/rollout-storage/src/postgres/mod.rs`
- FOUND: `crates/rollout-storage/src/postgres/listener.rs`
- FOUND: `crates/rollout-storage/src/postgres/migrations.rs`
- FOUND: `crates/rollout-storage/tests/postgres_integration.rs`
- FOUND: `database/migrations/0001_init.sql`
- FOUND: `database/migrations/0002_snapshots.sql`
- FOUND: `docs/book/src/training/postgres-backend.md`
- FOUND: `.sqlx/.gitkeep`

**Commits present (verified via `git log --oneline | grep`):**
- FOUND: `f18a7fd` (feat(04-03-01): PostgresStorage + migrations + sqlx offline scaffold)
- FOUND: `e708142` (feat(04-03-02): testcontainers integration + CI job + Makefile + mdBook chapter)

**Acceptance grep checks (all PASSED):**
- `grep -q 'CREATE TABLE kv' database/migrations/0001_init.sql` ✓
- `grep -q 'rollout_kv_notify' database/migrations/0001_init.sql` ✓
- `grep -q 'substring(payload, 1, 7999)' database/migrations/0001_init.sql` ✓
- `grep -q 'CREATE TABLE snapshots' database/migrations/0002_snapshots.sql` ✓
- `grep -q 'CREATE TABLE events' database/migrations/0002_snapshots.sql` ✓
- `grep -q 'impl Storage for PostgresStorage' crates/rollout-storage/src/postgres/mod.rs` ✓
- `grep -q 'sqlx::migrate!' crates/rollout-storage/src/postgres/mod.rs` ✓
- `grep -q 'PgListener' crates/rollout-storage/src/postgres/listener.rs` ✓
- `grep -q 'fn watch_stream' crates/rollout-storage/src/embedded/mod.rs` ✓ (uniform surface; landed in 04-00-a, preserved here)
- `grep -c '#\[ignore' crates/rollout-storage/tests/postgres_integration.rs` ⇒ 6 ✓
- `grep -q 'postgres-integration:' .github/workflows/ci.yml` ✓
- `grep -q '\-\-include-ignored' .github/workflows/ci.yml` ✓
- `grep -q '^postgres-test:' Makefile` ✓
- `grep -q 'train-smoke:' Makefile` ✓
- `grep -q 'training/postgres-backend.md' docs/book/src/SUMMARY.md` ✓

**Builds + tests:**
- `cargo build -p rollout-storage` ✓ (default features)
- `cargo build -p rollout-storage --features postgres` ✓ (offline mode)
- `cargo build -p rollout-storage --features postgres --tests` ✓
- `cargo clippy -p rollout-storage --all-targets --features postgres -- -D warnings` ✓
- `cargo test -p rollout-storage --features postgres --tests` ✓ (13 passed + 6 postgres `#[ignore]`d)
- `mdbook build docs/book` ✓

---
*Phase: 04-train-sft-rm-snapshots*
*Plan: 03*
*Completed: 2026-05-21*
