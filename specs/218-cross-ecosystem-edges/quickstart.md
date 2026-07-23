# Quickstart: Cross-ecosystem dep-name edge resolution

**Feature**: 218-cross-ecosystem-edges | **Date**: 2026-07-22

## For operators

### Scan a Ruby app with the experimental flag

```sh
waybill scan \
  --path ~/Projects/my-ruby-app \
  --format cyclonedx-json \
  --experimental-cross-ecosystem-edges \
  --output my-ruby-app.cdx.json
```

Or via env var:

```sh
WAYBILL_EXPERIMENTAL_CROSS_ECOSYSTEM_EDGES=1 \
  waybill scan --path ~/Projects/my-ruby-app --format cyclonedx-json \
  --output my-ruby-app.cdx.json
```

### Verify the recovered edges

```sh
jq '.dependencies[] | select(.ref | startswith("pkg:generic/")) | .dependsOn' \
  my-ruby-app.cdx.json
```

Expected output: an array of `pkg:gem/*` PURLs — one entry per Gemfile.lock DEPENDENCIES gem that resolved. Before this milestone (or with the flag off), the array is empty.

### Verify the per-edge annotation

```sh
jq '.dependencies[] | select(.ref | startswith("pkg:generic/")) | .properties' \
  my-ruby-app.cdx.json
```

Expected output: one `{"name":"waybill:cross-ecosystem-inference","value":"..."}` object per crossed edge; parse the `.value` field as JSON to recover the `{from_eco, lookup_via, target_purl, to_eco}` payload.

### Verify no unresolved names

```sh
jq '.metadata.properties[]? | select(.name == "waybill:cross-ecosystem-inference-unresolved")' \
  my-ruby-app.cdx.json
```

Absence = every DEPENDENCIES gem resolved cleanly. Presence = one or more names couldn't be bridged; the `.value` field is a JSON array of `{source_purl, unresolved_name}` records for the operator to investigate (typically: the offending gem was not in the resolver index because a lockfile parse failed, or the gem is a Gemfile-only reference to a git-sourced or path-sourced package with no registered PURL).

## For contributors

### Iterate on the resolver locally

```sh
# Run the FR-003 tie-break unit tests standalone (fast — ~seconds).
cargo +stable test -p waybill --lib generate::cross_ecosystem_edges::tie_break

# Run the flag-on integration test (fixture scan + edge count assertion).
cargo +stable test -p waybill --test cross_ecosystem_edges

# Run the SC-009 byte-identity gate (flag-off unchanged from post-m216).
cargo +stable test -p waybill --test cross_ecosystem_edges -- \
  flag_off_preserves_current_post_m216_byte_identity

# Extend the transitive_parity_gem test with the new m218 flag-on case.
cargo +stable test -p waybill --test transitive_parity_gem
```

### Pre-PR gate

Per Constitution mandatory verification:

```sh
./scripts/pre-pr.sh
```

Both clippy `-D warnings` and full-workspace test MUST pass. Read `feedback_prepr_gate_bails_on_first_failure.md` memory before treating any failure as a flake.

### Regenerate the SC-009 flag-off golden

Only if the m216 baseline itself moves (unlikely — that requires a separate spec):

```sh
MIKEBOM_UPDATE_CROSS_ECOSYSTEM_GOLDEN=1 \
  cargo +stable test -p waybill --test cross_ecosystem_edges -- \
  flag_off_preserves_current_post_m216_byte_identity
```

Review the resulting golden diff CAREFULLY at PR time — a change here means we're accepting a shift in the flag-off default output shape.

## For SBOM consumers

Read `docs/reference/cross-ecosystem-edges.md` for:
- The full annotation payload contract for `waybill:cross-ecosystem-inference` / `-ambiguous` / `-unresolved`.
- Decision tree for trust-scoring cross-ecosystem edges.
- Worked examples in Python and TypeScript.
- Experimental-status disclaimer + graduation contract.

## Verification checklist

- [ ] `waybill scan --help` shows the `--experimental-cross-ecosystem-edges` flag with the docs-page reference.
- [ ] Flag-on scan of the fastlane fixture emits ≥ 24 outgoing DEPENDS_ON edges from the `pkg:generic/` main-module.
- [ ] Every such edge carries a `waybill:cross-ecosystem-inference` property (CDX) / annotation (SPDX 2.3 / SPDX 3).
- [ ] Flag-off scan of the fastlane fixture is byte-identical to a post-m216 golden.
- [ ] Non-Ruby fixtures (11 `*_regression` baselines) are byte-identical to their pre-m218 goldens regardless of flag state (FR-008).
- [ ] Parity gate `every_catalog_row_has_an_extractor` passes with 3 new C-rows registered.
- [ ] `docs/reference/cross-ecosystem-edges.md` exists, is linked from `README.md`, covers all 5 FR-014 topics.
