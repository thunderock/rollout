---
phase: 04-train-sft-rm-snapshots
plan: 01
subsystem: snapshots
tags: [rollout-snapshots, snapshotter, train-state, tar, blake3, pitfall-9, deterministic-tar, retention-policy, mdbook, training]

# Dependency graph
requires:
  - phase: 04-train-sft-rm-snapshots
    provides: "Plan 04-00-a — Snapshotter trait + Snapshot / SnapshotKind / SnapshotPart / SnapshotId / SnapshotRequest / SnapshotFilter / RestoreTarget / PrunePolicy / RetentionPolicy at the spec 04 §5.2 surface"
  - phase: 04-train-sft-rm-snapshots
    provides: "Plan 04-00-b — rollout-snapshots crate skeleton + workspace deps (chrono, tar, walkdir, futures) + arch-lint #9 (snapshots ↛ algo-*)"
  - phase: 02-local-substrate
    provides: "EmbeddedStorage + FsObjectStore + Storage/ObjectStore traits; content-addressed put_bytes (returns ContentId from blake3)"
provides:
  - "SnapshotterImpl — concrete Snapshotter for SnapshotKind::TrainState (Phase-4 single-kind surface)"
  - "build_deterministic_tar(&Path) -> Result<Vec<u8>, CoreError> — Pitfall-9-aware byte-stable tar builder (explicit set_mode 0o644/0o755 + mtime=0 + uid=gid=0 + sorted walkdir)"
  - "save_train_state + restore_train_state pipeline (tar → blake3 → ObjectStore + Storage txn → restore = get_bytes + blake3-verify + extract)"
  - "apply_prune retention enforcement honoring keep_last / keep_labeled / max_age (RetentionPolicy)"
  - "T_SNAPSHOTS TableDefinition + table_for() arm in rollout-storage::embedded::tables (Phase-4 §5.1 namespace registered)"
  - "Training section in mdBook (docs/book/src/training/{index,snapshots}.md) wired from SUMMARY.md"
affects: [phase-04 plan 04-02 (SftAlgo.snapshot_save / snapshot_restore call SnapshotterImpl), plan 04-04 (RmAlgo same), plan 04-05 (snapshot_resume.rs byte-compare proof), plan 04-06 (CLI subcommands), plan 04-07 (smoke + docs polish)]

# Tech tracking
tech-stack:
  added:
    - "rollout-storage, rollout-cloud-local, ulid moved into rollout-snapshots dev-dependencies (test substrates)"
    - "tokio with `macros` + `rt-multi-thread` features added to rollout-snapshots dev-dependencies (#[tokio::test])"
  patterns:
    - "Phase-4 escape hatch: SnapshotterImpl exposes save_train_state / restore_train_state as public methods alongside the trait so callers can pass an explicit accelerate_dir. The bare Snapshotter::save/restore trait methods have nowhere to source the directory; they return Fatal { PluginContract } directing callers to the explicit-dir entry points."
    - "ContentId derived via ObjectStore::put_bytes return (content-addressed by design); we verify it matches our own blake3 computation as a defensive assertion."
    - "Snapshot row uses serde_json encoding (not postcard) because Snapshot.meta: serde_json::Value is self-describing — postcard refuses to encode it (Rule-1 fix during execution)."
    - "Per-kind error sentinels: Buffer → Phase 9, Process → Phase 11, EpisodicMemory → Phase 8 (Fatal { PluginContract, msg: \"Phase N: <kind>\" } so callers can grep for the future phase)."
    - "Prune deletes metadata rows only — ObjectStore::delete is not in the Phase-2 trait. Documented as Phase-5 deferred; orphaned blobs are safe because they are content-addressed and idempotent."

key-files:
  created:
    - "crates/rollout-snapshots/src/tar_build.rs"
    - "crates/rollout-snapshots/src/key.rs"
    - "crates/rollout-snapshots/src/kind/mod.rs"
    - "crates/rollout-snapshots/src/kind/train_state.rs"
    - "crates/rollout-snapshots/src/policy.rs"
    - "crates/rollout-snapshots/tests/deterministic_tar.rs"
    - "crates/rollout-snapshots/tests/save_restore_roundtrip.rs"
    - "crates/rollout-snapshots/tests/list_and_prune.rs"
    - "docs/book/src/training/index.md"
    - "docs/book/src/training/snapshots.md"
  modified:
    - "crates/rollout-snapshots/src/lib.rs (wholesale rewrite from placeholder skeleton)"
    - "crates/rollout-snapshots/Cargo.toml (4 dev-dependencies added: rollout-storage, rollout-cloud-local, tokio macros+rt-multi-thread, ulid)"
    - "crates/rollout-storage/src/embedded/tables.rs (T_SNAPSHOTS const + table_for arm + all_tables entry)"
    - "docs/book/src/SUMMARY.md (Training section inserted between Inference and Examples)"

key-decisions:
  - "JSON, not postcard, for the snapshot metadata row — postcard's wire model intentionally rejects self-describing serde_json::Value; switching to serde_json keeps the row encodable, self-describing, and human-readable on disk for debugging."
  - "Public save_train_state / restore_train_state as escape hatches — Snapshotter::save's signature has no dir, but TrainState fundamentally needs one. The trait method returns Fatal { PluginContract } directing callers to the public escape hatch. This is a deliberate Phase-4 deviation from the spec's \"one entry point\" wording, called out in the lib.rs docstring."
  - "ObjectStore::delete not used in prune — the Phase-2 trait has no delete; cascade is deferred to Phase 5. Documented in policy.rs module docs + the mdBook chapter."
  - "io_err takes &std::io::Error to avoid clippy::needless_pass_by_value; closures bind owned errors and pass references at the call site."
  - "Empty source dir produces a non-empty tar (terminator blocks only) — verified by deterministic_tar_empty_dir test."

patterns-established:
  - "Pattern: deterministic tar builder lives in its own pub module (tar_build) so other crates / tests can drive it directly; the kind::train_state::save_train_state pipeline composes it with ObjectStore + Storage."
  - "Pattern: per-kind sentinel errors enumerate the future phase number explicitly so a grep -r 'Phase 9' surfaces every deferral. SFT / RM / online tooling can match on the substring to detect 'not yet'."

requirements-completed: [TRAIN-03, DOCS-01, DOCS-02, DOCS-03]

# Metrics
duration: 9min
completed: 2026-05-21
---

# Phase 4 Plan 01: rollout-snapshots SnapshotterImpl Summary

**`SnapshotterImpl` ships end-to-end for `SnapshotKind::TrainState`: Pitfall-9-aware deterministic tar builder + content-addressed `ObjectStore::put_bytes` + JSON snapshot row on `EmbeddedStorage` under `namespace="snapshots"`. Other kinds (Buffer / Process / EpisodicMemory) return `Fatal { PluginContract, msg: "Phase N: <kind>" }`. list/prune work end-to-end with `RetentionPolicy { keep_last, keep_labeled, max_age }`. Training mdBook section bootstrapped. 18 tests green.**

## Performance

- **Duration:** ~9 min
- **Started:** 2026-05-21T21:15:47Z
- **Completed:** 2026-05-21T21:24:52Z
- **Tasks:** 2
- **Files created:** 10 (5 source files + 3 test files + 2 mdBook files)
- **Files modified:** 4 (lib.rs + Cargo.toml + tables.rs + SUMMARY.md)

## Accomplishments

- **Deterministic tar builder.** `build_deterministic_tar(&Path) -> Vec<u8>` implements Pitfall 9 byte-stability: explicit `set_mode(0o644 | 0o755)` on top of `HeaderMode::Deterministic` (which alone leaves mode bits at the OS-supplied default — macOS gives regular files `0o755`), `set_mtime(0) / set_uid(0) / set_gid(0)`, sorted walkdir traversal, no compression, GNU header format. Verified by parsing each entry header in `deterministic_tar_explicit_mode_bits`.
- **TrainState save pipeline.** `save_train_state(request, accelerate_dir)`:
  1. Tar `accelerate_dir` on a blocking thread,
  2. Compute `ContentId::of(tar_bytes) = blake3(tar)`,
  3. `ObjectStore::put_bytes(tar, hint)` — content-addressed; the returned `ContentId` matches the expected one (defensive assert).
  4. Build the `Snapshot` row (`parts[0] = { role: "tar", content: id, size }`),
  5. `Storage::begin().put_bytes(snapshot_key, json(snapshot)).commit()`.
- **TrainState restore pipeline.** `restore_train_state(snapshot, dst_dir)` fetches `parts[0].content` via `ObjectStore::get_bytes`, blake3-verifies the bytes, and extracts on a blocking thread. Mismatch → `Fatal { PluginContract, msg: "blake3 mismatch on restore" }`.
- **Snapshotter trait impl.** `save` enumerates all 4 SnapshotKind variants; `TrainState` returns a directing-error because the trait method has no `accelerate_dir` to pass through. `restore` discriminates `RestoreTarget` (SameRun directs to the explicit-dir path; Fork → Phase 9; Worker → Phase 6). `list` scans `namespace="snapshots"`, filters by run_id / kind / label_contains, sorts newest-first, applies limit. `prune` delegates to `policy::apply_prune` which enforces `RetentionPolicy { keep_last, keep_labeled, max_age }`.
- **Storage namespace registration.** `T_SNAPSHOTS: TableDefinition` + `table_for("snapshots") => T_SNAPSHOTS` + `all_tables` updated from 7 → 8 entries. Mirrors the Phase-3 `infer` namespace registration pattern exactly.
- **Tests.** 18 tests green:
  - **deterministic_tar.rs (4):** `deterministic_tar_byte_stability`, `deterministic_tar_explicit_mode_bits` (Pitfall 9 proof), `deterministic_tar_empty_dir`, `deterministic_tar_round_trip_via_extract`.
  - **save_restore_roundtrip.rs (7):** `save_restore_roundtrip` (byte-identical 3-file restore), `save_meta_round_trips` (serde_json::Value with strings + numbers + nested arrays + nested objects survives), `buffer_kind_returns_fatal_phase_9`, `process_kind_returns_fatal_phase_11`, `episodic_memory_returns_fatal_phase_8`, `restore_worker_target_phase_6`, `restore_fork_target_phase_9`.
  - **list_and_prune.rs (7):** `list_filters_by_label_contains`, `list_filters_by_kind`, `list_newest_first` (ordering proof with 2ms creation gaps), `list_respects_limit`, `prune_honors_keep_last_and_keep_labeled` (5 snapshots, 2 labeled, keep_last=2, keep_labeled=true → deletes 2 oldest unlabeled), `prune_keep_last_only`, `prune_runs_are_isolated`.
- **mdBook chapter.** `docs/book/src/training/snapshots.md` (~140 lines) covers architecture (ASCII diagram), metadata layout (per-field table), Pitfall 9 contract, restore semantics (per-target table), list/prune surface, meta opacity (D-DETERM-05), determinism caveats (CPU vs CUDA same-SM vs cross-machine), pointers. Linked under new `Training` section in `docs/book/src/SUMMARY.md`. `mdbook build docs/book` clean.

## Test coverage matrix

| Concern                              | Test                                                  | File                              |
| ------------------------------------ | ----------------------------------------------------- | --------------------------------- |
| Tar bytes deterministic same-run     | `deterministic_tar_byte_stability`                    | `deterministic_tar.rs`            |
| Tar mode bits explicit (Pitfall 9)   | `deterministic_tar_explicit_mode_bits`                | `deterministic_tar.rs`            |
| Tar tolerates empty src dir          | `deterministic_tar_empty_dir`                         | `deterministic_tar.rs`            |
| Tar round-trips via extract          | `deterministic_tar_round_trip_via_extract`            | `deterministic_tar.rs`            |
| Save + list + restore byte-identical | `save_restore_roundtrip`                              | `save_restore_roundtrip.rs`       |
| `Snapshot.meta` JSON round-trips     | `save_meta_round_trips`                               | `save_restore_roundtrip.rs`       |
| Buffer kind → Fatal Phase 9          | `buffer_kind_returns_fatal_phase_9`                   | `save_restore_roundtrip.rs`       |
| Process kind → Fatal Phase 11        | `process_kind_returns_fatal_phase_11`                 | `save_restore_roundtrip.rs`       |
| EpisodicMemory → Fatal Phase 8       | `episodic_memory_returns_fatal_phase_8`               | `save_restore_roundtrip.rs`       |
| restore Fork → Fatal Phase 9         | `restore_fork_target_phase_9`                         | `save_restore_roundtrip.rs`       |
| restore Worker → Fatal Phase 6       | `restore_worker_target_phase_6`                       | `save_restore_roundtrip.rs`       |
| list label_contains filter           | `list_filters_by_label_contains`                      | `list_and_prune.rs`               |
| list kind filter                     | `list_filters_by_kind`                                | `list_and_prune.rs`               |
| list newest-first ordering           | `list_newest_first`                                   | `list_and_prune.rs`               |
| list limit cap                       | `list_respects_limit`                                 | `list_and_prune.rs`               |
| Prune keep_last + keep_labeled combo | `prune_honors_keep_last_and_keep_labeled`             | `list_and_prune.rs`               |
| Prune keep_last only                 | `prune_keep_last_only`                                | `list_and_prune.rs`               |
| Prune per-run isolation              | `prune_runs_are_isolated`                             | `list_and_prune.rs`               |

## Task Commits

1. **Task 1: Deterministic tar + TrainState save pipeline + snapshots namespace** — `e149ec3` (`feat(04-01-01):`)
2. **Task 2: SnapshotterImpl save/restore/list/prune + integration tests + mdBook chapter** — `acd47d0` (`feat(04-01-02):`)

## Files Created/Modified

**Created (10):**
- `crates/rollout-snapshots/src/tar_build.rs` — Pitfall-9-aware deterministic tar builder + extractor.
- `crates/rollout-snapshots/src/key.rs` — `StorageKey` builders for `namespace="snapshots"`.
- `crates/rollout-snapshots/src/kind/mod.rs` — kind module gate.
- `crates/rollout-snapshots/src/kind/train_state.rs` — `save_train_state` + `restore_train_state` pipeline.
- `crates/rollout-snapshots/src/policy.rs` — `apply_prune` retention enforcement.
- `crates/rollout-snapshots/tests/deterministic_tar.rs` — 4 Pitfall-9 proof tests.
- `crates/rollout-snapshots/tests/save_restore_roundtrip.rs` — 7 integration tests (round-trip + Phase-N sentinels).
- `crates/rollout-snapshots/tests/list_and_prune.rs` — 7 integration tests (filter / order / prune).
- `docs/book/src/training/index.md` — Training section landing page.
- `docs/book/src/training/snapshots.md` — Snapshots mdBook chapter.

**Modified (4):**
- `crates/rollout-snapshots/src/lib.rs` — Wholesale rewrite from placeholder to full `SnapshotterImpl` + trait impl + public `save_train_state` / `restore_train_state` escape hatches + private `read_meta` helper.
- `crates/rollout-snapshots/Cargo.toml` — Added dev-deps on rollout-storage, rollout-cloud-local, tokio (macros + rt-multi-thread), ulid.
- `crates/rollout-storage/src/embedded/tables.rs` — `T_SNAPSHOTS` const + `table_for` arm + `all_tables` 7 → 8 entries.
- `docs/book/src/SUMMARY.md` — Training section inserted (Snapshots chapter linked); sibling 04-03 agent added Postgres backend chapter alongside, no conflict.

## Decisions Made

- **JSON metadata row encoding (not postcard).** `Snapshot.meta: serde_json::Value` is a self-describing format; postcard intentionally refuses to encode it. The chosen JSON encoding is self-describing by design, keeps the row human-readable on disk, and round-trips arbitrarily-nested `Value`s. Cost is negligible (small row, infrequent writes).
- **Public `save_train_state` / `restore_train_state` escape hatches.** The bare `Snapshotter::save` / `Snapshotter::restore` trait methods have nowhere to source the `accelerate_dir` from. Rather than thread a side-channel, we expose explicit-dir methods on `SnapshotterImpl` and have the trait methods return `Fatal { PluginContract }` directing callers to them. Documented in lib.rs docstring + mdBook chapter.
- **Prune deletes metadata rows only.** Phase-2 `ObjectStore` has no `delete`; cascade is a Phase-5 follow-up. Blobs are content-addressed and idempotent, so orphans are storage-cost-only — they never corrupt a restore.
- **Per-kind error sentinels enumerate future phase.** Every non-TrainState kind returns `Fatal { PluginContract, msg: "Phase N: <kind>" }` with the future phase number. Easy to grep + clear deferral signal.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `postcard::to_stdvec` rejects `serde_json::Value`**
- **Found during:** Task 2, on the first `cargo test -p rollout-snapshots --tests` run after wiring SnapshotterImpl + integration tests.
- **Issue:** Every test that exercised a non-`Null` `Snapshot.meta` (including the headline `save_restore_roundtrip`, all `list_*` tests, and all `prune_*` tests) failed with `Fatal(Internal { msg: "postcard decode Snapshot: This is a feature that PostCard will never implement" })`. Postcard's wire model is non-self-describing — it can encode known fixed types but not `serde_json::Value`, which can hold arbitrary nested heterogeneous data at runtime.
- **Fix:** Switched the encoding from `postcard::to_stdvec / from_bytes` to `serde_json::to_vec / from_slice` at three sites: `kind::train_state::save_train_state` (write), `lib::SnapshotterImpl::list` + `SnapshotterImpl::read_meta` (read), `policy::apply_prune` (read).
- **Files modified:** `crates/rollout-snapshots/src/kind/train_state.rs`, `crates/rollout-snapshots/src/lib.rs`, `crates/rollout-snapshots/src/policy.rs`.
- **Verification:** `cargo test -p rollout-snapshots --tests` exits 0 with 18/18 green; the `save_meta_round_trips` test specifically verifies arbitrary `serde_json::Value` (numbers + strings + nested arrays + nested objects) round-trips through Storage.
- **Committed in:** `acd47d0` (Task 2 commit).

**2. [Rule 1 - Bug] clippy::needless_pass_by_value on `io_err(e: std::io::Error)`**
- **Found during:** Task 1, `cargo clippy -p rollout-snapshots --all-targets -- -D warnings`.
- **Issue:** `fn io_err(e: std::io::Error)` consumed the error by value but only called `.to_string()` on it. Clippy's pedantic group flagged the unnecessary move.
- **Fix:** Changed signature to `fn io_err(e: &std::io::Error)`; updated all 8 call sites from `.map_err(io_err)` to `.map_err(|e| io_err(&e))`.
- **Files modified:** `crates/rollout-snapshots/src/tar_build.rs`.
- **Verification:** `cargo clippy -p rollout-snapshots --all-targets -- -D warnings` exits 0.
- **Committed in:** `e149ec3` (Task 1).

**3. [Rule 1 - Bug] clippy::doc_markdown on `<snapshot_id_hex>` in `key.rs` doc comment**
- **Found during:** Task 1, `cargo clippy -p rollout-snapshots --all-targets -- -D warnings`.
- **Issue:** Bare `snapshot_id_hex` inside the doc comment for `snapshot_key` tripped `clippy::doc_markdown`.
- **Fix:** Rewrote the docstring to use proper backticks: `Layout: \`namespace = "snapshots"\`, \`run_id = Some(run_id)\`, \`path = [snapshot_id_hex]\``.
- **Committed in:** `e149ec3`.

**4. [Rule 1 - Bug] Missing `Snapshotter` trait import in `tests/list_and_prune.rs`**
- **Found during:** Task 2, `cargo build -p rollout-snapshots --tests`.
- **Issue:** Test calls `snapper.list(...)` and `snapper.prune(...)`; these methods are part of the `Snapshotter` trait, which wasn't in scope, so rustc emitted E0599 ("method not found"). The Snapshotter trait isn't auto-imported by `use rollout_snapshots::SnapshotterImpl`.
- **Fix:** Added `Snapshotter` to the `use rollout_core::{...}` import.
- **Committed in:** `acd47d0`.

**5. [Rule 1 - Bug] clippy::doc_markdown on `TrainState` + `ContentIds` in `list_and_prune.rs` test helper docstring**
- **Found during:** Task 2, `cargo clippy -p rollout-snapshots --all-targets -- -D warnings`.
- **Issue:** Two bare identifiers in the `make_snap` helper docstring tripped doc_markdown.
- **Fix:** Wrapped both in backticks: `Create a \`TrainState\` snapshot with \`n\` unique source bytes (so \`ContentId\`s differ)`.
- **Committed in:** `acd47d0`.

---

**Total deviations:** 5 auto-fixed (1 substantive — postcard cannot encode `serde_json::Value`; 4 mechanical clippy / use-statement fixes). 0 architectural decisions required.

**Impact on plan:** The postcard → serde_json switch is a load-bearing decision (documented in the mdBook chapter + Decisions Made above), but it doesn't expand scope — JSON is the natural encoding for a row that carries an opaque `serde_json::Value`. The mechanical fixes are normal clippy hygiene that the plan implicitly required via DOCS-03 + acceptance `cargo clippy ... -- -D warnings`.

## Issues Encountered

- **Parallel-execution file scope.** Three Phase-4 wave-2 agents (04-01 / 04-02 / 04-03) ran simultaneously on `main`. All three agents touch the workspace `Cargo.lock`, and 04-03 also touches `docs/book/src/SUMMARY.md` (Postgres backend chapter). Staged commits only my files (per parallel_execution prompt rule "stay strictly within this plan's file scope"); the orchestrator picks up `Cargo.lock` once after all agents complete. My SUMMARY.md edit raced cleanly with 04-03's (different insertion line), and the working-tree state has both Training subsections after both commits land.

## User Setup Required

None.

## Next Phase Readiness

- TRAIN-03 substrate is delivered. Plan 04-02 (SftAlgo) can call
  `snapper.save_train_state(request, accelerate_dir)` from
  `PolicyAlgorithm::snapshot_save`; Plan 04-04 (RmAlgo) ditto.
- Plan 04-05 (`snapshot_resume.rs` byte-compare proof in
  `backend-vllm-train`) has its dependency satisfied: `Snapshot.parts[0].content`
  is the canonical blake3 hash; two saves of the same accelerate dir yield
  identical `ContentId`s by construction.
- Plan 04-06 (CLI `rollout train sft` + snapshot subcommands) consumes
  `SnapshotterImpl::new(storage, object, work_dir)` directly.

No blockers. All 18 tests green; clippy clean; rustdoc gate clean; mdbook
build clean.

## Self-Check: PASSED

**Files exist:**
- FOUND: `crates/rollout-snapshots/src/tar_build.rs`
- FOUND: `crates/rollout-snapshots/src/key.rs`
- FOUND: `crates/rollout-snapshots/src/kind/mod.rs`
- FOUND: `crates/rollout-snapshots/src/kind/train_state.rs`
- FOUND: `crates/rollout-snapshots/src/policy.rs`
- FOUND: `crates/rollout-snapshots/tests/deterministic_tar.rs`
- FOUND: `crates/rollout-snapshots/tests/save_restore_roundtrip.rs`
- FOUND: `crates/rollout-snapshots/tests/list_and_prune.rs`
- FOUND: `docs/book/src/training/index.md`
- FOUND: `docs/book/src/training/snapshots.md`

**Commits exist (verified via `git log --oneline | grep`):**
- FOUND: `e149ec3` (`feat(04-01-01): deterministic tar + TrainState save pipeline + snapshots namespace`)
- FOUND: `acd47d0` (`feat(04-01-02): SnapshotterImpl save/restore/list/prune + integration tests + mdBook chapter`)

**Acceptance grep checks (all PASSED):**
- `grep -q 'T_SNAPSHOTS' crates/rollout-storage/src/embedded/tables.rs` ✓
- `grep -q '"snapshots" =>' crates/rollout-storage/src/embedded/tables.rs` ✓
- `grep -q 'fn build_deterministic_tar' crates/rollout-snapshots/src/tar_build.rs` ✓
- `grep -q 'set_mode' crates/rollout-snapshots/src/tar_build.rs` ✓ (Pitfall 9 fix)
- `grep -q 'pub(crate) async fn save_train_state' crates/rollout-snapshots/src/kind/train_state.rs` ✓
- `grep -q 'impl Snapshotter for SnapshotterImpl' crates/rollout-snapshots/src/lib.rs` ✓
- `grep -q 'Phase 9: SnapshotKind::Buffer' crates/rollout-snapshots/src/lib.rs` ✓
- `grep -q 'fn apply_prune' crates/rollout-snapshots/src/policy.rs` ✓
- `grep -q 'training/snapshots.md' docs/book/src/SUMMARY.md` ✓
- `grep -q 'Pitfall 9' docs/book/src/training/snapshots.md` ✓

**Builds + tests + lints:**
- `cargo build -p rollout-snapshots` ✓
- `cargo build -p rollout-storage` ✓
- `cargo test -p rollout-snapshots --tests` ✓ (18 tests pass)
- `cargo clippy -p rollout-snapshots --all-targets -- -D warnings` ✓
- `RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" cargo doc -p rollout-snapshots --no-deps` ✓
- `mdbook build docs/book` ✓

---
*Phase: 04-train-sft-rm-snapshots*
*Plan: 01*
*Completed: 2026-05-21*
