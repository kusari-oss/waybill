# TheRock fetch census and baseline (SC-007, SC-008)

**Run:** `census-fetches` 44005855500 in `kusari-sandbox/waybill-fixture-therock-sdk`, fixture commit `c9001e9`.
- TheRock is at `b1cae390cc745283dce25581415353f9bc906162` (the pin `measure-source` uses).
- Each top-level submodule is at the commit TheRock records for it.
- The checkouts are sparse and blob-less, holding only `*.cmake`, `CMakeLists.txt` and `.gitmodules`. TheRock was not cloned locally.
- The script is `census/fetches.py`. It reads files and evaluates nothing. It was validated first on a synthetic tree exercising every shape it records.
- waybill `main` is at `7e567e0d` (v0.17.0).

## Re-measured (SC-007)

| | this census | #1516's figure |
|---|---|---|
| CMake files (`CMakeLists.txt`, `*.cmake`) | 6,172 | 6,161 |
| `repo report`: CMake files matched | 6,153 | 6,161 |
| `repo report`: CMake components emitted | 6 | 6 |
| `FetchContent_Declare` calls | 152 | 137 |
| `ExternalProject_Add` calls | 39 | 35 |

The 6 components at `main` are from `find_package` (4), a package-version file (1) and one more. **None is a fetch.** Of the 191 fetch calls, 1 is in today's file set: `cmake/therock_subproject.cmake:181`, an `ExternalProject_Add(${target_name} …)` inside TheRock's own `therock_subproject_fetch` function. Its name is a parameter, so nothing can be emitted for it.

**Scan time of this sparse tree at `main`:** 6.32 s, 6.32 s and 6.32 s (3 runs). This is the SC-006 baseline.

## Where the fetches are

| | count |
|---|---|
| in today's file set | 1 |
| reachable from the root through **literal** `add_subdirectory()` / `include()` | **0**. Of 6,172 files, 51 are reachable, and 229 `add_subdirectory()` calls name a variable. |
| under any `if()` | 148 |
| unconditional | 43 |
| inside a `function()` / `macro()` body | 17. 8 of these have a name containing `${…}`. |
| inside a loop | 0 |
| in a `test` / `tests` / `unittests` directory | 28 |

By top directory: `rocm-libraries/projects` 111, `rocm-systems/projects` 33, `rocm-libraries/shared` 15, `compiler/amd-llvm` 11, and 21 elsewhere.

## Fallback shapes

| shape | count |
|---|---|
| `FIND_PACKAGE_ARGS` | 5 |
| enclosing `NOT TARGET …` | 32 |
| enclosing `BUILD_TESTING` | **0**. TheRock uses project options: `BUILD_TEST` ×7, `USER_BUILD_TEST` ×6, and `USER_BUILD_TEST OR BUILD_HIPSTDPAR_TEST` ×8. |
| `set(FETCHCONTENT_TRY_FIND_PACKAGE_MODE …)` | 0 |
| `cmake_language(SET_DEPENDENCY_PROVIDER …)` | 1: `cmake/therock_subproject_dep_provider.cmake:82`, `SUPPORTED_METHODS FIND_PACKAGE` only |
| a FetchContent name also declared by an enclosing directory | **0** |
| FetchContent names declared in more than one file | 14. Examples: googletest in 38 files, rocm-cmake in 31, prim in 9, catch2 in 6. All are siblings. |

**The commonest conditions:**

| count | condition |
|---|---|
| 32 | `NOT ROCmCMakeBuildTools_FOUND` |
| 20 | `NOT TARGET GTest::GTest AND NOT TARGET GTest::gtest` |
| 14 | `NOT GTest_FOUND` |
| 8 | `USER_BUILD_TEST OR BUILD_HIPSTDPAR_TEST` |
| 8 | `ROCROLLER_ENABLE_FETCH` |
| 7 | `BUILD_TEST` |
| 6 | `NOT ROCM_FOUND` |
| 5 | `NOT Catch2_FOUND` |

**The fallback TheRock actually uses most is `if(NOT <Pkg>_FOUND)`** after a `find_package`. The spec's four guard shapes did not include it (research R3).

## Nested find_package (SC-008)

These calls are outside today's file set, so this feature does not read them. The counts go to the follow-up issue.

| | count |
|---|---|
| `find_package` | 2,030 |
| `pkg_check_modules` | 42 |
| `pkg_search_module` | 3 |
| `find_package` inside today's file set | 9 |
