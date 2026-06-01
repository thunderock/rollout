//! MMLU scorer (D-EVAL-03) — reports both `acc` and `acc_norm`.
//!
//! Mirrors lm-evaluation-harness `mmlu` (pinned [`crate::LM_EVAL_VERSION`]):
//! the policy scores each of the four answer continuations by total
//! log-likelihood (greedy, `temperature = 0`).
//! - `acc`     = argmax over raw per-choice total log-likelihood.
//! - `acc_norm`= argmax over length-normalized log-likelihood (divided by the
//!   continuation byte length), matching lm-eval's headline pair.
//!
//! Prompt format: lm-eval's `mmlu` default — question, four `A.`/`B.`/`C.`/`D.`
//! labelled choices, then an `"Answer:"` suffix; the model scores each letter.

use crate::datasets::MmluRow;

/// The four MMLU answer letters, in index order.
pub const CHOICES: [char; 4] = ['A', 'B', 'C', 'D'];

/// Per-example MMLU score (one bit per metric).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MmluItemScore {
    /// Raw argmax matched the gold letter.
    pub acc: bool,
    /// Length-normalized argmax matched the gold letter.
    pub acc_norm: bool,
}

/// Format the 0-shot MMLU prompt for `row` (lm-eval `mmlu` default).
#[must_use]
pub fn format_prompt(row: &MmluRow) -> String {
    let mut s = String::with_capacity(row.question.len() + 64);
    s.push_str(&row.question);
    s.push('\n');
    for (i, choice) in row.choices.iter().enumerate() {
        s.push(CHOICES[i]);
        s.push_str(". ");
        s.push_str(choice);
        s.push('\n');
    }
    s.push_str("Answer:");
    s
}

/// Score one example given the four continuation log-likelihoods.
///
/// `logprobs[i]` is the total log-likelihood of choice `i`'s continuation;
/// `cont_lens[i]` its byte length (for `acc_norm`). `gold` is the 0-based gold
/// choice index. Returns the per-metric correctness bits.
#[must_use]
pub fn score_item(logprobs: &[f64; 4], cont_lens: &[usize; 4], gold: usize) -> MmluItemScore {
    let argmax = argmax4(logprobs);
    // Continuation lengths are tiny (single-token-to-few-char); f64 is exact here.
    #[allow(clippy::cast_precision_loss)]
    let normed: [f64; 4] = [
        logprobs[0] / cont_lens[0].max(1) as f64,
        logprobs[1] / cont_lens[1].max(1) as f64,
        logprobs[2] / cont_lens[2].max(1) as f64,
        logprobs[3] / cont_lens[3].max(1) as f64,
    ];
    let argmax_norm = argmax4(&normed);
    MmluItemScore {
        acc: argmax == gold,
        acc_norm: argmax_norm == gold,
    }
}

fn argmax4(v: &[f64; 4]) -> usize {
    let mut best = 0;
    for i in 1..4 {
        if v[i] > v[best] {
            best = i;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acc_and_acc_norm_pick_independently() {
        // Raw argmax = index 1 (-2 > -3); but per-token (length-normalized)
        // index 0 wins (-0.3 > -1.0). Gold = 0 → acc fails, acc_norm passes.
        let logprobs = [-3.0, -2.0, -8.0, -9.0];
        let cont_lens = [10usize, 2, 1, 1];
        let s = score_item(&logprobs, &cont_lens, 0);
        assert!(!s.acc, "raw argmax should pick choice 1");
        assert!(s.acc_norm, "length-normalized argmax should pick choice 0");
    }

    #[test]
    fn both_metrics_agree_when_unambiguous() {
        let logprobs = [-0.1, -3.0, -3.0, -3.0];
        let cont_lens = [10usize, 10, 10, 10];
        let s = score_item(&logprobs, &cont_lens, 0);
        assert!(s.acc && s.acc_norm);
    }

    #[test]
    fn prompt_has_four_labelled_choices_and_answer_suffix() {
        let row = MmluRow {
            question: "2+2=?".into(),
            choices: ["3".into(), "4".into(), "5".into(), "6".into()],
            answer: 1,
        };
        let p = format_prompt(&row);
        assert!(p.contains("A. 3"));
        assert!(p.contains("D. 6"));
        assert!(p.trim_end().ends_with("Answer:"));
    }
}
