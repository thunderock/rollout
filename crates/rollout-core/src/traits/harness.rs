//! `EnvHarness`, `ToolHarness`, `EvalHarness`, `RewardModel`.

use async_trait::async_trait;

use crate::CoreError;

/// Wraps an environment that produces observations and consumes actions.
#[async_trait]
pub trait EnvHarness: Send + Sync {
    /// Reset the environment to an initial state.
    async fn reset(&mut self) -> Result<(), CoreError>;
}

/// Wraps a sandboxed tool / action surface (shell, code-exec, etc.).
#[async_trait]
pub trait ToolHarness: Send + Sync {
    /// Invoke the tool with raw bytes; returns raw bytes.
    async fn invoke(&self, payload: &[u8]) -> Result<Vec<u8>, CoreError>;
}

/// Wraps an evaluation suite (MMLU, `IFEval`, GSM8K, ...).
#[async_trait]
pub trait EvalHarness: Send + Sync {
    /// Run the evaluation and return a scalar score.
    async fn evaluate(&self) -> Result<f64, CoreError>;
}

/// A reward model — scores trajectories or generations.
#[async_trait]
pub trait RewardModel: Send + Sync {
    /// Score a single sample's bytes.
    async fn score(&self, sample: &[u8]) -> Result<f64, CoreError>;
}
