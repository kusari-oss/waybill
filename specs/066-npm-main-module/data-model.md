# Data Model: npm source-tree main-module component

## Entities

### NpmMainModuleEntry (new conceptual entity, no new Rust type)

A `PackageDbEntry` constrained to represent a synthetic main-module emitted by the npm source-tree reader for a single `package.json`.

| Field | Value | Source | FR |
|-------|-------|--------|-----|
| `purl` | `pkg:npm/<name>@<version>` (or `pkg:npm/%40<scope>/<name>@<version>` for scoped) | `package.json#name` + `package.json#version` (or placeholder) | FR-001 |
| `name` | `package.json#name` verbatim | manifest | FR-001 |
| `version` | literal version string or `"0.0.0-unknown"` placeholder | manifest, with placeholder fallback | FR-001 |
| `source` | `Some("path+file://<absolute-package-json-dir>")` | filesystem walker | (existing convention) |
| `lifecycle_scope` | `None` | n/a (Runtime by default) | (out of scope) |
| `sbom_tier` | `Some("source")` | constant | FR-006 |
| `extra_annotations` | BTreeMap with `mikebom:component-role: "main-module"` | constant | FR-004 |
| `parent_purl` | `None` | constant (top-level) | FR-001a |
| `depends` | `Vec<String>` of direct-dep names from `dependencies`/`devDependencies`/`peerDependencies`/`optionalDependencies`, scope-filtered | manifest tables, post existing scope filter | FR-007 |
| `licenses` | `vec![]` (empty) | constant | FR-005 |
| `hashes` | `vec![]` (empty) | constant | (n/a — synthetic component) |

### DroppedDuplicate (private helper struct)

Same shape as cargo's milestone-064 helper (`mikebom-cli/src/scan_fs/package_db/cargo.rs`). Returned from `dedup_npm_main_modules_by_purl(&mut Vec<PackageDbEntry>)` for caller-side `tracing::warn!` emission.

```rust
struct DroppedDuplicate {
    purl: String,
    kept_path: String,
    dropped_path: String,
}
```

## Relationships

### Direct-dep edges from main-module to dep targets

Each npm main-module emits direct-dep edges into the existing relationship graph:

```text
Relationship {
    from: <npm-main-module-purl>,           // e.g. pkg:npm/foo@1.2.3
    to: <dep-target-purl>,                  // e.g. pkg:npm/express@4.18.2
    relationship_type: DependsOn,
    provenance: {
        source: "<absolute-package.json-path>",
        data_type: "npm-manifest-direct-dep",
    },
}
```

Existing edge-emission machinery in `scan_fs/mod.rs` translates these via `name_to_purl` resolution + dangling-target dropping (deps whose targets aren't in the scan are silently dropped per the existing convention — same as Go and cargo).

### DESCRIBES relationship (document → main-module)

Inherits the multi-DESCRIBES wiring from milestone 064 + #127. For 1 npm main-module: single DESCRIBES + length-1 `documentDescribes`. For N>1 (workspace): one DESCRIBES per main-module + length-N `documentDescribes`, sorted alphabetically by SPDXID.

### Workspace-link edges (FR-011)

When member A's `package.json` declares `"<member-B>": "*"` (or any range that resolves to the in-tree workspace member), the existing edge-emission emits a `DependsOn` edge from A's main-module to B's main-module via `name_to_purl` resolution. Both endpoints are real components.

## State transitions

None — main-module emission is read-only and deterministic.

## Validation rules

| Rule | Source | Failure mode |
|------|--------|--------------|
| `package.json` MUST contain `name` to emit a main-module | FR-001 | Skip emission silently. |
| `package.json` with `private: true` AND no `version` MUST be skipped | FR-001 + #104 | Skip emission silently. |
| `package.json` with `name` but no `version` (and not `private`-skipped) emits with `0.0.0-unknown` placeholder | FR-001 + Q1 | No error; deterministic. |
| Scoped names (`@scope/name`) MUST encode `@` as `%40` per PURL spec | FR-001 | Use existing `build_npm_purl` helper. |
| Same-PURL emissions MUST be deduplicated to one entry | FR-001 + spec Edge Cases | First-discovered (alphabetical walker order) wins; `tracing::warn!` lists dropped paths. |
| Workspace-only `package.json` (no `name`, OR `private: true` + no version) MUST NOT emit | FR-002 | Members emit per FR-001. |

## Reuses from milestones 053 + 064 + #127

- `SpdxPrimaryPackagePurpose::Application` (milestone 053): set on npm main-module SPDX 2.3 packages identically.
- CDX `metadata.component` C40-tag-driven selector (milestone 064 T003): generalizes — already works for npm.
- CDX `components[]` exclusion (milestone 064 T004): C40-tag-driven; already works.
- SPDX 2.3 multi-root `documentDescribes` + per-root `DESCRIBES` (#127): already C40-tag-driven.
- SPDX 3 multi-root `rootElement` + per-root `describes` Relationship (#127): already C40-tag-driven via `pick_root_iri`.
- Cargo's `dedup_main_modules_by_purl` pattern (milestone 064 T010): copy-adapt to npm with `DroppedDuplicate` struct (functionally identical).

## Does NOT introduce

- No new public Rust type
- No new crate dependency
- No new CLI flag
- No new SBOM annotation key (C40 already exists)
- No new SPDX `primaryPackagePurpose` enum value (`Application` already wired)
- No subprocess calls
- No version-inheritance / workspace-context map (npm has no equivalent to cargo's `version.workspace = true`)
