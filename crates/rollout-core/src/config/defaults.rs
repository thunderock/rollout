//! Default values for config fields. Pure functions only — no I/O, no env reads.

/// Default schema version for new configs.
#[must_use]
pub fn schema_version() -> u32 {
    1
}
