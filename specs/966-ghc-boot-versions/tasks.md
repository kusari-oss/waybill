# Tasks: GHC boot-library versions from the compiler the closure names

**Input**: Design documents from `/specs/966-ghc-boot-versions/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Tests**: requested. FR-004 and FR-013 require them, so test tasks come before implementation in each story.

**Fixture rule (Kusari Inspector, PRs #285 and #640):**
- Every non-GHC package in a committed fixture is a synthetic `waybill-fixture-*` name.
- GHC boot libraries appear in fixtures **by name only** (`base`, `Win32`). Their versions come from the embedded table at run time, and no committed fixture file contains a boot library version.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: The user story the task serves (US1, US2, US3)

## Path Conventions

- `HP` = `waybill-cli/src/scan_fs/package_db/nix/haskell_packages`
- `T` = `waybill-cli/tests`

---

## Phase 1: Setup

- [X] T001 Write the probe `scripts/ghc-boot-libraries.py`, per `contracts/boot-library-table.md`.
  - Use Python stdlib only, run with `python3 -I -B`. Put downloads in a `tempfile.mkdtemp()` directory, never the repo.
  - Pin Stack's `global-hints.yaml` to a stackage-content commit SHA recorded in the output header.
  - Parse each release's "Included libraries" table with `html.parser`.
  - Keep a literal `OVERRIDES` list, starting with the 9.12.3 `process` 1.6.26.1 entry. Cite tag `ghc-9.12.3-release`, submodule commit `f7d51387ba7f7f6079f3a9d5ce011ad9359b7dbb` and `process.cabal`.
  - Exit non-zero on any notes-vs-Stack disagreement that has no override.
  - Mark `Win32` `windows` and every other library `all`.
  - Emit the TSV sorted by `(ghc as a version tuple, library as bytes)`.
- [X] T002 Run the probe and commit its output as `HP/ghc_boot_libraries.tsv`.
  - Record the row count and the release count in `specs/966-ghc-boot-versions/measurements/table-generation.txt`. Expect 36 releases, consistent with the 1,407-row Stack snapshot plus any notes-only libraries.
  - Spot-check against `measurements/global-hints-2026-11-12.yaml`:
    - 9.6.6 `base` 4.18.2.1;
    - 9.10.3 `base` 4.20.2.0;
    - 9.10.3 `process` 1.6.26.1;
    - 9.12.3 `process` 1.6.26.1 from `ghc-release-tag`.

## Phase 2: Foundational (blocks every story)

- [X] T003 [P] Write unit tests for the table in `HP/boot_table.rs`, under `#[cfg(test)]` with `#[cfg_attr(test, allow(clippy::unwrap_used))]`. They cover:
  - every line of the embedded TSV parses;
  - no `(ghc, library)` pair appears twice;
  - every release has `base`;
  - 9.12.3 `process` is `1.6.26.1` with source `GhcReleaseTag`;
  - `ships("9.10.3", "x86_64-linux")` excludes `Win32` and includes `unix`;
  - `ships("9.10.3", "x86_64-windows")` includes `Win32`;
  - `ships("99.0.1", …)` is `None`;
  - the TSV header carries `# observed:`, `# sources:` and `# regenerate:` lines (spec US3 acceptance 2).
- [X] T004 Implement `HP/boot_table.rs`, per data-model.md `BootLibraryTable`.
  - Embed the TSV with `include_str!` and load it once into a `OnceLock`.
  - Add `TableSource` and `Platform` enums, and `ships(release, system)`, `version_of(release, name, system)` and `observed()` (the header date).
  - A malformed line is a test failure, never a runtime panic: the loader skips it and counts it, and T003 asserts the count is 0.
  - Register the module in `HP/mod.rs`.
- [X] T005 [P] Write unit tests for `GhcRelease` parsing in `HP/closure_compilers.rs`:
  - `<hash>-ghc-9.10.3` gives `{9.10.3, bootstrap:false}`;
  - `<hash>-ghc-binary-9.8.4` gives `{9.8.4, bootstrap:true}`;
  - `ghc-9.10.3-doc`, `ghc-settings-edit` and `ghc-9.10` give `None`.
- [X] T006 Implement `GhcRelease` and the shared parser in `HP/closure_compilers.rs`. Switch `closure_identity::is_ghc_store_path` (`HP/closure_identity.rs:135`) to call it. The existing tests at `closure_identity.rs:258-297` must still pass unchanged.
- [X] T007 [P] Write unit tests for `ClosureCompilers::from_closure` in `HP/closure_compilers.rs`, using an in-memory `RawDerivation` map:
  - a member's compiler comes from its `nativeBuildInputs`;
  - a member naming two GHCs gets none;
  - `project` is the evaluated derivation's compiler;
  - if the evaluated derivation names none, `project` falls back to the single non-bootstrap `ghc` in the closure;
  - with two non-bootstrap GHCs and no evaluated compiler, `project` is `None`;
  - `seen` collects every distinct compiler.
- [X] T008 Implement `ClosureCompilers::from_closure(&ClassifiedClosure, fallback_system: &str)` in `HP/closure_compilers.rs`, per data-model.md. Use `closure.graph.evaluated` for the project derivation.
  - `system` is the evaluated derivation's `env.system`. If that is absent, use `fallback_system`.
  - Neither `ClassifiedClosure` (`closure/mod.rs:266`) nor `RawDerivation` has a system field. The caller (T010) passes the `NixSystem` detected at `scan_cmd.rs:4556` out of the closure block for the fallback.
  - Add a T007 case for each of the two sources.
- [X] T009 Add `compilers: Option<ClosureCompilers>` to `ResolveOptions` (`HP/mod.rs:481`). Default it to `None`, and update every construction site (`grep -rn 'ResolveOptions {'` across `waybill-cli/src` and `waybill-cli/tests`). With `None`, behaviour is unchanged.
- [X] T010 In `waybill-cli/src/cli/scan_cmd.rs`, derive `ClosureCompilers` from `nix_closure` when it is `Some(Ok(classified))`, after line ~4536 and before `nhp::enrich` at ~4604.
  - Pass the `NixSystem` detected at `:4556` out of the closure block, as T008's fallback.
  - Pass the result in `ResolveOptions.compilers`, and keep it for T017, T022 and T025.

**Checkpoint:** with `compilers` populated but unused, `./scripts/pre-pr.sh` passes, and every golden and test is unchanged.

---

## Phase 3: User Story 1 — a scanner can match the libraries GHC put in the program (P1) 🎯 MVP

**Goal:** when the closure names the compiler, every GHC boot library component carries that compiler's version, with C218, and every edge points at the version its member's compiler shipped.

**Independent test:** a fixture closure built by `ghc-9.10.3` gives `pkg:hackage/base@<table 9.10.3 base>`, with C218 `ghc-9.10.3` and no C170, C171 or C151. A bootstrap-built member's `base` edge points at the bootstrap compiler's version.

### Tests for User Story 1

- [ ] T011 [P] [US1] Create the fixture `T/fixtures/nix_closure_boot_versions/` (`closure.json`, `hackage-packages.nix`, `configuration-ghc-9.10.x.nix`, `flake.lock`, `flake.nix`, and the project `.cabal`). Model it on `T/fixtures/nix_closure_hackage_alias/` and the seeding in `T/nix_closure_hackage_identity.rs:206`.
  - The project `waybill-fixture-app`, built with `/nix/store/<hash>-ghc-9.10.3` in `nativeBuildInputs`, declares `base`, `process`, `Win32` and `waybill-fixture-lib`.
  - The member `waybill-fixture-lib` is built by `ghc-9.10.3` and declares `base`.
  - The member `waybill-fixture-boottool` is built by `ghc-binary-9.8.4` and declares `base`, plus one library whose 9.8.4 and 9.10.3 versions are identical in the TSV (choose it from T002's output and name it in a comment).
  - Separate subdirectories cover three more cases:
    - `absent/`: the compiler is `ghc-99.0.1`.
    - `no_closure/`: the same project, scanned without `--nix-closure`.
    - `closure_only/`: no `.cabal` in the scanned tree, so `enrich` declines, as on shellcheck. Its confirmed members declare `base`, so the versioning comes entirely from the closure tier.
  - Every derivation sets `env.system = "x86_64-linux"`.
  - Names only for boot libraries (fixture rule).
- [ ] T012 [P] [US1] Write `T/nix_closure_boot_versions.rs` tests using `T/common/nix_stub.rs` and the fixture, each asserting all three formats where the row is emitted:
  - `design_tier_base_takes_the_project_compilers_version`: version, PURL, C218 `ghc-9.10.3`, `sbom-tier` still `design`, and no C170, C171 or C151.
  - `bootstrap_member_edge_points_at_its_own_compilers_base`: two `base` components at the 9.10.3 and 9.8.4 versions. `waybill-fixture-boottool`'s edge goes to the 9.8.4 one, and every 9.10.3-built member's edge goes to the 9.10.3 one.
  - `a_library_both_compilers_ship_identically_is_one_component`.
  - `win32_is_never_versioned_on_linux`: on an `x86_64-linux` closure, `Win32` carries no version and no C218.
  - `no_versionless_twin_remains`: no `pkg:hackage/base` without a version beside a C218 one (FR-010), and no relationship endpoint missing from the components (#980).
  - `closure_only_scan_versions_boot_libraries`: on the `closure_only/` fixture, the closure-tier `base` carries the 9.10.3 version and C218, with no C174 in the document.
- [ ] T013 [P] [US1] Write parity tests for C218 in `waybill-cli/src/parity/extractors/` (cdx, spdx2, spdx3 test modules), following an existing per-component row's tests, e.g. C201 `waybill:closure-identity`.

### Implementation for User Story 1

- [X] T014 [US1] In `HP/mod.rs` `enrich`, with `opts.compilers` carrying a `project` release that is in the table: for a declared Haskell dependency whose name is in `ships(project, system)`, produce a **resolved** outcome with the table's version instead of `Unresolved{CompilerSupplied}` (the branch at `HP/mod.rs:287`).
  - Route it through the existing resolved path at `HP/mod.rs:~1060-1083`, so the rename is recorded in `summary.renames` and C170, C171 and C151 are removed (`HP/mod.rs:1650`).
  - Insert C218 (`waybill:boot-library-compiler` = `project.label`).
  - Add the constant `ANN_BOOT_LIBRARY_COMPILER` beside `ANN_UNRESOLVED_REASON` (`HP/mod.rs:475`).
- [X] T015 [US1] In `HP/mod.rs`, count table-resolved boot libraries separately in the summary (`summary.resolved_from_compiler`). Leave the existing `resolved` count unchanged, so C174's meaning does not shift. Log the count at INFO.
- [X] T016 [US1] In `HP/closure_identity.rs` `emit_declared_edges` (`:920`), implement research R7 rule 2. For a confirmed member whose `(pname, version)` has a compiler `g` in `ClosureCompilers.by_member` that is in the table, a declared target `t` with **no rank-2 Haskell build** in the document and `t ∈ ships(g, system)` binds to `pkg:hackage/t@version_of(g, t)`.
  - If no component holds that PURL, create one with `compiler_supplied_component` (`:1073`), extended to take `Option<(&GhcRelease, &str version)>`. When given one, it sets the version and C218 and omits C170.
  - Rank-2 targets, and members with no compiler or one absent from the table, keep today's path exactly.
- [X] T017 [US1] Thread `&ClosureCompilers` and the table into `closure_identity::reclassify` (`HP/closure_identity.rs:825`) and on to `emit_declared_edges`. Update the call at `waybill-cli/src/cli/scan_cmd.rs:4887`. When `None`, behaviour is today's.
- [ ] T018 [US1] Add C218 extractors in `waybill-cli/src/parity/extractors/{mod,cdx,spdx2,spdx3}.rs` (`SymmetricEqual`, per-component), and the catalog row C218 in `docs/reference/sbom-format-mapping.md`, from `contracts/c218-boot-library-compiler.md` with research R6's audit text. Both land together (memory: the mapping-row gate lives in the lib target).

**Checkpoint:** T012's US1 tests pass and the T013 parity tests pass. Scans without a closure are unchanged (`cargo test -p waybill --test nix_haskell_resolution_m926` still passes).

---

## Phase 4: User Story 2 — the compiler-supplied set matches the compiler used (P1)

**Goal:** with the project compiler known and in the table, the boot set is that compiler's `ships()` set for the system, so `os-string` on 9.6.6 is not compiler-supplied.

**Independent test:** a fixture built by `ghc-9.6.6` that declares `os-string` resolves it from the package set, not as `compiler-supplied`. The m1276 exemption set equals `ships(9.6.6, system)`.

### Tests for User Story 2

- [ ] T019 [P] [US2] Add `T/fixtures/nix_closure_boot_versions/os_string_966/`. The project is built by `ghc-9.6.6`, declares `os-string`, and has a `hackage-packages.nix` entry giving `os-string` a version. The `configuration-ghc-9.10.x.nix` there nulls `os-string`, reproducing m926's union over candidate series.
- [ ] T020 [P] [US2] Add tests to `T/nix_closure_boot_versions.rs`:
  - `os_string_is_not_compiler_supplied_on_966`: it resolves to the package-set version, with no C218 and no C170.
  - `the_scope_exemption_uses_the_exact_set`: checked per component, not by count. C202 counts the components the scope rule exempted, not the size of the boot set. `os-string` must carry no C203 `exempt-compiler-supplied`, and a library in `ships(9.6.6)` that the project reaches through a non-runtime path still does.
  - `without_a_closure_the_union_rule_still_applies`: the `no_closure/` fixture shows `os-string` as `compiler-supplied`, as on main.

### Implementation for User Story 2

- [X] T021 [US2] In `HP/mod.rs` `enrich`, when `opts.compilers.project` is in the table, set `boot` to `ships(project, system)` before the classification loop, in place of `boot_set(...)`'s union. That value is what reaches `summary.boot_libraries` (`HP/mod.rs:1365`, feeding m1276) and m926's own exemption. Otherwise keep the union. Leave `boot_libraries.rs` unchanged (fallback).
- [X] T022 [US2] In `waybill-cli/src/cli/scan_cmd.rs:~4873`, pass the exact set to `reclassify` as `boot` when the project compiler is in the table, instead of recomputing the union.
  - Take it from the `enrich` summary (`summary.boot_libraries`) when there is one.
  - When `enrich` declined (a closure-only scan, as on shellcheck: no C174), compute `ships(project, system)` directly.
  - Fall back to today's `boot_set` union only when the project compiler is unknown or absent from the table.

**Checkpoint:** T020 passes. US1 tests still pass.

---

## Phase 5: User Story 3 — a compiler waybill has no data for is reported honestly (P2)

**Goal:** C219 records which compilers were found and whether the table had each one. A compiler absent from the table leaves today's behaviour.

**Independent test:** the `absent/` fixture (`ghc-99.0.1`) gives no C218 anywhere, keeps `compiler-supplied`, and gives C219 `{"compilers":{"ghc-99.0.1":"absent-from-table"},…}`. A non-Haskell closure and a no-closure scan carry no C219.

### Tests for User Story 3

- [ ] T023 [P] [US3] Add tests to `T/nix_closure_boot_versions.rs`:
  - `an_unknown_compiler_changes_nothing_and_says_so`: C219 shape exactly per `contracts/c219-ghc-boot-libraries.md`, with keys sorted.
  - `c219_names_both_compilers_and_the_table_date`: on the main fixture, both compilers are present, `project` is `ghc-9.10.3`, and `table` equals the TSV header date.
  - `no_c219_without_a_closure`.
  - `no_c219_on_a_closure_without_haskell`: reuse an existing non-Haskell closure fixture, such as `T/fixtures/nix_closure_runtime/`.
  - `c219_present_on_a_closure_only_scan`: the `closure_only/` fixture has no C174, but C219 is present and its `compilers` keys equal the document's C218 values.
- [ ] T024 [P] [US3] Write C219 parity tests in the extractor test modules (document scope; follow C202's or C217's tests).

### Implementation for User Story 3

- [ ] T025 [US3] Build the C219 value in `HP/closure_compilers.rs`: `fn document_value(&self, table_date) -> serde_json::Value`, with sorted keys, `schema: 1`, `in-table` / `absent-from-table` per `seen`, and `project` label or `null`.
  - Build it in `waybill-cli/src/cli/scan_cmd.rs` from the `ClosureCompilers` kept in T010, **not** on `enrich`'s summary, which doesn't exist on a closure-only scan.
  - Emit it iff the closure tier ran and either `enrich` returned a summary (C174) or the identity pass examined at least one member (C201). That is the gate in `contracts/c219-ghc-boot-libraries.md`.
- [ ] T026 [US3] Emit C219 at document scope:
  - CDX `metadata.properties[]` in `waybill-cli/src/generate/cyclonedx/metadata.rs`;
  - SPDX 2.3 document `annotations[]` in `waybill-cli/src/generate/spdx/annotations.rs`;
  - SPDX 3 document `Annotation` in `waybill-cli/src/generate/spdx/v3_annotations.rs`.

  Wire the value through the same summary struct C202 and C217 use (`scan_cmd.rs:~5763` and `:~5819`).
- [ ] T027 [US3] Add C219 extractors in `waybill-cli/src/parity/extractors/{mod,cdx,spdx2,spdx3}.rs` (document scope, `SymmetricEqual`), and the catalog row C219 in `docs/reference/sbom-format-mapping.md`, from `contracts/c219-ghc-boot-libraries.md`. Add one sentence to C170's row: on a closure scan with the compiler in the table, a boot library carries C218 instead.

**Checkpoint:** all of `T/nix_closure_boot_versions.rs` passes.

---

## Phase 6: Table verification (FR-004, cross-cutting)

- [ ] T028 Write `T/ghc_boot_libraries_db.rs`, per `contracts/boot-library-table.md` § Verification.
  - Glob `/nix/store/*-ghc-*/lib/ghc-*/lib/package.conf.d` and `$WAYBILL_GHC_PACKAGE_DB_PATHS` (colon-separated). Parse each `.conf`'s `name:` and `version:`.
  - Assert agreement with the TSV, through a test-visible accessor or by parsing the TSV in the test with the same rules.
  - Assert every `all` row for that release is present, and no `windows` row is present for a non-Windows database.
  - Print each database checked with its counts.
  - With zero databases, print `no GHC package database found` and fail iff `WAYBILL_REQUIRE_GHC_PACKAGE_DB=1`. Put file-scope `#![cfg(unix)]` on the file (memory: Unix-only test files).
- [ ] T029 Run T028 locally with `--nocapture` and record the output in `specs/966-ghc-boot-versions/measurements/db-check.txt`. Expect ghc-9.6.6 (37) and ghc-9.10.3 (42, four store paths) to pass.
  - Mutation check: copy the TSV aside with `cp`, change the 9.10.3 `base` version, confirm the test fails, then restore it with `cp`, never `git checkout`. Record both runs in the same file.
- [ ] T030 Set `WAYBILL_REQUIRE_GHC_PACKAGE_DB: "1"` on the Linux corpus job's test step in `.github/workflows/public-corpus.yml`, and add `ghc_boot_libraries_db` to what that job runs, after the corpus build has realised the targets' GHCs.

---

## Phase 7: Corpus and polish

- [ ] T031 Run `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh` (no `+toolchain`), and confirm the per-target `test result: ok` lines.
  - FR-012 check: `git diff --exit-code main -- Cargo.lock` must exit 0.
  - `grep -nE 'std::fs|std::net|reqwest|Command::new' HP/boot_table.rs HP/closure_compilers.rs` must report no match outside `#[cfg(test)]`. The table enters only through `include_str!`.
  - Record both outputs in `specs/966-ghc-boot-versions/measurements/fr012.txt`.
- [ ] T032 Push the branch, then dispatch `gh workflow run public-corpus.yml --ref 966-ghc-boot-versions -f branch=966-ghc-boot-versions -f regen_goldens=true`. Review the golden diff with a normalized, sorted comparison. Record the review in `specs/966-ghc-boot-versions/measurements/golden-review.txt`. Expect:
  - SC-001 counts per target, counted from the regenerated `cdx.json`, recursively;
  - C218 only on boot-library components;
  - C219 only on the five closure targets;
  - `os-string` resolved on openai;
  - no change to any target without `--nix-closure` (SC-004);
  - the database check ran on Linux and printed its databases.
- [ ] T033 Commit the regenerated goldens. Then dispatch a non-regen run (`regen_goldens=false`) and confirm it passes with goldens unchanged.
- [ ] T034 SC-002: run `osv-scanner scan source --sbom <target>/cdx.json --format json` on each of the five regenerated closure goldens, and record the `base` vulnerability IDs in `specs/966-ghc-boot-versions/measurements/sc002-osv.txt`. Expect HSEC-2023-0007 on all five. If any target misses it, find out why before claiming SC-002.
- [ ] T035 [P] Add one paragraph to the Nix closure section of `docs/user-guide/`: boot-library versions come from the compiler the closure names, through a dated table (C218 and C219); an unknown compiler leaves them `compiler-supplied`. Add a CHANGELOG entry under Unreleased.
- [ ] T036 [P] Add a short `specs/966-ghc-boot-versions/measurements/README.md` preamble saying how to re-run each probe, and that `global-hints-2026-11-12.yaml` is the snapshot the spec's numbers came from.
- [ ] T037 Close-out: for every task above, verify its claim against the code or an artifact before ticking it (memory: a task can be unimplemented, not unticked). Re-run the gate if anything changed since T031.

---

## Dependencies & Execution Order

- **Setup (T001–T002)** comes first. T004's embedded data needs T002's output.
- **Foundational (T003–T010)** blocks every story.
  - T003, T005 and T007 are tests and can be written in parallel.
  - T006 must land before T008.
  - T009 must land before T010.
- **US1 (T011–T018):**
  - T011, T012 and T013 can run in parallel.
  - T014 comes before T015.
  - T016 and T017 belong together.
  - T018 can go in parallel with T016 and T017.
- **US2 (T019–T022)** depends on T014, because it shares the `enrich` branch. T021 comes before T022.
- **US3 (T023–T027)** depends on Foundational only. It can run alongside US1 and US2, except that T026 touches `scan_cmd.rs`, which T017 and T022 also edit, so those three run in sequence.
- **Verification (T028–T030)** depends on T004 only, and can start right after Foundational.
- **Corpus and polish (T031–T037)** comes after every story.

### Parallel example (after Foundational)

```text
Agent A: T011 → T012 → T014 → T015 → T016 → T017
Agent B: T013 → T018 → T024 → T027
Agent C: T028 → T029 → T030
```

## Implementation Strategy

- **MVP = Phase 1 + 2 + US1.** Design-tier `base` is versioned on closure scans, which is SC-002's value.
- **US2** corrects the boot set (SC-003).
- **US3** makes staleness visible.
- **Verification** gates the table's correctness.
- **Corpus** proves SC-001 and SC-004 on real closures.

Every phase ends with the pre-PR gate green and the no-closure goldens unchanged.
