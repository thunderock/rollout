//! `MockEvalBackend` — a GPU-free deterministic generation source.
//!
//! Mirrors `rollout-runtime-batch::MockBackend`: given a prompt + seed it returns
//! a canned completion (for `IFEval`/`GSM8K`) or per-choice continuation
//! log-likelihoods (for `MMLU`), so the scorers run with no GPU. The real backend
//! path (vLLM via Phase-3) is wired later, behind a feature; the witnesses use
//! this mock only.
//!
//! Determinism: every method is a pure function of `(prompt, seed)` — two runs
//! with the same seed produce identical generations + scores (temp = 0).

use std::collections::HashMap;

/// A deterministic, GPU-free eval backend for the always-on witnesses.
///
/// Holds an optional canned-answer map (prompt → completion / gold choice). When
/// a prompt is absent it falls back to a deterministic hash-derived response so
/// the path never blocks on a GPU.
#[derive(Debug, Default, Clone)]
pub struct MockEvalBackend {
    /// Prompt → canned completion text (`IFEval` / `GSM8K`).
    completions: HashMap<String, String>,
    /// Prompt → gold choice index the mock "prefers" (`MMLU`).
    preferred_choice: HashMap<String, usize>,
}

impl MockEvalBackend {
    /// Empty mock (all responses hash-derived / fallback).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a canned completion for `prompt` (`IFEval` / `GSM8K` generation).
    #[must_use]
    pub fn with_completion(
        mut self,
        prompt: impl Into<String>,
        completion: impl Into<String>,
    ) -> Self {
        self.completions.insert(prompt.into(), completion.into());
        self
    }

    /// Register the choice index this mock prefers for an `MMLU` `prompt`.
    #[must_use]
    pub fn with_preferred_choice(mut self, prompt: impl Into<String>, choice: usize) -> Self {
        self.preferred_choice.insert(prompt.into(), choice);
        self
    }

    /// Deterministic greedy completion for `prompt` (temp = 0; `seed` folded in).
    #[must_use]
    pub fn generate(&self, prompt: &str, seed: u64) -> String {
        if let Some(c) = self.completions.get(prompt) {
            return c.clone();
        }
        // Fallback: stable hash so repeated runs match; not a real model.
        format!("MOCK[{seed}]:{}", det_hash(prompt, seed))
    }

    /// Deterministic per-choice continuation log-likelihoods for an `MMLU` item.
    ///
    /// The preferred choice (registered, else hash-derived) gets the highest
    /// (least-negative) total log-likelihood; lengths are the choice byte lengths
    /// so `acc` and `acc_norm` are both well-defined.
    #[must_use]
    pub fn choice_logprobs(
        &self,
        prompt: &str,
        choices: &[String; 4],
        seed: u64,
    ) -> ([f64; 4], [usize; 4]) {
        let preferred = self
            .preferred_choice
            .get(prompt)
            .copied()
            .unwrap_or_else(|| usize::try_from(det_hash(prompt, seed) % 4).unwrap_or(0));
        let mut lp = [-10.0_f64; 4];
        lp[preferred] = -0.5;
        let lens = [
            choices[0].len(),
            choices[1].len(),
            choices[2].len(),
            choices[3].len(),
        ];
        (lp, lens)
    }
}

fn det_hash(s: &str, seed: u64) -> u64 {
    // FNV-1a folded with the seed; deterministic across runs/platforms.
    let mut h = 0xcbf2_9ce4_8422_2325_u64 ^ seed;
    for b in s.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_is_deterministic() {
        let b = MockEvalBackend::new();
        assert_eq!(b.generate("p", 7), b.generate("p", 7));
        assert_ne!(b.generate("p", 7), b.generate("p", 8));
    }

    #[test]
    fn canned_completion_wins() {
        let b = MockEvalBackend::new().with_completion("q", "the answer is 42");
        assert_eq!(b.generate("q", 0), "the answer is 42");
    }

    #[test]
    fn preferred_choice_has_max_logprob() {
        let choices = ["a".into(), "bb".into(), "ccc".into(), "dddd".into()];
        let b = MockEvalBackend::new().with_preferred_choice("p", 2);
        let (lp, lens) = b.choice_logprobs("p", &choices, 0);
        let argmax = (0..4).max_by(|&i, &j| lp[i].total_cmp(&lp[j])).unwrap();
        assert_eq!(argmax, 2);
        assert_eq!(lens, [1, 2, 3, 4]);
    }
}
