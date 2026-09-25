# Specification Quality Checklist: Transitive runtime closure for Nix-built Haskell projects

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-09-24
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

### Validation record

The spec was drafted in one pass, then reviewed against this checklist. Three
items failed that review and were fixed. Recording them because a checklist of
all-ticks with no findings is indistinguishable from a checklist nobody ran.

1. **SC-008 was not measurable.** It read "grows by no more than the time to
   read and index the already-cached package set once", which is an assertion
   about internals rather than an observable outcome, and it used
   implementation vocabulary ("re-parsing"). Rewritten as a ratio — no more
   than 1.5× the wall clock of the same scan with the feature disabled — which
   the scan itself establishes a baseline for. Absolute timings would be
   machine-specific and unverifiable by a reader.

2. **FR-003 stated a mechanism, not a requirement.** It said the closure must be
   derived from data "already retrieved and cached per revision by milestone
   926" — describing how, when the user-facing constraint is that resolving the
   closure costs no extra retrieval. Rewritten so it is checkable from outside:
   a project whose declared dependencies resolve offline must have its closure
   resolve offline too.

3. **The large-closure edge case was unfalsifiable.** "Must not make document
   size unbounded or scan time superlinear in a way an operator cannot predict"
   cannot be tested. Replaced with the actual bound — the closure cannot exceed
   the package set, which is finite and fixed by the pinned revision — and a
   pointer to SC-008 and FR-016, which carry the testable parts.

**No [NEEDS CLARIFICATION] markers were needed.** The three questions that would
normally arise here were settled by measurement before the spec was written
(method and figures in issue #962):

- *Which dependency classes* — settled by external agreement with a Nix
  evaluation of the runtime inputs, and by the 7.3× vs 1.5–3.8× size
  difference. Deferred half tracked as issue #985.
- *Whether the closure is per-GHC-series* — measured identical across four
  series on two projects.
- *Whether cycles are a real concern* — confirmed present, so termination is
  stated as FR-010 rather than assumed away.

### Clarification session 2026-09-24

Three questions asked and answered; all three changed requirements rather than
confirming them, which is the test of whether a clarification round earned its
place.

1. **Default behaviour** → default ON with an opt-out. Turned an Assumption into
   FR-016, added FR-017 (disabling returns byte-identical prior output) and
   SC-009 (verified against the existing corpus goldens before they are
   regenerated).

2. **How declared-vs-transitive is carried** → explicit per-component record,
   not graph position. Added FR-006a. The reason is concrete rather than
   stylistic: CycloneDX's primary-dependency fallback (milestone 894)
   synthesizes a root edge to every unreferenced component when the root has no
   declared edges, under which every closure member would read as declared.
   This was discovered empirically while building the per-PR integrity suite in
   #982, not reasoned from the spec.

3. **Unresolvable transitive names** → emitted as versionless components with a
   reason, same as declared ones. Added FR-005a, tightened SC-005. Option B
   (count only) was rejected because the inbound edge would then have to dangle
   or be dropped, violating FR-008 / invariant I2 — the defect #980 was filed
   for.

### Standing risk to carry into planning

The largest measured closure is 394 components from 162 declared. Document
growth of that order is the feature's main externality and the reason FR-016
requires an independent opt-out. Planning should confirm the opt-out is
reachable without also disabling milestone 926's declared-dependency
resolution, since those are separately valuable.
