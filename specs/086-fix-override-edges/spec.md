# Feature Specification: Fix override-path edge loss (milestone-084 follow-up)

**Feature Branch**: `086-fix-override-edges`
**Created**: 2026-05-08
**Status**: Draft
**Input**: User report on alpha.24 — "SBOMs look totally different and seem much worse." Investigation against a real Go project (`hosted-guac-mgmt`) with `--root-name slack-notifier --root-version narsa` revealed milestone 084 introduced a regression in the operator-override path: 3 dep edges lost vs alpha.22 baseline.

## Overview

Milestone 084 fixed the CDX 1.6 orphan-ref bug for the main-module path AND attempted to fix the analogous closure-violation in the `--root-name` / `--root-version` override path (milestone 077). For the override path, the fix used **Option A** (filter relationships whose `from` matches the dropped main-module PURL) per `specs/084-cdx-mainmod-collapse/research.md §2`. Real-world testing showed Option A drops legitimate dep edges:

**Reproduction** (Go project at `hosted-guac-mgmt`):

| | alpha.22 (override) | alpha.24 (override) | Δ |
|---|---|---|---|
| Edges from project root | 15 | 12 | **−3 lost** |
| Closure invariant satisfied | NO (orphan ref) | YES | ✅ |
| `metadata.component.bom-ref` | `slack-notifier@narsa` | `slack-notifier@narsa` | unchanged |
| Lost edges | (none) | aws-sdk-go-v2 + aws-sdk-go-v2/service/dynamodb + stretchr/testify | regression |

The 3 missing edges are real direct deps that *also* happen to be transitively depended on by other deps. After Option A filters out the main-module-PURL-keyed relationships, `target_ref = slack-notifier@narsa` has no outgoing edges, so `dependencies.rs`'s primary-dep fallback synthesizes edges from target_ref → roots-of-component-graph. The "roots" heuristic excludes any component that something else depends on — losing the 3 deps that have transitive dependents.

The right fix is **Option B** (rewrite, not filter): when a relationship's `from` equals a dropped main-module PURL, rewrite `from` to `target_ref` instead of dropping the relationship. All 15 edges are preserved with the override identity; closure invariant still satisfied.

This was correctly anticipated in research §2 ("rewrite preserves dep-tree shape under override") but Option A was chosen as simpler. Option A's hidden cost — silent edge loss when project deps form a non-tree DAG — wasn't caught because the milestone-077 override-path golden has only 2 deps, no transitive overlap.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Override path preserves all direct dep edges (Priority: P1)

An operator scanning a Go project with `--root-name <override> --root-version <override>` post-086 sees exactly the same number of direct dep edges from the project root as they would WITHOUT override (just under a different identity). No edges silently disappear because of the override flag.

**Why this priority**: Headline regression of alpha.24. Real users reported it. Hot-fix material.

**Independent Test**: Scan a fixture with ≥3 direct deps where some deps are transitively depended on by other deps — once without override, once with `--root-name x --root-version y`. Assert the project-root entry's `dependsOn[]` count is identical, just keyed by the override identity in the second case.

**Acceptance Scenarios**:

1. **Given** a Go project with N direct deps where some are transitively depended on by other deps, **When** mikebom emits CDX 1.6 with `--root-name <X> --root-version <Y>`, **Then** the project-root entry's `dependsOn[]` has exactly N edges, all keyed by `<X>@<Y>`.
2. **Given** the same project, **When** mikebom emits CDX 1.6 without override, **Then** the project-root entry's `dependsOn[]` has exactly N edges, all keyed by the milestone-053 main-module PURL.
3. **Given** the closure invariant from milestone-084, **When** the override scenario runs, **Then** the invariant holds (no orphan refs, milestone-084's primary win preserved).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: When `--root-name` / `--root-version` override is active, relationships whose `from` matches a dropped main-module PURL MUST be rewritten so that `from = target_ref` (the override identity), preserving the `to` and `relationship_type` fields. Replaces the milestone-084 filter behavior.
- **FR-002**: Edge count from `metadata.component.bom-ref` MUST be invariant under override application: scanning the same project with-and-without override produces the same number of direct-dep edges from the project root, just keyed by different identifiers.
- **FR-003**: The closure invariant from milestone 084 MUST continue to hold under override (no orphan refs).
- **FR-004**: Existing milestone-077 override tests MUST pass unmodified.
- **FR-005**: Existing milestone-084 closure-invariant test (`cdx_ref_closure_invariant`) MUST be extended with an override-mode assertion: scan one of the post-053 ecosystem fixtures with override flags, verify direct-edge count + closure invariant.
- **FR-006**: SPDX 2.3 + SPDX 3 emission MUST be unaffected (this is a CDX-only relationship-rewrite fix happening at `cyclonedx/builder.rs` time; SPDX uses unmodified `relationships`).
- **FR-007**: Pre-PR gate stays clean: clippy zero warnings; `cargo test --workspace` `0 failed`.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Reproduces zero edge loss on `hosted-guac-mgmt` with override (15 edges in both alpha.22 and post-086).
- **SC-002**: Closure invariant holds under override for all post-053 ecosystem fixtures.
- **SC-003**: New regression test in `cdx_ref_closure_invariant.rs` asserts edge-count invariance with-and-without override across at least one fixture.
- **SC-004**: Pre-PR gate clean.

## Assumptions

- The fix is a single-file change in `mikebom-cli/src/generate/cyclonedx/builder.rs` (replace the filter with a map). ~10 LOC.
- No new Cargo dependencies.
- No SPDX golden regen needed (the rewrite happens at `cyclonedx/builder.rs` time, not in the upstream `relationships` pipeline; SPDX still receives the unmodified pipeline output).

## Out of scope

- Polyglot scans with multiple main-module PURLs (research §2 noted this; the rewrite handles them via `dep_map`'s `BTreeSet` dedup, but the operator-facing semantic of "what does the project root mean in a polyglot override?" is a separate question for a follow-up).
- The `dependencies.rs` primary-dep fallback (line 78-91) — kept as-is; rewrite means the fallback rarely fires under override now.
