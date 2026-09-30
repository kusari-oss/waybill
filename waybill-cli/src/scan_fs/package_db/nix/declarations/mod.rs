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
    /// How many distinct packages carried a declaration. The record is coarse
    /// by design (FR-015a) — which packages they are is already answerable
    /// from the declaration-derived statements — but a count distinguishes
    /// one accepted exception from twenty.
    pub accepted_insecure_count: usize,
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
    /// The C188 wire value, identical in all three formats.
    ///
    /// One canonical string rather than an object in SPDX and a string in
    /// CycloneDX: a CDX property value must be a string, and a row differing
    /// in shape between formats forces its parity extractor to compensate —
    /// which papered over a real defect in milestone 1035.
    pub fn wire(&self) -> String {
        self.value().to_string()
    }

    /// The C188 annotation value.
    ///
    /// Coverage comes first because it bounds everything after it. A reader
    /// who does not know how many members were checked cannot interpret a
    /// declaration count, and the common case for this feature is finding
    /// nothing — a project that builds has already permitted whatever it
    /// contains.
    pub fn value(&self) -> serde_json::Value {
        serde_json::json!({
            "members-checked": self.members_checked,
            "members-unchecked": self.members_unchecked,
            "confirmed-by-set": self.confirmed_by_set,
            "declarations": self.declarations_total,
            "declarations-without-cve": self.declarations_without_cve,
            "distinct-cves": self.distinct_cves,
            "reconciliations-withheld": self.reconciliations_withheld,
            "accepted-insecure": self.accepted_insecure_count,
        })
    }

    /// The C187 acceptance record, or `None` when the build accepted nothing.
    ///
    /// Absent rather than a "false" value: a record saying "accepted: 0"
    /// invites reading as a clean bill of health, and FR-016a is explicit
    /// that absence must not mean rejection.
    pub fn acceptance(&self) -> Option<String> {
        (self.accepted_insecure_count > 0)
            .then(|| acceptance_record(self.accepted_insecure_count))
    }

    /// The C189 grade, or `None` when no CVE was recovered.
    ///
    /// A grade attached to nothing would assert a standard of evidence for
    /// claims that do not exist — the same rule milestone 1035 applies.
    pub fn grade(&self) -> Option<&'static str> {
        use crate::scan_fs::package_db::nix::closure::patches::EvidenceGrade;
        (self.distinct_cves > 0).then(|| EvidenceGrade::NixpkgsDeclared.wire())
    }

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
                    if !declarations.is_empty() {
                        s.accepted_insecure_count += 1;
                    }
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

/// C188 — what the declaration pass saw.
pub const ANN_NIXPKGS_SECURITY: &str = "waybill:nixpkgs-security";

/// C189 — how CVE associations from declarations were established.
pub const ANN_DECLARATION_GRADE: &str = "waybill:nixpkgs-declaration-grade";

/// C186 — what nixpkgs says about a component, where it named no CVE.
///
/// A composition fact, not a match result. "Vendors Electron 2.0" and
/// "Includes vulnerable versions of bundled libraries: openssl, ffmpeg, gdal,
/// and proj" each say the same thing: there are components inside this one
/// that the SBOM does not list. That is nixpkgs reporting the component graph
/// is incomplete, which is what an SBOM is for — so it lands here rather than
/// in VEX, and no identifier has to be invented to carry it.
pub const ANN_NIXPKGS_DECLARATION: &str = "waybill:nixpkgs-declaration";

/// Stamp prose declarations onto the components they are about (FR-010).
///
/// Only the entries naming no CVE. The CVE-bearing ones travel to VEX
/// through [`DeclaredFinding`] and deliberately never touch this bag, which
/// auto-flows into SBOM properties — a CVE claim in an SBOM is what this
/// milestone exists not to emit.
///
/// The text is carried verbatim. These entries are worth emitting *because*
/// of what the maintainer wrote; reducing one to a flag would keep the fact
/// that something is wrong and discard what it is.
pub(crate) fn annotate_prose(
    components: &mut [waybill_common::resolution::ResolvedComponent],
    resolutions: &std::collections::BTreeMap<String, AttributeResolution>,
) -> usize {
    let mut stamped = 0;
    for c in components.iter_mut() {
        let Some(AttributeResolution::Confirmed { declarations, .. }) = resolutions.get(&c.name)
        else {
            continue;
        };
        let prose: Vec<&str> = declarations
            .iter()
            .filter(|d| !d.names_a_cve())
            .map(|d| d.text.as_str())
            .collect();
        if prose.is_empty() {
            continue;
        }
        // An array, JSON-encoded as a string: the m134 / m147 / m173
        // convention for array-valued annotations, and a component can carry
        // several (four measured packages declare a vendored EOL Electron
        // alongside other entries).
        let Ok(encoded) = serde_json::to_string(&prose) else {
            continue;
        };
        c.extra_annotations.insert(
            ANN_NIXPKGS_DECLARATION.to_string(),
            serde_json::Value::String(encoded),
        );
        stamped += 1;
    }
    stamped
}

/// C187 — that this build contains a package nixpkgs marks insecure.
pub const ANN_ACCEPTED_INSECURE: &str = "waybill:nixpkgs-accepted-insecure";

/// The wording of the acceptance record (FR-015, FR-016).
///
/// States a property of the **build**, never of the operator's intent. Nix
/// refuses to evaluate a package marked insecure unless it was permitted, so
/// the package's presence proves permission was granted — but
/// `NIXPKGS_ALLOW_INSECURE=1` permits everything at once and is
/// indistinguishable from a targeted entry in `permittedInsecurePackages`.
/// "This build accepted one" is supportable; "the operator chose this
/// package" is not, and the difference matters to anyone reading the record
/// as evidence of a decision.
pub(crate) fn acceptance_record(count: usize) -> String {
    format!(
        "This build contains {count} package(s) that nixpkgs marks insecure, \
         and therefore permitted them: Nix refuses to evaluate such a package \
         otherwise. Whether the permission was targeted or blanket \
         (NIXPKGS_ALLOW_INSECURE) is not observable here. Absence of this \
         record means no such package was found, not that one was rejected."
    )
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
    use crate::scan_fs::package_db::nix::closure::patches::EvidenceGrade;

    // The rule derives from the grade ordering rather than restating it. If
    // a third provenance is ever added, the reconciliation follows without
    // anyone remembering to update it here — and if the ordering is ever
    // changed, this changes with it instead of silently disagreeing.
    if !EvidenceGrade::NixpkgsDeclared.outranks(EvidenceGrade::FilenameDerived) {
        return Default::default();
    }

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
) -> Result<
    (
        NixpkgsSecuritySummary,
        std::collections::BTreeMap<String, AttributeResolution>,
    ),
    crate::scan_fs::package_db::nix::eval::reason::DegradationReason,
> {
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
        return Ok((NixpkgsSecuritySummary::default(), Default::default()));
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
    let summary = NixpkgsSecuritySummary::build_with_purls(&resolutions, purl_of);
    // Returned rather than stamped here: the prose annotation mutates the
    // component set, and doing that inside a function whose job is to *ask*
    // nixpkgs a question would hide a write behind a read.
    Ok((summary, resolutions))
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

    /// Build a component the way the closure emitter does, then rename it.
    ///
    /// Reuses that path rather than hand-listing forty fields, so a field
    /// added there cannot leave this helper silently stale.
    fn component(name: &str, purl: &str) -> waybill_common::resolution::ResolvedComponent {
        use crate::scan_fs::package_db::nix::closure::{
            classify::classify, derivation::RawClosure, emit, ClassifiedClosure,
        };
        let raw = RawClosure::parse(
            r#"{"derivations":{"d-a.drv":{"env":{"pname":"seed","version":"1.0",
                 "buildInputs":"/nix/store/o1"},"outputs":{"out":{"path":"o2"}}},
               "d-b.drv":{"env":{"pname":"dep","version":"1.0"},
                 "outputs":{"out":{"path":"o1"}}}},"version":3}"#,
        )
        .expect("seed closure parses");
        let roles = classify(&raw);
        let mut c = emit::components(&ClassifiedClosure {
            attribute: "default".into(),
            raw,
            roles,
        })
        .pop()
        .expect("the seed closure yields a component");
        c.name = name.to_string();
        c.purl = waybill_common::types::purl::Purl::new(purl).expect("valid purl");
        c.extra_annotations.clear();
        c
    }

    fn confirmed(texts: &[&str]) -> AttributeResolution {
        AttributeResolution::Confirmed {
            source: DeclarationSource::TopLevel,
            declarations: texts.iter().map(|t| parse::Declaration::parse(t)).collect(),
        }
    }

    #[test]
    fn a_prose_declaration_reaches_the_component_with_its_text_intact() {
        // FR-010a. These entries are worth emitting *because* of what the
        // maintainer wrote — reducing one to a flag keeps the fact that
        // something is wrong and discards what it is.
        let text = "Includes vulnerable versions of bundled libraries: \
                    openssl, ffmpeg, gdal, and proj.";
        let mut comps = vec![component("bundler", "pkg:generic/bundler@1.0")];
        let res = [("bundler".to_string(), confirmed(&[text]))]
            .into_iter()
            .collect();

        assert_eq!(annotate_prose(&mut comps, &res), 1);
        let raw = comps[0].extra_annotations[ANN_NIXPKGS_DECLARATION]
            .as_str()
            .expect("encoded as a string");
        let got: Vec<String> = serde_json::from_str(raw).expect("decodes as an array");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0], text, "the text must survive verbatim");
        assert!(got[0].contains("openssl"), "the named components are the point");
    }

    #[test]
    fn a_cve_bearing_declaration_never_reaches_the_component() {
        // The SBOM/VEX boundary. This bag auto-flows into SBOM properties,
        // and a CVE claim in an SBOM is what this milestone exists not to
        // emit. CVE-bearing entries travel to VEX through DeclaredFinding.
        let mut comps = vec![component("cved", "pkg:generic/cved@1.0")];
        let res = [(
            "cved".to_string(),
            confirmed(&["CVE-2099-0001: synthetic entry"]),
        )]
        .into_iter()
        .collect();

        assert_eq!(annotate_prose(&mut comps, &res), 0);
        assert!(
            !comps[0].extra_annotations.contains_key(ANN_NIXPKGS_DECLARATION),
            "a CVE-bearing declaration must not land in the SBOM"
        );
    }

    #[test]
    fn a_component_with_both_kinds_emits_the_prose_and_withholds_the_cve() {
        // Neither displaces the other: the prose reaches the SBOM, the CVE
        // reaches VEX, and each goes to exactly one place.
        let mut comps = vec![component("both", "pkg:generic/both@1.0")];
        let res = [(
            "both".to_string(),
            confirmed(&["CVE-2099-0001: named", "Vendors an end-of-life component"]),
        )]
        .into_iter()
        .collect();

        assert_eq!(annotate_prose(&mut comps, &res), 1);
        let raw = comps[0].extra_annotations[ANN_NIXPKGS_DECLARATION]
            .as_str()
            .unwrap();
        let got: Vec<String> = serde_json::from_str(raw).unwrap();
        assert_eq!(got.len(), 1, "only the prose entry: {got:?}");
        assert!(got[0].contains("end-of-life"));
        assert!(
            !raw.contains("CVE-2099-0001"),
            "the identifier must not leak into the SBOM: {raw}"
        );
    }

    #[test]
    fn an_unchecked_member_is_never_annotated() {
        // "We could not ask" must not be emitted as "nixpkgs said this".
        let mut comps = vec![component("unknown", "pkg:generic/unknown@1.0")];
        for r in [AttributeResolution::PathMismatch, AttributeResolution::NoAttribute] {
            let res = [("unknown".to_string(), r)].into_iter().collect();
            assert_eq!(annotate_prose(&mut comps, &res), 0);
            assert!(!comps[0].extra_annotations.contains_key(ANN_NIXPKGS_DECLARATION));
        }
    }

    #[test]
    fn the_no_cve_count_reaches_the_summary() {
        // FR-011. Without it a consumer seeing few VEX statements cannot
        // tell whether nixpkgs said little, or whether most of what it said
        // had no identifier to hang a statement on.
        let res: std::collections::BTreeMap<String, AttributeResolution> = [
            ("a".to_string(), confirmed(&["CVE-2099-0001: named", "prose one"])),
            ("b".to_string(), confirmed(&["prose two"])),
        ]
        .into_iter()
        .collect();
        let s = NixpkgsSecuritySummary::build(&res);
        assert_eq!(s.declarations_total, 3);
        assert_eq!(s.declarations_without_cve, 2);
        assert_eq!(s.distinct_cves, 1);
    }

    #[test]
    fn the_acceptance_record_claims_only_what_presence_supports() {
        // FR-016. Presence proves permission was granted, because Nix will
        // not evaluate the package otherwise. It does NOT prove the operator
        // named that package -- NIXPKGS_ALLOW_INSECURE permits everything at
        // once and is indistinguishable from a targeted entry.
        let r = acceptance_record(2);
        assert!(r.contains("This build contains"), "{r}");
        assert!(
            r.contains("not observable"),
            "the blanket-vs-targeted ambiguity must be stated, not glossed: {r}"
        );
        for forbidden in ["the operator chose", "explicitly listed", "deliberately selected"] {
            assert!(
                !r.contains(forbidden),
                "claims intent the evidence does not support ({forbidden}): {r}"
            );
        }
    }

    #[test]
    fn the_acceptance_record_says_absence_is_not_rejection() {
        // FR-016a. A build with no insecure packages and a build that was
        // never asked look identical from outside. Without this sentence a
        // reader can take a missing record as a clean bill of health.
        let r = acceptance_record(1);
        assert!(
            r.contains("Absence of this record means no such package was found"),
            "absence must not read as rejection: {r}"
        );
    }

    #[test]
    fn the_acceptance_count_distinguishes_one_exception_from_many() {
        // Coarse by design (FR-015a), but one accepted package and twenty
        // are different risk postures and the record should not flatten them.
        let one: std::collections::BTreeMap<String, AttributeResolution> =
            [("a".to_string(), confirmed(&["CVE-2099-0001: x"]))]
                .into_iter()
                .collect();
        let many: std::collections::BTreeMap<String, AttributeResolution> = [
            ("a".to_string(), confirmed(&["CVE-2099-0001: x"])),
            ("b".to_string(), confirmed(&["prose"])),
            ("c".to_string(), confirmed(&[])),
        ]
        .into_iter()
        .collect();
        assert_eq!(NixpkgsSecuritySummary::build(&one).accepted_insecure_count, 1);
        // `c` declares nothing, so it is not an accepted exception.
        assert_eq!(NixpkgsSecuritySummary::build(&many).accepted_insecure_count, 2);
    }

    #[test]
    fn a_build_with_no_declarations_records_no_acceptance() {
        // The common case, and it must stay silent: a project that builds has
        // already permitted whatever it contains, so most scans find nothing
        // and that is not a failure.
        let res: std::collections::BTreeMap<String, AttributeResolution> =
            [("clean".to_string(), confirmed(&[]))].into_iter().collect();
        let s = NixpkgsSecuritySummary::build(&res);
        assert!(!s.accepted_insecure);
        assert_eq!(s.accepted_insecure_count, 0);
        // CONTROL: the member WAS checked, so this is "nixpkgs said nothing"
        // rather than "we could not ask".
        assert_eq!(s.members_checked, 1);
        assert_eq!(s.members_unchecked, 0);
    }

    #[test]
    fn the_record_leads_with_coverage_and_carries_every_count() {
        // T048's half that can be asserted without a document: the wire
        // value carries all seven figures. A record missing one is a reader
        // who cannot interpret the rest — coverage bounds everything after
        // it, and the common case for this feature is finding nothing.
        let res: std::collections::BTreeMap<String, AttributeResolution> = [
            ("a".to_string(), confirmed(&["CVE-2099-0001: x", "prose"])),
            ("b".to_string(), AttributeResolution::PathMismatch),
            ("c".to_string(), AttributeResolution::NoAttribute),
        ]
        .into_iter()
        .collect();
        let mut s = NixpkgsSecuritySummary::build(&res);
        s.reconciliations_withheld = 1;

        let v = s.value();
        assert_eq!(v["members-checked"], 1);
        assert_eq!(v["members-unchecked"], 2, "both unchecked kinds counted");
        assert_eq!(v["declarations"], 2);
        assert_eq!(v["declarations-without-cve"], 1);
        assert_eq!(v["distinct-cves"], 1);
        assert_eq!(v["reconciliations-withheld"], 1);
        assert_eq!(v["accepted-insecure"], 1);
        assert!(v["confirmed-by-set"].is_object());

        // The wire form is a string, because a CycloneDX property value must
        // be one. If SPDX carried an object instead, the parity extractor
        // would have to reconcile a shape difference that is not a real
        // difference.
        assert_eq!(s.wire(), v.to_string());
    }

    #[test]
    fn the_acceptance_record_and_grade_are_absent_rather_than_empty() {
        // A property saying "accepted: 0" invites reading as a clean bill of
        // health, and FR-016a is explicit that absence must not mean
        // rejection. A grade attached to no claims asserts a standard of
        // evidence for claims that do not exist.
        let clean: std::collections::BTreeMap<String, AttributeResolution> =
            [("c".to_string(), confirmed(&[]))].into_iter().collect();
        let s = NixpkgsSecuritySummary::build(&clean);
        assert!(s.acceptance().is_none());
        assert!(s.grade().is_none());
        // CONTROL: the member was checked, so this is "nixpkgs said nothing"
        // and not "the pass did not run".
        assert_eq!(s.members_checked, 1);

        let prose_only: std::collections::BTreeMap<String, AttributeResolution> =
            [("p".to_string(), confirmed(&["vendors something EOL"]))]
                .into_iter()
                .collect();
        let s = NixpkgsSecuritySummary::build(&prose_only);
        assert!(s.acceptance().is_some(), "a prose declaration is still an exception");
        assert!(
            s.grade().is_none(),
            "no CVE was recovered, so no grade is warranted"
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
