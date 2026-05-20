//! Deadline-based health helpers (spec 05 §6, RESEARCH Pattern 5).
//!
//! `next_due_at(now, hb_interval) = now + hb_interval × 2` — one period of
//! slack so a single missed heartbeat doesn't trigger a failure scan.
//!
//! `is_failed` returns true only when BOTH the clock-skew budget AND the
//! coordinator failure timeout have elapsed past `due_at`.

use std::time::{Duration, SystemTime};

/// Compute the next heartbeat deadline with one period of slack.
#[must_use]
pub fn next_due_at(now: SystemTime, hb_interval: Duration) -> SystemTime {
    now + hb_interval * 2
}

/// Whether a worker should be marked failed given its last `due_at`.
///
/// Returns `true` only when `now > due_at + skew` AND `now > due_at + coord_timeout`.
#[must_use]
pub fn is_failed(
    now: SystemTime,
    due_at: SystemTime,
    skew: Duration,
    coord_timeout: Duration,
) -> bool {
    let Ok(elapsed_past_due) = now.duration_since(due_at) else {
        return false;
    };
    elapsed_past_due > skew && elapsed_past_due > coord_timeout
}

#[cfg(test)]
mod tests {
    use super::{is_failed, next_due_at, Duration, SystemTime};

    #[test]
    fn next_due_at_adds_two_intervals() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        let hb = Duration::from_millis(500);
        let due = next_due_at(now, hb);
        assert_eq!(due, now + Duration::from_secs(1));
    }

    #[test]
    fn is_failed_only_when_both_thresholds_passed() {
        let due = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        let skew = Duration::from_millis(250);
        let coord = Duration::from_secs(5);

        // Before due_at: never failed.
        assert!(!is_failed(due - Duration::from_secs(1), due, skew, coord));
        // Just past due_at: under both thresholds.
        assert!(!is_failed(due + Duration::from_millis(100), due, skew, coord));
        // Past skew but under coord_timeout.
        assert!(!is_failed(due + Duration::from_secs(1), due, skew, coord));
        // Past both thresholds.
        assert!(is_failed(due + Duration::from_secs(6), due, skew, coord));
    }
}
