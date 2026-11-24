# Quickstart: spec 1653

## Reproduce the measurement (SC-001(a))

```sh
cd specs/1653-cmake-build-tree/measurements
./probe/run.sh /tmp/probe-out            # Docker; about 5 minutes; 24 build trees
python3 -I -B probe/analyze.py /tmp/probe-out > /tmp/probe-results.md
diff <(sed -n '/## Signals/,/## Build-tree size/p' probe-results.md) \
     <(sed -n '/## Signals/,/## Build-tree size/p' /tmp/probe-results.md)
```

The signal table should be identical. The run timestamps in file-API reply
names differ, and so do the sizes, by a few bytes.

## Use it (after implementation)

```sh
cmake -S . -B build -DCMAKE_EXPORT_COMPILE_COMMANDS=ON && cmake --build build
waybill sbom scan --path . --cmake-build-tree build \
  --format cyclonedx-json --output cyclonedx-json=sbom.cdx.json

# Which fallbacks did this build take?
jq -r '.components[] | select(.properties[]?.name=="waybill:cmake-build-settlement")
       | [.name, (.properties[] | select(.name=="waybill:cmake-build-settlement").value | fromjson.state)] | @tsv' sbom.cdx.json
# What did the reader see?
jq -r '.metadata.properties[] | select(.name=="waybill:cmake-build-tree-coverage").value | fromjson' sbom.cdx.json
```

For a super-build, name the top of the build tree. Every subproject's build
directory beneath it is found. For a build split across CI stages, repeat
`--cmake-build-tree` once per stage's tree.

## Verify (implementation PRs)

```sh
cargo test -p waybill --test cmake_build_tree
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

SC-004 (byte-identical source-only output) needs a `public-corpus.yml` run as
well, because the corpus goldens are `#[ignore]`d locally.
