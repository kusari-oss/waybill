# Specification Quality Checklist: Haskell/nixpkgs resolution correctness

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-11-02
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

### Validation findings

**All clarifications resolved.** FR-002's one marker — which manifest wins when
both are present — was asked and answered in session 2026-11-02:
`package.yaml`. The answer produced a requirement that was not in the first
draft (FR-004a): milestone 143's existing warning tells operators to
"regenerate before scanning", which stops being true once the manifest is what
gets read, so the advice has to be rewritten rather than inherited. A
clarification that only changed a sentence would not have surfaced that.

**Content-quality judgement.** Several requirements name `package.yaml`,
`*.cabal`, `hackage-packages.nix` and `flake.lock`. These are read as
*artifacts of the domain* rather than implementation details — they are the
files an operator's project contains, and a requirement about Haskell manifest
reading cannot be stated without naming the manifests. No requirement names a
language, crate, function or module.

**On SC-003 and SC-004 quoting absolute numbers.** The project's standing rule
is that a number describing external behaviour must be traceable to an
observation. All four are, to
`measurements/reproduce-1273.sh`, which is committed and re-runs. SC-003's
8-component figure is the one number taken from an issue (#1281's sbomnix
cross-check) rather than re-measured here; it is attributed in the spec and
should be re-measured in Phase 0 before any task depends on it.

**On FR-017.** Added after re-measurement, not from the issues: two of the four
defects (#1278's missing diagnostic, #1281's indistinguishable reconciliation
outcome) are unreachable-diagnostic defects. A milestone that fixes two
instances of that class and introduces a third would be a net loss, so it is a
requirement rather than a note.

### Deliberate spec-level decisions

- **FR-001 reverses milestone 143's Q3.** Recorded in the requirement itself
  with the reason, because reversing another milestone's clarified decision
  inside an implementation is precisely what this project treats as out of
  bounds.
- **#1282 and #1283 excluded**, with the reason (both depend on open scoping
  decisions #971 and #1034).
