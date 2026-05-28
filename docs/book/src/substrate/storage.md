# Storage

`rollout-storage` is the Phase-2 embedded `Storage` + `StorageTxn` impl, backed by [redb](https://docs.rs/redb) 2.5. It is the **default backend** in v1 per CONTEXT D-STO-01. The Postgres backend lives in Phase 4 (TRAIN-04).

## Backend choice

[Spec 04 §3.1](../../../specs/04-storage-snapshots.md) prefers redb: pure-Rust, single-file MVCC, copy-on-write, no compaction stalls. The async `Storage` trait wraps the sync redb API via `tokio::task::spawn_blocking`.

## Table layout

Each `StorageKey.namespace` maps to its own redb `TableDefinition<&[u8], &[u8]>`. The Phase-2 namespaces are:

| Table              | Purpose                                            |
| ------------------ | -------------------------------------------------- |
| `runs`             | Per-run metadata                                   |
| `workers`          | Worker registry (coordinator-owned)                |
| `heartbeats`       | Heartbeat ledger; coordinator scans this on `due_at` |
| `queue`            | Generic queue spill                                |
| `plugins`          | Plugin manifest cache                              |
| `cloudlocal_queue` | `rollout-cloud-local` queue restart-replay mirror  |

Adding a new namespace = adding a `const TableDefinition` in `embedded::tables.rs` and a `match` arm in `table_for`. No migration; redb opens new tables lazily on first write.

## Key encoding

Namespace selects the table, so the redb key bytes only encode `(run_id, path)`. The encoding is a single `postcard::to_allocvec(&(&run_id, &path))` so decoding is unambiguous regardless of any `0x00` bytes inside `Option<RunId>`. See `crates/rollout-storage/src/encoding.rs`.

## Value encoding

`postcard` per D-STO-04 — compact, schemafull, deterministic, serde-native. The `Storage` trait is bytes-only (`get_bytes` / `put_bytes` / `cas_bytes`); downstream crates layer postcard over the trait via free helpers when typed payloads are needed.

## Durability

Always-fsync per D-STO-03. Each `WriteTransaction` calls `set_durability(Durability::Immediate)` before staging writes; redb's `commit()` blocks until fsync completes. Default DB path is `./data/rollout.db`, overridable via `[storage.embedded] path = ...` in the run config.

## watch() semantics

`Storage::watch(prefix) -> tokio::sync::broadcast::Receiver<StorageEvent>` returns an **in-process** subscriber to commits whose key extends `prefix`. The `WatchRouter` lives inside `EmbeddedStorage`; per-prefix `broadcast::Sender`s are created on first subscribe.

The critical invariant (RESEARCH §"Pattern 2"): redb has **no post-commit hook**. The `EmbeddedTxn` buffers `StorageEvent`s inside the in-flight txn; `commit()` only fans them out to the router **after `WriteTransaction::commit()` returns `Ok`**. On `abort()` (explicit or via drop), the buffered events are discarded — subscribers never observe rolled-back writes.

```text
begin()  → buffer = []
put_bytes(K, V) → buffer.push(Put{K})
commit() → spawn_blocking(commit) → on Ok: for evt in buffer { watch.publish(evt) }
abort()  → drop(txn); buffer is dropped
```

## Phase-2 scan semantics

`Storage::scan_bytes(KeyRange { prefix, limit })` returns an owned `Vec<(StorageKey, Vec<u8>)>` rather than the `BoxStream` shown in spec 04 §2. The async-trait + object-safety constraint on stable Rust forbids stream-returning methods on `dyn Storage`; the simplification is documented in spec 04 §1a. Phase-2 callers (coordinator deadline scan, cloud-local restart replay) work with small per-namespace working sets where the owned-vec cost is negligible. A future `StorageStream` newtype can lift this restriction.

The scan iterates the namespace table and decodes each key on the way out; namespace already partitions the data, so tables stay small. `limit` short-circuits the iteration.

### Postgres path constraint (ASCII-printable)

The Postgres backend stores `StorageKey.path` as `TEXT[]`. Path components must be **ASCII-printable** (bytes `0x20`–`0x7E`); non-printable / NUL / high-bit bytes cannot round-trip through `TEXT[]` and would silently diverge from redb's byte-lex prefix scan (PITFALLS.md §17). Every Postgres CRUD and scan entry-point calls `StorageKey::validate_for_postgres()` and rejects offending keys with `Fatal(ConfigInvalid)` before any SQL runs. For binary identifiers in path components, use `hex::encode` at the `StorageKey` construction site. A 256-case proptest (`tests/postgres_scan_bytes_parity.rs`) witnesses byte-parity between redb and Postgres over the printable-ASCII range.

## Cross-process watch

NOT supported. The broadcast channel lives in process memory; another `EmbeddedStorage` instance pointing at the same file will not receive events. Cross-process watch arrives with the Postgres backend in Phase 4 (`LISTEN`/`NOTIFY`).

## Crash safety

The on-disk format is crash-safe at the redb commit boundary: a successful `commit()` implies fsync; anything before that is invisible on reopen. The active test in `tests/crash_safety.rs` exercises the abort-style variant (drop txn without commit, reopen, assert no keys visible). A "true SIGKILL between put and commit" variant is `#[ignore]`d for now — it needs a helper-binary harness with raw signals; Phase-6 `DIST-03` (restart-from-storage) will land that.

## Tests

| File                      | Scope                                                                |
| ------------------------- | -------------------------------------------------------------------- |
| `tests/crud.rs`           | put/get/delete/scan/get_many/ping                                    |
| `tests/tables.rs`         | per-namespace isolation; six-namespace reopen                        |
| `tests/txn.rs`            | commit/abort/cas (insert-only / CAS / delete-if-equal)               |
| `tests/watch.rs`          | publish-after-commit; abort suppresses; multi-subscriber; prefix isolation |
| `tests/crash_safety.rs`   | drop-without-commit reopen; SIGKILL variant `#[ignore]`d             |
