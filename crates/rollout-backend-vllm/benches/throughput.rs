//! Criterion throughput bench for the live `AsyncLLMEngine` (plan 03-03).
//!
//! Runs N=64 prompts × `max_tokens=64` with a fixed seed against
//! `facebook/opt-125m` (small enough for CPU; the perf exit criterion
//! comparing against `scripts/raw_vllm_baseline.py` lives on the
//! self-hosted GPU runner per CONTEXT D-CLI-05).
//!
//! Default-features build compiles a no-op `placeholder` bench so
//! `cargo bench -p rollout-backend-vllm` works without `--features vllm`.
//! With `--features vllm`, the real bench drives `VllmBackend::generate`.
#![allow(missing_docs)]

use criterion::{criterion_group, criterion_main, Criterion};

#[cfg(not(feature = "vllm"))]
fn placeholder(c: &mut Criterion) {
    c.bench_function("placeholder_no_vllm", |b| b.iter(|| 1 + 1));
}

#[cfg(feature = "vllm")]
fn bench_throughput(c: &mut Criterion) {
    use rollout_backend_vllm::VllmBackend;
    use rollout_core::{InferenceBackend, ModelRef, Prompt, SamplingParams};

    // Hard-gate on ROLLOUT_VLLM_AVAILABLE — the bench cannot run without a
    // real vllm install + GPU/CPU runtime. CI's default workflow does NOT
    // execute `cargo bench`; this guard is for the self-hosted runner.
    if std::env::var("ROLLOUT_VLLM_AVAILABLE").as_deref() != Ok("1") {
        c.bench_function("vllm_throughput_skipped", |b| b.iter(|| 1 + 1));
        return;
    }

    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let backend = rt.block_on(async {
        let mut b = VllmBackend::new("bench-throughput").expect("VllmBackend");
        b.init(&ModelRef {
            uri: "facebook/opt-125m".into(),
            content_id: None,
            tokenizer: None,
        })
        .await
        .expect("init");
        b
    });
    let prompts: Vec<Prompt> = (0..64).map(|i| Prompt(format!("hello {i}"))).collect();
    let mut params = SamplingParams::default();
    params.max_tokens = 64;
    params.seed = Some(42);

    c.bench_function("vllm_throughput_n64_t64", |bencher| {
        bencher.to_async(&rt).iter(|| async {
            let _ = backend
                .generate(&prompts, &params)
                .await
                .expect("generate");
        });
    });
}

#[cfg(not(feature = "vllm"))]
criterion_group!(benches, placeholder);
#[cfg(feature = "vllm")]
criterion_group!(benches, bench_throughput);
criterion_main!(benches);
