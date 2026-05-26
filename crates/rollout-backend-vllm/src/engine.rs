//! Dedicated Python-thread worker that owns the vLLM `AsyncLLMEngine` handle.
//!
//! Mirrors plan 02-05's `Pyo3State` pattern: one OS thread named
//! `rollout-py-vllm-<engine_id>`, an `mpsc::Sender<VllmTask>` for Tokio→Python
//! call hops, and a `Drop` join for clean shutdown. Plan 03-03 (Wave 3) wires
//! the live `AsyncLLMEngine` bridge through
//! `py.detach(|| rt.block_on(into_future(coro)))` (RESEARCH Pitfall 2).

use rollout_core::{Completion, CoreError, ModelRef, SamplingParams};
use tokio::sync::{mpsc, oneshot};

use crate::errors::internal;
#[cfg(not(feature = "vllm"))]
use crate::errors::wave2_stub;

/// Active mode of the dedicated Python worker thread (Phase-4).
///
/// Phase-4 simplification (D-TRAIN-PATH-02): a backend instance is single-mode
/// for its lifetime; bidirectional inference↔training swaps land in Phase 9.
/// `Inference` is reserved for the Phase-9 path that promotes Init →
/// `ActiveMode::Inference` so `set_train_mode(true)` after generate has a
/// typed rejection point; in Phase 4 only `None` and `Training` are reached.
#[cfg(feature = "train")]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[allow(dead_code)]
pub(crate) enum ActiveMode {
    /// No mode chosen yet (worker just spun up).
    None,
    /// Inference mode (vLLM `AsyncLLMEngine`). Reserved; see Phase-9 note.
    Inference,
    /// Training mode (HF transformers + accelerate via `train.py`).
    Training,
}

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
        /// Reply channel: returns the resolved model SHA (or URI fallback).
        reply: oneshot::Sender<Result<String, CoreError>>,
    },
    /// Generate one completion via the live `AsyncLLMEngine` (Wave 3).
    Generate {
        /// Prompt text.
        prompt: String,
        /// Sampling configuration.
        params: SamplingParams,
        /// Stable per-call ID — vLLM uses this as its scheduler key (Pitfall 6).
        request_id: String,
        /// Reply channel for the completion result.
        reply: oneshot::Sender<Result<Completion, CoreError>>,
    },
    /// Tear down the engine and exit the worker thread.
    Shutdown,
    /// Switch the worker between inference and training modes (Phase 4).
    #[cfg(feature = "train")]
    SetTrainMode {
        /// `true` flips to training mode; `false` tears training state down.
        enabled: bool,
        /// Reply channel.
        reply: oneshot::Sender<Result<(), CoreError>>,
    },
    /// Run one training forward + loss pass (Phase 4).
    #[cfg(feature = "train")]
    ForwardWithLoss {
        /// Raw text rows the backend will tokenize.
        rows: Vec<String>,
        /// Mask-scope hint.
        loss_scope: rollout_core::LossScope,
        /// Reply channel.
        reply: oneshot::Sender<Result<rollout_core::LossOutput, CoreError>>,
    },
    /// Apply accumulated gradients (Phase 4).
    #[cfg(feature = "train")]
    OptimizerStep {
        /// Opaque grad handle from a prior `ForwardWithLoss`.
        grads: rollout_core::GradHandle,
        /// Optimizer settings (lr is consumed; full schedule is Phase 9).
        opt: rollout_core::config::OptimizerSettings,
        /// Reply channel.
        reply: oneshot::Sender<Result<(), CoreError>>,
    },
    /// Persist Accelerator state via `accelerate.save_state` (Phase 4).
    #[cfg(feature = "train")]
    SaveWeights {
        /// Target directory (caller-supplied tempdir).
        target_dir: std::path::PathBuf,
        /// Reply channel.
        reply: oneshot::Sender<Result<rollout_core::ContentId, CoreError>>,
    },
    /// Restore Accelerator state via `accelerate.load_state` (Phase 4).
    #[cfg(feature = "train")]
    LoadWeights {
        /// Source directory.
        src_dir: std::path::PathBuf,
        /// Reply channel.
        reply: oneshot::Sender<Result<(), CoreError>>,
    },
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
            VllmTask::Init { model, reply } => {
                let _ = reply.send(Ok(model.uri.clone()));
            }
            VllmTask::Generate { reply, .. } => {
                let _ = reply.send(Err(wave2_stub()));
            }
        }
    }
}

/// `train`-feature worker (Phase 4): no live vLLM engine; the dedicated Python
/// thread serves training tasks only. Built when `train` is on but `vllm` is
/// not — currently impossible because `train = ["vllm"]` in Cargo.toml, but
/// guarded explicitly so a future fork can opt out.
///
/// Actual `--features train` builds run `worker_main_vllm` below, which now
/// dispatches BOTH inference and train arms.
#[cfg(all(feature = "train", not(feature = "vllm")))]
#[allow(clippy::needless_pass_by_value, dead_code)]
fn worker_main_train_only(mut rx: mpsc::Receiver<VllmTask>, secret_token: Option<String>) {
    let mut active_mode = ActiveMode::None;
    while let Some(task) = rx.blocking_recv() {
        match task {
            VllmTask::Shutdown => break,
            VllmTask::Init { model, reply } => {
                let _ = reply.send(Ok(model.uri.clone()));
            }
            VllmTask::Generate { reply, .. } => {
                let _ = reply.send(Err(crate::errors::transient(
                    "Generate not supported under --features train without vllm",
                )));
            }
            VllmTask::SetTrainMode { enabled, reply } => {
                let r = crate::train::run_set_train_mode(
                    enabled,
                    &mut active_mode,
                    secret_token.as_ref(),
                );
                let _ = reply.send(r);
            }
            VllmTask::ForwardWithLoss {
                rows,
                loss_scope,
                reply,
            } => {
                let _ = reply.send(crate::train::run_forward_with_loss(&rows, &loss_scope));
            }
            VllmTask::OptimizerStep { grads, opt, reply } => {
                let _ = reply.send(crate::train::run_optimizer_step(&grads, &opt));
            }
            VllmTask::SaveWeights { target_dir, reply } => {
                let _ = reply.send(crate::train::run_save_weights(&target_dir));
            }
            VllmTask::LoadWeights { src_dir, reply } => {
                let _ = reply.send(crate::train::run_load_weights(&src_dir));
            }
        }
    }
}

/// `vllm`-feature worker (Wave 3): imports `rollout.backends.vllm.engine`,
/// then dispatches Init/Generate through the live `AsyncLLMEngine`. Generate
/// uses the `py.detach(|| rt.block_on(into_future(coro)))` bridge per RESEARCH
/// Pitfall 2 — releasing the GIL across `block_on` lets vLLM's background
/// scheduler tasks make progress.
#[cfg(feature = "vllm")]
#[allow(clippy::needless_pass_by_value)] // `secret_token` is owned-on-thread by design.
fn worker_main_vllm(mut rx: mpsc::Receiver<VllmTask>, secret_token: Option<String>) {
    use pyo3::prelude::*;
    use pyo3::types::PyDict;

    // Phase-4: track active mode so train-side calls can refuse mid-run swaps.
    #[cfg(feature = "train")]
    let mut active_mode = ActiveMode::None;

    // Phase-4: under `--features train` we may receive train tasks BEFORE any
    // inference task — and the inference module import is non-trivial (pulls
    // vllm, which is heavyweight). Defer the import until the first inference
    // task arrives.
    let mut inference_module: Option<Py<PyModule>> = None;
    let mut inference_import_err: Option<String> = None;

    // Helper: lazily import the inference module on first inference task.
    let import_inference = |inference_module: &mut Option<Py<PyModule>>,
                            inference_import_err: &mut Option<String>| {
        if inference_module.is_some() || inference_import_err.is_some() {
            return;
        }
        // Pitfall 10: env-write BEFORE py.import. Phase-3 secret_token contract.
        let import_result: PyResult<Py<PyModule>> = Python::attach(|py| {
            if let Some(token) = &secret_token {
                let os = py.import("os")?;
                let environ: Bound<'_, PyDict> = os.getattr("environ")?.cast_into()?;
                environ.set_item("HF_TOKEN", token)?;
            }
            let module = py.import("rollout.backends.vllm.engine")?;
            Ok(module.unbind())
        });
        match import_result {
            Ok(m) => *inference_module = Some(m),
            Err(e) => *inference_import_err = Some(e.to_string()),
        }
    };

    while let Some(task) = rx.blocking_recv() {
        match task {
            VllmTask::Shutdown => {
                if let Some(m) = inference_module.as_ref() {
                    let _ = Python::attach(|py| -> PyResult<()> {
                        let _ = m.bind(py).call_method0("shutdown");
                        Ok(())
                    });
                }
                break;
            }
            VllmTask::Init { model, reply } => {
                import_inference(&mut inference_module, &mut inference_import_err);
                match (&inference_module, &inference_import_err) {
                    (Some(m), _) => {
                        let _ = reply.send(run_init(m, &model));
                    }
                    (_, Some(e)) => {
                        let _ = reply.send(Err(crate::errors::transient(&format!(
                            "vllm module import failed: {e}"
                        ))));
                    }
                    _ => unreachable!(),
                }
            }
            VllmTask::Generate {
                prompt,
                params,
                request_id,
                reply,
            } => {
                import_inference(&mut inference_module, &mut inference_import_err);
                match (&inference_module, &inference_import_err) {
                    (Some(m), _) => {
                        let _ = reply.send(run_generate(m, &prompt, &params, &request_id));
                    }
                    (_, Some(e)) => {
                        let _ = reply.send(Err(crate::errors::transient(&format!(
                            "vllm module import failed: {e}"
                        ))));
                    }
                    _ => unreachable!(),
                }
            }
            #[cfg(feature = "train")]
            VllmTask::SetTrainMode { enabled, reply } => {
                let r = crate::train::run_set_train_mode(
                    enabled,
                    &mut active_mode,
                    secret_token.as_ref(),
                );
                let _ = reply.send(r);
            }
            #[cfg(feature = "train")]
            VllmTask::ForwardWithLoss {
                rows,
                loss_scope,
                reply,
            } => {
                let _ = reply.send(crate::train::run_forward_with_loss(&rows, &loss_scope));
            }
            #[cfg(feature = "train")]
            VllmTask::OptimizerStep { grads, opt, reply } => {
                let _ = reply.send(crate::train::run_optimizer_step(&grads, &opt));
            }
            #[cfg(feature = "train")]
            VllmTask::SaveWeights { target_dir, reply } => {
                let _ = reply.send(crate::train::run_save_weights(&target_dir));
            }
            #[cfg(feature = "train")]
            VllmTask::LoadWeights { src_dir, reply } => {
                let _ = reply.send(crate::train::run_load_weights(&src_dir));
            }
        }
    }
}

/// Call the Python-side `init(model_uri, **engine_args)` and return the SHA.
#[cfg(feature = "vllm")]
fn run_init(
    module: &pyo3::Py<pyo3::types::PyModule>,
    model: &ModelRef,
) -> Result<String, CoreError> {
    use crate::errors::py_to_core;
    use pyo3::prelude::*;
    use pyo3::types::PyDict;

    Python::attach(|py| {
        let m = module.bind(py);
        let kwargs = PyDict::new(py);
        if let Some(tok) = &model.tokenizer {
            kwargs.set_item("tokenizer", tok).map_err(py_to_core)?;
        }
        let res = m
            .call_method("init", (model.uri.as_str(),), Some(&kwargs))
            .map_err(py_to_core)?;
        let sha: String = res.extract().map_err(py_to_core)?;
        Ok(sha)
    })
}

/// Drive `engine.generate_one(prompt, request_id, **sampling)` to completion.
///
/// Architecture: spin up a fresh `asyncio` event loop on this worker thread,
/// then drive the Python coroutine through it via
/// `pyo3_async_runtimes::tokio::run_until_complete`. The Rust closure inside
/// `run_until_complete` uses `into_future` to obtain a Rust `Future`
/// pointing at the Python coroutine and awaits it.
///
/// RESEARCH Pitfall 2 (GIL deadlock) is averted because the underlying
/// `event_loop.run_until_complete` is a Python C-level call that releases
/// the GIL whenever it has nothing to do — letting vLLM's background tasks
/// (which also run on this asyncio event loop) progress. See
/// `tests/pyo3_bridge_smoke.rs` for the regression test that proves a
/// background Python thread runs concurrently with the Rust `await`.
#[cfg(feature = "vllm")]
fn run_generate(
    module: &pyo3::Py<pyo3::types::PyModule>,
    prompt: &str,
    params: &SamplingParams,
    request_id: &str,
) -> Result<Completion, CoreError> {
    use crate::errors::py_to_core;
    use crate::python_glue::samplingparams_to_pydict;
    use pyo3::prelude::*;
    use pyo3::types::PyDict;
    use pyo3_async_runtimes::tokio::{into_future, run_until_complete};

    let prompt_owned = prompt.to_owned();
    let request_id_owned = request_id.to_owned();
    let params_owned = params.clone();

    // Drive the Python coroutine inside a fresh asyncio loop on this thread.
    // `run_until_complete` is what releases the GIL across the actual await
    // window (RESEARCH Pitfall 2): the underlying Python C-level
    // `loop.run_until_complete` runs the asyncio scheduler, which is what
    // gives vLLM's background tasks the GIL when our Rust `await` yields.
    let result_obj: pyo3::Py<pyo3::PyAny> = Python::attach(|py| -> Result<_, CoreError> {
        let asyncio = py.import("asyncio").map_err(py_to_core)?;
        let event_loop = asyncio.call_method0("new_event_loop").map_err(py_to_core)?;
        let module_for_async = module.clone_ref(py);
        let driver = async move {
            let coro = Python::attach(|py| -> PyResult<Py<PyAny>> {
                let m = module_for_async.bind(py);
                let kwargs = samplingparams_to_pydict(py, &params_owned).map_err(|e| {
                    pyo3::exceptions::PyRuntimeError::new_err(format!(
                        "samplingparams_to_pydict: {e:?}"
                    ))
                })?;
                let coro = m.call_method(
                    "generate_one",
                    (prompt_owned.as_str(), request_id_owned.as_str()),
                    Some(&kwargs),
                )?;
                Ok(coro.unbind())
            })?;
            let fut = Python::attach(|py| into_future(coro.into_bound(py)))?;
            fut.await
        };
        let event_loop_for_close = event_loop.clone();
        let res = run_until_complete::<_, Py<PyAny>>(event_loop, driver).map_err(py_to_core)?;
        // Close the loop to release its resources; ignore close errors.
        let _ = event_loop_for_close.call_method0("close");
        Ok(res)
    })?;

    // Re-acquire the GIL, convert dict → Completion.
    Python::attach(|py| -> Result<Completion, CoreError> {
        let bound = result_obj.bind(py);
        let dict: Bound<'_, PyDict> = bound
            .clone()
            .cast_into()
            .map_err(|e| py_to_core(e.into()))?;
        let text: String = dict
            .get_item("text")
            .map_err(py_to_core)?
            .ok_or_else(|| internal("generate_one returned dict without `text`"))?
            .extract()
            .map_err(py_to_core)?;
        let finish_reason: String = dict
            .get_item("finish_reason")
            .map_err(py_to_core)?
            .ok_or_else(|| internal("generate_one returned dict without `finish_reason`"))?
            .extract()
            .map_err(py_to_core)?;
        let prompt_tokens: u32 = dict
            .get_item("prompt_tokens")
            .map_err(py_to_core)?
            .ok_or_else(|| internal("generate_one returned dict without `prompt_tokens`"))?
            .extract()
            .map_err(py_to_core)?;
        let completion_tokens: u32 = dict
            .get_item("completion_tokens")
            .map_err(py_to_core)?
            .ok_or_else(|| internal("generate_one returned dict without `completion_tokens`"))?
            .extract()
            .map_err(py_to_core)?;
        Ok(Completion {
            text,
            finish_reason,
            prompt_tokens,
            completion_tokens,
        })
    })
}
