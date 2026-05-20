//! `VllmBackend` — Phase-3 `InferenceBackend` impl over a dedicated Python thread.
//!
//! Wave-2 ships the dispatch shape (`PyO3` thread bootstrap, `mpsc<VllmTask>`
//! hop, per-prompt one-shot reply). `generate` returns a typed
//! `Fatal(PluginContract { … "Wave 2" … })` until plan 03-03 swaps the worker's
//! `Generate` arm for the live `AsyncLLMEngine` bridge. The architecture is
//! identical to plan 02-05's `Pyo3State`, with `rollout-py-vllm-<engine_id>`
//! as the thread name.

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
    /// model URI; Wave-3 (plan 03-03) replaces with the resolved `HuggingFace`
    /// repo SHA so re-runs against the same model share a `ContentId`.
    model_id: ContentId,
}

impl VllmBackend {
    /// Construct a `VllmBackend` and spawn its dedicated Python thread.
    ///
    /// `engine_id` names the OS thread (`rollout-py-vllm-<engine_id>`) and
    /// seeds the pre-init `model_id`. Pass a ULID or run-scoped identifier.
    ///
    /// # Errors
    ///
    /// Returns `Fatal(Internal)` if the OS refuses to spawn the dedicated
    /// Python thread.
    pub fn new(engine_id: &str) -> Result<Self, CoreError> {
        // RESEARCH Pitfall 10 deferred: secret_token wiring will land in plan
        // 03-03 alongside the EnvSecretStore consumer; Wave-2 spawns the
        // thread without a token.
        let engine = VllmEngine::spawn(engine_id, None)?;
        Ok(Self {
            engine,
            model_id: ContentId::of(engine_id.as_bytes()),
        })
    }
}

#[async_trait]
impl InferenceBackend for VllmBackend {
    async fn init(&mut self, model: &ModelRef) -> Result<(), CoreError> {
        // Wave-3 will resolve `model.content_id` from the HuggingFace SHA;
        // for now derive deterministically from the URI so two `VllmBackend`s
        // loading the same model share a `model_id`.
        self.model_id = ContentId::of(model.uri.as_bytes());
        let (reply_tx, reply_rx) = oneshot::channel();
        self.engine
            .tx
            .send(VllmTask::Init {
                model: model.clone(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| transient("vllm engine thread closed"))?;
        reply_rx
            .await
            .map_err(|_| transient("vllm init reply dropped"))?
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
        // AGENTS.md principle #2: one `generate` per prompt; vLLM's continuous
        // batcher handles cross-prompt batching (RESEARCH Pattern 1).
        let mut out = Vec::with_capacity(prompts.len());
        for (i, p) in prompts.iter().enumerate() {
            let (reply_tx, reply_rx) = oneshot::channel();
            self.engine
                .tx
                .send(VllmTask::Generate {
                    prompt: p.0.clone(),
                    params: params.clone(),
                    request_id: format!("req-{i}"),
                    reply: reply_tx,
                })
                .await
                .map_err(|_| transient("vllm engine thread closed"))?;
            let completion = reply_rx
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
