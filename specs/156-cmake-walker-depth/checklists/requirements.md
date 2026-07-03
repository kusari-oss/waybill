# Specification Quality Checklist: CMake walker depth extension (milestone 156)

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-07-02
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs) — spec cites `cmake.rs` in Origin & context + FR-001 as a file anchor only; FRs describe user-facing extraction behavior. Implementation-level details (whether to reuse `safe_walk` directly or replicate the pattern, `read_dir` vs walkdir crate, etc.) are Assumptions §7 hints, not prescriptions.
- [X] Focused on user value and business needs — the compliance-auditor Kamailio flow drives US1 (0 → 1 → ≥10 identified components is the narrative). US2 is defensive (no regressions).
- [X] Written for non-technical stakeholders — outcomes phrased as "auditor sees full declared-dep roster" not "add recursive read_dir to Rust source."
- [X] All mandatory sections completed — User Scenarios + Requirements + Success Criteria + Assumptions all present.

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain — the milestone-155 F1 remediation established the exact scope + reasonable defaults for every open question (depth cap: unbounded with visited-set; broader-tree walking: OUT of scope; build-tree exclusion: operator-managed via existing `--exclude-path`; symlink handling: milestone-054 pattern).
- [X] Requirements are testable and unambiguous — FR-001 through FR-018 each name a concrete behavior; SC-001 through SC-010 each name a verification method (Kamailio manual scan, byte-identity golden diff, integration tests for symlink cycle / depth-3 / exclude-path / cross-depth version consolidation).
- [X] Success criteria are measurable — SC-001 sets ≥10 Kamailio components with named PURLs; SC-002 = byte-identity vs post-155 goldens; SC-008 = ≥6 unit tests.
- [X] Success criteria are technology-agnostic — outcomes phrased in SBOM-consumer terms; Kamailio testbed named as the concrete verification target.
- [X] All acceptance scenarios are defined — US1 has 5 Given/When/Then scenarios covering depth-2 / arbitrary-depth / cross-depth version consolidation / symlink cycle / exclude-path; US2 has 2 covering byte-identity + FR-009 boundary at depth-2.
- [X] Edge cases are identified — 8 edge cases enumerated: symlink cycles, out-of-root symlinks, build-tree contamination, vendored-with-own-find_package, extreme depth, case-sensitivity, `.cmake`-suffix-in-build-dir noise, `Find<Name>.cmake` semantics.
- [X] Scope is clearly bounded — FR-017 + FR-018 + Out of Scope section enumerate explicit exclusions (`src/**/CMakeLists.txt` NOT walked, no add_subdirectory following, no include() resolution, no auto-exclude of build/, no CMake variable evaluation).
- [X] Dependencies and assumptions identified — 8 Assumptions + explicit Dependencies on milestones 155 (emission code path), 102/103 (helper being modified), 054 (safe_walk pattern), 113 (exclude-path flag).

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria — each FR maps to a US acceptance scenario, an SC, or both.
- [X] User scenarios cover primary flows — US1 (compliance auditor gets full Kamailio roster) covers the F1 remediation debt; US2 (existing depth-1 emissions unchanged) covers the backward-compatibility guarantee.
- [X] Feature meets measurable outcomes defined in Success Criteria — SC-001 (Kamailio ≥10), SC-002 (byte-identity), SC-003 (symlink safety), SC-004 (depth-3 emission), SC-005 (cross-depth version consolidation), SC-006 (exclude-path integration), SC-007 (pre-PR gate), SC-008 (≥6 tests), SC-009 (CHANGELOG), SC-010 (no wire-format changes).
- [X] No implementation details leak into specification — the `cmake.rs:195` reference in FR-001 is a reader-anchor; the FRs/SCs describe outcomes independently.

## Notes

- All 16 checklist items pass on first authoring pass.
- The milestone-155 F1 remediation is the direct predecessor; the ≥10 Kamailio floor SC-001 sets IS the whole-tree count the milestone-155 origin story anticipated but couldn't hit due to walker-scope.
- The reused milestone-155 Kamailio-shape fixture at `mikebom-cli/tests/fixtures/cmake-find-package/kamailio-shape/` already contains a depth-2 `FindLibev.cmake` file with a `find_package_handle_standard_args` call — post-milestone-156 this file becomes discoverable and the FR-009 boundary verification at depth-2 comes for free (no new fixture needed for that specific case).
- `/speckit-clarify` session 2026-07-02 locked in one decision:
  - Q1 (`third_party/` recursive walking): **Depth-1 default; opt-in flag `--cmake-third-party-recursive` for full recursion**. FR-001 refined to name only `cmake/` + `Modules/` as recursive-by-default; new FR-019 + FR-020 codify the CLI flag. Assumption 5 + Edge Cases updated. New SC-011 verifies flag on/off behavior. SC-010 diff-file-list expanded to include the CLI arg-struct source.
- Ready for `/speckit-plan`.
