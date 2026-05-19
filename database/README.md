# database/

SQL schemas, migrations, and embedded-DB initialization scripts. The data model is described in [`/docs/specs/04-storage-snapshots.md`](../docs/specs/04-storage-snapshots.md); this directory holds the executable artifacts.

## Layout

```
database/
├── migrations/                 SQL migrations, applied in order by sqlx-migrate
│   ├── 0001_initial.sql
│   ├── 0002_...
│   └── ...
├── embedded/                   Embedded-DB schemas + seed scripts
│   └── schema.sql              (for the future SQLite-compatible embedded option, if chosen)
└── README.md                   ← you are here
```

## Migrations

Postgres migrations use `sqlx`'s migration format. Each migration:

- Is forward-only. Rollback migrations are out of v1 scope.
- Has a sequential 4-digit prefix (`0001`, `0002`, ...). Gaps not allowed.
- Includes a header comment with the date, author, and one-line description.
- Is small. A migration that touches many tables should be split unless atomicity demands otherwise.

### Tables (v1)

| Table | Purpose |
|---|---|
| `kv` | Generic namespaced KV store backing the `Storage` trait for non-specialized data |
| `runs` | Run summaries (one row per run) |
| `workers` | Worker registry (per-run) |
| `heartbeats` | Worker heartbeats (TTL-cleaned periodically) |
| `work_items` | Pull-based work queue |
| `snapshots` | Snapshot metadata (blobs in the object store) |
| `events` | Structured event log |
| `eval_reports` | Eval harness outputs |
| `plan_locks` | Plan files (small; stored in DB for queryability + retention) |

## Running migrations

```bash
# Against a local postgres
sqlx migrate run --database-url postgres://localhost/rollout

# From the CLI (runs migrations automatically before any storage op)
rollout runs list --storage.postgres.url=postgres://localhost/rollout
```

The `rollout` CLI runs migrations idempotently on startup; manual `sqlx migrate run` is rare.

## Embedded DB

The embedded backend (sled or redb — decided in Phase 2) uses a structured key scheme; there is no SQL schema. The init script in `embedded/` creates the initial namespace layout and writes a version marker.

## State: pre-implementation

Empty. First migration lands in Phase 4 (when the Postgres backend is introduced).
