# Contract: SPDX 2.3 `PROVIDED_DEPENDENCY_OF` wire format + FR-007 invariant

**Feature**: 178-spdx23-peer-provided
**Date**: 2026-07-09

Authoritative reference for the new relationship-type wire shape, directionality, compat-mode behavior, and the FR-007 bidirectional invariant.

## Wire format — full mode

**Relationship-type value**: `"PROVIDED_DEPENDENCY_OF"` (SCREAMING_SNAKE_CASE).

**Direction**: reversed relative to internal `A DependsOn B` (where B appears in A's peer-edge-targets). Emitted as SPDX `B PROVIDED_DEPENDENCY_OF A`.

**Example** — package `a@1.0.0` declares `b@2.0.0` as a peer:
```json
{
  "spdxElementId": "SPDXRef-Package-b-2.0.0",
  "relatedSpdxElement": "SPDXRef-Package-a-1.0.0",
  "relationshipType": "PROVIDED_DEPENDENCY_OF"
}
```

Reads: "package `b` is a provided dependency of package `a`" — matches the npm peer-dep semantic and SPDX spec grammar.

## Wire format — basic mode

**Relationship-type value**: `"DEPENDS_ON"` (unchanged from pre-178).

**Direction**: natural (unchanged from pre-178).

**Example** — same fixture as above:
```json
{
  "spdxElementId": "SPDXRef-Package-a-1.0.0",
  "relatedSpdxElement": "SPDXRef-Package-b-2.0.0",
  "relationshipType": "DEPENDS_ON"
}
```

Byte-identical to pre-178 output for peer edges under `--spdx2-relationship-compat=basic`.

## Emission predicate

The m178 classifier fires when ALL THREE conditions hold:

1. `spdx2_relationship_compat == Full` (compat-basic peer edges collapse to `DEPENDS_ON` via the existing catch-all Basic arm).
2. `rel.relationship_type == RelationshipType::DependsOn` (internal type is always `DependsOn` for peer edges per m147's merge-into-depends_set behavior).
3. `peer_edges.contains(&(rel.from, rel.to))` where `peer_edges` is the pre-computed HashSet from parsing every component's `mikebom:peer-edge-targets` annotation.

**Fail-open on missing/malformed annotation**: the source component's edges fall through to the generic `DependsOn` treatment. The peer classification is best-effort; malformed annotations do NOT corrupt the SBOM.

## FR-007 bidirectional invariant

**Forward direction**: every peer-target PURL listed in `mikebom:peer-edge-targets` on a source Package MUST produce exactly one edge with the mode-appropriate relationship type:
- Full mode: `PROVIDED_DEPENDENCY_OF` reversed direction.
- Basic mode: `DEPENDS_ON` natural direction.

**Reverse direction**: every `PROVIDED_DEPENDENCY_OF` edge in the emitted SPDX 2.3 output MUST have its (reversed-direction) source appearing in the (forward-direction) source component's `mikebom:peer-edge-targets` annotation.

**Verification**: SC-005 contract test cross-checks:
```jq
# Extract all peer-target PURLs from annotations (forward index):
$sbom | .packages[]
     | select(.annotations[]? | .comment | fromjson? | .field == "mikebom:peer-edge-targets")
     | { source_spdxid: .SPDXID, targets: (.annotations[] | select(.comment | fromjson? | .field == "mikebom:peer-edge-targets") | .comment | fromjson | .value | fromjson) }
```

For each `(source_spdxid, target_purl)` pair, assert there's exactly one relationship where:
- Full mode: `relatedSpdxElement == source_spdxid && relationshipType == "PROVIDED_DEPENDENCY_OF" && spdxElementId maps to a Package whose PURL == target_purl`
- Basic mode: `spdxElementId == source_spdxid && relationshipType == "DEPENDS_ON" && relatedSpdxElement maps to a Package whose PURL == target_purl`

## Cross-format contract

- **CDX 1.6**: unchanged. Peer edges continue to emit as `dependsOn` (there's no native peer construct in CDX). The `mikebom:peer-edge-targets` annotation on the source Component remains the sole peer signal.
- **SPDX 2.3**: NEW BEHAVIOR (this milestone). See sections above.
- **SPDX 3.0.1**: unchanged. Peer edges continue to emit as `dependsOn` (SPDX 3's `LifecycleScopeType` enum lacks a `peer` value). The `mikebom:peer-edge-targets` annotation on the source `software_Package` remains the sole peer signal.

## Compat-mode flag inheritance

**No new CLI flag.** m178 reuses the m228 `--spdx2-relationship-compat=<full|basic>` flag exclusively. Semantic:

| Flag value | Peer-edge SPDX 2.3 emission |
|---|---|
| `--spdx2-relationship-compat=full` (default) | `PROVIDED_DEPENDENCY_OF` reversed direction |
| Flag omitted | `PROVIDED_DEPENDENCY_OF` reversed direction (Full is default per m228) |
| `--spdx2-relationship-compat=basic` | `DEPENDS_ON` natural direction (pre-178 behavior) |

## Non-goals

- No changes to the internal `RelationshipType` enum (`mikebom-common`).
- No changes to CDX or SPDX 3 emitters.
- No changes to the m147 npm reader.
- No new CLI flags.
- No changes to `mikebom:peer-edge-targets` annotation value or emission (byte-identical pre-178 vs post-178).
- No distinction between mandatory and optional peer deps (per Q1 clarification — Option A).

## Milestone context

- **m147** — introduced `mikebom:peer-edge-targets`; the annotation is the classifier substrate for m178.
- **m163** — added logic to suppress phantom peer edges (unresolved-target peers); pre-processes before m178 sees the edges.
- **m228** — introduced `--spdx2-relationship-compat=<full|basic>` and established the reversed-direction convention for typed dep-scope relationships. m178 inherits both.
- **m178 (this feature)** — completes the Principle V native-first migration for npm peer semantics on SPDX 2.3.
