//! Attributing patches to the components that apply them, and grading the
//! CVE identifiers that attribution recovers.
//!
//! nixpkgs backports security fixes without moving a version string. `unzip`
//! in both measured closures is version 6.0 — unchanged since 2009 — and
//! carries 11 CVEs across 26 patches. No version-keyed SBOM can express that
//! in either direction: not "this build is patched", and not "this version
//! was considered vulnerable".
//!
//! The join is mechanical, not heuristic: a derivation's own `patches` field
//! lists the store paths it applies.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use regex::Regex;

use super::derivation::{store_basename, RawClosure};

/// How a CVE identifier came to be associated with a patch.
///
/// A single-variant enum, deliberately. Not a `bool` and not an `Option`,
/// because the point is that a *future* stronger provenance — a patch header,
/// an upstream mapping — must be distinguishable from this one. A consumer
/// reading the grade today keeps working when a second variant appears;
/// `Option<CveId>` would record the association and lose how it was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum EvidenceGrade {
    /// Parsed from the patch derivation's filename. The only grade v1
    /// produces, and weaker than it looks: a filename is not proof the patch
    /// fully resolves the issue, and a backport that does not name a CVE is
    /// invisible.
    FilenameDerived,
}

impl EvidenceGrade {
    pub(crate) fn wire(self) -> &'static str {
        match self {
            Self::FilenameDerived => "filename-derived",
        }
    }
}

/// A CVE identifier together with how it was established.
///
/// The grade is a field, not an `Option`, and there is no constructor that
/// omits it. That is how FR-012a is enforced — by construction rather than by
/// a check somebody can forget to call.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct GradedCve {
    pub(crate) id: String,
    pub(crate) grade: EvidenceGrade,
}

impl GradedCve {
    /// The only way to build one, and it fixes the grade to match how the id
    /// was actually obtained.
    fn from_filename(id: String) -> Self {
        Self {
            id,
            grade: EvidenceGrade::FilenameDerived,
        }
    }
}

/// One patch applied by one component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PatchRecord {
    /// The patch derivation's basename, e.g. `CVE-2019-13232-1.patch`.
    pub(crate) name: String,
    /// CVEs named by that basename. Usually zero — measured, 89% and 91% of patches
    /// in a real closure name none.
    pub(crate) resolves: Vec<GradedCve>,
}

/// Every patch a single component applies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ComponentPatches {
    pub(crate) component: String,
    pub(crate) version: Option<String>,
    pub(crate) patches: Vec<PatchRecord>,
}

impl ComponentPatches {
    pub(crate) fn distinct_cves(&self) -> BTreeSet<&str> {
        self.patches
            .iter()
            .flat_map(|p| p.resolves.iter())
            .map(|c| c.id.as_str())
            .collect()
    }
}

fn cve_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"CVE-\d{4}-\d+").expect("static CVE pattern is valid")
    })
}

/// CVE identifiers named by a patch filename.
pub(crate) fn cves_in_filename(name: &str) -> Vec<GradedCve> {
    let mut seen = BTreeSet::new();
    cve_pattern()
        .find_iter(name)
        .map(|m| m.as_str().to_string())
        .filter(|id| seen.insert(id.clone()))
        .map(GradedCve::from_filename)
        .collect()
}

/// Attribute every patch in the closure to the component applying it.
///
/// Reads `env.patches`, which lists store paths. Scanning derivation *names*
/// for CVE-shaped strings instead recovers roughly a fifth as many: measured,
/// 3 and 4 against 18 and 14 on the same two closures, because many patches
/// are files referenced by path rather than separate CVE-named derivations.
/// The undercount is silent, which is why it is called out here.
pub(crate) fn attribute(closure: &RawClosure) -> Vec<ComponentPatches> {
    let index = closure.output_index();
    let mut out = Vec::new();

    for drv in closure.derivations.values() {
        let paths: Vec<&str> = drv.paths_in("patches").collect();
        if paths.is_empty() {
            continue;
        }
        let Some(component) = drv.pname() else {
            continue;
        };
        let patches: Vec<PatchRecord> = paths
            .iter()
            .map(|path| {
                let base = store_basename(path);
                // Prefer the producing derivation's name when the path
                // resolves to one; fall back to the basename, which carries
                // the CVE either way. Both forms occur in real closures.
                let name = index
                    .get(base)
                    .and_then(|key| closure.derivations.get(*key))
                    .and_then(|d| d.pname())
                    .unwrap_or(base);
                PatchRecord {
                    name: name.to_string(),
                    resolves: cves_in_filename(name),
                }
            })
            .collect();
        out.push(ComponentPatches {
            component: component.to_string(),
            version: drv.version().map(str::to_string),
            patches,
        });
    }
    merge_by_identity(out)
}

/// Collapse entries for the same `(component, version)`.
///
/// A closure can hold several derivations with one pname — measured, `unzip
/// 6.0` appears twice in a real closure as two build variants, each applying
/// the same 17 patches. Emitted as-is that becomes duplicate `pedigree`
/// entries on one component.
///
/// The patch sets are unioned rather than one being dropped: the variants
/// happened to agree in the measured case, but nothing guarantees that, and
/// silently keeping whichever came first would lose the difference.
fn merge_by_identity(all: Vec<ComponentPatches>) -> Vec<ComponentPatches> {
    use std::collections::BTreeMap;
    let mut merged: BTreeMap<(String, Option<String>), Vec<PatchRecord>> = BTreeMap::new();
    for cp in all {
        merged
            .entry((cp.component, cp.version))
            .or_default()
            .extend(cp.patches);
    }
    merged
        .into_iter()
        .map(|((component, version), mut patches)| {
            patches.sort_by(|a, b| a.name.cmp(&b.name));
            patches.dedup_by(|a, b| a.name == b.name);
            ComponentPatches {
                component,
                version,
                patches,
            }
        })
        .collect()
}

/// Totals for the document-scope record (FR-010, SC-006b).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PatchTotals {
    pub(crate) patches: usize,
    /// Patches naming no CVE. Emitted because absence of a VEX statement
    /// would otherwise read as absence of a backport — measured, that is the
    /// overwhelming majority.
    pub(crate) without_cve: usize,
    pub(crate) distinct_cves: usize,
}

pub(crate) fn totals(all: &[ComponentPatches]) -> PatchTotals {
    let mut t = PatchTotals::default();
    let mut cves = BTreeSet::new();
    for cp in all {
        for p in &cp.patches {
            t.patches += 1;
            if p.resolves.is_empty() {
                t.without_cve += 1;
            }
        }
        cves.extend(cp.distinct_cves().into_iter().map(str::to_string));
    }
    t.distinct_cves = cves.len();
    t
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    const CLOSURE: &str = r#"{
      "derivations": {
        "d-p1.drv": { "env": {"pname":"CVE-2019-13232-1.patch"}, "outputs":{"out":{"path":"h1-p1"}} },
        "d-p2.drv": { "env": {"pname":"gcc-15.patch"},           "outputs":{"out":{"path":"h2-p2"}} },
        "d-unzip.drv": {
          "env": { "pname":"unzip", "version":"6.0",
                   "patches":"/nix/store/h1-p1 /nix/store/h2-p2 /nix/store/h3-CVE-2021-4217.patch" },
          "outputs":{"out":{"path":"h9-unzip"}}
        },
        "d-clean.drv": { "env": {"pname":"clean","version":"1.0"}, "outputs":{"out":{"path":"h8-clean"}} }
      },
      "version": 3
    }"#;

    fn attributed() -> Vec<ComponentPatches> {
        attribute(&RawClosure::parse(CLOSURE).unwrap())
    }

    #[test]
    fn a_cve_association_cannot_exist_without_its_grade() {
        // FR-012a by construction: `GradedCve` has no constructor that omits
        // the grade, so there is no ungraded association to emit. If this
        // stops compiling because a grade-less constructor appeared, that is
        // the regression.
        let cve = cves_in_filename("CVE-2019-13232-1.patch");
        assert_eq!(cve.len(), 1);
        assert_eq!(cve[0].grade, EvidenceGrade::FilenameDerived);
        assert_eq!(cve[0].grade.wire(), "filename-derived");
    }

    #[test]
    fn patches_attribute_to_the_component_that_applies_them() {
        let all = attributed();
        // CONTROL: exactly one component applies patches here, so the
        // assertions below are not passing over an empty vector.
        assert_eq!(all.len(), 1, "got {:?}", all.iter().map(|c| &c.component).collect::<Vec<_>>());
        let unzip = &all[0];
        assert_eq!(unzip.component, "unzip");
        assert_eq!(unzip.version.as_deref(), Some("6.0"));
        assert_eq!(unzip.patches.len(), 3);
    }

    #[test]
    fn recovers_cves_from_both_resolvable_and_bare_patch_paths() {
        // `h1-p1` resolves to a derivation whose name carries the CVE;
        // `h3-CVE-2021-4217.patch` does not resolve and falls back to its
        // basename. Real closures contain both, and a reader handling only
        // the first undercounts by roughly fivefold.
        let all = attributed();
        let cves = all[0].distinct_cves();
        assert!(cves.contains("CVE-2019-13232"), "resolvable path: {cves:?}");
        assert!(cves.contains("CVE-2021-4217"), "bare basename: {cves:?}");
        assert_eq!(cves.len(), 2);
    }

    #[test]
    fn a_patch_naming_no_cve_is_still_recorded() {
        // Dropping it would make partial coverage look like absence.
        let all = attributed();
        let plain = all[0].patches.iter().find(|p| p.name.contains("gcc-15")).unwrap();
        assert!(plain.resolves.is_empty());
    }

    #[test]
    fn totals_expose_how_much_is_uncovered() {
        let t = totals(&attributed());
        assert_eq!(t.patches, 3);
        assert_eq!(t.without_cve, 1);
        assert_eq!(t.distinct_cves, 2);
    }

    /// Cross-check against a real closure, when one is supplied.
    ///
    /// The fixture above is synthetic. The reference figures come from the
    /// committed Python probe at
    /// `specs/1035-nix-closure-sbom/measurements/patch-attribution.py`,
    /// which found 18 and 14 distinct CVEs on two real projects. A run
    /// recovering 3 or 4 means this is scanning derivation names rather than
    /// joining through `env.patches` — a silent fivefold undercount, and the
    /// specific failure SC-006a exists to catch.
    #[test]
    fn recovers_what_the_python_probe_found_on_a_real_closure() {
        let Ok(path) = std::env::var("WAYBILL_TEST_CLOSURE_JSON") else {
            eprintln!("skipping: set WAYBILL_TEST_CLOSURE_JSON to a closure dump");
            return;
        };
        let json = std::fs::read_to_string(&path).expect("closure json");
        let closure = RawClosure::parse(&json).expect("parses");
        let all = attribute(&closure);
        let t = totals(&all);
        eprintln!(
            "{path}: {} components apply patches; {} patches, {} naming no CVE, \
             {} distinct CVEs",
            all.len(),
            t.patches,
            t.without_cve,
            t.distinct_cves
        );
        for cp in &all {
            let cves = cp.distinct_cves();
            if !cves.is_empty() {
                eprintln!("    {} {:?} -> {:?}", cp.component, cp.version, cves);
            }
        }
        assert!(all.len() > 10, "only {} components apply patches", all.len());
        assert!(
            t.distinct_cves >= 10,
            "only {} distinct CVEs — a name scan recovers about this many, \
             the env.patches join recovers ~5x more",
            t.distinct_cves
        );
    }

    #[test]
    fn build_variants_of_one_component_merge_rather_than_duplicate() {
        // A closure can hold several derivations with one pname. Measured:
        // `unzip 6.0` appears twice as two build variants, each applying the
        // same 17 patches. Emitted as-is that is duplicate pedigree entries.
        let closure = r#"{
          "derivations": {
            "v1.drv": { "env": {"pname":"dup","version":"1.0",
                                "patches":"/nix/store/h-CVE-2020-1111.patch"},
                        "outputs":{"out":{"path":"o1"}} },
            "v2.drv": { "env": {"pname":"dup","version":"1.0",
                                "patches":"/nix/store/h-CVE-2020-1111.patch /nix/store/h-extra.patch"},
                        "outputs":{"out":{"path":"o2"}} }
          },
          "version": 3
        }"#;
        let all = attribute(&RawClosure::parse(closure).unwrap());
        assert_eq!(all.len(), 1, "two variants must collapse to one component");
        // Union, not first-wins: the second variant's extra patch survives.
        assert_eq!(all[0].patches.len(), 2, "{:?}", all[0].patches);
        assert_eq!(all[0].distinct_cves().len(), 1, "the shared CVE is not doubled");
    }

    #[test]
    fn a_component_applying_no_patches_is_absent_rather_than_empty() {
        assert!(!attributed().iter().any(|c| c.component == "clean"));
    }
}
