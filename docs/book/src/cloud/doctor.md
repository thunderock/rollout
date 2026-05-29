# rollout cloud doctor

Operator pre-flight tool that exercises all four cloud traits (object store,
queue, secret store, compute hint) against either AWS or GCP before a real
training job runs. Addresses **CLOUD-04** (D-DOCTOR-01..04).

## Usage

```bash
# Build with the provider feature(s) you need.
cargo run -p rollout-cli --features aws -- cloud doctor --provider aws --config examples/sft-tiny-aws.toml
cargo run -p rollout-cli --features gcp -- cloud doctor --provider gcp --config examples/sft-tiny-gcp.toml --format json
```

Config source is the TOML `[cloud]` block only (D-DOCTOR-04) — there are no
`--bucket`/`--queue`/`--secret-id` flag overrides in v1.1. The `--provider`
flag MUST match the `[cloud].provider` in the TOML, or doctor exits `2`.

## Checks (in order)

1. **reachability** — TCP + TLS handshake to the service endpoint
   (`s3.<region>.amazonaws.com` / `storage.googleapis.com`). Surfaces DNS /
   firewall issues distinctly from auth failures.
2. **auth** — credential-chain probe (a cheap metadata read that requires the
   resolved credentials). Catches broken AWS credential chains / GCP ADC.
3. **object_store** — small payload PUT + GET roundtrip on the configured bucket.
4. **queue** — `enqueue` → `dequeue_with_lease(30s)` → `ack` on the configured queue.
5. **secret_store** — read the FIRST allowlisted secret (`[cloud.*.secrets].allowlist`).
   An empty allowlist is reported as a failure with remediation guidance.
6. **compute_hint** — `inventory()` + `preemption_signal()` probe (returns `Ok(None)`
   off a cloud instance).
7. **content_id_roundtrip** — a 64 MiB random buffer through `put_stream` +
   `get_stream` + blake3 verify. Forces the multipart / resumable path; catches
   blake3-streaming bugs (Pitfall 16 / D-SNAP-04).

Wall-time target: ~5-10s on a healthy environment.

## Exit codes (D-DOCTOR-03)

- `0` — all checks pass.
- `1` — at least one check failed (use `--format json` to see which).
- `2` — invocation / config error (provider mismatch, missing TOML, malformed schema).

Plays well with shell `&&`:

```bash
rollout cloud doctor --provider aws --config production.toml && \
  rollout train sft --config production.toml
```

## Output formats (D-DOCTOR-02)

- `--format human` (default): colored steps with `✓`/`✗` icons + per-check
  latency + a `N pass, M fail — total <ms>` summary line.
- `--format json`: machine-readable; matches the schema in
  `crates/rollout-cli/src/commands/cloud/doctor/output/json.rs`:

  ```json
  {
    "checks": [
      { "name": "reachability", "status": "pass", "latency_ms": 142 },
      { "name": "queue", "status": "fail", "latency_ms": 31, "error": "enqueue: ..." }
    ],
    "summary": { "pass_count": 6, "fail_count": 1, "total_latency_ms": 5443 }
  }
  ```

  `error` is omitted on passing checks.

## Limitations (v1.1)

- Config-file-only (D-DOCTOR-04); no `--bucket`/`--queue`/`--secret-id` overrides.
- One comprehensive mode (D-DOCTOR-01); no `--quick`/`--deep` tiers.
- Cross-cloud (both `[cloud.aws]` and `[cloud.gcp]` in one TOML) is structurally
  impossible — `CloudConfig` is a `#[serde(tag = "provider")]` enum (D-XPROV-02).

## CI coverage

The `doctor_smoke` integration tests run on every PR:

- `cloud-emulator-aws` runs `doctor_smoke_aws_*` against localstack with
  pre-created bucket / queue / secret (exit 0 all-pass, exit 1 unreachable,
  human + JSON shape).
- `cloud-emulator-gcp` runs `doctor_smoke_gcp_*` against fake-gcs-server +
  pubsub-emulator with pre-created bucket / topic / subscription.
- The config-layer tests (provider mismatch → exit 2, malformed config → exit 2)
  and the `--help` golden run Docker-free on every PR.
