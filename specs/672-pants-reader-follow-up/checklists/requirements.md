# Specification Quality Checklist: m223 Pants pex-lockfile reader follow-up

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-09-01
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs) — spec cites `serde_json::from_slice` and `std::fs::canonicalize` in the Assumptions section only as anchors to existing m223 code the reader IS being extended against; the functional requirements themselves stay in "the reader MUST tolerate / recognize / dedup" form. This is the same convention the m671 spec used when citing the existing `classify()` function.
- [X] Focused on user value and business needs — every user story frames the outcome for an SRE/operator, not the shape of the code.
- [X] Written for non-technical stakeholders — Pants ≤ 2.29 vs 2.30+ front-matter shape is domain-technical but unavoidable given the target audience; the "why this priority" text explains the user pain in plain English.
- [X] All mandatory sections completed (User Scenarios & Testing, Requirements, Success Criteria; the Key Entities section is included because the front-matter block, resolve entry, and legacy counter are first-class data concepts).

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain.
- [X] Requirements are testable and unambiguous — every FR names a concrete triggering condition + a concrete required behavior. Golden-fixture tests + integration tests can verify each.
- [X] Success criteria are measurable — SC-001 through SC-007 all cite counts, byte-identity, or bounded latency.
- [X] Success criteria are technology-agnostic — no mention of clap, serde, TOML libraries, or Rust idioms.
- [X] All acceptance scenarios are defined — each user story has 2–4 Given-When-Then scenarios covering happy path + failure modes.
- [X] Edge cases are identified — 7 edge cases named across malformed metadata blocks, embedded `//` in JSON strings, duplicate resolves, non-string values, disagreeing overrides, whitespace-only prefixes, and large files.
- [X] Scope is clearly bounded — spec explicitly excludes the `.lock.metadata` SIDECAR shape (2.30+) and the Inspector 2,000-change ceiling.
- [X] Dependencies and assumptions identified — 8 assumptions covering Pants version boundaries, resolve-name safety, canonicalization semantics, and the FR-013 annotation-channel reuse.

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria — each FR maps to at least one Given-When-Then scenario or edge-case bullet.
- [X] User scenarios cover primary flows — US1 (legacy shape), US2 (`[python.resolves]`), US3 (diagnostic log). Both P1 stories are independently testable via synthetic fixtures.
- [X] Feature meets measurable outcomes defined in Success Criteria — the 7 SCs align with the 3 user stories (SC-001 ↔ US1, SC-002 ↔ US2, SC-005/SC-006 ↔ US3, SC-003 covers the byte-identity gate that guards every story).
- [X] No implementation details leak into specification — FRs stay behavioral; the two Assumptions-section mentions of `serde_json::from_slice` and `std::fs::canonicalize` reference the pre-existing m223 code the extension attaches to, not new implementation choices.

## Notes

- Items marked incomplete require spec updates before `/speckit.clarify` or `/speckit.plan`. All items pass on first review.
- The single-lockfile Altana orphan (`python-default.pants.lock` with empty `locked_resolves` at repo root, `//`-shape) is documented as US1 Acceptance Scenario 3. It's a no-op for component emission (0 components) but a real test case for the `//` stripper.
- **2026-09-01 clarify session** resolved three ambiguities: (1) FR-013 is log-line-only in v1 (annotation deferred to v2), (2) `[python.resolves]` supports bare-string values only in v1 (table-shape WARNs and skips), (3) prefix stripper runs uniformly on every lockfile (no retry-on-failure branching).
- Ready to proceed to `/speckit.plan`.
