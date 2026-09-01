# Contract: `pyproject.toml` reader

**File**: `waybill-cli/src/scan_fs/package_db/pip/pyproject_toml.rs`
**FRs covered**: FR-001, FR-002, FR-003, FR-003a, FR-014, FR-015, FR-016
**Called by**: `pip/mod.rs::dispatch` when a `pyproject.toml` is discovered by the m664 walker

## Input

- `path: &Path` — absolute path to a `pyproject.toml`
- `walker_context: &SharedWalkerContext` — for cross-reader coordination (m664)

## Output

`Vec<PackageDbEntry>` — one entry per declared dependency + one entry for the manifest's own project (main-module component per m064).

### Manifest-shape detection

Reader tries three shapes in this order:

1. **PEP 621**: `[project]` block with a `dependencies` array present → `manifest_shape = Pep621`
2. **PEP 735 addendum**: also `[dependency-groups]` present → `manifest_shape = Pep621WithPep735`
3. **Poetry-legacy fallback**: no `[project.dependencies]` but `[tool.poetry.dependencies]` present → `manifest_shape = PoetryLegacyOnly`

If none of these succeed, the reader emits ONLY the main-module component (from either `[project].name/version` or `[tool.poetry].name/version` if either is present) and warns via `tracing::debug!("pyproject.toml at {} has no dependency declarations", path.display())`.

### Dependency-emission per shape

| Shape | Reader reads | Emitted scope |
|-------|--------------|---------------|
| Pep621 | `[project.dependencies]` | `Main` |
| Pep621 | `[project.optional-dependencies]` groups | `Optional { scope_name: <group> }` |
| Pep621WithPep735 | Above + `[dependency-groups]` | Groups emitted as `Optional { scope_name: <group> }` |
| PoetryLegacyOnly | `[tool.poetry.dependencies]` | `Main` |
| PoetryLegacyOnly | `[tool.poetry.dev-dependencies]` | `Optional { scope_name: "dev" }` |
| PoetryLegacyOnly | `[tool.poetry.group.<name>.dependencies]` | `Optional { scope_name: <name> }` |

### Version resolution

Constraint-only (no paired lockfile at this reader):
- `"^1.2"` / `">=1,<2"` / `"==1.0"` — extract, emit `pkg:pypi/{name}@unresolved`, attach `waybill:version-constraint` annotation with raw string
- `"*"` / no value — emit `@unresolved` + `waybill:unresolved-reason = "python-unpinned-manifest"`

Lockfile-authoritative version reconciliation happens downstream at m191, not here.

### Direct URL / VCS handling (FR-005b)

PEP 508 direct-URL constraint (e.g., in Poetry's `requests = { git = "https://github.com/psf/requests", rev = "v2.31" }`) → emit `pkg:pypi/requests@<rev-or-unresolved>` + `waybill:direct-url-source` annotation with the URL and rev.

### PEP 508 marker preservation

If a dep entry includes an environment marker (`; python_version >= '3.10'` for Poetry, or the TOML markers-table syntax), extract the marker string and attach as `waybill:pep508-marker` annotation. Component is emitted regardless of whether the marker evaluates true on the scanning host (spec edge case; component is *declared*, not *effective*).

### Extras (`pkg[extra1,extra2]`)

Emit ONE component per package (not per extra). The extras array is preserved as a `waybill:python-extras` annotation.

### Sub-project detection

If nested `pyproject.toml` files are discovered by the walker (m664), each is dispatched independently. m127 workspace-member detection applies at the reconciler layer, not this reader. Each nested manifest produces its own main-module component per m064.

## Error behavior

- File exists but is not valid TOML → `tracing::warn!("skipping malformed pyproject.toml at {}", path.display())`, return `Ok(vec![])`
- File exists but declares no `[project]` and no `[tool.poetry]` → return `Ok(vec![])` (nothing to emit; not our concern)
- File cannot be read (permission denied, disappearing symlink) → `tracing::warn!`, return `Ok(vec![])`

**Never** propagates `Err` from `read`.

## Annotations emitted (parity-catalog registration)

For every new `waybill:*` annotation, per Principle V, we audit:

| Annotation | Data | Native alternative? | Justification |
|------------|------|---------------------|---------------|
| `waybill:version-constraint` | raw PEP 440 constraint | CDX has no field; SPDX 2.3 has no field; SPDX 3 has no field | No native carrier for pre-resolution constraint strings |
| `waybill:python-extras` | array of extra names | CDX no; SPDX no | No native array-of-optional-features carrier |
| `waybill:pep508-marker` | marker string | CDX no; SPDX no | No native carrier for PEP 508 markers |
| `waybill:direct-url-source` | `{url, kind, resolved_rev}` object | CDX has `externalReferences[]` with type `vcs` — evaluate whether that suffices | **Precedence decision**: emit BOTH — CDX `externalReferences[vcs]` for standards-native discovery AND `waybill:direct-url-source` for the finer-grained `resolved_rev` + `kind` split that CDX doesn't carry |

New parity-catalog rows added: C154 `waybill:version-constraint`, C155 `waybill:python-extras`, C156 `waybill:pep508-marker`, C157 `waybill:direct-url-source`. All `Directionality::SymmetricEqual`. All added in `docs/reference/sbom-format-mapping.md` + `parity/extractors/` per m145+m147 convention.

## Test coverage

Unit tests in `pip/pyproject_toml.rs` `#[cfg(test)] mod tests`:
- `parses_pep621_dependencies` — 5-line pyproject with 3 deps
- `parses_optional_dependencies` — with `docs` + `test` groups
- `parses_pep735_dependency_groups` — with `[dependency-groups]`
- `parses_poetry_legacy_only` — no `[project]`, only `[tool.poetry]`
- `precedes_pep621_over_poetry_legacy` — both present, PEP 621 wins
- `skips_malformed_toml` — asserts `warn!` + empty return
- `skips_pyproject_with_no_project_block` — pyproject.toml for a build backend
- `preserves_version_constraint_annotation`
- `preserves_pep508_marker`
- `extras_split_into_annotation`
