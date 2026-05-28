//! Postgres-backed `Storage` impl (feature `postgres`).
//!
//! Mirrors `EmbeddedStorage` semantics over sqlx 0.8 + `PgListener`. The schema
//! lives under `database/migrations/`; `sqlx::migrate!()` embeds it at compile
//! time. Two pools: a write/read pool sized by the caller and a small watch
//! pool dedicated to `PgListener` consumers.
//!
//! [`Storage::watch`] is not implemented for this backend (returns a typed
//! `Fatal(PluginContract)`). Cross-process callers use `watch_stream`, which is
//! backed by `LISTEN/NOTIFY` via the trigger function defined in
//! `0001_init.sql`. See `docs/book/src/training/postgres-backend.md`.
//!
//! SQL is run via runtime-checked `sqlx::query` rather than the compile-time
//! `query!` macro so the crate builds in offline mode without a pre-populated
//! `.sqlx/` cache. The cache directory is reserved for a future switch.

pub(crate) mod listener;
pub mod migrations;

use std::time::Duration;

use async_trait::async_trait;
use futures::stream::BoxStream;
use rollout_core::{
    CoreError, FatalError, KeyRange, RecoverableError, RetryHint, RunId, Storage, StorageEvent,
    StorageKey, StorageTxn,
};
use smol_str::SmolStr;
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::Row;

/// Postgres-backed `Storage` impl. See module docs.
pub struct PostgresStorage {
    pool: PgPool,
    watch_pool: PgPool,
}

impl PostgresStorage {
    /// Open the pool, run migrations, and prepare a small watch-only pool.
    ///
    /// # Errors
    /// Returns `Recoverable(Transient)` on connect failure; `Fatal(ConfigInvalid)`
    /// when migrations fail (typically a schema mismatch with the embedded
    /// migration set under `database/migrations/`).
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
}

#[async_trait]
impl Storage for PostgresStorage {
    async fn begin(&self) -> Result<Box<dyn StorageTxn>, CoreError> {
        let tx = self.pool.begin().await.map_err(transient)?;
        Ok(Box::new(PostgresTxn { tx: Some(tx) }))
    }

    async fn get_bytes(&self, key: &StorageKey) -> Result<Option<Vec<u8>>, CoreError> {
        key.validate_for_postgres()?;
        let path: Vec<String> = key.path.iter().map(SmolStr::to_string).collect();
        let run_uuid = key.run_id.map(ulid_to_uuid);
        let row = sqlx::query("SELECT value FROM kv WHERE namespace = $1 AND run_id IS NOT DISTINCT FROM $2 AND path = $3")
            .bind(key.namespace.as_str())
            .bind(run_uuid)
            .bind(&path)
            .fetch_optional(&self.pool)
            .await
            .map_err(transient)?;
        Ok(row.map(|r| r.get::<Vec<u8>, _>("value")))
    }

    async fn get_many_bytes(&self, keys: &[StorageKey]) -> Result<Vec<Option<Vec<u8>>>, CoreError> {
        for k in keys {
            k.validate_for_postgres()?;
        }
        let mut out = Vec::with_capacity(keys.len());
        // Sequential point reads; Postgres parses the prepared statement once
        // per session so batching here is a minor optimization left for later.
        for k in keys {
            out.push(self.get_bytes(k).await?);
        }
        Ok(out)
    }

    async fn scan_bytes(&self, range: KeyRange) -> Result<Vec<(StorageKey, Vec<u8>)>, CoreError> {
        let KeyRange { prefix, limit } = range;
        prefix.validate_for_postgres()?;
        let prefix_path: Vec<String> = prefix.path.iter().map(SmolStr::to_string).collect();
        let run_uuid = prefix.run_id.map(ulid_to_uuid);
        let lim = i64::try_from(limit.unwrap_or(usize::MAX / 2)).unwrap_or(i64::MAX);

        // path[1:n] = prefix_path matches all entries whose path STARTS with prefix_path.
        // Use array slice on the LHS only when prefix_path is non-empty.
        let rows = if prefix_path.is_empty() {
            sqlx::query(
                "SELECT namespace, run_id, path, value FROM kv \
                 WHERE namespace = $1 AND run_id IS NOT DISTINCT FROM $2 LIMIT $3",
            )
            .bind(prefix.namespace.as_str())
            .bind(run_uuid)
            .bind(lim)
            .fetch_all(&self.pool)
            .await
            .map_err(transient)?
        } else {
            sqlx::query(
                "SELECT namespace, run_id, path, value FROM kv \
                 WHERE namespace = $1 AND run_id IS NOT DISTINCT FROM $2 \
                 AND path[1:array_length($3, 1)] = $3 LIMIT $4",
            )
            .bind(prefix.namespace.as_str())
            .bind(run_uuid)
            .bind(&prefix_path)
            .bind(lim)
            .fetch_all(&self.pool)
            .await
            .map_err(transient)?
        };

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let ns: String = row.get("namespace");
            let run_id_uuid: Option<uuid::Uuid> = row.get("run_id");
            let path_strs: Vec<String> = row.get("path");
            let value: Vec<u8> = row.get("value");
            let key = StorageKey {
                namespace: SmolStr::from(ns),
                run_id: run_id_uuid.map(uuid_to_ulid),
                path: path_strs.into_iter().map(SmolStr::from).collect(),
            };
            out.push((key, value));
        }
        Ok(out)
    }

    async fn watch(
        &self,
        _prefix: StorageKey,
    ) -> Result<tokio::sync::broadcast::Receiver<StorageEvent>, CoreError> {
        Err(CoreError::Fatal(FatalError::PluginContract {
            plugin: "PostgresStorage".to_owned(),
            msg: "PostgresStorage does not support in-process broadcast watch; \
                  use watch_stream for cross-process notification"
                .to_owned(),
        }))
    }

    async fn watch_stream(
        &self,
        prefix: StorageKey,
    ) -> Result<BoxStream<'static, StorageEvent>, CoreError> {
        listener::pg_watch_stream(&self.watch_pool, prefix).await
    }

    async fn ping(&self) -> Result<(), CoreError> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(transient)
    }
}

/// Postgres transaction wrapper.
pub struct PostgresTxn {
    tx: Option<sqlx::Transaction<'static, sqlx::Postgres>>,
}

impl PostgresTxn {
    fn tx_mut(&mut self) -> Result<&mut sqlx::Transaction<'static, sqlx::Postgres>, CoreError> {
        self.tx.as_mut().ok_or_else(|| {
            CoreError::Fatal(FatalError::Internal {
                msg: "PostgresTxn used after commit/abort".to_owned(),
            })
        })
    }
}

#[async_trait]
impl StorageTxn for PostgresTxn {
    async fn put_bytes(&mut self, key: StorageKey, value: Vec<u8>) -> Result<(), CoreError> {
        key.validate_for_postgres()?;
        let path: Vec<String> = key.path.iter().map(SmolStr::to_string).collect();
        let run_uuid = key.run_id.map(ulid_to_uuid);
        let tx = self.tx_mut()?;
        sqlx::query(
            "INSERT INTO kv (namespace, run_id, path, value) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (namespace, run_id, path) \
             DO UPDATE SET value = EXCLUDED.value, \
                           version = kv.version + 1, \
                           updated_at = now()",
        )
        .bind(key.namespace.as_str())
        .bind(run_uuid)
        .bind(&path)
        .bind(&value)
        .execute(&mut **tx)
        .await
        .map_err(transient)?;
        Ok(())
    }

    async fn delete(&mut self, key: StorageKey) -> Result<(), CoreError> {
        key.validate_for_postgres()?;
        let path: Vec<String> = key.path.iter().map(SmolStr::to_string).collect();
        let run_uuid = key.run_id.map(ulid_to_uuid);
        let tx = self.tx_mut()?;
        sqlx::query(
            "DELETE FROM kv WHERE namespace = $1 AND run_id IS NOT DISTINCT FROM $2 AND path = $3",
        )
        .bind(key.namespace.as_str())
        .bind(run_uuid)
        .bind(&path)
        .execute(&mut **tx)
        .await
        .map_err(transient)?;
        Ok(())
    }

    async fn cas_bytes(
        &mut self,
        key: StorageKey,
        expected: Option<Vec<u8>>,
        new: Option<Vec<u8>>,
    ) -> Result<bool, CoreError> {
        key.validate_for_postgres()?;
        let path: Vec<String> = key.path.iter().map(SmolStr::to_string).collect();
        let run_uuid = key.run_id.map(ulid_to_uuid);
        let tx = self.tx_mut()?;

        // Read current value (FOR UPDATE locks the row if present).
        let current_row = sqlx::query(
            "SELECT value FROM kv WHERE namespace = $1 AND run_id IS NOT DISTINCT FROM $2 \
             AND path = $3 FOR UPDATE",
        )
        .bind(key.namespace.as_str())
        .bind(run_uuid)
        .bind(&path)
        .fetch_optional(&mut **tx)
        .await
        .map_err(transient)?;
        let current: Option<Vec<u8>> = current_row.map(|r| r.get::<Vec<u8>, _>("value"));

        if current != expected {
            return Ok(false);
        }

        match (expected, new) {
            (_, Some(value)) => {
                // Insert or update to `value`.
                sqlx::query(
                    "INSERT INTO kv (namespace, run_id, path, value) \
                     VALUES ($1, $2, $3, $4) \
                     ON CONFLICT (namespace, run_id, path) \
                     DO UPDATE SET value = EXCLUDED.value, \
                                   version = kv.version + 1, \
                                   updated_at = now()",
                )
                .bind(key.namespace.as_str())
                .bind(run_uuid)
                .bind(&path)
                .bind(&value)
                .execute(&mut **tx)
                .await
                .map_err(transient)?;
            }
            (Some(_), None) => {
                // Delete when current matches expected and `new` is None.
                sqlx::query(
                    "DELETE FROM kv WHERE namespace = $1 AND run_id IS NOT DISTINCT FROM $2 \
                     AND path = $3",
                )
                .bind(key.namespace.as_str())
                .bind(run_uuid)
                .bind(&path)
                .execute(&mut **tx)
                .await
                .map_err(transient)?;
            }
            (None, None) => {
                // CAS(absent → absent) is a no-op once `current == expected`.
            }
        }

        Ok(true)
    }

    async fn commit(mut self: Box<Self>) -> Result<(), CoreError> {
        let tx = self.tx.take().ok_or_else(|| {
            CoreError::Fatal(FatalError::Internal {
                msg: "PostgresTxn::commit called after commit/abort".to_owned(),
            })
        })?;
        tx.commit().await.map_err(transient)?;
        Ok(())
    }

    async fn abort(mut self: Box<Self>) -> Result<(), CoreError> {
        if let Some(tx) = self.tx.take() {
            tx.rollback().await.map_err(transient)?;
        }
        Ok(())
    }
}

/// Map a `RunId(Ulid)` to its 16-byte `Uuid` representation.
pub(crate) fn ulid_to_uuid(id: RunId) -> uuid::Uuid {
    uuid::Uuid::from_bytes(id.0.to_bytes())
}

/// Inverse of `ulid_to_uuid`.
pub(crate) fn uuid_to_ulid(u: uuid::Uuid) -> RunId {
    RunId(ulid::Ulid::from_bytes(*u.as_bytes()))
}

#[allow(clippy::needless_pass_by_value)] // by-value preserves call sites under `?` chaining
pub(crate) fn transient(e: sqlx::Error) -> CoreError {
    CoreError::Recoverable(RecoverableError::Transient {
        msg: e.to_string(),
        hint: RetryHint::Never,
    })
}

pub(crate) fn fatal_config(msg: &str) -> CoreError {
    CoreError::Fatal(FatalError::ConfigInvalid {
        msg: msg.to_owned(),
    })
}
