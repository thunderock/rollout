//! One-shot generator for the SHA-pinned 10-row eval fixtures.
//!
//! Run with `cargo run -p rollout-harness-eval --example gen_fixtures`. The
//! emitted parquet files are committed under `tests/fixtures/`; their blake3
//! hashes are pinned in `src/datasets/mod.rs`. The 10-row data is a curated
//! lm-eval-shaped subset (small, deterministic) so the parity witness is
//! always-on and offline. Re-running reproduces byte-identical files.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use arrow_array::builder::{Int32Builder, Int64Builder, ListBuilder, StringBuilder};
use arrow_array::{ArrayRef, RecordBatch, StringArray};
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");
    fs::create_dir_all(&dir).unwrap();
    write_mmlu(&dir.join("mmlu_10.parquet"));
    write_gsm8k(&dir.join("gsm8k_10.parquet"));
    write_ifeval(&dir.join("ifeval_10.parquet"));
    println!("wrote fixtures to {}", dir.display());
}

fn props() -> WriterProperties {
    // Uncompressed + fixed metadata → byte-reproducible across runs.
    WriterProperties::builder()
        .set_compression(Compression::UNCOMPRESSED)
        .set_created_by("rollout-harness-eval/gen_fixtures".to_owned())
        .build()
}

fn write(path: &Path, batch: &RecordBatch) {
    let file = fs::File::create(path).unwrap();
    let mut w = ArrowWriter::try_new(file, batch.schema(), Some(props())).unwrap();
    w.write(batch).unwrap();
    w.close().unwrap();
    let bytes = fs::read(path).unwrap();
    println!(
        "{}: blake3 = {}",
        path.file_name().unwrap().to_string_lossy(),
        blake3_hex(&bytes)
    );
}

fn blake3_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::new();
    for b in blake3::hash(bytes).as_bytes() {
        let _ = write!(s, "{b:02x}");
    }
    s
}

// 10 single-fact multiple-choice rows; gold answer index in 0..4.
fn write_mmlu(path: &Path) {
    let data: [(&str, [&str; 4], i32); 10] = [
        ("What is 2 + 2?", ["3", "4", "5", "6"], 1),
        (
            "Capital of France?",
            ["Berlin", "Madrid", "Paris", "Rome"],
            2,
        ),
        (
            "H2O is commonly called?",
            ["Salt", "Water", "Sugar", "Acid"],
            1,
        ),
        ("Largest planet?", ["Earth", "Mars", "Jupiter", "Venus"], 2),
        (
            "Speed of light is approx?",
            ["3e8 m/s", "3e6 m/s", "3e4 m/s", "30 m/s"],
            0,
        ),
        (
            "Author of Hamlet?",
            ["Dickens", "Shakespeare", "Twain", "Austen"],
            1,
        ),
        ("Square root of 81?", ["7", "8", "9", "10"], 2),
        ("Chemical symbol for gold?", ["Go", "Gd", "Au", "Ag"], 2),
        (
            "Primary color among these?",
            ["Green", "Orange", "Red", "Purple"],
            2,
        ),
        ("Number of continents?", ["5", "6", "7", "8"], 2),
    ];
    let questions: StringArray = data.iter().map(|d| Some(d.0)).collect();
    let mut choices = ListBuilder::new(StringBuilder::new());
    let mut answers = Int32Builder::new();
    for (_, c, a) in &data {
        for choice in c {
            choices.values().append_value(choice);
        }
        choices.append(true);
        answers.append_value(*a);
    }
    let batch = RecordBatch::try_from_iter(vec![
        ("question", Arc::new(questions) as ArrayRef),
        ("choices", Arc::new(choices.finish()) as ArrayRef),
        ("answer", Arc::new(answers.finish()) as ArrayRef),
    ])
    .unwrap();
    write(path, &batch);
}

// 10 grade-school problems; answer field carries the lm-eval `#### N` form.
fn write_gsm8k(path: &Path) {
    let data: [(&str, &str); 10] = [
        (
            "Tom has 3 apples and buys 2 more. How many?",
            "He buys 2 more.\n#### 5",
        ),
        (
            "A box holds 4 rows of 5 pens. Total pens?",
            "4 times 5.\n#### 20",
        ),
        ("Sara had 10 sweets, ate 3. Left?", "10 - 3.\n#### 7"),
        ("12 cookies split among 4 kids. Each?", "12 / 4.\n#### 3"),
        (
            "A car goes 60 km in 1h, how far in 2h?",
            "60 * 2.\n#### 120",
        ),
        ("5 + 6 + 7 = ?", "Sum them.\n#### 18"),
        ("Half of 50?", "50 / 2.\n#### 25"),
        ("9 books at $2 each cost?", "9 * 2.\n#### $18"),
        ("Triple of 11?", "11 * 3.\n#### 33"),
        ("100 minus 45?", "100 - 45.\n#### 55"),
    ];
    let questions: StringArray = data.iter().map(|d| Some(d.0)).collect();
    let answers: StringArray = data.iter().map(|d| Some(d.1)).collect();
    let batch = RecordBatch::try_from_iter(vec![
        ("question", Arc::new(questions) as ArrayRef),
        ("answer", Arc::new(answers) as ArrayRef),
    ])
    .unwrap();
    write(path, &batch);
}

// 10 IFEval prompts; `instructions` is a JSON array of {id, kwargs}. Rows 8 & 9
// carry a language-detection instruction (skipped + warned in v1.1).
fn write_ifeval(path: &Path) {
    let data: [(i64, &str, &str); 10] = [
        (
            0,
            "Write at least 5 words.",
            r#"[{"id":"length_constraints:number_words","kwargs":{"relation":"at least","num_words":5}}]"#,
        ),
        (
            1,
            "Reply in valid JSON.",
            r#"[{"id":"detectable_format:json_format","kwargs":{}}]"#,
        ),
        (
            2,
            "Use the word rust twice.",
            r#"[{"id":"keywords:frequency","kwargs":{"keyword":"rust","frequency":2,"relation":"at least"}}]"#,
        ),
        (
            3,
            "All lowercase please.",
            r#"[{"id":"change_case:english_lowercase","kwargs":{}}]"#,
        ),
        (
            4,
            "Include exactly 2 bullet points.",
            r#"[{"id":"detectable_format:number_bullet_lists","kwargs":{"num_bullets":2}}]"#,
        ),
        (
            5,
            "Mention apple and banana.",
            r#"[{"id":"keywords:existence","kwargs":{"keywords":["apple","banana"]}}]"#,
        ),
        (
            6,
            "Use at most 3 sentences.",
            r#"[{"id":"length_constraints:number_sentences","kwargs":{"relation":"at most","num_sentences":3}}]"#,
        ),
        (
            7,
            "Include at least 1 [placeholder].",
            r#"[{"id":"detectable_content:number_placeholders","kwargs":{"num_placeholders":1}}]"#,
        ),
        (
            8,
            "Respond in French.",
            r#"[{"id":"language:response_language","kwargs":{"language":"fr"}}]"#,
        ),
        (
            9,
            "Lowercase and at least 3 words.",
            r#"[{"id":"change_case:english_lowercase","kwargs":{}},{"id":"length_constraints:number_words","kwargs":{"relation":"at least","num_words":3}}]"#,
        ),
    ];
    let mut keys = Int64Builder::new();
    let mut prompts = StringBuilder::new();
    let mut instrs = StringBuilder::new();
    for (k, p, i) in &data {
        keys.append_value(*k);
        prompts.append_value(*p);
        instrs.append_value(*i);
    }
    let batch = RecordBatch::try_from_iter(vec![
        ("key", Arc::new(keys.finish()) as ArrayRef),
        ("prompt", Arc::new(prompts.finish()) as ArrayRef),
        ("instructions", Arc::new(instrs.finish()) as ArrayRef),
    ])
    .unwrap();
    write(path, &batch);
}
