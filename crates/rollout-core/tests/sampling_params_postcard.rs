//! Postcard determinism + Pitfall 4 (`stop = []` vs omitted) for `SamplingParams`.
//!
//! Resume + sample-ID derivation hinge on byte-stable serialisation. These
//! tests fail loudly if postcard output ever drifts across runs or if a config
//! omitting `stop` produces different bytes than `stop = []`.

use rollout_core::SamplingParams;

#[test]
fn sampling_params_default_postcard_is_deterministic() {
    let a = postcard::to_stdvec(&SamplingParams::default()).expect("postcard a");
    let b = postcard::to_stdvec(&SamplingParams::default()).expect("postcard b");
    assert_eq!(a, b, "postcard SamplingParams::default() must be byte-stable");
}

#[test]
fn sampling_params_empty_stop_matches_serde_default() {
    // RESEARCH Pitfall 4: a TOML config with `stop = []` MUST hash identically
    // to one omitting `stop` entirely; otherwise sample-IDs drift between
    // equivalent configs.
    let omitted_toml = r"
temperature = 1.0
top_p = 1.0
top_k = -1
max_tokens = 16
stream = false
";
    let explicit_toml = r"
temperature = 1.0
top_p = 1.0
top_k = -1
max_tokens = 16
stop = []
stream = false
";
    let from_omitted: SamplingParams = toml::from_str(omitted_toml).expect("omitted parse");
    let from_explicit: SamplingParams = toml::from_str(explicit_toml).expect("explicit parse");

    let a = postcard::to_stdvec(&from_omitted).expect("postcard omitted");
    let b = postcard::to_stdvec(&from_explicit).expect("postcard explicit");
    assert_eq!(a, b, "stop = [] and omitted stop must produce identical postcard bytes");
}
