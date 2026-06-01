//! Bundled eval-suite scorers (`MMLU`, `IFEval`, `GSM8K`).
//!
//! Each scorer mirrors lm-evaluation-harness conventions at the pinned
//! [`crate::LM_EVAL_VERSION`]; see the per-module docs for the exact definition.

pub mod gsm8k;
pub mod ifeval;
pub mod mmlu;

use serde::{Deserialize, Serialize};

/// The bundled eval suites this crate ships.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Suite {
    /// `MMLU` multiple-choice (`acc` + `acc_norm`).
    Mmlu,
    /// `IFEval` instruction-following (strict, non-language).
    Ifeval,
    /// `GSM8K` grade-school math (`####` numeric).
    Gsm8k,
}

impl Suite {
    /// The suite's stable lowercase name (used in descriptors + queue ids).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Suite::Mmlu => "mmlu",
            Suite::Ifeval => "ifeval",
            Suite::Gsm8k => "gsm8k",
        }
    }
}
