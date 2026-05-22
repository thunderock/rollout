# `rollout train` + `rollout snapshot` CLI

Phase-4 user-facing entry points for supervised fine-tuning, reward-model
training, and snapshot management. Mirrors the Phase-3 `rollout infer batch`
clap derive shape, run_id lifecycle, and backend-selection precedence.

## Subcommand overview

| Subcommand                  | Purpose                                                       |
| --------------------------- | ------------------------------------------------------------- |
| `rollout train sft`         | Supervised fine-tuning. Validates + runs an `SftAlgo` budget. |
| `rollout train rm`          | Reward-model (Bradley-Terry) training via `RmAlgo`.           |
| `rollout snapshot list`     | List snapshots (optionally filtered by run / kind).           |
| `rollout snapshot show`     | Print one snapshot's metadata by content-id.                  |
| `rollout snapshot prune`    | Delete snapshots per a retention policy.                      |

## `rollout train sft`

```bash
rollout train sft \
    --config examples/sft-tiny.toml \
    [--resume <snapshot_id>] \
    [--dry-run]
```

| Flag        | Default  | Purpose                                                                                  |
| ----------- | -------- | ---------------------------------------------------------------------------------------- |
| `--config`  | required | Path to the run TOML (schema below).                                                     |
| `--resume`  | none     | Snapshot content-id to restore from before the algorithm runs.                           |
| `--dry-run` | `false`  | Validate config + dataset path; never construct backend. Works with no backend feature.  |

### SFT TOML schema

```toml
schema_version = 1

[storage]
backend = "embedded"
[storage.embedded]
path = "./rollout.db"

[algorithm]
kind = "sft"

[algorithm.sft]
minibatch_size       = 1
gradient_accumulation = 1

[algorithm.sft.base_model]
uri = "Qwen/Qwen2.5-0.5B-Instruct"

[algorithm.sft.optimizer]
kind = "sgd"
lr   = 0.01

[algorithm.sft.budget]
max_steps = 10

[algorithm.sft.dataset]
kind = "jsonl_path"
path = "./data/sft.jsonl"

[algorithm.sft.packing]
kind        = "off"
max_seq_len = 64

[algorithm.sft.loss_on]
kind = "full"
```

The schema is owned by `rollout-core::config::RunConfig`; the CLI is just a
TOML-loader + clap surface (spec 11 single-source-of-truth). Unknown fields
fail load with a deterministic locator.

## `rollout train rm`

```bash
rollout train rm \
    --config examples/rm-tiny.toml \
    [--resume <snapshot_id>] \
    [--dry-run]
```

Same flags as `sft`. The TOML differs only in the algorithm block:

```toml
schema_version = 1
[storage]
backend = "embedded"
[storage.embedded]
path = "./rollout.db"

[algorithm]
kind = "rm"

[algorithm.rm]
minibatch_size = 1

[algorithm.rm.base_model]
uri = "Qwen/Qwen2.5-0.5B-Instruct"

[algorithm.rm.optimizer]
kind = "sgd"
lr   = 0.01

[algorithm.rm.budget]
max_steps = 10

[algorithm.rm.dataset]
kind = "jsonl_path"
path = "./data/pairs.jsonl"

[algorithm.rm.head]
kind = "bradley_terry"
```

JSONL input contract: each line `{ "prompt", "chosen", "rejected" }`.

## `--dry-run` semantics

In order, with the backend NEVER constructed:

1. Read + parse TOML against `RunConfig` (`deny_unknown_fields`).
2. Match `[algorithm].kind` against the subcommand (`sft` vs `rm`).
3. Validate `minibatch_size >= 1` and `optimizer.lr > 0`.
4. Confirm `dataset.path` exists on disk (for `jsonl_path` form).
5. Print `dry-run OK: algorithm=<sft|rm> model=… minibatch=… dataset=…` and
   exit `0`.

Because `--dry-run` short-circuits before `select_backend`, it runs cleanly on
builds with NEITHER `--features train` NOR `--features test-mock-backend`.

## `--resume <snapshot_id>` lifecycle

If `--resume <hex>` is set, the CLI scans the local `rollout.db` for a
`Snapshot` whose `id` matches the supplied content-id (32-byte blake3, 64-char
hex). The matching row is fed into `algorithm.snapshot_restore(snap)` before
`algorithm.run(&ctx)` begins. Determinism then follows the per-algorithm
contract: `SftAlgo` proves byte-identical resume via
`tests/snapshot_resume.rs::bit_identical_resume_at_step_5`; `RmAlgo` proves
parity via `tests/snapshot_resume.rs::rm_bit_identical_resume`.

Missing snapshot → `Fatal(ConfigInvalid)` with the failed hex echoed back.

## Backend selection

`rollout-cli` builds with at most one training backend at a time, by Cargo
feature. Selection at runtime is precedence-ordered:

| Order | Build flags                              | Env                                | Backend                                                            |
| ----- | ---------------------------------------- | ---------------------------------- | ------------------------------------------------------------------ |
| 1     | `--features test-mock-backend`           | `ROLLOUT_TEST_MOCK_BACKEND=1`      | `rollout_runtime_batch::MockBackend` (deterministic, no GPU/Python).|
| 2     | `--features vllm,train`                  | (any)                              | `rollout_backend_vllm::VllmBackend` in train mode (live HF / accelerate). |
| 3     | none                                     | (any)                              | `Fatal(ConfigInvalid)` with a build-mode hint; `--dry-run` still works. |

Build recipes:

```bash
# CI / tests (no Python, no GPU)
cargo build -p rollout-cli --features test-mock-backend

# Production (live HuggingFace + accelerate over Python)
cargo build -p rollout-cli --features vllm,train

# Both — vllm takes precedence at runtime unless ROLLOUT_TEST_MOCK_BACKEND=1.
cargo build -p rollout-cli --features test-mock-backend,vllm,train
```

The `train` feature implies `vllm` per `Cargo.toml`; the two are kept distinct
so the existing Phase-3 `infer batch` users get `vllm` without pulling the
training Python deps.

## `rollout snapshot list`

```bash
rollout snapshot list \
    [--storage-path ./rollout.db] \
    [--object-path ./object-store] \
    [--run-id <ULID>] \
    [--kind train_state|buffer|process|episodic_memory] \
    [--limit N]
```

Output: pretty-printed JSON array of `Snapshot` rows from `rollout-core`. Sort
order is newest-first by `created_at`.

| Flag             | Default          | Notes                                                                                |
| ---------------- | ---------------- | ------------------------------------------------------------------------------------ |
| `--storage-path` | `./rollout.db`   | Opens an `EmbeddedStorage` read-write.                                               |
| `--object-path`  | `./object-store` | Opens a `FsObjectStore` (read-only for `list`, but consistently surfaced).           |
| `--run-id`       | none             | Crockford ULID; restrict to one run. Without it, every run's snapshots are scanned.  |
| `--kind`         | none             | snake_case match against `SnapshotKind` variants.                                    |
| `--limit`        | none             | Cap result length.                                                                   |

## `rollout snapshot show <snapshot_id>`

```bash
rollout snapshot show \
    [--storage-path ./rollout.db] \
    [--object-path ./object-store] \
    <SNAPSHOT_ID>
```

`SNAPSHOT_ID` is the 64-char hex blake3 digest from `Snapshot.id`. Prints the
full `Snapshot` row as pretty-printed JSON. Missing id → exit 2 with
`snapshot not found: <id>`.

## `rollout snapshot prune`

```bash
rollout snapshot prune \
    --run-id <ULID> \
    [--storage-path ./rollout.db] \
    [--object-path ./object-store] \
    [--keep-last N=3] \
    [--keep-labeled]
```

Applies a `RetentionPolicy` scoped to a single run (`--run-id` is required to
avoid cross-run accidents). `--keep-last N` retains the N newest snapshots
regardless of label; `--keep-labeled` (default `true`) further retains every
labeled snapshot. Metadata rows are deleted; blob bytes stay in the object
store (the Phase-2 `ObjectStore` trait has no `delete` — pending Phase-5).
Prints `pruned <N> snapshots`.

## Storage path conventions

| Path                          | Owner            | Purpose                                  |
| ----------------------------- | ---------------- | ---------------------------------------- |
| `./rollout.db`                | `EmbeddedStorage`| redb on-disk DB (always-fsync).          |
| `./object-store/`             | `FsObjectStore`  | Content-addressed two-level sharded FS.  |
| `<config_dir>/rollout.db`     | training runs    | `train sft|rm` opens DB next to the config TOML by default. |
| `<config_dir>/object-store/`  | training runs    | Same sibling-of-config convention.       |

`snapshot list|show|prune` accept explicit `--storage-path` / `--object-path`
so out-of-band tooling and tests can target any directory pair.

## Exit codes

| Code | Meaning                                                                              |
| ---- | ------------------------------------------------------------------------------------ |
| 0    | Success (or successful `--dry-run`).                                                 |
| 2    | Config-invalid, missing dataset / snapshot, substrate error, or algorithm error.     |

## Observability

`RUST_LOG=info rollout train sft …` produces structured `tracing` events. Each
SFT / RM run emits at minimum `train_start`, `train_step`, `train_end`; the
exact fields are pinned by the per-algorithm chapters.
