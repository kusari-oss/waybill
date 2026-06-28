# Quickstart — milestone 147 npm peerDependencies edge emission

Operator-facing walkthrough.

## Scenario 1 — Reproduce the closed orphan gap on the audit lockfile

The motivating use case: the looker-frontend lockfile that surfaced 5 mikebom orphans vs Trivy's 0.

```bash
# Pre-147: 5 orphans
mikebom sbom scan --path /tmp/peer-edge-check/ \
    --format cyclonedx-json --output /tmp/mb-pre.cdx.json

jq '
  [.components[] | .["bom-ref"] // .purl] as $all
  | [.dependencies[]? | .dependsOn[]?] as $depended
  | ($all - $depended) - [.metadata.component."bom-ref"]
  | length
' /tmp/mb-pre.cdx.json
# Pre-147: 5
# Post-147: 0  (matches Trivy)
```

### Verify the specific Trivy-comparable peer chain

```bash
# react-native should have ≥1 inbound edge post-147 (was 0 pre-147)
mikebom sbom scan --path /tmp/peer-edge-check/ \
    --format cyclonedx-json --output /tmp/mb.cdx.json

jq '
  [.dependencies[] | select(.dependsOn[]? == "pkg:npm/react-native@0.85.3") | .ref]
' /tmp/mb.cdx.json
# Pre-147: []
# Post-147: ["pkg:npm/%40react-native-async-storage/async-storage@1.24.0",
#            "pkg:npm/%40react-native/virtualized-lists@0.85.3",
#            ...]   (4 peer-driven inbound edges, matching Trivy)
```

## Scenario 2 — Inspect the peer-kind metadata

After this milestone, consumers wanting the install-vs-functional distinction can filter:

```bash
# CDX: peer-edge-targets is a property on the source component (xs:string-encoded JSON array)
jq '
  .components[]
  | select(.purl | contains("async-storage"))
  | .properties[]
  | select(.name == "mikebom:peer-edge-targets")
' /tmp/mb.cdx.json
# {
#   "name": "mikebom:peer-edge-targets",
#   "value": "[\"pkg:npm/react-native@0.85.3\"]"
# }

# SPDX 2.3: same value carried as native JSON array inside the envelope
mikebom sbom scan --path /tmp/peer-edge-check/ \
    --format spdx-2.3-json --output /tmp/mb.spdx.json

jq -r '
  .packages[]
  | select(.name | contains("async-storage"))
  | .annotations[]?
  | .comment
  | fromjson
  | select(.field == "mikebom:peer-edge-targets")
  | .value
' /tmp/mb.spdx.json
# ["pkg:npm/react-native@0.85.3"]   (native array, not stringified — milestone-145 envelope-shape)
```

### Filter the dep-graph to exclude peer-driven edges

A consumer can reconstruct the pre-147 install-only graph by subtracting peer-edge-targets from each component's dependsOn:

```bash
jq '
  .components | map(
    select(.properties != null and (.properties | map(select(.name == "mikebom:peer-edge-targets")) | length > 0)) as $with_peers
    | if . == ($with_peers | .[]?) then
        .dependsOn = (.dependsOn - (.properties[] | select(.name == "mikebom:peer-edge-targets") | .value | fromjson))
      else . end
  )
' /tmp/mb.cdx.json
# Yields the install-only edge subset (= pre-147 mikebom behavior == current syft behavior)
```

## Scenario 3 — Verify the unmet-peer guard (FR-002)

```bash
# Construct a minimal lockfile where a peer is declared but NOT in packages{}:
cat > /tmp/unmet/package-lock.json <<'JSON'
{
  "name": "unmet-test",
  "lockfileVersion": 3,
  "packages": {
    "": { "name": "unmet-test", "dependencies": { "mlly": "^1.0.0" } },
    "node_modules/mlly": {
      "version": "1.0.0",
      "peerDependencies": { "pathe": "^2.0.0" }
    }
  }
}
JSON
# pathe is declared in mlly's peerDependencies but NOT present as a node_modules/pathe entry → unmet peer

mikebom sbom scan --path /tmp/unmet --format cyclonedx-json --output /tmp/unmet.cdx.json

# Expected: mlly has NO edge to pathe, NO mikebom:peer-edge-targets annotation.
jq '.components[] | select(.purl | contains("mlly"))' /tmp/unmet.cdx.json
# Should contain no dependsOn entries for pathe, no properties entry for mikebom:peer-edge-targets.
```

## Verification commands (in-tree, CI-binding)

```bash
# All updated + new unit tests in package_lock.rs:
cargo test -p mikebom --bin mikebom package_lock::tests::

# Parity catalog row C97 (cross-format invariance):
cargo test -p mikebom parity::extractors::tests::c97_

# Pre-PR gate:
./scripts/pre-pr.sh
```

## Golden refresh (post-fix, before commit)

```bash
# Three potentially-affected fixtures (npm-bearing):
MIKEBOM_UPDATE_CDX_GOLDENS=1   cargo test --test cdx_regression
MIKEBOM_UPDATE_SPDX_GOLDENS=1  cargo test --test spdx_regression
MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo test --test spdx3_regression

# Inspect:
git diff --stat -- mikebom-cli/tests/fixtures/golden/cyclonedx/npm.cdx.json \
                   mikebom-cli/tests/fixtures/golden/spdx-2.3/npm.spdx.json \
                   mikebom-cli/tests/fixtures/golden/spdx-3/npm.spdx3.json

# Acceptance: each diff line MUST be either (a) new dependsOn / DEPENDS_ON / relationship
# entry for a peer-driven edge, OR (b) new mikebom:peer-edge-targets property/annotation.
# Reject any unrelated drift.
#
# NOTE: if diffs are empty (existing npm fixture has no peer-deps), tasks.md includes a
# sub-task to extend the fixture's package-lock.json with at least one peer-dep case so
# the byte-identity goldens exercise the new code path.
```

## Cross-tool comparison (operator-cadence per SC-009)

```bash
# Three-tool comparison on the audit lockfile:
trivy fs --format cyclonedx --output /tmp/trivy.cdx.json /tmp/peer-edge-check/
syft scan dir:/tmp/peer-edge-check/ -o cyclonedx-json > /tmp/syft.cdx.json
mikebom sbom scan --path /tmp/peer-edge-check/ --format cyclonedx-json --output /tmp/mb.cdx.json

for tool in syft trivy mikebom; do
    f="/tmp/${tool}.cdx.json"
    comps=$(jq '[.components[]] | length' "$f")
    root=$(jq -r '.metadata.component."bom-ref" // .metadata.component.purl // empty' "$f")
    orphans=$(jq --arg r "$root" '
        [.components[] | .["bom-ref"] // .purl] as $all
        | [.dependencies[]? | .dependsOn[]?] as $depended
        | ($all - $depended) - [$r]
        | length' "$f")
    printf "%-8s components=%-5d orphans=%d\n" "$tool" "$comps" "$orphans"
done
# Pre-147: trivy=0, syft=151, mikebom=5
# Post-147: trivy=0, syft=151, mikebom=0   (mikebom now matches Trivy on orphans)
```

## Known deferrals (spec Out of Scope)

- npm v1 / v2 lockfile peer-edge support (structural reader change required).
- Yarn / pnpm / bun lockfile peer-edge support (separate reader modules; their peer-handling models differ).
- Edge-level annotation in CDX (per `dependsOn[]` element) — CDX 1.6 spec has no slot.
- SPDX 3 native `LifecycleScopedRelationship.scope = "peer"` — not in current SPDX 3 spec.
- `--no-peer-edges` CLI flag to restore pre-147 behavior — speculative; deferred until a real consumer asks for it.
