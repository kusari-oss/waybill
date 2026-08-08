# Phase 1 Data Model: NuGet main-module + root→direct edges

**Feature**: 230-nuget-main-module
**Date**: 2026-08-07

Everything below reuses existing types. No new struct is introduced by this milestone.

## Entities

### NuGetMainModule (conceptual; concrete type = `PackageDbEntry`)

Represents one `.csproj` / `.vbproj` / `.fsproj` project file discovered by the walker.

| Field | Type | Value for this milestone |
|-------|------|--------------------------|
| `purl` | `waybill_common::types::purl::Purl` | `pkg:nuget/<AssemblyName>@<version>` when version resolves; `pkg:generic/<project-stem>@0.0.0` on fallback. |
| `name` | `String` | Resolved `<AssemblyName>` or project filename stem (R3). Case-preserved. |
| `version` | `String` | Result of the FR-010 ladder or `"0.0.0"` on fallback. |
| `arch` | `Option<String>` | `None`. |
| `source_path` | `String` | `path+file://<project-file-path>` (matches cargo's `path+file://` convention at cargo.rs:556). |
| `depends` | `Vec<String>` | For US1 (locked): every distinct name across the lockfile framework blocks whose `entry_type` is `Direct` or `CentralTransitive`. For US2 (unlocked): every `<PackageReference Include="...">` value in the project file. Names are case-preserved as written; `scan_fs/mod.rs`'s edge emitter normalizes at lookup time via the existing `normalize_dep_name("nuget", name)` path. |
| `sbom_tier` | `Option<String>` | `Some("source")` for US1; `Some("source")` for US2 as well (matches how cargo m064's main-module is tagged regardless of manifest resolution state — the tier speaks to "this component represents a source-tree entity," not to lockfile-backing). Design-tier signaling on the *edges* to unresolved packages is carried by those packages' own entries via the existing `#653: unresolved (None) → design-tier` path at `nuget/mod.rs:369-382`. |
| `parent_purl` | `Option<Purl>` | `None`. Top-level (matches m064). |
| `extra_annotations["waybill:component-role"]` | JSON `String` | `"main-module"`. Registered in parity catalog row C40. |
| `extra_annotations["waybill:source-files"]` | JSON `String` (comma-separated) | Present when the main-module was assembled from multiple sources (project file + at least one ancestor `Directory.Packages.props`). Preserves the existing NuGet reader's multi-source convention at `nuget/mod.rs:355-367`. |
| All other `PackageDbEntry` fields | (various) | Left at struct defaults — matches cargo m064's builder shape at cargo.rs:557-587. |

### Existing lockfile entry classification (source of truth for FR-004; unchanged)

The `entry_type` field on each `packages.lock.json` entry — one of `Direct` / `Transitive` / `CentralTransitive` / `Project` — is parsed by `packages_lock.rs` and already consumed by `nuget/mod.rs:321-334` for the Project-skip and Transitive-tagging paths. This milestone adds a new consumer of the classification:

| entry_type | Existing behavior (pre-230) | New behavior (post-230) |
|------------|-----------------------------|-------------------------|
| `Direct` | Emitted as package-level component. | ALSO added to the containing project's main-module `depends` list. |
| `CentralTransitive` | Emitted as package-level component. | ALSO added to the containing project's main-module `depends` list (CPM projects reference these versionlessly, so they're direct references in practice). |
| `Transitive` | Emitted as package-level component; tagged as transitive. | Unchanged. NOT added to main-module `depends`. |
| `Project` | Skipped at `nuget/mod.rs:321`. | Unchanged. FR-007 out-of-scope. |

### Multi-TFM handling (FR-009 disposition)

Per Clarifications, one main-module per project. For a `packages.lock.json` with multiple framework blocks (`dependencies: { "net6.0": {...}, "net8.0": {...} }`):
1. Iterate every framework block.
2. For each entry with `entry_type` ∈ {`Direct`, `CentralTransitive`}, add the entry name to the main-module's `depends` list.
3. Deduplicate names (a package under both `net6.0` and `net8.0` produces one edge; the two resolved versions each become their own package-level component per existing behavior, and both become edge targets when `scan_fs/mod.rs` resolves the shared name via the `<name> <version>` disambiguation key at `scan_fs/mod.rs:606-609`).

### Version-derivation ladder (FR-010)

State machine over the project file's parsed elements:

```
                        parse project file
                                │
                                ▼
              read <Version> element (property-substituted)
                                │
                        non-empty & resolved?
                                │
                       ┌────yes─┴────no────┐
                       ▼                    ▼
              use <Version>       read <VersionPrefix> + <VersionSuffix>
                       │             (concatenate with "-" if suffix set,
                       │             property-substitute both)
                       │                    │
                       │             non-empty & resolved?
                       │                    │
                       │           ┌───yes──┴──no─┐
                       │           ▼               ▼
                       │  use "prefix" or        read <AssemblyVersion>
                       │  "prefix-suffix"        (property-substituted)
                       │           │                        │
                       │           │             non-empty & resolved?
                       │           │                        │
                       │           │                ┌───yes─┴──no─┐
                       │           │                ▼             ▼
                       │           │       use AssemblyVersion   fallback:
                       │           │                │           pkg:generic/
                       │           │                │           <stem>@0.0.0
                       ▼           ▼                ▼             ▼
                             emit pkg:nuget/<AssemblyName>@<version>
                                     (or pkg:generic fallback)
```

## Validation rules

- Main-module PURLs MUST validate via `Purl::new()` (Constitution Principle IV). The `<AssemblyName>` value (or filename stem) is PURL-segment-encoded via `waybill_common::types::purl::encode_purl_segment` — the existing NuGet PURL construction at `mod.rs:441-450` already uses this helper.
- The lockfile-name-vs-PackageReference-name space is case-preserved but treated as case-insensitive for edge resolution. `scan_fs/mod.rs`'s `normalize_dep_name` already handles this for NuGet in the same way it handles the existing package→package edges — the main-module edges inherit that behavior.
- No mutation of pre-230 package-level components (FR-006). The reader's existing `out.push(PackageDbEntry { ... })` at `nuget/mod.rs:391` is unchanged. Main-module entries are pushed additionally via a new `out.push(...)` call after the existing package-entry loop.

## Out-of-scope

- ProjectReference→ProjectReference edges between main-modules (FR-007). Deferred to a follow-up milestone.
- Graph-completeness algorithm modifications. Verified against RestSharp fixture: the classifier at `graph_completeness/bfs.rs:87 build_ecosystem_root_set` will naturally pick up NuGet main-modules and stop firing `multi-ecosystem-partial-root: nuget` (SC-004), but no code change to the classifier is needed.
- `.sln` solution-file parsing. Not required — the project-file walker surfaces every project without needing solution-level metadata.
