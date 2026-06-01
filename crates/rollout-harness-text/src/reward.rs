//! Plugin-host reward path (D-ENV-03).
//!
//! Reward is a user-supplied plugin, not a built-in trait. On `step`, the env
//! encodes a [`RewardInput`] with postcard, calls `plugin_host.call(handle,
//! "score", payload)`, and decodes a [`Reward`]. A decode failure surfaces as a
//! typed `Fatal(PluginContract { .. })`, never a panic.

use rollout_core::{CoreError, FatalError, HarnessDependencies, PluginHandle, Reward};
use serde::{Deserialize, Serialize};

/// Postcard wire contract handed to the reward plugin's `"score"` method.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RewardInput {
    /// Originating prompt text.
    pub prompt: String,
    /// Policy completion / action text.
    pub completion: String,
}

/// Invoke the reward plugin and decode its scalar reward.
///
/// # Errors
/// Returns [`CoreError`] when encoding fails, the plugin host call fails, or the
/// returned bytes violate the postcard `Reward` contract (mapped to a typed
/// `Fatal(PluginContract)`).
pub async fn compute_reward(
    deps: &HarnessDependencies,
    handle: &PluginHandle,
    prompt: &str,
    completion: &str,
) -> Result<Reward, CoreError> {
    let input = RewardInput {
        prompt: prompt.to_owned(),
        completion: completion.to_owned(),
    };
    let payload = postcard::to_stdvec(&input).map_err(|e| {
        CoreError::Fatal(FatalError::PluginContract {
            plugin: handle.manifest.name.clone(),
            msg: format!("encode RewardInput: {e}"),
        })
    })?;
    let bytes = deps.plugin_host.call(handle, "score", payload).await?;
    let reward: Reward = postcard::from_bytes(&bytes).map_err(|e| {
        CoreError::Fatal(FatalError::PluginContract {
            plugin: handle.manifest.name.clone(),
            msg: format!("decode Reward: {e}"),
        })
    })?;
    Ok(reward)
}
