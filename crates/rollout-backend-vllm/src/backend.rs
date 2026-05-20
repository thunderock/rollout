//! `VllmBackend` — Phase-3 `InferenceBackend` impl over a dedicated Python thread.
//!
//! Wave-3 (plan 03-03) wires the live `AsyncLLMEngine` via the
//! `py.detach(|| rt.block_on(into_future(coro)))` bridge — see
//! `engine::worker_main_vllm`. Default features keep the Wave-2 stub worker so
//! `cargo test -p rollout-backend-vllm` runs without `pip install vllm`
//! (AGENTS.md §7).

use async_trait::async_trait;
use rollout_core::{
    Completion, ContentId, CoreError, InferenceBackend, ModelRef, Prompt, SamplingParams,
};
use tokio::sync::oneshot;

use crate::engine::{VllmEngine, VllmTask};
use crate::errors::{streaming_rejected, transient};

/// vLLM-backed `InferenceBackend` (`PyO3` in-process, dedicated Python thread).
pub struct VllmBackend {
    engine: VllmEngine,
    /// Stable per-instance handle. `init` re-derives this from the resolved
    /// model SHA (returned by the Python-side `init` — `HuggingFace` repo SHA
    /// when reachable, model URI as a local-path fallback). The Wave-2 stub
    /// worker echoes the URI back; the `vllm`-feature worker returns the
    /// `huggingface_hub`-resolved SHA per RESEARCH "Re-deriving `model_content_id`".
    model_id: ContentId,
}

impl VllmBackend {
    /// Construct a `VllmBackend` without a secret token.
    ///
    /// `engine_id` names the OS thread (`rollout-py-vllm-<engine_id>`) and
    /// seeds the pre-init `model_id`. Pass a ULID or run-scoped identifier.
    ///
    /// # Errors
    ///
    /// Returns `Fatal(Internal)` if the OS refuses to spawn the dedicated
    /// Python thread.
    pub fn new(engine_id: &str) -> Result<Self, CoreError> {
        Self::with_secret_token(engine_id, None)
    }

    /// Construct a `VllmBackend` and thread a `HuggingFace` `HF_TOKEN` to the
    /// Python worker BEFORE it imports `vllm` (RESEARCH Pitfall 10).
    ///
    /// `secret_token` should be sourced from `EnvSecretStore`'s
    /// `ROLLOUT_SECRET_HF_TOKEN` allowlist entry. The token is owned by the
    /// worker thread for its lifetime; the caller does not need to keep it.
    ///
    /// # Errors
    ///
    /// Returns `Fatal(Internal)` if the OS refuses to spawn the dedicated
    /// Python thread.
    pub fn with_secret_token(
        engine_id: &str,
        secret_token: Option<String>,
    ) -> Result<Self, CoreError> {
        let engine = VllmEngine::spawn(engine_id, secret_token)?;
        Ok(Self {
            engine,
            model_id: ContentId::of(engine_id.as_bytes()),
        })
    }
}

#[async_trait]
impl InferenceBackend for VllmBackend {
    async fn init(&mut self, model: &ModelRef) -> Result<(), CoreError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.engine
            .tx
            .send(VllmTask::Init {
                model: model.clone(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| transient("vllm engine thread closed"))?;
        let sha = reply_rx
            .await
            .map_err(|_| transient("vllm init reply dropped"))??;
        // Content-addressed model_id from the resolved SHA (or URI fallback);
        // ContentId::of blake3s the input so the resulting digest is stable
        // and collision-resistant across runs of the same model.
        self.model_id = ContentId::of(sha.as_bytes());
        Ok(())
    }

    async fn generate(
        &self,
        prompts: &[Prompt],
        params: &SamplingParams,
    ) -> Result<Vec<Completion>, CoreError> {
        // D-BACKEND-03: streaming is Phase 8 (INFER-01); reject at the boundary.
        if params.stream {
            return Err(streaming_rejected());
        }
        // AGENTS.md principle #2: dispatch every prompt concurrently so vLLM's
        // continuous batcher sees them all in one scheduling window. The
        // worker thread serializes Python-side calls anyway (single GIL); the
        // gain is in feeding vLLM's batcher, not in CPU-side parallelism.
        // RESEARCH Pitfall 6: `request_id = format!("req-{i}-0")` — vLLM uses
        // it as a primary scheduler key. A future sample-id-based request_id
        // is deferred to Phase 4 callers that own sample IDs.
        let mut handles = Vec::with_capacity(prompts.len());
        for (i, p) in prompts.iter().enumerate() {
            let (reply_tx, reply_rx) = oneshot::channel();
            self.engine
                .tx
                .send(VllmTask::Generate {
                    prompt: p.0.clone(),
                    params: params.clone(),
                    request_id: format!("req-{i}-0"),
                    reply: reply_tx,
                })
                .await
                .map_err(|_| transient("vllm engine thread closed"))?;
            handles.push(reply_rx);
        }
        let mut out = Vec::with_capacity(handles.len());
        for rx in handles {
            let completion = rx
                .await
                .map_err(|_| transient("vllm generate reply dropped"))??;
            out.push(completion);
        }
        Ok(out)
    }

    fn model_id(&self) -> &ContentId {
        &self.model_id
    }

    async fn shutdown(&mut self) -> Result<(), CoreError> {
        let _ = self.engine.tx.send(VllmTask::Shutdown).await;
        Ok(())
    }
}
