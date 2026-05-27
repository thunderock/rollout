# Retrospective

*A living document updated after each milestone. Lessons feed forward into future planning.*

## Milestone: v1.0 — substrate + train

**Shipped:** 2026-05-27
**Phases:** 4 | **Plans:** 30 | **Tasks:** 59
**Commits:** 112 | **Timeline:** 7 days (2026-05-19 → 2026-05-26)

### What Was Built

- **Core foundations** — `rollout-core` 19-trait surface, error taxonomy (`Recoverable` / `Fatal` + `RetryHint`), ID types (`RunId` / `WorkerId` / `ContentId`), `RunConfig` schema-as-code; 13-crate Cargo workspace pinned at `1.88.0`.
- **Schema-as-code pipeline** — `cargo xtask schema-gen` deterministically regenerates JSON Schema + Python Pydantic stubs + docs reference; `schema-drift` CI job + `check-jsonschema --check-metaschema` lock the contract.
- **Local substrate** — `rollout-storage` (embedded redb 2.x + Postgres sqlx 0.8); `rollout-transport` (tonic 0.14 / HTTP-2 / rustls 0.23 mTLS-by-default; `quic` feature EXPERIMENTAL); `rollout-plugin-host` (Rust cdylib + PyO3 in-process + Python sidecar, full hot-reload behind `dev-hot-reload`); `rollout-cloud-local`; `rollout-coordinator` with deadline-based heartbeats. `make smoke` end-to-end gate green.
- **Batch inference** — `rollout-backend-vllm` driving live `vllm.AsyncLLMEngine` over a PyO3 asyncio↔Tokio bridge; `rollout-runtime-batch` with CAS sample-state machine + `SAMPLING_PARAMS_SCHEMA_VERSION`-prefixed content-addressed sample IDs; `rollout infer batch --resume` end-to-end with deterministic restart-no-duplicates test on every CI build.
- **Training** — `rollout-algo-sft` + `rollout-algo-rm` (Bradley-Terry); load-bearing **TRAIN-03 byte-identical resume proof** green on two witnesses; `rollout-snapshots::SnapshotterImpl` 4-method shape (save/restore/list/prune) over deterministic tar + content-addressed object store; `VllmBackend: TrainableBackend` behind `--features train`.
- **CLI** — `rollout {infer batch | train sft|rm | snapshot list|show|prune | schema}` with full clap-derive shape, plan-time TOML validation, `--dry-run` short-circuit, three-tier `run_id` lifecycle.
- **Docs + brand** — mdBook at `docs/book/` deployed to <https://thunderock.github.io/rollout/>; rustdoc gated by CI; AGENTS.md §9 standing rules; v1.0 brand system (logo, gradient wordmark, social card, theme-aware mdbook `custom.css`) shipped as the post-Phase-4 polish pass.

### What Worked

- **Plan-time validation as a first-class principle.** Every CLI subcommand goes through `validate_plan` before any heavy resource is constructed; this caught most config bugs at `rollout <cmd> --dry-run` and made the failure mode for invalid TOML feel cheap.
- **TRAIN-03 byte-compare resume as a single mechanical witness.** Two `bit_identical_resume_at_step_5` tests (SFT + RM) run on every CI build without GPU or HF — the determinism contract has a single, no-GPU enforcer.
- **Architecture-lint via `cargo_metadata` + deliberate-violation fixture crates.** Layered architecture (algorithm crates ↛ cloud crates) is self-enforcing through 10 invariants and ~3 sibling fixture crates that *must* fail to compile if the lint passes them.
- **AGENTS.md §9 as standing rules.** A single in-repo file holds the cross-cutting commit/lint/doc discipline; both human and Claude contributors converge on the same gate set, so phase-to-phase drift stayed low.
- **mdBook + `cargo doc` + `check-jsonschema` cross-validation.** Three independent doc systems point at the same Rust types; drift between book chapters, rustdoc, and the generated JSON Schema would have been caught structurally.
- **`testcontainers`-gated Postgres integration tests.** Default `cargo test` stays Docker-free (`#[ignore]`'d) while CI still exercises the Postgres path on PRs via the `postgres-integration` job.

### What Was Inefficient

- **SUMMARY.md frontmatter hygiene gap.** Roughly half of the 30 plan SUMMARYs had empty or absent `one_liner:` and `requirements_completed:` arrays, so automated milestone-audit + accomplishment extraction had to fall back to grepping prose. The milestone-audit and this retrospective had to be hand-curated.
- **Late-discovered `tonic-h3 quic` incompatibility.** `--features quic` does not compile against current `quinn 0.11.x` because `h3-quinn 0.0.7` accesses private `quinn::StreamId.0`. Found post-implementation; would have been caught earlier by a `cargo check --features quic` job in CI from day 1.
- **Five CI jobs went red on `main` after Phase-4 close.** `lint`, `test`, `unused-deps`, `deny`, and `docs-build` drifted between local `make check` and the CI workflow once the brand-system commits landed; required a 5-commit fix pass and a `.nojekyll` defensive commit to stop GitHub Pages from auto-injecting a Jekyll workflow over our mdbook output. All recorded in `.planning/debug/ci-main-red.md`.
- **Brand work retrofitted as out-of-phase commits.** The v1.0 brand system (`dfb5d19..2d8ab2f`) shipped after the Phase-4 audit, so it sits as a tail of polish commits on `main` rather than inside a phase folder; it makes the milestone-audit→ship boundary fuzzier than it needs to be.
- **Nyquist VALIDATION.md authored but never run.** Phases 02/03/04 ship draft VALIDATION.md scaffolding but were never put through `/gsd:validate-phase`; the underlying tests + verifier coverage are already strong so this is optional cleanup, but the artifact-vs-action gap is real.

### Patterns Established

- **Phase SUMMARY.md frontmatter convention** — `status`, `one_liner`, `requirements_completed`, `tests_added` (still not uniformly populated; needs schema-drift-style enforcement).
- **VALIDATION.md per phase** — Nyquist sampling validator scaffolded but not yet wired into the close-out gate.
- **"Evolve PROJECT.md after each phase" git-commit cadence** — `docs(phase-NN): evolve PROJECT.md after phase completion` is now a recognized commit shape.
- **Milestone audit `tech_debt:` YAML block** — canonical pre-archive checklist; landed audit-side and carried through to MILESTONES.md.
- **`AlgorithmConfig` enum variant per algorithm + cascade through schema-gen** — adding an algorithm (SFT, RM) means a `Box<…Settings>` variant + a `cargo xtask schema-gen` re-run; pattern is locked.

### Key Lessons

1. **Add a `requirements_completed:` non-empty assertion to the `schema-drift` check** so future milestone audits don't have to fall back to grep. Empty frontmatter arrays are silent failures right now.
2. **Wire `make check` (full local gates) into a pre-push hook** to catch `fmt` / `clippy` / `machete` / `deny` drift before it hits CI. The 5-job-red incident on `main` was structurally preventable.
3. **Plan template tightening** — require `one_liner:` non-empty in SUMMARY.md frontmatter; require `requirements_completed:` to cite at least one REQ-ID for any plan in a phase whose VERIFICATION.md cites them.
4. **For multi-milestone projects, keep `REQUIREMENTS.md` as a single living spec.** The per-milestone `.planning/milestones/v1.0-REQUIREMENTS.md` is the snapshot at completion; the active spec stays in `.planning/REQUIREMENTS.md`. The CLI default of "delete REQUIREMENTS.md on archive" does not match a multi-milestone shape.
5. **Brand / polish work belongs in a tracked phase**, not retrofitted post-audit. Either a "Phase 0.5 polish" track ahead of the milestone or a dedicated `phase-04.5-brand` folder beats letting it sit as tail commits on `main`.
6. **Add `cargo check --features <experimental>` for every EXPERIMENTAL feature flag to CI from day 1.** The `quic` deferral could have been a planning input rather than a tech-debt entry.

### Cost Observations

- **Model mix:** mostly opus for planning + executing complex plans; sonnet for lighter agents (checker/verifier/integration-checker).
- **Sessions:** rough estimate from git activity — ~12–15 distinct working sessions across the 7 days.
- **Notable:** most expensive step was the Phase-04 plans (training algorithms + snapshots + Postgres + Train-mode VllmBackend) — each required a deep `RESEARCH.md` pass and the Pitfall mitigation set per plan.

---

## Cross-Milestone Trends

### Process Evolution

| Milestone | Sessions | Phases | Key Change |
|-----------|----------|--------|------------|
| v1.0 | ~12–15 | 4 | First milestone — established the GSD phase / plan / SUMMARY / VERIFICATION / VALIDATION cadence; introduced standing rules in AGENTS.md §9; introduced per-milestone archive snapshots while keeping a living `REQUIREMENTS.md`. |

### Cumulative Quality

| Milestone | Tests | LOC (Rust) | Crates |
|-----------|-------|------------|--------|
| v1.0 | ~200 (default-CI) + ignored live-env suite | 17,901 | 13 |

### Top Lessons (Verified Across Milestones)

*(populated after v1.1)*
