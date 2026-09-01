# Contract: Lockfile readers

**Files**:
- `waybill-cli/src/scan_fs/package_db/pip/uv_lock.rs`
- `waybill-cli/src/scan_fs/package_db/pip/poetry_lock.rs`
- `waybill-cli/src/scan_fs/package_db/pip/pdm_lock.rs`
- `waybill-cli/src/scan_fs/package_db/pip/pipfile_lock.rs`

**FRs covered**: FR-008, FR-009, FR-010, FR-011, FR-012, FR-014, FR-015, FR-016
**Called by**: `pip/mod.rs::dispatch` when a lockfile filename is discovered

## Shared shape

Each of the four lockfile readers returns `Vec<PackageDbEntry>` with one entry per `LockedPackage`. Emitted components:
- `purl = pkg:pypi/<name>@<version>` — version is ALWAYS resolved from the lockfile
- `hashes` populated from the lockfile's hash fields
- `lifecycle_scope` derived from format-specific scope signal (see per-format below)
- `source_file_paths = [<lockfile-path>]`
- `extra_annotations`:
  - `waybill:python-lockfile-format = "uv|poetry|pdm|pipfile"` — per format identification (parity catalog row C159)

## Per-format specifics

### `uv.lock` reader

Input format: TOML, per <https://docs.astral.sh/uv/reference/#lockfile>.

Key parsing:
- Top-level `version` field (schema version) — accept `1`; if unknown, warn and continue best-effort
- `[[package]]` array-of-tables — each entry produces one component
- `[[package]] version` — required; if absent, warn+skip that entry
- `[[package]] source` — table; `registry = "https://pypi.org/simple"` indicates default PyPI; other keys (git, path) → also emit `waybill:direct-url-source` if applicable
- `[[package.wheels]] hash` — SHA-256; extract into `hashes`
- `[[package.dependencies]]` — record names (for future graph-edge use; NOT required in this milestone)

Scope: uv.lock doesn't natively encode dev/optional groups in the lockfile itself (uv's dev-groups are resolved into the flat lockfile). Waybill emits all uv-lock entries with `Main` scope. To recover group scope, the reader ALSO reads the sibling `pyproject.toml` and cross-references (this is the reconciler's job; the reader emits `Main` and lets m191 upgrade the scope tag from the manifest side).

### `poetry.lock` reader

Input format: TOML, per <https://python-poetry.org/docs/repositories/>.

Key parsing:
- `[[package]]` array-of-tables
- `category` field — `"main"` / `"dev"` / arbitrary → `Main` for `"main"`, `Optional { scope_name: <category> }` for others
- `optional` field — `true` → force `Optional`
- `[metadata.files] <name>` — table with per-file hashes; extract into `hashes`
- `[package.dependencies]` — record names (future edge-use)

Scope: poetry.lock natively encodes category; use it directly.

### `pdm.lock` reader

Input format: TOML, per <https://pdm-project.org/en/latest/usage/lockfile/>.

Key parsing:
- `[[package]]` array-of-tables
- `groups` array field — one or more group names; presence of `"default"` → `Main`; any other group → `Optional { scope_name: <group> }`
- `[[package.files]] hash` — SHA-256
- `[[package.dependencies]]` — record names (future edge-use)

Multi-group handling: if a package is in `default` AND another group, `Main` wins (matches lockfile precedence rule from spec).

### `Pipfile.lock` reader

Input format: JSON, per <https://github.com/pypa/pipfile>.

Key parsing:
- Top-level `default` object (`Main` scope) + top-level `develop` object (`Optional { scope_name: "dev" }`)
- Each key is a package name; each value has:
  - `version` field — with `==` prefix (strip)
  - `hashes` array — mixed SHA-256 and SHA-1 possible; extract both
  - `index` field — usually `"pypi"`; if custom, attach `waybill:index-url`

## Reconciliation contract

Every lockfile-emitted component's version is authoritative under FR-012. When the same PURL is emitted by:
- A manifest reader (with `@unresolved` + constraint) AND a lockfile reader (with resolved version) → lockfile version wins, constraint annotation preserved
- Multiple lockfile readers (Q1 clarification) → collapsed by m191, all `source_file_paths` preserved, version disagreement stays visible in evidence

## Error behavior

- Malformed TOML/JSON → warn+skip, return `Ok(vec![])`
- Unknown schema version → best-effort continue (log at INFO level)
- Missing required field on an entry (e.g., no `version`) → skip that entry only (not the whole file), warn per-entry
- **Never** propagates `Err`

## Test coverage

Unit tests per reader, using minimal 3-5 entry fixtures inline:
- `uv_lock`: parses_v1_schema, extracts_sha256_hashes, respects_direct_url_source, warns_on_unknown_schema_version
- `poetry_lock`: parses_main_category, parses_dev_category, extracts_metadata_files_hashes, respects_optional_flag
- `pdm_lock`: parses_default_group, parses_multi_group_precedence, extracts_file_hashes
- `pipfile_lock`: parses_default_and_develop_sections, extracts_mixed_sha256_sha1, strips_double_equals_prefix

## Annotations (parity catalog)

- **C159** `waybill:python-lockfile-format` — `SymmetricEqual`; carries `uv|poetry|pdm|pipfile`
- Reuses C154 `waybill:version-constraint` (from pyproject reader) when manifest constraint survives reconciler
- Reuses C155 `waybill:python-extras` when applicable
- Reuses C157 `waybill:direct-url-source` for non-PyPI-index entries
