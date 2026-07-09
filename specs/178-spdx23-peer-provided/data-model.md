# Phase 1 Data Model: SPDX 2.3 PROVIDED_DEPENDENCY_OF for npm peer deps (m178)

**Feature**: 178-spdx23-peer-provided
**Date**: 2026-07-09

Four entities. Zero new types on `mikebom-common` (per Constitution VI — this is a `mikebom-cli`-only milestone). One new `SpdxRelationshipType` enum variant. One new peer-edge lookup set. One new match arm. Two docs extensions.

## Entity 1 — `SpdxRelationshipType::ProvidedDependencyOf` enum variant

**Location**: `mikebom-cli/src/generate/spdx/relationships.rs`, inserted alphabetically-adjacent to the existing typed dep-scope variants (`DevDependencyOf` / `BuildDependencyOf` / `TestDependencyOf`).

**Shape**:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[allow(dead_code)]
pub enum SpdxRelationshipType {
    Describes,
    DependsOn,
    DevDependencyOf,
    BuildDependencyOf,
    TestDependencyOf,
    /// Milestone 178 — SPDX 2.3 §11.1 native semantic for npm peer
    /// deps: "SPDXRef-A depends on SPDXRef-B as a provided
    /// dependency" (source consumer provides the target dep).
    /// Emitted reversed-direction per m228 convention: internal
    /// `A DependsOn B` where B appears in A's `mikebom:peer-edge-
    /// targets` annotation → SPDX `B PROVIDED_DEPENDENCY_OF A`.
    ProvidedDependencyOf,
    Contains,
    ContainedBy,
    BuiltFrom,
}
```

**Contract**:
- **Serialization**: Rust `ProvidedDependencyOf` → SCREAMING_SNAKE_CASE → `PROVIDED_DEPENDENCY_OF`. Automatic via existing serde attribute.
- **Direction convention**: **reversed** (per m228 precedent) — internal `A DependsOn B` becomes SPDX `B PROVIDED_DEPENDENCY_OF A`.
- **Only fires under `Spdx2RelationshipCompat::Full`**. Basic mode falls through to the existing catch-all `DependsOn` collapse (R3 disposition).

## Entity 2 — Peer-edge lookup set

**Location**: `mikebom-cli/src/generate/spdx/relationships.rs::build_relationships`, computed at the top of the function before the relationships loop (line ~155 area, immediately after the `purl_to_id` map construction).

**Shape**:

```rust
// Milestone 178 — pre-compute peer-edge lookup set from the m147
// annotation on source components. Fail-open on missing/malformed
// annotations: the source's edges fall through to the existing
// DependsOn treatment. Runs once per SPDX 2.3 build, O(N + P).
let peer_edges: HashSet<(String, String)> = {
    let mut set = HashSet::new();
    for c in artifacts.components {
        let Some(value) = c.extra_annotations.get("mikebom:peer-edge-targets") else {
            continue;
        };
        let Some(s) = value.as_str() else {
            continue;
        };
        let Ok(targets) = serde_json::from_str::<Vec<String>>(s) else {
            continue;
        };
        for target_purl in targets {
            set.insert((c.purl.as_str().to_string(), target_purl));
        }
    }
    set
};
```

**Contract**:
- **Key**: `(source_purl_string, target_purl_string)` — both stringified via `Purl::as_str()`.
- **Fail-open**: missing annotation, non-string value, or malformed JSON all silently skip. The affected component's edges fall through to `DependsOn` treatment — no crash, no partial-classification state.
- **PURL string matching**: relies on the m147 wire contract that peer-edge-targets values are byte-identical to the target components' `.purl` values (verified via m147's `pkg:npm/{dep_name}@{version}` format string at `package_lock.rs:223`).

## Entity 3 — Match-arm insertion for peer edges

**Location**: `mikebom-cli/src/generate/spdx/relationships.rs::build_relationships`, inserted in the `match (compat, kind)` block at line ~186, BEFORE the generic `DependsOn` arm.

**Shape**:

```rust
let (source, target, kind) = match (
    artifacts.spdx2_relationship_compat,
    rel.relationship_type.clone(),
) {
    // Milestone 178 — peer edges under Full mode fire
    // PROVIDED_DEPENDENCY_OF reversed-direction. Matches m228
    // typed-dep-scope precedent (Dev/Build/Test DependencyOf).
    // Basic mode falls through to the existing catch-all Basic arm
    // below (SC-002 satisfied by construction).
    (crate::generate::Spdx2RelationshipCompat::Full, RelationshipType::DependsOn)
        if peer_edges.contains(&(rel.from.clone(), rel.to.clone())) =>
    {
        (to_id, from_id, SpdxRelationshipType::ProvidedDependencyOf)
    }
    (_, RelationshipType::DependsOn) => {
        (from_id, to_id, SpdxRelationshipType::DependsOn)
    }
    (crate::generate::Spdx2RelationshipCompat::Basic, _) => {
        (from_id, to_id, SpdxRelationshipType::DependsOn)
    }
    // ... existing Dev/Build/Test arms unchanged ...
};
```

**Contract**:
- **Guard clause pattern**: `if peer_edges.contains(...)` on the m178 arm — Rust `match` supports guards inline.
- **Ordering matters**: the m178 arm MUST appear BEFORE the generic `DependsOn` arm. If Rust match evaluates the generic first (which it doesn't — `match` evaluates in source order), we'd miss peer edges. Source-order-first-match is guaranteed.
- **Direction reversal**: `(to_id, from_id, ...)` — the tuple's source and target are swapped compared to the natural-direction arm.

## Entity 4 — Docs subsection extension in `reading-a-mikebom-sbom.md`

**Location**: `docs/reference/reading-a-mikebom-sbom.md`, the existing `mikebom:peer-edge-targets` subsection (around line 657 per m147 authoring).

**Change shape**: append a new sub-paragraph after the existing "What to do with it" content:

```markdown
> **Milestone 178 — SPDX 2.3 native primary signal**: post-m178, peer-driven edges in SPDX 2.3 output carry `relationshipType: "PROVIDED_DEPENDENCY_OF"` (reversed direction — matches SPDX spec semantic "B is a provided dependency of A") under the default `--spdx2-relationship-compat=full`. Under `--spdx2-relationship-compat=basic`, peer edges collapse to `DEPENDS_ON` (natural direction, pre-178 behavior preserved for downstream consumers with basic-vocabulary tooling per m228). The `mikebom:peer-edge-targets` annotation remains present in BOTH modes as the finer-grained "which specific peer targets" supplement — Principle V's "carry information the standard doesn't natively express" carve-out. Consumers walking SPDX 2.3 typed relationship types now see the peer distinction natively without needing to parse the annotation.
>
> **CDX 1.6 and SPDX 3.0.1 unchanged**: neither format has a native peer construct (CDX has no analog to `PROVIDED_DEPENDENCY_OF`; SPDX 3's `LifecycleScopeType` enum lacks a `peer` value). The annotation remains the primary signal in both formats.
>
> **jq recipe** — extract peer edges from SPDX 2.3 output (post-178, full mode):
>
> ```jq
> .relationships[]
>     | select(.relationshipType == "PROVIDED_DEPENDENCY_OF")
> ```
>
> Returns every peer edge. Reads as `{source: <target-package-SPDXID>, target: <source-package-SPDXID>, ...}` per the reversed-direction convention.
```

## Entity 5 — `sbom-format-mapping.md` C-row extension

**Location**: `docs/reference/sbom-format-mapping.md`, the existing C-row for `mikebom:peer-edge-targets` (exact C-number determined at implementation time via grep).

**Change shape**: extend the SPDX 2.3 column to cite `PROVIDED_DEPENDENCY_OF` as the primary native signal + note the compat-basic fallback + retain the annotation as finer-grained supplement.

Concretely: the row's SPDX 2.3 column currently says something like "annotation on Package". Post-178 it should read:

> Primary native signal: `PROVIDED_DEPENDENCY_OF` relationship (reversed direction) under `--spdx2-relationship-compat=full` (default); collapses to `DEPENDS_ON` (natural direction) under `--spdx2-relationship-compat=basic`. Supplemental annotation on the source Package (identical in both modes): `MikebomAnnotationCommentV1` envelope carrying the JSON-encoded array of peer-target PURLs — Principle V's "carry information the standard doesn't natively express" carve-out (SPDX 2.3's native `PROVIDED_DEPENDENCY_OF` says "this edge is peer" but not "which specific targets are declared"). Milestone 178.

## Cross-entity invariants (post-178)

1. **FR-007 bidirectional invariant**: every entry in a source component's `mikebom:peer-edge-targets` annotation MUST correspond to exactly one `PROVIDED_DEPENDENCY_OF` edge (full-mode) or `DEPENDS_ON` edge (basic-mode) in the emitted SPDX 2.3 output. Conversely, every `PROVIDED_DEPENDENCY_OF` edge MUST have its (reversed-direction) source appearing in the (forward-direction) source component's peer-edge-targets. Verified by SC-005 contract test.
2. **Basic-mode byte-identity for peer-edge relationship-type**: FR-002 gate — under Basic mode, peer edges emit `DEPENDS_ON` natural direction, byte-identical to pre-178.
3. **CDX + SPDX 3 byte-identity**: FR-004 + FR-005 — neither format's output is touched by this milestone.
4. **Non-npm SPDX 2.3 byte-identity**: FR-006 — non-npm goldens (cargo, gem, maven, pip, apk, deb, rpm, etc.) show zero drift.

## State transitions

None. Pure emission-time classification. No lifecycle events.
