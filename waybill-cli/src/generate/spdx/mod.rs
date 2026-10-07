//! SPDX output serializers (milestones 010 + 011).
//!
//! Two stable user-facing formats live here plus one deprecated
//! alias:
//!
//! * `spdx-2.3-json` — stable, covers all ecosystems supported by the
//!   CycloneDX path. See [`document`], [`packages`], [`relationships`].
//! * `spdx-3-json` — stable SPDX 3.0.1 JSON-LD emitter, full ecosystem
//!   coverage with waybill-specific signal fidelity vs. the SPDX 2.3
//!   path. See [`v3_document`], [`v3_packages`], [`v3_relationships`],
//!   [`v3_licenses`], [`v3_agents`], [`v3_external_ids`],
//!   [`v3_annotations`].
//! * `spdx-3-json-experimental` — deprecated alias (milestone 010
//!   legacy). Delegates verbatim to the stable `spdx-3-json`
//!   serializer; byte-identical output; removed in milestone 013 per
//!   research.md §R2. Prints a stderr deprecation notice at
//!   invocation time via the CLI layer in `cli/scan_cmd.rs`.
//!
//! Waybill-specific data without a native SPDX 2.3 / 3.0.1 home is
//! preserved losslessly via [`annotations`] (SPDX 2.3) and
//! [`v3_annotations`] (SPDX 3) using the same versioned JSON envelope
//! per `contracts/waybill-annotation.schema.json`.
//!
//! The data-placement map in `docs/reference/sbom-format-mapping.md`
//! is the authoritative cross-format contract these serializers honor.

pub mod annotations;
pub mod document;
mod document_name;
pub mod ids;
pub mod packages;
pub mod relationships;
pub mod v3_agents;
pub mod v3_annotations;
pub mod v3_document;
pub mod v3_external_ids;
pub mod v3_id_type_map;
pub mod v3_licenses;
pub mod v3_packages;
pub mod v3_relationships;

use std::path::PathBuf;

use anyhow::Context;

use super::{EmittedArtifact, OutputConfig, SbomSerializer, ScanArtifacts};

/// SPDX 2.3 JSON serializer (T026).
///
/// Produces a document under the default filename `waybill.spdx.json`.
/// Determinism is guaranteed by construction: the document's
/// `creationInfo.created` is taken from [`OutputConfig::created`] and
/// the `documentNamespace` is a SHA-256 hash of scan content; no
/// `Utc::now()` / `Uuid::new_v4()` inside the serialization path.
pub struct Spdx2_3JsonSerializer;

/// SPDX 3.0.1 stable serializer (milestone 011).
///
/// Full coverage across all 9 ecosystems waybill supports
/// (apk, cargo, deb, gem, go, maven, npm, pip, rpm); produces a
/// schema-valid SPDX 3.0.1 JSON-LD document with native-field
/// parity vs. the CycloneDX serializer (PURL, name, version,
/// license, hash, supplier/originator) plus waybill-specific
/// signal fidelity vs. the SPDX 2.3 serializer (every `waybill:*`
/// field reaches SPDX 3 either as a typed native property or as
/// an `Annotation` element under the Q2 strict-match rule).
/// `experimental()` returns `false` — this is a first-class
/// production-grade output format.
pub struct Spdx3JsonSerializer;

impl SbomSerializer for Spdx3JsonSerializer {
    fn id(&self) -> &'static str {
        "spdx-3-json"
    }

    fn default_filename(&self) -> &'static str {
        "waybill.spdx3.json"
    }

    fn experimental(&self) -> bool {
        false
    }

    fn serialize(
        &self,
        scan: &ScanArtifacts<'_>,
        cfg: &OutputConfig,
    ) -> anyhow::Result<Vec<EmittedArtifact>> {
        // Co-emit the OpenVEX sidecar when the scan produced at
        // least one advisory, mirroring the SPDX 2.3 path's
        // behavior (FR-013 — same shape, same default filename).
        // Build it first so the SPDX 3 document can cross-reference
        // it via an ExternalRef on the SpdxDocument element (FR-014
        // / clarification Q1).
        let openvex_artifact = crate::generate::openvex::serialize_openvex(scan, cfg)
            .context("building OpenVEX sidecar")?;
        // #1122: relative to this document, where the sidecar is written.
        let sidecar_locator: Option<String> = openvex_artifact.as_ref().map(|_| {
            crate::generate::openvex::sidecar_reference(
                &cfg.overrides,
                &["spdx-3-json", "spdx-3-json-experimental"],
                self.default_filename(),
            )
        });

        let doc = v3_document::build_document(scan, cfg, sidecar_locator.as_deref())?;
        let bytes = serde_json::to_string_pretty(&doc)
            .context("serializing SPDX 3.0.1 document to JSON")?
            .into_bytes();
        let mut out = vec![EmittedArtifact {
            relative_path: PathBuf::from(self.default_filename()),
            bytes,
        }];
        if let Some(artifact) = openvex_artifact {
            out.push(artifact);
        }
        Ok(out)
    }
}

/// SPDX 3.0.1 deprecation-track alias (milestone 010 stub →
/// milestone 011 alias).
///
/// Per spec FR-002 + research.md §R6: this identifier was the
/// milestone-010 experimental stub. Milestone 011 retains it as a
/// deprecation alias that delegates to [`Spdx3JsonSerializer::serialize`]
/// verbatim — byte-identical output, same `waybill.spdx3.json`
/// default filename, no comment-marker injection.
///
/// The deprecation signal is carried by the stderr notice (emitted
/// by the CLI dispatch layer in `cli/scan_cmd.rs`) plus the
/// help-text "[DEPRECATED]" annotation (via the
/// [`crate::generate::SbomSerializer`]-adjacent rendering in
/// `format_help_list`). `experimental()` returns `false` — the
/// alias output is production-grade (same bytes as the stable
/// emitter), it's the *identifier* that's on a deprecation path,
/// not the output quality.
///
/// Lifecycle: alias accepted through milestone 012; removed in
/// milestone 013 unless usage signals say otherwise (research.md
/// §R2).
pub struct Spdx3JsonExperimentalSerializer;

impl SbomSerializer for Spdx3JsonExperimentalSerializer {
    fn id(&self) -> &'static str {
        "spdx-3-json-experimental"
    }

    fn default_filename(&self) -> &'static str {
        // Deliberately the same as `Spdx3JsonSerializer` per
        // research.md §R6 — alias produces byte-identical output
        // including the on-disk filename when no `--output`
        // override is set.
        "waybill.spdx3.json"
    }

    fn experimental(&self) -> bool {
        // False — the alias's output is byte-identical to the
        // stable emitter (production-grade), so the constitution's
        // experimental-labeling clause doesn't apply. The
        // lifecycle signal (deprecation) is carried by the help
        // text + stderr notice, NOT by the `experimental` trait
        // flag.
        false
    }

    fn serialize(
        &self,
        scan: &ScanArtifacts<'_>,
        cfg: &OutputConfig,
    ) -> anyhow::Result<Vec<EmittedArtifact>> {
        // Delegate verbatim to the stable serializer — byte-for-byte
        // identity is the FR-002 + research.md §R6 contract.
        Spdx3JsonSerializer.serialize(scan, cfg)
    }
}

impl SbomSerializer for Spdx2_3JsonSerializer {
    fn id(&self) -> &'static str {
        "spdx-2.3-json"
    }

    fn default_filename(&self) -> &'static str {
        "waybill.spdx.json"
    }

    fn serialize(
        &self,
        scan: &ScanArtifacts<'_>,
        cfg: &OutputConfig,
    ) -> anyhow::Result<Vec<EmittedArtifact>> {
        let mut doc = document::build_document(scan, cfg);

        // T037 — co-emit the OpenVEX sidecar when the scan produces
        // advisories. The cross-reference in the SPDX document's
        // `externalDocumentRefs` has to name the sidecar's relative
        // path and the SHA-256 of its bytes, so we build the sidecar
        // FIRST and then inject the reference before serializing the
        // SPDX document. When there are no advisories the sidecar is
        // skipped entirely — no cross-reference, no file written —
        // per FR-016a.
        let openvex_artifact = crate::generate::openvex::serialize_openvex(scan, cfg)
            .context("building OpenVEX sidecar")?;
        if let Some(ref artifact) = openvex_artifact {
            let hex_sha256 = sha256_hex(&artifact.bytes);
            // The cross-reference path must name where the sidecar
            // actually lands on disk. When the user has set
            // `--output openvex=<path>`, the CLI layer will write
            // the sidecar there — cfg.overrides carries that path
            // through so the SPDX document and the filesystem
            // agree on one string.
            // #1122: relative to this document, where the sidecar is
            // written (beside it by default).
            let sidecar_path = crate::generate::openvex::sidecar_reference(
                &cfg.overrides,
                &[self.id()],
                self.default_filename(),
            );
            doc.external_document_refs.push(
                document::SpdxExternalDocumentRef {
                    id: "DocumentRef-OpenVEX".to_string(),
                    spdx_document: sidecar_path,
                    checksum: packages::SpdxChecksum {
                        algorithm: packages::SpdxChecksumAlgorithm::SHA256,
                        value: hex_sha256,
                    },
                },
            );
        }

        // Milestone 072 / T012 — when --bind-to-source was used,
        // emit the standards-native cross-document reference per
        // contracts/source-document-binding-annotation.md C-2 SPDX 2.3:
        //   * externalDocumentRefs[] entry naming the source SBOM
        //     by IRI + SHA-256 checksum.
        //   * DESCENDANT_OF relationship from the document root to a
        //     namespaced cross-doc SPDXID. We use the source-tier
        //     element form `DocumentRef-source-sbom:SPDXRef-DOCUMENT`
        //     since the SPDX 2.3 spec allows pointing at the
        //     document's root via the document SPDXID.
        if let Some(source_id) = scan.source_document_binding {
            let source_iri = source_id
                .iri
                .clone()
                .unwrap_or_else(|| format!("urn:sha256:{}", source_id.sha256));
            let ext_ref_id = "DocumentRef-source-sbom".to_string();
            doc.external_document_refs.push(
                document::SpdxExternalDocumentRef {
                    id: ext_ref_id.clone(),
                    spdx_document: source_iri,
                    checksum: packages::SpdxChecksum {
                        algorithm: packages::SpdxChecksumAlgorithm::SHA256,
                        value: source_id.sha256.clone(),
                    },
                },
            );
            // DESCENDANT_OF relationship: document root → cross-doc
            // SPDXRef-DOCUMENT. Per SPDX 2.3 §7.2, the cross-doc
            // SPDXID has the form `<DocumentRefId>:<SPDXID>`.
            doc.relationships.push(relationships::SpdxRelationship {
                source: doc.spdx_id.clone(),
                target: ids::SpdxId::cross_document_ref(&ext_ref_id, "SPDXRef-DOCUMENT"),
                kind: relationships::SpdxRelationshipType::DescendantOf,
                comment: Some(
                    "milestone-072 cross-tier binding: this build/deployment was \
                     produced from the source-tier SBOM referenced above"
                        .to_string(),
                ),
            });
        }

        // Last thing before serialization, so every producer above is
        // covered — build_relationships, the document.rs file/view edges and
        // the m072 DESCENDANT_OF edge alike.
        relationships::sort_relationships(&mut doc.relationships);

        let json_str = serde_json::to_string_pretty(&doc)
            .context("serializing SPDX 2.3 document to JSON")?;
        let mut out = vec![EmittedArtifact {
            relative_path: PathBuf::from(self.default_filename()),
            bytes: json_str.into_bytes(),
        }];
        if let Some(artifact) = openvex_artifact {
            out.push(artifact);
        }
        Ok(out)
    }
}

/// Lower-case hex SHA-256 of the given bytes. Used for the
/// `externalDocumentRefs.checksum.checksumValue` field per SPDX
/// 2.3 §6.6 (the value MUST match the linked document's bytes).
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    //! Tests for the SPDX ↔ OpenVEX sidecar co-emit path (T030/T037).
    //! waybill's scan pipeline doesn't populate `AdvisoryRef` anywhere
    //! today, so the only way to exercise the emit-with-VEX branch is
    //! to hand-build a `ScanArtifacts` with synthetic advisories. When
    //! the scanner grows a VEX-enrichment path later, these tests keep
    //! guarding the same contract via direct serializer calls.
    use super::*;
    use waybill_common::attestation::integrity::TraceIntegrity;
    use waybill_common::attestation::metadata::GenerationContext;
    use waybill_common::resolution::{
        AdvisoryRef, ResolutionEvidence, ResolutionTechnique, ResolvedComponent,
    };
    use waybill_common::types::purl::Purl;

    fn mk_component(purl: &str, advisories: Vec<AdvisoryRef>) -> ResolvedComponent {
        ResolvedComponent {
            build_inclusion: None,
            purl: Purl::new(purl).unwrap(),
            name: "x".to_string(),
            version: "1".to_string(),
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
            advisories,
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

    fn empty_integrity() -> TraceIntegrity {
        TraceIntegrity {
            ring_buffer_overflows: 0,
            events_dropped: 0,
            uprobe_attach_failures: vec![],
            kprobe_attach_failures: vec![],
            partial_captures: vec![],
            bloom_filter_capacity: 0,
            bloom_filter_false_positive_rate: 0.0,
            filter_categories_applied: vec![],
            unresolved_relative_opens: 0,
        }
    }

    fn mk_cfg() -> OutputConfig {
        OutputConfig {
            mikebom_version: "0.0.0-test",
            created: chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            overrides: std::collections::BTreeMap::new(),
        }
    }

    fn mk_artifacts<'a>(
        comps: &'a [ResolvedComponent],
        integ: &'a TraceIntegrity,
    ) -> ScanArtifacts<'a> {
        ScanArtifacts {
            target_name: "demo",
            components: comps,
            relationships: &[],
            integrity: integ,
            complete_ecosystems: &[],
            os_release_missing_fields: &[],
            go_transitive_coverage: None,
            go_transitive_fallback_count: None,
            unresolved_declared_dep_count: 0,
            go_cache_warming: None,
            go_mod_why: None,
            go_workspace_mode: None,
            go_toolchains_detected: None,
            cross_ecosystem_edges_report: None,
            nix_closure_summary: None,
            nixpkgs_security_summary: None,
            helm_extraction_mode: None,
            pants_resolve_summary: None,
            resolve_identity: None,
            haskell_parse_summary: None,
            gradle_scan_summary: None,
            no_binary_scan_mode: None,
            image_source: None,
            scan_target_coord: None,
            generation_context: GenerationContext::FilesystemScan,
            include_dev: false,
            include_hashes: true,
            include_source_files: false,
            scan_roots: Vec::new(),
            // Milestone 221 US4 — test-helper default preserves
            // pre-m221 behavior (no --sbom-version).
            sbom_version: None,
            enrichment_degraded: None,
            scope_mode: crate::generate::ScopeMode::Artifact,
            source_document_binding: None,
            identifiers: &[],
            component_identifiers: &[],
            file_inventory_stats: None,
            nixpkgs_haskell_degraded: None,
            nix_eval_tier: None,
            nix_eval_degraded: None,
            nix_closure_degraded: None,
            deps_dev_online: false,
            nix_eval_system: None,
            nixpkgs_haskell_resolution: None,
            nixpkgs_haskell_closure: None,
            file_inventory_mode: None,
            file_inventory_source_shapes: None,
            root_override: crate::generate::RootComponentOverride::default(),
            preserve_manifest_main_module: false,
            user_metadata: waybill::binding::user_metadata::UserMetadata::default(),
            sbom_type_override: None,
            spdx2_relationship_compat: crate::generate::Spdx2RelationshipCompat::Full,
            collisions_summary: None,
            compiler_pipeline: None,
            project_discovery_mode: None,
        }
    }

    fn parse_spdx(bytes: &[u8]) -> serde_json::Value {
        serde_json::from_slice(bytes).expect("SPDX bytes are valid JSON")
    }

    #[test]
    fn spdx_no_vex_emits_no_sidecar_and_no_external_doc_refs() {
        let integ = empty_integrity();
        let comps = [mk_component("pkg:cargo/a@1", vec![])];
        let arts = mk_artifacts(&comps, &integ);
        let artifacts =
            Spdx2_3JsonSerializer.serialize(&arts, &mk_cfg()).unwrap();
        assert_eq!(
            artifacts.len(),
            1,
            "no advisories → SPDX only, no sidecar artifact"
        );
        let spdx = parse_spdx(&artifacts[0].bytes);
        // externalDocumentRefs is `skip_serializing_if = "Vec::is_empty"`, so
        // its absence is the expected shape when there are no cross-refs.
        assert!(
            spdx.get("externalDocumentRefs").is_none(),
            "no advisories → no externalDocumentRefs entry"
        );
    }

    #[test]
    fn spdx_with_vex_emits_sidecar_and_cross_reference() {
        let integ = empty_integrity();
        let comps = [mk_component(
            "pkg:cargo/a@1",
            vec![AdvisoryRef {
                id: "CVE-2026-0001".to_string(),
                source: "osv".to_string(),
                url: None,
            }],
        )];
        let arts = mk_artifacts(&comps, &integ);
        let artifacts =
            Spdx2_3JsonSerializer.serialize(&arts, &mk_cfg()).unwrap();
        assert_eq!(
            artifacts.len(),
            2,
            "advisory present → SPDX artifact + OpenVEX sidecar"
        );
        let (spdx_art, vex_art) = match artifacts[0].relative_path.to_string_lossy().as_ref() {
            "waybill.spdx.json" => (&artifacts[0], &artifacts[1]),
            _ => (&artifacts[1], &artifacts[0]),
        };
        assert_eq!(
            spdx_art.relative_path,
            std::path::PathBuf::from("waybill.spdx.json")
        );
        assert_eq!(
            vex_art.relative_path,
            std::path::PathBuf::from("waybill.openvex.json")
        );

        let spdx = parse_spdx(&spdx_art.bytes);
        let refs = spdx["externalDocumentRefs"]
            .as_array()
            .expect("externalDocumentRefs present");
        assert_eq!(refs.len(), 1);
        let r = &refs[0];
        assert_eq!(r["externalDocumentId"], "DocumentRef-OpenVEX");
        assert_eq!(r["spdxDocument"], "waybill.openvex.json");
        assert_eq!(r["checksum"]["algorithm"], "SHA256");
        // The checksum MUST match the sidecar bytes — if this drifts
        // a consumer would integrity-check and reject the sidecar.
        assert_eq!(
            r["checksum"]["checksumValue"],
            sha256_hex(&vex_art.bytes)
        );
    }

    // ---- SPDX 3 ↔ OpenVEX cross-ref (milestone 011 T019) ---------

    fn parse_spdx3(bytes: &[u8]) -> serde_json::Value {
        serde_json::from_slice(bytes).expect("SPDX 3 bytes are valid JSON")
    }

    /// Find the single SpdxDocument element in an emitted SPDX 3
    /// document. Panics if absent — every document has one.
    fn find_spdx3_document(doc: &serde_json::Value) -> &serde_json::Value {
        doc["@graph"]
            .as_array()
            .expect("@graph array")
            .iter()
            .find(|e| e["type"] == "SpdxDocument")
            .expect("SpdxDocument element")
    }

    /// #993 (C23): SPDX 3 twin of the CDX and SPDX 2.3 tests. All
    /// three emitters must produce the SAME JSON-array-in-string for
    /// the attach-failures subkeys so `holistic_parity` can hold C23
    /// `SymmetricEqual`. Locks the NON-EMPTY case — the parity
    /// goldens are scan-mode and only ever carry empty lists, so
    /// without this the information-losing shape could return on the
    /// SPDX 3 path unnoticed.
    /// Every value of the annotation `field`, in any of the three formats:
    /// CycloneDX `properties[]` entries, and the SPDX 2.3 / SPDX 3
    /// `MikebomAnnotationCommentV1` envelopes in `comment` / `statement`.
    fn annotation_values(doc: &serde_json::Value, field: &str) -> Vec<String> {
        fn walk(v: &serde_json::Value, field: &str, out: &mut Vec<String>) {
            match v {
                serde_json::Value::Object(m) => {
                    if m.get("name").and_then(|n| n.as_str()) == Some(field) {
                        if let Some(s) = m.get("value").and_then(|v| v.as_str()) {
                            out.push(s.to_string());
                        }
                    }
                    for key in ["comment", "statement"] {
                        let env = m
                            .get(key)
                            .and_then(|c| c.as_str())
                            .and_then(|c| serde_json::from_str::<serde_json::Value>(c).ok());
                        if let Some(env) = env {
                            if env.get("field").and_then(|f| f.as_str()) == Some(field) {
                                if let Some(s) = env.get("value").and_then(|v| v.as_str()) {
                                    out.push(s.to_string());
                                }
                            }
                        }
                    }
                    m.values().for_each(|c| walk(c, field, out));
                }
                serde_json::Value::Array(a) => a.iter().for_each(|c| walk(c, field, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        walk(doc, field, &mut out);
        out.sort();
        out
    }

    /// Milestone 1069 (#878, T005, T009) — SPDX 3 dependency completeness
    /// agrees with CycloneDX `compositions[]` for every component, with and
    /// without a root override, and the waybill completeness annotations are
    /// unchanged.
    #[test]
    fn spdx3_completeness_agrees_with_cyclonedx_compositions() {
        use std::collections::{BTreeMap, BTreeSet};
        use waybill_common::resolution::{EnrichmentProvenance, Relationship, RelationshipType};
        let integ = empty_integrity();
        let mut app = mk_component("pkg:cargo/app@1.0.0", vec![]);
        app.extra_annotations.insert(
            "waybill:component-role".to_string(),
            serde_json::Value::String("main-module".to_string()),
        );
        let comps = [
            app,
            mk_component("pkg:cargo/a@1.0.0", vec![]), // complete
            mk_component("pkg:npm/x@1.0.0", vec![]),   // unknown: nothing reaches it
            mk_component("pkg:npm/leaf@1.0.0", vec![]), // unknown, and a leaf
            mk_component("pkg:pypi/p@1.0.0", vec![]),  // unclaimed: pypi not enumerated
        ];
        let edge = |f: &str, t: &str| Relationship {
            from: f.to_string(),
            to: t.to_string(),
            relationship_type: RelationshipType::DependsOn,
            provenance: EnrichmentProvenance {
                source: "test".to_string(),
                data_type: "relationship".to_string(),
            },
        };
        let rels = [
            edge("pkg:cargo/app@1.0.0", "pkg:cargo/a@1.0.0"),
            edge("pkg:cargo/app@1.0.0", "pkg:pypi/p@1.0.0"),
            edge("pkg:npm/x@1.0.0", "pkg:npm/leaf@1.0.0"),
        ];
        let ecosystems = ["cargo".to_string(), "npm".to_string()];

        for override_root in [false, true] {
            let mut arts = mk_artifacts(&comps, &integ);
            arts.relationships = &rels;
            arts.complete_ecosystems = &ecosystems;
            if override_root {
                arts.root_override = crate::generate::RootComponentOverride {
                    name: Some("renamed".to_string()),
                    version: Some("2.0".to_string()),
                    ..Default::default()
                };
            }
            let cfg = mk_cfg();
            let cdx: serde_json::Value = serde_json::from_slice(
                &crate::generate::cyclonedx::CycloneDxJsonSerializer.serialize(&arts, &cfg).unwrap()[0].bytes,
            )
            .unwrap();
            let spdx3: serde_json::Value = serde_json::from_slice(
                &Spdx3JsonSerializer.serialize(&arts, &cfg).unwrap()[0].bytes,
            )
            .unwrap();

            let cdx_root = cdx["metadata"]["component"]["bom-ref"].as_str().unwrap().to_string();
            let claim = |agg: &str| -> BTreeSet<String> {
                cdx["compositions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|c| c["aggregate"] == agg)
                    .flat_map(|c| c["dependencies"].as_array().cloned().unwrap_or_default())
                    .filter_map(|d| d.as_str().map(str::to_string))
                    .collect()
            };
            let (cdx_complete, cdx_unknown) = (claim("complete"), claim("unknown"));

            let graph = spdx3["@graph"].as_array().unwrap();
            let purl_of: BTreeMap<&str, &str> = graph
                .iter()
                .filter_map(|e| Some((e["spdxId"].as_str()?, e["software_packageUrl"].as_str()?)))
                .collect();
            let root_iri = graph
                .iter()
                .find(|e| e["type"] == "SpdxDocument")
                .and_then(|d| d["rootElement"][0].as_str())
                .unwrap();
            let mut spdx = BTreeMap::<String, BTreeSet<String>>::new();
            for e in graph.iter().filter(|e| e["relationshipType"] == "dependsOn") {
                let from = e["from"].as_str().unwrap();
                let who = if from == root_iri {
                    cdx_root.clone()
                } else {
                    purl_of[from].to_string()
                };
                let c = e["completeness"].as_str().unwrap_or("-").to_string();
                spdx.entry(who).or_default().insert(c);
            }
            let ctx = format!("override_root={override_root}");

            // Every CycloneDX `unknown` component is incomplete / noAssertion.
            for u in &cdx_unknown {
                let got = spdx.get(u).cloned().unwrap_or_default();
                assert!(
                    !got.is_empty() && got.iter().all(|c| c == "incomplete" || c == "noAssertion"),
                    "{ctx}: {u} is CycloneDX-unknown but SPDX 3 says {got:?}",
                );
            }
            // Every SPDX 3 qualifier has its CycloneDX counterpart, and no other.
            for (who, cs) in &spdx {
                for c in cs {
                    match c.as_str() {
                        "complete" => assert!(cdx_complete.contains(who), "{ctx}: {who} complete only in SPDX 3"),
                        "incomplete" | "noAssertion" => {
                            assert!(cdx_unknown.contains(who), "{ctx}: {who} {c} only in SPDX 3")
                        }
                        _ => assert!(
                            !cdx_complete.contains(who) && !cdx_unknown.contains(who),
                            "{ctx}: {who} is claimed by CycloneDX but unqualified in SPDX 3",
                        ),
                    }
                }
            }
            assert!(cdx_unknown.contains("pkg:npm/leaf@1.0.0"), "{ctx}: fixture must exercise an unknown leaf");
            assert_eq!(
                spdx["pkg:npm/leaf@1.0.0"],
                BTreeSet::from(["noAssertion".to_string()]),
                "{ctx}: an unknown leaf gets NoAssertionElement",
            );
            assert!(!spdx.contains_key("pkg:pypi/p@1.0.0"), "{ctx}: an unclaimed leaf gains nothing");

            // T009: the waybill completeness annotations are the SPDX 2.3 values.
            let spdx23: serde_json::Value = serde_json::from_slice(
                &Spdx2_3JsonSerializer.serialize(&arts, &cfg).unwrap()[0].bytes,
            )
            .unwrap();
            // C45 orphan-reason, C104 graph-completeness, C105 its reason.
            for row in ["C45", "C104", "C105"] {
                let e = waybill::parity::extractors::EXTRACTORS
                    .iter()
                    .find(|e| e.row_id == row)
                    .unwrap();
                assert_eq!((e.spdx23)(&spdx23), (e.spdx3)(&spdx3), "{ctx}: {row} differs between SPDX 2.3 and SPDX 3");
            }
        }
    }

    /// Milestone 1067 (T009, T017) — C191 per component and C192 at document
    /// scope, identical in all three formats, and C192's counts equal the
    /// C191 tallies (SC-005).
    #[test]
    fn deps_dev_outcomes_are_emitted_in_all_three_formats() {
        use crate::enrich::deps_dev_outcome::{record, Outcome};
        let integ = empty_integrity();
        let with = |purl: &str, o: Option<Outcome>| {
            let mut c = mk_component(purl, vec![]);
            record(&mut c, o);
            c
        };
        let comps = [
            with("pkg:cargo/absent@1.0.0", Some(Outcome::Absent)),
            with("pkg:npm/declined@1.0.0", Some(Outcome::DeclinedInvalidLicense)),
            with("pkg:pypi/broken@1.0.0", Some(Outcome::TransportFailure)),
            with("pkg:cargo/matched@1.0.0", None),
            with("pkg:deb/debian/zlib@1.0.0", None),
        ];
        let expected =
            r#"{"absent":1,"declined-invalid-license":1,"not-queried:unsupported-ecosystem":1,"transport-failure":1}"#;
        let docs = |online: bool| -> Vec<serde_json::Value> {
            let mut arts = mk_artifacts(&comps, &integ);
            arts.deps_dev_online = online;
            let cfg = mk_cfg();
            [
                crate::generate::cyclonedx::CycloneDxJsonSerializer.serialize(&arts, &cfg),
                Spdx2_3JsonSerializer.serialize(&arts, &cfg),
                Spdx3JsonSerializer.serialize(&arts, &cfg),
            ]
            .into_iter()
            .map(|r| serde_json::from_slice(&r.unwrap()[0].bytes).unwrap())
            .collect()
        };

        // The parity rows read what waybill writes, in all three formats.
        let online = docs(true);
        for row in ["C191", "C192"] {
            let e = waybill::parity::extractors::EXTRACTORS
                .iter()
                .find(|e| e.row_id == row)
                .unwrap();
            let (a, b, c) = ((e.cdx)(&online[0]), (e.spdx23)(&online[1]), (e.spdx3)(&online[2]));
            assert!(!a.is_empty(), "{row}: CDX extractor found nothing");
            assert_eq!(a, b, "{row}: CDX vs SPDX 2.3");
            assert_eq!(a, c, "{row}: CDX vs SPDX 3");
        }
        for doc in online {
            let per_component = annotation_values(&doc, "waybill:deps-dev-outcome");
            assert_eq!(
                per_component,
                vec!["absent", "declined-invalid-license", "transport-failure"],
            );
            let counts = annotation_values(&doc, "waybill:deps-dev-outcomes");
            assert_eq!(counts, vec![expected.to_string()]);
            // SC-005: every C191 value's tally is its C192 count.
            let parsed: std::collections::BTreeMap<String, usize> =
                serde_json::from_str(&counts[0]).unwrap();
            for (k, n) in parsed.iter().filter(|(k, _)| *k != "not-queried:unsupported-ecosystem") {
                assert_eq!(per_component.iter().filter(|v| *v == k).count(), *n, "{k}");
            }
        }
        for doc in docs(false) {
            assert!(annotation_values(&doc, "waybill:deps-dev-outcomes").is_empty());
        }
    }

    /// Milestone 1067 (T017) — a fully matched online scan adds nothing.
    #[test]
    fn a_fully_matched_scan_has_no_deps_dev_outcomes() {
        let integ = empty_integrity();
        let comps = [mk_component("pkg:cargo/a@1.0.0", vec![])];
        let mut arts = mk_artifacts(&comps, &integ);
        arts.deps_dev_online = true;
        let cfg = mk_cfg();
        for r in [
            crate::generate::cyclonedx::CycloneDxJsonSerializer.serialize(&arts, &cfg),
            Spdx2_3JsonSerializer.serialize(&arts, &cfg),
            Spdx3JsonSerializer.serialize(&arts, &cfg),
        ] {
            let doc: serde_json::Value = serde_json::from_slice(&r.unwrap()[0].bytes).unwrap();
            assert!(annotation_values(&doc, "waybill:deps-dev-outcome").is_empty());
            assert!(annotation_values(&doc, "waybill:deps-dev-outcomes").is_empty());
        }
    }

    #[test]
    fn spdx3_trace_integrity_attach_failures_carry_names_not_counts() {
        let integ = TraceIntegrity {
            uprobe_attach_failures: vec!["libssl.so:SSL_write".to_string()],
            kprobe_attach_failures: vec!["sys_connect".to_string(), "sys_accept".to_string()],
            ..TraceIntegrity::default()
        };
        let comps = [mk_component("pkg:cargo/a@1", vec![])];
        let arts = mk_artifacts(&comps, &integ);
        let artifacts = Spdx3JsonSerializer.serialize(&arts, &mk_cfg()).unwrap();
        let spdx3 = parse_spdx3(&artifacts[0].bytes);
        let value_of = |field: &str| -> String {
            let graph = spdx3
                .get("@graph")
                .and_then(|g| g.as_array())
                .expect("@graph array");
            for el in graph {
                let Some(stmt) = el.get("statement").and_then(|v| v.as_str()) else {
                    continue;
                };
                let Ok(parsed) = serde_json::from_str::<serde_json::Value>(stmt) else {
                    continue;
                };
                if parsed.get("field").and_then(|f| f.as_str()) == Some(field) {
                    return parsed
                        .get("value")
                        .and_then(|v| v.as_str())
                        .expect("envelope value is a string")
                        .to_string();
                }
            }
            panic!("missing SPDX 3 annotation {field}");
        };
        assert_eq!(
            value_of("waybill:trace-integrity-uprobe-attach-failures"),
            r#"["libssl.so:SSL_write"]"#,
        );
        assert_eq!(
            value_of("waybill:trace-integrity-kprobe-attach-failures"),
            r#"["sys_connect","sys_accept"]"#,
        );
    }

    #[test]
    fn spdx3_no_vex_emits_no_external_ref_on_document() {
        let integ = empty_integrity();
        let comps = [mk_component("pkg:cargo/a@1", vec![])];
        let arts = mk_artifacts(&comps, &integ);
        let artifacts = Spdx3JsonSerializer.serialize(&arts, &mk_cfg()).unwrap();
        assert_eq!(
            artifacts.len(),
            1,
            "no advisories → SPDX 3 only, no sidecar artifact"
        );
        let spdx3 = parse_spdx3(&artifacts[0].bytes);
        let spdx_doc = find_spdx3_document(&spdx3);
        assert!(
            spdx_doc.get("externalRef").is_none(),
            "no advisories → SpdxDocument must have no externalRef entry; got {:?}",
            spdx_doc.get("externalRef")
        );
    }

    #[test]
    fn spdx3_with_vex_emits_sidecar_and_external_ref_on_document() {
        let integ = empty_integrity();
        let comps = [mk_component(
            "pkg:cargo/a@1",
            vec![AdvisoryRef {
                id: "CVE-2026-0003".to_string(),
                source: "osv".to_string(),
                url: None,
            }],
        )];
        let arts = mk_artifacts(&comps, &integ);
        let artifacts = Spdx3JsonSerializer.serialize(&arts, &mk_cfg()).unwrap();
        assert_eq!(
            artifacts.len(),
            2,
            "advisory present → SPDX 3 artifact + OpenVEX sidecar"
        );
        let (spdx3_art, vex_art) = match artifacts[0].relative_path.to_string_lossy().as_ref() {
            "waybill.spdx3.json" => (&artifacts[0], &artifacts[1]),
            _ => (&artifacts[1], &artifacts[0]),
        };
        assert_eq!(
            spdx3_art.relative_path,
            std::path::PathBuf::from("waybill.spdx3.json")
        );
        assert_eq!(
            vex_art.relative_path,
            std::path::PathBuf::from("waybill.openvex.json")
        );

        let spdx3 = parse_spdx3(&spdx3_art.bytes);
        let spdx_doc = find_spdx3_document(&spdx3);
        let refs = spdx_doc["externalRef"]
            .as_array()
            .expect("externalRef present on SpdxDocument");
        assert_eq!(refs.len(), 1);
        let r = &refs[0];
        assert_eq!(r["type"], "ExternalRef");
        // Per research.md §R3 / data-model.md §"ExternalRef → OpenVEX
        // sidecar", we use the VEX-precise SPDX 3.0.1 enum value.
        assert_eq!(
            r["externalRefType"], "vulnerabilityExploitabilityAssessment"
        );
        assert_eq!(r["contentType"], "application/openvex+json");
        // `locator` is array-typed in the SPDX 3 vocabulary.
        assert_eq!(
            r["locator"],
            serde_json::json!(["waybill.openvex.json"])
        );
    }

    #[test]
    fn spdx3_openvex_override_path_threads_into_external_ref() {
        let integ = empty_integrity();
        let comps = [mk_component(
            "pkg:cargo/a@1",
            vec![AdvisoryRef {
                id: "CVE-2026-0004".to_string(),
                source: "osv".to_string(),
                url: None,
            }],
        )];
        let arts = mk_artifacts(&comps, &integ);
        let mut cfg = mk_cfg();
        cfg.overrides.insert(
            "openvex".to_string(),
            std::path::PathBuf::from("./vex/out.json"),
        );
        let artifacts = Spdx3JsonSerializer.serialize(&arts, &cfg).unwrap();
        let spdx3 = parse_spdx3(
            artifacts
                .iter()
                .find(|a| a.relative_path == std::path::Path::new("waybill.spdx3.json"))
                .map(|a| &a.bytes)
                .unwrap(),
        );
        let spdx_doc = find_spdx3_document(&spdx3);
        assert_eq!(
            spdx_doc["externalRef"][0]["locator"],
            // #1122: relative to the document (here in the working
            // directory), so the user's `./` prefix is normalized away.
            serde_json::json!(["vex/out.json"]),
            "user override path must appear in the SPDX 3 ExternalRef locator"
        );
    }

    #[test]
    fn spdx3_alias_bytes_are_byte_identical_to_stable() {
        // Contract §4 / research.md §R6: the alias delegates
        // verbatim to Spdx3JsonSerializer — byte-for-byte identical.
        let integ = empty_integrity();
        let comps = [mk_component("pkg:cargo/a@1", vec![])];
        let arts = mk_artifacts(&comps, &integ);
        let cfg = mk_cfg();
        let stable = Spdx3JsonSerializer.serialize(&arts, &cfg).unwrap();
        let alias = Spdx3JsonExperimentalSerializer
            .serialize(&arts, &cfg)
            .unwrap();
        assert_eq!(stable.len(), alias.len());
        for (s, a) in stable.iter().zip(alias.iter()) {
            assert_eq!(s.relative_path, a.relative_path);
            assert_eq!(
                s.bytes, a.bytes,
                "alias must emit byte-identical bytes for {:?}",
                s.relative_path
            );
        }
    }

    #[test]
    fn openvex_override_path_threads_into_external_doc_refs() {
        let integ = empty_integrity();
        let comps = [mk_component(
            "pkg:cargo/a@1",
            vec![AdvisoryRef {
                id: "CVE-2026-0002".to_string(),
                source: "osv".to_string(),
                url: None,
            }],
        )];
        let arts = mk_artifacts(&comps, &integ);
        let mut cfg = mk_cfg();
        cfg.overrides
            .insert("openvex".to_string(), std::path::PathBuf::from("./vex/out.json"));
        let artifacts =
            Spdx2_3JsonSerializer.serialize(&arts, &cfg).unwrap();
        let spdx = parse_spdx(
            artifacts
                .iter()
                .find(|a| a.relative_path == std::path::Path::new("waybill.spdx.json"))
                .map(|a| &a.bytes)
                .unwrap(),
        );
        assert_eq!(
            spdx["externalDocumentRefs"][0]["spdxDocument"],
            // #1122: relative to the document, `./` normalized away.
            "vex/out.json",
            "user override path must appear in the SPDX cross-reference"
        );
    }

    // ---- #799: per-package reference kinds across all three formats ----

    /// Every m776 reference kind reaches all three formats, read back
    /// through the catalog's own extractors (A9–A11, A14–A16).
    ///
    /// The goldens cannot cover this: every golden and corpus scan runs
    /// `--offline`, and deps.dev is the only producer of these kinds, so
    /// those documents carry no references and the rows compare empty
    /// sets. Here each row must extract the same non-empty set from
    /// documents the real serializers emitted.
    #[test]
    fn every_reference_kind_reaches_all_three_formats() {
        use crate::generate::cyclonedx::CycloneDxJsonSerializer;
        use waybill_common::resolution::ExternalReference;

        let mut c = mk_component("pkg:npm/sigstore@2.3.1", vec![]);
        c.external_references = [
            ("attestation", "https://registry.npmjs.org/-/npm/v1/attestations/sigstore@2.3.1"),
            ("distribution", "https://registry.npmjs.org/sigstore/-/sigstore-2.3.1.tgz"),
            ("documentation", "https://docs.sigstore.dev"),
            ("issue-tracker", "https://github.com/sigstore/sigstore-js/issues"),
            ("vcs", "https://github.com/sigstore/sigstore-js"),
            ("website", "https://sigstore.dev"),
        ]
        .into_iter()
        .map(|(ref_type, url)| ExternalReference {
            ref_type: ref_type.to_string(),
            url: url.to_string(),
        })
        .collect();
        let comps = [c];
        let integ = empty_integrity();
        let arts = mk_artifacts(&comps, &integ);
        let emit = |s: &dyn SbomSerializer| -> serde_json::Value {
            let out = s.serialize(&arts, &mk_cfg()).unwrap();
            assert_eq!(out.len(), 1, "{}: one document, no sidecar", s.id());
            serde_json::from_slice(&out[0].bytes).unwrap()
        };
        let cdx = emit(&CycloneDxJsonSerializer);
        let spdx23 = emit(&Spdx2_3JsonSerializer);
        let spdx3 = emit(&Spdx3JsonSerializer);

        for row in ["A9", "A10", "A11", "A14", "A15", "A16"] {
            let ex = waybill::parity::extractors::EXTRACTORS
                .iter()
                .find(|e| e.row_id == row)
                .unwrap();
            let from_cdx = (ex.cdx)(&cdx);
            assert!(!from_cdx.is_empty(), "{row}: CycloneDX carries no value");
            assert_eq!(from_cdx, (ex.spdx23)(&spdx23), "{row}: CycloneDX vs SPDX 2.3");
            assert_eq!(from_cdx, (ex.spdx3)(&spdx3), "{row}: CycloneDX vs SPDX 3");
        }

        // The SPDX 3 entries themselves, in the order the references were
        // normalized to, with `locator` array-typed per the 3.0.1 model.
        let pkg = spdx3["@graph"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["type"] == "software_Package" && e["name"] == "x")
            .unwrap();
        assert_eq!(
            pkg["externalRef"],
            serde_json::json!([
                {"type": "ExternalRef", "externalRefType": "buildMeta",
                 "locator": ["https://registry.npmjs.org/-/npm/v1/attestations/sigstore@2.3.1"]},
                {"type": "ExternalRef", "externalRefType": "documentation",
                 "locator": ["https://docs.sigstore.dev"]},
                {"type": "ExternalRef", "externalRefType": "issueTracker",
                 "locator": ["https://github.com/sigstore/sigstore-js/issues"]},
            ])
        );
    }

    /// A component with no reference that lacks a scalar slot gets no
    /// `externalRef` key at all, rather than an empty list.
    #[test]
    fn no_external_ref_key_without_such_references() {
        let comps = [mk_component("pkg:cargo/a@1", vec![])];
        let integ = empty_integrity();
        let arts = mk_artifacts(&comps, &integ);
        let out = Spdx3JsonSerializer.serialize(&arts, &mk_cfg()).unwrap();
        let spdx3 = parse_spdx3(&out[0].bytes);
        let pkg = spdx3["@graph"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["type"] == "software_Package")
            .unwrap();
        assert!(pkg.get("externalRef").is_none(), "{pkg}");
    }

    /// #1140: the document IRI hashes in the tool version, but no other
    /// element's IRI may depend on it. Hashing full IRIs re-identified every
    /// annotation and relationship at each version bump.
    #[test]
    fn spdx3_element_iris_do_not_change_with_the_tool_version() {
        use waybill_common::resolution::{EnrichmentProvenance, Relationship, RelationshipType};
        let integ = empty_integrity();
        let comps = [mk_component("pkg:cargo/a@1", vec![]), mk_component("pkg:cargo/b@1", vec![])];
        let rels = [Relationship {
            from: "pkg:cargo/a@1".into(),
            to: "pkg:cargo/b@1".into(),
            relationship_type: RelationshipType::DependsOn,
            provenance: EnrichmentProvenance { source: "test".into(), data_type: "test".into() },
        }];
        let local_ids = |version: &'static str| {
            let mut arts = mk_artifacts(&comps, &integ);
            arts.relationships = &rels;
            let cfg = OutputConfig { mikebom_version: version, ..mk_cfg() };
            let doc = parse_spdx(&Spdx3JsonSerializer.serialize(&arts, &cfg).unwrap()[0].bytes);
            let graph = doc["@graph"].as_array().unwrap().clone();
            let doc_iri = graph.iter().find(|e| e["type"] == "SpdxDocument").unwrap()["spdxId"]
                .as_str()
                .unwrap()
                .to_string();
            let ids: std::collections::BTreeSet<String> = graph
                .iter()
                .filter_map(|e| e["spdxId"].as_str())
                .filter(|i| *i != doc_iri)
                .map(|i| i.strip_prefix(doc_iri.as_str()).unwrap_or(i).to_string())
                .collect();
            (doc_iri, ids)
        };
        let (doc_a, ids_a) = local_ids("1.0.0");
        let (doc_b, ids_b) = local_ids("2.0.0");
        assert_ne!(doc_a, doc_b, "the document IRI is expected to carry the version");
        assert!(ids_a.iter().any(|i| i.contains("/anno-")) && ids_a.iter().any(|i| i.contains("/rel-")));
        assert_eq!(ids_a, ids_b);
    }
}
