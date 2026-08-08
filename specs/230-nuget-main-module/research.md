# Phase 0 Research: NuGet main-module + root→direct edges

**Feature**: 230-nuget-main-module
**Date**: 2026-08-07

Every decision below is anchored to code that already exists in the repo. No NEEDS CLARIFICATION items remain from the spec — the two markers were resolved during `/speckit.specify`'s clarification loop.

## R1 — Where does the main-module edge wiring actually happen?

**Decision**: Follow the cargo (m064) / maven (m085) pattern verbatim. The reader (`nuget/mod.rs`) returns `PackageDbEntry` objects whose `depends: Vec<String>` field contains the (case-preserved) NuGet package names. `scan_fs/mod.rs`'s existing edge-emission loop at `waybill-cli/src/scan_fs/mod.rs:560+` translates those names to resolved PURLs via the `name_to_purl` `HashMap<(String, String), String>` (ecosystem × normalized-name → PURL) that it builds in the same pass.

**Rationale**: The pattern is battle-tested across every other main-module-emitting reader. Reproducing it means:
- No changes to `scan_fs/mod.rs` unless a NuGet-specific name-normalization key is needed (analogous to maven's group:artifact key at `scan_fs/mod.rs:575-583` or the cargo `<name> <version>` disambiguation at `scan_fs/mod.rs:606+`). NuGet package IDs are case-insensitive but the `name_to_purl` lookup already normalizes via `normalize_dep_name(ecosystem, name)` — the existing NuGet package-entry emission at `nuget/mod.rs:391+` already relies on this. Adding main-modules doesn't introduce a new key shape.
- The main-module's `depends: Vec<String>` mirrors how package-level entries' `depends` fields are populated today at `nuget/mod.rs:351-354`.

**Alternatives considered**:
- *Emit fully-resolved PURL edges from the reader.* Rejected: breaks the m064 / m085 / m216 pattern and would require the reader to know about post-processing steps (workspace-parent tagging at m127, produces-binaries stamping at m116) that live outside the reader.
- *Introduce a new `main_module_depends: Vec<String>` field on `PackageDbEntry` to distinguish root→direct edges from transitive ones.* Rejected: no other ecosystem needs this; the existing `depends` field carries both classes and downstream consumers distinguish by the source component's `waybill:component-role` annotation. Complexity Tracking would be triggered.

## R2 — How does the main-module version-derivation ladder consume `msbuild_properties.rs`?

**Decision**: The version-derivation ladder from FR-010 reads elements from the parsed `.csproj`/`.vbproj`/`.fsproj`, then runs each candidate through the existing `msbuild_properties::substitute` helper against a merged property map assembled by the same walker the reader uses today for package-version resolution (`msbuild_properties::parse_properties_file` on the csproj + walked ancestor `Directory.Packages.props` chain, merged via `msbuild_properties::merge`).

Concretely: for each project file:
1. Read the file's XML text (already done by `csproj::parse`).
2. Assemble the property map: `merge(ancestor Directory.Packages.props properties, csproj-local properties)`.
3. For each candidate element in the ladder (`<Version>`, `<VersionPrefix>` (+ `<VersionSuffix>`), `<AssemblyVersion>`), extract the raw value; if non-empty, pass it through `substitute(raw_value, &property_map)`.
4. Check the result via `msbuild_properties::substitute_and_check` — if it still contains an unresolved `$(...)` reference, fall through to the next ladder step.
5. If all four ladder steps produce empty or unresolved values, use the FR-003 `pkg:generic/<project-filename-stem>@0.0.0` PURL shape.

**Rationale**: The `msbuild_properties.rs` helper (added under #654 / FU-002) is the correct substrate. It already handles case-insensitive property lookup, conditional groups (last-defined wins), and the `substitute_and_check` half-resolved detection needed for the fallback. Reusing it means the version-derivation ladder has the same MSBuild-semantic fidelity as the package-version resolution the reader already ships. No new parsing logic needed.

**Alternatives considered**:
- *Do MSBuild property substitution inline in a new helper.* Rejected: duplicates code the reader has just landed for exactly this problem class (#654 was about resolving `$(SystemTextJsonVer)` in `<PackageVersion>` elements — same substitution mechanic, different consumer).
- *Skip property substitution and only honor literal `<Version>` values.* Rejected: >30% of SDK-style projects declare `<Version>` via a property reference; skipping substitution would make the fallback fire on the majority path, wasting the ladder.

## R3 — How does the `<AssemblyName>` override interact with the PURL name segment?

**Decision**: The main-module PURL's name segment is derived via the following order: (1) the project file's `<AssemblyName>` element if set (this is the assembly the project actually produces, and how downstream .NET consumers reason about identity), (2) fall back to the project filename stem (`.csproj`/`.vbproj`/`.fsproj` filename without extension) if `<AssemblyName>` is absent.

**Rationale**: MSBuild's default rule for assembly-name-when-unset is the project's own filename stem; that's what the .NET compiler emits into the produced DLL's identity metadata. Mirroring that gives the main-module PURL a name that matches what a `dotnet build`-consumer would see. `<AssemblyName>` overrides are common in projects where the on-disk filename doesn't match the desired assembly name (e.g., `MyProject.Core.csproj` producing `Contoso.Framework.dll`).

**Alternatives considered**:
- *Always use the project filename stem.* Rejected: silently disagrees with the produced assembly's actual identity when `<AssemblyName>` is set; makes SBOM identity un-linkable to binary identity.
- *Look at `<PackageId>` (NuGet-specific).* Rejected: `<PackageId>` is meaningful for projects that produce a NuGet package themselves; the main-module represents the project as a build unit, not necessarily as a publishable package. Also, only ~5% of projects set it. `<AssemblyName>` covers the general case.

## R4 — What's the byte-parity boundary between pre-230 and post-230 goldens?

**Decision**: For the three existing audit fixtures at `specs/audit-nuget-realworld/artifacts/` (RestSharp, Serilog, Orleans), post-230 goldens differ from pre-230 goldens by:
- ADDED: new components with `type: "application"` (matching cargo's `sbom_tier: "source"` main-module shape), one per project file discovered, with `waybill:component-role: "main-module"` annotation.
- ADDED: new `dependencies[]` entries whose `ref` fields are the new main-module PURLs and whose `dependsOn[]` lists resolve to existing package-level component refs.
- UNCHANGED: every pre-230 package-level component (same PURL, same version, same annotations, same edges to its own package→package dependencies). SC-003 gates on this.

**Rationale**: Turning FR-006 + SC-003 into a concrete diff shape — "strict superset on components; strict superset on dependencies; zero mutation of existing entries" — gives the implementation a bright-line success signal.

**Alternatives considered**:
- *Snapshot the entire byte-for-byte SBOM output and diff.* Rejected: adds noise (timestamps, serial numbers, ordering) unrelated to the milestone. Component-set + edge-set diffs are the semantic invariants; the byte-level equivalence tooling from memory `feedback_verify_golden_churn_normalized` handles the noise-mask.
- *Update the pre-230 goldens in-place and diff against a prior git ref.* Rejected: harder to review; loses the audit-time snapshot as a durable baseline.

## R5 — Should the msbuild_properties.rs helper also feed <AssemblyName> resolution?

**Decision**: Yes. `<AssemblyName>` can itself be a property reference (`<AssemblyName>$(RootNamespace).Core</AssemblyName>`), and the same `substitute` helper covers it. R3's ladder result runs through `substitute` before being used as a PURL segment.

**Rationale**: Consistency with R2 — every non-literal string extracted from the project file gets the same property-substitution treatment. Zero additional code cost.

## R6 — Existing test fixtures + regression coverage

**Decision**: Use the m106 unit-test infrastructure inside `nuget/mod.rs` (tests at `mod.rs:486+`) for happy-path + edge-case unit tests. Add integration coverage by extending the existing `waybill-cli/tests/transitive_parity_*.rs` pattern (from milestone 083) with a new NuGet-specific parity test that scans the RestSharp fixture and asserts:
- Component count matches the pre-230 golden's NuGet-component subset exactly (byte-parity guard from FR-006).
- Every lockfile Direct/CentralTransitive component has ≥1 incoming edge (SC-001 + SC-002).

**Rationale**: Zero new test infrastructure needed. The pre-230 goldens live in-repo under `specs/audit-nuget-realworld/artifacts/`. The `transitive_parity_*` test file convention already exists and does exactly this shape of assertion for other ecosystems.

**Alternatives considered**:
- *Golden regeneration + diff test.* Rejected: same reason as R4's alternative — noise-mask complexity is orthogonal to what this milestone verifies.
- *Only unit-test at the reader layer, skip integration.* Rejected: R1's decision means main-module edges are wired outside the reader; a reader-only test would miss the failure mode where `scan_fs/mod.rs`'s edge emission doesn't recognize the new main-module.
