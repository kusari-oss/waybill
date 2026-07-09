# Phase 0 Research: SPDX 2.3 PROVIDED_DEPENDENCY_OF for npm peer deps (m178)

**Feature**: 178-spdx23-peer-provided
**Date**: 2026-07-09

Four research questions resolved by inspection of the m147 annotation-emission code + m228 SPDX 2.3 emission architecture + review of the `SpdxRelationshipType` enum. Every question was answerable without spawning subagents — the pattern-matches all trace to existing milestones.

---

## R1 — Annotation-driven emission vs new `RelationshipType` enum variant?

**Decision**: **Annotation-driven emission** at the SPDX 2.3 emitter.

**Rationale**:
- **Simpler**: single-file change in `mikebom-cli/src/generate/spdx/relationships.rs`. Zero changes to `mikebom-common`. Zero changes to the m147 npm reader.
- **Idempotent with FR-007 invariant by construction**: since the annotation is the SOLE substrate for peer identification, any edge that appears in `mikebom:peer-edge-targets` will emit as `PROVIDED_DEPENDENCY_OF` (and vice versa). The invariant holds trivially.
- **Doesn't ripple to CDX or SPDX 3**: the classification happens only in the SPDX 2.3 emitter — CDX and SPDX 3 continue to emit the same `dependsOn`-shape edge they do today. Zero downstream ripple.
- **Fail-open behavior**: if the annotation is missing (e.g., pre-m147 upstream, or a future reader that doesn't emit it), the peer edge falls through to `DependsOn` — semantically correct fallback.

**Alternative — new `RelationshipType::PeerDependsOn` variant in `mikebom-common`**:
- Would require: (a) enum variant addition, (b) m147 npm reader emit `PeerDependsOn` instead of `DependsOn` when populating the annotation, (c) CDX emitter downgrade `PeerDependsOn → dependsOn`, (d) SPDX 3 emitter downgrade `PeerDependsOn → dependsOn`, (e) all three emitters' tests updated.
- Pros: type-safe; peer status is captured at the resolver level, not annotation cross-check level; matches Dev/Build/Test precedent architecturally.
- Cons: 5x the surface area for zero user-visible behavior gain. Ripples through three crates and multiple emitters. Overkill for a single-format change.

**Chosen**: annotation-driven, per rationale above. If a future milestone finds compelling reason to elevate peer status to the type system (e.g., CDX 2.x adds a native peer construct), the migration to `RelationshipType::PeerDependsOn` becomes a natural refactor at that time.

---

## R2 — Directionality contract for `PROVIDED_DEPENDENCY_OF`?

**Decision**: **Reversed direction**, matching the m228 convention for typed dep-scope relationship types (`DEV_DEPENDENCY_OF`, `BUILD_DEPENDENCY_OF`, `TEST_DEPENDENCY_OF`).

**Rationale**:
- **SPDX 2.3 spec semantic**: "SPDXRef-A depends on SPDXRef-B as a provided dependency" reads as "A depends on B; B is provided by someone else."
- Grammatically parsed: `(A) PROVIDED_DEPENDENCY_OF (B)` = "A is a provided dependency of B" = "A is provided so that B can use it." Which reverses conventional dep-graph direction.
- **m228 established the convention**: internal `A DevDependsOn B` (meaning "A needs B for dev") emits as SPDX `B DEV_DEPENDENCY_OF A`. Verified at `relationships.rs:196–200`.
- Post-m178 analog: internal `A DependsOn B` where B is in A's peer-edge-targets emits as SPDX `B PROVIDED_DEPENDENCY_OF A` under full mode.

**Alternative — natural direction (`A PROVIDED_DEPENDENCY_OF B`)**:
- Rejected. Would break m228 convention consistency. Consumers walking SPDX 2.3 typed dep-scope relationships already expect reversed direction; introducing an unreversed variant would be surprising.

---

## R3 — Basic-compat mode handling — does it need a new match arm?

**Decision**: **No.** The existing catch-all Basic arm already collapses ALL relationship types to `DependsOn` natural-direction. Peer edges naturally fall through to the same behavior as regular DependsOn under Basic.

**Verified via**: `relationships.rs:193–195` — `(crate::generate::Spdx2RelationshipCompat::Basic, _)` matches ANY relationship type under Basic mode and emits `SpdxRelationshipType::DependsOn` natural-direction.

**Implication**: the m178 match arm only needs to fire under `Spdx2RelationshipCompat::Full`. Basic-mode falls through the same as m228 already does — byte-identical to pre-178 output for peer edges under Basic. This satisfies SC-002 by construction.

**Alternative — explicit Basic arm for peer**:
- Rejected. Would duplicate the existing catch-all behavior. No functional difference; extra code with no signal.

---

## R4 — Which existing goldens will regenerate?

**Decision**: **SPDX 2.3 npm golden (`npm.spdx.json`) is the primary flip candidate.** Empirically determined at implementation time via `MIKEBOM_UPDATE_SPDX_GOLDENS=1` regen + diff review.

**Rationale**:
- Fixture inventory scan: only fixtures that trigger m147 peer-edge annotation emission will regen. Grep-verified: `mikebom-cli/tests/fixtures/golden/spdx-2.3/npm.spdx.json` contains `mikebom:peer-edge-targets` entries.
- Other SPDX 2.3 goldens (cargo, gem, maven, pip, apk, deb, rpm, bazel, cmake, golang) have zero npm peer-edge annotations. They MUST stay byte-identical per SC-006.
- CDX 1.6 and SPDX 3.0.1 goldens for npm MUST stay byte-identical per SC-008 (m178 changes SPDX 2.3 emission only).

**SC-007 gate**: post-regen, the npm SPDX 2.3 golden should show:
- Peer edges: `relationshipType` flips `DEPENDS_ON` → `PROVIDED_DEPENDENCY_OF` AND direction flips natural → reversed.
- Non-peer edges: byte-identical (natural-direction `DEPENDS_ON`).
- `mikebom:peer-edge-targets` annotation: byte-identical (unchanged).
- Every other byte: unchanged.

**Fallback strategy** if regen shows unexpected drift outside the expected scope:
- Diff line-by-line to identify the surprise category.
- Common causes: (a) SPDX ID enumeration order — the peer-edge direction flip changes which edge is "first" in a sort; (b) `mikebom:lifecycle-scope` cross-contamination if a peer edge was previously being tagged as a regular DependsOn with a lifecycle-scope annotation.

**Alternatives considered**:
- **Pre-enumerate expected diffs before regen**: rejected — golden diffs are easier to review by inspection than predict. The SC-007 gate is defensively worded to catch surprises at review time.
- **Skip npm golden regen; write a new golden that only exercises peer-edges**: rejected — the existing fixture is broad-coverage; ripping peer edges out would lose coverage of other npm-emission code paths.

---

## Summary table

| ID | Question | Decision |
|---|---|---|
| R1 | Annotation-driven emission vs enum variant? | Annotation-driven — single-file SPDX 2.3 emitter change; FR-007 invariant holds by construction; fail-open on missing annotation |
| R2 | Directionality for `PROVIDED_DEPENDENCY_OF`? | Reversed direction per m228 convention — internal `A DependsOn B` (peer) → SPDX `B PROVIDED_DEPENDENCY_OF A` |
| R3 | New Basic-mode match arm needed? | No — existing catch-all Basic arm collapses peer edges to `DependsOn` naturally |
| R4 | Which goldens regenerate? | SPDX 2.3 `npm.spdx.json` primary flip candidate; empirical verification at regen time; CDX + SPDX 3 goldens stay byte-identical per SC-008 |
