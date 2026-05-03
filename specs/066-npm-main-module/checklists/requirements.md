# Specification Quality Checklist: npm source-tree main-module component

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-05-03
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details — *Spec describes manifest fields (`package.json` `name`, `version`, `private`, `workspaces`), PURL shapes, and SBOM constructs. Implementation file paths reference is via the existing reader location for clarity but no implementation choices for mikebom itself are specified.*
- [X] Focused on user value and business needs — *Each user story leads with consumer value: vuln-intersection accuracy, sbomqs scoring, GUAC ingest, npm being the highest-volume ecosystem.*
- [X] Written for non-technical stakeholders
- [X] All mandatory sections completed

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain — *Issue #104 itself answers the workspace + private+version handling. All other points have defensible defaults documented in Assumptions A1–A10.*
- [X] Requirements are testable and unambiguous — *FR-001 through FR-011 each name a specific observable behavior verifiable from the SBOM output.*
- [X] Success criteria are measurable — *SC-001 (100% emission), SC-002 (multi-main-module super-root coverage), SC-003 (≤1pp sbomqs delta), SC-004 (byte-identity across 3 hosts), SC-005 (placeholder removed).*
- [X] Success criteria are technology-agnostic
- [X] All acceptance scenarios are defined
- [X] Edge cases are identified — *11 edge cases enumerated covering private+no-version, scoped packages, workspaces, nesting, scope encoding, pre-release versions, lockfile-format-agnosticism, name edge cases, same-PURL collisions.*
- [X] Scope is clearly bounded — *npm only; cargo / Go / pip / maven / gem out of scope per A8. License detection deferred to #103. Yarn lockfile parsing remains out-of-scope (consistent with current mikebom posture).*
- [X] Dependencies and assumptions identified — *10 assumptions A1–A10.*

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria
- [X] User scenarios cover primary flows — *US1 P1 (project identification, the main value), US2 P2 (consumer signal), US3 P3 (doc root targeting).*
- [X] Feature meets measurable outcomes
- [X] No implementation details leak into specification

## Notes

This spec deliberately mirrors the structure of milestone 064 (cargo) where semantics align — same FR numbering, same C40 + multi-DESCRIBES infrastructure, same license-deferral posture. Key npm-specific divergences:

- **Scope encoding** (FR-001): `@scope/name` → `pkg:npm/%40scope/name@version` per PURL spec
- **`private` flag handling** (FR-001 / A2 / A3): npm-only signal that distinguishes "not a publishable artifact" from "monorepo-root publish guard"
- **`node_modules/` exclusion** (FR-003 / A5): explicit ecosystem-specific divergence from cargo's "emit excluded crates"
- **No version-inheritance feature** (A1): npm has no `version.workspace = true` equivalent, so no resolver ladder beyond literal-or-placeholder

The multi-main-module super-root + plural-DESCRIBES infrastructure shipped in #127 carries over to npm at zero marginal cost; that's the leverage of the C40-tag-driven generator hooks established by milestones 053 + 064.

All 12 quality-checklist items pass on first iteration. Spec is ready for `/speckit-plan`.
