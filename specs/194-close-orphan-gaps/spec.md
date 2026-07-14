# Feature Specification: Close Remaining Graph-Completeness Orphan Gaps

**Feature Branch**: `194-close-orphan-gaps`
**Created**: 2026-07-14
**Status**: Draft
**Input**: User description: "let's work 571 and 572 first before doing alpha.62" — bundle the two follow-up issues filed after m192/m193 restored graph-completeness accuracy, so the pico corpus (kusari-cli / pico / guac / molcajete) flips from `partial` → `complete` in alpha.62 rather than in a later release.

## Clarifications

### Session 2026-07-14

- Q: How should US2 make nested npm workspace transitives reachable? → A: Option A — emit a `mikebom:component-role: main-module` component for each nested workspace root. Reuses m127 mainmod convention + m158 workspace-peer edges + m192/m193 pre-rewrite for `--root-name` interaction. Zero new plumbing; extends the multi-workspace pattern that already works at the top level.
- Q: How should `--root-name` interact with nested-workspace mainmods introduced by US2? → A: Option B — drop ALL manifest-derived mainmods (top-level + nested) when `--root-name` is active; rely on m192/m193 pre-rewrite to re-anchor every dropped mainmod's outgoing edges onto `target_ref`. Consistent with operator intent ("root is X, not the manifest-derived things") and avoids partial-drop edge cases where some nested mainmods survive but others don't.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Go stdlib is no longer an orphan (Priority: P1)

An operator scans any Go source repo (with or without `--root-name`). The emitted SBOM contains a `pkg:golang/stdlib@v<X>.<Y>.<Z>` component representing the Go standard library. Pre-m194 this component was always emitted as an orphan (no incoming DependsOn edges, no outgoing edges) — even in native-root scans, forcing `mikebom:graph-completeness: partial` with `orphaned-components-detected: 1+`. Post-m194 the stdlib component is reachable from the primary Go root via a synthetic direct DependsOn edge, so the classifier reports `complete` when no other orphans exist. Consumers using the `mikebom:graph-completeness` signal to gate SBOM quality no longer see false-negative `partial` on well-formed Go source scans. Filed as issue #571.

**Why this priority**: Confirmed across all 3 Go source-repo fixtures in the Kusari pico corpus (kusari-cli, pico, guac) — every one carries exactly 1 stdlib orphan and thus reports `partial` post-m192/m193. Fixing this closes the orphan gap for kusari-cli + guac in one shot and reduces pico's residual orphans from 63 → 62 (bulk of that remainder is US2's npm subgraph). The fix is small (single synthetic edge per Go primary root).

**Independent Test**: Scan a Go source repo with a valid `go.mod` where mikebom detects a Go main-module (native OR under `--root-name` with detected mainmod). Assert (a) exactly one `pkg:golang/stdlib@v*` component is emitted, (b) the emitted CDX `dependencies[].dependsOn` array for the Go primary root includes the stdlib PURL, (c) the `mikebom:graph-completeness` value is `complete` when no other orphans exist.

**Acceptance Scenarios**:

1. **Given** a Go source repo with only stdlib as a "would-be orphan" (all other components reachable from the root), **When** scanned, **Then** `mikebom:graph-completeness` is `complete` and no `graph-completeness-reason` annotation is emitted.
2. **Given** the same repo scanned with `--root-name X --root-version Y` (operator override), **When** scanned, **Then** the stdlib edge is re-anchored onto the operator's synthetic root via the m192/m193 pre-rewrite mechanism, and the value is `complete`.
3. **Given** a repo with NO Go components, **When** scanned, **Then** no `pkg:golang/stdlib` component is emitted and no synthetic edge is added (no-op on non-Go scans; byte-identity preserved).
4. **Given** a mixed Go + npm repo where npm still has orphans (US2 not yet fired), **When** scanned, **Then** the stdlib is reachable but the emission still reports `partial` correctly due to the npm orphans (US1 fix doesn't paper over unrelated gaps).

---

### User Story 2 - npm sub-workspace transitive components are reachable from a root (Priority: P1)

An operator scans a repository containing multiple `package.json` + `package-lock.json` pairs (a common shape: root `package.json` for the primary product + nested `pkg/*/package.json` files for internal tooling or sub-libraries). Pre-m194 mikebom's npm reader emits components from ALL discovered lockfiles but only threads dep-graph edges from the TOP-level workspace root — nested lockfiles' transitive components appear as orphans. The pico corpus scan of `pico@2c2f9719` demonstrates this: 57 npm components (`extract-pg-schema`'s transitive tree — `chalk`, `commander`, `debug`, etc.) are emitted but orphaned. Post-m194 each nested `package.json` + `package-lock.json` pair is treated as its own workspace root, so its transitive components are reachable via emitted DependsOn edges. Filed as issue #572.

**Why this priority**: Confirmed on pico (62 npm orphans out of 63 total) and molcajete (2 orphans). This is the biggest remaining orphan cluster in the pico corpus. Fixing this closes pico + molcajete in one shot. Ships alongside US1 (same P1) so alpha.62 delivers a complete-signal-on-well-formed-scans experience.

**Independent Test**: Scan a repo with a nested `package.json` + `package-lock.json` (e.g., a `pkg/foo/package.json` declaring `commander: "^10.0.0"` + `pkg/foo/package-lock.json` resolving it). Assert (a) the `commander` component is emitted, (b) the emitted CDX `dependencies[]` includes an edge from the nested workspace root (or the primary root) to `commander`, (c) the `mikebom:graph-completeness` value is `complete` when no other orphans exist.

**Acceptance Scenarios**:

1. **Given** a repo with a nested `pkg/tools/package.json` + `pkg/tools/package-lock.json` declaring a dep tree, **When** scanned, **Then** every dep-tree component is reachable via at least one DependsOn edge chain from a root.
2. **Given** the same repo scanned with `--root-name X`, **When** scanned, **Then** the nested workspace's mainmod (if emitted) either gets its edges re-anchored onto the operator's synthetic root (per m192/m193 pattern) OR the nested lockfile's edges are emitted with the nested workspace root as `.from` (which is reachable via workspace-peer plumbing).
3. **Given** a repo with ONLY a top-level `package.json` + `package-lock.json` (no nested sub-workspaces), **When** scanned, **Then** the emission is byte-identical to pre-m194 (fix is a no-op on the single-workspace path).
4. **Given** a nested lockfile whose transitive tree includes a component ALSO present in the top-level lockfile at a different version, **When** scanned, **Then** both versions are emitted as distinct components (each reachable from its respective workspace root) — no cross-workspace deduplication.

---

### Edge Cases

- **Go stdlib version differs across binaries in a multi-binary scan**: e.g., a container image with two Go binaries built against `v1.25.9` and `v1.26.0`. Post-m194 both stdlib components should be emitted (as they are today) AND each should be reachable via a synthetic edge from ITS OWN binary's implied Go root — not cross-linked. If both binaries share a single Go root component, both stdlibs are edges from that shared root.
- **Repo with no Go binaries but with `pkg:golang/stdlib` erroneously emitted** (theoretical — mikebom's readers shouldn't produce this): the synthetic edge fix MUST NOT fire for a `pkg:golang/stdlib` that has no corresponding Go root. Falls back to leaving it as an orphan; the classifier still fires `OrphanedComponentsDetected` for that specific component (real gap surfaces, not hidden).
- **Nested npm workspace claimed by a parent workspace's `"workspaces"` array**: existing m163 cross-workspace resolution SHOULD already cover this case — the parent's lockfile references the nested workspace's contents. Post-m194 fix targets the OTHER case: nested manifest + nested lockfile that ISN'T claimed by any parent (fully independent sub-project). Verify at plan time that m163 covers the claimed case.
- **Nested lockfile without a sibling `package.json`** (rare — orphan lockfile): the fix emits a synthetic `pkg:npm/<workspace-name>` root using the directory name as workspace-name, and edges from that root to the lockfile's contents. Fallback shape is spec-clean per m191's versionless-PURL convention.
- **Ecosystem breadth**: this fix is scoped to Go stdlib (US1) and npm nested workspaces (US2). Other ecosystems (pip nested venvs, cargo workspace crates that aren't top-level, gem nested Gemfiles) may have analogous gaps — those are OUT OF SCOPE for m194 and filed separately if they surface.
- **Real Yocto `core-image-minimal` scan** (m190-style ipk scan): NOT in scope — ipk scans don't use `pkg:golang/stdlib` and don't have nested npm workspaces.
- **Byte-identity of goldens**: any golden that was NOT exhibiting a stdlib orphan OR nested-npm-orphan pre-m194 MUST pass byte-identically. Verify at Phase 2 audit — likely most goldens are unaffected because the mikebom in-repo fixture corpus doesn't include Go binary + Go stdlib emission OR nested npm workspaces at scale.
- **Cross-format consistency**: the fix affects the pre-emission Relationship set; all three format emitters (CDX 1.6, SPDX 2.3, SPDX 3) consume it identically.

## Requirements *(mandatory)*

### Functional Requirements

**Go stdlib synthetic edge (US1 — #571)**:

- **FR-001**: When mikebom emits a `pkg:golang/stdlib@v*` component AND at least one Go component with `mikebom:component-role: main-module` OR the operator's target_ref is a Go PURL, System MUST emit a synthetic `DependsOn` Relationship with `.from = <Go primary root>` and `.to = <stdlib PURL>`. The synthetic edge SHOULD be recorded with an EnrichmentProvenance identifying it as an m194 synthesis (e.g., `source: "m194-stdlib-synthesis"`, `data_type: "implicit-stdlib-edge"`).
- **FR-002**: When mikebom emits `pkg:golang/stdlib@v*` but has NO Go main-module component AND no Go PURL target_ref (edge case — should not occur in practice), System MUST NOT emit a synthetic edge; the stdlib remains as-is and the classifier's `OrphanedComponentsDetected` fires correctly for it.
- **FR-003**: When multiple Go main-module components exist (e.g., a container image with multiple Go binaries), System MUST emit ONE synthetic edge per Go main-module → matching-version stdlib pair. If only one stdlib is emitted, all Go mainmods link to it; if multiple stdlibs are emitted (per Go binary), each mainmod links to its own-binary's stdlib version.
- **FR-004**: The synthetic edge MUST appear in the emitted `dependencies[]` (CDX 1.6), `relationships[]` DEPENDS_ON entries (SPDX 2.3), and `Relationship` graph elements with `relationshipType: dependsOn` (SPDX 3). Cross-format consistency via the existing `mikebom_common::resolution::Relationship` plumbing.
- **FR-005**: The synthetic edge MUST be present BEFORE `compute_graph_completeness` runs so BFS reachability includes stdlib. Placement matters: same insertion point as m192/m193's dropped-mainmod pre-rewrite in `builder.rs` (before the classifier call).

**npm sub-workspace edge emission (US2 — #572)**:

- **FR-006**: When mikebom's npm reader discovers a nested `package.json` + `package-lock.json` pair NOT claimed by any parent workspace's `"workspaces"` array, System MUST treat the nested pair as its own workspace root: (a) emit a mainmod component for the nested workspace tagged with `mikebom:component-role: main-module` (per Q1 answer A, reusing the m127 convention), (b) emit DependsOn edges from that mainmod to every component the nested lockfile resolves — using the same edge-emission code path the top-level npm reader already uses for its top-level lockfile.
- **FR-007**: The nested workspace's mainmod component MUST carry `mikebom:component-role: main-module` (matching m127's existing convention), so the graph-completeness classifier picks it up as a per-ecosystem root via `select_root` / `pick_ecosystem_top`.
- **FR-008**: When multiple nested workspaces are discovered, System MUST emit a mainmod component per nested workspace + edges from each mainmod to ITS OWN lockfile's contents. Existing m163 workspace-peer linkage handles the multi-mainmod case at emit-time.
- **FR-009**: When `--root-name X` operator override is active AND nested workspaces are present, `apply_main_module_drop_or_demote` MUST drop ALL manifest-derived mainmods (top-level AND nested per Q2 answer B). The m192/m193 pre-rewrite mechanism MUST then re-anchor EACH dropped mainmod's outgoing edges onto the operator's `target_ref`. Verify at plan time that `apply_main_module_drop_or_demote` already handles multi-mainmod drops correctly (per m149 plumbing); if it only drops the top-level, extend it to drop nested mainmods as well. Zero partial-drop edge cases permitted — either all manifest-derived mainmods drop or none do.
- **FR-010**: When a component IS reachable from a nested workspace root but ALSO exists (at a different version) in the top-level workspace, both versions MUST remain as distinct components. No cross-workspace deduplication.

**Cross-cutting parity + regression control**:

- **FR-011**: Cross-format consistency (FR-004) — the synthetic edges appear identically in CDX / SPDX 2.3 / SPDX 3 via the shared `Relationship` plumbing.
- **FR-012**: Every existing golden fixture that did NOT exhibit either a stdlib orphan OR a nested-npm-orphan pre-m194 MUST pass byte-identically. Verified via Phase 2 drift audit.
- **FR-013**: Goldens that DID exhibit a stdlib orphan (any Go source golden) MAY be updated as a documented drift class — expected diff: one added edge from Go root → stdlib in the CDX `dependencies[]` array.
- **FR-014**: The fix MUST NOT introduce a new `mikebom:*` annotation — all new edges use the existing `Relationship` structure with `RelationshipType::DependsOn` per CLAUDE.md Principle V (standards-native fields).
- **FR-015**: One INFO-level `tracing` log line per US MUST report the number of synthetic edges emitted (stdlib edges for US1; nested-workspace mainmods + edges for US2). Matches m192's observability convention.
- **FR-016**: Real orphans STILL surface — the fix closes 2 SPECIFIC orphan classes; any other orphan class (unresolved Go transitives, file-tier components, image-tier system packages) continues to fire `OrphanedComponentsDetected` correctly.

### Key Entities *(include if feature involves data)*

- **Go primary root component**: an existing `ResolvedComponent` with `mikebom:component-role: main-module` AND `purl.ecosystem() == "golang"`. Post-m194 gains one outgoing synthetic edge to the matching-version stdlib per FR-001. Existing type; no schema change.
- **Go stdlib component**: an existing `ResolvedComponent` with `purl` matching `pkg:golang/stdlib@v*`. Post-m194 becomes reachable via the synthetic edge instead of appearing as an orphan.
- **Nested npm workspace root**: an npm mainmod component discovered in a nested directory. Existing shape (per m127/m149/m163 conventions) — no schema change. Post-m194 gets emitted with proper `mikebom:component-role: main-module` even when nested + carries workspace-peer edges via the existing m158 plumbing.
- **Synthetic stdlib edge (m194)**: a `Relationship` with `.from = <Go root PURL>`, `.to = <stdlib PURL>`, `relationship_type: DependsOn`, `provenance: EnrichmentProvenance { source: "m194-stdlib-synthesis", data_type: "implicit-stdlib-edge" }`. Ephemeral to emit time; not persisted.
- **Synthetic nested-npm-mainmod edges (m194)**: a set of `Relationship`s emitted per nested workspace, mirroring how the top-level npm reader emits edges from the top-level mainmod. Uses the same `EnrichmentProvenance { source: "<lockfile path>", data_type: "npm-workspace-lockfile" }` shape as existing npm relationships.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Post-m194, the Kusari pico corpus's `kusari-cli` fixture reports `mikebom:graph-completeness: complete` (currently `partial` with 7 orphans; 1 is stdlib, other 6 are same-ecosystem gaps that MAY also be closed by US1's Go transitive coverage).
- **SC-002**: Post-m194, the Kusari pico corpus's `guac` fixture reports `complete` (currently `partial` with 4 orphans, mostly stdlib + 3 similar).
- **SC-003**: Post-m194, the Kusari pico corpus's `pico` fixture reports `complete` (currently `partial` with 63 orphans; 1 stdlib closed by US1, 62 npm closed by US2).
- **SC-004**: Post-m194, the Kusari pico corpus's `molcajete` fixture reports `complete` (currently `partial` with 2 orphans, both npm).
- **SC-005**: Post-m194, ALL 6 pico corpus fixtures (4 source repos + 2 image scans) report `complete`. This is the observable customer-contract outcome — Kusari's downstream tooling consuming the `mikebom:graph-completeness` signal correctly identifies every fixture as well-formed.
- **SC-006**: Byte-identity — every existing mikebom in-repo golden that did NOT exhibit a stdlib orphan or nested-npm orphan MUST produce byte-identical CDX / SPDX 2.3 / SPDX 3 emission post-m194. Measured by running the workspace regression suite: zero drift on any golden that doesn't touch the fix's code paths.
- **SC-007**: Real orphans still surface — a fixture with a genuine orphan (an unresolved Go transitive, a file-tier component, a design-tier component with no source-tier resolution) STILL reports `partial` with `OrphanedComponentsDetected`. The fix does NOT hide real gaps.

## Assumptions

- The customer's downstream contract (per pico's `regenerate.sh` shape) treats `mikebom:graph-completeness` as a binary "well-formed/incomplete" signal. Reaching `complete` on all pico corpus fixtures matches the pre-regression consumer expectation.
- mikebom's Go reader already emits `pkg:golang/stdlib@v*` components correctly (as an in-repo scan artifact). Only the edge is missing — verify at plan-time.
- mikebom's npm reader already discovers nested `package.json` + `package-lock.json` pairs (m106/m163 established this). Verify: does it emit components for the nested lockfile's contents? If yes, only the edges need adding. If no, the fix has larger scope.
- No new Cargo dependencies are required. The fix reuses `mikebom_common::resolution::Relationship`, `Purl`, `serde_json`, and `tracing`. Consistent with every recent milestone.
- No new `mikebom:*` annotations — the synthetic edges are just `Relationship` records with `RelationshipType::DependsOn`.
- The Kusari pico corpus SHAs are pinned (per `regenerate.sh`: kusari-cli @ c12f150, pico @ 2c2f9719, guac @ ebb808e, molcajete @ 0a40304). Post-m194 SBOM output for these SHAs should match the SC-005 expectation.
- The two user stories (US1 + US2) are BOTH P1 in the spec because both are prerequisites for the pico corpus to hit `complete` (kusari-cli/guac need only US1; pico needs both; molcajete needs only US2). Bundling them saves an alpha release cycle.
- Post-m194 the `mikebom:graph-completeness` signal is meaningfully strengthened: `complete` fires for well-formed source-repo scans, `partial` fires only for real gaps. Consumer contract stability restored.
- The fix is orthogonal to m192/m193 — those milestones fixed the operator-override BFS bug + pre-rewrite ordering; m194 closes the specific orphan classes that remained visible AFTER those fixes.
- If US1 or US2 uncovers a deeper reader-level issue (Go stdlib emission itself has bugs, npm nested-workspace discovery is broken), that becomes a scope expansion decision at plan-time. The initial spec assumes the components ARE being emitted correctly and only the edges are missing.
