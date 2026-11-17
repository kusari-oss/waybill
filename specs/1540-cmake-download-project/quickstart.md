# Quickstart: check the feature by hand

The DownloadProject-shaped module below is written for this check; it is not a copy of the upstream file.

```sh
mkdir -p /tmp/dp/cmake /tmp/dp/deps && cd /tmp/dp
cat > cmake/DownloadProject.cmake <<'C'
set(_DownloadProjectDir "${CMAKE_CURRENT_LIST_DIR}")
function(download_project)
  set(options QUIET)
  set(oneValueArgs PROJ PREFIX DOWNLOAD_DIR SOURCE_DIR BINARY_DIR BUILD_PROJECT)
  set(multiValueArgs "")
  cmake_parse_arguments(DL_ARGS "${options}" "${oneValueArgs}" "${multiValueArgs}" ${ARGN})
  configure_file("${_DownloadProjectDir}/DownloadProject.CMakeLists.cmake.in" "${DL_ARGS_DOWNLOAD_DIR}/CMakeLists.txt")
  execute_process(COMMAND ${CMAKE_COMMAND} . WORKING_DIRECTORY "${DL_ARGS_DOWNLOAD_DIR}")
endfunction()
C
cat > CMakeLists.txt <<'C'
cmake_minimum_required(VERSION 3.18)
project(waybill_fixture_dp NONE)
include(cmake/DownloadProject.cmake)
if(WITH_DEP)
  download_project(PROJ waybill-fixture-r
    GIT_REPOSITORY https://github.com/waybill-org/waybill-fixture-r.git
    GIT_TAG v1.2 BUILD_PROJECT TRUE QUIET)
endif()
C
waybill --offline sbom scan --path . --root-name dp --root-version 1 --output dp.cdx.json
```

**Expect:**
- One fetch component, `pkg:github/waybill-org/waybill-fixture-r@v1.2`:
  - `scope: excluded`, `waybill:optional-derivation` = `cmake-conditional`;
  - `waybill:cmake-fetch-wrapper` = `[["download_project"]]`;
  - source file `CMakeLists.txt`.
- The document's `waybill:cmake-fetch-coverage`:
  - `read` = 1, `emitted` = 1;
  - `wrappers` = `{"ambiguous-calls":0,"calls":1,"definitions":1}`.

```sh
jq -c '[.components[] | {purl, scope, p: [.properties[]? | select(.name|test("cmake-fetch|optional-derivation")) | "\(.name)=\(.value)"]}]' dp.cdx.json
jq -r '.metadata.properties[] | select(.name=="waybill:cmake-fetch-coverage") | .value' dp.cdx.json
```

To re-measure what CMake fetches: `specs/1540-cmake-download-project/measurements/cmake-semantics/probe.sh <scratch-dir>`. It uses localhost tarballs and no network.

TheRock is measured only in CI: run `census-fetches` in the fixture repository with `waybill_ref` set to the branch and `base_ref` to `main`.
