# Quickstart: verify deps.dev enrichment outcomes

Build: `cargo build -p waybill`; `B=target/debug/waybill`; `M=specs/1067-depsdev-enrichment-outcome/measurements`.

## 1. A real repository with placeholder versions (US2, SC-002, SC-005)

```sh
R=$(ls -d ~/.cache/waybill/corpus/792f8c5a23a5766e/*/repo)    # opentelemetry-go
$B sbom scan --path $R --no-deep-hash --format cyclonedx-json --output cyclonedx-json=/tmp/otel.json
jq -r '.metadata.properties[] | select(.name=="waybill:deps-dev-outcomes") | .value' /tmp/otel.json
# {"absent":…,"not-queried:incomplete-coordinate":28,…}
jq -r '[.components[] | select(any(.properties[]?; .name=="waybill:deps-dev-outcome" and .value=="not-queried:incomplete-coordinate"))] | length' /tmp/otel.json
# 28
```

The per-pass log line's `network_lookups` drops by 28 from the baseline in `measurements/counts.txt`.

## 2. Fully matched: nothing added (US3)

```sh
R=$(ls -d ~/.cache/waybill/corpus/f12f3d8cb55abc01/*/repo)    # express
$B sbom scan --path $R --no-deep-hash --format cyclonedx-json --output cyclonedx-json=/tmp/ex.json
jq '[.components[].properties[]? | select(.name=="waybill:deps-dev-outcome")] | length' /tmp/ex.json   # 0
```

## 3. Offline: byte-identical (FR-007)

The corpus run shows `no semantic change` for every target.

## 4. How deps.dev reports each outcome

`python3 $M/probe_batch_outcomes.py` re-measures the batch and per-key shapes.
