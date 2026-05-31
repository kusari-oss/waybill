# Phase 1 Data Model: Ecosystem Coverage Expansion (Phase 1)

This document captures the in-process data structures introduced by milestone 106.
Everything is in-memory per scan; no persistent storage; mirrors every
filesystem-scan milestone since 002.

## Domain entities (new)

### `UvLockfile`

Parsed shape of a uv.lock file (TOML).

```rust
pub struct UvLockfile {
    pub packages: Vec<UvPackage>,
    pub source_path: PathBuf,
}

pub struct UvPackage {
    pub name: String,
    pub version: String,
    pub source: UvSource,
    pub dependencies: Vec<UvDepRef>, // resolved [[package.dependencies]] entries
}

pub enum UvSource {
    Registry { /* PyPI by default */ },
    Workspace,                                  // workspace-source member
    Git { url: String, revision: Option<String> },
    Path { path: PathBuf },                     // local-path source
}

pub struct UvDepRef {
    pub name: String,
    pub version: Option<String>,                // may be None for workspace-source
}
```

PURL derivation (FR-001):

- `UvSource::Registry` → `pkg:pypi/<name>@<version>`
- `UvSource::Workspace` → `pkg:pypi/<name>@<version>` + `mikebom:component-role: "main-module"`
- `UvSource::Git` → `pkg:git+https://<url>@<rev>`
- `UvSource::Path` → `pkg:generic/<name>` + `mikebom:source-type: "local"`

### `BunLockfile`

Parsed shape of a `bun.lock` file (JSONC).

```rust
pub struct BunLockfile {
    pub lockfile_version: u32,
    pub workspaces: BTreeMap<String, BunWorkspace>, // key="" = root, others = member paths
    pub packages: BTreeMap<String, BunPackage>,     // key format: "name@source"
    pub source_path: PathBuf,
}

pub struct BunWorkspace {
    pub name: Option<String>,                  // declared `name:` in member's package.json
    pub dependencies: BTreeMap<String, String>, // dep name → version-spec (incl "workspace:*")
}

pub struct BunPackage {
    pub key: String,                            // raw key like "lodash@4.17.21" or "@my/web@workspace:packages/web"
    pub resolved_name: String,
    pub resolved_version: String,
    pub source: BunSource,                      // Registry / Workspace / Git / Tarball
}
```

Parsing: file bytes pass through `npm::jsonc::strip_comments` first, then `serde_json::from_str::<Value>` (we use untyped `Value` because the schema is flexible — a `BunLockfile` is the typed view after we walk the JSON).

### `GradleLockEntry`

A single parsed line from `gradle.lockfile` or `buildscript-gradle.lockfile`.

```rust
pub struct GradleLockEntry {
    pub group: String,
    pub name: String,
    pub version: String,
    pub configurations: Vec<String>,           // e.g. ["compileClasspath", "runtimeClasspath"]
    pub source_path: PathBuf,
    pub is_buildscript: bool,                  // true → lifecycle_scope = Build
}
```

PURL derivation (FR-005): `pkg:maven/<group>/<name>@<version>`.

Lifecycle scope (FR-006): `is_buildscript == true` → `LifecycleScope::Build`. Else `None` (runtime default).

### `NugetPackageReference`

A single `<PackageReference>` element extracted from a project file.

```rust
pub struct NugetPackageReference {
    pub include: String,                       // package name from Include="..."
    pub version_explicit: Option<String>,      // Version="..." attribute, if present
    pub private_assets: Option<String>,        // PrivateAssets="..." attribute
    pub include_assets: Option<String>,        // IncludeAssets="..." attribute
    pub exclude_assets: Option<String>,        // ExcludeAssets="..." attribute
    pub condition: Option<String>,             // Condition="..." attribute (informational)
    pub source_file: PathBuf,                  // path to the .csproj/.vbproj/.fsproj
}
```

### `NugetCentralPackagesProps`

Parsed `Directory.Packages.props` (NuGet Central Package Management).

```rust
pub struct NugetCentralPackagesProps {
    pub package_versions: BTreeMap<String, String>, // name → version
    pub source_path: PathBuf,
}
```

The `.csproj` resolver walks UP from each project file to find the nearest `Directory.Packages.props` (per MSBuild's actual lookup behavior), then uses this map to resolve any `<PackageReference>` without an explicit `Version=` attribute (per FR-007a).

### `NugetPackagesLockfile`

Parsed `packages.lock.json` — NuGet's reproducible-restore lockfile.

```rust
pub struct NugetPackagesLockfile {
    pub version: u32,                          // lockfile version
    pub targets: BTreeMap<String, BTreeMap<String, NugetLockedDep>>,
    // outer key: target-framework (e.g. "net8.0"), inner key: package name
    pub source_path: PathBuf,
}

pub struct NugetLockedDep {
    pub kind: String,                          // "Direct" | "Transitive" | "Project"
    pub resolved: String,                      // exact resolved version
    pub dependencies: BTreeMap<String, String>, // transitive deps + their version specs
}
```

When `packages.lock.json` is present alongside a `.csproj`, it is the authoritative version source (FR-008). The `.csproj` provides only the direct-dependency set; the lockfile fills in transitives.

## Workspace emission model (new)

Per Clarification Q1, workspace projects emit:

1. **A synthetic workspace-root component**:
   - PURL: `pkg:generic/<workspace-name>` (where `workspace-name` derives from the root manifest's `name` field, or `"workspace-root"` placeholder when absent)
   - Annotation: `mikebom:component-role: "workspace-root"` (new enum value for the existing C40 annotation; doc-only update per research R3)
   - Annotation: `mikebom:source-files: "<path-to-root-manifest>"` (the root `pyproject.toml` or `package.json`)

2. **One component per workspace member**:
   - PURL: ecosystem-native (`pkg:pypi/<name>@<version>` for uv, `pkg:npm/<name>@<version>` for Bun)
   - Annotation: `mikebom:component-role: "main-module"` (existing enum value)
   - Annotation: `mikebom:source-files: "<path-to-member-pyproject-or-package-json>"`

3. **`dependsOn` edges**:
   - From workspace-root to each member (workspace membership)
   - Between members where declared (uv `[[package.dependencies]]` with workspace-source, Bun `"workspace:*"` source in member's package.json)
   - Independent members get NO edges between them

4. **External deps** (PyPI, npm) appear as regular transitive components with `dependsOn` edges from the appropriate member.

## Annotation entities (no new annotations introduced)

### `mikebom:component-role` (C40, enum extended)

Existing alpha.41+ open-enum values (per research R3):

- `"build-tool"` — Maven/Gradle/sbt-style installations of build tools at known prefixes
- `"language-runtime"` — JDK/Node/Python runtime artifacts at known prefixes
- `"main-module"` — workspace root of a Cargo / npm / pip / gem / golang / maven project

**Added in milestone 106**:

- `"workspace-root"` — synthetic component representing a uv / Bun workspace (or future cargo workspace root) above its members

Total: 4 closed-enum values after milestone 106. Documented in `docs/reference/sbom-format-mapping.md` C40 row.

### `mikebom:lifecycle-scope` (existing, unchanged usage)

Existing values from milestone 052:

- `"runtime"` (or absent — default)
- `"development"`
- `"build"`
- `"test"`

**Milestone 106 emits `"build"`** on:

- Gradle `buildscript-gradle.lockfile` components (FR-006)
- NuGet `<PackageReference>` with `PrivateAssets="All"` or equivalent `IncludeAssets`/`ExcludeAssets` patterns (FR-007b)

The CDX-side `scope: "excluded"` mapping and SPDX-side `BUILD_DEPENDENCY_OF` relationship emission are handled by the existing milestone-052 infrastructure (`generate/cyclonedx/builder.rs:590-605` + `generate/spdx/relationships.rs:79-91`). No new code in this milestone touches those paths.

### `mikebom:source-files` (existing)

Per-component annotation listing the manifest file(s) that contributed the entry. When multiple files resolve the same canonical PURL (e.g., `.csproj` + `packages.lock.json` + `Directory.Packages.props` all touch the same Newtonsoft.Json entry), the annotation contains a comma-joined list of all source files.

### `mikebom:source-type` (existing, extended values where applicable)

For uv `Path` source members, the annotation value `"local"` matches the existing milestone-053 convention for `pkg:generic/...` local-path components.

## Validation rules summary

| FR | Validation enforced where |
|---|---|
| FR-001 (uv.lock packages) | `pip::uv_lock::parse_lockfile` |
| FR-002 (uv.lock dependsOn edges) | `pip::uv_lock::emit` (constructs `PackageDbEntry.depends`) |
| FR-003 (bun.lock JSONC parsing) | `npm::bun_lock::parse_lockfile` (uses `npm::jsonc::strip_comments` first) |
| FR-004 (bun.lock dependency edges) | `npm::bun_lock::emit` |
| FR-005 (Gradle lockfile entries) | `gradle::lockfile::parse_entries` |
| FR-006 (Gradle lifecycle-scope) | `gradle::lockfile::emit` sets `lifecycle_scope: Some(Build)` when `is_buildscript == true` |
| FR-007 (NuGet `<PackageReference>` extraction) | `nuget::csproj::parse_project_file` |
| FR-007a (CPM Directory.Packages.props lookup) | `nuget::csproj::resolve_version` walks up to find `Directory.Packages.props`, then queries `NugetCentralPackagesProps.package_versions` |
| FR-007b (PrivateAssets → build scope) | `nuget::private_assets::classify` returns `LifecycleScope::Build` for `PrivateAssets="All"` etc. |
| FR-008 (packages.lock.json authoritative) | `nuget::packages_lock::resolve_transitives` is called by `nuget::mod::read` BEFORE `nuget::csproj::emit`; lockfile-resolved versions override `.csproj` `Version=` attributes when present |
| FR-009 (multi-source mikebom:source-files annotation) | **Within-ecosystem merge** (e.g., NuGet's `.csproj` + `Directory.Packages.props` + `packages.lock.json` all touching the same PURL): owned by each per-ecosystem reader. `nuget::mod::read` (T049) is the canonical case — collects all contributing source files into a `BTreeSet<PathBuf>` and emits a comma-joined `mikebom:source-files` annotation on the resulting `PackageDbEntry`. **Cross-ecosystem merge** (e.g., a Python project that has both `uv.lock` AND `poetry.lock` declaring the same package): owned by the milestone-105 dedup pipeline, which records the losing reader's source-mechanism in `mikebom:also-detected-via` and the winner's `mikebom:source-files` on the surviving component. The two mechanisms are distinct — within-ecosystem multi-file source contributions don't go through the dedup pipeline because they share the same source-mechanism. |
| FR-010 (warn-and-continue) | every reader returns `Vec<PackageDbEntry>` and uses `tracing::warn!` per-file; never returns `Err` from the top-level dispatch path |
| FR-011 (dispatcher integration) | `scan_fs/package_db/mod.rs::read_all` adds 4 new `<reader>::read(...)` calls |
| FR-012 (offline) | no network calls in any reader (verified by a build-time grep test, similar to milestone-105 T100a) |
| FR-013 (docs/ecosystems.md update) | manual update step in the final-phase tasks |
| FR-014 (`--exclude-scope` integration) | existing CLI flag handles via `LifecycleScope` — no new code |
| FR-015 (workspace-root + members emission) | `pip::uv_lock::emit_workspace` + `npm::bun_lock::emit_workspace` per-reader; helpers extracted to `scan_fs/package_db/workspace.rs` if duplication emerges |
