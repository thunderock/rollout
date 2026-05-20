//! Live-vLLM init smoke (gated by `ROLLOUT_VLLM_AVAILABLE=1`).
//!
//! Asserts `VllmBackend::init` succeeds on a real `AsyncLLMEngine` and that
//! the resulting `model_id` is stable across two `init()` calls (the
//! `huggingface_hub` SHA round-trip is content-addressed, so two inits of the
//! same URI must produce the same `ContentId`).
//!
//! Uses `facebook/opt-125m` — smaller than Qwen2.5-0.5B-Instruct, faster on
//! CPU CI. The `#[ignore]` keeps default `cargo test` clean; run with
//! `cargo test -p rollout-backend-vllm --features vllm --test vllm_init -- --include-ignored`
//! on a host with `pip install vllm` + `ROLLOUT_VLLM_AVAILABLE=1`.

#![cfg(feature = "vllm")]

use rollout_backend_vllm::VllmBackend;
use rollout_core::{InferenceBackend, ModelRef};

#[tokio::test]
#[ignore = "requires `pip install vllm` + ROLLOUT_VLLM_AVAILABLE=1 (RESEARCH Pitfall 3 — no macOS wheels)"]
async fn init_then_model_id_is_stable() {
    if std::env::var("ROLLOUT_VLLM_AVAILABLE").as_deref() != Ok("1") {
        eprintln!("ROLLOUT_VLLM_AVAILABLE != 1; skipping live vLLM init test");
        return;
    }
    let model = ModelRef {
        uri: "facebook/opt-125m".to_owned(),
        content_id: None,
        tokenizer: None,
    };

    let mut backend = VllmBackend::new("vllm-init-test").expect("construct VllmBackend");
    let timeout = std::time::Duration::from_secs(120);
    tokio::time::timeout(timeout, backend.init(&model))
        .await
        .expect("init within 120s")
        .expect("init Ok");
    let first = *backend.model_id();

    let mut backend2 = VllmBackend::new("vllm-init-test-2").expect("construct VllmBackend 2");
    tokio::time::timeout(timeout, backend2.init(&model))
        .await
        .expect("init within 120s")
        .expect("init Ok 2");
    let second = *backend2.model_id();

    assert_eq!(
        first, second,
        "model_id must be content-addressed and stable across re-inits of the same URI"
    );
}
