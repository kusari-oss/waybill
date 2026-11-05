# Specification Quality Checklist: `unreferenced` must mean "nothing depends on this"

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-11-05
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs)
- [X] Focused on user value and business needs
- [X] Written for non-technical stakeholders
- [X] All mandatory sections completed

## Requirement Completeness

- [ ] No [NEEDS CLARIFICATION] markers remain
- [X] Requirements are testable and unambiguous
- [X] Success criteria are measurable
- [X] Success criteria are technology-agnostic (no implementation details)
- [X] All acceptance scenarios are defined
- [X] Edge cases are identified
- [ ] Scope is clearly bounded
- [X] Dependencies and assumptions identified

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria
- [X] User scenarios cover primary flows
- [X] Feature meets measurable outcomes defined in Success Criteria
- [X] No implementation details leak into specification

## Notes

**Two items fail, and they are the same item.** Both open questions are
scope-defining rather than detail-level, which is why the scope box is also
unticked:

1. **The cross-compilation triples.** `depsTargetTarget` and
   `depsTargetTargetPropagated` name dependencies of the *target* platform —
   what a compiler being built hands to things it later compiles. They are
   neither in the artifact nor tooling that built it. Classifying them under
   an existing role is a judgement; adding a fifth role value changes the C182
   closed enum and every consumer of it, including invariant I10. The three
   options differ in blast radius by an order of magnitude.
2. **stdenv's defaults.** `defaultNativeBuildInputs` and
   `defaultBuildInputs` carry **323 of the 353** unread declarations, on 19
   derivations. They are stdenv's own machinery, not a package's declared
   dependencies. Classifying them grows `build-tooling` substantially;
   *not* classifying them but marking the distinction satisfies #1346's actual
   ask at a fraction of the change. Which one is wanted determines whether
   this milestone is small or large.

Neither has a defensible default, so neither was guessed. The spec is
otherwise complete and the baseline is measured, not estimated: the census
probe is committed at `measurements/attribute-census.py` with its invocation
and date, and reports 6 populated unconsulted attributes carrying 353
store-path declarations, against the issue's original "7 of ~20" estimate.

Worth noting for the clarify session: the measured distribution argues
*against* treating the six as one population. One attribute carries 88% of
the declarations and is the least package-like of them; the propagating one
carries 7 declarations but is the only one whose effect is not local. A
requirement written over "the six" would be wrong in both directions.
