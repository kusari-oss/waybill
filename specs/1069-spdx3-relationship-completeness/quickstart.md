# Quickstart: SPDX 3 dependency completeness

`B=target/debug/waybill`

## 1. CycloneDX and SPDX 3 agree (SC-001, SC-002)

Scan a degraded case. The corpus `go-cobra` target is cold-cache and has 8 components in `unknown`. Locally, use a fresh `GOMODCACHE`, or check against the regenerated corpus goldens. Then compare:

```sh
jq -r '[.compositions[] | select(.aggregate=="unknown") | .dependencies[]] | sort[]' out.cdx.json > cdx-unknown
jq -r '.["@graph"] as $g
       | ($g | map(select(.software_packageUrl)) | map({key: .spdxId, value: .software_packageUrl}) | from_entries) as $purl
       | [$g[] | select(.relationshipType=="dependsOn" and (.completeness=="incomplete" or .completeness=="noAssertion")) | $purl[.from]]
       | unique[]' out.spdx3.json > spdx3-not-complete
diff cdx-unknown spdx3-not-complete   # empty: 100% agreement
```

Repeat with `complete`: every CycloneDX-complete component that has a dependency relationship is `completeness: complete` in SPDX 3.

## 2. Shape

```sh
jq '[.["@graph"][] | select(.relationshipType=="dependsOn")] | group_by([.from, .type, .scope]) | map(length) | max' out.spdx3.json   # 1
```

## 3. Conformance and unchanged formats

- SPDX 3: the conformance gate passes, and `measurements/probe_validator.py` passes on a regenerated document.
- CycloneDX and SPDX 2.3 goldens: byte-identical.
