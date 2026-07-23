//! SPDX 2.3 document envelope + documentNamespace newtype
//! (milestone 010, T019 / T020 / T025).
//!
//! SPDX 2.3 §6.5 requires each document to declare a
//! `documentNamespace` URI that is globally unique for its content —
//! "A unique document identifier in the form of a URI that enables
//! the document to be referenced externally." We derive it
//! deterministically from scan inputs so two runs of the same scan
//! produce the same namespace (FR-020 / SC-007), and two different
//! scans produce different namespaces (so two SBOMs for two
//! different projects never collide).

use data_encoding::BASE32_NOPAD;
use sha2::{Digest, Sha256};

use super::ids::SpdxId;
use super::packages::SpdxPackage;
use super::relationships::SpdxRelationship;
use crate::generate::ScanArtifacts;

/// Length of the base32-encoded hash prefix used in the
/// documentNamespace URI. 32 chars × 5 bits = 160 bits of entropy.
/// Longer than the Package-ID prefix because the namespace is
/// document-global and participates in cross-document cross-references
/// — a collision here would silently merge two unrelated SBOMs.
const NAMESPACE_HASH_PREFIX_LEN: usize = 32;

const NAMESPACE_BASE: &str = "https://waybill.kusari.dev/spdx/";

/// SPDX 2.3 document namespace URI (research.md R8).
///
/// Scheme: `https://waybill.kusari.dev/spdx/<hash>` where `<hash>` is
/// the base32-encoded SHA-256 of:
///   * the scan target description (`ScanArtifacts::target_name`),
///   * the waybill version string,
///   * the sorted set of component PURLs in the scan result.
///
/// Storing the target name + version separately means a scan of the
/// same tree under a different target name (e.g. via CI job renames)
/// produces a distinct namespace — that's desirable: two CI-runs of
/// different names are semantically different documents even if the
/// component set is identical.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(transparent)]
pub struct SpdxDocumentNamespace(String);

impl SpdxDocumentNamespace {
    /// Derive the namespace URI from a scan.
    ///
    /// Inputs folded into the hash are appended in a stable order
    /// (target, version, then PURLs pre-sorted) so the output does
    /// not depend on component-discovery ordering.
    pub fn derive(artifacts: &ScanArtifacts<'_>, mikebom_version: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"target=");
        hasher.update(artifacts.target_name.as_bytes());
        hasher.update(b"\nmikebom=");
        hasher.update(mikebom_version.as_bytes());
        hasher.update(b"\npurls=");
        let mut purls: Vec<&str> =
            artifacts.components.iter().map(|c| c.purl.as_str()).collect();
        purls.sort_unstable();
        for p in purls {
            hasher.update(p.as_bytes());
            hasher.update(b"\n");
        }
        let digest = hasher.finalize();
        let encoded = BASE32_NOPAD.encode(&digest);
        let prefix = &encoded[..NAMESPACE_HASH_PREFIX_LEN];
        SpdxDocumentNamespace(format!("{NAMESPACE_BASE}{prefix}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// SPDX 2.3 annotation type enum (spec §8.6).
///
/// Waybill uses `OTHER` for its namespaced JSON-comment envelopes
/// (FR-016 fallback for `waybill:*` properties). `REVIEW` is reserved
/// for human-curated annotations and is not produced automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "UPPERCASE")]
#[allow(dead_code)]
pub enum SpdxAnnotationType {
    Other,
    Review,
}

/// One SPDX 2.3 annotation. The `comment` field carries the
/// serialized `MikebomAnnotationCommentV1` JSON envelope for
/// waybill-specific data (US2). Empty in US1 — [`SpdxPackage`] and
/// [`SpdxDocument`] both default to an empty annotations list and
/// the US2 phase populates them without touching the envelope shape.
#[derive(Debug, Clone, serde::Serialize)]
#[allow(dead_code)]
pub struct SpdxAnnotation {
    pub annotator: String,
    #[serde(rename = "annotationDate")]
    pub date: String,
    #[serde(rename = "annotationType")]
    pub kind: SpdxAnnotationType,
    pub comment: String,
}

/// SPDX 2.3 external document reference. Populated by the
/// OpenVEX-sidecar co-emission path in
/// [`super::Spdx2_3JsonSerializer::serialize`] per FR-016a — the
/// entry names the sidecar's relative path and a SHA-256 of its
/// bytes so a consumer reading only the SPDX file can locate and
/// integrity-check the sidecar.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SpdxExternalDocumentRef {
    #[serde(rename = "externalDocumentId")]
    pub id: String,
    #[serde(rename = "spdxDocument")]
    pub spdx_document: String,
    pub checksum: super::packages::SpdxChecksum,
}

/// SPDX 2.3 `creationInfo` object (spec §6.8 / §6.9 / §6.13).
#[derive(Debug, Clone, serde::Serialize)]
pub struct CreationInfo {
    /// RFC 3339 UTC timestamp — sourced from `OutputConfig.created`,
    /// never `Utc::now()` (determinism contract, data-model §8).
    pub created: String,
    /// `["Tool: waybill-<version>"]` at minimum. Experimental
    /// formats append a label to the tool creator string so
    /// consumers reading the document can see it's a stub (FR-019b).
    pub creators: Vec<String>,
    #[serde(rename = "licenseListVersion", skip_serializing_if = "Option::is_none")]
    pub license_list_version: Option<String>,
    /// SPDX 2.3 §6.13 free-text `comment` slot. waybill populates it
    /// with a document-level scope hint (scope mode + observed
    /// lifecycle phases + pointer to per-component
    /// `waybill:sbom-tier` annotations) so SPDX consumers reading
    /// only `creationInfo` get parity with CDX consumers reading
    /// `metadata.lifecycles[]`. Milestone 047.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

/// SPDX 2.3 top-level document (spec §6).
///
/// Field ordering follows the spec's table-of-contents order so the
/// emitted JSON matches common reader expectations. Omitted fields
/// use `serde(skip_serializing_if)` rather than `Option<Vec<_>>` to
/// keep the builder API simple.
#[derive(Debug, serde::Serialize)]
pub struct SpdxDocument {
    #[serde(rename = "spdxVersion")]
    pub spdx_version: &'static str,
    #[serde(rename = "dataLicense")]
    pub data_license: &'static str,
    #[serde(rename = "SPDXID")]
    pub spdx_id: SpdxId,
    pub name: String,
    #[serde(rename = "documentNamespace")]
    pub namespace: SpdxDocumentNamespace,
    #[serde(rename = "creationInfo")]
    pub creation_info: CreationInfo,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub packages: Vec<SpdxPackage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<SpdxRelationship>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<SpdxAnnotation>,
    #[serde(
        rename = "externalDocumentRefs",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub external_document_refs: Vec<SpdxExternalDocumentRef>,
    /// Document-level `hasExtractedLicensingInfos[]` array (SPDX 2.3
    /// §10.1) — holds one entry per distinct `LicenseRef-<hash>`
    /// referenced by any Package's `licenseDeclared` /
    /// `licenseConcluded`. Emitted by milestone 012 US3 when any
    /// CycloneDX license expression fails SPDX canonicalization
    /// (per the all-or-nothing rule, clarification Q1).
    /// `skip_serializing_if = "Vec::is_empty"` keeps existing scans
    /// byte-identical — a scan producing only canonicalizable
    /// licenses emits no `hasExtractedLicensingInfos` key at all.
    #[serde(
        rename = "hasExtractedLicensingInfos",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub has_extracted_licensing_infos: Vec<SpdxExtractedLicensingInfo>,
    #[serde(rename = "documentDescribes")]
    pub document_describes: Vec<SpdxId>,
}

/// SPDX 2.3 §10 `hasExtractedLicensingInfos[]` entry. Emitted when
/// the source CycloneDX `licenses[]` carries a term that SPDX's
/// expression grammar can't canonicalize (e.g. `"GNU General Public"`
/// — common free-text strings that lack an SPDX list ID).
///
/// Milestone 012 US3: the `license_id` is a deterministic content-
/// addressed `LicenseRef-<16-char-base32-sha256-prefix>` (derived
/// via `SpdxId::for_license_ref`); `extracted_text` is the raw
/// CycloneDX entries joined by ` AND ` verbatim (lossless); `name`
/// is the fixed literal `"waybill-extracted-license"` (SPDX §10.4
/// requires `name` non-empty but the value is not consumer-
/// significant).
#[derive(Debug, Clone, serde::Serialize)]
pub struct SpdxExtractedLicensingInfo {
    #[serde(rename = "licenseId")]
    pub license_id: String,
    #[serde(rename = "extractedText")]
    pub extracted_text: String,
    pub name: String,
}

/// Milestone 153 (issue #485) — the placeholder text emitted in every
/// sweep-produced `hasExtractedLicensingInfos[]` entry's `extractedText`
/// field.
///
/// Byte-locked per milestone-153 Clarifications Q1 wire contract:
/// downstream consumers may pattern-match on this exact string to
/// distinguish waybill-placeholder entries from entries with real
/// extracted text (e.g., the milestone-012 hash-fallback path's raw
/// preserved expression). Changing this string is a downstream break.
///
/// The `<name>` token inside the string is a LITERAL — waybill does NOT
/// substitute the package name at emission time. Consumers read it as
/// "look for /usr/share/licenses/<the-package-name-in-your-context>/".
///
/// Consumers can identify placeholder entries via:
///
/// ```text
/// jq '.hasExtractedLicensingInfos[]
///     | select(.extractedText
///              | startswith("License text not extracted by waybill."))'
/// ```
// Milestone 154 (closes issue #487): promoted to `pub(crate)` so the
// SPDX 3 emitter at `v3_licenses.rs::sweep_custom_licenses` can import
// this const. Single source of truth for the cross-format placeholder
// wire contract — the SPDX 3 side's `simplelicensing_licenseText`
// field emits this same byte-string, guaranteeing consumers can
// pattern-match on identical text across both SPDX formats.
pub(crate) const PLACEHOLDER_EXTRACTED_TEXT: &str =
    "License text not extracted by waybill. Consult the original package \
     (e.g., /usr/share/licenses/<name>/ on Debian/RPM, or upstream project \
     source) for the full text.";

/// Milestone 153 — compiled regex for extracting SPDX 2.3
/// `LicenseRef-<idstring>` tokens from license-expression strings.
///
/// Pattern: `(?:^|[^:])(LicenseRef-[a-zA-Z0-9.-]+)`
///
/// - Capture group 1 is the LicenseRef- token proper (never includes the
///   preceding character).
/// - The non-capturing prefix `(?:^|[^:])` excludes matches inside
///   `DocumentRef-<doc>:LicenseRef-<id>` compound tokens per SPDX 2.3
///   §10.1 (the LicenseRef- is defined in the referenced OTHER document,
///   not this one; waybill does not emit DocumentRef- forms today but
///   defensive-code future-proofs the sweep against operator-supplied
///   data via supplement-CDX or similar).
/// - The idstring grammar `[a-zA-Z0-9.-]` matches SPDX 2.3 §10.1 (the
///   `-` is at the end of the character class so it's literal, not a
///   range).
///
/// Compiled once via `OnceLock` (standard workspace pattern).
static LICENSE_REF_REGEX: std::sync::OnceLock<regex::Regex> =
    std::sync::OnceLock::new();

fn license_ref_regex() -> &'static regex::Regex {
    LICENSE_REF_REGEX.get_or_init(|| {
        regex::Regex::new(r"(?:^|[^:])(LicenseRef-[a-zA-Z0-9.-]+)")
            .expect("milestone-153 LicenseRef regex must compile")
    })
}

/// Milestone 153 (closes issue #485) — sweep every emitted SPDX 2.3
/// package's license fields for inline `LicenseRef-<idstring>`
/// substrings and emit a matching top-level
/// `hasExtractedLicensingInfos[]` entry per SPDX 2.3 §10.1.
///
/// Milestone 152's `preserve_known_operands_with_license_ref` in
/// `rpm_file.rs` injects `LicenseRef-<sanitized>` INSIDE compound
/// expressions (e.g., `"GPL-2.0-only AND LicenseRef-bzip2-1.0.4"`),
/// bypassing the milestone-012 per-package hash-fallback path at
/// `packages.rs:build_packages`. Without this sweep, the resulting
/// SPDX 2.3 document is §10.1-non-conformant (dangling LicenseRef
/// references).
///
/// # Field coverage
///
/// Sweeps `SpdxPackage::license_declared` and
/// `SpdxPackage::license_concluded` (matching on the `SpdxLicenseField`
/// enum's `Expression` and `LicenseRef` variants; `NoAssertion` and
/// `None` variants contribute nothing).
///
/// **`licenseInfoFromFiles` is NOT swept**: `SpdxPackage` does not carry
/// this field because waybill emits `filesAnalyzed: false` uniformly
/// (spec §7.9.4 makes `licenseInfoFromFiles` inapplicable when files
/// aren't analyzed). If a future milestone starts emitting per-file
/// license info, this sweep MUST be extended.
///
/// # Dedup with milestone-012
///
/// Entries in `existing` (produced by `build_packages`'s hash-fallback
/// path for wholly-non-canonicalizable expressions) WIN over any
/// placeholder entry the sweep would emit for the same `licenseId`.
/// Milestone-012 entries carry the raw expression as their
/// `extractedText`; the milestone-153 sweep entries carry the
/// `PLACEHOLDER_EXTRACTED_TEXT` const. The existing entry with its
/// real text is always more useful.
///
/// # Determinism
///
/// The returned Vec is sorted by `license_id` (lex-ascending) so
/// repeated runs on equal inputs produce byte-identical output. This
/// preserves the SPDX 2.3 golden-test byte-identity contract.
fn sweep_extracted_license_refs(
    packages: &[super::packages::SpdxPackage],
    existing: Vec<SpdxExtractedLicensingInfo>,
) -> Vec<SpdxExtractedLicensingInfo> {
    use std::collections::BTreeMap;

    // Seed the dedup map with milestone-012's entries FIRST so their
    // real extracted text wins over any placeholder emission (FR-005).
    let mut by_id: BTreeMap<String, SpdxExtractedLicensingInfo> = existing
        .into_iter()
        .map(|e| (e.license_id.clone(), e))
        .collect();

    let re = license_ref_regex();

    for pkg in packages {
        for field in [&pkg.license_declared, &pkg.license_concluded] {
            // Extract the string form of the license field. NoAssertion
            // + None variants contribute no LicenseRef- substrings.
            let expr = match field {
                super::packages::SpdxLicenseField::Expression(s)
                | super::packages::SpdxLicenseField::LicenseRef(s) => s.as_str(),
                super::packages::SpdxLicenseField::NoAssertion
                | super::packages::SpdxLicenseField::None => continue,
            };
            for cap in re.captures_iter(expr) {
                // Capture group 1 is the LicenseRef- token proper
                // (DocumentRef-prefixed matches are excluded by the
                // non-capturing prefix in the pattern).
                if let Some(m) = cap.get(1) {
                    let license_id = m.as_str().to_string();
                    if !by_id.contains_key(&license_id) {
                        let name = license_id
                            .strip_prefix("LicenseRef-")
                            .unwrap_or(&license_id)
                            .to_string();
                        by_id.insert(
                            license_id.clone(),
                            SpdxExtractedLicensingInfo {
                                license_id,
                                extracted_text: PLACEHOLDER_EXTRACTED_TEXT
                                    .to_string(),
                                name,
                            },
                        );
                    }
                }
            }
        }
    }

    by_id.into_values().collect()
}

/// Assemble the SPDX 2.3 document envelope from a scan.
///
/// (T025) Picks a deterministic root: if the scan carries exactly
/// one top-level component (no `parent_purl` on that entry, nothing
/// else top-level), that component is the `documentDescribes`
/// target; otherwise a synthetic `SPDXRef-DOCUMENT-ROOT`-style
/// Package is synthesized so consumers always have exactly one
/// described root (spec edge case "Multiple roots / no root").
///
/// The synthetic-root path is exercised by the pip + gem + deb +
/// apk fixtures which each have multiple independent components but
/// no single scan-target coord.
///
/// Milestone 077 — when `artifacts.root_override.is_active()`, the
/// override flow:
///   1. Filters manifest-derived main-module components OUT of the
///      `packages[]` array (clean replacement per Q2 clarification).
///   2. Synthesizes a root Package using the override values for
///      name + version + PURL + CPE (instead of the auto-derived
///      basename + `0.0.0` defaults).
pub fn build_document(
    artifacts: &ScanArtifacts<'_>,
    cfg: &crate::generate::OutputConfig,
) -> SpdxDocument {
    let namespace = SpdxDocumentNamespace::derive(artifacts, cfg.mikebom_version);

    // Single annotator + date pair used across every annotation
    // emitted from this scan: Package-level (from `build_packages`)
    // and Document-level (from `annotate_document`). Both mirror
    // the first `CreationInfo.creators` entry + `created` value so
    // a consumer can see that annotations were produced in the
    // same run as the document.
    let annotator = format!("Tool: waybill-{}", cfg.mikebom_version);
    let date = cfg
        .created
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    // Milestone 077 — when override is active, build a filtered
    // ScanArtifacts view that drops manifest-derived main-modules
    // BEFORE per-package emission. The downstream root-selection
    // logic then falls through to the synthesize_root path with
    // the operator-supplied identity (clean replacement).
    let override_active = artifacts.root_override.is_active();
    // Issue #229: capture the dropped main-module PURLs so the
    // relationship builder can alias them to the synthesized root's
    // SPDXID — otherwise dep edges sourced at those PURLs vanish
    // (their PURL is no longer in the components view, so the
    // resolver silently drops them) and the new root ends up
    // orphaned from the dependency graph.
    // Milestone 149 (issue #151) — drop logic consolidated into
    // `apply_main_module_drop_or_demote` in `root_selector.rs`; runs
    // identically across all three emitters (CDX, SPDX 2.3 here,
    // SPDX 3). When the new `preserve_manifest_main_module` flag is
    // set, the helper takes the demote-as-library branch; the demoted
    // entry's PURL still lands in `dropped_main_module_purls` so the
    // downstream relationship aliasing fires per US1 clarification
    // Option A (recorded 2026-06-29).
    let drop_result = crate::generate::root_selector::apply_main_module_drop_or_demote(
        artifacts.components,
        &artifacts.root_override,
        artifacts.preserve_manifest_main_module,
    );
    let dropped_main_module_purls: Vec<String> = drop_result.redirected_main_module_purls;
    let filtered_components_owned: Option<Vec<waybill_common::resolution::ResolvedComponent>> =
        if override_active {
            tracing::info!(
                name = artifacts.root_override.name.as_deref().unwrap_or(artifacts.target_name),
                version = artifacts.root_override.version.as_deref().unwrap_or("0.0.0"),
                "root component override active (SPDX 2.3): name='{}', version='{}'",
                artifacts.root_override.name.as_deref().unwrap_or(artifacts.target_name),
                artifacts.root_override.version.as_deref().unwrap_or("0.0.0"),
            );
            Some(drop_result.effective_components)
        } else {
            None
        };

    // The package builder needs a borrow of ScanArtifacts pointing
    // at the filtered components when override is active. We construct
    // a local view that mirrors the input but with components swapped.
    let view_artifacts: ScanArtifacts<'_> = if let Some(ref filtered) = filtered_components_owned {
        ScanArtifacts {
            target_name: artifacts.target_name,
            components: filtered.as_slice(),
            relationships: artifacts.relationships,
            integrity: artifacts.integrity,
            complete_ecosystems: artifacts.complete_ecosystems,
            os_release_missing_fields: artifacts.os_release_missing_fields,
            scan_target_coord: artifacts.scan_target_coord,
            generation_context: artifacts.generation_context.clone(),
            include_dev: artifacts.include_dev,
            include_hashes: artifacts.include_hashes,
            include_source_files: artifacts.include_source_files,
            scope_mode: artifacts.scope_mode,
            go_transitive_coverage: artifacts.go_transitive_coverage,
            go_transitive_fallback_count: artifacts.go_transitive_fallback_count,
            go_cache_warming: artifacts.go_cache_warming,
            go_workspace_mode: artifacts.go_workspace_mode,
            go_toolchains_detected: artifacts.go_toolchains_detected,
            helm_extraction_mode: artifacts.helm_extraction_mode,
            image_source: artifacts.image_source,
            source_document_binding: artifacts.source_document_binding,
            identifiers: artifacts.identifiers,
            component_identifiers: artifacts.component_identifiers,
            file_inventory_stats: None,
            file_inventory_mode: None,
            root_override: artifacts.root_override.clone(),
            preserve_manifest_main_module: artifacts.preserve_manifest_main_module,
            user_metadata: artifacts.user_metadata.clone(),
            sbom_type_override: artifacts.sbom_type_override,
            spdx2_relationship_compat: artifacts.spdx2_relationship_compat,
            collisions_summary: artifacts.collisions_summary,
            compiler_pipeline: artifacts.compiler_pipeline,
        }
    } else {
        ScanArtifacts {
            target_name: artifacts.target_name,
            components: artifacts.components,
            relationships: artifacts.relationships,
            integrity: artifacts.integrity,
            complete_ecosystems: artifacts.complete_ecosystems,
            os_release_missing_fields: artifacts.os_release_missing_fields,
            scan_target_coord: artifacts.scan_target_coord,
            generation_context: artifacts.generation_context.clone(),
            include_dev: artifacts.include_dev,
            include_hashes: artifacts.include_hashes,
            include_source_files: artifacts.include_source_files,
            scope_mode: artifacts.scope_mode,
            go_transitive_coverage: artifacts.go_transitive_coverage,
            go_transitive_fallback_count: artifacts.go_transitive_fallback_count,
            go_cache_warming: artifacts.go_cache_warming,
            go_workspace_mode: artifacts.go_workspace_mode,
            go_toolchains_detected: artifacts.go_toolchains_detected,
            helm_extraction_mode: artifacts.helm_extraction_mode,
            image_source: artifacts.image_source,
            source_document_binding: artifacts.source_document_binding,
            identifiers: artifacts.identifiers,
            component_identifiers: artifacts.component_identifiers,
            file_inventory_stats: None,
            file_inventory_mode: None,
            root_override: artifacts.root_override.clone(),
            preserve_manifest_main_module: artifacts.preserve_manifest_main_module,
            user_metadata: artifacts.user_metadata.clone(),
            sbom_type_override: artifacts.sbom_type_override,
            spdx2_relationship_compat: artifacts.spdx2_relationship_compat,
            collisions_summary: artifacts.collisions_summary,
            compiler_pipeline: artifacts.compiler_pipeline,
        }
    };
    let artifacts: &ScanArtifacts<'_> = &view_artifacts;

    let (packages, has_extracted_licensing_infos) =
        super::packages::build_packages(artifacts, &annotator, &date);

    // Milestone 153 (closes issue #485): sweep the assembled packages
    // for inline `LicenseRef-*` substrings (from milestone-152's escape-
    // hatch path in `rpm_file.rs`) and merge with the milestone-012 hash-
    // fallback entries returned above. Existing entries with real
    // extracted text win over the sweep's placeholder entries.
    let has_extracted_licensing_infos =
        sweep_extracted_license_refs(&packages, has_extracted_licensing_infos);

    // Root selection: deterministic single-root algorithm.
    //   0. Milestone 053 FR-008 + US3: if exactly one top-level
    //      component carries `waybill:component-role: main-module`,
    //      use it as the document root (the Go workspace's main-
    //      module is the BOM subject by design). Multiple main-
    //      modules (go.work monorepo) → synthesize a super-root that
    //      DESCRIBES each one (case 3 fall-through with synthesis).
    //   1. If a top-level component (no parent_purl) carries a PURL
    //      whose name matches `artifacts.target_name`, use that.
    //   2. Else if exactly one top-level component exists, use it.
    //   3. Else synthesize a root package and prepend it.
    let top_level: Vec<usize> = artifacts
        .components
        .iter()
        .enumerate()
        .filter(|(_, c)| c.parent_purl.is_none())
        .map(|(i, _)| i)
        .collect();

    // Milestone 127 — delegate BOM-subject selection to the central
    // `generate::root_selector::select_root` ladder. The selector
    // handles override > count==1 fast path > FR-002 repo-root >
    // FR-003 ecosystem-priority > FR-004 LCP > Maven coord >
    // synthetic placeholder, all in one call. When the result names
    // a `MainModule` (the count==1 fast path OR a count>1 tiebreaker
    // picked one), emit a single `documentDescribes` ID for it. When
    // the result names an `OperatorOverride`, synthesize a root using
    // the milestone-077 override values. Otherwise (Maven coord OR
    // synthetic placeholder), fall back to the existing top-level
    // selection (a top-level component whose name matches the scan
    // target, else synthesize).
    let selection = crate::generate::root_selector::select_root(
        artifacts.components,
        &artifacts.root_override,
        artifacts.scan_target_coord,
        artifacts.target_name,
        "0.0.0",
    );
    let (root_ids, synthetic_root) = match &selection.subject {
        crate::generate::root_selector::ResolvedRootSubject::OperatorOverride => {
            let (id, root) = synthesize_root_with_override(
                artifacts.target_name,
                &namespace,
                artifacts.root_override.name.as_deref(),
                artifacts.root_override.version.as_deref(),
                &artifacts.root_override,
            );
            (vec![id], Some(root))
        }
        crate::generate::root_selector::ResolvedRootSubject::MainModule(idx) => {
            let purl = &artifacts.components[*idx].purl;
            (vec![SpdxId::for_purl(purl)], None)
        }
        _ => match top_level.len() {
            0 => {
                let (id, root) = synthesize_root(artifacts.target_name, &namespace);
                (vec![id], Some(root))
            }
            1 => {
                let idx = top_level[0];
                let purl = &artifacts.components[idx].purl;
                (vec![SpdxId::for_purl(purl)], None)
            }
            _ => {
                if let Some(idx) = top_level
                    .iter()
                    .find(|&&i| artifacts.components[i].name == artifacts.target_name)
                {
                    let purl = &artifacts.components[*idx].purl;
                    (vec![SpdxId::for_purl(purl)], None)
                } else {
                    let (id, root) = synthesize_root(artifacts.target_name, &namespace);
                    (vec![id], Some(root))
                }
            }
        },
    };

    // Prepend the synthetic-root package (if any) so it precedes
    // every component-derived package in the output.
    let mut packages = packages;
    let synthetic_root_added = synthetic_root.is_some();
    if let Some(root_pkg) = synthetic_root {
        packages.insert(0, root_pkg);
    }

    // Issue #229: when override is active, alias every dropped
    // main-module PURL to the synthesized root's SPDXID so dep
    // edges originally sourced at those PURLs are rewritten to
    // source from the new root. In the non-override path the alias
    // list is empty and behavior is unchanged.
    let purl_aliases: Vec<(String, SpdxId)> =
        match (override_active, root_ids.first()) {
            (true, Some(root_id)) => dropped_main_module_purls
                .iter()
                .map(|p| (p.clone(), root_id.clone()))
                .collect(),
            _ => Vec::new(),
        };
    let mut relationships =
        super::relationships::build_relationships(artifacts, &root_ids, &purl_aliases);

    // Milestone 158 US1 — append workspace-peer synthetic edges so
    // SPDX 2.3 emits DEPENDS_ON relationships from the primary root
    // to each detected workspace peer (loser). Byte-identical to the
    // CDX side's peer-linkage. Reuses milestone-127's `losers` list.
    let m158_workspace_peer_edges =
        crate::generate::graph_completeness::build_workspace_peer_edges(
            &selection,
            artifacts.components,
        );
    if !m158_workspace_peer_edges.is_empty() {
        if let crate::generate::root_selector::ResolvedRootSubject::MainModule(idx) = &selection.subject {
            if let Some(root_c) = artifacts.components.get(*idx) {
                let root_id = super::ids::SpdxId::for_purl(&root_c.purl);
                for loser_purl in &selection.losers {
                    relationships.push(super::relationships::SpdxRelationship {
                        source: root_id.clone(),
                        target: super::ids::SpdxId::for_purl(loser_purl),
                        kind: super::relationships::SpdxRelationshipType::DependsOn,
                        comment: None,
                    });
                }
            }
        }
    }

    // Milestone 158 US2 — compute the multi-root BFS reachability
    // pass on the AUGMENTED graph (post workspace-peer linkage) and
    // pass into `annotate_document` for the two document-scope
    // annotations. Independently computed from the CDX side because
    // each format has its own emission flow — the result is
    // deterministic (same select_root ladder, same seed set, same
    // edges) so both emissions agree byte-equally on the value.
    // Milestone 158 — pass the emitted SPDX root's PURL (or empty when
    // the synthetic-root fallback fired without a Package ref) as the
    // target_ref so BFS mirrors the emitter's primary-dep-fallback.
    let m158_target_ref: String = match &selection.subject {
        crate::generate::root_selector::ResolvedRootSubject::MainModule(idx) => artifacts
            .components
            .get(*idx)
            .map(|c| c.purl.as_str().to_string())
            .unwrap_or_default(),
        crate::generate::root_selector::ResolvedRootSubject::MavenCoord => artifacts
            .scan_target_coord
            .map(|c| format!("pkg:maven/{}/{}@{}", c.group, c.artifact, c.version))
            .unwrap_or_default(),
        crate::generate::root_selector::ResolvedRootSubject::SyntheticPlaceholder { name, version } => {
            format!("pkg:generic/{name}@{version}")
        }
        crate::generate::root_selector::ResolvedRootSubject::OperatorOverride => {
            artifacts.target_name.to_string()
        }
    };
    // Milestone 194 US4 — apply the m192 pre-rewrite ONLY for the
    // classifier input (mirroring CDX's separation: emit uses
    // `effective_relationships`; classifier uses the pre-rewritten
    // set). Without this isolation, downstream SPDX 2.3 emit
    // machinery double-rewrites via the existing dropped-mainmod
    // alias in `spdx/relationships.rs` and produces empty
    // `.dependsOn` on the synthesized root.
    let m194_classifier_relationships: Vec<waybill_common::resolution::Relationship> = {
        let prerewritten = crate::generate::graph_completeness::rewrite_dropped_mainmod_edges(
            artifacts.relationships,
            &dropped_main_module_purls,
            &m158_target_ref,
        );
        prerewritten
            .into_iter()
            .chain(m158_workspace_peer_edges.iter().cloned())
            .collect()
    };
    let m158_graph_completeness =
        crate::generate::graph_completeness::compute_graph_completeness(
            artifacts.components,
            &m194_classifier_relationships,
            &selection,
            &m158_target_ref,
        );

    // Issue #236: when `synthesize_root` fires (multi-top-level
    // scans with no main-module and no name match — the dominant
    // case for image scans and OS-package scans), the synthetic
    // root has no outgoing edges in `artifacts.relationships`. CDX
    // covers this with the primary-dependency fallback in
    // `cyclonedx/dependencies.rs:74-99` (synthesize edges from
    // `metadata.component.bom-ref` to every graph-root component).
    // We mirror that here so SPDX 2.3 emits a connected graph
    // rooted at the same synthetic identity. Without this, the
    // synthetic root is orphaned in `relationships[]` — only the
    // `DESCRIBES` edge from `SPDXRef-DOCUMENT` reaches it and the
    // 31 (in the postgres:16 case) top-level Packages have no
    // incoming edges, producing N disconnected graph-tops where
    // CDX has a single root.
    //
    // Post-#236 gating rule (added after the alpha.35 regen
    // surfaced over-attachment under `--root-name`): only fire
    // the fallback when synth_id has no outgoing edges in the
    // already-built `relationships` vec. This mirrors CDX's
    // `target_has_no_edges` gate at
    // `cyclonedx/dependencies.rs:74-78` symmetrically. When
    // `--root-name` is active, the milestone-#229 alias rewrite
    // at lines 458-465 has already populated outgoing edges from
    // synth_id for every relationship that was originally sourced
    // at the dropped main-module's PURL. Firing the fallback
    // on top of those would over-attach graph-root components
    // (Go `// indirect` entries, orphan npm packages from
    // secondary `node_modules/` trees, etc.) as direct deps of
    // the override root — a divergence vs CDX which the alpha.35
    // bug reports caught. The fallback still fires correctly for
    // image scans, OS-package-only scans, and any other case
    // where `artifacts.relationships` has no main-module-sourced
    // edges to rewrite (synth_id stays with zero outgoing edges
    // after `build_relationships`, so the gate is satisfied).
    if synthetic_root_added {
        if let Some(synth_id) = root_ids.first() {
            let synth_has_outgoing = relationships
                .iter()
                .any(|r| r.source == *synth_id);
            if !synth_has_outgoing {
                // Mirror CDX's "graph roots" definition: components no
                // other component or relationship points to as a `to`
                // target. For a flat OS-package scan that's every
                // component; for a transitive scan it's just the top-
                // level deps.
                let depended_on: std::collections::BTreeSet<&str> = artifacts
                    .relationships
                    .iter()
                    .map(|r| r.to.as_str())
                    .collect();
                let mut graph_roots: Vec<&waybill_common::resolution::ResolvedComponent> =
                    artifacts
                        .components
                        .iter()
                        .filter(|c| {
                            c.parent_purl.is_none()
                                && !depended_on.contains(c.purl.as_str())
                        })
                        .collect();
                // Deterministic emission order: lex by PURL.
                graph_roots.sort_by(|a, b| a.purl.as_str().cmp(b.purl.as_str()));
                for c in graph_roots {
                    relationships.push(super::relationships::SpdxRelationship {
                        source: synth_id.clone(),
                        target: SpdxId::for_purl(&c.purl),
                        kind: super::relationships::SpdxRelationshipType::DependsOn,
                        comment: None,
                    });
                }
            }
        }
    }

    // Two creator entries: a `Tool:` identifying waybill (used
    // throughout the document as the `annotator` field on every
    // annotation we emit), plus an `Organization:` identifying the
    // waybill project as the SBOM's sbomqs-facing author.
    // sbomqs's `sbom_authors` feature checks for a non-Tool creator
    // — giving it an Organization entry mirrors what CDX emits in
    // `metadata.supplier` + `metadata.authors` and closes the
    // cross-format sbomqs Provenance gap.
    //
    // Milestone 073 — per Q2 clarification, redundant
    // `Tool: waybill-<version> source: <full-identifier>` text lines
    // are appended for each built-in identifier. This is the
    // free-form fallback for SPDX 2.3 consumers that don't decode
    // the typed `Package.externalRefs[PERSISTENT-ID]` rows on the
    // main-module Package. Order: auto-detected first, then manual
    // in supply order (per FR-009 / VR-008). Built-in identifiers
    // only — user-defined identifiers ride the document-level
    // `waybill:identifiers` annotation per Constitution
    // Principle V.
    let mut creators = vec![
        annotator.clone(),
        "Organization: waybill contributors".to_string(),
    ];
    for id in artifacts.identifiers {
        if id.is_builtin() {
            creators.push(format!(
                "{annotator} source: {wire}",
                annotator = annotator,
                wire = id.as_wire()
            ));
        }
    }
    // Milestone 080 — append user-supplied `--creator <Type: Name>`
    // entries verbatim per the SPDX 2.3 routing matrix (research §2).
    // Insertion order is file-then-flag (already enforced upstream by
    // `merge_file_and_flags`).
    for creator in &artifacts.user_metadata.creators {
        creators.push(format!("{} {}", creator.kind.spdx_prefix(), creator.name));
    }
    let creation_info = CreationInfo {
        created: date.clone(),
        creators,
        license_list_version: None,
        // Milestone 080 — user-supplied `--metadata-comment` wins as
        // the slot's primary value. The pre-080 scope-hint comment is
        // appended as a 2nd line so SPDX 2.3 readers retain the
        // milestone-047 scope diagnostic when an operator supplies a
        // comment. When no user comment is supplied, the scope-hint
        // value is the slot's value (alpha.20 byte-identity).
        comment: Some(match artifacts.user_metadata.metadata_comment.as_deref() {
            Some(user_text) => {
                format!("{}\n\n{}", user_text, build_scope_comment(artifacts))
            }
            None => build_scope_comment(artifacts),
        }),
    };

    // Document-level waybill annotations (Sections C21–C23 + E1).
    let mut annotations =
        super::annotations::annotate_document(&annotator, &date, artifacts, &m158_graph_completeness);
    // Milestone 080 — append user-supplied `--annotator` /
    // `--annotation-comment` pairs per the SPDX 2.3 routing matrix
    // (contracts/user-sbom-metadata.md). Each pair → SpdxAnnotation
    // with shape `{annotator: "<Type>: <Name>", annotationDate, type:
    // OTHER, comment}`.
    for ann in &artifacts.user_metadata.annotations {
        annotations.push(SpdxAnnotation {
            annotator: format!(
                "{} {}",
                ann.annotator.kind.spdx_prefix(),
                ann.annotator.name
            ),
            date: date.clone(),
            kind: SpdxAnnotationType::Other,
            comment: ann.comment.clone(),
        });
    }

    // Milestone 080 — `--scan-target-name` overrides the SPDX 2.3
    // top-level document `name` field (independent of milestone 077's
    // `--root-name` which targets the root Package's name; per
    // research §5 both flags are honored independently in SPDX 2.3).
    let document_name = artifacts
        .user_metadata
        .scan_target_name
        .clone()
        .unwrap_or_else(|| artifacts.target_name.to_string());

    SpdxDocument {
        spdx_version: "SPDX-2.3",
        data_license: "CC0-1.0",
        spdx_id: SpdxId::document(),
        name: document_name,
        namespace,
        creation_info,
        packages,
        relationships,
        annotations,
        external_document_refs: Vec::new(),
        has_extracted_licensing_infos,
        document_describes: root_ids,
    }
}

/// Build the document-level scope-hint string for SPDX 2.3
/// `creationInfo.comment` and SPDX 3 `SpdxDocument.comment`
/// (milestone 047). Names the scope mode (artifact vs manifest),
/// the observed CDX-style lifecycle phases (sorted
/// lexicographically via the `lifecycle_phases::aggregate_phases`
/// helper), and a pointer to the per-component
/// `waybill:sbom-tier` annotation for finer-grained scope detail.
///
/// Always returns a string. When no component carries a tier
/// (atypical), the phases-list line degrades to "no lifecycle
/// phases observed" rather than omitting the whole comment, so
/// downstream consumers can rely on the field being present.
pub(super) fn build_scope_comment(scan: &ScanArtifacts<'_>) -> String {
    use crate::generate::ScopeMode;

    let mode = match scan.scope_mode {
        ScopeMode::Artifact => "artifact (on-disk components only)",
        ScopeMode::Manifest => "manifest (declared transitives included)",
    };
    // Milestone 081: thread the operator-asserted `--sbom-type`
    // override through to the comment aggregator so SPDX 2.3 +
    // SPDX 3 `comment` strings reflect the override single-element
    // when the flag is set, identical to CDX `metadata.lifecycles[]`.
    let phases = crate::generate::lifecycle_phases::aggregate_phases(
        scan.components,
        scan.sbom_type_override,
    );
    let phases_text = if phases.is_empty() {
        "no lifecycle phases observed".to_string()
    } else {
        phases.join(", ")
    };
    format!(
        "Scope: {mode}. Observed lifecycle phases: {phases_text}. \
         Per-component scope detail in waybill:sbom-tier annotations."
    )
}

/// Deterministically derive a synthetic-root SPDXID and a
/// placeholder Package for it. Used when the scan has no natural
/// single root (multi-project trees, image scans, empty scans).
fn synthesize_root(
    target_name: &str,
    namespace: &SpdxDocumentNamespace,
) -> (SpdxId, SpdxPackage) {
    use super::packages::{
        SpdxExternalRef, SpdxExternalRefCategory, SpdxLicenseField,
    };

    // Stable SPDXID for the synthetic root: hash the namespace URI
    // (already scan-derived + waybill-version-stamped) plus a fixed
    // salt so it cannot collide with a PURL-derived package ID.
    let mut hasher = Sha256::new();
    hasher.update(b"synthetic-root\n");
    hasher.update(namespace.as_str().as_bytes());
    let digest = hasher.finalize();
    let encoded = BASE32_NOPAD.encode(&digest);
    let id = SpdxId::synthetic_root(&encoded[..16]);

    // Synthesize identity externalRefs for the synthetic root so
    // sbomqs's Vulnerability/comp_with_purl + comp_with_cpe features
    // don't ding every waybill SPDX document for "one component is
    // missing PURL/CPE" (the synthetic root is the one component).
    // The PURL uses `pkg:generic/<target>@0.0.0` — the same shape
    // CDX uses for the scan-subject metadata.component. The CPE
    // mirrors `metadata.component.cpe` in CDX. Both are synthetic
    // but spec-valid; consumers that want a real PURL/CPE look at
    // the component-level Packages, not the root.
    //
    // Issue #236: PURL and CPE have different escape rules, so they
    // are sanitized separately. The PURL uses
    // `encode_purl_segment` (the same helper CDX uses for its
    // `metadata.component.purl`), which preserves colon literals
    // (so `postgres:16` → `postgres:16`, matching CDX). Pre-fix this
    // path used `sanitize_for_coord` for both, producing
    // `postgres_16` for the SPDX PURL — a per-format root-identity
    // divergence the reporter flagged alongside the missing-edges
    // bug. The CPE keeps `sanitize_for_coord` because the
    // CPE 2.3 grammar uses `_` as the conventional component
    // separator-safe filler.
    let version = "0.0.0";
    let purl_name = waybill_common::types::purl::encode_purl_segment(target_name);
    let synth_purl = format!("pkg:generic/{purl_name}@{version}");
    let cpe_name = sanitize_for_coord(target_name);
    let synth_cpe =
        format!("cpe:2.3:a:waybill:{cpe_name}:{version}:*:*:*:*:*:*:*");

    let root = SpdxPackage {
        spdx_id: id.clone(),
        name: target_name.to_string(),
        version_info: version.to_string(),
        download_location: "NOASSERTION".to_string(),
        supplier: Some("Organization: waybill contributors".to_string()),
        originator: None,
        files_analyzed: false,
        checksums: Vec::new(),
        license_declared: SpdxLicenseField::NoAssertion,
        license_concluded: SpdxLicenseField::NoAssertion,
        copyright_text: None,
        external_refs: vec![
            SpdxExternalRef {
                category: SpdxExternalRefCategory::PackageManager,
                ref_type: "purl".to_string(),
                locator: synth_purl,
                comment: None,
            },
            SpdxExternalRef {
                category: SpdxExternalRefCategory::Security,
                ref_type: "cpe23Type".to_string(),
                locator: synth_cpe,
                comment: None,
            },
        ],
        annotations: Vec::new(),
        primary_package_purpose: None,
    };
    (id, root)
}

/// Milestone 077 — synthesize a root Package using operator-supplied
/// override values for name and/or version. Mirrors `synthesize_root`
/// but uses the new RFC 3986 percent-encoder for the PURL `name`
/// segment so npm-scoped names like `@acme/widget-svc` round-trip
/// correctly through the PURL field.
fn synthesize_root_with_override(
    target_name: &str,
    namespace: &SpdxDocumentNamespace,
    override_name: Option<&str>,
    override_version: Option<&str>,
    root_override: &crate::generate::RootComponentOverride,
) -> (SpdxId, super::packages::SpdxPackage) {
    use super::packages::{
        SpdxExternalRef, SpdxExternalRefCategory, SpdxLicenseField, SpdxPackage,
    };

    let name = override_name.unwrap_or(target_name);
    let version = override_version.unwrap_or("0.0.0");

    // Stable SPDXID — hash the namespace URI + the override values so
    // re-runs with the same override produce the same SPDXID
    // (determinism per FR-010 / VR-077-004). Distinct from the
    // non-override `synthesize_root` SPDXID prefix because the input
    // bytes differ.
    let mut hasher = Sha256::new();
    hasher.update(b"synthetic-root-077\n");
    hasher.update(namespace.as_str().as_bytes());
    hasher.update(b"\nname=");
    hasher.update(name.as_bytes());
    hasher.update(b"\nversion=");
    hasher.update(version.as_bytes());
    let digest = hasher.finalize();
    let encoded = BASE32_NOPAD.encode(&digest);
    let id = SpdxId::synthetic_root(&encoded[..16]);

    // PURL — uses `build_subject_purl` so `--root-purl-type` selects
    // the type segment and `--no-root-purl` returns `None`, in which
    // case we DO NOT emit the `purl` externalRef.
    let purl_opt = root_override.build_subject_purl(name, version);

    // CPE uses `cpe_escape`-style sanitization for both segments; reuse
    // the existing sanitize_for_coord helper which matches the CDX
    // path's behavior for the override case.
    let cpe_name = sanitize_for_coord(name);
    let cpe_version = sanitize_for_coord(version);
    let synth_cpe =
        format!("cpe:2.3:a:waybill:{cpe_name}:{cpe_version}:*:*:*:*:*:*:*");

    let mut external_refs: Vec<SpdxExternalRef> = Vec::with_capacity(2);
    if let Some(synth_purl) = purl_opt {
        external_refs.push(SpdxExternalRef {
            category: SpdxExternalRefCategory::PackageManager,
            ref_type: "purl".to_string(),
            locator: synth_purl,
            comment: None,
        });
    }
    external_refs.push(SpdxExternalRef {
        category: SpdxExternalRefCategory::Security,
        ref_type: "cpe23Type".to_string(),
        locator: synth_cpe,
        comment: None,
    });

    let root = SpdxPackage {
        spdx_id: id.clone(),
        name: name.to_string(),
        version_info: version.to_string(),
        download_location: "NOASSERTION".to_string(),
        supplier: Some("Organization: waybill contributors".to_string()),
        originator: None,
        files_analyzed: false,
        checksums: Vec::new(),
        license_declared: SpdxLicenseField::NoAssertion,
        license_concluded: SpdxLicenseField::NoAssertion,
        copyright_text: None,
        external_refs,
        annotations: Vec::new(),
        primary_package_purpose: None,
    };
    (id, root)
}

/// Normalize a target-name string for inclusion in a PURL/CPE
/// coord. Matches the loose shape CDX uses for its synthesized
/// scan-subject PURL (see `metadata.rs::cpe_sanitize`): lowercase
/// ASCII alphanumerics + `_` / `-` / `.` preserved; everything
/// else collapses to `_`.
fn sanitize_for_coord(raw: &str) -> String {
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

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use waybill_common::attestation::integrity::TraceIntegrity;
    use waybill_common::attestation::metadata::GenerationContext;
    use waybill_common::resolution::{
        ResolutionEvidence, ResolutionTechnique, ResolvedComponent,
    };
    use waybill_common::types::purl::Purl;

    fn empty_integrity() -> TraceIntegrity {
        TraceIntegrity {
            ring_buffer_overflows: 0,
            events_dropped: 0,
            uprobe_attach_failures: vec![],
            kprobe_attach_failures: vec![],
            partial_captures: vec![],
            bloom_filter_capacity: 0,
            bloom_filter_false_positive_rate: 0.0,
            filter_categories_applied: vec![],
        }
    }

    fn mk_component(purl: &str, name: &str, version: &str) -> ResolvedComponent {
        ResolvedComponent {
            build_inclusion: None,
            purl: Purl::new(purl).unwrap(),
            name: name.to_string(),
            version: version.to_string(),
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
            lifecycle_scope: None,
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

    fn mk_artifacts<'a>(
        target_name: &'a str,
        components: &'a [ResolvedComponent],
        relationships: &'a [waybill_common::resolution::Relationship],
        integrity: &'a TraceIntegrity,
    ) -> ScanArtifacts<'a> {
        ScanArtifacts {
            target_name,
            components,
            relationships,
            integrity,
            complete_ecosystems: &[],
            os_release_missing_fields: &[],
            go_transitive_coverage: None,
            go_transitive_fallback_count: None,
            go_cache_warming: None,
            go_workspace_mode: None,
            go_toolchains_detected: None,
            helm_extraction_mode: None,
            image_source: None,
            scan_target_coord: None,
            generation_context: GenerationContext::FilesystemScan,
            include_dev: false,
            include_hashes: true,
            include_source_files: false,
            scope_mode: crate::generate::ScopeMode::Artifact,
            source_document_binding: None,
            identifiers: &[],
            component_identifiers: &[],
            file_inventory_stats: None,
            file_inventory_mode: None,
            root_override: crate::generate::RootComponentOverride::default(),
            preserve_manifest_main_module: false,
            user_metadata: waybill::binding::user_metadata::UserMetadata::default(),
            sbom_type_override: None,
            spdx2_relationship_compat: crate::generate::Spdx2RelationshipCompat::Full,
            collisions_summary: None,
            compiler_pipeline: None,
        }
    }

    #[test]
    fn namespace_is_deterministic_for_identical_inputs() {
        let components = vec![mk_component("pkg:cargo/a@1", "a", "1")];
        let integ = empty_integrity();
        let a = SpdxDocumentNamespace::derive(
            &mk_artifacts("demo", &components, &[], &integ),
            "0.1.0",
        );
        let b = SpdxDocumentNamespace::derive(
            &mk_artifacts("demo", &components, &[], &integ),
            "0.1.0",
        );
        assert_eq!(a, b);
    }

    #[test]
    fn namespace_differs_for_different_components() {
        let integ = empty_integrity();
        let c1 = vec![mk_component("pkg:cargo/a@1", "a", "1")];
        let c2 = vec![mk_component("pkg:cargo/b@1", "b", "1")];
        let a = SpdxDocumentNamespace::derive(
            &mk_artifacts("demo", &c1, &[], &integ),
            "0.1.0",
        );
        let b = SpdxDocumentNamespace::derive(
            &mk_artifacts("demo", &c2, &[], &integ),
            "0.1.0",
        );
        assert_ne!(a, b);
    }

    #[test]
    fn namespace_differs_for_different_target_name() {
        let integ = empty_integrity();
        let c = vec![mk_component("pkg:cargo/a@1", "a", "1")];
        let a = SpdxDocumentNamespace::derive(
            &mk_artifacts("project-a", &c, &[], &integ),
            "0.1.0",
        );
        let b = SpdxDocumentNamespace::derive(
            &mk_artifacts("project-b", &c, &[], &integ),
            "0.1.0",
        );
        assert_ne!(a, b);
    }

    #[test]
    fn namespace_differs_for_different_mikebom_version() {
        let integ = empty_integrity();
        let c = vec![mk_component("pkg:cargo/a@1", "a", "1")];
        let a = SpdxDocumentNamespace::derive(
            &mk_artifacts("demo", &c, &[], &integ),
            "0.1.0",
        );
        let b = SpdxDocumentNamespace::derive(
            &mk_artifacts("demo", &c, &[], &integ),
            "0.2.0",
        );
        assert_ne!(a, b);
    }

    #[test]
    fn namespace_starts_with_mikebom_base_uri() {
        let integ = empty_integrity();
        let c = vec![mk_component("pkg:cargo/a@1", "a", "1")];
        let ns = SpdxDocumentNamespace::derive(
            &mk_artifacts("demo", &c, &[], &integ),
            "0.1.0",
        );
        assert!(
            ns.as_str().starts_with(NAMESPACE_BASE),
            "namespace {} should start with {NAMESPACE_BASE}",
            ns.as_str()
        );
    }

    /// Test helper: build a component with a specific `sbom_tier`
    /// for the `build_scope_comment` tests below.
    fn mk_component_with_tier(
        purl: &str,
        tier: Option<&str>,
    ) -> ResolvedComponent {
        let mut c = mk_component(purl, "x", "1");
        c.sbom_tier = tier.map(|s| s.to_string());
        c
    }

    #[test]
    fn build_scope_comment_emits_artifact_mode_with_phases() {
        let integ = empty_integrity();
        let comps = vec![
            mk_component_with_tier("pkg:cargo/a@1", Some("build")),
            mk_component_with_tier("pkg:cargo/b@1", Some("deployed")),
            mk_component_with_tier("pkg:cargo/c@1", Some("analyzed")),
        ];
        let mut arts = mk_artifacts("demo", &comps, &[], &integ);
        arts.scope_mode = crate::generate::ScopeMode::Artifact;
        let comment = build_scope_comment(&arts);
        assert!(
            comment.starts_with("Scope: artifact"),
            "expected artifact-mode prefix; got: {comment}"
        );
        // Phase order is lexicographic via BTreeSet:
        //   build → "build", deployed → "operations", analyzed → "post-build"
        assert!(
            comment.contains("build, operations, post-build"),
            "expected sorted phase list; got: {comment}"
        );
        assert!(
            comment.contains("waybill:sbom-tier"),
            "expected pointer to per-component annotation; got: {comment}"
        );
    }

    #[test]
    fn build_scope_comment_emits_manifest_mode() {
        let integ = empty_integrity();
        let comps = vec![mk_component_with_tier("pkg:cargo/a@1", Some("source"))];
        let mut arts = mk_artifacts("demo", &comps, &[], &integ);
        arts.scope_mode = crate::generate::ScopeMode::Manifest;
        let comment = build_scope_comment(&arts);
        assert!(
            comment.starts_with("Scope: manifest"),
            "expected manifest-mode prefix; got: {comment}"
        );
    }

    #[test]
    fn build_scope_comment_handles_empty_phases() {
        let integ = empty_integrity();
        let comps = vec![
            mk_component_with_tier("pkg:cargo/a@1", None),
            mk_component_with_tier("pkg:cargo/b@1", Some("not-a-known-tier")),
        ];
        let arts = mk_artifacts("demo", &comps, &[], &integ);
        let comment = build_scope_comment(&arts);
        assert!(
            comment.contains("no lifecycle phases observed"),
            "expected empty-phases degradation; got: {comment}"
        );
    }

    // -----------------------------------------------------------
    // Issue #236 — synthesized-root behavior
    // -----------------------------------------------------------

    fn build_doc_value(arts: &ScanArtifacts<'_>) -> serde_json::Value {
        let cfg = crate::generate::OutputConfig {
            mikebom_version: "0.0.0-test",
            created: chrono::DateTime::parse_from_rfc3339("2026-05-24T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            overrides: Default::default(),
        };
        let doc = build_document(arts, &cfg);
        serde_json::to_value(&doc).expect("SpdxDocument serializes to JSON")
    }

    #[test]
    fn synthesized_root_purl_preserves_colon_like_cdx() {
        // Issue #236 secondary observation: pre-fix the SPDX
        // synthesized-root PURL collapsed `:` to `_` via
        // sanitize_for_coord, producing `pkg:generic/postgres_16@0.0.0`
        // while CDX emitted `pkg:generic/postgres:16@0.0.0` for the
        // same image. Post-fix the SPDX PURL uses encode_purl_segment
        // (the same helper CDX uses) so colon is preserved literal.
        //
        // Two components so multi-top-level triggers synthesize_root
        // (single-top-level uses the lone component as root instead).
        let integ = empty_integrity();
        let comps = vec![
            mk_component("pkg:apk/alpine/busybox@1.36", "busybox", "1.36"),
            mk_component("pkg:apk/alpine/musl@1.2", "musl", "1.2"),
        ];
        let arts = mk_artifacts("postgres:16", &comps, &[], &integ);
        let doc = build_doc_value(&arts);
        let packages = doc["packages"].as_array().expect("packages[]");
        let synth = packages
            .iter()
            .find(|p| {
                p["SPDXID"]
                    .as_str()
                    .map(|s| s.starts_with("SPDXRef-DocumentRoot-"))
                    .unwrap_or(false)
            })
            .expect("synthetic root present");
        let purl = synth["externalRefs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["referenceType"].as_str() == Some("purl"))
            .and_then(|r| r["referenceLocator"].as_str())
            .expect("purl externalRef");
        assert_eq!(
            purl, "pkg:generic/postgres:16@0.0.0",
            "synthesized-root PURL must match CDX shape (colon preserved literal)"
        );
    }

    #[test]
    fn synthesized_root_has_outgoing_depends_on_to_graph_roots() {
        // Issue #236 primary bug: pre-fix the synthesized root had
        // only the incoming `DESCRIBES` edge — every top-level
        // component was an orphan graph-top with no incoming
        // `DEPENDS_ON`. Post-fix the synthesized root has outgoing
        // `DEPENDS_ON` to every component that nothing else depends
        // on (CDX's primary-dependency fallback mirrored into SPDX
        // 2.3).
        let integ = empty_integrity();
        // Three top-level components (image-scan-shape: no main
        // module, no name match, no inter-component edges).
        let comps = vec![
            mk_component("pkg:apk/alpine/busybox@1.36", "busybox", "1.36"),
            mk_component("pkg:apk/alpine/musl@1.2", "musl", "1.2"),
            mk_component("pkg:apk/alpine/ssl-client@3.18", "ssl-client", "3.18"),
        ];
        let arts = mk_artifacts("alpine:3", &comps, &[], &integ);
        let doc = build_doc_value(&arts);
        let rels = doc["relationships"].as_array().expect("relationships[]");
        let root_id = rels
            .iter()
            .find(|r| r["relationshipType"].as_str() == Some("DESCRIBES"))
            .and_then(|r| r["relatedSpdxElement"].as_str())
            .expect("DESCRIBES edge present");
        let outgoing: Vec<&str> = rels
            .iter()
            .filter(|r| {
                r["spdxElementId"].as_str() == Some(root_id)
                    && r["relationshipType"].as_str() == Some("DEPENDS_ON")
            })
            .filter_map(|r| r["relatedSpdxElement"].as_str())
            .collect();
        assert_eq!(
            outgoing.len(),
            3,
            "expected 3 outgoing DEPENDS_ON edges to the 3 graph-root components, got {outgoing:#?}"
        );
    }

    /// Build a main-module-tagged ResolvedComponent (carries the
    /// `waybill:component-role = "main-module"` annotation that the
    /// emitter drops under `--root-name`). Used by the alpha.35
    /// fallback-gating regression tests below.
    fn mk_main_module(purl: &str, name: &str, version: &str) -> ResolvedComponent {
        let mut c = mk_component(purl, name, version);
        c.extra_annotations.insert(
            "waybill:component-role".to_string(),
            serde_json::Value::String("main-module".to_string()),
        );
        c
    }

    /// Build a ScanArtifacts under `--root-name <name> --root-version
    /// <version>`. Mirrors the production CLI path that sets
    /// `root_override` from the override flags.
    fn mk_artifacts_with_override<'a>(
        target_name: &'a str,
        components: &'a [ResolvedComponent],
        relationships: &'a [waybill_common::resolution::Relationship],
        integrity: &'a TraceIntegrity,
        root_name: &str,
        root_version: &str,
    ) -> ScanArtifacts<'a> {
        let mut arts = mk_artifacts(target_name, components, relationships, integrity);
        arts.root_override = crate::generate::RootComponentOverride {
            name: Some(root_name.to_string()),
            version: Some(root_version.to_string()),
        ..Default::default()
    };
        arts
    }

    #[test]
    fn synth_root_fallback_skipped_when_alias_rewrite_already_populated_edges() {
        // Regression for the alpha.35 cross-format divergence
        // surfaced after #229 + #236 both shipped. When `--root-name`
        // is active, the milestone-#229 alias rewrite at
        // `document.rs:458-465` maps the dropped main-module's
        // PURL → synth-root SPDXID, so relations originally sourced
        // at the manifest main module become outgoing edges from
        // synth-root. The #236 graph-root fallback (lines 483+)
        // must NOT fire on top of that — otherwise it over-attaches
        // graph-root components (e.g., Go `// indirect` entries the
        // milestone-091 go.sum fallback couldn't inter-link) as
        // direct deps of the override root, diverging from CDX
        // which gates its primary-dep fallback on
        // `target_has_no_edges` symmetrically.
        //
        // Shape: 1 main module (dropped under override) + 1 direct
        // dep the main module points at + 1 orphan indirect (no
        // parent_purl, not in any relationship's `to`). After the
        // fix, synth-root has exactly 1 outgoing DEPENDS_ON (to
        // the direct dep — the aliased relationship). Without the
        // gate, synth-root would also pick up the orphan indirect.
        let integ = empty_integrity();
        let main_module = mk_main_module(
            "pkg:golang/github.com/guacsec/guac@v0.0.0-20260101-abcdef",
            "guac",
            "v0.0.0-20260101-abcdef",
        );
        let direct_dep = mk_component(
            "pkg:golang/github.com/spf13/cobra@v1.10.2",
            "cobra",
            "v1.10.2",
        );
        let orphan_indirect = mk_component(
            "pkg:golang/github.com/golang-jwt/jwt/v4@v4.5.2",
            "jwt",
            "v4.5.2",
        );
        let comps = vec![main_module.clone(), direct_dep.clone(), orphan_indirect];
        let rels = vec![waybill_common::resolution::Relationship {
            from: main_module.purl.as_str().to_string(),
            to: direct_dep.purl.as_str().to_string(),
            relationship_type: waybill_common::resolution::RelationshipType::DependsOn,
            provenance: waybill_common::resolution::EnrichmentProvenance {
                source: "test".to_string(),
                data_type: "runtime".to_string(),
            },
        }];
        let arts = mk_artifacts_with_override(
            "guac",
            &comps,
            &rels,
            &integ,
            "guac",
            "v0.0.0-20260101-abcdef",
        );
        let doc = build_doc_value(&arts);
        let rels = doc["relationships"].as_array().expect("relationships[]");
        let root_id = rels
            .iter()
            .find(|r| r["relationshipType"].as_str() == Some("DESCRIBES"))
            .and_then(|r| r["relatedSpdxElement"].as_str())
            .expect("DESCRIBES edge present");
        let outgoing_targets: Vec<&str> = rels
            .iter()
            .filter(|r| {
                r["spdxElementId"].as_str() == Some(root_id)
                    && r["relationshipType"].as_str() == Some("DEPENDS_ON")
            })
            .filter_map(|r| r["relatedSpdxElement"].as_str())
            .collect();
        assert_eq!(
            outgoing_targets.len(),
            1,
            "synth root should have exactly 1 outgoing DEPENDS_ON \
             (aliased from main-module → cobra); the orphan jwt indirect \
             must NOT be attached. Got: {outgoing_targets:?}"
        );
    }

    #[test]
    fn synth_root_fallback_skipped_when_orphan_components_exist_under_root_name() {
        // Bug-B mirror of the test above: orphan npm packages from
        // a secondary `node_modules/` tree (parent_purl unset by
        // the npm reader because the in-tree link wasn't resolved)
        // must NOT get attached to the synth root under `--root-name`
        // either. Same gate-behavior assertion as above; different
        // PURL ecosystem to lock in the cross-ecosystem coverage.
        let integ = empty_integrity();
        let main_module = mk_main_module(
            "pkg:npm/repro-root@0.0.0",
            "repro-root",
            "0.0.0",
        );
        let direct_dep = mk_component("pkg:npm/axios@1.16.1", "axios", "1.16.1");
        let orphan_a = mk_component("pkg:npm/pg@8.17.2", "pg", "8.17.2");
        let orphan_b = mk_component(
            "pkg:npm/pg-connection-string@2.13.0",
            "pg-connection-string",
            "2.13.0",
        );
        let comps = vec![main_module.clone(), direct_dep.clone(), orphan_a, orphan_b];
        let rels = vec![waybill_common::resolution::Relationship {
            from: main_module.purl.as_str().to_string(),
            to: direct_dep.purl.as_str().to_string(),
            relationship_type: waybill_common::resolution::RelationshipType::DependsOn,
            provenance: waybill_common::resolution::EnrichmentProvenance {
                source: "test".to_string(),
                data_type: "runtime".to_string(),
            },
        }];
        let arts = mk_artifacts_with_override(
            "repro",
            &comps,
            &rels,
            &integ,
            "repro",
            "0.0.0",
        );
        let doc = build_doc_value(&arts);
        let rels = doc["relationships"].as_array().expect("relationships[]");
        let root_id = rels
            .iter()
            .find(|r| r["relationshipType"].as_str() == Some("DESCRIBES"))
            .and_then(|r| r["relatedSpdxElement"].as_str())
            .expect("DESCRIBES edge present");
        let outgoing_targets: Vec<&str> = rels
            .iter()
            .filter(|r| {
                r["spdxElementId"].as_str() == Some(root_id)
                    && r["relationshipType"].as_str() == Some("DEPENDS_ON")
            })
            .filter_map(|r| r["relatedSpdxElement"].as_str())
            .collect();
        assert_eq!(
            outgoing_targets.len(),
            1,
            "synth root should have exactly 1 outgoing DEPENDS_ON \
             (aliased main-module → axios); orphan pg + pg-connection-string \
             must NOT be attached to root. Got: {outgoing_targets:?}"
        );
    }

    #[test]
    fn synthesized_root_excludes_already_depended_on_components_from_fallback() {
        // Mirrors CDX's "components nothing else depends on"
        // filter. Given a transitive relationship `A → B`, the
        // synthesized root should only get an edge to A (the graph
        // root), NOT to B (which is already pointed at by A).
        let integ = empty_integrity();
        let comps = vec![
            mk_component("pkg:apk/alpine/a@1", "a", "1"),
            mk_component("pkg:apk/alpine/b@1", "b", "1"),
        ];
        let rels = vec![waybill_common::resolution::Relationship {
            from: "pkg:apk/alpine/a@1".to_string(),
            to: "pkg:apk/alpine/b@1".to_string(),
            relationship_type: waybill_common::resolution::RelationshipType::DependsOn,
            provenance: waybill_common::resolution::EnrichmentProvenance {
                source: "test".to_string(),
                data_type: "runtime".to_string(),
            },
        }];
        let arts = mk_artifacts("alpine:3", &comps, &rels, &integ);
        let doc = build_doc_value(&arts);
        let rels = doc["relationships"].as_array().expect("relationships[]");
        let root_id = rels
            .iter()
            .find(|r| r["relationshipType"].as_str() == Some("DESCRIBES"))
            .and_then(|r| r["relatedSpdxElement"].as_str())
            .expect("DESCRIBES edge present");
        let outgoing_count = rels
            .iter()
            .filter(|r| {
                r["spdxElementId"].as_str() == Some(root_id)
                    && r["relationshipType"].as_str() == Some("DEPENDS_ON")
            })
            .count();
        assert_eq!(
            outgoing_count, 1,
            "synthetic root should get exactly 1 outgoing edge (to graph-root A); B is already depended on by A"
        );
    }

    // ------------------------------------------------------------------
    // Milestone 153 / Issue #485: hasExtractedLicensingInfos sweep tests
    // ------------------------------------------------------------------
    //
    // Tests for `sweep_extracted_license_refs` + `PLACEHOLDER_EXTRACTED_TEXT`.
    // The sweep closes SPDX 2.3 §10.1 conformance by defining every
    // inline `LicenseRef-*` referenced in any package's license field.
    //
    // Test naming convention matches milestone-152's rpm_file.rs tests.
    // See `specs/153-spdx-license-refs-conformance/` for the spec/plan/
    // tasks.

    /// Helper: construct a minimal `SpdxPackage` with the given license-
    /// declared string wrapped in `SpdxLicenseField::Expression`.
    fn mk_pkg_with_declared(expr: &str) -> super::super::packages::SpdxPackage {
        let mut p = mk_pkg("test");
        p.license_declared =
            super::super::packages::SpdxLicenseField::Expression(expr.to_string());
        p
    }

    /// Helper: minimal `SpdxPackage` with all-NoAssertion licenses.
    /// Constructor field order matches `packages.rs:177` struct
    /// definition; only the license fields are load-bearing for the
    /// milestone-153 sweep tests.
    fn mk_pkg(name: &str) -> super::super::packages::SpdxPackage {
        use super::super::packages::{SpdxLicenseField, SpdxPackage};
        SpdxPackage {
            spdx_id: super::super::ids::SpdxId::document(),
            name: name.to_string(),
            version_info: String::new(),
            download_location: "NOASSERTION".to_string(),
            supplier: None,
            originator: None,
            files_analyzed: false,
            checksums: Vec::new(),
            license_declared: SpdxLicenseField::NoAssertion,
            license_concluded: SpdxLicenseField::NoAssertion,
            copyright_text: None,
            external_refs: Vec::new(),
            annotations: Vec::new(),
            primary_package_purpose: None,
        }
    }

    #[test]
    fn sweep_single_package_single_licenseref() {
        // US1 A2 (liblzma5 case): a single-operand LicenseRef- becomes
        // one entry with name = idstring, extractedText = placeholder.
        let pkg = mk_pkg_with_declared("LicenseRef-PD");
        let entries = sweep_extracted_license_refs(&[pkg], Vec::new());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].license_id, "LicenseRef-PD");
        assert_eq!(entries[0].name, "PD");
        assert_eq!(entries[0].extracted_text, PLACEHOLDER_EXTRACTED_TEXT);
    }

    #[test]
    fn sweep_single_package_compound_licenseref() {
        // US1 A1 (busybox case): a compound expression yields ONE entry
        // for the LicenseRef- (the GPL-2.0-only portion is a bare SPDX
        // id, not a LicenseRef-, so it's not extracted).
        let pkg = mk_pkg_with_declared("GPL-2.0-only AND LicenseRef-bzip2-1.0.4");
        let entries = sweep_extracted_license_refs(&[pkg], Vec::new());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].license_id, "LicenseRef-bzip2-1.0.4");
        assert_eq!(entries[0].name, "bzip2-1.0.4");
    }

    #[test]
    fn sweep_dedup_across_multiple_packages() {
        // US1 A3: 4 busybox-family packages all reference the same
        // LicenseRef-bzip2-1.0.4 → exactly ONE entry.
        let pkgs: Vec<_> = ["busybox", "busybox-hwclock", "busybox-syslog", "busybox-udhcpc"]
            .iter()
            .map(|_| mk_pkg_with_declared("GPL-2.0-only AND LicenseRef-bzip2-1.0.4"))
            .collect();
        let entries = sweep_extracted_license_refs(&pkgs, Vec::new());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].license_id, "LicenseRef-bzip2-1.0.4");
    }

    #[test]
    fn sweep_covers_license_concluded_field() {
        // US1 A5 (cross-field): LicenseRef in licenseConcluded (not
        // licenseDeclared) still yields an entry.
        let mut pkg = mk_pkg("cross-field-test");
        pkg.license_concluded =
            super::super::packages::SpdxLicenseField::Expression(
                "LicenseRef-only-in-concluded".to_string(),
            );
        let entries = sweep_extracted_license_refs(&[pkg], Vec::new());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].license_id, "LicenseRef-only-in-concluded");
    }

    #[test]
    fn sweep_licenseref_variant_dedups_with_milestone_012() {
        // T012 (repurposed from `sweep_covers_licenseInfoFromFiles_field`
        // per implementation discovery: SpdxPackage does not carry
        // licenseInfoFromFiles because waybill emits filesAnalyzed:
        // false uniformly). This test instead covers the milestone-012
        // hash-fallback SpdxLicenseField::LicenseRef(...) variant: a
        // wholly-non-canonicalizable expression stored as a
        // LicenseRef-<hash> reference in the field, matched by the
        // existing hasExtractedLicensingInfos entry in `existing` →
        // sweep must NOT emit a duplicate placeholder entry.
        let mut pkg = mk_pkg("m012-hash-fallback");
        pkg.license_declared =
            super::super::packages::SpdxLicenseField::LicenseRef(
                "LicenseRef-hash-fallback-abc123".to_string(),
            );
        let existing = vec![SpdxExtractedLicensingInfo {
            license_id: "LicenseRef-hash-fallback-abc123".to_string(),
            extracted_text: "REAL EXTRACTED TEXT FROM M012".to_string(),
            name: "waybill-extracted-license".to_string(),
        }];
        let entries = sweep_extracted_license_refs(&[pkg], existing);
        assert_eq!(entries.len(), 1, "must not duplicate the m012 entry");
        // Milestone-012 entry wins — real text preserved, not overwritten
        // with placeholder.
        assert_eq!(
            entries[0].extracted_text,
            "REAL EXTRACTED TEXT FROM M012"
        );
    }

    #[test]
    fn sweep_dedup_with_milestone_012_entry() {
        // US1 A6 + FR-005: existing entry with real text wins over
        // placeholder for the same licenseId. Different from the test
        // above: this variant tests the case where the SpdxPackage's
        // license field is an Expression containing a LicenseRef
        // (milestone-152's inline injection path) AND the milestone-012
        // path already emitted an entry for the same id.
        let pkg = mk_pkg_with_declared("GPL-2.0-only AND LicenseRef-shared");
        let existing = vec![SpdxExtractedLicensingInfo {
            license_id: "LicenseRef-shared".to_string(),
            extracted_text: "REAL EXTRACTED TEXT".to_string(),
            name: "shared-license-actual-name".to_string(),
        }];
        let entries = sweep_extracted_license_refs(&[pkg], existing);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].extracted_text, "REAL EXTRACTED TEXT");
        assert_eq!(entries[0].name, "shared-license-actual-name");
    }

    #[test]
    fn sweep_ignores_document_ref_prefixed() {
        // Edge Case per spec: `DocumentRef-<doc>:LicenseRef-<id>` compound
        // refers to a LicenseRef defined in ANOTHER document. waybill's
        // sweep MUST NOT emit an entry for it (that would be technically
        // incorrect — the entry belongs in the OTHER document).
        let pkg = mk_pkg_with_declared("MIT AND DocumentRef-external:LicenseRef-foo");
        let entries = sweep_extracted_license_refs(&[pkg], Vec::new());
        assert_eq!(
            entries.len(),
            0,
            "DocumentRef-prefixed LicenseRef must not be swept"
        );
    }

    #[test]
    fn sweep_covers_nested_compound_structure() {
        // Edge Case per spec: nested parens + mixed operators. Both
        // LicenseRefs must be extracted regardless of surroundings.
        let pkg = mk_pkg_with_declared(
            "MIT AND LicenseRef-foo OR (LicenseRef-bar AND Apache-2.0)",
        );
        let entries = sweep_extracted_license_refs(&[pkg], Vec::new());
        assert_eq!(entries.len(), 2);
        // Sorted lex by license_id per determinism guarantee.
        assert_eq!(entries[0].license_id, "LicenseRef-bar");
        assert_eq!(entries[1].license_id, "LicenseRef-foo");
    }

    #[test]
    fn sweep_no_licenserefs_returns_empty_vec() {
        // US2 A2 + FR-006: no LicenseRef-* anywhere → returned Vec is
        // empty. Combined with the SpdxDocument's serde
        // `skip_serializing_if = "Vec::is_empty"`, this guarantees the
        // `hasExtractedLicensingInfos` JSON key is ABSENT from happy-
        // path output — byte-identity preserved for cargo/npm/go/pip
        // scans that never trigger milestone-152's fallback.
        let pkgs = vec![
            mk_pkg_with_declared("MIT"),
            mk_pkg_with_declared("Apache-2.0 AND GPL-2.0-only"),
        ];
        let entries = sweep_extracted_license_refs(&pkgs, Vec::new());
        assert!(entries.is_empty(), "no LicenseRefs → empty Vec");
    }

    #[test]
    fn placeholder_text_matches_wire_contract() {
        // Milestone-153 Clarifications Q1 wire contract: the placeholder
        // string is byte-locked. Any accidental future edit to the const
        // trips this test. Downstream consumers may pattern-match on
        // this exact string (see the const's doc comment for a jq
        // recipe).
        let expected = "License text not extracted by waybill. Consult the \
             original package (e.g., /usr/share/licenses/<name>/ on \
             Debian/RPM, or upstream project source) for the full text.";
        assert_eq!(PLACEHOLDER_EXTRACTED_TEXT, expected);
    }
}
