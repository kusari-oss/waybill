# Specification Quality Checklist: m673 Pants lockfile-discovery layout extensions

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-09-02
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs) — FR-005 mentions `std::fs::canonicalize` as an anchor to the existing m672 semantics; FR-003 quotes the `^2\.` regex as the accept-criterion inherited verbatim from m223. Both are behavioral contracts, not new implementation choices.
- [X] Focused on user value and business needs — every user story frames the outcome for an SRE/operator against the specific gap uncovered by the 2026-09-02 smoke test.
- [X] Written for non-technical stakeholders where possible — "Pants 2.31+ default layout" is domain-technical but unavoidable given the target audience; motivations are stated in plain English.
- [X] All mandatory sections completed (User Scenarios & Testing, Requirements, Success Criteria; Key Entities section included because the PEX-content-signature discriminator is a new concept worth surfacing).

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain.
- [X] Requirements are testable and unambiguous — every FR names a concrete triggering condition + a concrete required behavior. Synthetic-fixture + integration-test coverage is straightforward.
- [X] Success criteria are measurable — SC-001 through SC-006 all cite specific counts, byte-identity, or byte-identity-of-log-line-counts.
- [X] Success criteria are technology-agnostic where possible — SC-005 cites "byte-identical to pre-m673 output" which is verifiable via a diff, not a stack choice.
- [X] All acceptance scenarios are defined — each user story has 3 Given-When-Then scenarios covering happy path + edge cases + the sibling-file case.
- [X] Edge cases are identified — 6 edge cases named across overlapping paths, subdirectory nesting, `[python.resolves]` overrides, binary content, Pex 1.x, and symlinks.
- [X] Scope is clearly bounded — spec explicitly excludes recursive discovery below `lockfiles/` (FR-009), non-lowercase directory names, and Pex 1.x support (retained m223 behavior).
- [X] Dependencies and assumptions identified — 7 assumptions covering the canonical-layout set, non-recursive discovery rationale, content-detection robustness, byte-identity invariants, silent-skip UX rationale, and symlink behavior.

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria — each FR maps to at least one Given-When-Then scenario or edge-case bullet.
- [X] User scenarios cover primary flows — US1 (repo-root), US2 (`lockfiles/`), US3 (content-detection guard). Both P1 stories are independently testable via synthetic fixtures; US3 is a defensive P2 that guards US1+US2's wider blast radius.
- [X] Feature meets measurable outcomes defined in Success Criteria — the 6 SCs align with the 3 user stories (SC-001 ↔ US1, SC-002 ↔ US2, SC-003+SC-004 ↔ US3, SC-005 covers the byte-identity gate that guards every story).
- [X] No implementation details leak into specification — FRs stay behavioral; the two implementation-anchoring mentions (canonicalize semantics, `pex_version` accept regex) reference pre-existing contracts we're extending, not new implementation choices.

## Notes

- Items marked incomplete require spec updates before `/speckit.clarify` or `/speckit.plan`. All items pass on first review.
- **Motivation is grounded in observed behavior**: `pantsbuild/example-python` and `pantsbuild/example-django` were both cloned + scanned on 2026-09-02 to confirm the gap. Impact quantification (100% miss on transitive detail in example-python) is empirical.
- **US3 content-detection is a critical defensive requirement** — without it, extending discovery to `<repo-root>/*.lock` would false-positive-WARN on every Rust repo's `Cargo.lock`. The `pex_version` field is the standards-native discriminator inherited from m223 unchanged.
- **v2 extension points documented in-line**: recursive `lockfiles/<team>/*.lock` (FR-009), non-lowercase directory variants (Assumptions), table-shape `[python.resolves]` (unchanged from m672).
- **2026-09-02 clarify session** resolved one spec-level ambiguity: content-detection is scoped to the new FR-001/FR-002 wide-scope paths (repo-root + `lockfiles/`), NOT the narrow m223 `3rdparty/python/` default glob or explicit `pants.toml` override paths — those retain m223 WARN-and-skip on parse failure because a WARN in those Pants-owned locations catches genuine operator mistakes.
- Ready to proceed to `/speckit.plan`.
