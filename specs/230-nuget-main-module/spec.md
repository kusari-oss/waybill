# Feature Specification: NuGet main-module component + root→direct dependency edges

**Feature Branch**: `230-nuget-main-module`
**Created**: 2026-08-07
**Status**: Draft
**Input**: User description: "NuGet reader must emit a main-module component per project file (.csproj/.vbproj/.fsproj) and root->direct dependency edges. Today it emits only package->package edges from packages.lock.json, so every direct dependency that is not transitively pulled in by another package is orphaned. Follow the m064 (cargo) / m216 (Gemfile) pattern. Populate edges from packages.lock.json entries typed Direct or CentralTransitive; fall back to <PackageReference> items when no lockfile is present (design tier). ProjectReference->ProjectReference edges between main-modules are out of scope."

## Background

Every major single-project-file ecosystem in waybill already emits a main-module component per project file and populates root→direct dependency edges from it: cargo (m064), npm (m066), pip (m068), gem (m069), maven (m070), Gemfile (m216). NuGet is the outlier — the reader emits **only** package→package edges built from each locked package's own `dependencies` map. Consequently, every direct dependency declared in a `.csproj` or `Directory.Packages.props` that is *not* pulled in transitively by some other package ends up with zero incoming edges and is orphaned from the dependency graph.

Verification against the audit fixture at `specs/audit-nuget-realworld/artifacts/restsharp.waybill.cdx.json` confirms 16/16 NuGet components have zero incoming edges (100% orphan rate). Downstream consumers walking the graph from a root skip these packages entirely, so direct dependencies — the ones users most expect to act on — silently get no remediation analysis.

This milestone closes the gap by adopting the same shape the six sibling ecosystems already ship.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Locked NuGet project: direct dependencies reachable from a project root (Priority: P1)

A developer runs `waybill sbom scan` against a .NET solution containing one or more `.csproj`/`.vbproj`/`.fsproj` project files that opt in to `RestorePackagesWithLockFile` (a `packages.lock.json` sits next to each project). Every direct dependency declared in the project — either inline `<PackageReference>` items or Central Package Management entries in `Directory.Packages.props` — appears as an incoming edge target from a project-level main-module component in the emitted SBOM. No previously-detected component is dropped or renamed.

**Why this priority**: This is the primary failure mode reporters observe today. The evidence set (RestSharp: 16/16 orphaned; reporter's eShop shape: OpenTelemetry.Exporter.OpenTelemetryProtocol orphaned) shows every locked-mode NuGet scan is affected. Fixing this restores parity with the six sibling ecosystems that already ship this behavior.

**Independent Test**: Scan a solution that has `packages.lock.json` files. Assert every component whose lockfile `entry_type` is `Direct` or `CentralTransitive` has at least one incoming dependency edge whose source is a main-module component. Assert the pre-milestone-230 component count is unchanged (no packages added or removed to the component list).

**Acceptance Scenarios**:

1. **Given** a `.csproj` with a `packages.lock.json` and a direct `<PackageReference Include="OpenTelemetry.Exporter.OpenTelemetryProtocol" Version="1.9.0" />`, **When** the reader runs, **Then** an OpenTelemetry.Exporter.OpenTelemetryProtocol component exists in the SBOM AND at least one incoming edge points at it from a main-module component whose source-file path resolves to the same `.csproj`.
2. **Given** a Central Package Management setup where `Directory.Packages.props` declares `<PackageVersion Include="Microsoft.OpenApi" Version="1.6.14" />` and a leaf `.csproj` declares versionless `<PackageReference Include="Microsoft.OpenApi" />`, **When** the reader runs, **Then** the resulting Microsoft.OpenApi component has an incoming edge from the leaf project's main-module component.
3. **Given** a package that is only transitively depended on (lockfile `entry_type: Transitive`), **When** the reader runs, **Then** that component does NOT receive an incoming edge from any main-module — only from its parent package (existing behavior, unchanged).
4. **Given** the pre-milestone-230 RestSharp audit scan showed 16 NuGet components, **When** the milestone-230 reader runs against the same fixture, **Then** the SBOM still contains exactly the same 16 NuGet package components (component detection must not regress) PLUS one or more new main-module components.

---

### User Story 2 - Unlocked NuGet project: direct dependencies reachable via design-tier edges (Priority: P2)

A developer scans a repository where projects declare `<PackageReference>` items but no `packages.lock.json` is present (the common case in older or opt-out repos). The reader emits main-module components and design-tier edges derived from the `<PackageReference>` declarations themselves, so consumers still see the root→direct topology — labeled as design-tier so operators can distinguish it from lockfile-backed evidence.

**Why this priority**: The lockfile-backed case (US1) captures the majority of well-maintained repos, but many real-world .NET repos never opt into `RestorePackagesWithLockFile`. Without US2, those repos remain orphaned exactly as they are today. Ship US1 first (higher-fidelity data path), then fold in US2 so unlocked repos also benefit.

**Independent Test**: Scan a project with `<PackageReference>` items but no `packages.lock.json`. Assert every declared `<PackageReference>` has an incoming edge from the project's main-module component AND the edge (or the main-module) carries a design-tier marker on the resolution/annotation channel used by consumers.

**Acceptance Scenarios**:

1. **Given** a `.csproj` with `<PackageReference Include="Newtonsoft.Json" Version="13.0.3" />` and no `packages.lock.json`, **When** the reader runs, **Then** a Newtonsoft.Json component exists AND it has an incoming edge from a main-module whose source-file path resolves to the `.csproj`.
2. **Given** the same project, **When** the reader runs, **Then** the resulting main-module component is tagged as design-tier (source of the edge is a manifest declaration, not a resolved lockfile entry), consistent with how the reader marks other design-only components today.
3. **Given** a solution with mixed locked and unlocked projects, **When** the reader runs, **Then** the locked projects follow US1 semantics and the unlocked projects follow US2 semantics; both classes of main-module coexist in the same SBOM without collision.

---

### Edge Cases

- **Multi-target framework (TFM) project**: A `.csproj` may declare `<TargetFrameworks>net6.0;net8.0</TargetFrameworks>`, and `packages.lock.json` groups dependencies per-framework. Per FR-009, one main-module per project; its dependency-edge set is the union of Direct + CentralTransitive across every TFM. Same-name packages appearing under multiple TFMs with different resolved versions produce multiple edge targets (one per resolved version).
- **Unversioned or SDK-style project without an explicit `<Version>` / `<VersionPrefix>` / `<AssemblyVersion>`**: The reader must produce a stable, non-empty main-module PURL. Per FR-010, the derivation ladder is `<Version>` → `<VersionPrefix>`(+`<VersionSuffix>`) → `<AssemblyVersion>` → fallback to `pkg:generic/<project-filename-stem>@0.0.0`.
- **Project with a `<AssemblyName>` override**: The main-module PURL name segment should reflect the assembly the project actually produces, not the filename stem, because assembly identity is what downstream consumers reason about.
- **Project with no `<PackageReference>` items and no `packages.lock.json`**: Still emit a main-module component (represents the project itself) but with an empty depends list — matches the m064/m216 pattern for empty manifests.
- **CPM (Central Package Management) versionless `<PackageReference>` in the leaf project**: The version comes from `Directory.Packages.props`. The main-module→direct edge target must be the resolved (versioned) PURL, not a versionless one.
- **Duplicate main-modules across `.csproj` files with the same `<AssemblyName>`**: Two projects in a solution can declare the same output assembly (rare, but possible in sample/test setups). Emit both; downstream dedup by PURL if identical is the existing m064 pattern.
- **`ProjectReference` items (project-to-project edges)**: Explicitly out of scope for this milestone. The lockfile marks these `entry_type: "Project"`; the reader already reads and drops them at `nuget/mod.rs:321`. Leaving them out means main-modules within a multi-project solution won't yet form edges to each other, but every main-module still reaches its NuGet-declared direct dependencies — the primary user goal.
- **Malformed `packages.lock.json`**: Preserve existing behavior (silently continue with the packages the reader can parse); do not fabricate main-module edges from partial data.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The NuGet reader MUST emit one main-module component for every project file (`.csproj`, `.vbproj`, `.fsproj`) discovered under the scan root.
- **FR-002**: Each main-module component MUST carry the annotation `waybill:component-role: "main-module"` and MUST be marked as source-tier, matching the shape emitted by the cargo (m064) and Gemfile (m216) readers.
- **FR-003**: The main-module PURL MUST take the form `pkg:nuget/<AssemblyName>@<Version>` when a version is derivable, and `pkg:generic/<project-filename-stem>@0.0.0` when it is not, matching the reporter's proposed shape.
- **FR-004**: When a `packages.lock.json` is present for a project, the reader MUST populate the main-module's dependency edges from every lockfile entry whose `entry_type` is `Direct` or `CentralTransitive`. Entries typed `Transitive` MUST NOT be attached to the main-module.
- **FR-005**: When no `packages.lock.json` is present, the reader MUST derive the main-module's dependency edges from `<PackageReference Include=...>` items declared in the project file (design-tier fallback).
- **FR-006**: The reader MUST NOT alter the set of package-level components it produces today. Adding milestone 230 must add main-modules and edges only; it must not add, drop, rename, or version-shift any existing NuGet package component. Verified via byte-equivalence of the component list (excluding new main-modules) against pre-230 goldens for RestSharp, Serilog, and Orleans in `specs/audit-nuget-realworld/artifacts/`.
- **FR-007**: The reader MUST NOT emit main-module→main-module edges from `ProjectReference` items in this milestone (out of scope; deferred to a follow-up).
- **FR-008**: Entries typed `Project` in `packages.lock.json` MUST continue to be skipped as they are today at `nuget/mod.rs:321` — they represent inter-project references handled by FR-007's out-of-scope note, not NuGet package dependencies.
- **FR-009**: For multi-TFM projects, the reader MUST emit exactly one main-module per project file. The main-module's dependency-edge set MUST be the UNION of `Direct` and `CentralTransitive` entries across every target framework listed in `packages.lock.json` (or, in the design-tier fallback, across every `<PackageReference>` declared in the project regardless of any `Condition=` guarding by TFM). The same package appearing under multiple TFMs with different versions produces multiple edge targets, one per resolved version.
- **FR-010**: The main-module version MUST be derived via the following deterministic ladder, taking the first source that resolves to a non-empty string: (1) the project file's `<Version>` element, (2) `<VersionPrefix>` concatenated with `<VersionSuffix>` when set (dash-separated per SemVer convention), (3) `<AssemblyVersion>`. When no source resolves — including when a source contains an unresolvable MSBuild variable (e.g., `$(VersionPrefix)` with no definition in scope) — the reader MUST fall back to the `pkg:generic/<project-filename-stem>@0.0.0` PURL shape from FR-003.

### Key Entities

- **Main-module component**: A single component per project file, representing the project itself (the "thing being built"). Carries the source-tier marker and the `waybill:component-role: "main-module"` annotation. Its PURL identifies the assembly the project produces.
- **Root→direct edge**: A dependency edge whose source is a main-module component and whose target is a first-level NuGet package the project declares (via `<PackageReference>` or CPM). Distinct from the existing package→package edges built by `build_lock_edges` at `nuget/mod.rs:453`, which continue to represent transitive relationships within the resolved dependency graph.
- **Package-level component**: An existing per-package NuGet component (source: `packages.lock.json` or `<PackageReference>` resolution). Unchanged in identity and count by this milestone.
- **Lockfile entry classification**: The `entry_type` field on each `packages.lock.json` entry — `Direct`, `Transitive`, `CentralTransitive`, or `Project`. This classification is the source of truth for FR-004 (which entries feed the main-module edge list) and FR-008 (which entries the reader continues to skip).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On a scan of a solution with at least one `packages.lock.json`, the SBOM contains ≥1 main-module component per project file (`.csproj`/`.vbproj`/`.fsproj`), and zero NuGet components whose lockfile `entry_type` is `Direct` or `CentralTransitive` have zero incoming dependency edges. Measured by parsing the emitted SBOM and cross-referencing against the lockfile.
- **SC-002**: The pre-230 audit fixture at `specs/audit-nuget-realworld/artifacts/restsharp.waybill.cdx.json` currently reports 0/16 NuGet components with incoming edges. Post-230, every RestSharp NuGet component that is either declared in a `.csproj` `<PackageReference>` or listed as lockfile `Direct`/`CentralTransitive` has at least one incoming edge from a main-module component. Measured by re-running the audit harness and comparing edge-target-coverage percentages.
- **SC-003**: The NuGet component list emitted by a milestone-230 scan is a strict superset of the pre-230 component list for the same input (new main-modules added; every previously-emitted NuGet package still present with the same PURL). Measured by set-diff on component PURLs against the pre-230 audit-fixture goldens (RestSharp, Serilog, Orleans).
- **SC-004**: The `waybill:graph-completeness` annotation on a solution scan with fully-locked NuGet content no longer reports `multi-ecosystem-partial-root: nuget` as a reason. Measured by inspecting the annotation on a scan of the RestSharp fixture (which currently reports the code) — post-230 the code should be absent, because milestone 230 causes the graph-completeness classifier to seed a NuGet per-ecosystem root from the emitted main-modules (see `bfs.rs:87 build_ecosystem_root_set`).
- **SC-005**: An unlocked scan (no `packages.lock.json` present) of a `.csproj` with `<PackageReference>` items produces the same root→direct edge topology as the locked scan of an equivalent project, distinguished only by a design-tier marker on the main-module/edge. Measured by scanning a hand-crafted fixture pair (one locked, one unlocked with matching declarations) and asserting edge-set equality up to the tier marker.

## Assumptions

- The reader continues to run inside the existing `scan_fs::package_db::nuget` module hierarchy; no new package DB reader crate is introduced. This milestone extends the existing NuGet reader in place, following the m064/m216 pattern of adding a `build_*_main_module_entry` helper alongside `build_lock_edges` at `waybill-cli/src/scan_fs/package_db/nuget/mod.rs:453`.
- Existing `entry_type` parsing at `waybill-cli/src/scan_fs/package_db/nuget/mod.rs:321-334` already produces the `Direct` / `Transitive` / `CentralTransitive` / `Project` classification needed for FR-004 and FR-008. This milestone re-reads that classification; it does not modify the parser.
- ProjectReference edges between main-modules are deferred to a follow-up milestone (FR-007). Users who need cross-project topology in the same solution can pick it up when that follow-up lands; this milestone only closes the direct-dependency orphan gap.
- The reporter's secondary observation about `waybill:graph-completeness` reporting "complete" despite orphan-heavy graphs is deferred to a separate investigation. Verification against the RestSharp fixture shows the classifier correctly reports "partial" with reason `multi-ecosystem-partial-root: nuget`; the reporter's eShop observation requires their raw scan output to reproduce. Milestone 230 will make that observation moot for the specific reason code (see SC-004), but any residual completeness bug is out of scope here.
- The scope of "solution" for testing purposes is a directory tree containing one or more project files, matching how the existing NuGet audit fixtures at `specs/audit-nuget-realworld/artifacts/` are structured. No solution-file (`.sln`) parsing is required by this milestone (project-file walk already surfaces the entire project set).
- Milestone 230 does NOT touch the reader's MSBuild-property resolution (m655 area), `Directory.Build.props` walking (m655 area), or CPM property/variable substitution. Any project where the version-derivation ladder (FR-010) hits an unresolvable MSBuild variable (e.g., `<Version>$(SomeVar)</Version>` with `$(SomeVar)` undefined) falls through to the `pkg:generic/*@0.0.0` PURL shape from FR-003, matching the existing reader's design-tier fallback for unresolved package versions.
