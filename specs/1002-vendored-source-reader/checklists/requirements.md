# Specification Quality Checklist: Vendored source-tree components

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-09-26
**Feature**: [spec.md](../spec.md)

## Content Quality

- [x] No implementation details (languages, frameworks, APIs)
- [x] Focused on user value and business needs
- [x] Written for non-technical stakeholders
- [x] All mandatory sections completed

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain
- [x] Requirements are testable and unambiguous
- [x] Success criteria are measurable
- [x] Success criteria are technology-agnostic (no implementation details)
- [x] All acceptance scenarios are defined
- [x] Edge cases are identified
- [x] Scope is clearly bounded
- [x] Dependencies and assumptions identified

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria
- [x] User scenarios cover primary flows
- [x] Feature meets measurable outcomes defined in Success Criteria
- [x] No implementation details leak into specification

## Notes

Validation was run against the written spec, and three items failed on
the first pass. They are recorded here rather than silently corrected,
because each was a real defect in the draft:

1. **Implementation details leaked into the requirements.** The first
   draft named `MODULE.bazel`, `configure.ac` and `CMakeLists.txt` in
   FR-006 / FR-008 / FR-012 as the places to read identity from. Those
   are implementation choices, and naming them would also have frozen
   the file list into the contract. Rewritten as "a value stated within
   the vendored tree", with the measured file formats kept in the
   Assumptions and in #989 where they inform planning without binding
   it.

2. **A success criterion was not verifiable from outside.** SC-004
   originally read "no component carries an inferred version", which
   cannot be checked by inspecting output. Restated as "no component
   carries a version that is not stated verbatim somewhere in its
   vendored tree, verified by searching the tree for each emitted
   version" — an operator can run that check without reading the code.

3. **Two success criteria were quoted as targets, not observations.**
   SC-002 and SC-003 come from a single project measured on one day.
   Left as numbers because they are genuine observations and the
   fixture is pinned, but the Assumptions section now states their
   provenance explicitly and asks for a second project to be measured
   during planning before they are treated as generalisable. This is
   the CLAUDE.md rule that a number describing external behaviour must
   be traceable to an observation and say so.

One open question is deliberately carried into clarification rather than
guessed: what identifier a vendored component should bear, given there
is no registry coordinate for a vendored copy. It is the standing
question in #952 and materially shapes the emitted output, so it should
be answered explicitly.

Measurement provenance for every number in the spec: `mongodb/mongo` at
`41a5752480dd`, the pinned `cpp-mongo` corpus revision, surveyed
2026-09-26. 51 vendored directories; 11 stating a version of which 3 are
placeholders; 32 shipping a licence file; 2 whose stated name differs
from their directory name.
