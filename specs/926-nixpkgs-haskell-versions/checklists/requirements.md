# Specification Quality Checklist: Resolve Haskell dependency versions through the pinned nixpkgs

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

**All checklist items pass.** Both [NEEDS CLARIFICATION] markers were resolved
in the 2026-09-24 clarification session, recorded in the spec's
`## Clarifications` section.

**Resolved this session:**

- FR-014 (which compiler) → resolve against the per-compiler set when the flake
  names exactly one; when it names several, resolve only dependencies that are
  non-boot in *every* candidate set and leave the rest versionless with the
  candidates recorded. Fail-closed per Principle III. Became FR-014/a/b/c.
- FR-015 (default-on vs opt-in) → default-on, gated on the repository actually
  pinning a nixpkgs-shaped input *and* declaring Haskell dependencies, so the
  cost falls only on projects that benefit. Became FR-015/015a.
- A constraint the spec did not previously carry: the pinned input may be an
  **internal or private mirror** rather than upstream nixpkgs, and egress to it
  may be blocked or require credentials. Became FR-016 through FR-019 plus
  SC-008/SC-009 and two edge cases — the retrieval target is derived from the
  lock entry rather than assumed, an unreachable or unauthorized source
  degrades with its own reason code, nothing prompts, and nothing hangs.
- Scope: declared dependencies only; the transitive closure is deferred to a
  follow-up gated on measuring the component-count multiplier. Became FR-001a
  and an Out of Scope entry.

**Deferred to the plan phase (not blocking):**

- The numeric value of FR-019's retrieval time bound. Per CLAUDE.md a number
  describing external behaviour must trace to an observation, and fetch latency
  against a pinned source has not been measured. The plan's Phase 0 research
  should measure it rather than pick a round number.
- Cache lifetime. A pinned revision is immutable, so retention has an obvious
  default (keep indefinitely, key by revision) and needs no clarification.

**Earlier iteration findings, retained:**

- An earlier draft carried "12 of 19 resolved" from #947 as a success
  criterion. The probe could not reproduce it (10 of 19) and established the
  delta is on the boot-library side. Neither source enumerates the target's
  dependencies, so the figure is unverified in both directions. Replaced with
  SC-001, expressed against the boot set as discovered from the pinned
  revision. Recorded in `measurements/README.md` §M3 rather than dropped.
- An earlier draft assumed a fixed set of seven boot libraries, per #947's
  quoted snippet. Measurement M2 shows the nulled set is compiler-specific
  (40 attributes at GHC 9.4.x/9.6.x, 41 at 9.10.x, five-package symmetric
  difference), so a fixed list would be wrong. Became FR-004.

**Measurement status**: every nixpkgs figure in the spec is reproduced by
`measurements/probe_nixpkgs_haskell.py` against revision
`a799d3e3886da994fa307f817a6bc705ae538eeb`. Byte count and derivation count
match #947 exactly; the boot-library finding is new and contradicts the
issue's abridged snippet.
