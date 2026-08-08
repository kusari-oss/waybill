---

description: "Task list for feature 230-nuget-main-module: NuGet main-module component + root→direct dependency edges"
---

# Tasks: NuGet main-module component + root→direct dependency edges

**Input**: Design documents from `/specs/230-nuget-main-module/`
**Prerequisites**: `plan.md`, `spec.md`, `research.md`, `data-model.md`, `contracts/nuget-main-module-shape.md`, `quickstart.md`

**Tests**: Included. Milestone follows the m064 / m216 pattern of unit tests colocated with the reader plus one integration test asserting byte-parity + edge-coverage against the pre-230 audit fixture.

**Organization**: Grouped by user story. US1 (locked, P1) is the MVP; US2 (unlocked, P2) piles on top without changing US1 semantics. Polish covers golden regeneration + audit-doc refresh.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: `[US1]`, `[US2]` — maps back to spec.md's user stories
- File paths are absolute or repo-relative; every task cites the exact file it touches

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Ensure the branch is ready and the workspace builds cleanly before touching the NuGet reader. No new dependencies to add per plan.md.

- [X] T001 Verify feature branch is `230-nuget-main-module` (per `git branch --show-current`) and that `./scripts/pre-pr.sh` exits 0 against the untouched tree — locks in the pre-change green baseline that FR-006 / SC-003 compare against.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Land the three helpers both US1 and US2 need (version-derivation ladder, AssemblyName resolution, main-module `PackageDbEntry` builder). None of these are user-visible in isolation; they're the substrate the story phases wire into `read()`.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete. All three helpers are on the same file (`waybill-cli/src/scan_fs/package_db/nuget/mod.rs`) so are NOT parallelizable to each other, but T002–T004 land as three sequential commits on the same PR.

- [X] T002 Add helper `fn resolve_main_module_version(project_path: &Path, property_map: &msbuild_properties::PropertyMap) -> String` in `waybill-cli/src/scan_fs/package_db/nuget/mod.rs`. Implements the FR-010 ladder: read `<Version>` from the parsed `.csproj`/`.vbproj`/`.fsproj`, run through `msbuild_properties::substitute_and_check` against `property_map`; if unresolved or empty, try `<VersionPrefix>` (+ `<VersionSuffix>` joined with `-` if present); if still unresolved, try `<AssemblyVersion>`; if none resolve, return `String::new()` (caller falls back to `pkg:generic/*@0.0.0` shape).
- [X] T003 Add helper `fn resolve_main_module_name(project_path: &Path, property_map: &msbuild_properties::PropertyMap) -> String` in `waybill-cli/src/scan_fs/package_db/nuget/mod.rs`. Reads `<AssemblyName>` from the parsed project file; runs through `msbuild_properties::substitute`; returns the resolved value. On empty/unresolved, returns the project filename stem (via `project_path.file_stem()`). Matches research R3 + R5.
- [X] T004 Add helper `fn build_nuget_main_module_entry(project_path: &Path, property_map: &msbuild_properties::PropertyMap, depends: Vec<String>, tier: &str) -> Option<PackageDbEntry>` in `waybill-cli/src/scan_fs/package_db/nuget/mod.rs`. Constructs the main-module PURL (`pkg:nuget/<AssemblyName>@<version>` or `pkg:generic/<project-stem>@0.0.0` fallback), populates `extra_annotations["waybill:component-role"] = "main-module"`, sets `sbom_tier: Some(tier.to_string())` (caller passes `"source"` for both US1 and US2 per data-model.md), leaves other fields at struct defaults matching cargo m064's shape at `cargo.rs:557-587`. Returns `None` when both PURL construction paths fail (unreachable in practice; matches m064 defensive-None convention).

**Checkpoint**: Reader has the three helpers but doesn't call any of them yet. Workspace still builds; existing NuGet tests still pass unchanged (the helpers are dead code until US1 wires the call site in).

---

## Phase 3: User Story 1 — Locked NuGet project (Priority: P1) 🎯 MVP

**Goal**: For every `.csproj`/`.vbproj`/`.fsproj` that has a co-located `packages.lock.json`, emit a main-module component whose `depends` list contains every lockfile entry typed `Direct` or `CentralTransitive`. Package-level component detection remains byte-identical to pre-230.

**Independent Test**: Scan `specs/audit-nuget-realworld/fixtures/restsharp` (real fixture). Assert (a) ≥1 main-module component per project file; (b) every pre-230 NuGet package-level component with lockfile `entry_type ∈ {Direct, CentralTransitive}` has ≥1 incoming edge from a main-module ref; (c) the pre-230 NuGet package-component PURL set is byte-identical to the post-230 set (via sorted PURL diff, empty result).

### Tests for User Story 1

- [X] T005 [P] [US1] Add unit test `main_module_edges_from_lockfile_direct` in `waybill-cli/src/scan_fs/package_db/nuget/mod.rs` `mod tests` block. Fixture: one `.csproj` (`<Version>1.0.0</Version>`, `<PackageReference Include="MikebomFixture.SampleLib" />`) + `packages.lock.json` with `MikebomFixture.SampleLib` typed `Direct` at version `1.2.3`. Assert: `read()` returns 2 entries — package `pkg:nuget/MikebomFixture.SampleLib@1.2.3` (unchanged), plus main-module `pkg:nuget/App@1.0.0` whose `depends` contains `"MikebomFixture.SampleLib"`. Follow the fixture-naming convention in memory `feedback_fixture_synthetic_package_names` (real coordinates trip Kusari Inspector).
- [X] T006 [P] [US1] Add unit test `main_module_edges_from_lockfile_central_transitive` in same test block. Fixture: leaf `.csproj` (versionless `<PackageReference Include="MikebomFixture.SharedLib" />`) + root `Directory.Packages.props` declaring `<PackageVersion Include="MikebomFixture.SharedLib" Version="5.6.7" />` + `packages.lock.json` with entry_type `CentralTransitive`. Assert main-module's `depends` includes `"MikebomFixture.SharedLib"`; the resolved package PURL `pkg:nuget/MikebomFixture.SharedLib@5.6.7` is the edge target after `scan_fs/mod.rs`'s `name_to_purl` resolution.
- [X] T007 [P] [US1] Add unit test `main_module_excludes_transitive_entries` in same test block. Fixture: `packages.lock.json` with `MikebomFixture.OnlyTransitive` typed `Transitive`. Assert the main-module's `depends` does NOT contain `"MikebomFixture.OnlyTransitive"` (it's still emitted as a package-level component per existing behavior, but the main-module doesn't point at it).
- [X] T008 [P] [US1] Add unit test `main_module_multi_tfm_union` in same test block. Fixture: `packages.lock.json` with a package `MikebomFixture.OnlyNet8` under `net8.0` framework block only (`Direct`) and `MikebomFixture.Shared` under both `net6.0` and `net8.0` (`Direct` both). Assert main-module's `depends` is `["MikebomFixture.OnlyNet8", "MikebomFixture.Shared"]` — union with dedup by name per FR-009.
- [X] T009 [P] [US1] Add unit test `main_module_assembly_name_override` in same test block. Fixture: `App.csproj` declaring `<AssemblyName>Contoso.Framework</AssemblyName>` and `<Version>2.0.0</Version>`. Assert main-module PURL is `pkg:nuget/Contoso.Framework@2.0.0`, NOT `pkg:nuget/App@2.0.0`.
- [X] T010 [P] [US1] Add unit test `main_module_version_ladder_falls_through_to_generic` in same test block. Fixture: `App.csproj` with no `<Version>`, no `<VersionPrefix>`, no `<AssemblyVersion>`. Assert main-module PURL is `pkg:generic/App@0.0.0` (fallback per FR-003 + FR-010).
- [X] T011 [P] [US1] Add integration test `nuget_main_module_parity.rs` at `waybill-cli/tests/nuget_main_module_parity.rs`. Loads `specs/audit-nuget-realworld/fixtures/restsharp` via the existing corpus-harness pattern from milestone-083. Runs the scanner. Asserts SC-002 (16/16 NuGet package-level components have ≥1 incoming edge) and SC-003 (post-230 NuGet package-PURL set is a strict superset of pre-230's, and every pre-230 PURL is preserved). Also asserts SC-004 (graph-completeness annotation no longer contains `multi-ecosystem-partial-root: nuget`).

### Implementation for User Story 1

- [X] T012 [US1] Extend `waybill-cli/src/scan_fs/package_db/nuget/mod.rs`'s `read()` function (~line 210+) to iterate over discovered project files AFTER the existing `acc` accumulation loop. For each project file with a companion `packages.lock.json`, extract every lockfile-entry name whose `entry_type` is `Direct` or `CentralTransitive` across every framework block (union with dedup by name per FR-009 + data-model.md). Assemble the merged MSBuild property map (csproj-local ∪ ancestor `Directory.Packages.props` chain — reuses the existing walker the reader already uses for package-version resolution).
- [X] T013 [US1] In the same `read()` function, call `build_nuget_main_module_entry(project_path, &property_map, direct_names, "source")` for each project file that produced a non-empty `Direct`/`CentralTransitive` name list OR that has a `packages.lock.json` at all (empty `depends` is a valid main-module — represents a project with no direct deps). Push the returned entry to `out`. Preserve struct-default behavior for the entry's non-annotation fields.
- [X] T014 [US1] Run `cargo +stable test -p waybill nuget::` locally. Every T005–T010 unit test passes green; every pre-existing NuGet test continues passing (no regressions from Phase 2 helpers now going live).

**Checkpoint**: RestSharp fixture: `jq` queries in `quickstart.md` §SC-001 + §SC-002 both return the expected shapes. Byte-parity: `diff -u /tmp/pre230.purls /tmp/post230.purls` produces empty output per quickstart §SC-003. `waybill:graph-completeness-reason` no longer flags nuget per quickstart §SC-004.

---

## Phase 4: User Story 2 — Unlocked NuGet project design-tier fallback (Priority: P2)

**Goal**: When a project file has no companion `packages.lock.json`, populate the main-module's `depends` from the `<PackageReference Include="...">` items declared in the project file (or resolved via CPM from `Directory.Packages.props`). Distinguished from US1 only by data provenance; consumer-facing wire shape is identical.

**Independent Test**: Create a scratch project (matching `quickstart.md` §US2 walkthrough) that declares `<PackageReference Include="MikebomFixture.SampleLib" Version="1.2.3" />` and has NO `packages.lock.json`. Assert the main-module `pkg:nuget/App@1.0.0` exists AND has an incoming edge from `MikebomFixture.SampleLib`.

### Tests for User Story 2

- [X] T015 [P] [US2] Add unit test `main_module_unlocked_derives_from_package_reference` in `waybill-cli/src/scan_fs/package_db/nuget/mod.rs` `mod tests` block. Fixture: `App.csproj` with `<Version>1.0.0</Version>` and `<PackageReference Include="MikebomFixture.SampleLib" Version="1.2.3" />`, NO `packages.lock.json`. Assert main-module's `depends` contains `"MikebomFixture.SampleLib"` and resolves to `pkg:nuget/MikebomFixture.SampleLib@1.2.3`.
- [X] T016 [P] [US2] Add unit test `main_module_mixed_locked_and_unlocked_solution` in same test block. Fixture: two `.csproj` files — one with a `packages.lock.json` (Direct entry X), one without (`<PackageReference>` Y). Assert both main-modules exist; the locked project's main-module points at X; the unlocked project's main-module points at Y; no crossover.
- [X] T017 [P] [US2] Add unit test `main_module_unlocked_cpm_versionless` in same test block. Fixture: leaf `App.csproj` with versionless `<PackageReference Include="MikebomFixture.SharedLib" />` + root `Directory.Packages.props` with `<PackageVersion Include="MikebomFixture.SharedLib" Version="5.6.7" />`, NO `packages.lock.json`. Assert the main-module points at the CPM-resolved `pkg:nuget/MikebomFixture.SharedLib@5.6.7`.

### Implementation for User Story 2

- [X] T018 [US2] In `waybill-cli/src/scan_fs/package_db/nuget/mod.rs`'s `read()`, extend the main-module emission loop from T012–T013 to handle the no-lockfile case: when `packages.lock.json` is absent for a project, iterate the parsed `<PackageReference>` items from `csproj::parse` (or equivalent parser output already in scope) and use their `Include` names as the main-module's `depends`. Pass tier `"source"` to `build_nuget_main_module_entry` — the design-tier signal is carried by the (existing) package-level components' `sbom_tier: "design"` marker at `nuget/mod.rs:369-382`, not by the main-module itself.
- [X] T019 [US2] Run `cargo +stable test -p waybill nuget::` locally. T015–T017 pass green; T005–T014 continue passing.

**Checkpoint**: The `quickstart.md` §US2 walkthrough produces a main-module and an incoming edge on Newtonsoft.Json. Both locked and unlocked shapes coexist without interference in the same scan.

---

## Phase 5: Polish & Cross-Cutting Concerns

**Purpose**: Regenerate audit-fixture goldens (they now include main-modules and additional edges — pre-230 goldens are the byte-parity input, post-230 goldens are the new baseline), refresh the audit doc, and run the full pre-PR gate.

- [ ] T020 [P] Regenerate `specs/audit-nuget-realworld/artifacts/restsharp.waybill.cdx.json`, `serilog.waybill.cdx.json`, and `orleans.waybill.cdx.json` by scanning each corresponding fixture with the milestone-230 binary. Commit the new goldens as the post-230 audit baseline; the pre-230 versions live in git history for future regression comparison.
- [X] T021 [P] Append a "Post-m230 update" section to `docs/audits/2026-08-04-nuget-realworld.md`: summarize the closed gap (main-modules + root→direct edges), reference the RestSharp before/after edge-coverage delta (0/16 → 16/16 incoming), note ProjectReference→ProjectReference edges remain deferred (FR-007).
- [ ] T022 [P] Add a `waybill-common` / `waybill-cli` CHANGELOG-style entry or milestone-log entry per project convention (search for how m226 or m216 recorded their milestone-log entry — the naming convention has been consistent since milestone 001).
- [X] T023 Run `./scripts/pre-pr.sh` locally — expect green (clippy + full workspace tests). Per memory `feedback_prepr_gate_bails_on_first_failure`, if it fails, enumerate every `^---- .+ stdout ----` line in the failure output before triaging.
- [X] T024 Walk through `specs/230-nuget-main-module/quickstart.md` end-to-end against a fresh `cargo build --release -p waybill` binary. Confirm every SC-001..SC-005 assertion returns the expected shape. Any deviation → back to Phase 3 or Phase 4.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: T001 has no code dependencies.
- **Phase 2 (Foundational)**: T002 → T003 → T004 (same file; sequential edits). T004 requires T002 + T003 to be complete because it consumes their outputs.
- **Phase 3 (US1)**: Requires Phase 2 complete. Within US1, tests T005–T011 are all in different code paths (T005–T010 in `mod.rs::tests`, T011 in a new integration-test file) and can be authored in parallel. Implementation tasks T012 → T013 → T014 are sequential (T013 reads T012's setup; T014 verifies both).
- **Phase 4 (US2)**: Requires Phase 3 complete (US2 extends the US1 emission loop). Tests T015–T017 parallelizable; implementation T018 → T019 sequential.
- **Phase 5 (Polish)**: Requires Phase 4 complete. T020, T021, T022 parallelizable (different files). T023, T024 sequential — pre-PR gate before end-to-end walkthrough.

### User Story Dependencies

- **US1 (P1)**: Depends only on Phase 2. Delivers the MVP: locked NuGet solutions get main-modules + edges. RestSharp fixture proves the fix.
- **US2 (P2)**: Depends on US1 (extends the same emission loop). Not independent — but its acceptance test (unlocked scratch project) doesn't touch US1's fixture, so US1's regression signal stays clean.

### Parallel Opportunities

- All Phase 3 test tasks (T005–T011) — different unit-test names or different file. Parallelizable.
- All Phase 4 test tasks (T015–T017) — different unit-test names. Parallelizable.
- Polish tasks T020, T021, T022 — different files. Parallelizable.

---

## Implementation Strategy

### MVP First (US1 Only)

1. Complete Phase 1 setup (T001).
2. Complete Phase 2 foundational (T002 → T003 → T004).
3. Complete Phase 3 US1 tests (T005–T011, parallelizable) — expect them to fail (implementation hasn't landed yet).
4. Complete Phase 3 US1 implementation (T012 → T013 → T014). Tests turn green.
5. **STOP + VALIDATE**: RestSharp fixture flips 0/16 → 16/16 incoming. Byte-parity holds.
6. Ship a PR at this point — a viable milestone by itself.

### Incremental Delivery

1. Setup + Foundational + US1 → MVP PR (as above).
2. US2 tests (T015–T017) + implementation (T018–T019) → follow-up commit or same PR.
3. Polish tasks — golden regeneration + audit doc → same PR.
4. Pre-PR gate + quickstart walkthrough (T023 + T024) → final green before opening PR.

### Not a parallel-team milestone

Small enough (single reader module, ~150 LOC net addition) that one contributor completes it linearly. No team-parallelization needed.

---

## Notes

- Every task cites a concrete file path per the format requirement. Test tasks additionally name the test function so the checklist item maps to a specific `#[test] fn <name>` block.
- The unit tests live in the existing `mod.rs::tests` block per m064/m216 convention — do NOT create a new `tests/` subdirectory inside `nuget/`.
- The integration test at `waybill-cli/tests/nuget_main_module_parity.rs` is a NEW file — no equivalent exists today.
- Fixture package names use the `MikebomFixture.*` synthetic-package-name convention per memory `feedback_fixture_synthetic_package_names`. Real NuGet coordinates trip the Kusari Inspector advisory scan.
- Byte-parity verification in T011 + T024 uses the sorted-PURL-diff pattern documented in memory `feedback_verify_golden_churn_normalized`.
- No new Cargo dependencies; no `Cargo.toml` edits.
- No `scan_fs/mod.rs` changes required (verified in research R1) — the reader's `depends: Vec<String>` output already flows through the shared edge-emission loop.
