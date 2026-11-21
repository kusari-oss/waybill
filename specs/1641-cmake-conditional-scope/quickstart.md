# Quickstart: CMake conditional scope

## See the defect on `main`

```sh
mkdir -p /tmp/wbcond && cd /tmp/wbcond
cat > CMakeLists.txt <<'EOF'
cmake_minimum_required(VERSION 3.25)
project(waybill_fixture_app VERSION 1.0.0)
include(ExternalProject)
if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
ExternalProject_Add(waybill-fixture-zlib
  URL "https://example.invalid/waybill-fixture-zlib-1.3.2.tar.gz"
  URL_HASH "SHA256=0000000000000000000000000000000000000000000000000000000000000000")
endif()
EOF
waybill --offline sbom scan --path . --format cyclonedx-json --output out.cdx.json
jq -r '.components[] | select(.type!="file") | "\(.purl) \(.scope // "required")"' out.cdx.json
# main: excluded (#1639, measured at 786091a8); this branch: required
```

## Run this feature's tests

```sh
cargo test -p waybill --test cmake_conditional_scope
cargo test -p waybill --bin waybill cmake::conditions
```

## Gate

```sh
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```
