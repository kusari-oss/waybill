# Specification Quality Checklist: a closure member that is a known Hackage package is identified as one

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

**FR-012 resolved in the 2026-11-03 clarification session.** It was not an
oversight and not answerable by a reasonable default, because the two
reasonable answers contradicted each other:

- #1282 names "no edges" as a defect and measures 0 of 0 against sbomnix's
  8,230 on the same project;
- milestone 1066 deliberately decided that closure references drive a
  component's **role** and not its `dependsOn` edges, "because a store
  reference is not a declared dependency" — a decision with its own
  reasoning, recorded in `docs/ecosystems.md`.

Guessing either way would have silently overruled one of them. The resolution
takes neither: **edges come from the dependencies the package set declares**,
so m1066's role model is untouched and `dependsOn` keeps one meaning across
manifest- and closure-derived components. Recorded as FR-012/FR-012a-c and
SC-005a/SC-005b.

**On "no implementation details":** the spec names `pkg:hackage`,
`pkg:generic` and `hackage-packages.nix`. These are kept deliberately. They
are not implementation choices this feature makes — they are the published
wire vocabulary a consumer reads and the external artifact nixpkgs publishes,
so a requirement that avoided naming them would not be testable.

**Measurement provenance:** every number in the spec is from a scan at
`001fa50` on 2026-11-03, aarch64-darwin, nixpkgs `a799d3e3`. The probe that
produces them is committed at
`measurements/size-closure-members.py` and re-runs against any
`--nix-closure` document.
