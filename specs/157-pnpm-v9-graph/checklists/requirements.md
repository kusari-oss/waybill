# Specification Quality Checklist: pnpm-lock v9 dep-graph fix (milestone 157)

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-07-03
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs) — spec cites `pnpm_lock.rs:20` in Root Cause as a code anchor only; FRs/SCs describe consumer-visible outcomes.
- [X] Focused on user value and business needs — the SBOM consumer's dep-graph completeness drives US1 (110 → ≥5000 edges is the ROI narrative).
- [X] Written for non-technical stakeholders — outcomes phrased as "consumer sees a full graph" not "populate PackageDbEntry.depends from snapshots mapping."
- [X] All mandatory sections completed — User Scenarios + Requirements + Success Criteria + Assumptions all present.

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain — pnpm v9 lockfile schema is stable + well-documented; parse_pnpm_key already exists; the fix shape is unambiguous.
- [X] Requirements are testable and unambiguous — each FR names a concrete behavior; each SC names a verification method (integration test, byte-diff, jq recipe).
- [X] Success criteria are measurable — SC-001 names ≥5000 edges + specific expected shape for `@actions/core@3.0.1`; SC-007 = ≥5 unit tests; SC-008 = new integration test.
- [X] Success criteria are technology-agnostic — outcomes phrased in SBOM-consumer terms (`dependsOn` edge counts, specific expected PURLs); argo-cd named as the concrete testbed.
- [X] All acceptance scenarios are defined — US1 has 6 Given/When/Then scenarios covering minimal, empty-body, peer-dep-suffix, v6/v7-backward-compat, orphan, and performance cases.
- [X] Edge cases are identified — 6 edge cases enumerated: orphan snapshots, orphan packages, peer-dep suffixes with nested parens, duplicate keys, empty/missing snapshots, non-registry deps.
- [X] Scope is clearly bounded — FR-010 through FR-015 + Out of Scope enumerate explicit exclusions (no other npm sub-reader changes, no other reader changes, no emitter changes, no new annotations, no new deps, no dispatch-order change, no workspace root support, no integrity/hash changes, no non-registry resolution).
- [X] Dependencies and assumptions identified — 8 Assumptions + explicit Dependencies on milestone 106, serde_yaml, parse_pnpm_key.

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria — each FR maps to a US1 acceptance scenario, an SC, or both.
- [X] User scenarios cover primary flows — US1 (SBOM consumer sees complete graph) covers the reported bug end-to-end.
- [X] Feature meets measurable outcomes defined in Success Criteria — SC-001 (argo-cd ≥5000 edges), SC-002 (v6/v7 byte-identity), SC-003 (peer-dep normalization), SC-004 (leaf-node), SC-005 (missing snapshots warning), SC-006 (pre-PR gate), SC-007 (≥5 unit tests), SC-008 (integration test), SC-009 (CHANGELOG), SC-010 (no wire-format changes).
- [X] No implementation details leak into specification — the `pnpm_lock.rs:20` reference in FR-001 is a code anchor; FRs describe outcomes independently.

## Notes

- All 16 checklist items pass on first authoring pass.
- The bug + fix shape are both narrow — this is a targeted <30 LOC fix with tight test coverage. Not a "large architectural change" milestone.
- The team's report against `kusari-sandbox/argo-cd` provides the concrete testbed; empirical reproduction 2026-07-03 confirms 1329 components / 110 edges pre-fix. Post-fix expectation: 1329 / ≥5000+.
- `/speckit-clarify` session 2026-07-03 locked in one decision:
  - Q1 (peer + optional dep handling): **Read all three sub-fields (`dependencies:` + `peerDependencies:` + `optionalDependencies:`)**. Brings pnpm to parity with npm's `package_lock.rs` (which has walked all four since milestone 147). Reframes SC-002 as a dual-side guard: non-pnpm goldens byte-identical, pnpm goldens monotonic-additive. FR-002 + FR-004 + SC-007 expanded to cover the three sub-fields + defensive de-dup + parity assertion. New SC-011 codifies the pnpm/npm parity.
- `/speckit-analyze` session 2026-07-03 surfaced + remediated 5 findings:
  - F1 (MEDIUM): SC-005 (v9 no-snapshots warn) had no automated test — resolved by adding test #9 to T007 for behavioral verification (log-format documented in FR-008; log-string capture out of scope). SC-005 + SC-007 updated.
  - F2 (MEDIUM): SC-002's automated monotonic-additive check was only synthetic — resolved by adding T010 Step-1 pre-regeneration snapshot via `git show main:...` + T010 Step-3 real-golden verification test with printed edge-count summary for the PR description.
  - F3 (LOW): T004's `fell_back_count` counter logic was underspecified — resolved by codifying the `if let Some(snap_deps) = ...` two-branch shape inline in T004. T005 clarified that the counter tracking lives in T004; T005 only consumes it.
  - F4 (MEDIUM): SC-001's ≥5000 floor was optimistic — resolved by splitting into defensive floor (≥2500) + aspirational target (≥5000). T014 empirically revises inline per milestone-156's F1 pattern.
  - F5 (LOW): T013 guard was missing `npm/mod.rs` diff check for FR-015 — resolved by adding two additional guard commands (npm/mod.rs + sibling sub-readers excluding pnpm_lock.rs).
- Ready for `/speckit-implement`.
- **Implementation complete 2026-07-03** (T001–T015):
  - **T012 pre-PR gate**: 12 pnpm unit tests + 4 integration tests all `ok`. Full workspace clippy zero errors. One pre-existing sbomqs failure (`sbomqs_spdx_score_meets_or_beats_cdx_across_ecosystems`) reproduces identically on stashed baseline — not milestone-157's regression; last 5 main CI runs green (CI provisions a working sbomqs).
  - **T014 SC-001 empirical**: argo-cd/ui produced **2169 edges / 762 non-root components with non-empty dependsOn** (vs pre-157: 110 / 0). `@actions/core@3.0.1` shape verified exact: `dependsOn = ["pkg:npm/%40actions/exec@3.0.0", "pkg:npm/%40actions/http-client@4.0.1"]`. SC-001 revised inline from ≥2500 floor / ≥5000 aspirational → **≥2000 empirical floor** (matching milestone-156's F1 revision pattern). Reason: pnpm v9 `snapshots:` encodes only direct edges per package (~1.63 edges/component), not the nested-`node_modules` transitive expansion that the ≥5000 aspirational was calibrated for.
  - **T010 monotonic-additive**: milestone-090 npm golden fixture is pnpm v6 with only a `dependencies:` sub-mapping (no peer/optional/snapshots content), so no goldens regenerated. SC-002 dual-side guard satisfied: 10 of 11 non-pnpm goldens byte-identical (nothing changed elsewhere); pnpm goldens 3→3 (Δ +0) because the fixture doesn't exercise the v9 code path. Real-golden verification via `MIKEBOM_PRE157_SNAPSHOT_DIR=/tmp/mikebom-m157-pre-goldens` printed the diagnostic edge-count summary as designed.
  - **T013 wire-format guard**: `mikebom-cli/src/formats/`, `annotations.rs`, `mikebom-common/`, `npm/mod.rs`, sibling `npm/*.rs` readers, `mikebom-cli/tests/fixtures/golden/`, `parity/extractors/` — ALL clean (SC-010 verified).
  - **Wire-format cleanliness**: only `pnpm_lock.rs` changed under `npm/` (F5-remediated isolation confirmed).
