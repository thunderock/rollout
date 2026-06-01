//! `EchoEnv` witness: batched reset/step/close, multi-turn, per-episode error
//! isolation — all GPU-free, no plugin host.

mod support;

use rollout_core::{Action, EnvHarness, EpisodeStep, Prompt};
use rollout_harness_text::echo_env;

#[tokio::test]
async fn echo_env_reset_returns_one_episode_per_prompt() {
    let env = echo_env(support::deps_noop(), 1.0, 1);
    let episodes = env
        .reset(vec![Prompt("hi".into()), Prompt("yo".into())])
        .await
        .unwrap();

    assert_eq!(episodes.len(), 2);
    assert_ne!(episodes[0].id, episodes[1].id, "distinct episode ids");
    assert_eq!(episodes[0].observation.0, "hi", "observation echoes prompt");
    assert_eq!(episodes[1].observation.0, "yo");
}

#[tokio::test]
async fn echo_env_step_returns_canned_reward_in_input_order() {
    let env = echo_env(support::deps_noop(), 0.5, 1);
    let episodes = env
        .reset(vec![Prompt("a".into()), Prompt("b".into())])
        .await
        .unwrap();

    let batch = vec![
        EpisodeStep {
            episode_id: episodes[0].id,
            action: Action("x".into()),
        },
        EpisodeStep {
            episode_id: episodes[1].id,
            action: Action("y".into()),
        },
    ];
    let results = env.step(batch).await.unwrap();

    assert_eq!(results.len(), 2);
    assert_eq!(
        results[0].episode_id, episodes[0].id,
        "input order preserved"
    );
    assert_eq!(results[0].observation.0, "x", "observation echoes action");
    assert!(
        (results[0].reward.unwrap().0 - 0.5).abs() < f32::EPSILON,
        "canned reward"
    );
    assert!(results[0].done, "done at the 1-turn budget");
    assert_eq!(results[1].observation.0, "y");
}

#[tokio::test]
async fn echo_env_is_multi_turn_capable() {
    // D-ENV-01: stepping the same episode N times before done works.
    let env = echo_env(support::deps_noop(), 1.0, 3);
    let episodes = env.reset(vec![Prompt("p".into())]).await.unwrap();
    let id = episodes[0].id;

    for expected_turn in 1u32..=3 {
        let r = env
            .step(vec![EpisodeStep {
                episode_id: id,
                action: Action(format!("turn-{expected_turn}")),
            }])
            .await
            .unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].done, expected_turn >= 3, "done only at the budget");
    }
}

#[tokio::test]
async fn echo_env_close_then_step_isolates_per_episode_error() {
    let env = echo_env(support::deps_noop(), 1.0, 5);
    let episodes = env
        .reset(vec![Prompt("live".into()), Prompt("dead".into())])
        .await
        .unwrap();
    let (live, dead) = (episodes[0].id, episodes[1].id);

    env.close(vec![dead]).await.unwrap();

    let results = env
        .step(vec![
            EpisodeStep {
                episode_id: dead,
                action: Action("z".into()),
            },
            EpisodeStep {
                episode_id: live,
                action: Action("ok".into()),
            },
        ])
        .await
        .unwrap();

    // Closed episode → error entry, done; the other episode succeeds (spec §7).
    assert_eq!(results.len(), 2);
    assert!(results[0].done);
    assert!(results[0].info.get("error").is_some(), "closed id flagged");
    assert!(results[0].reward.is_none());
    assert_eq!(results[1].observation.0, "ok");
    assert!(results[1].reward.is_some());
}
