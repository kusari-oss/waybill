#![allow(dead_code)] // Lifted by T016/T017, which wire this into the
// Haskell reader. Until then every item here is reachable only from its own
// tests. Same posture as `file_tier/walker.rs` carried between m133 US1.A
// and US1.B; the alternative is wiring a half-built resolver into the scan
// path to keep the linter quiet.

//! Milestone 926 (#947) — resolve Haskell dependency versions through the
//! nixpkgs revision pinned in `flake.lock`.
//!
//! A Nix-built Haskell project declares ranges in `.cabal` and ships no
//! `cabal.project.freeze`, so every dependency lands at design tier with no
//! version. Those versions are not unknown: the pinned nixpkgs revision
//! determines them, and m925 already reads the lockfile without consulting
//! what it points at.
//!
//! Boundaries this module holds to:
//!
//! - It **enriches** components the Haskell reader already emitted. It never
//!   introduces one (Constitution Principle XII constraint 1, FR-001a). The
//!   transitive closure is deliberately out of scope — see #962.
//! - It never synthesises a version, hash or versioned identifier for a
//!   dependency it could not resolve (Principle IX, FR-005).
//! - A package supplied by the compiler has no version in the package set at
//!   all, and is reported as such rather than resolved from the default set
//!   (FR-014c).

pub(crate) mod boot_libraries;
pub(crate) mod cache;
pub(crate) mod fetch;
pub(crate) mod nix_base32;
pub(crate) mod package_set;

use std::collections::BTreeSet;

use super::lockfile::{FlakeLockDocument, LockedRef, OriginalPinState};
use fetch::{FetchError, Location};

/// Path of the generated Haskell package set within a nixpkgs checkout.
pub(crate) const HACKAGE_PACKAGES_PATH: &str =
    "pkgs/development/haskell-modules/hackage-packages.nix";

/// Cache key for that file.
pub(crate) const HACKAGE_PACKAGES_KEY: &str = "hackage-packages.nix";

/// How an input came to be recognised as nixpkgs-shaped (FR-015b).
///
/// Recorded so an operator can see why resolution did or did not engage,
/// rather than having to infer it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NixpkgsMatch {
    /// The root flake input is named `nixpkgs` — the near-universal
    /// convention.
    RootInputName,
    /// The locked entry's repository component is `nixpkgs`, which is what a
    /// fork or an internal mirror looks like: different owner or host, same
    /// repository name.
    RepositoryName,
}

impl NixpkgsMatch {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            NixpkgsMatch::RootInputName => "root-input-name",
            NixpkgsMatch::RepositoryName => "repository-name",
        }
    }
}

/// The nixpkgs input as `flake.lock` records it.
#[derive(Debug, Clone)]
pub(crate) struct PinnedNixpkgs {
    /// Exact commit the lock pins.
    pub(crate) revision: String,
    /// Where to retrieve it from — derived from the locked entry, never
    /// assumed to be upstream (FR-016).
    pub(crate) location: Option<Location>,
    /// Which rule recognised this input (FR-015b).
    pub(crate) matched_by: NixpkgsMatch,
}

/// Why a declared dependency has no version.
///
/// A closed set. Each variant calls for different operator action, which is
/// the whole reason they are not collapsed into one "unresolved".
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UnresolvedReason {
    /// The compiler supplies it; nixpkgs has no version to give (FR-014a).
    CompilerSupplied,
    /// Reached the revision; the package set does not contain this name.
    AbsentFromPackageSet,
    /// Could not reach, refused, unauthorized, or timed out (FR-017).
    SourceUnreachable,
    /// The lock pins a shape this reader cannot retrieve one file from —
    /// a bare git URL, a tarball. Distinct from `SourceUnreachable` because
    /// the remedy is different: nothing about the network is wrong.
    SourceUnsupported,
    /// The lock pins a moving reference, so there is no reproducible
    /// revision to resolve against (FR-012).
    NoExactRevision,
    /// The operator requested offline operation (FR-009).
    Offline,
}

impl UnresolvedReason {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            UnresolvedReason::CompilerSupplied => "compiler-supplied",
            UnresolvedReason::AbsentFromPackageSet => "absent-from-package-set",
            UnresolvedReason::SourceUnreachable => "source-unreachable",
            UnresolvedReason::SourceUnsupported => "source-unsupported",
            UnresolvedReason::NoExactRevision => "no-exact-revision",
            UnresolvedReason::Offline => "offline",
        }
    }
}

impl From<&FetchError> for UnresolvedReason {
    fn from(e: &FetchError) -> Self {
        match e {
            FetchError::UnsupportedSource => UnresolvedReason::SourceUnsupported,
            // A 404 at the package-set path means the source was reached but
            // carries no Haskell package set. That is not "absent from the
            // package set" — there is no package set — so it degrades the
            // same way an unreachable source does.
            FetchError::NotFound
            | FetchError::Unauthorized
            | FetchError::Unreachable(_)
            | FetchError::TimedOut => UnresolvedReason::SourceUnreachable,
        }
    }
}

/// What resolution concluded about one declared dependency.
///
/// Two variants with no shared fields: there is no representable state in
/// which a dependency carries half a resolution (Principle IV). FR-005 and
/// SC-003 are assertions about this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ResolutionOutcome {
    Resolved {
        version: String,
        /// Hex SHA-256 of the source tarball. `None` when the revision
        /// records no hash — a version without a hash is still worth having.
        source_hash: Option<String>,
        /// The revision this came from, recorded as provenance (FR-007).
        revision: String,
    },
    Unresolved {
        reason: UnresolvedReason,
    },
}

impl ResolutionOutcome {
    pub(crate) fn unresolved(reason: UnresolvedReason) -> Self {
        ResolutionOutcome::Unresolved { reason }
    }
}

/// Find the nixpkgs-shaped input in a parsed lockfile (FR-015b).
///
/// Classified **from the lockfile alone, with no network access**. Probing an
/// input to see whether it exposes a Haskell package set would be more
/// precise and would retrieve for repositories that do not qualify, breaking
/// SC-009 — so shape is judged by name.
///
/// Two rules, in order:
///
/// 1. the root flake input named `nixpkgs` — the near-universal convention;
/// 2. any locked entry whose repository component is `nixpkgs`, which is what
///    a fork or internal mirror looks like (FR-016).
pub(crate) fn find_nixpkgs_input(doc: &FlakeLockDocument) -> Option<(&LockedRef, NixpkgsMatch)> {
    // Rule 1: the root's input named `nixpkgs`.
    if let Some(root) = doc.nodes.get(&doc.root_key) {
        if let Some(edge) = root.inputs.get("nixpkgs") {
            if let Some(key) = doc.resolve(edge) {
                if let Some(locked) = doc.nodes.get(key).and_then(|n| n.locked.as_ref()) {
                    return Some((locked, NixpkgsMatch::RootInputName));
                }
            }
        }
    }
    // Rule 2: any node whose repository is `nixpkgs`. Deterministic because
    // `nodes` is a BTreeMap.
    doc.nodes
        .values()
        .filter_map(|n| n.locked.as_ref())
        .find(|l| l.repo.as_deref() == Some("nixpkgs"))
        .map(|l| (l, NixpkgsMatch::RepositoryName))
}

/// Resolve the pinned nixpkgs, or say why there is none to resolve.
///
/// `Err` carries the reason every declared dependency will take, so the
/// caller never has to invent one.
pub(crate) fn pinned_nixpkgs(doc: &FlakeLockDocument) -> Result<PinnedNixpkgs, UnresolvedReason> {
    let Some((locked, matched_by)) = find_nixpkgs_input(doc) else {
        return Err(UnresolvedReason::NoExactRevision);
    };

    // FR-012: only an exact revision is reproducible. A moving reference has
    // nothing stable to resolve against.
    let original_is_exact = doc
        .nodes
        .values()
        .filter(|n| n.locked.as_ref().is_some_and(|l| std::ptr::eq(l, locked)))
        .filter_map(|n| n.original.as_ref())
        .any(|o| matches!(o.pin_state(locked), OriginalPinState::Exact));

    let Some(revision) = locked.rev.clone().filter(|r| !r.is_empty()) else {
        return Err(UnresolvedReason::NoExactRevision);
    };
    if !original_is_exact {
        // The lock resolved a moving reference to a concrete revision. That
        // revision is reproducible *today*, but re-locking moves it, so the
        // document would claim more stability than it has.
        return Err(UnresolvedReason::NoExactRevision);
    }

    Ok(PinnedNixpkgs {
        revision,
        location: Location::from_locked(locked),
        matched_by,
    })
}

/// Classify one declared dependency against a package set and boot set.
///
/// The ordering is deliberate: boot status is checked **before** package-set
/// membership, because a boot library is often present in the package set
/// with a version the build does not use. Checking membership first would
/// resolve it and produce exactly the invented version FR-014c forbids.
pub(crate) fn classify(
    name: &str,
    packages: &package_set::PackageSet,
    boot: &BTreeSet<String>,
    revision: &str,
) -> ResolutionOutcome {
    if boot_libraries::is_boot_library(name, boot) {
        return ResolutionOutcome::unresolved(UnresolvedReason::CompilerSupplied);
    }
    match packages.get(name) {
        Some(entry) => ResolutionOutcome::Resolved {
            version: entry.version.clone(),
            source_hash: entry.source_hash.clone(),
            revision: revision.to_string(),
        },
        None => ResolutionOutcome::unresolved(UnresolvedReason::AbsentFromPackageSet),
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::scan_fs::package_db::nix::lockfile::parse_flake_lock_str;

    const EXACT_LOCK: &str = r#"{
      "nodes": {
        "nixpkgs": {
          "locked": { "type": "github", "owner": "NixOS", "repo": "nixpkgs",
                      "rev": "a799d3e3886da994fa307f817a6bc705ae538eeb" },
          "original": { "type": "github", "owner": "NixOS", "repo": "nixpkgs",
                        "rev": "a799d3e3886da994fa307f817a6bc705ae538eeb" }
        },
        "root": { "inputs": { "nixpkgs": "nixpkgs" } }
      },
      "root": "root", "version": 7
    }"#;

    const MOVING_LOCK: &str = r#"{
      "nodes": {
        "nixpkgs": {
          "locked": { "type": "github", "owner": "NixOS", "repo": "nixpkgs",
                      "rev": "a799d3e3886da994fa307f817a6bc705ae538eeb" },
          "original": { "type": "github", "owner": "NixOS", "repo": "nixpkgs",
                        "ref": "nixos-unstable" }
        },
        "root": { "inputs": { "nixpkgs": "nixpkgs" } }
      },
      "root": "root", "version": 7
    }"#;

    fn pkgs(pairs: &[(&str, &str)]) -> package_set::PackageSet {
        let body: String = pairs
            .iter()
            .map(|(n, v)| {
                format!(r#"mkDerivation {{ pname = "{n}"; version = "{v}"; }}"#)
            })
            .collect::<Vec<_>>()
            .join("\n");
        package_set::parse(&body)
    }

    #[test]
    fn m926_recognises_the_root_input_named_nixpkgs() {
        let doc = parse_flake_lock_str(EXACT_LOCK).unwrap();
        let (_, how) = find_nixpkgs_input(&doc).unwrap();
        assert_eq!(how, NixpkgsMatch::RootInputName);
    }

    /// FR-016: a fork or internal mirror keeps the repository name and
    /// changes the owner or host.
    #[test]
    fn m926_recognises_a_fork_by_repository_name() {
        let forked = EXACT_LOCK.replace("\"nixpkgs\": {\n          \"locked\"", "\"upstream\": {\n          \"locked\"")
            .replace("\"inputs\": { \"nixpkgs\": \"nixpkgs\" }", "\"inputs\": { \"upstream\": \"upstream\" }")
            .replace("\"owner\": \"NixOS\"", "\"owner\": \"some-org\"");
        let doc = parse_flake_lock_str(&forked).unwrap();
        let (locked, how) = find_nixpkgs_input(&doc).unwrap();
        assert_eq!(how, NixpkgsMatch::RepositoryName);
        assert_eq!(locked.owner.as_deref(), Some("some-org"));
    }

    #[test]
    fn m926_exact_pin_resolves() {
        let doc = parse_flake_lock_str(EXACT_LOCK).unwrap();
        let p = pinned_nixpkgs(&doc).unwrap();
        assert_eq!(p.revision, "a799d3e3886da994fa307f817a6bc705ae538eeb");
        assert!(p.location.is_some());
    }

    /// FR-012: a lock that resolved a branch is reproducible only until the
    /// next re-lock, so it is not treated as pinned.
    #[test]
    fn m926_a_moving_reference_is_not_resolvable() {
        let doc = parse_flake_lock_str(MOVING_LOCK).unwrap();
        assert!(matches!(
            pinned_nixpkgs(&doc),
            Err(UnresolvedReason::NoExactRevision)
        ));
    }

    #[test]
    fn m926_a_lock_without_nixpkgs_is_not_resolvable() {
        let none = r#"{"nodes":{"root":{"inputs":{}}},"root":"root","version":7}"#;
        let doc = parse_flake_lock_str(none).unwrap();
        assert!(pinned_nixpkgs(&doc).is_err());
    }

    /// The ordering that matters: a boot library present in the package set
    /// must NOT resolve, or the document carries a version the build never
    /// uses (FR-014c, Principle IX).
    #[test]
    fn m926_boot_status_is_checked_before_package_set_membership() {
        let ps = pkgs(&[("base", "4.22.0.0"), ("aeson", "2.2.4.1")]);
        let boot: BTreeSet<String> = ["base".to_string()].into_iter().collect();

        assert_eq!(
            classify("base", &ps, &boot, "rev"),
            ResolutionOutcome::unresolved(UnresolvedReason::CompilerSupplied),
            "base is in the package set with a version, and must still not resolve"
        );
        match classify("aeson", &ps, &boot, "rev") {
            ResolutionOutcome::Resolved { version, .. } => assert_eq!(version, "2.2.4.1"),
            other => panic!("expected aeson to resolve, got {other:?}"),
        }
    }

    #[test]
    fn m926_a_name_in_neither_is_absent_from_the_package_set() {
        let ps = pkgs(&[("aeson", "2.2.4.1")]);
        assert_eq!(
            classify("nowhere", &ps, &BTreeSet::new(), "rev"),
            ResolutionOutcome::unresolved(UnresolvedReason::AbsentFromPackageSet)
        );
    }

    /// Reason strings are a consumer-visible contract, so they are asserted
    /// rather than left to whatever `Debug` happens to print.
    #[test]
    fn m926_reason_strings_are_stable() {
        use UnresolvedReason::*;
        for (r, s) in [
            (CompilerSupplied, "compiler-supplied"),
            (AbsentFromPackageSet, "absent-from-package-set"),
            (SourceUnreachable, "source-unreachable"),
            (SourceUnsupported, "source-unsupported"),
            (NoExactRevision, "no-exact-revision"),
            (Offline, "offline"),
        ] {
            assert_eq!(r.as_str(), s);
        }
    }
}
