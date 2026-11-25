# Quickstart: checking spec 1655

## The stub fixture

```sh
cargo test -p waybill --test boot_library_edges
```

The tests check four things:
- every boot library's C247 is `{"source":…}`;
- `ghc-internal`, `rts` and `ghc-prim@0.11.0` are components;
- the bootstrap `base@4.19.2.0` reaches `ghc-prim@0.11.0` and never `@0.12.0`;
- C105 has no `boot-library-edges-incomplete`.

Every scan in that file goes through `common::parity::assert_parity`.

## The goldens, before and after

```sh
P=waybill-cli/src/scan_fs/package_db/nix/haskell_packages
python3 -I -B specs/1655-reachable-boot-libraries/measurements/shared_purls.py \
  $P/ghc_boot_libraries.tsv $P/ghc_boot_dependencies.tsv waybill-cli/tests/fixtures/public_corpus
```

- **Before** (`shared-purls-before.txt`): the per-compiler counts of PURLs the document lacks.
- **After the dispatch regenerates the goldens:** every `no-component` count is 0 and every `held-under-other-label` count is 0, because the lookup now finds those. Any other result is a difference to trace in `measurements/after.md`.

Parity on every golden:

```sh
specs/1654-parity-verdict/measurements/golden_parity.sh target/debug/waybill waybill-cli/tests/fixtures/public_corpus
```

The failing rows must be a subset of spec 1654's after-table.

## Regenerating the goldens

Only by dispatch:

```sh
gh workflow run public-corpus.yml -R kusari-oss/waybill --ref main \
  -f branch=1655-reachable-boot-libraries -f regen_goldens=true
```
