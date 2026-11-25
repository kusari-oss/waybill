# Quickstart: checking spec 1654 by hand

```sh
waybill sbom scan --path <tree> --format cyclonedx-json --format spdx-2.3-json --format spdx-3-json \
  --output cyclonedx-json=out/waybill.cdx.json --output spdx-2.3-json=out/waybill.spdx.json \
  --output spdx-3-json=out/waybill.spdx3.json
waybill sbom parity-check --format json --scan-dir out \
  | jq '[.rows[] | select(.verdict == "fail") | .row_id], .summary.parity_gaps'
# [] and 0 when every format agrees
```

Every corpus golden, before and after:

```sh
specs/1654-parity-verdict/measurements/golden_parity.sh target/debug/waybill waybill-cli/tests/fixtures/public_corpus
```

In a test:

```rust
mod common;
common::parity::assert_parity(out_dir);
```
