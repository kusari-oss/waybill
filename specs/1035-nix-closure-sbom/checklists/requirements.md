# Specification Quality Checklist: Nix derivation closure as SBOM content

**Purpose**: Validate specification completeness and quality before planning
**Created**: 2026-09-29
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

Same three items pass only under a stated reading as in milestone 1034, for the
same reasons, plus one new:

1. **"No implementation details" / "technology-agnostic"** — the spec names
   `nix`, `nativeBuildInputs`, CycloneDX `pedigree.patches[]` and specific CVE
   ids. These are the external systems and formats the feature is *about*, and
   the schema shapes are quoted because they were verified rather than
   remembered. No waybill language, crate, module or function appears.

2. **"Written for non-technical stakeholders"** — it is not, and no spec in
   this repository is. It is written to be readable by someone who has not used
   Nix, which is the achievable version here.

3. **"Success criteria are measurable"** — every SC cites a measured figure
   except SC-005 and SC-007, which assert the existence of a property rather
   than a threshold. SC-006's 40 and 46 are derived from the measured 43/50
   patches minus 3/4 CVE-named.

4. **The load-bearing unknown was measured rather than assumed.** SC-001
   originally claimed "more components" without knowing whether the closure's
   artifact inputs overlapped what waybill already emits. Measured before this
   spec was finalised: 216 and 218 components exist in the closure and not in
   today's output, against overlaps of 33 and 160. SC-001 now states those
   baselines instead of a direction.

   The same measurement surfaced a second fact the spec now records: 20 and 30
   components waybill emits are *absent* from the closure. That is carried into
   planning as an open question, because it decides whether closure emission
   replaces or supplements the manifest-derived set.

### Deliberately carried into planning

Four items are marked not-yet-measured in the research constraint: the
overlap question above, the mechanics of attributing a patch to a component,
whether closure composition holds outside Haskell, and cold-store cost. None is
an incomplete checklist item; all are the spec doing what the constraint
requires.
