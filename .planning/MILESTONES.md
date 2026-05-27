# Milestones

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
