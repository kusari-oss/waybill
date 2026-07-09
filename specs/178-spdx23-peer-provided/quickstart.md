# Quickstart: m178 Manual Verification

**Feature**: 178-spdx23-peer-provided
**Date**: 2026-07-09

Four verification paths — one per US + one for the byte-identity gates.

## Path A — US1: full-mode SPDX 2.3 emits `PROVIDED_DEPENDENCY_OF`

**Setup**: npm fixture with `peerDependencies` in the lockfile.

```bash
mkdir -p /tmp/m178-us1
cat > /tmp/m178-us1/package.json <<'EOF'
{
  "name": "m178-peer-demo",
  "version": "0.1.0",
  "dependencies": { "consumer-pkg": "^1.0.0" }
}
EOF

# A minimal package-lock with a peer edge (consumer-pkg peer-depends
# on provided-pkg; both listed in packages[""].dependencies to keep
# the walker happy).
cat > /tmp/m178-us1/package-lock.json <<'EOF'
{
  "name": "m178-peer-demo",
  "version": "0.1.0",
  "lockfileVersion": 3,
  "requires": true,
  "packages": {
    "": {
      "name": "m178-peer-demo",
      "version": "0.1.0",
      "dependencies": { "consumer-pkg": "^1.0.0", "provided-pkg": "^2.0.0" }
    },
    "node_modules/consumer-pkg": {
      "version": "1.2.3",
      "peerDependencies": { "provided-pkg": "^2.0.0" }
    },
    "node_modules/provided-pkg": {
      "version": "2.0.0"
    }
  }
}
EOF

mikebom --offline sbom scan --path /tmp/m178-us1 \
    --format spdx-json \
    --output /tmp/m178-us1.spdx.json \
    --no-deep-hash
```

**Path A.1 — peer edge fires `PROVIDED_DEPENDENCY_OF`**:

```bash
jq '.relationships[] | select(.relationshipType == "PROVIDED_DEPENDENCY_OF")' /tmp/m178-us1.spdx.json
```

**Expected**: exactly one match. `spdxElementId` maps to the `provided-pkg` Package (target); `relatedSpdxElement` maps to the `consumer-pkg` Package (source). Reads: "`provided-pkg` is a provided dependency of `consumer-pkg`".

**Path A.2 — reversed direction gate**:

```bash
# Verify the m228 reversed-direction convention holds: consumer-pkg
# is the source (in mikebom's internal model), so it should be the
# TARGET of the reversed edge.
jq -r '
    .relationships[]
    | select(.relationshipType == "PROVIDED_DEPENDENCY_OF")
    | "\(.spdxElementId) → \(.relatedSpdxElement)"
' /tmp/m178-us1.spdx.json
```

**Expected**: `SPDXRef-Package-provided-pkg-... → SPDXRef-Package-consumer-pkg-...` (target-then-source, matching the reversed convention).

## Path B — US2: basic-mode preserves `DEPENDS_ON`

**Setup**: same fixture as Path A.

```bash
mikebom --offline sbom scan --path /tmp/m178-us1 \
    --format spdx-json \
    --output /tmp/m178-us2-basic.spdx.json \
    --spdx2-relationship-compat basic \
    --no-deep-hash
```

**Path B.1 — no `PROVIDED_DEPENDENCY_OF` under basic**:

```bash
jq '.relationships[] | select(.relationshipType == "PROVIDED_DEPENDENCY_OF")' /tmp/m178-us2-basic.spdx.json
```

**Expected**: empty output (zero matches).

**Path B.2 — peer edge collapses to natural-direction `DEPENDS_ON`**:

```bash
jq -r '
    .relationships[]
    | select(.relationshipType == "DEPENDS_ON")
    | "\(.spdxElementId) → \(.relatedSpdxElement)"
' /tmp/m178-us2-basic.spdx.json
```

**Expected**: includes a line where `consumer-pkg`'s SPDXRef is the source (`spdxElementId`) and `provided-pkg`'s SPDXRef is the target (`relatedSpdxElement`) — natural direction, as pre-178.

## Path C — US3: annotation retained in both modes

**Setup**: emit under both compat modes (files from Paths A + B).

**Path C.1 — annotation present in full-mode SBOM**:

```bash
jq -r '
    .packages[]
    | select(.annotations[]? | .comment | fromjson? | .field == "mikebom:peer-edge-targets")
    | .name
' /tmp/m178-us1.spdx.json
```

**Expected**: prints `consumer-pkg` — the source of the peer edge.

**Path C.2 — annotation present in basic-mode SBOM**:

```bash
jq -r '
    .packages[]
    | select(.annotations[]? | .comment | fromjson? | .field == "mikebom:peer-edge-targets")
    | .name
' /tmp/m178-us2-basic.spdx.json
```

**Expected**: prints `consumer-pkg` — same as Path C.1.

**Path C.3 — cross-mode annotation-value byte-equality (SC-004)**:

```bash
FULL_ANNO=$(jq -r '
    .packages[]
    | select(.name == "consumer-pkg")
    | .annotations[]
    | select(.comment | fromjson? | .field == "mikebom:peer-edge-targets")
    | .comment
' /tmp/m178-us1.spdx.json)

BASIC_ANNO=$(jq -r '
    .packages[]
    | select(.name == "consumer-pkg")
    | .annotations[]
    | select(.comment | fromjson? | .field == "mikebom:peer-edge-targets")
    | .comment
' /tmp/m178-us2-basic.spdx.json)

[ "$FULL_ANNO" = "$BASIC_ANNO" ] && echo "SC-004 gate holds" || echo "SC-004 FAIL"
```

**Expected**: `SC-004 gate holds`.

## Path D — SC-006/SC-008: non-npm + non-SPDX-2.3 byte-identity

**Setup**: any pre-existing non-npm fixture (cargo, gem, etc.).

```bash
# Emit a fresh SPDX 2.3 SBOM against the cargo fixture:
mikebom --offline sbom scan --path mikebom-cli/tests/fixtures/cargo \
    --format spdx-json \
    --output /tmp/m178-cargo-fresh.spdx.json \
    --no-deep-hash

diff mikebom-cli/tests/fixtures/golden/spdx-2.3/cargo.spdx.json /tmp/m178-cargo-fresh.spdx.json
```

**Expected**: **zero diff** (byte-identical). SC-006 gate holds for non-npm SPDX 2.3.

**CDX + SPDX 3 verification** (SC-008):

```bash
mikebom --offline sbom scan --path mikebom-cli/tests/fixtures/npm \
    --format cyclonedx-json \
    --output /tmp/m178-npm-cdx-fresh.cdx.json \
    --no-deep-hash

diff mikebom-cli/tests/fixtures/golden/cyclonedx/npm.cdx.json /tmp/m178-npm-cdx-fresh.cdx.json
```

**Expected**: **zero diff** — npm CDX golden unchanged. Repeat for SPDX 3 (`--format spdx3-json`).

## Bonus Path — SC-005 FR-007 invariant contract test

**Setup**: same fixture as Path A.

```bash
# Cross-check: every peer-target PURL in the annotation has a
# PROVIDED_DEPENDENCY_OF edge; every PROVIDED_DEPENDENCY_OF edge
# has its source (post-reversal target) in a peer-edge-targets
# annotation.

# Extract every (source_purl, target_purl) tuple from annotations:
ANNO_PAIRS=$(jq -r '
    .packages[] as $src
    | ($src.annotations[]? | .comment | fromjson?
       | select(.field == "mikebom:peer-edge-targets")
       | .value | fromjson) as $targets
    | $targets[] as $target
    | "\($src.externalRefs[] | select(.referenceType == "purl") | .referenceLocator) \($target)"
' /tmp/m178-us1.spdx.json | sort -u)

# Extract every (source_purl, target_purl) tuple from PROVIDED_DEPENDENCY_OF
# edges (accounting for the direction reversal — spdxElementId is target,
# relatedSpdxElement is source):
EDGE_PAIRS=$(jq -r '
    (.packages | map({(.SPDXID): (.externalRefs[]? | select(.referenceType == "purl") | .referenceLocator)}) | add) as $spdxid_to_purl
    | .relationships[]
    | select(.relationshipType == "PROVIDED_DEPENDENCY_OF")
    | "\($spdxid_to_purl[.relatedSpdxElement]) \($spdxid_to_purl[.spdxElementId])"
' /tmp/m178-us1.spdx.json | sort -u)

diff <(echo "$ANNO_PAIRS") <(echo "$EDGE_PAIRS")
```

**Expected**: **zero diff** — bidirectional FR-007 invariant holds. Every annotation-declared peer target has a corresponding edge; every edge has a corresponding annotation entry.

## Full success criteria table

| SC | Verification | Expected |
|---|---|---|
| SC-001 | Path A.1 jq filter | ≥1 `PROVIDED_DEPENDENCY_OF` relationship |
| SC-002 | Path B.1 jq filter | 0 `PROVIDED_DEPENDENCY_OF` relationships |
| SC-003 | Cross-mode structural diff on Path A + B SBOMs (strip only peer-edge relationships) | zero delta outside peer-edge relationshipType |
| SC-004 | Path C.3 annotation value byte-equality | `SC-004 gate holds` |
| SC-005 | Bonus Path FR-007 invariant | zero diff (bidirectional) |
| SC-006 | Path D non-npm SPDX 2.3 golden diff | zero |
| SC-007 | Post-golden-regen review on `npm.spdx.json` | only peer-edge relationshipType + direction flip |
| SC-008 | Path D npm CDX + SPDX 3 golden diffs | zero |
