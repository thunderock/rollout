//! Bradley-Terry loss correctness — golden values + numerical-stability proof.

use rollout_algo_rm::{
    bradley_terry_batch_mean, bradley_terry_loss, load_pairs, logsigmoid, PairRow,
};
use std::fs;
use tempfile::tempdir;

fn approx_eq(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() < tol
}

#[test]
fn bradley_terry_known_values_zero_diff() {
    // chosen = rejected → diff = 0 → loss = -ln σ(0) = -ln 0.5 = ln 2 ≈ 0.6931.
    let l = bradley_terry_loss(1.0, 1.0);
    assert!(approx_eq(l, std::f32::consts::LN_2, 1e-6), "got {l}");
}

#[test]
fn bradley_terry_strong_preference_near_zero() {
    // diff = 10 → σ(10) ≈ 0.99995 → -ln ≈ 4.5398e-5.
    let l = bradley_terry_loss(5.0, -5.0);
    assert!(l < 1e-3, "got {l}; expected near zero");
    assert!(l >= 0.0, "loss must be non-negative; got {l}");
}

#[test]
fn bradley_terry_inverted_preference_large() {
    // diff = -10 → σ(-10) ≈ 4.5398e-5 → -ln ≈ 10.0000.
    let l = bradley_terry_loss(-5.0, 5.0);
    assert!(approx_eq(l, 10.0, 1e-3), "got {l}");
}

#[test]
fn bradley_terry_batch_mean_balances_two_pairs() {
    // Pair 1: diff=+1 → -ln σ(1) ≈ 0.3133
    // Pair 2: diff=-1 → -ln σ(-1) ≈ 1.3133
    // Mean ≈ 0.8133.
    let l = bradley_terry_batch_mean(&[(2.0, 1.0), (1.0, 2.0)]);
    assert!(approx_eq(l, 0.8133, 1e-3), "got {l}");
}

#[test]
fn bradley_terry_batch_mean_empty_returns_zero() {
    let l = bradley_terry_batch_mean(&[]);
    assert!(l.abs() < 1e-9, "expected 0.0, got {l}");
}

#[test]
fn logsigmoid_numerical_stability() {
    // logsigmoid(-50) must NOT be -inf or NaN.
    let v = logsigmoid(-50.0);
    assert!(v.is_finite(), "logsigmoid(-50) = {v}");
    assert!(approx_eq(v, -50.0, 1e-4));
    // logsigmoid(+50) must NOT be NaN.
    let v = logsigmoid(50.0);
    assert!(v.is_finite() && v < 0.0);
    assert!(approx_eq(v, 0.0, 1e-4));
}

#[tokio::test]
async fn data_loader_parses_pair_row() {
    let tmp = tempdir().unwrap();
    let p = tmp.path().join("pairs.jsonl");
    fs::write(&p, r#"{"prompt":"P","chosen":"C","rejected":"R"}"#).unwrap();
    let rows = load_pairs(&p).await.unwrap();
    assert_eq!(
        rows,
        vec![PairRow {
            prompt: "P".into(),
            chosen: "C".into(),
            rejected: "R".into(),
        }]
    );
}

#[tokio::test]
async fn data_loader_rejects_missing_field() {
    let tmp = tempdir().unwrap();
    let p = tmp.path().join("pairs.jsonl");
    fs::write(&p, r#"{"prompt":"P","chosen":"C"}"#).unwrap();
    let err = load_pairs(&p).await.unwrap_err();
    assert!(format!("{err:?}").contains(":1:"));
}
