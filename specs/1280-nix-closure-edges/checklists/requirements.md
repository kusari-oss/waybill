# Specification Quality Checklist: the nix closure tier emits its dependency graph

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-11-05
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

**16 of 16 pass** after the 2026-11-05 clarification session. Three questions
asked, three answered, zero markers remaining.

Figures corrected during validation, all against the committed golden rather
than from memory:

- SC-005's role census read `269 / 118 / 42 / 1,315`; the golden says
  `268 / 121 / 45 / 1,315`. Three of four numbers were wrong, and they are
  milestone 1279's own post-merge figures.
- SC-003 asserted `graph-completeness` stops being `partial`. It will not —
  the reason carries two independent causes and this feature addresses one — so
  the criterion is now worded against the `orphaned-components-detected`
  clause. As first written it would have failed for the milestone working
  perfectly.
- SC-002 read "orphans fall from 352 to **0**". Measured under the chosen
  projection it is **352 to 5**, and the five are a deliberate residue with one
  uniform cause. A criterion demanding zero would have pushed an implementer
  toward the full-transitive projection, which asserts edges nix never stated.
- The Context section claimed the closure contributes **zero** relationships.
  It contributes 103 SPDX 2.3 `BUILD_TOOL_OF` edges from milestone 1277; the
  zero is specifically *dependency* edges. SC-010 now pins the 103.

**Two judgements a reviewer should check**, since both were decisions rather
than deductions:

- FR-002 and FR-007a together forbid the declared attributes from influencing
  either the graph or the relationship type. That is what makes SC-005's
  "census unchanged" meaningful and keeps the three formats carrying one graph.
  A reviewer who wants role-typed relationships changes FR-007a, SC-005 and the
  parity expectations together.
- FR-010 leaves 5 components orphaned on purpose. The alternative that reaches
  zero (full transitive) was measured and rejected for asserting transitive
  edges as though nix had stated them.

**One requirement is stated against an unmeasured risk**, deliberately: FR-012
asks that the projection stay linear in closure size. Both measured closures
are small enough that 381 full traversals would not be noticed, which is why it
is a requirement rather than a discovery waiting on a larger closure.
