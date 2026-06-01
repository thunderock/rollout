//! Offline-default dataset loader (D-EVAL-01).
//!
//! With `HF_OFFLINE=1` / `HF_HUB_OFFLINE=1` (the test default) the loaders read
//! the vendored 10-row parquet fixtures under `tests/fixtures/` via `parquet` +
//! `arrow-array` (rustls-only, no openssl — Pitfall G). Each fixture's blake3 is
//! pinned in a `*_TEST_BLAKE3` const and checked on load (fail-on-drift,
//! Pitfall 12). The online path (`hf-hub` full-split download → `ObjectStore`
//! cache keyed by `ContentId`) is a thin function gated behind not-offline; the
//! always-on tests never reach it. Loaded datasets are cached in an `Arc` once
//! per process (Pitfall E — never reload per `run`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow_array::{Array, Int32Array, Int64Array, ListArray, StringArray};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use rollout_core::{ContentId, CoreError, FatalError};
use serde::{Deserialize, Serialize};

use crate::suites::Suite;

/// blake3 of `tests/fixtures/mmlu_10.parquet` (fail-on-drift).
pub const MMLU_TEST_BLAKE3: &str =
    "1ca8bbda8a66fe1592ec3f2978a0e8a7d7437e7e2be71c486b4cdc80b869bb7a";
/// blake3 of `tests/fixtures/ifeval_10.parquet`.
pub const IFEVAL_TEST_BLAKE3: &str =
    "d7eb173c556005ce979248315c0746e1110de4d4946594f40dc73088c395f669";
/// blake3 of `tests/fixtures/gsm8k_10.parquet`.
pub const GSM8K_TEST_BLAKE3: &str =
    "3abe754c7cf22ed2911fc7bc2bebf2818bd66a91e2a91fd7a6e9b548ec26d29a";

/// One `MMLU` example.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MmluRow {
    /// Question stem.
    pub question: String,
    /// Exactly four answer choices.
    pub choices: [String; 4],
    /// 0-based gold choice index.
    pub answer: usize,
}

/// One `GSM8K` example.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gsm8kRow {
    /// Problem statement.
    pub question: String,
    /// Reference answer (number after the final `####`).
    pub answer: String,
}

/// A single `IFEval` verifiable instruction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instruction {
    /// lm-eval instruction id (e.g. `length_constraints:number_words`).
    pub id: String,
    /// Instruction parameters (object).
    pub kwargs: serde_json::Value,
}

/// One `IFEval` example.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IfevalRow {
    /// Example key.
    pub key: i64,
    /// The instruction-following prompt.
    pub prompt: String,
    /// Verifiable instructions attached to this prompt.
    pub instructions: Vec<Instruction>,
}

/// A loaded dataset (one of the three suites).
#[derive(Debug, Clone)]
pub enum Dataset {
    /// `MMLU` rows.
    Mmlu(Arc<Vec<MmluRow>>),
    /// `IFEval` rows.
    Ifeval(Arc<Vec<IfevalRow>>),
    /// `GSM8K` rows.
    Gsm8k(Arc<Vec<Gsm8kRow>>),
}

impl Dataset {
    /// Number of examples in the dataset.
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Dataset::Mmlu(v) => v.len(),
            Dataset::Ifeval(v) => v.len(),
            Dataset::Gsm8k(v) => v.len(),
        }
    }

    /// Whether the dataset is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Returns `true` when offline mode is requested (the test default).
#[must_use]
pub fn is_offline() -> bool {
    truthy("HF_OFFLINE") || truthy("HF_HUB_OFFLINE")
}

fn truthy(var: &str) -> bool {
    std::env::var(var).is_ok_and(|v| v != "0" && !v.is_empty())
}

/// Load a suite's dataset, offline-by-default from the vendored fixture.
///
/// # Errors
/// Returns [`CoreError`] if offline and the fixture is missing / drifted, or if
/// online (full-split download) is requested — unsupported in the always-on
/// test path (see [`load_online`]).
pub fn load(suite: Suite, fixtures_dir: &Path) -> Result<Dataset, CoreError> {
    if !is_offline() {
        return load_online(suite);
    }
    let (file, expected) = match suite {
        Suite::Mmlu => ("mmlu_10.parquet", MMLU_TEST_BLAKE3),
        Suite::Ifeval => ("ifeval_10.parquet", IFEVAL_TEST_BLAKE3),
        Suite::Gsm8k => ("gsm8k_10.parquet", GSM8K_TEST_BLAKE3),
    };
    let path = fixtures_dir.join(file);
    let bytes =
        std::fs::read(&path).map_err(|e| fatal(format!("read fixture {}: {e}", path.display())))?;
    let got = ContentId::of(&bytes).to_string();
    if got != expected {
        return Err(fatal(format!(
            "fixture {file} blake3 drift: expected {expected}, got {got}"
        )));
    }
    match suite {
        Suite::Mmlu => Ok(Dataset::Mmlu(Arc::new(parse_mmlu(&bytes)?))),
        Suite::Ifeval => Ok(Dataset::Ifeval(Arc::new(parse_ifeval(&bytes)?))),
        Suite::Gsm8k => Ok(Dataset::Gsm8k(Arc::new(parse_gsm8k(&bytes)?))),
    }
}

/// Online full-split download (`hf-hub`, rustls). Not exercised by the always-on
/// tests; wiring the `ObjectStore` cache lands with the `rollout eval` CLI (07-05).
///
/// # Errors
/// Always returns [`CoreError`] in v1.1 — the offline fixtures are the supported
/// path; this stub documents the rustls-only dep surface (`hf-hub` is linked,
/// proving the openssl-free resolution holds via `cargo deny check bans`).
pub fn load_online(suite: Suite) -> Result<Dataset, CoreError> {
    let _api = hf_hub::api::tokio::ApiBuilder::new();
    Err(fatal(format!(
        "online dataset download for {} requires HF network + ObjectStore cache (07-05); set HF_OFFLINE=1 to use the vendored fixture",
        suite.name()
    )))
}

/// The crate's vendored fixtures directory (`<crate>/tests/fixtures`).
#[must_use]
pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

fn parse_mmlu(bytes: &[u8]) -> Result<Vec<MmluRow>, CoreError> {
    let mut rows = Vec::new();
    for batch in read_batches(bytes)? {
        let questions = col_str(&batch, "question")?;
        let answers = batch
            .column_by_name("answer")
            .and_then(|c| c.as_any().downcast_ref::<Int32Array>())
            .ok_or_else(|| fatal("mmlu: missing Int32 column `answer`".into()))?;
        let choices = batch
            .column_by_name("choices")
            .and_then(|c| c.as_any().downcast_ref::<ListArray>())
            .ok_or_else(|| fatal("mmlu: missing List column `choices`".into()))?;
        for i in 0..batch.num_rows() {
            let list = choices.value(i);
            let vals = list
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or_else(|| fatal("mmlu: `choices` elements must be Utf8".into()))?;
            if vals.len() != 4 {
                return Err(fatal(format!(
                    "mmlu row {i}: expected 4 choices, got {}",
                    vals.len()
                )));
            }
            rows.push(MmluRow {
                question: questions.value(i).to_owned(),
                choices: [
                    vals.value(0).to_owned(),
                    vals.value(1).to_owned(),
                    vals.value(2).to_owned(),
                    vals.value(3).to_owned(),
                ],
                answer: usize::try_from(answers.value(i)).unwrap_or(0),
            });
        }
    }
    Ok(rows)
}

fn parse_gsm8k(bytes: &[u8]) -> Result<Vec<Gsm8kRow>, CoreError> {
    let mut rows = Vec::new();
    for batch in read_batches(bytes)? {
        let questions = col_str(&batch, "question")?;
        let answers = col_str(&batch, "answer")?;
        for i in 0..batch.num_rows() {
            rows.push(Gsm8kRow {
                question: questions.value(i).to_owned(),
                answer: answers.value(i).to_owned(),
            });
        }
    }
    Ok(rows)
}

fn parse_ifeval(bytes: &[u8]) -> Result<Vec<IfevalRow>, CoreError> {
    let mut rows = Vec::new();
    for batch in read_batches(bytes)? {
        let keys = batch
            .column_by_name("key")
            .and_then(|c| c.as_any().downcast_ref::<Int64Array>())
            .ok_or_else(|| fatal("ifeval: missing Int64 column `key`".into()))?;
        let prompts = col_str(&batch, "prompt")?;
        // `instructions` is a Utf8 column holding a JSON array of {id, kwargs}.
        let instrs = col_str(&batch, "instructions")?;
        for i in 0..batch.num_rows() {
            let parsed: Vec<Instruction> = serde_json::from_str(instrs.value(i))
                .map_err(|e| fatal(format!("ifeval row {i}: bad instructions json: {e}")))?;
            rows.push(IfevalRow {
                key: keys.value(i),
                prompt: prompts.value(i).to_owned(),
                instructions: parsed,
            });
        }
    }
    Ok(rows)
}

fn read_batches(bytes: &[u8]) -> Result<Vec<arrow_array::RecordBatch>, CoreError> {
    let reader = ParquetRecordBatchReaderBuilder::try_new(bytes::Bytes::copy_from_slice(bytes))
        .map_err(|e| fatal(format!("parquet open: {e}")))?
        .build()
        .map_err(|e| fatal(format!("parquet reader: {e}")))?;
    reader
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| fatal(format!("parquet read: {e}")))
}

fn col_str<'a>(
    batch: &'a arrow_array::RecordBatch,
    name: &str,
) -> Result<&'a StringArray, CoreError> {
    batch
        .column_by_name(name)
        .and_then(|c| c.as_any().downcast_ref::<StringArray>())
        .ok_or_else(|| fatal(format!("missing Utf8 column `{name}`")))
}

fn fatal(msg: String) -> CoreError {
    CoreError::Fatal(FatalError::Internal { msg })
}
