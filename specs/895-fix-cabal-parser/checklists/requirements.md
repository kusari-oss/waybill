# Specification Quality Checklist: Trustworthy `.cabal` dependency parsing

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-09-16
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs)
- [X] Focused on user value and business needs
- [X] Written for non-technical stakeholders
- [X] All mandatory sections completed

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain
- [X] Requirements are testable and unambiguous
- [X] Success criteria are measurable
- [X] Success criteria are technology-agnostic (no implementation details)
- [X] All acceptance scenarios are defined
- [X] Edge cases are identified
- [X] Scope is clearly bounded
- [X] Dependencies and assumptions identified

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria
- [X] User scenarios cover primary flows
- [X] Feature meets measurable outcomes defined in Success Criteria
- [X] No implementation details leak into specification

## Notes

All 16 items pass. Four clarifications were resolved in session 2026-09-16
and integrated into the spec; no markers remain.

Two of the four answers expanded scope, deliberately:

- **Build tools become build-time scoped**, which means consumers filtering
  for runtime dependencies stop seeing them. A visible behaviour change, in
  the correct direction, recorded as an assumption rather than left to
  surprise a reviewer.
- **A Haskell target joins the public corpus.** This is real work beyond a
  parser fix — a mirrored fork, a manifest entry, goldens for three formats
  through CI — accepted because SC-002 is otherwise unverifiable and the
  ecosystem has no whole-document coverage at all.

### Validation notes

- **Implementation details**: the spec names no file, function, regular
  expression or crate. The root-cause analysis lives in #891, which the spec
  links rather than restates.
- **FR-009** is the one requirement that might read as implementation
  detail. It is not: fixing FR-005 removes information from a place
  consumers can currently read it, so requiring that the information stay
  reachable and catalogued is a user-facing guarantee.
- **SC-008** deliberately requires that each new test fail against the
  current implementation for its own reason. A test that passes before the
  fix proves nothing, and this project has shipped one — m868's
  `t009b_the_document_is_no_longer_flat` passed against the defect it was
  written to catch, because CycloneDX's primary-dependency fallback
  manufactured the property it asserted.
- **Corpus target naming** is constrained by the project's external-name
  policy: a neutrally-governed OSS project, mirrored to `kusari-sandbox`,
  and explicitly not the commercially-governed project that surfaced the
  bug. Recorded in Assumptions so the planning phase does not have to
  rediscover it.
