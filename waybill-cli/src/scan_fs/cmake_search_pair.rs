//! Spec 1614 (#1591, #1605), research R7.2, R7.3, R9, R10: a CMake package
//! search for something the scan already represents is not a second
//! component.
//!
//! Runs on the resolved components, after `installed_lib::resolve`, before
//! deduplication: every package-search entry is still one per call site.
//! In order, for each searched name:
//!
//! 1. **An installed-library record** (C14 `cmake-package-version` /
//!    `pkg-config`) of the same lowercased name: the search entries go, their
//!    files join the record's evidence, and each becomes a [`Pairing`].
//! 2. **A git submodule** whose directory's last segment is the name (C57's
//!    rule): the same.
//! 3. **An in-tree CMake project component** (spec 1618, #1616): the same;
//!    counted in C237 `paired.project`. A project whose name an installed
//!    record or a submodule claims is not a component: it goes first, and
//!    its file joins that component's evidence (FR-005).
//! 4. **A name the tree builds itself** with no project component (a
//!    package-config template, a provided config file): the entries go,
//!    with no edge, and the name is listed in C237 `in-tree`. This comes last
//!    because a submodule is usually also a CMake project (analysis I2).
//!
//! A fetch of the same name was already paired in the reader (R7.1). C57 is
//! widened here too: a submodule any package search names is
//! `declared-and-used`.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

use waybill_common::resolution::{EnrichmentProvenance, LifecycleScope, Relationship, RelationshipType, ResolvedComponent};

use super::declared_attach::{relative, DeclaredAttachment, OwnerIndex};
use super::package_db::cmake::searches::{FindCoverage, ANN_CMAKE_FIND_CONDITIONS};
use super::package_db::git_submodule::SubmoduleDirs;

type Annotations = BTreeMap<String, serde_json::Value>;

const MECHANISM: &str = "waybill:source-mechanism";
const ANN_BUILD_REFERENCE: &str = "waybill:build-reference";
const ANN_OPTIONAL_DERIVATION: &str = "waybill:optional-derivation";
const DERIVATION_CMAKE_CONDITIONAL: &str = "cmake-conditional";
/// C62, as the package-search reader writes it (`cmake/searches.rs`).
const ANN_SCOPE_DERIVATION: &str = "waybill:lifecycle-scope-derivation";
const DERIVATION_SOURCE_TEST_DIRECTORY: &str = "source-test-directory";
const DERIVATION_CMAKE_BUILD_TESTING: &str = "cmake-build-testing";

/// One package search answered by an existing component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pairing {
    /// The declaring file, as the search entry recorded it.
    pub(crate) file: String,
    /// The component the search stands for.
    pub(crate) target: String,
    /// Whether every search of the name is non-runtime, and their conditions.
    pub(crate) conditional: bool,
    pub(crate) conditions: BTreeSet<String>,
    /// Spec 1617 (#1623) research R4: the search's C62 when it is Test-scoped
    /// (`source-test-directory`, `cmake-build-testing`); `None` otherwise.
    pub(crate) test_derivation: Option<String>,
    /// An installed-library record, whose scope the searches can decide.
    pub(crate) installed: bool,
    /// Spec 1618: an in-tree project, whose scope its directory decides;
    /// searches add only conditions, or Test when all are from tests.
    pub(crate) project: bool,
}

fn is_cmake_project(c: &ResolvedComponent) -> bool {
    super::package_db::cmake::projects::is_cmake_project(c)
}

/// Spec 1618 (#1616) FR-006, research R4: an edge to each project component
/// nothing else reaches, from what encloses its directory, or a root anchor
/// when nothing does. A project a search edge or a search's root anchor
/// already reaches needs none.
pub(crate) fn enclose_projects(
    components: &[ResolvedComponent],
    relationships: &[Relationship],
    reached: &BTreeSet<String>,
    index: &OwnerIndex,
    scan_root: &Path,
) -> DeclaredAttachment {
    let canonical_root = std::fs::canonicalize(scan_root).unwrap_or_else(|_| scan_root.to_path_buf());
    let targeted: HashSet<&str> = relationships.iter().map(|r| r.to.as_str()).collect();
    let mut seen: HashSet<(String, String)> = relationships.iter().map(|r| (r.from.clone(), r.to.clone())).collect();
    let mut out = DeclaredAttachment::default();
    for c in components.iter().filter(|c| is_cmake_project(c)) {
        let purl = c.purl.as_str();
        if targeted.contains(purl) || reached.contains(purl) {
            continue;
        }
        let dirs: BTreeSet<std::path::PathBuf> = c
            .evidence
            .source_file_paths
            .iter()
            .filter(|f| Path::new(f).file_name().is_some_and(|n| n == "CMakeLists.txt"))
            .filter_map(|f| relative(f, scan_root, &canonical_root))
            .map(|r| r.parent().map(Path::to_path_buf).unwrap_or_default())
            .collect();
        let mut enclosed = false;
        for dir in &dirs {
            if let Some(owner) = index.enclosing(dir, purl) {
                enclosed = true;
                if seen.insert((owner.to_string(), purl.to_string())) {
                    out.edges.push(Relationship {
                        from: owner.to_string(),
                        to: purl.to_string(),
                        relationship_type: RelationshipType::DependsOn,
                        provenance: EnrichmentProvenance {
                            source: dir.to_string_lossy().into_owned(),
                            data_type: "cmake-project-enclosure".to_string(),
                        },
                    });
                }
            }
        }
        if !enclosed {
            out.root_anchors.insert(purl.to_string());
        }
    }
    out
}

fn mechanism(c: &ResolvedComponent) -> Option<&str> {
    c.extra_annotations.get(MECHANISM).and_then(|v| v.as_str())
}

fn is_search(c: &ResolvedComponent) -> bool {
    matches!(mechanism(c), Some("cmake-find-package" | "cmake-pkg-check-modules"))
}

fn test_derivation_of(c: &ResolvedComponent) -> Option<String> {
    if c.lifecycle_scope != Some(LifecycleScope::Test) {
        return None;
    }
    c.extra_annotations.get(ANN_SCOPE_DERIVATION).and_then(|v| v.as_str()).map(str::to_string)
}

/// Spec 1617 (#1623) FR-001: whether `c`'s own evidence lists a library file
/// it ships: `.so`, `.so.<N…>`, `.a`, `.dylib`, `.dll` or `.lib`, compared
/// case-insensitively. `installed_lib::resolve` has already attributed these
/// files to the record (research R2).
pub(crate) fn ships_library_file(c: &ResolvedComponent) -> bool {
    c.evidence.source_file_paths.iter().any(|p| {
        let name = Path::new(p).file_name().and_then(|n| n.to_str()).unwrap_or("").to_ascii_lowercase();
        if [".so", ".a", ".dylib", ".dll", ".lib"].iter().any(|ext| name.len() > ext.len() && name.ends_with(ext)) {
            return true;
        }
        name.split_once(".so.").is_some_and(|(stem, v)| {
            !stem.is_empty() && v.split('.').all(|seg| !seg.is_empty() && seg.chars().all(|ch| ch.is_ascii_digit()))
        })
    })
}

fn conditions_of(c: &ResolvedComponent) -> BTreeSet<String> {
    c.extra_annotations
        .get(ANN_CMAKE_FIND_CONDITIONS)
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

/// Pair, drop and count, as the module describes. Returns the pairings for
/// [`attach`] and [`record_scopes`].
pub(crate) fn pair(
    components: &mut Vec<ResolvedComponent>,
    submodule_dirs: &SubmoduleDirs,
    cov: Option<&mut FindCoverage>,
) -> Vec<Pairing> {
    let mut records: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for c in components.iter() {
        if matches!(mechanism(c), Some("cmake-package-version" | "pkg-config")) {
            records.entry(c.name.to_lowercase()).or_default().push(c.purl.as_str().to_string());
        }
    }
    let submodules: BTreeMap<String, String> = submodule_dirs
        .iter()
        .filter_map(|(dir, purl)| Some((dir.file_name()?.to_string_lossy().to_lowercase(), purl.clone())))
        .collect();

    // Spec 1618 FR-005: a project named like an installed record or a
    // submodule is that component; its declaring files join its evidence.
    // Before the early return: a tree with no package search still has
    // projects.
    let mut claimed: BTreeMap<String, Vec<String>> = BTreeMap::new();
    // Spec 1639 S8: the absorbed projects' annotations, by owner.
    let mut absorbed_projects: BTreeMap<String, Vec<Annotations>> = BTreeMap::new();
    components.retain(|c| {
        if !is_cmake_project(c) {
            return true;
        }
        let name = c.name.to_lowercase();
        let owners: Vec<String> = match (records.get(&name), submodules.get(&name)) {
            (Some(r), _) => r.clone(),
            (None, Some(s)) => vec![s.clone()],
            (None, None) => return true,
        };
        for o in owners {
            absorbed_projects.entry(o.clone()).or_default().push(c.extra_annotations.clone());
            claimed.entry(o).or_default().extend(c.evidence.source_file_paths.iter().cloned());
        }
        false
    });
    for c in components.iter_mut() {
        for a in absorbed_projects.get(c.purl.as_str()).into_iter().flatten() {
            super::also_detected::record("S8", c.purl.as_str(), &mut c.extra_annotations, a);
        }
        if let Some(files) = claimed.get(c.purl.as_str()) {
            for f in files {
                if !c.evidence.source_file_paths.contains(f) {
                    c.evidence.source_file_paths.push(f.clone());
                }
            }
        }
    }
    let Some(cov) = cov else { return Vec::new() };
    let mut projects: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for c in components.iter().filter(|c| is_cmake_project(c)) {
        projects.entry(c.name.to_lowercase()).or_default().push(c.purl.as_str().to_string());
    }

    // R10: C57 sees every searched name.
    for c in components.iter_mut() {
        if mechanism(c) != Some("git-submodule") {
            continue;
        }
        let used = submodule_dirs
            .iter()
            .find(|(_, p)| *p == c.purl.as_str())
            .and_then(|(d, _)| d.file_name())
            .is_some_and(|n| cov.searched.contains(&n.to_string_lossy().to_lowercase()));
        if used {
            c.extra_annotations.insert(ANN_BUILD_REFERENCE.to_string(), serde_json::json!("declared-and-used"));
        }
    }

    let mut pairings = Vec::new();
    let mut evidence: BTreeMap<String, Vec<String>> = BTreeMap::new();
    // Spec 1639 S9: the paired searches' annotations, by target.
    let mut absorbed_searches: BTreeMap<String, Vec<Annotations>> = BTreeMap::new();
    let mut gone: BTreeSet<String> = BTreeSet::new();
    let mut outcome: BTreeMap<String, &'static str> = BTreeMap::new();
    for c in components.iter().filter(|c| is_search(c)) {
        let name = c.name.to_lowercase();
        let (targets, installed, project): (Vec<String>, bool, bool) = if let Some(r) = records.get(&name) {
            (r.clone(), true, false)
        } else if let Some(s) = submodules.get(&name) {
            (vec![s.clone()], false, false)
        } else if let Some(p) = projects.get(&name) {
            (p.clone(), false, true)
        } else if cov.built_in_tree.contains(&name) {
            outcome.insert(name, "in-tree");
            gone.insert(c.purl.as_str().to_string());
            continue;
        } else {
            continue;
        };
        outcome.insert(name, if installed { "installed" } else if project { "project" } else { "submodule" });
        gone.insert(c.purl.as_str().to_string());
        for t in &targets {
            absorbed_searches.entry(t.clone()).or_default().push(c.extra_annotations.clone());
            for f in &c.evidence.source_file_paths {
                pairings.push(Pairing {
                    file: f.clone(),
                    target: t.clone(),
                    conditional: c.lifecycle_scope.is_some(),
                    conditions: conditions_of(c),
                    test_derivation: test_derivation_of(c),
                    installed,
                    project,
                });
                evidence.entry(t.clone()).or_default().push(f.clone());
            }
        }
    }
    components.retain(|c| !(is_search(c) && gone.contains(c.purl.as_str())));
    for c in components.iter_mut() {
        for a in absorbed_searches.get(c.purl.as_str()).into_iter().flatten() {
            super::also_detected::record("S9", c.purl.as_str(), &mut c.extra_annotations, a);
        }
        if let Some(files) = evidence.get(c.purl.as_str()) {
            for f in files {
                if !c.evidence.source_file_paths.contains(f) {
                    c.evidence.source_file_paths.push(f.clone());
                }
            }
        }
    }

    for (name, kind) in outcome {
        let n = cov.declarations.remove(&name).unwrap_or(0);
        match kind {
            "installed" => cov.paired_installed += n,
            "submodule" => cov.paired_submodule += n,
            "project" => cov.paired_project += n,
            _ => {
                cov.in_tree.insert(name);
                cov.in_tree_declarations += n;
            }
        }
    }
    let remaining: BTreeMap<&str, Option<LifecycleScope>> =
        components.iter().filter(|c| is_search(c)).map(|c| (c.purl.as_str(), c.lifecycle_scope)).collect();
    cov.components = remaining.len();
    cov.conditional = remaining.values().filter(|s| **s == Some(LifecycleScope::Optional)).count();
    cov.test = remaining.values().filter(|s| **s == Some(LifecycleScope::Test)).count();
    pairings
}

/// One edge from each pairing's declaring project to the component it
/// stands for; a file with no owner gives a root anchor (research R8).
pub(crate) fn attach(
    relationships: &[Relationship],
    pairings: &[Pairing],
    index: &OwnerIndex,
    scan_root: &Path,
) -> DeclaredAttachment {
    let canonical_root = std::fs::canonicalize(scan_root).unwrap_or_else(|_| scan_root.to_path_buf());
    let mut seen: HashSet<(String, String)> = relationships.iter().map(|r| (r.from.clone(), r.to.clone())).collect();
    let mut out = DeclaredAttachment::default();
    for p in pairings {
        let Some(rel) = relative(&p.file, scan_root, &canonical_root) else { continue };
        match index.owner_of(&rel) {
            Some(owner) if owner == p.target => {}
            Some(owner) => {
                if seen.insert((owner.to_string(), p.target.clone())) {
                    out.edges.push(Relationship {
                        from: owner.to_string(),
                        to: p.target.clone(),
                        relationship_type: RelationshipType::DependsOn,
                        provenance: EnrichmentProvenance {
                            source: rel.to_string_lossy().into_owned(),
                            data_type: "cmake-package-search".to_string(),
                        },
                    });
                }
            }
            None => {
                out.root_anchors.insert(p.target.clone());
            }
        }
    }
    out
}

/// The installed records searched for only under conditions, with the
/// union of those conditions, and the records an unconditional search
/// requires. Research R7.2 extends spec 1543's link rule: a record is
/// Optional only when every link and every search is conditional, nothing
/// else scoped it, and nothing targets it.
///
/// Spec 1617 (#1623): a record that ships a library file is never scoped by
/// its searches (`conditions_only`: C236 only, FR-001, FR-006), and a record
/// that ships none and is searched only from test contexts is Test
/// (FR-004). Neither joins `required`, so spec 1543's link half still
/// applies to them (FR-005).
#[derive(Debug, Default)]
pub(crate) struct RecordScopes {
    pub(crate) optional: BTreeMap<String, BTreeSet<String>>,
    pub(crate) required: BTreeSet<String>,
    /// Test records: their C62 derivation and conditions.
    pub(crate) test: BTreeMap<String, (&'static str, BTreeSet<String>)>,
    /// Records that ship a library file and that the search half would have
    /// made Optional: their conditions, recorded without a scope.
    pub(crate) conditions_only: BTreeMap<String, BTreeSet<String>>,
}

pub(crate) fn record_scopes(
    components: &[ResolvedComponent],
    pairings: &[Pairing],
    linked: &BTreeSet<String>,
    link_conditional: &BTreeMap<String, BTreeSet<String>>,
    targeted: &HashSet<String>,
) -> RecordScopes {
    let mut by_record: BTreeMap<&str, Vec<&Pairing>> = BTreeMap::new();
    for p in pairings.iter().filter(|p| p.installed || p.project) {
        by_record.entry(p.target.as_str()).or_default().push(p);
    }
    let mut out = RecordScopes::default();
    for (purl, ps) in by_record {
        if ps.iter().any(|p| !p.conditional) {
            out.required.insert(purl.to_string());
            // Spec 1641 (FR-007): a record whose non-conditional searches are
            // all ones spec 1641 kept in scope still records their C236.
            if ps.iter().all(|p| p.conditional || !p.conditions.is_empty()) {
                out.conditions_only.insert(purl.to_string(), ps.iter().flat_map(|p| p.conditions.iter().cloned()).collect());
            }
            continue;
        }
        let unconditionally_linked = linked.contains(purl) && !link_conditional.contains_key(purl);
        let scoped = components.iter().any(|c| c.purl.as_str() == purl && c.lifecycle_scope.is_some());
        if unconditionally_linked || scoped || targeted.contains(purl) {
            continue;
        }
        let conds: BTreeSet<String> = ps.iter().flat_map(|p| p.conditions.iter().cloned()).collect();
        let all_test = ps.iter().all(|p| p.test_derivation.is_some());
        // Spec 1618 (research R4): a project the tree builds is, like a
        // shipped library, never made Optional by a consumer's conditions.
        let built_in_tree = ps.iter().any(|p| p.project) && !all_test;
        if built_in_tree || components.iter().any(|c| c.purl.as_str() == purl && ships_library_file(c)) {
            out.conditions_only.insert(purl.to_string(), conds);
        } else if ps.iter().all(|p| p.test_derivation.is_some()) {
            let derivation = if ps.iter().any(|p| p.test_derivation.as_deref() == Some(DERIVATION_SOURCE_TEST_DIRECTORY)) {
                DERIVATION_SOURCE_TEST_DIRECTORY
            } else {
                DERIVATION_CMAKE_BUILD_TESTING
            };
            out.test.insert(purl.to_string(), (derivation, conds));
        } else {
            out.optional.insert(purl.to_string(), conds);
        }
    }
    out
}

/// Mark each record [`RecordScopes::optional`] holds Optional, with C122
/// `cmake-conditional` and C236; each [`RecordScopes::test`] record Test,
/// with its C62 and C236; and write C236 alone on each
/// [`RecordScopes::conditions_only`] record (spec 1617).
pub(crate) fn apply_scope(components: &mut [ResolvedComponent], scopes: &RecordScopes) {
    let conditions = |c: &mut ResolvedComponent, conds: &BTreeSet<String>| {
        if !conds.is_empty() {
            c.extra_annotations.insert(ANN_CMAKE_FIND_CONDITIONS.to_string(), serde_json::json!(conds));
        }
    };
    for c in components.iter_mut() {
        let purl = c.purl.as_str().to_string();
        if let Some(conds) = scopes.optional.get(&purl) {
            c.lifecycle_scope = Some(LifecycleScope::Optional);
            c.extra_annotations.insert(ANN_OPTIONAL_DERIVATION.to_string(), serde_json::json!(DERIVATION_CMAKE_CONDITIONAL));
            conditions(c, conds);
        } else if let Some((derivation, conds)) = scopes.test.get(&purl) {
            c.lifecycle_scope = Some(LifecycleScope::Test);
            c.extra_annotations.insert(ANN_SCOPE_DERIVATION.to_string(), serde_json::json!(derivation));
            conditions(c, conds);
        } else if let Some(conds) = scopes.conditions_only.get(&purl) {
            conditions(c, conds);
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::generate::graph_completeness::test_support::mk_component;

    const REC: &str = "pkg:generic/waybill_fixture_lib@1.2.3";

    fn record(files: &[&str]) -> ResolvedComponent {
        let mut c = mk_component(REC);
        c.evidence.source_file_paths = files.iter().map(|f| f.to_string()).collect();
        c
    }

    fn pairing(conds: &[&str], test: Option<&str>) -> Pairing {
        Pairing {
            file: "share/x/consumer-config.cmake".into(),
            target: REC.into(),
            conditional: true,
            conditions: conds.iter().map(|s| s.to_string()).collect(),
            test_derivation: test.map(str::to_string),
            installed: true,
            project: false,
        }
    }

    fn scopes(c: &ResolvedComponent, ps: &[Pairing]) -> RecordScopes {
        record_scopes(std::slice::from_ref(c), ps, &BTreeSet::new(), &BTreeMap::new(), &HashSet::new())
    }

    /// T004: library file names, case-insensitively.
    #[test]
    fn s1617_library_file_names() {
        for f in ["lib/x.so", "lib/libx.so.1.2", "lib/libX.SO.3", "lib/x.a", "lib/x.dylib", "bin/x.dll", "lib/x.lib"] {
            assert!(ships_library_file(&record(&[f])), "{f}");
        }
        for f in ["lib/cmake/x/xConfigVersion.cmake", "lib/pkgconfig/x.pc", "lib/x.so.txt", "lib/x.so.1a", "lib/.so", "share/data"] {
            assert!(!ships_library_file(&record(&[f])), "{f}");
        }
    }

    /// T004 / FR-001, FR-006: a shipping all-conditional record keeps its
    /// conditions, with no scope, and does not switch off the link half.
    #[test]
    fn s1617_a_shipping_record_is_conditions_only() {
        let c = record(&["lib/libwaybill_fixture_lib.so.1.2.3"]);
        let s = scopes(&c, &[pairing(&["NOT TARGET t"], None)]);
        assert_eq!(s.conditions_only.get(REC).map(|c| c.iter().cloned().collect::<Vec<_>>()), Some(vec!["NOT TARGET t".to_string()]));
        assert!(s.optional.is_empty() && s.test.is_empty() && s.required.is_empty(), "{s:?}");
    }

    /// T004: what today's rule leaves alone, this one does too: no C236.
    #[test]
    fn s1617_a_shipping_record_otherwise_scoped_is_untouched() {
        let mut scoped = record(&["lib/libx.so"]);
        scoped.lifecycle_scope = Some(LifecycleScope::Development);
        let s = scopes(&scoped, &[pairing(&["A"], None)]);
        assert!(s.conditions_only.is_empty() && s.optional.is_empty(), "{s:?}");
        let c = record(&["lib/libx.so"]);
        let linked: BTreeSet<String> = [REC.to_string()].into();
        let s = record_scopes(std::slice::from_ref(&c), &[pairing(&["A"], None)], &linked, &BTreeMap::new(), &HashSet::new());
        assert!(s.conditions_only.is_empty(), "unconditionally linked: {s:?}");
        let targeted: HashSet<String> = [REC.to_string()].into();
        let s = record_scopes(std::slice::from_ref(&c), &[pairing(&["A"], None)], &BTreeSet::new(), &BTreeMap::new(), &targeted);
        assert!(s.conditions_only.is_empty(), "targeted: {s:?}");
    }

    /// T004: a record that ships nothing stays Optional, as on `main`.
    #[test]
    fn s1617_a_bare_conditional_record_stays_optional() {
        let s = scopes(&record(&["lib/cmake/x/xConfigVersion.cmake"]), &[pairing(&["A"], None)]);
        assert!(s.optional.contains_key(REC), "{s:?}");
    }

    /// T009 / FR-004: all-Test pairings make a bare record Test, preferring
    /// `source-test-directory`; one Optional pairing keeps it Optional; a
    /// shipping all-Test record is conditions-only.
    #[test]
    fn s1617_test_pairings() {
        let bare = record(&["lib/cmake/x/xConfigVersion.cmake"]);
        let s = scopes(&bare, &[pairing(&[], Some("cmake-build-testing")), pairing(&[], Some("source-test-directory"))]);
        assert_eq!(s.test.get(REC).map(|(d, _)| *d), Some("source-test-directory"));
        let s = scopes(&bare, &[pairing(&["BUILD_TESTING"], Some("cmake-build-testing"))]);
        assert_eq!(s.test.get(REC).map(|(d, c)| (*d, c.len())), Some(("cmake-build-testing", 1)));
        let s = scopes(&bare, &[pairing(&[], Some("source-test-directory")), pairing(&["A"], None)]);
        assert!(s.optional.contains_key(REC) && s.test.is_empty(), "{s:?}");
        let shipping = record(&["lib/libx.so.1"]);
        let s = scopes(&shipping, &[pairing(&[], Some("source-test-directory"))]);
        assert!(s.conditions_only.contains_key(REC) && s.test.is_empty(), "{s:?}");
    }
}
