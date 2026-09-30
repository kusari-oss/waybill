//! What nixpkgs itself declares about the packages a build contains.
//!
//! `meta.knownVulnerabilities` is a first-party statement by the package set
//! the project pins, and Nix refuses to *evaluate* a package carrying one
//! unless the build permits it. That makes it a different kind of signal from
//! an advisory feed: it describes a decision rather than a match, and it does
//! not go stale.
//!
//! It is also not in the closure. Measured on a real project, `meta` appears
//! in **none** of 1,275 derivations — it is eval-time nixpkgs data, and a
//! closure member offers only its `pname` as a handle. So reaching it means a
//! second evaluation against the pinned package set, resolving a candidate
//! attribute by name and then proving the candidate is the same build before
//! believing anything it says.

pub(crate) mod evaluate;
pub(crate) mod parse;
pub(crate) mod resolve;

/// Which package set answered for a member.
///
/// Ordered, and the order is the resolution order rather than anything
/// alphabetical. Top-level alone reaches 22% of a measured closure; adding
/// the nested sets reaches 71%, with `haskellPackages` contributing more than
/// top-level on a Haskell project. The nested sets are most of the coverage,
/// not a refinement of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DeclarationSource {
    TopLevel,
    Haskell,
    Python3,
    Perl,
}

impl DeclarationSource {
    /// The sets to try, in order. First *confirmed* hit wins.
    pub(crate) const ORDER: [Self; 4] = [Self::TopLevel, Self::Haskell, Self::Python3, Self::Perl];

    /// The Nix expression that selects this set from `pkgs`.
    pub(crate) fn selector(self) -> &'static str {
        match self {
            Self::TopLevel => "pkgs",
            Self::Haskell => "pkgs.haskellPackages or {}",
            Self::Python3 => "pkgs.python3Packages or {}",
            Self::Perl => "pkgs.perlPackages or {}",
        }
    }

    pub(crate) fn wire(self) -> &'static str {
        match self {
            Self::TopLevel => "top-level",
            Self::Haskell => "haskell",
            Self::Python3 => "python3",
            Self::Perl => "perl",
        }
    }

    pub(crate) fn from_wire(s: &str) -> Option<Self> {
        Self::ORDER.into_iter().find(|c| c.wire() == s)
    }
}

/// What happened when we tried to reach one member's declaration.
///
/// `PathMismatch` and `NoAttribute` both report as *unchecked*, but they are
/// kept apart because they say different things about why. One is a name
/// collision; the other is a reach limit, and a later milestone widening the
/// probed sets moves members from the second into the first or into
/// `Confirmed`. Collapsing them now would erase the distinction that tells
/// you which.
///
/// None of the three means "nixpkgs says nothing about this". That claim
/// requires `Confirmed` with an empty declaration list, and conflating it
/// with the other two is the failure spec FR-001c exists to prevent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttributeResolution {
    /// An attribute was found and its output path matched the member's, so
    /// its declarations are authoritative for this member. The list may be
    /// empty, which is the healthy majority.
    Confirmed {
        source: DeclarationSource,
        declarations: Vec<parse::Declaration>,
    },
    /// An attribute of that name exists but builds something else. The
    /// declaration is discarded. Measured at 9% of members — these are the
    /// false attributions the path check exists to prevent.
    PathMismatch,
    /// No candidate in any probed set. Measured at 18%.
    NoAttribute,
}

impl AttributeResolution {
    /// Whether this member's declarations are known at all.
    pub(crate) fn is_checked(&self) -> bool {
        matches!(self, Self::Confirmed { .. })
    }
}

/// One CVE-bearing declaration, bound to the component it is about.
///
/// Carried to the VEX emitter through this record rather than on the
/// component's `extra_annotations`, and the distinction is deliberate: that
/// bag auto-flows into SBOM properties, and a CVE claim in an SBOM is
/// precisely what this feature exists not to emit. An SBOM is a composition
/// snapshot; a claim about a vulnerability belongs in VEX, where it can be
/// superseded without rewriting the composition.
///
/// The prose declarations go the other way — they *are* composition facts
/// ("this vendors an EOL Electron") and ride the annotation bag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredFinding {
    pub component_purl: String,
    pub cve: String,
    /// The declaration's own words, for the statement's note.
    pub text: String,
}

/// What this pass contributed, for the document-scope record.
///
/// A sibling of milestone 1035's `NixClosureSummary` rather than an extension
/// of it: the closure query and this pass degrade independently, and the spec
/// requires that be visible. Folding them together would make one record
/// carry two failure modes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NixpkgsSecuritySummary {
    pub members_checked: usize,
    /// Members whose attribute could not be confirmed. Reported because
    /// "nixpkgs says nothing about this" and "we could not ask" are different
    /// claims, and a consumer who cannot tell them apart will read the first
    /// when the second is true.
    pub members_unchecked: usize,
    /// Which set answered, for the members that were confirmed.
    pub confirmed_by_set: std::collections::BTreeMap<&'static str, usize>,
    pub declarations_total: usize,
    /// Declarations naming no CVE. Measured at 28%, and they are the half no
    /// advisory feed carries, so the count stops their absence from VEX
    /// reading as their absence entirely.
    pub declarations_without_cve: usize,
    pub distinct_cves: usize,
    /// Whether any confirmed member carried a declaration at all.
    ///
    /// Derived, not observed: Nix refuses to *evaluate* a package marked
    /// insecure, so a member carrying one is in the closure only because the
    /// build permitted it. Its presence is the permission.
    pub accepted_insecure: bool,
    /// CVE-bearing declarations, for the VEX emitter. Deliberately not on
    /// the components — see [`DeclaredFinding`].
    pub findings: Vec<DeclaredFinding>,
    /// How many patch-derived `not_affected` statements a declaration
    /// displaced (FR-013a).
    ///
    /// Emitted because silence and suppression look identical from outside.
    /// A consumer who finds no `not_affected` for a CVE needs to know whether
    /// none was produced or one was withheld — the second means the build
    /// patched it and nixpkgs disagreed, which is a fact about the build
    /// worth having.
    pub reconciliations_withheld: usize,
}

impl NixpkgsSecuritySummary {
    /// Build the record. `purl_of` maps a member's `pname` to the PURL the
    /// emitted component carries, so findings bind to the identity a
    /// consumer will actually see rather than to a nix-internal name.
    pub(crate) fn build_with_purls(
        resolutions: &std::collections::BTreeMap<String, AttributeResolution>,
        purl_of: &dyn Fn(&str) -> Option<String>,
    ) -> Self {
        let mut s = Self::build(resolutions);
        for (pname, r) in resolutions {
            let AttributeResolution::Confirmed { declarations, .. } = r else {
                continue;
            };
            let Some(purl) = purl_of(pname) else { continue };
            for d in declarations {
                for cve in &d.cves {
                    s.findings.push(DeclaredFinding {
                        component_purl: purl.clone(),
                        cve: cve.clone(),
                        text: d.text.clone(),
                    });
                }
            }
        }
        s.findings.sort_by(|a, b| {
            (&a.component_purl, &a.cve).cmp(&(&b.component_purl, &b.cve))
        });
        s.findings.dedup_by(|a, b| {
            a.component_purl == b.component_purl && a.cve == b.cve
        });
        s
    }

    pub(crate) fn build(
        resolutions: &std::collections::BTreeMap<String, AttributeResolution>,
    ) -> Self {
        let mut s = Self::default();
        let mut cves = std::collections::BTreeSet::new();
        for r in resolutions.values() {
            match r {
                AttributeResolution::Confirmed {
                    source,
                    declarations,
                } => {
                    s.members_checked += 1;
                    *s.confirmed_by_set.entry(source.wire()).or_insert(0) += 1;
                    for d in declarations {
                        s.declarations_total += 1;
                        s.accepted_insecure = true;
                        if d.names_a_cve() {
                            cves.extend(d.cves.iter().cloned());
                        } else {
                            s.declarations_without_cve += 1;
                        }
                    }
                }
                AttributeResolution::PathMismatch | AttributeResolution::NoAttribute => {
                    s.members_unchecked += 1;
                }
            }
        }
        s.distinct_cves = cves.len();
        s
    }
}

/// The (component, CVE) pairs where a declaration displaces a patch-derived
/// `not_affected` (spec FR-012).
///
/// One definition, used by both the emitter that withholds the statement and
/// the record that counts the withholdings. Computing it twice would let the
/// count drift from the behaviour it describes, and a count that disagrees
/// with the document is worse than no count — it tells a reader the
/// suppression happened somewhere they cannot find.
pub(crate) fn withheld_pairs(
    components: &[waybill_common::resolution::ResolvedComponent],
    findings: &[DeclaredFinding],
) -> std::collections::BTreeSet<(String, String)> {
    use crate::scan_fs::package_db::nix::closure::emit::ANN_CLOSURE_PATCHES;

    // CVEs each component's patches name, from the annotation milestone 1035
    // stamps. Read from the component rather than re-derived, so the two
    // features agree about what was patched by construction.
    let mut patched: std::collections::BTreeSet<(String, String)> = Default::default();
    for c in components {
        let Some(raw) = c
            .extra_annotations
            .get(ANN_CLOSURE_PATCHES)
            .and_then(|v| v.as_str())
        else {
            continue;
        };
        let Ok(list) = serde_json::from_str::<serde_json::Value>(raw) else {
            continue;
        };
        for id in list
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| p.get("resolves"))
            .filter_map(|r| r.as_array())
            .flatten()
            .filter_map(|i| i.get("id").and_then(|v| v.as_str()))
        {
            patched.insert((c.purl.as_str().to_string(), id.to_string()));
        }
    }

    // The intersection. A declaration about one component and a patch about
    // another are unrelated claims (FR-014) and neither displaces the other.
    findings
        .iter()
        .map(|f| (f.component_purl.clone(), f.cve.clone()))
        .filter(|k| patched.contains(k))
        .collect()
}

/// Ask the pinned package set what it declares about this build's packages.
///
/// Degrades rather than fails (FR-019), and degrades *independently* of the
/// closure query: a closure that resolved still emits its components when
/// this returns an error. The two are separate passes over the same data and
/// the spec requires their outcomes be separately visible.
pub(crate) fn run(
    closure: &crate::scan_fs::package_db::nix::closure::ClassifiedClosure,
    project_root: &std::path::Path,
    system: &str,
    budget: std::time::Duration,
    purl_of: &dyn Fn(&str) -> Option<String>,
) -> Result<NixpkgsSecuritySummary, crate::scan_fs::package_db::nix::eval::reason::DegradationReason>
{
    use crate::scan_fs::package_db::nix::eval::reason::DegradationReason;

    // `declared_pname`, not `pname`: the latter falls back to the
    // derivation's own name, and on a measured closure 447 of 823 such names
    // are patch files and fetched tarballs. Those have no `meta` and asking
    // nixpkgs about them is meaningless.
    let mut members: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for drv in closure.raw.derivations.values() {
        let Some(p) = drv.declared_pname() else { continue };
        members
            .entry(p.to_string())
            .or_default()
            .extend(drv.output_paths().map(str::to_string));
    }
    if members.is_empty() {
        return Ok(NixpkgsSecuritySummary::default());
    }

    // Reaching nixpkgs through the revision `flake.lock` pins keeps the
    // evaluation pure. Reaching it through the project's flake by path would
    // need `--impure`, which milestone 1034's argv guard refuses — correctly,
    // since that flag restores access to the host environment and the loss
    // would not be visible in any emitted document.
    let lock_path = project_root.join("flake.lock");
    let doc = crate::scan_fs::package_db::nix::lockfile::parse_flake_lock(&lock_path)
        .map_err(|e| DegradationReason::RevisionUnfetchable {
            revision: String::new(),
            detail: format!("flake.lock: {e}"),
        })?;
    let revision = doc
        .nodes
        .values()
        .filter_map(|n| n.locked.as_ref())
        .find_map(|l| {
            l.repo
                .as_deref()
                .filter(|r| *r == "nixpkgs")
                .and_then(|_| l.rev.clone())
        })
        .ok_or(DegradationReason::RevisionUnfetchable {
            revision: String::new(),
            detail: "no pinned nixpkgs revision in flake.lock".to_string(),
        })?;

    let names: Vec<String> = members.keys().cloned().collect();
    let candidates = evaluate::evaluate(&revision, system, &names, budget)?;
    let resolutions = resolve::resolve_all(&members, &candidates);
    Ok(NixpkgsSecuritySummary::build_with_purls(&resolutions, purl_of))
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn the_probe_order_puts_top_level_first_and_keeps_the_nested_sets() {
        // The order is load-bearing: first confirmed hit wins, so a
        // reshuffle changes which set answers for a name present in two.
        assert_eq!(
            DeclarationSource::ORDER,
            [
                DeclarationSource::TopLevel,
                DeclarationSource::Haskell,
                DeclarationSource::Python3,
                DeclarationSource::Perl
            ]
        );
    }

    #[test]
    fn every_source_round_trips_through_its_wire_form() {
        for s in DeclarationSource::ORDER {
            assert_eq!(DeclarationSource::from_wire(s.wire()), Some(s), "{s:?}");
        }
    }

    /// Reproduce research R1 from the shipped code, not from the probe.
    ///
    /// Everything downstream assumes ~71% of a real closure can be reached.
    /// That figure came from a throwaway script; this asserts the Rust path
    /// produces it too. Point `WAYBILL_TEST_CLOSURE_JSON` at a closure dump
    /// and `WAYBILL_TEST_PROJECT` at the project it came from.
    ///
    /// The lower bound on path-mismatch matters as much as the coverage band:
    /// a mismatch near zero would mean the verification is rejecting nothing,
    /// which looks like clean coverage and is actually the check not running.
    #[test]
    fn coverage_against_a_real_closure_matches_the_measured_figures() {
        let (Ok(closure_path), Ok(project)) = (
            std::env::var("WAYBILL_TEST_CLOSURE_JSON"),
            std::env::var("WAYBILL_TEST_PROJECT"),
        ) else {
            eprintln!("skipping: set WAYBILL_TEST_CLOSURE_JSON and WAYBILL_TEST_PROJECT");
            return;
        };
        let json = std::fs::read_to_string(&closure_path).expect("closure json");
        let raw = crate::scan_fs::package_db::nix::closure::derivation::RawClosure::parse(&json)
            .expect("closure parses");

        // pname -> every output path the closure records for it.
        //
        // `declared_pname`, not `pname`: the latter falls back to the
        // derivation's own name, which for 447 of moat's 823 is a patch file
        // or a fetched tarball. Asking nixpkgs about `CVE-2019-13232-1.patch`
        // is meaningless, and counting it as an unreachable package would
        // halve the apparent coverage while measuring something else.
        let mut members: std::collections::BTreeMap<String, Vec<String>> = Default::default();
        for drv in raw.derivations.values() {
            let Some(p) = drv.declared_pname() else { continue };
            members
                .entry(p.to_string())
                .or_default()
                .extend(drv.output_paths().map(str::to_string));
        }
        let names: Vec<String> = members.keys().cloned().collect();
        eprintln!("{} distinct pnames", names.len());

        let system = crate::scan_fs::package_db::nix::eval::invoke::detect_host_system(
            std::time::Duration::from_secs(120),
        )
        .expect("host system");
        // The revision the project pins, from its own lockfile. Reaching
        // nixpkgs through a locked flakeref keeps the call pure; reaching it
        // through the project's flake by path would need `--impure`, which
        // milestone 1034's argv guard refuses.
        let lock = crate::scan_fs::package_db::nix::lockfile::parse_flake_lock(
            &std::path::Path::new(&project).join("flake.lock"),
        )
        .expect("flake.lock parses");
        let revision = lock
            .nodes
            .values()
            .filter_map(|n| n.locked.as_ref())
            .find_map(|l| {
                l.repo
                    .as_deref()
                    .filter(|r| *r == "nixpkgs")
                    .and_then(|_| l.rev.clone())
            })
            .expect("a pinned nixpkgs revision");

        let started = std::time::Instant::now();
        let candidates = evaluate::evaluate(
            &revision,
            system.as_str(),
            &names,
            std::time::Duration::from_secs(300),
        )
        .expect("evaluation");
        let elapsed = started.elapsed();

        let resolutions = resolve::resolve_all(&members, &candidates);
        let s = NixpkgsSecuritySummary::build(&resolutions);
        let total = resolutions.len();
        let mismatch = resolutions
            .values()
            .filter(|r| matches!(r, AttributeResolution::PathMismatch))
            .count();
        let pct = |n: usize| 100 * n / total.max(1);
        eprintln!(
            "members {total}: confirmed {} ({}%), unchecked {} ({}%), mismatch {} ({}%), sets {:?}, {:.1}s",
            s.members_checked,
            pct(s.members_checked),
            s.members_unchecked,
            pct(s.members_unchecked),
            mismatch,
            pct(mismatch),
            s.confirmed_by_set,
            elapsed.as_secs_f64()
        );

        // CONTROL: the closure yielded members at all, so the bands below are
        // not being satisfied by an empty denominator.
        assert!(total > 100, "suspiciously small closure: {total}");
        assert!(
            (65..=77).contains(&pct(s.members_checked)),
            "confirmed {}% is outside the 65-77% band research R1 measured",
            pct(s.members_checked)
        );
        assert!(
            pct(mismatch) >= 5,
            "path-mismatch {}% is below 5%: the verification is rejecting \
             nothing, which reads as clean coverage and is more likely the \
             check not running",
            pct(mismatch)
        );
        let nested: usize = ["haskell", "python3", "perl"]
            .iter()
            .filter_map(|k| s.confirmed_by_set.get(*k))
            .sum();
        assert!(
            nested > *s.confirmed_by_set.get("top-level").unwrap_or(&0),
            "the nested sets should contribute more than top-level; got {:?}",
            s.confirmed_by_set
        );
    }

    #[test]
    fn only_a_confirmed_resolution_counts_as_checked() {
        // The distinction FR-001c turns on: an unchecked member must never be
        // reported as one nixpkgs said nothing about.
        assert!(AttributeResolution::Confirmed {
            source: DeclarationSource::TopLevel,
            declarations: vec![]
        }
        .is_checked());
        assert!(!AttributeResolution::PathMismatch.is_checked());
        assert!(!AttributeResolution::NoAttribute.is_checked());
    }
}
