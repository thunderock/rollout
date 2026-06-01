//! Compile-time assertion that the Phase-1 trait surface and the Phase-2
//! extensions are publicly exported, `Send + Sync`, and object-safe.

#![allow(dead_code)]
#![allow(clippy::let_underscore_future)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::needless_pass_by_value)]

use rollout_core::{
    Clock, Completion, ComputeHint, ComputeInventory, Coordinator, EntrySpec, EnvHarness,
    EvalHarness, Event, EventEmitter, EventKind, GpuInfo, Heartbeat, InferenceBackend, KeyRange,
    Level, ModelRef, ObjectStore, Plugin, PluginDependencies, PluginHandle, PluginHost, PluginId,
    PluginKind, PluginManifest, PluginMode, PolicyAlgorithm, Prompt, PutHint, Queue, QueueItemId,
    RuntimeHints, SamplingParams, Scheduler, SecretStore, SidecarProtocol, Snapshotter, SpanPhase,
    Storage, StorageEvent, StorageKey, StorageTxn, ToolHarness, Worker, WorkerRole, WorkerState,
};
use std::sync::Arc;

fn assert_send_sync<T: Send + Sync + ?Sized>() {}

// Phase-4 (D-WAVE0-02): `PolicyAlgorithm` carries an associated `Settings` type,
// so the trait is no longer object-safe (Phase-1 placeholder was). We check it
// compiles + Send + Sync via a generic bound instead of a trait object.
fn algorithm<T: PolicyAlgorithm>() {
    fn check_send_sync<T: Send + Sync>() {}
    check_send_sync::<T>();
}
fn worker() {
    let _: Option<Arc<dyn Worker>> = None;
}
fn coordinator() {
    let _: Option<Arc<dyn Coordinator>> = None;
}
fn scheduler() {
    let _: Option<Arc<dyn Scheduler>> = None;
}
fn plugin() {
    let _: Option<Arc<dyn Plugin>> = None;
}
fn plugin_host() {
    let _: Option<Arc<dyn PluginHost>> = None;
}
// Phase-7 (D-CORE-01): the harness traits carry an associated `Settings` type,
// so they are no longer object-safe (the v1.0 stub was). Check Send + Sync via a
// generic bound instead of a trait object — same pattern as `PolicyAlgorithm`.
fn env_harness<T: EnvHarness>() {
    fn check_send_sync<T: Send + Sync>() {}
    check_send_sync::<T>();
}
fn tool_harness<T: ToolHarness>() {
    fn check_send_sync<T: Send + Sync>() {}
    check_send_sync::<T>();
}
fn eval_harness<T: EvalHarness>() {
    fn check_send_sync<T: Send + Sync>() {}
    check_send_sync::<T>();
}
fn inference_backend() {
    let _: Option<Arc<dyn InferenceBackend>> = None;
}
fn storage() {
    let _: Option<Arc<dyn Storage>> = None;
}
fn storage_txn() {
    let _: Option<Arc<dyn StorageTxn>> = None;
}
fn snapshotter() {
    let _: Option<Arc<dyn Snapshotter>> = None;
}
fn object_store() {
    let _: Option<Arc<dyn ObjectStore>> = None;
}
fn secret_store() {
    let _: Option<Arc<dyn SecretStore>> = None;
}
fn compute_hint() {
    let _: Option<Arc<dyn ComputeHint>> = None;
}
fn queue() {
    let _: Option<Arc<dyn Queue>> = None;
}
fn clock() {
    let _: Option<Arc<dyn Clock>> = None;
}
fn event_emitter() {
    let _: Option<Arc<dyn EventEmitter>> = None;
}

// Send + Sync bounds.
fn send_sync_bounds() {
    // PolicyAlgorithm is no longer object-safe (Phase-4 added an associated
    // `Settings` type). Send + Sync is checked via the generic `algorithm<T>`
    // function above.
    assert_send_sync::<dyn Worker>();
    assert_send_sync::<dyn Coordinator>();
    assert_send_sync::<dyn Scheduler>();
    assert_send_sync::<dyn Plugin>();
    assert_send_sync::<dyn PluginHost>();
    // EnvHarness/ToolHarness/EvalHarness are no longer object-safe (associated
    // `Settings`); their Send + Sync is checked via the generic helpers above.
    assert_send_sync::<dyn InferenceBackend>();
    assert_send_sync::<dyn Storage>();
    assert_send_sync::<dyn StorageTxn>();
    assert_send_sync::<dyn Snapshotter>();
    assert_send_sync::<dyn ObjectStore>();
    assert_send_sync::<dyn SecretStore>();
    assert_send_sync::<dyn ComputeHint>();
    assert_send_sync::<dyn Queue>();
    assert_send_sync::<dyn Clock>();
    assert_send_sync::<dyn EventEmitter>();
}

#[test]
fn trait_surface_counts_19() {
    // Marker test so `cargo test --test trait_surface` reports a passing test.
    // The real surface check is the compilation of this file.
}

// --- Phase-2 extensions ---------------------------------------------------------

#[test]
fn storage_trait_has_extended_surface() {
    // Method-shape compile checks for the Phase-2 Storage surface.
    fn _shape(s: &dyn Storage, k: StorageKey, ks: Vec<StorageKey>, r: KeyRange) {
        let _ = s.begin();
        let _ = s.get_bytes(&k);
        let _ = s.get_many_bytes(&ks);
        let _ = s.scan_bytes(r);
        let _ = s.watch(k);
        let _ = s.ping();
    }
}

#[test]
fn storage_txn_has_extended_surface() {
    // Compile-only: assert StorageTxn carries put_bytes/delete/cas_bytes/commit/abort.
    fn _shape(t: &mut dyn StorageTxn, k: StorageKey, v: Vec<u8>) {
        let _ = t.put_bytes(k.clone(), v.clone());
        let _ = t.delete(k.clone());
        let _ = t.cas_bytes(k, Some(v.clone()), Some(v));
    }
    fn _commit_abort(t: Box<dyn StorageTxn>) {
        // Method shape: commit and abort consume the boxed txn.
        let _ = async move {
            t.commit().await.ok();
        };
    }
}

#[test]
fn plugin_host_has_extended_surface() {
    fn _shape(h: &dyn PluginHost, m: PluginManifest, handle: &PluginHandle, payload: Vec<u8>) {
        let _ = h.load(m);
        let _ = h.call(handle, "method", payload.clone());
        let _ = h.reload(handle, "reason");
        let _ = h.unload(handle.clone());
    }
}

#[test]
fn coordinator_has_heartbeat() {
    fn _shape(c: &dyn Coordinator, hb: Heartbeat) {
        let _ = c.heartbeat(hb);
    }
}

#[test]
fn worker_has_lifecycle_hooks() {
    fn _shape(w: &mut dyn Worker) {
        let ctx = rollout_core::WorkerContext;
        let _ = w.init(&ctx);
        let _ = w.ready();
    }
}

#[test]
fn cloud_traits_match_spec_06() {
    fn _os(o: &dyn ObjectStore, b: Vec<u8>, id: rollout_core::ContentId) {
        let _ = o.put_bytes(b, PutHint::default());
        let _ = o.get_bytes(&id);
        let _ = o.exists(&id);
    }
    fn _q(q: &dyn Queue, b: Vec<u8>, id: QueueItemId) {
        let _ = q.enqueue(b);
        let _ = q.dequeue();
        let _ = q.ack(id);
        let _ = q.nack(id);
    }
    fn _s(s: &dyn SecretStore) {
        let _ = s.get("k");
        let _ = s.put("k", "v");
    }
    fn _c(c: &dyn ComputeHint) {
        let _ = c.inventory();
        let _ = c.preemption_signal();
    }
}

#[test]
fn new_types_exist() {
    // Type-name compile checks for every new Phase-2 type.
    fn _types(
        _sk: StorageKey,
        _kr: KeyRange,
        _se: StorageEvent,
        _pm: PluginManifest,
        _ph: PluginHandle,
        _pk: PluginKind,
        _pmd: PluginMode,
        _hb: Heartbeat,
        _ws: WorkerState,
        _put: PutHint,
        _ci: ComputeInventory,
        _gpu: GpuInfo,
        _qid: QueueItemId,
        _id: PluginId,
        _es: EntrySpec,
        _sp: SidecarProtocol,
        _rh: RuntimeHints,
        _pd: PluginDependencies,
    ) {
    }
}

#[test]
fn event_emitter_trait_exists() {
    fn _assert_object_safe(_: &dyn EventEmitter) {}
    fn _types(_e: Event, _k: EventKind, _l: Level, _s: SpanPhase) {}
}

// --- Phase-3 extensions ---------------------------------------------------------

#[test]
fn inference_backend_has_extended_surface() {
    // Compile-only: assert the Phase-3 four-method shape resolves.
    fn _shape(
        b: &mut dyn InferenceBackend,
        m: ModelRef,
        prompts: Vec<Prompt>,
        params: SamplingParams,
    ) {
        let _ = b.init(&m);
        let _ = b.generate(&prompts, &params);
        let _: &rollout_core::ContentId = b.model_id();
        let _ = b.shutdown();
    }
}

#[test]
fn phase3_new_types_exist() {
    fn _types(_p: Prompt, _c: Completion, _m: ModelRef, _s: SamplingParams) {}
}

#[test]
fn sampling_params_has_serde_defaults() {
    // Default values matter for resume determinism (RESEARCH Pitfall 4).
    let sp = SamplingParams::default();
    assert!((sp.temperature - 1.0).abs() < f32::EPSILON);
    assert!((sp.top_p - 1.0).abs() < f32::EPSILON);
    assert_eq!(sp.top_k, -1);
    assert_eq!(sp.max_tokens, 16);
    assert_eq!(sp.seed, None);
    assert!(sp.stop.is_empty());
    assert!(!sp.stream);
}

#[test]
fn worker_role_variants_construct() {
    use smol_str::SmolStr;
    let variants = [
        WorkerRole::Coordinator,
        WorkerRole::BatchInference,
        WorkerRole::BatchReader,
        WorkerRole::BatchWriter,
        WorkerRole::LearnerWorker,
        WorkerRole::Custom(SmolStr::new("future-phase")),
    ];
    // Exhaustive pattern-match proves all six variants are public + constructible.
    for v in variants {
        match v {
            WorkerRole::Coordinator
            | WorkerRole::BatchInference
            | WorkerRole::BatchReader
            | WorkerRole::BatchWriter
            | WorkerRole::LearnerWorker
            | WorkerRole::Custom(_) => {}
        }
    }
}

// --- Phase-4 extensions ---------------------------------------------------------

#[test]
fn trainable_backend_is_object_safe_and_send_sync() {
    use rollout_core::TrainableBackend;
    fn _accept(_: Box<dyn TrainableBackend>) {}
    assert_send_sync::<dyn TrainableBackend>();
}

#[test]
fn snapshotter_phase4_is_object_safe_and_send_sync() {
    use rollout_core::Snapshotter;
    fn _accept(_: Box<dyn Snapshotter>) {}
    assert_send_sync::<dyn Snapshotter>();
}

#[test]
fn snapshot_kind_serde_round_trip() {
    use rollout_core::SnapshotKind;
    let s = serde_json::to_string(&SnapshotKind::TrainState).unwrap();
    assert_eq!(s, "\"train_state\"");
    let back: SnapshotKind = serde_json::from_str(&s).unwrap();
    assert!(matches!(back, SnapshotKind::TrainState));
}

#[test]
fn worker_role_learner_serde_round_trip() {
    let s = serde_json::to_string(&WorkerRole::LearnerWorker).unwrap();
    assert_eq!(s, "\"learner_worker\"");
    let back: WorkerRole = serde_json::from_str(&s).unwrap();
    assert!(matches!(back, WorkerRole::LearnerWorker));
}

#[test]
fn snapshot_filter_default() {
    use rollout_core::SnapshotFilter;
    let f = SnapshotFilter::default();
    assert!(f.run_id.is_none() && f.kind.is_none() && f.label_contains.is_none());
}

#[test]
fn phase4_new_types_exist() {
    use rollout_core::{
        AlgoContext, AlgoDependencies, AlgorithmId, ConfigViolation, GradHandle, LossOutput,
        LossScope, MaskSpec, PeriodicPolicy, Plan, PrunePolicy, RestoreTarget, RetentionPolicy,
        RunOutcome, Snapshot, SnapshotFilter, SnapshotId, SnapshotKind, SnapshotPart,
        SnapshotPolicy, SnapshotRequest, TrainBatch,
    };
    fn _types(
        _ac: Option<AlgoContext<'_>>,
        _ad: Option<AlgoDependencies>,
        _ai: AlgorithmId,
        _cv: ConfigViolation,
        _gh: GradHandle,
        _lo: Option<LossOutput>,
        _ls: LossScope,
        _ms: MaskSpec,
        _pp: PeriodicPolicy,
        _pl: Plan,
        _pr: PrunePolicy,
        _rt: RestoreTarget,
        _rp: RetentionPolicy,
        _ro: RunOutcome,
        _s: Snapshot,
        _sf: SnapshotFilter,
        _si: SnapshotId,
        _sk: SnapshotKind,
        _sp: SnapshotPart,
        _spl: SnapshotPolicy,
        _sr: SnapshotRequest,
        _tb: TrainBatch,
    ) {
    }
}
