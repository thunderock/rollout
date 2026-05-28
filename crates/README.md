# crates/

The Rust workspace. Each subdirectory is one crate. The full crate map, dependency graph, and publishing strategy live in [`/docs/specs/10-component-split.md`](../docs/specs/10-component-split.md) — this README is a quick reference.

## Layout

```
crates/
├── rollout-core/              Layer 0 — traits, types, errors, config schema
│
├── rollout-cloud-aws/         Layer 1 — AWS impls of cloud traits
├── rollout-cloud-gcp/         Layer 1 — GCP impls of cloud traits
├── rollout-cloud-local/       Layer 1 — local-fs / in-mem impls
│
├── rollout-storage/           Layer 2 — embedded + Postgres Storage
├── rollout-transport/         Layer 2 — gRPC-over-QUIC transport
├── rollout-backend-vllm/      Layer 2 — vLLM InferenceBackend
│
├── rollout-plugin-host/       Layer 3 — PyO3 + sidecar plugin host
├── rollout-snapshots/         Layer 3 — four-flavor snapshots
├── rollout-harness-text/      Layer 3 — text env harness
├── rollout-harness-tool/      Layer 3 — sandboxed tool harness
├── rollout-harness-eval/      Layer 3 — eval runner + bundled evals
│
├── rollout-algo-ppo/          Layer 4 — PPO
├── rollout-algo-grpo/         Layer 4 — GRPO
├── rollout-algo-dpo/          Layer 4 — DPO / IPO / KTO
├── rollout-algo-sft/          Layer 4 — SFT
├── rollout-algo-rm/           Layer 4 — reward model
│
├── rollout-runtime/           Layer 5 (lib) — composes storage+transport+plugins
├── rollout-cli/               Layer 5 (bin) — the `rollout` binary
├── rollout-py/                Layer 5 (lib) — PyO3 bindings
│
├── rollout-plugin-abi/        Internal — C ABI shim for Rust cdylib plugins
├── rollout-test-fixtures/     Internal — shared test fixtures
└── rollout-cloud-tests/       Internal — cloud compliance suite
```

## Per-crate conventions

Each crate has:

- `Cargo.toml` inheriting from the workspace.
- `src/lib.rs` (or `main.rs` for bin crates).
- `README.md` describing what the crate does in one paragraph and how it's used as a dependency.
- `tests/` for integration tests where applicable.
- For plugin crates: a `local-test/` sub-package proving the plugin-local-test contract.

## Dependency lint

A workspace lint enforces dependency direction. A crate cannot depend on a higher-layer crate. Violations break the workspace build.

See [`/docs/design-principles.md`](../docs/design-principles.md) §9 for the rule and [`/docs/specs/10-component-split.md`](../docs/specs/10-component-split.md) §8 for the enforcement mechanism.

## Adding a new crate

1. Decide its layer (see [`/ARCHITECTURE.md`](../ARCHITECTURE.md) §1).
2. Add the dir + `Cargo.toml` + `src/lib.rs` + `README.md`.
3. Add to the workspace `members` list.
4. Verify `cargo build` and `cargo test` from the workspace root.
5. If it will be published, add to the publish dependency order in `cargo-workspaces` config.
6. Add an entry to [`/docs/specs/10-component-split.md`](../docs/specs/10-component-split.md).

## State: pre-implementation

This directory is currently empty. Phase 1 ([`/ROADMAP.md`](../ROADMAP.md)) populates `rollout-core`. Subsequent phases fill in the rest in order.
