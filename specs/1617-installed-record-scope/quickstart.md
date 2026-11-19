# Quickstart

## Tests

```sh
cargo test -p waybill --test installed_record_scope
cargo test -p waybill --bins cmake_search_pair::
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

## Measure (CI only)

The SDK, with full SHAs:

```sh
R=kusari-sandbox/waybill-fixture-therock-sdk
gh workflow run measure-full.yml -R $R --ref main -f waybill_ref=<branch sha> -f base_ref=<main sha>
```

Check the 9 records of research R6, and diff every other component's C42, C62 and C122 against `main`.

TheRock's source tree, which should be unchanged:

```sh
gh workflow run measure-source.yml -R $R --ref main -f waybill_ref=<branch sha> -f base_ref=<main sha>
```

Public corpus regeneration:

```sh
gh workflow run public-corpus.yml --ref 1617-installed-record-scope -f branch=1617-installed-record-scope -f regen_goldens=true
```
