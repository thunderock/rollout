# Phase 3: Inference backend (vLLM) + batch inference — Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in `03-CONTEXT.md` — this log records that `--auto` mode picked recommended defaults.

**Date:** 2026-05-20
**Phase:** 03-inference-batch
**Mode:** `/gsd:discuss-phase 3 --auto` — fully autonomous; Claude auto-selected ALL gray areas and chose the recommended option for every question.

---

## Auto-Selected Gray Areas

1. vLLM integration path + Python packaging
2. `InferenceBackend` trait extension scope (Wave 0)
3. Resumable batch design (content-addressed IDs, queue, worker roles)
4. `rollout infer batch` CLI surface + test model + benchmark

---

## Auto-Picks (Area 1 — vLLM integration path + Python packaging)

| Question | Selected (recommended) | Alternative not chosen |
|---|---|---|
| PyO3 in-process vs sidecar? | **PyO3 in-process via 02-05's dedicated Python OS thread pattern** | Sidecar (gRPC over UDS) — extra IPC cost, less aligned with ROADMAP's "second user of PyO3 path" callout |
| Sync `LLM` vs `AsyncLLMEngine`? | **`AsyncLLMEngine`** | Sync `LLM` — would force Rust-side batching, breaks principle #1 async-native |
| vLLM packaging? | **Optional `vllm` Cargo feature (default OFF); CI tests `#[ignore]`d unless `ROLLOUT_VLLM_AVAILABLE=1`; smoke test uses CPU + tiny model** | Hard dep — breaks AGENTS.md §7 (every plugin testable without GPU) |
| CUDA detection? | **Runtime auto-detect via `nvml-wrapper` (02-03); vLLM picks CUDA if available, falls back to CPU; CI = CPU mode** | Build-time feature flag — too rigid for cross-platform dev |

## Auto-Picks (Area 2 — `InferenceBackend` trait extension scope)

| Question | Selected (recommended) | Alternative not chosen |
|---|---|---|
| Phase 3 extension scope? | **Inference-only: extend with `SamplingParams`, batched `generate`, content-addressed ID derivation. Defer training-mode methods to Phase 4** | Land full inference + training surface now — premature; Phase 4 may want a sibling `TrainableBackend` trait instead |
| `SamplingParams` shape? | **`{ temperature, top_p, top_k, max_tokens, seed, stop, stream=false }` matching vLLM 1:1** | Minimal `{ temperature, max_tokens }` — wouldn't cover real use cases |
| Streaming in Phase 3? | **No — batch only** | Yes — streaming is Phase 8 (INFER-01); out of scope |
| Tokenizer ownership? | **Backend owns the tokenizer; algorithms never see token IDs in Phase 3** | First-class tokenizer trait — defer to Phase 4 if SFT/RM need it |

## Auto-Picks (Area 3 — Resumable batch design)

| Question | Selected (recommended) | Alternative not chosen |
|---|---|---|
| Sample-ID derivation? | **`ContentId = blake3(model_uri \|\| prompt_bytes \|\| canonical(sampling_params) \|\| sample_index)`** | Hash of prompt only — would collide on duplicate prompts in the same batch |
| Persistence strategy? | **Storage namespace `infer/<run_id>/samples` with postcard SampleRecord; CAS transitions Pending → Running → Done/Failed; scan on restart** | Hold state in RAM only — would lose resume-zero-duplicates guarantee |
| Worker role split? | **Single `WorkerRole::BatchInference` in Phase 3 (read+infer+write); `BatchReader`/`BatchWriter` variants enumerated for Phase 6** | True 3-role split now — premature; only matters with multi-node |
| Queue? | **Reuse `rollout-cloud-local::InMemQueue` (02-03 RAM + Storage spill); coord enqueues sample-IDs at plan time; workers pull via Phase-2 Work channel** | New per-phase queue impl — wasted work, breaks reuse |

## Auto-Picks (Area 4 — CLI + test model + benchmark)

| Question | Selected (recommended) | Alternative not chosen |
|---|---|---|
| `infer batch` CLI surface? | **`rollout infer batch --config <path> [--resume <run_id>]`; TOML with [model], [sampling], [input], [output], [workers]** | Flag-based (no config file) — wouldn't scale, breaks spec 08 §3 |
| Input/output format? | **JSONL `{id?, prompt}` in; `{id, prompt, completion, sampling_params, model_uri, finish_reason, ...}` out** | CSV — won't handle multi-line prompts or completions cleanly |
| Test model? | **`Qwen/Qwen2.5-0.5B-Instruct` (Apache-2.0, ~1 GB, CPU-runnable, vLLM-supported)** | Llama-3.2-1B — gated; TinyLlama — older, less coverage |
| Benchmark shape? | **`crates/rollout-backend-vllm/benches/throughput.rs` (criterion); 64 prompts × 64 tokens; CI behind `runs-on: [self-hosted, gpu]` label** | Inline test — too noisy, hard to compare against baseline |

---

## Notes for User Review

This `--auto` run skipped interactive Q&A; every decision is a Claude pick. If any choice feels wrong, edit `03-CONTEXT.md` directly before plan-phase consumes it. Most consequential picks worth a second look:

- **PyO3 in-process for vLLM** — high blast radius. ROADMAP explicitly endorses this path, but sidecar mode is the safer Phase-3 hedge if you want better crash isolation. Trade-off: PyO3 is faster (no IPC), sidecar is more robust.
- **Phase 3 trait stays inference-only** — pushes the training-mode decision to Phase 4. If Phase 4 picks sibling-trait, the Phase-3 InferenceBackend stays clean. If Phase 4 extends in-place, Phase 4 will do that work.
- **CI gates real-vllm tests behind `ROLLOUT_VLLM_AVAILABLE=1`** — public-runner CI will not exercise the actual model load. Nightly CI on a dev box or self-hosted GPU runner does. Acceptable trade-off given vLLM's wheel size, but means "green CI" doesn't equal "Phase 3 works."
- **`Qwen/Qwen2.5-0.5B-Instruct`** as the canonical test model. Apache-2.0, no HF gating needed, fits in CI cache.
- **No streaming + no training-mode methods** — both explicitly deferred. If the user wants either in Phase 3, this needs to flip.

## Claude's Discretion (deferred to research/planner)

- Crate name for the runtime glue (standalone `rollout-runtime-batch` vs module inside `rollout-coordinator`)
- `Prompt` / `Completion` as newtypes vs bare `String`
- vLLM minimum version pin (likely `vllm>=0.6`)
- `disable_log_stats` / `disable_log_requests` defaults (recommend both true in Phase 3)
- `gpu_memory_utilization` default (recommend `0.85` on CUDA hosts)
- Plan-time model existence probe (recommend yes — fail fast)
- `--dry-run` flag (recommend yes)
- Python module shape: pure Python vs maturin (recommend pure Python in `python/rollout/backends/vllm/`)

## Deferred Ideas (captured in CONTEXT.md `<deferred>`)

Streaming generation (Phase 8), tool calling (Phase 8), training-mode forward/backward (Phase 4), multi-node distribution (Phase 6), real cloud object stores (Phase 5), snapshots (Phases 4/9/11), tokenizer trait (Phase 4), reader/writer worker split (Phase 6), speculative decoding tuning (post-Phase 9), additional inference backends like SGLang/TGI/Candle (Phase 8+), `rollout infer eval` (Phase 7), episodic memory (Phase 8).
