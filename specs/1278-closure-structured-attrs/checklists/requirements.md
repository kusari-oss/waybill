# Specification Quality Checklist: a closure member's attributes are read wherever nix put them

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-11-04
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

Three items were failed on the first pass and fixed rather than waved through:

- **No implementation details** — FR-007 originally said the widening "belongs
  at the accessors they already share", which prescribes where the code goes.
  Rewritten to state the observable property instead: every consumer must
  resolve attributes by one rule, and two consumers disagreeing about the same
  derivation is the defect. Key Entities likewise now says the classification
  source is readable "from emitted output" rather than describing it as an
  internal record.
- **Success criteria are technology-agnostic** — SC-002 named the classifier.
  Rewritten as "consulted when deciding the referenced members' roles".

Two deviations are recorded rather than fixed, both deliberate:

- **"Written for non-technical stakeholders"** is marked passing on the
  strength of the *Why this exists* section, which states the problem and its
  consequence without requiring knowledge of the codebase. The requirements
  and success criteria below it do assume familiarity with SBOM scope markers
  and nix's attribute model. That is the house style for this repository —
  every closure milestone's spec reads this way — and the alternative would be
  to describe a wire-format defect without naming the wire format.
- **Field names and annotation names appear throughout** (`buildInputs`,
  `scope: "excluded"`, `waybill:closure-role`). These are not implementation
  details but the published contract: they are exactly what a consumer of the
  emitted document reads, and a spec about them that declined to name them
  would not be testable.

Numbers are labelled by provenance per the repository's measurement rule. Every
count in Success Criteria is measured, with the probes and the invocation that
produced them committed under `measurements/`. SC-001 and SC-002 are stated as
invariants precisely because the counts beside them can move when nix or
nixpkgs does. The one claim that could not be measured directly — that the
defect is live in CI and not only locally — is carried by a control stated in
Assumptions (347 of 348 roles agree with the committed golden) rather than
asserted.
