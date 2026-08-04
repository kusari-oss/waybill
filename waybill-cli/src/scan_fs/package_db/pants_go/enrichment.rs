//! Milestone 226: inject `waybill:pants-target` onto existing `pkg:golang/*` components.
//!
//! Implements the R3+R4 matching algorithm from research.md:
//!
//! - **R3 `go_mod` ownership**: a component belongs to a `go_mod`
//!   target when ANY entry in `component.evidence.source_file_paths`
//!   starts with the go_mod BUILD file's directory. Longest-prefix
//!   match wins (deeper `go_mod` overrides shallower).
//! - **R3 explicit `go_third_party_package` match**: PURL →
//!   import_path reconstruction is
//!   `namespace().map(|n| format!("{n}/{}", name())).unwrap_or_else(|| name().to_string())`
//!   — e.g., `pkg:golang/github.com/spf13/cobra@v1.6.0` →
//!   `"github.com/spf13/cobra"`. Direct lookup in
//!   `import_path_to_addresses`.
//! - **R4 main-module attribution**: a component with
//!   `extra_annotations["waybill:component-role"] == "main-module"`
//!   is matched against `main_targets` (source_path.parent() equality)
//!   and `package_targets` (source_path.parent() `starts_with`).
//!
//! **Zero fabrication** (FR-012 / Principle IX): this module NEVER
//! constructs new `ResolvedComponent` entries. It only reads
//! existing ones and returns owning addresses; the orchestrator
//! injects annotations in place.

use std::path::PathBuf;

use waybill_common::resolution::ResolvedComponent;

use super::{GoOwnershipIndex, TargetAddress};

/// Reconstruct the Go module import path from a `pkg:golang/*` PURL.
/// Per R3 + finding I1 from `/speckit-analyze`.
fn purl_to_import_path(component: &ResolvedComponent) -> String {
    let purl = &component.purl;
    match purl.namespace() {
        Some(ns) => format!("{ns}/{}", purl.name()),
        None => purl.name().to_string(),
    }
}

/// Collect every Pants target address that owns this component per
/// R3+R4. Returned vec is lex-sorted + deduped; empty when no
/// target matches.
///
/// `scan_root` normalizes evidence paths: `evidence.source_file_paths`
/// entries may be either absolute (some readers) or scan-root-relative
/// (Go reader emits relative — e.g., `"3rdparty/go/go.sum"`).
/// Ownership-index paths are always absolute (built from
/// `build_file.parent()`), so we join relative source paths with
/// `scan_root` before matching.
///
/// Callers restrict this to `pkg:golang/*` components upstream —
/// the function has no guard of its own (matches only apply to Go
/// paths in the index anyway, so misuse returns empty).
pub(crate) fn collect_addresses_for_component(
    component: &ResolvedComponent,
    index: &GoOwnershipIndex,
    scan_root: &std::path::Path,
) -> Vec<TargetAddress> {
    let mut addresses: Vec<TargetAddress> = Vec::new();

    // Normalize every source_file_path to absolute form. Relative
    // paths (Go reader convention) get joined with scan_root.
    let absolute_source_paths: Vec<PathBuf> = component
        .evidence
        .source_file_paths
        .iter()
        .map(|s| {
            let p = PathBuf::from(s);
            if p.is_absolute() {
                p
            } else {
                scan_root.join(p)
            }
        })
        .collect();

    // R3(a) go_mod deepest-prefix match on any source_file_path.
    // `BTreeMap` iteration is ascending by key; we track the root
    // with the MOST path components across all evidence entries.
    let mut longest_go_mod_match: Option<(PathBuf, TargetAddress)> = None;
    for source_path in &absolute_source_paths {
        for (root, addr) in &index.go_mod_roots {
            if source_path.starts_with(root) {
                let take = match &longest_go_mod_match {
                    None => true,
                    Some((cur, _)) => root.components().count() > cur.components().count(),
                };
                if take {
                    longest_go_mod_match = Some((root.clone(), addr.clone()));
                }
            }
        }
    }
    if let Some((_, addr)) = longest_go_mod_match {
        addresses.push(addr);
    }

    // R3(b) explicit go_third_party_package via import-path lookup.
    let import_path = purl_to_import_path(component);
    if let Some(addrs) = index.import_path_to_addresses.get(&import_path) {
        for a in addrs {
            addresses.push(a.clone());
        }
    }

    // R4 main-module attribution. Only fires when the component
    // carries the m053 `waybill:component-role = main-module` marker.
    let is_main_module = component
        .extra_annotations
        .get("waybill:component-role")
        .and_then(|v| v.as_str())
        == Some("main-module");
    if is_main_module {
        for source_path in &absolute_source_paths {
            let source_dir = match source_path.parent() {
                Some(p) => p.to_path_buf(),
                None => continue,
            };
            for (main_dir, addr) in &index.main_targets {
                if source_dir == *main_dir || source_dir.starts_with(main_dir) {
                    addresses.push(addr.clone());
                }
            }
            for (pkg_dir, addr) in &index.package_targets {
                if source_dir.starts_with(pkg_dir) {
                    addresses.push(addr.clone());
                }
            }
        }
    }

    addresses.sort();
    addresses.dedup();
    addresses
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;
    use waybill_common::resolution::{ResolutionEvidence, ResolutionTechnique};
    use waybill_common::types::purl::Purl;

    fn go_component(purl_str: &str, source_paths: Vec<&str>) -> ResolvedComponent {
        let purl = Purl::new(purl_str).unwrap();
        ResolvedComponent {
            name: purl.name().to_string(),
            version: purl.version().unwrap_or("0.0.0").to_string(),
            purl,
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::UrlPattern,
                confidence: 1.0,
                source_connection_ids: Vec::new(),
                source_file_paths: source_paths.into_iter().map(String::from).collect(),
                deps_dev_match: None,
            },
            licenses: Vec::new(),
            concluded_licenses: Vec::new(),
            hashes: Vec::new(),
            supplier: None,
            cpes: Vec::new(),
            advisories: Vec::new(),
            occurrences: Vec::new(),
            lifecycle_scope: None,
            build_inclusion: None,
            requirement_ranges: Vec::new(),
            source_type: None,
            sbom_tier: Some("source".to_string()),
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
            external_references: Vec::new(),
            extra_annotations: BTreeMap::new(),
            binary_role: None,
        }
    }

    fn empty_index() -> GoOwnershipIndex {
        GoOwnershipIndex::default()
    }

    #[test]
    fn single_go_mod_match_returns_one_address() {
        let mut index = empty_index();
        index.go_mod_roots.insert(
            PathBuf::from("/repo/3rdparty/go"),
            TargetAddress("3rdparty/go:mod".to_string()),
        );
        let c = go_component(
            "pkg:golang/github.com/spf13/cobra@v1.6.0",
            vec!["/repo/3rdparty/go/go.sum"],
        );
        let addrs = collect_addresses_for_component(&c, &index, std::path::Path::new("/repo"));
        assert_eq!(addrs.len(), 1);
        assert_eq!(addrs[0].0, "3rdparty/go:mod");
    }

    #[test]
    fn no_match_returns_empty() {
        let index = empty_index();
        let c = go_component(
            "pkg:golang/github.com/spf13/cobra@v1.6.0",
            vec!["/other/go.sum"],
        );
        assert!(collect_addresses_for_component(&c, &index, std::path::Path::new("/repo")).is_empty());
    }

    #[test]
    fn multi_owner_merge_sorted_and_deduped() {
        let mut index = empty_index();
        index.go_mod_roots.insert(
            PathBuf::from("/repo/3rdparty/go"),
            TargetAddress("3rdparty/go:mod".to_string()),
        );
        index.import_path_to_addresses.insert(
            "github.com/spf13/cobra".to_string(),
            vec![TargetAddress("3rdparty/go:cobra".to_string())],
        );
        let c = go_component(
            "pkg:golang/github.com/spf13/cobra@v1.6.0",
            vec!["/repo/3rdparty/go/go.sum"],
        );
        let addrs = collect_addresses_for_component(&c, &index, std::path::Path::new("/repo"));
        assert_eq!(addrs.len(), 2);
        assert_eq!(addrs[0].0, "3rdparty/go:cobra");
        assert_eq!(addrs[1].0, "3rdparty/go:mod");
    }

    #[test]
    fn main_module_go_binary_main_dot_matches() {
        let mut index = empty_index();
        index.main_targets.push((
            PathBuf::from("/repo/cmd/frontend"),
            TargetAddress("cmd/frontend:frontend".to_string()),
        ));
        let mut c = go_component(
            "pkg:golang/github.com/waybill-fixture/frontend@v0.0.0",
            vec!["/repo/cmd/frontend/main.go"],
        );
        c.extra_annotations
            .insert("waybill:component-role".to_string(), json!("main-module"));
        let addrs = collect_addresses_for_component(&c, &index, std::path::Path::new("/repo"));
        assert_eq!(addrs.len(), 1);
        assert_eq!(addrs[0].0, "cmd/frontend:frontend");
    }

    #[test]
    fn main_module_go_package_matches() {
        let mut index = empty_index();
        index.package_targets.push((
            PathBuf::from("/repo/cmd/frontend"),
            TargetAddress("cmd/frontend:pkg".to_string()),
        ));
        let mut c = go_component(
            "pkg:golang/github.com/waybill-fixture/frontend@v0.0.0",
            vec!["/repo/cmd/frontend/main.go"],
        );
        c.extra_annotations
            .insert("waybill:component-role".to_string(), json!("main-module"));
        let addrs = collect_addresses_for_component(&c, &index, std::path::Path::new("/repo"));
        assert_eq!(addrs.len(), 1);
        assert_eq!(addrs[0].0, "cmd/frontend:pkg");
    }

    #[test]
    fn third_party_import_path_direct_match_over_go_mod_only() {
        let mut index = empty_index();
        index.go_mod_roots.insert(
            PathBuf::from("/repo/3rdparty/go"),
            TargetAddress("3rdparty/go:mod".to_string()),
        );
        index.import_path_to_addresses.insert(
            "github.com/spf13/cobra".to_string(),
            vec![TargetAddress("3rdparty/go:cobra".to_string())],
        );
        let c = go_component(
            "pkg:golang/github.com/spf13/cobra@v1.6.0",
            vec!["/repo/3rdparty/go/go.sum"],
        );
        let addrs = collect_addresses_for_component(&c, &index, std::path::Path::new("/repo"));
        // BOTH owners preserved (multi-owner merge); "cobra" < "mod" lex-sort.
        assert_eq!(addrs.len(), 2);
        assert_eq!(addrs[0].0, "3rdparty/go:cobra");
        assert_eq!(addrs[1].0, "3rdparty/go:mod");
    }
}
