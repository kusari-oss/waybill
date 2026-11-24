//! Spec 1541 (#1593), research R1: one reachability answer per document.
//!
//! Before this module the inputs to [`compute_graph_completeness`] were
//! prepared in four places: the CycloneDX builder, the SPDX 2.3 and SPDX 3
//! document builders, and the orphan classifier. The classifier skipped the
//! operator's root override and the anchoring rewrites, so on TheRock it
//! counted components reachable that the document counted unreachable, and
//! gave them no orphan reason. Every caller now prepares through
//! [`for_document`].

use waybill_common::resolution::{Relationship, ResolvedComponent};

use super::{
    anchor_resolve_components, anchor_retained_mainmod_edges, build_workspace_peer_edges,
    compute_graph_completeness, GraphCompletenessResult, UnreadManifests,
};
use crate::generate::root_selector::{
    apply_main_module_drop_or_demote, select_root, ResolvedRootSubject, RootSelectionHeuristic,
    RootSelectionResult, ScanTargetKind,
};
use crate::generate::RootComponentOverride;
use crate::scan_fs::package_db::maven::ScanTargetCoord;

/// What a document is about: the inputs that decide its root.
pub struct DocumentSubject<'a> {
    pub root_override: &'a RootComponentOverride,
    pub scan_target_coord: Option<&'a ScanTargetCoord>,
    pub target_name: &'a str,
    pub target_kind: ScanTargetKind,
}

/// The document's graph, prepared once.
pub struct DocumentGraph {
    /// The components after the override's main-module drop, when an
    /// override is active. `None` means the input components unchanged.
    effective_components: Option<Vec<ResolvedComponent>>,
    pub retained_main_module_purls: Vec<String>,
    /// #1626: see `root_selector::DropOrDemoteResult::subject_alias`.
    pub subject_alias: Option<String>,
    /// Spec 1618: the PURL of the main module the selector made the subject,
    /// if any.
    pub subject_main_module: Option<String>,
    /// The graph root's ref: the subject main module's PURL, or
    /// `<name>@<version>` for a synthetic subject.
    pub target_ref: String,
    pub workspace_peer_edges: Vec<Relationship>,
    pub completeness: GraphCompletenessResult,
}

impl DocumentGraph {
    pub fn effective<'a>(&'a self, components: &'a [ResolvedComponent]) -> &'a [ResolvedComponent] {
        self.effective_components.as_deref().unwrap_or(components)
    }
}

pub fn for_document(
    components: &[ResolvedComponent],
    relationships: &[Relationship],
    subject: &DocumentSubject<'_>,
    unread_manifests: UnreadManifests<'_>,
    scope_drop: Option<&crate::generate::project_discovery::ProjectDiscoveryDrop>,
) -> DocumentGraph {
    let root_override = subject.root_override;
    let drop_result = apply_main_module_drop_or_demote(components, root_override);
    let effective_components_owned = if root_override.is_active() {
        Some(drop_result.effective_components)
    } else {
        None
    };
    let retained_main_module_purls = drop_result.retained_main_module_purls;
    let subject_alias = drop_result.subject_alias;
    let effective_components: &[ResolvedComponent] =
        effective_components_owned.as_deref().unwrap_or(components);

    let selection = select_root(
        effective_components,
        root_override,
        subject.scan_target_coord,
        subject.target_name,
        "0.0.0",
        subject.target_kind,
    );

    let target_ref = target_ref_for(effective_components, &selection, subject);
    let subject_main_module = match &selection.subject {
        ResolvedRootSubject::MainModule(idx) => effective_components.get(*idx).map(|c| c.purl.as_str().to_string()),
        _ => None,
    };

    let workspace_peer_edges = build_workspace_peer_edges(&selection, effective_components);

    // Milestone 192: re-anchor a dropped main module's edges onto the root
    // before the BFS, or the root cannot reach the dependencies that hung
    // off it.
    let relationships: Vec<Relationship> = anchor_resolve_components(
        &anchor_retained_mainmod_edges(
            relationships,
            &retained_main_module_purls,
            subject_alias.as_deref(),
            &target_ref,
        ),
        effective_components,
        &target_ref,
    )
    .into_iter()
    .chain(workspace_peer_edges.iter().cloned())
    .collect();

    let completeness = super::apply_scope_drop(
        compute_graph_completeness(
            effective_components,
            &relationships,
            &selection,
            &target_ref,
            unread_manifests,
        ),
        scope_drop,
    );

    DocumentGraph {
        effective_components: effective_components_owned,
        retained_main_module_purls,
        subject_alias,
        subject_main_module,
        target_ref,
        workspace_peer_edges,
        completeness,
    }
}

fn target_ref_for(
    effective_components: &[ResolvedComponent],
    selection: &RootSelectionResult,
    subject: &DocumentSubject<'_>,
) -> String {
    let root_override = subject.root_override;
    // #1626: the resolved name and version, which `--root-purl` sets
    // through the PURL alone. The raw fields are `None` under it, and the
    // ref fell back to the directory name, matching no emitted root.
    let name = root_override
        .resolved_name()
        .unwrap_or(subject.target_name)
        .to_string();
    let version = root_override.resolved_version().unwrap_or("0.0.0").to_string();
    let main_module = match &selection.subject {
        ResolvedRootSubject::MainModule(idx) => effective_components.get(*idx),
        _ => None,
    };
    // Milestone 1377 (#1398): the evaluated Nix package is a component the
    // selector chose by tag, and roots the graph even for an image.
    let main_module_purl = if selection.heuristic == Some(RootSelectionHeuristic::NixEvaluatedPackage)
    {
        main_module
    } else if !root_override.is_active() && subject.target_kind != ScanTargetKind::ExplicitImage {
        // #1186: an explicitly named image is the subject, so an in-image
        // main module must not root the graph. #1572: the main module the
        // selector chose, not the first one found.
        main_module
    } else {
        None
    };
    match main_module_purl {
        Some(c) => c.purl.as_str().to_string(),
        None => format!("{name}@{version}"),
    }
}
