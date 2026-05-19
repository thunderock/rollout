---
phase: 01-core-foundations
plan: 02
type: execute
wave: 1
depends_on: []
files_modified:
  - Makefile
  - README.md
  - package.json
  - .gitignore
autonomous: true
requirements: [CORE-01, CORE-04, DOCS-01]

must_haves:
  truths:
    - "Top-level Makefile exposes lint, test, build, check, schema-gen, validate-schema, docs, graphify, help targets"
    - "All targets are .PHONY"
    - "`make -n <target>` (dry-run) parses with no errors for every target"
    - "`make docs` runs `mdbook build docs/book` and `cargo doc --workspace --no-deps --all-features`"
    - "`make graphify` runs `npx graphify-ts generate . --directed --svg` writing into `graphify-out/` (D-GRAPHIFY-01)"
    - "Root `package.json` exists declaring `@mohammednagy/graphify-ts` in `devDependencies`"
    - "`.gitignore` excludes `node_modules/`, `graphify-out/`, `*.tsbuildinfo`"
    - "README.md points users to `make help` for the canonical entrypoint"
  artifacts:
    - path: "Makefile"
      provides: "Local + CI entrypoint for all dev tasks"
      contains: ".PHONY"
    - path: "README.md"
      provides: "Quick-start blurb referencing `make help`"
      contains: "make help"
  key_links:
    - from: "Makefile"
      to: "cargo (build/test/clippy/fmt/run) + mdbook"
      via: "shell commands"
      pattern: "cargo\\s+(fmt|clippy|test|build|xtask|run|doc)|mdbook\\s+build"
    - from: "README.md"
      to: "Makefile"
      via: "documented entry point"
      pattern: "make\\s+(help|build|test|lint|docs)"
---

<objective>
Create the top-level `Makefile` per D-LOCAL-01/02 (locked decisions in CONTEXT.md) and update `README.md` to point at `make help` as the canonical local-dev entrypoint. The Makefile is the single entrypoint humans and CI both call; D-LOCAL-02 fixes `lint` and `test` command bodies exactly. AGENTS.md §9.1 also requires a `docs` target that builds the mdBook site + workspace rustdoc; Plan 07 ships the book scaffold this target consumes. Runs in parallel with Plans 01 and 07 (no file overlap).

Purpose: Lock in the user-facing dev surface (`make build / test / lint / check / schema-gen / docs`) before any later plan adds CI yaml that calls the same targets — single source of truth for "how do I build this thing".
Output: A Makefile that `make -n <target>` accepts for every target; a README blurb pointing to it.
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
</context>

<tasks>

<task type="auto" tdd="false">
  <name>Task 1: Write top-level Makefile with all required targets</name>
  <files>Makefile</files>
  <read_first>
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-RESEARCH.md (§Code Examples → Makefile and Schema validation in Makefile)
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-CONTEXT.md (D-LOCAL-01, D-LOCAL-02 — locked target commands; D-DOCS-01 — docs target)
    - /Users/ashutosh/personal/rollout/AGENTS.md §9.1 (docs site is standing rule; `make docs` is the local entrypoint)
    - /Users/ashutosh/personal/vector/Makefile (reference shape — read-only)
    - existing /Users/ashutosh/personal/rollout/Makefile (none expected; do not overwrite if present without diff)
  </read_first>
  <action>
Create `/Users/ashutosh/personal/rollout/Makefile` with EXACT content (tabs for recipe indentation — not spaces — make is tab-sensitive):

```makefile
.PHONY: lint test build check schema-gen validate-schema docs graphify help

export CARGO_TERM_COLOR := always

lint:
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -D warnings

test:
	cargo test --workspace --tests

build:
	cargo build --workspace

check: lint test

schema-gen:
	cargo xtask schema-gen

validate-schema:
	cargo run -p rollout-cli -- schema --format json > /tmp/rollout-schema-test.json
	check-jsonschema --check-metaschema /tmp/rollout-schema-test.json

docs:
	mdbook build docs/book
	cargo doc --workspace --no-deps --all-features

graphify:
	npx graphify-ts generate . --directed --svg

help:
	@echo "lint             cargo fmt --check + clippy -D warnings"
	@echo "test             cargo test --workspace --tests"
	@echo "build            cargo build --workspace"
	@echo "check            lint + test"
	@echo "schema-gen       regenerate schemas/rollout.schema.json + python stubs"
	@echo "validate-schema  meta-validate the JSON Schema (requires check-jsonschema)"
	@echo "docs             mdbook build + cargo doc --workspace --no-deps --all-features"
	@echo "graphify         build codebase knowledge graph via graphify-ts (out: graphify-out/)"
```

Concrete rules:
- Recipe lines MUST use a literal TAB (0x09) at the start (verify with `cat -A Makefile` — should show `^I` before each command).
- Body of `lint` is the EXACT pair from D-LOCAL-02: `cargo fmt --all -- --check` then `cargo clippy --all-targets --all-features -- -D warnings`.
- Body of `test` is the EXACT command from D-LOCAL-02: `cargo test --workspace --tests`.
- `check` is a composition: `check: lint test` — no recipe lines.
- `validate-schema` body uses `check-jsonschema --check-metaschema` per RESEARCH.md §Schema validation in Makefile + CI.
- `docs` body is the two-line pair from AGENTS.md §9.1: `mdbook build docs/book` then `cargo doc --workspace --no-deps --all-features`. Requires `mdbook` on PATH (`cargo install mdbook --locked --version 0.4.x` documented in README).
- Help target uses `@echo` (suppress recipe echo).
- Do NOT add `format` as a separate target — `lint` already covers `fmt --check`. (D-LOCAL-02 does not require it.)
- Do NOT include `dmg`, `run`, `start` from vector — rollout is a server framework, not a desktop app (CONTEXT.md §Deferred).
  </action>
  <acceptance_criteria>
    - `test -f /Users/ashutosh/personal/rollout/Makefile`
    - `grep -qE '^\.PHONY: .*lint.*test.*build.*check.*schema-gen.*validate-schema.*docs.*graphify.*help' /Users/ashutosh/personal/rollout/Makefile`
    - `grep -qE '^lint:' /Users/ashutosh/personal/rollout/Makefile`
    - `grep -qE '^test:' /Users/ashutosh/personal/rollout/Makefile`
    - `grep -qE '^build:' /Users/ashutosh/personal/rollout/Makefile`
    - `grep -qE '^check:' /Users/ashutosh/personal/rollout/Makefile`
    - `grep -qE '^schema-gen:' /Users/ashutosh/personal/rollout/Makefile`
    - `grep -qE '^validate-schema:' /Users/ashutosh/personal/rollout/Makefile`
    - `grep -qE '^docs:' /Users/ashutosh/personal/rollout/Makefile`
    - `grep -qE '^graphify:' /Users/ashutosh/personal/rollout/Makefile`
    - `grep -qE '^help:' /Users/ashutosh/personal/rollout/Makefile`
    - `grep -q 'cargo fmt --all -- --check' /Users/ashutosh/personal/rollout/Makefile`
    - `grep -q 'cargo clippy --all-targets --all-features -- -D warnings' /Users/ashutosh/personal/rollout/Makefile`
    - `grep -q 'cargo test --workspace --tests' /Users/ashutosh/personal/rollout/Makefile`
    - `grep -q 'cargo xtask schema-gen' /Users/ashutosh/personal/rollout/Makefile`
    - `grep -q 'check-jsonschema --check-metaschema' /Users/ashutosh/personal/rollout/Makefile`
    - `grep -q 'mdbook build docs/book' /Users/ashutosh/personal/rollout/Makefile`
    - `grep -q 'cargo doc --workspace --no-deps --all-features' /Users/ashutosh/personal/rollout/Makefile`
    - `grep -q 'npx graphify-ts generate . --directed --svg' /Users/ashutosh/personal/rollout/Makefile`
    - `cd /Users/ashutosh/personal/rollout && make -n help` exits 0 (parses)
    - `cd /Users/ashutosh/personal/rollout && make -n lint` exits 0
    - `cd /Users/ashutosh/personal/rollout && make -n test` exits 0
    - `cd /Users/ashutosh/personal/rollout && make -n schema-gen` exits 0
    - `cd /Users/ashutosh/personal/rollout && make -n validate-schema` exits 0
    - `cd /Users/ashutosh/personal/rollout && make -n docs` exits 0
    - `cd /Users/ashutosh/personal/rollout && make -n graphify` exits 0
    - `cat -A /Users/ashutosh/personal/rollout/Makefile | grep -E '^\^I' | head -1` shows the first recipe line uses a real tab (verifies tab indentation)
  </acceptance_criteria>
  <verify>
    <automated>cd /Users/ashutosh/personal/rollout && test -f Makefile && grep -q '^\.PHONY:' Makefile && grep -q 'cargo fmt --all -- --check' Makefile && grep -q 'cargo test --workspace --tests' Makefile && grep -q 'check-jsonschema --check-metaschema' Makefile && grep -qE '^docs:' Makefile && grep -q 'mdbook build docs/book' Makefile && grep -qE '^graphify:' Makefile && grep -q 'npx graphify-ts generate' Makefile && make -n help >/dev/null && make -n lint >/dev/null && make -n test >/dev/null && make -n schema-gen >/dev/null && make -n validate-schema >/dev/null && make -n docs >/dev/null && make -n graphify >/dev/null</automated>
  </verify>
  <done>Makefile present with all 8 targets, `.PHONY` declared, exact D-LOCAL-02 command bodies for `lint` and `test`, AGENTS.md §9.1 `docs` body for `docs`, and `make -n` parses every target. Maps to 01-VALIDATION.md sampling rate `make lint` / `make test` / `make docs` infrastructure (after wave merge).</done>
</task>

<task type="auto" tdd="false">
  <name>Task 2: README quick-start blurb pointing to `make help`</name>
  <files>README.md</files>
  <read_first>
    - /Users/ashutosh/personal/rollout/README.md (existing; preserve any existing content — append a quick-start section if not present)
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-CONTEXT.md (D-LOCAL-01)
  </read_first>
  <action>
1. Read the existing `/Users/ashutosh/personal/rollout/README.md`.
2. If it does NOT already contain a `## Quick start` or `## Building` section that mentions `make help`, append (do not replace existing content) a section at the end:

```markdown
## Quick start (local dev)

All tasks go through the top-level `Makefile`:

```bash
make help            # list targets
make build           # cargo build --workspace
make lint            # cargo fmt --check + clippy -D warnings
make test            # cargo test --workspace --tests
make check           # lint + test
make schema-gen      # regenerate schemas/ + python stubs
make validate-schema # meta-validate the JSON Schema (requires `pip install check-jsonschema`)
make docs            # mdbook build + cargo doc (requires `cargo install mdbook --locked`)
```

Requires only `cargo` (pinned to `1.88.0` via `rust-toolchain.toml`) and `make`. `make docs` additionally requires `mdbook` (`cargo install mdbook --locked --version 0.4.x`).
```

3. If the README already mentions `make help`, do nothing for that section (idempotent) — but still ensure the file ends with a newline.
  </action>
  <acceptance_criteria>
    - `test -f /Users/ashutosh/personal/rollout/README.md`
    - `grep -q 'make help' /Users/ashutosh/personal/rollout/README.md`
    - `grep -q 'make build' /Users/ashutosh/personal/rollout/README.md`
    - `grep -q 'make test' /Users/ashutosh/personal/rollout/README.md`
    - `grep -q 'make schema-gen' /Users/ashutosh/personal/rollout/README.md`
    - `grep -q 'make docs' /Users/ashutosh/personal/rollout/README.md`
    - `grep -q 'rust-toolchain.toml' /Users/ashutosh/personal/rollout/README.md`
  </acceptance_criteria>
  <verify>
    <automated>grep -q 'make help' /Users/ashutosh/personal/rollout/README.md && grep -q 'make schema-gen' /Users/ashutosh/personal/rollout/README.md && grep -q 'make docs' /Users/ashutosh/personal/rollout/README.md</automated>
  </verify>
  <done>README quick-start section in place, pointing humans to `make help` as the canonical entry. No clobbering of existing content. `make docs` is documented with its mdbook prerequisite.</done>
</task>

<task type="auto" tdd="false">
  <name>Task 3: Declare graphify-ts dev dependency (root package.json + .gitignore)</name>
  <files>package.json, .gitignore</files>
  <read_first>
    - /Users/ashutosh/personal/rollout/.planning/phases/01-core-foundations/01-CONTEXT.md (D-GRAPHIFY-01)
    - /Users/ashutosh/personal/rollout/AGENTS.md §9.6
    - /Users/ashutosh/personal/rollout/.planning/config.json (tools.graphify block)
    - existing /Users/ashutosh/personal/rollout/package.json (may already exist; verify with `cat` first)
    - existing /Users/ashutosh/personal/rollout/.gitignore (preserve existing content)
  </read_first>
  <action>
1. If `/Users/ashutosh/personal/rollout/package.json` does not exist, create it with EXACT content:

```json
{
  "name": "rollout-dev-tools",
  "private": true,
  "version": "0.0.0",
  "description": "Dev tooling shell for the rollout repo. Not published. Hosts dev-only Node tools (graphify, etc.); the actual project is the Rust workspace under crates/.",
  "scripts": {
    "graphify": "graphify-ts",
    "graphify:build": "graphify-ts build",
    "graphify:watch": "graphify-ts watch"
  },
  "devDependencies": {
    "@mohammednagy/graphify-ts": "^0.22.9"
  }
}
```

If it already exists, ensure it contains `@mohammednagy/graphify-ts` in `devDependencies` at `^0.22.9` (or newer compatible) and the `graphify` script; do not remove existing fields.

2. Append (idempotent — skip if lines already present) to `.gitignore`:

```gitignore

# Node dev tooling (graphify-ts, etc.)
node_modules/
graphify-out/
*.tsbuildinfo
```

3. Run `npm install` at the repo root to materialize `node_modules/.bin/graphify-ts`.

4. Smoke: `npx graphify-ts --help` exits 0 with output starting with `Usage: graphify-ts`.

Do NOT commit `node_modules/` or `graphify-out/`. Do NOT add graphify to a CI gate in Phase 1 (per D-GRAPHIFY-01 — local dev tool only).
  </action>
  <acceptance_criteria>
    - `test -f /Users/ashutosh/personal/rollout/package.json`
    - `grep -q '"name": "rollout-dev-tools"' /Users/ashutosh/personal/rollout/package.json`
    - `grep -q '"private": true' /Users/ashutosh/personal/rollout/package.json`
    - `grep -q '@mohammednagy/graphify-ts' /Users/ashutosh/personal/rollout/package.json`
    - `grep -q '^node_modules/$' /Users/ashutosh/personal/rollout/.gitignore`
    - `grep -q '^graphify-out/$' /Users/ashutosh/personal/rollout/.gitignore`
    - `grep -q '^\*.tsbuildinfo$' /Users/ashutosh/personal/rollout/.gitignore`
    - `test -x /Users/ashutosh/personal/rollout/node_modules/.bin/graphify-ts`
  </acceptance_criteria>
  <verify>
    <automated>cd /Users/ashutosh/personal/rollout && test -f package.json && grep -q '@mohammednagy/graphify-ts' package.json && grep -q '^node_modules/$' .gitignore && grep -q '^graphify-out/$' .gitignore && test -x node_modules/.bin/graphify-ts && ./node_modules/.bin/graphify-ts --help 2>&1 | head -1 | grep -q 'graphify-ts'</automated>
  </verify>
  <done>Root `package.json` declares `@mohammednagy/graphify-ts` as a dev dependency, `.gitignore` excludes `node_modules/` + `graphify-out/`, and the binary resolves at `node_modules/.bin/graphify-ts`. `make graphify` (added in Task 1) is now runnable from a clean checkout after `npm install`.</done>
</task>

</tasks>

<verification>
- `make -n` parses every Phony target without error.
- README documents `make help` as the entrypoint and `make docs` with its mdbook prereq.
- Functional CI integration (jobs that call these targets) lands in Plan 06.
</verification>

<success_criteria>
- `Makefile` exists with `.PHONY` and all 9 targets (`lint`, `test`, `build`, `check`, `schema-gen`, `validate-schema`, `docs`, `graphify`, `help`).
- D-LOCAL-02 exact command bodies preserved.
- `docs` target body matches AGENTS.md §9.1.
- `graphify` target body matches D-GRAPHIFY-01 and AGENTS.md §9.6.
- Root `package.json` declares `@mohammednagy/graphify-ts` in `devDependencies`; `.gitignore` excludes `node_modules/` + `graphify-out/`.
- README quick-start references `make help` and the major targets including `make docs`.
</success_criteria>

<output>
After completion, create `.planning/phases/01-core-foundations/01-PLAN-02-SUMMARY.md` documenting the final target list and any deviations from the locked command bodies (expected: none).
</output>
