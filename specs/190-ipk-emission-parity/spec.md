# Feature Specification: ipk Emission Parity with RPM Reader

**Feature Branch**: `190-ipk-emission-parity`
**Created**: 2026-07-13
**Status**: Draft
**Input**: User description: "m190: ipk emission parity with rpm reader — CDX license operator normalization (#550), SPDX 3 license emission (#551), ipk epoch PURL qualifier (#552)"

## Clarifications

### Session 2026-07-13

- Q: Which BitBake license-operator forms must the normalizer handle? → A: Handle both single and double forms (`&`, `|`, `&&`, `||`) with flexible surrounding whitespace, canonicalizing all to SPDX `AND`/`OR`.
- Q: How should the normalization pass transform the raw BitBake expression into an SPDX-canonical one? → A: String-level operator substitution (long-form first: `&&`/`||` before `&`/`|`) followed by `SpdxExpression::try_canonical` validation — reuses the m152 helper; preserves parenthesization and WITH-clauses verbatim; avoids re-implementing an SPDX expression parser.
- Q: How should empty/missing License fields be represented across the three formats? → A: Format-idiomatic — SPDX 2.3 emits `licenseDeclared: "NOASSERTION"`, CDX 1.6 emits `licenses: []` (empty array), SPDX 3 omits the `simplelicensing_LicenseExpression` link on the `software_Package` element. If the rpm reader currently diverges from this convention, align it as part of this milestone (verify at plan time).
- Q: Should the epoch fix (#552) sweep dpkg + apk readers as well? → A: opkg-only in m190; file a follow-up ticket to audit dpkg + apk readers for the same bug class. Rationale: keeps milestone scope aligned with the observed-and-reported bug; avoids doubling test-fixture + golden-regen surface for readers that may or may not have the same defect.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - CycloneDX license expressions use SPDX operators, not BitBake operators (Priority: P1)

An operator scanning a Yocto-built directory of `.ipk` files with a compound BitBake `License:` field (e.g., `GPL-2.0-only & MIT`) receives a CycloneDX 1.6 SBOM whose `components[].licenses` block uses SPDX-conformant operators (`AND`, `OR`, `WITH`) rather than the raw BitBake operators (`&`, `|`) that appear verbatim in the ipk control file. Downstream license-compliance tools (Trivy, Syft, FOSSology, ScanCode) accept the expression without a parse error.

**Why this priority**: This is a correctness-and-conformance bug that breaks the primary downstream use case for CDX SBOMs (feeding a license/vulnerability scanner). SPDX 2.3 already emits canonical operators for the same input; CDX is silently emitting a non-canonical form for the same data. Fixing this restores CDX/SPDX 2.3 parity and unblocks license-scanning workflows against Yocto artifacts. Filed as issue #550.

**Independent Test**: Scan a fixture ipk with `License: GPL-2.0-only & MIT` and produce a CDX 1.6 SBOM. Assert every `components[].licenses[].expression` field parses as a valid SPDX expression via the same normalization helper the SPDX 2.3 emitter already uses, and specifically contains no raw `&` or `|` operator characters outside quoted operand names.

**Acceptance Scenarios**:

1. **Given** an ipk with `License: GPL-2.0-only & MIT`, **When** scanned with `--format cyclonedx-json`, **Then** the emitted expression is `GPL-2.0-only AND MIT` (or an equivalent SPDX-canonical form).
2. **Given** an ipk with `License: MIT | Apache-2.0`, **When** scanned with `--format cyclonedx-json`, **Then** the emitted expression is `MIT OR Apache-2.0`.
3. **Given** an ipk with nested compound licenses (`License: (GPL-2.0-only & MIT) | Apache-2.0`), **When** scanned with `--format cyclonedx-json`, **Then** the emitted expression preserves grouping semantics using SPDX operators.
4. **Given** the same ipk fixture, **When** scanned once with `--format cyclonedx-json` and once with `--format spdx-2.3-json`, **Then** the license expressions in both outputs are semantically equivalent (identical after SPDX canonicalization).
5. **Given** an ipk with an unknown/vendor-specific operand (e.g., `License: SomeVendorLicense`), **When** scanned with `--format cyclonedx-json`, **Then** the emitter falls back to the same LicenseRef mechanism the SPDX 2.3 side already uses (no regression relative to the existing #481/m152 behavior).

---

### User Story 2 - SPDX 3 documents include license fields for ipk components (Priority: P1)

An operator scanning a Yocto-built directory of `.ipk` files and requesting `--format spdx-3-json` receives a document in which every `software_Package` element carries license information — as `simplelicensing_LicenseExpression` elements (for SPDX-legal expressions) and/or `simplelicensing_CustomLicense` elements (for vendor licenses). The current output emits `software_Package` elements with **no license fields at all**, dropping data that SPDX 2.3 (for the same scan) does emit correctly.

**Why this priority**: This is a data-loss regression: SPDX 3 elements omit license information that the ipk reader already extracted successfully (visible in SPDX 2.3 output for the same run). Filed as issue #551. Same root cause suspected as #550 — the ipk reader's license extraction hasn't been wired into all three format emitters. Ships together with US1 because both trace to the same routing gap.

**Independent Test**: Scan a fixture ipk with any non-empty `License:` field and produce SPDX 3 output. Assert the graph contains at least one `simplelicensing_LicenseExpression` OR `simplelicensing_CustomLicense` element, and that every `software_Package` element derived from an ipk has a valid link to license info (via the existing SPDX 3 relationship pattern used for rpm packages).

**Acceptance Scenarios**:

1. **Given** an ipk with `License: MIT`, **When** scanned with `--format spdx-3-json`, **Then** the emitted graph contains a `simplelicensing_LicenseExpression` element with the value `MIT`.
2. **Given** an ipk with `License: GPL-2.0-only & MIT`, **When** scanned with `--format spdx-3-json`, **Then** the emitted graph contains a `simplelicensing_LicenseExpression` element for the canonicalized `GPL-2.0-only AND MIT` expression.
3. **Given** an ipk with an unknown vendor license, **When** scanned with `--format spdx-3-json`, **Then** the emitted graph contains a `simplelicensing_CustomLicense` element (matching the m154 sweep for rpm).
4. **Given** the same ipk fixture, **When** scanned with `--format spdx-2.3-json` and `--format spdx-3-json`, **Then** every license value present in the SPDX 2.3 `packages[].licenseDeclared` (or `hasExtractedLicensingInfos`) appears as an equivalent element in the SPDX 3 graph.
5. **Given** a valid SPDX 3 output produced from a Yocto ipk directory, **When** validated against `spdx3-validate==0.0.5`, **Then** validation passes (no license-field conformance errors).

---

### User Story 3 - ipk PURLs encode epoch as a qualifier, not embedded in the version string (Priority: P2)

An operator scanning `.ipk` files whose filenames or control metadata carry an epoch prefix (`<digits>:<version>-<release>`, e.g., `netbase_1:6.4-r0_all.ipk`) receives components whose `version` field contains only the naked upstream version and whose PURL carries `?epoch=<digits>` as a qualifier — matching the purl-spec's opkg/deb/rpm convention. The current output emits the epoch inline in the version string (`version: "1:6.4"`, `purl: "pkg:opkg/netbase@1:6.4"`), which downstream tooling comparing by canonical PURL treats as a distinct component from the correctly-formed variant.

**Why this priority**: This is a real bug (breaks canonical-PURL identity), but it affects a smaller slice of components in typical Yocto builds (only packages with non-zero epoch — e.g., ~1 of 36 in the reporting user's test build). Ships alongside US1/US2 because the fix is scoped to the same reader and the reproducer set is identical. Filed as issue #552.

**Independent Test**: Emit an ipk fixture whose control file lists `Version: 1:2.0-r0` (or whose filename encodes `pkg_1:2.0-r0_all.ipk`), scan it with any format, assert the emitted PURL matches `pkg:opkg/pkg@2.0-r0?epoch=1` and the CDX `.components[].version` is `2.0-r0` (no leading `1:`).

**Acceptance Scenarios**:

1. **Given** an ipk with filename `netbase_1:6.4-r0_all.ipk` and control `Version: 1:6.4-r0`, **When** scanned with any format, **Then** the component's PURL contains `?epoch=1` and its `version` field is `6.4-r0`.
2. **Given** an ipk with no epoch (`Version: 2.0-r0`, no `<digits>:` prefix), **When** scanned, **Then** the PURL contains no `epoch=` qualifier (byte-identical to pre-fix behavior for the no-epoch path).
3. **Given** an ipk with `Version: 0:1.0-r0` (explicit zero-epoch), **When** scanned, **Then** the emitter treats it the same as no epoch (no `?epoch=0` qualifier, matching purl-spec convention where epoch=0 is the implicit default).
4. **Given** two ipk fixtures with identical name/version content but one encoded with inline epoch and one with the correct qualifier form, **When** both are scanned, **Then** they produce components with equal canonical PURLs.

---

### Edge Cases

- **Compound license with SPDX-with expression** (`License: GPL-2.0-only WITH Classpath-exception-2.0 & MIT`): Reader must preserve the `WITH` operand grouping while normalizing the `&`. Behavior mirrors what SPDX 2.3 already does today.
- **License field is empty or missing** (unusual for real Yocto builds but permitted by the ipk control format): All three emitters must handle absent license info format-idiomatically — SPDX 2.3 emits `licenseDeclared: "NOASSERTION"`, CDX 1.6 emits `licenses: []` (empty array), SPDX 3 omits the `simplelicensing_LicenseExpression` link on the `software_Package` element. No format silently drops the component itself. If the rpm reader currently diverges from this convention on any of the three formats, align it as part of this milestone (verify during planning).
- **Whitespace and quoting in license field** (e.g., `License: "GPL-2.0-only & MIT"` with literal quotes): The normalization pass must strip surrounding whitespace/quotes before parsing, matching current SPDX 2.3 behavior.
- **Epoch with non-standard characters** (`Version: abc:1.0-r0` where `abc` is not all digits): The epoch-detection regex must require `<digits>:` — anything else is treated as a literal version prefix and left in place (no false-positive epoch extraction). Emit a debug log noting the non-standard version shape.
- **Version with multiple colons** (`Version: 1:2.0-r0:beta`): Only the first `<digits>:` prefix is treated as epoch; the rest of the version string is preserved verbatim.
- **Existing goldens for no-epoch ipks**: Any ipk fixture without a `<digits>:` version prefix must produce byte-identical PURLs to pre-fix output. Epoch handling is additive-only for the affected packages.
- **Real-world Yocto scan of `core-image-minimal`**: Ships with a mix of epoch/non-epoch packages. All three fixes must land together so the same output validates cleanly for CDX, SPDX 2.3, and SPDX 3 across all packages.
- **Byte-identity of goldens for compound-license CDX fixtures**: Existing regression fixtures containing compound licenses (if any) will drift; those goldens must be regenerated as part of the milestone.

## Requirements *(mandatory)*

### Functional Requirements

**License normalization (US1 — CycloneDX side of #550)**:

- **FR-001**: System MUST normalize the raw `License:` field extracted from ipk control files by replacing BitBake operators — both single-character (`&`, `|`) and double-character (`&&`, `||`) forms, with any amount of surrounding whitespace — with their SPDX equivalents (`AND`, `OR`) before emitting into CycloneDX 1.6 `components[].licenses[].expression`.
- **FR-002**: System MUST introduce a preprocessing helper `normalize_bitbake_license_operators` in the ipk reader that runs BEFORE `SpdxExpression::try_canonical`. This helper is NEW in m190 — the pre-m190 SPDX 2.3 path does not normalize `&`/`|` either; it falls back to a `LicenseRef-<hex>` hashed form when canonicalization fails (per research §R1). Introducing the helper in the reader creates a single source of truth: once the raw string is normalized in the reader, ALL three emitters (CDX 1.6, SPDX 2.3, SPDX 3) transitively receive canonicalized license values through their existing shared `component.licenses` field. The helper performs string-level operator substitution (long-form `&&`/`||` matched before single `&`/`|` to avoid partial-token overlap), then hands off to `SpdxExpression::try_canonical` for validation. Parenthesization and `WITH`-clauses are preserved verbatim because only operator tokens change.
- **FR-003**: The output CycloneDX `licenses` block for an ipk with a compound license MUST parse as a valid SPDX expression via `SpdxExpression::try_canonical` (or the equivalent workspace validator).

**SPDX 3 license emission (US2 — #551)**:

- **FR-004**: System MUST emit SPDX 3 `simplelicensing_LicenseExpression` elements for every SPDX-legal license expression extracted from ipk control files.
- **FR-005**: System MUST emit SPDX 3 `simplelicensing_CustomLicense` elements for vendor-specific / non-SPDX license operands extracted from ipk control files, mirroring the m154 sweep already implemented for rpm packages.
- **FR-006**: Every `software_Package` element derived from an ipk MUST link to its license information via the existing SPDX 3 relationship pattern used elsewhere in the codebase (no new relationship type).
- **FR-007**: SPDX 3 documents produced from ipk fixtures MUST validate cleanly against `spdx3-validate==0.0.5` — the same conformance gate that already exists for the SPDX 3 rpm output path.

**Epoch handling (US3 — #552)**:

- **FR-008**: When parsing an ipk version string that matches `^<digits>:<rest>$` (where `<digits>` is one or more ASCII digits and `<rest>` is the remainder), System MUST strip the `<digits>:` prefix from the emitted `version` field.
- **FR-009**: When the version string has a non-zero-digit epoch prefix, System MUST emit a PURL qualifier of the form `?epoch=<digits>` on the resulting `pkg:opkg/...` PURL.
- **FR-010**: When the epoch value is zero (`0:...`), System MUST NOT emit an `epoch=` qualifier (matches purl-spec convention where `epoch=0` is implicit).
- **FR-011**: When the version string has no `<digits>:` prefix, System MUST emit byte-identical output to the pre-fix behavior — no `epoch=` qualifier, no version field change.
- **FR-012**: Epoch detection MUST work on both the ipk filename source (`pkg_<epoch>:<version>-<release>_<arch>.ipk`) and the control-file `Version:` field source; if both are present and disagree, the control-file value wins.

**Cross-cutting parity + regression control**:

- **FR-013**: For every ipk fixture in the test corpus, the CDX `.components[X].licenses[].expression`, the SPDX 2.3 `packages[X].licenseDeclared` (or extracted-licensing-info reference), and the SPDX 3 `simplelicensing_LicenseExpression` (or `CustomLicense`) value MUST all normalize to the same canonical SPDX expression string.
- **FR-014**: All existing golden fixtures MUST continue to pass byte-identity checks, except that goldens containing ipk components with compound licenses OR non-zero-epoch versions MAY be updated as a documented part of this milestone.
- **FR-015**: The milestone MUST NOT introduce new `mikebom:*` annotations for any of these three fixes — the standards-native SPDX/CDX license and PURL-qualifier fields carry all the needed information. (Per CLAUDE.md Principle V: standards-native fields take precedence over `mikebom:` properties.)

### Key Entities *(include if feature involves data)*

- **ipk control record**: The parsed representation of an `.ipk` package's control-file metadata (Package, Version, License, Description, etc.). Emerges from the ipk reader; consumed by all three format emitters. Key attributes: `name` (string), `version` (string — post-fix, has epoch stripped), `epoch` (optional non-zero unsigned integer — post-fix, new field), `license` (raw string, un-normalized).
- **Canonical license expression**: The SPDX-conformant form of a license field (`GPL-2.0-only AND MIT`, not `GPL-2.0-only & MIT`). Produced by a shared normalization helper; consumed by all three emitters. Represented internally as an `SpdxExpression` (the existing workspace newtype from `mikebom-common::types::license`).
- **Custom license element (SPDX 3)**: A `simplelicensing_CustomLicense` graph element representing a vendor-specific or otherwise non-SPDX-legal license operand. Matches the existing m154 emission pattern for rpm packages.
- **PURL epoch qualifier**: The `epoch=<digits>` name-value pair added to the query string of a `pkg:opkg/...` PURL when the source package has a non-zero epoch. Purely additive; never omitted mid-string once emitted.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: When scanning any Yocto-built directory of `.ipk` files with a compound BitBake license (`&` or `|` operators), 100% of the emitted CDX 1.6 `components[].licenses[].expression` values parse cleanly as SPDX expressions via `spdx::Expression::parse` (or the workspace equivalent). Zero raw `&` or `|` operators appear as SPDX operators in emitted CDX output.
- **SC-002**: When scanning the same directory with `--format spdx-3-json`, 100% of `software_Package` elements derived from ipks with non-empty license fields have an associated `simplelicensing_LicenseExpression` or `simplelicensing_CustomLicense` element in the graph. The count of license elements is greater than zero when the source has non-empty licenses.
- **SC-003**: For any ipk with a non-zero epoch prefix on its version (`<digits>:...`), the emitted PURL contains `?epoch=<digits>` and the `version` field contains no leading `<digits>:` prefix. Downstream canonical-PURL comparison treats the mikebom-emitted PURL as equal to a hand-formed reference PURL of the same package.
- **SC-004**: The SPDX 3 output produced from any ipk fixture in the test corpus validates against `spdx3-validate==0.0.5` with zero license-related conformance errors — matching the existing bar for rpm packages.
- **SC-005**: A single Yocto `core-image-minimal` real-world scan produces CDX, SPDX 2.3, and SPDX 3 outputs whose license expressions are semantically equivalent across all three formats for every component (verified by canonicalizing each format's license value and asserting set-equality per component).
- **SC-006**: For every ipk fixture in the test corpus without a `<digits>:` version prefix and without a compound license, the emitted PURL and license expression are byte-identical to pre-milestone output — no regression on the no-epoch, single-license path.

## Assumptions

- Real-world Yocto builds routinely emit compound licenses using BitBake's `&`/`|` operator syntax; the mikebom ipk reader already handles the SPDX 2.3 emission side correctly (per m152's LicenseRef work and m153's LicenseRef- conformance sweep), so this milestone is scoped to plugging the CDX and SPDX 3 gaps.
- Real-world Yocto builds occasionally include packages with non-zero epoch (Debian-derived recipes and security-team-bumped packages); a small percentage of components per typical build (e.g., 1-5%) will benefit from the epoch fix.
- The existing rpm reader's epoch-handling emits `?epoch=N` qualifier on its PURL and stores the naked version in `.version` — this milestone mirrors that behavior for opkg. Verify the rpm implementation as the reference before writing the ipk version-parser.
- No new Cargo dependencies are required — all three fixes reuse existing crates (`spdx`, `serde_json`, `regex` for the version-parse). Consistent with every recent ipk-reader milestone (m185, m187) and this repo's zero-new-deps posture.
- The purl-spec convention for opkg PURLs treats `?epoch=N` as a valid qualifier — verified against the purl-spec's ecosystem-list documentation for opkg/deb.
- Test fixtures needed: (a) synthetic ipks with `License: MIT`, `License: GPL-2.0-only & MIT`, `License: MIT | Apache-2.0`, and `License: SomeVendorLicense`; (b) synthetic ipks with `Version: 1:2.0-r0` and `Version: 2.0-r0`. Synthesized via the same technique used for the m185/m187 test fixtures — no need to check in a real Yocto-built ipk.
- Existing goldens: at least some CDX/SPDX regression fixtures may need to be regenerated if they contain compound-license ipks. Per `feedback_release_bump_regen_all_golden_tests`, use the "nuclear option" env-var regen if unsure which goldens are affected.
- `spdx3-validate==0.0.5` is already installed and CI-integrated per memory `reference_spdx3_validator`; the milestone adds new positive assertions but does not change the tool version.
- The three fixes ship as a single milestone because they all trace to the same reader and share test fixtures — bundled per the user's preference at triage time (option A: single m190 rather than m190+m191 split).
- Epoch fix is opkg-only for this milestone. dpkg (deb) and apk readers may harbor the same bug class; a follow-up audit ticket will be filed post-implementation to verify and, if needed, patch them independently. Doing so preserves visibility into the potential parity gap without doubling milestone size or risking byte-identity churn on unrelated goldens.
