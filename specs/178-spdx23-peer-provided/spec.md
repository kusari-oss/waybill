# Feature Specification: SPDX 2.3 `PROVIDED_DEPENDENCY_OF` for npm peer deps (Principle V native-first)

**Feature Branch**: `178-spdx23-peer-provided`
**Created**: 2026-07-09
**Status**: Draft
**Input**: User description: "SPDX 2.3 emit PROVIDED_DEPENDENCY_OF for npm peer deps (Principle V native-first, per issue #526)"

## Clarifications

### Session 2026-07-09

- Q: How to handle optional peer deps (`peerDependenciesMeta[<name>].optional = true`) under SPDX 2.3, given SPDX has both `PROVIDED_DEPENDENCY_OF` and `OPTIONAL_DEPENDENCY_OF` as separate native types with no combined variant? → A: **Always `PROVIDED_DEPENDENCY_OF` for all peer edges**. Ignore the optional distinction on the relationship-type axis. The peer semantic is what `mikebom:peer-edge-targets` is fundamentally about; the mandatory-vs-optional distinction is a finer-grained refinement that can layer on later via a new annotation IF consumer demand emerges (deferred; not in m178 scope). Keeps the taxonomy simpler and defers complexity until proven necessary.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — SPDX 2.3 consumer reads peer semantic from native relationship type (Priority: P1)

A downstream SBOM consumer parses an emitted SPDX 2.3 document and walks the `relationships[]` array. For each edge, they inspect `relationshipType`. Post-178, peer-driven edges (from npm `peerDependencies` in the lockfile) carry `PROVIDED_DEPENDENCY_OF` instead of the generic `DEPENDS_ON`. The consumer distinguishes install-driven peer edges from functional-dep edges via the native SPDX 2.3 relationship type — no need to inspect the `mikebom:peer-edge-targets` annotation.

**Why this priority**: this is the load-bearing Principle V correctness fix. Consumers parsing only relationship types (the vast majority of SPDX 2.3 tooling) currently see peer edges as ordinary `DEPENDS_ON` — losing the peer distinction that the standard actually provides a native carrier for. Ranked P1 because it's the direct standards-compliance improvement per Constitution Principle V (v1.4.0): "standards-native fields take precedence over `mikebom:`-prefixed properties."

**Independent Test**: emit an SPDX 2.3 SBOM from a fixture containing at least one npm package with `peerDependencies` in the lockfile. Assert the peer-driven `SPDXRef-source ↔ SPDXRef-target` relationship carries `relationshipType: "PROVIDED_DEPENDENCY_OF"` (not `"DEPENDS_ON"`).

**Acceptance Scenarios**:

1. **Given** an emitted SPDX 2.3 SBOM from an npm scan where package A declares B as a peer dep, **When** the consumer walks `relationships[]` and finds the A→B edge, **Then** the edge carries `relationshipType: "PROVIDED_DEPENDENCY_OF"`.
2. **Given** the same SBOM, **When** the consumer walks non-peer edges (regular `dependencies` from the lockfile), **Then** those edges carry `relationshipType: "DEPENDS_ON"` unchanged.
3. **Given** a consumer that categorizes edges by relationship type (e.g., a vulnerability scanner that treats provided deps differently from required deps), **When** they receive an emitted SBOM, **Then** they see the peer/functional distinction natively — no annotation-parsing required.

---

### User Story 2 — Basic-compat mode preserves DEPENDS_ON for legacy consumers (Priority: P1)

An operator running mikebom against a downstream toolchain that implements only the basic SPDX 2.3 relationship vocabulary (`DESCRIBES` / `CONTAINS` / `DEPENDS_ON`) sets `--spdx2-relationship-compat=basic`. Post-178, peer edges collapse back to `DEPENDS_ON` under basic mode — the same behavior as pre-178. The `mikebom:peer-edge-targets` annotation remains present so consumers who DO care about the peer distinction can still recover it from the annotation.

**Why this priority**: m228 already introduced the `--spdx2-relationship-compat` flag specifically because some downstream SBOM tools silently ignore relationship types beyond the basic three. m178 MUST respect this flag or it breaks the m228 escape hatch. Ranked P1 alongside US1 because breaking basic-compat mode is a regression for existing operators.

**Independent Test**: emit the same npm-with-peer-deps fixture twice — once with `--spdx2-relationship-compat=full` (default) and once with `--spdx2-relationship-compat=basic`. Assert full-mode uses `PROVIDED_DEPENDENCY_OF`; basic-mode uses `DEPENDS_ON`. Both modes retain the annotation.

**Acceptance Scenarios**:

1. **Given** a scan invoked with `--spdx2-relationship-compat=full` (or the default when the flag is omitted), **When** the SPDX 2.3 SBOM is emitted, **Then** peer edges carry `PROVIDED_DEPENDENCY_OF`.
2. **Given** the same scan invoked with `--spdx2-relationship-compat=basic`, **When** the SPDX 2.3 SBOM is emitted, **Then** peer edges carry `DEPENDS_ON` — byte-identical to pre-178 output for that edge's relationship type field.
3. **Given** an operator toggling between modes on the same fixture, **When** they compare the emitted SPDX 2.3 documents, **Then** the ONLY delta between the two is the relationship-type field on peer edges — every other byte (Package identity, annotations, non-peer edges, document envelope) is byte-identical.

---

### User Story 3 — Annotation retained in both modes for fine-grained targeting (Priority: P2)

An SPDX 2.3 consumer wants to answer "which specific peer-dep targets does this component declare?" — a finer-grained question than "which edges are peer-driven." The `mikebom:peer-edge-targets` annotation on the source Package enumerates the target PURL list. Post-178, this annotation MUST remain present in both compat modes so consumers can still access the fine-grained target list regardless of whether they get `PROVIDED_DEPENDENCY_OF` or `DEPENDS_ON` on the edge.

**Why this priority**: the annotation is the finer-grained supplement (Principle V's "carry information the standard doesn't natively express" carve-out). Removing it under `--spdx2-relationship-compat=full` would lose the target-list detail; keeping it means consumers get BOTH signals. Ranked P2 because the primary flow (US1) works without the annotation for consumers who only need the peer/functional distinction.

**Independent Test**: emit an npm-with-peer-deps fixture under both compat modes. Assert `mikebom:peer-edge-targets` annotation exists on the source Package in BOTH modes with the same value (JSON-encoded array of peer-target PURLs).

**Acceptance Scenarios**:

1. **Given** an SPDX 2.3 SBOM emitted under `--spdx2-relationship-compat=full`, **When** a consumer inspects the source Package's annotations, **Then** they find the `mikebom:peer-edge-targets` annotation with the JSON-encoded array of peer-target PURLs.
2. **Given** the same fixture emitted under `--spdx2-relationship-compat=basic`, **When** the consumer inspects the same Package's annotations, **Then** they find the same `mikebom:peer-edge-targets` annotation with the same value.
3. **Given** a consumer that walks BOTH the native relationship type AND the annotation, **When** they cross-check, **Then** every peer-target PURL named in the annotation corresponds to exactly one `PROVIDED_DEPENDENCY_OF` (or `DEPENDS_ON` in basic mode) edge originating from the source Package.

---

### Edge Cases

- **Optional peer deps** (npm `peerDependenciesMeta[<name>].optional = true`): **Decision (per Q1)**: emit `PROVIDED_DEPENDENCY_OF` uniformly for both mandatory and optional peers. The optional-vs-mandatory distinction is NOT projected onto the SPDX 2.3 relationship-type axis. Rationale: keeps the taxonomy simple (two relationship types only — `DEPENDS_ON` for regular deps + `PROVIDED_DEPENDENCY_OF` for all peer deps); the optional distinction can layer on later via a new annotation if consumer demand emerges. Deferred; out of m178 scope.
- **Workspace peers**: monorepo scans where a peer target is a workspace member (not a registry package). Same treatment as registry peers: the peer semantic is about "provided by the consumer," which applies whether the provider is a workspace peer or a registry install. **Decision**: `PROVIDED_DEPENDENCY_OF` fires uniformly for both workspace and registry peers.
- **Peer dep also declared as regular dep**: some npm packages declare a dep in BOTH `dependencies` AND `peerDependencies` (defensive pattern). mikebom's m147 emits a single edge per (source, target) pair; the semantic follows the reader's precedence. **Decision**: if the edge appears in `mikebom:peer-edge-targets` (m147's authoritative peer classifier), it becomes `PROVIDED_DEPENDENCY_OF` in full-mode. Consumers seeing the native type get the "provided" semantic; consumers walking the annotation see the target.
- **Peer dep unresolved / phantom edge**: m163 introduced logic to suppress peer edges whose target isn't present in the lockfile. Suppressed edges don't reach m178 — the classifier operates on already-emitted edges only.
- **`--offline` mode**: no interaction. This milestone is purely wire-shape transformation at emission time.
- **CDX 1.6 / SPDX 3.0.1 output**: neither format has a native peer construct. CDX 1.6 has no analog to `PROVIDED_DEPENDENCY_OF`; SPDX 3.0.1's `LifecycleScopeType` enum values are `build`/`design`/`development`/`runtime`/`test`/`other` — no `peer` value. Both formats' outputs are UNCHANGED by this milestone; the annotation continues to be the sole peer signal there.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Under the default `--spdx2-relationship-compat=full` mode (or when the flag is omitted, since `full` is the m228 default), the tool MUST emit peer-driven dep edges in SPDX 2.3 output with `relationshipType: "PROVIDED_DEPENDENCY_OF"`. Directionality follows the m228 convention for typed scoped relationships: reversed-direction. When package A declares B as a peer, the edge is `B PROVIDED_DEPENDENCY_OF A` (reads as "B is provided as a dependency for A" — matches the SPDX 2.3 spec's semantic).
- **FR-002**: Under `--spdx2-relationship-compat=basic`, the tool MUST emit peer-driven dep edges with `relationshipType: "DEPENDS_ON"` (natural direction, unchanged from pre-178) — byte-identical relationship-type value to pre-178 output on that edge. Preserves the m228 basic-vocabulary escape hatch.
- **FR-003**: The `mikebom:peer-edge-targets` annotation on the source Package MUST be present in both compat modes with byte-identical value. This annotation is the Principle V "carry information the standard doesn't natively express" carve-out — SPDX 2.3's native relationship type says "this edge is provided-peer" but not "which specific targets."
- **FR-004**: CDX 1.6 SBOM output MUST be byte-identical pre-178 vs post-178. CDX 1.6 has no native peer construct.
- **FR-005**: SPDX 3.0.1 SBOM output MUST be byte-identical pre-178 vs post-178. SPDX 3.0.1's `LifecycleScopeType` enum lacks a `peer` value.
- **FR-006**: SPDX 2.3 output for scans WITHOUT any npm peer edges (e.g., cargo-only, pip-only, non-peer-dep npm) MUST be byte-identical pre-178 vs post-178. No ripple beyond peer-edge relationship-type.
- **FR-007**: The predicate identifying "peer-driven edge" MUST match the m147 predicate that populates `mikebom:peer-edge-targets`. This is the invariant that ties the annotation and the relationship type together: every edge whose target appears in `mikebom:peer-edge-targets` on the source Package MUST carry `PROVIDED_DEPENDENCY_OF` (full-mode) OR `DEPENDS_ON` (basic-mode) — never the other way around, and conversely every `PROVIDED_DEPENDENCY_OF` edge MUST have its target in the source's annotation.
- **FR-008**: Documentation updates:
  - `docs/reference/reading-a-mikebom-sbom.md` — the existing `mikebom:peer-edge-targets` subsection MUST be updated to describe the new SPDX 2.3 primary-signal behavior + the compat-mode interaction.
  - `docs/reference/sbom-format-mapping.md` — the C-row for `mikebom:peer-edge-targets` MUST be updated: SPDX 2.3 column now cites `PROVIDED_DEPENDENCY_OF` as the primary native signal + the annotation as the finer-grained supplement.
- **FR-009**: The change MUST NOT introduce any new CLI flags. Reuses the m228 `--spdx2-relationship-compat=<full|basic>` flag exclusively.

### Key Entities

- **Peer-driven dep edge**: a `dependsOn`-shaped relationship whose source declared the target in `peerDependencies` (or `peerDependenciesMeta`) in the lockfile. Identified by the same predicate that populates `mikebom:peer-edge-targets` per m147. Semantic: "the target is provided by the consumer of the source, not installed by the source itself" — matches SPDX 2.3's `PROVIDED_DEPENDENCY_OF` definition.
- **SPDX 2.3 compat mode**: the value of `--spdx2-relationship-compat` (default `full`; alternative `basic`). Controls which relationship-type vocabulary the SPDX 2.3 emitter uses for scoped dep edges. m178 extends the flag's coverage to include peer-edge type-selection.
- **Fine-grained peer-target list**: the `mikebom:peer-edge-targets` annotation value (JSON-encoded array of peer-target PURLs). Preserved in both compat modes. The Principle V "finer-grained information the standard doesn't natively express" carve-out.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: SPDX 2.3 SBOMs emitted under `--spdx2-relationship-compat=full` (default) from an npm-with-peer-deps fixture contain ≥1 relationship with `relationshipType: "PROVIDED_DEPENDENCY_OF"`. Verified by integration test on a synthesized npm fixture.
- **SC-002**: SPDX 2.3 SBOMs emitted under `--spdx2-relationship-compat=basic` from the same fixture contain ZERO relationships with `relationshipType: "PROVIDED_DEPENDENCY_OF"`; all peer edges collapse to `DEPENDS_ON`. Verified by the same integration test with the flag toggled.
- **SC-003**: Cross-mode comparison — the SPDX 2.3 SBOMs emitted under `full` vs `basic` from the same fixture differ ONLY in the `relationshipType` field of peer-driven edges + potentially the reversed vs natural direction of those edges. Every other byte (Package identity, non-peer edges, document envelope, annotations) is byte-identical. Verified by structural diff.
- **SC-004**: The `mikebom:peer-edge-targets` annotation value on the source Package is byte-identical between the two compat modes on the same fixture. Verified by structural diff.
- **SC-005**: FR-007 invariant holds: every edge whose target appears in the annotation carries the mode-appropriate relationship type; every non-peer edge (from regular `dependencies`) carries `DEPENDS_ON` in BOTH modes. Verified by contract test cross-checking the annotation against the emitted relationships.
- **SC-006**: Existing golden regression fixtures for SPDX 2.3 that do NOT contain npm peer edges (e.g., cargo, gem, maven, pip, apk, deb, rpm) MUST stay byte-identical pre-178 vs post-178 (modulo the alpha.56→alpha.57 version bump if a release ships this milestone). Verified by the existing byte-identity SPDX regression suite.
- **SC-007**: Existing SPDX 2.3 golden regression fixtures for npm scans that contain peer edges show a bounded delta: ONLY the `relationshipType` values (and possibly reversed direction) on peer-driven relationships flip from `DEPENDS_ON` (pre-178) to `PROVIDED_DEPENDENCY_OF` (post-178, full mode). No other byte changes. Verified by post-regeneration diff review.
- **SC-008**: CDX 1.6 and SPDX 3.0.1 golden regression fixtures MUST stay byte-identical pre-178 vs post-178 (modulo the version bump). Verified by the CDX + SPDX 3 golden regression suites.

## Assumptions

- **`--spdx2-relationship-compat=full` is the default**: m228 established `full` as the default emission mode. Operators must explicitly opt into `basic` for the legacy escape hatch. m178 respects this default — the "improved native-first behavior" is what unconfigured operators get.
- **Constitution Principle V (v1.4.0) applies to closed-vocabulary relationship types**: Principle V says "if a native construct exists, mikebom MUST use the native construct as the primary signal." SPDX 2.3's `PROVIDED_DEPENDENCY_OF` is a native construct in the relationship-type closed vocabulary. Using it as the primary peer signal is the direct application of the principle. The compat-basic fallback exists because m228 already codified operator-facing escape hatch semantics; it doesn't dilute the Principle V compliance for consumers using the standard.
- **m147's peer-edge predicate is authoritative**: FR-007 pins the semantic to m147's existing classifier. Any edge that populates `mikebom:peer-edge-targets` MUST also carry `PROVIDED_DEPENDENCY_OF`. This composability contract avoids inventing a new peer-edge classifier.
- **SPDX 2.3 directional convention**: m228 established the convention for scoped typed relationships — e.g., `DEV_DEPENDENCY_OF` is emitted reversed-direction (`B DEV_DEPENDENCY_OF A` when A declares B as a dev dep). Post-178 peer edges follow the same convention: `B PROVIDED_DEPENDENCY_OF A` when A declares B as a peer. Matches the SPDX 2.3 spec's semantic ("SPDXRef-A depends on SPDXRef-B as a provided dependency" reads as "B is provided as a dep for A").
- **Golden regeneration is expected and bounded** — SC-007 gate ensures the ONLY delta on affected SPDX 2.3 goldens is the peer-edge relationship-type flip (and possibly direction reversal per the m228 convention). Consumers of the goldens receive an accurate signal upgrade — not a breaking change to unrelated fields.
