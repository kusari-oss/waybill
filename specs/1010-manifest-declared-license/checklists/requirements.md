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

**Iteration 4 — 2026-09-26 (`/speckit.analyze` remediation)**

Analysis found 11 issues, 0 CRITICAL. All remediated. Two were faults in this
checklist's own earlier passes, which is worth recording rather than quietly
fixing.

**Corrections to earlier ticks in this file:**

- *"Requirements are testable and unambiguous"* was ticked in iteration 1 and
  should not have survived iteration 3. FR-010c, added during planning, read
  "Research has established that most ecosystems do not define these semantics" —
  a finding about the world, not an obligation on waybill, and not testable as a
  MUST. It is now an Assumption. The tick was wrong because the checklist was not
  re-run against requirements added after it was last reviewed.
- *"No implementation details"* remains ticked, and the reasoning in iteration 1
  still holds, but note FR-002a now carries the Principle V audit into the spec.
  That is required by the constitution, which says the audit MUST be cited in the
  spec's Functional Requirements — it had been in plan.md only.

**Remediated:**

| ID | Severity | Fix |
|---|---|---|
| E1 | HIGH | FR-007 had zero coverage. New task: malformed-license-field fixture asserting the component is still emitted and exit is zero |
| E2 | HIGH | FR-011b had zero coverage. New task: workspace fixture whose root declares no license, asserting the member carries none and the scan succeeds |
| F1 | HIGH | 13 `[P]` markers spanned only 2 files, and one task created the file two others edited. Test files split per story (`declared_license.rs`, `_preservation.rs`, `_consistency.rs`); `[P]` removed from every same-file task. `[P]` count 37 → 24, and no two `[P]` tasks now share a file |
| D1 | HIGH | Principle V audit moved into the spec's Functional Requirements as FR-002a, per the constitution's explicit wording |
| U1 | MEDIUM | FR-010c removed from requirements; the finding recorded under Assumptions |
| E3 | MEDIUM | SC-004b had unit coverage only. New task: two-license fixture per list-valued ecosystem asserting the operator end to end |
| E4 | MEDIUM | Two edge cases resolved in place — the malformed manifest now points at FR-007 and its task; two readers disagreeing is declared out of scope, since merging two discoveries of one project belongs to the reconciliation pass |
| F2 | MEDIUM | "13 sites" vs 14 enumerated rows reconciled: 14 sites, 13 requiring work, 12 ecosystems (11 requiring work; gem and npm have two sites each) |
| A1 | LOW | Resolved by U1 |
| F3 | LOW | "five unverified rows" vs six Phase 1 tasks reconciled; the operator table genuinely needs five, cargo's row is verified separately |
| E5 | LOW | Left as-is: SC-001 and SC-004c remain indirectly covered, which is proportionate |

**Post-remediation state**: 26 FRs, 12 SCs, 58 tasks, T001–T058 contiguous, zero
phantom references, zero `[P]` conflicts, zero uncovered requirements. Three
requirements (FR-001, FR-002, FR-008) and three criteria (SC-001, SC-002, SC-004c)
are covered by task description rather than explicit citation, which is acceptable.

**Process note for next time**: the `[P]` defect and the FR-010c tick were both
introduced *after* the checklist was last run, and neither would have been caught
by re-reading the artifacts. Both were found by extracting IDs and file paths
mechanically and looking for contradictions. Re-run that extraction whenever an
artifact changes, rather than re-reading it.
