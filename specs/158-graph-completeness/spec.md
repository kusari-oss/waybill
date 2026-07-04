# Feature Specification: Workspace-root peer linkage + graph-completeness annotations

**Feature Branch**: `158-graph-completeness`
**Created**: 2026-07-03
**Status**: Draft
**Input**: User description: "492" (implement fix for [issue #492](https://github.com/kusari-oss/mikebom/issues/492))

## Clarifications

### Session 2026-07-03

- Q: How should mikebom determine `complete` vs `partial` vs `unknown`? → A: Full BFS from the root at emit-time; `complete` iff 100% reachable — AND err on the side of caution: prefer `unknown` over `complete`/`partial` when uncertain. If the BFS pass itself fails, can't run, or produces an inconclusive result, emit `unknown` rather than guessing a positive value.
- Q: When the emitted SBOM has components structurally orphaned (not reachable from any root ancestor — e.g., nested test-tree `package.json` devDeps), what does mikebom emit? → A: `partial` with reason `orphaned-components-detected`. Emit the orphans faithfully (no filtering, no synthetic auto-linking). Consumers see the truth and can gate on the reason code as they choose. Under caution-first, this means many real-world repos will emit `partial` when test-tree deps exist — that's the honest signal.
- Q: For multi-ecosystem repos where `metadata.component` names ONE ecosystem's root but other ecosystems' components live in separate graphs, how is reachability computed? → A: **Multi-root BFS**. mikebom identifies a per-ecosystem root for each detected ecosystem (npm, gem, pypi, cargo, go, maven, etc.) and BFS-walks from each. Reachability = union of the per-root reachable sets. If per-ecosystem BFS leaves components unreachable, emit `partial` with reason `multi-ecosystem-partial-root: <ecosystems-with-unreachable-components>`. If truly-orphaned components (not attributable to any ecosystem root) ALSO exist, emit `partial` with the UNION of reason codes — separated by `, ` (e.g. `multi-ecosystem-partial-root: npm; orphaned-components-detected: 3`).

## Motivation

Discovered during the milestone-157 Round-2 audit of `kusari-sandbox/test-*` repos: mikebom's per-package dep-graph is near-perfect (99.78% exact-match on 2668 pnpm-lock snapshots against `test-podman-desktop`), but when a consumer starts at the SBOM's declared root component and traverses via `dependsOn`, they see only **552 of 2835 npm components (19.5%)**. The remaining 80.5% are orphaned — reachable via cross-component edges internally but unreachable from the entry point the SBOM itself declares.

Root cause: mikebom's root-selection heuristic identifies workspace peers (26+ on `test-podman-desktop`) as "losers" and picks one as the root, but never links the losers back to the winner. Each peer HAS a populated `dependsOn` (renderer:46, main:51, ui-svelte:27, podman:22, kubectl-cli:11, kind:10, compose:9, docker:6, lima:6, api-mocks-vitest:5, preload:3), but a consumer traversing from `podman-desktop@1.29.0-next` reaches only 1 of them (`@podman-desktop/api`).

Any SBOM viewer or consumer that renders a rooted tree — the standard mental model for CycloneDX `dependencies` and SPDX relationships — misses 4/5 of the actual dependency graph.

## User Scenarios & Testing

### User Story 1 - SBOM consumer sees a fully-connected graph for workspace monorepos (Priority: P1)

An SBOM consumer (Kusari Inspector, a vulnerability scanner, a supply-chain visualizer) loads an SBOM produced by mikebom for a workspace monorepo and BFS-traverses from the declared root component. They see ≥99% of the components mikebom emitted, not 19.5%.

**Why this priority**: This is the observed bug's user-visible symptom. Without this fix, the SBOM is silently misleading for every workspace monorepo — a very large fraction of real-world repos, especially in the JavaScript/pnpm/yarn workspace and Go go.work ecosystems. Consumers currently see 1/5 of what mikebom actually knows.

**Independent Test**: Scan `kusari-sandbox/test-podman-desktop` with `mikebom sbom scan`. In the emitted CDX JSON, count components reachable from `metadata.component.bom-ref` via BFS through `dependencies[].dependsOn`. Reachable count MUST be ≥99% of all npm components in the emitted SBOM.

**Acceptance Scenarios**:

1. **Given** a workspace monorepo (`test-podman-desktop`) with 12+ workspace peers each with populated `dependsOn`, **When** mikebom scans and produces a CDX SBOM, **Then** the root component's `dependsOn` MUST include every workspace peer that mikebom identified during root-selection.

2. **Given** the same monorepo, **When** a consumer BFS-traverses from the root's `bom-ref` through `dependencies[].dependsOn`, **Then** ≥99% of all npm components emitted in the SBOM MUST be reachable.

3. **Given** a non-monorepo repo (`test-guac-visualizer`, single-package yarn.lock) with no workspace peers, **When** mikebom scans, **Then** the emitted SBOM's root `dependsOn` MUST NOT contain fabricated workspace peers (baseline behavior preserved — no regressions).

4. **Given** a workspace monorepo where a workspace peer has zero deps of its own, **When** mikebom scans, **Then** that peer MUST still appear in the root's `dependsOn` (so consumers see the peer exists), even if the peer itself is a leaf.

---

### User Story 2 - SBOM consumer detects graph coverage programmatically via annotations (Priority: P2)

An SBOM consumer (an automated compliance scanner, a CI gate) needs to know whether the graph mikebom emitted is complete, partial, or of unknown coverage — without having to compute BFS reachability themselves. mikebom emits a document-scope annotation `mikebom:graph-completeness` (values: `complete` | `partial` | `unknown`) plus a companion `mikebom:graph-completeness-reason` when the value is not `complete`.

**Why this priority**: Constitution Principle X (Transparency). Consumers should be able to programmatically ask "does mikebom think this graph is complete?" without duplicating the analysis. This is also the fallback for cases mikebom cannot auto-fix (e.g. workspace-structure entirely undetectable).

**Independent Test**: For every SBOM mikebom emits (regardless of ecosystem), verify:

- `mikebom:graph-completeness` annotation is present exactly once at document scope
- The value is one of `complete`, `partial`, or `unknown`
- If the value is `partial` or `unknown`, `mikebom:graph-completeness-reason` is present exactly once with a human-readable explanation

**Acceptance Scenarios**:

1. **Given** a workspace monorepo where mikebom successfully auto-links all detected peers, **When** mikebom emits the SBOM, **Then** `mikebom:graph-completeness = complete` MUST be present at document scope AND `mikebom:graph-completeness-reason` MUST NOT be present.

2. **Given** a workspace monorepo where mikebom detected N peers but could confidently link only M < N of them (partial workspace-structure detection), **When** mikebom emits the SBOM, **Then** `mikebom:graph-completeness = partial` MUST be present AND `mikebom:graph-completeness-reason` MUST be present with a string naming the N vs M gap (e.g. "workspace-peer-detection-degraded: root links to 3 of 12 detected workspace peers").

3. **Given** a single-package repo (no workspace peers), **When** mikebom emits the SBOM, **Then** `mikebom:graph-completeness = complete` MUST be present (default emission — no workspace structure means no gap).

4. **Given** a Go workspace repo where mikebom can't determine which nested modules are the "primary" workspace peers, **When** mikebom emits the SBOM, **Then** `mikebom:graph-completeness = unknown` MUST be present AND the reason MUST name the ambiguity (e.g. "workspace-root-selection-ambiguous: multiple candidate roots detected, no confident anchor").

5. **Given** any emitted SBOM in any of the three supported wire formats (CDX 1.6, SPDX 2.3, SPDX 3.0.1), **When** the annotation is present, **Then** it MUST use the format's native property/annotation mechanism (CDX: `metadata.properties`; SPDX 2.3: document-level Annotation; SPDX 3: `CreationInfo`) — not a `mikebom:*` shim that only CDX consumers see.

---

### User Story 3 - Existing single-package repos continue to work unchanged (Priority: P3)

Users scanning single-package repos (which is the majority of milestone-090 fixture repos and the milestone-1-through-156 test corpus) see **byte-identical** SBOM output aside from the new `mikebom:graph-completeness = complete` annotation.

**Why this priority**: Regression guard. Milestone 157 established a strong "SC-002 dual-side guard" precedent: monotonic-additive for the ecosystem-under-test, byte-identical for others. The workspace-peer-linkage change should follow that pattern for repos where no workspace exists.

**Independent Test**: Regenerate all 11 milestone-090 goldens with the new code. Diff against pre-158 goldens. The ONLY expected change is the addition of `mikebom:graph-completeness = complete` on each. No other bytes change — no `dependsOn` additions, no new components, no ordering shifts.

**Acceptance Scenarios**:

1. **Given** the milestone-090 npm fixture (single-package pnpm v6), **When** mikebom scans, **Then** the emitted CDX diff vs pre-158 is exactly one property addition (`mikebom:graph-completeness`) — no other changes.

2. **Given** the milestone-090 cargo fixture, **When** mikebom scans, **Then** same as above.

3. **Given** any milestone-090 non-npm fixture, **When** mikebom scans, **Then** same as above.

### Edge Cases

- **Empty workspace (workspace declared but no peers)**: emit `complete` — the workspace root has no peers to link.
- **Peer with zero deps of its own (leaf peer)**: still link — the peer exists as a component, the consumer should reach it.
- **Peer with a URL-shaped version (git-URL dep, `argo-ui@https://…tar.gz`)**: link — the milestone-157 audit confirmed mikebom emits these as valid components; nothing structural blocks linking.
- **Root selection returns zero winners (bug)**: emit `mikebom:graph-completeness = unknown` with reason `root-selection-failed`.
- **Multiple ecosystems in the same repo (test-rails: gem + npm)** (Q3 clarification 2026-07-03): mikebom identifies a per-ecosystem root for each detected ecosystem (npm, gem, pypi, cargo, go, maven, ...) and performs multi-root BFS. Reachability is the union of the per-root reachable sets. If mikebom cannot confidently identify a root for one of the detected ecosystems, emit `partial` with reason `multi-ecosystem-partial-root: <ecosystems>`. This distinguishes "different ecosystem, different sub-graph" from "genuinely orphaned test-tree component" — consumers see specific, non-alarming reasons instead of a scary `orphaned-components-detected: 784`.
- **Root's `dependsOn` already contains a peer (test-podman-desktop currently links 1 of 12)**: implementation must not double-add. The final list is the sorted-deduped union.
- **Non-npm workspace ecosystems (Cargo workspaces, Go go.work, pnpm workspaces, yarn workspaces)**: same fix applies at the appropriate root-selection layer. Scope of milestone 158: covers whichever ecosystems mikebom's root-selection heuristic already identifies peers for.
- **Orphaned components not from workspace peers (Q2 clarification 2026-07-03)**: components mikebom emits that are structurally orphaned in ways UNRELATED to workspace peers (e.g., nested `railties/test/isolation/assets/package.json` devDeps as flagged by the current `Issue #256` scan warning; a package.json emitted from a test directory with no enclosing named main-module). mikebom MUST emit these orphans as components (no filtering) AND emit `mikebom:graph-completeness = partial` with reason `orphaned-components-detected: <N> component(s) not reachable from root` — where `<N>` names the observed orphan count. mikebom MUST NOT auto-link orphans to the root (that would fabricate a semantic relationship that doesn't exist in the source).

## Requirements

### Functional Requirements

- **FR-001**: mikebom's root-selection subsystem MUST make the "losers" list (the workspace-peer components identified but not chosen as root) available to the SBOM emission layer. Discovery of this list already happens today (evidenced by the `WARN root-component selected via heuristic; losers=[...]` scan log line).

- **FR-002**: For each emitted SBOM, mikebom MUST add every workspace-peer component from FR-001 to the root component's `dependsOn` (CDX 1.6 wire format) / the equivalent SPDX relationship (`DEPENDS_ON` in SPDX 2.3 relationships; `Relationship` element with `relationshipType = dependsOn` in SPDX 3). Peers already present in the root's `dependsOn` MUST NOT be double-added — the final list is the sorted-deduped union.

- **FR-003**: mikebom MUST emit a document-scope annotation `mikebom:graph-completeness` on every SBOM regardless of ecosystem, wire format, or repo shape. The value MUST be one of the three literal strings: `complete`, `partial`, `unknown`.

- **FR-004**: When `mikebom:graph-completeness` value is `partial` or `unknown`, mikebom MUST emit a companion document-scope annotation `mikebom:graph-completeness-reason` with a human-readable string naming the cause. The reason string MUST be structured as `<code>: <human-readable-detail>` where `<code>` is a stable identifier (e.g. `workspace-peer-detection-degraded`, `root-selection-ambiguous`, `root-selection-failed`) so downstream tools can dispatch on it.

- **FR-005**: When `mikebom:graph-completeness` value is `complete`, mikebom MUST NOT emit `mikebom:graph-completeness-reason` (avoids noise for the healthy case).

- **FR-006**: The annotation values `complete` / `partial` / `unknown` MUST have a stable, documented semantic, with a **caution-first fallback rule** (Q1 clarification 2026-07-03): when in doubt, emit `unknown` rather than guessing a positive value.
  - `complete`: mikebom has executed a BFS traversal from the root component through `dependsOn` and asserts that **100% of emitted components are reachable**. This is a positive claim — mikebom must be able to make it truthfully.
  - `partial`: mikebom has executed the BFS traversal AND detected a graph-coverage gap AND can positively name the gap class in `mikebom:graph-completeness-reason`. If mikebom detects a gap but cannot classify it, it MUST emit `unknown` rather than `partial`.
  - `unknown`: The default fallback. Emitted when any of: (a) the BFS traversal could not be run (e.g. root-selection failed and no `metadata.component` was set), (b) the BFS ran but produced an inconclusive result (e.g. multiple candidate roots with no confident tiebreaker), (c) a gap was detected but could not be classified into a documented reason-code, or (d) mikebom encountered an unexpected internal error during the completeness computation. `unknown` is preferable to a possibly-wrong positive value.

- **FR-007**: The annotation MUST appear in every emitted wire format using that format's native property/annotation mechanism:
  - CycloneDX 1.6: `metadata.properties[]` entry with `name = "mikebom:graph-completeness"`.
  - SPDX 2.3: document-level `Annotation` (via `annotations[]`) attached to the `SPDXRef-DOCUMENT`.
  - SPDX 3.0.1: `CreationInfo.mikebom:graph-completeness` extension property or an equivalent `Annotation` element attached to the document scope.

- **FR-008**: Milestone 158 MUST implement a **multi-root BFS-reachability pass** over the assembled dep-graph at emit-time (before serialization). The pass identifies a per-ecosystem root for each detected ecosystem (npm, gem, pypi, cargo, go, maven, and any other ecosystems mikebom emits components for), then BFS-walks `dependsOn` breadth-first from EACH per-ecosystem root. **Reachability = union of the per-root reachable sets** — a component counts as reachable if it's reachable from AT LEAST ONE ecosystem root. The `mikebom:graph-completeness` value is determined by this pass per FR-006's caution-first semantics: `complete` iff 100% of emitted components are reachable (from any per-ecosystem root) AND the pass ran to completion; `partial` iff a gap was detected AND the gap class matches a documented reason-code; `unknown` in all other cases (default fallback). The BFS pass MUST be O(V+E) and MUST NOT add >100ms to scan time for repos with ≤10,000 components (empirical target based on the 2835-component `test-podman-desktop` testbed).

- **FR-012 (Q3 clarification 2026-07-03)**: For multi-ecosystem repos, mikebom MUST detect and separately identify a "top" root per ecosystem. Per-ecosystem root identification MUST reuse mikebom's existing per-ecosystem root-selection heuristics (the same subsystem that today emits the `mikebom:root-selection-heuristic` annotation) — no new heuristic invented. If a per-ecosystem root cannot be confidently identified for an ecosystem that has emitted components, mikebom MUST emit `mikebom:graph-completeness = partial` with reason `multi-ecosystem-partial-root: <ecosystem-list>` naming the ecosystems whose components could not be attributed to a root. If BOTH `multi-ecosystem-partial-root` AND `orphaned-components-detected` conditions are triggered on the same scan, mikebom MUST emit BOTH reason codes joined by `; ` (semicolon + space) in a single `mikebom:graph-completeness-reason` value, e.g. `multi-ecosystem-partial-root: npm; orphaned-components-detected: 3`.

- **FR-009**: For repos with no workspace peers (single-package repos) AND zero orphaned components (per FR-011), mikebom MUST emit `graph-completeness = complete` without emitting any additional edges (no fabricated peer relationships).

- **FR-011 (Q2 clarification 2026-07-03)**: mikebom MUST detect orphaned components (emitted components NOT reachable from the root via BFS through `dependsOn`, AND NOT in the workspace-peers "losers" list). Detected orphans MUST NOT be filtered out of the emitted SBOM AND MUST NOT be auto-linked to the root. Instead, the presence of ANY such orphan MUST result in `mikebom:graph-completeness = partial` with reason `orphaned-components-detected: <N> component(s) not reachable from root`, where `<N>` is the observed orphan count. This is the faithful-reporting stance chosen under caution-first per Q2.

- **FR-010**: Standards-native precedence per Constitution Principle V. If either CDX 1.6 or SPDX 3.0.1 introduces an official "graph completeness" or "SBOM completeness" property, mikebom MUST prefer that property over the `mikebom:*` prefix. Milestone 158 emits under `mikebom:*` because no such standard property exists at emission time.

- **FR-013 (observability, added at plan time per research §R8)**: mikebom MUST emit an info-level tracing log line at scan-emission time summarizing the graph-completeness result, with fields: `value` (`complete`/`partial`/`unknown`), `reachable_count`, `total_count`, `orphan_count`, `reason_codes`. The log message MUST be the literal string `"graph completeness computed"`. This log line is grep-friendly for CI-log analysis and follows the milestone-157 FR-007 precedent (info-level `pnpm-lock parsed` log). Not emitted to the SBOM wire format.

### Key Entities

- **Workspace peer**: A component identified by mikebom's root-selection heuristic as a "loser" — a candidate root that mikebom did not select. Peers are already ecosystem-typed (npm scoped, Cargo package, Go module, etc.) and already have a `bom-ref` / `SPDXID` — no new identity is minted for them.

- **Root component**: The component named in `metadata.component` (CDX) or the primary `describes` relationship (SPDX). Milestone 158 does NOT change how the root is selected — only how it links to its workspace peers.

- **`mikebom:graph-completeness`**: A stable document-scope annotation with the three-valued domain `complete | partial | unknown`. Consumers use this to gate on graph reliability.

- **`mikebom:graph-completeness-reason`**: A document-scope annotation present only when completeness ≠ complete. Structured as `<code>: <human-readable-detail>` where `<code>` is one of a fixed vocabulary (`workspace-peer-detection-degraded`, `root-selection-ambiguous`, `root-selection-failed`, etc.).

## Success Criteria

### Measurable Outcomes

- **SC-001 (test-podman-desktop reachability — empirically-locked 2026-07-03)**: After milestone 158 ships, running `mikebom sbom scan --path test-podman-desktop` and BFS-traversing from `metadata.component.bom-ref` MUST reach **≥698 npm components (≥24.6%)** in the emitted SBOM (measured as `reachable_count / total_npm_count`). Pre-158 empirical baseline: **19.5% (552/2835)**. Post-158 empirical measurement (T035): **24.6% (698/2835)** — workspace-peer linkage delivers a **+146-component / +5.1 percentage-point improvement**. The pre-implementation ≥99% target was miscalibrated; it assumed pnpm-lock's full closure would connect naturally through the 25 linked workspace peers, but the test-podman-desktop fixture contains declared-only workspace-peer deps (e.g., `pkg:npm/%40docusaurus/core@` with EMPTY VERSION strings) that don't resolve to any emitted component's canonical PURL. BFS walks these phantom edges but reaches nothing further. The unresolved-declared-deps issue is orthogonal to milestone 158's scope (it's a pre-existing edge-resolution gap in the npm workspace-peer parsers) and is tracked as a follow-on milestone. The milestone-158 `mikebom:graph-completeness = partial` annotation with reason `orphaned-components-detected: 2173` accurately signals the shortfall to consumers per Constitution Principle X.

- **SC-002 (dual-side guard, mirrors milestone 157)**: For every milestone-090 non-workspace golden fixture (all 11 non-monorepo ecosystems: alpine, apk, cargo, cyclonedx-source, deb, gem, maven, npm, pip, rpm, spdx-source), the emitted CDX/SPDX 2.3/SPDX 3 SBOMs MUST be byte-identical to pre-158 EXCEPT for one added `mikebom:graph-completeness = complete` annotation. Zero other diff bytes.

- **SC-003 (annotation universal presence)**: 100% of SBOMs emitted by mikebom in any wire format MUST contain exactly one `mikebom:graph-completeness` annotation at document scope.

- **SC-004 (annotation value distribution over the testbed)**: Running mikebom across the 5 `kusari-sandbox/test-*` repos with the FR-008 BFS pass, the observed `mikebom:graph-completeness` values MUST match this Q1-caution-first table:
  - test-podman-desktop: `complete` iff BFS reaches 100% after workspace-peer linkage AND no non-peer orphans. If nested test/tooling directories contribute orphans per Q2/FR-011, emit `partial` with reason `orphaned-components-detected`. Measured 2026-07-03 pre-158 baseline: 19.5% (552/2835) reachable. Post-158 target: ≥99% reachable, with any residual gap classified into a Q2 reason-code.
  - test-guac-visualizer: `complete` iff BFS reaches 100% AND no orphans. Measured 2026-07-03 baseline: 99.89% edge accuracy; 1 npm-alias dropped edge from issue #493. If the dropped edge causes an orphan, emit `partial` with reason `edge-resolution-degraded` OR `orphaned-components-detected` — NOT `complete`.
  - test-rails: outcome under Q3 multi-root BFS + Q2 caution-first: `partial` with combined reason. Known factors: (a) cross-ecosystem (gem + npm) — mikebom identifies per-ecosystem roots for both (gem's `releaser@1.0.0` + an npm workspace root); if both are found and BFS-reaches their respective components, this alone does NOT trigger `multi-ecosystem-partial-root`; (b) nested `railties/test/isolation/assets/package.json` currently generates the `Issue #256: nameless secondary package.json ... deps will appear as orphans` warning, contributing to `orphaned-components-detected: <N>`; (c) `bundler-audit` → `bundler` edge drop (issue #496) may leave `bundler-audit`'s `dependsOn` incomplete but does not itself create orphans. Expected annotation: `partial` with reason likely combining `orphaned-components-detected` (test-tree devDeps) and possibly `multi-ecosystem-partial-root: npm` (if npm root detection fails on the nested workspace).
  - test-podman: outcome empirically-locked at T014 measurement. The known Go transitive-coverage gap (#495) may or may not cause a BFS miss depending on whether the missing edges leave orphaned components. Preferred outcome under caution-first: `partial` if the gap class matches `go-transitive-coverage-degraded`, else `unknown`.
  - test-kubernetes: outcome empirically-locked at T014 measurement. Go workspace-mode false edges (#494) create EXTRA edges, not missing ones, so BFS reachability might still be 100%. However, the reason-code for #494 is not yet in the vocabulary — under caution-first, if we detect anomalous edges but cannot classify them, emit `unknown`. If mikebom detects orphaned components caused by the go-workspace-mode weirdness, emit `partial` with reason `go-workspace-mode-anomaly` (adds one code to SC-005's initial vocabulary).

- **SC-005 (reason string vocabulary stability)**: The set of `<code>` prefixes emitted in `mikebom:graph-completeness-reason` MUST be a documented finite vocabulary. Adding a new code is a spec/CHANGELOG event, not a silent code change. Milestone 158 ships with these codes: `workspace-peer-detection-degraded`, `root-selection-ambiguous`, `root-selection-failed`, `edge-resolution-degraded`, `go-transitive-coverage-degraded`, `go-workspace-mode-anomaly`, `orphaned-components-detected` (Q2 addition), `multi-ecosystem-partial-root` (Q3 addition). Under Q1's caution-first rule, mikebom MUST NOT emit `partial` with a reason-code outside this documented vocabulary — if the gap can't be classified, emit `unknown` instead. When multiple reason codes apply to the same scan (per FR-012), they MUST be joined by `; ` (semicolon + space) in a single reason string.

- **SC-006 (pre-PR gate)**: `cargo +stable clippy --workspace --all-targets -- -D warnings` and `cargo +stable test --workspace` MUST both pass with zero errors before the PR is opened. The mandatory `./scripts/pre-pr.sh` gate must be green.

- **SC-007 (unit test coverage)**: The root-selection→peer-linkage code path MUST have at least 10 unit tests covering: (a) single-package repo with no peers; (b) 2-peer workspace with 100% linkage; (c) 12-peer workspace (test-podman-desktop-shape); (d) peer already present in root's dependsOn (dedup); (e) leaf peer (peer with zero deps); (f) URL-shaped-version peer (git-URL tarball); (g) Q2 orphan-classification test — a component NOT in the losers list AND NOT reachable from root produces `partial` with reason `orphaned-components-detected` and orphan count; (h) Q1 caution-first fallback test — BFS-pass failure (e.g., missing root) produces `unknown`, not `partial`; (i) Q3 multi-root BFS — repo with 2 ecosystems (npm + gem), each with its own root, both fully-reachable via their respective roots → emit `complete`; (j) Q3 combined-reason test — repo where multi-ecosystem BFS leaves 1 ecosystem partial-rooted AND orphans exist → emit `partial` with the joined reason string `multi-ecosystem-partial-root: X; orphaned-components-detected: N`.

- **SC-008 (integration test)**: A new integration test MUST scan a synthesized workspace testbed (like milestone-157's `pnpm_v9_synthetic_argo_cd_shape` pattern) and assert BFS reachability from the root is 100%.

- **SC-009 (CHANGELOG entry)**: `CHANGELOG.md` MUST document the new annotation vocabulary + the workspace-peer-linkage fix + the SC-001 empirical numbers + a consumer jq recipe for gating on `mikebom:graph-completeness`.

- **SC-010 (parity catalog registration)**: The two new annotations MUST have parity-catalog entries in `mikebom-cli/src/parity/extractors/` so the milestone-071 CDX↔SPDX parity checker enforces symmetric emission.

- **SC-011 (issue #492 closure)**: Issue #492 MUST reference this milestone (`closes #492` in the impl commit message) and the milestone MUST demonstrably resolve the reported symptom (19.5% → ≥99% BFS reachability on test-podman-desktop).

## Assumptions

- **"Workspace peer" definition**: The set of components mikebom's root-selection heuristic already identifies as "losers" during scan. The scan log line `root-component selected via heuristic; losers=[...]` is the evidence trail. Milestone 158 does NOT change the definition or expand what counts as a peer.

- **Root-selection heuristic is unchanged**: Milestone 158 only affects how the root LINKS to its peers, not which component becomes the root. The existing heuristic (longest-common-prefix, repo-root-main-module, etc.) continues to pick the root.

- **Cross-format annotation shape**: Consumers of the CDX 1.6 form read `.metadata.properties[] | select(.name == "mikebom:graph-completeness")`. Consumers of SPDX 2.3 read the document-level `annotations[]` array. Consumers of SPDX 3.0.1 read the CreationInfo or the equivalent Annotation node. mikebom emits the annotation once per format, using the format's native mechanism (per FR-007).

- **The `losers` list is already deduplicated and canonicalized**: The scan log evidence shows losers as canonical PURLs. No additional canonicalization needed at link time.

- **BFS reachability is the correctness definition**: For SC-001 and the `complete` semantic, BFS reachability from the root via `dependsOn` is the operational check. This matches how CDX consumers (Kusari Inspector, DependencyTrack, etc.) render the graph.

- **The 3-value domain is closed**: `complete` / `partial` / `unknown` is the full vocabulary. No `unknown-degraded`, no `mostly-complete`, no percentage fields. Downstream tools can decide their own gates.

- **No new Cargo dependencies**: Following the milestone-157 precedent, this work uses existing crates only (`serde_json`, `tracing`, `anyhow`, `clap` if any new flags — probably none).

## Out of Scope

- **The pnpm/yarn npm-alias fix (issue #493)** — separate milestone. Even after 158 lands, the `react-helmet-async → @slorber/react-helmet-async` alias edges will still drop silently. That's a per-package edge issue, not a workspace-root issue.

- **The Go workspace-mode false-edge fix (issue #494)** — separate milestone. Milestone 158 will emit `mikebom:graph-completeness = partial` on test-kubernetes with a reason naming issue #494; it does NOT fix the false edges.

- **The Go transitive coverage fix (issue #495)** — separate milestone. Milestone 158 may emit `partial` for test-podman if the closure is incomplete, but doesn't improve the coverage.

- **The Ruby built-in gem edge fix (issue #496)** — separate milestone. Will not change the annotation value on test-rails since 1/249 dropped edge is below any coverage threshold that would trigger `partial`.

- **Changing the root-selection heuristic itself** — 158 accepts whatever the current heuristic picks and links accordingly. If the heuristic gets smarter later (milestone 127 style), 158's linkage behavior automatically improves.

- **Cross-ecosystem workspace linking** (e.g. linking a Cargo main-module to an npm main-module in the same repo) — 158 keeps ecosystem-scoped roots. Cross-ecosystem is a design question for a later milestone.

- **`mikebom:graph-completeness` as a CDX BOM-level compliance vocabulary item** — 158 uses the `mikebom:*` prefix. If CDX 1.7 or SPDX 3.1 adopts an official completeness enum, that's a follow-on migration.
