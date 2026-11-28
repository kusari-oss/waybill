# Quickstart

```sh
# C200 carries no host path
waybill --offline sbom scan --path <hpack-project> --format cyclonedx-json --output o.cdx.json
jq -r '.metadata.properties[]|select(.name=="waybill:hpack-read-outcome").value' o.cdx.json | grep -c '^/' # 0

# Haskell source_dir is a directory
waybill --offline sbom scan --path <tree> --split=workspace --output-dir out
jq -r '.entries[] | "\(.subproject_id) \(.source_dir)"' out/split-manifest.json
```
