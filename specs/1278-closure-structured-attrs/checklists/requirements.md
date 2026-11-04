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

## Analyze session 2026-11-04

Eight findings, no CRITICAL, no constitution violations. All eight remediated;
coverage is now 32/32 requirements with at least one task (17 FR, 15 SC).

| ID | Severity | What it was | How it was closed |
|---|---|---|---|
| F1 | HIGH | FR-009 and SC-011 required the disclosure "present in all three formats with equal content" while FR-015 had SPDX 2.3's payload carry the fact the other two put natively. Both could not hold, and it decided the parity row's directionality. | FR-015 now requires an **identical payload** in all three, with SPDX 2.3's *presence* carrying the fact rather than an extra field. Recorded in `contracts/read-outcome.md` with the reasoning: a format-specific field would force the row to `PresenceOnly` and give up the equal-content check for a redundancy. |
| F2 | HIGH | SC-012 had no task. The format version was produced (T008/T009) and emitted (T034) with nothing asserting it. | New **T051**, appended rather than inserted so no existing ID moved and no citation broke. Its out-of-sequence number is explained in the dependency section. SC-012 now also says "asserted by a test rather than inferred". |
| F3 | MEDIUM | Both requirement lists were out of numeric order after the clarify and plan insertions — FR-015 sat between FR-009 and FR-012, and SC-010 sat last. | Both reordered numerically. Reordering only, never renumbering, so every task citation still resolves — verified mechanically after the edit. |
| F4 | MEDIUM | T043, T045 and T047 did work no requirement asked for, so nothing failed if they were skipped. | Added **FR-016** (the refresh prediction is committed before the dispatch and scored after) and **FR-017** (invariant declarations are reconciled with the refreshed goldens). The three tasks now cite them. |
| F5 | MEDIUM | The plan's source tree omitted five files tasks modify, including `src/resolve/closure_role_scope.rs` — which sits outside the module research R6 names as the confinement boundary that SC-009's byte-identity claim rests on. | All five added. R6 now names that file explicitly and states why confinement still holds (the pass acts only on components carrying closure-role data, which exist only under `--nix-closure`) **and** what to do if a task finds itself changing rather than confirming it: the argument is invalidated and SC-009 needs re-checking against every target. |
| F6 | MEDIUM | SC-005 fixes exact per-direction transition counts; T021 asserted the census and the nine components but not the directions, so a right total by wrong transitions would pass. | T021 now asserts the transition table per direction plus "nothing moves toward `unreferenced`". |
| F7 | LOW | T012 onward create a new integration test crate; only T011 mentioned the `clippy::unwrap_used` allow, and that was for the in-module test. Verified against the sibling `tests/closure_role_scope.rs`, which carries the allow at crate level with 23 `.unwrap()` calls. | T012 is now explicitly the task that creates the file with its crate-level attributes, including whether `#![cfg(unix)]` applies — and, if so, gating the whole file rather than shimming a non-unix early return. |
| F8 | LOW | Terminology drift: the spec says "flat map" / "typed attribute object"; everything downstream says `env` / `structuredAttrs`. | A glossary line in Key Entities stating that both names refer to the same two carriers, and why the spec prefers the reader-facing ones. |

Two of these were worth the pass on their own. **F1** would not have surfaced
until the parity test failed, after the extractors were written — the
artifacts disagreed about something only an implementer would discover. **F2**
is the sharper one: a requirement about recording provenance was itself going
to ship unverified, in a milestone whose whole premise is that unverified
things look identical to correct ones.

All 16 original checklist items re-validated after remediation and still pass.
Task format re-validated: 51 tasks, unique IDs, story labels only in story
phases, every description naming a path.
