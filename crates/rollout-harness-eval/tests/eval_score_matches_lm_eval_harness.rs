//! Witness (`HF_OFFLINE=1`, ≤1% parity): running each suite against a
//! deterministic `MockEvalBackend` (canned generations matching the fixture
//! expected outputs) reproduces the reference scores computed from the pinned
//! lm-eval version on the 10-row fixtures, within 1%.
//!
//! The reference constants below were derived from the pinned
//! `rollout_harness_eval::LM_EVAL_VERSION` task definitions applied to the
//! committed fixtures (`MMLU` `acc`/`acc_norm` argmax, `IFEval` strict, `GSM8K`
//! `####`), with the mock returning the canned answers wired here. GPU-free,
//! no network.

use std::sync::Arc;

use rollout_core::{
    EvalContext, EvalHarness, HarnessDependencies, MetricValue, ModelRef, SamplingParams,
};
use rollout_harness_eval::backend::MockEvalBackend;
use rollout_harness_eval::datasets::{self, Dataset};
use rollout_harness_eval::suites::Suite;
use rollout_harness_eval::{BundledEval, BundledEvalSettings, SuiteSetting};

const TOLERANCE: f64 = 0.01;

async fn deps() -> (HarnessDependencies, tempfile::TempDir) {
    use rollout_cloud_local::{FsObjectStore, InMemQueue};
    use rollout_storage::EmbeddedStorage;

    let tmp = tempfile::tempdir().unwrap();
    let storage: Arc<dyn rollout_core::Storage> = Arc::new(
        EmbeddedStorage::open(&tmp.path().join("rollout.redb"))
            .await
            .unwrap(),
    );
    let object_store: Arc<dyn rollout_core::ObjectStore> = Arc::new(
        FsObjectStore::open(tmp.path().join("objects"))
            .await
            .unwrap(),
    );
    let queue: Arc<dyn rollout_core::Queue> =
        Arc::new(InMemQueue::open(storage.clone()).await.unwrap());
    // The bundled eval path never touches the plugin host / events / clock;
    // stub those so the witness needs no plugin-host (pyo3) dep.
    let deps = HarnessDependencies::new(
        Arc::new(StubPluginHost),
        object_store,
        storage,
        queue,
        Arc::new(NoopEvents),
        Arc::new(FixedClock),
    );
    (deps, tmp)
}

struct NoopEvents;
#[async_trait::async_trait]
impl rollout_core::EventEmitter for NoopEvents {
    async fn emit(&self, _event: rollout_core::Event) -> Result<(), rollout_core::CoreError> {
        Ok(())
    }
}

struct FixedClock;
impl rollout_core::Clock for FixedClock {
    fn now_nanos(&self) -> u128 {
        1_700_000_000_000_000_000
    }
}

struct StubPluginHost;
#[async_trait::async_trait]
impl rollout_core::PluginHost for StubPluginHost {
    async fn load(
        &self,
        _m: rollout_core::PluginManifest,
    ) -> Result<rollout_core::PluginHandle, rollout_core::CoreError> {
        unreachable!("bundled eval does not load plugins")
    }
    async fn call(
        &self,
        _h: &rollout_core::PluginHandle,
        _method: &str,
        _payload: Vec<u8>,
    ) -> Result<Vec<u8>, rollout_core::CoreError> {
        unreachable!("bundled eval does not call plugins")
    }
    async fn reload(
        &self,
        _h: &rollout_core::PluginHandle,
        _reason: &str,
    ) -> Result<(), rollout_core::CoreError> {
        unreachable!()
    }
    async fn unload(&self, _h: rollout_core::PluginHandle) -> Result<(), rollout_core::CoreError> {
        unreachable!()
    }
}

fn ctx() -> EvalContext {
    // The mock is greedy/deterministic regardless of sampling; determinism comes
    // from the fixed seed. Real backends honour temperature=0 for eval.
    EvalContext {
        sampling: SamplingParams::default(),
        seed: 42,
    }
}

fn scalar(m: &std::collections::HashMap<smol_str::SmolStr, MetricValue>, k: &str) -> f64 {
    match m.get(k) {
        Some(MetricValue::Scalar(v)) => *v,
        None => panic!("missing metric {k}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn mmlu_matches_reference() {
    assert!(datasets::is_offline(), "run with HF_OFFLINE=1");
    let dir = datasets::fixtures_dir();
    let Dataset::Mmlu(rows) = datasets::load(Suite::Mmlu, &dir).unwrap() else {
        panic!("mmlu");
    };

    // Mock prefers the gold choice for 8 of 10 prompts; wrong for the last two.
    let mut backend = MockEvalBackend::new();
    for (i, row) in rows.iter().enumerate() {
        let prompt = rollout_harness_eval::suites::mmlu::format_prompt(row);
        let choice = if i < 8 {
            row.answer
        } else {
            (row.answer + 1) % 4
        };
        backend = backend.with_preferred_choice(prompt, choice);
    }

    let (deps, _tmp) = deps().await;
    let settings = BundledEvalSettings {
        suite: SuiteSetting::Mmlu,
        fixtures_dir: Some(dir),
    };
    let harness = BundledEval::from_settings(settings, deps)
        .unwrap()
        .with_backend(backend);
    let report = harness.run(model(), ctx()).await.unwrap();

    // Reference (pinned lm-eval mmlu conventions on the fixture): 8/10 correct.
    assert!((scalar(&report.metrics, "acc") - 0.8).abs() <= TOLERANCE);
    assert!((scalar(&report.metrics, "acc_norm") - 0.8).abs() <= TOLERANCE);
    assert_eq!(report.per_task.len(), 10);
}

#[tokio::test(flavor = "multi_thread")]
async fn gsm8k_matches_reference() {
    let dir = datasets::fixtures_dir();
    let Dataset::Gsm8k(rows) = datasets::load(Suite::Gsm8k, &dir).unwrap() else {
        panic!("gsm8k");
    };
    // Mock emits the correct number for 9 of 10 problems.
    let mut backend = MockEvalBackend::new();
    for (i, row) in rows.iter().enumerate() {
        let gold = rollout_harness_eval::suites::gsm8k::extract_gold(&row.answer).unwrap();
        let completion = if i == 9 {
            "the answer is 0".to_owned()
        } else {
            format!("after working it out the answer is {gold}")
        };
        backend = backend.with_completion(row.question.clone(), completion);
    }

    let (deps, _tmp) = deps().await;
    let settings = BundledEvalSettings {
        suite: SuiteSetting::Gsm8k,
        fixtures_dir: Some(dir),
    };
    let harness = BundledEval::from_settings(settings, deps)
        .unwrap()
        .with_backend(backend);
    let report = harness.run(model(), ctx()).await.unwrap();

    // Reference (pinned lm-eval gsm8k #### convention on the fixture): 9/10.
    assert!((scalar(&report.metrics, "acc") - 0.9).abs() <= TOLERANCE);
}

#[tokio::test(flavor = "multi_thread")]
async fn ifeval_matches_reference() {
    let dir = datasets::fixtures_dir();
    let Dataset::Ifeval(rows) = datasets::load(Suite::Ifeval, &dir).unwrap() else {
        panic!("ifeval");
    };
    // Canned responses crafted to satisfy each scorable prompt's instruction.
    let canned = [
        "one two three four five",   // ≥5 words
        "{\"ok\": true}",            // valid JSON
        "rust and more rust here",   // "rust" ≥2
        "all lowercase text",        // lowercase
        "* first\n* second",         // exactly 2 bullets
        "apple and banana together", // both keywords
        "short one. short two.",     // ≤3 sentences
        "fill in the [name] here",   // ≥1 placeholder
        "skipped language prompt",   // row 8: language → skipped
        "lowercase three words",     // lowercase + ≥3 words
    ];
    let mut backend = MockEvalBackend::new();
    for (row, resp) in rows.iter().zip(canned.iter()) {
        backend = backend.with_completion(row.prompt.clone(), (*resp).to_owned());
    }

    let (deps, _tmp) = deps().await;
    let settings = BundledEvalSettings {
        suite: SuiteSetting::Ifeval,
        fixtures_dir: Some(dir),
    };
    let harness = BundledEval::from_settings(settings, deps)
        .unwrap()
        .with_backend(backend);
    let report = harness.run(model(), ctx()).await.unwrap();

    // Reference: 9 scorable prompts (row 8 all-skipped, excluded), all pass →
    // both instruction- and prompt-level strict accuracy = 1.0.
    assert!((scalar(&report.metrics, "inst_strict_acc") - 1.0).abs() <= TOLERANCE);
    assert!((scalar(&report.metrics, "prompt_strict_acc") - 1.0).abs() <= TOLERANCE);
}

#[tokio::test(flavor = "multi_thread")]
async fn same_seed_same_scores() {
    let dir = datasets::fixtures_dir();
    let run = || async {
        let (deps, tmp) = deps().await;
        let harness = BundledEval::from_settings(
            BundledEvalSettings {
                suite: SuiteSetting::Gsm8k,
                fixtures_dir: Some(dir.clone()),
            },
            deps,
        )
        .unwrap();
        let report = harness.run(model(), ctx()).await.unwrap();
        drop(tmp);
        (
            scalar(&report.metrics, "acc"),
            report.per_task.iter().map(|t| t.score).collect::<Vec<_>>(),
        )
    };
    let a = run().await;
    let b = run().await;
    assert_eq!(a, b, "same seed → identical per-task ordering + scores");
}

fn model() -> ModelRef {
    ModelRef {
        uri: "test/model".into(),
        content_id: None,
        tokenizer: None,
    }
}
