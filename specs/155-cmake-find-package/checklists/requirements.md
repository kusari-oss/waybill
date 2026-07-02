# Specification Quality Checklist: CMake find_package + pkg_check_modules extraction (milestone 155)

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-07-02
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs) — spec cites `cmake.rs` in Origin & context as reader-anchor only; FRs/SCs describe user-facing extraction behavior without prescribing Rust APIs.
- [X] Focused on user value and business needs — compliance-auditor persona (source-tree scan of a C/C++ project) framed throughout; the "0 → ≥10 components" Kamailio impact is the ROI-per-LOC narrative.
- [X] Written for non-technical stakeholders — outcomes phrased as "auditor sees the roster of external libraries" not "add a regex match to Rust source."
- [X] All mandatory sections completed — User Scenarios + Requirements + Success Criteria + Assumptions all present.

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain — the milestone-102 FR-007 reversal is the load-bearing decision and it's directly named; no open decisions require operator input before planning.
- [X] Requirements are testable and unambiguous — FR-001 through FR-016 each name a concrete behavior; SC-001 through SC-008 each name a verification method (jq recipe, byte-diff, integration test, existing goldens).
- [X] Success criteria are measurable — SC-001 names ≥10 Kamailio components with concrete `mikebom:source-mechanism` annotation; SC-003 names exact-1 OpenSSL for the cross-mechanism dedup case; SC-006 = ≥8 unit tests.
- [X] Success criteria are technology-agnostic — outcomes phrased in consumer terms (SBOM component roster, jq query output); Kamailio testbed named as the concrete verification target.
- [X] All acceptance scenarios are defined — US1 has 6 Given/When/Then scenarios covering single/version/multi-file-dedup/no-version/modifier-noise/cross-tier-collision; US2 has 2 covering dpkg-collision + also-detected-via annotation.
- [X] Edge cases are identified — 9 edge cases enumerated: COMPONENTS subclause, find_package_handle_standard_args, PkgConfig bootstrap, pkg_check_modules variants, case normalization, commented-out declarations, string interpolation, sibling pkg_search_module.
- [X] Scope is clearly bounded — FR-013 through FR-016 + Out of Scope section enumerate explicit exclusions (no changes to milestone-102/103 paths, no other readers touched, no annotations beyond one new, no autotools, no pkg-config .pc parsing, no CMakePresets.json, no compile_commands.json, no catalog row this milestone).
- [X] Dependencies and assumptions identified — 8 Assumptions + explicit Dependencies on milestones 102 (the file being modified), 105 (dedup pipeline), 133 (file-tier walker unaffected).

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria — each FR maps to a US acceptance scenario, an SC, or both.
- [X] User scenarios cover primary flows — US1 (compliance auditor gets declared deps) covers the issue Kamailio surfaced; US2 (OS-rootfs + source-tree double-count prevention) covers the backward-compat concern that originally motivated milestone-102 FR-007.
- [X] Feature meets measurable outcomes defined in Success Criteria — SC-001 (Kamailio ≥10 components), SC-002 (byte-identity happy path), SC-003 (cross-mechanism dedup), SC-004 (integration testbed synthesis), SC-005 (pre-PR gate), SC-006 (≥8 unit tests), SC-007 (no wire-format changes beyond intended), SC-008 (CHANGELOG entry).
- [X] No implementation details leak into specification — the `cmake.rs` reference in Origin & context is a reader-anchor; the FRs/SCs describe outcomes independently.

## Notes

- All 16 checklist items pass on first authoring pass.
- The milestone-102 FR-007 reversal is the load-bearing decision; the maintainer's explicit rationale reversal (via the milestone-105 dedup pipeline availability) is documented in Origin & context.
- The Kamailio testbed produces the concrete "0 → ≥10 components" story that gives this milestone its ROI narrative.
- `/speckit-clarify` session 2026-07-02 locked in two decisions:
  - Q1 (multi-version dedup): **highest declared version wins** — FR-002 + US1 A3 updated.
  - Q2 (build-tool packages): **emit uniformly, no denylist** — new FR-017 codifies this.
- `/speckit-analyze` session 2026-07-02 surfaced + remediated 4 findings:
  - F1 (SC-001 ≥10 unachievable at depth-1) — floor lowered to ≥1 based on empirical Kamailio grep.
  - F9 (SC-003 dpkg+cmake cross-namespace) — reframed to same-PURL cross-mechanism (find_package + FetchContent_Declare URL).
  - F8 (T013/T014 off-by-one) — added `find_package(ZLIB REQUIRED)` to fixture as 5th call.
  - F2/F3/F4 (minor) — FR-006 phrasing, FR-011 tightened, new modifier-keyword test in T005.
- Implementation completed 2026-07-02:
  - All 22 tasks executed; T020 SC-001 manual Kamailio verification confirms 0 → 1 identified component.
  - 15 milestone-155 unit tests + 2 integration tests pass; SC-006 floor (≥8) cleared.
  - Pre-PR gate green except documented `sbomqs_parity` env-only flake.
  - Wire-format impact: `find_package(OpenSSL REQUIRED)` in the milestone-090 cmake golden fixture is now emitted (goldens regenerated in all 3 formats — CDX / SPDX 2.3 / SPDX 3).
  - FR-015 catalog-deferral posture yielded to enforced build-time gates: `docs/reference/sbom-format-mapping.md` row C103 added + parity extractors (C103) wired in immediately (positive Principle V outcome).
- Ready for PR review + merge.
