# Specification Quality Checklist: npm workspace-peer phantom empty-version edges (milestone 163)

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-07-05
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs) — spec cites `mikebom-cli/src/scan_fs/package_db/npm/*.rs` as the module family for the FR-001 cross-resolution fix, but names emission behavior (edge targets, annotation names, per-format wire shape) rather than fix mechanics.
- [X] Focused on user value and business needs — the SBOM consumer (vulnerability scanner, graph-analysis tool) gets ≥99% BFS reachability on test-podman-desktop (up from 24.6%). Fixes milestone-158's load-bearing blocker for the aspirational ≥99% target. The 902 phantom edges are the ROI narrative — currently pointing to non-existent PURLs.
- [X] Written for non-technical stakeholders — outcomes phrased as "consumer's BFS reaches ≥99% of components" not "fix the workspace-peer reader's cross-resolution lookup". The Motivation section names the concrete audit shape + 3-tool comparison (mikebom vs Trivy vs Syft).
- [X] All mandatory sections completed — User Scenarios + Requirements + Success Criteria + Assumptions + Out of Scope all present.

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain — the fix shape is well-understood (cross-resolve against top-level lockfile). Two candidate clarifications identified for /speckit-clarify but neither blocking; recommended defaults are defensible.
- [X] Requirements are testable and unambiguous — each FR names a concrete emitted-artifact behavior; each SC names a verification method (BFS traversal ratio, jq-inspectable PURL shape, annotation presence, byte-diff, integration test).
- [X] Success criteria are measurable — SC-001 = ≥99% BFS reachability (from 24.6% baseline); SC-002 = zero phantom edges (from 902 baseline); SC-004 = zero empty-version PURLs (from 159 baseline); SC-005 = ≥2835 npm components preserved (coverage advantage over Trivy); SC-007 = ≥10 unit tests; SC-008 = new integration test; SC-011 = closes #498.
- [X] Success criteria are technology-agnostic — outcomes phrased in SBOM-consumer terms (BFS reachability, PURL shape, edge disposition). CDX / SPDX 2.3 / SPDX 3 named as inputs to the format-parity check, not as implementation choice.
- [X] All acceptance scenarios are defined — US1 has 4 Given/When/Then scenarios; US2 has 3; US3 has 2. Total: 9 acceptance scenarios.
- [X] Edge cases are identified — 7 edge cases enumerated: unresolvable dep, multiple nested versions, peer-dep vs regular-dep, range spec mismatch, npm alias syntax, root package.json also declares, version-less DEPENDENCIES reference.
- [X] Scope is clearly bounded — Out of Scope enumerates 6 explicit exclusions (Yarn Berry, peer-dep, nested-node_modules deep walk, workspace-peer aliases, cross-repo, semver range comparison).
- [X] Dependencies and assumptions identified — 8 Assumptions covering top-level lockfile as ground truth, test-podman-desktop as benchmark, nested node_modules as rare edge case, Yarn Berry OOS, no new Cargo deps, milestone-090 npm fixture may change, SC-001 empirically-adjustable, investigation-guided (not empirical loop).

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria — FR-001..FR-010 each map to a US1/US2/US3 acceptance scenario, an SC, or both. Zero orphaned FRs.
- [X] User scenarios cover primary flows — US1 (P1 BFS reachability + zero phantom edges) is the primary bug fix; US2 (P2 cross-resolution mechanism) is the underlying mechanism verification; US3 (P3 non-npm byte-identity) is the regression guard.
- [X] Feature meets measurable outcomes defined in Success Criteria — SC-001 (BFS reachability %), SC-002 (phantom edge count), SC-003 (byte-identity), SC-004 (zero empty-version PURLs invariant), SC-005 (coverage-preservation), SC-006 (pre-PR gate), SC-007 (unit test floor ≥10), SC-008 (integration test), SC-009 (CHANGELOG), SC-010 (parity catalog), SC-011 (closes #498).
- [X] No implementation details leak into specification — `mikebom-cli/src/scan_fs/package_db/npm/*.rs` reference in FR-001 is a code-anchor pointing to the containing module family; describes WHERE the semantic lives, not HOW it's implemented.

## Notes

- All 16 checklist items pass on first authoring pass.
- Unlike milestones 160 + 161, this milestone is NOT investigation-loop-heavy — the root cause is well-understood (workspace-peer reader doesn't cross-resolve). Implementation is targeted at the specific readers named in FR-001.
- The 902 phantom edges + 159 empty-version PURLs from the milestone-158 audit are the load-bearing SC-002/SC-004 evidence.
- Building on the milestone-158/159/160/161/162 vocabulary pattern: 1 new per-component annotation (`mikebom:unresolved-declared-dep`) with bare-string OR JSON-array value (matches milestone-159 C106/C107 + milestone-162 C114 multi-value precedent).
- SC-003 dual-side byte-identity guard verified achievable pre-authoring: 10 non-`npm` milestone-090 fixtures × 3 formats = 30 goldens byte-identical; the `npm` fixture goldens MAY change if its `package.json` files reference deps that get cross-resolved.
- Ready for `/speckit-clarify` OR `/speckit-plan`. Two candidate clarifications identified but neither blocking:
  - Q1 candidate: FR-004 unresolvable-dep disposition — SUPPRESS the edge + emit source-side annotation (recommended), OR emit the phantom edge but annotate it as unresolved-declared-dep, OR RE-EMIT the phantom edge unchanged (would be a no-op fix)? Recommended default: **SUPPRESS the edge + annotate the source**. Matches Constitution Principle IX (Accuracy — no phantom edges).
  - Q2 candidate: Range-spec-mismatch disposition — when a peer declares `"^4.0.0"` but only `3.10.1` is resolved (semver mismatch), suppress the edge OR emit against the closest-matching resolved version + annotate the mismatch? Recommended default: **suppress the edge + annotate as unresolved-declared-dep** (same disposition as FR-004 primary path). Rejects wrong-version matches from polluting the graph.
- Both are LOW-impact questions — could be resolved at `/speckit-clarify` or deferred to plan-time.
- Fixes milestone-158's load-bearing blocker for the aspirational ≥99% graph-completeness target — SC-001 delivers on that promise for the npm ecosystem.
