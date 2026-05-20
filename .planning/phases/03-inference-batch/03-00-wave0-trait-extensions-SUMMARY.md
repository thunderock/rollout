---
phase: 03-inference-batch
plan: 00
subsystem: inference-trait-surface
tags: [rollout-core, traits, trait-extensions, inference-backend, sampling-params, worker-role, model-ref, prompt, completion, postcard, schema-gen, dep-direction, mdbook, specs, rollout-backend-vllm, rollout-runtime-batch, criterion]

# Dependency graph
requires:
  - phase: 02-local-substrate
    provides: rollout-core Phase-2 trait surface + EmbeddedStorage + InMemQueue + FsObjectStore + PluginHostImpl PyO3 pattern + EventEmitter + dep-direction lint with 4 invariants
provides:
  - "InferenceBackend trait extended to four-method Phase-3 shape (init/generate/model_id/shutdown) with Prompt/Completion/ModelRef/SamplingParams newtypes; JsonSchema-derived for schema-gen"
  - "SamplingParams marked #[non_exhaustive] with serde defaults — locks the wire format against Pitfall 1 silent field-adds"
  - "Prompt(pub String) + Completion { text, finish_reason, prompt_tokens, completion_tokens } newtypes (RESEARCH OQ 3 recommendation)"
  - "ModelRef { uri, content_id, tokenizer } lifted from spec 02 §2 into rollout-core::traits::backend and re-exported through rollout-core::config (D-BACKEND-05)"
  - "WorkerRole enum with Coordinator | BatchInference | BatchReader | BatchWriter | Custom(SmolStr) variants (D-BACKEND-04)"
  - "ContentId / RunId / WorkerId gain JsonSchema (#[schemars(with = \"String\")]) so ModelRef.content_id round-trips through schema-gen"
  - "rollout-backend-vllm crate skeleton (Layer 2): rollout-core + pyo3 + pyo3-async-runtimes deps; `vllm` Cargo feature flag; placeholder criterion bench"
  - "rollout-runtime-batch crate skeleton (Layer 3): rollout-core + rollout-storage + rollout-cloud-local deps; `test-mock-backend` feature flag"
  - "Two new dep-direction lint invariants: #5 (backend ↛ cloud) + #6 (backend ↛ transport) + fixture Cargo.toml pairs"
  - "criterion 0.5 (features = [\"async_tokio\"]) pinned in [workspace.dependencies]"
  - "docs/book/src/inference/index.md landing page + SUMMARY.md wire-up under a new `Inference` heading"
  - "Phase 3 implementation notes appended to specs 01 §3a (WorkerRole) and 02 §2a (extended InferenceBackend + SamplingParams non_exhaustive + streaming reject path)"
  - "sampling_params_postcard test pins postcard determinism + RESEARCH Pitfall 4 (stop = [] vs omitted byte-stability)"
affects: [03-01-rollout-backend-vllm-skeleton, 03-02-rollout-runtime-batch, 03-03-vllm-async-engine, 03-04-cli-infer-batch, 03-05-smoke-docs-bench]

# Tech tracking
tech-stack:
  added:
    - "criterion 0.5 (features = [\"async_tokio\"]) — Phase-3 bench harness for rollout-backend-vllm; pinned at workspace level"
    - "rollout-backend-vllm v0.1.0 (workspace member; vllm Cargo feature default OFF)"
    - "rollout-runtime-batch v0.1.0 (workspace member; test-mock-backend Cargo feature default OFF)"
    - "schemars JsonSchema derive on ContentId / RunId / WorkerId (rendered as String via #[schemars(with = \"String\")])"
  patterns:
    - "Phase-2 §1a / Phase-3 §Na implementation notes pattern continued in specs 01 + 02 per AGENTS.md §4 — original spec body stays authoritative, Phase-N annotations append without renumbering"
    - "Workspace-stub crate pattern from 02-00 reused: [package].rust-version.workspace = true + [lints] workspace = true + rollout-core = { path, version = \"0.1\" } (no wildcard)"
    - "Dep-direction fixture pattern reused: hand-rolled Cargo.toml under tests/fixtures/violation_<edge>/ with src/lib.rs stub; non-workspace-member so cargo's tests/ auto-discovery skips them"
    - "#[non_exhaustive] + serde defaults on SamplingParams so postcard wire shape can't drift without an explicit SAMPLING_PARAMS_SCHEMA_VERSION bump (Pitfall 1)"

key-files:
  created:
    - "crates/rollout-backend-vllm/{Cargo.toml,src/lib.rs,benches/throughput.rs} — Layer-2 skeleton + placeholder criterion bench"
    - "crates/rollout-runtime-batch/{Cargo.toml,src/lib.rs} — Layer-3 skeleton"
    - "crates/rollout-core/tests/fixtures/violation_backend_uses_cloud/{Cargo.toml,src/lib.rs} — invariant #5 fixture"
    - "crates/rollout-core/tests/fixtures/violation_backend_uses_transport/{Cargo.toml,src/lib.rs} — invariant #6 fixture"
    - "crates/rollout-core/tests/sampling_params_postcard.rs — postcard determinism + Pitfall 4 byte-stability test"
    - "docs/book/src/inference/index.md — Phase-3 inference landing page"
  modified:
    - "Cargo.toml — added rollout-backend-vllm + rollout-runtime-batch members; pinned criterion 0.5 in [workspace.dependencies]"
    - "crates/rollout-core/Cargo.toml — added postcard + toml dev-deps for sampling_params_postcard test"
    - "crates/rollout-core/src/lib.rs — re-exported Phase-3 types (Prompt, Completion, ModelRef, SamplingParams, WorkerRole)"
    - "crates/rollout-core/src/traits/mod.rs — re-exported Phase-3 types from backend + worker modules"
    - "crates/rollout-core/src/traits/backend.rs — replaced one-method stub with four-method Phase-3 trait + four new types"
    - "crates/rollout-core/src/traits/worker.rs — added WorkerRole enum"
    - "crates/rollout-core/src/config/mod.rs — pub-use ModelRef + SamplingParams from traits::backend"
    - "crates/rollout-core/src/ids.rs — added JsonSchema derive (with String repr) to ContentId / RunId / WorkerId"
    - "crates/rollout-core/tests/trait_surface.rs — added 4 Phase-3 RED-now-GREEN tests (inference_backend_has_extended_surface, phase3_new_types_exist, sampling_params_has_serde_defaults, worker_role_variants_construct)"
    - "crates/rollout-core/tests/dependency_direction.rs — added BACKEND_CRATES const + two new violation predicates + two new fixture-detection tests"
    - "docs/specs/01-core-runtime.md — appended §3a Phase 3 implementation notes (WorkerRole)"
    - "docs/specs/02-algorithms.md — appended §2a Phase 3 implementation notes (extended InferenceBackend, non_exhaustive SamplingParams, streaming reject, training-mode deferral)"
    - "docs/book/src/SUMMARY.md — added top-level `Inference` entry between Substrate and Examples"

key-decisions:
  - "[Claude] ContentId / RunId / WorkerId gained JsonSchema derive — Phase-3 ModelRef.content_id is Option<ContentId> + #[derive(JsonSchema)]; the schemars derive macro recursively requires JsonSchema on every field. Used #[schemars(with = \"String\")] on the inner [u8;32] / Ulid so the JSON schema renders them as strings (matches the Display impls). This is a Wave-0 ripple from D-BACKEND-05 + RESEARCH OQ 3 (newtypes carry ContentId)."
  - "[Claude] WorkerRole::Custom(SmolStr) — SmolStr lacks JsonSchema; used #[schemars(with = \"String\")] on the SmolStr variant payload (parallel to the ContentId pattern)."
  - "[Spec → AGENTS.md §4] InferenceBackend extension shape lands as a Phase-3 §2a stanza in spec 02; the spec-02 §2 trait sketch retains the v1 shape so future phases see the migration path. Same pattern Phase 2 used for spec 01 §1a / 04 §1a / 06 §1a (per the 02-00 SUMMARY)."
  - "[Plan rationale] criterion is pinned at workspace level with the async_tokio feature even though Wave-0 only ships a placeholder bench — the feature flag belongs to the pin, not the consumer, so Wave-2's real bench inherits it without a workspace edit."
  - "[Plan rationale] rollout-runtime-batch ships as a STANDALONE crate (not a module inside rollout-coordinator) per RESEARCH Open Question 1 — keeps the dep graph flat (backend stays Layer 2; runtime is Layer 3; CLI is Layer 4) and symmetric with the future rollout-runtime-online crate that Phase 8 will introduce."

patterns-established:
  - "Spec §Na implementation-notes stanza pattern: Phase 3 appends §3a (spec 01) and §2a (spec 02) without renumbering — same shape as Phase 2's §1a stanzas. Future phases will append §Nb / §Nc as needed."
  - "Workspace crate stub for Phase-N Wave-1 streams: ship the Cargo.toml + minimal src/lib.rs with crate-level //! doc + workspace lints in Wave 0 so Wave-1 streams can `cargo build -p <crate>` immediately."
  - "JsonSchema for newtype wrappers around non-JsonSchema types: use #[schemars(with = \"String\")] on the inner field; the schema renders the public-facing string form (Display impl) rather than the internal byte/ULID layout."

deviations:
  - "[Rule 2 — missing critical functionality] Plan instructions for Task 1 didn't spell out that ContentId / RunId / WorkerId needed JsonSchema. The plan's interface block for ModelRef { uri, content_id: Option<ContentId>, tokenizer } combined with the `JsonSchema` derive on ModelRef per RESEARCH §\"Code Examples\" requires every field to implement JsonSchema. Added the derive (with String repr) to all three ID types. Same pattern was needed for SmolStr inside WorkerRole::Custom."
  - "[Rule 1 — clippy] Initial /// doc strings for ModelRef + its uri field said \"HuggingFace\" — clippy::doc_markdown insists on backticks. Replaced with `HuggingFace` in both spots. Same shape Phase 2 hit repeatedly (PyO3, EventEmitter)."
  - "[Rule 1 — clippy] Initial sampling_params_postcard test used a raw-string-with-hashes literal that didn't need hashes. clippy::needless_raw_string_hashes flagged; removed the hashes."
  - "[Rule 2 — missing critical functionality] rollout-backend-vllm's placeholder bench needed `#![allow(missing_docs)]` at the file level — criterion_group! macro expands to undocumented public items. The workspace `missing_docs = \"warn\"` lint became an error under -D warnings."

# Known stubs (intentional — populated by downstream Wave-1+ plans)
known_stubs:
  - "crates/rollout-backend-vllm/src/lib.rs is a //! stub awaiting plan 03-01 (skeleton + InferenceBackend impl returning a stub error) and plan 03-03 (real AsyncLLMEngine wiring) — intentional per the Wave-0 plan rationale"
  - "crates/rollout-runtime-batch/src/lib.rs is a //! stub awaiting plan 03-02 (CAS state machine + coordinator + worker glue + MockBackend) and plan 03-04 (CLI subcommand) — intentional per the Wave-0 plan rationale"
  - "crates/rollout-backend-vllm/benches/throughput.rs is a placeholder criterion harness so [[bench]] resolves at workspace build time — real raw-vLLM-vs-rollout overhead bench lands in plan 03-05"
  - "SAMPLING_PARAMS_SCHEMA_VERSION: u8 constant is NOT yet defined (per CONTEXT it lives in rollout-runtime-batch alongside sample_id()) — intentional, lands in plan 03-02"

# Authentication gates / preflight notes
preflight_note: "Dev machine has Python 3.10.14 selected via pyenv by default; pyo3 abi3-py311 requires Python ≥ 3.11 at link time, so cargo build --workspace was run with PYENV_VERSION=3.11.12. The xtask schema-gen path uses datamodel-codegen which is only installed in 3.10.14, so the schema-drift integration test must run on the default pyenv (without PYENV_VERSION override). Same dev-machine policy Phase 2 inherited (documented in 02-00 SUMMARY)."

requirements-completed: [BACKEND-01, BACKEND-02, DOCS-01, DOCS-02, DOCS-03]

# Metrics
duration: 18min
completed: 2026-05-20
---

# Phase 3 Plan 00: Wave-0 Trait Extensions Summary

**One-liner:** Extended `rollout-core::InferenceBackend` to the Phase-3 four-method surface with `Prompt` / `Completion` / `ModelRef` / `SamplingParams` newtypes; added `WorkerRole` enum (BatchInference + forward-compat variants); registered `rollout-backend-vllm` (Layer 2) and `rollout-runtime-batch` (Layer 3) as workspace members; pinned `criterion 0.5`; extended dep-direction lint with invariants #5 (backend ↛ cloud) and #6 (backend ↛ transport) plus matching fixtures; appended Phase 3 implementation notes to specs 01 §3a and 02 §2a; shipped the mdBook `inference/index.md` landing page.

## What landed

### Task 1: rollout-core trait surface extension

**Trait shape (`crates/rollout-core/src/traits/backend.rs`):**

```rust
#[async_trait]
pub trait InferenceBackend: Send + Sync {
    async fn init(&mut self, model: &ModelRef) -> Result<(), CoreError>;
    async fn generate(&self, prompts: &[Prompt], params: &SamplingParams)
        -> Result<Vec<Completion>, CoreError>;
    fn model_id(&self) -> &ContentId;
    async fn shutdown(&mut self) -> Result<(), CoreError>;
}
```

`model_id` is sync (not async) so `&self`-async lifetimes don't propagate through the four downstream Wave-1 streams. All four type newtypes (`Prompt`, `Completion`, `ModelRef`, `SamplingParams`) derive `Debug + Clone + Serialize + Deserialize + JsonSchema` per the RESEARCH §"Code Examples" template.

**`SamplingParams`** carries `#[non_exhaustive]` per RESEARCH Pitfall 1 + `#[serde(deny_unknown_fields)]` + serde defaults wired through `mod defaults` helpers. The Phase-3 `stream` field is `bool` (must be `false`); D-BACKEND-03 reject happens at config-validate in Wave 3 (plan 03-04). RESEARCH Pitfall 4 (`stop: Vec<String>` not `Option<Vec<String>>`) is honored.

**`WorkerRole`** lives next to `WorkerState` in `traits/worker.rs` with five variants. `serde(rename_all = "snake_case")` so TOML configs spell variants as `batch_inference`. The `Custom(SmolStr)` payload uses `#[schemars(with = "String")]` because `SmolStr` doesn't implement `JsonSchema` natively.

**Module re-exports:** `src/lib.rs` now re-exports `Completion`, `ModelRef`, `Prompt`, `SamplingParams`, `WorkerRole`; `src/config/mod.rs` `pub use`s `ModelRef` + `SamplingParams` from `traits::backend` so future `InferBatchConfig` block types compose them without crossing trait module boundaries.

**Tests:** `trait_surface.rs` gained four new compile-shape tests (`inference_backend_has_extended_surface`, `phase3_new_types_exist`, `sampling_params_has_serde_defaults`, `worker_role_variants_construct`). New `sampling_params_postcard.rs` covers postcard determinism + the Pitfall 4 omitted-vs-empty-stop byte-stability requirement (TOML round-trip).

**Spec edits:** `docs/specs/01-core-runtime.md` gained `## 3a. Phase 3 implementation notes` (WorkerRole); `docs/specs/02-algorithms.md` gained `## 2a. Phase 3 implementation notes` (extended `InferenceBackend`, `#[non_exhaustive] SamplingParams`, streaming-reject, training-mode deferral). Spec 08 §2.5 (`rollout infer batch --config batch-infer.toml`) was verified — already matches D-CLI-01; no change needed (per the plan).

**Wave-0 ripple — JsonSchema on ID types:** `ModelRef.content_id: Option<ContentId>` combined with `#[derive(JsonSchema)]` on `ModelRef` propagates a `JsonSchema` bound onto `ContentId`. Same cascade reached `RunId` + `WorkerId` because they're also part of the public re-export surface. Solved with `#[schemars(with = "String")]` so the JSON schema renders the human-facing string form (hex digest / Crockford ULID) rather than the byte/integer layout. Rust-side equality / serde wire shape is unchanged.

### Task 2: Crate skeletons + dep-direction lint + workspace registration + mdBook landing page

**`crates/rollout-backend-vllm/`** (Layer 2; depends on `rollout-core` + `pyo3` + `pyo3-async-runtimes` only):

- `Cargo.toml` with `vllm` Cargo feature gating the live PyO3 init path (default OFF per D-VLLM-03); `[[bench]] name = "throughput" harness = false`; `criterion` + `tempfile` as dev-deps.
- `src/lib.rs` is a crate-level `//!` doc citing D-BACKEND-01 + D-VLLM-03; real impl lands in plans 03-01 + 03-03.
- `benches/throughput.rs` is a placeholder criterion harness (`fn placeholder(c: &mut Criterion) { ... }` + `criterion_group!` + `criterion_main!`) so workspace build resolves the `[[bench]]` entry. Real raw-vLLM-vs-rollout overhead bench lands in plan 03-05.

**`crates/rollout-runtime-batch/`** (Layer 3; depends on `rollout-core` + `rollout-storage` + `rollout-cloud-local`):

- `Cargo.toml` with `test-mock-backend` Cargo feature for the deterministic restart-resume test (RESEARCH §"Restart-resume test design"). No direct `rollout-backend-vllm` dep — the runtime takes `Arc<dyn InferenceBackend>` so it stays backend-agnostic.
- `src/lib.rs` is a crate-level `//!` doc citing the CAS sample-state machine + JSONL I/O + `MockBackend` responsibilities; real impl lands in plans 03-02 + 03-04.

**Workspace `Cargo.toml`:**

- Two new `[workspace] members` entries.
- `criterion = { version = "0.5", features = ["async_tokio"] }` pinned in `[workspace.dependencies]` under a new `# Phase 3 — benches` heading.

**Dep-direction lint (`crates/rollout-core/tests/dependency_direction.rs`):**

- New `BACKEND_CRATES: &[&str] = &["rollout-backend-vllm"]` const.
- Two new violation predicates: `violation_backend_uses_cloud` and `violation_backend_uses_transport`, OR'd into `any_violation`.
- Two new fixture-detection tests: `backend_must_not_depend_on_cloud` and `backend_must_not_depend_on_transport`, exercised against `tests/fixtures/violation_backend_uses_cloud/Cargo.toml` and `violation_backend_uses_transport/Cargo.toml`. Both fixtures use `name = "rollout-backend-vllm"` so the predicate matches; the first declares `rollout-cloud-local = "0.1"`, the second declares `rollout-transport = "0.1"`.

**mdBook (`docs/book/src/inference/index.md` + `SUMMARY.md`):**

- A new top-level `Inference` entry between Substrate and Examples.
- The landing page is a ~25-line overview citing the three Phase-3 crates (`rollout-backend-vllm`, `rollout-runtime-batch`, `rollout-cli`), the Wave-0 trait extension cross-link to spec 02 §2a + spec 01 §3a, and a TODO list for per-component chapters that plan 03-05 will fill.

## End-to-end verification

All commands exit 0 (with the documented pyenv version):

```
PYENV_VERSION=3.11.12 cargo build --workspace
cargo test -p rollout-core --tests                  # 14 rollout-core unit tests + 13 trait_surface + 7 dep_direction + 2 postcard
cargo test -p rollout-core --test dependency_direction   # 7 tests incl. backend_must_not_depend_on_{cloud,transport}
PYENV_VERSION=3.11.12 cargo clippy --workspace --all-targets -- -D warnings
PYENV_VERSION=3.11.12 RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::missing_crate_level_docs" \
  cargo doc -p rollout-core -p rollout-backend-vllm -p rollout-runtime-batch --no-deps
cargo deny check
mdbook build docs/book
cargo xtask schema-gen   # zero drift; new types not yet wired into RunConfig (will land in plan 03-02)
```

Schema-gen reports zero drift because `SamplingParams` + `ModelRef` + `WorkerRole` are not yet referenced by `RunConfig` — they'll join via `InferBatchConfig` in plan 03-02. The `JsonSchema` derive is in place for that wire-up.

## Deviations from Plan

### Auto-fixed issues

1. **[Rule 2 — missing critical functionality] `ContentId` / `RunId` / `WorkerId` lacked `JsonSchema`.**
   - Found during: Task 1 first build of the extended `ModelRef`.
   - Issue: `#[derive(JsonSchema)]` on `ModelRef` requires `ContentId: JsonSchema` for the `Option<ContentId>` field; the same cascade reaches `RunId` + `WorkerId` because they're re-exported from `rollout-core` and consumed by downstream crates.
   - Fix: Added `JsonSchema` derive on all three ID types with `#[schemars(with = "String")]` on the inner field so the schema renders the human-facing string form (hex digest / Crockford ULID Display).
   - Files modified: `crates/rollout-core/src/ids.rs`.
   - Commit: 89bb97f (Task 1).

2. **[Rule 2 — missing critical functionality] `SmolStr` lacked `JsonSchema` for `WorkerRole::Custom`.**
   - Found during: Task 1 first build of `WorkerRole`.
   - Issue: Same cascade as #1 — `schemars` derive requires every field to implement `JsonSchema`; `smol_str::SmolStr` doesn't.
   - Fix: `#[schemars(with = "String")]` on the `SmolStr` variant payload.
   - Files modified: `crates/rollout-core/src/traits/worker.rs`.
   - Commit: 89bb97f (Task 1).

3. **[Rule 1 — clippy] `clippy::doc_markdown` flagged "HuggingFace" without backticks.**
   - Found during: Task 1 clippy run.
   - Issue: Two doc strings on `ModelRef` + `ModelRef.uri` mentioned `HuggingFace` without backticks; clippy::doc_markdown insists on the backtick form for unknown CamelCase identifiers.
   - Fix: Wrapped both in backticks (`HuggingFace`). Same shape Phase 2 hit repeatedly for PyO3 / EventEmitter.
   - Files modified: `crates/rollout-core/src/traits/backend.rs`.
   - Commit: 89bb97f (Task 1).

4. **[Rule 1 — clippy] `clippy::needless_raw_string_hashes` flagged the explicit-TOML literal.**
   - Found during: Task 1 clippy run on the new postcard test.
   - Issue: The TOML literal used `r#"..."#` but contained no `"` characters that needed hashing; clippy demands the simplest raw-string form.
   - Fix: Switched to `r"..."`.
   - Files modified: `crates/rollout-core/tests/sampling_params_postcard.rs`.
   - Commit: 89bb97f (Task 1).

5. **[Rule 2 — missing critical functionality] `criterion_group!` macro expansion violates `missing_docs`.**
   - Found during: Task 2 workspace clippy run.
   - Issue: `criterion_group!(benches, placeholder)` expands to a public `fn benches()` that the macro can't doc-comment, tripping the workspace `missing_docs = "warn"` lint under `-D warnings`.
   - Fix: Added `#![allow(missing_docs)]` at the top of `benches/throughput.rs` (file-level scope). The placeholder bench is internal scaffolding; the real bench in plan 03-05 will inherit the same allow.
   - Files modified: `crates/rollout-backend-vllm/benches/throughput.rs`.
   - Commit: 57b07a5 (Task 2).

6. **[Rule 1 — clippy] `clippy::doc_markdown` flagged "PyO3" in the new crate's `//!` doc.**
   - Found during: Task 2 workspace clippy run.
   - Issue: Initial `rollout-backend-vllm/src/lib.rs` mentioned PyO3 twice without backticks.
   - Fix: Wrapped both in backticks. Same Phase-2 shape (per 02-00 SUMMARY deviation #5).
   - Files modified: `crates/rollout-backend-vllm/src/lib.rs`.
   - Commit: 57b07a5 (Task 2).

### Rule-4 (architectural) deviations

None. All work stayed inside the trait-extension scope sanctioned by RESEARCH §"Wave 0 Gaps".

### Open questions surfaced for Wave 1

- **`SAMPLING_PARAMS_SCHEMA_VERSION: u8` constant location.** CONTEXT places it in `rollout-runtime-batch` alongside `sample_id()`. Plan 03-02 will land it and re-export through the public crate root. The Wave-0 trait extension does not yet reference it; sample-ID derivation in plan 03-02 is the first consumer.
- **`InferBatchConfig` block types** (`[model]` / `[sampling]` / `[input]` / `[output]` / `[workers]`) are not yet part of `RunConfig`. Plan 03-02 will introduce them; schema-gen drift will trigger at that point. The Wave-0 newtypes (`SamplingParams`, `ModelRef`) are already JsonSchema-shaped, so the wire-up is mechanical.
- **`Prompt` / `Completion` content-ID derivation** — both newtypes are `String`-based today. RESEARCH OQ 3 suggests they may grow `content_id() -> ContentId` accessors in plan 03-02 for the resume path. The Wave-0 shape doesn't preclude this; the field layout is forward-compatible.

## Commits

| Task | Hash    | Subject                                                                  |
| ---- | ------- | ------------------------------------------------------------------------ |
| 1    | 89bb97f | feat(03-00): extend rollout-core traits for inference + worker-role      |
| 2    | 57b07a5 | feat(03-00): register Phase-3 crate stubs + dep-lint invariants #5/#6    |

## Self-Check: PASSED

- crates/rollout-core/src/traits/backend.rs (extended) — FOUND
- crates/rollout-core/src/traits/worker.rs (WorkerRole) — FOUND
- crates/rollout-core/src/ids.rs (JsonSchema derives) — FOUND
- crates/rollout-core/tests/sampling_params_postcard.rs — FOUND
- crates/rollout-backend-vllm/{Cargo.toml,src/lib.rs,benches/throughput.rs} — FOUND
- crates/rollout-runtime-batch/{Cargo.toml,src/lib.rs} — FOUND
- crates/rollout-core/tests/fixtures/violation_backend_uses_cloud/{Cargo.toml,src/lib.rs} — FOUND
- crates/rollout-core/tests/fixtures/violation_backend_uses_transport/{Cargo.toml,src/lib.rs} — FOUND
- docs/book/src/inference/index.md — FOUND
- Cargo.toml (criterion 0.5 + 2 new members) — FOUND
- docs/specs/01-core-runtime.md §3a — FOUND
- docs/specs/02-algorithms.md §2a — FOUND
- Commits 89bb97f + 57b07a5 — both present in `git log --oneline -5`
