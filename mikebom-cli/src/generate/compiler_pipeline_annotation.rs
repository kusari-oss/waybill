// Consumed by the per-format emitter augmentation (T030-T032) which
// hasn't landed yet — the module compiles + tests exercise every
// public function but nothing in the production code path calls into
// it until the emitters are wired. Same shape as the m208 ingest
// scaffolds which had this attribute until the emitters caught up.
#![allow(dead_code)]

//! Milestone 210 — write-set → SBOM component mapping (task T029).
//!
//! Given a `ResolvedComponent` and a `CompilerPipelineData`, produce
//! the payload for the per-component `mikebom:source-read-set`
//! annotation (C130) OR the fallback `mikebom:read-set-source`
//! annotation value (C131) per contracts/annotations.md A-1 / A-2.
//!
//! Mapping rule (Clarifications Q1):
//! - Compute the component's known file paths (from
//!   `occurrences[].location` + `evidence.source_file_paths[]`).
//! - For every compiler invocation whose `write_set` contains any
//!   of those file paths, take that invocation as a MATCH.
//! - Union the matched invocations' `read_set` with the transitive
//!   read_sets of all ancestor invocations in the DAG (walking
//!   `parent_invocation_id` chains).
//! - Sort the union deterministically by path per research R8.
//! - Emit the source-read-set payload (or `"unknown"` if no
//!   invocation matched per revised FR-015).

use std::collections::{BTreeMap, HashSet};

use mikebom_common::attestation::compiler_pipeline::{
    CompilerInvocation, CompilerPipelineData, ReadKind, ReadSetEntry,
};
use mikebom_common::resolution::ResolvedComponent;
use serde_json::{json, Value};

/// The value emitted for `mikebom:read-set-source` (catalog row C131).
/// Per contracts/annotations.md A-2 — MVP only emits `Traced` or
/// `Unknown`; `CacheHit` + `TraceAttachLate` are follow-up work.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReadSetSource {
    /// Component's file path intersects at least one compiler
    /// invocation's write-set — a genuine attribution.
    Traced,
    /// Component has no matching write-set. Per FR-015 revised, this
    /// includes cache-served components (MVP can't distinguish; the
    /// `CacheHit` value is reserved for a future milestone that adds
    /// cache-server hooks).
    Unknown,
}

impl ReadSetSource {
    pub(crate) fn as_wire_str(self) -> &'static str {
        match self {
            ReadSetSource::Traced => "traced",
            ReadSetSource::Unknown => "unknown",
        }
    }
}

/// Result of the mapping pass for one component.
pub(crate) struct ComponentReadSet {
    pub(crate) source: ReadSetSource,
    /// `Some(payload_value)` when `source == Traced` — the deterministic
    /// `mikebom:source-read-set` annotation payload.
    /// `None` when `source == Unknown` (per A-2: C130 is OMITTED for
    /// non-traced components; C131 alone signals the state).
    pub(crate) payload: Option<Value>,
}

/// Compute the source-read-set for a single component per Q1.
///
/// Returns `Traced { payload }` when at least one compiler invocation's
/// write-set intersects the component's file paths; `Unknown` otherwise.
pub(crate) fn map_component_to_source_read_set(
    component: &ResolvedComponent,
    pipeline: &CompilerPipelineData,
) -> ComponentReadSet {
    let component_paths = component_file_paths(component);
    if component_paths.is_empty() {
        return ComponentReadSet {
            source: ReadSetSource::Unknown,
            payload: None,
        };
    }

    // Find every invocation whose write_set contains any of the
    // component's paths.
    let matching_invocations: Vec<&CompilerInvocation> = pipeline
        .invocations
        .iter()
        .filter(|inv| {
            inv.write_set
                .iter()
                .any(|w| component_paths.contains(w.path.to_string_lossy().as_ref()))
        })
        .collect();

    if matching_invocations.is_empty() {
        return ComponentReadSet {
            source: ReadSetSource::Unknown,
            payload: None,
        };
    }

    // Transitive-closure over the DAG. For each matched invocation,
    // walk ancestor chains + union their read_sets.
    let by_id: BTreeMap<u64, &CompilerInvocation> = pipeline
        .invocations
        .iter()
        .map(|inv| (inv.invocation_id, inv))
        .collect();

    let mut visited: HashSet<u64> = HashSet::new();
    let mut invocation_ids: Vec<u64> = Vec::new();
    let mut union_read_set: BTreeMap<String, ReadSetEntry> = BTreeMap::new();

    for match_inv in &matching_invocations {
        walk_ancestors_collecting_reads(
            match_inv.invocation_id,
            &by_id,
            &mut visited,
            &mut invocation_ids,
            &mut union_read_set,
        );
    }

    // Deterministic ordering per R8.
    invocation_ids.sort_unstable();
    let read_set_sorted: Vec<&ReadSetEntry> = union_read_set.values().collect();

    // Build the wire payload per contracts/annotations.md A-1.
    let read_set_json: Vec<Value> = read_set_sorted
        .into_iter()
        .map(|entry| match &entry.kind {
            ReadKind::File => json!({
                "path": entry.path.to_string_lossy(),
                "sha256": entry.sha256.value.as_str(),
                "kind": "file",
            }),
            ReadKind::StdinInput { bytes_read } => json!({
                "path": entry.path.to_string_lossy(),
                "kind": { "stdin_input": { "bytes_read": bytes_read } },
            }),
        })
        .collect();

    let payload = json!({
        "invocation_ids": invocation_ids,
        "read_set": read_set_json,
    });

    ComponentReadSet {
        source: ReadSetSource::Traced,
        payload: Some(payload),
    }
}

/// Collect the file paths a component owns / references. Union of
/// m133 `occurrences[].location` + `evidence.source_file_paths[]`.
fn component_file_paths(component: &ResolvedComponent) -> HashSet<String> {
    let mut paths: HashSet<String> = HashSet::new();
    for occ in &component.occurrences {
        paths.insert(occ.location.clone());
    }
    for path in &component.evidence.source_file_paths {
        paths.insert(path.clone());
    }
    paths
}

/// DFS over `parent_invocation_id` chain, unioning read_sets into
/// `out_read_set`. Visited set breaks cycles (which shouldn't exist
/// but we defend anyway).
fn walk_ancestors_collecting_reads(
    start_id: u64,
    by_id: &BTreeMap<u64, &CompilerInvocation>,
    visited: &mut HashSet<u64>,
    out_ids: &mut Vec<u64>,
    out_read_set: &mut BTreeMap<String, ReadSetEntry>,
) {
    if !visited.insert(start_id) {
        return;
    }
    let Some(inv) = by_id.get(&start_id) else {
        return;
    };
    out_ids.push(start_id);
    for entry in &inv.read_set {
        out_read_set
            .entry(entry.path.to_string_lossy().to_string())
            .or_insert_with(|| entry.clone());
    }
    if let Some(parent_id) = inv.parent_invocation_id {
        walk_ancestors_collecting_reads(parent_id, by_id, visited, out_ids, out_read_set);
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use mikebom_common::attestation::compiler_pipeline::{
        CompilerFamily, CompletenessState, WriteSetEntry,
    };
    use mikebom_common::resolution::{
        FileOccurrence, ResolutionEvidence, ResolutionTechnique, ResolvedComponent,
    };
    use mikebom_common::types::hash::{ContentHash, HashAlgorithm, HexString};
    use mikebom_common::types::purl::Purl;
    use mikebom_common::types::timestamp::Timestamp;
    use std::path::PathBuf;

    fn zero_hash() -> ContentHash {
        ContentHash {
            algorithm: HashAlgorithm::Sha256,
            value: HexString::new(&"0".repeat(64)).unwrap(),
        }
    }

    fn mk_component_with_path(purl: &str, path: &str) -> ResolvedComponent {
        ResolvedComponent {
            purl: Purl::new(purl).unwrap(),
            name: "test".into(),
            version: "1.0".into(),
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::UrlPattern,
                confidence: 0.9,
                source_connection_ids: vec![],
                source_file_paths: vec![path.into()],
                deps_dev_match: None,
            },
            licenses: vec![],
            concluded_licenses: vec![],
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
            extra_annotations: Default::default(),
            binary_role: None,
        }
    }

    fn mk_component_with_occurrence(purl: &str, location: &str) -> ResolvedComponent {
        let mut c = mk_component_with_path(purl, "/other/path");
        c.evidence.source_file_paths.clear();
        c.occurrences.push(FileOccurrence {
            location: location.into(),
            sha256: "a".repeat(64),
            md5_legacy: None,
            apk_sha1: None,
            rpm_file_digest: None,
        });
        c
    }

    fn mk_invocation(
        invocation_id: u64,
        parent: Option<u64>,
        reads: &[&str],
        writes: &[&str],
    ) -> CompilerInvocation {
        CompilerInvocation {
            invocation_id,
            compiler: CompilerFamily::Rustc,
            pid: invocation_id as u32,
            ppid: parent.map(|p| p as u32).unwrap_or(0),
            parent_invocation_id: parent,
            cgroup_id: 0,
            start_timestamp: Timestamp::now(),
            end_timestamp: None,
            argv_full_path: None,
            argv: vec![],
            cwd: None,
            exit_code: None,
            read_set: reads
                .iter()
                .map(|p| ReadSetEntry {
                    path: PathBuf::from(p),
                    sha256: zero_hash(),
                    kind: ReadKind::File,
                })
                .collect(),
            write_set: writes
                .iter()
                .map(|p| WriteSetEntry {
                    path: PathBuf::from(p),
                    sha256: Some(zero_hash()),
                    survived_trace_window: true,
                })
                .collect(),
            events_dropped: 0,
        }
    }

    fn empty_pipeline(invocations: Vec<CompilerInvocation>) -> CompilerPipelineData {
        CompilerPipelineData {
            invocations,
            dag_edges: vec![],
            completeness: CompletenessState::Complete,
            secrets_read_filtered: 0,
            include_system_reads_flag: false,
            filter_categories_applied: vec![],
        }
    }

    #[test]
    fn component_with_no_matching_write_returns_unknown() {
        let comp = mk_component_with_path("pkg:cargo/foo@1", "/target/release/foo");
        let pipeline = empty_pipeline(vec![mk_invocation(
            1,
            None,
            &["/src/foo.rs"],
            &["/target/release/other"],
        )]);
        let result = map_component_to_source_read_set(&comp, &pipeline);
        assert_eq!(result.source, ReadSetSource::Unknown);
        assert!(result.payload.is_none());
    }

    #[test]
    fn component_matching_single_invocation_gets_that_invocations_reads() {
        let comp = mk_component_with_path("pkg:cargo/foo@1", "/target/release/foo");
        let pipeline = empty_pipeline(vec![mk_invocation(
            1,
            None,
            &["/src/main.rs", "/src/lib.rs"],
            &["/target/release/foo"],
        )]);
        let result = map_component_to_source_read_set(&comp, &pipeline);
        assert_eq!(result.source, ReadSetSource::Traced);
        let payload = result.payload.unwrap();
        assert_eq!(payload["invocation_ids"], json!([1]));
        let paths: Vec<&str> = payload["read_set"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["path"].as_str().unwrap())
            .collect();
        // Sorted lex by path per R8.
        assert_eq!(paths, vec!["/src/lib.rs", "/src/main.rs"]);
    }

    #[test]
    fn component_matching_child_invocation_gets_ancestor_reads_too() {
        let comp = mk_component_with_path("pkg:cargo/foo@1", "/target/release/foo");
        // Parent (cargo-like) reads Cargo.toml; child (rustc-like)
        // reads main.rs + writes the binary.
        let parent = mk_invocation(1, None, &["/Cargo.toml"], &["/target/release/foo-metadata"]);
        let child = mk_invocation(
            2,
            Some(1),
            &["/src/main.rs"],
            &["/target/release/foo"],
        );
        let pipeline = empty_pipeline(vec![parent, child]);
        let result = map_component_to_source_read_set(&comp, &pipeline);
        assert_eq!(result.source, ReadSetSource::Traced);
        let payload = result.payload.unwrap();
        let paths: Vec<&str> = payload["read_set"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["path"].as_str().unwrap())
            .collect();
        assert!(paths.contains(&"/src/main.rs"));
        assert!(
            paths.contains(&"/Cargo.toml"),
            "ancestor's read should be included: {paths:?}"
        );
    }

    #[test]
    fn component_matched_via_occurrence_location() {
        let comp =
            mk_component_with_occurrence("pkg:deb/nginx@1", "/usr/sbin/nginx");
        let pipeline = empty_pipeline(vec![mk_invocation(
            1,
            None,
            &["/build/src/main.c"],
            &["/usr/sbin/nginx"],
        )]);
        let result = map_component_to_source_read_set(&comp, &pipeline);
        assert_eq!(result.source, ReadSetSource::Traced);
        let payload = result.payload.unwrap();
        let paths: Vec<&str> = payload["read_set"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["path"].as_str().unwrap())
            .collect();
        assert_eq!(paths, vec!["/build/src/main.c"]);
    }

    #[test]
    fn read_set_source_wire_strings_match_contract() {
        // Contract lock — see contracts/annotations.md A-2.
        assert_eq!(ReadSetSource::Traced.as_wire_str(), "traced");
        assert_eq!(ReadSetSource::Unknown.as_wire_str(), "unknown");
    }

    #[test]
    fn two_matching_invocations_deduplicate_shared_reads() {
        let comp = mk_component_with_path("pkg:cargo/foo@1", "/target/release/foo");
        // Two independent invocations both write to /target/release/foo
        // (odd but possible) — both contribute reads to the union.
        let inv_a = mk_invocation(
            1,
            None,
            &["/src/shared.rs", "/src/a.rs"],
            &["/target/release/foo"],
        );
        let inv_b = mk_invocation(
            2,
            None,
            &["/src/shared.rs", "/src/b.rs"],
            &["/target/release/foo"],
        );
        let pipeline = empty_pipeline(vec![inv_a, inv_b]);
        let result = map_component_to_source_read_set(&comp, &pipeline);
        let payload = result.payload.unwrap();
        let paths: Vec<&str> = payload["read_set"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["path"].as_str().unwrap())
            .collect();
        // Shared read appears only once (dedup by path).
        assert_eq!(
            paths,
            vec!["/src/a.rs", "/src/b.rs", "/src/shared.rs"],
            "sorted lex + dedup"
        );
    }
}
