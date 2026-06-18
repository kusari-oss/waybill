# Implementation Plan: Image-tier binary-extracted package readers

**Branch**: `129-binary-tier-readers` | **Date**: 2026-06-18 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/129-binary-tier-readers/spec.md`

## Summary

Three new image-tier readers extend mikebom's `sbom scan` polyglot-pipeline (the user-space scanner — NOT the eBPF
trace pipeline, which is governed separately by Principles II/III) to extract package coordinates from compiled
artifacts where source manifests are absent:

1. **`.deps.json` + PE/CLR managed-assembly metadata reader** (US1, P1) — fills the 1,489-package NuGet gap surfaced
   by the audit against `remediation-planner:latest`. New `scan_fs/package_db/dotnet/` module + `scan_fs/binary/dotnet_pe.rs`.
2. **`cargo-auditable` `.dep-v0` ELF section reader** (US2, P2) — fills the 928-package Cargo gap from
   `cargo auditable`-built Rust binaries (uv, uvx, rustup-tier tooling). New `scan_fs/binary/cargo_auditable.rs`.
3. **Maven nested-JAR recursion** (US3, P3) — extends the existing milestone-009 JAR reader with depth-bounded
   archive descent for Spring Boot uber JARs / fat JARs / WARs / EARs. ~300-package coverage gain.

All three readers reuse the existing milestone-105 `SourceMechanism`-based dedup pipeline (collisions with source-tier
findings merge with `mikebom:also-detected-via`), the milestone-114 `safe_walk` filesystem helper, the milestone-113
`--exclude-path` flag, the milestone-097 `mikebom:cpe-candidates` annotation channel, and the milestone-052 native
typed-edge lifecycle-scope model (CDX `scope` + SPDX 2.3 `DEV/BUILD/TEST_DEPENDENCY_OF` + SPDX 3 `LifecycleScopeType`)
— no new `mikebom:lifecycle-scope` annotation is introduced. **Zero new Cargo dependencies.**

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–128; no nightly required).

**Primary Dependencies**: Existing only — `object = "0.36"` (workspace; ELF section reading for `.dep-v0` per
milestones 096/098/099 + PE COFF + CLR `COR20_HEADER` parsing — `object` 0.36 has a `pe::ImageDataDirectory`
accessor for the COM Descriptor data directory), `zip` (already a direct dep at `mikebom-cli/Cargo.toml` per the
milestone-009 maven JAR reader; reused for nested-JAR descent), `serde`/`serde_json` (`.deps.json` + cargo-auditable
JSON payloads), `flate2 = "1"` (workspace; deflate-decompress for `.dep-v0` section content per the cargo-auditable
v0 wire spec), `tracing` (warn/debug logs per FR-006), `anyhow`/`thiserror`. **No new Cargo dependencies.**

The CLR metadata-table parser is small enough to hand-roll on top of `object`'s PE primitives (the table format is
documented in ECMA-335 §II.22). No `pelite` or `clrmetadata` crate added — the wire format we care about
(AssemblyName + AssemblyVersion + AssemblyFileVersion + AssemblyInformationalVersion) is a tightly-scoped slice
covered by ~150 LOC of straight-line code, less code than would be needed to wire an external crate cleanly.

**Storage**: N/A — all state in-process per scan; no caches, no persistence. Matches every milestone since 002.

**Testing**: Standard `cargo +stable test --workspace`. Four new integration test files (one per reader path)
+ unit tests inside each new module. Synthetic fixtures vendored in
`mikebom-cli/tests/fixtures/binary_tier_readers/` per the milestone-128 "stay-set" rule (small synthetic-shape
inputs stay in the main repo, not the sibling fixture-cache repo).

**Target Platform**: Linux rootfs (`mikebom sbom scan --image` always targets a Linux container per the existing
`--image-platform` constraint). The PE/CLR reader operates on `.dll` files **inside** a Linux container's rootfs
(e.g. `/usr/share/dotnet/sdk/8.0.127/...`); mikebom does not target Windows hosts itself.

**Project Type**: CLI tool (the `mikebom sbom scan` polyglot-scanner pipeline). NOT the eBPF trace pipeline.

**Performance Goals**:
- Per-binary cargo-auditable parse: <100ms for `uv` (50 MB ELF, ~200 crates in `.dep-v0`).
- Per-`.deps.json` parse: <50ms for a typical file (~50 library entries).
- Per-PE/CLR assembly parse: <20ms (small fixed-size metadata table read; no JIT, no method body parsing).
- Total scan time growth on the audit image: <30% relative to alpha.48 (measured against
  `767397973649.dkr.ecr.us-east-1.amazonaws.com/remediation-planner:latest`).

**Constraints**:
- Zero new Cargo dependencies (verified via `cargo tree -p mikebom`).
- Byte-identity preservation across the 33 committed alpha.48 goldens (verified via `./scripts/regen-goldens.sh`
  producing zero `.cdx.json` / `.spdx.json` churn).
- All three readers MUST respect `--offline` (no network), `--exclude-path` (milestone 113), and `safe_walk` paths
  (milestone 114).
- Per-nested-archive decompressed-size cap: 1 GB (FR-025).
- Nested-archive depth limit: 8 levels (FR-021, matching milestone-128 `INCLUDE_DEPTH_LIMIT`).

**Scale/Scope**:
- ~1,500 new `pkg:nuget` components emitted per typical .NET-bearing image.
- ~1,000 new `pkg:cargo` components emitted per cargo-auditable-tool-bearing image.
- ~300 new `pkg:maven` components emitted per Spring Boot fat JAR.
- 5 new `mikebom:*` annotation keys catalogued + parity-extracted (C-row range C87..C91 expected, but final
  range depends on whether any other milestone in flight lands first):
  - `mikebom:assembly-version-informational` (US1)
  - `mikebom:assembly-version-file` (US1)
  - `mikebom:assembly-version-runtime` (US1)
  - `mikebom:image-presence` (US1 edge case for `.deps.json` declared-but-not-installed entries)
  - `mikebom:cargo-source-mechanism` (US2, FR-018; emitted only when cargo-auditable `source: "local"`)
- Native fields used instead of `mikebom:*` for lifecycle scope (FR-017): CDX `scope` + SPDX 2.3 typed
  `DEV_DEPENDENCY_OF` / `BUILD_DEPENDENCY_OF` / `TEST_DEPENDENCY_OF` + SPDX 3 `LifecycleScopeType` per
  the existing milestone-052 model. **No new `mikebom:lifecycle-scope` annotation introduced** (corrected
  from the spec's FR-017 phrasing during this plan's Principle V audit — see Constitution Check below).

## Constitution Check

Audit against `mikebom Constitution v1.4.0` (`.specify/memory/constitution.md`):

| Principle | Verdict | Notes |
|---|---|---|
| I. Pure Rust, Zero C | ✓ Pass | All new code Rust; zero new transitive C deps (`object`, `zip`, `flate2`, `serde_json` all pure Rust). |
| II. eBPF-Only Observation | ✓ N/A | This milestone targets the `mikebom sbom scan` polyglot pipeline (user-space FS scanner). Principle II governs the SEPARATE `mikebom trace` eBPF pipeline. The two pipelines have always been distinct since milestone 002. |
| III. Fail Closed | ✓ Pass | Parse failures emit a single `warn`-level log + a `mikebom:parse-failure` component-scope annotation; scan continues on sibling files (FR-006). No silent omission; no fallback to a different reader path on failure. |
| IV. Type-Driven Correctness | ✓ Pass | All new components flow through `mikebom_common::types::purl::Purl` (PURL validation at construction time). `.deps.json` parsed via `serde_derive` with `LibraryEntry { r#type: LibraryType }` typed enum (no stringly-typed dispatch). No `.unwrap()` in production code — `anyhow` for application errors, `thiserror` for module-internal error variants. |
| V. Specification Compliance | ⚠ One correction required | **FR-017 audit finding**: The spec's FR-017 introduces `mikebom:lifecycle-scope` annotations for cargo-auditable `kind: "build"` and `kind: "dev"` entries. Principle V's standards-native-precedence rule rejects this — milestone 052 already migrated mikebom to CDX `scope` + SPDX 2.3 typed relationship types (`DEV_DEPENDENCY_OF`, `BUILD_DEPENDENCY_OF`, `TEST_DEPENDENCY_OF`) + SPDX 3 `LifecycleScopeType`, all of which carry the same semantic natively. The plan's implementation MUST route cargo-auditable `kind` values through the existing `ResolvedComponent.lifecycle_scope` field (which drives native emission), not through a new annotation. FR-017 in the spec should be re-read as a behavioral spec ("the lifecycle scope MUST reflect the cargo-auditable kind") rather than a wire-format spec ("emit a mikebom:lifecycle-scope annotation"). |
| | | **FR-007/010 audits**: `mikebom:image-presence` (declared-but-not-installed) and the three `mikebom:assembly-version-*` (informational / file / runtime) annotations are valid parity-bridging extensions per the "finer-grained information the standard does not express" carve-out. Neither CDX nor SPDX have an "installed vs declared" component-status field; neither has multi-version preservation (`version` / `versionInfo` are single-valued). The four new keys MUST be catalogued in `docs/reference/sbom-format-mapping.md` with full Principle V audit narratives per the milestone-128 convention. |
| | | **Cross-cutting**: All new components emit valid PURLs (`pkg:nuget/<name>@<version>`, `pkg:cargo/<name>@<version>`, `pkg:maven/<group>/<artifact>@<version>`) via `Purl::new`. CDX 1.6 + SPDX 2.3 + SPDX 3 emissions all flow through the existing format-specific builders unchanged. |
| VI. Three-Crate Architecture | ✓ Pass | All new code lives in `mikebom-cli`. No new crate created. |
| VII. Test Isolation | ✓ Pass | All new tests run in unprivileged user-space (no kernel privilege needed). The fixtures are synthetic input artifacts (small `.deps.json` JSON files, a hand-crafted minimal CLR DLL, a hand-crafted minimal cargo-auditable ELF with deflate-compressed JSON in a section, a hand-crafted minimal fat JAR). The PE / ELF fixtures are byte-arrays embedded as `include_bytes!` from `tests/fixtures/binary_tier_readers/`. |
| VIII. Completeness | ✓ Pass | Every well-formed `.deps.json` library entry, every well-formed cargo-auditable package, every well-formed nested JAR's `pom.properties` is surfaced. The Principle X transparency annotations cover the parse-failure / depth-limit / cycle-detect cases. |
| IX. Accuracy | ✓ Pass | All emitted components have PURLs derived directly from the source artifact (no heuristic / probabilistic matching). The dedup pipeline (milestone 105) prevents double-counting collisions. |
| X. Transparency | ✓ Pass | Parse failures surface via `mikebom:parse-failure`; depth-limit hits via `mikebom:nested-archive-depth-limit-reached` (existing annotation); declared-but-not-installed via the new `mikebom:image-presence`. All use the existing CDX `property` channel. |
| XI. Enrichment | ✓ Pass | CPE candidates per FR-013, license expressions per the nested-JAR `MANIFEST.MF` parse path, all flow through existing enrichment channels. |
| XII. External Data Source Enrichment | ✓ N/A | No external data sources consulted. `--offline` is honored (FR-004). |

**Strict Boundaries**:

1. **No lockfile-based dependency discovery** — N/A; this is the `mikebom sbom scan` pipeline, which is the
   deliberate polyglot-scanner path. Principle II / Strict Boundary #1 govern the SEPARATE eBPF `mikebom trace`
   pipeline.
2. **No MITM proxy** — ✓ N/A.
3. **No C code** — ✓ Zero new C deps (verified via `cargo tree`).
4. **No `.unwrap()` in production** — ✓ All new modules carry the standard
   `#[cfg_attr(test, allow(clippy::unwrap_used))]` guard on `mod tests` per the existing crate-root deny.

**Gate verdict**: PASS with one **correction folded into the plan** (FR-017 spec wording is loose; plan
implementation MUST use the existing native typed-edge mechanism, NOT a new `mikebom:lifecycle-scope`
annotation). The spec text is not strictly wrong — it talks about "lifecycle-scope" semantics — but the example
key name implies a wire-format choice the constitution rejects. Tasks.md will route via the native mechanism.

## Project Structure

### Documentation (this feature)

```text
specs/129-binary-tier-readers/
├── plan.md              # This file
├── spec.md              # Feature spec (already exists)
├── research.md          # Phase 0 output (this command)
├── data-model.md        # Phase 1 output (this command)
├── quickstart.md        # Phase 1 output (this command)
├── contracts/           # Phase 1 output (this command)
│   ├── annotation-schema.md     # The 4 new mikebom:* keys per Principle V
│   └── reader-behavior.md       # Per-reader input/output behavior contract
├── checklists/
│   └── requirements.md  # Spec-quality checklist (already exists from /speckit-specify)
└── tasks.md             # Phase 2 output (/speckit-tasks; NOT created by this command)
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   ├── scan_fs/
│   │   ├── binary/
│   │   │   ├── cargo_auditable.rs   # NEW — US2: `.dep-v0` ELF section reader
│   │   │   ├── dotnet_pe.rs         # NEW — US1: PE/CLR managed-assembly metadata reader
│   │   │   └── mod.rs               # Wires the two new submodules into the existing binary-tier dispatcher
│   │   └── package_db/
│   │       ├── dotnet/              # NEW MODULE — US1: `.deps.json` reader
│   │       │   ├── deps_json.rs     # Top-level `.deps.json` parser
│   │       │   ├── runtime_target.rs # Optional: `.NETCoreApp,Version=v8.0` parser
│   │       │   └── mod.rs           # Module entry; wires into package_db dispatcher
│   │       ├── maven/               # EXISTING (milestone 009) — extended for nested-JAR recursion
│   │       │   └── jar.rs           # Extended with `walk_nested_archives(zip_reader, depth)` per FR-021..026
│   │       └── mod.rs               # Comment updated to mention milestone-129 wiring
│   ├── parity/
│   │   └── extractors/
│   │       ├── cdx.rs               # +4 cdx_anno! entries (C87..C90)
│   │       ├── spdx2.rs             # +4 spdx23_anno! entries
│   │       ├── spdx3.rs             # +4 spdx3_anno! entries
│   │       └── mod.rs               # +4 ParityExtractor slice entries + matching `use` statements
│   └── generate/                    # Native CDX `scope` + SPDX typed relationships
│                                    # — unchanged; the existing milestone-052 emission paths handle
│                                    # cargo-auditable kind values via `ResolvedComponent.lifecycle_scope`
└── tests/
    ├── binary_tier_us1_dotnet_deps_json.rs    # NEW — US1 .deps.json acceptance scenarios
    ├── binary_tier_us1_dotnet_assembly_pe.rs  # NEW — US1 PE/CLR managed-assembly acceptance scenarios
    ├── binary_tier_us2_cargo_auditable.rs     # NEW — US2 acceptance scenarios
    ├── binary_tier_us3_maven_nested_jar.rs    # NEW — US3 nested-JAR acceptance scenarios
    └── fixtures/binary_tier_readers/
        ├── dotnet_deps_json/                  # Synthetic .deps.json files (well-formed + malformed)
        │   ├── well_formed_8_libraries.deps.json
        │   ├── project_type_skipped.deps.json
        │   ├── declared_not_installed.deps.json
        │   └── malformed_truncated.deps.json
        ├── dotnet_pe/                          # Minimal hand-crafted CLR DLL fixtures
        │   ├── valid_clr.dll                  # name=Foo.Bar version=1.2.3.4 + Info=1.2.3-rc.1
        │   ├── native_dll_no_clr.dll          # Win32 native DLL; reader must skip
        │   └── stripped_assembly.dll          # CLR header present but metadata stripped
        ├── cargo_auditable/                    # Minimal hand-crafted ELFs with .dep-v0 sections
        │   ├── elf_x86_64_with_dep_v0.elf
        │   ├── elf_aarch64_with_dep_v0.elf
        │   ├── elf_no_dep_v0.elf              # Vanilla cargo build output; reader must skip silently
        │   └── elf_malformed_dep_v0.elf       # Truncated deflate payload
        └── maven_nested_jar/                   # Synthetic fat JARs
            ├── spring_boot_uber.jar           # 5 nested JARs in BOOT-INF/lib/
            ├── ear_war_jar_3_levels.ear       # EAR > WAR > JAR depth chain
            ├── cycle.jar                      # Self-referencing nested archive (cycle-detect target)
            ├── zip_bomb.jar                   # Decompressed > 1 GB (size-cap target)
            └── corrupt_central_directory.jar  # Malformed; reader emits parse-failure annotation

docs/reference/sbom-format-mapping.md          # +4 C-rows (C87..C90) with Principle V audit narratives

CHANGELOG.md                                   # +1 milestone-129 entry under [Unreleased]
```

**Structure Decision**: Three reader paths, three locations:

- **`scan_fs/binary/`** (US2 cargo-auditable + US1 PE/CLR) — these readers operate on individual binary
  files (ELF / PE), so they live under the binary subsystem alongside milestones 096/098/099 binary
  fingerprint readers. Both consume `object`'s parsed file abstraction.
- **`scan_fs/package_db/dotnet/`** (US1 `.deps.json`) — `.deps.json` is a manifest-style file (JSON
  sidecar to assemblies), so it lives under `package_db/` alongside source-tier readers (cargo, npm,
  maven, etc.). The fact that it's emitted by the dotnet TOOLCHAIN at build time (rather than authored
  by a developer) is incidental — the wire format is a declarative manifest.
- **`scan_fs/package_db/maven/`** (US3 nested-JAR) — extends the existing milestone-009 reader. No new
  module; the change is a `walk_nested_archives(...)` recursive helper added to `jar.rs`.

The two-location split for US1 (binary reader for PE/CLR + manifest reader for `.deps.json`) mirrors syft's
own pattern (`dotnet-deps-binary-cataloger` + `dotnet-portable-executable-cataloger`). The dedup pipeline
(milestone 105) merges collisions cleanly.

## Phase 0: Outline & Research

See [research.md](./research.md) (generated by this command).

Research topics covered:

1. `.deps.json` wire format — the `libraries` map, `targets` map (sometimes the only place version
   information is, depending on dotnet-publish style), `runtimeTarget.name` (`.NETCoreApp,Version=vN.M`),
   `type: "package"` vs `type: "project"` vs `type: "referenceassembly"`.
2. CLR managed-assembly metadata table layout (ECMA-335 §II.22) — `#Strings` heap, `Assembly` table
   row layout, the `CustomAttribute` table where `AssemblyFileVersionAttribute` +
   `AssemblyInformationalVersionAttribute` are stored (their values live in `#Strings` keyed by
   attribute-constructor signature).
3. PE COFF + CLR header layout — `optional_header.data_directories[14]` is `IMAGE_DIRECTORY_ENTRY_COM_DESCRIPTOR`;
   when present and non-zero, the PE is a managed assembly.
4. cargo-auditable v0 wire format — `.dep-v0` ELF section name, raw deflate-compressed JSON payload,
   schema (`{packages: [{name, version, source, kind, dependencies, root}]}`).
5. cargo-auditable `kind` field semantics — `"runtime"` (default) → no scope tag; `"build"` →
   milestone-052 `lifecycle_scope = Build`; `"dev"` → milestone-052 `lifecycle_scope = Test` (per
   clarification Q1).
6. Nested-archive walk — how the existing milestone-009 maven JAR reader is structured (single-level only),
   what `zip::ZipArchive::read_from_seek` requires (the inner archive bytes must implement
   `Read + Seek`; a `Cursor<Vec<u8>>` over the extracted nested-archive entry bytes works), depth-limit
   + cycle-detection patterns from milestone-128's include-chain resolver.
7. Audit-image probe — how syft's `dotnet-deps-binary-cataloger` enumerates `.deps.json` files
   (a `walkdir` + extension match); what the typical `.deps.json` file size looks like (~20-200 KB).
8. SPDX 3 + CDX native typed-edge model — milestone-052's `lifecycle_scope` field on
   `ResolvedComponent` → CDX `scope` + SPDX 2.3 typed `DEV_DEPENDENCY_OF` etc. + SPDX 3
   `LifecycleScopeType`. Confirms FR-017 spec wording is loose; the implementation routes via the
   existing native channel.

## Phase 1: Design & Contracts

See [data-model.md](./data-model.md) and [contracts/](./contracts/).

Phase 1 artifacts:

- **data-model.md**: 4 entities — `DotnetDepsJsonDocument`, `ManagedPeAssembly`, `CargoAuditablePayload`,
  `NestedArchive`. Each entity defines its parsed-representation Rust struct, validation rules per FR-NNN
  references, and its `From` conversions into the existing `PackageDbEntry` / `ResolvedComponent` types.
- **contracts/annotation-schema.md**: The 4 new `mikebom:*` keys (informational/file/runtime version
  triples + `image-presence`) with Principle V audit narratives — to land in
  `docs/reference/sbom-format-mapping.md` as C-rows C87..C90.
- **contracts/reader-behavior.md**: Per-reader I/O contract — what the reader receives (a file path,
  the file bytes), what it returns (`Vec<PackageDbEntry>` or `Result<...>`), what it logs at each
  level (`debug` per silent skip, `warn` per parse failure), what side-effect-free invariants it
  upholds (offline, exclude-path-aware, safe_walk-routed).
- **quickstart.md**: A one-page operator-facing description of "scan a .NET image" / "scan a Rust
  binary" / "scan a Spring Boot fat JAR" with the expected output shape — used both as documentation
  and as the integration-test golden.

### Agent context update

After Phase 1, the plan invokes `.specify/scripts/bash/update-agent-context.sh claude` which appends a
milestone-129 entry to `/Users/mlieberman/Projects/mikebom/CLAUDE.md`'s "Recent Changes" tail and "Active
Technologies" list. Per CLAUDE.md's preservation rules, only the milestone-129 lines are added; no
existing content is rewritten.

## Complexity Tracking

> Filled because Constitution Check surfaced one correction (FR-017 wire-format wording).

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| FR-017's `mikebom:lifecycle-scope` wire-format wording (spec text) | The spec author wanted to express semantic intent — that cargo-auditable's `kind` field controls lifecycle scope — but used annotation-shaped phrasing. | Plan implementation routes via the existing milestone-052 native typed-edge mechanism (CDX `scope` + SPDX 2.3 typed relationship types + SPDX 3 `LifecycleScopeType`). No new annotation is emitted. The spec text is treated as a behavioral spec; the wire-format claim is corrected in the plan and propagated to tasks.md via T0NN ("FR-017 implementation MUST set `ResolvedComponent.lifecycle_scope` per the `kind` mapping; MUST NOT emit `mikebom:lifecycle-scope`"). |
