//! ID type round-trip, serde, and content-hash determinism tests.
use rollout_core::{ContentId, RunId, WorkerId};
use std::str::FromStr;
use ulid::Ulid;

/// blake3 of the empty input (well-known vector).
const EMPTY_BLAKE3: [u8; 32] = [
    0xaf, 0x13, 0x49, 0xb9, 0xf5, 0xf9, 0xa1, 0xa6, 0xa0, 0x40, 0x4d, 0xea, 0x36, 0xdc, 0xc9, 0x49,
    0x9b, 0xcb, 0x25, 0xc9, 0xad, 0xc1, 0x12, 0xb7, 0xcc, 0x9a, 0x93, 0xca, 0xe4, 0x1f, 0x32, 0x62,
];

#[test]
fn run_id_display_from_str_roundtrip() {
    let r = RunId(Ulid::new());
    let s = r.to_string();
    let parsed = RunId::from_str(&s).expect("parse RunId");
    assert_eq!(parsed, r);
}

#[test]
fn worker_id_display_from_str_roundtrip() {
    let w = WorkerId(Ulid::new());
    let s = w.to_string();
    let parsed = WorkerId::from_str(&s).expect("parse WorkerId");
    assert_eq!(parsed, w);
}

#[test]
fn content_id_determinism() {
    assert_eq!(ContentId::of(b"data"), ContentId::of(b"data"));
    assert_ne!(ContentId::of(b"data"), ContentId::of(b"other"));
}

#[test]
fn run_id_serde_json() {
    let r = RunId(Ulid::new());
    let json = serde_json::to_string(&r).expect("serialize");
    // transparent serde: string, not object.
    assert!(json.starts_with('"') && json.ends_with('"'));
    let back: RunId = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, r);
}

#[test]
fn content_id_known_vector() {
    assert_eq!(ContentId::of(b"").0, EMPTY_BLAKE3);
}
