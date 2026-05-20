---
phase: 02-local-substrate
plan: 02
type: execute
wave: 2
depends_on: [02-00]
files_modified:
  - crates/rollout-storage/Cargo.toml
  - crates/rollout-storage/src/lib.rs
  - crates/rollout-storage/src/config.rs
  - crates/rollout-storage/src/encoding.rs
  - crates/rollout-storage/src/embedded/mod.rs
  - crates/rollout-storage/src/embedded/tables.rs
  - crates/rollout-storage/src/embedded/txn.rs
  - crates/rollout-storage/src/embedded/watch.rs
  - crates/rollout-storage/tests/crud.rs
  - crates/rollout-storage/tests/txn.rs
  - crates/rollout-storage/tests/watch.rs
  - crates/rollout-storage/tests/tables.rs
  - crates/rollout-storage/tests/crash_safety.rs
  - docs/book/src/substrate/storage.md
  - docs/book/src/SUMMARY.md
autonomous: true
requirements: [SUBSTR-01, DOCS-01, DOCS-02, DOCS-03]
must_haves:
  truths:
    - "EmbeddedStorage implements Storage + StorageTxn against redb 2.5 with always-fsync durability."
    - "Storage::watch() delivers StorageEvent to per-prefix tokio::sync::broadcast subscribers AFTER commit."
    - "StorageTxn::cas_bytes correctly handles None-expected (insert-only) and Some-expected (compare-and-swap) cases."
    - "Each table-per-namespace (runs/workers/heartbeats/queue/plugins/cloudlocal) is independently scannable without prefix filtering."
    - "Crash safety: SIGKILL between put and commit leaves the database recoverable on reopen with no partial writes visible."
  artifacts:
    - path: crates/rollout-storage/src/embedded/mod.rs
      provides: "EmbeddedStorage: impl Storage for redb-backed local KV"
      contains: "pub struct EmbeddedStorage"
    - path: crates/rollout-storage/src/embedded/txn.rs
      provides: "EmbeddedTxn: impl StorageTxn with deferred-event publish"
      contains: "pub struct EmbeddedTxn"
    - path: crates/rollout-storage/src/embedded/watch.rs
      provides: "WatchRouter: per-prefix tokio::sync::broadcast fan-out"
      contains: "pub struct WatchRouter"
    - path: crates/rollout-storage/src/encoding.rs
      provides: "postcard helpers + StorageKey byte encoding"
      contains: "fn encode_key"
  key_links:
    - from: crates/rollout-storage/src/embedded/txn.rs
      to: crates/rollout-storage/src/embedded/watch.rs
      via: "publish only after redb commit() returns Ok"
      pattern: "watch.publish"
    - from: crates/rollout-storage/src/embedded/mod.rs
      to: rollout_core::Storage
      via: "#[async_trait] impl"
      pattern: "impl Storage for EmbeddedStorage"
---

<objective>
Implement `rollout-storage` with the **redb 2.5 embedded backend** per CONTEXT D-STO-01..04: single-file MVCC, always-fsync durability, postcard value encoding, table-per-namespace layout, and in-process `tokio::sync::broadcast` `watch()` whose events fire ONLY after `commit()` returns `Ok`.

Purpose: This is the SUBSTR-01 deliverable. Every downstream Phase-2 crate that persists state (cloud-local queue spill, coordinator worker registry, plugin host manifest cache) builds on this surface. Getting the commit-then-publish invariant right is load-bearing for the coordinator's deadline scan (which `watch()`es `heartbeats/*`).

Output: `cargo test -p rollout-storage --tests` green for crud, txn, watch, tables, crash_safety (the last `#[ignore]`-gated to Linux).
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
@$HOME/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/STATE.md
@.planning/phases/02-local-substrate/02-CONTEXT.md
@.planning/phases/02-local-substrate/02-RESEARCH.md
@.planning/phases/02-local-substrate/02-VALIDATION.md
@.planning/phases/02-local-substrate/02-00-wave0-trait-extensions-PLAN.md
@docs/specs/04-storage-snapshots.md
@crates/rollout-core/src/traits/storage.rs
@crates/rollout-core/src/lib.rs
@Cargo.toml
@crates/rollout-storage/Cargo.toml
@crates/rollout-storage/src/lib.rs

<interfaces>
Trait surface this plan implements (extended by Wave 0 / plan 02-00):

```rust
// rollout-core (post-Wave-0)
pub struct StorageKey { pub namespace: smol_str::SmolStr, pub run_id: Option<RunId>, pub path: Vec<smol_str::SmolStr> }
pub struct KeyRange { pub prefix: StorageKey, pub limit: Option<usize> }
pub enum StorageEvent { Put { key: StorageKey }, Delete { key: StorageKey } }

#[async_trait]
pub trait Storage: Send + Sync {
    async fn begin(&self) -> Result<Box<dyn StorageTxn>, CoreError>;
    async fn get_bytes(&self, key: &StorageKey) -> Result<Option<Vec<u8>>, CoreError>;
    async fn get_many_bytes(&self, keys: &[StorageKey]) -> Result<Vec<Option<Vec<u8>>>, CoreError>;
    async fn scan_bytes(&self, range: KeyRange) -> Result<Vec<(StorageKey, Vec<u8>)>, CoreError>;
    async fn watch(&self, prefix: StorageKey) -> Result<tokio::sync::broadcast::Receiver<StorageEvent>, CoreError>;
    async fn ping(&self) -> Result<(), CoreError>;
}

#[async_trait]
pub trait StorageTxn: Send + Sync {
    async fn put_bytes(&mut self, key: StorageKey, value: Vec<u8>) -> Result<(), CoreError>;
    async fn delete(&mut self, key: StorageKey) -> Result<(), CoreError>;
    async fn cas_bytes(&mut self, key: StorageKey, expected: Option<Vec<u8>>, new: Option<Vec<u8>>) -> Result<bool, CoreError>;
    async fn commit(self: Box<Self>) -> Result<(), CoreError>;
    async fn abort(self: Box<Self>) -> Result<(), CoreError>;
}
```

redb 2.5 surface used (from RESEARCH §"Pattern 1" and §"Code Examples"):
```rust
use redb::{Database, TableDefinition, WriteTransaction, ReadTransaction, Durability};
const T_RUNS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("runs");
// Database::create(path) -> Database
// db.begin_write() -> WriteTransaction
// db.begin_read()  -> ReadTransaction
// txn.set_durability(Durability::Immediate) — always-fsync per D-STO-03
// txn.open_table(table_def) -> Table
// table.insert(k: &[u8], v: &[u8])
// table.get(k: &[u8]) -> Option<AccessGuard>
// table.remove(k: &[u8])
// table.range(start..end)
// txn.commit() -> Result<(), CommitError>   (fsync inside commit when Immediate)
```
</interfaces>
</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: redb backend core — config, tables, encoding, EmbeddedStorage skeleton + CRUD tests</name>
  <files>
    crates/rollout-storage/Cargo.toml,
    crates/rollout-storage/src/lib.rs,
    crates/rollout-storage/src/config.rs,
    crates/rollout-storage/src/encoding.rs,
    crates/rollout-storage/src/embedded/mod.rs,
    crates/rollout-storage/src/embedded/tables.rs,
    crates/rollout-storage/src/embedded/txn.rs,
    crates/rollout-storage/tests/crud.rs,
    crates/rollout-storage/tests/tables.rs
  </files>
  <read_first>
    - crates/rollout-storage/Cargo.toml (Wave-0 stub)
    - crates/rollout-storage/src/lib.rs (Wave-0 stub)
    - crates/rollout-core/src/traits/storage.rs (post-Wave-0 trait surface — authoritative)
    - crates/rollout-core/src/errors.rs (CoreError + FatalError variants)
    - crates/rollout-core/src/ids.rs (RunId)
    - docs/specs/04-storage-snapshots.md §2-§3 (Storage spec)
    - docs/specs/11-config-schema.md (config block rules)
    - .planning/phases/02-local-substrate/02-RESEARCH.md §"Pattern 1: redb table-per-namespace" and §"Code Examples / redb table-per-namespace"
    - .planning/phases/02-local-substrate/02-CONTEXT.md §"Storage" (D-STO-01..04)
  </read_first>
  <behavior>
    RED first (`tests/crud.rs` + `tests/tables.rs`):
    - `crud_put_get_delete_roundtrip`: open EmbeddedStorage in tempdir; begin → put_bytes(key, value) → commit; new txn → get_bytes returns Some(value); begin → delete(key) → commit; get_bytes returns None.
    - `crud_get_many_returns_correct_order`: insert 5 keys; get_many_bytes returns Vec<Option<Vec<u8>>> in the SAME order as the input slice; missing keys = None.
    - `crud_scan_returns_within_prefix`: insert 3 keys under namespace="workers", 2 under namespace="runs"; scan_bytes(KeyRange{ prefix: namespace="workers", limit: None }) returns exactly the 3 workers.
    - `crud_scan_respects_limit`: insert 10 keys; scan with limit=Some(3) returns 3.
    - `crud_ping_succeeds`: ping returns Ok.
    - `tables_each_namespace_independent`: write to namespace="runs" and namespace="workers" in the SAME txn; scan namespace="runs" returns only the runs entries; the workers entries don't leak.
    - `tables_open_many_in_one_txn`: write to 6 namespaces (runs, workers, heartbeats, queue, plugins, cloudlocal_queue) in one commit; reopen DB; verify all 6 entries readable.

    All tests use `tempfile::TempDir`; `EmbeddedStorage::open(tmp.path().join("rollout.db")).await`.

    GREEN: implement the modules so these tests pass.
  </behavior>
  <action>
    **Step 1 — `crates/rollout-storage/Cargo.toml`:**
    ```toml
    [package]
    name = "rollout-storage"
    version.workspace = true
    edition.workspace = true
    license.workspace = true
    rust-version.workspace = true
    repository.workspace = true

    [lints]
    workspace = true

    [dependencies]
    rollout-core   = { path = "../rollout-core" }
    async-trait    = { workspace = true }
    serde          = { workspace = true }
    schemars       = { workspace = true }
    thiserror      = { workspace = true }
    tracing        = { workspace = true }
    tokio          = { workspace = true }
    smol_str       = { workspace = true }
    redb           = { workspace = true }
    postcard       = { workspace = true }

    [dev-dependencies]
    tempfile       = { workspace = true }
    tokio          = { workspace = true, features = ["macros", "rt-multi-thread"] }
    ```

    **Step 2 — `src/config.rs`:**
    ```rust
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    use std::path::PathBuf;

    /// Embedded redb storage config (default backend in Phase 2).
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct EmbeddedStorageConfig {
        /// Filesystem path to the redb file. Default: ./data/rollout.db
        #[serde(default = "default_db_path")]
        pub path: PathBuf,
    }

    fn default_db_path() -> PathBuf { PathBuf::from("./data/rollout.db") }

    impl Default for EmbeddedStorageConfig {
        fn default() -> Self { Self { path: default_db_path() } }
    }
    ```

    **Step 3 — `src/embedded/tables.rs`** — `const TableDefinition` for the six Phase-2 namespaces:
    ```rust
    use redb::TableDefinition;

    pub const T_RUNS:       TableDefinition<&[u8], &[u8]> = TableDefinition::new("runs");
    pub const T_WORKERS:    TableDefinition<&[u8], &[u8]> = TableDefinition::new("workers");
    pub const T_HEARTBEATS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("heartbeats");
    pub const T_QUEUE:      TableDefinition<&[u8], &[u8]> = TableDefinition::new("queue");
    pub const T_PLUGINS:    TableDefinition<&[u8], &[u8]> = TableDefinition::new("plugins");
    pub const T_CLOUDLOCAL: TableDefinition<&[u8], &[u8]> = TableDefinition::new("cloudlocal_queue");

    /// Map a `StorageKey.namespace` &str to the redb TableDefinition.
    /// Unknown namespace -> Err(Fatal(ConfigInvalid)).
    pub fn table_for(namespace: &str) -> Result<TableDefinition<'static, &'static [u8], &'static [u8]>, rollout_core::CoreError> {
        Ok(match namespace {
            "runs"             => T_RUNS,
            "workers"          => T_WORKERS,
            "heartbeats"       => T_HEARTBEATS,
            "queue"            => T_QUEUE,
            "plugins"          => T_PLUGINS,
            "cloudlocal_queue" => T_CLOUDLOCAL,
            other => return Err(rollout_core::CoreError::Fatal(
                rollout_core::FatalError::ConfigInvalid(format!("unknown storage namespace: {other}"))
            )),
        })
    }
    ```

    **Step 4 — `src/encoding.rs`** — postcard helpers + StorageKey byte encoding:
    ```rust
    use rollout_core::StorageKey;

    /// Encode `StorageKey` to bytes for use as a redb key.
    ///
    /// Layout: postcard(run_id_bytes) || 0x00 || postcard(path_segments).
    /// Namespace is encoded in the redb table choice, not the key bytes.
    pub fn encode_key(key: &StorageKey) -> Vec<u8> {
        let mut out = postcard::to_allocvec(&key.run_id).expect("infallible: in-memory");
        out.push(0x00);
        out.extend_from_slice(&postcard::to_allocvec(&key.path).expect("infallible: in-memory"));
        out
    }

    /// Whether `candidate`'s bytes start with `prefix`'s bytes (excluding namespace).
    pub fn key_has_prefix(candidate: &StorageKey, prefix: &StorageKey) -> bool {
        candidate.namespace == prefix.namespace
            && candidate.run_id == prefix.run_id
            && candidate.path.starts_with(&prefix.path[..])
    }
    ```

    **Step 5 — `src/embedded/mod.rs`** (EmbeddedStorage skeleton; watch wired in Task 2):
    ```rust
    use async_trait::async_trait;
    use redb::{Database, Durability, ReadableTable};
    use rollout_core::{CoreError, FatalError, KeyRange, Storage, StorageEvent, StorageKey, StorageTxn};
    use std::path::Path;
    use std::sync::Arc;

    pub mod tables;
    pub mod txn;
    pub mod watch;  // landed in Task 2

    /// redb-backed local-process Storage impl.
    pub struct EmbeddedStorage {
        db:    Arc<Database>,
        watch: Arc<watch::WatchRouter>,
    }

    impl EmbeddedStorage {
        /// Open or create a redb file at `path`. Always-fsync durability.
        pub async fn open(path: impl AsRef<Path>) -> Result<Self, CoreError> {
            let path = path.as_ref().to_path_buf();
            let db = tokio::task::spawn_blocking(move || Database::create(path))
                .await
                .map_err(|e| internal(e.to_string()))?
                .map_err(|e| internal(e.to_string()))?;
            Ok(Self { db: Arc::new(db), watch: Arc::new(watch::WatchRouter::default()) })
        }
    }

    fn internal<S: Into<String>>(s: S) -> CoreError {
        CoreError::Fatal(FatalError::Internal(s.into()))
    }

    #[async_trait]
    impl Storage for EmbeddedStorage {
        async fn begin(&self) -> Result<Box<dyn StorageTxn>, CoreError> {
            let db = Arc::clone(&self.db);
            let watch = Arc::clone(&self.watch);
            let mut wtxn = tokio::task::spawn_blocking(move || db.begin_write())
                .await.map_err(|e| internal(e.to_string()))?
                .map_err(|e| internal(e.to_string()))?;
            wtxn.set_durability(Durability::Immediate);
            Ok(Box::new(txn::EmbeddedTxn::new(wtxn, watch)))
        }

        async fn get_bytes(&self, key: &StorageKey) -> Result<Option<Vec<u8>>, CoreError> {
            let table_def = tables::table_for(&key.namespace)?;
            let k = crate::encoding::encode_key(key);
            let db = Arc::clone(&self.db);
            tokio::task::spawn_blocking(move || -> Result<Option<Vec<u8>>, CoreError> {
                let rtxn = db.begin_read().map_err(|e| internal(e.to_string()))?;
                let table = rtxn.open_table(table_def).map_err(|e| internal(e.to_string()))?;
                Ok(table.get(k.as_slice()).map_err(|e| internal(e.to_string()))?.map(|g| g.value().to_vec()))
            }).await.map_err(|e| internal(e.to_string()))?
        }

        async fn get_many_bytes(&self, keys: &[StorageKey]) -> Result<Vec<Option<Vec<u8>>>, CoreError> {
            // Implement sequentially with a single read txn for consistency.
            // Group by namespace -> table; encode each key; read; reassemble in input order.
            // Detailed implementation per RESEARCH "Code Examples / redb"; preserve input order.
            // ... (full body in Task)
            unimplemented!("see implementation: open one read txn, walk keys, group by table")
        }

        async fn scan_bytes(&self, range: KeyRange) -> Result<Vec<(StorageKey, Vec<u8>)>, CoreError> {
            // open table by namespace; build prefix bytes; iterate table.range(prefix..);
            // for each entry, decode the postcard run_id + path; reconstruct StorageKey;
            // honor `range.limit`.
            unimplemented!("see implementation")
        }

        async fn watch(&self, prefix: StorageKey) -> Result<tokio::sync::broadcast::Receiver<StorageEvent>, CoreError> {
            Ok(self.watch.subscribe(prefix))
        }

        async fn ping(&self) -> Result<(), CoreError> {
            let db = Arc::clone(&self.db);
            tokio::task::spawn_blocking(move || db.begin_read().map(|_| ()))
                .await.map_err(|e| internal(e.to_string()))?
                .map_err(|e| internal(e.to_string()))
        }
    }
    ```

    Implement `get_many_bytes` and `scan_bytes` fully — open ONE read txn, group keys by table, decode `(k, v)` pairs back to (StorageKey, Vec<u8>). The "preserve input order" property is what test `crud_get_many_returns_correct_order` exercises.

    **Step 6 — `src/embedded/txn.rs`:** stub watch publish for now; Task 2 wires it in. Implement put_bytes/delete/cas_bytes/commit/abort against redb, **buffering** pending events into `Vec<StorageEvent>` so Task 2 can fan them out post-commit. cas_bytes:
    - `expected = None, new = Some(v)`: succeed only if key absent (insert-only).
    - `expected = Some(e), new = Some(v)`: succeed only if current value bytes == e.
    - `expected = Some(e), new = None`: delete-if-equal.
    - `expected = None, new = None`: succeed only if key absent (no-op).
    Returns bool: true if applied, false if mismatch.

    **Step 7 — `src/lib.rs`:**
    ```rust
    //! redb-backed embedded `Storage` impl for the rollout substrate.
    //!
    //! Default backend per CONTEXT D-STO-01. Postgres backend lives in Phase 4 (TRAIN-04).
    //! Always-fsync durability; in-process `watch()` via `tokio::sync::broadcast`.
    #![forbid(unsafe_code)]

    pub mod config;
    pub mod embedded;
    pub mod encoding;

    pub use config::EmbeddedStorageConfig;
    pub use embedded::EmbeddedStorage;
    ```

    **Step 8 — write `tests/crud.rs` and `tests/tables.rs` per `<behavior>`.** Use `#[tokio::test]`.
  </action>
  <verify>
    <automated>cargo test -p rollout-storage --test crud &amp;&amp; cargo test -p rollout-storage --test tables &amp;&amp; cargo clippy -p rollout-storage --all-targets -- -D warnings</automated>
  </verify>
  <acceptance_criteria>
    - `crates/rollout-storage/src/embedded/mod.rs` contains `pub struct EmbeddedStorage` and `impl Storage for EmbeddedStorage`
    - `crates/rollout-storage/src/embedded/tables.rs` declares all six `T_*` table constants (runs, workers, heartbeats, queue, plugins, cloudlocal_queue)
    - `crates/rollout-storage/src/embedded/txn.rs` contains `pub struct EmbeddedTxn` and pending-events buffer
    - `cargo test -p rollout-storage --test crud` exits 0 (5 tests pass)
    - `cargo test -p rollout-storage --test tables` exits 0 (2 tests pass)
    - `cargo clippy -p rollout-storage --all-targets -- -D warnings` exits 0
    - Per-commit doc/test policy: every new `pub` item has a `///` doc; new tests live in `tests/`
  </acceptance_criteria>
  <done>
    CRUD + per-namespace-table semantics work against a real redb file; transactions buffer events but do not yet publish (publish lands in Task 2).
  </done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: watch() broadcast router + txn commit-then-publish + crash safety + storage mdBook chapter</name>
  <files>
    crates/rollout-storage/src/embedded/watch.rs,
    crates/rollout-storage/src/embedded/txn.rs,
    crates/rollout-storage/tests/txn.rs,
    crates/rollout-storage/tests/watch.rs,
    crates/rollout-storage/tests/crash_safety.rs,
    docs/book/src/substrate/storage.md,
    docs/book/src/SUMMARY.md
  </files>
  <read_first>
    - crates/rollout-storage/src/embedded/mod.rs (Task 1 output — context for watch wiring)
    - crates/rollout-storage/src/embedded/txn.rs (Task 1 output — extend with publish-on-commit)
    - .planning/phases/02-local-substrate/02-RESEARCH.md §"Pattern 2: redb's no post-commit hook — write-through watch()" — AUTHORITATIVE pattern
    - .planning/phases/02-local-substrate/02-CONTEXT.md D-STO-02 (watch via tokio::sync::broadcast)
    - .planning/phases/02-local-substrate/02-RESEARCH.md §"Pitfall 1: redb has no post-commit hook"
    - .planning/phases/02-local-substrate/02-RESEARCH.md §"Pitfall 6: dev CA per-run breaks tests" — relevant for crash-safety test (use tempdir)
    - docs/book/src/substrate/index.md (link from substrate index)
    - docs/specs/04-storage-snapshots.md (the spec annotation Wave 0 added)
  </read_first>
  <behavior>
    RED first:

    `tests/txn.rs`:
    - `txn_commit_persists_writes`: begin → put → commit; new read sees value.
    - `txn_abort_discards_writes`: begin → put → abort; new read does NOT see value.
    - `txn_cas_insert_only`: cas(expected=None, new=Some(v)) on absent key returns true; second cas(expected=None, new=Some(v')) on now-present key returns false.
    - `txn_cas_compare_and_swap`: insert v1; cas(expected=Some(v1), new=Some(v2)) returns true; cas(expected=Some(v1), new=Some(v3)) returns false.
    - `txn_cas_delete_if_equal`: insert v; cas(expected=Some(v), new=None) returns true; key is now absent.

    `tests/watch.rs`:
    - `watch_publishes_after_commit`: subscriber on prefix namespace="workers"; begin → put(workers/w1, value); subscriber sees NOTHING yet; commit; subscriber receives `StorageEvent::Put { key }` with the right key.
    - `watch_does_not_publish_after_abort`: begin → put → abort; subscriber receives nothing within 200ms.
    - `watch_multiple_subscribers_same_prefix`: two subscribers on same prefix; one commit fans out to both.
    - `watch_prefix_isolation`: subscriber on workers/*; commit on runs/*; subscriber receives nothing.
    - `watch_delete_emits_event`: insert key; subscribe; delete; commit; subscriber sees `StorageEvent::Delete`.

    `tests/crash_safety.rs`:
    - `#[test] #[ignore]` (manual gate; CI runs with `-- --include-ignored` only on Linux runners — but Phase-2 CI doesn't yet add a Linux test job, so this stays manual).
    - `crash_simulation_sigkill_does_not_corrupt`: fork a child process that opens EmbeddedStorage, writes 10 keys, then `std::process::exit(0)` BEFORE commit (this aborts the txn cleanly — redb commit semantics already handle this via fsync-on-commit; the test asserts that on the parent's reopen, none of the 10 keys are visible). Use `tokio::process::Command::new(env!("CARGO"))` to spawn a helper binary or, simpler, use `std::process::exit` in a child thread inside the same test process after `drop(txn)` (which aborts).
    - Document that "true SIGKILL mid-commit" can only be tested with a child process and Linux signals; skip on macOS via `#[cfg(target_os = "linux")] #[ignore]`.

    GREEN: implement WatchRouter + commit-then-publish + (best-effort) crash safety harness.
  </behavior>
  <action>
    **Step 1 — `src/embedded/watch.rs`:**
    ```rust
    //! Per-prefix `tokio::sync::broadcast` router. Events are published AFTER
    //! the redb commit succeeds — see RESEARCH Pattern 2.
    use rollout_core::{StorageEvent, StorageKey};
    use std::collections::HashMap;
    use std::sync::Mutex;
    use tokio::sync::broadcast;

    /// In-process pub/sub for Storage events, keyed by (namespace, run_id, path-prefix).
    #[derive(Default)]
    pub struct WatchRouter {
        channels: Mutex<HashMap<PrefixKey, broadcast::Sender<StorageEvent>>>,
    }

    #[derive(Debug, Clone, Hash, Eq, PartialEq)]
    struct PrefixKey {
        namespace: smol_str::SmolStr,
        run_id:    Option<rollout_core::RunId>,
        path:      Vec<smol_str::SmolStr>,
    }

    impl PrefixKey {
        fn from(k: &StorageKey) -> Self {
            Self { namespace: k.namespace.clone(), run_id: k.run_id, path: k.path.clone() }
        }
    }

    impl WatchRouter {
        /// Subscribe to events whose key has `prefix` as a prefix.
        /// Returns a receiver; the channel is created on first subscribe.
        pub fn subscribe(&self, prefix: StorageKey) -> broadcast::Receiver<StorageEvent> {
            let key = PrefixKey::from(&prefix);
            let mut chans = self.channels.lock().unwrap();
            chans.entry(key).or_insert_with(|| broadcast::channel(256).0).subscribe()
        }

        /// Fan out `event` to every subscribed prefix that matches the event's key.
        /// Called by `EmbeddedTxn::commit()` AFTER redb returns Ok.
        pub fn publish(&self, event: StorageEvent) {
            let event_key = match &event {
                StorageEvent::Put { key } | StorageEvent::Delete { key } => key.clone(),
            };
            let chans = self.channels.lock().unwrap();
            for (prefix, sender) in chans.iter() {
                if prefix_matches(prefix, &event_key) {
                    let _ = sender.send(event.clone());  // ignore SendError (no live receivers)
                }
            }
        }
    }

    fn prefix_matches(prefix: &PrefixKey, candidate: &StorageKey) -> bool {
        prefix.namespace == candidate.namespace
            && prefix.run_id == candidate.run_id
            && candidate.path.starts_with(&prefix.path[..])
    }
    ```

    **Step 2 — extend `src/embedded/txn.rs`** so `commit()` runs the redb commit on `spawn_blocking`, and on `Ok(())` drains the pending-events buffer and calls `watch.publish(evt)` for each. On `Err`, drop the events (do NOT publish):

    ```rust
    #[async_trait::async_trait]
    impl StorageTxn for EmbeddedTxn {
        async fn commit(self: Box<Self>) -> Result<(), CoreError> {
            let Self { redb_txn, pending, watch } = *self;
            let commit_result = tokio::task::spawn_blocking(move || redb_txn.commit())
                .await
                .map_err(|e| internal(e.to_string()))?;
            commit_result.map_err(|e| internal(e.to_string()))?;
            // Durability::Immediate means fsync completed; safe to fan out.
            for evt in pending { watch.publish(evt); }
            Ok(())
        }

        async fn abort(self: Box<Self>) -> Result<(), CoreError> {
            let Self { redb_txn, .. } = *self;
            // redb aborts on drop; we don't need to call anything explicit.
            // pending events are dropped → not published.
            drop(redb_txn);
            Ok(())
        }

        // put_bytes / delete / cas_bytes append to pending: StorageEvent::Put / Delete with the StorageKey.
    }
    ```

    Important: `redb::WriteTransaction` is not `Send` after a `Table` has been opened inside it (verify against redb 2.5 docs at impl time; the spawn_blocking move handles this — the txn moves into the closure, gets committed, then is dropped inside the worker thread).

    **Step 3 — `tests/txn.rs` and `tests/watch.rs`** per `<behavior>`.

    **Step 4 — `tests/crash_safety.rs`**: implement the abort-style test (drop without commit). Mark the "real SIGKILL on child process" variant `#[ignore]` and `#[cfg(target_os = "linux")]`. The actively-running test asserts: `drop(txn)` discards writes — equivalent to the "redb is crash-safe at the commit boundary" invariant.

    **Step 5 — `docs/book/src/substrate/storage.md`** (NEW, ~80 lines):
    - **Backend choice** — redb per spec 04 §3.1 + CONTEXT D-STO-01.
    - **Table layout** — table-per-namespace; the six Phase-2 namespaces.
    - **Key encoding** — postcard-encoded `Option<RunId> || 0x00 || path`.
    - **Value encoding** — postcard.
    - **Durability** — always-fsync (`Durability::Immediate`).
    - **watch() semantics** — in-process `tokio::sync::broadcast`; events fan out AFTER commit; aborted txns publish nothing.
    - **Why no streams in `scan`** — spec-edit note from Wave 0; Vec<(key, bytes)> for Phase 2.
    - **Cross-process watch** — NOT supported by embedded; Postgres in Phase 4 brings it.

    **Step 6 — `docs/book/src/SUMMARY.md`** add storage chapter under substrate:
    ```markdown
    - [Substrate](./substrate/index.md)
      - [Proto crate](./substrate/proto.md)
      - [Storage](./substrate/storage.md)
    ```
  </action>
  <verify>
    <automated>cargo test -p rollout-storage --tests &amp;&amp; cargo clippy -p rollout-storage --all-targets -- -D warnings &amp;&amp; mdbook build docs/book</automated>
  </verify>
  <acceptance_criteria>
    - `crates/rollout-storage/src/embedded/watch.rs` contains `pub struct WatchRouter` and `fn publish` and `fn subscribe`
    - `crates/rollout-storage/src/embedded/txn.rs` commit body shows publish-after-Ok pattern (`grep -A2 'commit_result' crates/rollout-storage/src/embedded/txn.rs` shows the for-loop AFTER the `?`)
    - `cargo test -p rollout-storage --test txn` exits 0 (5 tests pass)
    - `cargo test -p rollout-storage --test watch` exits 0 (5 tests pass)
    - `cargo test -p rollout-storage --test crash_safety` exits 0 (abort test passes; SIGKILL test stays `#[ignore]`)
    - `cargo test -p rollout-storage --tests` overall exits 0 (crud + tables + txn + watch + crash_safety all green)
    - `cargo clippy -p rollout-storage --all-targets -- -D warnings` exits 0
    - `mdbook build docs/book` exits 0 with the new storage chapter linked from SUMMARY.md
    - `docs/book/src/SUMMARY.md` references `./substrate/storage.md`
    - DOCS-02 satisfied: storage.md authored + tests authored in the same commit set
  </acceptance_criteria>
  <done>
    Storage::watch() fan-out is correct (only after commit; only to matching prefixes; abort drops events); SUBSTR-01 closes; mdBook substrate/storage chapter ships.
  </done>
</task>

</tasks>

<verification>
```bash
cargo build -p rollout-storage
cargo test -p rollout-storage --tests
cargo clippy -p rollout-storage --all-targets -- -D warnings
cargo doc -p rollout-storage --no-deps --all-features
mdbook build docs/book
```
All exit 0; no warnings.
</verification>

<success_criteria>
- SUBSTR-01 satisfied: EmbeddedStorage + EmbeddedTxn implement the post-Wave-0 trait surface against redb 2.5
- Always-fsync durability; abort discards writes; commit fans events out
- Six namespace tables independently scannable
- watch() prefix isolation correct; subscribers only see events after commit
- Substrate/storage mdBook chapter renders
</success_criteria>

<output>
After completion, create `.planning/phases/02-local-substrate/02-02-rollout-storage-SUMMARY.md` documenting:
- Final `EmbeddedStorageConfig` shape and default path
- The six table constants
- How `get_many_bytes` and `scan_bytes` are implemented (one read txn? multiple? which is documented for downstream callers)
- Crash-safety harness shape (which test variants are `#[ignore]`)
- Open questions for the coordinator (plan 02-06) on how to watch heartbeats/* efficiently
</output>
