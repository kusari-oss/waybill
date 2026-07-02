# Contract — `sweep_custom_licenses` (internal to `spdx/v3_licenses.rs`)

Module-local helper. Not `pub` outside `spdx::v3_licenses`. This contract documents the behavioral guarantees for `v3_document.rs`'s invocation + the inline test module + any future SPDX 3 emitter refactor.

## Contract 1 — Signature + memory ownership

```rust
fn sweep_custom_licenses(
    license_expression_elements: &[Value],
    doc_iri: &str,
    creation_info_id: &str,
) -> Vec<Value>
```

**Pre-conditions**:
- `license_expression_elements` MAY be empty.
- `doc_iri` is a URL-safe base IRI (mikebom's existing SPDX 3 emitter builds these upstream).
- `creation_info_id` is the SPDX 3 CreationInfo element's identifier (typically `_:creation-info` in mikebom's output).
- Element entries with `type != "simplelicensing_LicenseExpression"` are treated as no-ops (defensive tolerance — sweep only processes matching elements).

**Post-conditions**:
- Returns a `Vec<Value>` where each entry is a `simplelicensing_CustomLicense` JSON element per data-model.md §3.
- Returned Vec is sorted lex-ascending by the LicenseRef idstring (equivalent to sorting by `spdxId` since spdxId suffix = idstring).
- Pure function: no I/O, no logging, no panics on well-formed input.
- Deterministic: two invocations with equal inputs produce byte-identical outputs.

## Contract 2 — Dedup rule

The sweep deduplicates by LicenseRef **idstring** (LicenseRef- prefix stripped). Each distinct idstring produces exactly ONE `simplelicensing_CustomLicense` element regardless of how many `simplelicensing_LicenseExpression` elements reference the LicenseRef.

Implementation: `BTreeMap<String, Value>` keyed by idstring. First insert wins; subsequent references to the same idstring are no-ops (all yield the same element content per Contract 4).

## Contract 3 — Regex extraction (data-model.md §2)

The sweep uses the regex `(?:^|[^:])(LicenseRef-[a-zA-Z0-9.-]+)`, byte-identical to milestone 153's `document.rs::license_ref_regex()`. Capture group 1 is the full `LicenseRef-<idstring>` token; the non-capturing prefix excludes `DocumentRef-<doc>:LicenseRef-<id>` compound tokens.

**Invariant** (research.md §R3): the two files' regex patterns MUST stay lockstep. Any future change to milestone 153's regex MUST be mirrored here (and vice versa). Grammar drift silently breaks cross-format symmetry.

## Contract 4 — Element construction (data-model.md §3)

For each distinct LicenseRef idstring, the sweep constructs a JSON element with EXACTLY these fields (data-model.md §3 spec):

- `type`: literal `"simplelicensing_CustomLicense"`.
- `spdxId`: `format!("{doc_iri}/licenseref/{idstring}")` (Clarifications Q1 wire contract).
- `creationInfo`: `<creation_info_id>` (verbatim from the input parameter).
- `name`: `<idstring>` (LicenseRef- prefix stripped).
- `simplelicensing_licenseText`: `super::document::PLACEHOLDER_EXTRACTED_TEXT.to_string()` (imported from milestone 153; single source of truth).

**Additional / other fields**: NONE. The element is minimal — only the 5 fields above. No `comment`, `seeAlsos`, or other SPDX 3 optional fields.

## Contract 5 — Cross-format symmetry (FR-009 / FR-010 / FR-011)

The sweep MUST produce elements whose fields match milestone 153's SPDX 2.3 `hasExtractedLicensingInfos[]` entries for the same LicenseRef:

- **Token set equality** (FR-009): the set `{LicenseRef- + element.name for element in <returned Vec>}` MUST equal the set of `licenseId` values from milestone 153's SPDX 2.3 sweep on the same scan.
- **Placeholder identity** (FR-010): `element.simplelicensing_licenseText` MUST be byte-identical to the SPDX 2.3 side's `extractedText` (both use `PLACEHOLDER_EXTRACTED_TEXT`).
- **Name identity** (FR-011): `element.name` MUST equal the SPDX 2.3 side's `name` field for the same LicenseRef.

Contracts 3 (regex identity) + 4 (const import) + 5 (name derivation identity: idstring in both) mechanically enforce these three FRs. The invariant is compile-time-checkable — a bonus test #6 (per research.md §R6) asserts placeholder-text equality.

## Contract 6 — Empty-in, empty-out invariant (FR-007 / FR-008)

When `license_expression_elements` is empty OR when no element's `simplelicensing_licenseExpression` field contains any LicenseRef-* substring, the returned Vec is empty.

An empty returned Vec at the `v3_document.rs` wiring site (data-model.md §5) results in ZERO `simplelicensing_CustomLicense` elements pushed onto `graph` — byte-identity preserved for happy-path scans that never reference any LicenseRef.

## Contract 7 — Element ordering (SC-002 golden byte-identity)

The returned Vec is sorted lex-ascending by LicenseRef idstring. Combined with the wiring's push order (per data-model.md §5: LicenseExpression elements first, then CustomLicense elements), this places CustomLicense elements at a deterministic position in the `@graph` array.

**Interaction with the existing `sort_by_spdx_id` post-assembly sort in `build_document`**: `v3_document.rs` sorts the entire `@graph` by spdxId at the end (verified during Phase 0 code inspection). CustomLicense elements' spdxIds all start with `{doc_iri}/licenseref/`; LicenseExpression elements' spdxIds start with `{doc_iri}/license-decl-` or `{doc_iri}/license-conc-`. Post-sort order depends on the doc_iri common prefix + the alphabetical order of the divergent suffixes (`license-conc-<hash>` < `license-decl-<hash>` < `licenseref/<idstring>` lex-wise since `l` < `n` at char position 8). This ordering is stable across runs and matches expectations.

## Contract 8 — Determinism (SC-002)

For any given input, the returned Vec's content and ordering are deterministic. Two invocations with equal `license_expression_elements` + `doc_iri` + `creation_info_id` produce byte-identical outputs. Required for the existing SPDX 3 golden test infrastructure to remain valid without regeneration.

## Contract 9 — Integration site (data-model.md §5)

The sweep is invoked at `v3_document.rs:587` (immediately after `build_license_elements_and_relationships` returns) with:
- `license_expression_elements` = `&license_elements` (the just-returned Vec, borrowed).
- `doc_iri` = `&doc_iri` (already in scope at the integration site).
- `creation_info_id` = `CREATION_INFO_ID` (const in `v3_document.rs`).

The returned Vec is pushed element-by-element onto `graph` in a `for` loop matching the existing pattern for `license_elements`.

## Contract 10 — What this contract DOES NOT change

- The `simplelicensing_LicenseExpression` element definition or emission path (`build_license_elements_and_relationships` untouched per FR-012 wording adapted for SPDX 3 scope).
- The SPDX 3 dependency / license Relationship emission (`hasDeclaredLicense` / `hasConcludedLicense` untouched).
- The SPDX 3 IRI-construction helper `element_iri_for` (untouched — the new CustomLicense IRI uses a different scheme per Clarifications Q1, so no helper reuse).
- The SPDX 3 CreationInfo, Organization Agent, SoftwareAgent, or SpdxDocument element emission (untouched).
- The SPDX 2.3 emitter, milestone 153's `sweep_extracted_license_refs`, `PLACEHOLDER_EXTRACTED_TEXT` VALUE (byte-identical; only visibility changes), or any other file emitter.
- The CycloneDX 1.6 emitter (untouched per FR-013).
- Any catalog row in `docs/reference/sbom-format-mapping.md` (untouched per SC-007).
- The `SpdxExpression` newtype in `mikebom-common` (untouched per FR-014).
- The milestone-152 `preserve_known_operands_with_license_ref` in `rpm_file.rs` (untouched per FR-015).
