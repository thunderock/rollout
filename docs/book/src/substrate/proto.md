# `rollout-proto` — wire-format crate

`rollout-proto` owns the workspace's `.proto` files and runs `tonic-build` exactly
once. Downstream crates (`rollout-transport`, `rollout-plugin-host`,
`rollout-coordinator`) depend on `rollout-proto`, not on `.proto` regeneration.

## Why a dedicated crate

Per CONTEXT [D-PROTO-01](../../../.planning/phases/02-local-substrate/02-CONTEXT.md),
the workspace has a single `tonic-build` invocation site. Centralising wire-format
ownership means:

- The HTTP/2 ↔ QUIC swap (CONTEXT
  [D-TRANS-01](../../../.planning/phases/02-local-substrate/02-CONTEXT.md) fallback)
  shares the same source-of-truth proto schema.
- The sidecar IPC protocol (UDS) and the network transport carry the same message
  types — no drift between in-process and out-of-process plugin calls.
- Build-time codegen happens once per workspace build; downstream crates compile
  faster.

## Wire-format ownership

| File | Package | Owns |
|---|---|---|
| `proto/transport.proto` | `rollout.transport.v1` | Worker ↔ Coordinator transport |
| `proto/plugin.proto` | `rollout.plugin.v1` | Host ↔ sidecar Plugin protocol |

Both are committed to the repo. The build script (`build.rs`) compiles them via
`tonic-prost-build` on every workspace build; `rerun-if-changed` keeps incremental
builds fast.

## Three transport channels

`transport.proto` defines the three logical channels from
[spec 05 §3](../../specs/05-distribution.md):

| Channel | RPC kind | Cadence | Purpose |
|---|---|---|---|
| `Heartbeat` | unary `Beat` | every 500 ms | Worker liveness; carries `due_at` for deadline-based health |
| `Control` | server-stream `Subscribe` | as-needed | Coordinator pushes drain / snapshot / cancel |
| `Work` | bidi `Stream` | bursty | Pull/submit work items (full semantics land in Phase 6) |

All three multiplex over one connection — HTTP/2 today, QUIC behind a feature
flag in a later phase.

## Plugin sidecar protocol

`plugin.proto` defines the five sidecar RPCs from
[spec 03 §3.3 + §4](../../specs/03-plugin-system.md):

- `Init(InitRequest) -> InitResponse` — one-time setup per worker.
- `Preflight(PreflightRequest) -> PreflightResponse` — optional, after `Init`.
- `Call(CallRequest) -> CallResponse` — generic typed entry point; `payload` is
  opaque bytes (JSON or postcard, plugin-defined).
- `Reload(ReloadRequest) -> ReloadResponse` — hot reload signal; the sidecar
  may refuse.
- `Shutdown(ShutdownRequest) -> ShutdownResponse` — clean exit with grace period.

The Python sidecar host implementation lands in
[`rollout-plugin-host`](./plugin-host.md) (plan 02-05).

## Python stubs

The in-tree Python sample sidecar
(`python/examples/sample_sidecar/`) uses **stdlib length-prefixed JSON framing**
over UDS — per AGENTS.md §7, every plugin must be testable locally without
`pip install grpcio` or other external services. See
[`02-RESEARCH.md` §"Python sidecar IPC: avoid pip"](../../../.planning/phases/02-local-substrate/02-RESEARCH.md)
for the rationale.

If you are authoring a Python plugin that does want real gRPC, opt in via
`make protos` (which shells to `cargo xtask gen-protos`). Output goes to
`python/rollout/_proto/`. The xtask gracefully degrades when `grpcio-tools` is
missing — it prints a helpful message and exits 0; CI does not require it.

```bash
pip install grpcio-tools     # opt-in
make protos                  # writes python/rollout/_proto/*_pb2.py
```

## Versioning

Both protos use a `v1` package suffix
(`rollout.transport.v1`, `rollout.plugin.v1`). Bumping to `v2` is a future spec
edit; the `v1` modules will continue to compile alongside any future versions so
in-flight Phase 2 deployments do not break.

## Cross-references

- [spec 03 — plugin system](../../specs/03-plugin-system.md) §3.3, §4
- [spec 05 — distribution / transport](../../specs/05-distribution.md) §3, §6
- CONTEXT [D-PROTO-01](../../../.planning/phases/02-local-substrate/02-CONTEXT.md)
  for the "single tonic-build site" decision
