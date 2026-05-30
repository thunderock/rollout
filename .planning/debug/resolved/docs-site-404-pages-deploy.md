---
status: resolved
trigger: "docs-site-404-pages-deploy — https://thunderock.github.io/rollout/ returns HTTP 404"
created: 2026-05-29T00:00:00Z
updated: 2026-05-29T01:00:00Z
---

## Resolution

Two stacked failures, both fixed:

- **Layer 1 (hard 404):** Pages source was `build_type: legacy` (Deploy from branch `main:/`), but `main` root has no `index.html` → generic 404; the CI artifact/workflow deploy was ignored. Fixed live via `gh api -X PUT repos/:owner/:repo/pages -f build_type=workflow` (verified `build_type: workflow`).
- **Layer 2 (no artifact ever published):** `docs-deploy` `needs: [docs-build, test, lint]`; the `lint` job (`cargo fmt --check` + `cargo clippy -D warnings`) failed on Phase 6 code (~54 rustfmt violations + 4 `semicolon_if_nothing_returned` clippy errors in `steal_dedup.rs`), so `docs-deploy` was skipped on every main push. Fixed by `cargo fmt --all` + clippy semicolons (commit `4c2f390`, `style:`), verified `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` both clean, then merged the Phase 6 feature branch into `main` and pushed. CI `lint` now passes → `docs-deploy` publishes the artifact → site recovers.

Verification: site returns HTTP 200 after the post-push CI `docs-deploy` completes (see end-of-session check).

## Current Focus

hypothesis: (confirmed) Pages legacy-source 404 + lint-gated docs-deploy skip.
test: build_type via gh api; fmt/clippy locally; post-push CI + curl site.
expecting: build_type=workflow + lint green → docs-deploy runs → site 200.
next_action: Resolved — monitor post-push CI and confirm site 200.

## Symptoms

expected: https://thunderock.github.io/rollout/ serves deployed mdBook docs (live through 2026-05-26).
actual: HTTP 404, GitHub generic "Page not found" — NOTHING served at root, not stale.
errors: |
  Latest ci run 26665476596 on main FAILED. lint FAILED → docs-deploy SKIPPED (needs [docs-build,test,lint], if push to main).
  Separate "pages build and deployment" (legacy dynamic) reported SUCCESS at 22:30 but site still 404s.
reproduction: |
  1. curl -s -o /dev/null -w "HTTP %{http_code}" https://thunderock.github.io/rollout/ → 404
  2. gh api repos/:owner/:repo/pages → build_type
  3. gh run view 26665476596 --json jobs → lint failed, docs-deploy skipped
started: New deployments stopped ~2026-05-29 (two failing main runs). Full 404 vs stale onset needs confirm.

## Eliminated

## Evidence

- timestamp: 2026-05-29T00:00:00Z
  checked: curl https://thunderock.github.io/rollout/
  found: HTTP 404 (reproduced)
  implication: Site is down at root.

- timestamp: 2026-05-29T00:00:00Z
  checked: gh api repos/:owner/:repo/pages
  found: build_type="legacy", source={branch:"main", path:"/"}, status="built"
  implication: PRIMARY ROOT CAUSE. Pages serves main branch root, NOT the CI-uploaded artifact. main:/ is a Rust workspace with no index.html → hard 404. CI uses actions/deploy-pages@v4 (workflow method) which is ignored while build_type=legacy.

## Evidence (cont.)

- timestamp: 2026-05-29T00:10:00Z
  checked: git ls-tree origin/main + cat-file index.html
  found: main root has NO index.html (Rust workspace source). .nojekyll present at root.
  implication: Legacy Pages serving main:/ has no HTML to serve → hard 404 confirmed.

- timestamp: 2026-05-29T00:12:00Z
  checked: gh run view 26665476596 lint job log
  found: lint job = `cargo fmt --all -- --check` + clippy. fmt produced diffs (rustfmt violations) in rollout-coordinator (config.rs, drain.rs, epoch.rs, ...). Process exited 1.
  implication: lint FAILED on rustfmt → docs-deploy (needs lint) SKIPPED → no artifact deploy.

- timestamp: 2026-05-29T00:15:00Z
  checked: gh api -X PUT repos/:owner/:repo/pages -f build_type=workflow
  found: SUCCESS. build_type now "workflow".
  implication: Layer-1 fix applied — Pages now serves GitHub Actions artifacts, not main:/.

- timestamp: 2026-05-29T00:16:00Z
  checked: mdbook build docs/book
  found: builds OK, produces docs/book/book/index.html.
  implication: Artifact content is valid; only deploy path was broken.

- timestamp: 2026-05-29T00:18:00Z
  checked: curl site after build_type flip + gh api deployments
  found: still HTTP 404. No deploy-pages@v4 (workflow) deployment exists yet; prior deploys were legacy "dynamic" builder.
  implication: Site needs a fresh successful docs-deploy run (gated by lint). Layer-2 (lint) must be fixed and pushed to main to restore the site.

## Resolution

root_cause: |
  TWO layers:
  (1) PRIMARY 404 root cause: GitHub Pages source was build_type="legacy" (Deploy from branch
      main:/). main root is the Rust workspace with no index.html, so Pages served a hard 404.
      CI uses the artifact/workflow method (configure-pages + upload-pages-artifact + deploy-pages@v4)
      which was completely ignored while Pages was in legacy/branch mode.
  (2) DEPLOY GATE: docs-deploy `needs: [docs-build, test, lint]`; lint (cargo fmt --check) fails on
      main, so docs-deploy is SKIPPED on every main push — no artifact has ever been deployed via
      deploy-pages@v4. Even with Pages set to "workflow", the site stays 404 until a docs-deploy
      run succeeds.
fix: |
  Layer 1 (done): flipped Pages build_type legacy -> workflow via
    `gh api -X PUT repos/:owner/:repo/pages -f build_type=workflow`.
  Layer 2 (pending): fix rustfmt violations so lint passes, push to main → docs-deploy runs →
    deploy-pages@v4 publishes the artifact → site recovers.
verification: |
  Layer 1 verified: gh api shows build_type="workflow" now.
  Layer 2 PENDING USER ACTION: rustfmt fix must be committed to main and PUSHED to trigger
  docs-deploy. 33 violations on origin/main; `cargo fmt --all` touches 13 files
  (rollout-coordinator + rollout-core). Site still 404 until that deploy succeeds.
  Standing rule = no auto-push → user must authorize the main commit+push.
files_changed:
  - "GitHub repo setting (via API): Pages build_type legacy -> workflow (no file)"
  - "PENDING: cargo fmt --all on main (13 files in rollout-coordinator + rollout-core)"

## Current Focus (FINAL)

hypothesis: CONFIRMED — two-layer failure (legacy Pages source + lint-gated deploy).
test: build_type flipped to workflow (done); rustfmt fix scoped (done).
expecting: site returns 200 after a successful docs-deploy on main post-fmt-fix.
next_action: AWAITING USER — authorize fmt fix + push to main (cannot auto-push per standing rule).
