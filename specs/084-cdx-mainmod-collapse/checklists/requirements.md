# Specification Quality Checklist: CDX 1.6 main-module super-root collapse

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-05-08
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs) — *spec references exact file paths + line numbers in "Why this matters — root cause" section, and the CDX 1.6 schema is the contract; both are appropriate context for a bug-fix milestone targeting a specific identifier mismatch and are not implementation prescription. Code-level fix shape is in plan.md, not here.*
- [X] Focused on user value and business needs — *opens with the operator-pain framing (strict consumers fail), continues with semantic-impact-on-consumers table*
- [X] Written for non-technical stakeholders — *the contract violation is explained verbatim from the spec, with downstream-consumer impact described in operator terms (validation gates fail, reverse-walks dead-end, etc.)*
- [X] All mandatory sections completed — User Scenarios, Requirements, Success Criteria, Assumptions all present

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain — none used; every decision has a default with rationale (collapse onto PURL = canonical CDX pattern; preserve compositions semantics by retargeting; keep fallback path unchanged)
- [X] Requirements are testable and unambiguous — FR-001 through FR-011 each name a measurable invariant or a specific behavior; FR-006 expresses the closure invariant in set-theoretic terms (`S = ...`)
- [X] Success criteria are measurable — every SC-### names a check (closure invariant pass, zero validator warnings, byte-identical goldens, etc.)
- [X] Success criteria are technology-agnostic — SC-001/SC-002/SC-006/SC-007 describe operator/consumer-observable outcomes; SC-003/SC-004/SC-005 reference internal CI gates which are project-policy-level, not technology choices
- [X] All acceptance scenarios are defined — every user story has 2-4 Given/When/Then scenarios
- [X] Edge cases are identified — operator override, no-module-directive, workspace projects, hardcoded literal preservation, golden regeneration shape, SPDX exclusion all covered
- [X] Scope is clearly bounded — "Out of scope" section explicitly excludes SPDX work, slack-notifier ingestion fix, target_ref refactoring, first-party source-file inventory
- [X] Dependencies and assumptions identified — Dependencies section lists 4 prior milestones; Assumptions section documents 6 working assumptions

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria — FR-001 through FR-011 each map to user-story acceptance scenarios or to explicit invariant checks
- [X] User scenarios cover primary flows — US1 (strict validator), US2 (reverse-walk), US3 (compositions semantics), US4 (fallback preservation) cover all four downstream-consumer patterns surfaced in the analysis
- [X] Feature meets measurable outcomes defined in Success Criteria — SC-001 through SC-007 each verifiable post-merge against goldens or against operator-perceived behavior
- [X] No implementation details leak into specification — file paths and line numbers in the "Root cause" section are *diagnostic context* (showing where the two-layer mismatch lives), not implementation prescription; the actual fix shape lives in plan.md per speckit ladder

## Notes

- The bug-fix nature of this milestone makes a small amount of code-context (file paths + line numbers in the "Root cause" subsection) genuinely necessary for stakeholders to understand WHY this is a single-file fix versus a broader rework. This is the same pattern used in recent bug-fix-type spec.md files (e.g., milestone 054 filesystem-walker symlink-loop fix). The "Out of scope" section explicitly excludes refactoring beyond the targeted change.
- Strict CDX validator behavior (FR-006 closure invariant) is the testable contract; the user-story narratives explain the operator-facing motivation.
- Items marked incomplete require spec updates before `/speckit.clarify` or `/speckit.plan`.
