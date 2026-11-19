# Quickstart

## Try it

```sh
mkdir -p /tmp/w1615/proj/docs/sphinx && cd /tmp/w1615
echo 'waybill-fixture-theme==1.0' > proj/docs/sphinx/requirements.txt
echo 'waybill-fixture-runtime==2.0' > proj/requirements.txt
waybill --offline sbom scan --path . --format cyclonedx-json --output cyclonedx-json=out.json
jq -c '.components[] | select(.name|startswith("waybill-fixture")) | {name, scope, c155: ([.properties[]? | select(.name=="waybill:python-req-file-scope") | .value] | first)}' out.json
```

Expected: `waybill-fixture-theme` has `scope: excluded` and C155 `docs`, and `waybill-fixture-runtime` has no scope and no C155. On `main` both are required.

## Tests

```sh
cargo test -p waybill --test scan_nested_req_scope
cargo test -p waybill --bins requirements_txt::
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

## Measure on TheRock (CI only)

```sh
R=kusari-sandbox/waybill-fixture-therock-sdk
gh workflow run measure-source.yml -R $R --ref main -f waybill_ref=<full branch sha> -f base_ref=<full main sha>
```

Then diff the two documents' pip components by scope and C155, and run `waybill sbom parity-check` on each side.
