# Resume: zero-duplicate batch restart

Phase 3's `rollout infer batch` is **resumable**. If the worker is killed mid-batch, restarting with `--resume <run_id>` (or with the same `[output] dir =` and no explicit flag) continues from the last persisted sample with **zero duplicates** and **no skipped inputs**. This is one of the three ROADMAP Phase-3 exit criteria.

## The lifecycle

Every `rollout infer batch` invocation has exactly one `run_id` — a 26-character ULID. The CLI resolves it via a three-tier precedence:

1. **`--resume <id>` explicit override.** Use this when you know the run you want to reattach.
2. **`<output.dir>/run-id` file.** If `--resume` is absent but the output directory already has a `run-id` file (from a prior run), the CLI reads it and continues that run. This is the common case for "kill + restart with the same config."
3. **Mint a fresh ULID.** No override, no prior file → generate a new `RunId`, write it atomically (`tempfile + rename`) to `<output.dir>/run-id`, and proceed as a brand-new run.

`run-id` files are single-line UTF-8 ULIDs. They are safe to delete (forces a fresh run on next invocation) but should not be edited by hand.

## How resumability is implemented

Three subsystems collaborate (each documented in its own chapter):

- **`rollout-storage`** (the redb-backed `EmbeddedStorage` from Phase 2) holds the per-sample state KV under namespace `infer`. Each sample's `SampleRecord` carries one of four `SampleState` variants — `Pending`, `Running`, `Done { completion_blob }`, or `Failed { reason }` — and transitions atomically via `Storage::cas_bytes`.
- **`rollout-cloud-local::InMemQueue`** (also Phase 2) holds the work queue with Storage spill: every enqueue/ack mirrors to redb under `cloudlocal_queue`, so a coordinator restart replays whatever was unacknowledged.
- **`rollout-cloud-local::FsObjectStore`** stores the actual completion blobs at content-addressed paths under `<output.dir>/object-store/<sha256[0..2]>/<sha256[2..4]>/<sha256>`. The `SampleRecord::state = Done { completion_blob: ContentId }` variant carries the blob's `ContentId`; the actual completion text is read from the object store at output-writing time.

At plan time, `BatchCoordinator::scan_and_enqueue` iterates the existing samples namespace and:

- skips records with `state = Done` (already complete; their completion blobs survive in the object store);
- re-enqueues records with `state = Pending` or `state = Failed`;
- treats records with `state = Running` and a `started_at` older than `stale_after_ms` (default 60 000) as stale — CAS `Running → Pending` and re-enqueue (per RESEARCH Pitfall 5: covers the kill-mid-flight case where a worker crashed without finishing).

Only the new and re-enqueued samples flow through the queue. Workers process them via the same `BatchWorker::run_loop` as a fresh run.

## The deterministic test

`crates/rollout-cli/tests/restart_no_duplicates.rs` is the load-bearing proof. It runs on **every** CI build — no GPU, no vLLM, no real model — because it uses the `MockBackend` shipped by `rollout-runtime-batch` behind the `test-mock-backend` Cargo feature.

The test follows RESEARCH §"Restart-resume test design" verbatim:

1. Spawn `rollout infer batch --config <tmp>` as a subprocess via `tokio::process::Command::new(env!("CARGO_BIN_EXE_rollout"))` with `ROLLOUT_TEST_MOCK_BACKEND=1`.
2. Stream stdout, count `sample_completed` events.
3. After 3 completions, `child.start_kill()` (SIGKILL).
4. Read `<output.dir>/run-id`.
5. Spawn a second subprocess with `--resume <run_id>` and the same env.
6. Wait for exit 0.
7. Assert the final `completions.jsonl` has exactly N=8 lines, all unique `sample_id`s, and every input prompt is represented once.

Run locally:

```bash
cargo test -p rollout-cli --features test-mock-backend --test restart_no_duplicates
```

Total wall-clock: ~1.5 s on a modern dev box. The MockBackend completes each sample in 50 ms; the test deliberately spawns subprocesses to exercise the real CLI resume code path including `--resume` parsing, `run-id` file I/O, and `BatchCoordinator::scan_and_enqueue`.

## Content-addressed sample IDs

A sample's identity is derived deterministically from `(model, prompt, sampling_params, input_index)` — see `crates/rollout-runtime-batch/src/state.rs::sample_id`. Per CONTEXT D-RESUME-01 (extended by RESEARCH Pitfall 1):

```rust
fn sample_id(model: &ContentId, prompt: &str, params: &SamplingParams, idx: u64) -> ContentId {
    let mut h = blake3::Hasher::new();
    h.update(&[SAMPLING_PARAMS_SCHEMA_VERSION]);  // = 1 in Phase 3
    h.update(model.as_bytes());
    h.update(prompt.as_bytes());
    h.update(&postcard::to_stdvec(params).unwrap());
    h.update(&idx.to_le_bytes());
    ContentId::from(h.finalize())
}
```

The leading `SAMPLING_PARAMS_SCHEMA_VERSION` byte is the maintenance hook: bumping the constant invalidates every outstanding sample-id, which is the desired behavior whenever the `SamplingParams` struct gains, loses, or reorders fields. Without it, a future serde-evolution of `SamplingParams` could silently produce different postcard bytes for the same logical configuration and break resume across versions.

## What you cannot resume

- **Cross-model resume.** A run whose `<output.dir>/run-id` was minted under model A cannot resume against model B — the per-sample `model_id` is part of the content-addressed sample ID; mixing models triggers `Fatal(ConfigInvalid)` at scan time.
- **Cross-machine resume** (Phase 3). The `EmbeddedStorage` redb file is local to the host. Phase 5 (`CLOUD-03`) introduces object-store-backed snapshot storage that will allow cross-machine resume; until then, resume is single-host.
- **Streaming runs.** Phase 3 rejects `sampling.stream = true` at plan time (D-BACKEND-03); streaming is Phase 8 (`INFER-01`).

## See also

- `cli.md` — full CLI flag reference for `--resume`.
- `cpu-mode.md` — where the MockBackend-driven test runs vs. live vLLM.
- `batch-runtime.md` — the `BatchCoordinator` and `BatchWorker` internals.
- `vllm-backend.md` — Pitfall-2 GIL bridge details that the live engine relies on.
