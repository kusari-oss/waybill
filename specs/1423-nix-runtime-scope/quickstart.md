# Quickstart: verifying 1423

## Hermetic (normal lanes)

```sh
cargo test -p waybill --test closure_runtime_scope
cargo test -p waybill --bins closure_role_scope
cargo test -p waybill --bins edit::
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

The `closure_runtime_scope` tests drive a stub `nix` through four cases:
- the local store answers;
- a substituter answers and verifies;
- a substituter answers but verification fails, so the walk takes over;
- no store answers, so the walk takes over.

They also include an ormolu-shaped fixture with test-only Haskell dependencies, run once with declared edges and once without.

## Live, on a host with nix (the US1/US4 check, SC-001/SC-002)

```sh
for t in curl openssh httpie gh ripgrep; do
  waybill sbom scan --path "$FLAKES/$t" --nix-closure --format cyclonedx-json -o "$OUT/$t.cdx.json"
  jq -r '.metadata.properties[] | select(.name=="waybill:closure-scope-method") | .value' "$OUT/$t.cdx.json"
  waybill sbom edit "$OUT/$t.cdx.json" --drop 'scope=excluded' -o "$OUT/$t.rt.cdx.json"
  GRYPE_DB_AUTO_UPDATE=false grype "sbom:$OUT/$t.rt.cdx.json" -o json > "$OUT/$t.rt.grype.json"
done
```

Score the result against the committed runtime truth with `measurements/runtime.py`. Expect 350 findings in total, and all 243 of sbomnix's runtime findings among them.

## Format agreement (SC-004)

```sh
for f in cdx.json spdx.json spdx3.json; do
  waybill sbom edit "doc.$f" --keep 'scope=excluded' -o "sel.$f"
done
# compare the PURL sets of sel.*; they must be equal
```

## Corpus (nightly, FR-016)

The `nix-closure-shellcheck` target runs with the other corpus targets in `public-corpus.yml`. Its expectations assert:
- C216 `method = runtime-closure` and `verified = true`;
- 50 `statically-linked` Hackage components;
- none of them marked `excluded`.
