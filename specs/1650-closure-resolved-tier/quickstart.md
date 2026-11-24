# Quickstart: verify closure-resolved tiers

```sh
# Fixture cases and unit tests
cargo test -p waybill --test closure_resolved_tier
cargo test -p waybill --bin waybill closure_resolved_tier

# I11's positive control against main's committed goldens (before regeneration)
cargo test -p waybill --test public_corpus -- --ignored invariant   # see tasks.md for the exact filter

# Tiers in a closure golden
jq -r '[..|objects|select(has("bom-ref") and has("name"))
        |((.properties//[])|map(select(.name=="waybill:sbom-tier"))|.[0].value // "none")]
       | group_by(.) | map("\(.[0])=\(length)") | join(" ")' \
  waybill-cli/tests/fixtures/public_corpus/nix-closure-slacklinker/cdx.json

# The gate
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

**Expected after the change** (research R7):
- `design` is 1 on moat (`readme`) and 0 on the other four closure targets;
- I11 passes on all five.
