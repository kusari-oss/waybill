# Specification Quality Checklist: licenses for nix closure components

**Purpose**: Validate specification completeness and quality before planning
**Created**: 2026-11-06
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
- [x] Success criteria are technology-agnostic
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

**16 of 16** after the 2026-11-06 clarification. One question asked, one
answered, zero markers remaining.

FR-011 resolved to a second pure `nix eval` over the pinned nixpkgs, joined by
exact store path. The candidates were measured before the question was put, not
after — a pure `nix eval` over a locked flakeref returns licenses with both join
keys in 0.47s for four packages, with no `--impure` and therefore no conflict
with `argv_is_safe`.

Three requirements were **added** by that answer rather than implied by it, each
from something the measurement exposed: FR-012 (reuse the closure tier's safety
machinery rather than build a parallel set), FR-013 (survive the
unsupported-system throw that `tryEval` does not catch — one of milestone 1050's
two traps, hit on the first enumeration attempt), and FR-014 (use nixpkgs' own
`spdxId` rather than re-deriving it, so there is not a second mapping to fall
out of date).

**One judgement a reviewer should check**: FR-002 forbids joining by name, or by
name and version, and requires an exact store path. That rules out the cheapest
possible implementation. It is there because a wrong license reaching a
compliance reviewer carries no signal that it is wrong — the same reasoning
milestone 1275 used when it refused a `(name, version)` match across a
PURL-type boundary.
