# Feature Specification: Workspace-root peer linkage + graph-completeness annotations

**Feature Branch**: `158-graph-completeness`
**Created**: 2026-07-03
**Status**: Draft
**Input**: User description: "492" (implement fix for [issue #492](https://github.com/kusari-oss/mikebom/issues/492))

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
- **Multiple ecosystems in the same repo (test-rails: gem + npm)**: each ecosystem gets its own root; the annotation reflects the OVERALL graph completeness (worst case across ecosystems).
- **Root's `dependsOn` already contains a peer (test-podman-desktop currently links 1 of 12)**: implementation must not double-add. The final list is the sorted-deduped union.
- **Non-npm workspace ecosystems (Cargo workspaces, Go go.work, pnpm workspaces, yarn workspaces)**: same fix applies at the appropriate root-selection layer. Scope of milestone 158: covers whichever ecosystems mikebom's root-selection heuristic already identifies peers for.

## Requirements

### Functional Requirements

- **FR-001**: mikebom's root-selection subsystem MUST make the "losers" list (the workspace-peer components identified but not chosen as root) available to the SBOM emission layer. Discovery of this list already happens today (evidenced by the `WARN root-component selected via heuristic; losers=[...]` scan log line).

- **FR-002**: For each emitted SBOM, mikebom MUST add every workspace-peer component from FR-001 to the root component's `dependsOn` (CDX 1.6 wire format) / the equivalent SPDX relationship (`DEPENDS_ON` in SPDX 2.3 relationships; `Relationship` element with `relationshipType = dependsOn` in SPDX 3). Peers already present in the root's `dependsOn` MUST NOT be double-added — the final list is the sorted-deduped union.

- **FR-003**: mikebom MUST emit a document-scope annotation `mikebom:graph-completeness` on every SBOM regardless of ecosystem, wire format, or repo shape. The value MUST be one of the three literal strings: `complete`, `partial`, `unknown`.

- **FR-004**: When `mikebom:graph-completeness` value is `partial` or `unknown`, mikebom MUST emit a companion document-scope annotation `mikebom:graph-completeness-reason` with a human-readable string naming the cause. The reason string MUST be structured as `<code>: <human-readable-detail>` where `<code>` is a stable identifier (e.g. `workspace-peer-detection-degraded`, `root-selection-ambiguous`, `root-selection-failed`) so downstream tools can dispatch on it.

- **FR-005**: When `mikebom:graph-completeness` value is `complete`, mikebom MUST NOT emit `mikebom:graph-completeness-reason` (avoids noise for the healthy case).

- **FR-006**: The annotation values `complete` / `partial` / `unknown` MUST have a stable, documented semantic:
  - `complete`: mikebom asserts every component in the emitted SBOM is reachable from the root via `dependsOn` traversal.
  - `partial`: mikebom detected a graph-coverage gap but could still produce a useful SBOM. The reason names the specific gap class.
  - `unknown`: mikebom cannot determine reachability at emission time (e.g. root-selection failed or produced multiple ambiguous roots).

- **FR-007**: The annotation MUST appear in every emitted wire format using that format's native property/annotation mechanism:
  - CycloneDX 1.6: `metadata.properties[]` entry with `name = "mikebom:graph-completeness"`.
  - SPDX 2.3: document-level `Annotation` (via `annotations[]`) attached to the `SPDXRef-DOCUMENT`.
  - SPDX 3.0.1: `CreationInfo.mikebom:graph-completeness` extension property or an equivalent `Annotation` element attached to the document scope.

- **FR-008**: When mikebom's root-selection succeeds and identifies workspace peers, mikebom MUST link every peer to the root (per FR-002) AND emit `graph-completeness = complete` (per FR-003) if BFS reachability from the root covers 100% of emitted components. Otherwise emit `partial` with a reason.

- **FR-009**: For repos with no workspace peers (single-package repos), mikebom MUST emit `graph-completeness = complete` without emitting any additional edges (no fabricated peer relationships).

- **FR-010**: Standards-native precedence per Constitution Principle V. If either CDX 1.6 or SPDX 3.0.1 introduces an official "graph completeness" or "SBOM completeness" property, mikebom MUST prefer that property over the `mikebom:*` prefix. Milestone 158 emits under `mikebom:*` because no such standard property exists at emission time.

### Key Entities

- **Workspace peer**: A component identified by mikebom's root-selection heuristic as a "loser" — a candidate root that mikebom did not select. Peers are already ecosystem-typed (npm scoped, Cargo package, Go module, etc.) and already have a `bom-ref` / `SPDXID` — no new identity is minted for them.

- **Root component**: The component named in `metadata.component` (CDX) or the primary `describes` relationship (SPDX). Milestone 158 does NOT change how the root is selected — only how it links to its workspace peers.

- **`mikebom:graph-completeness`**: A stable document-scope annotation with the three-valued domain `complete | partial | unknown`. Consumers use this to gate on graph reliability.

- **`mikebom:graph-completeness-reason`**: A document-scope annotation present only when completeness ≠ complete. Structured as `<code>: <human-readable-detail>` where `<code>` is one of a fixed vocabulary (`workspace-peer-detection-degraded`, `root-selection-ambiguous`, `root-selection-failed`, etc.).

## Success Criteria

### Measurable Outcomes

- **SC-001 (test-podman-desktop reachability)**: After milestone 158 ships, running `mikebom sbom scan --path test-podman-desktop` and BFS-traversing from `metadata.component.bom-ref` MUST reach **≥99%** of all npm components in the emitted SBOM (measured as `reachable_count / total_npm_count`). Pre-158 empirical baseline: **19.5% (552/2835)**. Target: ≥99% (≥2807/2835). This SC is empirically-locked to the concrete testbed named in issue #492.

- **SC-002 (dual-side guard, mirrors milestone 157)**: For every milestone-090 non-workspace golden fixture (all 11 non-monorepo ecosystems: alpine, apk, cargo, cyclonedx-source, deb, gem, maven, npm, pip, rpm, spdx-source), the emitted CDX/SPDX 2.3/SPDX 3 SBOMs MUST be byte-identical to pre-158 EXCEPT for one added `mikebom:graph-completeness = complete` annotation. Zero other diff bytes.

- **SC-003 (annotation universal presence)**: 100% of SBOMs emitted by mikebom in any wire format MUST contain exactly one `mikebom:graph-completeness` annotation at document scope.

- **SC-004 (annotation value distribution over the testbed)**: Running mikebom across the 5 `kusari-sandbox/test-*` repos, the observed `mikebom:graph-completeness` values MUST match:
  - test-podman-desktop: `complete` (workspace peers auto-linked)
  - test-guac-visualizer: `complete` (no workspace)
  - test-rails: `complete` (multi-ecosystem, gem + npm each cleanly rooted)
  - test-podman: `complete` OR `partial` (Go — depends on separate issue #495 investigation, but 158 emits an accurate value regardless)
  - test-kubernetes: `partial` (Go workspace-mode false-edge issue #494 unresolved, so completeness cannot be `complete`); reason MUST name issue-#494-shape cause.

- **SC-005 (reason string vocabulary stability)**: The set of `<code>` prefixes emitted in `mikebom:graph-completeness-reason` MUST be a documented finite vocabulary. Adding a new code is a spec/CHANGELOG event, not a silent code change. Milestone 158 ships with at least these codes: `workspace-peer-detection-degraded`, `root-selection-ambiguous`, `root-selection-failed`.

- **SC-006 (pre-PR gate)**: `cargo +stable clippy --workspace --all-targets -- -D warnings` and `cargo +stable test --workspace` MUST both pass with zero errors before the PR is opened. The mandatory `./scripts/pre-pr.sh` gate must be green.

- **SC-007 (unit test coverage)**: The root-selection→peer-linkage code path MUST have at least 6 unit tests covering: (a) single-package repo with no peers; (b) 2-peer workspace with 100% linkage; (c) 12-peer workspace (test-podman-desktop-shape); (d) peer already present in root's dependsOn (dedup); (e) leaf peer (peer with zero deps); (f) URL-shaped-version peer (git-URL tarball).

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
