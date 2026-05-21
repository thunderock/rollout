//! Deterministic `MockBackend` for resume integration tests (no live vLLM).
//!
//! Gated by the `test-mock-backend` Cargo feature so it never ships in default
//! builds (AGENTS.md §7 local-test parity — the runtime crate must `cargo test`
//! without Python / vLLM).
//!
//! ## Phase-4 — `TrainableBackend` extension
//!
//! Beyond the Phase-3 `InferenceBackend` impl, `MockBackend` also implements
//! `TrainableBackend` with deterministic SGD over `ndarray::Array1<f32>` fake
//! weights. The `snapshot_resume.rs` byte-compare test (TRAIN-03 LOAD-BEARING)
//! relies on this: two backends initialised with the same seed produce
//! byte-equal weights after K identical `optimizer_step` calls.

use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use ndarray::Array1;
use rollout_core::config::OptimizerSettings;
use rollout_core::{
    Completion, ContentId, CoreError, FatalError, GradHandle, InferenceBackend, LossOutput,
    LossScope, ModelRef, Prompt, SamplingParams, TrainBatch, TrainableBackend,
};

/// Test-only `InferenceBackend` returning `"MOCK:{prompt}"` after an optional sleep.
///
/// Phase 4: also implements `TrainableBackend` with deterministic SGD over an
/// `ndarray::Array1<f32>` fake weights vector. `train_state` is `Some(_)` only
/// when constructed via `MockBackend::new_train` / `new_train_with_weights`;
/// inference-only constructors leave it `None` (Phase-3 path untouched).
pub struct MockBackend {
    model_id: ContentId,
    delay: Duration,
    train_state: Option<TrainState>,
}

impl MockBackend {
    /// Build a `MockBackend` whose `generate()` sleeps `delay_ms` before responding.
    #[must_use]
    pub fn new(delay_ms: u64) -> Self {
        Self {
            model_id: ContentId::of(b"mock"),
            delay: Duration::from_millis(delay_ms),
            train_state: None,
        }
    }

    /// Construct a `MockBackend` pre-loaded for training tests.
    ///
    /// `seed` controls both the initial weight value AND the optimizer-step
    /// delta so identical seeds + identical step counts produce byte-equal
    /// weights.
    #[must_use]
    pub fn new_train(seed: u64) -> Self {
        // Deterministic init only; precision loss is acceptable for a fake backend.
        #[allow(clippy::cast_precision_loss)]
        let init_value = (seed as f32) / 1000.0;
        let weights = Array1::<f32>::from_elem(8, init_value);
        Self::new_train_with_weights(seed, weights)
    }

    /// Internal: construct with explicit initial weights (used by snapshot restore).
    #[must_use]
    pub fn new_train_with_weights(seed: u64, weights: Array1<f32>) -> Self {
        Self {
            model_id: ContentId::of(b"mock"),
            delay: Duration::from_millis(0),
            train_state: Some(TrainState {
                weights: Mutex::new(weights),
                step: Mutex::new(0),
                seed,
            }),
        }
    }

    /// Snapshot the current weights (test helper; not on the trait).
    ///
    /// # Panics
    /// Panics if the backend wasn't constructed via `new_train` /
    /// `new_train_with_weights`, or if the weights mutex is poisoned.
    #[must_use]
    pub fn weights_snapshot(&self) -> Array1<f32> {
        self.train_state
            .as_ref()
            .expect("not in train mode")
            .weights
            .lock()
            .unwrap()
            .clone()
    }

    /// Read the current step counter (test helper).
    ///
    /// # Panics
    /// Panics if the backend wasn't constructed via `new_train` /
    /// `new_train_with_weights`, or if the step mutex is poisoned.
    #[must_use]
    pub fn step(&self) -> u64 {
        *self
            .train_state
            .as_ref()
            .expect("not in train mode")
            .step
            .lock()
            .unwrap()
    }
}

pub(crate) struct TrainState {
    pub(crate) weights: Mutex<Array1<f32>>,
    pub(crate) step: Mutex<u64>,
    pub(crate) seed: u64,
}

#[async_trait]
impl InferenceBackend for MockBackend {
    async fn init(&mut self, _model: &ModelRef) -> Result<(), CoreError> {
        Ok(())
    }

    async fn generate(
        &self,
        prompts: &[Prompt],
        _params: &SamplingParams,
    ) -> Result<Vec<Completion>, CoreError> {
        tokio::time::sleep(self.delay).await;
        Ok(prompts
            .iter()
            .map(|p| Completion {
                text: format!("MOCK:{}", p.0),
                finish_reason: "stop".to_owned(),
                prompt_tokens: 0,
                completion_tokens: u32::try_from(p.0.len()).unwrap_or(u32::MAX),
            })
            .collect())
    }

    fn model_id(&self) -> &ContentId {
        &self.model_id
    }

    async fn shutdown(&mut self) -> Result<(), CoreError> {
        Ok(())
    }
}

#[async_trait]
impl TrainableBackend for MockBackend {
    async fn set_train_mode(&mut self, _enabled: bool) -> Result<(), CoreError> {
        // Idempotent; `MockBackend` is "always in train mode" once `new_train`
        // was called. Returns Ok even when `train_state` is None so production
        // code paths that call `set_train_mode(false)` on a fresh inference
        // backend don't fail.
        Ok(())
    }

    async fn forward_with_loss(
        &self,
        batch: &TrainBatch,
        _: &LossScope,
    ) -> Result<LossOutput, CoreError> {
        let state = self.train_state.as_ref().ok_or_else(not_in_train)?;
        let step = *state.step.lock().unwrap();
        Ok(LossOutput::new(
            0.5,
            GradHandle { step: step + 1 },
            batch.n_tokens,
        ))
    }

    async fn optimizer_step(
        &self,
        grads: GradHandle,
        opt: &OptimizerSettings,
    ) -> Result<(), CoreError> {
        let state = self.train_state.as_ref().ok_or_else(not_in_train)?;
        let mut weights = state.weights.lock().unwrap();
        let mut step = state.step.lock().unwrap();
        // Deterministic SGD: every element gets the same delta = (seed + step) * lr.
        // Precision/truncation losses are deliberate — this is a deterministic
        // fake-weights backend, not a numerically-faithful optimizer.
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
        let delta = (state.seed.wrapping_add(grads.step)) as f32 * opt.lr as f32;
        for w in weights.iter_mut() {
            *w -= delta;
        }
        *step = grads.step;
        Ok(())
    }

    async fn save_weights(&self) -> Result<ContentId, CoreError> {
        let state = self.train_state.as_ref().ok_or_else(not_in_train)?;
        let weights = state.weights.lock().unwrap();
        let bytes = postcard::to_stdvec(&weights.to_vec()).map_err(|e| {
            CoreError::Fatal(FatalError::Internal {
                msg: format!("postcard encode weights: {e}"),
            })
        })?;
        Ok(ContentId::of(&bytes))
    }

    async fn load_weights(&mut self, _weights_id: &ContentId) -> Result<(), CoreError> {
        // No-op for `MockBackend`: `snapshot_resume.rs` restores weights via a
        // direct test helper (`MockBackend::new_train_with_weights`) so the
        // byte-compare assertion is meaningful. Production backends do
        // actual blob loading here.
        Ok(())
    }
}

fn not_in_train() -> CoreError {
    CoreError::Fatal(FatalError::PluginContract {
        plugin: "MockBackend".into(),
        msg: "set_train_mode(true) was not called or MockBackend::new_train wasn't used".into(),
    })
}

#[cfg(test)]
mod train_tests {
    use super::*;
    use rollout_core::config::{LrSchedule, OptimizerKind};
    use rollout_core::LossScope;

    fn settings(lr: f64) -> OptimizerSettings {
        OptimizerSettings {
            kind: OptimizerKind::Sgd,
            lr,
            weight_decay: 0.0,
            betas: [0.9, 0.999],
            eps: 1e-8,
            warmup_steps: 0,
            schedule: LrSchedule::Constant,
        }
    }

    fn batch() -> TrainBatch {
        TrainBatch::with_rows(1, 16, vec!["hi".into()])
    }

    #[tokio::test]
    async fn forward_returns_constant_loss() {
        let mock = MockBackend::new_train(42);
        let b = batch();
        let out = mock
            .forward_with_loss(&b, &LossScope::AssistantOnly)
            .await
            .unwrap();
        assert!((out.loss - 0.5).abs() < f32::EPSILON);
        assert_eq!(out.n_tokens, 16);
        assert_eq!(out.grad_handle.step, 1);
    }

    #[tokio::test]
    async fn optimizer_step_deterministic_with_same_seed() {
        let mut a = MockBackend::new_train(42);
        let mut b = MockBackend::new_train(42);
        let opt = settings(0.01);
        let bt = TrainBatch::with_rows(1, 1, vec!["x".into()]);
        for _ in 0..5 {
            let la = a.forward_with_loss(&bt, &LossScope::Full).await.unwrap();
            let lb = b.forward_with_loss(&bt, &LossScope::Full).await.unwrap();
            a.optimizer_step(la.grad_handle, &opt).await.unwrap();
            b.optimizer_step(lb.grad_handle, &opt).await.unwrap();
        }
        assert_eq!(a.weights_snapshot(), b.weights_snapshot());
    }

    #[tokio::test]
    async fn save_load_weights_round_trip() {
        let mut mock = MockBackend::new_train(7);
        let id1 = mock.save_weights().await.unwrap();
        mock.load_weights(&id1).await.unwrap();
        let id2 = mock.save_weights().await.unwrap();
        assert_eq!(id1, id2);
    }

    #[tokio::test]
    async fn set_train_mode_is_idempotent() {
        let mut mock = MockBackend::new_train(1);
        mock.set_train_mode(true).await.unwrap();
        mock.set_train_mode(true).await.unwrap();
        mock.set_train_mode(false).await.unwrap();
    }
}
