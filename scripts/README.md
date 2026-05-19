# scripts/

Dev, CI, and ops scripts. Anything that isn't part of the framework runtime but is needed to develop, test, or operate it.

## Conventions

- Prefer **Rust** (a `cargo xtask` subcommand) over Bash for anything more than ~30 lines.
- Bash scripts use `set -euo pipefail` at the top, no exceptions.
- Python scripts use the `uv` toolchain (`#!/usr/bin/env -S uv run -q --script`).
- Every script's first lines are a short usage comment.

## Categories

```
scripts/
├── dev/            Developer-local helpers (e.g., spin up a local postgres, generate a fake dataset)
├── ci/             CI-only entrypoints (the CI pipeline calls these by name)
├── ops/            Operational helpers (rotate a credential, drain a queue, etc.)
└── codegen/        Anything that generates files in the repo (schema, stubs, fixtures)
```

## Examples (planned)

- `scripts/dev/local-stack.sh` — start localstack (S3 / SQS / Secrets Manager) + postgres for local integration tests.
- `scripts/dev/tiny-model.sh` — download a tiny test model used by all integration tests.
- `scripts/ci/test-matrix.sh` — entry point for CI, runs the right tests for the changed crates.
- `scripts/ops/migrate-run.sh` — export a run from embedded storage, import into postgres.

## Codegen

Generated artifacts live in the repo (so consumers see them in cargo doc / pypi), but they are produced by codegen scripts. CI verifies no drift.

Codegen entries:

- `cargo xtask schema-gen` — config schema (Rust → JSON Schema + .pyi).
- `cargo xtask abi-gen` — C ABI bindings for `rollout-plugin-abi`.
- `cargo xtask proto-gen` — gRPC proto-derived code (transport + sidecar).

## State: pre-implementation

Empty. Bootstrapped as needed by each phase.
