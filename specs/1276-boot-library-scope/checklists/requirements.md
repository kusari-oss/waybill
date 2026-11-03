# Specification Quality Checklist: a package the compiler ships is never scoped as absent from the artifact

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-11-03
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

Three items were judgement calls rather than clean passes, recorded so a
reviewer can disagree with the reasoning rather than the verdict.

**"No implementation details"** — Context quotes the vendored CycloneDX 1.6
schema and the Key Entities name "the propagation's immune set". Both are
retained deliberately. The schema quotation *is* the feature's premise: without
it this is a semantics preference, and with it it is a conformance defect, so
removing it would leave the spec unable to say why the fix is correct. The
immune set is named because it establishes that the mechanism already exists,
which bounds the work; it names no function, file or type.

**"Technology-agnostic success criteria"** — SC-001 names `pretty` and
`slack-web`, and SC-002 names two corpus targets. These are the measured
subjects, not implementation choices: the project's standard requires every
number in a success criterion to be traceable to an observation, and these
are the observations. A criterion reading "boot libraries are correctly
scoped" would be technology-agnostic and unverifiable.

**"Written for non-technical stakeholders"** — partial, honestly. A reader
needs to know what a GHC boot library is. The Context and Key Entities define
it in terms of what the compiler does rather than how waybill detects it,
which is as far as this subject can be taken without losing the meaning.

## Verification performed

Every figure in the spec was measured on this branch before it was written,
not inherited from the issue:

- `pretty` scoped `excluded` on `slack-web`: reproduced via
  `specs/1273-haskell-nixpkgs-truthfulness/measurements/sc003-public-library.sh`
  against revision `5ae3b07d…`, which is in the local per-revision cache.
- 10 of 78 boot-library components mis-scoped, in 2 targets; 142 OS-package and
  175 file-tier components carrying no scope at all: measured over the 17
  committed corpus goldens, with the no-scope counts reported as the positive
  control, because a bare zero cannot distinguish "never mis-scoped" from
  "never examined".
- The CycloneDX 1.6 `meta:enum` definitions of `required`, `optional` and
  `excluded`: read from `waybill-cli/tests/fixtures/schemas/cyclonedx-1.6.json`,
  not from memory.
- The 8-component control in SC-003: from the same `slack-web` probe run.
