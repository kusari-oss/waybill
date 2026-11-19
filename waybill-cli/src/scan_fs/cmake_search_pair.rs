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
//! 3. **A name the tree builds itself** (`project()`, a package-config
//!    template, a provided config file): the entries go, with no edge, and
//!    the name is listed in C237 `in-tree`. This comes last because a
//!    submodule is usually also a CMake project (analysis I2).
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

const MECHANISM: &str = "waybill:source-mechanism";
const ANN_BUILD_REFERENCE: &str = "waybill:build-reference";
const ANN_OPTIONAL_DERIVATION: &str = "waybill:optional-derivation";
const DERIVATION_CMAKE_CONDITIONAL: &str = "cmake-conditional";

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
    /// An installed-library record, whose scope the searches can decide.
    pub(crate) installed: bool,
}

fn mechanism(c: &ResolvedComponent) -> Option<&str> {
    c.extra_annotations.get(MECHANISM).and_then(|v| v.as_str())
}

fn is_search(c: &ResolvedComponent) -> bool {
    matches!(mechanism(c), Some("cmake-find-package" | "cmake-pkg-check-modules"))
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
    let Some(cov) = cov else { return Vec::new() };
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
    let mut gone: BTreeSet<String> = BTreeSet::new();
    let mut outcome: BTreeMap<String, &'static str> = BTreeMap::new();
    for c in components.iter().filter(|c| is_search(c)) {
        let name = c.name.to_lowercase();
        let (targets, installed): (Vec<String>, bool) = if let Some(r) = records.get(&name) {
            (r.clone(), true)
        } else if let Some(s) = submodules.get(&name) {
            (vec![s.clone()], false)
        } else if cov.built_in_tree.contains(&name) {
            outcome.insert(name, "in-tree");
            gone.insert(c.purl.as_str().to_string());
            continue;
        } else {
            continue;
        };
        outcome.insert(name, if installed { "installed" } else { "submodule" });
        gone.insert(c.purl.as_str().to_string());
        for t in &targets {
            for f in &c.evidence.source_file_paths {
                pairings.push(Pairing {
                    file: f.clone(),
                    target: t.clone(),
                    conditional: c.lifecycle_scope.is_some(),
                    conditions: conditions_of(c),
                    installed,
                });
                evidence.entry(t.clone()).or_default().push(f.clone());
            }
        }
    }
    components.retain(|c| !(is_search(c) && gone.contains(c.purl.as_str())));
    for c in components.iter_mut() {
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
#[derive(Debug, Default)]
pub(crate) struct RecordScopes {
    pub(crate) optional: BTreeMap<String, BTreeSet<String>>,
    pub(crate) required: BTreeSet<String>,
}

pub(crate) fn record_scopes(
    components: &[ResolvedComponent],
    pairings: &[Pairing],
    linked: &BTreeSet<String>,
    link_conditional: &BTreeMap<String, BTreeSet<String>>,
    targeted: &HashSet<String>,
) -> RecordScopes {
    let mut by_record: BTreeMap<&str, Vec<&Pairing>> = BTreeMap::new();
    for p in pairings.iter().filter(|p| p.installed) {
        by_record.entry(p.target.as_str()).or_default().push(p);
    }
    let mut out = RecordScopes::default();
    for (purl, ps) in by_record {
        if ps.iter().any(|p| !p.conditional) {
            out.required.insert(purl.to_string());
            continue;
        }
        let unconditionally_linked = linked.contains(purl) && !link_conditional.contains_key(purl);
        let scoped = components.iter().any(|c| c.purl.as_str() == purl && c.lifecycle_scope.is_some());
        if unconditionally_linked || scoped || targeted.contains(purl) {
            continue;
        }
        let conds = ps.iter().flat_map(|p| p.conditions.iter().cloned()).collect();
        out.optional.insert(purl.to_string(), conds);
    }
    out
}

/// Mark each record [`RecordScopes::optional`] holds Optional, with C122
/// `cmake-conditional` and C236.
pub(crate) fn apply_scope(components: &mut [ResolvedComponent], optional: &BTreeMap<String, BTreeSet<String>>) {
    for c in components.iter_mut() {
        let Some(conds) = optional.get(c.purl.as_str()) else { continue };
        c.lifecycle_scope = Some(LifecycleScope::Optional);
        c.extra_annotations.insert(ANN_OPTIONAL_DERIVATION.to_string(), serde_json::json!(DERIVATION_CMAKE_CONDITIONAL));
        if !conds.is_empty() {
            c.extra_annotations.insert(ANN_CMAKE_FIND_CONDITIONS.to_string(), serde_json::json!(conds));
        }
    }
}
