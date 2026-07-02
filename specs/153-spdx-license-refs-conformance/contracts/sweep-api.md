# Contract — `sweep_extracted_license_refs` (internal to `spdx/document.rs`)

Module-local helper. Not `pub` outside `spdx::document`. This contract documents the behavioral guarantees for the file's other functions + the inline test module + any future SPDX 3 sibling helper.

## Contract 1 — Signature + memory ownership

```rust
fn sweep_extracted_license_refs(
    packages: &[SpdxPackage],
    existing: Vec<SpdxExtractedLicensingInfo>,
) -> Vec<SpdxExtractedLicensingInfo>
```

**Pre-conditions**:
- `packages` MAY be empty.
- `existing` MAY be empty.
- Every `existing[i].license_id` MUST start with `"LicenseRef-"` (this is a milestone-012 invariant, restated for the sweep's dedup safety).

**Post-conditions**:
- Returns a `Vec<SpdxExtractedLicensingInfo>` containing:
  - All entries from `existing` (unchanged).
  - Plus one additional entry per unique LicenseRef-* found across all packages' `licenseDeclared` / `licenseConcluded` / `licenseInfoFromFiles` fields that was NOT already in `existing`.
- Returned Vec is sorted by `license_id` (lexicographic) for determinism.
- Pure function: no I/O, no logging, no panics on well-formed input.

## Contract 2 — Dedup rule (FR-005)

Entries in `existing` WIN over sweep-found entries when their `licenseId` matches. This preserves milestone-012's real extracted text (which is more useful to consumers than milestone-153's placeholder).

Implementation: seed a `BTreeMap<String, SpdxExtractedLicensingInfo>` from `existing` before iterating packages. For each sweep match:

```rust
if !map.contains_key(&license_id) {
    map.insert(
        license_id.clone(),
        SpdxExtractedLicensingInfo {
            license_id: license_id.clone(),
            extracted_text: PLACEHOLDER_EXTRACTED_TEXT.to_string(),
            name: license_id.strip_prefix("LicenseRef-")
                .unwrap_or(&license_id).to_string(),
        },
    );
}
```

## Contract 3 — Placeholder text (Clarifications Q1 wire contract)

The `extracted_text` field for sweep-emitted entries carries the value of the module-level `PLACEHOLDER_EXTRACTED_TEXT` const, byte-identically. The const's value is documented in data-model.md §2 AND in the shipped CHANGELOG.md entry. Changing this string requires a milestone-153.1 (or higher) coordinated change with downstream consumer tooling.

## Contract 4 — Name field derivation (FR-003)

The `name` field for sweep-emitted entries is the idstring portion — i.e., `licenseId.strip_prefix("LicenseRef-").unwrap_or(licenseId)`. The `unwrap_or` fallback is defensive; every match of the regex (Contract 5) starts with `LicenseRef-` so the `strip_prefix` always succeeds.

## Contract 5 — Regex extraction (data-model.md §3)

The sweep uses the regex `(?:^|[^:])(LicenseRef-[a-zA-Z0-9.-]+)`. Capture group 1 is the LicenseRef- token (never includes the preceding character). The non-capturing prefix `(?:^|[^:])` excludes `DocumentRef-<doc>:LicenseRef-<id>` compound tokens.

## Contract 6 — Field coverage (FR-001)

The sweep MUST cover all three SPDX 2.3 §10.1 license-carrying fields per package:
- `licenseDeclared` (`SpdxPackage::license_declared`)
- `licenseConcluded` (`SpdxPackage::license_concluded`)
- `licenseInfoFromFiles` (`SpdxPackage::license_info_from_files`)

If any of these fields is an `Option::None` or its String form is empty, that field contributes zero matches — no error, no panic.

## Contract 7 — Empty-in, empty-out invariant (FR-006 / FR-007)

When both `packages` is empty AND `existing` is empty, the returned Vec is empty. When `packages` contains no LicenseRef-* references AND `existing` is empty, the returned Vec is empty.

An empty returned Vec, combined with the existing `#[serde(skip_serializing_if = "Vec::is_empty")]` on the document's `has_extracted_licensing_infos` field, guarantees byte-identity for happy-path scans — the `hasExtractedLicensingInfos` JSON key stays absent from the emitted document.

## Contract 8 — Determinism (SC-002 golden byte-identity)

The returned Vec's ordering is deterministic across runs: sorted by `license_id` lex-ascending. Two invocations with equal inputs produce byte-identical outputs. This is required for the existing SPDX 2.3 golden test infrastructure to remain valid without regeneration.

## Contract 9 — Integration site (data-model.md §4)

The sweep is invoked at `document.rs:352-353` immediately after `build_packages` returns. The returned Vec REPLACES the local `has_extracted_licensing_infos` binding (which was the milestone-012 seed). The replacement is a purely-additive superset; no milestone-012 entry is lost.

## Contract 10 — SPDX 3 sibling (`sweep_custom_licenses`, conditional)

If the SPDX 3 investigation concludes an equivalent path is needed (research.md §R5), a sibling helper `sweep_custom_licenses` is added to `v3_licenses.rs` with an analogous contract:

- Signature: `fn sweep_custom_licenses(license_expression_elements: &[Value], doc_iri: &str, creation_info_id: &str) -> Vec<Value>`
- Behavior: emit one `licensing_CustomLicense` graph element per unique LicenseRef-* found in any expression element's `simplelicensing_licenseExpression` field.
- Dedup: BTreeMap<licenseRefId, Value> to prevent duplicates.
- Return: sorted-by-spdxId `Vec<Value>` merged into the SPDX 3 document's `@graph` array by the caller.

If the investigation concludes no SPDX 3 work is needed, this contract is unrealized; the PR description documents the finding.

## What this contract DOES NOT change

- The `SpdxPackage` struct definition (no field additions).
- The `SpdxExtractedLicensingInfo` struct definition (reused verbatim).
- The `SpdxDocument` envelope (reuses the existing `has_extracted_licensing_infos` field).
- The milestone-012 `build_packages` hash-fallback path (untouched; its output is the sweep's seed).
- Any other format emitter (`generate/cyclonedx/`, `generate/spdx/v3_*` outside the conditional `v3_licenses.rs` extension — untouched per FR-010 + FR-011 + FR-014).
- The `SpdxExpression` newtype in `mikebom-common/src/types/license.rs` (untouched per FR-011).
- The milestone-152 `preserve_known_operands_with_license_ref` helper in `rpm_file.rs` (untouched per FR-014).
- Any catalog row in `docs/reference/sbom-format-mapping.md` (untouched per SC-007).
