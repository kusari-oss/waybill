# TheRock fetch-wrapper census (SC-001)

**Run:** `census-fetches` 44096336322 in `kusari-sandbox/waybill-fixture-therock-sdk`, fixture commit `15e06bd`.
- TheRock is at `b1cae390cc745283dce25581415353f9bc906162`, and each top-level submodule is at the commit TheRock records for it (`census-refs.txt`).
- The checkouts are sparse and blob-less, holding only `*.cmake`, `CMakeLists.txt` and `.gitmodules`. TheRock was not cloned locally.
- The script is `census/wrappers.py` in the fixture repository, copied here as `census-wrappers.py`. It reads files and evaluates nothing. It was validated first on a synthetic tree with every shape it records, a nested wrapper, a loop call, an in-tree wrapper, a literal-only function and a wrapper defined twice with different bodies.
- Its output is `census-wrappers.json`.
- The same run scanned the tree with waybill `main` at `b8ba7e4` (spec 1538 landed).

Four call sites were read from TheRock at the pin through GitHub's contents API, one file each, to see the `set()` their URLs use.

## Definitions

| | count |
|---|---|
| CMake files | 6,172 |
| functions or macros whose body calls a fetch built-in or another such function | 18 names, 19 definitions |
| … of which forward the caller's arguments (**wrappers**) | **7**, 1 definition each |
| … which call a fetch with only literal or non-parameter arguments (not wrappers) | 11 names, 12 definitions |
| wrappers defined more than once | **0**. `rocm_libs_declare_shared_deps` is defined twice with identical bodies, and is not a wrapper. |

**The seven wrappers and how they forward:**

| wrapper | defined in | forwarding shapes | calls |
|---|---|---|---|
| `therock_subproject_fetch` | `cmake/therock_subproject.cmake:147` | name parameter; `${ARG_UNPARSED_ARGUMENTS}` after `cmake_parse_arguments(PARSE_ARGV 1 ARG …)`; parsed keywords re-passed by name (`SOURCE_DIR ${ARG_SOURCE_DIR}`, `PREFIX ${ARG_PREFIX}`) | 40 |
| `llvm_ExternalProject_Add` | `compiler/amd-llvm/llvm/cmake/modules/LLVMExternalProjectUtils.cmake:73` | name parameter; parsed keywords re-passed by name | 8 top-level, 4 from other wrappers |
| `add_custom_libcxx` (macro) | `compiler/amd-llvm/compiler-rt/cmake/Modules/AddCompilerRT.cmake:650` | name parameter; parsed variables | 3 |
| `builtin_default_target`, `builtin_register_target`, `runtime_default_target`, `runtime_register_target` | `compiler/amd-llvm/llvm/runtimes/CMakeLists.txt` | call `llvm_ExternalProject_Add` with parsed keywords re-passed by name (nested, depth 2) | 1, 1, 2, 2 |

`${ARGN}` and `${ARGV}` reach a fetch **directly** in none of them. LLVM's wrappers pass `${ARGN}` to `cmake_parse_arguments` and re-pass keywords by name.

`therock_subproject_fetch` sets literal defaults for `ARG_EXCLUDE_FROM_ALL`, `ARG_PREFIX` and `ARG_SOURCE_DIR`, and passes a body-local `${_extra}`. None of them affects identity.

## Call sites

| | count |
|---|---|
| wrapper call sites | 57 |
| … inside another wrapper's body (part of that wrapper, not a call site) | 4 |
| **top-level call sites** | **53** |
| … passing a literal `URL` or `GIT_REPOSITORY` | 36, all `therock_subproject_fetch` |
| … passing a `URL` built from a variable | 4, all `therock_subproject_fetch`. All four variables have a same-file literal `set()`, which the reader already expands. |
| … passing neither (in-tree builds) | 13, all LLVM's wrappers |
| … with a name built from a variable | 7, all LLVM's, and none has an identity |
| … inside a loop | 6, all LLVM's |
| … under an `if()` | 38. 25 of the 40 `therock_subproject_fetch` calls. |

**`therock_subproject_fetch`:**
- 40 calls, all with `URL_HASH SHA256=…`, over **39 distinct URLs**. `third-party/fftw3` calls it twice with the same URL under two conditions.
- Every URL is on TheRock's S3 mirror (`rocm-third-party-deps.s3.us-east-2.amazonaws.com`).

**FR-005 naming, applied by the census script to the 36 literal URLs** (the reader expands the other 4 first):
- 34 parse as `<name>-<version>`. Examples: `zstd-1.5.7.tar.gz` → `zstd`; `grpc-v1.78.1.tar.gz` → `grpc` at 1.78.1; `json-3.12.0.tar.gz` (nlohmann-json) → `json`; `binutils-2.46.0.tar.gz` (libiberty) → `binutils`; `libdrm-libdrm-2.4.134.tar.bz2` → `libdrm-libdrm`.
- 2 do not: `OpenBLAS-18638c7.tar.gz` and `libbacktrace-<40 hex>.zip` carry a commit, not a version. They keep the bound name, `therock-OpenBLAS-sources` and `therock-libbacktrace-sources`, at `unknown`.

Of the four variable URLs:
- `openmpi-${OPENMPI_VERSION}` (5.0.9), `simde-${SIMDE_VERSION}` (0.8.2) and `${_xz_url}` (`xz-5.8.1.tar.bz2`) expand to parseable names;
- `sqlite-amalgamation-${SQLITE3_AMALGAMATION_VERSION}` expands to `…-3510300.zip`, SQLite's `MMNNPP00` encoding, which carries no dotted version. It keeps its bound name at `unknown`.

## Commands that pass a `URL` or `GIT_REPOSITORY` but are not wrappers (FR-010)

| command | calls | what it is |
|---|---|---|
| `download_project` | 8 | The DownloadProject module (`cmake/DownloadProject.cmake`, vendored by rocprim and its copies). Its body writes a generated project containing `ExternalProject_Add` through `configure_file()`, so no fetch call is in its body to bind through. Example: `rocm-libraries/projects/hipccl/hipccl2/rocprim/cmake/Dependencies.cmake:211`, `PROJ rocrand GIT_REPOSITORY https://github.com/ROCmSoftwarePlatform/rocRAND.git GIT_TAG develop`. |
| `set_package_properties` | 3 | Not a fetch: `URL` is a package property. |

## waybill `main` on the same tree (baseline)

- C231: `{"components":65,"conditional":42,"emitted":110,"merged":{"existing-component":37,"first-declaration-wins":0},"read":191,"test":9,"unparseable-files":0,"unresolved":{"identity":44}}`.
- 3 of the 44 unresolved declarations are the built-in calls inside wrapper bodies: `therock_subproject_fetch`, `llvm_ExternalProject_Add` and `add_custom_libcxx`.
- Scan time of the sparse tree: 6.687 s, 6.698 s and 6.704 s.

## Prediction for the branch (to be checked by T-census at implementation)

*Inference, from the counts above and the design in `research.md`; not a measurement.*
- `read` = 191 − 3 templates + 53 calls = **241**.
- `unresolved.identity` = 44 − 3 + 13 = **54**.
- 40 more declarations are emitted, as **39** new components: 36 named from the archive, and 3 (OpenBLAS, libbacktrace, sqlite) at their bound name and `unknown`.
- `wrappers` = `{"ambiguous-calls":0,"calls":53,"definitions":7}`.

## Measured at the branch

See `therock.md` (run 44105368718). The prediction above was off in three counts, each traced:
- **`definitions` 9, not 7; `calls` 67, not 53.** This census script counts a parameter as forwarding only when it is the fetch's exact first argument. `rocdecode_add_test_ext` and `rocjpeg_add_test_ext` name their project `test_${TEST_EXE}` and pass `${TEST_SOURCE_DIR}`, so the reader's `Parameter` shape (any position) recognises them, and this script listed them as forwarding nothing. Their 14 calls build in-tree tests, with no URL, and are unresolved.
- **`read` 253 and `unresolved.identity` 66** follow from that, exactly.
- **39 wrapped components, of which 34 are new PURLs.** Five tarballs (`Catch2`, `FunctionalPlus`, `frugally-deep`, `googletest@1.17.0`, `json@3.12.0`) were already components, declared directly elsewhere in the tree at the same version, with the same hash.
