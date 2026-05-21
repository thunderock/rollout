# CPU mode

vLLM ships with first-class CUDA support, but for AGENTS.md §7 ("every plugin testable locally without GPU") rollout must also work on a plain CPU. This chapter documents the CPU-mode contract for `rollout-backend-vllm`, the dev-loop reality of macOS Apple-Silicon, and the smoke-test posture that lets default CI stay green without any GPU.

## Where CPU mode is selected

Per Phase 3 CONTEXT decision D-VLLM-04 (as overridden by RESEARCH §"Pitfall 9"), the Python-side glue in `python/rollout/backends/vllm/engine.py` performs an **explicit `torch.cuda.is_available()` probe** and passes `device="cuda"` or `device="cpu"` to `AsyncEngineArgs` — never `device="auto"`. The auto-detect path was rejected because vLLM silently falls back to CPU when CUDA libraries are partially installed (driver present, runtime missing, etc.), and a silent fallback would mask configuration mistakes at runtime instead of failing at plan time.

```python
import torch
device = "cuda" if torch.cuda.is_available() else "cpu"
engine_args = AsyncEngineArgs(
    model=model_uri,
    device=device,           # explicit; not "auto"
    disable_log_stats=True,
    disable_log_requests=True,
)
```

`rollout-cloud-local::ComputeHint::inventory()` still informs observability events (`gpu_inventory_collected`) and worker-config decisions, but it is no longer the source-of-truth for the engine `device` kwarg.

## Expected CPU throughput

vLLM's CPU backend is functional but not fast. Approximate single-stream throughput for the canonical Phase-3 test model (`Qwen/Qwen2.5-0.5B-Instruct`, 16 max-tokens):

| Host | tokens/sec |
|---|---|
| Apple M1 Pro (8-core perf) | ~3–6 |
| Linux x86_64 (16-core) | ~2–4 |
| Generic CI runner (4-core) | ~1–2 |

A 4-prompt × 16-token smoke run finishes in well under 60 s on any of these. Anything longer than the canonical `examples/batch-tiny.toml` shape should target a CUDA host.

## macOS Apple-Silicon

**vLLM has no Apple-Silicon wheel as of Phase 3.** `pip install vllm` on macOS produces `ERROR: No matching distribution found for vllm`. The two paths forward:

1. **Build-from-source (slow, brittle).** `VLLM_TARGET_DEVICE=cpu pip install -e .` against a freshly cloned vLLM repo. Compilation takes 10–30 min and depends on Apple-clang versions in ways that drift between vLLM releases. Documented but not recommended for routine dev work.
2. **Docker (recommended).** See `dev-on-macos.md`. A `linux/amd64` (or `linux/arm64` if available) container with `vllm>=0.10` pre-installed lets the rollout binaries run identically to CI.

Either way, the Rust-side test surface (~80 % of Phase 3's automated tests — `SamplingParams` postcard determinism, `sample_id` derivation, CAS state-machine transitions, JSONL round-trip, the `MockBackend`-driven `restart_no_duplicates` test) runs natively on macOS without any vLLM installed. Only the live-engine integration tests (`vllm_init.rs`, `vllm_generate.rs`) and the `make infer-smoke` script require a real vLLM.

## CI posture

- **Default CI (public runners):** `ROLLOUT_VLLM_AVAILABLE` unset. The `infer-smoke` workflow job is gated on `vars.ROLLOUT_VLLM_AVAILABLE == '1'` and therefore does not fire. The `cargo test --workspace --tests` job still runs `restart_no_duplicates` (gated on `--features test-mock-backend`); the load-bearing exit criterion (b) proof is exercised on every PR.
- **Self-hosted runner with vLLM installed:** set `vars.ROLLOUT_VLLM_AVAILABLE = '1'` in repo settings. The `infer-smoke` job downloads `Qwen2.5-0.5B-Instruct` on first run (~1 GiB; cached under `~/.cache/huggingface/hub/`), runs `rollout infer batch --config examples/batch-tiny.toml`, and asserts the produced `data/completions/batch-tiny/completions.jsonl` has 4 non-empty completion rows.
- **Local dev:** `ROLLOUT_VLLM_AVAILABLE=1 make infer-smoke` after `pip install 'vllm>=0.10,<0.22'`. On Apple-Silicon, prefer the Docker path documented in `dev-on-macos.md`.

## Failure modes

| Failure | Surface | Diagnosis |
|---|---|---|
| `import torch` fails | Python ImportError at engine init | Active venv missing torch — `pip install torch` first |
| `import vllm` fails | Python ImportError at engine init | Active venv missing vllm — `pip install 'vllm>=0.10,<0.22'` |
| `torch.cuda.is_available() == False` on a GPU host | engine boots in CPU mode silently | NVIDIA driver/runtime mismatch — install matching CUDA runtime; the explicit probe surfaces this rather than masking it |
| `vllm` import succeeds but `AsyncLLMEngine.from_engine_args` panics with `device="cpu"` not supported | vLLM version too old | upgrade to `vllm>=0.10` |
| `make infer-smoke` times out (>300 s) on a CPU host | model larger than `Qwen2.5-0.5B-Instruct` | use the canonical `examples/batch-tiny.toml` model; do not run multi-billion-param models on CPU |
