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


## Clarification session 2026-09-11

Three questions asked and integrated. All three resolved genuine
ambiguity rather than confirming defaults:

1. **Non-drift failures contradicted SC-001.** FR-012 forbade
   regenerating such a target's golden while SC-001 demanded a 100%
   pass rate — mutually unsatisfiable if such a target exists. Resolved:
   repair it here or drop it from gating with a tracked issue, and
   SC-001 now measures "targets it gates". Added FR-012a so a dropped
   target is visible in lane output, because shrinking coverage to reach
   100% would otherwise be indistinguishable from earning it.

2. **FR-013 was written with an unresolved "or".** Now decided: one
   change covering every target. Added FR-013a requiring evidence be
   comparable ACROSS targets — a target whose delta pattern differs from
   its peers is the likeliest place for a regression to hide, and that
   signal only exists when they are reviewed together.

3. **FR-015 required evidence but not a location.** Now the PR
   description, explicitly not a committed document, with FR-015a
   requiring the commit to reference the PR so it stays reachable from
   `git log`. Aligns with #827's retirement of point-in-time documents
   that read as current long after they aren't.

A terminology pass followed Q1: FR-009 and a Story 1 acceptance scenario
still said "every target" where SC-001 had become "targets it gates".
Both corrected, so the spec now uses one term for one concept.

Counts after clarification: 18 FRs, 7 SCs, 0 unresolved markers.
