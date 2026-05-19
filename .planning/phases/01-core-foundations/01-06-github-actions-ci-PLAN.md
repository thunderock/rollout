---
phase: 01-core-foundations
plan: 06
type: execute
wave: 4
depends_on: ['01-core-foundations/02', '01-core-foundations/03', '01-core-foundations/04', '01-core-foundations/05', '01-core-foundations/07']
files_modified:
  - .github/workflows/ci.yml
  - scripts/check-docs-tests-touched.sh
autonomous: true
requirements: [CORE-02, CORE-04, DOCS-01, DOCS-02, DOCS-03]

must_haves:
  truths:
    - "CI runs on pull_request and push to main"
    - "Separate jobs: lint, test, deny, commitlint, schema-drift, architecture-lint, unused-deps, rustdoc-check, docs-build, docs-deploy, docs-test-policy"
    - "Each job uses dtolnay/rust-toolchain@1.88.0 + Swatinem/rust-cache@v2 with a unique shared-key"
    - "deny job runs on ubuntu-latest via EmbarkStudios/cargo-deny-action@v2"
    - "schema-drift job runs `cargo xtask schema-gen` then `git diff --exit-code schemas/ python/` (regenerates Python stubs locally before diffing)"
    - "architecture-lint job runs `cargo test --test dependency_direction --workspace`"
    - "commitlint job installs convco with the OS-correct artifact (deb on Ubuntu, macos on macOS)"
    - "schema-drift job invokes `check-jsonschema --check-metaschema` on the generated schema"
    - "rustdoc-check job runs cargo doc with the §9.3 RUSTDOCFLAGS and fails on missing crate-level docs / broken intra-doc links / warnings (DOCS-03)"
    - "docs-build job installs mdBook and uploads docs/book/book/ as a Pages artifact (DOCS-01)"
    - "docs-deploy job runs only on push to main and uses actions/deploy-pages@v4 with pages: write + id-token: write (DOCS-01)"
    - "docs-test-policy job runs scripts/check-docs-tests-touched.sh on PRs, skipped on main pushes (DOCS-02)"
    - "scripts/check-docs-tests-touched.sh exists, is executable, and honors the [skip-docs-check] commit trailer"
  artifacts:
    - path: ".github/workflows/ci.yml"
      provides: "All 11 CI jobs (7 original + rustdoc-check + docs-build + docs-deploy + docs-test-policy)"
      contains: "name: ci"
    - path: "scripts/check-docs-tests-touched.sh"
      provides: "Per-commit doc/test policy enforcement (DOCS-02)"
      contains: "skip-docs-check"
  key_links:
    - from: ".github/workflows/ci.yml"
      to: "Makefile + cargo + xtask + check-jsonschema + mdbook + scripts/check-docs-tests-touched.sh"
      via: "run steps invoking make/cargo/sh/mdbook"
      pattern: "cargo|make|check-jsonschema|cargo deny|mdbook|check-docs-tests-touched"
    - from: "scripts/check-docs-tests-touched.sh"
      to: "git diff (PR base..HEAD)"
      via: "shell script"
      pattern: "git diff --name-only"
---

<objective>
Land the GitHub Actions CI workflow mirroring `/Users/ashutosh/personal/vector/.github/workflows/ci.yml` shape (D-CI-01) and adding the rollout-specific jobs: architecture-lint (D-CI-02), schema-drift (D-CI-03), commitlint (D-CI-04), and the four standing docs-policy jobs from AGENTS.md §9 — `rustdoc-check` (D-DOCS-04 / DOCS-03), `docs-build` + `docs-deploy` (D-DOCS-02 / DOCS-01), `docs-test-policy` (D-DOCS-03 / DOCS-02). Closes ROADMAP exit criteria for CORE-02 ("Dependency-boundary lint enforced in CI") and CORE-04 ("`rollout schema --format json` emits a JSON Schema validated by an external validator"), and brings DOCS-01..03 into CI per AGENTS.md §9.

Purpose: Convert local correctness (`make check`, `cargo test`, drift test, `make docs`) into branch-protection-grade CI gating. Plan 06 is the last Phase 1 plan because it consumes the outputs of Plans 02–05 and 07.
Output: A single `.github/workflows/ci.yml` with 11 jobs; a `scripts/check-docs-tests-touched.sh` enforcer; each job pinned to specific action versions per RESEARCH.md and AGENTS.md §9.
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
@$HOME/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/STATE.md
@.planning/ROADMAP.md
@.planning/REQUIREMENTS.md
@ROADMAP.md
@.planning/phases/01-core-foundations/01-CONTEXT.md
@.planning/phases/01-core-foundations/01-RESEARCH.md
@.planning/phases/01-core-foundations/01-VALIDATION.md
@AGENTS.md
@.planning/phases/01-core-foundations/01-PLAN-02-SUMMARY.md
@.planning/phases/01-core-foundations/01-PLAN-03-SUMMARY.md
@.planning/phases/01-core-foundations/01-PLAN-04-SUMMARY.md
@.planning/phases/01-core-foundations/01-PLAN-05-SUMMARY.md
@.planning/phases/01-core-foundations/01-PLAN-07-SUMMARY.md
@Makefile
@deny.toml
@docs/book/book.toml

<interfaces>
<!-- Action versions (RESEARCH.md §Standard Stack → CI / tooling): -->
- dtolnay/rust-toolchain@1.88.0
- Swatinem/rust-cache@v2 (per-job shared-key: ci-lint, ci-test, ci-deny, ci-schema-drift, ci-arch-lint, ci-unused-deps, ci-rustdoc, ci-docs-build)
- EmbarkStudios/cargo-deny-action@v2
- bnjbvr/cargo-machete@v0.9.2
- actions/checkout@v4
- actions/setup-python@v5
- peaceiris/actions-mdbook@v2 (mdBook 0.4.x)
- actions/upload-pages-artifact@v3
- actions/deploy-pages@v4
- actions/configure-pages@v5

<!-- convco install URLs (RESEARCH.md §Installation): -->
Ubuntu: https://github.com/convco/convco/releases/latest/download/convco-deb.zip → sudo dpkg -i
macOS:  https://github.com/convco/convco/releases/latest/download/convco-macos.zip → chmod +x && sudo mv

<!-- Python tools to pip-install: -->
pip install datamodel-code-generator==0.57.0 check-jsonschema==0.37.2

<!-- Runners (D-CI-01): -->
- lint, test, commitlint: macos-14
- deny, schema-drift, architecture-lint, unused-deps, rustdoc-check, docs-build, docs-deploy, docs-test-policy: ubuntu-latest

<!-- Rustdoc gate flags (AGENTS.md §9.3 / D-DOCS-04): -->
RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs"
</interfaces>
</context>

<tasks>

<task type="auto" tdd="false">
  <name>Task 1: .github/workflows/ci.yml — 7 core jobs per RESEARCH.md</name>
  <files>.github/workflows/ci.yml</files>
  <read_first>
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-RESEARCH.md (§Code Examples → GitHub Actions CI + §Pitfalls 6, 7)
    - /Users/ashutosh/personal/vector/.github/workflows/ci.yml (reference shape — read-only)
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-CONTEXT.md (D-CI-01 through D-CI-04)
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-VALIDATION.md (Manual-Only Verifications — CI gating)
    - /Users/ashutosh/personal/rollout/Makefile (from Plan 02)
    - /Users/ashutosh/personal/rollout/deny.toml (from Plan 05)
    - /Users/ashutosh/personal/rollout/scripts/check-schema.sh (from Plan 04)
  </read_first>
  <action>
Create `/Users/ashutosh/personal/rollout/.github/workflows/ci.yml` (mkdir -p `.github/workflows` first). This task lays down the 7 original jobs from D-CI-01..04; Task 2 appends the 4 standing-docs jobs and the policy script. Use EXACT content (mirroring RESEARCH.md §Code Examples → GitHub Actions CI):

```yaml
name: ci

on:
  pull_request:
  push:
    branches: [main]

# docs-deploy needs these to publish GitHub Pages.
permissions:
  contents: read
  pages: write
  id-token: write

# Allow one concurrent Pages deploy per ref.
concurrency:
  group: "pages-${{ github.ref }}"
  cancel-in-progress: false

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: short

jobs:
  lint:
    runs-on: macos-14
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.88.0
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
        with:
          shared-key: ci-lint
      - run: cargo fmt --all -- --check
      - run: cargo clippy --all-targets --all-features -- -D warnings

  test:
    runs-on: macos-14
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.88.0
      - uses: Swatinem/rust-cache@v2
        with:
          shared-key: ci-test
      - run: cargo test --workspace --tests

  deny:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.88.0
      - uses: EmbarkStudios/cargo-deny-action@v2
        with:
          command: check advisories licenses bans sources

  commitlint:
    runs-on: macos-14
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - name: Install convco (macOS)
        run: |
          set -euo pipefail
          curl -sSL https://github.com/convco/convco/releases/latest/download/convco-macos.zip -o /tmp/convco.zip
          unzip -o /tmp/convco.zip -d /tmp/convco
          chmod +x /tmp/convco/convco
          sudo mv /tmp/convco/convco /usr/local/bin/
      - name: Lint commits
        run: |
          if [ "${{ github.event_name }}" = "pull_request" ]; then
            convco check ${{ github.event.pull_request.base.sha }}..HEAD
          else
            # Tolerant on direct main pushes during bootstrap (D-CI-04).
            convco check HEAD~10..HEAD || true
          fi

  schema-drift:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.88.0
      - uses: Swatinem/rust-cache@v2
        with:
          shared-key: ci-schema-drift
      - name: Set up Python
        uses: actions/setup-python@v5
        with:
          python-version: "3.11"
      - name: Install Python tools
        run: pip install datamodel-code-generator==0.57.0 check-jsonschema==0.37.2
      - name: Regenerate schemas
        run: cargo xtask schema-gen
      - name: Assert no drift
        run: |
          set -e
          git diff --exit-code schemas/ python/ \
            || (echo "::error::Schema drift detected. Run 'cargo xtask schema-gen' and commit."; exit 1)
      - name: Validate generated schema (meta-schema)
        run: check-jsonschema --check-metaschema schemas/rollout.schema.json

  architecture-lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.88.0
      - uses: Swatinem/rust-cache@v2
        with:
          shared-key: ci-arch-lint
      - name: Dependency-direction lint
        run: cargo test -p rollout-core --test dependency_direction

  unused-deps:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: bnjbvr/cargo-machete@v0.9.2
```

Concrete rules:
- Action versions pinned EXACTLY per RESEARCH.md §Standard Stack.
- Each job has a UNIQUE `shared-key` (RESEARCH.md §Pitfall 7) — `ci-lint`, `ci-test`, `ci-schema-drift`, `ci-arch-lint`.
- macOS convco install ONLY (no Ubuntu commitlint job — commitlint runs on macos-14 per D-CI-01 + RESEARCH.md §Pitfall 6).
- `schema-drift` job: install python tools → regenerate → diff (fail on drift) → meta-schema validate. The order matters: regenerate FIRST, otherwise drift check is meaningless (RESEARCH.md §Pitfall 5).
- `architecture-lint` job: runs the SAME test as `make test` would — `cargo test -p rollout-core --test dependency_direction`. This is the deliberate-violation gate from CORE-02 exit criterion.
- `unused-deps`: `cargo-machete` action; no extra config needed.
- `commitlint` is tolerant on direct `main` pushes (`|| true`) per D-CI-04.
- Top-level `permissions:` and `concurrency:` blocks are required for the docs-deploy job that Task 2 will append. Set them now so they're not forgotten.
- DO NOT add a release workflow, dmg/app-bundle workflow, or Windows runners (CONTEXT.md §Deferred).
  </action>
  <acceptance_criteria>
    - `test -f /Users/ashutosh/personal/rollout/.github/workflows/ci.yml`
    - `grep -qE '^name: ci' /Users/ashutosh/personal/rollout/.github/workflows/ci.yml`
    - `grep -q '^  lint:' /Users/ashutosh/personal/rollout/.github/workflows/ci.yml`
    - `grep -q '^  test:' /Users/ashutosh/personal/rollout/.github/workflows/ci.yml`
    - `grep -q '^  deny:' /Users/ashutosh/personal/rollout/.github/workflows/ci.yml`
    - `grep -q '^  commitlint:' /Users/ashutosh/personal/rollout/.github/workflows/ci.yml`
    - `grep -q '^  schema-drift:' /Users/ashutosh/personal/rollout/.github/workflows/ci.yml`
    - `grep -q '^  architecture-lint:' /Users/ashutosh/personal/rollout/.github/workflows/ci.yml`
    - `grep -q '^  unused-deps:' /Users/ashutosh/personal/rollout/.github/workflows/ci.yml`
    - `grep -q 'dtolnay/rust-toolchain@1.88.0' /Users/ashutosh/personal/rollout/.github/workflows/ci.yml`
    - `grep -q 'pages: write' /Users/ashutosh/personal/rollout/.github/workflows/ci.yml`
    - `grep -q 'id-token: write' /Users/ashutosh/personal/rollout/.github/workflows/ci.yml`
    - `! grep -q 'windows-' /Users/ashutosh/personal/rollout/.github/workflows/ci.yml` (no Windows runners — deferred)
    - `python3 -c "import yaml; yaml.safe_load(open('/Users/ashutosh/personal/rollout/.github/workflows/ci.yml'))"` exits 0 (valid YAML)
  </acceptance_criteria>
  <verify>
    <automated>cd /Users/ashutosh/personal/rollout && test -f .github/workflows/ci.yml && python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))" && grep -q 'dtolnay/rust-toolchain@1.88.0' .github/workflows/ci.yml && grep -q 'cargo test -p rollout-core --test dependency_direction' .github/workflows/ci.yml && grep -q 'check-jsonschema --check-metaschema' .github/workflows/ci.yml && grep -q '^  lint:' .github/workflows/ci.yml && grep -q '^  test:' .github/workflows/ci.yml && grep -q '^  deny:' .github/workflows/ci.yml && grep -q '^  commitlint:' .github/workflows/ci.yml && grep -q '^  schema-drift:' .github/workflows/ci.yml && grep -q '^  architecture-lint:' .github/workflows/ci.yml && grep -q '^  unused-deps:' .github/workflows/ci.yml && grep -q 'pages: write' .github/workflows/ci.yml && grep -q 'id-token: write' .github/workflows/ci.yml</automated>
  </verify>
  <done>The 7 core CI jobs are committed (lint, test, deny, commitlint, schema-drift, architecture-lint, unused-deps), pinned action versions, per-job rust-cache shared-keys, schema-drift + architecture-lint exit-criteria gates in place. The top-level `permissions:` block is ready for Task 2's docs-deploy job. Maps to 01-VALIDATION.md Manual-Only Verifications.</done>
</task>

<task type="auto" tdd="false">
  <name>Task 2: Append 4 docs/rustdoc jobs to ci.yml + create scripts/check-docs-tests-touched.sh</name>
  <files>.github/workflows/ci.yml, scripts/check-docs-tests-touched.sh</files>
  <read_first>
    - /Users/ashutosh/personal/rollout/AGENTS.md §9 (entire standing-rules section, especially §9.1 docs site, §9.2 per-commit policy, §9.3 rustdoc gate, §9.5 enforcement)
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-CONTEXT.md (D-DOCS-02, D-DOCS-03, D-DOCS-04)
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-PLAN-07-SUMMARY.md (docs/book scaffold output)
    - /Users/ashutosh/personal/rollout/docs/book/book.toml (from Plan 07 — must exist before this task runs)
    - /Users/ashutosh/personal/rollout/.github/workflows/ci.yml (written by Task 1 — append-only)
    - existing /Users/ashutosh/personal/rollout/scripts/ (mkdir -p if missing)
  </read_first>
  <action>
**Step A — append four jobs to `.github/workflows/ci.yml`.** Open the file written in Task 1 and append the following jobs UNDER the existing `jobs:` map (same indentation as `lint:`, `test:`, etc.):

```yaml
  rustdoc-check:
    runs-on: ubuntu-latest
    env:
      RUSTDOCFLAGS: "-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs"
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.88.0
      - uses: Swatinem/rust-cache@v2
        with:
          shared-key: ci-rustdoc
      - name: Build workspace rustdoc with deny flags
        run: cargo doc --workspace --no-deps --all-features

  docs-build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.88.0
      - uses: Swatinem/rust-cache@v2
        with:
          shared-key: ci-docs-build
      - uses: peaceiris/actions-mdbook@v2
        with:
          mdbook-version: '0.4.40'
      - name: Build mdBook
        run: mdbook build docs/book
      - name: Configure Pages
        if: github.event_name == 'push' && github.ref == 'refs/heads/main'
        uses: actions/configure-pages@v5
      - name: Upload Pages artifact
        if: github.event_name == 'push' && github.ref == 'refs/heads/main'
        uses: actions/upload-pages-artifact@v3
        with:
          path: docs/book/book

  docs-deploy:
    if: github.event_name == 'push' && github.ref == 'refs/heads/main'
    needs: [docs-build, test, lint]
    runs-on: ubuntu-latest
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    steps:
      - name: Deploy to GitHub Pages
        id: deployment
        uses: actions/deploy-pages@v4

  docs-test-policy:
    if: github.event_name == 'pull_request'
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - name: Run docs+tests touched check
        env:
          BASE_SHA: ${{ github.event.pull_request.base.sha }}
          HEAD_SHA: ${{ github.event.pull_request.head.sha }}
        run: bash scripts/check-docs-tests-touched.sh
```

Concrete rules:
- `rustdoc-check`: top-level `env.RUSTDOCFLAGS` is the EXACT string from AGENTS.md §9.3 / D-DOCS-04 — do not abbreviate.
- `docs-build`: pinned to `peaceiris/actions-mdbook@v2` with `mdbook-version: '0.4.40'` (mdBook 0.4.x compatible per Plan 07). Pages artifact upload happens only on pushes to main (PRs only build to verify; they don't deploy).
- `docs-deploy`: gated by `if: github.event_name == 'push' && github.ref == 'refs/heads/main'`; `needs: [docs-build, test, lint]` so deploy doesn't ship a green book when tests are red. The `environment: github-pages` block is REQUIRED for `actions/deploy-pages@v4` to publish.
- `docs-test-policy`: gated by `if: github.event_name == 'pull_request'`. On `push` to main, the job is skipped entirely (bootstrap exemption per D-DOCS-03 / AGENTS.md §9.2). Passes the base + head SHAs to the script via env vars.
- After append, verify YAML still parses: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"`.

**Step B — create `scripts/check-docs-tests-touched.sh`.** mkdir -p `scripts/` first. Write the file with EXACT content (then `chmod +x`):

```bash
#!/usr/bin/env bash
# scripts/check-docs-tests-touched.sh
# Enforces AGENTS.md §9.2 / DOCS-02:
#   Every commit modifying code under crates/, python/, or xtask/
#   must also touch docs/, tests/, or inline doc comments.
# Bypass via [skip-docs-check] trailer in the latest commit message.

set -euo pipefail

BASE="${BASE_SHA:-}"
HEAD="${HEAD_SHA:-HEAD}"

if [[ -z "$BASE" ]]; then
  echo "::error::BASE_SHA env var is required (PR base ref)."
  exit 2
fi

# Bypass: [skip-docs-check] in the most recent commit message on the PR head.
if git log -1 --format=%B "$HEAD" | grep -qF '[skip-docs-check]'; then
  echo "docs-test-policy: bypassed via [skip-docs-check] trailer"
  exit 0
fi

CHANGED_FILES=$(git diff --name-only "${BASE}...${HEAD}")

code_changed=false
docs_or_tests_changed=false

while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  case "$f" in
    crates/*|python/*|xtask/*)
      code_changed=true
      ;;
    docs/*|*/tests/*|tests/*)
      docs_or_tests_changed=true
      ;;
  esac
done <<< "$CHANGED_FILES"

if ! $code_changed; then
  echo "docs-test-policy: no code changes; nothing to enforce."
  exit 0
fi

if $docs_or_tests_changed; then
  echo "docs-test-policy: code change accompanied by docs/ or tests/ change."
  exit 0
fi

# Fallback: inline doc-comment edits in the diff hunks.
# git diff -U0 yields hunk lines prefixed with `+` or `-`; look for /// or //!.
if git diff -U0 "${BASE}...${HEAD}" -- 'crates/**' 'python/**' 'xtask/**' \
     | grep -qE '^\+.*(///|//!|""")'; then
  echo "docs-test-policy: code change accompanied by inline doc-comment edits."
  exit 0
fi

echo "::error::Code under crates/, python/, or xtask/ changed without accompanying docs/, tests/, or inline doc-comment changes. See AGENTS.md §9.2. To bypass for bootstrap or mechanical refactors, add '[skip-docs-check]' to the most recent commit message."
exit 1
```

Then `chmod +x /Users/ashutosh/personal/rollout/scripts/check-docs-tests-touched.sh`.

**Step C — final YAML validation.**
```bash
cd /Users/ashutosh/personal/rollout
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))" && echo "YAML OK"
test -x scripts/check-docs-tests-touched.sh
```
  </action>
  <acceptance_criteria>
    - `grep -q '^  rustdoc-check:' /Users/ashutosh/personal/rollout/.github/workflows/ci.yml`
    - `grep -q '^  docs-build:' /Users/ashutosh/personal/rollout/.github/workflows/ci.yml`
    - `grep -q '^  docs-deploy:' /Users/ashutosh/personal/rollout/.github/workflows/ci.yml`
    - `grep -q '^  docs-test-policy:' /Users/ashutosh/personal/rollout/.github/workflows/ci.yml`
    - `grep -q 'rustdoc::broken_intra_doc_links' /Users/ashutosh/personal/rollout/.github/workflows/ci.yml`
    - `grep -q 'rustdoc::missing_crate_level_docs' /Users/ashutosh/personal/rollout/.github/workflows/ci.yml`
    - `grep -q 'cargo doc --workspace --no-deps --all-features' /Users/ashutosh/personal/rollout/.github/workflows/ci.yml`
    - `grep -q 'peaceiris/actions-mdbook@v2' /Users/ashutosh/personal/rollout/.github/workflows/ci.yml`
    - `grep -q 'mdbook build docs/book' /Users/ashutosh/personal/rollout/.github/workflows/ci.yml`
    - `grep -q 'actions/upload-pages-artifact@v3' /Users/ashutosh/personal/rollout/.github/workflows/ci.yml`
    - `grep -q 'actions/deploy-pages@v4' /Users/ashutosh/personal/rollout/.github/workflows/ci.yml`
    - `grep -q 'environment:' /Users/ashutosh/personal/rollout/.github/workflows/ci.yml`
    - `grep -q 'name: github-pages' /Users/ashutosh/personal/rollout/.github/workflows/ci.yml`
    - `grep -q 'scripts/check-docs-tests-touched.sh' /Users/ashutosh/personal/rollout/.github/workflows/ci.yml`
    - `test -x /Users/ashutosh/personal/rollout/scripts/check-docs-tests-touched.sh`
    - `head -1 /Users/ashutosh/personal/rollout/scripts/check-docs-tests-touched.sh | grep -q '^#!/usr/bin/env bash'`
    - `grep -qF '[skip-docs-check]' /Users/ashutosh/personal/rollout/scripts/check-docs-tests-touched.sh`
    - `grep -q 'git diff --name-only' /Users/ashutosh/personal/rollout/scripts/check-docs-tests-touched.sh`
    - `python3 -c "import yaml; yaml.safe_load(open('/Users/ashutosh/personal/rollout/.github/workflows/ci.yml'))"` exits 0 (still valid YAML)
  </acceptance_criteria>
  <verify>
    <automated>cd /Users/ashutosh/personal/rollout && grep -q '^  rustdoc-check:' .github/workflows/ci.yml && grep -q '^  docs-build:' .github/workflows/ci.yml && grep -q '^  docs-deploy:' .github/workflows/ci.yml && grep -q '^  docs-test-policy:' .github/workflows/ci.yml && test -x scripts/check-docs-tests-touched.sh && grep -qF '[skip-docs-check]' scripts/check-docs-tests-touched.sh && grep -q 'git diff --name-only' scripts/check-docs-tests-touched.sh && python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"</automated>
  </verify>
  <done>The four standing-docs jobs (`rustdoc-check`, `docs-build`, `docs-deploy`, `docs-test-policy`) are appended to `ci.yml` with correct gating (PRs build only; main pushes deploy), pinned action versions, and `scripts/check-docs-tests-touched.sh` exists, is executable, and honors the `[skip-docs-check]` trailer. Closes DOCS-01..03 CI gates per AGENTS.md §9. Maps to 01-VALIDATION.md row 06/2.</done>
</task>

</tasks>

<verification>
- YAML is syntactically valid (python yaml.safe_load).
- All 11 jobs present (7 core + rustdoc-check + docs-build + docs-deploy + docs-test-policy).
- Action versions match RESEARCH.md exactly.
- Per-job shared-keys are unique.
- schema-drift regenerates BEFORE diffing AND runs the meta-schema validator.
- architecture-lint runs `cargo test --test dependency_direction`.
- rustdoc-check applies the §9.3 RUSTDOCFLAGS.
- docs-build uses `peaceiris/actions-mdbook@v2` and uploads `docs/book/book/`.
- docs-deploy gated to main pushes with `environment: github-pages` and the required `pages: write` + `id-token: write` permissions.
- docs-test-policy gated to PRs and invokes the script.
- `scripts/check-docs-tests-touched.sh` exists, is executable, and honors `[skip-docs-check]`.
- No Windows runners (deferred per CONTEXT.md).
</verification>

<success_criteria>
- `.github/workflows/ci.yml` exists with all 11 jobs.
- Pinned versions: `dtolnay/rust-toolchain@1.88.0`, `Swatinem/rust-cache@v2`, `EmbarkStudios/cargo-deny-action@v2`, `bnjbvr/cargo-machete@v0.9.2`, `peaceiris/actions-mdbook@v2`, `actions/upload-pages-artifact@v3`, `actions/deploy-pages@v4`.
- Schema-drift gate operational (CORE-04 CI gate).
- Architecture-lint gate operational (CORE-02 CI gate).
- Rustdoc gate operational (DOCS-03 CI gate).
- mdBook build + Pages deploy operational (DOCS-01 CI gate).
- docs-test-policy operational (DOCS-02 CI gate).
- Manual-Only Verifications from 01-VALIDATION.md are queued for first-PR check.
</success_criteria>

<output>
After completion, create `.planning/phases/01-core-foundations/01-PLAN-06-SUMMARY.md` documenting:
- Final job list + runner OS for each (11 jobs total)
- Pinned action versions
- Any deviations from RESEARCH.md / AGENTS.md §9 (expected: none)
- The two Manual-Only Verifications that must be checked on first PR (per 01-VALIDATION.md)
- Confirmation that `scripts/check-docs-tests-touched.sh` was created with executable bit and `[skip-docs-check]` bypass
</output>
