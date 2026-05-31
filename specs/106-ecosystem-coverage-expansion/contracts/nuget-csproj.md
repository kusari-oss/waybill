# Contract: NuGet `.csproj` / `.vbproj` / `.fsproj` reader (US4, FR-007, FR-007b)

**New module**: `mikebom-cli/src/scan_fs/package_db/nuget/csproj.rs`

## Trigger

Any file matching `**/*.csproj`, `**/*.vbproj`, or `**/*.fsproj` anywhere in the scan tree.

## Parsing

`quick-xml` (existing workspace dep; used by `maven.rs`). The .NET project XML schema is well-defined:

```xml
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <ManagePackageVersionsCentrally>true</ManagePackageVersionsCentrally>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="Newtonsoft.Json" Version="13.0.3" />
    <PackageReference Include="Microsoft.SourceLink.GitHub" Version="8.0.0" PrivateAssets="All" />
    <PackageReference Include="MyAnalyzer" Version="1.0.0">
      <IncludeAssets>build,buildMultitargeting</IncludeAssets>
    </PackageReference>
  </ItemGroup>
</Project>
```

The parser walks all `<PackageReference>` elements (including nested under `<ItemGroup>` with `Condition="..."` attributes) and collects them into `NugetPackageReference` records.

## Version resolution (FR-007 + FR-007a)

Two-step:

1. **Explicit `Version=` attribute wins**: if the `<PackageReference>` element has `Version="..."`, use that.
2. **CPM fallback**: if `Version=` is absent, walk UP from the .csproj's directory looking for `Directory.Packages.props`. Use the `<PackageVersion Include="X" Version="..."/>` entry that matches `Include=`. (See `contracts/nuget-cpm.md` for the props file parsing.)
3. **Unresolved**: if neither resolves, emit `pkg:nuget/<name>@unresolved` + `tracing::warn!` naming the .csproj + the missing package.

## PURL derivation

```
pkg:nuget/<Include-value>@<resolved-version>
```

Names are case-preserved from the source file (NuGet is case-insensitive on the registry side but mikebom records what the source says — dedup pipeline handles cross-source merging).

## Lifecycle-scope (FR-007b — via `nuget::private_assets::classify`)

| Source attribute | LifecycleScope |
|---|---|
| `PrivateAssets="All"` | `Some(Build)` |
| `IncludeAssets="..."` where the comma-list omits `runtime` | `Some(Build)` |
| `ExcludeAssets="runtime"` (or `="..."` containing `runtime`) | `Some(Build)` |
| `PrivateAssets="None"` | `None` (runtime default) |
| (attributes absent) | `None` (runtime default) |

The existing milestone-052 mapping (`generate/cyclonedx/builder.rs:590-605`) handles the rest — `Some(Build)` emits CDX `scope: "excluded"` and SPDX `BUILD_DEPENDENCY_OF`.

Attribute parsing is case-insensitive (MSBuild treats attribute values case-insensitively); value matching uses lowercase tokens.

## Annotations emitted (per component)

| Annotation | Value |
|---|---|
| `mikebom:source-files` | path(s) of the `.csproj`/`.vbproj`/`.fsproj` (+ `Directory.Packages.props` if it contributed the version) |
| `mikebom:lifecycle-scope` | `"build"` for PrivateAssets-tagged entries; absent for runtime |
| `mikebom:lifecycle-scope-guard` | Best-effort: the literal `Condition="..."` value when the element appears inside a conditional `<ItemGroup>` (informational; mikebom does NOT evaluate the expression) |

## Test fixtures

- `tests/fixtures/golden_inputs/nuget/csproj_legacy/` — `<PackageReference>` with `Version=` (no CPM)
- `tests/fixtures/golden_inputs/nuget/csproj_cpm/` — `<PackageReference>` without `Version=` + `Directory.Packages.props` at root
- `tests/fixtures/golden_inputs/nuget/private_assets_all/` — three PrivateAssets variants exercising all build-only paths
- `tests/fixtures/golden_inputs/nuget/multi_target_framework/` — conditional `<ItemGroup Condition="...">` blocks
