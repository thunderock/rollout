//! GSM8K scorer (D-EVAL-04) — `####` gold extraction + numeric equivalence.
//!
//! Mirrors lm-evaluation-harness `gsm8k` (pinned [`crate::LM_EVAL_VERSION`]),
//! `temperature = 0`:
//! - Gold: the number after the final `####` in the dataset `answer` field,
//!   with commas / `$` / whitespace stripped.
//! - Model: lm-eval's `gsm8k` filter regex, applied verbatim below, takes the
//!   LAST match in the generation.
//! - Score: exact numeric equivalence (both parsed to `f64`, compared).

use std::sync::LazyLock;

use regex::Regex;

/// lm-eval `gsm8k` answer-extraction filter regex (verbatim, `regex: "(-?[$0-9.,]{2,})|(-?[0-9]+)"`).
pub const GSM8K_FILTER_REGEX: &str = r"(-?[$0-9.,]{2,})|(-?[0-9]+)";

static FILTER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(GSM8K_FILTER_REGEX).expect("valid regex"));

/// Extract the gold number from a dataset `answer` field (after the final `####`).
#[must_use]
pub fn extract_gold(answer: &str) -> Option<f64> {
    let tail = answer.rsplit("####").next()?;
    parse_number(tail.trim())
}

/// Extract the model's answer: the LAST regex match in the generation (lm-eval semantics).
#[must_use]
pub fn extract_model_answer(generation: &str) -> Option<f64> {
    let last = FILTER.find_iter(generation).last()?;
    parse_number(last.as_str())
}

/// Score one example: gold extraction + model extraction + numeric equality.
#[must_use]
pub fn score_item(gold_answer: &str, generation: &str) -> bool {
    match (extract_gold(gold_answer), extract_model_answer(generation)) {
        (Some(g), Some(m)) => (g - m).abs() < 1e-6,
        _ => false,
    }
}

fn parse_number(raw: &str) -> Option<f64> {
    let cleaned: String = raw
        .chars()
        .filter(|c| !matches!(c, ',' | '$' | ' ' | '\t' | '\n'))
        .collect();
    let cleaned = cleaned.trim_end_matches('.');
    cleaned.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gold_takes_number_after_final_hashes() {
        let ans = "Step one.\nStep two.\n#### 1,234";
        assert_eq!(extract_gold(ans), Some(1234.0));
    }

    #[test]
    fn model_takes_last_number_via_filter() {
        let gen = "First we get 12 then the answer is 42.";
        assert_eq!(extract_model_answer(gen), Some(42.0));
    }

    #[test]
    fn numeric_equivalence_decides() {
        assert!(score_item("#### 18", "... so the total is 18"));
        assert!(score_item("#### 1,000", "answer: 1000"));
        assert!(!score_item("#### 18", "the answer is 19"));
    }

    #[test]
    fn strips_dollar_and_commas() {
        assert_eq!(extract_gold("#### $2,500"), Some(2500.0));
    }
}
