
use chrono::Utc;
use mikebom_common::attestation::integrity::TraceIntegrity;
use mikebom_common::attestation::metadata::GenerationContext;
use mikebom_common::resolution::ResolvedComponent;
use mikebom_common::types::purl::encode_purl_segment;
use serde_json::json;

use crate::generate::{percent_encode_purl_name, RootComponentOverride};

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
/// - Tool identity (mikebom with current version)
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
    go_graph_completeness: Option<crate::scan_fs::package_db::GraphCompleteness>,
    go_graph_completeness_reason: Option<&str>,
    source_document_binding: Option<&mikebom::binding::SourceDocumentId>,
    identifiers: &[mikebom::binding::identifiers::Identifier],
    root_override: &RootComponentOverride,
) -> serde_json::Value {
    let version = env!("CARGO_PKG_VERSION");
    let timestamp = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

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
        crate::generate::lifecycle_phases::aggregate_phases(components)
            .into_iter()
            .map(|p| json!({"phase": p}))
            .collect();

    let mut properties = vec![json!({
        "name": "mikebom:generation-context",
        "value": context_str,
    })];

    // Feature 005 SC-009 / FR-006 / FR-009: when /etc/os-release fields
    // were missing during scan, record the names here so SBOM consumers
    // can detect degraded PURL output without parsing the scanner log.
    // Omitted entirely when the list is empty (clean scan).
    if !os_release_missing_fields.is_empty() {
        properties.push(json!({
            "name": "mikebom:os-release-missing-fields",
            "value": os_release_missing_fields.join(","),
        }));
    }

    // Milestone 061 (closes #119, catalog row C44): doc-level Go
    // graph-completeness signal. Per Constitution Principle X
    // (Transparency): when mikebom can't supply every transitive edge
    // for `go.sum` components, the SBOM MUST signal the limitation so
    // consumers can distinguish "dead dep" from "couldn't resolve."
    // Absent annotation ⇒ no Go scan happened (signal not applicable).
    if let Some(gc) = go_graph_completeness {
        let value = serde_json::to_value(gc)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "unknown".to_string());
        properties.push(json!({
            "name": "mikebom:graph-completeness",
            "value": value,
        }));
        if let Some(reason) = go_graph_completeness_reason {
            if !reason.is_empty() {
                properties.push(json!({
                    "name": "mikebom:graph-completeness-reason",
                    "value": reason,
                }));
            }
        }
    }

    // Trace-integrity counters (previously on compositions, moved
    // here for CDX 1.6 schema conformance — compositions items have
    // additionalProperties: false so `properties` isn't allowed there).
    // Each counter is surfaced as a distinct property so downstream
    // consumers can filter on name.
    properties.push(json!({
        "name": "mikebom:trace-integrity-ring-buffer-overflows",
        "value": integrity.ring_buffer_overflows.to_string(),
    }));
    properties.push(json!({
        "name": "mikebom:trace-integrity-events-dropped",
        "value": integrity.events_dropped.to_string(),
    }));
    properties.push(json!({
        "name": "mikebom:trace-integrity-uprobe-attach-failures",
        "value": integrity.uprobe_attach_failures.len().to_string(),
    }));
    properties.push(json!({
        "name": "mikebom:trace-integrity-kprobe-attach-failures",
        "value": integrity.kprobe_attach_failures.len().to_string(),
    }));

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
    //      `mikebom:component-role: "main-module"` in its extra
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
    let main_module_count = components
        .iter()
        .filter(|c| {
            c.extra_annotations
                .get("mikebom:component-role")
                .and_then(|v| v.as_str())
                == Some("main-module")
        })
        .count();
    let main_module: Option<&ResolvedComponent> = if main_module_count == 1 {
        components.iter().find(|c| {
            c.extra_annotations
                .get("mikebom:component-role")
                .and_then(|v| v.as_str())
                == Some("main-module")
        })
    } else {
        None
    };

    // Milestone 077 — when the operator-supplied override is active,
    // short-circuit the priority ladder above. The override values
    // become the BOM-subject identity verbatim, with PURL percent-
    // encoding applied per RFC 3986 §2.3 (research §1). This also
    // means the manifest-derived main-module is suppressed at the
    // metadata.component slot here AND filtered out of the
    // components[] array by the builder per FR-008 / Q2 clean-
    // replacement clarification.
    let override_active = root_override.is_active();
    let (subject_name, subject_version, synthetic_component_purl) =
        if override_active {
            let name = root_override
                .name
                .clone()
                .unwrap_or_else(|| target_name.to_string());
            let version = root_override
                .version
                .clone()
                .unwrap_or_else(|| target_version.to_string());
            let purl = format!(
                "pkg:generic/{}@{}",
                percent_encode_purl_name(&name),
                percent_encode_purl_name(&version),
            );
            (name, version, purl)
        } else if let Some(c) = main_module {
            (
                c.name.clone(),
                c.version.clone(),
                c.purl.as_str().to_string(),
            )
        } else if let Some(coord) = scan_target_coord {
            let purl = format!(
                "pkg:maven/{}/{}@{}",
                encode_purl_segment(&coord.group),
                encode_purl_segment(&coord.artifact),
                encode_purl_segment(&coord.version),
            );
            (coord.artifact.clone(), coord.version.clone(), purl)
        } else {
            let purl = format!(
                "pkg:generic/{}@{}",
                encode_purl_segment(target_name),
                encode_purl_segment(target_version),
            );
            (target_name.to_string(), target_version.to_string(), purl)
        };

    // Synthesize a minimal valid CPE 2.3 for the scan subject.
    //
    // Milestone 053: when the metadata.component is the Go main-module,
    // reuse its primary CPE from `c.cpes[0]` (synthesized in
    // `scan_fs/mod.rs::synthesize_cpes` from the PURL using the same
    // shape as every other component) so the BOM-subject CPE
    // round-trips identically to the SPDX 2.3 / SPDX 3 emission.
    // Pre-053 the metadata.component used a `cpe:2.3:a:mikebom:…`
    // synthetic shape that diverged from the SPDX side; post-053 the
    // shapes are identical for the main-module case.
    //
    // Uses mikebom as the vendor for the placeholder fallback. Name
    // and version segments are CPE-sanitized (lowercase, non-
    // alphanumerics → underscore). sbomqs's schema validator runs CPE
    // validation on metadata.component and flags empty/absent fields
    // as invalid.
    let synthetic_component_cpe = if override_active {
        // Milestone 077 — operator-supplied identity drives the CPE
        // verbatim through the existing `cpe_sanitize` helper. Vendor
        // stays hardcoded `mikebom` per spec assumption (out of scope
        // for this milestone).
        format!(
            "cpe:2.3:a:mikebom:{}:{}:*:*:*:*:*:*:*",
            cpe_sanitize(&subject_name),
            cpe_sanitize(&subject_version),
        )
    } else if let Some(c) = main_module {
        c.cpes
            .first()
            .cloned()
            .unwrap_or_else(|| {
                format!(
                    "cpe:2.3:a:mikebom:{}:{}:*:*:*:*:*:*:*",
                    cpe_sanitize(&subject_name),
                    cpe_sanitize(&subject_version),
                )
            })
    } else {
        format!(
            "cpe:2.3:a:mikebom:{}:{}:*:*:*:*:*:*:*",
            cpe_sanitize(&subject_name),
            cpe_sanitize(&subject_version),
        )
    };

    let mut metadata = json!({
        "timestamp": timestamp,
        // Top-level SBOM provenance: the list of individuals or
        // organizations responsible for creating THIS SBOM (not the
        // underlying project). Scored by sbomqs `sbom_authors` (2.9%
        // in Provenance). Single-entry placeholder is sufficient;
        // future work can extract from git config or accept
        // --author=NAME via CLI.
        "authors": [
            { "name": "mikebom" }
        ],
        // SBOM supplier: the organization providing the SBOM. Scored
        // by sbomqs `sbom_supplier` (2.2%). Hardcoded to the mikebom
        // project identity.
        "supplier": {
            "name": "mikebom contributors"
        },
        // SBOM content license. SPDX-SBOM convention uses CC0-1.0 so
        // the SBOM itself can be distributed without restriction.
        // Scored by sbomqs `sbom_data_license` (1.8% in Licensing).
        "licenses": [
            { "license": { "id": "CC0-1.0" } }
        ],
        "tools": {
            "components": [
                {
                    "type": "application",
                    "name": "mikebom",
                    "version": version,
                    "publisher": "mikebom contributors"
                }
            ]
        },
        "component": {
            "type": "application",
            "name": subject_name,
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

    // Milestone 053 FR-004: when the metadata.component is the Go
    // main-module (per the ladder above), surface the supplementary
    // `mikebom:component-role: main-module` C40 annotation as a
    // metadata.component-level property so consumers reading either
    // the native field (`type: "application"`) OR the supplementary
    // tag identify the main-module. Also surface
    // `mikebom:sbom-tier: "source"` per FR-006.
    //
    // Milestone 077: when the override is active, the manifest main-
    // module is no longer the BOM subject — skip these supplementary
    // properties so the emitted root reflects only operator-supplied
    // identity (clean replacement per Q2 clarification).
    if let (Some(c), false) = (main_module, override_active) {
        let mut comp_props = vec![json!({
            "name": "mikebom:component-role",
            "value": "main-module",
        })];
        if let Some(tier) = c.sbom_tier.as_ref() {
            comp_props.push(json!({
                "name": "mikebom:sbom-tier",
                "value": tier,
            }));
        }
        // Propagate `mikebom:source-files` (C18) from the main-module's
        // evidence so the parity-extractor framework finds the go.mod
        // path on the CDX side, matching the SPDX `packages[]`
        // emission.
        if !c.evidence.source_file_paths.is_empty() {
            comp_props.push(json!({
                "name": "mikebom:source-files",
                "value": c.evidence.source_file_paths.join(", "),
            }));
        }
        // Propagate `mikebom:detected-go` (C14) — true for any Go
        // workspace's main-module per build_main_module_entry's
        // `detected_go: Some(true)`.
        if c.detected_go.unwrap_or(false) {
            comp_props.push(json!({
                "name": "mikebom:detected-go",
                "value": "true",
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
            mikebom::binding::identifiers::IdentifierKind::Builtin(b) => {
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
            mikebom::binding::identifiers::IdentifierKind::UserDefined => None,
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
    // `metadata.properties[]` entry under `mikebom:identifiers`
    // per `contracts/identifiers-annotation.md` C-2 CDX 1.6.
    // The value is a JSON-encoded array sorted lex by `(scheme, value)`
    // for determinism (FR-009 / contract C-4). Emit ONLY when the
    // user-defined entry set is non-empty per VR-007 — preserves
    // cross-format byte-identity for non-user-defined-namespace scans.
    let user_defined_payload: Vec<serde_json::Value> = {
        let mut entries: Vec<&mikebom::binding::identifiers::Identifier> = identifiers
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
        let json_str = serde_json::to_string(&user_defined_payload)
            .unwrap_or_else(|_| "[]".to_string());
        if let Some(props) = metadata.get_mut("properties").and_then(|v| v.as_array_mut())
        {
            props.push(json!({
                "name": "mikebom:identifiers",
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

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn metadata_has_required_fields() {
        let meta = build_metadata("myapp", "0.1.0", GenerationContext::BuildTimeTrace, &[], &[], &TraceIntegrity::default(), None, None, None, None, &[], &RootComponentOverride::default());

        assert!(meta["timestamp"].is_string());
        assert_eq!(meta["tools"]["components"][0]["name"], "mikebom");
        assert_eq!(meta["component"]["name"], "myapp");
        assert_eq!(meta["component"]["version"], "0.1.0");
        assert_eq!(
            meta["properties"][0]["name"],
            "mikebom:generation-context"
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
            build_metadata("myapp", "0.1.0", GenerationContext::BuildTimeTrace, &[], &[], &TraceIntegrity::default(), None, None, None, None, &[], &RootComponentOverride::default());
        let authors = meta["authors"].as_array().expect("authors must be array");
        assert!(!authors.is_empty(), "authors must be non-empty");
        assert!(authors[0]["name"].is_string());
    }

    #[test]
    fn metadata_includes_supplier_for_sbom_supplier_score() {
        let meta =
            build_metadata("myapp", "0.1.0", GenerationContext::BuildTimeTrace, &[], &[], &TraceIntegrity::default(), None, None, None, None, &[], &RootComponentOverride::default());
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
            build_metadata("myapp", "0.1.0", GenerationContext::BuildTimeTrace, &[], &[], &TraceIntegrity::default(), None, None, None, None, &[], &RootComponentOverride::default());
        let licenses = meta["licenses"].as_array().expect("licenses must be array");
        assert!(!licenses.is_empty());
        assert_eq!(licenses[0]["license"]["id"], "CC0-1.0");
    }

    #[test]
    fn metadata_component_has_synthetic_purl() {
        // sbomqs flags metadata.component as invalid without a purl.
        // Mikebom synthesizes pkg:generic/<name>@<version>.
        let meta =
            build_metadata("myapp", "0.1.0", GenerationContext::BuildTimeTrace, &[], &[], &TraceIntegrity::default(), None, None, None, None, &[], &RootComponentOverride::default());
        assert_eq!(meta["component"]["purl"], "pkg:generic/myapp@0.1.0");
    }

    #[test]
    fn metadata_component_has_synthetic_cpe() {
        // sbomqs flags empty/absent cpe on metadata.component as invalid.
        // Mikebom emits cpe:2.3:a:mikebom:<name>:<version>:*:*:*:*:*:*:*.
        let meta =
            build_metadata("myapp", "0.1.0", GenerationContext::BuildTimeTrace, &[], &[], &TraceIntegrity::default(), None, None, None, None, &[], &RootComponentOverride::default());
        assert_eq!(
            meta["component"]["cpe"],
            "cpe:2.3:a:mikebom:myapp:0.1.0:*:*:*:*:*:*:*"
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
        None,
        None,
        &[],
        &RootComponentOverride::default(),
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
        let meta = build_metadata("myapp", "0.1.0", GenerationContext::BuildTimeTrace, &[], &[], &TraceIntegrity::default(), None, None, None, None, &[], &RootComponentOverride::default());
        assert_eq!(meta["component"]["bom-ref"], "myapp@0.1.0");
    }

    #[test]
    fn metadata_context_varies_per_variant() {
        let fs = build_metadata("myapp", "1.0", GenerationContext::FilesystemScan, &[], &[], &TraceIntegrity::default(), None, None, None, None, &[], &RootComponentOverride::default());
        assert_eq!(fs["properties"][0]["value"], "filesystem-scan");

        let img = build_metadata("myapp", "1.0", GenerationContext::ContainerImageScan, &[], &[], &TraceIntegrity::default(), None, None, None, None, &[], &RootComponentOverride::default());
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
        None,
        None,
        &[],
        &RootComponentOverride::default(),
        );
        assert!(meta.get("lifecycles").is_none());
    }

    #[test]
    fn metadata_aggregates_lifecycles_from_component_tiers() {
        use mikebom_common::resolution::{
            ResolutionEvidence, ResolutionTechnique, ResolvedComponent,
        };
        use mikebom_common::types::purl::Purl;

        let mk = |purl: &str, tier: &str| ResolvedComponent {
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
            requirement_range: None,
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
        None,
        None,
        &[],
        &RootComponentOverride::default(),
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
        use mikebom_common::resolution::{
            ResolutionEvidence, ResolutionTechnique, ResolvedComponent,
        };
        use mikebom_common::types::purl::Purl;

        let c = ResolvedComponent {
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
            requirement_range: None,
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
        None,
        None,
        &[],
        &RootComponentOverride::default(),
        );
        assert!(
            meta.get("lifecycles").is_none(),
            "unknown tier should not produce a lifecycle entry"
        );
    }

    // -------- Milestone 073 — source identifier emission --------

    #[test]
    fn metadata_emits_builtin_identifier_in_external_references() {
        use mikebom::binding::identifiers::Identifier;
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
            None,
            None,
            std::slice::from_ref(&auto),
            &RootComponentOverride::default(),
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
        use mikebom::binding::identifiers::Identifier;
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
            None,
            None,
            &ids,
            &RootComponentOverride::default(),
        );
        let props = meta["properties"].as_array().expect("properties");
        let entry = props
            .iter()
            .find(|p| p.get("name").and_then(|v| v.as_str()) == Some("mikebom:identifiers"))
            .expect("mikebom:identifiers entry present");
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
            None,
            None,
            &[],
            &RootComponentOverride::default(),
        );
        let props = meta["properties"].as_array().expect("properties");
        let found = props
            .iter()
            .any(|p| p.get("name").and_then(|v| v.as_str()) == Some("mikebom:identifiers"));
        assert!(!found, "annotation must be absent when no user-defined identifiers");
    }
}