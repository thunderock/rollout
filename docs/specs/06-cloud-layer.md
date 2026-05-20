# Spec 06 — Cloud layer

The cloud layer is the **only place** that mentions a specific cloud's SDK. Algorithm code, scheduler code, and CLI code depend on the trait surface in `rollout-core`; the cloud crates implement those traits behind a registry.

## 1. Purpose

`rollout` runs on more than one cloud, from day 1. Lock-in to any single provider is a design failure. The cloud layer exists so that:

- Adding a new cloud is a fixed-scope task (implement four traits, pass the compliance suite, done).
- Algorithms remain testable locally — the **local** cloud is just another implementation.
- Cloud SDK upgrades cannot break algorithms.

## 1a. Phase 2 implementation notes

`Queue::ack` / `nack`, `ObjectStore::exists`, `ComputeHint::preemption_signal`, and `SecretStore::put` ship in `rollout-core` in Phase 2 (plan 02-00). Concrete impls land in `rollout-cloud-local` (Phase 2) and `rollout-cloud-aws` / `-gcp` (Phase 5). `ObjectStore` becomes content-addressed in Phase 2: `put_bytes(Vec<u8>, PutHint) -> ContentId`; the string-keyed Phase-1 stub had no impls and is replaced. `ComputeHint::instance_type` is folded into `ComputeInventory { cpu_count, memory_mib, gpus, instance_type }` returned by a single `inventory()` call. `Queue::enqueue` returns a `QueueItemId(Ulid)` handle and `ack` / `nack` use that handle (replacing the Phase-1 blob enqueue/dequeue).

## 2. Traits implemented by the cloud layer

Every cloud crate (`rollout-cloud-<provider>`) implements four traits, all defined in `rollout-core`:

1. `ObjectStore` — bulk blob storage (S3 / GCS / local fs).
2. `Queue` — distributed queue for work items (SQS / Pub/Sub / in-memory).
3. `SecretStore` — credential/secret retrieval (Secrets Manager / Secret Manager / env-vars).
4. `ComputeHint` — node introspection (EC2 metadata / GCE metadata / `uname -a`).

Optional:

5. `BlockStore` — attached block storage (EBS / Persistent Disk / local file). Used for embedded-DB durability when local disk is ephemeral. Default impl is a no-op; clouds opt in.

## 3. Trait definitions (recap)

```rust
#[async_trait]
pub trait ObjectStore: Send + Sync { ... }   // (full trait in spec 04)

#[async_trait]
pub trait Queue: Send + Sync {
    async fn enqueue(&self, batch: Vec<QueueItem>) -> Result<Vec<QueueItemId>, CoreError>;
    async fn dequeue(&self, budget: DequeueBudget) -> Result<Vec<QueueItem>, CoreError>;
    async fn ack(&self, ids: &[QueueItemId]) -> Result<(), CoreError>;
    async fn nack(&self, ids: &[QueueItemId], retry: RetryHint) -> Result<(), CoreError>;
    async fn purge(&self, filter: QueueFilter) -> Result<u64, CoreError>;
    async fn ping(&self) -> Result<Duration, CoreError>;
}

#[async_trait]
pub trait SecretStore: Send + Sync {
    async fn get(&self, key: &SecretKey) -> Result<SecretValue, CoreError>;
    async fn put(&self, key: SecretKey, value: SecretValue) -> Result<(), CoreError>;
}

#[async_trait]
pub trait ComputeHint: Send + Sync {
    fn provider(&self) -> CloudProvider;
    async fn region(&self) -> Result<String, CoreError>;
    async fn zone(&self) -> Result<String, CoreError>;
    async fn instance_type(&self) -> Result<String, CoreError>;
    async fn gpu_inventory(&self) -> Result<GpuInventory, CoreError>;
    async fn preemption_signal(&self) -> Result<Option<PreemptionEvent>, CoreError>;
}
```

## 4. Provider crates

### 4.1 `rollout-cloud-aws`

Backends:

- `ObjectStore` → S3 (`aws-sdk-s3`).
- `Queue` → SQS (`aws-sdk-sqs`) — *with the polling efficiency caveats below.*
- `SecretStore` → Secrets Manager (`aws-sdk-secretsmanager`).
- `ComputeHint` → EC2 instance metadata service (IMDSv2 only).

**SQS polling note:** SQS long-polling caps a single receive at 10 messages. Naive polling for a 64-item batch costs at least 7 calls. The implementation issues parallel long-polls and merges; benchmark in Phase 5.

### 4.2 `rollout-cloud-gcp`

Backends:

- `ObjectStore` → GCS (`google-cloud-storage`).
- `Queue` → Pub/Sub (`google-cloud-pubsub`).
- `SecretStore` → Secret Manager (`google-cloud-secretmanager`).
- `ComputeHint` → GCE/GKE metadata server.

### 4.3 `rollout-cloud-local`

Pure-Rust local impls. Used for:

- Plugin local-test contract (spec 03).
- Single-node runs on a developer laptop.
- CI without external services.

Backends:

- `ObjectStore` → filesystem under a configurable root, with fsync and atomic-rename semantics.
- `Queue` → in-memory (with optional disk-backed durability via the embedded storage).
- `SecretStore` → env-var lookup with a configurable prefix (`ROLLOUT_SECRET_*`).
- `ComputeHint` → `uname` + `/proc` + NVIDIA SMI / ROCm-smi parsing.

## 5. Compliance suite

Every cloud crate must pass the same compliance test suite — defined in `rollout-cloud-tests`. This is the contract: if your implementation passes the suite, it's a valid cloud backend.

The suite covers, for each trait:

- Happy-path round-trip.
- Idempotency: repeat the same operation, observe consistent result.
- Concurrency: 32 parallel ops, no data loss, no torn writes.
- Failure injection: simulated network errors, observe correct `RetryHint` propagation.
- Pagination: list operations over > one page.
- Large objects: object store handles multi-GB blobs without OOM.
- Auth failure: wrong creds produce a typed `Fatal::ConfigInvalid` error, not a generic IO error.

**CI runs the compliance suite against all in-tree clouds on every PR.** AWS/GCP run against localstack / fake-gcs-server / Pub/Sub emulator; nightly CI runs against real cloud accounts.

## 6. Registry and resolution

The runtime resolves cloud impls through a registry:

```rust
pub struct CloudRegistry {
    object_stores: HashMap<CloudProvider, Arc<dyn ObjectStore>>,
    queues:        HashMap<CloudProvider, Arc<dyn Queue>>,
    secrets:       HashMap<CloudProvider, Arc<dyn SecretStore>>,
    hints:         HashMap<CloudProvider, Arc<dyn ComputeHint>>,
}

impl CloudRegistry {
    pub fn from_plan(plan: &Plan) -> Result<Self, CoreError> {
        // Reads plan.cloud and instantiates the requested provider(s).
    }
}
```

A run can mix clouds — e.g., object store in S3, queue in SQS, secrets in Vault (third-party impl) — but each individual trait has exactly one impl per plan.

## 7. `cloud doctor`

```bash
rollout cloud doctor --provider aws
```

Runs the compliance suite (subset: reachability + auth + a small write) against a live cloud environment. Output:

```
AWS cloud check
  ✓ Credentials resolved (chain: env)
  ✓ S3 bucket reachable: my-rollout-bucket (region us-west-2)
  ✓ S3 round-trip put/get/delete: 84ms
  ✓ SQS queue reachable: rollout-work (region us-west-2)
  ✓ SQS round-trip enqueue/dequeue/ack: 132ms
  ✓ Secrets Manager reachable; lookup test-key: 41ms
  ⚠ EC2 IMDS unreachable (running outside EC2 — using fallback hints)
  ✓ GPU inventory: 0 GPUs detected (CPU-only environment)
```

Use `cloud doctor` before any new environment goes into production.

## 8. Boundary lint

A workspace lint enforces:

- No crate outside `crates/rollout-cloud-*` lists `aws-sdk-*`, `google-cloud-*`, or any other cloud-SDK crate as a dependency.
- No crate outside `crates/rollout-cloud-*` imports those SDKs even transitively from build scripts.

The lint is implemented via `cargo deny` rules in the workspace `Cargo.toml`. Violation fails the build.

## 9. Failure modes

| Failure | Detection | Recovery |
|---|---|---|
| Cloud SDK breaking change | compliance suite fails post-upgrade | pin SDK version; fix impl; re-run suite |
| Provider outage | runtime ops fail with cloud-side error | propagate as `Recoverable::Transient`; backoff per `RetryHint` |
| Cross-region misconfig | `ping` exceeds budget at plan time | warn; user opts in to high-latency mode |
| Credentials expired | typed `Fatal::ConfigInvalid` | run drains; user re-auths and re-runs |
| Quota exhausted | provider-specific error → mapped to `Recoverable::Throttled` | backoff + retry; if persistent, fatal |

## 10. Test contract

- **Compliance suite** runs against every in-tree provider on every PR (with emulators).
- **Live cloud nightly:** the same suite against real AWS / GCP accounts; PR-bound only for changes inside a cloud crate.
- **Boundary lint** enforced in CI.
- **`cloud doctor`** has an integration test against localstack and fake-gcs-server.

## 11. Adding a new cloud

Process to add (e.g.) Azure:

1. Create `crates/rollout-cloud-azure`.
2. Implement the four traits (+ optional `BlockStore`).
3. Pass the compliance suite.
4. Add to the runtime registry and CLI `--provider` choices.
5. Document in `docs/specs/06-cloud-layer.md` (this file).

Estimate: ~3–4 weeks for an experienced contributor with the existing AWS/GCP crates as reference.

## 12. Open questions

- **Hybrid runs (multi-cloud single run):** out of v1 scope but architecturally supported. Document the explicit boundary.
- **Per-bucket / per-queue config:** v1 single bucket / queue per role. Multi-bucket needs in v2.
- **Workload Identity / IRSA / Workload Identity Federation:** prefer these over static keys. v1 cloud crates auto-detect; documentation pass before 1.0.
- **Quota / billing visibility:** out of v1 scope; surface as a future ADR.
