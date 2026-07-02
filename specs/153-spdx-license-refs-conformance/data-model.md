# Data model — milestone 153

Adds 1 module-local helper function + 1 module-level `const` in `mikebom-cli/src/generate/spdx/document.rs`. No public API changes. No `Cargo.toml` changes.

## §1 — `sweep_extracted_license_refs` helper

```rust
/// Sweep every LicenseRef-<idstring> substring from the assembled SPDX
/// 2.3 packages' license fields and emit matching hasExtractedLicensingInfos[]
/// entries. Closes SPDX 2.3 §10.1 conformance gap #485.
///
/// Composes with milestone 012's per-package hash-fallback path
/// (packages.rs:build_packages). Milestone-012 entries with real
/// extracted text WIN over placeholder entries emitted by this sweep.
///
/// # Behavior
///
/// 1. Seed a BTreeMap<licenseId, SpdxExtractedLicensingInfo> from
///    `existing` (milestone-012's Vec) — existing entries survive
///    unchanged.
/// 2. Compile regex `(?:^|[^:])(LicenseRef-[a-zA-Z0-9.-]+)` once via
///    OnceLock; capture group 1 is the LicenseRef- token, the
///    non-capturing prefix filter excludes DocumentRef-doc:LicenseRef-
///    forms.
/// 3. For each package: for each of licenseDeclared / licenseConcluded /
///    licenseInfoFromFiles: extract capture-group-1 matches; for each
///    match, insert into the map ONLY if not already present.
/// 4. Return map.into_values() sorted by licenseId for determinism.
///
/// # Returns
///
/// `Vec<SpdxExtractedLicensingInfo>` — the merged, deduped, lex-sorted
/// Vec ready to hand to the document envelope's
/// `has_extracted_licensing_infos` field. Empty when no LicenseRef-* is
/// found and no milestone-012 entries were passed in; the caller's
/// serde `skip_serializing_if = "Vec::is_empty"` handles the byte-
/// identity contract for happy-path scans.
fn sweep_extracted_license_refs(
    packages: &[SpdxPackage],
    existing: Vec<SpdxExtractedLicensingInfo>,
) -> Vec<SpdxExtractedLicensingInfo>;
```

## §2 — `PLACEHOLDER_EXTRACTED_TEXT` const

```rust
/// The uniform placeholder text emitted for every milestone-153 sweep
/// entry per Clarifications Q1 wire contract. Locked byte-exact —
/// changing this string is a downstream break for consumers pattern-
/// matching on it. Documented in the milestone-153 CHANGELOG entry.
///
/// Consumers can distinguish mikebom-emitted placeholder entries from
/// entries with real extracted text via:
///
///     jq '.hasExtractedLicensingInfos[]
///         | select(.extractedText
///                  | startswith("License text not extracted by mikebom."))'
const PLACEHOLDER_EXTRACTED_TEXT: &str =
    "License text not extracted by mikebom. Consult the original package \
     (e.g., /usr/share/licenses/<name>/ on Debian/RPM, or upstream project \
     source) for the full text.";
```

Note the `<name>` token is a LITERAL — mikebom does not substitute the package name at emission time (documented in spec FR-004). Consumers read `<name>` as "look under `/usr/share/licenses/<the-package-name-in-your-context>/`."

## §3 — LicenseRef- extraction regex

```rust
static LICENSE_REF_REGEX: OnceLock<Regex> = OnceLock::new();

fn license_ref_regex() -> &'static Regex {
    LICENSE_REF_REGEX.get_or_init(|| {
        // The `(?:^|[^:])` prefix excludes `DocumentRef-<doc>:LicenseRef-<id>`
        // compound tokens (LicenseRef- is defined in the referenced OTHER
        // document per SPDX 2.3 §10.1, not this one). Capture group 1 is
        // the LicenseRef- token proper.
        Regex::new(r"(?:^|[^:])(LicenseRef-[a-zA-Z0-9.-]+)")
            .expect("milestone-153 LicenseRef regex compile")
    })
}
```

Grammar per SPDX 2.3 §10.1: `LicenseRef-<idstring>` where `<idstring>` is `[a-zA-Z0-9-.]+`. The regex uses the character class `[a-zA-Z0-9.-]` (identical up to ordering; `-` at end so it's literal not a range).

## §4 — Integration site diff at `document.rs:352-353`

Current code:

```rust
let (packages, has_extracted_licensing_infos) =
    super::packages::build_packages(artifacts, &annotator, &date);
```

New code:

```rust
let (packages, has_extracted_licensing_infos) =
    super::packages::build_packages(artifacts, &annotator, &date);

// Milestone 153: sweep assembled packages for inline LicenseRef-*
// substrings from milestone-152's escape-hatch path + dedup with the
// milestone-012 hash-fallback entries just returned above. Closes #485.
let has_extracted_licensing_infos = sweep_extracted_license_refs(
    &packages,
    has_extracted_licensing_infos,
);
```

Net delta at the integration site: 2 lines → 6 lines. The change is purely additive; existing behavior for happy-path scans is preserved (empty in → empty out → serde skip).

## §5 — SPDX 3 conditional path (`sweep_custom_licenses` in `v3_licenses.rs`)

Only implemented if the milestone's `spdx3-validate` investigation (research.md §R5 / §R6) concludes SPDX 3 requires equivalent work. Signature (draft):

```rust
/// SPDX 3 equivalent of milestone-153's SPDX 2.3 sweep. Emits one
/// licensing_CustomLicense graph element per unique LicenseRef-* found
/// in any simplelicensing_LicenseExpression element's
/// simplelicensing_licenseExpression field.
///
/// Called after build_license_elements_and_relationships in the SPDX 3
/// document-assembly path. Elements appended to the returned Vec
/// alongside the existing simplelicensing_LicenseExpression + Relationship
/// elements.
///
/// Emitted graph-element shape (per SPDX 3.0.1 spec):
///
///     {
///         "type": "licensing_CustomLicense",
///         "spdxId": "<doc-iri>/custom-license-<idstring>",
///         "creationInfo": "<creation-info-id>",
///         "simplelicensing_licenseText": "<PLACEHOLDER_EXTRACTED_TEXT>",
///         "name": "<idstring>"
///     }
fn sweep_custom_licenses(
    license_expression_elements: &[Value],
    doc_iri: &str,
    creation_info_id: &str,
) -> Vec<Value>;
```

Implementation is deferred to Phase 2/implementation only if the validator confirms it's needed. Test coverage for SPDX 3 path uses the milestone-078 `spdx3-validate` harness at implementation time.

## §6 — Test inventory (per research.md §R9)

10 unit tests inline in `document.rs`. Each independent. Names + purposes per §R9. Test #10 asserts the exact byte-string of `PLACEHOLDER_EXTRACTED_TEXT` — locks the wire contract per Clarifications Q1.

## §7 — CHANGELOG.md entry shape

Single `### <heading>` subsection under `## [Unreleased]`, matching milestones 152 + 478 convention. Content documents:
- The §10.1 conformance fix + issue #485 reference.
- The verbatim locked placeholder text.
- The SPDX 3 investigation outcome (populated at implementation time).
- The interaction with the milestone-012 hash-fallback path (dedup, not replace).
- A jq recipe consumers can use to distinguish placeholder entries from real-text entries.
