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

use waybill_common::attestation::integrity::TraceIntegrity;
use waybill_common::attestation::metadata::GenerationContext;
use waybill_common::resolution::{ResolutionTechnique, ResolvedComponent};

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

/// Coerce a raw `extra_annotations` SCALAR value into the canonical
/// form the envelope's `value` field carries when CDX would have
/// stringified it. Applies to `Value::Bool` + `Value::Number` +
/// `Value::Null` only. Strings pass through unchanged. Arrays and
/// objects pass through with their structure preserved.
///
/// **Cross-format parity contract (scalar coercion only)**:
/// CycloneDX 1.6's `property.value` is spec-typed as a string
/// (`<simpleType name="propertyValue">` → `xs:string` in the schema),
/// so the CDX emitter at
/// `mikebom-cli/src/generate/cyclonedx/builder.rs:1092` calls
/// `serde_json::to_string(other)` on non-String JSON values, coercing
/// `Value::Bool(true)` → the 4-char string `"true"`, `Value::Number(N)`
/// → its JSON-numeric form, etc. This SPDX-side helper mirrors that
/// stringification ONLY for scalars so external parity audits that
/// don't run mikebom's compare-time canonicalizer
/// (`parity/extractors/common.rs::canonicalize_atomic_values`) see the
/// same string representation across all three format outputs.
///
/// **Why arrays + objects pass through structured**: when CDX
/// stringifies an array like `mikebom:identifiers`, it produces a
/// JSON-literal string `"[{\"scheme\":...}]"`. The SPDX 2.3 + SPDX 3
/// envelopes are not spec-constrained to strings the way CDX
/// property.value is; their `value` field is free-form JSON. Internal
/// consumers (`identifiers_determinism.rs::extract_spdx23_identifiers`,
/// the parity extractors at `parity/extractors/{spdx2,spdx3}.rs`) and
/// external consumers walking the structured array directly depend on
/// the array shape being preserved. Stringifying arrays here would
/// break that machine-readable contract for no audit benefit (the
/// audit's complaint surfaced specifically on boolean annotations:
/// `mikebom:detected-cargo-auditable`, `mikebom:not-linked`).
///
/// Caught originally by the sbom-conformance harness 2026-06 audit;
/// the array-preservation carve-out caught by
/// `identifiers_determinism::cross_format_consistency_same_identifier_set`
/// during this fix's first-pass test run.
pub fn coerce_envelope_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        // Scalars CDX stringifies — mirror that.
        serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::Null => serde_json::Value::String(
            serde_json::to_string(&value).unwrap_or_default(),
        ),
        // Strings + structured (Array/Object): pass through unchanged.
        _ => value,
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
    let envelope = MikebomAnnotationCommentV1::new(field, coerce_envelope_value(value));
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
    compiler_pipeline: Option<
        &waybill_common::attestation::compiler_pipeline::CompilerPipelineData,
    >,
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
        use waybill_common::resolution::LifecycleScope;
        let scope_str = match scope {
            LifecycleScope::Development => Some("development"),
            LifecycleScope::Build => Some("build"),
            LifecycleScope::Test => Some("test"),
            LifecycleScope::Optional => Some("optional"), // milestone 179
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
    // C20 requirement-ranges (milestone 199 — always-array shape;
    // supersedes the m191 singular `mikebom:requirement-range` scalar).
    if !c.requirement_ranges.is_empty() {
        push(&mut out, "mikebom:requirement-ranges", json!(c.requirement_ranges));
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
        if crate::generate::root_selector::is_internal_emission_key(key)
            || crate::generate::root_selector::is_field_owned_annotation_key(key)
        {
            // Milestone 145 US3 (FR-009): skip keys already emitted
            // from a field-derived source (e.g., `mikebom:source-files`
            // comes from `c.evidence.source_file_paths` at line ~302
            // above) — re-emitting from the bag double-stamps.
            continue;
        }
        push(&mut out, key, value.clone());
    }

    // Milestone 210 — per-component compiler-pipeline attribution.
    // C130 (`mikebom:source-read-set`) + C131 (`mikebom:read-set-source`)
    // + C134 (`mikebom:trace-attach-late`) per contracts/annotations.md
    // A-1/A-2/A-5. Only emitted when the scan ran with `mikebom trace`
    // (eBPF) AND at least one compiler invocation was captured.
    // Matching = component's file paths intersect at least one
    // invocation's write-set. `Traced` ⇒ C130 + C131. `Unknown` ⇒
    // C131 only. C134 fires when the doc-scope completeness is
    // `Partial(AttachLate)` — the trace attached mid-build so every
    // captured component potentially has a partial read-set. No
    // pipeline ⇒ none — preserves byte-identity for scan-mode.
    if let Some(pipeline) = compiler_pipeline {
        let mapping = crate::generate::compiler_pipeline_annotation::map_component_to_source_read_set(
            c,
            pipeline,
        );
        if let Some(payload) = mapping.payload {
            push(&mut out, "mikebom:source-read-set", payload);
        }
        push(
            &mut out,
            "mikebom:read-set-source",
            json!(mapping.source.as_wire_str()),
        );
        if matches!(
            pipeline.completeness,
            waybill_common::attestation::compiler_pipeline::CompletenessState::Partial {
                reason: waybill_common::attestation::compiler_pipeline::PartialReason::AttachLate,
            }
        ) {
            push(&mut out, "mikebom:trace-attach-late", json!("true"));
        }
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
    graph_completeness: &crate::generate::graph_completeness::GraphCompletenessResult,
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

    // Milestone 158 US2 — always-emit `mikebom:graph-completeness`
    // at document scope per FR-003 (universal presence). Value is
    // the three-way `complete|partial|unknown` string. The
    // companion `mikebom:graph-completeness-reason` is emitted
    // conditionally per FR-004/FR-005.
    push(
        &mut out,
        "mikebom:graph-completeness",
        json!(graph_completeness.value.as_str()),
    );
    if graph_completeness.value
        != crate::generate::graph_completeness::GraphCompletenessValue::Complete
        && !graph_completeness.reason_codes.is_empty()
    {
        push(
            &mut out,
            "mikebom:graph-completeness-reason",
            json!(crate::generate::graph_completeness::join_reason_codes(
                &graph_completeness.reason_codes,
            )),
        );
    }

    // Milestone 160 (T034/T035): doc-scope Go-transitive coverage
    // annotations (C110/C111). C110 emitted iff the scan had ≥1 Go
    // component (`go_transitive_coverage` is `Some`). C111 conditionally
    // emitted iff coverage != Complete. Per FR-004/FR-005 + Q1
    // reason-code-driven decision rule.
    // Milestone 173: C119 doc-scope `mikebom:go-cache-warming-failed`.
    // Emitted BEFORE C118 for alphabetic sort. Gated on Go presence
    // AND at least one failing workspace. JSON-encoded array value.
    if let Some(cw) = artifacts.go_cache_warming {
        if !cw.failures.is_empty() {
            let value = serde_json::to_string(&cw.failures).unwrap_or_default();
            push(
                &mut out,
                "mikebom:go-cache-warming-failed",
                json!(value),
            );
        }
    }

    // Milestone 176: C121 doc-scope `mikebom:workspaces-detected`.
    // Value is a JSON-encoded array of the sorted-deduplicated union
    // of every per-component `mikebom:workspace-member` value (FR-003
    // + FR-012). Emission gated on the union being non-empty (FR-003:
    // absent when zero workspaces detected). Computed via the shared
    // helper so all three formats guarantee the FR-012 cross-
    // annotation invariant by construction.
    {
        let workspaces =
            crate::generate::workspace_detected::compute(artifacts.components);
        if !workspaces.is_empty() {
            let value = serde_json::to_string(&workspaces).unwrap_or_default();
            push(
                &mut out,
                "mikebom:workspaces-detected",
                json!(value),
            );
        }
    }

    // Milestone 173: C118 doc-scope `mikebom:go-cache-warming-mode`.
    // Emitted BEFORE C110 for alphabetic sort. Value one of `"off"` /
    // `"per-workspace"` / `"offline-inhibited"`.
    if let Some(cw) = artifacts.go_cache_warming {
        push(
            &mut out,
            "mikebom:go-cache-warming-mode",
            json!(cw.mode.as_wire_str()),
        );
    }

    if let Some(coverage) = artifacts.go_transitive_coverage {
        push(
            &mut out,
            "mikebom:go-transitive-coverage",
            json!(coverage.value_wire_str()),
        );
        if let Some(reason) = coverage.reason() {
            push(
                &mut out,
                "mikebom:go-transitive-coverage-reason",
                json!(reason),
            );
        }
    }

    // Milestone 172: doc-scope C117 `mikebom:go-transitive-fallback-
    // count` annotation. Emitted iff `go_transitive_fallback_count` is
    // `Some(_)` (Go was scanned). Value `"0"` explicit on healthy scans
    // per Q1 clarification. Companion to C110.
    if let Some(count) = artifacts.go_transitive_fallback_count {
        push(
            &mut out,
            "mikebom:go-transitive-fallback-count",
            json!(count.to_string()),
        );
    }

    // Milestone 161 (T044): doc-scope Go-workspace-mode annotation
    // (C112). Emitted iff `go.work` file was present at the scanned
    // root (`Detected` or `Malformed` variant); `Absent` is treated
    // as unpopulated to preserve SC-003 byte-identity on non-workspace
    // scans.
    if let Some(mode) = artifacts.go_workspace_mode {
        use crate::scan_fs::package_db::golang::gowork::WorkspaceMode;
        if !matches!(mode, WorkspaceMode::Absent) {
            push(
                &mut out,
                "mikebom:go-workspace-mode",
                json!(mode.as_wire_str()),
            );
        }
    }

    // Milestone 204 (#554): C123 doc-scope helm image-extraction
    // completeness annotation. Emitted iff helm reader ran. Wire
    // value derived from HelmExtractionMode::as_wire_str().
    if let Some(mode) = artifacts.helm_extraction_mode {
        push(
            &mut out,
            "mikebom:image-extraction-completeness",
            json!(mode.as_wire_str()),
        );
    }

    // Milestone 206 (#440): C124 doc-scope image-source annotation.
    // Emitted iff artifacts.image_source == Some(Podman) — conditional
    // emission preserves FR-005 byte-identity for docker/remote/path
    // scans.
    if matches!(
        artifacts.image_source,
        Some(crate::cli::scan_cmd::ImageSource::Podman)
    ) {
        push(&mut out, "mikebom:image-source", json!("podman"));
    }

    // Milestone 210: document-scope compiler-pipeline transparency
    // (C132 + C133) per contracts/annotations.md A-3 / A-4. C132
    // is unconditional (always some `{state: ...}` value) — its
    // Principle-X transparency purpose is defeated by skip-on-complete
    // semantics. C133 fires only when the filtered count is non-zero.
    // Both silent when `compiler_pipeline == None` (byte-identity
    // preserved for scan-mode).
    if let Some(pipeline) = artifacts.compiler_pipeline {
        if let Ok(value) = serde_json::to_value(&pipeline.completeness) {
            push(&mut out, "mikebom:compiler-pipeline-completeness", value);
        }
        if pipeline.secrets_read_filtered > 0 {
            push(
                &mut out,
                "mikebom:secrets-read-filtered",
                json!(pipeline.secrets_read_filtered.to_string()),
            );
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
        let mut entries: Vec<&waybill::binding::identifiers::Identifier> = artifacts
            .identifiers
            .iter()
            .filter(|id| {
                matches!(
                    id.kind,
                    waybill::binding::identifiers::IdentifierKind::UserDefined
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

    // -- coerce_envelope_value: cross-format parity regression guards --
    //
    // These tests pin the contract that `build_annotation` always
    // produces an envelope whose `value` field is a `Value::String`,
    // mirroring CDX's `serde_json::to_string(other)` coercion at
    // `mikebom-cli/src/generate/cyclonedx/builder.rs:1092`. External
    // parity audits (sbom-conformance harness, 2026-06) caught the
    // pre-coercion mismatch on `mikebom:detected-cargo-auditable`
    // (`Value::Bool(true)` → CDX `"true"` string vs SPDX `true` bool).

    #[test]
    fn coerce_envelope_value_passes_strings_through() {
        let v = coerce_envelope_value(serde_json::json!("instrumentation"));
        assert_eq!(v, serde_json::Value::String("instrumentation".to_string()));
    }

    #[test]
    fn coerce_envelope_value_stringifies_bool_true() {
        let v = coerce_envelope_value(serde_json::json!(true));
        assert_eq!(v, serde_json::Value::String("true".to_string()));
    }

    #[test]
    fn coerce_envelope_value_stringifies_bool_false() {
        let v = coerce_envelope_value(serde_json::json!(false));
        assert_eq!(v, serde_json::Value::String("false".to_string()));
    }

    #[test]
    fn coerce_envelope_value_stringifies_number() {
        let v = coerce_envelope_value(serde_json::json!(42));
        assert_eq!(v, serde_json::Value::String("42".to_string()));
        let v = coerce_envelope_value(serde_json::json!(0.92));
        assert_eq!(v, serde_json::Value::String("0.92".to_string()));
    }

    #[test]
    fn coerce_envelope_value_stringifies_null() {
        let v = coerce_envelope_value(serde_json::Value::Null);
        assert_eq!(v, serde_json::Value::String("null".to_string()));
    }

    #[test]
    fn coerce_envelope_value_preserves_arrays_and_objects() {
        // Arrays + objects pass through structured. SPDX envelopes are
        // free-form JSON (unlike CDX property.value which is spec-typed
        // as a string), so consumers walking the structured array
        // shape — e.g., mikebom:identifiers — must see the original
        // Value::Array. Stringifying these would break the
        // identifiers_determinism cross-format extractor and any
        // external consumer depending on machine-readable array
        // structure.
        let arr_in = serde_json::json!(["a", "b"]);
        let arr_out = coerce_envelope_value(arr_in.clone());
        assert_eq!(arr_out, arr_in, "arrays must pass through structured");
        let obj_in = serde_json::json!({"scheme": "git", "value": "abc"});
        let obj_out = coerce_envelope_value(obj_in.clone());
        assert_eq!(obj_out, obj_in, "objects must pass through structured");
    }

    #[test]
    fn build_annotation_with_bool_value_produces_string_envelope_value() {
        // Regression for the wire-format divergence the sbom-conformance
        // harness caught: pre-fix, `Value::Bool(true)` flowed through
        // build_annotation unchanged and the envelope serialized as
        // `"value":true` (JSON boolean), while CDX emitted the same
        // datum as the string `"true"`. Post-fix, both formats carry
        // the string `"true"`.
        let a = build_annotation(
            "Tool: mikebom-test",
            "2026-06-26T12:00:00Z",
            "mikebom:not-linked",
            serde_json::json!(true),
        );
        let parsed: MikebomAnnotationCommentV1 =
            serde_json::from_str(&a.comment).unwrap();
        assert_eq!(
            parsed.value,
            serde_json::Value::String("true".to_string()),
            "envelope value must be a String, not a Bool — cross-format parity contract",
        );
        // Belt-and-suspenders: the serialized comment string must contain
        // the literal `"value":"true"` substring (quotes around the
        // value), not `"value":true` (bare bool).
        assert!(
            a.comment.contains(r#""value":"true""#),
            "comment must serialize bool as string-true: {}",
            a.comment,
        );
        assert!(
            !a.comment.contains(r#""value":true"#),
            "comment must NOT serialize bool as bare true: {}",
            a.comment,
        );
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
        scope: Option<waybill_common::resolution::LifecycleScope>,
    ) -> ResolvedComponent {
        use waybill_common::resolution::{ResolutionEvidence, ResolutionTechnique};
        use waybill_common::types::purl::Purl;
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
            Some(waybill_common::resolution::LifecycleScope::Test),
        );
        let annos = annotate_component("Tool: mikebom-test", "2026-05-23T00:00:00Z", &c, false, false, None);
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
            (waybill_common::resolution::LifecycleScope::Development, "development"),
            (waybill_common::resolution::LifecycleScope::Build, "build"),
        ] {
            let c = mk_minimal_component("pkg:cargo/x@1", Some(scope));
            let annos = annotate_component("Tool: mikebom-test", "2026-05-23T00:00:00Z", &c, false, false, None);
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
            Some(waybill_common::resolution::LifecycleScope::Runtime),
            None,
        ] {
            let c = mk_minimal_component("pkg:deb/debian/libc6@2.36", scope);
            let annos = annotate_component("Tool: mikebom-test", "2026-05-23T00:00:00Z", &c, false, false, None);
            let leaked = annos
                .iter()
                .any(|a| parse_envelope(a).field == "mikebom:lifecycle-scope");
            assert!(!leaked, "no lifecycle-scope annotation for scope={scope:?}; got {annos:#?}");
        }
    }
}
