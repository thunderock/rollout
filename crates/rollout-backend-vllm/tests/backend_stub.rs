//! `VllmBackend` Wave-2 skeleton contract.
//!
//! Verifies: (a) Send + Sync auto-trait bounds hold, (b) `generate` returns a
//! typed `Fatal(PluginContract { … "Wave 2" … })` until plan 03-03 wires the
//! real `AsyncLLMEngine`, (c) `model_id()` returns a stable handle.

use rollout_backend_vllm::VllmBackend;
use rollout_core::{CoreError, FatalError, InferenceBackend, Prompt, SamplingParams};

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn vllm_backend_is_send_sync() {
    assert_send_sync::<VllmBackend>();
}

#[tokio::test]
async fn generate_returns_wave2_stub_error() {
    let backend = VllmBackend::new("test-engine").expect("construct VllmBackend");
    let prompts = [Prompt("hi".into())];
    let params = SamplingParams::default();
    let res = backend.generate(&prompts, &params).await;
    let err = res.expect_err("Wave-2 stub must error");
    match err {
        CoreError::Fatal(FatalError::PluginContract { msg, .. }) => {
            assert!(
                msg.contains("Wave 2"),
                "expected Wave-2 sentinel in plugin-contract message, got: {msg}"
            );
        }
        other => panic!("expected Fatal(PluginContract), got: {other:?}"),
    }
}

#[tokio::test]
async fn model_id_is_stable_before_init() {
    let backend = VllmBackend::new("test-engine").expect("construct VllmBackend");
    let a = *backend.model_id();
    let b = *backend.model_id();
    assert_eq!(a, b, "model_id() must be stable across calls");
}
