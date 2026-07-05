# Feature Specification: pnpm v9 multi-version edge disambiguation

**Feature Branch**: `164-pnpm-multi-version-edges`
**Created**: 2026-07-05
**Status**: Draft
**Input**: Empirical follow-up to milestone 163. 2026-07-05 re-measurement of `github.com/podman-desktop/podman-desktop` with milestone-163 mikebom (`bfd0f6d`) confirmed BFS reachability jumped 24.6% → 77.4% (+52.8pp) as designed. 22-point residual gap remains. Root-cause investigation traced 435 of 568 remaining orphans (76.6% of the gap) to a single bug class: pnpm-lock v9 multi-version edge misresolution — same-name package emitted at multiple versions, parent edges point at the wrong version because the pnpm-lock parser doesn't preserve the version qualifier through to the edge resolver.

## Motivation

Post-163 measurement (2026-07-05, live upstream podman-desktop, fresh clone):

- Empty-version PURLs: **0** ✅ (milestone-163 SC-004 pass)
- Phantom edges: **0** ✅ (milestone-163 SC-002 pass)
- BFS reachability: **24.6% → 77.4%** (+52.8pp)
- 568 orphans remain (out of 2748 npm components)

Orphan classification:

- **435 (76.6%) — multi-version orphans**: same package name emitted at multiple versions (e.g. `pkg:npm/@algolia/autocomplete-core@1.17.9` AND `pkg:npm/@algolia/autocomplete-core@1.19.8`). Only one version is reachable per name; the other is orphaned.
- **~50-100 — platform-specific optional bindings** (`@oxc-parser/binding-*`, `@esbuild/*-*`): out of scope, separate follow-on.
- **~30-50 — truly isolated**: out of scope, separate investigation.

Concrete root cause verified 2026-07-05:

- **Lockfile ground truth** (podman-desktop pnpm-lock.yaml, `@docsearch/react@3.9.0`'s deps):
  ```yaml
  dependencies:
    '@algolia/autocomplete-core': 1.17.9(@algolia/client-search@5.42.0)(algoliasearch@5.42.0)(search-insights@2.17.3)
  ```

- **Mikebom's emitted SBOM** (`pkg:npm/@docsearch/react@3.9.0.dependsOn`):
  ```
  → pkg:npm/@algolia/autocomplete-core@1.19.8   ← WRONG (lockfile says 1.17.9)
  ```

Both versions are legitimately present in the lockfile (different peer-dep constraint scopes install different `@algolia/autocomplete-core` versions). Mikebom's `name_to_purl` disambiguation at `scan_fs/mod.rs:471-525` handles multi-version resolution via a `"<name> <version>"` disambiguation key form (per issue #262, extended for npm alongside cargo per milestone 087). But the pnpm-lock parser doesn't emit into the parent's `depends` array in that disambiguation form — it emits bare names — so the name-only fallback path last-write-wins between `1.17.9` and `1.19.8`.

**This is a Constitution Principle IX (Accuracy) failure**: emitted edges point at the wrong version of a real component. Milestone 087 solved the same class for cargo. Milestone 164 applies the analogous fix to pnpm v9.

## Distinction from milestones 147, 159, 163

- **Milestone 147** (npm peer-deps C1/C2): peer-dep envelope annotation on the CONSUMER. Different mechanism.
- **Milestone 159** (pnpm/yarn alias): alias name-remapping. Different failure class.
- **Milestone 163** (npm workspace-peer phantom empty-version edges): eliminated 159 phantom empty-version PURLs. Different bug — phantom targets, not wrong-version targets.

All four are complementary. Milestone 164 is the natural next step: with phantoms gone (163) and aliases resolved (159), the remaining gap is dominated by wrong-version edges among real components.

## User Scenarios & Testing

### User Story 1 - SBOM consumer's edges resolve to the correct concrete version (Priority: P1)

An SBOM consumer (Kusari Inspector, a vulnerability scanner) loads mikebom's npm SBOM for a pnpm-monorepo with multi-version co-existence. Pre-164, half the multi-version edges point at the wrong version — a downstream CVE lookup gets false negatives (the vulnerable version is orphaned; the "wrong-target" edge points at a safe version). Post-164, every parent's `dependsOn` edge targets the SAME concrete version pnpm-lock actually resolved.

**Why this priority**: Constitution Principle IX (Accuracy). Directly delivers ~15pp of BFS reachability on podman-desktop (435 of 568 orphans). Load-bearing for closing the milestone-158 aspirational ≥99% target.

**Independent Test**: Scan `github.com/podman-desktop/podman-desktop` (freshly cloned). BFS-walk the emitted `dependencies[]` graph. Assert:

- Multi-version orphan count MUST drop from 435 to ≤ 30 (94% reduction).
- Total BFS reachability MUST increase from 77.4% to ≥ 93%.
- Zero regressions in SC-002/SC-004 from milestone 163 (phantom edge + empty-version PURL invariants preserved).

**Acceptance Scenarios**:

1. **Given** a pnpm-lock v9 fixture where `@docsearch/react@3.9.0` depends on `@algolia/autocomplete-core: 1.17.9(...)` per the lockfile, **When** mikebom emits the SBOM, **Then** `pkg:npm/%40docsearch/react@3.9.0`'s `dependsOn` MUST include `pkg:npm/%40algolia/autocomplete-core@1.17.9` — NOT `pkg:npm/%40algolia/autocomplete-core@1.19.8`.

2. **Given** the same fixture where BOTH `1.17.9` AND `1.19.8` are emitted, **When** BFS-walking from the workspace root, **Then** BOTH versions MUST be reachable (each via a distinct parent).

3. **Given** any pnpm-lock v9 lockfile with N distinct versions of the same package, **When** mikebom emits the SBOM, **Then** the fraction of correctly-wired edges (edge target version == lockfile-declared version) MUST equal 100%.

---

### User Story 2 - pnpm v6/v7 legacy lockfile format unchanged (Priority: P2)

Users on pnpm v6/v7 (which pre-dates the peer-dep-suffixed key form) see byte-identical output vs pre-164.

**Why this priority**: Regression guard. Pnpm v6/v7 doesn't use peer-dep-suffixed keys and isn't affected by the milestone-164 fix. Confirming zero drift on the pnpm v6/v7 path prevents accidental over-scoping.

**Independent Test**: Regenerate any milestone-090 pnpm v6/v7 fixture. Diff against pre-164. Zero diff bytes.

**Acceptance Scenarios**:

1. **Given** a pnpm-lock v6 fixture, **When** mikebom scans, **Then** emitted SBOM MUST be byte-identical to pre-164.

---

### User Story 3 - Non-pnpm-v9 scans byte-identical to pre-164 (Priority: P3)

Users scanning repos without pnpm-lock v9 files (yarn.lock, package-lock.json, bun.lock, non-npm ecosystems entirely) see byte-identical SBOM output vs pre-164.

**Why this priority**: Regression guard. Mirrors milestones 158/159/160/161/162/163 dual-side byte-identity precedent.

**Independent Test**: Regenerate all non-pnpm-v9 milestone-090 goldens. Diff against pre-164. Zero diff bytes on 10 non-pnpm × 3 = 30 goldens PLUS the `npm` fixture (which uses package-lock.json, not pnpm-lock.yaml v9).

**Acceptance Scenarios**:

1. **Given** the milestone-090 cargo fixture (no pnpm components), **When** mikebom scans, **Then** the emitted CDX diff vs pre-164 is exactly ZERO bytes.

### Edge Cases

- **Two versions of the same package, both with parents in the lockfile**: happy path. Each parent's `dependsOn` targets its lockfile-declared version.

- **Two versions where one is only in `snapshots:` (peer-dep-suffixed key form)**: mikebom emits the base `<name>@<version>` component (stripping the peer-dep suffix per FR-005). Parent edges MUST target the correct version via the disambiguation key form.

- **Three-plus versions of the same package** (rare but valid): all N versions correctly wire.

- **Package declared with a peer-dep-suffixed key but no matching parent** (dangling target): the component is emitted but has no incoming edge. Legitimate orphan — mikebom's existing `mikebom:orphan-reason` (milestone 158) covers this.

- **Aliased dep at a specific version**: milestone-159's alias handling remains authoritative. If a peer declares `"my-name": "npm:@real@^1.0.0"` AND the lockfile pins `@real` at TWO versions, the alias + version disambiguation compose (each is orthogonal).

- **Same-version-different-source** (GitHub URL vs registry install of the same package at the same version): mikebom emits a single component (PURL is version-only). Milestone 164 doesn't address source-provenance; orthogonal.

- **pnpm v9 `overrides:` field**: overrides pin specific versions across the tree. Milestone 164 respects the lockfile's authoritative version (whatever pnpm resolved with overrides applied), not the pre-override manifest declaration.

- **Malformed peer-dep-suffixed key**: unbalanced parentheses, missing version. Per FR-008, mikebom logs a warning and falls back to bare-name form (pre-164 behavior). Never crash, never silently drop.

## Requirements

### Functional Requirements

- **FR-001**: mikebom's pnpm-lock parser MUST emit each parent's `depends` array in a version-qualified form (`"<name> <version>"` — the disambiguation key form used at `scan_fs/mod.rs:519-525`) whenever the parent's lockfile-declared dep specifier resolves to a specific, unambiguous version. When the lockfile declares `'@docsearch/react@3.9.0' → depends: '@algolia/autocomplete-core': 1.17.9(...)`, the parent's `depends` entry MUST let the downstream `name_to_purl` lookup disambiguate between multiple emitted versions.

- **FR-002**: The disambiguation MUST work for pnpm-lock v9 (the format that introduced peer-dep-suffixed keys). Pnpm v6/v7 lockfiles are unchanged (byte-identity per User Story 2).

- **FR-003**: The disambiguation MUST hold for ALL npm scoped-name shapes: `@scope/name`, plain `name`, and aliased-name variants from milestone 159's alias handling.

- **FR-004**: When the lockfile declares a peer-dep-suffixed dep-value like `1.17.9(@algolia/client-search@5.42.0)(algoliasearch@5.42.0)(search-insights@2.17.3)`, mikebom MUST parse out the base version (`1.17.9`) and use it for edge disambiguation. The peer-dep suffix itself is discarded for PURL purposes (matches milestone 147 posture — peer relationships are their own signal, not part of the target's PURL).

- **FR-005**: The emitted PURL for the target component MUST NOT include the peer-dep suffix. `pkg:npm/@algolia/autocomplete-core@1.17.9` is the correct emitted PURL; NEVER `pkg:npm/@algolia/autocomplete-core@1.17.9(algoliasearch@5.42.0)`.

- **FR-006**: Multi-version co-existence in the emitted SBOM MUST remain observable. When the lockfile pins both `1.17.9` AND `1.19.8`, BOTH components MUST appear in `components[]`. Milestone 164 fixes edge resolution; it does NOT collapse or dedup emitted components.

- **FR-007**: Standards-native precedence per Constitution Principle V. This milestone reuses the existing `name_to_purl` disambiguation mechanism at `scan_fs/mod.rs:519-525`. No new annotations, no new parity-catalog rows.

- **FR-008**: When a peer-dep-suffixed key form CANNOT be parsed (malformed lockfile, unexpected format drift), mikebom MUST log a `tracing::warn!` naming the malformed key and fall back to the bare-name form (pre-164 behavior). Constitution Principle VIII (Completeness) — never silently drop; never crash.

- **FR-009**: An info-level tracing log line MUST fire per pnpm-lock v9 file processed, summarizing: (a) `packages_count`, (b) `snapshots_count`, (c) `multi_version_disambiguated_count`, (d) `malformed_key_warn_count`. Grep-friendly per the milestone-157/158/159/160/161/162/163 observability convention.

- **FR-010**: Milestone-147 peer-dep handling (peerDependencies C1/C2 annotations) MUST remain unchanged. Milestone 164's scope is `dependencies:` + inline `snapshots:` resolution ONLY.

### Key Entities

- **Peer-dep-suffixed key**: pnpm-lock v9 dep-value form like `1.17.9(@algolia/client-search@5.42.0)(algoliasearch@5.42.0)(search-insights@2.17.3)`. The base version (`1.17.9`) is the concrete resolved version; parenthesized suffixes are peer-dep discriminators used by pnpm's resolver internally.

- **Version-qualified disambiguation form**: `"<name> <version>"` string used as the `name_to_purl` lookup key (per issue #262 npm precedent + milestone 087 cargo precedent). Mikebom's edge resolver already indexes by both bare name AND this disambiguation form; milestone 164 ensures the pnpm-lock parser emits into `depends` in the disambiguation form for edges that need it.

- **Multi-version orphan**: an emitted component whose PURL is `pkg:npm/<name>@<version-A>` where another `pkg:npm/<name>@<version-B>` is ALSO emitted, AND `<version-A>` has zero incoming `dependsOn` edges. Pre-164 podman-desktop baseline: 435 such orphans / 2748 total components.

## Success Criteria

### Measurable Outcomes

- **SC-001 (multi-version orphan reduction)**: On live `github.com/podman-desktop/podman-desktop` (freshly cloned), multi-version orphan count MUST drop from 435 to ≤ 30 (94% reduction).

- **SC-002 (BFS reachability improvement)**: Post-164, BFS-walking `dependencies[]` from `metadata.component` on podman-desktop MUST achieve ≥ 93% npm-component reachability. Pre-164 baseline (measured 2026-07-05): 77.4% (2126 of 2748). Target: ≥ 93% (+15pp from milestone-164 alone). Full ≥99% aspiration requires follow-on milestones addressing platform-optional-binding + truly-isolated classes.

- **SC-003 (dual-side byte-identity, mirrors milestones 158–163)**: For every milestone-090 non-pnpm-v9 golden fixture (10 non-pnpm ecosystems + the `npm` fixture which uses package-lock.json), emitted CDX / SPDX 2.3 / SPDX 3 SBOMs MUST be byte-identical to pre-164. Zero diff bytes on 11 × 3 = 33 goldens. Pnpm-lock v9 fixture goldens MAY change (deliberate — edge targets change to correct versions).

- **SC-004 (milestone-163 invariants preserved)**: Zero empty-version PURLs and zero phantom edges in ANY scan post-164. Milestone 163's SC-002 + SC-004 MUST continue to hold.

- **SC-005 (component count preserved)**: Post-164 emitted SBOM component count MUST equal pre-164 count for any given fixture. Milestone 164 fixes edge targets, not components emitted.

- **SC-006 (pre-PR gate)**: `cargo +stable clippy --workspace --all-targets -- -D warnings` and `cargo +stable test --workspace --no-fail-fast` MUST both pass with zero errors.

- **SC-007 (unit test coverage)**: The new pnpm-lock parser changes MUST have at least 8 unit tests covering: (a) peer-dep-suffixed key parses to base version; (b) parent `depends` array carries version-qualified disambiguation form; (c) `name_to_purl` disambiguation lookup returns the correct version-specific PURL; (d) pnpm v6/v7 key form unchanged (regression guard); (e) malformed peer-dep-suffixed key falls back to bare-name form with WARN log (FR-008); (f) multi-version co-existence: both versions emitted, both reachable via their respective parents; (g) FR-010 peerDependencies handling unchanged; (h) FR-005 emitted PURL never includes peer-dep suffix.

- **SC-008 (integration test)**: A new integration test at `mikebom-cli/tests/pnpm_multi_version.rs` MUST synthesize a pnpm-lock v9 fixture with two versions of the same package (both with parent entries) and assert (a) both parent edges resolve to their correct version-specific PURLs; (b) both versions are BFS-reachable from the workspace root; (c) zero multi-version orphans.

- **SC-009 (CHANGELOG entry)**: `CHANGELOG.md` MUST document the fix + the empirical pre/post numbers on podman-desktop (77.4% → ≥93%; 435 → ≤30) + a consumer jq recipe.

- **SC-010 (opportunistic real audit)**: A gated integration test at `mikebom-cli/tests/pnpm_multi_version_audit.rs` (behind `MIKEBOM_PNPM_MULTIVER_AUDIT=1`, matching milestone-160/161/162/163 pattern) MUST assert on a cached `podman-desktop` (via `MIKEBOM_FIXTURES_DIR`) that (a) multi-version orphans ≤ 30; (b) BFS reachability ≥ 93%. NOT blocking for the PR.

- **SC-011 (empirical closure)**: The impl commit MUST reference this milestone and demonstrably resolve the observed symptom (multi-version orphan count 435 → ≤ 30 on podman-desktop). Since this milestone is not tied to a specific GitHub issue number (empirical follow-up to milestone 163's podman-desktop re-measurement), the commit body MUST include the empirical measurement table and reference the 2026-07-05 baseline.

## Assumptions

- **Podman-desktop is the empirical benchmark**: SC-001/SC-002 numbers are pinned to a live clone of `github.com/podman-desktop/podman-desktop`. Numbers may drift with upstream commits; the milestone MUST re-measure at implementation time and adjust SC-001/SC-002 targets if the pre-164 baseline shifted materially (matches milestone-156/157/158 empirical-revision precedent).

- **Pnpm-lock v9 is the peer-dep-suffixed key format**: Introduced transitionally in pnpm 8.0, finalized in pnpm 9.x. Milestone 164 targets v9 specifically; pnpm 8.0 transitional format is implicitly covered iff its shape matches v9 (verified at authoring time).

- **Milestone-087 cargo pattern is directly applicable**: The `<name> <version>` disambiguation-key mechanism at `scan_fs/mod.rs:519-525` was extended for npm per issue #262 (see the inline comment block referencing the nested-`node_modules/<parent>/node_modules/<dep>` case). Milestone 164 activates that mechanism on the pnpm v9 code path — the receiver is already there.

- **Peer-dep-suffix stripping is at the pnpm-lock parser**: The existing parser at `mikebom-cli/src/scan_fs/package_db/npm/pnpm_lock.rs:400` already strips parenthesized peer-dep suffixes (per inline comment "Strip any parenthesised peer-dep suffix"). Milestone 164 preserves the stripped base-version + threads it into the `depends` array in disambiguation form.

- **No new Cargo dependencies**: Following the milestone-158/159/160/161/162/163 precedent.

- **milestone-090 pnpm fixture MAY change**: If existing pnpm-lock fixtures include multi-version cases, their goldens drift deliberately. Verified at authoring time.

- **SC-001/SC-002 targets are empirically-adjustable**: If investigation reveals corner cases that cap improvement below 93%, SC-002 may be revised inline per the milestone-156/157/158/159/160/161/162/163 empirical-revision pattern.

- **No GitHub issue number pending**: Milestone 164 is an empirical follow-up to milestone 163's podman-desktop measurement — surfaced during the 2026-07-05 audit, not filed as a separate issue at spec time. The `implements milestone 164` commit reference (rather than `closes #NNN`) matches this posture.

## Out of Scope

- **Platform-specific optional-binding orphans** (`@oxc-parser/binding-*`, `@esbuild/*-*`) — separate future milestone. `optionalDependencies:` entries where the lockfile lists ALL platform binaries but only one is loaded per host. ~50-100 orphans on podman-desktop.

- **Truly-isolated orphans not covered by the multi-version pattern** — separate follow-on. ~30-50 residual orphans need their own root-cause analysis.

- **Pnpm-lock v6/v7 key form** — unchanged (User Story 2 byte-identity guard). Pre-v8 lockfiles don't use peer-dep-suffixed keys; edges already resolve correctly via the bare-name path.

- **Yarn v1 / Yarn Berry / bun.lock** — different formats, different resolution semantics. Yarn multi-version handling is milestone 106's territory.

- **Non-npm ecosystems** — cargo multi-version disambiguation was solved by milestone 087. Other ecosystems' multi-version handling is out of scope.

- **Aliased-dep multi-version composition** — milestone 159 remains authoritative for alias name-remapping; milestone 164 handles version disambiguation on the already-resolved target name. The two compose; neither subsumes the other.

- **Peer-dep suffix as first-class SBOM data** — milestone 147 already handles peer-dep relationships via C1/C2. Milestone 164 does NOT introduce a "the peer-dep constraint context that pinned this version" annotation.

- **Retroactive rewrite of pre-164 SBOMs** — scan-time fix only; no consumer-side migration tooling.
