---
phase: 07-harnesses-env-tool-eval
plan: 05
subsystem: cli-eval-docs
tags: [rust, cli, clap, eval, mdbook, spec-reconcile, phase-gate, sc4]

# Dependency graph
requires:
  - phase: 07-harnesses-env-tool-eval
    plan: 03
    provides: "rollout-harness-eval BundledEval (EvalHarness::run), BundledEvalSettings/SuiteSetting, MockEvalBackend, offline fixtures, eval_reports"
  - phase: 07-harnesses-env-tool-eval
    plan: 01
    provides: "rollout-harness-text (env chapter source)"
  - phase: 07-harnesses-env-tool-eval
    plan: 04
    provides: "rollout-harness-tool README sandbox-depth matrix (tool-sandbox chapter source)"
provides:
  - "rollout eval top-level CLI subcommand (D-EVAL-02): --suite/--checkpoint/--config/--dry-run/--format + checkpoint→ModelRef resolution"
  - "spec 08 reconciled: rollout infer eval removed, top-level rollout eval added, Harness DAG footnoted v1.2"
  - "mdBook Harnesses chapter (index, env, tool-sandbox, eval, CLI)"
  - "SC4 phase-gate verification record (5 new crates green + 14 dep-direction invariants + no schema drift)"
affects: [phase-07-closeout, docs, cli]

# Tech tracking
tech-stack:
  added: []
  patterns: [cli-mirrors-infer-batch-clap-surface, checkpoint-to-modelref-resolution, dry-run-short-circuit-before-backend]

key-files:
  created:
    - crates/rollout-cli/src/eval.rs
    - crates/rollout-cli/tests/eval_cli.rs
    - docs/book/src/harnesses/index.md
    - docs/book/src/harnesses/env.md
    - docs/book/src/harnesses/tool-sandbox.md
    - docs/book/src/harnesses/eval.md
    - docs/book/src/harnesses/cli.md
  modified:
    - crates/rollout-cli/src/main.rs
    - crates/rollout-cli/Cargo.toml
    - docs/specs/08-cli.md
    - docs/book/src/SUMMARY.md

decisions:
  - "rollout eval is a top-level Cmd arm (sibling to Infer/Train/Snapshot/Cloud), NOT a subcommand of infer (D-EVAL-02)"
  - "--checkpoint resolves via Snapshot row (tar part content-id) when present, else treated as a direct content-id pin — supports dry-run without an on-disk snapshot"
  - "EvalContext built with temperature=0 (greedy/deterministic eval, D-EVAL-05); the bundled path always uses MockEvalBackend until the live full-split download lands"
  - "DOCS-03 phase-gate honored as CI enforces it (cargo doc --workspace --no-deps, no --all-features); --all-features pre-existing h3-quinn/quic failure deferred"

# Metrics
duration: 14min
completed: 2026-06-01
---

# Phase 7 Plan 05: Closeout — rollout eval CLI + spec-08 reconcile + mdBook + SC4 Summary

**Wired the top-level `rollout eval` CLI (D-EVAL-02) onto `rollout-cli` mirroring the `infer batch` clap surface (dry-run, feature-gated backend, checkpoint→ModelRef resolution) dispatching to `rollout-harness-eval`'s `BundledEval::run`; reconciled `docs/specs/08-cli.md` (removed `rollout infer eval`, added top-level `rollout eval`, footnoted the Harness DAG as v1.2); shipped the mdBook Harnesses chapter (env, tool-sandbox, eval, CLI); and verified the SC4 phase gate green — `cargo test --workspace --tests` with all 5 new crates present, the dep-direction lint at 14 invariants, no schema drift, and `cargo deny` clean.**

## Performance
- **Duration:** ~14 min
- **Tasks:** 2
- **Files created/modified:** 11 (7 created, 4 modified)

## Accomplishments

### Task 1 — `rollout eval` CLI + spec-08 reconciliation (TDD)
- `crates/rollout-cli/src/eval.rs`: `#[derive(Args)] EvalCmd` with `--suite <mmlu|ifeval|gsm8k>` (clap `ValueEnum`), `--checkpoint <snapshot-id>`, `--config` (optional), `--storage-path`/`--object-path`, `--seed`, `--dry-run`, `--format json`. `run_eval` resolves the checkpoint, short-circuits on `--dry-run` (no backend), else builds `HarnessDependencies` from local substrate and dispatches `BundledEval::from_settings(...).run(model, ctx)`, printing the `EvalReport` json.
- **Final flag surface:** `rollout eval --suite <mmlu|ifeval|gsm8k> --checkpoint <snapshot-id> [--config <toml>] [--storage-path PATH] [--object-path PATH] [--seed N] [--dry-run] [--format json]`.
- **Checkpoint→ModelRef resolution:** `--checkpoint` parses as a `ContentId`. If a `Snapshot` row in local storage matches, its `"tar"` part's `ContentId` pins `ModelRef.content_id` (spec 04); otherwise the value is used directly as a content-id pin (the dry-run / bare-id path). `lookup_snapshot` scans the `"snapshots"` namespace exactly like `snapshot show` / `train --resume`.
- `main.rs`: added `Eval(eval::EvalCmd)` to `enum Cmd` + `eval_dispatch` (tokio runtime + error→ExitCode mapping, mirroring infer/train/snapshot).
- `Cargo.toml`: added `rollout-harness-eval` path dep (existing `test-mock-backend` feature reused; the bundled eval path always uses `MockEvalBackend`, so no new feature wiring was needed).
- **Spec-08 reconciliation (D-EVAL-02):** removed the `rollout infer eval --config ... --checkpoint ...` line from §"rollout infer <mode>"; added a top-level `### rollout eval` entry (with examples + flag table) under §2; added `rollout eval` to the subcommand table; footnoted the `rollout plan` "Harness DAG: acyclic, 3 nodes" line as a v1.2 concern (D-CORE-02) — DAG validation NOT implemented.

### Task 2 — mdBook Harnesses chapter + SC4 verification
- New `docs/book/src/harnesses/` chapter wired into `SUMMARY.md` after Distribution:
  - `index.md` — three-trait batched principle (spec 07 §1), `from_settings(deps)` seam, algo-layer dep-direction (14-invariant lint), local-first (no cloud creds / GPU) constraint.
  - `env.md` — HARNESS-01: batched reset/step/close, multi-turn (D-ENV-01), reward via plugin host (D-ENV-03), no ObjectStore persistence in v1.1 (D-ENV-02), the EchoEnv / MockRewardEnv / `env_deterministic_replay` witnesses.
  - `tool-sandbox.md` — HARNESS-02: full depth matrix (summarized from the 07-04 README single-source-of-truth), seccomp/landlock layer order, fail-closed kernel gate (D-TOOL-02), macOS dev stub (D-TOOL-05), SSRF connector, honest threat boundary ("process-isolated, NOT VM-isolated; NOT a security perimeter for actively malicious code").
  - `eval.md` — HARNESS-03: MMLU acc/acc_norm, IFEval strict (language skipped), GSM8K `####`, pinned `LM_EVAL_VERSION`, offline-default fixtures (`HF_OFFLINE`), eval-as-WorkQueue-job (D-EVAL-05).
  - `cli.md` — `rollout eval` surface + spec-08 reconciliation note.
- **SC4 phase gate verified** (no lint edits — the 14 invariants + harness crate names were added pre-emptively in Phase 5; 07-00 made the crates physical).

## SC4 phase-gate verification record
| Gate | Command | Result |
|---|---|---|
| Workspace tests + 5 new crates | `cargo test --workspace --tests` (bare) | exit 0; `rollout_cloud_aws`, `rollout_cloud_gcp`, `rollout_harness_text`, `rollout_harness_tool`, `rollout_harness_eval` all present + green |
| Dep-direction at 14 invariants | `cargo test -p rollout-core dep_direction_invariants_hold` | exit 0; "Fourteen invariants total" comment intact (line 139) |
| Schema drift | `cargo xtask schema-gen` + `cargo test -p rollout-core --test schema_drift` | no drift (generated `rollout.schema.json` byte-identical to committed; RunConfig unchanged — no HarnessNode per D-CORE-02) |
| Supply chain | `cargo deny check` | `advisories ok, bans ok, licenses ok, sources ok` |
| Docs (DOCS-03, CI-shape) | `cargo doc --workspace --no-deps` (RUSTDOCFLAGS-deny) | exit 0 across all crates incl. the 5 new ones |
| mdBook | `mdbook build docs/book` | exit 0 |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `rollout eval` tests shared CWD-relative `./rollout.db` / `./object-store` → parallel-run flake**
- **Found during:** Task 1 (the `eval` test-name filter ran `eval_dry_run_no_backend` + `eval_dispatch_mock_backend_json` concurrently).
- **Issue:** both tests defaulted `--storage-path`/`--object-path` to the CWD, so the dispatch test's storage write collided with the dry-run test's snapshot lookup (redb lock / stray dir), and a stray `crates/rollout-cli/object-store/` was created.
- **Fix:** each test now passes an isolated `tempfile::tempdir()` via `--storage-path`/`--object-path`; cleaned the stray dirs. `tempfile` was already a dev-dependency.
- **Files modified:** `crates/rollout-cli/tests/eval_cli.rs`
- **Committed in:** `d5a43e0`

**2. [Rule 1 - Bug] rustdoc intra-doc link `[EvalReport]` not in scope**
- **Found during:** Task 1 (RUSTDOCFLAGS-deny `cargo doc` with broken-intra-doc-links).
- **Issue:** the crate-level doc referenced `[`EvalReport`]` but the type is not imported into `eval.rs` scope.
- **Fix:** demoted to plain code-span `` `EvalReport` ``.
- **Files modified:** `crates/rollout-cli/src/eval.rs`
- **Committed in:** `d5a43e0`

## Deferred Issues (out of scope — pre-existing, logged to `deferred-items.md`)

- **`cargo doc --workspace --no-deps --all-features` fails on `h3-quinn 0.0.7`.** `--all-features` enables `rollout-transport`'s EXPERIMENTAL `quic` stretch feature → pulls `h3-quinn 0.0.7` which accesses `quinn::StreamId.0` (now private) → `E0616`. **Confirmed pre-existing** (reproduced on the pre-Phase-7 tree). The DOCS-03 gate as CI enforces it (`cargo doc --workspace --no-deps`, no `--all-features`) is green. Fixing requires a quinn/h3-quinn version bump to an unused experimental feature — out of scope for the closeout.
- **`restart_no_duplicates.rs` clippy lints under `--features test-mock-backend`** (`format_push_string` + `too_many_lines`). Pre-existing plan 03-05 test; not in the Phase-7 verify path (Task-1 clippy uses no feature; SC4 bare `cargo test --workspace --tests` doesn't compile it).

## Verification (all green)
- `cargo test -p rollout-cli --features test-mock-backend --test eval_cli` → 4/4 (help-parse, unknown-suite reject, dry-run no-backend, mock-backend json dispatch).
- `cargo build -p rollout-cli` (no backend feature) → exit 0 (dry-run path works with neither feature).
- `rg "rollout eval" docs/specs/08-cli.md` matches; `rg "rollout infer eval" docs/specs/08-cli.md` → nothing.
- `cargo clippy -p rollout-cli --all-targets -- -D warnings` → clean; `cargo fmt --all -- --check` → clean.
- `RUSTDOCFLAGS=... cargo doc -p rollout-cli --no-deps --all-features` → clean (the EvalCmd public surface documented).
- The full SC4 gate table above → all green.

## Task Commits
1. **Task 1 RED:** `cee3d41` (test) — failing `eval_cli.rs`.
2. **Task 1 GREEN:** `d5a43e0` (feat) — `rollout eval` CLI + spec-08 reconcile.
3. **Task 2:** `8d5948e` (docs) — mdBook Harnesses chapter + deferred-items.

## Next Plan Readiness
Phase 7 is functionally complete: all three harness crates are usable end-to-end (env via trait, tools via sandbox, eval via the new `rollout eval` CLI), the spec is aligned to the shipped shape, the docs ship, and the SC4 workspace-count + 14-invariant gate is proven. Carry-forward to v1.2: the eval live full-split download (`datasets::load_online` + ObjectStore cache), the eval gate (HARNESS-04), HarnessGraph/DAG validation (D-CORE-02), and the two deferred pre-existing items above.

## Self-Check: PASSED
- All 8 claimed created files exist on disk (`eval.rs`, `eval_cli.rs`, 5 mdBook chapters, this SUMMARY).
- All 3 task commits present in git log: `cee3d41`, `d5a43e0`, `8d5948e`.
- Acceptance greps verified: `rollout eval` present + `rollout infer eval` absent in spec 08; `Eval(` arm in main.rs; `EvalHarness`/`harness_eval` dispatch in eval.rs; `seccomp` + `NOT VM-isolated` in tool-sandbox.md; `acc_norm` + `HF_OFFLINE` in eval.md; `Harnesses` + tool-sandbox link in SUMMARY.md.
- SC4 gate all green (workspace tests + 5 crates, 14 invariants, no schema drift, deny clean, CI-shape doc, mdbook build).
