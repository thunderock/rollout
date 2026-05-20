//! `sample_id()` derivation tests — locked hex + per-input sensitivity (proptest)
//! + `SCHEMA_VERSION` regression (RESEARCH Pitfall 1).

use proptest::prelude::*;
use rollout_core::{ContentId, SamplingParams};
use rollout_runtime_batch::{sample_id, SAMPLING_PARAMS_SCHEMA_VERSION};

fn fixed_model() -> ContentId {
    ContentId::of(b"phase-3-fixed-model")
}

#[test]
fn sample_id_is_deterministic() {
    let m = fixed_model();
    let p = SamplingParams::default();
    let a = sample_id(&m, "hello", &p, 0);
    let b = sample_id(&m, "hello", &p, 0);
    assert_eq!(a, b, "identical input must produce identical ContentId");
}

#[test]
fn sample_id_matches_locked_hex_for_default_params() {
    // Locked at test-write time to detect any silent hasher-input-order drift.
    // Regenerate ONLY when SAMPLING_PARAMS_SCHEMA_VERSION bumps (RESEARCH Pitfall 1).
    let m = fixed_model();
    let p = SamplingParams::default();
    let got = sample_id(&m, "hello", &p, 0);
    let hex = got.to_string();
    assert_eq!(hex.len(), 64, "ContentId hex must be 64 chars");
    // Print on failure so a planned schema-version bump can re-lock.
    eprintln!("sample_id_default = {hex}");
    assert_eq!(
        hex, LOCKED_HEX,
        "sample_id drift — bump SAMPLING_PARAMS_SCHEMA_VERSION and re-lock"
    );
}

/// Locked hex for `sample_id(ContentId::of(b"phase-3-fixed-model"), "hello", SamplingParams::default(), 0)`
/// under `SAMPLING_PARAMS_SCHEMA_VERSION = 1`. Bumping schema-version requires re-locking.
const LOCKED_HEX: &str = "aed11582cbb156d052a7ad784812a87794c76569c734a73999b63394ab460f2c";

#[test]
fn schema_version_byte_is_first() {
    // If we omitted the schema-version byte, the digest would differ.
    let m = fixed_model();
    let p = SamplingParams::default();
    let with_byte = sample_id(&m, "hello", &p, 0);
    let without_byte = {
        let mut h = blake3::Hasher::new();
        h.update(&m.0);
        h.update(b"hello");
        h.update(&postcard::to_stdvec(&p).unwrap());
        h.update(&0u64.to_le_bytes());
        ContentId(*h.finalize().as_bytes())
    };
    assert_ne!(
        with_byte, without_byte,
        "SCHEMA_VERSION byte must be part of the hasher input (Pitfall 1)"
    );
}

#[test]
fn schema_version_value_is_one_in_phase_3() {
    assert_eq!(SAMPLING_PARAMS_SCHEMA_VERSION, 1);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn sample_id_changes_with_prompt(
        a in "[a-zA-Z0-9 ]{1,32}",
        b in "[a-zA-Z0-9 ]{1,32}",
    ) {
        prop_assume!(a != b);
        let m = fixed_model();
        let p = SamplingParams::default();
        prop_assert_ne!(sample_id(&m, &a, &p, 0), sample_id(&m, &b, &p, 0));
    }

    #[test]
    fn sample_id_changes_with_idx(
        a in any::<u64>(),
        b in any::<u64>(),
    ) {
        prop_assume!(a != b);
        let m = fixed_model();
        let p = SamplingParams::default();
        prop_assert_ne!(sample_id(&m, "hello", &p, a), sample_id(&m, "hello", &p, b));
    }

    #[test]
    fn sample_id_changes_with_model(
        a in "[a-zA-Z0-9]{1,16}",
        b in "[a-zA-Z0-9]{1,16}",
    ) {
        prop_assume!(a != b);
        let ma = ContentId::of(a.as_bytes());
        let mb = ContentId::of(b.as_bytes());
        let p = SamplingParams::default();
        prop_assert_ne!(sample_id(&ma, "hello", &p, 0), sample_id(&mb, "hello", &p, 0));
    }

    #[test]
    fn sample_id_changes_with_temperature(
        a in 0.0f32..2.0,
        b in 0.0f32..2.0,
    ) {
        prop_assume!((a - b).abs() > 1e-6);
        let m = fixed_model();
        let mut pa = SamplingParams::default(); pa.temperature = a;
        let mut pb = SamplingParams::default(); pb.temperature = b;
        prop_assert_ne!(sample_id(&m, "hello", &pa, 0), sample_id(&m, "hello", &pb, 0));
    }

    #[test]
    fn sample_id_changes_with_max_tokens(
        a in 1u32..1024,
        b in 1u32..1024,
    ) {
        prop_assume!(a != b);
        let m = fixed_model();
        let mut pa = SamplingParams::default(); pa.max_tokens = a;
        let mut pb = SamplingParams::default(); pb.max_tokens = b;
        prop_assert_ne!(sample_id(&m, "hello", &pa, 0), sample_id(&m, "hello", &pb, 0));
    }

    #[test]
    fn sample_id_changes_with_seed(
        a in any::<u64>(),
        b in any::<u64>(),
    ) {
        prop_assume!(a != b);
        let m = fixed_model();
        let mut pa = SamplingParams::default(); pa.seed = Some(a);
        let mut pb = SamplingParams::default(); pb.seed = Some(b);
        prop_assert_ne!(sample_id(&m, "hello", &pa, 0), sample_id(&m, "hello", &pb, 0));
    }
}

