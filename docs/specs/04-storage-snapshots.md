# Spec 04 — Storage and snapshots

This spec defines the dual storage layer (embedded + Postgres), the object store abstraction, and the four-flavor snapshot system.

## 1. Purpose

Three things are persisted in a run:

1. **Metadata** — run state, worker registry, work-item queue, heartbeats, plugin manifests, structured events. Small records, frequent reads/writes, strong consistency required.
2. **Blobs** — trajectories, model weights, snapshots, generated samples. Large objects, infrequent reads/writes, no need for strong consistency between blobs.
3. **Streams** — events, metrics, logs. Append-only, time-ordered, eventually-durable.

`rollout` separates these by design. Metadata → `Storage` trait. Blobs → `ObjectStore` trait. Streams → observability layer (spec 09).

## 1a. Phase 2 implementation notes

Phase 2's `Storage::scan_bytes` returns `Vec<(StorageKey, Vec<u8>)>` rather than the `BoxStream` shown in §2 — object-safety with `dyn Storage` + `async_trait` is incompatible with stream-returning methods on stable Rust. Streaming `scan` is deferred to a later phase that introduces a `StorageStream` newtype. The Phase-2 `rollout-core` trait surface also lowers generic typed-payload methods (`get<T>`, `put<T>`, `cas<T>`) to `_bytes` variants (`get_bytes`, `put_bytes`, `cas_bytes`); downstream crates layer `postcard` on top per `02-CONTEXT.md` D-STO-04.

## 2. Storage trait (`rollout-core`)

```rust
#[async_trait]
pub trait Storage: Send + Sync {
    /// Open a transaction. All writes within one transaction are atomic.
    async fn begin(&self) -> Result<Box<dyn StorageTxn>, CoreError>;

    /// Read-only point query.
    async fn get<T: DeserializeOwned + Send>(&self, key: &StorageKey) -> Result<Option<T>, CoreError>;

    /// Read-only batch query (principle 2: batching first).
    async fn get_many<T: DeserializeOwned + Send>(&self, keys: &[StorageKey]) -> Result<Vec<Option<T>>, CoreError>;

    /// Range scan.
    async fn scan<T: DeserializeOwned + Send>(&self, range: KeyRange) -> Result<BoxStream<'_, Result<(StorageKey, T), CoreError>>, CoreError>;

    /// Subscribe to changes on a key prefix. Used by coordinator to watch heartbeats.
    async fn watch(&self, prefix: &StorageKey) -> Result<BoxStream<'_, StorageEvent>, CoreError>;

    /// Health probe.
    async fn ping(&self) -> Result<Duration, CoreError>;
}

#[async_trait]
pub trait StorageTxn: Send {
    async fn put<T: Serialize + Send + Sync>(&mut self, key: StorageKey, value: T) -> Result<(), CoreError>;
    async fn delete(&mut self, key: StorageKey) -> Result<(), CoreError>;
    async fn cas<T: Serialize + DeserializeOwned + Send + Sync>(
        &mut self,
        key: StorageKey,
        expected: Option<T>,
        new: Option<T>,
    ) -> Result<bool, CoreError>;
    async fn commit(self: Box<Self>) -> Result<(), CoreError>;
    async fn abort(self: Box<Self>) -> Result<(), CoreError>;
}

/// Structured, typed keys. Always namespace-prefixed.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct StorageKey {
    pub namespace: SmolStr,    // e.g., "runs", "workers", "heartbeats", "queue"
    pub run_id:    Option<RunId>,
    pub path:      Vec<SmolStr>,
}
```

## 3. Backends

### 3.1 Embedded

In-process KV store. Used for local dev, single-node runs, and tests.

**Candidate:** `redb` (preferred — single-file MVCC, copy-on-write, no compaction stalls) or `sled` (more mature but compaction surprises in long runs). Decision in Phase 2.

**Properties:**

- ACID for single-key writes.
- Crash-consistent (uses fsync).
- No network. Fastest path for local.
- File-backed. Backup = copy the file.

### 3.2 Postgres

Production backend. Used when multiple coordinators / workers across machines need shared metadata.

**Schema:**

- One generic `kv` table for the bulk of writes: `(namespace, run_id, path, value, version, updated_at)`.
- Specialized tables for high-volume / structured queries: `runs`, `workers`, `heartbeats`, `work_items`, `snapshots`, `events`.
- All schema is migration-versioned. `database/migrations/` holds the source of truth.

**Properties:**

- Strong consistency, transactions, listen/notify for `watch`.
- Connection pooling via `sqlx` or `tokio-postgres` + `deadpool`.
- Logical clock via `now_at_db()` calls when needed; do not rely on worker clocks for total order.

### 3.3 Selection

A run's `storage` config picks the backend:

```toml
[storage]
backend = "embedded"          # or "postgres"

[storage.embedded]
path = "./data/rollout.db"

[storage.postgres]
url = "postgres://user@host/db"
pool_size = 16
```

**Same trait, same operations, same CLI.** A run can be moved from embedded to Postgres by changing the config and reimporting; the framework provides a `runs export` / `runs import` pair (Phase 5).

## 4. Object store trait

```rust
#[async_trait]
pub trait ObjectStore: Send + Sync {
    /// Put a blob. Content-addressed by default; the returned ContentId is what callers persist.
    async fn put(&self, body: impl AsyncRead + Send, hint: PutHint) -> Result<ContentId, CoreError>;

    /// Get a blob by content ID.
    async fn get(&self, id: &ContentId) -> Result<impl AsyncRead + Send, CoreError>;

    /// Multipart-aware put for large blobs (model checkpoints, process snapshots).
    async fn put_multipart(&self, parts: impl Stream<Item = Bytes> + Send, hint: PutHint) -> Result<ContentId, CoreError>;

    /// Range read.
    async fn get_range(&self, id: &ContentId, range: ByteRange) -> Result<impl AsyncRead + Send, CoreError>;

    /// Existence check (no download).
    async fn head(&self, id: &ContentId) -> Result<Option<ObjectMeta>, CoreError>;

    /// Delete (idempotent).
    async fn delete(&self, id: &ContentId) -> Result<(), CoreError>;

    /// Health.
    async fn ping(&self) -> Result<Duration, CoreError>;
}
```

Impls:

- `rollout-cloud-local` — filesystem-backed object store under a configurable root.
- `rollout-cloud-aws` — S3.
- `rollout-cloud-gcp` — GCS.

The framework's high-level operations (snapshot save/restore, trajectory persistence) call `ObjectStore` traits exclusively; no S3 or GCS SDK leaks above Layer 1.

## 5. Snapshots

Four flavors. Each is a first-class kind, persisted as a content-addressed blob in the object store, with a metadata row in storage that ties content IDs together.

### 5.1 Common shape

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Snapshot {
    pub id:         SnapshotId,
    pub kind:       SnapshotKind,
    pub run_id:     RunId,
    pub created_at: DateTime<Utc>,
    pub label:      Option<SmolStr>,
    pub parts:      Vec<SnapshotPart>,        // each is a content ID + role + size
    pub meta:       serde_json::Value,        // kind-specific structured metadata
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotKind {
    TrainState,
    Buffer,
    Process,
    EpisodicMemory,
}
```

### 5.2 Snapshotter trait

```rust
#[async_trait]
pub trait Snapshotter: Send + Sync {
    async fn save(&self, request: SnapshotRequest) -> Result<Snapshot, CoreError>;
    async fn restore(&self, id: &SnapshotId, target: RestoreTarget) -> Result<(), CoreError>;
    async fn list(&self, filter: SnapshotFilter) -> Result<Vec<Snapshot>, CoreError>;
    async fn prune(&self, policy: PrunePolicy) -> Result<u64, CoreError>;
}
```

### 5.3 TrainState

Captures: model weights, optimizer state, LR schedule cursor, step counter, RNG state, KL controller state (PPO/GRPO), any algorithm-internal state declared `Serialize`.

**Restore guarantee:** continuing from a TrainState snapshot at step `N` produces bit-identical weights at step `N+K` to a non-interrupted run, given the same input data and the same node topology. Off-policy randomness (e.g., a flaky reward model) breaks this; we document the boundary in the spec rather than promising impossible determinism.

### 5.4 Buffer

Replay buffer (DPO/offline) or rollout buffer (PPO/GRPO). Persists in-flight trajectories so a restarted run does not re-collect them.

**Implementation:** the buffer's underlying storage is content-addressed; a buffer snapshot is the union of those content IDs. Restore is a metadata operation, not a re-upload.

### 5.5 Process (CRIU-style)

Linux-only. Freezes a running worker's memory pages, file descriptors, and (where feasible) CUDA state.

**Mechanism:** CRIU for CPU + FDs. CUDA state is preserved via the inference backend's serialization hook where the backend supports it; otherwise the snapshot is "best-effort CPU" and CUDA state is reconstructed on restore from the TrainState snapshot.

**Use case:** spot preemption recovery. The framework opportunistically requests a process snapshot on preemption signal; if it fails, the run falls back to TrainState+Buffer.

### 5.6 Episodic memory

Per-agent persistent memory across episodes. Used by long-running agents (e.g., tool-using LLMs) that need to recall facts across sessions.

**Shape:** an append-only event log per agent, keyed by `(agent_id, episode_id)`. A snapshot is a content-addressed slice of the log at a point in time. Restore loads the log into the agent's harness.

**Note:** the *content* of episodic memory is the agent's business — a vector store, a JSON log, an SQLite file. The framework provides the storage primitives, not the format.

## 6. Snapshot policy

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SnapshotPolicy {
    /// Periodic snapshots during a run.
    pub periodic: Option<PeriodicPolicy>,
    /// Final snapshot at successful run completion.
    pub on_completion: bool,
    /// Opportunistic snapshot on spot preemption signal.
    pub on_preemption: bool,
    /// Retention policy.
    pub retention: RetentionPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PeriodicPolicy {
    pub interval_steps:   Option<u64>,
    pub interval_tokens:  Option<u64>,
    pub interval_walltime: Option<Duration>,
    pub kinds: Vec<SnapshotKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RetentionPolicy {
    pub keep_last:       u32,
    pub keep_labeled:    bool,
    pub max_age:         Option<Duration>,
}
```

## 7. Restore semantics

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum RestoreTarget {
    /// Restore into the same run (resume).
    SameRun,
    /// Fork: create a new run that starts from this snapshot.
    Fork { new_run_id: RunId },
    /// Restore in-place for a specific worker (used by process snapshots).
    Worker { worker_id: WorkerId },
}
```

A snapshot may be incompatible with a target plan (different algorithm, different model arch). The framework checks compatibility at `rollout plan` time and rejects with a descriptive error.

## 8. Local-test parity

Snapshot ops are testable without S3 / GCS:

```rust
// Tests use rollout-cloud-local as the object store.
let store = LocalObjectStore::tempdir()?;
let snap = snapshotter.save(SnapshotRequest {
    kind: SnapshotKind::TrainState,
    run_id,
    parts: ...,
}).await?;
snapshotter.restore(&snap.id, RestoreTarget::SameRun).await?;
```

The full snapshot pipeline runs identically against `LocalObjectStore`, `S3ObjectStore`, and `GcsObjectStore`. CI exercises all three (S3 / GCS via localstack / fake-gcs-server).

## 9. Failure modes

| Failure | Detection | Recovery |
|---|---|---|
| Embedded DB file corrupt | open fails | fatal; user restores from backup |
| Postgres unreachable | `ping` fails | retry with backoff; if persistent, run drains |
| Object store unreachable | `put`/`get` fails | retry per `RetryHint`; if persistent, snapshot fails (run continues if `on_completion` is the only policy) |
| Snapshot upload OOMs the worker | streaming put + bounded buffer | shrink chunk size; if still fails, fatal |
| Restore into incompatible plan | plan-time check | fatal at plan |
| Process snapshot (CRIU) fails | exit code from CRIU | fall back to TrainState+Buffer |

## 10. Test contract

- **Unit:** `StorageKey` parsing, content-ID generation, snapshot metadata serialization.
- **Integration (embedded):** end-to-end CRUD + `watch` on the embedded backend.
- **Integration (Postgres):** same test suite against a containerized Postgres (`testcontainers`).
- **Integration (object store):** the same `ObjectStore` test suite must pass against `LocalObjectStore`, S3 via localstack, and GCS via fake-gcs-server.
- **Snapshot determinism:** train → TrainState snapshot at step N → restore → continue → checksum at step N+K matches a non-interrupted reference.
- **Snapshot crash safety:** kill the process mid-snapshot; verify either the snapshot is fully present or fully absent, never partial.

## 11. Open questions

- **Embedded backend choice:** redb vs sled. Decision in Phase 2 after a small benchmark on heartbeat-write workload.
- **Postgres logical decoding for `watch`:** use `LISTEN/NOTIFY` (simple) or logical replication slots (richer, more complex). Default: LISTEN/NOTIFY in v1.
- **Object store consistency model:** S3 is now strong-read-after-write, GCS always was. Local needs an fsync barrier; localstack approximates. Document the contract: "consistent within a region, no cross-region guarantees".
- **Snapshot compression:** zstd at level 3 default. Configurable. Tune in Phase 4 once we see real checkpoint sizes.
