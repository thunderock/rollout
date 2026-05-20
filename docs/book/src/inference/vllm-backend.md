# vLLM backend (`rollout-backend-vllm`)

`rollout-backend-vllm` implements the Phase-3 `InferenceBackend` trait
(`init` / `generate` / `model_id` / `shutdown`) over [vLLM's `AsyncLLMEngine`][vllm]
via [PyO3][pyo3] **in-process**. The crate is the second user of the dedicated
Python OS-thread pattern hardened in plan 02-05; the first was
`rollout-plugin-host`. The architecture is documented in spec
[03-plugin-system §3.2][spec-03] and the trait surface in spec
[02-algorithms §2 / §2a][spec-02].

> **Status (plan 03-01):** Wave-2 skeleton only. The PyO3 dedicated-thread
> bootstrap, `mpsc::Sender<VllmTask>` dispatch shape, and Python module stub
> all ship; `generate` returns a typed
> `Fatal(PluginContract { msg: "vllm engine not yet wired (Wave 2 …)" })`
> until plan 03-03 (Wave 3) lands the real `AsyncLLMEngine` bridge.

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
startup. Plan 03-03 swaps the stub Python module for the real
`AsyncLLMEngine` wrapper.

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
| `python/rollout/backends/vllm/engine.py` stub      | shipped             | replaced            |
| `AsyncLLMEngine.from_engine_args` wiring           | —                   | shipped             |
| `py.detach(\|\| rt.block_on(into_future(coro)))`   | —                   | shipped             |
| Plan-time HF model probe (`HF_TOKEN`, repo SHA)    | —                   | shipped             |

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
