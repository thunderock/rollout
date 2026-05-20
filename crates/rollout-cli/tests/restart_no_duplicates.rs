//! BACKEND-02 load-bearing proof: SIGKILL the CLI mid-batch, restart with
//! `--resume <run_id>`, assert the output JSONL has exactly N entries + all
//! unique ids + all input prompts present.
//!
//! Drives the real `rollout` CLI binary as a subprocess with `MockBackend`
//! (no Python / vLLM / GPU) so this test runs on EVERY CI build. Per
//! RESEARCH §"Restart-resume test design" + §"Pitfall 5".
//!
//! Location note (Rule-3 deviation): the plan called for this test under
//! `crates/rollout-runtime-batch/tests/`, but stable Cargo only exposes
//! `CARGO_BIN_EXE_<name>` for integration tests inside the same package as
//! the binary. The test lives here so `env!("CARGO_BIN_EXE_rollout")`
//! resolves; the `test-mock-backend` Cargo feature on `rollout-cli`
//! propagates to `rollout-runtime-batch/test-mock-backend` and swaps the
//! backend at runtime when `ROLLOUT_TEST_MOCK_BACKEND=1` is set.

#![cfg(feature = "test-mock-backend")]

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

const N_PROMPTS: usize = 8;

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

fn boxed<E: std::error::Error + Send + Sync + 'static>(
    e: E,
) -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(e)
}

fn write_test_config(tmp: &Path) -> std::io::Result<(PathBuf, PathBuf, PathBuf)> {
    let out_dir = tmp.join("out");
    let prompts_path = tmp.join("prompts.jsonl");
    let cfg_path = tmp.join("config.toml");

    let mut prompts = String::new();
    for i in 0..N_PROMPTS {
        prompts.push_str(&format!(
            "{{\"id\":\"p{i}\",\"prompt\":\"prompt number {i}\"}}\n",
        ));
    }
    std::fs::write(&prompts_path, prompts)?;

    let cfg = format!(
        r#"
[model]
uri = "mock://qwen-test"

[sampling]
temperature = 0.7
top_p       = 0.9
top_k       = -1
max_tokens  = 16
seed        = 42
stop        = []
stream      = false

[input]
glob = "{glob}"

[output]
dir = "{out}"

[workers]
count = 2
"#,
        glob = prompts_path.display(),
        out = out_dir.display(),
    );
    std::fs::write(&cfg_path, cfg)?;

    Ok((cfg_path, out_dir, prompts_path))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn restart_resumes_with_zero_duplicates() -> TestResult {
    let tmp = tempfile::tempdir().map_err(boxed)?;
    let (cfg_path, out_dir, _prompts_path) = write_test_config(tmp.path()).map_err(boxed)?;

    // Phase A — first run; kill after 3 sample_completed events.
    let mut child = Command::new(env!("CARGO_BIN_EXE_rollout"))
        .args([
            "infer",
            "batch",
            "--config",
            cfg_path.to_str().unwrap(),
            "--workers",
            "2",
        ])
        .env("ROLLOUT_TEST_MOCK_BACKEND", "1")
        .env("ROLLOUT_TEST_STALE_AFTER_MS", "0")
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(boxed)?;

    let stdout = child.stdout.take().expect("piped stdout");
    let mut lines = BufReader::new(stdout).lines();

    let mut completed = 0usize;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while completed < 3 {
        let line = tokio::time::timeout_at(deadline, lines.next_line()).await;
        let Ok(line) = line else {
            child.start_kill().ok();
            let _ = child.wait().await;
            panic!("timed out waiting for sample_completed");
        };
        match line {
            Ok(Some(text)) => {
                if text.contains("sample_completed") {
                    completed += 1;
                }
            }
            Ok(None) => break,
            Err(e) => {
                child.start_kill().ok();
                let _ = child.wait().await;
                return Err(boxed(e));
            }
        }
    }
    // SIGKILL — proves the resume path survives a hard kill.
    child.start_kill().map_err(boxed)?;
    let _ = child.wait().await;

    // Phase B — read the run id and restart with --resume.
    let run_id = std::fs::read_to_string(out_dir.join("run-id"))
        .map_err(boxed)?
        .trim()
        .to_string();
    assert!(!run_id.is_empty(), "run-id file is empty");

    let restart = Command::new(env!("CARGO_BIN_EXE_rollout"))
        .args([
            "infer",
            "batch",
            "--config",
            cfg_path.to_str().unwrap(),
            "--resume",
            &run_id,
            "--workers",
            "2",
        ])
        .env("ROLLOUT_TEST_MOCK_BACKEND", "1")
        .env("ROLLOUT_TEST_STALE_AFTER_MS", "0")
        .env("RUST_LOG", "info")
        .output()
        .await
        .map_err(boxed)?;
    assert!(
        restart.status.success(),
        "resume run failed: status={:?} stderr={}",
        restart.status,
        String::from_utf8_lossy(&restart.stderr),
    );

    // Phase C — assertions on completions.jsonl.
    let body = std::fs::read_to_string(out_dir.join("completions.jsonl")).map_err(boxed)?;
    let rows: Vec<serde_json::Value> = body
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("valid json row"))
        .collect();

    assert_eq!(
        rows.len(),
        N_PROMPTS,
        "expected exactly {N_PROMPTS} completions, got {}",
        rows.len()
    );

    let ids: HashSet<String> = rows
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        ids.len(),
        N_PROMPTS,
        "expected {N_PROMPTS} unique ids, got {} ids={:?}",
        ids.len(),
        ids
    );

    let prompts: HashSet<String> = rows
        .iter()
        .map(|r| r["prompt"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        prompts.len(),
        N_PROMPTS,
        "expected {N_PROMPTS} unique prompts present, got {} prompts={:?}",
        prompts.len(),
        prompts
    );
    for i in 0..N_PROMPTS {
        let expected = format!("prompt number {i}");
        assert!(
            prompts.contains(&expected),
            "missing original prompt: {expected:?}"
        );
    }

    Ok(())
}
