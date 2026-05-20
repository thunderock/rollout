---
phase: 03-inference-batch
plan: 01
type: execute
wave: 2
depends_on: [03-00]
files_modified:
  - crates/rollout-backend-vllm/src/lib.rs
  - crates/rollout-backend-vllm/src/backend.rs
  - crates/rollout-backend-vllm/src/engine.rs
  - crates/rollout-backend-vllm/src/python_glue.rs
  - crates/rollout-backend-vllm/src/errors.rs
  - crates/rollout-backend-vllm/tests/sampling_params.rs
  - crates/rollout-backend-vllm/tests/backend_stub.rs
  - python/rollout/backends/__init__.py
  - python/rollout/backends/vllm/__init__.py
  - python/rollout/backends/vllm/engine.py
  - docs/book/src/inference/vllm-backend.md
  - docs/book/src/SUMMARY.md
autonomous: true
requirements: [BACKEND-01, DOCS-01, DOCS-02, DOCS-03]
must_haves:
  truths:
    - "rollout-backend-vllm builds with --features '' and --features vllm with zero warnings."
    - "`VllmBackend` struct implements `InferenceBackend` returning a typed `Fatal { PluginContract { msg: \"vllm engine not yet wired (Wave 2)\" } }` for generate/init until plan 03-03 lands the live engine."
    - "PyO3 dedicated-thread bootstrap (`rollout-py-vllm-<run_id>`) mirrors plan 02-05's pattern: `Python::attach` + `tokio::sync::mpsc<VllmTask>` + per-task current-thread Tokio runtime."
    - "Python-side glue `python/rollout/backends/vllm/engine.py` is importable + exposes `init`, `generate_one`, `shutdown` (Wave 2 wires the real AsyncLLMEngine; Wave 1 ships a deterministic stub that returns `{text: \"STUB:<prompt>\", finish_reason: \"stop\", prompt_tokens: 0, completion_tokens: 0}`)."
    - "SamplingParams postcard round-trip determinism is enforced by `sampling_params.rs` test (RESEARCH Pitfall 1)."
  artifacts:
    - path: crates/rollout-backend-vllm/src/backend.rs
      provides: "VllmBackend + impl InferenceBackend (stub) + Wave-2 hook points"
      contains: "pub struct VllmBackend"
    - path: crates/rollout-backend-vllm/src/engine.rs
      provides: "VllmEngine: dedicated Python thread + mpsc task channel"
      contains: "rollout-py-vllm"
    - path: crates/rollout-backend-vllm/src/python_glue.rs
      provides: "Rust ↔ Python conversion helpers"
      contains: "py_to_core"
    - path: python/rollout/backends/vllm/engine.py
      provides: "Python module imported by the Rust thread"
      contains: "def init"
    - path: docs/book/src/inference/vllm-backend.md
      provides: "mdBook chapter for the backend crate"
  key_links:
    - from: crates/rollout-backend-vllm/src/backend.rs
      to: "crates/rollout-backend-vllm/src/engine.rs"
      via: "Arc<VllmEngine>"
      pattern: "VllmEngine"
    - from: crates/rollout-backend-vllm/src/engine.rs
      to: "python/rollout/backends/vllm/engine.py"
      via: "py.import(\"rollout.backends.vllm.engine\")"
      pattern: "rollout\\.backends\\.vllm\\.engine"
---

<objective>
Land `rollout-backend-vllm` skeleton: the Rust adapter, PyO3 dedicated-thread bootstrap, `VllmBackend` struct implementing the Phase-3 `InferenceBackend` surface, Python-side glue module under `python/rollout/backends/vllm/`. The backend returns typed stub errors / stub completions until plan 03-03 wires the real `AsyncLLMEngine`. This isolates the PyO3 plumbing risk from the vLLM API risk.

Purpose: prove the PyO3 thread bootstrap + InferenceBackend impl compile + run without live vLLM. Decouples the FFI scaffolding from the engine integration so Wave-2 plans don't fight both at once.
Output: skeleton crate, importable Python module, mdBook chapter.
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
@$HOME/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/phases/03-inference-batch/03-CONTEXT.md
@.planning/phases/03-inference-batch/03-RESEARCH.md
@.planning/phases/02-local-substrate/02-05-rollout-plugin-host-SUMMARY.md
@AGENTS.md
@crates/rollout-plugin-host/src/lib.rs
@crates/rollout-core/src/traits/backend.rs

<interfaces>
From plan 03-00 (now in rollout-core):
```rust
pub struct Prompt(pub String);
pub struct Completion { pub text, pub finish_reason, pub prompt_tokens, pub completion_tokens }
pub struct ModelRef { pub uri: String, pub tokenizer: Option<String> }
pub struct SamplingParams { /* ...; #[non_exhaustive] */ }
#[async_trait] pub trait InferenceBackend: Send + Sync {
    async fn init(&mut self, model: ModelRef) -> Result<(), CoreError>;
    async fn generate(&self, prompts: &[Prompt], params: &SamplingParams) -> Result<Vec<Completion>, CoreError>;
    fn model_id(&self) -> &ContentId;
    async fn shutdown(&mut self) -> Result<(), CoreError>;
}
```

From plan 02-05 (PyO3 pattern proven):
```rust
// dedicated OS thread, no Tokio context
std::thread::Builder::new().name(format!("rollout-py-vllm-{id}")).spawn(move || {
    Python::attach(|py| {
        // import module, loop on rx.blocking_recv()
    });
});
// Tokio side: oneshot::channel reply pattern
```
RESEARCH §"Pattern 1" gives the full VllmTask enum + worker_main shape.
</interfaces>
</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: VllmBackend skeleton + dedicated Python thread + stub completion path</name>
  <read_first>
    - crates/rollout-plugin-host/src/lib.rs + src/modes/pyo3.rs (the PyO3 dedicated-thread pattern this mirrors)
    - .planning/phases/02-05-rollout-plugin-host-SUMMARY.md (Rule-1 fixes: pyo3 0.28 API: Python::attach not with_gil; auto-initialize feature)
    - crates/rollout-core/src/traits/backend.rs (post-03-00 surface)
    - .planning/phases/03-inference-batch/03-RESEARCH.md §"Pattern 1" + §"Pattern 2"
    - .planning/phases/03-inference-batch/03-CONTEXT.md §"Implementation Decisions" D-VLLM-01..05
  </read_first>
  <behavior>
    - Test 1: `cargo test -p rollout-backend-vllm --test sampling_params` proves `postcard::to_stdvec(&SamplingParams::default())` is byte-deterministic across two runs (RESEARCH Pitfall 1).
    - Test 2: `cargo test -p rollout-backend-vllm --test sampling_params` proves `SamplingParams { stop: vec![], ... }` and the serde-default-instantiated value hash to byte-identical postcard output (RESEARCH Pitfall 4).
    - Test 3: `cargo test -p rollout-backend-vllm --test backend_stub` — default-features build: `VllmBackend::new()` constructs, `model_id()` returns a stable `ContentId` derived from the uri at init, `generate(&[Prompt("hi")], &SamplingParams::default())` returns `Err(CoreError::Fatal(PluginContract { msg: <contains "Wave 2"> }))`.
    - Test 4: `VllmBackend: Send + Sync` static-asserted via `fn _assert_send_sync<T: Send + Sync>() {} _assert_send_sync::<VllmBackend>();`.
  </behavior>
  <action>
    Create `crates/rollout-backend-vllm/src/errors.rs`:
    - `pub(crate) fn py_to_core(e: pyo3::PyErr) -> CoreError` — maps `PyErr::value(py).to_string()` into `CoreError::Fatal(FatalError::PluginContract { msg })`. Pattern matches plan 02-05's `python::python_err_to_core`.
    - `pub(crate) fn io_to_core(e: std::io::Error) -> CoreError` — `Fatal(Internal { msg })`.
    - `pub(crate) fn transient(msg: &str) -> CoreError` — `Recoverable(Transient { msg, retry: RetryHint::Never })`.

    Create `crates/rollout-backend-vllm/src/python_glue.rs`:
    - `pub(crate) fn samplingparams_to_pydict(py: Python<'_>, p: &SamplingParams) -> PyResult<Bound<'_, PyDict>>` — maps SamplingParams → Python dict matching `python/rollout/backends/vllm/engine.py::generate_one`'s kwargs.
    - `pub(crate) fn pydict_to_completion(d: &Bound<'_, PyDict>) -> Result<Completion, CoreError>` — extract text/finish_reason/prompt_tokens/completion_tokens.

    Create `crates/rollout-backend-vllm/src/engine.rs` per RESEARCH §"Pattern 1":
    ```rust
    use pyo3::prelude::*;
    use tokio::sync::{mpsc, oneshot};
    use rollout_core::{CoreError, Completion, ModelRef, SamplingParams, ContentId};
    use crate::errors::{py_to_core, transient};

    pub(crate) enum VllmTask {
        Init   { model: ModelRef, reply: oneshot::Sender<Result<(), CoreError>> },
        Generate { prompt: String, params: SamplingParams, request_id: String, reply: oneshot::Sender<Result<Completion, CoreError>> },
        Shutdown,
    }

    pub(crate) struct VllmEngine {
        pub(crate) tx: mpsc::Sender<VllmTask>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl VllmEngine {
        pub(crate) fn spawn(plugin_id: &str) -> Result<Self, CoreError> {
            let (tx, rx) = mpsc::channel(64);
            let name = format!("rollout-py-vllm-{plugin_id}");
            let thread = std::thread::Builder::new().name(name)
                .spawn(move || worker_main(rx))
                .map_err(|e| CoreError::Fatal(rollout_core::FatalError::Internal { msg: e.to_string() }))?;
            Ok(Self { tx, thread: Some(thread) })
        }
    }

    impl Drop for VllmEngine {
        fn drop(&mut self) {
            // best-effort shutdown
            let _ = self.tx.try_send(VllmTask::Shutdown);
            if let Some(t) = self.thread.take() { let _ = t.join(); }
        }
    }

    #[cfg(feature = "vllm")]
    fn worker_main(mut rx: mpsc::Receiver<VllmTask>) {
        Python::attach(|py| {
            let _vllm = py.import("rollout.backends.vllm.engine").expect("engine module import");
            loop {
                let Some(task) = rx.blocking_recv() else { break };
                match task {
                    VllmTask::Shutdown => break,
                    VllmTask::Init { model, reply } => {
                        let _ = reply.send(stub_init(py, &model));
                    }
                    VllmTask::Generate { reply, .. } => {
                        // Wave 2 wires real engine.generate_one here
                        let _ = reply.send(Err(CoreError::Fatal(rollout_core::FatalError::PluginContract { msg: "vllm engine not yet wired (Wave 2)".into() })));
                    }
                }
            }
        });
    }

    #[cfg(not(feature = "vllm"))]
    fn worker_main(mut rx: mpsc::Receiver<VllmTask>) {
        // default-features build never imports Python; just drain the queue
        // returning typed errors so tests can exercise the dispatch path.
        loop {
            let Some(task) = rx.blocking_recv() else { break };
            match task {
                VllmTask::Shutdown => break,
                VllmTask::Init { reply, .. } => { let _ = reply.send(Ok(())); }
                VllmTask::Generate { reply, .. } => {
                    let _ = reply.send(Err(CoreError::Fatal(rollout_core::FatalError::PluginContract {
                        msg: "vllm engine not yet wired (Wave 2)".into()
                    })));
                }
            }
        }
    }

    #[cfg(feature = "vllm")]
    fn stub_init(_py: Python<'_>, _m: &ModelRef) -> Result<(), CoreError> {
        // Wave 2 replaces with the real AsyncLLMEngine.from_engine_args path.
        Ok(())
    }
    ```

    Create `crates/rollout-backend-vllm/src/backend.rs`:
    ```rust
    use async_trait::async_trait;
    use rollout_core::{Completion, ContentId, CoreError, InferenceBackend, ModelRef, Prompt, SamplingParams};
    use tokio::sync::oneshot;
    use crate::engine::{VllmEngine, VllmTask};
    use crate::errors::transient;

    pub struct VllmBackend {
        engine: VllmEngine,
        model_id: ContentId,
    }

    impl VllmBackend {
        pub fn new(plugin_id: &str) -> Result<Self, CoreError> {
            Ok(Self {
                engine: VllmEngine::spawn(plugin_id)?,
                // Phase-3 stub: hash the plugin_id until init() computes the real model hash from HF SHA
                model_id: ContentId::from(*blake3::hash(plugin_id.as_bytes()).as_bytes()),
            })
        }
    }

    #[async_trait]
    impl InferenceBackend for VllmBackend {
        async fn init(&mut self, model: ModelRef) -> Result<(), CoreError> {
            // Phase-3 §D-BACKEND-03: reject streaming at validate-time. Defer here for now.
            self.model_id = ContentId::from(*blake3::hash(model.uri.as_bytes()).as_bytes());
            let (reply_tx, reply_rx) = oneshot::channel();
            self.engine.tx.send(VllmTask::Init { model, reply: reply_tx }).await
                .map_err(|_| transient("engine closed"))?;
            reply_rx.await.map_err(|_| transient("reply dropped"))?
        }

        async fn generate(&self, prompts: &[Prompt], params: &SamplingParams)
            -> Result<Vec<Completion>, CoreError>
        {
            if params.stream {
                return Err(CoreError::Fatal(rollout_core::FatalError::ConfigInvalid {
                    msg: "streaming generation is Phase 8 (INFER-01)".into()
                }));
            }
            // Per AGENTS.md principle #2: one generate per prompt; let vLLM's continuous batcher do the work.
            let mut out = Vec::with_capacity(prompts.len());
            for (i, p) in prompts.iter().enumerate() {
                let (reply_tx, reply_rx) = oneshot::channel();
                self.engine.tx.send(VllmTask::Generate {
                    prompt: p.0.clone(),
                    params: params.clone(),
                    request_id: format!("req-{i}"),
                    reply: reply_tx,
                }).await.map_err(|_| transient("engine closed"))?;
                out.push(reply_rx.await.map_err(|_| transient("reply dropped"))??);
            }
            Ok(out)
        }

        fn model_id(&self) -> &ContentId { &self.model_id }

        async fn shutdown(&mut self) -> Result<(), CoreError> {
            let _ = self.engine.tx.send(VllmTask::Shutdown).await;
            Ok(())
        }
    }
    ```

    Update `crates/rollout-backend-vllm/src/lib.rs` to module-declare + re-export:
    ```rust
    //! `rollout-backend-vllm` — vLLM-backed `InferenceBackend` impl via PyO3 in-process.
    //!
    //! See `docs/book/src/inference/vllm-backend.md` for architecture and the
    //! Python-thread / GIL contract.
    mod backend;
    mod engine;
    mod errors;
    mod python_glue;

    pub use backend::VllmBackend;
    ```

    Tests:
    - `crates/rollout-backend-vllm/tests/sampling_params.rs`: postcard determinism + Pitfall-4 equality.
    - `crates/rollout-backend-vllm/tests/backend_stub.rs`: `#[tokio::test]` that builds a `VllmBackend::new("test")`, calls `.generate(...)` and asserts `Err(Fatal(PluginContract { msg })) where msg.contains("Wave 2")`. Compiles both with default features and `--features vllm`.

    Python side (Wave 1 stub; Wave 2 plan 03-03 swaps to real AsyncLLMEngine):
    - `python/rollout/backends/__init__.py`: empty.
    - `python/rollout/backends/vllm/__init__.py`: empty.
    - `python/rollout/backends/vllm/engine.py`:
      ```python
      """Wave-1 stub. Replaced in plan 03-03 with the real AsyncLLMEngine bridge."""
      _engine = None

      def init(model_uri: str, **engine_args) -> None:
          global _engine
          _engine = {"model": model_uri}

      async def generate_one(prompt: str, request_id: str, **sampling) -> dict:
          return {
              "text": f"STUB:{prompt}",
              "finish_reason": "stop",
              "prompt_tokens": 0,
              "completion_tokens": 0,
          }

      def shutdown() -> None:
          global _engine
          _engine = None
      ```

    Create `docs/book/src/inference/vllm-backend.md` (~80 lines): scope, PyO3 thread pattern, vllm feature gate, ROLLOUT_VLLM_AVAILABLE contract, Wave-1 vs Wave-2 split, link to RESEARCH §"Pitfalls". Add the chapter under `# Inference` in `docs/book/src/SUMMARY.md`.
  </action>
  <verify>
    <automated>cargo test -p rollout-backend-vllm --tests &amp;&amp; cargo build -p rollout-backend-vllm --features vllm &amp;&amp; cargo clippy -p rollout-backend-vllm --all-targets -- -D warnings &amp;&amp; python3 -c "import sys; sys.path.insert(0, 'python'); from rollout.backends.vllm import engine; engine.init('m')" &amp;&amp; mdbook build docs/book</automated>
  </verify>
  <acceptance_criteria>
    - `grep -q 'pub struct VllmBackend' crates/rollout-backend-vllm/src/backend.rs`
    - `grep -q 'impl InferenceBackend for VllmBackend' crates/rollout-backend-vllm/src/backend.rs`
    - `grep -q 'rollout-py-vllm-' crates/rollout-backend-vllm/src/engine.rs`
    - `grep -q 'Python::attach' crates/rollout-backend-vllm/src/engine.rs`
    - `grep -q 'streaming generation is Phase 8' crates/rollout-backend-vllm/src/backend.rs`
    - `test -f python/rollout/backends/vllm/engine.py &amp;&amp; grep -q 'async def generate_one' python/rollout/backends/vllm/engine.py`
    - `test -f docs/book/src/inference/vllm-backend.md`
    - `grep -q 'inference/vllm-backend.md' docs/book/src/SUMMARY.md`
    - `cargo test -p rollout-backend-vllm --tests` exits 0
    - `cargo build -p rollout-backend-vllm --features vllm` exits 0
    - `cargo clippy -p rollout-backend-vllm --all-targets -- -D warnings` exits 0
  </acceptance_criteria>
  <done>
    rollout-backend-vllm builds + tests pass under both feature configurations; VllmBackend impls InferenceBackend with the stub generate path; Python module is importable from `python/`; mdBook chapter ships.
  </done>
</task>

</tasks>

<verification>
- `cargo test -p rollout-backend-vllm --tests` clean (default features; Wave-2 will add `vllm`-gated live tests).
- `cargo clippy -p rollout-backend-vllm --all-targets -- -D warnings` clean.
- `mdbook build docs/book` clean.
- DOCS-02: this plan touches `crates/`, `docs/`, and `tests/` — policy satisfied.
</verification>

<success_criteria>
Wave-2 plan 03-03 can drop the real `AsyncLLMEngine.from_engine_args` call into `engine.py::init` and the `engine.generate(...)` async-for loop into `engine.py::generate_one` + the corresponding Rust dispatch into `worker_main`'s `Generate` arm without restructuring the crate.
</success_criteria>

<output>
After completion, create `.planning/phases/03-inference-batch/03-01-rollout-backend-vllm-skeleton-SUMMARY.md` per template.
</output>
