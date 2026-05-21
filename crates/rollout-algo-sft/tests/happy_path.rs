//! Happy-path SFT test: SftAlgo + MockBackend run 2 steps without error.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use rollout_algo_sft::SftAlgo;
use rollout_cloud_local::FsObjectStore;
use rollout_core::config::training::{
    DatasetRef, LrSchedule, OptimizerKind, OptimizerSettings, PackingKind, PackingPolicy,
    SftSettings, TrainingBudget,
};
use rollout_core::{
    AlgoDependencies, AlgorithmId, CoreError, Event, EventEmitter, LossScope, ModelRef,
    ObjectStore, PolicyAlgorithm, Snapshotter, Storage, TrainableBackend, WorkerRole,
};
use rollout_runtime_batch::MockBackend;
use rollout_snapshots::SnapshotterImpl;
use rollout_storage::EmbeddedStorage;
use tempfile::tempdir;

// Local stub: rollout-coordinator's NoopEmitter would force a cycle.
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

fn settings(dataset: PathBuf) -> SftSettings {
    SftSettings {
        base_model: ModelRef {
            uri: "mock://".into(),
            content_id: None,
            tokenizer: None,
        },
        optimizer: opt(),
        budget: TrainingBudget {
            max_steps: Some(2),
            max_tokens: None,
            max_walltime: None,
        },
        dataset: DatasetRef::JsonlPath { path: dataset },
        packing: PackingPolicy {
            kind: PackingKind::Off,
            max_seq_len: 512,
        },
        loss_on: LossScope::Full,
        minibatch_size: 1,
        gradient_accumulation: 1,
    }
}

#[tokio::test]
async fn sft_id_is_stable() {
    let id = SftAlgo::id();
    assert_eq!(id, AlgorithmId(smol_str::SmolStr::new_inline("sft")));
}

#[tokio::test]
async fn validate_plan_rejects_zero_minibatch() {
    let tmp = tempdir().unwrap();
    let dataset = tmp.path().join("d.jsonl");
    std::fs::write(&dataset, r#"{"prompt":"q","completion":"a"}"#).unwrap();

    let mut s = settings(dataset);
    s.minibatch_size = 0;

    let backend = Arc::new(MockBackend::new_train(1));
    let (deps, _keep) = build_deps(backend, tmp.path()).await;
    let algo = SftAlgo::from_settings(s, deps).unwrap();
    let violations = algo.validate_plan(&rollout_core::Plan::default()).unwrap_err();
    assert!(violations.iter().any(|v| v.locator.contains("minibatch_size")));
}

#[tokio::test]
async fn required_roles_is_learner() {
    let tmp = tempdir().unwrap();
    let dataset = tmp.path().join("d.jsonl");
    std::fs::write(&dataset, r#"{"prompt":"q","completion":"a"}"#).unwrap();

    let backend = Arc::new(MockBackend::new_train(1));
    let (deps, _keep) = build_deps(backend, tmp.path()).await;
    let algo = SftAlgo::from_settings(settings(dataset), deps).unwrap();
    let roles = algo.required_roles();
    assert!(roles.contains(&WorkerRole::LearnerWorker));
}

#[tokio::test]
async fn happy_path_two_steps_no_crash() {
    let tmp = tempdir().unwrap();
    let dataset = tmp.path().join("d.jsonl");
    std::fs::write(&dataset, r#"{"prompt":"q","completion":"a"}"#).unwrap();

    let backend = Arc::new(MockBackend::new_train(1));
    let backend_view: Arc<MockBackend> = Arc::clone(&backend);
    let (deps, _keep) = build_deps(backend, tmp.path()).await;
    let mut algo = SftAlgo::from_settings(settings(dataset), deps).unwrap();

    algo.step_once().await.unwrap();
    algo.step_once().await.unwrap();

    assert_eq!(algo.step(), 2);
    assert_eq!(backend_view.step(), 2);
}

/// Build `AlgoDependencies` plus a "keep-alive" tuple so the Storage/Object
/// handles aren't dropped before the test finishes.
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
    let object: Arc<dyn ObjectStore> = Arc::new(
        FsObjectStore::open(&scratch_dir.join("obj"))
            .await
            .unwrap(),
    );
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
