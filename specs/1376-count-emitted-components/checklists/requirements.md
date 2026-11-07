# Specification Quality Checklist: C209 promises a sum a consumer cannot check

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-11-07
**Feature**: [spec.md](../spec.md) | **Issue**: #1385

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

**This spec is unusual: nothing is broken.** The counts are correct in every
format; the sentence describing them is not. FR-002 says so explicitly so
that an implementer does not "fix" a number that is right — which is the
mistake the originating issue made, and which cost a wrong diagnosis before
the formats were measured side by side.

**The scope boundary is the one to watch.** The tidiest-looking fix — emit
the root twice in CDX so `.components | length` works — is **out of scope**,
because it changes document shape for a reporting convenience. A reviewer who
thinks that is the obvious answer should read the Out of scope section first.

### A risk this checklist cannot catch

FR-005 asked where a real-root test should live. **Resolved during
clarification**: an integration test scanning an existing fixture without
`--root-name`, so the real `select_root` ladder runs.

The risk this item was raised against has partly evaporated on measurement —
four in-tree fixtures already produce a promoted real root, and two were
checked to exhibit the exact failure (`row=10` against
`.components|length=9`). So the shape was never unreachable; it was simply
never asserted against.

**The residual risk is narrower and worth stating.** The four fixtures are
`npm` and `hackage` roots. A promoted root arising from a different ecosystem
— a single Cargo crate, a single Go module — is not covered by any of them,
and the `cargo` and `pants_go` fixtures were measured to produce *synthetic*
roots instead. Planning should decide whether one ecosystem's promoted root
is enough evidence for the rule, or whether a second is wanted.
