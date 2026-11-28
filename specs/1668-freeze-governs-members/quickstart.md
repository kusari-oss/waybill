# Quickstart

```sh
t=$(mktemp -d); mkdir -p $t/app
printf 'packages: app\n' > $t/cabal.project
printf 'constraints: waybill-fixture-frozen ==4.0.0,\n             waybill-fixture-unused ==1.0.0\n' > $t/cabal.project.freeze
printf 'cabal-version: 2.4\nname: waybill-fixture-app\nversion: 0.1.0\nlibrary\n  build-depends: base, waybill-fixture-frozen\n' > $t/app/waybill-fixture-app.cabal
waybill --offline sbom scan --path $t --no-deep-hash --format cyclonedx-json --output o.cdx.json
jq '[.components[]|select(.name|startswith("waybill-fixture-frozen"))|.purl]' o.cdx.json
```

On `main` this gives two PURLs. On the branch it gives one, `@4.0.0`. `waybill-fixture-unused` keeps `edgeless-lock-pin`.
