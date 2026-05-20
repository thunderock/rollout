//! Dedicated Python-thread worker that owns the vLLM `AsyncLLMEngine` handle.
//!
//! Mirrors plan 02-05's `Pyo3State` pattern: one OS thread named
//! `rollout-py-vllm-<engine_id>`, an `mpsc::Sender<VllmTask>` for Tokio→Python
//! call hops, and a `Drop` join for clean shutdown. Wave-2 ships the dispatch
//! shape; the live `Python::attach` + `py.detach(|| rt.block_on(into_future))`
//! bridge to `AsyncLLMEngine.generate` lands in plan 03-03 (Wave 3).

use rollout_core::{Completion, CoreError, ModelRef, SamplingParams};
use tokio::sync::{mpsc, oneshot};

use crate::errors::{internal, wave2_stub};

/// Tasks the dedicated Python worker thread accepts.
///
/// `dead_code` is allowed at variant scope because the `vllm`-feature worker
/// reads every field (in plan 03-03), but the default-features stub worker
/// only destructures `reply` — without the allow, the default-features
/// build would warn on `model` / `prompt` / `params` / `request_id`.
#[allow(clippy::large_enum_variant, dead_code)]
pub(crate) enum VllmTask {
    /// Bring the engine up against `model`.
    Init {
        /// Model reference to load.
        model: ModelRef,
        /// Reply channel for the init result.
        reply: oneshot::Sender<Result<(), CoreError>>,
    },
    /// Generate one completion. Wave-2 stub always returns the typed Wave-2 error.
    Generate {
        /// Prompt text.
        prompt: String,
        /// Sampling configuration.
        params: SamplingParams,
        /// Stable per-call ID — used by vLLM's scheduler in Wave 3 (Pitfall 6).
        request_id: String,
        /// Reply channel for the completion result.
        reply: oneshot::Sender<Result<Completion, CoreError>>,
    },
    /// Tear down the engine and exit the worker thread.
    Shutdown,
}

/// Worker-side handle: send `VllmTask`s into the Python OS thread.
pub(crate) struct VllmEngine {
    pub(crate) tx: mpsc::Sender<VllmTask>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl VllmEngine {
    /// Spawn the dedicated Python thread `rollout-py-vllm-<engine_id>`.
    ///
    /// `HF_TOKEN` handling (RESEARCH Pitfall 10): if `secret_token` is `Some`,
    /// the spawned thread sets `HF_TOKEN` in its own `os.environ` BEFORE
    /// importing `vllm`, so the gated-model download path picks it up.
    #[allow(clippy::needless_pass_by_value)] // Token is owned-on-worker-thread in the `vllm` build.
    pub(crate) fn spawn(
        engine_id: &str,
        #[cfg_attr(not(feature = "vllm"), allow(unused_variables))] secret_token: Option<String>,
    ) -> Result<Self, CoreError> {
        let (tx, rx) = mpsc::channel(64);
        let name = format!("rollout-py-vllm-{engine_id}");
        let thread = std::thread::Builder::new()
            .name(name)
            .spawn(move || {
                #[cfg(feature = "vllm")]
                worker_main_vllm(rx, secret_token);
                #[cfg(not(feature = "vllm"))]
                worker_main_stub(rx);
            })
            .map_err(|e| internal(format!("spawn rollout-py-vllm thread: {e}")))?;
        Ok(Self {
            tx,
            thread: Some(thread),
        })
    }
}

impl Drop for VllmEngine {
    fn drop(&mut self) {
        // Best-effort: queue a shutdown then join. `try_send` fails silently if
        // the channel is full (which it shouldn't be — capacity 64, single producer).
        let _ = self.tx.try_send(VllmTask::Shutdown);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Default-features worker: drain the queue and return the Wave-2 stub error
/// from `Generate`. Lets tests exercise the full Tokio→thread dispatch path
/// without a Python interpreter present.
#[cfg(not(feature = "vllm"))]
fn worker_main_stub(mut rx: mpsc::Receiver<VllmTask>) {
    while let Some(task) = rx.blocking_recv() {
        match task {
            VllmTask::Shutdown => break,
            VllmTask::Init { reply, .. } => {
                let _ = reply.send(Ok(()));
            }
            VllmTask::Generate { reply, .. } => {
                let _ = reply.send(Err(wave2_stub()));
            }
        }
    }
}

/// `vllm`-feature worker: imports `rollout.backends.vllm.engine` once, then
/// dispatches. Wave-2 still returns the Wave-2 stub error from `Generate`;
/// plan 03-03 replaces the `Generate` arm with the real
/// `py.detach(|| rt.block_on(into_future(coro)))` bridge.
#[cfg(feature = "vllm")]
#[allow(clippy::needless_pass_by_value)] // `secret_token` is owned-on-thread by design.
fn worker_main_vllm(mut rx: mpsc::Receiver<VllmTask>, secret_token: Option<String>) {
    use pyo3::prelude::*;
    use pyo3::types::PyDict;

    let init_result: PyResult<()> = Python::attach(|py| {
        // Pitfall 10: env-write BEFORE `import vllm` so huggingface_hub sees the token.
        if let Some(token) = &secret_token {
            let os = py.import("os")?;
            let environ: Bound<'_, PyDict> = os.getattr("environ")?.cast_into()?;
            environ.set_item("HF_TOKEN", token)?;
        }
        // The Python stub does not import vllm; plan 03-03 swaps the stub for the
        // real AsyncLLMEngine wrapper which DOES `import vllm` at module top.
        let _engine_module = py.import("rollout.backends.vllm.engine")?;
        Ok(())
    });

    // If import failed, drain the queue with errors instead of crashing the thread.
    if let Err(e) = init_result {
        let err_str = e.to_string();
        while let Some(task) = rx.blocking_recv() {
            match task {
                VllmTask::Shutdown => break,
                VllmTask::Init { reply, .. } => {
                    let _ = reply.send(Err(crate::errors::transient(&format!(
                        "vllm module import failed: {err_str}"
                    ))));
                }
                VllmTask::Generate { reply, .. } => {
                    let _ = reply.send(Err(crate::errors::transient(&format!(
                        "vllm module import failed: {err_str}"
                    ))));
                }
            }
        }
        return;
    }

    while let Some(task) = rx.blocking_recv() {
        match task {
            VllmTask::Shutdown => break,
            VllmTask::Init { model, reply } => {
                let _ = reply.send(stub_init(&model));
            }
            VllmTask::Generate { reply, .. } => {
                // Wave 3 wires `engine.generate_one(prompt, request_id, **kwargs)`
                // here via `crate::python_glue::samplingparams_to_pydict` +
                // `pyo3_async_runtimes::tokio::into_future` (RESEARCH Pattern 2).
                let _ = reply.send(Err(wave2_stub()));
            }
        }
    }
}

/// Wave-2 placeholder: pretend init succeeded. Plan 03-03 replaces this with
/// the real `AsyncLLMEngine.from_engine_args` call routed through the Python
/// stub module — that version returns the real `Result` (this signature keeps
/// the call-site shape stable across waves).
#[cfg(feature = "vllm")]
#[allow(clippy::unnecessary_wraps)]
fn stub_init(_m: &ModelRef) -> Result<(), CoreError> {
    Ok(())
}
