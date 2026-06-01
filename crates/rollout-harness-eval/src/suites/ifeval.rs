//! `IFEval` scorer (D-EVAL-04) — strict, non-language constraints only.
//!
//! Mirrors lm-evaluation-harness `ifeval` (pinned [`crate::LM_EVAL_VERSION`]).
//! Implements the verifiable non-language instructions in pure Rust (regex /
//! string-ops). Language-detection instructions (`language:response_language`)
//! are SKIPPED and warned at load time — see the denominator policy below + the
//! crate README.
//!
//! ## Denominator policy (stated, per D-EVAL-04)
//! A single skipped (language) instruction is dropped from its prompt's
//! instruction list. The prompt is still scored on its remaining instructions;
//! if ALL of a prompt's instructions are skipped, that prompt is excluded from
//! both the instruction-level and prompt-level strict denominators entirely.
//!
//! Reported: instruction-level strict accuracy + prompt-level strict accuracy.

use crate::datasets::IfevalRow;

/// Instruction ids that require language detection — unsupported in v1.1.
pub const SKIPPED_LANGUAGE_PREFIX: &str = "language:";

/// Aggregate `IFEval` strict accuracies over a set of prompts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IfevalScore {
    /// Fraction of (non-skipped) instructions satisfied.
    pub instruction_strict_acc: f64,
    /// Fraction of prompts whose every (non-skipped) instruction was satisfied.
    pub prompt_strict_acc: f64,
    /// Count of language-detection instructions skipped (warned at load).
    pub skipped_language: usize,
}

/// Score one (prompt, response) pair. Returns `(satisfied, total, skipped)`
/// instruction counts and whether the prompt fully passed (None if all skipped).
#[must_use]
pub fn score_prompt(row: &IfevalRow, response: &str) -> (usize, usize, usize, Option<bool>) {
    let mut satisfied = 0usize;
    let mut total = 0usize;
    let mut skipped = 0usize;
    let mut all_pass = true;
    for instr in &row.instructions {
        if instr.id.starts_with(SKIPPED_LANGUAGE_PREFIX) {
            skipped += 1;
            continue;
        }
        total += 1;
        if check_instruction(&instr.id, &instr.kwargs, response) {
            satisfied += 1;
        } else {
            all_pass = false;
        }
    }
    let prompt_pass = (total > 0).then_some(all_pass);
    (satisfied, total, skipped, prompt_pass)
}

/// Score a batch of prompts; emits a one-shot warning if any constraints were skipped.
#[must_use]
pub fn score_batch(rows: &[IfevalRow], responses: &[&str]) -> IfevalScore {
    let mut sat = 0usize;
    let mut tot = 0usize;
    let mut skipped = 0usize;
    let mut prompt_pass = 0usize;
    let mut prompt_total = 0usize;
    for (row, resp) in rows.iter().zip(responses) {
        let (s, t, sk, pass) = score_prompt(row, resp);
        sat += s;
        tot += t;
        skipped += sk;
        if let Some(p) = pass {
            prompt_total += 1;
            if p {
                prompt_pass += 1;
            }
        }
    }
    if skipped > 0 {
        tracing::warn!(
            "IFEval: skipping {skipped} language-detection constraints (unsupported in v1.1)"
        );
    }
    IfevalScore {
        instruction_strict_acc: ratio(sat, tot),
        prompt_strict_acc: ratio(prompt_pass, prompt_total),
        skipped_language: skipped,
    }
}

fn ratio(num: usize, den: usize) -> f64 {
    if den == 0 {
        0.0
    } else {
        // Counts are bounded by the tiny fixture/full-split size; f64 is exact.
        #[allow(clippy::cast_precision_loss)]
        {
            num as f64 / den as f64
        }
    }
}

/// Extract a non-negative integer kwarg as `usize` (saturating; absent → 0).
fn kw_usize(kwargs: &serde_json::Value, key: &str) -> usize {
    kwargs
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .and_then(|v| usize::try_from(v).ok())
        .unwrap_or(0)
}

/// Strict check for a single non-language verifiable instruction.
fn check_instruction(id: &str, kwargs: &serde_json::Value, response: &str) -> bool {
    match id {
        "length_constraints:number_words" => {
            let words = response.split_whitespace().count();
            let relation = kwargs
                .get("relation")
                .and_then(|v| v.as_str())
                .unwrap_or("at least");
            let n = kw_usize(kwargs, "num_words");
            match relation {
                "at least" => words >= n,
                "less than" => words < n,
                "at most" => words <= n,
                _ => false,
            }
        }
        "length_constraints:number_sentences" => {
            let sentences = response
                .split(['.', '!', '?'])
                .filter(|s| !s.trim().is_empty())
                .count();
            let relation = kwargs
                .get("relation")
                .and_then(|v| v.as_str())
                .unwrap_or("at least");
            let n = kw_usize(kwargs, "num_sentences");
            match relation {
                "at least" => sentences >= n,
                "less than" => sentences < n,
                "at most" => sentences <= n,
                _ => false,
            }
        }
        "detectable_format:number_bullet_lists" => {
            let bullets = response
                .lines()
                .filter(|l| {
                    let t = l.trim_start();
                    t.starts_with("* ") || t.starts_with("- ")
                })
                .count();
            let n = kw_usize(kwargs, "num_bullets");
            bullets == n
        }
        "detectable_format:json_format" => {
            let trimmed = strip_code_fence(response.trim());
            serde_json::from_str::<serde_json::Value>(trimmed).is_ok()
        }
        "keywords:existence" => kwargs
            .get("keywords")
            .and_then(|v| v.as_array())
            .is_some_and(|kws| {
                let lower = response.to_lowercase();
                kws.iter()
                    .filter_map(|k| k.as_str())
                    .all(|k| lower.contains(&k.to_lowercase()))
            }),
        "keywords:frequency" => {
            let kw = kwargs.get("keyword").and_then(|v| v.as_str()).unwrap_or("");
            let n = kw_usize(kwargs, "frequency");
            let relation = kwargs
                .get("relation")
                .and_then(|v| v.as_str())
                .unwrap_or("at least");
            let count = count_occurrences(&response.to_lowercase(), &kw.to_lowercase());
            match relation {
                "at least" => count >= n,
                "less than" => count < n,
                "at most" => count <= n,
                _ => false,
            }
        }
        "change_case:english_lowercase" => response
            .chars()
            .filter(|c| c.is_alphabetic())
            .all(char::is_lowercase),
        "change_case:english_capital" => response
            .chars()
            .filter(|c| c.is_alphabetic())
            .all(char::is_uppercase),
        "detectable_content:number_placeholders" => {
            let n = kw_usize(kwargs, "num_placeholders");
            count_placeholders(response) >= n
        }
        // Unknown / unmodelled non-language constraint: conservatively fail
        // (never silently pass an instruction we cannot verify).
        _ => false,
    }
}

fn strip_code_fence(s: &str) -> &str {
    let s = s
        .strip_prefix("```json")
        .or_else(|| s.strip_prefix("```"))
        .unwrap_or(s);
    s.strip_suffix("```").unwrap_or(s).trim()
}

fn count_occurrences(haystack: &str, needle: &str) -> usize {
    if needle.is_empty() {
        return 0;
    }
    haystack.matches(needle).count()
}

fn count_placeholders(s: &str) -> usize {
    let mut count = 0;
    let mut depth = 0;
    for c in s.chars() {
        match c {
            '[' => depth += 1,
            ']' if depth > 0 => {
                count += 1;
                depth -= 1;
            }
            _ => {}
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datasets::Instruction;

    fn row(id: &str, kwargs: serde_json::Value) -> IfevalRow {
        IfevalRow {
            key: 0,
            prompt: "p".into(),
            instructions: vec![Instruction {
                id: id.into(),
                kwargs,
            }],
        }
    }

    #[test]
    fn word_count_constraint_strict() {
        let r = row(
            "length_constraints:number_words",
            serde_json::json!({"relation": "at least", "num_words": 3}),
        );
        let (s, t, sk, pass) = score_prompt(&r, "one two three four");
        assert_eq!((s, t, sk), (1, 1, 0));
        assert_eq!(pass, Some(true));
        let (s2, _, _, pass2) = score_prompt(&r, "one two");
        assert_eq!(s2, 0);
        assert_eq!(pass2, Some(false));
    }

    #[test]
    fn json_format_constraint() {
        let r = row("detectable_format:json_format", serde_json::json!({}));
        assert_eq!(score_prompt(&r, "{\"a\": 1}").3, Some(true));
        assert_eq!(score_prompt(&r, "not json").3, Some(false));
    }

    #[test]
    fn keyword_frequency_constraint() {
        let r = row(
            "keywords:frequency",
            serde_json::json!({"keyword": "rust", "frequency": 2, "relation": "at least"}),
        );
        assert_eq!(score_prompt(&r, "rust is rust").3, Some(true));
        assert_eq!(score_prompt(&r, "rust only once").3, Some(false));
    }

    #[test]
    fn language_constraint_is_skipped_and_warned() {
        let r = row(
            "language:response_language",
            serde_json::json!({"language": "fr"}),
        );
        let (s, t, sk, pass) = score_prompt(&r, "anything");
        assert_eq!((s, t, sk), (0, 0, 1));
        assert_eq!(pass, None, "all-skipped prompt excluded from denominator");
        let score = score_batch(&[r], &["anything"]);
        assert_eq!(score.skipped_language, 1);
    }
}
