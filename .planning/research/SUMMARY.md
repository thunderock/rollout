# Project Research Summary

**Project:** rollout v1.1 (cloud + multi-node + harnesses)
**Domain:** LLM-RL post-training infrastructure — multi-cloud, multi-node, sandboxed harnesses
**Researched:** 2026-05-27
**Confidence:** HIGH

## Executive Summary

rollout v1.1 adds three pillars on top of the v1.0 substrate (13 crates, 19 traits, content-addressed determinism, dep-direction lint, schema-as-code): Cloud (AWS S3/SQS/SM + GCP GCS/Pub-Sub/SM via official SDKs), Distribution (multi-node coordinator + work-stealing pull queue + coordinator restart + spot-preemption drain), and Harnesses (text-completion env + best-effort tool sandbox + MMLU/IFEval/GSM8K eval). All 10 requirements are additive: nothing in v1.0's public API breaks. The 5 new crates (`rollout-cloud-aws`, `rollout-cloud-gcp`, `rollout-harness-text`, `rollout-harness-tool`, `rollout-evals`) join the existing 13, extend already-shipped traits with default-impl-backward-compatible methods, and satisfy the existing dep-direction lint whose invariant array already enumerates these crate names.

The recommended build order is Cloud → Distribution → Harnesses, because the cloud layer (especially `ObjectStore::put_stream/get_stream` and `Queue::dequeue_with_lease`) is consumed by the distribution layer (coordinator state persistence, work-stealing, spot drain), and harnesses are laterally independent of both but depend on `ObjectStore` and `WorkQueue` for trajectory/eval caching. The hardest single item is **DIST-03 (coordinator restart)**: unlike Ray (Raft+GCS), Slurm (backup-controller), or Temporal (event-history replay), rollout's "coordinator is a stateless replayer against Storage" pattern has no direct peer-framework template; it requires careful split-brain fencing via a Postgres lease row and per-RPC epoch validation on every worker.

The dominant risk cluster is the **cloud-impl surface**: SDK type leakage through trait bounds, emulator-vs-production behavioral divergence, IMDSv2 correctness, S3 multipart abort hygiene, and transitive license drift (aws-lc-rs `OpenSSL` license, cap-std `Apache-2.0 WITH LLVM-exception`). A secondary risk cluster is **Phase 6 correctness**: work-stealing dedup races during fence-epoch flip, split-brain from a hung-but-not-dead old coordinator, and the latent Postgres `scan_bytes` wildcard bug carried from v1.0 that becomes load-bearing in multi-node. Both clusters have clear prevention strategies documented per-pitfall with named CI jobs and test fixtures; the key discipline is landing the prevention gates in the same PR as the feature, never after.

---

## Key Findings

### Recommended Stack

v1.1 adds 11 net-new crate dependencies. The AWS SDK cohort is pinned exact (`=`) at `aws-sdk-s3 =1.112.0` / `aws-config =1.8.17` because the current aws-sdk-rust `main` requires Rust 1.91 while our workspace pins 1.88; `cargo update` will silently resolve to the MSRV-incompatible version without exact pins. The GCP SDK is `googleapis/google-cloud-rust` (official, Apache-2.0, MSRV 1.87), not the community yoshidan crates which were renamed. The tool sandbox uses a layered defense (`rustix` + `landlock` + `seccompiler` + `cap-std`) with no C FFI and no external sandbox binaries, keeping the single-binary deployment model. Eval dataset loading uses `hf-hub` (pure Rust, rustls-only) with vendored 10-row fixtures as the always-on CI path. Distributed work-stealing needs no new dependency: it is custom RPC over the existing `Queue` trait and tonic transport.

**Core new technologies:**
- `aws-sdk-s3 =1.112.0` / `aws-config =1.8.17` + cohort — AWS cloud impls; **exact-pin mandatory** due to MSRV 1.88→1.91 drift; pin verified against docs.rs `rust-version = "1.88.0"` in Cargo.toml.orig
- `gcloud-storage/pubsub/secretmanager-v1/auth` (googleapis monorepo, version placeholder pending integration-time confirmation) — GCP official SDK; MSRV 1.87, Apache-2.0
- `rustix =1.1.4` + `landlock =0.4.5` + `seccompiler =0.5.0` + `cap-std =4.0.2` — layered sandbox (Linux-only enforcement, macOS stub); cap-std license `Apache-2.0 WITH LLVM-exception` needs `deny.toml` audit
- `hf-hub 0.3` + `parquet 55` + `arrow-array 55` — eval dataset loading; all Apache-2.0, rustls-only, MSRV-compatible
- No new crates for work-stealing or spot preemption — reuse tonic + `aws-config::imds` + `gcloud-auth::mds`

**Cross-document reinforcements:**
- STACK.md pins `aws-sdk-s3 =1.112.0`; ARCHITECTURE.md adds 5 crates consuming this cohort; PITFALLS.md §14 flags transitive aws-lc-rs license risk and MSRV pin drift. All three documents agree: exact-pin + `deny-cloud-features` CI gate is non-negotiable.
- STACK.md flags `cap-std` license; PITFALLS.md §14 independently flags the same — strong cross-document agreement. Must address in Phase 5 PR 1.

### Expected Features

All 10 requirements land in v1.1. The invariant: every requirement has a load-bearing CI test running without GPU or cloud credentials.

**Must have (all P1):**
- CLOUD-01/02/03: Full AWS + GCP impl of `ObjectStore`, `WorkQueue`, `SecretStore`, `ComputeHint`; snapshot storage via `ObjectStore` (wiring only); conformance test suite parameterized over FS/S3/GCS
- DIST-01/02/03/04: Multi-node coord + workers; work-stealing pull queue with lease semantics and dedup; coordinator restart from Storage (SPOF elimination); graceful spot-preemption drain (120s AWS / 30s GCP)
- HARNESS-01/02/03: Text-completion env with plugin-host reward; tool sandbox (seccomp + cgroups v2 + path/HTTP allowlists, Linux full / macOS stub); MMLU + IFEval + GSM8K eval with `rollout eval` CLI + offline-mode default

**Defer to v1.2+:** gVisor/Firecracker, CRIU, tool-call streaming inference (INFER-02), PPO/GRPO (RL-01..04), LLM-as-judge eval, Azure/OCI clouds, lm-eval-harness YAML compat (P2 stretch)

**Anti-features to enforce explicitly:** no cross-cloud single run, no `object_store` crate as the core trait definition, no priority queues, no active-active coordinator pair, no Raft/etcd dependency

### Architecture Approach

v1.1 is a **5-crate addition onto a fixed 13-crate substrate** with **~16 new/expanded trait methods**, all on `rollout-core`. The dep-direction lint (10 invariants in v1.0) grows to 13 invariants with 4 new violation fixtures — and the invariant array already enumerates the new crate names, so the architecture is pre-wired. Coordinator state that was in-memory in v1.0 (work assignments, fence epoch, queue-item bindings) moves to Storage under new namespaces (`"work"`, `"epoch"`, `"queue_items"`), enabling coordinator restart with no new infrastructure dependency (redb for dev, Postgres for production multi-node). Harness traits expand from single-method placeholders to full contracts that v1.2 PPO/GRPO will consume. The `rollout-evals` crate name (per the lint array) conflicts with naming symmetry for the other harness crates — recommend renaming to `rollout-harness-eval` and updating the lint in the same PR.

**Major components added:**
1. `rollout-cloud-aws` / `rollout-cloud-gcp` — implement four cloud traits each; gated behind `aws`/`gcp` Cargo features on `rollout-cli`; independent of each other (invariant #10)
2. `rollout-coordinator` (modified) — lifts assignment state into Storage; adds `pull_work`/`complete_work`/`drain_request`; Postgres lease row for coordinator fencing; fence-epoch CAS
3. `rollout-harness-text` / `rollout-harness-tool` / `rollout-evals` — algo-layer crates (no cloud deps); satisfy expanded harness traits
4. `RunConfig` schema additions: `CloudConfig`, `CoordinatorConfig`, `QueueConfig`, `HarnessConfig` — each touching PR must `cargo xtask schema-gen` and commit drift

**Cross-document tension (ARCHITECTURE.md vs PITFALLS.md):** ARCHITECTURE.md §2.3 marks the Vec<u8>-buffering `put_stream` default as "slow path" but permits it; PITFALLS.md §16 argues the default should carry `#[deprecated]` to force cloud impl overrides. Resolution: **add `#[deprecated(note = "Override for cloud-backed stores; default buffers")]` on the default impl.**

### Critical Pitfalls

6 load-bearing prevention strategies (from 17 documented pitfalls):

1. **SDK type leakage into `rollout-core`** — dep-direction invariant #14 + `public-api-cloud-leak` CI job in Phase 5 PR 1, before any SDK crate lands. `put_stream`/`get_stream` parameters must be `Pin<Box<dyn AsyncRead + Send>>` and `ContentId` only; SDK errors collapse to `CoreError` strings at the crate boundary.

2. **Split-brain coordinator** — Postgres single-row coordinator lease (`UPDATE ... WHERE expires_at < now() RETURNING ...`); old coordinator self-aborts on stale-lease detection; workers validate `coord_epoch` on every RPC. CI test: `split_brain_old_coord_self_fences`. Must land in Phase 6 PR 1.

3. **Work-stealing dedup race during fence-epoch flip** — CAS on item state (not lease) in `Coordinator::complete_work`; `extend_lease` for long-running work. CI test: `concurrent_ack_and_steal_no_double_execute`. Tool `invoke()` side effects are not idempotent — flag this for v1.2.

4. **Tool harness sandbox escapes (5 sub-pitfalls)** — CI grep bans `shell=True` and `libc::fork(`; HTTP tool uses IP-allowlist post-DNS with RFC1918/link-local/loopback rejection and redirect re-validation; file tool uses `cap-std::fs::Dir` + `O_NOFOLLOW | O_RESOLVE_BENEATH`; seccomp allowlist is strace-derived with explicit `clone3`/`openat2`/`faccessat2`. Each sub-pitfall gets its own test fixture in the same PR as the corresponding tool.

5. **S3 multipart orphan + blake3 retry hash divergence** — `MultipartGuard` type with Drop-impl that spawns abort task; hash-before-send pattern (buffer chunk → hash → pass `Bytes` to SDK so retry replays same bytes). Bucket lifecycle policy as belt to the suspenders.

6. **Postgres `scan_bytes` wildcard parity (latent v1.0 bug)** — fix as Phase 5 precursor task (byte-range `WHERE key >= / <`, not LIKE), with proptest parity test over 0x00–0xFF. Carrying into Phase 6 causes intermittent coordinator-restart failures.

---

## Implications for Roadmap

### Phase 5: Cloud Layer + Object-Store Snapshots (CLOUD-01/02/03)

**Rationale:** Cloud traits must exist and be correct before distribution can use them. `ObjectStore::put_stream` and `Queue::dequeue_with_lease` are consumed by Phase 6. Phase 5 also contains the Postgres `scan_bytes` precursor fix — the latent v1.0 bug becomes load-bearing in multi-node.

**Delivers:** Working S3 + GCS + SQS + Pub/Sub + Secrets Manager implementations; streaming snapshot storage; `bit_identical_resume_at_step_5_via_{s3,gcs}` witnesses; `cloud-emulator-{aws,gcp}` always-on CI jobs; dep-direction invariants 11–14; `deny-cloud-features` and `public-api-cloud-leak` CI gates.

**Pitfalls addressed this phase:** #1 (SDK leakage), #2 (emulator vs prod), #3 (IMDSv1), #4 (S3 multipart), #5 (GCS resumable), #9 (cross-provider resume), #14 (cargo-deny license), #15 (nested Tokio runtime), #16 (blake3 retry hash), #17 (scan_bytes precursor)

**Build order within phase:** (1) `rollout-core` trait extensions + `rollout-cloud-local` updates → (2) scan_bytes fix in `rollout-storage` → (3) `rollout-cloud-aws` per-trait PRs (S3 → SQS → SM+IMDSv2) → (4) `rollout-cloud-gcp` per-trait PRs (GCS → Pub/Sub → SM+GCE metadata) → (5) snapshot streaming witnesses

**Research flag: Standard patterns — skip research.** All crate choices confirmed with exact versions. Validation tasks only: verify `gcloud-*` exact monorepo version numbers at integration time; audit `aws-lc-rs` license SPDX string before updating `deny.toml`.

---

### Phase 6: Multi-Node Distribution (DIST-01/02/03/04)

**Rationale:** Depends on Phase 5's `Queue::dequeue_with_lease`, `ObjectStore::put_stream`, and cloud `ComputeHint` impls. DIST-03 (coordinator restart) is the hardest piece in v1.1 — doing it after the cloud layer is stable means distribution bugs are not conflated with cloud-impl bugs.

**Delivers:** Pull-based multi-node coordinator (`pull_work`/`complete_work`/`drain_request`); coordinator state persisted to Storage under new namespaces; fence-epoch CAS; Postgres coordinator lease (split-brain protection); graceful spot-preemption drain; 3-node real-cloud smoke; `coord_restart_no_duplicates` and `split_brain_old_coord_self_fences` always-on CI tests.

**Pitfalls addressed this phase:** #6 (dedup race), #7 (split-brain), #8 (preemption hint), #13 (fork after PyO3)

**Build order within phase:** (1) `rollout-core::Coordinator` extensions → (2) work-state machine in `rollout-coordinator` (assignment ledger in Storage) → (3) fence epoch + Postgres coordinator lease → (4) worker-side pull loop in `rollout-runtime-batch` → (5) spot drain orchestration → (6) 3-node real-cloud smoke

**Research flag: Phase 6 needs an architecture spike on DIST-03 before planning.** The storage-backed stateless-replayer pattern with Postgres-lease fencing is bespoke; Temporal/Ray/Slurm analogies are useful but not directly applicable. Recommended spike: write the `coordinator_lease` table schema and the `split_brain_old_coord_self_fences` test skeleton before committing to the PR plan.

---

### Phase 7: Harnesses (HARNESS-01/02/03)

**Rationale:** Harnesses depend on Phase 5's `ObjectStore` (trajectory/eval result caching) and Phase 6's `WorkQueue` (eval runs as WorkQueue jobs). The three harnesses are laterally independent and can be developed in parallel internally. HARNESS-02 is the most complex item; HARNESS-01 and HARNESS-03 are well-documented.

**Delivers:** Text-completion env with `EchoEnv` + plugin-host reward; best-effort tool sandbox (process isolation + seccomp + cgroups v2 + path/HTTP allowlists); MMLU + IFEval + GSM8K eval with `rollout eval` CLI + offline-mode default + hash-pinned dataset fixtures; CI witnesses: `env_deterministic_replay`, `tool_sandbox_escape_blocked`, `eval_score_matches_lm_eval_harness`.

**Pitfalls addressed this phase:** #10a–e (sandbox escapes), #11 (sync eval blocks loop), #12 (HF rate limits)

**Build order within phase:** (1) `rollout-core::traits::harness` expansion (new return types) → (2) `rollout-harness-text` (pure Rust) → (3) `rollout-harness-tool` process isolation sandbox → (4) `rollout-harness-tool` path/HTTP allowlist → (5) `rollout-evals` loaders + scorers + CLI + fixtures

**Research flag: HARNESS-02 seccomp allowlist needs a targeted exercise before planning.** Run `strace -c python3 -c 'print(1)'` against the actual sandbox Python version to derive ground-truth `clone3`/`openat2`/`faccessat2` requirements. 1–2 hour exercise; prevents kernel-version CI failures. HARNESS-01 and HARNESS-03 are standard patterns — skip research for those.

---

### Phase Ordering Rationale

- Cloud before Distribution: `Queue::dequeue_with_lease`, `ObjectStore::put_stream`, and cloud `ComputeHint::preemption_signal` are required inputs for the distribution layer.
- Postgres `scan_bytes` fix as Phase 5 precursor: carrying the v1.0 latent bug into Phase 6 multi-node coordinator namespaces causes intermittent coordinator-restart failures. Fix cost is ~1 day; discovery cost post-Phase 6 is ~1 week.
- Harnesses after Distribution: eval runs as WorkQueue jobs; trajectory storage uses `put_stream`. Harnesses can be developed in parallel with late Phase 6 work but not integration-tested against live cloud until Phase 6 stabilizes.
- All phases preserve the v1.0 no-GPU-no-cloud-creds CI discipline.

---

### Research Flags

**Needs research / architecture spike:**
- **Phase 6 (DIST-03):** coordinator lease-based fencing design spike before planning
- **Phase 7 (HARNESS-02 seccomp):** strace-derived allowlist ground truth before planning

**Standard patterns — skip research:**
- Phase 5 cloud impls (all crate choices confirmed with exact versions)
- Phase 5 streaming put/get (blake3 incremental pattern documented with code in PITFALLS.md §16)
- Phase 7 HARNESS-01 (pure Rust trait + mock; no external research)
- Phase 7 HARNESS-03 (lm-eval-harness is the documented reference; MMLU/IFEval/GSM8K formats known)

---

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | All crate choices verified against docs.rs at May 2026 versions; MSRV verified for 1.88; license status confirmed for all except aws-lc-rs (needs `cargo metadata` audit). GCP exact versions need confirmation at integration. |
| Features | HIGH | All 10 requirements have table-stakes/differentiator/anti-feature breakdowns with explicit dependency graph. DIST-03 shape is medium-confidence: well-motivated but less directly cited in peer literature. |
| Architecture | HIGH | Grounded in direct repo inspection (ARCHITECTURE.md cites file paths and line numbers). 5 new crates, ~16 trait method additions, 4 new invariants — all specific and traceable to existing code. |
| Pitfalls | HIGH | 17 pitfalls with named prevention CI jobs, test fixtures, phase assignments, and recovery cost estimates. Strong cross-document cross-referencing (STACK.md × PITFALLS.md on licenses, MSRV, IMDS). |

**Overall confidence: HIGH**

### Gaps to Address During Planning / Implementation

1. **gcloud-* exact crate versions:** STACK.md uses placeholder for all gcloud-* crates. Verify and pin exact monorepo release cohort at Phase 5 PR time.
2. **aws-lc-rs license exact SPDX string:** audit `cargo metadata | jq '.packages[] | select(.name == "aws-lc-rs") | .license'` before updating deny.toml; if OpenSSL advertising clause is present, legal review advisable before public 0.1.0 release.
3. **Rust workspace MSRV bump decision (1.88 → 1.91):** STACK.md recommends evaluating PyO3/tonic impact as a Phase 5 precursor task; if low-impact, bump to 1.91 and drop exact-pin constraint on AWS SDKs.
4. **`rollout-evals` vs `rollout-harness-eval` crate name:** decide before Phase 7 planning; requires one-line change in `dependency_direction.rs` + PROJECT.md update.
5. **PROJECT.md crate count (17 → 18):** update in the same PR that introduces the 5th new crate.
6. **IFEval language-detection constraints:** document skip in `rollout-evals` README and emit plan-time warning; affects benchmark comparability with lm-eval-harness.

---

## Sources

**Primary (HIGH confidence):** repo inspection (`traits/cloud.rs`, `dependency_direction.rs`, `rollout-coordinator/src/lib.rs`), aws-sdk-s3 1.112.0 Cargo.toml.orig on docs.rs, aws-sdk-rust README (MSRV 1.91.1 policy), googleapis/google-cloud-rust (MSRV 1.87, Apache-2.0), AWS S3 multipart lifecycle docs, AWS IMDSv2 docs, GCS resumable upload protocol, GCP Spot VM preemption docs, Linux landlock kernel docs, PyO3 0.28 sub-interpreters docs, blake3 incremental hashing API, HuggingFace gated dataset docs, EleutherAI lm-evaluation-harness, Temporal durable execution docs.

**Secondary (MEDIUM confidence):** Ray RLlib architecture, Slurm HA controller, agent sandbox isolation landscape (2026 secondary), RLHF infrastructure comparison, NeMo-Aligner paper.

---

*Research completed: 2026-05-27*
*Ready for roadmap: yes*
