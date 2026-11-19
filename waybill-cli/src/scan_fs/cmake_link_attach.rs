//! Spec 1543 (#1602), research R4 and R5: an installed-library record that
//! a CMake file links by link directory and plain name is a dependency of
//! the project that links it.
//!
//! A record (spec 1531: a pkg-config file or a CMake version file in an
//! install location) reaches the graph through `find_package` correlation.
//! A vendored prebuilt SDK linked by directory never does: TheRock's `wkmi`
//! has package files under `lnx/lib/cmake/wkmi/`, but the WSL build links it
//! as `target_link_directories(… wkmi/lnx/lib)` plus `target_link_libraries(…
//! wkmi)`, and nothing calls `find_package(wkmi)`.
//!
//! - **Match.** A link directory inside the record's prefix, with the
//!   linked name equal to the record's name (ignoring case), or naming a
//!   `lib<name>.a`, `lib<name>.so` or `lib<name>.so.*` in that directory, or
//!   a DVC pointer to one (`<file>.dvc`; the library itself is often not in
//!   the checkout). With several matching records, the longest prefix wins.
//! - **Owner.** Spec 1537's `OwnerIndex`, as CMake fetches use: the nearest
//!   enclosing main module or git submodule of the linking file, else a
//!   root anchor.
//! - **Scope.** A record linked only inside blocks, and depended on by
//!   nothing else, is Optional with C122 `cmake-conditional-link` and C235
//!   the union of the conditions. One unconditional link, an existing scope
//!   or an existing edge into the record leaves its scope as it was.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use waybill_common::resolution::{
    EnrichmentProvenance, LifecycleScope, Relationship, RelationshipType, ResolvedComponent,
};

use super::declared_attach::{relative, DeclaredAttachment, OwnerIndex};
use super::package_db::cmake::links::LinkDeclaration;
use super::package_db::installed_lib::location::prefix_of;
use super::package_db::installed_lib::RecordKind;

/// C122 value for a record linked only under conditions.
pub(crate) const DERIVATION_CMAKE_CONDITIONAL_LINK: &str = "cmake-conditional-link";
/// C235.
pub(crate) const ANN_CMAKE_LINK_CONDITIONS: &str = "waybill:cmake-link-conditions";
const ANN_OPTIONAL_DERIVATION: &str = "waybill:optional-derivation";

/// What the pass found: the edges and anchors, and the records to mark
/// Optional with their conditions.
#[derive(Debug, Default)]
pub(crate) struct LinkAttachment {
    pub(crate) attached: DeclaredAttachment,
    pub(crate) conditional: BTreeMap<String, BTreeSet<String>>,
    /// Spec 1614 research R7.2: every record a link matched, conditional or
    /// not, so a package search's scope can combine with the links'.
    pub(crate) linked: BTreeSet<String>,
}

fn record_kind(c: &ResolvedComponent) -> Option<RecordKind> {
    match c.extra_annotations.get("waybill:source-mechanism").and_then(|v| v.as_str())? {
        "cmake-package-version" => Some(RecordKind::CmakeVersion),
        "pkg-config" => Some(RecordKind::PkgConfig),
        _ => None,
    }
}

/// Whether `dir` holds the library `name` links, or a DVC pointer to it.
fn holds_library(dir: &Path, name: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else { return false };
    let (a, so) = (format!("lib{name}.a"), format!("lib{name}.so"));
    let so_dot = format!("{so}.");
    entries.flatten().any(|e| {
        let file = e.file_name().to_string_lossy().into_owned();
        let lib = file.strip_suffix(".dvc").unwrap_or(&file);
        lib == a || lib == so || lib.starts_with(&so_dot)
    })
}

/// Each record's absolute prefix, by PURL.
fn record_prefixes<'a>(components: &'a [ResolvedComponent], canonical_root: &Path) -> Vec<(PathBuf, &'a ResolvedComponent)> {
    let mut out = Vec::new();
    for c in components {
        let Some(kind) = record_kind(c) else { continue };
        let prefixes: BTreeSet<PathBuf> = c
            .evidence
            .source_file_paths
            .iter()
            .filter_map(|p| prefix_of(Path::new(p), kind, &c.name))
            .collect();
        out.extend(prefixes.into_iter().map(|p| (canonical_root.join(p), c)));
    }
    out
}

pub(crate) fn attach(
    components: &[ResolvedComponent],
    relationships: &[Relationship],
    links: &[LinkDeclaration],
    index: &OwnerIndex,
    scan_root: &Path,
) -> LinkAttachment {
    let mut out = LinkAttachment::default();
    if links.is_empty() {
        return out;
    }
    let canonical_root = std::fs::canonicalize(scan_root).unwrap_or_else(|_| scan_root.to_path_buf());
    let records = record_prefixes(components, &canonical_root);
    if records.is_empty() {
        return out;
    }
    let mut seen: HashSet<(String, String)> = relationships.iter().map(|r| (r.from.clone(), r.to.clone())).collect();
    let targeted: HashSet<&str> = relationships.iter().map(|r| r.to.as_str()).collect();
    // Per record: every matched link's conditions (empty = unconditional).
    let mut matched: BTreeMap<String, Vec<Vec<String>>> = BTreeMap::new();
    for l in links {
        let best = records
            .iter()
            .filter(|(prefix, c)| {
                l.directory.starts_with(prefix)
                    && (c.name.eq_ignore_ascii_case(&l.name) || holds_library(&l.directory, &l.name))
            })
            .max_by_key(|(prefix, _)| prefix.components().count());
        let Some((_, record)) = best else { continue };
        let purl = record.purl.as_str().to_string();
        matched.entry(purl.clone()).or_default().push(l.conditions.clone());
        out.linked.insert(purl.clone());
        let Some(rel) = relative(&l.file.to_string_lossy(), scan_root, &canonical_root) else { continue };
        match index.owner_of(&rel) {
            Some(owner) if owner == purl => {}
            Some(owner) => {
                if seen.insert((owner.to_string(), purl.clone())) {
                    out.attached.edges.push(Relationship {
                        from: owner.to_string(),
                        to: purl.clone(),
                        relationship_type: RelationshipType::DependsOn,
                        provenance: EnrichmentProvenance {
                            source: rel.to_string_lossy().into_owned(),
                            data_type: "cmake-link-declaration".to_string(),
                        },
                    });
                }
            }
            None => {
                out.attached.root_anchors.insert(purl.clone());
            }
        }
    }
    for (purl, conds) in matched {
        let unconditional = conds.iter().any(Vec::is_empty);
        let record = components.iter().find(|c| c.purl.as_str() == purl);
        let scoped = record.is_some_and(|c| c.lifecycle_scope.is_some());
        if unconditional || scoped || targeted.contains(purl.as_str()) {
            continue;
        }
        out.conditional.insert(purl, conds.into_iter().flatten().collect());
    }
    tracing::debug!(
        edges = out.attached.edges.len(),
        root_anchors = out.attached.root_anchors.len(),
        conditional = out.conditional.len(),
        "cmake links: attached"
    );
    out
}

/// Mark each conditionally linked record Optional, with C122 and C235.
pub(crate) fn apply_scope(components: &mut [ResolvedComponent], conditional: &BTreeMap<String, BTreeSet<String>>) {
    for c in components.iter_mut() {
        let Some(conds) = conditional.get(c.purl.as_str()) else { continue };
        c.lifecycle_scope = Some(LifecycleScope::Optional);
        c.extra_annotations
            .insert(ANN_OPTIONAL_DERIVATION.to_string(), serde_json::json!(DERIVATION_CMAKE_CONDITIONAL_LINK));
        c.extra_annotations.insert(ANN_CMAKE_LINK_CONDITIONS.to_string(), serde_json::json!(conds));
    }
}
