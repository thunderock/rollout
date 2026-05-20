//! JSONL reader + writer round-trip — extras preserved (D-CLI-02 / D-CLI-03).

use rollout_runtime_batch::{read_jsonl, write_jsonl, JsonlOutput};
use serde_json::json;

#[tokio::test]
async fn read_jsonl_preserves_extras() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("in.jsonl");
    let body = concat!(
        r#"{"prompt":"hello","meta":"x","tags":["a","b"]}"#,
        "\n",
        r#"{"id":"row-2","prompt":"world"}"#,
        "\n",
        r#"{"prompt":"third","nested":{"k":1}}"#,
        "\n",
    );
    tokio::fs::write(&path, body).await.unwrap();
    let rows = read_jsonl(&path).await.unwrap();
    assert_eq!(rows.len(), 3);

    assert_eq!(rows[0].id, None);
    assert_eq!(rows[0].prompt, "hello");
    assert_eq!(rows[0].extras.get("meta").unwrap(), &json!("x"));
    assert_eq!(rows[0].extras.get("tags").unwrap(), &json!(["a", "b"]));

    assert_eq!(rows[1].id.as_deref(), Some("row-2"));
    assert!(rows[1].extras.is_empty());

    assert_eq!(rows[2].extras.get("nested").unwrap(), &json!({"k": 1}));
}

#[tokio::test]
async fn write_jsonl_round_trips_extras_into_output() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("out.jsonl");

    let mut extras = serde_json::Map::new();
    extras.insert("meta".into(), json!("x"));
    let out = vec![
        JsonlOutput {
            id: "a".into(),
            prompt: "hello".into(),
            completion: "MOCK:hello".into(),
            sampling_params: json!({"temperature": 0.7}),
            model_uri: "fake/model".into(),
            finish_reason: "stop".into(),
            model_content_id: "00".repeat(32),
            completion_blob_id: "11".repeat(32),
            generated_at: "2026-05-20T00:00:00Z".into(),
            extras: extras.clone(),
        },
        JsonlOutput {
            id: "b".into(),
            prompt: "world".into(),
            completion: "MOCK:world".into(),
            sampling_params: json!({"temperature": 0.7}),
            model_uri: "fake/model".into(),
            finish_reason: "stop".into(),
            model_content_id: "00".repeat(32),
            completion_blob_id: "22".repeat(32),
            generated_at: "2026-05-20T00:00:01Z".into(),
            extras: serde_json::Map::new(),
        },
    ];
    write_jsonl(&path, &out).await.unwrap();
    let body = tokio::fs::read_to_string(&path).await.unwrap();
    let lines: Vec<&str> = body.lines().collect();
    assert_eq!(lines.len(), 2);
    let row0: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(row0.get("meta").unwrap(), &json!("x"));
    assert_eq!(row0.get("completion").unwrap(), &json!("MOCK:hello"));
    let row1: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert!(row1.get("meta").is_none());
    assert_eq!(row1.get("id").unwrap(), &json!("b"));
}

#[tokio::test]
async fn read_jsonl_full_round_trip_with_input_extras_preserved() {
    let tmp = tempfile::tempdir().unwrap();
    let in_path = tmp.path().join("in.jsonl");
    tokio::fs::write(
        &in_path,
        concat!(
            r#"{"prompt":"alpha","meta":1}"#,
            "\n",
            r#"{"prompt":"beta","meta":2,"tag":"hi"}"#,
            "\n",
        ),
    )
    .await
    .unwrap();
    let rows = read_jsonl(&in_path).await.unwrap();

    let out_path = tmp.path().join("out.jsonl");
    let outputs: Vec<JsonlOutput> = rows
        .into_iter()
        .map(|r| JsonlOutput {
            id: r.id.unwrap_or_else(|| "auto".into()),
            prompt: r.prompt.clone(),
            completion: format!("MOCK:{}", r.prompt),
            sampling_params: json!({}),
            model_uri: "fake/model".into(),
            finish_reason: "stop".into(),
            model_content_id: "00".repeat(32),
            completion_blob_id: "00".repeat(32),
            generated_at: "now".into(),
            extras: r.extras,
        })
        .collect();
    write_jsonl(&out_path, &outputs).await.unwrap();

    let body = tokio::fs::read_to_string(&out_path).await.unwrap();
    let lines: Vec<serde_json::Value> = body
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines[0].get("meta").unwrap(), &json!(1));
    assert_eq!(lines[1].get("meta").unwrap(), &json!(2));
    assert_eq!(lines[1].get("tag").unwrap(), &json!("hi"));
}
