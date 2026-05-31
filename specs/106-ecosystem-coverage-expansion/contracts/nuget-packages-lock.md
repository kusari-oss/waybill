# Contract: NuGet `packages.lock.json` reader (US4, FR-008)

**New module**: `mikebom-cli/src/scan_fs/package_db/nuget/packages_lock.rs`

## Trigger

A file named `packages.lock.json` adjacent to a `.csproj` (or `.vbproj` / `.fsproj`) file.

## Parsing

Standard `serde_json::from_str::<NugetPackagesLockfile>` per `data-model.md` schema:

```json
{
  "version": 1,
  "dependencies": {
    "net8.0": {
      "Newtonsoft.Json": {
        "type": "Direct",
        "requested": "[13.0.3, )",
        "resolved": "13.0.3",
        "contentHash": "...",
        "dependencies": {}
      },
      "System.Buffers": {
        "type": "Transitive",
        "resolved": "4.5.1",
        "contentHash": "..."
      }
    }
  }
}
```

## Authoritative version resolution (FR-008)

When `packages.lock.json` is present alongside a `.csproj`:

1. Parse the lockfile into the per-target-framework dependency map.
2. For each `.csproj` `<PackageReference Include="X"/>`, look up `dependencies.<framework>.X.resolved` in the lockfile. If found, use that as the version — overrides the `.csproj` `Version=` attribute (which is typically a range like `[13.0.3, )` and the lockfile's `resolved` is the pinned version).
3. Also emit transitive deps from the lockfile that are NOT present in any `.csproj` — these are NuGet's resolved transitives that wouldn't appear from `.csproj` alone.

## PURL derivation

```
pkg:nuget/<package-name>@<resolved-version>
```

Multi-target-framework projects (e.g., `net6.0` + `net8.0`) emit one component per unique `(name, version)` tuple. The dedup pipeline (existing) collapses duplicates by canonical PURL with `mikebom:source-files` listing all source files.

## Dependency edges

Each entry's `dependencies` map gives the direct deps of that package within the resolved tree. mikebom emits `dependsOn` edges from each package to its direct deps from the lockfile.

For the `.csproj`'s direct deps:
- The "root" package of each `.csproj` is the project itself — implicitly the SBOM root (or a workspace member if part of a solution).
- The project's `<PackageReference Include="X"/>` entries become direct edges from the project to each X.

## Annotations emitted (per component)

| Annotation | Value |
|---|---|
| `mikebom:source-files` | path of `packages.lock.json` (+ `.csproj` if also directly referenced) |
| `mikebom:source-type` | `"transitive"` for entries with `"type": "Transitive"`; absent for `"Direct"` |

## Edge cases handled

- **Multi-target-framework with conflicting versions**: when `net6.0.X.resolved` ≠ `net8.0.X.resolved`, BOTH versions emit as separate components with `mikebom:target-arch` annotations naming the framework. Matches the existing multi-arch dedup pattern.
- **Project references** (`"type": "Project"`): these are intra-solution references to another `.csproj`. Emit as `pkg:generic/<name>` + `mikebom:source-type: "project-ref"`. Future milestone can promote these to workspace-member style.
- **ContentHash field**: NOT used today; mikebom doesn't verify NuGet content hashes (would require a separate enrichment pass).

## Test fixtures

- `tests/fixtures/golden_inputs/nuget/packages_lock_present/` — `.csproj` + `packages.lock.json` covering the standard case
- `tests/fixtures/golden_inputs/nuget/multi_target_framework/` — same lockfile drives `net6.0` + `net8.0` framework targets
