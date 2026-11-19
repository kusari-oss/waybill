# Quickstart

## Tests

```sh
cargo test -p waybill --test propagation_seeds
cargo test -p waybill --bins scope_propagation::
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

## Measure

TheRock, in CI only, with full SHAs:

```sh
R=kusari-sandbox/waybill-fixture-therock-sdk
gh workflow run measure-source.yml -R $R --ref main -f waybill_ref=<branch sha> -f base_ref=<main sha>
```

Public corpus golden regeneration:

```sh
gh workflow run public-corpus.yml --ref 1616-propagation-seeds -f branch=1616-propagation-seeds -f regen_goldens=true
```

In each document diff, every changed component must go from C62 `transitive` (non-runtime) to required, and nothing else may change.
