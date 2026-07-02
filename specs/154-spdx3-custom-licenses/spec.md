# Feature Specification: SPDX 3 `simplelicensing_CustomLicense` for inline `LicenseRef-*` (issue #487 — paired follow-up to #485)

**Feature Branch**: `154-spdx3-custom-licenses`
**Created**: 2026-07-02
**Status**: Draft
**Input**: User description: "154"

## Origin & context

GitHub issue [#487](https://github.com/kusari-oss/mikebom/issues/487), filed 2026-07-02 by the maintainer, is a paired follow-up to milestone 153 (PR #486, closed 2026-07-01 in `2d7ab0e`). Milestone 153 fixed the SPDX 2.3 side of the LicenseRef-* conformance issue (issue #485) by sweeping every emitted document's license fields and adding a matching `hasExtractedLicensingInfos[]` entry per SPDX 2.3 §10.1. During milestone 153's investigation, `spdx3-validate==0.0.5` returned exit 0 on a synthetic SPDX 3.0.1 document with an inline `LicenseRef-*` in a `simplelicensing_licenseExpression` WITHOUT any matching `simplelicensing_CustomLicense` element — I concluded SPDX 3.0.1's license-reference model did NOT require the equivalent emission (Outcome B) and shipped milestone 153 with the SPDX 3 side unchanged.

The maintainer's `spdx3-validate` run was correct as a validator check, but the maintainer explicitly filed #487 as a **symmetry ask**, not a spec non-conformance report:

> "SPDX 3.0.1 §7 is less prescriptive than SPDX 2.3 §10.1 about *whether* `CustomLicense` elements are strictly required. A permissive validator will accept the current output; a strict one will flag the same 3 references as dangling. Filing this as a paired follow-up to #485 for symmetry — the same license tokens should be resolvable in either format."

The consumer-facing outcome: a downstream tool that reads both a mikebom SPDX 2.3 document AND its SPDX 3 sibling should get the same LicenseRef-resolution experience. Today they don't: the SPDX 2.3 side defines each LicenseRef via `hasExtractedLicensingInfos[]` (post-153); the SPDX 3 side leaves them dangling.

This milestone closes the symmetry gap by adding a matching sweep in the SPDX 3 emitter that emits `simplelicensing_CustomLicense` graph elements — one per distinct `LicenseRef-*` referenced by any `simplelicensing_LicenseExpression`. The placeholder text is byte-identical to milestone 153's `PLACEHOLDER_EXTRACTED_TEXT` const so consumers pattern-matching on the same prefix work across both formats.

**Concrete testbed evidence** (from #487 body, same testbed as #485 / #481 / #475):

```
$ jq '[.["@graph"][] | select(.type == "simplelicensing_LicenseExpression")
                     | .simplelicensing_licenseExpression]
       | map(scan("LicenseRef-[A-Za-z0-9._-]+")) | flatten | unique' \
    core-image-minimal.spdx.json
[
  "LicenseRef-GPL-2.0-with-OpenSSL-exception",
  "LicenseRef-PD",
  "LicenseRef-bzip2-1.0.4"
]

$ jq '[.["@graph"][] | select(.type | test("_CustomLicense$")) | .spdxId]' \
    core-image-minimal.spdx.json
[]
```

3 distinct LicenseRefs referenced across `simplelicensing_LicenseExpression` elements, 0 `CustomLicense` elements declaring them. Same 3 tokens as issue #485 (same underlying Yocto build).

**Scope split**:

- **SPDX 3.0.1**: MUST fix — this milestone's core work.
- **SPDX 2.3**: no change (already fixed in milestone 153).
- **CycloneDX 1.6**: no work needed (no §10.1-equivalent constraint in CDX).

## Clarifications

### Session 2026-07-02

- Q: What per-element IRI suffix does `simplelicensing_CustomLicense` use? → A: **Readable path segment** — `{doc_iri}/licenseref/{idstring}` (e.g., `https://.../licenseref/bzip2-1.0.4`). Matches the issue body's example. Human-readable — a consumer inspecting a graph can visually match `LicenseRef-bzip2-1.0.4` in a license expression to `licenseref/bzip2-1.0.4` in the CustomLicense's IRI. The idstring alphabet `[a-zA-Z0-9-.]+` is a subset of RFC 3986 unreserved characters, so no percent-encoding needed. Locked as the wire contract; consumers may reference the IRI. Different suffix scheme from the existing `license-decl-<hash>` / `license-conc-<hash>` pattern (which hashes arbitrary expression text); LicenseRef idstrings are already compact stable identifiers, so hashing indirection is unnecessary.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Cross-format symmetry for LicenseRef resolution (Priority: P1)

A downstream compliance auditor reads both the SPDX 2.3 AND the SPDX 3.0.1 output for the same mikebom-scanned artifact (e.g., a compliance workflow that ingests both formats to compare cross-format consistency). Today, the SPDX 2.3 side defines every `LicenseRef-*` via a `hasExtractedLicensingInfos[]` entry (milestone 153); the SPDX 3 side leaves them dangling. After this milestone, both formats define the same LicenseRefs with byte-identical placeholder text — the auditor gets consistent LicenseRef-resolution across formats.

**Why this priority**: Cross-format symmetry is the direct ask from issue #487. Compliance workflows that consume both formats treat the pair as one artifact; a divergence between them is a trust-eroding inconsistency.

**Independent Test**: Scan the issue-#485 / #487 testbed (`yocto-test`, `core-image-minimal` qemux86-64, scarthgap LTS, poky `802e4c1`) with the milestone-154 build. Emit BOTH `spdx-2.3-json` AND `spdx-3-json` outputs. Assert: (a) both formats' license-definition arrays contain the same set of LicenseRef-tokens; (b) the SPDX 3 side's `simplelicensing_CustomLicense.simplelicensing_licenseText` field carries the exact same placeholder string as the SPDX 2.3 side's `hasExtractedLicensingInfos[].extractedText`; (c) the SPDX 3 side's `simplelicensing_CustomLicense.name` field carries the same idstring (LicenseRef-prefix stripped) as the SPDX 2.3 side's `hasExtractedLicensingInfos[].name`.

**Acceptance Scenarios**:

1. **Given** a mikebom-emitted SPDX 3.0.1 document containing a `simplelicensing_LicenseExpression` with `simplelicensing_licenseExpression = "GPL-2.0-only AND LicenseRef-bzip2-1.0.4"`, **When** the consumer reads the graph's `simplelicensing_CustomLicense` elements, **Then** the graph MUST contain an element with `type: "simplelicensing_CustomLicense"`, `name: "bzip2-1.0.4"`, and `simplelicensing_licenseText` = milestone-153's `PLACEHOLDER_EXTRACTED_TEXT` byte-identically.
2. **Given** the same document containing `simplelicensing_licenseExpression = "LicenseRef-PD"` (single-operand case), **When** the consumer reads the graph, **Then** it MUST contain a matching `simplelicensing_CustomLicense` element for `LicenseRef-PD`.
3. **Given** a document where multiple `simplelicensing_LicenseExpression` elements reference the same `LicenseRef-bzip2-1.0.4` (busybox-family — 4 packages), **When** the consumer reads the graph, **Then** it MUST contain **exactly one** `simplelicensing_CustomLicense` element for `LicenseRef-bzip2-1.0.4` (deduplicated across expressions).
4. **Given** a document that has NO `LicenseRef-*` references anywhere in any `simplelicensing_LicenseExpression`, **When** the consumer reads the graph, **Then** the graph MUST contain zero `simplelicensing_CustomLicense` elements (byte-identity preserved for happy-path scans).
5. **Given** BOTH a mikebom-emitted SPDX 2.3 document AND its SPDX 3 sibling for the same testbed, **When** the consumer extracts the set of LicenseRef-token definitions from each, **Then** the two sets MUST be equal (cross-format symmetry).
6. **Given** BOTH emitted formats for the same testbed, **When** the consumer inspects the placeholder text on any matching pair (SPDX 2.3 `extractedText` and SPDX 3 `simplelicensing_licenseText` for the same LicenseRef), **Then** the two strings MUST be byte-identical (the milestone-153 wire contract is preserved cross-format).

---

### User Story 2 — Byte-identical happy path when no LicenseRef is present (Priority: P2)

A developer running mikebom against a source tree where every emitted license expression is a canonical SPDX id (no milestone-152 LicenseRef fallback fires; no milestone-012 hash-fallback either) expects byte-identical SPDX 3 output before vs. after milestone 154 — the new sweep MUST be a strict no-op on documents that don't need it.

**Why this priority**: Same reasoning as milestones 152 + 153 US2 — prevents the fix from causing spurious changes to existing SBOM consumers' tooling pipelines. Byte-identity is verified via the existing milestone-090 golden test infrastructure, which covers Cargo / npm / Go / pip fixtures — none of which currently emit LicenseRef-* values.

**Independent Test**: Scan the milestone-090 sibling-fixture testbeds (`transitive_parity/cargo`, `transitive_parity/npm`, `transitive_parity/go`, `transitive_parity/pip_*`) with the milestone-154 build. Emit SPDX 3 JSON. Assert byte-identity against pre-milestone-154 golden files.

**Acceptance Scenarios**:

1. **Given** a scan target with no LicenseRef-* values in any emitted `simplelicensing_licenseExpression`, **When** mikebom emits SPDX 3, **Then** the output MUST be byte-identical to pre-milestone-154 output for the same input.
2. **Given** the emitted SPDX 3 graph for a happy-path scan, **When** a consumer counts `simplelicensing_CustomLicense` elements, **Then** the count MUST be zero — the sweep MUST NOT introduce these elements when no LicenseRef- references exist.

---

### Edge Cases

- **Dedup across declared + concluded expressions on the same package**: if a package's `licenseDeclared` and `licenseConcluded` both reference `LicenseRef-foo`, exactly ONE `simplelicensing_CustomLicense` element is emitted (dedup by name).
- **LicenseRef with nested compound structure**: e.g., `MIT AND LicenseRef-foo OR (LicenseRef-bar AND Apache-2.0)`. The sweep MUST extract BOTH `LicenseRef-foo` and `LicenseRef-bar` regardless of operator/paren surroundings.
- **LicenseRef with DocumentRef prefix**: `DocumentRef-<doc>:LicenseRef-<id>` compound tokens MUST be excluded from the sweep (the LicenseRef is defined in the referenced OTHER document, not this one). Same rule as milestone 153's SPDX 2.3 sweep.
- **`DocumentRef-` in SPDX 3**: SPDX 3's cross-document reference model uses `import[]` ExternalMap + IRI-based cross-references, not the SPDX 2.3 `DocumentRef-<doc>:LicenseRef-<id>` string shape. mikebom doesn't emit either form today; the sweep's DocumentRef-exclusion regex still handles this defensively for operator-supplied data via supplement-CDX or similar.
- **Empty document (no components)**: the sweep runs but finds no `simplelicensing_LicenseExpression` elements to scan; zero `simplelicensing_CustomLicense` elements emitted.
- **`spdxId` / IRI construction for `simplelicensing_CustomLicense` elements**: mikebom's existing SPDX 3 emitter constructs `spdxId`s via `element_iri_for(...)` from a doc IRI + a stable per-element suffix (see `v3_licenses.rs`). The new `simplelicensing_CustomLicense` elements MUST follow the same pattern — the exact suffix scheme is a planning-phase decision.
- **`creationInfo` reference**: SPDX 3 elements require a `creationInfo` reference pointing at the doc's CreationInfo element. The new `simplelicensing_CustomLicense` elements MUST include this field with the same value used by the SPDX 3 emitter's other elements.
- **Existing `simplelicensing_LicenseExpression` elements that ARE themselves the milestone-012 LicenseRef-<hash> form**: milestone 012's SPDX 2.3 hash-fallback path emits `LicenseRef-<hash>` for wholly-non-canonicalizable expressions. The SPDX 3 emitter doesn't (yet) reuse this hash-fallback path — every SPDX 3 `simplelicensing_LicenseExpression` currently carries the raw or canonical string. The sweep runs on the string content; when the string itself IS `LicenseRef-<hash>`, that hash SHOULD get a `simplelicensing_CustomLicense` element (same rule as any other LicenseRef- reference).

## Requirements *(mandatory)*

### Functional Requirements

#### Core fix (US1)

- **FR-001**: At SPDX 3 document-serialization time, mikebom MUST sweep every emitted `simplelicensing_LicenseExpression` graph element's `simplelicensing_licenseExpression` string for `LicenseRef-<idstring>` substrings (per SPDX 2.3 §10.1 grammar; SPDX 3.0.1 uses the same idstring grammar for LicenseRef within license expressions).

- **FR-002**: The sweep MUST deduplicate LicenseRef- values by their `LicenseRef-<idstring>` full-token key. Each distinct LicenseRef MUST produce exactly one `simplelicensing_CustomLicense` graph element regardless of how many `simplelicensing_LicenseExpression` elements reference it.

- **FR-003**: Each emitted `simplelicensing_CustomLicense` element MUST have the following fields:
  - `type`: the literal string `"simplelicensing_CustomLicense"`.
  - `spdxId`: the IRI `{doc_iri}/licenseref/{idstring}` per Clarifications Q1 (e.g., `https://mikebom.kusari.dev/spdx3/.../licenseref/bzip2-1.0.4`). The `{doc_iri}` is the document's base IRI; `{idstring}` is the LicenseRef idstring (LicenseRef- prefix stripped). No percent-encoding required — the idstring alphabet `[a-zA-Z0-9-.]+` is a subset of RFC 3986 unreserved characters. Locked as the wire contract from this milestone forward.
  - `creationInfo`: the same CreationInfo reference used by every other element in the emitted graph.
  - `name`: the LicenseRef idstring (LicenseRef- prefix stripped) — byte-identical to the milestone-153 SPDX 2.3 side's `hasExtractedLicensingInfos[].name` for the same LicenseRef.
  - `simplelicensing_licenseText`: the exact `PLACEHOLDER_EXTRACTED_TEXT` string introduced by milestone 153 in `mikebom-cli/src/generate/spdx/document.rs`. Consumers pattern-matching on the string see byte-identical text across both formats.

- **FR-004**: The `simplelicensing_licenseText` placeholder MUST be the exact same byte-string as milestone 153's `PLACEHOLDER_EXTRACTED_TEXT` (per the Clarifications Q1 wire contract from milestone 153):

  ```
  License text not extracted by mikebom. Consult the original package (e.g., /usr/share/licenses/<name>/ on Debian/RPM, or upstream project source) for the full text.
  ```

  The `<name>` token is a LITERAL (identical to milestone 153's treatment). Cross-format placeholder identity is the load-bearing invariant — do NOT diverge the string across formats.

- **FR-005**: The sweep MUST correctly handle nested compound license expressions with mixed operators (`AND`, `OR`, `WITH`) and parenthesized sub-expressions. Every `LicenseRef-<idstring>` substring regardless of surroundings MUST be extracted.

- **FR-006**: The sweep MUST correctly exclude `DocumentRef-<docid>:LicenseRef-<idstring>` compound tokens (the LicenseRef is defined in the referenced OTHER document, not this one). Same exclusion rule as milestone 153's SPDX 2.3 sweep.

#### Emission conditions + no-op guard (US2)

- **FR-007**: When zero distinct `LicenseRef-*` values are referenced across all emitted `simplelicensing_LicenseExpression` elements' expression strings, the emitted SPDX 3 document MUST NOT contain any `simplelicensing_CustomLicense` graph element. This preserves byte-identity for happy-path scans that don't hit milestone 152's fallback.

- **FR-008**: The sweep MUST be a strict no-op on scans that emit zero LicenseRef- values — no new graph elements, no re-sorting, no whitespace changes, no property-ordering changes on any existing element. Verified via SC-002 byte-identity against pre-milestone-154 golden fixtures.

#### Symmetry with milestone 153 (US1 cross-format guarantee)

- **FR-009**: For any given scan target that produces LicenseRef-* references, the set of `LicenseRef-<idstring>` full-token strings defined by the SPDX 2.3 side (`hasExtractedLicensingInfos[].licenseId`) MUST equal the set of `LicenseRef-<idstring>` full-token strings referenced by the SPDX 3 side's `simplelicensing_CustomLicense` elements (where each element's referenced full-token is `LicenseRef-<name>`). Cross-format token-set equality is the symmetry contract.

- **FR-010**: For any given LicenseRef defined in both formats, the placeholder text field (`extractedText` in SPDX 2.3; `simplelicensing_licenseText` in SPDX 3) MUST be byte-identical. Cross-format placeholder identity is the symmetry contract.

- **FR-011**: For any given LicenseRef defined in both formats, the human-readable name field (`name` in both formats) MUST be byte-identical.

#### Scope guards

- **FR-012**: This milestone MUST NOT change the SPDX 2.3 emitter (`mikebom-cli/src/generate/spdx/document.rs`, `packages.rs`) — the milestone-153 fix is complete on that side.

- **FR-013**: This milestone MUST NOT change the CycloneDX 1.6 emitter (CDX has no equivalent constraint; per issue #487, CDX is a no-op).

- **FR-014**: This milestone MUST NOT change the `SpdxExpression` newtype in `mikebom-common/src/types/license.rs` (the license value flow is unchanged; only the SPDX 3 document-serialization layer sweeps for LicenseRef- values).

- **FR-015**: This milestone MUST NOT change the milestone-152 `preserve_known_operands_with_license_ref` helper in `rpm_file.rs` (LicenseRef injection is upstream of document serialization).

- **FR-016**: This milestone MUST NOT introduce a new `mikebom:*` annotation key (per Constitution Principle V — `simplelicensing_CustomLicense` is the SPDX 3-native carrier).

- **FR-017**: This milestone MUST NOT extract real license text from any source. Placeholder text only, byte-identical to milestone 153 per FR-004. Real text extraction is deferred to a future milestone if operator demand surfaces (would apply equally to both formats).

- **FR-018**: This milestone MUST NOT change the milestone-153 `PLACEHOLDER_EXTRACTED_TEXT` const in `mikebom-cli/src/generate/spdx/document.rs`. This milestone READS the const's value (via `pub(crate)` visibility promotion OR by duplicating the byte-exact string in the SPDX 3 emitter with a doc comment naming milestone 153 as the source of truth). The choice between visibility-promotion vs. byte-duplication is a planning-phase decision; both preserve the wire-contract identity.

### Key Entities

- **`simplelicensing_CustomLicense` graph element**: SPDX 3.0.1 element with fields `type` / `spdxId` / `creationInfo` / `name` / `simplelicensing_licenseText`. Analogous to SPDX 2.3's `SpdxExtractedLicensingInfo` struct.
- **The 3 issue-#487 reference LicenseRefs** (used as the SC-001 acceptance fixture — same set as issue #485): `LicenseRef-bzip2-1.0.4`, `LicenseRef-PD`, `LicenseRef-GPL-2.0-with-OpenSSL-exception`.
- **The cross-format symmetry pair**: for any given scan target, the SPDX 2.3 `hasExtractedLicensingInfos[]` entries and the SPDX 3 `simplelicensing_CustomLicense` graph elements MUST describe the same LicenseRef set with byte-identical placeholder text + name fields (FR-009 / FR-010 / FR-011).
- **The milestone-153 `PLACEHOLDER_EXTRACTED_TEXT` const**: byte-locked wire contract from milestone 153. Reused verbatim by this milestone. Do NOT diverge.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001 (issue-#487 testbed cross-format symmetry)**: After milestone 154 ships, re-scanning the issue-#487 testbed and emitting BOTH `spdx-2.3-json` AND `spdx-3-json` outputs, the maintainer's diagnostic recipes MUST return:
  - `jq '[.["@graph"][] | select(.type == "simplelicensing_CustomLicense") | .name] | sort'` on the SPDX 3 output returns exactly `["GPL-2.0-with-OpenSSL-exception", "PD", "bzip2-1.0.4"]`.
  - Cross-format symmetry check (bash + jq):
    ```bash
    diff <(jq -c '[.hasExtractedLicensingInfos[].licenseId] | sort' out.spdx.json) \
         <(jq -c '["LicenseRef-" + .["@graph"][].name] | map(select(startswith("LicenseRef-"))) | sort' out.spdx3.json | jq 'select(length > 0)')
    ```
    MUST produce empty diff (both sets equal).
  - `jq '.["@graph"][] | select(.type == "simplelicensing_CustomLicense") | .simplelicensing_licenseText' out.spdx3.json` returns 3 lines, all byte-identical to the milestone-153 `PLACEHOLDER_EXTRACTED_TEXT` string.

- **SC-002 (byte-identical happy path)**: Scanning the milestone-090 sibling-fixture testbeds (cargo + npm + go + pip) with the milestone-154 build produces byte-identical SPDX 3 output compared to pre-milestone-154 (verified via the existing golden test infrastructure). This confirms FR-007 + FR-008.

- **SC-003 (`spdx3-validate` continues to pass)**: `spdx3-validate==0.0.5` MUST return exit 0 on both pre-154 and post-154 SPDX 3 output for the issue-#487 testbed (adding well-formed `simplelicensing_CustomLicense` elements does NOT introduce any schema or SHACL violation).

- **SC-004 (cross-format placeholder identity)**: The `simplelicensing_licenseText` field of every emitted `simplelicensing_CustomLicense` element MUST be byte-identical to the `extractedText` field of the corresponding SPDX 2.3 `hasExtractedLicensingInfos[]` entry for the same LicenseRef. Verified mechanically via a golden-test cross-format-diff at authoring time OR via SC-001's cross-format recipe.

- **SC-005 (pre-PR gate)**: `./scripts/pre-pr.sh` MUST pass with the same status as pre-154 main (clippy clean + every test passes except the documented `sbomqs_parity::sbomqs_spdx_score_meets_or_beats_cdx_across_ecosystems` env-only flake).

- **SC-006 (new unit-test coverage)**: At least 5 new unit tests covering: (a) single expression with single LicenseRef; (b) single expression with compound `expr AND LicenseRef-*`; (c) multiple expressions sharing the same LicenseRef (dedup); (d) DocumentRef-prefixed LicenseRef excluded; (e) empty scan (no LicenseRef references) → no `simplelicensing_CustomLicense` elements emitted.

- **SC-007 (no wire-format / annotation changes beyond intended)**: The shipped diff MUST NOT touch `docs/reference/sbom-format-mapping.md`. No new `mikebom:*` annotation keys introduced. The CycloneDX emitter MUST be untouched. The SPDX 2.3 emitter MUST be untouched (per FR-012).

- **SC-008 (CHANGELOG entry)**: The shipped diff MUST include an entry in `CHANGELOG.md` under `[Unreleased]` naming the SPDX 3 symmetry fix + issue #487 reference + the byte-identical placeholder guarantee with milestone 153.

## Assumptions

1. **Reuse milestone-153's `PLACEHOLDER_EXTRACTED_TEXT`**: The exact placeholder text is fixed by milestone 153's Clarifications Q1 wire contract. This milestone does NOT introduce a separate SPDX-3-specific placeholder; it uses the same byte-string. Whether reuse happens via `pub(crate)` visibility promotion or by duplicating the string with a doc-comment reference to milestone 153 is a planning-phase decision — both preserve the invariant.

2. **The maintainer has the Yocto testbed locally**: same as milestones 478 / 152 / 153; SC-001 verification is manual operator-cadence.

3. **`spdx3-validate==0.0.5` is available**: The workspace pin from milestone 078 remains valid + confirmed working during milestone 153's investigation. This milestone's SC-003 check re-uses the same tool.

4. **SPDX 3's element-IRI convention is stable**: mikebom's existing `element_iri_for(...)` helper at `v3_licenses.rs` gives every `simplelicensing_LicenseExpression` a deterministic IRI. The new `simplelicensing_CustomLicense` elements follow the same convention with a distinct per-element suffix (e.g., `licenseref-<idstring>`, exact scheme decided at plan time).

5. **CreationInfo reuse**: SPDX 3 requires every element to reference a CreationInfo. The new `simplelicensing_CustomLicense` elements will use the same CreationInfo reference as every other emitted element (planning-phase detail: pass `creation_info_id` into the new sweep helper).

6. **No CDX or SPDX 2.3 regression risk**: milestone 154 is fully SPDX-3-emitter-scoped (per FR-012 + FR-013). The other formats' code paths are unchanged.

7. **Milestone-153 SPDX 2.3 sweep continues to fire correctly**: FR-009 / FR-010 / FR-011 cross-format symmetry contracts depend on milestone 153's SPDX 2.3 sweep producing well-formed entries. Milestone-153 shipped with SC-001 verification pending manual maintainer testbed run; this milestone's SC-001 verification exercises BOTH milestone-153's SPDX 2.3 sweep AND milestone-154's SPDX 3 sweep in the same testbed pass.

8. **Milestone-012 SPDX 2.3 hash-fallback continues to fire correctly**: milestone 012's `LicenseRef-<hash>` path for wholly-non-canonicalizable expressions is unchanged. If any such `LicenseRef-<hash>` reaches the SPDX 3 emitter through the shared `SpdxExpression` newtype (unlikely — SPDX 3 emitter's canonicalize_or_raw path preserves the raw expression rather than falling back to a hash), the sweep handles it uniformly with any other LicenseRef- reference.

9. **`simplelicensing_CustomLicense` is the correct SPDX 3.0.1 element type**: per the SPDX 3.0.1 spec (via issue #487 body's citation) and the JPEWdev `spdx3-validate` schema. If planning-phase investigation surfaces a different element type is preferred (e.g., `expandedlicensing_CustomLicense` with richer fields), the decision is documented at plan time; the placeholder-text approach applies to whichever element type is chosen.

## Dependencies

- **Milestone 153** (PR #486, merged 2026-07-01): introduces the `PLACEHOLDER_EXTRACTED_TEXT` const + the SPDX 2.3 sweep. This milestone reuses the const value + implements the symmetric SPDX 3 sweep.
- **Milestone 152** (PR #484, merged 2026-06-30): introduces the inline `LicenseRef-<sanitized>` values that this milestone's SPDX 3 sweep must define.
- **Milestone 011 SPDX 3 emitter** at `mikebom-cli/src/generate/spdx/v3_licenses.rs`: provides the existing `build_license_elements_and_relationships` function + `element_iri_for` helper. Reused, not replaced.
- **Milestone 078 SPDX 3 conformance harness**: provides `spdx3-validate==0.0.5` for the SC-003 check.

## Out of Scope

- No real license text extraction (per FR-017). Deferred to a future milestone if operator demand surfaces.
- No SPDX 2.3 or CycloneDX changes (per FR-012 + FR-013).
- No `SpdxExpression` newtype changes (per FR-014).
- No new `mikebom:*` annotation keys (per FR-016).
- No milestone-152 rpm_file.rs changes (per FR-015).
- No milestone-153 `PLACEHOLDER_EXTRACTED_TEXT` const changes (per FR-018).
- No `expandedlicensing_CustomLicense` (the richer SPDX 3 element type). If planning-phase investigation surfaces a strong reason to prefer it, the decision is revisited at that time; per default assumption 9, `simplelicensing_CustomLicense` is the target.
- No cross-document `DocumentRef-<doc>:LicenseRef-<id>` handling (mikebom doesn't emit these; regex excludes them defensively).
- No retroactive re-emission of pre-milestone-154 SPDX 3 SBOMs.
