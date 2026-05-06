//! SPDX 3.0.1 JSON-LD document builder (milestone 011).
//!
//! Top-level entry point — composes Packages, Relationships,
//! LicenseExpressions, Agents, Annotations, and the SpdxDocument
//! root element into one `@graph`. Per `data-model.md` §"Element
//! catalog" + §"Deterministic ordering rules".
//!
//! See `specs/011-spdx-3-full-support/data-model.md` for the
//! authoritative element catalog.

use data_encoding::BASE32_NOPAD;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use mikebom_common::resolution::ResolvedComponent;

use crate::generate::{OutputConfig, ScanArtifacts};

const SPDX_3_CONTEXT: &str = "https://spdx.org/rdf/3.0.1/spdx-context.jsonld";
const IRI_BASE: &str = "https://mikebom.kusari.dev/spdx3/";
const CREATION_INFO_ID: &str = "_:creation-info";

/// Build a complete SPDX 3.0.1 JSON-LD document from a scan.
///
/// `@graph` ordering (data-model.md §"Deterministic ordering rules"):
/// 1. CreationInfo (single)
/// 2. Tool
/// 3. SpdxDocument (+ optional `externalRef` to the OpenVEX sidecar)
/// 4. software_Package elements (sorted by spdxId)
/// 5. Organization / Person elements (sorted by spdxId)
/// 6. simplelicensing_LicenseExpression elements (sorted by spdxId)
/// 7. Relationship elements (sorted by spdxId)
/// 8. Annotation elements (sorted by spdxId)
///
/// `openvex_locator`: relative path the OpenVEX sidecar will land
/// at on disk, when the scan produced at least one advisory. When
/// `None`, no ExternalRef is injected. The sidecar itself is built
/// + emitted by the serializer wrapper in `mod.rs`, not here.
pub fn build_document(
    scan: &ScanArtifacts<'_>,
    cfg: &OutputConfig,
    openvex_locator: Option<&str>,
) -> anyhow::Result<Value> {
    // Milestone 077 — when override is active, build a filtered view
    // of `scan` that drops manifest-derived main-module components
    // BEFORE per-package emission (clean replacement per Q2 / FR-008).
    // The downstream pick_root_iri then synthesizes a root from the
    // override values verbatim.
    let override_active = scan.root_override.is_active();
    let filtered_components_owned: Option<Vec<ResolvedComponent>> =
        if override_active {
            let mut keep: Vec<ResolvedComponent> = Vec::with_capacity(scan.components.len());
            for c in scan.components.iter() {
                let is_main_module = c
                    .extra_annotations
                    .get("mikebom:component-role")
                    .and_then(|v| v.as_str())
                    == Some("main-module");
                if is_main_module {
                    tracing::info!(
                        purl = %c.purl,
                        "override is set; dropping manifest-derived main-module component '{}' from emitted SBOM (per milestone 077 clean-replacement; see GitHub issue #151)",
                        c.purl
                    );
                } else {
                    keep.push(c.clone());
                }
            }
            tracing::info!(
                name = scan.root_override.name.as_deref().unwrap_or(scan.target_name),
                version = scan.root_override.version.as_deref().unwrap_or("0.0.0"),
                "root component override active (SPDX 3): name='{}', version='{}'",
                scan.root_override.name.as_deref().unwrap_or(scan.target_name),
                scan.root_override.version.as_deref().unwrap_or("0.0.0"),
            );
            Some(keep)
        } else {
            None
        };
    let view_scan_storage: ScanArtifacts<'_>;
    let scan: &ScanArtifacts<'_> = if let Some(ref filtered) = filtered_components_owned {
        view_scan_storage = ScanArtifacts {
            target_name: scan.target_name,
            components: filtered.as_slice(),
            relationships: scan.relationships,
            integrity: scan.integrity,
            complete_ecosystems: scan.complete_ecosystems,
            os_release_missing_fields: scan.os_release_missing_fields,
            scan_target_coord: scan.scan_target_coord,
            generation_context: scan.generation_context.clone(),
            include_dev: scan.include_dev,
            include_hashes: scan.include_hashes,
            include_source_files: scan.include_source_files,
            scope_mode: scan.scope_mode,
            go_graph_completeness: scan.go_graph_completeness,
            go_graph_completeness_reason: scan.go_graph_completeness_reason,
            source_document_binding: scan.source_document_binding,
            identifiers: scan.identifiers,
            component_identifiers: scan.component_identifiers,
            root_override: scan.root_override.clone(),
        };
        &view_scan_storage
    } else {
        scan
    };

    let fingerprint = scan_fingerprint(scan, cfg);
    let doc_iri = format!("{IRI_BASE}doc-{fingerprint}");
    let tool_iri = format!("{doc_iri}/tool/mikebom");
    let created = cfg
        .created
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    let mut graph: Vec<Value> = Vec::new();

    // 1. CreationInfo.
    graph.push(json!({
        "type": "CreationInfo",
        "@id": CREATION_INFO_ID,
        "specVersion": "3.0.1",
        "created": created,
        "createdBy": [tool_iri],
    }));

    // 2. Tool.
    graph.push(json!({
        "type": "Tool",
        "spdxId": tool_iri,
        "creationInfo": CREATION_INFO_ID,
        "name": format!("mikebom-{}", cfg.mikebom_version),
    }));

    // Two-pass Package build: (a) precompute the PURL → IRI
    // lookup, (b) build agents against the lookup, (c) build
    // Packages with agent attachments inlined.
    let package_iri_by_purl =
        super::v3_packages::build_iri_lookup(scan.components, &doc_iri);

    let agent_build = super::v3_agents::build_agents(
        scan.components,
        &package_iri_by_purl,
        &doc_iri,
        CREATION_INFO_ID,
    );

    // Milestone 076 — track per-component identifier matches so
    // unmatched selectors warn after build_packages completes
    // (FR-010 / VR-076-004).
    let mut match_counts: std::collections::BTreeMap<usize, usize> =
        std::collections::BTreeMap::new();
    for i in 0..scan.component_identifiers.len() {
        match_counts.insert(i, 0);
    }
    let (mut packages, _) = super::v3_packages::build_packages(
        scan.components,
        &doc_iri,
        CREATION_INFO_ID,
        &agent_build.attachments,
        scan.component_identifiers,
        &mut match_counts,
    );
    for (idx, count) in &match_counts {
        if *count == 0 {
            let flag = &scan.component_identifiers[*idx];
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

    // Choose root element. If no Package matches the scan target
    // and the scan is non-empty, fall back to the first Package.
    // Empty-scan case: synthesize a root Package so the document
    // is still structurally valid (matches SPDX 2.3 path's
    // synthesize-root behavior for sbomqs parity).
    let (root_iris, synthetic_root_added) = pick_root_iri(
        scan,
        &doc_iri,
        &package_iri_by_purl,
        &mut packages,
        scan.components,
    );

    // 3. SpdxDocument (placed in the graph before the per-element
    // sections so a JSON-walker reading top-down hits the document
    // shape early). When the scan produced OpenVEX advisories, an
    // ExternalRef pointing at the sidecar is attached here —
    // clarification Q1 / FR-014: SPDX 3 cross-references the
    // OpenVEX sidecar via an `externalRef` on the document element,
    // using the VEX-precise enum value `vulnerabilityExploitability
    // Assessment` (the most specific match in SPDX 3.0.1's
    // `prop_ExternalRef_externalRefType` enum for an OpenVEX
    // payload).
    // Document-level scope hint (milestone 047) — same prose the
    // SPDX 2.3 path emits in `creationInfo.comment`. Per the
    // SPDX 3.0.1 model docs, Element-level `comment` is "comments
    // by the creator of the Element about the Element"; on the
    // SpdxDocument that's exactly the document-level scope note.
    // The shared `spdx-context.jsonld` already maps the
    // unprefixed `comment` key, so no @context change needed.
    let scope_comment = super::document::build_scope_comment(scan);
    // Milestone 077 — when override is active, the document's `name`
    // field reflects the operator-supplied identity (consistent with
    // the root element's name). When inactive, the auto-derived
    // `target_name` is preserved (alpha.17 byte-identity).
    let document_name: &str = if scan.root_override.is_active() {
        scan.root_override
            .name
            .as_deref()
            .unwrap_or(scan.target_name)
    } else {
        scan.target_name
    };
    let mut spdx_document = json!({
        "type": "SpdxDocument",
        "spdxId": doc_iri,
        "creationInfo": CREATION_INFO_ID,
        "name": document_name,
        "dataLicense": "https://spdx.org/licenses/CC0-1.0",
        "rootElement": root_iris.clone(),
        "comment": scope_comment,
    });
    if let Some(locator) = openvex_locator {
        spdx_document["externalRef"] = json!([
            {
                "type": "ExternalRef",
                "externalRefType": "vulnerabilityExploitabilityAssessment",
                "contentType": "application/openvex+json",
                "locator": [locator],
                "comment": "OpenVEX 0.2.0 sidecar produced by mikebom",
            }
        ]);
    }
    // Milestone 073 — identifiers ride
    // `Element.externalIdentifier[]` natively per
    // `contracts/identifiers-annotation.md` C-1 SPDX 3 (the
    // open-typed multi-identifier model handles BOTH built-in and
    // user-defined schemes uniformly — no separate annotation
    // envelope needed on the SPDX 3 side). Order: auto-detected
    // first, then manual in supply order (per FR-009 / VR-008).
    if !scan.identifiers.is_empty() {
        let id_entries: Vec<Value> = scan
            .identifiers
            .iter()
            .map(|id| {
                let mut entry = json!({
                    "type": "ExternalIdentifier",
                    "externalIdentifierType": id.scheme.as_str(),
                    "identifier": id.value.as_str(),
                });
                if let Some(label) = id.source_label.as_deref() {
                    entry["comment"] = json!(label);
                } else {
                    entry["comment"] = json!("manual identifier flag");
                }
                entry
            })
            .collect();
        spdx_document["externalIdentifier"] = json!(id_entries);
    }

    // Milestone 072 / T014 — when --bind-to-source was used, attach
    // the standards-native `import[]` ExternalMap pointing at the
    // source-tier SBOM. The `Relationship[built_from]` graph element
    // is appended into `all_relationships` further below (so it
    // sorts with the other relationship records).
    let built_from_rel: Option<Value> = if let Some(source_id) =
        scan.source_document_binding
    {
        let source_iri = source_id
            .iri
            .clone()
            .unwrap_or_else(|| format!("urn:sha256:{}", source_id.sha256));
        spdx_document["import"] = json!([
            {
                "type": "ExternalMap",
                "externalSpdxId": source_iri.clone(),
                "verifiedUsing": [
                    {
                        "type": "Hash",
                        "algorithm": "sha256",
                        "hashValue": source_id.sha256.clone(),
                    }
                ],
            }
        ]);
        let rel_iri = format!("{}/relationship/built-from-source", doc_iri);
        Some(json!({
            "type": "Relationship",
            "spdxId": rel_iri,
            "creationInfo": CREATION_INFO_ID,
            "from": doc_iri.clone(),
            "to": [source_iri],
            "relationshipType": "built_from",
            "comment": "milestone-072 cross-tier binding: this build/deployment was produced from the source-tier SBOM referenced by the import[] ExternalMap above",
        }))
    } else {
        None
    };
    graph.push(spdx_document);

    // 4 (cont). Append the Package elements.
    for pkg in packages {
        graph.push(pkg);
    }

    // 5. Organization / Person Agent elements. (Supplier/originator
    //    attachments are already inlined on Packages above; no
    //    Relationship edges needed — SPDX 3 puts these as
    //    Artifact_props fields.)
    for agent in agent_build.elements {
        graph.push(agent);
    }

    // 6. simplelicensing_LicenseExpression elements + their
    //    Relationships.
    let (license_elements, license_relationships) =
        super::v3_licenses::build_license_elements_and_relationships(
            scan.components,
            &package_iri_by_purl,
            &doc_iri,
            CREATION_INFO_ID,
        );
    for elem in license_elements {
        graph.push(elem);
    }

    // 7. Relationship elements — dependency edges, containment edges,
    //    license/agent edges, document-describes edge. Combined into
    //    one bucket so they sort together by spdxId.
    let mut all_relationships: Vec<Value> = Vec::new();
    all_relationships.extend(super::v3_relationships::build_dependency_relationships(
        scan.relationships,
        &package_iri_by_purl,
        &doc_iri,
        CREATION_INFO_ID,
    ));
    all_relationships.extend(super::v3_relationships::build_containment_relationships(
        scan.components,
        &package_iri_by_purl,
        &doc_iri,
        CREATION_INFO_ID,
    ));
    all_relationships.extend(license_relationships);
    if !synthetic_root_added {
        let describes_rels = super::v3_relationships::build_describes_relationships(
            &doc_iri,
            &root_iris,
            CREATION_INFO_ID,
        );
        all_relationships.extend(describes_rels);
    }
    // Milestone 072 / T014 — append the cross-tier binding's
    // `built_from` Relationship into the sortable bucket so it
    // sorts with peers.
    if let Some(rel) = built_from_rel {
        all_relationships.push(rel);
    }
    all_relationships.sort_by(|a, b| {
        let key = |v: &Value| v["spdxId"].as_str().unwrap_or("").to_string();
        key(a).cmp(&key(b))
    });
    for rel in all_relationships {
        graph.push(rel);
    }

    // 8. Annotation elements — component-level (C1–C20 + D1/D2)
    //    + document-level (C21–C23 + E1).
    let mut annotations: Vec<Value> =
        super::v3_annotations::build_component_annotations(
            scan.components,
            &package_iri_by_purl,
            &doc_iri,
            CREATION_INFO_ID,
            scan.include_dev,
            scan.include_source_files,
        );
    annotations.extend(super::v3_annotations::build_document_annotations(
        scan,
        &doc_iri,
        CREATION_INFO_ID,
    ));
    annotations.sort_by(|a, b| {
        let key = |v: &Value| v["spdxId"].as_str().unwrap_or("").to_string();
        key(a).cmp(&key(b))
    });
    for anno in annotations {
        graph.push(anno);
    }

    Ok(json!({
        "@context": SPDX_3_CONTEXT,
        "@graph": graph,
    }))
}

/// Pick the root Package IRI. Preference order:
/// 0. **Milestone 077** — when `scan.root_override.is_active()`,
///    ALWAYS synthesize a root using the override values. The
///    manifest-derived main-modules have already been filtered out
///    of the components slice by the time this runs (clean replacement
///    per Q2 clarification).
/// 1. A Package whose name matches `scan.target_name`.
/// 2. The first Package in the (already sorted) packages list.
/// 3. Synthesize a root Package and prepend it — used for the
///    empty-scan case + the scan-target-isn't-a-package case
///    (e.g., scanning a directory whose name doesn't match any
///    discovered component).
///
/// Returns `(root_iri, synthetic_root_added)`.
fn pick_root_iri(
    scan: &ScanArtifacts<'_>,
    doc_iri: &str,
    package_iri_by_purl: &std::collections::BTreeMap<String, String>,
    packages: &mut Vec<Value>,
    components: &[ResolvedComponent],
) -> (Vec<String>, bool) {
    // Milestone 077 — override path takes precedence over every
    // auto-derivation step. The synthesized root carries the operator-
    // supplied identity AND its PURL uses RFC 3986 percent-encoding
    // (research §1) so npm-scoped names round-trip correctly.
    if scan.root_override.is_active() {
        let name = scan
            .root_override
            .name
            .as_deref()
            .unwrap_or(scan.target_name);
        let version = scan.root_override.version.as_deref().unwrap_or("0.0.0");
        let purl_name = crate::generate::percent_encode_purl_name(name);
        let purl_version = crate::generate::percent_encode_purl_name(version);
        let synth_purl = format!("pkg:generic/{purl_name}@{purl_version}");
        let synth_iri = format!(
            "{doc_iri}/pkg-root-{}",
            hash_prefix(synth_purl.as_bytes(), 16)
        );
        // CPE: reuse `url_friendly` for sanitization parity with the
        // existing non-override path. CPE has its own escape rules
        // distinct from RFC 3986 percent-encoding.
        let synth_cpe = format!(
            "cpe:2.3:a:mikebom:{}:{}:*:*:*:*:*:*:*",
            url_friendly(name),
            url_friendly(version),
        );
        let synth_pkg = json!({
            "type": "software_Package",
            "spdxId": synth_iri,
            "creationInfo": CREATION_INFO_ID,
            "name": name,
            "software_packageVersion": version,
            "software_packageUrl": synth_purl,
            "externalIdentifier": [
                {
                    "type": "ExternalIdentifier",
                    "externalIdentifierType": "cpe23",
                    "identifier": synth_cpe,
                },
                {
                    "type": "ExternalIdentifier",
                    "externalIdentifierType": "packageUrl",
                    "identifier": synth_purl,
                },
            ],
        });
        packages.insert(0, synth_pkg);
        return (vec![synth_iri], true);
    }

    // Milestones 053 (Go) + 064 (cargo) FR-008 + #127: prefer
    // main-module-tagged components as root elements. SPDX 3.0.1's
    // `rootElement` is a plural array, so multi-main-module
    // workspaces (cargo workspace members, polyglot scans) emit one
    // root entry per main-module.
    let main_module_iris: Vec<String> = components
        .iter()
        .filter(|c| {
            c.parent_purl.is_none()
                && c.extra_annotations
                    .get("mikebom:component-role")
                    .and_then(|v| v.as_str())
                    == Some("main-module")
        })
        .filter_map(|c| package_iri_by_purl.get(c.purl.as_str()).cloned())
        .collect();
    if !main_module_iris.is_empty() {
        let mut sorted = main_module_iris;
        sorted.sort();
        return (sorted, false);
    }

    if let Some(c) = components.iter().find(|c| c.name == scan.target_name) {
        if let Some(iri) = package_iri_by_purl.get(c.purl.as_str()) {
            return (vec![iri.clone()], false);
        }
    }

    // Synthesize a root Package. Mirrors the SPDX 2.3 emitter's
    // synthesize_root behavior — preserves sbomqs scoring parity
    // (a document with no rootElement scores worse).
    let synth_purl = format!("pkg:generic/{}@0.0.0", url_friendly(scan.target_name));
    let synth_iri = format!(
        "{doc_iri}/pkg-root-{}",
        hash_prefix(synth_purl.as_bytes(), 16)
    );
    let synth_cpe = format!(
        "cpe:2.3:a:mikebom:{}:0.0.0:*:*:*:*:*:*:*",
        url_friendly(scan.target_name)
    );
    let synth_pkg = json!({
        "type": "software_Package",
        "spdxId": synth_iri,
        "creationInfo": CREATION_INFO_ID,
        "name": scan.target_name,
        "software_packageVersion": "0.0.0",
        "software_packageUrl": synth_purl,
        "externalIdentifier": [
            {
                "type": "ExternalIdentifier",
                "externalIdentifierType": "cpe23",
                "identifier": synth_cpe,
            },
            {
                "type": "ExternalIdentifier",
                "externalIdentifierType": "packageUrl",
                "identifier": synth_purl,
            },
        ],
    });
    packages.insert(0, synth_pkg);
    (vec![synth_iri], true)
}

/// Replace characters that aren't legal in a PURL name with `-`.
fn url_friendly(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

fn hash_prefix(input: &[u8], chars: usize) -> String {
    let digest = Sha256::digest(input);
    let encoded = BASE32_NOPAD.encode(&digest);
    encoded[..chars].to_string()
}

/// Stable scan fingerprint — same inputs the SPDX 2.3
/// `documentNamespace` and the milestone-010 stub use, so re-runs
/// produce the same document IRI (FR-015 / SC-006).
fn scan_fingerprint(scan: &ScanArtifacts<'_>, cfg: &OutputConfig) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"spdx3\n");
    hasher.update(b"target=");
    hasher.update(scan.target_name.as_bytes());
    hasher.update(b"\nmikebom=");
    hasher.update(cfg.mikebom_version.as_bytes());
    hasher.update(b"\npurls=");
    let mut purls: Vec<&str> =
        scan.components.iter().map(|c| c.purl.as_str()).collect();
    purls.sort_unstable();
    for p in purls {
        hasher.update(p.as_bytes());
        hasher.update(b"\n");
    }
    let digest = hasher.finalize();
    BASE32_NOPAD.encode(&digest)[..24].to_string()
}
