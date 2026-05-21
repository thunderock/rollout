//! TRAIN-02 content-addressed checkpoint round-trip:
//! `save_weights` returns a stable `ContentId` when nothing has changed, and a
//! different `ContentId` after a non-trivial optimizer step.

use rollout_core::{ContentId, TrainableBackend};
use rollout_runtime_batch::MockBackend;

#[tokio::test]
async fn checkpoint_content_id_stable_when_idle() {
    let backend = MockBackend::new_train(99);
    let id1: ContentId = backend.save_weights().await.unwrap();
    let id2: ContentId = backend.save_weights().await.unwrap();
    assert_eq!(
        id1, id2,
        "save_weights should be stable when no SGD step occurred"
    );
}

#[tokio::test]
async fn checkpoint_content_id_changes_after_step() {
    use rollout_core::config::training::{LrSchedule, OptimizerKind, OptimizerSettings};
    use rollout_core::{LossScope, TrainBatch};

    let backend = MockBackend::new_train(99);
    let id1: ContentId = backend.save_weights().await.unwrap();

    let opt = OptimizerSettings {
        kind: OptimizerKind::Sgd,
        lr: 0.01,
        weight_decay: 0.0,
        betas: [0.9, 0.999],
        eps: 1e-8,
        warmup_steps: 0,
        schedule: LrSchedule::Constant,
    };
    let batch = TrainBatch::with_rows(1, 4, vec!["x".into()]);
    let l = backend
        .forward_with_loss(&batch, &LossScope::Full)
        .await
        .unwrap();
    backend.optimizer_step(l.grad_handle, &opt).await.unwrap();

    let id2: ContentId = backend.save_weights().await.unwrap();
    assert_ne!(
        id1, id2,
        "save_weights should differ after a non-trivial optimizer step"
    );
}
