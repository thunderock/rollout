//! macOS dev-stub witness (D-TOOL-05). On Linux this whole file compiles out
//! (the real enforcement + positive witnesses live in `sandbox_positive.rs`).
#![cfg(not(target_os = "linux"))]

mod support;

use rollout_core::traits::harness::{ToolCall, ToolCallId, ToolContext, ToolHarness, ToolOutcome};
use rollout_core::WorkerId;
use rollout_harness_tool::{ToolHarnessImpl, ToolSettings};
use ulid::Ulid;

#[tokio::test]
async fn macos_stub_returns_documented_fatal() {
    let harness = ToolHarnessImpl::from_settings(ToolSettings::default(), support::deps_noop())
        .expect("construction succeeds on macOS (kernel gate stubbed)");
    let call = ToolCall {
        call_id: ToolCallId(Ulid::new()),
        tool: "python_exec".into(),
        args: serde_json::json!({ "code": "print(1)" }),
        context: ToolContext {
            worker_id: WorkerId(Ulid::new()),
            episode_id: None,
        },
    };
    let results = harness.invoke(vec![call]).await.expect("invoke returns Ok");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].outcome, ToolOutcome::Error);
    assert_eq!(
        results[0].stderr.as_deref(),
        Some("sandbox unavailable on macOS — dev stub"),
        "stub returns the exact documented Fatal string"
    );
}
