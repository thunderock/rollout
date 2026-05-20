//! `CoreError` variant + `#[from]` propagation tests.
use rollout_core::{CoreError, FatalError, RecoverableError, RetryHint};

#[test]
fn variants_exist() {
    let r: CoreError = CoreError::Recoverable(RecoverableError::Preempted {
        hint: RetryHint::Never,
    });
    let f: CoreError = CoreError::Fatal(FatalError::Internal { msg: "x".into() });
    assert!(matches!(r, CoreError::Recoverable(_)));
    assert!(matches!(f, CoreError::Fatal(_)));
}

#[test]
fn from_propagation() {
    fn inner() -> Result<(), CoreError> {
        Err(RecoverableError::Preempted {
            hint: RetryHint::Never,
        })?;
        Ok(())
    }
    let err = inner().expect_err("must error");
    assert!(matches!(err, CoreError::Recoverable(_)));
}

#[test]
fn display_formats_recoverable_and_fatal() {
    let r = CoreError::Recoverable(RecoverableError::Throttled {
        hint: RetryHint::Never,
    });
    assert!(format!("{r}").starts_with("recoverable: "));
    let f = CoreError::Fatal(FatalError::ConfigInvalid { msg: "bad".into() });
    assert!(format!("{f}").starts_with("fatal: "));
}

#[test]
fn not_serializable() {
    // Marker test for RESEARCH.md Anti-Pattern 4. The real enforcement is the
    // grep check in plan-03 acceptance: `errors.rs` MUST NOT derive Serialize.
    // Compile-time assertion of absence-of-impl needs nightly negative bounds.
    let bytes = include_bytes!("../src/errors.rs");
    let src = std::str::from_utf8(bytes).expect("utf8");
    // Check derive attribute lines only — comments may legitimately mention serde.
    for line in src.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("#[derive(") || trimmed.starts_with("use serde") {
            assert!(
                !trimmed.contains("Serialize") && !trimmed.contains("Deserialize"),
                "errors.rs must not derive/import serde traits: {line}"
            );
        }
    }
}
