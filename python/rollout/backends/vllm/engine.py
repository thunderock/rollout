"""Wave-2 stub for ``rollout-backend-vllm``.

Imported by the dedicated Python thread (``rollout-py-vllm-<engine_id>``) the
Rust crate spawns via PyO3. Plan 03-03 replaces this with the real
``AsyncLLMEngine.from_engine_args`` + ``async for out in engine.generate(...)``
bridge; the Wave-2 contract is:

- ``init(model_uri, **engine_args)`` stores the model URI on the module.
- ``generate_one(prompt, request_id, **sampling)`` is ``async`` and returns a
  deterministic stub dict; never raises.
- ``shutdown()`` is idempotent.

The shape is intentionally identical to the Wave-3 surface so the Rust side's
``crate::python_glue::samplingparams_to_pydict`` call layout doesn't change.
"""

_engine: dict | None = None


def init(model_uri: str, **engine_args: object) -> None:
    """Bring the (stub) engine up. Wave-3 wires AsyncLLMEngine.from_engine_args here."""
    global _engine
    _engine = {"model": model_uri, "engine_args": dict(engine_args)}


async def generate_one(prompt: str, request_id: str, **sampling: object) -> dict:
    """Return a deterministic stub completion. Wave-3 wires the real engine.generate loop."""
    assert _engine is not None, "init() not called"
    _ = request_id
    _ = sampling
    return {
        "text": f"STUB:{prompt}",
        "finish_reason": "stop",
        "prompt_tokens": 0,
        "completion_tokens": 0,
    }


def shutdown() -> None:
    """Drop the (stub) engine handle. Wave-3 deletes the AsyncLLMEngine instance here."""
    global _engine
    _engine = None
