# Quickstart: spec 1663

```sh
cargo nextest run -p waybill --test stack_snapshot_identity
# hs-opentelemetry: no `unknown`, the yesod snapshot is lts-18.8, and the root lock parses.
waybill --offline sbom scan --path <hs-opentelemetry checkout> --format cyclonedx-json --output cyclonedx-json=out.json
jq -r '[.. | objects | .purl? // empty | select(startswith("pkg:generic/"))] | unique[]' out.json
```
