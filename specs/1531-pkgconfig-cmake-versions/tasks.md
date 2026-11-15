# Tasks: Installed C/C++ library versions from pkg-config and CMake version files

**Input**: `specs/1531-pkgconfig-cmake-versions/` — spec.md, plan.md, research.md (R1–R11), data-model.md, contracts/wire.md, quickstart.md
**Tests**: required. The spec's success criteria are test-defined, and the plan's Approach is test-first.

**Format**: `[ID] [P?] [Story] Description`. `[P]` marks a task that touches different files from the other open tasks and depends on none of them.

**Rules for every task**:
- Fixtures use `waybill-fixture-*` names in tempdirs, with no network.
- Never run `cargo fmt`. Never pipe a check through `head`. Revert a mutation with `cp` from a saved copy.
- Gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with no `+toolchain`.
- Test modules that use `.unwrap()` carry `#[cfg_attr(test, allow(clippy::unwrap_used))]`.

## Phase 1: Setup and re-verification

- [X] T001 Re-verify research R4 against current main, and record the result in `specs/1531-pkgconfig-cmake-versions/research.md` under R4 as "re-verified at <sha>" or as a correction:
  - `deduplicate` in `waybill-cli/src/resolve/deduplicator.rs` still groups by `(ecosystem, name, version, parent_purl, namespace)`;
  - it keeps the first entry on equal confidence;
  - it keeps the winner's C55 and `sbom_tier`.
- [X] T002 [P] Capture a real `write_basic_package_version_file` output and save it as the unit-test fixture `waybill-cli/src/scan_fs/package_db/installed_lib/testdata/BasicConfigVersion-SameMajor.cmake`. Generate it in a tempdir by running `cmake` on a three-line project that calls `write_basic_package_version_file(... VERSION 4.5.6 COMPATIBILITY SameMajorVersion)`, if `cmake` is available. Otherwise copy the literal shape from CMake's `Modules/BasicConfigVersion-SameMajorVersion.cmake.in` with `@CVF_VERSION@` filled in. Record which route was used in research R7.
- [X] T003 [P] Decide whether to reuse the existing `waybill:cmake-find-package-name` row for the declared CMake case or to add `waybill:cmake-package-name` (research R5). Read its catalog definition in `docs/reference/sbom-format-mapping.md` and its extractors in `waybill-cli/src/parity/extractors/`. Prefer reuse if the definition is "the declared CMake package name when it differs from the lowercased identity". Record the decision in research R5.
- [X] T004 [P] Confirm the next free catalog rows in `docs/reference/sbom-format-mapping.md`, checking against main and any open PR touching the catalog (`gh pr list --search "sbom-format-mapping"`). PR #1561 claims C223. Write the two chosen numbers into `contracts/wire.md`, replacing "expected C224 and C225".
- [X] T005 Write a test asserting that the CMake reader emits no component from a CMake package version file (research R1), in `waybill-cli/tests/installed_lib_cmake_reader_inert.rs`. Scan a tree holding only `lib/cmake/WaybillFixtureB/WaybillFixtureBConfigVersion.cmake` from T002's fixture. Assert zero components whose C55 starts with `cmake-`, and no `pkg:generic/waybillfixtureb` component. Run it on main before any reader code exists; it must pass. If it fails, stop and record the finding in research R1, since the plan's assumption is wrong.

## Phase 2: Foundational (blocks every story)

- [X] T006 Add the OS-only claim set (research R3) to `waybill-cli/src/scan_fs/package_db/mod.rs`:
  - `DbScanResult` gains `os_claimed_paths: HashSet<PathBuf>` and `os_claimed_inodes: HashSet<(u64,u64)>`;
  - at the dpkg, apk, alpm, opkg and rpm claim call sites (`read_all`, ~`:1692-1782` and `:2044-2054`), insert the same paths and inodes into both sets. Do not insert at the pip `RECORD` or ipk call sites;
  - thread both sets out of `read_all` to `scan_fs::scan_path` in `waybill-cli/src/scan_fs/mod.rs`, unused for now.
- [X] T007 Add a unit test in `waybill-cli/src/scan_fs/package_db/mod.rs`, building a rootfs with a dpkg `.list` naming `usr/lib/x86_64-linux-gnu/pkgconfig/waybill-fixture-z.pc` and a pip `RECORD` naming `site-packages/sdk/lib/pkgconfig/waybill-fixture-w.pc`:
  - the dpkg path is in `os_claimed_paths`;
  - the pip path is in `claimed_paths` but not in `os_claimed_paths`. This is the control showing the sets differ.
- [X] T008 No-op control: run the full gate after T006 and T007. Confirm the `test result: ok.` count rises only by T007's tests, and that `git diff --stat` shows no golden or fixture changes. Record the pass count in `specs/1531-pkgconfig-cmake-versions/measurements/gate-log.md`.
- [X] T009 Add `ReaderId::INSTALLED_LIB` and append it to `ALL_READER_IDS` in `waybill-cli/src/scan_fs/walk_registry/mod.rs`.
- [X] T010 Create the module skeleton `waybill-cli/src/scan_fs/package_db/installed_lib/mod.rs` with `pkgconfig.rs`, `cmake_version.rs`, `location.rs` and `resolve.rs` as empty submodules. Declare `pub mod installed_lib;` in `waybill-cli/src/scan_fs/package_db/mod.rs`. Define `Record`, `RecordKind` and `SkipReason` per `data-model.md`, with `SkipReason::as_str()` as the single serialisation point.

## Phase 3: User Story 1 — every library an installed prefix declares gets its exact version (P1, MVP)

**Goal**: one versioned `pkg:generic` component per recognised `.pc` or CMake version file that no OS package owns.
**Independent test**: the US1 prefix fixture gives exactly the stated components and versions (SC-001).

### Tests (write first; they must fail before the implementation)

- [X] T011 [P] [US1] Write the `.pc` parser unit tests in `waybill-cli/src/scan_fs/package_db/installed_lib/pkgconfig.rs`, covering:
  - a plain `Version: 1.2.3`;
  - `version=2.0.1` plus `Version: ${version}`;
  - a two-level variable chain;
  - an undefined variable, which gives `Err(UnresolvedVersion)`;
  - a variable cycle, which gives `Err(UnresolvedVersion)` and must not hang;
  - `@VERSION@`, which gives `Err(UnresolvedVersion)`;
  - a missing `Version:`, which gives `Err(NoVersion)`;
  - `$$` as a literal `$`;
  - `#` comments;
  - a `\` line continuation;
  - CRLF line endings;
  - non-UTF-8 input or a file over 1 MiB, which gives `Err(Malformed)`.
- [X] T012 [P] [US1] Write the CMake version parser unit tests in `waybill-cli/src/scan_fs/package_db/installed_lib/cmake_version.rs`:
  - T002's real fixture gives `4.5.6`;
  - `set(PACKAGE_VERSION "1.0")` gives `1.0`;
  - `SET(PACKAGE_VERSION 1.0)` (bare, upper case) gives `1.0`;
  - `set(PACKAGE_VERSION ${FOO_VERSION})` gives `Err(ComputedVersion)`;
  - no `PACKAGE_VERSION`, or a version set only inside `if()`, gives `Err(ComputedVersion)`;
  - a commented-out `# set(PACKAGE_VERSION "9")` is ignored.
- [X] T013 [P] [US1] Write the location and prefix unit tests in `waybill-cli/src/scan_fs/package_db/installed_lib/location.rs`, for both recognised and rejected paths (research R7).
  - These are recognised:
    - `opt/sdk/lib/pkgconfig/x.pc` (prefix `opt/sdk`);
    - `usr/lib/x86_64-linux-gnu/pkgconfig/x.pc` (prefix `usr`);
    - `usr/share/pkgconfig/x.pc`;
    - `opt/sdk/lib64/cmake/Foo/FooConfigVersion.cmake` (prefix `opt/sdk`);
    - `opt/sdk/Foo-1.0/cmake/foo-config-version.cmake`.
  - These are rejected as unrecognised:
    - `x.pc` at the scan root;
    - `src/tests/x.pc`;
    - `cmake/FooConfigVersion.cmake` at a repository root with no `lib*` or `share` parent.
  - Template names (`x.pc.in`) never reach the parser, because the glob does not match them.
- [X] T014 [US1] Write the integration test `waybill-cli/tests/installed_lib_us1.rs` (SC-001, SC-005, SC-009). Build a prefix in a tempdir:
  - three `.pc` files, one of them unresolved;
  - two CMake version files, one computed;
  - one `.pc` and one CMake version file stating the same name and version (US1 scenario 5);
  - a source-tree area holding `x.pc.in`, a root-level `x.pc` and `tests/x.pc`;
  - a dpkg database whose `.list` owns `usr/lib/x86_64-linux-gnu/pkgconfig/waybill-fixture-z.pc`, with the file present;
  - a wheel installed into a virtual environment: `.venv/lib/python3.12/site-packages/waybill_fixture_sdk/lib/pkgconfig/waybill-fixture-v.pc`, with a matching `.dist-info/RECORD` listing it. This is spec C1: it must be emitted, although pip claims it.

  Scan with all three formats and assert:
  - the exact set of `pkg:generic/waybill-fixture-*` PURLs and versions;
  - C55 values and tier `deployed`;
  - the merged component has C55 `pkg-config` and both files in its source files;
  - no component comes from the source-tree area, nor from `waybill-fixture-z`.

  Also include a control: the same tree without the dpkg `.list` emits `pkg:generic/waybill-fixture-z@<v>`.

### Implementation

- [X] T015 [P] [US1] Implement the `.pc` parser in `waybill-cli/src/scan_fs/package_db/installed_lib/pkgconfig.rs` (research R7): variables, fields, recursive expansion with cycle detection, continuation and comments. Return `name`, `version: Result<String, SkipReason>`, `requires` and `requires_private` (the grammar is in T019). T011 must pass.
- [X] T016 [P] [US1] Implement the CMake version parser in `waybill-cli/src/scan_fs/package_db/installed_lib/cmake_version.rs` (research R7). It is literal-only and evaluates nothing. T012 must pass.
- [X] T017 [P] [US1] Implement the location and prefix rules in `waybill-cli/src/scan_fs/package_db/installed_lib/location.rs`, including the multiarch-triplet test (research R7). T013 must pass.
- [X] T018 [US1] Implement phase 1 in `waybill-cli/src/scan_fs/package_db/installed_lib/mod.rs` and wire it into `waybill-cli/src/scan_fs/package_db/mod.rs`:
  - register patterns `*.pc`, `*ConfigVersion.cmake` and `*-config-version.cmake` with an `on_file` that only collects paths into `Arc<Mutex<BTreeSet<PathBuf>>>`, following `git_submodule/mod.rs:107-145`. Set `descend_into` to `.venv` and `venv`, following `go_binary.rs:624-637` (research R1, spec C1);
  - register it in `run_shared_walker_pilot`;
  - add a `SharedPilotOutput` field;
  - add `finalize(paths, scan_root) -> Vec<Record>`, which reads each file and applies T015 to T017. Each file is read only after root-relative symlink resolution (T041); an escaping link becomes `malformed`;
  - thread the records out of `read_all` to `scan_path`.
- [X] T019 [US1] Implement phase 2, entries only, in `waybill-cli/src/scan_fs/package_db/installed_lib/resolve.rs`. Call it from `waybill-cli/src/scan_fs/mod.rs` right after the `binary::read` extend (~`:514-521`). The step:
  - drops unrecognised and failed records;
  - drops records owned per `os_claimed_*`, via `binary::is_path_claimed`, or per the Homebrew keg rule (research R3);
  - builds entries per `data-model.md`;
  - inserts them into `db_entries` immediately before the first binary-scan entry (research R4), with `.pc` entries before CMake ones and each group sorted by path.

  T014 must pass.
- [X] T041 [US1] Implement root-relative path resolution in `waybill-cli/src/scan_fs/package_db/installed_lib/location.rs` (research R3, spec FR-012):
  - a lexical join of absolute link targets onto the scan root;
  - `..` normalisation;
  - rejection when a component leaves the root;
  - at most 40 hops.

  T018's finalize and T019's Homebrew keg check use it, and nothing in the reader calls `std::fs::canonicalize`.

  Unit tests, using symlinks built in a tempdir:
  - an absolute target `/opt/sdk/lib/pkgconfig/x.pc` resolves to `<root>/opt/sdk/lib/pkgconfig/x.pc`;
  - a relative target resolves;
  - `../../../../outside.pc` is rejected;
  - a link cycle is rejected.

  This must be done before T018.
- [X] T042 [US1] Write a no-escape test in `waybill-cli/tests/installed_lib_us1.rs` (spec G1, FR-012).
  - **Escaping links:** create `<tmp>/outside/waybill-fixture-x.pc` with `Version: 9.9.9` beside the scan root, and two links to it in `<root>/usr/lib/pkgconfig/`: one relative (`../../../outside/…`) and one absolute (`<tmp>/outside/waybill-fixture-x.pc`, which is outside the root when read root-relatively). Assert that no component has version `9.9.9`, and that the coverage record counts both links as `malformed`.
  - **Positive control:** an absolute link whose root-relative target exists inside the root (`/opt/sdk/lib/pkgconfig/waybill-fixture-y.pc` → `<root>/opt/sdk/lib/pkgconfig/waybill-fixture-y.pc`) is emitted.
- [X] T043 [US1] Write an image-mode test in `waybill-cli/tests/installed_lib_image.rs` (spec FR-014, G2). Build an image tarball in a tempdir with the existing helper pattern in `waybill-cli/tests/binding_spdx_conformance.rs::build_image_tarball` (a docker-archive layout). Its single layer holds `opt/sdk/lib/pkgconfig/waybill-fixture-a.pc`, plus `var/lib/dpkg/status` and `var/lib/dpkg/info/waybill-fixture-z.list` owning `usr/lib/x86_64-linux-gnu/pkgconfig/waybill-fixture-z.pc`. Scan it with `--image`, and assert:
  - `pkg:generic/waybill-fixture-a@<v>` is present;
  - no `waybill-fixture-z` component exists;
  - the coverage record shows `owned-by-os-package: 1`.
- [X] T020 [US1] Add a Homebrew keg ownership test to `waybill-cli/tests/installed_lib_us1.rs`: a `.pc` under `opt/homebrew/Cellar/waybill-fixture-k/1.0/lib/pkgconfig/`, with `INSTALL_RECEIPT.json` in `Cellar/waybill-fixture-k/1.0/` and a symlink from `opt/homebrew/lib/pkgconfig/waybill-fixture-k.pc`. It must produce no component. The control is the same tree without `INSTALL_RECEIPT.json`, which emits it.

**Checkpoint**: US1 is shippable alone. It provides versions for every non-OS-owned library, without edges or binary evidence.

## Phase 4: User Story 2 — the libraries' declared dependencies become edges (P1)

**Goal**: `Requires` and `Requires.private` become `DependsOn` edges by exact PURL, with private-only targets annotated.
**Independent test**: SC-002.

- [X] T021 [P] [US2] Write the `Requires` grammar unit tests in `waybill-cli/src/scan_fs/package_db/installed_lib/pkgconfig.rs`:
  - `a, b` and `a b` give `[a, b]`;
  - `a >= 1.0, b` gives `[a, b]`, so the `1.0` is consumed and is not a module;
  - `a>=1.0` with no spaces gives `[a]`;
  - `a != 2 b < 3` gives `[a, b]`;
  - an empty `Requires:` gives `[]`;
  - duplicates are preserved in order.
- [X] T022 [P] [US2] Write the resolution unit tests in `waybill-cli/src/scan_fs/package_db/installed_lib/resolve.rs` (data-model "Resolution of `Requires`"):
  - same-prefix wins over another prefix;
  - a name unique across the scan resolves;
  - a name with two candidates in other prefixes is unresolved;
  - a missing name is unresolved and counted;
  - an OS-owned target is unresolved;
  - a module in both `Requires` and `Requires.private` gives one edge and is not private.
- [X] T023 [US2] Write the integration test `waybill-cli/tests/installed_lib_us2.rs` (SC-002). Build the US1 prefix plus the `Requires` and `Requires.private` lines from quickstart.md, and one requirement naming a module absent from the scan. In all three formats, assert:
  - the edge set equals the union exactly;
  - `waybill:pkg-config-private-requires` equals the private-only PURLs exactly;
  - no component exists for the missing module;
  - a second prefix holding a different-version `waybill-fixture-c` is not the edge target, because the same prefix wins.
- [X] T024 [US2] Implement the `Requires` grammar in `pkgconfig.rs` (T021) and resolution in `resolve.rs` (T022). Write exact target PURLs into `depends`, sorted and deduplicated, and never write unresolved names (research R6). Set the private-requires annotation per `contracts/wire.md`.
- [X] T025 [US2] Add the `waybill:pkg-config-private-requires` catalog row, using the number from T004:
  - the row and its definition in `docs/reference/sbom-format-mapping.md`;
  - emission in the CycloneDX component properties, the SPDX 2.3 package annotations and the SPDX 3 package annotations (follow an existing component-scope JSON-array row's emitters; find one with `grep -n 'JSON array' docs/reference/sbom-format-mapping.md`);
  - `SymmetricEqual` extractors in `waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3,mod}.rs`.

  `every_catalog_row_has_an_extractor` and the parity tests must pass, and T023 must pass.

## Phase 5: User Story 3 — the binaries in the prefix are tied to their library (P2)

**Goal**: a library component's evidence lists its prefix's matching shared-library files. The binaries' own components are unchanged.
**Independent test**: SC-008.

- [X] T026 [P] [US3] Write the matching unit tests in `resolve.rs`:
  - `libwaybill-fixture-a.so`, `libwaybill-fixture-a.so.1` and `libwaybill-fixture-a.so.1.2.3` match module `waybill-fixture-a`;
  - `libwaybill-fixture-a.dylib` and `libwaybill-fixture-a.1.dylib` match;
  - `libfoo.so.1` matches module `libfoo`;
  - `libwaybill-fixture-ab.so` does not match `waybill-fixture-a`;
  - a binary in another prefix does not match;
  - a binary matching two modules lists under neither.
- [X] T027 [US3] Write the integration test `waybill-cli/tests/installed_lib_us3.rs` (SC-008). In the US1 prefix, add minimal ELF64 `ET_DYN` files at `lib/libwaybill-fixture-a.so.1.2.3` and `lib/libwaybill-fixture-zz.so`; follow the hand-built ELF helper in `waybill-cli/tests/binary_test_inputs.rs` (PR #1561) or the existing binary tests. Assert:
  - `pkg:generic/waybill-fixture-a@1.2.3` lists the `.so` in its source files and occurrences;
  - the `.so`'s own file component is byte-identical to a scan of the same tree with the `.pc` files removed. That is the control showing the binary component is untouched;
  - `libwaybill-fixture-zz.so` is listed under nothing.
- [X] T028 [US3] Implement the matching in `resolve.rs` against `db_entries` whose `binary_role` is a shared library and whose `source_path` is in the record's prefix. Append matches to `extra_source_paths`, sorted (research R9). T026 and T027 must pass.
- [X] T029 [US3] Add a zlib merge test to `waybill-cli/tests/installed_lib_us3.rs` (SC-003). Use a `zlib.pc` with `Version: 1.3.1` beside a binary carrying zlib's embedded version string for `1.3.1`; reuse the string fixture the #1525 tests use (`grep -rn 'deflate 1' waybill-cli/src/scan_fs/binary/`). Assert one `pkg:generic/zlib@1.3.1` component with C55 `pkg-config`, tier `deployed` and the binary's path in its source files. The control: without the `.pc`, the component has tier `analyzed`.
- [X] T040 [US3] Measure the walk cost of entering virtual environments (research R1, spec C1). Build a synthetic `.venv/lib/python3.12/site-packages/` holding 50,000 empty files across 2,000 directories, and no `.pc`. Time a `--path` scan 3 times each with T018's `descend_into` and with it removed, and report min, median and max. Record the result in `specs/1531-pkgconfig-cmake-versions/measurements/venv-walk-timing.md`. If the median rises by more than 20%, restrict descent to `site-packages` subtrees (`descend_into` patterns `.venv`, `venv`, then let `on_dir` ignore everything but `lib/python*/site-packages/**`) and re-measure.
- [X] T030 [US3] Measure the evidence re-hashing risk (research R9). Time a scan of the US3 fixture with `lib/libwaybill-fixture-a.so.1.2.3` replaced by a 200 MiB sparse ELF file, with and without the `.pc`. Take 3 runs each and report min, median and max (memory rule: a single run is not a measurement). Record the times in `specs/1531-pkgconfig-cmake-versions/measurements/rehash-timing.md`. If the `.pc` arm's median is more than 1.5× the other's, take the occurrence hash from the matched binary component's `file-sha256` qualifier instead of re-reading the file, then re-measure.

## Phase 6: Coverage record (cross-cutting; needed by SC-001 and SC-007)

- [X] T031 Implement the coverage record in `resolve.rs` per `data-model.md`: counts per kind, every skip reason with zeros, `unresolved-requires` and `version-disagreements`. It is present only when phase 1 matched at least one file. Log a warning per skip naming the path.
- [X] T032 Add the `waybill:installed-library-coverage` catalog row (number from T004). Emit it in the CycloneDX `metadata.properties`, the SPDX 2.3 document annotations and the SPDX 3 document annotations, following C222 `waybill:git-submodule-coverage`'s emitters, and add `SymmetricEqual` extractors. Extend T014 and T023 to assert the exact coverage JSON in all three formats, including the `owned-by-os-package: 1` from T014's dpkg file.
- [X] T033 Write a byte-identity test in `waybill-cli/tests/installed_lib_us1.rs`. Ten repeated scans of the US1 to US3 fixture are byte-identical in all three formats, and a tree with no matching file carries no coverage record (contracts invariants).

## Phase 7: Polish and cross-cutting

- [X] T034 Run the mutations from plan.md Approach step 7. For each: save with `cp`, apply, run the named tests, record the failing test and message, restore with `cp` and confirm with `cmp`. Record all of them in `specs/1531-pkgconfig-cmake-versions/measurements/mutations.md`, together with any first attempt that tested the wrong thing.
- [X] T035 Docs:
  - add `pkg-config` and `cmake-package-version` to C55's documented values in `docs/reference/sbom-format-mapping.md`;
  - add a CHANGELOG entry under Unreleased `### Added` in `CHANGELOG.md`, separated from any other unreleased entry by at least one existing entry (memory: land-currency CHANGELOG adjacency);
  - document the reader in the user guide where the other installed-package readers are listed (`grep -rln 'dist-info' docs/user-guide/`).
- [X] T036 SC-006 in CI only. Choose a published ROCm SDK payload, either a release tarball or a wheel holding `lib/pkgconfig`, and pin it by digest. Add a branch-only workflow step that fetches it and:
  - counts `*.pc` and CMake version files;
  - scans with and without this feature (the base commit's binary);
  - records the component counts with a version, the coverage record and the scan times.

  Copy the output to `specs/1531-pkgconfig-cmake-versions/measurements/rocm-sdk.md`. Never fetch or scan the payload locally. Remove the step before the PR leaves draft.
- [X] T037 Corpus goldens (SC-007, research R10). Dispatch `public-corpus.yml` regeneration on the branch, and download all goldens. Compare them to the committed ones with a normalised, sorted diff: mask content-addressed IDs, `LC_ALL=C sort` (memory: `feedback_verify_golden_churn_normalized`). Confirm that exactly `image-postgres16`'s three files change, each gaining only the coverage record with `owned-by-os-package: 2`. Commit them, and record the review in `specs/1531-pkgconfig-cmake-versions/measurements/golden-review.md`. If the prediction fails, record why before committing anything.
- [X] T038 Run the gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`. Record `GATE_RESULT` and the `test result: ok.` count in `measurements/gate-log.md`.
- [ ] T039 Close-out. Verify each task's claim against the code, not just its checkbox (memory: a task can be unimplemented, not unticked). Tick `tasks.md`, and draft the PR body ending with the attribution line.

## Dependencies

- **Phase 1:** T001 to T004 are independent. T005 depends on T002.
- **Phase 2:** T006 → T007 → T008. T009 and T010 are independent of T006.
- **US1 (T011–T020):** depends on Phase 2.
  - T011, T012 and T013 run in parallel, then T015, T016 and T017 in parallel.
  - T014 can be written in parallel with them.
  - T041 depends on T017 and comes before T018. T018 depends on T015 to T017 and T041. T019 depends on T018 and T006. T020, T042 and T043 depend on T019.
  - T040 depends on T018. It is listed under US3 because it is a measurement, but nothing in US3 depends on it.
- **US2 (T021–T025):** depends on US1's T019. T021 and T022 can be written in parallel with US1's implementation.
- **US3 (T026–T030):** depends on US1's T019, and is independent of US2.
- **Coverage (T031–T033):** depends on US1. T032 extends T014 and T023, so it follows US2.
- **Polish:** T034 follows every story. T036 and T037 need a pushed branch. T038 and T039 come last.

## Parallel examples

- **After Phase 2:** T011, T012, T013 and T014 (tests), then T015, T016 and T017 (parsers). All are in different files.
- **After T019:** US2 (T021–T025) and US3 (T026–T030) can proceed in parallel, because they touch different functions in `resolve.rs`. Coordinate on that file, or run them sequentially if one person does both.

## Implementation strategy

- **MVP = US1 plus coverage (T001–T020, T031–T033).** This gives versions for every non-OS-owned library and the coverage record. It is shippable alone.
- **Then US2.** Edges are the next-highest value, because `Requires` states the SDK's own structure.
- **Then US3.** This links the shipped binaries to their versions.
- **Polish:** T034 to T039, including CI-only work in T036 and T037.
