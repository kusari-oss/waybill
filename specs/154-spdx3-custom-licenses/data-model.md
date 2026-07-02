# Data model — milestone 154

Adds 1 module-local helper function in `mikebom-cli/src/generate/spdx/v3_licenses.rs` + 1-line visibility change to the milestone-153 const in `mikebom-cli/src/generate/spdx/document.rs`. No public API changes, no `Cargo.toml` changes, no struct changes.

## §1 — `sweep_custom_licenses` helper

```rust
/// Milestone 154 (closes issue #487) — sweep every emitted
/// `simplelicensing_LicenseExpression` element's expression string for
/// inline `LicenseRef-<idstring>` substrings and emit a matching
/// `simplelicensing_CustomLicense` graph element per distinct LicenseRef.
///
/// Paired follow-up to milestone 153 which added the SPDX 2.3
/// `hasExtractedLicensingInfos[]` sweep. The two milestones together
/// preserve cross-format symmetry (FR-009 / FR-010 / FR-011): the SPDX
/// 2.3 side's `hasExtractedLicensingInfos[]` entries and this
/// milestone's `simplelicensing_CustomLicense` elements describe the
/// same LicenseRef set with byte-identical placeholder text.
///
/// # Behavior
///
/// 1. Iterate over `license_expression_elements` — each is expected to
///    have `type == "simplelicensing_LicenseExpression"` with a
///    `simplelicensing_licenseExpression` string field. Elements of
///    other types (defensive) are ignored.
/// 2. For each expression string, extract all capture-group-1 matches
///    of the regex `(?:^|[^:])(LicenseRef-[a-zA-Z0-9.-]+)`. Same regex
///    grammar as milestone 153; DocumentRef-prefixed compound tokens
///    are excluded by the non-capturing prefix.
/// 3. Strip the `LicenseRef-` prefix to derive the idstring; dedup by
///    idstring via `BTreeMap<String, Value>`.
/// 4. For each distinct idstring, construct a `simplelicensing_CustomLicense`
///    element with:
///      - type = "simplelicensing_CustomLicense"
///      - spdxId = format!("{doc_iri}/licenseref/{idstring}")
///      - creationInfo = <passed-in>
///      - name = <idstring>
///      - simplelicensing_licenseText = super::document::PLACEHOLDER_EXTRACTED_TEXT
///        (imported from milestone 153; single source of truth)
/// 5. `map.into_values().collect()` — `BTreeMap`'s lex ordering on the
///    idstring key produces deterministic output; since spdxId ends
///    with the idstring, this is equivalent to sorting by spdxId.
///
/// # Returns
///
/// A `Vec<Value>` of `simplelicensing_CustomLicense` graph elements.
/// May be empty (when no LicenseRef-* are referenced anywhere) — the
/// caller pushes each returned element onto the `@graph` array.
///
/// Empty return preserves byte-identity for happy-path scans per FR-007.
fn sweep_custom_licenses(
    license_expression_elements: &[Value],
    doc_iri: &str,
    creation_info_id: &str,
) -> Vec<Value>
```

## §2 — Regex helper

```rust
/// Milestone 154 — compiled regex for extracting SPDX 3 LicenseRef-*
/// tokens from `simplelicensing_licenseExpression` strings.
///
/// Byte-identical pattern to milestone 153's `license_ref_regex()` in
/// `document.rs`. Duplicated inline per research.md §R3 (the pattern is
/// a 3-line construct + spec-defined constant; drift risk is nil;
/// promoting to a shared module would over-engineer for 3 lines).
/// Milestone 153's `document.rs::license_ref_regex()` is the canonical
/// reference — any change to that pattern MUST be mirrored here (and
/// vice versa).
static LICENSE_REF_REGEX: std::sync::OnceLock<regex::Regex> =
    std::sync::OnceLock::new();

fn license_ref_regex() -> &'static regex::Regex {
    LICENSE_REF_REGEX.get_or_init(|| {
        regex::Regex::new(r"(?:^|[^:])(LicenseRef-[a-zA-Z0-9.-]+)")
            .expect("milestone-154 LicenseRef regex must compile")
    })
}
```

Grammar per SPDX 2.3 §10.1 (SPDX 3.0.1 uses the same idstring grammar within `simplelicensing_LicenseExpression` strings). Non-capturing prefix `(?:^|[^:])` excludes DocumentRef-prefixed compound tokens.

## §3 — Emitted `simplelicensing_CustomLicense` element shape

Each emitted element is a `serde_json::Value` (JSON object):

```json
{
    "type": "simplelicensing_CustomLicense",
    "spdxId": "{doc_iri}/licenseref/{idstring}",
    "creationInfo": "{creation_info_id}",
    "name": "{idstring}",
    "simplelicensing_licenseText": "{PLACEHOLDER_EXTRACTED_TEXT byte-identical to milestone 153}"
}
```

Concrete example (for `LicenseRef-bzip2-1.0.4` in a scan of the issue-#487 testbed):

```json
{
    "type": "simplelicensing_CustomLicense",
    "spdxId": "https://mikebom.kusari.dev/spdx3/doc-abc123/licenseref/bzip2-1.0.4",
    "creationInfo": "_:creation-info",
    "name": "bzip2-1.0.4",
    "simplelicensing_licenseText": "License text not extracted by mikebom. Consult the original package (e.g., /usr/share/licenses/<name>/ on Debian/RPM, or upstream project source) for the full text."
}
```

Field-by-field cross-format invariants (per FR-010 + FR-011):
- `name` in this SPDX 3 element ≡ `name` in the corresponding SPDX 2.3 `hasExtractedLicensingInfos[]` entry ≡ the LicenseRef idstring (LicenseRef- prefix stripped).
- `simplelicensing_licenseText` in this SPDX 3 element ≡ `extractedText` in the corresponding SPDX 2.3 entry ≡ `PLACEHOLDER_EXTRACTED_TEXT` (single source of truth via `pub(crate)` in `document.rs`).

## §4 — Visibility promotion in `document.rs`

**Before** (milestone 153 as-shipped):

```rust
const PLACEHOLDER_EXTRACTED_TEXT: &str =
    "License text not extracted by mikebom. ...";
```

**After** (milestone 154):

```rust
pub(crate) const PLACEHOLDER_EXTRACTED_TEXT: &str =
    "License text not extracted by mikebom. ...";
```

Value BYTE-IDENTICAL. Only the visibility modifier changes. The doc-comment on the const stays as-is (already documents the wire contract).

**Consumer** (in `v3_licenses.rs`):

```rust
use super::document::PLACEHOLDER_EXTRACTED_TEXT;
```

## §5 — Wiring at `v3_document.rs:587-590`

**Before**:

```rust
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
```

**After**:

```rust
let (license_elements, license_relationships) =
    super::v3_licenses::build_license_elements_and_relationships(
        scan.components,
        &package_iri_by_purl,
        &doc_iri,
        CREATION_INFO_ID,
    );

// Milestone 154 (closes #487): sweep the emitted license-expression
// elements for inline LicenseRef-* substrings and emit matching
// simplelicensing_CustomLicense elements per SPDX 3.0.1 §7.
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
```

Net delta: 4 lines → ~13 lines (comment + helper call + second for loop). Existing `license_elements` push loop unchanged.

## §6 — Test inventory (per research.md §R6)

6 unit tests inline in `v3_licenses.rs` (add a `#[cfg(test)] mod tests` block if none exists). Each independent. Uses `serde_json::json!` for synthetic input elements.

## §7 — CHANGELOG.md entry shape

Single `### <heading>` subsection under `## [Unreleased]`, immediately above milestone 153's entry (chronological). Content per research.md §R7.
