# Phase 1 Data Model: Go graph resolver — per-main-module scoping

**Feature**: 233-go-per-mainmod-scope
**Date**: 2026-08-11

Two extensions to existing types; two new helpers; one modified emission loop. No new struct introduced beyond an extension to an existing one.

## Modified struct: `ModuleGraphEntry`

Existing definition at `waybill-cli/src/scan_fs/package_db/golang/graph_resolver.rs` (search `pub struct ModuleGraphEntry`). Add:

| Field | Type | Value semantics |
|---|---|---|
| `discovering_project_roots` | `HashSet<PathBuf>` | Every project_root whose `go.mod` or `go.sum` (or `go mod graph` invocation) produced or referenced this module. Multiple project_roots can co-contribute the same module — the set is the union. Consulted by `gosum_fallback_paths_for` to answer "did this module come from THIS project's manifests?" |

Every existing insertion path in `graph_resolver.rs` and its callers in `legacy.rs` MUST be extended to record which project_root is the source of the insertion. Insertion sites (grep target: `.insert(` on `ModuleGraphMap` / its inner `HashMap`):
- `parse_go_mod` output → insert with `project_root = go.mod's directory`
- `parse_go_sum` output → same
- `go mod graph` subprocess output → same
- Cache probe hits → same
- Proxy fetch results → same (the project_root that triggered the fetch)

## New method: `ModuleGraphMap::gosum_fallback_paths_for`

Signature:

```rust
pub fn gosum_fallback_paths_for(&self, project_root: &Path) -> Vec<String>
```

Returns only the modules whose `source == GoSumFallback` AND whose `discovering_project_roots` contains `project_root`. Same shape as the existing `gosum_fallback_paths()` (which becomes deprecated or scan-global-only for callers that legitimately need the aggregate; TBD during implementation whether to keep it around or delete it).

## Modified: `build_main_module_entry` augmentation

Location: `waybill-cli/src/scan_fs/package_db/golang/legacy.rs:~1893`.

Existing:
```rust
let fallback_paths = graph_map.gosum_fallback_paths();
```

Post-232:
```rust
let fallback_paths = graph_map.gosum_fallback_paths_for(project_root);
```

Plus a new pass that inserts sibling main-module edges when the current `go.mod`'s `replace` directives point at another discovered main-module's project_root. Concretely: for each `replace old => local-path` in the current doc, check if `local-path` (canonicalized relative to project_root) equals another entry in `parsed_roots`. If yes, add the sibling's `module_path` to `main_entry.depends`.

## Modified: stdlib injection (FR-008)

Location: `waybill-cli/src/scan_fs/package_db/golang/legacy.rs:2304` (existing `e.depends.push("stdlib".to_string())`).

Post-232: use the current main-module's `go <version>` directive to produce a versioned stdlib name:

```rust
let stdlib_name = format!("stdlib@{}", go_version);  // or similar encoding
e.depends.push(stdlib_name);
```

Then a corresponding component emitter creates one `pkg:golang/stdlib@<version>` component per distinct Go version discovered across the scan. Two options for where the emission code lives:

- **Option 1** (preferred): extend the existing stdlib-component emission in `build_entries_from_go_module_with_lookup` (grep for where the stdlib entry currently gets emitted) to emit one per distinct Go version rather than one global.
- **Option 2**: emit stdlib components lazily as they're referenced in `main_entry.depends`; requires the emission loop to track already-emitted versions to dedup.

Chosen: **Option 1** — deterministic, upfront emission is simpler to reason about.

## `waybill:workspace-member` union (FR-004)

**No code change needed** at the tagging layer. The m176 `tag_components_with_workspace_member` at `scan_fs/mod.rs:1290` already computes the union via `BTreeSet<String>` collection over `evidence.source_file_paths`. Post-fix, when two main-modules contribute the same `(name, version)` module, the shared component's `source_file_paths` naturally accumulates both go.sum paths, and the tagging pass emits the union.

Verified during Phase 2 implementation with a specific unit test.

## Out-of-scope

- Changes to `waybill-cli/src/generate/project_discovery/`. Verified upstream root cause; no project-discovery changes needed.
- Changes to `waybill:workspace-member` tagging logic. Already produces the union.
- Changes to any other reader (npm, cargo, maven, etc.). Fix is Go-specific.
- Changes to the `scan_fs/mod.rs:526-547` edge-emission loop. Existing `depends → PURL` lookup logic works unchanged.
