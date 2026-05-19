# Phase 1: Core foundations — Context

**Gathered:** 2026-05-19
**Last amended:** 2026-05-19 (added DOCS-01..03 + v1-example commitment)
**Status:** Ready for planning
**Source:** Synthesized from user inline input + ROADMAP.md + AGENTS.md + ARCHITECTURE.md + docs/specs/

<domain>
## Phase Boundary

Phase 1 establishes the **inner ring** of the workspace that everything else builds on:

- The `rollout-core` crate — trait surface, ID types, error taxonomy, config types (Serialize + Deserialize + JsonSchema). Zero runtime deps beyond `serde`, `thiserror`, `schemars`, `tracing`, `ulid`, `blake3`. **No** `tokio`, `aws-sdk-*`, `pyo3`.
- The Cargo workspace skeleton + workspace `Cargo.toml` + `deny.toml` + `rust-toolchain.toml`.
- A schema-generation pipeline: `cargo xtask schema-gen` regenerates `schemas/rollout.schema.json` + `python/rollout/_config_stubs.pyi`; CI fails on drift.
- A `rollout schema --format json` CLI surface (minimal `rollout-cli` shim — the rest of the CLI is later phases).
- A dependency-direction lint via `cargo deny` (algo crates may not depend on cloud crates; cloud SDKs only inside `rollout-cloud-*`).
- **Local dev ergonomics:** `Makefile` with `lint` / `test` / `build` / `check` / `schema-gen` / **`docs`** targets, runnable on a clean checkout with only `cargo` + `make` (+ `mdbook` for `make docs`) installed.
- **Remote CI:** GitHub Actions at the level of `../vector` — multi-job (lint, test, deny, commitlint, schema-drift, architecture-lint, **docs-build, docs-deploy, rustdoc-check, docs-test-policy**), branch-protection-grade, with caching via `Swatinem/rust-cache@v2`.
- **Docs site bootstrap (DOCS-01):** mdBook skeleton under `docs/book/` (`book.toml` + `src/SUMMARY.md` + minimal landing page) wired to a GitHub Actions workflow that builds on every PR and **deploys to GitHub Pages on every push to `main`**. Bootstrap is real (the site renders and deploys); content is filled by later phases.
- **Per-commit doc/test policy (DOCS-02):** a CI check inspects the changed-file set of each PR and fails if a code-only diff (touching `crates/`, `python/`, or `xtask/`) lands without an accompanying change under `docs/` or `**/tests/` or inline doc comments. Commit-trailer `[skip-docs-check]` provides a controlled escape hatch for bootstrap.
- **Rustdoc gate (DOCS-03):** `cargo doc --workspace --no-deps --all-features` runs in CI with `RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs"`.

**Out of scope (explicit):** any storage backend, transport, plugin host, inference backend, training crate, distribution code, cloud-specific code, or harness. Only **traits + types + config + workspace plumbing + CI/Makefile + docs-site bootstrap + per-commit-policy CI check**.

**Forward-looking commitment (not in Phase 1 scope, but recorded for downstream agents):**
- **v1 ships with a working model example (SHIP-03 hardened).** The v1 release gate requires at least one end-to-end recipe (`make example` / `cargo run --example`) that runs SFT or PPO on a real small open-weights model, completes on commodity hardware, is exercised by nightly CI, and is documented on the docs site. Phase 1 does not ship the recipe, but Phase 1's docs-site bootstrap MUST reserve a `docs/book/src/examples/` directory and a `SUMMARY.md` placeholder entry for that recipe so subsequent phases just fill it in.

</domain>

<decisions>
## Implementation Decisions

### User-locked (from inline input)

- **D-LOCAL-01** — A top-level `Makefile` exists from day one with at minimum `lint`, `test`, `build`, `check`, and `schema-gen` (or `schema`) targets. Targets are the entry points humans and CI both call. `.PHONY` declared.
- **D-LOCAL-02** — `make lint` runs `cargo fmt --all -- --check` and `cargo clippy --all-targets --all-features -- -D warnings`. `make test` runs `cargo test --workspace --tests`. Both must pass on a clean checkout with only `cargo` + `make` installed.
- **D-CI-01** — GitHub Actions parity with `/Users/ashutosh/personal/vector/.github/workflows/ci.yml` at minimum: separate `lint`, `test`, `deny`, `commitlint` jobs; `Swatinem/rust-cache@v2`; `dtolnay/rust-toolchain` pinned. Runs on `pull_request` and `push` to `main`. macos-14 acceptable as primary runner (matches vector); add `ubuntu-latest` for `deny` and Linux-only checks since rollout is a server/Linux framework not a desktop app.
- **D-CI-02** — Architecture/dependency-direction lint runs in CI and fails the build on violation (mirrors vector's "Architecture-lint" job).
- **D-CI-03** — Schema-drift CI job: regenerate JSON Schema + Python stubs, fail if `git diff` is non-empty.
- **D-CI-04** — Conventional-commits lint via `convco check` on PRs (mirrors vector's `commitlint` job). Tolerant on direct main pushes for bootstrap.
- **D-DOCS-01** — A docs website exists from day one. Toolchain: **mdBook** for narrative + `cargo doc` for API reference, surfaced via the mdBook site (either embedded or cross-linked). Layout: `docs/book/book.toml`, `docs/book/src/SUMMARY.md`, `docs/book/src/introduction.md`, `docs/book/src/architecture.md` (link/include from `ARCHITECTURE.md`), `docs/book/src/examples/.gitkeep` + a stubbed `examples/index.md` (reserved for the v1 working-model recipe per SHIP-03). Build target: `make docs` runs `mdbook build docs/book` and `cargo doc --workspace --no-deps`.
- **D-DOCS-02** — GitHub Actions workflow `.github/workflows/docs.yml` (or a `docs-build` + `docs-deploy` job within `ci.yml`) that: (a) on every PR — builds the book + rustdoc as a required check, uploads the rendered site as an artifact; (b) on push to `main` — deploys to GitHub Pages via `actions/deploy-pages` and `actions/upload-pages-artifact`. The `pages` permission on `GITHUB_TOKEN` must be enabled.
- **D-DOCS-03** — Per-commit doc/test policy enforced by a CI script `scripts/check-docs-tests-touched.sh` (or equivalent) that reads `git diff --name-only origin/${{ github.base_ref }}...HEAD` and fails if any file under `crates/`, `python/`, or `xtask/` is modified without a sibling change to `docs/`, a `tests/` directory, or inline rust/python doc comments in the same diff. Recognize a `[skip-docs-check]` trailer in `git log -1 --format=%B HEAD` as a bypass. Run as a CI job `docs-test-policy`.
- **D-DOCS-04** — Rustdoc gate: a CI job `rustdoc-check` runs `RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" cargo doc --workspace --no-deps --all-features`. Phase 1 crates (`rollout-core`, `rollout-cli`, `xtask`) must each have a crate-level `//!` doc comment.

### Roadmap-locked

- **D-CRATE-01** — Crate is named `rollout-core`. Path: `crates/rollout-core/`. License: MIT (matches repo `LICENSE`). Edition 2021.
- **D-CRATE-02** — `rollout-core` exposes the trait surface defined in REQ CORE-01 (full list): `PolicyAlgorithm`, `Worker`, `Coordinator`, `Scheduler`, `Plugin`, `EnvHarness`, `ToolHarness`, `EvalHarness`, `RewardModel`, `InferenceBackend`, `Storage`, `StorageTxn`, `Queue`, `ObjectStore`, `SecretStore`, `ComputeHint`, `Snapshotter`, `PluginHost`, `Clock`. Each trait lives in a module file under `src/traits/`. No concrete impls in this phase.
- **D-ERR-01** — Error taxonomy per REQ CORE-03: `CoreError = Recoverable { Throttled, Transient, Preempted } | Fatal { ConfigInvalid, SchemaViolation, PluginContract, Internal }`, each variant carries a `RetryHint`. Single `CoreError` type with two enum sub-variants. Derived via `thiserror`.
- **D-ID-01** — IDs per REQ CORE-05: `RunId(Ulid)`, `WorkerId(Ulid)`, `ContentId([u8; 32])` (blake3). All implement `Serialize + Deserialize + Display + FromStr`. ULIDs use the `ulid` crate; content hashes use the `blake3` crate.
- **D-CFG-01** — Config types use `#[derive(Serialize, Deserialize, JsonSchema)]` + `#[serde(deny_unknown_fields)]` per spec 11. Top-level type is `RunConfig` with `schema_version: u32` (range 1..=1 in v1).
- **D-CFG-02** — `cargo xtask schema-gen` is a workspace `xtask` crate (binary). Outputs: `schemas/rollout.schema.json` + `python/rollout/_config_stubs.pyi` + `docs/schema-reference.md` (the docs file may be a placeholder header in this phase). A workspace-level test asserts no drift between committed artifacts and freshly regenerated ones.
- **D-CLI-01** — `rollout-cli` crate exists as a binary with **only one** working subcommand in this phase: `rollout schema --format json` (and `--format pretty`). Everything else can `unimplemented!()` or return "not yet implemented". This satisfies the exit criterion "`rollout schema --format json` emits a JSON Schema validated by an external validator."
- **D-DENY-01** — `deny.toml` at workspace root configures `cargo deny` for `advisories`, `licenses`, `bans`, `sources`. License allowlist mirrors vector's (`Apache-2.0`, `MIT`, BSD variants, ISC, Unicode-3.0, CC0-1.0, Zlib, 0BSD, MPL-2.0, CDLA-Permissive-2.0). Bans: `openssl`, `openssl-sys` (use rustls when TLS arrives in later phases).
- **D-LINT-01** — Dependency-direction enforcement: a `tests/architecture.rs` integration test in `rollout-core` (or a dedicated `rollout-arch-lint` xtask) that parses `Cargo.toml` files and asserts no algorithm/Layer-3+ crate depends on a cloud/Layer-1 crate. Mirrors vector's "Architecture-lint per-crate tests" idea. Exit criterion: "Dependency-boundary lint enforced in CI; deliberate violation fails the build."
- **D-V1-EXAMPLE** — *(Forward-looking; not built in Phase 1)* The v1 release ships with at least one end-to-end working model example. Phase 1's only obligation is to **reserve the surface**: `docs/book/src/examples/index.md` placeholder + a `[skip-docs-check]`-exempt commit recording the future contract. Implementation lands progressively (Phase 4 stub → Phase 9 real recipe → Phase 12 polished docs).
- **D-GRAPHIFY-01** — `@mohammednagy/graphify-ts` is declared as a dev dependency in a root `package.json`. The package was installed in the planning session (`node_modules/.bin/graphify-ts` resolves). `.planning/config.json` carries `workflow.graphify: true` and a `tools.graphify` block. `graphify-out/` + `node_modules/` are gitignored. Phase 1 must: (a) commit the root `package.json` + the `.gitignore` additions; (b) add a `make graphify` target wrapping `npx graphify-ts generate . --directed --svg`; (c) record the standing rule in `AGENTS.md` §9.6 (already done). No CI integration required in Phase 1 — graphify is a local dev tool, not a release gate.

### Claude's Discretion

- Naming of `xtask` subcommands beyond `schema-gen` (e.g., `xtask check-deps`, `xtask deny`).
- Whether dependency-boundary lint lives in `xtask` vs an integration test (both acceptable; pick one).
- Specific Rust toolchain version: pin to a recent stable (≥ 1.85.0, no nightly features). `rust-toolchain.toml` pins it for both local and CI.
- Whether traits are `async` (likely yes for I/O traits — Storage/Queue/ObjectStore/Coordinator) using `async_trait` or `trait async fn` (stable since 1.75). Choose based on minimum supported toolchain.
- Module structure inside `rollout-core/src/` (e.g., `traits/mod.rs`, `errors.rs`, `ids.rs`, `config/mod.rs`, `events.rs`).
- Whether `tracing` is re-exported from `rollout-core` (recommended, single version pin for the workspace).
- Specific shape of `RetryHint` (likely `enum RetryHint { Never, After(Duration), Backoff { base: Duration, max: Duration } }`).
- Whether to add `cargo machete` (unused-deps lint) in CI now or later — recommend now, mirrors vector.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Roadmap & requirements
- `ROADMAP.md` — narrative Phase 1 goal, includes, and exit criteria (authoritative for scope)
- `.planning/REQUIREMENTS.md` — REQ-IDs CORE-01..CORE-05 (authoritative for what must ship)
- `.planning/ROADMAP.md` — phase → requirement mapping

### Architectural source-of-truth
- `AGENTS.md` — the 10 north-star principles (especially #4 single source of truth, #9 layered cloud abstraction, #10 observability not optional)
- `ARCHITECTURE.md` §1 (layered architecture), §2 (Layer 0 contents)
- `docs/specs/00-overview.md` — index of all 11 specs
- `docs/specs/01-core-runtime.md` — Worker, Coordinator, WorkerContext, Heartbeat, lifecycle state machine
- `docs/specs/02-algorithms.md` — `PolicyAlgorithm` trait shape
- `docs/specs/03-plugin-system.md` — `Plugin`, `PluginHost` traits
- `docs/specs/04-storage-snapshots.md` — `Storage`, `StorageTxn`, `Snapshotter` traits
- `docs/specs/05-distribution.md` — `Scheduler`, `Queue` traits
- `docs/specs/06-cloud-layer.md` — `ObjectStore`, `SecretStore`, `ComputeHint` traits
- `docs/specs/07-harnesses.md` — `EnvHarness`, `ToolHarness`, `EvalHarness` traits
- `docs/specs/08-cli.md` — `rollout` CLI surface (this phase only implements `schema`)
- `docs/specs/09-observability.md` — `EventEmitter`, structured event shape
- `docs/specs/10-component-split.md` — crate publishing strategy, dependency-direction rule (§8)
- `docs/specs/11-config-schema.md` — **the** spec for the schema-gen pipeline (single source of truth rule)
- `docs/design-principles.md` §9 — dependency-direction rule

### CI / dev ergonomics reference (external repo, read-only)
- `/Users/ashutosh/personal/vector/Makefile` — reference shape for top-level Makefile targets
- `/Users/ashutosh/personal/vector/.github/workflows/ci.yml` — reference shape for multi-job CI
- `/Users/ashutosh/personal/vector/.github/workflows/dev-build.yml` — reference for dev-build job
- `/Users/ashutosh/personal/vector/deny.toml` — reference shape for `cargo deny` config

### Repo state
- `crates/README.md` — declared crate layout (currently empty; Phase 1 populates `rollout-core` + scaffolds the rest)
- `SKILLS.md` — repo skills (informational)

</canonical_refs>

<specifics>
## Specific Ideas

- **CORE-01 trait list is exhaustive.** All 19 named traits must compile in `rollout-core` after Phase 1, even if some are skeletal (one or two methods) — the surface is the contract for Phase 2+.
- **Schema test is the proof.** `rollout schema --format json` emitting a doc that an *external* validator (e.g., `ajv` via a script, or `jsonschema` CLI in Python) accepts is the exit criterion. Build a shell or Python script in `scripts/` that runs the validator; wire it into the Makefile and CI.
- **Deliberate-violation test.** A `tests/dependency-direction.rs` (or equivalent) must fail when a fixture Cargo.toml introduces an illegal dependency. Mirrors vector's "Architecture-lint" pattern of a per-crate failing test.
- **JSON Schema output stable.** Set a stable ordering (sorted keys) so CI diff comparisons are deterministic.
- **Python stub generation** can be lightweight in Phase 1 — a single `_config_stubs.pyi` with one or two top-level types. Real Python bindings come later; the contract here is "the codegen pipeline exists and CI enforces drift."

</specifics>

<deferred>
## Deferred Ideas

- Actual `Storage`/`Queue`/`ObjectStore`/etc. implementations (Phases 2, 4, 5).
- Real CLI subcommands beyond `schema` (Phases 3, 5, 6, 8).
- `pip install rollout` packaging (Phase 12 — SHIP-02).
- `docs.rs` + mdBook docs build (Phase 12 — SHIP-04).
- Multi-platform CI matrix beyond `macos-14` + `ubuntu-latest` (e.g., Windows, aarch64-linux) — Phase 12.
- Release workflow (`v*` tag → crates.io publish) — Phase 12 — SHIP-01.
- DMG / app-bundle packaging — never (rollout is not a desktop app).
- Tmux/clipboard CI smoke tests from vector — N/A.

</deferred>

---

*Phase: 01-core-foundations*
*Context gathered: 2026-05-19 via plan-phase --research with synthesized inline-input context*
