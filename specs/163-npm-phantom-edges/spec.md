# Feature Specification: npm workspace-peer phantom empty-version edges (fix + regression guard)

**Feature Branch**: `163-npm-phantom-edges`
**Created**: 2026-07-05
**Status**: Draft
**Input**: User description: "498" (implement fix for [issue #498](https://github.com/kusari-oss/mikebom/issues/498))

## Clarifications

### Session 2026-07-05

- Q: How should mikebom dispose of an unresolvable workspace-peer dep declaration? → A: SUPPRESS the edge from the source's `dependsOn` list entirely + emit `mikebom:unresolved-declared-dep` annotation on the source workspace peer naming the unresolvable dep. Zero phantom PURLs anywhere in the graph — SC-004 invariant preserved unconditionally. Consumer BFS traversal never hits phantom edges (SC-001 reachability unaffected by unresolvable deps). Auditors reading the source component's annotation see the "declared but unresolvable" signal per Constitution Principle X (Transparency). Rejected: emit-phantom-with-annotation (still pollutes graph) and emit-phantom-unchanged (the current bug).
- Q: Range-spec mismatch — what happens when the declared range doesn't match any resolved version? → A: SUPPRESS the edge + emit `mikebom:unresolved-declared-dep` annotation on the source (unified with Q1 disposition). Consistent single rule for "the peer's declaration doesn't cleanly resolve" — whether the reason is "no lockfile entry" (Q1) or "lockfile entry doesn't satisfy declared range" (Q2), the mikebom emission behavior is identical. Constitution Principle IX (Accuracy) — no dubious edges. Rejected: emit-edge-anyway (violates accuracy — the emitted target doesn't satisfy the declared range) and belt-and-suspenders dual-emission (unnecessary complexity on a rare case).

## Motivation

Discovered during the milestone-158 T035 measurement + follow-on Trivy/Syft comparison on `kusari-sandbox/test-podman-desktop`: mikebom's npm workspace-peer readers emit dependency edges pointing at **empty-version PURLs** (e.g. `pkg:npm/%40docusaurus/core@` — no version segment after the `@`) that don't resolve to any emitted component. BFS reachability walks these phantom edges to dead ends, capping test-podman-desktop's `mikebom:graph-completeness` reachability at only 24.6% (698 of 2835 components).

**The real resolved versions DO exist as components in the same SBOM** (`pkg:npm/%40docusaurus/core@3.10.1` is emitted elsewhere in the components array). The resolution is possible — the workspace-peer readers just aren't wiring the edge to the resolved component. This is a Constitution Principle IX (Accuracy) failure: emitted edges point to non-existent PURLs.

Empirical measurement 2026-07-03 on `test-podman-desktop`:

- **159 components** with empty-version PURL shape (`pkg:npm/name@` — no version segment).
- **902 of 6208 dep-graph edges (14.5%)** target these phantom PURLs.
- **BFS reaches 698/2835 (24.6%)** from `metadata.component` — capped by phantom edges walking to dead ends.
- **Milestone-158 aspirational target: ≥99%**. This issue is the load-bearing blocker.

Sample emission from the pre-163 CDX:

```json
{
  "ref": "pkg:npm/docs@0.0.0",
  "dependsOn": [
    "pkg:npm/%40docusaurus/core@",
    "pkg:npm/%40docusaurus/module-type-aliases@",
    "pkg:npm/%40docusaurus/plugin-client-redirects@"
  ]
}
```

The `docs/package.json` (a workspace peer inside podman-desktop's monorepo) declared these as deps with range specs (e.g. `"@docusaurus/core": "^3.10.1"`). The workspace-peer reader parsed the name but couldn't find the resolved version locally, so it emitted the dep-name with an empty version segment rather than cross-resolving against the top-level lockfile.

Three-way tool comparison on `test-podman-desktop`:

| tool | npm components | BFS reachable | comment |
|---|---|---|---|
| mikebom (post-158) | **2835** | 698 (24.6%) | Detects most packages, honestly flags `partial` |
| Syft 1.44.0 | 2650 | 0 (0.0%) | Structurally broken root |
| Trivy 0.71.1 | 1817 | 1817 (100%) | Perfect reachability, misses 1018 packages mikebom finds |

Of the 1018 packages mikebom finds but Trivy doesn't:
- **859 have resolved versions** — mikebom's workspace-peer coverage advantage. To PRESERVE.
- **159 have empty versions** — the phantom-edge bug. To FIX.

## Distinction from milestones 160, 161, 162

- **Milestone 160** (#494): fixed **missing** Go transitive edges (proxy-fetch degradation).
- **Milestone 161** (#495): fixed **wrong** Go workspace edges (workspace-root leakage).
- **Milestone 162** (#496): fixed **silently dropped** Ruby built-in edges (toolchain gems).
- **Milestone 163** (this issue, #498): fixes **phantom empty-version** npm edges (workspace-peer readers not cross-resolving).

All four are complementary but non-overlapping — each addresses a different failure class in a different ecosystem.

## User Scenarios & Testing

### User Story 1 - SBOM consumer's BFS traversal reaches ≥99% of npm components on test-podman-desktop (Priority: P1)

An SBOM consumer (Kusari Inspector, a vulnerability scanner, a graph-analysis tool) loads mikebom's npm SBOM for `test-podman-desktop` and BFS-walks the `dependencies[]` graph starting from `metadata.component`. Pre-163, the walk reaches only 698 of 2835 npm components (24.6%) because 902 edges point to phantom empty-version PURLs. Post-163, ≥ 99% of npm components are reachable — matching Trivy's reachability WITHOUT sacrificing mikebom's 1018-package coverage advantage.

**Why this priority**: This is the observed bug's user-visible symptom AND the load-bearing blocker for milestone-158's aspirational ≥99% graph-completeness target. Fixing this issue directly delivers the "best of both worlds": mikebom's coverage advantage (859 packages Trivy misses) PLUS Trivy's reachability property (100% BFS). Constitution Principle IX (Accuracy — no phantom edges) + Principle X (Transparency — consumers can trust the emitted graph).

**Independent Test**: Scan `kusari-sandbox/test-podman-desktop` with mikebom. Compute BFS from the emitted `metadata.component` walking every `dependencies[].dependsOn[]` edge to real (non-phantom) PURLs. Assert:

- Reachable component count ÷ total npm component count ≥ 0.99 (SC-001).
- Zero emitted PURLs match the shape `pkg:npm/*@` (empty version segment after `@`) (SC-004).
- Total npm component count ≥ 2676 (SC-005 — 2835 baseline minus 159 phantom entries; the 859 resolved-version workspace-peer transitives mikebom finds MUST remain in the SBOM).

**Acceptance Scenarios**:

1. **Given** `test-podman-desktop` scanned via `mikebom sbom scan --path test-podman-desktop --format cyclonedx-json`, **When** iterating the emitted `components[]` array, **Then** ZERO components MUST have a PURL matching `pkg:npm/*@` (empty version segment).

2. **Given** the same scan, **When** BFS-walking the `dependencies[]` graph from `metadata.component`, **Then** the reachable component count MUST be ≥ 99% of the total npm component count.

3. **Given** the same scan, **When** counting the total npm components in the SBOM, **Then** the count MUST be ≥ 2676 (preserving mikebom's 859-package resolved-version coverage advantage over Trivy per the milestone-158 audit; the 159 phantom empty-version entries this milestone eliminates transform into edges or annotations, not components).

4. **Given** a non-npm repo (any milestone-090 fixture except `npm`), **When** mikebom scans, **Then** the emitted SBOM MUST be byte-identical to pre-163 (SC-003 dual-side byte-identity).

---

### User Story 2 - Workspace-peer dep declarations cross-resolve against the top-level lockfile (Priority: P2)

A compliance auditor loads a mikebom SBOM for a multi-workspace npm monorepo (like podman-desktop) and wants to know: for a workspace peer's `dependencies:` entry (e.g., `docs/package.json` declaring `"@docusaurus/core": "^3.10.1"`), the emitted edge MUST resolve to the concrete pinned version (`pkg:npm/%40docusaurus/core@3.10.1`) from the top-level lockfile — not to a phantom empty-version PURL.

**Why this priority**: Constitution Principle IX (Accuracy). This is the mechanism that fixes the bug. Auditors verifying dependency-tree correctness need the cross-resolution to hold.

**Independent Test**: For every emitted `pkg:npm/` component whose PURL contains `@<real-version>`, verify that at least one incoming edge from a workspace-peer source (a `pkg:npm/...` component with a `package.json` source path) matches the range spec that peer declared. Also verify zero incoming edges to any `pkg:npm/*@` (empty version) PURL — i.e., no phantom edges REMAIN in the graph.

**Acceptance Scenarios**:

1. **Given** `test-podman-desktop` scanned, **When** enumerating the edges of `pkg:npm/docs@0.0.0`, **Then** every `@docusaurus/core` edge MUST target the concrete `pkg:npm/%40docusaurus/core@3.10.1` (or the actual resolved version) — NEVER `pkg:npm/%40docusaurus/core@` with empty version.

2. **Given** the same scan, **When** enumerating all edges in `dependencies[]`, **Then** ZERO edges MUST target a phantom empty-version PURL.

3. **Given** a workspace peer with a nested `node_modules/` containing a different resolved version than the top-level (rare but valid npm topology), **When** the reader resolves the dep, **Then** the closer-ancestor version is preferred over the top-level (matches actual Node.js runtime resolution semantics per FR-003).

---

### User Story 3 - Non-npm scans byte-identical to pre-163 (Priority: P3)

Users scanning repos with NO npm components see byte-identical SBOM output vs pre-163 milestones.

**Why this priority**: Regression guard. The fix is scoped to the npm workspace-peer reader. Mirrors milestone-158/159/160/161/162 dual-side byte-identity precedent.

**Independent Test**: Regenerate all non-npm milestone-090 goldens with the milestone-163 code. Diff against pre-163. Zero diff bytes on the 10 non-`npm` ecosystems × 3 formats = 30 goldens. The `npm` fixture goldens MAY change if its `package.json` files reference deps that get cross-resolved.

**Acceptance Scenarios**:

1. **Given** the milestone-090 cargo fixture (no npm components), **When** mikebom scans, **Then** the emitted CDX diff vs. pre-163 is exactly ZERO bytes.

2. **Given** the milestone-090 `npm` fixture (single-workspace), **When** mikebom scans, **Then** the emitted CDX MAY change if the fixture's `package.json` file declares deps that get cross-resolved. Diffs limited to affected edge targets — no new components, no new annotations.

### Edge Cases

- **Workspace peer references a dep NOT in the top-level lockfile**: mikebom cannot cross-resolve. Options: (a) suppress the edge; (b) emit with empty version (current pre-163 behavior — the bug); (c) emit with a new `mikebom:unresolved-declared-dep` annotation. The chosen option is Q1 clarification-worthy.

- **Multiple resolved versions in the same tree**: `top/node_modules/@docusaurus/core@3.10.1` AND `top/packages/docs/node_modules/@docusaurus/core@3.9.0`. Per FR-003, the closer-ancestor version wins. This matches Node.js's actual runtime resolution algorithm.

- **Peer dep vs regular dep**: workspace peers can declare `peerDependencies:` which have different resolution semantics. Milestone 163 does NOT change peer-dep handling — that's milestone-147's C1/C2 scope. The fix applies to `dependencies:` and `devDependencies:` blocks only.

- **Range spec doesn't match any resolved version**: e.g., peer says `"^4.0.0"` but only `3.10.1` is resolved. Per Q2 clarification: SUPPRESS the edge + emit `mikebom:unresolved-declared-dep` annotation on the source (unified with FR-004 disposition). Accuracy-first — no dubious edges where target doesn't satisfy the declared range.

- **npm alias syntax at workspace peer**: `"my-name": "npm:@real/package@^1.0.0"` — the peer declared an alias. Milestone-159 handled aliases at the pnpm-lock + yarn-lock v1 layer; workspace-peer `package.json` alias handling is out of scope for this milestone unless empirical investigation surfaces it as a common pattern.

- **Root `package.json` also declares the dep**: if the workspace root `package.json` also lists the dep, the top-level lockfile resolves it. Cross-resolution is guaranteed. Common happy path.

- **Version-less DEPENDENCIES block reference**: package.json `dependencies:` can list a dep as `"@docusaurus/core": "*"` or `"@docusaurus/core": "latest"`. The lockfile still pins it to a concrete version. Cross-resolution proceeds normally against the pinned version.

## Requirements

### Functional Requirements

- **FR-001**: mikebom's npm workspace-peer readers MUST cross-resolve every workspace-peer `package.json` dep-declaration against the **union of lockfile-derived Tier A entries across all project roots in the scan** (`package-lock.json`, `yarn.lock`, `pnpm-lock.yaml`, or `bun.lock`) to obtain the concrete resolved version. When a resolution succeeds, the emitted edge MUST target the concrete `pkg:npm/<name>@<version>` PURL — never an empty-version PURL. When multiple independent monorepos exist in the same scan tree and produce colliding names with different versions, the first encountered entry wins (deterministic per project-root walk order).

- **FR-002**: The workspace-peer readers MUST NOT emit any `pkg:npm/` component or `dependsOn` edge target with an empty version segment (i.e., a PURL matching `pkg:npm/*@` with nothing after the `@`). Pre-163 emitted 159 such components on test-podman-desktop; post-163 MUST emit zero.

- **FR-003**: When multiple resolved versions of the same package exist in the workspace tree (e.g., root `node_modules/foo@1.0.0` AND `packages/pkg-a/node_modules/foo@2.0.0`), mikebom MUST prefer the **closest-ancestor** version — the resolution that Node.js's actual runtime resolver would use for the declaring workspace peer. Matches the walk-up-node_modules-parents algorithm documented in the Node.js resolution spec.

- **FR-004**: When cross-resolution fails per Q1 (no lockfile entry for the declared dep) OR per Q2 (lockfile entry doesn't satisfy the declared semver range), mikebom MUST NOT emit a phantom empty-version edge AND MUST NOT emit an edge to a resolved-but-out-of-range target. Instead — for BOTH failure cases with unified disposition: SUPPRESS the edge from the source workspace-peer's `dependsOn` list AND emit `mikebom:unresolved-declared-dep = "<dep-name>"` annotation on the source workspace-peer component. This preserves the "declared" signal for auditors while removing all phantom / dubious PURLs from consumer graph-traversal paths.

- **FR-005**: FR-001–FR-004 semantics apply to all four top-level lockfile formats mikebom currently reads: `package-lock.json` (npm), `pnpm-lock.yaml` (pnpm), `yarn.lock` v1 (yarn v1), and `bun.lock` (bun — milestone-106 US2 support). Yarn Berry (v2+) lockfile is out of scope per Assumption §5.

- **FR-006**: The 859-package resolved-version coverage advantage over Trivy MUST be preserved — post-163 emission MUST retain at least the 859 resolved-version packages. The 159 previously-phantom entries transform into either (a) real edges to already-emitted resolved components (FR-001) OR (b) `mikebom:unresolved-declared-dep` annotations (FR-004), depending on cross-resolution outcome. Both dispositions REMOVE the phantom empty-version component from `components[]`. In the (a) case, the phantom is replaced by an edge to the already-emitted resolved component (target was already visible to consumers — no signal loss). In the (b) case, the phantom is replaced by a source-side annotation on the workspace-peer's main-module component (new C115 signal, no phantom endpoint). Post-163 npm component count therefore drops from 2835 → 2676 on `test-podman-desktop`; no RESOLVED lockfile-derived component is dropped.

- **FR-007**: Standards-native precedence per Constitution Principle V. If either CDX 1.6 or SPDX 3.0.1 introduces an official "declared-but-unresolved dep" property, mikebom MUST prefer that property. As of 2026-07-05, no such standard property exists; the `mikebom:unresolved-declared-dep` prefix is used.

- **FR-008**: `mikebom:unresolved-declared-dep` MUST be registered as a new per-component parity-catalog row (C115) with `Directionality::SymmetricEqual` — matching the milestone-158/159/160/161/162 pattern.

- **FR-009**: When the workspace-peer cross-resolution runs, mikebom MUST emit an info-level tracing log per Gemfile/lockfile pair: `"npm workspace-peer cross-resolution summary"` with fields `workspace_root`, `resolved_count`, `phantom_prevented_count`, `unresolved_declared_count`. Grep-friendly for CI-log analysis per the milestone-157/158/159/160/161/162 observability convention.

- **FR-010**: mikebom's existing milestone-147 peer-dep handling (peerDependencies with C1/C2 annotations) MUST remain unchanged. This milestone's fix scope is `dependencies:` and `devDependencies:` cross-resolution ONLY.

### Key Entities

- **Top-level lockfile**: The authoritative version-resolution source for a workspace root. Read from `<workspace-root>/package-lock.json`, `<workspace-root>/pnpm-lock.yaml`, `<workspace-root>/yarn.lock`, or `<workspace-root>/bun.lock`. Milestone 163 assumes exactly one lockfile per workspace root (multi-lockfile-in-same-root edge case is out of scope). Multiple independent monorepos in the same scan tree each contribute their own lockfile entries to the cross-workspace resolution index; see Assumptions §multi-monorepo-scans.

- **Workspace peer**: A subdirectory under the workspace root with its own `package.json` (e.g., `packages/docs/package.json`, `apps/renderer/package.json`). Declared via the workspace root's `workspaces:` field OR discovered via directory-walk. Its `package.json`'s `dependencies:` + `devDependencies:` blocks are subject to FR-001 cross-resolution.

- **Cross-resolution result**: For each `(workspace-peer, dep-name, range-spec)` triple: either `Resolved { concrete_version: String }` (FR-001 happy path) or `Unresolved` (FR-004 annotation path).

- **`mikebom:unresolved-declared-dep` (per-component)**: Component-scope annotation on a workspace-peer component naming a declared dep whose cross-resolution failed. Bare-string value (single dep) OR JSON array of names (multiple unresolved deps per source). Byte-identical shape to the milestone-159 C106/C107 + milestone-162 C114 multi-value precedent.

## Success Criteria

### Measurable Outcomes

- **SC-001 (test-podman-desktop BFS reachability)**: After milestone 163 ships, running `mikebom sbom scan --path test-podman-desktop --format cyclonedx-json` and BFS-walking the emitted `dependencies[]` graph from `metadata.component` MUST reach ≥ 99% of the emitted npm component count. Pre-163 baseline: 24.6% (698 of 2835). Target: ≥ 99%. This SC is empirically-locked to the concrete testbed named in issue #498.

- **SC-002 (phantom edge count)**: The 902 phantom edges targeting empty-version PURLs on test-podman-desktop MUST reduce to zero. Post-163: `jq '[.dependencies[].dependsOn[] | select(test("pkg:npm/[^@]+@$"))] | length' out.cdx.json` MUST return 0.

- **SC-003 (dual-side byte-identity, mirrors milestones 158/159/160/161/162)**: For every milestone-090 non-`npm` golden fixture (10 of 11 ecosystems: apk, bazel, cargo, cmake, deb, gem, golang, maven, pip, rpm), the emitted CDX / SPDX 2.3 / SPDX 3 SBOMs MUST be byte-identical to pre-163. The `npm` fixture goldens MAY change if its `package.json` files reference deps that get cross-resolved. Zero diff bytes on the 10 non-`npm` × 3 = 30 goldens.

- **SC-004 (zero empty-version PURLs)**: Post-163, the emitted SBOM from any scan MUST have ZERO `pkg:npm/` PURLs matching the shape `pkg:npm/*@` (empty version segment). Verification: `jq '[.components[].purl | select(test("^pkg:npm/[^@]+@$"))] | length' out.cdx.json` == 0 across all fixtures + test-podman-desktop.

- **SC-005 (coverage advantage preserved)**: The 859-package resolved-version coverage advantage over Trivy MUST be preserved. Post-163 npm component count on `test-podman-desktop` MUST be ≥ **2676** (pre-163 baseline 2835 minus the 159 phantom empty-version entries this milestone eliminates; the 859 resolved-version lockfile-derived packages that Trivy misses remain in the SBOM). Post-163's `.components | map(select(.purl | startswith("pkg:npm/"))) | length ≥ 2676`. Additionally, ALL 859 resolved-version lockfile-derived packages emitted pre-163 MUST also be emitted post-163 (no RESOLVED component is dropped).

- **SC-006 (pre-PR gate)**: `cargo +stable clippy --workspace --all-targets -- -D warnings` and `cargo +stable test --workspace --no-fail-fast` MUST both pass with zero errors before the PR is opened.

- **SC-007 (unit test coverage)**: The new cross-resolution code paths MUST have at least 10 unit tests covering: (a) simple root-lockfile resolution succeeds; (b) closest-ancestor resolution prefers nested over root (FR-003); (c) unresolved dep produces `mikebom:unresolved-declared-dep` annotation (FR-004); (d) zero empty-version PURLs emitted for happy path; (e) zero empty-version PURLs emitted for unresolved path; (f) `dependsOn` edge points to concrete-version PURL when resolved; (g) `dependsOn` edge is SUPPRESSED when unresolved (source-side annotation emitted instead); (h) FR-010 peer-dep handling unchanged (regression guard); (i) devDependencies get same cross-resolution as dependencies (dedicated test via T024a); (j) FR-006 coverage-preservation — every resolved component still emitted.

- **SC-008 (integration test)**: A new integration test at `mikebom-cli/tests/npm_phantom_edges.rs` MUST synthesize a multi-workspace npm monorepo (workspace root + 2 workspace peers, one peer declaring a dep resolved via the top-level lockfile, one declaring an unresolvable dep) and assert (a) resolved dep produces a concrete-version edge; (b) unresolvable dep produces `mikebom:unresolved-declared-dep` annotation; (c) zero empty-version PURLs; (d) 100% BFS reachability from the workspace-root component.

- **SC-009 (CHANGELOG entry)**: `CHANGELOG.md` MUST document the fix + FR-004 annotation vocabulary + the SC-001 empirical numbers (24.6% → ≥99%) + a consumer jq recipe for verifying zero empty-version PURLs.

- **SC-010 (parity catalog registration)**: The new annotation (C115 per-component) MUST have a parity-catalog entry with `Directionality::SymmetricEqual`. Milestone-071 parity check MUST pass symmetrically across CDX / SPDX 2.3 / SPDX 3.

- **SC-011 (issue #498 closure)**: Issue #498 MUST reference this milestone (`closes #498` in the impl commit message) and the milestone MUST demonstrably resolve the reported symptom (BFS reachability 24.6% → ≥99% on test-podman-desktop).

## Assumptions

- **Ground truth = top-level lockfile**: The workspace root's lockfile (package-lock.json, pnpm-lock.yaml, yarn.lock v1, OR bun.lock) is the authoritative resolved-version source. SC-001 measures against this. Consumers running SC-001 verification themselves need the fixture with its lockfile intact.

- **Multi-monorepo scans**: mikebom may encounter multiple independent monorepo project roots in a single scan tree (e.g., multiple app directories inside a container image, each with its own lockfile). The cross-workspace resolution index is a UNION across ALL such roots' lockfile-derived Tier A entries. Name collisions across independent monorepos default to first-encountered (rare in practice; overrideable in a future milestone if a concrete case emerges).

- **`test-podman-desktop` is the empirical benchmark**: SC-001/SC-002/SC-005 numbers are pinned to this repo. Pre-163 measurement: 24.6% BFS reachability, 902 phantom edges, 2835 total npm components.

- **Nested node_modules is rare in typical npm workspaces**: Modern workspace tooling (npm workspaces, pnpm, yarn v3 Berry) uses hoisted node_modules where the root is the only resolution site. FR-003 nested-resolution handles the edge case but is not the primary path.

- **Yarn Berry (v2+) is out of scope**: yarn.lock v2/v3/v4 (Berry) has different resolution semantics (Plug'n'Play, virtual packages). Milestone 163 targets yarn v1 only; Berry support is a separate future milestone if needed.

- **No new Cargo dependencies**: Following the milestone-158/159/160/161/162 precedent, this work uses existing crates only.

- **milestone-090 npm fixture MAY change**: If the fixture's `package.json` files declare deps that get cross-resolved, the goldens will change. Verified at authoring time via inspection.

- **SC-001 target is empirically-adjustable**: If T014-T016 empirical investigation reveals corner cases that cap reachability below 99% (e.g., legitimate cross-workspace edges that BFS can't traverse), SC-001 may be revised inline per the milestone-156/157/158/159/160/161/162 empirical-revision pattern.

- **The fix is investigation-guided, not empirical loop**: Unlike milestones 160/161 (which needed multi-round investigation), the root cause is well-understood (workspace-peer reader doesn't cross-resolve). Implementation should require ~1-2 investigation cycles to verify the closest-ancestor semantics + validate on test-podman-desktop.

## Out of Scope

- **Yarn Berry (v2+) lockfile support** — separate future milestone if empirical evidence surfaces it as high-value.

- **Peer-dependency cross-resolution** — milestone 147's C1/C2 peer-dep handling remains unchanged. Milestone 163 scope is `dependencies:` + `devDependencies:` only.

- **Nested-node_modules deep discovery** — the `node_modules/` walker's existing behavior is preserved. FR-003 cross-resolution reads the RESOLVED versions from lockfile entries; it does NOT walk deeper into node_modules subtrees.

- **npm workspace-peer alias handling** — milestone 159 handled aliases at the pnpm-lock + yarn-lock v1 layer. Workspace-peer package.json alias syntax (e.g., `"my-name": "npm:@real/package@^1.0.0"` in a peer's `dependencies:`) is out of scope unless empirical investigation surfaces it as a common pattern.

- **Cross-repo resolution** — if a workspace-peer references a dep hosted OUTSIDE the workspace root (git URL, file URL, tarball URL), no cross-resolution is attempted. These become `mikebom:unresolved-declared-dep` annotations per FR-004.

- **Version-range comparison logic** — mikebom does NOT parse semver ranges to pick "the best match" among multiple resolved versions. The FR-003 closest-ancestor rule sidesteps this: within a given resolution scope, there is exactly one resolved version per package.

- **Retrofitting the fix to milestone-158's graph-completeness reason vocab** — milestone 158's `mikebom:graph-completeness-reason` already includes `orphaned-components-detected` for the class of components made orphan by phantom edges. Post-163, that reason should fire less often on npm workspace scans, but no new reason code is needed.
