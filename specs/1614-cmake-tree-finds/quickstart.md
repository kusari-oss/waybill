# Quickstart: Read find_package from every CMake file, with the conditions that reach it

## Try it

```sh
mkdir -p /tmp/w1614/projects/app && cd /tmp/w1614
cat > CMakeLists.txt <<'EOF'
project(waybill_fixture_root)
if(BUILD_TESTING)
  add_subdirectory(projects/app)
endif()
EOF
echo 'find_package(waybill_fixture_ext 2.1)' > projects/app/CMakeLists.txt
waybill --offline sbom scan --path . --format cyclonedx-json --output cyclonedx-json=out.json
jq '.components[] | select(.purl=="pkg:generic/waybill_fixture_ext") | {scope, properties}' out.json
```

Expected: the component exists, with `scope: excluded`. Its properties carry C62 `cmake-build-testing` and C236 `waybill:cmake-find-conditions` `["BUILD_TESTING"]`. On `main` it is absent.

## Tests

```sh
cargo test -p waybill --test scan_cmake_tree_finds
cargo test -p waybill --bins reach::
cargo test -p waybill --bins searches::
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

## Measure on TheRock (CI only)

The commit must be pushed, and SHAs must be full:

```sh
R=kusari-sandbox/waybill-fixture-therock-sdk
gh workflow run measure-source.yml -R $R --ref main -f waybill_ref=<full branch sha> -f base_ref=<full main sha>
gh workflow run census-includes.yml -R $R --ref main   # the reference counts
```

Then:
- `python3 -I -B specs/1614-cmake-tree-finds/measurements/predict.py <includes.json> <base.sub.cdx.json>` gives the expected count.
- Compare it with the branch document's `cmake-find-package` components.
- Run `waybill sbom parity-check` on each side's three documents.
