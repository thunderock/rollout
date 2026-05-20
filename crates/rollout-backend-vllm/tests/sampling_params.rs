//! Postcard determinism for `SamplingParams` (RESEARCH Pitfall 1 + Pitfall 4).
//!
//! Pins the wire-format invariant that backs Phase-3 sample-ID derivation: identical
//! `SamplingParams` values produce byte-identical postcard output across runs, and
//! the serde-default-instantiated value matches the explicit empty-stop construction.

use rollout_core::SamplingParams;

#[test]
fn postcard_default_is_byte_deterministic_across_runs() {
    let a = postcard::to_stdvec(&SamplingParams::default()).expect("postcard a");
    let b = postcard::to_stdvec(&SamplingParams::default()).expect("postcard b");
    assert_eq!(a, b, "postcard SamplingParams must be byte-deterministic");
}

#[test]
fn postcard_empty_stop_matches_serde_default_stop() {
    // RESEARCH Pitfall 4: `stop: Vec::new()` (constructed via Default) must hash
    // to the same bytes as `stop` omitted from TOML and re-deserialized through
    // the serde `default`.
    let from_default = SamplingParams::default();
    let omitted_toml = r"
        temperature = 1.0
        top_p       = 1.0
        top_k       = -1
        max_tokens  = 16
        stream      = false
    ";
    let from_toml: SamplingParams = toml::from_str(omitted_toml).expect("toml parse");
    let a = postcard::to_stdvec(&from_default).expect("postcard default");
    let b = postcard::to_stdvec(&from_toml).expect("postcard toml-default");
    assert_eq!(
        a, b,
        "Vec::new() and omitted-then-serde-default `stop` must encode identically"
    );
}
