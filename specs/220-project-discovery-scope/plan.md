# Implementation Plan: `--project-discovery=<mode>` — cap main-module discovery scope

**Branch**: `220-project-discovery-scope` | **Date**: 2026-07-24 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/220-project-discovery-scope/spec.md`

## Summary

Add a `--project-discovery=<mode>` CLI flag accepting `all` (default; current behavior), `root-only` (new; discover only root-level main-modules + their ecosystem-native workspace-declared members), and `strict` (new; discover only the root-level manifest itself, no workspace members). The mode gates POST-discovery via a filter/BFS-projection pass over the resolved-component set — reusing the m215 `SubprojectRoot` + `project_for_root` BFS infrastructure. No per-reader touch: readers keep walking + discovering as they do today; the m220 filter runs after `enumerate_workspace_roots` populates the main-module set, drops out-of-scope main-modules + their unreachable transitive components, and threads the retained set into every downstream emitter.

Extensibility: mirror the m219 `SplitMode` enum-with-method pattern. Future variants (`explicit=<paths>`, `depth=<N>`) plug in via a `is_in_scope(&scan_root, &component) -> bool` method on the enum. FR-011 doc-scope annotation `waybill:project-discovery-mode` follows the m217 C136 silence-on-default precedent.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–219; no nightly required for this user-space-only work).
**Primary Dependencies**: Existing only — `clap` (new `ValueEnum` flag mirroring m219 `--split=<mode>` shape), `serde` / `serde_json` (doc-scope annotation value), `tracing` (FR-012 INFO log). Reuses milestone-215 `SubprojectRoot` + `project_for_root` + `enumerate_workspace_roots` verbatim as the discovery+BFS substrate; reuses milestone-127's `waybill:workspace-member` annotation as the workspace-member detection signal. **Zero new Cargo dependencies.**
**Storage**: N/A — all state in-process per scan. The scope filter operates on the already-in-memory `Vec<ResolvedComponent>` + `Vec<Relationship>` slices; no new persistent state.
**Testing**: `cargo +stable test --workspace` — unit tests for `ProjectDiscoveryMode::is_in_scope` (workspace/root-only/strict variants); integration tests extending a new `waybill-cli/tests/project_discovery_scope.rs` with fixtures for polyglot-nested-independent-projects, Cargo-workspace-with-independent-neighbor, npm-workspaces-with-cargo-neighbor.
**Target Platform**: linux-x86_64 + macOS + Windows (all three CI lanes). No eBPF surface touched.
**Project Type**: CLI-flag extension + post-discovery filter pass. Single crate touched (`waybill-cli`); the filter lives in a new `waybill-cli/src/generate/project_discovery/` module directory.
**Performance Goals**: Filter cost is O(N × E) where N = discovered components, E = relationships — dominated by the BFS-projection reachability computation which is the same shape as m215's existing per-projection BFS. On the largest realistic scans (~10k components), well under 100ms — trivial against total scan time.
**Constraints**: SC-005 byte-identity contract — `--project-discovery=all` (default) MUST produce byte-identical output to alpha.68 on every existing test fixture. Zero golden regeneration required for the default path. FR-011 doc-scope annotation absent on default-mode SBOMs; present only when non-default mode is used.
**Scale/Scope**: New CLI flag surface: 1 flag with 3 accepted values. New enum: `ProjectDiscoveryMode` with 3 variants + `is_in_scope` method. New filter pass: 1 module (~200 lines). New fixture: 2-3 polyglot layouts. New docs page: `docs/reference/project-discovery.md`.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Waybill Constitution v2.0.0 principles evaluated against this milestone:

- **I. Pure Rust, Zero C** — ✅ No C. New code is stdlib + workspace deps.
- **II. eBPF-Only Observation** — ✅ N/A. Post-resolve emit-time transformation over already-observed component sets.
- **III. Fail Closed** — ✅ Invalid `--project-discovery=<mode>` → CLI parse error → non-zero exit (per FR-010). FR-008 fallback (WARN log + synthetic-`pkg:generic/` root) preserves the "shallow means shallow, but still emit an SBOM" contract for the no-root-manifest case.
- **IV. Type-Driven Correctness** — ✅ `ProjectDiscoveryMode` is a new enum (not stringly-typed); `clap::ValueEnum` derive validates at parse time. No `.unwrap()` in production; test `.unwrap()` guarded per convention.
- **V. Specification Compliance** — ⚠️ Introduces ONE new `waybill:*` annotation: `waybill:project-discovery-mode` (doc-scope). Standards-native audit per Principle V:
  - **CDX `metadata.properties[]`**: no native "scan-scope was capped" concept. Rejected.
  - **SPDX 2.3 `creationInfo.creators[]`**: producer-scope (same rejection reasoning as m217 C136). Rejected.
  - **SPDX 3 `SpdxDocument.scope`**: no such field in 3.0.1. Rejected.
  - **The genuine signal**: consumers need to distinguish "this SBOM covers everything scannable at this root" from "this SBOM was scoped via --project-discovery=root-only." Silence on default preserves byte-identity; presence on non-default is auditable. **KEEP-NO-NATIVE** documented in `docs/reference/sbom-format-mapping.md` C140 per the m216/m217/m218/m219 precedent.
- **VI. Three-Crate Architecture** — ✅ Only `waybill-cli` touched. `waybill-common` untouched. `waybill-ebpf` untouched.
- **VII. Test Isolation** — ✅ New tests are unprivileged (no eBPF, no root). Runs under `cargo test --workspace` in every CI lane.
- **VIII. Completeness** — ✅ Under `--project-discovery=all` (default), completeness is unchanged. Under `root-only`/`strict`, completeness is OPERATOR-SCOPED: the SBOM covers what the operator asked for (root project + optionally workspace members) — nothing MORE is dropped than what the mode explicitly excludes. Doc annotation makes the scope decision auditable per Principle X.
- **IX. Accuracy** — ✅ No new inference. Filter is a pure post-hoc rearrangement of already-resolved components. Nothing spurious is added; things are only DROPPED per operator intent.
- **X. Transparency** — ✅ FR-011 doc-scope annotation `waybill:project-discovery-mode` emitted when non-default; FR-012 INFO log at scan-driver exit names the mode + counts. Operators + consumers see the scope decision.
- **XI. Enrichment** — ✅ N/A (no external data source).
- **XII. External Data Source Enrichment** — ✅ N/A (in-process filter over scan-local data).

**Constitution check result: PASS.** One new `waybill:*` annotation with a Principle-V audit clean (KEEP-NO-NATIVE precedent from m216-m219). No violations. No amendment required.

## Project Structure

### Documentation (this feature)

```text
specs/220-project-discovery-scope/
├── plan.md              # This file
├── spec.md              # Feature specification (committed 132ede3)
├── research.md          # Phase 0 output (this /speckit-plan pass)
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   ├── project-discovery-flag.md          # CLI flag surface + ValueEnum shape
│   ├── scope-filter-algorithm.md          # in_scope filter + BFS-projection pass
│   ├── workspace-member-preservation.md   # how root-only preserves waybill:workspace-member components
│   └── project-discovery-annotation.md    # C140 waybill:project-discovery-mode doc-scope annotation
├── checklists/
│   └── requirements.md  # Spec quality checklist (committed 132ede3)
└── tasks.md             # Phase 2 output — /speckit-tasks
```

### Source Code (repository root)

Single-crate (`waybill-cli`) touch; `waybill-common` + `waybill-ebpf` stay byte-identical.

```text
waybill-cli/
├── src/
│   ├── cli/
│   │   └── scan_cmd.rs                                              # + --project-discovery flag (ValueEnum, mirrors m219 shape); bridge to env var
│   ├── generate/
│   │   ├── project_discovery/                                       # NEW module dir
│   │   │   ├── mod.rs                                               #   ProjectDiscoveryMode enum + Default + Display + is_in_scope method
│   │   │   ├── filter.rs                                            #   apply_scope_filter(components, relationships, roots, scan_root, mode) → (Vec<ResolvedComponent>, Vec<Relationship>, ProjectDiscoveryReport)
│   │   │   └── report.rs                                            #   ProjectDiscoveryReport { mode, root_main_modules, workspace_members_followed, nested_projects_ignored }
│   │   ├── cyclonedx/
│   │   │   └── metadata.rs                                          # + emit C140 waybill:project-discovery-mode doc-scope annotation
│   │   ├── spdx/
│   │   │   ├── annotations.rs                                       # + emit C140 SPDX 2.3 doc-scope annotation
│   │   │   └── v3_annotations.rs                                    # + emit C140 SPDX 3 doc-scope annotation
│   │   └── mod.rs                                                   # + ScanArtifacts.project_discovery_mode field (Option<ProjectDiscoveryMode>) threaded through
│   ├── scan_fs/
│   │   └── mod.rs                                                   # + apply_scope_filter call BEFORE emit, gated on flag value; env-var bridge (WAYBILL_PROJECT_DISCOVERY)
│   └── parity/extractors/
│       ├── cdx.rs                                                   # + c140_cdx extractor
│       ├── spdx2.rs                                                 # + c140_spdx23 extractor
│       ├── spdx3.rs                                                 # + c140_spdx3 extractor
│       └── mod.rs                                                   # + C140 EXTRACTORS row + use-list additions
└── tests/
    ├── project_discovery_scope.rs                                   # NEW — 5+ scenarios per US1/US2/US3 + SC-005/SC-006/SC-007/SC-011
    └── fixtures/
        └── project_discovery/                                       # NEW
            ├── polyglot_nested_independent/                         #   root Cargo.toml + services/api/package.json + services/worker/go.mod (US1)
            ├── cargo_workspace_with_independent_neighbor/           #   root [workspace] + crates/{a,b} + bench/Gemfile (US2)
            └── strict_atomic/                                       #   reuse cargo_workspace_with_independent_neighbor for US3

docs/
├── reference/
│   ├── project-discovery.md                                         # NEW — FR-013 consumer-facing doc
│   └── sbom-format-mapping.md                                       # + C140 row per Principle V KEEP-NO-NATIVE template
└── (README.md — link to project-discovery.md added under SBOM interpretation)
```

**Structure Decision**: Post-discovery filter approach in a new `generate/project_discovery/` module. Filter runs after `enumerate_workspace_roots` populates the main-module set, BEFORE the emitters see the components. Reuses m215's `project_for_root` BFS routine to compute reachability from in-scope roots; drops unreachable components + relationships. Doc-scope annotation follows the m217 C136 shape (silence-on-default, present when non-default). Parity extractor row registered at C140.

## Complexity Tracking

> No constitution violations. Complexity table intentionally empty.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| — | — | — |
