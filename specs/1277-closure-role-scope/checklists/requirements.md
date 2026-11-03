# Specification Quality Checklist: a closure member that only builds the artifact must not read as part of it

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-11-03
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

**Both [NEEDS CLARIFICATION] markers are resolved** in `/speckit.clarify`
Session 2026-11-03 and recorded in the spec's Clarifications section. FR-004
takes `excluded` for the `unreferenced` role; FR-005 keeps milestone 1276's
exemption and leaves invariant I9 unamended. The second answer's losing
argument is recorded alongside it, because the overlap's population is zero
and I9 failing is the event that makes the question answerable with data.

**One requirement rests on something unmeasured, and says so.** FR-004a asks
whether the role classifier's seven field names cover the dependency keys
real derivations carry. Every fixture in the tree carries only those seven,
so they cannot answer it, and a real closure needs a host `nix` — so this is
a probe the plan must schedule, not a number quoted into a requirement.
SC-009 therefore asks for the count to be *known and recorded*, not to be
zero: quoting zero without the enumeration is exactly the assumption the
requirement exists to retire.

**Content-quality note on "no implementation details".** The spec names
CycloneDX `scope` values, the C182 annotation and nix's own field names
(`buildInputs`, `nativeBuildInputs`). These are the domain's vocabulary and
the external contracts the feature is judged against, not implementation
choices — the same standard applied in milestones 1275 and 1276. Internal
structure (function names, module paths, types) appears only in
`measurements/baseline.md`, where it cites the evidence for a measured
claim.

**On the premise correction.** #1342 asserts no closure member carries a
native `scope`; ten do. The spec states the corrected figure and carries the
consequence into FR-002 rather than restating the issue. A spec that
inherited the wrong premise would have specified `artifact-input → required`,
which `hspec` falsifies on real data.
