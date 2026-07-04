# Specification Quality Checklist: Go transitive-edge coverage investigation + gap surface (milestone 160)

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-07-04
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs) — spec cites `graph_resolver.rs:229` (existing `LadderCounters`) as the starting point for FR-001 per-module instrumentation; other FRs describe emitted-artifact behavior (annotation names, values, per-format shape). Standards-native precedence acknowledged in FR-008.
- [X] Focused on user value and business needs — the SBOM consumer (vulnerability scanner, compliance auditor) gets ≥90% edge coverage on `test-podman` (up from 52.2%). The 47.8% pre-160 mismatch is the ROI narrative — vulnerability scans currently silently miss half the closure.
- [X] Written for non-technical stakeholders — outcomes phrased as "consumer scanning for CVE gets a match" not "wire the go.sum fallback loop." The Motivation section names the concrete audit shape + 5 specific missing edges from `containernetworking/plugins@v1.9.1`.
- [X] All mandatory sections completed — User Scenarios + Requirements + Success Criteria + Assumptions + Out of Scope all present.

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain — the empirical shape is well-known from the milestone-157 audit + the 2026-07-03 measurement. FR-006's investigate-first sub-requirements (FR-006a/b/c) codify what to look for during T014-T016 empirical investigation.
- [X] Requirements are testable and unambiguous — each FR names a concrete emitted-artifact behavior; each SC names a verification method (edge-count comparison, jq-inspectable annotation presence, byte-diff, integration test).
- [X] Success criteria are measurable — SC-001 = ≥90% edge match (from pre-160 baseline 52.2%); SC-002 = 5 specific missing edges present OR annotated with reason; SC-004 = 100% per-component annotation universal presence; SC-005 = 100% document-scope; SC-008 = ≥10 unit tests; SC-009 = new integration test; SC-012 = closes #494.
- [X] Success criteria are technology-agnostic — outcomes phrased in SBOM-consumer terms (edge-count percentages, annotation presence, byte-identical outputs). CDX / SPDX 2.3 / SPDX 3 named as inputs to the format-parity check, not as implementation choice.
- [X] All acceptance scenarios are defined — US1 has 4 Given/When/Then scenarios; US2 has 4; US3 has 2. Total: 10 acceptance scenarios.
- [X] Edge cases are identified — 7 edge cases enumerated: empty-closure, GOPRIVATE modules, GOPROXY chain with `off`, `go mod graph`-vs-mikebom closure discrepancy, network flake vs 404 disambiguation, `go` binary unavailable to auditor, build-tag boundary.
- [X] Scope is clearly bounded — Out of Scope enumerates 7 explicit exclusions (issues #495/#496/#498 named, build-tag-aware filtering, alternative proxy backends, `vendor/` scanning, cross-scan aggregation).
- [X] Dependencies and assumptions identified — 8 Assumptions covering `go mod graph` ground-truth authority, online-mode primary target, `test-podman` empirical benchmark, build-tag filtering explicitly out-of-scope, retry/concurrency preservation, no new Cargo deps, empirically-adjustable SC-001, milestone-090 golang fixture will change.

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria — FR-001..FR-010 each map to a US1/US2/US3 acceptance scenario, an SC, or both. Zero orphaned FRs.
- [X] User scenarios cover primary flows — US1 (P1 edge coverage fix) is the primary bug fix; US2 (P2 document-scope coverage signal) is the transparency mechanism; US3 (P3 non-Go byte-identity) is the regression guard.
- [X] Feature meets measurable outcomes defined in Success Criteria — SC-001 (edge match %), SC-002 (specific 5 missing edges), SC-003 (dual-side byte-identity), SC-004 (per-component universal presence), SC-005 (document-scope universal presence), SC-006 (offline signals unknown), SC-007 (pre-PR gate), SC-008 (unit test floor ≥10), SC-009 (integration test), SC-010 (CHANGELOG), SC-011 (parity catalog), SC-012 (closes #494).
- [X] No implementation details leak into specification — `graph_resolver.rs:229` reference in FR-001 is a code anchor for the existing `LadderCounters` being extended; describes WHERE the semantic lives, not HOW it's implemented.

## Notes

- All 16 checklist items pass on first authoring pass.
- The fix shape is investigation-heavy: FR-006a/b/c prescribe root-cause classes to look for during empirical investigation. Unlike milestones 157-159, we can't know the exact fix before scanning + comparing. The plan phase will need a discovery-heavy Phase 2.
- The 5 concrete missing edges from milestone-157's audit are the load-bearing SC-002 evidence. If T014-T016 investigation shows some are legitimately platform-filtered, the spec allows annotating them per FR-006's build-tag-filtered fallback.
- Building on the milestone-158 vocabulary pattern: 3-value document-scope annotation (`complete` | `partial` | `unknown`) + companion reason string with `<code>: <detail>[; ...]` grammar. Consumer tooling gating on Go-transitive coverage is a natural extension.
- SC-003 dual-side byte-identity guard is verified achievable pre-authoring: 10 non-Go milestone-090 fixtures × 3 formats = 30 goldens byte-identical; the milestone-090 golang fixture changes (new annotations expected).
- Ready for `/speckit-clarify` OR `/speckit-plan`. Two candidate clarifications identified but neither blocking:
  - Q1 candidate: Should the coverage-threshold for `partial` vs `unknown` be a fixed number (e.g. <50% resolved = unknown, 50-95% = partial) OR based on the presence of specific reason codes? Recommended default per FR-005: reason-code-driven (offline mode = unknown regardless of count; fetch-failures = partial).
  - Q2 candidate: For the FR-002 per-component `mikebom:go-transitive-source` annotation, should the emission be UNIVERSAL (every Go component) OR SIGNAL-ONLY (only when the source is `proxy-fetch` or later)? Recommended default per SC-004: universal — matches milestone-158's C104 universal presence pattern.
- Both are LOW-impact question — could be resolved at `/speckit-clarify` or deferred to plan-time.
