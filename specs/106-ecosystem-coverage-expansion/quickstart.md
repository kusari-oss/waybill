# Quickstart: Ecosystem Coverage Expansion (milestone 106)

Once milestone 106 ships, mikebom can scan four previously-blind project shapes. This page shows the minimum-viable invocation for each.

## uv-using Python projects

```bash
mikebom sbom scan --path ./my-uv-project --format cyclonedx-json --output sbom.cdx.json
```

Expected output: one `pkg:pypi/...` component per package resolved in `uv.lock`. Dependency edges from `[[package.dependencies]]` arrays populate the SBOM's relationship graph.

## uv workspace projects (monorepos)

For a Cargo-style uv workspace with multiple members:

```
my-monorepo/
├── pyproject.toml        # [tool.uv.workspace] members = ["apps/web", "libs/shared"]
├── uv.lock
├── apps/web/pyproject.toml
└── libs/shared/pyproject.toml
```

```bash
mikebom sbom scan --path ./my-monorepo --format cyclonedx-json --output sbom.cdx.json
```

Output includes:

- A synthetic `pkg:generic/my-monorepo` workspace-root component with `mikebom:component-role: "workspace-root"`.
- One `pkg:pypi/<name>@<version>` workspace-member per `[tool.uv.workspace].members` entry, tagged `mikebom:component-role: "main-module"`.
- `dependsOn` edges from workspace-root to each member, plus any intra-workspace edges declared in the lockfile (e.g., if `apps/web` lists `libs/shared` as a workspace-source dep).
- External PyPI deps as transitives with edges from the appropriate member.

## Bun JS projects

```bash
mikebom sbom scan --path ./my-bun-app --format cyclonedx-json --output sbom.cdx.json
```

mikebom parses `bun.lock` (JSONC format — the `// bun: lockfileVersion: 1` marker comment is auto-stripped) and emits `pkg:npm/<name>@<version>` per resolved package. Scoped packages encode as `pkg:npm/%40scope/name@version`.

Bun workspaces (with `workspaces: ["packages/*"]` in the root `package.json`) follow the same emission pattern as uv workspaces.

## Gradle projects with dependency locking

For a JVM project using `gradle.lockfile`:

```bash
mikebom sbom scan --path ./my-gradle-project --format cyclonedx-json --output sbom.cdx.json
```

Output: one `pkg:maven/<group>/<name>@<version>` per locked entry. The same project may have `buildscript-gradle.lockfile` for plugins (Spotless, jib, etc.); those components are tagged `mikebom:lifecycle-scope: "build"` and emit with CDX native `scope: "excluded"`. Default vuln-scanner pipelines filter them out automatically.

```bash
# To INCLUDE build-only deps in vuln scans:
jq '.components[] | select(.scope != "excluded" or (.properties[]? | .name == "mikebom:lifecycle-scope" and .value == "build"))' sbom.cdx.json
```

## .NET / NuGet projects

```bash
mikebom sbom scan --path ./my-dotnet-app --format cyclonedx-json --output sbom.cdx.json
```

mikebom recognizes three NuGet file shapes:

1. **`.csproj` / `.vbproj` / `.fsproj`** — extracts `<PackageReference Include="X" Version="Y"/>` elements.
2. **`packages.lock.json`** — when present, this is the authoritative version source (transitive deps included).
3. **`Directory.Packages.props`** — Central Package Management (CPM) lookup table for `<PackageReference Include="X"/>` entries without a `Version=` attribute. mikebom walks UP from each `.csproj` to find the nearest `Directory.Packages.props`.

Output: `pkg:nuget/<name>@<version>` per resolved package. Multi-target-framework projects produce separate components per `(name, version)` tuple from the lockfile.

Build-only dependencies (source generators, analyzers, source-link) declared with `PrivateAssets="All"` (or equivalent `IncludeAssets`/`ExcludeAssets` patterns omitting `runtime`) are tagged `mikebom:lifecycle-scope: "build"` + CDX `scope: "excluded"` automatically — same treatment as Gradle's `buildscript-gradle.lockfile`.

## Polyglot scans

A project tree that mixes ecosystems scans cleanly:

```
my-polyglot-app/
├── package.json                # npm side
├── bun.lock                    # Bun side (mikebom prefers — it's more authoritative for Bun-runtime deps)
├── pyproject.toml              # Python config
├── uv.lock                     # uv side
├── gradle/                     # JVM tooling
│   └── gradle.lockfile
└── tools/                      # .NET utilities
    └── MyTools.csproj
```

```bash
mikebom sbom scan --path ./my-polyglot-app --format cyclonedx-json --output sbom.cdx.json
```

All four ecosystems' components appear in the same SBOM, deduplicated by canonical PURL via the milestone-105 dedup pipeline. The `mikebom:also-detected-via` annotation (from milestone 105) records any cross-ecosystem collisions; in practice they're rare across these four since each ecosystem uses a distinct PURL type.

## Verification commands

After running a scan, you can spot-check the new annotations:

```bash
# What ecosystem breakdown did we get?
jq -r '.components[] | .purl // ""' sbom.cdx.json | sed 's|^pkg:\([^/]*\)/.*|\1|' | sort | uniq -c

# Where did mikebom find each component?
jq -r '.components[] | "\(.purl): \((.properties // [])[] | select(.name == "mikebom:source-files") | .value)"' sbom.cdx.json

# Which components are build-only?
jq -r '.components[] | select(.scope == "excluded") | "\(.purl) [build-only]"' sbom.cdx.json

# What's in a workspace?
jq -r '.components[] | select(.properties[]? | (.name == "mikebom:component-role" and .value == "workspace-root")) | "Workspace root: \(.purl)"' sbom.cdx.json
jq -r '.components[] | select(.properties[]? | (.name == "mikebom:component-role" and .value == "main-module")) | "  Member: \(.purl)"' sbom.cdx.json
```
