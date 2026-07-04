# Specification Quality Checklist: Go workspace-mode false dep-graph edges (milestone 161)

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-07-04
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs) — spec cites `mikebom-cli/src/scan_fs/package_db/golang/` as the containing module for the FR-007 fix, but names emission behavior + annotation names, not fix mechanics. FR-007a/b/c prescribe root-cause classes rather than exact code changes.
- [X] Focused on user value and business needs — the SBOM consumer (vulnerability scanner, compliance auditor) gets accurate dep-graph edges on go.work-based repos. The 30.8% pre-161 wrong-edge rate is the ROI narrative — vulnerability scans currently amplify wrong edges into spurious CVE findings.
- [X] Written for non-technical stakeholders — outcomes phrased as "consumer scanning for CVE gets a real match" not "fix the multi-go.mod walker's attribution loop." The Motivation section names the concrete audit shape + 3 specific false edges from Kubernetes.
- [X] All mandatory sections completed — User Scenarios + Requirements + Success Criteria + Assumptions + Out of Scope all present.

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain — the empirical shape is well-known from the milestone-157 audit + the 2026-07-03 measurement. FR-007's investigate-first sub-requirements (FR-007a/b/c) codify what to look for during T014-T016 empirical investigation. Q candidates for /speckit-clarify identified below (LOW-impact).
- [X] Requirements are testable and unambiguous — each FR names a concrete emitted-artifact behavior; each SC names a verification method (per-module edge-count comparison, jq-inspectable annotation presence, byte-diff, integration test).
- [X] Success criteria are measurable — SC-001 = ≤ 5% wrong edges (from pre-161 baseline 30.8%); SC-002 = 3 specific false edges MUST NOT appear; SC-004 = 100% workspace-mode annotation presence; SC-005 = use-count matches parsed go.work; SC-006 = zero v0.0.0-unknown workspace-internal targets; SC-009 = ≥10 unit tests; SC-010 = new integration test; SC-013 = closes #495.
- [X] Success criteria are technology-agnostic — outcomes phrased in SBOM-consumer terms (edge accuracy, annotation presence, byte-identical outputs). CDX / SPDX 2.3 / SPDX 3 named as inputs to the format-parity check, not as implementation choice.
- [X] All acceptance scenarios are defined — US1 has 4 Given/When/Then scenarios; US2 has 3; US3 has 2. Total: 9 acceptance scenarios.
- [X] Edge cases are identified — 7 edge cases enumerated: empty use block, single-entry use, nested go.work, go.work replace directives, workspace-root has own go.mod, v0.0.0-unknown tell, GOWORK=off.
- [X] Scope is clearly bounded — Out of Scope enumerates 6 explicit exclusions (issues #496/#498 named, transitive-coverage extension, cross-workspace attribution, go.work.sum verification, walker changes).
- [X] Dependencies and assumptions identified — 9 Assumptions covering `go mod graph` per-module ground-truth authority, online-mode primary target, `test-kubernetes` empirical benchmark, no new Cargo deps, `golang` fixture unchanged, new `golang-workspace` fixture needed, go.work grammar spec-defined, empirically-adjustable SC-001, downstream completeness-signal drift as legitimate.

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria — FR-001..FR-011 each map to a US1/US2/US3 acceptance scenario, an SC, or both. Zero orphaned FRs.
- [X] User scenarios cover primary flows — US1 (P1 workspace-attribution fix) is the primary bug fix; US2 (P2 document-scope workspace-mode signal) is the transparency mechanism; US3 (P3 non-workspace byte-identity) is the regression guard.
- [X] Feature meets measurable outcomes defined in Success Criteria — SC-001 (per-module wrong-edge ratio), SC-002 (specific 3 false edges), SC-003 (byte-identity), SC-004 (annotation presence), SC-005 (annotation value correctness), SC-006 (workspace-internal v0.0.0-unknown suppression), SC-007 (non-workspace regression guard), SC-008 (pre-PR gate), SC-009 (unit test floor ≥10), SC-010 (integration test), SC-011 (CHANGELOG), SC-012 (parity catalog), SC-013 (closes #495).
- [X] No implementation details leak into specification — `mikebom-cli/src/scan_fs/package_db/golang/` reference in FR-007a is a code anchor for the multi-go.mod walker being extended; describes WHERE the semantic lives, not HOW it's implemented.

## Notes

- All 16 checklist items pass on first authoring pass.
- The fix shape is investigation-heavy: FR-007a/b/c prescribe root-cause classes to look for during empirical investigation. Unlike milestones 157/158/159, we can't know the exact fix before scanning + comparing. The plan phase will need a discovery-heavy Phase 2 mirroring milestone 160's T014-T016 shape.
- The 3 concrete false edges from milestone-157's Round-3 audit are the load-bearing SC-002 evidence. Zero false edges is the acceptance signal.
- Building on the milestone-158/160 vocabulary pattern: 3-value document-scope annotation (`detected: N` | `absent` | `malformed: <reason>`) with FR-005 grammar. Consumer tooling gating on workspace-mode is a natural extension.
- SC-003 dual-side byte-identity guard verified achievable pre-authoring: 10 non-Go milestone-090 fixtures × 3 formats = 30 goldens byte-identical; PLUS the single-module `golang` fixture × 3 = 3 more byte-identical (this milestone doesn't change single-module behavior). Total 33 byte-identical goldens.
- Ready for `/speckit-clarify` OR `/speckit-plan`. Two candidate clarifications identified but neither blocking:
  - Q1 candidate: FR-007b's `v0.0.0-unknown` false-edge disposition — SUPPRESS the edge entirely (matches FR-002 truthful per-module attribution) OR RESOLVE to sibling's real version and preserve the edge (may be legitimate if the source module DOES require the target)? Recommended default: **SUPPRESS by default; RESOLVE only if the target module IS explicitly named in the source's own require block**. Matches Q1-caution-first from milestone 158.
  - Q2 candidate: The `mikebom:go-workspace-mode` value shape when `go.work` present but zero `use` entries — emit `detected: 0 use-modules` (transparency-through-explicit-zero) OR emit `malformed: empty-use-block` (defensive)? Recommended default: **`detected: 0 use-modules`** — the file is syntactically valid, just semantically empty. Users' intentions differ from parse failures.
- Both are LOW-impact questions — could be resolved at `/speckit-clarify` or deferred to plan-time.
