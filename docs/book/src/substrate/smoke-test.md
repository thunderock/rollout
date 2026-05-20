# Smoke test

`make smoke` is the SUBSTR-02 / SUBSTR-03 / SUBSTR-04 acceptance gate. It runs
the live substrate end-to-end on a clean checkout with no cloud credentials
and proves that deadline-based failure detection works on the wire.

## What it proves

The smoke test boots the full Phase-2 substrate and exercises every concern in
one run:

- **`rollout-storage`** opens three independent redb files (one per
  coordinator / w1 / w2) with always-fsync durability.
- **`rollout-transport`** brings up the HTTP/2 listener with an
  auto-generated dev CA + per-host mTLS certificates.
- **`rollout-plugin-host`** loads two plugins per worker — a Rust cdylib and
  a Python sidecar (stdlib-only framed-JSON over UDS).
- **`rollout-coordinator`** persists the worker registry + heartbeat ledger
  and runs the deadline-based failure scan that emits `worker_failed` for any
  worker overdue by more than `coordinator_failure_timeout + clock_skew_budget`.

When the script kills `w1` with `SIGKILL`, the coordinator must observe and
emit the failure event for `w1`'s ULID inside the deadline budget — that is
the contract the substrate ships.

## Topology

```
                        coord.log (NDJSON events)
                          ▲
                          │
    ┌─────────────────────┴─────────────────────┐
    │            rollout-coordinator            │
    │  ./data/smoke/coord.db   tls/ca.pem       │
    │  listen 127.0.0.1:50051 (mTLS, H/2)       │
    └───────────────┬──────────────┬────────────┘
                    │              │
              heartbeat 500ms  heartbeat 500ms
                    │              │
    ┌───────────────▼──┐     ┌────▼─────────────┐
    │  rollout worker w1│     │ rollout worker w2 │
    │  w1.db            │     │ w2.db             │
    │  plugins:         │     │ plugins:          │
    │    cdylib sample  │     │   cdylib sample   │
    │    sidecar sample │     │   sidecar sample  │
    └───────────────────┘     └───────────────────┘
```

Both workers load both plugin samples; the cdylib path is built on demand by
the script (`cargo build --manifest-path .../rust_cdylib_sample/Cargo.toml`)
and the Python sidecar resolves via `PYTHONPATH=python/examples`.

## Timeline

| Step | Wall-clock budget | Asserted invariant |
|------|------------------|--------------------|
| Build binaries + cdylib    | ~30 s (cold) / instant (cached) | exit 0 |
| Spawn coordinator           | < 1 s | port 50051 up |
| Spawn w1 + w2               | < 1 s | both PIDs alive |
| Wait heartbeat-stable       | < 5 s | `worker_heartbeat` for both ULIDs in coord.log |
| `kill -KILL w1`             | instant | — |
| Detect `worker_failed(w1)`  | < 8 s (D-TIME-01 floor: `2 × heartbeat_interval + skew`) | `worker_failed` topic + w1 ULID in coord.log |

The 8-second detection deadline is well inside the spec contract: at the
locked Phase-2 defaults (`heartbeat_interval = 500 ms`,
`coordinator_failure_timeout = 5 s`, `clock_skew_budget = 250 ms`), the
failure-scan loop ticks at 250 ms and fires the event within
`coordinator_failure_timeout + clock_skew_budget ≈ 5.25 s` of the missed
beat — observed locally at ~5.2 s.

## Where logs go

All smoke artifacts land under `./data/smoke/` (gitignored):

| Path | Purpose |
|------|---------|
| `data/smoke/coord.db`       | Coordinator embedded storage |
| `data/smoke/w1.db`, `w2.db` | Per-worker embedded storage  |
| `data/smoke/tls/`           | Auto-generated dev CA + per-host certs |
| `data/smoke/sidecars/`      | Per-plugin UDS sockets       |
| `data/smoke/logs/coord.log` | Coordinator stdout (NDJSON spec-09 events + tracing) |
| `data/smoke/logs/w1.log`, `w2.log` | Worker stdout (tracing) |
| `data/smoke/logs/w1.toml`, `w2.toml` | Per-worker TOML (sed-rewritten from the shared fixture so each worker has its own db) |
| `data/smoke/logs/w1.id`, `w2.id`     | Pre-allocated worker ULIDs (smoke driver writes these so the grep step is deterministic) |

Searching `coord.log` for `worker_failed.*$W1_ULID` is the assertion the
script makes.

## Configuration fixtures

The committed fixtures at `tests/smoke/coordinator.toml` and
`tests/smoke/worker.toml` use the D-TIME-01 timing defaults verbatim. The
worker TOML carries a single storage path; the smoke driver derives
`data/smoke/w1.db` and `data/smoke/w2.db` at runtime with `sed` because redb
takes an exclusive file lock per `Database::create` and two worker processes
sharing the same file would conflict.

## CI integration

`.github/workflows/ci.yml` includes a `smoke` job on `ubuntu-latest` that
runs after the standard test job. The job installs `netcat-openbsd` for the
port-poll, runs `bash scripts/preflight.sh`, then `make smoke`. On failure,
all `data/smoke/logs/` contents are uploaded as a `smoke-logs` artifact for
post-mortem.

The pre-existing 11 CI jobs (`lint`, `test`, `deny`, `commitlint`,
`schema-drift`, `architecture-lint`, `unused-deps`, `rustdoc-check`,
`docs-build`, `docs-deploy`, `docs-test-policy`) are untouched — no
`continue-on-error`, no skipped steps. The `architecture-lint` job
automatically picks up the 4th invariant added in `dependency_direction.rs`
(rollout-coordinator must not depend on rollout-plugin-host or any
cloud-layer crate) without any YAML changes.

## First-run UX

On first invocation the coordinator emits

```
Generated dev CA at ./data/smoke/tls/ca.pem
```

to stderr and proceeds. No manual `openssl` steps are required; `rcgen`
mints the CA + per-host certs in-process. The `tls/` directory is gitignored
along with the rest of `data/`.

## How to extend

- **Add a plugin:** drop a manifest TOML under `tests/smoke/plugins/`, then
  add a `--plugin <path>` flag to the worker spawn lines in `scripts/smoke.sh`.
- **Change timings:** edit `tests/smoke/coordinator.toml` and
  `tests/smoke/worker.toml`; the cross-field invariants
  (`worker_self_fence_timeout < coordinator_failure_timeout`,
  `clock_skew_budget < heartbeat_interval × 2`) are enforced at
  `validate_cross_fields` time and failing configs early-exit.
- **Add a worker:** copy the `spawn_worker` block; allocate a new ULID; add
  the per-worker `sed`-rewrite line for the storage path.

## Reproducing locally

```bash
bash scripts/preflight.sh   # verifies cargo + make + python3 >= 3.11
make smoke
```

Total wall-clock is dominated by the cold cargo build (~30 s on a warm
laptop); the actual integration test runs in ~7 s once binaries exist.

If `python3 --version` reports < 3.11, install a newer interpreter
(`brew install python@3.13`, `pyenv install 3.13.3`, or your distro's
`python3.11` package) and re-run; the preflight check guards against the
runtime-incompatible interpreter.
