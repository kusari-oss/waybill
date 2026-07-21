//! SPDX 3.0.1 `software_Package` element builder (milestone 011).
//!
//! Per `data-model.md` Element Catalog §`software_Package`:
//! emits one Package per discovered component with `name`,
//! `software_packageVersion`, `software_packageUrl`,
//! `verifiedUsing[]` (Hash value-objects), `software_homePage`,
//! `software_sourceInfo`, `software_downloadLocation`, and the
//! Package's `externalIdentifier[]` (PURL + any fully-resolved
//! CPE 2.3 vectors).
//!
//! Output is deterministically ordered by `spdxId`. The IRI for
//! each Package is `<doc IRI>/pkg-<base32(SHA256(<purl>))[..16]>`,
//! identical to the milestone-010 stub's derivation.

use std::collections::BTreeMap;

use data_encoding::BASE32_NOPAD;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use waybill_common::resolution::ResolvedComponent;

use super::v3_agents::PackageAgentAttachments;
use super::v3_external_ids::build_external_identifiers_for;
use super::v3_id_type_map::map_scheme_to_vocab;

/// Build the PURL → Package-IRI lookup table. Used as a first
/// pass by `v3_document::build_document` so Agent and License
/// builders can reference Package IRIs before Packages are
/// composed.
pub fn build_iri_lookup(
    components: &[ResolvedComponent],
    doc_iri: &str,
) -> BTreeMap<String, String> {
    let mut lookup: BTreeMap<String, String> = BTreeMap::new();
    for c in components {
        let purl_str = c.purl.as_str();
        let pkg_iri = format!("{doc_iri}/pkg-{}", hash_prefix(purl_str.as_bytes(), 16));
        lookup.insert(purl_str.to_string(), pkg_iri);
    }
    lookup
}

/// Build the `software_Package` elements for a scan plus the
/// PURL → IRI lookup needed by relationship/license/agent
/// builders. Returns `(packages, package_iri_by_purl)` with
/// packages already sorted by `spdxId` for determinism.
///
/// `agent_attachments` (per-package `suppliedBy`/`originatedBy`)
/// comes from `v3_agents::build_agents` and is inlined onto each
/// Package — SPDX 3 puts these as direct Artifact_props fields,
/// not Relationship edges.
pub fn build_packages(
    components: &[ResolvedComponent],
    doc_iri: &str,
    creation_info_id: &str,
    agent_attachments: &BTreeMap<String, PackageAgentAttachments>,
    component_identifiers: &[waybill::binding::identifiers::component_id::ComponentIdentifierFlag],
    match_counts: &mut BTreeMap<usize, usize>,
) -> (Vec<Value>, BTreeMap<String, String>) {
    let mut package_iri_by_purl: BTreeMap<String, String> = BTreeMap::new();
    let mut packages: Vec<Value> = Vec::with_capacity(components.len());

    for c in components {
        let purl_str = c.purl.as_str();
        let pkg_iri = format!("{doc_iri}/pkg-{}", hash_prefix(purl_str.as_bytes(), 16));
        package_iri_by_purl.insert(purl_str.to_string(), pkg_iri.clone());

        // Milestone 133 US1.C: file-tier components emit as a
        // distinct element type per research §"SPDX 3 element type
        // for file-tier components" (FR-001). Detect the
        // `waybill:component-tier = "file"` annotation and emit
        // `software_File` instead of the regular package element
        // type; suppress `software_packageUrl` per FR-009.
        let is_file_tier = c
            .extra_annotations
            .get(crate::scan_fs::file_tier::COMPONENT_TIER_KEY)
            .and_then(|v| v.as_str())
            == Some(crate::scan_fs::file_tier::COMPONENT_TIER_FILE_VALUE);

        let mut pkg = serde_json::Map::new();
        let element_type = if is_file_tier {
            "software_File"
        } else {
            "software_Package"
        };
        pkg.insert("type".to_string(), json!(element_type));
        pkg.insert("spdxId".to_string(), json!(pkg_iri));
        pkg.insert("creationInfo".to_string(), json!(creation_info_id));
        pkg.insert("name".to_string(), json!(c.name));
        if !c.version.is_empty() {
            pkg.insert("software_packageVersion".to_string(), json!(c.version));
        }
        if !is_file_tier {
            pkg.insert("software_packageUrl".to_string(), json!(purl_str));
        }

        // Milestone 053 FR-001a (SPDX 3.0.1): components carrying
        // `waybill:component-role: main-module` (catalog row C40) are
        // the workspace's main-module — set the native SPDX 3.0.1
        // `software_primaryPurpose: "application"` field per the
        // schema's `prop_software_SoftwareArtifact_software_primaryPurpose`
        // definition.
        //
        // Milestone 104 — binary-reader-discovered components also
        // populate `software_primaryPurpose` from their `BinaryRole`
        // classification. Mapping per
        // `specs/104-binary-role-classification/contracts/binary-role-cross-format-mapping.md`:
        // Application→"application", SharedLibrary→"library",
        // Object→"file", Other→omitted.
        let primary_purpose = match c.binary_role {
            Some(waybill_common::resolution::BinaryRole::Application) => Some("application"),
            Some(waybill_common::resolution::BinaryRole::SharedLibrary) => Some("library"),
            Some(waybill_common::resolution::BinaryRole::Object) => Some("file"),
            Some(waybill_common::resolution::BinaryRole::Other) => None,
            None => {
                if c.extra_annotations
                    .get("waybill:component-role")
                    .and_then(|v| v.as_str())
                    == Some("main-module")
                {
                    Some("application")
                } else {
                    None
                }
            }
        };
        if let Some(p) = primary_purpose {
            pkg.insert("software_primaryPurpose".to_string(), json!(p));
        }

        // verifiedUsing[] — Hash value-objects, one per integrity
        // checksum waybill computed. SPDX 3's algorithm enum uses
        // lowercase-with-no-hyphen form (`sha256`, `sha1`, `md5`).
        // See `prop_Hash_algorithm` in the bundled schema.
        if !c.hashes.is_empty() {
            let mut hashes: Vec<Value> = c
                .hashes
                .iter()
                .map(|h| {
                    json!({
                        "type": "Hash",
                        "algorithm": spdx3_algorithm_name(h.algorithm),
                        "hashValue": h.value.as_str(),
                    })
                })
                .collect();
            // Deterministic ordering inside the array per
            // data-model.md §"Deterministic ordering rules".
            hashes.sort_by(|a, b| {
                let key = |v: &Value| -> (String, String) {
                    (
                        v["algorithm"].as_str().unwrap_or("").to_string(),
                        v["hashValue"].as_str().unwrap_or("").to_string(),
                    )
                };
                key(a).cmp(&key(b))
            });
            pkg.insert("verifiedUsing".to_string(), json!(hashes));
        }

        // software_homePage / software_sourceInfo / software_downloadLocation
        // — populated from the first matching CycloneDX
        // externalReferences entry per A9/A10/A11.
        for r in &c.external_references {
            match r.ref_type.as_str() {
                "homepage" | "website" => {
                    pkg.entry("software_homePage")
                        .or_insert_with(|| json!(r.url));
                }
                "vcs" => {
                    pkg.entry("software_sourceInfo")
                        .or_insert_with(|| json!(r.url));
                }
                "distribution" => {
                    pkg.entry("software_downloadLocation")
                        .or_insert_with(|| json!(r.url));
                }
                _ => {}
            }
        }

        // externalIdentifier[] — PURL (always one entry) plus any
        // fully-resolved CPE vectors. Delegated to
        // v3_external_ids::build_external_identifiers_for so the
        // shape is owned by one module.
        let mut ext_ids = build_external_identifiers_for(c);
        // Milestone 133 US1.C: filter out the `packageUrl` entry
        // for file-tier components per FR-009. CPE entries (and any
        // operator-supplied identifiers) stay; only the placeholder
        // PURL is suppressed.
        if is_file_tier {
            ext_ids.retain(|v| {
                v.get("externalIdentifierType").and_then(|s| s.as_str())
                    != Some("packageUrl")
            });
        }
        // Milestone 076 — append per-component user-defined
        // identifiers after the pre-existing PURL/CPE entries.
        // Milestone 079 — every appended entry's
        // `externalIdentifierType` MUST come from the SPDX 3
        // controlled vocabulary; vocab-named user schemes (e.g.,
        // `cve`) pass through verbatim, non-vocab user schemes
        // (e.g., `jira`) map to `other` with the original scheme
        // preserved on the `comment` field per FR-003.
        for (idx, flag) in component_identifiers.iter().enumerate() {
            if flag.selector_purl == c.purl.as_str() {
                *match_counts.entry(idx).or_insert(0) += 1;
                let mapping = map_scheme_to_vocab(&flag.scheme, flag.value.as_str());
                let mut entry = serde_json::Map::new();
                entry.insert("type".to_string(), json!("ExternalIdentifier"));
                entry.insert(
                    "externalIdentifierType".to_string(),
                    json!(mapping.vocab_type.as_str()),
                );
                entry.insert("identifier".to_string(), json!(flag.value.as_str()));
                if let Some(comment) = mapping.comment {
                    entry.insert("comment".to_string(), json!(comment));
                }
                ext_ids.push(Value::Object(entry));
            }
        }
        // Milestone 079 / VR-079-006 — sort by
        // `(externalIdentifierType, identifier, comment)` to keep
        // determinism + dedup correct when two identifiers map to
        // the same vocab+identifier but differ only in
        // original-scheme provenance.
        ext_ids.sort_by(|a, b| {
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
        ext_ids.dedup_by(|a, b| {
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
        if !ext_ids.is_empty() {
            pkg.insert("externalIdentifier".to_string(), json!(ext_ids));
        }

        // suppliedBy / originatedBy — per-Package Agent attachments.
        // SPDX 3 puts these as Artifact_props properties, not
        // Relationship edges (unlike SPDX 2.3).
        if let Some(attach) = agent_attachments.get(&pkg_iri) {
            if let Some(iri) = &attach.supplied_by {
                pkg.insert("suppliedBy".to_string(), json!(iri));
            }
            if !attach.originated_by.is_empty() {
                pkg.insert("originatedBy".to_string(), json!(attach.originated_by));
            }
        }

        packages.push(Value::Object(pkg));
    }

    // Milestone 119 phase-2 — append supplement-declared services as
    // SPDX 3 `software_Package` elements tagged via the existing C40
    // waybill annotation pattern per research Decision 4. The
    // standalone SPDX 3.0.1 stable schema doesn't carry a Service
    // element type usable across consumer tooling; the C40 fallback
    // pattern preserves consumer interoperability with the SPDX 2.3
    // projection. The supplement bom-ref / name seeds the IRI hash;
    // the resulting IRI is stable across runs.
    if let Some(services) = crate::supplement::current_services() {
        for svc in &services {
            packages.push(supplement_service_to_v3_package(svc, doc_iri, creation_info_id));
        }
    }

    // Deterministic ordering by spdxId (data-model.md §"Deterministic
    // ordering rules").
    packages.sort_by(|a, b| {
        let key = |v: &Value| v["spdxId"].as_str().unwrap_or("").to_string();
        key(a).cmp(&key(b))
    });

    (packages, package_iri_by_purl)
}

fn supplement_service_to_v3_package(
    svc: &crate::supplement::SupplementService,
    doc_iri: &str,
    creation_info_id: &str,
) -> Value {
    let id_seed = svc.bom_ref.as_deref().unwrap_or(svc.name.as_str());
    let pkg_iri = format!("{doc_iri}/pkg-{}", hash_prefix(id_seed.as_bytes(), 16));
    let mut pkg = serde_json::Map::new();
    pkg.insert("type".to_string(), json!("software_Package"));
    pkg.insert("spdxId".to_string(), json!(pkg_iri));
    pkg.insert("creationInfo".to_string(), json!(creation_info_id));
    pkg.insert("name".to_string(), json!(svc.name));
    if let Some(desc) = &svc.description {
        pkg.insert("description".to_string(), json!(desc));
    }
    // Endpoints surface on `software_homePage` when there's exactly
    // one; otherwise they ride a waybill-namespaced extension below.
    // The SPDX 3.0.1 stable schema has no native `endpoints[]` slot.
    if let Some(endpoints) = &svc.endpoints {
        if endpoints.len() == 1 {
            pkg.insert("software_homePage".to_string(), json!(endpoints[0]));
        }
    }
    Value::Object(pkg)
}

/// Return the IRI for a supplement-declared service package. Used by
/// `v3_document::build_document` to construct Annotation elements
/// targeting the service after `build_packages` has already emitted
/// the Package itself.
pub fn supplement_service_iri(svc: &crate::supplement::SupplementService, doc_iri: &str) -> String {
    let id_seed = svc.bom_ref.as_deref().unwrap_or(svc.name.as_str());
    format!("{doc_iri}/pkg-{}", hash_prefix(id_seed.as_bytes(), 16))
}

/// Deterministic base32 prefix of SHA-256(input). Used for IRI path
/// segments. Identical to the helper in v3_stub.rs.
fn hash_prefix(input: &[u8], chars: usize) -> String {
    let digest = Sha256::digest(input);
    let encoded = BASE32_NOPAD.encode(&digest);
    encoded[..chars].to_string()
}

/// Convert a waybill `HashAlgorithm` to the SPDX 3 `Hash.algorithm`
/// enum value (lowercase, no hyphens) per `prop_Hash_algorithm` in
/// the bundled schema.
fn spdx3_algorithm_name(algo: waybill_common::types::hash::HashAlgorithm) -> &'static str {
    use waybill_common::types::hash::HashAlgorithm;
    match algo {
        HashAlgorithm::Sha1 => "sha1",
        HashAlgorithm::Sha256 => "sha256",
        HashAlgorithm::Sha512 => "sha512",
        HashAlgorithm::Md5 => "md5",
    }
}
