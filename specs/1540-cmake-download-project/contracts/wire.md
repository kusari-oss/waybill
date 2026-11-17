# Contract: what the document says about `download_project()` calls

Each case is a synthetic `waybill-fixture-*` tree, scanned with `--offline` and checked in CycloneDX, SPDX 2.3 and SPDX 3. "DP" is a synthetic module with DownloadProject's shape, written for the tests and not copied from upstream, at `cmake/DownloadProject.cmake`:
- `set(options QUIET)`, `set(oneValueArgs PROJ PREFIX DOWNLOAD_DIR SOURCE_DIR BINARY_DIR BUILD_PROJECT)`, `set(multiValueArgs "")`;
- `cmake_parse_arguments(DL_ARGS "${options}" "${oneValueArgs}" "${multiValueArgs}" ${ARGN})`;
- defaults, then `configure_file("${_DownloadProjectDir}/DownloadProject.CMakeLists.cmake.in" …)` and two `execute_process()` calls.

"DP(chain)" means the component carries C234 = `[chain]`, C18 names the call site's file, and it is the component a direct `ExternalProject_Add(<PROJ> <forwarded arguments>)` would give.

| # | Tree | Expected |
|---|---|---|
| D-1 | DP; `download_project(PROJ waybill-fixture-r GIT_REPOSITORY https://github.com/waybill-org/waybill-fixture-r.git GIT_TAG v1.2 BUILD_PROJECT TRUE)` | `pkg:github/waybill-org/waybill-fixture-r@v1.2`, mechanism `cmake-externalproject`, required, DP(`["download_project"]`) (US1-1) |
| D-2 | DP; `download_project(PROJ waybill-fixture-u URL https://example.invalid/waybill-fixture-u-2.0.tar.gz URL_HASH SHA256=<64 hex>)` | `pkg:generic/waybill-fixture-u@2.0` with the hash (US1-2) |
| D-3 | DP; a URL whose archive does not parse (`…/download?id=7`), `PROJ waybill-fixture-np` | `pkg:generic/waybill-fixture-np@unknown`: named `<PROJ>`, not `<PROJ>-download` (FR-004) |
| D-4 | DP; a call passing `PREFIX`, `SOURCE_DIR`, `DOWNLOAD_DIR`, `BINARY_DIR`, `QUIET` and `BUILD_PROJECT FALSE` alongside a URL | the same component as without them: DownloadProject's keywords are consumed, not forwarded (US1-3, probes D2, D4) |
| D-5 | DP; `PROJ` given twice | named by the last (probe D3) |
| D-6 | DP; no `URL`, no `GIT_REPOSITORY` | no component; `unresolved.identity` = 1, `wrappers.calls` = 1 (US1-4, FR-007) |
| D-7 | DP; no `PROJ` | no component; `unresolved.identity` = 1 (FR-007) |
| D-8 | DP modified to add `EXTRA_KW` to `oneValueArgs`; a call passing `EXTRA_KW x` and a URL | the component, with `EXTRA_KW` consumed (probe D5a) |
| D-9 | DP with `oneValueArgs` set under `if(X)`; any call | no component; `unresolved.identity` = 1: an unresolved keyword list is not guessed (clarification) |
| D-10 | a function named `download_project` without DP's shape (no `DL_ARGS`, no template), forwarding nothing | its calls are not read; no C231 change (FR-001) |
| D-11 | DP in `a/`, and a function named `download_project` in `b/` without the shape; one call with a URL | no component; `wrappers.ambiguous-calls` = 1 (FR-001, spec 1539 FR-002) |
| D-12 | DP excluded with `--exclude-path` | the call is not recognised, and counted nowhere (edge case) |
| D-13 | DP; a call under `if(NOT WaybillFixtureR_FOUND)` | Optional(`cmake-find-package-fallback`); C232 is the call site's condition only (the template's `if` is not modelled) |
| D-14 | DP; a call in `tests/CMakeLists.txt` | test scope, C62 `source-test-directory` |
| D-15 | DP; a project function `add_dep(name)` that calls `download_project(PROJ ${name} ${ARGN})`; called with a GitHub repository | DP(`["add_dep","download_project"]`): a spec 1539 wrapper of it |
| D-16 | DP; two calls of one repository and tag in sibling directories | one component (one per PURL), with both files in C18 |
| D-17 | DP; a call with a URL given twice | the first URL's component (probe D6) |
| D-18 | spec 1539 wrapper whose `cmake_parse_arguments` keyword lists are variables (`set(one URL NAME)`, `"${one}"`) | binds with the resolved keywords: `URL` is consumed and re-passed as spec 1539 binds it |

Spec 1538's C-1 to C-15 and spec 1539's W-1 to W-26 still hold, unchanged.

## Invariants

- **I-1 to I-7:** specs 1538 and 1539's, unchanged.
- **I-8:** a component carries a C234 chain ending in `download_project` iff a `download_project` call is among the declarations it stands for.
