# Specification Quality Checklist: Split-mode grouping strategies

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-07-23
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

- The spec surfaces two plan-phase decisions rather than pinning them now: (a) `SplitManifest` schema evolution — additive-optional `members` field vs v2 bump; (b) filename convention for multi-member groups — `<dir-slug>.multi` vs `<dir-slug>.<hash>` vs `<combined-hash>`. Both are documented in Assumptions with the tradeoff frames; the plan phase (or `/speckit-clarify`) locks them.
- `SplitMode` enum-with-method (not trait-object) is a deliberate design choice per Assumptions to keep the extension surface compile-time cheap; alternatives (Box<dyn> trait, function-pointer table) rejected for that reason.
- SC-005 backward-compat contract is the load-bearing invariant: bare `--split` and `--split=workspace` must be byte-identical to alpha.67 output. This forbids any change to m215's per-workspace split-manifest schema for the default path.
- The spec cites specific waybill filesystem paths in Assumptions + entities (e.g., `waybill-cli/src/generate/split.rs`, `SubprojectRoot::subproject_id()`) — this is a traceability aid for the plan phase, standard practice for waybill milestones per m216/m217/m218 precedent. Not user-facing requirements.
- Items marked incomplete require spec updates before `/speckit.clarify` or `/speckit.plan`.
