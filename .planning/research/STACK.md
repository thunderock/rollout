# Technology Stack — v1.1 Additions

**Project:** rollout v1.1 (cloud + multi-node + harnesses)
**Researched:** 2026-05-27
**Toolchain constraint:** Rust 1.88.0 (workspace `rust-version`); cargo-deny openssl ban; rustls-only TLS policy; MIT-compatible licenses only.

## Scope

This document covers **NEW** dependencies for v1.1. The v1.0 stack (tonic 0.14, rustls 0.23, redb 2.x, sqlx 0.8, pyo3 0.28, libloading 0.8, etc. — see workspace root `Cargo.toml`) is treated as fixed; deltas only.

---

## 1. AWS SDK (CLOUD-01)

### Recommendation

| Crate | Pinned Version | Rationale |
|---|---|---|
| `aws-config` | `=1.8.17` | Provides `BehaviorVersion`, IMDSv2 client, default credential chain. Last MSRV-1.88-compatible series. |
| `aws-sdk-s3` | `=1.112.0` | S3 object store. **MSRV 1.88.0 verified** (`docs.rs/crate/aws-sdk-s3/1.112.0/source/Cargo.toml.orig`). |
| `aws-sdk-sqs` | `=1.65.x` (release-aligned with smithy-rs at S3 1.112 cut) | SQS queue. Pin to the contemporaneous release-train version; verify MSRV at integration time. |
| `aws-sdk-secretsmanager` | `=1.65.x` (same cohort) | Secrets Manager. Pin to same release train as above. |
| `aws-smithy-runtime` | `=1.9.4` | Pulled transitively; pinned by aws-sdk-s3 1.112.0. |
| `aws-smithy-runtime-api` | inherited | Transitive; no direct dep. |
| `aws-credential-types` | `=1.2.9` | For custom `CredentialsProvider` impls (we will *not* re-implement IMDSv2; reuse `imds::client::Client`). |

**Default features to set:**
```toml
aws-sdk-s3 = { version = "=1.112.0", default-features = false, features = [
    "behavior-version-latest",
    "rt-tokio",
    "default-https-client",  # hyper 1.x + rustls + aws-lc (post-2024 default stack)
    "sigv4a",
] }
```

### Critical MSRV Gotcha

**aws-sdk-rust current `main` MSRV is 1.91.1** as of May 2026. Cannot use the latest releases (s3 ≥ 1.133, sqs ≥ 1.99). The aws-sdk-rust policy is "stable-2" (current stable + two prior), and Rust 1.91 stable shipped 2026-Q1, displacing 1.88. **Pin all `aws-sdk-*` and `aws-smithy-*` crates with `=` exact-version specifiers** to avoid Cargo silently resolving to MSRV-1.91 versions during `cargo update`.

If we want to track upstream long-term, the cleanest fix is bumping our workspace MSRV to 1.91 in a future milestone. Until then: **exact-version pinning + a CI job that periodically tries `cargo update -p aws-sdk-s3 --precise <newer>` and reports MSRV breaks** is the standing pattern.

### TLS / cargo-deny Compatibility

- The new (post-2024) `default-https-client` stack uses **hyper 1.x + rustls + aws-lc**. **No openssl.** Compatible with our openssl ban.
- The older `rustls` feature still works but pulls hyper 0.14.x and rustls 0.21.x (legacy). Avoid; we want hyper 1.x to match the rest of the workspace eventually.
- License: all `aws-sdk-*` crates are **Apache-2.0** — already on our cargo-deny allowlist.

### Integration with v1.0 Surfaces

| v1.0 trait | AWS impl backing | Mapping |
|---|---|---|
| `ObjectStore` (`rollout-core`) | `aws-sdk-s3::Client` | `put_bytes` → `PutObject`; `get_bytes` → `GetObject`; CAS keying preserved as object key. |
| `Queue` (`rollout-core`) | `aws-sdk-sqs::Client` | `enqueue` → `SendMessage`; `dequeue` → `ReceiveMessage` + visibility timeout; ack → `DeleteMessage`. Long-poll via `wait_time_seconds`. |
| `SecretStore` (`rollout-core`) | `aws-sdk-secretsmanager::Client` | `get_secret` → `GetSecretValue`. Allowlist enforcement stays in the wrapper, identical to env-var impl. |
| `ComputeHint` (`rollout-core`) | `aws-config::imds::client::Client` | `/latest/meta-data/instance-type` → GPU class inference; `/latest/meta-data/spot/instance-action` → preemption signal. |

### What NOT to Add

- ❌ `rusoto_*` — abandoned upstream.
- ❌ `aws-smithy-client` with `native-tls` — pulls openssl.
- ❌ Any `aws-sdk-*` with default `legacy-rustls` feature — pulls hyper 0.14.

---

## 2. GCP SDK (CLOUD-02)

### Recommendation: Official `gcloud-*` (googleapis/google-cloud-rust)

| Crate | Pinned Version | Rationale |
|---|---|---|
| `gcloud-storage` | latest from `googleapis/google-cloud-rust` (≥1.0, monorepo-versioned May 2026) | Official Google client, MSRV 1.87 (compatible with 1.88). |
| `gcloud-pubsub` | same monorepo cohort | Official, generated from googleapis protos. |
| `gcloud-secretmanager-v1` | same monorepo cohort | Official, generated. Note: secret manager is `*-v1` suffixed because of multi-version protos. |
| `gcloud-auth` | same monorepo cohort | ADC, workload identity, GCE metadata. |

### Why the official Google SDK, not yoshidan's

- **Maintainer**: `googleapis/google-cloud-rust` is now Google-official as of late 2025 — Google "generously received the google-cloud-storage crate name" from the community, and the project is actively developed (last release May 2026). The community `google-cloud-*` crates by yoshidan have been renamed `gcloud-*` to free the namespace.
- **MSRV**: 1.87 (matches `rust-version` in their workspace `Cargo.toml`). **Compatible with our 1.88 pin.** Google's MSRV policy is "previous year of rustc," and changes are not considered breaking.
- **License**: Apache-2.0 (cargo-deny compatible).
- **TLS**: Uses `tonic` + `rustls` underneath (same transport stack as our v1.0 transport crate — no new TLS surface).
- **Coverage**: Storage + Pub/Sub + Secret Manager + Auth + GCE metadata via `gcloud-auth::credentials::mds` are all covered.

### Alternatives Rejected

| Alternative | Why Not |
|---|---|
| `google-cloud-storage` (yoshidan, v1.3.0 community) | Community-maintained; less SLA. The official Google SDK is now the better choice. Keep as a fallback if a specific gcloud-* crate is missing a feature. |
| `yup-oauth2` + hand-rolled REST | Lower-level; would require writing every GCS/PubSub/SM REST call by hand. Only choose this for ad-hoc one-off integrations, not a production cloud layer. |
| `googleapis-tonic-google-cloud-storage-v2` (community-generated tonic stubs) | Low-level gRPC; no auth/retry/resumable-upload helpers. Pass. |

### Integration

Identical trait-mapping pattern to AWS:

| v1.0 trait | GCP impl backing |
|---|---|
| `ObjectStore` | `gcloud-storage::client::Client` (resumable-upload for large snapshots) |
| `Queue` | `gcloud-pubsub::client::Client` (topic + subscription model; pull mode for parity with SQS) |
| `SecretStore` | `gcloud-secretmanager-v1::client::SecretManagerServiceClient` |
| `ComputeHint` | `gcloud-auth::credentials::mds::Client` (`/computeMetadata/v1/instance/preempted`, `/maintenance-event`) |

### What NOT to Add

- ❌ Any GCP SDK requiring `openssl-sys` (cargo-deny ban).
- ❌ `google-cloud-rust-raw` (deprecated, pre-Tonic).

---

## 3. Object Store Abstraction (CLOUD-03)

### Recommendation: **Keep hand-rolled `ObjectStore` trait; do NOT depend on `object_store`**

The v1.0 `rollout-core::ObjectStore` trait + `FsObjectStore` in `rollout-cloud-local` is the contract that algorithm crates depend on. Replacing it with the Apache Arrow `object_store` crate as the *trait definition* would break the layered-architecture lint (algorithm crates would transitively depend on cloud SDKs).

**Strategy:** Implement the existing `ObjectStore` trait three more times — once each in `rollout-cloud-aws`, `rollout-cloud-gcp`, and (optional) a thin adapter over `object_store` for users who want Azure / WebDAV without code changes.

### Why not adopt `object_store` directly

- **Trait surface mismatch**: `object_store` is async-trait with `Path`/`PutPayload` types; our trait uses `ContentId` + `Bytes`. Adapter shim is cheap; replacement is not.
- **Architecture lint**: If `object_store` becomes our core trait, every dep of every algorithm crate transitively pulls cloud SDKs. The 10-invariant dependency_direction test fails.
- **Pinning churn**: `object_store` 0.13/0.14 churns ~quarterly; v1.0 is its own stable contract.

### Optional: `object_store` as backend (feature-gated)

Add a `rollout-cloud-objectstore` crate that impls our `ObjectStore` trait by delegating to `object_store::ObjectStore`. Gives users Azure/WebDAV/HTTP/local-via-arrow for free, **without** the core depending on it.

| Crate | Pinned Version | Rationale |
|---|---|---|
| `object_store` | `=0.13.2` (latest May 2026; 0.14 is in flight) | Apache-2.0; covers S3/GCS/Azure/local/HTTP. Uses rustls (`tls-webpki-roots` feature available, else system roots). |

### What NOT to do

- ❌ Re-export `object_store::ObjectStore` from `rollout-core`. Trait remains hand-rolled.
- ❌ Hand-roll S3 REST + signing. `aws-sdk-s3` already does it correctly with sigv4a + retries; reinventing is a waste.
- ❌ Hand-roll GCS REST + ADC. `gcloud-storage` already does it.

---

## 4. Work-Stealing Queue (DIST-02)

### Recommendation: **Custom over existing `Queue` trait — do NOT pull `crossbeam-deque`**

The DIST-02 requirement is for a **distributed** work-stealing queue (workers steal across the network from peers, backed by the durable `Queue` trait). `crossbeam-deque` is an **in-process** SPMC deque — wrong layer.

### Why not `crossbeam-deque`

- It solves a different problem: in-process Tokio task scheduling. Tokio's own scheduler already uses `crossbeam-deque` internally. We do not need a second layer in-process.
- Cross-node stealing is fundamentally an RPC + `Queue::dequeue_from(peer_id)` operation, backed by the existing tonic transport — no new dependency needed.

### Design (no new deps)

- Per-worker bounded local channel (`tokio::sync::mpsc` or `crossbeam_channel`) → already covered by `tokio` workspace dep.
- Steal protocol: new gRPC method on coordinator surface, `StealWork(from_worker_id, n) → Vec<WorkItem>`, gated by visibility timeouts so the durable backing queue (SQS / Pub/Sub / redb) remains the source of truth.
- Idempotency: rely on the v1.0 CAS sample-state machine — duplicate steals collapse on content-addressed IDs.

### Optional addition

| Crate | Version | When |
|---|---|---|
| `crossbeam-channel` | `0.5` | Only if a specific in-process MPMC path needs lock-free perf beyond what `tokio::sync::mpsc` gives. Defer unless a benchmark demands it. |

### What NOT to Add

- ❌ `crossbeam-deque` — wrong layer (see above).
- ❌ `flume` — duplicates `tokio::sync::mpsc`/`crossbeam-channel`.
- ❌ Any "distributed work queue" crate (e.g., `apalis`, `faktory-rs`) — too opinionated, brings transport choices we've already made.

---

## 5. Spot-Preemption Signal Sources (DIST-04)

### AWS

- **Source:** EC2 IMDSv2 at `http://169.254.169.254/latest/meta-data/spot/instance-action` (returns `stop` / `terminate` / `hibernate` ~2 minutes before reclamation).
- **Crate:** `aws-config::imds::client::Client` — already pulled by `aws-config`. **No new dep.**
- Token-based IMDSv2 auth handled by `aws-config`; never roll our own IMDS client.

### GCP

- **Source:** GCE metadata server at `http://metadata.google.internal/computeMetadata/v1/instance/preempted` (returns `TRUE` ~30 seconds before preemption; also `/instance/maintenance-event` for live-migration warnings on N1/N2).
- **Crate:** `gcloud-auth::credentials::mds::Client` — pulled by `gcloud-auth`. **No new dep.**
- Required header `Metadata-Flavor: Google` is set by the crate; never roll our own.

### Integration

- Wire both into a unified `PreemptionSignal` trait in `rollout-core`:
  ```rust
  trait PreemptionSignal: Send + Sync {
      fn poll(&self) -> impl Future<Output = Option<PreemptionNotice>>;
  }
  ```
- Cloud-specific impls live in `rollout-cloud-aws` / `rollout-cloud-gcp`. Local impl returns `None`.
- Worker subscribes; on `Some(notice)`, gracefully drains in-flight RPCs, flushes the last snapshot, releases lease to coordinator.

### What NOT to Add

- ❌ Polling library (`backoff`, `retry`) — straightforward `tokio::time::interval` with 1-5s cadence suffices.
- ❌ Any IMDS reimplementation (security-sensitive; defer to SDK).

---

## 6. Tool Harness Sandboxing (HARNESS-02)

### Scope reminder

From PROJECT.md: **"best-effort sandbox: process isolation + resource limits + path/HTTP allowlist; gVisor/Firecracker explicitly out."** This is a defense-in-depth surface, not a hard security boundary. Linux-only enforcement is acceptable; macOS dev can stub.

### Recommendation: Layered defense (Linux), Python sidecar uses `resource` module

| Layer | Crate / Mechanism | Pinned Version | Linux | macOS | Purpose |
|---|---|---|---|---|---|
| Process isolation | `tokio::process::Command` + `clone3(CLONE_NEWUSER\|CLONE_NEWPID\|CLONE_NEWNET)` via `rustix` | `rustix` `=1.1.4` | ✅ | stub | New user / pid / net namespaces; net namespace = default deny. |
| Filesystem sandbox | `landlock` | `=0.4.5` | ✅ (≥5.13) | stub | Path allowlist (read/write/exec) enforced by kernel. MSRV 1.71. |
| Capability-based FS API | `cap-std` | `=4.0.2` | ✅ | ✅ | Resolves paths relative to allowlisted dirs; defense-in-depth for Rust-side file ops. |
| Syscall filter | `seccompiler` | `=0.5.0` | ✅ (x86_64/aarch64/riscv64) | stub | BPF syscall allowlist; deny `execve`/`ptrace`/`socket` by default. Apache-2.0 OR BSD-3. |
| Resource limits | `rustix::process::{setrlimit, Resource}` | inherited | ✅ | partial | CPU time, AS, NOFILE, NPROC. |
| HTTP egress allowlist | App-level: route tool HTTP through a `hyper` client whose `Connect` impl checks host against allowlist before TCP. | — | ✅ | ✅ | Belt for the net-namespace braces. |
| Python sidecar self-limit | `import resource; resource.setrlimit(...)` + `os.chroot` (if root) | stdlib | ✅ | partial | Python-side mirror of rustix rlimits. |

### Why these crates

- **`landlock`**: Native kernel mechanism since 5.13; growing through 6.15 (ABI v7). MSRV 1.71 (compatible). Apache-2.0. No FFI to libseccomp.
- **`seccompiler`**: rust-vmm project (used by Firecracker). Pure Rust seccomp-bpf compiler — does not require `libseccomp` C library on the host. Apache-2.0 OR BSD-3-Clause (allowlist).
- **`rustix`**: We already pull this transitively via tokio; making it a direct dep gives us `setrlimit` + namespace syscalls without `libc` raw FFI.
- **`cap-std`**: Bytecode Alliance maintained. Apache-2.0-WITH-LLVM-exception OR Apache-2.0 OR MIT (allowlist; verify the `WITH-LLVM-exception` clause is on our cargo-deny allowlist — **flag for cargo-deny audit**).

### Alternatives Rejected

| Alternative | Why Not |
|---|---|
| `libseccomp` (Rust bindings to C lib) | FFI to libseccomp C library. Adds runtime dep on host; complicates static builds. `seccompiler` is pure Rust. |
| `nsjail` / `firejail` (external binary) | Adds runtime dependency on a system binary not on every Linux distro. We want a single-binary deployment. |
| `gVisor` / `Firecracker` | **Explicitly out of v1.1 per PROJECT.md.** |
| `bubblewrap` | External binary; same problem as nsjail. |
| `bpf-restrict` / `extrasafe` | Wrappers over landlock + seccompiler — convenient but add a layer we don't need; we'll wrap them ourselves at the harness layer. |

### Python sidecar (Tool harness)

```python
import resource, signal
resource.setrlimit(resource.RLIMIT_AS, (mem_bytes, mem_bytes))
resource.setrlimit(resource.RLIMIT_CPU, (cpu_secs, cpu_secs))
resource.setrlimit(resource.RLIMIT_NOFILE, (n_fds, n_fds))
signal.signal(signal.SIGXCPU, lambda *_: os._exit(137))
```

No new Python deps — stdlib only.

### What NOT to Add

- ❌ Any sandbox crate that compiles C (libseccomp-sys, capsicum-base) — keeps build clean.
- ❌ A "sandbox framework" crate that ties our hands on policy (e.g., `extrasafe`).

### cargo-deny flag

- `cap-std` license is `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` — **verify the LLVM-exception flavor is in `[licenses].allow`** before adding. If not, request only the `MIT`-licensed code path or add the exception to the allowlist.

---

## 7. Eval Dataset Bundling (HARNESS-03)

### Recommendation: **Runtime download via `hf-hub` (Rust HF Hub client), with vendored fallback for offline / air-gapped CI**

| Crate | Pinned Version | Rationale |
|---|---|---|
| `hf-hub` | `0.3` (latest stable; verify at integration time) | Pure-Rust async HF Hub client. Tokenizers project. Apache-2.0. rustls-backed. Used by `candle`, `tokenizers`. |
| (optional) bundled tarball | n/a | Ship a tiny SHA-pinned snapshot of the three benchmark test splits in `crates/rollout-harness-eval/data/` for air-gapped CI. ~MB-scale (MMLU test ≈14k items, IFEval ≈540, GSM8K test ≈1.3k). |

### Dataset specs

| Bench | Hub ID | Test split size | Format | Scoring |
|---|---|---|---|---|
| MMLU | `cais/mmlu` | ~14,042 multi-choice items, 57 subjects | parquet | exact match on letter (A-D) |
| IFEval | `google/IFEval` | ~541 prompts | parquet | constraint-satisfaction grader (open-source `instruction-following-eval` reference impl) |
| GSM8K | `openai/gsm8k` | ~1,319 grade-school math | parquet | answer-equivalence (regex on `####`) |

All three are < 50 MB total at the test-split layer. Vendoring is feasible if we want zero-network CI; we recommend **vendor with content-hash gate** to avoid surprise dataset drift.

### Why hf-hub

- Pure Rust, async, **rustls only** (no openssl).
- Apache-2.0 (license allowlist).
- Used by half the Rust ML ecosystem (`tokenizers`, `candle`); battle-tested.
- Supports auth via `HF_TOKEN` env var, mirroring how we already wire SecretStore.

### Parsing parquet

| Crate | Version |
|---|---|
| `parquet` (arrow-rs) | `=55.x` or whatever aligns with `object_store` 0.13.2 versioning (verify at integration; tracks arrow-rs releases). |
| `arrow-array` | matching parquet version |

Apache-2.0 ecosystem; cargo-deny clean. Rust 1.88-compatible (arrow-rs MSRV trails by ~6 months).

### What NOT to Add

- ❌ Python `datasets` library — Tool harness already supports Python plugins; we do NOT want to take a hard dep on `datasets` in the Rust core eval harness. (Tool/Env harnesses written in Python can use it freely via the sidecar.)
- ❌ Bare `reqwest` to hit raw HF URLs — reinvents auth, retry, cache layout, content-addressing.

### Integration

- `rollout-harness-eval` (new crate) ships:
  - `EvalLoader` trait with `mmlu`, `ifeval`, `gsm8k` impls.
  - At test time: read vendored data first; if absent, fall back to `hf-hub` download into the local CAS via the v1.0 `ObjectStore::put_bytes` (so future runs hit cache).
  - Scoring helpers live in the same crate.

---

## Summary Matrix

| Capability | New Crate(s) | Required / Optional | Feature-gated? |
|---|---|---|---|
| AWS cloud | aws-config, aws-sdk-s3, aws-sdk-sqs, aws-sdk-secretsmanager | Required (CLOUD-01) | Yes — `aws` feature on `rollout-cloud-aws` |
| GCP cloud | gcloud-storage, gcloud-pubsub, gcloud-secretmanager-v1, gcloud-auth | Required (CLOUD-02) | Yes — `gcp` feature on `rollout-cloud-gcp` |
| Object-store backend (Azure/HTTP/WebDAV) | object_store | Optional | Yes — `object-store-backend` feature on a new adapter crate |
| Work-stealing queue | none (custom over Queue + RPC) | Required (DIST-02) | n/a |
| Spot preemption | (covered by aws-config + gcloud-auth) | Required (DIST-04) | n/a |
| Tool sandbox (Linux) | rustix, landlock, seccompiler, cap-std | Required (HARNESS-02) | Yes — `sandbox` feature; macOS = stub |
| Eval datasets | hf-hub, parquet, arrow-array | Required (HARNESS-03) | n/a |

## Versions to Add to Workspace `Cargo.toml`

```toml
# v1.1 — AWS (CLOUD-01) — pin EXACT versions due to MSRV 1.88 → 1.91 drift
aws-config              = { version = "=1.8.17", default-features = false, features = ["behavior-version-latest", "rustls", "rt-tokio"] }
aws-sdk-s3              = { version = "=1.112.0", default-features = false, features = ["behavior-version-latest", "rt-tokio", "default-https-client", "sigv4a"] }
aws-sdk-sqs             = { version = "=1.65.0", default-features = false, features = ["behavior-version-latest", "rt-tokio", "default-https-client"] }   # verify exact MSRV-1.88 cohort number at integration
aws-sdk-secretsmanager  = { version = "=1.65.0", default-features = false, features = ["behavior-version-latest", "rt-tokio", "default-https-client"] }  # same

# v1.1 — GCP (CLOUD-02) — official googleapis/google-cloud-rust
gcloud-storage          = "1"   # follow monorepo release cohort; verify versions at integration
gcloud-pubsub           = "1"
gcloud-secretmanager-v1 = "1"
gcloud-auth             = "1"

# v1.1 — Object store optional adapter (CLOUD-03)
object_store            = { version = "=0.13.2", default-features = false, features = ["aws", "gcp", "azure", "tls-webpki-roots"] }

# v1.1 — Sandbox (HARNESS-02)
rustix                  = { version = "=1.1.4", features = ["process", "fs", "thread", "stdio"] }
landlock                = "=0.4.5"
seccompiler             = "=0.5.0"
cap-std                 = "=4.0.2"

# v1.1 — Eval datasets (HARNESS-03)
hf-hub                  = { version = "0.3", default-features = false, features = ["tokio", "rustls-tls"] }
parquet                 = { version = "55", default-features = false, features = ["async", "arrow"] }
arrow-array             = "55"
```

## Risk Flags for Roadmapper

1. **HIGH — aws-sdk-rust MSRV creep**: We are pinning to an old version cohort (s3 1.112) because current `main` requires Rust 1.91. **Either bump workspace MSRV to 1.91 in a precursor task or accept that we cannot pull AWS security/feature updates without an MSRV bump.** Recommend a small precursor plan in CLOUD-01 phase to evaluate the workspace impact of moving to 1.91 (does it break PyO3 0.28, tonic 0.14, anything else?).
2. **MEDIUM — gcloud-* monorepo versioning**: Google's official SDK uses date-cohort releases (e.g., `v20260319`). Exact crate versions need verification at integration time; the `"1"` placeholder above is a stub. Pin precisely once first integration PR lands.
3. **MEDIUM — cap-std license (`Apache-2.0 WITH LLVM-exception`)**: Verify against current `deny.toml` `[licenses].allow`. If absent, either add it or restrict to a different sandbox crate (cap-std is convenient but not load-bearing — `rustix` alone covers `openat`-style operations).
4. **MEDIUM — Tonic compatibility across cloud SDKs**: aws-sdk-rust uses hyper 1.x; gcloud-* uses tonic 0.12+. Verify no hyper-version conflicts in the dep graph at first integration. v1.0 workspace uses tonic 0.14 (hyper 1.x) — should align cleanly.
5. **LOW — arrow-rs/parquet MSRV**: arrow-rs MSRV historically follows Rust by ~6 months; v55 should be fine on 1.88 but verify before pinning.
6. **LOW — hf-hub version**: 0.3 is stable but the project iterates; verify exact version at integration.

## What is Explicitly NOT Being Added (cargo-deny + policy)

- ❌ Anything pulling `openssl-sys` (cargo-deny bans openssl).
- ❌ `native-tls` (forces openssl on Linux).
- ❌ `libseccomp` C library FFI (`seccompiler` is pure Rust).
- ❌ External sandbox binaries (`nsjail`, `firejail`, `bubblewrap`).
- ❌ gVisor / Firecracker integration (explicitly out per PROJECT.md).
- ❌ `crossbeam-deque` (wrong layer; Tokio already uses it internally).
- ❌ `rusoto` (abandoned).
- ❌ Python `datasets` as a Rust core dep (Python sidecars can use it freely; Rust core stays slim).

## Sources

- [aws-sdk-s3 1.112.0 Cargo.toml on docs.rs](https://docs.rs/crate/aws-sdk-s3/1.112.0/source/Cargo.toml.orig) — confirms `rust-version = "1.88.0"`, default features include `rustls` + `default-https-client`, no openssl.
- [aws-sdk-rust README](https://github.com/awslabs/aws-sdk-rust) — current MSRV 1.91.1 (May 2026), policy is "stable-2".
- [aws-sdk-rust Discussion #1257 — HTTPS client stack change](https://github.com/awslabs/aws-sdk-rust/discussions/1257) — default stack is hyper 1.x + rustls + aws-lc.
- [Google Cloud Rust (googleapis/google-cloud-rust)](https://github.com/googleapis/google-cloud-rust) — MSRV 1.87, official, Apache-2.0, last release May 2026.
- [Google Cloud Supported Rust versions](https://docs.cloud.google.com/rust/supported-rust-versions) — MSRV policy "Rust >= 1.87".
- [object_store on docs.rs](https://docs.rs/object_store/latest/object_store/) — v0.13.2, multi-backend, rustls + system roots default.
- [landlock crate](https://docs.rs/landlock) — v0.4.5, MSRV 1.71, supports Linux 5.13–6.15 (ABI v1–v7).
- [seccompiler crate](https://docs.rs/seccompiler) — v0.5.0, pure Rust, Apache-2.0 OR BSD-3-Clause.
- [cap-std crate](https://docs.rs/cap-std) — v4.0.2, Bytecode Alliance.
- [rustix crate](https://docs.rs/rustix) — v1.1.4, no sandboxing claim itself (delegate to cap-std/landlock).
- [crossbeam-deque docs](https://docs.rs/crossbeam-deque) — v0.8.6, confirmed wrong layer for distributed work-stealing.
- [HuggingFace Datasets loading guide](https://huggingface.co/docs/datasets/loading) — MMLU/IFEval/GSM8K standard load pattern.
