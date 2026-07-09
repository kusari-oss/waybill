# Implementation Plan: SPDX 2.3 `PROVIDED_DEPENDENCY_OF` for npm peer deps (m178)

**Branch**: `178-spdx23-peer-provided` | **Date**: 2026-07-09 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/178-spdx23-peer-provided/spec.md`

## Summary

**Primary requirement**: at SPDX 2.3 emission time, cross-check each dependency edge against the source component's `mikebom:peer-edge-targets` annotation (populated by m147). If the target's PURL appears in the annotation, emit the edge as `PROVIDED_DEPENDENCY_OF` (reversed direction) under `--spdx2-relationship-compat=full` (default); collapse to `DEPENDS_ON` (natural direction) under `--spdx2-relationship-compat=basic`. Zero changes to CDX 1.6, SPDX 3.0.1, or the resolver — this is purely a SPDX 2.3 emitter refinement.

**Technical approach**: **annotation-driven emission at the SPDX 2.3 emitter**. Two options considered (see research §R1); chosen path:

1. **Pre-compute a peer-edge lookup set** at the top of `build_relationships` in `mikebom-cli/src/generate/spdx/relationships.rs`. Iterate `artifacts.components`; for each component with `extra_annotations["mikebom:peer-edge-targets"]` populated (JSON-array-in-string per m147 wire convention), parse the array and insert `(source_purl, target_purl)` tuples into a `HashSet<(String, String)>`. O(N) time, O(P) auxiliary space where P is peer-edge count.

2. **Extend the existing `match (compat, kind)` block** at line 186 to add a new arm that fires BEFORE the generic `DependsOn` arm. Predicate: `RelationshipType::DependsOn && peer_edges_set.contains(&(rel.from.clone(), rel.to.clone()))`. Under `Full` mode → `SpdxRelationshipType::ProvidedDependencyOf` with REVERSED direction (matches the m228 `DevDependencyOf` / `BuildDependencyOf` / `TestDependencyOf` precedent). Under `Basic` mode → the existing catch-all Basic arm already collapses to `DependsOn` natural-direction (no new handling needed — peer edges naturally fall through to the same behavior as regular DependsOn under Basic).

3. **Add `ProvidedDependencyOf` variant to `SpdxRelationshipType` enum** at line 31 of `relationships.rs`. Serialized as `PROVIDED_DEPENDENCY_OF` via the existing `SCREAMING_SNAKE_CASE` serde attribute (automatic — Rust `ProvidedDependencyOf` → SCREAMING_SNAKE_CASE gives `PROVIDED_DEPENDENCY_OF`).

**No changes to `mikebom-common::RelationshipType`**: the resolver continues to emit peer edges as `RelationshipType::DependsOn` (unchanged from m147). The SPDX 2.3 emitter is the ONLY site that distinguishes peer from non-peer, via the annotation cross-check. This keeps the change contained to a single crate + a single file per FR-004/FR-005 (CDX + SPDX 3 unchanged).

**Optional peer deps** (per Q1 clarification): the classifier does NOT distinguish mandatory vs optional. All entries in `mikebom:peer-edge-targets` fire `PROVIDED_DEPENDENCY_OF` uniformly. m147's annotation predicate already includes both `peerDependencies` and (m147 code inspection confirms) any `peerDependenciesMeta`-flagged entries that survive the m147 resolution predicate; m178 inherits that same treatment.

**Docs updates**:
- **`docs/reference/reading-a-mikebom-sbom.md`** — the existing `mikebom:peer-edge-targets` subsection gets a new sub-paragraph: "SPDX 2.3 primary signal post-m178 is `PROVIDED_DEPENDENCY_OF` (native); annotation remains as fine-grained target list; compat-basic falls back to `DEPENDS_ON`."
- **`docs/reference/sbom-format-mapping.md`** — the C-row for `mikebom:peer-edge-targets` gets an SPDX 2.3 column update citing `PROVIDED_DEPENDENCY_OF` (full) / `DEPENDS_ON` (basic) as the primary native signal alongside the annotation as the finer-grained supplement.

**Cross-format parity**: no new parity extractor needed. `mikebom:peer-edge-targets` annotation VALUE is `SymmetricEqual` across all three formats (unchanged). The relationship-type change on SPDX 2.3 is captured by the existing structural-edges extractor which already differs per format by design (CDX has no scoped-dep-type equivalent; SPDX 3's `LifecycleScopedRelationship.scope` carries scope on its own extractor).

**Ripple**: SPDX 2.3 golden regeneration on the npm fixture (empirically: `npm.spdx.json` is the primary flip candidate — need to verify at implementation time; other formats' npm goldens MUST stay byte-identical per SC-008).

**Blast radius**: ~15 lines in `mikebom-cli/src/generate/spdx/relationships.rs` (one enum variant + one match arm + one peer-edge-set pre-compute at function head), ~40 lines in docs (2 files), ~200 lines in a new integration test file at `mikebom-cli/tests/spdx23_peer_provided.rs` (5-6 tests covering US1/US2/US3 + FR-007 invariant), ~1 SPDX 2.3 golden regenerated with bounded delta.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–177; local + CI both on 1.97). No nightly required.

**Primary Dependencies**: Existing only — `serde_json` (already used pervasively; here for parsing the peer-edge-targets annotation), `std::collections::HashSet` (peer-edge lookup set), existing `mikebom_common::resolution::{RelationshipType, ResolvedComponent}` types. **Zero new Cargo dependencies.** No subprocess calls. No network access.

**Storage**: N/A — pure emission-time classification; no persistence.

**Testing**: `cargo test` — 1 new integration test at `mikebom-cli/tests/spdx23_peer_provided.rs` covering the 3 US acceptance predicates + FR-007 invariant + SC-005 cross-check. Plus 2-3 unit tests inline in `relationships.rs` for the new `ProvidedDependencyOf` serialization + the compat-mode match-arm behavior on synthesized components.

**Target Platform**: All hosts mikebom builds on — Linux, macOS, Windows.

**Project Type**: cli (mikebom sbom-generation CLI).

**Performance Goals**: N/A — the peer-edge lookup set is `O(N + P)` construction where N is component count and P is peer-edge count; per-edge check is `O(1)`. Happens once per scan at emission time (same site as the existing m228 typed-relationship-type dispatch).

**Constraints**:
- **SC-006 gate** — non-npm SPDX 2.3 goldens byte-identical pre-178 vs post-178.
- **SC-007 gate** — npm SPDX 2.3 goldens show ONLY peer-edge relationship-type flip (and possibly direction reversal) as delta.
- **SC-008 gate** — CDX 1.6 + SPDX 3.0.1 goldens byte-identical.
- **FR-007 invariant** — bidirectional: every peer-annotation-listed target has a `PROVIDED_DEPENDENCY_OF` edge in full-mode, AND every such edge has its target listed in the source's annotation. Verified by contract test.

**Scale/Scope**: Small. 1 code file touched, 2 docs files touched, 1 new integration test file, ~1 SPDX 2.3 golden regenerated with bounded per-fixture delta.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **I. Pure Rust, Zero C**: ✅ No new Cargo dependencies. Pure Rust addition.
- **II. eBPF-Only Observation**: ✅ `mikebom-ebpf` untouched. mikebom does NOT change discovery; only refines emission.
- **III. Fail Closed**: ✅ If the annotation is missing or malformed on a component, that component's edges fall back to the existing `DependsOn` treatment (fail-open on the peer-classification path is correct — misses the type refinement but doesn't corrupt the SBOM). No change to any existing fail-closed invariants.
- **IV. Type-Driven Correctness**: ✅ New enum variant added to `SpdxRelationshipType` (existing serde-derived). No new domain types. No `.unwrap()` in production — annotation parse uses `serde_json::from_str(...).ok()` fail-open.
- **V. Specification Compliance / Standards-native precedence**: ✅ **THIS IS THE DIRECT MOTIVATION.** m178 IS the Principle V native-first migration: use SPDX 2.3's native `PROVIDED_DEPENDENCY_OF` as the primary signal for peer semantic; keep `mikebom:peer-edge-targets` as the finer-grained "which specific targets" carve-out per Principle V's "carry information the standard doesn't natively express" clause. No new `mikebom:*` annotation introduced. The compat-basic fallback exists specifically because m228 codified the operator-facing escape hatch semantics — doesn't dilute Principle V compliance for consumers using the standard.
- **VI. Three-Crate Architecture**: ✅ Change contained to `mikebom-cli`. Zero changes to `mikebom-common` or `mikebom-ebpf`. Existing `RelationshipType` enum unchanged.
- **VII. Test Isolation**: ✅ Integration test uses per-test tempdir.
- **VIII. Completeness**: ✅ Not affected — no components added or removed; no edges suppressed.
- **IX. Accuracy**: ✅ **Advances Principle IX** — SPDX 2.3 consumers now see peer edges as `PROVIDED_DEPENDENCY_OF`, accurately reflecting the npm peer-dep semantic.
- **X. Transparency**: ✅ **Advances Principle X** — the semantic distinction is now expressible in the native format, not hidden inside an annotation only mikebom-aware tools decode.
- **XI. Enrichment**: N/A — no enrichment path touched.
- **XII. External Data Source Enrichment**: N/A.

**Strict Boundaries check**:
- **New subprocess**: ✅ None.
- **New network access**: ✅ None.
- **New filesystem writes**: ✅ None. Purely emission-time refinement.
- **New `mikebom:*` annotation namespaces**: ✅ **Zero.** The existing `mikebom:peer-edge-targets` (m147) is REUSED as the classifier substrate.
- **New Cargo dependencies**: ✅ Zero.
- **Strict Boundary §5 (file-tier no-duplicates)**: N/A — file-tier walker unaffected.

**Verdict**: All principles pass. Zero violations. Milestone directly advances Principle V (the standards-native precedence rule) — it's the CANONICAL implementation of Principle V for a scoped native relationship type. Also advances Principles IX / X.

## Project Structure

### Documentation (this feature)

```text
specs/178-spdx23-peer-provided/
├── spec.md              # Feature specification (already written + Q1 clarified)
├── plan.md              # This file
├── research.md          # Phase 0 — annotation-driven vs enum-variant approach + directionality contract + fixture inventory
├── data-model.md        # Phase 1 — new SpdxRelationshipType variant + peer-edge lookup set + match-arm insertion
├── quickstart.md        # Phase 1 — 3-scenario verification (US1 full-mode, US2 basic-mode fallback, US3 annotation retention)
├── contracts/           # Phase 1 — SPDX 2.3 relationship-type wire contract + FR-007 invariant contract
├── checklists/          # Requirements checklist (spec-phase output — 16/16 PASS post-Q1)
└── tasks.md             # Phase 2 output (/speckit.tasks — NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   └── generate/
│       └── spdx/
│           └── relationships.rs         # ~15 lines: new ProvidedDependencyOf variant + peer-edge lookup set + match arm
└── tests/
    └── spdx23_peer_provided.rs         # NEW ~200 lines: 5-6 US1/US2/US3 integration tests

docs/reference/
├── reading-a-mikebom-sbom.md            # ~40 lines: extend the m147 peer-edge subsection with m178 SPDX 2.3 primary signal
└── sbom-format-mapping.md               # ~5 lines: update the C-row for peer-edge-targets SPDX 2.3 column

# NO changes to:
mikebom-common/                          # RelationshipType enum unchanged
mikebom-cli/src/scan_fs/                 # m147 npm reader unchanged
mikebom-cli/src/generate/cyclonedx/      # CDX unchanged (FR-004)
mikebom-cli/src/generate/spdx/v3_*.rs    # SPDX 3 unchanged (FR-005)
mikebom-cli/src/parity/                  # No new parity extractor

mikebom-cli/tests/fixtures/golden/       # SPDX 2.3 npm golden regenerates with bounded delta; CDX + SPDX 3 goldens byte-identical
```

**Structure Decision**: single-file classifier extension in the SPDX 2.3 emitter. One enum variant + one peer-edge-set pre-compute + one match arm. Docs + integration test + bounded SPDX 2.3 npm-golden regeneration. Zero ripple to CDX, SPDX 3, resolver, or `mikebom-common`.

## Complexity Tracking

No constitution violations to justify. The plan is a straight-line SPDX 2.3 emitter refinement that IS the canonical Principle V native-first implementation for a scoped relationship type. Golden regeneration is bounded (npm SPDX 2.3 only) and semantically intentional.
