# vLLM backend (`rollout-backend-vllm`)

`rollout-backend-vllm` implements the Phase-3 `InferenceBackend` trait
(`init` / `generate` / `model_id` / `shutdown`) over [vLLM's `AsyncLLMEngine`][vllm]
via [PyO3][pyo3] **in-process**. The crate is the second user of the dedicated
Python OS-thread pattern hardened in plan 02-05; the first was
`rollout-plugin-host`. The architecture is documented in spec
[03-plugin-system §3.2][spec-03] and the trait surface in spec
[02-algorithms §2 / §2a][spec-02].

> **Status (plan 03-03):** Wave-3 lives. The PyO3 dedicated thread now imports
> `rollout.backends.vllm.engine`, which wraps `vllm.AsyncLLMEngine`; `generate`
> drives the engine through a fresh asyncio event loop on the worker thread
> via `pyo3_async_runtimes::tokio::run_until_complete`. The default-features
> (no-`vllm`) build keeps the Wave-2 stub worker so
> `cargo test -p rollout-backend-vllm` still runs without Python / vLLM.

## Architecture

```text
+--------------------+ tokio::sync::mpsc::Sender<VllmTask>  +--------------------+
|  Tokio runtime     | -----------------------------------> |  rollout-py-vllm-  |
|  VllmBackend       |                                      |     <engine_id>    |
|  (async impl       | <----------------------------------- |  Python::attach    |
|   InferenceBackend)|       oneshot::Sender<Result<...>>   |  rt.block_on(...)  |
+--------------------+                                      +--------------------+
                                                                       |
                                                                       v
                                                    rollout.backends.vllm.engine
                                                    (Wave 3: AsyncLLMEngine)
```

- The Tokio side never touches Python. Every call hops through one OS thread
  named `rollout-py-vllm-<engine_id>` that owns the interpreter for the
  backend's lifetime.
- The thread imports `rollout.backends.vllm.engine` once at startup, then loops
  on `mpsc::Receiver<VllmTask>` until `VllmTask::Shutdown` arrives or the
  channel closes.
- Each `Generate` task uses a `oneshot::channel` for its reply so multiple
  concurrent prompts can be in flight without head-of-line blocking on the
  channel (Tokio side dispatches them sequentially in Wave 2; Wave 3 will
  span them concurrently to let vLLM's continuous batcher do the work).

## Cargo feature gating

```toml
[features]
default = []       # no PyO3 link; tests use the stub worker
vllm    = []       # imports rollout.backends.vllm.engine on the dedicated thread
```

With `--features vllm` **off** (default), the crate builds without invoking
`pyo3` at runtime. The dispatch path still exists; `Generate` returns the
Wave-2 stub error from a pure-Rust worker. This honors AGENTS.md §7
(every plugin testable locally without GPU) — no Python interpreter, no vLLM
install, no CUDA required for `cargo test -p rollout-backend-vllm`.

With `--features vllm` **on**, the worker thread calls
`Python::attach(|py| py.import("rollout.backends.vllm.engine"))` once at
startup. The Python module top-imports `vllm.AsyncLLMEngine` (plan 03-03).

**vLLM version pin:** `vllm>=0.10,<0.22`. The lower bound is the first
release where `AsyncLLMEngine` is an alias for the new v1 engine
(`vllm.v1.engine.async_llm.AsyncLLM`); the upper bound guards against
future-version drops of the alias. The pin is documented but NOT enforced
in `Cargo.toml` — vLLM is a Python install. The engine module's import
falls back to `from vllm.engine.async_llm_engine import …` if the
top-level alias is removed in a future version.

## Pitfall 10: env-write before import

vLLM imports `huggingface_hub` which lazily reads `os.environ.get("HF_TOKEN")`
when downloading gated models. The dedicated Python thread therefore writes
`HF_TOKEN` into its own `os.environ` **before** the `py.import("rollout…")`
call:

```rust
Python::attach(|py| {
    if let Some(token) = &secret_token {
        let os = py.import("os")?;
        let environ: Bound<'_, PyDict> = os.getattr("environ")?.cast_into()?;
        environ.set_item("HF_TOKEN", token)?;
    }
    py.import("rollout.backends.vllm.engine")?;
    Ok(())
});
```

The token flows through the `VllmEngine::spawn(engine_id, secret_token)`
constructor; the SecretStore consumer is wired in plan 03-03 alongside the
live engine.

## Wave 2 vs Wave 3 split

| Concern                                            | Wave 2 (plan 03-01) | Wave 3 (plan 03-03) |
|----------------------------------------------------|:-------------------:|:-------------------:|
| Cargo crate + `vllm` feature flag                  | shipped             | inherited           |
| PyO3 dedicated thread (`rollout-py-vllm-…`)        | shipped             | inherited           |
| `VllmTask` enum + `mpsc::Sender` dispatch          | shipped             | inherited           |
| `VllmBackend: InferenceBackend` impl shape         | shipped (stub)      | live engine         |
| `python/rollout/backends/vllm/engine.py`           | stub                | real `AsyncLLMEngine` |
| `AsyncLLMEngine.from_engine_args` wiring           | —                   | shipped             |
| `pyo3_async_runtimes::tokio::run_until_complete`   | —                   | shipped             |
| Explicit `torch.cuda.is_available()` device probe  | —                   | shipped             |
| `HF_TOKEN` env-write before `import vllm`          | wired (passthrough) | exercised           |
| Content-addressed `model_id` from HF repo SHA      | —                   | shipped             |
| `criterion` throughput bench + raw-vllm baseline   | placeholder         | shipped             |

## Bridging the asyncio ↔ Tokio gap (RESEARCH Pitfall 2)

`vllm.AsyncLLMEngine.generate(prompt, sampling, request_id)` returns an
async generator that vLLM's own scheduler drives. To consume it from Rust,
we:

1. Build a coroutine on the GIL by calling `engine.generate_one(...)`
   (the Python module's wrapper that drives the async-for loop to
   completion and returns the final `RequestOutput` as a dict).
2. Create a fresh `asyncio` event loop on the worker thread.
3. Hand the coroutine to
   `pyo3_async_runtimes::tokio::run_until_complete(event_loop, async move {
   into_future(coro).await })`. The Python C-level
   `event_loop.run_until_complete` releases the GIL whenever the loop has
   nothing to do — which is exactly when our Rust `await` yields. That is
   what lets vLLM's background scheduler tasks (also on this asyncio
   event loop) make progress.

The contract is verified on every CI build by
[`tests/pyo3_bridge_smoke.rs`][smoke]: a Python `async def smoke()` spawns a
background `threading.Thread` that polls a flag; the assertion fails if the
background thread does not see the flag set, proving the GIL would have
been held across the await (Pitfall 2 regression).

[smoke]: ../../../../crates/rollout-backend-vllm/tests/pyo3_bridge_smoke.rs

## Pitfall 9: explicit device probe

vLLM's `EngineArgs(device="auto")` is unreliable on partially-installed CUDA
hosts (silent CPU fallback). The Python glue probes
`torch.cuda.is_available()` and passes an explicit `device="cuda"` or
`device="cpu"` to `AsyncEngineArgs`. CONTEXT D-VLLM-04's earlier `"auto"`
guidance is superseded by this probe.

```python
import torch
device = "cuda" if torch.cuda.is_available() else "cpu"
args = AsyncEngineArgs(model=model_uri, device=device, ...)
```

## Live integration tests (gated)

Two `#[ignore]`'d integration tests under
`crates/rollout-backend-vllm/tests/` exercise the live engine:

- `vllm_init.rs` — bring up `facebook/opt-125m`; assert
  `backend.model_id()` is content-addressed and stable across two `init()`
  calls of the same URI.
- `vllm_generate.rs` — bring up `Qwen/Qwen2.5-0.5B-Instruct` and run a
  1 prompt × 8 tokens round-trip with a 300 s timeout (RESEARCH Pitfall 8
  CPU mode is slow).

Both tests skip unless `ROLLOUT_VLLM_AVAILABLE=1` is set in the
environment. Run them with:

```bash
ROLLOUT_VLLM_AVAILABLE=1 cargo test \
    -p rollout-backend-vllm --features vllm \
    --test vllm_init --test vllm_generate -- --include-ignored
```

The default workspace `cargo test` skips them.

## Benchmark methodology

`crates/rollout-backend-vllm/benches/throughput.rs` runs a 64-prompt ×
64-token criterion bench against `facebook/opt-125m`; the companion
`scripts/raw_vllm_baseline.py` drives the same prompt set through raw
`vllm.LLM` (sync API). Compare the two tokens/sec numbers to verify the
BACKEND-02 `<10% overhead` exit criterion — a ratio ≥ 0.9 passes.

The bench is gated behind `--features vllm` and `ROLLOUT_VLLM_AVAILABLE=1`.
CI does not run it by default; the perf check lives on the self-hosted GPU
runner per CONTEXT D-CLI-05.

```bash
# rollout side:
ROLLOUT_VLLM_AVAILABLE=1 cargo bench \
    -p rollout-backend-vllm --features vllm --bench throughput

# baseline side:
python scripts/raw_vllm_baseline.py facebook/opt-125m
```

## Cross-references

- [RESEARCH §"Pattern 1"][research] — the PyO3 dedicated-thread shape this
  crate mirrors from plan 02-05.
- [RESEARCH §"Common Pitfalls"][research] — Pitfalls 2 (GIL deadlock),
  9 (`device="auto"` not reliable), 10 (env-write before import).
- [spec 02 §2a][spec-02] — the locked Phase-3 trait surface.

[vllm]: https://docs.vllm.ai/
[pyo3]: https://pyo3.rs/v0.28.0/
[spec-02]: ../../../specs/02-algorithms.md
[spec-03]: ../../../specs/03-plugin-system.md
[research]: ../../../../.planning/phases/03-inference-batch/03-RESEARCH.md
