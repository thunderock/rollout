---
phase: 02-local-substrate
plan: 03
type: execute
wave: 2
depends_on: [02-00]
files_modified:
  - crates/rollout-cloud-local/Cargo.toml
  - crates/rollout-cloud-local/src/lib.rs
  - crates/rollout-cloud-local/src/config.rs
  - crates/rollout-cloud-local/src/object_store.rs
  - crates/rollout-cloud-local/src/queue.rs
  - crates/rollout-cloud-local/src/secrets.rs
  - crates/rollout-cloud-local/src/hints/mod.rs
  - crates/rollout-cloud-local/src/hints/linux.rs
  - crates/rollout-cloud-local/src/hints/macos.rs
  - crates/rollout-cloud-local/tests/object_store.rs
  - crates/rollout-cloud-local/tests/queue_replay.rs
  - crates/rollout-cloud-local/tests/secrets.rs
  - crates/rollout-cloud-local/tests/hints_linux.rs
  - crates/rollout-cloud-local/tests/hints_macos.rs
  - docs/book/src/substrate/cloud-local.md
  - docs/book/src/SUMMARY.md
autonomous: true
requirements: [SUBSTR-04, DOCS-01, DOCS-02, DOCS-03]
must_haves:
  truths:
    - "FsObjectStore writes content-addressed blobs under ./data/object-store/<sha[0..2]>/<sha[2..4]>/<sha> with sibling .meta.json"
    - "InMemQueue hot path is tokio::sync::Mutex<VecDeque<_>>; every enqueue/ack/nack mirrors to Storage; restart replay restores unack'd items"
    - "EnvSecretStore reads ROLLOUT_SECRET_<KEY> env vars filtered by config-defined allowlist; put() returns Fatal(ConfigInvalid)"
    - "ComputeHint::inventory works on Linux via /proc parsing and on macOS via sysinfo (empty gpu_inventory, preemption_signal=None)"
  artifacts:
    - path: crates/rollout-cloud-local/src/object_store.rs
      provides: "FsObjectStore: content-addressed sharded FS impl of rollout_core::ObjectStore"
      contains: "pub struct FsObjectStore"
    - path: crates/rollout-cloud-local/src/queue.rs
      provides: "InMemQueue with Storage spill for restart replay"
      contains: "pub struct InMemQueue"
    - path: crates/rollout-cloud-local/src/secrets.rs
      provides: "EnvSecretStore (read-only env var allowlist)"
      contains: "pub struct EnvSecretStore"
    - path: crates/rollout-cloud-local/src/hints/linux.rs
      provides: "LinuxComputeHint (/proc + optional nvml)"
      contains: "pub struct LinuxComputeHint"
    - path: crates/rollout-cloud-local/src/hints/macos.rs
      provides: "MacosComputeHint (sysinfo stub)"
      contains: "pub struct MacosComputeHint"
  key_links:
    - from: crates/rollout-cloud-local/src/queue.rs
      to: rollout_storage::EmbeddedStorage
      via: "trait object &dyn Storage injected at construction"
      pattern: "Arc<dyn Storage>"
    - from: crates/rollout-cloud-local/src/secrets.rs
      to: "ROLLOUT_SECRET_* env vars"
      via: "std::env::var with allowlist filter"
      pattern: "ROLLOUT_SECRET_"
---

<objective>
Implement `rollout-cloud-local` — the Phase-2 Layer-1 implementations so the rest of the stack has a real `ObjectStore` / `Queue` / `SecretStore` / `ComputeHint` to target with **zero cloud creds**.

Per CONTEXT D-LOCAL-01..05:
- `FsObjectStore` with two-level sharded layout under `./data/object-store/`.
- `InMemQueue` with VecDeque hot path + Storage spill to `cloudlocal_queue` namespace for restart replay.
- `EnvSecretStore` reading `ROLLOUT_SECRET_<KEY>` env vars through a config allowlist; `put()` returns `Fatal(ConfigInvalid)`.
- `ComputeHint` Linux full (`/proc` + optional NVML feature) + macOS minimal stub via `sysinfo`.
- `BlockStore` **skipped** (D-LOCAL-05).

Purpose: SUBSTR-04 deliverable. Also the queue is the test bed for "DIST-03 spirit" (restart replay of unack'd items) even though DIST-03 itself is Phase 6.

Output: `cargo test -p rollout-cloud-local --tests` green across all four sub-modules; Linux-only tests gated by `#[cfg(target_os = "linux")]`.
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
@docs/specs/06-cloud-layer.md
@crates/rollout-core/src/traits/cloud.rs
@crates/rollout-core/src/lib.rs
@Cargo.toml
@crates/rollout-cloud-local/Cargo.toml
@crates/rollout-cloud-local/src/lib.rs

<interfaces>
Trait surface this plan implements (extended by Wave 0):

```rust
pub struct PutHint { pub expected_size: Option<u64>, pub content_type: Option<String> }
#[async_trait] pub trait ObjectStore: Send + Sync {
    async fn put_bytes(&self, bytes: Vec<u8>, hint: PutHint) -> Result<ContentId, CoreError>;
    async fn get_bytes(&self, id: &ContentId) -> Result<Vec<u8>, CoreError>;
    async fn exists(&self, id: &ContentId) -> Result<bool, CoreError>;
}
#[async_trait] pub trait SecretStore: Send + Sync {
    async fn get(&self, name: &str) -> Result<String, CoreError>;
    async fn put(&self, name: &str, value: &str) -> Result<(), CoreError>;
}
pub struct GpuInfo { pub vendor: String, pub model: String, pub memory_mib: u64 }
pub struct ComputeInventory { pub cpu_count: u32, pub memory_mib: u64, pub gpus: Vec<GpuInfo>, pub instance_type: Option<String> }
#[async_trait] pub trait ComputeHint: Send + Sync {
    async fn inventory(&self) -> Result<ComputeInventory, CoreError>;
    async fn preemption_signal(&self) -> Result<Option<std::time::Duration>, CoreError>;
}
pub struct QueueItemId(pub ulid::Ulid);
#[async_trait] pub trait Queue: Send + Sync {
    async fn enqueue(&self, payload: Vec<u8>) -> Result<QueueItemId, CoreError>;
    async fn dequeue(&self) -> Result<Option<(QueueItemId, Vec<u8>)>, CoreError>;
    async fn ack(&self, id: QueueItemId) -> Result<(), CoreError>;
    async fn nack(&self, id: QueueItemId) -> Result<(), CoreError>;
}
```
</interfaces>
</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: FsObjectStore + EnvSecretStore + crate scaffolding + tests</name>
  <files>
    crates/rollout-cloud-local/Cargo.toml,
    crates/rollout-cloud-local/src/lib.rs,
    crates/rollout-cloud-local/src/config.rs,
    crates/rollout-cloud-local/src/object_store.rs,
    crates/rollout-cloud-local/src/secrets.rs,
    crates/rollout-cloud-local/tests/object_store.rs,
    crates/rollout-cloud-local/tests/secrets.rs
  </files>
  <read_first>
    - crates/rollout-cloud-local/Cargo.toml (Wave-0 stub)
    - crates/rollout-cloud-local/src/lib.rs (Wave-0 stub)
    - crates/rollout-core/src/traits/cloud.rs (post-Wave-0 trait surface)
    - crates/rollout-core/src/ids.rs (ContentId = [u8; 32] blake3 + Display impl)
    - docs/specs/06-cloud-layer.md (ObjectStore + SecretStore spec)
    - .planning/phases/02-local-substrate/02-CONTEXT.md D-LOCAL-01 + D-LOCAL-03
    - .planning/phases/02-local-substrate/02-RESEARCH.md §"Content-addressed sharded FS object store" (verbatim layout)
    - .planning/phases/02-local-substrate/02-RESEARCH.md §"Don't Hand-Roll" — `blake3` workspace pin + `hex` crate added in plan 02-00
  </read_first>
  <behavior>
    RED first:

    `tests/object_store.rs`:
    - `put_get_roundtrip`: put_bytes(b"hello", PutHint::default()) returns ContentId == blake3(b"hello"); get_bytes(&id) returns b"hello".
    - `put_creates_sharded_layout`: after put, file exists at root/`<hex[0..2]>/<hex[2..4]>/<hex>` (use `Path::exists` against the tempdir).
    - `put_writes_meta_json`: sibling `<hex>.meta.json` exists with size + content_type.
    - `exists_returns_true_after_put_and_false_for_missing`.
    - `put_is_idempotent_for_same_content`: putting same bytes twice returns same ContentId; the file isn't double-written (mtime stable or just verify the put returns Ok and file still exists).
    - `get_missing_returns_recoverable_or_fatal_appropriately`: choose semantics — recommend `Recoverable::Transient` is WRONG (it implies retry could fix). Use `Fatal(Internal("object not found: <hex>"))`. Document in tests/object_store.rs comment.

    `tests/secrets.rs`:
    - `secret_get_reads_env_var_with_prefix`: `std::env::set_var("ROLLOUT_SECRET_FOO", "bar")`; build EnvSecretStore with allowlist=["FOO"]; `get("FOO")` returns "bar".
    - `secret_get_outside_allowlist_returns_fatal_config_invalid`: allowlist=["FOO"]; `get("BAR")` returns `Err(Fatal(ConfigInvalid(...)))`.
    - `secret_get_unset_var_returns_recoverable_transient`: allowlist=["BAZ"]; var unset; `get("BAZ")` returns `Err(Recoverable::Transient { .. })` (the secret IS allowed but not provisioned — caller should retry after operator action).
    - `secret_put_returns_fatal_config_invalid`: `put("FOO", "x")` ALWAYS returns `Err(Fatal(ConfigInvalid("EnvSecretStore is read-only")))`.

    GREEN: implement modules.
  </behavior>
  <action>
    **Step 1 — `crates/rollout-cloud-local/Cargo.toml`:**
    ```toml
    [package]
    name = "rollout-cloud-local"
    version.workspace = true
    edition.workspace = true
    license.workspace = true
    rust-version.workspace = true
    repository.workspace = true

    [lints]
    workspace = true

    [features]
    default = []
    nvml = ["dep:nvml-wrapper"]

    [dependencies]
    rollout-core   = { path = "../rollout-core" }
    rollout-storage = { path = "../rollout-storage" }
    async-trait    = { workspace = true }
    serde          = { workspace = true }
    serde_json     = { workspace = true }
    schemars       = { workspace = true }
    thiserror      = { workspace = true }
    tracing        = { workspace = true }
    tokio          = { workspace = true }
    blake3         = { workspace = true }
    postcard       = { workspace = true }
    hex            = { workspace = true }
    ulid           = { workspace = true }
    sysinfo        = { workspace = true }
    smol_str       = { workspace = true }
    nvml-wrapper   = { workspace = true, optional = true }

    [package.metadata.cargo-machete]
    ignored = ["nvml-wrapper"] # optional dep behind feature flag; see RESEARCH Pitfall 10.

    [dev-dependencies]
    tempfile       = { workspace = true }
    tokio          = { workspace = true, features = ["macros", "rt-multi-thread"] }
    ```

    **Step 2 — `src/config.rs`:**
    ```rust
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    use std::path::PathBuf;

    /// Configuration for the cloud-local substrate Layer-1 impls.
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct CloudLocalConfig {
        /// Filesystem root for FsObjectStore. Default: ./data/object-store
        #[serde(default = "default_obj_root")]
        pub object_store_root: PathBuf,
        /// Allowlist of secret names (without the ROLLOUT_SECRET_ prefix) the local secret store may read.
        #[serde(default)]
        pub secret_allowlist: Vec<String>,
    }

    fn default_obj_root() -> PathBuf { PathBuf::from("./data/object-store") }

    impl Default for CloudLocalConfig {
        fn default() -> Self { Self { object_store_root: default_obj_root(), secret_allowlist: Vec::new() } }
    }
    ```

    **Step 3 — `src/object_store.rs`** — based on RESEARCH §"Code Examples / Content-addressed sharded FS object store" but simplified to in-memory Vec<u8> input (the trait takes `Vec<u8>`, not `impl AsyncRead`):
    ```rust
    use async_trait::async_trait;
    use rollout_core::{ContentId, CoreError, FatalError, ObjectStore, PutHint};
    use serde::{Deserialize, Serialize};
    use std::path::{Path, PathBuf};

    /// Sibling .meta.json shape.
    #[derive(Serialize, Deserialize)]
    struct ObjectMeta { size: u64, content_type: Option<String>, created_at_ms: u128 }

    /// Local-filesystem ObjectStore with two-level sharded content-addressed layout.
    pub struct FsObjectStore {
        root: PathBuf,
    }

    impl FsObjectStore {
        /// Open or create the object-store root at `root`.
        pub async fn open(root: impl AsRef<Path>) -> Result<Self, CoreError> {
            let root = root.as_ref().to_path_buf();
            tokio::fs::create_dir_all(&root).await.map_err(internal)?;
            Ok(Self { root })
        }

        fn path_for(&self, id: &ContentId) -> PathBuf {
            let hex = id.to_string();   // ContentId Display = hex
            self.root.join(&hex[0..2]).join(&hex[2..4]).join(&hex)
        }
    }

    #[async_trait]
    impl ObjectStore for FsObjectStore {
        async fn put_bytes(&self, bytes: Vec<u8>, hint: PutHint) -> Result<ContentId, CoreError> {
            let id = ContentId::of(&bytes);
            let final_path = self.path_for(&id);
            if let Some(parent) = final_path.parent() {
                tokio::fs::create_dir_all(parent).await.map_err(internal)?;
            }
            // Idempotent: if file already exists with right size, skip write.
            if !tokio::fs::try_exists(&final_path).await.map_err(internal)? {
                let tmp = final_path.with_extension("tmp");
                tokio::fs::write(&tmp, &bytes).await.map_err(internal)?;
                tokio::fs::rename(&tmp, &final_path).await.map_err(internal)?;
            }
            let meta = ObjectMeta {
                size: bytes.len() as u64,
                content_type: hint.content_type,
                created_at_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis(),
            };
            let meta_path = final_path.with_extension("meta.json");
            tokio::fs::write(&meta_path, serde_json::to_vec(&meta).map_err(internal)?).await.map_err(internal)?;
            Ok(id)
        }

        async fn get_bytes(&self, id: &ContentId) -> Result<Vec<u8>, CoreError> {
            let p = self.path_for(id);
            tokio::fs::read(&p).await.map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => CoreError::Fatal(FatalError::Internal(format!("object not found: {id}"))),
                _ => internal(e),
            })
        }

        async fn exists(&self, id: &ContentId) -> Result<bool, CoreError> {
            tokio::fs::try_exists(self.path_for(id)).await.map_err(internal)
        }
    }

    fn internal<E: std::fmt::Display>(e: E) -> CoreError {
        CoreError::Fatal(FatalError::Internal(e.to_string()))
    }
    ```

    **Step 4 — `src/secrets.rs`:**
    ```rust
    use async_trait::async_trait;
    use rollout_core::{CoreError, FatalError, RecoverableError, RetryHint, SecretStore};
    use std::collections::HashSet;

    /// Read-only env-var secret store. Per CONTEXT D-LOCAL-03:
    /// - Reads `ROLLOUT_SECRET_<NAME>` env vars filtered by `allowlist`.
    /// - `put()` always returns `Fatal(ConfigInvalid)` — the local secret store is read-only by design.
    pub struct EnvSecretStore { allowlist: HashSet<String> }

    impl EnvSecretStore {
        /// Construct from a config allowlist of secret names (without the prefix).
        pub fn new(allowlist: impl IntoIterator<Item = String>) -> Self {
            Self { allowlist: allowlist.into_iter().collect() }
        }
    }

    #[async_trait]
    impl SecretStore for EnvSecretStore {
        async fn get(&self, name: &str) -> Result<String, CoreError> {
            if !self.allowlist.contains(name) {
                return Err(CoreError::Fatal(FatalError::ConfigInvalid(
                    format!("secret '{name}' is not in the cloud-local allowlist"))));
            }
            let var = format!("ROLLOUT_SECRET_{name}");
            match std::env::var(&var) {
                Ok(v) => Ok(v),
                Err(_) => Err(CoreError::Recoverable(RecoverableError::Transient {
                    retry: RetryHint::after_seconds(0),  // operator action needed
                    reason: format!("env var {var} is not set"),
                })),
            }
        }

        async fn put(&self, _name: &str, _value: &str) -> Result<(), CoreError> {
            Err(CoreError::Fatal(FatalError::ConfigInvalid(
                "EnvSecretStore is read-only — use a cloud-backed SecretStore to write secrets".into())))
        }
    }
    ```
    NOTE on `RecoverableError::Transient` shape: check the actual Phase-1 `errors.rs` for the exact constructor signature. If `Transient { retry: RetryHint }` (without a `reason` field), simplify accordingly. The test acceptance is "returns Recoverable not Fatal" — exact variant fields are Phase-1 locked.

    **Step 5 — `src/lib.rs`:**
    ```rust
    //! `rollout-cloud-local` — Layer-1 substrate impls so the rest of the stack has
    //! a real `ObjectStore` / `Queue` / `SecretStore` / `ComputeHint` to target with
    //! zero cloud creds. Per CONTEXT D-LOCAL-01..05.
    #![forbid(unsafe_code)]

    pub mod config;
    pub mod hints;
    pub mod object_store;
    pub mod queue;     // landed in Task 2
    pub mod secrets;

    pub use config::CloudLocalConfig;
    pub use object_store::FsObjectStore;
    pub use queue::InMemQueue;
    pub use secrets::EnvSecretStore;
    ```
    The `pub mod queue` and `pub mod hints` files must exist as empty `pub fn _wave1_stub() {}` stubs so Task 1 builds; Task 2 fills them.

    **Step 6 — `tests/object_store.rs` and `tests/secrets.rs`** per `<behavior>`. Use `tempfile::TempDir`. For secret tests, `#[tokio::test]` with `serial_test` is OVERKILL — instead use UNIQUE env var names per test (e.g., `ROLLOUT_SECRET_FOO_TEST1`, `_TEST2`) to avoid cross-test races without needing the `serial_test` crate.
  </action>
  <verify>
    <automated>cargo test -p rollout-cloud-local --test object_store &amp;&amp; cargo test -p rollout-cloud-local --test secrets &amp;&amp; cargo clippy -p rollout-cloud-local --all-targets -- -D warnings</automated>
  </verify>
  <acceptance_criteria>
    - `crates/rollout-cloud-local/src/object_store.rs` contains `pub struct FsObjectStore` and `impl ObjectStore for FsObjectStore`
    - `crates/rollout-cloud-local/src/secrets.rs` contains `pub struct EnvSecretStore` and `impl SecretStore for EnvSecretStore`
    - `cargo test -p rollout-cloud-local --test object_store` exits 0
    - `cargo test -p rollout-cloud-local --test secrets` exits 0 (4 tests pass)
    - File layout in tests asserts `<root>/<hex[0..2]>/<hex[2..4]>/<hex>` exists after put
    - `cargo clippy -p rollout-cloud-local --all-targets -- -D warnings` exits 0
    - DOCS-02: tests/* + inline /// docs touched in same commit
  </acceptance_criteria>
  <done>
    FsObjectStore + EnvSecretStore work; sharded layout verified; allowlist enforced; put() read-only by design.
  </done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: InMemQueue (with Storage spill + restart replay) + ComputeHint (Linux + macOS) + mdBook chapter</name>
  <files>
    crates/rollout-cloud-local/src/queue.rs,
    crates/rollout-cloud-local/src/hints/mod.rs,
    crates/rollout-cloud-local/src/hints/linux.rs,
    crates/rollout-cloud-local/src/hints/macos.rs,
    crates/rollout-cloud-local/tests/queue_replay.rs,
    crates/rollout-cloud-local/tests/hints_linux.rs,
    crates/rollout-cloud-local/tests/hints_macos.rs,
    docs/book/src/substrate/cloud-local.md,
    docs/book/src/SUMMARY.md
  </files>
  <read_first>
    - crates/rollout-cloud-local/src/lib.rs (Task 1 output — has the `pub mod queue` and `pub mod hints` slots)
    - crates/rollout-core/src/traits/cloud.rs (post-Wave-0 Queue + ComputeHint trait surface)
    - crates/rollout-storage/src/lib.rs (EmbeddedStorage that the queue spills into)
    - docs/specs/06-cloud-layer.md §3 (Queue + ComputeHint)
    - .planning/phases/02-local-substrate/02-CONTEXT.md D-LOCAL-02 (queue spill) + D-LOCAL-04 (compute hints)
    - .planning/phases/02-local-substrate/02-RESEARCH.md §"Don't Hand-Roll" (sysinfo + nvml-wrapper rows)
    - .planning/phases/02-local-substrate/02-RESEARCH.md §"Pitfall 10: cargo-machete + optional features"
  </read_first>
  <behavior>
    RED first (`tests/queue_replay.rs`):
    - `enqueue_dequeue_basic`: enqueue 3 payloads; dequeue returns them in FIFO order with monotonically-increasing QueueItemIds (ULID); after ack, nothing more to dequeue.
    - `nack_returns_to_front`: enqueue A; dequeue → got A; nack A; dequeue → got A again.
    - `restart_replays_unacked_items`: open Storage at tempdir/db; build InMemQueue with that storage; enqueue 3 items; dequeue 2 (don't ack); drop queue; rebuild InMemQueue from the same storage; the 3 items (including the 2 dequeued-but-unacked) reappear on dequeue.
    - `ack_removes_from_storage`: enqueue; dequeue; ack; verify the `cloudlocal_queue` namespace in Storage has no entry for that QueueItemId.
    - `nack_keeps_in_storage_returns_to_queue`: enqueue; dequeue; nack; verify the entry is still in Storage AND the item is back in the in-mem deque.

    `tests/hints_macos.rs` (`#[cfg(target_os = "macos")]`):
    - `macos_inventory_has_cpu_and_memory`: inventory().cpu_count > 0; memory_mib > 0; gpus.is_empty(); instance_type is Some(...) or None (don't assert exact value).
    - `macos_preemption_signal_returns_none`: preemption_signal returns Ok(None).

    `tests/hints_linux.rs` (`#[cfg(target_os = "linux")]`):
    - `linux_inventory_parses_proc_cpuinfo`: inventory().cpu_count == num_cpus from /proc/cpuinfo OR sysinfo fallback if /proc missing in CI sandbox.
    - `linux_inventory_parses_proc_meminfo`: memory_mib > 0.
    - `linux_gpu_inventory_empty_without_nvml_feature`: with default features, gpus is empty (NVML feature OFF in test runs).
    - Mark the actual `nvml` test `#[ignore]` and `#[cfg(feature = "nvml")]`.

    GREEN: implement modules.
  </behavior>
  <action>
    **Step 1 — `src/queue.rs`:**
    ```rust
    use async_trait::async_trait;
    use rollout_core::{CoreError, FatalError, Queue, QueueItemId, Storage, StorageKey, StorageTxn};
    use std::collections::VecDeque;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    /// In-memory hot-path queue that mirrors every operation to a `Storage` impl
    /// under namespace `cloudlocal_queue/<ulid>` (postcard) so a worker restart
    /// can replay unacked items.
    pub struct InMemQueue {
        inner: Arc<Mutex<VecDeque<(QueueItemId, Vec<u8>)>>>,
        storage: Arc<dyn Storage>,
    }

    impl InMemQueue {
        /// Construct, replaying any unack'd items from `storage`'s cloudlocal_queue namespace.
        pub async fn open(storage: Arc<dyn Storage>) -> Result<Self, CoreError> {
            // Scan namespace cloudlocal_queue; rebuild VecDeque in ULID order.
            let prefix = StorageKey {
                namespace: smol_str::SmolStr::new("cloudlocal_queue"),
                run_id: None,
                path: vec![],
            };
            let entries = storage.scan_bytes(rollout_core::KeyRange { prefix, limit: None }).await?;
            let mut deque = VecDeque::new();
            for (k, payload) in entries {
                // Path segment 0 = ULID string
                if let Some(seg) = k.path.first() {
                    if let Ok(ulid) = seg.parse::<ulid::Ulid>() {
                        deque.push_back((QueueItemId(ulid), payload));
                    }
                }
            }
            // ULIDs are lex-sortable; sort to recover insertion order.
            deque.make_contiguous().sort_by_key(|(id, _)| id.0);
            Ok(Self { inner: Arc::new(Mutex::new(deque)), storage })
        }

        fn key_for(id: &QueueItemId) -> StorageKey {
            StorageKey {
                namespace: smol_str::SmolStr::new("cloudlocal_queue"),
                run_id: None,
                path: vec![smol_str::SmolStr::new(id.0.to_string())],
            }
        }
    }

    #[async_trait]
    impl Queue for InMemQueue {
        async fn enqueue(&self, payload: Vec<u8>) -> Result<QueueItemId, CoreError> {
            let id = QueueItemId(ulid::Ulid::new());
            let mut txn = self.storage.begin().await?;
            txn.put_bytes(Self::key_for(&id), payload.clone()).await?;
            txn.commit().await?;
            self.inner.lock().await.push_back((id, payload));
            Ok(id)
        }
        async fn dequeue(&self) -> Result<Option<(QueueItemId, Vec<u8>)>, CoreError> {
            Ok(self.inner.lock().await.pop_front())
        }
        async fn ack(&self, id: QueueItemId) -> Result<(), CoreError> {
            let mut txn = self.storage.begin().await?;
            txn.delete(Self::key_for(&id)).await?;
            txn.commit().await
        }
        async fn nack(&self, id: QueueItemId) -> Result<(), CoreError> {
            // Re-read payload from Storage and push back to front.
            let payload = self.storage.get_bytes(&Self::key_for(&id)).await?
                .ok_or_else(|| CoreError::Fatal(FatalError::Internal(format!("nack: queue item {id:?} absent from storage"))))?;
            self.inner.lock().await.push_front((id, payload));
            Ok(())
        }
    }
    ```

    Note: `QueueItemId(pub ulid::Ulid)` from Wave 0. If it doesn't impl `Debug` add the derive in Wave 0 task 1.

    **Step 2 — `src/hints/mod.rs`:**
    ```rust
    //! ComputeHint impls — Linux full (/proc), macOS minimal (sysinfo).
    use async_trait::async_trait;
    use rollout_core::{ComputeHint, ComputeInventory, CoreError};

    #[cfg(target_os = "linux")] pub mod linux;
    #[cfg(target_os = "macos")] pub mod macos;

    /// Construct the platform-appropriate ComputeHint.
    pub fn for_current_platform() -> Box<dyn ComputeHint> {
        #[cfg(target_os = "linux")] { Box::new(linux::LinuxComputeHint::new()) }
        #[cfg(target_os = "macos")] { Box::new(macos::MacosComputeHint::new()) }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        { compile_error!("rollout-cloud-local supports Linux and macOS only in Phase 2"); }
    }
    ```

    **Step 3 — `src/hints/linux.rs`** (`#[cfg(target_os = "linux")]`):
    - Parse `/proc/cpuinfo` to count `processor :` lines → cpu_count.
    - Parse `/proc/meminfo` `MemTotal: NNN kB` → memory_mib.
    - GPU inventory: behind `#[cfg(feature = "nvml")]`, use `nvml_wrapper::Nvml::init()` (best-effort: if init fails, return empty Vec, don't error per CONTEXT D-LOCAL-04).
    - instance_type: read `/sys/devices/virtual/dmi/id/product_name` if present; else None.
    - preemption_signal: return Ok(None) — local hosts don't get spot preemption signals (Phase 5 cloud impls do).

    **Step 4 — `src/hints/macos.rs`** (`#[cfg(target_os = "macos")]`):
    ```rust
    use async_trait::async_trait;
    use rollout_core::{ComputeHint, ComputeInventory, CoreError};
    use sysinfo::System;

    pub struct MacosComputeHint;
    impl MacosComputeHint { pub fn new() -> Self { Self } }
    impl Default for MacosComputeHint { fn default() -> Self { Self::new() } }

    #[async_trait]
    impl ComputeHint for MacosComputeHint {
        async fn inventory(&self) -> Result<ComputeInventory, CoreError> {
            let mut sys = System::new_all();
            sys.refresh_all();
            Ok(ComputeInventory {
                cpu_count: sys.cpus().len() as u32,
                memory_mib: (sys.total_memory() / 1024 / 1024) as u64,
                gpus: Vec::new(),
                instance_type: None,
            })
        }
        async fn preemption_signal(&self) -> Result<Option<std::time::Duration>, CoreError> {
            Ok(None)
        }
    }
    ```

    **Step 5 — tests per `<behavior>`.** queue_replay.rs uses `rollout_storage::EmbeddedStorage::open(tempdir.path().join("db"))` to construct the storage handle.

    **Step 6 — `docs/book/src/substrate/cloud-local.md`** (NEW, ~100 lines):
    - **What ships** — ObjectStore (FS sharded), Queue (in-mem + spill), SecretStore (env-var read-only), ComputeHint (Linux full, macOS stub).
    - **What's deferred** — BlockStore (D-LOCAL-05), full sandboxing (Phase 7), cgroups/seccomp.
    - **Queue restart semantics** — honors DIST-03's spirit (restart replay); full DIST-01..05 lands in Phase 6.
    - **Secret allowlist** — `ROLLOUT_SECRET_<KEY>` env vars; put() Fatal by design.
    - **GPU inventory** — `nvml` feature; opt-in; degrades to empty Vec gracefully.

    **Step 7 — `docs/book/src/SUMMARY.md`** extend:
    ```markdown
    - [Substrate](./substrate/index.md)
      - [Proto crate](./substrate/proto.md)
      - [Storage](./substrate/storage.md)
      - [Cloud-local](./substrate/cloud-local.md)
    ```
  </action>
  <verify>
    <automated>cargo test -p rollout-cloud-local --tests &amp;&amp; cargo clippy -p rollout-cloud-local --all-targets --all-features -- -D warnings &amp;&amp; mdbook build docs/book</automated>
  </verify>
  <acceptance_criteria>
    - `crates/rollout-cloud-local/src/queue.rs` contains `pub struct InMemQueue` and `impl Queue for InMemQueue`
    - `crates/rollout-cloud-local/src/hints/linux.rs` exists with `#[cfg(target_os = "linux")]` and `pub struct LinuxComputeHint`
    - `crates/rollout-cloud-local/src/hints/macos.rs` exists with `pub struct MacosComputeHint`
    - `cargo test -p rollout-cloud-local --test queue_replay` exits 0 (5 tests pass)
    - `cargo test -p rollout-cloud-local --test hints_macos` exits 0 on macOS dev machine (`#[cfg(target_os = "macos")]` gated)
    - `cargo test -p rollout-cloud-local --test hints_linux` compiles even on macOS (gated to no-op via `#[cfg]`); on Linux runners, the test runs and passes
    - `docs/book/src/substrate/cloud-local.md` exists; `mdbook build docs/book` exits 0
    - `cargo clippy -p rollout-cloud-local --all-targets --all-features -- -D warnings` exits 0
    - `cargo machete` (CI's unused-deps) does not flag `nvml-wrapper` (Step 1's `[package.metadata.cargo-machete]` ignore)
    - DOCS-02 satisfied: cloud-local.md + tests in same commit
  </acceptance_criteria>
  <done>
    SUBSTR-04 satisfied: ObjectStore, Queue, SecretStore, ComputeHint all working with the deferral notes (BlockStore skipped; full sandbox is Phase 7).
  </done>
</task>

</tasks>

<verification>
```bash
cargo build -p rollout-cloud-local --all-features
cargo test -p rollout-cloud-local --tests
cargo clippy -p rollout-cloud-local --all-targets --all-features -- -D warnings
cargo doc -p rollout-cloud-local --no-deps --all-features
mdbook build docs/book
```
All exit 0.
</verification>

<success_criteria>
- SUBSTR-04 satisfied across 4 sub-impls
- Queue spill-and-replay correctness verified
- macOS dev tests run; Linux integration tests gated by `#[cfg(target_os = "linux")]`
- `nvml` feature compiles independently
- Substrate/cloud-local mdBook chapter renders
</success_criteria>

<output>
After completion, create `.planning/phases/02-local-substrate/02-03-rollout-cloud-local-SUMMARY.md` documenting:
- Final config shape (CloudLocalConfig)
- Storage namespace used (cloudlocal_queue)
- The four impls' file paths
- Linux vs macOS coverage decisions
- Decisions made under "Claude's Discretion" (nvml feature gating, instance_type heuristic source on Linux, etc.)
- Open questions for plan 02-06 (does Coordinator use ObjectStore? Likely no in Phase 2; defer)
</output>
