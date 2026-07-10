# Tasks: npm / yarn / pnpm optional-dependency classification (m180)

**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md) · **Research**: [research.md](./research.md) · **Data model**: [data-model.md](./data-model.md)

**Delivery slice** (per plan.md Decision 4): m180 delivers all 5 user stories as one PR if code-cost stays low. US3 (yarn) may split to a follow-up PR on the same branch if its parent-child cross-reference logic proves expensive. US5 (bun) may defer to m181 if schema audit reveals unexpected complexity.

**Zero core-model changes** — m180 reuses m179's `LifecycleScope::Optional` + `RelationshipType::OptionalDependsOn` + `SpdxRelationshipType::OptionalDependencyOf` + C122 parity catalog row verbatim. All work is reader-side.

## Phase 1: Setup

- [X] T001 Verify current branch is `180-npm-optional-dep-reader` and working tree is clean at `/Users/mlieberman/Projects/mikebom` (allow the specs/180 directory as expected in-flight state); confirm base is main HEAD post-m179 merge
- [X] T002 Run baseline pre-PR gate at `/Users/mlieberman/Projects/mikebom/scripts/pre-pr.sh`; establishes the "before" baseline for SC-002 through SC-005 zero-drift gates (may skip if we know it passed post-m179; recommend confirming once)

## Phase 2: Foundational (Blocking — required by US1–US4)

**Purpose**: Add the shared reader-time `is_peer_optional` predicate helper. All four US1-US4 reader touches consume this helper; centralizing it here avoids per-reader drift.

- [X] T003 Add shared helper `is_peer_optional(entry_name: &str, parent_pkg_json: &serde_json::Value) -> bool` to `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/npm/mod.rs` (or a new sibling `peer_optional.rs` module — pick whichever the existing structure suggests); implementation per contracts/peer-precedence-guard.md — returns true iff BOTH `peerDependencies.<entry_name>` exists AND `peerDependenciesMeta.<entry_name>.optional == true`; add 4 unit tests: `is_peer_optional_true_when_both_present`, `is_peer_optional_false_when_peer_dep_missing`, `is_peer_optional_false_when_meta_missing`, `is_peer_optional_false_when_optional_flag_false`

**Foundational checkpoint**: `cargo +stable clippy --workspace --all-targets -- -D warnings` MUST pass clean after T003. The 4 unit tests MUST pass under `cargo +stable test --workspace`.

## Phase 3: User Story 1 — npm `package-lock.json` optional deps map to `LifecycleScope::Optional` (P1)

**Goal**: The npm reader classifies components with `optional: true` (and `dev: false`) as `LifecycleScope::Optional` and emits the `mikebom:optional-derivation = "npm-optional-dependencies"` annotation. Peer-optional deps stay peer-classified via the T003 guard.

**Independent Test**: Scan the T007 npm fixture; verify SPDX 2.3 emits `<opt-dep> OPTIONAL_DEPENDENCY_OF <root>`, CDX emits `scope: "excluded"` on the same, and both formats carry the `mikebom:optional-derivation` annotation.

### 3a. Reader classifier extension

- [X] T004 [US1] Extend the npm reader at `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/npm/package_lock.rs` line ~308 from `if is_dev { Development } else { Runtime }` to three-way match: `if is_dev { Development } else if is_optional && !is_peer_optional(&name, &parent_pkg_json) { Optional } else { Runtime }`; use the T003 helper; the `is_optional` bool is already computed at lines 63-66 + 97-100; ensure the parent's package.json is accessible at this site (may require plumbing depending on the existing reader structure — verify during implementation)
- [X] T005 [US1] In the same block, add `extra_annotations.insert("mikebom:optional-derivation".into(), serde_json::Value::String("npm-optional-dependencies".into()))` when the entry is classified as Optional (immediately after the T004 dispatch); position adjacent to the m147 peer-edge-targets emission at lines 278-289 for locality
- [X] T006 [P] [US1] Add unit test `npm_optional_true_populates_lifecycle_scope_optional` to `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/npm/package_lock.rs` `#[cfg(test)]` block; assert an entry with `optional: true, dev: false` gets `LifecycleScope::Optional` + the annotation
- [X] T007 [P] [US1] Add unit test `npm_dev_true_wins_over_optional` to the same test block; assert an entry with BOTH `dev: true` AND `optional: true` gets `LifecycleScope::Development` (m179 FR-015 precedence)
- [X] T008 [P] [US1] Add unit test `npm_peer_optional_stays_peer_not_optional` to the same test block; construct a synthetic parent package.json with `peerDependencies.<name>` + `peerDependenciesMeta.<name>.optional = true`; assert the target entry does NOT get `LifecycleScope::Optional` and does NOT get the annotation

### 3b. Integration fixture + test

- [X] T009 [US1] Create `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/fixtures/optional_dep/npm/` with `package.json` declaring `optionalDependencies: {"fsevents": "^2"}` (plus a regular runtime dep like `left-pad = "1"` for contrast) and a HAND-AUTHORED `package-lock.json` v3 with hydrated entries — the `packages/node_modules/fsevents` entry carries `optional: true`; do NOT run `npm install` (fixture must be static and reproducible)
- [X] T010 [US1] Add end-to-end integration test `optional_dep_npm_e2e.rs` at `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/optional_dep_npm_e2e.rs`; model after m179's `optional_dep_cargo_e2e.rs`; scan the T009 fixture; assert `fsevents` shows `scope: "excluded"` in CDX + `mikebom:optional-derivation = "npm-optional-dependencies"` property + SPDX 2.3 `OPTIONAL_DEPENDENCY_OF` reversed-direction edge; also assert `left-pad` stays Runtime (regression guard)
- [X] T011 [US1] Add basic-mode test `npm_optional_dep_basic_mode_collapses` to the same integration file; scan with `--spdx2-relationship-compat=basic`; assert zero `OPTIONAL_DEPENDENCY_OF` edges in emitted SPDX 2.3; assert annotation IS still present (orthogonal to relationship-compat)

## Phase 4: User Story 2 — pnpm `pnpm-lock.yaml` optional deps map to `LifecycleScope::Optional` (P1)

**Goal**: The pnpm reader extracts the per-entry `optional: true` marker (currently NOT parsed at all) and classifies matching components as `LifecycleScope::Optional` + emits the annotation.

**Independent Test**: Scan the T015 pnpm fixture; verify all three format emissions match US1's shape.

### 4a. Reader classifier extension

- [X] T012 [US2] Extend the pnpm reader at `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/npm/pnpm_lock.rs` around line 279 (immediately after the existing `is_dev` extraction at line 276) to add `let is_optional = tbl.get("optional").and_then(|v| v.as_bool()).unwrap_or(false);` — mirror the extraction shape m179's cargo reader used
- [X] T013 [US2] Extend the classifier at `pnpm_lock.rs:351` from `if is_dev { Development } else { Runtime }` to the same three-way match as US1 T004, using the T003 helper; if the parent's package.json is not currently accessible at this site, plumb it through the same mechanism the pnpm reader uses for other parent-scoped lookups (see the peer-dep detection at lines 33 + 655+)
- [X] T014 [US2] Add the `mikebom:optional-derivation` annotation emission in the same block, matching T005's shape
- [X] T015 [P] [US2] Add unit test `pnpm_optional_true_populates_lifecycle_scope_optional` to the pnpm reader's `#[cfg(test)]` block

### 4b. Integration fixture + test

- [X] T016 [US2] Create `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/fixtures/optional_dep/pnpm/` with `package.json` (same shape as npm fixture) + HAND-AUTHORED `pnpm-lock.yaml` v9 with an `importers.'.'.optionalDependencies` block AND a `packages.'/fsevents@2.3.3'` entry carrying `optional: true`
- [X] T017 [US2] Add end-to-end integration test `optional_dep_pnpm_e2e.rs` at `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/optional_dep_pnpm_e2e.rs`; scan the T016 fixture; assert the same shape as T010

## Phase 5: User Story 3 — yarn v1 + Berry optional deps map to `LifecycleScope::Optional` (P2)

**Goal**: The yarn reader classifies components reached via a parent's `optionalDependencies:` sub-block (v1) OR via a `dependenciesMeta.<name>.optional = true` package.json field (Berry) as `LifecycleScope::Optional`. Currently the reader emits `lifecycle_scope: None` at line 378 — this milestone plumbs the missing classification.

**Independent Test**: Scan a yarn v1 fixture + a yarn Berry fixture; verify both emit the SC-001 filter-parity shape.

### 5a. Reader classifier extension (yarn v1)

- [ ] T018 [US3] Add a pre-pass helper `build_yarn_v1_optional_set(&parsed_lockfile) -> HashSet<String>` at `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/npm/yarn_lock.rs`; walk every parent entry's `optionalDependencies:` sub-block (already parsed at line 183) and collect child names into a set; returns the set for classifier consumption
- [ ] T019 [US3] Extend the classifier at `yarn_lock.rs:378` from `lifecycle_scope: None` to a three-way match: `if optional_names.contains(&name) && !is_peer_optional(&name, &parent_pkg_json) { Some(LifecycleScope::Optional) } else { None }` — keeping None as the runtime fallback since yarn v1 doesn't classify runtime today
- [ ] T020 [US3] Add the annotation emission in the same block, matching T005's shape

### 5b. yarn Berry cross-reference

- [ ] T021 [US3] Extend the yarn Berry path in `yarn_lock.rs` (the polymorphic branch — verify site during implementation) to cross-reference `package.json`'s `dependenciesMeta.<name>.optional = true` field; augment the same optional-name set from T018
- [ ] T022 [P] [US3] Add unit tests `yarn_v1_optional_dependencies_subblock_populates_lifecycle_scope_optional` and `yarn_berry_dependencies_meta_optional_populates_lifecycle_scope_optional` to the yarn reader's `#[cfg(test)]` block

### 5c. Integration fixtures + tests

- [ ] T023 [US3] Create `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/fixtures/optional_dep/yarn-v1/` with `package.json` + hand-authored `yarn.lock` v1 having an `optionalDependencies:` sub-block on a parent entry
- [ ] T024 [US3] Create `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/fixtures/optional_dep/yarn-berry/` with `package.json` declaring `dependenciesMeta` field + hand-authored yarn Berry `yarn.lock` + minimal `.yarnrc.yml` if needed for reader recognition
- [ ] T025 [US3] Add end-to-end integration test `optional_dep_yarn_e2e.rs` at `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/optional_dep_yarn_e2e.rs`; scan both v1 and Berry fixtures; assert the same filter-parity shape

## Phase 6: User Story 4 — m178 peer-optional precedence regression guard (P1)

**Goal**: A fixture-based end-to-end test pins the m178 vs m180 peer-optional precedence rule so future changes can't silently regress it.

**Independent Test**: Scan a fixture whose `package.json` has both `peerDependencies` AND `peerDependenciesMeta.<name>.optional = true`; verify SPDX 2.3 emits `PROVIDED_DEPENDENCY_OF` (m178 wins), NOT `OPTIONAL_DEPENDENCY_OF`.

### 6a. Fixture + integration test

- [X] T026 [US4] Create `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/fixtures/optional_dep/peer-optional/` with `package.json` declaring: `peerDependencies: {"react": "^18"}` AND `peerDependenciesMeta: {"react": {"optional": true}}`; plus a hand-authored `package-lock.json` v3 with a `react` entry (the peer semantic doesn't require the peer to be installed, but for mikebom to emit the edge the lockfile must have the dep — use a minimal fixture)
- [X] T027 [US4] Add integration test `optional_dep_peer_precedence.rs` at `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/optional_dep_peer_precedence.rs`; test signature per contracts/peer-precedence-guard.md; assert (a) SPDX 2.3 has `react` as source of `PROVIDED_DEPENDENCY_OF`, (b) SPDX 2.3 does NOT have `react` as source of `OPTIONAL_DEPENDENCY_OF`, (c) CDX `react` component does NOT carry `mikebom:optional-derivation`, (d) source component carries m147's `mikebom:peer-edge-targets`
- [X] T028 [US4] Add a sibling test `optional_dep_peer_only_stays_peer` to the same file; test the case where a dep is in `peerDependencies` WITHOUT the `peerDependenciesMeta.<name>.optional` flag — assert it emits `PROVIDED_DEPENDENCY_OF` (m178 semantic unchanged, no interaction with m180)

## Phase 7: User Story 5 — bun.lock optional handling (P3, contingent)

**Goal**: The bun reader classifies bun-locked optional deps as `LifecycleScope::Optional`. **This phase is CONTINGENT** on a Phase 5-style schema audit revealing an on-disk flag that mikebom can parse without significant additional dep-parser work.

**Independent Test**: Scan the T031 bun fixture and verify the filter-parity shape.

### 7a. Schema audit + reader classifier extension

- [ ] T029 [US5] Audit bun's `bun.lock` schema — inspect real-world bun-locked projects (or bun's docs) to identify the on-disk flag that names optional deps; document findings inline as a code comment in `bun_lock.rs`; if the schema is too complex to handle with the existing bun reader, mark this phase as "deferred to m181" and skip T030-T032
- [ ] T030 [US5] Extend the bun reader at `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/npm/bun_lock.rs` lines 175 + 259 from `lifecycle_scope: None` to a three-way match matching T004's shape; add the annotation emission
- [ ] T031 [US5] Create `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/fixtures/optional_dep/bun/` with hand-authored bun-lock reflecting the schema T029 documented
- [ ] T032 [US5] Add end-to-end integration test `optional_dep_bun_e2e.rs` at `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/optional_dep_bun_e2e.rs`; scan the T031 fixture; assert the filter-parity shape

## Phase 8: Polish & Cross-Cutting Concerns

- [X] T033 Update `/Users/mlieberman/Projects/mikebom/docs/reference/reading-a-mikebom-sbom.md` — extend the m179 `mikebom:optional-derivation` subsection to list `"npm-optional-dependencies"` as the new value (single value covering npm/yarn/pnpm/bun); include a note about the peer-precedence guard preserving m178's `PROVIDED_DEPENDENCY_OF` semantic
- [X] T034 Regenerate CDX 1.6 goldens: `MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo +stable test --workspace`; review diff to confirm ADDITIVE-ONLY changes on the m180 fixtures + zero drift on existing fixtures (SC-003 gate)
- [X] T035 Regenerate SPDX 2.3 goldens: `MIKEBOM_UPDATE_SPDX_GOLDENS=1 cargo +stable test --workspace`; verify new `OPTIONAL_DEPENDENCY_OF` edges on the m180 fixtures + zero decrement in existing `*_DEPENDENCY_OF` counts on any fixture (SC-002 gate); verify m178's existing `PROVIDED_DEPENDENCY_OF` edges are unchanged (SC-008)
- [X] T036 Regenerate SPDX 3 goldens: `MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo +stable test --workspace`; verify only annotation additions on the m180 fixtures + zero drift in typed relationships / `lifecycleScope` values on any fixture (SC-004 gate)
- [X] T037 Run SPDX 3 conformance validator: `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 cargo +stable test --workspace` — confirm every emitted SPDX 3 document passes JPEWdev `spdx3-validate==0.0.5` (m078 conformance gate)
- [X] T038 Run parity CI: `cargo +stable test --workspace -- parity_symmetric_equal` — confirm C122 (`mikebom:optional-derivation`) shows `SymmetricEqual` polarity for the m180 fixtures (SC-006 gate)
- [X] T039 Run existing m178 npm regression tests: `cargo +stable test --workspace -- spdx23_peer_provided` — assert m178's peer-edge count is byte-identical pre-vs-post m180 (SC-008)
- [X] T040 Run walker audit allowlist check locally: `grep -rEn "fn walk[_(]" mikebom-cli/src/scan_fs/ | sed 's/:[0-9]*:/:/' | sort -u > /tmp/walk-actual.txt && diff /tmp/walk-actual.txt mikebom-cli/src/scan_fs/walk.audit-allowlist.txt` — m180 should introduce NO new walker functions (reader changes are all classification, not walking)
- [X] T041 Run the mandatory pre-PR gate: `/Users/mlieberman/Projects/mikebom/scripts/pre-pr.sh` — MUST report `>>> all pre-PR checks passed.` before commit
- [X] T042 Verify the m180 quickstart: manually run the consumer-flow jq recipes from `/Users/mlieberman/Projects/mikebom/specs/180-npm-optional-dep-reader/quickstart.md` against the newly-regenerated CDX + SPDX 2.3 goldens for the npm fixture; confirm both recipes return the same sorted PURL set

## Dependencies

- T001–T002 (Setup) must complete before any other work.
- T003 (Foundational shared helper) MUST complete before T004, T008, T013, T019, T027, T030 (any task using `is_peer_optional`).
- **Phase 3 (US1)**: T004 → T005 → T006/T007/T008 (parallel unit tests) → T009 (fixture) → T010 (integration test requires fixture + reader changes) → T011.
- **Phase 4 (US2)**: T012 → T013 → T014 → T015 → T016 (fixture) → T017. Independent of Phase 3 after T003 completes.
- **Phase 5 (US3)**: T018 → T019 → T020 → T021 (Berry) → T022 → T023/T024 (parallel fixtures) → T025.
- **Phase 6 (US4)**: T026 → T027 → T028. Independent of Phase 3/4/5 after T003 (uses existing m178 npm reader + Phase 3's npm classifier work, so T027 requires T004+T005 landing).
- **Phase 7 (US5) — contingent**: T029 (audit) → T030 → T031 → T032. May be deferred to m181 if T029 finds schema surprises.
- **Phase 8 polish**: T033 independent (doc). T034/T035/T036 run in sequence. T037/T038/T039 in parallel after T036. T040 independent. T041/T042 are pre-commit gates, sequential.

## Parallel Execution Examples

**Phase 2 setup**:
```
T001 → T002 → T003 (foundational shared helper)
```

**Phase 3+4+5 (US1+US2+US3 kickoff, after T003 lands)**:
```
Launch T004+T005 (US1 reader) alongside T012+T013+T014 (US2 reader)
alongside T018+T019+T020+T021 (US3 reader)
Then per-story unit test bursts:
  US1: T006, T007, T008 in parallel
  US2: T015
  US3: T022
Then fixture creation + integration tests can also parallelize:
  T009, T016, T023, T024 in parallel (fixture authoring — different dirs)
  Then T010, T017, T025 in parallel (integration tests — different files)
```

**Phase 6 US4**:
```
T026 → T027 → T028 (sequential — same file)
```

**Phase 7 US5 (contingent on T029 audit)**:
```
T029 (audit) — if positive: T030 → T031 → T032. Else: skip and defer.
```

**Phase 8 polish**:
```
T034 → T035 → T036 (golden regen sequence — one format per pass)
Then T037, T038, T039 in parallel
Then T040 → T041 → T042 (commit prep gates)
T033 can run any time after Phase 3-6 lands (doc-only)
```

## Implementation Strategy

**MVP scope (single m180 PR)**: US1 + US2 + US4 (three P1 stories) + polish = 30 tasks (T001-T017, T026-T028, T033-T042 minus US3+US5). This is the minimum for the flagship pico-continuation user value on npm+pnpm plus the peer-precedence regression guard.

**Full m180 scope (if code-cost stays low)**: MVP + US3 (T018-T025) = 38 tasks.

**Full m180 + US5 (if bun schema audit is favorable)**: Full + T029-T032 = 42 tasks.

**Recommended cadence**:
- **Single-PR bundle**: If US3 yarn plumbing turns out ≤50 LOC per reader and doesn't fight the existing structure, ship US1+US2+US3+US4 as one PR + defer US5 to m181.
- **Two-PR split**: If US3 needs >100 LOC of reader-refactor, land US1+US2+US4 first (delivers flagship value on the two most-common lockfiles + the peer-precedence guard), then land US3 as a follow-up PR on the same branch.
- **US5 defer**: Ship US5 in m181 unless T029's audit reveals a trivial code path.

**Rationale for US4 elevation to P1**: The peer-precedence guard is a subtle semantic — a silent regression (peer-optional dep incorrectly emitted as `OPTIONAL_DEPENDENCY_OF`) would go unnoticed without a specific fixture-based test. Landing US4 in the same PR as US1 ensures the m178/m180 interaction is pinned at first submission.

## Success Criteria Coverage

| SC | Gate | Task(s) |
|----|------|---------|
| SC-001 | CDX/SPDX 2.3 filter-set equality on JavaScript scans | T010 (npm), T017 (pnpm), T025 (yarn), T042 (quickstart verification) |
| SC-002 | No decrement in existing `*_DEPENDENCY_OF` counts | T035 (SPDX 2.3 regen review) |
| SC-003 | Zero drift on CDX goldens for un-touched fixtures | T034 (CDX regen review) |
| SC-004 | Zero drift on SPDX 3 goldens (typed relationships) | T036 (SPDX 3 regen review) |
| SC-005 | Basic-mode zero typed edges | T011 (npm basic mode integration test) + T035 |
| SC-006 | `mikebom:optional-derivation` byte-identity across formats | T038 (parity CI) |
| SC-007 | Peer-precedence preserved | T027 + T028 (US4 regression guard) |
| SC-008 | m178 peer-edge count preserved | T039 (m178 regression test suite) + T035 |
