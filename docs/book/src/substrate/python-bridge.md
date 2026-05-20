# Python bridge (PyO3)

Phase 2 pins `pyo3 = 0.28` + `pyo3-async-runtimes = 0.28` in lockstep. Both crates moved under the [PyO3 organisation](https://github.com/PyO3) in 2025 (the predecessor `pyo3-asyncio` is archived).

## Version pin rationale

| Crate | Pinned version | Why |
| --- | --- | --- |
| `pyo3` | `0.28` | First release with the new `Python::attach` / `Python::try_attach` API (replaces `Python::with_gil`); stable abi3 support; MSRV 1.83 satisfies our 1.88 toolchain. |
| `pyo3-async-runtimes` | `0.28` | Mandatory lockstep with `pyo3` — the FFI surface is private and tied to the patch version. Successor to `pyo3-asyncio` (archived). |

Both versions are declared in `[workspace.dependencies]` so every consumer (currently just `rollout-plugin-host`) inherits the same patch range.

## `abi3-py311` strategy

We enable `pyo3/abi3-py311`. This produces a **single Python extension binary that runs on any CPython 3.11+** instead of building one wheel per minor (3.11 → 3.12 → 3.13). The trade-off:

- ✅ Smaller CI matrix; one wheel ships everywhere.
- ✅ Stdlib `tomllib` is available (used by sidecar samples to parse `pyproject.toml` if needed).
- ✅ pyo3 abi3 was stabilised circa 0.20 and is well-exercised.
- ❌ 3.10 builds are rejected at link time with `cannot set a minimum Python version 3.11 higher than the interpreter version 3.10`. Local dev machines that default to `python3.10` must `export PYENV_VERSION=3.11.x` (or similar) before `cargo build`.
- ❌ Some C-extension features (e.g. private CPython internals) aren't accessible through abi3.

`scripts/preflight.sh` (added in Wave-0 plan 02-00) verifies `python3 >= 3.11` before `make smoke` does anything destructive.

## Dedicated Python OS thread

Per RESEARCH Pitfall 3, mixing PyO3 calls onto random Tokio worker threads deadlocks under contention because the GIL ends up held across `.await` points. The host avoids this by **owning one OS thread per `PluginHostImpl`**:

```text
                 ┌────────────────────┐
   Tokio task ───► mpsc::Sender ─────►│  rollout-py-* OS thread
                 │ (PyTask enum)      │  ───────────────────────
                 │                    │  Python::attach(...) {
                 │                    │      plugin.call(...)
                 │ ◄─── oneshot ──────┤  }
                 └────────────────────┘
```

The worker thread is created with `std::thread::Builder::new().name("rollout-py-<plugin>")` so debuggers and `tracing` spans can distinguish per-plugin Python contexts. With `pyo3/auto-initialize` the interpreter spins up on first `Python::attach`; we do not call the (removed-in-0.28) `prepare_freethreaded_python`.

Heavy-CPU Python code should release the GIL via `Py::allow_threads` per spec 03 §3.2 — Phase 2 in-tree samples are tiny enough not to need this.

## In-tree samples and the no-pip rule

Per AGENTS.md §7, every in-tree sample must run without `pip install`. Phase 2 ships three:

- [`python/examples/sample_inproc/`](https://github.com/astiwari/rollout/tree/main/python/examples/sample_inproc) — PyO3 in-process. `create_plugin().call(method, payload)` echoes payload or returns `b"pong"`.
- [`python/examples/sample_sidecar/`](https://github.com/astiwari/rollout/tree/main/python/examples/sample_sidecar) — Python sidecar over stdlib JSON framing. Runs as `python -m sample_sidecar <socket_path>`.
- [`tests/smoke/plugins/rust_cdylib_sample/`](https://github.com/astiwari/rollout/tree/main/tests/smoke/plugins/rust_cdylib_sample) — Rust cdylib implementing ABI v1.

User plugins are free to bring their own virtualenv with `grpcio`, `numpy`, etc. — the no-pip rule applies only to the in-tree samples that gate `cargo test` and `make smoke`.
