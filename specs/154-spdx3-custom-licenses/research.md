# Research — milestone 154

Phase 0 outputs for the SPDX 3 `simplelicensing_CustomLicense` sweep.

## R1 — Existing SPDX 3 emitter integration site

**Decision**: Invoke the new `sweep_custom_licenses` helper at `mikebom-cli/src/generate/spdx/v3_document.rs` **inside step 6** of `build_document`, right after the existing `build_license_elements_and_relationships(...)` call returns `(license_elements, license_relationships)`. Push the returned `Vec<Value>` of `simplelicensing_CustomLicense` elements onto `graph` alongside the license expression elements (which are pushed in the same block).

**Verified at planning time** — inspecting `v3_document.rs:579-590`:

```rust
// 6. simplelicensing_LicenseExpression elements + their
//    Relationships.
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

**New wiring** (per data-model.md §5):

```rust
let (license_elements, license_relationships) =
    super::v3_licenses::build_license_elements_and_relationships(...);

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

**Rationale**: keeps the CustomLicense emission close to its input source (the LicenseExpression elements) + preserves the existing `graph` push ordering (LicenseExpression elements before CustomLicense elements is aesthetically consistent — expressions declared, then the elements that resolve their LicenseRef- tokens).

**Alternatives considered**:
- Extend `build_license_elements_and_relationships` to return a THIRD Vec: rejected — changes the function's public signature + touches every existing call site + test.
- Emit inside `build_license_elements_and_relationships` and append to its returned `elements` Vec: rejected — same reason (would need caller to re-sort by spdxId if we want lex-ordered).
- Invoke at the top of `build_document` before step 6: impossible — LicenseExpression elements don't exist yet at that point.

## R2 — Dedup rule + return-type shape

**Decision**: Use a `BTreeMap<String, Value>` keyed by LicenseRef **idstring** (not the full `LicenseRef-<idstring>` token). Each entry's Value is the fully-constructed `simplelicensing_CustomLicense` element JSON. Final `map.into_values().collect()` produces the returned Vec; sorted by insertion order is `BTreeMap`'s lex order on keys (idstring alphabetical), which matches the `sort_by_spdx_id` invariant used elsewhere in `v3_licenses.rs` since the spdxId ends with `licenseref/{idstring}`.

**Rationale**: dedup by idstring produces the natural set — one CustomLicense element per unique LicenseRef-referenced-anywhere-in-the-doc. Multiple LicenseExpression elements referencing the same LicenseRef yield exactly ONE CustomLicense element (US1 A3).

**Alternatives considered**:
- Dedup by full `LicenseRef-<idstring>` token: functionally equivalent but requires an extra `LicenseRef-` prefix operation on every insert. Simpler to key by idstring directly.
- Use `HashSet<String>` first, then build elements in a second pass: extra pass; no benefit.
- Sort by spdxId post-collection: `BTreeMap`'s lex order on keys IS the sort by spdxId (spdxId suffix = idstring). No extra sort needed.

## R3 — Regex sharing between milestone 153 (`document.rs`) and milestone 154 (`v3_licenses.rs`)

**Decision**: **Duplicate inline in `v3_licenses.rs`**. The regex is a 3-line construct (`OnceLock<Regex>` static + `license_ref_regex()` accessor function + pattern string). Promoting to a shared module (e.g., a new `mikebom-cli/src/generate/spdx/license_ref_extraction.rs` or similar) would require:
1. A new module declaration in `mod.rs`
2. An import in BOTH `document.rs` and `v3_licenses.rs`
3. A new file-level doc comment explaining what the module does

vs. the alternative of duplicating the 3 lines of code (with a doc comment on the v3_licenses.rs copy naming milestone 153 + document.rs as the source-of-truth definition; both stay lockstep because the pattern is trivial).

**Rationale**: matches the same author-guidance from milestone 153's analysis remediation A2 (which said "prefer inline duplication over module promotion since the scope is 3 lines of regex-init code across one additional file"). The pattern itself is a spec-defined constant (SPDX 2.3 §10.1 idstring grammar); it doesn't drift.

**Alternatives considered**:
- Promote to `pub(crate)` in `document.rs` alongside `PLACEHOLDER_EXTRACTED_TEXT`: works but pollutes `document.rs`'s public surface with an SPDX-3-specific helper.
- New shared module: over-engineered for 3 lines.

## R4 — `PLACEHOLDER_EXTRACTED_TEXT` const sharing

**Decision**: **Promote to `pub(crate) const` in `document.rs`**. The const's byte-value IS the wire contract (Clarifications Q1 from milestone 153). It must be single-sourced. `pub(crate)` visibility lets `v3_licenses.rs` import it as `super::document::PLACEHOLDER_EXTRACTED_TEXT`.

**Different from the regex** (R3) because:
- The regex pattern is stable spec-derived; drift risk is nil.
- The placeholder text is longer + operator-visible; drift risk is real (any typo change in one file could silently diverge from the other, breaking cross-format pattern-matching).

**Rationale**: single-source-of-truth for a load-bearing wire-contract string. `pub(crate)` scoped promotion keeps the const module-adjacent (still in `document.rs` alongside its documentation).

**Alternatives considered**:
- Byte-duplicate the string in `v3_licenses.rs` with a doc comment naming `document.rs` as source: rejected — drift risk. If a future edit changes one and misses the other, cross-format identity silently breaks.
- Move to a new shared module: over-engineered.
- Move to `mikebom-common`: rejected — the placeholder is an SPDX-emitter concern, not a shared type.

## R5 — IRI construction implementation

**Decision**: Implement as a small helper function (or inline `format!` at the insertion site) using the pattern `format!("{doc_iri}/licenseref/{idstring}")` per Clarifications Q1.

**Verified at planning time** — idstring alphabet `[a-zA-Z0-9-.]+`:
- All lowercase letters, uppercase letters, digits, `-`, `.` are in RFC 3986 §2.3 "unreserved" set (`ALPHA / DIGIT / "-" / "." / "_" / "~"`).
- No percent-encoding required.

**Uniqueness / collision-freedom**: two `simplelicensing_CustomLicense` elements would only collide if they shared the same idstring — but dedup (R2) precludes emitting duplicates. Cross-element-type collisions with existing `simplelicensing_LicenseExpression` elements are structurally impossible: the existing scheme uses `{doc_iri}/{license-decl,license-conc}-{hash}`; the new scheme uses `{doc_iri}/licenseref/{idstring}`. Different path prefix (`license-decl-<hash>` vs `licenseref/<idstring>`), guaranteed non-overlapping.

**Alternatives considered**:
- Use a hash-of-idstring suffix for uniformity with existing pattern: rejected per Clarifications Q1 (Option B chosen for human-readability).
- Percent-encode as defensive-code: rejected — unnecessary complexity for a subset alphabet.

## R6 — Test inventory (SC-006)

**Decision**: 5 unit tests inline in `v3_licenses.rs`'s test module (add one if absent). Each independent. Uses synthetic `Value` (`serde_json::json!` macro) `simplelicensing_LicenseExpression` elements as input.

| # | Test | Covers |
|---|------|--------|
| 1 | `sweep_custom_licenses_single_expression_single_licenseref` | US1 A2: single expression with `LicenseRef-PD` → 1 element with correct IRI + name + placeholder |
| 2 | `sweep_custom_licenses_compound_expression` | US1 A1: `"GPL-2.0-only AND LicenseRef-bzip2-1.0.4"` → 1 element for `bzip2-1.0.4` only (GPL-2.0-only is a bare SPDX id) |
| 3 | `sweep_custom_licenses_dedup_across_expressions` | US1 A3: 4 expressions all referencing `LicenseRef-bzip2-1.0.4` → 1 element (dedup) |
| 4 | `sweep_custom_licenses_ignores_document_ref_prefixed` | Edge Case: `DocumentRef-external:LicenseRef-foo` → 0 elements |
| 5 | `sweep_custom_licenses_no_licenserefs_returns_empty` | US2 + FR-007: no LicenseRef in any expression → empty Vec |

Plus a **6th cross-format identity test** (bonus, not in floor): assert that when both milestone-153's `sweep_extracted_license_refs` AND milestone-154's `sweep_custom_licenses` run on synthetic-but-identical LicenseRef sets, the resulting placeholder text values are byte-identical. Locks the cross-format wire contract at compile time.

Total: 6 tests (comfortably above SC-006's floor of 5).

**Rationale**: Inline synthetic tests keep the suite self-contained + fast. No fixture-repo touches. The bonus cross-format test explicitly validates FR-010 as a compile-time regression guard.

**Alternatives considered**:
- Reuse the milestone-078 `spdx3-validate` harness at `spdx3_conformance.rs`: covered by SC-003 already (existing harness runs against ALL SPDX 3 goldens; if any golden ends up containing LicenseRef-* + CustomLicense post-154, the harness verifies the shape).
- Property-based tests via `proptest`: overkill for 5 unit tests.

## R7 — CHANGELOG.md entry shape

**Decision**: Single subsection under `## [Unreleased]` in `CHANGELOG.md`, immediately above milestone-153's entry (chronological within the section). Content documents:
- The SPDX 3 symmetry fix + issue #487 reference.
- The byte-identical cross-format placeholder guarantee (invariant with milestone 153).
- The IRI scheme `{doc_iri}/licenseref/{idstring}`.
- The `pub(crate)` visibility promotion of `PLACEHOLDER_EXTRACTED_TEXT` (single-source-of-truth for the wire contract).
- A cross-format jq recipe consumers can use to verify parity.

## R8 — Verification approach

**SC-001** (issue-#487 testbed cross-format symmetry): manual operator-cadence per quickstart.md Scenario 1. Same maintainer testbed as milestones 152/153/478.

**SC-002** (byte-identical happy path): automated via existing SPDX 3 golden tests + the `every_existing_golden_passes_validator` test in `spdx3_conformance.rs` (which runs `spdx3-validate` on every committed SPDX 3 golden; if any golden regenerates post-154, that's an SC-002 violation).

**SC-003** (`spdx3-validate` continues to pass): re-run `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 cargo test --workspace` (same harness as milestone 078); expect all tests to pass including the new synthetic-emission test if added.

**SC-004** (cross-format placeholder identity): the `pub(crate)` const promotion mechanically enforces this at compile time. Bonus test #6 asserts it explicitly.

## R9 — Interaction with milestone 012's SPDX-3-invisible hash-fallback path

**Decision**: No special handling needed.

**Verified at planning time**: `v3_licenses.rs:reduce_license_vec` uses `canonicalize_or_raw` which preserves the raw expression on canonicalization failure:

```rust
fn canonicalize_or_raw(expr: &str) -> String {
    match SpdxExpression::try_canonical(expr) {
        Ok(canon) => canon.as_str().to_string(),
        Err(_) => expr.to_string(),
    }
}
```

The SPDX 3 emitter does NOT invoke milestone 012's `SpdxId::for_license_ref` (which is SPDX-2.3-only). So no `LicenseRef-<hash>` tokens ever appear in SPDX 3 output from this path. The only LicenseRef- substrings the milestone-154 sweep will see are those injected by milestone 152's inline escape-hatch (RPM-only) via the raw expression preserved on canonicalization failure.

**Consequence**: the sweep's behavior is uniform — every LicenseRef- token found in any expression string gets a matching CustomLicense element. There is no need to distinguish milestone-012-origin vs milestone-152-origin references. (This is simpler than the SPDX 2.3 sweep, which DID need to dedup with milestone-012's pre-existing entries via the `existing` parameter.)
