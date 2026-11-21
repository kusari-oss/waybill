# Quickstart: C56 on a merged component

## See it

A super-project pins `waybill-fixture-mixed` as a submodule at commit `C`, and its `CMakeLists.txt` fetches the same repository at `C`:

```cmake
include(FetchContent)
FetchContent_Declare(waybill-fixture-mixed
  GIT_REPOSITORY https://github.com/ROCm/waybill-fixture-mixed.git
  GIT_TAG <C>)
```

```sh
waybill sbom scan --path . --format cyclonedx-json --output cyclonedx-json=out.cdx.json
jq '.components[] | select(.purl | startswith("pkg:github/rocm/waybill-fixture-mixed"))
    | {purl, props: [.properties[] | select(.name | test("source-mechanism|also-detected-via"))]}' out.cdx.json
```

Expected: one component, `waybill:source-mechanism` as on `main`, and `waybill:also-detected-via` listing the other mechanism.

## Check the three formats agree

```sh
mkdir -p out
waybill sbom scan --path . --format cyclonedx-json,spdx-2.3-json,spdx-3-json \
  --output cyclonedx-json=out/waybill.cdx.json \
  --output spdx-2.3-json=out/waybill.spdx.json \
  --output spdx-3-json=out/waybill.spdx3.json
waybill sbom parity-check --scan-dir out
```

C55, C56 and C242 must agree.

## Run the census (maintainer, CI only for TheRock)

```sh
gh workflow run census-merges.yml -R kusari-sandbox/waybill-fixture-therock-sdk \
  --ref main -f waybill_ref=<full branch sha>
```

Each job uploads `merges.json` (events by site and mechanism pair) and `refs.txt` (the waybill SHA it scanned).

## Tests

```sh
cargo test -p waybill --test also_detected_via
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```
