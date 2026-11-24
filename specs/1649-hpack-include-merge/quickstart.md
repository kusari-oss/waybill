# Quickstart: hpack `!include` and YAML merge keys

## Reproduce #1674 on `main`

```sh
mkdir -p /tmp/p5 && printf 'name: waybill-fixture-p5\nversion: 0.1.0\n_a: &a\n  dependencies: [base, containers]\nlibrary:\n  <<: *a\n  source-dirs: src\n' > /tmp/p5/package.yaml
./target/debug/waybill --offline sbom scan --path /tmp/p5 --format cyclonedx-json --output cyclonedx-json=/tmp/p5.cdx.json
jq -r '[.. | objects | select((.purl // "") | startswith("pkg:hackage/")) | .purl] | unique[]' /tmp/p5.cdx.json
```

- On `main`: only `pkg:hackage/waybill-fixture-p5@0.1.0`.
- On the branch: `base` and `containers` as well.

## hpack's answer for the same tree

```sh
(cd /tmp/p5 && mkdir -p src && nix run nixpkgs#hpack -- --force && grep -A3 build-depends *.cabal)
```

## The tests

```sh
cargo test -p waybill --test hpack_include_merge
cargo test -p waybill --bins s1649
```

## The agreement check on the corpus (R7)

```sh
python3 -I -B specs/1649-hpack-include-merge/measurements/hpack_agreement.py <repo-copy> <waybill.cdx.json>
```

## The gate

```sh
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```
