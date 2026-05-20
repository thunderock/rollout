# Phase 3: Inference backend (vLLM) + batch inference — Context

**Gathered:** 2026-05-20
**Status:** Ready for planning
**Mode:** `/gsd:discuss-phase 3 --auto` — Claude auto-selected recommended defaults; user can edit before plan-phase consumes this file.
**Source:** Synthesized from `ROADMAP.md` §"Phase 3", `.planning/REQUIREMENTS.md` BACKEND-01..02, AGENTS.md §9 standing rules, `docs/specs/02-algorithms.md` §2 (InferenceBackend), `docs/specs/08-cli.md` §2 (CLI surface), Phase 1 + Phase 2 CONTEXT.md, all eight Phase-2 plan SUMMARYs.

<domain>
## Phase Boundary

Phase 3 delivers **end-to-end batch inference on a real model** — the first "useful" thing rollout does. Three new crates ship plus one Wave-0 trait extension to `rollout-core`:

- **`rollout-backend-vllm`** — Rust crate implementing the `InferenceBackend` trait against vLLM via PyO3 in-process (the second user of the PyO3 path proven by 02-05). Loads a model into vLLM's `AsyncLLMEngine`; receives `(model_ref, sampling_params, prompts)` and returns completions. CUDA auto-detected at runtime via `nvml-wrapper` (already in `rollout-cloud-local`); falls back to CPU when no GPU is present.
- **`rollout-cli infer batch`** — new CLI subcommand (extends `rollout-cli` from Phase 1 + 2). Reads a TOML config + a JSONL input file, enqueues content-addressed samples into the coordinator's queue (via `rollout-cloud-local::InMemQueue` from 02-03), waits for completion, writes JSONL output. `--resume <run_id>` re-attaches and skips already-completed samples.
- **`rollout-runtime-batch`** (or a `batch` module inside an existing crate — planner's call) — the worker-side glue. Pulls sample-IDs from the queue via the Phase-2 transport Work channel (which 02-04 stubbed), invokes the backend, persists per-sample state in `rollout-storage`.
- **Wave 0: `rollout-core` extensions.** Same pattern as Phase 2's 02-00. `InferenceBackend` extends with `SamplingParams` + batched `generate`; `WorkerRole` enum lands (variants for `BatchInference` now, `BatchReader` / `BatchWriter` enumerated for Phase 6); new types `SampleRecord`, `SampleState`, `ModelRef`. Spec 02 / 01 / 08 updated in-place per AGENTS.md §4 if extensions differ.

**Out of scope (explicit):**
- Streaming generation — Phase 8 (`INFER-01..02`, online inference + tool calling).
- Training-mode forward/backward — Phase 4 (`TRAIN-01..04`). BACKEND-01 phrasing covers training mode too, but the Phase-3 deliverable is inference-only. Phase 4 either (a) extends `InferenceBackend` with forward/backward methods, or (b) introduces a sibling `TrainableBackend: InferenceBackend` trait — Phase 4 owns that decision. Phase 3 keeps the trait inference-shaped.
- Multi-node distribution / true reader-writer worker split — Phase 6 (`DIST-01..05`). Phase 3 runs one coordinator + one or more inference workers on the same host.
- Object-store-backed sample storage in Phase 3 uses `rollout-cloud-local::FsObjectStore`; S3/GCS lands in Phase 5 (`CLOUD-01..03`).
- Snapshot integration (training-state, buffer, process) — Phases 4 / 9 / 11. Phase 3 has no snapshot story; restart-resume rides on the sample-state KV in `rollout-storage`.
- Episodic-memory or evaluation harness — Phases 8 / 7.

</domain>

<decisions>
## Implementation Decisions

### vLLM integration path (`rollout-backend-vllm`)

- **D-VLLM-01** — **PyO3 in-process loader** is the integration path (not sidecar). Reuses 02-05's dedicated Python OS thread pattern (`rollout-py-vllm` thread; `tokio::sync::mpsc` request hop; `Python::attach` on the owning thread). ROADMAP risk callout names this explicitly: "vLLM's Python-only API forces the backend through PyO3."
- **D-VLLM-02** — vLLM runtime API: **`AsyncLLMEngine`**, not the synchronous `LLM` wrapper. Async-native per AGENTS.md principle #1; gives vLLM's continuous batching for free (no Rust-side batching logic). Coroutines bridged to Tokio via `pyo3_async_runtimes::tokio::future_into_py` / `into_future`.
- **D-VLLM-03** — vLLM packaging: **optional `vllm` Cargo feature** (default OFF). When enabled, the crate links pyo3 ABI3 + expects `vllm` (and its CUDA wheel) installed in the active Python environment via `pip install vllm`. CI tests gated `#[ignore]` unless `ROLLOUT_VLLM_AVAILABLE=1` env var is set. `cargo test --features vllm` runs them locally on a dev box with vllm installed.
- **D-VLLM-04** — CUDA detection: at runtime via the existing `rollout-cloud-local::ComputeHint::inventory()` (Linux NVML path from 02-03). The vLLM engine constructor receives a `device = "auto"` setting; vLLM internally picks CUDA when available, falls back to CPU. Tests run CPU mode; nightly CI / dev machines can run GPU mode.
  - **Updated per RESEARCH Pitfall 9:** explicit `torch.cuda.is_available()` probe is used in place of vLLM's `device="auto"` to avoid silent CPU fallback when CUDA libraries are partially installed. `ComputeHint::inventory()` still informs the worker config / observability events but is no longer the source-of-truth for the vLLM `device` kwarg. The Python-side glue performs the probe and passes `device="cuda"` or `device="cpu"` explicitly to `EngineArgs`.
- **D-VLLM-05** — Tokenizer ownership: the **backend owns the tokenizer**. Loaded at engine init from `ModelRef.tokenizer` override or autodetected from the model. Algorithms (Phase 4+) never see token IDs in Phase 3. A first-class tokenizer trait may land in Phase 4 if SFT/RM need direct access — out of Phase 3 scope.

### `InferenceBackend` trait extension (Wave 0)

- **D-BACKEND-01** — Phase 3 extends the trait with the **inference surface only**. Training-mode forward/backward is deferred to Phase 4's discretion (either trait extension or sibling `TrainableBackend: InferenceBackend` — Phase 4 picks). The Phase-3 trait extension looks roughly like:
  ```rust
  #[async_trait]
  pub trait InferenceBackend: Send + Sync {
      async fn init(&mut self, model: ModelRef) -> Result<(), CoreError>;
      async fn generate(
          &self,
          prompts: &[Prompt],
          params: &SamplingParams,
      ) -> Result<Vec<Completion>, CoreError>;
      fn model_id(&self) -> &ContentId;     // for content-addressed sample IDs
      fn shutdown(&mut self) -> Result<(), CoreError>;
  }
  ```
- **D-BACKEND-02** — `SamplingParams` shape (matches vLLM SamplingParams 1:1 so we can serialize through PyO3 without a per-field mapping layer):
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
  pub struct SamplingParams {
      pub temperature: f32,    // default 1.0
      pub top_p:       f32,    // default 1.0
      pub top_k:       i32,    // default -1 (disabled)
      pub max_tokens:  u32,    // default 16
      pub seed:        Option<u64>,
      pub stop:        Vec<String>,
      pub stream:      bool,   // Phase 3 = always false; rejected at config-validate time if true
  }
  ```
- **D-BACKEND-03** — No streaming in Phase 3. `SamplingParams.stream = true` is rejected at config-validate time with `Fatal { kind: FatalKind::ConfigInvalid, msg: "streaming generation is Phase 8 (INFER-01)" }`. Streaming arrives with the online inference server.
- **D-BACKEND-04** — `WorkerRole` enum lands in Wave 0 with variants `Coordinator | BatchInference | BatchReader | BatchWriter | Custom(SmolStr)`. Phase 3 wires `BatchInference` only; `BatchReader` / `BatchWriter` are enumerated for forward-compatibility with Phase 6.
- **D-BACKEND-05** — `ModelRef` already in spec 02 §2; Wave 0 lifts it into `rollout-core::config` so it's reusable across phases.

### Resumable batch design

- **D-RESUME-01** — Sample-ID derivation:
  ```rust
  fn sample_id(model: &ContentId, prompt: &str, params: &SamplingParams, idx: u64) -> ContentId {
      let mut h = blake3::Hasher::new();
      h.update(&[SAMPLING_PARAMS_SCHEMA_VERSION]);  // = 1 in Phase 3; prepended per RESEARCH Pitfall 1
      h.update(model.as_bytes());
      h.update(prompt.as_bytes());
      h.update(&postcard::to_stdvec(params).unwrap());
      h.update(&idx.to_le_bytes());
      ContentId::from(h.finalize())
  }
  ```
  Deterministic; identical input ⇒ identical ID. `idx` breaks ties when the same prompt + params appears twice in one batch.
  The `SAMPLING_PARAMS_SCHEMA_VERSION: u8` constant lives in `rollout-runtime-batch` (alongside `sample_id()`) and re-exports through the public crate root. Bumping it invalidates outstanding `Pending`/`Running` sample-IDs on resume — documented migration path: drain the in-flight batch under the old version, then resume with the new schema (per RESEARCH §"Pitfall 1").
- **D-RESUME-02** — Persistence: `rollout-storage` namespace `infer/<run_id>/samples`. Key = `sample_id.to_string()`. Value = postcard `SampleRecord { id: ContentId, prompt_blob: ContentId, state: SampleState, started_at, finished_at, worker_id }` where:
  ```rust
  pub enum SampleState {
      Pending,
      Running,
      Done { completion_blob: ContentId },
      Failed { reason: String },
  }
  ```
  Workers update state transitions atomically via `Storage::cas`. Coordinator scans the namespace on startup and on `--resume`.
- **D-RESUME-03** — Worker role split: **one logical role in Phase 3** — `WorkerRole::BatchInference`. Each worker runs the read+infer+write loop in-process. `BatchReader` and `BatchWriter` variants are enumerated for Phase 6 but unused. ROADMAP's "reader/writer worker types" language survives in the config schema (`[workers.reader]` / `[workers.writer]` blocks are accepted but their `count` must equal 0 in Phase 3 configs).
- **D-RESUME-04** — Queue: reuse `rollout-cloud-local::InMemQueue` (02-03; RAM hot path + Storage spill for restart replay). Coordinator at plan time enqueues all sample-IDs that have `state ∈ {Pending, Running, Failed}` (skips `Done`). Workers pull batches via the Phase-2 `rollout-transport` Work channel — the transport had a bidi-stub in 02-04; Phase 3 wires the real handler.
- **D-RESUME-05** — Output blob storage: `rollout-cloud-local::FsObjectStore` (02-03; content-addressed sharded FS under `./data/object-store/`). Each completion blob is keyed by its own `ContentId = blake3(completion_text)`; sample record stores the blob's `ContentId` so duplicate completions deduplicate naturally. Object store ⇄ S3 swap is Phase 5.

### CLI surface, input/output, test model

- **D-CLI-01** — `rollout infer batch --config <path> [--resume <run_id>] [--workers N]`. Config TOML:
  ```toml
  [model]
  uri       = "Qwen/Qwen2.5-0.5B-Instruct"
  tokenizer = "..."                  # optional override
  
  [sampling]
  temperature = 0.7
  top_p       = 0.9
  max_tokens  = 64
  seed        = 42
  stop        = []
  
  [input]
  glob = "data/prompts/*.jsonl"
  
  [output]
  dir  = "data/completions"
  
  [workers]
  count = 1                          # Phase 3 single-host; ≥1
  ```
  Output: progress bar in `--verbose` mode; silent + non-zero exit on failure. JSON/text mode controlled by global `--format`. `--resume <run_id>` re-attaches; coordinator scans existing storage and only enqueues outstanding samples.
- **D-CLI-02** — Input JSONL: one object per line. Required: `prompt: String`. Optional: `id: String` (used verbatim if present; otherwise `id = blake3(prompt)`). Extra fields preserved and round-tripped to output.
- **D-CLI-03** — Output JSONL: `{ id, prompt, completion, sampling_params, model_uri, finish_reason, model_content_id, completion_blob_id, generated_at }`. One line per input prompt. Order matches input file order (workers may produce out-of-order; CLI sorts on write).
- **D-CLI-04** — Test model: **`Qwen/Qwen2.5-0.5B-Instruct`** (Apache-2.0, ~1 GB, CPU-runnable, vLLM-supported, well-known small chat model). Downloaded on first run via vLLM's HuggingFace integration; cached under `~/.cache/huggingface/`. Smoke test uses a 4-prompt × 16-token batch — completes in <60 s on CPU.
- **D-CLI-05** — Benchmark: `crates/rollout-backend-vllm/benches/throughput.rs` (criterion-driven). Runs N=64 prompts × max_tokens=64 with fixed seed against rollout-backend-vllm vs a raw `vllm.LLM` baseline driven from Python. Reports tokens/sec ratio; CI exposes a `bench` job that runs only on a `runs-on: [self-hosted, gpu]` label so the public-runner workflow never blocks on it. Exit-criterion <10 % overhead is verified there.

### Wave breakdown (planner reference)

- **Wave 0 (single plan):** Extend `rollout-core` traits (InferenceBackend extension, SamplingParams, ModelRef, WorkerRole, SampleRecord/SampleState) + register `rollout-backend-vllm` + supporting workspace deps (pyo3 0.28 already pinned in Phase 2; add `criterion`, `tokenizers` if needed) + spec edits to 01/02/08 + extend `dependency_direction.rs` invariants (`rollout-backend-vllm` may depend on PyO3 + rollout-core only; no cloud crates).
- **Wave 1 (parallel, 2 streams):**
  - `rollout-backend-vllm` skeleton — Rust adapter, manifest, PyO3 thread bootstrap, `InferenceBackend` impl returning a stub error until W2 lands the real engine. Plus the Python-side glue under `python/rollout/backends/vllm/` (an importable module with a `serve()` entrypoint that the Rust side calls into).
  - `rollout-runtime-batch` (or batch module) — coordinator-side queue management; per-sample state CAS logic; Worker side pull loop. Stubbed `InferenceBackend` for now.
- **Wave 2:** Wire the real `AsyncLLMEngine`. Implement init → generate → shutdown lifecycle on the dedicated Python thread. Implement CUDA-vs-CPU auto-detection. Land the `criterion` throughput benchmark.
- **Wave 3:** `rollout-cli infer batch` subcommand. TOML config + JSONL input/output + `--resume` semantics.
- **Wave 4:** End-to-end test + docs. `scripts/infer-smoke.sh` runs `rollout infer batch --config examples/batch-tiny.toml` against a 4-prompt input on CPU; verifies output JSONL parses + completions non-empty. Substrate-style mdBook chapter under `docs/book/src/inference/`. CI integration (new `infer-smoke` job behind `ROLLOUT_VLLM_AVAILABLE=1` env; for now, marked optional / skip).

### Claude's Discretion (defer to research / planner)

- Exact crate name for the runtime glue: standalone `rollout-runtime-batch` vs a `batch` module inside `rollout-cli` or `rollout-coordinator`. Planner picks based on dep-direction cleanliness.
- Whether `Prompt` and `Completion` are bare `String` aliases or wrapped newtypes carrying `ContentId`. Recommend newtypes for type safety.
- vLLM minimum supported version pin (research picks the latest stable that supports the chosen pyo3 ABI3 boundary; likely `vllm>=0.6`).
- Whether the Python-side glue ships as `python/rollout/backends/vllm/` (pure Python module) or via maturin (compiled extension). Phase 2 chose stdlib-only for sample plugins — but vLLM is itself a heavy compiled dep, so the constraint relaxes. Recommend pure Python module that imports vllm; maturin only if Phase 4 needs it.
- Whether the AsyncLLMEngine runs in `EngineArgs(disable_log_stats=True)` mode in Phase 3 (recommend yes for clean CI output).
- Memory-pressure handling: vLLM's `gpu_memory_utilization` config knob default. Recommend `0.85` for CUDA hosts and N/A for CPU.
- Whether the coordinator's plan-time validation pre-loads the model to check it exists / is downloadable. Recommend yes — fail fast at `rollout plan`, not at minute 47 of a run (AGENTS.md principle #3).
- HuggingFace auth: if the model is gated, the Phase-3 SecretStore env-var `ROLLOUT_SECRET_HF_TOKEN` is read at plan time. Document the allowlist requirement.
- Whether to ship a `--dry-run` flag that validates everything but doesn't actually generate. Recommend yes — useful for users.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Roadmap & requirements
- `ROADMAP.md` §"Phase 3 — Inference backend (vLLM) + batch inference" — goal, includes, exit criteria, risks
- `.planning/REQUIREMENTS.md` — BACKEND-01, BACKEND-02

### Architectural source-of-truth
- `AGENTS.md` — principles #1 async-native, #2 batching first-class, #3 plan-time validation, #4 single source of truth config, #7 every plugin testable locally without GPU, #9 layered cloud abstraction, #10 observability not optional
- `AGENTS.md` §9 — DOCS-01/02/03 still apply (per-commit doc/test policy, mdBook chapters, rustdoc gate). §9.4 v1-example commitment — Phase 3 lands the inference building block but does NOT yet ship the v1 example recipe (SHIP-03 hardens in Phase 4 stub → Phase 9 real → Phase 12 docs).

### Phase-3 canonical specs (implementation contracts)
- `docs/specs/02-algorithms.md` §2 — shared trait surface incl. `InferenceBackend`, `ModelRef`, `AlgoDependencies` (verifies the InferenceBackend extension shape matches what Phase 4 algorithm crates will expect)
- `docs/specs/02-algorithms.md` §11 Open questions — tokenizer ownership, content-addressing of model weights
- `docs/specs/08-cli.md` §2.5 `rollout infer <mode>` — `infer batch` subcommand surface
- `docs/specs/08-cli.md` §3 — config file conventions (TOML; `deny_unknown_fields`)
- `docs/specs/01-core-runtime.md` §3 — `Worker` lifecycle; Phase-3 introduces `WorkerRole::BatchInference`
- `docs/specs/03-plugin-system.md` §3.2 PyO3 in-process — the integration mode `rollout-backend-vllm` reuses
- `docs/specs/04-storage-snapshots.md` §2 — `Storage`, `StorageTxn` (sample-state KV uses CAS for the `Pending → Running → Done` transitions)
- `docs/specs/06-cloud-layer.md` §3 — `ObjectStore`, `Queue` (FsObjectStore + InMemQueue from 02-03 are the Phase-3 backing stores)
- `docs/specs/09-observability.md` — `EventEmitter` (Phase 3 emits sample_started, sample_completed, sample_failed, generation_throughput events)
- `docs/specs/10-component-split.md` — dep-direction rule for `rollout-backend-vllm` (Layer 2: depends on rollout-core + pyo3; MUST NOT depend on cloud crates or other backend crates)
- `docs/specs/11-config-schema.md` — single-source-of-truth config; new `[model]`, `[sampling]`, `[input]`, `[output]`, `[workers]` blocks for `infer batch` configs follow these rules

### Prior phase context (decisions inherited)
- `.planning/phases/01-core-foundations/01-CONTEXT.md` — Makefile shape, CI jobs (11 + smoke = 12 from Phase 2), schema-gen pipeline, mdBook layout, dep-direction lint, conventional-commits, per-commit doc/test policy
- `.planning/phases/02-local-substrate/02-CONTEXT.md` — six new crates from Phase 2, locked decisions D-STO-* through D-SANDBOX-*
- `.planning/phases/02-local-substrate/02-00-wave0-trait-extensions-SUMMARY.md` — the trait extension pattern Phase 3 Wave 0 mirrors
- `.planning/phases/02-local-substrate/02-05-rollout-plugin-host-SUMMARY.md` — proven PyO3 dedicated-thread pattern (`Python::attach`, pyo3 0.28 API)
- `.planning/phases/02-local-substrate/02-03-rollout-cloud-local-SUMMARY.md` — InMemQueue with Storage spill (resumable queue backend); FsObjectStore (content-addressed sharded FS)
- `.planning/phases/02-local-substrate/02-02-rollout-storage-SUMMARY.md` — EmbeddedStorage with redb + CAS for sample-state transitions

### Repo state Phase 3 modifies or extends
- `crates/rollout-core/src/traits/backend.rs` — extend `InferenceBackend` (Wave 0)
- `crates/rollout-core/src/traits/worker.rs` — add `WorkerRole` enum (Wave 0)
- `crates/rollout-core/src/config/` — add `SamplingParams`, `ModelRef`, infer-batch config block types (Wave 0)
- `crates/rollout-cli/src/main.rs` — add `infer batch` subcommand (Wave 3)
- `crates/rollout-backend-vllm/` — NEW crate (Wave 1 + 2)
- `crates/rollout-runtime-batch/` (or batch module) — NEW crate (Wave 1)
- `python/rollout/backends/vllm/` — NEW Python module bridging into vLLM (Wave 1 + 2)
- `Cargo.toml` (workspace) — register new crates + add criterion + tokenizers if needed
- `deny.toml` — verify new transitive deps pass
- `Makefile` — add `infer-smoke` target (Wave 4)
- `.github/workflows/ci.yml` — add optional `infer-smoke` job behind `ROLLOUT_VLLM_AVAILABLE=1` (Wave 4)
- `docs/book/src/SUMMARY.md` + `docs/book/src/inference/` — new section (Wave 4)
- `examples/batch-tiny.toml` — the canonical example config referenced in the ROADMAP exit criterion

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **`InferenceBackend` trait stub** at `crates/rollout-core/src/traits/backend.rs` — single method, needs extension.
- **`PluginHostImpl` PyO3 mode** (02-05) — proven dedicated Python OS thread pattern. `rollout-backend-vllm` may either: (a) reuse the same pattern in-crate, or (b) load itself as a PyO3 plugin via `PluginHostImpl::load(manifest)`. Recommend (a) for the Phase-3 first user — the backend's lifecycle is too tightly coupled to the algorithm runtime to live as an arms-length plugin.
- **`rollout-storage::EmbeddedStorage`** (02-02) — sample-state persistence via CAS. Namespace pattern from 02-03 (`cloudlocal/queue/*`) is the template for `infer/<run_id>/samples/*`.
- **`rollout-cloud-local::InMemQueue`** (02-03) — already supports Storage spill for restart replay. Phase 3 enqueues sample-IDs at plan time; workers pull.
- **`rollout-cloud-local::FsObjectStore`** (02-03) — content-addressed sharded FS for completion blobs.
- **`rollout-coordinator`** (02-06) — already speaks the Heartbeat + Control + Work channels. Phase 3 wires the Work channel's real handler (Phase 2 had a bidi-stub).
- **`rollout-transport`** (02-04) — HTTP/2 + rustls + mTLS-by-default already shipped. No transport changes needed in Phase 3 unless the bidi-stub needs growth.

### Established Patterns
- **Workspace deps in `[workspace.dependencies]`** — pyo3 0.28, pyo3-async-runtimes 0.28, blake3 already pinned. Phase 3 adds `criterion` (benches), possibly `tokenizers` (HuggingFace's Rust tokenizer crate — optional, only if the planner wants a tokenizer trait pre-emptively).
- **`cargo deny` allowlist** — already accepts Apache-2.0 WITH LLVM-exception (added in 02-05 for target-lexicon via pyo3-build-config). vLLM's Python deps don't matter for cargo deny; Rust deps will.
- **Per-crate `[lints.rust] unsafe_code = "deny"`** with `#[allow(unsafe_code)]` at FFI boundaries — established pattern in 02-05 for cdylib + nix. Apply same shape for any unsafe vLLM FFI helpers if needed (likely none — PyO3 is the boundary).
- **DOCS-02 per-commit policy** — every commit modifying `crates/`, `python/`, or `xtask/` must touch docs/tests. CI enforces.
- **Conventional commits** — `feat(03-NN): ...`, `docs(03-NN): ...`, `fix(03-NN): ...`. Per-plan commits are atomic.
- **mdBook substrate chapters template** — 02-* shipped 8 chapters under `docs/book/src/substrate/`. Phase 3 adds chapters under `docs/book/src/inference/` following the same template (overview + per-component pages).

### Integration Points
- `crates/rollout-cli/src/main.rs` — add `infer batch` clap subcommand. Existing `coordinator run` / `worker run` / `schema` untouched.
- `crates/rollout-coordinator/src/lib.rs` — extend `CoordinatorImpl` to dispatch batch work via the Work channel and update sample-state. Or introduce a `BatchCoordinator` wrapper (planner picks).
- `crates/rollout-core/tests/dependency_direction.rs` — 4 invariants today (Phase 2 added). Phase 3 adds: `rollout-backend-vllm ↛ rollout-cloud-*` (backend has no business touching cloud crates directly; it gets ObjectStore via the algorithm-deps injection mechanism in Phase 4).
- `.github/workflows/ci.yml` — existing 12 jobs. Phase 3 adds an `infer-smoke` job (`needs: test`; `if: env.ROLLOUT_VLLM_AVAILABLE == '1'`). The default no-vllm CI runs unchanged.
- `examples/batch-tiny.toml` — the named exit-criterion config; planner ships it under `examples/` at the repo root.

</code_context>

<specifics>
## Specific Ideas

- **First "useful" thing matters.** Phase 3 is the first time `rollout` does something end users can grok ("run inference on a model and get completions"). The CLI UX should feel clean: `rollout infer batch --config examples/batch-tiny.toml` is the only command needed.
- **vLLM is heavy.** First-run downloads multi-GB CUDA wheels + model weights. The CLI should print clear progress ("Downloading Qwen2.5-0.5B-Instruct (1.0 GiB)...") and respect `~/.cache/huggingface/` for re-runs.
- **CPU mode must work.** AGENTS.md §7 is non-negotiable. The smoke test runs `rollout infer batch` on CPU with a 4-prompt × 16-token batch in <60 s. Documented in `docs/book/src/inference/cpu-mode.md`.
- **Resume is the win.** Killing the worker mid-batch and restarting must produce zero duplicates. Test this explicitly in the integration tests: 8-prompt batch, kill after 3 samples complete, restart, verify 5 remaining samples complete + 0 duplicates in output JSONL.
- **Phase 4 carries the baton.** Phase 4's SFT/RM crates will inject `rollout-backend-vllm` as `Arc<dyn InferenceBackend>` via `AlgoDependencies`. Phase 3 must keep the trait inference-shaped enough that Phase 4 can either extend it (forward/backward) or add a sibling trait (`TrainableBackend`) without churning Phase-3 code.
- **HF_TOKEN handling.** Gated models (anything Meta-licensed, etc.) require `HF_TOKEN`. Read via `ROLLOUT_SECRET_HF_TOKEN` allowlist + the env-var SecretStore from 02-03. Plan-time validation tries a model probe; fails fast if missing.
- **vLLM logging is noisy.** Enable `disable_log_stats=True` + `disable_log_requests=True` in the engine args; rollout's own observability fills the gap via the EventEmitter trait.
- **Sample-id idempotency is load-bearing.** A retry of a `Pending → Running → Failed` sample MUST produce the same ContentId so the resume scan picks it up correctly. Postcard's deterministic encoding + the canonical SamplingParams shape ensure this.
- **`examples/batch-tiny.toml` lives at the repo root.** Per spec 08's example invocations. Keep it small (4 prompts, 16 max_tokens, Qwen2.5-0.5B-Instruct).

</specifics>

<deferred>
## Deferred Ideas

- **Streaming generation** — Phase 8 (`INFER-01`, online inference + SSE).
- **Tool calling integrated into generation** — Phase 8 (`INFER-02`).
- **Training-mode forward/backward** — Phase 4 (`TRAIN-01..04`).
- **Multi-node coordinator / worker pool / work-stealing** — Phase 6 (`DIST-01..05`).
- **S3 / GCS object store backends** — Phase 5 (`CLOUD-01..03`); Phase 3 uses `FsObjectStore`.
- **Snapshot integration (training-state, buffer, process)** — Phases 4 / 9 / 11.
- **First-class tokenizer trait in `rollout-core`** — defer to Phase 4 if SFT/RM need it.
- **Reader/Writer worker split** — Phase 6 (when multi-node lands).
- **vLLM speculative decoding / prefix caching tuning** — performance tuning sub-phase post-Phase 9.
- **Inference backends beyond vLLM (SGLang, TGI, Candle)** — Phase 8 or later; spec 02 §11 leaves this open.
- **`rollout infer eval`** (eval-suite mode from spec 08 §2.5) — Phase 7 (`HARNESS-03`).
- **Episodic memory** — Phase 8 (`INFER-03`).

</deferred>

---

*Phase: 03-inference-batch*
*Context gathered: 2026-05-20 via `/gsd:discuss-phase 3 --auto`*
