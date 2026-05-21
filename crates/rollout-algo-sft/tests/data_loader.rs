//! JSONL data loader tests — D-DATA-01 schema coverage + malformed-row rejection.

use rollout_algo_sft::{load_jsonl, DataRow};
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn parses_prompt_completion_shape() {
    let tmp = tempdir().unwrap();
    let p = tmp.path().join("d.jsonl");
    fs::write(&p, r#"{"prompt":"Q","completion":"A"}"#).unwrap();
    let rows = load_jsonl(&p).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0],
        DataRow {
            prompt: "Q".into(),
            assistant: "A".into()
        }
    );
}

#[tokio::test]
async fn parses_messages_chat_shape() {
    let tmp = tempdir().unwrap();
    let p = tmp.path().join("d.jsonl");
    fs::write(
        &p,
        r#"{"messages":[{"role":"user","content":"Q"},{"role":"assistant","content":"A"}]}"#,
    )
    .unwrap();
    let rows = load_jsonl(&p).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].prompt.contains('Q'));
    assert_eq!(rows[0].assistant, "A");
}

#[tokio::test]
async fn rejects_malformed_row_with_line_number() {
    let tmp = tempdir().unwrap();
    let p = tmp.path().join("d.jsonl");
    fs::write(&p, "{\"unknown\": 42}").unwrap();
    let err = load_jsonl(&p).await.unwrap_err();
    assert!(format!("{err:?}").contains(":1:"));
}

#[tokio::test]
async fn rejects_messages_without_assistant_turn() {
    let tmp = tempdir().unwrap();
    let p = tmp.path().join("d.jsonl");
    fs::write(&p, r#"{"messages":[{"role":"user","content":"hi"}]}"#).unwrap();
    let err = load_jsonl(&p).await.unwrap_err();
    assert!(format!("{err:?}").contains("at least one assistant turn"));
}
