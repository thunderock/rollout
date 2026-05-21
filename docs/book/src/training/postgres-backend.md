# Postgres backend

Phase 4 / TRAIN-04: a `Storage` impl backed by Postgres 16 alongside the
default embedded redb store. Same trait, same semantics — choose at config
time. The crate-level Cargo feature `postgres` on `rollout-storage` gates
the dep set (sqlx 0.8 + uuid + ulid + async-stream); default builds remain
sqlx-free.

## Why Postgres alongside embedded

The embedded backend (redb) is local-process only. Multi-process or
multi-host runs need a shared store that fan-outs change notifications
across processes — that's what Postgres gives us via `LISTEN/NOTIFY`. The
trait surface stays uniform: `EmbeddedStorage` and `PostgresStorage` both
implement [`Storage::watch_stream`], wrapping their respective notification
mechanisms in a `BoxStream<StorageEvent>`.

| Method                   | Embedded                                  | Postgres                                              |
| ------------------------ | ----------------------------------------- | ----------------------------------------------------- |
| `begin / put / get / cas`| redb txn over local fs                    | sqlx txn over network                                 |
| `watch` (broadcast)      | `tokio::sync::broadcast::Receiver`        | unsupported — returns `Fatal(PluginContract)`         |
| `watch_stream`           | wraps the broadcast in `BroadcastStream`  | `PgListener` over `LISTEN/NOTIFY`                     |

Cross-process subscribers MUST use `watch_stream`. The embedded backend
implements `watch_stream` by wrapping its in-process broadcast — handy when
some callers need a stream-shaped surface even on a local run.

## Schema

Two migrations under `database/migrations/` are embedded at build time via
`sqlx::migrate!()`:

- `0001_init.sql` — the `kv` table that backs all `StorageKey` rows.
- `0002_snapshots.sql` — `snapshots` + `events` tables consumed by
  `rollout-snapshots` (plan 04-01) and the spec-09 observability ledger.

The `runs` / `workers` tables defer to Phase 6 (multi-node distribution).

### `kv` table

```sql
CREATE TABLE kv (
    namespace   TEXT NOT NULL,
    run_id      UUID,                       -- ULID-as-UUID; NULL for global rows
    path        TEXT[] NOT NULL,
    value       BYTEA NOT NULL,
    version     BIGINT NOT NULL DEFAULT 0,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (namespace, run_id, path)
);
```

`run_id` is stored as `UUID` (16 bytes) — the same byte layout as the
underlying ULID. `RunId(Ulid)` round-trips through `Uuid::from_bytes` and
back without loss.

## LISTEN / NOTIFY contract

A row-level trigger on `kv` fires `pg_notify(channel, payload)` after every
INSERT/UPDATE/DELETE:

- **channel**: `rollout_watch_<namespace>` (max 63 chars; `rollout_watch_`
  is 14, leaving 49 for the namespace).
- **payload**: `<run_id_uuid_or_empty>|<path_parts_joined_by_slash>`,
  truncated to 7999 bytes per Pitfall 5 (Postgres caps `pg_notify` payloads
  at 8000).

`PgListener` consumers parse the payload, filter by prefix (matching the
caller's `prefix.run_id` + `prefix.path`), and emit a `StorageEvent::Put`
per notification. Put vs delete is not distinguished in the payload format
shipped here; Phase 9 may extend the trigger to include a `+/-` prefix if
downstream needs demand it.

`PgListener` reconnects transparently on connection drop; the `watch_stream`
loop logs the failure and continues.

## Trait surface

`Storage::watch` (broadcast) is intentionally **not implemented** by the
Postgres backend — broadcast is an in-process abstraction. Callers that
need a `tokio::sync::broadcast::Receiver` must run against `EmbeddedStorage`
or implement their own fan-out on top of `watch_stream`. The Postgres impl
returns:

```text
Fatal(PluginContract { plugin: "PostgresStorage",
    msg: "PostgresStorage does not support in-process broadcast watch;
          use watch_stream for cross-process notification" })
```

## Migrations

Migrations are forward-only, sequentially numbered (`0001_*.sql`,
`0002_*.sql`, ...). `PostgresStorage::new` runs them once via
`sqlx::migrate!("../../database/migrations").run(&pool)`. The macro embeds
every SQL file into the binary at compile time, so the pool only needs
network access — no on-disk migration directory in production.

Running `new()` twice on the same pool is a no-op: `sqlx::migrate` records
applied versions in `_sqlx_migrations`.

### Adding a migration

1. Write `database/migrations/NNNN_<name>.sql`.
2. Start a local Postgres: `docker run --rm -e POSTGRES_PASSWORD=pw -p 5432:5432 postgres:16`.
3. (Optional, when we switch to compile-time `query!` macros) regenerate
   the `.sqlx/` offline cache:
   `DATABASE_URL=postgres://postgres:pw@localhost/postgres SQLX_OFFLINE=false cargo sqlx prepare --workspace -- --features postgres`.
4. Commit the migration AND the regenerated `.sqlx/` files (currently the
   directory only carries a `.gitkeep`; this plan ships runtime-checked
   SQL via `sqlx::query`).

## Offline mode

`SQLX_OFFLINE=true` lives in `.cargo/config.toml` (not `.env`) per Pitfall 4
— `sqlx-cli` reads `.env` at startup and refuses to talk to the DB during
`cargo sqlx prepare` if `SQLX_OFFLINE=true` is set there. Putting it in
`.cargo/config.toml` keeps `cargo build --features postgres` happy
everywhere except when explicitly running `sqlx prepare`.

Phase 4 ships runtime-checked `sqlx::query` calls; the `.sqlx/` directory
is reserved (`.gitkeep`) for a future switch to compile-time `query!`
macros once the surface stabilizes.

## Pool sizing

`PostgresStorage::new(url, pool_size)` opens two pools:

| Pool         | min | max          | acquire    | idle      |
| ------------ | --- | ------------ | ---------- | --------- |
| Write pool   | 0   | `pool_size`  | 30 s       | 10 min    |
| Watch pool   | 0   | 4            | 30 s       | (default) |

`pool_size = 16` is a reasonable default for a Phase-6 worker; tests use
4. The watch pool is small and dedicated because each `PgListener` holds a
connection for the lifetime of its `watch_stream`; you don't want them
crowding out write capacity.

## Testcontainers CI

`crates/rollout-storage/tests/postgres_integration.rs` runs against a
disposable Postgres 16 container via `testcontainers-modules`. Six tests
cover CRUD, CAS atomicity, watch_stream delivery, migration idempotency,
pool reuse under load, and prefix scan.

Each test carries `#[ignore = "requires Docker / testcontainers"]` so the
default `cargo test --workspace --tests` flow (macOS dev loop, no Docker
guaranteed) stays green. CI opts in via:

```bash
cargo test -p rollout-storage --features postgres \
  --test postgres_integration -- --include-ignored --test-threads=1
```

`--test-threads=1` because the test starts a fresh container per test
function; running six simultaneously is wasteful (and slower than
serial under most CI runners' Docker capacity).

The retry loop on `PostgresStorage::new` (30 attempts × 2 s) handles
Pitfall 6: the container reports "running" before Postgres accepts
connections.

## Local dev

```bash
make postgres-test
```

The target checks `docker info` first and fails fast with a helpful
message if Docker isn't running.

## Limitations

- **Put-only events from `pg_notify`.** The trigger emits a single
  notification format for both INSERT/UPDATE/DELETE; consumers see
  `StorageEvent::Put` only. Phase 9 may extend the trigger payload to
  carry a `+/-` operation byte if downstream callers need to distinguish
  deletes.
- **`get_many_bytes` is sequential.** A future optimization could batch
  via `ANY($1)` array binding; not on the Phase-4 critical path.
- **No streaming `scan_bytes`.** Phase-2 simplification carries forward;
  prefix scans return owned `Vec` rows. Hot prefixes (millions of rows)
  will need a streaming variant in Phase 6.
- **No `query!` macro yet.** Runtime-checked SQL keeps the build hermetic
  without a `.sqlx/` cache; revisit once the schema stabilizes.

[`Storage::watch_stream`]: https://docs.rs/rollout-core/latest/rollout_core/trait.Storage.html#tymethod.watch_stream
