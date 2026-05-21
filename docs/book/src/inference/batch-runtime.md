# `rollout-runtime-batch`

The runtime-glue crate behind `rollout infer batch`. Owns the CAS sample-state
machine, the queue management around the Phase-2 `InMemQueue`, JSONL I/O, the
`InferBatchConfig` TOML schema, and the test-only `MockBackend` that lets us
exercise the full coordinator/worker path without spinning up a real vLLM
engine.

## Why a separate crate?

`rollout-backend-vllm` (Layer 2) depends only on `rollout-core` + `pyo3` — it
has no business touching `rollout-storage` / `rollout-cloud-local` /
`rollout-transport`. The dep-direction lint (invariants #5 and #6 from plan
03-00) enforces this. The Phase-3 work that *does* need storage + queue +
object-store — the sample-state CAS machine, the resume scan, the JSONL
reader — lives here in `rollout-runtime-batch` (Layer 3). The runtime composes
any `Arc<dyn InferenceBackend>`; the backend stays cloud-agnostic.

```
+------------------+        rollout-cloud-local::InMemQueue
|  rollout-cli     |---┐    rollout-cloud-local::FsObjectStore
|  infer batch     |   |    rollout-storage::EmbeddedStorage
+------------------+   ▼
                  +-------------------------+      +-----------------------+
                  | rollout-runtime-batch   |◀────▶| Arc<dyn               |
                  |                         |      |  InferenceBackend>    |
                  | • BatchCoordinator      |      |                       |
                  | • BatchWorker           |      | • VllmBackend (prod)  |
                  | • SampleRecord + CAS    |      | • MockBackend (test)  |
                  | • JSONL I/O             |      +-----------------------+
                  | • InferBatchConfig      |
                  +-------------------------+
```

## `SAMPLING_PARAMS_SCHEMA_VERSION`

Per RESEARCH §"Pitfall 1", the deterministic `sample_id()` hash is brittle
against any change to `SamplingParams`'s wire shape. Postcard is *not*
self-describing — adding a field rewrites every byte of `to_stdvec(&params)`,
which would invalidate every outstanding `Pending` / `Running` sample-ID in
storage.

The defence is a schema-version byte:

```rust
const SAMPLING_PARAMS_SCHEMA_VERSION: u8 = 1;

fn sample_id(model: &ContentId, prompt: &str, params: &SamplingParams, idx: u64) -> ContentId {
    let mut h = blake3::Hasher::new();
    h.update(&[SAMPLING_PARAMS_SCHEMA_VERSION]);  // FIRST byte
    h.update(&model.0);
    h.update(prompt.as_bytes());
    h.update(&postcard::to_stdvec(params).unwrap());
    h.update(&idx.to_le_bytes());
    ContentId(*h.finalize().as_bytes())
}
```

When you add a field to `SamplingParams`, bump `SAMPLING_PARAMS_SCHEMA_VERSION`.
That invalidates outstanding IDs *by design* — drain the in-flight batch under
the old version before deploying the new schema, or re-run any orphaned
samples.

Tests:

- `content_id_derivation.rs::sample_id_matches_locked_hex_for_default_params` —
  locks the hex digest for the default-params case so any silent change to
  hasher input order trips immediately.
- `content_id_derivation.rs::schema_version_byte_is_first` — explicit
  regression catch for Pitfall 1.
- Six proptest property tests prove that *every* component of the input
  (`prompt`, `idx`, `model`, `temperature`, `max_tokens`, `seed`) participates
  in the hash.

## CAS state machine

`SampleRecord { id, prompt_blob, state, created_at_ms, input_idx }` lives at
`infer/<run_id>/samples/<sample_id_hex>` (storage namespace `infer`,
table `T_INFER`).

```
              try_claim                      try_complete
   Pending  ──────────────▶ Running ────────────────────▶ Done
     ▲                       │                            ▲
     │                       │                            │ (terminal)
     │ try_repending         │ try_fail
     │  (stale only)         ▼
     └──────────────────── Failed
                            (transient errors;
                             coordinator re-enqueues)
```

Helpers (in `src/state.rs`):

- `try_claim(txn, record, run_id, worker_id, now_ms, stale_after_ms) → Result<bool>`
  CAS-swaps `Pending` (or stale `Running`) → `Running`. Returns `false` on
  race-loss.
- `try_complete(txn, running_record, run_id, completion_blob, finished_at_ms)`
- `try_fail(txn, running_record, run_id, reason, failed_at_ms)`
- `try_repending(txn, running_record, run_id)` — for the resume path.

All helpers take a `&mut Box<dyn StorageTxn>` so the caller controls the
commit/abort lifecycle.

## Resume semantics

`BatchCoordinator::scan_and_enqueue(inputs, model_content_id, sampling)` is
idempotent. For each input it derives `sample_id(model, prompt, sampling, idx)`,
looks up the existing record at `sample_key(run_id, sid)`, and:

| Current state          | Action                                            |
|------------------------|---------------------------------------------------|
| (absent)               | write `Pending` + prompt blob + enqueue           |
| `Pending`              | enqueue (worker will claim via CAS)               |
| `Failed`               | enqueue (retry)                                   |
| `Running` (fresh)      | skip — live owner                                 |
| `Running` (stale)      | CAS `Running` → `Pending`; enqueue on success     |
| `Done`                 | skip — terminal                                   |

"Stale" defaults to **5 minutes** (`DEFAULT_STALE_AFTER_MS = 5 * 60_000`).
Override per-coordinator via `.with_stale_after_ms(ms)`. The 5-minute window
follows RESEARCH §"Pitfall 5" — short enough to recover from a `SIGKILL`'d
worker, long enough to absorb a slow generation pass.

The stale-Running re-`Pending` step is itself a CAS so two coordinators racing
on the same orphaned sample don't double-enqueue. The test
`resume_skips_done.rs::scan_enqueues_only_non_terminal_samples` covers all four
transition cases in a single integration run.

## `BatchWorker` flow

```rust
pub async fn run_loop(&self) -> Result<usize, CoreError> {
    loop {
        match self.run_one().await? {
            RunOutcome::Drained => return Ok(completed),
            RunOutcome::Completed => completed += 1,
            RunOutcome::Failed | RunOutcome::Skipped => { /* keep going */ }
        }
    }
}
```

`run_one()` dequeues a sample-id, loads the `SampleRecord`, CAS-claims it,
fetches the prompt blob, calls `backend.generate(&[Prompt], &sampling)`, writes
the completion to the object store, and CAS-transitions `Running → Done`.
Backend errors translate to `Running → Failed` via `try_fail`; the queue item
is `ack`'d either way (the coordinator re-enqueues `Failed` rows on the next
`scan_and_enqueue`).

Concurrency: each `BatchWorker` is one Tokio task; spawn `N` of them sharing
the same `Arc<dyn Queue>` / `Arc<dyn Storage>` / `Arc<dyn ObjectStore>` and
the CAS dance guarantees at-most-once completion per sample.

## `MockBackend`

Gated by the `test-mock-backend` Cargo feature. Returns
`Completion { text: format!("MOCK:{}", p.0), finish_reason: "stop", … }` after
an optional `tokio::time::sleep`. Used by `worker_happy_path.rs` and (in
Wave-4) by the restart-no-duplicates integration test to exercise the full
pull-loop without a Python / vLLM dependency.

The contract: `MockBackend` impls the same `InferenceBackend` trait that
`VllmBackend` does, so it composes against `BatchWorker` identically. CLI code
in Wave 3 (plan 03-04) wires `VllmBackend`; nothing in `BatchWorker` /
`BatchCoordinator` knows the difference.

## `InferBatchConfig`

The TOML schema for `rollout infer batch --config <path>` lives here in
`src/config.rs` (NOT in `rollout-cli`, per WARN-5). `rollout-cli` imports it
via `use rollout_runtime_batch::config::InferBatchConfig;`.

```toml
[model]
uri = "Qwen/Qwen2.5-0.5B-Instruct"

[sampling]
temperature = 0.7
top_p       = 0.9
max_tokens  = 64
seed        = 42

[input]
glob = "data/prompts/*.jsonl"

[output]
dir = "data/completions"

[workers]
count = 1
```

`#[serde(deny_unknown_fields)]` on every block per spec 11 — typos in the
config file fail at plan-time, not minute 47 of a run.

## JSONL I/O

`read_jsonl` + `write_jsonl` use stdlib `tokio::io::BufReader::lines()` +
`serde_json::from_str` — no `serde_jsonlines` dep. Arbitrary extra fields on
the input row are preserved verbatim via `#[serde(flatten)]` and round-trip
to the output row. `write_jsonl` writes one JSON object per line; the CLI
sorts on `SampleRecord.input_idx` before calling `write_jsonl` so output order
matches input order regardless of which worker finished which sample first.

## Phase 4 — `TrainableBackend` impl on `MockBackend`

Beyond the Phase-3 `InferenceBackend` impl, `MockBackend` ships a
`TrainableBackend` impl gated behind the same `test-mock-backend` feature.
The training extension drives plan 04-02's LOAD-BEARING `snapshot_resume.rs`
byte-compare test (TRAIN-03) — no Python, no GPU, runs on every CI build.

- `MockBackend::new_train(seed)` initialises an `ndarray::Array1<f32>` of length 8
  with every element set to `(seed as f32) / 1000.0`.
- `forward_with_loss` returns `loss = 0.5` and a `GradHandle { step: prev + 1 }`.
- `optimizer_step` applies a deterministic SGD delta
  `(seed + grad_handle.step) * lr` to every weight element.
- `save_weights` returns `ContentId::of(postcard::to_stdvec(&weights))`.
- `load_weights` is a no-op; `new_train_with_weights(seed, weights)` is the
  test-side restore hook so the byte-compare assertion in `snapshot_resume.rs`
  is meaningful.

Determinism contract: two `MockBackend`s constructed with the same seed produce
byte-equal weights after K identical `optimizer_step` calls. Plan 04-02's test
proves bit-identical resume by running 10 uninterrupted steps versus 5-snapshot-5
and asserting the two final weight vectors are equal.

## See also

- RESEARCH §"Pitfall 1" — schema-version byte rationale.
- RESEARCH §"Pitfall 5" — stale-Running re-Pending CAS dance.
- `docs/specs/02-algorithms.md` §2a — `InferenceBackend` extension shape.
- `docs/specs/04-storage-snapshots.md` §2 — `Storage` + `StorageTxn` + CAS.
- `docs/specs/06-cloud-layer.md` §3 — `Queue` + `ObjectStore`.
- Plan 04-02 — SFT skeleton + the snapshot-resume byte-compare proof.
