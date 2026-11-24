# Quickstart: checking spec 1652 by hand

```sh
cargo build -p waybill
D=specs/1652-root-only-haskell/measurements
$D/make-fixtures.sh /tmp/fx
mkdir -p /tmp/h /tmp/out
WAYBILL=target/debug/waybill SCAN_HOME=/tmp/h $D/scan.sh /tmp/fx/f1-json-root root-only /tmp/out/f1.cdx.json
```

## The drop record (C244) and completeness

```sh
jq -r '.metadata.properties[] | select(.name|test("project-discovery|graph-completeness")) | "\(.name) = \(.value)"' /tmp/out/f1.cdx.json
```

Expected:

```
waybill:graph-completeness = partial
waybill:graph-completeness-reason = project-discovery-scope: 2 project(s) dropped from hackage
waybill:project-discovery-dropped = {"count":2,"main-modules":["pkg:hackage/waybill-fixture-a@0.1.0","pkg:hackage/waybill-fixture-b@0.1.0"],"reason":"project-discovery-scope"}
waybill:project-discovery-mode = root-only
```

## C200

```sh
jq -r '.metadata.properties[] | select(.name=="waybill:hpack-read-outcome") | .value | fromjson | ."manifests-dropped-by-scope"' /tmp/out/f1.cdx.json
```

Expected: both nested `package.yaml` paths.

## A root declaration sets the scope

```sh
WAYBILL=target/debug/waybill SCAN_HOME=/tmp/h $D/scan.sh /tmp/fx/f3-cabal-project root-only /tmp/out/f3.cdx.json
grep -c 'found zero root-level manifests' /tmp/out/f3.cdx.json.log   # 0
```

`a` and `b` are kept, and `tools/e` is in C244.

## Nothing changes by default

Scan any fixture with `all` on this branch and on `main`. Mask the serial number and timestamp, then diff: the documents must be identical.
