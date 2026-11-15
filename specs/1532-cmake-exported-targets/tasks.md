# Tasks: Attribute installed files to their package through CMake exported targets

**Input**: `specs/1532-cmake-exported-targets/` — spec.md, plan.md, research.md (R1–R8), data-model.md, contracts/wire.md, quickstart.md
**Tests**: required. The success criteria are test-defined, and the plan's Approach is test-first.

**Rules for every task**:
- Fixtures use `waybill-fixture-*` names in tempdirs, with no network.
- Never run `cargo fmt`. Never pipe a check through `head`. Revert a mutation with `cp` from a saved copy.
- Gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with no `+toolchain`.
- Nothing from a TheRock tarball is fetched or scanned locally.

## Phase 1: Setup and re-verification

- [X] T001 [P] Extend `waybill-cli/tests/installed_lib_cmake_reader_inert.rs` with a case asserting the CMake reader emits no component from an exported-targets file (research R2). Use a `cmake/WaybillFixturePTargets.cmake` and a `cmake/waybill-fixture-q-targets-release.cmake`, placed where the CMake reader does parse, beside the existing live control. Run it on main first; it must pass. If it fails, record the finding in research R2 and stop.
- [X] T002 [P] Write a test in `waybill-cli/tests/installed_lib_targets.rs` that confirms research R5's inferred claim: the `file-sha256` qualifier of a binary file component equals the SHA-256 of the same file. Use `common::installed_lib::elf_shared_lib`, and compute the reference digest in the test with `sha2`, which is already a dev-dependency of `waybill-cli`; check `Cargo.toml` before relying on it. If the two differ, stop and record the finding in research R5.

## Phase 2: Foundational

- [X] T003 Make `commands()` and the `Arg` type in `waybill-cli/src/scan_fs/package_db/installed_lib/cmake_version.rs` `pub(crate)`, with no behaviour change.
- [X] T004 Extend `classify_name` in `waybill-cli/src/scan_fs/package_db/installed_lib/mod.rs` with `RecordKind::ExportedTargets` for the six patterns in research R1: `*Targets.cmake`, `*Targets-*.cmake`, `*-targets.cmake`, `*-targets-*.cmake`, `*Exports.cmake`, `*Exports-*.cmake`. Add those globs to `registration()`. `venv_candidates`' `collect_matching` uses `classify_name`, so it picks them up. Extend the `classify_name_selects_kind_and_stem` unit test with:
  - every pattern;
  - non-matches: `FooTargets.cmake.in`, `targets.cmake`, `FooConfig.cmake`;
  - the existing #1531 kinds, which must classify unchanged.

## Phase 3: User Story 1 — every file a versioned package exports is attributed to it (P1, MVP)

**Independent test**: SC-001. In a synthetic prefix, the package component's evidence lists a shared library, a static archive with an unrelated name, and an executable.

### Tests first

- [X] T005 [P] [US1] Write unit tests for the targets parser in the new file `waybill-cli/src/scan_fs/package_db/installed_lib/targets.rs`:
  - a CMake-generated main file with 3 raise lines gives `raise = Some(3)`;
  - a `<config>` file gives its `IMPORTED_LOCATION_RELEASE` strings in order;
  - a file with both `IMPORTED_LOCATION` and `IMPORTED_LOCATION_DEBUG` gives both;
  - a commented-out `# set_target_properties(...)` is ignored;
  - `INTERFACE_*` properties and `IMPORTED_SONAME` / `IMPORTED_IMPLIB` values are not locations;
  - a truncated statement yields what was read before it, without panicking;
  - the role is `Main` for `XTargets.cmake` / `x-targets.cmake` / `XExports.cmake`, and `Config("release")` for `XTargets-release.cmake` / `x-targets-release.cmake`.
- [X] T006 [P] [US1] Write integration tests in `waybill-cli/tests/installed_lib_targets.rs` covering US1 scenarios 1–5 and SC-001:
  - the `WaybillFixtureP` package from `quickstart.md`, plus a shared library `lib/libwaybill-fixture-p.so.4` built with `elf_shared_lib`;
  - the lower-case `waybill-fixture-q` package;
  - an `Exports` package;
  - one location naming a file that is not shipped;
  - the shared library also matched by #1531's file-name rule.

  Assert, in all three formats:
  - the exact evidence locations of each package component (CycloneDX `evidence.occurrences[].location`; C18 through `row_all_formats(&docs, "C18")`);
  - that a file appears once;
  - the exact `exported-targets` object in C225.

  Control: the same tree with the targets files removed has none of those evidence entries, and no `exported-targets` key.

### Implementation

- [X] T007 [US1] Implement `targets.rs`: `parse(content) -> TargetsFile { raise: Option<usize>, locations: Vec<String> }`, using `cmake_version::commands`, per research R3. T005 must pass.
- [X] T008 [US1] Phase 1 in `installed_lib/mod.rs`. `finalize` returns `TargetsFile` records next to the #1531 records, through a new `targets` field on the records container. Each carries `path`, `dir`, `role`, `prefix` (from `location::prefix_of` with CMake rules), `resolved` (root-relative) and the parsed fields. Thread them to phase 2 the same way as the #1531 records, through `DbScanResult`.
- [X] T009 [US1] Phase 2 in `installed_lib/resolve.rs`, per the data model:
  - group targets by `dir`, and take the raise count from the `Main` file;
  - resolve each location: `${_IMPORT_PREFIX}/` only, then `location::resolve_in_root`, then an existence check;
  - associate the package: the version-record live entry in the same directory, else a same-prefix, same-`lname` live entry;
  - append the attributed absolute paths to that entry's `extra_source_paths`, deduplicated and sorted;
  - count `ExportedTargetsCoverage`;
  - skip OS-owned targets files with the #1531 `os_owned` check.

  T006 must pass.
- [X] T010 [US1] Extend `Coverage::wire` in `resolve.rs` with the optional `exported-targets` object, keys sorted, present only when `read + owned-by-os-package > 0`. Extend the unit test `the_wire_value_lists_every_reason_with_sorted_keys` with a case including the object, and keep the existing case byte-identical.
- [X] T011 [US1] Hash reuse (FR-005a, research R5) in `waybill-cli/src/scan_fs/mod.rs` and `installed_lib/resolve.rs`.
  - **Seed the cache.** Before the evidence loop that calls `manifest_occurrence` (~`:1056`), seed `manifest_sha_cache` from every `db_entries` entry whose PURL carries a `file-sha256` qualifier: key `source_path`, value `Some(hex)`.
  - **Make the keys match** (analysis U2). In phase 2, when a resolved location is the same file as a binary file component, by `(dev, ino)` on unix and canonical path otherwise, put that component's exact `source_path` string into `extra_source_paths`. *(Implemented, then removed: mutation M7 showed the two spellings already agree. See `measurements/mutations.md`.)*
  - **Make reuse observable** (analysis U1). Count lookups that hit a seeded entry, and log `hash cache: seeded=<n> reused=<m>` once at INFO.
  - **Test,** in `installed_lib_targets.rs`. It asserts both that the attributed shared library's occurrence `sha256` equals its binary component's `file-sha256` (which proves the digest matches), and that the scan log reports `reused` ≥ 1 for a tree with one attributed shared library (which proves the hash was not recomputed). The second assertion is the one T014's no-reuse mutation must fail.

**Checkpoint**: US1 alone ships the attribution.

## Phase 4: User Story 2 — nothing is attributed without a versioned package, and nothing is guessed (P1)

**Independent test**: SC-002. Each uncertainty yields zero attributions and one count under its reason.

- [X] T012 [P] [US2] Write integration tests in `waybill-cli/tests/installed_lib_targets.rs`, covering US2 scenarios 1–5. Each case is a separate prefix:
  - a computed version file beside its targets, giving `no-package-version`;
  - a `.pc`-only package `waybill-fixture-r` (`lib/pkgconfig/waybill-fixture-r.pc`) with `lib/cmake/waybill-fixture-r/waybill-fixture-r-targets*.cmake` and no version file, which attaches through the same-name rule;
  - `${CMAKE_INSTALL_PREFIX}/lib/x.so` and `${FOO}/x.so`, each giving `unresolved-location`;
  - `${_IMPORT_PREFIX}/../../../../outside.so` with that file existing outside the root, giving `unresolved-location`, never read and never listed;
  - a dpkg `.list` owning the targets file, giving `owned-by-os-package`, with no attribution;
  - a `<config>` file with no main file in its directory, giving `unresolved-location` (FR-003);
  - two packages in different directories naming the same shipped file. It is listed under both, and `attached` counts 2 (analysis I1).
- [X] T013 [US2] Make T012 pass. The logic is already in T009; this task fixes what the tests expose.

## Phase 5: Polish

- [X] T014 Run the mutations from plan.md Approach step 6, recording each failing test and message in `specs/1532-cmake-exported-targets/measurements/mutations.md`. The no-reuse mutation, which removes the cache seeding, must fail T011's `reused` assertion. Run the harness under bash with `--no-fail-fast` and parse the `---- <name> stdout ----` blocks; that is the spec 1531 lesson.
- [X] T015 Byte-identity tests in `installed_lib_targets.rs` (SC-004):
  - ten scans of the US1 tree are identical, with CycloneDX `serialNumber` masked;
  - a tree with only #1531 files (no targets) produces C225 without `exported-targets`, byte-identical to its pre-feature shape. Compare against a literal expected string.
- [X] T016 Docs:
  - extend C225's row in `docs/reference/sbom-format-mapping.md` with the `exported-targets` object and its counts;
  - add a sentence to the installed-library section of `docs/ecosystems.md`;
  - add a CHANGELOG entry under Unreleased `### Added`, separated from other unreleased entries.
- [X] T017 Gate. Record `GATE_RESULT` and the `test result: ok.` count in `specs/1532-cmake-exported-targets/measurements/gate-log.md`.
- [X] T018 Push the branch. Dispatch `public-corpus.yml` with `regen_goldens=true` on it. Compare all goldens byte for byte, then do a normalised diff of any that differ. The prediction is that none change (research R7). Record the result in `measurements/golden-review.md`, and commit any regenerated goldens only after review.
- [X] T019 ROCm measurement, CI only (SC-003, SC-006, research R8). In `kusari-sandbox/waybill-fixture-therock-sdk`, add a `measure-full` job to `.github/workflows/measure.yml`. It:
  - checks `df -h / /mnt`;
  - downloads the pinned tarball, with the same URL, version id and size check as `capture`;
  - extracts it where 25 GB is free, or else only `lib/`, `bin/` and `libexec/`, recording which;
  - builds waybill at `waybill_ref` and `base_ref`;
  - runs one warm-up per arm, then 3 interleaved scans per arm (`--offline --no-deep-hash`);
  - reports median times and their ratio, the `exported-targets` C225 object, and the number of files in package components' evidence that come from exported targets.

  Record the result in `specs/1532-cmake-exported-targets/measurements/therock-full.md`. SC-003 expects 903 attached, 4 `not-shipped` and 7 `no-package-version` (the census counts distinct shipped files). SC-006 expects a ratio of at most 1.25. If it exceeds that, implement FR-005a's path-only fallback for attributed static archives, re-measure, and record both.
- [X] T020 Close-out. Verify each task's claim against the code, tick `tasks.md`, and draft the PR body with the attribution line.

## Dependencies

- **T001 and T002** first. Both must hold, or the plan changes.
- **T003 and T004**, then the US1 tests (T005 and T006, in parallel), then T007 → T008 → T009 → T010 → T011.
- **US2:** T012 can be written alongside US1. T013 needs T009.
- **Polish:** T014 and T015 follow both stories. T018 and T019 need the pushed branch. T020 comes last.

## Parallel examples

- **T001 and T002.** T005 and T006 (in different files). T012 alongside US1's implementation.
- **T019's workflow change** lives in the fixture repository, so it can be written while T014–T017 run.

## Implementation strategy

- **MVP: US1 plus coverage (T001–T011).** This is the attribution itself.
- **Then US2's negative paths (T012–T013).** They are mostly tests over T009's logic.
- **Polish:** mutations, the gate, goldens, and the CI measurement that settles SC-003 and SC-006.
