# Feature Specification: ipk reader bug fixes (filename fallback + license extraction)

**Feature Branch**: `185-ipk-reader-fixes`
**Created**: 2026-07-11
**Status**: Draft
**Input**: User description: "m185 — two ipk-family correctness bug fixes surfaced by the yocto-test testbed's feature `003-ipk-package-format` rerun against alpha.58: (a) `parse_ipk_filename` at `ipk_file.rs:609` uses `split('_')` with a strict `len() != 3` guard that misparses ipk filenames whose version field contains an embedded underscore (multi-underscore pattern from BitBake's `SRCPV` expansion — hits every Yocto kernel module in a stock `core-image-minimal` build); (b) the opkg installed-package reader at `opkg.rs:289` emits `licenses: Vec::new()` on every emitted component, ignoring the `License:` field it already parses out of the stanza — producing `licenses: []` / `licenseDeclared: NOASSERTION` on all 4586 components in a stock Yocto `core-image-minimal` scan. Both bugs are follow-ups to milestone 169 (ipk reader landing, PR merged as `31b3cfa`) filed as issues #538 and #539."

## Clarifications

### Session 2026-07-11

- Q: How should mikebom handle opkg License strings that fail SPDX-expression parsing entirely (syntactically broken operators, garbage tokens, non-SPDX operators, etc.)? → A: Wholesale-wrap the entire string as a single `LicenseRef-<sanitized>` operand — matches the rpm reader's per-#481 fail-safe wholesale-wrap behavior. Preserves the raw string for downstream review, unparseable strings count toward SC-004's 80% coverage threshold.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — ipk filename fallback correctly parses versions containing underscore (Priority: P1)

The ipk filename convention per `ipk-spec` is `<name>_<version>_<arch>.ipk` — three fields separated by `_`. The version field itself is OPAQUE and legally contains any characters other than the outermost separator context. BitBake's `SRCPV` expansion for git-sourced upstream recipes emits versions like `6.6.127+git0+45f69741c7_70af2998be-r0` — the version contains an underscore. mikebom's current `parse_ipk_filename` at `ipk_file.rs:609` splits the stem on `_` using `split('_')` and rejects anything without exactly 3 parts, so multi-underscore versions fail parsing entirely.

Today, on a stock Yocto `core-image-minimal` build for `qemux86-64` (scarthgap release), 9 of the 36 installed packages exhibit this pattern — 4 kernel modules with the definitive git-sourced version shape, plus 5 others in a related pattern. All 9 emit as broken components: `name` contains the full basename (including `.ipk` extension), `version` is empty string, `purl` is null. Downstream tooling can't identify these components at all.

**Why this priority**: Every stock Yocto image that includes a kernel with a git-sourced upstream produces reproducible failures. This is the default for `poky` on any release. The 9 affected packages are all installed on the rootfs — this isn't a rare edge case, it hits every Yocto operator scanning post-alpha.52 (when m169 ipk reader landed).

**Independent Test**: Create a fake ipk with a multi-underscore version filename (e.g., `test-pkg_1.0+git0+abc_def-r0_all.ipk`), scan the containing directory, verify (a) emitted component has `name = "test-pkg"`, `version = "1.0+git0+abc_def-r0"`, `arch = "all"`, (b) emitted `purl` is `pkg:opkg/test-pkg@1.0%2Bgit0%2Babc_def-r0?arch=all` (or equivalent with `distro=` qualifier if configured), (c) the component is NOT null-`purl` and is NOT the basename-as-name shape.

**Acceptance Scenarios**:

1. **Given** a `.ipk` filename `test-pkg_1.0+git0+abc_def-r0_all.ipk` in the scanned rootfs, **When** mikebom's ipk filename-fallback path fires, **Then** the emitted component MUST have `name = "test-pkg"`, `version = "1.0+git0+abc_def-r0"`, `arch = "all"`, and `purl = "pkg:opkg/test-pkg@1.0%2Bgit0%2Babc_def-r0?arch=all"`.
2. **Given** the same filename with a canonical `<name>_<version>_<arch>.ipk` shape but WITHOUT underscore in the version (e.g., `packagegroup-core-boot_1.0-r0_all.ipk`), **When** mikebom parses the filename, **Then** the parse behavior MUST be byte-identical to pre-m185 — `name = "packagegroup-core-boot"`, `version = "1.0-r0"`, `arch = "all"` (regression pin: the fix does NOT change the well-formed-filename path).
3. **Given** a filename that does NOT match the `<name>_<version>_<arch>.ipk` shape (e.g., missing arch, extra field beyond 3 non-version underscores, or the `.ipk` extension is absent), **When** mikebom parses it, **Then** the parser MUST continue to return None (fail-safe: only the underscore-in-version case is fixed).
4. **Given** a real Yocto kernel-module ipk with the observed BitBake `SRCPV` shape (`kernel-module-nf-conntrack-tftp-6.6.127-yocto-standard_6.6.127+git0+45f69741c7_70af2998be-r0_qemux86_64.ipk`), **When** mikebom scans, **Then** the emitted component MUST NOT have `name` containing the full basename, MUST NOT have empty `version`, and MUST NOT emit `purl = null`.

---

### User Story 2 — opkg installed-package reader extracts License field from stanzas (Priority: P1)

The opkg / dpkg-family stanza format used by the opkg installed-package database (`/var/lib/opkg/status`, `/usr/lib/opkg/status`, and similar paths on Yocto and OpenWrt) includes a `License:` field mirroring the recipe `LICENSE` variable. mikebom's opkg installed-package reader at `opkg.rs` already reads the stanza and extracts `Package`, `Version`, `Depends`, `Maintainer`, etc. — but at line 289 it emits `licenses: Vec::new()` on every component, ignoring the `License:` field entirely.

The consequence is measurable: every downstream license-audit workflow on ipk-based Yocto builds gets zero coverage. On a stock `core-image-minimal` scan (4586 emitted components), 0 components carry any license identifier. All show `licenseDeclared: "NOASSERTION"` in SPDX 2.3 output. Yocto's own SPDX 2.2 rollup for the same packages correctly emits `licenseDeclared: "GPL-2.0-only"`, `GPL-2.0-or-later AND LGPL-2.1-or-later`, `Apache-2.0`, etc. — the data is available in every installed opkg record, mikebom just isn't reading it.

**Why this priority**: Zero-coverage license absence blocks license-audit workflows entirely. The user-visible impact (`0 of 4586 components have licenses`) is wholesale, not partial. The rpm reader has already solved this exact class of problem via a chain of hardening fixes (#470, #475, #481, #485, #487) — the ipk / opkg readers should route through the same normalization codepath so the hardening applies for free.

**Independent Test**: Scan a Yocto rootfs (or a synthetic opkg-status fixture) containing a package whose stanza declares `License: GPLv2 & bzip2-1.0.4`. Verify (a) emitted component's `licenses` array contains the normalized SPDX expression `GPL-2.0-only AND LicenseRef-bzip2-1.0.4` (per the rpm reader's normalization chain: `&` → `AND`, unknown operand → `LicenseRef-`), (b) CDX 1.6 emits the two-operand `licenses[]` structure with the correct `acknowledgement: "declared"` marker, (c) SPDX 2.3 emits `licenseDeclared` with the same normalized expression + populates `hasExtractedLicensingInfos` for the `LicenseRef-bzip2-1.0.4` operand.

**Acceptance Scenarios**:

1. **Given** an opkg stanza with `License: GPL-2.0-only`, **When** mikebom emits the component, **Then** the CDX 1.6 `licenses[]` MUST contain `{ "license": { "acknowledgement": "declared", "id": "GPL-2.0-only" } }`.
2. **Given** an opkg stanza with `License: GPLv2 & bzip2-1.0.4` (Yocto's raw recipe-style), **When** mikebom normalizes, **Then** the emitted CDX 1.6 `licenses[]` MUST contain the two-operand structure: `id: "GPL-2.0-only"` for the known operand and `name: "LicenseRef-bzip2-1.0.4"` for the unknown operand (mirrors the m152 escape hatch path the rpm reader gained via #481).
3. **Given** an opkg stanza with NO `License:` field OR `License: NOASSERTION` explicit, **When** mikebom emits the component, **Then** the emitted `licenses[]` is empty and SPDX 2.3 falls through to `licenseDeclared: "NOASSERTION"` — same as pre-m185 for the missing-License case (regression pin: absent-License handling unchanged).
4. **Given** the same Yocto scan under `--spdx2-relationship-compat=full` (default), **When** mikebom emits SPDX 2.3, **Then** the `hasExtractedLicensingInfos` array MUST include every `LicenseRef-<sanitized>` produced by the m152 escape hatch across all opkg-emitted components (m185 flows through the same #485 / #487 sweep the rpm reader uses).

---

### Edge Cases

- **ipk filename with `.ipk` extension MISSING**: unchanged behavior. `parse_ipk_filename` continues to require the `.ipk` suffix and returns None otherwise.
- **ipk filename with well-formed 3-underscore shape**: unchanged behavior. The pre-m185 canonical parse still fires for `<name>_<version>_<arch>.ipk` without underscores in the version field.
- **ipk filename with 4+ underscores but arch or version empty**: continues to return None (fail-safe: the fix targets only the case where the outer `rsplitn(3, '_')` split can extract non-empty arch AND version AND name).
- **ipk archive-format reader path (`.ipk` archive contents readable)**: unchanged behavior. m185 US1 only affects the FILENAME FALLBACK path (per `ipk_file.rs:557`'s `filename_fallback_entry`); the archive-format extraction path at `ipk_file.rs:455` already reads License from the control stanza.
- **opkg-status stanza with License containing `AND`/`OR` (already SPDX-shaped)**: mikebom passes through unchanged (the normalization chain is a no-op on already-canonical SPDX expressions).
- **opkg-status stanza with License containing multiple `&`/`|` operators**: recursively normalized. `GPLv2 & LGPL-2.1 & MIT` → `GPL-2.0-only AND LGPL-2.1-only AND MIT`.
- **opkg-status stanza with License containing whitespace-only value**: treated as absent (emit no license) per the rpm reader's precedent.
- **opkg-status stanza with License string that fails SPDX parsing entirely** (per FR-014's clarification): the WHOLE original string is wholesale-wrapped as `LicenseRef-<sanitized>` — matches the rpm reader's per-#481 escape hatch. The emitted component has ONE `licenses[]` entry (the wholesale-wrapped LicenseRef) rather than empty. Auditors grep for the sanitized form to recover the original raw value. Contrast with the whitespace-only case above (treated as absent).
- **opkg-status stanza with License field on ONE package but not another in the same file**: per-package classification — each stanza is independent.
- **Filename-fallback + no License field in stanza**: expected behavior. The filename fallback fires only when archive extraction fails; in that case, no stanza is available to read License from, so the emitted component correctly has `licenses: []`. This is legitimate NOASSERTION and MUST be preserved (not a bug).
- **rpm-side license goldens (unchanged)**: FR-011 non-Yocto regression pin. Every non-opkg / non-Yocto CDX + SPDX 2.3 + SPDX 3 golden MUST be byte-identical to pre-m185.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: `parse_ipk_filename` at `mikebom-cli/src/scan_fs/package_db/ipk_file.rs:609` MUST switch from left-to-right `split('_')` with a strict `len() != 3` guard to right-to-left splitting via `rsplitn(3, '_')`. The split MUST extract: `arch` (rightmost `_`-delimited field), `version` (middle field between rightmost and second-rightmost `_`), and `name` (all remaining characters left of the second-rightmost `_`).
- **FR-002**: The new parser MUST accept multi-underscore version fields. For a filename `<name>_<version-part-a>_<version-part-b>_<arch>.ipk` where the underscore-inside-version is the middle underscore, the parser MUST emit `name` unchanged, `version = <version-part-a>_<version-part-b>`, and `arch = <arch>`.
- **FR-003**: The new parser MUST continue to return None when the stem has fewer than 2 underscores (can't extract 3 fields) OR when any of the 3 extracted fields is empty after trimming.
- **FR-004**: The new parser MUST continue to accept well-formed 2-underscore filenames byte-identically to pre-m185 — the fix is additive on the multi-underscore path only.
- **FR-005**: The opkg installed-package reader at `mikebom-cli/src/scan_fs/package_db/opkg.rs:289` MUST extract the `License:` field from each parsed stanza and populate the emitted component's `licenses` field with the normalized value. The `licenses: Vec::new()` placeholder MUST be replaced.
- **FR-006**: The normalization pipeline used by the opkg reader MUST match the rpm reader's proven codepath (post-#475/#481/#485/#487). Specifically: (a) `&` → `AND`, (b) `|` → `OR`, (c) unknown operands wrapped as `LicenseRef-<sanitized>` per m152 SPDX 2.3 escape hatch, (d) `hasExtractedLicensingInfos` sweep populated per #485 for SPDX 2.3 and per #487 for SPDX 3.
- **FR-007**: When the `License:` field is ABSENT from an opkg stanza (or contains whitespace-only value), the emitted component MUST have `licenses: Vec::new()` and SPDX 2.3 MUST emit `licenseDeclared: "NOASSERTION"` — regression pin on the absent-License path (matches pre-m185 behavior).
- **FR-008**: The ipk archive-format reader path at `ipk_file.rs:455` (which ALREADY reads License from control stanza) MUST NOT be modified. m185 US2 scope is the OPKG INSTALLED-PACKAGE READER (`opkg.rs`), not the ipk-file archive path.
- **FR-009**: For scans that do NOT exercise the new signals (non-ipk / non-opkg fixtures + ipk fixtures with only well-formed 2-underscore filenames + opkg fixtures without License fields), the emitted CDX 1.6, SPDX 2.3, and SPDX 3.0.1 documents MUST be byte-identical to the pre-m185 baseline (regression guard).
- **FR-010**: The existing ipk regression tests (m169 US1–US6 in `ipk_file.rs::tests`) MUST continue to pass, allowing for additive changes only on tests that specifically exercise multi-underscore filenames (which pre-m185 returned None for).
- **FR-011**: The existing opkg regression tests (m107 test cases in `opkg.rs::tests`) MUST continue to pass, allowing for additive changes on any test whose fixture stanza carries a License field.
- **FR-012**: The m185 changes MUST NOT introduce any new Cargo dependency. The `rsplitn` splitting is stdlib; the license normalization chain reuses existing pipeline from rpm_file.rs.
- **FR-013**: The `mikebom:source-mechanism = "ipk-file-filename-fallback"` annotation emitted by `filename_fallback_entry` MUST continue to appear byte-identically for filename-fallback components — the annotation content is unchanged; only the parsed name/version/arch triple changes when the input has an underscore-in-version.
- **FR-014**: When the opkg License string cannot be parsed as a valid SPDX expression (per the rpm reader's post-#481 escape-hatch definition: syntactically broken operators, garbage tokens, non-SPDX operators like `+` outside a version, empty operand slots, etc.), mikebom MUST wrap the WHOLE original string as a single `LicenseRef-<sanitized>` operand — matching the rpm reader's wholesale-wrap fail-safe. The sanitization rule mirrors the m152 escape hatch (replace non-`[A-Za-z0-9.-]` characters with `-`; prefix with `LicenseRef-`). Preserves the raw string for downstream license auditors while producing a single valid SPDX-2.3 operand. The wholesale-wrapped component IS counted toward SC-004's 80% coverage threshold — it carries at least one identifier, unlike the pre-m185 `licenses: []` shape.

### Key Entities

- **`parse_ipk_filename`** (existing function at `ipk_file.rs:609`): input is `filename: &str`; output is `Option<(String, String, String)>` (name/version/arch triple). Under m185, the internal implementation switches from left-to-right `split('_')` to right-to-left `rsplitn(3, '_')`.
- **`filename_fallback_entry`** (existing function at `ipk_file.rs:557`): unchanged. Consumes the `parse_ipk_filename` output.
- **opkg stanza `License:` field**: existing wire-format field in the opkg installed-package database. String value; typically a raw SPDX expression or an operator-shaped multi-license expression using `&`/`|` operators.
- **`opkg::read_stanza` (or equivalent)**: the parser that already extracts the License field text. m185 wires the normalized value into the emitted component's `licenses` array.
- **Shared license-normalization pipeline**: the codepath used by rpm_file.rs (post-#475/#481/#485/#487). m185 US2 routes opkg license text through this same pipeline; no new normalization logic is invented.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For a scan of the reproducer fixture (a single `.ipk` file with filename `test-pkg_1.0+git0+abc_def-r0_all.ipk`), the emitted CDX 1.6 component MUST have `purl = "pkg:opkg/test-pkg@1.0%2Bgit0%2Babc_def-r0?arch=all"`, `name = "test-pkg"`, `version = "1.0+git0+abc_def-r0"`. Pre-m185 baseline: `purl = null`, `name = full basename`, `version = ""`.
- **SC-002**: For a scan of a stock Yocto `core-image-minimal` rootfs (or synthesized fixture mimicking the 4 kernel-module ipk shapes), the count of `purl = null` components MUST drop from 9 (per the issue report against alpha.58) to 0. Every previously-broken filename-fallback emission MUST now emit a valid `pkg:opkg/...` PURL.
- **SC-003**: For a scan of an opkg-status fixture containing a stanza `Package: busybox\nVersion: 1.36.1-r0\nLicense: GPL-2.0-only\n`, the emitted CDX 1.6 component MUST include `licenses: [{ "license": { "acknowledgement": "declared", "id": "GPL-2.0-only" } }]`. Pre-m185 baseline: `licenses: []`.
- **SC-004**: For a scan of a stock Yocto `core-image-minimal` rootfs (or comparable synthesized fixture), the count of components carrying at least one license identifier MUST rise from 0 (per the issue report against alpha.58) to a significant majority — specifically at least 80% of installed-opkg-package components MUST emit at least one `licenses[]` entry. The remaining <20% accounts for stanzas legitimately missing a License field OR whitespace-only License fields (both correctly emit `licenses: []` per FR-007). Unparseable License strings do NOT reduce the 80% coverage — per FR-014 they wholesale-wrap as a single `LicenseRef-<sanitized>` operand, which DOES carry a license identifier and DOES count toward the threshold.
- **SC-005**: Zero drift in any mikebom CDX 1.6 golden file that does not exercise ipk / opkg fixtures (regression guard on non-Yocto ecosystems — same shape as the m180 SC-003 / m181 SC-004 / m183 SC-005 / m184 SC-004 regression-pattern).
- **SC-006**: Zero drift in any mikebom SPDX 3.0.1 golden file that does not exercise ipk / opkg fixtures.
- **SC-007**: The `licenses[]` array + `hasExtractedLicensingInfos` entries + SPDX 3 `simplelicensing_CustomLicense` entries emitted for m185-classified opkg components MUST appear byte-identically across CDX 1.6, SPDX 2.3, and SPDX 3.0.1 (parity gate — inherited from the rpm reader's existing normalization pipeline).
- **SC-008**: Existing ipk regression tests (m169 US1–US6) AND existing opkg regression tests (m107) MUST continue to pass, allowing for additive changes on tests that specifically exercise the m185 signals (multi-underscore filename parsing OR License-field extraction).
- **SC-009**: Zero new production Cargo dependencies added to `mikebom-cli/Cargo.toml` — `cargo tree -p mikebom | wc -l` MUST be identical pre- vs post-m185.

## Assumptions

- The ipk filename convention `<name>_<version>_<arch>.ipk` is authoritative per the ipk-spec (Debian-derived, adopted by opkg). Version fields legally contain any character other than `_` in the canonical case, but in practice contain `_` when produced by BitBake's `SRCPV` expansion for git-sourced upstream builds. m185 explicitly supports the observed BitBake shape.
- The opkg installed-package reader's stanza parser already extracts the `License:` field into an intermediate representation. m185 does NOT add stanza-parsing logic — it only wires the extracted License value into the emitted component's `licenses` field via the existing rpm-side normalization pipeline.
- The license-normalization pipeline used by rpm_file.rs (post-#475/#481/#485/#487) is generic — it accepts any SPDX-style expression string and normalizes it. Reusing it for opkg is a code-locality decision (extract the shared codepath into a helper if not already extracted; otherwise call it in-place).
- The rpm reader path is NOT affected by m185 — its behavior remains byte-identical. FR-011 verification pins this.
- Golden fixture regeneration (`MIKEBOM_UPDATE_{CDX,SPDX,SPDX3}_GOLDENS=1`) will show additive changes ONLY on the fixture that exercises multi-underscore filenames (US1) AND additive changes on any opkg fixture that carries License-field data (US2). All other fixtures MUST show zero drift per SC-005/SC-006.
- The 5 "other affected packages" mentioned in issue #538 (`base-files_3.0.14-r89_all.ipk`, `init-ifupdown_1.0-r7_all.ipk`, `packagegroup-core-boot_1.0-r0_all.ipk`, `sysvinit-inittab_2.88dsf-r10_all.ipk`, `v86d_0.1.10-r0_core2-64.ipk`) are well-formed 2-underscore filenames and should ALREADY parse correctly under m169. If they emit as broken components in a stock Yocto scan, the root cause is a DISTINCT bug (not the filename-fallback issue); m185 does NOT scope those cases. Investigation of that separate pattern is deferred to a follow-up milestone.
- The 4 kernel-module packages mentioned in issue #538 are the definitive multi-underscore pattern; m185 US1 solely targets that class.

## Constitution Alignment

**Principle III (Fail Closed)**: FR-003 preserves the fail-safe None-return for genuinely malformed filenames. The fix only unlocks the previously-rejected multi-underscore case; every other rejection path is preserved byte-identically.

**Principle IX (Accuracy)**: SC-001/SC-002 close the wholesale-misclassification gap for Yocto kernel modules. SC-003/SC-004 close the wholesale-absence gap for ipk-based license data. Both address correctness bugs where mikebom was emitting demonstrably-wrong data (null PURLs, empty licenses) against a well-understood ground-truth.

**Principle X (Transparency)**: FR-013 preserves the existing `mikebom:source-mechanism = "ipk-file-filename-fallback"` annotation so operators can still audit which components came from the filename-fallback vs archive-extraction paths.

**Principle I (Pure Rust, Zero C)**: FR-012 verified via SC-009 — no new Cargo deps. The `rsplitn` splitting is stdlib; the license normalization pipeline is existing pure-Rust code.

## Deferred to Future Milestones

- **The 5 "other affected packages"** from issue #538 (non-kernel packages with well-formed 2-underscore filenames that reportedly emit as broken components): root cause is DISTINCT from the m185 filename-fallback fix. Investigation deferred until reproducible standalone repro is available.
- **Legacy ar-format .ipk (pre-2015 opkg-build) license extraction**: legitimate NOASSERTION per the current filename-fallback path (no stanza available to read License from). m185 does NOT change this. If a follow-up milestone adds ar-format archive-content extraction to the fallback path, license extraction would land alongside.
- **Yocto SPDX 2.2 rollup comparison verification**: the issue report cites Yocto's own SPDX output as the ground-truth for expected license values. m185's success-criteria SC-004 uses an aggregate-percentage measure rather than package-by-package equivalence to avoid coupling mikebom's normalization pipeline to Yocto's specific output shape. A follow-up milestone MAY add a Yocto-SPDX-comparison harness once the m152/m165/m168 audit-harness family matures further.
