# Data Model: pip source-tree main-module component

## Entities

### PipMainModuleEntry (no new Rust type — constrained `PackageDbEntry`)

| Field | Value | Source | FR |
|-------|-------|--------|-----|
| `purl` | `pkg:pypi/<pep503-name>@<version>` | `[project].name` (PEP 503-normalized) + `[project].version` (or placeholder) | FR-001 |
| `name` | `[project].name` verbatim (NOT normalized — display value) | manifest | FR-001 |
| `version` | literal `[project].version` or `"0.0.0-unknown"` placeholder | manifest, with placeholder fallback | FR-001 |
| `source` | `Some("path+file://<absolute-pyproject-toml-dir>")` | filesystem walker | (existing convention) |
| `lifecycle_scope` | `None` | n/a (Runtime by default) | (out of scope) |
| `sbom_tier` | `Some("source")` (overridden to `"deployed"` on editable-install merge per FR-011) | constant; venv-overrides | FR-006 + FR-011 |
| `extra_annotations` | BTreeMap with `mikebom:component-role: "main-module"` | constant | FR-004 |
| `parent_purl` | `None` | constant (top-level) | FR-001a |
| `depends` | `Vec<String>` of direct-dep names from `[project.dependencies]` + `[project.optional-dependencies].*` | manifest tables | FR-007 |
| `licenses` | `vec![]` (empty) | constant | FR-005 |
| `hashes` | `vec![]` (empty; venv-merged value when Tier-1 venv reader supplied hashes) | constant + venv-overrides | FR-011 |

### DroppedDuplicate (private helper struct, mirrors cargo/npm pattern)

```rust
struct DroppedDuplicate {
    pub purl: String,
    pub kept_path: String,
    pub dropped_path: String,
}
```

## Relationships

### Direct-dep edges from main-module to dep targets

```text
Relationship {
    from: <pip-main-module-purl>,           // pkg:pypi/<name>@<version>
    to: <dep-target-purl>,                  // pkg:pypi/<dep>@<version>
    relationship_type: DependsOn,
    provenance: {
        source: "<absolute-pyproject.toml-path>",
        data_type: "pyproject-direct-dep",
    },
}
```

Existing edge-emission machinery in `scan_fs/mod.rs` resolves these via `name_to_purl` + dangling-target dropping.

### DESCRIBES relationship (document → main-module)

Inherits the multi-DESCRIBES wiring from milestone 064 + #127. Single-project: length-1 `documentDescribes`. Multi-project polyglot: length-N, sorted alphabetically by SPDXID.

### Editable-install merge (FR-011)

When the venv reader (`pip/dist_info.rs`) emits a `.dist-info`-derived entry whose PURL matches a Phase-A main-module's PURL, the augment-existing-entry logic merges:
- `sbom_tier`: venv's `"deployed"` wins over Phase A's `"source"`
- `evidence_kind`: venv's filesystem-evidence wins
- `hashes`: venv's METADATA-derived hashes added
- `extra_annotations`: Phase A's C40 added (or kept if already present)
- `parent_purl`: forced to `None` (main-module is top-level)
- everything else: existing entry's values preserved

## State transitions

None — main-module emission is read-only and deterministic.

## Validation rules

| Rule | Source | Failure mode |
|------|--------|--------------|
| `pyproject.toml` MUST contain `[project].name` to emit a main-module | FR-001 | Skip silently |
| `[tool.poetry]`-only manifest (no `[project]`) MUST be skipped | FR-002 | `tracing::info!` notes the skip |
| Both `[project]` AND `[tool.poetry]` present: emit from `[project]` | FR-003 | (no failure mode) |
| Name MUST be PEP 503-normalized in the PURL | FR-001 | Use existing `normalize_pypi_name_for_purl` |
| `dynamic = ["version"]` AND no literal version → `0.0.0-unknown` placeholder | FR-001 | Deterministic |
| `[project]` present, `version` missing, NOT in `dynamic` → placeholder + `tracing::warn!` | FR-001 | Lenient parse |
| Same-PURL emissions MUST be deduplicated to one entry | FR-001 + spec edge cases | First-discovered wins; `tracing::warn!` |

## Reuses from milestones 053+064+066+#127

- `normalize_pypi_name_for_purl` (existing `pip/mod.rs:71`) — PEP 503 normalization
- `build_pypi_purl_str` (existing `pip/mod.rs:79`) — PURL builder with `+` → `%2B` for local version segments
- `candidate_python_project_roots` (existing `pip/mod.rs:171`) — manifest discovery walker
- C40-tag-driven CDX `metadata.component` selector + components[] exclusion (milestone 064)
- C40-tag-driven SPDX `primaryPackagePurpose` predicate (milestone 053+064)
- Multi-root `documentDescribes` + per-root DESCRIBES Relationship (#127)
- Multi-root SPDX 3 `rootElement` + per-root describes Relationship (#127)
- Cargo's `dedup_main_modules_by_purl` pattern (milestone 064 T010) — copy-adapt to pip

## Does NOT introduce

- No new public Rust type
- No new crate dependency
- No new CLI flag
- No new SBOM annotation key
- No subprocess calls (no setuptools-scm shellout)
- No version-inheritance / workspace-context map
