# Design Principles

These are the *why* behind every architectural choice in `rollout`. They are derived from concrete production pain we want to avoid. Each principle is stated, motivated, and given a falsifiability test — i.e., a way an implementation can demonstrably violate it.

When two principles conflict, the order they appear in this document is the tie-break.

---

## 1. Async-native end-to-end

**Statement:** Every operation that crosses a process or device boundary exposes an async API. Sync APIs exist only for test fixtures and local scripts. No async function may block on I/O, including via a sync wrapper.

**Why:** Sync I/O on async hot paths is one of the most common, hardest-to-detect performance bugs at scale. A single `time.sleep`-equivalent in an async loop costs orders of magnitude more throughput than it appears to. The cost is invisible until the system is loaded, by which point fixing it is expensive.

**Falsifiability test:** `cargo clippy` lints + a custom workspace lint that flags `std::fs`, `std::net`, and `std::thread::sleep` in any function reachable from an async entry point. The lint must be `deny`.

---

## 2. Batching is a first-class trait

**Statement:** Every operation that hits a GPU, a remote service, or a queue accepts a batch. Single-item APIs are convenience wrappers over the batch path.

**Why:** Per-item I/O wastes more compute than any other single mistake in ML systems. Worse: when single-item APIs are primary, batching becomes opt-in, and most callers never opt in.

**Falsifiability test:** Every public trait method in `rollout-core` that names a "sample" or "request" type takes a `&[Sample]` / `Vec<Request>`, not `Sample` / `Request`. Convenience wrappers are explicitly marked.

---

## 3. Plan-time validation

**Statement:** Errors that can be detected from config + plugin manifests + reachability are detected at `rollout plan`, not at runtime.

**Why:** Runtime errors that should have been config errors cost wall-clock + GPU time + on-call attention. The same error caught at plan time costs a few seconds of CLI execution.

**Categories that must be caught at plan time:**
- Config schema violations.
- Plugin manifest mismatches (declared trait vs runtime trait).
- DAG topology errors (cycles, type mismatches between connected harnesses).
- Resource budget infeasibility (requested GPUs > available GPUs).
- Storage / queue reachability.
- Secrets/credentials present and valid for the requested cloud.

**Falsifiability test:** an integration test for each category that fails-at-plan must continue to fail at plan. If any category regresses to "fails at run", CI fails.

---

## 4. Single source of truth for config

**Statement:** Rust types annotated with `serde` + `schemars` are the *only* authoritative config schema. JSON Schema, Python type stubs, CLI help, and editor completions are all *generated* from those types.

**Why:** Parallel schemas in different languages drift. Drift causes the worst class of bugs — silent acceptance of invalid configs that fail late. The fix isn't discipline; it's removing the second source.

**Falsifiability test:** `rollout schema --format json` produces output identical to the committed `rollout.schema.json` (CI diff check). `rollout schema --format python` produces a `.pyi` that mypy validates against the published Python API.

---

## 5. Deadline-based health, not interval polling

**Statement:** Liveness is established by `next_heartbeat_due_at` timestamps. Failure is established by the coordinator's monotonic clock passing that timestamp.

**Why:** Fixed-interval polling masks failures: a worker that died can sleep one full interval before being declared dead. Worse, race conditions between "should I poll now?" and "the next deadline" allow workers to be declared dead while alive, or alive while dead.

**Falsifiability test:** kill a worker via `SIGKILL`. Time-to-detection must be `<= heartbeat_interval + clock_skew_budget`, never `> 2 * heartbeat_interval`.

---

## 6. Composition over monoliths

**Statement:** Behaviors larger than ~300 lines of logic are built by composing smaller units with explicit, typed data contracts. Long methods, large config blobs, and conditional pipelines inside one trait impl are red flags.

**Why:** Monolithic transforms / handlers / managers cannot be tested independently, swapped, or hot-reloaded. They also cannot be safely modified by an agent without re-understanding the whole.

**Falsifiability test:** `clippy::too_many_lines` set tight (e.g., 120). Trait impls exceeding that need a documented justification comment with a tracking issue.

---

## 7. Every plugin is locally testable

**Statement:** A plugin's CI entry runs without network, without cloud credentials, and without a GPU. The framework provides the test fixtures necessary to do so.

**Why:** Plugins that require cloud creds to test cannot be tested by contributors, cannot run in standard CI, and accumulate undetected regressions. Local-test parity is what makes a plugin ecosystem viable.

**Falsifiability test:** the workspace CI runs every plugin's local test in a sandbox with no AWS / GCP / OpenAI credentials and no GPU. If any plugin fails because of a missing creds/GPU, the workspace fails.

---

## 8. Hot reload for plugins, not for core

**Statement:** Plugins can be reloaded into a running worker without restarting it. The core runtime cannot — core changes require a worker restart.

**Why:** Dev velocity for plugin authors requires reload. But reloading core is a complexity sink with no commensurate benefit — restarts are seconds; the bug surface from reloadable globals is months.

**Falsifiability test:** an end-to-end test runs `rollout plugins reload <name>` on a live worker, verifies the new code is executing, and confirms in-flight work was not interrupted.

---

## 9. Layered cloud abstraction

**Statement:** Cloud SDKs (`aws-sdk-*`, `google-cloud-*`) appear in **exactly one layer**: the cloud crates (`rollout-cloud-*`). Everything else depends on the cloud-facing traits from `rollout-core`.

**Why:** Cloud SDKs leak idioms, error types, and runtime assumptions. Letting them leak into algorithm code makes the algorithm code unportable, untestable, and brittle to SDK upgrades.

**Falsifiability test:** the workspace dependency lint asserts that no crate outside `crates/rollout-cloud-*` lists an `aws-sdk-*` or `google-cloud-*` dependency. A violation fails CI.

---

## 10. Observability is not optional

**Statement:** Every public operation emits a structured event with run/trace/span IDs. Metrics are continuous, not sampled by the operator. Run state is queryable through a documented API.

**Why:** "Add logging when something breaks" never works. By the time you need it, you've lost the data. Observability must be the default; opting *out* requires justification.

**Falsifiability test:** for every operation listed in `SKILLS.md`, a trace span exists in the test output. If a skill ships without a span, CI fails.

---

## Auxiliary principles (apply when the above don't decide)

### A1. **Static where possible, dynamic where necessary.**
Enum dispatch over trait objects when the set is closed. Trait objects only at plugin boundaries.

### A2. **Errors carry retry hints.**
`Recoverable` errors include a `RetryHint`. The retry policy is per-error, not per-call-site.

### A3. **Idempotency by content addressing.**
Every persisted artifact (sample, snapshot, trajectory) is content-addressed. Re-runs and retries dedupe naturally.

### A4. **Workspace-level lints, not per-crate.**
Lint config lives in the workspace `Cargo.toml`. A new crate inherits all standards. No crate may override to lower a lint.

### A5. **MSRV is set explicitly.**
The workspace declares a minimum supported Rust version. Bumping it requires a PR with rationale.

### A6. **No global state.**
No `lazy_static`, no `OnceCell::global()`. All state is passed in. The exception is the panic handler and the global tracing dispatcher (one each, set in main).

### A7. **Public API is `#[non_exhaustive]` by default.**
External users can match without binding themselves to today's variant set.

---

## How agents should use this document

When making a design choice, scan the principles in order. The first principle that addresses your choice decides it. If none apply, fall through to auxiliary principles. If still ambiguous, propose an ADR in `docs/adr/` and ask.

When reviewing code, the first thing to check is **which principle the change might violate**. Most code-review friction in ML systems is over things these principles already settle.
