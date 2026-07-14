//! Shared test helpers for the `graph_completeness/` submodule
//! tests. `#[cfg(test)]`-only.
//!
//! Kept in a dedicated file so `bfs.rs` and `mod.rs` don't have to
//! duplicate the ~40-line `ResolvedComponent` boilerplate.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use mikebom_common::resolution::{
    EnrichmentProvenance, Relationship, RelationshipType, ResolutionEvidence, ResolutionTechnique,
    ResolvedComponent,
};
use mikebom_common::types::purl::Purl;
use serde_json::json;

use crate::generate::root_selector::{ResolvedRootSubject, RootSelectionResult};

pub(crate) fn mk_component(purl_str: &str) -> ResolvedComponent {
    let purl = Purl::new(purl_str).expect("valid purl");
    ResolvedComponent {
        build_inclusion: None,
        name: purl.name().to_string(),
        version: purl.version().unwrap_or("0.0.0").to_string(),
        purl,
        evidence: ResolutionEvidence {
            technique: ResolutionTechnique::PackageDatabase,
            confidence: 0.85,
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
        requirement_range: None,
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
        extra_annotations: Default::default(),
        binary_role: None,
    }
}

pub(crate) fn mk_main_module(purl_str: &str) -> ResolvedComponent {
    let mut c = mk_component(purl_str);
    c.extra_annotations.insert(
        "mikebom:component-role".to_string(),
        json!("main-module"),
    );
    c
}

pub(crate) fn mk_workspace_root(purl_str: &str) -> ResolvedComponent {
    let mut c = mk_main_module(purl_str);
    c.extra_annotations
        .insert("mikebom:is-workspace-root".to_string(), json!(true));
    c
}

pub(crate) fn mk_rel(from: &str, to: &str) -> Relationship {
    Relationship {
        from: from.to_string(),
        to: to.to_string(),
        relationship_type: RelationshipType::DependsOn,
        provenance: EnrichmentProvenance {
            source: "test-support".to_string(),
            data_type: "dependency-graph".to_string(),
        },
    }
}

pub(crate) fn selection_with_main_module(idx: usize) -> RootSelectionResult {
    RootSelectionResult {
        subject: ResolvedRootSubject::MainModule(idx),
        heuristic: None,
        losers: Vec::new(),
    }
}

/// Milestone 192 test helper — build a `RootSelectionResult` for the
/// operator-override branch. Used by tests that exercise the m192
/// per-ecosystem placeholder synthesis path.
pub(crate) fn selection_with_operator_override() -> RootSelectionResult {
    RootSelectionResult {
        subject: ResolvedRootSubject::OperatorOverride,
        heuristic: None,
        losers: Vec::new(),
    }
}
