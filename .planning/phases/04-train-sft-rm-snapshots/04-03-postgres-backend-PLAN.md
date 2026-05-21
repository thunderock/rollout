---
phase: 04-train-sft-rm-snapshots
plan: 03
type: execute
wave: 2
depends_on: [04-00-a, 04-00-b]
files_modified:
  - crates/rollout-storage/src/lib.rs
  - crates/rollout-storage/src/postgres/mod.rs
  - crates/rollout-storage/src/postgres/listener.rs
  - crates/rollout-storage/src/postgres/migrations.rs
  - crates/rollout-storage/src/embedded/mod.rs
  - crates/rollout-storage/Cargo.toml
  - database/migrations/0001_init.sql
  - database/migrations/0002_snapshots.sql
  - .sqlx/.gitkeep
  - crates/rollout-storage/tests/postgres_integration.rs
  - .github/workflows/ci.yml
  - Makefile
  - docs/book/src/training/postgres-backend.md
  - docs/book/src/SUMMARY.md
autonomous: true
requirements: [TRAIN-04, DOCS-01, DOCS-02, DOCS-03]
must_haves:
  truths:
    - "PostgresStorage impls Storage trait CRUD + scan + watch + watch_stream + ping; gated behind `postgres` Cargo feature."
    - "Migrations `0001_init.sql` (kv table) and `0002_snapshots.sql` (snapshots + events) are embedded via `sqlx::migrate!()`; idempotent (running twice is a no-op)."
    - "watch_stream() returns a BoxStream<StorageEvent> backed by PgListener; cross-process notification works (verified by spawning a second connection in the integration test)."
    - "EmbeddedStorage also impls watch_stream() by wrapping its broadcast::Receiver in tokio_stream::wrappers::BroadcastStream — uniform surface across backends."
    - ".github/workflows/ci.yml has a new `postgres-integration` job using testcontainers Postgres 16 on ubuntu-latest; runs by default on every PR."
    - "Pitfall 4 prevention: SQLX_OFFLINE lives in .cargo/config.toml (already from plan 04-00-b); cargo sqlx prepare --workspace --check passes."
    - "Pitfall 5: pg_notify payload capped at 7999 chars via trigger function substring()."
    - "Pitfall 6: testcontainers wait-for-ready uses WaitFor::message_in_stdout(\"database system is ready to accept connections\") + retry loop on first acquire."
  artifacts:
    - path: crates/rollout-storage/src/postgres/mod.rs
      provides: "PostgresStorage struct + Storage impl"
      contains: "impl Storage for PostgresStorage"
    - path: crates/rollout-storage/src/postgres/listener.rs
      provides: "PgListener-backed watch_stream"
      contains: "PgListener"
    - path: database/migrations/0001_init.sql
      provides: "kv table + LISTEN/NOTIFY trigger function rollout_kv_notify"
      contains: "CREATE TABLE kv"
    - path: database/migrations/0002_snapshots.sql
      provides: "snapshots + events tables"
      contains: "CREATE TABLE snapshots"
    - path: crates/rollout-storage/tests/postgres_integration.rs
      provides: "testcontainers Postgres 16 driven integration test"
      contains: "Postgres::default()"
    - path: .github/workflows/ci.yml
      provides: "postgres-integration CI job (default-fire on ubuntu-latest)"
      contains: "postgres-integration:"
    - path: docs/book/src/training/postgres-backend.md
      provides: "mdBook chapter — schema, watch, sqlx-data.json, testcontainers"
      contains: "PgListener"
  key_links:
    - from: crates/rollout-storage/src/lib.rs
      to: "PostgresStorage (feature-gated) + EmbeddedStorage"
      via: "pub use postgres::PostgresStorage gated on feature=postgres"
      pattern: "PostgresStorage"
    - from: crates/rollout-storage/src/postgres/mod.rs
      to: "sqlx::PgPool + sqlx::migrate!() + Storage trait"
      via: "Storage impl + migration run at new()"
      pattern: "sqlx::migrate!"
    - from: .github/workflows/ci.yml
      to: "testcontainers-modules Postgres image"
      via: "cargo test -p rollout-storage --features postgres --test postgres_integration"
      pattern: "postgres-integration:"
---

<objective>
Implement TRAIN-04: a Postgres `Storage` backend alongside the existing embedded one. Same trait, same semantics. Behind `postgres` Cargo feature so default builds stay sqlx-free.

Ships:
- `PostgresStorage` + `PostgresTxn` via sqlx 0.8 with compile-time SQL macros + offline mode.
- Two migrations under `database/migrations/`: `kv` (with the LISTEN/NOTIFY trigger) + `snapshots`/`events`.
- `PgListener`-backed `watch_stream()` for cross-process notification.
- `EmbeddedStorage::watch_stream()` companion (wrapping `BroadcastStream`) so the trait method is implemented uniformly.
- testcontainers Postgres 16 integration test under `crates/rollout-storage/tests/postgres_integration.rs`.
- New `postgres-integration` CI job (default-fire on `ubuntu-latest`; 15th job).
- `Makefile` `postgres-test` target.
- mdBook chapter `docs/book/src/training/postgres-backend.md`.

This plan is sequential in Wave 2 (alone) because it touches the `Storage` trait re-exports + introduces the watch_stream method's first impl. Keeping it isolated keeps the diff reviewable.
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
@$HOME/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/phases/04-train-sft-rm-snapshots/04-CONTEXT.md
@.planning/phases/04-train-sft-rm-snapshots/04-RESEARCH.md
@docs/specs/04-storage-snapshots.md
@.planning/phases/04-train-sft-rm-snapshots/04-00-a-wave0-trait-surface-PLAN.md
@.planning/phases/04-train-sft-rm-snapshots/04-00-b-wave0-crate-registrations-PLAN.md
@crates/rollout-storage/src/lib.rs
@crates/rollout-storage/src/embedded/mod.rs
@.planning/phases/02-local-substrate/02-02-rollout-storage-SUMMARY.md
@.github/workflows/ci.yml
@Makefile

<interfaces>
<!-- Storage trait surface this plan implements (after Wave 0). -->

From rollout-core::traits::storage::Storage (after plan 04-00-a):
```rust
#[async_trait] pub trait Storage: Send + Sync {
    async fn begin(&self) -> Result<Box<dyn StorageTxn>, CoreError>;
    async fn get_bytes(&self, key: &StorageKey) -> Result<Option<Vec<u8>>, CoreError>;
    async fn get_many_bytes(&self, keys: &[StorageKey]) -> Result<Vec<Option<Vec<u8>>>, CoreError>;
    async fn scan_bytes(&self, range: KeyRange) -> Result<Vec<(StorageKey, Vec<u8>)>, CoreError>;
    async fn watch(&self, prefix: StorageKey)
        -> Result<tokio::sync::broadcast::Receiver<StorageEvent>, CoreError>;
    async fn watch_stream(&self, prefix: StorageKey)
        -> Result<futures::stream::BoxStream<'static, StorageEvent>, CoreError>;
    async fn ping(&self) -> Result<(), CoreError>;
}
```

`StorageKey { namespace: SmolStr, run_id: Option<RunId>, path: Vec<SmolStr> }`.

From Phase-3 03-02 SUMMARY: namespaces in use: runs, workers, heartbeats, plugins, cloudlocal_queue, infer, snapshots (added in plan 04-01).
</interfaces>

</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: PostgresStorage impl + migrations + EmbeddedStorage::watch_stream + sqlx offline data</name>
  <files>
    crates/rollout-storage/src/lib.rs,
    crates/rollout-storage/src/postgres/mod.rs,
    crates/rollout-storage/src/postgres/listener.rs,
    crates/rollout-storage/src/postgres/migrations.rs,
    crates/rollout-storage/src/embedded/mod.rs,
    crates/rollout-storage/Cargo.toml,
    database/migrations/0001_init.sql,
    database/migrations/0002_snapshots.sql,
    .sqlx/.gitkeep
  </files>
  <read_first>
    crates/rollout-storage/src/lib.rs (current export surface — extend with feature-gated PostgresStorage re-export),
    crates/rollout-storage/src/embedded/mod.rs (existing EmbeddedStorage — ADD watch_stream method without disturbing watch),
    crates/rollout-storage/Cargo.toml (after 04-00-b — `postgres` feature already exists; add `[package.metadata.docs.rs] all-features = true`),
    .planning/phases/04-train-sft-rm-snapshots/04-RESEARCH.md §"Code Examples" → PostgresStorage skeleton (lines 980-1030) + CAS (1033-1070) + Migrations (1072-1146),
    .planning/phases/04-train-sft-rm-snapshots/04-RESEARCH.md §"Pitfall 4" (.cargo/config.toml SQLX_OFFLINE — already in place from 04-00-b),
    .planning/phases/04-train-sft-rm-snapshots/04-RESEARCH.md §"Pitfall 5" (NOTIFY payload truncation at 8000 bytes — trigger substring at 7999),
    .planning/phases/04-train-sft-rm-snapshots/04-RESEARCH.md §"Pitfall 6" (testcontainers wait-for-ready),
    .planning/phases/04-train-sft-rm-snapshots/04-CONTEXT.md D-PG-01..05 (sqlx 0.8 features, migrations dir, watch via PgListener, schema subset, testcontainers default-fire CI),
    docs/specs/04-storage-snapshots.md §3.2 (Postgres schema specification),
    .planning/phases/02-local-substrate/02-02-rollout-storage-SUMMARY.md (EmbeddedStorage shape; how watch was wired via tokio::sync::broadcast)
  </read_first>
  <behavior>
    - Unit Test 1 (postgres_module_compiles_without_db): `cargo build -p rollout-storage --features postgres` exits 0 WITHOUT a live DB (offline mode via .sqlx/).
    - Unit Test 2 (embedded_watch_stream_returns_events): EmbeddedStorage::watch_stream(prefix) emits StorageEvent::Put on next put_bytes (no Postgres dependency; verifies the trait method works for the embedded backend too).
    - Unit Test 3 (migration_files_exist_and_have_kv_table): grep `database/migrations/0001_init.sql` contains `CREATE TABLE kv`.
  </behavior>
  <action>
    **Step A — `crates/rollout-storage/Cargo.toml`:** confirm the `postgres` feature already added by 04-00-b is set up. Add `[package.metadata.docs.rs] all-features = true` to make docs.rs build all features.

    **Step B — Create `database/migrations/0001_init.sql`** (verbatim from RESEARCH §"Code Examples" lines 1072-1113; the trigger function caps payload at 7999 per Pitfall 5):

    ```sql
    -- Phase 4 (TRAIN-04): Postgres Storage backend, kv table.
    -- Mirrors EmbeddedStorage namespace semantics so the Storage trait works identically.

    CREATE TABLE kv (
        namespace   TEXT NOT NULL,
        run_id      UUID,                       -- ULID-as-UUID; NULL for global rows
        path        TEXT[] NOT NULL,
        value       BYTEA NOT NULL,
        version     BIGINT NOT NULL DEFAULT 0,
        updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
        PRIMARY KEY (namespace, run_id, path)
    );

    CREATE INDEX kv_namespace_run_idx ON kv (namespace, run_id);
    CREATE INDEX kv_updated_at_idx ON kv (updated_at);

    -- LISTEN/NOTIFY trigger: emit a notify on every kv mutation.
    -- Channel name `rollout_watch_<namespace>` (max 63 chars; "rollout_watch_" = 14 chars,
    -- leaves 49 for namespace).
    -- Payload truncated to 7999 bytes per Pitfall 5.
    CREATE OR REPLACE FUNCTION rollout_kv_notify() RETURNS trigger AS $$
    DECLARE
        channel TEXT;
        payload TEXT;
    BEGIN
        channel := 'rollout_watch_' || COALESCE(NEW.namespace, OLD.namespace);
        payload := COALESCE(NEW.run_id::text, OLD.run_id::text, '') || '|' ||
                   array_to_string(COALESCE(NEW.path, OLD.path), '/');
        payload := substring(payload, 1, 7999);
        PERFORM pg_notify(channel, payload);
        RETURN COALESCE(NEW, OLD);
    END;
    $$ LANGUAGE plpgsql;

    CREATE TRIGGER kv_notify_trg
        AFTER INSERT OR UPDATE OR DELETE ON kv
        FOR EACH ROW EXECUTE FUNCTION rollout_kv_notify();
    ```

    **Step C — Create `database/migrations/0002_snapshots.sql`** (verbatim from RESEARCH lines 1115-1146):

    ```sql
    -- Phase 4 (TRAIN-03): snapshot metadata + structured events.

    CREATE TABLE snapshots (
        id              UUID PRIMARY KEY,
        run_id          UUID NOT NULL,
        kind            TEXT NOT NULL,
        algorithm_id    TEXT NOT NULL,
        label           TEXT,
        parts_json      JSONB NOT NULL,
        meta            JSONB NOT NULL DEFAULT '{}'::jsonb,
        created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
    );
    CREATE INDEX snapshots_run_idx       ON snapshots (run_id);
    CREATE INDEX snapshots_kind_idx      ON snapshots (kind);
    CREATE INDEX snapshots_label_idx     ON snapshots (label) WHERE label IS NOT NULL;
    CREATE INDEX snapshots_created_idx   ON snapshots (created_at DESC);

    CREATE TABLE events (
        id              BIGSERIAL PRIMARY KEY,
        run_id          UUID NOT NULL,
        worker_id       UUID,
        ts              TIMESTAMPTZ NOT NULL DEFAULT now(),
        kind            TEXT NOT NULL,
        level           SMALLINT NOT NULL,
        payload         JSONB NOT NULL
    );
    CREATE INDEX events_run_ts_idx ON events (run_id, ts DESC);
    CREATE INDEX events_kind_idx   ON events (kind);
    ```

    **Step D — Write `crates/rollout-storage/src/postgres/mod.rs`** — `PostgresStorage` + `PostgresTxn` with the full Storage impl. Reference the RESEARCH skeleton at lines 980-1070 for the exact CAS pattern. The implementation must include:

    1. `PostgresStorage::new(url: &str, pool_size: u32)` — opens two pools (write + watch), runs migrations:

       ```rust
       use std::time::Duration;
       use rollout_core::*;
       use sqlx::postgres::{PgPool, PgPoolOptions};
       use sqlx::Executor;

       pub struct PostgresStorage {
           pool: PgPool,
           watch_pool: PgPool,
       }

       impl PostgresStorage {
           pub async fn new(url: &str, pool_size: u32) -> Result<Self, CoreError> {
               let pool = PgPoolOptions::new()
                   .min_connections(0)
                   .max_connections(pool_size.max(1))
                   .acquire_timeout(Duration::from_secs(30))
                   .idle_timeout(Some(Duration::from_secs(600)))
                   .connect(url)
                   .await
                   .map_err(transient)?;

               sqlx::migrate!("../../database/migrations")
                   .run(&pool)
                   .await
                   .map_err(|e| fatal_config(&format!("migration failed: {e}")))?;

               let watch_pool = PgPoolOptions::new()
                   .min_connections(0)
                   .max_connections(4)
                   .acquire_timeout(Duration::from_secs(30))
                   .connect(url)
                   .await
                   .map_err(transient)?;

               Ok(Self { pool, watch_pool })
           }

           #[must_use] pub(crate) fn watch_pool(&self) -> &PgPool { &self.watch_pool }
       }
       ```

    2. Storage impl with `get_bytes`, `get_many_bytes`, `scan_bytes`, `watch` (returns a not-implemented error — broadcast is in-process only; cross-process callers use watch_stream), `watch_stream` (delegates to `listener::pg_watch_stream`), `ping`, `begin`.

       For `watch`, return `CoreError::Fatal(Fatal::PluginContract { plugin: "PostgresStorage", msg: "PostgresStorage does not support in-process broadcast watch; use watch_stream for cross-process notification" })`. Document in the chapter.

       For `get_bytes`:
       ```rust
       async fn get_bytes(&self, key: &StorageKey) -> Result<Option<Vec<u8>>, CoreError> {
           let path_arr: Vec<&str> = key.path.iter().map(|s| s.as_str()).collect();
           let row = sqlx::query!(
               "SELECT value FROM kv WHERE namespace = $1 AND run_id = $2 AND path = $3",
               key.namespace.as_str(),
               key.run_id.map(ulid_to_uuid),
               &path_arr as &[&str],
           )
           .fetch_optional(&self.pool)
           .await
           .map_err(transient)?;
           Ok(row.map(|r| r.value))
       }
       ```

    3. `PostgresTxn` impl with `put_bytes`, `delete`, `cas_bytes`, `commit`, `abort`. CAS via the three-arm match from RESEARCH lines 1033-1070. Implement `begin()` by acquiring a sqlx transaction via `sqlx::Acquire`:

       ```rust
       async fn begin(&self) -> Result<Box<dyn StorageTxn>, CoreError> {
           let tx = self.pool.begin().await.map_err(transient)?;
           Ok(Box::new(PostgresTxn { tx: Some(tx) }))
       }
       ```

       `PostgresTxn` wraps `Option<sqlx::Transaction<'static, sqlx::Postgres>>` and owns the lifetime.

    4. Helper `fn ulid_to_uuid(id: RunId) -> uuid::Uuid { uuid::Uuid::from_bytes(id.0.to_bytes()) }` (RunId is ULID-shaped; cast through the 16-byte representation).

    5. Helper `fn transient(e: sqlx::Error) -> CoreError` returning `CoreError::Recoverable(Recoverable::Transient { source: e.to_string(), retry: RetryHint::immediate() })`. Helper `fn fatal_config(msg: &str)` returning `CoreError::Fatal(Fatal::ConfigInvalid { msg: msg.into() })`.

    6. Module-level `#![cfg(feature = "postgres")]` so the file only compiles under the feature.

    **Step E — Write `crates/rollout-storage/src/postgres/listener.rs`** — PgListener-backed `watch_stream`:

    ```rust
    #![cfg(feature = "postgres")]

    use futures::stream::{BoxStream, StreamExt};
    use rollout_core::{CoreError, StorageEvent, StorageKey};
    use sqlx::postgres::{PgListener, PgPool};
    use smol_str::SmolStr;

    /// Open a PgListener on the channel(s) corresponding to `prefix.namespace`,
    /// filter notifications by run_id + path, return a BoxStream of StorageEvents.
    pub(crate) async fn pg_watch_stream(
        pool: &PgPool,
        prefix: StorageKey,
    ) -> Result<BoxStream<'static, StorageEvent>, CoreError> {
        let mut listener = PgListener::connect_with(pool).await.map_err(transient)?;
        let channel = format!("rollout_watch_{}", prefix.namespace);
        listener.listen(&channel).await.map_err(transient)?;

        let stream = async_stream::stream! {
            loop {
                match listener.recv().await {
                    Ok(notification) => {
                        if let Some(event) = parse_payload(notification.payload(), &prefix) {
                            yield event;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(target: "rollout_storage::postgres",
                            error = %e, "PgListener recv failed; PgListener auto-reconnects on next loop");
                    }
                }
            }
        };
        Ok(stream.boxed())
    }

    /// Parse a `pg_notify` payload of the form `<run_id_or_empty>|<path_parts_joined_by_slash>`.
    fn parse_payload(payload: &str, prefix: &StorageKey) -> Option<StorageEvent> {
        let (run_id_str, path_str) = payload.split_once('|')?;
        let run_id = if run_id_str.is_empty() { None } else {
            uuid::Uuid::parse_str(run_id_str).ok().map(|u| {
                rollout_core::RunId::from_bytes(u.as_bytes().to_owned())
            })
        };
        let path: Vec<SmolStr> = path_str.split('/').map(SmolStr::from).collect();

        // Filter by prefix: run_id must match if specified; path must start with prefix.path.
        if prefix.run_id.is_some() && prefix.run_id != run_id { return None; }
        if !path.starts_with(&prefix.path) { return None; }

        Some(StorageEvent::Put {
            key: StorageKey { namespace: prefix.namespace.clone(), run_id, path },
        })
    }

    fn transient(e: sqlx::Error) -> CoreError {
        CoreError::Recoverable(rollout_core::Recoverable::Transient {
            source: e.to_string(),
            retry: rollout_core::RetryHint::immediate(),
        })
    }
    ```

    Add `async-stream = "0.3"` to workspace deps + crate deps (it's a small crate; pin `0.3`).

    Note: `pg_notify` doesn't distinguish Put vs Delete in the payload format used by the trigger. For Phase 4 we emit `StorageEvent::Put` for all notifications and document this — Phase 9 may extend the trigger payload to carry a `+/-` prefix. Document in chapter.

    `RunId::from_bytes` may not exist; check `crates/rollout-core/src/lib.rs` for the actual constructor — likely `RunId::new` (auto-generates) or a `From<[u8; 16]>` impl. If absent, add a helper.

    **Step F — Write `crates/rollout-storage/src/postgres/migrations.rs`** — a wrapper module for migration loading + offline-mode notes. Most of the work is done by the `sqlx::migrate!()` macro in `postgres/mod.rs::new()`. Include a comment block documenting the manual `cargo sqlx prepare --workspace --check` workflow:

    ```rust
    //! Migrations live under `<repo>/database/migrations/`. They're embedded into
    //! the binary at compile time via the `sqlx::migrate!()` macro in
    //! `super::PostgresStorage::new`.
    //!
    //! Workflow for adding a migration:
    //! 1. Create `database/migrations/NNNN_<name>.sql`.
    //! 2. Start a local Postgres: `docker run --rm -e POSTGRES_PASSWORD=pw -p 5432:5432 postgres:16`.
    //! 3. `DATABASE_URL=postgres://postgres:pw@localhost/postgres SQLX_OFFLINE=false cargo sqlx prepare --workspace -- --features postgres`.
    //! 4. Commit both the migration AND the regenerated `.sqlx/` files.
    //!
    //! CI verifies the cache with `cargo sqlx prepare --workspace --check`.
    ```

    **Step G — Extend `crates/rollout-storage/src/embedded/mod.rs`** (or wherever `impl Storage for EmbeddedStorage` lives) with a `watch_stream` method. Wrap the existing broadcast::Receiver in tokio_stream:

    ```rust
    async fn watch_stream(
        &self,
        prefix: StorageKey,
    ) -> Result<futures::stream::BoxStream<'static, StorageEvent>, CoreError> {
        let receiver = self.watch(prefix).await?;
        let stream = tokio_stream::wrappers::BroadcastStream::new(receiver)
            .filter_map(|r| async move { r.ok() });
        Ok(futures::stream::StreamExt::boxed(stream))
    }
    ```

    Add `tokio-stream` + `futures` as dependencies (workspace deps from plan 04-00-b). The `BroadcastStream` wrapper drops messages if the receiver lags, matching Phase-2 semantics.

    **Step H — Update `crates/rollout-storage/src/lib.rs`** to re-export PostgresStorage behind the feature gate:

    ```rust
    pub use embedded::EmbeddedStorage;

    #[cfg(feature = "postgres")]
    pub mod postgres;

    #[cfg(feature = "postgres")]
    pub use postgres::PostgresStorage;
    ```

    **Step I — Create `.sqlx/.gitkeep`** so the directory is tracked. The actual cache files are generated by `cargo sqlx prepare --workspace` later (developer runs this once with a live Postgres; commits results). For the FIRST commit of this plan, the .gitkeep is enough — Task 2's CI job verifies the cache stays in sync.

    **Step J — Update Cargo.toml workspace dep** to add `async-stream`:

    ```toml
    async-stream = "0.3"
    ```

    Commit message: `feat(04-03-01): PostgresStorage + migrations + EmbeddedStorage::watch_stream + sqlx offline scaffold`.
  </action>
  <verify>
    <automated>
test -f database/migrations/0001_init.sql &&
test -f database/migrations/0002_snapshots.sql &&
grep -q 'CREATE TABLE kv' database/migrations/0001_init.sql &&
grep -q 'CREATE TABLE snapshots' database/migrations/0002_snapshots.sql &&
grep -q 'pg_notify' database/migrations/0001_init.sql &&
grep -q 'substring(payload, 1, 7999)' database/migrations/0001_init.sql &&
grep -q 'impl Storage for PostgresStorage' crates/rollout-storage/src/postgres/mod.rs &&
grep -q 'PgListener' crates/rollout-storage/src/postgres/listener.rs &&
grep -q 'fn watch_stream' crates/rollout-storage/src/embedded/mod.rs &&
cargo build -p rollout-storage &&
cargo build -p rollout-storage --features postgres &&
cargo test -p rollout-storage --tests
    </automated>
  </verify>
  <acceptance_criteria>
    - `test -f database/migrations/0001_init.sql && test -f database/migrations/0002_snapshots.sql` both exit 0.
    - `grep -q 'CREATE TABLE kv' database/migrations/0001_init.sql` exits 0.
    - `grep -q 'rollout_kv_notify' database/migrations/0001_init.sql` exits 0.
    - `grep -q 'substring(payload, 1, 7999)' database/migrations/0001_init.sql` exits 0 (Pitfall 5).
    - `grep -q 'CREATE TABLE snapshots' database/migrations/0002_snapshots.sql` exits 0.
    - `grep -q 'CREATE TABLE events' database/migrations/0002_snapshots.sql` exits 0.
    - `grep -q 'impl Storage for PostgresStorage' crates/rollout-storage/src/postgres/mod.rs` exits 0.
    - `grep -q 'sqlx::migrate!' crates/rollout-storage/src/postgres/mod.rs` exits 0.
    - `grep -q 'PgListener' crates/rollout-storage/src/postgres/listener.rs` exits 0.
    - `grep -q 'fn watch_stream' crates/rollout-storage/src/embedded/mod.rs` exits 0 (uniform surface).
    - `cargo build -p rollout-storage` exits 0 (default features unaffected).
    - `cargo build -p rollout-storage --features postgres` exits 0 (offline mode — no live DB).
    - `cargo test -p rollout-storage --tests` exits 0 (existing Phase-2 tests not regressed; watch_stream unit tests pass).
    - `cargo clippy -p rollout-storage --all-targets --features postgres -- -D warnings` exits 0.
    - HEAD commit message matches `^feat\(04-03-01\):`.
    - DOCS-02 satisfied: SQL migrations are docs-equivalent (rustdoc on the postgres module mentions migrations; cross-link to spec 04 §3.2).
  </acceptance_criteria>
  <done>
    PostgresStorage compiles in offline mode. Migrations exist and embed via sqlx::migrate. PgListener-backed watch_stream code path compiles. EmbeddedStorage gains watch_stream via BroadcastStream wrapper.
  </done>
</task>

<task type="auto">
  <name>Task 2: testcontainers Postgres integration test + CI job + Makefile target + mdBook chapter</name>
  <files>
    crates/rollout-storage/tests/postgres_integration.rs,
    .github/workflows/ci.yml,
    Makefile,
    docs/book/src/training/postgres-backend.md,
    docs/book/src/SUMMARY.md
  </files>
  <read_first>
    crates/rollout-storage/tests/ (existing integration tests — mirror their structure),
    .github/workflows/ci.yml (existing 14 jobs — DO NOT rewrite; ADD a new `postgres-integration` job),
    Makefile (existing targets — ADD `postgres-test` target without disturbing existing `test`/`lint`/`smoke`),
    .planning/phases/04-train-sft-rm-snapshots/04-RESEARCH.md §"Pitfall 6" (testcontainers wait-for-ready: WaitFor::message_in_stdout + retry loop on first acquire),
    .planning/phases/04-train-sft-rm-snapshots/04-RESEARCH.md §"Validation Architecture" (test coverage table — postgres_integration covers CRUD + CAS + LISTEN/NOTIFY + migration idempotency + connection pool reuse),
    .planning/phases/04-train-sft-rm-snapshots/04-CONTEXT.md D-PG-04 (CI job is DEFAULT-FIRE on ubuntu-latest; Docker available; ~30s boot + ~10s tests)
  </read_first>
  <action>
    **Step A — Write `crates/rollout-storage/tests/postgres_integration.rs`:**

    ```rust
    //! TRAIN-04 LOAD-BEARING — testcontainers Postgres 16 integration test.
    //! Default-fire on `ubuntu-latest` in CI. Locally: `make postgres-test`.

    #![cfg(feature = "postgres")]

    use std::sync::Arc;
    use std::time::Duration;

    use futures::StreamExt;
    use rollout_core::{KeyRange, RunId, Storage, StorageEvent, StorageKey};
    use rollout_storage::PostgresStorage;
    use smol_str::SmolStr;
    use testcontainers::runners::AsyncRunner;
    use testcontainers_modules::postgres::Postgres;

    async fn start_postgres() -> (testcontainers::ContainerAsync<Postgres>, String) {
        let container = Postgres::default()
            .start()
            .await
            .expect("start postgres container");
        let port = container.get_host_port_ipv4(5432).await.unwrap();
        let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
        (container, url)
    }

    /// Retry loop per RESEARCH §"Pitfall 6": container reports "running" before
    /// PG is ready to accept connections. Wait up to 60 s for the first connect.
    async fn new_storage_with_retry(url: &str) -> PostgresStorage {
        let mut last_err = None;
        for attempt in 0..30 {
            match PostgresStorage::new(url, 4).await {
                Ok(s) => return s,
                Err(e) => {
                    last_err = Some(e);
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    if attempt == 0 {
                        tracing::info!("waiting for postgres readiness...");
                    }
                }
            }
        }
        panic!("postgres never became ready: {last_err:?}");
    }

    fn key(ns: &str, run_id: Option<RunId>, parts: &[&str]) -> StorageKey {
        StorageKey {
            namespace: SmolStr::from(ns),
            run_id,
            path: parts.iter().map(|s| SmolStr::from(*s)).collect(),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn crud_round_trip() {
        let (_c, url) = start_postgres().await;
        let storage = new_storage_with_retry(&url).await;

        let run_id = RunId::new();
        let k = key("snapshots", Some(run_id), &["abc"]);

        // PUT
        let mut txn = storage.begin().await.unwrap();
        txn.put_bytes(k.clone(), b"hello".to_vec()).await.unwrap();
        txn.commit().await.unwrap();

        // GET
        let bytes = storage.get_bytes(&k).await.unwrap();
        assert_eq!(bytes.as_deref(), Some(b"hello".as_ref()));

        // DELETE
        let mut txn = storage.begin().await.unwrap();
        txn.delete(k.clone()).await.unwrap();
        txn.commit().await.unwrap();
        assert!(storage.get_bytes(&k).await.unwrap().is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cas_atomicity() {
        let (_c, url) = start_postgres().await;
        let storage = new_storage_with_retry(&url).await;
        let k = key("snapshots", Some(RunId::new()), &["cas-test"]);

        // Insert-if-absent succeeds.
        let mut txn = storage.begin().await.unwrap();
        assert!(txn.cas_bytes(k.clone(), None, Some(b"v1".to_vec())).await.unwrap());
        txn.commit().await.unwrap();

        // Insert-if-absent again fails.
        let mut txn = storage.begin().await.unwrap();
        assert!(!txn.cas_bytes(k.clone(), None, Some(b"v2".to_vec())).await.unwrap());
        txn.commit().await.unwrap();

        // CAS v1 → v2 succeeds.
        let mut txn = storage.begin().await.unwrap();
        assert!(txn.cas_bytes(k.clone(), Some(b"v1".to_vec()), Some(b"v2".to_vec())).await.unwrap());
        txn.commit().await.unwrap();

        // CAS v1 → v3 now fails (current value is v2).
        let mut txn = storage.begin().await.unwrap();
        assert!(!txn.cas_bytes(k.clone(), Some(b"v1".to_vec()), Some(b"v3".to_vec())).await.unwrap());
        txn.commit().await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn watch_stream_delivers_events() {
        let (_c, url) = start_postgres().await;
        let storage = Arc::new(new_storage_with_retry(&url).await);

        // Subscribe FIRST (PgListener must be live before the writer commits).
        let prefix = key("snapshots", None, &[]);
        let storage_w = Arc::clone(&storage);
        let listener_task = tokio::spawn(async move {
            let mut stream = storage_w.watch_stream(prefix).await.unwrap();
            tokio::time::timeout(Duration::from_secs(10), stream.next())
                .await
                .expect("watch_stream timeout")
        });

        // Give the listener a moment to attach.
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Write.
        let k = key("snapshots", Some(RunId::new()), &["watched-key"]);
        let mut txn = storage.begin().await.unwrap();
        txn.put_bytes(k.clone(), b"trigger".to_vec()).await.unwrap();
        txn.commit().await.unwrap();

        // Receive the event.
        let evt = listener_task.await.unwrap();
        match evt {
            Some(StorageEvent::Put { key: ev_key }) => {
                assert_eq!(ev_key.namespace.as_str(), "snapshots");
                // Path matches.
                assert_eq!(ev_key.path, k.path);
            }
            other => panic!("expected Put event, got {other:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn migrations_are_idempotent() {
        let (_c, url) = start_postgres().await;
        // Running new() twice runs migrations twice; sqlx::migrate is idempotent.
        let _s1 = new_storage_with_retry(&url).await;
        let _s2 = PostgresStorage::new(&url, 4).await.unwrap();
        // If we got here, idempotency holds.
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn pool_reuse_handles_many_writes() {
        let (_c, url) = start_postgres().await;
        let storage = Arc::new(new_storage_with_retry(&url).await);
        for i in 0..50 {
            let k = key("snapshots", Some(RunId::new()),
                        &[Box::leak(format!("k-{i}").into_boxed_str()) as &str]);
            let mut txn = storage.begin().await.unwrap();
            txn.put_bytes(k, vec![i as u8]).await.unwrap();
            txn.commit().await.unwrap();
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn scan_returns_matching_prefix() {
        let (_c, url) = start_postgres().await;
        let storage = new_storage_with_retry(&url).await;
        let run_id = RunId::new();
        for i in 0..3 {
            let k = key("snapshots", Some(run_id),
                        &[Box::leak(format!("k-{i}").into_boxed_str()) as &str]);
            let mut txn = storage.begin().await.unwrap();
            txn.put_bytes(k, vec![i]).await.unwrap();
            txn.commit().await.unwrap();
        }
        let prefix = key("snapshots", Some(run_id), &[]);
        let rows = storage.scan_bytes(KeyRange { prefix, limit: None }).await.unwrap();
        assert_eq!(rows.len(), 3);
    }
    ```

    Add `tracing` to dev-dependencies if not already present. Don't gate the entire file behind `#[ignore]` — D-PG-04 says default-fire on `ubuntu-latest`. macOS local dev needs Docker; `make postgres-test` handles that. Document the local-mac dev path in the mdBook chapter.

    Wait — RESEARCH says the test gate is `--include-ignored`. Re-read CONTEXT D-PG-04: "runs `cargo test -p rollout-storage --features postgres --test postgres_integration -- --include-ignored`". That phrasing implies `#[ignore]` on the tests so they don't fire in default `cargo test --workspace`. **Adopt this**: add `#[ignore = "requires Docker / testcontainers"]` to every `#[tokio::test]` in the file, and the CI job uses `-- --include-ignored`.

    **Step B — Update `.github/workflows/ci.yml`** to add the `postgres-integration` job. Append to the existing `jobs:` map (do NOT modify existing jobs):

    ```yaml
      postgres-integration:
        runs-on: ubuntu-latest
        needs: test
        timeout-minutes: 15
        steps:
          - uses: actions/checkout@v4
          - uses: dtolnay/rust-toolchain@1.88.0
          - uses: Swatinem/rust-cache@v2
            with:
              shared-key: ci-postgres-integration
          - name: Verify sqlx-data.json in sync
            env:
              SQLX_OFFLINE: "true"
            run: |
              # In offline mode the build picks up .sqlx/ cache.
              cargo check -p rollout-storage --features postgres
          - name: Run testcontainers Postgres integration tests
            env:
              SQLX_OFFLINE: "true"
              RUST_LOG: rollout_storage=info,sqlx=warn
            run: cargo test -p rollout-storage --features postgres --test postgres_integration -- --include-ignored --test-threads=1
    ```

    `--test-threads=1` because testcontainers spins a container per test; serial avoids 6 simultaneous Postgres containers.

    **Step C — Update `Makefile`** to add `postgres-test` + `train-smoke` placeholder (train-smoke implemented in plan 04-07; add a stub here that prints "see plan 04-07"):

    ```make
    .PHONY: postgres-test
    postgres-test:  ## Run testcontainers Postgres integration tests (requires Docker)
    	@docker info >/dev/null 2>&1 || { echo "Docker not running; start Docker and retry"; exit 1; }
    	SQLX_OFFLINE=true cargo test -p rollout-storage --features postgres --test postgres_integration -- --include-ignored --test-threads=1

    .PHONY: train-smoke
    train-smoke:  ## Placeholder; populated in plan 04-07
    	@echo "train-smoke lands in plan 04-07-examples-docs-smoke"
    	@exit 1
    ```

    Preserve all existing Makefile targets verbatim.

    **Step D — Write `docs/book/src/training/postgres-backend.md`** (~180 lines). Sections:

    1. Why Postgres alongside embedded (D-PG-04 — cross-process watch_stream is the key capability).
    2. Schema: `kv`, `snapshots`, `events` tables (the `runs`/`workers` defer to Phase 6).
    3. LISTEN/NOTIFY contract (channel name `rollout_watch_<namespace>`, payload format, Pitfall 5 8000-byte cap with substring fallback to 7999).
    4. PgListener auto-reconnect behavior + `watch_stream` semantics.
    5. Trait surface: `Storage::watch` is in-process broadcast (embedded ONLY); `Storage::watch_stream` works on both backends.
    6. Migrations: `database/migrations/`, `sqlx::migrate!()`, idempotency, manual `cargo sqlx prepare` workflow.
    7. Offline mode (`SQLX_OFFLINE=true` in `.cargo/config.toml` per Pitfall 4; `.sqlx/` directory committed; CI verifies with `cargo sqlx prepare --workspace --check`).
    8. Pool sizing (RESEARCH §"Open Questions" #9 — production 1/16/30s/10m; tests 0/4/30s).
    9. Testcontainers CI integration (Pitfall 6 — WaitFor::message_in_stdout + retry loop).
    10. Local dev: `make postgres-test`.
    11. Limitations: Put-only events from `pg_notify` (Phase 9 may extend trigger payload).

    Add `postgres-backend.md` to `docs/book/src/SUMMARY.md` under the Training section.

    Commit message: `feat(04-03-02): testcontainers integration + postgres-integration CI job + Makefile + mdBook chapter`.
  </action>
  <verify>
    <automated>
test -f crates/rollout-storage/tests/postgres_integration.rs &&
grep -q 'testcontainers' crates/rollout-storage/tests/postgres_integration.rs &&
grep -q '#\[ignore = "requires Docker' crates/rollout-storage/tests/postgres_integration.rs &&
grep -q 'postgres-integration:' .github/workflows/ci.yml &&
grep -q 'cargo test -p rollout-storage --features postgres --test postgres_integration' .github/workflows/ci.yml &&
grep -q 'postgres-test:' Makefile &&
test -f docs/book/src/training/postgres-backend.md &&
grep -q 'training/postgres-backend.md' docs/book/src/SUMMARY.md &&
mdbook build docs/book &&
cargo build -p rollout-storage --features postgres
    </automated>
  </verify>
  <acceptance_criteria>
    - `test -f crates/rollout-storage/tests/postgres_integration.rs` exits 0.
    - `grep -q 'use testcontainers' crates/rollout-storage/tests/postgres_integration.rs` exits 0.
    - `grep -c '#\[ignore' crates/rollout-storage/tests/postgres_integration.rs` returns ≥ 6 (every test is `#[ignore]`d so default workspace test doesn't fire it).
    - `grep -q 'fn crud_round_trip' crates/rollout-storage/tests/postgres_integration.rs` exits 0.
    - `grep -q 'fn cas_atomicity' crates/rollout-storage/tests/postgres_integration.rs` exits 0.
    - `grep -q 'fn watch_stream_delivers_events' crates/rollout-storage/tests/postgres_integration.rs` exits 0.
    - `grep -q 'fn migrations_are_idempotent' crates/rollout-storage/tests/postgres_integration.rs` exits 0.
    - `grep -q 'postgres-integration:' .github/workflows/ci.yml` exits 0.
    - `grep -q 'needs: test' .github/workflows/ci.yml` exits 0 (the postgres-integration job has this `needs`; old jobs unchanged).
    - `grep -q '\-\-include-ignored' .github/workflows/ci.yml` exits 0.
    - `grep -q '^postgres-test:' Makefile` exits 0.
    - `grep -q 'train-smoke:' Makefile` exits 0 (placeholder).
    - `test -f docs/book/src/training/postgres-backend.md` exits 0.
    - `grep -q 'PgListener' docs/book/src/training/postgres-backend.md` exits 0.
    - `grep -q 'Pitfall 5' docs/book/src/training/postgres-backend.md` exits 0 OR equivalent narrative about 7999 truncation.
    - `grep -q 'training/postgres-backend.md' docs/book/src/SUMMARY.md` exits 0.
    - `mdbook build docs/book` exits 0.
    - `cargo build -p rollout-storage --features postgres` exits 0 (offline mode; no DB needed for build).
    - `cargo test --workspace --tests` exits 0 (the #[ignore]d tests don't fire; no regression).
    - HEAD commit message matches `^feat\(04-03-02\):`.
  </acceptance_criteria>
  <done>
    testcontainers integration test exists with ≥ 6 #[ignore]d test cases. CI gains a `postgres-integration` job that fires on every PR (Docker available on ubuntu-latest). Makefile has `postgres-test` for local invocation. mdBook chapter covers the architecture + ops.
  </done>
</task>

</tasks>

<verification>
**Phase-gate checks:**
- `cargo build -p rollout-storage --features postgres` exits 0 (offline mode).
- `cargo build -p rollout-storage` exits 0 (default features unchanged).
- `cargo test --workspace --tests` no regressions (`#[ignore]`d Postgres tests don't fire).
- `cargo clippy -p rollout-storage --features postgres --all-targets -- -D warnings` clean.
- `cargo doc -p rollout-storage --features postgres --no-deps` clean.
- `mdbook build docs/book` clean.
- On a machine with Docker: `make postgres-test` passes (≥ 6 tests green; takes ~30s for boot + ~10s tests per the CONTEXT D-PG-04 estimate).
- `cargo deny check` clean (verify new sqlx + async-stream + uuid transitive deps).

**Conventional commits:** `feat(04-03-01)`, `feat(04-03-02)`.
**DOCS-01..03:** SQL migrations + rustdoc + mdBook chapter all land per-commit.
</verification>

<success_criteria>
- TRAIN-04 delivered: PostgresStorage impls full Storage trait + CAS + watch_stream via PgListener.
- Two migrations land in `database/migrations/` (kv + snapshots + events).
- testcontainers integration test covers CRUD, CAS, LISTEN/NOTIFY, migration idempotency, pool reuse, scan.
- New `postgres-integration` CI job (15th total) default-fires on ubuntu-latest.
- EmbeddedStorage::watch_stream wraps the existing broadcast — uniform surface across backends.
- mdBook postgres-backend chapter linked from SUMMARY.md.
</success_criteria>

<output>
After completion, create `.planning/phases/04-train-sft-rm-snapshots/04-03-postgres-backend-SUMMARY.md` recording: (1) PostgresStorage code surface + Storage impl method list, (2) Migration files + the trigger function payload format, (3) PgListener watch_stream behaviour (Put-only events; Phase 9 deferred), (4) testcontainers test coverage table (6 tests × what they verify), (5) CI job shape + needs dependency, (6) Pitfall 4-6 mitigations actually applied, (7) explicit confirmation that `cargo build -p rollout-storage --features postgres` succeeds in offline mode (no live DB) — `.sqlx/` cache state, (8) any deviation (e.g., if `cargo sqlx prepare` cache had to be generated against a one-off Postgres before commit).
</output>
