# Specification Quality Checklist: Transitive propagation of non-runtime lifecycle scope

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-10-25
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

Three deliberate departures from the default reading of these items,
each recorded because a reviewer would otherwise read them as misses.

1. **"No implementation details" is satisfied in the requirements, not
   in the diagnosis.** FR-001..FR-018 and SC-001..SC-008 name no module,
   function or type. The *Why this is not a greenfield feature* and
   *Two populations* sections do cite file paths — `scope_propagation.rs`,
   `invariant-declarations.toml`, `generate/graph_completeness/` — because
   they are evidence for where the defect was measured and which gate
   already fails. Removing those citations would make the measurements
   unverifiable, which the project's own measurement rule forbids. The
   same applies to the closing assumption about where the pass runs: it
   records a placement constraint discovered by reading the call order,
   and is a planning input, not a requirement.

2. **Two P1 stories, deliberately.** US2 (no runtime dependency is
   demoted) is not a lower priority than US1 (dev transitives get
   marked). A pass that ships US1 without US2 is net-harmful: it passes
   every count-based test while silently demoting shipping dependencies,
   which is under-reporting and invisible in the counts US1 is measured
   by. Both are P1, and the MVP is both, not the first.

3. **One success criterion was corrected against the issue, not copied
   from it.** #1209's "Done when" promises `pydantic`'s reported orphan
   count drops. It will not: that count is derived from reachability
   alone and scope marking does not change reachability. FR-018 forbids
   claiming it and the spec says why. A spec that inherited the claim
   would have been unachievable in a way only visible at acceptance.

Numbers carry provenance: the five I3 counts are current-build corpus
observations, the 103 / 76 / 21 are waybill 0.10.1 against a
non-corpus pin and are marked as provenance rather than targets
(SC-008).
