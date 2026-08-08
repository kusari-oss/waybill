# Quickstart: verifying the NuGet main-module fix works

**Feature**: 230-nuget-main-module
**Phase**: 1

Executes SC-001, SC-002, SC-003, and SC-004 predicates against the finished implementation so the coding phase has a concrete acceptance test. All commands assume repository root at `/Users/mlieberman/Projects/mikebom` and cargo-built waybill binary at `target/release/waybill`.

## Setup

Build the milestone-230 waybill binary:

```bash
cargo build --release -p waybill
```

## Walkthrough — SC-001: locked NuGet main-modules exist and reach every direct dep

Scan the RestSharp audit fixture:

```bash
./target/release/waybill sbom scan \
  --path specs/audit-nuget-realworld/fixtures/restsharp \
  --format cyclonedx-json \
  --output /tmp/restsharp.230.cdx.json \
  --no-deep-hash
```

Verify at least one main-module per `.csproj`:

```bash
jq '.components | map(select((.properties // []) | any(.name == "waybill:component-role" and .value == "main-module"))) | length' /tmp/restsharp.230.cdx.json
# Expect: >=1 (RestSharp has one main project + test project = 2)
```

Verify every lockfile-Direct component has at least one incoming edge:

```bash
jq -r '
  ([.dependencies[] | .dependsOn[]?] | unique) as $has_incoming |
  ([.components[] | select(.purl // "" | startswith("pkg:nuget/")) | select(((.properties // []) | any(.name == "waybill:component-role" and .value == "main-module")) | not) | ."bom-ref"]) as $nuget_pkgs |
  ($nuget_pkgs | map(select(. as $r | $has_incoming | index($r) | not)))
' /tmp/restsharp.230.cdx.json
# Expect: [] (empty list of orphaned NuGet packages)
```

## Walkthrough — SC-002: RestSharp fixture flips from 0/16 → 16/16 incoming coverage

Compare pre-230 vs post-230:

```bash
# Pre-230 baseline (committed audit artifact)
jq -r '
  ([.dependencies[] | .dependsOn[]?] | unique) as $has_incoming |
  ([.components[] | select(.purl // "" | startswith("pkg:nuget/")) | ."bom-ref"]) as $nuget |
  {
    total: ($nuget | length),
    with_incoming: (($nuget | map(select(. as $r | $has_incoming | index($r)))) | length)
  }
' specs/audit-nuget-realworld/artifacts/restsharp.waybill.cdx.json
# Expect (pre-230): {"total": 16, "with_incoming": 0}

# Post-230 output
jq -r '
  ([.dependencies[] | .dependsOn[]?] | unique) as $has_incoming |
  ([.components[] | select(.purl // "" | startswith("pkg:nuget/")) | select(((.properties // []) | any(.name == "waybill:component-role" and .value == "main-module")) | not) | ."bom-ref"]) as $nuget_pkgs |
  {
    total: ($nuget_pkgs | length),
    with_incoming: (($nuget_pkgs | map(select(. as $r | $has_incoming | index($r)))) | length)
  }
' /tmp/restsharp.230.cdx.json
# Expect (post-230): {"total": 16, "with_incoming": 16}
# (All 16 pre-230 NuGet package components now have incoming edges from a main-module)
```

## Walkthrough — SC-003: package-component byte parity

```bash
# Extract the NuGet package-component PURL set from both scans
jq -r '.components[] | select(.purl // "" | startswith("pkg:nuget/")) | select(((.properties // []) | any(.name == "waybill:component-role" and .value == "main-module")) | not) | .purl' \
  specs/audit-nuget-realworld/artifacts/restsharp.waybill.cdx.json | LC_ALL=C sort > /tmp/pre230.purls

jq -r '.components[] | select(.purl // "" | startswith("pkg:nuget/")) | select(((.properties // []) | any(.name == "waybill:component-role" and .value == "main-module")) | not) | .purl' \
  /tmp/restsharp.230.cdx.json | LC_ALL=C sort > /tmp/post230.purls

diff -u /tmp/pre230.purls /tmp/post230.purls
# Expect: empty diff — every pre-230 NuGet package PURL is preserved
```

## Walkthrough — SC-004: graph-completeness no longer flags nuget as partial-root

```bash
jq -r '.metadata.properties[] | select(.name == "waybill:graph-completeness-reason") | .value' /tmp/restsharp.230.cdx.json
# Expect: absent, OR present without the substring "multi-ecosystem-partial-root: nuget"
# Pre-230 comparison (should currently contain that substring):
jq -r '.metadata.properties[] | select(.name == "waybill:graph-completeness-reason") | .value' specs/audit-nuget-realworld/artifacts/restsharp.waybill.cdx.json
# Expect: "multi-ecosystem-partial-root: nuget; transitive-edges-unresolvable: npm"
```

## Walkthrough — US2: unlocked scan produces design-tier edges

Create a scratch project without `packages.lock.json`:

```bash
tmp=$(mktemp -d)
cat > "$tmp/App.csproj" <<'EOF'
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net8.0</TargetFramework>
    <Version>1.0.0</Version>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="Newtonsoft.Json" Version="13.0.3" />
  </ItemGroup>
</Project>
EOF

./target/release/waybill sbom scan --path "$tmp" --format cyclonedx-json \
  --output "$tmp/scan.cdx.json" --no-deep-hash

# Assert main-module exists
jq '.components[] | select(.purl == "pkg:nuget/App@1.0.0")' "$tmp/scan.cdx.json"

# Assert Newtonsoft.Json is reachable from the main-module
jq '.dependencies[] | select(.ref == "pkg:nuget/App@1.0.0") | .dependsOn' "$tmp/scan.cdx.json"
# Expect: ["pkg:nuget/Newtonsoft.Json@13.0.3"]
```

## Post-implementation checklist

- [ ] SC-001: at least one main-module component per `.csproj` in the RestSharp fixture; every lockfile-Direct NuGet package has ≥1 incoming edge.
- [ ] SC-002: RestSharp post-230 shows 16/16 NuGet package components with incoming edges (vs 0/16 pre-230).
- [ ] SC-003: NuGet package-component PURL set byte-identical across pre-/post-230 (empty `diff -u`).
- [ ] SC-004: `waybill:graph-completeness-reason` does not include `multi-ecosystem-partial-root: nuget` on the RestSharp fixture.
- [ ] SC-005: an unlocked scratch project produces the same root→direct edge topology as an equivalent locked one.
- [ ] Pre-PR gate: `./scripts/pre-pr.sh` exit 0.
- [ ] Regenerate `specs/audit-nuget-realworld/artifacts/restsharp.waybill.cdx.json` + `serilog.waybill.cdx.json` + `orleans.waybill.cdx.json` and commit the new goldens as milestone-230 baselines.
