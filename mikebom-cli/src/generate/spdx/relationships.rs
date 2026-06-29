//! SPDX 2.3 Relationship struct + mapping from internal
//! `Relationship.kind` (milestone 010, T022 / T024).
//!
//! SPDX 2.3 models every edge in an SBOM's graph as an entry in
//! `relationships[]` (spec §11). Mikebom's internal model carries:
//!
//!   * `Relationship { from: PURL, to: PURL, kind: RelationshipType }`
//!     — the dependency-graph edges the CDX serializer consumes.
//!   * `ResolvedComponent.parent_purl` — the containment edge for
//!     CycloneDX nested components (shade-jar children, image layers).
//!
//! Both surfaces have to be flattened into explicit SPDX relationships
//! here (FR-010, FR-011). The document-level `DESCRIBES` edge from
//! `SPDXRef-DOCUMENT` to the root package is also emitted from this
//! builder so callers get a complete edge list in one place.

use std::collections::BTreeMap;

use mikebom_common::resolution::RelationshipType;

use super::ids::SpdxId;
use crate::generate::ScanArtifacts;

/// SPDX 2.3 relationship-type enum (spec §11.1).
///
/// Mikebom emits a subset today; unused variants from the spec are
/// added as new ecosystems or US2 annotations demand them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[allow(dead_code)] // ContainedBy is the symmetric inverse of Contains, included for spec completeness against the SPDX 2.3 relationshipType enum.
pub enum SpdxRelationshipType {
    Describes,
    DependsOn,
    DevDependencyOf,
    BuildDependencyOf,
    TestDependencyOf,
    Contains,
    ContainedBy,
    /// Milestone 072 / T012 — SPDX 2.3 §11.1 native semantic for
    /// "this component was built from that source-tier element".
    /// Cross-document edge: target SPDXID is namespaced into a
    /// `DocumentRef-source-sbom:SPDXRef-...` form (SPDX 2.3 §7.2).
    BuiltFrom,
}

/// One SPDX 2.3 relationship edge.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SpdxRelationship {
    #[serde(rename = "spdxElementId")]
    pub source: SpdxId,
    #[serde(rename = "relatedSpdxElement")]
    pub target: SpdxId,
    #[serde(rename = "relationshipType")]
    pub kind: SpdxRelationshipType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

/// Build the `relationships[]` array for an SPDX 2.3 document (T024).
///
/// Emits three categories:
///
/// 1. **Document root**: one `DESCRIBES` edge from
///    `SPDXRef-DOCUMENT` to the caller-chosen root SPDXID.
///
/// 2. **Dependency edges** (FR-010): one SPDX edge per
///    `ScanArtifacts.relationships` entry. `from`/`to` are PURL
///    strings; we resolve them back to SPDXIDs via
///    [`SpdxId::for_purl`]. If a PURL fails to parse or doesn't
///    correspond to any component in the scan, the edge is skipped
///    with a debug log rather than crashing — edge corruption from
///    upstream enrichment shouldn't poison the SBOM. Direction and
///    verb depend on `ScanArtifacts.spdx2_relationship_compat`
///    (issue #228):
///
///    **`Spdx2RelationshipCompat::Full` (default)** — per the
///    data-model.md §3.4 table:
///
///    | internal `RelationshipType` | SPDX `relationshipType`          | direction |
///    |-----------------------------|----------------------------------|-----------|
///    | `DependsOn`                 | `DEPENDS_ON`                     | same      |
///    | `DevDependsOn`              | `DEV_DEPENDENCY_OF`              | reversed  |
///    | `BuildDependsOn`            | `BUILD_DEPENDENCY_OF`            | reversed  |
///    | `TestDependsOn`             | `TEST_DEPENDENCY_OF`             | reversed  |
///
///    Reversal for dev/build/test dep edges matches SPDX semantics —
///    `(A) DEV_DEPENDENCY_OF (B)` means "A is a dev dep of B", so
///    internal `(A DevDependsOn B)` "A needs B for dev" swaps to
///    `(B) DEV_DEPENDENCY_OF (A)` = "B is a dev dep of A". This is
///    the spec-richest emission and the SPDX 2.3 spec's full answer
///    for "what scope is this edge?".
///
///    **`Spdx2RelationshipCompat::Basic`** — every dep variant maps
///    to `DEPENDS_ON` in natural direction. Drops scope-on-edge in
///    favor of the basic SPDX 2.3 relationship vocabulary the
///    typical downstream consumer set (Trivy, Syft, and tooling
///    built on top of them) actually implements. Scope info lives
///    on the target Package's `mikebom:lifecycle-scope` annotation
///    (which is set in both modes — see C42 in
///    `docs/reference/sbom-format-mapping.md`).
///
/// 3. **Containment edges** (FR-011): for each component whose
///    `parent_purl` is set AND points at a top-level component in
///    the scan, one `CONTAINS` edge from parent → child (and SPDX
///    implicitly carries `CONTAINED_BY` as the inverse — consumers
///    get both readings). Orphans (parent_purl points nowhere
///    resolvable) are dropped rather than producing dangling edges.
///
/// `purl_aliases` (issue #229): alias entries `(dropped_purl, new_id)`
/// used when milestone 077's `--root-name` override has filtered the
/// manifest-derived main-module Package out of `packages[]` and
/// replaced it with a synthesized root carrying a different PURL.
/// Without this alias step, dependency edges sourced at the dropped
/// main-module's PURL silently disappear (the PURL is no longer in
/// `artifacts.components`, so the resolver falls through the "PURL
/// not present" branch), leaving the new root with zero outgoing
/// edges. Inserting the alias rewrites those edges to source from the
/// synthetic root's SPDXID, preserving CDX↔SPDX parity. Pass `&[]`
/// when no override is active.
pub fn build_relationships(
    artifacts: &ScanArtifacts<'_>,
    roots: &[SpdxId],
    purl_aliases: &[(String, SpdxId)],
) -> Vec<SpdxRelationship> {
    let mut out: Vec<SpdxRelationship> = Vec::new();

    // 1. Document describes the root(s). Multi-root case (cargo
    //    workspace, polyglot scans with multiple per-ecosystem main-
    //    modules) emits one DESCRIBES edge per root — SPDX 2.3
    //    `documentDescribes[]` is plural by design and the
    //    DESCRIBES relationship type is many-to-many. Single-root
    //    case (the dominant flow) emits exactly one edge as before.
    for root in roots {
        out.push(SpdxRelationship {
            source: SpdxId::document(),
            target: root.clone(),
            kind: SpdxRelationshipType::Describes,
            comment: None,
        });
    }

    // Build a PURL→SpdxId map once so dependency edges don't re-hash
    // each PURL. BTreeMap keeps iteration deterministic if we ever
    // need it, though here we only do O(1) lookups.
    let mut purl_to_id: BTreeMap<String, SpdxId> = BTreeMap::new();
    for c in artifacts.components {
        purl_to_id.insert(c.purl.as_str().to_string(), SpdxId::for_purl(&c.purl));
    }
    // Aliases override component-derived entries when both exist —
    // override callers pass `(dropped_main_module_purl, synthetic_id)`.
    for (purl, id) in purl_aliases {
        purl_to_id.insert(purl.clone(), id.clone());
    }

    // 2. Dependency edges.
    for rel in artifacts.relationships {
        let from_id = match purl_to_id.get(&rel.from) {
            Some(id) => id.clone(),
            None => {
                tracing::debug!(
                    purl = %rel.from,
                    "dropping relationship: 'from' PURL not present in component set"
                );
                continue;
            }
        };
        let to_id = match purl_to_id.get(&rel.to) {
            Some(id) => id.clone(),
            None => {
                tracing::debug!(
                    purl = %rel.to,
                    "dropping relationship: 'to' PURL not present in component set"
                );
                continue;
            }
        };
        // Issue #228: `Spdx2RelationshipCompat::Basic` collapses
        // every dep — runtime, dev, build, test — into a natural-
        // direction `DEPENDS_ON` edge, restricting emission to the
        // basic SPDX 2.3 relationship vocabulary downstream consumers
        // actually implement (Trivy, Syft, etc.). Scope info still
        // rides on the target Package's `mikebom:lifecycle-scope`
        // annotation (emitted in both modes) so the dev/build/test
        // distinction is recoverable from the document regardless of
        // which compat mode is in effect.
        let (source, target, kind) = match (
            artifacts.spdx2_relationship_compat,
            rel.relationship_type.clone(),
        ) {
            (_, RelationshipType::DependsOn) => {
                (from_id, to_id, SpdxRelationshipType::DependsOn)
            }
            (crate::generate::Spdx2RelationshipCompat::Basic, _) => {
                (from_id, to_id, SpdxRelationshipType::DependsOn)
            }
            (crate::generate::Spdx2RelationshipCompat::Full, RelationshipType::DevDependsOn) => {
                // Reverse direction for *_DEPENDENCY_OF verbs (see
                // module docs).
                (to_id, from_id, SpdxRelationshipType::DevDependencyOf)
            }
            (crate::generate::Spdx2RelationshipCompat::Full, RelationshipType::BuildDependsOn) => {
                (to_id, from_id, SpdxRelationshipType::BuildDependencyOf)
            }
            (crate::generate::Spdx2RelationshipCompat::Full, RelationshipType::TestDependsOn) => {
                // Same direction-reversal convention as DevDependsOn /
                // BuildDependsOn — internal `(A) TestDependsOn (B)`
                // "A needs B for tests" → SPDX
                // `(B) TEST_DEPENDENCY_OF (A)` "B is a test dep of A".
                (to_id, from_id, SpdxRelationshipType::TestDependencyOf)
            }
        };
        out.push(SpdxRelationship {
            source,
            target,
            kind,
            comment: None,
        });
    }

    // 3. Containment edges from parent_purl. Top-level PURLs (set of
    //    components with `parent_purl = None`) are the only valid
    //    parent targets; orphan pointers are dropped.
    //
    //    Issue #229: aliased PURLs (dropped main-module → synthetic
    //    root) also count as valid containment parents, so children
    //    whose `parent_purl` references a filtered-out main module
    //    are re-parented to the new root instead of being dropped.
    let mut top_level: std::collections::HashSet<&str> = artifacts
        .components
        .iter()
        .filter(|c| c.parent_purl.is_none())
        .map(|c| c.purl.as_str())
        .collect();
    for (purl, _) in purl_aliases {
        top_level.insert(purl.as_str());
    }
    for c in artifacts.components {
        let Some(parent_purl) = c.parent_purl.as_deref() else {
            continue;
        };
        if !top_level.contains(parent_purl) {
            continue; // orphan — CDX emits these at top-level; we just don't synthesize a containment edge
        }
        let Some(parent_id) = purl_to_id.get(parent_purl) else {
            continue;
        };
        let child_id = SpdxId::for_purl(&c.purl);
        out.push(SpdxRelationship {
            source: parent_id.clone(),
            target: child_id,
            kind: SpdxRelationshipType::Contains,
            comment: None,
        });
    }

    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use mikebom_common::attestation::integrity::TraceIntegrity;
    use mikebom_common::attestation::metadata::GenerationContext;
    use mikebom_common::resolution::{
        EnrichmentProvenance, Relationship, ResolutionEvidence, ResolutionTechnique,
        ResolvedComponent,
    };
    use mikebom_common::types::purl::Purl;

    fn mk_component(purl: &str, name: &str, version: &str) -> ResolvedComponent {
        ResolvedComponent {
            build_inclusion: None,
            purl: Purl::new(purl).unwrap(),
            name: name.to_string(),
            version: version.to_string(),
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::UrlPattern,
                confidence: 0.9,
                source_connection_ids: vec![],
                source_file_paths: vec![],
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

    fn prov() -> EnrichmentProvenance {
        EnrichmentProvenance {
            source: "test".to_string(),
            data_type: "runtime".to_string(),
        }
    }

    fn empty_integrity() -> TraceIntegrity {
        TraceIntegrity {
            ring_buffer_overflows: 0,
            events_dropped: 0,
            uprobe_attach_failures: vec![],
            kprobe_attach_failures: vec![],
            partial_captures: vec![],
            bloom_filter_capacity: 0,
            bloom_filter_false_positive_rate: 0.0,
        }
    }

    fn mk_artifacts<'a>(
        comps: &'a [ResolvedComponent],
        rels: &'a [Relationship],
        integ: &'a TraceIntegrity,
    ) -> ScanArtifacts<'a> {
        ScanArtifacts {
            target_name: "demo",
            components: comps,
            relationships: rels,
            integrity: integ,
            complete_ecosystems: &[],
            os_release_missing_fields: &[],
            go_graph_completeness: None,
            go_graph_completeness_reason: None,
            scan_target_coord: None,
            generation_context: GenerationContext::FilesystemScan,
            include_dev: false,
            include_hashes: true,
            include_source_files: false,
            scope_mode: crate::generate::ScopeMode::Artifact,
            source_document_binding: None,
            identifiers: &[],
            component_identifiers: &[],
            file_inventory_stats: None,
            file_inventory_mode: None,
            root_override: crate::generate::RootComponentOverride::default(),
            preserve_manifest_main_module: false,
            user_metadata: mikebom::binding::user_metadata::UserMetadata::default(),
            sbom_type_override: None,
            spdx2_relationship_compat: crate::generate::Spdx2RelationshipCompat::Full,
            collisions_summary: None,
        }
    }

    #[test]
    fn emits_describes_edge_from_document_to_root() {
        let integ = empty_integrity();
        let comps: Vec<ResolvedComponent> = vec![];
        let arts = mk_artifacts(&comps, &[], &integ);
        let root = SpdxId::synthetic_root("AAAAAAAAAAAAAAAA");
        let rels = build_relationships(&arts, std::slice::from_ref(&root), &[]);
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].source, SpdxId::document());
        assert_eq!(rels[0].target, root);
        assert_eq!(rels[0].kind, SpdxRelationshipType::Describes);
    }

    #[test]
    fn depends_on_keeps_same_direction() {
        let a = mk_component("pkg:cargo/a@1", "a", "1");
        let b = mk_component("pkg:cargo/b@1", "b", "1");
        let rel = Relationship {
            from: a.purl.as_str().to_string(),
            to: b.purl.as_str().to_string(),
            relationship_type: RelationshipType::DependsOn,
            provenance: prov(),
        };
        let integ = empty_integrity();
        let comps = vec![a, b];
        let rels_arr = [rel];
        let arts = mk_artifacts(&comps, &rels_arr, &integ);
        let root = SpdxId::for_purl(&comps[0].purl);
        let rels = build_relationships(&arts, std::slice::from_ref(&root), &[]);
        let dep = rels
            .iter()
            .find(|r| r.kind == SpdxRelationshipType::DependsOn)
            .expect("DEPENDS_ON edge present");
        assert_eq!(dep.source, SpdxId::for_purl(&comps[0].purl));
        assert_eq!(dep.target, SpdxId::for_purl(&comps[1].purl));
    }

    /// Issue #228 — helper that builds artifacts in basic-vocabulary
    /// compat mode for the regression tests. Stand-alone helper
    /// rather than a param on `mk_artifacts` so the other ~10 tests
    /// stay unchanged.
    fn mk_artifacts_basic<'a>(
        comps: &'a [ResolvedComponent],
        rels: &'a [Relationship],
        integ: &'a TraceIntegrity,
    ) -> ScanArtifacts<'a> {
        let mut a = mk_artifacts(comps, rels, integ);
        a.spdx2_relationship_compat = crate::generate::Spdx2RelationshipCompat::Basic;
        a
    }

    #[test]
    fn basic_compat_collapses_dev_to_depends_on() {
        // Issue #228 — under Spdx2RelationshipCompat::Basic, internal
        // DevDependsOn / BuildDependsOn / TestDependsOn must all emit
        // as natural-direction DEPENDS_ON. Scope info lives on the
        // target Package's `mikebom:lifecycle-scope` annotation, not
        // on the edge.
        let a = mk_component("pkg:npm/a@1", "a", "1");
        let b = mk_component("pkg:npm/b@1", "b", "1");
        for variant in [
            RelationshipType::DevDependsOn,
            RelationshipType::BuildDependsOn,
            RelationshipType::TestDependsOn,
        ] {
            let rel = Relationship {
                from: a.purl.as_str().to_string(),
                to: b.purl.as_str().to_string(),
                relationship_type: variant.clone(),
                provenance: prov(),
            };
            let integ = empty_integrity();
            let comps = vec![a.clone(), b.clone()];
            let rels_arr = [rel];
            let arts = mk_artifacts_basic(&comps, &rels_arr, &integ);
            let root = SpdxId::for_purl(&comps[0].purl);
            let rels = build_relationships(&arts, std::slice::from_ref(&root), &[]);
            let dep = rels
                .iter()
                .find(|r| r.kind == SpdxRelationshipType::DependsOn && r.source != SpdxId::document())
                .unwrap_or_else(|| panic!("variant {variant:?}: expected DEPENDS_ON, got {rels:#?}"));
            assert_eq!(
                dep.source,
                SpdxId::for_purl(&comps[0].purl),
                "variant {variant:?}: natural direction (A->B)"
            );
            assert_eq!(dep.target, SpdxId::for_purl(&comps[1].purl));
            // And no typed `*_DEPENDENCY_OF` variant should leak through.
            let leaked: Vec<_> = rels
                .iter()
                .filter(|r| matches!(
                    r.kind,
                    SpdxRelationshipType::DevDependencyOf
                        | SpdxRelationshipType::BuildDependencyOf
                        | SpdxRelationshipType::TestDependencyOf
                ))
                .collect();
            assert!(
                leaked.is_empty(),
                "variant {variant:?}: no typed scoped edges expected in Basic mode, got {leaked:#?}"
            );
        }
    }

    #[test]
    fn full_is_the_default_relationship_compat() {
        // Default compat mode preserves the milestone-052/part-2
        // typed reversed-direction emission. Belt-and-suspenders
        // alongside `dev_depends_on_reverses_to_dev_dependency_of`:
        // asserts that the *default* value of Spdx2RelationshipCompat
        // is Full (so a caller that constructs
        // `ScanArtifacts { .. spdx2_relationship_compat: Default::default(), .. }`
        // gets the spec-rich emission).
        assert_eq!(
            crate::generate::Spdx2RelationshipCompat::default(),
            crate::generate::Spdx2RelationshipCompat::Full,
        );
    }

    #[test]
    fn dev_depends_on_reverses_to_dev_dependency_of() {
        let a = mk_component("pkg:npm/a@1", "a", "1");
        let b = mk_component("pkg:npm/b@1", "b", "1");
        let rel = Relationship {
            from: a.purl.as_str().to_string(),
            to: b.purl.as_str().to_string(),
            relationship_type: RelationshipType::DevDependsOn,
            provenance: prov(),
        };
        let integ = empty_integrity();
        let comps = vec![a, b];
        let rels_arr = [rel];
        let arts = mk_artifacts(&comps, &rels_arr, &integ);
        let root = SpdxId::for_purl(&comps[0].purl);
        let rels = build_relationships(&arts, std::slice::from_ref(&root), &[]);
        let dev = rels
            .iter()
            .find(|r| r.kind == SpdxRelationshipType::DevDependencyOf)
            .expect("DEV_DEPENDENCY_OF edge present");
        // Internal A DevDependsOn B  =>  SPDX B DEV_DEPENDENCY_OF A.
        assert_eq!(dev.source, SpdxId::for_purl(&comps[1].purl));
        assert_eq!(dev.target, SpdxId::for_purl(&comps[0].purl));
    }

    #[test]
    fn containment_parent_to_child_contains_edge() {
        let parent = mk_component("pkg:maven/com/example/parent@1", "parent", "1");
        let mut child = mk_component("pkg:maven/com/example/child@1", "child", "1");
        child.parent_purl = Some(parent.purl.as_str().to_string());
        let integ = empty_integrity();
        let comps = vec![parent, child];
        let arts = mk_artifacts(&comps, &[], &integ);
        let root = SpdxId::for_purl(&comps[0].purl);
        let rels = build_relationships(&arts, std::slice::from_ref(&root), &[]);
        let contains = rels
            .iter()
            .find(|r| r.kind == SpdxRelationshipType::Contains)
            .expect("CONTAINS edge for parent→child");
        assert_eq!(contains.source, SpdxId::for_purl(&comps[0].purl));
        assert_eq!(contains.target, SpdxId::for_purl(&comps[1].purl));
    }

    #[test]
    fn orphan_containment_is_dropped() {
        // child's parent_purl points at a PURL that isn't in the scan.
        let mut child = mk_component("pkg:maven/x/y/child@1", "child", "1");
        child.parent_purl = Some("pkg:maven/x/y/not-present@9".to_string());
        let integ = empty_integrity();
        let comps = vec![child];
        let arts = mk_artifacts(&comps, &[], &integ);
        let root = SpdxId::synthetic_root("AAAAAAAAAAAAAAAA");
        let rels = build_relationships(&arts, std::slice::from_ref(&root), &[]);
        // Only the DESCRIBES edge; the orphan containment is dropped.
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].kind, SpdxRelationshipType::Describes);
    }

    #[test]
    fn purl_alias_rewrites_depends_on_source_to_synthetic_root() {
        // Issue #229 regression: when --root-name drops the manifest-
        // derived main module and synthesizes a new root, dep edges
        // sourced at the dropped PURL must be rewritten to source from
        // the synthetic root's SPDXID (otherwise the new root ends up
        // orphaned from the dependency graph, and the SPDX output
        // diverges from CDX). Only `direct_dep` is in the components
        // view here — the old main module's PURL is gone, but its
        // outgoing edge is present in `relationships`; the alias step
        // is what reconnects it.
        let direct_dep = mk_component("pkg:cargo/dep@1", "dep", "1");
        let rel = Relationship {
            from: "pkg:cargo/old-main@1".to_string(),
            to: direct_dep.purl.as_str().to_string(),
            relationship_type: RelationshipType::DependsOn,
            provenance: prov(),
        };
        let integ = empty_integrity();
        let comps = vec![direct_dep];
        let rels_arr = [rel];
        let arts = mk_artifacts(&comps, &rels_arr, &integ);
        let synth_root = SpdxId::synthetic_root("FFFFFFFFFFFFFFFF");
        let aliases = vec![("pkg:cargo/old-main@1".to_string(), synth_root.clone())];
        let rels = build_relationships(&arts, std::slice::from_ref(&synth_root), &aliases);
        let dep = rels
            .iter()
            .find(|r| r.kind == SpdxRelationshipType::DependsOn)
            .expect("DEPENDS_ON edge present after alias rewrite");
        assert_eq!(dep.source, synth_root, "edge source rewritten to synthetic root");
        assert_eq!(dep.target, SpdxId::for_purl(&comps[0].purl));
    }

    #[test]
    fn purl_alias_rewrites_reversed_dev_dependency_of_target() {
        // Issue #229: the alias also has to win for direction-reversed
        // edges (DevDependsOn / BuildDependsOn / TestDependsOn).
        // Internal `(old-main DevDependsOn dep)` becomes SPDX
        // `(dep) DEV_DEPENDENCY_OF (synthetic-root)` after the alias.
        let direct_dep = mk_component("pkg:npm/dep@1", "dep", "1");
        let rel = Relationship {
            from: "pkg:npm/old-main@1".to_string(),
            to: direct_dep.purl.as_str().to_string(),
            relationship_type: RelationshipType::DevDependsOn,
            provenance: prov(),
        };
        let integ = empty_integrity();
        let comps = vec![direct_dep];
        let rels_arr = [rel];
        let arts = mk_artifacts(&comps, &rels_arr, &integ);
        let synth_root = SpdxId::synthetic_root("DEADBEEFDEADBEEF");
        let aliases = vec![("pkg:npm/old-main@1".to_string(), synth_root.clone())];
        let rels = build_relationships(&arts, std::slice::from_ref(&synth_root), &aliases);
        let dev = rels
            .iter()
            .find(|r| r.kind == SpdxRelationshipType::DevDependencyOf)
            .expect("DEV_DEPENDENCY_OF edge present");
        assert_eq!(dev.source, SpdxId::for_purl(&comps[0].purl));
        assert_eq!(dev.target, synth_root, "edge target rewritten to synthetic root");
    }

    #[test]
    fn purl_alias_rewrites_containment_parent_to_synthetic_root() {
        // Issue #229: a child whose parent_purl pointed at the
        // dropped main module should be re-parented to the synthetic
        // root via the alias step (otherwise the CONTAINS edge is
        // dropped as orphan).
        let mut child = mk_component("pkg:maven/x/y/child@1", "child", "1");
        child.parent_purl = Some("pkg:maven/x/y/old-main@1".to_string());
        let integ = empty_integrity();
        let comps = vec![child];
        let arts = mk_artifacts(&comps, &[], &integ);
        let synth_root = SpdxId::synthetic_root("CAFEBABE12345678");
        let aliases = vec![("pkg:maven/x/y/old-main@1".to_string(), synth_root.clone())];
        let rels = build_relationships(&arts, std::slice::from_ref(&synth_root), &aliases);
        let contains = rels
            .iter()
            .find(|r| r.kind == SpdxRelationshipType::Contains)
            .expect("CONTAINS edge present after alias rewrite");
        assert_eq!(contains.source, synth_root);
        assert_eq!(contains.target, SpdxId::for_purl(&comps[0].purl));
    }

    #[test]
    fn unknown_purl_in_relationship_is_dropped() {
        // Relationship references a PURL that's not in the component set.
        let a = mk_component("pkg:cargo/a@1", "a", "1");
        let rel = Relationship {
            from: a.purl.as_str().to_string(),
            to: "pkg:cargo/not-present@9".to_string(),
            relationship_type: RelationshipType::DependsOn,
            provenance: prov(),
        };
        let integ = empty_integrity();
        let comps = vec![a];
        let rels_arr = [rel];
        let arts = mk_artifacts(&comps, &rels_arr, &integ);
        let root = SpdxId::for_purl(&comps[0].purl);
        let rels = build_relationships(&arts, std::slice::from_ref(&root), &[]);
        // Only the DESCRIBES edge remains.
        assert_eq!(rels.len(), 1);
    }
}
