# Implementation Plan: Workspace-root peer linkage + graph-completeness annotations

**Branch**: `158-graph-completeness` | **Date**: 2026-07-03 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/158-graph-completeness/spec.md`

## Summary

Fix issue #492: workspace-monorepo SBOMs currently have only ~20% BFS-reachability from the declared root because mikebom's root-selection identifies workspace peers as "losers" but never links them to the chosen root. Milestone 158 wires those losers into the root's `dependsOn` AND emits two new document-scope annotations (`mikebom:graph-completeness` + `mikebom:graph-completeness-reason`) after a multi-root BFS pass at emit-time. Q1 caution-first + Q2 orphan-classification + Q3 multi-ecosystem semantics from the 2026-07-03 clarify session are all in-scope. Constitution Principle VIII (Completeness) + Principle X (Transparency) are the north stars.

Empirical target: 19.5% → ≥99% BFS reachability on `test-podman-desktop`; universal annotation emission across all 11 milestone-090 goldens plus the 5 `kusari-sandbox/test-*` repos.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–157; no nightly required for this user-space-only work).

**Primary Dependencies**: Existing only — `serde` / `serde_json` (annotation envelope + JSON emission), `tracing` (info-level log line per FR-013 below), `anyhow` / `thiserror` (error propagation), `clap` (no new flags — the annotation is unconditional). The multi-root BFS + reason-code classifier uses `std::collections::{HashMap, HashSet, VecDeque}` from stdlib. **Zero new Cargo dependencies.**

**Storage**: N/A — all state in-process per scan; the graph-completeness pass runs once at emit-time, its result is a stack-allocated `GraphCompletenessResult` struct that flows into the three format emitters and is dropped when serialization finishes. No caches, no persistence (matches every milestone since 002).

**Testing**: `cargo +stable test --workspace` — new unit tests under `mikebom-cli/src/generate/graph_completeness/mod.rs` (SC-007 floor ≥10) and a new integration test `mikebom-cli/tests/graph_completeness_workspace_bfs.rs` synthesizing a `test-podman-desktop`-shaped monorepo (SC-008).

**Target Platform**: Same as milestone 157 — Linux + macOS + Windows via existing CI matrix. No platform-specific behavior.

**Project Type**: Rust workspace with three crates (mikebom-cli, mikebom-common, mikebom-ebpf); milestone 158 touches ONLY `mikebom-cli` (user-space).

**Performance Goals**: The BFS pass MUST be O(V+E) and MUST NOT add >100ms to scan time for repos with ≤10,000 components (per FR-008). Empirical target: <20ms on the 2835-component `test-podman-desktop` testbed.

**Constraints**:

- Standards-native precedence (Constitution Principle V): the two annotations use the `mikebom:*` prefix because no CDX/SPDX-native "graph completeness" property exists at emission time; per FR-010, a future CDX 1.7 / SPDX 3.1 native enum would supersede.
- Byte-identity guard on 11 milestone-090 non-workspace goldens (SC-002 dual-side mirror of milestone 157): the ONLY diff bytes should be the added `mikebom:graph-completeness = complete` annotation.
- No `.unwrap()` in production paths (Constitution Principle IV) — test code uses the standard `#[cfg_attr(test, allow(clippy::unwrap_used))]` guard.

**Scale/Scope**: Milestone-158 touches ~200–300 LOC in `mikebom-cli` (new `graph_completeness/` submodule + wire-up in `metadata.rs`, `annotations.rs`, `v3_annotations.rs`, and the per-format dependency emitters where losers get linked to root). Plus ~150 LOC of unit tests, ~100 LOC of integration test, ~3 parity-catalog rows.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Milestone 158 impacts these principles:

- **Principle I (Pure Rust, Zero C)**: PASS. Zero new Cargo deps. No C source, no FFI. The BFS pass uses stdlib collections only.
- **Principle IV (No `.unwrap()` in production)**: PASS. All production paths use `?` propagation. Test-only `.unwrap()` is `#[cfg(test)]`-guarded per the convention established in milestone 001.
- **Principle V (Specification Compliance — standards-native precedence)**: PASS with acknowledged deviation. The two annotations use `mikebom:*` because no CDX 1.6 / SPDX 2.3 / SPDX 3.0.1 native "SBOM graph completeness" property exists. FR-010 codifies the migration path if either standard adds one. The precedent (milestone 127's `mikebom:root-selection-heuristic`, milestone 134's `mikebom:divergent-purl-*`) is directly analogous.
- **Principle VIII (Completeness)**: **This is the milestone's whole point**. Milestone 158 makes graph-completeness a first-class emitted signal AND fixes the workspace-root undercount that was masking the completeness gap. The Q2 orphaned-components rule ("emit orphans faithfully + flag `partial`, no filtering, no auto-linking") is the constitutionally-correct stance — matches the milestone-133 orphan-fallback contract's spirit.
- **Principle X (Transparency)**: **The other whole-point principle**. The two annotations ARE transparency metadata in the constitution's exact sense: "When mikebom cannot guarantee completeness, it MUST include structured metadata in the SBOM output that informs the consumer of the limitation." Milestone 158 operationalizes this for graph-shape completeness specifically.
- **Strict Boundary #4 (No `.unwrap()`)**: PASS — reiterated above.
- **Strict Boundary #5 (No file-tier duplicates in default mode)**: N/A — milestone 158 does NOT emit file-tier components. It emits document-scope annotations only.

**Result**: All gates PASS. No violations to track in Complexity Tracking.

## Project Structure

### Documentation (this feature)

```text
specs/158-graph-completeness/
├── plan.md                                 # This file
├── research.md                             # Phase 0 output
├── data-model.md                           # Phase 1 output
├── quickstart.md                           # Phase 1 output
├── contracts/
│   ├── annotation-schema.md                # Per-format annotation shape
│   ├── graph-completeness-vocabulary.md    # 8 reason codes + grammar
│   └── reachability-algorithm.md           # Multi-root BFS pseudocode
├── checklists/
│   └── requirements.md                     # (from /speckit-specify)
├── spec.md                                 # (from /speckit-specify + /speckit-clarify)
└── tasks.md                                # Phase 2 output (from /speckit-tasks)
```

### Source Code (repository root)

New submodule + emission wire-up:

```text
mikebom-cli/
├── src/
│   ├── generate/
│   │   ├── graph_completeness/             # NEW submodule (milestone 158)
│   │   │   ├── mod.rs                      # `compute_graph_completeness` public API
│   │   │   ├── bfs.rs                      # Multi-root BFS pass (FR-008 + FR-012)
│   │   │   ├── reason_codes.rs             # 8-code vocabulary enum + join-with-;
│   │   │   └── tests.rs                    # ≥10 unit tests (SC-007)
│   │   ├── cyclonedx/
│   │   │   ├── metadata.rs                 # +annotation emission (like milestone-127 C69 pattern at :438)
│   │   │   └── dependencies.rs             # +losers-→-root linkage (FR-002)
│   │   ├── spdx/
│   │   │   ├── annotations.rs              # +document-scope Annotation (like milestone-127 :492)
│   │   │   └── relationships.rs            # +DEPENDS_ON edges for losers (FR-002)
│   │   ├── spdx3/
│   │   │   ├── v3_annotations.rs           # +document-scope Annotation (like milestone-127 :454)
│   │   │   └── v3_relationships.rs         # +dependsOn Relationships for losers
│   │   └── root_selector.rs                # unchanged — losers already exposed
│   ├── parity/extractors/
│   │   ├── mod.rs                          # +2 new catalog rows: C70/C71 (SC-010)
│   │   ├── cdx.rs                          # +2 extractors
│   │   ├── spdx2.rs                        # +2 extractors
│   │   └── spdx3.rs                        # +2 extractors
│   └── cli/
│       └── scan_cmd.rs                     # +FR-013 tracing::info!("graph completeness computed")
└── tests/
    └── graph_completeness_workspace_bfs.rs # NEW SC-008 integration test
```

**Structure Decision**: Introduce a new `mikebom-cli/src/generate/graph_completeness/` submodule as the source-of-truth for the BFS pass + reason-code classification. All three format emitters (CDX / SPDX 2.3 / SPDX 3) consume its `GraphCompletenessResult` output through the same API — matching the milestone-127 `RootSelectionResult`-driven emission pattern that already ships. The workspace-peer linkage step (adding losers to root's `dependsOn`) happens in each format's dependency emitter using the SAME `RootSelectionResult.losers` field that milestone 127 already exposes. This keeps the fix contained and reuses existing infrastructure rather than inventing new plumbing.

## Complexity Tracking

*Empty — no Constitution violations to justify.*

The design deliberately reuses:

- Milestone 127's `RootSelectionResult` (already exposes `losers: Vec<Purl>`)
- Milestone 127's per-format annotation-emission pattern (metadata.rs:426, annotations.rs:492, v3_annotations.rs:454)
- The milestone-071 parity catalog (2 new rows, symmetric across 3 formats — same as milestone 134's collision-summary rows)
- Stdlib `HashMap`/`HashSet`/`VecDeque` for BFS (no `petgraph` or similar)

The Q3 multi-root BFS is genuinely new but is a 30-line function once the seed set is identified from the components' `mikebom:component-role = "main-module"` annotation (grouped by ecosystem). No unusual data structures.
