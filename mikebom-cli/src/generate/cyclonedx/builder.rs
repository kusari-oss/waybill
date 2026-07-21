use serde_json::json;
use uuid::Uuid;

use mikebom_common::attestation::integrity::TraceIntegrity;
use mikebom_common::attestation::metadata::GenerationContext;
use mikebom_common::resolution::{BinaryRole, Relationship, ResolvedComponent};
use mikebom_common::types::license::SpdxExpression;

use super::compositions::build_compositions;
use super::dependencies::build_dependencies;
use super::evidence::{build_evidence, evidence_to_properties};
use super::metadata::build_metadata;
use super::vex::build_vulnerabilities;

/// Configuration for CycloneDX BOM generation.
#[derive(Clone, Debug)]
pub struct CycloneDxConfig {
    /// Whether to include per-component content hashes.
    pub include_hashes: bool,
    /// Whether to include source file paths in evidence.
    pub include_source_files: bool,
    /// How this SBOM was produced. Gets surfaced in the CycloneDX
    /// `mikebom:generation-context` property so downstream consumers can
    /// distinguish a build-time trace from a post-hoc filesystem scan.
    pub generation_context: GenerationContext,
    /// Whether the caller ran the scan with `--include-dev`. Controls
    /// emission of the `mikebom:dev-dependency` property on dev-flagged
    /// components — the flag is only ever emitted when dev components
    /// were intentionally included, so downstream consumers can trust
    /// the absence of the property to mean "this component is prod".
    pub include_dev: bool,
}

impl Default for CycloneDxConfig {
    fn default() -> Self {
        Self {
            include_hashes: true,
            include_source_files: false,
            generation_context: GenerationContext::BuildTimeTrace,
            include_dev: false,
        }
    }
}

/// Builder that assembles a complete CycloneDX 1.6 BOM document.
pub struct CycloneDxBuilder {
    config: CycloneDxConfig,
    /// Feature 005 SC-009 — names of `/etc/os-release` fields that were
    /// missing during the scan. Populated by the caller via
    /// `set_os_release_missing_fields`; emitted into the SBOM's
    /// `metadata.properties` as `mikebom:os-release-missing-fields`
    /// when non-empty.
    os_release_missing_fields: Vec<String>,
    /// Milestone 160 (T034/T035): doc-scope Go-transitive coverage
    /// signal. Distinct from `go_graph_completeness` per research.md R1.
    /// `None` ⇒ no Go scan (annotation absent).
    go_transitive_coverage:
        Option<crate::scan_fs::package_db::golang::graph_resolver::GoTransitiveCoverage>,
    /// Milestone 172: doc-scope Go step-5 fallback count for the C117
    /// `mikebom:go-transitive-fallback-count` annotation. Sibling of
    /// `go_transitive_coverage`; both are Go-gated. `None` iff no Go
    /// scan happened.
    go_transitive_fallback_count: Option<usize>,
    /// Milestone 173: doc-scope Go cache-warming outcome for the C118
    /// (`mikebom:go-cache-warming-mode`) + C119
    /// (`mikebom:go-cache-warming-failed`) annotations. Sibling of
    /// `go_transitive_coverage`; both are Go-gated. `None` iff no Go
    /// scan happened.
    go_cache_warming:
        Option<crate::scan_fs::package_db::golang::CacheWarmingResult>,
    /// Milestone 161 (T041): doc-scope Go-workspace-mode signal.
    /// Distinct from `go_transitive_coverage` per research.md R1.
    /// `None` ⇒ no `go.work` at scanned root (C112 absent).
    go_workspace_mode:
        Option<crate::scan_fs::package_db::golang::gowork::WorkspaceMode>,
    /// Milestone 204 (#554): doc-scope helm image-extraction-mode
    /// signal for the C123 `mikebom:image-extraction-completeness`
    /// annotation. `None` ⇒ no helm reader ran (C123 absent).
    helm_extraction_mode:
        Option<crate::scan_fs::package_db::HelmExtractionMode>,
    /// Milestone 206 (#440): doc-scope image-source signal for the
    /// C124 `mikebom:image-source` annotation. Conditional emission
    /// (podman-only) preserves FR-005 byte-identity for docker/remote
    /// scans.
    image_source: Option<crate::cli::scan_cmd::ImageSource>,
    /// Milestone 072 / T010: source-tier SBOM identity for the
    /// document-level cross-document reference
    /// (`metadata.component.externalReferences[type:bom]`). `None`
    /// when the scan was NOT invoked with `--bind-to-source`.
    source_document_binding: Option<mikebom::binding::SourceDocumentId>,
    /// Milestone 073: identifiers (auto-detected `repo:` /
    /// `image:` plus manual `--repo` / `--git-ref` / `--image` /
    /// `--attestation` / `--id <scheme>=<value>` flags). Built-in
    /// identifiers ride `metadata.component.externalReferences[]`;
    /// user-defined identifiers ride a `metadata.properties[]` entry
    /// under `mikebom:identifiers`. The Vec is already
    /// deduplicated and ordered by the resolution pipeline in
    /// `cli/scan_cmd.rs::resolve_identifiers`.
    identifiers: Vec<mikebom::binding::identifiers::Identifier>,
    /// Milestone 076 — per-component user-defined identifiers from
    /// `--component-id <PURL>=<scheme>:<value>` flags. Threaded into
    /// `build_components` so each per-component `properties[]` array
    /// gains entries for matching PURLs.
    component_identifiers:
        Vec<mikebom::binding::identifiers::component_id::ComponentIdentifierFlag>,
    /// Milestone 077 — operator-supplied overrides for the root
    /// component's name + version. When `is_active()`, the override
    /// values replace the auto-derived ones in `metadata.component`
    /// AND any manifest-derived main-module components are filtered
    /// from the emitted `components[]` array (clean replacement per
    /// the 2026-05-06 Q2 clarification).
    root_override: crate::generate::RootComponentOverride,
    /// Milestone 149 (issue #151): when `true` AND `root_override.is_active()`,
    /// preserve the manifest-derived main-module as a `library`-typed
    /// entry in `components[]` rather than dropping it per the milestone-
    /// 077 clean-replacement default. Default `false` preserves milestone-
    /// 077 byte-identity (SC-002 regression guard).
    preserve_manifest_main_module: bool,
    /// Milestone 080 — user-provided SBOM metadata aggregated from the
    /// `--creator` / `--annotator` / `--annotation-comment` /
    /// `--metadata-comment` / `--scan-target-name` / `--metadata-file`
    /// flags. Threaded into `build_metadata` so each entry lands at
    /// the format's standards-native carrier.
    user_metadata: mikebom::binding::user_metadata::UserMetadata,
    /// Milestone 081 — operator-asserted CISA SBOM Type from the new
    /// `--sbom-type <type>` flag. When `Some(_)`, the CDX
    /// `metadata.lifecycles[]` aggregator returns a single-element
    /// array with the asserted phase via the equivalence table; when
    /// `None`, the milestone-047 per-component aggregation continues
    /// unchanged. Per-component `mikebom:sbom-tier` annotations are
    /// preserved in either case (operator override is document-level
    /// only).
    sbom_type_override:
        Option<crate::generate::lifecycle_phases::SbomType>,
    /// Milestone 133 US3 — file-tier walker diagnostic counters. `None`
    /// when `--file-inventory=off`; `Some(_)` for orphan/full modes.
    file_inventory_stats:
        Option<crate::scan_fs::file_tier::walker::WalkerStats>,
    /// Milestone 133 US4 — `--file-inventory` mode label. Only
    /// `Some("full")` triggers the document-level override marker
    /// (Constitution Strict Boundary §5).
    file_inventory_mode: Option<String>,
    /// Milestone 134 — document-scope aggregate of divergent-PURL
    /// collision records detected in the scan. `None` ⇒ no
    /// collisions ⇒ no document-scope annotation (FR-009).
    collisions_summary:
        Option<mikebom_common::divergence::CollisionsSummary>,
    /// Milestone 210 — compiler-pipeline data captured from the
    /// eBPF trace. When `Some(_)`, `build_components` walks each
    /// component through `map_component_to_source_read_set` and
    /// emits per-component `mikebom:source-read-set` (C130) +
    /// `mikebom:read-set-source` (C131) `properties[]` entries
    /// per contracts/annotations.md A-1/A-2. `None` ⇒ scan ran
    /// without eBPF ⇒ neither property emitted (byte-identity
    /// preserved for the non-trace code path per m208 defensive-
    /// default pattern).
    compiler_pipeline:
        Option<mikebom_common::attestation::compiler_pipeline::CompilerPipelineData>,
}

impl CycloneDxBuilder {
    /// Create a new builder with the given configuration.
    pub fn new(config: CycloneDxConfig) -> Self {
        Self {
            config,
            os_release_missing_fields: Vec::new(),
            go_transitive_coverage: None,
            go_transitive_fallback_count: None,
            go_cache_warming: None,
            go_workspace_mode: None,
            helm_extraction_mode: None,
            image_source: None,
            source_document_binding: None,
            identifiers: Vec::new(),
            component_identifiers: Vec::new(),
            root_override: crate::generate::RootComponentOverride::default(),
            preserve_manifest_main_module: false,
            user_metadata: mikebom::binding::user_metadata::UserMetadata::default(),
            sbom_type_override: None,
            file_inventory_stats: None,
            file_inventory_mode: None,
            collisions_summary: None,
            compiler_pipeline: None,
        }
    }

    /// Milestone 210 — record the compiler-pipeline data captured
    /// from the eBPF trace so `build_components` can emit per-
    /// component C130 + C131 properties. `None` ⇒ no trace ⇒ no
    /// annotations (byte-identical to the pre-m210 code path).
    pub fn with_compiler_pipeline(
        mut self,
        pipeline: Option<mikebom_common::attestation::compiler_pipeline::CompilerPipelineData>,
    ) -> Self {
        self.compiler_pipeline = pipeline;
        self
    }

    /// Milestone 134 — record the document-scope `CollisionsSummary`
    /// aggregating every divergent-PURL collision detected in the
    /// scan. `None` ⇒ no collisions ⇒ no annotation emitted.
    pub fn with_collisions_summary(
        mut self,
        summary: Option<mikebom_common::divergence::CollisionsSummary>,
    ) -> Self {
        self.collisions_summary = summary;
        self
    }

    /// Milestone 133 US3 — record the file-tier walker's diagnostic
    /// counters so the CDX `metadata.properties[]` emitter can surface
    /// non-zero skip counts as Principle-X transparency annotations.
    pub fn with_file_inventory_stats(
        mut self,
        stats: Option<crate::scan_fs::file_tier::walker::WalkerStats>,
    ) -> Self {
        self.file_inventory_stats = stats;
        self
    }

    /// Milestone 133 US4 — record the operator-supplied
    /// `--file-inventory` mode label. Only `Some("full")` triggers
    /// the document-level override marker.
    pub fn with_file_inventory_mode(mut self, mode: Option<String>) -> Self {
        self.file_inventory_mode = mode;
        self
    }

    /// Milestone 081 — record the operator-supplied CISA SBOM Type
    /// override from `--sbom-type <type>`. When present, all CDX
    /// emission paths collapse `metadata.lifecycles[]` to a single-
    /// element array via the equivalence table.
    pub fn with_sbom_type_override(
        mut self,
        t: Option<crate::generate::lifecycle_phases::SbomType>,
    ) -> Self {
        self.sbom_type_override = t;
        self
    }

    /// Milestone 080 — record the user-supplied SBOM metadata. When
    /// `user_metadata.is_active()`, `build_metadata` routes each
    /// entry to the CDX 1.6 standards-native carrier per
    /// `specs/080-user-sbom-metadata/contracts/`.
    pub fn with_user_metadata(
        mut self,
        m: mikebom::binding::user_metadata::UserMetadata,
    ) -> Self {
        self.user_metadata = m;
        self
    }

    /// Milestone 077 — record the operator-supplied root-component
    /// override. When the override `is_active()`, the per-format
    /// build path uses the operator-supplied name/version verbatim
    /// (with PURL percent-encoding applied at emission) and drops
    /// manifest-derived main-module components from the emitted
    /// `components[]` array per the 2026-05-06 clean-replacement
    /// clarification.
    pub fn with_root_override(
        mut self,
        ov: crate::generate::RootComponentOverride,
    ) -> Self {
        self.root_override = ov;
        self
    }

    /// Milestone 149 (issue #151) — record the operator-supplied
    /// `--preserve-manifest-main-module` flag. When `true` AND
    /// `root_override.is_active()`, the manifest-derived main-module
    /// is preserved as a `library`-typed entry in `components[]` with
    /// a `mikebom:demoted-from-main-module = "true"` annotation rather
    /// than being dropped per the milestone-077 clean-replacement
    /// default. No-op without an active root override (silent + INFO
    /// log per spec FR-006) and on multi-main-module scans (silent +
    /// INFO log per FR-013).
    pub fn with_preserve_manifest_main_module(
        mut self,
        preserve: bool,
    ) -> Self {
        self.preserve_manifest_main_module = preserve;
        self
    }

    /// Milestone 072 / T010 — record the source-tier SBOM identifier
    /// for the document-level
    /// `metadata.component.externalReferences[type:bom]` cross-document
    /// reference. Pass `None` when `--bind-to-source` was not supplied.
    pub fn with_source_document_binding(
        mut self,
        id: Option<mikebom::binding::SourceDocumentId>,
    ) -> Self {
        self.source_document_binding = id;
        self
    }

    /// Milestone 073 — record the identifiers for the emitted SBOM.
    /// Built-in schemes ride
    /// `metadata.component.externalReferences[]` per scheme; user-
    /// defined schemes ride a `mikebom:identifiers` property
    /// at metadata level.
    pub fn with_identifiers(
        mut self,
        ids: Vec<mikebom::binding::identifiers::Identifier>,
    ) -> Self {
        self.identifiers = ids;
        self
    }

    /// Milestone 076 — record per-component user-defined identifiers
    /// from `--component-id <PURL>=<scheme>:<value>` flags. Each
    /// matching component gets the identifier appended to its
    /// `properties[]` array per research §2 + FR-008. Zero-match
    /// selectors warn and the scan continues per FR-010.
    pub fn with_component_identifiers(
        mut self,
        ids: Vec<
            mikebom::binding::identifiers::component_id::ComponentIdentifierFlag,
        >,
    ) -> Self {
        self.component_identifiers = ids;
        self
    }

    /// Feature 005 — record diagnostic fields observed during the scan.
    /// When non-empty, they drive the `mikebom:os-release-missing-fields`
    /// CycloneDX metadata property.
    pub fn with_os_release_missing_fields(mut self, fields: Vec<String>) -> Self {
        self.os_release_missing_fields = fields;
        self
    }

    /// Milestone 160 (T034/T035) — record the doc-scope Go-transitive
    /// coverage signal per FR-004/FR-005. Drives the C110/C111
    /// document-scope annotations. `None` ⇒ no Go scan happened
    /// (annotations absent per SC-003).
    pub fn with_go_transitive_coverage(
        mut self,
        coverage: Option<crate::scan_fs::package_db::golang::graph_resolver::GoTransitiveCoverage>,
    ) -> Self {
        self.go_transitive_coverage = coverage;
        self
    }

    /// Milestone 172 — record the doc-scope Go step-5 fallback count
    /// per FR-002 + Q1. Drives the C117
    /// `mikebom:go-transitive-fallback-count` annotation. `None` iff no
    /// Go scan happened (annotation absent). `Some(0)` on healthy scans
    /// (annotation emitted with `"0"`).
    pub fn with_go_transitive_fallback_count(
        mut self,
        count: Option<usize>,
    ) -> Self {
        self.go_transitive_fallback_count = count;
        self
    }

    /// Milestone 173 — record the doc-scope Go cache-warming outcome.
    /// Drives the C118 (`mikebom:go-cache-warming-mode`) unconditional
    /// annotation + C119 (`mikebom:go-cache-warming-failed`) conditional
    /// annotation. `None` iff no Go scan happened.
    pub fn with_go_cache_warming(
        mut self,
        warming: Option<crate::scan_fs::package_db::golang::CacheWarmingResult>,
    ) -> Self {
        self.go_cache_warming = warming;
        self
    }

    /// Milestone 161 (T041) — record the doc-scope Go-workspace-mode
    /// signal per FR-004. Drives the C112 document-scope annotation.
    /// `None` ⇒ no `go.work` at scanned root (annotation absent per
    /// SC-003).
    pub fn with_go_workspace_mode(
        mut self,
        mode: Option<crate::scan_fs::package_db::golang::gowork::WorkspaceMode>,
    ) -> Self {
        self.go_workspace_mode = mode;
        self
    }

    /// Milestone 204 (#554) — record the doc-scope helm image-extraction
    /// mode signal per FR-005. Drives the C123
    /// `mikebom:image-extraction-completeness` document-scope
    /// annotation. `None` ⇒ no helm reader ran (annotation absent per
    /// FR-004 / SC-004 byte-identity for non-Helm scans).
    pub fn with_helm_extraction_mode(
        mut self,
        mode: Option<crate::scan_fs::package_db::HelmExtractionMode>,
    ) -> Self {
        self.helm_extraction_mode = mode;
        self
    }

    /// Milestone 206 (#440) — record the doc-scope image-source signal
    /// per FR-014. Drives the C124 `mikebom:image-source` annotation.
    /// Conditional emission (podman-only in MVP) preserves FR-005
    /// byte-identity for docker/remote/path scans.
    pub fn with_image_source(
        mut self,
        source: Option<crate::cli::scan_cmd::ImageSource>,
    ) -> Self {
        self.image_source = source;
        self
    }

    /// Build a complete CycloneDX 1.6 JSON BOM.
    ///
    /// Assembles all sections: metadata, components, compositions,
    /// dependencies, and vulnerabilities.
    pub fn build(
        &self,
        components: &[ResolvedComponent],
        relationships: &[Relationship],
        integrity: &TraceIntegrity,
        target_name: &str,
        complete_ecosystems: &[String],
        scan_target_coord: Option<&crate::scan_fs::package_db::maven::ScanTargetCoord>,
    ) -> anyhow::Result<serde_json::Value> {
        let serial_number = format!("urn:uuid:{}", Uuid::new_v4());

        // Milestone 077 — when the operator supplied --root-name and/or
        // --root-version, the override values become the BOM-subject
        // identity AND any manifest-derived main-module components are
        // dropped from the emitted components[] array (clean replacement
        // per Q2 clarification). The unset half (when only one flag is
        // passed) falls through to the existing auto-derivation.
        let override_active = self.root_override.is_active();
        let effective_target_name: String = self
            .root_override
            .name
            .clone()
            .unwrap_or_else(|| target_name.to_string());
        let effective_target_version: String = self
            .root_override
            .version
            .clone()
            .unwrap_or_else(|| "0.0.0".to_string());
        if override_active {
            tracing::info!(
                name = %effective_target_name,
                version = %effective_target_version,
                "root component override active: name='{}' (replacing '{}'), version='{}' (replacing '0.0.0')",
                effective_target_name,
                target_name,
                effective_target_version,
            );
        }

        // Milestone 077 — when override is active, filter manifest-
        // derived main-module components OUT of the components slice
        // BEFORE downstream emission (build_components, compositions,
        // dependencies). This is the clean-replacement implementation
        // per FR-008 / VR-077-003.
        // Milestone 084 — capture the dropped main-module PURLs alongside
        // the existing component-filter so we can also filter relationships
        // keyed off them (otherwise the override path leaves an orphan
        // dependencies[].ref entry pointing at the now-dropped main-module
        // PURL — same shape bug as the pre-fix main-module path, just with
        // the orphan and the legitimate root swapped).
        // Milestone 149 (issue #151) — drop logic consolidated into
        // `apply_main_module_drop_or_demote` in `root_selector.rs` so
        // the same filter runs identically across all three emitters
        // (CDX here, SPDX 2.3 + SPDX 3 parallel sites). When the new
        // `preserve_manifest_main_module` flag is set, the helper
        // takes the demote-as-library branch instead of dropping; the
        // demoted entry's PURL still lands in `dropped_main_module_purls`
        // (renamed `redirected_main_module_purls` in the helper return
        // type) so the downstream relationship re-anchoring at lines
        // 442-447 continues to fire per US1 clarification Option A
        // (recorded 2026-06-29).
        let drop_result = crate::generate::root_selector::apply_main_module_drop_or_demote(
            components,
            &self.root_override,
            self.preserve_manifest_main_module,
        );
        let filtered_components_owned: Option<Vec<ResolvedComponent>> = if override_active {
            Some(drop_result.effective_components)
        } else {
            None
        };
        let dropped_main_module_purls: Vec<String> = drop_result.redirected_main_module_purls;
        let effective_components: &[ResolvedComponent] =
            filtered_components_owned.as_deref().unwrap_or(components);

        // Milestone 084 — when milestone-053's main-module promotion is in
        // effect, target_ref MUST equal metadata.component.bom-ref (the
        // main-module PURL). Pre-084 this was always `name@version`, which
        // produced an orphan ref in dependencies[] and compositions[]
        // because metadata.rs:391-409 already returns the PURL for the
        // main-module case while builder.rs kept emitting the legacy
        // short-form. Detection uses the same `mikebom:component-role:
        // main-module` annotation as the override-filter at lines 272-293.
        let main_module_purl: Option<String> = if !override_active {
            effective_components
                .iter()
                .find(|c| {
                    c.extra_annotations
                        .get("mikebom:component-role")
                        .and_then(|v| v.as_str())
                        == Some("main-module")
                })
                .map(|c| c.purl.as_str().to_string())
        } else {
            None
        };
        let target_ref: String = match main_module_purl.as_deref() {
            Some(purl) => purl.to_string(),
            None => format!("{}@{}", effective_target_name, effective_target_version),
        };

        // Milestone 158 (US1 + US2) — pre-compute selection so we can:
        //   (a) augment the effective relationships with root → loser
        //       edges (workspace-peer linkage per FR-002).
        //   (b) run the multi-root BFS reachability pass over the
        //       AUGMENTED graph and thread the result into
        //       `build_metadata` for the two document-scope
        //       annotations (FR-003 + FR-004).
        //
        // We duplicate the `select_root` call that `build_metadata`
        // will also make internally (needed for milestone 127's
        // `mikebom:root-selection-heuristic`). Both callers see the
        // same result — the ladder is deterministic — so no drift.
        let m158_selection = crate::generate::root_selector::select_root(
            effective_components,
            &self.root_override,
            scan_target_coord,
            target_name,
            "0.0.0",
        );

        // FR-002 workspace-peer linkage: for each loser Purl, emit a
        // synthetic `root → loser` DependsOn edge. `build_dependencies`
        // and `build_relationships` (SPDX side) naturally emit these
        // via the existing infrastructure once they're in the list.
        let m158_workspace_peer_edges: Vec<Relationship> =
            crate::generate::graph_completeness::build_workspace_peer_edges(
                &m158_selection,
                effective_components,
            );

        // Milestone 192 — pre-rewrite dropped-main-module relationships
        // BEFORE the classifier runs. When operator-override drops a
        // native main-module (e.g., `--root-name X` on a Go source scan
        // that had `pkg:golang/github.com/example/foo` as its detected
        // mainmod), the mainmod's outgoing DependsOn edges are still in
        // `relationships` with `.from = <dropped-purl>`. The m086 rewrite
        // block below re-anchors those edges onto `target_ref`; without
        // running that rewrite here, `compute_graph_completeness` sees
        // stale edges pointing FROM components that no longer exist in
        // `effective_components`, and BFS from the operator's synthetic
        // target_ref can't reach the transitive Go/npm/etc. deps that
        // used to hang off the dropped mainmod. Pre-m192 this manifested
        // as `partial: multi-ecosystem-partial-root: <eco>`; post-m192
        // (post-bfs.rs fix) it manifested as `partial: orphaned-
        // components-detected: N` because the m192 synthesis empties
        // ecosystems_without_root but doesn't rebuild the transitive
        // reachability from target_ref.
        //
        // Fix: apply the m086 rewrite eagerly so the classifier operates
        // on the same edge topology that build_dependencies (line 626)
        // will emit. Reuse `dropped_main_module_purls` computed above.
        // Extracted to `graph_completeness::rewrite_dropped_mainmod_edges`
        // in m194 US4 so SPDX 2.3 + SPDX 3 emitters share the same
        // pre-rewrite and reach classifier `complete` on operator-
        // override scans (SC-005).
        let m192_prerewritten_relationships: Vec<Relationship> =
            crate::generate::graph_completeness::rewrite_dropped_mainmod_edges(
                relationships,
                &dropped_main_module_purls,
                &target_ref,
            );
        let metadata_relationships_augmented: Vec<Relationship> = m192_prerewritten_relationships
            .iter()
            .cloned()
            .chain(m158_workspace_peer_edges.iter().cloned())
            .collect();

        // FR-008 multi-root BFS on the AUGMENTED + REWRITTEN graph.
        let graph_completeness =
            crate::generate::graph_completeness::compute_graph_completeness(
                effective_components,
                &metadata_relationships_augmented,
                &m158_selection,
                &target_ref,
            );

        // FR-013 tracing log line (research §R8). Grep-friendly for
        // CI-log analysis, follows the milestone-157 FR-007 precedent.
        tracing::info!(
            value = %graph_completeness.value,
            reachable_count = graph_completeness.reachable_count,
            total_count = graph_completeness.total_count,
            orphan_count = graph_completeness.orphan_count,
            reason_codes = ?graph_completeness.reason_codes,
            "graph completeness computed"
        );

        let metadata = build_metadata(
            target_name,
            "0.0.0",
            self.config.generation_context.clone(),
            effective_components,
            &self.os_release_missing_fields,
            integrity,
            scan_target_coord,
            self.source_document_binding.as_ref(),
            &self.identifiers,
            &self.root_override,
            &self.user_metadata,
            self.sbom_type_override,
            self.file_inventory_stats.as_ref(),
            self.file_inventory_mode.as_deref(),
            self.collisions_summary.as_ref(),
            &graph_completeness,
            self.go_transitive_coverage.as_ref(),
            self.go_workspace_mode.as_ref(),
            self.go_transitive_fallback_count,
            self.go_cache_warming.as_ref(),
            self.helm_extraction_mode.as_ref(),
            self.image_source.as_ref(),
            self.compiler_pipeline.as_ref(),
        );
        // Milestone 076 — track per-component identifier matches so
        // we can emit a warn for any selector that matched zero
        // components (FR-010 / VR-076-004).
        let mut match_counts: std::collections::BTreeMap<usize, usize> =
            std::collections::BTreeMap::new();
        for i in 0..self.component_identifiers.len() {
            match_counts.insert(i, 0);
        }
        let cdx_components = self.build_components(effective_components, &mut match_counts)?;
        for (idx, count) in &match_counts {
            if *count == 0 {
                let flag = &self.component_identifiers[*idx];
                tracing::warn!(
                    selector = %flag.selector_purl,
                    scheme = flag.scheme.as_str(),
                    value = flag.value.as_str(),
                    "--component-id selector `{}` matched zero components; \
                     identifier `{}:{}` not attached",
                    flag.selector_purl,
                    flag.scheme.as_str(),
                    flag.value.as_str(),
                );
            }
        }
        let compositions = build_compositions(
            integrity,
            &target_ref,
            effective_components,
            complete_ecosystems,
        );
        // Milestone 084 — when override has dropped main-module components,
        // closure-invariant fix: relationships whose `from` is one of the
        // dropped PURLs would otherwise leave a dangling `dependencies[].ref`
        // entry (the dropped PURL isn't in `components[]` and isn't
        // `metadata.component.bom-ref` under override).
        //
        // Milestone 086 — REWRITE rather than FILTER. The original
        // milestone-084 fix dropped these relationships entirely (Option A
        // in research §2), which left target_ref empty and triggered the
        // `dependencies.rs:78-91` primary-dep fallback to synthesize
        // edges from "components nothing else depends on". For projects
        // whose direct deps form a non-tree DAG (some direct deps are
        // also transitively depended on by other deps), the synthesis
        // silently dropped legitimate edges. Real-world reproduction:
        // hosted-guac-mgmt under `--root-name slack-notifier --root-
        // version narsa` lost 3 of 15 direct edges. Option B (rewrite)
        // re-anchors each dropped-main-module-keyed edge onto target_ref,
        // preserving every edge with the override identity and still
        // satisfying the closure invariant.
        let filtered_relationships_owned: Option<Vec<Relationship>> =
            if !dropped_main_module_purls.is_empty() {
                let dropped: std::collections::HashSet<&str> = dropped_main_module_purls
                    .iter()
                    .map(|s| s.as_str())
                    .collect();
                let rewritten: Vec<Relationship> = relationships
                    .iter()
                    .map(|r| {
                        if dropped.contains(r.from.as_str()) {
                            Relationship {
                                from: target_ref.clone(),
                                to: r.to.clone(),
                                relationship_type: r.relationship_type.clone(),
                                provenance: r.provenance.clone(),
                            }
                        } else {
                            r.clone()
                        }
                    })
                    .collect();
                Some(rewritten)
            } else {
                None
            };
        let effective_relationships_base: &[Relationship] = filtered_relationships_owned
            .as_deref()
            .unwrap_or(relationships);
        // Milestone 158 US1 — append the workspace-peer synthetic
        // edges computed above so `build_dependencies` naturally
        // emits root → each-loser edges in the CDX `dependencies[]`
        // output. Reuses the same edge set the graph-completeness
        // BFS was run against for internal consistency.
        let effective_relationships_owned: Option<Vec<Relationship>> =
            if !m158_workspace_peer_edges.is_empty() {
                Some(
                    effective_relationships_base
                        .iter()
                        .cloned()
                        .chain(m158_workspace_peer_edges.iter().cloned())
                        .collect(),
                )
            } else {
                None
            };
        let effective_relationships: &[Relationship] = effective_relationships_owned
            .as_deref()
            .unwrap_or(effective_relationships_base);
        let deps = build_dependencies(effective_components, effective_relationships, &target_ref);
        let vulnerabilities = build_vulnerabilities(effective_components);

        // Milestone 080 — build CDX 1.6 `bom.annotations[]` for the
        // user-supplied --metadata-comment, --annotator + --annotation-
        // comment pairs, and any 2nd+ Organization creators that don't
        // fit in metadata.manufacturer. Empty when user_metadata is
        // not active. Subjects[] points at the root component's bom-
        // ref so the CDX 1.6 schema's `subjects: required, uniqueItems`
        // contract is satisfied.
        let user_annotations = if self.user_metadata.is_active() {
            let root_bom_ref = metadata
                .get("component")
                .and_then(|c| c.get("bom-ref"))
                .and_then(|v| v.as_str())
                .unwrap_or(target_ref.as_str())
                .to_string();
            let timestamp = metadata
                .get("timestamp")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            super::metadata::build_user_annotations(
                &self.user_metadata,
                &root_bom_ref,
                &timestamp,
            )
        } else {
            Vec::new()
        };

        let mut bom = json!({
            "bomFormat": "CycloneDX",
            "specVersion": "1.6",
            "serialNumber": serial_number,
            "version": 1,
            "metadata": metadata,
            "components": cdx_components,
            "compositions": compositions,
            "dependencies": deps,
            "vulnerabilities": vulnerabilities
        });
        // Milestone 119 (#326) — supplement-declared `services[]`. The
        // section is omitted entirely when no supplement is in effect
        // OR the supplement declared zero services, preserving byte-
        // identity with pre-119 emission per FR-013 / SC-006.
        let supplement_services =
            crate::supplement::current_services().unwrap_or_default();
        let services_value = super::services::build_services(&supplement_services);
        if !services_value.is_null() {
            if let Some(obj) = bom.as_object_mut() {
                obj.insert("services".to_string(), services_value);
            }
        }
        if !user_annotations.is_empty() {
            if let Some(obj) = bom.as_object_mut() {
                obj.insert("annotations".to_string(), json!(user_annotations));
            }
        }

        Ok(bom)
    }

    /// Build the CycloneDX components array from resolved components.
    ///
    /// Components carrying `parent_purl = Some(parent)` are emitted
    /// nested under their parent's `component.components[]` array per
    /// CDX 1.6's nested-components shape — used today for Maven
    /// shade-plugin fat-jar vendored coords. Nested entries get a
    /// composite bom-ref (`<child-purl>#<parent-purl>`) so the CDX
    /// document's bom-ref uniqueness invariant holds even when the
    /// same coord appears nested under multiple parents AND
    /// standalone. Top-level entries (parent_purl = None) keep their
    /// plain-PURL bom-ref.
    ///
    /// If a component's declared parent_purl doesn't match any
    /// top-level component's PURL (orphan), we gracefully fall back to
    /// emitting it top-level with its plain-PURL bom-ref — better than
    /// losing the component entirely. This can happen when the Maven
    /// scanner couldn't identify a fat-jar's primary coord but still
    /// extracted vendored children.
    fn build_components(
        &self,
        components: &[ResolvedComponent],
        match_counts: &mut std::collections::BTreeMap<usize, usize>,
    ) -> anyhow::Result<serde_json::Value> {
        // Milestones 053 (Go) + 064 (cargo) FR-001a: a main-module is
        // emitted via CDX `metadata.component` per Constitution
        // Principle V (native BOM-subject construct). Skip it here so
        // it does NOT also appear as a sibling in the top-level
        // `components[]` array — sibling-emission is the pre-053
        // pattern these milestones replace. Edges from the main-
        // module to direct deps continue to emit via `dependencies[]`
        // because the existing edge-emission loop reads relationships
        // keyed by the main-module's PURL, which
        // `metadata.component.bom-ref` matches.
        //
        // **Multi-main-module case (cargo workspace, polyglot)**:
        // when N > 1 main-modules exist, NONE are promoted to
        // `metadata.component` (see `metadata.rs` — the placeholder
        // path is used instead). In that case all N main-modules
        // MUST emit normally in `components[]` so consumers can find
        // every workspace member. We detect this by counting the
        // main-modules: skip from `components[]` only when there's
        // exactly one (matching `metadata.rs`'s promotion predicate).
        let main_module_count = components
            .iter()
            .filter(|c| {
                c.extra_annotations
                    .get("mikebom:component-role")
                    .and_then(|v| v.as_str())
                    == Some("main-module")
            })
            .count();
        let is_main_module = |c: &ResolvedComponent| {
            c.extra_annotations
                .get("mikebom:component-role")
                .and_then(|v| v.as_str())
                == Some("main-module")
        };
        let is_promoted_main_module = |c: &ResolvedComponent| {
            main_module_count == 1 && is_main_module(c)
        };

        // First pass: identify top-level PURLs so we can route children
        // that reference valid parents. Orphans fall back to top-level.
        let top_level_purls: std::collections::HashSet<String> = components
            .iter()
            .filter(|c| c.parent_purl.is_none() && !is_promoted_main_module(c))
            .map(|c| c.purl.as_str().to_string())
            .collect();

        // Build one JSON entry per component up front, keyed by the
        // component's canonical PURL (plus its parent_purl, so two
        // nested siblings with the same PURL under different parents
        // don't collide). We'll fold children into their parents
        // in a second pass.
        let mut cdx_components: Vec<serde_json::Value> = Vec::new();
        // Map from parent PURL to list of child entry indices into
        // cdx_components. Children get stripped from cdx_components
        // after folding.
        let mut children_indices_by_parent: std::collections::BTreeMap<
            String,
            Vec<usize>,
        > = std::collections::BTreeMap::new();

        for component in components {
            // Milestone 053: skip the Go main-module — it lives in
            // `metadata.component`, not `components[]`.
            if is_promoted_main_module(component) {
                continue;
            }
            // Decide this entry's bom-ref: plain PURL when top-level,
            // `<child>#<parent>` composite when the parent exists in
            // the top-level set. Orphans (declared parent not in the
            // top-level set) get demoted to top-level with plain ref.
            let effective_parent: Option<&String> = component
                .parent_purl
                .as_ref()
                .filter(|p| top_level_purls.contains(p.as_str()));
            let bom_ref = match effective_parent {
                Some(parent) => format!("{}#{}", component.purl.as_str(), parent),
                None => component.purl.as_str().to_string(),
            };
            // Milestone 133 US1.B: file-tier components carry a
            // `mikebom:component-tier = "file"` annotation. When
            // present, override `type` to the CDX-native `"file"`
            // (per FR-001) and OMIT `purl` (per FR-009 — the
            // placeholder PURL is in-process identity only; the
            // wire shape has no PURL for file-tier).
            let is_file_tier = component
                .extra_annotations
                .get(crate::scan_fs::file_tier::COMPONENT_TIER_KEY)
                .and_then(|v| v.as_str())
                == Some(crate::scan_fs::file_tier::COMPONENT_TIER_FILE_VALUE);
            let mut entry = if is_file_tier {
                json!({
                    "type": crate::scan_fs::file_tier::FILE_TIER_CDX_TYPE,
                    "name": component.name,
                    "version": component.version,
                    "bom-ref": bom_ref,
                    "evidence": build_evidence(&component.evidence, &component.occurrences, None, &[])
                })
            } else {
                // Milestone 105 phase 2E: extra (None, &[]) params are
                // the cross-reader-dedup emission slots. Wired through
                // by US1-US6 reader phases — until then the call shape
                // produces byte-identical output to the pre-milestone-
                // 105 builder.
                //
                // Milestone 191 (#558): omit `version` field entirely
                // when component.version is empty (design-tier
                // component with no source-tier resolution). Matches
                // purl-spec convention where the versionless PURL is
                // the canonical shape. Build the entry via
                // `serde_json::Map` so the version key can be
                // conditionally absent (vs `json!({...})` which always
                // materializes every listed key).
                let mut base = serde_json::Map::new();
                base.insert(
                    "type".to_string(),
                    json!(binary_role_to_cdx_type(component.binary_role)),
                );
                base.insert("name".to_string(), json!(component.name));
                if !component.version.is_empty() {
                    base.insert("version".to_string(), json!(component.version));
                }
                base.insert("purl".to_string(), json!(component.purl.as_str()));
                base.insert("bom-ref".to_string(), json!(bom_ref));
                base.insert(
                    "evidence".to_string(),
                    build_evidence(
                        &component.evidence,
                        &component.occurrences,
                        None,
                        &[],
                    ),
                );
                serde_json::Value::Object(base)
            };

            // Milestone 052/part-2: native CDX `scope` field. Per
            // FR-010, components with non-Runtime lifecycle_scope
            // emit `scope: "excluded"` (CDX 1.6 enum value meaning
            // "not in deployment footprint"). Runtime + None omit
            // the field (default = `required`). The dev-vs-build-
            // vs-test distinction lives in the
            // `mikebom:lifecycle-scope` property emitted later in
            // the properties[] block — CDX's 3-value `scope` enum
            // doesn't express that finer split.
            if self.config.include_dev {
                if let Some(scope) = component.lifecycle_scope {
                    if scope.is_non_runtime() {
                        entry["scope"] = json!("excluded");
                    }
                }
            }

            // Milestone 112 (T016): `BuildInclusion::NotNeeded`
            // components emit the native `scope: "excluded"`
            // UNCONDITIONALLY — independent of the include-dev gate
            // above (clarification 2026-06-11). Rationale: `go mod
            // why` proved the production build does not need this
            // module, which is a build-graph fact, not a lifecycle-
            // scope preference the operator can toggle. The component
            // is kept in the SBOM (never dropped by scope filtering —
            // its lifecycle_scope stays `None`, so `--exclude-scope`
            // can't match it) with the finer-grained reason carried by
            // the `mikebom:build-inclusion` property below.
            if component.build_inclusion
                == Some(mikebom_common::resolution::BuildInclusion::NotNeeded)
            {
                entry["scope"] = json!("excluded");
            }

            // Include hashes if configured.
            if self.config.include_hashes && !component.hashes.is_empty() {
                let hashes: Vec<serde_json::Value> = component
                    .hashes
                    .iter()
                    .map(|h| {
                        json!({
                            "alg": format!("{}", h.algorithm).to_uppercase().replace("SHA", "SHA-"),
                            "content": h.value.as_str()
                        })
                    })
                    .collect();
                entry["hashes"] = json!(hashes);
            }

            // CDX 1.6 license emission. Two shapes per item:
            // - `{"license": {"id": "<SPDX>", "acknowledgement": "..."}}`
            //   for single-identifier licenses on the SPDX list.
            //   sbomqs's `comp_with_valid_licenses` requires this form.
            // - `{"expression": "<expr>", "acknowledgement": "..."}` for
            //   compound (AND/OR/WITH), unknown identifiers, LicenseRefs.
            //
            // The `acknowledgement` enum (CDX 1.6) distinguishes:
            // - "declared" — what the package author asserted in their
            //   manifest (mikebom: `component.licenses`)
            // - "concluded" — result of comprehensive analysis
            //   (mikebom: `component.concluded_licenses`, populated by
            //   the ClearlyDefined enrichment source)
            // sbomqs's `comp_with_licenses`, `comp_with_valid_licenses`,
            // `comp_no_deprecated_licenses`, `comp_no_restrictive_licenses`
            // all read concluded; `comp_with_declared_licenses` reads
            // declared.
            // CDX 1.6 `licenses` schema is oneOf:
            // - An array of `{license: {id/name, ...}}` objects (any
            //   length), OR
            // - An array of exactly ONE `{expression: ...}` entry.
            // Mixing the two shapes, or emitting multiple expression
            // entries, is a schema error. We accumulate both declared
            // + concluded sources, split `A OR B` compounds into
            // individual ids when possible, and fall back to a single
            // expression entry (concluded > declared) only when a
            // genuine compound remains.
            let mut all_licenses: Vec<serde_json::Value> = Vec::new();
            let mut pending_expression: Option<(&str, &str)> = None;
            let sources: [(&[SpdxExpression], &str); 2] = [
                (&component.licenses, "declared"),
                (&component.concluded_licenses, "concluded"),
            ];
            for (exprs, ack) in sources {
                for l in exprs {
                    if let Some(id) = l.as_spdx_id() {
                        all_licenses.push(json!({
                            "license": { "id": id, "acknowledgement": ack }
                        }));
                    } else if l.as_str().starts_with("LicenseRef-")
                        || l.as_str().starts_with("DocumentRef-")
                    {
                        // Bare LicenseRef-* / DocumentRef-* aren't valid
                        // in CDX `license.id` (id is restricted to the
                        // SPDX list). Emit via `license.name` — schema-
                        // legal and counted by sbomqs.
                        all_licenses.push(json!({
                            "license": { "name": l.as_str(), "acknowledgement": ack }
                        }));
                    } else if let Some(tokens) = try_split_or_compound(l.as_str()) {
                        for tok in tokens {
                            // Milestone 202: license_entry_for_token may
                            // return `Value::Null` for tokens whose
                            // sanitizer returns None (all-invalid-chars).
                            // Drop those rather than emitting a null in
                            // the licenses[] array.
                            let entry = license_entry_for_token(&tok, ack);
                            if !entry.is_null() {
                                all_licenses.push(entry);
                            }
                        }
                    } else {
                        pending_expression = Some((l.as_str(), ack));
                    }
                }
            }
            let final_licenses = if let Some((expr, ack)) = pending_expression {
                vec![json!({ "expression": expr, "acknowledgement": ack })]
            } else {
                all_licenses
            };
            if !final_licenses.is_empty() {
                entry["licenses"] = json!(final_licenses);
            }

            // Include supplier if present.
            if let Some(ref supplier) = component.supplier {
                entry["supplier"] = json!({
                    "name": supplier
                });
            }

            // External references — VCS repos, homepages, etc.
            // Drives sbomqs `comp_with_source_code` when a `vcs`
            // entry is present.
            if !component.external_references.is_empty() {
                let refs: Vec<serde_json::Value> = component
                    .external_references
                    .iter()
                    .map(|r| json!({ "type": r.ref_type, "url": r.url }))
                    .collect();
                entry["externalReferences"] = json!(refs);
            }

            // CycloneDX `component.cpe` is single-valued. Emit the first
            // (highest-signal) synthesized candidate there; stash the full
            // vendor-candidate list under a property so downstream NVD
            // matchers can take the union of heuristics instead of being
            // locked to one guess.
            let mut properties: Vec<serde_json::Value> = Vec::new();
            if !component.cpes.is_empty() {
                entry["cpe"] = json!(component.cpes[0]);
                if component.cpes.len() > 1 {
                    properties.push(json!({
                        "name": "mikebom:cpe-candidates",
                        "value": component.cpes.join(" | ")
                    }));
                }
            }

            // Include source file paths if configured and present.
            // Milestone 133 US2.1 (FR-012 Defect B): emit as JSON array
            // instead of pre-133's comma-separated string. Path
            // normalization (Defects A + C — rootfs-prefix-strip and
            // leading-`/`-strip) is done at source-population time in
            // `scan_fs::mod.rs`, so consumers here just serialize the
            // already-clean `source_file_paths` Vec.
            if self.config.include_source_files {
                if let Some(value) = crate::scan_fs::sbom_path::source_files_as_json_array(
                    &component.evidence.source_file_paths,
                ) {
                    properties.push(json!({
                        "name": "mikebom:source-files",
                        "value": value,
                    }));
                }
            }

            // Milestone 052: `mikebom:lifecycle-scope` property carrying
            // the finer-grained dev/build/test distinction that CDX 1.6's
            // 3-value native `scope` enum cannot express. The native
            // `scope: "excluded"` field is set on the component itself
            // (in the component-builder block above the properties array
            // — see the lifecycle_scope branch on `Component::scope`).
            // Constitution Principle V (v1.4.0): native fields take
            // precedence; this property carries the carve-out for
            // information the standard doesn't natively express.
            if self.config.include_dev {
                if let Some(scope) = component.lifecycle_scope.filter(|s| s.is_non_runtime()) {
                    properties.push(json!({
                        "name": "mikebom:lifecycle-scope",
                        "value": scope.as_str()
                    }));
                }
            }
            // Milestone 112: `mikebom:build-inclusion` property from
            // the typed `BuildInclusion` field. `unknown` carries no
            // native CDX construct (the 3-value `scope` enum cannot
            // express "undetermined"); `not-needed`'s PRIMARY signal is
            // the native `scope: "excluded"` set in the scope block —
            // this property carries the finer-grained reason
            // (contracts/annotations.md, Constitution V/X). The
            // companion `mikebom:build-inclusion-derivation` flows
            // through the extra_annotations bag.
            if let Some(inclusion) = component.build_inclusion {
                properties.push(json!({
                    "name": "mikebom:build-inclusion",
                    "value": inclusion.as_str()
                }));
            }
            // Milestone 199 — always-array shape. CDX `properties[].value`
            // is a string; the JSON array is serialized-into-string per
            // the m197 contracts/annotation-shapes.md wire contract.
            if !component.requirement_ranges.is_empty() {
                properties.push(json!({
                    "name": "mikebom:requirement-ranges",
                    "value": serde_json::to_string(&component.requirement_ranges).unwrap_or_default(),
                }));
            }
            if let Some(ref src_type) = component.source_type {
                properties.push(json!({
                    "name": "mikebom:source-type",
                    "value": src_type
                }));
            }
            // `mikebom:co-owned-by` — set by the Maven JAR walker on
            // coords extracted from JARs whose bytes are ALSO claimed
            // by an OS package-db reader (RPM/deb/apk). Value is the
            // owner ecosystem. Downstream consumers can filter on this
            // property to collapse dual-identity components to a
            // single view (e.g. drop the Maven coord when they only
            // want distro-level CVE tracking via the RPM component).
            // See docs/design-notes.md "Dual-identity: JAR-embedded
            // Maven coords in RPM-owned artifacts" for rationale.
            if let Some(ref owner) = component.co_owned_by {
                properties.push(json!({
                    "name": "mikebom:co-owned-by",
                    "value": owner
                }));
            }
            // Evidence-derived provenance properties. Replaces the
            // former `evidence.identity[].tools` entries — those fail
            // CDX 1.6 schema because `tools[]` must be bom-refs to
            // declared BOM elements, which source_connection_ids and
            // deps.dev markers are not. Properties are the idiomatic
            // home for scanner-specific provenance data.
            properties.extend(evidence_to_properties(&component.evidence));
            // `mikebom:sbom-tier` — the traceability-ladder classifier
            // introduced in milestone 002 (spec FR-021a, research R13).
            // Emitted on every component that carries one. Values:
            // build | deployed | analyzed | source | design.
            if let Some(ref tier) = component.sbom_tier {
                properties.push(json!({
                    "name": "mikebom:sbom-tier",
                    "value": tier
                }));
            }
            // `mikebom:npm-role` — feature 005 US1 (spec FR-001, FR-003).
            // Emitted only on npm components discovered inside npm's own
            // bundled tree (`**/node_modules/npm/node_modules/**`) during
            // --image scans. Value: `internal`. Absent on application
            // deps (the vast majority) and on all --path-mode scans,
            // where the internals are filtered out before they reach
            // the builder. See data-model.md §PackageDbEntry.npm_role.
            if let Some(ref role) = component.npm_role {
                properties.push(json!({
                    "name": "mikebom:npm-role",
                    "value": role
                }));
            }
            // `mikebom:raw-version` — feature 005 US4 (spec FR-013).
            // Verbatim `VERSION-RELEASE` string from the rpmdb header.
            // Populated on every rpm component so downstream consumers
            // can cross-reference `rpm -qa`'s `%{VERSION}-%{RELEASE}`
            // column without re-parsing the PURL. Absent on non-rpm
            // components today; reserved for other ecosystems to opt
            // in later via the same field on `PackageDbEntry`.
            if let Some(ref raw) = component.raw_version {
                properties.push(json!({
                    "name": "mikebom:raw-version",
                    "value": raw
                }));
            }
            // `mikebom:buildinfo-status` — milestone 003 (spec FR-015).
            // Emitted ONLY on file-level Go binary components where
            // `runtime/debug.BuildInfo` couldn't be recovered. Operators
            // distinguish "no modules found" from "scan failed" via the
            // value: `"missing"` (stripped binary, magic absent) or
            // `"unsupported"` (Go <1.18 pre-inline format).
            if let Some(ref status) = component.buildinfo_status {
                properties.push(json!({
                    "name": "mikebom:buildinfo-status",
                    "value": status
                }));
            }
            // `mikebom:evidence-kind` — milestone 004 (spec FR-004,
            // contracts/schema.md). Six-value canonical enum identifying
            // how the component was discovered. Consumers filter by this.
            // Valid values enforced by `debug_assert!` per data-model.md
            // §Validation rules.
            if let Some(ref kind) = component.evidence_kind {
                debug_assert!(
                    matches!(
                        kind.as_str(),
                        "rpm-file"
                            | "rpmdb-sqlite"
                            | "rpmdb-bdb"
                            | "dynamic-linkage"
                            | "elf-note-package"
                            | "embedded-version-string"
                            | "symbol-fingerprint"
                            | "python-stdlib-collapsed"
                            | "jdk-runtime-collapsed"
                            | "alpm-local-db"
                            | "brew-install-receipt"
                            | "brew-cask-metadata"
                            | "pubspec-lock"
                            | "pubspec-yaml"
                            | "composer-lock"
                            | "composer-json"
                            | "composer-installed-json"
                            | "cocoapods-podfile-lock"
                            | "cocoapods-podfile"
                            | "cocoapods-manifest-lock"
                            | "mix-lock"
                            | "mix-exs"
                            | "rebar-lock"
                            | "rebar-config"
                            | "app-src"
                            | "sbt-lock"
                            | "sbt-build"
                            | "cabal-freeze"
                            | "stack-yaml-lock"
                            | "cabal-pkg-descriptor"
                            // Milestone 169 (T013, closes #500) — ipk
                            // archive-file reader + opkg installed-DB
                            // hardening. See spec.md FR-009 + FR-015.
                            | "ipk-file"
                            | "opkg-status-db"
                            // Milestone 188 (#455) — Helm chart reader.
                            // Chart-level components carry
                            // `helm-chart-yaml` or `helm-chart-lock`;
                            // image-ref components extracted from
                            // `templates/*.yaml` carry
                            // `helm-template-image-ref`.
                            | "helm-chart-yaml"
                            | "helm-chart-lock"
                            | "helm-template-image-ref"
                    ),
                    "mikebom:evidence-kind value '{kind}' is not in the canonical \
                     enum (rpm-file | rpmdb-sqlite | rpmdb-bdb | \
                     dynamic-linkage | elf-note-package | \
                     embedded-version-string | symbol-fingerprint | \
                     python-stdlib-collapsed | jdk-runtime-collapsed | \
                     alpm-local-db | brew-install-receipt | brew-cask-metadata | \
                     pubspec-lock | pubspec-yaml | composer-lock | composer-json | \
                     composer-installed-json | cocoapods-podfile-lock | \
                     cocoapods-podfile | cocoapods-manifest-lock | mix-lock | mix-exs | \
                     rebar-lock | rebar-config | app-src | sbt-lock | sbt-build | \
                     cabal-freeze | stack-yaml-lock | cabal-pkg-descriptor | \
                     ipk-file | opkg-status-db)"
                );
                properties.push(json!({
                    "name": "mikebom:evidence-kind",
                    "value": kind
                }));
            }
            // Milestone 004 US2 binary-component properties. Each is
            // emitted only when Some(...) — the absence of the property
            // is itself informative (e.g. no `mikebom:binary-class` =
            // non-binary component).
            if let Some(ref confidence) = component.confidence {
                debug_assert_eq!(
                    confidence, "heuristic",
                    "mikebom:confidence is currently only valid as 'heuristic'"
                );
                properties.push(json!({
                    "name": "mikebom:confidence",
                    "value": confidence
                }));
            }
            if let Some(ref class) = component.binary_class {
                debug_assert!(
                    matches!(class.as_str(), "elf" | "macho" | "pe"),
                    "mikebom:binary-class value '{class}' is not in {{elf, macho, pe}}"
                );
                properties.push(json!({
                    "name": "mikebom:binary-class",
                    "value": class
                }));
            }
            if let Some(stripped) = component.binary_stripped {
                properties.push(json!({
                    "name": "mikebom:binary-stripped",
                    "value": if stripped { "true" } else { "false" }
                }));
            }
            if let Some(ref linkage) = component.linkage_kind {
                debug_assert!(
                    matches!(linkage.as_str(), "dynamic" | "static" | "mixed"),
                    "mikebom:linkage-kind value '{linkage}' is not in {{dynamic, static, mixed}}"
                );
                properties.push(json!({
                    "name": "mikebom:linkage-kind",
                    "value": linkage
                }));
            }
            if component.detected_go == Some(true) {
                properties.push(json!({
                    "name": "mikebom:detected-go",
                    "value": "true"
                }));
            }
            if component.shade_relocation == Some(true) {
                properties.push(json!({
                    "name": "mikebom:shade-relocation",
                    "value": "true"
                }));
            }
            if let Some(ref packed) = component.binary_packed {
                debug_assert!(
                    matches!(packed.as_str(), "upx" | "none"),
                    "mikebom:binary-packed value '{packed}' is not in the canonical \
                     enum (upx | none); milestone-096 Q2 always-emits 'none' on \
                     file-level components when no packer is detected"
                );
                properties.push(json!({
                    "name": "mikebom:binary-packed",
                    "value": packed
                }));
            }

            // Milestone 023: generic per-component annotation bag.
            // Each entry surfaces as a CycloneDX property. Strings
            // pass through verbatim; other JSON values are
            // serde_json-stringified (matches the existing convention
            // for array- and object-shaped CDX property values).
            //
            // Milestone 127: filter out internal-only keys (the
            // `mikebom:is-workspace-root` signal that drives root-selector
            // logic but is NOT meant to surface in emitted SBOMs).
            for (key, value) in &component.extra_annotations {
                if crate::generate::root_selector::is_internal_emission_key(key)
                    || crate::generate::root_selector::is_field_owned_annotation_key(key)
                {
                    // Milestone 145 US3 (FR-009): skip keys already
                    // emitted from a field-derived source (e.g.,
                    // `mikebom:source-files` comes from
                    // `c.evidence.source_file_paths` higher up in
                    // this function — re-emitting from the bag
                    // would double-stamp and produce value drift.
                    continue;
                }
                let value_str = match value {
                    serde_json::Value::String(s) => s.clone(),
                    other => serde_json::to_string(other).unwrap_or_default(),
                };
                properties.push(json!({
                    "name": key,
                    "value": value_str,
                }));
            }

            // Milestone 210: per-component compiler-pipeline attribution
            // (C130 + C131 + C134). Only emitted when the scan was
            // invoked via `mikebom trace` AND at least one compiler
            // invocation was captured. Matched via write-set intersection
            // with the component's known file paths per
            // contracts/annotations.md A-1/A-2/A-5. `Traced` ⇒ both C130
            // (source-read-set payload) + C131 (source label).
            // `Unknown` ⇒ C131 only. C134 = `"true"` when the doc-scope
            // completeness is `Partial(AttachLate)` — the trace attached
            // mid-build so every captured component potentially has a
            // partial read-set (best-effort per-component signal from
            // doc-scope truth; per-invocation granularity is a future
            // milestone). No pipeline ⇒ none of the three, preserving
            // byte-identity for scan-mode consumers.
            if let Some(ref pipeline) = self.compiler_pipeline {
                let mapping = crate::generate::compiler_pipeline_annotation::map_component_to_source_read_set(
                    component,
                    pipeline,
                );
                if let Some(payload) = mapping.payload {
                    properties.push(json!({
                        "name": "mikebom:source-read-set",
                        "value": serde_json::to_string(&payload).unwrap_or_default(),
                    }));
                }
                properties.push(json!({
                    "name": "mikebom:read-set-source",
                    "value": mapping.source.as_wire_str(),
                }));
                if matches!(
                    pipeline.completeness,
                    mikebom_common::attestation::compiler_pipeline::CompletenessState::Partial {
                        reason: mikebom_common::attestation::compiler_pipeline::PartialReason::AttachLate,
                    }
                ) {
                    properties.push(json!({
                        "name": "mikebom:trace-attach-late",
                        "value": "true",
                    }));
                }
            }

            // Milestone 076: per-component user-defined identifiers
            // from `--component-id <PURL>=<scheme>:<value>` flags.
            // Match by byte-equality of `purl` per research §5; append
            // matching entries AFTER pre-existing properties (research
            // §6 — preserve original positions, lex-sort the new
            // entries by `(scheme, value)`).
            let mut new_per_component_props: Vec<(String, String)> = Vec::new();
            for (idx, flag) in self.component_identifiers.iter().enumerate() {
                if flag.selector_purl == component.purl.as_str() {
                    *match_counts.entry(idx).or_insert(0) += 1;
                    new_per_component_props.push((
                        flag.scheme.as_str().to_string(),
                        flag.value.as_str().to_string(),
                    ));
                }
            }
            new_per_component_props.sort();
            new_per_component_props.dedup();
            for (name, value) in new_per_component_props {
                properties.push(json!({
                    "name": name,
                    "value": value,
                }));
            }

            if !properties.is_empty() {
                entry["properties"] = json!(properties);
            }

            // Record index for parent-child folding. Orphans whose
            // declared parent isn't in the top-level set get routed
            // to top-level (effective_parent is None).
            let pushed_index = cdx_components.len();
            cdx_components.push(entry);
            if let Some(parent) = effective_parent {
                children_indices_by_parent
                    .entry(parent.clone())
                    .or_default()
                    .push(pushed_index);
            }
        }

        // Fold children into their parents. Walk in reverse-index
        // order so later removals don't shift earlier indices.
        let mut child_indices_to_remove: std::collections::BTreeSet<usize> =
            std::collections::BTreeSet::new();
        // Map parent PURL -> index in cdx_components. Built once.
        let mut parent_index_by_purl: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for (i, entry) in cdx_components.iter().enumerate() {
            if let Some(purl) = entry.get("purl").and_then(|v| v.as_str()) {
                // Top-level entries (those whose bom-ref equals the
                // plain PURL) are the only valid parents.
                let bom_ref = entry.get("bom-ref").and_then(|v| v.as_str()).unwrap_or("");
                if bom_ref == purl {
                    parent_index_by_purl.insert(purl.to_string(), i);
                }
            }
        }
        for (parent_purl, child_idxs) in &children_indices_by_parent {
            let Some(&parent_idx) = parent_index_by_purl.get(parent_purl) else {
                continue;
            };
            let mut child_entries: Vec<serde_json::Value> =
                Vec::with_capacity(child_idxs.len());
            for &ci in child_idxs {
                child_entries.push(cdx_components[ci].clone());
                child_indices_to_remove.insert(ci);
            }
            if !child_entries.is_empty() {
                cdx_components[parent_idx]["components"] = json!(child_entries);
            }
        }
        // Remove folded children from top-level (reverse order).
        for &idx in child_indices_to_remove.iter().rev() {
            cdx_components.remove(idx);
        }

        Ok(json!(cdx_components))
    }
}

/// Split an SPDX expression of the shape `A OR B OR C` OR
/// `A AND B AND C` into its constituent identifiers. Returns `None`
/// for expressions that mix operators, contain `WITH`, parentheses,
/// license refs, or any component that isn't a bare SPDX-list
/// identifier — those can't be represented as a set of independent
/// `{license: {id}}` entries without losing semantics.
///
/// Motivation: CDX 1.6 allows only ONE `{expression}` entry per
/// `licenses[]` array, and sbomqs `comp_with_licenses` scores credit
/// on `license.id` / `license.name` only, not on `expression`. So
/// `Apache-2.0 OR MIT` (cargo dual-licensed pattern) and
/// `BSD-2-Clause AND BSD-3-Clause` (ClearlyDefined curated-AND
/// pattern) both become multiple `{license: {id}}` entries.
///
/// For AND the split is semantically faithful (both licenses apply →
/// list both). For OR it's a compromise (the disjunction relation is
/// lost) but downstream readers still see every candidate ID.
/// Milestone 104 — map a `BinaryRole` to the CycloneDX 1.6
/// `Component.type` enum value. See
/// `specs/104-binary-role-classification/contracts/binary-role-cross-format-mapping.md`.
///
/// `None` (component did not come from the binary reader) and the
/// `Other` bucket both fall back to `"library"` — the historic
/// default that every binary-reader component used pre-milestone-104.
/// This preserves backward compatibility for consumers reading the
/// `type` field on components mikebom can't classify further.
pub(super) fn binary_role_to_cdx_type(role: Option<BinaryRole>) -> &'static str {
    match role {
        Some(BinaryRole::Application) => "application",
        Some(BinaryRole::SharedLibrary) => "library",
        Some(BinaryRole::Object) => "file",
        Some(BinaryRole::Other) | None => "library",
    }
}

fn try_split_or_compound(expr: &str) -> Option<Vec<String>> {
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains('(') || trimmed.contains(')') {
        return None;
    }
    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    if tokens.contains(&"WITH") {
        return None;
    }
    // Pick a single top-level operator. Mixed operators (e.g.
    // `A AND B OR C`) require parens for unambiguous parsing, so
    // bail — let the single-expression fallback handle them.
    let has_or = tokens.contains(&"OR");
    let has_and = tokens.contains(&"AND");
    let separator = match (has_or, has_and) {
        (true, false) => " OR ",
        (false, true) => " AND ",
        _ => return None,
    };
    let parts: Vec<&str> = trimmed.split(separator).map(str::trim).collect();
    if parts.len() < 2 {
        return None;
    }
    let mut tokens_out = Vec::with_capacity(parts.len());
    for p in parts {
        // Every operand must be a single token (SPDX id or
        // LicenseRef-*); whitespace inside an operand means the
        // expression has nested operators we can't flatten.
        if p.is_empty() || p.contains(char::is_whitespace) {
            return None;
        }
        tokens_out.push(p.to_string());
    }
    Some(tokens_out)
}

/// Map one split-expression token to the right CDX `license` shape.
///
/// Three-branch classifier post-milestone 202 (closes #579):
///
/// 1. **Pre-formed reference** (`LicenseRef-*` / `DocumentRef-*`): route to
///    `license.name` verbatim (schema-legal free-text label; sbomqs counts
///    it via `comp_with_licenses`).
/// 2. **SPDX-list-canonical identifier** (member of the SPDX License List
///    per `spdx::license_id`, or an SPDX exception per `spdx::exception_id`):
///    route to `license.id` — the canonical CDX 1.6 §5.4.4.1 slot. Value
///    preserved verbatim (no `try_canonical` normalization — that would
///    silently rewrite legacy long-form names like `GPL-2.0` → `GPL-2.0-only`
///    and drift emitted goldens).
/// 3. **Non-canonical operand** (compound-expression operand that isn't on
///    the SPDX List, e.g. `bzip2-1.0.4` from a Yocto recipe License field):
///    route to `license.name = "LicenseRef-<sanitized>"` per CDX 1.6
///    §5.4.4.2 escape-hatch convention. Uses the shared
///    `mikebom_common::types::license::sanitize_license_operand_to_ref`
///    helper — same function the SPDX 2.3 emitter uses (m152) — so both
///    formats produce byte-identical `LicenseRef-*` identifiers for the
///    same input token (FR-002 CDX/SPDX 2.3 parity).
///
/// Defensive fallback: if the sanitizer returns `None` (all-invalid-chars
/// input after filtering), emit `serde_json::Value::Null` so the caller's
/// filter drops the entry rather than producing schema-invalid output.
fn license_entry_for_token(token: &str, acknowledgement: &str) -> serde_json::Value {
    if token.starts_with("LicenseRef-") || token.starts_with("DocumentRef-") {
        return json!({
            "license": {
                "name": token,
                "acknowledgement": acknowledgement,
            }
        });
    }
    let is_spdx_list_id =
        spdx::license_id(token).is_some() || spdx::exception_id(token).is_some();
    if is_spdx_list_id {
        return json!({
            "license": {
                "id": token,
                "acknowledgement": acknowledgement,
            }
        });
    }
    match mikebom_common::types::license::sanitize_license_operand_to_ref(token) {
        Some(sanitized) => json!({
            "license": {
                "name": format!("LicenseRef-{sanitized}"),
                "acknowledgement": acknowledgement,
            }
        }),
        None => serde_json::Value::Null,
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use mikebom_common::resolution::{ResolutionEvidence, ResolutionTechnique};
    use mikebom_common::types::purl::Purl;

    // ---- Milestone 202 (issue #579) — license_entry_for_token ----

    /// FR-001 Branch 2: SPDX-list-canonical identifier routes to `license.id`.
    #[test]
    fn license_entry_for_token_routes_canonical_to_id_slot_m202() {
        let entry = license_entry_for_token("MIT", "declared");
        assert_eq!(
            entry,
            json!({"license": {"id": "MIT", "acknowledgement": "declared"}})
        );
        // Cross-check with additional canonical identifiers.
        for canonical in &["Apache-2.0", "GPL-3.0-only", "BSD-2-Clause", "MPL-2.0"] {
            let e = license_entry_for_token(canonical, "declared");
            assert_eq!(
                e["license"]["id"].as_str(),
                Some(*canonical),
                "canonical id `{canonical}` MUST route to license.id; got {e:?}"
            );
            assert!(
                e["license"]["name"].is_null(),
                "canonical id `{canonical}` MUST NOT populate license.name; got {e:?}"
            );
        }
    }

    /// FR-001 Branch 3: non-canonical operand routes to `license.name` with
    /// the `LicenseRef-<sanitized>` prefix per CDX 1.6 §5.4.4.2.
    #[test]
    fn license_entry_for_token_routes_non_canonical_to_licenseref_name_slot_m202() {
        let entry = license_entry_for_token("bzip2-1.0.4", "declared");
        assert_eq!(
            entry,
            json!({"license": {"name": "LicenseRef-bzip2-1.0.4", "acknowledgement": "declared"}})
        );
        // Cross-check additional non-canonical operands.
        for (operand, expected_ref) in &[
            ("custom-license", "LicenseRef-custom-license"),
            ("made-up-name-2.0", "LicenseRef-made-up-name-2.0"),
        ] {
            let e = license_entry_for_token(operand, "declared");
            assert_eq!(
                e["license"]["name"].as_str(),
                Some(*expected_ref),
                "non-canonical `{operand}` MUST route to LicenseRef-* via license.name; got {e:?}"
            );
            assert!(
                e["license"]["id"].is_null(),
                "non-canonical `{operand}` MUST NOT populate license.id; got {e:?}"
            );
        }
    }

    /// FR-003: pre-formed `LicenseRef-*` / `DocumentRef-*` tokens pass
    /// through verbatim — no double-prefixing.
    #[test]
    fn license_entry_for_token_preserves_pre_formed_licenseref_verbatim_m202() {
        for pre_formed in &[
            "LicenseRef-user-supplied",
            "LicenseRef-bzip2-1.0.4",
            "DocumentRef-doc:LicenseRef-external",
        ] {
            let entry = license_entry_for_token(pre_formed, "declared");
            assert_eq!(
                entry["license"]["name"].as_str(),
                Some(*pre_formed),
                "pre-formed `{pre_formed}` MUST pass through unchanged (no double-prefixing); got {entry:?}"
            );
        }
    }

    /// FR-002 sanitizer parity: the CDX splitter's sanitizer output MUST
    /// match the SPDX 2.3 side by using the SAME shared sanitizer function.
    /// Verifies the structural single-source-of-truth guarantee.
    #[test]
    fn license_entry_for_token_uses_shared_sanitizer_m202() {
        // Compute the expected LicenseRef- output by calling the shared
        // sanitizer directly.
        let expected_sanitized =
            mikebom_common::types::license::sanitize_license_operand_to_ref("bzip2 with spaces!")
                .expect("sanitizer produces non-empty output for this input");
        let expected_ref = format!("LicenseRef-{expected_sanitized}");

        // Compute the CDX splitter output for the same input.
        let entry = license_entry_for_token("bzip2 with spaces!", "declared");
        let cdx_name = entry["license"]["name"]
            .as_str()
            .expect("license.name populated for non-canonical operand");

        assert_eq!(
            cdx_name, expected_ref,
            "CDX splitter MUST call the shared sanitizer — structural parity per FR-002"
        );
    }

    /// Defensive fallback: all-invalid-chars input → sanitizer returns None
    /// → splitter emits Value::Null → caller filter drops the entry (no
    /// schema-invalid empty licenses[] element in output).
    #[test]
    fn license_entry_for_token_returns_null_for_all_invalid_chars_m202() {
        let entry = license_entry_for_token("!@#$", "declared");
        assert!(entry.is_null(), "all-invalid-chars input MUST return Value::Null");
    }

    fn clean_integrity() -> TraceIntegrity {
        TraceIntegrity {
            ring_buffer_overflows: 0,
            events_dropped: 0,
            uprobe_attach_failures: vec![],
            kprobe_attach_failures: vec![],
            partial_captures: vec![],
            bloom_filter_capacity: 100_000,
            bloom_filter_false_positive_rate: 0.01,
            filter_categories_applied: vec![],
        }
    }

    fn make_component(name: &str, version: &str) -> ResolvedComponent {
        let purl_str = format!("pkg:cargo/{name}@{version}");
        ResolvedComponent {
            build_inclusion: None,
            purl: Purl::new(&purl_str).expect("valid purl"),
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
            concluded_licenses: Vec::new(),
            hashes: vec![],
            supplier: None,
            cpes: vec![],
            advisories: vec![],
            occurrences: vec![],
            lifecycle_scope: None,
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

    #[test]
    fn bom_has_correct_top_level_structure() {
        let builder = CycloneDxBuilder::new(CycloneDxConfig::default());
        let components = vec![make_component("serde", "1.0.197")];
        let integrity = clean_integrity();

        let bom = builder
            .build(&components, &[], &integrity, "myapp", &[], None)
            .expect("build bom");

        assert_eq!(bom["bomFormat"], "CycloneDX");
        assert_eq!(bom["specVersion"], "1.6");
        assert_eq!(bom["version"], 1);
        assert!(bom["serialNumber"]
            .as_str()
            .expect("serial number")
            .starts_with("urn:uuid:"));
        assert!(bom["metadata"].is_object());
        assert!(bom["components"].is_array());
        assert!(bom["compositions"].is_array());
        assert!(bom["dependencies"].is_array());
        assert!(bom["vulnerabilities"].is_array());
    }

    /// Milestone 112 (T016): `BuildInclusion::NotNeeded` emits the
    /// native `scope: "excluded"` UNCONDITIONALLY — the default
    /// config has `include_dev: false`, so this proves the emission
    /// is independent of the include-dev gate, and that the
    /// component is kept in `components[]` rather than dropped by
    /// scope filtering (clarification 2026-06-11).
    #[test]
    fn not_needed_build_inclusion_emits_excluded_scope_without_include_dev() {
        let config = CycloneDxConfig::default();
        assert!(!config.include_dev, "test premise: include_dev off");
        let builder = CycloneDxBuilder::new(config);
        let mut excluded = make_component("excluded-mod", "1.0.0");
        excluded.build_inclusion =
            Some(mikebom_common::resolution::BuildInclusion::NotNeeded);
        let kept = make_component("kept-mod", "2.0.0");
        let components = vec![excluded, kept];
        let integrity = clean_integrity();

        let bom = builder
            .build(&components, &[], &integrity, "myapp", &[], None)
            .expect("build bom");
        let comps = bom["components"].as_array().expect("components array");
        assert_eq!(comps.len(), 2, "NotNeeded component must never be dropped");
        let not_needed = comps
            .iter()
            .find(|c| c["name"] == "excluded-mod")
            .expect("NotNeeded component present in components[]");
        assert_eq!(not_needed["scope"], "excluded");
        let props = not_needed["properties"]
            .as_array()
            .expect("properties array");
        assert!(
            props.iter().any(|p| p["name"] == "mikebom:build-inclusion"
                && p["value"] == "not-needed"),
            "mikebom:build-inclusion: not-needed property must be emitted"
        );
        // The unaffected sibling has no scope field (default = required).
        let plain = comps
            .iter()
            .find(|c| c["name"] == "kept-mod")
            .expect("plain component present");
        assert!(plain.get("scope").is_none());
    }

    /// Shade-jar nested emission (CDX 1.6 component.components[]).
    /// When a child carries parent_purl == some top-level component's
    /// PURL, it's folded under that parent's `components` array and
    /// gets a composite `<child>#<parent>` bom-ref.
    #[test]
    fn nested_components_fold_under_parent_with_composite_bom_ref() {
        let builder = CycloneDxBuilder::new(CycloneDxConfig::default());
        let parent_purl_str = "pkg:cargo/fatjar@1.0.0";
        let parent = make_component("fatjar", "1.0.0");
        let mut child_a = make_component("guava", "31.1");
        child_a.parent_purl = Some(parent_purl_str.to_string());
        let mut child_b = make_component("commons-lang3", "3.14");
        child_b.parent_purl = Some(parent_purl_str.to_string());
        let components = vec![parent, child_a, child_b];
        let integrity = clean_integrity();

        let bom = builder
            .build(&components, &[], &integrity, "myapp", &[], None)
            .expect("build bom");
        let top = bom["components"].as_array().expect("top-level array");
        // 1 top-level component (the fat-jar), 2 nested under it.
        assert_eq!(top.len(), 1, "children should not appear at top level");
        assert_eq!(top[0]["name"], "fatjar");
        let nested = top[0]["components"].as_array().expect("nested array");
        assert_eq!(nested.len(), 2);
        let names: Vec<&str> = nested
            .iter()
            .map(|c| c["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"guava"));
        assert!(names.contains(&"commons-lang3"));
        // Composite bom-refs on children.
        for c in nested {
            let bom_ref = c["bom-ref"].as_str().unwrap();
            assert!(
                bom_ref.contains('#'),
                "child bom-ref should be composite <child>#<parent>, got {bom_ref}"
            );
            assert!(bom_ref.ends_with(parent_purl_str));
        }
        // Parent's bom-ref stays as the plain PURL (no composite).
        assert_eq!(top[0]["bom-ref"], parent_purl_str);
    }

    /// Orphan children (parent_purl pointing at a PURL absent from the
    /// component set) get demoted to top-level with a plain bom-ref
    /// rather than disappearing from the SBOM.
    #[test]
    fn orphan_children_degrade_to_top_level() {
        let builder = CycloneDxBuilder::new(CycloneDxConfig::default());
        let mut orphan = make_component("orphan", "1.0.0");
        orphan.parent_purl = Some("pkg:cargo/non-existent-parent@9.9.9".to_string());
        let components = vec![orphan];
        let integrity = clean_integrity();

        let bom = builder
            .build(&components, &[], &integrity, "myapp", &[], None)
            .expect("build bom");
        let top = bom["components"].as_array().expect("array");
        assert_eq!(top.len(), 1);
        assert_eq!(top[0]["name"], "orphan");
        // Plain bom-ref, not composite — the orphan was demoted.
        let bom_ref = top[0]["bom-ref"].as_str().unwrap();
        assert!(!bom_ref.contains('#'));
    }

    /// Same child coord under two different parents surfaces as two
    /// distinct nested entries (CDX intended shape for fat-jars that
    /// each vendor the same library).
    #[test]
    fn same_coord_nested_under_two_parents_emits_twice() {
        let builder = CycloneDxBuilder::new(CycloneDxConfig::default());
        let parent_a = make_component("parent-a", "1.0.0");
        let parent_b = make_component("parent-b", "2.0.0");
        let mut child_under_a = make_component("shared-lib", "1.0.0");
        child_under_a.parent_purl = Some(parent_a.purl.as_str().to_string());
        let mut child_under_b = make_component("shared-lib", "1.0.0");
        child_under_b.parent_purl = Some(parent_b.purl.as_str().to_string());
        let components = vec![parent_a, parent_b, child_under_a, child_under_b];
        let integrity = clean_integrity();

        let bom = builder
            .build(&components, &[], &integrity, "myapp", &[], None)
            .expect("build bom");
        let top = bom["components"].as_array().expect("array");
        assert_eq!(top.len(), 2, "both parents at top level");
        // Each parent carries one shared-lib child.
        for parent in top {
            let nested = parent["components"].as_array().expect("nested");
            assert_eq!(nested.len(), 1);
            assert_eq!(nested[0]["name"], "shared-lib");
        }
        // All bom-refs document-wide must be unique (CDX invariant).
        let mut all_refs: Vec<&str> = Vec::new();
        for parent in top {
            all_refs.push(parent["bom-ref"].as_str().unwrap());
            if let Some(nested) = parent["components"].as_array() {
                for c in nested {
                    all_refs.push(c["bom-ref"].as_str().unwrap());
                }
            }
        }
        let unique: std::collections::HashSet<&str> = all_refs.iter().copied().collect();
        assert_eq!(unique.len(), all_refs.len(), "bom-refs not unique: {all_refs:?}");
    }

    #[test]
    fn components_include_purl_and_evidence() {
        let builder = CycloneDxBuilder::new(CycloneDxConfig::default());
        let components = vec![make_component("serde", "1.0.197")];
        let integrity = clean_integrity();

        let bom = builder
            .build(&components, &[], &integrity, "myapp", &[], None)
            .expect("build bom");

        let cdx_components = bom["components"].as_array().expect("components array");
        assert_eq!(cdx_components.len(), 1);

        let comp = &cdx_components[0];
        assert_eq!(comp["name"], "serde");
        assert_eq!(comp["version"], "1.0.197");
        assert_eq!(comp["type"], "library");
        assert!(comp["purl"].as_str().expect("purl").contains("serde"));
        assert!(comp["evidence"].is_object());
    }

    #[test]
    fn no_hashes_config_omits_hashes() {
        let config = CycloneDxConfig {
            include_hashes: false,
            include_source_files: false,
            generation_context: GenerationContext::BuildTimeTrace,
            include_dev: false,
        };
        let builder = CycloneDxBuilder::new(config);

        let mut component = make_component("serde", "1.0.197");
        // Even with hashes on the component, they should be omitted.
        component.hashes = vec![
            mikebom_common::types::hash::ContentHash::sha256(
                "3fb1c873e1b9b056a4dc4c0c198b24c3ffa59243c322bfd971d2d5ef4f463ee1",
            )
            .expect("valid hash"),
        ];

        let integrity = clean_integrity();
        let bom = builder
            .build(&[component], &[], &integrity, "myapp", &[], None)
            .expect("build bom");

        let cdx_components = bom["components"].as_array().expect("components array");
        assert!(cdx_components[0].get("hashes").is_none());
    }

    #[test]
    fn metadata_references_target() {
        let builder = CycloneDxBuilder::new(CycloneDxConfig::default());
        let integrity = clean_integrity();

        let bom = builder
            .build(&[], &[], &integrity, "myapp", &[], None)
            .expect("build bom");

        assert_eq!(bom["metadata"]["component"]["name"], "myapp");
    }

    #[test]
    fn cpes_emit_primary_plus_candidate_property() {
        let builder = CycloneDxBuilder::new(CycloneDxConfig::default());
        let mut component = make_component("jq", "1.6-2.1");
        component.cpes = vec![
            "cpe:2.3:a:debian:jq:1.6-2.1:*:*:*:*:*:*:*".to_string(),
            "cpe:2.3:a:jq:jq:1.6-2.1:*:*:*:*:*:*:*".to_string(),
        ];
        let integrity = clean_integrity();

        let bom = builder
            .build(&[component], &[], &integrity, "myapp", &[], None)
            .expect("build bom");

        let cdx = bom["components"].as_array().expect("components");
        assert_eq!(cdx.len(), 1);
        assert_eq!(
            cdx[0]["cpe"].as_str().expect("cpe field"),
            "cpe:2.3:a:debian:jq:1.6-2.1:*:*:*:*:*:*:*"
        );
        let props = cdx[0]["properties"]
            .as_array()
            .expect("properties array");
        assert!(
            props.iter().any(|p| p["name"] == "mikebom:cpe-candidates"
                && p["value"].as_str().unwrap().contains("jq:jq")),
            "expected cpe-candidates property, got {props:?}"
        );
    }

    #[test]
    fn single_cpe_omits_candidates_property() {
        let builder = CycloneDxBuilder::new(CycloneDxConfig::default());
        let mut component = make_component("serde", "1.0.197");
        component.cpes = vec!["cpe:2.3:a:serde:serde:1.0.197:*:*:*:*:*:*:*".to_string()];
        let integrity = clean_integrity();

        let bom = builder
            .build(&[component], &[], &integrity, "myapp", &[], None)
            .expect("build bom");

        let cdx = bom["components"].as_array().expect("components");
        assert_eq!(cdx[0]["cpe"], "cpe:2.3:a:serde:serde:1.0.197:*:*:*:*:*:*:*");
        // Only one candidate — no candidates property needed.
        let props = cdx[0].get("properties");
        if let Some(props) = props {
            assert!(
                !props
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|p| p["name"] == "mikebom:cpe-candidates"),
                "unexpected cpe-candidates property with single CPE"
            );
        }
    }

    #[test]
    fn buildinfo_status_missing_surfaces_property() {
        let builder = CycloneDxBuilder::new(CycloneDxConfig::default());
        let mut component = make_component("stripped-hello", "unknown");
        component.buildinfo_status = Some("missing".to_string());
        let integrity = clean_integrity();
        let bom = builder
            .build(&[component], &[], &integrity, "myapp", &[], None)
            .expect("build bom");
        let cdx = bom["components"].as_array().expect("components");
        let props = cdx[0]["properties"].as_array().expect("properties");
        let found = props
            .iter()
            .find(|p| p["name"] == "mikebom:buildinfo-status")
            .expect("mikebom:buildinfo-status property must be present");
        assert_eq!(found["value"], "missing");
    }

    #[test]
    fn buildinfo_status_unsupported_surfaces_property() {
        let builder = CycloneDxBuilder::new(CycloneDxConfig::default());
        let mut component = make_component("pre118-hello", "unknown");
        component.buildinfo_status = Some("unsupported".to_string());
        let integrity = clean_integrity();
        let bom = builder
            .build(&[component], &[], &integrity, "myapp", &[], None)
            .expect("build bom");
        let cdx = bom["components"].as_array().expect("components");
        let props = cdx[0]["properties"].as_array().expect("properties");
        let found = props
            .iter()
            .find(|p| p["name"] == "mikebom:buildinfo-status")
            .expect("mikebom:buildinfo-status property must be present");
        assert_eq!(found["value"], "unsupported");
    }

    #[test]
    fn buildinfo_status_none_does_not_surface_property() {
        let builder = CycloneDxBuilder::new(CycloneDxConfig::default());
        let component = make_component("serde", "1.0.197");
        // buildinfo_status is None by default on non-Go components.
        let integrity = clean_integrity();
        let bom = builder
            .build(&[component], &[], &integrity, "myapp", &[], None)
            .expect("build bom");
        let cdx = bom["components"].as_array().expect("components");
        let props = cdx[0].get("properties");
        if let Some(props) = props {
            assert!(
                !props
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|p| p["name"] == "mikebom:buildinfo-status"),
                "non-Go component must not surface mikebom:buildinfo-status"
            );
        }
    }

    // --- CDX 1.6 evidence serialization (sbomqs parse-failure fix) -----

    #[test]
    fn evidence_connection_ids_land_in_component_properties() {
        let builder = CycloneDxBuilder::new(CycloneDxConfig::default());
        let mut component = make_component("serde", "1.0.197");
        component.evidence.source_connection_ids =
            vec!["conn-1".to_string(), "conn-2".to_string()];
        let integrity = clean_integrity();

        let bom = builder
            .build(&[component], &[], &integrity, "myapp", &[], None)
            .expect("build bom");

        let comp = &bom["components"].as_array().expect("components")[0];
        let props = comp["properties"]
            .as_array()
            .expect("component must have properties");
        let conn_prop = props
            .iter()
            .find(|p| p["name"] == "mikebom:source-connection-ids")
            .expect("source-connection-ids property must be present");
        assert_eq!(conn_prop["value"], "conn-1,conn-2");
    }

    #[test]
    fn evidence_tools_field_absent_from_serialized_output() {
        // Regression guard for sbomqs parse failure:
        // `cannot unmarshal object into Go struct field
        //  Component.components.evidence.tools of type cyclonedx.BOMReference`.
        // Build a component with every flavor of provenance populated
        // (connection IDs, deps.dev match) and confirm nothing surfaces
        // under `evidence.identity[].tools`.
        let builder = CycloneDxBuilder::new(CycloneDxConfig::default());
        let mut component = make_component("express", "4.19.2");
        component.evidence.source_connection_ids = vec!["conn-42".to_string()];
        component.evidence.deps_dev_match = Some(
            mikebom_common::resolution::DepsDevMatch {
                system: "npm".to_string(),
                name: "express".to_string(),
                version: "4.19.2".to_string(),
            },
        );
        let integrity = clean_integrity();

        let bom = builder
            .build(&[component], &[], &integrity, "myapp", &[], None)
            .expect("build bom");

        let comp = &bom["components"].as_array().expect("components")[0];
        let identity = comp["evidence"]["identity"]
            .as_array()
            .expect("evidence.identity must be an array (CDX 1.6)");
        assert_eq!(identity.len(), 1);
        assert!(
            identity[0].get("tools").is_none(),
            "evidence.identity[].tools must not be emitted; got {:?}",
            identity[0].get("tools")
        );
    }

    #[test]
    fn deps_dev_match_lands_in_component_properties() {
        let builder = CycloneDxBuilder::new(CycloneDxConfig::default());
        let mut component = make_component("express", "4.19.2");
        component.evidence.deps_dev_match = Some(
            mikebom_common::resolution::DepsDevMatch {
                system: "npm".to_string(),
                name: "express".to_string(),
                version: "4.19.2".to_string(),
            },
        );
        let integrity = clean_integrity();

        let bom = builder
            .build(&[component], &[], &integrity, "myapp", &[], None)
            .expect("build bom");

        let comp = &bom["components"].as_array().expect("components")[0];
        let props = comp["properties"]
            .as_array()
            .expect("component must have properties");
        let dd_prop = props
            .iter()
            .find(|p| p["name"] == "mikebom:deps-dev-match")
            .expect("deps-dev-match property must be present");
        assert_eq!(dd_prop["value"], "npm:express@4.19.2");
    }

    // --- License shape (sbomqs score lift Fix 1) -----------------------

    #[test]
    fn component_with_single_spdx_license_emits_id_form_with_acknowledgement() {
        let builder = CycloneDxBuilder::new(CycloneDxConfig::default());
        let mut component = make_component("serde", "1.0.197");
        component.licenses = vec![
            mikebom_common::types::license::SpdxExpression::new("MIT").unwrap(),
        ];
        let integrity = clean_integrity();

        let bom = builder
            .build(&[component], &[], &integrity, "myapp", &[], None)
            .expect("build bom");

        let comp = &bom["components"].as_array().expect("components")[0];
        let licenses = comp["licenses"].as_array().unwrap();
        assert_eq!(licenses.len(), 1);
        assert_eq!(licenses[0]["license"]["id"], "MIT");
        assert_eq!(licenses[0]["license"]["acknowledgement"], "declared");
    }

    #[test]
    fn compound_or_license_splits_into_individual_ids() {
        // CDX 1.6 allows only ONE `{expression}` entry in a
        // `licenses[]` array and sbomqs scores `license.id`/`name`
        // only. `A OR B` becomes two separate `{license: {id}}`
        // entries — the disjunction is preserved structurally.
        let builder = CycloneDxBuilder::new(CycloneDxConfig::default());
        let mut component = make_component("anyhow", "1.0.80");
        component.licenses = vec![
            mikebom_common::types::license::SpdxExpression::new(
                "Apache-2.0 OR MIT",
            )
            .unwrap(),
        ];
        let integrity = clean_integrity();
        let bom = builder
            .build(&[component], &[], &integrity, "myapp", &[], None)
            .expect("build bom");
        let comp = &bom["components"].as_array().expect("components")[0];
        let licenses = comp["licenses"].as_array().unwrap();
        assert_eq!(licenses.len(), 2);
        assert_eq!(licenses[0]["license"]["id"], "Apache-2.0");
        assert_eq!(licenses[0]["license"]["acknowledgement"], "declared");
        assert_eq!(licenses[1]["license"]["id"], "MIT");
        assert_eq!(licenses[1]["license"]["acknowledgement"], "declared");
    }

    #[test]
    fn compound_and_license_splits_into_individual_ids() {
        // AND splits cleanly: "both licenses apply" maps to listing
        // both as `{license: {id}}` entries (multiple listed licenses
        // = all apply, per CDX 1.6 `licenses` array semantics). This
        // is strictly more semantically faithful than an expression
        // for the AND case.
        let builder = CycloneDxBuilder::new(CycloneDxConfig::default());
        let mut component = make_component("flask", "3.0.3");
        component.concluded_licenses = vec![
            mikebom_common::types::license::SpdxExpression::new(
                "BSD-2-Clause AND BSD-3-Clause",
            )
            .unwrap(),
        ];
        let integrity = clean_integrity();
        let bom = builder
            .build(&[component], &[], &integrity, "myapp", &[], None)
            .expect("build bom");
        let comp = &bom["components"].as_array().expect("components")[0];
        let licenses = comp["licenses"].as_array().unwrap();
        assert_eq!(licenses.len(), 2);
        assert_eq!(licenses[0]["license"]["id"], "BSD-2-Clause");
        assert_eq!(licenses[0]["license"]["acknowledgement"], "concluded");
        assert_eq!(licenses[1]["license"]["id"], "BSD-3-Clause");
    }

    #[test]
    fn compound_with_expression_falls_back_to_single_expression() {
        // `X WITH exception` can't be split — the WITH operator is
        // a semantic modifier on a base license, not a disjunction
        // or conjunction of independent licenses. Stays as one
        // `{expression}` entry.
        let builder = CycloneDxBuilder::new(CycloneDxConfig::default());
        let mut component = make_component("openjdk", "21");
        component.concluded_licenses = vec![
            mikebom_common::types::license::SpdxExpression::new(
                "GPL-2.0-only WITH Classpath-exception-2.0",
            )
            .unwrap(),
        ];
        let integrity = clean_integrity();
        let bom = builder
            .build(&[component], &[], &integrity, "myapp", &[], None)
            .expect("build bom");
        let comp = &bom["components"].as_array().expect("components")[0];
        let licenses = comp["licenses"].as_array().unwrap();
        assert_eq!(licenses.len(), 1);
        assert_eq!(
            licenses[0]["expression"],
            "GPL-2.0-only WITH Classpath-exception-2.0",
        );
    }

    #[test]
    fn compound_and_with_license_ref_splits_using_name_field() {
        // ClearlyDefined returns shapes like
        // `BSD-3-Clause AND LicenseRef-scancode-google-patent-license-golang`
        // for `golang.org/x/sys`. CDX 1.6's `license.id` is SPDX-list
        // only, so the LicenseRef operand routes to `license.name`
        // instead. Both entries are schema-legal and sbomqs-countable.
        let builder = CycloneDxBuilder::new(CycloneDxConfig::default());
        let mut component = make_component("x-sys", "0.5.0");
        component.concluded_licenses = vec![
            mikebom_common::types::license::SpdxExpression::new(
                "BSD-3-Clause AND LicenseRef-scancode-google-patent-license-golang",
            )
            .unwrap(),
        ];
        let integrity = clean_integrity();
        let bom = builder
            .build(&[component], &[], &integrity, "myapp", &[], None)
            .expect("build bom");
        let comp = &bom["components"].as_array().expect("components")[0];
        let licenses = comp["licenses"].as_array().unwrap();
        assert_eq!(licenses.len(), 2);
        assert_eq!(licenses[0]["license"]["id"], "BSD-3-Clause");
        assert_eq!(
            licenses[1]["license"]["name"],
            "LicenseRef-scancode-google-patent-license-golang",
        );
        assert!(licenses[1]["license"].get("id").is_none());
    }

    #[test]
    fn bare_license_ref_emits_name_form() {
        let builder = CycloneDxBuilder::new(CycloneDxConfig::default());
        let mut component = make_component("proprietary", "1.0.0");
        component.licenses = vec![
            mikebom_common::types::license::SpdxExpression::new(
                "LicenseRef-internal-eula",
            )
            .unwrap(),
        ];
        let integrity = clean_integrity();
        let bom = builder
            .build(&[component], &[], &integrity, "myapp", &[], None)
            .expect("build bom");
        let comp = &bom["components"].as_array().expect("components")[0];
        let licenses = comp["licenses"].as_array().unwrap();
        assert_eq!(licenses.len(), 1);
        assert_eq!(licenses[0]["license"]["name"], "LicenseRef-internal-eula");
    }

    #[test]
    fn mixed_operators_fall_back_to_single_expression() {
        // `A AND B OR C` has ambiguous precedence without parens —
        // splitting would misrepresent either interpretation. Stays
        // as one `{expression}` entry.
        let builder = CycloneDxBuilder::new(CycloneDxConfig::default());
        let mut component = make_component("complex", "1.0.0");
        component.concluded_licenses = vec![
            mikebom_common::types::license::SpdxExpression::new(
                "Apache-2.0 AND MIT OR BSD-3-Clause",
            )
            .unwrap(),
        ];
        let integrity = clean_integrity();
        let bom = builder
            .build(&[component], &[], &integrity, "myapp", &[], None)
            .expect("build bom");
        let comp = &bom["components"].as_array().expect("components")[0];
        let licenses = comp["licenses"].as_array().unwrap();
        assert_eq!(licenses.len(), 1);
        assert!(licenses[0]["expression"].is_string());
    }

    #[test]
    fn component_license_unknown_identifier_falls_back_to_expression() {
        let builder = CycloneDxBuilder::new(CycloneDxConfig::default());
        let mut component = make_component("myapp", "0.1.0");
        component.licenses = vec![
            mikebom_common::types::license::SpdxExpression::new(
                "Custom-In-House-License",
            )
            .unwrap(),
        ];
        let integrity = clean_integrity();

        let bom = builder
            .build(&[component], &[], &integrity, "myapp", &[], None)
            .expect("build bom");

        let comp = &bom["components"].as_array().expect("components")[0];
        let licenses = comp["licenses"].as_array().unwrap();
        assert_eq!(licenses[0]["expression"], "Custom-In-House-License");
        assert_eq!(licenses[0]["acknowledgement"], "declared");
    }

    #[test]
    fn concluded_licenses_emit_with_acknowledgement_concluded() {
        // Simulates the ClearlyDefined enrichment having added a
        // concluded SPDX expression after the package's manifest
        // declared one.
        let builder = CycloneDxBuilder::new(CycloneDxConfig::default());
        let mut component = make_component("express", "4.18.2");
        component.licenses = vec![
            mikebom_common::types::license::SpdxExpression::new("MIT").unwrap(),
        ];
        component.concluded_licenses = vec![
            mikebom_common::types::license::SpdxExpression::new("MIT").unwrap(),
        ];
        let integrity = clean_integrity();

        let bom = builder
            .build(&[component], &[], &integrity, "myapp", &[], None)
            .expect("build bom");

        let comp = &bom["components"].as_array().expect("components")[0];
        let licenses = comp["licenses"].as_array().unwrap();
        assert_eq!(licenses.len(), 2);
        // First entry: declared MIT (from manifest).
        assert_eq!(licenses[0]["license"]["id"], "MIT");
        assert_eq!(licenses[0]["license"]["acknowledgement"], "declared");
        // Second entry: concluded MIT (from CD enrichment).
        assert_eq!(licenses[1]["license"]["id"], "MIT");
        assert_eq!(licenses[1]["license"]["acknowledgement"], "concluded");
    }

    #[test]
    fn concluded_licenses_can_differ_from_declared() {
        // CD's analysis may yield a different SPDX expression than the
        // package's own declared license — emit both side by side.
        let builder = CycloneDxBuilder::new(CycloneDxConfig::default());
        let mut component = make_component("foo", "1.0.0");
        component.licenses = vec![
            mikebom_common::types::license::SpdxExpression::new("MIT").unwrap(),
        ];
        component.concluded_licenses = vec![
            mikebom_common::types::license::SpdxExpression::new("Apache-2.0").unwrap(),
        ];
        let integrity = clean_integrity();

        let bom = builder
            .build(&[component], &[], &integrity, "myapp", &[], None)
            .expect("build bom");

        let comp = &bom["components"].as_array().expect("components")[0];
        let licenses = comp["licenses"].as_array().unwrap();
        assert_eq!(licenses.len(), 2);
        let mut seen = std::collections::HashSet::new();
        for l in licenses {
            seen.insert((
                l["license"]["id"].as_str().unwrap().to_string(),
                l["license"]["acknowledgement"].as_str().unwrap().to_string(),
            ));
        }
        assert!(seen.contains(&("MIT".to_string(), "declared".to_string())));
        assert!(seen.contains(&("Apache-2.0".to_string(), "concluded".to_string())));
    }

    // -------- Milestone 077 — root component override --------

    fn make_main_module_component(
        ecosystem: &str,
        name: &str,
        version: &str,
    ) -> ResolvedComponent {
        let mut c = make_component(name, version);
        let purl_str = format!("pkg:{ecosystem}/{name}@{version}");
        c.purl = Purl::new(&purl_str).expect("valid purl");
        c.extra_annotations.insert(
            "mikebom:component-role".to_string(),
            serde_json::Value::String("main-module".to_string()),
        );
        c
    }

    #[test]
    fn override_active_replaces_metadata_component_identity() {
        // FR-001 + FR-002 + FR-004 — name/version override drives all
        // derived fields verbatim through the percent-encode helper.
        let builder = CycloneDxBuilder::new(CycloneDxConfig::default())
            .with_root_override(crate::generate::RootComponentOverride {
                name: Some("widget-svc".to_string()),
                version: Some("1.2.3".to_string()),
            ..Default::default()
        });
        let components = vec![make_component("serde", "1.0.0")];
        let integrity = clean_integrity();
        let bom = builder
            .build(&components, &[], &integrity, "abc123-snapshot", &[], None)
            .expect("build bom");
        let comp = &bom["metadata"]["component"];
        assert_eq!(comp["name"], "widget-svc");
        assert_eq!(comp["version"], "1.2.3");
        assert_eq!(comp["bom-ref"], "widget-svc@1.2.3");
        assert_eq!(comp["purl"], "pkg:generic/widget-svc@1.2.3");
        assert_eq!(
            comp["cpe"],
            "cpe:2.3:a:mikebom:widget-svc:1.2.3:*:*:*:*:*:*:*"
        );
    }

    #[test]
    fn override_drops_manifest_main_module_from_components_array() {
        // SC-006 / FR-008 — manifest-derived main-module is filtered
        // OUT of components[] when override is active.
        let builder = CycloneDxBuilder::new(CycloneDxConfig::default())
            .with_root_override(crate::generate::RootComponentOverride {
                name: Some("widget-svc".to_string()),
                version: Some("1.2.3".to_string()),
            ..Default::default()
        });
        let components = vec![
            make_main_module_component("cargo", "foo-internal", "0.5.1"),
            make_component("serde", "1.0.0"),
        ];
        let integrity = clean_integrity();
        let bom = builder
            .build(&components, &[], &integrity, "myapp", &[], None)
            .expect("build bom");
        let cdx_components = bom["components"].as_array().expect("components[]");
        // Main-module is dropped; only `serde` remains.
        let purls: Vec<&str> = cdx_components
            .iter()
            .filter_map(|c| c["purl"].as_str())
            .collect();
        assert!(
            !purls.contains(&"pkg:cargo/foo-internal@0.5.1"),
            "main-module PURL must be dropped; got: {purls:?}"
        );
        assert!(
            purls.contains(&"pkg:cargo/serde@1.0.0"),
            "regular dep must be preserved; got: {purls:?}"
        );
    }

    #[test]
    fn no_override_preserves_main_module() {
        // FR-009 — no-flag case preserves manifest-derived main-module
        // (no regression).
        let builder = CycloneDxBuilder::new(CycloneDxConfig::default());
        let components = vec![
            make_main_module_component("cargo", "foo-internal", "0.5.1"),
            make_component("serde", "1.0.0"),
        ];
        let integrity = clean_integrity();
        let bom = builder
            .build(&components, &[], &integrity, "myapp", &[], None)
            .expect("build bom");
        // With one main-module + no override, it gets promoted to
        // metadata.component (CDX 053-style placement) so it's NOT in
        // components[]. The override-aware test above is about the
        // override path. Here we verify the auto-derivation: the
        // metadata.component MUST be the main-module (not a synthesized
        // override identity).
        assert_eq!(
            bom["metadata"]["component"]["name"], "foo-internal",
            "auto-derived metadata.component should be the main-module"
        );
        assert_eq!(
            bom["metadata"]["component"]["version"], "0.5.1"
        );
    }

    #[test]
    fn override_on_build_tier_with_identifiers_orthogonal() {
        // T011(b) US2 — synthetic build-tier scenario: override is
        // active, build-tier identifiers (`repo:` + `subject:`) are
        // also attached, and BOTH coexist independently in the emitted
        // SBOM. Verifies FR-011 (orthogonality with milestones 073/076).
        use mikebom::binding::identifiers::Identifier;
        let repo_id = Identifier::parse("repo:git@github.com:acme/widget-svc.git")
            .expect("parse repo");
        let subject_id = Identifier::parse(
            "subject:sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .expect("parse subject");
        let identifiers = vec![repo_id, subject_id];

        let builder = CycloneDxBuilder::new(CycloneDxConfig {
            include_hashes: true,
            include_source_files: false,
            generation_context: GenerationContext::BuildTimeTrace,
            include_dev: false,
        })
        .with_identifiers(identifiers)
        .with_root_override(crate::generate::RootComponentOverride {
            name: Some("widget-svc".to_string()),
            version: Some("1.2.3".to_string()),
        ..Default::default()
    });
        let components = vec![make_component("serde", "1.0.0")];
        let integrity = clean_integrity();
        let bom = builder
            .build(
                &components,
                &[],
                &integrity,
                "build-tier-target",
                &[],
                None,
            )
            .expect("build bom");

        // Override identity drives metadata.component.
        assert_eq!(bom["metadata"]["component"]["name"], "widget-svc");
        assert_eq!(bom["metadata"]["component"]["version"], "1.2.3");

        // Identifiers ride the orthogonal externalReferences[] slot —
        // unaffected by the override.
        let ext_refs = bom["metadata"]["component"]["externalReferences"]
            .as_array()
            .expect("externalReferences[]");
        let urls: Vec<&str> = ext_refs
            .iter()
            .filter_map(|r| r["url"].as_str())
            .collect();
        assert!(
            urls.contains(&"git@github.com:acme/widget-svc.git"),
            "repo: identifier preserved; got: {urls:?}"
        );
        assert!(
            urls.iter()
                .any(|u| u.starts_with("sha256:0123456789")),
            "subject: identifier preserved; got: {urls:?}"
        );
    }
}