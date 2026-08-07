//! Unit tests for the `WAYBILL_VERSION` build-time env override
//! (feature 229 FR-005 + FR-012).
//!
//! These tests exercise the runtime `waybill::VERSION` constant —
//! the build-time validation lives in `waybill-cli/build.rs` and is
//! implicitly tested by any `cargo build` invocation with the env var
//! set (any invalid input panics the build; valid input propagates).
//!
//! The override-path itself (`option_env!("WAYBILL_VERSION_OVERRIDE")`)
//! is compile-time — it can't be exercised by `std::env::set_var()` at
//! runtime, so the "override applies" path is verified end-to-end at
//! the workflow level (nightly.yml's build sets the env var, its
//! resulting binary reports the override string).
//!
//! Reference: contracts/build-rs-version-override.md.

#[cfg_attr(test, allow(clippy::unwrap_used))]
#[cfg(test)]
mod tests {
    #[test]
    fn version_is_non_empty() {
        // Fallback (no override set at THIS build's time) should
        // report SOME non-empty version string.
        let v = waybill::VERSION;
        assert!(!v.is_empty(), "waybill::VERSION must be non-empty");
    }

    #[test]
    fn version_starts_with_digit() {
        // Every SemVer version starts with a digit — check that
        // the fallback path emits a valid-shaped string.
        let v = waybill::VERSION;
        let first = v.chars().next().expect("version has at least one char");
        assert!(
            first.is_ascii_digit(),
            "waybill::VERSION must start with a digit; got {v:?}"
        );
    }

    #[test]
    fn version_contains_three_dotted_segments() {
        // Loose SemVer shape check: must contain at least 2 dots
        // separating major.minor.patch. Pre-release / build metadata
        // adds `-` or `+` after the patch, which is fine.
        let v = waybill::VERSION;
        let dot_count = v.chars().filter(|&c| c == '.').count();
        assert!(
            dot_count >= 2,
            "waybill::VERSION must have at least 2 dots (SemVer X.Y.Z); got {v:?}"
        );
    }

    #[test]
    fn version_no_env_var_leak_when_override_unset() {
        // At runtime, `option_env!("WAYBILL_VERSION_OVERRIDE")` is
        // resolved at COMPILE time. If build.rs saw WAYBILL_VERSION
        // unset (this test's build path), the const falls through to
        // env!("CARGO_PKG_VERSION") — which matches the Cargo.toml
        // declared version.
        let v = waybill::VERSION;
        // Note: we can't assert v equals env!("CARGO_PKG_VERSION")
        // here because the test binary might have been built WITH
        // WAYBILL_VERSION set (in that case the version override
        // does apply). We just verify it's SOME valid-shaped string.
        assert!(
            !v.contains("$(") && !v.contains("${"),
            "waybill::VERSION must not contain unresolved shell/MSBuild-style variable refs; got {v:?}"
        );
    }
}
