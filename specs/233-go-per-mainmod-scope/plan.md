# Implementation Plan: Go graph resolver — per-main-module `dependsOn` scoping

**Branch**: `233-go-per-mainmod-scope` | **Date**: 2026-08-11 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/233-go-per-mainmod-scope/spec.md`

## Summary

The Go graph resolver's `gosum_fallback_paths()` accumulator (`waybill-cli/src/scan_fs/package_db/golang/graph_resolver.rs`) aggregates every `go.sum` module discovered under the scan root into ONE global set. Every main-module's `build_main_module_entry` (`legacy.rs:1893-1902`) then augments its `depends` list with the same global aggregate — which is why the reporter observed every main-module in a 4-module fixture claiming `dependsOn x/text@v0.25.0` (the deepest module's version, last-writer-wins over the walk).

The fix scopes fallback-paths per main-module: each project_root builds `depends` from its own `go.mod` `require` list + its own `go.sum` entries + its own `replace` directives. Cross-module bleed is eliminated at the source. project-discovery's algorithm needs no changes.

Additionally, FR-008 requires per-Go-version `stdlib` components — one `pkg:golang/stdlib@<version>` per distinct `go <version>` declaration across the scan.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–232; no nightly required).
**Primary Dependencies**: Existing only — `std::collections::{HashMap, HashSet}`, `tracing`, `anyhow`. The existing `parse_go_mod` + `parse_go_sum` helpers in `legacy.rs`, `graph_resolver.rs::ModuleGraphMap`, `waybill_common::resolution::{ResolvedComponent, Relationship}`. **Zero new Cargo dependencies.**
**Storage**: N/A — all state in-process per scan.
**Testing**: `cargo +stable test --workspace` — new unit tests colocated with `legacy.rs::tests` and `graph_resolver::tests` (both already exist). One integration test in a new file `waybill-cli/tests/go_per_mainmod_scope.rs` running the reporter's minimal repro shape end-to-end. Grafana verification (SC-003, SC-004) is a one-shot manual step.
**Target Platform**: All platforms waybill already builds on.
**Project Type**: Single-file bug fix inside `waybill-cli/src/scan_fs/package_db/golang/`. Touches `legacy.rs` (edge-emission loop) + `graph_resolver.rs` (per-main-module fallback-paths API). One new integration-test file. No new modules.
**Performance Goals**: Per-main-module fallback path lookup runs once per project_root (typically ≤50 per scan; unbounded but capped by tree depth). Each lookup is a HashSet-of-paths retention against the current project's parsed go.sum — negligible cost. Grafana-scale scan (47 units × ~20-50 modules each = ~1000 main-modules) should see no measurable slowdown.
**Constraints**: FR-005 (identical output across `--project-discovery` modes modulo project-discovery's own filtering). FR-006 (offline correctness). Must NOT introduce a "correct only when online" regression.
**Scale/Scope**: Same scale as any existing Go scan. Real repos range from 1 module (small services) to Grafana's ~40-module workspace to platform monorepos with 100+ modules. Fix is O(modules × mean-go.sum-size); no perf-relevant threshold.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Evaluated against `.specify/memory/constitution.md` v2.1.0. **All principles PASS.**

- **I. Pure Rust, Zero C**: PASS. Zero new C, zero new deps.
- **II. eBPF-Only Observation** + **XII. External Data Source Enrichment**: PASS. Static-scan mode; no new dependency-discovery mechanism.
- **III. Fail Closed**: PASS. Post-fix, an unresolvable transitive dep records as "unresolved" rather than substituting a wrong version — that's the FR-006 requirement.
- **IV. Type-Driven Correctness**: PASS. No new `String`-typed domain values; the fix scopes an existing `HashSet<String>` accumulator. No `.unwrap()` in production paths.
- **V. Specification Compliance**: PASS. No new `waybill:*` annotation introduced. The fix corrects values on existing annotations (`waybill:workspace-member`, `waybill:build-inclusion`, `dependencies[]` edges) and produces more emission of an existing component type (`pkg:golang/stdlib@<version>` — the fix ADDS stdlib components when Go versions differ, but the shape is unchanged).
- **VI. Three-Crate Architecture**: PASS. Contained inside `waybill-cli/`. No new crates.
- **VII. Test Isolation**: PASS. Unit tests + one subprocess-based integration test. No eBPF, no root, no CAP_BPF.
- **VIII. Completeness**: PASS. This fix directly restores completeness — pre-fix the reporter's fixture shows root's declared v0.40.0 as an orphan while a sibling's v0.25.0 is falsely attached; post-fix each main-module's declared version is correctly attached.
- **IX. Accuracy**: PASS. Fix produces the edges Go's own toolchain would produce (matches `go mod graph`). Removes phantom edges; no new heuristics.
- **X. Transparency**: PASS. When the offline resolver can't reach a transitive dep, it records the dep as `unresolved` — same as m112's existing skip-reason path. Operators see the same diagnostic surface they see today.
- **XI. Enrichment** + **XII.**: PASS. No enrichment path touched.

No violations. No Complexity Tracking entries needed.

## Project Structure

### Documentation (this feature)

```text
specs/233-go-per-mainmod-scope/
├── plan.md                        # This file
├── research.md                    # Phase 0 output
├── data-model.md                  # Phase 1 output
├── quickstart.md                  # Phase 1 output
├── contracts/
│   └── go-per-mainmod-edges.md    # Per-main-module edge contract
├── checklists/
│   └── requirements.md            # From /speckit.specify
├── spec.md                        # Feature spec (with Clarifications)
└── tasks.md                       # Phase 2 output (/speckit.tasks — NOT here)
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/golang/
├── legacy.rs                      # Existing edge-emission loop.
│                                  # This milestone modifies:
│                                  #   - The `build_main_module_entry`
│                                  #     augmentation at ~1893 so
│                                  #     `depends` gets ONLY the
│                                  #     current project_root's go.sum
│                                  #     entries + its go.mod requires
│                                  #     + its replace directives.
│                                  #   - The stdlib injection at ~2304
│                                  #     to emit per-Go-version stdlib
│                                  #     components per FR-008.
├── graph_resolver.rs              # Existing graph builder. This
│                                  # milestone modifies:
│                                  #   - `gosum_fallback_paths()` →
│                                  #     `gosum_fallback_paths_for
│                                  #     (project_root: &Path)` returning
│                                  #     only the modules attributed to
│                                  #     that specific main-module's
│                                  #     go.sum.
│                                  #   - Per-main-module `ModuleId`
│                                  #     provenance tracking so we can
│                                  #     answer "did this module come
│                                  #     from THIS project_root's
│                                  #     go.sum?"
├── mod.rs                         # UNCHANGED (dispatch)
└── ...                            # Other files UNCHANGED

waybill-cli/tests/                 # New integration test file:
└── go_per_mainmod_scope.rs        # 3+ subprocess-based tests scanning
                                   # the reporter's minimal repro
                                   # fixture across all three
                                   # --project-discovery modes.

waybill-cli/tests/fixtures/golden_inputs/golang/ (NEW fixtures):
├── per_mainmod_scope_4modules/    # Reporter's minimal repro (4
│                                  # modules, 4 distinct x/text
│                                  # versions).
├── per_mainmod_scope_shared_ver/  # US2: 2 modules requiring the
│                                  # same x/text version → union
│                                  # workspace-member.
└── per_mainmod_scope_mixed_go/    # FR-008: 2 modules with distinct
                                   # `go <version>` directives.
```

**Structure Decision**: In-place extension of `legacy.rs` + `graph_resolver.rs`. The fix is a targeted change to per-project scoping in the augmentation loop; no new modules, no restructure. Fixture placement mirrors the existing m231 `golang/workspace_mode/` layout.

## Complexity Tracking

> No constitution violations to justify. Section intentionally empty.
