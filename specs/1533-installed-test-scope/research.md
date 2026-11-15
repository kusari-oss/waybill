# Research: test scope for installed trees

**Fixture:** `kusari-sandbox/waybill-fixture-therock-sdk` at capture `f7935cb`, the same pinned tarball as #1531 and #1563. Two captures were added for this feature:
- `ca91afe` → `5ded2fd` captured `CTestTestfile.cmake`;
- `951b8f7` → `f7935cb` captured `*_include.cmake`.

The script is `measurements/test-census.py`, with its output in `measurements/test-census.json`. It reads the fixture clone and its listing, never the tarball.

## R1. What installed CTest files contain (measured)

28 `CTestTestfile.cmake` files ship, with 1,665 `add_test` commands between them.

| Form | `add_test`s | Where | Outcome |
|---|---|---|---|
| `add_test(<name> "../<exe>" args…)`, written by the project "for install time" with relative paths | 1,153 | `bin/<pkg>/` for rocblas, hipcub, rocprim, rocthrust, rocwmma and 15 more | **resolved**, naming 432 shipped files |
| `add_test(<name> "${_ROCPROFSYS_EXE}" …)`, variables set from `$ENV{…}` | 474 | `share/rocprofiler-systems/tests/` | unresolved (variable) |
| `add_test([=[<name>]=] "python3" "-m" "pytest" … "tests/<x>.py")`, CMake-generated | 38 | `libexec/rocprofiler-compute/` | the script arguments resolve under `tests/` |

**`include()` and `subdirs()`.**
- `share/hip/catch_tests/CTestTestfile.cmake` declares its tests only through 54 `include(script/<T>_include.cmake)` lines. Each script computes `EXE_PATH` from `${CMAKE_CURRENT_LIST_FILE}` and calls `catch_discover_tests_impl(TEST_EXECUTABLE ${EXE_PATH}/<T>)`, so the tests are discovered by running each executable at `ctest` time.
- `subdirs()` appears 3 times, and every directory it names has its own CTest file.

**Decision:**
- Parse `add_test`, in both its positional and `NAME … COMMAND …` forms, with #1531's tokenizer. Bracket arguments are already supported.
- The command word is the location, resolved relative to the CTest file's directory.
- For an interpreter (`python`, `python3`, `bash`, `sh`, `perl`, `env`), every non-option argument that resolves to a shipped file is a location. The interpreter itself never is.
- A word containing `${` or `$<`, or an absolute path, is `unresolved`.
- `include()` and `subdirs()` are **not followed**. The walker already finds every CTest file, which makes following `subdirs()` redundant. An `include()`d discovery script cannot be read without evaluating it. Each `include()` is counted as `unresolved`.

**Consequence:** HIP's `catch_tests` (127 files, 0.514 GB) stays unscoped on ROCm. It is counted in C226 (54 unresolved), not guessed.

**Alternatives rejected:**
- *Following `catch_discover_tests_impl(TEST_EXECUTABLE …)` by substituting `EXE_PATH`* would be evaluating CMake. It is narrow, but it is the first step of an interpreter.
- *A `catch_tests` path convention* is a name guess with one witness.

## R2. Where the pass runs, and why that is FR-007 (code)

In `scan_cmd.rs`:
- scope propagation runs at about line 5373 (`propagate_non_runtime_scope`);
- file-tier components join at about line 5720;
- `--exclude-scope` runs at about line 5794, documented as "nothing below this point adds, drops or re-scopes a component".

**Decision:** run the test-scope pass between file-tier emission and `--exclude-scope`.

- **No propagation, by construction.** The pass runs after propagation, so propagation never sees its scopes as seeds. A product library linked only by `rocblas-test` stays unscoped.
- **`--exclude-scope test` still works,** because it runs after the pass.
- **The rejected alternative,** scoping in `scan_fs` and excluding the new C62 values from propagation seeds, needs a second mechanism to guarantee what position gives for free.

**Mutation M2** moves the pass before propagation, and must fail a test with a library linked only by a test executable.

## R3. What each rule reaches on ROCm (measured, regular files in the listing)

| Rule | Files | Bytes |
|---|---|---|
| CTest declaration | 432 | 2.632 GB |
| Exported targets of a test package (`rocprofiler-sdk-tests`) | 22 | 0.188 GB (matches #1563) |
| Path convention, beyond the declarations | 14,465 | 0.236 GB |
| **Total** | **14,919** | **3.056 GB** |

That is 50% of #1564's 6.09 GB census.

These are files, not components. A file the scan emits no component for stays unscoped:
- **Over the file-inventory limit:** three of the four `*_gtest.data` blobs.
- **OS-owned:** none in this tree.

`measure-full` gives the component counts (SC-001 to SC-004).

**rocThrust shows why declarations come first.** Its CTest file names 168 `bin/*.hip` executables (0.764 GB). No name pattern could match them. Its other 164 `bin/test_thrust_*` executables are declared nowhere.

## R4. Name patterns: none adopted (measured, FR-009)

These are the files directly under `bin/` that a candidate matches and no declaration names:

| Candidate | Matches | Already declared | Name-only | Name-only bytes |
|---|---|---|---|---|
| `test_*` | 354 | 190 | 164 (`test_thrust_*`) | 0.353 GB |
| `benchmark_*` | 80 | 0 | 80 | 0.223 GB |
| `*-bench` | 10 | 0 | 10 (`rocblas-bench`, `hipblaslt-bench`, …) | 0.168 GB |
| `*_tests` | 20 | 19 | 1 (`rocshmem_functional_tests`) | 0.108 GB |
| `*-tune` | 1 | 0 | 1 (`rocblas-gemm-tune`) | 0.070 GB |
| `*-test` | 10 | 7 | 3 | 0.025 GB |
| `*_gtest.data` | 4 | 0 | 4 | 1.452 GB (one is under the size limit) |
| `*-tests`, `*_test`, `*_gtest` | 21 | 21 | 0 | 0 |

**Decision:** adopt no name pattern in this feature.
- Every candidate either adds nothing beyond the declarations (the last row), or adds only files that have no independent in-tree check. FR-009 requires that check for every match.
- `*-bench` and `*-tune` match user-facing driver tools, the case #1564 flagged.
- The name-only payload, about 0.95 GB of files with components, stays unscoped. The measurement is committed, so a later feature can adopt a pattern with a reason.

**US3 therefore ships as:**
- the committed measurement;
- a test that no name rule exists: `test_waybill_fixture_op` in `bin/`, undeclared and outside a test directory, stays unscoped.

No C62 value exists for a name rule. A later feature that adopts a pattern adds one.

## R5. Installed-tree recognition and the corpus prediction (code + goldens)

**Signal:** a `--path` scan is `Recognised` on either kind of evidence:
- at least one #1531 pkg-config or CMake-version record in a recognised install location that is not OS-owned (exactly the records C225 counts as `read`);
- or an OS package database (dpkg, rpm, apk, alpm, opkg) that was read and claimed at least one path, so `os_claimed_paths` is non-empty. This was added by clarification: an extracted rootfs is an installed tree.

Image scans are always `Image`.

**Corpus:** of the 21 public-corpus targets, only `image-postgres16` carries C225 today, and it is the only one with OS-package components (142 `pkg:deb`). It is also the only image. No source-tree target has an installed-library record or an OS package database, so none is recognised under either signal.

**Prediction** (inference; verified by the CI golden regeneration):
- every source-tree golden is byte-identical, unless a target ships an installed CTest file or a test package's exported targets, which are not expected in source trees;
- `image-postgres16` changes only if an unclaimed file sits under a conventional test directory. Debian's files are dpkg-owned and excluded (FR-013), so no change is predicted.

## R6. A component with several paths (code)

File-tier components deduplicate by content and record every path in `waybill:file-paths`. A binary file component's paths are in `evidence.source_file_paths`.

**Decision:** scope only when *every* path matches a rule. The recorded reason is the weakest rule across those paths, because the claim is only as strong as its weakest witness. Content shipped both as a test fixture and in `lib/` is product.

**Mutation M1** drops the all-paths rule.

## R7. Coverage row: a new C226 (FR-010)

C225 exists only when installed-library records do, and C223 is a fixed-meaning decimal counter (contracts/wire.md). **Decision:** C226 `waybill:test-scope-coverage`, in the C225 shape: a sorted-key JSON object, emitted only when non-empty.

## R8. SPDX 3 purpose and A13 (code)

`v3_packages.rs` sets `software_primaryPurpose` from `BinaryRole` (m104). For a test-scoped element:
- it becomes `test`;
- the role moves to `software_additionalPurpose: ["<role>"]`.

`parity/extractors/spdx3.rs::spdx3_binary_role` reads only `software_primaryPurpose`. It must fall back to `software_additionalPurpose` when the primary purpose is `test`, or A13's cross-format check breaks for every scoped binary. **Mutation M8** removes the fallback.

## R9. Which exported-targets packages are test packages (measured)

The captured CMake metadata contains two candidates:
- `rocprofiler-sdk-tests` has a version file and a targets file. It matches the suffix rule (`-tests`, `_tests`, `-test`, `_test`).
- `hipdnn_test_sdk` is a library that tests link against. It does not match, correctly.

Two abseil `.pc` files (`absl_*_test_helper_internal`, `absl_random_internal_distribution_test_util`) are pkg-config files, not exported targets, and are out of scope.

## R10. `--include-test-binaries` (#1524, FR-012)

The binaries #1524 skips are those under `binary::discover::TEST_INPUT_DIRS` (`test`, `tests`, `unittests`, `Inputs`).

**Decision:** with the flag, the post-pass scopes binary file components under exactly those directories, recorded as `test-directory`, whether or not the tree is recognised. #1524's set is reused, not copied.

Note the difference: `Inputs` is in #1524's set but is not a path convention in FR-005. It is scoped only through this flag.

**Found at implementation (FR-016):** #1524's skip ran on every `--path` scan, and the ROCm SDK is measured with `--path`. The 22 binaries `rocprofiler-sdk-tests` declares live under `share/rocprofiler-sdk/tests/`, so they would never become components, and SC-001 would hold vacuously. The integration test `us1_declared_test_payload_is_test_scoped` failed for this reason, with no component named `unit-t`.

Recognition needs only the #1531 records and the OS claims, both known before the binary scan. So `scan_path` decides before it, and keeps #1524's skip only for a tree that is not recognised. A source tree, the case #1524 was written for, is unchanged.
