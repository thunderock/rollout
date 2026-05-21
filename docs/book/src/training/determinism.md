# Determinism

Phase 4 of rollout commits to **deterministic training-state snapshots**
(D-DETERM-01): a `Snapshot.parts[0].content` is the same `ContentId` whenever
the same code drives the same model with the same RNG seed and the same input
stream. This page documents the contract, the platform caveats, and every
mitigation that lands in `rollout-backend-vllm --features train`.

## The determinism stack

| Layer                 | Mitigation                                                       | Where               |
| --------------------- | ---------------------------------------------------------------- | ------------------- |
| Process env           | `CUBLAS_WORKSPACE_CONFIG=:4096:8`, `PYTHONHASHSEED=0`             | `train.py` preamble |
| RNG seeds             | `random` + `numpy` + `torch` + `torch.cuda.manual_seed_all`       | `_set_determinism_flags` |
| PyTorch deterministic | `torch.use_deterministic_algorithms(True)`                        | `_set_determinism_flags` |
| cuDNN                 | `cudnn.deterministic = True`, **`cudnn.benchmark = False`**       | `_set_determinism_flags` |
| Matmul precision      | `torch.set_float32_matmul_precision("highest")`                   | `_set_determinism_flags` |
| Chat template         | `{% generation %}` markers via `GENERATION_MARKED_QWEN25_TEMPLATE` | `qwen25_chat_template.py` |
| Dataloader            | `torchdata.StatefulDataLoader` if available; step-replay fallback | `init_train` |
| Accelerator           | `accelerator.prepare(model, optimizer, scheduler)`                | `init_train` |
| LR scheduler          | `register_for_checkpointing` (Pitfall 10 fallback)                | `init_train` |
| Tar                   | byte-stable mode bits + sorted walkdir + mtime=0 (Pitfall 9)      | `rollout-snapshots::build_deterministic_tar` |

## Why preamble ordering matters (Pitfall 2)

`CUBLAS_WORKSPACE_CONFIG` and `PYTHONHASHSEED` are read by `torch` **on first
import**. If `import torch` runs before they are set, the values are baked in
and later `os.environ` mutations do not change behavior. The Rust side
enforces ordering by writing the env vars on the worker thread **before**
calling `py.import("rollout.backends.vllm.train")` — see
`rollout-backend-vllm/src/train.rs::import_train_module`.

`train.py` repeats `os.environ.setdefault(...)` at the top of the module
defensively: if someone imports it directly from a Python REPL with `torch`
already loaded, the user gets non-determinism, but the source still documents
the contract.

## cuDNN.benchmark is the silent killer (Pitfall 8)

cuDNN auto-tunes kernel selection by running candidate kernels and picking the
fastest. This is non-deterministic — re-running the same workload picks a
different kernel under load. **`cudnn.benchmark = False` is required**, even
when `cudnn.deterministic = True`. The Phase-4 implementation sets both
explicitly in `_set_determinism_flags`.

## LR scheduler must be in the save-state capture path (Pitfall 10)

`accelerator.save_state` captures the model, optimizer, RNG, and any object
explicitly registered for checkpointing. The Phase-4 path prefers passing the
scheduler through `accelerator.prepare(model, optimizer, scheduler)`; if that
fails (e.g., the scheduler shape isn't supported by accelerate's prepare),
the code falls back to `accelerator.register_for_checkpointing(scheduler)`.
Without one of these, the LR resumes from `lr_start` after a snapshot restore
instead of continuing the schedule — silent non-determinism.

## CPU vs CUDA contract

| Hardware                          | Bit-identical?                          |
| --------------------------------- | --------------------------------------- |
| Same CPU model, same OS, same env | **Yes** (the load-bearing CI target)    |
| Same SM (sm_80 vs sm_80), same cuDNN | Yes, with `deterministic + !benchmark` |
| Different SM (sm_80 vs sm_90)     | No — kernels differ                     |
| Different cuDNN minor version     | No — algorithm shortlist may differ     |
| Cross-machine (CPU A vs CPU B)    | Best-effort; FP rounding differs        |

The `snapshot_resume_live.rs` live witness exercises the **same-CPU** case on
the dev box. CI runs the `MockBackend` variant from plan 04-02
(`snapshot_resume.rs::bit_identical_resume_at_step_5`) for the unconditional
green signal.

## accelerate.save_state captures

| Object        | Source                                | Restored on `load_state`? |
| ------------- | ------------------------------------- | ------------------------- |
| Model weights | `accelerator.prepare(model)`           | Yes                       |
| Optimizer     | `accelerator.prepare(optimizer)`       | Yes                       |
| RNG (torch)   | implicit via `torch.random.get_rng_state` | Yes                  |
| RNG (cuda)    | implicit if CUDA is initialized       | Yes                       |
| LR scheduler  | `prepare(scheduler)` OR `register_for_checkpointing` | Yes        |
| Dataloader    | `DataLoaderConfiguration(use_stateful_dataloader=True)` | If torchdata installed |

The Phase-4 contract: anything that affects subsequent step output must be in
this list. If a future plan adds custom state (e.g., reward-model normalizer
running stats), it MUST register via `register_for_checkpointing` to keep the
TRAIN-03 byte-compare proof intact.

## torchdata stateful dataloader (Pitfall 3)

`init_train` probes for `torchdata`. If present, it constructs a
`DataLoaderConfiguration(use_stateful_dataloader=True)` so the dataloader's
position is checkpointed by `save_state`. Without `torchdata`, the runtime
falls back to step-replay: the resume side re-reads from the JSONL head and
skips `step` rows. Both modes preserve the TRAIN-03 byte-compare invariant on
deterministic input streams.

## Pitfall 7 — Accelerator singleton

`Accelerator()` is a singleton per Python process; constructing two of them
in the same interpreter raises. `init_train` is idempotent — the second call
returns the cached `_STATE` dict. `teardown_train` flushes the singleton via
`del` + `gc.collect()` + `torch.cuda.empty_cache()` so the next `init_train`
call (in a subsequent run, or after a mid-process swap-back to vLLM
inference) can construct a fresh accelerator.

A bidirectional mid-process swap (training ↔ inference under the same OS
thread) is **Phase 9** — see the deferral note in
`rollout-backend-vllm/src/train.rs::run_set_train_mode`.

## MockBackend vs live HF path

| Backend       | CPU bit-identical?       | When to use                 |
| ------------- | ------------------------ | --------------------------- |
| `MockBackend` | Yes (Array1<f32> SGD)    | CI; every PR; algo-side tests |
| Live HF       | Yes on identical CPU; same-SM only on CUDA | Dev-box live witness; nightly |

The `MockBackend` path is the **load-bearing CI proof** for TRAIN-03. The
live HF path is the gated witness behind `ROLLOUT_TRANSFORMERS_AVAILABLE=1`.

## Where the code lives

- `python/rollout/backends/vllm/train.py` — determinism preamble + Accelerator construction.
- `python/rollout/backends/vllm/qwen25_chat_template.py` — generation-marked chat template.
- `crates/rollout-backend-vllm/src/train.rs` — env-write-before-import enforcer; `py.detach` wrappers.
- `crates/rollout-snapshots/src/tar_build.rs` — Pitfall 9 deterministic tar.

## Related

- [Snapshots](./snapshots.md) — Phase-4 snapshot pipeline.
- [SFT](./sft.md) — SFT algorithm + TRAIN-03 byte-compare proof.
- [CPU mode](./cpu-mode.md) — running Phase-4 training on CPU only.
