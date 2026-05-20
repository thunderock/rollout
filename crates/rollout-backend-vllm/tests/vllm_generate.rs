//! Live-vLLM generate smoke (gated by `ROLLOUT_VLLM_AVAILABLE=1`).
//!
//! Runs a 1 prompt × 8 tokens round-trip against `Qwen/Qwen2.5-0.5B-Instruct`
//! (the named CONTEXT D-CLI-04 test model). Per RESEARCH Pitfall 8, CPU-mode
//! generation can take tens of seconds on small models; wraps the await in a
//! generous `tokio::time::timeout(Duration::from_secs(300), ...)`.

#![cfg(feature = "vllm")]

use std::time::Duration;

use rollout_backend_vllm::VllmBackend;
use rollout_core::{InferenceBackend, ModelRef, Prompt, SamplingParams};

#[tokio::test]
#[ignore = "requires `pip install vllm` + ROLLOUT_VLLM_AVAILABLE=1 (RESEARCH Pitfall 3 — no macOS wheels)"]
async fn generate_one_prompt_eight_tokens() {
    if std::env::var("ROLLOUT_VLLM_AVAILABLE").as_deref() != Ok("1") {
        eprintln!("ROLLOUT_VLLM_AVAILABLE != 1; skipping live vLLM generate test");
        return;
    }
    let model = ModelRef {
        uri: "Qwen/Qwen2.5-0.5B-Instruct".to_owned(),
        content_id: None,
        tokenizer: None,
    };
    let mut params = SamplingParams::default();
    params.max_tokens = 8;
    params.seed = Some(42);
    let prompts = [Prompt("Hello, world!".to_owned())];

    let mut backend =
        VllmBackend::new("vllm-generate-test").expect("construct VllmBackend");
    tokio::time::timeout(Duration::from_secs(180), backend.init(&model))
        .await
        .expect("init within 180s")
        .expect("init Ok");

    let completions = tokio::time::timeout(
        Duration::from_secs(300),
        backend.generate(&prompts, &params),
    )
    .await
    .expect("generate within 300s")
    .expect("generate Ok");

    assert_eq!(completions.len(), 1);
    let c = &completions[0];
    assert!(!c.text.is_empty(), "completion text must be non-empty");
    assert!(
        matches!(c.finish_reason.as_str(), "stop" | "length" | "eos"),
        "unexpected finish_reason: {}",
        c.finish_reason
    );
}
