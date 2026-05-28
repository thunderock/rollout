---
phase: 05-cloud-layer-object-store-snapshots
plan: 03
type: execute
wave: 1
depends_on: []
files_modified:
  - rust-toolchain.toml
  - Cargo.toml
  - .github/workflows/ci.yml
  - .planning/research/PRECURSOR-C-MSRV-DECISION.md
  - .planning/research/STACK.md
autonomous: false
requirements: [DOCS-01, DOCS-02]
precursor: true
gap_closure: false
must_haves:
  truths:
    - "A spike artifact (.planning/research/PRECURSOR-C-MSRV-DECISION.md) records BUMP or STAY with evidence."
    - "If BUMP: workspace MSRV is 1.91; rust-toolchain.toml + Cargo.toml workspace.package.rust-version + CI matrix all updated; AWS SDK exact pins can be relaxed in Plan 05."
    - "If STAY: workspace MSRV stays 1.88; a `msrv-probe` weekly cron CI job lands; STACK.md Risk Flag #1 documents the blocking crate."
    - "Either way: `cargo test --workspace --tests` exits 0, `cargo clippy --workspace --all-targets -- -D warnings` exits 0, `cargo deny check` exits 0."
  artifacts:
    - path: ".planning/research/PRECURSOR-C-MSRV-DECISION.md"
      provides: "Decision record with status (clean | warnings | broken), failing-crate list, recommendation"
      contains: "## Decision"
    - path: "rust-toolchain.toml"
      provides: "channel = \"1.91\" (if BUMP) or unchanged (if STAY)"
      contains: "channel"
    - path: ".github/workflows/ci.yml"
      provides: "every dtolnay/rust-toolchain@ pin updated to 1.91 (if BUMP) or msrv-probe job added (if STAY)"
      contains: "rust-toolchain"
  key_links:
    - from: ".planning/research/PRECURSOR-C-MSRV-DECISION.md"
      to: "Cargo.toml + rust-toolchain.toml"
      via: "PR diff documented in decision record"
      pattern: "BUMP|STAY"
---

<objective>
**Precursor C** — Evaluate Rust workspace MSRV bump 1.88 → 1.91 (per D-MSRV-01..03 + RESEARCH.md Pattern 9). Lands as a **spike + decision** PR: either bumps MSRV and drops AWS SDK exact pins (BUMP path), or stays on 1.88 with a documented blocker + weekly msrv-probe cron (STAY path per D-MSRV-02 fallback).

**This plan contains a `checkpoint:decision` checkpoint** — the spike output drives a BUMP vs STAY choice that the user signs off before the PR lands.

**Lands as standalone PR against `main` BEFORE Phase 5 Stages 1-5.** Per D-PRECURSOR-01 ordering: B → A → C (this plan = C, lands last so Plan 01 + 02 land on a clean baseline first).

Purpose: validate MSRV-1.91 compatibility BEFORE AWS SDK crates enter the workspace; eliminate the `=`-exact-pin tax on aws-sdk-* if 1.91 is clean.
Output: spike artifact + (conditional) toolchain bump + (conditional) msrv-probe cron job.
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
@$HOME/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/PROJECT.md
@.planning/ROADMAP.md
@.planning/phases/05-cloud-layer-object-store-snapshots/05-CONTEXT.md
@.planning/phases/05-cloud-layer-object-store-snapshots/05-RESEARCH.md
@.planning/research/STACK.md
@rust-toolchain.toml
@Cargo.toml
@.github/workflows/ci.yml
</context>

<tasks>

<task type="auto">
  <name>Task 1: Run the MSRV-1.91 spike + produce decision artifact</name>
  <files>.planning/research/PRECURSOR-C-MSRV-DECISION.md</files>
  <read_first>
    - rust-toolchain.toml (current channel)
    - Cargo.toml (current workspace.package.rust-version)
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-RESEARCH.md §"Pattern 9" lines 663-697 (full spike methodology)
    - .planning/research/STACK.md (the MSRV / exact-pin justification)
  </read_first>
  <action>
    On a throwaway branch (do not commit the toolchain edits during the spike — they only commit if the decision is BUMP, in Task 2):

    1. Save current state:
       ```bash
       git stash --include-untracked || true
       ```
    2. Probe-edit `rust-toolchain.toml`: change `channel = "1.88.0"` (or whatever the current line says) to `channel = "1.91.0"`.
    3. Probe-edit `Cargo.toml`: change `rust-version = "1.88"` to `rust-version = "1.91"` under `[workspace.package]`.
    4. Run the 9-step matrix from RESEARCH.md Pattern 9, capturing pass/fail per crate:
       ```bash
       rustup install 1.91.0 || true
       cargo +1.91 build --workspace --all-features 2>&1 | tee /tmp/msrv-1.91-build-all-features.log
       cargo +1.91 build -p rollout-storage --features postgres 2>&1 | tee /tmp/msrv-1.91-storage.log
       cargo +1.91 build -p rollout-backend-vllm --features vllm 2>&1 | tee /tmp/msrv-1.91-vllm.log
       cargo +1.91 build -p rollout-backend-vllm --features train 2>&1 | tee /tmp/msrv-1.91-train.log
       cargo +1.91 build -p rollout-plugin-host --features dev-hot-reload 2>&1 | tee /tmp/msrv-1.91-plugin.log
       cargo +1.91 build -p rollout-runtime-batch 2>&1 | tee /tmp/msrv-1.91-runtime-batch.log
       cargo +1.91 test --workspace --tests 2>&1 | tee /tmp/msrv-1.91-test.log
       cargo +1.91 clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tee /tmp/msrv-1.91-clippy.log
       cargo +1.91 deny check 2>&1 | tee /tmp/msrv-1.91-deny.log
       ```
    5. Revert the probe edits regardless of outcome (the BUMP path commits a clean diff in Task 2):
       ```bash
       git checkout -- rust-toolchain.toml Cargo.toml
       ```
    6. Write `.planning/research/PRECURSOR-C-MSRV-DECISION.md` capturing the spike outcome. **Use this exact template:**

       ```markdown
       # Phase 5 Precursor C — MSRV Bump Decision

       **Date:** YYYY-MM-DD
       **Spike branch:** spike/msrv-1.91 (throwaway)
       **Author:** rollout team
       **Decision:** BUMP | STAY  ← exactly one

       ## Matrix Results (Rust 1.91)

       | Step | Command | Status |
       |------|---------|--------|
       | 1 | cargo +1.91 build --workspace --all-features | ✅ clean / ⚠️ warnings / ❌ broken |
       | 2 | cargo +1.91 build -p rollout-storage --features postgres | ✅ / ⚠️ / ❌ |
       | 3 | cargo +1.91 build -p rollout-backend-vllm --features vllm | ✅ / ⚠️ / ❌ |
       | 4 | cargo +1.91 build -p rollout-backend-vllm --features train | ✅ / ⚠️ / ❌ |
       | 5 | cargo +1.91 build -p rollout-plugin-host --features dev-hot-reload | ✅ / ⚠️ / ❌ |
       | 6 | cargo +1.91 build -p rollout-runtime-batch | ✅ / ⚠️ / ❌ |
       | 7 | cargo +1.91 test --workspace --tests | ✅ / ⚠️ / ❌ |
       | 8 | cargo +1.91 clippy --workspace --all-targets --all-features -- -D warnings | ✅ / ⚠️ / ❌ |
       | 9 | cargo +1.91 deny check | ✅ / ⚠️ / ❌ |

       ## New clippy lints on 1.91 not on 1.88
       (paste excerpts)

       ## Failing crates
       (none, or paste error excerpts)

       ## Decision

       **BUMP** | **STAY**

       ## Rationale
       (1-3 paragraphs)

       ## If BUMP, follow-up actions
       - Edit `rust-toolchain.toml` channel → 1.91.0
       - Edit `Cargo.toml` rust-version → "1.91"
       - Update `.github/workflows/ci.yml` `dtolnay/rust-toolchain@` pins → 1.91.0
       - Plan 05 (AWS SDK PR) MAY use caret versions instead of `=`-exact pins on aws-sdk-*.

       ## If STAY, follow-up actions
       - Add `msrv-probe` weekly cron CI job (per D-MSRV-02).
       - Update `.planning/research/STACK.md` Risk Flag #1 documenting the blocking crate(s).
       - Plan 05 (AWS SDK PR) MUST use `=`-exact pins per D-MSRV-02 fallback.
       ```

       Fill in every row of the matrix with the captured logs. The decision is a **judgment call**: if all 9 steps pass clean, recommend BUMP; if any step fails or any clippy lint requires significant code churn, recommend STAY.
  </action>
  <verify>
    <automated>test -f .planning/research/PRECURSOR-C-MSRV-DECISION.md && grep -E '^\\*\\*Decision:\\*\\*.*(BUMP|STAY)' .planning/research/PRECURSOR-C-MSRV-DECISION.md</automated>
  </verify>
  <acceptance_criteria>
    - `test -f .planning/research/PRECURSOR-C-MSRV-DECISION.md` is true.
    - File contains exactly one of `**Decision:** BUMP` or `**Decision:** STAY` (not both, not "TBD").
    - File contains a populated matrix table with all 9 rows annotated.
    - File contains a Rationale section ≥ 1 paragraph.
    - At spike completion: `git diff rust-toolchain.toml Cargo.toml` shows NO changes (probe edits reverted; BUMP edits land in Task 2 if decision = BUMP).
    - `cargo +1.91 deny check` log (if generated) attached or summarized in the decision file.
  </acceptance_criteria>
  <done>
    Spike complete; decision artifact committed (file under `.planning/research/`); workspace `rust-toolchain.toml` + `Cargo.toml` unchanged from baseline.
  </done>
</task>

<task type="checkpoint:decision" gate="blocking">
  <name>Task 2 (checkpoint): User signs off on BUMP or STAY</name>
  <decision>Should we bump workspace MSRV from 1.88 to 1.91, or stay on 1.88 with the msrv-probe cron fallback?</decision>
  <context>
    Task 1 produced `.planning/research/PRECURSOR-C-MSRV-DECISION.md` with the spike matrix and a recommendation. This checkpoint surfaces the recommendation to the user for explicit sign-off because the choice affects every downstream Phase 5 plan (Plan 05 AWS SDK pinning strategy) and may surface clippy churn that warrants a separate cleanup PR.

    Per D-MSRV-01 the default expectation is BUMP if the spike is clean. Per D-MSRV-02 the fallback is STAY with a documented blocker. Per D-MSRV-03 there is no 1.89/1.90 intermediate.
  </context>
  <options>
    <option id="option-a">
      <name>BUMP — proceed to 1.91</name>
      <pros>Drops aws-sdk-* exact-pin tax; aligns with aws-sdk-rust main MSRV (1.91.1); reduces precursor follow-up work.</pros>
      <cons>If new clippy lints fired, requires a small cleanup commit in the same PR; cargo cache invalidates (developers see one-time "incompatible metadata" until `cargo clean`).</cons>
    </option>
    <option id="option-b">
      <name>STAY — remain on 1.88, add msrv-probe cron</name>
      <pros>Zero code churn; preserves the proven baseline; the weekly cron tracks when 1.91 becomes viable.</pros>
      <cons>Plan 05 MUST keep `=`-exact pins on aws-sdk-*; future security advisories on aws-sdk-* may force a manual MSRV revisit before v1.1 ships.</cons>
    </option>
  </options>
  <resume-signal>Select: option-a (BUMP) or option-b (STAY)</resume-signal>
  <files>n/a (checkpoint surfaces decision; no autonomous file edits)</files>
  <action>Pause execution and surface the BUMP/STAY decision recorded in `.planning/research/PRECURSOR-C-MSRV-DECISION.md` to the user. Wait for the user to type "option-a" (BUMP) or "option-b" (STAY). Record the selection so Task 3 branches correctly.</action>
  <verify>
    <automated>test -f .planning/research/PRECURSOR-C-MSRV-DECISION.md &amp;&amp; grep -E '^\*\*Decision:\*\*.*(BUMP|STAY)' .planning/research/PRECURSOR-C-MSRV-DECISION.md</automated>
  </verify>
  <acceptance_criteria>
    - The user has explicitly selected option-a or option-b via the chat surface.
    - `.planning/research/PRECURSOR-C-MSRV-DECISION.md` `**Decision:**` line agrees with the user's selection (if they override the spike's recommendation, the file is updated).
  </acceptance_criteria>
  <done>User has signed off on BUMP or STAY; Task 3 has a definitive branch to follow.</done>
</task>

<task type="auto">
  <name>Task 3: Apply the decision — toolchain edits (BUMP) or msrv-probe cron (STAY)</name>
  <files>rust-toolchain.toml, Cargo.toml, .github/workflows/ci.yml, .planning/research/STACK.md</files>
  <read_first>
    - .planning/research/PRECURSOR-C-MSRV-DECISION.md (the recorded decision)
    - rust-toolchain.toml (current channel for diff context)
    - Cargo.toml (workspace.package.rust-version location)
    - .github/workflows/ci.yml (every `dtolnay/rust-toolchain@<version>` reference — there are at least 8 in the current 14-job file)
    - .planning/research/STACK.md (Risk Flag section)
  </read_first>
  <action>
    Read `.planning/research/PRECURSOR-C-MSRV-DECISION.md` first; branch on the recorded `**Decision:**` value.

    **If Decision = BUMP:**
    1. `rust-toolchain.toml` — change the `channel` line to `channel = "1.91.0"`.
    2. `Cargo.toml` — under `[workspace.package]`, change `rust-version = "1.88"` to `rust-version = "1.91"`.
    3. `.github/workflows/ci.yml` — every `uses: dtolnay/rust-toolchain@1.88.0` line becomes `uses: dtolnay/rust-toolchain@1.91.0`. There are at least 8 occurrences across `lint`, `test`, `deny`, `commitlint` (none, it's macos), `schema-drift`, `architecture-lint`, `rustdoc-check`, `docs-build`, `smoke`, `postgres-integration`, `infer-smoke`, `train-smoke`. Run `grep -nE 'dtolnay/rust-toolchain@[0-9]' .github/workflows/ci.yml` to enumerate.
    4. `.planning/research/STACK.md` — under the existing MSRV section, add a one-paragraph note: "Workspace MSRV bumped 1.88 → 1.91 in Phase 5 precursor C (YYYY-MM-DD). AWS SDK crates may use caret version selectors as of this commit; the `=`-exact-pin discipline (D-MSRV-02 fallback) is no longer required."
    5. Add a PR description note: "After pulling this PR, run `cargo clean` then rebuild — 1.88-built .rlib metadata is incompatible with 1.91."

    Run the full local CI matrix to confirm green:
    ```bash
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace --tests
    cargo deny check
    cargo doc --workspace --no-deps
    ```

    **If Decision = STAY:**
    1. Do NOT edit `rust-toolchain.toml` or `Cargo.toml`.
    2. Add a new CI job to `.github/workflows/ci.yml` (insert before the `docs-build` job):
       ```yaml
         msrv-probe:
           # D-MSRV-02 fallback: weekly cron that tries cargo update -p aws-sdk-s3 --precise <next>
           # and reports MSRV breaks. Lands here because Plan 05's AWS SDK will be exact-pinned.
           runs-on: ubuntu-latest
           if: github.event_name == 'schedule'
           steps:
             - uses: actions/checkout@v4
             - uses: dtolnay/rust-toolchain@1.88.0
             - uses: Swatinem/rust-cache@v2
               with:
                 shared-key: ci-msrv-probe
             - name: Probe next aws-sdk-s3 version against MSRV 1.88
               continue-on-error: true
               run: |
                 set -e
                 # Read current pinned version from Cargo.toml workspace deps
                 CUR=$(grep -E '^aws-sdk-s3.*version.*"=1' Cargo.toml || true)
                 echo "Current pin: $CUR"
                 cargo update -p aws-sdk-s3 --precise 1.113.0 2>&1 | tee /tmp/probe-1.113.log || true
                 if cargo build -p rollout-cloud-aws 2>&1 | tee /tmp/probe-build.log; then
                   echo "::warning::aws-sdk-s3 1.113.0 builds on MSRV 1.88 — consider unpinning."
                 else
                   echo "::notice::aws-sdk-s3 1.113.0 still MSRV-incompat; staying pinned."
                 fi
                 git checkout Cargo.lock
       ```
       Plus add a `schedule:` trigger at the top of the workflow (if not already present):
       ```yaml
       on:
         pull_request:
         push:
           branches: [main]
         schedule:
           - cron: '0 6 * * 1'   # Mondays 06:00 UTC — D-MSRV-02 weekly probe
       ```
    3. `.planning/research/STACK.md` — under Risk Flag #1, document the blocking crate(s) from PRECURSOR-C-MSRV-DECISION.md (e.g., "pyo3-async-runtimes 0.28 emits clippy::elided_lifetime_in_paths under 1.91 — would require 20+ touch-up edits across rollout-backend-vllm; staying on 1.88 until pyo3-async-runtimes resolves upstream.")
    4. PR description: "Stays on Rust 1.88 per D-MSRV-02 fallback. See `.planning/research/PRECURSOR-C-MSRV-DECISION.md` for rationale. Plan 05 will keep `=`-exact pins on aws-sdk-*."
  </action>
  <verify>
    <automated>cargo test --workspace --tests && cargo clippy --workspace --all-targets -- -D warnings && cargo deny check</automated>
  </verify>
  <acceptance_criteria>
    **If BUMP:**
    - `grep -E 'channel.*=.*"1.91' rust-toolchain.toml` returns a match.
    - `grep -E 'rust-version.*=.*"1.91"' Cargo.toml` returns a match.
    - `grep -cE 'dtolnay/rust-toolchain@1.91.0' .github/workflows/ci.yml` returns at least 8 (matches the count of 1.88.0 lines previously).
    - `grep -cE 'dtolnay/rust-toolchain@1.88.0' .github/workflows/ci.yml` returns 0.
    - `.planning/research/STACK.md` contains the string "1.91" within an MSRV-related paragraph.
    - `cargo +1.91 test --workspace --tests` exits 0 (or just `cargo test --workspace --tests` after rust-toolchain.toml flips the default).

    **If STAY:**
    - `grep -E 'channel.*=.*"1.88' rust-toolchain.toml` returns a match (unchanged).
    - `grep -E '^  msrv-probe:' .github/workflows/ci.yml` returns a match.
    - `grep -nE 'cron:.*0 6' .github/workflows/ci.yml` returns a match (Monday 06:00 schedule).
    - `.planning/research/STACK.md` contains the documented blocker name from the decision artifact.
    - `cargo test --workspace --tests` exits 0 (no behavior change on the existing 1.88 baseline).

    **Either path:**
    - `cargo clippy --workspace --all-targets -- -D warnings` exits 0.
    - `cargo deny check` exits 0.
    - `cargo doc --workspace --no-deps` builds clean with RUSTDOCFLAGS deny.
  </acceptance_criteria>
  <done>
    Either workspace MSRV is bumped to 1.91 with all toolchain pins updated and CI green, OR workspace stays on 1.88 with the `msrv-probe` weekly cron job and STACK.md documents the blocker. Full CI matrix passes.
  </done>
</task>

</tasks>

<verification>
  <wave-checks>
    - `cargo fmt --all -- --check` exits 0.
    - `cargo test --workspace --tests` exits 0.
    - `cargo clippy --workspace --all-targets -- -D warnings` exits 0.
    - `cargo deny check` exits 0.
    - `cargo doc --workspace --no-deps` builds with RUSTDOCFLAGS deny set.
    - `.planning/research/PRECURSOR-C-MSRV-DECISION.md` exists with a single explicit BUMP/STAY decision.
  </wave-checks>
</verification>

<success_criteria>
  - Spike output is captured in a committed artifact (decision file).
  - User has signed off on BUMP or STAY via the checkpoint.
  - Workspace toolchain configuration reflects the decision exactly (no half-state).
  - If STAY: msrv-probe cron job lands so we get weekly signal on when 1.91 becomes viable.
  - If BUMP: every CI runner pin is updated; PR description warns developers to `cargo clean`.
  - Either path leaves Plan 05 with a clear AWS SDK pinning rule (exact vs caret).
</success_criteria>

<output>
After completion, create `.planning/phases/05-cloud-layer-object-store-snapshots/05-03-SUMMARY.md` per template.
</output>

## Validation Architecture

| Test type | Coverage | Sampling cadence |
|-----------|----------|------------------|
| smoke (full CI matrix on 1.91 or 1.88) | Workspace builds + tests + clippy + deny stay green | every PR after the toolchain change lands |
| cron (msrv-probe, STAY path only) | Track when 1.91 becomes viable upstream | weekly Mondays 06:00 UTC |
| manual (decision sign-off) | User confirms BUMP/STAY | once at checkpoint Task 2 |

**Wave 0 dependency:** none — spike file is created fresh; toolchain edits land in Task 3.

**autonomous: false** — contains `checkpoint:decision` in Task 2.
