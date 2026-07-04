//! SPDX 3.0.1 `Annotation` element builder (milestone 011 US2).
//!
//! Per `data-model.md` Element Catalog §`Annotation`: any mikebom
//! signal whose typed semantics don't match a native SPDX 3
//! property exactly (Q2 strict-match rule, FR-011) lands here.
//! One `Annotation` per `(subject, field, value)` tuple.
//!
//! The `statement` property carries the JSON-encoded
//! `MikebomAnnotationCommentV1` envelope reused verbatim from
//! milestone 010 (`super::annotations::MikebomAnnotationCommentV1`).
//! Reusing the envelope across format versions means downstream
//! consumers parse one shape whether they're reading SPDX 2.3
//! `annotations[].comment` or SPDX 3 `Annotation.statement`.
//!
//! Field set mirrors `super::annotations::annotate_component` and
//! `annotate_document` verbatim — if SPDX 2.3 emits a
//! `mikebom:<foo>` annotation for a given component, SPDX 3 emits
//! the same field with the same value (the annotation-fidelity
//! contract, FR-018 / SC-005). The only difference is wrapper
//! shape: SPDX 2.3 uses `SpdxAnnotation { annotator, date, type,
//! comment }`; SPDX 3 uses `{type: "Annotation", spdxId, subject,
//! annotationType: "other", statement}`.

use data_encoding::BASE32_NOPAD;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use mikebom_common::attestation::metadata::GenerationContext;
use mikebom_common::resolution::{ResolutionTechnique, ResolvedComponent};

use super::annotations::{coerce_envelope_value, MikebomAnnotationCommentV1};
use crate::generate::ScanArtifacts;

/// Build the `Annotation` elements for component-level mikebom
/// signals (Section C rows C1–C20 + D1/D2 of the format-mapping
/// doc that stay annotation-bound under the Q2 strict-match rule).
pub fn build_component_annotations(
    components: &[ResolvedComponent],
    package_iri_by_purl: &std::collections::BTreeMap<String, String>,
    doc_iri: &str,
    creation_info_id: &str,
    _include_dev: bool,
    include_source_files: bool,
) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for c in components {
        let Some(pkg_iri) = package_iri_by_purl.get(c.purl.as_str()) else {
            continue;
        };
        push_component_fields(
            &mut out,
            pkg_iri,
            doc_iri,
            creation_info_id,
            c,
            _include_dev,
            include_source_files,
        );
    }
    sort_by_spdx_id(&mut out);
    out
}

/// Build the `Annotation` elements for document-level mikebom
/// signals (rows C21–C23 + E1) — generation-context, os-release-
/// missing-fields, trace-integrity-*, compositions.
pub fn build_document_annotations(
    scan: &ScanArtifacts<'_>,
    doc_iri: &str,
    creation_info_id: &str,
    graph_completeness: &crate::generate::graph_completeness::GraphCompletenessResult,
) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    push_document_fields(&mut out, doc_iri, creation_info_id, scan);
    push_m158_graph_completeness_annotations(
        &mut out,
        doc_iri,
        creation_info_id,
        graph_completeness,
    );
    sort_by_spdx_id(&mut out);
    out
}

/// Milestone 158 US2 — emit the two document-scope
/// `mikebom:graph-completeness` + `mikebom:graph-completeness-reason`
/// annotations in SPDX 3.
fn push_m158_graph_completeness_annotations(
    out: &mut Vec<Value>,
    doc_iri: &str,
    creation_info_id: &str,
    gc: &crate::generate::graph_completeness::GraphCompletenessResult,
) {
    // Always emit the primary value annotation per FR-003.
    out.push(build_annotation(
        doc_iri,
        doc_iri,
        creation_info_id,
        "mikebom:graph-completeness",
        serde_json::Value::String(gc.value.as_str().to_string()),
    ));
    // Conditional reason per FR-004 / FR-005.
    if gc.value != crate::generate::graph_completeness::GraphCompletenessValue::Complete
        && !gc.reason_codes.is_empty()
    {
        out.push(build_annotation(
            doc_iri,
            doc_iri,
            creation_info_id,
            "mikebom:graph-completeness-reason",
            serde_json::Value::String(
                crate::generate::graph_completeness::join_reason_codes(&gc.reason_codes),
            ),
        ));
    }
}

/// Milestone 119 phase-2 — build SPDX 3 Annotation elements for the
/// supplement-declared services projected onto `software_Package` via
/// `v3_packages::supplement_service_to_v3_package`. Each service gets
/// two Annotations: `mikebom:component-role = "saas-service"` (C40
/// fallback per research Decision 4) and `mikebom:source-tier =
/// "declared"` (C65 marker). Endpoints + description are already
/// carried on the Package element by the projection helper.
pub fn build_supplement_service_annotations(
    doc_iri: &str,
    creation_info_id: &str,
) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    let Some(services) = crate::supplement::current_services() else {
        return out;
    };
    for svc in &services {
        let pkg_iri = super::v3_packages::supplement_service_iri(svc, doc_iri);
        out.push(build_annotation(
            &pkg_iri,
            doc_iri,
            creation_info_id,
            "mikebom:component-role",
            json!("saas-service"),
        ));
        out.push(build_annotation(
            &pkg_iri,
            doc_iri,
            creation_info_id,
            "mikebom:source-tier",
            json!("declared"),
        ));
    }
    sort_by_spdx_id(&mut out);
    out
}

/// Build a single SPDX 3 `Annotation` element wrapping the shared
/// `MikebomAnnotationCommentV1` envelope.
fn build_annotation(
    subject_iri: &str,
    doc_iri: &str,
    creation_info_id: &str,
    field: &str,
    value: serde_json::Value,
) -> Value {
    let envelope = MikebomAnnotationCommentV1::new(field, coerce_envelope_value(value));
    let statement = envelope.to_comment_string();
    // ID derivation MUST NOT include `statement` — that string carries
    // workspace-relative source-file paths for `mikebom:source-files`,
    // and including host-specific bytes here breaks cross-host
    // byte-identity (milestone 017 T013b: same scan on macOS dev vs
    // Linux CI produced different `anno-*` hashes, displacing every
    // annotation in the spdxId-sorted `@graph[]` array). `subject|field`
    // is already unique per annotation: `push_*_fields` emits one
    // annotation per (component, field) pair, with no duplicate field
    // names per subject.
    let anno_iri = format!(
        "{doc_iri}/anno-{}",
        hash_prefix(format!("{subject_iri}|{field}").as_bytes(), 16)
    );
    json!({
        "type": "Annotation",
        "spdxId": anno_iri,
        "creationInfo": creation_info_id,
        "subject": subject_iri,
        "annotationType": "other",
        "statement": statement,
    })
}

/// Mirror of `annotations::annotate_component` — same 20 rows,
/// same emission gates, same envelope shape. Keep in lockstep.
#[allow(clippy::too_many_arguments)]
fn push_component_fields(
    out: &mut Vec<Value>,
    subject_iri: &str,
    doc_iri: &str,
    creation_info_id: &str,
    c: &ResolvedComponent,
    _include_dev: bool,
    include_source_files: bool,
) {
    let push = |out: &mut Vec<Value>, field: &str, value: serde_json::Value| {
        out.push(build_annotation(
            subject_iri,
            doc_iri,
            creation_info_id,
            field,
            value,
        ));
    };

    // C1 source-type
    if let Some(ref v) = c.source_type {
        push(out, "mikebom:source-type", json!(v));
    }
    // C2 source-connection-ids
    if !c.evidence.source_connection_ids.is_empty() {
        push(
            out,
            "mikebom:source-connection-ids",
            json!(c.evidence.source_connection_ids.join(",")),
        );
    }
    // C3 deps-dev-match
    if let Some(ref m) = c.evidence.deps_dev_match {
        push(
            out,
            "mikebom:deps-dev-match",
            json!(format!("{}:{}@{}", m.system, m.name, m.version)),
        );
    }
    // C4 evidence-kind
    if let Some(ref v) = c.evidence_kind {
        push(out, "mikebom:evidence-kind", json!(v));
    }
    // C5 sbom-tier
    if let Some(ref v) = c.sbom_tier {
        push(out, "mikebom:sbom-tier", json!(v));
    }
    // C6 (milestone 052/part-2): the legacy `mikebom:dev-dependency`
    // annotation is REMOVED. Per Constitution Principle V (v1.4.0),
    // SPDX 3.0.1 has a native `lifecycleScope` parameter on
    // `dependsOn` relationships (LifecycleScopeType enum:
    // `development`, `build`, `test`, `runtime`). The signal travels
    // via the typed `RelationshipType::{Dev,Build,Test}DependsOn`
    // variants set by `apply_lifecycle_scope_to_edges` in
    // `scan_fs/mod.rs`, then emitted as `lifecycleScope` by
    // `spdx/v3_relationships.rs`. No annotation needed.
    // Milestone 112: `mikebom:build-inclusion` — parity-bridging
    // element annotation. SPDX 3.0.1's `LifecycleScopeType` has no
    // excluded or unknown value (CDX expresses not-needed natively
    // via `scope: "excluded"`); the annotation is the only carrier
    // here. Values: `unknown` | `not-needed`. The companion
    // `mikebom:build-inclusion-derivation` flows through the
    // extra_annotations bag. Documented in
    // `docs/reference/sbom-format-mapping.md`.
    if let Some(inclusion) = c.build_inclusion {
        push(out, "mikebom:build-inclusion", json!(inclusion.as_str()));
    }
    // C7 co-owned-by
    if let Some(ref v) = c.co_owned_by {
        push(out, "mikebom:co-owned-by", json!(v));
    }
    // C8 shade-relocation
    if c.shade_relocation == Some(true) {
        push(out, "mikebom:shade-relocation", json!("true"));
    }
    // C9 npm-role
    if let Some(ref v) = c.npm_role {
        push(out, "mikebom:npm-role", json!(v));
    }
    // C10 binary-class
    if let Some(ref v) = c.binary_class {
        push(out, "mikebom:binary-class", json!(v));
    }
    // C11 binary-stripped
    if let Some(v) = c.binary_stripped {
        push(
            out,
            "mikebom:binary-stripped",
            json!(if v { "true" } else { "false" }),
        );
    }
    // C12 linkage-kind
    if let Some(ref v) = c.linkage_kind {
        push(out, "mikebom:linkage-kind", json!(v));
    }
    // C13 buildinfo-status
    if let Some(ref v) = c.buildinfo_status {
        push(out, "mikebom:buildinfo-status", json!(v));
    }
    // C14 detected-go
    if c.detected_go == Some(true) {
        push(out, "mikebom:detected-go", json!("true"));
    }
    // C15 binary-packed
    if let Some(ref v) = c.binary_packed {
        push(out, "mikebom:binary-packed", json!(v));
    }
    // C16 confidence
    if let Some(ref v) = c.confidence {
        push(out, "mikebom:confidence", json!(v));
    }
    // C17 raw-version
    if let Some(ref v) = c.raw_version {
        push(out, "mikebom:raw-version", json!(v));
    }
    // C42 mikebom:lifecycle-scope — DELIBERATELY OMITTED in SPDX 3.
    // SPDX 3 carries scope natively via `LifecycleScopedRelationship.
    // scope` (set in `v3_relationships.rs` for Dev/Build/TestDependsOn
    // edges). Per Constitution Principle V, the native field is the
    // primary signal and mikebom MUST NOT add a redundant annotation.
    // Issue #228 + spdx3_annotation_fidelity test enforces this contract.
    // Milestone 145 US2 originally tried to emit here; reverted after the
    // fidelity test caught the Principle-V violation. The 261 audit
    // findings flagged on this annotation are false positives from the
    // harness misreading the SPDX 3 scope mechanism.
    // C18 source-files
    if include_source_files && !c.evidence.source_file_paths.is_empty() {
        push(
            out,
            "mikebom:source-files",
            json!(c.evidence.source_file_paths),
        );
    }
    // C19 cpe-candidates — emits full candidate list when more
    // than one candidate exists. Matches SPDX 2.3 shape. The
    // native ExternalIdentifier[cpe23] entries (T012) cover the
    // fully-resolved candidates separately; this annotation
    // carries the whole candidate set for lossless recovery.
    if c.cpes.len() > 1 {
        push(out, "mikebom:cpe-candidates", json!(c.cpes));
    }
    // C20 requirement-range
    if let Some(ref v) = c.requirement_range {
        push(out, "mikebom:requirement-range", json!(v));
    }

    // D1 evidence.identity — unconditional.
    let technique = match c.evidence.technique {
        ResolutionTechnique::UrlPattern => "url-pattern",
        ResolutionTechnique::HashMatch => "hash-match",
        ResolutionTechnique::PackageDatabase => "package-database",
        ResolutionTechnique::FilePathPattern => "file-path-pattern",
        ResolutionTechnique::HostnameHeuristic => "hostname-heuristic",
    };
    push(
        out,
        "evidence.identity",
        json!({
            "technique": technique,
            "confidence": c.evidence.confidence,
        }),
    );

    // D2 evidence.occurrences
    if !c.occurrences.is_empty() {
        let items: Vec<serde_json::Value> = c
            .occurrences
            .iter()
            .map(|o| {
                let mut obj = serde_json::Map::new();
                obj.insert(
                    "location".into(),
                    json!(crate::scan_fs::sbom_path::normalize_sbom_path_str(&o.location)),
                );
                obj.insert("sha256".into(), json!(o.sha256));
                if let Some(ref md5) = o.md5_legacy {
                    obj.insert("md5".into(), json!(md5));
                }
                serde_json::Value::Object(obj)
            })
            .collect();
        push(out, "evidence.occurrences", json!(items));
    }

    // Milestone 023: generic per-component annotation bag. Each
    // entry surfaces as a SPDX 3 graph-element Annotation. BTreeMap
    // iteration order is sorted by key — deterministic across runs.
    //
    // Milestone 127: filter out internal-only keys (the
    // `mikebom:is-workspace-root` signal that drives root-selector
    // logic but is NOT meant to surface in emitted SBOMs).
    for (key, value) in &c.extra_annotations {
        if crate::generate::root_selector::is_internal_emission_key(key)
            || crate::generate::root_selector::is_field_owned_annotation_key(key)
        {
            // Milestone 145 US3 (FR-009): skip keys already emitted
            // from a field-derived source (e.g., `mikebom:source-files`
            // comes from `c.evidence.source_file_paths` at line ~267
            // above) — re-emitting from the bag double-stamps.
            continue;
        }
        push(out, key, value.clone());
    }
}

/// Mirror of `annotations::annotate_document` — C21–C23 + E1.
fn push_document_fields(
    out: &mut Vec<Value>,
    doc_iri: &str,
    creation_info_id: &str,
    scan: &ScanArtifacts<'_>,
) {
    let push = |out: &mut Vec<Value>, field: &str, value: serde_json::Value| {
        out.push(build_annotation(
            doc_iri,
            doc_iri,
            creation_info_id,
            field,
            value,
        ));
    };

    // C21 generation-context
    let gc = match scan.generation_context {
        GenerationContext::FilesystemScan => "filesystem-scan",
        GenerationContext::ContainerImageScan => "container-image-scan",
        GenerationContext::BuildTimeTrace => "build-time-trace",
    };
    push(out, "mikebom:generation-context", json!(gc));

    // Milestone 133 US4 (Constitution Strict Boundary §5):
    // `--file-inventory=full` opt-in marker.
    if let Some("full") = scan.file_inventory_mode {
        push(out, "mikebom:file-inventory-mode", json!("full"));
    }

    // Milestone 133 US3 (C93/C94/C95): file-tier walker diagnostic
    // skip counters. Constitution Principle X. See CDX +
    // SPDX 2.3 twins.
    if let Some(stats) = scan.file_inventory_stats {
        if stats.oversize_skipped > 0 {
            push(
                out,
                "mikebom:file-inventory-skipped-oversize",
                json!(stats.oversize_skipped.to_string()),
            );
        }
        if stats.special_skipped > 0 {
            push(
                out,
                "mikebom:file-inventory-skipped-special-files",
                json!(stats.special_skipped.to_string()),
            );
        }
        if stats.unreadable_skipped > 0 {
            push(
                out,
                "mikebom:file-inventory-unreadable",
                json!(stats.unreadable_skipped.to_string()),
            );
        }
    }

    // C22 os-release-missing-fields
    if !scan.os_release_missing_fields.is_empty() {
        push(
            out,
            "mikebom:os-release-missing-fields",
            json!(scan.os_release_missing_fields),
        );
    }

    // Milestone 113 FR-014 / Constitution Principle X: user-supplied
    // directory exclusions active for this scan. Mirrors CDX
    // `metadata.properties` + SPDX 2.3 envelope annotation.
    if let Some(entries) =
        crate::scan_fs::package_db::exclude_path::current_annotation()
    {
        if !entries.is_empty() {
            push(
                out,
                "mikebom:exclude-path",
                json!(entries.join(",")),
            );
        }
    }

    // Milestone 119 phase-2 — document-scope supplement-cdx provenance
    // mirrors the CDX twin in `cyclonedx/metadata.rs` and the SPDX 2.3
    // twin in `spdx/annotations.rs`.
    if let Some(prov) = crate::supplement::current_provenance() {
        push(
            out,
            "mikebom:supplement-cdx",
            json!(
                crate::supplement::annotation::build_supplement_cdx_provenance_string(
                    &prov.source_path,
                    &prov.source_sha256,
                )
            ),
        );
    }

    // Milestone 127 FR-006 — document-scope root-selection-heuristic
    // signal. Emitted only when the selector's ladder fired AND the
    // auto-pick actually fell through past at least one detected
    // main-module (losers non-empty). Suppressed on the count==1 fast
    // path (heuristic=None), under operator override (heuristic=None),
    // AND when zero main-modules existed (losers empty — no loss to
    // signal). Preserves byte-identity on the 33 alpha.48 goldens
    // per SC-003. Envelope shape per contracts/annotation-schema.md.
    {
        let selection = crate::generate::root_selector::select_root(
            scan.components,
            &scan.root_override,
            scan.scan_target_coord,
            scan.target_name,
            "0.0.0",
        );
        if let Some(h) = selection.heuristic {
            if !selection.losers.is_empty() {
                push(
                    out,
                    "mikebom:root-selection-heuristic",
                    json!({
                        "heuristic": h.name(),
                        "confidence": h.confidence(),
                    }),
                );
            }
        }
    }

    // C44 (milestone 061, closes #119): doc-level Go graph-completeness
    // signal — Principle X transparency for the Trivy-style orphan
    // condition introduced by milestone 059. Absent annotation ⇒ no
    // Go scan happened (signal not applicable).
    if let Some(gc) = scan.go_graph_completeness {
        let value = serde_json::to_value(gc)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "unknown".to_string());
        push(out, "mikebom:graph-completeness", json!(value));
        if let Some(reason) = scan.go_graph_completeness_reason {
            if !reason.is_empty() {
                push(out, "mikebom:graph-completeness-reason", json!(reason));
            }
        }
    }

    // Milestone 134 (closes #125, catalog row C100): document-scope
    // `mikebom:purl-collisions-detected` summary. Omitted when no
    // collisions were detected so clean scans stay byte-identical to
    // alpha.51 emissions (FR-009 / SC-002).
    if let Some(summary) = scan.collisions_summary {
        if let Ok(value) = serde_json::to_value(summary) {
            push(out, "mikebom:purl-collisions-detected", value);
        }
    }

    // C23 trace-integrity-* — four unconditional scalars.
    push(
        out,
        "mikebom:trace-integrity-ring-buffer-overflows",
        json!(scan.integrity.ring_buffer_overflows),
    );
    push(
        out,
        "mikebom:trace-integrity-events-dropped",
        json!(scan.integrity.events_dropped),
    );
    push(
        out,
        "mikebom:trace-integrity-uprobe-attach-failures",
        json!(scan.integrity.uprobe_attach_failures),
    );
    push(
        out,
        "mikebom:trace-integrity-kprobe-attach-failures",
        json!(scan.integrity.kprobe_attach_failures),
    );

    // E1 compositions
    if !scan.complete_ecosystems.is_empty() {
        push(
            out,
            "compositions",
            json!({
                "complete_ecosystems": scan.complete_ecosystems,
            }),
        );
    }
}

fn sort_by_spdx_id(values: &mut [Value]) {
    values.sort_by(|a, b| {
        let key = |v: &Value| v["spdxId"].as_str().unwrap_or("").to_string();
        key(a).cmp(&key(b))
    });
}

fn hash_prefix(input: &[u8], chars: usize) -> String {
    let digest = Sha256::digest(input);
    let encoded = BASE32_NOPAD.encode(&digest);
    encoded[..chars].to_string()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    /// SPDX 3 mirror of the SPDX 2.3 regression test in
    /// `annotations.rs::build_annotation_with_bool_value_produces_string_envelope_value`.
    /// Both emitters MUST coerce non-String envelope values to strings
    /// so the wire output matches CDX's `serde_json::to_string(other)`
    /// coercion. Caught by the 2026-06 sbom-conformance audit on
    /// `mikebom:detected-cargo-auditable` + `mikebom:not-linked`.
    #[test]
    fn v3_build_annotation_with_bool_value_produces_string_envelope_value() {
        let anno = build_annotation(
            "https://example.org/doc#SPDXRef-comp-1",
            "https://example.org/doc",
            "_:creationinfo",
            "mikebom:not-linked",
            serde_json::json!(true),
        );
        let statement = anno
            .get("statement")
            .and_then(|s| s.as_str())
            .expect("annotation must carry a statement field");
        let parsed: MikebomAnnotationCommentV1 = serde_json::from_str(statement).unwrap();
        assert_eq!(
            parsed.value,
            serde_json::Value::String("true".to_string()),
            "SPDX 3 envelope value must be a String, not a Bool",
        );
        assert!(
            statement.contains(r#""value":"true""#),
            "statement must serialize bool as string-true: {statement}",
        );
        assert!(
            !statement.contains(r#""value":true"#),
            "statement must NOT serialize bool as bare true: {statement}",
        );
    }

    // -----------------------------------------------------------------
    // Milestone 145 US2 (T007/T008/T009): SPDX 3 lifecycle-scope
    // emission parity with CDX + SPDX 2.3.
    // -----------------------------------------------------------------

    /// Helper: construct a minimal valid `ResolvedComponent` for tests.
    /// Only `purl` is required; everything else defaults. The caller
    /// overrides specific fields (e.g., `lifecycle_scope`) before use.
    fn synthetic_resolved_component(
        lifecycle_scope: Option<mikebom_common::resolution::LifecycleScope>,
    ) -> ResolvedComponent {
        use mikebom_common::resolution::{
            ResolutionEvidence, ResolutionTechnique, ResolvedComponent,
        };
        let purl = mikebom_common::types::purl::Purl::new("pkg:npm/test@1.0.0").unwrap();
        ResolvedComponent {
            build_inclusion: None,
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            purl,
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::PackageDatabase,
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
            lifecycle_scope,
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
            external_references: vec![],
            extra_annotations: Default::default(),
            binary_role: None,
        }
    }

    /// Returns the parsed `MikebomAnnotationCommentV1` envelopes from a
    /// vector of SPDX 3 annotation graph elements whose `field` matches
    /// the supplied predicate.
    fn envelopes_for_field(
        annos: &[Value],
        field_name: &str,
    ) -> Vec<MikebomAnnotationCommentV1> {
        annos
            .iter()
            .filter_map(|a| a.get("statement").and_then(|s| s.as_str()))
            .filter_map(|s| serde_json::from_str::<MikebomAnnotationCommentV1>(s).ok())
            .filter(|env| env.field == field_name)
            .collect()
    }

    /// Milestone 145 US2 REVERTED (Principle V): SPDX 3 carries
    /// lifecycle scope natively via `LifecycleScopedRelationship.scope`.
    /// Asserts the `mikebom:lifecycle-scope` annotation is NOT emitted
    /// at the Package level — issue #228's existing design contract.
    /// Guards against future reintroduction of the redundant emission.
    #[test]
    fn spdx3_lifecycle_scope_not_emitted_as_annotation_md145() {
        use mikebom_common::resolution::LifecycleScope;
        for scope in [
            Some(LifecycleScope::Development),
            Some(LifecycleScope::Build),
            Some(LifecycleScope::Test),
            Some(LifecycleScope::Runtime),
            None,
        ] {
            let c = synthetic_resolved_component(scope);
            let mut out = Vec::new();
            push_component_fields(
                &mut out,
                "https://example.org/doc#SPDXRef-comp-1",
                "https://example.org/doc",
                "_:creationinfo",
                &c,
                true,
                true,
            );
            let envs = envelopes_for_field(&out, "mikebom:lifecycle-scope");
            assert!(
                envs.is_empty(),
                "Principle V violation: SPDX 3 carries scope natively via \
                 LifecycleScopedRelationship.scope — mikebom:lifecycle-scope \
                 annotation MUST NOT appear on Package elements. Got: {envs:?} (scope={scope:?})"
            );
        }
    }

    /// T016 (US3 + FR-009 + SC-009): when a component has BOTH a
    /// field-derived `mikebom:source-files` (from
    /// `c.evidence.source_file_paths`) AND a legacy stamping in
    /// `extra_annotations["mikebom:source-files"]` (the pre-145 Maven
    /// nested-JAR pattern), the SPDX 3 emitter MUST emit EXACTLY ONE
    /// `mikebom:source-files` annotation — the field-derived one.
    /// Guards against regression of the per-emitter double-emission
    /// bug that caused 51 audit findings on polyglot-builder-image.
    #[test]
    fn spdx3_source_files_dedup_no_double_emission_md145() {
        let mut c = synthetic_resolved_component(None);
        c.evidence.source_file_paths =
            vec!["root/.m2/repository/test/foo/1.0/foo-1.0.jar".to_string()];
        // Pre-145 the Maven reader stamped THIS key (now renamed to
        // mikebom:source-files-nested-url per Option 2b); we replicate
        // the pre-145 shape to PROVE the emitter-side guard (Option 1)
        // would suppress the double-emission even if a future reader
        // recreated the trap.
        c.extra_annotations.insert(
            "mikebom:source-files".to_string(),
            serde_json::json!("root/.m2/.../foo-1.0.jar!META-INF/MANIFEST.MF"),
        );
        let mut out = Vec::new();
        push_component_fields(
            &mut out,
            "https://example.org/doc#SPDXRef-comp-1",
            "https://example.org/doc",
            "_:creationinfo",
            &c,
            /* include_dev = */ true,
            /* include_source_files = */ true,
        );
        let envs = envelopes_for_field(&out, "mikebom:source-files");
        assert_eq!(
            envs.len(),
            1,
            "FR-009 violation: SPDX 3 emitted {} mikebom:source-files entries; \
             expected EXACTLY ONE (the field-derived one wins). envs={envs:?}",
            envs.len()
        );
        // The surviving entry MUST be the field-derived rootfs-relative
        // JAR path, NOT the legacy `<outer>!<inner>!...` URL string.
        assert_eq!(
            envs[0].value,
            serde_json::json!(["root/.m2/repository/test/foo/1.0/foo-1.0.jar"])
        );
    }

}
