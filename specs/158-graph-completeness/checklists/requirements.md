# Specification Quality Checklist: Workspace-root peer linkage + graph-completeness annotations (milestone 158)

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-07-03
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs) — spec cites CDX 1.6 / SPDX 2.3 / SPDX 3.0.1 as wire-format targets (unavoidable — the annotation is emitted TO these formats), and cites `mikebom-cli/src/parity/extractors/` in SC-010 as a code anchor for parity-catalog registration. All FRs describe consumer-visible outcomes (annotation presence, values, native-mechanism placement), not implementation shape.
- [X] Focused on user value and business needs — the SBOM consumer (Kusari Inspector, vulnerability scanners, supply-chain visualizers) gets a fully-connected graph. The empirical 19.5% → ≥99% BFS reachability jump is the ROI narrative.
- [X] Written for non-technical stakeholders — outcomes phrased as "consumer sees ≥99% of components reachable from root" not "populate `root_component.depends` from `losers`". The Motivation section names the concrete symptom.
- [X] All mandatory sections completed — User Scenarios + Requirements + Success Criteria + Assumptions + Out of Scope all present.

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain — the fix shape is unambiguous per issue #492's proposed fix section. The `complete | partial | unknown` domain is closed. FR-007's per-format native-mechanism placement is determined by each format's spec (metadata.properties, annotations, CreationInfo).
- [X] Requirements are testable and unambiguous — each FR names a concrete emitted-artifact behavior; each SC names a verification method (integration test, byte-diff, jq recipe, empirical measurement).
- [X] Success criteria are measurable — SC-001 names ≥99% BFS reachability with the empirically-locked 19.5% baseline; SC-002 = byte-identity + one property addition; SC-003 = 100% presence; SC-004 = per-repo expected values across all 5 kusari-sandbox testbed repos; SC-007 = ≥6 unit tests; SC-008 = new integration test; SC-011 = closes issue #492.
- [X] Success criteria are technology-agnostic — outcomes phrased in SBOM-consumer terms (reachability %, annotation presence, byte-identical outputs). CDX/SPDX wire formats are named as inputs to the format-parity check, not as implementation choice.
- [X] All acceptance scenarios are defined — US1 has 4 Given/When/Then scenarios; US2 has 5; US3 has 3. Total: 12 acceptance scenarios covering monorepo/single-package/leaf-peer/ambiguous-root paths.
- [X] Edge cases are identified — 7 edge cases enumerated: empty workspace, leaf peer, URL-shaped-version peer, root-selection failure, multi-ecosystem repo, already-linked peer (dedup), non-npm workspace ecosystems.
- [X] Scope is clearly bounded — Out of Scope enumerates 7 explicit exclusions (issues #493/#494/#495/#496 all named, root-selection heuristic changes, cross-ecosystem linking, standards-vocabulary migration).
- [X] Dependencies and assumptions identified — 7 Assumptions covering peer definition, heuristic invariance, cross-format shape, canonicalization, BFS-as-correctness-definition, closed domain, no new deps.

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria — FR-001 through FR-010 each map to a US1/US2/US3 acceptance scenario, an SC, or both.
- [X] User scenarios cover primary flows — US1 (fully-connected graph) is the P1 fix; US2 (programmatic detection) is the P2 transparency mechanism; US3 (non-regression on single-package) is the P3 safety guarantee.
- [X] Feature meets measurable outcomes defined in Success Criteria — SC-001 (test-podman-desktop reachability), SC-002 (dual-side byte-guard), SC-003 (100% presence), SC-004 (per-repo value distribution), SC-005 (vocabulary stability), SC-006 (pre-PR gate), SC-007 (unit test floor), SC-008 (integration test), SC-009 (CHANGELOG), SC-010 (parity catalog), SC-011 (issue closure).
- [X] No implementation details leak into specification — the `mikebom-cli/src/parity/extractors/` code anchor in SC-010 is the only pointer; it names WHERE the parity check lives (a milestone-071 invariant), not HOW to register the new entry.

## Notes

- All 16 checklist items pass on first authoring pass.
- The fix shape is well-scoped: the "losers" list already exists in memory during root-selection (evidenced by the current scan log line); milestone 158 only needs to (a) wire it into the root's `dependsOn` and (b) emit the two document-scope annotations after a BFS reachability pass. Estimated 150–250 LOC of source-tree code (upped from 100 after Q3 added multi-root BFS + per-ecosystem root identification) plus tests and parity catalog rows.
- The empirical baseline (19.5% BFS reachability on test-podman-desktop, measured 2026-07-03) is the ground-truth SC-001 target. Any implementation that doesn't push this to ≥99% doesn't meet SC-001.
- The three-valued `complete | partial | unknown` domain and the structured reason `<code>: <detail>` format follow the milestone-127 root-selection annotation pattern (existing precedent for structured mikebom annotations).
- The SC-002 dual-side byte-identity guard follows milestone 157's precedent exactly: golden regression should show ONE property addition and zero other bytes changed.
- `/speckit-clarify` session 2026-07-03 locked in three decisions:
  - Q1 (complete/partial/unknown determination): **Full BFS at emit-time + caution-first fallback** — prefer `unknown` over guessing when uncertain. FR-006 + FR-008 updated with the semantics.
  - Q2 (orphaned components not from workspace peers): **`partial` with reason `orphaned-components-detected: <N>`** — faithful reporting, no filtering, no auto-linking. FR-011 added. SC-004 test-rails outcome updated to reflect the nested test-tree package.json warning virtually guaranteeing `partial` for test-rails.
  - Q3 (multi-ecosystem repos with multiple conceptual roots): **Multi-root BFS + new reason-code `multi-ecosystem-partial-root`**. Reachability = union of per-ecosystem-root reachable sets. FR-012 added. When multiple reason codes apply, joined by `; ` in a single reason string.
- SC-005 vocabulary expanded from 3 codes to 8 (with orphaned-components-detected + multi-ecosystem-partial-root + edge-resolution-degraded + go-transitive-coverage-degraded + go-workspace-mode-anomaly added by clarifications).
- SC-007 unit test floor raised from 6 → 10 to cover the Q2/Q3 additions.
- Ready for `/speckit-plan`. Coverage taxonomy: all High/Medium-impact categories Clear. One Deferred: log-line/observability (milestone-157-style FR-007 pattern) — better resolved at planning time.
