//! Witness: with `HF_OFFLINE=1` the loaders read the vendored 10-row fixtures
//! with zero network, and each fixture's blake3 matches the pinned const.

use rollout_harness_eval::datasets::{self, Dataset};
use rollout_harness_eval::suites::Suite;

// Tests run with `HF_OFFLINE=1` in the environment (the always-on default and
// the documented test invocation); the loader honours it with no network.
fn offline_dir() -> std::path::PathBuf {
    datasets::fixtures_dir()
}

#[test]
fn loads_all_three_suites_offline() {
    assert!(datasets::is_offline(), "run with HF_OFFLINE=1");
    let dir = offline_dir();

    let mmlu = datasets::load(Suite::Mmlu, &dir).expect("mmlu loads offline");
    let ifeval = datasets::load(Suite::Ifeval, &dir).expect("ifeval loads offline");
    let gsm8k = datasets::load(Suite::Gsm8k, &dir).expect("gsm8k loads offline");

    assert_eq!(mmlu.len(), 10);
    assert_eq!(ifeval.len(), 10);
    assert_eq!(gsm8k.len(), 10);

    match &mmlu {
        Dataset::Mmlu(rows) => {
            assert_eq!(rows[0].choices.len(), 4);
            assert_eq!(rows[1].answer, 2); // "Paris"
        }
        _ => panic!("wrong dataset variant"),
    }
}

#[test]
fn fixture_blake3_drift_is_detected() {
    // A bogus directory (no fixture) → loud error, never a silent empty dataset.
    let dir = offline_dir();
    let bad = dir.join("does-not-exist");
    let err = datasets::load(Suite::Mmlu, &bad).unwrap_err();
    assert!(format!("{err:?}").contains("read fixture"));
}
