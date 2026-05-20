//! Run, worker, and content identifier types.
//!
//! `RunId` and `WorkerId` wrap a ULID (lexicographically sortable, k-sortable).
//! `ContentId` is a blake3 hash of bytes — equal hashes imply equal content.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use ulid::Ulid;

/// ULID-based identifier for a single run.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct RunId(
    /// Underlying ULID; JSON-schema represented as a 26-char Crockford string.
    #[schemars(with = "String")]
    pub Ulid,
);

impl fmt::Display for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for RunId {
    type Err = ulid::DecodeError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.parse()?))
    }
}

/// ULID-based identifier for a worker process.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct WorkerId(
    /// Underlying ULID; JSON-schema represented as a 26-char Crockford string.
    #[schemars(with = "String")]
    pub Ulid,
);

impl fmt::Display for WorkerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for WorkerId {
    type Err = ulid::DecodeError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.parse()?))
    }
}

/// blake3 content hash; equality implies content equality.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ContentId(
    /// Raw 32-byte blake3 digest; JSON-schema represented as a 64-char hex string.
    #[schemars(with = "String")]
    pub [u8; 32],
);

impl ContentId {
    /// Compute the blake3 hash of `data`.
    #[must_use]
    pub fn of(data: &[u8]) -> Self {
        Self(*blake3::hash(data).as_bytes())
    }
}

impl fmt::Display for ContentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in &self.0 {
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

impl FromStr for ContentId {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != 64 {
            return Err(format!("ContentId: expected 64 hex chars, got {}", s.len()));
        }
        let mut out = [0u8; 32];
        for i in 0..32 {
            out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).map_err(|e| e.to_string())?;
        }
        Ok(Self(out))
    }
}
