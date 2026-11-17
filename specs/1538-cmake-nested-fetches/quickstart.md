# Quickstart: check the feature by hand

```sh
mkdir -p /tmp/wf/third-party/fx && cd /tmp/wf
cat > CMakeLists.txt <<'C'
cmake_minimum_required(VERSION 3.25)
project(waybill_fixture_superbuild)
include(FetchContent)
FetchContent_Declare(waybill-fixture-gtest URL https://example.invalid/g-1.0.tar.gz
  URL_HASH SHA256=1111111111111111111111111111111111111111111111111111111111111111)
add_subdirectory(third-party/fx)
C
cat > third-party/fx/CMakeLists.txt <<'C'
if(NOT TARGET waybill_fixture_zlib)
  FetchContent_Declare(waybill-fixture-zlib URL https://example.invalid/z-1.3.tar.gz)
endif()
FetchContent_Declare(waybill-fixture-gtest URL https://example.invalid/g-2.0.tar.gz)
C
waybill --offline sbom scan --path . --root-name wf --root-version 1 --output wf.cdx.json
```

**Expect:**
- `waybill-fixture-zlib`:
  - `scope: excluded`;
  - `waybill:optional-derivation` = `cmake-not-target`;
  - `waybill:cmake-fetch-conditions` = `["NOT TARGET waybill_fixture_zlib"]`.
- `waybill-fixture-gtest`:
  - one component, at 1.0;
  - `waybill:superseded-declarations` lists `third-party/fx/CMakeLists.txt` at 2.0.
- The document's `waybill:cmake-fetch-coverage`:
  - `read` = 3, `emitted` = 2, `merged.first-declaration-wins` = 1, `conditional` = 1.

```sh
jq -c '[.components[] | {purl, scope, p: [.properties[]? | select(.name|test("optional-derivation|cmake-fetch|superseded")) | "\(.name)=\(.value)"]}]' wf.cdx.json
jq -r '.metadata.properties[] | select(.name=="waybill:cmake-fetch-coverage") | .value' wf.cdx.json
```

TheRock is measured only in CI: run `census-fetches` in the fixture repository with `waybill_ref` set to the branch.
