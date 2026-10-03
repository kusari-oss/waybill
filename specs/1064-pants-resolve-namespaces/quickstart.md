# Quickstart: verify Pants resolve ownership across namespaces

## 1. The collision fixture (User Story 2)

```sh
cargo build -p waybill
B=./target/debug/waybill
F=waybill-cli/tests/fixtures/pants_namespace_collision
$B --offline sbom scan --path $F --no-deep-hash \
  --format cyclonedx-json --output cyclonedx-json=/tmp/col.cdx.json

# Statement: qualified names, both namespaces
jq -r '.metadata.properties[] | select(.name=="waybill:resolve-ownership") | .value' /tmp/col.cdx.json
#   {"declared":["jvm:default","python:default","python:lint"],…}

# Two distinct anchors for the two `default`s
jq -r '.components[] | select(any(.properties[]?; .name=="waybill:component-kind" and .value=="lockfile-resolve")) | .purl' /tmp/col.cdx.json
#   pkg:generic/default?pants-namespace=jvm
#   pkg:generic/default?pants-namespace=python
#   pkg:generic/lint?pants-namespace=python
```

## 2. A JVM-only repository (User Stories 1 and 3)

```sh
git clone --depth 1 https://github.com/kusari-sandbox/example-jvm /tmp/ejvm
$B --offline sbom scan --path /tmp/ejvm --no-deep-hash \
  --format cyclonedx-json --output cyclonedx-json=/tmp/ejvm.cdx.json

jq -r '.metadata.properties[] | select(.name=="waybill:resolve-ownership") | .value' /tmp/ejvm.cdx.json
#   {"declared":["jvm:jvm-default"],…}      (before: absent)

# Membership names the resolve as Pants does
jq -r '[.components[].properties[]? | select(.name=="waybill:pants-resolve") | .value] | unique[]' /tmp/ejvm.cdx.json
#   ["jvm-default"]                         (before: ["default"])

# No JVM package is left without an owner: every pkg:maven/* is reachable
# from the root (check the graph-completeness property reports none unreachable)
jq -r '.metadata.properties[] | select(.name=="waybill:graph-completeness") | .value' /tmp/ejvm.cdx.json
```

## 3. Format agreement on the polyglot target (FR-013, SC-002)

```sh
cargo test -p waybill --test public_corpus known_spdx3   # after goldens regenerate
```

`pants-clojure-polyglot` is expected to leave `KNOWN_SPDX3_ROOT_EDGE_DIVERGENCE`:
equal root out-edge counts in CycloneDX, SPDX 2.3 and SPDX 3.

## 4. Non-Pants repositories are untouched (FR-014, SC-004)

The read-only public-corpus run on the branch must show `no semantic change` for
all 13 non-Pants targets (`xtask corpus-diff`).

## Recorded results (T050, 2026-10-03, branch build)

| Step | Expected | Observed |
|---|---|---|
| 1. statement | `{"declared":["jvm:default","python:default","python:lint"],…}` | identical, `weak_classification` 3 |
| 1. anchors | three, two named `default` | `default?pants-namespace=jvm`, `default?pants-namespace=python`, `lint?pants-namespace=python` |
| 1. root out-edges CDX / SPDX 2.3 / SPDX 3 | equal | 3 / 3 / 3 |
| 2. `example-jvm` statement | `{"declared":["jvm:jvm-default"],…}` | identical, `weak_classification` 1 |
| 2. membership | `["jvm-default"]` | `["jvm-default"]` |
| 2. graph completeness | none unreachable | `complete`; anchor has 4 top-level edges |
| 3. `pants_backend_clojure` @ `e068ffb`, local scan | equal root out-edges | 4 / 4 / 4 (each format: the four anchors); `complete` |
| 4. non-Pants targets | `no semantic change` | pending the CI corpus run (T043) |

Found while running step 1: dedup grouped components on
`(ecosystem, name, version, parent_purl)`, so the two `default` anchors merged
into one even though their PURLs differ. Fixed under T035.
