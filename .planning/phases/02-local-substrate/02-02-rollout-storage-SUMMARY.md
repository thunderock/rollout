---
phase: 02-local-substrate
plan: 02
subsystem: substrate-storage
tags: [rollout-storage, redb, postcard, storage, watch, broadcast, transactions, durability, mdbook]

# Dependency graph
requires:
  - phase: 02-local-substrate
    provides: rollout-storage stub + redb/postcard/smol_str workspace pins + Storage/StorageTxn trait surface (plan 02-00 Wave 0)
provides:
  - "EmbeddedStorage implementing Storage against redb 2.5 (Durability::Immediate; postcard value encoding; six-namespace table layout)"
  - "EmbeddedTxn implementing StorageTxn with deferred-publish: events buffered during the txn, fanned out via WatchRouter ONLY after WriteTransaction::commit() returns Ok"
  - "WatchRouter: in-process per-prefix tokio::sync::broadcast fan-out keyed by (namespace, run_id, path)"
  - "cas_bytes covering insert-only / compare-and-swap / delete-if-equal / vacuous no-op"
  - "Single-tuple postcard key encoding (encode_key/decode_key_payload/key_has_prefix) — unambiguous decode regardless of 0x00 bytes inside Option<RunId>"
  - "Six BytesTable consts (runs / workers / heartbeats / queue / plugins / cloudlocal_queue) + table_for(namespace) lookup + all_tables() enumerator"
  - "EmbeddedStorageConfig (default path ./data/rollout.db; deny_unknown_fields)"
  - "docs/book/src/substrate/storage.md substrate chapter wired into SUMMARY.md"
affects: [02-03-rollout-cloud-local, 02-06-rollout-coordinator]

# Tech tracking
tech-stack:
  added:
    - "rollout-storage now depends on: async-trait, redb 2.5, postcard 1.0, smol_str =0.3.2, tokio, tracing, schemars, serde, thiserror — all via workspace = true"
    - "dev-dep: tempfile 3.10 (TempDir for per-test redb file)"
  patterns:
    - "Async-over-sync redb via tokio::task::spawn_blocking; each Storage method moves an Arc<Database> clone into the blocking closure"
    - "WriteTransaction held in Option<> inside EmbeddedTxn — taken via Option::take, moved into spawn_blocking, returned via a StageResult<T> enum so we can keep using it across async hot paths without &mut-borrow-vs-move conflicts (redb Tables borrow the txn)"
    - "Publish-after-commit (RESEARCH Pattern 2): pending: Vec<StorageEvent> buffer in the txn; drained only on commit() Ok; dropped on abort()/Drop"
    - "Single-tuple postcard key encoding to_allocvec(&(&run_id, &path)) — avoids the prefix-encoding ambiguity that bit the first iteration (postcard Option<RunId> with None discriminant 0x00 collides with the original separator-byte layout)"
    - "Per-namespace table opened lazily; missing-table => Ok(None)/empty scan rather than error (so reading a never-written namespace before any commit is non-fatal)"

key-files:
  created:
    - "crates/rollout-storage/src/config.rs — EmbeddedStorageConfig (default `./data/rollout.db`)"
    - "crates/rollout-storage/src/encoding.rs — encode_key/decode_key_payload/key_has_prefix (single-tuple postcard layout)"
    - "crates/rollout-storage/src/embedded/mod.rs — EmbeddedStorage + impl Storage (begin/get_bytes/get_many_bytes/scan_bytes/watch/ping)"
    - "crates/rollout-storage/src/embedded/tables.rs — BytesTable alias + 6 const TableDefinitions + table_for + all_tables"
    - "crates/rollout-storage/src/embedded/txn.rs — EmbeddedTxn + impl StorageTxn (put_bytes/delete/cas_bytes/commit/abort)"
    - "crates/rollout-storage/src/embedded/watch.rs — WatchRouter (subscribe/publish; per-prefix HashMap<PrefixKey, broadcast::Sender>)"
    - "crates/rollout-storage/tests/crud.rs — 5 round-trip tests"
    - "crates/rollout-storage/tests/tables.rs — 2 per-namespace isolation tests (incl. six-namespace reopen)"
    - "crates/rollout-storage/tests/txn.rs — 5 commit/abort/cas tests"
    - "crates/rollout-storage/tests/watch.rs — 5 watch semantics tests"
    - "crates/rollout-storage/tests/crash_safety.rs — 1 active drop-without-commit recovery + 1 ignored Phase-6 SIGKILL placeholder"
    - "docs/book/src/substrate/storage.md — substrate chapter"
  modified:
    - "crates/rollout-storage/Cargo.toml — concrete dep set (rollout-core + async-trait + serde + schemars + thiserror + tracing + tokio + smol_str + redb + postcard; dev: tempfile)"
    - "crates/rollout-storage/src/lib.rs — crate-level //! doc + module wiring + re-exports (EmbeddedStorageConfig, EmbeddedStorage)"
    - "docs/book/src/SUMMARY.md — nests [Storage](./substrate/storage.md) under Substrate"
    - "Cargo.lock — refreshed for redb 2.5 + postcard 1.0 transitives"

key-decisions:
  - "[D-STO-01 / D-STO-03] EmbeddedStorage::begin() sets Durability::Immediate on the WriteTransaction (always-fsync). Wrapped in spawn_blocking because set_durability is sync."
  - "[D-STO-02] watch() returns broadcast::Receiver<StorageEvent> from a per-prefix Sender in WatchRouter. Channels are allocated on first subscribe (or-insert-with broadcast::channel(256)); send errors (no live receivers) are silently ignored."
  - "[D-STO-04] Value encoding is left to callers (Storage::*_bytes is byte-level). Key encoding uses postcard internally; the namespace is encoded by the table choice, NOT by the key bytes."
  - "[Claude] Single-tuple key encoding (`postcard::to_allocvec(&(&run_id, &path))`) — replaced the initial 'postcard(run_id) || 0x00 || postcard(path)' layout when the first scan test panicked decoding a key whose postcard-encoded `Option<RunId>` contained an inner 0x00 (the `None` discriminant). The tuple form is self-describing and decodes unambiguously."
  - "[Claude] StageResult<T> enum + Option<WriteTransaction>: redb's Table borrows the txn, so we can't move the txn into spawn_blocking AND back out while a Table is open. The pattern is: open Table inside the closure, drop it before returning, move the txn back via StageResult::{Ok,Err}(txn, ...). Allows the txn to survive across multiple `&mut self` async method calls."
  - "[Claude] cas_bytes semantics: expected=None new=Some -> insert-only; expected=Some new=Some -> CAS; expected=Some new=None -> delete-if-equal; expected=None new=None -> vacuous no-op (returns true if key absent, false otherwise — no event emitted). Matches the plan's <behavior> spec."
  - "[Claude] scan_bytes does full-table iteration with per-entry decode + key_has_prefix check. Acceptable for Phase 2 because tables are partitioned by namespace; if heartbeats/* grows hot in Phase 6, a future StorageStream + byte-prefix-encoded layout can lift the iteration cost."
  - "[Claude] Crash-safety: the active test exercises the drop-without-commit path (redb's abort-on-drop is byte-equivalent to a SIGKILL between put and commit from the on-disk perspective). A separate-process SIGKILL variant is ignored + deferred to Phase 6 DIST-03 — needs a helper binary + raw signal harness."
  - "[Rule 1 — bug fix during execution] FatalError variants are struct-form ({msg: String}) not tuple-form — plan instructions used `FatalError::Internal(string)` (Phase-1-shape), but the Wave-0 traits use the struct form. All internal() helpers wrap into `FatalError::Internal { msg: ... }`."
  - "[Rule 1 — bug fix during execution] WatchRouter::subscribe / publish take `&StorageKey` and `&StorageEvent` per clippy `needless_pass_by_value`. The `Storage::watch` trait method still takes prefix by value (trait signature unchanged); the impl borrows and forwards."

patterns-established:
  - "Async-storage-over-sync-engine pattern: hold the underlying handle in Option<>, take it on each &mut self op, move into spawn_blocking, return via a StageResult-like sum so the caller can keep using it"
  - "Postcard key encoding via single-tuple to_allocvec avoids hand-rolled separators; trade-off is that prefix scans cannot be byte-range scans (must decode + filter) — acceptable while per-namespace tables stay small"
  - "Publish-after-commit broadcast: store pending events in the txn; drain after redb commit returns Ok; drop on abort/Drop. Subscribers never observe rolled-back writes"
  - "Six-namespace lazy table opens: opening an unknown table after reopen returns redb::TableError::TableDoesNotExist; the impl maps that to empty results rather than propagating an error, so first-read-before-any-write paths are non-fatal"

deviations:
  - "[Rule 1 — bug] FatalError variants are struct-form, not tuple-form (plan's snippet used `FatalError::ConfigInvalid(string)` and `FatalError::Internal(string)`). The post-Wave-0 trait surface uses `FatalError::ConfigInvalid { msg }` and `FatalError::Internal { msg }`. Fixed in encoding.rs, tables.rs, and all internal() helpers."
  - "[Rule 1 — bug] Initial key encoding layout `postcard(run_id) || 0x00 || postcard(path)` was ambiguous: postcard encodes `Option::None` as 0x00, so the separator collided with the run_id payload. Switched to single-tuple `postcard::to_allocvec(&(&run_id, &path))`. Surfaced by scan tests panicking with 'Hit the end of buffer'."
  - "[Rule 1 — bug] Plan's txn.rs sketch held the txn as a bare `WriteTransaction` field. redb's `Table` borrows the txn — can't move out while a Table is open AND `&mut self` async methods on the trait don't permit moving the txn out via `*self`. Restructured to `txn: Option<WriteTransaction>` + `Option::take` + return via `StageResult::{Ok,Err}(txn, payload)` from inside spawn_blocking."
  - "[Rule 2 — missing critical functionality] Plan didn't anticipate `clippy::needless_pass_by_value` on WatchRouter::subscribe/publish. Changed to take `&StorageKey` and `&StorageEvent`; the trait method signature (`watch(prefix: StorageKey)`) is unchanged."
  - "[Rule 2 — missing critical functionality] Plan didn't anticipate `clippy::items_after_statements` on the inline `use std::collections::HashMap` inside `get_many_bytes`. Hoisted the import to the file's `use` block."
  - "[Rule 2 — missing critical functionality] Plan didn't anticipate `clippy::very_complex_type` on the const TableDefinition signature. Introduced `pub type BytesTable = TableDefinition<'static, &'static [u8], &'static [u8]>` to keep the public surface tidy."
  - "[Rule 2 — missing critical functionality] Added `# Panics` rustdoc sections on `encode_key`, `WatchRouter::subscribe`, `WatchRouter::publish` (clippy::missing_panics_doc — the workspace `lints.rust = { missing_docs = warn }` + `lints.clippy.pedantic` policy demands it)."
  - "[Rule 1 — bug] Test code initially used `i as u8` casts in `for i in 0..N { ... vec![i as u8] }`. clippy::cast_possible_truncation + cast_sign_loss flagged these under -D warnings. Rewrote to `vec![u8::try_from(i).unwrap()]` (crud.rs) and `for i in 0u8..10u8 { vec![i] }` (crash_safety.rs)."
  - "[Rule 1 — bug] crash_safety.rs had a duplicate `#[cfg_attr(not(linux), ignore = ...)]` + `#[ignore = ...]` causing an unused_attributes warning. Kept the unconditional `#[ignore]` with a Phase-6 DIST-03 message."

# Known stubs (intentional — populated by downstream plans)
known_stubs:
  - "The SIGKILL crash-safety test (`crash_sigkill_helper_does_not_corrupt`) is `#[ignore]`d; full implementation needs a helper child binary + raw signal harness — tracked for Phase-6 DIST-03 (restart-from-storage tests)"
  - "`scan_bytes` does full-table iteration with per-entry decode + prefix filter — acceptable for Phase 2's small per-namespace working sets but will need a `StorageStream` + byte-range layout if heartbeats/* grows hot in Phase 6"

# Authentication gates / preflight notes
preflight_note: "None. `cargo test -p rollout-storage --tests` runs hermetically; each test uses `tempfile::TempDir` for its own redb file. No system services required."

requirements-completed: [SUBSTR-01, DOCS-02, DOCS-03]

# Metrics
duration: 9min
completed: 2026-05-20
---

# Phase 2 Plan 02: rollout-storage Crate Summary

**One-liner:** Shipped `EmbeddedStorage` — the Phase-2 default `Storage` impl over redb 2.5 with `Durability::Immediate`, postcard value encoding, table-per-namespace layout (six namespaces), and an in-process per-prefix `tokio::sync::broadcast` `watch()` whose events fire ONLY after the redb commit returns `Ok`; 18 tests green across crud / tables / txn / watch / crash_safety; gated by `cargo build/test/clippy/doc -p rollout-storage`, `cargo deny check`, and `mdbook build`.

## What landed

### Task 1 — redb core + CRUD + per-namespace tables

`crates/rollout-storage/src/embedded/mod.rs` defines `EmbeddedStorage { db: Arc<Database>, watch: Arc<WatchRouter> }` and impls all six `Storage` methods:

- `begin()` opens a `WriteTransaction`, sets `Durability::Immediate`, hands ownership to a new `EmbeddedTxn`.
- `get_bytes(&key)` opens a `ReadTransaction`, picks the namespace table (lazy; missing-table → `None`), looks up the postcard-encoded key bytes.
- `get_many_bytes(keys)` opens **one** `ReadTransaction`, groups input indices by static-namespace string so each table is opened at most once, preserves input order via index-list bookkeeping.
- `scan_bytes(KeyRange)` iterates the namespace table, decodes each key, applies `key_has_prefix`, honors `limit`.
- `watch(prefix)` delegates to `WatchRouter::subscribe`.
- `ping()` opens-and-drops a `ReadTransaction`.

`crates/rollout-storage/src/embedded/tables.rs` declares `BytesTable = TableDefinition<'static, &'static [u8], &'static [u8]>` plus the six const tables (`T_RUNS`, `T_WORKERS`, `T_HEARTBEATS`, `T_QUEUE`, `T_PLUGINS`, `T_CLOUDLOCAL`) and the `table_for(namespace)` lookup. Unknown namespaces return `Fatal(ConfigInvalid)`.

`crates/rollout-storage/src/embedded/txn.rs` defines `EmbeddedTxn { txn: Option<WriteTransaction>, pending: Vec<StorageEvent>, watch: Arc<WatchRouter> }`. Each mutating op uses an internal `StageResult<T>` enum to move the `WriteTransaction` into a `spawn_blocking` closure, run the redb work, and return the txn back out (this dance is required because `Table` borrows the txn, so we can't keep a Table open across an `.await`). `cas_bytes` covers four cases: insert-only / compare-and-swap / delete-if-equal / vacuous no-op (Option<Vec<u8>> × Option<Vec<u8>>).

`crates/rollout-storage/src/encoding.rs` ships `encode_key`, `decode_key_payload`, `key_has_prefix`. The encoding is a single `postcard::to_allocvec(&(&key.run_id, &key.path))` — self-describing, unambiguous, no separator-byte tricks. (The initial separator-byte layout collided with postcard's `None` discriminant; see deviations.)

`crates/rollout-storage/src/config.rs` ships `EmbeddedStorageConfig { path: PathBuf }` with `#[serde(deny_unknown_fields)]` and default `./data/rollout.db`.

`crates/rollout-storage/src/lib.rs` re-exports `EmbeddedStorage` + `EmbeddedStorageConfig`; the crate-level `//!` doc covers the four core invariants (always-fsync, postcard values, table-per-namespace, publish-after-commit).

Tests in `tests/crud.rs` (5) + `tests/tables.rs` (2) green. Clippy `-D warnings` clean.

### Task 2 — watch broadcast + commit-then-publish + crash safety + mdBook chapter

`crates/rollout-storage/src/embedded/watch.rs` implements `WatchRouter` with a `Mutex<HashMap<PrefixKey, broadcast::Sender<StorageEvent>>>`. `subscribe(&prefix)` creates a per-prefix channel on first call; `publish(&event)` iterates subscribers and dispatches to those whose `PrefixKey` matches the event's key (same namespace, same `Option<RunId>`, candidate path starts with prefix path). Send errors (no live receivers) are silently swallowed.

`EmbeddedTxn::commit()` runs `txn.commit()` on `spawn_blocking`, awaits the result, then — and only then — drains `pending` and calls `watch.publish(&evt)` for each. `EmbeddedTxn::abort()` drops the txn (redb aborts on drop) without publishing. Pending events are recorded by `put_bytes` (always emits `Put`), `delete` (emits `Delete` only if the key actually existed), and `cas_bytes` (emits `Put` if `new = Some(...)` applied; emits `Delete` if `new = None` removed a present key; emits nothing for the vacuous-no-op case).

Tests:

- `tests/txn.rs` (5): commit-persists / abort-discards / cas insert-only / cas compare-and-swap / cas delete-if-equal.
- `tests/watch.rs` (5): publishes after commit (with a 50ms pre-commit anti-window check), no publish after abort, multiple subscribers same prefix fan out, prefix isolation (workers subscriber sees no runs events), delete emits Delete.
- `tests/crash_safety.rs`: active `crash_simulation_drop_does_not_corrupt` exercises drop-without-commit + reopen + assert none of 10 keys visible. `crash_sigkill_helper_does_not_corrupt` is `#[ignore]`d with a Phase-6-DIST-03 placeholder body.

`docs/book/src/substrate/storage.md` (~80 lines) authored — covers backend choice (D-STO-01 + spec 04 §3.1), table layout table, key encoding (single-tuple postcard), value encoding (postcard), durability (`Durability::Immediate`), `watch()` semantics with the begin/put/commit/abort timeline, why-Vec-not-stream for `scan_bytes` (spec 04 §1a), cross-process watch disclaimer (Phase 4 Postgres), crash safety section, and a tests table.

`docs/book/src/SUMMARY.md` nests `[Storage](./substrate/storage.md)` under Substrate; the `[Examples]` placeholder is preserved.

## End-to-end verification

All commands exit 0:

```
cargo build -p rollout-storage
cargo test  -p rollout-storage --tests          # crud(5) + tables(2) + txn(5) + watch(5) + crash_safety(1 pass + 1 ignored) = 18 pass + 1 ignored
cargo clippy -p rollout-storage --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" \
  cargo doc -p rollout-storage --no-deps --all-features
mdbook build docs/book
cargo fmt -p rollout-storage -- --check
cargo deny check                                # advisories ok, bans ok, licenses ok, sources ok
cargo build --workspace                         # all 7 Phase-2 crates still compile
cargo test  --workspace --tests                 # full workspace green
```

## Deviations from Plan

### Rule-1 (auto-fix bug)

1. **`FatalError` is struct-form on the post-Wave-0 trait surface.** Plan snippets used `FatalError::ConfigInvalid("...")` and `FatalError::Internal("...")` (Phase-1 tuple-form); the Wave-0 traits ship `FatalError::ConfigInvalid { msg }` and `FatalError::Internal { msg }`. Fixed in `encoding.rs`, `tables.rs`, and all `internal()` helpers across `mod.rs` and `txn.rs`.

2. **Initial key encoding was ambiguous.** First iteration used `postcard(run_id) || 0x00 || postcard(path)`, but postcard encodes `Option::None` as a single `0x00` byte — `decode_key_payload` then split on the wrong 0x00 when `run_id = None`. Surfaced by `crud_scan_returns_within_prefix` / `crud_scan_respects_limit` panicking with `postcard run_id: Hit the end of buffer, expected more data`. Switched to single-tuple `postcard::to_allocvec(&(&run_id, &path))` — self-describing and unambiguous.

3. **redb `Table` borrow vs `WriteTransaction` move.** Plan's `txn.rs` sketch held the txn as a bare field. redb's `Table<'_, K, V>` borrows the txn, so we can't move the txn into `spawn_blocking` while a `Table` is alive, AND `&mut self` async methods can't take ownership of the field directly. Restructured to `txn: Option<WriteTransaction>` + `Option::take` + return via a `StageResult::{Ok,Err}(txn, payload)` sum (Table opened inside the spawn_blocking closure, dropped before the closure returns).

4. **Test-code numeric casts.** `vec![i as u8]` flagged `clippy::cast_possible_truncation` + `clippy::cast_sign_loss` under `-D warnings`. Rewrote as `vec![u8::try_from(i).unwrap()]` (crud.rs) and `for i in 0u8..10u8 { ... vec![i] }` (crash_safety.rs).

5. **Duplicate `#[ignore]` attribute on the SIGKILL test.** Original draft stacked `#[cfg_attr(not(linux), ignore = ...)]` AND `#[ignore = ...]`, which `unused_attributes` flagged. Kept the unconditional `#[ignore]` with a Phase-6 DIST-03 message.

### Rule-2 (auto-add missing critical functionality)

1. **`# Panics` rustdoc sections.** `clippy::missing_panics_doc` (pedantic, `-D warnings`) demanded sections on `encode_key`, `WatchRouter::subscribe`, `WatchRouter::publish`. Added.

2. **`BytesTable` type alias.** `clippy::very_complex_type` flagged the `TableDefinition<'static, &'static [u8], &'static [u8]>` repetition. Introduced `pub type BytesTable = ...` in `embedded/tables.rs`.

3. **`subscribe(&StorageKey)` / `publish(&StorageEvent)` signatures.** `clippy::needless_pass_by_value`. Trait method `Storage::watch(prefix: StorageKey)` is unchanged; the impl forwards `&prefix`.

4. **Hoisted `use std::collections::HashMap`.** `clippy::items_after_statements` flagged the inline `use` inside `get_many_bytes`. Moved to the file's import block.

### Rule-4 (architectural)

None. All changes stayed within the rollout-storage crate scope. The post-Wave-0 trait surface from plan 02-00 was already correctly shaped for the impl.

## Open Questions for Downstream Plans

- **Plan 02-03 (`rollout-cloud-local`):** The cloud-local `Queue` will mirror to `cloudlocal_queue/*` via this `Storage`. The current `scan_bytes` is full-table-iterate + filter; for restart-replay (potentially thousands of pending items), is this fast enough, or do we need a byte-range scan layout already in Phase 2? Recommendation: ship Phase 2 on full-iterate, revisit only if the smoke test latency budget proves it's a problem.

- **Plan 02-06 (`rollout-coordinator`):** The coordinator's deadline-scan loop will `watch("heartbeats/*")` for cancellations and periodically `scan_bytes("heartbeats", ...)` for the active set. The `WatchRouter` HashMap key is `(namespace, run_id, path)`, so each unique prefix gets its own broadcast channel — confirm the coordinator subscribes to `(namespace=heartbeats, run_id=None, path=[])` exactly once and reuses the receiver, rather than re-subscribing per loop iteration. The router's `or_insert_with` is idempotent but extra clones are wasteful.

- **Plan 02-06 (`rollout-coordinator`):** `Storage::watch` returns a `broadcast::Receiver`, not a stream. The coordinator's loop will need `tokio::select!` over `rx.recv().await` + a `tokio::time::interval` for the periodic deadline scan. Document the pattern in the coordinator's mdBook chapter.

- **Future Phase 6 (DIST-03):** The `crash_sigkill_helper_does_not_corrupt` test needs a helper binary that opens a DB, stages writes, and is killed by the parent test with `kill -9`. The current `drop(txn)` test covers the byte-level invariant (no partial writes survive); the SIGKILL variant is mostly insurance against OS-level interactions (mmap page caches, etc).

## Commits

| Task | Hash    | Subject                                                                       |
| ---- | ------- | ----------------------------------------------------------------------------- |
| 1    | 53dc78c | feat(02-02): wire redb-backed EmbeddedStorage CRUD + table-per-namespace      |
| 2    | f16fb29 | feat(02-02): publish StorageEvents after commit + mdBook storage chapter      |

## Self-Check: PASSED

- `crates/rollout-storage/src/config.rs` — FOUND
- `crates/rollout-storage/src/encoding.rs` — FOUND
- `crates/rollout-storage/src/embedded/mod.rs` — FOUND (`pub struct EmbeddedStorage` + `impl Storage`)
- `crates/rollout-storage/src/embedded/tables.rs` — FOUND (all 6 `T_*` consts + `BytesTable`)
- `crates/rollout-storage/src/embedded/txn.rs` — FOUND (`pub struct EmbeddedTxn` + pending buffer + commit-then-publish)
- `crates/rollout-storage/src/embedded/watch.rs` — FOUND (`pub struct WatchRouter` + `publish` + `subscribe`)
- `crates/rollout-storage/tests/{crud,tables,txn,watch,crash_safety}.rs` — all FOUND, all green (18 pass + 1 ignored)
- `docs/book/src/substrate/storage.md` — FOUND
- `docs/book/src/SUMMARY.md` — FOUND (`[Storage](./substrate/storage.md)` nested under Substrate)
- Commit `53dc78c` — FOUND in `git log --oneline -10`
- Commit `f16fb29` — FOUND in `git log --oneline -10`
