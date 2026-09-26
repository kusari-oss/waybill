# Specification Quality Checklist: The scanned project's declared license reaches its SBOM

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-09-26
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

**Iteration 1 — 2026-09-26**

Three items warrant comment rather than a bare tick.

- *No implementation details*: FR-013 and FR-014 name five source locations and
  one test by path. These are retained deliberately. They specify **what must
  change** — a fixed, enumerable set of stale references — not how to change it,
  and enumerating them is what makes the requirement testable (SC-008 counts
  them). Removing the paths would make the requirement vague, which the
  "testable and unambiguous" criterion weighs more heavily.

- *Success criteria are technology-agnostic*: SC-001 through SC-007 describe
  document contents and scan behaviour, not internals. SC-008 references issue
  #103 by number, which is a project artifact rather than a technology.

- *No [NEEDS CLARIFICATION] markers remain*: **fails on purpose.** Three
  questions in the "Open Questions" section materially affect scope and have no
  defensible default:
  1. whether the synthetic scan-root component inherits a license;
  2. whether dependency components are in scope;
  3. whether the emitted license records which manifest supplied it.

  Question 1 is the sharpest: the headline measure (a license on the document's
  root component for every corpus target) is **unreachable** if the answer is
  "leave it absent", because the root is synthetic and has no manifest. SC-001 is
  therefore phrased against the main-module component and may need restating once
  this is settled.

  **Resolved in iteration 2 (same session).** All three were put to the user and
  answered; the spec now records them under "Clarifications / Session 2026-09-26"
  and carries the consequences:

  1. Scan-root inherits a license only when exactly one main-module carries one →
     FR-016, FR-017, SC-001a.
  2. Main-module only; incidental dependency coverage accepted, not pursued →
     Out of Scope.
  3. No source annotation; the declared attribution suffices → Out of Scope.

  SC-001 was restated as a result: it had been phrased against "a root license",
  which was unreachable before the inheritance rule existed. It is now split into
  SC-001 (main-module) and SC-001a (document primary component).

**Baseline evidence**: measurements in the Context section were taken from the
committed corpus goldens at `main` @ `5453c6f2`, not estimated. The #957 Haskell
output quoted there is copied from `haskell-aeson/cdx.json`.

**Iteration 3 — 2026-09-26 (`/speckit.clarify`)**

Three further questions asked and integrated. All were edge cases the spec *listed*
but did not *resolve*, which the first pass had ticked as "edge cases identified"
without noticing that identifying is not deciding.

1. **Uncanonicalisable declarations** are now preserved as custom non-listed license
   references rather than dropped (FR-004 → FR-004c). This **supersedes** #957 and
   adds FR-008a to bring it into line — otherwise the one finished ecosystem would
   be the only one that loses a declaration.
2. **Multi-license declarations** are combined by the reader using its own
   ecosystem's documented operator (FR-010 → FR-010b). Investigation found the
   shared emitter joins with an unconditional `AND`, which would assert conjunction
   for ecosystems whose arrays document choice. Avoided rather than fixed; recorded
   under Out of Scope as an observed issue affecting the OS-package readers.
3. **Inherited licenses** are resolved per ecosystem rules (FR-011a, FR-011b).
   Evidence: both of waybill's own member crates use `license.workspace = true`, so
   without this the feature returns nothing for the repository it is built in.

Two claims were checked against the code rather than assumed, and both changed the
design:

- `SpdxExpression` has a lenient constructor alongside the strict one, and the
  emitters already mint a `LicenseRef-<hash>` with document-level extracted text
  when canonicalisation fails. Preservation is therefore expected to need **no**
  emitter change — the opposite of the assumption that it would widen scope.
- The emitter's `AND` join was found by reading `reduce_license_vec`, not predicted.

Spec grew from 305 to 404 lines; 25 functional requirements, 12 success criteria,
6 recorded clarifications. All checklist items remain passing.
