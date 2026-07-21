
use chrono::Utc;
use waybill_common::attestation::integrity::TraceIntegrity;
use waybill_common::attestation::metadata::GenerationContext;
use waybill_common::resolution::ResolvedComponent;
use waybill_common::types::license::SpdxExpression;
use waybill_common::types::purl::encode_purl_segment;
use serde_json::json;

use crate::generate::RootComponentOverride;

/// Normalize a string for inclusion in a CPE 2.3 segment.
///
/// CPE 2.3 well-formed name segments (per NIST) are lowercase and use
/// `_` for separators; other characters are typically escaped with a
/// backslash. For our synthetic scan-subject CPE we only need a
/// minimally-valid form: lowercase, ASCII alphanumerics + `_` / `-` /
/// `.` preserved, everything else → `_`.
fn cpe_sanitize(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for c in raw.chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push('_');
    }
    out
}

/// Build the CycloneDX `metadata` section.
///
/// Includes:
/// - Tool identity (waybill with current version)
/// - Generation timestamp
/// - Component reference (the build target)
/// - Properties indicating generation context
/// - `lifecycles[]`: aggregated union of tier values observed across
///   the components, per milestone 002's traceability ladder (R13).
#[allow(clippy::too_many_arguments)]
pub fn build_metadata(
    target_name: &str,
    target_version: &str,
    context: GenerationContext,
    components: &[ResolvedComponent],
    os_release_missing_fields: &[String],
    integrity: &TraceIntegrity,
    scan_target_coord: Option<&crate::scan_fs::package_db::maven::ScanTargetCoord>,
    source_document_binding: Option<&waybill::binding::SourceDocumentId>,
    identifiers: &[waybill::binding::identifiers::Identifier],
    root_override: &RootComponentOverride,
    user_metadata: &waybill::binding::user_metadata::UserMetadata,
    // Milestone 081 — when the operator passed `--sbom-type
    // <type>`, this Some(_) value drives the lifecycle aggregation
    // toward a single-element array containing the asserted CDX
    // phase regardless of per-component tier values. None preserves
    // the milestone-047 auto-aggregation.
    sbom_type_override: Option<crate::generate::lifecycle_phases::SbomType>,
    // Milestone 133 US3 — file-tier walker diagnostic counters. None
    // when `--file-inventory=off`; Some(_) for orphan/full modes.
    file_inventory_stats: Option<&crate::scan_fs::file_tier::walker::WalkerStats>,
    // Milestone 133 US4 — operator-supplied `--file-inventory` mode
    // label. Only `Some("full")` triggers the document-level
    // override marker per Strict Boundary §5.
    file_inventory_mode: Option<&str>,
    // Milestone 134 (closes #125) — document-scope aggregate of every
    // divergent-PURL collision detected in the scan. `None` ⇒ no
    // collisions ⇒ omit the annotation entirely (FR-009).
    collisions_summary: Option<&waybill_common::divergence::CollisionsSummary>,
    // Milestone 158 (US2) — the multi-root BFS reachability pass
    // result. Drives the two document-scope annotations
    // `waybill:graph-completeness` + `waybill:graph-completeness-reason`
    // per FR-003 + FR-004. Threaded from `builder::build()` after the
    // graph is fully assembled (post workspace-peer linkage).
    graph_completeness: &crate::generate::graph_completeness::GraphCompletenessResult,
    // Milestone 160 (T034/T035) — doc-scope Go-transitive coverage
    // signal. Drives the C110/C111 document-scope annotations per
    // FR-004/FR-005. `None` ⇒ no Go scan happened (annotations absent).
    go_transitive_coverage: Option<
        &crate::scan_fs::package_db::golang::graph_resolver::GoTransitiveCoverage,
    >,
    // Milestone 161 (T042) — doc-scope Go-workspace-mode signal.
    // Drives the C112 document-scope annotation per FR-004. `None`
    // ⇒ no `go.work` at scanned root (annotation absent per SC-003).
    go_workspace_mode: Option<
        &crate::scan_fs::package_db::golang::gowork::WorkspaceMode,
    >,
    // Milestone 172 — doc-scope Go step-5 fallback count. Drives the
    // C117 `waybill:go-transitive-fallback-count` annotation per FR-002
    // + Q1 clarification. `None` iff no Go scan happened (annotation
    // absent); `Some(N)` else — including `Some(0)` on healthy scans,
    // where the annotation is emitted with value `"0"` explicitly.
    go_transitive_fallback_count: Option<usize>,
    // Milestone 173 — doc-scope Go cache-warming outcome. Drives the
    // C118 (`waybill:go-cache-warming-mode`) unconditional annotation
    // AND the C119 (`waybill:go-cache-warming-failed`) conditional
    // annotation. `None` iff no Go scan happened. When `Some(_)`, the
    // mode is emitted verbatim; failures (if non-empty) emit C119
    // with a JSON-encoded array value per contracts/annotation-wire-
    // shapes.md.
    go_cache_warming: Option<
        &crate::scan_fs::package_db::golang::CacheWarmingResult,
    >,
    // Milestone 204 (#554) — doc-scope Helm image-extraction-mode
    // signal. Drives the C123 `waybill:image-extraction-completeness`
    // annotation. `None` iff no helm reader ran (annotation absent
    // per FR-004 / SC-004 byte-identity for non-Helm scans).
    // `Some(Unrendered)` → `"partial"`. `Some(Rendered)` → `"full"`.
    helm_extraction_mode: Option<
        &crate::scan_fs::package_db::HelmExtractionMode,
    >,
    // Milestone 206 (#440) — doc-scope image-source signal for the
    // C124 `waybill:image-source` annotation. Conditional emission
    // (podman-only in MVP) preserves FR-005 byte-identity for
    // docker/remote/path scans.
    image_source: Option<&crate::cli::scan_cmd::ImageSource>,
    // Milestone 210 — compiler-pipeline data from the eBPF trace.
    // Drives C132 (`waybill:compiler-pipeline-completeness`, always
    // emitted when `Some(_)`) + C133 (`waybill:secrets-read-filtered`,
    // emitted only when `secrets_read_filtered > 0`). `None` ⇒ neither
    // annotation emitted (byte-identity preserved for scan-mode).
    compiler_pipeline: Option<
        &waybill_common::attestation::compiler_pipeline::CompilerPipelineData,
    >,
) -> serde_json::Value {
    let version = env!("CARGO_PKG_VERSION");
    // Determinism: honor `WAYBILL_FIXED_TIMESTAMP` (same env-var
    // contract as `scan_cmd::scan_created_timestamp`) so two
    // back-to-back scans inside a test produce byte-identical
    // metadata.timestamp + annotations[].timestamp values. Without
    // this, two scans straddling a 1-second boundary produced
    // differing `bom.annotations[].timestamp` strings (the normalizer
    // masks `metadata.timestamp` but not per-annotation timestamps).
    // Pre-existing latent bug from milestone 080 — surfaced as a
    // Linux-CI flake on milestone 092's PR. An unparseable env value
    // is treated as unset (defensive belt-and-braces, matching
    // scan_created_timestamp).
    let timestamp = {
        let resolved = std::env::var("WAYBILL_FIXED_TIMESTAMP")
            .ok()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);
        resolved.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    };

    // Serialize the enum via serde to reuse the existing kebab-case rename
    // attributes. Dropping quotes so the property value is a bare string.
    let context_str = serde_json::to_value(&context)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "unknown".to_string());

    // Aggregate lifecycle phases from the observed component tiers.
    // Source-of-truth lives in `crate::generate::lifecycle_phases`
    // so the SPDX serializers' document-level scope comment uses the
    // same phase set.
    let lifecycles: Vec<serde_json::Value> =
        crate::generate::lifecycle_phases::aggregate_phases(
            components,
            sbom_type_override,
        )
        .into_iter()
        .map(|p| json!({"phase": p}))
        .collect();

    let mut properties = vec![json!({
        "name": "waybill:generation-context",
        "value": context_str,
    })];

    // Milestone 133 US4 (Constitution Strict Boundary §5): when
    // the operator opted into `--file-inventory=full` (the explicit
    // dedupe-bypass override), the document MUST carry a marker so
    // consumers can detect at parse time that the file-tier set may
    // duplicate package/binary tier coverage. `orphan` (default)
    // and `off` modes do NOT emit the marker (preserves byte-
    // identity on every default-mode SBOM).
    if let Some("full") = file_inventory_mode {
        properties.push(json!({
            "name": "waybill:file-inventory-mode",
            "value": "full",
        }));
    }

    // Milestone 133 US3 (Constitution Principle X): file-tier walker
    // diagnostic skip counters surface as document-level properties
    // so operators have transparent visibility into what the walker
    // skipped (oversized files, special files, unreadable files).
    // Emitted ONLY when the walker ran AND the counter is non-zero.
    if let Some(stats) = file_inventory_stats {
        if stats.oversize_skipped > 0 {
            properties.push(json!({
                "name": "waybill:file-inventory-skipped-oversize",
                "value": stats.oversize_skipped.to_string(),
            }));
        }
        if stats.special_skipped > 0 {
            properties.push(json!({
                "name": "waybill:file-inventory-skipped-special-files",
                "value": stats.special_skipped.to_string(),
            }));
        }
        if stats.unreadable_skipped > 0 {
            properties.push(json!({
                "name": "waybill:file-inventory-unreadable",
                "value": stats.unreadable_skipped.to_string(),
            }));
        }
    }

    // Feature 005 SC-009 / FR-006 / FR-009: when /etc/os-release fields
    // were missing during scan, record the names here so SBOM consumers
    // can detect degraded PURL output without parsing the scanner log.
    // Omitted entirely when the list is empty (clean scan).
    if !os_release_missing_fields.is_empty() {
        properties.push(json!({
            "name": "waybill:os-release-missing-fields",
            "value": os_release_missing_fields.join(","),
        }));
    }

    // Milestone 113 FR-014 / Constitution Principle X: when the scan
    // applied user-supplied directory exclusions, list the entries
    // verbatim in source order. Absent annotation ⇒ no exclusions in
    // effect (default, byte-identical to pre-feature emission).
    if let Some(entries) =
        crate::scan_fs::package_db::exclude_path::current_annotation()
    {
        if !entries.is_empty() {
            properties.push(json!({
                "name": "waybill:exclude-path",
                "value": entries.join(","),
            }));
        }
    }

    // Milestone 119 (#326) FR-012 / Decision 6: when the scan was
    // invoked with `--supplement-cdx <PATH>`, record the supplement
    // file's verbatim path + sha256 hash on the envelope so consumers
    // can verify which supplement file fed the merge. Absence
    // preserves byte-identity with pre-119 waybill output per FR-013.
    if let Some(prov) = crate::supplement::current_provenance() {
        properties.push(json!({
            "name": "waybill:supplement-cdx",
            "value": crate::supplement::annotation::build_supplement_cdx_provenance_string(
                &prov.source_path,
                &prov.source_sha256,
            ),
        }));
    }

    // Milestone 134 (closes #125, catalog row C100): document-scope
    // `waybill:purl-collisions-detected` summary annotation. Omitted
    // entirely when `collisions_summary.is_none()` so clean scans
    // stay byte-identical to alpha.51 emissions (FR-009 / SC-002).
    // Per `contracts/document-scope-annotation.md`, `value` is a
    // JSON-encoded string of the `CollisionsSummary` envelope.
    if let Some(summary) = collisions_summary {
        properties.push(json!({
            "name": "waybill:purl-collisions-detected",
            "value": serde_json::to_string(summary)
                .unwrap_or_default(),
        }));
    }

    // Trace-integrity counters (previously on compositions, moved
    // here for CDX 1.6 schema conformance — compositions items have
    // additionalProperties: false so `properties` isn't allowed there).
    // Each counter is surfaced as a distinct property so downstream
    // consumers can filter on name.
    properties.push(json!({
        "name": "waybill:trace-integrity-ring-buffer-overflows",
        "value": integrity.ring_buffer_overflows.to_string(),
    }));
    properties.push(json!({
        "name": "waybill:trace-integrity-events-dropped",
        "value": integrity.events_dropped.to_string(),
    }));
    properties.push(json!({
        "name": "waybill:trace-integrity-uprobe-attach-failures",
        "value": integrity.uprobe_attach_failures.len().to_string(),
    }));
    properties.push(json!({
        "name": "waybill:trace-integrity-kprobe-attach-failures",
        "value": integrity.kprobe_attach_failures.len().to_string(),
    }));

    // Milestone 210 — document-scope compiler-pipeline transparency
    // (C132 + C133) per contracts/annotations.md A-3 / A-4. Only fires
    // when `waybill trace` captured pipeline data. C132 is
    // unconditional (always some `{state: "complete|degraded|partial"}`
    // value) — its transparency purpose is defeated by skip-on-complete
    // semantics. C133 is emitted only when the filtered count is
    // non-zero (silent = zero, per FR-016a).
    if let Some(pipeline) = compiler_pipeline {
        let completeness_value = serde_json::to_string(&pipeline.completeness)
            .unwrap_or_else(|_| "{\"state\":\"complete\"}".to_string());
        properties.push(json!({
            "name": "waybill:compiler-pipeline-completeness",
            "value": completeness_value,
        }));
        if pipeline.secrets_read_filtered > 0 {
            properties.push(json!({
                "name": "waybill:secrets-read-filtered",
                "value": pipeline.secrets_read_filtered.to_string(),
            }));
        }
    }

    // Synthesize a `pkg:generic/<target>@<version>` purl for the scan
    // subject. sbomqs's schema validator reports the metadata.component
    // as invalid when it lacks a purl; the spec itself doesn't require
    // one on application components, but the synthetic purl is cheap
    // and unambiguous (the scan-subject's identity is already the
    // `name@version` pair). Improves sbomqs's Structural score +2.0%.
    //
    // Priority ladder for the metadata.component subject (most-
    // precise wins):
    //   1. Milestones 053 (Go) + 064 (cargo): any main-module component
    //      is present (any ResolvedComponent carrying
    //      `waybill:component-role: "main-module"` in its extra
    //      annotations) — use its real `pkg:<ecosystem>/...@<ver>`
    //      PURL. Per FR-001a this is the standards-native CDX
    //      placement (Trivy's pattern). The predicate is C40-tag-
    //      driven, so any future ecosystem (issue #104: npm, pip,
    //      maven, gem) inherits this slot automatically once it
    //      emits a main-module entry. When multiple main-modules
    //      exist (cargo workspace, polyglot scans), the FIRST one
    //      sorted by walker order is selected here — but the
    //      polyglot super-root path in `document.rs` / `builder.rs`
    //      is what consumers should rely on for the multi-root case.
    //   2. M3 — Maven scan-target-coord identified by the JAR walker
    //      (either target-name match or fat-jar heuristic): use the
    //      `pkg:maven/<g>/<a>@<v>` coord — far more useful than the
    //      generic placeholder for Maven Central advisory mapping.
    //   3. Default — `pkg:generic/<target>@<version>` placeholder for
    //      non-main-module-bearing scan subjects.
    // Count all main-modules in the scan. CDX `metadata.component` is
    // singular, so it can only host ONE component. If exactly one
    // main-module exists (single-crate scans, single-go.mod scans),
    // promote it to `metadata.component`. If multiple main-modules
    // exist (cargo workspace with N members per milestone 064; rare
    // go.work multi-module), fall through to the synthetic-placeholder
    // path so all N main-modules can emit naturally as siblings in
    // `components[]` and be co-targeted by `documentDescribes` /
    // `dependsOn` from the placeholder. This is the simplest correct
    // CDX shape for the workspace-multi-member case; Trivy's "Root:
    // true" pattern doesn't generalize cleanly when there's no single
    // root crate to elect.
    // Milestone 127 — delegate BOM-subject selection to the central
    // ladder in `generate::root_selector::select_root`. The ladder
    // handles override > count==1 fast path > FR-002 repo-root >
    // FR-003 ecosystem-priority > FR-004 LCP > Maven coord > synthetic
    // placeholder, all in one call. Returns the elected subject + the
    // heuristic name (None on fast-path / override) + losers (for
    // FR-007 warning emission downstream).
    //
    // The match below maps `ResolvedRootSubject` back into the
    // `(main_module, subject_name, subject_version, synthetic_component_purl)`
    // tuple downstream consumers (CPE synthesizer at line ~327,
    // metadata.component builder at line ~404) read from.
    let selection = crate::generate::root_selector::select_root(
        components,
        root_override,
        scan_target_coord,
        target_name,
        target_version,
    );
    let override_active = root_override.is_active();
    let (main_module, subject_name, subject_version, synthetic_component_purl): (
        Option<&ResolvedComponent>,
        String,
        String,
        String,
    ) = match &selection.subject {
        crate::generate::root_selector::ResolvedRootSubject::OperatorOverride => {
            // Issue #359 — when `--root-purl <full>` is set, the BOM
            // subject's name + version come from the parsed PURL.
            // `resolved_name()` / `resolved_version()` apply the
            // precedence (full-purl-parsed name wins over discrete
            // `--root-name`); both fall back to the auto-derived
            // target when neither is set.
            let name = root_override
                .resolved_name()
                .map(str::to_string)
                .unwrap_or_else(|| target_name.to_string());
            let version = root_override
                .resolved_version()
                .map(str::to_string)
                .unwrap_or_else(|| target_version.to_string());
            // Milestone 077/#358 — `build_subject_purl` returns `None`
            // when `--no-root-purl` is in effect; the empty-string
            // fallback is safe because the `purl` JSON field is
            // post-processed off the emitted `metadata.component`
            // below when `omit_purl` is set.
            let purl = root_override
                .build_subject_purl(&name, &version)
                .unwrap_or_default();
            (None, name, version, purl)
        }
        crate::generate::root_selector::ResolvedRootSubject::MainModule(idx) => {
            let c = components.get(*idx);
            match c {
                Some(comp) => (
                    Some(comp),
                    comp.name.clone(),
                    comp.version.clone(),
                    comp.purl.as_str().to_string(),
                ),
                None => {
                    // Defensive fallback — should never happen because
                    // the selector validated the index. Degrade to the
                    // synthetic placeholder shape.
                    let purl = format!(
                        "pkg:generic/{}@{}",
                        encode_purl_segment(target_name),
                        encode_purl_segment(target_version),
                    );
                    (
                        None,
                        target_name.to_string(),
                        target_version.to_string(),
                        purl,
                    )
                }
            }
        }
        crate::generate::root_selector::ResolvedRootSubject::MavenCoord => {
            // Selector returns MavenCoord only when scan_target_coord is
            // Some; defend against an unexpected None by falling back to
            // the synthetic placeholder.
            match scan_target_coord {
                Some(coord) => {
                    let purl = format!(
                        "pkg:maven/{}/{}@{}",
                        encode_purl_segment(&coord.group),
                        encode_purl_segment(&coord.artifact),
                        encode_purl_segment(&coord.version),
                    );
                    (None, coord.artifact.clone(), coord.version.clone(), purl)
                }
                None => {
                    let purl = format!(
                        "pkg:generic/{}@{}",
                        encode_purl_segment(target_name),
                        encode_purl_segment(target_version),
                    );
                    (
                        None,
                        target_name.to_string(),
                        target_version.to_string(),
                        purl,
                    )
                }
            }
        }
        crate::generate::root_selector::ResolvedRootSubject::SyntheticPlaceholder {
            name,
            version,
        } => {
            let purl = format!(
                "pkg:generic/{}@{}",
                encode_purl_segment(name),
                encode_purl_segment(version),
            );
            (None, name.clone(), version.clone(), purl)
        }
    };

    // Milestone 127 FR-006 — emit the document-scope
    // `waybill:root-selection-heuristic` property when a tiebreaker
    // fired AND the auto-pick actually fell through past at least one
    // detected main-module (losers non-empty). This matches FR-007's
    // warning gate exactly and preserves byte-identity on the 8
    // zero-main-module fixtures (apk, bazel, cargo, cmake, deb, gem,
    // pip, rpm) per SC-003 — those fixtures hit `SyntheticPlaceholder`
    // with empty losers, where there's no "loss" to signal. The 3
    // single-main-module fixtures (golang, maven, npm) hit the
    // count==1 fast path with `heuristic == None`. Envelope shape
    // matches contracts/annotation-schema.md.
    if let Some(h) = selection.heuristic {
        if !selection.losers.is_empty() {
            let envelope = json!({
                "schema": "waybill-annotation/v1",
                "field": "waybill:root-selection-heuristic",
                "value": {
                    "heuristic": h.name(),
                    "confidence": h.confidence(),
                }
            });
            properties.push(json!({
                "name": "waybill:root-selection-heuristic",
                "value": serde_json::to_string(&envelope).unwrap_or_default(),
            }));
        }
    }

    // Milestone 158 US2 — always-emit `waybill:graph-completeness`
    // at document scope per FR-003 (universal presence). Value is
    // the three-way `complete|partial|unknown` string. The
    // companion `waybill:graph-completeness-reason` is emitted
    // conditionally per FR-004/FR-005 (present iff value !=
    // complete AND reason codes exist).
    properties.push(json!({
        "name": "waybill:graph-completeness",
        "value": graph_completeness.value.as_str(),
    }));
    if graph_completeness.value
        != crate::generate::graph_completeness::GraphCompletenessValue::Complete
        && !graph_completeness.reason_codes.is_empty()
    {
        properties.push(json!({
            "name": "waybill:graph-completeness-reason",
            "value": crate::generate::graph_completeness::join_reason_codes(
                &graph_completeness.reason_codes,
            ),
        }));
    }

    // Milestone 173 — C119 doc-scope Go cache-warming failed
    // annotation. Emitted BEFORE C118 for alphabetic sort:
    // `waybill:go-cache-warming-failed` sorts before
    // `waybill:go-cache-warming-mode`. Emission gated on Go presence
    // (Some) AND at least one failing workspace. Value is a JSON-
    // encoded array per contracts/annotation-wire-shapes.md.
    if let Some(cw) = go_cache_warming {
        if !cw.failures.is_empty() {
            let value = serde_json::to_string(&cw.failures).unwrap_or_default();
            properties.push(json!({
                "name": "waybill:go-cache-warming-failed",
                "value": value,
            }));
        }
    }

    // Milestone 176 — C121 doc-scope `waybill:workspaces-detected`
    // annotation. Value is a JSON-encoded array of the sorted-
    // deduplicated union of every per-component `waybill:workspace-
    // member` value (FR-003 + FR-012). Emission gated on the union
    // being non-empty (FR-003: absent when zero workspaces detected).
    // Computed by the shared helper so all three formats guarantee
    // the FR-012 cross-annotation invariant by construction.
    {
        let workspaces = crate::generate::workspace_detected::compute(components);
        if !workspaces.is_empty() {
            let value = serde_json::to_string(&workspaces).unwrap_or_default();
            properties.push(json!({
                "name": "waybill:workspaces-detected",
                "value": value,
            }));
        }
    }

    // Milestone 173 — C118 doc-scope Go cache-warming mode annotation.
    // Emitted BEFORE C110 for alphabetic sort. Value is one of `"off"`
    // / `"per-workspace"` / `"offline-inhibited"`. Emission gated on
    // Go presence (Some).
    if let Some(cw) = go_cache_warming {
        properties.push(json!({
            "name": "waybill:go-cache-warming-mode",
            "value": cw.mode.as_wire_str(),
        }));
    }

    // Milestone 160 (T034/T035) — doc-scope Go-transitive coverage.
    // C110 emitted iff the scan had ≥1 Go component (`go_transitive_coverage`
    // is `Some`). C111 conditionally emitted iff coverage != Complete.
    // Per FR-004/FR-005 + Q1 reason-code-driven decision rule.
    if let Some(coverage) = go_transitive_coverage {
        properties.push(json!({
            "name": "waybill:go-transitive-coverage",
            "value": coverage.value_wire_str(),
        }));
        if let Some(reason) = coverage.reason() {
            properties.push(json!({
                "name": "waybill:go-transitive-coverage-reason",
                "value": reason,
            }));
        }
    }

    // Milestone 172 — C117 doc-scope Go step-5 fallback count.
    // Emitted iff `go_transitive_fallback_count` is `Some(_)` (Go was
    // scanned). Value `"0"` explicit on healthy scans per Q1
    // clarification — consumers rely on presence-with-value to
    // differentiate "no Go here" from "Go scanned, no degradation".
    if let Some(count) = go_transitive_fallback_count {
        properties.push(json!({
            "name": "waybill:go-transitive-fallback-count",
            "value": count.to_string(),
        }));
    }

    // Milestone 161 (T042) — doc-scope Go-workspace-mode annotation.
    // C112 emitted iff `go.work` file was present at the scanned root
    // (i.e. `go_workspace_mode` is `Some` and the variant is
    // `Detected` or `Malformed`; `Absent` is treated as unpopulated
    // to preserve SC-003 byte-identity on non-workspace scans).
    if let Some(mode) = go_workspace_mode {
        use crate::scan_fs::package_db::golang::gowork::WorkspaceMode;
        if !matches!(mode, WorkspaceMode::Absent) {
            properties.push(json!({
                "name": "waybill:go-workspace-mode",
                "value": mode.as_wire_str(),
            }));
        }
    }

    // Milestone 204 (#554): C123 doc-scope helm image-extraction
    // completeness annotation. Emitted iff `helm_extraction_mode` is
    // `Some(_)` (helm reader ran). Wire values `"partial"` for
    // Unrendered (default US2 line-based extraction) and `"full"` for
    // Rendered (m203 `--helm-render` success). Byte-identity preserved
    // for non-Helm scans per FR-004 (annotation absent when `None`).
    if let Some(mode) = helm_extraction_mode {
        properties.push(json!({
            "name": "waybill:image-extraction-completeness",
            "value": mode.as_wire_str(),
        }));
    }

    // Milestone 206 (#440): C124 doc-scope image-source annotation.
    // Emitted iff `image_source` is `Some(Podman)` — conditional
    // emission preserves FR-005 byte-identity for docker/remote/path
    // scans (they see no drift vs pre-m206 output). Wire value:
    // closed enum `"podman"` in m206 MVP.
    if matches!(image_source, Some(crate::cli::scan_cmd::ImageSource::Podman)) {
        properties.push(json!({
            "name": "waybill:image-source",
            "value": "podman",
        }));
    }

    // Synthesize a minimal valid CPE 2.3 for the scan subject.
    //
    // Milestone 053: when the metadata.component is the Go main-module,
    // reuse its primary CPE from `c.cpes[0]` (synthesized in
    // `scan_fs/mod.rs::synthesize_cpes` from the PURL using the same
    // shape as every other component) so the BOM-subject CPE
    // round-trips identically to the SPDX 2.3 / SPDX 3 emission.
    // Pre-053 the metadata.component used a `cpe:2.3:a:waybill:…`
    // synthetic shape that diverged from the SPDX side; post-053 the
    // shapes are identical for the main-module case.
    //
    // Uses waybill as the vendor for the placeholder fallback. Name
    // and version segments are CPE-sanitized (lowercase, non-
    // alphanumerics → underscore). sbomqs's schema validator runs CPE
    // validation on metadata.component and flags empty/absent fields
    // as invalid.
    let synthetic_component_cpe = if override_active {
        // Milestone 077 — operator-supplied identity drives the CPE
        // verbatim through the existing `cpe_sanitize` helper. Vendor
        // stays hardcoded `waybill` per spec assumption (out of scope
        // for this milestone).
        format!(
            "cpe:2.3:a:waybill:{}:{}:*:*:*:*:*:*:*",
            cpe_sanitize(&subject_name),
            cpe_sanitize(&subject_version),
        )
    } else if let Some(c) = main_module {
        c.cpes
            .first()
            .cloned()
            .unwrap_or_else(|| {
                format!(
                    "cpe:2.3:a:waybill:{}:{}:*:*:*:*:*:*:*",
                    cpe_sanitize(&subject_name),
                    cpe_sanitize(&subject_version),
                )
            })
    } else {
        format!(
            "cpe:2.3:a:waybill:{}:{}:*:*:*:*:*:*:*",
            cpe_sanitize(&subject_name),
            cpe_sanitize(&subject_version),
        )
    };

    // Milestone 080 — user-supplied creators routed by Type per the
    // research §2 routing matrix. `Tool` → metadata.tools.components[];
    // `Person` → metadata.authors[]; first `Organization` →
    // metadata.manufacturer (single-valued); subsequent Organization
    // entries are emitted into bom.annotations[] by the builder layer
    // (see `build_user_annotations`).
    use waybill::binding::user_metadata::CreatorKind;
    let mut user_authors: Vec<serde_json::Value> = Vec::new();
    let mut user_tool_components: Vec<serde_json::Value> = Vec::new();
    let mut user_manufacturer: Option<serde_json::Value> = None;
    for creator in &user_metadata.creators {
        match creator.kind {
            CreatorKind::Tool => {
                user_tool_components.push(json!({
                    "type": "application",
                    "name": creator.name,
                }));
            }
            CreatorKind::Organization => {
                if user_manufacturer.is_none() {
                    user_manufacturer = Some(json!({ "name": creator.name }));
                }
                // Subsequent Organization creators are routed to
                // bom.annotations[] by `build_user_annotations`.
            }
            CreatorKind::Person => {
                user_authors.push(json!({ "name": creator.name }));
            }
        }
    }

    let mut tools_components_arr = vec![json!({
        "type": "application",
        "name": "waybill",
        "version": version,
        "publisher": "waybill contributors"
    })];
    for c in user_tool_components {
        tools_components_arr.push(c);
    }
    let mut authors_arr = vec![json!({ "name": "waybill" })];
    for a in user_authors {
        authors_arr.push(a);
    }
    let supplier_obj = match &user_manufacturer {
        Some(_) => json!({ "name": "waybill contributors" }),
        None => json!({ "name": "waybill contributors" }),
    };
    let mut metadata = json!({
        "timestamp": timestamp,
        // Top-level SBOM provenance: the list of individuals or
        // organizations responsible for creating THIS SBOM (not the
        // underlying project). Scored by sbomqs `sbom_authors` (2.9%
        // in Provenance). Waybill is always present; user-supplied
        // `--creator "Person: ..."` entries append (milestone 080).
        "authors": authors_arr,
        // SBOM supplier: the organization providing the SBOM. Scored
        // by sbomqs `sbom_supplier` (2.2%). Hardcoded to the waybill
        // project identity.
        "supplier": supplier_obj,
        // SBOM content license. SPDX-SBOM convention uses CC0-1.0 so
        // the SBOM itself can be distributed without restriction.
        // Scored by sbomqs `sbom_data_license` (1.8% in Licensing).
        "licenses": [
            { "license": { "id": "CC0-1.0" } }
        ],
        "tools": {
            "components": tools_components_arr
        },
        "component": {
            "type": "application",
            "name": if !override_active && user_metadata.scan_target_name.is_some() && main_module.is_none() && scan_target_coord.is_none() {
                // Milestone 080 — `--scan-target-name` overrides the
                // metadata.component.name when --root-name is not set
                // AND no manifest-derived main-module / Maven coord
                // claims the slot. Per research §5, --root-name takes
                // precedence on CDX when both are set.
                serde_json::Value::String(
                    user_metadata
                        .scan_target_name
                        .clone()
                        .unwrap_or_else(|| subject_name.clone()),
                )
            } else {
                serde_json::Value::String(subject_name.clone())
            },
            "version": subject_version,
            "bom-ref": if main_module.is_some() && !override_active {
                // Milestone 053: when the metadata.component is the
                // Go main-module (and the operator has NOT overridden
                // the root identity per milestone 077), its bom-ref
                // MUST equal the PURL so existing `dependencies[].ref`
                // entries (which key off the main-module's PURL via
                // `scan_fs/mod.rs`'s edge-emission loop) resolve to
                // it. The default `name@version` shape works for
                // synthetic placeholders + override paths but breaks
                // edge resolution for real main-module components.
                synthetic_component_purl.clone()
            } else {
                // Milestone 077: when override is active, bom-ref uses
                // the operator-supplied identity verbatim — manifest-
                // derived main-modules are dropped from components[]
                // anyway (clean replacement), so there are no
                // dependencies[].ref entries keyed off their PURL.
                format!("{}@{}", subject_name, subject_version)
            },
            "purl": synthetic_component_purl,
            "cpe": synthetic_component_cpe,
        },
        "properties": properties,
    });

    // Milestone N+1: `--no-root-purl` drops the `purl` field from
    // `metadata.component` entirely. The CDX 1.6 schema makes `purl`
    // optional, so absence (vs null) is the spec-correct shape.
    if root_override.omit_purl {
        if let Some(comp_obj) = metadata
            .get_mut("component")
            .and_then(|v| v.as_object_mut())
        {
            comp_obj.remove("purl");
        }
    }

    // Milestone 080 — first user-supplied `Organization:` creator
    // populates CDX 1.6 `metadata.manufacturer` (single-valued slot
    // per research §1).
    if let Some(m) = user_manufacturer {
        if let Some(obj) = metadata.as_object_mut() {
            obj.insert("manufacturer".to_string(), m);
        }
    }

    // Milestone 053 FR-004: when the metadata.component is the Go
    // main-module (per the ladder above), surface the supplementary
    // `waybill:component-role: main-module` C40 annotation as a
    // metadata.component-level property so consumers reading either
    // the native field (`type: "application"`) OR the supplementary
    // tag identify the main-module. Also surface
    // `waybill:sbom-tier: "source"` per FR-006.
    //
    // Milestone 077: when the override is active, the manifest main-
    // module is no longer the BOM subject — skip these supplementary
    // properties so the emitted root reflects only operator-supplied
    // identity (clean replacement per Q2 clarification).
    if let (Some(c), false) = (main_module, override_active) {
        let mut comp_props = vec![json!({
            "name": "waybill:component-role",
            "value": "main-module",
        })];
        if let Some(tier) = c.sbom_tier.as_ref() {
            comp_props.push(json!({
                "name": "waybill:sbom-tier",
                "value": tier,
            }));
        }
        // Propagate `waybill:source-files` (C18) from the main-module's
        // evidence so the parity-extractor framework finds the go.mod
        // path on the CDX side, matching the SPDX `packages[]` emission.
        // Milestone 133 US2.1 (FR-012 Defect B): JSON-array serialization
        // (paths arrive pre-normalized from the source-population sites
        // in `scan_fs::mod.rs`).
        if let Some(value) = crate::scan_fs::sbom_path::source_files_as_json_array(
            &c.evidence.source_file_paths,
        ) {
            comp_props.push(json!({
                "name": "waybill:source-files",
                "value": value,
            }));
        }
        // Propagate `waybill:detected-go` (C14) — true for any Go
        // workspace's main-module per build_main_module_entry's
        // `detected_go: Some(true)`.
        if c.detected_go.unwrap_or(false) {
            comp_props.push(json!({
                "name": "waybill:detected-go",
                "value": "true",
            }));
        }
        // Milestone 176 — propagate `waybill:workspace-member` (C120)
        // so the main-module's workspace annotation lands on
        // metadata.component's properties[] (parity with SPDX 2.3 +
        // SPDX 3 which emit the main-module as a Package with the
        // annotation intact). Mirrors the milestone-116 / milestone-
        // 134 propagation pattern above. Without this, holistic
        // parity fails: SPDX side finds the main-module's workspace
        // path (e.g., `.`) while CDX drops it, producing a
        // C120 `SymmetricEqual` violation.
        if let Some(value) = c
            .extra_annotations
            .get("waybill:workspace-member")
            .and_then(|v| v.as_str())
        {
            comp_props.push(json!({
                "name": "waybill:workspace-member",
                "value": value,
            }));
        }
        // Milestone 116 — propagate `waybill:produces-binaries` (C64) so
        // single-package scans (which promote the manifest main-module to
        // `metadata.component`) carry the declaration where the cross-
        // tier binder will find it. Multi-member workspaces emit through
        // the components[] path and pick up the property automatically
        // via the general extra_annotations → properties wiring; only
        // the single-main-module → metadata.component promotion path
        // needs explicit propagation.
        if let Some(value) = c
            .extra_annotations
            .get("waybill:produces-binaries")
            .and_then(|v| v.as_array())
        {
            comp_props.push(json!({
                "name": "waybill:produces-binaries",
                "value": serde_json::Value::Array(value.clone()).to_string(),
            }));
        }
        // Milestone 134 — propagate `waybill:duplicate-purl-divergent`
        // (C99) when the deduped main-module is promoted to
        // `metadata.component`. Single-crate workspaces hit this path;
        // multi-member workspaces emit via `components[]` and pick up
        // the property automatically through the general
        // extra_annotations → properties wiring in
        // `builder.rs`. Mirrors the milestone-116
        // `waybill:produces-binaries` propagation pattern above.
        if let Some(value) = c
            .extra_annotations
            .get("waybill:duplicate-purl-divergent")
        {
            let value_str = match value {
                serde_json::Value::String(s) => s.clone(),
                other => serde_json::to_string(other).unwrap_or_default(),
            };
            comp_props.push(json!({
                "name": "waybill:duplicate-purl-divergent",
                "value": value_str,
            }));
        }
        // Milestone 140 — propagate `waybill:umbrella-root` when the
        // Elixir umbrella root main-module is promoted to
        // `metadata.component` (small umbrella scans with the root as
        // the dominant project). Multi-member umbrellas emit via
        // components[] and pick up the property automatically.
        // Mirrors the milestone-116 + milestone-134 propagation
        // patterns above.
        if let Some(value) = c.extra_annotations.get("waybill:umbrella-root") {
            let value_str = match value {
                serde_json::Value::String(s) => s.clone(),
                other => serde_json::to_string(other).unwrap_or_default(),
            };
            comp_props.push(json!({
                "name": "waybill:umbrella-root",
                "value": value_str,
            }));
        }
        // Milestone 142 (Scala/SBT) — propagate `waybill:scala-version-source`
        // when the SBT main-module is promoted to `metadata.component`.
        // Required by the F6 remediation contract: operators must be
        // able to distinguish a `_2.13` suffix derived from explicit
        // `scalaVersion := "..."` from one defaulted via the Q1 cascade's
        // rung-3 fallback. Without this propagation, the annotation is
        // visible on the main-module ONLY in the components[] array path
        // (multi-subproject scans), not on the promoted subject path.
        // Mirrors the milestone-140 `waybill:umbrella-root` pattern above.
        if let Some(value) = c.extra_annotations.get("waybill:scala-version-source") {
            let value_str = match value {
                serde_json::Value::String(s) => s.clone(),
                other => serde_json::to_string(other).unwrap_or_default(),
            };
            comp_props.push(json!({
                "name": "waybill:scala-version-source",
                "value": value_str,
            }));
        }
        metadata["component"]["properties"] = json!(comp_props);

        // Propagate the supplier so the parity Section A `cdx_supplier`
        // extractor matches the SPDX 2.3 `Package.supplier` derivation
        // (both come from the PURL namespace via `supplier_from_purl`).
        if let Some(supplier_name) = c.supplier.as_ref() {
            metadata["component"]["supplier"] = json!({
                "name": supplier_name,
            });
        }

        // Milestone 119 follow-up — propagate the main-module's typed
        // `licenses[]` (and `concluded_licenses[]`) onto the
        // metadata.component subject. When a supplement override is in
        // effect the supplement's declared licenses have already been
        // projected into the typed Vec by
        // `crate::supplement::conflict::resolve_component`; this
        // propagation makes them visible on the BOM subject regardless
        // of whether the main-module promoted to metadata.component
        // (single-member workspace) or stayed in components[]
        // (multi-member workspace).
        //
        // Absent supplement → the main-module's pre-existing typed
        // licenses (from Cargo.toml `license` field, etc.) propagate
        // exactly the same way; behavior is byte-identical when the
        // scanner had nothing AND no supplement was supplied (FR-013).
        let license_array = build_metadata_component_licenses(c);
        if !license_array.is_empty() {
            metadata["component"]["licenses"] = json!(license_array);
        }

        // Milestone 133 US2.3 (FR-014): propagate the main-module's
        // `evidence.occurrences[]` onto the promoted metadata.component
        // so the CDX-native field is on the BOM subject in the same
        // place SPDX 2.3/3 carry it (the main-module is a regular
        // Package on the SPDX side). Without this, the
        // `holistic_parity` D2 SymmetricEqual assertion fails on
        // single-main-module workspaces because SPDX has the
        // main-module's go.mod / Cargo.lock / pom.xml occurrence and
        // CDX does not.
        if !c.occurrences.is_empty() {
            let evidence_value = crate::generate::cyclonedx::evidence::build_evidence(
                &c.evidence,
                &c.occurrences,
                None,
                &[],
            );
            metadata["component"]["evidence"] = evidence_value;
        }
    }

    if !lifecycles.is_empty() {
        metadata["lifecycles"] = json!(lifecycles);
    }

    // Milestone 073 — built-in identifiers ride
    // `metadata.component.externalReferences[]` per
    // `contracts/identifiers-annotation.md` C-1 CDX 1.6. Per-
    // scheme `type` mapping per research.md §2 (`vcs` for repo:/git:,
    // `distribution` for image:, `attestation` for attestation:).
    // Order: auto-detected first (per FR-009 / VR-008), then manual
    // in supply order. The Vec is already deduplicated and ordered
    // by `cli/scan_cmd.rs::resolve_identifiers`.
    let builtin_id_refs: Vec<serde_json::Value> = identifiers
        .iter()
        .filter_map(|id| match id.kind {
            waybill::binding::identifiers::IdentifierKind::Builtin(b) => {
                let comment = id
                    .source_label
                    .clone()
                    .unwrap_or_else(|| "manual identifier flag".to_string());
                Some(json!({
                    "type": b.cdx_external_reference_type(),
                    "url": id.value.as_str(),
                    "comment": comment,
                }))
            }
            waybill::binding::identifiers::IdentifierKind::UserDefined => None,
        })
        .collect();
    if !builtin_id_refs.is_empty() {
        let existing = metadata
            .get_mut("component")
            .and_then(|c| c.get_mut("externalReferences"))
            .and_then(|v| v.as_array_mut());
        match existing {
            Some(arr) => {
                for r in builtin_id_refs.iter() {
                    arr.push(r.clone());
                }
            }
            None => {
                if let Some(comp) = metadata.get_mut("component") {
                    comp["externalReferences"] = json!(builtin_id_refs);
                }
            }
        }
    }

    // Milestone 073 — user-defined identifiers ride a single
    // `metadata.properties[]` entry under `waybill:identifiers`
    // per `contracts/identifiers-annotation.md` C-2 CDX 1.6.
    // The value is a JSON-encoded array sorted lex by `(scheme, value)`
    // for determinism (FR-009 / contract C-4). Emit ONLY when the
    // user-defined entry set is non-empty per VR-007 — preserves
    // cross-format byte-identity for non-user-defined-namespace scans.
    let user_defined_payload: Vec<serde_json::Value> = {
        let mut entries: Vec<&waybill::binding::identifiers::Identifier> = identifiers
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
        let json_str = serde_json::to_string(&user_defined_payload)
            .unwrap_or_else(|_| "[]".to_string());
        if let Some(props) = metadata.get_mut("properties").and_then(|v| v.as_array_mut())
        {
            props.push(json!({
                "name": "waybill:identifiers",
                "value": json_str,
            }));
        }
    }

    // Milestone 072 / T010 — standards-native cross-document reference
    // to the source-tier SBOM per
    // `contracts/source-document-binding-annotation.md` C-2 CDX 1.6.
    // `type: "bom"` is the CDX 1.6 native cross-document semantic.
    if let Some(id) = source_document_binding {
        let mut ref_obj = json!({
            "type": "bom",
            "comment": "source-tier SBOM that produced this build/deployment",
            "hashes": [{ "alg": "SHA-256", "content": id.sha256.clone() }],
        });
        // The URL field is mandatory in CDX 1.6's
        // externalReferences[]. We use the IRI when available;
        // otherwise fall back to a content-addressed `urn:sha256:`
        // pseudo-IRI so consumers without the source SBOM file can
        // still reference it by content hash.
        let url = id
            .iri
            .clone()
            .unwrap_or_else(|| format!("urn:sha256:{}", id.sha256));
        ref_obj["url"] = json!(url);
        let existing = metadata
            .get_mut("component")
            .and_then(|c| c.get_mut("externalReferences"))
            .and_then(|v| v.as_array_mut());
        match existing {
            Some(arr) => arr.push(ref_obj),
            None => {
                if let Some(comp) = metadata.get_mut("component") {
                    comp["externalReferences"] = json!([ref_obj]);
                }
            }
        }
    }

    metadata
}

/// Milestone 080 — build CDX 1.6 `bom.annotations[]` entries for the
/// user-supplied `--metadata-comment`, `--annotator` /
/// `--annotation-comment` pairs, and any subsequent `Organization:`
/// creators that don't fit in `metadata.manufacturer`. Returns an
/// empty vec when `user_metadata.is_active()` would not produce any
/// bom-level annotation entries.
///
/// Milestone 119 follow-up — render the main-module's typed
/// `licenses[]` + `concluded_licenses[]` into the CDX 1.6 license
/// array shape (`oneOf` `{license:{id,...}}` / `{license:{name,...}}`
/// / `{expression,...}`). Mirrors the lighter end of `build_components`'s
/// per-component license rendering at `builder.rs:678-715` — single-
/// component scope so no `try_split_or_compound` short-circuit is
/// needed for the MVP propagation.
fn build_metadata_component_licenses(
    c: &ResolvedComponent,
) -> Vec<serde_json::Value> {
    let mut out: Vec<serde_json::Value> = Vec::new();
    let sources: [(&[SpdxExpression], &str); 2] = [
        (&c.licenses, "declared"),
        (&c.concluded_licenses, "concluded"),
    ];
    for (exprs, ack) in sources {
        for l in exprs {
            if let Some(id) = l.as_spdx_id() {
                out.push(json!({
                    "license": { "id": id, "acknowledgement": ack }
                }));
            } else if l.as_str().starts_with("LicenseRef-")
                || l.as_str().starts_with("DocumentRef-")
            {
                out.push(json!({
                    "license": { "name": l.as_str(), "acknowledgement": ack }
                }));
            } else {
                // Compound or otherwise non-id expression — emit as a
                // license.name so single-string operator-declared
                // values (e.g. "Acme Custom License") surface
                // intelligibly. The full builder.rs path also handles
                // `OR`/`AND` splitting + expression fallback; we keep
                // this helper minimal because the BOM subject's
                // licenses are operator-declared (already canonical)
                // far more often than the per-component case.
                out.push(json!({
                    "license": { "name": l.as_str(), "acknowledgement": ack }
                }));
            }
        }
    }
    out
}

/// Per research §1, CDX 1.6 `bom.annotations[]` is the
/// standards-native landing slot. The `subjects[]` entry points at
/// the root component's bom-ref to satisfy the CDX 1.6 schema's
/// `subjects: required, uniqueItems` contract.
///
/// Each entry has shape `{subjects, annotator: <oneOf>, timestamp,
/// text}` per research §1's audit; the `annotator` field's `oneOf`
/// choice is selected by the creator's Type:
/// - `Tool` → `annotator.component = { type: "application", name }`
/// - `Organization` → `annotator.organization = { name }`
/// - `Person` → `annotator.individual = { name }`
pub fn build_user_annotations(
    user_metadata: &waybill::binding::user_metadata::UserMetadata,
    root_bom_ref: &str,
    timestamp: &str,
) -> Vec<serde_json::Value> {
    use waybill::binding::user_metadata::CreatorKind;
    let mut out: Vec<serde_json::Value> = Vec::new();

    // 1) --metadata-comment: annotator.organization.name = "waybill contributors"
    if let Some(comment) = &user_metadata.metadata_comment {
        out.push(json!({
            "subjects": [root_bom_ref],
            "annotator": { "organization": { "name": "waybill contributors" } },
            "timestamp": timestamp,
            "text": comment,
        }));
    }

    // 2) Multi-Organization edge case: 2nd+ Organization creators
    //    route to bom.annotations[] per research §1 + §2.
    let mut org_seen = false;
    for creator in &user_metadata.creators {
        if matches!(creator.kind, CreatorKind::Organization) {
            if !org_seen {
                org_seen = true;
                continue;
            }
            out.push(json!({
                "subjects": [root_bom_ref],
                "annotator": { "organization": { "name": creator.name } },
                "timestamp": timestamp,
                "text": "creator",
            }));
        }
    }

    // 3) --annotator + --annotation-comment pairs.
    for ann in &user_metadata.annotations {
        let annotator_obj = match ann.annotator.kind {
            CreatorKind::Tool => json!({
                "component": { "type": "application", "name": ann.annotator.name }
            }),
            CreatorKind::Organization => json!({
                "organization": { "name": ann.annotator.name }
            }),
            CreatorKind::Person => json!({
                "individual": { "name": ann.annotator.name }
            }),
        };
        out.push(json!({
            "subjects": [root_bom_ref],
            "annotator": annotator_obj,
            "timestamp": timestamp,
            "text": ann.comment,
        }));
    }

    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn metadata_has_required_fields() {
        let meta = build_metadata("myapp", "0.1.0", GenerationContext::BuildTimeTrace, &[], &[], &TraceIntegrity::default(), None, None, &[], &RootComponentOverride::default(), &waybill::binding::user_metadata::UserMetadata::default(), None, None, None, None, &crate::generate::graph_completeness::GraphCompletenessResult::trivially_complete(), None, None, None, None, None, None, None);

        assert!(meta["timestamp"].is_string());
        assert_eq!(meta["tools"]["components"][0]["name"], "waybill");
        assert_eq!(meta["component"]["name"], "myapp");
        assert_eq!(meta["component"]["version"], "0.1.0");
        assert_eq!(
            meta["properties"][0]["name"],
            "waybill:generation-context"
        );
        assert_eq!(
            meta["properties"][0]["value"],
            "build-time-trace"
        );
    }

    // --- sbomqs score lift: metadata completeness (Fixes 3-6) ------------

    #[test]
    fn metadata_includes_authors_for_sbom_authors_score() {
        let meta =
            build_metadata("myapp", "0.1.0", GenerationContext::BuildTimeTrace, &[], &[], &TraceIntegrity::default(), None, None, &[], &RootComponentOverride::default(), &waybill::binding::user_metadata::UserMetadata::default(), None, None, None, None, &crate::generate::graph_completeness::GraphCompletenessResult::trivially_complete(), None, None, None, None, None, None, None);
        let authors = meta["authors"].as_array().expect("authors must be array");
        assert!(!authors.is_empty(), "authors must be non-empty");
        assert!(authors[0]["name"].is_string());
    }

    #[test]
    fn metadata_includes_supplier_for_sbom_supplier_score() {
        let meta =
            build_metadata("myapp", "0.1.0", GenerationContext::BuildTimeTrace, &[], &[], &TraceIntegrity::default(), None, None, &[], &RootComponentOverride::default(), &waybill::binding::user_metadata::UserMetadata::default(), None, None, None, None, &crate::generate::graph_completeness::GraphCompletenessResult::trivially_complete(), None, None, None, None, None, None, None);
        assert!(
            meta["supplier"]["name"].is_string(),
            "supplier.name must be present as a string"
        );
    }

    #[test]
    fn metadata_includes_cc0_data_license() {
        // sbomqs sbom_data_license scores the SBOM's own license. SPDX
        // convention is CC0-1.0 so SBOM content is free to redistribute.
        let meta =
            build_metadata("myapp", "0.1.0", GenerationContext::BuildTimeTrace, &[], &[], &TraceIntegrity::default(), None, None, &[], &RootComponentOverride::default(), &waybill::binding::user_metadata::UserMetadata::default(), None, None, None, None, &crate::generate::graph_completeness::GraphCompletenessResult::trivially_complete(), None, None, None, None, None, None, None);
        let licenses = meta["licenses"].as_array().expect("licenses must be array");
        assert!(!licenses.is_empty());
        assert_eq!(licenses[0]["license"]["id"], "CC0-1.0");
    }

    #[test]
    fn metadata_component_has_synthetic_purl() {
        // sbomqs flags metadata.component as invalid without a purl.
        // Waybill synthesizes pkg:generic/<name>@<version>.
        let meta =
            build_metadata("myapp", "0.1.0", GenerationContext::BuildTimeTrace, &[], &[], &TraceIntegrity::default(), None, None, &[], &RootComponentOverride::default(), &waybill::binding::user_metadata::UserMetadata::default(), None, None, None, None, &crate::generate::graph_completeness::GraphCompletenessResult::trivially_complete(), None, None, None, None, None, None, None);
        assert_eq!(meta["component"]["purl"], "pkg:generic/myapp@0.1.0");
    }

    #[test]
    fn metadata_component_has_synthetic_cpe() {
        // sbomqs flags empty/absent cpe on metadata.component as invalid.
        // Waybill emits cpe:2.3:a:waybill:<name>:<version>:*:*:*:*:*:*:*.
        let meta =
            build_metadata("myapp", "0.1.0", GenerationContext::BuildTimeTrace, &[], &[], &TraceIntegrity::default(), None, None, &[], &RootComponentOverride::default(), &waybill::binding::user_metadata::UserMetadata::default(), None, None, None, None, &crate::generate::graph_completeness::GraphCompletenessResult::trivially_complete(), None, None, None, None, None, None, None);
        assert_eq!(
            meta["component"]["cpe"],
            "cpe:2.3:a:waybill:myapp:0.1.0:*:*:*:*:*:*:*"
        );
    }

    #[test]
    fn cpe_sanitize_handles_special_characters() {
        assert_eq!(cpe_sanitize("My App"), "my_app");
        assert_eq!(cpe_sanitize("app+v1"), "app_v1");
        assert_eq!(cpe_sanitize("MYAPP"), "myapp");
        assert_eq!(cpe_sanitize("my-app.v2"), "my-app.v2");
        assert_eq!(cpe_sanitize(""), "_");
    }

    #[test]
    fn metadata_component_purl_encodes_special_chars() {
        // Ensure target names / versions with special chars are
        // percent-encoded via encode_purl_segment.
        let meta = build_metadata(
            "my app with spaces",
            "1.0+build-1",
            GenerationContext::FilesystemScan,
            &[],
            &[],
            &TraceIntegrity::default(),
        None,
        None,
        &[],
        &RootComponentOverride::default(),
        &waybill::binding::user_metadata::UserMetadata::default(),
        None,
            None,
        None,
        None,
            &crate::generate::graph_completeness::GraphCompletenessResult::trivially_complete(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        let purl = meta["component"]["purl"].as_str().unwrap();
        assert!(
            purl.starts_with("pkg:generic/"),
            "purl must start with pkg:generic/, got {purl}"
        );
        // The `+` in `1.0+build-1` must be encoded.
        assert!(
            purl.contains("%20") || purl.contains("%2B") || !purl.contains(' '),
            "special chars must be encoded: {purl}"
        );
    }

    #[test]
    fn metadata_bom_ref_format() {
        let meta = build_metadata("myapp", "0.1.0", GenerationContext::BuildTimeTrace, &[], &[], &TraceIntegrity::default(), None, None, &[], &RootComponentOverride::default(), &waybill::binding::user_metadata::UserMetadata::default(), None, None, None, None, &crate::generate::graph_completeness::GraphCompletenessResult::trivially_complete(), None, None, None, None, None, None, None);
        assert_eq!(meta["component"]["bom-ref"], "myapp@0.1.0");
    }

    // =====================================================================
    // Milestone 160 (T037/T038): doc-scope C110/C111 emission behavior
    // =====================================================================

    #[test]
    fn t037_c110_emitted_when_coverage_present() {
        // SC-005: doc-scope C110 present iff `go_transitive_coverage` is Some.
        use crate::scan_fs::package_db::golang::graph_resolver::GoTransitiveCoverage;
        let coverage = GoTransitiveCoverage::Complete;
        let meta = build_metadata(
            "myapp", "0.1.0", GenerationContext::FilesystemScan,
            &[], &[], &TraceIntegrity::default(),
            None, None, &[],
            &RootComponentOverride::default(),
            &waybill::binding::user_metadata::UserMetadata::default(),
            None, None, None, None,
            &crate::generate::graph_completeness::GraphCompletenessResult::trivially_complete(),
            Some(&coverage),
            None,
            None,
            None,
            None,
            None,
            None,
        );
        let props = meta["properties"].as_array().expect("properties array");
        let c110 = props.iter().find(|p| p["name"] == "waybill:go-transitive-coverage");
        assert!(c110.is_some(), "T037: C110 must be present when coverage is Some");
        assert_eq!(c110.unwrap()["value"], "complete");
        // C111 absent on Complete variant (Q1: reason is None).
        let c111 = props.iter().find(|p| p["name"] == "waybill:go-transitive-coverage-reason");
        assert!(c111.is_none(), "T037: C111 must be absent on Complete");
    }

    #[test]
    fn t037_c110_absent_when_coverage_none() {
        // SC-003 byte-identity guard: no Go scan → no C110/C111.
        let meta = build_metadata(
            "myapp", "0.1.0", GenerationContext::FilesystemScan,
            &[], &[], &TraceIntegrity::default(),
            None, None, &[],
            &RootComponentOverride::default(),
            &waybill::binding::user_metadata::UserMetadata::default(),
            None, None, None, None,
            &crate::generate::graph_completeness::GraphCompletenessResult::trivially_complete(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        let props = meta["properties"].as_array().expect("properties array");
        assert!(
            props.iter().all(|p| p["name"] != "waybill:go-transitive-coverage"),
            "SC-003: C110 must be absent on Go-free scans"
        );
        assert!(
            props.iter().all(|p| p["name"] != "waybill:go-transitive-coverage-reason"),
            "SC-003: C111 must be absent on Go-free scans"
        );
    }

    #[test]
    fn t038_c111_present_on_partial_coverage() {
        // FR-005: reason accompanies C110 iff value != Complete.
        use crate::scan_fs::package_db::golang::graph_resolver::GoTransitiveCoverage;
        let coverage = GoTransitiveCoverage::Partial(
            "proxy-fetch-degraded: 45 of 300 modules unresolved".to_string(),
        );
        let meta = build_metadata(
            "myapp", "0.1.0", GenerationContext::FilesystemScan,
            &[], &[], &TraceIntegrity::default(),
            None, None, &[],
            &RootComponentOverride::default(),
            &waybill::binding::user_metadata::UserMetadata::default(),
            None, None, None, None,
            &crate::generate::graph_completeness::GraphCompletenessResult::trivially_complete(),
            Some(&coverage),
            None,
            None,
            None,
            None,
            None,
            None,
        );
        let props = meta["properties"].as_array().expect("properties array");
        let c110 = props.iter().find(|p| p["name"] == "waybill:go-transitive-coverage");
        assert_eq!(c110.expect("C110 present").as_object().unwrap()["value"], "partial");
        let c111 = props.iter().find(|p| p["name"] == "waybill:go-transitive-coverage-reason");
        assert_eq!(
            c111.expect("C111 present").as_object().unwrap()["value"],
            "proxy-fetch-degraded: 45 of 300 modules unresolved"
        );
    }

    #[test]
    fn t038_c111_present_on_unknown_coverage() {
        // FR-005: Unknown variant emits reason too.
        use crate::scan_fs::package_db::golang::graph_resolver::GoTransitiveCoverage;
        let coverage = GoTransitiveCoverage::Unknown(
            "offline-mode: transitive edges from proxy fetches unavailable".to_string(),
        );
        let meta = build_metadata(
            "myapp", "0.1.0", GenerationContext::FilesystemScan,
            &[], &[], &TraceIntegrity::default(),
            None, None, &[],
            &RootComponentOverride::default(),
            &waybill::binding::user_metadata::UserMetadata::default(),
            None, None, None, None,
            &crate::generate::graph_completeness::GraphCompletenessResult::trivially_complete(),
            Some(&coverage),
            None,
            None,
            None,
            None,
            None,
            None,
        );
        let props = meta["properties"].as_array().expect("properties array");
        let c110 = props.iter().find(|p| p["name"] == "waybill:go-transitive-coverage");
        assert_eq!(c110.expect("C110 present").as_object().unwrap()["value"], "unknown");
        let c111 = props.iter().find(|p| p["name"] == "waybill:go-transitive-coverage-reason");
        assert!(
            c111.expect("C111 present").as_object().unwrap()["value"]
                .as_str().unwrap()
                .starts_with("offline-mode:"),
            "T038: Unknown reason must retain offline-mode: prefix"
        );
    }

    // =====================================================================
    // Milestone 161 (T047/T048/T049): doc-scope C112 emission behavior
    // =====================================================================

    #[test]
    fn t047_c112_emitted_when_workspace_mode_detected() {
        // SC-004: doc-scope C112 present iff `go_workspace_mode` is Some
        // and variant is `Detected`. Value follows the C112 wire vocab.
        use crate::scan_fs::package_db::golang::gowork::WorkspaceMode;
        let mode = WorkspaceMode::Detected { use_count: 5 };
        let meta = build_metadata(
            "myapp", "0.1.0", GenerationContext::FilesystemScan,
            &[], &[], &TraceIntegrity::default(),
            None, None, &[],
            &RootComponentOverride::default(),
            &waybill::binding::user_metadata::UserMetadata::default(),
            None, None, None, None,
            &crate::generate::graph_completeness::GraphCompletenessResult::trivially_complete(),
            None,
            Some(&mode),
            None,
            None,
            None,
            None,
            None,
        );
        let props = meta["properties"].as_array().expect("properties array");
        let c112 = props.iter().find(|p| p["name"] == "waybill:go-workspace-mode");
        assert!(c112.is_some(), "T047: C112 must be present when workspace_mode is Detected");
        assert_eq!(c112.unwrap()["value"], "detected: 5 use-modules");
    }

    #[test]
    fn t048_c112_absent_when_workspace_mode_absent() {
        // SC-003 byte-identity guard: no go.work file → no C112.
        // Even when `go_workspace_mode` is Some(Absent), the annotation
        // must be absent to preserve byte-identity on non-workspace scans.
        use crate::scan_fs::package_db::golang::gowork::WorkspaceMode;
        let mode = WorkspaceMode::Absent;
        let meta = build_metadata(
            "myapp", "0.1.0", GenerationContext::FilesystemScan,
            &[], &[], &TraceIntegrity::default(),
            None, None, &[],
            &RootComponentOverride::default(),
            &waybill::binding::user_metadata::UserMetadata::default(),
            None, None, None, None,
            &crate::generate::graph_completeness::GraphCompletenessResult::trivially_complete(),
            None,
            Some(&mode),
            None,
            None,
            None,
            None,
            None,
        );
        let props = meta["properties"].as_array().expect("properties array");
        assert!(
            props.iter().all(|p| p["name"] != "waybill:go-workspace-mode"),
            "SC-003: C112 must be absent when WorkspaceMode is Absent"
        );
    }

    #[test]
    fn t048_c112_absent_when_workspace_mode_none() {
        // Complementary: None workspace mode also emits no C112.
        let meta = build_metadata(
            "myapp", "0.1.0", GenerationContext::FilesystemScan,
            &[], &[], &TraceIntegrity::default(),
            None, None, &[],
            &RootComponentOverride::default(),
            &waybill::binding::user_metadata::UserMetadata::default(),
            None, None, None, None,
            &crate::generate::graph_completeness::GraphCompletenessResult::trivially_complete(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        let props = meta["properties"].as_array().expect("properties array");
        assert!(
            props.iter().all(|p| p["name"] != "waybill:go-workspace-mode"),
            "SC-003: C112 must be absent when go_workspace_mode is None"
        );
    }

    #[test]
    fn t049_c112_malformed_variant_emits_reason() {
        // FR-004: Malformed variant emits `malformed: <reason>` value.
        use crate::scan_fs::package_db::golang::gowork::WorkspaceMode;
        let mode = WorkspaceMode::Malformed {
            reason: "missing-use-close-paren".to_string(),
        };
        let meta = build_metadata(
            "myapp", "0.1.0", GenerationContext::FilesystemScan,
            &[], &[], &TraceIntegrity::default(),
            None, None, &[],
            &RootComponentOverride::default(),
            &waybill::binding::user_metadata::UserMetadata::default(),
            None, None, None, None,
            &crate::generate::graph_completeness::GraphCompletenessResult::trivially_complete(),
            None,
            Some(&mode),
            None,
            None,
            None,
            None,
            None,
        );
        let props = meta["properties"].as_array().expect("properties array");
        let c112 = props.iter().find(|p| p["name"] == "waybill:go-workspace-mode");
        assert_eq!(
            c112.expect("C112 present").as_object().unwrap()["value"],
            "malformed: missing-use-close-paren",
        );
    }

    #[test]
    fn metadata_context_varies_per_variant() {
        let fs = build_metadata("myapp", "1.0", GenerationContext::FilesystemScan, &[], &[], &TraceIntegrity::default(), None, None, &[], &RootComponentOverride::default(), &waybill::binding::user_metadata::UserMetadata::default(), None, None, None, None, &crate::generate::graph_completeness::GraphCompletenessResult::trivially_complete(), None, None, None, None, None, None, None);
        assert_eq!(fs["properties"][0]["value"], "filesystem-scan");

        let img = build_metadata("myapp", "1.0", GenerationContext::ContainerImageScan, &[], &[], &TraceIntegrity::default(), None, None, &[], &RootComponentOverride::default(), &waybill::binding::user_metadata::UserMetadata::default(), None, None, None, None, &crate::generate::graph_completeness::GraphCompletenessResult::trivially_complete(), None, None, None, None, None, None, None);
        assert_eq!(img["properties"][0]["value"], "container-image-scan");
    }

    #[test]
    fn metadata_omits_lifecycles_when_no_tiers_present() {
        // A component without a sbom_tier value contributes nothing.
        let meta = build_metadata(
            "myapp",
            "0.1.0",
            GenerationContext::BuildTimeTrace,
            &[],
            &[],
            &TraceIntegrity::default(),
        None,
        None,
        &[],
        &RootComponentOverride::default(),
        &waybill::binding::user_metadata::UserMetadata::default(),
        None,
            None,
        None,
        None,
            &crate::generate::graph_completeness::GraphCompletenessResult::trivially_complete(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert!(meta.get("lifecycles").is_none());
    }

    #[test]
    fn metadata_aggregates_lifecycles_from_component_tiers() {
        use waybill_common::resolution::{
            ResolutionEvidence, ResolutionTechnique, ResolvedComponent,
        };
        use waybill_common::types::purl::Purl;

        let mk = |purl: &str, tier: &str| ResolvedComponent {
            build_inclusion: None,
            purl: Purl::new(purl).expect("valid purl"),
            name: "x".to_string(),
            version: "1.0".to_string(),
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
            requirement_ranges: Vec::new(),
            source_type: None,
            sbom_tier: Some(tier.to_string()),
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
        };

        let components = vec![
            mk("pkg:deb/debian/jq@1.6", "deployed"),
            mk("pkg:pypi/requests@2.31.0", "source"),
            mk("pkg:npm/foo@1.0.0", "design"),
            // Duplicate tier should collapse.
            mk("pkg:apk/alpine/musl@1.2.4-r2", "deployed"),
        ];

        let meta = build_metadata(
            "myapp",
            "0.1.0",
            GenerationContext::ContainerImageScan,
            &components,
            &[],
            &TraceIntegrity::default(),
        None,
        None,
        &[],
        &RootComponentOverride::default(),
        &waybill::binding::user_metadata::UserMetadata::default(),
        None,
            None,
        None,
        None,
            &crate::generate::graph_completeness::GraphCompletenessResult::trivially_complete(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );

        let lifecycles = meta["lifecycles"]
            .as_array()
            .expect("lifecycles array");
        let phases: Vec<&str> = lifecycles
            .iter()
            .map(|p| p["phase"].as_str().unwrap())
            .collect();

        // Sorted alphabetically, duplicates collapsed.
        assert_eq!(phases, vec!["design", "operations", "pre-build"]);
    }

    #[test]
    fn metadata_unknown_tier_is_dropped_from_lifecycles() {
        use waybill_common::resolution::{
            ResolutionEvidence, ResolutionTechnique, ResolvedComponent,
        };
        use waybill_common::types::purl::Purl;

        let c = ResolvedComponent {
            build_inclusion: None,
            purl: Purl::new("pkg:generic/weird@1.0").expect("valid purl"),
            name: "weird".to_string(),
            version: "1.0".to_string(),
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
            requirement_ranges: Vec::new(),
            source_type: None,
            sbom_tier: Some("nonsense-tier".to_string()),
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
        };

        let meta = build_metadata(
            "myapp",
            "0.1.0",
            GenerationContext::BuildTimeTrace,
            std::slice::from_ref(&c),
            &[],
            &TraceIntegrity::default(),
        None,
        None,
        &[],
        &RootComponentOverride::default(),
        &waybill::binding::user_metadata::UserMetadata::default(),
        None,
            None,
        None,
        None,
            &crate::generate::graph_completeness::GraphCompletenessResult::trivially_complete(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert!(
            meta.get("lifecycles").is_none(),
            "unknown tier should not produce a lifecycle entry"
        );
    }

    // -------- Milestone 073 — source identifier emission --------

    #[test]
    fn metadata_emits_builtin_identifier_in_external_references() {
        use waybill::binding::identifiers::Identifier;
        let auto = {
            let mut id = Identifier::parse("repo:git@github.com:foo/bar.git").unwrap();
            id.source_label = Some("auto-detected from git remote `origin`".to_string());
            id
        };
        let meta = build_metadata(
            "myapp",
            "0.1.0",
            GenerationContext::FilesystemScan,
            &[],
            &[],
            &TraceIntegrity::default(),
            None,
            None,
            std::slice::from_ref(&auto),
            &RootComponentOverride::default(),
            &waybill::binding::user_metadata::UserMetadata::default(),
        None,
            None,
        None,
        None,
            &crate::generate::graph_completeness::GraphCompletenessResult::trivially_complete(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        let refs = meta["component"]["externalReferences"]
            .as_array()
            .expect("externalReferences emitted");
        let vcs_entry = refs
            .iter()
            .find(|r| r.get("type").and_then(|v| v.as_str()) == Some("vcs"))
            .expect("vcs entry present");
        assert_eq!(
            vcs_entry["url"].as_str(),
            Some("git@github.com:foo/bar.git")
        );
        assert_eq!(
            vcs_entry["comment"].as_str(),
            Some("auto-detected from git remote `origin`")
        );
    }

    #[test]
    fn metadata_emits_user_defined_identifier_in_properties() {
        use waybill::binding::identifiers::Identifier;
        let m1 = Identifier::parse("acme_corp_id:abc123").unwrap();
        let m2 = Identifier::parse("internal_ticket:PROJ-456").unwrap();
        let ids = vec![m1, m2];
        let meta = build_metadata(
            "myapp",
            "0.1.0",
            GenerationContext::FilesystemScan,
            &[],
            &[],
            &TraceIntegrity::default(),
            None,
            None,
            &ids,
            &RootComponentOverride::default(),
            &waybill::binding::user_metadata::UserMetadata::default(),
        None,
            None,
        None,
        None,
            &crate::generate::graph_completeness::GraphCompletenessResult::trivially_complete(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        let props = meta["properties"].as_array().expect("properties");
        let entry = props
            .iter()
            .find(|p| p.get("name").and_then(|v| v.as_str()) == Some("waybill:identifiers"))
            .expect("waybill:identifiers entry present");
        let value_str = entry["value"].as_str().unwrap();
        let parsed: serde_json::Value =
            serde_json::from_str(value_str).expect("value is JSON-encoded array");
        let arr = parsed.as_array().expect("array");
        assert_eq!(arr.len(), 2);
        // Sorted lex by (scheme, value): acme_corp_id < internal_ticket.
        assert_eq!(arr[0]["scheme"].as_str(), Some("acme_corp_id"));
        assert_eq!(arr[1]["scheme"].as_str(), Some("internal_ticket"));
    }

    #[test]
    fn metadata_omits_user_defined_property_when_set_is_empty() {
        let meta = build_metadata(
            "myapp",
            "0.1.0",
            GenerationContext::FilesystemScan,
            &[],
            &[],
            &TraceIntegrity::default(),
            None,
            None,
            &[],
            &RootComponentOverride::default(),
            &waybill::binding::user_metadata::UserMetadata::default(),
        None,
            None,
        None,
        None,
            &crate::generate::graph_completeness::GraphCompletenessResult::trivially_complete(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        let props = meta["properties"].as_array().expect("properties");
        let found = props
            .iter()
            .any(|p| p.get("name").and_then(|v| v.as_str()) == Some("waybill:identifiers"));
        assert!(!found, "annotation must be absent when no user-defined identifiers");
    }
}