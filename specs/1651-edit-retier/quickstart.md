# Quickstart: `sbom edit --retier`

```sh
# Relabel declared-but-unresolved components as source, in each format of one scan
for f in out.cdx.json out.spdx.json out.spdx3.json; do
  waybill sbom edit --retier design=source --output "retiered.$f" "$f"
done
waybill sbom parity-check --scan-dir .        # the three edited documents agree
waybill sbom verify-chain retiered.out.cdx.json --original out.cdx.json

# What changed, per mapping
jq -r '.metadata.properties[] | select(.name=="waybill:derivation") | .value | fromjson | .operations' retiered.out.cdx.json

# Which components, and what they were
jq -r '.components[] | select(any(.properties[]?; .name=="waybill:retiered-from")) | .purl' retiered.out.cdx.json
```

The tests: `cargo test -p waybill --test sbom_edit_retier`, `--test sbom_edit_parity`, and the unit tests under `edit::`.
