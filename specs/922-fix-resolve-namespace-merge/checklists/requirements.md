# Specification Quality Checklist: Same-named resolves in different Pants namespaces

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-09-19
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

**Post-`/speckit.analyze` (2026-09-19).** Five findings resolved before
implementation:

- **D1 (CRITICAL)** — Principle V requires the audit RESULT in the spec's
  Functional Requirements, and it was only in plan.md. FR-006e added. This is
  the same defect m912 hit and fixed; doing the audit thoroughly and filing it
  in the wrong artifact is a distinct failure from not doing it.
- **C1 (HIGH)** — nothing verified that the membership annotation's key and
  shape are unchanged, which is the guarantee the entire Option-A choice rests
  on. New T009.
- **C2 / C3** — T008 widened to the corpus (SC-010 names it) and to FR-007's
  absent-not-empty rule.
- **I1** — T019/T020 were marked parallel while explicitly conditional on
  T018's finding.

**All clarifications resolved (4, session 2026-09-19).** Q1 route: a new
per-component annotation, membership left unchanged. Additive, at the cost of a catalogue row with three extractors
and corpus-golden movement on every Pants target. Rejected: qualifying the
membership values in place, which would be the second consumer-visible break
to that same key in one release cycle; and keeping the namespace internal,
which leaves the document unable to answer a question it gets asked.

Three further questions were resolved in `/speckit.clarify`: filenames and
manifest ids are namespace-qualified only on collision (FR-002a/b); C161 is out
of scope and filed as #924; the namespace annotation is emitted unconditionally
(FR-006d).

**One thing the plan must MEASURE rather than inherit (FR-006c):** whether a
component's namespace is singular. Across ecosystems it cannot be plural, but
`pkg:generic/*` entries from non-PyPI Pex sources are not obviously bounded.
A plural answer changes the annotation's shape.

**On "no implementation details":** the spec names annotation keys
(`waybill:pants-resolve`, `waybill:document-resolve`) and catalogue row C163.
These are the emitted SBOM's wire contract — the product surface a consumer
reads — not internal structure. Referring to them is the same as a web spec
naming a response field.

**Deliberately carried forward rather than re-derived:** the finding that the
grouping cannot be fixed without per-component namespace (FR-005) was
established by inspecting a merged document, not assumed. Both components
carry identical `["default"]` membership. This is the constraint that makes
the milestone bigger than its one-line appearance, and the plan should not
re-litigate it.
