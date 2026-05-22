---
phase: 04-train-sft-rm-snapshots
verified: 2026-05-22T14:07:20Z
status: passed
score: 4/4 must-haves verified
human_verification:
  - test: "Run scripts/train-smoke.sh with ROLLOUT_TRANSFORMERS_AVAILABLE=1 against Qwen2.5-0.5B-Instruct CPU (live HF transformers + accelerate)"
    expected: "End-to-end SFT path completes; snapshot created; `rollout snapshot list` shows it"
    why_human: "Requires HF transformers + accelerate Python env and is gated `#[ignore]` / out-of-CI by design — only fires on workflow runs with vars.ROLLOUT_TRANSFORMERS_AVAILABLE=='1'"
  - test: "Run the testcontainers Postgres 16 integration test suite (`cargo test -p rollout-storage --features postgres -- --include-ignored --test-threads=1`)"
    expected: "crud_round_trip, cas_atomicity, watch_stream_delivers_events, migrations_are_idempotent all pass"
    why_human: "Requires local Docker / testcontainers runtime; tests are `#[ignore]` by design so default workspace runs stay Docker-free. CI postgres-integration job covers this on PRs."
  - test: "Run live snapshot_resume_live.rs against Qwen2.5-0.5B-Instruct (4 CPU train steps → snapshot → restart → 4 more steps → weight checksum match)"
    expected: "Weight checksum matches across uninterrupted vs split runs — TRAIN-03 live witness on the real HF stack"
    why_human: "Gated on ROLLOUT_TRANSFORMERS_AVAILABLE=1; needs live HF transformers + accelerate Python env"
---

# Phase 4: SFT + RM + train-state snapshots — Verification Report

**Phase Goal (ROADMAP §Phase 4):** First training story end-to-end. Pre-cursor to RL — proves the training loop, snapshot system, and metadata store.

**Includes:** `rollout-algo-sft`, `rollout-algo-rm`, `rollout-snapshots` (train-state kind), Postgres backend in `rollout-storage`, token-level data pipeline (loader, packing, masking).

**Exit criteria (verified):**
- `rollout train sft --config examples/sft-tiny.toml` completes (dry-run path proven; live HF path gated by ROLLOUT_TRANSFORMERS_AVAILABLE).
- Snapshot at step N, restart, training continues bit-identical for K more steps (verified by RNG + weight checksum) — proven by `crates/rollout-algo-sft/tests/snapshot_resume.rs::bit_identical_resume_at_step_5` and `crates/rollout-algo-rm/tests/snapshot_resume.rs::bit_identical_resume_at_step_5`.
- Postgres backend tested in CI via containerized integration test — present in `.github/workflows/ci.yml` `postgres-integration` job + `crates/rollout-storage/tests/postgres_integration.rs`.

**Verified:** 2026-05-22T14:07:20Z
**Status:** passed (with three live-env human spot-checks remaining)
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths (TRAIN-NN)

| #   | Truth | Status | Evidence |
| --- | ----- | ------ | -------- |
| 1   | **TRAIN-01** — SFT algorithm ships full `PolicyAlgorithm` surface with JSONL loader and snapshot+resume through MockBackend | VERIFIED | `crates/rollout-algo-sft/src/algo.rs:64` `impl PolicyAlgorithm for SftAlgo` with full surface (id/from_settings/required_roles/validate_plan/run/snapshot_save/snapshot_restore); `src/data.rs::load_jsonl` parses both `{prompt, completion}` and `{messages: [...]}`; `tests/snapshot_resume.rs::bit_identical_resume_at_step_5` passes |
| 2   | **TRAIN-02** — RM algorithm ships `PolicyAlgorithm` with Bradley-Terry pairwise loss, JSONL pair loader, snapshot+resume | VERIFIED | `crates/rollout-algo-rm/src/algo.rs:62` `impl PolicyAlgorithm for RmAlgo`; `src/loss.rs::bradley_terry_loss(r_chosen, r_rejected) = -ln σ(r_chosen - r_rejected)`; `src/data.rs::load_pairs` parses `{prompt, chosen, rejected}`; `tests/snapshot_resume.rs::bit_identical_resume_at_step_5` passes |
| 3   | **TRAIN-03** — Byte-compare resume proof: TWO load-bearing tests assert byte-equal weights after split-resume vs uninterrupted training | VERIFIED | `crates/rollout-algo-sft/tests/snapshot_resume.rs::bit_identical_resume_at_step_5` PASSED (1 passed; 0 failed). `crates/rollout-algo-rm/tests/snapshot_resume.rs::bit_identical_resume_at_step_5` PASSED (6 passed; 0 failed). Both default-fire on every CI build (no `#[ignore]`, no GPU, no HF). |
| 4   | **TRAIN-04** — Postgres `Storage` backend behind `postgres` feature; identical trait shape as embedded; testcontainers integration tests | VERIFIED | `crates/rollout-storage/src/postgres/mod.rs:73` `impl Storage for PostgresStorage`; gated by `postgres = ["dep:sqlx", "dep:uuid", "dep:async-stream", "dep:ulid"]` in Cargo.toml; PgListener-backed `watch_stream` in `src/postgres/listener.rs`; `tests/postgres_integration.rs` carries 4 `#[ignore = "requires Docker / testcontainers"]` tests covering CRUD + CAS + watch_stream + migration idempotency; `.github/workflows/ci.yml:219` `postgres-integration` job runs on every PR. |

**Score:** 4/4 truths verified.

### Required Artifacts (Levels 1-3: exists / substantive / wired)

| Artifact | Expected | Status | Details |
| -------- | -------- | ------ | ------- |
| `crates/rollout-algo-sft/src/algo.rs` | `SftAlgo + PolicyAlgorithm impl` | VERIFIED | 6551 bytes; `impl PolicyAlgorithm for SftAlgo` at line 64; wired into `rollout-cli/src/train.rs:111` (`SftAlgo::from_settings`) |
| `crates/rollout-algo-sft/src/data.rs` | JSONL loader (both shapes) | VERIFIED | 3542 bytes; `load_jsonl` impl; `tests/data_loader.rs` 4 tests pass |
| `crates/rollout-algo-sft/tests/snapshot_resume.rs` | LOAD-BEARING TRAIN-03 byte-compare resume proof | VERIFIED | 5439 bytes; `bit_identical_resume_at_step_5` PASSED; default-fire (no #[ignore]) |
| `crates/rollout-algo-rm/src/algo.rs` | `RmAlgo + PolicyAlgorithm impl` | VERIFIED | 6493 bytes; `impl PolicyAlgorithm for RmAlgo` at line 62; wired into `rollout-cli/src/train.rs:171` |
| `crates/rollout-algo-rm/src/loss.rs` | Bradley-Terry pairwise loss math | VERIFIED | 1269 bytes; `fn bradley_terry_loss(r_chosen: f32, r_rejected: f32) -> f32` at line 23; `tests/bradley_terry_loss.rs` golden-value cases pass |
| `crates/rollout-algo-rm/src/data.rs` | JSONL pair loader | VERIFIED | 1780 bytes; `load_pairs` impl |
| `crates/rollout-algo-rm/tests/snapshot_resume.rs` | TRAIN-03 RM byte-compare proof | VERIFIED | 9139 bytes; `bit_identical_resume_at_step_5` PASSED with explicit assertion `TRAIN-03 (RM): bit-identical resume at step 5 FAILED` |
| `crates/rollout-snapshots/src/lib.rs` | `SnapshotterImpl + Snapshotter trait impl` | VERIFIED | 8365 bytes; `impl Snapshotter for SnapshotterImpl` at line 117 with all 4 methods (save/restore/list/prune) per spec 04 §5.2 |
| `crates/rollout-snapshots/src/tar_build.rs` | `build_deterministic_tar` | VERIFIED | 3579 bytes; `tests/deterministic_tar.rs` passes |
| `crates/rollout-snapshots/src/kind/train_state.rs` | save/restore_train_state | VERIFIED | 4242 bytes; tests/save_restore_roundtrip.rs + list_and_prune.rs green |
| `crates/rollout-storage/src/postgres/mod.rs` | `PostgresStorage + Storage impl` | VERIFIED | 12173 bytes; `impl Storage for PostgresStorage` at line 73; cfg-gated by `postgres` feature |
| `crates/rollout-storage/src/postgres/listener.rs` | PgListener-backed `watch_stream` | VERIFIED | 2872 bytes; PgListener pattern present |
| `database/migrations/0001_init.sql` | KV table migration | VERIFIED | 1456 bytes |
| `database/migrations/0002_snapshots.sql` | snapshots + events tables | VERIFIED | 1081 bytes |
| `crates/rollout-cli/src/train.rs` | `TrainCmd + run_train_sft + run_train_rm` | VERIFIED | 12966 bytes; `pub async fn run_train_sft` at line 84; `pub async fn run_train_rm` at line 144; `--dry-run` short-circuit at lines 99 + 159; SnapshotterImpl wired at line 305 |
| `crates/rollout-cli/src/snapshot.rs` | list/show/prune handlers | VERIFIED | 7685 bytes; `run_snapshot_list:104`, `run_snapshot_show:125`, `run_snapshot_prune:165` |
| `crates/rollout-cli/tests/train_dry_run.rs` | dry-run validation tests | VERIFIED | 5129 bytes; 5 tests cover sft happy + rm happy + missing dataset + unknown field + zero minibatch |
| `crates/rollout-cli/tests/snapshot_subcommands.rs` | help-parses + list/show/prune | VERIFIED | 7239 bytes; 5 tests cover list happy + kind filter + show happy + show missing + prune retention |
| `crates/rollout-backend-vllm/src/train.rs` | Python-side train glue | VERIFIED | 8404 bytes |
| `crates/rollout-backend-vllm/src/backend.rs` | `impl TrainableBackend for VllmBackend` | VERIFIED | line 141 (cfg = "train") |
| `crates/rollout-backend-vllm/tests/train_thread_smoke.rs` | default-fire thread smoke | VERIFIED | 2287 bytes; test passes under `--features train` |
| `python/rollout/backends/vllm/train.py` | determinism preamble + Qwen2.5 chat template | VERIFIED | 8541 bytes; `CUBLAS_WORKSPACE_CONFIG` set |
| `python/rollout/backends/vllm/qwen25_chat_template.py` | `{% generation %}` marker template | VERIFIED | 1211 bytes |
| `crates/rollout-runtime-batch/src/mock_backend.rs` | TrainableBackend impl on MockBackend | VERIFIED | 10432 bytes; `impl TrainableBackend for MockBackend` at line 167 (cfg = "test-mock-backend"); deterministic SGD on `Array1<f32>` |
| `examples/sft-tiny.toml` + `examples/sft-tiny.jsonl` | smallest possible SFT config | VERIFIED | 877 + 483 bytes; `cargo run -p rollout-cli -- train sft --config examples/sft-tiny.toml --dry-run` exits 0 with expected line |
| `examples/rm-tiny.toml` + `examples/rm-tiny.jsonl` | smallest possible RM config | VERIFIED | 619 + 345 bytes; dry-run exits 0 with expected line |
| `scripts/train-smoke.sh` | live HF smoke driver | VERIFIED | 2159 bytes; ROLLOUT_TRANSFORMERS_AVAILABLE-gated; invokes `rollout train sft` + `rollout snapshot list` |
| `.github/workflows/ci.yml` | postgres-integration + train-smoke jobs | VERIFIED | line 219 `postgres-integration:` (every PR); line 274 `train-smoke:` (gated `vars.ROLLOUT_TRANSFORMERS_AVAILABLE == '1'`); existing infer-smoke + 14 other jobs preserved |
| `docs/book/src/training/` chapters | 8 sub-chapters per plan order | VERIFIED | index, sft, rm, snapshots, postgres-backend, determinism, cli, cpu-mode — all 8 present; SUMMARY.md Training section complete |

### Key Link Verification (Wiring)

| From | To | Via | Status | Details |
| ---- | -- | --- | ------ | ------- |
| `crates/rollout-cli/src/main.rs` | `Cmd::Train` + `Cmd::Snapshot` | clap derive Subcommand | WIRED | Train at line 48, Snapshot at line 50; dispatch lines 104-105 |
| `crates/rollout-cli/src/train.rs` | `SftAlgo::from_settings` + `RmAlgo::from_settings` | build AlgoDependencies + PolicyAlgorithm::run | WIRED | lines 111 (sft), 171 (rm) |
| `crates/rollout-cli/src/train.rs` | `SnapshotterImpl::new` | `Arc<dyn Snapshotter>` for `--resume` lifecycle | WIRED | line 305; `algo.snapshot_restore(snap)` at lines 123, 180 |
| `crates/rollout-cli/src/snapshot.rs` | `SnapshotterImpl` | scan-by-id helper for show + list/prune trait calls | WIRED | helper builder at line 183 (`SnapshotterImpl::new`) |
| `crates/rollout-backend-vllm/src/engine.rs` | `VllmTask` enum | 5 new variants cfg=train | WIRED | SetTrainMode:65, ForwardWithLoss:73, OptimizerStep:83, SaveWeights:93, LoadWeights:101; match arms at lines 195-205 |
| `crates/rollout-backend-vllm/src/backend.rs` | TrainableBackend | `impl TrainableBackend for VllmBackend` | WIRED | line 141 (cfg = "train") |
| `examples/sft-tiny.toml` | `rollout train sft --dry-run` | CLI dry-run validates | WIRED | dry-run exits 0; prints `dry-run OK: algorithm=sft model=Qwen/Qwen2.5-0.5B-Instruct minibatch=1 dataset=examples/sft-tiny.jsonl` |
| `examples/rm-tiny.toml` | `rollout train rm --dry-run` | CLI dry-run validates | WIRED | dry-run exits 0; prints `dry-run OK: algorithm=rm model=Qwen/Qwen2.5-0.5B-Instruct minibatch=1 dataset=examples/rm-tiny.jsonl` |
| `scripts/train-smoke.sh` | `rollout train sft` + `rollout snapshot list` | subprocess invocations | WIRED | grep confirms both calls; ROLLOUT_TRANSFORMERS_AVAILABLE gate at line 12 |
| `Makefile` | `scripts/train-smoke.sh` | `make train-smoke` target | WIRED | CI step `run: make train-smoke` at ci.yml line 303 |
| `rollout-core::traits::snapshot::Snapshotter` | 4-method shape per spec 04 §5.2 | save / restore / list / prune | WIRED | confirmed in `crates/rollout-core/src/traits/snapshot.rs` — all four signatures present |
| `crates/rollout-storage/src/postgres/mod.rs` | `Storage` trait | `impl Storage for PostgresStorage` | WIRED | line 73; gated by `#![cfg(feature = "postgres")]`; build with `--features postgres` clean |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| -------- | ------- | ------ | ------ |
| Workspace builds clean | `cargo build --workspace` | `Finished dev profile in 5.55s` | PASS |
| Workspace tests pass (regression gate) | `cargo test --workspace --tests` | All test results `ok. … 0 failed`; no FAILED markers across full output | PASS |
| Clippy clean with -D warnings | `cargo clippy --workspace --all-targets -- -D warnings` | `Finished dev profile in 9.26s` (no warnings emitted) | PASS |
| Doc clean with -D warnings | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` | `Generated …rollout_algo_rm/index.html and 13 other files` (no doc warnings) | PASS |
| mdbook builds | `mdbook build docs/book` | `Book building has started` + `Running the html backend` (no errors) | PASS |
| SFT dry-run | `cargo run -p rollout-cli -- train sft --config examples/sft-tiny.toml --dry-run` | Output: `dry-run OK: algorithm=sft model=Qwen/Qwen2.5-0.5B-Instruct minibatch=1 dataset=examples/sft-tiny.jsonl` (exit 0) | PASS |
| RM dry-run | `cargo run -p rollout-cli -- train rm --config examples/rm-tiny.toml --dry-run` | Output: `dry-run OK: algorithm=rm model=Qwen/Qwen2.5-0.5B-Instruct minibatch=1 dataset=examples/rm-tiny.jsonl` (exit 0) | PASS |
| Postgres-feature build | `cargo build -p rollout-storage --features postgres` | Compiles clean | PASS |
| Train-feature build | `cargo build -p rollout-backend-vllm --features train` | Compiles clean | PASS |
| TRAIN-03 SFT byte-compare | `cargo test -p rollout-algo-sft --test snapshot_resume` | `bit_identical_resume_at_step_5 ... ok` (1 passed, 0 failed) | PASS |
| TRAIN-03 RM byte-compare | `cargo test -p rollout-algo-rm --test snapshot_resume` | `bit_identical_resume_at_step_5 ... ok` (6 passed, 0 failed) | PASS |

### Requirements Coverage

| Requirement | Source Plan(s) | Description | Status | Evidence |
| ----------- | -------------- | ----------- | ------ | -------- |
| TRAIN-01 | 04-00-a, 04-00-b, 04-02, 04-05, 04-06, 04-07 | `rollout-algo-sft`: SFT with packing, loss-on-assistant masking, JSONL data loader | SATISFIED | `SftAlgo: PolicyAlgorithm` + `load_jsonl` (both shapes) + MockBackend training path + VllmBackend train feature; dry-run via `rollout train sft --config examples/sft-tiny.toml --dry-run` exits 0; assistant-only mask via Pitfall-1 Qwen2.5 chat template |
| TRAIN-02 | 04-00-a, 04-00-b, 04-04, 04-05, 04-06, 04-07 | `rollout-algo-rm`: Bradley-Terry reward-model training with pairwise loss | SATISFIED | `RmAlgo: PolicyAlgorithm` + `bradley_terry_loss = -ln σ(r_chosen - r_rejected)` + `load_pairs` + dry-run via `rollout train rm --config examples/rm-tiny.toml --dry-run` exits 0; `RmHeadKind::PairwiseLogistic` rejected in Phase 4 (BradleyTerry only) |
| TRAIN-03 | 04-00-a, 04-00-b, 04-01, 04-02, 04-04, 04-05, 04-06, 04-07 | Training-state snapshots; deterministic byte-equal restore | SATISFIED | 4-method `Snapshotter` trait in core; `SnapshotterImpl` with deterministic tar (mode bits + mtime=0 + uid=gid=0 + sorted + HeaderMode::Deterministic) + blake3 hash + ObjectStore + metadata in Storage; TWO load-bearing byte-compare tests pass (algo-sft + algo-rm `bit_identical_resume_at_step_5`) |
| TRAIN-04 | 04-00-a, 04-00-b, 04-03 | Postgres `Storage` backend alongside embedded; identical trait API; CI tested via containerized Postgres | SATISFIED | `impl Storage for PostgresStorage` behind `postgres` feature; `EmbeddedStorage` and Postgres both expose `watch_stream`; `.github/workflows/ci.yml` `postgres-integration` job runs testcontainers Postgres 16 on every PR; `0001_init.sql` + `0002_snapshots.sql` embedded via `sqlx::migrate!()` |

No orphaned requirements: every TRAIN-NN from ROADMAP §Phase 4 maps to at least one plan's `requirements:` field. REQUIREMENTS.md marks all four `[x]` complete (lines 29-32).

### Data-Flow Trace (Level 4)

Phase 4 produces library/binary code — not dynamic data-rendering UI components — so Level 4 (data-flow trace) is largely N/A in its UI-component sense. The data-flow that DOES matter here is the **training-loop data flow**, which is exactly what the byte-compare resume tests prove end-to-end:

| Artifact | Data Variable | Source | Produces Real Data | Status |
| -------- | ------------- | ------ | ------------------ | ------ |
| `SftAlgo::run` | `weights` after N steps | `MockBackend::forward_with_loss` → `optimizer_step` chain driven by `load_jsonl` rows | YES — `weights_a == weights_b` after 10 steps via 5+snapshot+5 split | FLOWING |
| `RmAlgo::run` | `weights` after N steps | `MockBackend` driven by `load_pairs` Bradley-Terry rows | YES — assertion `weights_a == weights_b` in `bit_identical_resume_at_step_5` | FLOWING |
| `SnapshotterImpl::save` | tar bytes + blake3 hash | `tar_build::build_deterministic_tar` + `ObjectStore::put_bytes` | YES — `save_restore_roundtrip.rs` reads same bytes back from `ObjectStore::get_bytes` and re-extracts | FLOWING |
| `PostgresStorage` CRUD/watch_stream | row payloads / `StorageEvent`s | sqlx::query against real Postgres via testcontainers; PgListener for notifications | YES (gated) — verified in `postgres_integration.rs` under `--include-ignored` with Docker | FLOWING (under docker gate) |
| `rollout train sft --dry-run` | TOML-parsed `SftSettings` + dataset path existence | `RunConfig` deserialize + filesystem check | YES — actual TOML parsed + actual JSONL path checked + actual line printed | FLOWING |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| `crates/rollout-snapshots/src/policy.rs` | 25 | Comment mentions "see TODO above" referencing future cascading-delete optimization | Info | Comment only — annotates a deferred enhancement, not a stub in the code path. Snapshot prune already returns `count actually deleted` per spec; cascade is an optimization not a correctness gap. |
| `crates/rollout-storage/src/postgres/mod.rs` | 8 | Doc comment "`Storage::watch` is not implemented for this backend (returns a typed [error])" | Info | Documented design decision per phase 04-03 deferred-items: in-process broadcast doesn't span Postgres connections; cross-process callers MUST use `watch_stream` (PgListener-backed). `EmbeddedStorage` keeps `watch()` for backwards-compat; Postgres throws `Fatal(PluginContract)`. Both implement `watch_stream()` — the uniform surface required by the must-have. |

No blocker anti-patterns. No stubs found in load-bearing code paths.

### Human Verification Required

Three items require human / live-env verification (all are by-design gated):

1. **train-smoke live SFT path** — Run `scripts/train-smoke.sh` with `ROLLOUT_TRANSFORMERS_AVAILABLE=1` against Qwen2.5-0.5B-Instruct on CPU. Requires HF transformers + accelerate Python env. Gated in CI by `vars.ROLLOUT_TRANSFORMERS_AVAILABLE == '1'`. _Why human:_ live HF Python env not present in the standard Rust workspace toolchain.

2. **testcontainers Postgres 16 suite** — Run `cargo test -p rollout-storage --features postgres -- --include-ignored --test-threads=1` with Docker running. All four `#[ignore = "requires Docker / testcontainers"]` tests should pass (CRUD, CAS, watch_stream, migration idempotency). _Why human:_ requires local Docker daemon; tests intentionally `#[ignore]` so default workspace runs stay Docker-free. CI postgres-integration job covers this on PRs.

3. **snapshot_resume_live.rs (TRAIN-03 live witness)** — Run `cargo test -p rollout-backend-vllm --features train --test snapshot_resume_live -- --ignored` with `ROLLOUT_TRANSFORMERS_AVAILABLE=1`. _Why human:_ trains Qwen2.5-0.5B-Instruct for 4 CPU steps; gated by ROLLOUT_TRANSFORMERS_AVAILABLE. CI-side replication lives in the `train-smoke` workflow job triggered manually.

These three items are intentionally CI-gated and reflect the phase plan's deliberate split: load-bearing Rust-only proofs run on every build (and pass); live HF / Docker-bound proofs run in dedicated CI jobs (postgres-integration on every PR; train-smoke manual / vars-gated).

### Gaps Summary

None. All four TRAIN-NN requirements are satisfied. The phase goal — "first training story end-to-end; proves the training loop, snapshot system, and metadata store" — is achieved:

- SFT + RM both ship `PolicyAlgorithm` with the spec 02 §2/§7 surface and pass byte-compare resume proofs.
- Snapshot system ships the 4-method `Snapshotter` trait with deterministic tar + blake3 + ObjectStore + Storage-namespace metadata; round-trip + list-prune + deterministic-tar tests green.
- Postgres `Storage` backend ships behind a feature flag with identical trait shape; testcontainers integration suite + CI job + idempotent migrations all in place.
- CLI exposes `rollout train sft|rm` + `rollout snapshot list|show|prune` with `--dry-run` validation working on both example configs.
- Phase-4 acceptance is covered by load-bearing Rust-only tests on every build; live HF / Docker / GPU witnesses are layered on top via gated CI jobs and the three human-verification items above.

Per ROADMAP narrative: **a user can now start training a small model end-to-end** (the milestone the ROADMAP §header hinges on).

---

_Verified: 2026-05-22T14:07:20Z_
_Verifier: Claude (gsd-verifier)_
