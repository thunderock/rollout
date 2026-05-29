# Roadmap (planning index)

The narrative roadmap lives at the repo root: [`../ROADMAP.md`](../ROADMAP.md). This document is the planning index that maps requirements (REQ-IDs in `REQUIREMENTS.md`) to phases, plus the active-milestone phase detail consumed by `/gsd:plan-phase`.

## Milestones

| Milestone | Status | Phases | Shipped |
|---|---|---|---|
| **v1.0 — substrate + train** | ✓ SHIPPED | 1, 2, 3, 4 | 2026-05-27 |
| **v1.1 — cloud + multi-node + harnesses** | active | 5, 6, 7 | — |
| v1.2 — online inference + RL + offline + spot | planned | 8, 9, 10, 11 | — |
| v1.0 release ship | planned | 12 | — |

<details>
<summary><strong>v1.0 — substrate + train (✓ SHIPPED 2026-05-27)</strong></summary>

- 4 phases · 30 plans · 59 tasks · 112 commits · 18.6k LOC · 7-day cycle
- 18/18 v1.0 requirements satisfied (CORE-01..05, SUBSTR-01..04, BACKEND-01..02, TRAIN-01..04, DOCS-01..03)
- Archive: [`milestones/v1.0-ROADMAP.md`](milestones/v1.0-ROADMAP.md), [`milestones/v1.0-REQUIREMENTS.md`](milestones/v1.0-REQUIREMENTS.md), [`milestones/v1.0-MILESTONE-AUDIT.md`](milestones/v1.0-MILESTONE-AUDIT.md)
- Retrospective: [`RETROSPECTIVE.md`](RETROSPECTIVE.md)

</details>

## Phase → Requirements

| Phase | Name | Requirements delivered | Status |
|---|---|---|---|
| 1 | Core foundations | CORE-01, CORE-02, CORE-03, CORE-04, CORE-05, DOCS-01, DOCS-02, DOCS-03 | ✓ v1.0 |
| 2 | Local substrate | SUBSTR-01, SUBSTR-02, SUBSTR-03, SUBSTR-04 | ✓ v1.0 |
| 3 | Inference backend + batch | BACKEND-01, BACKEND-02 | ✓ v1.0 |
| 4 | SFT + RM + train-state snapshots | TRAIN-01, TRAIN-02, TRAIN-03, TRAIN-04 | ✓ v1.0 |
| 5 | Cloud layer + object-store snapshots | CLOUD-01, CLOUD-02, CLOUD-03, CLOUD-04 | **v1.1 active** |
| 6 | Multi-node distribution | DIST-01, DIST-02, DIST-03, DIST-04, DIST-05 | **v1.1 active** |
| 7 | Harnesses (env + tool + eval) | HARNESS-01, HARNESS-02, HARNESS-03 | **v1.1 active** |
| 8 | Online inference + episodic memory | INFER-01, INFER-02, INFER-03, INFER-04 | v1.2 planned |
| 9 | PPO + GRPO + buffer snapshots | RL-01, RL-02, RL-03, RL-04 | v1.2 planned |
| 10 | DPO / IPO / KTO | OFFLINE-01, OFFLINE-02, OFFLINE-03 | v1.2 planned |
| 11 | Process snapshots + spot recovery | SNAPSHOT-01, HARNESS-04 | v1.2 planned |
| 12 | Hardening + 1.0 | SHIP-01, SHIP-02, SHIP-03, SHIP-04 | planned |

> **Note:** `HARNESS-04` (eval gate) moved from Phase 7 to Phase 11 (v1.2) — it needs algo+dist+harness coupling and lands with PPO consumers. See `REQUIREMENTS.md` for the deferral rationale.

## Exit criteria

Each phase has measurable exit criteria stated in the narrative roadmap. They are not duplicated here to avoid drift; this index is purely the mapping plus the active-milestone phase detail below.

## Coverage

100% — every v1 requirement maps to exactly one phase.

**v1.1 coverage:** 12/12 in-scope requirements mapped (Phase 5: 4 · Phase 6: 5 · Phase 7: 3).

**Cross-cutting requirements:** `DOCS-01` (docs site bootstrap), `DOCS-02` (per-commit doc/test policy), and `DOCS-03` (rustdoc CI gate) bootstrap in Phase 1 but apply to **every** phase thereafter — every phase's plans must enforce doc + test updates per commit.

**v1 release gate:** `SHIP-03` is hardened — v1 cannot ship without at least one end-to-end working model example. Recipe lands progressively (Phase 4 stub → Phase 9 real → Phase 12 documented).

The narrative `../ROADMAP.md` is authoritative for goals, risks, and exit criteria. The phase-detail `.planning/phase-N/` directories (created later by `/gsd:plan-phase N`) are authoritative for tasks.

---

## Milestone v1.1 — cloud + distribution + harnesses

**Goal:** Lift v1.0's local substrate to real multi-host runs on real cloud, with the harness surface needed to feed RL training (RL phases stay v1.2).

**Phases:** 3 (5, 6, 7) — continued from v1.0 phase numbering.
**In-scope requirements:** 12 (CLOUD-01..04, DIST-01..05, HARNESS-01..03).
**Build order:** Cloud (Phase 5) → Distribution (Phase 6) → Harnesses (Phase 7). DIST consumes Phase 5's `ObjectStore::put_stream` + `Queue::dequeue_with_lease`; Harnesses consume both.
**Proof bar:** 3+ node setup runs `make smoke` against real AWS/GCP; spot-preempt signal triggers graceful drain. Every phase ships a load-bearing CI witness that runs without GPU or cloud creds.

### Phases (summary)

- [ ] **Phase 5: Cloud layer + object-store snapshots** — AWS + GCP impls of `ObjectStore` / `Queue` / `SecretStore` / `ComputeHint` over the v1.0 trait surface; streaming put/get for big-blob snapshots; `rollout cloud doctor` CLI.
- [ ] **Phase 6: Multi-node distribution** — Pull-based coordinator with state persisted in Storage; work-stealing queue with lease/CAS dedup; coordinator restart from storage; spot-preemption graceful drain; split-brain fencing.
- [ ] **Phase 7: Harnesses (env + tool + eval)** — Text-completion env with plugin-host reward; best-effort tool sandbox (process isolation + seccomp + cgroups v2 + path/HTTP allowlists; Linux full / macOS dev-only stub); MMLU + IFEval + GSM8K eval with `rollout eval` CLI + offline-mode default.

### Phase Details

#### Phase 5: Cloud layer + object-store snapshots

**Goal**: An operator can run the existing SFT/RM/batch-inference flows against real AWS or GCP buckets and queues, with snapshots streaming to object storage — same config schema as v1.0, single `cloud.provider` flip.

**Depends on**: Phase 4 (v1.0 — Snapshotter + Postgres backend shipped).

**Requirements**: CLOUD-01, CLOUD-02, CLOUD-03, CLOUD-04.

**Precursor tasks (no new REQ-ID — folded into Phase 5 plan):**
1. Postgres `scan_bytes` wildcard parity fix (v1.0 latent; becomes load-bearing in Phase 6's multi-node coordinator namespaces).
2. `rollout-evals` → `rollout-harness-eval` rename + dep-direction lint update (symmetry with the two other harness crate names; lands before Phase 7 plan).
3. Rust workspace MSRV bump evaluation (1.88 → 1.91 spike): does PyO3 0.28 / tonic 0.14 / rest of workspace tolerate it? If yes, drop AWS SDK exact-pins.

**Success Criteria** (what must be TRUE):
  1. Operator runs `rollout train sft --config examples/sft-tiny-aws.toml` (or `-gcp`) and the run completes against real S3+SQS+SecretsManager (or GCS+Pub/Sub+SecretManager) — same `examples/sft-tiny.toml` shape with a `[cloud]` block flipped.
  2. `rollout cloud doctor --provider aws` (and `--provider gcp`) returns clean on a freshly-bootstrapped cloud account — verifies reachability, auth, write-test against a scratch bucket+queue+secret.
  3. Byte-identical SFT/RM resume holds over cloud storage: `bit_identical_resume_at_step_5_via_s3` + `bit_identical_resume_at_step_5_via_gcs` run on every commit against localstack / fake-gcs-server (no live creds).
  4. `cargo test --workspace --tests` and `architecture-lint` stay green with 4 new dep-direction invariants (aws ↛ gcp, harness ↛ cloud, coord ↛ cloud, no SDK type leakage into `rollout-core` public API).
  5. CI gains always-on `cloud-emulator-aws` + `cloud-emulator-gcp` jobs (localstack + fake-gcs-server) running the same `ObjectStore`/`Queue` conformance suite that `rollout-cloud-local` already passes.

**Plans:** 8 plans

Plans:
- [x] 05-01-precursor-postgres-scan-bytes-fix-PLAN.md — Wave 1 precursor A: StorageKey::validate_for_postgres + proptest parity (lands as standalone PR against `main` BEFORE Phase 5 stages 1-5)
- [x] 05-02-precursor-rollout-evals-rename-PLAN.md — Wave 1 precursor B: rename `rollout-evals` → `rollout-harness-eval` in dep-direction lint + planning docs (standalone PR)
- [x] 05-03-precursor-msrv-bump-PLAN.md — Wave 1 precursor C: spike + BUMP/STAY decision for Rust MSRV 1.88 → 1.91 (standalone PR; has `checkpoint:decision`)
- [x] 05-04-stage1-trait-extensions-ci-gates-PLAN.md — Wave 2 stage 1: `ObjectStore::put_stream`/`get_stream` + `Queue::dequeue_with_lease`/`extend_lease` trait extensions + CloudConfig schema + 4 new dep-direction invariants (#11-14) + `public-api-cloud-leak` + `forbidden-patterns` CI gates
- [x] 05-05-stage2-cloud-aws-impl-PLAN.md — Wave 3 stage 2: `rollout-cloud-aws` impls (S3 → SQS → SecretsManager + IMDSv2) + MultipartGuard + blake3-hash-before-send + cloud-emulator-aws CI job; addresses CLOUD-01
- [x] 05-06-stage3-cloud-gcp-impl-PLAN.md — Wave 3 stage 3: `rollout-cloud-gcp` impls (GCS → Pub/Sub → SM + GCE MDS) + cloud-emulator-gcp CI job + in-test mock secret manager; addresses CLOUD-02
- [x] 05-07-stage4-snapshot-streaming-witnesses-PLAN.md — Wave 4 stage 4: `bit_identical_resume_at_step_5_via_{s3,gcs}` always-on witnesses + cross-provider portability witness + examples/sft-tiny-{aws,gcp}.toml; addresses CLOUD-03
- [x] 05-08-stage5-rollout-cloud-doctor-PLAN.md — Wave 5 stage 5: `rollout cloud doctor` CLI subcommand (7 checks, human + json output, exit 0/1/2); addresses CLOUD-04

#### Phase 6: Multi-node distribution

**Goal**: A run spans multiple hosts on real cloud; idle workers steal from busy ones; coordinator restart is invisible to overall progress; spot preemption drains gracefully without data loss.

**Depends on**: Phase 5 (`ObjectStore::put_stream`, `Queue::dequeue_with_lease`, cloud `ComputeHint::preemption_signal` impls all required).

**Requirements**: DIST-01, DIST-02, DIST-03, DIST-04, DIST-05.

**Architecture spike before planning**: DIST-03 (coordinator restart) is the hardest single item in v1.1 — the storage-backed stateless-replayer pattern with Postgres-lease fencing has no direct peer-framework template (Ray uses Raft+GCS, Slurm uses backup-controller, Temporal uses event-history replay — none directly apply). Write the `coordinator_lease` table schema and the `split_brain_old_coord_self_fences` test skeleton before committing to the PR plan.

**Success Criteria** (what must be TRUE):
  1. Operator runs `make smoke-3node-aws` (and `-gcp`); 1 coordinator + 3 workers spin up, exchange heartbeats, dequeue work, and the run reports `done` within 30s — no GPU, mock backend, real cloud transport.
  2. Operator kills the coordinator mid-run on the 3-node smoke; a fresh coordinator process starts, recovers assignment ledger + fence epoch from Postgres, and the run completes with zero duplicate sample IDs (witnessed by `coord_restart_no_duplicates` test running on every CI build via in-process simulation).
  3. Operator triggers a mock spot-preemption signal on a worker (AWS budget 60s / GCP 15s); worker stops pulling, requeues in-flight items via lease nack, opportunistically snapshots if budget allows, deregisters cleanly — surviving workers pick up the requeued items and the run completes (`spot_drain_completes_within_lead_time`).
  4. Two coordinator processes started against the same Postgres lease detect the split-brain: exactly one self-fences (`std::process::abort`) within 5s, the survivor advances the epoch, workers reject responses tagged with the stale epoch (`split_brain_old_coord_self_fences`).
  5. Work-stealing dedup race during fence-epoch flip never produces double-execution: `concurrent_ack_and_steal_no_double_execute` exercises CAS-on-state collapsing duplicate acks, runs on every commit.

**Plans**: 5 plans

Plans:
- [x] 06-00-wave0-test-infra-lease-trait-PLAN.md — Wave 0: CoordinatorLease trait (rollout-core, no SDK) + shared WorkItemRecord CAS module + in-process N-worker sim harness + subprocess abort harness (DIST-01..05 infra)
- [x] 06-01-lease-epoch-fencing-PLAN.md — Wave 1: StorageLease (dual-backed single-row CAS) + epoch stamping/rejection + self-fence; witnesses lease_exclusion_single_winner (SC1) + split_brain_old_coord_self_fences (SC4); DIST-01, DIST-05
- [x] 06-02-work-ledger-stealing-PLAN.md — Wave 2: queue_items dispatch + coordinator-mediated steal (ceil(n/2), MAX_STEAL_BATCH) + CAS dedup; witness concurrent_ack_and_steal_no_double_execute (SC5); DIST-02
- [ ] 06-03-restart-replayer-spot-drain-PLAN.md — Wave 3: stateless-replayer boot + spot-drain state machine + D-SPOT-04 doc reconciliation; witnesses coord_restart_no_duplicates (SC2) + spot_drain_completes_within_lead_time (SC3); DIST-03, DIST-04
- [ ] 06-04-smoke-cli-pg-lane-PLAN.md — Wave 4: make smoke-3node-aws/-gcp (1 coord + 3 workers, mock backend) + Postgres-lease CI lane + --test-fence abort subcommand + mdBook chapter; closes all 5 SCs

#### Phase 7: Harnesses (env + tool + eval)

**Goal**: An LLM can interact with text-completion environments, invoke sandboxed tools, and be scored on bundled evals — three new algo-layer crates that v1.2 PPO/GRPO will consume directly via trait objects.

**Depends on**: Phase 5 (`ObjectStore` for trajectory + eval-result caching) and Phase 6 (`WorkQueue` for eval-as-job; not strictly required at PR time for HARNESS-01/03 but lands after Phase 6 stabilizes so harness CI doesn't conflate with cloud bugs).

**Requirements**: HARNESS-01, HARNESS-02, HARNESS-03.

**Strace exercise before planning**: HARNESS-02 seccomp allowlist needs `strace -c python3 -c 'print(1)'` ground truth (and the same for the shell/file/http tools) to derive `clone3` / `openat2` / `faccessat2` / `rseq` / `arch_prctl` requirements. 1–2 hour exercise; prevents kernel-version CI failures. HARNESS-01 and HARNESS-03 are standard patterns — skip the spike for those.

**Success Criteria** (what must be TRUE):
  1. Operator runs `cargo test -p rollout-harness-text` and a deterministic-replay test (`env_deterministic_replay`) shows the same seed produces the same trajectory, with `EchoEnv` + `MockRewardEnv` exercising the plugin-host reward path — no GPU, no cloud creds.
  2. Operator runs `cargo test -p rollout-harness-tool` (Linux); negative tests (`tool_sandbox_escape_blocked`, `http_tool_blocks_dns_rebinding`, `http_tool_blocks_redirect_to_imds`, `sandbox_blocks_userns`, `seccomp_blocks_unexpected_syscall`) all return EPERM/error; positive tests confirm shell/file/HTTP/python-exec tools work within their allowlists. On macOS the crate compiles to a documented dev-only stub.
  3. Operator runs `rollout eval --suite mmlu --checkpoint <snapshot_id>` and gets a per-task score; bundled 10-row fixtures under `crates/rollout-harness-eval/tests/fixtures/` make the always-on `eval_score_matches_lm_eval_harness` test deterministic with no HF network call (HF_OFFLINE=1 default).
  4. `cargo test --workspace --tests` stays green with 5 new crates in the workspace (rename: `rollout-evals` → `rollout-harness-eval`; new: `rollout-cloud-aws`, `rollout-cloud-gcp`, `rollout-harness-text`, `rollout-harness-tool` — the cloud crates land in Phase 5 but the workspace count check happens here); dep-direction lint reaches 14 invariants total.

**Plans**: TBD
**UI hint**: no

### Progress

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 5. Cloud layer + object-store snapshots | 0/0 | Not started | - |
| 6. Multi-node distribution | 0/0 | Not started | - |
| 7. Harnesses (env + tool + eval) | 0/0 | Not started | - |

---

## Milestone v1.0 (archived — preserved for reference)

The full v1.0 phase detail (Phases 1–4 shipped 2026-05-27) lives in the root narrative roadmap at [`../ROADMAP.md`](../ROADMAP.md) and the archive at [`milestones/v1.0-ROADMAP.md`](milestones/v1.0-ROADMAP.md). Summary above.
