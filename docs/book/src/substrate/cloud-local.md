# Cloud-local substrate

`rollout-cloud-local` ships the Phase-2 Layer-1 implementations so the rest of
the stack has a real `ObjectStore` / `Queue` / `SecretStore` / `ComputeHint` to
target with **zero cloud creds**. Per CONTEXT D-LOCAL-01..05.

## What ships

| Capability     | Type                | Implementation                                                                                                 |
| -------------- | ------------------- | -------------------------------------------------------------------------------------------------------------- |
| Blob storage   | `FsObjectStore`     | Content-addressed two-level sharded FS under `./data/object-store/`; sibling `<hex>.meta.json` per blob.        |
| Work queue     | `InMemQueue`        | `tokio::sync::Mutex<VecDeque<_>>` hot path + spill to `rollout-storage` (`cloudlocal_queue` namespace).         |
| Secrets        | `EnvSecretStore`    | Read-only env-var allowlist (`ROLLOUT_SECRET_<NAME>`); `put()` returns `Fatal(ConfigInvalid)` by design.        |
| Compute hint   | `hints::*`          | Linux full (`/proc/cpuinfo` + `/proc/meminfo`, optional NVML feature); macOS minimal (sysinfo cpu+memory).      |

## What's deferred

- **`BlockStore`** — D-LOCAL-05: declared in `rollout-core`, not implemented
  here; opt-in for clouds that need it.
- **Sandboxing beyond network allowlist** — Phase 7, when the tool harness
  lands and untrusted-code isolation matters (cgroups + seccomp + FD limits +
  fs write restrictions).
- **Real cloud backends** (`rollout-cloud-aws`, `rollout-cloud-gcp`) — Phase 5.

## `FsObjectStore` layout (D-LOCAL-01)

```text
./data/object-store/
├── ab/                       <- hex[0..2]
│   └── cd/                   <- hex[2..4]
│       ├── abcd…fullhash     <- blob
│       └── abcd…fullhash.meta.json
└── ...
```

Writes are tmp-then-rename for atomicity. Idempotent for repeated puts of the
same bytes (same `ContentId`, no double-write). `get_bytes` on a missing id
returns `Fatal(Internal("object not found: …"))` — choosing `Fatal` because a
missing content hash signals an upstream contract violation, not a transient
I/O fault.

## Queue restart semantics (D-LOCAL-02)

Every `enqueue` writes through a `Storage` transaction under
`cloudlocal_queue/<ulid>` BEFORE the item is pushed onto the in-memory deque.
`ack` deletes the storage entry; `nack` re-pushes the item to the front of the
deque without touching storage so the next restart still replays it.

On `InMemQueue::open(storage)`, the queue scans `cloudlocal_queue/*`, decodes
each `QueueItemId(Ulid)` from the path segment, sorts by ULID (which is
k-sortable so this recovers enqueue order), and rebuilds the deque. This honors
the **spirit of DIST-03** (restart replay) for the local backend; the full
DIST-01..05 fault-tolerance work lands in Phase 6.

## Secret allowlist (D-LOCAL-03)

`EnvSecretStore::new(allowlist)` accepts a list of secret names (without the
`ROLLOUT_SECRET_` prefix). At `get(name)` time:

| Condition                                         | Result                                          |
| ------------------------------------------------- | ----------------------------------------------- |
| `name` not in allowlist                           | `Err(Fatal(ConfigInvalid("not in allowlist")))` |
| `name` allowed, `ROLLOUT_SECRET_<name>` set       | `Ok(value)`                                     |
| `name` allowed, env var unset                     | `Err(Recoverable(Transient, RetryHint::Never))` |
| `put(name, value)` — ALWAYS                       | `Err(Fatal(ConfigInvalid("read-only")))`        |

The "allowed but unset" case is recoverable rather than fatal because the
operator can provision the variable without changing config; subsequent calls
will succeed.

## GPU inventory (D-LOCAL-04)

GPU enumeration is **opt-in** behind a `nvml` Cargo feature. When the feature
is off (default), `LinuxComputeHint::inventory().gpus` is always empty. When
the feature is on but `libnvidia-ml.so` is missing or NVML init fails, the
inventory still returns an empty `gpus` vector — **never errors**. This keeps
local dev machines and CI runners (no GPU) functional without conditional code
on the caller.

macOS skips GPU inventory entirely.

## Tests

| File                                    | Coverage                                                        |
| --------------------------------------- | --------------------------------------------------------------- |
| `tests/object_store.rs` (6)             | Round-trip, sharded layout, meta sidecar, exists, idempotency, fatal-on-missing |
| `tests/secrets.rs` (4)                  | Allowlist read, outside-allowlist fatal, unset transient, put fatal             |
| `tests/queue_replay.rs` (5)             | FIFO ULID order, nack-to-front, **restart replay**, ack removes, nack keeps     |
| `tests/hints_macos.rs` (2, `cfg=macos`) | CPU + memory present, preemption signal `None`                  |
| `tests/hints_linux.rs` (3, `cfg=linux`) | `/proc/cpuinfo` parse, `/proc/meminfo` parse, no GPUs without `nvml` feature    |

Linux-only tests are `#[cfg(target_os = "linux")]` so workspace `cargo test
--tests` stays green on every host. The NVML integration test is additionally
`#[ignore]`d behind `--ignored` since it requires a live `libnvml`.
