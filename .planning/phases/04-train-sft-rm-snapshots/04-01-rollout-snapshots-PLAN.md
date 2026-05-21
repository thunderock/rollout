---
phase: 04-train-sft-rm-snapshots
plan: 01
type: execute
wave: 2
depends_on: [04-00-a, 04-00-b]
files_modified:
  - crates/rollout-snapshots/src/lib.rs
  - crates/rollout-snapshots/src/tar_build.rs
  - crates/rollout-snapshots/src/kind/mod.rs
  - crates/rollout-snapshots/src/kind/train_state.rs
  - crates/rollout-snapshots/src/policy.rs
  - crates/rollout-snapshots/src/key.rs
  - crates/rollout-snapshots/Cargo.toml
  - crates/rollout-snapshots/tests/save_restore_roundtrip.rs
  - crates/rollout-snapshots/tests/deterministic_tar.rs
  - crates/rollout-snapshots/tests/list_and_prune.rs
  - crates/rollout-storage/src/embedded/tables.rs
  - docs/book/src/training/snapshots.md
  - docs/book/src/SUMMARY.md
autonomous: true
requirements: [TRAIN-03, DOCS-01, DOCS-02, DOCS-03]
must_haves:
  truths:
    - "SnapshotterImpl saves a TrainState snapshot end-to-end: tar a directory deterministically, blake3-hash, write to ObjectStore, persist metadata row to Storage under namespace=\"snapshots\"."
    - "Tar build is byte-identical across runs and platforms (Pitfall 9: explicit mode bits 0o644/0o755, mtime=0, uid=gid=0, sort by name, HeaderMode::Deterministic)."
    - "Restore round-trip: save → list → fetch tar via ObjectStore::get_bytes → blake3-verify → extract to tempdir → confirm files match originals byte-for-byte."
    - "SnapshotKind::Buffer / Process / EpisodicMemory return Fatal { PluginContract, msg: \"Phase N: <kind>\" } as documented in spec 04 §5a."
    - "SnapshotterImpl::list filters by run_id + kind + label_contains + limit; results returned newest-first."
    - "SnapshotterImpl::prune honors RetentionPolicy { keep_last, keep_labeled, max_age } and returns the count actually deleted."
    - "rollout-storage gains the `snapshots` namespace registered in embedded::tables::table_for() (mirrors Phase-3 `infer` namespace addition)."
  artifacts:
    - path: crates/rollout-snapshots/src/lib.rs
      provides: "SnapshotterImpl + Snapshotter trait impl"
      contains: "impl Snapshotter for SnapshotterImpl"
    - path: crates/rollout-snapshots/src/tar_build.rs
      provides: "build_deterministic_tar(&Path) -> Result<Vec<u8>, CoreError>"
      contains: "build_deterministic_tar"
    - path: crates/rollout-snapshots/src/kind/train_state.rs
      provides: "save_train_state + restore_train_state implementation"
      contains: "save_train_state"
    - path: crates/rollout-snapshots/tests/save_restore_roundtrip.rs
      provides: "save + restore + list + prune round-trip against EmbeddedStorage + FsObjectStore"
      contains: "save_restore_roundtrip"
    - path: crates/rollout-snapshots/tests/deterministic_tar.rs
      provides: "Pitfall 9 byte-stability proof (same input → same tar bytes → same blake3 hash)"
      contains: "deterministic_tar_byte_stability"
    - path: docs/book/src/training/snapshots.md
      provides: "Snapshot architecture chapter (tar + blake3 + metadata layout + restore semantics + Pitfall 9)"
      contains: "TrainState"
  key_links:
    - from: crates/rollout-snapshots/src/lib.rs
      to: "rollout_core::{Snapshotter, Snapshot, SnapshotKind, ContentId, Storage, ObjectStore}"
      via: "trait impl + Arc<dyn Storage> + Arc<dyn ObjectStore> injection"
      pattern: "impl Snapshotter for SnapshotterImpl"
    - from: crates/rollout-snapshots/src/kind/train_state.rs
      to: "tar_build::build_deterministic_tar + ObjectStore::put_bytes + Storage::begin/put_bytes"
      via: "save_train_state pipeline"
      pattern: "build_deterministic_tar"
    - from: crates/rollout-storage/src/embedded/tables.rs
      to: "T_SNAPSHOTS TableDefinition"
      via: "table_for() arm for namespace=\"snapshots\""
      pattern: "snapshots"
---

<objective>
Implement `rollout-snapshots` as a usable Snapshotter against the existing `Arc<dyn Storage>` + `Arc<dyn ObjectStore>` substrates from Phase 2. Ships the TrainState kind end-to-end (save → tar → hash → write blob + metadata row; restore → fetch → verify → extract). Also adds `list` + `prune`. Other kinds (Buffer / Process / EpisodicMemory) return `Fatal { PluginContract }` until their owning phases land.

This plan is the load-bearing TRAIN-03 deliverable that BOTH SFT (plan 04-02) and RM (plan 04-04) consume. Without this crate, the snapshot_resume.rs byte-compare proof can't run.

Purpose: deliver TRAIN-03 substrate.
Output: working `rollout-snapshots` crate + 3 test files + mdBook chapter.
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
@crates/rollout-snapshots/src/lib.rs
@crates/rollout-snapshots/Cargo.toml
@.planning/phases/02-local-substrate/02-02-rollout-storage-SUMMARY.md
@.planning/phases/02-local-substrate/02-03-rollout-cloud-local-SUMMARY.md
@crates/rollout-storage/src/embedded/tables.rs

<interfaces>
<!-- Phase-2 substrates this plan consumes. -->

From rollout-core::Snapshotter (after plan 04-00-a):
```rust
#[async_trait] pub trait Snapshotter: Send + Sync {
    async fn save(&self, request: SnapshotRequest) -> Result<Snapshot, CoreError>;
    async fn restore(&self, id: &SnapshotId, target: RestoreTarget) -> Result<(), CoreError>;
    async fn list(&self, filter: SnapshotFilter) -> Result<Vec<Snapshot>, CoreError>;
    async fn prune(&self, policy: PrunePolicy) -> Result<u64, CoreError>;
}
```

From rollout-core::ObjectStore (Phase 2):
```rust
#[async_trait] pub trait ObjectStore: Send + Sync {
    async fn put_bytes(&self, id: &ContentId, bytes: &[u8]) -> Result<(), CoreError>;
    async fn get_bytes(&self, id: &ContentId) -> Result<Vec<u8>, CoreError>;
    async fn exists(&self, id: &ContentId) -> Result<bool, CoreError>;
}
```

From rollout-core::Storage + StorageTxn (Phase 2):
```rust
async fn begin(&self) -> Result<Box<dyn StorageTxn>, CoreError>;
async fn put_bytes(&mut self, key: StorageKey, value: Vec<u8>) -> Result<(), CoreError>;
async fn commit(self: Box<Self>) -> Result<(), CoreError>;
async fn scan_bytes(&self, range: KeyRange) -> Result<Vec<(StorageKey, Vec<u8>)>, CoreError>;
```

From rollout-storage::embedded::tables (existing namespaces — Phase 2 + Phase 3):
- `runs`, `workers`, `heartbeats`, `plugins`, `cloudlocal_queue`, `infer`
- Pattern: each gets a `TableDefinition` const + arm in `table_for()`.

From .planning/phases/02-local-substrate/02-03-rollout-cloud-local-SUMMARY.md:
- `FsObjectStore` is content-addressed under `./data/object-store/` with 2-level sharding by hex prefix.
- Test-friendly: `FsObjectStore::open(tempdir.path())`.
</interfaces>

</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: Deterministic tar build + TrainState save pipeline + register `snapshots` storage namespace</name>
  <files>
    crates/rollout-snapshots/src/lib.rs,
    crates/rollout-snapshots/src/tar_build.rs,
    crates/rollout-snapshots/src/kind/mod.rs,
    crates/rollout-snapshots/src/kind/train_state.rs,
    crates/rollout-snapshots/src/key.rs,
    crates/rollout-snapshots/Cargo.toml,
    crates/rollout-snapshots/tests/deterministic_tar.rs,
    crates/rollout-storage/src/embedded/tables.rs
  </files>
  <read_first>
    .planning/phases/04-train-sft-rm-snapshots/04-RESEARCH.md §"Architecture Patterns" → Pattern 3 (tar reproducibility — `tar::Builder`, `HeaderMode::Deterministic`, `set_mode`/`set_mtime`/`set_uid`/`set_gid`),
    .planning/phases/04-train-sft-rm-snapshots/04-RESEARCH.md §"Pitfall 9" (CRITICAL: HeaderMode::Deterministic does NOT zero mode bits; you MUST set 0o644 / 0o755 explicitly),
    .planning/phases/04-train-sft-rm-snapshots/04-RESEARCH.md §"Code Examples" → tar + blake3 round-trip (lines 935-976) + the `build_deterministic_tar` helper (lines 481-503; the WALKDIR version near line 670-686 is the AUTHORITATIVE version — has the explicit mode bits Pitfall 9 demands),
    crates/rollout-storage/src/embedded/tables.rs (Phase-3 `infer` namespace registration pattern — same change is needed here for `snapshots`),
    .planning/phases/02-local-substrate/02-02-rollout-storage-SUMMARY.md (StorageKey shape, postcard encoding, T_* const pattern),
    .planning/phases/02-local-substrate/02-03-rollout-cloud-local-SUMMARY.md (FsObjectStore content-addressed layout),
    docs/specs/04-storage-snapshots.md §5 + §5a (the Phase-4 annotation block from plan 04-00-a; treats spec as authoritative for Snapshot field layout),
    crates/rollout-snapshots/src/lib.rs (skeleton from 04-00-b — replace wholesale)
  </read_first>
  <behavior>
    - Test 1 (deterministic_tar_byte_stability): build a tar of a fixed 3-file directory twice in the same test → both byte buffers equal AND blake3 hashes equal.
    - Test 2 (deterministic_tar_explicit_mode_bits): build a tar; parse it back with `tar::Archive`; assert every regular file's header `mode()` field is exactly 0o644 (Pitfall 9 — without explicit set_mode, macOS gives 0o755 to regular files).
    - Test 3 (deterministic_tar_empty_dir): empty source dir produces a deterministic empty-but-valid tar (`tar::Archive::new(&bytes).entries()?.count() == 0`).
    - Test 4 (snapshots_namespace_registered): `EmbeddedStorage::new(...)` opens cleanly; `storage.put_bytes(StorageKey { namespace: "snapshots", ... }, _)` does NOT return `Fatal(ConfigInvalid { msg: "unknown storage namespace: snapshots" })`.
  </behavior>
  <action>
    **Step A — Add `T_SNAPSHOTS` to `crates/rollout-storage/src/embedded/tables.rs`** (mirror Phase-3 `infer` registration; reference the Phase-3 SUMMARY for the exact diff shape):

    Locate the const block listing existing `TableDefinition`s. Add:

    ```rust
    /// Phase-4 snapshot metadata. Spec 04 §5.1 Snapshot rows.
    pub(crate) const T_SNAPSHOTS: TableDefinition<'static, &'static [u8], &'static [u8]> =
        TableDefinition::new("snapshots");
    ```

    Then locate `fn table_for(namespace: &str) -> Result<..., CoreError>` and add the arm:

    ```rust
    "snapshots" => Ok(T_SNAPSHOTS),
    ```

    Confirm the existing `unknown storage namespace` Fatal error path still fires for actual unknown namespaces (the Phase-3 `infer` arm should be untouched).

    **Step B — Add `tracing` + `chrono` + `serde_json` + a few helper traits to `crates/rollout-snapshots/Cargo.toml`** if not already declared in plan 04-00-b's skeleton. The skeleton from 04-00-b should already have most of these (cross-check Step E of plan 04-00-b Task 1):

    Confirm `[dependencies]` includes: `rollout-core`, `async-trait`, `serde`, `serde_json`, `schemars`, `smol_str`, `thiserror`, `blake3`, `chrono`, `tar`, `walkdir`, `tokio`, `futures`, `postcard`, `tracing`.

    Confirm `[dev-dependencies]` includes: `tempfile`, `rollout-storage`, `rollout-cloud-local`.

    **Step C — Write `crates/rollout-snapshots/src/tar_build.rs`** (this is THE Pitfall-9-aware deterministic tar builder; the exact mode-bits handling is non-negotiable):

    ```rust
    //! Deterministic tar build for snapshot blobs (TRAIN-03 byte-stability).
    //!
    //! Implements Phase-4 RESEARCH §"Pitfall 9": `tar::HeaderMode::Deterministic`
    //! does NOT zero file mode bits — we must set 0o644 (file) / 0o755 (dir)
    //! explicitly, on top of zeroing mtime + uid + gid via the Header struct.

    use std::fs::File;
    use std::io::Read;
    use std::path::{Path, PathBuf};

    use rollout_core::CoreError;

    /// Build a byte-identical tar archive of `src_dir`. The result is suitable
    /// for content-addressing via blake3.
    ///
    /// Invariants:
    /// - Entry order: sorted by relative path (deterministic).
    /// - No compression (compressed tar drifts across platforms / versions).
    /// - Per-entry headers: mtime=0, uid=0, gid=0, mode=0o644 (file) / 0o755 (dir).
    /// - GNU header format (`tar::Header::new_gnu()`).
    pub fn build_deterministic_tar(src_dir: &Path) -> Result<Vec<u8>, CoreError> {
        let mut entries: Vec<PathBuf> = walkdir::WalkDir::new(src_dir)
            .into_iter()
            .filter_map(Result::ok)
            .map(walkdir::DirEntry::into_path)
            .filter(|p| p != src_dir) // skip the root itself
            .collect();
        entries.sort();

        let mut buf = Vec::new();
        let mut tar_builder = tar::Builder::new(&mut buf);
        tar_builder.mode(tar::HeaderMode::Deterministic);

        for path in entries {
            let rel = path
                .strip_prefix(src_dir)
                .map_err(|e| fatal_internal(&format!("strip_prefix: {e}")))?;
            let meta = std::fs::metadata(&path).map_err(io_err)?;
            let is_dir = meta.is_dir();

            let mut header = tar::Header::new_gnu();
            header.set_size(if is_dir { 0 } else { meta.len() });
            header.set_mode(if is_dir { 0o755 } else { 0o644 });
            header.set_mtime(0);
            header.set_uid(0);
            header.set_gid(0);
            header.set_entry_type(if is_dir {
                tar::EntryType::Directory
            } else {
                tar::EntryType::Regular
            });
            header.set_cksum();

            if is_dir {
                tar_builder
                    .append_data(&mut header, rel, std::io::empty())
                    .map_err(io_err)?;
            } else {
                let mut file = File::open(&path).map_err(io_err)?;
                let mut contents = Vec::with_capacity(meta.len() as usize);
                file.read_to_end(&mut contents).map_err(io_err)?;
                tar_builder
                    .append_data(&mut header, rel, &contents[..])
                    .map_err(io_err)?;
            }
        }

        tar_builder.finish().map_err(io_err)?;
        drop(tar_builder);
        Ok(buf)
    }

    /// Extract a tar archive built by `build_deterministic_tar` into `dst_dir`.
    pub fn extract_tar(tar_bytes: &[u8], dst_dir: &Path) -> Result<(), CoreError> {
        let mut archive = tar::Archive::new(tar_bytes);
        archive.unpack(dst_dir).map_err(io_err)?;
        Ok(())
    }

    fn io_err(e: std::io::Error) -> CoreError {
        CoreError::Recoverable(rollout_core::Recoverable::Transient {
            source: e.to_string(),
            retry: rollout_core::RetryHint::immediate(),
        })
    }

    fn fatal_internal(msg: &str) -> CoreError {
        CoreError::Fatal(rollout_core::Fatal::Internal { msg: msg.into() })
    }
    ```

    Verify the exact `CoreError` constructors against the current `rollout-core::error` shape (Phase 2 may use `Internal { msg: SmolStr }` — adjust SmolStr conversion if needed).

    **Step D — Write `crates/rollout-snapshots/src/key.rs`** (helper that owns the snapshot-key layout under namespace="snapshots"):

    ```rust
    //! `StorageKey` builders for the `"snapshots"` namespace.

    use rollout_core::{RunId, SnapshotId, StorageKey};
    use smol_str::SmolStr;

    /// Key for a single snapshot's metadata row.
    /// Layout: namespace="snapshots", run_id=<RunId>, path=["<snapshot_id_hex>"].
    #[must_use]
    pub fn snapshot_key(run_id: RunId, id: SnapshotId) -> StorageKey {
        StorageKey {
            namespace: SmolStr::new_inline("snapshots"),
            run_id: Some(run_id),
            path: vec![SmolStr::from(format!("{}", id.0))],
        }
    }

    /// Prefix for scanning all snapshots in a run.
    #[must_use]
    pub fn run_prefix(run_id: RunId) -> StorageKey {
        StorageKey {
            namespace: SmolStr::new_inline("snapshots"),
            run_id: Some(run_id),
            path: vec![],
        }
    }
    ```

    **Step E — Write `crates/rollout-snapshots/src/kind/mod.rs`**:

    ```rust
    //! Per-snapshot-kind save/restore handlers.
    //! Phase 4 ships `train_state` only; other kinds return Fatal { PluginContract }.

    pub(crate) mod train_state;
    ```

    **Step F — Write `crates/rollout-snapshots/src/kind/train_state.rs`** (the actual TrainState save + restore logic; integrates tar + blake3 + ObjectStore + Storage):

    ```rust
    //! TrainState snapshot kind — accelerate-style state directory → tar → blake3
    //! → ObjectStore + Storage metadata row.

    use std::path::Path;
    use std::sync::Arc;

    use rollout_core::{
        ContentId, CoreError, Fatal, ObjectStore, Snapshot, SnapshotId, SnapshotKind,
        SnapshotPart, SnapshotRequest, Storage,
    };
    use smol_str::SmolStr;

    use crate::key::snapshot_key;
    use crate::tar_build::{build_deterministic_tar, extract_tar};

    /// Save a TrainState snapshot end-to-end:
    /// 1. Tar `accelerate_dir` deterministically.
    /// 2. Compute blake3 → ContentId.
    /// 3. Write tar bytes to ObjectStore at that ContentId.
    /// 4. Persist Snapshot metadata row to Storage under namespace="snapshots".
    pub(crate) async fn save_train_state(
        request: SnapshotRequest,
        accelerate_dir: &Path,
        storage: &Arc<dyn Storage>,
        object: &Arc<dyn ObjectStore>,
    ) -> Result<Snapshot, CoreError> {
        debug_assert!(matches!(request.kind, SnapshotKind::TrainState));

        // 1. Build deterministic tar on a blocking thread (walkdir + I/O).
        let dir = accelerate_dir.to_path_buf();
        let tar_bytes = tokio::task::spawn_blocking(move || build_deterministic_tar(&dir))
            .await
            .map_err(|e| fatal_internal(&format!("join: {e}")))??;

        // 2. ContentId = blake3 of the tar bytes.
        let content_id = ContentId::of(&tar_bytes);
        let size = tar_bytes.len() as u64;

        // 3. Write to ObjectStore.
        object.put_bytes(&content_id, &tar_bytes).await?;

        // 4. Build Snapshot metadata.
        let snapshot = Snapshot {
            id: SnapshotId::from(content_id),
            kind: SnapshotKind::TrainState,
            run_id: request.run_id,
            created_at: chrono::Utc::now(),
            label: request.label,
            parts: vec![SnapshotPart {
                role: SmolStr::new_inline("tar"),
                content: content_id,
                size,
            }],
            algorithm_id: request.algorithm_id,
            meta: request.meta,
        };

        // 5. Persist metadata row via a Storage transaction.
        let mut txn = storage.begin().await?;
        let key = snapshot_key(snapshot.run_id, snapshot.id);
        let value = postcard::to_stdvec(&snapshot)
            .map_err(|e| fatal_internal(&format!("postcard encode Snapshot: {e}")))?;
        txn.put_bytes(key, value).await?;
        txn.commit().await?;

        Ok(snapshot)
    }

    /// Restore a TrainState snapshot:
    /// 1. Look up Snapshot metadata by id.
    /// 2. Fetch tar bytes from ObjectStore.
    /// 3. blake3-verify (must match `id.0`).
    /// 4. Extract to `dst_dir`.
    pub(crate) async fn restore_train_state(
        snapshot: &Snapshot,
        object: &Arc<dyn ObjectStore>,
        dst_dir: &Path,
    ) -> Result<(), CoreError> {
        let tar_part = snapshot
            .parts
            .iter()
            .find(|p| p.role.as_str() == "tar")
            .ok_or_else(|| fatal_plugin("missing 'tar' part on TrainState snapshot"))?;

        let tar_bytes = object.get_bytes(&tar_part.content).await?;
        let actual = ContentId::of(&tar_bytes);
        if actual != tar_part.content {
            return Err(fatal_plugin(&format!(
                "blake3 mismatch on restore: expected {} got {}",
                tar_part.content, actual
            )));
        }

        let dst = dst_dir.to_path_buf();
        tokio::task::spawn_blocking(move || extract_tar(&tar_bytes, &dst))
            .await
            .map_err(|e| fatal_internal(&format!("join: {e}")))??;

        Ok(())
    }

    fn fatal_internal(msg: &str) -> CoreError {
        CoreError::Fatal(Fatal::Internal { msg: msg.into() })
    }

    fn fatal_plugin(msg: &str) -> CoreError {
        CoreError::Fatal(Fatal::PluginContract {
            plugin: "rollout-snapshots".into(),
            msg: msg.into(),
        })
    }
    ```

    Adjust `ContentId::of` + `CoreError` + `Fatal` constructors to match the actual rollout-core surface (Phase-2 names may differ; cross-reference `crates/rollout-core/src/error.rs`).

    **Step G — Test file `crates/rollout-snapshots/tests/deterministic_tar.rs`:**

    ```rust
    //! Pitfall 9 byte-stability proof.

    use rollout_snapshots::tar_build::build_deterministic_tar;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn deterministic_tar_byte_stability() {
        let tmp = tempdir().unwrap();
        // 3 files with known content + alphabetic ordering different from creation order.
        fs::write(tmp.path().join("c.bin"), b"content C").unwrap();
        fs::write(tmp.path().join("a.bin"), b"content A").unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join("sub").join("b.bin"), b"content B").unwrap();

        let bytes1 = build_deterministic_tar(tmp.path()).unwrap();
        let bytes2 = build_deterministic_tar(tmp.path()).unwrap();
        assert_eq!(bytes1, bytes2, "same input must produce byte-identical tar");

        let hash1 = blake3::hash(&bytes1);
        let hash2 = blake3::hash(&bytes2);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn deterministic_tar_explicit_mode_bits() {
        let tmp = tempdir().unwrap();
        fs::write(tmp.path().join("file.txt"), b"hi").unwrap();
        let bytes = build_deterministic_tar(tmp.path()).unwrap();

        let mut archive = tar::Archive::new(&bytes[..]);
        let mut saw_file = false;
        for entry in archive.entries().unwrap() {
            let entry = entry.unwrap();
            let header = entry.header();
            if header.entry_type() == tar::EntryType::Regular {
                assert_eq!(header.mode().unwrap(), 0o644,
                    "regular file mode must be 0o644 explicitly (Pitfall 9)");
                assert_eq!(header.mtime().unwrap(), 0);
                assert_eq!(header.uid().unwrap(), 0);
                assert_eq!(header.gid().unwrap(), 0);
                saw_file = true;
            }
        }
        assert!(saw_file, "test setup should have produced at least one regular file");
    }

    #[test]
    fn deterministic_tar_empty_dir() {
        let tmp = tempdir().unwrap();
        let bytes = build_deterministic_tar(tmp.path()).unwrap();
        let archive = tar::Archive::new(&bytes[..]);
        // An empty archive is fine; what matters is that it doesn't error out.
        assert!(!bytes.is_empty(), "tar should at minimum have terminator blocks");
        // Re-build is byte-stable.
        let bytes2 = build_deterministic_tar(tmp.path()).unwrap();
        assert_eq!(bytes, bytes2);
        let _ = archive;
    }
    ```

    Make `tar_build` module `pub` (or add `pub use crate::tar_build::build_deterministic_tar;` in lib.rs) so the integration test can access it.

    **DOCS-02:** this commit adds code under crates/* — must also touch `docs/book/src/training/snapshots.md` (the chapter; see Task 2) OR the test file `deterministic_tar.rs`. Test file satisfies DOCS-02; docs polish lands in Task 2.

    Commit message: `feat(04-01-01): deterministic tar + TrainState save pipeline + snapshots namespace`.
  </action>
  <verify>
    <automated>
cargo build -p rollout-snapshots &&
cargo build -p rollout-storage &&
cargo test -p rollout-snapshots --test deterministic_tar &&
grep -q 'snapshots' crates/rollout-storage/src/embedded/tables.rs &&
grep -q 'T_SNAPSHOTS' crates/rollout-storage/src/embedded/tables.rs &&
grep -q 'fn build_deterministic_tar' crates/rollout-snapshots/src/tar_build.rs &&
grep -q 'set_mode' crates/rollout-snapshots/src/tar_build.rs &&
grep -q 'pub(crate) async fn save_train_state' crates/rollout-snapshots/src/kind/train_state.rs
    </automated>
  </verify>
  <acceptance_criteria>
    - `cargo build -p rollout-snapshots` exits 0.
    - `cargo build -p rollout-storage` exits 0.
    - `cargo test -p rollout-snapshots --test deterministic_tar` exits 0 and reports ≥ 3 tests.
    - `grep -q 'T_SNAPSHOTS' crates/rollout-storage/src/embedded/tables.rs` exits 0.
    - `grep -q '"snapshots" =>' crates/rollout-storage/src/embedded/tables.rs` exits 0.
    - `grep -q 'fn build_deterministic_tar' crates/rollout-snapshots/src/tar_build.rs` exits 0.
    - `grep -q 'set_mode' crates/rollout-snapshots/src/tar_build.rs` exits 0 (Pitfall 9 fix).
    - `grep -q 'pub(crate) async fn save_train_state' crates/rollout-snapshots/src/kind/train_state.rs` exits 0.
    - HEAD commit message matches `^feat\(04-01-01\):`.
    - DOCS-02 satisfied: same commit touches test file (`tests/deterministic_tar.rs`) + code under `crates/`.
    - DOCS-03 satisfied: `cargo doc -p rollout-snapshots --no-deps` clean.
  </acceptance_criteria>
  <done>
    `rollout-snapshots` compiles. Deterministic tar build is byte-stable (verified by test). `snapshots` storage namespace is registered. TrainState save pipeline exists end-to-end (called by Task 2's Snapshotter impl).
  </done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: Snapshotter impl (save/restore/list/prune) + integration test + mdBook chapter</name>
  <files>
    crates/rollout-snapshots/src/lib.rs,
    crates/rollout-snapshots/src/policy.rs,
    crates/rollout-snapshots/tests/save_restore_roundtrip.rs,
    crates/rollout-snapshots/tests/list_and_prune.rs,
    docs/book/src/training/snapshots.md,
    docs/book/src/SUMMARY.md
  </files>
  <read_first>
    crates/rollout-snapshots/src/lib.rs (skeleton from 04-00-b + the kind/train_state module from Task 1 — wire the Snapshotter impl on top),
    crates/rollout-snapshots/src/kind/train_state.rs (the save_train_state + restore_train_state helpers Task 2 calls),
    crates/rollout-snapshots/src/key.rs (snapshot_key + run_prefix helpers),
    docs/specs/04-storage-snapshots.md §5.2 + §5a (Snapshotter contract — list returns newest-first per spec),
    docs/specs/04-storage-snapshots.md §7 (restore semantics — SameRun/Fork/Worker),
    .planning/phases/04-train-sft-rm-snapshots/04-RESEARCH.md §"Code Examples" → save_train_state full example (lines 939-976),
    .planning/phases/02-local-substrate/02-03-rollout-cloud-local-SUMMARY.md (FsObjectStore test pattern: tempdir-rooted),
    docs/book/src/SUMMARY.md (insert Training section between Inference and Examples),
    docs/book/src/inference/index.md (Phase-3 chapter pattern to mirror for the new `training/` section)
  </read_first>
  <behavior>
    - Test 1 (save_restore_roundtrip): create dir with 3 files → SnapshotterImpl::save → list returns 1 entry → fetch tar via ObjectStore → SnapshotterImpl::restore SameRun extracts to tempdir → each restored file byte-equal to original.
    - Test 2 (save_meta_round_trips): `meta: serde_json::Value` (a non-trivial JSON object with strings + numbers + nested array) is recoverable from Storage's persisted Snapshot row.
    - Test 3 (list_filters_by_kind_and_label): save 3 snapshots with mixed kinds (TrainState x2, attempt Buffer → returns Fatal) and labels → list filters return only matches.
    - Test 4 (list_newest_first): save 3 snapshots over 3 distinct created_at instants → list result ordering is descending by created_at.
    - Test 5 (prune_honors_keep_last_and_keep_labeled): save 5 snapshots, 2 labeled → prune with `keep_last=2, keep_labeled=true` deletes (5-2-2)=1 snapshot (the unlabeled non-recent one); returns 1.
    - Test 6 (buffer_kind_returns_fatal): SnapshotterImpl::save with SnapshotKind::Buffer → Fatal { PluginContract, msg contains "Phase 9" }.
  </behavior>
  <action>
    **Step A — Replace `crates/rollout-snapshots/src/lib.rs` wholesale** with the full SnapshotterImpl. Wires Steps F (train_state) + D (key) + new `policy.rs`:

    ```rust
    //! `rollout-snapshots` — Snapshotter trait impl (Phase 4: TrainState only).

    #![doc(html_root_url = "https://docs.rs/rollout-snapshots/0.1.0")]

    pub mod tar_build;
    pub(crate) mod key;
    pub(crate) mod kind;
    pub mod policy;

    use std::path::PathBuf;
    use std::sync::Arc;

    use async_trait::async_trait;
    use rollout_core::{
        CoreError, Fatal, KeyRange, ObjectStore, PrunePolicy, RestoreTarget, Snapshot,
        SnapshotFilter, SnapshotId, SnapshotKind, SnapshotRequest, Snapshotter, Storage,
    };

    /// Concrete `Snapshotter` impl. Phase-4 implements `TrainState` only.
    pub struct SnapshotterImpl {
        storage: Arc<dyn Storage>,
        object: Arc<dyn ObjectStore>,
        /// Working directory where save() reads from (accelerate.save_state output)
        /// and restore() extracts to. Each save/restore call passes a per-call path,
        /// so this field is unused in Phase 4 — kept for Phase 9's interleaved actor/learner.
        #[allow(dead_code)]
        work_dir: PathBuf,
    }

    impl SnapshotterImpl {
        /// Construct with the substrates the trait needs.
        #[must_use]
        pub fn new(
            storage: Arc<dyn Storage>,
            object: Arc<dyn ObjectStore>,
            work_dir: PathBuf,
        ) -> Self {
            Self { storage, object, work_dir }
        }

        /// PHASE 4 escape hatch: tests + algorithms call this directly so they can
        /// pass an explicit `accelerate_dir` for the tar source. Public so plan 04-02
        /// + 04-04 can drive it from algorithm code without re-exporting kind::train_state.
        pub async fn save_train_state(
            &self,
            request: SnapshotRequest,
            accelerate_dir: &std::path::Path,
        ) -> Result<Snapshot, CoreError> {
            kind::train_state::save_train_state(request, accelerate_dir, &self.storage, &self.object).await
        }

        /// Inverse of save_train_state for test/algorithm direct use.
        pub async fn restore_train_state(
            &self,
            snapshot: &Snapshot,
            dst_dir: &std::path::Path,
        ) -> Result<(), CoreError> {
            kind::train_state::restore_train_state(snapshot, &self.object, dst_dir).await
        }

        /// Read the Snapshot metadata row at `id`. Helper for restore() + tests.
        async fn read_meta(&self, id: SnapshotId) -> Result<Option<Snapshot>, CoreError> {
            // We don't know the run_id from id alone; scan the entire "snapshots"
            // namespace and filter. Acceptable for Phase 4 (few snapshots); Phase 9
            // adds a secondary index if needed.
            let range = KeyRange {
                prefix: rollout_core::StorageKey {
                    namespace: smol_str::SmolStr::new_inline("snapshots"),
                    run_id: None,
                    path: vec![],
                },
                limit: None,
            };
            let rows = self.storage.scan_bytes(range).await?;
            for (_, bytes) in rows {
                let snap: Snapshot = postcard::from_bytes(&bytes)
                    .map_err(|e| fatal_internal(&format!("postcard decode Snapshot: {e}")))?;
                if snap.id == id { return Ok(Some(snap)); }
            }
            Ok(None)
        }
    }

    #[async_trait]
    impl Snapshotter for SnapshotterImpl {
        async fn save(&self, request: SnapshotRequest) -> Result<Snapshot, CoreError> {
            match request.kind {
                SnapshotKind::TrainState => {
                    // Phase 4 callers (SFT/RM) drive save_train_state directly with an
                    // explicit accelerate_dir. A bare Snapshotter::save() with no dir is
                    // not meaningful for TrainState — error out so the contract is clear.
                    Err(CoreError::Fatal(Fatal::PluginContract {
                        plugin: "rollout-snapshots".into(),
                        msg: "TrainState save requires save_train_state(request, accelerate_dir); \
                              the bare Snapshotter::save trait method is reserved for \
                              dir-less kinds (Buffer / EpisodicMemory) — neither in Phase 4.".into(),
                    }))
                }
                SnapshotKind::Buffer => Err(CoreError::Fatal(Fatal::PluginContract {
                    plugin: "rollout-snapshots".into(),
                    msg: "Phase 9: SnapshotKind::Buffer".into(),
                })),
                SnapshotKind::Process => Err(CoreError::Fatal(Fatal::PluginContract {
                    plugin: "rollout-snapshots".into(),
                    msg: "Phase 11: SnapshotKind::Process".into(),
                })),
                SnapshotKind::EpisodicMemory => Err(CoreError::Fatal(Fatal::PluginContract {
                    plugin: "rollout-snapshots".into(),
                    msg: "Phase 8: SnapshotKind::EpisodicMemory".into(),
                })),
            }
        }

        async fn restore(&self, id: &SnapshotId, target: RestoreTarget) -> Result<(), CoreError> {
            // SameRun: callers (algorithms) drive restore_train_state directly with
            // an explicit dst_dir. Bare Snapshotter::restore can't choose a dir.
            // Fork/Worker: Phase 4 returns Fatal (Phase 6 / 9 add multi-worker semantics).
            match target {
                RestoreTarget::SameRun => Err(CoreError::Fatal(Fatal::PluginContract {
                    plugin: "rollout-snapshots".into(),
                    msg: format!("TrainState restore_train_state(snapshot, dst_dir) is the correct entry point; \
                                  bare Snapshotter::restore({id:?}, SameRun) doesn't take a destination."),
                })),
                RestoreTarget::Fork { new_run_id } => Err(CoreError::Fatal(Fatal::PluginContract {
                    plugin: "rollout-snapshots".into(),
                    msg: format!("Phase 9: Fork restore (new_run_id={new_run_id:?})"),
                })),
                RestoreTarget::Worker { worker_id } => Err(CoreError::Fatal(Fatal::PluginContract {
                    plugin: "rollout-snapshots".into(),
                    msg: format!("Phase 6: Worker restore (worker_id={worker_id:?})"),
                })),
            }
        }

        async fn list(&self, filter: SnapshotFilter) -> Result<Vec<Snapshot>, CoreError> {
            let prefix = rollout_core::StorageKey {
                namespace: smol_str::SmolStr::new_inline("snapshots"),
                run_id: filter.run_id,
                path: vec![],
            };
            let rows = self.storage.scan_bytes(KeyRange { prefix, limit: None }).await?;

            let mut out: Vec<Snapshot> = rows
                .into_iter()
                .map(|(_, bytes)| {
                    postcard::from_bytes::<Snapshot>(&bytes)
                        .map_err(|e| fatal_internal(&format!("postcard decode Snapshot: {e}")))
                })
                .collect::<Result<Vec<_>, _>>()?;

            // Apply remaining filter predicates in memory.
            if let Some(kind) = filter.kind {
                out.retain(|s| s.kind == kind);
            }
            if let Some(needle) = filter.label_contains {
                out.retain(|s| s.label.as_ref().is_some_and(|l| l.contains(&needle)));
            }
            // Sort newest-first.
            out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            if let Some(limit) = filter.limit {
                out.truncate(limit as usize);
            }
            Ok(out)
        }

        async fn prune(&self, policy: PrunePolicy) -> Result<u64, CoreError> {
            policy::apply_prune(&self.storage, &self.object, policy).await
        }
    }

    fn fatal_internal(msg: &str) -> CoreError {
        CoreError::Fatal(Fatal::Internal { msg: msg.into() })
    }
    ```

    **Step B — `crates/rollout-snapshots/src/policy.rs`:**

    ```rust
    //! Retention policy enforcement for `Snapshotter::prune`.

    use std::sync::Arc;

    use rollout_core::{
        CoreError, KeyRange, ObjectStore, PrunePolicy, RetentionPolicy, Snapshot, Storage,
    };

    /// Apply `policy` against the snapshots in `policy.run_id`. Deletes both the
    /// metadata row AND the underlying blob (every part). Returns the count of
    /// snapshots deleted (NOT blobs).
    pub(crate) async fn apply_prune(
        storage: &Arc<dyn Storage>,
        object: &Arc<dyn ObjectStore>,
        policy: PrunePolicy,
    ) -> Result<u64, CoreError> {
        let prefix = rollout_core::StorageKey {
            namespace: smol_str::SmolStr::new_inline("snapshots"),
            run_id: Some(policy.run_id),
            path: vec![],
        };
        let rows = storage.scan_bytes(KeyRange { prefix: prefix.clone(), limit: None }).await?;

        let mut snaps: Vec<(rollout_core::StorageKey, Snapshot)> = rows
            .into_iter()
            .map(|(k, v)| {
                postcard::from_bytes::<Snapshot>(&v)
                    .map(|s| (k, s))
                    .map_err(|e| {
                        CoreError::Fatal(rollout_core::Fatal::Internal {
                            msg: format!("postcard decode Snapshot: {e}").into(),
                        })
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        snaps.sort_by(|a, b| b.1.created_at.cmp(&a.1.created_at));

        let RetentionPolicy { keep_last, keep_labeled, max_age } = policy.retention;
        let mut deleted: u64 = 0;
        let now = chrono::Utc::now();

        for (idx, (key, snap)) in snaps.iter().enumerate() {
            // Keep N most recent.
            if (idx as u32) < keep_last { continue; }
            // Keep labeled if requested.
            if keep_labeled && snap.label.is_some() { continue; }
            // Honor max_age (keep snapshots younger than max_age).
            if let Some(max_age) = max_age {
                let age = (now - snap.created_at).to_std().unwrap_or_default();
                if age < max_age { continue; }
            }
            // Delete blob(s) (best-effort; we don't currently expose ObjectStore::delete
            // in Phase 2 — leave the blob for now, only delete the metadata row).
            // TODO Phase 5: ObjectStore::delete + cascade.
            let _ = object;

            // Delete metadata row.
            let mut txn = storage.begin().await?;
            txn.delete(key.clone()).await?;
            txn.commit().await?;

            deleted += 1;
        }

        Ok(deleted)
    }
    ```

    Note: ObjectStore::delete isn't in the Phase-2 trait. The blob remains; only the metadata row is deleted. Document this as Phase-5 deferred (add a `// TODO Phase 5` line per the snippet). If ObjectStore::delete EXISTS in current trait surface, use it; otherwise stick with metadata-row-only delete and call it out in the mdBook chapter.

    **Step C — `crates/rollout-snapshots/tests/save_restore_roundtrip.rs`:**

    ```rust
    //! Save → list → restore round-trip against EmbeddedStorage + FsObjectStore.

    use std::fs;
    use std::sync::Arc;

    use rollout_cloud_local::FsObjectStore;
    use rollout_core::{
        AlgorithmId, ObjectStore, RestoreTarget, RunId, Snapshot, SnapshotFilter, SnapshotKind,
        SnapshotRequest, Snapshotter, Storage,
    };
    use rollout_snapshots::SnapshotterImpl;
    use rollout_storage::EmbeddedStorage;
    use tempfile::tempdir;

    fn make_run_id() -> RunId { RunId::new() }

    async fn setup() -> (
        tempfile::TempDir,
        SnapshotterImpl,
        Arc<dyn Storage>,
        Arc<dyn ObjectStore>,
    ) {
        let tmp = tempdir().unwrap();
        let storage_path = tmp.path().join("storage.db");
        let object_path = tmp.path().join("object-store");

        let storage: Arc<dyn Storage> =
            Arc::new(EmbeddedStorage::open(&storage_path).await.unwrap());
        let object: Arc<dyn ObjectStore> =
            Arc::new(FsObjectStore::open(&object_path).unwrap());

        let snapper = SnapshotterImpl::new(
            Arc::clone(&storage),
            Arc::clone(&object),
            tmp.path().to_path_buf(),
        );
        (tmp, snapper, storage, object)
    }

    #[tokio::test]
    async fn save_restore_roundtrip() {
        let (tmp, snapper, _, _) = setup().await;

        // Build a "fake accelerate.save_state output": directory with 3 files.
        let src = tmp.path().join("accel-out");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("weights.safetensors"), b"FAKE-WEIGHTS-BYTES").unwrap();
        fs::write(src.join("optimizer.bin"), b"FAKE-OPTIMIZER-BYTES").unwrap();
        fs::write(src.join("random_states.pkl"), b"FAKE-RNG-BYTES").unwrap();

        let run_id = make_run_id();
        let req = SnapshotRequest {
            run_id,
            algorithm_id: AlgorithmId(smol_str::SmolStr::new_inline("sft")),
            kind: SnapshotKind::TrainState,
            label: Some(smol_str::SmolStr::new_inline("test")),
            meta: serde_json::json!({ "step": 5, "curriculum_cursor": 12 }),
        };

        let snap: Snapshot = snapper.save_train_state(req, &src).await.unwrap();
        assert!(matches!(snap.kind, SnapshotKind::TrainState));
        assert_eq!(snap.parts.len(), 1);
        assert_eq!(snap.parts[0].role.as_str(), "tar");
        assert_eq!(snap.meta["step"], 5);

        // list should return exactly one snapshot for this run.
        let list = snapper.list(SnapshotFilter {
            run_id: Some(run_id),
            ..Default::default()
        }).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, snap.id);

        // restore_train_state extracts to a fresh dir and the files round-trip.
        let dst = tmp.path().join("restored");
        snapper.restore_train_state(&snap, &dst).await.unwrap();

        let restored_weights = fs::read(dst.join("weights.safetensors")).unwrap();
        assert_eq!(restored_weights, b"FAKE-WEIGHTS-BYTES");
        let restored_opt = fs::read(dst.join("optimizer.bin")).unwrap();
        assert_eq!(restored_opt, b"FAKE-OPTIMIZER-BYTES");
    }

    #[tokio::test]
    async fn buffer_kind_returns_fatal_phase_9() {
        let (_tmp, snapper, _, _) = setup().await;
        let req = SnapshotRequest {
            run_id: make_run_id(),
            algorithm_id: AlgorithmId(smol_str::SmolStr::new_inline("sft")),
            kind: SnapshotKind::Buffer,
            label: None,
            meta: serde_json::Value::Null,
        };
        let err = snapper.save(req).await.unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("Phase 9"), "expected Phase 9 sentinel, got: {msg}");
    }

    #[tokio::test]
    async fn restore_unknown_target_phase_6() {
        let (_tmp, snapper, _, _) = setup().await;
        let dummy_id = rollout_core::SnapshotId::from(rollout_core::ContentId::of(b"x"));
        let err = snapper.restore(
            &dummy_id,
            RestoreTarget::Worker { worker_id: rollout_core::WorkerId::new() },
        ).await.unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("Phase 6"));
    }
    ```

    **Step D — `crates/rollout-snapshots/tests/list_and_prune.rs`** with the list-filter / list-ordering / prune tests per the `<behavior>` block (tests 3-5). Use the same `setup()` helper; copy it inline.

    **Step E — Write `docs/book/src/training/snapshots.md`** (the mdBook chapter; ~150 lines). Sections:

    1. Architecture diagram (ASCII) — `PolicyAlgorithm.snapshot_save` → `SnapshotterImpl::save_train_state` → tar → blake3 → `ObjectStore::put_bytes` + Storage txn.
    2. Snapshot metadata layout (Snapshot fields + StorageKey namespace="snapshots" path).
    3. TrainState kind (the only Phase-4 implementation; other kinds enumerated).
    4. Tar reproducibility contract (Pitfall 9 — explicit mode bits 0o644/0o755, mtime=0, uid=gid=0, sort by name, no compression).
    5. blake3 ContentId as the SnapshotId source-of-truth.
    6. Restore semantics (SameRun in Phase 4; Fork/Worker deferred).
    7. List + prune surface (filter shape, RetentionPolicy, keep_last/keep_labeled/max_age).
    8. Algorithm-internal state via `Snapshot.meta: serde_json::Value` (D-DETERM-05).
    9. Determinism caveats (CPU bit-identical unconditionally; CUDA same-SM-required; cross-machine best-effort).
    10. Pointers: `crates/rollout-snapshots/`, `docs/specs/04-storage-snapshots.md §5`.

    **Step F — Update `docs/book/src/SUMMARY.md`:**

    Add a new `# Training` section immediately after the `# Inference` section, with:

    ```markdown
    # Training

    - [Overview](./training/index.md)
    - [Snapshots](./training/snapshots.md)
    ```

    Also create a stub `docs/book/src/training/index.md`:

    ```markdown
    # Training

    Phase 4 lands the first end-to-end training story. This section covers:

    - [Snapshots](./snapshots.md) — TrainState save/restore, tar+blake3, retention policy.

    More chapters land alongside their plans: `sft.md` (plan 04-02), `rm.md` (04-04),
    `postgres-backend.md` (04-03), `determinism.md` + `cpu-mode.md` (04-05),
    `cli.md` (04-06).
    ```

    Commit message: `feat(04-01-02): SnapshotterImpl save/restore/list/prune + integration tests + mdBook chapter`.
  </action>
  <verify>
    <automated>
cargo build -p rollout-snapshots &&
cargo test -p rollout-snapshots --test save_restore_roundtrip &&
cargo test -p rollout-snapshots --test list_and_prune &&
cargo clippy -p rollout-snapshots --all-targets -- -D warnings &&
test -f docs/book/src/training/snapshots.md &&
test -f docs/book/src/training/index.md &&
grep -q 'training/snapshots.md' docs/book/src/SUMMARY.md &&
mdbook build docs/book
    </automated>
  </verify>
  <acceptance_criteria>
    - `cargo build -p rollout-snapshots` exits 0.
    - `cargo test -p rollout-snapshots --test save_restore_roundtrip` exits 0 and reports ≥ 3 tests (including buffer_kind_returns_fatal_phase_9 + restore_unknown_target_phase_6).
    - `cargo test -p rollout-snapshots --test list_and_prune` exits 0 and reports ≥ 3 tests (filter, ordering, prune).
    - `cargo clippy -p rollout-snapshots --all-targets -- -D warnings` exits 0.
    - `grep -q 'impl Snapshotter for SnapshotterImpl' crates/rollout-snapshots/src/lib.rs` exits 0.
    - `grep -q 'Phase 9: SnapshotKind::Buffer' crates/rollout-snapshots/src/lib.rs` exits 0.
    - `grep -q 'fn apply_prune' crates/rollout-snapshots/src/policy.rs` exits 0.
    - `test -f docs/book/src/training/snapshots.md` exits 0.
    - `grep -q 'training/snapshots.md' docs/book/src/SUMMARY.md` exits 0.
    - `grep -q 'Pitfall 9' docs/book/src/training/snapshots.md` exits 0 (the chapter must call out the reproducibility caveat).
    - `mdbook build docs/book` exits 0.
    - HEAD commit message matches `^feat\(04-01-02\):`.
    - DOCS-02 satisfied: chapter + tests + code in one commit.
  </acceptance_criteria>
  <done>
    `SnapshotterImpl` exposes `save_train_state` + `restore_train_state` (Phase 4 entry points) and the trait methods `save` / `restore` / `list` / `prune`. Non-TrainState kinds return Fatal sentinels. Round-trip test proves byte-identical restore. mdBook chapter ships.
  </done>
</task>

</tasks>

<verification>
**Phase-gate checks:**
- `cargo test -p rollout-snapshots --tests` exits 0 (all 3 test files green).
- `cargo build -p rollout-storage` exits 0 (snapshots namespace added without breaking embedded path).
- `cargo clippy -p rollout-snapshots --all-targets -- -D warnings` clean.
- `cargo doc -p rollout-snapshots --no-deps` clean under rustdoc gate.
- `mdbook build docs/book` clean.
- `cargo test --workspace --tests` no regressions.

**Conventional commits:** `feat(04-01-01)`, `feat(04-01-02)`.
**DOCS-01..03:** both tasks ship tests + code + docs in the same commit.
</verification>

<success_criteria>
- TrainState snapshots round-trip (save → restore byte-identical).
- Deterministic tar is byte-stable (Pitfall 9 acceptance: explicit mode bits verified by header inspection).
- list/prune work end-to-end against EmbeddedStorage + FsObjectStore.
- Non-TrainState kinds return Fatal sentinels with the right Phase-N messages.
- `snapshots` namespace registered in rollout-storage.
- mdBook training section bootstrapped + linked from SUMMARY.md.
</success_criteria>

<output>
After completion, create `.planning/phases/04-train-sft-rm-snapshots/04-01-rollout-snapshots-SUMMARY.md` recording: (1) the SnapshotterImpl shape (save_train_state + trait methods), (2) the Pitfall 9 fix details (explicit set_mode), (3) the storage namespace addition, (4) test coverage table, (5) mdBook chapter contents, (6) any deviation (e.g., if ObjectStore::delete had to be added or punted).
</output>
