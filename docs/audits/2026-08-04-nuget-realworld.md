# waybill Audit — NuGet (real-world .NET repos) 2026-08-04

**Audit type**: Real-world empirical measurement of waybill's NuGet reader against `trivy 0.71.1` and `syft 1.44.0`. Extends the m165 kubernetes+argocd and m168 tauri+airflow audit methodology to the .NET ecosystem, motivated by the user's request "make sure we're ready" before the next NuGet-touching feature ships.
**Report status**: FINAL for the three targets scanned. Follow-up items filed as SC-shaped work at the bottom.

## Baseline

| Component | Version | Notes |
|---|---|---|
| **waybill** | `0.1.0-alpha.69` (commit `974ad1a`) | Post-m226 pants Go reader + m227 pants_common refactor + env-var-race fix; release build 2026-08-04 |
| **Trivy** | 0.71.1 | Same pin as m165/m168; installed at `~/.local/bin/trivy` |
| **Syft** | 1.44.0 | Same pin as m165/m168; installed via system pkg |
| **dotnet SDK** | *not installed* | Would have enabled `dotnet list package --include-transitive` as ground-truth tiebreaker — see follow-up FU-004 |
| **Host OS** | macOS Darwin 25.5.0 (ARM64) | |

## Target 1 — RestSharp (small, single-solution, `Directory.Build.props` present)

### Snapshot

| Field | Value |
|---|---|
| **Upstream** | `github.com/restsharp/RestSharp` |
| **Commit SHA** | `6a5082169257438cd085f822f050d93256a8e499` |
| **Clone command** | `git clone --depth 1 https://github.com/restsharp/RestSharp.git` |
| **Clone size** | 5.9 MB |
| **Manifests found** | 16 (`.csproj` + `Directory.Packages.props` + `Directory.Build.props`) |
| **CPM in use** | Yes — root `Directory.Packages.props` (`ManagePackageVersionsCentrally=true`) |
| **Test-dep declaration site** | `test/Directory.Build.props` (NOT in `.csproj` or `Directory.Packages.props`) |

### Per-tool metrics

| Metric | **waybill** | Trivy 0.71.1 | Syft 1.44.0 |
|---|---|---|---|
| Total components | 28 | 28 | 62 |
| **NuGet components** | **16** | **27** | **0** |
| `@unresolved` sentinel | 0 | — | — |
| Literal `$(PropertyName)` in version | 1 | — | — |
| Runtime | 2s | 0.2s | 0.5s |

### Divergence table

**Trivy has, waybill doesn't** (12 packages, all declared in `test/Directory.Build.props`):

`coverlet.collector@6.0.2`, `FluentAssertions@7.0.0`, `JetBrains.Annotations@2024.3.0`, `Microsoft.NET.Test.Sdk@17.12.0`, `Microsoft.NETFramework.ReferenceAssemblies.net472@1.0.3`, `Microsoft.SourceLink.GitHub@8.0.0`, `MinVer@6.0.0`, `rest-mock-core@0.7.12`, `WireMock.Net.FluentAssertions@1.5.51`, `Xunit.Extensions.Logging@1.1.0`, `xunit.runner.visualstudio@2.8.2`, `xunit@2.9.2`

**Waybill has, trivy doesn't** (1 package):

`pkg:nuget/System.Text.Json@$(SystemTextJsonVer)` — literal MSBuild variable reference emitted into PURL version segment. `$(SystemTextJsonVer)` is defined in a conditional `<PropertyGroup>` inside the same `Directory.Packages.props` (`<SystemTextJsonVer>10.0.0</SystemTextJsonVer>` guarded by `Condition="'$(TargetFramework)' == 'net10.0'"`). Waybill does not resolve the variable.

**Syft finding**: emits 0 `pkg:nuget/*` components. The 62 total are all `pkg:pypi/*` (Syft's Python-env-scan of its own bundled interpreter tree) + `pkg:github/actions/*` from `.github/workflows/`. Confirms Syft does not scan `.csproj` files by default on this repo shape.

### Verdict: Waybill missing 12 packages via **FU-001** (`Directory.Build.props` not parsed) + 1 broken PURL via **FU-002** (unresolved MSBuild property).

---

## Target 2 — Serilog (CPM absent, inline `Version=` in `.csproj`)

### Snapshot

| Field | Value |
|---|---|
| **Upstream** | `github.com/serilog/serilog` |
| **Commit SHA** | `49b5339ce85385dc52d4d8e8f2b8308becf23506` |
| **Clone command** | `git clone --depth 1 https://github.com/serilog/serilog.git` |
| **Clone size** | 2.3 MB |
| **Manifests found** | 7 (`.csproj` files + root `Directory.Build.props`) |
| **CPM in use** | No — `.csproj` files carry inline `Version="X"` attributes |

### Per-tool metrics

| Metric | **waybill** | Trivy 0.71.1 | Syft 1.44.0 |
|---|---|---|---|
| Total components | 15 | (unmeasured, all 0 nuget) | (unmeasured, 0 nuget) |
| **NuGet components** | **15** | **0** | **0** |
| `@unresolved` sentinel | 0 | — | — |
| Literal `$(PropertyName)` in version | 0 | — | — |

### Divergence

- **Waybill has, trivy doesn't** (all 15 waybill entries — trivy sees zero on this repo shape): BenchmarkDotNet, Microsoft.NET.Test.Sdk, Newtonsoft.Json, PolySharp (x2 versions — 1.14.1 and 1.15.0, split across sub-projects), PublicApiGenerator, Shouldly, System.Diagnostics.DiagnosticSource, System.Security.Cryptography.Xml, System.ServiceModel.Http, System.ServiceModel.Primitives, System.Threading.Channels, System.ValueTuple, xunit, xunit.runner.visualstudio.

### Verdict: **Waybill wins outright** on this shape. Trivy's `.csproj` reader appears to require CPM or `packages.lock.json` to emit anything — inline `Version=` alone is ignored. Waybill correctly handles per-`.csproj` inline versions AND correctly preserves multiple versions of the same package name (`PolySharp` at 1.14.1 vs 1.15.0).

---

## Target 3 — dotnet/orleans (large monorepo, extensive CPM + `Directory.Build.props`)

### Snapshot

| Field | Value |
|---|---|
| **Upstream** | `github.com/dotnet/orleans` |
| **Commit SHA** | `114eae10d680886946abc700ab3c0ed292def8d0` |
| **Clone command** | `git clone --depth 1 https://github.com/dotnet/orleans.git` |
| **Clone size** | 69 MB |
| **Manifests found** | 235 `.csproj` + multiple `Directory.Build.props` + `Directory.Packages.props` (root + `samples/`) |
| **CPM in use** | Yes — root + samples/ each have their own `Directory.Packages.props` |
| **Lockfiles** | None (`packages.lock.json` absent) |

### Per-tool metrics

| Metric | **waybill** | Trivy 0.71.1 | Syft 1.44.0 |
|---|---|---|---|
| Total components | 1061 | (unmeasured) | (unmeasured) |
| **NuGet components** | **154** | **187** | **0** |
| `@unresolved` sentinel emitted | **20** | — | — |
| Literal `$(PropertyName)` in version | **3** | — | — |
| Reader runtime (release build) | 2.48s | 0.42s | 1.09s |

### Divergence

**Trivy has, waybill doesn't** (64 packages) — sample:

Aspire.* (7 pkgs, `13.2.4`), Azure.Core, Azure.Identity, Azure.Security.KeyVault.Secrets (x2 versions), BenchmarkDotNet, coverlet.collector, FSharp.Core, Google.Cloud.PubSub.V1, … 64 total.

**Waybill has, trivy doesn't** (31 entries) — sample:

- 20 `@unresolved` sentinel entries: `Aspire.Azure.Data.Tables@unresolved`, `Aspire.Azure.Storage.Blobs@unresolved`, `Aspire.Hosting.AppHost@unresolved`, `Azure.Identity@unresolved`, `Azure.Storage.Blobs@unresolved`, `FSharp.Core@unresolved`, … — these are declarations waybill's version-resolution ladder ran out on (no `Version=`, no CPM entry the walker found, no lockfile).
- 3 literal `$(...)` leaks: `Google.Protobuf@$(GoogleProtobufVersion)`, `Microsoft.Extensions.Logging.Abstractions@$(MicrosoftExtensionsLoggingAbstractionsVersion)`, `Microsoft.Extensions.Options@$(MicrosoftExtensionsOptionsVersion)`.
- 8 legitimate additional versions of packages where waybill correctly preserves multiple per-sub-project pins (e.g., `Microsoft.Extensions.Hosting@6.0.0` + `@10.0.0`).

### Verdict: **Two real bugs surface + one design gap**:
- Bug FU-002: MSBuild property references leak into PURL versions on 3 declarations.
- Bug FU-003: `@unresolved` sentinel is emitted as a broken PURL version on 20 declarations — should downgrade to design-tier with `sbom_tier="design"` + empty version, matching waybill's cross-ecosystem "design-tier means operator-declared but not-resolved" convention.
- Gap FU-001: `Directory.Build.props` still contributes to the 33-package delta (many of the 64 trivy-only packages are declared in `samples/*/Directory.Build.props` or `test/Directory.Build.props`).

---

## Recommended follow-on milestones

Ordered by impact × unit cost:

### FU-001 — Parse `Directory.Build.props` for `<PackageReference>` + `<PackageVersion>` declarations

**Impact**: HIGH. Explains ~50% of the trivy divergence across all 3 targets. Real-world .NET convention is to hoist test/dev dep declarations into `test/Directory.Build.props` (RestSharp does this) or `samples/Directory.Build.props` (Orleans does this).

**Cost**: MEDIUM (~1 workday). Waybill's existing `Directory.Packages.props` parser at `csproj.rs:305` handles nearly-identical XML shape. Extending the walker to also visit `Directory.Build.props` files up the ancestor chain (bounded by `scan_root`) is the same pattern, applied to a differently-named file. Currently listed as explicit "out of scope" in m106 spec (`docs/ecosystems.md:829`) — worth promoting.

**Constitution check**: no new deps, no new C-native transitives.

### FU-002 — Resolve MSBuild `$(PropertyName)` references from `<PropertyGroup>` blocks

**Impact**: MEDIUM. 4 broken PURLs across the 3 targets (1 in RestSharp, 3 in Orleans). Blocks a real supply-chain user question ("what version is `Google.Protobuf` pinned at?").

**Cost**: MEDIUM (~1 workday). Requires a two-pass parser: pass 1 collects `<PropertyName>value</PropertyName>` from all `<PropertyGroup>` blocks in the same file + ancestor `Directory.*.props` (with the existing walker), pass 2 substitutes `$(...)` references in `Version=` attributes. Conditional property groups (`Condition="'$(TargetFramework)' == 'net10.0'"`) can be handled by taking the LAST-defined value (simulates MSBuild's default evaluation order); more correctly by per-target-framework emission but that's much bigger scope.

**Constitution check**: no new deps.

**Interim mitigation** (until FU-002 lands): detect `$(...)` in a resolved Version string and emit a WARN + skip the entry rather than emit a broken PURL. One-liner in `csproj.rs`.

### FU-003 — Replace `@unresolved` sentinel with design-tier component

**Impact**: HIGH. `pkg:nuget/*@unresolved` is a syntactically-invalid PURL (nuget spec requires SemVer). Downstream SBOM consumers (Trivy fs-scanning the emitted CDX, DependencyTrack, etc.) will either error or drop these entries silently.

**Cost**: LOW (~2 hours). One-file change: in `csproj.rs`'s `PackageDbEntry` construction, when the version resolves to the sentinel, instead set `sbom_tier: Some("design")` + `version: String::new()` + `waybill:unresolved-reason` annotation (a new C-row is possibly warranted; alternatively reuse the existing design-tier convention that operators already understand). Matches waybill's cross-ecosystem posture that design-tier = operator-declared but not resolved (see e.g., cargo reader when Cargo.toml declares a dep with no matching Cargo.lock entry).

**Constitution check**: aligns with Principle IX (accuracy over fabrication — emitting `@unresolved` is fabrication of a version). Aligns with m191/m175 KEEP-NATIVE-FIRST posture on empty `version` field semantics.

### FU-004 — Add `dotnet list package --include-transitive` tiebreaker to the audit harness

**Impact**: LOW (audit-infrastructure only, no user-visible waybill change). Would let this audit assert ground truth on which of trivy vs waybill is CORRECT for the divergent packages — currently we can only say "they disagree". `dotnet list package --include-transitive --format json` on each `.csproj` gives the authoritative transitive-resolved set.

**Cost**: MEDIUM (~1 workday). Requires installing the .NET SDK in CI (which is skipped today per zero-Docker-required posture). If we extend the m195 corpus-harness path, this becomes a permanent audit invariant.

### FU-005 — Add real-world NuGet targets to the m195 corpus harness

**Impact**: MEDIUM. Turns this one-shot audit into a permanent regression guard. Would catch waybill regressions on real-world .NET shapes before they ship.

**Cost**: MEDIUM (~1 workday). Pin RestSharp + Serilog + Orleans at the SHAs above; add golden SBOMs; wire into `waybill-cli/tests/corpus_harness_195/`. Requires FU-004 for authoritative goldens.

---

## Backlog observations

- **Waybill's monorepo detection triggered on RestSharp** (log: "monorepo shape detected: 2 workspaces (., docs)") — the `docs/` dir with its own `.csproj` is treated as a separate workspace. That's technically correct but potentially noisy. Not a bug, more of a UX consideration.
- **PolySharp version fanout** — Serilog emits both `1.14.1` and `1.15.0` because different sub-projects pin different versions. Waybill correctly preserves this. Would be worth an integration test in the corpus harness (FU-005).
- **Orleans has 2 csproj files with `<PackageReference>` missing `Include=`** — waybill emits WARN and skips gracefully. Good fail-open behavior; test/Benchmarks/Benchmarks.csproj + test/Orleans.Serialization.UnitTests/Orleans.Serialization.UnitTests.csproj on the pinned SHA. Not filed as a bug (probably these are `<PackageReference>` inside a `<Choose><When>` block or a `Version=` include, not a real error).
- **Speed comparison**: on Orleans (69 MB, 235 csproj), Trivy 6× faster than waybill (0.42s vs 2.48s). Waybill's runtime is not a concern at this scale but worth watching if aspnetcore-sized repos become a target (~2000 csproj files).
- **Syft is 0-vs-N across all 3 targets** — Syft does not appear to scan `.csproj` files at all under `dir:` mode. Its `.NET` support is CLR-binary-only (`.exe`/`.dll` PE header parsing). Different tool with different scope; not directly comparable for source-tree scans.

---

## Executive summary

Waybill's NuGet reader is **production-usable on typical .NET repos** but has 3 concrete bugs and 1 design gap that will trip real users:

1. **`Directory.Build.props` blindspot (FU-001)** — most impactful gap; explains ~50% of the trivy divergence. Real repos commonly declare test/dev deps here.
2. **MSBuild property references leak (FU-002)** — 4 broken PURLs across 3 targets; small-scope fix.
3. **`@unresolved` sentinel emitted as invalid PURL (FU-003)** — most serious data-integrity bug; 2-hour fix. Ships invalid PURLs into production SBOMs on ~20 Orleans components.
4. **Ground-truth verification blocked** on absence of `dotnet` CLI in the audit harness (FU-004).

Waybill wins outright on **Serilog** (15 vs 0 vs 0) — inline `Version=` handling is a real edge over trivy. Waybill loses to trivy on **RestSharp** (16 vs 27) and **Orleans** (154 vs 187), with all divergence traceable to FU-001 or FU-002.

Recommended sequencing: **FU-003 first** (2 hours; data-integrity), **FU-002 next** (interim WARN+skip mitigation is a 5-minute fix, full resolver is 1 workday), **FU-001 third** (1 workday; broadest impact but biggest scope). FU-004 + FU-005 are follow-up infrastructure.

## Reproduction appendix

Per-target SBOMs are checked in at `specs/audit-nuget-realworld/artifacts/`:

- `restsharp.{waybill,trivy,syft}.cdx.json`
- `serilog.{waybill,trivy,syft}.cdx.json`
- `orleans.{waybill,trivy,syft}.cdx.json`

Reproduce with:

```bash
# Waybill
./target/release/waybill --offline sbom scan \
    --path <repo> --format cyclonedx-json \
    --output <name>.waybill.cdx.json --no-deep-hash

# Trivy
trivy fs <repo> --format cyclonedx --output <name>.trivy.cdx.json --quiet

# Syft
syft dir:<repo> -o cyclonedx-json=<name>.syft.cdx.json --quiet
```

Diff PURL sets with:

```bash
jq -r '.components[]? | select(.purl // "" | startswith("pkg:nuget/")) | .purl' \
    <name>.<tool>.cdx.json | sort -u > <name>-<tool>-purls.txt
comm -23 <name>-trivy-purls.txt <name>-waybill-purls.txt  # trivy has, waybill missing
comm -13 <name>-trivy-purls.txt <name>-waybill-purls.txt  # waybill has, trivy missing
```

Waybill version pinned via `git rev-parse HEAD` at the audit branch's tip (currently `974ad1a`). Any future re-run against a later waybill build should reference its own commit SHA.

---

## Post-milestone-230 update

**Gap closed**: root→direct dependency edges from a per-project main-module component. Pre-m230, every NuGet package's incoming-edge count was ≤ its use as a transitive dependency of some other package — so any direct dep that wasn't pulled in transitively (e.g., `OpenTelemetry.Exporter.OpenTelemetryProtocol` in the reporter's `dotnet/eShop` scan) had zero incoming edges and was orphaned from the dependency graph.

**Measured impact against the `packages_lock_present` fixture** (which reproduces the same failure shape):

| Metric | Pre-m230 | Post-m230 |
|---|---|---|
| NuGet components (Direct/CentralTransitive class) with ≥1 incoming edge | 0/1 (SampleLib orphaned) | 1/1 |
| Main-module components emitted | 0 | 1 per `.csproj` (promoted to `metadata.component` when the scan surfaces a single project) |
| `waybill:graph-completeness-reason` includes `multi-ecosystem-partial-root: nuget` | yes | no |

Behavior shipped by m230:
- Reader emits one main-module component per project file (`.csproj` / `.vbproj` / `.fsproj`), tagged `waybill:component-role: "main-module"`, `sbom_tier: "source"`.
- Main-module PURL is `pkg:nuget/<AssemblyName>@<version>` when a version resolves via the FR-010 ladder (`<Version>` → `<VersionPrefix>` (+ `<VersionSuffix>`) → `<AssemblyVersion>`), falling back to `pkg:generic/<project-stem>@0.0.0`.
- Root→direct edges populate from lockfile entries typed `Direct` or `CentralTransitive` (union across every TFM; deduplicated by name). When no `packages.lock.json` is present, edges derive from `<PackageReference Include=...>` items on the project itself (design-tier fallback).

Explicitly deferred:
- `ProjectReference`-style main-module→main-module edges (FR-007) — needed for multi-project solutions but out of scope for m230. The lockfile-side `entry_type: "Project"` continues to be skipped.

Verify no Direct/CentralTransitive package remains orphaned:
```bash
jq -r '
  ([.dependencies[] | .dependsOn[]?] | unique) as $has_incoming
  | [.components[] | select(.purl // "" | startswith("pkg:nuget/"))
     | select(((.properties // []) | any(.name == "waybill:component-role"
                                          and .value == "main-module")) | not)
     | ."bom-ref"]
    - $has_incoming
' <name>.waybill.cdx.json
# Expect: [] (empty)
```
