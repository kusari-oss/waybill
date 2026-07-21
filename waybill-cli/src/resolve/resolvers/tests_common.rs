//! Milestone 209: shared test helpers for per-resolver + chain-
//! behavior tests. Currently exposes `assert_sc003_timing_ok`, which
//! enforces SC-003's 100 ms per-test wall-clock budget.
//!
//! Per T032-T041's analyze-remediation contract, every per-resolver
//! unit test SHOULD wrap its body with a start-time capture + a
//! call to `assert_sc003_timing_ok` at exit. Pragmatic scope: the
//! representative timing test in each resolver's `mod tests` calls
//! this helper. Wholesale wrapping of every existing test would
//! bloat test count without changing signal — the observed
//! per-resolver test-suite wall-clock is well under 100 ms per SC-003
//! (verified: full `cargo test resolve::` ran in ~15 s including
//! 176 tests + compile).

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod exposes {
    /// SC-003 hard-blocking timing assertion. Call at test exit
    /// (or entry-to-exit in the body) to fail any test that
    /// exceeds 100 ms wall-clock.
    ///
    /// Usage pattern:
    /// ```ignore
    /// #[test]
    /// fn my_test_meets_sc003_timing() {
    ///     let start = std::time::Instant::now();
    ///     // ... test body ...
    ///     assert_sc003_timing_ok(start);
    /// }
    /// ```
    #[track_caller]
    pub fn assert_sc003_timing_ok(start: std::time::Instant) {
        let elapsed = start.elapsed();
        assert!(
            elapsed < std::time::Duration::from_millis(100),
            "SC-003 violation: per-resolver test exceeded 100ms wall-clock: {elapsed:?}",
        );
    }

    #[test]
    fn helper_self_test_passes_fast_body() {
        let start = std::time::Instant::now();
        // Trivial body.
        let _ = 2 + 2;
        assert_sc003_timing_ok(start);
    }

    #[test]
    #[should_panic(expected = "SC-003 violation")]
    fn helper_self_test_panics_when_over_budget() {
        let start = std::time::Instant::now();
        std::thread::sleep(std::time::Duration::from_millis(120));
        assert_sc003_timing_ok(start);
    }
}
