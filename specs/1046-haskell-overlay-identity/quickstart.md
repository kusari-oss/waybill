# Quickstart: verifying 1046

## Unit and integration tests (no network)

```sh
cargo test -p waybill --bin waybill twin_merge
cargo test -p waybill --test closure_haskell_overlay   # stub nix and a stub Hackage source
```

The integration suite covers these cases, each with a stub `nix` closure and an injected Hackage source:
- **The merge itself:** the in-range merge (slack-web shape) and the out-of-range merge (hspec-discover shape, C221).
- **When there is no merge:** a non-Haskell namesake, and several variants with none or more than one in range.
- **Each Hackage outcome** (200, 404, unreachable, `--offline`, `--no-hackage-lookup`).
- **Where the source is recorded:** a `fetchgit` source with `url`/`rev` under `structuredAttrs`, and a `fetchurl` source in `env`.

## Corpus

Regenerate with `public-corpus.yml`, then check:

```sh
G=waybill-cli/tests/fixtures/public_corpus
jq '[..|objects|select(.name=="slack-web")|{purl,version}]' $G/nix-closure-slacklinker/cdx.json      # one component (SC-001)
jq '[..|objects|select(.name=="hspec-discover")|.properties[]?|select(.name=="waybill:requirement-unsatisfied")]' $G/nix-closure-slack-web/cdx.json   # C221 (SC-002)
jq '[..|objects|select(has("bom-ref") and any(.properties[]?; .name=="waybill:closure-member-role" and .value=="source"))|select((.externalReferences//[])|length>0)]|length' $G/nix-closure-slacklinker/cdx.json   # > 111 (SC-003)
```

## Live check

Run `measurements/hackage-probe.sh` again before trusting R3. Hackage's behaviour is external, and it can change.
