# Tasks: The scanned project's declared license reaches its SBOM

**Input**: Design documents from `/specs/1010-manifest-declared-license/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/license-extraction.md

**Tests**: Included. Constitution Principle VII requires unit coverage, and each user
story in spec.md defines an Independent Test.

**Organization**: Grouped by user story. US1 is the MVP and is deliverable with a
single ecosystem implemented.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1 / US2 / US3 per spec.md

## Path Conventions

Three-crate Cargo workspace. Readers live in
`waybill-cli/src/scan_fs/package_db/`; integration tests in `waybill-cli/tests/`;
unit tests inline in `#[cfg(test)]` modules beside the code.

> **Clippy note**: `waybill-cli` denies `clippy::unwrap_used` workspace-wide,
> including in `#[cfg(test)]`. Every new test module needs
> `#[cfg_attr(test, allow(clippy::unwrap_used))]`, matching the existing
> convention. `--all-targets` will fail otherwise.

---

## Phase 1: Setup (Evidence Completion)

**Purpose**: close the six contract rows still lacking evidence before any code depends on
them. Phase 0 found two of its own assumptions wrong by checking; these rows are
unchecked, so implementing from them would be building on the same kind of guess.

- [X] T001 [P] Verify the elixir license key and multi-license semantics against the Hex package-metadata documentation, and update the elixir row in `specs/1010-manifest-declared-license/contracts/license-extraction.md` with the source URL
- [X] T002 [P] Verify the erlang license key (`.app.src` vs `rebar.config`) and multi-license semantics against the rebar3 documentation, and update the erlang row in `specs/1010-manifest-declared-license/contracts/license-extraction.md`
- [X] T003 [P] Verify the scala `licenses` setting shape and semantics against the sbt reference, and update the scala row in `specs/1010-manifest-declared-license/contracts/license-extraction.md`
- [X] T004 [P] Verify the cocoapods `license` attribute (string vs hash with `:type`) against the podspec reference, and update the cocoapods row in `specs/1010-manifest-declared-license/contracts/license-extraction.md`
- [X] T005 [P] Verify the nuget `PackageLicenseExpression` field and whether `Directory.Build.props` supplies it, against the MSBuild pack documentation, and update the nuget row in `specs/1010-manifest-declared-license/contracts/license-extraction.md`
- [X] T006 [P] Verify cargo's `[package].license` and `license-file` against the Cargo manifest reference and update the cargo row in `specs/1010-manifest-declared-license/contracts/license-extraction.md`
- [X] T007 Capture the pre-change baseline by running the procedure in `specs/1010-manifest-declared-license/quickstart.md` against this repository, and record the observed count in `specs/1010-manifest-declared-license/research.md` under R1

**Checkpoint**: every contract row carries evidence. No row still reads *to verify*.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: the shared resolution ladder every reader calls. Duplicating this
across thirteen sites is what let #957 diverge in the first place.

**⚠️ CRITICAL**: no user story work begins until T013 passes.

- [X] T008 Create `waybill-cli/src/scan_fs/package_db/declared_license.rs` defining the `DeclaredLicense` enum with its three states (`Canonical`, `Preserved`, `Absent`) per `data-model.md`
- [X] T009 Implement the two-step resolution ladder `resolve(raw: &str) -> DeclaredLicense` in `waybill-cli/src/scan_fs/package_db/declared_license.rs`: `SpdxExpression::try_canonical` first, falling back to `SpdxExpression::new` on error (FR-004a)
- [X] T010 Implement the per-ecosystem join helper in `waybill-cli/src/scan_fs/package_db/declared_license.rs` taking an operator (conjunction or disjunction) and several raw declarations, returning one combined string before resolution (FR-010, FR-010b)
- [X] T011 Add the debug-level diagnostic emitted when `resolve` falls back to `Preserved`, naming manifest path, raw value and canonicalisation error, in `waybill-cli/src/scan_fs/package_db/declared_license.rs` (FR-004b)
- [X] T012 Register the `declared_license` module in `waybill-cli/src/scan_fs/package_db/mod.rs`
- [X] T013 Add unit tests in `waybill-cli/src/scan_fs/package_db/declared_license.rs` covering all three ladder outcomes: a canonical value, a non-canonically-spelled value that canonicalises (FR-005), and an uncanonicalisable value that is preserved verbatim (FR-004)
- [X] T014 Add unit tests in `waybill-cli/src/scan_fs/package_db/declared_license.rs` asserting the join helper produces `A OR B` for a disjunctive ecosystem and `A AND B` for the conjunctive fallback (FR-010a)
- [X] T015 Add a unit test in `waybill-cli/src/scan_fs/package_db/declared_license.rs` asserting an empty or whitespace-only declaration yields `Absent` with no diagnostic (FR-006)

**Checkpoint**: the ladder is proven in isolation; readers can now be converted independently.

---

## Phase 3: User Story 1 — An auditor asks what the project is licensed under (Priority: P1) 🎯 MVP

**Goal**: the main-module component carries the license its manifest declares, for
every affected ecosystem, and the document's primary component inherits it when
unambiguous.

**Independent Test**: scan a project whose manifest declares a license in any one
ecosystem and confirm the main-module component carries it with declared
attribution. Deliverable with a single ecosystem done.

### Tests for User Story 1

- [X] T016 [US1] Create `waybill-cli/tests/declared_license.rs` asserting that scanning a cargo fixture with `license = "MIT"` yields that license with declared attribution in CycloneDX, SPDX 2.3 and SPDX 3, with a **control assertion** that the fixture emits no license before the change so the test cannot pass vacuously
- [X] T017 [US1] Add a case to `waybill-cli/tests/declared_license.rs` asserting an `--offline` scan yields the declared license, proving independence from enrichment (FR-012)
- [X] T018 [US1] Add a case to `waybill-cli/tests/declared_license.rs` asserting concluded-attribution licenses are unchanged in count and value against a pre-change scan of the same input (FR-003, SC-006)
- [X] T019 [US1] Add a case to `waybill-cli/tests/declared_license.rs` using a fixture whose manifest parses for identity but whose license field is malformed, asserting the component is still emitted and the scan exits zero (FR-007)

### Implementation for User Story 1

- [X] T020 [US1] Extract `[package].license` in `waybill-cli/src/scan_fs/package_db/cargo.rs::build_cargo_main_module_entry` (licenses@703), calling the shared ladder
- [X] T021 [US1] Resolve `license.workspace = true` against `[workspace.package]` in `waybill-cli/src/scan_fs/package_db/cargo.rs`, reusing the traversal already used by `resolve_cargo_main_module_version` (FR-011a)
- [X] T022 [US1] Add a case to `waybill-cli/tests/declared_license.rs` using a workspace fixture whose member declares `license.workspace = true` while the root declares no license, asserting the member carries no license and the scan succeeds (FR-011b)
- [X] T023 [US1] Replace the assertion at `waybill-cli/src/scan_fs/package_db/cargo.rs:3004` (`assert!(entry.licenses.is_empty())`) with one asserting the declared license is present, so the regression guard points the right way (FR-014)
- [ ] T024 [P] [US1] Extract `license` in `waybill-cli/src/scan_fs/package_db/npm/walk.rs::build_npm_main_module_entry` (licenses@670)
- [ ] T025 [P] [US1] Extract `license` in `waybill-cli/src/scan_fs/package_db/npm/mod.rs::synthesize_nameless_nested_mainmods` (licenses@716)
- [ ] T026 [P] [US1] Extract `[project].license` in `waybill-cli/src/scan_fs/package_db/pip/mod.rs::build_pip_main_module_entry` (licenses@1022), treating the deprecated `license.file` table form as `Absent` per FR-011
- [ ] T027 [P] [US1] Extract `licenses` / `license` in `waybill-cli/src/scan_fs/package_db/gem.rs::build_gem_main_module_entry` (licenses@1505), joining with the conjunctive fallback
- [ ] T028 [P] [US1] Extract the same in `waybill-cli/src/scan_fs/package_db/gem.rs::build_gem_application_main_module_entry` (licenses@1786)
- [ ] T029 [P] [US1] Extract `<licenses><license><name>` in `waybill-cli/src/scan_fs/package_db/maven.rs::build_maven_main_module_entry` (licenses@4434), joining with the conjunctive fallback
- [ ] T030 [US1] Resolve maven parent-POM license inheritance in `waybill-cli/src/scan_fs/package_db/maven.rs`, since `licenses` is an inherited POM element (FR-011a)
- [ ] T031 [P] [US1] Extract `license` in `waybill-cli/src/scan_fs/package_db/composer.rs::emit_main_module` (licenses@560), joining an array with **disjunction** per the documented Composer semantics — the one ecosystem that is not the fallback
- [ ] T032 [P] [US1] Extract the license in `waybill-cli/src/scan_fs/package_db/elixir.rs::emit_main_module` (licenses@1074) per the T001-verified contract row
- [ ] T033 [P] [US1] Extract the license in `waybill-cli/src/scan_fs/package_db/erlang.rs::build_main_module_component` (licenses@1489) per the T002-verified contract row
- [ ] T034 [P] [US1] Extract the license in `waybill-cli/src/scan_fs/package_db/scala.rs::build_main_module_component` (licenses@1322) per the T003-verified contract row
- [ ] T035 [P] [US1] Extract the license in `waybill-cli/src/scan_fs/package_db/cocoapods.rs::emit_main_module` (licenses@686) per the T004-verified contract row
- [ ] T036 [P] [US1] Extract `PackageLicenseExpression` in `waybill-cli/src/scan_fs/package_db/nuget/mod.rs::build_nuget_main_module_entry` (licenses@781) per the T005-verified contract row
- [ ] T037 [US1] Implement scan-root license inheritance in `waybill-cli/src/generate/root_selector.rs` (alongside `select_root` at line 165, which already has the resolved component set in hand), attaching a license only when exactly one main-module component carries one (FR-016, FR-017)
- [ ] T038 [US1] Add a case to `waybill-cli/tests/declared_license.rs` asserting the scan-root inherits when exactly one main-module carries a license, **and does not** when two do — using this repository's own two-crate workspace as the negative case (SC-001a)
- [ ] T039 [P] [US1] Add per-ecosystem unit tests in the `#[cfg(test)]` module of each converted reader — `cargo.rs`, `npm/walk.rs`, `npm/mod.rs`, `pip/mod.rs`, `gem.rs`, `maven.rs`, `composer.rs`, `elixir.rs`, `erlang.rs`, `scala.rs`, `cocoapods.rs`, `nuget/mod.rs` under `waybill-cli/src/scan_fs/package_db/` — asserting a declared license is extracted

**Checkpoint**: every affected ecosystem emits its declared license; US1 is independently verifiable.

---

## Phase 4: User Story 2 — An invalid declaration is preserved, not passed off (Priority: P2)

**Goal**: an uncanonicalisable declaration survives as a non-listed license
reference, and is never presented as a recognised identifier.

**Independent Test**: scan a project declaring `AllRightsReserved` and confirm a
`LicenseRef` with matching extracted text, not `NOASSERTION`.

### Tests for User Story 2

- [ ] T040 [US2] Add a case to `waybill-cli/tests/declared_license_preservation.rs` asserting an uncanonicalisable declaration yields a `LicenseRef-` identifier in SPDX 2.3 plus a matching `hasExtractedLicensingInfos` entry carrying the raw text, and **not** `NOASSERTION` (FR-004, FR-004c)
- [ ] T041 [US2] Add a case to `waybill-cli/tests/declared_license_preservation.rs` asserting the same declaration appears in CycloneDX and SPDX 3 without being presented as a listed identifier (FR-004c, SC-004)
- [ ] T042 [US2] Add a case to `waybill-cli/tests/declared_license_preservation.rs` asserting the count of declarations found equals the count emitted across a fixture set mixing valid and invalid values (SC-004a)

### Implementation for User Story 2

- [ ] T043 [US2] Change `waybill-cli/src/scan_fs/package_db/haskell.rs` (license extraction around lines 1258–1283) to call the shared ladder instead of dropping on canonicalisation failure, bringing #957 into line (FR-008a)
- [ ] T044 [US2] Update the superseded comment at `waybill-cli/src/scan_fs/package_db/haskell.rs:1279` which states the value is "omitted rather than emitted unverified", since it is now preserved rather than omitted
- [ ] T045 [P] [US2] Create the fixture used by the US2 tests under `waybill-cli/tests/fixtures/`, using `waybill-fixture-*` package names — real coordinates trip the repository's advisory scanning

**Checkpoint**: no declared license is lost in any ecosystem, including Haskell.

---

## Phase 5: User Story 3 — Coverage is consistent across ecosystems (Priority: P3)

**Goal**: identical treatment of the same input shape in every ecosystem, and the
excluded ecosystems documented as deliberate.

**Independent Test**: one fixture per ecosystem, each asserting the same three
outcomes, passing uniformly.

### Tests for User Story 3

- [ ] T046 [US3] Add a table-driven case to `waybill-cli/tests/declared_license_consistency.rs` running the same three outcomes (canonical, preserved, absent) across one fixture per affected ecosystem, so an ecosystem behaving differently fails rather than being noticed later (SC-003)
- [ ] T047 [US3] Add a case to `waybill-cli/tests/declared_license_consistency.rs` asserting repeated scans of identical input produce byte-identical license data (FR-015, SC-007)
- [ ] T048 [US3] Add a case to `waybill-cli/tests/declared_license_consistency.rs` asserting no component loses a license it carried before the change (SC-005)
- [ ] T049 [US3] Create a two-license fixture per list-valued ecosystem (maven, gem, composer, elixir, erlang, scala) under `waybill-cli/tests/fixtures/`, and assert in `waybill-cli/tests/declared_license_consistency.rs` that each yields one expression whose operator matches its contract row — disjunction for composer, conjunction for the rest (SC-004b)

### Implementation for User Story 3

- [ ] T050 [P] [US3] Document the per-ecosystem license key, multi-license operator and inheritance rule in `docs/reference/`, stating explicitly where the conjunctive operator is waybill's inference rather than the ecosystem's declaration (FR-010a)
- [ ] T051 [P] [US3] Document in `docs/reference/` why Go, Swift and Dart carry no declared license — their manifests have no license field, and file-content detection is a different mechanism (FR-009)

**Checkpoint**: coverage is uniform and the gaps are explained rather than apparent.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [ ] T052 [P] Correct the five stale `#103` comments deferring license detection, at `waybill-cli/src/scan_fs/package_db/cargo.rs:634`, `pip/mod.rs:614`, `npm/walk.rs:506`, `golang/legacy.rs:965` and `golang/legacy.rs:4185` (FR-013, SC-008)
- [ ] T053 [P] Correct the `PackageDbEntry.licenses` doc comment at `waybill-cli/src/scan_fs/package_db/mod.rs:137` where it describes sources as unpopulated
- [ ] T054 Confirm `#1008` is closed and the public-corpus lane is green **before** regenerating any golden, so this change's diff is readable in isolation — procedure in `docs/development/refreshing-corpus-goldens.md`
- [ ] T055 Regenerate the public-corpus goldens in CI — never locally — and read every diff before accepting, per `docs/development/refreshing-corpus-goldens.md`
- [ ] T056 Regenerate goldens for all six golden-writing test files that respond to `WAYBILL_UPDATE_*` env vars, not only the three `*_regression` ones: `waybill-cli/tests/cdx_regression.rs`, `spdx_regression.rs`, `spdx3_regression.rs`, `oci_pull_backward_compat.rs`, `optional_dep_classification.rs`, `pkg_alias_binding_us1.rs`
- [ ] T057 Run the quickstart validation in `specs/1010-manifest-declared-license/quickstart.md` end to end, including the negative scan-root case
- [ ] T058 Run `./scripts/pre-pr.sh` and enumerate every per-target result line; both clippy `--all-targets` and the full workspace test run must be clean before a PR is opened

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: no dependencies. T001–T006 are fully parallel; T007 is independent of them.
- **Phase 2 (Foundational)**: depends on Phase 1 only for the ecosystems whose rows it encodes in the operator table (T010 needs T001–T005; T006 verifies cargo, which is not an operator row). **Blocks all user stories.**
- **Phase 3 (US1)**: depends on Phase 2. The twelve parallel extraction tasks are T024–T029 and T031–T036, one file each. Not parallel: T020 (cargo extraction, which T021 and T023 depend on), T021, T023, T030 (maven inheritance, depends on T029), T037 (needs at least one extraction to exist), and T016–T019/T022/T038, which all edit one test file.
- **Phase 4 (US2)**: depends on Phase 2. Independent of US1 — the Haskell reader already extracts, so only its failure branch changes.
- **Phase 5 (US3)**: depends on US1 and US2, since it asserts uniformity across what they produce.
- **Phase 6 (Polish)**: T052/T053 anytime after Phase 2. T054 blocks T055 and T056. T058 last.

### Critical path

```text
T001–T005 ─> T010 ─┐
T008 ─> T009 ──────┼─> T013 ─> T020 ─> T021 ─> T037 ─> T046 ─> T054 ─> T055 ─> T058
                   └─> T014
```

### Parallel Opportunities

- **Phase 1**: all six verification tasks at once (T001–T006).
- **Phase 2**: none. T013–T015 all add test cases to the same module, so they are sequential; the parallelism here is illusory and was removed rather than left as a trap.
- **Phase 3**: thirteen extraction tasks in total, twelve of which touch twelve different files — T024–T029 and T031–T036 are all parallel. Cargo (T020) is excluded because T021 and T023 build on it. This is the widest fan-out in the feature.
- **Phase 5**: T050 and T051 are documentation, parallel with each other and with Phase 3 testing.

### Within Each User Story

- Tests before implementation. T016's control assertion must be seen to hold on unchanged code, or the test proves nothing.
- The shared ladder before any reader calls it.
- Extraction before inheritance resolution in the same reader.
- All extraction before scan-root inheritance, which counts main-modules.

---

## Implementation Strategy

### MVP

**Phase 1 + Phase 2 + T016–T023.** That is cargo alone: extraction, workspace
inheritance, the corrected regression test, and the ladder underneath. It is
independently valuable — it makes this repository's own SBOM carry its license —
and it validates every design decision before being repeated eleven times.

### Incremental delivery

1. **PR 1** — Phases 1–2 and cargo (T001–T023). Establishes the contract in code.
2. **PR 2** — remaining ecosystems (T024–T036, T039). Wide but mechanical, each file independent.
3. **PR 3** — scan-root inheritance and US2 preservation, including the #957 correction (T037–T038, T040–T045).
4. **PR 4** — US3 consistency, documentation, and the golden regeneration (T046–T058), gated on #1008.

Splitting the goldens into their own PR matters: a regeneration diff mixed with
eleven reader changes is unreadable, and reading it is the point.

### Risks

- **T010's operator table encodes five unverified operator rows** (elixir, erlang, scala, plus cocoapods and nuget confirming they are single-expression). If Phase 1 finds an
  ecosystem documents disjunction, the table changes and any reader already written
  against the fallback is wrong. Hence Phase 1 precedes Phase 2.
- **T043 edits code #957 shipped days ago.** Expect its existing tests to assert
  the superseded behaviour; they change with it.
- **T037 is not a reader concern.** No single reader can count main-modules across
  ecosystems, so implementing it inside one would be wrong however convenient.
