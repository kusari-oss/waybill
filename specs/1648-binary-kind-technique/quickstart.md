# Quickstart: dynamic linkage and embedded source paths are labelled binary analysis

## See the technique on a real binary

```sh
mkdir -p /tmp/k1 && cp /bin/ls /tmp/k1/   # Linux; on macOS use a Linux ELF
./target/debug/waybill --offline sbom scan --path /tmp/k1 --format cyclonedx-json --output cyclonedx-json=/tmp/k1.cdx.json
jq '[.. | objects | select((.properties // []) | any(.value == "dynamic-linkage")) | .evidence.identity[0].methods[0].technique] | unique' /tmp/k1.cdx.json
```

- On `main`: `["manifest-analysis"]`.
- On the wired branch: `["binary-analysis"]`.

## The census

```sh
RUST_LOG=waybill::version_census=debug ./target/debug/waybill --offline sbom scan --path <dir> ... 2> <dir>.log
grep -E 'technique_change=|kind_merge=|binary_derived_kind=' <dir>.log
```

## The tests

```sh
cargo test -p waybill --test binary_kind_technique
cargo test -p waybill --bins s1648
```

## The gate

```sh
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

## The census on TheRock (CI only)

Dispatch `census-versions.yml` in kusari-sandbox/waybill-fixture-therock-sdk with `--ref main -f waybill_ref=<full SHA>`, then verify every job's `refs.txt`.
