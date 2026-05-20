# `rollout-transport` — gRPC plane with mTLS

`rollout-transport` is the worker ↔ coordinator gRPC plane for Phase 2. It
ships **HTTP/2 + rustls + mTLS** as the plan-of-record. QUIC support is
**experimental** and lives behind a Cargo feature flag.

## Plan-of-record: HTTP/2 + rustls

Phase 2 research (`.planning/phases/02-local-substrate/02-RESEARCH.md`,
"Transport stack") found `tonic-h3` v0.0.5 (latest at planning time,
2025-11-01) was explicitly labelled experimental, with bidirectional
streaming not documented as supported. The upstream
[`hyperium/tonic#339`](https://github.com/hyperium/tonic/issues/339) gRPC-over-HTTP/3
issue remains open. Shipping a `Work` channel (bidi-streaming) on top of
`tonic-h3` would risk runtime hangs under load.

Therefore plan 02-04 ships HTTP/2 tonic 0.14 + rustls 0.23 + rcgen 0.13.
The same `.proto` schema is forward-compatible with QUIC; the swap will
become a single Cargo-feature flip in a later phase once `tonic-h3` (or
its successor) ships documented bidi support.

## Three logical channels

All three channels are multiplexed over one HTTP/2 connection per
(coordinator, worker) pair, per spec
[`05-distribution.md`](../../../docs/specs/05-distribution.md) §3.

| Channel   | RPC kind            | Purpose                                          |
|-----------|---------------------|--------------------------------------------------|
| Heartbeat | unary, frequent     | Worker's "I'm alive" ping; carries `due_at`      |
| Control   | server-streaming    | Coordinator pushes drain / snapshot / cancel     |
| Work      | bidirectional       | Phase-2 stub; Phase 6 wires pull/submit          |

The Work channel ships as a wired-but-stub `WorkServiceImpl` that echoes a
heartbeat marker back on every received frame. Real pull/submit semantics
arrive with DIST-01..02 in Phase 6.

## mTLS auto-bootstrap (D-TRANS-02)

On first run the transport invokes `tls::ensure_dev_ca(tls_dir)`. This:

1. Generates a self-signed CA via `rcgen 0.13`.
2. Writes `ca.pem` (public certificate) and `ca.key.pem` (private key).
3. Sets `ca.key.pem` permissions to `0o600` on Unix.

Subsequent calls are idempotent — the existing files are read back.

`tls::issue_server_cert(ca_cert, ca_key, dns_names)` issues server certs
(EKU `ServerAuth`). `tls::issue_client_cert(ca_cert, ca_key, names)`
issues client certs (EKU `ClientAuth`). Both are signed by the dev CA.

Filenames live under `./data/tls/` (gitignored). No manual `openssl`
steps are required; rcgen produces pure-Rust output with no
`openssl-sys` dependency (banned by `deny.toml`).

## Deadline-based health (D-TIME-01..02)

Spec
[`05-distribution.md`](../../../docs/specs/05-distribution.md) §6
mandates **deadline-based** health, not polling. Workers publish
`due_at = now + heartbeat_interval × 2` on every Beat; coordinators scan
worker state and mark failure only when:

```text
elapsed_past_due > clock_skew_budget  AND  elapsed_past_due > coordinator_failure_timeout
```

Helpers `health::next_due_at` and `health::is_failed` encode the formulas
above. They take `SystemTime` directly so callers can swap a test clock
trivially.

Default constants (D-TIME-01):

| Constant                       | Default |
|--------------------------------|---------|
| `heartbeat_interval`           | 500 ms  |
| `worker_self_fence_timeout`    | 4 s     |
| `coordinator_failure_timeout`  | 5 s     |
| `clock_skew_budget`            | 250 ms  |

## Config invariants enforced at plan time (D-TIME-02)

`TransportConfig::validate_cross_fields` enforces two split-brain
prevention rules at plan time, never at runtime:

1. `worker_self_fence_timeout < coordinator_failure_timeout` — a worker
   that fails to self-fence before the coordinator times it out causes
   classic split-brain (two writers think they own the same lease).
2. `clock_skew_budget < heartbeat_interval × 2` — a clock-skew budget
   greater than two periods would make deadline-based failure detection
   meaningless.

`rollout plan` calls `validate_cross_fields` before any worker starts;
violations return `Fatal(ConfigInvalid)` with a human-readable message
identifying the offending field.

## QUIC feature flag (EXPERIMENTAL)

The `quic` Cargo feature pulls in `quinn`, `tonic-h3`, `h3`, and
`h3-quinn`:

```toml
[features]
default = ["h2"]
h2 = []
quic = ["dep:quinn", "dep:tonic-h3", "dep:h3", "dep:h3-quinn"]
```

At plan-02-04 execution time (2026-05-20), `cargo build -p
rollout-transport --features quic` fails to compile because `h3-quinn
0.0.7` references `quinn::StreamId` internals that became private in
`quinn 0.11.x`. This confirms the RESEARCH §"Pitfall 2" assessment:
**the QUIC stack is not production-ready**.

When `tonic-h3` (or a successor) ships documented bidi-streaming and
publishes a `quinn 0.11`-compatible release, the swap is:

1. Verify `cargo build --features quic` succeeds.
2. Implement `server::serve_quic` (currently returns
   `Fatal(Internal)` with an EXPERIMENTAL message).
3. Add a matching `client::build_quic_channel`.
4. Flip the default feature, or expose a CLI flag.

The proto schema does not change.

## Channel: Work (Phase-2 stub)

`work::WorkServiceImpl::stream` accepts a bidi stream and echoes a
"heartbeat" marker per inbound frame. This is enough for the Phase-2
smoke test in plan 02-07 to verify that the bidi pipe is wired
end-to-end. Real pull/submit semantics — assignment, ack, work-stealing,
restart-from-storage — arrive with DIST-01..02 in Phase 6.

## Observability (principle #10)

Every server handler is wrapped in a `tracing::instrument` span with a
`channel` field. Emitted events include:

- `transport_server_starting` (with `%addr`)
- `mtls_handshake_bootstrap` (when CA is generated)
- `heartbeat_received` (worker_id, run_id, state)
- `control_subscribed` (worker_id, run_id)

Binary crates configure the subscriber; library crates emit only — per
D-OBSERVE-01.

## Cross-references

- [`docs/specs/05-distribution.md`](../../../docs/specs/05-distribution.md) §3, §6
- [`crates/rollout-proto/proto/transport.proto`](../../../crates/rollout-proto/proto/transport.proto)
- [`.planning/phases/02-local-substrate/02-CONTEXT.md`](../../../.planning/phases/02-local-substrate/02-CONTEXT.md) D-TRANS-01..03, D-TIME-01..02
- [`.planning/phases/02-local-substrate/02-RESEARCH.md`](../../../.planning/phases/02-local-substrate/02-RESEARCH.md) §"Transport stack", Pitfall 2
