# `rollout infer batch` CLI

The first user-visible building block of rollout: a resumable batch-inference
pipeline driven by a single TOML config. Bridges the Phase-3 substrate
(`rollout-backend-vllm` + `rollout-runtime-batch`) behind a clap subcommand on
`rollout-cli`.

## Invocation

```bash
rollout infer batch \
    --config examples/batch-tiny.toml \
    [--resume <run_id>] \
    [--workers N] \
    [--dry-run]
```

| Flag        | Default          | Purpose                                                                            |
| ----------- | ---------------- | ---------------------------------------------------------------------------------- |
| `--config`  | required         | Path to the TOML config (schema below).                                            |
| `--resume`  | implicit from output dir | Override the `<output.dir>/run-id` lookup with an explicit ULID.           |
| `--workers` | `[workers].count` | Override the worker pool size for this invocation.                                |
| `--dry-run` | `false`          | Validate config + probe inputs + check `HF_TOKEN` allowlist; never call the backend. |

## TOML schema

The schema lives in `rollout-runtime-batch::config::InferBatchConfig` (the CLI
imports it; spec 11 single-source-of-truth). Every block is
`#[serde(deny_unknown_fields)]` — typos are caught at load time.

```toml
[model]
uri       = "Qwen/Qwen2.5-0.5B-Instruct"   # HF repo, local path, or object-store URI
tokenizer = "..."                          # optional override

[sampling]
temperature = 0.7
top_p       = 0.9
top_k       = -1     # -1 disables
max_tokens  = 64
seed        = 42     # optional; deterministic when set
stop        = []
stream      = false  # MUST be false in Phase 3 (D-BACKEND-03)

[input]
glob = "data/prompts/*.jsonl"

[output]
dir = "data/completions"

[workers]
count = 1            # >= 1
```

## JSONL input contract

One JSON object per line. Required: `prompt`. Optional: `id` (defaults to the
deterministic sample-id from `blake3(model || prompt || params)` per
`rollout-runtime-batch::sample_id`). Extra fields are preserved and
round-tripped to output.

```json
{ "prompt": "Translate to French: Hello.", "id": "p-001", "tag": "demo" }
```

## JSONL output contract (D-CLI-03)

Each row:

```json
{
  "id":                "<content-addressed sample id>",
  "prompt":            "<original prompt text>",
  "completion":        "<generated text>",
  "sampling_params":   { ... },
  "model_uri":         "Qwen/Qwen2.5-0.5B-Instruct",
  "finish_reason":     "stop",
  "model_content_id":  "<blake3-hex of resolved model SHA>",
  "completion_blob_id":"<blake3-hex of completion bytes in the object-store>",
  "generated_at":      "2026-05-20T22:31:09.123Z"
}
```

Order matches input file order regardless of worker concurrency — the CLI
calls `BatchCoordinator::collect_done_records()` (sorted by `input_idx`) and
emits in that order.

## `run_id` lifecycle (BLOCKER 6)

Three-tier resolution:

| Source                                     | When applied                                             |
| ------------------------------------------ | -------------------------------------------------------- |
| `--resume <ULID>`                          | Always honored if present.                               |
| `<output.dir>/run-id` (file)               | Re-attach if the file exists and `--resume` is absent.   |
| Freshly minted ULID                        | First run; written atomically (tmp + rename).            |

The file is single-line UTF-8 (ULID Crockford form). Plan 03-05's
`restart_no_duplicates` test reads this file between phases to obtain the ULID
for the explicit `--resume` flag.

## `--dry-run` semantics

Performs (in order):

1. Parse TOML against `InferBatchConfig` (`deny_unknown_fields`).
2. Validate `sampling.stream == false`, `sampling.max_tokens > 0`,
   `workers.count >= 1`, `input.glob` non-empty.
3. Resolve `input.glob` and read every JSONL file end-to-end.
4. Probe whether the model URI is on a known-gated prefix
   (`meta-llama/*`, `mistralai/*`) and look up `ROLLOUT_SECRET_HF_TOKEN` (best-effort).
5. Print `dry-run OK: model=… inputs=… workers=…` and exit `0`.

Crucially, **the backend is never constructed** — `--dry-run` works on a build
with neither `--features vllm` nor `--features test-mock-backend`.

## Backend selection

The CLI picks one backend at runtime based on Cargo features + env:

| Order | Condition                                                                    | Backend                                  |
| ----- | ---------------------------------------------------------------------------- | ---------------------------------------- |
| 1     | `--features test-mock-backend` + `ROLLOUT_TEST_MOCK_BACKEND=1`               | `rollout_runtime_batch::MockBackend`     |
| 2     | `--features vllm`                                                            | `rollout_backend_vllm::VllmBackend`      |
| 3     | none                                                                         | fast-fail with `Fatal(ConfigInvalid)` at full-run; dry-run still works. |

## Observability

`RUST_LOG=info rollout infer batch …` produces structured `tracing` events.
Key fields: `run_id`, `enqueued`, `total`, `completed`. Worker spans carry
`worker = <ulid>`.

## Exit codes

| Code | Meaning                                                |
| ---- | ------------------------------------------------------ |
| 0    | Success (or successful dry-run).                       |
| 2    | Config-invalid, infrastructure error, or backend error. |

## CPU-mode caveat

vLLM CPU inference on the test model (Qwen2.5-0.5B-Instruct) is dramatically
slower than CUDA: expect ~1–5 tokens/sec. On macOS Apple-Silicon, vLLM has no
PyPI wheels (RESEARCH Pitfall 3) — run via Docker. CI's `infer-smoke` job is
Linux-only and gated on `ROLLOUT_VLLM_AVAILABLE=1`.
