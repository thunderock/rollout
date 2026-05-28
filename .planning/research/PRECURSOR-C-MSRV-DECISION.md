# Phase 5 Precursor C — MSRV Bump Decision

**Date:** 2026-05-28
**Spike branch:** probe edits applied in-place then reverted (no throwaway branch needed — `branching_strategy: none`); toolchains `1.91.0` + `1.91.1` already installed locally
**Author:** rollout team
**Decision:** BUMP

## Matrix Results (Rust 1.91)

| Step | Command | Status |
|------|---------|--------|
| 1 | cargo +1.91 build --workspace --all-features | ❌ broken — **pre-existing, NOT MSRV** (see note) |
| 1b | cargo +1.91 build --workspace (no `quic`) | ✅ clean |
| 2 | cargo +1.91 build -p rollout-storage --features postgres | ✅ clean |
| 3 | cargo +1.91 build -p rollout-backend-vllm --features vllm | ✅ clean |
| 4 | cargo +1.91 build -p rollout-backend-vllm --features train | ✅ clean |
| 5 | cargo +1.91 build -p rollout-plugin-host --features dev-hot-reload | ✅ clean |
| 6 | cargo +1.91 build -p rollout-runtime-batch | ✅ clean |
| 7 | cargo +1.91 test --workspace --tests | ✅ clean — 224 passed, 0 failed, 6 ignored (env-gated) |
| 8 | cargo +1.91 clippy --workspace --all-targets -- -D warnings | ⚠️ warnings — 1 new lint, 1 site, trivial (see below) |
| 9 | cargo +1.91 deny check | ✅ clean — advisories/bans/licenses/sources all ok |

### Note on Step 1 (`--all-features`)

The only `--all-features` failure is `h3-quinn 0.0.7` accessing the private field `quinn::StreamId.0`
(`E0616`). This is **not an MSRV-1.91 regression** — it is the documented v1.0 deferred tech-debt
(PROJECT.md Key Decisions: *"`tonic-h3 quic` deferred to post-Phase-6 — h3-quinn 0.0.7 accesses
private `quinn::StreamId.0`; tonic-h3 0.0.5 doesn't compile against quinn 0.11.x"*). It is gated
behind the EXPERIMENTAL `quic` Cargo feature, which is never built in default CI. The plain
`cargo +1.91 build --workspace` (Step 1b) — i.e. the surface CI actually exercises — is clean.
The bump does not change this situation: `quic` is broken on 1.88 too, for the same reason.

## New clippy lints on 1.91 not on 1.88

Cold (`target/debug/.fingerprint` wiped) full-workspace clippy on 1.91 reports exactly **one**
warning site:

```
warning: item in documentation is missing backticks
  --> crates/rollout-core/src/traits/harness.rs:21:38
   |
21 | /// Wraps an evaluation suite (MMLU, IFEval, GSM8K, ...).
   |                                      ^^^^^^
   = note: `-D clippy::doc-markdown` (clippy::pedantic, promoted via workspace lints)
help: try
   | /// Wraps an evaluation suite (MMLU, `IFEval`, GSM8K, ...).
```

`clippy::doc_markdown` (a `pedantic` lint, which the workspace promotes to `-D warnings`) newly
flags the unbackticked camel-case identifier `IFEval` in one doc comment. `MMLU` and `GSM8K` are
not flagged (all-caps, not mixed-case). Fix is a single backtick pair on one line. No code-logic
churn, no API change.

## Failing crates

None. (The `h3-quinn` failure is a third-party EXPERIMENTAL-feature dep, pre-existing on 1.88,
unrelated to the MSRV level — see Step 1 note above.)

## Decision

**BUMP**

## Rationale

The MSRV-relevant surface is clean on 1.91. Every crate CI actually builds compiles without error
(workspace build, postgres, vllm, train, dev-hot-reload, runtime-batch), all 224 workspace tests
pass, and `cargo deny check` is green. The historically risk-concentrated combination — pyo3 0.28 /
pyo3-async-runtimes 0.28 (Steps 3-4) — builds clean with zero deprecation or MSRV warnings, which
was the single biggest unknown going in.

The only new signal on 1.91 is one `clippy::doc_markdown` warning at a single doc-comment site
(`IFEval` needs backticks). That is a one-line cosmetic fix folded into the BUMP PR — not the kind
of "significant code churn" that would justify STAY. The `--all-features` failure is the
already-known, already-deferred `quic`/`h3-quinn` tech-debt and is invariant to the MSRV choice.

Bumping to 1.91 aligns the workspace with `aws-sdk-rust`'s current `main` MSRV (1.91.1, "stable-2"
policy) and **eliminates the `=`-exact-pin tax on `aws-sdk-*` / `aws-smithy-*`** that STACK.md Risk
Flag #1 flagged as HIGH. Plan 05 (AWS SDK PR) can then track current S3/SQS releases with caret
selectors and pull security/feature updates without a future forced MSRV revisit. Staying on 1.88
would lock us to the s3 1.112 cohort indefinitely and carry the manual-probe maintenance burden for
zero code-quality benefit, given the spike is effectively clean.

## If BUMP, follow-up actions

- Edit `rust-toolchain.toml` channel → `1.91.0`.
- Edit `Cargo.toml` `[workspace.package].rust-version` → `"1.91.0"`.
- Update `.github/workflows/ci.yml` — all 11 `dtolnay/rust-toolchain@1.88.0` pins → `@1.91.0`
  (jobs: `lint`, `test`, `deny`, `schema-drift`, `architecture-lint`, `rustdoc-check`,
  `docs-build`, `smoke`, `postgres-integration`, `infer-smoke`, `train-smoke`).
- Backtick `IFEval` in `crates/rollout-core/src/traits/harness.rs:21` to clear the new
  `clippy::doc_markdown` lint.
- Add the MSRV-bump note to `.planning/research/STACK.md` (caret pins now allowed on `aws-sdk-*`;
  `=`-exact-pin discipline of D-MSRV-02 fallback no longer required).
- PR description: *"After pulling this PR, run `cargo clean` then rebuild — 1.88-built `.rlib`
  metadata is incompatible with 1.91."*
- Plan 05 (AWS SDK PR) MAY use caret versions instead of `=`-exact pins on `aws-sdk-*`.

## If STAY, follow-up actions

- Add `msrv-probe` weekly cron CI job (per D-MSRV-02).
- Update `.planning/research/STACK.md` Risk Flag #1 documenting the blocking crate(s).
- Plan 05 (AWS SDK PR) MUST use `=`-exact pins per D-MSRV-02 fallback.

## Raw spike logs

Captured under `/tmp/msrv-1.91-*.log` during the 2026-05-28 spike:
`build-all-features` (quic E0616), `build-workspace` (clean), `storage`, `vllm`, `train`, `plugin`,
`runtime-batch`, `test` (224/0/6), `clippy-cold` (1 doc_markdown site), `deny` (ok).
