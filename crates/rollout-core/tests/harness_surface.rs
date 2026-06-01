//! Spec-07 (D-CORE-01) harness trait-surface witnesses: batched signatures,
//! defaulted `snapshot_episode`, `JsonSchema` on descriptor/report types, and
//! `HarnessDependencies` construction from the six substrate handles.

#![allow(dead_code)]

use std::time::Duration;

use async_trait::async_trait;
use rollout_core::{
    Action, EnvHarness, Episode, EpisodeId, EpisodeStep, EvalContext, EvalDescriptor, EvalReport,
    HarnessDependencies, MetricSpec, MetricValue, ModelRef, Observation, ResourceEstimate, Reward,
    SideEffectClass, StepResult, TaskResult, ToolCall, ToolCallId, ToolContext, ToolDescriptor,
    ToolHarness, ToolOutcome, ToolResult, ToolSpec,
};
use rollout_core::{CoreError, Snapshot};
use ulid::Ulid;

// A trait-object-free EnvHarness impl proves the batched `step` signature
// compiles and `snapshot_episode` has a callable default body.
struct CannedEnv;

#[async_trait]
impl EnvHarness for CannedEnv {
    type Settings = ();

    fn from_settings(_settings: (), _deps: HarnessDependencies) -> Result<Self, CoreError> {
        Ok(Self)
    }

    async fn reset(&self, prompts: Vec<rollout_core::Prompt>) -> Result<Vec<Episode>, CoreError> {
        Ok(prompts
            .into_iter()
            .map(|p| Episode {
                id: EpisodeId(Ulid::new()),
                observation: Observation(p.0),
                info: serde_json::Value::Null,
            })
            .collect())
    }

    async fn step(&self, batch: Vec<EpisodeStep>) -> Result<Vec<StepResult>, CoreError> {
        Ok(batch
            .into_iter()
            .map(|s| StepResult {
                episode_id: s.episode_id,
                observation: Observation(s.action.0),
                reward: Some(Reward(1.0)),
                done: true,
                info: serde_json::Value::Null,
            })
            .collect())
    }

    async fn close(&self, _episode_ids: Vec<EpisodeId>) -> Result<(), CoreError> {
        Ok(())
    }
    // snapshot_episode intentionally NOT overridden — exercises the v1.1 default.
}

#[tokio::test]
async fn env_harness_step_is_batched_and_snapshot_defaults_to_none() {
    let env = CannedEnv;
    let eps = env
        .reset(vec![rollout_core::Prompt("hi".into())])
        .await
        .unwrap();
    let steps: Vec<EpisodeStep> = eps
        .iter()
        .map(|e| EpisodeStep {
            episode_id: e.id,
            action: Action("a".into()),
        })
        .collect();
    let results: Vec<StepResult> = env.step(steps).await.unwrap();
    assert_eq!(results.len(), 1);
    // Default snapshot_episode returns Ok(None) without an override.
    let snap: Option<Snapshot> = env.snapshot_episode(eps[0].id).await.unwrap();
    assert!(snap.is_none());
}

#[test]
fn descriptor_and_report_types_derive_json_schema() {
    // Compile-time proof every descriptor/report type derives JsonSchema.
    let _ = schemars::schema_for!(ToolDescriptor);
    let _ = schemars::schema_for!(ToolSpec);
    let _ = schemars::schema_for!(EvalDescriptor);
    let _ = schemars::schema_for!(EvalReport);
    let _ = schemars::schema_for!(MetricSpec);
    let _ = schemars::schema_for!(MetricValue);
    let _ = schemars::schema_for!(ResourceEstimate);
    let _ = schemars::schema_for!(TaskResult);
    let _ = schemars::schema_for!(StepResult);
    let _ = schemars::schema_for!(Episode);
}

#[test]
fn tool_call_carries_typed_calls_not_raw_bytes() {
    // ToolHarness::invoke takes Vec<ToolCall>, not &[u8]. Compile-only shape check.
    fn _shape<T: ToolHarness>(
        h: &T,
        calls: Vec<ToolCall>,
    ) -> impl std::future::Future<Output = Result<Vec<ToolResult>, CoreError>> + '_ {
        h.invoke(calls)
    }
    let _spec = ToolSpec {
        name: "python_exec".into(),
        description: "run python".into(),
        input_schema: serde_json::json!({}),
        side_effects: SideEffectClass::Exec,
        timeout: Duration::from_secs(5),
    };
    let _call = ToolCall {
        call_id: ToolCallId(Ulid::new()),
        tool: "python_exec".into(),
        args: serde_json::json!({}),
        context: ToolContext {
            worker_id: rollout_core::WorkerId(Ulid::new()),
            episode_id: None,
        },
    };
    let _res = ToolResult {
        call_id: ToolCallId(Ulid::new()),
        outcome: ToolOutcome::Success,
        output: serde_json::Value::Null,
        stderr: None,
        duration: Duration::from_millis(1),
    };
}

#[test]
fn eval_context_and_metric_value_construct() {
    let _ctx = EvalContext {
        sampling: rollout_core::SamplingParams::default(),
        seed: 0,
    };
    assert!(matches!(MetricValue::Scalar(1.0), MetricValue::Scalar(_)));
    let _m = ModelRef {
        uri: "x".into(),
        content_id: None,
        tokenizer: None,
    };
}
