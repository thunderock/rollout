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

## Milestone: v1.1 — cloud + distribution + harnesses

**Shipped:** 2026-06-01
**Phases:** 3 (5-7) | **Plans:** 19
**Commits:** 100 | **Timeline:** 5 days (2026-05-28 → 2026-06-01)

### What Was Built

Real AWS + GCP cloud adapters (`rollout-cloud-aws`/`-gcp`) over the v1.0 trait surface with streaming snapshots and `rollout cloud doctor`; a pull-based multi-node coordinator with dual-backed CAS lease + epoch fencing, work-stealing, storage-backed stateless-replayer restart, and spot-drain; and three algo-layer harness crates (text env, sandboxed tool harness, bundled-eval harness) over the full spec-07 batched trait surface, plus the top-level `rollout eval` CLI. 12/12 in-scope requirements; dep-direction lint at 14 invariants; 5 new crates.

### What Worked

- **Goal-backward verification per phase** caught real gaps before milestone audit — e.g. the offline-default eval defect surfaced in spot-checks because the success criterion was the *bare* `cargo test --workspace`, not an env-gated invocation.
- **Mock-backend + offline-fixture discipline** kept every load-bearing witness GPU-free / network-free / Docker-free, so CI stays fast and the milestone is reproducible on a laptop.
- **Layered-defense sandbox** designed from a curated seccomp allowlist + fail-closed kernel gate rather than ad-hoc blocking; the macOS dev-stub split let the workspace stay green on dev machines while enforcement validates on a dedicated Linux CI lane.

### What Was Inefficient

- **gsd-tools resolves project root to the primary worktree**, so running the full plan→execute→audit→complete chain inside a background-session git worktree split `.planning` writes across two trees; phase-completion bookkeeping had to be reconciled by hand and the branch fast-forwarded at the end.
- **ROADMAP plan-counter aggregation under-reported** (`summary_count: 0`) for completed phases, so progress tables needed manual correction.
- One executor hit a mid-plan API socket drop (07-02); a fresh continuation executor resumed cleanly from the last commit — but only because tasks were committed atomically.

### Patterns Established

- **Eval-as-WorkQueue-job** (one example = one queue item over the Phase-6 CAS machine) — never call `evaluate()` synchronously in an inner loop.
- **Offline-by-default datasets** with SHA-pinned fixtures (`HF_OFFLINE` default true; opt online with `HF_OFFLINE=0`) so the parity witness is always-on.
- **Honest threat-model docs** — sandbox-depth matrix states "process-isolated, NOT VM-isolated / not a security perimeter" rather than overclaiming.

### Key Lessons

- Make the always-on witness match the *exact* command the gate runs (bare `cargo test --workspace`), or env-coupled tests pass locally and fail in CI.
- When isolating bg-session work in a worktree, treat the branch as authoritative and reconcile/merge at the end — don't trust tools that resolve to the primary checkout.
- Deferring speculative surface (HarnessGraph, eval gate, trajectory persistence) to v1.2 kept the harness contract clean for its real consumers.

### Cost Observations

- **Model mix:** opus for planning + executors + verifier-of-record; sonnet for plan-checker + integration-checker.
- **Notable:** Phase 7's tool sandbox (07-02/07-04) was the most expensive — layered Linux sandbox + SSRF connector + CVE matrix, with a mid-run resume.

---

## Cross-Milestone Trends

### Process Evolution

| Milestone | Sessions | Phases | Key Change |
|-----------|----------|--------|------------|
| v1.0 | ~12–15 | 4 | First milestone — established the GSD phase / plan / SUMMARY / VERIFICATION / VALIDATION cadence; introduced standing rules in AGENTS.md §9; introduced per-milestone archive snapshots while keeping a living `REQUIREMENTS.md`. |
| v1.1 | ~8–10 | 3 | First milestone executed largely autonomously (plan→execute→audit→complete chain) inside an isolated git worktree; surfaced the gsd-tools-resolves-to-primary-worktree friction; established eval-as-job + offline-default-fixture + layered-sandbox patterns. |

### Cumulative Quality

| Milestone | Tests | LOC (Rust) | Crates |
|-----------|-------|------------|--------|
| v1.0 | ~200 (default-CI) + ignored live-env suite | 17,901 | 13 |
| v1.1 | ~310 (default-CI, GPU/Docker-free) + Linux sandbox + cloud-emulator lanes | 34,951 | 18 |

### Top Lessons (Verified Across Milestones)

- **Witnesses must match the gate command exactly.** Env-coupled tests (e.g. `HF_OFFLINE=1`-only) pass locally but fail the bare `cargo test --workspace` CI gate — default the behavior, don't require the env.
- **Atomic per-task commits make mid-run failures cheap.** Both milestones recovered from interruptions (v1.0 hot-reload, v1.1 socket drop) by resuming from the last commit with a fresh agent.
- **Defer speculative surface.** Both milestones kept contracts clean by pushing not-yet-consumed features (quic transport, HarnessGraph, eval gate) behind explicit later-milestone markers rather than building them ahead of a consumer.
