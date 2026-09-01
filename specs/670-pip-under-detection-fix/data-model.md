# Phase 1 Data Model: Python under-detection fix

**Feature**: 670-pip-under-detection-fix
**Date**: 2026-08-31

## Overview

This milestone introduces no new persistent data model — all state is in-process per scan, per the workspace convention since m002. What follows is the in-memory shape of the parsed artifacts that flow between the new reader files and the existing `waybill-cli/src/scan_fs/mod.rs::apply_python_reconciler` pass.

## Entities

### `PyProjectManifest`

Parsed representation of a single `pyproject.toml`. Populated from one of three shapes (PEP 621, PEP 735, Poetry-legacy); the parser records which shape(s) were present.

**Fields**:
- `project_name: Option<String>` — from `[project].name` or `[tool.poetry].name`
- `project_version: Option<String>` — from `[project].version` or `[tool.poetry].version`
- `manifest_shape: PyManifestShape` — enum: `Pep621`, `Pep621WithPep735`, `PoetryLegacy`, `PoetryLegacyOnly`
- `main_dependencies: Vec<DeclaredDep>` — from `[project.dependencies]` OR (fallback) `[tool.poetry.dependencies]`
- `optional_dependencies: BTreeMap<String, Vec<DeclaredDep>>` — from `[project.optional-dependencies]` groups (keyed by group name) AND `[tool.poetry.group.<name>.dependencies]`
- `dependency_groups: BTreeMap<String, Vec<DeclaredDep>>` — from `[dependency-groups]` (PEP 735); each is scoped as `optional` with the group name
- `poetry_dev_dependencies: Vec<DeclaredDep>` — from `[tool.poetry.dev-dependencies]` (scope=dev)
- `source_path: PathBuf` — absolute path to the manifest file

**Invariants**:
- If `manifest_shape == Pep621WithPep735`, both `main_dependencies` (from PEP 621) and `dependency_groups` (from PEP 735) may be populated
- If `manifest_shape == PoetryLegacyOnly`, `main_dependencies` is populated from Poetry-legacy sections; `optional_dependencies` may still be populated from `[tool.poetry.group.*.dependencies]`
- `[project.dependencies]` (PEP 621) takes precedence over `[tool.poetry.dependencies]` when both are present (FR-003a precedence rule)

### `DeclaredDep`

A single dependency declaration from any manifest source, before lockfile-authoritative version resolution.

**Fields**:
- `name: String` — package name (name-normalized per PEP 503 at PURL construction time, not stored normalized)
- `version_constraint: Option<String>` — the raw constraint string, e.g., `">=1.0,<2"`. `None` means unpinned.
- `extras: Vec<String>` — from `pkg[extra1,extra2]` syntax
- `pep508_marker: Option<String>` — the `;`-suffixed environment marker if present
- `direct_url: Option<DirectUrlRef>` — populated for `pkg @ git+https://...` entries
- `is_editable: bool` — `true` for `-e .` / `-e git+...` entries
- `scope: LifecycleScope` — from the m179/m180 enum; `Main` or `Optional { scope_name }`
- `source_file: PathBuf` — the manifest / requirements file that declared this dep

### `DirectUrlRef`

FR-005b — captures PEP 508 direct-URL and VCS-URL references.

**Fields**:
- `url: String` — the full URL as written
- `kind: DirectUrlKind` — enum: `VcsGit { rev: Option<String> }`, `VcsHg`, `Archive`, `LocalPath`
- `resolved_rev: Option<String>` — for VCS URLs, the rev if statically extractable from the URL fragment

### `LockedPackage`

Represents one entry from any of the four lockfile formats.

**Fields**:
- `name: String`
- `version: String` — always populated (lockfiles are by definition resolved)
- `hashes: Vec<Hash>` — SHA-256 (uv.lock, pdm.lock, poetry.lock) or SHA-1 (Pipfile.lock supports both)
- `scope: LifecycleScope` — from lockfile category/groups/develop section
- `dependencies: Vec<String>` — names of packages this depends on (for future graph-edge use; not required for FR-001–FR-011)
- `source_file: PathBuf` — the lockfile that declared this package
- `lockfile_format: LockfileFormat` — enum: `UvLock`, `PoetryLock`, `PdmLock`, `PipfileLock`

### `Hash`

Reuses `waybill_common::types::hash::ContentHash` (existing type).

### `LifecycleScope`

Reuses `waybill_common::resolution::LifecycleScope` (existing enum, m179/m180). Variants:
- `Main`
- `Optional { scope_name: String }` — populated with `dev`, `test`, `docs`, `ci`, or the PEP 735 group name / Poetry group name

### `RequirementsFileClassification`

Output of the FR-005a heuristic.

**Fields**:
- `file_path: PathBuf`
- `derived_scope: RequirementsScope` — enum: `Main`, `Dev`, `Test`, `Docs`, `Ci`
- `parent_dir_signal: Option<String>` — `"docs"`, `"tests"`, `"ci"` if the parent directory matched
- `filename_signal: Option<String>` — `"requirements-dev"`, `"dev-requirements"`, etc.

**Derivation** (FR-005a):
1. If parent-dir name ∈ {`docs`, `test`, `tests`, `ci`} → `Optional` with parent-name as scope_name
2. Else if filename matches `requirements-{dev,test,docs}*.txt` or `{dev,test,docs}-requirements.txt` → `Optional` with derived scope_name
3. Else → `Main`

### `VenvPruneSet`

Static allowlist per FR-004 extended. Not persisted; a `const &[&str]` list checked at walker-descent time.

**Contents**:
```rust
&[
    "site-packages",
    ".venv",
    "venv",
    ".tox",
    "*.egg-info",
    "build",
    ".eggs",
]
```

**Override**: `--include-python-vendored` CLI flag disables the prune-set for a given scan.

### `PythonComponent`

Reuses `waybill_common::resolution::ResolvedComponent` (existing type). Fields populated:
- `purl: Purl` — always `pkg:pypi/<name>@<version>`
- `name: String`
- `version: String` — `unresolved` (const) when neither lockfile nor pinned requirements provide a version
- `hashes: Vec<Hash>` — from lockfiles when available
- `lifecycle_scope: LifecycleScope`
- `source_file_paths: Vec<PathBuf>` — evidence trail (FR-014)
- `extra_annotations: Vec<Annotation>` — including:
  - `waybill:unresolved-reason` (m236) with one of the locked reason strings
  - `waybill:python-req-file-scope` (new) with the derived scope name
  - `waybill:direct-url-source` (new) with the URL + rev
  - `waybill:version-constraint` (new? or reuse existing) with the raw PEP 440 constraint
  - `waybill:pep508-marker` (new? or reuse existing) with the environment marker string

## Relationships

**Manifest → Component**: One `PyProjectManifest` typically produces:
- 1 main-module component (project itself, from `project_name/project_version`)
- N components from `main_dependencies` (unresolved unless a paired lockfile provides versions)
- M components from `optional_dependencies` groups (scoped)

**Lockfile → Component**: One `LockedPackage` produces exactly one `PythonComponent`; version is authoritative.

**Reconciliation** (m191): When the same PURL is produced by multiple readers (e.g., `pyproject.toml [project.dependencies]` declares `requests>=2.28` AND `uv.lock` locks `requests==2.31.0`), the m191 reconciler collapses into one component:
- Version = lockfile's (FR-012)
- `source_file_paths` = union
- `lifecycle_scope` = manifest's (declarative intent) when lockfile doesn't disagree

**Multi-lockfile** (Q1 clarification): When multiple lockfiles produce the same PURL at different versions, m191 collapses to one component and preserves both `source_file_paths`; version disagreement is diagnosed via evidence, not annotated as a conflict.

## State transitions

Not applicable — pure in-memory computation per scan. No persistence, no lifecycle.

## Validation rules

Enforced at reader level:

| Rule | FR reference | Enforcement |
|------|--------------|-------------|
| PURL name follows PEP 503 normalization | FR-001 (implicit; PURL spec compliance) | `Purl::new()` validates |
| Version=`unresolved` triggers `waybill:unresolved-reason` | FR-013 | reader emits both together (compile-time-linked via a constructor helper) |
| No component without at least one `source_file_paths` entry | FR-014 | asserted at reader-exit; test coverage |
| No Python code execution | FR-015 | code-review + grep audit against `std::process::Command::new("python*")` |
| Parse-fail warn-and-skip | FR-016 | `Result<Vec<PackageDbEntry>, ParseError>` per file; `Ok(vec![])` on skip |
| PyProject shape precedence | FR-003a | `manifest_shape` field records which won; validated in unit test |
