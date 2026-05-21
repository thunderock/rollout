# Spec 10 — Component split

This spec defines the v1 crate map, the corresponding PyPI packages, the dependency graph, and the publishing strategy. The split is **aggressive on purpose**: small components compose into larger ones, are reusable by external projects, and force boundary discipline.

## 1. Crate map

12 Rust crates in v1.

| # | Crate | Layer | Depends on | Purpose |
|---|---|---|---|---|
| 1 | `rollout-core` | 0 | (none beyond `serde`, `thiserror`, `schemars`, `tracing`) | Traits, types, errors, config schema |
| 2 | `rollout-cloud-aws` | 1 | core | AWS impls of cloud traits |
| 3 | `rollout-cloud-gcp` | 1 | core | GCP impls of cloud traits |
| 4 | `rollout-cloud-local` | 1 | core | Local-fs / in-mem impls for tests + single-node |
| 5 | `rollout-storage` | 2 | core | Embedded + Postgres `Storage` impls |
| 6 | `rollout-transport` | 2 | core | gRPC over QUIC inter-node transport |
| 7 | `rollout-backend-vllm` | 2 | core | vLLM `InferenceBackend` impl |
| 8 | `rollout-plugin-host` | 3 | core | PyO3 + sidecar plugin host |
| 9 | `rollout-snapshots` | 3 | core, storage | Four-flavor snapshot system |
| 10 | `rollout-harness-text` | 3 | core, plugin-host | In-tree text env harness |
| 11 | `rollout-harness-tool` | 3 | core, plugin-host | In-tree tool harness with sandboxed tools |
| 12 | `rollout-evals` | 3 | core, plugin-host | Eval harness runner + bundled evals |

Plus 5 algorithm crates:

| # | Crate | Depends on |
|---|---|---|
| 13 | `rollout-algo-ppo` | core, snapshots, plugin-host |
| 14 | `rollout-algo-grpo` | core, snapshots, plugin-host |
| 15 | `rollout-algo-dpo` | core, snapshots, plugin-host |
| 16 | `rollout-algo-sft` | core, snapshots, plugin-host |
| 17 | `rollout-algo-rm` | core, snapshots, plugin-host |

Plus 3 surface crates:

| # | Crate | Depends on |
|---|---|---|
| 18 | `rollout-cli` | everything in the workspace |
| 19 | `rollout-py` | core, plugin-host, algos (PyO3 bindings) |
| 20 | `rollout-runtime` | core, storage, transport, plugin-host, snapshots |

Plus internal-only:

| # | Crate | Purpose |
|---|---|---|
| 21 | `rollout-plugin-abi` | C-ABI shim used by Rust cdylib plugins |
| 22 | `rollout-test-fixtures` | Shared test fixtures for plugins (`mock_inference_backend`, etc.) |
| 23 | `rollout-cloud-tests` | Compliance suite for cloud impls |

Total: **23 crates**, of which **17 are publishable** to crates.io.

## 2. Dependency graph (publishable subset)

```
                              rollout-core
                                  ▲
              ┌───────────────────┼────────────────────┬─────────────┐
              │                   │                    │             │
      rollout-cloud-*      rollout-storage   rollout-transport   rollout-backend-vllm
              │                   │                    │             │
              └─────────┬─────────┘                    │             │
                        ▼                              │             │
                  rollout-snapshots                    │             │
                        ▲                              │             │
                        │                              │             │
              rollout-plugin-host  ◀──────────────────┘              │
                        ▲                                            │
              ┌─────────┼─────────┬────────────┐                     │
              │         │         │            │                     │
       rollout-harness-* rollout-evals  rollout-algo-*  ─────────────┘
                                            │
                                            └──▶ rollout-runtime ──▶ rollout-cli
                                                                     rollout-py
```

The lint enforced in CI: **no upward arrows** (a lower-layer crate cannot depend on a higher one).

Invariants #7 (algo ↛ cloud), #8 (algo ↛ transport), #9 (snapshots ↛ algo) added in Phase 4. See `crates/rollout-core/tests/dependency_direction.rs` for the full list (Phases 1-4 fold into nine machine-checked invariants).

## 3. PyPI packages

Python publishing mirrors the crate split but consolidates where ergonomics matter.

| PyPI package | Wraps | Notes |
|---|---|---|
| `rollout` | `rollout-py` | The main package. Plan / run / inspect from Python. |
| `rollout-plugins` | `rollout-plugin-host` + bundled in-tree plugin base classes | What plugin authors import. |
| `rollout-eval-mmlu` | bundled eval | Each in-tree eval is its own PyPI package for selective install. |
| `rollout-eval-ifeval` | bundled eval | |
| `rollout-eval-gsm8k` | bundled eval | |

Each `rollout-eval-*` is a thin wrapper around a corresponding crate; the actual eval logic is implemented in Rust where it benefits, in Python where it doesn't.

User-authored Python plugins publish to PyPI under arbitrary names; they implement the `rollout_plugins.Plugin` Python ABC.

## 4. Publishing strategy

### 4.1 Pre-1.0 versioning

All crates and PyPI packages share a synchronized minor version line. At Phase 12 we cut `0.1.0` simultaneously across the workspace.

The `0.x` → `0.(x+1)` bump is allowed to contain breaking changes. We commit to API stability at `1.0`.

### 4.2 Release process

1. **CI green** on the release branch.
2. **Compliance suite passes** against real AWS + GCP nightly.
3. **Reference recipe** runs end-to-end in CI on a small model.
4. **`cargo publish` in dependency order** (core first, surfaces last). `cargo-workspaces` handles the topological sort.
5. **`maturin publish` for PyPI packages** in the same order.
6. **Tag the workspace**: `v0.x.y`.

### 4.3 Yanking

If a release has a critical bug, yank with `cargo yank`. Yanked versions are kept in the registry but no new resolution picks them. PyPI's equivalent (`pip` does not resolve yanked versions unless explicitly requested).

### 4.4 Documentation

`docs.rs` is the authoritative API docs site for crates. For PyPI, we host a parallel docs site built with mkdocs (Phase 12). All docs are generated from source comments — no parallel hand-written docs that can drift.

## 5. Library usage examples

### From a third-party Rust project

```toml
[dependencies]
rollout-core = "0.1"
rollout-algo-ppo = "0.1"
rollout-backend-vllm = "0.1"
rollout-storage = "0.1"
rollout-cloud-aws = "0.1"
rollout-runtime = "0.1"
```

```rust
use rollout_core::{Plan, RunId};
use rollout_runtime::Runtime;

let plan = Plan::from_file("ppo.toml").await?;
let runtime = Runtime::from_plan(&plan).await?;
runtime.run().await?;
```

### From a third-party Python project

```bash
pip install rollout
```

```python
from rollout import Plan, Runtime

plan = Plan.from_file("ppo.toml")
runtime = Runtime.from_plan(plan)
runtime.run()
```

The Python API is intentionally a thin mirror of the Rust API; same names, same shapes. Differences are documented in the Python package's README.

### Composing one component without the framework

A user might only want `rollout-snapshots` for their own training loop:

```toml
[dependencies]
rollout-snapshots = "0.1"
rollout-cloud-aws = "0.1"     # for the S3-backed object store
```

The dependency graph is shallow enough that this works without dragging in the algorithm crates or the runtime.

## 6. Feature flags

Cargo feature flags are used sparingly. Allowed cases:

- **Backend selection** in `rollout-cli`: features `vllm` (default), `sglang`, `candle`. Multi-backend builds enabled by listing features.
- **Cloud selection** in `rollout-cli`: features `aws`, `gcp`, `local` (default = all-of-the-above for the default build, but installers can slim).
- **Plugin host modes** in `rollout-plugin-host`: features `pyo3` (default), `sidecar` (default). Disabling one is rare but supported (e.g., for a Rust-only deployment).

Public API surface inside a crate **does not** change with feature flags. Features turn on/off implementations, not types.

## 7. Workspace `Cargo.toml`

A single workspace `Cargo.toml` at the repo root:

```toml
[workspace]
members = [
    "crates/rollout-core",
    "crates/rollout-cloud-aws",
    "crates/rollout-cloud-gcp",
    "crates/rollout-cloud-local",
    "crates/rollout-storage",
    "crates/rollout-transport",
    "crates/rollout-backend-vllm",
    "crates/rollout-plugin-host",
    "crates/rollout-snapshots",
    "crates/rollout-harness-text",
    "crates/rollout-harness-tool",
    "crates/rollout-evals",
    "crates/rollout-algo-ppo",
    "crates/rollout-algo-grpo",
    "crates/rollout-algo-dpo",
    "crates/rollout-algo-sft",
    "crates/rollout-algo-rm",
    "crates/rollout-runtime",
    "crates/rollout-cli",
    "crates/rollout-py",
    "crates/rollout-plugin-abi",
    "crates/rollout-test-fixtures",
    "crates/rollout-cloud-tests",
]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2021"
rust-version = "1.78"
license = "MIT"
repository = "https://github.com/<owner>/rollout"

[workspace.lints.rust]
missing_docs = "warn"
unsafe_code = "forbid"           # except in `rollout-plugin-abi` which opts in

[workspace.lints.clippy]
all = { level = "warn", priority = -1 }
pedantic = { level = "warn", priority = -1 }
missing_errors_doc = "allow"
module_name_repetitions = "allow"
```

## 8. Workspace lints / deny rules

Workspace-level `deny.toml` (used by `cargo deny`) enforces:

- `aws-sdk-*` and `google-cloud-*` only allowed inside `crates/rollout-cloud-aws` / `crates/rollout-cloud-gcp`.
- No GPL-licensed transitive deps.
- MSRV pinned; bump requires a PR.

CI runs `cargo deny check` on every PR.

## 9. Test contract

- **CI builds every crate independently** (no inherited compilation context). Validates that each is `cargo build` / `cargo test`-able on its own.
- **CI publishes a dry-run** (`cargo publish --dry-run`) for every publishable crate on every PR — catches publishing-blockers (missing license, missing README, etc.) before release.
- **PyPI build** via `maturin build --release` runs in CI on Linux + macOS.

## 10. Open questions

- **Cargo workspace vs cargo unstable per-crate publish:** workspace in v1. If publish flakiness materializes, revisit.
- **PyPI wheel matrix:** Linux x86_64 + aarch64, macOS x86_64 + arm64. Windows deferred (Linux-only sandboxing means tool harness is partial on Windows).
- **Crate splitting more aggressively post-v1:** `rollout-algo-dpo` could split into `-dpo`, `-ipo`, `-kto` if users actually need that selectivity. Track requests; don't preemptively split.
