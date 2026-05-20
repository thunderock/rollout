# Coordinator

`rollout-coordinator` is the **Phase-2 minimal control plane**. It registers
workers, accepts heartbeats, persists the worker registry and heartbeat ledger
to local `EmbeddedStorage`, and surfaces deadline-detected failures.

## Scope

Phase 2 explicitly ships only the heartbeat-receiver slice:

| In scope (Phase 2) | Out of scope (Phase 6 `DIST-01..05`) |
| --- | --- |
| Register worker, accept heartbeat | Work distribution / pull / submit |
| Persist `workers/*` + `heartbeats/*` to Storage | Coordinator lease / CAS / HA |
| Deadline-based failure scan + tracing events | Multi-coordinator handoff |
| Mount the three `rollout-transport` services | Restart-from-storage 4-node test |

The same binary scales to the full distribution story later — Phase 6 adds
work-stealing on top of the existing transport + storage wiring.

## Storage layout

Two redb tables (`rollout-storage` provides them; this crate only writes):

- Namespace `workers`, path `[<worker_id>]` → postcard-encoded `WorkerRegistryEntry`
- Namespace `heartbeats`, path `[<worker_id>]` → postcard-encoded `HeartbeatRecord`

Heartbeats are **overwrite-on-write** in Phase 2 — only the latest beat is
kept. Phase 6 may bolt on a ledger if work-stealing needs history.

## Failure-detection formula

A worker is marked failed iff

```
elapsed_past_due = now - due_at
elapsed_past_due > clock_skew_budget
elapsed_past_due > coordinator_failure_timeout
```

Both thresholds must trip (spec 05 §6). The defaults (CONTEXT D-TIME-01):

| Constant | Value |
| --- | --- |
| `heartbeat_interval` | 500 ms |
| `worker_self_fence_timeout` | 4 s |
| `coordinator_failure_timeout` | 5 s |
| `clock_skew_budget` | 250 ms |

Plan-time invariants enforced by `TransportConfig::validate_cross_fields`:

1. `worker_self_fence_timeout < coordinator_failure_timeout` (split-brain
   prevention).
2. `clock_skew_budget < 2 × heartbeat_interval`.

## Failure-scan loop

The scan ticks every `heartbeat_interval / 2` (250 ms by default) so a single
missed beat is detected within `2 × heartbeat_interval` — the SUBSTR-02
acceptance criterion #3. Each tick scans the `heartbeats/*` namespace,
decodes via postcard, and emits two outputs per overdue worker:

- A `tracing::warn!` line with `target = "coordinator"` and
  `worker_id = <id>` + `due_at_ms = <ms>` fields.
- An `Event { kind: EventKind::Domain { topic: "worker_failed" }, level:
  Warn, … }` via the injected `EventEmitter` (see D-OBSERVE-01 below).

Already-failed workers are tracked in an in-memory `HashSet` so the loop
emits **exactly one** failure event per worker per coordinator lifetime.

## Observability (D-OBSERVE-01)

`StdoutJsonEmitter` is the Phase-2 sink for `rollout_core::EventEmitter`:

- Holds a `Mutex<tokio::io::Stdout>` so concurrent emits don't interleave.
- Writes one NDJSON line per event using `serde_json`.
- Flushes after each event.

`CoordinatorImpl::new(storage, run_id, emitter)` takes the emitter as an
`Arc<dyn EventEmitter>` so non-stdout backends drop in without code change
in later phases. The coordinator emits:

- `worker_registered` — on the first `register()` (or first heartbeat from
  an unknown worker; see CLI section).
- `worker_heartbeat` — on every accepted beat.
- `worker_deregistered` — on graceful drain.
- `worker_failed` — from the failure-scan loop.

Tests use `NoopEmitter` (also in `emitter.rs`) which discards every event.

## CLI

### `rollout coordinator run`

```
rollout coordinator run --config <path>
```

Boots the coordinator from a TOML file:

```toml
run_id = "01JZ..."        # ULID
[storage]
path = "./data/rollout.db"
[transport]
listen_addr = "127.0.0.1:50051"
tls_dir = "./data/tls"
```

The same logic ships as a standalone binary `rollout-coordinator` (Cargo
`[[bin]]`) so the smoke-test wrapper can invoke it directly without
`rollout-cli` in the path.

### `rollout worker run`

```
rollout worker run --config <path> [--worker-id <ulid>] [--plugin <manifest.toml> ...] [--hot-reload]
```

The Phase-2 worker:

1. Opens its own `EmbeddedStorage`.
2. Builds a `PluginHostImpl::with_storage(...)` and `load()`s each
   `--plugin` manifest.
3. Dials the coordinator over mTLS using an ephemeral client cert issued
   from the dev CA at `./data/tls/`.
4. Sends `Beat(state=Init)` immediately; the coordinator auto-registers
   the worker on first heartbeat (the proto has no separate `register`
   RPC).
5. Beats every `heartbeat_interval` after that, advancing
   `state -> Ready`.
6. On SIGTERM: sends `Beat(state=Draining)` and exits 0.

## First-run UX

On first boot, the coordinator generates `data/tls/ca.pem` +
`data/tls/ca.key.pem` (chmod 600 on the key) and prints:

```
Generated dev CA at ./data/tls/ca.pem
```

Subsequent runs are idempotent (read-through). The CA + per-host certs
follow `rcgen 0.13` defaults — adequate for dev; production should swap
in a real CA in a later phase.
