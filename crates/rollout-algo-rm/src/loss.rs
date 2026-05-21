//! Bradley-Terry pairwise loss math.
//!
//! Spec 02 §7: `L = -E[ ln σ(r_chosen - r_rejected) ]` where σ is the logistic.
//!
//! Numerically-stable `logsigmoid` via the standard softplus trick so the
//! large-magnitude tails don't overflow.

/// `logsigmoid(x) = ln σ(x)`. Numerically stable for very large |x|.
#[must_use]
pub fn logsigmoid(x: f32) -> f32 {
    if x >= 0.0 {
        -((-x).exp().ln_1p())
    } else {
        x - x.exp().ln_1p()
    }
}

/// Bradley-Terry pairwise loss for a single preference pair.
///
/// Spec 02 §7: `-ln σ(r_chosen - r_rejected)`. Non-negative; near-zero when
/// chosen ≫ rejected, large when rejected ≫ chosen.
#[must_use]
pub fn bradley_terry_loss(r_chosen: f32, r_rejected: f32) -> f32 {
    -logsigmoid(r_chosen - r_rejected)
}

/// Batched Bradley-Terry loss — mean over `pairs`. Returns 0.0 for empty.
#[must_use]
pub fn bradley_terry_batch_mean(pairs: &[(f32, f32)]) -> f32 {
    if pairs.is_empty() {
        return 0.0;
    }
    let sum: f32 = pairs
        .iter()
        .map(|(c, r)| bradley_terry_loss(*c, *r))
        .sum();
    // Batch sizes in this code path are small (minibatch_size: u32); f32 is fine.
    #[allow(clippy::cast_precision_loss)]
    let n = pairs.len() as f32;
    sum / n
}
