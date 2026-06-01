---
phase: 07-harnesses-env-tool-eval
plan: 03
subsystem: eval
tags: [rust, eval, mmlu, ifeval, gsm8k, lm-eval-harness, parquet, hf-hub, rustls, work-queue, cas]

# Dependency graph
requires:
  - phase: 07-harnesses-env-tool-eval
    plan: 00
    provides: "EvalHarness trait + spec-07 types, eval_reports row type, hf-hub/parquet/arrow-array workspace pins"
  - phase: 06-multi-node-distribution
    provides: "rollout-coordinator work_item CAS state machine (try_claim/try_complete)"
provides:
  - "rollout-harness-eval: bundled MMLU + IFEval + GSM8K scorers mirroring lm-eval-harness"
  - "offline-default SHA-pinned parquet dataset loader (HF_OFFLINE) + hf-hub rustls online stub"
  - "eval-as-WorkQueue-job (D-EVAL-05) reusing the Phase-6 CAS machine; MockEvalBackend (GPU-free)"
  - "BundledEval impl EvalHarness + aggregate EvalReport persistence (eval_reports row + CAS blob)"
  - "eval_reports namespace registered in embedded storage (07-00 row type wired)"
affects: [07-05-eval-cli, harness, eval]

# Tech tracking
tech-stack:
  added: [hf-hub@0.4.3, parquet@55.2.0, arrow-array@55.2.0, regex@1.11]
  patterns: [offline-default-fixture-loader-with-blake3-fail-on-drift, eval-as-WorkQueue-job, MockEvalBackend-mirrors-MockBackend, in-tree-Rust-fixture-generator]

key-files:
  created:
    - crates/rollout-harness-eval/src/suites/{mod,mmlu,ifeval,gsm8k}.rs
    - crates/rollout-harness-eval/src/datasets/mod.rs
    - crates/rollout-harness-eval/src/backend.rs
    - crates/rollout-harness-eval/src/job.rs
    - crates/rollout-harness-eval/examples/gen_fixtures.rs
    - crates/rollout-harness-eval/tests/fixtures/{mmlu,ifeval,gsm8k}_10.parquet
    - crates/rollout-harness-eval/tests/eval_loader_works_with_no_network.rs
    - crates/rollout-harness-eval/tests/eval_score_matches_lm_eval_harness.rs
  modified:
    - crates/rollout-harness-eval/src/lib.rs
    - crates/rollout-harness-eval/Cargo.toml
    - crates/rollout-storage/src/embedded/tables.rs
    - Cargo.toml

decisions:
  - "Pinned lm-eval reference: LM_EVAL_VERSION = \"lm-eval-harness-v0.4.9\""
  - "hf-hub 0.4 online API is api::tokio::ApiBuilder (the rustls/tokio feature path); 0.4 has no sync module without the ureq feature"
  - "Dropped parquet `async` feature (sync reads suffice for tiny fixtures) — keeps the dep surface lean; still rustls/openssl-free"
  - "Fixtures generated in-tree by a Rust example (no pyarrow/pip — AGENTS.md §7); uncompressed + fixed created_by → byte-reproducible"
  - "eval_reports embedded-storage table was unregistered after 07-00 — registered here (Rule 3 blocking fix)"

# Metrics
duration: 18min
completed: 2026-06-01
---

# Phase 7 Plan 03: HARNESS-03 rollout-harness-eval Summary

**Built `rollout-harness-eval`: MMLU (acc + acc_norm), IFEval (strict non-language), and GSM8K (`####` numeric) scorers that mirror lm-eval-harness at a pinned version, an offline-default SHA-pinned parquet dataset loader (rustls/openssl-free), eval-as-WorkQueue-job over the Phase-6 CAS machine, and a GPU-free MockEvalBackend — with the `eval_score_matches_lm_eval_harness` (≤1% parity, HF_OFFLINE=1) and `eval_loader_works_with_no_network` witnesses green and `cargo deny check bans` proving no openssl.**

## Performance
- **Duration:** ~18 min
- **Tasks:** 2
- **Files created/modified:** 14

## Accomplishments
- Three suite scorers under `src/suites/`:
  - **MMLU** (D-EVAL-03): `acc` = argmax raw continuation log-likelihood; `acc_norm` = argmax of length-normalized; lm-eval `mmlu` prompt format (`A.`/`B.`/`C.`/`D.` + `Answer:`). Both metrics reported.
  - **IFEval** (D-EVAL-04): pure-Rust strict checks for word/sentence count, JSON format, bullet count, keyword existence/frequency, case, placeholders. Language-detection (`language:*`) constraints skipped + warned.
  - **GSM8K** (D-EVAL-04): gold = number after final `####` (strip `,`/`$`); model = lm-eval `gsm8k` filter regex (last match); numeric equality.
- Offline-default loader (`src/datasets/mod.rs`): reads vendored 10-row parquet under `HF_OFFLINE`, blake3 fail-on-drift consts, `Arc`-cached; `hf-hub` (rustls) online stub linked so deny re-asserts the openssl-free graph.
- `MockEvalBackend` (`src/backend.rs`): deterministic canned completions + per-choice logprobs (mirrors `rollout-runtime-batch::MockBackend`), GPU-free.
- Eval-as-WorkQueue-job (`src/job.rs`, D-EVAL-05): idempotent `ContentId::of(postcard((suite, version, idx, model_id)))`, reuses `rollout_coordinator::work_item` `try_claim`/`try_complete`; per-example result blob content-addressed in the object store. No eval gate.
- `BundledEval` impl `EvalHarness` (`src/lib.rs`): dispatch by suite, aggregate `EvalReport` persisted to the `eval_reports` namespace (row) + content-addressed blob.
- Witnesses green: `eval_score_matches_lm_eval_harness` (MMLU/IFEval/GSM8K within 1% of baked-in reference + same-seed determinism), `eval_loader_works_with_no_network`.

## Pinned lm-eval version
`rollout_harness_eval::LM_EVAL_VERSION = "lm-eval-harness-v0.4.9"` — cited in scorer rustdoc and the descriptor `version`. The fixture reference scores were derived from this tag's `mmlu`/`ifeval`/`gsm8k` task conventions.

## Fixture blake3 hashes (SHA-pinned, byte-reproducible)
| Fixture | blake3 |
|---|---|
| `tests/fixtures/mmlu_10.parquet`   | `1ca8bbda8a66fe1592ec3f2978a0e8a7d7437e7e2be71c486b4cdc80b869bb7a` |
| `tests/fixtures/ifeval_10.parquet` | `d7eb173c556005ce979248315c0746e1110de4d4946594f40dc73088c395f669` |
| `tests/fixtures/gsm8k_10.parquet`  | `3abe754c7cf22ed2911fc7bc2bebf2818bd66a91e2a91fd7a6e9b548ec26d29a` |

Regenerate with `cargo run -p rollout-harness-eval --example gen_fixtures` (uncompressed parquet + fixed `created_by` → identical bytes). Consts live in `src/datasets/mod.rs` (`MMLU_TEST_BLAKE3` / `IFEVAL_TEST_BLAKE3` / `GSM8K_TEST_BLAKE3`).

## Expected fixture scores (witness references, ≤1% tolerance)
| Suite | Metrics | Reference (with the wired canned answers) |
|---|---|---|
| MMLU  | `acc`, `acc_norm` | 0.8 / 0.8 (mock prefers gold on 8/10) |
| GSM8K | `acc`             | 0.9 (mock correct on 9/10) |
| IFEval| `inst_strict_acc`, `prompt_strict_acc` | 1.0 / 1.0 over the 9 scorable prompts (row 8 all-skipped, excluded) |

## GSM8K filter regex (verbatim)
`(-?[$0-9.,]{2,})|(-?[0-9]+)` — `GSM8K_FILTER_REGEX` in `src/suites/gsm8k.rs`; the LAST match in the generation is parsed (commas/`$` stripped) and compared numerically to the gold.

## IFEval skipped constraints + denominator policy
- **Skipped:** any instruction id starting with `language:` (e.g. `language:response_language`) — no langdetect/langid dep (D-EVAL-04). A load-time `tracing::warn!("IFEval: skipping N language-detection constraints (unsupported in v1.1)")` fires when any are skipped.
- **Denominator policy (stated):** a skipped instruction is dropped from its prompt's instruction list; the prompt is still scored on the remainder. A prompt whose instructions are *all* skipped is excluded from BOTH the instruction-level and prompt-level strict denominators.
- **Implemented non-language checks:** `length_constraints:number_words`, `length_constraints:number_sentences`, `detectable_format:number_bullet_lists`, `detectable_format:json_format`, `keywords:existence`, `keywords:frequency`, `change_case:english_lowercase`, `change_case:english_capital`, `detectable_content:number_placeholders`. Unknown non-language ids conservatively fail (never silently pass).

## Eval-as-job queue-item shape (D-EVAL-05)
- One example = one `rollout_coordinator::work_item::WorkItemRecord { id, state }`.
- `id = ContentId::of(&postcard::to_stdvec(&(suite_name, LM_EVAL_VERSION, idx, model_id))?)` — idempotent; re-enqueue is a single-winner CAS no-op.
- Lifecycle `Pending → Running{worker, started_at_ms} → Done{result_id}` via `try_claim`/`try_complete`; `result_id` is the object-store `ContentId` of the postcard `ExampleResult { idx, score }`.
- Aggregate `EvalReport` → `eval_reports/<run>/report/<report_id>` (postcard row) + full blob content-addressed in the object store (spec 07 §4, spec 04).
- Per-task determinism: fixed seeded order + `temperature = 0` (mock is greedy/deterministic; same seed → identical per-task ordering + scores, asserted by `same_seed_same_scores`).

## Task Commits
1. **Task 1: suite scorers + offline fixture loader** — `54f5331` (feat)
2. **Task 2: EvalHarness impl + eval-as-job + MockEvalBackend + parity witness** — `fe72b02` (feat)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `eval_reports` storage namespace was never registered in the embedded backend**
- **Found during:** Task 2 (parity witness — `EvalReport` persistence)
- **Issue:** 07-00 shipped the `eval_reports` row type + `eval_report_key` (namespace `"eval_reports"`), but `rollout-storage`'s embedded `table_for`/`all_tables` allowlist was never extended, so any `eval_reports` write failed with `Fatal(ConfigInvalid { "unknown storage namespace: eval_reports" })`.
- **Fix:** Registered `T_EVAL_REPORTS` in `crates/rollout-storage/src/embedded/tables.rs` (`table_for` arm + `all_tables` 12→13). Postgres backend uses a generic `kv` table (no per-namespace allowlist) — no change needed there.
- **Files modified:** `crates/rollout-storage/src/embedded/tables.rs`
- **Committed in:** `fe72b02`

**2. [Rule 3 - Blocking] hf-hub 0.4 online API path differs from the planned `api::sync`**
- **Found during:** Task 1 (loader online stub)
- **Issue:** `hf_hub::api::sync` is gated behind the `ureq` feature (not enabled — we use `tokio` + `rustls-tls`). The sync module is unavailable; the tokio API is `hf_hub::api::tokio::ApiBuilder`.
- **Fix:** The online stub references `hf_hub::api::tokio::ApiBuilder` (links the crate, proving the rustls resolution; the always-on offline path never reaches it). The full online download + ObjectStore cache lands with the CLI in 07-05.
- **Files modified:** `crates/rollout-harness-eval/src/datasets/mod.rs`
- **Committed in:** `54f5331`

**3. [Rule 3 - Blocking] `regex` not in workspace deps; parquet `async` feature unnecessary**
- **Found during:** Task 1 (Cargo wiring)
- **Issue:** GSM8K/IFEval need `regex` (absent from `[workspace.dependencies]`). The 07-00 parquet pin enabled the `async` feature, unneeded for synchronous fixture reads.
- **Fix:** Added `regex = "1.11"` to workspace deps; dropped `async` from the parquet pin (kept `arrow`). Still rustls/openssl-free.
- **Files modified:** `Cargo.toml`
- **Committed in:** `54f5331`

**Total deviations:** 3 auto-fixed (all Rule 3 / blocking wiring). No architectural changes; the hf-hub openssl-free pin from Wave 0 held (no TLS-stack switch needed).

## Verification (all green, HF_OFFLINE=1)
- `cargo test -p rollout-harness-eval --tests` → 24 tests pass (incl. `eval_score_matches_lm_eval_harness` ≤1% parity + `eval_loader_works_with_no_network` + `same_seed_same_scores` determinism).
- `cargo deny check bans` → `bans ok`; `cargo tree -i openssl-sys` → no match (rustls-only resolution holds).
- `cargo deny check licenses` → `licenses ok` (zero new allowlist entries).
- `cargo clippy -p rollout-harness-eval --all-targets --all-features -- -D warnings` → clean.
- `RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" cargo doc -p rollout-harness-eval --no-deps --all-features` → clean (DOCS-03).
- `cargo fmt --all -- --check` → clean.
- `cargo test -p rollout-core --test dependency_direction` → 14/14 invariants hold (rollout-harness-eval → rollout-coordinator edge permitted).
- `cargo test -p rollout-storage` → green (eval_reports registration didn't regress existing tables).
- `cargo xtask schema-gen` → `RunConfig` JSON schema unchanged (no drift; new `BundledEvalSettings`/`SuiteSetting` derive JsonSchema but are not part of RunConfig). NOTE: the Python `datamodel-codegen` stub step is a CI-only tool, not installed on this dev box.

## Next Plan Readiness
07-05 (`rollout eval` CLI + spec-08 reconciliation) can now: dispatch `BundledEval` via `BundledEvalSettings`, wire the `hf-hub` online full-split download + ObjectStore cache into `datasets::load_online`, and surface the `eval_reports` rows. The IFEval per-task `score = -1.0` sentinel marks all-skipped prompts for the CLI to filter.

## Self-Check: PASSED
- All 11 claimed source/fixture/test files exist on disk.
- Both task commits present in git log: `54f5331`, `fe72b02`.
- Acceptance greps pass: `acc_norm`+`acc` (mmlu), `unsupported in v1.1` (ifeval), `####` (gsm8k), `LM_EVAL_VERSION`, `WorkItemRecord`+`ContentId::of` (job), `MockEvalBackend`.
- The eval-gate / openssl grep "hits" are false positives — both match only the documentation that asserts their ABSENCE (`//! no pause/resume-training hook here`; `# ... openssl-free`). No eval gate, no openssl dep (`cargo deny check bans` = `bans ok`).
