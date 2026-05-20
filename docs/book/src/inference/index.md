# Inference

Phase 3 delivers `rollout`'s first end-user-visible building block: a batch-inference
pipeline that loads a model into [vLLM](https://docs.vllm.ai/) via PyO3, pulls
content-addressed prompts off the Phase-2 in-memory queue, and writes JSONL
completions to disk. The pipeline is **resumable** — `rollout infer batch
--resume <run_id>` scans the per-sample state in `rollout-storage` and only
enqueues outstanding work, producing zero duplicates on restart.

Phase 3 ships two new crates plus a CLI subcommand:

| Crate                    | Layer | Phase-3 responsibility |
|--------------------------|:-----:|------------------------|
| `rollout-backend-vllm`   |   2   | `InferenceBackend` impl over vLLM's `AsyncLLMEngine` via PyO3 (in-process). |
| `rollout-runtime-batch`  |   3   | CAS sample-state machine, JSONL I/O, plan-time validation, mock backend. |
| `rollout-cli` (extended) |   4   | `rollout infer batch --config <toml> [--resume <run_id>]` subcommand. |

Per-component chapters land in plan 03-05 (smoke + docs + bench). For now, see
the Wave-0 trait extension in [spec 02 §2a](../../../specs/02-algorithms.md) and
the `WorkerRole` addition in [spec 01 §3a](../../../specs/01-core-runtime.md).

> **TODO (plan 03-05):** vLLM backend chapter · batch-runtime chapter · CPU-mode
> chapter · macOS Docker dev workflow · resume semantics walkthrough.
