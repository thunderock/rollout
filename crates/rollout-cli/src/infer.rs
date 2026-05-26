//! `rollout infer batch` subcommand — load TOML config, run `BatchCoordinator`
//! + N × `BatchWorker` against `Arc<dyn InferenceBackend>`, write JSONL output.
//!
//! Backend selection precedence:
//!   1. `--features test-mock-backend` + `ROLLOUT_TEST_MOCK_BACKEND=1` → `MockBackend` (plan 03-05).
//!   2. `--features vllm` → `VllmBackend` (live `AsyncLLMEngine`).
//!   3. Neither → fast-fail with `Fatal(ConfigInvalid)`.
//!
//! `run_id` lifecycle (BLOCKER 6):
//!   1. `--resume <id>` provided → parse + use it.
//!   2. Else `<output.dir>/run-id` exists → read + parse it.
//!   3. Else mint a fresh `RunId` (ULID) and persist to `<output.dir>/run-id`.

use clap::{Args, Subcommand};
use rollout_core::{
    CoreError, FatalError, InferenceBackend, ModelRef, ObjectStore, Prompt, RunId, WorkerId,
};
use rollout_runtime_batch::{
    BatchCoordinator, BatchWorker, InferBatchConfig, InputItem, JsonlOutput, SampleState,
};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// `rollout infer ...` command group.
#[derive(Debug, Args)]
pub struct InferCmd {
    #[command(subcommand)]
    pub sub: InferSub,
}

/// Subcommands under `rollout infer`.
#[derive(Debug, Subcommand)]
pub enum InferSub {
    /// Run a batch of completions against the configured model.
    Batch(BatchArgs),
}

/// `rollout infer batch` flags (D-CLI-01).
#[derive(Debug, Args)]
pub struct BatchArgs {
    /// TOML config (`[model] [sampling] [input] [output] [workers]`).
    #[arg(long, value_name = "PATH")]
    pub config: PathBuf,
    /// Re-attach to an existing run ID. If omitted, `<output.dir>/run-id` is consulted.
    #[arg(long, value_name = "RUN_ID")]
    pub resume: Option<String>,
    /// Worker count override (otherwise `[workers].count` from config is used).
    #[arg(long, value_name = "N")]
    pub workers: Option<u32>,
    /// Validate config + probe inputs but do not invoke the backend.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
}

/// Entry point dispatched from `main.rs`.
///
/// # Errors
/// Returns whatever the load / validate / run pipeline returns.
pub async fn run_infer_batch(args: &BatchArgs) -> Result<(), CoreError> {
    let cfg = crate::infer_config::load_from_file(&args.config)?;
    validate(&cfg)?;
    let effective_workers = args.workers.unwrap_or(cfg.workers.count);
    if effective_workers < 1 {
        return Err(cfg_err("workers.count must be >= 1"));
    }

    let inputs = glob_inputs(&cfg.input.glob).await?;
    if inputs.is_empty() {
        return Err(cfg_err(&format!(
            "no input rows matched glob {:?}",
            cfg.input.glob
        )));
    }

    let hf_token = read_hf_token_if_required(&cfg.model.uri);

    if args.dry_run {
        tracing::info!(
            model = %cfg.model.uri,
            inputs = inputs.len(),
            workers = effective_workers,
            "dry-run: config valid"
        );
        println!(
            "dry-run OK: model={} inputs={} workers={}",
            cfg.model.uri,
            inputs.len(),
            effective_workers
        );
        return Ok(());
    }

    let backend = select_backend(&cfg.model, hf_token).await?;
    run_pool(args, &cfg, backend, inputs, effective_workers).await
}

/// Plan-time validation per AGENTS.md principle #3.
fn validate(cfg: &InferBatchConfig) -> Result<(), CoreError> {
    if cfg.sampling.stream {
        return Err(cfg_err("streaming generation is Phase 8 (INFER-01)"));
    }
    if cfg.sampling.max_tokens == 0 {
        return Err(cfg_err("sampling.max_tokens must be > 0"));
    }
    if cfg.workers.count < 1 {
        return Err(cfg_err("workers.count must be >= 1"));
    }
    if cfg.input.glob.trim().is_empty() {
        return Err(cfg_err("input.glob must be non-empty"));
    }
    Ok(())
}

/// Resolve a glob (or literal path) to a deterministic, ordered `Vec<InputItem>`.
///
/// # Errors
/// Returns `Fatal(ConfigInvalid)` for glob/I/O errors.
pub async fn glob_inputs(pattern: &str) -> Result<Vec<InputItem>, CoreError> {
    let mut files: Vec<PathBuf> = if pattern.contains('*') || pattern.contains('?') {
        glob::glob(pattern)
            .map_err(|e| cfg_err(&format!("glob {pattern:?}: {e}")))?
            .filter_map(Result::ok)
            .collect()
    } else {
        vec![PathBuf::from(pattern)]
    };
    files.sort();

    let mut out: Vec<InputItem> = Vec::new();
    let mut idx: u64 = 0;
    for path in &files {
        let rows = rollout_runtime_batch::read_jsonl(path).await?;
        for r in rows {
            out.push(InputItem {
                input_idx: idx,
                prompt: Prompt(r.prompt),
            });
            idx += 1;
        }
    }
    Ok(out)
}

fn read_hf_token_if_required(model_uri: &str) -> Option<String> {
    // Heuristic: only known-gated prefixes pull the secret. Phase 4 can widen this
    // to a config-driven allowlist if other publishers gate their models too.
    if model_uri.starts_with("meta-llama/") || model_uri.starts_with("mistralai/") {
        std::env::var("ROLLOUT_SECRET_HF_TOKEN").ok()
    } else {
        None
    }
}

/// Build the backend (mock or vLLM) and run `init()`.
#[allow(clippy::unused_async)] // Only `await`s when a backend feature is active.
async fn select_backend(
    model: &ModelRef,
    hf_token: Option<String>,
) -> Result<Arc<dyn InferenceBackend>, CoreError> {
    #[cfg(feature = "test-mock-backend")]
    if std::env::var("ROLLOUT_TEST_MOCK_BACKEND").as_deref() == Ok("1") {
        let _ = &hf_token;
        let mut b = rollout_runtime_batch::MockBackend::new(50);
        b.init(model).await?;
        return Ok(Arc::new(b));
    }

    #[cfg(feature = "vllm")]
    {
        let engine_id = format!("cli-{}", ulid::Ulid::new());
        let mut b = rollout_backend_vllm::VllmBackend::with_secret_token(&engine_id, hf_token)?;
        b.init(model).await?;
        Ok(Arc::new(b))
    }

    #[cfg(not(feature = "vllm"))]
    {
        let _ = (model, hf_token);
        Err(cfg_err(
            "rollout-cli was built without the `vllm` feature; rebuild with --features vllm",
        ))
    }
}

/// Orchestrate storage, queue, coordinator, workers, and write JSONL output.
async fn run_pool(
    args: &BatchArgs,
    cfg: &InferBatchConfig,
    backend: Arc<dyn InferenceBackend>,
    inputs: Vec<InputItem>,
    workers_count: u32,
) -> Result<(), CoreError> {
    tokio::fs::create_dir_all(&cfg.output.dir)
        .await
        .map_err(|e| cfg_err(&format!("create output.dir: {e}")))?;

    let storage =
        Arc::new(rollout_storage::EmbeddedStorage::open(cfg.output.dir.join("rollout.db")).await?);
    let object_store = Arc::new(
        rollout_cloud_local::FsObjectStore::open(cfg.output.dir.join("object-store")).await?,
    );
    let queue = Arc::new(rollout_cloud_local::InMemQueue::open(storage.clone()).await?);

    let run_id = resolve_run_id(args, &cfg.output.dir).await?;

    let mut coord =
        BatchCoordinator::new(storage.clone(), queue.clone(), object_store.clone(), run_id);
    // Test hook: shorten the stale-claim window so plan 03-05's
    // restart_no_duplicates integration test can re-enqueue mid-flight samples
    // immediately on restart instead of waiting the default 5 minutes (RESEARCH
    // Pitfall 5). Never set in production deployments.
    if let Ok(ms) = std::env::var("ROLLOUT_TEST_STALE_AFTER_MS") {
        if let Ok(ms) = ms.parse::<u64>() {
            coord = coord.with_stale_after_ms(ms);
        }
    }
    let model_cid = *backend.model_id();
    let enqueued = coord
        .scan_and_enqueue(&inputs, &model_cid, &cfg.sampling)
        .await?;
    tracing::info!(
        run_id = %run_id,
        enqueued,
        total = inputs.len(),
        "scan_and_enqueue complete"
    );

    // Spawn the worker pool. Each worker shares the Arc deps; the queue + CAS
    // guarantee at-most-once Done per sample.
    let mut set: tokio::task::JoinSet<Result<usize, CoreError>> = tokio::task::JoinSet::new();
    for w in 0..workers_count {
        let worker_id = WorkerId(ulid::Ulid::new());
        // NOTE: workers keep the default `stale_after_ms` so intra-process
        // races between peer workers stay correct. Only the coordinator's
        // `stale_after_ms` is shortened in the test path (see above).
        let worker = BatchWorker::new(
            backend.clone(),
            storage.clone(),
            object_store.clone(),
            queue.clone(),
            run_id,
            worker_id,
            cfg.sampling.clone(),
        );
        tracing::info!(worker = %worker_id, slot = w, "spawned worker");
        set.spawn(async move { worker.run_loop().await });
    }
    let mut total_completed = 0usize;
    while let Some(joined) = set.join_next().await {
        let n = joined.map_err(|e| {
            CoreError::Fatal(FatalError::Internal {
                msg: format!("worker join: {e}"),
            })
        })??;
        total_completed += n;
    }
    tracing::info!(completed = total_completed, "worker pool drained");

    // Collect Done rows, sorted by input_idx, and emit JSONL.
    let done = coord.collect_done_records().await?;
    let mut rows: Vec<JsonlOutput> = Vec::with_capacity(done.len());
    let sampling_json =
        serde_json::to_value(&cfg.sampling).map_err(|e| internal(&e.to_string()))?;
    for rec in done {
        // Re-derive the input ordinal -> input row.
        let idx = usize::try_from(rec.input_idx).unwrap_or(usize::MAX);
        let input = inputs.get(idx).ok_or_else(|| {
            internal(&format!(
                "Done record input_idx {} out of bounds (len={})",
                rec.input_idx,
                inputs.len()
            ))
        })?;
        let SampleState::Done {
            completion_blob,
            finished_at_ms,
        } = rec.state
        else {
            continue;
        };
        let completion_bytes = object_store.get_bytes(&completion_blob).await?;
        let completion_text = String::from_utf8(completion_bytes)
            .map_err(|e| internal(&format!("completion blob not UTF-8: {e}")))?;
        let row = JsonlOutput {
            id: rec.id.to_string(),
            prompt: input.prompt.0.clone(),
            completion: completion_text,
            sampling_params: sampling_json.clone(),
            model_uri: cfg.model.uri.clone(),
            finish_reason: "stop".to_string(),
            model_content_id: model_cid.to_string(),
            completion_blob_id: completion_blob.to_string(),
            generated_at: format_unix_ms(finished_at_ms),
            extras: serde_json::Map::new(),
        };
        rows.push(row);
    }
    let out_path = cfg.output.dir.join("completions.jsonl");
    rollout_runtime_batch::write_jsonl(&out_path, &rows).await?;
    tracing::info!(rows = rows.len(), path = %out_path.display(), "wrote completions");

    // Best-effort shutdown — Arc'd backend may not be uniquely owned, so we
    // skip explicit shutdown here; vLLM's engine drops on process exit.
    Ok(())
}

/// Resolve the run id per BLOCKER 6 lifecycle.
async fn resolve_run_id(args: &BatchArgs, output_dir: &Path) -> Result<RunId, CoreError> {
    let run_id_path = output_dir.join("run-id");
    if let Some(explicit) = &args.resume {
        return RunId::from_str(explicit.trim())
            .map_err(|e| cfg_err(&format!("invalid --resume {explicit:?}: {e}")));
    }
    if tokio::fs::try_exists(&run_id_path).await.unwrap_or(false) {
        let s = tokio::fs::read_to_string(&run_id_path)
            .await
            .map_err(|e| cfg_err(&format!("read run-id: {e}")))?;
        return RunId::from_str(s.trim())
            .map_err(|e| cfg_err(&format!("parse run-id {}: {e}", run_id_path.display())));
    }
    let fresh = RunId(ulid::Ulid::new());
    // Persist via tempfile + rename for atomicity.
    let tmp = run_id_path.with_extension("tmp");
    tokio::fs::write(&tmp, fresh.to_string())
        .await
        .map_err(|e| cfg_err(&format!("write run-id: {e}")))?;
    tokio::fs::rename(&tmp, &run_id_path)
        .await
        .map_err(|e| cfg_err(&format!("rename run-id: {e}")))?;
    Ok(fresh)
}

fn format_unix_ms(unix_ms: u64) -> String {
    // Minimal RFC3339-ish UTC formatter — no chrono dep. Uses civil date math
    // since 1970-01-01. Sufficient for log/metadata fields.
    let _ = SystemTime::UNIX_EPOCH;
    let secs = unix_ms / 1_000;
    let millis = unix_ms % 1_000;
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let rem = secs % 86_400;
    let hour = rem / 3_600;
    let minute = (rem % 3_600) / 60;
    let second = rem % 60;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

/// Howard Hinnant's `civil_from_days` algorithm (proleptic Gregorian).
#[allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::similar_names
)]
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year_civil = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year_final = if month <= 2 {
        year_civil + 1
    } else {
        year_civil
    };
    (year_final as i32, month as u32, day as u32)
}

fn cfg_err(msg: &str) -> CoreError {
    CoreError::Fatal(FatalError::ConfigInvalid {
        msg: msg.to_string(),
    })
}

fn internal(msg: &str) -> CoreError {
    CoreError::Fatal(FatalError::Internal {
        msg: msg.to_string(),
    })
}

// `SystemTime::UNIX_EPOCH` import kept silent against unused-import lints.
#[allow(dead_code)]
const _UNIX_EPOCH: SystemTime = UNIX_EPOCH;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_unix_ms_handles_epoch() {
        assert_eq!(format_unix_ms(0), "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn format_unix_ms_handles_known_date() {
        // 2024-01-01T00:00:00Z = 1_704_067_200_000 ms.
        assert_eq!(
            format_unix_ms(1_704_067_200_000),
            "2024-01-01T00:00:00.000Z"
        );
    }
}
