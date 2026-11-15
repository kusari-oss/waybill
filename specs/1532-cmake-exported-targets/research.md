# Research: CMake exported targets

Line numbers are at `cba55f7b` (main after #1562). **Read** means the claim was seen in the code; **inferred** means reasoned from it. Every inferred claim a task depends on is re-verified by that task.

## R1 — The input, measured

**Measured** on `kusari-sandbox/waybill-fixture-therock-sdk`. Its capture runs extract every exported-targets file from the ROCm 7.14 dist tarball; the analysis script is in `measurements/targets-census.py`.

**The files:**
- **150 files.** Both conventions occur: `<Pkg>Targets[-<config>].cmake` / `<Pkg>Exports[-<config>].cmake`, and lower-case `<pkg>-targets[-<config>].cmake` (76 files, most ROCm libraries).
- **No lower-case `-exports` files exist.** The first capture run with that pattern failed on it (run 43746630629).
- **Only one configuration**, `release`, across 67 configuration files.

**The locations they name:**
- **914 imported locations**, every one written `${_IMPORT_PREFIX}/…`.
- **910 resolve to a shipped file** and 4 do not.

**How those 910 files would be attributed:**
- 900 are in a directory whose CMake version file produced a #1531 component.
- 3 are in `amd-dbgapi`, a directory with no version file, whose `.pc` gives a same-name component.
- 7 are in packages with no versioned component.

**Decision.** The reader matches all six name patterns:
- `*Targets.cmake`, `*Targets-*.cmake`;
- `*-targets.cmake`, `*-targets-*.cmake`;
- `*Exports.cmake`, `*Exports-*.cmake`.

## R2 — Where it plugs in

**Read.** The #1531 reader (`scan_fs/package_db/installed_lib/`) has the machinery this needs:
- a walker registration (`mod.rs::registration`, basename globs);
- a venv probe that reuses `classify_name` (`venv_candidates`);
- phase-1 records (`finalize`);
- a phase-2 post-pass called after the binary scan (`resolve::resolve`, from `scan_fs/mod.rs`) that holds the live #1531 records with their path and prefix, the OS claims, and the binary entries.

**Decision.**
- **Classification.** A targets classification is added next to `classify_name`. The walker, the venv probe, location rules (spec 1531 FR-006, the CMake search locations) and OS ownership apply unchanged.
- **Phase 1** parses each targets file into a `TargetsFile`: its directory, whether it is the main file or a `<config>` file, its raise count (main file only), and its raw `IMPORTED_LOCATION*` strings.
- **Phase 2** groups them by directory, resolves locations, finds the package component (R4), and appends the files to that entry's `extra_source_paths`.

**Alternative rejected.** A separate reader. It would duplicate the location rules, the ownership check, the root-relative resolution and the post-pass wiring.

## R3 — Parsing (FR-002, FR-003)

**Shape**, from CMake's `install(EXPORT)` generator, as measured in the 150 files:
- **The main file** contains `get_filename_component(_IMPORT_PREFIX "${CMAKE_CURRENT_LIST_FILE}" PATH)`, followed by N lines `get_filename_component(_IMPORT_PREFIX "${_IMPORT_PREFIX}" PATH)`.
- **Each `<config>` file** contains `set_target_properties(<tgt> PROPERTIES … IMPORTED_LOCATION_<CONFIG> "<path>" …)`.

**Decision.**
- **Commands.** Reuse #1531's CMake command tokenizer (`cmake_version.rs::commands`, made `pub(crate)`). It already strips comments and handles quoting.
- **The raise count.** The number of top-level `get_filename_component` commands whose arguments are exactly `_IMPORT_PREFIX "${_IMPORT_PREFIX}" PATH`.
- **Locations.** In `set_target_properties` commands, every argument after `IMPORTED_LOCATION` or `IMPORTED_LOCATION_<CONFIG>`.
- **What a location may contain.** One that starts with `${_IMPORT_PREFIX}/` and contains no other `${` is resolvable. Anything else is `unresolved-location`.
- **Where the raise count comes from.** A `<config>` file takes it from the main file in the same directory. A directory with `<config>` files but no main file is unresolved.

## R4 — Which component a targets file belongs to (FR-004)

**Read.** #1531 components come from live records, `resolve.rs::Live`, each carrying `record.path`, `prefix` and `lname`. A CMake-kind live record's directory is `record.path.parent()`.

**Decision:**
1. A CMake-version live record whose directory equals the targets file's directory.
2. Otherwise, a live record of either kind, with `prefix` equal to the targets file's install prefix and `lname` equal to the directory's lowercased name.
3. Otherwise, none: the files are counted under `no-package-version`.

The targets file's prefix comes from `location::prefix_of` with CMake rules. Both conventions live in the same search directories.

**Note.** The component is the record's entry before the identical-identity merge. Evidence on the CMake entry is unioned into the merged component by the deduplicator (spec 1531 research R4), so where it attaches before the merge does not matter.

## R5 — Evidence and hash reuse (FR-005, FR-005a)

**Read.**
- `manifest_occurrence` (`scan_fs/mod.rs:1821`) hashes each `source_path` / `extra_source_paths` file, with a 256 MB cap, through `manifest_sha_cache`. The cache is created at `:868` and keyed by the absolute path string.
- Binary file components carry the file's SHA-256 in their PURL qualifier, `?file-sha256=` (`binary/entry.rs:598`).

**Decision.**
- **Seeding.** Before the evidence loop, `manifest_sha_cache` is seeded with each binary file component's `source_path` and the `file-sha256` qualifier of its PURL. An attributed shared library or executable is then not hashed a second time. Static archives, for which the binary scan emits nothing (#1534), are hashed through the existing path.
- **Paths.** Evidence paths are root-resolved locations, normalised as every other evidence path is.

**Verified (T002).** The qualifier is the SHA-256 of the whole file: the binary scan reads the whole file with `std::fs::read` (`binary/mod.rs:414`) and skips any file outside its size range. `a_binary_components_file_sha256_is_the_whole_file_digest` compares it with an independent digest.

## R6 — Coverage (FR-007)

**Decision.** `Coverage` gains an `Option<ExportedTargetsCoverage>`, serialised as the key `exported-targets` only when at least one targets file was matched. Its counts are `attached`, `locations`, `no-package-version`, `not-shipped`, `owned-by-os-package`, `read` and `unresolved-location`, with keys sorted. C225's catalog text is extended. A document with no targets file keeps the #1531 wire shape byte for byte.

## R7 — Which goldens move (SC-005)

**Read.** `image-postgres16`'s committed goldens contain no `targets` or `Targets` string. *Inferred:* the image ships no exported-targets file under a path waybill records, and 0 of the 20 git targets carry one (spec 1531 R10). A golden would change only if a `Targets` file exists unrecorded in the image.

**Decision.** Regenerate the goldens in CI and verify with a normalised diff. The predicted result is no golden change.

## R8 — Measuring on the full tree (SC-003, SC-006)

**Inferred, to measure in the task.** The metadata-only fixture has no binaries, so SC-003 and SC-006 need the full 16.55 GB tree. A GitHub-hosted runner's root filesystem may not hold the 3.3 GB tarball and the extracted tree together.

**Decision.** Add a `measure-full` job to the fixture repository:
- it checks free space (`df -h / /mnt`) first, and extracts under whichever filesystem has 25 GB free;
- it builds waybill at the branch and at its base;
- it runs one warm-up per arm, then 3 interleaved scans per arm;
- it reports the scan-time ratio (SC-006) and the attached-file count (SC-003).

If neither filesystem has room, the job extracts only `lib/`, `bin/` and `libexec/`, which hold every named file, and records that it did so.

## Re-verification log

- **R2 (T001).** The CMake reader emits nothing from `cmake/WaybillFixturePTargets.cmake` or `cmake/waybill-fixture-q-targets-release.cmake`, while a `FetchContent_Declare` in the same `cmake/` directory is emitted as the live control. `the_cmake_reader_emits_nothing_from_an_exported_targets_file` passes.
- **R5 (T002).** Verified, as above.
