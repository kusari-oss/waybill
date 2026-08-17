---
description: "Task list for m236 — universalize waybill:unresolved-reason"
---

# Tasks: Universalize `waybill:unresolved-reason` per-component annotation

**Input**: Design documents from `/specs/236-unresolved-reason/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/annotation-wire.md, contracts/per-reader-strings.md

**Tests**: Per-reader unit tests + one cross-reader integration test + one blacklist scan. Tests are explicitly required by FR-006, FR-007, FR-009, FR-010, SC-001–SC-005.

**Organization**: Tasks are grouped by user story (US1 top-5 ecosystems / US2 JVM+tool / US3 long-tail) so each story ships as an independently-testable MVP increment. Setup + Foundational phases block all user stories.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Parallelizable — different files, no dependencies on incomplete tasks
- **[Story]**: US1 / US2 / US3 — maps to spec.md user stories

## Path Conventions

Single-crate change in `waybill-cli/`. All paths relative to repo root.

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Locate existing baseline + verify catalog-row state (research R3).

- [X] T001 Grep completed. Found scope drift: cargo, gem, kotlin_dsl/mod, npm/mod DO NOT emit design-tier. Spec updated (Q2 clarification) — scope trimmed to 13 emitting files.

- [X] T002 R3 branch B: no existing row for `waybill:unresolved-reason`. C151 assigned as the new row number.

- [X] T003 Fixture root created at `waybill-cli/tests/fixtures/golden_inputs/unresolved_reason/` with README.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Register the parity extractor + catalog row so cross-format assertions can run in Phase 3+.

- [X] T004 NuGet baseline captured — 12 nuget/mod tests pass on main pre-m236.

- [X] T005 C151 CDX extractor added at `waybill-cli/src/parity/extractors/cdx.rs`.

- [X] T006 C151 SPDX 2.3 extractor added at `waybill-cli/src/parity/extractors/spdx2.rs`.

- [X] T007 C151 SPDX 3 extractor added at `waybill-cli/src/parity/extractors/spdx3.rs`.

- [X] T008 EXTRACTORS array registered with C151 in `waybill-cli/src/parity/extractors/mod.rs`.

- [X] T009 C151 catalog row landed in `docs/reference/sbom-format-mapping.md` (above C150).

- [X] T010 `every_catalog_row_has_an_extractor` parity gate PASSES with C151 registered.

**Checkpoint**: After Phase 2 completes, parity plumbing is ready; every US1/US2/US3 emission will flow through the new extractor.

---

## Phase 3: User Story 1 — Top-5 ecosystems (Priority: P1) 🎯 MVP

**Story goal**: Every design-tier component from cargo / gem / maven / npm (both call-sites) / pip carries the reader-specific reason string per `contracts/per-reader-strings.md`.

**Independent test criteria**: For each of the 6 reader files, a per-reader unit test asserts the exact reason string on a temp-dir fixture. A US1-scoped subset of the cross-reader integration test (Phase 6 T041) asserts each reader's fixture produces a design-tier component carrying the annotation.

### Fixture creation (T011–T016)

- [X] T011 [SKIP] cargo does not emit design-tier today (T001 grep). Dropped per Q2 scope trim. Recorded in `spec.md` Clarifications.

- [X] T012 [SKIP] gem does not emit design-tier today. Dropped per Q2 scope trim.

- [ ] T013 [P] [US1] Create `waybill-cli/tests/fixtures/golden_inputs/unresolved_reason/maven/` with a `pom.xml` declaring one `<dependency>` (synthetic coord) without a `<version>` element (or with a version placeholder like `${my.version}` that doesn't resolve) — MUST hit maven's design-tier path.

- [X] T014 [SKIP] npm/mod does not emit design-tier today (T001 grep — only source-tier). Dropped per Q2 scope trim.

- [ ] T015 [P] [US1] Create `waybill-cli/tests/fixtures/golden_inputs/unresolved_reason/npm_workspace/` with a `package.json` declaring `workspaces: ["packages/*"]` + a `packages/foo/package.json` (workspace member) with no lockfile-resolved version. Should trigger `npm/walk.rs` workspace-member design-tier path.

- [ ] T016 [P] [US1] Create `waybill-cli/tests/fixtures/golden_inputs/unresolved_reason/pip/` with a `requirements.txt` listing one entry without a version specifier (e.g., `waybill-fixture-pip-dep` with no `==X.Y`) and no `uv.lock` / `poetry.lock`.

### Reader modification + inline unit test (T017–T022)

- [X] T017 [SKIP] cargo dropped from scope.

- [X] T018 [SKIP] gem dropped from scope.

- [X] T019 [US1] maven annotation injected + unit test `m236_maven_design_tier_carries_unresolved_reason` passing. Variable-driven emission handled at the shared PackageDbEntry construction.

- [X] T020 [SKIP] npm/mod dropped from scope.

- [X] T021 [US1] npm/walk annotation injected + unit test `m236_npm_walk_design_tier_carries_unresolved_reason` passing.

- [X] T022 [US1] pip/requirements_txt annotation injected + unit tests `m236_pip_design_tier_carries_unresolved_reason` and `m236_pip_source_tier_does_not_carry_unresolved_reason` (FR-004 negative check) both passing.

### US1 checkpoint

- [X] T023 [US1] 4/4 m236 unit tests pass (maven, npm/walk, pip design, pip source — the pip source-tier absence test adds FR-004 verification as a bonus).

**Checkpoint**: US1 (MVP) is shippable at this point — the top-5 ecosystems all carry the annotation with their locked reason strings.

---

## Phase 4: User Story 2 — JVM + tool ecosystems (Priority: P2)

**Story goal**: kotlin_dsl (2 call-sites) + scala + gradle_static + helm + yocto carry their reader-specific reason strings.

**Independent test criteria**: 6 per-reader unit tests assert the exact strings.

### Fixture creation (T024–T029)

- [X] T024 [SKIP] kotlin_dsl/mod dropped from scope (T001 grep — only source-tier emission).

- [ ] T025 [P] [US2] Create `waybill-cli/tests/fixtures/golden_inputs/unresolved_reason/kotlin_dsl_buildscript/` with a `build.gradle.kts` declaring a `buildscript { dependencies { classpath(...) } }` block. Also scanned with `--include-declared-deps`.

- [ ] T026 [P] [US2] Create `waybill-cli/tests/fixtures/golden_inputs/unresolved_reason/scala/` with a `build.sbt` declaring one dep (synthetic coord) without a coursier-resolved lockfile.

- [ ] T027 [P] [US2] Create `waybill-cli/tests/fixtures/golden_inputs/unresolved_reason/gradle_static/` with a `build.gradle` (Groovy DSL) declaring one dep + no lockfile + no cache. Reuses m235 static parser design-tier path.

- [ ] T028 [P] [US2] Create `waybill-cli/tests/fixtures/golden_inputs/unresolved_reason/helm/` with a `Chart.yaml` declaring one dependency (synthetic name) — scanned without `--helm-render` to hit the unrendered-dependency design-tier path.

- [ ] T029 [P] [US2] Create `waybill-cli/tests/fixtures/golden_inputs/unresolved_reason/yocto/` with a `.bb` recipe declaring PN + PV placeholders that don't resolve.

### Reader modification + inline unit test (T030–T035)

- [X] T030 [SKIP] kotlin_dsl/mod dropped from scope.

- [ ] T031 [US2] Modify `waybill-cli/src/scan_fs/package_db/kotlin_dsl/build_script.rs`: insert annotation with value `"Kotlin DSL buildscript declaration; --include-declared-deps enables emission"`. Add unit test `m236_kotlin_dsl_buildscript_design_tier_carries_unresolved_reason`.

- [ ] T032 [US2] Modify `waybill-cli/src/scan_fs/package_db/scala.rs`: insert annotation with value `"declared in build.sbt; no coursier-resolved lockfile"`. Add unit test `m236_scala_design_tier_carries_unresolved_reason`.

- [ ] T033 [US2] Modify `waybill-cli/src/scan_fs/package_db/gradle/static_parser.rs`: insert annotation with value `"declared in build.gradle; US2 cache reader had no matching seed"`. Add unit test `m236_gradle_static_design_tier_carries_unresolved_reason`.

- [ ] T034 [US2] Modify `waybill-cli/src/scan_fs/package_db/helm.rs`: insert annotation with value `"unrendered Chart.yaml dependency; --helm-render subprocess disabled or unavailable"`. Add unit test `m236_helm_design_tier_carries_unresolved_reason`.

- [ ] T035 [US2] Modify `waybill-cli/src/scan_fs/package_db/yocto/recipe.rs`: insert annotation with value `"recipe .bb declaration; no PV/PR resolution"`. Add unit test `m236_yocto_design_tier_carries_unresolved_reason`.

### US2 checkpoint

- [ ] T036 [US2] Run `cargo +stable test -p waybill --bin waybill m236_kotlin_dsl m236_scala m236_gradle_static m236_helm m236_yocto` and confirm 5/5 US2 unit tests pass (kotlin_dsl_buildscript + scala + gradle_static + helm + yocto — 6→5 after Q2 scope trim). Combined with US1: 8/8 tests pass.

---

## Phase 5: User Story 3 — Long-tail ecosystems (Priority: P3)

**Story goal**: cocoapods + composer + dart + elixir + erlang + haskell + pants_shell + pants_go carry their reader-specific reason strings.

### Fixture creation (T037–T044)

- [ ] T037 [P] [US3] Create `waybill-cli/tests/fixtures/golden_inputs/unresolved_reason/cocoapods/` with a `Podfile` declaring one pod (synthetic name) and no `Podfile.lock`.

- [ ] T038 [P] [US3] Create `waybill-cli/tests/fixtures/golden_inputs/unresolved_reason/composer/` with a `composer.json` declaring one dep and no `composer.lock`.

- [ ] T039 [P] [US3] Create `waybill-cli/tests/fixtures/golden_inputs/unresolved_reason/dart/` with a `pubspec.yaml` declaring one dep and no `pubspec.lock`.

- [ ] T040 [P] [US3] Create `waybill-cli/tests/fixtures/golden_inputs/unresolved_reason/elixir/` with an `mix.exs` declaring one dep and no `mix.lock`.

- [ ] T041 [P] [US3] Create `waybill-cli/tests/fixtures/golden_inputs/unresolved_reason/erlang/` with a `rebar.config` declaring one dep and no `rebar.lock`.

- [ ] T042 [P] [US3] Create `waybill-cli/tests/fixtures/golden_inputs/unresolved_reason/haskell/` with a `stack.yaml` or `.cabal` declaring one dep with no lockfile.

- [ ] T043 [P] [US3] Create `waybill-cli/tests/fixtures/golden_inputs/unresolved_reason/pants_shell/` with a `pants.toml` `[shellcheck]` (or similar) block declaring a tool without a version specifier + a BUILD file that references it.

- [ ] T044 [P] [US3] Create `waybill-cli/tests/fixtures/golden_inputs/unresolved_reason/pants_go/` with a `pants.toml` `[golang] expected_version = "..."` block and a Go project that produces no matching Go corpus component.

### Reader modification + inline unit test (T045–T052)

- [ ] T045 [US3] Modify `waybill-cli/src/scan_fs/package_db/cocoapods.rs`: insert annotation with value `"no matching entry in Podfile.lock"`. Add unit test `m236_cocoapods_design_tier_carries_unresolved_reason`.

- [ ] T046 [US3] Modify `waybill-cli/src/scan_fs/package_db/composer.rs`: insert annotation with value `"no matching entry in composer.lock"`. Add unit test `m236_composer_design_tier_carries_unresolved_reason`.

- [ ] T047 [US3] Modify `waybill-cli/src/scan_fs/package_db/dart.rs`: insert annotation with value `"no matching entry in pubspec.lock"`. Add unit test `m236_dart_design_tier_carries_unresolved_reason`.

- [ ] T048 [US3] Modify `waybill-cli/src/scan_fs/package_db/elixir.rs`: insert annotation with value `"no matching entry in mix.lock"`. Add unit test `m236_elixir_design_tier_carries_unresolved_reason`.

- [ ] T049 [US3] Modify `waybill-cli/src/scan_fs/package_db/erlang.rs`: insert annotation with value `"no matching entry in rebar.lock"`. Add unit test `m236_erlang_design_tier_carries_unresolved_reason`.

- [ ] T050 [US3] Modify `waybill-cli/src/scan_fs/package_db/haskell.rs`: insert annotation with value `"declared in stack.yaml / .cabal; no stack.yaml.lock fallback"`. Add unit test `m236_haskell_design_tier_carries_unresolved_reason`.

- [ ] T051 [US3] Modify `waybill-cli/src/scan_fs/package_db/pants_shell/component_emit.rs`: insert annotation with value `"pants shell tool pin without version specifier"`. Add unit test `m236_pants_shell_design_tier_carries_unresolved_reason`.

- [ ] T052 [US3] Modify `waybill-cli/src/scan_fs/package_db/pants_go/mod.rs`: insert annotation with value `"pants_go expected_version declared; no matching go corpus component"`. Add unit test `m236_pants_go_design_tier_carries_unresolved_reason`.

### US3 checkpoint

- [ ] T053 [US3] Run `cargo +stable test -p waybill --bin waybill m236_cocoapods m236_composer m236_dart m236_elixir m236_erlang m236_haskell m236_pants` and confirm 8/8 US3 unit tests pass. Combined with US1 + US2: 20/20 unit tests pass.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Cross-reader integration test (SC-001), byte-identity regression guard (SC-003), source-tier absence assertion (SC-004), blacklist scan (FR-010), docs.

### Cross-reader coverage tests (T054–T057)

- [ ] T054 Create `waybill-cli/tests/unresolved_reason_universal.rs` implementing the cross-reader integration test per SC-001: scan a temp directory populated with copies of all 20 fixtures (`unresolved_reason/<reader>/`) side-by-side, assert every emitted design-tier component in the resulting CDX carries a non-empty `waybill:unresolved-reason` annotation. Test name: `sc001_all_readers_design_tier_carry_unresolved_reason`.

- [ ] T055 [P] Add SC-002 cross-format parity assertion to `unresolved_reason_universal.rs`: for the same fixture corpus, scan once for CDX, once for SPDX 2.3, once for SPDX 3; assert every design-tier component from every reader carries byte-identical annotation values across all three formats. Test name: `sc002_annotation_cross_format_parity`.

- [ ] T056 [P] Add SC-003 NuGet byte-identity regression to `unresolved_reason_universal.rs`: reuse the existing NuGet fixture (from the PR-#656-era test suite) or create a minimal one; assert the emitted string is exactly `"no Version= on <PackageReference>, no CPM entry in Directory.Packages.props, no packages.lock.json entry"` byte-for-byte. Test name: `sc003_nuget_wire_value_unchanged`.

- [ ] T057 [P] Add SC-004 source-tier absence assertion to `unresolved_reason_universal.rs`: use a mixed-ecosystem fixture that produces both source-tier and design-tier components (e.g., a cargo project WITH Cargo.lock resolving to source-tier). Assert every source-tier component in the emitted SBOM does NOT carry `waybill:unresolved-reason`. Test name: `sc004_source_tier_components_absent_annotation`.

### Blacklist + docs (T058–T060)

- [ ] T058 [P] Add FR-010 blacklist scan to `unresolved_reason_universal.rs`: enumerate every `waybill:unresolved-reason` value in the fixture-corpus scan output; assert no value contains any of `/`, `\`, `~`, `@`, `password=`, `token=`, `api_key=`, `Bearer `, `192.168.`, `.com`, `.net`, `.org`. Test name: `fr010_reason_strings_no_pii_paths_credentials`. Failure output MUST print the offending reader + string.

- [ ] T059 [P] Update `docs/reference/sbom-format-mapping.md` to include the finalized per-reader reason-string enumeration from `contracts/per-reader-strings.md`. Cross-reference to that contract file.

- [ ] T060 [P] Update `docs/ecosystems.md` §4 (design-tier subsection) to note that all 18 readers now emit `waybill:unresolved-reason` uniformly. Remove the m227-era caveat that downstream tools "should treat annotation ABSENCE as 'no reason provided'"; replace with the stronger promise that the annotation is present on every design-tier component.

### Close-out (T061–T063)

- [ ] T061 Run pre-PR gate: `./scripts/pre-pr.sh`. MUST pass clean (zero clippy warnings, every suite green).

- [ ] T062 Open PR titled `feat(m236): universalize waybill:unresolved-reason across all 18 design-tier readers (closes #659)`. Body includes: summary linking spec + plan + tasks + issue #659; per-reader table listing the shipped reason strings (from contracts/per-reader-strings.md); test plan checklist covering per-reader unit tests + SC-001..SC-005 + FR-010 blacklist; deferred section (none — this milestone closes the cross-reader gap completely).

- [ ] T063 Add spec close-out note to `specs/236-unresolved-reason/spec.md` under a new `## Close-out (post-implementation)` section (per m235 tradition): (a) list of covered readers with confirmed strings; (b) link to merged PR; (c) SC verification pass/fail per SC. Add `memory/reference_unresolved_reason.md` auto-memory entry linking the milestone's SoT paths + docs cross-reference.

---

## Dependencies

**Phase order** (blocking):

1. Phase 1 (Setup) → Phase 2 (Foundational) → Phase 3+ (User Stories in parallel-optional order)
2. Phase 2 blocks all user stories (parity extractor must exist before per-reader tests can flow through it)
3. Phase 6 (Polish) requires all US1 + US2 + US3 tasks complete (T054's fixture-corpus scan needs every fixture)

**Task-level dependencies**:

- T008 depends on T005 + T006 + T007 (extractor one-liners must exist before EXTRACTORS registration)
- T010 depends on T008 + T009 (parity gate needs both extractor + docs row)
- T017–T022 (US1 readers) depend on T010 (parity infra ready)
- T030–T035 (US2 readers) depend on T010
- T045–T052 (US3 readers) depend on T010
- T054 depends on ALL of T017–T022 + T030–T035 + T045–T052 (needs every reader shipping to run the cross-corpus scan)

**Within each user story**, fixture creation ([P] tasks with T0xx numbers early in the story) can run in parallel with each other, but reader modification tasks (T0xx numbers later in the story) MUST land AFTER their corresponding fixture task (the unit test in the modification task reads the fixture).

## Parallel execution examples

### Setup phase (Phase 1) — sequential

Runs in order: T001 (grep) → T002 (docs check) → T003 (mkdir).

### Foundational phase (Phase 2)

T005, T006, T007 are [P] (different files). Batch them:

```bash
# Round 1: parallel extractor one-liners
task T005 & task T006 & task T007
wait

# Round 2: sequential registration
task T008
task T009
task T010
```

### Per-user-story fixture batch

All fixture-creation tasks within a story are [P]. E.g., US1 fixtures:

```bash
task T011 & task T012 & task T013 & task T014 & task T015 & task T016
wait
```

### Per-user-story reader modification batch

Reader modifications within a story are all in distinct files, so all [P] within the story:

- US1: T017, T018, T019, T020, T021, T022 all in parallel
- US2: T030, T031, T032, T033, T034, T035 all in parallel
- US3: T045–T052 all in parallel

## Implementation strategy — MVP scope

**MVP = Phase 1 + Phase 2 + Phase 3 (US1)**

Ships:
- 6 reader modifications covering the top-5 ecosystems (cargo, gem, maven, npm×2, pip)
- Parity extractor + catalog row
- 6 per-reader unit tests
- The MVP is independently shippable: US1 fixtures + unit tests validate the pattern end-to-end; the cross-reader integration test at T054 can be scoped to just US1 fixtures for MVP verification.

**Incremental delivery**:

- **PR 1 (MVP)**: T001–T023 (Setup + Foundational + US1). ~23 tasks, ~150 LOC.
- **PR 2 (US2)**: T024–T036. ~13 tasks, ~120 LOC.
- **PR 3 (US3)**: T037–T053. ~17 tasks, ~160 LOC.
- **PR 4 (Polish)**: T054–T063. ~10 tasks, ~250 LOC (cross-reader test + docs).

## Task summary

| Phase | Count | Purpose |
|---|---|---|
| Phase 1 Setup | 3 | Locate baseline + fixture-root dir |
| Phase 2 Foundational | 7 | Parity extractor triple + registration + catalog row + docs row + gate verify |
| Phase 3 US1 (P1) | 13 | 6 fixtures + 6 reader mods with inline unit tests + checkpoint |
| Phase 4 US2 (P2) | 13 | 6 fixtures + 6 reader mods with inline unit tests + checkpoint |
| Phase 5 US3 (P3) | 17 | 8 fixtures + 8 reader mods with inline unit tests + checkpoint |
| Phase 6 Polish | 10 | 4 integration tests + 1 blacklist test + 2 docs + close-out |
| **Total** | **63** | |

## Format validation

- ✅ Every task starts with `- [ ]` markdown checkbox
- ✅ Every task has a sequential T-ID (T001–T063)
- ✅ Every task in Phase 3+ has a `[US1]` / `[US2]` / `[US3]` label
- ✅ Every setup + foundational + polish task has NO story label (correct)
- ✅ Parallelizable tasks marked `[P]`
- ✅ Every task includes an exact file path
