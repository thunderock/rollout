# rollout-cli

The `rollout` binary — the primary user surface for the rollout framework in v1.

## Status

Phase 1 skeleton. Only one subcommand resolves in this phase: `rollout schema --format json` (stub message; real wiring lands in plan 01-04). Every other CLI subcommand arrives in later phases.

## Usage

```bash
cargo run -p rollout-cli -- schema --format json
```

See [`docs/specs/08-cli.md`](../../docs/specs/08-cli.md) for the full CLI surface contract.
