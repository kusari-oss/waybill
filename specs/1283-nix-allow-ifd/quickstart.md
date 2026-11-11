# Quickstart: verifying 1283

## Hermetic (normal lanes)

```sh
cargo test -p waybill --test nix_allow_ifd
cargo test -p waybill --bins nix::eval::preflight
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

`nix_allow_ifd.rs` covers, with a stub `nix`:
- default refusal unchanged;
- allowed and verified;
- allowance unverified, which degrades;
- IFD build failed, which degrades with a distinct reason;
- sandbox off, which WARNs but proceeds;
- the refused flags still refused;
- the flag without a tier, which is rejected;
- C217 in all three formats.

## Local live check (macOS, flakes already built here)

```sh
for t in slack-web openai; do
  waybill sbom scan --path "$T/$t" --nix-closure --nix-allow-ifd \
    --format cyclonedx-json --output cyclonedx-json="$OUT/$t.cdx.json"
  jq -r '.metadata.properties[]|select(.name|test("nix-evaluation-policy|nix-closure-degraded|closure-scope-method"))|"\(.name)=\(.value)"' "$OUT/$t.cdx.json"
done
```

Expect C216 `runtime-closure`, C217 `allowed` / `verified: true` / `sandbox: "false"`, and no C190. Then run the recall-corpus probe (stage 2 scripts) to measure grype recall against sbomnix's runtime findings. slack-web and openai must be ≥ 90% (SC-001, SC-002).

## Linux (nightly corpus, FR-014)

1. Dispatch `recall-truth.yml` for the three targets, review the artifact, and commit each `recall-truth.json`.
2. Dispatch `public-corpus.yml` with `regen_goldens=true`, then commit the goldens.
3. Confirm the nightly run: each IFD target's expectations assert no IFD degradation, C217 `allowed` with `verified: true`, and proxy recall ≥ 0.90. That covers SC-001–SC-003 on Linux, including slacklinker.
