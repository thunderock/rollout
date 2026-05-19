# Spec 00 — Overview

This is the index for `docs/specs/`. Each spec is an **implementation contract**: it tells an implementor exactly what to build, what trait surface to expose, how it composes with other components, and how to verify correctness.

## Spec index

| # | Title | Owns | Layer |
|---|---|---|---|
| 00 | Overview *(this doc)* | spec index + cross-cutting | — |
| 01 | Core runtime | async runtime, worker model, scheduling, lifecycle, heartbeats | 0–2 |
| 02 | Algorithms | PPO, GRPO, DPO/IPO/KTO, SFT, RM | 4 |
| 03 | Plugin system | PyO3 + sidecar dual-mode plugin host | 3 |
| 04 | Storage & snapshots | embedded + Postgres backends, object store, four snapshot kinds | 1–3 |
| 05 | Distribution | multi-node, transport, work-stealing, fault tolerance | 2 |
| 06 | Cloud layer | pluggable infra (AWS, GCP) traits and impls | 1 |
| 07 | Harnesses | env, tool/action, eval | 3 |
| 08 | CLI | command surface, config files, lifecycle | 5 |
| 09 | Observability | metrics, tracing, logging, run state | cross-cutting |
| 10 | Component split | crate map, PyPI packages, publishing | cross-cutting |
| 11 | Config schema | single source of truth: Rust → JSON Schema → Python stubs | 0 |

## How to read a spec

Every spec follows the same skeleton:

1. **Purpose** — what this component is and isn't.
2. **Trait surface** — the exact public API in `rollout-core`-compatible Rust.
3. **Lifecycle** — when methods are called and in what order.
4. **Composition** — what other components this one talks to.
5. **Configuration** — the `serde + schemars` types this component exposes.
6. **Failure modes** — known failure classes and how they manifest.
7. **Test contract** — what a passing test suite looks like.
8. **Open questions** — explicit unknowns, deferred to ADRs.

If a spec is missing one of these sections, that's a bug — file a PR.

## Cross-cutting contracts

Some contracts cut across every component and are stated here rather than repeated in each spec.

### Content addressing

Every durable artifact has a content-addressed ID:

```
ContentId = "<algo>-<hex>"   // "blake3-9f3a..."
```

- **Algo:** `blake3` (default), `sha256` (legacy interop only).
- **Hex:** lowercase, no padding, no separators.

Content IDs are compared bytewise. Two artifacts with the same content ID are interchangeable.

### Run IDs

```
RunId = "run-<ulid>"
```

ULID is monotonically sortable, time-prefixed, URL-safe. Generated at `rollout plan` time, frozen in the plan.

### Trace / span IDs

Standard OpenTelemetry format. Every public API call opens a span. Span attributes always include `run_id`, `worker_id`, and `plugin_id` when applicable.

### Error taxonomy

Errors in `rollout-core` form a closed hierarchy:

```rust
pub enum CoreError {
    Recoverable(RecoverableError),
    Fatal(FatalError),
}

pub enum RecoverableError {
    Throttled    { retry_hint: RetryHint },
    Transient    { retry_hint: RetryHint, source: BoxedError },
    Preempted    { retry_hint: RetryHint },
}

pub enum FatalError {
    ConfigInvalid    { field: String, message: String },
    SchemaViolation  { actual: String, expected: String },
    PluginContract   { plugin: PluginId, violation: String },
    Internal         { source: BoxedError },
}

pub enum RetryHint {
    Immediate,
    After(Duration),
    Jittered { base: Duration, max: Duration },
    Never,
}
```

Layer 1+ crates use their own error types but **must** implement `Into<CoreError>` so they can surface across boundaries with consistent retry semantics.

### Versioning

- **Crates:** semver. Pre-1.0, `0.x.y` where `x` is the breaking-change axis.
- **Config schema:** the schema is versioned via a top-level `schema_version` field. The framework refuses to load a config from a future schema version.
- **Plan files:** content-addressed; never mutated. Plan format itself is versioned via `plan_version`.

### Naming

- **Crates:** `rollout-<role>` for libraries, `rollout-<role>-<concrete>` for impls. E.g., `rollout-core`, `rollout-algo-ppo`, `rollout-cloud-aws`.
- **PyPI packages:** `rollout-<role>` (hyphenated). Imports use underscores: `import rollout_core`.
- **Types:** `PascalCase`. Traits as `PascalCase` describing the role (`PolicyAlgorithm`, not `IPolicyAlgorithm` or `PolicyAlgorithmTrait`).
- **Files:** `snake_case.rs` / `snake_case.py`.

## Cross-cutting MUSTs

Repeated here because every spec must obey them — restating them in each spec would be noise.

1. **All public APIs are documented.** A trait or function without a doc comment fails CI's `missing_docs` lint.
2. **All trait methods that hit I/O are `async`.** No exceptions.
3. **All trait methods that operate on collections are batch-shaped.** `&[T]` or `Vec<T>`, not `T`.
4. **Every spec'd struct that appears in config implements `Serialize + Deserialize + JsonSchema`.**
5. **Every component exposes a `Settings` config type and a `from_settings()` constructor.** No constructors that take more than a `Settings` + (optionally) a `Dependencies` struct.

## Open meta-questions

Tracked here so we don't lose them. Each will be resolved before its dependent phase starts.

- **Embedded KV store choice:** sled vs redb vs rocksdb. Decision deadline: Phase 2.
- **Async runtime pinning policy:** do we pin a tokio minor in the workspace? Decision deadline: Phase 1.
- **Process snapshot tool:** CRIU vs `criu-ns` vs custom. Decision deadline: Phase 11.
- **Distributed clock:** assume NTP, or build our own logical clock for split-brain prevention? Decision deadline: Phase 6.

Each becomes an ADR in `docs/adr/` when resolved.
