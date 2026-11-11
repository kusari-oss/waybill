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
use super::result::{IfdAllowanceVerified, IfdPolicyVerified, IfdRefusalVerified};

/// The setting whose presence the tier depends on.
pub(crate) const IFD_SETTING: &str = "allow-import-from-derivation";

/// The key of nix's sandbox setting in `nix config show`.
const SANDBOX_KEY: &str = "sandbox";

/// Milestone 1283 (#1283) — the import-from-derivation policy the operator
/// requested.
///
/// `Refuse` unless `--nix-allow-ifd` was passed. [`Self::setting`] is the only
/// source of the value placed after [`IFD_SETTING`] in any `nix` argv, so a
/// scan cannot half-allow: one call site allowing and another refusing would
/// let the closure be read while the license evaluation was refused (research
/// R1). Requesting is not having, so every evaluating entry point takes an
/// [`IfdPolicyVerified`], which only [`verify_for`] produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum IfdPolicy {
    #[default]
    Refuse,
    Allow,
}

impl IfdPolicy {
    /// The value nix is passed for [`IFD_SETTING`].
    pub(crate) fn setting(&self) -> &'static str {
        match self {
            Self::Refuse => "false",
            Self::Allow => "true",
        }
    }

    /// The C217 `import-from-derivation` value.
    pub(crate) fn wire(&self) -> &'static str {
        match self {
            Self::Refuse => "refused",
            Self::Allow => "allowed",
        }
    }
}

/// Milestone 1283 — what the pre-flight observed about nix's environment,
/// recorded and never changed (C217).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct NixEnvironment {
    /// nix's `sandbox` setting, verbatim (`true` / `false` / `relaxed`), or
    /// `None` when `nix config show` printed no such line. The key is matched
    /// exactly: `sandbox-fallback` and `sandbox-paths` are other settings.
    pub(crate) sandbox: Option<String>,
}

impl NixEnvironment {
    fn from_config_output(stdout: &str) -> Self {
        let sandbox = stdout.lines().find_map(|l| {
            let (k, v) = l.split_once('=')?;
            (k.trim() == SANDBOX_KEY)
                .then(|| v.trim().to_string())
                .filter(|v| !v.is_empty())
        });
        Self { sandbox }
    }
}

/// Decide, from `nix config show --option allow-import-from-derivation
/// <policy>` output, whether the requested policy is in effect.
///
/// Split from the subprocess call so the decision is testable against captured
/// output without a `nix` on the bench.
///
/// The whole line is matched, never the setting's name: `= true` under a
/// requested refusal means IFD is ON, and `= false` under a requested
/// allowance means the opt-in did not apply (an old nix, or a configuration
/// that wins over the command line). Each is its own degradation, because the
/// operator's remedy differs.
pub(crate) fn verify_for(
    policy: IfdPolicy,
    stdout: &str,
) -> Result<(IfdPolicyVerified, NixEnvironment), DegradationReason> {
    let expected = format!("{IFD_SETTING} = {}", policy.setting());
    let present = stdout.lines().any(|l| l.trim() == expected);
    if !present {
        return Err(match policy {
            IfdPolicy::Refuse => DegradationReason::IfdRefusalUnverified(format!(
                "`nix config show` did not report `{expected}`; this nix \
                 likely does not support `{IFD_SETTING}`, and would silently \
                 evaluate with import-from-derivation enabled"
            )),
            IfdPolicy::Allow => DegradationReason::IfdAllowanceUnverified(format!(
                "`nix config show` did not report `{expected}`; --nix-allow-ifd \
                 was requested and nix did not apply it, so nothing was evaluated"
            )),
        });
    }
    let verified = match policy {
        IfdPolicy::Refuse => IfdPolicyVerified::Refused(IfdRefusalVerified::attest()),
        IfdPolicy::Allow => IfdPolicyVerified::Allowed(IfdAllowanceVerified::attest()),
    };
    Ok((verified, NixEnvironment::from_config_output(stdout)))
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
        assert!(verify_for(IfdPolicy::Refuse, SUPPORTED).is_ok());
    }

    #[test]
    fn degrades_when_the_setting_is_absent_rather_than_assuming_it_applied() {
        let err = verify_for(IfdPolicy::Refuse, UNSUPPORTED).unwrap_err();
        assert!(matches!(err, DegradationReason::IfdRefusalUnverified(_)));
        assert_eq!(err.wire(), "ifd-refusal-unverified");
    }

    #[test]
    fn does_not_accept_the_setting_left_at_its_permissive_default() {
        // The whole point: `= true` means IFD is ON. Matching the setting
        // *name* rather than the whole line would pass here, which is the bug
        // this test exists to prevent.
        let permissive = "allow-import-from-derivation = true\n";
        assert!(verify_for(IfdPolicy::Refuse, permissive).is_err());
    }

    // ── T006 (#1283) — the requested value is what is verified ──────────

    /// Captured from `nix config show --option allow-import-from-derivation
    /// true` on Determinate Nix 3.20.0 (aarch64-darwin), trimmed. The
    /// neighbouring `sandbox-*` keys are kept: matching the key `sandbox` by
    /// prefix would read them.
    const ALLOWED_SANDBOX_OFF: &str = "\
allow-dirty = true
allow-import-from-derivation = true
sandbox = false
sandbox-fallback = true
sandbox-paths = /System/Library/Frameworks /bin/sh
";

    #[test]
    fn t006_allow_with_the_allowance_reflected_is_allowed() {
        let (v, env) = verify_for(IfdPolicy::Allow, ALLOWED_SANDBOX_OFF).unwrap();
        assert!(matches!(v, IfdPolicyVerified::Allowed(_)), "{v:?}");
        assert_eq!(v.policy(), IfdPolicy::Allow);
        assert_eq!(env.sandbox.as_deref(), Some("false"));
    }

    #[test]
    fn t006_allow_with_the_refusal_still_in_effect_is_unverified() {
        // An old nix, or a config that wins over the command line: the
        // allowance was requested and not applied.
        let err = verify_for(
            IfdPolicy::Allow,
            "allow-import-from-derivation = false\nsandbox = true\n",
        )
        .unwrap_err();
        assert!(matches!(err, DegradationReason::IfdAllowanceUnverified(_)), "{err:?}");
        assert_eq!(err.wire(), "ifd-allowance-unverified");
    }

    #[test]
    fn t006_allow_with_the_setting_absent_is_unverified() {
        let err = verify_for(IfdPolicy::Allow, UNSUPPORTED).unwrap_err();
        assert_eq!(err.wire(), "ifd-allowance-unverified");
    }

    #[test]
    fn t006_refuse_with_the_refusal_reflected_is_refused() {
        let (v, env) = verify_for(IfdPolicy::Refuse, SUPPORTED).unwrap();
        assert!(matches!(v, IfdPolicyVerified::Refused(_)), "{v:?}");
        assert_eq!(v.policy(), IfdPolicy::Refuse);
        // SUPPORTED carries no `sandbox` line.
        assert_eq!(env.sandbox, None);
    }

    #[test]
    fn t006_refuse_with_the_permissive_value_is_refusal_unverified() {
        let err = verify_for(IfdPolicy::Refuse, ALLOWED_SANDBOX_OFF).unwrap_err();
        assert!(matches!(err, DegradationReason::IfdRefusalUnverified(_)), "{err:?}");
    }

    #[test]
    fn t006_a_missing_sandbox_line_is_unknown_not_false() {
        let (_, env) =
            verify_for(IfdPolicy::Allow, "allow-import-from-derivation = true\n").unwrap();
        assert_eq!(env.sandbox, None);
    }

    #[test]
    fn t006_the_sandbox_value_is_recorded_verbatim() {
        let (_, env) = verify_for(
            IfdPolicy::Allow,
            "allow-import-from-derivation = true\nsandbox = relaxed\n",
        )
        .unwrap();
        assert_eq!(env.sandbox.as_deref(), Some("relaxed"));
        // `sandbox-fallback` alone is not the sandbox setting.
        let (_, env) = verify_for(
            IfdPolicy::Refuse,
            "allow-import-from-derivation = false\nsandbox-fallback = true\n",
        )
        .unwrap();
        assert_eq!(env.sandbox, None);
    }

    #[test]
    fn t003_the_policy_is_refuse_by_default_and_maps_to_the_argv_value() {
        assert_eq!(IfdPolicy::default(), IfdPolicy::Refuse);
        assert_eq!(IfdPolicy::Refuse.setting(), "false");
        assert_eq!(IfdPolicy::Allow.setting(), "true");
    }

    #[test]
    fn empty_output_degrades() {
        // A `nix` that failed to run at all produces nothing. Absence of the
        // line is the safe reading in every case.
        assert!(verify_for(IfdPolicy::Refuse, "").is_err());
    }
}
