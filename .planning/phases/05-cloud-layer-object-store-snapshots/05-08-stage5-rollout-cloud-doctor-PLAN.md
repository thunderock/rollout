---
phase: 05-cloud-layer-object-store-snapshots
plan: 08
type: execute
wave: 5
depends_on: [07]
files_modified:
  - crates/rollout-cli/src/main.rs
  - crates/rollout-cli/src/commands/cloud/mod.rs
  - crates/rollout-cli/src/commands/cloud/doctor/mod.rs
  - crates/rollout-cli/src/commands/cloud/doctor/checks.rs
  - crates/rollout-cli/src/commands/cloud/doctor/config.rs
  - crates/rollout-cli/src/commands/cloud/doctor/output/mod.rs
  - crates/rollout-cli/src/commands/cloud/doctor/output/human.rs
  - crates/rollout-cli/src/commands/cloud/doctor/output/json.rs
  - crates/rollout-cli/tests/doctor_smoke.rs
  - docs/book/src/cloud/doctor.md
  - docs/book/src/SUMMARY.md
  - .github/workflows/ci.yml
autonomous: true
requirements: [CLOUD-04, DOCS-01, DOCS-02, DOCS-03]
gap_closure: false
must_haves:
  truths:
    - "`rollout cloud doctor --provider aws --config examples/sft-tiny-aws.toml` runs the 7 named checks and exits 0 on a green localstack."
    - "`rollout cloud doctor --provider gcp --config examples/sft-tiny-gcp.toml` runs the 7 named checks and exits 0 on a green fake-gcs-server + pubsub-emulator + mock secret manager."
    - "Exit codes follow D-DOCTOR-03: 0 = all pass; 1 = any check failed; 2 = invocation/config error."
    - "`--format human` (default) produces colored step-by-step output; `--format json` produces `{checks: [...], summary: {...}}` per D-DOCTOR-02."
    - "Check #7 (ContentId roundtrip) puts a 64 MiB random buffer via put_stream, reads back via get_stream, verifies blake3 hash — covers the multipart/resumable path."
    - "Provider mismatch (--provider aws + [cloud.gcp] block in TOML) exits 2 with a clear error."
    - "doctor_smoke integration tests run on every PR via the cloud-emulator-{aws,gcp} CI jobs (always-on, no live cloud)."
  artifacts:
    - path: "crates/rollout-cli/src/commands/cloud/doctor/mod.rs"
      provides: "DoctorCmd::run() entry; CLI surface `rollout cloud doctor --provider <aws|gcp> --config <path> [--format <human|json>]`"
      contains: "pub struct DoctorArgs"
    - path: "crates/rollout-cli/src/commands/cloud/doctor/checks.rs"
      provides: "7 named check functions: reachability, auth, object_store, queue, secret_store, compute_hint, content_id_roundtrip"
      contains: "async fn run_all_checks"
    - path: "crates/rollout-cli/src/commands/cloud/doctor/output/human.rs"
      provides: "Colored ANSI step-by-step output with ✓/✗"
      contains: "print"
    - path: "crates/rollout-cli/src/commands/cloud/doctor/output/json.rs"
      provides: "serde_json serializer for {checks: [{name, status, latency_ms, error?}], summary: {pass_count, fail_count, total_latency_ms}}"
      contains: "summary"
    - path: "crates/rollout-cli/tests/doctor_smoke.rs"
      provides: "Smoke tests for doctor against both emulators; cover exit codes 0/1/2 + human + json output shapes"
      contains: "doctor_smoke_aws_localstack_all_pass"
  key_links:
    - from: "crates/rollout-cli/src/commands/cloud/doctor/checks.rs"
      to: "crates/rollout-cli/src/cloud_factory.rs"
      via: "build_cloud_runtime() supplies the four Arc<dyn ...> trait objects; checks exercise each one"
      pattern: "build_cloud_runtime"
    - from: "Check 7 (content_id_roundtrip)"
      to: "S3ObjectStore::put_stream + GcsObjectStore::put_stream"
      via: "64 MiB random buffer → put_stream → get_stream → blake3 verify; forces multipart/resumable path"
      pattern: "64.*1024.*1024|64 \\* 1024 \\* 1024"
---

<objective>
**Stage 5 — `rollout cloud doctor` CLI** per D-DOCTOR-01..04 + CLOUD-04. Operator pre-flight tool exercising all four cloud traits against either AWS or GCP, with human + JSON output and Unix exit codes.

Deliverables (RESEARCH.md Pattern 10):
- New CLI subcommand `rollout cloud doctor --provider <aws|gcp> --config <path> [--format <human|json>]`.
- Seven named checks per D-DOCTOR-01: reachability, auth, object_store, queue, secret_store, compute_hint, content_id_roundtrip.
- Exit codes per D-DOCTOR-03: 0 = all pass; 1 = any check failed; 2 = invocation/config error.
- Output formats per D-DOCTOR-02: human (colored, default) + json.
- Config source per D-DOCTOR-04: TOML `[cloud]` block only; no `--bucket`/`--queue`/`--secret-id` overrides in v1.1.
- Smoke tests against both emulators via cloud-emulator-{aws,gcp} CI jobs (always-on, no live cloud).
- mdBook chapter `cloud/doctor.md`.

**Addresses CLOUD-04.** Lands AFTER Plan 07 (needs working ObjectStore + Queue + SecretStore + ComputeHint impls for both providers — Plans 05/06; and the example TOMLs from Plan 07).

Purpose: ship the operator-facing pre-flight tool that proves a `[cloud]` block is correctly configured before running a real training job.
Output: `rollout cloud doctor` subcommand + 7 check functions + 2 output formats + smoke tests + operator playbook.
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
@$HOME/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/PROJECT.md
@.planning/ROADMAP.md
@.planning/STATE.md
@.planning/phases/05-cloud-layer-object-store-snapshots/05-CONTEXT.md
@.planning/phases/05-cloud-layer-object-store-snapshots/05-RESEARCH.md
@.planning/research/PITFALLS.md
@crates/rollout-cli/src/main.rs
@crates/rollout-cli/src/cloud_factory.rs
@crates/rollout-core/src/config/cloud.rs
@examples/sft-tiny-aws.toml
@examples/sft-tiny-gcp.toml

<interfaces>
<!-- Existing rollout-cli subcommand registry pattern (Phase 3 03-04 + Phase 4 04-06) -->
<!-- Cmd enum has Snapshot, Schema, Infer, Train, CoordinatorRun, WorkerRun. -->
<!-- Each subcommand lives in crates/rollout-cli/src/commands/<name>/mod.rs. -->

<!-- CloudRuntime + build_cloud_runtime from Plan 05 -->
```rust
pub struct CloudRuntime {
    pub object_store: Arc<dyn ObjectStore>,
    pub queue: Arc<dyn Queue>,
    pub secret_store: Arc<dyn SecretStore>,
    pub compute_hint: Arc<dyn ComputeHint>,
}
pub async fn build_cloud_runtime(cfg: &CloudConfig) -> Result<CloudRuntime, CoreError>;
```
</interfaces>
</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: CLI subcommand surface — `Cloud(CloudCmd)` + `CloudSub::Doctor(DoctorArgs)` + 7 check function skeletons + module layout</name>
  <files>crates/rollout-cli/src/main.rs, crates/rollout-cli/src/commands/cloud/mod.rs, crates/rollout-cli/src/commands/cloud/doctor/mod.rs, crates/rollout-cli/src/commands/cloud/doctor/checks.rs, crates/rollout-cli/src/commands/cloud/doctor/config.rs, crates/rollout-cli/src/commands/cloud/doctor/output/mod.rs, crates/rollout-cli/src/commands/cloud/doctor/output/human.rs, crates/rollout-cli/src/commands/cloud/doctor/output/json.rs, crates/rollout-cli/Cargo.toml</files>
  <read_first>
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-RESEARCH.md §"Pattern 10" lines 700-852 (full doctor implementation sketch — CLI surface, check fn signatures, exit-code branch, JSON schema)
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-CONTEXT.md D-DOCTOR-01..04 (locked decisions)
    - crates/rollout-cli/src/main.rs (current Cmd enum — Schema, Infer, Train, Snapshot subcommands as templates)
    - crates/rollout-cli/src/commands/snapshot/mod.rs (per-subcommand module pattern from Phase 4 04-06)
    - crates/rollout-cli/src/cloud_factory.rs (build_cloud_runtime — Plan 05/06 output, the dispatch entrypoint)
    - crates/rollout-core/src/config/cloud.rs (CloudConfig::Aws/Gcp variant matching)
    - examples/sft-tiny-aws.toml (TOML shape doctor will load)
  </read_first>
  <behavior>
    - Test `doctor_args_parse_aws_human_default`: clap parses `cloud doctor --provider aws --config foo.toml` into DoctorArgs { provider: Aws, config: PathBuf("foo.toml"), format: Human }.
    - Test `doctor_args_parse_gcp_json`: clap parses `cloud doctor --provider gcp --config bar.toml --format json` into the expected struct.
    - Test `doctor_args_reject_unknown_provider`: clap fails on `--provider azure`.
    - Test `doctor_args_reject_unknown_format`: clap fails on `--format yaml`.
    - Test `doctor_config_provider_mismatch_returns_exit_2`: TOML has [cloud.gcp] but `--provider aws` → fn returns exit code 2 (via internal helper that maps to `std::process::exit`).
    - Test `doctor_config_loads_from_aws_toml`: load examples/sft-tiny-aws.toml; CloudConfig::Aws variant returned with bucket=="rollout-snapshots-prod" etc.
    - Test `output_human_format_renders_check_name_status_latency`: print() over a small Vec<CheckResult> emits exactly N lines with `✓`/`✗` markers and per-check latency.
    - Test `output_json_format_matches_d_doctor_02_schema`: serde_json output deserializes back into `{ checks: Vec<CheckResult>, summary: Summary }` with correct pass_count + fail_count + total_latency_ms.
  </behavior>
  <action>
    **Step 1 — `crates/rollout-cli/src/main.rs`** — extend Cmd enum:
    ```rust
    #[derive(Parser)]
    enum Cmd {
        // ... existing Schema, Infer, Train, Snapshot, CoordinatorRun, WorkerRun ...
        /// Cloud diagnostics and pre-flight checks.
        Cloud(CloudCmd),
    }

    #[derive(Parser)]
    struct CloudCmd {
        #[command(subcommand)]
        sub: CloudSub,
    }

    #[derive(Parser)]
    enum CloudSub {
        /// Verify cloud provider configuration end-to-end.
        Doctor(crate::commands::cloud::doctor::DoctorArgs),
    }
    ```
    Dispatch in the main match:
    ```rust
    Cmd::Cloud(cloud) => match cloud.sub {
        CloudSub::Doctor(args) => crate::commands::cloud::doctor::run(args).await,
    },
    ```

    Add `mod commands;` (likely already exists) and inside `commands/mod.rs` add `pub mod cloud;`.

    **Step 2 — `crates/rollout-cli/src/commands/cloud/mod.rs`** — module stub:
    ```rust
    //! `rollout cloud` subcommand group. v1.1 ships `doctor` only; future
    //! enhancements add `cloud setup`, `cloud cleanup`, etc.
    pub mod doctor;
    ```

    **Step 3 — `crates/rollout-cli/src/commands/cloud/doctor/mod.rs`** — primary entry per RESEARCH.md §"Pattern 10":
    ```rust
    use clap::{Parser, ValueEnum};
    use std::path::PathBuf;

    #[derive(Parser, Debug, Clone)]
    pub struct DoctorArgs {
        /// Cloud provider to validate. MUST match [cloud].provider in --config TOML.
        #[arg(long, value_enum)]
        pub provider: ProviderArg,

        /// Path to the TOML config file (same shape as `rollout train sft --config`).
        #[arg(long)]
        pub config: PathBuf,

        /// Output format. Default = human (colored).
        #[arg(long, value_enum, default_value = "human")]
        pub format: OutputFormat,
    }

    #[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
    pub enum ProviderArg { Aws, Gcp }

    #[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
    pub enum OutputFormat { Human, Json }

    pub mod checks;
    pub mod config;
    pub mod output;

    pub async fn run(args: DoctorArgs) -> ! {
        let cfg = match config::cloud_config_from_toml(&args.config) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Config error: {e}");
                std::process::exit(2);
            }
        };
        if !config::provider_matches(&cfg, args.provider) {
            eprintln!(
                "--provider {:?} does not match [cloud].provider in {}",
                args.provider, args.config.display()
            );
            std::process::exit(2);
        }

        let results = checks::run_all_checks(args.provider, &cfg).await;
        let exit_code = if results.iter().any(|c| matches!(c.status, checks::CheckStatus::Fail)) { 1 } else { 0 };

        match args.format {
            OutputFormat::Human => output::human::print(&results),
            OutputFormat::Json => output::json::print(&results),
        }
        std::process::exit(exit_code);
    }
    ```

    Note `pub async fn run(args: DoctorArgs) -> !` — the function never returns; it calls `std::process::exit`. This is the only acceptable place to `process::exit` per AGENTS.md (CLI binaries — `anyhow` etc. allowed).

    **Step 4 — `commands/cloud/doctor/config.rs`** — TOML loading helpers:
    ```rust
    use std::path::Path;
    use rollout_core::config::{CloudConfig, RunConfig};
    use crate::commands::cloud::doctor::ProviderArg;

    pub fn cloud_config_from_toml(path: &Path) -> Result<CloudConfig, String> {
        let s = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let run: RunConfig = toml::from_str(&s).map_err(|e| format!("toml parse: {e}"))?;
        // Cross-field validation (Plan 04 added validate_cross_fields).
        run.cloud.validate_cross_fields().map_err(|e| format!("config invalid: {e}"))?;
        Ok(run.cloud)
    }

    pub fn provider_matches(cfg: &CloudConfig, arg: ProviderArg) -> bool {
        match (cfg, arg) {
            (CloudConfig::Aws(_), ProviderArg::Aws) => true,
            (CloudConfig::Gcp(_), ProviderArg::Gcp) => true,
            (CloudConfig::Local, _) => false,
            _ => false,
        }
    }
    ```

    **Step 5 — `commands/cloud/doctor/checks.rs`** — 7 named check functions per D-DOCTOR-01 + RESEARCH.md §"Pattern 10" lines 756-820:
    ```rust
    use std::sync::Arc;
    use std::time::Instant;
    use rollout_core::config::CloudConfig;
    use crate::commands::cloud::doctor::ProviderArg;

    #[derive(Debug, serde::Serialize)]
    pub struct CheckResult {
        pub name: &'static str,
        pub status: CheckStatus,
        pub latency_ms: u128,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub error: Option<String>,
    }

    #[derive(Debug, serde::Serialize, Clone, Copy, PartialEq, Eq)]
    #[serde(rename_all = "lowercase")]
    pub enum CheckStatus { Pass, Fail }

    pub async fn run_all_checks(provider: ProviderArg, cfg: &CloudConfig) -> Vec<CheckResult> {
        let mut out = Vec::with_capacity(7);

        // Build the runtime once; all 7 checks reuse it.
        let runtime = match crate::cloud_factory::build_cloud_runtime(cfg).await {
            Ok(rt) => rt,
            Err(e) => {
                // Surface build failure as one composite "auth/reachability" failure.
                out.push(CheckResult { name: "auth", status: CheckStatus::Fail, latency_ms: 0, error: Some(format!("runtime init: {e}")) });
                return out;
            }
        };
        let runtime = Arc::new(runtime);

        // 1. Reachability — TCP + TLS handshake to the service endpoint.
        out.push(timed("reachability", check_reachability(&runtime, provider, cfg).await).await);

        // 2. Auth — STS GetCallerIdentity (AWS) | ADC token mint (GCP).
        out.push(timed("auth", check_auth(&runtime, provider, cfg).await).await);

        // 3-6 run in parallel — saves 1-2s wall-clock per Claude's Discretion.
        let (os, q, ss, ch) = tokio::join!(
            timed_async("object_store",  check_object_store(&runtime)),
            timed_async("queue",         check_queue(&runtime)),
            timed_async("secret_store",  check_secret_store(&runtime, cfg)),
            timed_async("compute_hint",  check_compute_hint(&runtime)),
        );
        out.extend([os, q, ss, ch]);

        // 7. ContentId roundtrip — 64 MiB random buffer (D-DOCTOR-01 step 7).
        out.push(timed("content_id_roundtrip", check_content_id_roundtrip(&runtime).await).await);

        out
    }

    async fn timed(name: &'static str, result: Result<(), String>) -> CheckResult {
        let (status, error) = match result { Ok(()) => (CheckStatus::Pass, None), Err(e) => (CheckStatus::Fail, Some(e)) };
        CheckResult { name, status, latency_ms: 0 /* populated by timed_async; this path is for already-timed inputs */, error }
    }

    async fn timed_async<F: std::future::Future<Output = Result<(), String>>>(name: &'static str, fut: F) -> CheckResult {
        let start = Instant::now();
        let result = fut.await;
        let latency_ms = start.elapsed().as_millis();
        let (status, error) = match result { Ok(()) => (CheckStatus::Pass, None), Err(e) => (CheckStatus::Fail, Some(e)) };
        CheckResult { name, status, latency_ms, error }
    }

    // --- check implementations ---

    async fn check_reachability(_runtime: &Arc<crate::cloud_factory::CloudRuntime>, provider: ProviderArg, cfg: &CloudConfig) -> Result<(), String> {
        // TCP connect + TLS handshake to the service endpoint. For AWS: sqs.<region>.amazonaws.com:443.
        // For GCP: storage.googleapis.com:443. Use tokio::net::TcpStream::connect + a quick TLS probe.
        // Implementation: ~30 lines using hyper-rustls or direct tokio-rustls.
        match (provider, cfg) {
            (ProviderArg::Aws, CloudConfig::Aws(aws)) => {
                let host = format!("s3.{}.amazonaws.com", aws.region);
                tcp_tls_probe(&host, 443).await
            }
            (ProviderArg::Gcp, CloudConfig::Gcp(_)) => {
                tcp_tls_probe("storage.googleapis.com", 443).await
            }
            _ => Err("provider/config mismatch".to_owned()),
        }
    }

    async fn tcp_tls_probe(host: &str, port: u16) -> Result<(), String> {
        // ~20 lines: resolve + connect + TLS Client setup + handshake. Use rustls.
        unimplemented!("implement using tokio + rustls; document in PR")
    }

    async fn check_auth(runtime: &Arc<crate::cloud_factory::CloudRuntime>, provider: ProviderArg, _cfg: &CloudConfig) -> Result<(), String> {
        // AWS: aws-sdk-sts GetCallerIdentity via the same SdkConfig used by the runtime.
        // GCP: gcloud-auth credential mint.
        // For v1.1: surface the failure if the credential chain is broken. Both options need adding
        // an `aws-sdk-sts` dep (small) + an Auth probe in the GCP side.
        Ok(())   // placeholder — flesh out at integration time using the runtime's loaded SdkConfig
    }

    async fn check_object_store(runtime: &Arc<crate::cloud_factory::CloudRuntime>) -> Result<(), String> {
        use rollout_core::traits::cloud::PutHint;
        let payload = format!("doctor-probe-{}", ulid::Ulid::new()).into_bytes();
        let id = runtime.object_store.put_bytes(payload.clone(), PutHint::default()).await
            .map_err(|e| format!("put_bytes: {e}"))?;
        let got = runtime.object_store.get_bytes(&id).await.map_err(|e| format!("get_bytes: {e}"))?;
        if got != payload { return Err("roundtrip mismatch".to_owned()); }
        Ok(())
    }

    async fn check_queue(runtime: &Arc<crate::cloud_factory::CloudRuntime>) -> Result<(), String> {
        let payload = format!("doctor-probe-{}", ulid::Ulid::new()).into_bytes();
        let _id = runtime.queue.enqueue(payload.clone()).await.map_err(|e| format!("enqueue: {e}"))?;
        // Use dequeue_with_lease(30s) so we exercise the v1.1 lease path.
        let (id, got, _token) = runtime.queue.dequeue_with_lease(std::time::Duration::from_secs(30)).await
            .map_err(|e| format!("dequeue: {e}"))?
            .ok_or_else(|| "dequeue returned None".to_owned())?;
        if got != payload { return Err("queue roundtrip mismatch".to_owned()); }
        runtime.queue.ack(id).await.map_err(|e| format!("ack: {e}"))?;
        Ok(())
    }

    async fn check_secret_store(runtime: &Arc<crate::cloud_factory::CloudRuntime>, cfg: &CloudConfig) -> Result<(), String> {
        // Use the FIRST allowlisted secret name. If allowlist is empty, skip with a non-failure note.
        let name = match cfg {
            CloudConfig::Aws(aws) => aws.secrets.allowlist.first().cloned(),
            CloudConfig::Gcp(gcp) => gcp.secrets.allowlist.first().cloned(),
            CloudConfig::Local => None,
        };
        let Some(name) = name else {
            return Err("no secrets in allowlist; configure [cloud.*.secrets].allowlist to enable this check".to_owned());
        };
        let _val = runtime.secret_store.get(&name).await.map_err(|e| format!("get_secret({name}): {e}"))?;
        Ok(())
    }

    async fn check_compute_hint(runtime: &Arc<crate::cloud_factory::CloudRuntime>) -> Result<(), String> {
        let _inv = runtime.compute_hint.inventory().await.map_err(|e| format!("inventory: {e}"))?;
        let _signal = runtime.compute_hint.preemption_signal().await.map_err(|e| format!("preemption_signal: {e}"))?;
        Ok(())
    }

    async fn check_content_id_roundtrip(runtime: &Arc<crate::cloud_factory::CloudRuntime>) -> Result<(), String> {
        // D-DOCTOR-01 step 7: 64 MiB random buffer, force multipart/resumable path, blake3 verify.
        use std::pin::Pin;
        use tokio::io::AsyncRead;
        use rollout_core::traits::cloud::PutHint;
        use rollout_core::ContentId;

        let buf: Vec<u8> = (0..64 * 1024 * 1024).map(|i| (i % 251) as u8).collect();
        let expected = ContentId::from(blake3::hash(&buf));
        let stream: Pin<Box<dyn AsyncRead + Send>> = Box::pin(std::io::Cursor::new(buf.clone()));
        let id = runtime.object_store.put_stream(stream, PutHint { expected_size: Some(buf.len() as u64), content_type: None }).await
            .map_err(|e| format!("put_stream: {e}"))?;
        if id != expected {
            return Err(format!("ContentId mismatch: got {id:?}, expected {expected:?}"));
        }
        let mut got_stream = runtime.object_store.get_stream(&id).await.map_err(|e| format!("get_stream: {e}"))?;
        let mut got = Vec::with_capacity(buf.len());
        tokio::io::AsyncReadExt::read_to_end(&mut got_stream, &mut got).await.map_err(|e| format!("get_stream read: {e}"))?;
        if got != buf { return Err("get_stream returned wrong bytes".to_owned()); }
        Ok(())
    }
    ```

    Note the `check_reachability` `tcp_tls_probe` is left as `unimplemented!()` in the sketch; flesh out at integration time using a small ~20-line `tokio::net::TcpStream::connect` + `tokio_rustls::TlsConnector` snippet. The SDK clients themselves do not directly expose a "ping" — we probe at the TCP/TLS layer to surface DNS/firewall issues distinctly from auth failures.

    **Step 6 — `commands/cloud/doctor/output/mod.rs` + `human.rs` + `json.rs`:**

    `output/mod.rs`:
    ```rust
    pub mod human;
    pub mod json;
    ```

    `output/human.rs` — colored ANSI per D-DOCTOR-02:
    ```rust
    use crate::commands::cloud::doctor::checks::{CheckResult, CheckStatus};

    pub fn print(results: &[CheckResult]) {
        let pass = results.iter().filter(|c| matches!(c.status, CheckStatus::Pass)).count();
        let fail = results.iter().filter(|c| matches!(c.status, CheckStatus::Fail)).count();
        for c in results {
            let icon = if matches!(c.status, CheckStatus::Pass) { "\x1b[32m✓\x1b[0m" } else { "\x1b[31m✗\x1b[0m" };
            print!("  {icon} {:30} {:>7}ms", c.name, c.latency_ms);
            if let Some(e) = &c.error { print!("  \x1b[31m{e}\x1b[0m"); }
            println!();
        }
        let total: u128 = results.iter().map(|c| c.latency_ms).sum();
        println!();
        println!("  {pass} pass, {fail} fail — total {total}ms");
    }
    ```

    `output/json.rs` per D-DOCTOR-02 + RESEARCH.md §"Pattern 10" JSON schema:
    ```rust
    use crate::commands::cloud::doctor::checks::{CheckResult, CheckStatus};
    use serde::Serialize;

    #[derive(Serialize)]
    struct Summary {
        pass_count: usize,
        fail_count: usize,
        total_latency_ms: u128,
    }

    #[derive(Serialize)]
    struct DoctorReport<'a> {
        checks: &'a [CheckResult],
        summary: Summary,
    }

    pub fn print(results: &[CheckResult]) {
        let pass_count = results.iter().filter(|c| matches!(c.status, CheckStatus::Pass)).count();
        let fail_count = results.iter().filter(|c| matches!(c.status, CheckStatus::Fail)).count();
        let total_latency_ms = results.iter().map(|c| c.latency_ms).sum();
        let report = DoctorReport { checks: results, summary: Summary { pass_count, fail_count, total_latency_ms } };
        println!("{}", serde_json::to_string_pretty(&report).expect("doctor report serializes"));
    }
    ```

    **Step 7 — `crates/rollout-cli/Cargo.toml`** — verify the deps used above are present: `serde_json` (likely already), `tokio-rustls` (for tcp_tls_probe — add if missing), `aws-sdk-sts` if Step 5's auth check needs it (small extra crate; gated behind `aws` feature). Add to workspace if not pinned:
    ```toml
    [dependencies]
    # ... existing ...
    serde_json   = { workspace = true }
    tokio-rustls = { workspace = true }   # NEW, may need workspace addition
    ```

    Add `tokio-rustls = "0.26"` to workspace `Cargo.toml` if not present (rustls workspace already at 0.23 from Phase 2 02-04; tokio-rustls is the tokio adapter).

    **Step 8 — Unit tests** in each module's `#[cfg(test)] mod tests`:
    - `mod.rs`: clap parsing tests (4 tests).
    - `config.rs`: provider_matches + cloud_config_from_toml tests (3 tests).
    - `output/human.rs`: render-to-buffer assertion (write to a `Vec<u8>` instead of stdout; verify content).
    - `output/json.rs`: serialize → deserialize roundtrip; assert pass_count + fail_count + structure.
  </action>
  <verify>
    <automated>cargo test -p rollout-cli --features 'aws,gcp' --lib commands::cloud::doctor 2>&amp;1 | grep -E 'test result: ok' &amp;&amp; cargo build -p rollout-cli --features 'aws,gcp' &amp;&amp; cargo run -p rollout-cli --features 'aws,gcp' -- cloud doctor --help</automated>
  </verify>
  <acceptance_criteria>
    - `grep -nE 'Cloud\\(CloudCmd\\)' crates/rollout-cli/src/main.rs` returns 1.
    - `grep -nE 'pub async fn run\\(args: DoctorArgs\\) -> !' crates/rollout-cli/src/commands/cloud/doctor/mod.rs` returns 1.
    - `grep -nE 'std::process::exit\\(2\\)' crates/rollout-cli/src/commands/cloud/doctor/mod.rs` returns at least 1.
    - `grep -nE 'std::process::exit\\(1\\)' crates/rollout-cli/src/commands/cloud/doctor/mod.rs` returns at least 1 OR equivalent dynamic exit (e.g., `std::process::exit(exit_code)`).
    - `grep -cE 'async fn check_(reachability|auth|object_store|queue|secret_store|compute_hint|content_id_roundtrip)' crates/rollout-cli/src/commands/cloud/doctor/checks.rs` returns 7.
    - `grep -nE '64 \\* 1024 \\* 1024' crates/rollout-cli/src/commands/cloud/doctor/checks.rs` returns 1 (the 64 MiB probe buffer).
    - `grep -nE 'pass_count|fail_count|total_latency_ms' crates/rollout-cli/src/commands/cloud/doctor/output/json.rs` returns at least 3.
    - `cargo test -p rollout-cli --features 'aws,gcp' --lib commands::cloud::doctor` reports at least 8 tests passing.
    - `cargo build -p rollout-cli --features 'aws,gcp'` exits 0.
    - `cargo build -p rollout-cli` exits 0 (default features — the `Cloud` subcommand should still compile but the `Doctor` impl gracefully reports "rebuild with --features aws or gcp" if neither is enabled — handle this in mod.rs via #[cfg]).
    - `cargo run -p rollout-cli --features 'aws,gcp' -- cloud doctor --help` exits 0 and prints clap help with `--provider`, `--config`, `--format` flags.
  </acceptance_criteria>
  <done>
    CLI subcommand surface lives; 7 check functions stub'd with real impls (one TLS probe + factory-built impls + 64 MiB ContentId roundtrip); 2 output formats; exit codes wired; unit tests cover the parsing + output + provider-matching surface.
  </done>
</task>

<task type="auto">
  <name>Task 2: Doctor smoke integration tests against both emulators + mdBook chapter</name>
  <files>crates/rollout-cli/tests/doctor_smoke.rs, docs/book/src/cloud/doctor.md, docs/book/src/SUMMARY.md, .github/workflows/ci.yml</files>
  <read_first>
    - crates/rollout-cli/src/commands/cloud/doctor/mod.rs (just created in Task 1)
    - crates/rollout-cli/tests/ (any existing integration test for inspiration — e.g., snapshot CLI tests from Phase 4)
    - examples/sft-tiny-aws.toml + sft-tiny-gcp.toml (Plan 07 outputs)
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-CONTEXT.md D-DOCTOR-03 (exit code contract)
    - .planning/phases/05-cloud-layer-object-store-snapshots/05-RESEARCH.md §"Pattern 10" line 843-853 (JSON schema for the human-readable test target)
  </read_first>
  <behavior>
    - `doctor_smoke_aws_localstack_all_pass` (#[ignore = "requires LOCALSTACK_ENDPOINT"]): build a TOML at a tempfile path pointing at localstack via test_credentials override; invoke `rollout cloud doctor --provider aws --config <tmp> --format json` via `std::process::Command::cargo_bin`. Assert exit 0 + JSON `summary.fail_count == 0` + 7 checks present.
    - `doctor_smoke_gcp_emulators_all_pass` (#[ignore]): same for GCP via fake-gcs-server + pubsub-emulator + mock secret manager.
    - `doctor_smoke_aws_unreachable_returns_exit_1`: build a TOML pointing at a deliberately wrong region (e.g., `us-east-99`); assert exit 1.
    - `doctor_smoke_provider_mismatch_returns_exit_2`: TOML has `[cloud.gcp]` block but `--provider aws`; assert exit 2 + stderr contains "does not match".
    - `doctor_smoke_human_format_default`: invoke with `--format human` (or omit --format); assert stdout contains `✓` (pass icons) and a final "N pass, 0 fail" line.
    - `doctor_smoke_json_schema_round_trip`: invoke with `--format json`; pipe stdout to `serde_json::from_slice::<DoctorReport>()` — must deserialize.
  </behavior>
  <action>
    **Step 1 — `crates/rollout-cli/tests/doctor_smoke.rs`** — integration tests using `assert_cmd` or `std::process::Command::new(env!("CARGO_BIN_EXE_rollout-cli"))`:
    ```rust
    //! CLOUD-04 acceptance smoke. Invokes the built rollout binary against
    //! emulator-backed cloud services; asserts exit codes + JSON schema.
    //!
    //! Tests are #[ignore]'d for default Docker-free `cargo test --workspace --tests`;
    //! the cloud-emulator-{aws,gcp} CI jobs opt in via `--include-ignored`.

    use std::process::Command;

    fn doctor_bin() -> Command {
        // Cargo sets CARGO_BIN_EXE_<bin_name> for integration tests in the same package.
        Command::new(env!("CARGO_BIN_EXE_rollout-cli"))
    }

    #[test]
    #[ignore = "requires LOCALSTACK_ENDPOINT (set by cloud-emulator-aws CI job)"]
    fn doctor_smoke_aws_localstack_all_pass() {
        // Write a minimal AWS TOML pointing at localstack. Use a tempfile.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), AWS_LOCALSTACK_TOML).unwrap();
        let output = doctor_bin()
            .args(["cloud", "doctor", "--provider", "aws", "--config", tmp.path().to_str().unwrap(), "--format", "json"])
            .env("LOCALSTACK_ENDPOINT", std::env::var("LOCALSTACK_ENDPOINT").unwrap())
            .env("AWS_ACCESS_KEY_ID", "test")
            .env("AWS_SECRET_ACCESS_KEY", "test")
            .env("AWS_REGION", "us-east-1")
            .output().unwrap();
        assert_eq!(output.status.code(), Some(0), "doctor exited non-zero: {}", String::from_utf8_lossy(&output.stderr));
        let stdout = String::from_utf8_lossy(&output.stdout);
        let report: serde_json::Value = serde_json::from_str(&stdout).expect("JSON shape");
        assert_eq!(report["summary"]["fail_count"], 0);
        assert_eq!(report["checks"].as_array().unwrap().len(), 7);
    }

    const AWS_LOCALSTACK_TOML: &str = r#"
schema_version = 1
[run]
name = "doctor-smoke"
[storage]
backend = "embedded"
path = "/tmp/doctor.db"
[cloud]
provider = "aws"
[cloud.aws]
region = "us-east-1"
[cloud.aws.s3]
bucket = "rollout-doctor-test"
[cloud.aws.sqs]
queue_url = "http://localhost:4566/000000000000/doctor-test"
[cloud.aws.secrets]
allowlist = ["doctor-test-secret"]
[algorithm]
kind = "sft"
"#;

    // ... symmetric GCP_EMULATOR_TOML constant ...
    // ... 5 additional tests covering exit_1, exit_2, human format, JSON roundtrip ...
    ```

    For localstack-only tests: bucket + queue + secret must be created before doctor runs. Two options:
    - (a) Pre-create in a separate setup step in the CI job (`aws --endpoint-url=http://localhost:4566 s3 mb s3://rollout-doctor-test`, etc.)
    - (b) Skip the secret_store check by setting an empty allowlist in the test TOML, and rely on doctor returning that check as Fail-with-message (the check_secret_store returns "no secrets in allowlist" which IS a Fail under D-DOCTOR-03 → exit 1).

    Pick **(a)** for the "all_pass" tests — pre-create the resources in the CI job. Document the setup script inline in `.github/workflows/ci.yml`.

    **Step 2 — Update `.github/workflows/ci.yml` cloud-emulator-aws and cloud-emulator-gcp jobs** with pre-create steps + doctor invocation:

    Add to `cloud-emulator-aws` job:
    ```yaml
          - name: Pre-create localstack resources for doctor smoke
            env:
              AWS_ACCESS_KEY_ID: test
              AWS_SECRET_ACCESS_KEY: test
              AWS_REGION: us-east-1
            run: |
              aws --endpoint-url=http://localhost:4566 s3 mb s3://rollout-doctor-test
              aws --endpoint-url=http://localhost:4566 sqs create-queue --queue-name doctor-test
              aws --endpoint-url=http://localhost:4566 secretsmanager create-secret --name doctor-test-secret --secret-string "value"

          - name: Run doctor smoke tests
            env:
              LOCALSTACK_ENDPOINT: http://localhost:4566
              AWS_ACCESS_KEY_ID: test
              AWS_SECRET_ACCESS_KEY: test
              AWS_REGION: us-east-1
            run: |
              cargo test -p rollout-cli --features 'aws,gcp' --test doctor_smoke doctor_smoke_aws -- --include-ignored
    ```

    Add equivalent for cloud-emulator-gcp: pre-create the GCS bucket + Pub/Sub topic/subscription + Mock SM, then run `doctor_smoke_gcp_*` tests.

    The provider-mismatch + exit-1 + format-output tests run on EITHER emulator (they don't require real cloud round-trips for the exit-code assertion — they fail at the config validation or reachability layer). Place them in cloud-emulator-aws so they run always-on.

    **Step 3 — `docs/book/src/cloud/doctor.md`** — operator playbook:
    ```markdown
    # rollout cloud doctor

    Pre-flight tool that exercises all four cloud traits against either AWS or GCP.

    ## Usage

    ```bash
    rollout cloud doctor --provider aws --config examples/sft-tiny-aws.toml
    rollout cloud doctor --provider gcp --config examples/sft-tiny-gcp.toml --format json
    ```

    ## Checks (in order)

    1. **reachability** — TCP + TLS handshake to the service endpoint. Catches DNS / firewall issues distinctly from auth.
    2. **auth** — STS GetCallerIdentity (AWS) / ADC token mint (GCP). Catches credential-chain configuration.
    3. **object_store** — Small payload PUT + GET roundtrip on the configured bucket.
    4. **queue** — enqueue → dequeue_with_lease(30s) → ack on the configured queue.
    5. **secret_store** — Read the FIRST allowlisted secret (configured via `[cloud.*.secrets].allowlist`).
    6. **compute_hint** — Inventory + preemption_signal probe (returns Ok(None) if not on a cloud instance).
    7. **content_id_roundtrip** — 64 MiB random buffer via put_stream + get_stream + blake3 verify. Forces multipart/resumable path; catches blake3-streaming bugs (Pitfall 16).

    Wall-time target: ~5-10s on a healthy environment.

    ## Exit codes (D-DOCTOR-03)

    - `0` — all checks pass.
    - `1` — at least one check failed (use `--format json` to see which).
    - `2` — invocation/config error (bad provider, missing TOML, malformed schema).

    Plays well with shell `&&`:
    ```bash
    rollout cloud doctor --provider aws --config production.toml && rollout train sft --config production.toml
    ```

    ## Output formats

    - `--format human` (default): colored steps with ✓/✗ icons + per-check latency + pass/fail summary line.
    - `--format json`: machine-readable; matches the schema in `crates/rollout-cli/src/commands/cloud/doctor/output/json.rs`:
      ```json
      {
        "checks": [{"name": "...", "status": "pass|fail", "latency_ms": 142, "error": "..."}, ...],
        "summary": {"pass_count": 7, "fail_count": 0, "total_latency_ms": 5443}
      }
      ```

    ## Limitations (v1.1)

    - Config-file-only (D-DOCTOR-04); no `--bucket`/`--queue`/`--secret-id` overrides.
    - One comprehensive mode (D-DOCTOR-01); no `--quick` or `--deep` tiers.
    - Cross-cloud (both `[cloud.aws]` and `[cloud.gcp]` in same TOML) rejected at plan-time validation (D-XPROV-02).
    ```

    **Step 4 — `docs/book/src/SUMMARY.md`** — add `cloud/doctor.md` under the Cloud section.

    **Step 5 — Help-output golden test (optional, hardens v1.1 user experience).** Add to `doctor_smoke.rs`:
    ```rust
    #[test]
    fn doctor_help_lists_all_flags() {
        let output = doctor_bin().args(["cloud", "doctor", "--help"]).output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("--provider"));
        assert!(stdout.contains("--config"));
        assert!(stdout.contains("--format"));
    }
    ```
    This test does NOT need #[ignore] — it runs on default CI as a fast clap-help sanity check.
  </action>
  <verify>
    <automated>test -f crates/rollout-cli/tests/doctor_smoke.rs &amp;&amp; cargo build -p rollout-cli --features 'aws,gcp' --tests &amp;&amp; cargo test -p rollout-cli --features 'aws,gcp' --test doctor_smoke doctor_help_lists_all_flags 2>&amp;1 | grep -E 'test result: ok' &amp;&amp; mdbook build docs/book</automated>
  </verify>
  <acceptance_criteria>
    - `test -f crates/rollout-cli/tests/doctor_smoke.rs` is true.
    - `grep -cE 'fn doctor_smoke_' crates/rollout-cli/tests/doctor_smoke.rs` returns at least 5.
    - `grep -nE 'assert_eq!\\(output\\.status\\.code\\(\\), Some\\(0\\)' crates/rollout-cli/tests/doctor_smoke.rs` returns at least 1 (exit-0 assertion).
    - `grep -nE 'assert_eq!\\(output\\.status\\.code\\(\\), Some\\(2\\)' crates/rollout-cli/tests/doctor_smoke.rs` returns at least 1 (exit-2 assertion for provider-mismatch).
    - `grep -nE '"checks"\\].as_array' crates/rollout-cli/tests/doctor_smoke.rs` returns at least 1 (JSON schema assertion on 7 checks).
    - `cargo test -p rollout-cli --features 'aws,gcp' --test doctor_smoke doctor_help_lists_all_flags` exits 0 (the non-#[ignore]'d test runs on default CI).
    - `grep -E 'doctor_smoke_aws|doctor_smoke_gcp' .github/workflows/ci.yml` returns at least 2 matches (CI runs both).
    - `grep -E 'aws --endpoint-url=http://localhost:4566 s3 mb' .github/workflows/ci.yml` returns a match (pre-create step).
    - `test -f docs/book/src/cloud/doctor.md` is true; grep `5-10s` + `--format json` + `exit codes`.
    - `docs/book/src/SUMMARY.md` references `cloud/doctor.md`.
    - `mdbook build docs/book` exits 0.
    - On a localstack/fake-gcs-server runner: all doctor_smoke tests pass via `--include-ignored`.
  </acceptance_criteria>
  <done>
    `rollout cloud doctor` ships end-to-end: 7 checks + 2 output formats + 3 exit codes; smoke tests against both emulators run on every CI PR; mdBook chapter `cloud/doctor.md` published.
  </done>
</task>

</tasks>

<verification>
  <wave-checks>
    - `cargo build --workspace` exits 0 (default features off; the `Cloud(CloudCmd)` subcommand compiles but the Doctor impl gracefully reports "rebuild with --features aws or gcp" if neither feature enabled).
    - `cargo build -p rollout-cli --features 'aws,gcp'` exits 0.
    - `cargo test --workspace --tests` exits 0 (doctor_smoke tests are #[ignore]'d except the help-list test).
    - `cargo test -p rollout-cli --features 'aws,gcp' --lib commands::cloud::doctor` reports at least 8 unit tests passing.
    - `cargo clippy --workspace --all-targets --features 'aws,gcp' -- -D warnings` exits 0.
    - `cargo public-api -p rollout-core --simplified` still has 0 SDK symbols.
    - `cargo deny check` exits 0.
    - `cargo test -p rollout-core --test dependency_direction` exits 0 (still 14 invariants).
    - On runners with appropriate emulators + pre-created resources: doctor_smoke_aws_localstack_all_pass + doctor_smoke_gcp_emulators_all_pass exit 0.
    - `mdbook build docs/book` exits 0.
  </wave-checks>
</verification>

<success_criteria>
  - **CLOUD-04 acceptance criterion satisfied:** `rollout cloud doctor --provider <aws|gcp> --config <toml>` runs 7 checks and reports pass/fail with Unix exit codes.
  - D-DOCTOR-01..04 all implemented exactly:
    - D-DOCTOR-01: 7 named checks including 64 MiB ContentId roundtrip via put_stream/get_stream
    - D-DOCTOR-02: human (colored, default) + json (`{checks: [...], summary: {...}}`)
    - D-DOCTOR-03: exit 0 / 1 / 2 Unix convention
    - D-DOCTOR-04: TOML config source only, no flag overrides
  - Doctor smoke tests run on every CI PR via cloud-emulator-aws + cloud-emulator-gcp jobs (no live cloud).
  - mdBook chapter `cloud/doctor.md` published.
  - Phase 5 closes with CLOUD-01..04 all satisfied and 5 always-on emulator-backed CI gates green.
</success_criteria>

<output>
After completion, create `.planning/phases/05-cloud-layer-object-store-snapshots/05-08-SUMMARY.md` per template.
</output>

## Validation Architecture

| Test type | Coverage | Sampling cadence |
|-----------|----------|------------------|
| unit (rollout-cli doctor module) | clap parsing + config loading + provider matching + output formatting | every PR via `cargo test -p rollout-cli --features 'aws,gcp' --lib commands::cloud::doctor` |
| smoke (CARGO_BIN_EXE_rollout-cli invocation, default features) | doctor --help lists all flags | every PR — always-on, no #[ignore] |
| integration (doctor_smoke_aws_localstack_all_pass) | end-to-end against localstack with pre-created resources | every PR via cloud-emulator-aws CI job |
| integration (doctor_smoke_gcp_emulators_all_pass) | end-to-end against fake-gcs + pubsub-emulator + mock SM | every PR via cloud-emulator-gcp CI job |
| integration (exit-1 / exit-2 fixtures) | error paths: unreachable region, provider mismatch | every PR via cloud-emulator-aws CI job |

**Wave 0 dependency:** Plan 07 (witness tests prove the underlying stack works); Plans 05 + 06 transitively (provide the runtime impls). The `rollout cloud doctor` subcommand exercises the full vertical stack.
