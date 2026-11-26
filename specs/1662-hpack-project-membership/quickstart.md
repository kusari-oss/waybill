# Quickstart: spec 1662

```sh
# Positive control: declared package.yaml kept, undeclared dropped.
cargo nextest run -p waybill hpack_project_membership

# ogmios: json-via-show should be gone.
waybill sbom scan --offline --path <ogmios checkout> --output out.cdx.json
grep -c 'pkg:hackage/json-via-show' out.cdx.json   # expect 0
```
