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
pub(crate) mod closure;
pub(crate) mod closure_identity;
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

/// Issue #1033 — the shared override file. `hackage-packages.nix` is
/// generated from Hackage; this is where nixpkgs rebinds some of those names
/// to a pinned alternative, and reading only the generated file emits the
/// superseded version.
pub(crate) const CONFIGURATION_COMMON_PATH: &str =
    "pkgs/development/haskell-modules/configuration-common.nix";

/// Cache key for that file.
pub(crate) const CONFIGURATION_COMMON_KEY: &str = "configuration-common.nix";

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
    /// Whether re-locking would move this revision: `exact`,
    /// `branch-or-tag`, or `default-branch`. Provenance, not a gate — a
    /// consumer deciding how much to trust a resolved version wants it, but
    /// it is not a reason to withhold one.
    pub(crate) pin_state: &'static str,
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
        // A 404 at the package-set path means the source was reached but
        // carries no Haskell package set. That is not "absent from the
        // package set" — there is no package set — so it degrades the same
        // way an unreachable source does.
        //
        // There is no arm for an unsupported source shape: that is detected
        // before any retrieval, when `Location::from_locked` declines.
        match e {
            FetchError::NotFound
            | FetchError::Unauthorized
            | FetchError::Unreachable(_)
            | FetchError::TimedOut => UnresolvedReason::SourceUnreachable,
            // #975: nothing was attempted, so the source is not implicated.
            FetchError::OfflineCacheMiss => UnresolvedReason::Offline,
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
        /// Milestone 1273 (FR-010) — the license this entry declares.
        ///
        /// Carried on the outcome rather than looked up again at the call
        /// site, for the same reason `source_hash` is: `classify` resolves
        /// aliases, so the entry it matched may live under a different
        /// attribute name than the component's, and a second lookup by the
        /// component's own name would miss it.
        license: Option<package_set::DeclaredLicense>,
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

    // FR-012: the gate is whether the **lock** pins an exact revision, not
    // whether the author wrote one in `flake.nix`.
    //
    // Those are very different populations. Measured against two real
    // Nix-built Haskell repositories, both write
    // `nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable"` and both locks
    // pin an exact 40-hex revision. Requiring the *original* to be exact
    // rejected both and resolved nothing — the rare case (a revision written
    // directly into `flake.nix`) was being treated as the only case.
    //
    // A lock resolving a branch to a concrete revision is what a lockfile is
    // for. Re-locking moves it, but that is true of every lockfile-derived
    // version in every ecosystem: `cargo update` moves `Cargo.lock` too, and
    // waybill does not decline to read it for that reason. The volatility is
    // provenance, recorded below and already annotated by m925 as C167 —
    // not grounds to refuse.
    let Some(revision) = locked.rev.clone().filter(|r| !r.is_empty()) else {
        return Err(UnresolvedReason::NoExactRevision);
    };

    // Whether re-locking would move this revision, for the provenance record.
    let pin_state = doc
        .nodes
        .values()
        .filter(|n| n.locked.as_ref().is_some_and(|l| std::ptr::eq(l, locked)))
        .filter_map(|n| n.original.as_ref())
        .map(|o| match o.pin_state(locked) {
            OriginalPinState::Exact => "exact",
            OriginalPinState::NamedRef(_) => "branch-or-tag",
            OriginalPinState::DefaultBranch => "default-branch",
        })
        .next()
        .unwrap_or("unknown");

    Ok(PinnedNixpkgs {
        revision,
        location: Location::from_locked(locked),
        matched_by,
        pin_state,
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
    aliases: &std::collections::BTreeMap<String, String>,
    revision: &str,
) -> ResolutionOutcome {
    if boot_libraries::is_boot_library(name, boot) {
        return ResolutionOutcome::unresolved(UnresolvedReason::CompilerSupplied);
    }
    // #984: the compiler configuration can bind a bare name to a versioned
    // attribute (`os-string = doDistribute self.os-string_2_0_10;`), in which
    // case the bare name exists nowhere in the package set. Consulted only
    // AFTER a direct hit, so an alias can never override a name the package
    // set defines in its own right.
    let entry = packages.get(name).or_else(|| {
        aliases
            .get(name)
            .and_then(|target| packages.get(target.as_str()))
    });
    match entry {
        Some(entry) => ResolutionOutcome::Resolved {
            version: entry.version.clone(),
            source_hash: entry.source_hash.clone(),
            revision: revision.to_string(),
            license: entry.license.clone(),
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
                        "ref": "nixos-unstable" }
        },
        "root": { "inputs": { "nixpkgs": "nixpkgs" } }
      },
      "root": "root", "version": 7
    }"#;

    const MOVING_LOCK: &str = r#"{
      "nodes": {
        "nixpkgs": {
          "locked": { "type": "github", "owner": "NixOS", "repo": "nixpkgs" },
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
            // Real shape: a top-level attribute binding, which is what the
            // parser keys on (#970). A bare `mkDerivation` block with no
            // attribute head is not something nixpkgs ever emits, and a
            // fixture in that shape is why #970 went unnoticed.
            .map(|(n, v)| {
                format!(
                    "  {n} = callPackage ({{ mkDerivation }}: mkDerivation {{\n\
                           pname = \"{n}\"; version = \"{v}\";\n\
                     }}) {{ }};\n"
                )
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

    /// FR-012: the gate is the LOCK's revision, not the author's reference.
    /// A lock carrying no `rev` at all has nothing reproducible to resolve
    /// against. A lock that resolved a branch to a revision DOES — that is
    /// what a lockfile is for, and both real repositories measured have
    /// exactly that shape.
    #[test]
    fn m926_a_lock_without_a_revision_is_not_resolvable() {
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
            classify("base", &ps, &boot, &Default::default(), "rev"),
            ResolutionOutcome::unresolved(UnresolvedReason::CompilerSupplied),
            "base is in the package set with a version, and must still not resolve"
        );
        match classify("aeson", &ps, &boot, &Default::default(), "rev") {
            ResolutionOutcome::Resolved { version, .. } => assert_eq!(version, "2.2.4.1"),
            other => panic!("expected aeson to resolve, got {other:?}"),
        }
    }

    #[test]
    fn m926_a_name_in_neither_is_absent_from_the_package_set() {
        let ps = pkgs(&[("aeson", "2.2.4.1")]);
        assert_eq!(
            classify("nowhere", &ps, &BTreeSet::new(), &Default::default(), "rev"),
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

// ---------------------------------------------------------------------------
// Orchestration (T016): retrieve, classify, and enrich in place.
// ---------------------------------------------------------------------------

use std::path::Path;

use waybill_common::resolution::ResolvedComponent;
use waybill_common::types::hash::ContentHash;

/// Annotation keys. Registered as catalog rows by T031.
pub(crate) const ANN_RESOLVED_VIA: &str = "waybill:nixpkgs-resolved-via";
/// Milestone 985 (#962) — declared vs transitively reached, per component.
///
/// Recorded explicitly rather than inferred from graph position: CycloneDX's
/// primary-dependency fallback (milestone 894) synthesizes a root edge to
/// every unreferenced component when the root has no declared edges, under
/// which every closure member would read as declared (FR-006).
pub(crate) const ANN_COMPONENT_ORIGIN: &str = "waybill:nixpkgs-component-origin";
pub(crate) const ANN_UNRESOLVED_REASON: &str = "waybill:haskell-version-unresolved-reason";
pub(crate) const ANN_CANDIDATE_COMPILERS: &str = "waybill:nixpkgs-candidate-compilers";
pub(crate) const ANN_VERSION_DISAGREEMENT: &str = "waybill:nixpkgs-version-disagreement";

/// Options the operator controls.
#[derive(Debug, Clone)]
pub(crate) struct ResolveOptions {
    /// `--offline` (FR-009).
    pub(crate) offline: bool,
    /// The feature's own opt-out (FR-015a).
    pub(crate) disabled: bool,
    /// Milestone 985 (#962) — `--no-nixpkgs-haskell-closure`.
    ///
    /// Narrower than `disabled`: suppresses the transitive closure while
    /// leaving milestone 926's declared-dependency resolution active, because
    /// an operator who wants versions on declared dependencies but not a 3.8×
    /// document should not have to give up both (FR-016).
    pub(crate) closure_disabled: bool,
    /// Milestone 1034 (#971 part A) — `--nix-eval`.
    ///
    /// `None` is the default and means no `nix` process is started at all
    /// (spec FR-002), which is what makes the flag-off path byte-identical.
    pub(crate) nix_eval: Option<super::eval::TierConfig>,
}

// The FR-019 budget is not carried here: it belongs to the retrieval
// boundary, and `HttpSource` already owns it. Duplicating it would create two
// places to change and one of them would drift.

/// Outcome of one enrichment pass, for the document-scope record (T044).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct EnrichmentSummary {
    /// Dependencies that gained a version.
    pub(crate) resolved: usize,
    /// Dependencies left versionless, by reason.
    pub(crate) unresolved: std::collections::BTreeMap<String, usize>,
    /// Revision resolved against, when there was one.
    pub(crate) revision: Option<String>,
    /// Set when the whole pass degraded (FR-008, contract C7).
    pub(crate) degraded_reason: Option<String>,
    /// Components whose locally-established version differs from the pinned
    /// revision's (FR-013). The local value is kept; the difference is
    /// recorded.
    pub(crate) disagreements: usize,
    /// Milestone 985 (#962) — what the transitive runtime closure did, and
    /// the edges it produced. `None` when the closure did not run.
    pub(crate) closure: Option<closure::ClosureSummary>,
    /// Edges the closure produced, endpoints as ATTRIBUTE names. The caller
    /// converts them to relationships and MUST apply `renames` to them in the
    /// same step identities are rewritten (#980).
    pub(crate) closure_edges: Vec<closure::ClosureEdge>,
    /// Milestone 1273 (FR-007, FR-008) — what the optional-mark
    /// reconciliation did.
    ///
    /// `None` means it **did not run**. `Some` with an empty `withdrawn`
    /// means it ran and withdrew nothing. Those were the same output before
    /// this milestone, which is the condition under which a feature that
    /// *cannot* fire looks like a feature with nothing to do.
    pub(crate) optional_reconciliation: Option<OptionalReconciliation>,
    /// PURL rewrites this pass performed, as `(before, after)` (#980).
    ///
    /// Assigning a version changes the component's PURL, and the PURL is the
    /// identity the dependency graph keys on — `Relationship::from` / `::to`
    /// are PURL strings. The edges were built before this pass ran, so every
    /// rewrite silently orphans its component unless the endpoints are
    /// rewritten too. The caller MUST apply these; `apply_renames` does it.
    pub(crate) renames: Vec<(String, String)>,
    /// Milestone 1034 (#971 part A) — what the `nix eval` tier did, when the
    /// operator opted in. `None` when `--nix-eval` was absent, which is what
    /// keeps the default path byte-identical (spec FR-002).
    pub(crate) nix_eval: Option<NixEvalSummary>,
    /// Milestone 1276 — the packages the pinned revision's compiler supplies,
    /// handed on to the lifecycle-scope propagation.
    ///
    /// This pass computes the set to classify with and then drops it. The
    /// propagation needs it to treat a compiler-supplied package as immune
    /// (#1340): such a package is installed in every build, so the CycloneDX
    /// definitions of `optional` and `excluded` are both false of it, and a
    /// non-runtime scope propagated to it is a conformance defect.
    ///
    /// Carried here rather than re-derived because the propagation runs for
    /// scans that never enable `--nix-closure` — the two affected corpus
    /// targets are both offline scans — so milestone 1275's boot-set load,
    /// which lives inside the closure arm, is unreachable from there.
    ///
    /// `None` when this pass did not run or degraded, which exempts nothing
    /// (FR-005). Unearned immunity is the dangerous default.
    pub(crate) boot_libraries: Option<BTreeSet<String>>,
    /// Milestone 1275 — the parsed package set this pass loaded, handed on to
    /// the closure-identity pass so the scan parses it once.
    ///
    /// Measured: re-parsing costs 1,121 ms for a 16 MB file the process
    /// already read (T005, findings F2). Deliberately NOT rendered into
    /// `to_document_value` — it is 19,437 entries of input, not a record of
    /// what the pass did.
    pub(crate) package_set: Option<package_set::PackageSet>,
}

/// What the evaluation tier did for one scan.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct NixEvalSummary {
    /// Platform evaluated for; `None` when the tier degraded before
    /// establishing one.
    pub(crate) system: Option<String>,
    /// Attribute names evaluation returned a result for.
    pub(crate) evaluated: usize,
    /// Components whose evaluated version superseded the file-parsed one.
    pub(crate) superseded: usize,
    /// Why the tier contributed nothing, when it did not. Wire form of
    /// `DegradationReason`, a closed set.
    pub(crate) degraded_reason: Option<String>,
}

/// Rewrite dependency-edge endpoints after a pass changed component PURLs
/// (#980, invariant I2).
///
/// I2 — "every edge endpoint must resolve to a component present in the
/// document" — is named in `generate::graph_completeness` (m860, FR-001,
/// C-3.3). Before #980 it was only ever asserted against a hand-built
/// fixture, never against a real emission, which is how a pass that rewrote
/// identities without touching edges shipped: CycloneDX kept edges pointing
/// at the old PURL, and SPDX dropped the relationships entirely — 66% of the
/// graph on a real project, with neither format raising an error.
pub(crate) fn apply_renames(
    renames: &[(String, String)],
    relationships: &mut [waybill_common::resolution::Relationship],
) -> usize {
    if renames.is_empty() {
        return 0;
    }
    let map: std::collections::HashMap<&str, &str> = renames
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    let mut rewritten = 0usize;
    for r in relationships.iter_mut() {
        if let Some(new) = map.get(r.from.as_str()) {
            r.from = (*new).to_string();
            rewritten += 1;
        }
        if let Some(new) = map.get(r.to.as_str()) {
            r.to = (*new).to_string();
            rewritten += 1;
        }
    }
    rewritten
}

impl EnrichmentSummary {
    /// Render this pass as the C174 document-scope value (#973).
    ///
    /// The summary has always carried every field a consumer wants, but
    /// only `degraded_reason` ever reached the document: a scan that
    /// resolved 97 components said so in a log line and nothing else, so a
    /// regression that disabled the pass outright left the document's
    /// property set byte-identical to a healthy one. This renders the whole
    /// record, so the artifact that outlives the run carries it.
    ///
    /// `degraded_reason` is deliberately *not* folded in: C173 already
    /// carries it and consumers key on it. This is additive.
    ///
    /// Keys are emitted through `serde_json::Map`, which is a `BTreeMap`
    /// here (no `preserve_order` feature), so field order is sorted and
    /// stable across runs — a golden can pin the string.
    pub(crate) fn to_document_value(&self) -> serde_json::Value {
        let mut v = serde_json::json!({
            "resolved": self.resolved,
            "unresolved": self.unresolved,
            "disagreements": self.disagreements,
            "revision": self.revision,
        });
        // Milestone 1273 (FR-007, FR-008, FR-018) — the optional-mark
        // reconciliation, in the DOCUMENT and not only the log. Additive to
        // C173 rather than a new catalog row, which is what FR-019 asks for:
        // this annotation already means "what the nixpkgs pass concluded".
        //
        // The key is ABSENT when the reconciliation did not run, which keeps
        // a scan that never reached it byte-identical and makes FR-008's
        // three states readable from the artifact: no C173 at all (the pass
        // did not run), C173 without this key (the closure is disabled or
        // the pass degraded), C173 with it (it ran, and says what it did).
        if let Some(r) = self.optional_reconciliation.as_ref() {
            if let Some(map) = v.as_object_mut() {
                map.insert(
                    "optional-reconciliation".to_string(),
                    r.to_document_value(),
                );
            }
        }
        v
    }

    fn note_unresolved(&mut self, reason: &UnresolvedReason) {
        *self
            .unresolved
            .entry(reason.as_str().to_string())
            .or_insert(0) += 1;
    }
}

/// Milestone 1273 (FR-007, FR-008, FR-009) — the account of one
/// optional-mark reconciliation, for the document.
///
/// `BTreeMap` rather than `HashMap`: this reaches the emitted document and a
/// golden has to be able to pin the string, so iteration order is part of the
/// contract. Same reasoning as `ClosureSummary`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct OptionalReconciliation {
    /// Marks under review — components carrying
    /// `cabal-flag-disabled-stanza`.
    pub(crate) reviewed: usize,
    /// FR-007 — each withdrawal, mapped to the non-seed parent whose closure
    /// edge justified it, so an operator can tell which path was taken.
    pub(crate) withdrawn: std::collections::BTreeMap<String, String>,
    /// FR-009 — declined, so both the scope and its derivation stand.
    pub(crate) declined: usize,
}

impl OptionalReconciliation {
    /// The document-scope value. Keys sort through `serde_json::Map`, which is
    /// a `BTreeMap` here, so the string is stable across runs.
    pub(crate) fn to_document_value(&self) -> serde_json::Value {
        serde_json::json!({
            "reviewed": self.reviewed,
            "withdrawn": self
                .withdrawn
                .iter()
                .map(|(k, v)| serde_json::json!({
                    "component": k,
                    "justified-by": v,
                }))
                .collect::<Vec<_>>(),
            "declined": self.declined,
        })
    }
}

/// Is this a Haskell component this pass is concerned with at all?
/// Issue #955 + #1281 — withdraw an optional mark only on evidence that is
/// not the mark itself.
///
/// The `.cabal` reader marks a dependency `Optional` when the only stanza
/// declaring it is excluded by `buildable:` under the flag defaults the
/// manifest DECLARES (milestone 955 / PR #1030). nixpkgs' Haskell package set
/// is generated by cabal2nix, which finalises each package under ITS flag
/// assignment — usually those same defaults, but nixpkgs overrides flags per
/// package, and where it does the two disagree. Left alone the document would
/// say both "optional, not in the default build" and, through a closure edge,
/// "runtime dependency", with nothing to tell a consumer that the two
/// sentences describe different build configurations.
///
/// # Why the previous test could only ever confirm the mark
///
/// The withdrawal used to ask whether the marked name appeared anywhere in
/// the closure's edge endpoints:
///
/// ```text
/// in_closure := { e.from } ∪ { e.to }   for every edge e
/// withdraw c ⟸ c.marked ∧ c.name ∈ in_closure
/// ```
///
/// A marked component is a closure **seed** — the walk is seeded from the
/// declared dependency set, which contains it — so the walk expands it and it
/// appears as `e.from` of its own out-edges. Membership was therefore true by
/// construction for every marked component, and `restored=2` on the measured
/// fixture meant only that two marks had been handed in. Nothing in the
/// project reached either one.
///
/// # What replaces it
///
/// A second closure, seeded from `declared \ marked`. A component that walk
/// still reaches really is built; one it does not reach keeps its mark, and so
/// does everything below it.
///
/// Two cheaper-looking fixes were considered and are both wrong:
///
/// - **`e.to` only.** Still unsound: a marked component reached by another
///   marked component is a `to`, so a pair of marks confirms itself.
/// - **`reached_from` with one parent outside the marked set.** Correct for a
///   directly-marked component, wrong one level down. Probe D measured 8
///   non-boot components reachable from the root *only* through the marked
///   pair on the public library #955 was filed against; each has a parent that
///   is not itself marked, so a parent-local test withdraws nothing for the
///   pair and still publishes all 8 as runtime-reachable. The error fans out,
///   so the test has to be global.
///
/// Re-seeding costs one extra breadth-first walk over an already-parsed
/// in-memory package set: no new I/O, no fetch.
pub(crate) fn reconcile_optional_with_closure(
    reachable_without_marks: &std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    components: &mut [ResolvedComponent],
) -> OptionalReconciliation {
    let mut record = OptionalReconciliation::default();
    for c in components.iter_mut() {
        let marked = c
            .extra_annotations
            .get("waybill:optional-derivation")
            .and_then(|v| v.as_str())
            == Some("cabal-flag-disabled-stanza");
        if !marked {
            continue;
        }
        record.reviewed += 1;
        let Some(parents) = reachable_without_marks.get(c.name.as_str()) else {
            // FR-009 — declined. The scope AND its derivation both stand;
            // restoring one without the other would publish a scope with no
            // account of where it came from.
            record.declined += 1;
            continue;
        };
        // FR-007 — name the parent that justified it. A seed of the second
        // walk has no parent, in which case the project declares it directly
        // through a stanza that IS built.
        let justification = parents
            .iter()
            .next()
            .cloned()
            .unwrap_or_else(|| "declared by a buildable stanza".to_string());
        // Back to what it was before the mark: the reader only ever overrides
        // a Runtime scope, so Runtime is what it displaced.
        c.lifecycle_scope = Some(waybill_common::resolution::LifecycleScope::Runtime);
        c.extra_annotations.remove("waybill:optional-derivation");
        record.withdrawn.insert(c.name.clone(), justification);
    }
    record
}

/// Is this the project being scanned, rather than one of its dependencies?
///
/// Mirrors the detection in `nix/mod.rs`, which keys on `source_type` ending
/// in `main-module`.
fn is_main_module(c: &ResolvedComponent) -> bool {
    c.source_type
        .as_deref()
        .is_some_and(|t| t.ends_with("main-module"))
}

fn is_haskell(c: &ResolvedComponent) -> bool {
    c.purl.as_str().starts_with("pkg:hackage/")
}

/// Is this component one this pass may *assign* a version to?
///
/// Deliberately narrow. Enrichment never introduces a component
/// (Principle XII constraint 1) and never overwrites a version that came
/// from a project-local freeze file, which outranks a nixpkgs-resolved one
/// (FR-013, contract C6).
fn is_enrichable(c: &ResolvedComponent) -> bool {
    is_haskell(c) && c.version.is_empty()
}

/// Enrich Haskell components with versions from the pinned nixpkgs.
///
/// Returns `None` when the gate (contract C1) did not open, which is the
/// common case: a repository with no flake, or no Haskell dependencies, does
/// no work and produces no annotation (SC-009).
pub(crate) fn enrich(
    scan_root: &Path,
    components: &mut Vec<ResolvedComponent>,
    opts: ResolveOptions,
    source: &dyn fetch::RevisionSource,
) -> Option<EnrichmentSummary> {
    if opts.disabled {
        return None;
    }
    // Gate half 2 first: it is free, and a repository with no Haskell
    // dependencies must not even read the lockfile (SC-009).
    if !components.iter().any(is_enrichable) {
        return None;
    }
    // The acquisition sequence lives in `load_package_set` so the milestone
    // 1275 closure-identity pass can reach the same package set without
    // inheriting the two gates above. Those gates are correct for THIS pass
    // and wrong for that one: `is_enrichable` accepts only versionless
    // components, so a repository whose only Haskell content arrives through
    // the derivation closure returns `None` here -- and that is exactly the
    // scan #1282 is about.
    let (pinned, location, mut packages) = match load_package_set(
        scan_root,
        opts.offline,
        source,
    ) {
        PackageSetLoad::NoLockfile => return None,
        PackageSetLoad::Degraded(reason) => return Some(degrade_all(components, reason)),
        PackageSetLoad::Loaded(l) => (l.pinned, l.location, l.packages),
    };
    let mut nix_eval_divergences = Vec::new();

    // Milestone 1034 (#971 part A) — the opt-in evaluation tier.
    //
    // Last in the ladder because it is the most authoritative: parsing the
    // generated set, then the override file (#1033), then asking Nix. Every
    // failure here degrades to whatever the earlier rungs produced, so a
    // missing or unusable `nix` costs nothing that was already established.
    let mut nix_eval_summary: Option<NixEvalSummary> = None;
    let mut nix_eval_asked: std::collections::HashSet<String> = Default::default();
    // Captured where the tier finishes rather than read off `nix_eval_summary`,
    // which is moved into the summary struct further down.
    let mut nix_eval_ran_early = false;
    if opts.nix_eval.is_some() && opts.offline {
        // `--offline` promises no outbound network calls. Evaluation resolves
        // the pinned revision through `getFlake`, which fetches when the nix
        // store lacks it, so the tier cannot honour that promise -- and nix's
        // own `--offline` does not stop it.
        let reason = super::eval::reason::DegradationReason::OfflineRequested;
        nix_eval_summary = Some(NixEvalSummary {
            degraded_reason: Some(reason.wire().to_string()),
            ..Default::default()
        });
        tracing::info!(
            reason = reason.wire(),
            "nix-eval: skipped, --offline forbids the network this tier may need"
        );
    } else if let Some(cfg) = &opts.nix_eval {
        // Every Haskell component, not just the ones awaiting a version.
        // `is_enrichable` excludes components that already have one, but a
        // version that is present and *wrong* is precisely the #1033 case this
        // tier exists to correct — filtering those out would leave the tier
        // unable to supersede anything, which is the whole point of it.
        //
        // The scanned project itself is excluded. Asking nixpkgs about it is
        // not just useless but actively unsafe: a project shares its name with
        // whatever nixpkgs publishes under that name, and those are different
        // artifacts. Measured on one real project — the local source is
        // 2.2.2.0 and nixpkgs carries 2.2.0.0 under the same name. Applying
        // that answer would make the document claim the scanned tree is a
        // version it is not. The m926 local-version guard happens to prevent
        // it today; this removes the question instead of relying on that.
        let names: Vec<String> = components
            .iter()
            .filter(|c| is_haskell(c) && !is_main_module(c))
            .map(|c| c.name.clone())
            .collect();
        nix_eval_asked = names.iter().cloned().collect();
        let evaluator = super::eval::invoke::NixEvaluator;
        match super::eval::run(&pinned.revision, &names, cfg, &evaluator) {
            Ok(outcome) => {
                nix_eval_ran_early = true;
                nix_eval_divergences = packages.apply_evaluated_versions(&outcome.versions);
                nix_eval_summary = Some(NixEvalSummary {
                    system: Some(outcome.system.as_str().to_string()),
                    evaluated: outcome.versions.len(),
                    superseded: nix_eval_divergences.len(),
                    degraded_reason: None,
                });
                tracing::info!(
                    // the outcome's own revision, not the caller's: this is
                    // the revision the versions actually describe
                    revision = %outcome.revision,
                    system = outcome.system.as_str(),
                    evaluated = outcome.versions.len(),
                    superseded = nix_eval_divergences.len(),
                    "nix-eval: evaluation superseded file-parsed versions"
                );
            }
            Err(reason) => {
                nix_eval_summary = Some(NixEvalSummary {
                    degraded_reason: Some(reason.wire().to_string()),
                    ..Default::default()
                });
                tracing::info!(
                    revision = %pinned.revision,
                    reason = reason.wire(),
                    detail = %reason,
                    "nix-eval: degrading to file-parsed versions"
                );
            }
        }
    }

    let packages = packages;
    tracing::debug!(
        revision = %pinned.revision,
        packages = packages.len(),
        nix_eval_superseded = nix_eval_divergences.len(),
        "nixpkgs-haskell: package set parsed"
    );
    if packages.is_empty() {
        // The tier ran before this rung; a later failure must not
        // erase what it established, or the document goes silent
        // about an evaluation that actually happened.
        let mut s = degrade_all(components, UnresolvedReason::SourceUnreachable);
        s.nix_eval = nix_eval_summary;
        if nix_eval_ran_early {
            annotate_nix_eval_provenance(components, &nix_eval_asked, &nix_eval_divergences);
        }
        return Some(s);
    }

    // Boot libraries, unioned across every candidate compiler (FR-014a).
    // #975: a partial cache must not half-resolve. An offline miss on any
    // compiler configuration degrades the whole pass rather than proceeding
    // with an empty boot set.
    let BootSet { boot, aliases, candidates } =
        match boot_set(source, &location, &pinned.revision, scan_root, opts.offline) {
            Ok(v) => v,
            Err(e) => {
                let reason = UnresolvedReason::from(&e);
                tracing::info!(
                    revision = %pinned.revision,
                    reason = reason.as_str(),
                    "nixpkgs-haskell: degrading, compiler configuration unavailable"
                );
                // The tier ran before this rung; a later failure must not
                // erase what it established, or the document goes silent
                // about an evaluation that actually happened.
                let mut s = degrade_all(components, reason);
                s.nix_eval = nix_eval_summary;
                if nix_eval_ran_early {
                    annotate_nix_eval_provenance(components, &nix_eval_asked, &nix_eval_divergences);
                }
                return Some(s);
            }
        };

    let nix_eval_ran = nix_eval_ran_early;
    let mut summary = EnrichmentSummary {
        revision: Some(pinned.revision.clone()),
        nix_eval: nix_eval_summary,
        ..Default::default()
    };
    let candidate_note = (candidates.len() > 1).then(|| {
        serde_json::Value::String(candidates.join(","))
    });

    // FR-013 / C6: a version already established locally wins, but a
    // disagreement with the pinned revision is recorded rather than silently
    // resolved. Without this the local value simply shadows nixpkgs and a
    // consumer never learns the two sources differ — which is exactly the
    // question someone comparing a Nix build to a cabal build is asking.
    for c in components.iter_mut().filter(|c| is_haskell(c) && !c.version.is_empty()) {
        if boot_libraries::is_boot_library(&c.name, &boot) {
            continue;
        }
        if let Some(entry) = packages.get(&c.name) {
            if entry.version != c.version {
                summary.disagreements += 1;
                c.extra_annotations.insert(
                    ANN_VERSION_DISAGREEMENT.to_string(),
                    serde_json::json!({
                        "emitted": c.version,
                        "nixpkgs": entry.version,
                        "revision": pinned.revision,
                        "note": "the emitted version came from a project-local \
                                 lockfile or freeze file and takes precedence",
                    }),
                );
            }
        }
    }

    for c in components.iter_mut().filter(|c| is_enrichable(c)) {
        match classify(&c.name, &packages, &boot, &aliases, &pinned.revision) {
            ResolutionOutcome::Resolved {
                version,
                source_hash,
                revision,
                license,
            } => {
                apply_resolved(
                    c,
                    ResolvedFacts {
                        version: &version,
                        source_hash: source_hash.as_deref(),
                        revision: &revision,
                        matched_by: pinned.matched_by,
                        pin_state: pinned.pin_state,
                        license: license.as_ref(),
                    },
                    &mut summary.renames,
                );
                summary.resolved += 1;
            }
            ResolutionOutcome::Unresolved { reason } => {
                summary.note_unresolved(&reason);
                c.extra_annotations.insert(
                    ANN_UNRESOLVED_REASON.to_string(),
                    serde_json::Value::String(reason.as_str().to_string()),
                );
                // FR-014b: only meaningful when the compiler was ambiguous.
                if reason == UnresolvedReason::CompilerSupplied {
                    if let Some(v) = candidate_note.clone() {
                        c.extra_annotations
                            .insert(ANN_CANDIDATE_COMPILERS.to_string(), v);
                    }
                    // Milestone 1273 (#1279, FR-014) — correct the reader's
                    // C151, which an operator filters on.
                    //
                    // The reader stamped "no cabal.project.freeze fallback",
                    // which was its best answer with no version in hand. For
                    // a package the COMPILER supplies it is the wrong answer:
                    // a freeze file would not pin it either. `cabal
                    // v2-freeze` declines to pin exactly this set (#938) —
                    // two independent tools declining for the same reason —
                    // so an operator told to go and produce one is being
                    // sent after something that does not exist.
                    //
                    // FR-014 asks that the reason distinguish
                    // compiler-supplied from undeterminable. This is where
                    // the pass learns which of the two it is, so this is
                    // where C151 is corrected rather than left stale.
                    c.extra_annotations.insert(
                        "waybill:unresolved-reason".to_string(),
                        serde_json::Value::String(
                            "supplied by the compiler; nixpkgs has no version \
                             to give and a freeze file would not pin it either"
                                .to_string(),
                        ),
                    );
                }
            }
        }
    }
    // ------------------------------------------------------------------
    // Milestone 985 (#962) — the transitive runtime closure.
    //
    // Runs AFTER declared classification so the declared set is known: a
    // package reachable both ways must be recorded as declared, and that
    // decision needs the declared set complete (FR-007).
    // ------------------------------------------------------------------
    if !opts.closure_disabled {
        // `is_haskell`, NOT `is_enrichable`. The declared loop above has
        // already assigned versions, and `is_enrichable` means "Haskell AND
        // still versionless" — so filtering on it here would seed the walk
        // with only the dependencies that FAILED to resolve. Measured while
        // building this: the closure reported declared=2 on a fixture
        // declaring four, and walked zero relations.
        let declared: Vec<String> = components
            .iter()
            .filter(|c| is_haskell(c))
            .map(|c| c.name.clone())
            .collect();

        let relations_of = |n: &str| packages.get(n).map(|e| e.runtime_relations.clone());
        let is_boot = |n: &str| boot_libraries::is_boot_library(n, &boot);
        let lookup = closure::RelationLookup { relations_of: &relations_of, is_boot: &is_boot };
        let mut result = closure::walk(&declared, &lookup);

        let existing: std::collections::BTreeSet<String> =
            components.iter().map(|c| c.name.clone()).collect();

        for member in &result.members {
            // Origin is recorded on EVERY member, declared ones included:
            // marking only transitive components makes absence ambiguous,
            // indistinguishable from a component the resolver never examined
            // (FR-006a).
            if !existing.contains(&member.name) {
                let mut c = new_closure_component(&member.name);
                // #984: the closure resolves aliased names too, so a
                // transitively-reached package whose bare name exists only
                // via the compiler configuration gets its version rather
                // than `absent-from-package-set`. This is the call site that
                // closes the one remaining nix-eval disagreement measured in
                // #962 -- `os-string`, reached through
                // `directory-ospath-streaming`.
                match classify(&c.name, &packages, &boot, &aliases, &pinned.revision) {
                    ResolutionOutcome::Resolved {
                        version,
                        source_hash,
                        revision,
                        license,
                    } => {
                        apply_resolved(
                            &mut c,
                            ResolvedFacts {
                                version: &version,
                                source_hash: source_hash.as_deref(),
                                revision: &revision,
                                matched_by: pinned.matched_by,
                                pin_state: pinned.pin_state,
                                license: license.as_ref(),
                            },
                            &mut summary.renames,
                        );
                    }
                    ResolutionOutcome::Unresolved { reason } => {
                        // FR-005a — emitted versionless with a reason, never
                        // dropped. The dependency is known to exist; it was
                        // read from another package's relation list.
                        *result
                            .summary
                            .unresolved
                            .entry(reason.as_str().to_string())
                            .or_insert(0) += 1;
                        c.extra_annotations.insert(
                            ANN_UNRESOLVED_REASON.to_string(),
                            serde_json::Value::String(reason.as_str().to_string()),
                        );
                    }
                }
                components.push(c);
            }
        }

        // Second pass so declared components added before the walk also get
        // their origin.
        let origin_of: std::collections::BTreeMap<&str, closure::ComponentOrigin> = result
            .members
            .iter()
            .map(|m| (m.name.as_str(), m.origin))
            .collect();
        // Also `is_haskell` — every Haskell component the resolver touched
        // carries an origin, resolved ones included (FR-006a).
        for c in components.iter_mut().filter(|c| is_haskell(c)) {
            if let Some(origin) = origin_of.get(c.name.as_str()) {
                c.extra_annotations.insert(
                    ANN_COMPONENT_ORIGIN.to_string(),
                    serde_json::Value::String(origin.as_str().to_string()),
                );
            }
        }

        tracing::info!(
            declared = result.summary.declared,
            transitive = result.summary.transitive,
            relations_walked = result.summary.relations_walked,
            unresolved = ?result.summary.unresolved,
            "nixpkgs-haskell: runtime closure resolved"
        );
        summary.closure_edges = std::mem::take(&mut result.edges);
        summary.closure = Some(result.summary);

        // Milestone 1273 (#1281) — the optional-mark reconciliation.
        //
        // A SECOND walk, seeded from the declared set with the marked names
        // removed. The first walk cannot answer the question: it is seeded
        // from the set containing those names, so it reaches every one of
        // them by construction and can only confirm the marks it was handed.
        //
        // Reuses the same `RelationLookup` over the same already-parsed
        // package set, so this is one extra breadth-first traversal and no
        // new I/O.
        let marked: std::collections::BTreeSet<String> = components
            .iter()
            .filter(|c| {
                c.extra_annotations
                    .get("waybill:optional-derivation")
                    .and_then(|v| v.as_str())
                    == Some("cabal-flag-disabled-stanza")
            })
            .map(|c| c.name.clone())
            .collect();
        let reconciliation = if marked.is_empty() {
            // Ran, with nothing to review. Distinct from "did not run"
            // (FR-008), which is `None`.
            OptionalReconciliation::default()
        } else {
            let unmarked_seed: Vec<String> = declared
                .iter()
                .filter(|n| !marked.contains(*n))
                .cloned()
                .collect();
            let without = closure::walk(&unmarked_seed, &lookup);
            let reachable: std::collections::BTreeMap<
                String,
                std::collections::BTreeSet<String>,
            > = without
                .members
                .into_iter()
                .map(|m| (m.name, m.reached_from))
                .collect();
            reconcile_optional_with_closure(&reachable, components)
        };
        // DEBUG, not INFO. `scan_cmd` owns the operator-facing report and
        // distinguishes all three FR-008 states there; a second INFO line
        // here said roughly the same thing twice.
        //
        // Mutation M31 is what surfaced it: demoting this line changed
        // nothing any test could observe, because the report that matters
        // still fired. A log nobody would miss is a log that should not be
        // competing for an operator's attention at INFO.
        tracing::debug!(
            reviewed = reconciliation.reviewed,
            withdrawn = reconciliation.withdrawn.len(),
            declined = reconciliation.declined,
            "nixpkgs-haskell: optional-mark reconciliation computed; a mark is \
             withdrawn only where a closure seeded WITHOUT the marked set \
             still reaches the component (#1281)"
        );
        summary.optional_reconciliation = Some(reconciliation);
    }

    // Phase 2 of the tier: the closure has now added components the first
    // evaluation never saw. Phase 1 corrected the package set before the walk,
    // so those components inherited any correction that applied to an entry it
    // touched -- but nix was never *asked* about a package that appears only
    // transitively, which on real projects is most of them (34 of 53 on one,
    // 146 of 190 on another). Asking now closes that gap; the measured cost of
    // a second evaluation is a fraction of a second.
    if nix_eval_ran {
        if let Some(cfg) = &opts.nix_eval {
            let unasked: Vec<String> = components
                .iter()
                .filter(|c| is_haskell(c) && !is_main_module(c))
                .map(|c| c.name.clone())
                .filter(|n| !nix_eval_asked.contains(n))
                .collect();
            if !unasked.is_empty() {
                let evaluator = super::eval::invoke::NixEvaluator;
                match super::eval::run(&pinned.revision, &unasked, cfg, &evaluator) {
                    Ok(outcome) => {
                        let mut corrected = 0usize;
                        for c in components.iter_mut().filter(|c| is_haskell(c)) {
                            let Some(Some(evaluated)) = outcome.versions.get(&c.name) else {
                                continue;
                            };
                            if !c.version.is_empty() && &c.version != evaluated {
                                nix_eval_divergences.push(
                                    package_set::EvaluatedDivergence {
                                        name: c.name.clone(),
                                        file_parsed: c.version.clone(),
                                        evaluated: evaluated.clone(),
                                    },
                                );
                                correct_version_in_place(
                                    c,
                                    evaluated,
                                    &mut summary.renames,
                                );
                                corrected += 1;
                            }
                        }
                        nix_eval_asked.extend(unasked.iter().cloned());
                        if let Some(n) = summary.nix_eval.as_mut() {
                            n.evaluated += outcome.versions.len();
                            n.superseded += corrected;
                        }
                        tracing::info!(
                            transitive_evaluated = outcome.versions.len(),
                            corrected,
                            "nix-eval: second pass over transitively-reached packages"
                        );
                    }
                    Err(reason) => {
                        // Phase 1's results stand; only the extra coverage is
                        // lost, so this is not a degradation of the pass.
                        tracing::info!(
                            reason = reason.wire(),
                            "nix-eval: second pass unavailable; declared-only coverage"
                        );
                    }
                }
            }
        }
        annotate_nix_eval_provenance(components, &nix_eval_asked, &nix_eval_divergences);
    }

    // Milestone 1276 — hand the compiler-supplied set on to the scope
    // propagation, which cannot re-derive it: it runs for scans with no
    // `--nix-closure`, where nothing else reads the compiler configurations.
    summary.boot_libraries = Some(boot.clone());

    // Milestone 1275 — hand the parsed set on rather than making the
    // closure-identity pass re-read it. Moved, not cloned, and set here
    // because this is the first point at which nothing else needs it.
    // Measured: a second parse of this file costs 1,121 ms (T005).
    summary.package_set = Some(packages);

    Some(summary)
}

/// A component for a package the closure reached that nothing else emitted.
///
/// Mirrors what the design-tier readers produce: no version yet (the caller
/// classifies), design tier, and a PURL without a version so
/// `apply_resolved` rewrites it — and records the rewrite, so the edges
/// pointing at it are rewritten too (#980).
fn new_closure_component(name: &str) -> ResolvedComponent {
    use waybill_common::resolution::{ResolutionEvidence, ResolutionTechnique};
    let purl = waybill_common::types::purl::Purl::new(&format!("pkg:hackage/{name}"))
        .unwrap_or_else(|_| {
            // Unreachable for a Hackage name, but Principle IV forbids
            // `.unwrap()` in production; fall back to a name-only PURL.
            waybill_common::types::purl::Purl::new("pkg:hackage/unknown")
                .expect("static fallback purl is valid")
        });
    ResolvedComponent {
        build_inclusion: None,
        name: name.to_string(),
        version: String::new(),
        purl,
        evidence: ResolutionEvidence {
            technique: ResolutionTechnique::UrlPattern,
            confidence: 0.95,
            source_connection_ids: vec![],
            source_file_paths: vec![],
            deps_dev_match: None,
        },
        licenses: vec![],
        concluded_licenses: vec![],
        hashes: vec![],
        supplier: None,
        cpes: vec![],
        advisories: vec![],
        occurrences: vec![],
        lifecycle_scope: None,
        requirement_ranges: Vec::new(),
        source_type: None,
        sbom_tier: Some("design".to_string()),
        buildinfo_status: None,
        evidence_kind: None,
        binary_class: None,
        binary_stripped: None,
        linkage_kind: None,
        detected_go: None,
        confidence: None,
        binary_packed: None,
        npm_role: None,
        raw_version: None,
        parent_purl: None,
        co_owned_by: None,
        shade_relocation: None,
        external_references: vec![],
        extra_annotations: Default::default(),
        binary_role: None,
    }
}

/// Attach a resolved version, its hash and its provenance.
///
/// The hash goes into `hashes`, which every emitter already maps to its
/// format's **native** checksum field — research R2 verified this value is a
/// flat SHA-256 of the source tarball, so a `waybill:` annotation would
/// violate Principle V.
/// Correct one component's version in place, recording the PURL rewrite.
///
/// Narrower than [`apply_resolved`] on purpose: this runs on a component that
/// has already been resolved and annotated, so it must change the version and
/// the identity and nothing else. Re-running `apply_resolved` would re-stamp
/// provenance annotations that describe the earlier, superseded resolution.
fn correct_version_in_place(
    c: &mut ResolvedComponent,
    version: &str,
    renames: &mut Vec<(String, String)>,
) {
    c.version = version.to_string();
    if let Ok(p) = waybill_common::types::purl::Purl::new(&format!(
        "pkg:hackage/{}@{}",
        c.name, version
    )) {
        let before = c.purl.as_str().to_string();
        let after = p.as_str().to_string();
        if before != after {
            renames.push((before, after));
        }
        c.purl = p;
    }
}

/// Attach C177 / C181 to every Haskell component the document will carry.
///
/// Called from the normal path AND from every degradation path that runs after
/// the tier. The tier's work must not be erased by a later, unrelated failure:
/// a scan that evaluated 21 attributes and then degraded for want of a compiler
/// configuration still knows which components were checked, and saying so is
/// the point of C181.
fn annotate_nix_eval_provenance(
    components: &mut [ResolvedComponent],
    asked: &std::collections::HashSet<String>,
    divergences: &[package_set::EvaluatedDivergence],
) {
    let superseded: std::collections::HashMap<&str, &str> = divergences
        .iter()
        .map(|d| (d.name.as_str(), d.file_parsed.as_str()))
        .collect();
    for c in components.iter_mut().filter(|c| is_haskell(c)) {
        let origin = if asked.contains(&c.name) {
            "evaluated"
        } else {
            "file-parsed"
        };
        c.extra_annotations.insert(
            ANN_NIX_EVAL_ORIGIN.to_string(),
            serde_json::Value::String(origin.to_string()),
        );
        if let Some(file_parsed) = superseded.get(c.name.as_str()) {
            c.extra_annotations.insert(
                ANN_NIX_EVAL_SUPERSEDED.to_string(),
                serde_json::Value::String((*file_parsed).to_string()),
            );
        }
    }
}

/// C181 — whether this component's version came from evaluating Nix or from
/// parsing the package-set files.
///
/// Emitted on **every** component the tier examined, agreements included.
/// Marking only the superseded ones would make absence ambiguous: a consumer
/// could not tell "the tier checked this and agreed" from "the tier never
/// looked at it", and those are different claims. This is the trap C175
/// documents for declared-vs-transitive, applied here.
const ANN_NIX_EVAL_ORIGIN: &str = "waybill:nix-eval-origin";

/// C177 — the file-parsed version an evaluated one superseded.
///
/// Retained rather than dropped: the divergence between the two sources is how
/// issue #1033 was found, and discarding the loser would remove the signal
/// that catches the next one.
const ANN_NIX_EVAL_SUPERSEDED: &str = "waybill:nix-eval-superseded-version";

/// Milestone 1273 (#1280, FR-010/FR-011/FR-012) — attach the license the
/// pinned revision declares.
///
/// Called only from [`apply_resolved`], which is the single place this pass
/// assigns a version. That is what makes FR-012 structural rather than
/// asserted: a component the resolution did not resolve is never reached by
/// this code, so no license can be invented for it.
///
/// A canonical expression goes into `licenses`, the field every emitter
/// already maps to its own format's **native** license slot. Principle V's
/// primary case: no `waybill:` annotation carries the license itself.
///
/// Which slot each format uses is deliberately NOT named here. This is
/// resolution code and must stay format-neutral, an invariant
/// `spdx3_us3_acceptance::scenario_1` enforces by grepping this tree for
/// format-specific identifiers — prose included, which is how naming one in
/// this very comment turned the gate red. The per-format mapping lives in
/// `docs/reference/sbom-format-mapping.md` and in the emitters.
///
/// FR-011's uncanonicalisable declaration becomes `LicenseRef-<sanitized>`
/// through the shared `sanitize_license_operand_to_ref` helper — the standard
/// mechanism for a license that is not on the SPDX list, and the one
/// milestone 185 (rpm) and milestone 202 already emit, byte-identically
/// across formats. The `LicenseRef-` prefix **is** the record that the value
/// is not a canonical identifier, so FR-011's "record that it is
/// uncanonicalised" is satisfied by a standards-native construct rather than
/// a new annotation.
/// Shared with the milestone 1275 closure-identity pass, so that "by the same
/// rules the existing resolution uses" (FR-002, C3) is one implementation
/// rather than two that agree today.
pub(crate) fn attach_declared_license(
    c: &mut ResolvedComponent,
    license: Option<&package_set::DeclaredLicense>,
) {
    use waybill_common::types::license::{
        sanitize_license_operand_to_ref, SpdxExpression,
    };
    // A license already present outranks this one: a project-local
    // declaration (milestone 1010) or an earlier enrichment is closer to the
    // artifact than a distro's package set.
    if !c.licenses.is_empty() {
        return;
    }
    let Some(declared) = license else { return };
    // FR-020 — the signal is logged as well as emitted, so an operator can
    // see what the revision declared and whether it canonicalised without
    // diffing the document.
    tracing::debug!(
        component = %c.name,
        declared = %declared.as_declared(),
        canonical = declared.is_canonical(),
        "nixpkgs-haskell: attaching the license the pinned revision declares"
    );
    match declared {
        package_set::DeclaredLicense::Canonical(e) => c.licenses.push(e.clone()),
        package_set::DeclaredLicense::Uncanonical(raw) => {
            if let Some(id) = sanitize_license_operand_to_ref(raw) {
                if let Ok(e) = SpdxExpression::new(&format!("LicenseRef-{id}")) {
                    c.licenses.push(e);
                }
            }
        }
    }
}

/// Attach the revision's source hash, or no hash at all.
///
/// `ContentHash::sha256` validates both the alphabet and the 64-char width. A
/// value that fails here contributes no hash rather than a malformed one,
/// which is the same posture the base32 decoder takes (Principle IX). It
/// should be unreachable: the decoder already rejected anything that was not
/// 32 bytes.
///
/// Shared with the milestone 1275 closure-identity pass (FR-003, C4), so the
/// "hex or nothing, never malformed" rule has one implementation.
pub(crate) fn attach_source_hash(c: &mut ResolvedComponent, source_hash: Option<&str>) {
    let Some(hex) = source_hash else { return };
    match ContentHash::sha256(hex) {
        Ok(h) => c.hashes.push(h),
        Err(e) => tracing::debug!(
            error = %e,
            package = %c.name,
            "nixpkgs-haskell: refusing a source hash that failed validation"
        ),
    }
}

/// What the pinned revision says about one resolved component.
///
/// A struct rather than eight positional parameters. Milestone 1273 added the
/// license and crossed clippy's argument threshold, which is a fair signal:
/// `version`, `source_hash` and `license` all come from the same package-set
/// entry and `revision`, `matched_by` and `pin_state` all describe the same
/// pin, so passing them separately was already losing that structure.
pub(crate) struct ResolvedFacts<'a> {
    pub(crate) version: &'a str,
    pub(crate) source_hash: Option<&'a str>,
    pub(crate) revision: &'a str,
    pub(crate) matched_by: NixpkgsMatch,
    pub(crate) pin_state: &'a str,
    pub(crate) license: Option<&'a package_set::DeclaredLicense>,
}

fn apply_resolved(
    c: &mut ResolvedComponent,
    facts: ResolvedFacts<'_>,
    renames: &mut Vec<(String, String)>,
) {
    let ResolvedFacts {
        version,
        source_hash,
        revision,
        matched_by,
        pin_state,
        license,
    } = facts;
    c.version = version.to_string();
    attach_declared_license(c, license);
    // Milestone 1273 (#1279, FR-013) — the reader's C151 is now stale.
    //
    // It was correct when stamped: at read time the component had no version
    // and the reason named the artifact that would have supplied one
    // (`no cabal.project.freeze fallback`, `no stack.yaml.lock fallback`).
    // This pass has just supplied it from the pinned revision, so the
    // annotation describes a situation that no longer exists — and a
    // consumer filtering on it to find what waybill could not identify is
    // actively misled rather than merely under-informed.
    //
    // Removed HERE, in the same mutation that assigns the version, rather
    // than swept at emission time. The fact changed here; a sweep would also
    // have to decide what to do about every other reader's C151, which
    // FR-015 puts out of scope.
    c.extra_annotations.remove("waybill:unresolved-reason");
    // The PURL must carry the version too, or the component is versioned in
    // one place and not the other.
    //
    // #980: the PURL is also the dependency graph's identity
    // (`Relationship::from`/`::to` are PURL strings) and the edges were built
    // before this pass runs, so the rewrite is recorded and the endpoints
    // updated by `apply_renames`. Changing identity without changing the
    // edges orphans the component in every format.
    if let Ok(p) = waybill_common::types::purl::Purl::new(&format!(
        "pkg:hackage/{}@{}",
        c.name, version
    )) {
        let before = c.purl.as_str().to_string();
        let after = p.as_str().to_string();
        if before != after {
            renames.push((before, after));
        }
        c.purl = p;
    }
    attach_source_hash(c, source_hash);
    c.sbom_tier = Some("source".to_string());
    c.extra_annotations.insert(
        ANN_RESOLVED_VIA.to_string(),
        serde_json::json!({
            "source": "nixpkgs",
            "revision": revision,
            "matched-by": matched_by.as_str(),
            "pin-state": pin_state,
        }),
    );
}

/// Mark every enrichable component unresolved with one reason.
fn degrade_all(
    components: &mut [ResolvedComponent],
    reason: UnresolvedReason,
) -> EnrichmentSummary {
    let mut summary = EnrichmentSummary {
        degraded_reason: Some(reason.as_str().to_string()),
        ..Default::default()
    };
    for c in components.iter_mut().filter(|c| is_enrichable(c)) {
        summary.note_unresolved(&reason);
        c.extra_annotations.insert(
            ANN_UNRESOLVED_REASON.to_string(),
            serde_json::Value::String(reason.as_str().to_string()),
        );
    }
    summary
}

/// Read a revision's file from cache, retrieving it only on a miss.
/// Retrieve one artifact, preferring the local cache (#975).
///
/// `offline` refuses the *network* call, not the cache read. A cache read is
/// the local filesystem, which is what `--offline` promises to fall back to,
/// and the key is the pinned revision — immutable, so a hit cannot be stale.
/// Before #975 the offline check sat in `enrich`, above this function, so a
/// fully-hydrated cache resolved nothing.
/// What acquiring the pinned package set concluded.
///
/// Three variants because `enrich` has always had three behaviours here, and
/// collapsing them loses the distinction between "there is nothing to resolve
/// against" and "there is, and we could not reach it" (Principle X). An
/// `Option<Result<_, _>>` would carry the same information and say nothing
/// about what the outer `None` meant.
// `Loaded` carries a 19,437-entry package set and the other two carry almost
// nothing, which clippy flags as a large size difference. Boxing it would add
// an allocation to the only variant on the hot path to save stack on two that
// are returned once per scan, so the lint is allowed here with that reasoning
// rather than silenced by a `Box` nobody wanted.
#[allow(clippy::large_enum_variant)]
pub(crate) enum PackageSetLoad {
    /// No `flake.lock`, or one that does not parse. Nothing is pinned, so
    /// there is nothing to resolve against and nothing to report.
    NoLockfile,
    /// A revision was pinned, and the package set could not be obtained.
    Degraded(UnresolvedReason),
    /// The set is in hand.
    Loaded(LoadedPackageSet),
}

/// The pinned revision's Haskell package set, and where it came from.
pub(crate) struct LoadedPackageSet {
    /// The nixpkgs input as `flake.lock` records it.
    pub(crate) pinned: PinnedNixpkgs,
    /// Retrieval location, retained because callers fetch further files from
    /// the same revision (the compiler configurations, for one).
    pub(crate) location: Location,
    /// Parsed, with `configuration-common.nix`'s version rebinds applied.
    pub(crate) packages: package_set::PackageSet,
}

/// Acquire the pinned revision's Haskell package set.
///
/// Extracted from `enrich` (milestone 1275, T004) so that a second pass can
/// read the same set without inheriting `enrich`'s enrichability gate. The
/// sequence is unchanged: lockfile, pinned revision, retrieval location,
/// `hackage-packages.nix` preferring the cache, then
/// `configuration-common.nix`'s rebinds applied best-effort.
///
/// Retrieval goes through `cached_or_fetch`, so an already-warmed per-revision
/// cache costs no network call -- which is what lets a second caller read the
/// set without a second fetch. It does cost a second PARSE; the measured cost
/// is recorded in `specs/1275-closure-hackage-identity/measurements/findings.md`.
pub(crate) fn load_package_set(
    scan_root: &Path,
    offline: bool,
    source: &dyn fetch::RevisionSource,
) -> PackageSetLoad {
    let lock_path = scan_root.join("flake.lock");
    if !lock_path.exists() {
        return PackageSetLoad::NoLockfile;
    }
    let doc = match super::lockfile::parse_flake_lock(&lock_path) {
        Ok(d) => d,
        Err(e) => {
            tracing::debug!(error = ?e, "nixpkgs-haskell: flake.lock unparseable");
            return PackageSetLoad::NoLockfile;
        }
    };
    // A nixpkgs-shaped input pinned to an exact revision.
    let pinned = match pinned_nixpkgs(&doc) {
        Ok(p) => p,
        Err(reason) => return PackageSetLoad::Degraded(reason),
    };
    // #975: the offline check used to sit HERE, above every retrieval, so a
    // scan with the pinned revision already cached resolved nothing. It now
    // lives inside `cached_or_fetch`, which refuses the network call but
    // still reads the cache, and surfaces `OfflineCacheMiss` when the cache
    // genuinely lacks the artifact. A cold offline scan degrades exactly as
    // it did before.
    let Some(location) = pinned.location.clone() else {
        return PackageSetLoad::Degraded(UnresolvedReason::SourceUnsupported);
    };

    // Retrieve the package set, preferring the cache (FR-010, SC-005).
    let text = match cached_or_fetch(
        source,
        &location,
        &pinned.revision,
        HACKAGE_PACKAGES_PATH,
        HACKAGE_PACKAGES_KEY,
        offline,
    ) {
        Ok(t) => t,
        Err(e) => {
            let reason = UnresolvedReason::from(&e);
            tracing::info!(
                revision = %pinned.revision,
                reason = reason.as_str(),
                "nixpkgs-haskell: degrading, package set unavailable"
            );
            return PackageSetLoad::Degraded(reason);
        }
    };
    let mut packages = package_set::parse(&text);
    // Issue #1033 — apply the shared override file's version rebinds.
    //
    // Best-effort by design: this file supplements the generated set, so if it
    // cannot be retrieved the resolution proceeds on the generated versions
    // exactly as it did before. Degrading the whole scan because a supplement
    // is unavailable would trade a small, bounded inaccuracy for none at all.
    match cached_or_fetch(
        source,
        &location,
        &pinned.revision,
        CONFIGURATION_COMMON_PATH,
        CONFIGURATION_COMMON_KEY,
        offline,
    ) {
        Ok(common) => {
            let rebinds = package_set::parse_top_level_version_rebinds(&common);
            let changed = packages.apply_version_rebinds(&rebinds);
            tracing::debug!(
                revision = %pinned.revision,
                rebinds = rebinds.len(),
                changed,
                "nixpkgs-haskell: applied configuration-common.nix version rebinds"
            );
        }
        Err(e) => {
            tracing::info!(
                revision = %pinned.revision,
                error = ?e,
                "nixpkgs-haskell: configuration-common.nix unavailable; \
                 resolving on the generated package set alone (#1033)"
            );
        }
    }
    PackageSetLoad::Loaded(LoadedPackageSet { pinned, location, packages })
}

fn cached_or_fetch(
    source: &dyn fetch::RevisionSource,
    location: &Location,
    rev: &str,
    path: &str,
    key: &str,
    offline: bool,
) -> Result<String, FetchError> {
    if let Some(hit) = cache::read(rev, key) {
        return Ok(hit);
    }
    if offline {
        return Err(FetchError::OfflineCacheMiss);
    }
    let text = source.fetch(location, rev, path)?;
    cache::write(rev, key, &text);
    Ok(text)
}

/// GHC series a flake might name, newest first.
const KNOWN_GHC_SERIES: &[&str] = &[
    "9.16.x", "9.14.x", "9.12.x", "9.10.x", "9.8.x", "9.6.x", "9.4.x", "9.0.x",
];

/// Union the boot libraries of every candidate compiler, and name the
/// candidates.
///
/// Candidates come from an explicit `haskell.packages.ghc<NN>` path in the
/// project's flake when one is found, and from every known series otherwise.
/// A configuration that cannot be retrieved contributes nothing rather than
/// aborting: a missing series is not a reason to stop resolving.
/// Shared with the milestone 1275 closure-identity pass, which needs the
/// same nulled set to tell a GHC boot library from a package the closure
/// merely lacks (FR-012c).
pub(crate) fn boot_set(
    source: &dyn fetch::RevisionSource,
    location: &Location,
    rev: &str,
    scan_root: &Path,
    offline: bool,
) -> Result<BootSet, FetchError> {
    let candidates = candidate_series(scan_root);
    let mut texts: Vec<String> = Vec::new();
    for series in &candidates {
        let path = format!("pkgs/development/haskell-modules/configuration-ghc-{series}.nix");
        let key = format!("configuration-ghc-{series}.nix");
        match cached_or_fetch(source, location, rev, &path, &key, offline) {
            Ok(text) => texts.push(text),
            // #975 — the one error that must NOT be tolerated. Every other
            // failure means the series genuinely is not there, and the union
            // rule makes a missing series safe. An offline cache miss means
            // the opposite: the configuration exists and we simply declined
            // to look. Tolerating it yields an EMPTY boot set, so every boot
            // library takes a version from the package set instead of being
            // classified `compiler-supplied` — a version the build never
            // uses, asserted with a source hash. Principle III: fail closed.
            Err(e @ FetchError::OfflineCacheMiss) => return Err(e),
            Err(_) => {}
        }
    }
    let boot = boot_libraries::union_nulled(texts.iter().map(String::as_str));
    // #984: the same files also rebind bare names onto versioned attributes.
    let aliases = boot_libraries::union_aliases(texts.iter().map(String::as_str));
    Ok(BootSet { boot, aliases, candidates })
}

/// What one pass learns from the candidate compiler configurations.
///
/// Both halves come from the same files and are always read together, so they
/// travel together rather than as a widening tuple.
pub(crate) struct BootSet {
    /// Names the compiler supplies; never resolved from Hackage.
    pub(crate) boot: BTreeSet<String>,
    /// #984 — bare name → the versioned attribute it actually refers to.
    pub(crate) aliases: std::collections::BTreeMap<String, String>,
    /// The GHC series considered, for the document-scope record.
    pub(crate) candidates: Vec<String>,
}

/// Which compiler package sets the project might build against.
///
/// Scans the flake textually for `haskell.packages.ghc<NN>`. `flake.nix` is a
/// program rather than data, and evaluating it would need a host `nix`
/// (Principle I), so an explicit attribute path is the available signal.
/// Finding none falls back to every known series — conservative, and the
/// union rule makes that safe (FR-014a).
fn candidate_series(scan_root: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(scan_root.join("flake.nix")) else {
        return KNOWN_GHC_SERIES.iter().map(|s| s.to_string()).collect();
    };
    let mut found: BTreeSet<String> = BTreeSet::new();
    for series in KNOWN_GHC_SERIES {
        // `9.6.x` in a config filename is `ghc96` in an attribute path.
        let attr: String = format!("ghc{}", series.trim_end_matches(".x").replace('.', ""));
        if text.contains(&attr) {
            found.insert((*series).to_string());
        }
    }
    if found.is_empty() {
        KNOWN_GHC_SERIES.iter().map(|s| s.to_string()).collect()
    } else {
        found.into_iter().collect()
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod enrich_tests {
    use super::*;
    use crate::testing::EnvGuard;

    const LOCK: &str = r#"{
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

    /// Hermetic stand-in for the network. Records every path requested so a
    /// test can assert that nothing was retrieved at all (SC-009).
    struct StubSource {
        packages: String,
        config: String,
        requests: std::cell::RefCell<Vec<String>>,
        fail: Option<FetchError>,
    }

    impl StubSource {
        fn new(packages: &str, config: &str) -> Self {
            Self {
                packages: packages.to_string(),
                config: config.to_string(),
                requests: std::cell::RefCell::new(Vec::new()),
                fail: None,
            }
        }
        fn failing(e: FetchError) -> Self {
            Self {
                packages: String::new(),
                config: String::new(),
                requests: std::cell::RefCell::new(Vec::new()),
                fail: Some(e),
            }
        }
        fn request_count(&self) -> usize {
            self.requests.borrow().len()
        }
    }

    impl fetch::RevisionSource for StubSource {
        fn fetch(&self, _l: &Location, _rev: &str, path: &str) -> Result<String, FetchError> {
            self.requests.borrow_mut().push(path.to_string());
            if let Some(e) = &self.fail {
                return Err(e.clone());
            }
            if path.ends_with("hackage-packages.nix") {
                Ok(self.packages.clone())
            } else {
                Ok(self.config.clone())
            }
        }
    }

    /// A design-tier Haskell dependency: named, versionless, no hash.
    pub(super) fn comp(name: &str) -> ResolvedComponent {
        use waybill_common::resolution::{ResolutionEvidence, ResolutionTechnique};
        let purl =
            waybill_common::types::purl::Purl::new(&format!("pkg:hackage/{name}")).unwrap();
        ResolvedComponent {
            build_inclusion: None,
            name: name.to_string(),
            version: String::new(),
            purl,
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::UrlPattern,
                confidence: 0.95,
                source_connection_ids: vec![],
                source_file_paths: vec![],
                deps_dev_match: None,
            },
            licenses: vec![],
            concluded_licenses: vec![],
            hashes: vec![],
            supplier: None,
            cpes: vec![],
            advisories: vec![],
            occurrences: vec![],
            lifecycle_scope: None,
            requirement_ranges: Vec::new(),
            source_type: None,
            sbom_tier: Some("design".to_string()),
            buildinfo_status: None,
            evidence_kind: None,
            binary_class: None,
            binary_stripped: None,
            linkage_kind: None,
            detected_go: None,
            confidence: None,
            binary_packed: None,
            npm_role: None,
            raw_version: None,
            parent_purl: None,
            co_owned_by: None,
            shade_relocation: None,
            external_references: vec![],
            extra_annotations: Default::default(),
            binary_role: None,
        }
    }

    /// A non-Haskell component, for the "nothing to enrich" gate.
    fn other_comp() -> ResolvedComponent {
        let mut c = comp("x");
        c.purl = waybill_common::types::purl::Purl::new("pkg:cargo/serde@1.0.0").unwrap();
        c.version = "1.0.0".to_string();
        c
    }

    fn fixture_root(lock: Option<&str>, flake: Option<&str>) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        if let Some(l) = lock {
            std::fs::write(d.path().join("flake.lock"), l).unwrap();
        }
        if let Some(f) = flake {
            std::fs::write(d.path().join("flake.nix"), f).unwrap();
        }
        d
    }

    const PACKAGES: &str = r#"
  waybill-fixture-liba = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-liba";
      version = "1.2.3";
      sha256 = "1zym9yia0is8wxfd6d1ldwvvghwxg4ww3y4lrxnyb2nk60ig29ly";
  }) { };

  waybill-fixture-boot = callPackage ({ mkDerivation }: mkDerivation {
      pname = "waybill-fixture-boot";
      version = "9.9.9";
      sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r";
  }) { };
"#;
    const CONFIG: &str = r#"self: super: { waybill-fixture-boot = null; }"#;

    fn opts() -> ResolveOptions {
        ResolveOptions {
            offline: false,
            disabled: false,
            closure_disabled: false,
            // These tests exercise the file-parsing ladder; the eval tier is
            // off, which is also the default and the byte-identical path.
            nix_eval: None,
        }
    }

    fn isolated_cache() -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        std::env::set_var(cache::CACHE_ENV, t.path());
        t
    }

    /// US1: a declared dependency the revision carries gains an exact version
    /// and a native SHA-256, and leaves design tier.
    #[test]
    fn m926_resolves_a_declared_dependency_with_a_native_hash() {
        let _g = EnvGuard::acquire();
        let _c = isolated_cache();
        let root = fixture_root(Some(LOCK), Some("haskell.packages.ghc96"));
        let src = StubSource::new(PACKAGES, CONFIG);
        let mut comps = vec![comp("waybill-fixture-liba")];

        let s = enrich(root.path(), &mut comps, opts(), &src).unwrap();

        assert_eq!(s.resolved, 1);
        assert_eq!(comps[0].version, "1.2.3");
        assert_eq!(comps[0].sbom_tier.as_deref(), Some("source"));
        assert_eq!(comps[0].hashes.len(), 1);
        assert_eq!(
            comps[0].hashes[0].value.as_str(),
            "9e26f12230d38ae56dcf94f8c139799dc3b7376f3434d35ce74847a0a24fd5ff"
        );
        assert!(comps[0].extra_annotations.contains_key(ANN_RESOLVED_VIA));
        std::env::remove_var(cache::CACHE_ENV);
    }

    /// US2 + FR-014c: a boot library is present in the package set WITH a
    /// version, and must still not resolve.
    #[test]
    fn m926_a_boot_library_never_takes_the_package_sets_version() {
        let _g = EnvGuard::acquire();
        let _c = isolated_cache();
        let root = fixture_root(Some(LOCK), Some("haskell.packages.ghc96"));
        let src = StubSource::new(PACKAGES, CONFIG);
        let mut comps = vec![comp("waybill-fixture-boot")];

        let s = enrich(root.path(), &mut comps, opts(), &src).unwrap();

        assert_eq!(s.resolved, 0);
        assert!(comps[0].version.is_empty(), "no version may be invented");
        assert!(comps[0].hashes.is_empty(), "no hash either");
        assert_ne!(comps[0].sbom_tier.as_deref(), Some("source"));
        assert_eq!(
            comps[0].extra_annotations.get(ANN_UNRESOLVED_REASON),
            Some(&serde_json::Value::String("compiler-supplied".into()))
        );
        std::env::remove_var(cache::CACHE_ENV);
    }

    /// SC-009: a repository with no Haskell dependencies retrieves nothing.
    #[test]
    fn m926_no_haskell_dependencies_means_no_retrieval() {
        let _g = EnvGuard::acquire();
        let _c = isolated_cache();
        let root = fixture_root(Some(LOCK), None);
        let src = StubSource::new(PACKAGES, CONFIG);
        let mut comps = vec![other_comp()];

        assert!(enrich(root.path(), &mut comps, opts(), &src).is_none());
        assert_eq!(src.request_count(), 0);
        std::env::remove_var(cache::CACHE_ENV);
    }

    /// FR-012 / SC-007: no flake.lock means no change at all.
    #[test]
    fn m926_no_flake_lock_means_no_change() {
        let _g = EnvGuard::acquire();
        let _c = isolated_cache();
        let root = fixture_root(None, None);
        let src = StubSource::new(PACKAGES, CONFIG);
        let mut comps = vec![comp("waybill-fixture-liba")];

        assert!(enrich(root.path(), &mut comps, opts(), &src).is_none());
        assert_eq!(src.request_count(), 0);
        assert!(comps[0].version.is_empty());
        assert!(comps[0].extra_annotations.is_empty());
        std::env::remove_var(cache::CACHE_ENV);
    }

    /// FR-009: offline degrades without retrieving.
    #[test]
    fn m926_offline_degrades_without_retrieving() {
        let _g = EnvGuard::acquire();
        let _c = isolated_cache();
        let root = fixture_root(Some(LOCK), None);
        let src = StubSource::new(PACKAGES, CONFIG);
        let mut comps = vec![comp("waybill-fixture-liba")];

        let s = enrich(root.path(), &mut comps, ResolveOptions { offline: true, ..opts() }, &src).unwrap();

        assert_eq!(src.request_count(), 0);
        assert_eq!(s.degraded_reason.as_deref(), Some("offline"));
        assert!(comps[0].version.is_empty());
        std::env::remove_var(cache::CACHE_ENV);
    }

    /// FR-015a: the opt-out disables this feature and nothing else.
    #[test]
    fn m926_the_opt_out_flag_disables_the_pass() {
        let _g = EnvGuard::acquire();
        let _c = isolated_cache();
        let root = fixture_root(Some(LOCK), None);
        let src = StubSource::new(PACKAGES, CONFIG);
        let mut comps = vec![comp("waybill-fixture-liba")];

        assert!(enrich(root.path(), &mut comps, ResolveOptions { disabled: true, ..opts() }, &src).is_none());
        assert_eq!(src.request_count(), 0);
        std::env::remove_var(cache::CACHE_ENV);
    }

    /// FR-017 / C7: an unreachable source degrades every dependency with a
    /// reason, and invents nothing.
    #[test]
    fn m926_an_unreachable_source_degrades_with_a_reason() {
        let _g = EnvGuard::acquire();
        let _c = isolated_cache();
        let root = fixture_root(Some(LOCK), None);
        let src = StubSource::failing(FetchError::Unauthorized);
        let mut comps = vec![comp("waybill-fixture-liba")];

        let s = enrich(root.path(), &mut comps, opts(), &src).unwrap();

        assert_eq!(s.degraded_reason.as_deref(), Some("source-unreachable"));
        assert!(comps[0].version.is_empty());
        assert_eq!(
            comps[0].extra_annotations.get(ANN_UNRESOLVED_REASON),
            Some(&serde_json::Value::String("source-unreachable".into()))
        );
        std::env::remove_var(cache::CACHE_ENV);
    }

    /// FR-010 / SC-005: a revision already retrieved is not retrieved again.
    #[test]
    fn m926_a_cached_revision_is_not_retrieved_again() {
        let _g = EnvGuard::acquire();
        let _c = isolated_cache();
        let root = fixture_root(Some(LOCK), Some("haskell.packages.ghc96"));
        let mut comps = vec![comp("waybill-fixture-liba")];

        let first = StubSource::new(PACKAGES, CONFIG);
        enrich(root.path(), &mut comps, opts(), &first).unwrap();
        assert!(first.request_count() > 0, "the first scan must retrieve");

        let mut comps2 = vec![comp("waybill-fixture-liba")];
        let second = StubSource::new(PACKAGES, CONFIG);
        enrich(root.path(), &mut comps2, opts(), &second).unwrap();

        assert_eq!(second.request_count(), 0, "the second scan must not retrieve");
        assert_eq!(comps2[0].version, "1.2.3", "and must still resolve");
        std::env::remove_var(cache::CACHE_ENV);
    }

    /// FR-013 / C6: a locally-established version wins and is not overwritten.
    #[test]
    fn m926_a_local_version_wins_over_nixpkgs() {
        let _g = EnvGuard::acquire();
        let _c = isolated_cache();
        let root = fixture_root(Some(LOCK), Some("haskell.packages.ghc96"));
        let src = StubSource::new(PACKAGES, CONFIG);
        let mut c = comp("waybill-fixture-liba");
        c.version = "0.0.1-from-freeze".to_string();
        // A second, versionless dependency keeps the gate open so the
        // comparison path actually runs.
        let mut comps = vec![c, comp("waybill-fixture-absent")];

        enrich(root.path(), &mut comps, opts(), &src).unwrap();

        assert_eq!(comps[0].version, "0.0.1-from-freeze", "local value must survive");
        assert!(comps[0].hashes.is_empty(), "and must not gain a nixpkgs hash");
        std::env::remove_var(cache::CACHE_ENV);
    }

    /// FR-013: when the two sources disagree, the difference is recorded
    /// rather than silently resolved. Without this a consumer cannot tell
    /// that the Nix build and the cabal build would use different versions.
    #[test]
    fn m926_a_disagreement_with_nixpkgs_is_recorded() {
        let _g = EnvGuard::acquire();
        let _c = isolated_cache();
        let root = fixture_root(Some(LOCK), Some("haskell.packages.ghc96"));
        let src = StubSource::new(PACKAGES, CONFIG);
        let mut c = comp("waybill-fixture-liba");
        c.version = "0.0.1-from-freeze".to_string();
        let mut comps = vec![c, comp("waybill-fixture-absent")];

        let s = enrich(root.path(), &mut comps, opts(), &src).unwrap();

        assert_eq!(s.disagreements, 1);
        let rec = comps[0]
            .extra_annotations
            .get(ANN_VERSION_DISAGREEMENT)
            .expect("the disagreement must be recorded");
        assert_eq!(rec.get("emitted").and_then(|v| v.as_str()), Some("0.0.1-from-freeze"));
        assert_eq!(rec.get("nixpkgs").and_then(|v| v.as_str()), Some("1.2.3"));
        std::env::remove_var(cache::CACHE_ENV);
    }

    /// Agreement is not a disagreement: no annotation when the two match.
    #[test]
    fn m926_agreement_records_nothing() {
        let _g = EnvGuard::acquire();
        let _c = isolated_cache();
        let root = fixture_root(Some(LOCK), Some("haskell.packages.ghc96"));
        let src = StubSource::new(PACKAGES, CONFIG);
        let mut c = comp("waybill-fixture-liba");
        c.version = "1.2.3".to_string(); // same as the package set
        let mut comps = vec![c, comp("waybill-fixture-absent")];

        let s = enrich(root.path(), &mut comps, opts(), &src).unwrap();

        assert_eq!(s.disagreements, 0);
        assert!(!comps[0].extra_annotations.contains_key(ANN_VERSION_DISAGREEMENT));
        std::env::remove_var(cache::CACHE_ENV);
    }

    /// A boot library carrying a local version is not a disagreement: nixpkgs
    /// has no version for it to disagree with.
    #[test]
    fn m926_a_boot_library_with_a_local_version_is_not_a_disagreement() {
        let _g = EnvGuard::acquire();
        let _c = isolated_cache();
        let root = fixture_root(Some(LOCK), Some("haskell.packages.ghc96"));
        let src = StubSource::new(PACKAGES, CONFIG);
        let mut c = comp("waybill-fixture-boot");
        c.version = "4.17.0.0".to_string();
        let mut comps = vec![c, comp("waybill-fixture-absent")];

        let s = enrich(root.path(), &mut comps, opts(), &src).unwrap();

        assert_eq!(s.disagreements, 0);
        assert!(!comps[0].extra_annotations.contains_key(ANN_VERSION_DISAGREEMENT));
        std::env::remove_var(cache::CACHE_ENV);
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod license_attach_tests {
    //! Milestone 1273 US3 (#1280) — `attach_declared_license`, tested
    //! directly.
    //!
    //! The precedence guard ("a license already present outranks this one")
    //! survived mutation M23, and investigation showed why: no component
    //! reaching `apply_resolved` can have a pre-existing license today.
    //! `build_main_module` is the only reader path that sets one, and the
    //! main module carries a version, so `is_enrichable` excludes it.
    //!
    //! That made the guard **untested defensive code**. Rather than delete a
    //! precedence invariant that a future caller would need — the m1010
    //! manifest-declared licenses are exactly such a caller — it is tested
    //! here against the helper, where the case is reachable without inventing
    //! a scan input that does not exist.

    use super::enrich_tests::comp;
    use super::*;
    use package_set::DeclaredLicense;
    use waybill_common::types::license::SpdxExpression;

    #[test]
    fn a_canonical_declaration_lands_in_the_native_field() {
        let mut c = comp("waybill-fixture-x");
        let lic = DeclaredLicense::Canonical(
            SpdxExpression::try_canonical("BSD-3-Clause").unwrap(),
        );
        attach_declared_license(&mut c, Some(&lic));
        assert_eq!(c.licenses.len(), 1);
        assert_eq!(c.licenses[0].as_str(), "BSD-3-Clause");
    }

    #[test]
    fn an_uncanonical_declaration_becomes_a_license_ref() {
        let mut c = comp("waybill-fixture-x");
        let lic = DeclaredLicense::Uncanonical("GPL".to_string());
        attach_declared_license(&mut c, Some(&lic));
        assert_eq!(c.licenses.len(), 1);
        // SPDX's own mechanism for a license off its list. The prefix IS the
        // record that the value is not a canonical identifier (FR-011).
        assert_eq!(c.licenses[0].as_str(), "LicenseRef-GPL");
    }

    /// The case mutation M23 removed. A license already on the component —
    /// a project-local declaration (milestone 1010), or an earlier
    /// enrichment — is closer to the artifact than a distro's package set,
    /// so it stands.
    #[test]
    fn an_existing_license_is_not_overwritten() {
        let mut c = comp("waybill-fixture-x");
        c.licenses
            .push(SpdxExpression::try_canonical("MIT").unwrap());
        let lic = DeclaredLicense::Canonical(
            SpdxExpression::try_canonical("BSD-3-Clause").unwrap(),
        );
        attach_declared_license(&mut c, Some(&lic));
        assert_eq!(
            c.licenses.iter().map(|l| l.as_str()).collect::<Vec<_>>(),
            vec!["MIT"],
            "the package set must not displace a closer declaration"
        );
    }

    /// FR-012 — nothing is invented when the revision declares nothing.
    #[test]
    fn no_declaration_attaches_nothing() {
        let mut c = comp("waybill-fixture-x");
        attach_declared_license(&mut c, None);
        assert!(c.licenses.is_empty());
    }

    /// A declaration that sanitizes to nothing attaches nothing rather than
    /// an empty `LicenseRef-`.
    #[test]
    fn an_unsanitizable_declaration_attaches_nothing() {
        let mut c = comp("waybill-fixture-x");
        let lic = DeclaredLicense::Uncanonical("!@#$".to_string());
        attach_declared_license(&mut c, Some(&lic));
        assert!(c.licenses.is_empty());
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod closure_reconcile_tests {
    //! Issue #955 + #1281 — a mark is withdrawn only where a closure seeded
    //! WITHOUT the marked set still reaches the component.
    //!
    //! These were written against the pre-#1281 signature, which took the
    //! closure's edge list. They now take the reachability map of the
    //! re-seeded walk, which is the input that can actually answer the
    //! question: the edge list could not, because a marked component is a
    //! seed of the walk that produced it and therefore appears in its own
    //! out-edges.

    use super::enrich_tests::comp;
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    use waybill_common::resolution::LifecycleScope;

    fn marked(name: &str) -> ResolvedComponent {
        let mut c = comp(name);
        c.lifecycle_scope = Some(LifecycleScope::Optional);
        c.extra_annotations.insert(
            "waybill:optional-derivation".to_string(),
            serde_json::Value::String("cabal-flag-disabled-stanza".to_string()),
        );
        c
    }

    /// `name` reached by `parents` in the walk seeded without the marks.
    fn reached(pairs: &[(&str, &[&str])]) -> BTreeMap<String, BTreeSet<String>> {
        pairs
            .iter()
            .map(|(n, ps)| {
                (
                    (*n).to_string(),
                    ps.iter().map(|p| (*p).to_string()).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn a_mark_the_unmarked_closure_reaches_is_withdrawn() {
        let mut comps = vec![marked("butcher"), marked("monad-loops")];
        // The walk seeded WITHOUT the marks still reaches `butcher`, through
        // `slack-web` — so it really is built and the mark goes.
        let r = reached(&[("butcher", &["slack-web"])]);
        let record = reconcile_optional_with_closure(&r, &mut comps);

        assert_eq!(record.reviewed, 2);
        assert_eq!(record.withdrawn.len(), 1);
        assert_eq!(record.declined, 1);
        // FR-007 — the withdrawal names the non-seed parent.
        assert_eq!(record.withdrawn["butcher"], "slack-web");

        assert_eq!(comps[0].lifecycle_scope, Some(LifecycleScope::Runtime));
        assert!(!comps[0].extra_annotations.contains_key("waybill:optional-derivation"));

        // FR-009 — not reached, so BOTH the scope and its derivation stand.
        assert_eq!(comps[1].lifecycle_scope, Some(LifecycleScope::Optional));
        assert_eq!(
            comps[1].extra_annotations["waybill:optional-derivation"].as_str(),
            Some("cabal-flag-disabled-stanza"),
        );
    }

    /// #1281's defect, as a unit test. Before the fix the input was the edge
    /// list of a walk seeded FROM the marked set, so a marked component was
    /// present by construction. Here the re-seeded walk reaches neither, and
    /// both marks must stand — the case the old implementation could not
    /// produce.
    #[test]
    fn marks_reached_only_by_the_marked_set_all_stand() {
        let mut comps = vec![marked("butcher"), marked("monad-loops")];
        let record = reconcile_optional_with_closure(&reached(&[]), &mut comps);
        assert_eq!(record.reviewed, 2);
        assert!(record.withdrawn.is_empty());
        assert_eq!(record.declined, 2);
        for c in &comps {
            assert_eq!(c.lifecycle_scope, Some(LifecycleScope::Optional));
        }
    }

    /// A component the re-seeded walk reaches as a SEED of its own has no
    /// parent. It is still built — the project declares it through a stanza
    /// that is not excluded — so the mark goes, with a justification saying
    /// so rather than an empty string.
    #[test]
    fn a_seed_of_the_unmarked_walk_is_withdrawn_with_a_reason() {
        let mut comps = vec![marked("butcher")];
        let record = reconcile_optional_with_closure(&reached(&[("butcher", &[])]), &mut comps);
        assert_eq!(record.withdrawn.len(), 1);
        assert_eq!(
            record.withdrawn["butcher"],
            "declared by a buildable stanza"
        );
        assert_eq!(comps[0].lifecycle_scope, Some(LifecycleScope::Runtime));
    }

    /// The pass touches ONLY components this reader marked. An optional
    /// classification from any other ecosystem mechanism — cargo's
    /// `optional = true`, npm's `optionalDependencies` — is not ours to
    /// withdraw, whatever the closure contains. (FR-015.)
    #[test]
    fn an_optional_from_another_mechanism_is_left_alone() {
        let mut c = comp("some-lib");
        c.lifecycle_scope = Some(LifecycleScope::Optional);
        c.extra_annotations.insert(
            "waybill:optional-derivation".to_string(),
            serde_json::Value::String("cargo-optional-true".to_string()),
        );
        let mut comps = vec![c];
        let record =
            reconcile_optional_with_closure(&reached(&[("some-lib", &["x"])]), &mut comps);
        assert_eq!(
            record.reviewed, 0,
            "only `cabal-flag-disabled-stanza` marks are ours to review"
        );
        assert!(record.withdrawn.is_empty());
        assert_eq!(comps[0].lifecycle_scope, Some(LifecycleScope::Optional));
    }

    /// FR-008 — "ran and withdrew nothing" is a record, not an absence. The
    /// absence is reserved for "did not run", which `enrich` represents by
    /// leaving `optional_reconciliation` as `None`.
    #[test]
    fn an_empty_reachability_map_still_produces_a_record() {
        let mut comps = vec![marked("butcher")];
        let record = reconcile_optional_with_closure(&reached(&[]), &mut comps);
        assert_eq!(record.reviewed, 1);
        assert!(record.withdrawn.is_empty());
        assert_eq!(comps[0].lifecycle_scope, Some(LifecycleScope::Optional));
        // And it renders for the document rather than being log-only.
        let v = record.to_document_value();
        assert_eq!(v["reviewed"], 1);
        assert_eq!(v["declined"], 1);
        assert_eq!(v["withdrawn"].as_array().map(Vec::len), Some(0));
    }
}

/// Milestone 1275 T005 — what a second parse of the package set costs.
///
/// Not a test of behaviour: a committed, re-runnable probe answering one
/// question the plan asserted rather than measured. `enrich` and the
/// closure-identity pass both need the parsed set, and `EnrichmentSummary`
/// cannot carry it between them (adding a `PackageSet` field breaks that
/// struct's `PartialEq`). The per-revision disk cache means the second
/// caller performs no second FETCH; it does perform a second PARSE, and
/// whether that is material decides whether a per-scan memo is worth its
/// own failure modes.
///
/// `#[ignore]` and env-gated: it reads a real 16 MB `hackage-packages.nix`,
/// which no ordinary test may do (Principle VII). Run it deliberately:
///
/// ```sh
/// WAYBILL_T005_PACKAGE_SET=~/.cache/waybill/nixpkgs/<rev>/hackage-packages.nix \
///   cargo test -p waybill --lib m1275_t005 -- --ignored --nocapture
/// ```
#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod t005_parse_cost_probe {
    use super::package_set;

    #[test]
    #[ignore = "reads a real package set; env-gated probe, not a behaviour test"]
    fn m1275_t005_second_parse_cost() {
        let Ok(path) = std::env::var("WAYBILL_T005_PACKAGE_SET") else {
            eprintln!("WAYBILL_T005_PACKAGE_SET unset; nothing measured");
            return;
        };
        let text = std::fs::read_to_string(&path).unwrap();
        let mut ms: Vec<u128> = Vec::new();
        // N=5. One run is not a measurement -- a single observation was read
        // as a property twice in one session on this project.
        for _ in 0..5 {
            let t = std::time::Instant::now();
            let set = package_set::parse(&text);
            let elapsed = t.elapsed().as_millis();
            // Consume the result so the parse cannot be optimised away.
            assert!(!set.is_empty(), "a package set that parsed to nothing");
            ms.push(elapsed);
        }
        ms.sort_unstable();
        eprintln!(
            "T005 parse of {} bytes, N=5: min={}ms median={}ms max={}ms (entries={})",
            text.len(),
            ms[0],
            ms[2],
            ms[4],
            package_set::parse(&text).len(),
        );
    }
}
