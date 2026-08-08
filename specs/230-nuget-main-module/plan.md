# Implementation Plan: NuGet main-module component + root→direct dependency edges

**Branch**: `230-nuget-main-module` | **Date**: 2026-08-07 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/230-nuget-main-module/spec.md`

## Summary

Bring the NuGet reader in line with the six sibling ecosystems that already emit a main-module component per project file: cargo (m064), npm (m066), pip (m068), gem (m069), maven (m070), Gemfile (m216). Today, `waybill-cli/src/scan_fs/package_db/nuget/mod.rs:453 build_lock_edges` builds only package→package edges from `packages.lock.json` — every direct dependency that isn't pulled in transitively by another package ends up orphaned. This milestone adds one main-module per `.csproj`/`.vbproj`/`.fsproj` and populates its `depends` list from lockfile entries typed `Direct` + `CentralTransitive` (US1); when no lockfile exists, from `<PackageReference>` items in the project file (US2, design tier). No new NuGet components are added; component detection is byte-identical to the pre-230 goldens.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–229; no nightly required for this user-space-only ecosystem-parity work)
**Primary Dependencies**: Existing only — `quick-xml = "0.31"` (already used pervasively in the NuGet reader for `.csproj`/`Directory.Packages.props`/`Directory.Build.props` parsing), `serde_json` (already used for `packages.lock.json`), `waybill_common::types::purl::Purl` + `encode_purl_segment` (main-module PURL construction), `tracing`, `anyhow`, `thiserror`. **Zero new Cargo dependencies.** No subprocess calls. No network access.
**Storage**: N/A — all state in-process per scan; mirrors every reader milestone since 002.
**Testing**: `cargo +stable test --workspace` — new unit tests colocated with existing NuGet tests in `mod.rs`; integration coverage via the existing `specs/audit-nuget-realworld/artifacts/` fixture set (RestSharp, Serilog, Orleans) — pre-230 goldens become the byte-parity baseline for FR-006 / SC-003.
**Target Platform**: All platforms waybill already builds on (Linux, macOS, Windows per m100). No platform-specific code.
**Project Type**: Single-crate extension inside the existing `waybill-cli/src/scan_fs/package_db/nuget/` module. No new crate.
**Performance Goals**: No measurable regression on the existing NuGet audit fixture set. The main-module path adds one component per project file (typically ≤50 per solution) and one edge per direct dep (typically ≤200 per solution). Both well below any perf-relevant threshold in the scan_fs pipeline.
**Constraints**: FR-006 (byte-identical package-component set pre/post 230, verified against the three audit fixtures). Must reuse existing `entry_type` classification at `nuget/mod.rs:321-334`; no changes to `packages_lock` module structs.
**Scale/Scope**: Real .NET solutions range from 1 project (bat-shape utilities) to ~250 projects (Orleans monorepo). Both bounds handled by the same path — main-module count grows linearly with project-file count; edge count grows linearly with lockfile-entry count. No structural change to the reader's iteration pattern.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Evaluated against `.specify/memory/constitution.md` v2.1.0. **All principles PASS.**

- **I. Pure Rust, Zero C**: PASS. Extension is Rust-only, no new C, no new toolchain dependencies. No new Cargo dependencies at all.
- **II. eBPF-Only Observation** + **XII. External Data Source Enrichment**: PASS with note. The NuGet reader lives in `scan_fs/package_db/`, which is waybill's static-scan mode — a mode whose existence is already sanctioned by every sibling milestone (m064 cargo, m066 npm, m068 pip, m069 gem, m070 maven, m216 Gemfile) and by the constitution's XII carve-out for "lockfiles MAY be read to add dependency-tree edges." This milestone reads the *same* files the reader reads today (`.csproj` + `packages.lock.json`) and adds an *enriching* edge shape (root→direct) that closes the parity gap with sibling readers. It does not introduce a new dependency source or add components not already discoverable by the existing package-DB reader.
- **III. Fail Closed**: N/A — this is a scan_fs-mode feature; there's no eBPF trace to fail closed on. The existing reader's warn-and-skip behavior on malformed lockfiles (documented in the spec's Edge Cases) is preserved.
- **IV. Type-Driven Correctness**: PASS. All new PURLs go through `waybill_common::types::purl::Purl::new()` for validation (spec-blessed `pkg:nuget/` and `pkg:generic/` types). No new raw `String`-typed domain values. No `.unwrap()` in production paths (test-code `.unwrap()` guarded per the existing `#[cfg_attr(test, allow(clippy::unwrap_used))]` pattern).
- **V. Specification Compliance**: PASS with explicit audit. The `waybill:component-role: "main-module"` annotation this milestone emits is **not new** — it is the same field cargo (m064), gem (m069), maven (m070), Gemfile (m216) already emit and that milestone-071's parity catalog (row C40) already registers. Because the annotation carries a semantic (this component *is* the project itself, not a dependency of it) that neither CDX 1.6 nor SPDX 2.3 nor SPDX 3.0 expresses natively via any single field, the `waybill:*` annotation is the correct carrier per Principle V's carve-out for "finer-grained information the standard does not express." No new `waybill:*` field is introduced by this milestone; the mapping to sbom-format-mapping.md's C40 row is inherited. FR-002 in the spec cites this audit.
- **VI. Three-Crate Architecture**: PASS. No new crates. Change is contained inside `waybill-cli/src/scan_fs/package_db/nuget/`.
- **VII. Test Isolation**: PASS. All new tests are pure-logic unit tests + fixture-driven integration tests. No eBPF, no root, no CAP_BPF.
- **VIII. Completeness**: PASS. This milestone directly closes a completeness gap (100% of NuGet package components currently orphaned in the RestSharp fixture; post-230 the orphan rate drops to 0% for Direct + CentralTransitive entries). SC-002 measures this.
- **IX. Accuracy**: PASS. FR-006 + SC-003 explicitly guard byte-parity on the pre-230 package-component set: no new phantom components are introduced. Main-modules are added, but they represent the project files themselves (which physically exist on disk), not fabricated packages.
- **X. Transparency**: PASS. US2's design-tier fallback marks its edges as design-tier per the existing reader's convention (`sbom_tier: "source"` for main-module, standard tier-marking for `<PackageReference>`-derived edges); consumers can distinguish lockfile-backed from design-tier evidence.
- **XI. Enrichment** / **XII. External Data Source Enrichment**: PASS. This milestone *is* enrichment — it enriches the existing package-graph with the root→direct edge shape derivable from the same manifest data the reader already parses. No external network calls, no new data sources.

No violations. No entries needed in Complexity Tracking below.

## Project Structure

### Documentation (this feature)

```text
specs/230-nuget-main-module/
├── plan.md              # This file (/speckit.plan output)
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   └── nuget-main-module-shape.md   # Emitted shape spec
├── checklists/
│   └── requirements.md              # From /speckit.specify
├── spec.md              # Feature spec
└── tasks.md             # Phase 2 output (/speckit.tasks — NOT created here)
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/nuget/
├── mod.rs               # Existing reader. This milestone adds:
│                        #   - build_nuget_main_module_entry() helper
│                        #     (mirrors build_cargo_main_module_entry
│                        #     at cargo.rs:504 and build_gem_main_module_entry
│                        #     at gem.rs:1055)
│                        #   - main-module edge population in the
│                        #     PackageDbEntry-return path
│                        #   - version-derivation ladder helper
│                        #     (Version → VersionPrefix+VersionSuffix →
│                        #     AssemblyVersion → generic-fallback)
├── packages_lock.rs     # Existing packages.lock.json parser. UNCHANGED —
│                        # the entry_type classification it already produces
│                        # is the source of truth for FR-004.
├── csproj.rs            # Existing .csproj/.vbproj/.fsproj parser. This
│                        # milestone READS the <Version>/<VersionPrefix>/
│                        # <VersionSuffix>/<AssemblyName>/<AssemblyVersion>
│                        # elements — the parser may or may not already
│                        # surface them (Phase 0 research determines).
└── msbuild_properties.rs # (New file already present in working tree;
                          # inspect during Phase 0 to determine role.)

waybill-cli/tests/       # Existing test surface. This milestone adds
                         # golden-comparison tests against the three
                         # pre-230 audit-fixture snapshots (RestSharp,
                         # Serilog, Orleans) at
                         # specs/audit-nuget-realworld/artifacts/, gated
                         # by SC-003's byte-parity requirement.
```

**Structure Decision**: In-place extension of the existing `waybill-cli/src/scan_fs/package_db/nuget/` module. No new crates, no new modules created (the `msbuild_properties.rs` file already exists in the working tree — Phase 0 research task R2 determines whether the version-derivation ladder consumes it directly or whether the ladder lives in `mod.rs`).

## Complexity Tracking

> No constitution violations to justify. Section intentionally empty.
