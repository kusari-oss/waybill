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

use crate::generate::spdx::v3_id_type_map::map_scheme_to_vocab;
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
    // Issue #229: capture the dropped main-module PURLs so the
    // post-`pick_root_iri` alias step (below) can map them to the
    // synthesized root IRI in `package_iri_by_purl`. Without this
    // dependency edges sourced at those PURLs silently disappear in
    // `build_dependency_relationships`, leaving the new root with
    // zero outgoing edges (parity break vs CycloneDX).
    // Milestone 149 (issue #151) — drop logic consolidated into
    // `apply_main_module_drop_or_demote` in `root_selector.rs`; runs
    // identically across all three emitters (CDX, SPDX 2.3, SPDX 3
    // here). When the new `preserve_manifest_main_module` flag is set,
    // the helper takes the demote-as-library branch; the demoted entry's
    // PURL still lands in `dropped_main_module_purls` so the
    // `package_iri_by_purl` aliasing below fires per US1 clarification
    // Option A (recorded 2026-06-29).
    let drop_result = crate::generate::root_selector::apply_main_module_drop_or_demote(
        scan.components,
        &scan.root_override,
        scan.preserve_manifest_main_module,
    );
    let dropped_main_module_purls: Vec<String> = drop_result.redirected_main_module_purls;
    let filtered_components_owned: Option<Vec<ResolvedComponent>> =
        if override_active {
            tracing::info!(
                name = scan.root_override.name.as_deref().unwrap_or(scan.target_name),
                version = scan.root_override.version.as_deref().unwrap_or("0.0.0"),
                "root component override active (SPDX 3): name='{}', version='{}'",
                scan.root_override.name.as_deref().unwrap_or(scan.target_name),
                scan.root_override.version.as_deref().unwrap_or("0.0.0"),
            );
            Some(drop_result.effective_components)
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
            file_inventory_stats: None,
            file_inventory_mode: None,
            root_override: scan.root_override.clone(),
            preserve_manifest_main_module: scan.preserve_manifest_main_module,
            user_metadata: scan.user_metadata.clone(),
            sbom_type_override: scan.sbom_type_override,
            spdx2_relationship_compat: scan.spdx2_relationship_compat,
            collisions_summary: scan.collisions_summary,
        };
        &view_scan_storage
    } else {
        scan
    };

    let fingerprint = scan_fingerprint(scan, cfg);
    let doc_iri = format!("{IRI_BASE}doc-{fingerprint}");
    let tool_iri = format!("{doc_iri}/tool/mikebom");
    // Milestone 078 — Organization Agent for `CreationInfo.createdBy`.
    // SPDX 3 SHACL constraint: `Core/createdBy` requires the IRI to
    // resolve to an `Agent` subclass (Person / Organization /
    // SoftwareAgent). Pre-fix mikebom emission pointed at a `Tool`
    // (separate class hierarchy), tripping the SHACL validator and
    // the Java SPDX library's range check ("Incompatible type for
    // property Core/createdBy: class core.Agent"). Per spec
    // clarification 2026-05-06, route `createdBy` to an
    // `Organization` whose name matches the CDX
    // `metadata.tools[0].publisher` value, and move the existing
    // Tool reference to the new `createdUsing` field. Determinism
    // contract per research §6: IRI is `{doc_iri}/agent/mikebom-
    // contributors` (path-style, mirroring the Tool IRI scheme).
    let org_iri = format!("{doc_iri}/agent/mikebom-contributors");
    // Milestone 078 — `simplelicensing_LicenseExpression` element
    // for `SpdxDocument.dataLicense`. SPDX 3 SHACL constraint:
    // `Core/dataLicense` requires the IRI to resolve to a
    // `SimpleLicensing/AnyLicenseInfo` subclass; pre-fix mikebom
    // emitted a bare URI string. The IRI value is unchanged
    // (`https://spdx.org/licenses/CC0-1.0`) — what changes is the
    // graph now contains a typed element at that IRI. Reuses the
    // existing per-component license-element pattern from
    // `v3_licenses.rs` (concrete subclass `simplelicensing_License
    // Expression` with required field `simplelicensing_license
    // Expression`; T001(c) confirmed against the SPDX 3 JSON-LD
    // schema's `simplelicensing_AnyLicenseInfo_derived` enumeration).
    let data_license_iri = "https://spdx.org/licenses/CC0-1.0";
    let created = cfg
        .created
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    let mut graph: Vec<Value> = Vec::new();

    // Milestone 080 — synthesize Agent / Tool elements for user-
    // supplied `--creator <Type: Name>` flags. IRIs follow the
    // milestone-078 path-style scheme: `<doc_iri>/<kind>/<slug>-
    // <hash16>`. The hash is BASE32(SHA-256(`<kind>:<name>`))[..16]
    // so two operators supplying the same `name` produce a single
    // collision-free IRI; reuses the `hash_prefix` helper at
    // line ~664 for consistency with the synthesized-component path.
    use mikebom::binding::user_metadata::CreatorKind;
    let mut user_tool_iris: Vec<String> = Vec::new();
    let mut user_agent_iris: Vec<String> = Vec::new();
    let mut user_creator_elements: Vec<Value> = Vec::new();
    for creator in &scan.user_metadata.creators {
        let (kind_segment, type_str) = match creator.kind {
            CreatorKind::Tool => ("tool", "Tool"),
            CreatorKind::Organization => ("org", "Organization"),
            CreatorKind::Person => ("person", "Person"),
        };
        let slug = url_friendly(&creator.name);
        let hash_input = format!(
            "{}:{}",
            kind_segment, creator.name
        );
        let hash = hash_prefix(hash_input.as_bytes(), 16);
        let iri = format!("{doc_iri}/{kind_segment}/{slug}-{hash}");
        user_creator_elements.push(json!({
            "type": type_str,
            "spdxId": iri.clone(),
            "creationInfo": CREATION_INFO_ID,
            "name": creator.name,
        }));
        match creator.kind {
            CreatorKind::Tool => user_tool_iris.push(iri),
            CreatorKind::Organization | CreatorKind::Person => {
                user_agent_iris.push(iri);
            }
        }
    }

    // 1a. Organization Agent — must be present in `@graph` BEFORE
    //     the CreationInfo references it. Determinism: same scan
    //     inputs → byte-identical Organization element across
    //     re-runs (research §6).
    graph.push(json!({
        "type": "Organization",
        "spdxId": org_iri,
        "creationInfo": CREATION_INFO_ID,
        "name": "mikebom contributors",
    }));

    // 1b. CreationInfo. `createdBy` references the mikebom
    //     contributors Organization IRI (Agent subclass — satisfies
    //     SHACL); the existing Tool reference lives in `createdUsing`.
    //     Milestone 080 — append user-supplied Tool / Organization /
    //     Person element IRIs to the appropriate slot.
    let mut created_by_arr: Vec<Value> = vec![Value::String(org_iri.clone())];
    for iri in &user_agent_iris {
        created_by_arr.push(Value::String(iri.clone()));
    }
    let mut created_using_arr: Vec<Value> = vec![Value::String(tool_iri.clone())];
    for iri in &user_tool_iris {
        created_using_arr.push(Value::String(iri.clone()));
    }
    graph.push(json!({
        "type": "CreationInfo",
        "@id": CREATION_INFO_ID,
        "specVersion": "3.0.1",
        "created": created,
        "createdBy": created_by_arr,
        "createdUsing": created_using_arr,
    }));

    // 2. Tool. Identity unchanged from pre-fix emission — only the
    //    referencing slot on CreationInfo moved.
    graph.push(json!({
        "type": "Tool",
        "spdxId": tool_iri,
        "creationInfo": CREATION_INFO_ID,
        "name": format!("mikebom-{}", cfg.mikebom_version),
    }));

    // 2b. Milestone 078 — `simplelicensing_LicenseExpression`
    //     element for the document-level `dataLicense` slot.
    //     Emitted once per document; the IRI is the SPDX-listed-
    //     license URL so downstream tools that already understand
    //     SPDX-listed-license IRIs recognize it.
    graph.push(json!({
        "type": "simplelicensing_LicenseExpression",
        "spdxId": data_license_iri,
        "creationInfo": CREATION_INFO_ID,
        "simplelicensing_licenseExpression": "CC0-1.0",
    }));

    // 2c. Milestone 080 — user-supplied creator elements (Tool /
    //     Organization / Person) referenced from CreationInfo.
    //     Insertion order = file-creators-then-flag-creators per
    //     research §6 (the upstream `merge_file_and_flags` enforces
    //     this; we iterate verbatim).
    for elem in user_creator_elements.iter().cloned() {
        graph.push(elem);
    }

    // Two-pass Package build: (a) precompute the PURL → IRI
    // lookup, (b) build agents against the lookup, (c) build
    // Packages with agent attachments inlined.
    let mut package_iri_by_purl =
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

    // Issue #229: when --root-name produced a synthesized root, alias
    // every dropped main-module PURL → synthesized root IRI so dep
    // edges originally sourced at those PURLs get rewritten to source
    // from the new root in build_dependency_relationships. Mirrors the
    // SPDX 2.3 alias path in `relationships.rs`.
    if synthetic_root_added {
        if let Some(synth_iri) = root_iris.first().cloned() {
            for purl in &dropped_main_module_purls {
                package_iri_by_purl.insert(purl.clone(), synth_iri.clone());
            }
        }
    }

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
    // Milestone 080 — `--scan-target-name` overrides `software_Sbom.name`
    // independently of milestone 077's `--root-name` (per research §5
    // SPDX 3 honors both flags independently). Precedence:
    //   1. `--scan-target-name` (milestone 080) — highest precedence
    //      for SPDX 3 document-level name.
    //   2. milestone 077 `--root-name` (when active).
    //   3. auto-derived `target_name`.
    let document_name_owned: String = if let Some(s) = scan.user_metadata.scan_target_name.as_deref() {
        s.to_string()
    } else if scan.root_override.is_active() {
        scan.root_override
            .name
            .as_deref()
            .unwrap_or(scan.target_name)
            .to_string()
    } else {
        scan.target_name.to_string()
    };
    let document_name: &str = document_name_owned.as_str();
    let mut spdx_document = json!({
        "type": "SpdxDocument",
        "spdxId": doc_iri,
        "creationInfo": CREATION_INFO_ID,
        "name": document_name,
        "dataLicense": "https://spdx.org/licenses/CC0-1.0",
        "rootElement": root_iris.clone(),
        "comment": scope_comment,
    });
    // Milestone 081 — SPDX 3 native `software_Sbom.software_sbomType[]`
    // emission per Constitution Principle V (standards-native first).
    // Aggregates from the same per-component `mikebom:sbom-tier`
    // values that drive CDX `metadata.lifecycles[]`, via the new
    // `aggregate_spdx3_sbom_types` helper. When the operator passes
    // `--sbom-type <type>`, the aggregator returns a single-element
    // Vec with the asserted value (overriding per-component
    // aggregation per research §4); per-component
    // `mikebom:sbom-tier` annotations preserve auto-detected values.
    // Empty result (no components carry tiers, or tiers don't map to
    // known IRIs) → no `software_Sbom` element is added (matches the
    // milestone-047 `metadata_omits_lifecycles_when_no_tiers_present`
    // pattern at `cyclonedx/metadata.rs`).
    //
    // Implementation note: the SPDX 3 schema places `software_sbomType`
    // exclusively on the `software_Sbom` class (a sibling of
    // `SpdxDocument` — both descend from `ElementCollection` via
    // different inheritance paths). The schema's
    // `unevaluatedProperties: false` constraint on @graph items
    // rejects `software_sbomType` on a `SpdxDocument`-typed element
    // AND rejects `dataLicense`/`import`/`namespaceMap` on a
    // `software_Sbom`-typed element. mikebom emits BOTH elements:
    // the existing `SpdxDocument` retains `dataLicense` (milestone
    // 078) + `import` (milestone 072 binding) + `namespaceMap`; the
    // new `software_Sbom` element carries `software_sbomType[]` and
    // mirrors `rootElement` so consumers can find the SBOM type via
    // either entry point. Both elements live in the same @graph
    // and share the same `creationInfo`.
    let sbom_type_values =
        crate::generate::lifecycle_phases::aggregate_spdx3_sbom_types(
            scan.components,
            scan.sbom_type_override,
        );
    let software_sbom_element: Option<Value> = if sbom_type_values.is_empty() {
        None
    } else {
        let sbom_iri = format!("{doc_iri}/sbom");
        Some(json!({
            "type": "software_Sbom",
            "spdxId": sbom_iri,
            "creationInfo": CREATION_INFO_ID,
            "name": document_name,
            "rootElement": root_iris.clone(),
            "software_sbomType": sbom_type_values,
        }))
    };
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
        // Milestone 079 — the emitted `externalIdentifierType` value
        // MUST come from the SPDX 3 controlled vocabulary
        // (`Core/externalIdentifierType` SHACL enum). The mapping
        // helper takes the internal mikebom scheme + identifier value
        // and returns the conformant vocab string + an optional
        // `comment` carrying the original scheme name (formatted as
        // `"original-scheme: <name>"`) when the mapping would
        // otherwise lose information. Pre-079 code wrote the
        // `source_label` / "manual identifier flag" string into
        // `comment`; per the milestone-079 wire-format contract that
        // slot now carries the original-scheme info-preservation
        // string. (The pre-079 source_label is still observable on
        // CDX 1.6 + SPDX 2.3 emission paths — those use independent
        // vocabularies and aren't touched by this milestone.)
        let mut id_entries: Vec<Value> = scan
            .identifiers
            .iter()
            .map(|id| {
                let mapping = map_scheme_to_vocab(&id.scheme, id.value.as_str());
                let mut entry = serde_json::Map::new();
                entry.insert("type".to_string(), json!("ExternalIdentifier"));
                entry.insert(
                    "externalIdentifierType".to_string(),
                    json!(mapping.vocab_type.as_str()),
                );
                entry.insert("identifier".to_string(), json!(id.value.as_str()));
                if let Some(comment) = mapping.comment {
                    entry.insert("comment".to_string(), json!(comment));
                }
                Value::Object(entry)
            })
            .collect();
        // Determinism (research §4 / VR-079-006): sort by
        // `(externalIdentifierType, identifier, comment)` so multi-
        // source dedup is correct when two identifiers map to the
        // same vocab+identifier but differ only in original-scheme.
        id_entries.sort_by(|a, b| {
            let key = |v: &Value| -> (String, String, String) {
                (
                    v.get("externalIdentifierType")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string(),
                    v.get("identifier")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string(),
                    v.get("comment")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string(),
                )
            };
            key(a).cmp(&key(b))
        });
        id_entries.dedup_by(|a, b| {
            let key = |v: &Value| -> (String, String, String) {
                (
                    v.get("externalIdentifierType")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string(),
                    v.get("identifier")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string(),
                    v.get("comment")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string(),
                )
            };
            key(a) == key(b)
        });
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

    // 3b. Milestone 081 — emit the `software_Sbom` element when the
    //     scan produced at least one mappable lifecycle tier. Lives
    //     immediately after the SpdxDocument so a JSON-walker
    //     reading top-down hits both document-class entry points
    //     before per-package data. Sort: not part of the canonical
    //     element catalog (this is a single optional element); its
    //     position is fixed at "after SpdxDocument, before
    //     packages" for byte-identity goldens determinism.
    if let Some(sbom_el) = software_sbom_element {
        graph.push(sbom_el);
    }

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

    // Milestone 154 (closes issue #487): sweep the emitted
    // simplelicensing_LicenseExpression elements for inline LicenseRef-*
    // substrings and emit matching simplelicensing_CustomLicense elements
    // per SPDX 3.0.1 § licensing_CustomLicense. Paired follow-up to
    // milestone 153's SPDX 2.3 hasExtractedLicensingInfos[] sweep —
    // preserves cross-format symmetry (same LicenseRef set defined in
    // both formats with byte-identical placeholder text).
    let custom_license_elements = super::v3_licenses::sweep_custom_licenses(
        &license_elements,
        &doc_iri,
        CREATION_INFO_ID,
    );

    for elem in license_elements {
        graph.push(elem);
    }
    for elem in custom_license_elements {
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

    // Issue #236: when `pick_root_iri` synthesized a root (multi-
    // top-level scans with no main-module and no name match — the
    // dominant case for image scans and OS-package scans), the
    // synthetic root has no outgoing edges in `scan.relationships`.
    // CDX covers this with the primary-dependency fallback in
    // `cyclonedx/dependencies.rs:74-99`; we mirror it here for SPDX
    // 3 so the emitted graph is rooted at the same synthetic
    // identity. Without this fix the SpdxDocument's `rootElement`
    // points at a Package no Relationship targets as a source —
    // the SBOM is structurally valid but the dep graph has N
    // disconnected graph-tops where CDX has a single root.
    if synthetic_root_added {
        if let Some(synth_iri) = root_iris.first() {
            let depended_on: std::collections::BTreeSet<&str> = scan
                .relationships
                .iter()
                .map(|r| r.to.as_str())
                .collect();
            let mut graph_root_iris: Vec<&str> = scan
                .components
                .iter()
                .filter(|c| c.parent_purl.is_none() && !depended_on.contains(c.purl.as_str()))
                .filter_map(|c| package_iri_by_purl.get(c.purl.as_str()).map(String::as_str))
                .collect();
            // Deterministic emission order: lex by IRI.
            graph_root_iris.sort();
            for to_iri in graph_root_iris {
                all_relationships.push(super::v3_relationships::build_relationship(
                    synth_iri.as_str(),
                    "dependsOn",
                    to_iri,
                    &doc_iri,
                    CREATION_INFO_ID,
                ));
            }
        }
    }

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
    // Milestone 119 phase-2 — supplement-declared services need C40
    // saas-service + C65 source-tier=declared annotations on the
    // service Package elements `v3_packages::build_packages` already
    // emitted.
    annotations.extend(super::v3_annotations::build_supplement_service_annotations(
        &doc_iri,
        CREATION_INFO_ID,
    ));
    // Milestone 080 — user-supplied --metadata-comment + --annotator/
    // --annotation-comment pairs land as Annotation elements pointed
    // at the SpdxDocument. Each annotator references the corresponding
    // user-creator element (added above) when one matches by
    // (kind, name); otherwise mikebom synthesizes a fresh agent IRI on
    // the fly so the SHACL contract `Annotation.subject = Element` is
    // satisfied.
    if scan.user_metadata.metadata_comment.is_some()
        || !scan.user_metadata.annotations.is_empty()
    {
        // (a) --metadata-comment → one Annotation referencing the
        //     SpdxDocument as subject. annotationType: "other".
        if let Some(comment) = &scan.user_metadata.metadata_comment {
            let hash = hash_prefix(
                format!("metadata-comment:{}", comment).as_bytes(),
                16,
            );
            let anno_iri = format!("{doc_iri}/annotation/metadata-comment-{hash}");
            annotations.push(json!({
                "type": "Annotation",
                "spdxId": anno_iri,
                "creationInfo": CREATION_INFO_ID,
                "subject": doc_iri.clone(),
                "annotationType": "other",
                "statement": comment,
            }));
        }
        // (b) --annotator + --annotation-comment pairs.
        for (i, ann) in scan.user_metadata.annotations.iter().enumerate() {
            let hash = hash_prefix(
                format!("anno:{}:{}:{}", i, ann.annotator.name, ann.comment)
                    .as_bytes(),
                16,
            );
            let slug = url_friendly(&ann.annotator.name);
            let anno_iri = format!("{doc_iri}/annotation/{slug}-{hash}");
            annotations.push(json!({
                "type": "Annotation",
                "spdxId": anno_iri,
                "creationInfo": CREATION_INFO_ID,
                "subject": doc_iri.clone(),
                "annotationType": "other",
                "statement": ann.comment,
            }));
        }
    }
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
        // Milestone N+1: `build_subject_purl` returns `None` when
        // `--no-root-purl` is in effect; otherwise builds
        // `pkg:<type>/<name>@<version>` with the type from
        // `--root-purl-type` (default `generic`).
        let synth_purl_opt = scan.root_override.build_subject_purl(name, version);
        // IRI: hash the PURL when present, else hash `name@version` so
        // the synthesized IRI stays stable across runs in the omit case.
        let iri_seed = synth_purl_opt
            .clone()
            .unwrap_or_else(|| format!("{name}@{version}"));
        let synth_iri = format!(
            "{doc_iri}/pkg-root-{}",
            hash_prefix(iri_seed.as_bytes(), 16)
        );
        // CPE: reuse `url_friendly` for sanitization parity with the
        // existing non-override path. CPE has its own escape rules
        // distinct from RFC 3986 percent-encoding.
        let synth_cpe = format!(
            "cpe:2.3:a:mikebom:{}:{}:*:*:*:*:*:*:*",
            url_friendly(name),
            url_friendly(version),
        );
        // Build externalIdentifier[] conditionally — CPE always, PURL
        // only when present.
        let mut ext_ids: Vec<serde_json::Value> = Vec::with_capacity(2);
        ext_ids.push(json!({
            "type": "ExternalIdentifier",
            "externalIdentifierType": "cpe23",
            "identifier": synth_cpe,
        }));
        if let Some(ref synth_purl) = synth_purl_opt {
            ext_ids.push(json!({
                "type": "ExternalIdentifier",
                "externalIdentifierType": "packageUrl",
                "identifier": synth_purl,
            }));
        }
        let mut synth_pkg = json!({
            "type": "software_Package",
            "spdxId": synth_iri,
            "creationInfo": CREATION_INFO_ID,
            "name": name,
            "software_packageVersion": version,
            "externalIdentifier": ext_ids,
        });
        // Add software_packageUrl only when the PURL is present.
        if let Some(synth_purl) = synth_purl_opt {
            if let Some(obj) = synth_pkg.as_object_mut() {
                obj.insert(
                    "software_packageUrl".to_string(),
                    serde_json::Value::String(synth_purl),
                );
            }
        }
        packages.insert(0, synth_pkg);
        return (vec![synth_iri], true);
    }

    // Milestone 127 — delegate root-element selection to the central
    // `generate::root_selector::select_root` ladder. The selector
    // handles count==1 fast path > FR-002 repo-root > FR-003
    // ecosystem-priority > FR-004 LCP > Maven coord > synthetic
    // placeholder. When the result names a `MainModule`, return its
    // IRI as the sole rootElement. Otherwise fall through to the
    // existing target-name match + synthesis branches below (which
    // handle the no-main-modules case and provide the
    // sbomqs-compatible synthesized root).
    let selection = crate::generate::root_selector::select_root(
        components,
        &scan.root_override,
        scan.scan_target_coord,
        scan.target_name,
        "0.0.0",
    );
    if let crate::generate::root_selector::ResolvedRootSubject::MainModule(idx) =
        &selection.subject
    {
        let comp = &components[*idx];
        if let Some(iri) = package_iri_by_purl.get(comp.purl.as_str()) {
            return (vec![iri.clone()], false);
        }
    }

    if let Some(c) = components.iter().find(|c| c.name == scan.target_name) {
        if let Some(iri) = package_iri_by_purl.get(c.purl.as_str()) {
            return (vec![iri.clone()], false);
        }
    }

    // Synthesize a root Package. Mirrors the SPDX 2.3 emitter's
    // synthesize_root behavior — preserves sbomqs scoring parity
    // (a document with no rootElement scores worse).
    //
    // Issue #236: PURL and CPE have different escape rules, so they
    // are sanitized separately. The PURL uses
    // `encode_purl_segment` (the same helper CDX uses for its
    // `metadata.component.purl`), which preserves colon literals
    // (so `postgres:16` → `postgres:16`, matching CDX). Pre-fix
    // this path used `url_friendly` for both, producing
    // `postgres-16` for the SPDX 3 PURL — yet a third per-format
    // root-identity variant alongside CDX's `postgres:16` and the
    // SPDX 2.3 path's `postgres_16` (also fixed in this milestone).
    // The CPE keeps `url_friendly` because the CPE 2.3 grammar
    // uses `-` as the conventional component separator-safe filler.
    let synth_purl = format!(
        "pkg:generic/{}@0.0.0",
        mikebom_common::types::purl::encode_purl_segment(scan.target_name),
    );
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
