# Specification Quality Checklist: Refresh the public-corpus goldens with verified drift

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-09-11
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

Validation ran three passes. Issues found and fixed:

1. **Tool, file and env-var names throughout.** The first draft named the
   regeneration env var, the workflow file, the harness module and the
   specific merged milestone believed to cause most of the churn. All
   removed — they are plan/research material. FR-002 and FR-003 now state
   the *constraint* ("same environment class as the gate", "not a
   maintainer's machine") rather than the mechanism, which keeps them
   testable without pinning the spec to today's CI layout.

2. **Fixed target and delta counts were stated as fact.** The draft said
   "10 targets" and "147 merges" in requirements. Those are observations
   from one lane run and will drift before the work starts. FR-001 now
   reads "every target currently failing", so scope tracks reality; the
   observed numbers stay in Assumptions where they are labelled as
   point-in-time.

3. **The assumption that drift is benign was load-bearing.** An early
   draft treated "the churn is expected" as established, which would have
   licensed exactly the rubber-stamp this feature exists to prevent. It
   is now explicitly marked as something the feature must verify per
   target, with FR-006 and FR-007 as the enforcement.

Two things deliberately promoted from nice-to-have to requirement, because
both have already gone wrong on sibling artifacts in this repo:

- **FR-002 / FR-003** (generation environment). A perf baseline recorded
  on one machine class and compared on another produced nine days of
  phantom failures; a corpus fixture whose content depended on optional
  host tooling produced three nights of them. Encoding the generation
  environment is the fix for a recurring class of defect, not a
  formality.
- **FR-010 / SC-003** (prove the gate still detects). A refresh that
  makes a lane green without confirming it can still go red is
  indistinguishable from disabling it.
