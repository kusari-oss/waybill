# Quickstart: Read dependency manifests in CI and docs dot-directories

```sh
mkdir -p /tmp/dd/.ci/metrics /tmp/dd/.github/actions/act
printf 'waybill-fixture-ci-a==1.0\n' > /tmp/dd/.ci/requirements.txt
printf 'waybill-fixture-ci-b==1.0\n' > /tmp/dd/.ci/metrics/requirements.txt
printf 'waybill-fixture-all==1.0\n'  > /tmp/dd/.ci/all_requirements.txt
printf '{"name":"act","version":"1.0.0","dependencies":{"waybill-fixture-npm-a":"1.0.0"}}\n' > /tmp/dd/.github/actions/act/package.json
waybill --offline sbom scan --path /tmp/dd --format cyclonedx-json --output cyclonedx-json=/tmp/dd.cdx.json
jq '.components[] | {name, scope, props: [.properties[]? | select(.name|test("python-req-file-scope|lifecycle-scope-derivation"))]}' /tmp/dd.cdx.json
```

**Expected:**
- the three pip packages, each `scope: "excluded"` with C155 `ci`;
- `waybill-fixture-npm-a`, `scope: "excluded"` with C62 `ci-directory`.

**On `main`:** none of the four appear.
