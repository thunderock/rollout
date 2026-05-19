# rollout-core

Layer 0 of the rollout framework: the trait surface, ID types, error taxonomy, and config schema that every other crate builds against. Zero runtime dependencies beyond `serde`, `thiserror`, `schemars`, `tracing`, `ulid`, and `blake3` — no `tokio`, no cloud SDKs, no `pyo3`.

This is the single source of truth for rollout's public contract. JSON Schema (`schemas/rollout.schema.json`) and Python stubs (`python/rollout/_config_stubs.py`) are generated from this crate by `cargo xtask schema-gen`.

## Usage

Depend on `rollout-core` from any rollout crate that needs the trait surface, error types, or config schema:

```toml
[dependencies]
rollout-core = { path = "../rollout-core" }
```

See [`docs/specs/10-component-split.md`](../../docs/specs/10-component-split.md) for the layered architecture and [`docs/specs/11-config-schema.md`](../../docs/specs/11-config-schema.md) for the schema-gen contract.

## Status

Phase 1 skeleton — empty-but-compiles. Trait modules, error taxonomy, ID types, and config schema land in plan 01-03.
