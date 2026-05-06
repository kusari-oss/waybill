# Specification Quality Checklist: Operator-overridable root component name and version

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-05-06
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

- Smallest milestone yet: two new CLI flags (`--root-name`, `--root-version`), four production files touched (one per per-format builder + the CLI parsing site), no new types, no new modules. Estimated <2 days.
- Three user stories: US1 (P1) source-tier override, US2 (P2) image/build-tier override, US3 (P2) override-vs-manifest precedence. The US2/US3 scope is small additions on top of US1 — same flags, different call sites get them.
- Constitution Principle V audit: zero new `mikebom:*` annotations introduced. The flags change the *value* of an existing standards-native field (`metadata.component.name` / `version`); derived fields (`bom-ref`, `purl`, `cpe`) flow through the existing pipeline unchanged.
- Bounded scope: only the root component. Per-component name/version overrides + CPE vendor override + per-component PURL overrides are explicit non-goals. Operator escape hatch for those continues to be `mikebom sbom enrich` post-processing.
- Determinism (FR-010, SC-009) inherited trivially: the flags carry constant values across re-runs; the existing emission pipeline is already deterministic.
- All items pass on first iteration; spec is ready for `/speckit.plan` (no `/speckit.clarify` needed — every spec-level decision has an obvious answer or a stated assumption).
- **Post-`/speckit.clarify` integration (2026-05-06)**: applied two clarifications — (Q1) `--root-name` validation is permissive (reject only whitespace/control/`?`/`#`; URL-encode the rest at PURL emission); (Q2) override is a clean replacement (manifest-derived main-module dropped entirely from emitted SBOM). Updated FR-006, FR-008, SC-006, SC-008, edge cases, and assumptions. The Q2 follow-up "demote-to-library" exploration tracked as GitHub issue #151.
- **Post-`/speckit.analyze` remediation pass (2026-05-06)**: applied finding-driven edits to address C1 (HIGH — clarified plan.md project-structure that `cli/generate.rs` is back-compat field-update only, NOT a flag-wiring target, matching research §6 + spec assumptions; T001(a) audit description tightened to reinforce this), C2 (MEDIUM — reframed T011(b) build-tier override test from "invoke `auto_detect_build_tier_identifiers`" to "construct synthetic `ScanArtifacts` with `BuildTimeTrace` context and pass to per-format builders directly", mirroring milestone 074/075's testing pattern), C3 (MEDIUM — renamed T010(d) from `no_flags_byte_identical_to_alpha17` to `no_flags_emits_basename_derived_name` with positive assertions on the auto-derived name/version/bom-ref/purl values), U1 (LOW — added 4 unit tests for `is_active()` to T002), C4 (LOW — broadened T012(c) orthogonality test from "subject-hash only" to a multi-flag scan exercising `--root-name` + `--root-version` + `--repo` + `--subject-hash` + `--component-id` simultaneously, verifying all five identifier surfaces coexist independently). All findings resolved; no `[NEEDS CLARIFICATION]` markers remain.
