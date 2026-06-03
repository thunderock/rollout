"""Real ``vllm.AsyncLLMEngine`` bridge for ``rollout-backend-vllm`` (plan 03-03).

Imported by the dedicated Python OS thread the Rust crate spawns. Phase 3 is
inference-only (D-BACKEND-01); training-mode forward/backward is Phase 4.

Contract:
- ``init(model_uri, **engine_args) -> str`` returns the resolved model SHA
  (the blake3 input for ``VllmBackend::model_id``).
- ``generate_one(prompt, request_id, **sampling) -> dict`` is ``async``;
  returns ``{text, finish_reason, prompt_tokens, completion_tokens}``.
- ``shutdown()`` is idempotent.

Pitfall 9 (RESEARCH): a ``torch.cuda.is_available()`` probe gates
``gpu_memory_utilization``; device itself is left to vLLM's platform
auto-detection (the ``device`` kwarg was removed from ``AsyncEngineArgs``).
Pitfall 10: ``HF_TOKEN`` is written into ``os.environ`` BEFORE this module is
imported (handled on the Rust side; see ``engine.rs::worker_main_vllm``).
"""

from __future__ import annotations

import logging

logging.getLogger("vllm").setLevel(logging.WARNING)

# vLLM >= 0.10 exposes AsyncLLMEngine via the top-level package; older paths
# fall through to the v1 module. Both surface ``from_engine_args``.
try:
    from vllm import AsyncEngineArgs, AsyncLLMEngine
    from vllm import SamplingParams as VllmSamplingParams
except ImportError:  # vLLM >= 0.22 may drop the top-level alias.
    from vllm.engine.async_llm_engine import (  # type: ignore[no-redef]
        AsyncEngineArgs,
        AsyncLLMEngine,
    )
    from vllm import SamplingParams as VllmSamplingParams  # type: ignore[no-redef]

import torch  # transitively pulled by vllm; explicit import for the device probe

_engine: AsyncLLMEngine | None = None
_model_sha: str | None = None


def init(model_uri: str, **engine_args: object) -> str:
    """Bring up ``AsyncLLMEngine`` and return the resolved model SHA."""
    global _engine, _model_sha
    device = "cuda" if torch.cuda.is_available() else "cpu"  # Pitfall 9
    gpu_memory_utilization = (
        engine_args.get("gpu_memory_utilization", 0.85) if device == "cuda" else None
    )
    tokenizer = engine_args.get("tokenizer")
    if device == "cuda":
        args = AsyncEngineArgs(
            model=model_uri,
            disable_log_stats=True,
            disable_log_requests=True,
            gpu_memory_utilization=gpu_memory_utilization,
            tokenizer=tokenizer if isinstance(tokenizer, str) and tokenizer else None,
        )
    else:
        args = AsyncEngineArgs(
            model=model_uri,
            disable_log_stats=True,
            disable_log_requests=True,
            tokenizer=tokenizer if isinstance(tokenizer, str) and tokenizer else None,
        )
    _engine = AsyncLLMEngine.from_engine_args(args)
    # Resolve the HF repo SHA for content-addressed model_id; fall back to the
    # URI when the model is local or the API call fails (offline dev hosts).
    try:
        from huggingface_hub import HfApi

        _model_sha = HfApi().model_info(model_uri).sha or model_uri
    except Exception:
        _model_sha = model_uri
    return _model_sha


async def generate_one(
    prompt: str, request_id: str, **sampling: object
) -> dict[str, object]:
    """Run one request to completion; return only the final ``RequestOutput``."""
    assert _engine is not None, "init() not called"
    sp_kwargs: dict[str, object] = {
        "temperature": sampling["temperature"],
        "top_p": sampling["top_p"],
        "top_k": sampling["top_k"],
        "max_tokens": sampling["max_tokens"],
    }
    seed = sampling.get("seed")
    if seed is not None:
        sp_kwargs["seed"] = seed
    stop = sampling.get("stop")
    if stop:
        sp_kwargs["stop"] = stop
    sp = VllmSamplingParams(**sp_kwargs)  # type: ignore[arg-type]
    final_out = None
    async for out in _engine.generate(prompt, sp, request_id):
        final_out = out  # Phase-3 non-streaming: keep only the latest.
    assert final_out is not None
    return {
        "text": final_out.outputs[0].text,
        "finish_reason": final_out.outputs[0].finish_reason or "stop",
        "prompt_tokens": len(final_out.prompt_token_ids or []),
        "completion_tokens": len(final_out.outputs[0].token_ids or []),
    }


def shutdown() -> None:
    """Drop the engine handle. ``AsyncLLMEngine`` has no explicit shutdown."""
    global _engine, _model_sha
    if _engine is not None:
        del _engine
        _engine = None
    _model_sha = None


def model_sha() -> str | None:
    """Last ``init()``'s resolved SHA (or URI fallback). ``None`` pre-init."""
    return _model_sha
