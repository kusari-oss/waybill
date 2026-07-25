//! Milestone 220 — post-discovery scope filter.
//!
//! [`apply_scope_filter`] runs after per-ecosystem readers populate
//! the resolved-component + relationship slices (and after the
//! m127-era `waybill:workspace-member` tagging pass in
//! `scan_cmd::scan`). Under `All` it's a zero-op; under `RootOnly` /
//! `Strict` it BFS-projects each in-scope root, applies the
//! FR-004 belt-and-suspenders workspace-member follow-up, iterates
//! the FR-005 fixpoint over nested-workspace roots, then filters
//! the component + relationship slices to the reachable set.
//!
//! See `contracts/scope-filter-algorithm.md` +
//! `contracts/workspace-member-preservation.md`.

use std::collections::BTreeSet;
use std::path::Path;

use waybill_common::resolution::{Relationship, ResolvedComponent};

use super::{ProjectDiscoveryMode, ProjectDiscoveryReport};

/// The main-module component-role annotation key stamped by every
/// per-ecosystem reader that reports a workspace member (cargo /
/// npm / go / maven / …). Used by the FR-005 fixpoint pass to
/// detect nested workspace roots pulled in by the outer workspace's
/// annotation follow-up.
const COMPONENT_ROLE_KEY: &str = "waybill:component-role";
/// The workspace-member back-reference annotation key stamped by
/// `scan_cmd::tag_components_with_workspace_member` on every
/// component under a workspace-root's declared members. Value =
/// PURL string of the workspace root.
const WORKSPACE_MEMBER_KEY: &str = "waybill:workspace-member";
const MAIN_MODULE_ROLE: &str = "main-module";

/// Post-discovery filter entry point.
///
/// Under `All`: zero-op (no clone, no BFS, no annotation walk;
/// returns inputs verbatim). Under `RootOnly` / `Strict`: enumerate
/// in-scope roots, BFS-project each, run the workspace-member
/// follow-up + fixpoint pass under `RootOnly`, filter both slices,
/// return the report.
///
/// Preserves input ordering under all modes (filter is
/// order-preserving).
pub fn apply_scope_filter(
    components: Vec<ResolvedComponent>,
    relationships: Vec<Relationship>,
    mode: ProjectDiscoveryMode,
    scan_root: &Path,
) -> (Vec<ResolvedComponent>, Vec<Relationship>, ProjectDiscoveryReport) {
    // Step 0: fast-path for default mode. Zero-cost — no allocation,
    // no BFS, no annotation walk. SC-005 byte-identity gate.
    if mode == ProjectDiscoveryMode::All {
        return (
            components,
            relationships,
            ProjectDiscoveryReport::all_default(),
        );
    }

    // Step 1: enumerate + filter main-modules.
    let all_roots =
        crate::generate::split::enumerate_workspace_roots(&components, scan_root);
    let in_scope_roots: Vec<_> = all_roots
        .iter()
        .filter(|r| mode.is_root_in_scope(r, scan_root))
        .cloned()
        .collect();
    let nested_projects_ignored =
        all_roots.len().saturating_sub(in_scope_roots.len());

    // Step 2: BFS-project each in-scope root. Reuses m215's
    // `project_for_root` verbatim (correctness inherits from that:
    // BFS over dep-edge relationships, self-contained relationship
    // filtering, sibling-main-module demotion).
    let mut reachable: BTreeSet<String> = BTreeSet::new();
    for root in &in_scope_roots {
        let proj = crate::generate::split::project_for_root(
            root,
            &components,
            &relationships,
        );
        for c in &proj.components {
            reachable.insert(c.purl.as_str().to_string());
        }
    }

    // Step 3: workspace-member inclusion (RootOnly only) + FR-005
    // fixpoint recursion for nested workspaces. Skipped under
    // Strict — workspace members drop even if annotated.
    let mut workspace_members_followed = 0usize;
    if mode.follows_workspace_members() {
        let mut root_purls: BTreeSet<String> = in_scope_roots
            .iter()
            .map(|r| r.purl_string.clone())
            .collect();
        // Initial belt-and-suspenders pass: some workspace-member
        // components aren't BFS-reachable from a root (e.g., Cargo
        // `[workspace] members` doesn't automatically create a
        // root→member dep edge). Annotation-based follow-up
        // captures them.
        annotation_follow_up(
            &components,
            &root_purls,
            &mut reachable,
            &mut workspace_members_followed,
        );

        // FR-005 nested-workspace fixpoint: any pulled-in component
        // tagged `waybill:component-role = main-module` is itself a
        // nested workspace root. Add its PURL to `root_purls` and
        // re-run the annotation pass; repeat until no new roots are
        // added. Terminates because each iteration only adds and the
        // component set is bounded.
        loop {
            let mut newly_added: BTreeSet<String> = BTreeSet::new();
            for c in &components {
                if !reachable.contains(c.purl.as_str()) {
                    continue;
                }
                let is_mm = matches!(
                    c.extra_annotations.get(COMPONENT_ROLE_KEY),
                    Some(v) if v.as_str() == Some(MAIN_MODULE_ROLE)
                );
                if !is_mm {
                    continue;
                }
                let purl = c.purl.as_str().to_string();
                if !root_purls.contains(&purl) {
                    newly_added.insert(purl);
                }
            }
            if newly_added.is_empty() {
                break;
            }
            root_purls.extend(newly_added);
            annotation_follow_up(
                &components,
                &root_purls,
                &mut reachable,
                &mut workspace_members_followed,
            );
        }
    }

    // Step 4: filter component + relationship slices (order-preserving).
    let filtered_components: Vec<ResolvedComponent> = components
        .into_iter()
        .filter(|c| reachable.contains(c.purl.as_str()))
        .collect();
    let filtered_relationships: Vec<Relationship> = relationships
        .into_iter()
        .filter(|r| reachable.contains(&r.from) && reachable.contains(&r.to))
        .collect();

    // Step 5: build report.
    let report = ProjectDiscoveryReport {
        mode,
        root_main_modules: in_scope_roots.len(),
        workspace_members_followed,
        nested_projects_ignored,
    };
    (filtered_components, filtered_relationships, report)
}

/// One workspace-member annotation-follow-up pass. Walks
/// `components`, retains any component whose
/// `waybill:workspace-member` annotation value matches a PURL in
/// `root_purls` (and isn't already reachable). Bumps
/// `workspace_members_followed` per newly-included component.
fn annotation_follow_up(
    components: &[ResolvedComponent],
    root_purls: &BTreeSet<String>,
    reachable: &mut BTreeSet<String>,
    workspace_members_followed: &mut usize,
) {
    for c in components {
        if reachable.contains(c.purl.as_str()) {
            continue;
        }
        let Some(v) = c.extra_annotations.get(WORKSPACE_MEMBER_KEY) else {
            continue;
        };
        let Some(root_ref) = v.as_str() else { continue };
        if root_purls.contains(root_ref) {
            reachable.insert(c.purl.as_str().to_string());
            *workspace_members_followed += 1;
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use serde_json::{Value, json};
    use waybill_common::resolution::{
        Relationship, ResolutionEvidence, ResolutionTechnique, ResolvedComponent,
    };
    use waybill_common::types::purl::Purl;

    use super::*;

    fn mk_component(purl: &str, role: Option<&str>) -> ResolvedComponent {
        let p = Purl::new(purl).unwrap();
        let mut ann: BTreeMap<String, Value> = BTreeMap::new();
        if let Some(r) = role {
            ann.insert(COMPONENT_ROLE_KEY.to_string(), Value::String(r.to_string()));
        }
        ResolvedComponent {
            purl: p.clone(),
            name: p.name().to_string(),
            version: p.version().unwrap_or("").to_string(),
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::UrlPattern,
                confidence: 1.0,
                source_connection_ids: vec![],
                source_file_paths: vec![],
                deps_dev_match: None,
            },
            licenses: vec![],
            concluded_licenses: Vec::new(),
            hashes: vec![],
            supplier: None,
            cpes: vec![],
            advisories: vec![],
            occurrences: vec![],
            lifecycle_scope: None,
            build_inclusion: None,
            requirement_ranges: Vec::new(),
            source_type: None,
            sbom_tier: None,
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
            extra_annotations: ann,
            binary_role: None,
        }
    }

    fn with_annotation(
        mut c: ResolvedComponent,
        key: &str,
        val: Value,
    ) -> ResolvedComponent {
        c.extra_annotations.insert(key.to_string(), val);
        c
    }

    fn with_source_dir(mut c: ResolvedComponent, path: &str) -> ResolvedComponent {
        c.evidence.source_file_paths = vec![format!("{path}/Cargo.toml")];
        c
    }

    #[test]
    fn all_mode_zero_op_preserves_inputs() {
        let comp = vec![mk_component("pkg:cargo/root@0.1.0", Some("main-module"))];
        let rel: Vec<Relationship> = vec![];
        let scan_root = PathBuf::from(".");
        let (out_c, out_r, report) =
            apply_scope_filter(comp.clone(), rel, ProjectDiscoveryMode::All, &scan_root);
        assert_eq!(out_c.len(), comp.len());
        assert!(out_r.is_empty());
        assert_eq!(report.mode, ProjectDiscoveryMode::All);
        assert_eq!(report.root_main_modules, 0);
        assert_eq!(report.workspace_members_followed, 0);
        assert_eq!(report.nested_projects_ignored, 0);
    }

    #[test]
    fn root_only_drops_nested_independent_project() {
        // Root-level cargo main-module + nested npm main-module.
        let root = mk_component("pkg:cargo/root@0.1.0", Some("main-module"));
        let nested = with_source_dir(
            mk_component("pkg:npm/nested@0.1.0", Some("main-module")),
            "services/api",
        );
        let comp = vec![root, nested];
        let rel: Vec<Relationship> = vec![];
        let scan_root = std::env::current_dir().unwrap();
        let (out_c, _out_r, report) = apply_scope_filter(
            comp,
            rel,
            ProjectDiscoveryMode::RootOnly,
            &scan_root,
        );
        let purls: Vec<String> =
            out_c.iter().map(|c| c.purl.as_str().to_string()).collect();
        assert!(purls.iter().any(|p| p.starts_with("pkg:cargo/")));
        assert!(!purls.iter().any(|p| p.starts_with("pkg:npm/")));
        assert_eq!(report.root_main_modules, 1);
        assert_eq!(report.nested_projects_ignored, 1);
    }

    #[test]
    fn root_only_preserves_workspace_member_via_annotation() {
        // Root cargo workspace + one workspace-member (not
        // BFS-reachable — no dep edge from root; tagged only via
        // annotation).
        let root_purl = "pkg:cargo/wsroot@0.1.0";
        let root = mk_component(root_purl, Some("main-module"));
        let member = with_annotation(
            mk_component("pkg:cargo/api@0.1.0", None),
            WORKSPACE_MEMBER_KEY,
            json!(root_purl),
        );
        let comp = vec![root, member];
        let rel: Vec<Relationship> = vec![];
        let scan_root = std::env::current_dir().unwrap();
        let (out_c, _out_r, report) = apply_scope_filter(
            comp,
            rel,
            ProjectDiscoveryMode::RootOnly,
            &scan_root,
        );
        let purls: Vec<String> =
            out_c.iter().map(|c| c.purl.as_str().to_string()).collect();
        assert!(purls.contains(&root_purl.to_string()));
        assert!(purls.contains(&"pkg:cargo/api@0.1.0".to_string()));
        assert_eq!(report.workspace_members_followed, 1);
    }

    #[test]
    fn strict_drops_workspace_member_even_if_annotated() {
        // Same shape as the preceding test — but Strict must drop the
        // annotated member.
        let root_purl = "pkg:cargo/wsroot@0.1.0";
        let root = mk_component(root_purl, Some("main-module"));
        let member = with_annotation(
            mk_component("pkg:cargo/api@0.1.0", None),
            WORKSPACE_MEMBER_KEY,
            json!(root_purl),
        );
        let comp = vec![root, member];
        let rel: Vec<Relationship> = vec![];
        let scan_root = std::env::current_dir().unwrap();
        let (out_c, _out_r, report) = apply_scope_filter(
            comp,
            rel,
            ProjectDiscoveryMode::Strict,
            &scan_root,
        );
        let purls: Vec<String> =
            out_c.iter().map(|c| c.purl.as_str().to_string()).collect();
        assert!(purls.contains(&root_purl.to_string()));
        assert!(!purls.contains(&"pkg:cargo/api@0.1.0".to_string()));
        assert_eq!(report.workspace_members_followed, 0);
    }

    #[test]
    fn report_counters_populated_under_non_default() {
        let root = mk_component("pkg:cargo/root@0.1.0", Some("main-module"));
        let nested1 = with_source_dir(
            mk_component("pkg:npm/n1@0.1.0", Some("main-module")),
            "svc/a",
        );
        let nested2 = with_source_dir(
            mk_component("pkg:golang/example.com/n2@v0.1.0", Some("main-module")),
            "svc/b",
        );
        let comp = vec![root, nested1, nested2];
        let scan_root = std::env::current_dir().unwrap();
        let (_c, _r, report) = apply_scope_filter(
            comp,
            vec![],
            ProjectDiscoveryMode::RootOnly,
            &scan_root,
        );
        assert_eq!(report.mode, ProjectDiscoveryMode::RootOnly);
        assert_eq!(report.root_main_modules, 1);
        assert_eq!(report.nested_projects_ignored, 2);
    }

    #[test]
    fn belt_and_suspenders_annotation_pass_covers_orphan_members() {
        // Cargo workspace-shape: root has [workspace] members but no
        // dep edge to them; annotation is the only signal. Under
        // RootOnly, filter picks the member up via Step 3.
        let root_purl = "pkg:cargo/wsroot@0.1.0";
        let root = mk_component(root_purl, Some("main-module"));
        let orphan_member = with_annotation(
            mk_component("pkg:cargo/api@0.1.0", None),
            WORKSPACE_MEMBER_KEY,
            json!(root_purl),
        );
        let comp = vec![root, orphan_member];
        let scan_root = std::env::current_dir().unwrap();
        let (out_c, _out_r, _r) = apply_scope_filter(
            comp,
            vec![],
            ProjectDiscoveryMode::RootOnly,
            &scan_root,
        );
        assert_eq!(out_c.len(), 2, "orphan member should be retained via annotation pass");
    }

    #[test]
    fn nested_workspace_fixpoint_recursion() {
        // Outer workspace root + inner workspace root (itself a
        // member of the outer) + inner-workspace member. Under
        // RootOnly, fixpoint pass should pull the inner member in
        // via the second iteration.
        let outer_purl = "pkg:cargo/outer@0.1.0";
        let inner_purl = "pkg:cargo/inner@0.1.0";
        let outer_root = mk_component(outer_purl, Some("main-module"));
        let inner_root = with_annotation(
            mk_component(inner_purl, Some("main-module")),
            WORKSPACE_MEMBER_KEY,
            json!(outer_purl),
        );
        let inner_member = with_annotation(
            mk_component("pkg:cargo/inner-sub@0.1.0", None),
            WORKSPACE_MEMBER_KEY,
            json!(inner_purl),
        );
        let comp = vec![outer_root, inner_root, inner_member];
        let scan_root = std::env::current_dir().unwrap();
        let (out_c, _out_r, report) = apply_scope_filter(
            comp,
            vec![],
            ProjectDiscoveryMode::RootOnly,
            &scan_root,
        );
        let purls: Vec<String> =
            out_c.iter().map(|c| c.purl.as_str().to_string()).collect();
        assert!(purls.contains(&outer_purl.to_string()));
        assert!(purls.contains(&inner_purl.to_string()));
        assert!(
            purls.contains(&"pkg:cargo/inner-sub@0.1.0".to_string()),
            "FR-005 fixpoint should pull inner workspace member"
        );
        assert_eq!(report.workspace_members_followed, 2);
    }
}
