//! Verify the import-from-derivation refusal is *in effect* before evaluating.
//!
//! The obvious implementation — pass `--option allow-import-from-derivation
//! false` and evaluate — does not work, and fails silently. Measured (research
//! R3, reproducible via `specs/1034-nix-eval-tier/measurements/`):
//!
//! ```text
//! $ nix eval --expr '1' --impure --option definitely-not-a-real-option true
//! warning: unknown setting 'definitely-not-a-real-option'
//! 1
//! $ echo $?
//! 0
//! ```
//!
//! A `nix` that does not know the setting accepts the flag, warns, and carries
//! on with exit code 0. So a waybill that merely *passed* the option would
//! evaluate with import-from-derivation enabled while every observable signal
//! — exit code, stdout — looked exactly like success.
//!
//! `nix config show` reflects an override only when the setting is real, which
//! makes the difference observable:
//!
//! ```text
//! $ nix config show --option allow-import-from-derivation false | grep allow-import
//! allow-import-from-derivation = false
//! $ nix config show --option definitely-not-a-real-option false | grep definitely
//! warning: unknown setting 'definitely-not-a-real-option'     # never as a setting
//! ```
//!
//! This deliberately does **not** gate on a `nix` version. Determinate Nix
//! 3.20.0 reports itself as `nix 2.34.6`, so a version parser has two numbering
//! schemes to reconcile before forks and vendor builds are considered.
//! Detecting the capability is both simpler and strictly more accurate.

use super::reason::DegradationReason;
use super::result::IfdRefusalVerified;

/// The setting whose presence the tier depends on.
pub(crate) const IFD_SETTING: &str = "allow-import-from-derivation";

/// The exact line `nix config show` prints when the override took effect.
const IFD_DISABLED_LINE: &str = "allow-import-from-derivation = false";

/// Decide, from `nix config show --option allow-import-from-derivation false`
/// output, whether the refusal is in effect.
///
/// Split from the subprocess call so the decision is testable against captured
/// output without a `nix` on the bench.
pub(crate) fn verify_from_config_output(
    stdout: &str,
) -> Result<IfdRefusalVerified, DegradationReason> {
    let present = stdout
        .lines()
        .any(|l| l.trim() == IFD_DISABLED_LINE);

    if present {
        Ok(IfdRefusalVerified::attest())
    } else {
        Err(DegradationReason::IfdRefusalUnverified(format!(
            "`nix config show` did not report `{IFD_DISABLED_LINE}`; this nix \
             likely does not support `{IFD_SETTING}`, and would silently \
             evaluate with import-from-derivation enabled"
        )))
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    /// Captured from `nix config show --option allow-import-from-derivation
    /// false` on Determinate Nix 3.20.0, trimmed to the neighbouring settings.
    const SUPPORTED: &str = "\
allow-dirty = true
allow-import-from-derivation = false
allow-new-privileges = false
allow-symlinked-store = false
";

    /// What a nix without the setting prints: the warning goes to stderr, and
    /// the setting simply never appears in the config listing.
    const UNSUPPORTED: &str = "\
allow-dirty = true
allow-new-privileges = false
allow-symlinked-store = false
";

    #[test]
    fn accepts_when_the_override_is_reflected() {
        assert!(verify_from_config_output(SUPPORTED).is_ok());
    }

    #[test]
    fn degrades_when_the_setting_is_absent_rather_than_assuming_it_applied() {
        let err = verify_from_config_output(UNSUPPORTED).unwrap_err();
        assert!(matches!(err, DegradationReason::IfdRefusalUnverified(_)));
        assert_eq!(err.wire(), "ifd-refusal-unverified");
    }

    #[test]
    fn does_not_accept_the_setting_left_at_its_permissive_default() {
        // The whole point: `= true` means IFD is ON. Matching the setting
        // *name* rather than the whole line would pass here, which is the bug
        // this test exists to prevent.
        let permissive = "allow-import-from-derivation = true\n";
        assert!(verify_from_config_output(permissive).is_err());
    }

    #[test]
    fn empty_output_degrades() {
        // A `nix` that failed to run at all produces nothing. Absence of the
        // line is the safe reading in every case.
        assert!(verify_from_config_output("").is_err());
    }
}
