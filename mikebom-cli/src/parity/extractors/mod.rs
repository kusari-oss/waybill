//! Per-catalog-row extractor table (milestone 013 T004–T009).
//!
//! One entry per `CatalogRow` whose Classification has at least
//! one Present format. Each entry carries three extractor
//! closures (CDX, SPDX 2.3, SPDX 3) returning the normalized set
//! of "observable values" for that datum in the format's output,
//! plus a `Directionality` flag (SymmetricEqual vs.
//! CdxSubsetOfSpdx).
//!
//! When a new catalog row lands in `docs/reference/sbom-format-mapping.md`,
//! a corresponding entry MUST be added to [`EXTRACTORS`] —
//! `every_catalog_row_has_an_extractor` (in tests) fires
//! otherwise. Per spec FR-005a / clarification Q2: the catalog
//! is the source of truth; the extractor table is the executable
//! interpretation; mismatches surface at the pre-PR gate.
//!
//! ## Milestone 076 — `subject:` + per-component `--component-id`
//!
//! Both new identifier surfaces ride **standards-native carriers** per
//! Constitution Principle V (native-precedence) and therefore do NOT
//! introduce new catalog rows:
//!
//! - **`subject:` document-level identifier** (built-in scheme, fifth
//!   in the registry): emits via the same CDX
//!   `metadata.component.externalReferences[type:attestation]` carrier
//!   as milestone 073's `attestation:` identifiers, plus SPDX 2.3
//!   main-module `Package.externalRefs[PERSISTENT-ID]` (via existing
//!   milestone-073 logic) + redundant `creationInfo.creators[]` text
//!   line, plus SPDX 3 `SpdxDocument.externalIdentifier[]`. No
//!   `mikebom:*` annotation introduced. The CDX externalReferences
//!   union with milestone 073's `attestation:` IRI emissions is
//!   distinguishable by `url` shape (digest vs IRI).
//!
//! - **Per-component user-defined identifiers** (via
//!   `--component-id <PURL>=<scheme>:<value>`): emit via CDX
//!   `components[].properties[]` (matching mikebom's existing
//!   per-component property emission pattern), SPDX 2.3
//!   `Package.externalRefs[PERSISTENT-ID]`, and SPDX 3
//!   `Element.externalIdentifier[]`. These are NOT envelope-wrapped
//!   under `mikebom:identifiers` (C47); they ride the native
//!   per-component identifier carriers directly. No catalog row
//!   needed — the surfaces are already type-checked at the format-
//!   shape level by each format's existing schema-validation tests.
//!
//! See `specs/076-subject-component-ids/contracts/{subject-identifier,
//! per-component-id}.md` for the per-format wire contracts.

mod cdx;
mod common;
mod spdx2;
mod spdx3;

pub use common::{
    canonicalize_for_compare, extract_mikebom_annotation_values, walk_cdx_components,
    walk_spdx23_packages, walk_spdx3_packages, Directionality, ParityExtractor,
};

use cdx::{
    c10_cdx, c11_cdx, c12_cdx, c13_cdx, c14_cdx, c15_cdx, c16_cdx, c17_cdx, c18_cdx, c19_cdx,
    c1_cdx, c20_cdx, c21_cdx, c22_cdx, c23_cdx, c24_cdx, c25_cdx, c26_cdx, c27_cdx, c28_cdx,
    c29_cdx, c2_cdx, c30_cdx, c31_cdx, c32_cdx, c33_cdx, c34_cdx, c35_cdx, c36_cdx, c37_cdx,
    c38_cdx, c39_cdx, c3_cdx, c40_cdx, c41_cdx, c42_cdx, c44_cdx, c45_cdx, c46_cdx, c47_cdx, c48_cdx, c49_cdx, c4_cdx, c50_cdx, c51_cdx, c52_cdx, c53_cdx, c54_cdx, c55_cdx, c56_cdx, c57_cdx, c58_cdx, c59_cdx, c5_cdx, c60_cdx, c61_cdx, c62_cdx, c7_cdx, c8_cdx, c9_cdx, cdx_containment,
    cdx_binary_role, cdx_cpe, cdx_dev_deps, cdx_distribution, cdx_hashes, cdx_homepage, cdx_licenses_concluded,
    cdx_licenses_declared, cdx_name, cdx_purl, cdx_root, cdx_runtime_deps, cdx_supplier, cdx_vcs,
    cdx_version, d1_cdx, d2_cdx, e1_cdx, f1_cdx, g1_cdx,
};
use common::empty;
use spdx2::{
    c10_spdx23, c11_spdx23, c12_spdx23, c13_spdx23, c14_spdx23, c15_spdx23, c16_spdx23,
    c17_spdx23, c18_spdx23, c19_spdx23, c1_spdx23, c20_spdx23, c21_spdx23, c22_spdx23,
    c23_spdx23, c24_spdx23, c25_spdx23, c26_spdx23, c27_spdx23, c28_spdx23, c29_spdx23, c2_spdx23,
    c30_spdx23, c31_spdx23, c32_spdx23, c33_spdx23, c34_spdx23, c35_spdx23, c36_spdx23, c37_spdx23,
    c38_spdx23, c39_spdx23, c3_spdx23, c40_spdx23, c41_spdx23, c44_spdx23, c45_spdx23, c46_spdx23, c47_spdx23, c48_spdx23, c49_spdx23, c4_spdx23, c50_spdx23, c51_spdx23, c52_spdx23, c53_spdx23, c54_spdx23, c55_spdx23, c56_spdx23, c57_spdx23, c58_spdx23, c59_spdx23, c5_spdx23, c60_spdx23, c61_spdx23, c62_spdx23, c7_spdx23, c8_spdx23,
    c9_spdx23, d1_spdx23, d2_spdx23, e1_spdx23, f1_spdx23, g1_spdx23, spdx23_containment,
    spdx23_binary_role, spdx23_cpe, spdx23_dev_deps, spdx23_distribution, spdx23_hashes, spdx23_homepage,
    spdx23_licenses_concluded, spdx23_licenses_declared, spdx23_name, spdx23_purl, spdx23_root,
    spdx23_runtime_deps, spdx23_supplier, spdx23_vcs, spdx23_version,
};
use spdx3::{
    c10_spdx3, c11_spdx3, c12_spdx3, c13_spdx3, c14_spdx3, c15_spdx3, c16_spdx3, c17_spdx3,
    c18_spdx3, c19_spdx3, c1_spdx3, c20_spdx3, c21_spdx3, c22_spdx3, c23_spdx3, c24_spdx3,
    c25_spdx3, c26_spdx3, c27_spdx3, c28_spdx3, c29_spdx3, c2_spdx3, c30_spdx3, c31_spdx3,
    c32_spdx3, c33_spdx3, c34_spdx3, c35_spdx3, c36_spdx3, c37_spdx3, c38_spdx3, c39_spdx3, c40_spdx3,
    c41_spdx3, c44_spdx3, c45_spdx3, c46_spdx3, c47_spdx3, c48_spdx3, c49_spdx3, c3_spdx3, c4_spdx3, c50_spdx3, c51_spdx3, c52_spdx3, c53_spdx3, c54_spdx3, c55_spdx3, c56_spdx3, c57_spdx3, c58_spdx3, c59_spdx3, c5_spdx3, c60_spdx3, c61_spdx3, c62_spdx3, c7_spdx3, c8_spdx3, c9_spdx3, d1_spdx3, d2_spdx3,
    e1_spdx3, f1_spdx3, g1_spdx3, spdx3_binary_role, spdx3_containment, spdx3_cpe, spdx3_dev_deps,
    spdx3_distribution, spdx3_hashes, spdx3_homepage, spdx3_licenses_concluded,
    spdx3_licenses_declared, spdx3_name, spdx3_purl, spdx3_root, spdx3_runtime_deps,
    spdx3_supplier, spdx3_vcs, spdx3_version,
};

// All extractor functions live in the per-format submodules
// (cdx.rs / spdx2.rs / spdx3.rs) as of milestone 022. mod.rs
// owns only the `EXTRACTORS` table that wires them together,
// the public re-exports (above), and the structural tests
// (below). The G/H rows that don't have format-specific business
// logic route through `common::empty` (was `g_empty` pre-022;
// collapsed since the two sentinels did the same thing).

// ============================================================
// EXTRACTORS table — keyed by row id, sorted alphabetically.
// ============================================================

pub static EXTRACTORS: &[ParityExtractor] = &[
    // Section A — Core identity
    ParityExtractor { row_id: "A1",  label: "PURL",                    cdx: cdx_purl,        spdx23: spdx23_purl,        spdx3: spdx3_purl,        directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "A2",  label: "name",                    cdx: cdx_name,        spdx23: spdx23_name,        spdx3: spdx3_name,        directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "A3",  label: "version",                 cdx: cdx_version,     spdx23: spdx23_version,     spdx3: spdx3_version,     directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "A4",  label: "supplier",                cdx: cdx_supplier,    spdx23: spdx23_supplier,    spdx3: spdx3_supplier,    directional: Directionality::SymmetricEqual, order_sensitive: false },
    // A5 author — format-restricted on all three (mikebom doesn't
    // surface originator yet); empty extractors.
    ParityExtractor { row_id: "A5",  label: "author",                  cdx: empty,           spdx23: empty,              spdx3: empty,             directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "A6",  label: "hashes",                  cdx: cdx_hashes,      spdx23: spdx23_hashes,      spdx3: spdx3_hashes,      directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "A7",  label: "license — declared",      cdx: cdx_licenses_declared,  spdx23: spdx23_licenses_declared,  spdx3: spdx3_licenses_declared,  directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "A8",  label: "license — concluded",     cdx: cdx_licenses_concluded, spdx23: spdx23_licenses_concluded, spdx3: spdx3_licenses_concluded, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "A9",  label: "external ref — homepage", cdx: cdx_homepage,    spdx23: spdx23_homepage,    spdx3: spdx3_homepage,    directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "A10", label: "external ref — VCS",      cdx: cdx_vcs,         spdx23: spdx23_vcs,         spdx3: spdx3_vcs,         directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "A11", label: "external ref — distribution", cdx: cdx_distribution, spdx23: spdx23_distribution, spdx3: spdx3_distribution, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "A12", label: "CPE",                     cdx: cdx_cpe,         spdx23: spdx23_cpe,         spdx3: spdx3_cpe,         directional: Directionality::CdxSubsetOfSpdx, order_sensitive: false },
    // Milestone 104: per-component role (Application / SharedLibrary
    // / Object / Other) emitted into each format's native
    // component-type field. SymmetricEqual: all three formats must
    // agree component-by-component per FR-008.
    ParityExtractor { row_id: "A13", label: "binary-reader role",     cdx: cdx_binary_role, spdx23: spdx23_binary_role, spdx3: spdx3_binary_role, directional: Directionality::SymmetricEqual, order_sensitive: false },
    // Section B — Graph structure
    ParityExtractor { row_id: "B1",  label: "dependency edge (runtime)", cdx: cdx_runtime_deps, spdx23: spdx23_runtime_deps, spdx3: spdx3_runtime_deps, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "B2",  label: "dependency edge (dev)",   cdx: cdx_dev_deps,    spdx23: spdx23_dev_deps,    spdx3: spdx3_dev_deps,    directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "B3",  label: "nested containment",      cdx: cdx_containment, spdx23: spdx23_containment, spdx3: spdx3_containment, directional: Directionality::SymmetricEqual, order_sensitive: false },
    // B4 image/filesystem root: each format encodes the root
    // PURL with format-specific name-sanitization (CDX preserves
    // the raw image tag `mikebom-perf:latest@0.0.0`; SPDX 2.3
    // substitutes `:` → `_` per SPDXID rules; SPDX 3 substitutes
    // `:` → `-` per the stricter Element-name rules). The root
    // concept is the same datum across formats — presence-only
    // enforcement.
    ParityExtractor { row_id: "B4",  label: "image / filesystem root", cdx: cdx_root,        spdx23: spdx23_root,        spdx3: spdx3_root,        directional: Directionality::PresenceOnly, order_sensitive: false },
    // Section C — mikebom-specific annotations
    ParityExtractor { row_id: "C1",  label: "mikebom:source-type",     cdx: c1_cdx,  spdx23: c1_spdx23,  spdx3: c1_spdx3,  directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C2",  label: "mikebom:source-connection-ids", cdx: c2_cdx,  spdx23: c2_spdx23,  spdx3: c2_spdx3,  directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C3",  label: "mikebom:deps-dev-match",  cdx: c3_cdx,  spdx23: c3_spdx23,  spdx3: c3_spdx3,  directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C4",  label: "mikebom:evidence-kind",   cdx: c4_cdx,  spdx23: c4_spdx23,  spdx3: c4_spdx3,  directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C5",  label: "mikebom:sbom-tier",       cdx: c5_cdx,  spdx23: c5_spdx23,  spdx3: c5_spdx3,  directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C7",  label: "mikebom:co-owned-by",     cdx: c7_cdx,  spdx23: c7_spdx23,  spdx3: c7_spdx3,  directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C8",  label: "mikebom:shade-relocation", cdx: c8_cdx, spdx23: c8_spdx23, spdx3: c8_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C9",  label: "mikebom:npm-role",        cdx: c9_cdx,  spdx23: c9_spdx23,  spdx3: c9_spdx3,  directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C10", label: "mikebom:binary-class",    cdx: c10_cdx, spdx23: c10_spdx23, spdx3: c10_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C11", label: "mikebom:binary-stripped", cdx: c11_cdx, spdx23: c11_spdx23, spdx3: c11_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C12", label: "mikebom:linkage-kind",    cdx: c12_cdx, spdx23: c12_spdx23, spdx3: c12_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C13", label: "mikebom:buildinfo-status", cdx: c13_cdx, spdx23: c13_spdx23, spdx3: c13_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C14", label: "mikebom:detected-go",     cdx: c14_cdx, spdx23: c14_spdx23, spdx3: c14_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C15", label: "mikebom:binary-packed",   cdx: c15_cdx, spdx23: c15_spdx23, spdx3: c15_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C16", label: "mikebom:confidence",      cdx: c16_cdx, spdx23: c16_spdx23, spdx3: c16_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C17", label: "mikebom:raw-version",     cdx: c17_cdx, spdx23: c17_spdx23, spdx3: c17_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C18", label: "mikebom:source-files",    cdx: c18_cdx, spdx23: c18_spdx23, spdx3: c18_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    // C19 cpe-candidates: CDX serializes the candidates as a
    // pipe-separated single-property string with single-backslash
    // PURL-escapes (`github.com\/foo`); SPDX serializes as an
    // array-valued envelope with double-backslash escapes
    // (`github.com\\\\/foo` in the wire form). The atomic CPEs
    // are the same datum but the per-format escape conventions
    // differ; presence-only enforcement keeps the parity check
    // honest about the shared emission across formats without
    // tripping on the cosmetic escaping difference.
    ParityExtractor { row_id: "C19", label: "mikebom:cpe-candidates",  cdx: c19_cdx, spdx23: c19_spdx23, spdx3: c19_spdx3, directional: Directionality::PresenceOnly, order_sensitive: false },
    ParityExtractor { row_id: "C20", label: "mikebom:requirement-range", cdx: c20_cdx, spdx23: c20_spdx23, spdx3: c20_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C21", label: "mikebom:generation-context", cdx: c21_cdx, spdx23: c21_spdx23, spdx3: c21_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    // C22: CDX serializes the missing-field set as a comma-joined
    // string property; SPDX serializes as an annotation with a
    // real JSON-array-valued envelope. The atomic atoms differ —
    // CDX cannot losslessly emit a JSON array via a property's
    // `value` (CDX 1.6 properties are stringly-typed). Both carry
    // the same datum; presence-only enforcement reflects the
    // shape gap.
    ParityExtractor { row_id: "C22", label: "mikebom:os-release-missing-fields", cdx: c22_cdx, spdx23: c22_spdx23, spdx3: c22_spdx3, directional: Directionality::PresenceOnly, order_sensitive: false },
    ParityExtractor { row_id: "C23", label: "mikebom:trace-integrity-*", cdx: c23_cdx, spdx23: c23_spdx23, spdx3: c23_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    // Section C continued — milestone 023 ELF identity (CDX/SPDX
    // emitted via the extra_annotations bag in entry.rs::make_file_level_component;
    // catalog rows defined in docs/reference/sbom-format-mapping.md C24-C26).
    ParityExtractor { row_id: "C24", label: "mikebom:elf-build-id",      cdx: c24_cdx, spdx23: c24_spdx23, spdx3: c24_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C25", label: "mikebom:elf-runpath",       cdx: c25_cdx, spdx23: c25_spdx23, spdx3: c25_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C26", label: "mikebom:elf-debuglink",     cdx: c26_cdx, spdx23: c26_spdx23, spdx3: c26_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    // Section C continued — milestone 025 Go VCS metadata (CDX/SPDX
    // emitted via the extra_annotations bag in
    // go_binary.rs::build_vcs_annotations on the main-module entry
    // only; catalog rows in docs/reference/sbom-format-mapping.md C27-C29).
    ParityExtractor { row_id: "C27", label: "mikebom:go-vcs-revision",   cdx: c27_cdx, spdx23: c27_spdx23, spdx3: c27_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C28", label: "mikebom:go-vcs-time",       cdx: c28_cdx, spdx23: c28_spdx23, spdx3: c28_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C29", label: "mikebom:go-vcs-modified",   cdx: c29_cdx, spdx23: c29_spdx23, spdx3: c29_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    // C30-C32 — Mach-O binary identity (milestone 024). Emitted via
    // the extra_annotations bag in
    // binary/entry.rs::build_macho_identity_annotations on the
    // file-level Mach-O component; catalog rows in
    // docs/reference/sbom-format-mapping.md C30-C32).
    ParityExtractor { row_id: "C30", label: "mikebom:macho-uuid",        cdx: c30_cdx, spdx23: c30_spdx23, spdx3: c30_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C31", label: "mikebom:macho-rpath",       cdx: c31_cdx, spdx23: c31_spdx23, spdx3: c31_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C32", label: "mikebom:macho-min-os",      cdx: c32_cdx, spdx23: c32_spdx23, spdx3: c32_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    // C33-C35 — PE binary identity (milestone 028). Emitted via the
    // extra_annotations bag in
    // binary/entry.rs::build_pe_identity_annotations on the file-level
    // PE component; catalog rows in
    // docs/reference/sbom-format-mapping.md C33-C35.
    ParityExtractor { row_id: "C33", label: "mikebom:pe-pdb-id",         cdx: c33_cdx, spdx23: c33_spdx23, spdx3: c33_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C34", label: "mikebom:pe-machine",        cdx: c34_cdx, spdx23: c34_spdx23, spdx3: c34_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C35", label: "mikebom:pe-subsystem",      cdx: c35_cdx, spdx23: c35_spdx23, spdx3: c35_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    // C36 — cargo-auditable cross-link (milestone 029). Emitted via
    // the extra_annotations bag in
    // binary/entry.rs::build_cargo_auditable_cross_link on the
    // file-level Rust binary component (5th amortization-proof
    // consumer of the milestone-023 bag); catalog row in
    // docs/reference/sbom-format-mapping.md C36.
    ParityExtractor { row_id: "C36", label: "mikebom:detected-cargo-auditable", cdx: c36_cdx, spdx23: c36_spdx23, spdx3: c36_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    // C37-C39 — Mach-O codesign metadata (milestone 030). Emitted via
    // the extra_annotations bag in
    // binary/entry.rs::build_macho_identity_annotations on the
    // file-level Mach-O component (6th amortization-proof consumer
    // of the milestone-023 bag); catalog rows in
    // docs/reference/sbom-format-mapping.md C37-C39.
    ParityExtractor { row_id: "C37", label: "mikebom:macho-codesign-identifier", cdx: c37_cdx, spdx23: c37_spdx23, spdx3: c37_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C38", label: "mikebom:macho-codesign-flags",      cdx: c38_cdx, spdx23: c38_spdx23, spdx3: c38_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C39", label: "mikebom:macho-codesign-team-id",    cdx: c39_cdx, spdx23: c39_spdx23, spdx3: c39_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    // C40 — component-role classifier (milestone 048). Filesystem-
    // position-classified role: `build-tool`, `language-runtime`, or
    // (when no heuristic matches) absent. Emitted via the
    // extra_annotations bag in
    // resolve/deduplicator.rs::classify_component_roles after
    // dedup; catalog row in
    // docs/reference/sbom-format-mapping.md C40.
    ParityExtractor { row_id: "C40", label: "mikebom:component-role",            cdx: c40_cdx, spdx23: c40_spdx23, spdx3: c40_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    // C41 — not-linked classifier (milestone 050). Set on Go
    // source-tier components (from go.sum) when a Go binary is also
    // present in the rootfs AND the binary's BuildInfo does NOT
    // confirm the component as linked. Set in
    // package_db::mod::apply_go_linked_filter via the
    // extra_annotations bag, propagated to ResolvedComponent via
    // scan_fs::mod's PackageDbEntry → ResolvedComponent conversion,
    // then surfaced identically across CDX/SPDX 2.3/SPDX 3 by the
    // generic extra_annotations serializer paths. See
    // docs/reference/sbom-format-mapping.md C41.
    ParityExtractor { row_id: "C41", label: "mikebom:not-linked",                cdx: c41_cdx, spdx23: c41_spdx23, spdx3: c41_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    // C42 — `mikebom:lifecycle-scope` (milestone 052/part-2). CDX-
    // only finer-info carve-out per Constitution Principle V (v1.4.0):
    // CDX's native `scope` enum has only 3 values
    // (`required`/`optional`/`excluded`) and cannot express the
    // dev/build/test split. SPDX 2.3 + SPDX 3 carry the same
    // lifecycle signal natively via B2's typed dep-relationships /
    // `lifecycleScope` parameter — checked there, not here.
    // `CdxOnly` directionality skips the SPDX sides for this
    // specific row.
    ParityExtractor { row_id: "C42", label: "mikebom:lifecycle-scope",           cdx: c42_cdx, spdx23: empty,      spdx3: empty,      directional: Directionality::CdxOnly, order_sensitive: false },
    // C44 — doc-level Go graph-completeness (milestone 061, closes
    // #119). SymmetricEqual: identical open-enum value (`complete` /
    // `partial`) + free-text reason MUST appear in CDX
    // `metadata.properties[]`, SPDX 2.3 document-level annotations[],
    // SPDX 3 document-level annotations. Per Constitution Principle X
    // (Transparency): the SBOM signals the limitation when mikebom
    // can't supply every transitive edge.
    ParityExtractor { row_id: "C44", label: "mikebom:graph-completeness",        cdx: c44_cdx, spdx23: c44_spdx23, spdx3: c44_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    // C45 — per-component orphan-reason (milestone 061, closes #119).
    // SymmetricEqual: open-enum string on each orphan component;
    // absent on non-orphans (three-state semantics).
    ParityExtractor { row_id: "C45", label: "mikebom:orphan-reason",             cdx: c45_cdx, spdx23: c45_spdx23, spdx3: c45_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    // C46 — `mikebom:source-document-binding` per-component cross-tier
    // binding (milestone 072 PR-A T008). The JSON-encoded
    // `SourceDocumentBinding` payload (algo, source_doc_id, hash,
    // strength, reason) MUST be byte-identical across CDX
    // `properties[].value` (string-encoded JSON) and SPDX 2.3 / SPDX 3
    // envelope `value` (real JSON object). The milestone-071
    // `canonicalize_atomic_values` helper handles the
    // string-encoded-JSON-vs-real-JSON-object equivalence so
    // SymmetricEqual is the correct directionality. Only emitted on
    // `mikebom:sbom-tier: build` or `deployed` components — source-tier
    // SBOMs do NOT carry this row's signal (per
    // `contracts/source-document-binding-annotation.md` C-1).
    ParityExtractor { row_id: "C46", label: "mikebom:source-document-binding",   cdx: c46_cdx, spdx23: c46_spdx23, spdx3: c46_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    // C47 — `mikebom:identifiers` document-level user-defined
    // identifiers (milestone 073). On CDX + SPDX 2.3 the
    // annotation envelope's `value` is a sorted-by-(scheme,value) JSON
    // array of `{scheme, value, source_label?}` objects (user-defined
    // schemes only — built-in schemes ride standards-native carriers
    // per FR-005 and never appear under this row). On SPDX 3 the
    // user-defined identifiers ride `Element.externalIdentifier[]`
    // natively per `contracts/identifiers-annotation.md` C-1
    // SPDX 3 — `c47_spdx3` reaches into the native carrier and
    // canonicalizes the same `{scheme, value}` payload shape, filtering
    // out the 4 built-in schemes so SymmetricEqual holds across
    // formats.
    ParityExtractor { row_id: "C47", label: "mikebom:identifiers",               cdx: c47_cdx, spdx23: c47_spdx23, spdx3: c47_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    // C48 — `mikebom:resolver-step` per-component go-resolver-step
    // provenance discriminator (milestone 091, closes #174). Three-state
    // semantics: present with value `go-sum-fallback` on Go components
    // reached via step 5 of the resolver ladder; absent on components
    // reached via the higher-fidelity steps 1–3. SymmetricEqual across
    // formats — every Go component carrying the annotation appears in
    // all three carriers; the absent set is the same across formats too.
    ParityExtractor { row_id: "C48", label: "mikebom:resolver-step",             cdx: c48_cdx, spdx23: c48_spdx23, spdx3: c48_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    // Milestone 098 — Build-tier provenance signals (compiler/linker
    // stamps from ELF `.comment`, Mach-O `LC_BUILD_VERSION`, PE
    // `IMAGE_OPTIONAL_HEADER.MajorLinkerVersion`/`MinorLinkerVersion`).
    // No native equivalent in CDX 1.6 / SPDX 2.3 / SPDX 3 (Constitution V
    // audit per milestone-098 research §1); `mikebom:*` namespace
    // justified, mirroring the milestone-023/024/028 identity helpers.
    ParityExtractor { row_id: "C49", label: "mikebom:elf-compiler-stamps",       cdx: c49_cdx, spdx23: c49_spdx23, spdx3: c49_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: true },
    ParityExtractor { row_id: "C50", label: "mikebom:macho-build-version",       cdx: c50_cdx, spdx23: c50_spdx23, spdx3: c50_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C51", label: "mikebom:macho-build-tools",         cdx: c51_cdx, spdx23: c51_spdx23, spdx3: c51_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: true },
    ParityExtractor { row_id: "C52", label: "mikebom:pe-linker-version",         cdx: c52_cdx, spdx23: c52_spdx23, spdx3: c52_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C53", label: "mikebom:download-url",              cdx: c53_cdx, spdx23: c53_spdx23, spdx3: c53_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C54", label: "mikebom:bazel-archive-name",        cdx: c54_cdx, spdx23: c54_spdx23, spdx3: c54_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C55", label: "mikebom:source-mechanism",          cdx: c55_cdx, spdx23: c55_spdx23, spdx3: c55_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C56", label: "mikebom:also-detected-via",         cdx: c56_cdx, spdx23: c56_spdx23, spdx3: c56_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C57", label: "mikebom:build-reference",           cdx: c57_cdx, spdx23: c57_spdx23, spdx3: c57_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C58", label: "mikebom:fingerprint-corpus-sha",    cdx: c58_cdx, spdx23: c58_spdx23, spdx3: c58_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "C59", label: "mikebom:fingerprint-confidence",    cdx: c59_cdx, spdx23: c59_spdx23, spdx3: c59_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    // C60 — `mikebom:build-inclusion` (milestone 112). Open-enum
    // `unknown` / `not-needed` build-participation marker. Parity
    // bridge on the SPDX side (no native excluded-scope field in
    // SPDX 2.3; SPDX 3 `LifecycleScopeType` has no excluded/unknown
    // value). SymmetricEqual — every component carrying the marker
    // appears in all three carriers with the same value.
    ParityExtractor { row_id: "C60", label: "mikebom:build-inclusion",            cdx: c60_cdx, spdx23: c60_spdx23, spdx3: c60_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    // C61 — `mikebom:build-inclusion-derivation` (milestone 112).
    // Provenance discriminator naming the evidence source for a C60
    // `not-needed` verdict (value: `go-mod-why`). Rides the
    // extra_annotations bag → property / annotation in all three
    // formats. SymmetricEqual.
    ParityExtractor { row_id: "C61", label: "mikebom:build-inclusion-derivation", cdx: c61_cdx, spdx23: c61_spdx23, spdx3: c61_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    // C62 — `mikebom:lifecycle-scope-derivation` (test-closure
    // propagation fix + milestone 112). Open-enum
    // `test-only-closure` / `go-mod-why` on graph-derived
    // test-scoped components. Rides the extra_annotations bag in all
    // three formats (the scope itself is native on SPDX 3 but the
    // derivation is not). SymmetricEqual.
    ParityExtractor { row_id: "C62", label: "mikebom:lifecycle-scope-derivation", cdx: c62_cdx, spdx23: c62_spdx23, spdx3: c62_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    // Section D — Evidence
    // D1 evidence shape diverges — CDX `evidence.identity[].{field,
    // confidence, methods[]}` is the full CDX evidence model;
    // SPDX condenses to flat `{technique, confidence}`. The
    // `technique` strings can even differ (CDX names the concrete
    // method; SPDX uses the higher-level evidence type).
    // Presence-only.
    ParityExtractor { row_id: "D1",  label: "evidence — identity",     cdx: d1_cdx, spdx23: d1_spdx23, spdx3: d1_spdx3, directional: Directionality::PresenceOnly, order_sensitive: false },
    ParityExtractor { row_id: "D2",  label: "evidence — occurrences",  cdx: d2_cdx, spdx23: d2_spdx23, spdx3: d2_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    // Section E — Compositions
    // E1 compositions: CDX preserves the full CDX-native
    // compositions[] array verbatim; SPDX uses a condensed
    // `{complete_ecosystems: [...]}` annotation per the catalog
    // doc. The shapes irreconcilably diverge; presence-only.
    ParityExtractor { row_id: "E1",  label: "ecosystem completeness",  cdx: e1_cdx, spdx23: e1_spdx23, spdx3: e1_spdx3, directional: Directionality::PresenceOnly, order_sensitive: false },
    // Section F — VEX
    ParityExtractor { row_id: "F1",  label: "vulnerabilities (VEX)",   cdx: f1_cdx, spdx23: f1_spdx23, spdx3: f1_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    // Section G — Document envelope (mostly format-shape)
    ParityExtractor { row_id: "G1",  label: "tool name + version",     cdx: g1_cdx, spdx23: g1_spdx23, spdx3: g1_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "G2",  label: "created timestamp",       cdx: empty, spdx23: empty, spdx3: empty, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "G3",  label: "data license",            cdx: empty, spdx23: empty, spdx3: empty, directional: Directionality::SymmetricEqual, order_sensitive: false },
    ParityExtractor { row_id: "G4",  label: "document namespace",      cdx: empty, spdx23: empty, spdx3: empty, directional: Directionality::SymmetricEqual, order_sensitive: false },
    // Section H — Structural-difference meta-rows
    ParityExtractor { row_id: "H1",  label: "nested vs. flat",         cdx: empty, spdx23: empty, spdx3: empty, directional: Directionality::SymmetricEqual, order_sensitive: false },
];

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn extractors_table_is_sorted_by_row_id() {
        let mut last: Option<&str> = None;
        for e in EXTRACTORS {
            if let Some(prev) = last {
                assert!(
                    natural_compare(prev, e.row_id),
                    "EXTRACTORS not sorted: {prev} >= {}",
                    e.row_id
                );
            }
            last = Some(e.row_id);
        }
    }

    /// Compare row IDs naturally — A1 < A2 < ... < A12 < B1.
    fn natural_compare(a: &str, b: &str) -> bool {
        let (a_section, a_num) = split_id(a);
        let (b_section, b_num) = split_id(b);
        match a_section.cmp(&b_section) {
            std::cmp::Ordering::Less => true,
            std::cmp::Ordering::Greater => false,
            std::cmp::Ordering::Equal => a_num < b_num,
        }
    }
    fn split_id(id: &str) -> (char, u32) {
        let section = id.chars().next().unwrap();
        let num: u32 = id[1..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse()
            .unwrap_or(0);
        (section, num)
    }

    #[test]
    fn every_catalog_row_has_an_extractor() {
        let mapping_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("docs/reference/sbom-format-mapping.md");
        let rows = super::super::catalog::parse_mapping_doc(&mapping_path);
        let extractor_ids: std::collections::BTreeSet<&str> =
            EXTRACTORS.iter().map(|e| e.row_id).collect();
        let missing: Vec<&str> = rows
            .iter()
            .map(|r| r.id.as_str())
            .filter(|id| !extractor_ids.contains(id))
            .collect();
        assert!(
            missing.is_empty(),
            "catalog rows without extractors: {missing:?}"
        );
        let row_ids: std::collections::BTreeSet<&str> =
            rows.iter().map(|r| r.id.as_str()).collect();
        let orphans: Vec<&str> = EXTRACTORS
            .iter()
            .map(|e| e.row_id)
            .filter(|id| !row_ids.contains(id))
            .collect();
        assert!(
            orphans.is_empty(),
            "EXTRACTORS entries without catalog rows: {orphans:?}"
        );
    }
}
