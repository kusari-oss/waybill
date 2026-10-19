# Specification Quality Checklist: Every unreachable component says why it has no edge

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-10-19
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

All five clarifications are resolved and recorded in the spec's
Clarifications section — two from `/speckit.specify` (the A1 and A2
deferrals) and three from `/speckit.clarify` (C45 extension, mechanism E's
vocabulary, mechanism C's identification). Scope is 18 of I7's 34
offenders; the other 16 are deferred to **#1234** (thirteen missing
`requirements.txt` edges) and **#1235** (three duplicate maven
coordinates), which are reader defects rather than explanation gaps.

**Two ambiguities were closed by measurement rather than by asking**, and
both would have been plausible questions:

- *Is the classifier scope-gated, as `unfalsifiable` was in feature 1217?*
  No. `classify_orphans_pre_emit` is called once, unconditionally, at
  `scan_cmd.rs:4940` on the shared `components` Vec every emitter reads.
  Feature 1217's FR-008a asymmetry has no analogue here.
- *Could extending C151 to `bazel.rs` and `nix/lockfile.rs` close
  mechanisms C and D by itself?* No. C151 attaches only to `design`-tier
  components, and every offender is `source`-tier except mechanism E's two.
  FR-015 therefore closes no cell and must not be counted toward SC-001.

**Content-quality note.** Three items cite file paths and one cites a Rust
snippet (`orphan_reason.rs:161`). Kept deliberately: the ecosystem guard is
the defect being specified, and naming it is what makes FR-003 testable.
The requirements themselves name no language, framework or API.

**The non-obvious success criterion is SC-004**, which requires that
#1234's thirteen and #1235's three *still fail* I7 after this feature.
Without it, a mechanism predicate written loosely would show as a better
SC-001 number while silently closing the two cells that report real bugs —
the failure mode where the metric improves and the product gets worse. It
is paired with FR-011's prohibition on a catch-all marker.

**Deviations from #1214, recorded in the spec rather than silently
accepted**:

- Two of the issue's five population descriptions name the wrong mechanism;
  corrected in `measurements/i7-populations.md`.
- Its marker count is four where the harness now has five (C197 was added
  by m1156 after filing).
- Its "delete all five declarations" done-condition is **not met by
  design**: three are deleted, two survive with corrected counts and
  reasons citing #1234 and #1235, following commit `a3691e56`'s precedent
  for I8. A declaration that outlives its issue is the visible record of a
  remaining gap, which is the property the lane exists to preserve.
