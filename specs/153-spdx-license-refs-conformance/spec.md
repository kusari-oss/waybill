# Feature Specification: SPDX 2.3 §10.1 conformance — emit `hasExtractedLicensingInfos` for every `LicenseRef-*` (issue #485)

**Feature Branch**: `153-spdx-license-refs-conformance`
**Created**: 2026-07-01
**Status**: Draft
**Input**: User description: "can we look at: #485 Missing hasExtractedLicensingInfos for LicenseRef-*"

## Origin & context

GitHub issue [#485](https://github.com/kusari-oss/mikebom/issues/485), filed 2026-07-01 by the maintainer, is a follow-up to milestone 152 (#484, closed in `feba7cb`). Milestone 152 introduced the SPDX 2.3 `LicenseRef-<sanitized>` escape hatch to preserve known operands in compound RPM license expressions when one operand is unrecognized (closed issue #481). The fix works end-to-end for its immediate purpose — the 5 originally-affected packages (`busybox`-family + `liblzma5`) now emit `GPL-2.0-only AND LicenseRef-bzip2-1.0.4` and `LicenseRef-PD` instead of `NOASSERTION`.

But a **strict SPDX 2.3 consumer will reject** the resulting document as non-conformant. SPDX 2.3 §10.1 requires that every distinct `LicenseRef-<idstring>` appearing anywhere in the document (in `licenseDeclared` / `licenseConcluded` / `licenseInfoFromFiles`, per package) MUST have a matching entry in the top-level `hasExtractedLicensingInfos[]` array, with at least `licenseId` and `extractedText` populated. Optional but recommended fields are `name`, `comment`, `seeAlsos`.

Current mikebom output does not populate that array **at all** when the LicenseRef arrives inline in a compound expression. The maintainer's testbed rerun at `feba7cb` shows 3 distinct `LicenseRef-*` values referenced across packages (`LicenseRef-GPL-2.0-with-OpenSSL-exception`, `LicenseRef-PD`, `LicenseRef-bzip2-1.0.4`) but the `hasExtractedLicensingInfos` key is entirely absent from the document.

**Context — existing infrastructure**: `mikebom-cli/src/generate/spdx/document.rs:174-209` already defines the `SpdxExtractedLicensingInfo` struct + emission path for the milestone-012 hash-fallback case (where an entire non-canonicalizable expression becomes a `LicenseRef-<hash>` and gets a matching entry). But milestone 152's inline LicenseRef injection bypasses that path — the LicenseRef- ends up as a substring of a compound `licenseDeclared` value without ever being registered in the doc-level array. This milestone extends the existing infrastructure to sweep every emitted license field for LicenseRef- substrings and emit matching entries.

**Scope split per format**:

- **SPDX 2.3**: MUST fix — the §10.1 conformance rule is unambiguous.
- **SPDX 3.0.1**: needs sanity-check investigation. SPDX 3 uses a different license-reference model (`ExpandedLicense` / `ExtendedLicense`) that may or may not require equivalent work. Deferred to the plan/research phase.
- **CycloneDX 1.6**: no work needed. CDX doesn't have the §10.1-equivalent constraint; `license.expression` / `license.name` accept arbitrary tokens without a separate definition table.

## Clarifications

### Session 2026-07-01

- Q: What placeholder string does mikebom emit in the `extractedText` field? → A: **Full disclosure with pointer** — the literal string `"License text not extracted by mikebom. Consult the original package (e.g., /usr/share/licenses/<name>/ on Debian/RPM, or upstream project source) for the full text."` The string is byte-identical across every milestone-153-emitted `extractedText` field so consumers can pattern-match on it as a "mikebom didn't extract real text here" marker. Once shipped, changing this string is a downstream break; FR-004 pins it as the wire contract.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — SPDX 2.3 consumer sees §10.1-conformant output (Priority: P1)

A downstream SPDX 2.3 consumer (compliance auditor, syft/grype/trivy interop, sbomqs, the LF SPDX tools validator) reads a mikebom-emitted SPDX 2.3 document that contains at least one package with a `LicenseRef-*` in its `licenseDeclared` (or `licenseConcluded` / `licenseInfoFromFiles`). Today, the consumer's SPDX validator reports one or more "dangling LicenseRef- reference" errors — the license is used but never defined. After this milestone, the same document passes strict §10.1 validation because every distinct `LicenseRef-<idstring>` referenced anywhere in the document appears as a well-formed entry in the top-level `hasExtractedLicensingInfos[]` array.

**Why this priority**: This is the direct fix for the user-filed issue. SPDX 2.3 §10.1 non-conformance is a blocking bug for consumers that run strict validation (LF SPDX tools, sbomqs, and downstream compliance pipelines). It undermines the trust guarantees mikebom's Constitution Principle V (Specification Compliance) commits to.

**Independent Test**: Scan the issue-#485 testbed (same as milestone 152's — `yocto-test` local repo, `core-image-minimal` qemux86-64, scarthgap LTS, poky `802e4c1`) with the milestone-153 build. Emit SPDX 2.3 JSON. Assert: (a) `hasExtractedLicensingInfos` key is present at the document root; (b) it contains exactly one entry for each of the 3 distinct `LicenseRef-*` values the maintainer's issue enumerates (`LicenseRef-bzip2-1.0.4`, `LicenseRef-PD`, `LicenseRef-GPL-2.0-with-OpenSSL-exception`); (c) each entry has non-empty `licenseId`, `name`, and `extractedText` fields; (d) `jq '.hasExtractedLicensingInfos // "MISSING"'` returns the array (not `"MISSING"`).

**Acceptance Scenarios**:

1. **Given** a mikebom-emitted SPDX 2.3 document containing a package with `licenseDeclared: "GPL-2.0-only AND LicenseRef-bzip2-1.0.4"`, **When** the consumer reads the top-level `hasExtractedLicensingInfos[]` array, **Then** it MUST contain an entry with `licenseId: "LicenseRef-bzip2-1.0.4"`, `name: "bzip2-1.0.4"`, and a non-empty `extractedText` field.
2. **Given** the same document containing a package with `licenseDeclared: "LicenseRef-PD"` (single-operand case from milestone 152), **When** the consumer reads `hasExtractedLicensingInfos[]`, **Then** it MUST contain an entry for `LicenseRef-PD`.
3. **Given** a document where multiple packages reference the same `LicenseRef-bzip2-1.0.4` (busybox-family — 4 packages share the ref), **When** the consumer reads `hasExtractedLicensingInfos[]`, **Then** it MUST contain **exactly one** entry for `LicenseRef-bzip2-1.0.4` (deduplicated).
4. **Given** a document that has NO LicenseRef- references anywhere (all licenses are canonical SPDX ids), **When** the consumer reads the document, **Then** the `hasExtractedLicensingInfos` key MUST be either absent OR present as an empty array (per SPDX 2.3 spec — the field is optional when empty).
5. **Given** a document where a LicenseRef appears in `licenseConcluded` OR `licenseInfoFromFiles` (not just `licenseDeclared`), **When** the consumer reads `hasExtractedLicensingInfos[]`, **Then** the corresponding entry MUST still be present (sweep covers all three fields per SPDX 2.3 §10.1).
6. **Given** a document that ALREADY has a milestone-012-style hash-fallback `LicenseRef-<hash>` (the pre-existing per-package extraction path), **When** milestone-153 sweeps for LicenseRef-, **Then** the existing entry MUST NOT be duplicated in `hasExtractedLicensingInfos[]` (dedup by `licenseId`).
7. **Given** the emitted `hasExtractedLicensingInfos[]` array, **When** consumers run a strict SPDX 2.3 validator against the document, **Then** the validator MUST NOT report any "undefined LicenseRef-" errors.

---

### User Story 2 — Byte-identical happy path when no LicenseRef is present (Priority: P2)

A developer running mikebom against a source tree where every emitted license expression is a canonical SPDX id (no milestone-152 LicenseRef fallback fires) expects byte-identical SPDX 2.3 output before vs. after milestone 153 — the new sweep MUST be a strict no-op on documents that don't need it.

**Why this priority**: Prevents the fix from causing spurious changes to existing SBOM consumers' tooling pipelines. Byte-identity is verified via the existing milestone-090 golden test infrastructure, which covers Cargo / npm / Go / pip fixtures — none of which currently emit LicenseRef- values.

**Independent Test**: Scan the milestone-090 sibling-fixture testbeds (`transitive_parity/cargo`, `transitive_parity/npm`, `transitive_parity/go`) with the milestone-153 build. Emit SPDX 2.3 JSON. Assert byte-identity against pre-milestone-153 golden files.

**Acceptance Scenarios**:

1. **Given** a scan target with no LicenseRef-* values in any emitted license field, **When** mikebom emits SPDX 2.3, **Then** the output MUST be byte-identical to pre-milestone-153 output for the same input.
2. **Given** the emitted SPDX 2.3 document for a happy-path scan, **When** a consumer inspects the top-level structure, **Then** the `hasExtractedLicensingInfos` key MUST be absent (or empty per Acceptance 4 above) — the sweep MUST NOT introduce an empty array where none previously existed.

---

### User Story 3 — SPDX 3 sanity-check (Priority: P3)

A future SPDX 3 consumer reading a mikebom-emitted SPDX 3.0.1 document with LicenseRef-* values expects the equivalent §10.1-equivalent construct to be populated. SPDX 3 uses a different license-reference model — `ExpandedLicense` / `ExtendedLicense` graph elements — and the equivalent conformance rule may or may not apply. This milestone MUST determine whether SPDX 3 requires equivalent work, and either (a) apply the fix to SPDX 3 as well, or (b) document that SPDX 3's model doesn't require it and close the sanity-check.

**Why this priority**: Lower priority because SPDX 3 is still stabilizing (mikebom labels its SPDX 3 emitter as experimental per Constitution Principle V), and the issue body explicitly defers SPDX 3 to a "sanity check" not a mandatory fix. But not ignoring it — the eventual SPDX 3 consumer base deserves the same conformance guarantee.

**Independent Test**: Emit SPDX 3.0.1 for the same issue-#485 testbed. Run against a JSON-LD-aware SPDX 3 validator (the workspace's pinned `spdx3-validate==0.0.5` per milestone 078). Assert: no undefined-license-reference errors. If SPDX 3's model does not require equivalent entries (verified by validator behavior), document the finding in the milestone's PR description and close the US3 sanity-check.

**Acceptance Scenarios**:

1. **Given** the milestone-153 SPDX 3 emitter output for the same testbed, **When** the maintainer runs `spdx3-validate` against it, **Then** the validator MUST NOT report any "undefined license reference" errors. (Either because the fix is applied to SPDX 3 or because the model doesn't require it — determined by planning-phase investigation.)
2. **Given** the investigation outcome, **When** the milestone-153 PR is opened, **Then** the PR description MUST explicitly state whether SPDX 3 required equivalent work AND cite the validator result as evidence.

---

### Edge Cases

- **LicenseRef in `licenseConcluded` from `--conclude-licenses`**: milestone 132 introduced the operator-asserted license-conclusion flow (`mikebom:license-concluded-source = "operator-asserted"`). If an operator concludes a license that happens to be `LicenseRef-*`, the sweep MUST cover `licenseConcluded` too, not just `licenseDeclared`.
- **LicenseRef with nested compound structure**: e.g., `MIT AND LicenseRef-foo OR (LicenseRef-bar AND Apache-2.0)`. The sweep MUST extract BOTH `LicenseRef-foo` and `LicenseRef-bar` regardless of operator/paren surroundings.
- **LicenseRef with DocumentRef prefix**: SPDX 2.3 also supports `DocumentRef-<docid>:LicenseRef-<idstring>` for cross-document references. Milestone 152's `preserve_known_operands_with_license_ref` explicitly does NOT emit `DocumentRef-` forms (FR-011), but the sweep MUST handle them if they appear from any future code path (e.g., a supplement-CDX merge — milestone 119). For this milestone, `DocumentRef-*:LicenseRef-*` cases are out of scope: mikebom doesn't emit them; if they arrive from operator-supplied data, they're passed through as-is without a matching document-level entry (which is the correct behavior since the LicenseRef is defined in the referenced OTHER document, not this one).
- **Empty document (no packages)**: the sweep runs but finds no LicenseRef-*; the `hasExtractedLicensingInfos` key stays absent.
- **Duplicate LicenseRef- across multiple packages**: dedup by `licenseId`. See US1 Acceptance 3.
- **`extractedText` best-effort extraction**: the issue body suggests extracting from RPM `/usr/share/licenses/<pkg>/COPYING` when possible. This milestone uses a **placeholder text** for the MVP (per FR-004) — extraction from RPM contents is DEFERRED to a follow-up milestone. The placeholder is honest ("License text not extracted; consult the original package for the full text.") and fully satisfies §10.1's minimum conformance requirement.
- **CycloneDX and SPDX 3**: CDX is a no-op per Constitution Principle V (no equivalent constraint). SPDX 3 handling is US3.
- **The pre-existing milestone-012 hash-fallback path** at `packages.rs:216-267` MUST continue to emit its per-package entry — the new sweep MUST detect and dedup rather than duplicate.

## Requirements *(mandatory)*

### Functional Requirements

#### Core fix (US1)

- **FR-001**: At SPDX 2.3 document-serialization time, mikebom MUST sweep every emitted `licenseDeclared`, `licenseConcluded`, and `licenseInfoFromFiles` field across all packages for `LicenseRef-<idstring>` substrings (per SPDX 2.3 §10.1). The `idstring` grammar is `[a-zA-Z0-9-.]+` per §10.1.

- **FR-002**: The sweep MUST deduplicate LicenseRef- values by `licenseId` (the full `LicenseRef-<idstring>` string). Each distinct `LicenseRef-<idstring>` MUST produce exactly one entry in the top-level `hasExtractedLicensingInfos[]` array, regardless of how many packages reference it.

- **FR-003**: Each `hasExtractedLicensingInfos[]` entry MUST include the following fields (per SPDX 2.3 §10.1):
  - `licenseId`: the full `LicenseRef-<idstring>` string, verbatim.
  - `extractedText`: a non-empty string (see FR-004).
  - `name`: the `<idstring>` portion (i.e., the `LicenseRef-` prefix stripped). This field is optional per §10.1 but recommended and improves consumer experience.

- **FR-004**: The `extractedText` field MUST carry the following **exact placeholder string**, byte-identical across every milestone-153-emitted entry (per Clarifications Q1 — locked as the wire contract; changing it later is a downstream break):

  ```
  License text not extracted by mikebom. Consult the original package (e.g., /usr/share/licenses/<name>/ on Debian/RPM, or upstream project source) for the full text.
  ```

  The `<name>` token in the placeholder is a **literal** — mikebom does NOT substitute the package name at emission time; consumers reading the placeholder understand it as "look for /usr/share/licenses/<the-package-name>/ where `<the-package-name>` is whatever consumer-context corresponds to this component." Uniform text lets consumers pattern-match on it (e.g., `jq '.hasExtractedLicensingInfos[] | select(.extractedText | startswith("License text not extracted by mikebom."))'`) to distinguish "mikebom placeholder" from real extracted text.

  Best-effort text extraction from RPM contents is DEFERRED to a follow-up milestone (see Out of Scope).

- **FR-005**: The sweep MUST integrate with the pre-existing milestone-012 hash-fallback path at `packages.rs:216-267`. When the sweep finds a `LicenseRef-<idstring>` that was already emitted by the pre-existing path (with its own actual `extractedText`), the sweep MUST NOT duplicate the entry — the existing entry (with its real extracted text) wins.

#### Emission conditions + no-op guard (US2)

- **FR-006**: When zero distinct `LicenseRef-*` values are referenced anywhere in the document (across all packages' `licenseDeclared` / `licenseConcluded` / `licenseInfoFromFiles`), the emitted SPDX 2.3 document MUST NOT emit an empty `hasExtractedLicensingInfos: []` — the field MUST be absent entirely. This preserves byte-identity for happy-path scans that don't hit the milestone-152 fallback.

- **FR-007**: The sweep MUST be a strict no-op on scans that emit zero LicenseRef- values — no new top-level fields introduced, no whitespace changes, no property-ordering changes. Verified via SC-002 byte-identity against pre-milestone-153 golden fixtures.

#### SPDX 3 sanity-check (US3)

- **FR-008**: The milestone's Phase 0 (research) MUST determine whether SPDX 3.0.1 requires an equivalent construct for LicenseRef-* references. Options: (a) emit an equivalent `ExpandedLicense` / `ExtendedLicense` graph element per LicenseRef; (b) confirm via `spdx3-validate` that SPDX 3's model doesn't require it and close the sanity-check.

- **FR-009**: If SPDX 3 DOES require equivalent work (Option a above), this milestone MUST apply the fix to the SPDX 3 emitter as well, with a matching sweep at document-serialization time in `mikebom-cli/src/generate/spdx/v3_document.rs`. If SPDX 3 does NOT require it (Option b), the milestone MUST document this finding in the PR description and cite `spdx3-validate` output as evidence.

#### Scope guards

- **FR-010**: This milestone MUST NOT change the CycloneDX 1.6 emitter (CDX has no §10.1-equivalent constraint per issue body).

- **FR-011**: This milestone MUST NOT change the `SpdxExpression` newtype in `mikebom-common/src/types/license.rs` (the license value flow is unchanged; only the document-serialization layer sweeps for LicenseRef- values).

- **FR-012**: This milestone MUST NOT extract real license text from RPM `/usr/share/licenses/*/` or from any other source. Placeholder text only, per FR-004. Real text extraction is deferred to a follow-up milestone if operator demand surfaces.

- **FR-013**: This milestone MUST NOT introduce a new `mikebom:*` annotation key (per Constitution Principle V — the `hasExtractedLicensingInfos` construct is SPDX 2.3-native; no annotation needed).

- **FR-014**: This milestone MUST NOT change the milestone-152 `preserve_known_operands_with_license_ref` helper in `rpm_file.rs` (the LicenseRef injection is upstream of document serialization; the sweep runs after the packages Vec is fully materialized).

### Key Entities

- **`LicenseRef-<idstring>` substring**: a substring in any package's license field matching the regex `LicenseRef-[a-zA-Z0-9-.]+` per SPDX 2.3 §10.1 grammar.
- **`hasExtractedLicensingInfos[]` array**: SPDX 2.3 top-level document field per §10.1. Each entry is a `SpdxExtractedLicensingInfo` object with at least `licenseId` + `extractedText`.
- **The 3 issue-#485 reference LicenseRefs** (used as the SC-001 acceptance fixture): `LicenseRef-bzip2-1.0.4`, `LicenseRef-PD`, `LicenseRef-GPL-2.0-with-OpenSSL-exception`.
- **The pre-existing hash-fallback entry** (milestone 012): entries produced by `spdx/packages.rs:216-267` for the wholly-non-canonicalizable-expression case. Milestone 153's sweep MUST dedup with these, not duplicate.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001 (issue-#485 testbed conformance)**: After milestone 153 ships, re-scanning the issue-#485 testbed and running the maintainer's diagnostic jq recipes returns:
  - `jq '.hasExtractedLicensingInfos // "MISSING"' out.json` returns the array (not `"MISSING"`).
  - The array contains exactly 3 entries corresponding to the 3 referenced LicenseRefs (busybox-family + liblzma5 + GPL-2.0-with-OpenSSL-exception).
  - Every distinct `LicenseRef-*` from the diagnostic recipe `[.packages[].licenseDeclared | scan("LicenseRef-[A-Za-z0-9._-]+")] | unique` has a matching entry.

- **SC-002 (byte-identical happy path)**: Scanning the milestone-090 sibling-fixture testbeds (cargo + npm + go + pip) with the milestone-153 build produces byte-identical SPDX 2.3 output compared to pre-milestone-153 (verified via the existing golden test infrastructure). This confirms FR-006 + FR-007.

- **SC-003 (strict SPDX 2.3 validator passes)**: Running a strict SPDX 2.3 validator (LF SPDX tools OR sbomqs's conformance mode OR any comparable validator) against the milestone-153 output for the issue-#485 testbed MUST NOT report any "undefined LicenseRef- reference" errors.

- **SC-004 (SPDX 3 investigation outcome documented)**: The milestone-153 PR description explicitly states whether SPDX 3.0.1 required equivalent work and cites `spdx3-validate` output as evidence. Either the SPDX 3 emitter was updated (and the same testbed passes `spdx3-validate`) or the investigation concluded no work needed (with validator confirmation).

- **SC-005 (pre-PR gate)**: `./scripts/pre-pr.sh` MUST pass with the same status as pre-153 main (clippy clean + every test passes except the documented `sbomqs_parity::sbomqs_spdx_score_meets_or_beats_cdx_across_ecosystems` env-only flake).

- **SC-006 (new unit-test coverage)**: At least 6 new unit tests covering: (a) single package with single LicenseRef (matches the liblzma5 case); (b) single package with compound `expr AND LicenseRef-*` (matches busybox case); (c) multiple packages sharing the same LicenseRef (dedup); (d) LicenseRef in `licenseConcluded` but not `licenseDeclared`; (e) empty document (no LicenseRef references) → no `hasExtractedLicensingInfos` field emitted; (f) dedup with the pre-existing milestone-012 hash-fallback path (both paths reference the same idstring — single entry).

- **SC-007 (no wire-format / annotation changes)**: The shipped diff MUST NOT touch `docs/reference/sbom-format-mapping.md`. No new `mikebom:*` annotation keys introduced. The CycloneDX emitter MUST be untouched.

- **SC-008 (CHANGELOG entry)**: The shipped diff MUST include an entry in `CHANGELOG.md` under `[Unreleased]` naming the SPDX 2.3 §10.1 conformance fix + the placeholder-text approach + the issue #485 reference + the SPDX 3 investigation outcome from FR-008/FR-009.

## Assumptions

1. **`spdx/document.rs:174-209` existing infrastructure is reusable**: The `SpdxExtractedLicensingInfo` struct + serde serialization are already in place. This milestone extends the emission path to sweep across all packages rather than only fire on the milestone-012 hash-fallback case.

2. **The placeholder-text approach is acceptable for the MVP**: The issue body explicitly endorses it ("The placeholder path is a small change and fully addresses the §10.1 conformance issue"). Real text extraction from RPM contents is a follow-up milestone if operator demand surfaces.

3. **Placeholder string uniformity**: All milestone-153-emitted `extractedText` fields share one identical placeholder string (per FR-004). This keeps the output deterministic and lets consumers pattern-match on the placeholder to distinguish "mikebom knew this was a LicenseRef but couldn't extract text" from an entry with real extracted text.

4. **The maintainer has the Yocto testbed locally**: same as milestones 478 + 152; SC-001 verification is manual operator-cadence.

5. **SPDX 3 investigation is scoped to determine the answer, not fix the answer**: If the investigation concludes SPDX 3 needs equivalent work, this milestone applies the fix (FR-009 Option a). If not, the milestone documents the finding and closes the sanity-check (FR-009 Option b). No follow-up milestone is created regardless.

6. **`spdx3-validate==0.0.5` is available**: The workspace pin from milestone 078 remains valid. The tool has been used in prior milestones (078, 080, 081) and is documented in the project memory (`reference_spdx3_validator.md`).

7. **No cross-format wire-shape verification needed**: Milestone 153 touches SPDX 2.3 serialization (and possibly SPDX 3) only. CDX is untouched per FR-010. The mikebom `SpdxExpression` newtype is unchanged per FR-011. No format emitter reads from another — cross-format regressions are structurally impossible.

8. **Byte-identity holds for happy-path scans**: SC-002's regression guard depends on FR-006's absent-when-empty rule. If FR-006 is implemented correctly (as opposed to always emitting `hasExtractedLicensingInfos: []`), the milestone-090 goldens remain valid without regeneration.

9. **Milestone 012 hash-fallback path stays operational**: The existing `SpdxExtractedLicensingInfo` emission at `packages.rs:216-267` continues to fire for its original case (whole-expression LicenseRef-<hash>). Milestone 153 dedups with it, not replaces it.

## Dependencies

- **Milestone 152** (PR #484, merged 2026-06-30): introduces the inline `LicenseRef-<sanitized>` values that this milestone must define. Without milestone 152, the SPDX 2.3 output has no inline LicenseRef- values and this milestone would be a strict no-op.
- **Milestone 012 hash-fallback**: existing `SpdxExtractedLicensingInfo` infrastructure at `spdx/document.rs:174-209` + `spdx/packages.rs:216-267`. Reused, not replaced.
- **Milestone 078 SPDX 3 conformance harness**: provides `spdx3-validate==0.0.5` for the SPDX 3 investigation (FR-008 / SC-004).

## Out of Scope

- No real license text extraction from RPM `/usr/share/licenses/*/COPYING` or equivalent sources (per FR-012). This is a natural follow-up milestone if operator demand surfaces; the placeholder path fully addresses §10.1 conformance.
- No CycloneDX 1.6 changes (per FR-010) — no §10.1-equivalent constraint.
- No changes to the milestone-152 `preserve_known_operands_with_license_ref` helper (per FR-014).
- No new `mikebom:*` annotation keys (per FR-013 + Constitution Principle V).
- No `mikebom-common::types::license::SpdxExpression` changes (per FR-011).
- No `DocumentRef-<docid>:LicenseRef-*` cross-document reference emission — mikebom doesn't emit these; if they arrive from operator-supplied data, they're passed through unchanged without a matching document-level entry (correct behavior).
- No investigation into whether OTHER inline LicenseRef sources (e.g., a hypothetical future ecosystem-specific reader) need equivalent coverage — this milestone's sweep is comprehensive across all license fields, so any future inline LicenseRef will automatically get an entry.
- No retroactive re-emission of pre-milestone-153 SBOMs. Existing consumers with cached mikebom SBOMs will keep their non-conformant documents until they re-scan.
