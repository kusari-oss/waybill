# Implementation Plan: Close Remaining Graph-Completeness Orphan Gaps

**Branch**: `194-close-orphan-gaps` | **Date**: 2026-07-14 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/194-close-orphan-gaps/spec.md`

## Summary

Two-part fix bundled into one milestone at user direction:

- **US1 (#571)** — At Go stdlib emission time in `mikebom-cli/src/scan_fs/package_db/golang/legacy.rs::build_stdlib_entry` (or its caller at line 2256), also emit a synthetic `Relationship { from: <Go primary root PURL>, to: pkg:golang/stdlib@v<version>, DependsOn }` for every emitted stdlib. Handled at read-time so the pre-emission Relationship set contains the edge before `compute_graph_completeness` runs, and the m192/m193 pre-rewrite naturally re-anchors it onto `target_ref` when `--root-name` is active.

- **US2 (#572)** — In `mikebom-cli/src/scan_fs/package_db/npm/mod.rs`, extend the mainmod-emission pass (m066, line 194+) so nameless nested `package.json` files that ARE npm project roots (discovered via `candidate_project_roots`) get a synthesized mainmod component with `mikebom:component-role: main-module` + a versionless PURL derived from the directory basename (`pkg:npm/<dir-basename>` per m191's spec-clean convention). Existing lockfile-tier walker already emits transitive components + Relationships from named packages; the new mainmod provides the missing DependsOn anchor. Compatible with `--root-name` per Q2 answer B (dropped alongside top-level; m192/m193 pre-rewrite re-anchors edges).

Technical approach: extend two readers with small localized additions. Zero new modules. Zero new Cargo dependencies. All infrastructure (Relationships, mainmod annotations, m158 workspace-peer edges, m192/m193 pre-rewrite) is already in place — this milestone just plugs the two remaining emission gaps.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–193; no nightly required).

**Primary Dependencies**: Existing only — `mikebom_common::resolution::{Relationship, RelationshipType, EnrichmentProvenance, ResolvedComponent}` (existing types), `mikebom_common::types::purl::Purl` (existing type; `.ecosystem()` accessor for US1 root matching + versionless PURL construction for US2), `serde_json` (annotation values), `tracing` (INFO summary logs per FR-015), `anyhow`. **Zero new Cargo dependencies.** No subprocess calls. No network access.

**Storage**: N/A — pure in-memory extensions to existing read-time emission paths. No caches, no persistence. Matches every milestone since 002.

**Testing**: `cargo +stable test --workspace`. New assertions:
- Unit tests in `golang/legacy.rs::tests` for the stdlib edge emission (2-3 tests: single-version fixture, multi-version fixture, no-mainmod fallback).
- Unit tests in `npm/mod.rs::tests` for the nameless-nested-mainmod synthesis (3-4 tests: nested nameless workspace → mainmod emitted, nested named workspace unchanged, top-level-only unchanged, `--root-name` interaction).
- Integration tests: extend `mikebom-cli/tests/graph_completeness_operator_root.rs` (from m192) with 2 new tests — pico corpus regression fixture (Go source + nested npm workspace) reports `complete` post-fix.
- Regression: every existing golden byte-identical unless it exercises stdlib-emission or nested-nameless-npm — flip those specific goldens per Phase 2 audit.

**Target Platform**: Linux + macOS host builds; behavior host-agnostic.

**Project Type**: CLI (existing `mikebom sbom scan` subcommand).

**Performance Goals**: US1 adds one Relationship per emitted stdlib (typically 1 per scan). US2 adds one Relationship per nameless nested workspace's declared deps (typically 1-10 per scan). Both O(N) over existing collections; no perf regression concern.

**Constraints**: Byte-identity for goldens outside the drift set (FR-012). No new `mikebom:*` annotations (FR-014 / Principle V). No new Cargo deps.

**Scale/Scope**: ~30-50 LOC in `golang/legacy.rs` (US1) + ~50-80 LOC in `npm/mod.rs` (US2). Estimated 15-20 tasks including tests.

## Constitution Check

Post-Phase-0 recheck below. Initial pass:

| Principle | Status | Notes |
|---|---|---|
| I. Pure Rust, Zero C | PASS | No new deps; no C. |
| II. eBPF-Only Observation | N/A | User-space reader extensions. |
| III. Fail Closed | PASS | Fixes are additive — synthetic edges + mainmods. Failures (no Go mainmod detected, no nameless nested workspace) fall through to pre-m194 behavior (still emits components, still fires OrphanedComponentsDetected on real gaps). |
| IV. Type-Driven Correctness | PASS | Reuses existing `Relationship` + `Purl` + `ResolvedComponent` newtypes; no `.unwrap()` in production code. |
| V. Specification Compliance | PASS + explicit per FR-014. No new `mikebom:*` annotations. Reuses existing `mikebom:component-role: main-module` + `Relationship` shapes. Audit result recorded. |
| VI. Three-Crate Architecture | PASS | Changes limited to `mikebom-cli` (2 files). No new crate. |
| VII. Test Isolation | PASS | Pure-Rust unit + integration tests; no eBPF privilege. |
| VIII. Completeness | PASS | Closes 2 specific orphan classes; real orphans (design-tier, file-tier, transitive resolver gaps) STILL surface per FR-016. Improves the Completeness signal's accuracy. |
| IX. Accuracy | PASS | Fixes REMOVE false negatives from the completeness signal; classifier fires `partial` ONLY for real gaps post-m194. |
| X. Transparency | PASS | INFO-level logs per FR-015 (matching m192/m193 convention). Standards-native edge + annotation channels carry all signal. |
| XI. Enrichment | N/A | No external data source touched. |
| XII. External Data Source Enrichment | N/A | Not a discovery change. |
| Strict Boundary 1 (No lockfile discovery) | N/A | Uses existing lockfile-tier data; no new lockfile-as-discovery. |
| Strict Boundary 2-4 | PASS | No MITM, no C, no `.unwrap()` in production. |
| Strict Boundary 5 (No file-tier duplicates in default mode) | N/A | Not a file-tier change. |

**No violations. Proceed to Phase 0.**

## Project Structure

### Documentation (this feature)

```text
specs/194-close-orphan-gaps/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/       # emission-shape contracts for stdlib edge + nested mainmod
├── checklists/requirements.md
└── tasks.md         # generated by /speckit-tasks
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   └── scan_fs/package_db/
│       ├── golang/legacy.rs         # PRIMARY (US1): extend build_stdlib_entry
│       │                             # call site at line ~2256 to also emit the
│       │                             # synthetic mainmod→stdlib Relationship.
│       │                             # Requires access to the Relationships Vec
│       │                             # from the outer scan_fs/mod.rs — either
│       │                             # return the edges from the reader and let
│       │                             # the caller merge, or accept a &mut Vec.
│       └── npm/mod.rs                # PRIMARY (US2): extend
│                                     # apply_nameless_secondary_umbrella (line
│                                     # 361+) to ALSO synthesize a mainmod
│                                     # component per nested nameless workspace,
│                                     # not just merge deps upward. Preserves
│                                     # existing umbrella behavior as fallback
│                                     # (backward compat).
└── tests/
    ├── graph_completeness_operator_root.rs   # EXTEND: 2 new tests covering
                                              # US1 stdlib reachability + US2
                                              # nested-nameless-npm reachability
                                              # (via a synthetic fixture that
                                              # reproduces the pico shape).
    └── existing goldens/                     # UNCHANGED except: any golden
                                              # currently exhibiting a stdlib
                                              # orphan or nested-nameless-npm
                                              # orphan will drift.
```

**Structure Decision**: Purely emission-side extensions in two existing readers. No new modules, no new emitters, no new plumbing. The completeness classifier itself (mod.rs / bfs.rs) is untouched — this milestone gives it more edges + roots to work with rather than changing its behavior.

## Complexity Tracking

*No constitution violations — table intentionally empty.*
