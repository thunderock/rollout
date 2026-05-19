# Spec 09 — Observability

Observability is not an add-on. Per design principle 10, **every public operation emits a structured event with run / trace / span IDs.** This spec defines what gets emitted, how it's transported, and what the contract is with operators.

## 1. Three pillars

### 1.1 Metrics

Numeric, aggregated, continuous. Exposed via:

- **Prometheus** scrape endpoint at `/metrics` on every long-running process. Default port `9090`.
- **OpenTelemetry OTLP** push to a configurable collector.

Both can be enabled at once.

**Naming convention:** `rollout_<subsystem>_<metric>` with snake_case. Histograms use `_seconds` / `_bytes` suffixes; counters use `_total`.

Mandatory labels on every metric: `run_id`, `worker_role`. Conditional labels: `algorithm`, `phase` (rollout / learner / inference / eval), `plugin_id`.

### 1.2 Traces

OpenTelemetry-format spans. Every public API call opens a span. Cross-process spans are linked via the standard `traceparent` header (gRPC metadata).

Span attributes always include:

- `run_id`, `worker_id`, `plugin_id` (when applicable)
- `algorithm`, `phase`
- For batched ops: `batch_size`

Critical paths instrumented in v1:

- `plan.*` — full plan-time pipeline
- `worker.lifecycle.*` — init / ready / drain / shutdown
- `coord.heartbeat`, `coord.pull`, `coord.submit`, `coord.control`
- `algo.step` — one full PPO/GRPO/DPO/SFT step
- `backend.generate`, `backend.forward`, `backend.update`
- `harness.reset`, `harness.step`, `tool.invoke`
- `snapshot.save`, `snapshot.restore`
- `storage.*`, `object.*`, `queue.*`

### 1.3 Logs

Structured, JSON, append-only. Logs are events with `level` and `message`; they are emitted on the same channel as other events.

Levels: `trace` / `debug` / `info` / `warn` / `error`. Default: `info`. Configurable per-module.

## 2. Event model

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub ts:        DateTime<Utc>,
    pub kind:      EventKind,
    pub level:     Level,
    pub run_id:    Option<RunId>,
    pub worker_id: Option<WorkerId>,
    pub trace_id:  Option<TraceId>,
    pub span_id:   Option<SpanId>,
    pub plugin_id: Option<PluginId>,
    pub algorithm: Option<AlgorithmId>,
    pub message:   Option<String>,           // for log-style events
    pub attrs:     serde_json::Value,        // structured fields
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Log,                          // free-form log line
    Metric  { name: SmolStr, value: f64, unit: SmolStr },
    Span    { phase: SpanPhase },  // start | end
    Domain  { topic: SmolStr },    // domain events (plan.ok, snapshot.saved, etc.)
}
```

The event format is the same across all backends. Prometheus and OTLP exports are computed from the event stream.

## 3. Run state

In addition to ephemeral events, the framework persists structured run state to `Storage`:

```rust
pub struct RunSummary {
    pub run_id:      RunId,
    pub state:       RunState,        // planning | scheduled | running | drained | completed | failed
    pub plan_id:     PlanId,
    pub started_at:  Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub progress:    Progress,        // algorithm-specific structured progress
    pub last_error:  Option<TypedError>,
}
```

Updated at every state transition. Queryable via `rollout runs show`.

## 4. Transport

The event stream is emitted by every worker and the coordinator. Transport options:

1. **stdout (NDJSON)** — for `rollout run` foreground.
2. **Storage** — persisted to the metadata DB (table: `events`) for queryability via `rollout logs`.
3. **OTLP** — push to a collector.
4. **File** — append to a configurable path; useful when no collector is available.

A run can enable any combination. Production typically uses Storage + OTLP; dev uses stdout + Storage.

## 5. Cardinality discipline

Metrics with unbounded label cardinality kill Prometheus. The framework enforces:

- Labels are typed (`enum` where possible, not free strings).
- New labels are added via the registry, not ad-hoc.
- IDs like `run_id` and `worker_id` are **never** Prometheus labels — they go on traces / events only.

## 6. Plan-time observability hooks

`rollout plan` itself emits events:

- `plan.start`, `plan.validating_schema`, `plan.loading_plugins`, `plan.reachability_check`, `plan.computing_resources`, `plan.ok` / `plan.fail`.

This means even a failed `plan` is debuggable from the event log alone.

## 7. Plugin observability contract

Plugins inherit the host's observability:

- The host wraps every plugin call in a span (`plugin.<name>.<method>`).
- Plugin-internal events emitted via the `events` field of `PluginDependencies` are tagged automatically with `plugin_id`.
- Sidecar plugins propagate `traceparent` over gRPC metadata.

## 8. Opt-out

```bash
rollout --no-telemetry run --plan plan.lock
```

requires the config to include:

```toml
[telemetry]
disabled = true
disabled_reason = "air-gapped environment"
```

CI rejects opt-outs without a justification. Observability is not optional **by default**; it is opt-out **with reason**, never silent.

## 9. Failure modes

| Failure | Detection | Recovery |
|---|---|---|
| OTLP collector unreachable | export error counter increments | events buffer locally up to `buffer_max`; oldest dropped after; backoff retry |
| Prometheus scrape endpoint port conflict | bind error at startup | fatal at startup with descriptive error |
| Storage write of event fails | storage error | drop the event, increment `rollout_event_dropped_total` |
| Trace context lost across gRPC | span linkage missing | warn-level log; the trace is recoverable from run_id |

## 10. Test contract

- **Span coverage:** a CI test enumerates the operations in `SKILLS.md` and asserts each emits a span in the test trace.
- **Metric naming lint:** a CI check enforces the naming convention.
- **Cardinality lint:** any label whose values are not type-bounded fails the check.
- **No-telemetry opt-out:** integration test asserts the rejection without `disabled_reason`.

## 11. Dashboards

The repo ships starter dashboards (Grafana JSON) in `docs/dashboards/`:

- **Run overview** — state, throughput, error rate.
- **Rollout phase** — GPU utilization, tokens/sec, KL.
- **Learner phase** — step time, gradient norm, optimizer state.
- **Plugin health** — per-plugin call count, latency, error rate.
- **Cloud health** — object store / queue latency, error rate.

These are the same dashboards CI exercises when validating the perf bar in Phase 9.

## 12. Open questions

- **Trace sampling:** v1 always-on; sampling at ingest. Document the perf cost of always-on; revisit if it becomes a bottleneck.
- **Per-sample audit log:** for compliance use cases. Out of v1 scope; design space note here so we don't paint ourselves into a corner.
- **Log search backend:** v1 uses the metadata DB. ElasticSearch / Loki integration deferred.
