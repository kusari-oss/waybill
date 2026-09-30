//! Milestone 1035 (#1034, #1040) — the Nix derivation closure as SBOM content.
//!
//! Milestone 1034's tier asks nix for package *versions*. This asks for the
//! whole derivation graph, which holds two things that tier cannot reach.
//!
//! **Components no manifest mentions.** Measured on two real Haskell
//! libraries: 216 and 218 artifact inputs present in the closure and absent
//! from today's output, against the 53 and 190 components waybill emits.
//!
//! **Evidence that a vulnerability was already fixed.** nixpkgs backports
//! security patches without moving the version string — `unzip 6.0` carries 11
//! CVEs across 26 patches in both closures. No version-keyed SBOM can express
//! that in either direction.
//!
//! Unlike milestone 1034, this evaluates the **scanned project's own flake**,
//! so repository-authored expressions do run. The safety machinery in
//! `super::eval` — pre-flight, argv guard, budget, degradation reasons — is
//! reused unchanged and becomes load-bearing rather than precautionary.

pub(crate) mod classify;
pub(crate) mod derivation;
pub(crate) mod emit;
pub(crate) mod patches;
pub(crate) mod summary;

use std::path::Path;
use std::time::Duration;

use super::eval::invoke::{argv_is_safe, run_bounded};
use super::eval::preflight::IFD_SETTING;
use super::eval::reason::DegradationReason;
use super::eval::result::NixSystem;
use classify::DerivationRole;
use derivation::RawClosure;

/// Default budget for acquiring and querying a closure.
///
/// **Provisional**, and labelled so for the same reason milestone 1034's is:
/// the budget spans acquisition as well as query, because nix fetches during
/// evaluation and there is no seam to bound separately. Measured warm-store
/// cost is 1.09s / 5.7MB and 1.07s / 7.4MB on two real projects; cold-store
/// cost is unmeasured and needs a clean runner (research T-R3).
pub(crate) const PROVISIONAL_CLOSURE_BUDGET_SECS: u64 = 300;

/// What the operator asked for.
#[derive(Debug, Clone)]
pub(crate) struct ClosureConfig {
    /// Attribute under `packages.<system>`; `default` unless overridden.
    pub(crate) attribute: String,
    pub(crate) budget: Duration,
}

impl ClosureConfig {
    pub(crate) fn from_flags(attribute: Option<String>, budget_secs: Option<u64>) -> Self {
        Self {
            attribute: attribute.unwrap_or_else(|| "default".to_string()),
            budget: Duration::from_secs(
                budget_secs.unwrap_or(PROVISIONAL_CLOSURE_BUDGET_SECS),
            ),
        }
    }
}

/// A classified closure, ready for emission.
#[derive(Debug)]
pub(crate) struct ClassifiedClosure {
    pub(crate) attribute: String,
    pub(crate) raw: RawClosure,
    pub(crate) roles: std::collections::BTreeMap<String, DerivationRole>,
}

impl ClassifiedClosure {
    /// Members in each role, for the document-scope record (FR-018).
    pub(crate) fn role_counts(&self) -> std::collections::BTreeMap<&'static str, usize> {
        let mut counts = std::collections::BTreeMap::new();
        for role in self.roles.values() {
            *counts.entry(role.wire()).or_insert(0) += 1;
        }
        counts
    }
}

/// Build the argv for a closure query.
///
/// Separated so the safety properties are assertable without running nix, and
/// so production and tests share one definition rather than two that drift.
///
/// The flake is addressed through the **CLI flakeref form**. That is not a
/// style choice: `builtins.getFlake` on a local path requires `--impure`
/// (measured, research R4), and `--impure` is refused by `argv_is_safe`.
fn closure_argv(flakeref: &str) -> Vec<&str> {
    vec![
        "derivation",
        "show",
        "-r",
        "--option",
        IFD_SETTING,
        "false",
        flakeref,
    ]
}

/// Build the argv that lists the attributes under `packages.<system>`.
fn attributes_argv(flakeref: &str) -> Vec<&str> {
    vec![
        "eval",
        "--json",
        "--option",
        IFD_SETTING,
        "false",
        "--apply",
        "builtins.attrNames",
        flakeref,
    ]
}

/// Decide whether the tier may run at all, before any nix process starts.
///
/// `--offline` refuses it, for milestone 1034's measured reason: resolving a
/// flake reference fetches when the store lacks it, and nix's own `--offline`
/// governs substituters rather than flake inputs. A promise of "no outbound
/// network calls" kept only when a cache happens to be warm is not a promise.
///
/// Separate from [`resolve`] so the refusal is assertable without a `nix` on
/// the bench, and so the *order* is unambiguous: this is checked first, and a
/// refusal here means no subprocess is spawned at all.
pub(crate) fn admission(enabled: bool, offline: bool) -> Result<(), Option<DegradationReason>> {
    if !enabled {
        // Not a degradation — nothing was asked for, and nothing is recorded.
        return Err(None);
    }
    if offline {
        return Err(Some(DegradationReason::OfflineRequested));
    }
    Ok(())
}

/// Take and classify the closure of one flake attribute.
///
/// Every failure degrades. The closure supplements the manifest-derived set
/// (spec FR-003a), so losing it costs only the supplement.
pub(crate) fn resolve(
    project: &Path,
    system: &NixSystem,
    cfg: &ClosureConfig,
) -> Result<ClassifiedClosure, DegradationReason> {
    let base = format!("{}#packages.{}", project.display(), system.as_str());

    // Which attributes exist? Answers FR-015b's "name what IS available"
    // without a second failure mode when the attribute is simply missing.
    let list_ref = base.clone();
    let list_argv = attributes_argv(&list_ref);
    guard(&list_argv)?;
    let listed = run_bounded(&list_argv, cfg.budget)?;
    if !listed.status_success {
        return Err(DegradationReason::NoEvaluableAttribute);
    }
    let available: Vec<String> =
        serde_json::from_str(listed.stdout.trim()).unwrap_or_default();
    if !available.contains(&cfg.attribute) {
        tracing::info!(
            requested = %cfg.attribute,
            available = %available.join(", "),
            "nix-closure: requested attribute is absent"
        );
        return Err(DegradationReason::NoEvaluableAttribute);
    }

    let flakeref = format!("{base}.{}", cfg.attribute);
    let argv = closure_argv(&flakeref);
    guard(&argv)?;
    let out = run_bounded(&argv, cfg.budget)?;
    if !out.status_success {
        return Err(DegradationReason::EvaluationFailed(
            out.stderr.trim().chars().take(200).collect(),
        ));
    }

    let raw = RawClosure::parse(&out.stdout).map_err(|e| {
        DegradationReason::EvaluationFailed(format!("closure json unparseable: {e}"))
    })?;
    let roles = classify::classify(&raw);
    tracing::info!(
        attribute = %cfg.attribute,
        derivations = raw.derivations.len(),
        "nix-closure: closure classified"
    );
    Ok(ClassifiedClosure {
        attribute: cfg.attribute.clone(),
        raw,
        roles,
    })
}

/// Refuse an argv that would hand evaluation settings to the scanned flake.
///
/// Reuses milestone 1044's guard rather than reimplementing it — two copies
/// would drift, and the drift would be exactly the thing the guard prevents.
/// It matters more here than it did there: milestone 1034 evaluates nixpkgs at
/// a pinned revision, while this evaluates the project's own flake, and real
/// flakes request `allow-import-from-derivation` through `nixConfig`.
fn guard(argv: &[&str]) -> Result<(), DegradationReason> {
    if argv_is_safe(argv) {
        return Ok(());
    }
    Err(DegradationReason::EvaluationFailed(
        "refusing to evaluate: the nix invocation would hand evaluation \
         settings to the scanned flake"
            .to_string(),
    ))
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn the_closure_argv_stays_pure_and_refuses_ifd() {
        let argv = closure_argv("/proj#packages.x86_64-linux.default");
        assert!(argv_is_safe(&argv), "closure argv must stay pure: {argv:?}");
        assert!(argv.contains(&IFD_SETTING));
        assert_eq!(
            argv.iter().position(|a| *a == IFD_SETTING).map(|i| argv[i + 1]),
            Some("false"),
            "the setting must be passed as false, not merely named"
        );
        assert!(
            !argv.contains(&"--impure"),
            "getFlake on a local path needs --impure; the CLI flakeref form \
             exists precisely to avoid it"
        );
    }

    #[test]
    fn the_attribute_listing_argv_is_guarded_too() {
        // Both invocations touch the project's flake, so both need the guard.
        assert!(argv_is_safe(&attributes_argv("/proj#packages.x86_64-linux")));
    }

    #[test]
    fn the_guard_rejects_what_it_exists_to_reject() {
        assert!(guard(&closure_argv("/p#a")).is_ok());
        assert!(guard(&["derivation", "show", "--accept-flake-config"]).is_err());
        assert!(guard(&["derivation", "show", "--impure"]).is_err());
    }

    #[test]
    fn offline_refuses_before_any_process_starts() {
        // The refusal must precede the subprocess, not follow it. Checking
        // after spawning would already have made the call `--offline`
        // promised would not happen.
        let refused = admission(true, true).unwrap_err().unwrap();
        assert_eq!(refused.wire(), "offline-requested");
    }

    #[test]
    fn the_flag_off_path_is_not_a_degradation() {
        // Nothing was asked for, so nothing is recorded — distinct from
        // asking and being refused, which a consumer must be able to tell
        // apart.
        assert!(admission(false, false).unwrap_err().is_none());
        assert!(admission(false, true).unwrap_err().is_none());
    }

    #[test]
    fn enabled_and_online_is_admitted() {
        assert!(admission(true, false).is_ok());
    }

    #[test]
    fn default_attribute_unless_overridden() {
        assert_eq!(ClosureConfig::from_flags(None, None).attribute, "default");
        assert_eq!(
            ClosureConfig::from_flags(Some("pkg-ghc96".into()), None).attribute,
            "pkg-ghc96"
        );
    }
}
