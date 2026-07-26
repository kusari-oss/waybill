# Specification Quality Checklist: `--project-discovery=<mode>`

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-07-24
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

- The spec inherits two design frames from the 2026-07-24 user discussion: (1) walker-depth vs project-discovery-scope are DIFFERENT concepts (m220 is the latter — walker depth remains hardcoded per-reader for good reasons like yocto's 8-deep recipe walks); (2) per-ecosystem selective scope is deferred to a future config-file-shaped milestone.
- Three modes (`all` / `root-only` / `strict`) cover the design space per user framing. Two modes would have been enough for the primary use case (US1) but `strict` gives operators the truly-literal shallow semantic for niche audit/compliance cases.
- FR-006 explicitly punts per-ecosystem workspace-member detection to existing reader logic — this keeps m220's scope tight (no new heuristics to maintain, no new spec surface for pyproject.toml ambiguities).
- SC-005 byte-identity contract via `--project-discovery=all` default is the load-bearing invariant. Any deviation requires an explicit spec update.
- The spec cites specific waybill filesystem paths in the Assumptions section (e.g., m127 root-selector, m215 `enumerate_workspace_roots`, m219 SplitMode extensibility pattern) — this is a traceability aid for the plan phase, standard practice for waybill milestones per m216/m217/m218/m219 precedent. Not user-facing requirements.
- Items marked incomplete require spec updates before `/speckit.clarify` or `/speckit.plan`.
