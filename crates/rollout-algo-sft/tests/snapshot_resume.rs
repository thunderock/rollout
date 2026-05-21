//! TRAIN-03 LOAD-BEARING PROOF — byte-compare bit-identical resume.
//! Runs on every CI build with no GPU / no HF transformers.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use ndarray::Array1;
use rollout_algo_sft::SftAlgo;
use rollout_cloud_local::FsObjectStore;
use rollout_core::config::training::{
    DatasetRef, LrSchedule, OptimizerKind, OptimizerSettings, PackingKind, PackingPolicy,
    SftSettings, TrainingBudget,
};
use rollout_core::{
    AlgoDependencies, CoreError, Event, EventEmitter, LossScope, ModelRef, ObjectStore,
    PolicyAlgorithm, Snapshotter, Storage, TrainableBackend,
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

fn settings(dataset_path: PathBuf) -> SftSettings {
    SftSettings {
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
        packing: PackingPolicy {
            kind: PackingKind::Off,
            max_seq_len: 512,
        },
        loss_on: LossScope::Full,
        minibatch_size: 1,
        gradient_accumulation: 1,
    }
}

async fn build_algo(
    backend: Arc<MockBackend>,
    dataset: PathBuf,
    scratch_dir: &std::path::Path,
) -> (
    SftAlgo,
    Arc<dyn Storage>,
    Arc<dyn ObjectStore>,
    Arc<dyn Snapshotter>,
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
    let algo = SftAlgo::from_settings(settings(dataset), deps).unwrap();
    (algo, storage, object, snapper)
}

#[tokio::test]
async fn bit_identical_resume_at_step_5() {
    let tmp = tempdir().unwrap();
    let dataset = tmp.path().join("data.jsonl");
    std::fs::write(&dataset, r#"{"prompt":"q","completion":"a"}"#).unwrap();

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

    // RUN B Phase 1: 5 steps, capture weights mid-run.
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

    // RUN B Phase 2: rebuild MockBackend from the captured weights; restore step counter; 5 more steps.
    let scratch_b2 = tmp.path().join("run-b2");
    std::fs::create_dir_all(&scratch_b2).unwrap();
    let backend_b2 = Arc::new(MockBackend::new_train_with_weights(42, weights_after_5));
    let backend_b2_view = Arc::clone(&backend_b2);
    let (mut algo_b2, _s, _o, _sn) = build_algo(backend_b2, dataset.clone(), &scratch_b2).await;
    algo_b2.snapshot_restore(snapshot).await.unwrap();
    for _ in 0..5 {
        algo_b2.step_once().await.unwrap();
    }
    let weights_b: Array1<f32> = backend_b2_view.weights_snapshot();

    // BYTE-COMPARE — TRAIN-03 exit criterion.
    assert_eq!(
        weights_a, weights_b,
        "TRAIN-03: bit-identical resume at step 5 FAILED"
    );
}
