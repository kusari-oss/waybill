# Specification Quality Checklist: Reduce `go mod why` subprocess-spawn amplification on Go monorepos

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-09-04
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs)
    - Language names appear only where they name external tooling the user must have installed (the `go` binary and its `go mod why -m` subcommand) or where they name existing waybill artifacts referenced by the milestone (file paths under `waybill-cli/src/scan_fs/package_db/golang/mod_why.rs`). These are targets of the change, not implementation choices.
- [X] Focused on user value and business needs
    - Every US names the operator-visible outcome (faster scan, complete classification data) rather than the internal mechanism.
- [X] Written for non-technical stakeholders
    - Motivation section leads with the observable symptom (82.6s scan → 60s budget exhausted → 22/39 workspaces skipped) before the technical decomposition.
- [X] All mandatory sections completed
    - User Scenarios & Testing, Requirements, Success Criteria, Assumptions all present.

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain
    - Zero markers in spec.
- [X] Requirements are testable and unambiguous
    - Every FR names a specific, verifiable behavior (batch size ≥ 500, subprocess cap = logical-CPU count, exactly one preflight per go.work scope, etc.).
- [X] Success criteria are measurable
    - SC-001 through SC-006 each state a concrete measurable outcome (wall-time seconds, log-line field presence, byte-identity of goldens, Cargo.lock delta).
- [X] Success criteria are technology-agnostic (no implementation details)
    - Criteria reference operator-visible artifacts (wall-time of a shell command, contents of the emitted SBOM, contents of a log line, presence/absence of a Cargo dependency at the lockfile level). SC-004's phrasing about Cargo is inherent to the "zero new dependencies" constraint the user set.
- [X] All acceptance scenarios are defined
    - Each US has 3-4 Given/When/Then scenarios.
- [X] Edge cases are identified
    - 7 edge cases listed: zero main-modules, missing toolchain, single-workspace repo, old Go toolchain, argv limit, budget contention, per-member divergence within a `go.work`, log-line interleave.
- [X] Scope is clearly bounded
    - Non-Goals section explicitly excludes 8 out-of-scope changes (parsing semantics, verdict rules, flag surface, budget default, log wire-shape, WorkspaceMode enum, operator-facing flags, cross-scan caching).
- [X] Dependencies and assumptions identified
    - Assumptions section covers reference-class benchmark host, canonical fixture, budget adequacy, toolchain version floor, argv-length empirical basis, concurrent-safe module cache, and byte-identity of existing goldens. Dependencies section names m112 + m231 + m669.

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria
    - FR-001 → US1 acceptance-1; FR-002 → US1 edge-case (argv limit); FR-003 → US2 acceptance-2; FR-004 → US2 acceptance-4; FR-005 → US2 acceptance-3 + SC-005; FR-006 → US3 acceptance-1; FR-007 → US3 acceptance-4; FR-008 → US3 acceptance-2; FR-009 → SC-003; FR-010 → SC-006; FR-011 → SC-003; FR-012 → SC-003; FR-013 → SC-004; FR-014 → US3 edge-case (old toolchain); FR-015 → SC-003.
- [X] User scenarios cover primary flows
    - US1/US2/US3 cover the entire fix arc from MVP through full milestone.
- [X] Feature meets measurable outcomes defined in Success Criteria
    - Each US's independent test cites an SC and a threshold.
- [X] No implementation details leak into specification
    - See first item above.

## Notes

- No items are incomplete. Spec is ready for `/speckit.clarify` (optional — this spec has zero NEEDS CLARIFICATION markers because the empirical decomposition already resolved the ambiguities) or `/speckit.plan`.
