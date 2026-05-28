# Pitfalls Research — v1.1 (cloud + multi-node + harnesses)

**Domain:** Lifting v1.0 single-host RL/LLM training framework to multi-cloud, multi-node, with sandboxed harnesses
**Researched:** 2026-05-27
**Confidence:** HIGH (grounded in the v1.0 codebase, STACK.md and ARCHITECTURE.md picks; AWS/GCP behavior cross-verified with official docs)
**Phase reference:** Phase 5 = Cloud (CLOUD-01..03); Phase 6 = Distribution (DIST-01..04); Phase 7 = Harnesses (HARNESS-01..03)

This document enumerates pitfalls **specific to the v1.1 delta** — adding `rollout-cloud-aws`, `rollout-cloud-gcp`, real multi-node coordinator + work-stealing, spot-preempt drain, and three new harness crates onto an already-shipped v1.0 substrate. Each pitfall cites the surface that hits it (v1.0 invariant, STACK.md pin, ARCHITECTURE.md trait extension) and the prevention strategy is actionable (CI job name, test fixture name, invariant number, config validator rule, or PR-time discipline).

---

## Critical Pitfalls

### Pitfall 1: AWS / GCP SDK type leakage into algorithm crates through trait bounds

**What goes wrong:**
A method on `ObjectStore` / `Queue` / `SecretStore` / `ComputeHint` exposes an SDK type — either as a parameter (`aws_sdk_s3::types::ChecksumAlgorithm`), an error variant (`aws_smithy_runtime_api::client::result::SdkError<...>` flattened into `CoreError::Internal(String)` *while keeping the source chain that pulls SDK types through `std::error::Error::source`*), or as an associated type on a Stream (`ByteStream` from `aws-smithy-types`). `cargo_metadata` dep-direction lint passes because no crate in the algo layer *names* `aws-sdk-s3` in `Cargo.toml` — the type leaks via *re-export* through `rollout-core`.

**Why it happens:**
The §2.3 streaming trait extensions (`put_stream`/`get_stream`) are the natural carrier. `aws_sdk_s3::operation::put_object::PutObjectOutput` has an `e_tag` field; convenient to plumb it through for "verify upload" semantics. Once it's on the `ObjectStore` trait, every algo crate that names `ObjectStore` transitively types-on the SDK.

`pyo3_async_runtimes`-style re-exports also leak: if `rollout-core` re-exports `bytes::Bytes` from `aws-smithy-types::byte_stream`, the SDK semver-pin propagates into the public surface.

**How to avoid:**
- The §2.3 `put_stream`/`get_stream` trait methods MUST be typed against `Pin<Box<dyn AsyncRead + Send>>` (tokio `AsyncRead`) and our own `ContentId` / `PutHint` types — **never** an SDK byte stream. STACK.md does not require this; ARCHITECTURE.md §2.3 does. Make it a hard rule.
- All cloud SDK errors collapse into `CoreError::Recoverable { Throttled | Transient }` / `CoreError::Fatal { Internal(String) }` at the **crate boundary inside `rollout-cloud-aws` / `rollout-cloud-gcp`**. The error chain must NOT use `#[source]` on an SDK type — store the rendered string only. (Loses some debuggability; preserves the lint.)
- **New CI gate:** `cargo public-api --diff-against=v1.0.0 -p rollout-core` should report no new types from `aws_*` / `gcloud_*` / `aws_smithy_*` crates. Add a `public-api-cloud-leak` job that greps `cargo public-api -p rollout-core` output for the prefixes `aws_`, `gcloud_`, `google_cloud_`, `aws_smithy_` and fails on any hit.
- New dep-direction **invariant #14** (extends ARCHITECTURE.md §6 which currently lists 10→13): `rollout-core::ObjectStore::Item` associated types and method signatures must not reference any crate matching `^(aws|gcloud|google-cloud|aws-smithy)-.*`. Implement via `cargo_metadata` walk of `rollout-core`'s direct dep graph: assert the closure contains zero AWS/GCP SDK crates.

**Warning signs:**
- `cargo doc -p rollout-core --no-deps` rustdoc shows any `aws_` / `gcloud_` symbol in a public-method signature.
- Reviewer says "I had to bump aws-sdk-s3 in `rollout-algo-sft`'s Cargo.lock" — the algo crate should not transitively depend on the SDK.
- `cargo tree -p rollout-algo-sft | grep -E '(aws-sdk|gcloud-)'` returns non-empty.

**Phase to address:** Phase 5 (Cloud), **first PR before any AWS impl crate lands.** The trait extensions are `rollout-core` changes; they go in first, with the lint as a co-landing test. If `rollout-cloud-aws` lands before invariant #14, retrofitting is much harder.

---

### Pitfall 2: localstack / fake-gcs-server "lies about prod" — passes locally, fails on real AWS/GCS

**What goes wrong:**
Code passes against localstack S3 + fake-gcs-server but breaks on production AWS/GCS because emulators do not replicate:
- **Strong-consistency edge cases** — real S3 is strongly consistent since Dec 2020 for read-after-write, but is *eventually* consistent on cross-region replication and on `ListObjectsV2` after rapid delete-then-list. Localstack is monotonic-per-process; fake-gcs-server is single-node strong. Production GCS read-after-write is strong only for new-object writes — **updates to existing objects are strong on GCS but list-after-write within a bucket can lag**.
- **Throttle / 503 SlowDown / 429** behavior — localstack rarely 503s; production S3 throttles per-prefix at ~3,500 PUT/COPY/POST/DELETE per partitioned prefix per second. Code that does `for sample in samples: object_store.put_stream(...).await?` against localstack works; against prod S3 with a hot prefix it gets `ServiceError(Throttling)` and the v1.0 `CoreError::Recoverable { Throttled }` path must engage. localstack also accepts unsigned requests by default — exposes credential-config bugs only in prod.
- **Error codes** — localstack returns `NoSuchKey`; some real-S3 paths return `404` with no body for `HeadObject` (HEAD requests don't carry the error code in body). Code that string-matches `e.code() == "NoSuchKey"` works locally; against prod HEAD it returns a different shape.
- **Multipart upload abort cleanup** — localstack auto-aborts incomplete multiparts after restart; prod S3 leaves them forever (billing surprise + Pitfall 4).
- **Pub/Sub emulator** lacks message ordering, lacks dead-letter-queue, lacks `ack_deadline`-based redelivery semantics that the real service uses; emulator redelivers on connection drop only.
- **fake-gcs-server resumable upload** doesn't implement the full retry/restart protocol; a real client doing `PUT /upload/storage/v1/b/bucket/o?uploadType=resumable&upload_id=...` with `Content-Range: bytes */SIZE` to query status is silently NOOPed against fake-gcs-server.

**Why it happens:**
Emulators are designed for the 80% happy path. Engineers treat "all green on localstack" as "ready for prod." The remaining 20% — throttling, large-object behavior, multipart cleanup, exact error-code matching — only surfaces under real-cloud load.

**How to avoid:**
- **Two distinct CI jobs per cloud, not one:**
  - `cloud-emulator-aws` (always-on, localstack) — covers happy path + the *injectable* error cases (use localstack's `FAILURE_INJECTION` env var or a wrapper layer that returns `503 SlowDown` on N% of requests).
  - `cloud-live-aws` (nightly/manual + on-PR if `crates/rollout-cloud-aws/**` touched) — real AWS, OIDC creds, runs the full conformance suite + a "hot-prefix throttle" stress test.
  - Mirror for GCP: `cloud-emulator-gcp` + `cloud-live-gcp`.
- **Conformance suite separated from emulator suite:** the conformance tests in `rollout-cloud-aws/tests/conformance.rs` parameterize over a `ConformanceTarget { Localstack | RealAws }`. The *same test* runs both ways. Any test that passes on localstack but fails on real AWS is the bug.
- **Throttle-path test fixture** named `throttled_put_recovers_via_retry_hint` — uses a fault-injecting middleware to return `503 SlowDown` on the first 3 PUTs; asserts `CoreError::Recoverable { Throttled }` is returned with a non-zero `RetryHint`, and the higher-level code (snapshotter) drives retry via the v1.0 error taxonomy. **Runs on every CI build, no real cloud needed.**
- **Documented "emulator delta" table** in `crates/rollout-cloud-aws/README.md` and `crates/rollout-cloud-gcp/README.md`: enumerate the four classes above; engineers reading the README know which behaviors are *only* validated on `cloud-live-*` jobs.
- **Error-code mapping is centralized** in `rollout-cloud-aws::error::map_sdk_error` — a single match arm per SDK error code → CoreError variant. Tests assert both `NoSuchKey` (GET) and `404 NotFound` (HEAD) map to the same `Fatal::Internal` or `Recoverable::Transient` outcome.

**Warning signs:**
- A test passes locally on `make test-cloud-aws-emulator` but fails the first time anyone runs `make smoke-multi-node-aws`.
- `aws s3 ls s3://.../multipart-uploads/` (or the equivalent `aws s3api list-multipart-uploads`) shows uploads older than 1 day — emulator never showed this; prod is leaking storage cost.
- Hot-prefix PUT pattern visible in code review: `format!("{prefix}/sample/{i}")` with sequential `i` — partitions on the prefix in S3.
- CI failure message contains "TooManyRequests" / "RateExceeded" / "throttled" — first time anyone has seen this; means the `cloud-emulator-*` jobs were not exercising the throttle path.

**Phase to address:** Phase 5. The emulator-vs-live discipline must be set up alongside the very first cloud impl PR; retrofitting after the AWS impl ships means the bugs were already shipped.

---

### Pitfall 3: IMDSv1 vs IMDSv2 — accidentally pinning to v1 or hand-rolling IMDS

**What goes wrong:**
Three failure modes:
1. Code uses a raw `reqwest::get("http://169.254.169.254/latest/meta-data/...")` for IMDS — that's IMDSv1, which AWS has been deprecating since 2020 and which many accounts have **disabled at the org level** (`MetadataOptions.HttpTokens=required`). Result: 401 from IMDS, `ComputeHint::preemption_signal` returns `Ok(None)` forever, spot drain never fires, worker gets reclaimed mid-step.
2. Code uses `aws-config` but instantiates with `BehaviorVersion::v2023_11_09()` or older — those versions defaulted to IMDSv1-fallback. Current `BehaviorVersion::latest()` is IMDSv2-only.
3. Code wraps `aws_config::imds::client::Client` but hard-codes `token_ttl(Duration::from_secs(60))` — short TTLs work but the 60s session token has to be refreshed; under sustained polling at 1Hz the IMDS endpoint can throttle at the per-instance per-credential-rotation cap.

**Why it happens:**
- "It worked on my dev EC2" because the dev account didn't enforce IMDSv2-required.
- Sample code from older AWS blog posts (2019-2021) uses IMDSv1 patterns; LLMs (us included) tend to regurgitate that pattern from training.
- Spot-preempt is rare; the failure is silent until the first real preemption.

**How to avoid:**
- **Hard rule:** STACK.md §1 mandates `aws-config::imds::client::Client`. The `rollout-cloud-aws::compute_hint` impl must call `aws_config::load_defaults(BehaviorVersion::latest()).await` and use `aws_config::imds::client::Client::builder().build()`. **No `reqwest::get("169.254.169.254/...")` anywhere in the workspace.**
- **CI grep gate:** add a `forbidden-patterns` check in CI that greps `crates/**/*.rs` for `169.254.169.254` and `metadata.google.internal` outside of `crates/rollout-cloud-aws/src/imds/*` and `crates/rollout-cloud-gcp/src/metadata/*`. Treat any other hit as a build failure.
- **Test fixture `imds_v1_disabled_falls_back_gracefully`:** point a localstack-imds-mock at the AWS impl with `HttpTokens=required` and v1 explicitly rejected; assert `ComputeHint::preemption_signal` still works (because we use `aws-config`'s IMDSv2 client). This is a negative test — if v1 code accidentally lands, this test fails.
- **Behavior version pinned at workspace level:** `rollout-cloud-aws::config::load_aws_config()` is a single function used everywhere; it locks `BehaviorVersion::latest()`. No call site re-implements config loading.
- **Polling cadence:** poll IMDS at 5s intervals (matches FEATURES.md §4 "polled at ≤5s"). Don't go below 1s — IMDS has per-instance rate limits.

**Warning signs:**
- `aws-sdk-*` SDK trace log contains `IMDSv1Fallback` — should never appear with `BehaviorVersion::latest()`. Add a `tracing` event filter that promotes this to an error.
- The string "169.254.169.254" appears anywhere in `git grep` outside the cloud-aws crate.
- Spot-drain integration test (`spot_drain_completes_within_lead_time`) hangs because `preemption_signal()` keeps returning `Ok(None)` — IMDS query is silently failing.

**Phase to address:** Phase 5, in the AWS `ComputeHint` PR (Phase 5 PR 5 per ARCHITECTURE.md §4). The CI grep gate is added the moment the first IMDS line of code lands.

---

### Pitfall 4: S3 strong consistency edges + multipart upload abort leak

**What goes wrong:**
Two related failures:
1. **`ListObjectsV2`-after-PUT race:** v1.0 `Snapshotter::list` enumerates `Snapshot.parts[].content` keys from `ObjectStore::list`. After a `put_stream` returns success, the object is read-strong-consistent on GET, but a `LIST` issued within milliseconds across a different bucket partition can omit the object if the bucket is using S3's globally-replicated partition index under contention. On the snapshot resume path, this manifests as "snapshot saved, snapshot not listed, restart loads previous snapshot, byte-identical resume test fails."
2. **Multipart upload abort leak:** when a `put_stream` for a 30 GiB checkpoint is interrupted (worker preempted, network drop, SDK retry exhausted, blake3 streaming error — see Pitfall 16), the underlying S3 multipart upload (initiated via `CreateMultipartUpload`) is **NOT auto-aborted by aws-sdk-s3**. Each unaborted upload sits in S3 *billable storage* indefinitely. After a week of churning spot workers each writing a partial 30 GiB tar, you have terabytes of "phantom" storage cost and no surface that warns you.

**Why it happens:**
- v1.0 was FS-only; `FsObjectStore::list` is process-local and atomic. The race doesn't exist there. Engineers carry the "list reflects PUT immediately" assumption into S3.
- `aws-sdk-s3` exposes `PutObject` (single-shot) and `CreateMultipartUpload`+`UploadPart`+`CompleteMultipartUpload`. The high-level `byte_stream` upload selects multipart automatically above ~8 MB. Bug: there is no built-in `Drop` impl that aborts on the underlying multipart. If the future is dropped between `UploadPart`s, the multipart is orphaned.

**How to avoid:**
- **Snapshot resume invariant is GET-by-key, not LIST-then-GET.** The current v1.0 `Snapshotter::restore` already takes a `SnapshotId` — confirm this code path does NOT do `list → pick latest`. If `Snapshotter::list_latest` is used anywhere on resume, replace with explicit `SnapshotId` from coordinator state (which is in v1.0 `Storage` — CAS-strong). **Plan-time validation rule:** `coordinator.last_committed_snapshot` is the resume pointer; resume CLI requires `--resume <snapshot_id>` (already true per v1.0 `rollout train sft --resume`).
- **Multipart abort on Drop:** wrap the S3 multipart upload in a guard struct in `rollout-cloud-aws::s3::MultipartGuard` that holds `(upload_id, key)` and on `Drop` (sync, since aws-sdk-s3 is async — must spawn an abort task on the runtime if not consumed cleanly) calls `AbortMultipartUpload`. Hard rule: every `put_stream` impl in `S3ObjectStore` constructs a `MultipartGuard`; only `.commit()` defuses the abort. Test fixture: `put_stream_dropped_aborts_multipart` — start an upload, drop the future mid-stream, assert `ListMultipartUploads` returns empty within 5s.
- **Lifecycle policy as belt to the suspenders:** the bucket setup playbook (in `crates/rollout-cloud-aws/docs/bucket-setup.md`) MUST include an S3 lifecycle rule that aborts incomplete multiparts after 1 day. This is the operator's defense if the Drop-guard fails on process crash (`SIGKILL` doesn't run Drop).
- **CI test against localstack with fault-injection:** `put_stream_network_drop_no_orphan_upload` — drop the connection mid-upload, assert no orphan multipart remains. localstack supports this via its HTTP error-injection layer.

**Warning signs:**
- S3 bill spikes 10x with no run-duration change → run `aws s3api list-multipart-uploads --bucket <bucket>` and look for old entries.
- The byte-identical resume test `bit_identical_resume_at_step_5_via_s3` (ARCHITECTURE.md §7) is flaky — passes most of the time, fails intermittently with "snapshot not found." Strong signal of the LIST-after-PUT race.
- `aws s3api list-multipart-uploads --bucket <bucket> --query 'Uploads[?Initiated<`yesterday`]'` returns rows.

**Phase to address:** Phase 5 (CLOUD-01 / CLOUD-03 PRs). The `MultipartGuard` is part of the `S3ObjectStore::put_stream` impl PR; cannot be retrofitted without rewriting the upload path.

---

### Pitfall 5: GCS resumable upload + retry semantics under spot preemption mid-upload

**What goes wrong:**
GCS resumable uploads use a `upload_id` in the URL; a client interrupted mid-upload can resume by issuing `PUT` with `Content-Range: bytes */SIZE` to query last-committed-byte, then `PUT` with `Content-Range: bytes N-SIZE/SIZE` to resume. **`gcloud-storage` (the Google official Rust SDK) handles this — but only within a single client invocation.** If the worker process dies between the initial `POST /upload/...?uploadType=resumable` and the first `PUT` chunk, the `upload_id` is in the SDK's in-memory state, lost on process exit. The next worker process initiates a *new* upload; the old `upload_id` becomes a 7-day orphan (GCS retains incomplete resumable uploads for 7 days).

Worse: under spot preemption (30s on GCP), the chunked upload may have committed N MiB. The new worker (after coordinator reassignment) starts the upload from byte 0 — wasting the 30s of partial progress AND breaking the blake3 incremental hash chain (see Pitfall 16) because the streaming hasher restarts from a different `Bytes::new()` boundary.

**Why it happens:**
- GCS resumable upload protocol is well-documented; SDK abstractions hide it, so engineers assume "it just works" across process restarts.
- The actual protocol semantics REQUIRE persisting the `upload_id` somewhere external if you want cross-process resume. Neither `gcloud-storage` nor `aws-sdk-s3` does this automatically — that's an application-level concern.

**How to avoid:**
- **Design rule (architectural):** Snapshot upload is **atomic per snapshot file**, not per byte. A snapshot tarball part is either fully uploaded (committed via final `Content-Range`) or not. If interrupted, the next worker re-uploads from byte 0. **Never persist `upload_id` to Storage** — the cost of re-uploading a single snapshot part (typically ≤ a few GiB at v1.1 model sizes; we are not training 70B in v1.1) is lower than the complexity of cross-process upload resume.
- **Content-addressed identity preservation:** because the upload key in `ObjectStore::put_stream` is `ContentId` (blake3 of the bytes), the re-uploaded file has the *same* key. If both the orphan partial AND the re-upload "succeed," the orphan partial is at a different (non-existent) key and gets cleaned up by lifecycle. **Critical:** `put_stream` MUST compute blake3 incrementally over the *complete* stream and only commit the final `PUT` if the hash matches the expected `ContentId` (or compute the ContentId on the fly and use it as the key). Truncated uploads must be aborted, not committed.
- **GCS bucket lifecycle policy:** `AbortIncompleteMultipartUpload` doesn't exist in GCS terms, but the bucket setup playbook (`crates/rollout-cloud-gcp/docs/bucket-setup.md`) must set a lifecycle rule: `condition.daysSinceCustomTime` paired with `action.type=Delete` on objects with no `customTime` set, OR just rely on the 7-day GCS implicit cleanup.
- **Test fixture `gcs_resumable_upload_preempted_mid_stream_reuploads_cleanly`** in `crates/rollout-cloud-gcp/tests/`: against fake-gcs-server, start a resumable upload, kill the future at byte 1 MiB, start a fresh upload (different SDK instance, same key), assert it succeeds and the final object's hash matches.
- **Plan-time validator rule:** `cloud.gcp.object_store.max_snapshot_part_bytes` ≤ 5 GiB by default. Above this, recommend sharded snapshot parts (the snapshotter already supports `Snapshot.parts[]`). Reduces re-upload-on-preempt waste.

**Warning signs:**
- Storage usage on the snapshot bucket grows monotonically even when no new runs start — orphan resumable uploads being retained for 7 days.
- Spot preemption tests show wall-clock for snapshot save > 30s on a 5+ GiB part — re-upload is starting over every preempt, and 30s is too short to finish.
- `gsutil ls -L gs://bucket/path/to/object` shows `Component-Count` other than 1 for a snapshot key — indicates a composed-from-chunks upload that didn't get cleaned up.

**Phase to address:** Phase 5 (CLOUD-02 PR for GCS object store). The atomicity invariant is baked into `put_stream`'s contract; cannot be retrofitted without rewriting.

---

### Pitfall 6: Work-stealing dedup race during fence-epoch flip

**What goes wrong:**
ARCHITECTURE.md §3 says coordinator restart bumps a fence epoch in `Storage` (`namespace="epoch"`, CAS-incremented). The standard pattern: workers carry the epoch they last observed in their heartbeats; coordinator rejects stale-epoch heartbeats. Race: between `t1=`worker A pulls item I at epoch=5` and `t2=`worker A's ack arrives at coord`, the coordinator restarts and bumps epoch to 6. Worker B pulls "stuck in flight" item I at epoch 6 (because lease expired). Worker A's ack arrives — coordinator at epoch 6 rejects it (stale epoch). Worker B finishes I separately. **If the operation is idempotent (sample with content-addressed `sample_id`), the v1.0 CAS state machine collapses the duplicate.** If the operation is **NOT** idempotent — and that's the trap — you get a double-execution.

For v1.1 specifically:
- **Batch infer**: idempotent (v1.0 CAS by `sample_id`). Safe.
- **Eval**: idempotent (eval items are content-addressed by `(suite, example_idx, model_id)`). Safe.
- **Tool harness `invoke()`**: NOT idempotent (a shell tool with side effects). Dangerous when tool calls become assignment payloads in v1.2 RL.
- **Snapshot save**: idempotent at the ContentId level but writes the same key twice → S3 PUT cost only, not correctness.

The deeper race is **lease expiration during ack-in-flight**:
- Worker A finishes item I; sends ack at t=29s; lease=30s.
- Coordinator's lease timer fires at t=30s; visibility-timeout reset; item I goes back to queue.
- Worker B pulls item I at t=30.5s.
- Worker A's ack arrives at t=31s; coordinator has already moved I back to pending; ack is silently dropped or returns "unknown assignment."
- Worker B runs I again. Double-execution.

**Why it happens:**
- Visibility timeout is a *probabilistic* invariant. The lease has to be longer than `(work_duration + ack_latency)` at p99.99, and coordinator-restart skews the budget.
- v1.0 batch infer was push-from-coordinator (BatchCoordinator drives BatchWorker); the pull semantics in §2.2 (`Coordinator::pull_work`) introduce the lease/visibility-timeout race for the first time.
- ARCHITECTURE.md §3 fence epoch is per-coord-restart, not per-assignment. It catches coordinator-restart staleness but NOT lease-expiration-during-ack races.

**How to avoid:**
- **CAS on item state, not on lease:** the v1.0 CAS state machine over sample IDs is the load-bearing dedup. When worker B "steals" item I, it must `cas(item_state, expected=PendingOrInFlight, new=ClaimedByB)`. If A's ack later races, A's `cas(item_state, expected=ClaimedByA, new=Done)` fails because state is `ClaimedByB`. A receives `CoreError::Recoverable::Transient` and discards. **This is the v1.0 invariant; v1.1 must keep it on `Coordinator::complete_work`.** Test fixture: `concurrent_ack_and_steal_no_double_execute`.
- **`extend_lease` for long-running ops:** STACK.md §2.1 already adds `Queue::extend_lease`. Workers running >50% of the lease budget must extend. Plan-time validator: `coordinator.worker_lease_timeout > 2 * max_estimated_work_duration` else warn at config validation.
- **Idempotency-key requirement on `Coordinator::pull_work`:** the returned `WorkAssignment` MUST carry an idempotency key (ContentId derived from the work item). Workers commit results keyed by this. Non-idempotent operations (tool calls with side effects) are not safe — flag them in the trait via a marker.
- **CI test name:** `coord_restart_no_duplicates` (FEATURES.md §3 already names this). Kill coord mid-batch with workers in `InFlight` state; new coord starts at epoch+1; workers send heartbeats and ack with their old epoch; assert no item runs twice AND no item is lost. Runs on every CI build via in-process simulation, no real cloud.
- **Postgres `scan_bytes` integration:** for the production multi-node path (Postgres-backed coord state), the CAS pattern on `WorkAssignment` must use `INSERT ... ON CONFLICT DO NOTHING RETURNING ...` not `SELECT-then-INSERT`. See Pitfall 17 for the related `scan_bytes` wildcard bug.

**Warning signs:**
- Eval test scores diverge run-to-run by 1-2 examples — same example was scored twice, or one was dropped.
- Batch infer output has duplicate `sample_id` keys (rare — v1.0 CAS catches this; if it happens, the CAS implementation regressed).
- Coordinator log shows `complete_work: assignment in unexpected state (ClaimedByB, expected ClaimedByA)` — this is the *correct* behavior signaling a near-miss; track the rate.

**Phase to address:** Phase 6 (DIST-02 work-stealing pull queue PR + DIST-03 coordinator restart PR). The CAS-on-state-not-on-lease invariant is the core design; if it's not in the first DIST-02 PR, the bug is shipped.

---

### Pitfall 7: Split-brain — old coordinator still running while new coordinator starts

**What goes wrong:**
A scenario:
- Coord process on host H1 hangs (deep GC pause, kernel hung for 5 minutes, network partition).
- Operator (or autoscaler) brings up a new coord on host H2. New coord reads state from Postgres, bumps fence epoch.
- H1 unhangs. Old coord still has all in-memory state. It thinks it's the only coord. It writes a worker assignment to Storage.
- Both coords are now "live." Workers connected to H1 use epoch 5; workers connected to H2 use epoch 6.
- Result: same work assigned to two worker pools, two snapshot ledgers, two ack streams. Catastrophic.

This is the **fencing problem.** Storage-based fence (epoch CAS) is necessary but not sufficient: it catches old-coord writes that race new-coord writes (epoch mismatch → CAS fail), but it does NOT prevent old-coord from continuing to communicate with workers that were never aware of the new epoch.

**Why it happens:**
- The "kill old, start new" sequence is operator-driven; there is no atomic "old process terminated" signal that storage can observe.
- ARCHITECTURE.md §3 fence epoch is in Storage. If the old coord has cached the worker-registry in memory and answers `pull_work` from its memory, no Storage write happens, no CAS fail, no detection.
- v1.0 deadline heartbeats (500ms/4s/5s) protect *workers from dead coord*, not coords from each other.

**How to avoid:**
- **Workers MUST validate coordinator epoch on every RPC.** The `Coordinator` tonic service includes a `coord_epoch` field in every response. Workers track the highest epoch seen; reject lower-epoch responses (treat as `Fatal::Internal("stale coordinator")`); refuse to talk to a coord at lower epoch than they've already seen. Reverse fencing: the worker fences the coord. Implementation: `Coordinator::pull_work` returns `WorkAssignment { coord_epoch, ... }`; worker compares to local `latest_known_epoch` (persisted in worker's redb so it survives worker restart).
- **Heartbeat carries epoch in BOTH directions:** worker sends "epoch I last knew = N"; coord rejects worker if `coord.current_epoch != N + k` for small k (sliding window allows for one in-flight restart); coord also returns its `current_epoch` so worker can advance.
- **Postgres-side: `coordinator_lease` row, single-row table.** Coord on startup does `UPDATE coordinator_lease SET host=$1, epoch=$2, expires_at=now()+10s WHERE expires_at < now() RETURNING ...` — if zero rows returned, old coord is still holding the lease, new coord must wait or abort. Old coord on the way down (or at heartbeat tick) reasserts the lease; if it ever sees `expires_at < now()`, it self-fences immediately (`std::process::abort`). This is `pg_advisory_lock`-style; we use a plain UPDATE because v1.0 already has Postgres without the lock extension.
- **Self-fence on stale-lease detection:** coord runs a "I'm still leader" check every 2s. If the check returns "you've been usurped," coord calls `std::process::abort()` — do NOT try to graceful-shutdown, because graceful-shutdown is the bug (the hung GC was the graceful-shutdown machinery, possibly).
- **CI test `split_brain_old_coord_self_fences`:** spawn two coord processes against the same Postgres; assert exactly one of them is responsive to RPCs after 5s, the other has exited.
- **Plan-time validator:** if `coordinator.mode = "multi_node"`, refuse to start without `coordinator.fence_lease_seconds` set (default 10s) and `storage.backend = "postgres"` (already in ARCHITECTURE.md §5; reinforce here because this is the load-bearing surface).

**Warning signs:**
- Two `coordinator: starting` log lines from different hosts within a 1-minute window.
- Worker logs `coord_epoch downgrade: was 6, got 5` — proves split-brain in flight.
- Two distinct `WorkerId` allocations for the same physical worker host — old and new coord both registered it.

**Phase to address:** Phase 6 (DIST-01 + DIST-03). This is THE hardest pitfall in v1.1 (FEATURES.md §3 already flags DIST-03 as "highest complexity-to-precedent ratio"). The lease-based fencing must be in the first multi-node coord PR.

---

### Pitfall 8: Spot-preemption notice is a HINT, not a guarantee

**What goes wrong:**
AWS docs say "2-minute warning"; GCP docs say "30-second warning (120s for some preview features)". In practice:
- AWS sometimes provides less than 2 minutes (terminations happen earlier under capacity pressure). Documented but not loudly.
- GCP's 30s can be 20s in some zones; the metadata flag flips and you have *less* than the documented lead.
- The signal is **not delivered to the application** by the cloud — the application polls. If your poll interval is 5s and the notice fires at t=29s (GCP), your worker observes the notice at t=24s budget (because it noticed 5s after the flip in worst case).
- The drain protocol itself takes time: snapshot save (5-30s for v1.1 model sizes), ack to coordinator (network round-trip + Postgres write, 100ms-1s), deregister (1s). If drain takes 25s and you noticed with 24s left, you're racing.

**Why it happens:**
- Cloud preemption is fundamentally a *cooperative* notice. The cloud will reclaim regardless of whether the worker has finished.
- v1.0 has `Recoverable::Preempted` in the error taxonomy but no enforced drain protocol — that's v1.1's job.

**How to avoid:**
- **Drain protocol must be idempotent — can be interrupted and resumed.** Specifically: every step in drain (`drain_request` to coord, snapshot save, deregister) is a separate transaction. If preempted mid-drain, the *next* worker (resuming the work) sees the partial state and recovers via the v1.0 CAS state machine.
- **Conservative budget config:** `coordinator.preemption_drain_lead = "60s"` for AWS (use only 60s of the 120s budget; treat the remainder as slack), `"15s"` for GCP. Plan-time validator: `cloud.aws.preemption_drain_lead <= 90s` AND `cloud.gcp.preemption_drain_lead <= 25s`. Above these, refuse to start with `Fatal::ConfigInvalid("preemption budget exceeds cloud-documented lead")`.
- **Snapshot-on-drain decision tree (per FEATURES.md §4):** if `time_remaining < snapshot_estimate * 2` → skip snapshot, just requeue. Compute `snapshot_estimate` from the previous snapshot duration (track in worker state).
- **Poll IMDS / metadata at 5s** (matches FEATURES.md). Don't poll faster — IMDS rate-limits. Don't poll slower — the lead time is small.
- **CI test `spot_drain_mid_drain_preemption_no_duplicate_work`:** mock preemption signal at t=0; mid-drain (during snapshot save), inject a *second* (sooner) hard-preempt signal; assert the next worker resumes cleanly, no work duplicated, no work lost.
- **Drain test budget budget assertion:** `spot_drain_completes_within_60s` for AWS-mode, `_within_15s` for GCP-mode. Hard CI gate.

**Warning signs:**
- Production metric `drain_partial_count` non-zero — drain was interrupted by hard-preempt before completion.
- Metric `drain_skipped_snapshot_count` non-zero — budget was too small to snapshot. Cumulative loss of training progress.
- Spot preemption notice observed in coord log but no corresponding `drain_request` — worker IMDS polling is broken (see Pitfall 3).

**Phase to address:** Phase 6 (DIST-04). The drain protocol idempotency invariant is the core design; cannot be retrofitted.

---

### Pitfall 9: Cross-provider snapshot resume (AWS → GCP) and back

**What goes wrong:**
The byte-identical resume invariant says: snapshot saved at step N + restore at step N → byte-identical model state. The content-addressed identity (`Snapshot.parts[].content` = blake3) **should** make cross-provider portable: copy the bytes from S3 to GCS, the ContentId is the same, the restore code path is provider-agnostic.

What breaks in practice:
1. **PROJECT.md says cross-cloud single run is Out of Scope.** But the codebase will support it accidentally because of content-addressing. Operators will try it (transfer a snapshot from prod-AWS to dev-GCP). Subtle breakages:
   - `Snapshot` metadata `provenance` field references the source bucket URL (`s3://...`). Re-uploading to GCS without rewriting metadata leaves stale references; resume code path that does any kind of provenance-trust check breaks.
   - blake3 hash must be computed over the *unwrapped* bytes, not over the (potentially provider-encoded) bytes. If `S3ObjectStore::get_stream` returns bytes with an AWS-added `Content-MD5` header transparently consumed, vs `GcsObjectStore::get_stream` which doesn't, the streaming hasher could see different byte counts.
2. **Streaming get on one provider, streaming put on another:** the §2.3 trait defaults say "buffers into Vec<u8> and delegates to put_bytes (slow path)." If `S3ObjectStore::get_stream` is overridden but `GcsObjectStore::put_stream` is not, a 30 GiB transfer ends up buffering 30 GiB in RAM. OOM.
3. **Storage layout differences:** `FsObjectStore` uses sharded path layout `cas/ab/cd/efgh...`. `S3ObjectStore` and `GcsObjectStore` impls might use a flat layout or different shard depth. If one of the cross-provider tests reads `s3://bucket/cas/ab/cd/...` keys but the GCS impl wrote them as `gs://bucket/abcd...`, the restore code path that hard-codes a prefix mismatches.

**Why it happens:**
- Engineers assume content-addressing solves everything. It solves *byte identity*; it does not solve *naming conventions*.
- The "snapshot is just bytes" intuition skips over the (real) metadata sidecar that v1.0 already produces in `Snapshot` rows (`Storage` namespace `"snapshots"`).

**How to avoid:**
- **Make snapshot self-contained:** `Snapshot.parts[].content` is the ONLY load-bearing reference. NO field in `Snapshot` may reference a bucket URL, a region, or a provider. The restore code path takes `(Snapshot, Arc<dyn ObjectStore>)` and calls `object_store.get_stream(content_id)`. Provider is determined by which `ObjectStore` impl is in scope.
- **Object-store key layout is part of the `ObjectStore` impl contract, not the snapshot metadata.** Each cloud impl decides its own key layout for a given `ContentId`. The trait contract: `put_bytes(bytes) → ContentId` then `get_bytes(ContentId) → bytes` round-trips. Test fixture `cas_key_layout_round_trip` for each impl.
- **Cross-provider conformance test fixture `cross_provider_resume_via_explicit_transfer`:** save snapshot via `S3ObjectStore` (localstack); manually `aws s3 cp` to a directory; `aws s3 cp` to fake-gcs-server; restore via `GcsObjectStore`. Assert byte-identical model state. **This is exercised only on `cloud-emulator-{aws,gcp}` jobs, never the always-on path** (because cross-cloud is Out of Scope per PROJECT.md and the operator path of "transfer snapshot" isn't a v1.1 CLI surface).
- **Document in `Snapshot` rustdoc:** "Snapshots are content-addressed. The same Snapshot can be restored from any `ObjectStore` impl that holds the referenced bytes. Provider/region/bucket are NOT part of the snapshot identity." Add to `docs/book/src/snapshots.md`.
- **Streaming default impl audit:** the §2.3 default `put_stream` / `get_stream` that buffers into `Vec<u8>` is correct for `FsObjectStore` (small RAM cost in dev) but a footgun in cloud impls. Add an `#[deprecated(note = "Override for cloud-backed stores; default buffers")]` on the default impl, OR make the trait method NOT default-impl'd — force every cloud impl to write a streaming version. ARCHITECTURE.md §2.3 says "Default: buffers into Vec<u8>" — flag for revision.

**Warning signs:**
- A user reports "I copied my snapshot from S3 to my laptop, restore says hash mismatch." Almost certainly cause: a `Snapshot.parts[].content` reference includes provider-side encoding.
- Test `cross_provider_resume_via_explicit_transfer` reports OOM — `get_stream` default impl is buffering.
- Restore code path stack trace contains any string literal "s3://" or "gs://" — there's hidden provider coupling.

**Phase to address:** Phase 5 (CLOUD-03). The "self-contained snapshot" invariant must be re-verified during the snapshot-storage-via-ObjectStore PR. The cross-provider conformance test is an optional fixture but cheap to add.

---

### Pitfall 10: Sandbox escapes in the tool harness

**What goes wrong:**
The threat model from STACK.md §6 and PROJECT.md is **best-effort sandbox: process isolation + resource limits + path/HTTP allowlist; gVisor/Firecracker out.** Multiple sub-pitfalls, each independently catastrophic:

**10a. `subprocess.Popen(shell=True)` in the Python sidecar for shell tool:**
Calling `Popen(cmd, shell=True)` invokes `/bin/sh -c cmd`. If `cmd` is constructed from user input (LLM output → shell tool call args), classic command injection: `ls; rm -rf /`. The allowlist of "allowed commands" doesn't help because `/bin/sh -c` is the entry point.

**10b. Path-traversal in file tool:**
"Path allowlist: `/tmp/sandbox/`" is naïvely implemented as `if path.starts_with("/tmp/sandbox/"): allow`. Defeats:
- `/tmp/sandbox/../etc/passwd` — Rust's `Path::starts_with` is a *literal-component* match (not lexical), but if you do `format!("/tmp/sandbox/{user_path}")` and let `..` through, the OS resolves it.
- Symlinks: place a symlink `/tmp/sandbox/escape -> /etc/passwd`. `starts_with` passes; `open()` reads `/etc/passwd`.
- TOCTOU: check the path, then a separate `open()` syscall — between check and use, attacker swaps a symlink in.
- Hardlinks: similar but harder to detect.

**10c. SSRF in HTTP tool — domain allowlist is insufficient:**
"HTTP allowlist: `api.openai.com`" is implemented as `if url.host() == "api.openai.com": allow`. Defeats:
- DNS rebinding: `api.openai.com` first resolves to public IP (allowed), then on the *next* DNS lookup (when the HTTP client actually connects) resolves to `169.254.169.254` (IMDS!), `10.0.0.1` (internal), or `127.0.0.1`.
- Direct IP: `http://169.254.169.254/...` if the allowlist is by domain only.
- Redirects: `https://api.openai.com/redirect?to=http://169.254.169.254/...` — the HTTP client follows.
- IPv6: `http://[::ffff:127.0.0.1]` — embedded IPv4.

**10d. Kernel-version requirements:**
- `landlock` requires kernel ≥ 5.13. Ubuntu 22.04 LTS ships 5.15 — OK. RHEL 8 ships 4.18 — **landlock unavailable.** RHEL 9 ships 5.14 — borderline; check ABI v1 features available. Amazon Linux 2 (still common) ships 5.10 — landlock unavailable. Without explicit detection, code that assumes landlock-always-works falls back to no FS isolation silently.
- `clone3` is available since 5.3 but seccomp filters need explicit allow for `clone3` separately from `clone` — many tutorials filter `clone` and miss `clone3`; modern glibc uses `clone3` by default (since glibc 2.34). Result: any sandboxed process can't start child processes at all.
- `openat2` (5.6+) vs `openat` similarly — newer glibc may use `openat2` for `RESOLVE_BENEATH` semantics; seccomp allowlists that miss it break file ops.

**10e. seccomp allowlist gotchas beyond clone3/openat2:**
- `mmap` vs `mmap2`: 32-bit vs 64-bit. We're x86_64/aarch64 only per STACK.md §6, but `mmap` has multiple flag variants — `MAP_GROWSDOWN` lets you grow stack maps unexpectedly.
- `prctl(PR_SET_DUMPABLE)` and `prctl(PR_CAPBSET_DROP)` — needed to drop caps cleanly, often forgotten.
- `arch_prctl` for setting TLS — required by Rust runtime; forgotten allowlists break threads.
- `rt_sigprocmask`/`rt_sigaction` — every Rust binary uses these for panic handling; if missed, segfaults instead of clean panics.

**Why it happens:**
- "Best-effort sandbox" is interpreted as "we tried"; the failure modes are unintuitive.
- Tutorials and example seccomp filters from blog posts circa 2020 are missing post-2020 syscalls (`clone3`, `openat2`, `faccessat2`).
- Path-traversal protection is easy to get wrong; the correct primitive is `cap-std` (already in STACK.md §6) or `realpath`-then-`open` atomically, which most engineers don't reach for.

**How to avoid:**

For **10a:**
- Hard rule: `subprocess.Popen(shell=True)` is BANNED in `rollout-harness-tool`'s Python sidecar. Add a `forbidden-patterns` CI grep that fails on `shell=True` in `crates/rollout-harness-tool/**/*.py`. Use `Popen([cmd, *args], shell=False)` exclusively, with `cmd` validated against an exact allowlist of binary paths.
- Allowlist is an exact-match list of *full paths* (`/usr/bin/python3`, `/usr/bin/wc`), not command names. `which python3` resolution is done at sandbox-init, not per-invocation.

For **10b:**
- Use `cap-std::fs::Dir::open_dir(allowlist_root)` and only operate on paths *within* that capability. `cap-std` rejects `..` and symlink escapes by construction.
- Resolve final path with `cap_std::fs::Dir::canonicalize`, then assert `canonicalized.starts_with(allowlist_root)` *after* canonicalization. (`cap-std` does this internally; this is the belt to the suspenders.)
- Open file with `O_NOFOLLOW | O_CLOEXEC | O_RESOLVE_BENEATH` (via `openat2` — STACK.md `rustix` allows this) — TOCTOU-safe.

For **10c:**
- HTTP tool DOES NOT use domain allowlist. Uses **IP allowlist applied after DNS resolution but before TCP connect**. Implementation: custom `hyper` `Connect` impl that:
  1. Resolves DNS.
  2. Filters the resolved IPs: reject anything in RFC1918 (10.0.0.0/8, 172.16/12, 192.168/16), RFC6598 (100.64/10), link-local (169.254/16, fe80::/10), loopback (127/8, ::1), CGNAT, IPv4-mapped-IPv6, multicast.
  3. Reject if the resolved set intersects with these AND with the allowlist (defense against split-horizon DNS).
  4. Pin the resolved IP for the duration of the connection (defends against DNS rebinding).
- Follow-redirects must re-apply the same filter on each redirect, including IP filter (not just domain).
- Use `hyper-rustls` directly with the custom Connect impl; do NOT use `reqwest` default behavior which auto-follows redirects without an injection point for re-validation.
- CI test fixtures: `http_tool_blocks_dns_rebinding`, `http_tool_blocks_redirect_to_imds`, `http_tool_blocks_rfc1918`, `http_tool_blocks_ipv6_loopback_v4_mapped`.

For **10d:**
- Plan-time validator: at sandbox-init, detect kernel version via `uname()`. If `< 5.13`, set `landlock_available = false`. If `harness.tool.require_landlock = true` (default), refuse to start with `Fatal::ConfigInvalid("landlock requires kernel >= 5.13, found {n}")`. Operators on RHEL 8 must explicitly opt into `require_landlock = false` (accepting reduced isolation).
- CI matrix runs sandbox tests on Ubuntu 22.04 (kernel 5.15) AND if possible on a RHEL 8-equivalent. At minimum, document in `crates/rollout-harness-tool/README.md` that RHEL 8 has reduced isolation.

For **10e:**
- Seccomp allowlist is built **from a curated set** in `rollout-harness-tool::seccomp::ALLOWLIST`, with a documented justification per syscall. Use `strace -c /usr/bin/python3 -c 'print(1)'` as a sanity check during development to find missing syscalls.
- Explicit allow for: `clone3` (with `CLONE_VM | CLONE_FS | CLONE_FILES | CLONE_SIGHAND | CLONE_THREAD | CLONE_SYSVSEM | CLONE_SETTLS | CLONE_PARENT_SETTID | CLONE_CHILD_CLEARTID` only — refuse `CLONE_NEWUSER`), `openat2`, `faccessat2`, `pidfd_send_signal`, `rseq`, `prctl(PR_SET_DUMPABLE/PR_GET_DUMPABLE/PR_SET_NAME/PR_CAPBSET_DROP)`, `arch_prctl`, `rt_sigprocmask`, `rt_sigaction`, `rt_sigreturn`.
- Test `seccomp_blocks_unexpected_syscall`: invoke a tool that tries `ptrace(PTRACE_TRACEME)`; assert EPERM.
- Test `seccomp_python_runs`: invoke a Python interpreter under the seccomp filter; assert it can `print("hi")` (catches missing arch_prctl / rseq / etc.).
- Test `seccomp_no_socket`: invoke a tool that tries `socket(AF_INET, SOCK_STREAM, 0)`; assert EPERM (sandbox should have no network).
- Negative test fixture per CVE class: `sandbox_blocks_userns`, `sandbox_blocks_mount`, `sandbox_blocks_keyctl`, `sandbox_blocks_bpf`.

**Warning signs:**
- The string `shell=True` appears anywhere in `crates/rollout-harness-tool/**/*.py`.
- The string `169.254` or `metadata.google.internal` resolved from a tool invocation in test logs.
- A sandboxed test segfaults instead of cleanly returning `EPERM` — likely a missed signal-handling syscall in the seccomp allowlist.
- New CVEs in landlock or seccompiler are released and the workspace pins are stale — track via `cargo audit`.

**Phase to address:** Phase 7 (HARNESS-02). The tool harness PRs are the only place to enforce these. **Each sub-pitfall (10a-e) gets its own test fixture and must be in the PR that introduces the corresponding tool.** A tool harness without these tests is a known-broken security surface.

---

### Pitfall 11: Eval harness — synchronous scoring blocks the rollout

**What goes wrong:**
The eval harness `EvalHarness::evaluate(&completion) -> EvalResult` (ARCHITECTURE.md §2.5) is synchronous in shape. If a v1.2 RL loop calls `evaluate` for every rollout step to compute reward (or for periodic mid-training eval), and `evaluate` does any heavy work (loads a 14k MMLU dataset on each call, runs a Python-side scorer, makes an HTTP call to a grader service), the rollout thread blocks. GPU utilization drops from 80% to 5%. The v1.0 perf bar (≥80% GPU util on rollout) — a v1.2 perf-bar concern but seeded by v1.1 design — is missed.

Additional v1.1-specific footguns:
- **lm-eval-harness compatibility boundary** (FEATURES.md §7 marks this as P2/v1.2): users will *try* to run our eval harness on a lm-eval-harness YAML task and get inconsistent scores (off-by-one in normalization, different prompt formatting). They will report bugs on our scoring rather than on lm-eval compat. Documentation must mark our scoring as authoritative for our suites; lm-eval-harness compat is a separate v1.2 feature.
- **MMLU scoring divergence:** there are at least three published MMLU scoring conventions (HELM uses raw multi-choice probability; Eleuther's lm-eval uses `acc`, `acc_norm`, and `byte_norm`; some papers report `acc` only on subset-shots). Our impl must declare which.
- **IFEval constraint checker** depends on `langid` or `langdetect` for language detection in some constraints; pulling those is a Python sidecar tax. Our pure-Rust scorer must either reimplement language detection (hard) or document that the language-related constraints are skipped.

**Why it happens:**
- "Eval" sounds simple. It is, until you try to run it in the inner loop of an RL trainer.
- Scoring details vary by paper; "MMLU score" is ambiguous.

**How to avoid:**
- **Eval runs as a separate `WorkQueue` job** (FEATURES.md §7 already states this — reinforce). `EvalHarness::evaluate` should not be called synchronously in a training inner loop. The architecture: training loop enqueues "eval me at step N"; eval harness pulls from queue; eval results returned via Storage, polled by training loop. **Plan-time validator:** if `[rl].mid_training_eval.enabled = true`, require `[rl].mid_training_eval.async = true` (default).
- **Dataset caching is sticky:** `EvalHarness::load_dataset` is called once per process; cached in `Arc<EvalDataset>`. Test fixture `eval_loader_caches_dataset_across_calls` asserts `load_dataset` is called exactly once across 100 `evaluate()` calls.
- **MMLU scoring convention is documented as `acc` (Eleuther convention)** in `crates/rollout-harness-eval/README.md`. Spec the exact regex / argmax rule. CI test `mmlu_score_matches_eleuther_acc_on_fixture` runs the v1.0 fixture (10 examples), asserts score is exactly the published reference score for those examples (or ±1% if floating-point).
- **IFEval language-detection constraints are skipped, documented as such.** Plan-time validator emits a warning at load-time: "IFEval: skipping N language-detection constraints (not yet supported in v1.1)."
- **Eval suite versioning:** `Eval.suite_version = "mmlu_v1"` is included in the `ContentId` of the result. If we change scoring rules in v1.2, the old results are not silently invalidated.

**Warning signs:**
- Profiling shows `evaluate()` called from the same thread as the training step.
- Eval scores differ from published reference by >5% — likely scoring convention mismatch.
- "MMLU eval taking 30 minutes" — dataset loaded per call instead of cached.

**Phase to address:** Phase 7 (HARNESS-03). The trait shape and async-job pattern are PR-1 territory; cannot be retrofitted because v1.2 RL phases will hard-code the pattern.

---

### Pitfall 12: HuggingFace dataset download in CI without HF_TOKEN

**What goes wrong:**
The eval harness loads MMLU/IFEval/GSM8K from HuggingFace via `hf-hub` (STACK.md §7). Without `HF_TOKEN`:
- Anonymous rate limits are ~few hundred requests/hour from a single IP. CI on shared GitHub Actions IPs (cross-runner traffic) hits this fast; first PR to land succeeds, next 5 fail with 429.
- `cais/mmlu` is public and ungated; works anonymously. `openai/gsm8k` is public. **`google/IFEval` is public but the org-level rate-limit posture is stricter** — historically subject to anonymous-rate stricter caps.
- Some HF datasets (not these three by default, but if anyone adds future ones) are *gated*: require accepting a license at https://huggingface.co/datasets/... before access. Anonymous access fails with 403 even with the right URL.
- HF mirror URLs change occasionally; hardcoded URLs break.

Specific v1.1 footgun: the test fixture set must work *air-gapped* per FEATURES.md §7 ("Offline mode is the default"). If a developer runs `cargo test -p rollout-harness-eval` on a flight, the test must pass.

**Why it happens:**
- "Just download from HF" is the path of least resistance.
- HF_TOKEN is not surfaced as a hard requirement in the eval crate's README; new contributors don't set it.

**How to avoid:**
- **Ship 10-row fixtures per suite** in `crates/rollout-harness-eval/tests/fixtures/{mmlu_10.parquet, ifeval_10.parquet, gsm8k_10.parquet}`. These are content-hash-pinned, committed to the repo, and are the source for the always-on CI eval tests.
- **`HF_OFFLINE=1` env var is the default for `cargo test`.** When set, the `hf-hub` loader is bypassed; loaders read the local fixture. Documented in the crate README.
- **`cloud-emulator-*` jobs do NOT pull from HF.** Only the nightly `hf-dataset-refresh` job validates that the real HF URLs still resolve and content hashes still match the pinned values. Failure → flag for vendoring or version bump.
- **Test fixture `eval_loader_works_with_no_network`:** unset `HTTP_PROXY` / `HTTPS_PROXY`, set `HF_HUB_OFFLINE=1`, run all suite loaders; assert all pass using fixtures.
- **Plan-time validator:** if `harness.eval.suite = "mmlu"` AND no fixture present AND no `HF_TOKEN` AND not interactive → emit `Fatal::ConfigInvalid("eval suite mmlu requires HF_TOKEN or vendored fixture")` at load.
- **Vendor the test split SHAs:** in `crates/rollout-harness-eval/src/datasets/mmlu.rs`, hardcode `const MMLU_TEST_BLAKE3: &str = "..."`. On first download, blake3 the downloaded file; if mismatch, fail loudly. Detects HF-side drift.

**Warning signs:**
- CI `rollout-harness-eval` test job intermittently fails with `429 Too Many Requests` or `403 Forbidden`.
- A run reports `loaded MMLU: 14042 examples` on one CI run and `14040` on another — drift on HF side.
- Dev says "cargo test fails on the train, network is bad" — likely missing offline-mode default.

**Phase to address:** Phase 7 (HARNESS-03), PR 5 (rollout-harness-eval crate). The fixture + offline-default discipline must be in the first PR.

---

### Pitfall 13: PyO3 + fork — fork-after-PyO3-init is broken

**What goes wrong:**
v1.0's BACKEND-01 ships the PyO3 dedicated-thread pattern: a single Tokio task owns the Python interpreter, asyncio↔Tokio bridge via `pyo3_async_runtimes::tokio::run_until_complete`. **`fork()` after PyO3 has been initialized is broken** — Python's interpreter holds locks that don't survive a fork, async/await state lives in `asyncio` event loop that the child can't recover. Result: deadlocks, segfaults, or silent corruption.

v1.1 multi-node worker spawn pattern is the trap. Three plausible (and wrong) patterns:
1. **Coordinator pre-init PyO3 then `fork()` workers** — classic "fork to save startup time" pattern. Each worker is a child process inheriting the locked Python state. Deadlocks on first vLLM call.
2. **Worker process is multi-threaded and spawns tool harness via `fork()` (not `posix_spawn`/`execve`)** — for fork-then-exec, only the calling thread survives in the child; PyO3 state is whatever the calling thread had locked, which may include the GIL → child immediately deadlocks.
3. **PyO3 sub-interpreters** — PyO3 0.28 marks sub-interpreters as experimental; they don't have isolated GIL state in current CPython (CPython 3.12+ has per-interpreter GIL, but PyO3 0.28's sub-interpreter API does not safely use it). Using sub-interpreters for "isolate one vLLM per worker" sounds appealing; will explode.

**Why it happens:**
- "fork is fast" is engineering folklore.
- The dedicated-thread pattern is documented for v1.0 BACKEND-01 (per PROJECT.md), but the multi-node spawn pattern hasn't been written down yet; risk of regressing the dedicated-thread invariant.

**How to avoid:**
- **Hard rule:** worker processes spawn via `posix_spawn` (or `execve` after `fork()`, which discards the parent's memory image — equivalent for our purposes). NO `fork()` followed by Python code in the child. Document in `crates/rollout-runtime-batch/src/spawn.rs` rustdoc.
- **Worker is its own process from `main()`**, not a forked sub-process of a long-running parent. The coordinator launches workers via `tokio::process::Command::new("rollout").args(["worker", ...])` — execve, not fork. (This is the natural pattern anyway; just don't optimize it later.)
- **No PyO3 sub-interpreters in v1.1.** Document in PROJECT.md as a v1.2+ concern, gated on PyO3 0.29+ and per-interpreter GIL stabilization.
- **Tool harness subprocess spawn uses `Command::new(...).spawn()`** which goes through `posix_spawn`/`execve` on Linux. Hard rule: NO `unsafe { libc::fork() }` anywhere in the workspace. CI grep gate: `forbidden-patterns` checks for `libc::fork(` in the workspace.
- **Test fixture `worker_spawn_after_pyo3_init_works`:** in a parent process, initialize PyO3 (vLLM-feature-off, just `pyo3::prepare_freethreaded_python()`); spawn a worker process via `Command::new`; assert worker boots and reports vLLM-init-success (or vLLM-not-installed-skipped). Detects accidental fork patterns.

**Warning signs:**
- Worker process hangs immediately after coordinator-spawn with `py.import("vllm")` in the stack trace.
- `libc::fork(` appears in code review.
- `tokio::process::Command::new` is replaced with anything that doesn't go through execve.

**Phase to address:** Phase 6 (DIST-01 multi-node coord+worker). The spawn pattern is decided in PR 4 (worker-side pull loop per ARCHITECTURE.md §4).

---

### Pitfall 14: cargo-deny + AWS SDK transitive license deltas (aws-lc-rs)

**What goes wrong:**
v1.0 cargo-deny has a full license allowlist + openssl bans. STACK.md §1 mandates `aws-sdk-s3 1.112.0` with `default-https-client` feature → pulls `aws-lc-rs` (ISC OR (Apache-2.0 AND OpenSSL)) → the **OpenSSL license** is in the transitive license set.

The OpenSSL license is *not* OpenSSL the *crate* — it's a license text used by aws-lc-rs because aws-lc is forked from BoringSSL which is forked from OpenSSL. cargo-deny applies license checks not crate-name bans; the openssl-ban in our v1.0 deny.toml is on the *crate name* `openssl-sys` (or similar). But the *license* `OpenSSL` (the SPDX identifier) is a separate question.

Two failure modes:
1. **License `OpenSSL` is not on our allowlist** → cargo-deny fails the build the moment `aws-sdk-s3` is added. CI red.
2. **License `OpenSSL` is silently allowed** (because someone added it to the allowlist without scrutiny) → we are now legally redistributing under the OpenSSL license, which has the "advertising clause" (3-clause variant). For MIT-licensed projects this is technically a compatibility question that should be reviewed by counsel before shipping a 0.1.0 release. (The 4-clause "advertising" requirement is the one that's been historically problematic; OpenSSL has both the 3-clause-with-advertising and modern Apache-2.0 dual licensing — verify which aws-lc-rs uses.)

Additional v1.1 transitive license footguns:
- **`aws-lc-rs`** has an `ISC OR (Apache-2.0 AND OpenSSL)` triple. ISC is OK; Apache-2.0 is OK; OpenSSL is the question.
- **`cap-std`** is `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` (STACK.md §6 already flags this) — `WITH LLVM-exception` may not be in the allowlist.
- **gcloud-* crates** are all Apache-2.0 (clean); googleapis-tonic if pulled transitively might have a different license.
- **`landlock`**, **`seccompiler`**: STACK.md confirms Apache-2.0 OR BSD-3-Clause. BSD-3 must be in allowlist.

**Why it happens:**
- License changes in transitive deps are easy to miss.
- aws-lc-rs being a "default" is a recent (2024) decision in aws-sdk-rust; older code samples reference `rustls-only` features that no longer exist.

**How to avoid:**
- **Pre-merge cargo-deny check on a representative cloud-feature build:** add a new CI job `deny-cloud-features` that runs `cargo deny check --all-features` and additionally `cargo deny check --features aws,gcp,vllm,sandbox`. Distinct from the `deny` job, which runs `--no-default-features`.
- **License-allowlist additions are PR-reviewed:** any change to `deny.toml`'s `[licenses].allow` is a hard human-review gate. Document in `AGENTS.md §9` (the standing-rules file) that adding a license requires a one-paragraph justification in the PR description.
- **Audit aws-lc-rs license at integration time:** the actual license declared by `aws-lc-rs 1.x` on crates.io — verify `cargo metadata --format-version 1 | jq '.packages[] | select(.name == "aws-lc-rs") | .license'`. If it returns `ISC AND (Apache-2.0 OR ISC) AND OpenSSL` we need to allowlist `ISC` AND `OpenSSL`. If it returns only `ISC OR Apache-2.0` (modern alternate), we're clean.
- **`OpenSSL` SPDX identifier is denied by default:** `deny.toml` should EXPLICITLY have `OpenSSL` in `[licenses].deny`, not just be silent on it. Forces a documented opt-in if and when we accept it.
- **`Apache-2.0 WITH LLVM-exception`:** STACK.md §6 already flags this for cap-std; add to `deny.toml` only if we choose to accept it. Alternative: drop cap-std (`rustix` covers most of what we need) — flagged as fallback in STACK.md.

**Warning signs:**
- `cargo deny check --features aws` fails with `license `OpenSSL` is denied` or `license `Apache-2.0 WITH LLVM-exception` is unknown`.
- A PR adds 3+ licenses to `deny.toml`'s `[licenses].allow` without justification.

**Phase to address:** Phase 5, first PR (the rollout-core trait extensions PR is *before* this; the *first* PR that adds an SDK dep). Set up `deny-cloud-features` CI job in the same PR.

---

### Pitfall 15: Tokio runtime + cloud SDK runtime conflicts (nested block_on)

**What goes wrong:**
v1.0 uses a single Tokio multi-thread runtime, instantiated in `main()`. aws-sdk-rust's `aws-smithy-runtime` uses the *same* runtime by default (when feature `rt-tokio` is enabled, per STACK.md §1). `gcloud-*` uses `tonic` which uses Tokio. **As long as everything shares one runtime, we're fine.**

Failure modes:
1. **Hidden second runtime:** some SDK helpers or test utilities internally create a `tokio::runtime::Builder::new_current_thread().build()` for a synchronous helper API. Calling such a helper from inside a Tokio task panics with "Cannot start a runtime from within a runtime" or hangs in deadlock.
2. **`block_on` nested:** SDK retry helpers or auth-token refresh sometimes call `Handle::current().block_on(...)`. If we're already on a Tokio worker thread that called the SDK, this nests-block_on and panics.
3. **aws-smithy-runtime feature mismatch:** if any sub-crate accidentally enables `aws-smithy-runtime/default` (which is `connector-hyper`), and we elsewhere expect `rt-tokio`, you get TWO instances of the smithy runtime initialized.
4. **PyO3 vs Tokio:** v1.0's BACKEND-01 dedicated-thread pattern uses `run_until_complete` which is a custom bridge; this is OK. But a *separate* PyO3 call from a Tokio task that doesn't go through the dedicated thread can call `Python::with_gil` which blocks the entire Tokio worker — drops cloud SDK requests for the duration.

**Why it happens:**
- "It compiles" = "it works" for runtimes is a wrong assumption; runtime mismatch is a runtime panic.
- Multiple cloud SDKs cohabiting one process is rare in OSS; the patterns aren't well-tested.

**How to avoid:**
- **Single Tokio runtime invariant:** v1.0 already enforces this; document explicitly in `crates/rollout-cli/src/main.rs`. Hard rule: no `tokio::runtime::Builder` outside of `main()` and the dedicated-PyO3-thread.
- **Feature unification:** workspace `Cargo.toml` pins `aws-smithy-runtime` features explicitly (STACK.md §1 already does this). Add CI check `cargo tree --duplicate -p aws-smithy-runtime` — assert single version, single feature set.
- **No `block_on` inside async fn:** clippy lint `clippy::disallowed_methods` configured in `clippy.toml` to forbid `tokio::runtime::Handle::block_on` and `tokio::runtime::Runtime::block_on` outside of `main`-level entry points. (Already a common config in production Tokio codebases.)
- **`Python::with_gil` is forbidden outside the dedicated PyO3 thread.** Configure `clippy.toml` `disallowed_methods` to require `Python::with_gil` only inside `rollout-backend-vllm` (and v1.1 harness-tool if it grows a PyO3 surface). Cross-crate enforcement via `#[deny(clippy::disallowed_methods)]` at workspace lint level.
- **CI test `single_tokio_runtime`:** in a test binary, link `rollout-cloud-aws` + `rollout-cloud-gcp` + `rollout-backend-vllm` together; instantiate once; verify `tokio::runtime::Handle::current()` succeeds from every code path and is the same `Handle`.
- **Initialize cloud SDKs lazily and share clients:** `Arc<aws_sdk_s3::Client>` is constructed once per run, passed via `Arc<dyn ObjectStore>`. Never construct a client inside a Tokio task except at run-init.

**Warning signs:**
- Panic message contains "Cannot start a runtime from within a runtime."
- `cargo tree --duplicate` for `aws-smithy-runtime` / `tokio` / `tonic` returns non-empty.
- Profiling shows a worker thread stuck in `Python::with_gil` for >100ms while SDK requests pile up.

**Phase to address:** Phase 5, integration PRs for cloud crates. The `single_tokio_runtime` test runs the moment both AWS and GCP impl crates are in the same binary.

---

### Pitfall 16: Incremental blake3 over streamed upload — SDK retry restart breaks hash chain

**What goes wrong:**
ARCHITECTURE.md §2.3 says the streaming `put_stream` MUST compute blake3 incrementally to preserve content-addressing. The pattern:
```rust
let mut hasher = blake3::Hasher::new();
let mut multipart = s3_client.start_multipart_upload(...)?;
while let Some(chunk) = stream.next().await {
    let chunk = chunk?;
    hasher.update(&chunk);
    multipart.upload_part(chunk).await?;
}
let content_id = ContentId(hasher.finalize());
multipart.complete().await?;
```

What breaks:
- **aws-sdk-s3 retries `upload_part` internally** on transient errors (3 retries by default with exponential backoff). The retry re-reads from the input stream. If `stream` is a `Box<dyn AsyncRead>` from a `tokio::fs::File`, the position has advanced; retry reads garbage or short-reads. *The hasher is NOT consulted on retry — it's a one-way pipe.*
- **Worse:** if the SDK retries with the same chunk it has already buffered (typical for short chunks), the upload succeeds. But if we computed `hasher.update(&chunk)` only once, the hash is correct. If our code computes the hash inside the `upload_part` future (in a `map`), retries trigger duplicate `hasher.update` calls. Hash diverges from actual bytes.
- **gcloud-storage resumable upload:** similar — on a 503 from a chunk, the client may re-send. The local hasher state must mirror the *bytes actually committed*, not the bytes we *attempted* to send.
- **Mid-stream interruption (Pitfall 5 cousin):** if upload restarts (process death, network drop), the hasher is reset. Pitfall 9 says snapshot upload is atomic at the file level — re-upload from byte 0. **Good.** But within a single `put_stream` call, retries inside the SDK are NOT a re-upload from byte 0.

**Why it happens:**
- SDKs hide retry as an "internal" mechanism. Cargo crate docs don't always document that retries re-read the input stream.
- Mental model: "hasher and uploader are independent" — they're not.

**How to avoid:**
- **Hash before send, not during:** the streaming chunks pipeline reads from input → hashes a chunk → uploads the same chunk (already hashed). If the SDK retries the upload, it gets the same bytes back (the chunk is held by the upload future). The hasher only sees each chunk once because we feed it in our code, not in the SDK callback.
- **Implementation pattern:**
  ```rust
  let mut hasher = blake3::Hasher::new();
  let mut buf = Vec::with_capacity(CHUNK_SIZE);
  while let Some(chunk_or_eof) = read_chunk(&mut stream, &mut buf, CHUNK_SIZE).await? {
      hasher.update(&buf);                              // hash exactly once per chunk
      multipart.upload_part(buf.clone()).await?;        // SDK may retry; same bytes
      buf.clear();
  }
  ```
- **Refuse to retry across an exhausted source stream:** wrap the input in a `tokio::io::BufReader` and confirm that all bytes are read into a `Vec<u8>` per-chunk before `upload_part`. The chunk lives in a `Bytes` (cheap to clone) that the SDK can retry against.
- **Configure SDK retry behavior to be aware of our pattern:** `aws_sdk_s3::config::retry::RetryConfig::standard().with_max_attempts(3)` is fine. But use `aws-sdk-s3` chunk-buffering (it does this for us if we hand it `Bytes`).
- **CI test fixture `put_stream_content_id_matches_post_retry`:** inject a fault-injecting middleware that fails the first `upload_part` call; assert (a) the final content-id matches the input bytes hashed externally, (b) the SDK retry succeeded, (c) the hasher was called exactly N times for N chunks.
- **The byte-identical resume witness (`bit_identical_resume_at_step_5_via_s3`)** is the load-bearing test — it must run with fault-injection on at least one CI job to exercise this path.

**Warning signs:**
- "ContentId mismatch on restore" — restore reads bytes, hashes, compares to stored ContentId, fails. Almost certainly: hasher saw different bytes than uploader.
- Test `put_stream_content_id_matches_post_retry` fails — proof.
- Profiling shows `blake3::Hasher::update` called more than expected number of times per upload.

**Phase to address:** Phase 5 (CLOUD-01 / CLOUD-02 streaming put PRs). The hash-then-upload pattern must be in the first PR; retrofitting means rewriting the upload path.

---

### Pitfall 17: Postgres `scan_bytes` wildcard divergence under multi-node coord state read

**What goes wrong:**
Per the prompt and v1.0 milestone audit: v1.0 carries a latent `scan_bytes` wildcard divergence in `PostgresStorage`. The bug is "scan with wildcard prefix returns slightly different result set than redb under some edge case." Flagged for v1.2 (RL-01) because v1.0 didn't exercise the pattern enough to trip it.

v1.1 introduces multi-node coordinator state in Postgres (ARCHITECTURE.md §3, §5). The new namespaces are `"work"`, `"epoch"`, `"queue_items"`. Each is scanned with wildcards routinely:
- `scan_bytes("work/", ...)` — coordinator restart enumerates all in-flight assignments.
- `scan_bytes("workers/", ...)` — registry enumeration (already in v1.0 but multi-node exercises it harder).
- `scan_bytes("epoch/", ...)` — epoch history read on fence advancement.

If the bug is wildcard-related (e.g., Postgres LIKE pattern not properly escaping bytes that contain `%` or `_`, or LIKE collation differing from redb's lex-order), the consequences in v1.1 are:
- **Coord restart misses in-flight assignments** — restarts at incomplete state. Workers' acks fail because coord doesn't know about them. Effectively split-brain (Pitfall 7 cousin).
- **Registry enumeration misses workers** — some workers are "invisible" to coord. They never get pull_work; they idle.
- **Epoch read returns wrong row** — fence epoch off-by-one. Workers reject coord's valid responses.

**Why it happens:**
- The bug is latent because v1.0's surface scanned namespaces with stable prefixes that didn't contain special characters.
- v1.1's new namespaces are also stable prefixes BUT the *keys* under them are content-addressed (hex-encoded blake3) — should be safe. **The risk is that v1.1 also stores byte-encoded keys** (e.g., `WorkAssignmentId` is a struct with binary fields; some serialization paths could produce `%` or `_` in the bytes).

**How to avoid:**
- **Fix the v1.0 bug as a Phase 5 precursor.** Don't carry it into v1.1. It is a known issue with a known surface; the fix is presumably to use Postgres `bytea` columns with proper byte-range queries (`WHERE key >= start AND key < end`), not LIKE pattern matching. Track the actual bug location in `crates/rollout-storage/src/postgres/` to confirm.
- **CI test `scan_bytes_wildcard_parity`** — for every namespace prefix we use in v1.1 (`work/`, `epoch/`, `queue_items/`, `workers/`, `heartbeats/`, `snapshots/`), insert keys containing every byte value 0x00-0xFF; assert `redb` and `postgres` impls return identical results for `scan_bytes(prefix, ...)`. This is a property test (`proptest` crate).
- **All keys are hex-encoded** at the `StorageKey` construction site for multi-node namespaces. ContentId is already hex. WorkAssignmentId, EpochId — encode as hex. No raw bytes in keys. Documented in `rollout-core::storage::StorageKey` rustdoc.
- **Plan-time validator:** if `coordinator.mode = "multi_node"`, refuse to start with `Fatal::ConfigInvalid` unless the `scan_bytes_wildcard_parity` test has been run in the build (impossible to enforce at runtime — this is a discipline rule, enforced by the CI gating release).

**Warning signs:**
- Phase 6 CI test `coord_restart_no_duplicates` fails intermittently after switching from `redb` to `postgres` backend (parity broken).
- Worker count reported by `coordinator status` is less than the actual number of workers.
- Fence epoch read after restart returns wrong value (off-by-one).

**Phase to address:** **Phase 5 precursor PR** (before any multi-node code lands). Fixing the v1.0 latent bug is a small Phase 5 task; carrying it into Phase 6 is the actual pitfall.

---

## Technical Debt Patterns

Shortcuts that seem reasonable but create long-term problems.

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Use `Vec<u8>`-buffering default `put_stream` for cloud impls (ARCHITECTURE.md §2.3) | One-line cloud impl; ships fast | OOM on 30 GiB snapshots; sets misleading "it works" baseline | Local FS only; never for S3/GCS — flag the default impl as `#[deprecated]` for cloud crates |
| Skip `MultipartGuard` Drop-abort (Pitfall 4) | Less code; one less type | Indefinite S3 storage cost; surprise bill | Never. Drop-abort is non-negotiable. |
| Domain-allowlist (not IP-allowlist) HTTP tool (Pitfall 10c) | Simpler config; readable | SSRF to IMDS; data exfil via DNS rebinding | Never for tool harness; OK for non-LLM-driven HTTP clients elsewhere (e.g., user-supplied test fixtures) |
| `BehaviorVersion::v2023_11_09()` AWS config (Pitfall 3) | "Stable" pin | IMDSv1 fallback; spot drain silently broken | Never on `BehaviorVersion::latest()` should be the only allowed value |
| Pin `aws-sdk-s3 = "1.112"` without exact `=` (STACK.md §1) | Allow patches | Cargo silently picks MSRV-1.91 version on `cargo update` | Never; STACK.md mandates `=1.112.0` exact |
| Single CI job for AWS (`cloud-emulator-aws` only, no `cloud-live-aws`) | Avoids OIDC setup | Production bugs ship; "passed CI" lies | OK pre-Phase-5-end; must add live job before Phase 6 ships |
| Ignore Postgres `scan_bytes` parity bug (carry from v1.0) | One less Phase 5 task | Phase 6 coord-restart bugs; split-brain (Pitfall 17) | Never — fix in Phase 5 precursor |
| Use `subprocess.Popen(shell=True)` in Python sidecar tool (Pitfall 10a) | Easier command construction | Command injection at LLM call time | Never |
| Treat 17 → 18 crate count as "one more, ship it" (ARCHITECTURE.md §1) | Less PROJECT.md churn | Misleads roadmap planners on Phase 7 sizing | OK; just update PROJECT.md to 18 in same PR that lands the new crates |
| Hand-roll IMDS HTTP client instead of `aws-config::imds::client::Client` (Pitfall 3) | Avoids `aws-config` dep | IMDSv1 bug; never auto-refreshes session token | Never |

---

## Integration Gotchas

Common mistakes when connecting to the v1.0 substrate or external services.

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| `Snapshotter` ↔ cloud `ObjectStore` | Assume `Snapshotter::list` returns the same set on FS and S3 (LIST-after-PUT race) | Use `--resume <snapshot_id>` from coord-state pointer, not `list_latest` |
| `Coordinator` ↔ `Storage` (Postgres) | Use redb-backed state for "production-looking" tests, miss multi-node CAS races | Run `coord_restart_no_duplicates` on both backends in CI; require `postgres` backend for `coordinator.mode = "multi_node"` |
| `ComputeHint` ↔ IMDSv2 | Use `reqwest::get("169.254.169.254/...")` directly | `aws_config::imds::client::Client::builder().build()` — never hand-roll |
| `aws-sdk-s3` ↔ tokio runtime | Construct client inside async task on each call | `Arc<aws_sdk_s3::Client>` at run-init, share via DI |
| `gcloud-storage` ↔ blake3 streaming | Compute hash inside `upload_part` callback | Hash externally before `upload_part`; pass `Bytes` to SDK |
| Tool harness Python sidecar ↔ PyO3 host | Inherit the host's Python interpreter (fork) | Spawn fresh `python3` subprocess; never share interpreter |
| HF dataset ↔ CI without `HF_TOKEN` | 429s in CI on every PR | Ship 10-row fixtures; default `HF_HUB_OFFLINE=1` for `cargo test` |
| Postgres `scan_bytes` ↔ v1.1 namespaces | Carry v1.0 latent bug into multi-node | Fix in Phase 5 precursor; add `scan_bytes_wildcard_parity` property test |
| seccomp filter ↔ glibc 2.34+ | Filter `clone` only, miss `clone3` | Explicit allow for `clone3` with restricted flags; same for `openat2` |
| localstack ↔ real AWS | "Localstack passes" = "production works" | Two CI tracks: emulator-always-on + live-nightly; conformance tests run both |
| `aws-config` ↔ `BehaviorVersion` | Pick a version like `v2023_11_09()` | Always `BehaviorVersion::latest()`; pinned by `aws-config` crate version |
| `Coordinator::pull_work` ↔ lease | Worker holds long-running work; lease expires; double-execution | `extend_lease` mid-work; CAS on `WorkAssignment` state, not lease alone |

---

## Performance Traps

Patterns that work at small scale but fail as multi-node + cloud scale grows.

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Hot S3 prefix on sequential `sample/0`, `sample/1`, … | 503 SlowDown on every Nth PUT | Hash-prefix the key (`{hash[0..2]}/sample/{i}`) for write-heavy paths; or use random prefixes | ~3,500 ops/sec/prefix on S3 |
| `Vec<u8>` buffering 30 GiB snapshot via `put_bytes` (ARCH §2.3 default) | OOM at worker | Use `put_stream` override on cloud impls; never default-buffer for cloud | Snapshots > ~1 GiB |
| Sync `EvalHarness::evaluate` in RL inner loop | GPU util drops to <50% | Async eval job via WorkQueue; cache datasets | v1.2 RL phases |
| Per-call `aws_sdk_s3::Client::from_conf(...)` | Latency 100ms+ per request (TLS handshake every call) | `Arc<Client>` at run-init; reuse | Any prod load |
| Per-call HF dataset download in `EvalHarness::load_dataset` | 30s startup per process; 429s | Cache dataset in `ObjectStore` after first download; load from cache | Any non-trivial CI |
| `IMDS` poll at <1s | 429 on IMDS rate limit | Poll at 5s | Always (cloud-side rate limit) |
| Sync `Python::with_gil` on Tokio worker thread | Other Tokio tasks blocked for ms-to-seconds | Dedicated PyO3 thread (v1.0 BACKEND-01 pattern); never `with_gil` on shared workers | Any concurrent SDK use |
| `tokio::sync::Mutex` around the whole coordinator state | Mutex contention as worker count grows | Per-namespace storage CAS; lock-free reads via Storage::scan_bytes | >10 workers |
| Postgres `SELECT ... WHERE key LIKE '$prefix%'` for scans (Pitfall 17) | Slow scans; wildcard divergence | `WHERE key >= $start AND key < $end` byte-range; indexed bytea | Any prod multi-node |
| Per-tool subprocess spawn for Python eval tool | 100-300ms spawn cost per call | Long-lived sandboxed Python worker pool (post-v1.1; v1.1 defaults to spawn-per-call) | Latency-sensitive workloads |

---

## Security Mistakes

Domain-specific security issues beyond OWASP basics.

| Mistake | Risk | Prevention |
|---------|------|------------|
| `subprocess.Popen(shell=True)` in Python sidecar tool (Pitfall 10a) | Command injection from LLM output → arbitrary code execution on worker | Banned by CI grep; allowlist of absolute binary paths only |
| Domain-only HTTP allowlist (Pitfall 10c) | SSRF to IMDS → cloud credentials theft; RFC1918 scan | IP-allowlist post-DNS; reject RFC1918/link-local/loopback; pin resolved IP through redirects |
| Path-prefix check via `str::starts_with` (Pitfall 10b) | Path traversal, symlink escape, TOCTOU → read arbitrary files | `cap-std::fs::Dir` + `O_NOFOLLOW | O_RESOLVE_BENEATH` |
| Forget `clone3` / `openat2` in seccomp allowlist (Pitfall 10e) | Sandboxed process can't `fork`/`exec`; OR worse, the sandbox is silently disabled | Curated allowlist with strace-derived ground truth; per-syscall justification |
| Secrets in `RunConfig` TOML (ARCHITECTURE.md §5 already bans) | Secrets in git, in logs, in error messages | Plan-time validator rejects any `[*.credentials]`/`[*.secret]` block; secrets only via SecretStore |
| Allow user-config `seccomp.allowlist` extension (Pitfall 10e — anti-pattern) | Misconfig = security incident | Single hardened default; users who need more impl their own `Tool` |
| Trust HF dataset content (Pitfall 12) | Supply-chain: a poisoned MMLU release flips a label, training run uses wrong scoring | Vendor SHA-pinned content hashes; verify on download |
| Log raw `aws_smithy_runtime_api` errors (Pitfall 1) | SDK errors may contain temporary credentials, signed URLs, bucket policies | Collapse to string at crate boundary; redact known patterns |
| Fork-after-PyO3-init for "fast worker startup" (Pitfall 13) | Locked Python state → deadlock; OR partial state → memory corruption | `posix_spawn`/execve only; CI grep ban on `libc::fork` |
| Trust IMDS without IMDSv2 (Pitfall 3) | Stale credentials; spot drain silently broken on IMDSv1-disabled accounts | `aws-config::imds::client::Client` only; CI grep ban on raw `169.254.169.254` |
| Mid-step snapshot inside fwd/bwd (FEATURES.md §4 anti-feature) | Non-deterministic resume; byte-identical invariant broken | Step-boundary snapshots only; mid-step = requeue work |
| Cross-cloud snapshot transfer without re-validation (Pitfall 9) | Wrong restore data; subtle corruption | Snapshot is self-contained; restore re-validates ContentId on every part |

---

## "Looks Done But Isn't" Checklist

Things that appear complete but are missing critical pieces.

- [ ] **AWS S3 ObjectStore impl:** Looks done — passes localstack. Often missing: `MultipartGuard` Drop-abort (Pitfall 4); IP-allowlist check on get_stream redirects; `BehaviorVersion::latest()` enforcement; multipart cleanup lifecycle policy in bucket setup docs. Verify by `aws s3api list-multipart-uploads` after a test run — should be empty.
- [ ] **GCS ObjectStore impl:** Looks done — passes fake-gcs-server. Often missing: atomic-per-snapshot upload semantics (Pitfall 5); resumable-upload `upload_id` is NOT persisted to Storage; lifecycle policy documented. Verify by killing a worker mid-upload and checking `gsutil ls -L` for orphan composite objects.
- [ ] **SQS Queue impl:** Looks done — pull/dequeue works. Often missing: `dequeue_with_lease` returns a usable `LeaseToken` that round-trips through `extend_lease` (STACK.md §2.1); visibility-timeout configured per-message not per-queue; dead-letter queue routing. Verify by deliberately holding a message past visibility timeout and asserting it reappears.
- [ ] **Pub/Sub Queue impl:** Looks done — ack works. Often missing: subscription ack-deadline-mismatch (sub config says 30s, our lease says 60s — Pub/Sub redelivers at 30s anyway); ordering keys NOT used (don't enable; we don't need ordering and ordered subscriptions throttle harder). Verify by setting a 30s ack deadline and 60s lease and asserting redelivery at 30s.
- [ ] **Spot drain:** Looks done — `drain` test passes. Often missing: drain protocol is NOT idempotent against re-preemption mid-drain (Pitfall 8); `preemption_drain_lead` not plan-time validated; snapshot-or-skip decision tree not actually consulted (always tries to snapshot). Verify by injecting a second preemption signal mid-drain.
- [ ] **Coord restart from storage:** Looks done — kill coord, new coord recovers state. Often missing: fence-epoch CAS not actually advanced (Pitfall 7); old coord doesn't self-fence (Pitfall 7); workers don't validate `coord_epoch` on every RPC (Pitfall 7); split-brain test not in CI. Verify by leaving old coord running and starting new coord against same Postgres — should be exactly one live.
- [ ] **Tool harness shell tool:** Looks done — `echo hello` works. Often missing: `shell=True` ban (Pitfall 10a); allowlist of binary *paths* (not names); seccomp `clone3` allow (Pitfall 10e); cgroups v2 limits actually enforced (test by spawning `:(){ :|:& };:` fork bomb — should be killed by `pids.max`). Verify by trying command injection: `; cat /etc/passwd`.
- [ ] **Tool harness file tool:** Looks done — read/write to `/tmp/sandbox` works. Often missing: `..` rejection post-canonicalize (Pitfall 10b); symlink rejection (`O_NOFOLLOW`); cap-std actually wraps every fs call. Verify by creating `/tmp/sandbox/escape -> /etc/passwd` and trying to read `escape`.
- [ ] **Tool harness HTTP tool:** Looks done — `GET https://example.com/` works. Often missing: IP-allowlist post-DNS (Pitfall 10c); RFC1918 rejection; DNS rebinding defense; redirect re-validation. Verify by allowlisting `example.com` and trying `https://example.com/redir?to=http://169.254.169.254/...` — must reject.
- [ ] **Eval harness MMLU:** Looks done — scores 0.42 on a tiny model. Often missing: scoring convention documented as `acc` (Pitfall 11); dataset cached across calls; offline-mode default (Pitfall 12); 10-row fixture committed. Verify by running `HF_HUB_OFFLINE=1 cargo test -p rollout-harness-eval` — should pass.
- [ ] **Byte-identical resume via cloud (`bit_identical_resume_at_step_5_via_s3`):** Looks done — passes once. Often missing: fault-injection on SDK retry path (Pitfall 16); cross-provider test (Pitfall 9); LIST-after-PUT race protection (Pitfall 4). Verify by running 100x with `localstack` fault-injection at 10% — should still pass.
- [ ] **cargo-deny with cloud features:** Looks done — `cargo deny check` is green. Often missing: feature-combination coverage (`--features aws,gcp,vllm,sandbox`); aws-lc-rs license actually allowlisted with documented justification (Pitfall 14); `OpenSSL` license explicitly denied in `[licenses].deny`. Verify by `cargo deny check --features aws,gcp,vllm,sandbox`.
- [ ] **Multi-node smoke test on real cloud:** Looks done — `make smoke-multi-node-aws` exits 0. Often missing: assertion that `coord.fence_epoch` actually advanced; assertion that no orphan multipart uploads exist post-test; assertion that all worker logs show same `coord_epoch` value. Verify by inspecting Postgres state post-test.
- [ ] **`single_tokio_runtime` check (Pitfall 15):** Looks done — no panics. Often missing: cargo tree dedup check; `clippy::disallowed_methods` for `block_on`. Verify by running `cargo tree --duplicate -p tokio` — should report no duplicates.
- [ ] **Postgres `scan_bytes_wildcard_parity` (Pitfall 17):** Looks done — basic scans work. Often missing: property test over all byte values 0x00-0xFF in keys; multi-node namespaces (`work/`, `epoch/`, `queue_items/`) covered. Verify by running `cargo test -p rollout-storage scan_bytes_wildcard_parity -- --features postgres` against testcontainers Postgres.

---

## Recovery Strategies

When pitfalls occur despite prevention.

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| 1 (SDK type leak) | MEDIUM | Revert the leaky PR; add the `public-api-cloud-leak` CI gate; rewrite the trait method without SDK type; consumers re-compile. ~1 day. |
| 2 (emulator lies) | MEDIUM | Add `cloud-live-*` CI job; backfill conformance tests for the missed prod behavior; document in README. ~2 days. |
| 3 (IMDSv1) | LOW | Switch to `aws_config::imds::client::Client`; add CI grep gate; backfill `imds_v1_disabled_falls_back_gracefully` test. ~half a day. |
| 4 (S3 multipart leak) | LOW-MEDIUM | Add `MultipartGuard` with Drop-abort; configure lifecycle policy on existing buckets via `aws s3api put-bucket-lifecycle-configuration`; manually abort existing orphans via `aws s3api list-multipart-uploads | xargs aws s3api abort-multipart-upload`. ~1 day + cleanup. |
| 5 (GCS resumable orphan) | LOW | Rely on bucket lifecycle policy for cleanup; document atomic-per-file rule; abort partial uploads manually if cost surge. ~half a day. |
| 6 (dedup race) | HIGH | Audit every non-idempotent code path; ensure CAS-on-state holds; add `concurrent_ack_and_steal_no_double_execute` test. ~3 days if discovered late. |
| 7 (split-brain) | HIGH | This is the worst recovery: data corruption possible. Halt all multi-node runs; verify last-known-good state in Postgres; manually advance fence; re-run with lease-based fencing properly wired. ~1 week if data corruption. |
| 8 (preemption budget too small) | LOW | Lengthen `preemption_drain_lead`; add idempotency tests; deploy. ~1 day. |
| 9 (cross-provider resume) | LOW | Add `cross_provider_resume_via_explicit_transfer` test; document snapshot self-containment rule. ~half a day. |
| 10a-e (sandbox escapes) | CRITICAL | If exploited: incident response. Otherwise: backfill each test fixture (10a-e), audit prior tool invocations for evidence of misuse. ~1 week. |
| 11 (sync eval blocks loop) | MEDIUM | Refactor `evaluate` to async-job pattern via WorkQueue; cache datasets. ~2 days. |
| 12 (HF rate limits) | LOW | Add fixture-based loaders; default `HF_HUB_OFFLINE=1`; update CI. ~1 day. |
| 13 (fork after PyO3) | MEDIUM | Audit all spawn sites for `libc::fork`; replace with `Command::new(...).spawn()`; add CI grep gate. ~1 day. |
| 14 (cargo-deny license) | LOW | Add license to allowlist with justification; OR find alternative crate. ~half a day. |
| 15 (nested runtime) | MEDIUM | Audit for `block_on` calls; add clippy `disallowed_methods`; restructure single-runtime invariant. ~1-2 days. |
| 16 (blake3 retry hash) | MEDIUM-HIGH | The `bit_identical_resume_at_step_5_via_*` test catches it. If discovered post-ship: every snapshot taken with the bug is suspect; re-validate by re-hashing all snapshot parts on restore (already done by v1.0 restore code per ARCHITECTURE.md §2.4); discard mismatched snapshots. ~2-3 days. |
| 17 (scan_bytes parity) | LOW | Fix the v1.0 latent bug in Phase 5 precursor; add property test; backfill. ~1 day. |

---

## Pitfall-to-Phase Mapping

How v1.1 roadmap phases address each pitfall.

| # | Pitfall | Prevention Phase | Prevention Mechanism | Verification |
|---|---------|------------------|---------------------|--------------|
| 1 | SDK type leakage | Phase 5 PR 1 (rollout-core trait extensions) | Dep-direction invariant #14 + `public-api-cloud-leak` CI job | `cargo public-api -p rollout-core` shows no `aws_*`/`gcloud_*` types |
| 2 | localstack/emulator lies | Phase 5 (every cloud impl PR) | Two CI tracks: `cloud-emulator-*` always-on + `cloud-live-*` nightly; conformance suite parameterized | Conformance suite passes on BOTH emulator and live |
| 3 | IMDSv1 footgun | Phase 5 PR 5 (AWS ComputeHint) | CI `forbidden-patterns` grep for `169.254.169.254` outside cloud-aws; `BehaviorVersion::latest()` enforced | `imds_v1_disabled_falls_back_gracefully` test |
| 4 | S3 multipart leak + LIST race | Phase 5 PR 3 (S3ObjectStore) | `MultipartGuard` Drop-abort type; bucket-lifecycle docs; resume by SnapshotId not list_latest | `put_stream_dropped_aborts_multipart` test + `aws s3api list-multipart-uploads` clean |
| 5 | GCS resumable orphan | Phase 5 PR 6 (GcsObjectStore) | Atomic-per-file rule (no cross-process upload_id persistence); plan-time `max_snapshot_part_bytes` validator | `gcs_resumable_upload_preempted_mid_stream_reuploads_cleanly` test |
| 6 | Work-stealing dedup race | Phase 6 PR 2 + 4 (work-state machine + worker pull loop) | CAS-on-state (extend v1.0 sample-state pattern); `extend_lease` for long ops | `concurrent_ack_and_steal_no_double_execute` test + `coord_restart_no_duplicates` |
| 7 | Split-brain coord | Phase 6 PR 3 (fence epoch) | Postgres lease-row UPDATE with `expires_at`; worker validates `coord_epoch` per-RPC; self-abort on stale-lease | `split_brain_old_coord_self_fences` test |
| 8 | Spot preemption hints | Phase 6 PR 5 (drain orchestration) | Idempotent drain protocol; conservative budget defaults (60s AWS / 15s GCP); snapshot-or-skip decision tree | `spot_drain_mid_drain_preemption_no_duplicate_work` + `spot_drain_completes_within_60s` |
| 9 | Cross-provider resume | Phase 5 PR 9 (snapshot streaming over cloud OS) | Self-contained snapshots (no provider URLs in metadata); ContentId-only references | `cross_provider_resume_via_explicit_transfer` fixture (emulator job) |
| 10a | shell=True | Phase 7 PR 3 (tool sandbox process iso) | CI grep gate on `shell=True` in tool harness Python; allowlist of absolute paths | `tool_shell_blocks_injection` test |
| 10b | Path traversal | Phase 7 PR 4 (tool path/HTTP allowlist) | `cap-std` + `O_NOFOLLOW \| O_RESOLVE_BENEATH`; canonicalize-then-prefix-check | `tool_file_blocks_symlink_escape` + `tool_file_blocks_dotdot` |
| 10c | SSRF / DNS rebinding | Phase 7 PR 4 | Custom `hyper::Connect` with IP-allowlist post-DNS + RFC1918 deny + redirect re-validation | `http_tool_blocks_{dns_rebinding,redirect_to_imds,rfc1918}` |
| 10d | landlock kernel version | Phase 7 PR 3 | Runtime kernel-version detection at sandbox init; plan-time validator with `require_landlock` flag | Test on Ubuntu 22.04 + documented RHEL 8 limitations |
| 10e | seccomp clone3/openat2 | Phase 7 PR 3 | Curated allowlist with strace-derived justification; explicit clone3/openat2/faccessat2 | `seccomp_python_runs` + `seccomp_blocks_unexpected_syscall` |
| 11 | Sync eval blocks loop | Phase 7 PR 5 (rollout-harness-eval) | Async-job-via-WorkQueue pattern documented; `evaluate()` cache datasets via Arc | `eval_loader_caches_dataset_across_calls` + design rule in trait rustdoc |
| 12 | HF rate limits | Phase 7 PR 5 | 10-row fixtures committed; `HF_HUB_OFFLINE=1` default for `cargo test`; SHA-pinned download verification | `eval_loader_works_with_no_network` |
| 13 | Fork after PyO3 init | Phase 6 PR 4 (worker spawn pattern) | CI grep ban on `libc::fork`; spawn via `Command::new` only; no sub-interpreters | `worker_spawn_after_pyo3_init_works` |
| 14 | cargo-deny license | Phase 5 PR 1 (first SDK dep) | New `deny-cloud-features` CI job; `OpenSSL` explicit deny in `deny.toml`; aws-lc-rs license audited | `cargo deny check --features aws,gcp,vllm,sandbox` green |
| 15 | Nested Tokio runtime | Phase 5 (cloud integration) | Single runtime invariant in `main()`; `clippy::disallowed_methods` for `block_on`; cargo tree dedup check | `single_tokio_runtime` test |
| 16 | blake3 retry hash | Phase 5 PR 3 + 6 (S3 + GCS put_stream) | Hash externally before `upload_part`; SDK retries replay buffered chunk | `put_stream_content_id_matches_post_retry` + `bit_identical_resume_at_step_5_via_{s3,gcs}` under fault injection |
| 17 | Postgres scan_bytes parity | **Phase 5 precursor** (fix v1.0 latent) | Byte-range queries instead of LIKE; hex-encoded multi-node keys; property test parity | `scan_bytes_wildcard_parity` property test on every CI build |

---

## Cross-Cutting CI Job Additions Summary

These jobs are referenced by multiple pitfalls; consolidating for the roadmap.

| CI Job | Always-on / Nightly | Pitfalls covered | Notes |
|--------|---------------------|------------------|-------|
| `cloud-emulator-aws` | always | 2, 4, 16 | localstack S3 + SQS + SecretsManager + IMDS-mock; fault-injection at 10% |
| `cloud-emulator-gcp` | always | 2, 5, 9, 16 | fake-gcs-server + pubsub-emulator |
| `cloud-live-aws` | nightly + on-PR if touches `crates/rollout-cloud-aws` | 2, 3, 4 | OIDC creds; ~$0.10/run |
| `cloud-live-gcp` | nightly + on-PR if touches `crates/rollout-cloud-gcp` | 2, 5 | WIF creds |
| `multi-node-smoke-redb` | always | 6, 7, 8, 13 | In-process 3-worker simulation |
| `multi-node-smoke-postgres` | always (extends `postgres-integration`) | 6, 7, 17 | Postgres-backed coord; testcontainers |
| `deny-cloud-features` | always | 14 | `cargo deny check --features aws,gcp,vllm,sandbox` |
| `public-api-cloud-leak` | always | 1 | `cargo public-api -p rollout-core` greps for SDK type names |
| `forbidden-patterns` | always | 3, 10a, 13 | greps for `169.254.169.254`, `shell=True`, `libc::fork(` |
| `sandbox-tests-linux` | always (linux runner) | 10b, 10c, 10d, 10e | All `seccomp_*`, `landlock_*`, `tool_*` fixtures |
| `hf-dataset-refresh` | weekly | 12 | Validate HF URLs + SHAs still resolve |

Adds ~6 always-on jobs + 4 nightly. Estimated CI time impact: ~7-10 min on PRs (most jobs run in parallel; cloud-live nightly only).

---

## Sources

- `/Users/ashutosh/personal/rollout/.planning/PROJECT.md` — milestone scope, Out of Scope, Key Decisions
- `/Users/ashutosh/personal/rollout/.planning/MILESTONES.md` — v1.0 ship state, tech debt (Postgres `scan_bytes` flagged)
- `/Users/ashutosh/personal/rollout/.planning/RETROSPECTIVE.md` — v1.0 lessons (5-job-red on `main`, missing CI for experimental features, brand-work retrofit)
- `/Users/ashutosh/personal/rollout/.planning/research/STACK.md` — pinned versions, MSRV gotchas (aws-sdk 1.91), cap-std license flag, landlock kernel ≥5.13
- `/Users/ashutosh/personal/rollout/.planning/research/ARCHITECTURE.md` — trait extensions, 13 dep-direction invariants, coord state in Storage namespaces, snapshot self-containment
- `/Users/ashutosh/personal/rollout/.planning/research/FEATURES.md` — drain protocol, lm-eval-harness, conformance test pattern, MockBackend witness pattern
- [AWS S3 multipart upload lifecycle (`AbortIncompleteMultipartUpload`)](https://docs.aws.amazon.com/AmazonS3/latest/userguide/mpu-abort-incomplete-mpu-lifecycle-config.html) — HIGH confidence, official
- [AWS IMDSv2 enforcement (`HttpTokens=required`)](https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/configuring-instance-metadata-service.html) — HIGH, official
- [aws-sdk-rust BehaviorVersion API docs](https://docs.rs/aws-config/latest/aws_config/struct.BehaviorVersion.html) — HIGH, official
- [GCS resumable upload protocol](https://cloud.google.com/storage/docs/performing-resumable-uploads) — HIGH, official
- [GCS object lifecycle management](https://cloud.google.com/storage/docs/lifecycle) — HIGH, official
- [GCP spot VM 30s preemption notice](https://docs.cloud.google.com/compute/docs/instances/spot) — HIGH, official
- [Linux landlock kernel version requirements](https://docs.kernel.org/userspace-api/landlock.html) — HIGH, official
- [seccomp filter and `clone3` (CVE-2020-29368 background; broader pattern)](https://man7.org/linux/man-pages/man2/seccomp.2.html) — HIGH, official
- [PyO3 0.28 sub-interpreters status](https://pyo3.rs/v0.28.0/) — HIGH, official
- [cap-std crate (Apache-2.0 WITH LLVM-exception)](https://docs.rs/cap-std) — HIGH, official
- [blake3 incremental hashing API](https://docs.rs/blake3/latest/blake3/struct.Hasher.html) — HIGH, official
- [aws-lc-rs licensing](https://github.com/aws/aws-lc-rs/blob/main/LICENSE) — HIGH, official
- [tokio runtime nesting panic patterns](https://docs.rs/tokio/latest/tokio/runtime/index.html) — HIGH, official
- [HuggingFace anonymous rate limits / gated datasets](https://huggingface.co/docs/hub/datasets-gated) — HIGH, official

---

*Pitfalls research for: rollout v1.1 (cloud + multi-node + harnesses)*
*Researched: 2026-05-27*
