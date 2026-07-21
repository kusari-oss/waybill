//! Milestone 209: shared helpers used by URL-family resolvers.
//!
//! Every URL-family resolver (Cargo, PyPI, npm, Golang, Maven,
//! RubyGems, Deb) extracts a `Purl` from a `(hostname, path)` pair,
//! then wraps it into a `ResolvedComponent` populated with the
//! resolver's technique + confidence + connection provenance. This
//! module holds the wrapping helper so the per-resolver files stay
//! focused on ecosystem-specific extraction logic.

use std::collections::HashMap;

use waybill_common::attestation::file::FileOperation;
use waybill_common::attestation::network::Connection;
use waybill_common::resolution::{
    ResolutionEvidence, ResolutionTechnique, ResolvedComponent,
};
use waybill_common::types::hash::ContentHash;
use waybill_common::types::purl::Purl;

/// Build a `ResolvedComponent` for a URL-family match. Matches the
/// pre-refactor pipeline.rs URL-branch construction (lines 147-186)
/// byte-for-byte — every field defaulted to the same value so SC-001
/// byte-identity holds.
pub(super) fn build_url_component(
    purl: Purl,
    conn: &Connection,
    path: &str,
    basename_to_file_op: &HashMap<&str, &FileOperation>,
    technique: ResolutionTechnique,
    confidence: f64,
) -> ResolvedComponent {
    let url_basename = path.rsplit('/').next().unwrap_or("");
    let mut hashes = collect_connection_hashes(conn);
    let mut matched_file_paths: Vec<String> = Vec::new();
    if !url_basename.is_empty() {
        if let Some(op) = basename_to_file_op.get(url_basename) {
            if let Some(h) = &op.content_hash {
                hashes.push(h.clone());
            }
            matched_file_paths.push(op.path.clone());
        }
    }

    ResolvedComponent {
        name: purl.name().to_string(),
        version: purl.version().unwrap_or("").to_string(),
        purl,
        evidence: ResolutionEvidence {
            technique,
            confidence,
            source_connection_ids: vec![conn.id.clone()],
            source_file_paths: matched_file_paths,
            deps_dev_match: None,
        },
        licenses: vec![],
        concluded_licenses: Vec::new(),
        hashes,
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
        extra_annotations: Default::default(),
        binary_role: None,
    }
}

/// Collect content hashes from a connection's response. Mirrors
/// `pipeline::collect_connection_hashes` verbatim so SC-001 byte-
/// identity holds when the pipeline rewire routes through the chain.
pub(super) fn collect_connection_hashes(conn: &Connection) -> Vec<ContentHash> {
    conn.response
        .as_ref()
        .and_then(|r| r.content_hash.as_ref())
        .cloned()
        .into_iter()
        .collect()
}

/// Extract `(hostname, path)` from a `Connection` — the two inputs
/// every URL-family resolver's `handles()` + `resolve()` needs.
/// Returns `("", "")` when either is absent; resolvers treat empty
/// input as no-match.
pub(super) fn hostname_and_path(conn: &Connection) -> (&str, &str) {
    let hostname = conn
        .destination
        .hostname
        .as_deref()
        .or_else(|| conn.tls.as_ref().and_then(|t| t.sni.as_deref()))
        .unwrap_or("");

    let path = conn.request.as_ref().map(|r| r.path.as_str()).unwrap_or("");
    (hostname, path)
}
