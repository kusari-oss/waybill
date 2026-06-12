---

description: "Task list for milestone 113 — user-supplied directory exclusion for `mikebom scan`"
---

# Tasks: User-Supplied Directory Exclusion for `mikebom scan`

**Input**: Design documents from `/specs/113-exclude-path-flag/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md (all present)

**Organization**: Tasks are grouped by user story (US1 → US3 as in spec.md). Each user-story phase ships an independently demonstrable increment.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can be implemented in parallel (different files, no incomplete deps).
- **[Story]**: Maps to a user story (US1/US2/US3). Absent on Setup, Foundational, and Polish phases.
- Every task names the exact file path it touches.

## Path Conventions

Single-project layout (matches every milestone since 001). All paths are relative to repository root `/Users/mlieberman/Projects/mikebom/`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: New crate dep + skeleton module + vendored fixture so Foundational tasks compile against real artifacts.

- [X] T001 Add `globset = "0.4"` as a new direct dep under `[dependencies]` in `mikebom-cli/Cargo.toml`; run `cargo +stable build -p mikebom` to verify the lockfile updates cleanly with no new top-level transitives (regex/regex-syntax already present)
- [X] T002 Create `mikebom-cli/src/scan_fs/package_db/exclude_path.rs` as a stub module declaring `pub(crate) use` re-exports; register the module via `pub(crate) mod exclude_path;` in `mikebom-cli/src/scan_fs/package_db/mod.rs`; run `cargo +stable check -p mikebom` to confirm the workspace compiles
- [X] T003 [P] Vendor polyglot fixture tree under `mikebom-cli/tests/fixtures/exclude_path/` — one real top-level project per ecosystem (cargo, maven, gem, pip, npm, gradle, nuget, yocto, golang) AND a sibling `tests/fixtures/<ecosystem>/` for each that would emit a fixture main-module pre-feature; include at least one fixture manifest whose `require`/`dependsOn` points at the parent project (the synthetic-edge case from issue #334); also stage a pre-built Go binary at `tests/fixtures/golang/bin/foo` for the FR-013 binary-tier coverage required by T021 (the binary need only carry valid Go BuildInfo — produce with `cd <fixture-golang-src> && GOOS=linux GOARCH=amd64 go build -o tests/fixtures/golang/bin/foo` once and commit the artifact)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Wire `ExclusionSet` end-to-end so every walker consults the empty set as a no-op. Once these complete, US1's behavior change is one annotation away.

**CRITICAL**: No US1/US2/US3 work can begin until this phase is complete.

- [X] T004 Define `ExcludePathError` (thiserror enum: `EmptyEntry`, `MalformedPattern{entry, source}`), `ExclusionEntry` enum (`Literal(PathBuf)` / `Pattern(globset::Glob)`) with `ExclusionEntry::parse(&str) -> Result<Self, ExcludePathError>`, and `ExclusionSet` struct (fields `entries`, `pattern_set: Option<GlobSet>`, `literal_paths: Vec<String>`) with constructors `from_iter`/`new_empty` and operations `is_empty`/`matches(&str) -> bool`/`as_normalized_strings() -> Vec<String>` in `mikebom-cli/src/scan_fs/package_db/exclude_path.rs`
- [X] T005 Unit tests in `mikebom-cli/src/scan_fs/package_db/exclude_path.rs` `#[cfg(test)] mod tests` covering: literal-vs-pattern classification by metacharacter presence; literal match anchored at scan root; pattern match at arbitrary depth; cross-platform separator normalization (`\\` in entry matches forward-slash candidate); dedup of duplicate inputs; empty-entry rejection; malformed-pattern rejection (`[` unmatched); empty-set is_empty true; non-empty-set as_normalized_strings preserves source order; **no-match no-op** (FR-008: ExclusionSet containing one entry that matches no candidate path emits no warning and returns is_empty()==false — verified by inspecting `tracing::subscriber` capture)
- [X] T006 Change `WalkConfig.should_skip` closure shape from `&'a dyn Fn(&str) -> bool` to `&'a dyn Fn(&Path, &Path) -> bool`; change `should_skip_default_descent` signature to `(candidate: &Path, rootfs: &Path, exclude_set: &ExclusionSet) -> bool` (compute candidate's relative path, check ExclusionSet first, then fall through to existing built-in name-based skips); update every closure constructed at `pip/mod.rs:285`, `npm/mod.rs:460`, `gradle/mod.rs:36`, `nuget/mod.rs:77`, `yocto/recipe.rs:59` to capture `exclude_set` and pass the new arg shape through; touches `mikebom-cli/src/scan_fs/package_db/project_roots.rs` + 5 caller files
- [X] T007 Update each per-walker descent helper to `(candidate: &Path, rootfs: &Path, exclude_set: &ExclusionSet) -> bool`, consulting ExclusionSet before the existing built-in checks: `should_skip_descent` in `mikebom-cli/src/scan_fs/package_db/cargo.rs:1167`, `mikebom-cli/src/scan_fs/package_db/maven.rs:3228`, `mikebom-cli/src/scan_fs/package_db/gem.rs:1069`, `mikebom-cli/src/scan_fs/package_db/golang/legacy.rs:2034`, and `should_skip_binary_descent` in `mikebom-cli/src/scan_fs/package_db/go_binary.rs:653`; update each helper's existing callsites within the same file
- [X] T008 Thread `exclude_set: &ExclusionSet` through every per-ecosystem `read` fn signature: `cargo::read` at `mikebom-cli/src/scan_fs/package_db/cargo.rs:824`, `maven::read` at `mikebom-cli/src/scan_fs/package_db/maven.rs:2254`, `gem::read` at `mikebom-cli/src/scan_fs/package_db/gem.rs:604`, `golang::read` at `mikebom-cli/src/scan_fs/package_db/golang/legacy.rs:1222`, `go_binary::read` at `mikebom-cli/src/scan_fs/package_db/go_binary.rs:492`, plus the four shared-helper readers (`pip::read`, `npm::read`, `gradle::read`, `nuget::read`) and `yocto::read`; each fn forwards the borrow to its WalkConfig closure or per-walker helper
- [X] T009 Thread `exclude_set: &ExclusionSet` through `read_all` in `mikebom-cli/src/scan_fs/package_db/mod.rs:1299` (the `golang::read(rootfs, include_dev)` call at line 1307 and every other per-ecosystem read invocation gains the new arg)
- [X] T010 Thread `exclude_set: &ExclusionSet` through `scan_path` in `mikebom-cli/src/scan_fs/mod.rs:118` (add the parameter after `scan_target_name`, forward to `read_all`)
- [X] T011 Add `--exclude-path <PATH_OR_PATTERN>` clap flag with `ArgAction::Append` and the comprehensive doc-comment from `contracts/cli-flag.md` to `mikebom-cli/src/main.rs`; add `MIKEBOM_EXCLUDE_PATH` env-var parser in `mikebom-cli/src/cli/scan_cmd.rs` (mirror the `MIKEBOM_PKG_ALIAS` precedent at `scan_cmd.rs:2262` but split entries on platform path-list separator using `std::env::split_paths`); construct `ExclusionSet::from_iter(cli_entries.chain(env_entries))?`; surface the set to `scan_path`; reject malformed entries before the scan starts (FR-007 / SC-005); add `.env_remove("MIKEBOM_EXCLUDE_PATH")` to the existing test harness in `scan_cmd.rs` so the new env var doesn't leak across tests

**Checkpoint**: foundational complete. Every walker compiles against the new signatures; empty ExclusionSet preserves pre-feature behavior; `mikebom scan --exclude-path tests/fixtures /repo` parses and runs, but emits no transparency annotation yet.

---

## Phase 3: User Story 1 — Literal-path exclusion across every ecosystem (Priority: P1) 🎯 MVP

**Goal**: A single `--exclude-path <literal>` argument suppresses fixture components and their synthetic edges across every ecosystem walker, and the SBOM carries the required transparency annotation.

**Independent Test**: Run `mikebom scan --exclude-path tests/fixtures /path/to/polyglot/fixture` and verify (a) every ecosystem's real top-level component appears, (b) every fixture's component does not, (c) no dependency edge references any suppressed component, and (d) the emitted SBOM carries `mikebom:exclude-path = "tests/fixtures"` at envelope level in all three formats.

### Implementation for User Story 1

- [X] T012 [P] [US1] Emit `mikebom:exclude-path` property on CDX 1.6 `metadata.properties[]` when `exclude_set.is_empty() == false`, value = comma-joined `as_normalized_strings()` output, in `mikebom-cli/src/generate/cyclonedx/metadata.rs`; thread the ExclusionSet borrow in via existing emission-context plumbing
- [X] T013 [P] [US1] Emit `mikebom:exclude-path` annotation on SPDX 2.3 `creationInfo.annotations[]` (annotationType=OTHER, annotator=`Tool: mikebom-<version>`, annotationDate=existing emission timestamp, comment=`mikebom:exclude-path=<joined>`) when the set is non-empty, in `mikebom-cli/src/generate/spdx/annotations.rs`
- [X] T014 [P] [US1] Emit `mikebom:exclude-path` as an SPDX 3 document-level `Annotation` element (annotationType=other, subject=SpdxDocument SPDXID, statement=`mikebom:exclude-path=<joined>`, creationInfo=existing blank-node ref) when the set is non-empty, in `mikebom-cli/src/generate/spdx/v3_annotations.rs`
- [X] T015 [P] [US1] Add three parity-catalog rows tracking that all three emitters produce byte-equivalent `mikebom:exclude-path` payloads for the same scan invocation, in `mikebom-cli/src/parity/extractors/cdx.rs`, `mikebom-cli/src/parity/extractors/spdx2.rs`, `mikebom-cli/src/parity/extractors/spdx3.rs` (envelope-level extraction; mirror the milestone-072 cross-tier-binding row pattern)
- [X] - [ ] T016 [US1] Integration test `cargo_fixture_suppressed_under_tests_fixtures` in `mikebom-cli/tests/exclude_path_integration.rs`: scan polyglot fixture with `--exclude-path tests/fixtures`; assert real top-level cargo crate component present, fixture cargo crate component absent
- [X] - [ ] T017 [US1] Integration test `maven_fixture_suppressed_under_tests_fixtures` in `mikebom-cli/tests/exclude_path_integration.rs`: same shape, asserting maven fixture suppression
- [X] - [ ] T018 [US1] Integration test `gem_fixture_suppressed_under_tests_fixtures` in `mikebom-cli/tests/exclude_path_integration.rs`: same shape, asserting gem fixture suppression
- [X] - [ ] T019 [US1] Integration test `shared_helper_walkers_fixtures_suppressed_under_tests_fixtures` in `mikebom-cli/tests/exclude_path_integration.rs`: assert pip, npm, gradle, nuget, yocto fixtures are all suppressed by the same single argument (proves the WalkConfig closure threading from T006 works)
- [ ] T020 [US1] Integration test `golang_source_fixture_suppressed_under_tests_fixtures` in `mikebom-cli/tests/exclude_path_integration.rs`: place a Go fixture at `tests/fixtures/golang/go.mod` (NOT under `testdata/` or a `_`-prefixed dir, so the Go-tool unconditional skip from milestone 113 doesn't fire); assert it's suppressed by `--exclude-path tests/fixtures`
- [ ] T021 [US1] Integration test `go_binary_fixture_suppressed_under_tests_fixtures` in `mikebom-cli/tests/exclude_path_integration.rs`: place a pre-built Go fixture binary at `tests/fixtures/golang/bin/foo`; assert it's suppressed by `--exclude-path tests/fixtures` (FR-013 binary-tier coverage)
- [X] T022 [US1] Integration test `polyglot_union_single_argument_suppresses_every_ecosystem` in `mikebom-cli/tests/exclude_path_integration.rs`: one scan of the polyglot fixture with `--exclude-path tests/fixtures` produces zero fixture components from any ecosystem in one SBOM (proves walker union semantics in one invocation)
- [ ] T023 [US1] Integration test `dependency_edges_referencing_suppressed_components_dropped` in `mikebom-cli/tests/exclude_path_integration.rs`: load a fixture whose manifest declares a synthetic require on the parent; assert the emitted SBOM contains zero CDX `dependencies[]` entries with `ref` or `dependsOn` mentioning the fixture's purl (FR-010); also add `scan_root_excluded_yields_only_metadata_component` covering the edge case where `--exclude-path .` suppresses every ecosystem component but leaves `metadata.component` intact
- [X] T024a [US1] Transparency annotation test `exclude_path_annotation_emitted_when_set_non_empty_omitted_when_empty` in `mikebom-cli/tests/exclude_path_integration.rs`: scan polyglot fixture (i) with `--exclude-path tests/fixtures` and assert `mikebom:exclude-path = "tests/fixtures"` is present at envelope level in CDX (`metadata.properties[]`), SPDX 2.3 (`creationInfo.annotations[]`), and SPDX 3 (document-level Annotation element); (ii) with NO exclusion flag and assert the annotation is ABSENT in every format (SC-007 / FR-014)
- [X] T024 [US1] Byte-identity regression test `no_flag_scan_is_byte_identical_with_committed_golden` in `mikebom-cli/tests/exclude_path_integration.rs`: (a) **Golden generation** — once T010 lands so ExclusionSet is wired with a default-empty value, run `MIKEBOM_FIXED_TIMESTAMP=2026-01-01T00:00:00Z cargo run -p mikebom -- scan mikebom-cli/tests/fixtures/exclude_path --format cdx --output mikebom-cli/tests/fixtures/exclude_path/goldens/no_flag.cdx.json` (and the SPDX 2.3 + SPDX 3 equivalents) with NO `--exclude-path` and `MIKEBOM_EXCLUDE_PATH` unset; serial-number-mask each golden via the existing helper from `mikebom-cli/tests/common/normalize.rs` and commit; these goldens ARE the "pre-feature" baseline since the polyglot fixture is itself new. (b) **Regression test** — scan polyglot fixture root with no exclusion entries on every subsequent run, mask the serial number identically, compare each emitted format against the committed golden; assert byte-equality (FR-003 / SC-002)

**Checkpoint**: US1 is fully functional. `mikebom scan --exclude-path tests/fixtures /repo` works end-to-end for every ecosystem, the SBOM carries the transparency annotation, and pre-feature behavior is provably preserved when the flag is absent.

---

## Phase 4: User Story 2 — Pattern matching across a monorepo (Priority: P2)

**Goal**: A single `**`-style pattern matches every fixture directory at any depth in a monorepo with multiple fixture trees.

**Independent Test**: Scan a synthetic monorepo with `services/a/testdata/`, `services/b/testdata/`, `services/c/testdata/` subtrees containing fixture manifests; pass `--exclude-path '**/testdata'`; assert every real service component appears and zero fixture components do.

### Implementation for User Story 2

- [X] T025 [US2] Integration test `glob_pattern_matches_nested_testdata_subtrees` in `mikebom-cli/tests/exclude_path_integration.rs`: synthesize per-test a multi-service repo (write 3 `services/<name>/testdata/<ecosystem>/<manifest>` subtrees via `tempfile`); scan with `--exclude-path '**/testdata'`; assert zero fixture components, every real service present
- [ ] T026 [US2] Integration test `multiple_pattern_entries_combine_by_union` in `mikebom-cli/tests/exclude_path_integration.rs`: scan with two pattern entries (e.g. `--exclude-path '**/testdata' --exclude-path '**/_archive'`); assert both subtree shapes suppressed in one scan
- [ ] T027 [US2] Integration test `cross_platform_separator_normalization` in `mikebom-cli/tests/exclude_path_integration.rs`: pass a literal entry `tests\fixtures` (Windows-style backslash); assert the same directories are suppressed as when passing `tests/fixtures` (FR-009); skip-on-windows for the inverse direction is unnecessary because both forms normalize identically

**Checkpoint**: US2 is fully functional. Pattern entries demonstrably suppress monorepo-scale fixture trees.

---

## Phase 5: User Story 3 — Discoverability + Documentation (Priority: P3)

**Goal**: An operator new to mikebom can discover and correctly use `--exclude-path` without reading source.

**Independent Test**: Run `mikebom scan --help` and verify `--exclude-path` is listed with a one-line description and a pointer to `docs/user-guide/cli-reference.md`. Read the referenced section and confirm it contains a fully-worked non-Go example plus the troubleshooting matrix.

### Implementation for User Story 3

- [X] T028 [P] [US3] Add comprehensive doc-comment for the `--exclude-path` clap flag (model after the milestone-112 `--no-go-mod-why` doc-comment) in `mikebom-cli/src/main.rs` and the global-flag table at `mikebom-cli/src/cli/scan_cmd.rs`; mention literal-vs-pattern classification, `MIKEBOM_EXCLUDE_PATH`, and the pointer to user-guide section
- [X] T029 [P] [US3] Add full `### --exclude-path` section in `docs/user-guide/cli-reference.md`: insert a row in the global-flags table; write the section content from `specs/113-exclude-path-flag/quickstart.md` (problem statement, literal usage, repeated flags, pattern usage, env-var usage, transparency-annotation inspection, troubleshooting matrix); cross-link to `docs/ecosystems.md`
- [X] T030 [P] [US3] Add `mikebom:exclude-path` row to `docs/reference/sbom-format-mapping.md` with the Principle V bullet 5 justification clause from `specs/113-exclude-path-flag/contracts/annotations.md` (cite the audit result naming the absent native field per format)
- [ ] T031 [P] [US3] Cross-link from each ecosystem section in `docs/ecosystems.md` to the cli-reference `--exclude-path` entry: one paragraph per ecosystem section noting that operator-supplied directory exclusion is available for that ecosystem
- [ ] T032 [P] [US3] Integration test `help_text_documents_exclude_path` in `mikebom-cli/tests/exclude_path_integration.rs`: spawn `mikebom scan --help`; assert stdout contains the substring `--exclude-path` and a pointer to the user-guide section (specific phrase match against the doc-comment from T028)

**Checkpoint**: All three user stories are independently functional and demonstrable.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [ ] T033 [P] Emit `tracing::debug!` per matched directory ("exclude-path: matched <relative-path> against entry <X>") at each descent decision when exclude_set match fires (in `should_skip_default_descent` and each per-walker descent helper); emit `tracing::info!` summary "exclude-path: applied N entries, suppressed M directories" at end of `scan_path` in `mikebom-cli/src/scan_fs/mod.rs` when set non-empty; matches milestone-112's FR-013 stderr-summary pattern
- [ ] T034 Performance regression check: ignored-by-default test `exclude_path_overhead_within_budget` in `mikebom-cli/tests/exclude_path_integration.rs` (gated with `#[ignore]` or `MIKEBOM_PERF=1`) — time scan of polyglot fixture with vs without `--exclude-path '**/testdata'`; assert with-flag wall time ≤ 1.10 × no-flag wall time (SC-003)
- [X] T035 Run the mandatory pre-PR gate `MIKEBOM_SKIP_DOCKER_INTEGRATION=1 ./scripts/pre-pr.sh` — BOTH `cargo +stable clippy --workspace --all-targets -- -D warnings` and `cargo +stable test --workspace` must pass clean before the PR opens; mandatory per the constitution's "Pre-PR Verification" section

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 Setup**: no deps, run first.
- **Phase 2 Foundational**: depends on Phase 1; BLOCKS US1/US2/US3.
- **Phase 3 US1**: depends on Phase 2 complete; independent of US2/US3.
- **Phase 4 US2**: depends on Phase 2 complete (NOT on US1 — pattern parsing already lives in Phase 2's `ExclusionEntry::parse`).
- **Phase 5 US3**: depends on Phase 2 complete (docs reference flag surface defined in T011) AND on T028 (--help text) for T032.
- **Phase 6 Polish**: T033/T034 depend on US1 complete (need walker integration to log/benchmark); T035 depends on every other task.

### Within-Phase Dependencies

| Task | Depends on |
|---|---|
| T002 | T001 |
| T004 | T002 |
| T005 | T004 |
| T006 | T004 |
| T007 | T004 |
| T008 | T006 + T007 |
| T009 | T008 |
| T010 | T009 |
| T011 | T010 |
| T012–T015 | T011 (need ExclusionSet wired in) |
| T015 | T012 + T013 + T014 (parity extractors read what the emitters write) |
| T024a | T012 + T013 + T014 (transparency annotation test asserts emitter output) |
| T016–T024 | T012/T013/T014 (annotation emission must exist for T024's byte-identity golden capture; T016–T023 only need scan-path threading) |
| T025–T027 | T011 (need parser) |
| T028 | T011 |
| T029–T031 | T011 |
| T032 | T028 |
| T033 | T010 (scan_path), T007 (walker helpers) |
| T034 | T011 + US1 complete (T024 golden) |
| T035 | every prior task |

### Parallel Opportunities

- T003 runs in parallel with T001–T002 (vendoring is decoupled from code).
- T012, T013, T014, T015 are [P] — different files (cyclonedx/metadata.rs, spdx/annotations.rs, spdx/v3_annotations.rs, three parity extractor files).
- T028–T031 are [P] — different files (main.rs/scan_cmd.rs, cli-reference.md, sbom-format-mapping.md, ecosystems.md).
- T016–T024 all touch `mikebom-cli/tests/exclude_path_integration.rs` so they are sequential within that file (cargo's parallel test runner still runs them concurrently at test time, but the source-edit order is sequential).

---

## Parallel Example: Phase 3 US1 annotation emitters

```bash
# Once T011 lands and ExclusionSet is wired into scan_path:
Task T012: Emit mikebom:exclude-path on CDX metadata.properties in cyclonedx/metadata.rs
Task T013: Emit mikebom:exclude-path on SPDX 2.3 creationInfo.annotations in spdx/annotations.rs
Task T014: Emit mikebom:exclude-path as SPDX 3 Annotation element in spdx/v3_annotations.rs
Task T015: Add parity-catalog rows in parity/extractors/{cdx,spdx2,spdx3}.rs
# Four developers (or four parallel agent runs) can complete these in one batch.
```

## Parallel Example: Phase 5 US3 docs

```bash
# Once T011 + T028 land:
Task T029: Author cli-reference --exclude-path section in docs/user-guide/cli-reference.md
Task T030: Add sbom-format-mapping.md row with Principle V justification clause
Task T031: Cross-link from each ecosystem section in docs/ecosystems.md
# All three documentation files are independent.
```

---

## Implementation Strategy

### MVP (User Story 1 only)

1. Complete Phase 1 (T001–T003): new dep, skeleton module, fixture.
2. Complete Phase 2 (T004–T011): parser, walker plumbing, CLI/env wiring.
3. Complete Phase 3 (T012–T024): annotation emission + per-walker + polyglot + byte-identity tests.
4. STOP and VALIDATE: run the polyglot integration suite; demo `mikebom scan --exclude-path tests/fixtures /repo` against a real polyglot repo.
5. Ship as standalone PR if desired — US2/US3 are pure additions on top.

### Incremental Delivery

1. PR 1: Phase 1 + Phase 2 + Phase 3 (US1) — MVP, demonstrable
2. PR 2: Phase 4 (US2) — pattern support
3. PR 3: Phase 5 (US3) — docs + discoverability
4. PR 4: Phase 6 (Polish) — logging, perf check, pre-PR gate

Or bundle everything into one PR if the diff stays reviewable; the user has already expressed preference for single PRs in milestone 112.

### Parallel Team Strategy

| Lane | Tasks |
|---|---|
| A (model/walker) | T001 → T002 → T004 → T005 → T006 → T007 → T008 → T009 → T010 → T011 |
| B (fixtures) | T003 → block on A reaching T011 → T016/T017/T018/T019/T020/T021/T022/T023/T024 sequentially |
| C (emission) | block on A reaching T011 → T012, T013, T014, T015 in parallel |
| D (docs/discoverability) | block on T011 → T028 → T029/T030/T031 in parallel, then T032 |

A and B+C run mostly sequentially; D is decoupled after T011.

---

## Notes

- Tests are first-class for this feature: the spec ships testable Acceptance Scenarios for every story and FR-003/SC-002's byte-identity guarantee requires committed goldens. Per-ecosystem suppression tests (T016–T021) plus the polyglot union test (T022) prove every walker honors the contract.
- The Principle X transparency annotation (T012/T013/T014/T015) is not optional — it is required for SBOM compliance whenever exclusions are applied. Treat it as a US1 deliverable, not a polish item.
- The Principle V standards-native audit (T030) is required to ship the `mikebom:exclude-path` annotation legally per the constitution.
- Tasks T016–T024 share one file (`exclude_path_integration.rs`); coordinate appends to avoid merge conflicts but each test is independently authored.
- The pre-PR gate (T035) is the hard CI parity check — `cargo test -p mikebom` alone is INSUFFICIENT per the constitution's pre-PR-verification section.
