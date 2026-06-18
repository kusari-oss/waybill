---

description: "Task list for milestone 129 — Image-tier binary-extracted package readers"

---

# Tasks: Image-tier binary-extracted package readers (milestone 129)

**Input**: Design documents from `/specs/129-binary-tier-readers/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/{annotation-schema.md,reader-behavior.md}

**Tests**: Tests are REQUIRED for this milestone. The spec's User Story acceptance scenarios are
expressed in `Given/When/Then` form and the Success Criteria SC-001..006 are unit/integration-test
verifiable. Test tasks are therefore included alongside implementation tasks.

**Organization**: Tasks are grouped by user story (US1 P1, US2 P2, US3 P3) so each story can be
implemented, tested, and shipped independently as an MVP increment. The dedup pipeline (milestone 105)
makes the three readers' interactions emergent — no inter-story coordination required.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Add the three new files' stubs to the existing scan-orchestrator dispatcher so subsequent
tasks can be implemented and tested in isolation without orchestrator changes.

- [ ] T001 Create empty stub `mikebom-cli/src/scan_fs/package_db/dotnet/mod.rs` (declares `pub(crate) mod deps_json;`) and `mikebom-cli/src/scan_fs/package_db/dotnet/deps_json.rs` (empty `pub(crate) fn read_all(rootfs: &Path, exclusions: &ExclusionSet) -> Vec<PackageDbEntry> { vec![] }`); wire `pub(crate) mod dotnet;` into `mikebom-cli/src/scan_fs/package_db/mod.rs`.
- [ ] T002 [P] Create empty stub `mikebom-cli/src/scan_fs/binary/dotnet_pe.rs` (empty `pub(crate) fn read_all(rootfs: &Path, exclusions: &ExclusionSet) -> Vec<PackageDbEntry> { vec![] }`); wire `pub(crate) mod dotnet_pe;` into `mikebom-cli/src/scan_fs/binary/mod.rs`.
- [ ] T003 [P] Create empty stub `mikebom-cli/src/scan_fs/binary/cargo_auditable.rs` (same shape as T002); wire `pub(crate) mod cargo_auditable;` into `mikebom-cli/src/scan_fs/binary/mod.rs`.
- [ ] T004 Run `cargo +stable build -p mikebom` and confirm the workspace still builds clean with the stubs wired in. No behavior change expected (stubs return empty vecs).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Add the four new `SourceMechanism` enum variants + wire the stubs into the
`scan_fs::scan_path` dispatch + extend the milestone-105 dedup pipeline. After this phase, the three
user stories can proceed in parallel.

**⚠️ CRITICAL**: No user-story work begins until T009 is checkpointed.

- [ ] T005 Extend `SourceMechanism` enum in `mikebom-cli/src/scan_fs/dedup.rs` (or wherever the existing variants live — grep for `enum SourceMechanism`) with four new variants: `DotnetDepsJson`, `DotnetAssemblyMetadata`, `CargoAuditableBinary`, `MavenJarNested`. Update the existing `Display` + `FromStr` impls to round-trip the new variants. Add unit tests for the round-trip per the existing convention.
- [ ] T006 Extend the source-mechanism string constants used in `mikebom:source-mechanism` annotation emission. Verify via `grep -rn 'mikebom:source-mechanism' mikebom-cli/src/` that all annotation-emission sites pick up the new variants automatically.
- [ ] T007 [P] Wire `package_db::dotnet::deps_json::read_all` into the `package_db::read_all` dispatcher at `mikebom-cli/src/scan_fs/package_db/mod.rs`. The dispatcher's exact shape is "call each reader, collect Vec<PackageDbEntry>"; add a single call site for the dotnet reader. No behavior change yet (stub returns empty).
- [ ] T008 [P] Wire `binary::dotnet_pe::read_all` and `binary::cargo_auditable::read_all` into the `binary::read_all` dispatcher at `mikebom-cli/src/scan_fs/binary/mod.rs`. Same shape as T007.
- [ ] T009 Run `cargo +stable build -p mikebom && cargo +stable clippy --workspace --all-targets -- -D warnings` and confirm both are clean. The pre-PR gate target for this checkpoint.

**Checkpoint**: Foundation ready — US1/US2/US3 implementation can now begin in parallel.

---

## Phase 3: User Story 1 — .NET / NuGet from compiled assemblies (Priority: P1) 🎯 MVP

**Goal**: Fill the 1,489-package NuGet gap surfaced in the audit. Two-pronged reader: `.deps.json`
parser (the bulk) + PE/CLR managed-assembly metadata reader (fallback for assemblies without
`.deps.json` neighbors).

**Independent Test**: Run `mikebom sbom scan --image mcr.microsoft.com/dotnet/runtime:8.0-alpine` and
confirm the emitted SBOM contains `pkg:nuget/<name>@<version>` components for every entry in every
`.deps.json` AND for every CLR-tagged `.dll` not covered by a `.deps.json`. No
`mikebom:parse-failure` annotations on well-formed inputs.

### Reader A — `.deps.json` (US1)

- [ ] T010 [P] [US1] Create fixture directory `mikebom-cli/tests/fixtures/binary_tier_readers/dotnet_deps_json/` and add 5 synthetic `.deps.json` files covering both layouts per FR-012: (a) `per_app_layout/MyApp.deps.json` — 8 `package`-type entries + 1 `project`-type entry that must be skipped (per-application layout); (b) `runtime_store_layout/usr/share/dotnet/shared/Microsoft.NETCore.App/8.0.0/Microsoft.NETCore.App.deps.json` — 4 `package`-type entries laid out at the runtime-store path (verifies FR-012 layout-agnostic walk); (c) `project_type_skipped.deps.json` — single `project` entry → emits nothing; (d) `declared_not_installed.deps.json` — 1 `package` entry whose `path` field points at a file not present in the fixture; (e) `malformed_truncated.deps.json` — JSON parse error.
- [ ] T011 [US1] Implement `DotnetDepsJsonDocument`, `RuntimeTarget`, `DepsJsonKey`, `LibraryEntry`, `LibraryType` structs/enums in `mikebom-cli/src/scan_fs/package_db/dotnet/deps_json.rs` per data-model.md Entity 1. Include the custom `DepsJsonKey` `Deserialize` impl that fails-fast on malformed `name/version` keys.
- [ ] T012 [US1] Implement `read_all(rootfs, exclusions) -> Vec<PackageDbEntry>` in `deps_json.rs`: walks via `safe_walk` for `*.deps.json` extension, parses each, emits one `PackageDbEntry` per `LibraryType::Package` entry. Skips `Project` / `Referenceassembly` silently. Logs `warn` on parse failures. Per FR-002 + FR-006.
- [ ] T013 [US1] Implement `mikebom:image-presence = "declared-not-installed"` emission per edge case: when a `Package` entry's `path` field is non-empty AND the file at that path doesn't exist in the rootfs, set the annotation. Skip the check entirely when `path` is `None` (the .NET runtime store layout doesn't always populate it).
- [ ] T014 [US1] Add unit tests inside `deps_json.rs` for: well-formed parse (8 entries); `type:project` skip; `type:referenceassembly` skip; malformed key fail-fast; unknown `LibraryType` skip-with-warn; `runtime_target.name` round-trip.
- [ ] T015 [US1] Create `mikebom-cli/tests/binary_tier_us1_dotnet_deps_json.rs` integration test (mirrors the milestone-128 yocto US1 test scaffold): runs `mikebom sbom scan --path <fixture>` against each fixture; asserts emitted CDX has the expected component count + PURLs + annotations. Three acceptance scenarios per spec US1.

### Reader B — PE/CLR managed-assembly (US1)

- [ ] T016 [P] [US1] Create fixture directory `mikebom-cli/tests/fixtures/binary_tier_readers/dotnet_pe/` and add 3 hand-crafted minimal PE fixtures embedded as byte-array literals OR as committed binary files: `valid_clr.dll` (name=`Foo.Bar`, AssemblyVersion=1.2.3.4, FileVersion=1.2.3.5, InformationalVersion=`1.2.3-rc.1`); `native_dll_no_clr.dll` (PE with `DataDirectory[14]` zeroed); `stripped_assembly.dll` (CLR header present, metadata tables truncated). For the synthetic CLR fixtures, hand-craft via a `build.rs`-style fixture builder that emits the bytes at test-build time OR commit pre-built binaries with a README explaining their origin.
- [ ] T017 [US1] Implement `is_managed_assembly(pe: &PeFile) -> bool` in `mikebom-cli/src/scan_fs/binary/dotnet_pe.rs` per FR-010 + research R3 (`DataDirectory[14]` non-zero check). Returns `false` cheaply for native DLLs.
- [ ] T018 [US1] Implement the CLR metadata-table reader in `dotnet_pe.rs` per data-model.md Entity 2 + research R2: locate `#Strings` heap + `#~` tables stream → read `Assembly` table row 0 → extract `Name` (from `#Strings`), `MajorVersion`, `MinorVersion`, `BuildNumber`, `RevisionNumber`. Iterate `CustomAttribute` table for `AssemblyFileVersionAttribute` and `AssemblyInformationalVersionAttribute` rows; resolve their `Blob` heap references and parse the UTF-8 length-prefixed strings.
- [ ] T019 [US1] Implement `ManagedPeAssembly::purl_version()` ladder per FR-010 + clarification Q3: `informational → file → runtime-4-tuple`. Unit-test all three branches.
- [ ] T020 [US1] Implement `read_all(rootfs, exclusions) -> Vec<PackageDbEntry>` in `dotnet_pe.rs`: walks via `safe_walk` for `*.dll` extension; for each: parse via `object::read::pe::PeFile`, gate via `is_managed_assembly()`, extract metadata, emit one `PackageDbEntry` per managed assembly.
- [ ] T021 [US1] Add the 3 new annotations per FR-010: `mikebom:assembly-version-informational`, `mikebom:assembly-version-file`, `mikebom:assembly-version-runtime`. Emit always-3 when present (omit the annotation entirely if the corresponding metadata field is `None`).
- [ ] T022 [US1] Add unit tests inside `dotnet_pe.rs` for: managed-vs-native detection; metadata-table-row extraction; PURL-version ladder; all three custom-attribute fields populated; subset of attributes populated.
- [ ] T023 [US1] Create `mikebom-cli/tests/binary_tier_us1_dotnet_assembly_pe.rs` integration test: scans the three PE fixtures, asserts emitted CDX has 1 component for valid_clr.dll, 0 for native_dll_no_clr.dll (silent skip), 0 for stripped_assembly.dll (warn-log but no component).

### Cross-reader dedup (US1, FR-011)

- [ ] T024 [US1] Add a multi-reader integration test in `mikebom-cli/tests/binary_tier_us1_dotnet_deps_json.rs`: fixture where the same NuGet package is declared in BOTH `.deps.json` AND a sibling `.dll` with managed metadata. Assert exactly ONE emitted component with `mikebom:also-detected-via` listing both `dotnet-deps-json` AND `dotnet-assembly-metadata` source mechanisms. Verifies the milestone-105 dedup pipeline handles the new variants.

### Catalog + parity (US1)

- [ ] T025 [US1] Add 4 new C-rows (C87..C90 — final numbers may shift based on in-flight milestones; grep `docs/reference/sbom-format-mapping.md` for the highest current C-NN before pinning) to `docs/reference/sbom-format-mapping.md` per contracts/annotation-schema.md. Each row gets the full Principle V audit narrative.
- [ ] T026 [P] [US1] Register C87..C90 as `cdx_anno!` entries in `mikebom-cli/src/parity/extractors/cdx.rs`. Place immediately after the highest existing C-NN entry. Each is component-scope, SymmetricEqual.
- [ ] T027 [P] [US1] Register C87..C90 as `spdx23_anno!` entries in `mikebom-cli/src/parity/extractors/spdx2.rs`. Mirror T026's location.
- [ ] T028 [P] [US1] Register C87..C90 as `spdx3_anno!` entries in `mikebom-cli/src/parity/extractors/spdx3.rs`.
- [ ] T029 [US1] Register C87..C90 as `ParityExtractor` slice entries in `mikebom-cli/src/parity/extractors/mod.rs` with matching `use` statements imported from cdx/spdx2/spdx3. Verify the existing `extractors_table_is_sorted_by_row_id` + `every_catalog_row_has_an_extractor` shape tests still pass.

### US1 verification

- [ ] T030 [US1] Run `cargo +stable test -p mikebom --test binary_tier_us1_dotnet_deps_json --test binary_tier_us1_dotnet_assembly_pe` and confirm all acceptance scenarios pass.
- [ ] T031 [US1] Run the full quickstart Scenario 1: `mikebom sbom scan --image mcr.microsoft.com/dotnet/runtime:8.0-alpine` and `jq -r '.components[].purl' | grep -c '^pkg:nuget'` returns > 0.

**Checkpoint**: US1 fully functional and testable independently. SC-001 (NuGet count ≥1,415 on audit image) verifiable.

---

## Phase 4: User Story 2 — Rust crates via `cargo-auditable` ELF section (Priority: P2)

**Goal**: Fill the 928-package Cargo gap from `cargo auditable`-built binaries (uv, uvx, rustup-tier
tooling). Reads the `.dep-v0` ELF section per the upstream wire format.

**Independent Test**: Run `mikebom sbom scan --path <dir-containing-uv>` and confirm the emitted SBOM
contains `pkg:cargo/<crate>@<version>` components matching `rust-audit-info`'s parse of the same
binary.

- [ ] T032 [P] [US2] Create fixture directory `mikebom-cli/tests/fixtures/binary_tier_readers/cargo_auditable/`. For the synthetic ELFs, use a `build.rs`-style fixture builder that emits a minimal ELF with a `.dep-v0` section carrying a deterministic deflate-compressed JSON payload. Four fixtures: `elf_x86_64_with_dep_v0.elf` (10 packages, 1 root, 2 build-kind, 1 dev-kind); `elf_aarch64_with_dep_v0.elf` (5 packages); `elf_no_dep_v0.elf` (plain `cargo build` output; no section); `elf_malformed_dep_v0.elf` (deflate truncated mid-stream).
- [ ] T033 [US2] Implement `CargoAuditablePayload`, `CargoAuditablePackage`, `CargoAuditableSource`, `CargoAuditableKind` structs/enums in `mikebom-cli/src/scan_fs/binary/cargo_auditable.rs` per data-model.md Entity 3.
- [ ] T034 [US2] Implement `CargoAuditableKind::into_lifecycle_scope() -> LifecycleScope` per clarification Q1 + FR-017 correction in plan.md: `Runtime → Runtime`, `Build → Build`, `Dev → Test`. **Do NOT** emit a `mikebom:lifecycle-scope` annotation — route the scope value through `ResolvedComponent.lifecycle_scope` so the existing milestone-052 native emitters (CDX `scope`, SPDX 2.3 typed relationships, SPDX 3 `LifecycleScopeType`) produce the wire output. Verify via post-emission inspection of a fixture's emitted CDX.
- [ ] T035 [US2] Implement `find_dep_v0_section(elf: &ElfFile) -> Option<Vec<u8>>` helper: iterates `file.sections()`, finds the one named `.dep-v0`, returns its raw bytes. Returns `None` for ELFs without the section (the common case — silent skip per FR-020).
- [ ] T036 [US2] Implement `decode_payload(section_bytes: &[u8]) -> Result<CargoAuditablePayload>`: deflate-decompress via `flate2::read::DeflateDecoder` (RAW deflate, no gzip frame), parse via `serde_json::from_slice`. Map decompression / parse errors to `thiserror`-typed variants.
- [ ] T037 [US2] Implement `read_all(rootfs, exclusions) -> Vec<PackageDbEntry>` in `cargo_auditable.rs`: walks via `safe_walk` for files matching the ELF magic-byte sequence (reuse the milestone-096 `is_elf` helper from `symbol_fingerprint.rs`). For each ELF: call `find_dep_v0_section`, gate on `Some`, call `decode_payload`, gate on `Ok`. On payload success: emit one `PackageDbEntry` per `packages[]` entry with `lifecycle_scope` set from `kind.into_lifecycle_scope()`. On payload failure: log `warn` + skip.
- [ ] T038 [US2] Per FR-018: when `CargoAuditableSource::Local`, add the `mikebom:cargo-source-mechanism = "local-path"` annotation to the emitted component. This is finer-grained than what any native field expresses (it's about whether the crate is a path-dep vs registry-dep); document the Principle V audit in a code comment AND add a corresponding C-row to the catalog (C91 or next-available).
- [ ] T039 [US2] Add unit tests inside `cargo_auditable.rs` for: section discovery; deflate roundtrip; kind→lifecycle-scope mapping all three variants; source enum round-trip; root package handling.
- [ ] T040 [US2] Create `mikebom-cli/tests/binary_tier_us2_cargo_auditable.rs` integration test: scans the four ELF fixtures; asserts emitted CDX has 10 components for x86_64, 5 for aarch64, 0 for no_dep_v0, 0 (with warn-log) for malformed; verifies `lifecycle_scope` flows to native CDX `scope` field on the build-kind and dev-kind entries.
- [ ] T041 [US2] Register C91 (or whatever number is next available) for `mikebom:cargo-source-mechanism` in the parity catalog + the three extractor files + mod.rs. Same shape as T025..T029.

### US2 verification

- [ ] T042 [US2] Run `cargo +stable test -p mikebom --test binary_tier_us2_cargo_auditable` and confirm all acceptance scenarios pass.
- [ ] T043 [US2] Run the full quickstart Scenario 2 against `uv` and confirm `jq -r '.components[].purl' | grep -c '^pkg:cargo'` ≈ 200.

**Checkpoint**: US2 fully functional and testable independently. SC-002 (Cargo count ≥937 on audit image) verifiable.

---

## Phase 5: User Story 3 — Maven dependencies inside nested JARs (Priority: P3)

**Goal**: Fill the 300-package Maven gap from Spring Boot fat JARs / WARs / EARs. Extends the existing
milestone-009 reader with depth-bounded recursive archive descent.

**Independent Test**: Run `mikebom sbom scan --path <dir-containing-spring-boot-jar>` and confirm
the emitted SBOM contains `pkg:maven/...` components for every nested JAR's
`META-INF/maven/.../pom.properties`.

- [ ] T044 [P] [US3] Create fixture directory `mikebom-cli/tests/fixtures/binary_tier_readers/maven_nested_jar/`. Build the fixtures at test-build time via a `build.rs`-style helper that constructs ZIP archives in-memory: `spring_boot_uber.jar` (top-level + 5 nested `.jar` entries in `BOOT-INF/lib/`, each with `META-INF/maven/.../pom.properties`); `ear_war_jar_3_levels.ear` (EAR > WAR > JAR depth chain); `cycle.jar` (an archive that references its own SHA via a marker file — checks cycle detection); `zip_bomb.jar` (an entry declaring uncompressed size = 2 GB via a forged ZIP central-directory entry; must NOT extract); `corrupt_central_directory.jar` (malformed; reader must emit parse-failure annotation).
- [ ] T045 [US3] Implement `NestedArchiveWalker` struct in `mikebom-cli/src/scan_fs/package_db/maven/jar.rs` per data-model.md Entity 4: visited-set, depth counter, size cap, output accumulator, outer_path. Default values: `depth = 0`, `size_cap = 1 << 30` (1 GB).
- [ ] T046 [US3] Implement `walk_nested_archives(&mut self, archive_bytes: &[u8])`: SHA-256 of bytes via the existing milestone-038 `sha256_of` helper (or `sha2::Sha256` directly); cycle-detect via `visited.insert`; gate on `depth < 8`; open `zip::ZipArchive::new(Cursor::new(archive_bytes))`; iterate entries; for each `META-INF/maven/<group>/<artifact>/pom.properties`: emit a `PackageDbEntry`; for each entry with `.jar` / `.war` / `.ear` extension AND uncompressed_size ≤ 1 GB: extract bytes into a `Vec<u8>`, recurse with `self.depth += 1`. Restore `self.depth -= 1` on return.
- [ ] T047 [US3] Extension filter (FR-022, clarification Q2): match ONLY `.jar` / `.war` / `.ear` suffixes (case-insensitive). `.zip` MUST NOT trigger recursion. Add a unit test that asserts `.zip` entries are NOT descended into.
- [ ] T048 [US3] Per FR-025: per-entry uncompressed-size cap of 1 GB. Entries declaring `size() > 1 GB` are skipped with a single `warn` log naming the outer_path + entry name. Add a unit test using a synthetic forged-size archive.
- [ ] T049 [US3] Wire `walk_nested_archives` into the existing milestone-009 top-level reader at the per-JAR processing site. For each detected top-level JAR: instantiate a `NestedArchiveWalker`, call `walker.walk_nested_archives(&jar_bytes)`, drain `walker.out` into the reader's output `Vec<PackageDbEntry>`.
- [ ] T050 [US3] Tag every nested-archive-emitted `PackageDbEntry` with `mikebom:source-mechanism = "maven-jar-nested"` per FR-026; tag top-level entries with `"maven-jar"` (existing milestone-009 behavior — verify no regression).
- [ ] T051 [US3] Construct the `mikebom:source-files` annotation value for nested entries as `<outer-jar-path>!<nested-path>!<deeper-nested-path>...` (`!` separator matches the JAR-URL convention).
- [ ] T052 [US3] Add unit tests inside `jar.rs` for: SHA-256 cycle detection; depth limit; 1 GB size cap; extension filter; `!`-separator path construction.
- [ ] T053 [US3] Create `mikebom-cli/tests/binary_tier_us3_maven_nested_jar.rs` integration test: scans the five fixtures; asserts emitted CDX has the expected nested component counts; asserts cycle and depth-limit cases emit `warn` logs to stderr; asserts corrupt central directory emits `mikebom:parse-failure`.

### US3 verification

- [ ] T054 [US3] Run `cargo +stable test -p mikebom --test binary_tier_us3_maven_nested_jar` and confirm all acceptance scenarios pass.
- [ ] T055 [US3] Run the full quickstart Scenario 3 against `spring-petclinic`'s uber JAR; confirm `jq` shows ~50 `maven-jar-nested`-sourced components plus the existing `maven-jar` top-level entries.

**Checkpoint**: US3 fully functional. SC-003 (Maven count ≥335 on audit image) verifiable.

---

## Phase 6: Polish & Cross-Cutting Verification

**Purpose**: End-to-end SC verification, CHANGELOG, pre-PR gate, PR.

- [ ] T056 Run end-to-end SC-001/002/003 verification: `mikebom sbom scan --image 767397973649.dkr.ecr.us-east-1.amazonaws.com/remediation-planner:latest --output cyclonedx-json=/tmp/rp-129.cdx.json --root-name 767397973649.dkr.ecr.us-east-1.amazonaws.com/remediation-planner --offline` and assert per-ecosystem counts within the SC bounds (nuget ≥1,415; cargo ≥937; maven ≥335).
- [ ] T057 Run SC-007 weighted-score verification: `/Users/mlieberman/Projects/sbom-comparison/sbom-comparison --format summary /tmp/rp-129.cdx.json ~/Downloads/remediation-planner-syft-image-sbom.json` and assert weighted score ≥ 4.0 vs syft 2.6 (alpha.48 was 3.3).
- [ ] T058 Run SC-008 byte-identity verification: `./scripts/regen-goldens.sh && git status --short mikebom-cli/tests/fixtures/` produces zero `.cdx.json` / `.spdx.json` churn.
- [ ] T059 Run SC-009 performance verification: time the audit-image scan pre vs post; assert wall-clock growth <30%.
- [ ] T060 Update `CHANGELOG.md` `[Unreleased]` section with the milestone-129 entry: lead with the PURL gap closure (1,489 NuGet + 928 Cargo + 300 Maven), then break down by US1/US2/US3, then call out the 4 new mikebom:* annotation keys (C87..C91), then close with the SC verification results.
- [ ] T061 Run the pre-PR gate: `./scripts/pre-pr.sh` and confirm `>>> all pre-PR checks passed.` Fix any clippy lints surfaced.
- [ ] T062 Commit + push the `129-binary-tier-readers` branch.
- [ ] T063 Open PR via `gh pr create` with the summary referencing the audit findings + the milestone-129 PR template (mirroring the milestone-128 PR shape).
- [ ] T064 Create `mikebom-cli/tests/offline_mode_audit_ecosystem_129.rs` mirroring the milestone-107/108 offline-audit pattern: grep the four new modules (`scan_fs/package_db/dotnet/`, `scan_fs/binary/dotnet_pe.rs`, `scan_fs/binary/cargo_auditable.rs`, `scan_fs/package_db/maven/jar.rs` nested-walk additions) for `reqwest::`, `std::process::Command`, `tokio::net::` — assert zero occurrences. Per FR-004 verification.

---

## Dependency Graph

```text
Phase 1 (Setup) ───→ Phase 2 (Foundational) ───┬──→ Phase 3 (US1, P1) ──┐
                                               │                         ├──→ Phase 6 (Polish)
                                               ├──→ Phase 4 (US2, P2) ──┤
                                               │                         │
                                               └──→ Phase 5 (US3, P3) ──┘
```

Phases 3, 4, 5 are **independent** — after Phase 2's checkpoint, all three user stories can be
implemented and merged in any order, including parallel work on separate branches that rebase onto a
shared milestone-129 root. The dedup pipeline (milestone 105) handles interactions emergently; the
test fixtures don't overlap.

## Parallel Execution Opportunities

**Within Phase 1**: T002 + T003 in parallel (different files, no dependencies between them).

**Within Phase 2**: T007 + T008 in parallel after T005/T006 complete.

**Within Phase 3 (US1)**:

- T010 (fixture) + T016 (fixture) in parallel — different directories.
- T011..T015 (deps_json track) + T017..T023 (PE/CLR track) can proceed on parallel mini-tracks within
  one developer's queue OR on parallel branches.
- T026 + T027 + T028 in parallel — three separate extractor files, all independent edits.

**Within Phase 4 (US2)**: most tasks are sequential within the single file `cargo_auditable.rs`;
T032 (fixtures) runs in parallel with the implementation prep.

**Within Phase 5 (US3)**: T044 (fixtures) runs in parallel with the implementation prep; the rest is
sequential (extends one file).

**Across Phases 3/4/5**: after Phase 2 checkpoint, the three user stories run fully in parallel.

## Implementation Strategy

**MVP scope**: US1 alone. The .NET gap is the single largest reputation-shaping limitation surfaced by
the audit; closing it (1,489 packages) moves mikebom's completeness score from 1/5 to ~3/5 on the
audit image. US2 and US3 are increments on top — each independently testable, each independently
shippable as a follow-up PR if the milestone is split.

**Recommended cadence**: ship all three in one PR. The fixtures and tests are tightly scoped (the
synthetic byte-array fixtures keep test data <500 KB total), the implementation work is bounded
(<2 KLOC across all three readers), and the dedup pipeline integration is uniform. Splitting introduces
golden-regen overhead per PR without commensurate review benefit.

**Risk**: T018 (CLR metadata-table reader hand-roll) is the most complex single task — ECMA-335 §II.22
table-layout parsing is well-documented but tedious. Time-box at 1 day; if it slips, the fallback is
to add the `pelite = "0.10"` dep (which would cost zero behavioral fidelity but burn the "zero new
Cargo deps" plan goal). Document the time-box decision in tasks.md as a comment if the fallback is
exercised.
