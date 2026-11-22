# Quickstart: verify spec 1642

## Reproduce the defects (any build)

```sh
cargo build --bin waybill
specs/1642-requirements-owner-editable/measurements/probe.sh "$PWD/target/debug/waybill" "$SCRATCH/probe"
```

On `main` the R1 lines show the edge into `waybill-fixture-shared` coming from the dev package in the `devtool` order, and C18 `["tool/uv.lock"]`. The `r4` lines show `excluded … transitive`. After this spec, the edge comes from `waybill-fixture-tool@1.0.0` in both orders, C18 starts with `tool/devtool`, and `r4` is `required`.

## Tests

```sh
cargo test -p waybill --test requirements_owner
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

## Census (CI only for TheRock)

```sh
gh workflow run census-owners.yml -R kusari-sandbox/waybill-fixture-therock-sdk --ref main \
  -f waybill_ref=<full SHA of the log-only commit>
```

Verify each job's `refs.txt` names that SHA, then read `owners-<target>.json`.
