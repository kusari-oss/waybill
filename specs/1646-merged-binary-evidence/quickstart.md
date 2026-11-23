# Quickstart: spec 1646

## See the merged component's methods

```sh
waybill --offline sbom scan --path <tree> --format cyclonedx-json,spdx-2.3-json \
  --output cyclonedx-json=out.cdx.json --output spdx-2.3-json=out.spdx.json
jq -c '[.components[]? | recurse(.components[]?)] | .[] | select(.name=="ncurses") | .evidence.identity[0].methods' out.cdx.json
jq -r '.packages[] | select(.name=="ncurses") | .annotations[] | .comment | select(test("evidence.identity"))' out.spdx.json
```

- `main`: `[{"technique":"manifest-analysis","confidence":0.85}]`, and the SPDX annotation shows `package-database` only.
- Spec 1646: the methods are `manifest-analysis` and `binary-analysis`, and the SPDX annotation adds `"methods"`.

## Census

```sh
RUST_LOG=waybill::version_census=debug waybill --offline sbom scan --path <tree> … 2>&1 \
  | grep -E ' (technique_change|binary_derived_kind|binary_merge)='
```

## Tests

```sh
cargo test -p waybill --test merged_binary_evidence
cargo test -p waybill --bins resolve::deduplicator::
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```
