//! CycloneDX-side parity extractors (milestone 022 commit 2).
//!
//! Owns every `cdx_*` and `c*_cdx` / `d*_cdx` / `e*_cdx` / `f*_cdx` /
//! `g*_cdx` extractor function referenced by `EXTRACTORS` in
//! `super::mod`. The `cdx_property_values` helper + the `cdx_anno!`
//! macro are CDX-internal (single-format equivalent of the
//! pre-022 cross-format `component_anno_extractors!` /
//! `document_anno_extractors!` macros).
//!
//! Visibility: every fn referenced from `super::EXTRACTORS` is
//! `pub(super)`; helpers used only inside this module stay private.

use std::collections::BTreeSet;

use serde_json::Value;

use super::common::{
    canonicalize_atomic_values, walk_cdx_components,
    walk_cdx_components_and_main_module, walk_cdx_components_main_module_and_synth_subject,
};

// ============================================================
// CDX-side property-name extractor — reused by the C-section
// annotation stub generator below.
// ============================================================

/// Yield the set of property values whose `name` matches `field_name`.
/// For component-level properties (`subject_is_document = false`)
/// walks each component's `properties[]`; for document-level (`true`)
/// walks `metadata.properties[]`.
fn cdx_property_values(
    doc: &Value,
    field_name: &str,
    subject_is_document: bool,
) -> BTreeSet<String> {
    let pools: Vec<&Value> = if subject_is_document {
        doc.get("metadata")
            .and_then(|m| m.get("properties"))
            .into_iter()
            .collect()
    } else {
        // Milestone 053 FR-004 + C18 + C40: include
        // `metadata.component`'s properties[] when the metadata-
        // component is the Go main-module (per FR-001a). The main-
        // module carries C40 (`mikebom:component-role`), C18
        // (`mikebom:source-files`), and `mikebom:sbom-tier`
        // promoted to metadata.component-level properties; without
        // walking metadata.component the parity-extractor
        // SymmetricEqual checks diverge from the SPDX side, where
        // the main-module is a regular `packages[]` entry.
        walk_cdx_components_and_main_module(doc)
            .into_iter()
            .filter_map(|c| c.get("properties"))
            .collect()
    };
    let mut out = BTreeSet::new();
    for pool in pools {
        let Some(arr) = pool.as_array() else { continue };
        for p in arr {
            if p.get("name").and_then(|v| v.as_str()) != Some(field_name) {
                continue;
            }
            let Some(value) = p.get("value") else { continue };
            // Canonicalize via the same flatten-and-decode helper as
            // the SPDX side so byte-equivalent atomic values collapse
            // identically across formats — handles JSON-encoded
            // scalars (`"true"` → `true`) and array values both
            // inline (`[a,b]`) and split-per-property.
            for v in canonicalize_atomic_values(value) {
                out.insert(v);
            }
        }
    }
    out
}

/// Single-format C-section stub generator. Component-scope:
/// `cdx_anno!(c1_cdx, "mikebom:source-type", component);`
/// Document-scope:
/// `cdx_anno!(c21_cdx, "mikebom:generation-context", document);`
macro_rules! cdx_anno {
    ($name:ident, $field:literal, component) => {
        pub(super) fn $name(doc: &Value) -> BTreeSet<String> {
            cdx_property_values(doc, $field, false)
        }
    };
    ($name:ident, $field:literal, document) => {
        pub(super) fn $name(doc: &Value) -> BTreeSet<String> {
            cdx_property_values(doc, $field, true)
        }
    };
}

// ============================================================
// Section A — Core identity (A1-A12)
// ============================================================

pub(super) fn cdx_purl(doc: &Value) -> BTreeSet<String> {
    // Milestone 053: include metadata.component when it's the Go
    // main-module (FR-001a). The main-module's PURL round-trips
    // identically to a regular components[] entry's, so SymmetricEqual
    // parity holds.
    walk_cdx_components_and_main_module(doc)
        .iter()
        .filter_map(|c| c.get("purl").and_then(|v| v.as_str()).map(String::from))
        .collect()
}

pub(super) fn cdx_name(doc: &Value) -> BTreeSet<String> {
    walk_cdx_components_and_main_module(doc)
        .iter()
        .filter_map(|c| c.get("name").and_then(|v| v.as_str()).map(String::from))
        .collect()
}

pub(super) fn cdx_version(doc: &Value) -> BTreeSet<String> {
    walk_cdx_components_and_main_module(doc)
        .iter()
        .filter_map(|c| c.get("version").and_then(|v| v.as_str()).map(String::from))
        // Milestone 133 US1.C: drop empty-string versions so the A3
        // SymmetricEqual check tolerates the SPDX 3 file-element
        // shape, which conditionally OMITS `software_packageVersion`
        // when the version is empty (file-tier components have no
        // version concept per FR-009). CDX emits `"version": ""`
        // verbatim for those components; without this filter the A3
        // row sees `{""}` only on the CDX side.
        .filter(|s| !s.is_empty())
        .collect()
}

pub(super) fn cdx_hashes(doc: &Value) -> BTreeSet<String> {
    walk_cdx_components(doc)
        .iter()
        .flat_map(|c| {
            c.get("hashes")
                .and_then(|v| v.as_array())
                .map(|arr| arr.as_slice())
                .unwrap_or(&[])
                .iter()
                .filter_map(|h| {
                    let alg = h.get("alg").and_then(|v| v.as_str())?;
                    let content = h.get("content").and_then(|v| v.as_str())?;
                    Some(format!("{}:{}", super::common::normalize_alg(alg), content))
                })
        })
        .collect()
}

fn cdx_external_ref_by_type(doc: &Value, ref_type: &str) -> BTreeSet<String> {
    walk_cdx_components(doc)
        .iter()
        .flat_map(|c| {
            c.get("externalReferences")
                .and_then(|v| v.as_array())
                .map(|arr| arr.as_slice())
                .unwrap_or(&[])
                .iter()
                .filter(|r| r.get("type").and_then(|v| v.as_str()) == Some(ref_type))
                .filter_map(|r| r.get("url").and_then(|v| v.as_str()).map(String::from))
        })
        .collect()
}

pub(super) fn cdx_homepage(doc: &Value) -> BTreeSet<String> {
    let mut out = cdx_external_ref_by_type(doc, "website");
    out.extend(cdx_external_ref_by_type(doc, "homepage"));
    out
}
pub(super) fn cdx_vcs(doc: &Value) -> BTreeSet<String> {
    cdx_external_ref_by_type(doc, "vcs")
}
pub(super) fn cdx_distribution(doc: &Value) -> BTreeSet<String> {
    cdx_external_ref_by_type(doc, "distribution")
}

pub(super) fn cdx_cpe(doc: &Value) -> BTreeSet<String> {
    walk_cdx_components_and_main_module(doc)
        .iter()
        .filter_map(|c| c.get("cpe").and_then(|v| v.as_str()).map(String::from))
        .collect()
}

/// Milestone 104 — per-component role from CDX `Component.type`.
/// Returns `<purl>=<role>` strings keyed by PURL so the parity
/// comparison is component-wise.
///
/// Scoped to binary-reader-emitted components only — detected via
/// presence of the `mikebom:binary-class` property (set by the
/// binary reader on every emitted component, never by other
/// readers). Non-binary-reader components emit CDX `type: library`
/// by the per-ecosystem default while SPDX 2.3 / SPDX 3 omit the
/// `primaryPackagePurpose` / `software_primaryPurpose` field for
/// them. Including them would create a false-positive
/// SymmetricEqual failure since the formats genuinely diverge for
/// non-binary components by design.
pub(super) fn cdx_binary_role(doc: &Value) -> BTreeSet<String> {
    walk_cdx_components(doc)
        .iter()
        .filter_map(|c| {
            let purl = c.get("purl").and_then(|v| v.as_str())?;
            let ty = c.get("type").and_then(|v| v.as_str())?;
            // Only binary-reader-emitted components carry
            // `mikebom:binary-class` — restrict the parity check to
            // them.
            let from_binary_reader = c
                .get("properties")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter().any(|p| {
                        p.get("name").and_then(|v| v.as_str())
                            == Some("mikebom:binary-class")
                    })
                })
                .unwrap_or(false);
            if !from_binary_reader {
                return None;
            }
            match ty {
                "application" | "library" | "file" => {
                    Some(format!("{purl}={ty}"))
                }
                _ => None,
            }
        })
        .collect()
}

fn cdx_licenses_typed(doc: &Value, ack: &str) -> BTreeSet<String> {
    walk_cdx_components(doc)
        .iter()
        .flat_map(|c| {
            c.get("licenses")
                .and_then(|v| v.as_array())
                .map(|arr| arr.as_slice())
                .unwrap_or(&[])
                .iter()
                .filter(|l| {
                    // CDX 1.6 nests acknowledgement inside the
                    // `license` object for {license: {id, name,
                    // acknowledgement}}, and at the top of the
                    // entry for {expression, acknowledgement}.
                    let nested = l
                        .get("license")
                        .and_then(|li| li.get("acknowledgement"))
                        .and_then(|v| v.as_str());
                    let top = l.get("acknowledgement").and_then(|v| v.as_str());
                    nested == Some(ack) || top == Some(ack)
                })
                .filter_map(|l| {
                    if let Some(id) = l.get("license")
                        .and_then(|li| li.get("id"))
                        .and_then(|v| v.as_str())
                    {
                        return Some(id.to_string());
                    }
                    if let Some(name) = l
                        .get("license")
                        .and_then(|li| li.get("name"))
                        .and_then(|v| v.as_str())
                    {
                        return Some(name.to_string());
                    }
                    if let Some(expr) = l.get("expression").and_then(|v| v.as_str()) {
                        return Some(expr.to_string());
                    }
                    None
                })
        })
        .collect()
}
pub(super) fn cdx_licenses_declared(doc: &Value) -> BTreeSet<String> {
    cdx_licenses_typed(doc, "declared")
}
pub(super) fn cdx_licenses_concluded(doc: &Value) -> BTreeSet<String> {
    cdx_licenses_typed(doc, "concluded")
}

pub(super) fn cdx_supplier(doc: &Value) -> BTreeSet<String> {
    walk_cdx_components_and_main_module(doc)
        .iter()
        .filter_map(|c| {
            c.get("supplier")
                .and_then(|s| s.get("name"))
                .and_then(|v| v.as_str())
                .map(String::from)
        })
        .collect()
}

// ============================================================
// Section B — Graph structure (B1-B4)
// ============================================================

/// Collect (from_purl, to_purl) edges from CDX `dependencies[]`.
/// Uses `bom-ref` → `purl` lookup since dependencies are keyed
/// by bom-ref. Milestone 052/part-2: filter dev/non-dev via the
/// native CDX `scope: "excluded"` field plus the new
/// `mikebom:lifecycle-scope` property (the legacy
/// `mikebom:dev-dependency` annotation was removed). The scope
/// signal lives on the TARGET component (the dep target), not
/// the source, in B2's new shape — but for parity-test purposes
/// (SymmetricEqual against SPDX 2.3 dep types and SPDX 3
/// lifecycleScope on the target relationship), `dev_only`
/// remains a source-side filter to match how the SPDX side
/// extractors classify edges.
fn cdx_dependency_edges(doc: &Value, dev_only: bool) -> BTreeSet<String> {
    // Build bom-ref → component lookup. Milestone 053: include
    // metadata.component-when-main-module so its outgoing
    // `dependencies[].ref` lookups resolve.
    //
    // Issue #236: also include `metadata.component` when it's a
    // synthetic scan-subject placeholder (no main-module tag) — the
    // primary-dep fallback at `cyclonedx/dependencies.rs:74-99`
    // synthesizes edges sourced at the placeholder's bom-ref, and
    // those edges now have a matching synth-root → top-level edge
    // in SPDX 2.3 + SPDX 3 (issue-#236 fix). Without the synth
    // subject in this lookup the CDX bucket would silently drop
    // those edges and parity tests would see a false-positive
    // SPDX-vs-CDX divergence.
    let mut comp_by_bomref: std::collections::BTreeMap<String, &Value> =
        std::collections::BTreeMap::new();
    for c in walk_cdx_components_main_module_and_synth_subject(doc) {
        if let Some(bref) = c.get("bom-ref").and_then(|v| v.as_str()) {
            comp_by_bomref.insert(bref.to_string(), c);
        }
    }
    let mut out = BTreeSet::new();
    let Some(deps) = doc.get("dependencies").and_then(|v| v.as_array()) else {
        return out;
    };
    for d in deps {
        let Some(from_ref) = d.get("ref").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(from_comp) = comp_by_bomref.get(from_ref) else {
            continue;
        };
        let from_purl = match from_comp.get("purl").and_then(|v| v.as_str()) {
            Some(p) => p.to_string(),
            None => continue,
        };
        // Milestone 052/part-2: dev classification via native CDX
        // `scope: "excluded"` (any non-runtime).
        //
        // Milestone 085: classify per-edge using EITHER endpoint's
        // scope, not just the source-side. Maven (and any future
        // ecosystem that puts scope on the target rather than the
        // source — e.g., pom.xml `<scope>test</scope>` lives on the
        // dep declaration, attached to the target component) needs
        // the target-side check or runtime-vs-dev parity against
        // SPDX 2.3's typed `DEPENDS_ON`/`TEST_DEPENDENCY_OF` shape
        // produces false positives in CDX's runtime bucket. Pre-085
        // the source-side-only filter masked this for ecosystems
        // (cargo etc.) where dev/test deps happened to be excluded
        // from `dependencies[]` entirely; post-085 the per-edge
        // check correctly classifies edges where any endpoint
        // carries scope=excluded as non-runtime.
        let from_is_dev = from_comp
            .get("scope")
            .and_then(|v| v.as_str())
            == Some("excluded");
        let Some(targets) = d.get("dependsOn").and_then(|v| v.as_array()) else {
            continue;
        };
        for t in targets {
            let Some(to_ref) = t.as_str() else { continue };
            let Some(to_comp) = comp_by_bomref.get(to_ref) else {
                continue;
            };
            let Some(to_purl) = to_comp.get("purl").and_then(|v| v.as_str()) else {
                continue;
            };
            let to_is_dev = to_comp.get("scope").and_then(|v| v.as_str()) == Some("excluded");
            let edge_is_dev = from_is_dev || to_is_dev;
            if dev_only != edge_is_dev {
                continue;
            }
            out.insert(format!("{from_purl}->{to_purl}"));
        }
    }
    out
}

pub(super) fn cdx_runtime_deps(doc: &Value) -> BTreeSet<String> {
    cdx_dependency_edges(doc, false)
}
pub(super) fn cdx_dev_deps(doc: &Value) -> BTreeSet<String> {
    cdx_dependency_edges(doc, true)
}

// B3 nested containment: CDX nests via `component.components[]`.
// Returns set of `parent_purl->child_purl` strings walked from the
// nested structure.
pub(super) fn cdx_containment(doc: &Value) -> BTreeSet<String> {
    fn recur<'a>(parent: Option<&'a str>, node: &'a Value, out: &mut BTreeSet<String>) {
        if let Some(arr) = node.get("components").and_then(|v| v.as_array()) {
            for c in arr {
                let purl = c.get("purl").and_then(|v| v.as_str());
                if let (Some(p), Some(child)) = (parent, purl) {
                    out.insert(format!("{p}->{child}"));
                }
                recur(purl, c, out);
            }
        }
    }
    let mut out = BTreeSet::new();
    recur(None, doc, &mut out);
    out
}

// B4 root: CDX `metadata.component.purl` (singleton).
pub(super) fn cdx_root(doc: &Value) -> BTreeSet<String> {
    doc.get("metadata")
        .and_then(|m| m.get("component"))
        .and_then(|c| c.get("purl"))
        .and_then(|v| v.as_str())
        .map(|s| BTreeSet::from([s.to_string()]))
        .unwrap_or_default()
}

// ============================================================
// Section C — mikebom-specific annotations (C1-C23 CDX side)
// ============================================================

cdx_anno!(c1_cdx, "mikebom:source-type", component);
cdx_anno!(c2_cdx, "mikebom:source-connection-ids", component);
cdx_anno!(c3_cdx, "mikebom:deps-dev-match", component);
cdx_anno!(c4_cdx, "mikebom:evidence-kind", component);
cdx_anno!(c5_cdx, "mikebom:sbom-tier", component);
cdx_anno!(c42_cdx, "mikebom:lifecycle-scope",        component);
cdx_anno!(c7_cdx, "mikebom:co-owned-by", component);
cdx_anno!(c8_cdx, "mikebom:shade-relocation", component);
cdx_anno!(c9_cdx, "mikebom:npm-role", component);
cdx_anno!(c10_cdx, "mikebom:binary-class", component);
cdx_anno!(c11_cdx, "mikebom:binary-stripped", component);
cdx_anno!(c12_cdx, "mikebom:linkage-kind", component);
cdx_anno!(c13_cdx, "mikebom:buildinfo-status", component);
cdx_anno!(c14_cdx, "mikebom:detected-go", component);
cdx_anno!(c15_cdx, "mikebom:binary-packed", component);
cdx_anno!(c16_cdx, "mikebom:confidence", component);
cdx_anno!(c17_cdx, "mikebom:raw-version", component);
cdx_anno!(c18_cdx, "mikebom:source-files", component);

/// C19 cpe-candidates: CDX serializes the candidate list as a
/// pipe-separated string per property (mikebom convention,
/// matching the CycloneDX `cpe` field's single-value cardinality);
/// SPDX emits each candidate as its own annotation. Split the CDX
/// pipe-string into atoms so the directional containment test
/// (`CDX ⊆ SPDX`) compares apples-to-apples atomic CPEs.
pub(super) fn c19_cdx(doc: &Value) -> BTreeSet<String> {
    cdx_property_values(doc, "mikebom:cpe-candidates", false)
        .into_iter()
        .flat_map(|raw| {
            // `cdx_property_values` JSON-encodes the string ⇒ the
            // raw entry is `"cpe1 | cpe2"` (quotes-wrapped). Strip
            // the outer quotes before splitting on the pipe
            // delimiter, then re-encode each atom via `to_string`
            // so the form matches the SPDX side
            // (`"cpe1"` / `"cpe2"` post-canonicalization).
            let unquoted = raw.trim_matches('"');
            unquoted
                .split(" | ")
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| serde_json::to_string(s).unwrap_or_else(|_| s.to_string()))
                .collect::<Vec<_>>()
        })
        .collect()
}

cdx_anno!(c20_cdx, "mikebom:requirement-range", component);

// C21-C23 (document-level).
cdx_anno!(c21_cdx, "mikebom:generation-context", document);
cdx_anno!(c22_cdx, "mikebom:os-release-missing-fields", document);
// C23 actually expands into 4 sub-fields (ring-buffer-overflows,
// events-dropped, uprobe-attach-failures, kprobe-attach-failures);
// the parity test treats it as one row per the catalog. Use the
// ring-buffer-overflows scalar as the canary; the other three
// share the same emit path.
cdx_anno!(c23_cdx, "mikebom:trace-integrity-ring-buffer-overflows", document);

// C24-C26 (milestone 023 — ELF identity, surfaced via the
// extra_annotations bag in entry.rs::make_file_level_component).
cdx_anno!(c24_cdx, "mikebom:elf-build-id", component);
cdx_anno!(c25_cdx, "mikebom:elf-runpath", component);
cdx_anno!(c26_cdx, "mikebom:elf-debuglink", component);

// C27-C29 (milestone 025 — Go VCS metadata, surfaced via the
// extra_annotations bag in go_binary.rs::build_vcs_annotations on
// the main-module Go entry only).
cdx_anno!(c27_cdx, "mikebom:go-vcs-revision", component);
cdx_anno!(c28_cdx, "mikebom:go-vcs-time", component);
cdx_anno!(c29_cdx, "mikebom:go-vcs-modified", component);

// C30-C32 (milestone 024 — Mach-O binary identity, surfaced via the
// extra_annotations bag in binary/entry.rs::build_macho_identity_annotations
// on the file-level Mach-O component).
cdx_anno!(c30_cdx, "mikebom:macho-uuid", component);
cdx_anno!(c31_cdx, "mikebom:macho-rpath", component);
cdx_anno!(c32_cdx, "mikebom:macho-min-os", component);

// C33-C35 (milestone 028 — PE binary identity, surfaced via the
// extra_annotations bag in binary/entry.rs::build_pe_identity_annotations
// on the file-level PE component).
cdx_anno!(c33_cdx, "mikebom:pe-pdb-id", component);
cdx_anno!(c34_cdx, "mikebom:pe-machine", component);
cdx_anno!(c35_cdx, "mikebom:pe-subsystem", component);

// C36 (milestone 029 — cargo-auditable cross-link, surfaced via the
// extra_annotations bag in binary/entry.rs::build_cargo_auditable_cross_link
// on the file-level Rust binary component).
cdx_anno!(c36_cdx, "mikebom:detected-cargo-auditable", component);

// C37-C39 (milestone 030 — Mach-O codesign metadata, surfaced via
// the extra_annotations bag in binary/entry.rs::build_macho_identity_annotations
// on the file-level Mach-O component).
cdx_anno!(c37_cdx, "mikebom:macho-codesign-identifier", component);
cdx_anno!(c38_cdx, "mikebom:macho-codesign-flags",      component);
cdx_anno!(c39_cdx, "mikebom:macho-codesign-team-id",    component);

// C40 — component-role classifier (milestone 048). Filesystem-
// position-classified role: `build-tool`, `language-runtime`, or
// (when no heuristic matches) absent.
cdx_anno!(c40_cdx, "mikebom:component-role",            component);

// C41 — not-linked classifier (milestone 050). Set on Go source-tier
// components when a Go binary is also present in the rootfs AND the
// binary's BuildInfo does NOT confirm the component as linked.
cdx_anno!(c41_cdx, "mikebom:not-linked",                 component);

// C44 — doc-level Go graph-completeness signal (milestone 061,
// closes #119). The annotation has TWO field-name members
// (`mikebom:graph-completeness` + `mikebom:graph-completeness-reason`)
// emitted as separate metadata properties; the parity extractor
// pulls both into one set for the SymmetricEqual check.
pub(super) fn c44_cdx(doc: &serde_json::Value) -> std::collections::BTreeSet<String> {
    let mut out = cdx_property_values(doc, "mikebom:graph-completeness", true);
    out.extend(cdx_property_values(doc, "mikebom:graph-completeness-reason", true));
    out
}

// C45 — per-component orphan-reason (milestone 061, closes #119).
cdx_anno!(c45_cdx, "mikebom:orphan-reason",              component);

// C46 — per-component cross-tier source-document binding (milestone 072
// PR-A T008). Emitted on every non-source-tier component (i.e.,
// `mikebom:sbom-tier: build` or `deployed`) that binds back to a
// source-tier SBOM. Carrier shape per
// `contracts/source-document-binding-annotation.md` C-3 CDX 1.6.
cdx_anno!(c46_cdx, "mikebom:source-document-binding",   component);

// C47 — document-level user-defined source identifiers (milestone 073).
// CDX carrier: `metadata.properties[]` entry whose `value` is a
// JSON-encoded sorted-by-(scheme,value) array of {scheme, value,
// source_label?} objects. Built-in identifiers do NOT appear here —
// they ride standards-native carriers (CDX `externalReferences[]` with
// per-scheme `type`). SPDX 2.3 carries the same payload via the
// document-level annotation envelope. SPDX 3 carries every identifier
// (built-in + user-defined) natively via `Element.externalIdentifier[]`,
// so on the SPDX 3 side the annotation is intentionally absent and the
// extractor reaches into the native carrier instead — see
// c47_spdx3 below.
cdx_anno!(c47_cdx, "mikebom:identifiers",               document);

// C48 — per-component go-resolver-step provenance discriminator
// (milestone 091, closes #174). Tags Go transitive components reached
// via step 5 (go.sum flat fallback) so consumers can distinguish the
// lower-fidelity discovery path from steps 1–3.
cdx_anno!(c48_cdx, "mikebom:resolver-step",             component);

// C49-C52 — milestone-098 build-tier provenance signals
// (compiler/linker stamps). All four properties are emitted as
// `component.properties[].value` entries on file-level binary
// components. Symmetric-equal directionality, mixed array/scalar
// order-sensitivity per the catalog rows in mod.rs.
cdx_anno!(c49_cdx, "mikebom:elf-compiler-stamps",       component);
cdx_anno!(c50_cdx, "mikebom:macho-build-version",       component);
cdx_anno!(c51_cdx, "mikebom:macho-build-tools",         component);
cdx_anno!(c52_cdx, "mikebom:pe-linker-version",         component);

// Milestone 103 — Bazel WORKSPACE / CMake source-tree readers.
// `mikebom:download-url`: declared upstream archive URL from Bazel
// `http_archive.urls[0]` or CMake `FetchContent_Declare(URL ...)` /
// `ExternalProject_Add(URL ...)`. `mikebom:bazel-archive-name`:
// original `http_archive.name = "..."` label when sanitization
// re-maps it. Both emit as `component.properties[].value` entries.
cdx_anno!(c53_cdx, "mikebom:download-url",              component);
cdx_anno!(c54_cdx, "mikebom:bazel-archive-name",        component);
// C55 — closed-enum source-mechanism identifying which C/C++
// reader emitted the component (cmake-fetchcontent-git /
// cmake-fetchcontent-url / cmake-externalproject / cmake-vendored /
// bazel-http-archive / vcpkg-manifest / conan-recipe / + milestone-105
// additions: cpm-cmake / zephyr-west / idf-component /
// idf-component-local / vcpkg-classic / git-submodule).
cdx_anno!(c55_cdx, "mikebom:source-mechanism",          component);

// C56 — `mikebom:also-detected-via` (FR-015). Records the
// source-mechanism values of OTHER readers that produced the same
// canonical PURL as the winning reader. Lets downstream consumers
// see multi-reader corroboration without inflating component count.
//
// CDX-native emission per research R1: each detection record is a
// `methods[]` entry under `evidence.identity[0].methods[]` carrying
// `{technique, confidence, mikebom-source-mechanism}`. The FIRST
// method entry is the winning reader (its source-mechanism is the
// component's top-level `mikebom:source-mechanism` property already
// covered by C55); the remaining entries are the losers. C56
// extracts the loser set.
//
// On components that weren't dedup'd (single-reader detection),
// `methods[]` has zero or one entry → empty set returned. The
// SPDX-side extractor (`c56_spdx23` / `c56_spdx3`) reads the
// `mikebom:also-detected-via` JSON-array annotation and yields the
// same BTreeSet for SymmetricEqual parity.
pub(super) fn c56_cdx(doc: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for component in walk_cdx_components_and_main_module(doc) {
        let methods = match component
            .get("evidence")
            .and_then(|e| e.get("identity"))
            .and_then(|i| i.as_array())
            .and_then(|arr| arr.first())
            .and_then(|first| first.get("methods"))
            .and_then(|m| m.as_array())
        {
            Some(m) => m,
            None => continue,
        };
        // Skip the first method (= winner). The remaining methods'
        // `mikebom-source-mechanism` sub-field values are the losers.
        for method in methods.iter().skip(1) {
            if let Some(s) = method
                .get("mikebom-source-mechanism")
                .and_then(|v| v.as_str())
            {
                out.insert(s.to_string());
            }
        }
    }
    out
}

// C57 — `mikebom:build-reference` (FR-008a). Closed enum
// `declared-and-used` / `declared-only` attached to git-submodule
// components. Lets vuln scanners filter un-referenced submodules
// with a single query. Simple property — uses the standard macro.
cdx_anno!(c57_cdx, "mikebom:build-reference",           component);

// C58 — `mikebom:fingerprint-corpus-sha` (milestone 108 FR-005).
// 12-hex SHA prefix OR literal `bundled` sentinel attached to
// components identified via the symbol-fingerprint matcher.
// Emitted only when the operator opted in via `--fingerprints-corpus`
// (preserves SC-003 byte-identity for non-opt-in scans). Simple
// property — uses the standard macro.
cdx_anno!(c58_cdx, "mikebom:fingerprint-corpus-sha",    component);

// C59 — `mikebom:fingerprint-confidence` (milestone 110 FR-017).
// Numeric fused-confidence value formatted as "X.XX" attached to
// fingerprint-derived components. Co-gated with C58 on the
// `--fingerprints-corpus` opt-in. Distinct from the C16
// `mikebom:confidence` enum-string carrier (value="heuristic") so
// no value-space collision. Simple property — uses the standard macro.
cdx_anno!(c59_cdx, "mikebom:fingerprint-confidence",    component);

// C60 — `mikebom:build-inclusion` (milestone 112). Open-enum
// `unknown` / `not-needed` build-participation marker on Go
// components discovered via the lower-fidelity fallback paths
// (C48 go-sum-fallback / C45 flat-attached-fallback) or proven
// not-needed by `go mod why -m`. Simple property — the native CDX
// `scope: "excluded"` companion (not-needed only) rides the native
// field and is NOT part of this row's parity payload.
cdx_anno!(c60_cdx, "mikebom:build-inclusion",           component);

// C61 — `mikebom:build-inclusion-derivation` (milestone 112).
// Provenance discriminator naming the evidence source for a C60
// `not-needed` verdict (value this milestone: `go-mod-why`). NOT
// emitted alongside `unknown`. Simple property — standard macro.
cdx_anno!(c61_cdx, "mikebom:build-inclusion-derivation", component);

// C62 — `mikebom:lifecycle-scope-derivation` (test-closure
// propagation fix + milestone 112). Open-enum `test-only-closure` /
// `go-mod-why` on graph-derived test-scoped components; absent on
// direct-import-walk test tags. Simple property — standard macro.
cdx_anno!(c62_cdx, "mikebom:lifecycle-scope-derivation", component);

// C63 — `mikebom:exclude-path` (milestone 113 FR-014 / SC-007).
// Envelope-level transparency annotation listing every user-supplied
// directory-exclusion entry that was in effect for the scan. Emitted
// only when at least one entry was supplied; absent (+ byte-identical
// pre-feature output) when no exclusions in effect.
cdx_anno!(c63_cdx, "mikebom:exclude-path",              document);

// C64 — `mikebom:produces-binaries` (milestone 116 FR-001 / FR-005..010).
// Per-main-module-component annotation listing the canonical (lowercase,
// extensionless, sorted+deduped) binary names the source's ecosystem
// manifest declares. Read by the cross-tier `--bind-to-source` flow to
// auto-alias `pkg:generic/<name>` image components to the source-tier
// ecosystem PURL. Library-only components MUST NOT carry this property.
cdx_anno!(c64_cdx, "mikebom:produces-binaries",         component);

// C65 — `mikebom:source-tier = "declared"` (milestone 119 FR-011 / SC-004).
// Per-component value-extension on the existing source-tier key for
// supplement-introduced solo entries (collisions keep the scanner's
// pre-existing tier). The existing C5 extractor reads the same key, so
// this extractor is a value-agnostic duplicate registered under the
// C65 row_id so the catalog-coverage test recognizes the value-set
// extension as separately documented.
cdx_anno!(c65_cdx, "mikebom:source-tier",               component);

// C66 — `mikebom:supplement-cdx` (milestone 119 FR-012 / SC-004).
// Envelope-level provenance recording the operator-supplied supplement
// file's verbatim path + sha256. Emitted only when --supplement-cdx
// was in effect; absent (+ byte-identical pre-feature output) when no
// supplement was supplied.
cdx_anno!(c66_cdx, "mikebom:supplement-cdx",            document);

// C67 — `mikebom:assertion-conflict` (milestone 119 FR-008 / FR-009 / SC-003).
// Per-component conflict-record annotation. Repeatable conflicts on
// one component accumulate into a JSON-encoded array under the same
// property key. Emitted only on components where supplement-declared
// values contradicted scanner-discovered values; absent on all other
// components.
cdx_anno!(c67_cdx, "mikebom:assertion-conflict",        component);

// C68 — `mikebom:kmp-source-set` (milestone 122 FR-006).
// Per-component Kotlin Multiplatform source-set provenance. JSON-
// encoded array of source-set names (lex-sorted, deduped). Emitted
// only on components discovered from a `kotlin { sourceSets { ... } }`
// block in `build.gradle.kts`; absent on non-KMP components.
cdx_anno!(c68_cdx, "mikebom:kmp-source-set",            component);

// Milestone 127: C69 `mikebom:root-selection-heuristic` — envelope-level
// signal naming the heuristic + confidence that elected the BOM subject.
// Emitted ONLY when the new ladder fired AND the auto-pick fell through
// past at least one detected main-module. Pattern parallels C63
// (exclude-path) and C66 (supplement-cdx).
cdx_anno!(c69_cdx, "mikebom:root-selection-heuristic",  document);

// Milestone 128: C70..C86 — Yocto/OpenEmbedded source-tier
// annotation family (recipe enrichment per FR-001..FR-019 + FR-002a).
// All per-component-scope per data-model.md + contracts/annotation-schema.md.
cdx_anno!(c70_cdx, "mikebom:srcrev",                    component);
cdx_anno!(c71_cdx, "mikebom:src-uri",                   component);
cdx_anno!(c72_cdx, "mikebom:srcrev-by-machine",         component);
cdx_anno!(c73_cdx, "mikebom:yocto-layer",               component);
cdx_anno!(c74_cdx, "mikebom:yocto-layer-version",       component);
cdx_anno!(c75_cdx, "mikebom:yocto-layer-series",        component);
cdx_anno!(c76_cdx, "mikebom:bbappend-applied",          component);
cdx_anno!(c77_cdx, "mikebom:depends-unresolved",        component);
cdx_anno!(c78_cdx, "mikebom:rdepends-unresolved",       component);
cdx_anno!(c79_cdx, "mikebom:yocto-unexpanded-vars",     component);
cdx_anno!(c80_cdx, "mikebom:yocto-license-closed",      component);
cdx_anno!(c81_cdx, "mikebom:yocto-description",         component);
cdx_anno!(c82_cdx, "mikebom:src-uri-local-only",        component);
cdx_anno!(c83_cdx, "mikebom:yocto-class-extend",        component);
cdx_anno!(c84_cdx, "mikebom:yocto-overrides-merged",    component);
cdx_anno!(c85_cdx, "mikebom:yocto-recipe-name",         component);
cdx_anno!(c86_cdx, "mikebom:yocto-recipe-version",      component);
cdx_anno!(c87_cdx, "mikebom:assembly-version-informational-stripped", component);
cdx_anno!(c88_cdx, "mikebom:layer-digest", component);
cdx_anno!(c91_cdx, "mikebom:component-tier", component);
cdx_anno!(c92_cdx, "mikebom:file-paths", component);
cdx_anno!(c93_cdx, "mikebom:file-inventory-skipped-oversize", document);
cdx_anno!(c94_cdx, "mikebom:file-inventory-skipped-special-files", document);
cdx_anno!(c95_cdx, "mikebom:file-inventory-unreadable", document);
cdx_anno!(c96_cdx, "mikebom:file-paths-truncated", component);
cdx_anno!(c97_cdx, "mikebom:file-inventory-mode", document);
cdx_anno!(c98_cdx, "mikebom:license-concluded-source", component);
// Milestone 134 (closes #125):
//   C99  — `mikebom:duplicate-purl-divergent` per-component property
//          on the deduped root component for every detected divergent
//          collision. Stamped via the cargo reader's `extra_annotations`
//          bag and emitted via the standard per-component property
//          carrier in CDX, the standard `extra_annotations` envelope
//          in SPDX 2.3/3, and propagated to `metadata.component` for
//          single-crate workspaces that promote the main-module to BOM
//          subject.
//   C100 — `mikebom:purl-collisions-detected` document-scope summary
//          (CDX `metadata.properties[]`, SPDX 2.3 top-level
//          `annotations[]`, SPDX 3 `SpdxDocument` element-level
//          annotation). Carries the full `CollisionsSummary` envelope
//          aggregating every divergent collision in the scan.
cdx_anno!(c99_cdx, "mikebom:duplicate-purl-divergent", component);
cdx_anno!(c100_cdx, "mikebom:purl-collisions-detected", document);
// Milestone 147: npm peerDependencies emit DEPENDS_ON edges; the
// peer-edge-targets annotation lists their PURLs so consumers can
// filter install-vs-functional. Per-component property.
cdx_anno!(c101_cdx, "mikebom:peer-edge-targets", component);
// Milestone 149 (closes #151): preserves manifest-derived main-module
// as a library entry when --root-name override fires + the new
// --preserve-manifest-main-module flag is set. Per-component property.
cdx_anno!(c102_cdx, "mikebom:demoted-from-main-module", component);

// ============================================================
// Section D — Evidence (D1, D2 — CDX-native shape)
// ============================================================

// CDX shape is different (native evidence model under
// `component.evidence`) — use a custom CDX extractor that
// serializes the array verbatim.
pub(super) fn d1_cdx(doc: &Value) -> BTreeSet<String> {
    walk_cdx_components(doc)
        .iter()
        .filter_map(|c| {
            let id = c.get("evidence")?.get("identity")?;
            // Match the SPDX-side serialized shape: an array of
            // {technique, confidence}. CDX has the array under
            // evidence.identity.
            serde_json::to_string(id).ok()
        })
        .collect()
}
// Walk every CDX `evidence.occurrences[]` element and normalize it to
// the same flat-key shape the SPDX 2.3/3 D2 extractors produce so the
// `Directionality::SymmetricEqual` assertion at row D2 holds across
// formats. The CDX-spec occurrence shape is
// `{location, additionalContext: '{"sha256":"...","md5":"...",...}'}`
// (additionalContext is a JSON-encoded string carrying scanner-specific
// fields); the SPDX-annotation shape is the flat
// `{location, sha256, md5?, ...}`. We lift the additionalContext keys
// to the top level, drop any non-canonical CDX wrapping, and re-
// stringify each occurrence individually — yielding ONE entry per
// occurrence, identical to the SPDX side. Milestone 133 US2.3.
pub(super) fn d2_cdx(doc: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    // Use the main-module-inclusive walker so the CDX
    // `metadata.component` (which carries main-module-tagged components
    // in CDX) is compared against the SPDX 2.3/3 main-module Package
    // (which lives in the regular `packages[]` array). Without this,
    // the language-ecosystem main-module's go.mod / Cargo.lock /
    // pom.xml occurrence is asymmetrically present on the SPDX side
    // only. Milestone 133 US2.3.
    for component in walk_cdx_components_and_main_module(doc) {
        let Some(occurrences) = component
            .get("evidence")
            .and_then(|e| e.get("occurrences"))
            .and_then(|v| v.as_array())
        else {
            continue;
        };
        for occ in occurrences {
            let Some(obj) = occ.as_object() else {
                continue;
            };
            let mut normalized = serde_json::Map::new();
            if let Some(loc) = obj.get("location") {
                normalized.insert("location".to_string(), loc.clone());
            }
            // additionalContext is a JSON-encoded string per CDX 1.6.
            // Decode and merge its keys into the flat shape.
            if let Some(ctx_str) = obj.get("additionalContext").and_then(|v| v.as_str()) {
                if let Ok(serde_json::Value::Object(ctx)) = serde_json::from_str(ctx_str) {
                    for (k, v) in ctx {
                        normalized.insert(k, v);
                    }
                }
            }
            // Sort keys deterministically by re-emitting through a
            // BTreeMap (serde_json::Map preserves insertion order).
            let sorted: std::collections::BTreeMap<String, Value> = normalized.into_iter().collect();
            if let Ok(s) = serde_json::to_string(&sorted) {
                out.insert(s);
            }
        }
    }
    out
}

// ============================================================
// Section E — Compositions (E1)
// ============================================================

// E1 compositions — document-level. CDX has /compositions[] with
// every aggregate (`complete`, `incomplete_first_party_only`,
// etc.); SPDX 2.3 + 3 emit a `compositions` annotation only when
// at least one *complete* ecosystem claim is present (the SPDX
// annotation collapses to `{complete_ecosystems: [...]}`, which
// is empty for incomplete-only scans). For PresenceOnly parity,
// the CDX side reports presence only when CDX has at least one
// `aggregate == "complete"` entry — matching the SPDX semantics
// and avoiding false-positive failures on incomplete-only
// fixtures (e.g., rpm/bdb-only).
pub(super) fn e1_cdx(doc: &Value) -> BTreeSet<String> {
    let Some(comps) = doc.get("compositions").and_then(|v| v.as_array()) else {
        return BTreeSet::new();
    };
    let any_complete = comps
        .iter()
        .any(|c| c.get("aggregate").and_then(|v| v.as_str()) == Some("complete"));
    if any_complete {
        serde_json::to_string(comps).into_iter().collect()
    } else {
        BTreeSet::new()
    }
}

// ============================================================
// Section F — VEX (F1)
// ============================================================

pub(super) fn f1_cdx(doc: &Value) -> BTreeSet<String> {
    doc.get("vulnerabilities")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.get("id")).filter_map(|v| v.as_str()).map(String::from).collect())
        .unwrap_or_default()
}

// ============================================================
// Section G — Document envelope (G1 tool name)
// ============================================================

pub(super) fn g1_cdx(doc: &Value) -> BTreeSet<String> {
    doc.get("metadata")
        .and_then(|m| m.get("tools"))
        .and_then(|t| t.get("components"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c.get("name").and_then(|v| v.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default()
}
