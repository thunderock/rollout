---
phase: 03-inference-batch
plan: 00
type: execute
wave: 1
depends_on: []
files_modified:
  - crates/rollout-core/src/traits/backend.rs
  - crates/rollout-core/src/traits/worker.rs
  - crates/rollout-core/src/traits/mod.rs
  - crates/rollout-core/src/config/mod.rs
  - crates/rollout-core/src/lib.rs
  - crates/rollout-core/Cargo.toml
  - crates/rollout-core/tests/trait_surface.rs
  - crates/rollout-core/tests/sampling_params_postcard.rs
  - crates/rollout-core/tests/dependency_direction.rs
  - crates/rollout-core/tests/fixtures/violation_backend_uses_cloud/Cargo.toml
  - crates/rollout-core/tests/fixtures/violation_backend_uses_cloud/src/lib.rs
  - crates/rollout-core/tests/fixtures/violation_backend_uses_transport/Cargo.toml
  - crates/rollout-core/tests/fixtures/violation_backend_uses_transport/src/lib.rs
  - crates/rollout-backend-vllm/Cargo.toml
  - crates/rollout-backend-vllm/src/lib.rs
  - crates/rollout-backend-vllm/benches/throughput.rs
  - crates/rollout-runtime-batch/Cargo.toml
  - crates/rollout-runtime-batch/src/lib.rs
  - Cargo.toml
  - docs/specs/01-core-runtime.md
  - docs/specs/02-algorithms.md
  - docs/specs/08-cli.md
  - docs/book/src/SUMMARY.md
  - docs/book/src/inference/index.md
autonomous: true
requirements: [BACKEND-01, BACKEND-02, DOCS-01, DOCS-02, DOCS-03]
must_haves:
  truths:
    - "rollout-core exposes the Phase-3 InferenceBackend surface (init/generate/model_id/shutdown) with SamplingParams + ModelRef + Prompt + Completion types."
    - "WorkerRole enum lands with BatchInference/BatchReader/BatchWriter/Coordinator/Custom variants."
    - "The two new crates (rollout-backend-vllm, rollout-runtime-batch) are registered workspace members with crate-level //! docs."
    - "Dep-direction lint forbids rollout-backend-vllm depending on any rollout-cloud-* crate (#5) or rollout-transport (#6)."
    - "criterion 0.5 is pinned in [workspace.dependencies]; pyo3 0.28 + pyo3-async-runtimes 0.28 already pinned from Phase 2 are reused."
    - "schema-gen regenerates cleanly with the new SamplingParams / ModelRef / WorkerRole types serialised."
  artifacts:
    - path: crates/rollout-core/src/traits/backend.rs
      provides: "Extended InferenceBackend trait + SamplingParams + ModelRef + Prompt + Completion newtypes"
      contains: "trait InferenceBackend"
    - path: crates/rollout-core/src/traits/worker.rs
      provides: "WorkerRole enum"
      contains: "enum WorkerRole"
    - path: crates/rollout-backend-vllm/Cargo.toml
      provides: "Skeleton crate with `vllm` feature gate"
      contains: "name = \"rollout-backend-vllm\""
    - path: crates/rollout-runtime-batch/Cargo.toml
      provides: "Skeleton runtime-glue crate"
      contains: "name = \"rollout-runtime-batch\""
    - path: crates/rollout-core/tests/dependency_direction.rs
      provides: "Invariants #5 (backend ↛ cloud) and #6 (backend ↛ transport)"
      contains: "violation_backend_uses_cloud"
    - path: Cargo.toml
      provides: "Two new workspace members + criterion 0.5 workspace dep"
      contains: "rollout-backend-vllm"
  key_links:
    - from: crates/rollout-core/src/lib.rs
      to: "traits::backend, traits::worker"
      via: "pub use"
      pattern: "SamplingParams|ModelRef|WorkerRole|InferenceBackend"
    - from: Cargo.toml
      to: "two new crates"
      via: "[workspace] members"
      pattern: "rollout-(backend-vllm|runtime-batch)"
    - from: crates/rollout-core/tests/dependency_direction.rs
      to: "BACKEND_CRATES const"
      via: "violation rules"
      pattern: "rollout-backend-vllm"
---

<objective>
Wave-0 trait surgery + new-crate registration so every Wave-1 stream compiles without trait churn. This plan:

1. Extends `rollout-core::InferenceBackend` to the Phase-3 surface (init/generate/model_id/shutdown) with `SamplingParams`, `ModelRef`, `Prompt`, `Completion` newtypes.
2. Adds the `WorkerRole` enum to `rollout-core::traits::worker`.
3. Registers two new crates as workspace members: `rollout-backend-vllm` (Layer-2 backend; depends on rollout-core + pyo3 only) and `rollout-runtime-batch` (Layer-3 glue; depends on rollout-core + rollout-storage + rollout-cloud-local).
4. Adds dep-direction invariants #5 (backend ↛ cloud) and #6 (backend ↛ transport) with fixture-based negative tests.
5. Pins `criterion = "0.5"` in `[workspace.dependencies]`.
6. Updates specs 01 / 02 / 08 with `## Na. Phase 3 implementation notes` sections per AGENTS.md §4.

Plan is split across **two sequential tasks** to keep each task's blast radius under the 15-file blocker threshold (per checker BLOCKER 7). Both tasks live in Wave 1; Task 2 strictly depends on Task 1.

Purpose: lock the trait surface + workspace topology before Wave 1 splits into the two parallel skeleton streams.
Output: extended traits, two crate skeletons, extended dep-lint, criterion pin, spec edits, mdBook inference landing page.
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
@$HOME/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/STATE.md
@.planning/ROADMAP.md
@.planning/REQUIREMENTS.md
@.planning/phases/03-inference-batch/03-CONTEXT.md
@.planning/phases/03-inference-batch/03-RESEARCH.md
@.planning/phases/03-inference-batch/03-VALIDATION.md
@.planning/phases/02-local-substrate/02-00-wave0-trait-extensions-SUMMARY.md
@AGENTS.md
@docs/specs/01-core-runtime.md
@docs/specs/02-algorithms.md
@docs/specs/08-cli.md
@crates/rollout-core/src/traits/backend.rs
@crates/rollout-core/src/traits/worker.rs
@crates/rollout-core/src/config/mod.rs
@crates/rollout-core/tests/dependency_direction.rs
@Cargo.toml

<interfaces>
Current backend.rs (line 9-12):
```rust
#[async_trait]
pub trait InferenceBackend: Send + Sync {
    async fn generate(&self, prompts: &[String]) -> Result<Vec<String>, CoreError>;
}
```
Phase-3 target shape (from CONTEXT D-BACKEND-01 + RESEARCH §"Code Examples"):
```rust
pub struct Prompt(pub String);
pub struct Completion {
    pub text: String,
    pub finish_reason: String,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}
pub struct ModelRef {
    pub uri: String,                // HF repo id or local path
    pub tokenizer: Option<String>,  // optional override
}
#[non_exhaustive]
pub struct SamplingParams {
    pub temperature: f32,           // default 1.0
    pub top_p: f32,                 // default 1.0
    pub top_k: i32,                 // default -1
    pub max_tokens: u32,            // default 16
    pub seed: Option<u64>,
    pub stop: Vec<String>,          // default Vec::new() — NOT Option<Vec<_>> (RESEARCH Pitfall 4)
    pub stream: bool,               // Phase 3: must be false
}
#[async_trait]
pub trait InferenceBackend: Send + Sync {
    async fn init(&mut self, model: ModelRef) -> Result<(), CoreError>;
    async fn generate(&self, prompts: &[Prompt], params: &SamplingParams)
        -> Result<Vec<Completion>, CoreError>;
    fn model_id(&self) -> &ContentId;
    async fn shutdown(&mut self) -> Result<(), CoreError>;
}
```

WorkerRole (lifts from CONTEXT D-BACKEND-04):
```rust
pub enum WorkerRole {
    Coordinator,
    BatchInference,
    BatchReader,
    BatchWriter,
    Custom(smol_str::SmolStr),
}
```
</interfaces>
</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: Trait surface extension (rollout-core only)</name>
  <read_first>
    - crates/rollout-core/src/traits/backend.rs (current stub)
    - crates/rollout-core/src/traits/worker.rs (WorkerState already lives here)
    - crates/rollout-core/src/config/mod.rs (RunConfig shape)
    - crates/rollout-core/src/lib.rs (pub-use surface)
    - .planning/phases/02-local-substrate/02-00-wave0-trait-extensions-SUMMARY.md (exact pattern this mirrors)
    - .planning/phases/03-inference-batch/03-CONTEXT.md §"Decisions" D-BACKEND-01..05
    - .planning/phases/03-inference-batch/03-RESEARCH.md §"Code Examples" + §"Pitfalls 1 + 4"
  </read_first>
  <behavior>
    - Test 1: `cargo test -p rollout-core --test trait_surface` — compile-time proves `InferenceBackend: Send + Sync` with the new four-method shape + `Prompt`, `Completion`, `ModelRef`, `SamplingParams` types resolvable through `rollout_core::*`.
    - Test 2: `cargo test -p rollout-core --test trait_surface` — proves `WorkerRole::{Coordinator, BatchInference, BatchReader, BatchWriter, Custom(_)}` all construct.
    - Test 3: `cargo test -p rollout-core --test schema_drift` (already exists) — passes after regenerating schemas with `cargo xtask schema-gen`.
    - Test 4: `cargo test -p rollout-core --test sampling_params_postcard` — `postcard::to_stdvec(&SamplingParams::default())` round-trips byte-identically on two runs (RESEARCH Pitfall 1 — determinism).
    - Test 5: SamplingParams carries `#[non_exhaustive]` so external crates can't add fields without our version bump (RESEARCH Pitfall 1).
    - Test 6: SamplingParams with `stop: vec![]` and a serde-default-instantiated one produce byte-identical postcard output (RESEARCH Pitfall 4).
  </behavior>
  <action>
    Edit `crates/rollout-core/src/traits/backend.rs`:
    - Replace the current stub with the extended surface from `<interfaces>` above.
    - Add `Prompt(pub String)` and `Completion { text, finish_reason, prompt_tokens, completion_tokens }` newtypes with `Debug + Clone + Serialize + Deserialize + JsonSchema`.
    - Add `ModelRef { uri: String, tokenizer: Option<String> }` (Debug + Clone + Serialize + Deserialize + JsonSchema; `#[serde(deny_unknown_fields)]`).
    - Add `SamplingParams` per `<interfaces>` block above with `#[non_exhaustive]` + `#[serde(deny_unknown_fields)]` + `impl Default` returning `{ temperature: 1.0, top_p: 1.0, top_k: -1, max_tokens: 16, seed: None, stop: vec![], stream: false }`.
    - Trait method `model_id(&self) -> &ContentId` must be sync (not async) to avoid `&self` async lifetimes.
    - `shutdown` is `async fn shutdown(&mut self) -> Result<(), CoreError>` per RESEARCH §"Code Examples".

    Edit `crates/rollout-core/src/traits/worker.rs`:
    - Add `WorkerRole` enum after `WorkerState` per `<interfaces>` block. Derives: `Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema`. `#[serde(rename_all = "snake_case")]`. `Custom(SmolStr)` variant uses `smol_str::SmolStr` (workspace dep `=0.3.2` from Phase 2).

    Edit `crates/rollout-core/src/traits/mod.rs` + `src/lib.rs`:
    - `pub use traits::backend::{InferenceBackend, Prompt, Completion, ModelRef, SamplingParams};`
    - `pub use traits::worker::WorkerRole;`

    Edit `crates/rollout-core/Cargo.toml`: ensure `smol_str = { workspace = true }` is a dep (likely already is from Phase 2).

    Edit `crates/rollout-core/src/config/mod.rs`:
    - Re-export `SamplingParams` and `ModelRef` from `traits::backend` at the config module level (`pub use crate::traits::backend::{ModelRef, SamplingParams};`) so future config blocks can compose them.

    Extend `crates/rollout-core/tests/trait_surface.rs`: add `fn _proves_backend_shape()` that constructs zero values and asserts `&dyn InferenceBackend: Send + Sync`. Add `fn _proves_worker_role_variants()` that pattern-matches all five variants.

    Create `crates/rollout-core/tests/sampling_params_postcard.rs` (new file): runs `postcard::to_stdvec(&SamplingParams::default())` twice and `assert_eq!(a, b)`; also asserts the wire bytes do NOT change when `stop: vec![]` vs omitting (covers RESEARCH Pitfall 4 — a TOML config with `stop = []` and one without `stop` must hash identically because serde default is `vec![]`).

    Run `cargo xtask schema-gen` after the edits; commit any drift to `schemas/rollout.schema.json` + `python/rollout/_config_stubs.py` + `docs/schema-reference.md`.

    Spec edits per AGENTS.md §4 (append a `## Na. Phase 3 implementation notes` section to each, do NOT modify earlier sections):
    - `docs/specs/02-algorithms.md` — append `## 2a. Phase 3 implementation notes`. Note: extended `InferenceBackend` to four methods; `SamplingParams::stream` rejected at config-validate per D-BACKEND-03; `SamplingParams` is `#[non_exhaustive]` + carries a `SAMPLING_PARAMS_SCHEMA_VERSION` constant prepended to sample-ID hashing (the constant lives in `rollout-runtime-batch`; consumed by sample-ID derivation).
    - `docs/specs/01-core-runtime.md` — append `## 3a. Phase 3 implementation notes`. Note: `WorkerRole` enum added; Phase 3 wires `BatchInference` only.
  </action>
  <verify>
    <automated>cargo test -p rollout-core --tests 2>&amp;1 | grep -E "test result.*ok" &amp;&amp; cargo xtask schema-gen --out-dir /tmp/schema-gen-check &amp;&amp; cargo clippy -p rollout-core --all-targets -- -D warnings</automated>
  </verify>
  <acceptance_criteria>
    - `grep -q 'trait InferenceBackend' crates/rollout-core/src/traits/backend.rs`
    - `grep -q 'pub struct SamplingParams' crates/rollout-core/src/traits/backend.rs`
    - `grep -q '#\[non_exhaustive\]' crates/rollout-core/src/traits/backend.rs`
    - `grep -q 'pub enum WorkerRole' crates/rollout-core/src/traits/worker.rs`
    - `grep -q 'BatchInference' crates/rollout-core/src/traits/worker.rs`
    - `test -f crates/rollout-core/tests/sampling_params_postcard.rs`
    - `grep -q '## 2a. Phase 3 implementation notes' docs/specs/02-algorithms.md`
    - `grep -q '## 3a. Phase 3 implementation notes' docs/specs/01-core-runtime.md`
    - `cargo test -p rollout-core --tests` exits 0
    - `cargo clippy -p rollout-core --all-targets -- -D warnings` exits 0
  </acceptance_criteria>
  <done>
    Trait extension done in `rollout-core`. SamplingParams `#[non_exhaustive]` + postcard determinism proven. `WorkerRole` enum lands. Spec edits in place. No new crates yet — that is Task 2.
  </done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: Crate skeletons + dep-direction lint + workspace registration + mdBook landing page</name>
  <read_first>
    - Task 1 output: `crates/rollout-core/src/traits/backend.rs` (extended surface — the new crates depend on it)
    - .planning/phases/02-local-substrate/02-00-wave0-trait-extensions-SUMMARY.md (crate-skeleton pattern this mirrors)
    - .planning/phases/03-inference-batch/03-CONTEXT.md §"Decisions" D-VLLM-*
    - Cargo.toml (workspace members + dependencies)
    - crates/rollout-core/tests/dependency_direction.rs (existing 4 invariants; we add #5/#6)
    - docs/book/src/SUMMARY.md (current shape)
  </read_first>
  <behavior>
    - Test 1: `cargo build -p rollout-backend-vllm` succeeds with default features (no vllm).
    - Test 2: `cargo build -p rollout-runtime-batch` succeeds.
    - Test 3: `cargo test -p rollout-core --test dependency_direction` passes including the two new negative tests (`backend_must_not_depend_on_cloud`, `backend_must_not_depend_on_transport`) — fixture TOMLs trigger violations as expected.
    - Test 4: `mdbook build docs/book` succeeds; the new `inference/index.md` landing page is reachable from `SUMMARY.md`.
  </behavior>
  <action>
    Create skeleton crates:

    `crates/rollout-backend-vllm/Cargo.toml`:
    ```toml
    [package]
    name = "rollout-backend-vllm"
    version.workspace = true
    edition.workspace = true
    license.workspace = true
    description = "vLLM-backed InferenceBackend (PyO3 in-process)"

    [lints]
    workspace = true

    [features]
    default = []
    vllm    = []   # gates live PyO3 init path; tests use ROLLOUT_VLLM_AVAILABLE=1

    [dependencies]
    rollout-core         = { path = "../rollout-core", version = "0.1" }
    pyo3                 = { workspace = true }
    pyo3-async-runtimes  = { workspace = true }
    async-trait          = { workspace = true }
    serde                = { workspace = true }
    serde_json           = { workspace = true }
    thiserror            = { workspace = true }
    tokio                = { workspace = true }
    tracing              = { workspace = true }
    blake3               = { workspace = true }
    postcard             = { workspace = true }

    [dev-dependencies]
    criterion            = { workspace = true }
    tempfile             = { workspace = true }

    [[bench]]
    name = "throughput"
    harness = false
    ```

    `crates/rollout-backend-vllm/src/lib.rs`:
    ```rust
    //! `rollout-backend-vllm` — vLLM-backed `InferenceBackend` impl via PyO3 in-process.
    //!
    //! Phase 3 surface is inference-only (D-BACKEND-01). Training-mode forward/backward
    //! is Phase 4. The `vllm` Cargo feature gates the live PyO3 init path; tests are
    //! `#[ignore]`'d unless `ROLLOUT_VLLM_AVAILABLE=1` (D-VLLM-03).
    ```

    `crates/rollout-backend-vllm/benches/throughput.rs` — minimal stub with `criterion_group!/criterion_main!` so Cargo's `[[bench]]` resolves (real impl arrives Wave 2):
    ```rust
    use criterion::{criterion_group, criterion_main, Criterion};
    fn placeholder(c: &mut Criterion) { c.bench_function("placeholder", |b| b.iter(|| 1 + 1)); }
    criterion_group!(benches, placeholder);
    criterion_main!(benches);
    ```

    `crates/rollout-runtime-batch/Cargo.toml`:
    ```toml
    [package]
    name = "rollout-runtime-batch"
    version.workspace = true
    edition.workspace = true
    license.workspace = true
    description = "Batch-inference runtime glue: queue management + CAS sample-state machine"

    [lints]
    workspace = true

    [features]
    default            = []
    test-mock-backend  = []

    [dependencies]
    rollout-core         = { path = "../rollout-core", version = "0.1" }
    rollout-storage      = { path = "../rollout-storage", version = "0.1" }
    rollout-cloud-local  = { path = "../rollout-cloud-local", version = "0.1" }
    async-trait          = { workspace = true }
    serde                = { workspace = true }
    serde_json           = { workspace = true }
    thiserror            = { workspace = true }
    tokio                = { workspace = true }
    tracing              = { workspace = true }
    blake3               = { workspace = true }
    postcard             = { workspace = true }
    ulid                 = { workspace = true }
    smol_str             = { workspace = true }
    toml                 = { workspace = true }

    [dev-dependencies]
    tempfile             = { workspace = true }
    proptest             = { workspace = true }
    ```

    `crates/rollout-runtime-batch/src/lib.rs`:
    ```rust
    //! `rollout-runtime-batch` — coordinator + worker glue for `rollout infer batch`.
    //!
    //! Owns the CAS sample-state machine (`infer/<run_id>/samples/*`), queue
    //! enqueue/dequeue against `rollout-cloud-local::InMemQueue`, JSONL I/O,
    //! the `InferBatchConfig` TOML schema (`src/config.rs`), and the
    //! `MockBackend` (gated by `test-mock-backend`) used by deterministic
    //! resume integration tests (RESEARCH §"Restart-resume test design").
    //!
    //! Crate split rationale: keeps `rollout-backend-vllm` cloud-agnostic
    //! (spec 10 + dep-direction invariants #5/#6).
    ```

    Update `Cargo.toml` workspace root:
    - Add `"crates/rollout-backend-vllm"` and `"crates/rollout-runtime-batch"` to `[workspace] members`.
    - Add to `[workspace.dependencies]`:
      ```toml
      criterion = { version = "0.5", features = ["async_tokio"] }
      ```
    - Verify `toml = "..."` already in `[workspace.dependencies]` (it is from Phase 1 CLI config loading); if not, pin `toml = "0.8"`.

    Extend `crates/rollout-core/tests/dependency_direction.rs`:
    - Add `const BACKEND_CRATES: &[&str] = &["rollout-backend-vllm"];`
    - Add `fn violation_backend_uses_cloud(pkg, dep) -> bool { BACKEND_CRATES.contains(&pkg) && CLOUD_CRATES.contains(&dep) }`
    - Add `fn violation_backend_uses_transport(pkg, dep) -> bool { BACKEND_CRATES.contains(&pkg) && dep == "rollout-transport" }`
    - OR them into `any_violation`.
    - Create fixture dirs `crates/rollout-core/tests/fixtures/violation_backend_uses_cloud/Cargo.toml` (+ `src/lib.rs` stub) and `crates/rollout-core/tests/fixtures/violation_backend_uses_transport/Cargo.toml` (+ `src/lib.rs` stub) mirroring the existing `violation/Cargo.toml` pattern (hand-rolled, not a workspace member). The first sets `name = "rollout-backend-vllm"` with `rollout-cloud-local = { path = "../../../../rollout-cloud-local" }`; the second swaps the dep for `rollout-transport`.
    - Add load-bearing negative tests `backend_must_not_depend_on_cloud()` and `backend_must_not_depend_on_transport()` that parse the fixture TOMLs (using the existing hand-rolled `toml_pkg_name`/`toml_dep_names` helpers) and assert `any_violation(...)` returns `true`.

    Create `docs/book/src/inference/index.md` (landing page; one paragraph + a TODO note that per-component chapters land in plan 03-05). Append it to `docs/book/src/SUMMARY.md` under a new `# Inference` heading.
  </action>
  <verify>
    <automated>cargo build -p rollout-backend-vllm &amp;&amp; cargo build -p rollout-runtime-batch &amp;&amp; cargo test -p rollout-core --test dependency_direction &amp;&amp; mdbook build docs/book &amp;&amp; cargo clippy --workspace --all-targets -- -D warnings</automated>
  </verify>
  <acceptance_criteria>
    - `grep -q 'rollout-backend-vllm' Cargo.toml`
    - `grep -q 'rollout-runtime-batch' Cargo.toml`
    - `grep -q 'criterion' Cargo.toml`
    - `grep -q 'BACKEND_CRATES' crates/rollout-core/tests/dependency_direction.rs`
    - `grep -q 'violation_backend_uses_cloud' crates/rollout-core/tests/dependency_direction.rs`
    - `grep -q 'violation_backend_uses_transport' crates/rollout-core/tests/dependency_direction.rs`
    - `test -f crates/rollout-core/tests/fixtures/violation_backend_uses_cloud/Cargo.toml`
    - `test -f crates/rollout-core/tests/fixtures/violation_backend_uses_transport/Cargo.toml`
    - `test -f crates/rollout-backend-vllm/src/lib.rs && grep -q '^//!' crates/rollout-backend-vllm/src/lib.rs`
    - `test -f crates/rollout-runtime-batch/src/lib.rs && grep -q '^//!' crates/rollout-runtime-batch/src/lib.rs`
    - `grep -q 'inference/index.md' docs/book/src/SUMMARY.md`
    - `cargo build --workspace` exits 0
    - `cargo clippy --workspace --all-targets -- -D warnings` exits 0
    - `cargo test -p rollout-core --test dependency_direction` exits 0
    - `mdbook build docs/book` exits 0
  </acceptance_criteria>
  <done>
    Two new crates registered + buildable; dep-lint enforces #5 + #6 with fixture tests; criterion pinned; mdBook landing page in place. Spec 08 verify happens via `grep -q '## 2a. Phase 3 implementation notes' docs/specs/08-cli.md` (the planner adds the §2a stanza when wiring CLI subcommand routes in plan 03-04; for Wave 0 the spec stanza in 02 / 01 is sufficient).
  </done>
</task>

</tasks>

<verification>
End-to-end gate for Wave 0:
- `cargo build --workspace` exits 0
- `cargo test --workspace --tests` exits 0 (existing tests + new trait_surface + sampling_params_postcard + dep-direction backend invariants all green)
- `cargo clippy --workspace --all-targets -- -D warnings` clean
- `cargo xtask schema-gen` produces no drift (run twice; second `git diff` empty)
- `cargo deny check` clean
- `mdbook build docs/book` clean
- DOCS-02: at least one of docs/, inline rustdoc, or tests touched (this plan touches all three)
</verification>

<success_criteria>
Downstream Wave-1 streams (plans 03-01 and 03-02) can `cargo build -p rollout-backend-vllm` and `cargo build -p rollout-runtime-batch` against the new trait surface without any further rollout-core edits.
</success_criteria>

<output>
After completion, create `.planning/phases/03-inference-batch/03-00-wave0-trait-extensions-SUMMARY.md` per the SUMMARY template.
</output>
