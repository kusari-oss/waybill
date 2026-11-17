# Quickstart: check the feature by hand

```sh
mkdir -p /tmp/ww/cmake /tmp/ww/third-party/zstd /tmp/ww/third-party/llvm && cd /tmp/ww
cat > CMakeLists.txt <<'C'
cmake_minimum_required(VERSION 3.25)
project(waybill_fixture_superbuild)
include(ExternalProject)
include(cmake/wrap.cmake)
add_subdirectory(third-party/zstd)
add_subdirectory(third-party/llvm)
C
cat > cmake/wrap.cmake <<'C'
function(wf_fetch target_name)
  cmake_parse_arguments(PARSE_ARGV 1 ARG "CMAKE_PROJECT" "SOURCE_DIR;PREFIX" "TOUCH")
  if(NOT DEFINED ARG_PREFIX)
    set(ARG_PREFIX "${CMAKE_CURRENT_BINARY_DIR}/${target_name}_fetch")
  endif()
  ExternalProject_Add("${target_name}" PREFIX "${ARG_PREFIX}" SOURCE_DIR "${ARG_SOURCE_DIR}"
    CONFIGURE_COMMAND "" BUILD_COMMAND "" INSTALL_COMMAND "" ${ARG_UNPARSED_ARGUMENTS})
endfunction()
C
cat > third-party/zstd/CMakeLists.txt <<'C'
wf_fetch(waybill-fixture-zstd-sources
  SOURCE_DIR "${CMAKE_CURRENT_BINARY_DIR}/source"
  URL "https://mirror.invalid/waybill-fixture-zstd-1.5.7.tar.gz"
  URL_HASH "SHA256=1111111111111111111111111111111111111111111111111111111111111111"
  TOUCH "${CMAKE_CURRENT_BINARY_DIR}/stamp")
C
echo 'wf_fetch(waybill-fixture-runtimes SOURCE_DIR "${CMAKE_SOURCE_DIR}/runtimes")' > third-party/llvm/CMakeLists.txt
waybill --offline sbom scan --path . --root-name ww --root-version 1 --output ww.cdx.json
```

**Expect:**
- One fetch component, `pkg:generic/waybill-fixture-zstd@1.5.7`:
  - with the SHA-256;
  - `waybill:source-mechanism` = `cmake-externalproject`;
  - `waybill:cmake-fetch-wrapper` = `[["wf_fetch"]]`;
  - its source file is `third-party/zstd/CMakeLists.txt`, the call site, not `cmake/wrap.cmake`.
- No component for `waybill-fixture-runtimes`: it names no URL.
- The document's `waybill:cmake-fetch-coverage`:
  - `read` = 2, `emitted` = 1, `unresolved.identity` = 1;
  - `wrappers` = `{"ambiguous-calls":0,"calls":2,"definitions":1}`.

```sh
jq -c '[.components[] | {purl, hashes, p: [.properties[]? | select(.name|test("cmake-fetch|source-mechanism")) | "\(.name)=\(.value)"]}]' ww.cdx.json
jq -r '.metadata.properties[] | select(.name=="waybill:cmake-fetch-coverage") | .value' ww.cdx.json
```

The binding rules can be re-measured with a real CMake: `specs/1539-cmake-fetch-wrappers/measurements/cmake-semantics/probe.sh <scratch-dir>`. It uses no network and serves its tarballs from localhost.

TheRock is measured only in CI. Run `census-fetches` in the fixture repository with `waybill_ref` set to the branch, and compare with `measurements/census.md`'s prediction.
