//! SPDX 3.0.1-side parity extractors (milestone 022 commit 4).
//!
//! Mirrors `extractors/cdx.rs` and `extractors/spdx2.rs` but for
//! SPDX 3.0.1 graph shape (`@graph[]` of typed elements, IRI-keyed
//! relationships). Owns every `spdx3_*` and `c*_spdx3` /
//! `d*_spdx3` / `e*_spdx3` / `f*_spdx3` / `g*_spdx3` extractor
//! function referenced by `EXTRACTORS` in `super::mod`.

use std::collections::BTreeSet;

use serde_json::Value;

use super::common::{
    extract_mikebom_annotation_values, normalize_alg, spdx_relationship_edges,
    walk_spdx3_packages,
};

/// Single-format SPDX 3 C-section stub generator.
macro_rules! spdx3_anno {
    ($name:ident, $field:literal, component) => {
        pub(super) fn $name(doc: &Value) -> BTreeSet<String> {
            extract_mikebom_annotation_values(doc, $field, false)
        }
    };
    ($name:ident, $field:literal, document) => {
        pub(super) fn $name(doc: &Value) -> BTreeSet<String> {
            extract_mikebom_annotation_values(doc, $field, true)
        }
    };
}

// ============================================================
// Section A — Core identity (A1-A12)
// ============================================================

pub(super) fn spdx3_purl(doc: &Value) -> BTreeSet<String> {
    walk_spdx3_packages(doc)
        .iter()
        .filter_map(|p| {
            p.get("software_packageUrl")
                .and_then(|v| v.as_str())
                .map(String::from)
        })
        .collect()
}

pub(super) fn spdx3_name(doc: &Value) -> BTreeSet<String> {
    walk_spdx3_packages(doc)
        .iter()
        .filter_map(|p| p.get("name").and_then(|v| v.as_str()).map(String::from))
        .collect()
}

pub(super) fn spdx3_version(doc: &Value) -> BTreeSet<String> {
    walk_spdx3_packages(doc)
        .iter()
        .filter_map(|p| {
            p.get("software_packageVersion")
                .and_then(|v| v.as_str())
                .map(String::from)
        })
        .collect()
}

pub(super) fn spdx3_hashes(doc: &Value) -> BTreeSet<String> {
    walk_spdx3_packages(doc)
        .iter()
        .flat_map(|p| {
            p.get("verifiedUsing")
                .and_then(|v| v.as_array())
                .map(|arr| arr.as_slice())
                .unwrap_or(&[])
                .iter()
                .filter_map(|h| {
                    let alg = h.get("algorithm").and_then(|v| v.as_str())?;
                    let val = h.get("hashValue").and_then(|v| v.as_str())?;
                    Some(format!("{}:{}", normalize_alg(alg), val))
                })
        })
        .collect()
}

pub(super) fn spdx3_homepage(doc: &Value) -> BTreeSet<String> {
    walk_spdx3_packages(doc)
        .iter()
        .filter_map(|p| {
            p.get("software_homePage")
                .and_then(|v| v.as_str())
                .map(String::from)
        })
        .collect()
}
pub(super) fn spdx3_vcs(doc: &Value) -> BTreeSet<String> {
    walk_spdx3_packages(doc)
        .iter()
        .filter_map(|p| {
            p.get("software_sourceInfo")
                .and_then(|v| v.as_str())
                .map(String::from)
        })
        .collect()
}
pub(super) fn spdx3_distribution(doc: &Value) -> BTreeSet<String> {
    walk_spdx3_packages(doc)
        .iter()
        .filter_map(|p| {
            p.get("software_downloadLocation")
                .and_then(|v| v.as_str())
                .map(String::from)
        })
        .collect()
}

/// Milestone 104 — per-component role from SPDX 3
/// `software_Package.software_primaryPurpose`. Returns
/// `<purl>=<role>` strings. The SPDX 3 values are already lowercase
/// (`application`/`library`/`file`) so the cross-format byte
/// comparison with CDX succeeds directly.
///
/// Scoped to binary-reader-emitted Packages only — detected via the
/// `mikebom:binary-class` Annotation element pointing at the
/// package's IRI. Mirrors the scoping in `cdx_binary_role`; see
/// that function for rationale.
pub(super) fn spdx3_binary_role(doc: &Value) -> BTreeSet<String> {
    let Some(graph) = doc.get("@graph").and_then(|v| v.as_array()) else {
        return BTreeSet::new();
    };
    // Build the set of package IRIs that have a `mikebom:binary-class`
    // Annotation pointing at them (`subject` field).
    let binary_reader_iris: std::collections::BTreeSet<&str> = graph
        .iter()
        .filter(|el| el.get("type").and_then(|v| v.as_str()) == Some("Annotation"))
        .filter(|el| {
            el.get("statement")
                .and_then(|v| v.as_str())
                .map(|s| s.contains("\"field\":\"mikebom:binary-class\""))
                .unwrap_or(false)
        })
        .filter_map(|el| el.get("subject").and_then(|v| v.as_str()))
        .collect();
    walk_spdx3_packages(doc)
        .iter()
        .filter_map(|p| {
            let iri = p.get("spdxId").and_then(|v| v.as_str())?;
            if !binary_reader_iris.contains(iri) {
                return None;
            }
            let purl = p
                .get("software_packageUrl")
                .and_then(|v| v.as_str())?;
            let purpose = p.get("software_primaryPurpose").and_then(|v| v.as_str())?;
            match purpose {
                "application" | "library" | "file" => {
                    Some(format!("{purl}={purpose}"))
                }
                _ => None,
            }
        })
        .collect()
}

pub(super) fn spdx3_cpe(doc: &Value) -> BTreeSet<String> {
    walk_spdx3_packages(doc)
        .iter()
        .flat_map(|p| {
            p.get("externalIdentifier")
                .and_then(|v| v.as_array())
                .map(|arr| arr.as_slice())
                .unwrap_or(&[])
                .iter()
                .filter(|e| {
                    e.get("externalIdentifierType").and_then(|v| v.as_str()) == Some("cpe23")
                })
                .filter_map(|e| {
                    e.get("identifier")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                })
        })
        .collect()
}

// SPDX 3 walks simplelicensing_LicenseExpression elements + their
// hasDeclared/hasConcludedLicense Relationships.
fn spdx3_license_expressions_by_relationship(
    doc: &Value,
    rel_type: &str,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Some(graph) = doc.get("@graph").and_then(|v| v.as_array()) else {
        return out;
    };
    let mut expr_by_iri = std::collections::BTreeMap::new();
    for el in graph {
        if el.get("type").and_then(|v| v.as_str())
            == Some("simplelicensing_LicenseExpression")
        {
            if let (Some(id), Some(expr)) = (
                el.get("spdxId").and_then(|v| v.as_str()),
                el.get("simplelicensing_licenseExpression")
                    .and_then(|v| v.as_str()),
            ) {
                expr_by_iri.insert(id.to_string(), expr.to_string());
            }
        }
    }
    for el in graph {
        if el.get("type").and_then(|v| v.as_str()) != Some("Relationship") {
            continue;
        }
        if el.get("relationshipType").and_then(|v| v.as_str()) != Some(rel_type) {
            continue;
        }
        let Some(targets) = el.get("to").and_then(|v| v.as_array()) else {
            continue;
        };
        for t in targets {
            if let Some(iri) = t.as_str() {
                if let Some(expr) = expr_by_iri.get(iri) {
                    out.insert(expr.clone());
                }
            }
        }
    }
    out
}
pub(super) fn spdx3_licenses_declared(doc: &Value) -> BTreeSet<String> {
    spdx3_license_expressions_by_relationship(doc, "hasDeclaredLicense")
}
pub(super) fn spdx3_licenses_concluded(doc: &Value) -> BTreeSet<String> {
    spdx3_license_expressions_by_relationship(doc, "hasConcludedLicense")
}

pub(super) fn spdx3_supplier(doc: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Some(graph) = doc.get("@graph").and_then(|v| v.as_array()) else {
        return out;
    };
    let mut name_by_iri = std::collections::BTreeMap::new();
    for el in graph {
        if matches!(
            el.get("type").and_then(|v| v.as_str()),
            Some("Organization") | Some("Person")
        ) {
            if let (Some(id), Some(name)) = (
                el.get("spdxId").and_then(|v| v.as_str()),
                el.get("name").and_then(|v| v.as_str()),
            ) {
                name_by_iri.insert(id.to_string(), name.to_string());
            }
        }
    }
    for p in walk_spdx3_packages(doc) {
        if let Some(iri) = p.get("suppliedBy").and_then(|v| v.as_str()) {
            if let Some(name) = name_by_iri.get(iri) {
                out.insert(name.clone());
            }
        }
    }
    out
}

// ============================================================
// Section B — Graph structure (B1-B4)
// ============================================================

pub(super) fn spdx3_runtime_deps(doc: &Value) -> BTreeSet<String> {
    // Milestone 085: SPDX 3 puts lifecycle classification on the
    // Relationship itself via the `scope` parameter (per milestone
    // 052/part-2 — `dev` / `build` / `test` / `runtime`). The
    // generic `spdx_relationship_edges` walker can't see that
    // because B2 (dev) is signaled by a separate annotation
    // mechanism in this extractor file. For the runtime bucket,
    // include only relationships whose scope is absent or runtime;
    // exclude any with scope=dev/build/test so SPDX 3 matches CDX's
    // post-085 per-edge classifier (which excludes edges where the
    // target carries `scope: "excluded"`) and SPDX 2.3's typed
    // relationshipType filter (which excludes DEV/BUILD/TEST_*_OF
    // by counting only DEPENDS_ON).
    let Some(graph) = doc.get("@graph").and_then(|v| v.as_array()) else {
        return BTreeSet::new();
    };
    let mut purl_by_iri: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    for el in graph {
        if el.get("type").and_then(|v| v.as_str()) == Some("software_Package") {
            if let (Some(iri), Some(purl)) = (
                el.get("spdxId").and_then(|v| v.as_str()),
                el.get("software_packageUrl").and_then(|v| v.as_str()),
            ) {
                purl_by_iri.insert(iri.to_string(), purl.to_string());
            }
        }
    }
    let mut out = BTreeSet::new();
    for el in graph {
        let el_type = el.get("type").and_then(|v| v.as_str());
        if !matches!(el_type, Some("Relationship") | Some("LifecycleScopedRelationship")) {
            continue;
        }
        if el.get("relationshipType").and_then(|v| v.as_str()) != Some("dependsOn") {
            continue;
        }
        let scope = el.get("scope").and_then(|v| v.as_str());
        if matches!(scope, Some("development") | Some("build") | Some("test")) {
            continue;
        }
        let Some(from_iri) = el.get("from").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(to_arr) = el.get("to").and_then(|v| v.as_array()) else {
            continue;
        };
        let Some(from_purl) = purl_by_iri.get(from_iri) else {
            continue;
        };
        for t in to_arr {
            if let Some(t_iri) = t.as_str() {
                if let Some(to_purl) = purl_by_iri.get(t_iri) {
                    out.insert(format!("{from_purl}->{to_purl}"));
                }
            }
        }
    }
    out
}

// SPDX 3 lacks `devDependencyOf`; per milestone-052/part-2 the
// dev-vs-runtime distinction lives on the `Relationship` itself
// via the `scope` parameter (`dev` / `build` / `test`).
//
// Milestone 085: walk `dependsOn` Relationships and include only
// those whose `scope` is dev/build/test. Mirrors B1 (which
// excludes the same scopes) and matches SPDX 2.3's typed
// DEV/BUILD/TEST_DEPENDENCY_OF representation. The previous
// implementation read a deprecated `mikebom:dev-dependency`
// annotation on the source Package; that annotation was removed
// when 052/part-2 promoted the native scope encoding.
pub(super) fn spdx3_dev_deps(doc: &Value) -> BTreeSet<String> {
    let Some(graph) = doc.get("@graph").and_then(|v| v.as_array()) else {
        return BTreeSet::new();
    };
    let mut purl_by_iri: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    for el in graph {
        if el.get("type").and_then(|v| v.as_str()) == Some("software_Package") {
            if let (Some(iri), Some(purl)) = (
                el.get("spdxId").and_then(|v| v.as_str()),
                el.get("software_packageUrl").and_then(|v| v.as_str()),
            ) {
                purl_by_iri.insert(iri.to_string(), purl.to_string());
            }
        }
    }
    let mut out = BTreeSet::new();
    for el in graph {
        let el_type = el.get("type").and_then(|v| v.as_str());
        if !matches!(el_type, Some("Relationship") | Some("LifecycleScopedRelationship")) {
            continue;
        }
        if el.get("relationshipType").and_then(|v| v.as_str()) != Some("dependsOn") {
            continue;
        }
        let scope = el.get("scope").and_then(|v| v.as_str());
        if !matches!(scope, Some("development") | Some("build") | Some("test")) {
            continue;
        }
        let Some(from_iri) = el.get("from").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(to_arr) = el.get("to").and_then(|v| v.as_array()) else {
            continue;
        };
        let Some(from_purl) = purl_by_iri.get(from_iri) else {
            continue;
        };
        for t in to_arr {
            if let Some(t_iri) = t.as_str() {
                if let Some(to_purl) = purl_by_iri.get(t_iri) {
                    out.insert(format!("{from_purl}->{to_purl}"));
                }
            }
        }
    }
    out
}

pub(super) fn spdx3_containment(doc: &Value) -> BTreeSet<String> {
    spdx_relationship_edges(doc, "", "contains")
}

pub(super) fn spdx3_root(doc: &Value) -> BTreeSet<String> {
    let Some(graph) = doc.get("@graph").and_then(|v| v.as_array()) else {
        return BTreeSet::new();
    };
    let mut purl_by_iri: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    for el in graph {
        if el.get("type").and_then(|v| v.as_str()) == Some("software_Package") {
            if let (Some(iri), Some(purl)) = (
                el.get("spdxId").and_then(|v| v.as_str()),
                el.get("software_packageUrl").and_then(|v| v.as_str()),
            ) {
                purl_by_iri.insert(iri.to_string(), purl.to_string());
            }
        }
    }
    let mut out = BTreeSet::new();
    for el in graph {
        if el.get("type").and_then(|v| v.as_str()) != Some("SpdxDocument") {
            continue;
        }
        let Some(roots) = el.get("rootElement").and_then(|v| v.as_array()) else {
            continue;
        };
        for r in roots {
            if let Some(iri) = r.as_str() {
                if let Some(purl) = purl_by_iri.get(iri) {
                    out.insert(purl.clone());
                }
            }
        }
    }
    out
}

// ============================================================
// Section C — annotation stubs (C1-C23 SPDX 3 side)
// ============================================================

spdx3_anno!(c1_spdx3, "mikebom:source-type", component);
spdx3_anno!(c2_spdx3, "mikebom:source-connection-ids", component);
spdx3_anno!(c3_spdx3, "mikebom:deps-dev-match", component);
spdx3_anno!(c4_spdx3, "mikebom:evidence-kind", component);
spdx3_anno!(c5_spdx3, "mikebom:sbom-tier", component);
spdx3_anno!(c7_spdx3, "mikebom:co-owned-by", component);
spdx3_anno!(c8_spdx3, "mikebom:shade-relocation", component);
spdx3_anno!(c9_spdx3, "mikebom:npm-role", component);
spdx3_anno!(c10_spdx3, "mikebom:binary-class", component);
spdx3_anno!(c11_spdx3, "mikebom:binary-stripped", component);
spdx3_anno!(c12_spdx3, "mikebom:linkage-kind", component);
spdx3_anno!(c13_spdx3, "mikebom:buildinfo-status", component);
spdx3_anno!(c14_spdx3, "mikebom:detected-go", component);
spdx3_anno!(c15_spdx3, "mikebom:binary-packed", component);
spdx3_anno!(c16_spdx3, "mikebom:confidence", component);
spdx3_anno!(c17_spdx3, "mikebom:raw-version", component);
spdx3_anno!(c18_spdx3, "mikebom:source-files", component);
spdx3_anno!(c19_spdx3, "mikebom:cpe-candidates", component);
spdx3_anno!(c20_spdx3, "mikebom:requirement-ranges", component);
spdx3_anno!(c21_spdx3, "mikebom:generation-context", document);
spdx3_anno!(c22_spdx3, "mikebom:os-release-missing-fields", document);
spdx3_anno!(c23_spdx3, "mikebom:trace-integrity-ring-buffer-overflows", document);

// C24-C26 (milestone 023 — ELF identity, surfaced via the
// extra_annotations bag in entry.rs::make_file_level_component).
spdx3_anno!(c24_spdx3, "mikebom:elf-build-id", component);
spdx3_anno!(c25_spdx3, "mikebom:elf-runpath", component);
spdx3_anno!(c26_spdx3, "mikebom:elf-debuglink", component);

// C27-C29 (milestone 025 — Go VCS metadata).
spdx3_anno!(c27_spdx3, "mikebom:go-vcs-revision", component);
spdx3_anno!(c28_spdx3, "mikebom:go-vcs-time", component);
spdx3_anno!(c29_spdx3, "mikebom:go-vcs-modified", component);

// C30-C32 (milestone 024 — Mach-O binary identity).
spdx3_anno!(c30_spdx3, "mikebom:macho-uuid", component);
spdx3_anno!(c31_spdx3, "mikebom:macho-rpath", component);
spdx3_anno!(c32_spdx3, "mikebom:macho-min-os", component);

// C33-C35 (milestone 028 — PE binary identity).
spdx3_anno!(c33_spdx3, "mikebom:pe-pdb-id", component);
spdx3_anno!(c34_spdx3, "mikebom:pe-machine", component);
spdx3_anno!(c35_spdx3, "mikebom:pe-subsystem", component);

// C36 (milestone 029 — cargo-auditable cross-link).
spdx3_anno!(c36_spdx3, "mikebom:detected-cargo-auditable", component);

// C37-C39 (milestone 030 — Mach-O codesign metadata).
spdx3_anno!(c37_spdx3, "mikebom:macho-codesign-identifier", component);
spdx3_anno!(c38_spdx3, "mikebom:macho-codesign-flags",      component);
spdx3_anno!(c39_spdx3, "mikebom:macho-codesign-team-id",    component);

// C40 (milestone 048 — component-role classifier).
spdx3_anno!(c40_spdx3, "mikebom:component-role",            component);

// C41 (milestone 050 — not-linked classifier).
spdx3_anno!(c41_spdx3, "mikebom:not-linked",                component);

// C44 removed in milestone 170 — see cdx.rs for context.

// C45 — per-component orphan-reason (milestone 061).
spdx3_anno!(c45_spdx3, "mikebom:orphan-reason",             component);

// C46 — per-component cross-tier source-document binding (milestone 072
// PR-A T008). Carrier shape per
// `contracts/source-document-binding-annotation.md` C-3 SPDX 3.
spdx3_anno!(c46_spdx3, "mikebom:source-document-binding",  component);

// C48 — per-component go-resolver-step provenance (milestone 091).
spdx3_anno!(c48_spdx3, "mikebom:resolver-step",            component);

// C49-C52 — milestone-098 build-tier provenance signals
// (compiler/linker stamps). Emitted as `Annotation` elements with
// `statement = "mikebom:<key>=<value>"` per SPDX 3 conventions.
spdx3_anno!(c49_spdx3, "mikebom:elf-compiler-stamps",      component);
spdx3_anno!(c50_spdx3, "mikebom:macho-build-version",      component);
spdx3_anno!(c51_spdx3, "mikebom:macho-build-tools",        component);
spdx3_anno!(c52_spdx3, "mikebom:pe-linker-version",        component);

// Milestone 103 — Bazel WORKSPACE / CMake source-tree readers.
// Both emit as `Annotation` elements with the
// `mikebom:<key>=<value>` statement prefix.
spdx3_anno!(c53_spdx3, "mikebom:download-url",             component);
spdx3_anno!(c54_spdx3, "mikebom:bazel-archive-name",       component);
// C55 — closed-enum source-mechanism. See cdx.rs for the docs.
spdx3_anno!(c55_spdx3, "mikebom:source-mechanism",         component);
// C56 — `mikebom:also-detected-via` (FR-015). Same shape as
// C56's SPDX 2.3 form: the annotation value is a JSON-array of
// source-mechanism strings; the extractor parses + flattens.
// SymmetricEqual against the CDX-native evidence.identity path.
pub(super) fn c56_spdx3(doc: &Value) -> BTreeSet<String> {
    extract_mikebom_annotation_values(doc, "mikebom:also-detected-via", false)
        .into_iter()
        .filter_map(|json_array_str| {
            serde_json::from_str::<Vec<String>>(&json_array_str).ok()
        })
        .flatten()
        .collect()
}
// C57 — `mikebom:build-reference` (FR-008a). Closed enum.
spdx3_anno!(c57_spdx3, "mikebom:build-reference",          component);

// C58 — `mikebom:fingerprint-corpus-sha` (milestone 108 FR-005).
// 12-hex SHA prefix OR literal `bundled` sentinel.
spdx3_anno!(c58_spdx3, "mikebom:fingerprint-corpus-sha",   component);

// C59 — `mikebom:fingerprint-confidence` (milestone 110 FR-017).
// Numeric "X.XX" fused-confidence string.
spdx3_anno!(c59_spdx3, "mikebom:fingerprint-confidence",   component);

// C60 — `mikebom:build-inclusion` (milestone 112). Open-enum
// `unknown` / `not-needed` — parity bridge (`LifecycleScopeType`
// has no excluded/unknown value).
spdx3_anno!(c60_spdx3, "mikebom:build-inclusion",          component);

// C61 — `mikebom:build-inclusion-derivation` (milestone 112).
// Provenance discriminator for C60 `not-needed`.
spdx3_anno!(c61_spdx3, "mikebom:build-inclusion-derivation", component);

// C62 — `mikebom:lifecycle-scope-derivation` (test-closure fix +
// milestone 112). Unlike C42 (scope itself rides the native
// `LifecycleScopeType`), the derivation has no native carrier, so
// the annotation IS emitted on SPDX 3.
spdx3_anno!(c62_spdx3, "mikebom:lifecycle-scope-derivation", component);

// C63 — `mikebom:exclude-path` (milestone 113 FR-014 / SC-007).
// Envelope-level transparency annotation. Document-scope, mirrors
// CDX `metadata.properties[].mikebom:exclude-path`.
spdx3_anno!(c63_spdx3, "mikebom:exclude-path",             document);

// C64 — `mikebom:produces-binaries` (milestone 116). Per-Package
// graph-element annotation listing produced binary names.
spdx3_anno!(c64_spdx3, "mikebom:produces-binaries",        component);

// C65 — `mikebom:source-tier = "declared"` (milestone 119).
// Per-component graph-element annotation; value-set extension on
// the existing source-tier key.
spdx3_anno!(c65_spdx3, "mikebom:source-tier",              component);

// C66 — `mikebom:supplement-cdx` (milestone 119). Document-scope
// provenance for `--supplement-cdx`; envelope shape mirrors C63.
spdx3_anno!(c66_spdx3, "mikebom:supplement-cdx",           document);

// Milestone 127: C69 — envelope-level mirror of CDX C69. Same emission
// gating as the CDX side.
spdx3_anno!(c69_spdx3, "mikebom:root-selection-heuristic", document);

// Milestone 128: C70..C86 — Yocto annotation family.
spdx3_anno!(c70_spdx3, "mikebom:srcrev",                    component);
spdx3_anno!(c71_spdx3, "mikebom:src-uri",                   component);
spdx3_anno!(c72_spdx3, "mikebom:srcrev-by-machine",         component);
spdx3_anno!(c73_spdx3, "mikebom:yocto-layer",               component);
spdx3_anno!(c74_spdx3, "mikebom:yocto-layer-version",       component);
spdx3_anno!(c75_spdx3, "mikebom:yocto-layer-series",        component);
spdx3_anno!(c76_spdx3, "mikebom:bbappend-applied",          component);
spdx3_anno!(c77_spdx3, "mikebom:depends-unresolved",        component);
spdx3_anno!(c78_spdx3, "mikebom:rdepends-unresolved",       component);
spdx3_anno!(c79_spdx3, "mikebom:yocto-unexpanded-vars",     component);
spdx3_anno!(c80_spdx3, "mikebom:yocto-license-closed",      component);
spdx3_anno!(c81_spdx3, "mikebom:yocto-description",         component);
spdx3_anno!(c82_spdx3, "mikebom:src-uri-local-only",        component);
spdx3_anno!(c83_spdx3, "mikebom:yocto-class-extend",        component);
spdx3_anno!(c84_spdx3, "mikebom:yocto-overrides-merged",    component);
spdx3_anno!(c85_spdx3, "mikebom:yocto-recipe-name",         component);
spdx3_anno!(c86_spdx3, "mikebom:yocto-recipe-version",      component);
spdx3_anno!(c87_spdx3, "mikebom:assembly-version-informational-stripped", component);
spdx3_anno!(c88_spdx3, "mikebom:layer-digest", component);
spdx3_anno!(c91_spdx3, "mikebom:component-tier", component);
spdx3_anno!(c92_spdx3, "mikebom:file-paths", component);
spdx3_anno!(c93_spdx3, "mikebom:file-inventory-skipped-oversize", document);
spdx3_anno!(c94_spdx3, "mikebom:file-inventory-skipped-special-files", document);
spdx3_anno!(c95_spdx3, "mikebom:file-inventory-unreadable", document);
spdx3_anno!(c96_spdx3, "mikebom:file-paths-truncated", component);
spdx3_anno!(c97_spdx3, "mikebom:file-inventory-mode", document);
spdx3_anno!(c98_spdx3, "mikebom:license-concluded-source", component);
// Milestone 134 (closes #125): divergent-PURL detection — see cdx.rs
// for the C99/C100 design notes.
spdx3_anno!(c99_spdx3, "mikebom:duplicate-purl-divergent", component);
spdx3_anno!(c100_spdx3, "mikebom:purl-collisions-detected", document);
// Milestone 147: npm peerDependencies emit DEPENDS_ON edges; the
// peer-edge-targets annotation lists their PURLs so consumers can
// filter install-vs-functional. Per-component graph-element annotation.
spdx3_anno!(c101_spdx3, "mikebom:peer-edge-targets", component);
// Milestone 149 (closes #151): preserves manifest-derived main-module
// as a library entry when --root-name override fires + the new
// --preserve-manifest-main-module flag is set. Per-component
// graph-element annotation.
spdx3_anno!(c102_spdx3, "mikebom:demoted-from-main-module", component);

// C103 — `mikebom:cmake-find-package-name` (milestone 155). Per-component
// graph-element annotation.
spdx3_anno!(c103_spdx3, "mikebom:cmake-find-package-name", component);

// Milestone 158 (closes #492): C104/C105 — document-scope graph-
// completeness signal + reason. See contracts/annotation-schema.md.
spdx3_anno!(c104_spdx3, "mikebom:graph-completeness",        document);
spdx3_anno!(c105_spdx3, "mikebom:graph-completeness-reason", document);

// Milestone 159 (closes #493): C106/C107 — per-component alias-provenance
// annotations.
spdx3_anno!(c106_spdx3, "mikebom:pnpm-alias",                component);
spdx3_anno!(c107_spdx3, "mikebom:yarn-alias",                component);

// Milestone 160 (closes #494): C108/C109 per-component + C110/C111
// document-scope Go-transitive coverage annotations.
spdx3_anno!(c108_spdx3, "mikebom:go-transitive-source",             component);
spdx3_anno!(c109_spdx3, "mikebom:go-transitive-unresolved-reason",  component);
spdx3_anno!(c110_spdx3, "mikebom:go-transitive-coverage",           document);
spdx3_anno!(c111_spdx3, "mikebom:go-transitive-coverage-reason",    document);

// Milestone 161 (closes #495): C112 document-scope Go-workspace-mode
// detection annotation.
spdx3_anno!(c112_spdx3, "mikebom:go-workspace-mode",                document);

// Milestone 162 (closes #496): C113/C114 per-component Ruby built-in
// gem synthetic-component annotations.
spdx3_anno!(c113_spdx3, "mikebom:synthetic-built-in",               component);
spdx3_anno!(c114_spdx3, "mikebom:built-in-requirement",             component);

// Milestone 163 (closes #498): C115 per-component npm workspace-peer
// unresolved-declared-dep annotation.
spdx3_anno!(c115_spdx3, "mikebom:unresolved-declared-dep",          component);
// Milestone 169 (closes #500 Q2): C116 per-source-component
// `mikebom:dep-alternative-alternates` annotation for Debian/opkg alt-lists.
spdx3_anno!(c116_spdx3, "mikebom:dep-alternative-alternates",       component);
// Milestone 172: C117 document-scope
// `mikebom:go-transitive-fallback-count` annotation. Companion to C110.
spdx3_anno!(c117_spdx3, "mikebom:go-transitive-fallback-count",     document);
// Milestone 173: C118 + C119 document-scope Go cache-warming
// annotations.
spdx3_anno!(c118_spdx3, "mikebom:go-cache-warming-mode",            document);
spdx3_anno!(c119_spdx3, "mikebom:go-cache-warming-failed",          document);
// Milestone 176: C120 per-component workspace-member annotation.
spdx3_anno!(c120_spdx3, "mikebom:workspace-member",                 component);
// Milestone 176: C121 document-scope workspaces-detected aggregate.
spdx3_anno!(c121_spdx3, "mikebom:workspaces-detected",              document);
// Milestone 179: C122 per-component optional-dep derivation source.
spdx3_anno!(c122_spdx3, "mikebom:optional-derivation",              component);

// C67 — `mikebom:assertion-conflict` (milestone 119). Per-component
// graph-element annotation carrying the JSON-encoded array of
// conflict records.
spdx3_anno!(c67_spdx3, "mikebom:assertion-conflict",       component);

// C68 — `mikebom:kmp-source-set` (milestone 122). Per-component
// graph-element annotation carrying the JSON-encoded array of Kotlin
// Multiplatform source-set names that declared the dep.
spdx3_anno!(c68_spdx3, "mikebom:kmp-source-set",           component);

// C47 — document-level user-defined identifiers (milestone 073).
// Per `contracts/identifiers-annotation.md` C-1 SPDX 3 and C-2
// SPDX 3: user-defined identifiers ride `Element.externalIdentifier[]`
// natively on the SpdxDocument element rather than a separate
// `mikebom:identifiers` annotation. The C47 row must therefore
// reach into the native carrier and emit the same canonical
// `{scheme, value}` payload that the CDX/SPDX 2.3 sides produce from
// their respective annotation envelopes — filtering OUT the built-in
// schemes (which the CDX/SPDX 2.3 sides exclude from the C47 carrier
// entirely; built-ins ride standards-native carriers per C46-style
// pattern).
//
// Milestone 079 — mikebom's internal scheme names (`image`, `repo`,
// `git`, `subject`, `attestation`) no longer appear in the
// `externalIdentifierType` field; that field now carries the SPDX 3
// controlled-vocab value (`other` for non-vocab built-ins) with the
// original scheme preserved on the `comment` field as
// `original-scheme: <name>`. The C47 extractor reconstructs the
// original mikebom scheme via the comment-prefix recovery and
// continues to filter out built-ins so the cross-format C47 set
// matches CDX / SPDX 2.3.
pub(super) fn c47_spdx3(doc: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Some(graph) = doc.get("@graph").and_then(|v| v.as_array()) else {
        return out;
    };
    for el in graph {
        if el.get("type").and_then(|v| v.as_str()) != Some("SpdxDocument") {
            continue;
        }
        let Some(idents) = el.get("externalIdentifier").and_then(|v| v.as_array()) else {
            continue;
        };
        for ident in idents {
            // Per milestone 079: recover the original mikebom scheme
            // from the `comment` field's `original-scheme: ` prefix
            // when present, else fall through to the vocab value
            // (operator-named-vocab case, e.g., `cve` passthrough).
            let vocab_type = ident
                .get("externalIdentifierType")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let recovered_scheme: String = ident
                .get("comment")
                .and_then(|v| v.as_str())
                .and_then(|c| c.strip_prefix("original-scheme: "))
                .map(|s| s.to_string())
                .unwrap_or_else(|| vocab_type.to_string());
            // Filter to user-defined namespace only (matches the CDX
            // / SPDX 2.3 C47-annotation contents). Includes
            // `subject` per milestone 076.
            if matches!(
                recovered_scheme.as_str(),
                "repo" | "git" | "image" | "attestation" | "subject"
            ) {
                continue;
            }
            let value = ident.get("identifier").and_then(|v| v.as_str()).unwrap_or("");
            // Canonical payload shape: {"scheme":<name>,"value":<value>}.
            // Match the CDX/SPDX 2.3 annotation envelope payload shape
            // (no source_label — manual flags don't have one and
            // user-defined entries today never have an auto-detected
            // label).
            let canonical =
                serde_json::json!({"scheme": recovered_scheme, "value": value});
            // Use compact ordered form — same canonicalization the
            // CDX/SPDX 2.3 annotation extractors produce via
            // canonicalize_atomic_values.
            if let Ok(s) = serde_json::to_string(&canonical) {
                out.insert(s);
            }
        }
    }
    out
}

// ============================================================
// Sections D-G — custom SPDX 3 extractors
// ============================================================

pub(super) fn d1_spdx3(doc: &Value) -> BTreeSet<String> {
    extract_mikebom_annotation_values(doc, "evidence.identity", false)
}
pub(super) fn d2_spdx3(doc: &Value) -> BTreeSet<String> {
    extract_mikebom_annotation_values(doc, "evidence.occurrences", false)
}

pub(super) fn e1_spdx3(doc: &Value) -> BTreeSet<String> {
    extract_mikebom_annotation_values(doc, "compositions", true)
}

// F1 VEX: SPDX 3 emits an externalRef on SpdxDocument with type
// `vulnerabilityExploitabilityAssessment`.
pub(super) fn f1_spdx3(doc: &Value) -> BTreeSet<String> {
    let Some(graph) = doc.get("@graph").and_then(|v| v.as_array()) else {
        return BTreeSet::new();
    };
    let has_ref = graph
        .iter()
        .filter(|el| el.get("type").and_then(|v| v.as_str()) == Some("SpdxDocument"))
        .any(|el| {
            el.get("externalRef")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter().any(|r| {
                        r.get("externalRefType").and_then(|v| v.as_str())
                            == Some("vulnerabilityExploitabilityAssessment")
                    })
                })
                .unwrap_or(false)
        });
    if has_ref {
        BTreeSet::from(["__openvex_sidecar_present__".to_string()])
    } else {
        BTreeSet::new()
    }
}

pub(super) fn g1_spdx3(doc: &Value) -> BTreeSet<String> {
    let Some(graph) = doc.get("@graph").and_then(|v| v.as_array()) else {
        return BTreeSet::new();
    };
    graph
        .iter()
        .filter(|el| el.get("type").and_then(|v| v.as_str()) == Some("Tool"))
        .filter_map(|el| el.get("name").and_then(|v| v.as_str()))
        .map(|s| s.split('-').next().unwrap_or("").to_string())
        .collect()
}
