# Phase 0 Research: Go graph resolver — per-main-module `dependsOn` scoping

**Feature**: 233-go-per-mainmod-scope
**Date**: 2026-08-11

All decisions grounded in the empirical repro + existing code inspection. The two open ambiguities from spec drafting were resolved in `/speckit.clarify` (Session 2026-08-11).

## R1 — Root cause of the leak: where the aggregation happens

**Decision**: The bug lives in `waybill-cli/src/scan_fs/package_db/golang/graph_resolver.rs::ModuleGraphMap::gosum_fallback_paths()`. It returns a scan-global aggregate. Every call site in `legacy.rs::build_main_module_entry` (~line 1893) consumes the aggregate and inserts it into that specific main-module's `depends` list. Every main-module gets the same set. Fix scoping: introduce a `_for(project_root: &Path)` variant that returns only the modules whose source-file provenance names that project_root's `go.sum`.

**Evidence**:
- `legacy.rs:1619` iterates `project_roots` and parses each's `go.mod` + `go.sum`, populating a single shared `graph_map`.
- `legacy.rs:1893` calls `graph_map.gosum_fallback_paths()` — no project_root argument.
- `legacy.rs:1897` unconditionally pushes every returned path into `main_entry.depends`.

The reporter's "deepest-writer-wins" observation is explained by: each `parse_go_sum` call inserts entries into `graph_map` in walk order; when the deepest module is processed last, its version overwrites earlier entries in the shared map because module IDs are keyed by name-only, not (name, discovering_project).

**Alternatives considered**:
- *Filter after the fact in `build_main_module_entry`.* Rejected: filtering post-aggregation still requires knowing which entries came from which go.sum, which is exactly the provenance we need to track in the graph. Might as well track it at insert time.

## R2 — Per-main-module provenance tracking

**Decision**: Extend `ModuleGraphMap`'s per-module entry (see `graph_resolver.rs::ModuleGraphEntry`) with a `discovering_project_roots: HashSet<PathBuf>` field. Every insert path (`parse_go_sum`, `go mod graph` output, cache probe, proxy fetch, gosum_fallback) records which project_root's `go.mod` / `go.sum` contributed this module. Multiple project_roots can contribute the same module — the field is a union, not a single value.

`gosum_fallback_paths_for(project_root: &Path)` then filters the fallback set to entries whose `discovering_project_roots` contains `project_root`.

**Rationale**: Preserves the existing shared-graph performance (single-parse per go.sum, single deps.dev lookup) while adding per-project scoping at query time. Zero cost when only one project_root contributes an entry (single-element set).

**Alternatives considered**:
- *Per-project `ModuleGraphMap` instances.* Rejected: duplicates the resolver's work (each map re-fetches, re-walks). N-times slowdown for N project_roots.
- *Filter by walking source_file_paths.* Rejected: `source_file_paths` is per-component metadata; ModuleGraphMap operates on `ModuleId` (name-only) keys. Filtering that way requires the exact provenance data we're already suggesting to track.

## R3 — `replace` directive handling (Clarifications §2)

**Decision**: Per Clarifications §2, a `replace some.example.com/B => ../local/B` in module A's `go.mod` (where `../local/B` is a discovered main-module) produces an edge A `dependsOn` B's main-module PURL (`pkg:golang/some.example.com/B@v0.0.0-unknown` or the version A declares in its `require`).

Implementation: `build_main_module_entry` reads A's parsed `go.mod`'s `replace` directives. For each replace whose target is a filesystem path resolvable to another discovered main-module's project_root, add the sibling's module name to `main_entry.depends`. The `scan_fs/mod.rs:526-547` edge-emission loop already translates `depends` name-strings to PURLs via the shared `name_to_purl` lookup — no new resolution logic needed.

**Rationale**: Matches `go mod graph` and the reporter's expected behavior. No inlining; A's dependsOn contains B but not B's transitive graph.

**Alternatives considered**:
- Options B and C from Clarifications §2 (already rejected).

## R4 — `stdlib` per-Go-version handling (Clarifications §1 / FR-008)

**Decision**: `legacy.rs:2304` currently pushes `"stdlib".to_string()` into `main_entry.depends`. This resolves via `name_to_purl` to a single `pkg:golang/stdlib@<some-version>` component regardless of the main-module's declared Go version. Fix: parse each `go.mod`'s `go <version>` directive (already stored on `GoModDocument::go_version` or equivalent — verified during Phase 2), and push `format!("stdlib@{}", go_version)` OR change the emission to construct a version-qualified name that the `name_to_purl` lookup recognizes.

The version-qualified `stdlib` components are constructed at scan time from the same `parse_go_mod` output — no new deps or parsing.

**Rationale**: Minimal-change encoding of Clarifications §1's decision. Uses existing `depends: Vec<String>` slot rather than introducing a new field.

**Alternatives considered**:
- *Special-case stdlib in the emission loop.* Rejected: less uniform than treating stdlib the same as any other per-version dependency.

## R5 — Test fixture shape

**Decision**: Three new fixtures under `waybill-cli/tests/fixtures/golden_inputs/golang/`:

1. `per_mainmod_scope_4modules/` — the reporter's minimal repro verbatim (root + hack + tools + deep/src/thing, each requiring `x/text` at a distinct synthetic-name version). Under synthetic names: use `example.com/mikebomfixture/text` v0.40/v0.37/v0.29/v0.25 to match the shape but keep memory `feedback_fixture_synthetic_package_names` policy.
2. `per_mainmod_scope_shared_ver/` — 2 modules (hack + tools) both requiring `mikebomfixture/text v0.29.0`. Asserts FR-004: single component with union `waybill:workspace-member: ["hack", "tools"]`.
3. `per_mainmod_scope_mixed_go/` — 2 modules with distinct `go 1.24.0` / `go 1.22.5` directives. Asserts FR-008: two distinct `pkg:golang/stdlib@v1.24.0` and `pkg:golang/stdlib@v1.22.5` components, each main-module points at its own.

**Rationale**: Three fixtures matches the three distinct assertion families (US1 + US2 + FR-008). Each is tight, self-contained, and mirrors real-world shapes.

**Alternatives considered**:
- *One giant combined fixture.* Rejected: harder to debug individual assertion failures.

## R6 — Integration-test scaffold

**Decision**: Reuse the `common::bin` + `apply_fake_home_env` subprocess pattern from `waybill-cli/tests/nuget_main_module_parity.rs` (m230) and `golang_workspace_mode_preflight.rs` (m231). New integration-test file `waybill-cli/tests/go_per_mainmod_scope.rs` with 3-4 tests per fixture across `--project-discovery` modes.

**Rationale**: Battle-tested pattern; zero new test-infrastructure code.

**Alternatives considered**: None — pattern is standard.
