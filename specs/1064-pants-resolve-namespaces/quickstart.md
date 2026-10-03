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
