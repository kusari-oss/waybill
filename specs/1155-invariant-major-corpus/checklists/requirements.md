# Specification Quality Checklist: Universal Semantic-Invariant Layer for the Public Corpus

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-10-15
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

### Validation iterations

**Iteration 1 — three items failed, all now fixed.**

1. *"No implementation details"* — failed. The spec cited file paths and
   a Rust type signature (`manifest.rs:17`, `fn(&EmittedSboms) ->
   Result<(), AssertionFailure>`). **Resolved as a deliberate
   exception**: this feature's subject *is* internal test
   infrastructure, so its "users" are maintainers and the existing code
   shape is the problem being described. The citations are confined to
   the Context section and the Dependencies list, where they establish
   the current state. No FR or SC names a language, type or file. Items
   under Content Quality and Feature Readiness are marked pass on that
   basis, and a planner should read the Context citations as evidence
   rather than as design.

2. *"Success criteria are measurable"* — initially failed on SC-001,
   which read "a defect class is found across all ecosystems at once"
   with no counted quantity. Rewritten to name the target count (17) and
   the baseline it replaces (reading 17 documents by hand).

3. *"Scope is clearly bounded"* — initially failed: the spec did not say
   whether the twelve failing cells get fixed here. Resolved by the first
   Assumption, which puts fixes under their own issues and gives the
   reason (a visibility mechanism and its fixes landing together makes
   golden movement unattributable).

**Iteration 2 — one item failed, now fixed.**

4. *"Requirements are testable and unambiguous"* — FR-012 ("materially
   different observation") was a judgement call with no stated rule.
   **Resolved in the 2026-10-15 clarification session**: declarations
   pin a violation *kind* and record a count; the kind must match
   exactly, the count may drift (FR-010, FR-012, FR-012a). The reasoning
   that kept it vague in iteration 2 — that an exact-match rule turns
   innocent drift into lane failures and gets the gate switched off —
   is now the stated rationale inside FR-012 rather than a note here.

**Iteration 3 — all items pass.**

### Clarification session 2026-10-15

Five questions asked, five answered; all integrated. Net effect on this
checklist: one previously-deferred item closed (FR-012, above), and four
areas that were Partial are now Clear.

| # | Question | Answer | Requirements added |
|---|---|---|---|
| 1 | How is the matrix persisted, and what does the gate compare against? | Declarations committed as the gate; full matrix as an uncommitted per-run artifact | FR-014a, FR-014b, FR-014c; SC-003 reworded |
| 2 | What is an invariant's durable identity under restatement? | Stable append-only numbers; meaning change retires the number, retired numbers never reused | FR-001a, FR-001b, FR-001c; FR-019 scoped to authoring time |
| 3 | Must an invariant be falsifiable in this corpus? | Yes — ≥1 applicable target, enforced by the gate, counts published | FR-006a, FR-015a, SC-010 |
| 4 | What counts as a material change in a declared observation? | Violation kind pinned exactly; count may drift | FR-010, FR-012, FR-012a; one edge case resolved |
| 5 | Which concept owns the word "invariant"? | Universal properties keep it; per-target checks become "expectations" | FR-020, FR-020a; US3 scenario 4 |

Two entities were added as a consequence: **Violation Kind**, and the
clarified **Matrix Report** / **Known-Failure Declaration** split
between what is committed and what is regenerated per run.

### Standing risks for the planner

- **SC-004 (mutation check) and SC-009 (no vacuous passes) are the
  load-bearing criteria.** No vacuity check was performed on the
  evidence (see `measurements/findings.md`), so the probe's twelve
  failures are trustworthy but its fifty-one passes are not yet
  evidence of anything. An invariant suite that cannot be made to fail
  is decoration.
- **FR-019 (restate I3 and I7) is a prerequisite, not polish.** The
  evidence shows the same defect class landing in the wrong invariant,
  which routes a fix to the wrong place. Implementing the catalog before
  the overlap is resolved bakes the misattribution in.
- **The evidence corpus is not the delivered corpus.** Nine Pico
  nightly pairs produced the matrix; the feature targets the 17-entry
  public corpus. Eight of those 17 have no measured state for any of the
  seven invariants. Plan for discovery there rather than treating the
  twelve-cell figure as the work item count.
- **Zero [NEEDS CLARIFICATION] markers was a choice**, and the
  clarification session partly vindicated and partly corrected it. Of
  the five candidate ambiguities defaulted at authoring time (exemption
  expiry, gate strictness, catalog location, initial catalog scope, lane
  placement), gate strictness turned out to need a decision after all
  and got one (Q4). The remaining four stand as Assumptions. Two
  ambiguities the authoring pass missed entirely — invariant identity
  under restatement (Q2) and the falsifiability floor (Q3) — were only
  visible once the declarations file became the committed gate in Q1,
  which is an argument for running clarify after specify rather than
  trying to pre-empt it.
- **FR-012a is a new constraint on the catalog, not just on the gate.**
  An invariant whose checker can only produce a count cannot be
  catalogued, because it cannot satisfy the kind-pinning rule. This
  should be checked against each of the seven initial invariants during
  planning; at least one of them (I2, duplicate identities) will need
  thought about what distinguishes one kind of duplication from another.
