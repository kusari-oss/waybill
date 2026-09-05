# Specification Quality Checklist: Parallel Go Source-Import Collection

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-09-04
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs) — technology names appear only in Assumptions/Motivation as evidence, not as requirements
- [X] Focused on user value and business needs — SC-001 wall-time target is user-visible; byte-identity preserves emitted-SBOM contract
- [X] Written for stakeholders with enough context to review scope
- [X] All mandatory sections completed (User Scenarios, Requirements, Success Criteria, Assumptions)

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain
- [X] Requirements are testable and unambiguous — each FR has an implementation-agnostic acceptance surface
- [X] Success criteria are measurable — SC-001 (≤ 18s / ≤ 10s), SC-002 (byte-identity), SC-003 (zero new deps), SC-004 (double-run determinism), SC-005 (≤ 3% single-workspace overhead), SC-006 (pre-pr green), SC-007 (--no-go-mod-why compat)
- [X] Success criteria are technology-agnostic where possible — wall time, byte-identity, dependency count. Some criteria (SC-006 pre-pr, SC-003 Cargo.lock) reference the project's build system, matching precedent set by m771/m773 specs.
- [X] All acceptance scenarios are defined — five per US1 covering monorepo, single-workspace, error surface, determinism, --no-go-mod-why orthogonality
- [X] Edge cases are identified — zero-workspace, single-workspace, asymmetric file counts, deep nesting, panic isolation, known_modules sharing
- [X] Scope is clearly bounded — FR-012 explicitly rules out walker unification; FR-013 rules out tokio; NFR-002 rules out degenerate overhead
- [X] Dependencies and assumptions identified — Amdahl ceiling, m771 pattern reuse, m664 walker unchanged, --include-dev orthogonality

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria — every FR maps to a Given/When/Then in the acceptance scenarios OR to a byte-identity/log-line contract
- [X] User scenarios cover primary flows — monorepo (P1), single-workspace degenerate, error/panic, determinism, flag orthogonality
- [X] Feature meets measurable outcomes defined in Success Criteria — SC-001 through SC-007 are all verifiable via existing test infrastructure or a one-shot manual bench
- [X] No implementation details leak into specification — Key Entities describe SHAPE not TYPE definitions; the m771 US2 reference is Motivation citation, not a requirement

## Notes

- The Motivation section leans heavily on the m774 profiling evidence gathered on `scratch-m774-profile`. This is deliberate — the m773 rollback taught us that specs without empirical Step 3 verification silently target the wrong subsystem. m774's decomposition table IS the Step 3 evidence.
- FR-014's `parallel_workers_used` field in the summary log is chosen so operators can attribute wall time to the pool size their host provided — useful for `available_parallelism()` debugging on containerized hosts.
- The `SharedImportState<'a>` naming mirrors m773's `ResolverSharedState` naming for team-familiarity, even though m773 was rolled back. The design lesson (fail-fast propagation via `std::thread::scope`) is preserved; only the target subsystem changed.
