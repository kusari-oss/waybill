//! Milestone 839 (#766) — progress reporting for the enrichment phase.
//!
//! The defect in #766 was reported as a hang. It is not: the scan
//! completes. But the last line printed before enrichment is `scan
//! complete`, after which the process is silent for minutes, so every
//! operator kills it and gets no SBOM at all. A slow phase that says
//! nothing is indistinguishable from a stuck one.
//!
//! The trigger is **elapsed time, not completed count**. A small
//! component count behind a slow or throttled endpoint produces
//! exactly the same silence as a large one, so counting work would
//! miss the case the bug report is about. It would also spray output
//! on a large-but-fast phase, which is the opposite failure.
//!
//! Time is passed in rather than read internally, so the tests can
//! synthesise an eleven-second phase without taking eleven seconds.

use std::time::{Duration, Instant};
use tracing::info;

/// How long the phase must run before the first line, and the maximum
/// gap between lines thereafter.
///
/// One constant for both: the question a waiting operator asks is "has
/// it been too long since I saw anything", and that question has the
/// same answer at the start of the phase as in the middle of it.
pub const REPORT_INTERVAL: Duration = Duration::from_secs(10);

/// Emits enrichment progress on a time trigger.
pub struct ProgressReporter {
    total: usize,
    start: Instant,
    last_report: Option<Instant>,
    reports: usize,
}

impl ProgressReporter {
    /// Start tracking a phase of `total` items.
    pub fn new(total: usize) -> Self {
        Self {
            total,
            start: Instant::now(),
            last_report: None,
            reports: 0,
        }
    }

    /// When the phase began. A test affordance: it exists so tests
    /// can build `now` values relative to the start without sleeping.
    #[cfg(test)]
    pub fn started_at(&self) -> Instant {
        self.start
    }

    /// Number of lines emitted so far. A test affordance: asserting
    /// silence needs a count, and production has no use for one.
    #[cfg(test)]
    pub fn reports_emitted(&self) -> usize {
        self.reports
    }

    /// Consider emitting progress. Cheap to call per item — it does a
    /// duration comparison and nothing else until the trigger fires.
    ///
    /// Returns whether a line was emitted, which is what the tests
    /// assert on; production callers ignore it.
    pub fn tick(&mut self, completed: usize, now: Instant) -> bool {
        // FR-010: a phase with no work says nothing at all. Not a
        // "0/0 complete" line, which asserts that something happened.
        if self.total == 0 {
            return false;
        }
        let since_last = match self.last_report {
            // FR-009: nothing until the phase has actually been slow.
            None => now.duration_since(self.start),
            // FR-009a: thereafter, never a gap longer than the
            // interval. A single line followed by renewed silence
            // reproduces the defect with a longer fuse.
            Some(last) => now.duration_since(last),
        };
        if since_last < REPORT_INTERVAL {
            return false;
        }
        info!(
            completed,
            total = self.total,
            elapsed_secs = now.duration_since(self.start).as_secs(),
            "enriching dependencies from deps.dev",
        );
        self.last_report = Some(now);
        self.reports += 1;
        true
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn at(r: &ProgressReporter, secs: u64) -> Instant {
        r.started_at() + Duration::from_secs(secs)
    }

    #[test]
    fn silent_before_the_first_interval() {
        // US2 scenario 2: a phase that finishes quickly emits nothing.
        // Most scans are this case, and a line for each would be noise.
        let mut r = ProgressReporter::new(500);
        for s in [0, 1, 5, 9] {
            assert!(!r.tick(s as usize, at(&r, s)));
        }
        assert_eq!(r.reports_emitted(), 0);
    }

    #[test]
    fn reports_once_the_phase_is_slow() {
        let mut r = ProgressReporter::new(500);
        assert!(!r.tick(100, at(&r, 9)));
        assert!(r.tick(120, at(&r, 10)));
        assert_eq!(r.reports_emitted(), 1);
    }

    #[test]
    fn keeps_reporting_at_the_interval() {
        // FR-009a. One line then silence would reproduce the defect
        // with a longer fuse, so assert the cadence continues.
        let mut r = ProgressReporter::new(5000);
        r.tick(100, at(&r, 10));
        assert!(!r.tick(200, at(&r, 15)), "15s is only 5s after the first line");
        assert!(r.tick(300, at(&r, 20)));
        assert!(r.tick(400, at(&r, 31)));
        assert_eq!(r.reports_emitted(), 3);
    }

    #[test]
    fn zero_work_is_completely_silent() {
        // FR-010. Even an arbitrarily long phase with nothing to do
        // says nothing.
        let mut r = ProgressReporter::new(0);
        assert!(!r.tick(0, at(&r, 600)));
        assert_eq!(r.reports_emitted(), 0);
    }

    #[test]
    fn a_long_gap_emits_once_not_once_per_missed_interval() {
        // If the process stalls for a minute, the operator wants one
        // current line, not six stale ones replayed at them.
        let mut r = ProgressReporter::new(500);
        assert!(r.tick(10, at(&r, 60)));
        assert_eq!(r.reports_emitted(), 1);
    }

    #[test]
    fn trigger_is_time_not_completed_count() {
        // The property this module exists for. A tiny component count
        // behind a slow endpoint must still report, and a large count
        // that finishes fast must not.
        let mut slow_and_small = ProgressReporter::new(3);
        assert!(slow_and_small.tick(1, at(&slow_and_small, 12)));

        let mut fast_and_large = ProgressReporter::new(50_000);
        assert!(!fast_and_large.tick(49_999, at(&fast_and_large, 2)));
    }
}
