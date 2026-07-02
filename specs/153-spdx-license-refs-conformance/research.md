# Research — milestone 153

Phase 0 outputs for the SPDX 2.3 §10.1 conformance fix + SPDX 3 sanity check.

## R1 — Existing infrastructure survey

**Decision**: Reuse the existing `SpdxExtractedLicensingInfo` struct + serde config at `mikebom-cli/src/generate/spdx/document.rs:204-211` verbatim. No struct changes; no serde-annotation changes. The struct already carries the exact SPDX 2.3 §10.1 fields (`licenseId`, `extractedText`, `name`).

**Confirmed at planning time**:
- Struct definition at `document.rs:204-211`: 3 fields (`license_id`, `extracted_text`, `name`) with `#[serde(rename = ...)]` matching §10.1's expected JSON keys.
- The document envelope field `has_extracted_licensing_infos` at `document.rs:187` is a `Vec<SpdxExtractedLicensingInfo>` with `#[serde(rename = "hasExtractedLicensingInfos", skip_serializing_if = "Vec::is_empty")]`. **Critical**: the `skip_serializing_if` is exactly what FR-006 + FR-007 require (empty Vec → key absent → happy-path byte-identity preserved).
- The document assembly at `document.rs:352` calls `build_packages(artifacts, ...)` which returns `(Vec<SpdxPackage>, Vec<SpdxExtractedLicensingInfo>)`. The second element is the milestone-012 hash-fallback Vec. This is the seed for the milestone-153 sweep.
- The final assignment at `document.rs:629` writes `has_extracted_licensing_infos` into the `SpdxDocument` struct.

**Existing milestone-012 emission path** (`spdx/packages.rs:302-359`):
- `build_packages` iterates over all components; for each component, calls `build_package_licenses(...)` which returns an `Option<SpdxExtractedLicensingInfo>` per license kind (declared, concluded).
- Entries are inserted into a `BTreeMap<String, SpdxExtractedLicensingInfo>` (`extracted_by_id`) keyed by `licenseId`. This is per-emitter dedup — same LicenseRef across multiple packages produces ONE entry.
- At the end (`packages.rs:358`), `extracted_by_id.into_values().collect()` produces the returned `Vec<SpdxExtractedLicensingInfo>`.

**Integration point for milestone 153**: right after `build_packages` returns at `document.rs:352-353`, insert the new `sweep_extracted_license_refs` call:

```rust
let (packages, has_extracted_licensing_infos) =
    super::packages::build_packages(artifacts, &annotator, &date);

// Milestone 153: sweep the assembled packages for inline LicenseRef-*
// substrings (milestone-152 escape-hatch output that bypasses the
// milestone-012 per-package path). Dedup with the pre-existing entries.
let has_extracted_licensing_infos = sweep_extracted_license_refs(
    &packages,
    has_extracted_licensing_infos,
);
```

The dedup contract: entries from `build_packages` (real extracted text from milestone-012) survive unchanged; the sweep only ADDS entries for LicenseRef-ids not already present.

**Alternatives considered**:
- Move the sweep INSIDE `build_packages` at `packages.rs`: rejected — the sweep needs to see the fully-assembled `Vec<SpdxPackage>` (with `licenseDeclared` values already stringified). Doing it inside `build_packages` would require walking each package's licenses twice.
- Add a Display-time sweep as part of `Serialize::serialize`: rejected — mikebom's SPDX emission uses derive-Serialize; a custom impl would fork the serde model + risk drift.
- Emit entries eagerly as milestone 152's `preserve_known_operands_with_license_ref` fires in the RPM reader: rejected per FR-014 — this milestone MUST NOT change milestone 152's code. The RPM reader is upstream of document assembly; emitting entries there would require plumbing an extra `Vec<SpdxExtractedLicensingInfo>` through every layer between the reader and the document builder.

## R2 — Regex grammar for LicenseRef- extraction

**Decision**: Use the pattern `LicenseRef-[a-zA-Z0-9.-]+` compiled once via `std::sync::OnceLock` (standard workspace pattern, matches every other lazy-compiled regex in `mikebom-cli/`).

**Rationale**: SPDX 2.3 §10.1 defines the `idstring` grammar as `[a-zA-Z0-9-.]+`. The regex `LicenseRef-[a-zA-Z0-9.-]+` (note: `.` is inside the character class where it's literal) matches every valid LicenseRef- token. The greedy `+` quantifier consumes the longest legal idstring — which is what we want, because the sanitizer strips trailing `-` and `.` isn't an operator character in SPDX license expressions (parens + whitespace + `WITH`/`AND`/`OR` terminate an operand).

**Verified at planning time**: the milestone-152 `sanitize_to_license_ref_idstring` at `rpm_file.rs` produces idstrings matching `[a-zA-Z0-9-.]+`. The regex covers those AND cross-document (`DocumentRef-doc:LicenseRef-id` — the pattern would match `LicenseRef-id` inside the compound). Per Edge Cases in spec, DocumentRef-prefixed tokens are out of scope — the pattern matching `LicenseRef-id` inside `DocumentRef-doc:LicenseRef-id` is HARMLESS because DocumentRef-referenced LicenseRefs are defined in the OTHER document, not this one; producing an entry for them in THIS document would be technically incorrect. To handle this cleanly, the regex uses a **word-boundary-like negative lookbehind** — but Rust's `regex` crate doesn't support lookbehind. Alternative: **filter out matches that appear immediately after `DocumentRef-<docid>:`** in a post-processing step.

Simpler alternative: **prefix-check each match's start position**. After the regex finds a match, check if `haystack[..match.start()]` ends with `:` — if yes, the match is inside a `DocumentRef-<id>:LicenseRef-...` compound; skip it.

**Chosen approach**: regex `LicenseRef-[a-zA-Z0-9.-]+` + post-match filter that skips matches immediately preceded by `:`. Simple, deterministic, matches SPDX 2.3 §10.1 grammar strictly.

**Alternatives considered**:
- Full SPDX expression tokenization (reuse milestone 152's `tokenize`): rejected — that tokenizer lives in `rpm_file.rs` and is not appropriate to promote to a general helper for this scope. The regex approach is simpler and equally correct for the LicenseRef- extraction use case.
- Regex with an explicit non-`:` prefix: `(^|[^:])LicenseRef-[a-zA-Z0-9.-]+` — matches only when preceded by start-of-string or non-`:`. Actually cleaner than the post-filter approach. Adopt this.

**Final decision**: regex `(?:^|[^:])LicenseRef-[a-zA-Z0-9.-]+` with a non-capturing group; the DocumentRef prefix check is baked into the regex. Match start needs +1 offset when the preceding-char capture is non-empty, so extract just the `LicenseRef-...` substring via `matches.iter().map(|m| ...find "LicenseRef-" in m ...)` OR use a captured group for the LicenseRef portion.

**Cleaner final decision** with capture group: `(?:^|[^:])(LicenseRef-[a-zA-Z0-9.-]+)`. Capture group 1 is the pure LicenseRef- token. All matches in group 1 are guaranteed non-DocumentRef-prefixed. Zero post-processing overhead.

## R3 — Sweep function signature + behavior

**Decision**: Signature `fn sweep_extracted_license_refs(packages: &[SpdxPackage], existing: Vec<SpdxExtractedLicensingInfo>) -> Vec<SpdxExtractedLicensingInfo>`.

**Rationale**:
- `packages: &[SpdxPackage]` — borrow the already-assembled Vec, no mutation. The sweep only READS the `licenseDeclared` / `licenseConcluded` / `licenseInfoFromFiles` fields.
- `existing: Vec<SpdxExtractedLicensingInfo>` — take ownership of the milestone-012 output Vec. The sweep dedups against it AND returns the merged Vec (so the caller can use a single assignment).
- Return `Vec<SpdxExtractedLicensingInfo>` — the merged, deduped, sorted-by-`licenseId` Vec ready to hand to the document envelope.

**Behavior** (per data-model.md §1 pseudo-code):
1. Seed a `BTreeMap<String, SpdxExtractedLicensingInfo>` from `existing` (keyed by `license_id`). Existing entries WIN over placeholder entries per FR-005.
2. Iterate over `packages`. For each `SpdxPackage`:
   - For each of the three license-carrying fields, if the value is a String or a String-representation of the license, extract every capture-group-1 match of the regex `(?:^|[^:])(LicenseRef-[a-zA-Z0-9.-]+)`.
   - For each match: if `license_id` NOT already in the map, insert a new `SpdxExtractedLicensingInfo` with:
     - `license_id`: the match verbatim.
     - `extracted_text`: the milestone-153 `PLACEHOLDER_EXTRACTED_TEXT` const.
     - `name`: the idstring portion (strip `LicenseRef-` prefix).
3. `map.into_values().collect()` → sort by `license_id` for determinism → return.

## R4 — Dedup rule with milestone-012 entries (FR-005)

**Decision**: BTreeMap dedup by `licenseId` with milestone-012 entries seeded FIRST. This produces:

- If a LicenseRef-id has ONLY a milestone-012 entry: milestone-012 entry survives (with its real extracted text).
- If a LicenseRef-id has ONLY a milestone-153 entry (i.e., from the milestone-152 inline-injection path): milestone-153 entry emitted (with placeholder text).
- If a LicenseRef-id has BOTH: milestone-012 entry survives; milestone-153 sweep sees it in the map and skips the insert.

**Rationale**: milestone-012's entries carry real extracted text (the raw non-canonicalizable expression preserved verbatim). They are always more useful to consumers than the placeholder. Milestone 153 is strictly ADDITIVE — it adds entries for LicenseRefs milestone-012 doesn't cover; it never overwrites.

**Alternatives considered**:
- Emit milestone-153 entry always, overriding milestone-012: rejected — loses the milestone-012 real-text signal.
- Emit BOTH entries with a mikebom:extraction-source annotation: rejected — SPDX 2.3 §10.1 forbids duplicate `licenseId`s in the array (implicit from "distinct" semantics of §10.1).

## R5 — SPDX 3.0.1 spec §10.1-equivalent investigation (FR-008 / FR-009)

**Decision**: Adopt a **two-step empirical investigation** at Phase 0 (planning-time) + implementation-time:

**Step 1 (planning-time, this doc)**: read the SPDX 3.0.1 spec's license-referencing chapter + inspect the JPEWdev `spdx3-validate==0.0.5` tool's behavior on a document with a bare `LicenseRef-*` in a `simplelicensing_licenseExpression` field WITHOUT a matching `licensing_CustomLicense` element.

**Step 2 (implementation-time, T### task)**: run `spdx3-validate` against a mikebom-emitted SPDX 3 for the issue-#485 testbed. If the validator reports an undefined-reference error, the SPDX 3 fix is required (FR-009 Option a); otherwise it's not needed (FR-009 Option b).

**Preliminary reading of SPDX 3.0.1 spec**:
- SPDX 3 defines `licensing_CustomLicense` (formerly `ExtractedLicensingInfo` in SPDX 2.x) as the graph element that DEFINES a custom license identifier.
- SPDX 3's `simplelicensing_LicenseExpression` element carries a license-expression STRING that may contain LicenseRef-* references.
- The SPDX 3 spec's "License Expression Syntax" section states: "A LicenseRef-* referenced within a simplelicensing_licenseExpression SHOULD have a corresponding `licensing_CustomLicense` element in the same document." "SHOULD" here is normative-but-not-strict — a strict validator MAY flag it as a warning or error.

**Preliminary read of `spdx3-validate==0.0.5` behavior**: the tool is a Python-based spec-conformance validator. It's been used in milestones 078, 080, 081 for SPDX 3 conformance gates. Whether it flags undefined LicenseRef-* is not documented in the tool's readme — it MUST be verified empirically.

**Contingency plan**:
- **Option A (SPDX 3 needs equivalent work)**: extend `spdx/v3_licenses.rs` with a `sweep_custom_licenses` helper that emits a `licensing_CustomLicense` graph element per unique LicenseRef-* found in any `simplelicensing_licenseExpression`. Same dedup rule as SPDX 2.3, different graph-element shape.
- **Option B (SPDX 3 does not need it)**: document the finding in the PR description with the `spdx3-validate` output as evidence. No code change to `v3_licenses.rs`.

**Alternatives considered**:
- Skip the SPDX 3 investigation entirely: rejected per FR-008 — the spec requires it.
- Apply the fix to SPDX 3 unconditionally without validator confirmation: rejected — risks introducing an unnecessary code path if SPDX 3 doesn't need it.

## R6 — `spdx3-validate==0.0.5` runbook

**Decision**: Reuse the milestone-078 test infrastructure at `mikebom-cli/tests/spdx3_conformance.rs` (or equivalent). This test harness sets up `spdx3-validate` invocation for arbitrary emitted documents.

**Verified at planning time**:
- `.venv/spdx3-validate/bin/spdx3-validate` exists at the workspace root (per project memory `reference_spdx3_validator.md`).
- Milestone 078 established the CI gate: `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 cargo test --workspace` runs `spdx3-validate` against every emitted SPDX 3 golden.

**Testbed for this milestone's SPDX 3 investigation**:
- Author a small integration test that scans a synthetic RPM (or reuses an existing fixture) that triggers milestone-152's LicenseRef injection.
- Emit SPDX 3.
- Invoke `spdx3-validate` on the output.
- Assert: (a) if the validator reports "undefined LicenseRef" → FR-009 Option A path is required; (b) if it reports no error → FR-009 Option B; document the finding.

**Alternatives considered**:
- Extend the milestone-078 harness to test all emitted goldens for LicenseRef-*: rejected — too broad. This milestone's investigation is scoped to determining the answer, not extending the harness.

## R7 — Golden test byte-identity for happy-path scans (SC-002)

**Decision**: The `skip_serializing_if = "Vec::is_empty"` on the `has_extracted_licensing_infos` field at `document.rs:185` already guarantees byte-identity when zero LicenseRef-* are found — the JSON key is completely absent from the output. **No golden regeneration needed** for cargo/npm/go/pip fixtures.

**Verified at planning time**: the milestone-090 sibling-fixture testbeds don't emit any LicenseRef-* values in their SPDX 2.3 output (verified by grepping the existing goldens at `mikebom-cli/tests/fixtures/golden/*/`). Adding the sweep is a strict no-op for these fixtures.

**Rationale**: SC-002's byte-identity contract is preserved by relying on the existing serde skip-if-empty behavior + the sweep's contract that it only INSERTS into a non-empty map. If both the milestone-012 seed AND the sweep-found matches are empty, the returned Vec is empty, and serde skips serialization entirely.

## R8 — CHANGELOG.md format + placement

**Decision**: Single-bullet entry under `## [Unreleased]` in `CHANGELOG.md`, matching the milestone-152 + milestone-478 convention. The bullet describes:
- The §10.1 conformance fix + issue #485 reference.
- The locked placeholder text (verbatim, so consumers can grep-and-diff).
- The SPDX 3 investigation outcome (either "applied" or "not required").
- The interaction with the milestone-012 hash-fallback path (dedup, not replace).

The entry goes ABOVE the milestone-152 `[Unreleased]` entry (chronological within the section). Milestones 152 + 153 will ship in the same release cadence.

## R9 — Test inventory (SC-006)

**Decision**: 10 new unit tests inline in `mikebom-cli/src/generate/spdx/document.rs` (or an adjacent test module). Each independent. Uses synthetic `SpdxPackage` instances.

| # | Test | Covers |
|---|------|--------|
| 1 | `sweep_single_package_single_licenseref` | US1 A2 (liblzma5 case): `LicenseRef-PD` alone → 1 entry |
| 2 | `sweep_single_package_compound_licenseref` | US1 A1 (busybox case): `GPL-2.0-only AND LicenseRef-bzip2-1.0.4` → 1 entry (only the LicenseRef-, not GPL-2.0-only) |
| 3 | `sweep_dedup_across_multiple_packages` | US1 A3: 4 busybox-family packages sharing `LicenseRef-bzip2-1.0.4` → 1 entry |
| 4 | `sweep_covers_licenseConcluded_field` | US1 A5 + Edge Case: LicenseRef in `licenseConcluded` (not `licenseDeclared`) → entry emitted |
| 5 | `sweep_covers_licenseInfoFromFiles_field` | US1 A5 (files-side): LicenseRef in `licenseInfoFromFiles` → entry emitted |
| 6 | `sweep_no_licenserefs_returns_empty_vec` | US2 A2: no LicenseRef-* anywhere → returned Vec is empty (byte-identity preserved via serde skip-if-empty) |
| 7 | `sweep_dedup_with_milestone_012_entry` | US1 A6 + FR-005: milestone-012 entry with real text wins over placeholder |
| 8 | `sweep_ignores_document_ref_prefixed` | Edge Case: `DocumentRef-doc:LicenseRef-foo` → NO entry emitted (LicenseRef defined in referenced doc, not this one) |
| 9 | `sweep_covers_nested_compound_structure` | Edge Case: `MIT AND LicenseRef-foo OR (LicenseRef-bar AND Apache-2.0)` → 2 entries (both LicenseRefs extracted regardless of surroundings) |
| 10 | `placeholder_text_matches_wire_contract` | Clarifications Q1 wire contract: the `PLACEHOLDER_EXTRACTED_TEXT` const value is exactly the byte-string documented in FR-004 |

Final count: 10 tests (exceeds SC-006 floor of ≥6).

**Rationale**: Inline synthetic tests keep the suite self-contained + fast. No fixture-repo touches.

**Alternatives considered**:
- Add integration tests scanning a real RPM: rejected — the unit tests + existing golden tests are sufficient. Full end-to-end SC-001 verification remains manual operator-cadence per Assumption 4.
