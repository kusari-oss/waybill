# Specification Quality Checklist: Single-Flight Go Preflight + go.work Directive Tolerance

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-09-05
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs) — requirements state coordination *properties* ("at most once per scope", "waiters reuse the outcome", "distinct scopes proceed concurrently") rather than naming a primitive. Motivation cites file paths and measurements as evidence, which is the perf-methodology-mandated Step 2/3 record, not requirement text.
- [X] Focused on user value and business needs — SC-001 is operator-visible wall time; SC-002 protects the emitted-SBOM contract; SC-008 fixes an operator-facing annotation that is currently wrong.
- [X] Written for stakeholders with enough context to review scope
- [X] All mandatory sections completed (User Scenarios, Requirements, Success Criteria, Assumptions)

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain
- [X] Requirements are testable and unambiguous — each FR has an observable acceptance surface (subprocess count, outcome propagation, annotation value, byte-identity)
- [X] Success criteria are measurable — SC-001 (≤21s), SC-003 (≤6 invocations), SC-004 (≥90% reduction), SC-009 (±3%), plus binary criteria SC-002/005/006/007/008
- [X] Success criteria are technology-agnostic where the domain permits — wall time, invocation counts, byte-identity, dependency count. SC-006/SC-007 reference the project's build and verification surface, matching m771/m774 precedent for this repository.
- [X] All acceptance scenarios are defined — five for US1 (single scope, failure propagation, cross-scope independence, loose fallback, byte-identity), five for US2 (godebug, toolchain, genuinely-malformed still reported, unchanged pre-existing input, diff containment)
- [X] Edge cases are identified — panic, budget exhaustion mid-flight, single-workspace, zero go.work, nested go.work, repeated scans, godebug-without-use, duplicate directives
- [X] Scope is clearly bounded — FR-008/009 rule out new operator surface and dependencies; Assumptions explicitly exclude sub-workspace import parallelization and future-directive proofing
- [X] Dependencies and assumptions identified — reference hardware, fixture revision, preflight-count arithmetic, the independence of the two defects, the `go mod why` non-regression note, and the scratch branch's reference-only status

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria — FR-001..006 map to US1 scenarios 1–4; FR-007 to US1 scenario 5; FR-011..014 to US2 scenarios 1–4; FR-008/009/010 are verifiable by inspection of the operator surface, lockfile, and log shape
- [X] User scenarios cover primary flows — shared scope, failed preflight, multi-scope concurrency, loose workspace, and both parser-tolerance directions
- [X] Feature meets measurable outcomes defined in Success Criteria — every SC is verifiable with existing infrastructure plus the subprocess-counting shim already built during profiling
- [X] No implementation details leak into specification — Key Entities describe roles and coordination semantics, not concrete types or synchronization primitives

## Notes

- The Motivation section carries the full Step 1–3 empirical record (per-phase table, subprocess-level instrumentation, and an inclusion/exclusion verification build with before/after measurements). This is deliberate: m772 and m773 were both rolled back at implement-time for targeting phases that per-phase logs mis-attributed, and m774 codified the requirement that a perf spec cite Step 3 evidence. m775's stampede finding came specifically from *not* trusting the phase log — the classifier's 13.7s did not reconcile with a 0.17s per-invocation measurement, and the subprocess shim resolved the gap.
- US2 is included because it was surfaced by the same profiling pass and lives in the same file family, but it is deliberately P2 and independently testable. If US1 needs to ship alone, US2 can be split without rework.
- SC-003 is stated as an upper bound rather than an exact count. The scratch build observed 5 preflights where naive arithmetic predicted 6; the discrepancy turns on how the `go.work` root `.` entry resolves against discovered workspaces, and pinning an exact number would make the criterion fixture-revision-fragile without adding rigor.
- FR-014 (both parsers must agree) is the requirement that actually prevents recurrence. Fixing only the directive list would leave two independent parsers free to diverge again on the next `go.work` format addition.
