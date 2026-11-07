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

FR-005 asks for a test exercising a **real** root, which the corpus cannot
produce while `--root-name` is passed everywhere. Whether that belongs in the
corpus harness, a unit test, or a new fixture is a design question for
planning. If it lands as a unit test over constructed components, it will not
exercise the real `select_root` ladder — and the ladder is where the
behaviour lives. Planning should say which it is and what that choice does
not cover.
