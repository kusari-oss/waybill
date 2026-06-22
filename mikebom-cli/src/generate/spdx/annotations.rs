//! SPDX 2.3 `annotations[]` envelope for mikebom-specific data
//! preserved losslessly via `MikebomAnnotationCommentV1` (milestone
//! 010, T033 / T034).
//!
//! SPDX 2.3 has no native home for mikebom's cross-cutting
//! properties — `mikebom:*` component properties, CycloneDX
//! `evidence.identity` / `evidence.occurrences`, and `compositions`.
//! Per spec.md Clarification Q2 + FR-016, these land in SPDX
//! `annotations[]` entries whose `comment` field carries a JSON-
//! encoded envelope. Consumers that ignore annotations see a clean
//! SPDX document; consumers that parse them recover full mikebom
//! fidelity. The per-field placement contract is
//! `contracts/sbom-format-mapping.md` Sections C / D / E.
//!
//! The envelope's JSON schema is
//! `contracts/mikebom-annotation.schema.json` — the
//! `annotation_envelope_schema_matches_json_file` unit test in this
//! module is a structural canary that catches drift between the
//! Rust type and the committed schema.

use mikebom_common::attestation::integrity::TraceIntegrity;
use mikebom_common::attestation::metadata::GenerationContext;
use mikebom_common::resolution::{ResolutionTechnique, ResolvedComponent};

use super::document::{SpdxAnnotation, SpdxAnnotationType};
use crate::generate::ScanArtifacts;

/// Versioned envelope identifier. Bumping this constant requires a
/// coordinated update to `contracts/mikebom-annotation.schema.json`
/// (which pins `schema` to the exact same string via `const`).
pub const ENVELOPE_SCHEMA_V1: &str = "mikebom-annotation/v1";

/// The JSON payload mikebom places inside `SpdxAnnotation.comment`.
///
/// `field` is the originating mikebom identifier as it appears in
/// CycloneDX — e.g. `"mikebom:evidence-kind"`, `"evidence.identity"`,
/// `"compositions"`. The exact set of legal identifiers is
/// enumerated in the data-placement map.
///
/// `value` is free-form JSON: mirrors whatever shape the same field
/// carries in CycloneDX. Consumers should treat it as opaque unless
/// they know the field's schema.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MikebomAnnotationCommentV1 {
    pub schema: String,
    pub field: String,
    pub value: serde_json::Value,
}

impl MikebomAnnotationCommentV1 {
    /// Construct a v1 envelope with the fixed `schema` constant.
    pub fn new(field: impl Into<String>, value: serde_json::Value) -> Self {
        Self {
            schema: ENVELOPE_SCHEMA_V1.to_string(),
            field: field.into(),
            value,
        }
    }

    /// Serialize the envelope to a compact JSON string suitable for
    /// placing inside `SpdxAnnotation.comment`. "Compact" (not
    /// pretty-printed) because the comment is already nested inside
    /// a pretty-printed SPDX document; two layers of indentation
    /// make the output harder to read, not easier.
    pub fn to_comment_string(&self) -> String {
        serde_json::to_string(self)
            .expect("MikebomAnnotationCommentV1 serializes infallibly (no Map<K,V>)")
    }
}

/// Build an `SpdxAnnotation` whose `comment` is a mikebom-namespaced
/// v1 envelope. `annotator` and `date` are passed through — callers
/// typically use `"Tool: mikebom-<version>"` (matching
/// `CreationInfo.creators`) and the shared
/// `OutputConfig.created` stamp respectively, so annotation
/// timestamps stay consistent with the document's creation info.
pub fn build_annotation(
    annotator: &str,
    date: &str,
    field: &str,
    value: serde_json::Value,
) -> SpdxAnnotation {
    let envelope = MikebomAnnotationCommentV1::new(field, value);
    SpdxAnnotation {
        annotator: annotator.to_string(),
        date: date.to_string(),
        kind: SpdxAnnotationType::Other,
        comment: envelope.to_comment_string(),
    }
}

/// Build every per-component annotation mikebom's SPDX 2.3 output
/// emits for `c`. Follows `contracts/sbom-format-mapping.md`
/// Sections C (rows C1–C20) and D (D1 identity, D2 occurrences).
///
/// Entry-emission rules mirror the CycloneDX emission in
/// `generate/cyclonedx/builder.rs` + `evidence.rs` — if a field is
/// emitted in CDX for a given `ResolvedComponent`, its SPDX
/// annotation twin is emitted here too (that's the FR-015 / FR-016
/// fidelity guarantee). Absent fields stay absent.
pub fn annotate_component(
    annotator: &str,
    date: &str,
    c: &ResolvedComponent,
    _include_dev: bool,
    include_source_files: bool,
) -> Vec<SpdxAnnotation> {
    use serde_json::json;
    let mut out: Vec<SpdxAnnotation> = Vec::new();
    let push = |out: &mut Vec<SpdxAnnotation>, field: &str, value: serde_json::Value| {
        out.push(build_annotation(annotator, date, field, value));
    };

    // C1 source-type
    if let Some(ref v) = c.source_type {
        push(&mut out, "mikebom:source-type", json!(v));
    }
    // C2 source-connection-ids (from evidence)
    if !c.evidence.source_connection_ids.is_empty() {
        // Match CDX shape: comma-joined; consumers that want the
        // list back can split on commas. Losslessly trivial.
        push(
            &mut out,
            "mikebom:source-connection-ids",
            json!(c.evidence.source_connection_ids.join(",")),
        );
    }
    // C3 deps-dev-match (from evidence)
    if let Some(ref m) = c.evidence.deps_dev_match {
        push(
            &mut out,
            "mikebom:deps-dev-match",
            json!(format!("{}:{}@{}", m.system, m.name, m.version)),
        );
    }
    // C4 evidence-kind
    if let Some(ref v) = c.evidence_kind {
        push(&mut out, "mikebom:evidence-kind", json!(v));
    }
    // C5 sbom-tier
    if let Some(ref v) = c.sbom_tier {
        push(&mut out, "mikebom:sbom-tier", json!(v));
    }
    // C42 `mikebom:lifecycle-scope` — parity-bridging annotation per
    // Constitution Principle V's "format-asymmetry" carve-out, added
    // for issue #228.
    //
    // Background: milestone 052/part-2 removed the legacy
    // `mikebom:dev-dependency` annotation because SPDX 2.3 already
    // carries the same signal natively via the typed scoped
    // relationship variants (`DEV_DEPENDENCY_OF` / `BUILD_DEPENDENCY_OF`
    // / `TEST_DEPENDENCY_OF`). That removal was correct given those
    // edges were the sole consumer signal. Issue #228 surfaced a
    // separate problem: scope information that lives ONLY on the
    // relationship type is invisible to any consumer walking the
    // dependency graph by `DEPENDS_ON` alone — which, per the Trivy +
    // Syft survey, covers most of the deployed SBOM-consumer
    // ecosystem. CDX bridges this with a Package-level
    // `mikebom:lifecycle-scope` property (the C42 slot, also set on
    // CDX components per `cdx/builder.rs`). Without an equivalent on
    // the SPDX 2.3 Package, a consumer reading the SPDX 2.3 document
    // (without scope-aware edge walking) cannot tell that
    // `testify` / `junit` / `criterion` are dev-tier rather than
    // deployed-runtime — a material distinction for vulnerability
    // triage, license risk, and deployment auditing.
    //
    // The annotation is additive — the typed relationship variants
    // remain the primary spec-native signal, and the
    // `--spdx2-relationship-compat` flag controls whether they're
    // emitted (`full`, default) or collapsed to flat `DEPENDS_ON`
    // for downstream consumers that only implement the basic
    // vocabulary (`basic`). The Package annotation is the same in
    // both modes; consumers can rely on it regardless of compat
    // mode.
    //
    // Per Principle V documentation requirement, this is documented
    // in `docs/reference/sbom-format-mapping.md` under C42.
    if let Some(ref scope) = c.lifecycle_scope {
        use mikebom_common::resolution::LifecycleScope;
        let scope_str = match scope {
            LifecycleScope::Development => Some("development"),
            LifecycleScope::Build => Some("build"),
            LifecycleScope::Test => Some("test"),
            LifecycleScope::Runtime => None, // runtime is the default; no annotation
        };
        if let Some(s) = scope_str {
            push(&mut out, "mikebom:lifecycle-scope", json!(s));
        }
    }
    // Milestone 112: `mikebom:build-inclusion` — parity-bridging
    // annotation per Constitution Principle V's format-asymmetry
    // carve-out. SPDX 2.3 has no per-package excluded-scope or
    // build-inclusion construct (CDX expresses not-needed natively
    // via `scope: "excluded"`); the annotation is the only carrier
    // here. Values: `unknown` | `not-needed`. The companion
    // `mikebom:build-inclusion-derivation` flows through the
    // extra_annotations bag. Documented in
    // `docs/reference/sbom-format-mapping.md`.
    if let Some(inclusion) = c.build_inclusion {
        push(&mut out, "mikebom:build-inclusion", json!(inclusion.as_str()));
    }
    // C7 co-owned-by
    if let Some(ref v) = c.co_owned_by {
        push(&mut out, "mikebom:co-owned-by", json!(v));
    }
    // C8 shade-relocation
    if c.shade_relocation == Some(true) {
        push(&mut out, "mikebom:shade-relocation", json!("true"));
    }
    // C9 npm-role
    if let Some(ref v) = c.npm_role {
        push(&mut out, "mikebom:npm-role", json!(v));
    }
    // C10 binary-class
    if let Some(ref v) = c.binary_class {
        push(&mut out, "mikebom:binary-class", json!(v));
    }
    // C11 binary-stripped
    if let Some(v) = c.binary_stripped {
        push(
            &mut out,
            "mikebom:binary-stripped",
            json!(if v { "true" } else { "false" }),
        );
    }
    // C12 linkage-kind
    if let Some(ref v) = c.linkage_kind {
        push(&mut out, "mikebom:linkage-kind", json!(v));
    }
    // C13 buildinfo-status
    if let Some(ref v) = c.buildinfo_status {
        push(&mut out, "mikebom:buildinfo-status", json!(v));
    }
    // C14 detected-go
    if c.detected_go == Some(true) {
        push(&mut out, "mikebom:detected-go", json!("true"));
    }
    // C15 binary-packed
    if let Some(ref v) = c.binary_packed {
        push(&mut out, "mikebom:binary-packed", json!(v));
    }
    // C16 confidence
    if let Some(ref v) = c.confidence {
        push(&mut out, "mikebom:confidence", json!(v));
    }
    // C17 raw-version
    if let Some(ref v) = c.raw_version {
        push(&mut out, "mikebom:raw-version", json!(v));
    }
    // C18 source-files — same gate as CDX (only when
    // include_source_files AND non-empty). SPDX value is the array
    // (CDX uses a comma-joined string; here we keep JSON fidelity).
    if include_source_files && !c.evidence.source_file_paths.is_empty() {
        push(
            &mut out,
            "mikebom:source-files",
            json!(c.evidence.source_file_paths),
        );
    }
    // C19 cpe-candidates — only when MORE than one candidate was
    // synthesized. The first (primary) candidate goes into the
    // native `externalRefs[SECURITY/cpe23Type]` per A12 (handled in
    // packages.rs); the full candidate set lives here.
    if c.cpes.len() > 1 {
        push(&mut out, "mikebom:cpe-candidates", json!(c.cpes));
    }
    // C20 requirement-range
    if let Some(ref v) = c.requirement_range {
        push(&mut out, "mikebom:requirement-range", json!(v));
    }

    // D1 evidence.identity — technique + confidence. Emit
    // unconditionally because every ResolvedComponent has a
    // technique; `confidence` defaults to 0.0 if absent, which is
    // information too (and CDX emits this too).
    let technique = match c.evidence.technique {
        ResolutionTechnique::UrlPattern => "url-pattern",
        ResolutionTechnique::HashMatch => "hash-match",
        ResolutionTechnique::PackageDatabase => "package-database",
        ResolutionTechnique::FilePathPattern => "file-path-pattern",
        ResolutionTechnique::HostnameHeuristic => "hostname-heuristic",
    };
    push(
        &mut out,
        "evidence.identity",
        json!({
            "technique": technique,
            "confidence": c.evidence.confidence,
        }),
    );

    // D2 evidence.occurrences — only when non-empty (deep-hashed
    // db-sourced components).
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
        push(&mut out, "evidence.occurrences", json!(items));
    }

    // Milestone 023: generic per-component annotation bag. Each
    // entry surfaces as a SPDX 2.3 `annotations[]` envelope via the
    // shared `MikebomAnnotationCommentV1` machinery. BTreeMap iteration
    // order is sorted by key — deterministic across runs.
    //
    // Milestone 127: filter out internal-only keys (the
    // `mikebom:is-workspace-root` signal that drives root-selector
    // logic but is NOT meant to surface in emitted SBOMs).
    for (key, value) in &c.extra_annotations {
        if crate::generate::root_selector::is_internal_emission_key(key) {
            continue;
        }
        push(&mut out, key, value.clone());
    }

    out
}

/// Build every document-level annotation mikebom's SPDX 2.3 output
/// emits. Follows Sections C21–C23 (document-level mikebom metadata)
/// and E1 (compositions). Always emits at least the
/// `generation-context` annotation plus four `trace-integrity-*`
/// scalars — those are constitution-mandated transparency signals
/// (Principles V and X) and CDX emits them unconditionally.
pub fn annotate_document(
    annotator: &str,
    date: &str,
    artifacts: &ScanArtifacts<'_>,
) -> Vec<SpdxAnnotation> {
    use serde_json::json;
    let mut out: Vec<SpdxAnnotation> = Vec::new();
    let push = |out: &mut Vec<SpdxAnnotation>, field: &str, value: serde_json::Value| {
        out.push(build_annotation(annotator, date, field, value));
    };

    // C21 generation-context
    let gc = match artifacts.generation_context {
        GenerationContext::FilesystemScan => "filesystem-scan",
        GenerationContext::ContainerImageScan => "container-image-scan",
        GenerationContext::BuildTimeTrace => "build-time-trace",
    };
    push(&mut out, "mikebom:generation-context", json!(gc));

    // Milestone 133 US4 (Constitution Strict Boundary §5):
    // `--file-inventory=full` opt-in marker. CDX + SPDX 3 twins.
    if let Some("full") = artifacts.file_inventory_mode {
        push(&mut out, "mikebom:file-inventory-mode", json!("full"));
    }

    // Milestone 133 US3 (C93/C94/C95): file-tier walker diagnostic
    // skip counters. Constitution Principle X — operators get
    // transparent visibility into what the orphan/full walker
    // skipped. Per-counter emission gated on `> 0` so the document
    // stays clean when nothing was skipped.
    if let Some(stats) = artifacts.file_inventory_stats {
        if stats.oversize_skipped > 0 {
            push(
                &mut out,
                "mikebom:file-inventory-skipped-oversize",
                json!(stats.oversize_skipped.to_string()),
            );
        }
        if stats.special_skipped > 0 {
            push(
                &mut out,
                "mikebom:file-inventory-skipped-special-files",
                json!(stats.special_skipped.to_string()),
            );
        }
        if stats.unreadable_skipped > 0 {
            push(
                &mut out,
                "mikebom:file-inventory-unreadable",
                json!(stats.unreadable_skipped.to_string()),
            );
        }
    }

    // C22 os-release-missing-fields — CDX emits as
    // comma-joined-with-trailing-empty shape when empty; our JSON
    // value keeps the list-of-strings shape, skipped entirely when
    // empty (skip_serializing_if-style).
    if !artifacts.os_release_missing_fields.is_empty() {
        push(
            &mut out,
            "mikebom:os-release-missing-fields",
            json!(artifacts.os_release_missing_fields),
        );
    }

    // Milestone 113 FR-014 / Constitution Principle X: user-supplied
    // directory exclusions active for this scan. See CDX twin in
    // `cyclonedx/metadata.rs`; the SPDX 2.3 envelope encodes the same
    // payload as a comma-joined string under the document-level
    // annotation comment.
    if let Some(entries) =
        crate::scan_fs::package_db::exclude_path::current_annotation()
    {
        if !entries.is_empty() {
            push(
                &mut out,
                "mikebom:exclude-path",
                json!(entries.join(",")),
            );
        }
    }

    // Milestone 119 phase-2 — document-scope supplement-cdx provenance
    // mirrors the CDX twin in `cyclonedx/metadata.rs`. Same `<path>@
    // sha256:<hex>` shape as CDX so consumers can cross-format-grep.
    if let Some(prov) = crate::supplement::current_provenance() {
        push(
            &mut out,
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
            artifacts.components,
            &artifacts.root_override,
            artifacts.scan_target_coord,
            artifacts.target_name,
            "0.0.0",
        );
        if let Some(h) = selection.heuristic {
            if !selection.losers.is_empty() {
                push(
                    &mut out,
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
    // signal. Per Constitution Principle X (Transparency): when mikebom
    // can't supply every transitive edge for `go.sum` components, the
    // SBOM MUST signal the limitation so consumers can distinguish
    // "dead dep" from "couldn't resolve." Absent annotation ⇒ signal
    // not applicable (no Go scan happened).
    if let Some(gc) = artifacts.go_graph_completeness {
        let value = serde_json::to_value(gc)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "unknown".to_string());
        push(&mut out, "mikebom:graph-completeness", json!(value));
        if let Some(reason) = artifacts.go_graph_completeness_reason {
            if !reason.is_empty() {
                push(
                    &mut out,
                    "mikebom:graph-completeness-reason",
                    json!(reason),
                );
            }
        }
    }

    // Milestone 134 (closes #125, catalog row C100): document-scope
    // `mikebom:purl-collisions-detected` summary annotation. Omitted
    // entirely when no collisions were detected so clean scans stay
    // byte-identical to alpha.51 emissions (FR-009 / SC-002). Per
    // `contracts/document-scope-annotation.md`, value is the
    // serialized `CollisionsSummary` envelope wrapped in the standard
    // `MikebomAnnotationCommentV1` envelope.
    if let Some(summary) = artifacts.collisions_summary {
        if let Ok(value) = serde_json::to_value(summary) {
            push(&mut out, "mikebom:purl-collisions-detected", value);
        }
    }

    // C23 trace-integrity-* — four scalars, emitted unconditionally
    // so consumers can distinguish "no trace ran" (0/0/[]/[]) from
    // "we didn't record it". Matches CDX's metadata-level shape.
    push_trace_integrity(&mut out, annotator, date, artifacts.integrity);

    // E1 compositions — emit when any complete-ecosystem claim is
    // present. The annotation's value is the `complete_ecosystems`
    // list (simpler than duplicating the full CDX `compositions[]`
    // shape; consumers can reconstruct the aggregate claim from
    // membership).
    if !artifacts.complete_ecosystems.is_empty() {
        push(
            &mut out,
            "compositions",
            json!({
                "complete_ecosystems": artifacts.complete_ecosystems,
            }),
        );
    }

    // C47 (milestone 073) — user-defined identifiers ride a
    // single document-level `mikebom:identifiers` annotation
    // wrapped in the `MikebomAnnotationCommentV1` envelope. Built-in
    // identifiers ride the dual-carrier standards-native path
    // (main-module `Package.externalRefs[PERSISTENT-ID]` + redundant
    // `creationInfo.creators` text line). The annotation array is
    // sorted lex by `(scheme, value)` for determinism (FR-009 /
    // contract C-4). Emit ONLY when the user-defined entry set is
    // non-empty per VR-007 — preserves cross-format byte-identity for
    // non-user-defined-namespace scans.
    let user_defined_payload: Vec<serde_json::Value> = {
        let mut entries: Vec<&mikebom::binding::identifiers::Identifier> = artifacts
            .identifiers
            .iter()
            .filter(|id| {
                matches!(
                    id.kind,
                    mikebom::binding::identifiers::IdentifierKind::UserDefined
                )
            })
            .collect();
        entries.sort_by(|a, b| {
            (a.scheme.as_str(), a.value.as_str())
                .cmp(&(b.scheme.as_str(), b.value.as_str()))
        });
        entries
            .into_iter()
            .map(|id| {
                json!({
                    "scheme": id.scheme.as_str(),
                    "value": id.value.as_str(),
                })
            })
            .collect()
    };
    if !user_defined_payload.is_empty() {
        push(
            &mut out,
            "mikebom:identifiers",
            json!(user_defined_payload),
        );
    }

    out
}

fn push_trace_integrity(
    out: &mut Vec<SpdxAnnotation>,
    annotator: &str,
    date: &str,
    integrity: &TraceIntegrity,
) {
    use serde_json::json;
    out.push(build_annotation(
        annotator,
        date,
        "mikebom:trace-integrity-ring-buffer-overflows",
        json!(integrity.ring_buffer_overflows),
    ));
    out.push(build_annotation(
        annotator,
        date,
        "mikebom:trace-integrity-events-dropped",
        json!(integrity.events_dropped),
    ));
    out.push(build_annotation(
        annotator,
        date,
        "mikebom:trace-integrity-uprobe-attach-failures",
        json!(integrity.uprobe_attach_failures),
    ));
    out.push(build_annotation(
        annotator,
        date,
        "mikebom:trace-integrity-kprobe-attach-failures",
        json!(integrity.kprobe_attach_failures),
    ));
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn envelope_serializes_schema_field_value_in_that_order() {
        let env = MikebomAnnotationCommentV1::new(
            "mikebom:sbom-tier",
            serde_json::Value::String("deployed".to_string()),
        );
        let json = env.to_comment_string();
        // Field order mirrors the committed JSON schema's `required`
        // array + property declaration order; swap detection is part
        // of the drift guard below, but we also check the basic
        // shape here.
        assert!(json.starts_with("{\"schema\":\"mikebom-annotation/v1\""));
        assert!(json.contains("\"field\":\"mikebom:sbom-tier\""));
        assert!(json.contains("\"value\":\"deployed\""));
    }

    #[test]
    fn build_annotation_wraps_envelope_as_spdx_comment() {
        let a = build_annotation(
            "Tool: mikebom-0.1.0",
            "2026-04-24T10:00:00Z",
            "mikebom:evidence-kind",
            serde_json::json!("instrumentation"),
        );
        assert_eq!(a.annotator, "Tool: mikebom-0.1.0");
        assert_eq!(a.date, "2026-04-24T10:00:00Z");
        assert!(matches!(a.kind, SpdxAnnotationType::Other));
        // The comment parses back to a v1 envelope with matching field.
        let parsed: MikebomAnnotationCommentV1 =
            serde_json::from_str(&a.comment).unwrap();
        assert_eq!(parsed.schema, ENVELOPE_SCHEMA_V1);
        assert_eq!(parsed.field, "mikebom:evidence-kind");
        assert_eq!(parsed.value, serde_json::json!("instrumentation"));
    }

    #[test]
    fn value_can_be_any_json_type() {
        // string
        assert_eq!(
            MikebomAnnotationCommentV1::new("f", serde_json::json!("x"))
                .value
                .as_str(),
            Some("x")
        );
        // number
        assert_eq!(
            MikebomAnnotationCommentV1::new("f", serde_json::json!(0.92))
                .value
                .as_f64(),
            Some(0.92)
        );
        // array
        let arr = MikebomAnnotationCommentV1::new(
            "f",
            serde_json::json!(["a", "b"]),
        );
        assert_eq!(arr.value.as_array().map(|v| v.len()), Some(2));
        // object
        let obj = MikebomAnnotationCommentV1::new(
            "f",
            serde_json::json!({"technique": "hash-comparison", "confidence": 1.0}),
        );
        assert!(obj.value.as_object().is_some());
    }

    /// Structural drift guard: the Rust envelope and the committed
    /// JSON schema at `contracts/mikebom-annotation.schema.json`
    /// MUST stay in sync on the three things a consumer writes code
    /// against: the fixed `schema` constant, the set of required
    /// fields, and the `additionalProperties: false` constraint.
    #[test]
    fn envelope_matches_committed_json_schema() {
        let schema_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .join(
                "specs/010-spdx-output-support/contracts/\
                 mikebom-annotation.schema.json",
            );
        let raw = std::fs::read_to_string(&schema_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", schema_path.display()));
        let schema: serde_json::Value = serde_json::from_str(&raw).unwrap();

        let schema_const = schema["properties"]["schema"]["const"]
            .as_str()
            .expect("schema.properties.schema.const is a string");
        assert_eq!(
            schema_const, ENVELOPE_SCHEMA_V1,
            "Rust envelope constant drifted from committed JSON schema"
        );

        let required: Vec<&str> = schema["required"]
            .as_array()
            .expect("required array")
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        let mut expected = vec!["schema", "field", "value"];
        expected.sort();
        let mut got = required.clone();
        got.sort();
        assert_eq!(got, expected, "required-fields set drift");

        assert_eq!(
            schema["additionalProperties"].as_bool(),
            Some(false),
            "schema must forbid additional properties so consumers can \
             duck-type on the three known fields"
        );
    }

    // -----------------------------------------------------------
    // Issue #228 — `mikebom:lifecycle-scope` annotation emission
    // -----------------------------------------------------------

    fn mk_minimal_component(
        purl: &str,
        scope: Option<mikebom_common::resolution::LifecycleScope>,
    ) -> ResolvedComponent {
        use mikebom_common::resolution::{ResolutionEvidence, ResolutionTechnique};
        use mikebom_common::types::purl::Purl;
        ResolvedComponent {
            build_inclusion: None,
            purl: Purl::new(purl).unwrap(),
            name: "demo".to_string(),
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
            advisories: vec![],
            occurrences: vec![],
            lifecycle_scope: scope,
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

    fn parse_envelope(a: &SpdxAnnotation) -> MikebomAnnotationCommentV1 {
        serde_json::from_str(&a.comment).expect("annotation comment is valid v1 envelope")
    }

    #[test]
    fn lifecycle_scope_annotation_emitted_for_test_scope() {
        // Issue #228 — a Test-scoped Package must carry a
        // `mikebom:lifecycle-scope: "test"` annotation regardless of
        // edge-style. This is the parity-bridging signal CDX
        // consumers expect on the target component, and the only
        // way a flat-DEPENDS_ON SPDX 2.3 consumer can distinguish
        // dev / build / test deps from deployed-runtime deps.
        let c = mk_minimal_component(
            "pkg:golang/example.com/testify@v1",
            Some(mikebom_common::resolution::LifecycleScope::Test),
        );
        let annos = annotate_component("Tool: mikebom-test", "2026-05-23T00:00:00Z", &c, false, false);
        let scope_annos: Vec<_> = annos
            .iter()
            .filter(|a| parse_envelope(a).field == "mikebom:lifecycle-scope")
            .collect();
        assert_eq!(scope_annos.len(), 1, "exactly one lifecycle-scope annotation");
        assert_eq!(parse_envelope(scope_annos[0]).value, serde_json::json!("test"));
    }

    #[test]
    fn lifecycle_scope_annotation_emitted_for_dev_and_build() {
        // Coverage for the other two non-runtime variants.
        for (scope, expected) in [
            (mikebom_common::resolution::LifecycleScope::Development, "development"),
            (mikebom_common::resolution::LifecycleScope::Build, "build"),
        ] {
            let c = mk_minimal_component("pkg:cargo/x@1", Some(scope));
            let annos = annotate_component("Tool: mikebom-test", "2026-05-23T00:00:00Z", &c, false, false);
            let found = annos
                .iter()
                .any(|a| {
                    let env = parse_envelope(a);
                    env.field == "mikebom:lifecycle-scope" && env.value == serde_json::json!(expected)
                });
            assert!(found, "expected lifecycle-scope={expected} annotation for scope={scope:?}");
        }
    }

    #[test]
    fn lifecycle_scope_annotation_omitted_for_runtime_and_none() {
        // Runtime is the default; emitting an annotation would be
        // noise. Same for components that never had scope assigned
        // (e.g., OS package readers).
        for scope in [
            Some(mikebom_common::resolution::LifecycleScope::Runtime),
            None,
        ] {
            let c = mk_minimal_component("pkg:deb/debian/libc6@2.36", scope);
            let annos = annotate_component("Tool: mikebom-test", "2026-05-23T00:00:00Z", &c, false, false);
            let leaked = annos
                .iter()
                .any(|a| parse_envelope(a).field == "mikebom:lifecycle-scope");
            assert!(!leaked, "no lifecycle-scope annotation for scope={scope:?}; got {annos:#?}");
        }
    }
}
