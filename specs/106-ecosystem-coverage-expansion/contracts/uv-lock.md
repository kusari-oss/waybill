# Contract: uv.lock reader (US1, FR-001, FR-002, FR-015)

**New module**: `mikebom-cli/src/scan_fs/package_db/pip/uv_lock.rs`

## Trigger

A file named `uv.lock` at the scan root, or in any descendant directory.

## Parsing

`toml::from_str` (existing workspace dep) into the `UvLockfile` struct from `data-model.md`. The lockfile structure is a top-level `[[package]]` array per uv 0.5+ format.

## PURL derivation per package

| `UvSource` variant | PURL |
|---|---|
| `Registry` (default) | `pkg:pypi/<name>@<version>` |
| `Workspace` (workspace member) | `pkg:pypi/<name>@<version>` + `mikebom:component-role: "main-module"` |
| `Git { url, revision }` | `pkg:git+https://<url>@<revision>` |
| `Path { path }` | `pkg:generic/<name>` + `mikebom:source-type: "local"` |

## Workspace handling

When the root `pyproject.toml` declares `[tool.uv.workspace]`:

1. Emit a synthetic workspace-root component:
   - `pkg:generic/<root-name>` (where `root-name` = root pyproject.toml's `name` field, or `"workspace-root"` placeholder)
   - `mikebom:component-role: "workspace-root"`
   - `mikebom:source-files: "<root pyproject.toml path>"`
2. Add `dependsOn` edges from workspace-root → each workspace member (identified by `source = { workspace = true }`).
3. Resolve intra-workspace edges: for each workspace member's `[[package.dependencies]]` entries that name another workspace member, add a `dependsOn` edge between them.

## Annotations emitted (per component)

| Annotation | Value |
|---|---|
| `mikebom:source-files` | absolute path of `uv.lock` (or `pyproject.toml` for the workspace-root) |
| `mikebom:component-role` | `"workspace-root"` (root only), `"main-module"` (workspace members only), absent for transitive deps |
| `mikebom:source-type` | `"local"` for Path-source members; absent otherwise |

## Edge cases handled

- **Source-only entry** (no `version =` at top level — rare): emit `tracing::warn!` naming the unresolvable package, skip the entry.
- **Workspace member with no `version`**: fall back to reading `version` from the member's own `pyproject.toml` (if present); else `"unknown"` with `tracing::warn!`.
- **Git source with no `revision`**: `pkg:git+https://<url>@unknown` + `tracing::warn!`.

## Test fixtures

Per project structure in plan.md:

- `tests/fixtures/golden_inputs/uv_lock/basic/` — 3-package PyPI-only lockfile
- `tests/fixtures/golden_inputs/uv_lock/with_dependencies/` — `[[package.dependencies]]` graph
- `tests/fixtures/golden_inputs/uv_lock/workspace/` — root + 2 members, one intra-workspace dep
- `tests/fixtures/golden_inputs/uv_lock/source_only/` — degenerate case for the warn path
