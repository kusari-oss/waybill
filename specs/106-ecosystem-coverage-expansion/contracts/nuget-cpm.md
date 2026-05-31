# Contract: NuGet Central Package Management (`Directory.Packages.props`) (US4, FR-007a)

**New module**: `mikebom-cli/src/scan_fs/package_db/nuget/directory_packages_props.rs`

## Trigger

A file named `Directory.Packages.props` anywhere in the scan tree, OR in any ancestor directory of a `.csproj`/`.vbproj`/`.fsproj` file (MSBuild's walk-up resolution).

## Parsing

`quick-xml` (same dep as `csproj.rs`). Schema:

```xml
<Project>
  <PropertyGroup>
    <ManagePackageVersionsCentrally>true</ManagePackageVersionsCentrally>
  </PropertyGroup>
  <ItemGroup>
    <PackageVersion Include="Newtonsoft.Json" Version="13.0.3" />
    <PackageVersion Include="Microsoft.SourceLink.GitHub" Version="8.0.0" />
  </ItemGroup>
</Project>
```

The parser walks all `<PackageVersion>` elements and builds an `Include` → `Version` lookup map.

## Walk-up resolution (FR-007a)

For each `.csproj` file at `<path-to-csproj>/<name>.csproj`:

1. Start at `<path-to-csproj>/`.
2. Walk UP the directory tree (parent, grandparent, ...) until either:
   - A `Directory.Packages.props` file is found → parse it, use its `<PackageVersion>` map.
   - The scan-root boundary is reached → no CPM in use; falls back to `.csproj` `Version=` resolution (or `unresolved` if absent).

This mirrors MSBuild's actual lookup behavior. Each `.csproj`'s closest ancestor `Directory.Packages.props` wins.

## Detection signal

The `<ManagePackageVersionsCentrally>true</ManagePackageVersionsCentrally>` property is treated as INFORMATIONAL only. Per Clarification Q2: **presence of a `Directory.Packages.props` file in the walk-up chain is sufficient signal that CPM is in use**. The flag-element check is omitted because real-world repos sometimes forget to declare it.

## Annotations emitted (per resolved component)

The `Directory.Packages.props` file does NOT emit components itself — it's a lookup table. Its path appears in the resolved component's `mikebom:source-files` annotation (e.g., `mikebom:source-files: "/path/to/MyApp.csproj,/path/to/Directory.Packages.props"`).

## Edge cases handled

- **`Directory.Packages.props.user` files**: skipped (typically `.gitignore`d, not part of the authoritative state). `tracing::info!` records the skip.
- **`Directory.Build.props` `<PackageVersion>` entries**: NOT parsed in this milestone (out of scope per spec). Some repos use this file for the same purpose; mikebom's resolver only walks for `Directory.Packages.props`. Real-world demand for `Directory.Build.props` support can drive a follow-up milestone.
- **Multiple `Directory.Packages.props` in the walk-up chain**: the CLOSEST one wins (MSBuild semantics — closer files override more-distant ones). mikebom does not merge or combine maps.
- **`<PackageVersion>` in conditional `<ItemGroup Condition="...">`**: extracted regardless of condition (best-effort; conditions recorded as informational annotation if needed in future).
