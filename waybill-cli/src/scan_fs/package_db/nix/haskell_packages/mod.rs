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

// ---------------------------------------------------------------------------
// Orchestration (T016): retrieve, classify, and enrich in place.
// ---------------------------------------------------------------------------

use std::path::Path;

use waybill_common::resolution::ResolvedComponent;
use waybill_common::types::hash::ContentHash;

/// Annotation keys. Registered as catalog rows by T031.
pub(crate) const ANN_RESOLVED_VIA: &str = "waybill:nixpkgs-resolved-via";
pub(crate) const ANN_UNRESOLVED_REASON: &str = "waybill:haskell-version-unresolved-reason";
pub(crate) const ANN_CANDIDATE_COMPILERS: &str = "waybill:nixpkgs-candidate-compilers";

/// Options the operator controls.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ResolveOptions {
    /// `--offline` (FR-009).
    pub(crate) offline: bool,
    /// The feature's own opt-out (FR-015a).
    pub(crate) disabled: bool,
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
}

impl EnrichmentSummary {
    fn note_unresolved(&mut self, reason: &UnresolvedReason) {
        *self
            .unresolved
            .entry(reason.as_str().to_string())
            .or_insert(0) += 1;
    }
}

/// Is this component a Haskell dependency this pass may enrich?
///
/// Deliberately narrow. Enrichment never introduces a component
/// (Principle XII constraint 1) and never touches one that already has a
/// version — a version from a project-local freeze file outranks a
/// nixpkgs-resolved one (FR-013, contract C6).
fn is_enrichable(c: &ResolvedComponent) -> bool {
    c.purl.as_str().starts_with("pkg:hackage/") && c.version.is_empty()
}

/// Enrich Haskell components with versions from the pinned nixpkgs.
///
/// Returns `None` when the gate (contract C1) did not open, which is the
/// common case: a repository with no flake, or no Haskell dependencies, does
/// no work and produces no annotation (SC-009).
pub(crate) fn enrich(
    scan_root: &Path,
    components: &mut [ResolvedComponent],
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
    let lock_path = scan_root.join("flake.lock");
    if !lock_path.exists() {
        return None;
    }
    let doc = match super::lockfile::parse_flake_lock(&lock_path) {
        Ok(d) => d,
        Err(e) => {
            tracing::debug!(error = ?e, "nixpkgs-haskell: flake.lock unparseable");
            return None;
        }
    };
    // Gate half 1: a nixpkgs-shaped input pinned to an exact revision.
    let pinned = match pinned_nixpkgs(&doc) {
        Ok(p) => p,
        Err(reason) => return Some(degrade_all(components, reason)),
    };
    if opts.offline {
        return Some(degrade_all(components, UnresolvedReason::Offline));
    }
    let Some(location) = pinned.location.clone() else {
        return Some(degrade_all(components, UnresolvedReason::SourceUnsupported));
    };

    // Retrieve the package set, preferring the cache (FR-010, SC-005).
    let text = match cached_or_fetch(
        source,
        &location,
        &pinned.revision,
        HACKAGE_PACKAGES_PATH,
        HACKAGE_PACKAGES_KEY,
    ) {
        Ok(t) => t,
        Err(e) => {
            let reason = UnresolvedReason::from(&e);
            tracing::info!(
                revision = %pinned.revision,
                reason = reason.as_str(),
                "nixpkgs-haskell: degrading, package set unavailable"
            );
            return Some(degrade_all(components, reason));
        }
    };
    let packages = package_set::parse(&text);
    tracing::debug!(
        revision = %pinned.revision,
        packages = packages.len(),
        "nixpkgs-haskell: package set parsed"
    );
    if packages.is_empty() {
        return Some(degrade_all(components, UnresolvedReason::SourceUnreachable));
    }

    // Boot libraries, unioned across every candidate compiler (FR-014a).
    let (boot, candidates) = boot_set(source, &location, &pinned.revision, scan_root);

    let mut summary = EnrichmentSummary {
        revision: Some(pinned.revision.clone()),
        ..Default::default()
    };
    let candidate_note = (candidates.len() > 1).then(|| {
        serde_json::Value::String(candidates.join(","))
    });

    for c in components.iter_mut().filter(|c| is_enrichable(c)) {
        match classify(&c.name, &packages, &boot, &pinned.revision) {
            ResolutionOutcome::Resolved {
                version,
                source_hash,
                revision,
            } => {
                apply_resolved(c, &version, source_hash.as_deref(), &revision, pinned.matched_by);
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
                }
            }
        }
    }
    Some(summary)
}

/// Attach a resolved version, its hash and its provenance.
///
/// The hash goes into `hashes`, which every emitter already maps to its
/// format's **native** checksum field — research R2 verified this value is a
/// flat SHA-256 of the source tarball, so a `waybill:` annotation would
/// violate Principle V.
fn apply_resolved(
    c: &mut ResolvedComponent,
    version: &str,
    source_hash: Option<&str>,
    revision: &str,
    matched_by: NixpkgsMatch,
) {
    c.version = version.to_string();
    // The PURL must carry the version too, or the component is versioned in
    // one place and not the other.
    if let Ok(p) = waybill_common::types::purl::Purl::new(&format!(
        "pkg:hackage/{}@{}",
        c.name, version
    )) {
        c.purl = p;
    }
    if let Some(hex) = source_hash {
        // `ContentHash::sha256` validates both the alphabet and the 64-char
        // width. A value that fails here contributes no hash rather than a
        // malformed one, which is the same posture the decoder takes
        // (Principle IX). It should be unreachable: the decoder already
        // rejected anything that was not 32 bytes.
        match ContentHash::sha256(hex) {
            Ok(h) => c.hashes.push(h),
            Err(e) => tracing::debug!(
                error = %e,
                package = %c.name,
                "nixpkgs-haskell: refusing a source hash that failed validation"
            ),
        }
    }
    c.sbom_tier = Some("source".to_string());
    c.extra_annotations.insert(
        ANN_RESOLVED_VIA.to_string(),
        serde_json::json!({
            "source": "nixpkgs",
            "revision": revision,
            "matched-by": matched_by.as_str(),
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
fn cached_or_fetch(
    source: &dyn fetch::RevisionSource,
    location: &Location,
    rev: &str,
    path: &str,
    key: &str,
) -> Result<String, FetchError> {
    if let Some(hit) = cache::read(rev, key) {
        return Ok(hit);
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
fn boot_set(
    source: &dyn fetch::RevisionSource,
    location: &Location,
    rev: &str,
    scan_root: &Path,
) -> (BTreeSet<String>, Vec<String>) {
    let candidates = candidate_series(scan_root);
    let mut texts: Vec<String> = Vec::new();
    for series in &candidates {
        let path = format!("pkgs/development/haskell-modules/configuration-ghc-{series}.nix");
        let key = format!("configuration-ghc-{series}.nix");
        if let Ok(text) = cached_or_fetch(source, location, rev, &path, &key) {
            texts.push(text);
        }
    }
    let boot = boot_libraries::union_nulled(texts.iter().map(String::as_str));
    (boot, candidates)
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
                        "rev": "a799d3e3886da994fa307f817a6bc705ae538eeb" }
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
    fn comp(name: &str) -> ResolvedComponent {
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
      mkDerivation { pname = "waybill-fixture-liba"; version = "1.2.3"; sha256 = "1zym9yia0is8wxfd6d1ldwvvghwxg4ww3y4lrxnyb2nk60ig29ly"; }
      mkDerivation { pname = "waybill-fixture-boot"; version = "9.9.9"; sha256 = "091h1ifc1srv803rrkzc8mgvhpsnw6cn6r0mqqs44ss1shjaan6r"; }
    "#;
    const CONFIG: &str = r#"self: super: { waybill-fixture-boot = null; }"#;

    fn opts() -> ResolveOptions {
        ResolveOptions { offline: false, disabled: false }
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

    /// FR-013 / C6: a component that already has a version is untouched — a
    /// local freeze file outranks nixpkgs.
    #[test]
    fn m926_an_already_versioned_component_is_left_alone() {
        let _g = EnvGuard::acquire();
        let _c = isolated_cache();
        let root = fixture_root(Some(LOCK), Some("haskell.packages.ghc96"));
        let src = StubSource::new(PACKAGES, CONFIG);
        let mut c = comp("waybill-fixture-liba");
        c.version = "0.0.1-from-freeze".to_string();
        let mut comps = vec![c];

        assert!(enrich(root.path(), &mut comps, opts(), &src).is_none());
        assert_eq!(comps[0].version, "0.0.1-from-freeze");
        std::env::remove_var(cache::CACHE_ENV);
    }
}
