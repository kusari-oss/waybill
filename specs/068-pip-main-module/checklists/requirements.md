# Specification Quality Checklist: pip source-tree main-module component

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-05-03
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details — spec describes manifest fields (PEP 621 `[project]` table, `name`, `version`, `dynamic`), PURL shapes (PEP 503 normalization), and SBOM constructs.
- [X] Focused on user value and business needs
- [X] Written for non-technical stakeholders
- [X] All mandatory sections completed

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain — Issue #104 plus the well-trodden 053/064/066 pattern resolves all the previously-ambiguous decisions.
- [X] Requirements are testable and unambiguous — FR-001 through FR-011 each name a specific observable behavior.
- [X] Success criteria are measurable — SC-001 (100% emission), SC-002 (PEP 503 normalization correct), SC-003 (≤1pp sbomqs delta), SC-004 (byte-identity across 3 hosts), SC-005 (placeholder removed).
- [X] Success criteria are technology-agnostic
- [X] All acceptance scenarios are defined
- [X] Edge cases are identified — 10 edge cases covering Poetry deferral, dynamic version, PEP 503 denormalization, pre-release versions, local version segments, editable installs, missing-field cases, and same-PURL collisions.
- [X] Scope is clearly bounded — pip + PEP 621 only; Poetry deferred per #104; license deferred to #103; cargo/Go/npm/maven/gem out of scope.
- [X] Dependencies and assumptions identified — 11 assumptions A1–A11.

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria
- [X] User scenarios cover primary flows — US1 (P1, project identification), US2 (P2, consumer signal), US3 (P3, doc root targeting).
- [X] Feature meets measurable outcomes
- [X] No implementation details leak into specification

## Notes

This spec mirrors milestones 064 (cargo) + 066 (npm) where semantics align — same FR numbering, same C40 + multi-DESCRIBES infrastructure inherited from 053+064+#127. Pip-specific divergences:

- **PEP 503 name normalization** (FR-001 / A2): consistent with mikebom's existing `normalize_pypi_name_for_purl` helper for non-main-module pip components
- **Poetry deferral** (FR-002 / A3): explicit per #104; existing Poetry lockfile-driven dep emission is unaffected
- **Dynamic version handling** (FR-001 / Edge Cases): `0.0.0-unknown` placeholder for the rare case where `dynamic = ["version"]` defers to setuptools-scm or similar
- **Editable install merge** (FR-011 / A11): venv-discovered evidence overrides Phase-A defaults when PURLs match — Python's editable-install pattern is unique vs cargo/npm

The multi-main-module super-root + plural-DESCRIBES infrastructure shipped in #127 carries over to pip at zero marginal cost. Implementation should be smaller than 064/066 because most generator-side work is already done.

All 12 quality-checklist items pass on first iteration. Spec ready for `/speckit-plan`.
