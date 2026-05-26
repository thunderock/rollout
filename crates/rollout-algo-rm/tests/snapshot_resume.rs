//! TRAIN-03 second-witness — byte-compare bit-identical RM resume.
//! Mirrors `rollout-algo-sft::tests::snapshot_resume` for the RM variant.
//! Also exercises the algo-level surface (id, `validate_plan`, happy path).

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use ndarray::Array1;
use rollout_algo_rm::RmAlgo;
use rollout_cloud_local::FsObjectStore;
use rollout_core::config::training::{
    DatasetRef, LrSchedule, OptimizerKind, OptimizerSettings, RmHeadKind, RmSettings,
    TrainingBudget,
};
use rollout_core::{
    AlgoDependencies, AlgorithmId, CoreError, Event, EventEmitter, ModelRef, ObjectStore,
    PolicyAlgorithm, Snapshotter, Storage, TrainableBackend, WorkerRole,
};
use rollout_runtime_batch::MockBackend;
use rollout_snapshots::SnapshotterImpl;
use rollout_storage::EmbeddedStorage;
use tempfile::tempdir;

struct TestEmitter;

#[async_trait]
impl EventEmitter for TestEmitter {
    async fn emit(&self, _event: Event) -> Result<(), CoreError> {
        Ok(())
    }
}

fn opt() -> OptimizerSettings {
    OptimizerSettings {
        kind: OptimizerKind::Sgd,
        lr: 0.01,
        weight_decay: 0.0,
        betas: [0.9, 0.999],
        eps: 1e-8,
        warmup_steps: 0,
        schedule: LrSchedule::Constant,
    }
}

fn settings(dataset_path: PathBuf) -> RmSettings {
    RmSettings {
        base_model: ModelRef {
            uri: "mock://".into(),
            content_id: None,
            tokenizer: None,
        },
        optimizer: opt(),
        budget: TrainingBudget {
            max_steps: Some(0),
            max_tokens: None,
            max_walltime: None,
        },
        dataset: DatasetRef::JsonlPath { path: dataset_path },
        head: RmHeadKind::BradleyTerry,
        minibatch_size: 1,
    }
}

async fn build_algo(
    backend: Arc<MockBackend>,
    dataset: PathBuf,
    scratch_dir: &std::path::Path,
) -> (
    RmAlgo,
    Arc<dyn Storage>,
    Arc<dyn ObjectStore>,
    Arc<dyn Snapshotter>,
) {
    let storage: Arc<dyn Storage> = Arc::new(
        EmbeddedStorage::open(&scratch_dir.join("st.db"))
            .await
            .unwrap(),
    );
    let object: Arc<dyn ObjectStore> =
        Arc::new(FsObjectStore::open(&scratch_dir.join("obj")).await.unwrap());
    let snapper: Arc<dyn Snapshotter> = Arc::new(SnapshotterImpl::new(
        Arc::clone(&storage),
        Arc::clone(&object),
        scratch_dir.to_path_buf(),
    ));
    let events: Arc<dyn EventEmitter> = Arc::new(TestEmitter);

    let deps = AlgoDependencies {
        backend: backend as Arc<dyn TrainableBackend>,
        storage: Arc::clone(&storage),
        object: Arc::clone(&object),
        snapshots: Arc::clone(&snapper),
        events,
    };
    let algo = RmAlgo::from_settings(settings(dataset), deps).unwrap();
    (algo, storage, object, snapper)
}

async fn build_deps(
    backend: Arc<MockBackend>,
    scratch_dir: &std::path::Path,
) -> (
    AlgoDependencies,
    (Arc<dyn Storage>, Arc<dyn ObjectStore>, Arc<dyn Snapshotter>),
) {
    let storage: Arc<dyn Storage> = Arc::new(
        EmbeddedStorage::open(&scratch_dir.join("st.db"))
            .await
            .unwrap(),
    );
    let object: Arc<dyn ObjectStore> =
        Arc::new(FsObjectStore::open(&scratch_dir.join("obj")).await.unwrap());
    let snapper: Arc<dyn Snapshotter> = Arc::new(SnapshotterImpl::new(
        Arc::clone(&storage),
        Arc::clone(&object),
        scratch_dir.to_path_buf(),
    ));
    let events: Arc<dyn EventEmitter> = Arc::new(TestEmitter);

    let deps = AlgoDependencies {
        backend: backend as Arc<dyn TrainableBackend>,
        storage: Arc::clone(&storage),
        object: Arc::clone(&object),
        snapshots: Arc::clone(&snapper),
        events,
    };
    (deps, (storage, object, snapper))
}

#[tokio::test]
async fn rm_id_is_stable() {
    let id = RmAlgo::id();
    assert_eq!(id, AlgorithmId(smol_str::SmolStr::new_inline("rm")));
}

#[tokio::test]
async fn validate_plan_rejects_pairwise_logistic() {
    let tmp = tempdir().unwrap();
    let dataset = tmp.path().join("d.jsonl");
    std::fs::write(&dataset, r#"{"prompt":"P","chosen":"C","rejected":"R"}"#).unwrap();

    let mut s = settings(dataset);
    s.head = RmHeadKind::PairwiseLogistic;

    let backend = Arc::new(MockBackend::new_train(1));
    let (deps, _keep) = build_deps(backend, tmp.path()).await;
    let algo = RmAlgo::from_settings(s, deps).unwrap();
    let violations = algo
        .validate_plan(&rollout_core::Plan::default())
        .unwrap_err();
    assert!(
        violations.iter().any(|v| {
            v.locator.contains("head")
                && v.message.contains("PairwiseLogistic")
                && v.message.contains("Phase 9")
        }),
        "expected PairwiseLogistic Phase 9 violation, got {violations:?}"
    );
}

#[tokio::test]
async fn validate_plan_rejects_zero_minibatch() {
    let tmp = tempdir().unwrap();
    let dataset = tmp.path().join("d.jsonl");
    std::fs::write(&dataset, r#"{"prompt":"P","chosen":"C","rejected":"R"}"#).unwrap();

    let mut s = settings(dataset);
    s.minibatch_size = 0;

    let backend = Arc::new(MockBackend::new_train(1));
    let (deps, _keep) = build_deps(backend, tmp.path()).await;
    let algo = RmAlgo::from_settings(s, deps).unwrap();
    let violations = algo
        .validate_plan(&rollout_core::Plan::default())
        .unwrap_err();
    assert!(violations
        .iter()
        .any(|v| v.locator.contains("minibatch_size")));
}

#[tokio::test]
async fn required_roles_is_learner() {
    let tmp = tempdir().unwrap();
    let dataset = tmp.path().join("d.jsonl");
    std::fs::write(&dataset, r#"{"prompt":"P","chosen":"C","rejected":"R"}"#).unwrap();
    let backend = Arc::new(MockBackend::new_train(1));
    let (deps, _keep) = build_deps(backend, tmp.path()).await;
    let algo = RmAlgo::from_settings(settings(dataset), deps).unwrap();
    assert!(algo.required_roles().contains(&WorkerRole::LearnerWorker));
}

#[tokio::test]
async fn happy_path_two_steps_no_crash() {
    let tmp = tempdir().unwrap();
    let dataset = tmp.path().join("d.jsonl");
    std::fs::write(&dataset, r#"{"prompt":"P","chosen":"C","rejected":"R"}"#).unwrap();

    let backend = Arc::new(MockBackend::new_train(1));
    let backend_view: Arc<MockBackend> = Arc::clone(&backend);
    let (deps, _keep) = build_deps(backend, tmp.path()).await;
    let mut algo = RmAlgo::from_settings(settings(dataset), deps).unwrap();

    algo.step_once().await.unwrap();
    algo.step_once().await.unwrap();

    assert_eq!(algo.step(), 2);
    assert_eq!(backend_view.step(), 2);
}

/// TRAIN-03 SECOND-WITNESS — byte-compare bit-identical RM resume at step 5.
#[tokio::test]
async fn bit_identical_resume_at_step_5() {
    let tmp = tempdir().unwrap();
    let dataset = tmp.path().join("data.jsonl");
    std::fs::write(&dataset, r#"{"prompt":"P","chosen":"C","rejected":"R"}"#).unwrap();

    // RUN A: 10 steps uninterrupted with seed=42.
    let scratch_a = tmp.path().join("run-a");
    std::fs::create_dir_all(&scratch_a).unwrap();
    let backend_a = Arc::new(MockBackend::new_train(42));
    let backend_a_view = Arc::clone(&backend_a);
    let (mut algo_a, _s, _o, _sn) = build_algo(backend_a, dataset.clone(), &scratch_a).await;
    for _ in 0..10 {
        algo_a.step_once().await.unwrap();
    }
    let weights_a: Array1<f32> = backend_a_view.weights_snapshot();

    // RUN B Phase 1: 5 steps, capture mid-run weights + snapshot.
    let scratch_b1 = tmp.path().join("run-b1");
    std::fs::create_dir_all(&scratch_b1).unwrap();
    let backend_b1 = Arc::new(MockBackend::new_train(42));
    let backend_b1_view = Arc::clone(&backend_b1);
    let (mut algo_b1, _s, _o, _sn) = build_algo(backend_b1, dataset.clone(), &scratch_b1).await;
    for _ in 0..5 {
        algo_b1.step_once().await.unwrap();
    }
    let weights_after_5 = backend_b1_view.weights_snapshot();
    let snapshot = algo_b1.snapshot_save().await.unwrap();
    drop(algo_b1);
    drop(backend_b1_view);

    // RUN B Phase 2: rebuild MockBackend from the captured weights, push step
    // counter to 5 (mirrors what a production load_weights would do), restore
    // algo step from snapshot meta, run 5 more steps.
    let scratch_b2 = tmp.path().join("run-b2");
    std::fs::create_dir_all(&scratch_b2).unwrap();
    let backend_b2 = Arc::new(MockBackend::new_train_with_weights(42, weights_after_5));
    let backend_b2_view = Arc::clone(&backend_b2);
    backend_b2_view.set_step(5);
    let (mut algo_b2, _s, _o, _sn) = build_algo(backend_b2, dataset.clone(), &scratch_b2).await;
    algo_b2.snapshot_restore(snapshot).await.unwrap();
    for _ in 0..5 {
        algo_b2.step_once().await.unwrap();
    }
    let weights_b: Array1<f32> = backend_b2_view.weights_snapshot();

    // BYTE-COMPARE — TRAIN-03 second-witness exit criterion.
    assert_eq!(
        weights_a, weights_b,
        "TRAIN-03 (RM): bit-identical resume at step 5 FAILED"
    );
}
