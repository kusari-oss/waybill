# Specification Quality Checklist: the curated CPE table covers 12 names; 51 more exist

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-11-07
**Feature**: [spec.md](../spec.md) | **Issue**: #1381

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
- [X] Measurable outcomes are traceable to an observation, not a guess

## Notes

Three items warrant comment rather than a bare tick.

**"Success criteria are measurable"** is satisfied in an unusual way: SC-004
is stated as a *property* (`with_candidates +N` / `unaddressable −N` for the
same N) rather than a number, and the spec says why. N is at most 51 and will
be lower, because title confirmation rejects candidates — `ninja` already.
Fixing N now would mean quoting the sweep's entry counts as though they were
verified rows, which is the exact error FR-002 exists to prevent. This follows
the project standard: *"If it cannot be measured yet, express it as a ratio
against a baseline a task will establish, not as an absolute quoted from an
unverified source."*

**"No implementation details"** holds in the requirements and criteria.
Named identifiers — `GENERIC_LIBRARY_CPES`,
`every_table_pair_exists_in_the_nvd_dictionary` — appear in the Summary and
FR-001 because the feature's whole point is to use existing enforcement
rather than build new, and naming it is the shortest honest way to say so.

**The external dependency is dated, not permanent.** Every figure rests on
NVD's dictionary as of 2026-11-07. The Assumptions section says so, and the
existing test asserts only that a pair *was seen* rather than how many
entries it had — which is the right strength for a service that grows.

### A risk this checklist cannot catch

Title confirmation is a **judgement**, not a computation. `ninja` is obvious
once the titles are read; a borderline case — a vendor that ships a fork, or
a renamed project — is not. The plan should state what evidence a row needs
and keep the title text in the record, so a later reader can disagree with a
specific judgement rather than re-derive all of them.
