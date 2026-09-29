# Specification Quality Checklist: Opt-in `nix eval` resolution tier

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-09-28
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

Three items pass only against a reading of them that should be stated, rather
than silently assumed:

1. **"No implementation details" / "technology-agnostic"** — the spec names
   `nix`, `nix eval`, `allow-import-from-derivation` and `builtins.getEnv`.
   These are the *external system the feature is about* and observations of its
   behaviour, not waybill's internals. The spec names no language, crate,
   module, type or function of waybill's own. Read strictly, a
   "technology-agnostic" success criterion for a feature whose entire purpose is
   invoking one specific tool would be vacuous.

2. **"Written for non-technical stakeholders"** — it is not, and no spec in this
   repository is. The readership is supply-chain and SBOM engineers. The spec is
   written to be readable by someone who has not used Nix, which is the
   achievable version of this item here.

3. **"Success criteria are measurable"** — SC-007 and FR-012 deliberately defer
   a number to a planning-phase measurement rather than quoting one. This is
   required by the project's own rule (CLAUDE.md, "Measure external behaviour
   before designing around it"): a figure describing external behaviour must be
   traceable to an observation, and where it cannot be measured yet it is
   expressed as a ratio against a baseline a task will establish. The
   *requirement* is testable; only its threshold is pending.

One item was failed on the first validation pass and fixed:

- **"All functional requirements have clear acceptance criteria"** — FR-018 and
  FR-019 originally read "MUST make X reachable to a later feature", which is
  not testable. Rewritten so FR-018 asserts a checkable property of a single
  evaluation pass, with SC-009 added to cover it. SC-008 was added at the same
  time to cover FR-005 and FR-006, which had acceptance scenarios in User Story
  5 but no success criterion.

### Deliberately carried into planning

Two claims are marked in the spec as **not measured** (Assumptions A-6, A-7) and
are listed in the research constraint: the minimum `nix` version honouring
`allow-import-from-derivation`, and whether `nix` bounds evaluation time or
memory by default. Both must be probed before they enter the plan. This is not
an incomplete checklist item — it is the spec doing what the research constraint
requires of it.
