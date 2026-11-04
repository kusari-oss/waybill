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

## Clarify session 2026-11-04

Three questions asked, three answered; `## Clarifications` holds one bullet per
accepted answer. All 16 checklist items re-validated after integration and
still pass. Sections touched: Clarifications (new), Functional Requirements
(FR-009 rewritten, FR-012 to FR-014 added), User Story 4 acceptance scenarios,
Edge Cases, Key Entities, Success Criteria (SC-009 amended, SC-011 to SC-014
added), Assumptions.

The scan found three Partial categories and nothing Missing. All three were
asked and resolved:

| Taxonomy category | Before | After |
|---|---|---|
| Edge cases & failure handling | Partial — FR-009 named no surface | Resolved — document-scope disclosure, not a log line |
| Integration & external dependencies | Partial — a format version beyond 4 had no defined behaviour | Resolved — version-agnostic reads, version recorded |
| Functional scope & behaviour | Partial — the guard's breadth was undefined | Resolved — reports every unconsulted store-path field, diagnostic only |

Everything else scanned Clear: domain and data model, interaction flow,
non-functional attributes, constraints and tradeoffs, terminology, completion
signals, and placeholders.

One wording fix during integration: SC-011 and an assumption initially named
internal machinery (the parity extractor). Both were reworded to state the
property being checked — a documented field that nothing emits is a failure —
since a success criterion should describe the outcome, not the mechanism. This
is a narrower standard than the one applied to field and annotation names,
which stay because they are the published contract a consumer reads.

**SC-013 is deliberately unpredicted.** It asks the disclosure to name every
store-path field classification does not consult, and states no expected count.
That number is #1346's coverage gap, and this milestone measures it rather than
assuming it — quoting a figure here would be the derived-arithmetic-as-
measurement error the repository's rules name explicitly.
