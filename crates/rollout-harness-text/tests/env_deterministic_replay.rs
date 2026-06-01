//! `env_deterministic_replay` witness (D-ENV-03): the same seed over the same
//! prompts + actions yields a byte-identical trajectory; a different seed
//! diverges. Episode ids are minted fresh per reset (ULID), so the trajectory
//! comparison normalizes them out — determinism is over observations, rewards,
//! done flags, and the seeded `info.nonce`.

mod support;

use rollout_core::{Action, EnvHarness, EpisodeStep, Prompt};
use rollout_harness_text::{TextCompletionEnv, TextEnvSettings};

/// Run a fixed prompts+actions schedule and serialize the seed-dependent part
/// of each `StepResult` (everything but the non-deterministic episode id).
async fn run_trajectory(seed: u64) -> Vec<u8> {
    let settings = TextEnvSettings {
        max_turns: 3,
        echo_reward: Some(0.25),
        seed: Some(seed),
    };
    let env = TextCompletionEnv::from_settings(settings, support::deps_noop()).unwrap();

    let prompts = vec![Prompt("alpha".into()), Prompt("beta".into())];
    let episodes = env.reset(prompts).await.unwrap();
    let ids: Vec<_> = episodes.iter().map(|e| e.id).collect();

    let mut normalized = Vec::new();
    for turn in 0..3 {
        let batch: Vec<EpisodeStep> = ids
            .iter()
            .map(|&episode_id| EpisodeStep {
                episode_id,
                action: Action(format!("act-{turn}")),
            })
            .collect();
        for r in env.step(batch).await.unwrap() {
            // Drop episode_id (fresh ULID per run); keep the seed-dependent fields.
            let tuple = (r.observation, r.reward, r.done, r.info);
            normalized.extend(serde_json::to_vec(&tuple).unwrap());
        }
    }
    normalized
}

#[tokio::test]
async fn env_deterministic_replay() {
    let a = run_trajectory(42).await;
    let b = run_trajectory(42).await;
    assert_eq!(a, b, "same seed → byte-identical trajectory");

    let c = run_trajectory(43).await;
    assert_ne!(a, c, "different seed → divergent trajectory");
}
