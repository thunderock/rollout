//! `MockRewardEnv` witness: the plugin-host reward path (D-ENV-03). A
//! deterministic mock `PluginHost` answers `call(handle, "score", ..)`; a decode
//! failure surfaces as `Fatal(PluginContract)`, not a panic.

mod support;

use std::sync::Arc;

use rollout_core::{Action, CoreError, EnvHarness, EpisodeStep, FatalError, Prompt};
use rollout_harness_text::reward::{compute_reward, RewardInput};
use rollout_harness_text::{TextCompletionEnv, TextEnvSettings};

use support::{deps_with_host, reward_handle, MockRewardHost};

#[tokio::test]
async fn mock_reward_env_uses_plugin_host_reward() {
    let host = Arc::new(MockRewardHost {
        reward: 0.875,
        corrupt: false,
    });
    let deps = deps_with_host(host);
    let env = TextCompletionEnv::with_reward_plugin(
        TextEnvSettings {
            max_turns: 1,
            echo_reward: None,
            seed: Some(7),
        },
        deps,
        reward_handle(),
    );

    let episodes = env.reset(vec![Prompt("q".into())]).await.unwrap();
    let results = env
        .step(vec![EpisodeStep {
            episode_id: episodes[0].id,
            action: Action("a".into()),
        }])
        .await
        .unwrap();

    let reward = results[0].reward.expect("plugin reward present");
    assert!(
        (reward.0 - 0.875).abs() < f32::EPSILON,
        "reward equals the mock plugin output, got {}",
        reward.0
    );
}

#[tokio::test]
async fn mock_reward_env_decode_failure_is_typed_fatal() {
    let host = Arc::new(MockRewardHost {
        reward: 0.0,
        corrupt: true,
    });
    let deps = deps_with_host(host);

    // Exercise the reward path directly: corrupt bytes → Fatal(PluginContract).
    let err = compute_reward(&deps, &reward_handle(), "p", "c")
        .await
        .expect_err("corrupt plugin bytes must error");
    match err {
        CoreError::Fatal(FatalError::PluginContract { plugin, msg }) => {
            assert_eq!(plugin, "mock-reward");
            assert!(msg.contains("decode Reward"), "got: {msg}");
        }
        other => panic!("expected Fatal(PluginContract), got {other:?}"),
    }
}

#[test]
fn reward_input_postcard_round_trips() {
    let input = RewardInput {
        prompt: "hello".into(),
        completion: "world".into(),
    };
    let bytes = postcard::to_stdvec(&input).unwrap();
    let back: RewardInput = postcard::from_bytes(&bytes).unwrap();
    assert_eq!(input, back);
}
