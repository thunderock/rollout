# Milestones

## v1.1 — cloud + distribution + harnesses (Shipped: 2026-06-01)

**Phases completed:** 3 phases (5-7), 19 plans

**Key accomplishments:**

- **Cloud layer (Phase 5)** — `rollout-cloud-aws` (S3 multipart + MultipartGuard Drop-abort + blake3-before-send · SQS lease queue · read-only Secrets Manager · IMDSv2 ComputeHint) and `rollout-cloud-gcp` (GCS resumable upload · Pub/Sub lease · Secret Manager v1 REST · GCE-MDS), both behind default-off feature flags with zero SDK types leaking into `rollout-core`, gated by always-on localstack / fake-gcs-server + pubsub-emulator CI jobs. Streaming `ObjectStore::put_stream`/`get_stream` + `Queue::dequeue_with_lease`/`extend_lease` trait extensions. Byte-identical SFT/RM resume over cloud storage (`bit_identical_resume_at_step_5_via_{s3,gcs}` + cross-provider portability). `rollout cloud doctor --provider <aws|gcp>` 7-check pre-flight. Closed the v1.0 latent Postgres `scan_bytes` divergence with a 256-case parity proptest.
- **Multi-node distribution (Phase 6)** — Pull-based coordinator with state in `Storage`: dual-backed single-row CAS `StorageLease` with monotonic epoch (exactly-one-coordinator + stale-epoch rejection); coordinator-mediated work-stealing (`ceil(n/2)`, `MAX_STEAL_BATCH=32`) on a `WorkItemRecord` CAS state machine; stateless-replayer restart (`replay_and_serve` — adopt epoch, reconstruct in-flight without re-executing); spot-preemption graceful drain (notice lead 120s/30s vs conservative drain deadline 60s/15s); split-brain fencing (`std::process::abort` within 5s). Witnesses `coord_restart_no_duplicates`, `spot_drain_completes_within_lead_time`, `split_brain_old_coord_self_fences`, `concurrent_ack_and_steal_no_double_execute` — all Docker-free, plus `make smoke-3node-{aws,gcp}` and a Postgres CAS-lease CI lane.
- **Harnesses (Phase 7)** — Three algo-layer crates over the full spec-07 batched trait surface (`EnvHarness`/`ToolHarness`/`EvalHarness` + `HarnessDependencies`): `rollout-harness-text` (multi-turn text-completion env, plugin-host reward path, `env_deterministic_replay`); `rollout-harness-tool` (layered Linux sandbox — rustix namespaces + landlock kernel≥5.13 fail-closed + curated seccompiler allowlist + cap-std + cgroups v2 — with six tools incl. SSRF-hardened HTTP, macOS dev stub, and the full CVE-class negative matrix on the `harness-linux` CI lane); `rollout-harness-eval` (pure-Rust MMLU/IFEval/GSM8K scorers, SHA-pinned offline fixtures, eval-as-WorkQueue-job, `eval_score_matches_lm_eval_harness` ≤1% parity). Top-level `rollout eval` CLI; dep-direction lint at 14 invariants; 5 new crates in the workspace.

**Stats:** 3 phases · 19 plans · 100 commits · 270 files (+40,585 / −239) · 5-day milestone (2026-05-28 → 2026-06-01)

**Audit:** `.planning/milestones/v1.1-MILESTONE-AUDIT.md` — status `tech_debt`: 12/12 requirements satisfied, 3/3 phases verified, 6/6 cross-phase integration seams wired, 2/2 E2E flows complete. No blockers. HARNESS-04 (eval gate) deferred to v1.2.

**Tech debt carried into v1.2:** Nyquist validation never run on phases 6/7 (draft VALIDATION.md present); `--all-features` rustdoc upstream `h3-quinn` quic issue (CI uses no `--all-features`, green); `restart_no_duplicates.rs` clippy under `test-mock-backend` (plan 03-05 follow-up); plus the 9 by-design items carried from v1.0.

---

## v1.0 — substrate + train (Shipped: 2026-05-27)

**Phases completed:** 4 phases, 30 plans, 59 tasks

**Key accomplishments:**

- **Core foundations** — Cargo workspace bootstrapped (13 publishable crates + `xtask`) on a pinned `1.88.0` toolchain; `rollout-core` ships the full trait surface (19 traits), error taxonomy (`Recoverable` / `Fatal` + `RetryHint`), ID types (`RunId` / `WorkerId` / `ContentId`), and the single-source-of-truth `RunConfig` tree.
- **Schema-as-code pipeline** — `cargo xtask schema-gen` deterministically regenerates JSON Schema + Python Pydantic stubs + docs reference from the Rust types; `rollout schema --format json|pretty` prints the schema; CI `schema-drift` job + `check-jsonschema --check-metaschema` lock the contract.
- **Architecture enforcement** — `cargo deny` at workspace root with full license allowlist + openssl bans; `dependency_direction` integration test enforces 10 layered-architecture invariants via `cargo_metadata` plus deliberate-violation fixture crates (algorithm crates cannot reach cloud crates, etc.).
- **Local substrate** — `rollout-storage` (embedded redb 2.x + Postgres via sqlx 0.8) · `rollout-transport` (tonic 0.14 / HTTP-2 / rustls 0.23 / mTLS-by-default; `quic` feature gated EXPERIMENTAL) · `rollout-plugin-host` (Rust cdylib via libloading + PyO3 in-process via `pyo3-async-runtimes 0.28` dedicated-thread pattern + Python sidecar via stdlib JSON-over-UDS, with `dev-hot-reload` feature) · `rollout-cloud-local` (content-addressed sharded FS object store + RAM queue + env-var SecretStore + ComputeHint) · `rollout-coordinator` driving deadline-based heartbeats (500 ms / 4 s self-fence / 5 s coord-failure). `make smoke` boots 1 coordinator + 2 workers, loads both plugin modes, kills a worker, and verifies `worker_failed` within deadline.
- **Batch inference** — `rollout-backend-vllm` drives the live `vllm.AsyncLLMEngine` through `pyo3_async_runtimes::tokio::run_until_complete` (asyncio↔Tokio bridge with GIL released across `await`, smoke-tested without vLLM installed); `rollout-runtime-batch` owns the CAS sample-state machine with content-addressed `sample_id` (`SAMPLING_PARAMS_SCHEMA_VERSION` byte prefix) and resume scan; `rollout infer batch --config <toml> [--resume <id>] [--workers N] [--dry-run]` is wired end-to-end with a deterministic restart-no-duplicates test that runs on every CI build (no GPU).
- **Training** — `rollout-algo-sft` and `rollout-algo-rm` ship `PolicyAlgorithm` impls driven by `MockBackend`; the load-bearing **TRAIN-03 byte-identical resume proof** is green on two witnesses (SFT + RM `bit_identical_resume_at_step_5`). `rollout-snapshots::SnapshotterImpl` ships the 4-method `Snapshotter` (save/restore/list/prune) over a deterministic tar builder + content-addressed `ObjectStore::put_bytes`. Bradley-Terry pairwise loss uses numerically-stable `logsigmoid`. `VllmBackend` impls `TrainableBackend` end-to-end behind `--features train` with Pitfall-mitigated determinism preamble + Qwen2.5 chat-template override.
- **CLI surface** — `rollout train sft|rm --config <toml> [--resume <snapshot_id>] [--dry-run]` and `rollout snapshot {list,show,prune}` mount on `rollout-cli` with full clap-derive shape, plan-time TOML validation, three-tier `run_id` lifecycle, and backend selection across `vllm` / `train` / `test-mock-backend` Cargo features. `examples/{batch,sft,rm}-tiny.toml` ship as ready-to-run recipes.
- **Docs + brand** — mdBook scaffold at `docs/book/` covering Introduction, Architecture, Substrate, Inference, Training, Examples; deployed at <https://thunderock.github.io/rollout/>; rustdoc gated by CI; AGENTS.md §9 codified as standing rules; v1.0 brand system (logo + gradient wordmark + social card + theme-aware mdbook `custom.css`) landed as the post-Phase-4 polish pass.

**Stats:** 4 phases · 30 plans · 59 tasks · 112 commits · 386 files (+66,106 / -48) · 17,901 Rust + 741 Python LOC · 7-day milestone (2026-05-19 → 2026-05-26)

**Tech debt carried into v1.1:** see `.planning/milestones/v1.0-MILESTONE-AUDIT.md` `tech_debt:` block. 9 items, all by-design human-gate or live-env (GPU/HF/Docker) plus 1 latent Postgres `scan_bytes` wildcard divergence flagged for RL-01 (Phase 9). Nyquist validation: 1/4 phases compliant (02/03/04 have draft VALIDATION.md but were never run through `/gsd:validate-phase` — optional v1.1 cleanup).

**Post-Phase-4 polish (out-of-phase, on main):** v1.0 brand system (logo, gradient wordmark, social card, theme-aware mdbook custom.css, README + docs-landing logo) shipped as `dfb5d19..2d8ab2f`. CI red on main was resolved across 5 atomic commits + `.nojekyll` Pages bypass (`c8dc8b9..b916ae3`). All recorded in `.planning/debug/ci-main-red.md`.

---
