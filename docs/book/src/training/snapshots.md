# Snapshots

`rollout-snapshots` ships `SnapshotterImpl`, the Phase-4 implementation of the
`rollout_core::Snapshotter` trait. It owns the persistence path for
`SnapshotKind::TrainState` — the only kind implemented in v1's training story.
Three other kinds (`Buffer`, `Process`, `EpisodicMemory`) compile but return
`Fatal { PluginContract, msg: "Phase N: <kind>" }` until their owning phases
land (9 / 11 / 8 respectively).

See `docs/specs/04-storage-snapshots.md` for the authoritative contract and
`crates/rollout-snapshots/` for the implementation.

## Architecture

```text
PolicyAlgorithm                  SnapshotterImpl                  Storage + ObjectStore
─────────────────────            ───────────────────              ──────────────────────
algo.snapshot_save  ──▶  save_train_state(request, dir)
                                │
                                ▼
                         build_deterministic_tar(dir)             (no I/O on substrate yet)
                                │
                                ▼
                         ContentId::of(tar_bytes) = blake3
                                │
                ┌───────────────┴────────────────┐
                ▼                                ▼
   ObjectStore::put_bytes(tar)          Storage::begin().txn
        (returns same ContentId)        put_bytes(snapshot_key, json(Snapshot))
                │                                │
                └────────► tar blob              └────────► snapshot row (namespace=snapshots)
```

Save returns a `Snapshot` whose `parts[0].content == ContentId::of(tar)`;
restore inverts the pipeline (fetch → blake3-verify → extract).

## Metadata layout

A `Snapshot` row lives at
`StorageKey { namespace = "snapshots", run_id = Some(run_id), path = [hex(snapshot_id)] }`
and is JSON-encoded. Spec-04 §5.1 lists the fields:

| Field          | Type                | Purpose                                                  |
| -------------- | ------------------- | -------------------------------------------------------- |
| `id`           | `SnapshotId`        | `ContentId` of the tar bytes (Phase 4 = single part)     |
| `kind`         | `SnapshotKind`      | `TrainState` in Phase 4; others reserved                 |
| `run_id`       | `RunId`             | Owning run                                               |
| `created_at`   | `DateTime<Utc>`     | RFC3339 wire form                                        |
| `label`        | `Option<SmolStr>`   | Optional human-readable label (CLI `--label`)            |
| `parts`        | `Vec<SnapshotPart>` | One per blob; Phase 4 ships exactly one (`role="tar"`)   |
| `algorithm_id` | `AlgorithmId`       | Producing algorithm (`"sft"`, `"rm"`, ...)               |
| `meta`         | `serde_json::Value` | Algorithm-internal extras (D-DETERM-05; opaque to core)  |

JSON (not postcard) is the on-disk encoding because `serde_json::Value` is a
self-describing format that postcard intentionally refuses to encode. The
small row size and infrequent writes make the choice cheap, and the row is
human-readable on disk for debugging.

## Tar reproducibility contract (Pitfall 9)

`build_deterministic_tar(&Path) -> Vec<u8>` is the byte-stable tar builder.
It must produce identical bytes for identical input directories across runs,
machines, and tar versions — otherwise the `blake3` hash drifts and resumes
fail to find their predecessor blob.

The invariants enforced:

- **Sort entries by path** before writing (file-system iteration order is
  non-deterministic).
- **`HeaderMode::Deterministic`** zeroes mtime/uid/gid metadata at write
  time **but does not zero mode bits**. The mode bits are platform-dependent
  (macOS gives regular files `0o755` by default).
- **Explicit `header.set_mode(0o644)` for files and `0o755` for dirs.**
- **Explicit `set_mtime(0)`, `set_uid(0)`, `set_gid(0)`** on top of
  `HeaderMode::Deterministic`.
- **No compression** — gzip / zstd byte streams drift across versions.
- **GNU header format** (`Header::new_gnu`) — stable across tar releases.

`crates/rollout-snapshots/tests/deterministic_tar.rs` proves Pitfall 9 holds
by parsing every entry header and asserting `mode = 0o644 | 0o755`.

## Restore semantics

`restore_train_state(&snapshot, dst_dir)` fetches the tar blob via
`ObjectStore::get_bytes(parts[0].content)`, verifies
`blake3(bytes) == parts[0].content`, and extracts to `dst_dir`. A mismatch
returns `Fatal { PluginContract, msg: "blake3 mismatch on restore: ..." }`.

The bare `Snapshotter::restore` trait method takes a `RestoreTarget` enum:

| Variant    | Phase 4 behavior                                                                                   |
| ---------- | -------------------------------------------------------------------------------------------------- |
| `SameRun`  | Returns `Fatal { PluginContract }` — the trait method has no `dst_dir`; callers use `restore_train_state(snapshot, dst_dir)` directly. |
| `Fork`     | Returns `Fatal { PluginContract, msg: "Phase 9: Fork restore (new_run_id=...)" }`                  |
| `Worker`   | Returns `Fatal { PluginContract, msg: "Phase 6: Worker restore (worker_id=...)" }`                 |

This is intentional: the Phase-4 surface optimizes for the SFT/RM training
loop, which holds the destination directory locally and drives
`restore_train_state` directly. Phase-9 PPO actor-swap adds the multi-worker
restore plumbing.

## List + prune surface

`Snapshotter::list(SnapshotFilter)` scans `namespace="snapshots"`, optionally
filters by `run_id` / `kind` / `label_contains`, sorts newest-first
(`created_at` descending), and caps by `limit`. The scan is O(snapshots in
namespace); the secondary `SnapshotId → key` index is deferred to Phase 9.

`Snapshotter::prune(PrunePolicy { run_id, retention })` enforces
`RetentionPolicy`:

- `keep_last: u32` — keep the N most recent snapshots regardless of label.
- `keep_labeled: bool` — labeled snapshots are immune to pruning when true.
- `max_age: Option<Duration>` — anything older than `max_age` is eligible.

Returns the count of metadata rows deleted. The underlying tar blobs are
**not deleted** in Phase 4 — `ObjectStore` has no `delete` method yet
(Phase-5 addition). Blobs are content-addressed and idempotent; orphaned
ones cost storage but never corrupt a restore.

## Algorithm-internal state — `meta: serde_json::Value`

`Snapshot.meta` is an opaque JSON blob owned by the producing algorithm
(D-DETERM-05). The framework never inspects it. SFT might store
`{ "step": 5, "curriculum_cursor": 12 }`; RM might store
`{ "epoch": 2, "best_loss": 0.42 }`. This keeps algorithm-specific resume
state out of the framework-owned snapshot row.

## Determinism caveats

- **CPU mode:** byte-identical on the same toolchain across machines.
- **CUDA same-SM:** weights+optimizer bit-identical when the producing and
  restoring GPU expose the same compute capability. Cross-SM is best-effort.
- **Cross-machine:** best-effort. The tar is reproducible; PyTorch /
  accelerate restore is not always bit-identical across GPU generations.

Plan 04-05 (`backend-vllm-train`) lands the determinism CI gate and CPU-mode
fallback for environments without CUDA.

## Pointers

- `crates/rollout-snapshots/src/lib.rs` — `SnapshotterImpl` + trait impl.
- `crates/rollout-snapshots/src/tar_build.rs` — deterministic tar builder.
- `crates/rollout-snapshots/src/kind/train_state.rs` — save + restore pipeline.
- `crates/rollout-snapshots/src/policy.rs` — `Snapshotter::prune` retention enforcement.
- `crates/rollout-snapshots/src/key.rs` — `StorageKey` helpers for `namespace="snapshots"`.
- `crates/rollout-storage/src/embedded/tables.rs` — `T_SNAPSHOTS` table definition.
- `docs/specs/04-storage-snapshots.md` — authoritative contract (especially §5 + §5a + §7).
