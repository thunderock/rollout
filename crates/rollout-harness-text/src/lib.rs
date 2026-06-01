//! `rollout-harness-text` (HARNESS-01) — text-completion [`EnvHarness`].
//!
//! `Observation = prompt`, `Action = completion`. Batched `reset`/`step`/`close`
//! over an in-memory episode store; reward is computed via the plugin host
//! (D-ENV-03), never a built-in reward trait. The step loop is multi-turn
//! capable (D-ENV-01): each episode carries a `max_turns` budget and a turn
//! counter, so v1.2 conversational envs need no contract change. No trajectory
//! persistence to `ObjectStore` (D-ENV-02) — `StepResult`s stay in-memory.
//!
//! Witnesses (all GPU-free, cloud-free): `EchoEnv` (canned reward),
//! `MockRewardEnv` (plugin-host reward path), `env_deterministic_replay`
//! (same seed → same trajectory).
#![forbid(unsafe_code)]

pub mod episode;
pub mod reward;

use std::sync::Arc;

use async_trait::async_trait;
use rollout_core::{
    Action, CoreError, EnvHarness, Episode, EpisodeId, EpisodeStep, HarnessDependencies,
    Observation, PluginHandle, Prompt, Reward, StepResult,
};
use schemars::JsonSchema;
use serde::Deserialize;
use ulid::Ulid;

use crate::episode::{EpisodeState, EpisodeStore, SplitMix64};

/// Settings for [`TextCompletionEnv`], deserialized from TOML.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct TextEnvSettings {
    /// Per-episode turn budget (D-ENV-01 multi-turn). `done` once turn reaches it.
    pub max_turns: u32,
    /// Canned reward returned when no reward plugin is configured (`EchoEnv` path).
    pub echo_reward: Option<f32>,
    /// Deterministic seed for per-episode RNG (D-ENV-03). `None` ⇒ seed 0.
    pub seed: Option<u64>,
}

impl Default for TextEnvSettings {
    fn default() -> Self {
        Self {
            max_turns: 1,
            echo_reward: None,
            seed: None,
        }
    }
}

/// Text-completion environment implementing the spec-07 [`EnvHarness`] surface.
///
/// Reward source: a configured `reward_handle` invokes the plugin host
/// (D-ENV-03); otherwise the canned `echo_reward` is used (`EchoEnv`).
pub struct TextCompletionEnv {
    settings: TextEnvSettings,
    deps: HarnessDependencies,
    /// Reward plugin handle; `None` ⇒ canned `echo_reward` path (`EchoEnv`).
    reward_handle: Option<PluginHandle>,
    store: EpisodeStore,
}

impl TextCompletionEnv {
    /// Construct an env that computes reward via the given plugin handle.
    #[must_use]
    pub fn with_reward_plugin(
        settings: TextEnvSettings,
        deps: HarnessDependencies,
        reward_handle: PluginHandle,
    ) -> Self {
        Self {
            settings,
            deps,
            reward_handle: Some(reward_handle),
            store: EpisodeStore::default(),
        }
    }

    /// Seed for episode index `idx` (seed XOR index — the v1.0 pattern).
    fn episode_seed(&self, idx: u64) -> u64 {
        self.settings.seed.unwrap_or(0) ^ idx
    }
}

/// `EchoEnv`: a [`TextCompletionEnv`] with a canned reward and no reward plugin.
///
/// Convenience constructor for the canned-reward witness.
#[must_use]
pub fn echo_env(deps: HarnessDependencies, echo_reward: f32, max_turns: u32) -> TextCompletionEnv {
    TextCompletionEnv {
        settings: TextEnvSettings {
            max_turns,
            echo_reward: Some(echo_reward),
            seed: None,
        },
        deps,
        reward_handle: None,
        store: EpisodeStore::default(),
    }
}

#[async_trait]
impl EnvHarness for TextCompletionEnv {
    type Settings = TextEnvSettings;

    fn from_settings(settings: Self::Settings, deps: HarnessDependencies) -> Result<Self, CoreError>
    where
        Self: Sized,
    {
        Ok(Self {
            settings,
            deps,
            reward_handle: None,
            store: EpisodeStore::default(),
        })
    }

    async fn reset(&self, prompts: Vec<Prompt>) -> Result<Vec<Episode>, CoreError> {
        let mut out = Vec::with_capacity(prompts.len());
        for (idx, Prompt(text)) in prompts.into_iter().enumerate() {
            let id = EpisodeId(Ulid::new());
            let seed = self.episode_seed(idx as u64);
            self.store
                .insert(
                    id,
                    EpisodeState {
                        prompt: text.clone(),
                        turn: 0,
                        max_turns: self.settings.max_turns,
                        rng: SplitMix64::new(seed),
                    },
                )
                .await;
            // Observation echoes the prompt (v1.1 text-in/text-out).
            out.push(Episode {
                id,
                observation: Observation(text),
                info: serde_json::json!({}),
            });
        }
        Ok(out)
    }

    async fn step(&self, batch: Vec<EpisodeStep>) -> Result<Vec<StepResult>, CoreError> {
        let mut out = Vec::with_capacity(batch.len());
        for EpisodeStep {
            episode_id,
            action: Action(action_text),
        } in batch
        {
            // Snapshot the per-step state under the lock; missing/closed → error entry.
            let snap = self
                .store
                .with_episode(episode_id, |state| {
                    state.turn += 1;
                    // Draw from the seeded RNG so a fixed seed reproduces the run.
                    let nonce = state.rng.next_u64();
                    (state.turn, state.max_turns, state.prompt.clone(), nonce)
                })
                .await;

            let Some((turn, max_turns, prompt, _nonce)) = snap else {
                // Per-episode error isolation (spec §7): other episodes unaffected.
                out.push(StepResult {
                    episode_id,
                    observation: Observation(String::new()),
                    reward: None,
                    done: true,
                    info: serde_json::json!({ "error": "unknown or closed episode" }),
                });
                continue;
            };

            let reward = self
                .compute_reward(&prompt, &action_text)
                .await?
                .map(Reward);

            out.push(StepResult {
                episode_id,
                // EchoEnv next observation echoes the action text.
                observation: Observation(action_text),
                reward,
                done: turn >= max_turns,
                info: serde_json::json!({ "turn": turn }),
            });
        }
        Ok(out)
    }

    async fn close(&self, episode_ids: Vec<EpisodeId>) -> Result<(), CoreError> {
        for id in episode_ids {
            self.store.remove(id).await;
        }
        Ok(())
    }
}

impl TextCompletionEnv {
    /// Compute the reward for one transition.
    ///
    /// Plugin path (D-ENV-03) when a `reward_handle` is configured; otherwise
    /// the canned `echo_reward`. Returns `None` when reward is deferred.
    ///
    /// # Errors
    /// Propagates plugin-host / decode failures as [`CoreError`].
    async fn compute_reward(
        &self,
        prompt: &str,
        completion: &str,
    ) -> Result<Option<f32>, CoreError> {
        if let Some(handle) = &self.reward_handle {
            let r = reward::compute_reward(&self.deps, handle, prompt, completion).await?;
            return Ok(Some(r.0));
        }
        Ok(self.settings.echo_reward)
    }
}

/// Re-export `Arc` for downstream `HarnessDependencies` wiring convenience.
#[doc(hidden)]
pub type SharedDeps = Arc<HarnessDependencies>;
