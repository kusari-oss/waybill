//! Spec 1538 (#1516), research R6: a CMake fetch is a dependency of the
//! project that declares it. The owner is spec 1537's: the nearest
//! enclosing main module or git submodule, up to and including the scan
//! root, found through the same `OwnerIndex`. A declaring file with no
//! owner makes the fetch a declared root anchor. On TheRock the owners are
//! the submodules (`rocm-libraries`, `rocm-systems`, …).

use std::collections::HashSet;
use std::path::Path;

use waybill_common::resolution::{EnrichmentProvenance, Relationship, RelationshipType, ResolvedComponent};

use super::declared_attach::{relative, DeclaredAttachment, OwnerIndex};

/// A CMake fetch, or (spec 1614, research R8) a CMake package search: both
/// are a dependency of the project whose file declares them. Before spec
/// 1614 a package search reached the root only through the graph-top
/// fallback, which strands it once the root has any other edge.
fn is_cmake_declaration(c: &ResolvedComponent) -> bool {
    c.extra_annotations
        .get("waybill:source-mechanism")
        .and_then(|v| v.as_str())
        .is_some_and(|m| {
            m.starts_with("cmake-fetchcontent")
                || m == "cmake-externalproject"
                || m == "cmake-find-package"
                || m == "cmake-pkg-check-modules"
        })
}

/// One edge from each declaring file's owner; a file with no owner gives a
/// root anchor. No self-edge and no duplicate of an existing edge.
pub(crate) fn attach(
    components: &[ResolvedComponent],
    relationships: &[Relationship],
    index: &OwnerIndex,
    scan_root: &Path,
) -> DeclaredAttachment {
    let canonical_root = std::fs::canonicalize(scan_root).unwrap_or_else(|_| scan_root.to_path_buf());
    let mut seen: HashSet<(String, String)> =
        relationships.iter().map(|r| (r.from.clone(), r.to.clone())).collect();
    let mut out = DeclaredAttachment::default();
    for c in components.iter().filter(|c| is_cmake_declaration(c)) {
        for p in &c.evidence.source_file_paths {
            let Some(rel) = relative(p, scan_root, &canonical_root) else { continue };
            match index.owner_of(&rel) {
                Some(owner) if owner == c.purl.as_str() => {}
                Some(owner) => {
                    if seen.insert((owner.to_string(), c.purl.as_str().to_string())) {
                        out.edges.push(Relationship {
                            from: owner.to_string(),
                            to: c.purl.as_str().to_string(),
                            relationship_type: RelationshipType::DependsOn,
                            provenance: EnrichmentProvenance {
                                source: rel.to_string_lossy().into_owned(),
                                data_type: "cmake-fetch-declaration".to_string(),
                            },
                        });
                    }
                }
                None => {
                    out.root_anchors.insert(c.purl.as_str().to_string());
                }
            }
        }
    }
    tracing::debug!(edges = out.edges.len(), root_anchors = out.root_anchors.len(), "cmake fetches: attached");
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::generate::graph_completeness::test_support::mk_component;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    const ROOT: &str = "/nonexistent-waybill-fixture-root";

    fn fetch(purl: &str, files: &[&str]) -> ResolvedComponent {
        let mut c = mk_component(purl);
        c.evidence.source_file_paths = files.iter().map(|f| f.to_string()).collect();
        c.extra_annotations.insert("waybill:source-mechanism".into(), serde_json::json!("cmake-fetchcontent-url"));
        c
    }

    #[test]
    fn a_submodule_owns_the_fetches_in_its_tree_and_the_rest_are_root_anchors() {
        let sub = "pkg:github/waybill-fixture/x@abc";
        let a = "pkg:generic/waybill-fixture-a@1.0";
        let b = "pkg:generic/waybill-fixture-b@1.0";
        let comps = [fetch(a, &["third_party/x/CMakeLists.txt"]), fetch(b, &["tools/CMakeLists.txt"])];
        let dirs: BTreeMap<PathBuf, String> = [(Path::new(ROOT).join("third_party/x"), sub.to_string())].into();
        let index = OwnerIndex::build(&comps, &dirs, Path::new(ROOT), crate::scan_fs::ScanMode::Path);
        let d = attach(&comps, &[], &index, Path::new(ROOT));
        let edges: Vec<(String, String)> = d.edges.iter().map(|r| (r.from.clone(), r.to.clone())).collect();
        assert_eq!(edges, vec![(sub.to_string(), a.to_string())]);
        assert_eq!(d.root_anchors.into_iter().collect::<Vec<_>>(), vec![b.to_string()]);
    }

    /// Spec 1614 research R8: a package search attaches as a fetch does
    /// (before, spec 1538 attached fetches only). Any other mechanism does not.
    #[test]
    fn fetches_and_package_searches_are_attached_and_nothing_else() {
        let with = |purl: &str, mechanism: &str| {
            let mut c = mk_component(purl);
            c.evidence.source_file_paths = vec!["sub/CMakeLists.txt".into()];
            c.extra_annotations.insert("waybill:source-mechanism".into(), serde_json::json!(mechanism));
            c
        };
        let comps = [
            with("pkg:generic/waybill-fixture-fp", "cmake-find-package"),
            with("pkg:generic/waybill-fixture-pc", "cmake-pkg-check-modules"),
            with("pkg:generic/waybill-fixture-other@1.0", "cmake-package-version"),
        ];
        let index = OwnerIndex::build(&[], &BTreeMap::new(), Path::new(ROOT), crate::scan_fs::ScanMode::Path);
        let d = attach(&comps, &[], &index, Path::new(ROOT));
        assert!(d.edges.is_empty());
        assert_eq!(
            d.root_anchors.into_iter().collect::<Vec<_>>(),
            vec!["pkg:generic/waybill-fixture-fp".to_string(), "pkg:generic/waybill-fixture-pc".to_string()]
        );
    }
}
