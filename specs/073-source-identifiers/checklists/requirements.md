# Specification Quality Checklist: Source identifiers — built-in + user-defined

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-05-05
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

- This spec is intentionally narrower than milestone 072. It's the foundation layer for milestone 074 (multi-source `--bind-to-source` resolution by identifier). Without 073, the multi-source flag has no canonical source-of-identity to key on; with 073, multi-source becomes a small wiring change.
- References to milestone-072 (`SourceDocumentBinding`, `cross-tier-binding.md`, `mikebom:sbom-tier`, the parity catalog from milestone 071) are project-state facts that anchor this spec's contracts. The published milestone-072 work ships as alpha.15 — already on main.
- The four built-in schemes (`repo:`, `git:`, `image:`, `attestation:`) are an opinionated cut. Adding more later is a separate milestone; the user-defined passthrough mechanism (FR-004 + the `mikebom:source-identifiers` annotation) means the design doesn't lock out future additions.
- `/speckit.clarify` is recommended next. The spec has informed-guess defaults for several decisions worth pinning before plan-phase: (a) git remote selection algorithm (origin → upstream → first-listed?), (b) per-format carrier choices for SPDX 2.3 (`Package.externalRefs[PERSISTENT-ID]` is the closest fit but may not be operator-recognizable), (c) treatment of multiple identifiers in the same scheme (today: emit all, no dedup-by-scheme).
- All items pass on first iteration; spec is ready for clarify/plan.
