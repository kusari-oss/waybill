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
/// The workspace-provenance annotation key stamped by m176's
/// `scan_fs::tag_components_with_workspace_member` on every component
/// whose evidence points at a manifest inside the scan root.
///
/// **Value shape**: a JSON-encoded array of scan-root-relative
/// workspace DIRECTORIES — `"[\".\"]"`, `"[\"services/api\"]"` — each
/// derived from the parent directory of one of the component's own
/// `evidence.source_file_paths` (root-level manifests use the `"."`
/// sentinel). It is self-descriptive provenance, NOT a back-reference
/// to a workspace root's PURL, and it is stamped on plain transitive
/// dependencies too — so membership is decided by directory identity,
/// not by matching a root's identifier.
///
/// That shape is exactly what makes it useful here: a cargo workspace
/// member's only evidence path is the shared workspace `Cargo.lock`,
/// so root and members alike carry `"[\".\"]"` and are retained
/// together under `RootOnly`, while an independent nested project
/// carries its own subdirectory and is not.
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
    declared_member_dirs: &[String],
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
    let mut in_scope_roots: Vec<_> = all_roots
        .iter()
        .filter(|r| mode.is_root_in_scope(r, scan_root))
        .cloned()
        .collect();

    // Step 1a (spec 1652, FR-003): under `RootOnly`, a package a root
    // declaration names (`cabal.project` / `stack.yaml` `packages:`, resolved
    // by the reader into directories) is a member of the root project, and is
    // projected like a root. With no root package, the declared ones ARE the
    // roots, so a declaration alone sets the scope (Clarification 1).
    // `Strict` drops members, so it ignores declarations (Clarification 2).
    let mut declared_roots_added = false;
    if mode.follows_workspace_members() && !declared_member_dirs.is_empty() {
        let declared: BTreeSet<&str> = declared_member_dirs.iter().map(String::as_str).collect();
        let already: BTreeSet<String> =
            in_scope_roots.iter().map(|r| r.purl_string.clone()).collect();
        for r in &all_roots {
            if already.contains(&r.purl_string) {
                continue;
            }
            if declared.contains(dir_key(&super::root_dir(r, scan_root)).as_str()) {
                in_scope_roots.push(r.clone());
                declared_roots_added = true;
            }
        }
    }

    // Step 1b (FR-007, Strict only): separate the workspace ROOT from
    // its workspace MEMBERS.
    //
    // Why this needs its own pass: a cargo workspace member's only
    // evidence path is the SHARED workspace `Cargo.lock` (m064's
    // augment-in-place), so `source_dir` is empty for the root AND
    // every member alike — the Step-1 root-level test cannot tell them
    // apart. m201 already solved exactly this collision for root
    // election, and `waybill:is-workspace-root` is the signal it
    // produced: stamped `true` only when a crate's own manifest
    // declares both `[package]` and `[workspace]`, `false` for every
    // other cargo main-module. Reusing it keeps FR-006's "no new
    // per-ecosystem detection heuristics" promise intact.
    //
    // Degradation is deliberate: when NO in-scope root carries the
    // flag — single-crate projects, virtual workspace manifests (no
    // root `[package]` to represent), and every non-cargo ecosystem —
    // the set is left alone so Strict collapses to RootOnly rather
    // than emptying the SBOM.
    if mode == ProjectDiscoveryMode::Strict {
        let workspace_root_purls: BTreeSet<String> = components
            .iter()
            .filter(|c| is_workspace_root(c))
            .map(|c| c.purl.to_string())
            .collect();
        if in_scope_roots
            .iter()
            .any(|r| workspace_root_purls.contains(&r.purl_string))
        {
            in_scope_roots
                .retain(|r| workspace_root_purls.contains(&r.purl_string));
        }
    }

    // Nothing in scope: return the inputs untouched (issue #1045).
    //
    // This is the FR-008 "zero root-level manifests" case, and the filter
    // answers it here rather than leaving the caller to undo the filtering
    // afterwards. It could not: `apply_scope_filter` consumes its inputs,
    // and running it again with `All` — which is what the caller used to do
    // — is a zero-op over whatever it is handed, so the already-dropped
    // components never came back. A project whose only manifest is
    // `requirements.txt` produces no main-module at all, so it took this
    // path and emitted a single file-tier component while WARNing that it
    // had fallen back to full scope. Reported in production against 0.9.0:
    // a project with 7 pinned packages, all carrying known CVEs, ingested
    // with 0 dependencies and 0 vulnerabilities, and nothing errored.
    //
    // Returning early makes that failure unreachable rather than merely
    // fixed at one call site: there is no longer a state in which the
    // filter has dropped components it has no roots to justify dropping.
    //
    // The counts describe what the filter DID, which is nothing — so
    // `nested_projects_ignored` is 0 even when `all_roots` is non-empty.
    // Those roots were found and then emitted, not ignored.
    if in_scope_roots.is_empty() {
        return (
            components,
            relationships,
            ProjectDiscoveryReport {
                mode,
                root_main_modules: 0,
                workspace_members_followed: 0,
                nested_projects_ignored: 0,
                dropped_main_modules: Vec::new(),
                dropped_evidence_paths: Vec::new(),
            },
        );
    }

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

    // Step 3: directory-scoped follow-up. BFS alone under-collects:
    // plenty of readers record a dependency without emitting a
    // `dependsOn` edge to it (a Gemfile.lock gem, a Cargo
    // `[workspace] members` entry), leaving it a graph orphan that
    // Step 2 would discard even though it plainly belongs to the root
    // project. The `waybill:workspace-member` directory tag is what
    // recovers those.
    //
    // The two non-default modes differ only in whether that recovery
    // is allowed to cross a main-module boundary:
    //   * RootOnly  — yes: a sibling main-module sharing an in-scope
    //     directory IS a workspace member (FR-004), and once pulled in
    //     it seeds the FR-005 fixpoint for its own sub-members.
    //   * Strict    — no: workspace members drop (FR-007). Only the
    //     root project's own non-main-module dependencies ride along,
    //     and there is no fixpoint.
    let mut workspace_members_followed = 0usize;
    {
        // Seed the in-scope directory set from the in-scope roots. The
        // root's own `waybill:workspace-member` value is preferred
        // (self-consistent with what members carry); `source_dir` is
        // the fallback for roots the m176 tagging pass didn't reach.
        let root_purls: BTreeSet<&str> = in_scope_roots
            .iter()
            .map(|r| r.purl_string.as_str())
            .collect();
        let mut in_scope_dirs: BTreeSet<String> = components
            .iter()
            .filter(|c| root_purls.contains(c.purl.as_str()))
            .flat_map(member_dirs)
            .collect();
        // Spec 1652: by directory, not `source_dir`, which is a manifest file
        // for an ecosystem `split::is_manifest_basename` does not list.
        in_scope_dirs.extend(
            in_scope_roots
                .iter()
                .map(|r| dir_key(&super::root_dir(r, scan_root))),
        );
        // Spec 1652 (T032): a root declaration is a manifest at the scan
        // root, so the root directory is the root project's even when no
        // package lives there. Without this, a declaration-only repository
        // lost its root-level components — haskell-security-advisories all
        // three `flake.lock` inputs, hs-opentelemetry its devenv inputs.
        if declared_roots_added {
            in_scope_dirs.insert(".".to_string());
        }

        let follows_members = mode.follows_workspace_members();
        annotation_follow_up(
            &components,
            &in_scope_dirs,
            !follows_members,
            &mut reachable,
            &mut workspace_members_followed,
        );

        // FR-005 nested-workspace fixpoint: any reachable component
        // tagged `waybill:component-role = main-module` is itself a
        // (possibly nested) workspace root. Fold ITS workspace
        // directories into `in_scope_dirs` and re-run the annotation
        // pass so that sub-workspace members living under that
        // directory are followed too. Repeat until no new directory
        // appears. Terminates because each iteration only adds
        // directories drawn from a bounded component set.
        // Strict never follows members, so it has no fixpoint at all.
        if follows_members {
            loop {
                let mut newly_added: BTreeSet<String> = BTreeSet::new();
                for c in &components {
                    if !reachable.contains(c.purl.as_str()) || !is_main_module(c) {
                        continue;
                    }
                    for d in member_dirs(c) {
                        if !in_scope_dirs.contains(&d) {
                            newly_added.insert(d);
                        }
                    }
                }
                if newly_added.is_empty() {
                    break;
                }
                in_scope_dirs.extend(newly_added);
                annotation_follow_up(
                    &components,
                    &in_scope_dirs,
                    false,
                    &mut reachable,
                    &mut workspace_members_followed,
                );
            }
        }
    }

    // Step 3b (spec 1652, T033): a component that is not a main module, has
    // no directory provenance (`waybill:workspace-member` absent) and touches
    // no relationship cannot be attributed to any project, so the scope has
    // no evidence to drop it. On the corpus these are exactly the Haskell
    // reader's Stack snapshot and compiler components (`stackage-lts-*`,
    // `ghc-*`, `snapshot.yaml`), which the root `stack.yaml` names; dropping
    // them made `root-only` lose the root project's toolchain silently.
    {
        let touched: BTreeSet<&str> = relationships
            .iter()
            .flat_map(|r| [r.from.as_str(), r.to.as_str()])
            .collect();
        for c in &components {
            let key = c.purl.as_str();
            if !reachable.contains(key)
                && !is_main_module(c)
                && !c.extra_annotations.contains_key(WORKSPACE_MEMBER_KEY)
                && !touched.contains(key)
            {
                reachable.insert(key.to_string());
            }
        }
    }

    // Step 4 (spec 1652, FR-004): what was actually dropped. Decided after
    // the projection and follow-up, because the projection keeps a nested
    // main module the root reaches through an edge, so "out of scope" and
    // "dropped" differ. Taken before Step 5 consumes the components, because
    // C200 (FR-006) needs each dropped main module's manifest path.
    let mut dropped_main_modules: Vec<String> = all_roots
        .iter()
        .filter(|r| !reachable.contains(r.purl.as_str()))
        .map(|r| r.purl_string.clone())
        .collect();
    dropped_main_modules.sort();
    dropped_main_modules.dedup();
    let dropped_set: BTreeSet<&str> = dropped_main_modules.iter().map(String::as_str).collect();
    let mut dropped_evidence_paths: Vec<String> = components
        .iter()
        .filter(|c| is_main_module(c) && dropped_set.contains(c.purl.as_str()))
        .flat_map(|c| c.evidence.source_file_paths.iter().cloned())
        .collect();
    dropped_evidence_paths.sort();
    dropped_evidence_paths.dedup();

    // Step 5: filter component + relationship slices (order-preserving).
    let filtered_components: Vec<ResolvedComponent> = components
        .into_iter()
        .filter(|c| reachable.contains(c.purl.as_str()))
        .collect();
    let filtered_relationships: Vec<Relationship> = relationships
        .into_iter()
        .filter(|r| reachable.contains(&r.from) && reachable.contains(&r.to))
        .collect();

    // Step 6: build report.
    let report = ProjectDiscoveryReport {
        mode,
        root_main_modules: in_scope_roots.len(),
        workspace_members_followed,
        nested_projects_ignored: dropped_main_modules.len(),
        dropped_main_modules,
        dropped_evidence_paths,
    };
    (filtered_components, filtered_relationships, report)
}

/// Read the m201 `waybill:is-workspace-root` flag. Stamped on every
/// main-module by `scan_fs::tag_main_modules_with_workspace_root`;
/// absent or non-boolean reads as `false`.
fn is_workspace_root(c: &ResolvedComponent) -> bool {
    c.extra_annotations
        .get(crate::generate::root_selector::IS_WORKSPACE_ROOT_KEY)
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Normalize a scan-root-relative directory into the same key space
/// m176's `derive_workspace_root` emits: the scan root itself is the
/// `"."` sentinel, everything else is forward-slash separated.
fn dir_key(rel: &Path) -> String {
    let s = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    if s.is_empty() { ".".to_string() } else { s }
}

/// Decode a `waybill:workspace-member` value into the scan-root-relative
/// workspace directories it names. Canonical shape is a JSON-encoded
/// array carried in a string (`"[\".\"]"`); a bare JSON array is
/// tolerated so a future encoding relaxation doesn't silently
/// no-op this pass. Anything else yields no directories.
fn member_dirs(c: &ResolvedComponent) -> BTreeSet<String> {
    match c.extra_annotations.get(WORKSPACE_MEMBER_KEY) {
        Some(serde_json::Value::String(s)) => {
            serde_json::from_str::<Vec<String>>(s).unwrap_or_default()
        }
        Some(serde_json::Value::Array(a)) => a
            .iter()
            .filter_map(|e| e.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
    .into_iter()
    .collect()
}

/// Is this component a per-ecosystem main-module (i.e. a workspace
/// member or standalone project root)? Same signal m215's split axis
/// uses.
fn is_main_module(c: &ResolvedComponent) -> bool {
    matches!(
        c.extra_annotations.get(COMPONENT_ROLE_KEY),
        Some(v) if v.as_str() == Some(MAIN_MODULE_ROLE)
    )
}

/// One directory-scoped follow-up pass. Walks `components`, retaining
/// any not-yet-reachable component whose `waybill:workspace-member`
/// annotation names a directory in `in_scope_dirs`. Bumps
/// `workspace_members_followed` per newly included component.
///
/// Directory identity — not PURL back-reference — is the join key,
/// because that is what the annotation actually carries (see
/// [`WORKSPACE_MEMBER_KEY`]). This is what lets a cargo workspace
/// member (whose only evidence is the shared root `Cargo.lock`, hence
/// `"."`) ride along with its root while an independent nested project
/// under its own subdirectory does not.
///
/// `skip_main_modules` is the Strict-mode lever: with it set, a
/// component sharing an in-scope directory is retained only if it is
/// NOT itself a main-module — the root project's own dependencies come
/// along, its workspace members do not (FR-007).
fn annotation_follow_up(
    components: &[ResolvedComponent],
    in_scope_dirs: &BTreeSet<String>,
    skip_main_modules: bool,
    reachable: &mut BTreeSet<String>,
    workspace_members_followed: &mut usize,
) {
    for c in components {
        if reachable.contains(c.purl.as_str()) {
            continue;
        }
        if skip_main_modules && is_main_module(c) {
            continue;
        }
        if member_dirs(c).iter().any(|d| in_scope_dirs.contains(d)) {
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

    use serde_json::Value;
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
                also: Vec::new(),
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
            artifact_presence: None,
            scope_evidence: None,
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

    /// Stamp `waybill:workspace-member` the way m176 really does: a
    /// JSON-encoded array of scan-root-relative workspace directories.
    fn with_member_dirs(c: ResolvedComponent, dirs: &[&str]) -> ResolvedComponent {
        let encoded = serde_json::to_string(dirs).unwrap();
        with_annotation(c, WORKSPACE_MEMBER_KEY, Value::String(encoded))
    }

    #[test]
    fn all_mode_zero_op_preserves_inputs() {
        let comp = vec![mk_component("pkg:cargo/root@0.1.0", Some("main-module"))];
        let rel: Vec<Relationship> = vec![];
        let scan_root = PathBuf::from(".");
        let (out_c, out_r, report) =
            apply_scope_filter(comp.clone(), rel, ProjectDiscoveryMode::All, &scan_root, &[]);
        assert_eq!(out_c.len(), comp.len());
        assert!(out_r.is_empty());
        assert_eq!(report.mode, ProjectDiscoveryMode::All);
        assert_eq!(report.root_main_modules, 0);
        assert_eq!(report.workspace_members_followed, 0);
        assert_eq!(report.nested_projects_ignored, 0);
    }

    /// Issue #1045 — no in-scope roots means no filtering, not empty output.
    ///
    /// The caller cannot repair this after the fact: `apply_scope_filter`
    /// consumes its inputs, so a second call with `All` operates on what the
    /// first call already dropped. That was the shipped behaviour, and it
    /// emptied the SBOM of a `requirements.txt`-only project while logging
    /// that it had fallen back to full scope.
    #[test]
    fn zero_in_scope_roots_returns_the_inputs_untouched() {
        // A dependency with no main-module anywhere — the requirements.txt
        // shape, where no reader elects a root.
        let dep = mk_component("pkg:pypi/waybill-fixture-alpha@2.25.1", None);
        let comps = vec![dep];
        let scan_root = PathBuf::from(".");
        let (out_c, out_r, report) = apply_scope_filter(
            comps.clone(),
            vec![],
            ProjectDiscoveryMode::RootOnly,
            &scan_root,
            &[],
        );
        // CONTROL: this is the zero-root path, not a path where a root was
        // found and everything happened to survive.
        assert_eq!(report.root_main_modules, 0);
        assert_eq!(
            out_c.len(),
            comps.len(),
            "the component was dropped with no root to justify dropping it"
        );
        assert!(out_r.is_empty());
        // Nothing was ignored, because nothing was filtered.
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
            &[],
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
        let root =
            with_member_dirs(mk_component(root_purl, Some("main-module")), &["."]);
        let member =
            with_member_dirs(mk_component("pkg:cargo/api@0.1.0", None), &["."]);
        let comp = vec![root, member];
        let rel: Vec<Relationship> = vec![];
        let scan_root = std::env::current_dir().unwrap();
        let (out_c, _out_r, report) = apply_scope_filter(
            comp,
            rel,
            ProjectDiscoveryMode::RootOnly,
            &scan_root,
            &[],
        );
        let purls: Vec<String> =
            out_c.iter().map(|c| c.purl.as_str().to_string()).collect();
        assert!(purls.contains(&root_purl.to_string()));
        assert!(purls.contains(&"pkg:cargo/api@0.1.0".to_string()));
        assert_eq!(report.workspace_members_followed, 1);
    }

    #[test]
    fn strict_drops_workspace_member_even_if_annotated() {
        // Real cargo workspace shape: root and member are BOTH
        // root-level main-modules sharing the workspace `Cargo.lock`
        // (so both are annotated `["."]`), told apart only by m201's
        // `waybill:is-workspace-root`. Strict keeps the root and the
        // root's own plain dependency; the member drops (FR-007).
        let root_purl = "pkg:cargo/wsroot@0.1.0";
        let ws_root_key = crate::generate::root_selector::IS_WORKSPACE_ROOT_KEY;
        let root = with_annotation(
            with_member_dirs(mk_component(root_purl, Some("main-module")), &["."]),
            ws_root_key,
            Value::Bool(true),
        );
        let member = with_annotation(
            with_member_dirs(
                mk_component("pkg:cargo/api@0.1.0", Some("main-module")),
                &["."],
            ),
            ws_root_key,
            Value::Bool(false),
        );
        let dep = with_member_dirs(mk_component("pkg:cargo/serde@1.0.0", None), &["."]);
        let comp = vec![root, member, dep];
        let rel: Vec<Relationship> = vec![];
        let scan_root = std::env::current_dir().unwrap();
        let (out_c, _out_r, _report) = apply_scope_filter(
            comp,
            rel,
            ProjectDiscoveryMode::Strict,
            &scan_root,
            &[],
        );
        let purls: Vec<String> =
            out_c.iter().map(|c| c.purl.as_str().to_string()).collect();
        assert!(purls.contains(&root_purl.to_string()));
        assert!(
            !purls.contains(&"pkg:cargo/api@0.1.0".to_string()),
            "SC-006: workspace member must drop under Strict"
        );
        assert!(
            purls.contains(&"pkg:cargo/serde@1.0.0".to_string()),
            "the root project's own dependency must survive Strict"
        );
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
            &[],
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
        let root =
            with_member_dirs(mk_component(root_purl, Some("main-module")), &["."]);
        let orphan_member =
            with_member_dirs(mk_component("pkg:cargo/api@0.1.0", None), &["."]);
        let comp = vec![root, orphan_member];
        let scan_root = std::env::current_dir().unwrap();
        let (out_c, _out_r, _r) = apply_scope_filter(
            comp,
            vec![],
            ProjectDiscoveryMode::RootOnly,
            &scan_root,
            &[],
        );
        assert_eq!(out_c.len(), 2, "orphan member should be retained via annotation pass");
    }

    #[test]
    fn nested_workspace_fixpoint_recursion() {
        // Outer workspace root at the scan root + inner workspace root
        // one directory down (reachable from the outer via a dep edge)
        // + a member of that inner workspace. The inner member's
        // directory is NOT in scope on the first annotation pass — only
        // after the fixpoint folds the inner root's own directory in.
        let outer_purl = "pkg:cargo/outer@0.1.0";
        let inner_purl = "pkg:cargo/inner@0.1.0";
        let scan_root = std::env::current_dir().unwrap();
        let outer_root =
            with_member_dirs(mk_component(outer_purl, Some("main-module")), &["."]);
        let mut inner_root = with_member_dirs(
            mk_component(inner_purl, Some("main-module")),
            &["crates/inner"],
        );
        // Nest the inner workspace root one directory down so it is NOT
        // itself root-level — it enters scope only via the dep edge.
        inner_root.evidence.source_file_paths = vec![
            scan_root
                .join("crates/inner/Cargo.toml")
                .to_string_lossy()
                .into_owned(),
        ];
        let inner_member = with_member_dirs(
            mk_component("pkg:cargo/inner-sub@0.1.0", None),
            &["crates/inner"],
        );
        let edge = Relationship {
            from: outer_purl.to_string(),
            to: inner_purl.to_string(),
            relationship_type:
                waybill_common::resolution::RelationshipType::DependsOn,
            provenance: waybill_common::resolution::EnrichmentProvenance {
                source: "Cargo.lock".to_string(),
                data_type: "dependency".to_string(),
            },
        };
        let comp = vec![outer_root, inner_root, inner_member];
        let (out_c, _out_r, report) = apply_scope_filter(
            comp,
            vec![edge],
            ProjectDiscoveryMode::RootOnly,
            &scan_root,
            &[],
        );
        let purls: Vec<String> =
            out_c.iter().map(|c| c.purl.as_str().to_string()).collect();
        assert!(purls.contains(&outer_purl.to_string()));
        assert!(purls.contains(&inner_purl.to_string()));
        assert!(
            purls.contains(&"pkg:cargo/inner-sub@0.1.0".to_string()),
            "FR-005 fixpoint should pull inner workspace member"
        );
        // Only the inner member arrives via the annotation pass; the
        // inner root itself came in through BFS.
        assert_eq!(report.workspace_members_followed, 1);
    }

    // ---------------- spec 1652: the drop set (FR-004, research R3) ----------------

    fn dep(from: &str, to: &str) -> Relationship {
        Relationship {
            from: from.to_string(),
            to: to.to_string(),
            relationship_type: waybill_common::resolution::RelationshipType::DependsOn,
            provenance: waybill_common::resolution::EnrichmentProvenance {
                source: "test".to_string(),
                data_type: "test".to_string(),
            },
        }
    }

    fn nested(purl: &str, dir: &str) -> ResolvedComponent {
        with_source_dir(mk_component(purl, Some("main-module")), dir)
    }

    fn purls_of(cs: &[ResolvedComponent]) -> BTreeSet<String> {
        cs.iter().map(|c| c.purl.as_str().to_string()).collect()
    }

    /// Invariants I2 and I3 (`contracts/scope-filter.md`): every dropped PURL
    /// was an input main module and is absent from the output; every input
    /// main module is in exactly one of the two.
    fn assert_partition(
        input: &[ResolvedComponent],
        out: &[ResolvedComponent],
        report: &ProjectDiscoveryReport,
    ) {
        let kept = purls_of(out);
        let dropped: BTreeSet<String> = report.dropped_main_modules.iter().cloned().collect();
        assert_eq!(dropped.len(), report.dropped_main_modules.len(), "no duplicates");
        assert_eq!(report.nested_projects_ignored, dropped.len());
        let mains: BTreeSet<String> = input
            .iter()
            .filter(|c| is_main_module(c))
            .map(|c| c.purl.as_str().to_string())
            .collect();
        for d in &dropped {
            assert!(mains.contains(d), "I2: {d} was not an input main module");
            assert!(!kept.contains(d), "I2: {d} is both dropped and kept");
        }
        for m in &mains {
            assert!(
                kept.contains(m) ^ dropped.contains(m),
                "I3: {m} must be kept or dropped, exactly one"
            );
        }
        let mut sorted = report.dropped_main_modules.clone();
        sorted.sort();
        assert_eq!(sorted, report.dropped_main_modules, "sorted");
    }

    /// R3: the old count was "roots out of scope", which counted a nested
    /// main module the root reaches through an edge. It reported 2 here.
    #[test]
    fn dropped_set_excludes_main_modules_reached_through_edges() {
        let root = mk_component("pkg:cargo/root@0.1.0", Some("main-module"));
        let reached = nested("pkg:cargo/reached@0.1.0", "crates/reached");
        let alone = nested("pkg:npm/alone@0.1.0", "services/alone");
        let input = vec![root, reached, alone];
        let (out, _r, report) = apply_scope_filter(
            input.clone(),
            vec![dep("pkg:cargo/root@0.1.0", "pkg:cargo/reached@0.1.0")],
            ProjectDiscoveryMode::RootOnly,
            &std::env::current_dir().unwrap(),
            &[],
        );
        assert_eq!(report.dropped_main_modules, vec!["pkg:npm/alone@0.1.0".to_string()]);
        assert_eq!(report.nested_projects_ignored, 1);
        assert!(purls_of(&out).contains("pkg:cargo/reached@0.1.0"));
        assert_partition(&input, &out, &report);
    }

    #[test]
    fn cycle_between_nested_main_modules_terminates_and_keeps_both() {
        let root = mk_component("pkg:cargo/root@0.1.0", Some("main-module"));
        let a = nested("pkg:cargo/a@0.1.0", "crates/a");
        let b = nested("pkg:cargo/b@0.1.0", "crates/b");
        let input = vec![root, a, b];
        let rels = vec![
            dep("pkg:cargo/root@0.1.0", "pkg:cargo/a@0.1.0"),
            dep("pkg:cargo/a@0.1.0", "pkg:cargo/b@0.1.0"),
            dep("pkg:cargo/b@0.1.0", "pkg:cargo/a@0.1.0"),
        ];
        for mode in [ProjectDiscoveryMode::RootOnly, ProjectDiscoveryMode::Strict] {
            let (out, out_r, report) = apply_scope_filter(
                input.clone(),
                rels.clone(),
                mode,
                &std::env::current_dir().unwrap(),
            &[],
            );
            let kept = purls_of(&out);
            assert!(kept.contains("pkg:cargo/a@0.1.0") && kept.contains("pkg:cargo/b@0.1.0"));
            assert_eq!(out_r.len(), 3, "{mode}: the cycle's edges are kept");
            assert!(report.dropped_main_modules.is_empty());
            assert_partition(&input, &out, &report);
        }
    }

    #[test]
    fn nested_chain_unreached_from_root_drops_whole_chain() {
        let root = mk_component("pkg:cargo/root@0.1.0", Some("main-module"));
        let x = nested("pkg:npm/x@0.1.0", "services/x");
        let y = nested("pkg:npm/y@0.1.0", "services/y");
        let input = vec![root, x, y];
        let (out, out_r, report) = apply_scope_filter(
            input.clone(),
            vec![dep("pkg:npm/x@0.1.0", "pkg:npm/y@0.1.0")],
            ProjectDiscoveryMode::RootOnly,
            &std::env::current_dir().unwrap(),
            &[],
        );
        assert_eq!(
            report.dropped_main_modules,
            vec!["pkg:npm/x@0.1.0".to_string(), "pkg:npm/y@0.1.0".to_string()]
        );
        assert!(out_r.is_empty());
        assert_partition(&input, &out, &report);
    }

    /// I2 and I3 over the shapes the milestone-220 tests above build.
    #[test]
    fn partition_holds_over_the_existing_shapes() {
        let cwd = std::env::current_dir().unwrap();
        let shapes: Vec<(Vec<ResolvedComponent>, Vec<Relationship>)> = vec![
            (
                vec![
                    mk_component("pkg:cargo/root@0.1.0", Some("main-module")),
                    nested("pkg:npm/nested@0.1.0", "services/api"),
                ],
                vec![],
            ),
            (
                vec![
                    with_member_dirs(mk_component("pkg:cargo/wsroot@0.1.0", Some("main-module")), &["."]),
                    with_member_dirs(mk_component("pkg:cargo/api@0.1.0", Some("main-module")), &["."]),
                    nested("pkg:gem/bench@0.1.0", "bench"),
                ],
                vec![],
            ),
        ];
        for (input, rels) in shapes {
            for mode in [ProjectDiscoveryMode::RootOnly, ProjectDiscoveryMode::Strict] {
                let (out, _r, report) =
                    apply_scope_filter(input.clone(), rels.clone(), mode, &cwd, &[]);
                assert_partition(&input, &out, &report);
            }
        }
    }

    // ---------------- spec 1652: declared members (FR-003) ----------------

    /// A main module whose evidence names a directory, so `root_dir` is it.
    fn in_dir(purl: &str, dir: &str) -> ResolvedComponent {
        let mut c = mk_component(purl, Some("main-module"));
        c.evidence.source_file_paths = vec![dir.to_string()];
        c
    }

    fn declared_shape() -> Vec<ResolvedComponent> {
        vec![
            in_dir("pkg:hackage/waybill-fixture-a@0.1.0", "local-packages/a"),
            in_dir("pkg:hackage/waybill-fixture-b@0.1.0", "local-packages/b"),
            in_dir("pkg:hackage/waybill-fixture-e@0.1.0", "tools/e"),
        ]
    }

    #[test]
    fn declared_members_are_roots_under_root_only() {
        let cwd = std::env::current_dir().unwrap();
        let input = declared_shape();
        let decl = vec!["local-packages/a".to_string(), "local-packages/b".to_string()];
        let (out, _r, report) =
            apply_scope_filter(input.clone(), vec![], ProjectDiscoveryMode::RootOnly, &cwd, &decl);
        assert_eq!(report.root_main_modules, 2, "no root package: the declared ones are the roots");
        assert_eq!(
            purls_of(&out),
            ["pkg:hackage/waybill-fixture-a@0.1.0", "pkg:hackage/waybill-fixture-b@0.1.0"]
                .iter()
                .map(|s| s.to_string())
                .collect()
        );
        assert_eq!(report.dropped_main_modules, vec!["pkg:hackage/waybill-fixture-e@0.1.0".to_string()]);
        assert_partition(&input, &out, &report);
    }

    /// I4: `strict` ignores declarations — the same output as with none.
    #[test]
    fn strict_ignores_declared_members() {
        let cwd = std::env::current_dir().unwrap();
        let mut input = declared_shape();
        input.insert(0, mk_component("pkg:npm/waybill-fixture-root-js@1.0.0", Some("main-module")));
        let decl = vec!["local-packages/a".to_string()];
        let with = apply_scope_filter(input.clone(), vec![], ProjectDiscoveryMode::Strict, &cwd, &decl);
        let without = apply_scope_filter(input, vec![], ProjectDiscoveryMode::Strict, &cwd, &[]);
        assert_eq!(purls_of(&with.0), purls_of(&without.0));
        assert_eq!(with.2.dropped_main_modules, without.2.dropped_main_modules);
        assert_eq!(with.2.dropped_main_modules.len(), 3);
    }

    /// I5: directories that name no main module change nothing — with no root
    /// package, that is the #1045 fall-back.
    #[test]
    fn unmatched_declared_dirs_are_a_no_op() {
        let cwd = std::env::current_dir().unwrap();
        let input = declared_shape();
        let decl = vec!["nowhere".to_string()];
        let (out, _r, report) =
            apply_scope_filter(input.clone(), vec![], ProjectDiscoveryMode::RootOnly, &cwd, &decl);
        assert_eq!(report.root_main_modules, 0);
        assert_eq!(out.len(), input.len(), "fell back to the full scan");
        assert!(report.dropped_main_modules.is_empty());
    }

    /// T032: a declaration-only scope keeps root-level (`["."]`) components.
    #[test]
    fn declaration_only_scope_keeps_root_directory_components() {
        let cwd = std::env::current_dir().unwrap();
        let mut input = declared_shape();
        input.push(with_member_dirs(
            mk_component("pkg:github/waybill-fixture-org/waybill-fixture-input@0123456", None),
            &["."],
        ));
        let decl = vec!["local-packages/a".to_string(), "local-packages/b".to_string()];
        let (out, _r, _report) =
            apply_scope_filter(input, vec![], ProjectDiscoveryMode::RootOnly, &cwd, &decl);
        assert!(purls_of(&out).contains("pkg:github/waybill-fixture-org/waybill-fixture-input@0123456"));
    }

    /// T033: no provenance and no relationship: kept, under both modes. With
    /// a relationship to a dropped project, it is attributable, and dropped.
    #[test]
    fn unattributable_components_are_kept_attributable_ones_are_not() {
        let cwd = std::env::current_dir().unwrap();
        let root = mk_component("pkg:cargo/root@0.1.0", Some("main-module"));
        let nested_p = nested("pkg:npm/alone@0.1.0", "services/alone");
        let loose = mk_component("pkg:generic/stackage-lts-22.0@unspecified", None);
        let owned = mk_component("pkg:npm/owned-dep@1.0.0", None);
        let input = vec![root, nested_p, loose, owned];
        let rels = vec![dep("pkg:npm/alone@0.1.0", "pkg:npm/owned-dep@1.0.0")];
        for mode in [ProjectDiscoveryMode::RootOnly, ProjectDiscoveryMode::Strict] {
            let (out, _r, report) = apply_scope_filter(input.clone(), rels.clone(), mode, &cwd, &[]);
            let kept = purls_of(&out);
            assert!(kept.contains("pkg:generic/stackage-lts-22.0@unspecified"), "{mode}: {kept:?}");
            assert!(!kept.contains("pkg:npm/owned-dep@1.0.0"), "{mode}: {kept:?}");
            assert_partition(&input, &out, &report);
        }
    }
}
