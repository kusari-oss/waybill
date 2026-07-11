# Specification Quality Checklist: ipk reader bug fixes

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-07-11
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs) — spec references `ipk_file.rs:609`, `ipk_file.rs:557`, `opkg.rs:289`, and `ipk_file.rs:455` as file-path anchors so a planner can find the affected code, but user-story text stays at the "operator wants" level (Yocto scan produces null PURLs and missing licenses). Constitution Alignment cites internal patterns (`rsplitn`, license-normalization pipeline) as design signals, not prescriptions.
- [X] Focused on user value and business needs — two P1 user stories each pin a distinct correctness bug against a concrete measurable impact (9 broken components in stock Yocto for US1; 4586 zero-license components for US2). Both bugs are reported against alpha.58 with reproducible test artifacts.
- [X] Written for non-technical stakeholders — reader can follow "Yocto kernel modules emit as null-PURL broken components today; ipk-based license data is entirely missing" without opening code.
- [X] All mandatory sections completed — User Scenarios (2 stories + 10 edge cases), Requirements (13 FRs), Success Criteria (9 SCs), Assumptions, Constitution Alignment, Deferred to Future Milestones all present.

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain — every design decision documented in Assumptions with cross-references to the issue reports (#538, #539) and to the rpm reader's existing normalization pipeline (post-#475/#481/#485/#487) as the pre-hardened path to reuse.
- [X] Requirements are testable and unambiguous — FR-001 pins the exact `rsplitn(3, '_')` semantic; FR-002/FR-003/FR-004 pin the three case-split outputs; FR-005/FR-006 pin the normalization pipeline reuse; FR-007 pins the absent-License regression; FR-008 pins the archive-format non-modification; FR-009 pins the byte-identity regression guard; FR-012 pins zero-new-dep.
- [X] Success criteria are measurable — SC-001 is a per-fixture equivalence gate; SC-002 pins the concrete null-PURL count (9 → 0); SC-003 is a specific stanza-to-CDX shape gate; SC-004 uses an aggregate 80% threshold (avoids per-package coupling to Yocto's specific normalization output); SC-005/SC-006 pin byte-identity on non-Yocto goldens; SC-007 is the cross-format parity gate; SC-008 is test-continuity; SC-009 is `cargo tree` line-count invariant.
- [X] Success criteria are technology-agnostic — reference operator-visible behaviors (null-PURL counts, license-coverage percentages, byte-identity across formats) rather than specific parsing internals.
- [X] All acceptance scenarios are defined — US1 has 4 scenarios (multi-underscore happy path, well-formed regression, malformed regression, real BitBake kernel-module shape); US2 has 4 scenarios (SPDX-canonical operand, mixed `&` + LicenseRef operand, absent-License regression, `hasExtractedLicensingInfos` cross-format sweep); every scenario uses GIVEN/WHEN/THEN.
- [X] Edge cases are identified — 10 cases covering `.ipk`-extension-missing, well-formed 3-underscore, malformed-with-underscores, archive-format-unchanged, already-SPDX License, multi-operator License, whitespace-only License, per-stanza classification, filename-fallback-plus-no-License, non-Yocto byte-identity.
- [X] Scope is clearly bounded — Deferred section explicitly lists the 5 non-kernel "other affected packages" from #538, legacy ar-format license extraction, and Yocto SPDX 2.2 rollup comparison as OUT of m185 scope.
- [X] Dependencies and assumptions identified — 7 assumptions covering ipk-spec filename convention, opkg stanza parser existing extraction, rpm normalization pipeline genericity, rpm-side non-modification invariant, expected regen shape, DISTINCT 5-package pattern deferral, definitive 4-kernel-module pattern targeting.

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria — FR-001/002/003/004 ⇔ US1 acceptance 1-4 + SC-001/002; FR-005/006 ⇔ US2 acceptance 1-2 + SC-003/004; FR-007 ⇔ US2 acceptance 3 (regression pin); FR-008 ⇔ Edge Cases archive-format-unchanged; FR-009 ⇔ SC-005/006 byte-identity guards; FR-010/011 ⇔ SC-008 test continuity; FR-012 ⇔ SC-009 zero-new-dep; FR-013 ⇔ Constitution Alignment X transparency pin.
- [X] User scenarios cover primary flows — two distinct correctness-bug flows: filename fallback misparse (US1, hits every Yocto operator with kernel modules) + wholesale license absence (US2, hits every Yocto operator with opkg-installed packages). Combined they close both #538 and #539 in one milestone.
- [X] Feature meets measurable outcomes defined in Success Criteria — SC-001/002 are the two individual US1 gap-fix gates; SC-003/004 are the two individual US2 gap-fix gates; SC-005/006 are byte-identity guards; SC-007 is C122-adjacent cross-format parity; SC-008 is test-continuity; SC-009 is zero-new-dep.
- [X] No implementation details leak into specification — spec names internal functions (`parse_ipk_filename`, `filename_fallback_entry`) and file-path anchors as design signals but frames them as planning-phase surfaces, not user-facing prescriptions.

## Notes

- **All 16 checklist items PASS as of 2026-07-11**. Ready for `/speckit-plan`.
- Delivery cadence: 2 P1 correctness-bug fixes in one milestone. Both surfaced by the same yocto-test testbed run; both are follow-ups to m169's ipk-reader landing (`31b3cfa`). Single-PR bundle per the m179+ optional-dep milestone precedent.
- **US1 (filename fallback) is a TARGETED PARSER FIX** — small surface: swap `split('_')` for `rsplitn(3, '_')` at one call site, adjust the empty/malformed-guards to match. Zero new state.
- **US2 (license extraction) is a WIRING FIX** — the opkg reader already parses the License field text; m185 wires it through the rpm reader's existing normalization pipeline. Whether the pipeline needs a shared-helper refactor to be callable from opkg.rs is a planning-phase decision (Decision candidate).
- **Distinct scope from #538's "5 other affected packages"**: the 5 non-kernel packages listed in the issue are well-formed 2-underscore filenames that SHOULD parse under m169. If they emit as broken, the root cause is a distinct bug — investigation deferred to a follow-up per Assumptions section. m185 tightly scopes to the 4-kernel-module pattern.
- **rpm reader path is off-limits** — FR-011 non-modification invariant. rpm goldens MUST stay byte-identical.
- No clarifications needed. The two issue reports include reproduction steps and expected-vs-actual output, resolving every "how should this behave?" question.
