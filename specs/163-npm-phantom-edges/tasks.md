---
description: "Task list for milestone 163 — npm workspace-peer phantom empty-version edges"
---

# Tasks: npm workspace-peer phantom empty-version edges

**Input**: Design documents from `/specs/163-npm-phantom-edges/`
**Prerequisites**: plan.md (required), spec.md (required), research.md, data-model.md, contracts/annotations.md, quickstart.md

**Tests**: INCLUDED. SC-007 requires ≥10 unit tests; SC-008 requires a new integration test. All test surfaces are load-bearing SC evidence and MUST land alongside the implementation.

**Organization**: Tasks grouped by the 3 user stories from spec.md (US1 P1 BFS reachability + phantom-edge elimination, US2 P2 cross-resolution mechanism, US3 P3 byte-identity guard). US1 is the load-bearing MVP.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Different files, no dependencies on incomplete tasks — parallelizable.
- **[Story]**: US1 / US2 / US3 for user-story tasks. Setup, Foundational, Polish have NO story label.

## Path Conventions

Single-project workspace layout per plan.md §Project Structure. All paths absolute-relative to repo root `/Users/mlieberman/Projects/mikebom/`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: New Rust types for the cross-workspace resolution flow.

- [X] T001 Add NEW `CrossResolution` enum with 2 variants (`Resolved { version: String }` / `Unresolved`) per data-model.md E1 in `mikebom-cli/src/scan_fs/package_db/npm/walk.rs`. Doc-comment naming Q1+Q2 unified disposition.
- [X] T002 Add NEW `CrossWorkspaceIndex` type alias (`HashMap<String, String>` mapping npm-package-name → concrete-version) per data-model.md E2 in `mikebom-cli/src/scan_fs/package_db/npm/walk.rs`.
- [X] T003 Add NEW `CrossWorkspaceContext<'a>` struct with `peer_root: &'a Path` + `index: &'a CrossWorkspaceIndex` fields per data-model.md E5 in `mikebom-cli/src/scan_fs/package_db/npm/walk.rs`.
- [X] T004 Add NEW `WorkspacePeerAccumulator` struct with `resolved_deps: Vec<String>` + `unresolved_deps: Vec<String>` fields per data-model.md E6 in `mikebom-cli/src/scan_fs/package_db/npm/walk.rs`.
- [X] T005 Verify SC-003 pre-implementation risk: run `find <milestone-090-npm-fixture>/ -name package.json -exec grep -l "dependencies\|devDependencies" {} \;` on the milestone-090 npm fixture. Non-empty result → the fixture's `package.json` files declare deps; the goldens MAY change post-163 (limited to design-tier phantom entries disappearing OR peer edges resolving to concrete PURLs). Empty result → goldens will NOT change. This is a knowledge-only task — result informs T033/T034 expectations.

**Checkpoint**: Types compile. `cargo +stable check -p mikebom` succeeds. No behavior change yet.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Cross-workspace index builder + per-peer resolver + reshaped parser + wired into `npm::read()` + parity catalog registration + docs mapping. All user stories depend on this.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T006 Implement `build_cross_workspace_index(entries: &[PackageDbEntry]) -> CrossWorkspaceIndex` per data-model.md E3 in `mikebom-cli/src/scan_fs/package_db/npm/walk.rs`. Filters on `pkg:npm/` PURL prefix + non-empty version; first-encountered wins on multi-version collision.
- [X] T007 Implement `resolve_for_workspace_peer(peer_root: &Path, dep_name: &str, index: &CrossWorkspaceIndex) -> CrossResolution` per data-model.md E4 in `mikebom-cli/src/scan_fs/package_db/npm/walk.rs`. Step 1: read `peer_root/node_modules/<dep_name>/package.json` (FR-003 closest-ancestor). Step 2: fall through to `index.get(dep_name)`. Return `Unresolved` on both misses.
- [X] T008 Reshape `parse_root_package_json(root, source_path, include_dev, cross_workspace_ctx: Option<&CrossWorkspaceContext>) -> (Vec<PackageDbEntry>, WorkspacePeerAccumulator)` per data-model.md E5 in `mikebom-cli/src/scan_fs/package_db/npm/walk.rs`. When `cross_workspace_ctx` is `None`, preserve pre-163 behavior (backward compat). When `Some(_)`, call `resolve_for_workspace_peer` per dep: `Resolved` → accumulate name in `accumulator.resolved_deps`; `Unresolved` → accumulate name in `accumulator.unresolved_deps`. **Do NOT emit design-tier phantom entries when `cross_workspace_ctx.is_some()`.**
- [X] T009 Update the `read_root_package_json` caller in `walk.rs` to accept + thread the `cross_workspace_ctx` through to `parse_root_package_json`. Return type extends to `Option<(Vec<PackageDbEntry>, WorkspacePeerAccumulator)>`.
- [X] T010 Update EVERY existing test-site call to `parse_root_package_json` in `walk.rs` to pass `None` as the new 4th argument (backward-compat preservation for standalone-package.json unit tests). Pre-count: `grep -c 'parse_root_package_json(' mikebom-cli/src/scan_fs/package_db/npm/walk.rs` — expect ~3-6 sites.
- [X] T011 Wire cross-workspace index construction into `npm::read()` at `mikebom-cli/src/scan_fs/package_db/npm/mod.rs` after ALL project roots complete Tier A + Tier B + Tier C. Build `let cross_workspace_index = walk::build_cross_workspace_index(&entries);` — anchor the index off the collected `entries` vector so ALL lockfile-emitted entries feed the index.
- [X] T012 Update the workspace-peer Tier C invocation in `npm::read()` to pass `Some(&CrossWorkspaceContext { peer_root: &project_root, index: &cross_workspace_index })` when the caller is a workspace peer, `None` otherwise. **Distinguish via this rule**: a project root is a workspace peer iff (a) it has a `package.json`, AND (b) NO lockfile file (`package-lock.json`, `pnpm-lock.yaml`, `yarn.lock`, `bun.lock`) exists at that root. When peer: pass `Some(_)`. When standalone (any lockfile alongside `package.json`): pass `None` — preserves pre-163 design-tier phantom emission for backward-compatible standalone package.json scans.
- [X] T013 Stamp the peer's main-module component (emitted by milestone-066's main-module logic) with (a) `depends` extended by the `WorkspacePeerAccumulator.resolved_deps`; (b) `mikebom:unresolved-declared-dep` annotation on `extra_annotations` when `WorkspacePeerAccumulator.unresolved_deps` is non-empty (bare string for length 1; JSON array for length ≥ 2, sorted + deduplicated). Emit FR-009 info-level tracing log `"npm workspace-peer cross-resolution summary"` with fields `workspace_root`, `resolved_count`, `phantom_prevented_count` (equal to `resolved_deps.len() + unresolved_deps.len()`), `unresolved_declared_count`.
- [X] T014 [P] Register C115 (`mikebom:unresolved-declared-dep`, component) `cdx_anno!` invocation per contracts/annotations.md §Parity catalog integration in `mikebom-cli/src/parity/extractors/cdx.rs`.
- [X] T015 [P] Register C115 `spdx23_anno!` invocation in `mikebom-cli/src/parity/extractors/spdx2.rs`.
- [X] T016 [P] Register C115 `spdx3_anno!` invocation in `mikebom-cli/src/parity/extractors/spdx3.rs`.
- [X] T017 Add 1 `ParityExtractor` entry (C115 with `Directionality::SymmetricEqual`, `order_sensitive: false`) adjacent to the existing C114 block in `mikebom-cli/src/parity/extractors/mod.rs` AND add `c115_cdx`/`c115_spdx23`/`c115_spdx3` to the 3 import lines.
- [X] T018 Add C115 row to `docs/reference/sbom-format-mapping.md` per contracts/annotations.md §C115 wire format — needed for `every_mikebom_emitted_field_has_a_map_row` test coverage.

**Checkpoint**: `cargo +stable clippy --workspace --all-targets -- -D warnings` clean. Cross-resolution flow wired but not yet verified end-to-end (tests in Phase 3).

---

## Phase 3: User Story 1 - BFS reachability jumps to ≥99% via phantom-edge elimination (Priority: P1) 🎯 MVP

**Goal**: Verify the cross-resolution flow produces zero empty-version PURLs, zero phantom edges, and enables ≥99% BFS reachability. Sample fixture (synthesized multi-workspace monorepo) fully exercises the mechanism.

**Independent Test**: Scan a synthesized multi-workspace monorepo (via T028 integration test). Assert:

- Zero components with PURL matching `^pkg:npm/[^@]+@$` (empty-version regex) (SC-004).
- Zero edges in `dependencies[].dependsOn[]` matching the same regex (SC-002).
- ≥ 99% BFS reachability from `metadata.component` (SC-001, on the synthesized fixture: 100% reachable).
- Every workspace peer's declared dep with a lockfile-resolved version produces a REAL edge to the concrete-version PURL.
- Unresolvable declared deps produce `mikebom:unresolved-declared-dep` annotations on the peer's main-module component.

### Tests for User Story 1

- [X] T019 [P] [US1] Unit test: `build_cross_workspace_index` builds a name → version map from a synthesized entries vector containing 1 `pkg:npm/@docusaurus/core@3.10.1` + 1 `pkg:npm/thor@1.4.0` — assert both are indexed per SC-007 in `mikebom-cli/src/scan_fs/package_db/npm/walk.rs`.
- [X] T020 [P] [US1] Unit test: `build_cross_workspace_index` SKIPS design-tier entries (empty version). Synthesize an entry vector with 1 real + 1 empty-version — assert only the real one is indexed per T004 empty-version-filter clause.
- [X] T021 [P] [US1] Unit test: `resolve_for_workspace_peer` returns `Resolved { version }` when the dep is in the cross-workspace index. Synthesize a fresh tempdir peer_root (no nested node_modules), a `CrossWorkspaceIndex` with `("@docusaurus/core", "3.10.1")`, and call the resolver for `@docusaurus/core`. Assert `Resolved { version: "3.10.1" }` per SC-007 (a) in `walk.rs`.
- [X] T022 [P] [US1] Unit test: `resolve_for_workspace_peer` returns `Unresolved` when the dep is NOT in the cross-workspace index. Synthesize a peer_root (no nested node_modules), an empty `CrossWorkspaceIndex`, and call the resolver for `@some/missing`. Assert `Unresolved` per SC-007 (c) in `walk.rs`.
- [X] T023 [P] [US1] Unit test: `resolve_for_workspace_peer` FR-003 closest-ancestor — when peer_root has its own `node_modules/<dep_name>/package.json` with a different version than the cross-workspace index, the nested version wins. Synthesize a tempdir with `peer_root/node_modules/foo/package.json` containing `{"version": "2.0.0"}`, a `CrossWorkspaceIndex` with `("foo", "1.0.0")`. Assert `Resolved { version: "2.0.0" }` per SC-007 (b) + FR-003 in `walk.rs`.
- [X] T024 [P] [US1] Unit test: reshaped `parse_root_package_json` — when `cross_workspace_ctx` is `Some(_)` AND the declared dep resolves, ZERO design-tier phantom entries are appended to the returned `Vec<PackageDbEntry>` AND the dep-name IS accumulated in `WorkspacePeerAccumulator.resolved_deps` per SC-007 (d) + (f) in `walk.rs`.
- [X] T024a [P] [US1] Unit test: reshaped `parse_root_package_json` devDependencies coverage — synthesize a peer whose `package.json` has a devDependencies-declared dep (only present in `devDependencies`, NOT in `dependencies`). Call `parse_root_package_json(_, _, include_dev = true, Some(&ctx))`. Assert the devDep name is accumulated into `WorkspacePeerAccumulator.resolved_deps` when the cross-workspace index has the name, and into `WorkspacePeerAccumulator.unresolved_deps` when it doesn't. Confirms SC-007 sub-item (i): devDependencies get the same cross-resolution treatment as dependencies. In `walk.rs`.
- [X] T025 [P] [US1] Unit test: reshaped `parse_root_package_json` — when `cross_workspace_ctx` is `Some(_)` AND the declared dep does NOT resolve, ZERO design-tier phantom entries are appended AND the dep-name IS accumulated in `WorkspacePeerAccumulator.unresolved_deps` per SC-007 (e) + (g) in `walk.rs`.
- [X] T026 [P] [US1] Unit test: reshaped `parse_root_package_json` backward compat — when `cross_workspace_ctx` is `None`, pre-163 behavior preserved: design-tier phantom entries emitted with empty version + `requirement_range` populated per T008 backward-compat clause in `walk.rs`.
- [X] T027 [P] [US1] Unit test: `WorkspacePeerAccumulator.unresolved_deps` multi-value shape — 1 unresolved dep → annotation value is bare String; 2+ unresolved deps → annotation value is JSON Array (sorted + deduplicated) per contracts/annotations.md C115 wire format in `walk.rs`.
- [X] T028 [US1] Integration test at `mikebom-cli/tests/npm_phantom_edges.rs` per SC-008 — synthesize a tempdir with a multi-workspace monorepo shape:
  - `<tempdir>/package.json` — workspace root, declaring `"workspaces": ["packages/*"]`.
  - `<tempdir>/package-lock.json` — v3 lockfile pinning `@docusaurus/core@3.10.1` AND `thor@1.4.0`.
  - `<tempdir>/packages/docs/package.json` — workspace peer declaring `"@docusaurus/core": "^3.10.1"` (resolvable) + `"@some/removed": "^1.0.0"` (unresolvable — NOT in the lockfile).
  - `<tempdir>/packages/renderer/package.json` — workspace peer declaring `"thor": "^1.0.0"` (resolvable via the top-level lockfile).
  Invoke the release binary via `env!("CARGO_BIN_EXE_mikebom")`, parse emitted CDX, assert:
  1. Zero components with PURL matching `^pkg:npm/[^@]+@$` (empty-version regex).
  2. Zero edges in `dependencies[].dependsOn[]` matching the same regex.
  3. `pkg:npm/docs@0.0.0` has a `dependsOn` edge to `pkg:npm/%40docusaurus/core@3.10.1` (concrete version).
  4. `pkg:npm/docs@0.0.0` carries `mikebom:unresolved-declared-dep = "@some/removed"` annotation.
  5. `pkg:npm/renderer@0.0.0` has a `dependsOn` edge to `pkg:npm/thor@1.4.0`.
  6. Total npm component count remains stable (2 workspace peers + 2 lockfile entries = 4 real npm components); ALL 2 lockfile-resolved-version components (`@docusaurus/core@3.10.1` + `thor@1.4.0`) are present in `components[]` — no RESOLVED component dropped.
  7. BFS from `metadata.component` reaches 100% of npm components.

**Checkpoint**: US1 is fully functional. Running `cargo +stable test --test npm_phantom_edges` verifies end-to-end that zero phantom edges are emitted + workspace-peer edges cross-resolve + unresolved declared deps produce C115 annotations. All 10 unit tests pass.

---

## Phase 4: User Story 2 - Cross-resolution mechanism (Priority: P2)

**Goal**: Verify the SC-005 coverage-preservation invariant + FR-010 peer-dep regression guard + FR-003 nested-preferred semantics all hold.

**Independent Test**: Verify against the synthesized fixture from T028 + against an alternate synthesized fixture with nested node_modules.

### Tests for User Story 2

- [X] T029 [P] [US2] Unit test: SC-005 coverage-preservation — after `build_cross_workspace_index` runs on a mixed vector (real + design-tier), the total entries count in `entries` remains unchanged (index construction does NOT drop entries) per SC-005 in `mikebom-cli/src/scan_fs/package_db/npm/walk.rs`.
- [X] T030 [P] [US2] Unit test: FR-010 peer-dep regression guard — `peerDependencies:` block in a workspace peer's `package.json` is NOT subject to the FR-001 cross-resolution rewrite. Synthesize a peer's `package.json` with `peerDependencies:` + `dependencies:`; call the reshaped parser; assert the `peerDependencies:` names are NOT accumulated in the `resolved_deps` / `unresolved_deps` sidecar (per data-model.md §Emission conditions, the reshape scope is FR-005 dependencies + devDependencies only) in `walk.rs`.
- [X] T031 [US2] Integration test at `mikebom-cli/tests/npm_phantom_edges.rs` — extend T028 with an additional workspace peer that has its own `node_modules/foo/package.json` at version 2.0.0 (top-level lockfile pins `foo@1.0.0`). Assert the peer's edge targets `pkg:npm/foo@2.0.0` (nested wins per FR-003).
- [X] T032 [US2] Unit test: FR-005 lockfile-format-agnostic behavior — `build_cross_workspace_index` operates on `&[PackageDbEntry]` regardless of which lockfile format produced the entries. Synthesize a `Vec<PackageDbEntry>` with entries whose `source_type` field varies across `package-lock.json` / `pnpm-lock.yaml` / `yarn.lock v1` / `bun.lock` provenance; call `build_cross_workspace_index`; assert every non-empty-version entry is indexed regardless of provenance. (Per-format lockfile PARSING is milestone-055/106 territory and already tested there; this test only guards the index-builder's agnostic-consumer behavior.) In `walk.rs`.

**Checkpoint**: US2 is fully functional. All 4 tests pass.

---

## Phase 5: User Story 3 - Non-npm scans byte-identical to pre-163 (Priority: P3)

**Goal**: Regression guard. Verify the milestone-090 non-`npm` goldens (10 ecosystems × 3 formats = 30 files) are byte-identical to pre-163.

**Independent Test**: `git diff <pre-163-sha> HEAD -- mikebom-cli/tests/fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/{apk,bazel,cargo,cmake,deb,gem,golang,maven,pip,rpm}.*` produces zero output.

### Golden verification for User Story 3

- [X] T033 [US3] Run `cargo +stable test --workspace --no-fail-fast` after Phase 3+4 land. Inspect the diff for any golden that changed. Expected: zero changes on the 10 non-`npm` goldens. The `npm` fixture golden MAY change per T005's finding — if changed, regenerate via `MIKEBOM_UPDATE_CDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo test` and manually inspect the diff. Verify the change is limited to: (a) design-tier phantom entries disappearing from `components[]` array; (b) workspace-peer `dependsOn` list pointing to concrete-version PURLs instead of empty-version PURLs; NO new components, NO new annotations outside the C115 case.
- [X] T034 [US3] Verify SC-003 dual-side byte-identity: `git diff HEAD~ -- mikebom-cli/tests/fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/{apk,bazel,cargo,cmake,deb,gem,golang,maven,pip,rpm}.*` produces exactly ZERO changed lines. Any diff on those 30 goldens indicates an emission-leak bug that needs fixing before proceeding.

**Checkpoint**: SC-003 byte-identity verified. `npm` fixture may or may not change depending on its `package.json` structure per T005 finding.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Documentation, CHANGELOG, pre-PR gate, optional test-podman-desktop audit, issue closure.

- [X] T035 [P] Add `CHANGELOG.md` entry per SC-009 documenting: (a) motivation (issue #498 + milestone-158 audit), (b) fix summary (cross-workspace resolution index + Tier C reshape + `mikebom:unresolved-declared-dep` annotation), (c) new annotation vocab table (C115), (d) empirical impact — pre/post SC-001 numbers on test-podman-desktop (24.6% → target ≥99%; measured via T028 fully-controlled fixture; T037 opportunistic real audit), (e) consumer jq recipe from contracts/annotations.md, (f) Q1+Q2 unified disposition rule.
- [X] T036 [P] Verify T018 `docs/reference/sbom-format-mapping.md` C115 row matches the final wire shape after implementation — no-op if T018 already captured the correct shape.
- [ ] T037 Optional SC-001 audit test at `mikebom-cli/tests/npm_phantom_edges_audit.rs` per research.md R5 — gated behind `MIKEBOM_NPM_PHANTOM_AUDIT=1` env var. If a cached copy of `test-podman-desktop` is available (via `MIKEBOM_FIXTURES_DIR`), invoke the release binary + assert (a) zero empty-version PURLs; (b) zero phantom edges; (c) BFS reachability ≥ 99%; (d) npm component count ≥ 2835. This is OPPORTUNISTIC — not blocking for the PR. Matches milestone-160 T033 + milestone-161 T040 + milestone-162 T034 fixture-gated audit pattern.
- [X] T038 Run `./scripts/pre-pr.sh` — both `cargo +stable clippy --workspace --all-targets -- -D warnings` and `cargo +stable test --workspace --no-fail-fast` MUST pass clean (SC-006). Any failure blocks PR opening.
- [X] T039 Include `closes #498` in the impl PR body per SC-011 so merging the PR auto-closes the tracking issue. PR body should also document: (a) SC-001 verified via T028 synthesized fixture (100% BFS); (b) real test-podman-desktop verification is T037 opt-in (parallels milestone-160 T033 + milestone-161 T040 + milestone-162 T034 pattern); (c) delivers milestone-158's ≥99% aspirational target for the npm ecosystem.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No prerequisites. Types land first.
- **Phase 2 (Foundational)**: Depends on Phase 1. Resolution logic + parser reshape + wiring + parity catalog. Blocks US1/US2/US3.
- **Phase 3 (US1)**: Depends on Phase 2. 10 unit tests + SC-008 integration test.
- **Phase 4 (US2)**: Depends on Phase 3 (needs the reshape wired). 4 tests.
- **Phase 5 (US3)**: Depends on Phase 3+4 completion. Byte-identity verification.
- **Phase 6 (Polish)**: Depends on Phases 1–5 completion.

### Within Each User Story

- **US1**: T019-T027 + T024a (10 unit tests, mostly parallel) → T028 (integration test).
- **US2**: T029-T030 (unit tests, parallel) → T031 (extend T028 integration) → T032 (index-builder agnostic-consumer test).
- **US3**: T033 (test-run + inspect) → T034 (byte-identity assertion).

### Parallel Opportunities

Genuine `[P]` (different files, no dependencies):

- **Phase 2 T014/T015/T016** — parity registration across 3 different files (cdx.rs, spdx2.rs, spdx3.rs).
- **Phase 3 T019-T027 + T024a** — 10 unit tests all in the same file (`walk.rs`) BUT non-conflicting append-only fn additions; can be authored in parallel.
- **Phase 4 T029-T030 + T032** — 3 unit tests in the same file, non-conflicting.
- **Phase 6 T035/T036** — CHANGELOG + docs updates in different files.

---

## Parallel Example: Phase 2 parity registration

```bash
# T014 + T015 + T016 all edit DIFFERENT files:
Task: "Register C115 cdx_anno! invocation in mikebom-cli/src/parity/extractors/cdx.rs"
Task: "Register C115 spdx23_anno! invocation in mikebom-cli/src/parity/extractors/spdx2.rs"
Task: "Register C115 spdx3_anno! invocation in mikebom-cli/src/parity/extractors/spdx3.rs"

# T017 depends on T014-T016 completing (mod.rs registration references the extractor fns).
```

---

## Implementation Strategy

### MVP scope

**US1 alone is the MVP.** Delivers the observable-bug fix (zero phantom edges + real cross-resolution) via the T028 fully-controlled synthesized fixture. US2's coverage + FR-010 + FR-003 tests are P2 verification; US3 is a regression guard.

Ship order:

1. Phase 1 (Setup) — 1 sitting. Types.
2. Phase 2 (Foundational) — 1-2 sittings. Resolution logic + parser reshape + wiring + parity catalog.
3. Phase 3 (US1) — 1-2 sittings. 10 unit tests + integration test.
4. **STOP + VALIDATE**: Run T028 integration test. Iterate if failures.
5. Phase 4 (US2) — 1 sitting.
6. Phase 5 (US3) — 1 sitting. Should be trivially green — SC-003 zero-diff on 10 non-npm goldens.
7. Phase 6 (Polish) — 1 sitting.

### Total effort

~40 tasks. Estimated 3-4 focused sessions total. Matches milestone-162's pattern — the smallest of the milestone-160/161/162/163 series.

### Empirical revision escape hatch

Per spec.md Assumptions §7, if T028 investigation reveals corner cases (e.g., legitimate cross-workspace edges that BFS can't traverse), SC-001 target may be revised inline per the milestone-156–162 empirical-revision pattern. Fully-controlled synthesized fixture in T028 provides 100% BFS reachability as the achievable floor.

### Parallel team strategy

With 2 contributors:

- Contributor A: Phase 1 → Phase 2 → Phase 3 US1 core (T028) — the load-bearing path.
- Contributor B: Phase 3 unit tests (T019-T027 + T024a, in parallel with A) + Phase 4 US2 tests + Phase 6 docs.

---

## Notes

- All test tasks are load-bearing SC evidence (SC-007 requires ≥10 unit tests; SC-008 requires the integration test). Skipping tests fails the milestone acceptance.
- Unlike milestones 160 + 161, NO empirical investigation loop is needed — the fix mechanism is fully specified at plan time.
- Preserve milestone-051's dev-scope classification + milestone-066 main-module emission unchanged — those are anchor points the reshape builds ON, not touches.
- Constitution Principle IV (`no .unwrap()` in production): all new code follows the milestone-055/091/160/161/162 pattern with `anyhow::Result` + `?` propagation.
- No new Cargo dependencies. `HashMap<String, String>` + std filesystem I/O only.
- Delivers milestone-158's ≥99% BFS-reachability aspirational target for the npm ecosystem.
