# Tasks: yarn v1 + Berry optional-dependency classification (m181)

**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md) · **Research**: [research.md](./research.md) · **Data model**: [data-model.md](./data-model.md)

**Delivery slice** (per plan.md Decision 6): all 3 US ship in one PR. Single-file source change (`yarn_lock.rs`) — no cross-file coordination needed. If v1 or Berry proves surprisingly expensive at implementation time, US2 can defer to a follow-up per research.md Decision 6 fallback clause.

**Zero core-model changes** — m181 reuses m179's `LifecycleScope::Optional` + m180's C122 catalog row + m180's `peer_optional::is_peer_optional` helper verbatim. All work is inside `yarn_lock.rs`.

## Phase 1: Setup

- [X] T001 Verify current branch is `181-yarn-optional-dep` and working tree is clean at `/Users/mlieberman/Projects/mikebom` (allow the specs/181 directory as expected in-flight state); confirm base is main HEAD post-alpha.57 tag
- [X] T002 (Optional; may skip since alpha.57 CI just went green) run baseline pre-PR gate at `/Users/mlieberman/Projects/mikebom/scripts/pre-pr.sh` on the m181 base

## Phase 2: Foundational (Blocking — required by US1 + US2 + US3)

**Purpose**: extend `read_yarn_lock` + `parse_yarn_lock` to load and thread the root `package.json` through to both v1 and Berry parsers. This is the single plumbing change all three user stories consume.

- [X] T003 Extend `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/npm/yarn_lock.rs::read_yarn_lock` (line 34-44) to ALSO read `rootfs.join("package.json")` alongside `yarn.lock`; parse as `serde_json::Value` with `Value::Null` fallback on any error per FR-004 fail-safe; emit a `tracing::debug!` diagnostic when the file is missing/unparseable per contracts/yarn-classifier-extension.md. Extend `parse_yarn_lock` signature to accept `pkg_json: &serde_json::Value` as the third parameter (data-model.md §3.2). Thread the value through to `parse_v1(text, source_path, pkg_json)` and `parse_berry(text, source_path, pkg_json)` (both signatures gain the third param — see T004 and T012 for the parser-side changes). Update any existing test callers of `parse_yarn_lock` to pass `&serde_json::Value::Null` — this is backward-compatible for tests that don't exercise m181 classification.

**Foundational checkpoint**: `cargo +stable check -p mikebom` MUST pass clean after T003 (existing yarn tests must still compile). Existing yarn tests may fail at runtime because of the signature change to `parse_yarn_lock` — that's expected and will be addressed by updating callers with `&Value::Null` in T003 itself.

## Phase 3: User Story 1 — yarn v1 `optionalDependencies:` sub-block deps map to `LifecycleScope::Optional` (P1)

**Goal**: The yarn v1 parser splits its body-block accumulator (regular vs optional); components whose name appears in any parent's `optionalDependencies:` sub-block (but NOT in any other parent's regular `dependencies:` sub-block per FR-007) get `LifecycleScope::Optional` + the `mikebom:optional-derivation = "npm-optional-dependencies"` annotation. Peer-optional entries short-circuit via the m180 helper (US3-gated).

**Independent Test**: Scan the T009 yarn-v1 fixture; verify SPDX 2.3 emits `<optional-child> OPTIONAL_DEPENDENCY_OF <parent>`, CDX emits `scope: "excluded"` on it, and both formats carry the annotation. Regular runtime deps (yarn v1's `dependencies:` sub-block children) stay `lifecycle_scope: None` per SC-008.

### 3a. Reader classifier extension

- [X] T004 [US1] Extend `parse_v1` at `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/npm/yarn_lock.rs` (line 139-226) per research.md Decision 3 + contracts/yarn-classifier-extension.md: (a) add `pkg_json: &serde_json::Value` as third parameter; (b) inside the body-block loop, introduce `let mut is_optional_block = false;` companion to the existing `let mut in_deps_block = false;` — set `is_optional_block = true` (and `in_deps_block = true`) when the trimmed line is `"optionalDependencies:"`, set both booleans appropriately for `"dependencies:"`; (c) inside the `leading_ws >= 4` branch, route the dep name to a per-entry `optional_dep_names: Vec<String>` when `is_optional_block`, and to the existing `dep_names` otherwise; (d) union both accumulators into `depends` at end-of-entry (edge emission unchanged); (e) BEFORE the entry-emission loop, build TWO scan-wide sets: `optional_children_seen: HashSet<String>` (union of all entries' `optional_dep_names`) and `regular_children_seen: HashSet<String>` (union of all `dep_names`); (f) compute the final `optional_names: HashSet<String>` as `optional_children_seen - regular_children_seen` (diamond-shape rule per FR-007); (g) apply the peer-precedence guard via `optional_names.retain(|n| !crate::scan_fs::package_db::npm::peer_optional::is_peer_optional(n, pkg_json));`; (h) pass `&optional_names` to the `build_entry` call at line 255
- [X] T005 [US1] Extend `build_entry` at `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/npm/yarn_lock.rs:361` per data-model.md §3.4: (a) add `optional_names: &HashSet<String>` as fifth parameter; (b) when `optional_names.contains(name)`, set `lifecycle_scope: Some(mikebom_common::resolution::LifecycleScope::Optional)` AND insert `mikebom:optional-derivation = "npm-optional-dependencies"` into `extra_annotations`; (c) otherwise preserve pre-m181 behavior (`lifecycle_scope: None` + empty `extra_annotations`) — this is the SC-008 byte-identity guard. Update both call sites (line 119 for Berry, line 255 for v1) to pass their respective per-parser `optional_names` sets. The Berry-side change is done in T012.

### 3b. Unit tests (colocated in yarn_lock.rs `#[cfg(test)] mod tests`)

- [X] T006 [P] [US1] Add unit test `v1_optional_dep_populates_lifecycle_scope_optional` at the end of the tests module; construct a v1 yarn.lock fixture as a `&str` with a parent entry declaring `optionalDependencies:` naming `optional-child`; call `parse_yarn_lock(text, "yarn.lock", &serde_json::Value::Null)`; assert `optional-child`'s emitted `lifecycle_scope == Some(LifecycleScope::Optional)` and the annotation is present
- [X] T007 [P] [US1] Add unit test `v1_diamond_regular_wins_over_optional` in the same block; fixture where parent A declares `optionalDependencies: {shared-child}` and parent B declares `dependencies: {shared-child}`; assert `shared-child`'s `lifecycle_scope == None` (Runtime wins per FR-007)
- [X] T008 [P] [US1] Add unit test `v1_no_optional_sub_blocks_stays_none` in the same block; fixture with only `dependencies:` sub-blocks; assert every emitted component has `lifecycle_scope == None` (SC-008 regression guard)
- [X] T009 [P] [US1] Add unit test `v1_dep_only_in_regular_stays_none` in the same block; fixture where `child` appears in a regular `dependencies:` sub-block (no `optionalDependencies:` at all); assert `child.lifecycle_scope == None` (regression pin)

### 3c. Integration fixture + test

- [X] T010 [US1] Create `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/fixtures/optional_dep/yarn-v1/` with `package.json` declaring `optionalDependencies: {"fsevents": "^2"}` and a plain runtime `dependencies: {"lodash": "^4"}`; plus a HAND-AUTHORED `yarn.lock` v1 header + parent entry for the root project declaring an `optionalDependencies:` sub-block with `fsevents` and a `dependencies:` sub-block with `lodash`; plus resolved entries for both `fsevents` and `lodash` (matching m180's npm fixture shape for shared readability)
- [X] T011 [US1] Add end-to-end integration test at `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/optional_dep_yarn_v1_e2e.rs`; model after m180's `optional_dep_npm_e2e.rs::t010_npm_optional_full_mode_end_to_end`; scan the T010 fixture in Full mode (default `--spdx2-relationship-compat=full`); assert (a) `fsevents` component has CDX `scope: "excluded"` + `mikebom:optional-derivation = "npm-optional-dependencies"` property, (b) SPDX 2.3 emits `fsevents OPTIONAL_DEPENDENCY_OF` reversed-direction edge under Full mode, (c) `lodash` (runtime) has NO `scope: "excluded"` and NO derivation annotation
- [X] T011b [US1] Add basic-mode integration test `t011b_yarn_v1_optional_basic_mode_collapses` to the same integration file created in T011 (`optional_dep_yarn_v1_e2e.rs`); model after m180's `optional_dep_npm_e2e.rs::t011_npm_optional_basic_mode_collapses`; scan the T010 fixture with `--spdx2-relationship-compat=basic`; assert (a) zero `OPTIONAL_DEPENDENCY_OF` edges in emitted SPDX 2.3 (SC-006 gate), (b) fsevents CDX `scope: "excluded"` is UNCHANGED in basic mode (CDX emission is INDEPENDENT of `--spdx2-relationship-compat`), (c) the `mikebom:optional-derivation` annotation IS still present in basic mode (annotation is orthogonal to relationship-compat)

## Phase 4: User Story 2 — yarn Berry `dependenciesMeta.<name>.optional = true` maps to `LifecycleScope::Optional` (P1)

**Goal**: The yarn Berry parser cross-references the root `package.json` for `dependenciesMeta.<name>.optional = true` entries. Matching components in the Berry lockfile get `LifecycleScope::Optional` + the annotation.

**Independent Test**: Scan the T015 yarn-berry fixture; verify all three format emissions match US1's shape.

### 4a. Reader classifier extension

- [X] T012 [US2] Extend `parse_berry` at `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/npm/yarn_lock.rs:65` per research.md Decision 4 + contracts/yarn-classifier-extension.md: (a) add `pkg_json: &serde_json::Value` as third parameter; (b) BEFORE the mapping-walk loop, call a new helper `berry_optional_names_from_pkg_json(pkg_json)` that returns a `HashSet<String>` from `pkg_json["dependenciesMeta"].<name>.optional == true`; (c) apply the peer-precedence guard via `optional_names.retain(|n| !crate::scan_fs::package_db::npm::peer_optional::is_peer_optional(n, pkg_json));`; (d) pass `&optional_names` to the `build_entry` call at line 119 (extended in T005). Define the `berry_optional_names_from_pkg_json` helper at file scope OR as a nested `fn` immediately before `parse_berry` — pick whichever fits the existing structure

### 4b. Unit tests

- [X] T013 [P] [US2] Add unit test `berry_dependencies_meta_populates_lifecycle_scope_optional` at the end of the tests module; construct a Berry-format yarn.lock `&str` PLUS a `serde_json::Value` synthetic `package.json` with `dependenciesMeta: {"foo": {"optional": true}}`; call `parse_yarn_lock(text, "yarn.lock", &pkg_json)`; assert `foo.lifecycle_scope == Some(LifecycleScope::Optional)` + annotation
- [X] T014 [P] [US2] Add unit test `berry_no_dependencies_meta_stays_none` in the same block; Berry fixture + `Value::Null` package.json; assert every component has `lifecycle_scope == None` (regression pin)
- [X] T015 [P] [US2] Add unit test `berry_dependencies_meta_optional_false_stays_runtime` in the same block; Berry fixture + package.json with `dependenciesMeta: {"foo": {"optional": false}}`; assert `foo.lifecycle_scope == None` (defensive check on the `optional` field parsing)

### 4c. Integration fixture + test

- [X] T016 [US2] Create `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/fixtures/optional_dep/yarn-berry/` with `package.json` declaring `dependencies: {"lodash": "^4"}` PLUS `dependenciesMeta: {"fsevents": {"optional": true}}`; a hand-authored Berry-format `yarn.lock` with entries for both `fsevents` and `lodash`
- [X] T017 [US2] Add end-to-end integration test at `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/optional_dep_yarn_berry_e2e.rs`; model after T011; same assertions as US1 but on the Berry fixture

## Phase 5: User Story 3 — Peer-precedence guard for yarn (P1)

**Goal**: The `is_peer_optional` helper (m180) is now consumed by yarn. When a dep name appears BOTH in `optionalDependencies:` (v1) OR `dependenciesMeta` (Berry) AND in `peerDependencies + peerDependenciesMeta.<name>.optional = true` (peer-optional), the m178 `PROVIDED_DEPENDENCY_OF` classification wins.

**Independent Test**: Scan the T019 yarn peer-optional fixture; verify SPDX 2.3 emits `PROVIDED_DEPENDENCY_OF` on the react edge, NOT `OPTIONAL_DEPENDENCY_OF`; verify `mikebom:optional-derivation` does NOT appear on the react component.

### 5a. Remove the m180 dead-code marker

- [X] T018 [US3] Update `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/npm/peer_optional.rs`: REMOVE the `#[allow(dead_code)]` attribute on `is_peer_optional`; update the "Reader usage note" docstring to state that yarn v1 + Berry now consume the helper (m180 npm/pnpm short-circuit via lockfile flags as before). Verify no stray `dead_code` warnings via `cargo check -p mikebom`

### 5b. Peer-precedence unit tests

- [X] T019 [P] [US3] Add unit test `v1_peer_optional_stays_peer_not_optional` in the yarn_lock.rs tests module; v1 yarn.lock fixture where parent declares `optionalDependencies: {react}`; `pkg_json` synthetic with `peerDependencies: {react: "^18"}` + `peerDependenciesMeta: {react: {optional: true}}`; call parse; assert `react.lifecycle_scope != Some(LifecycleScope::Optional)` AND `!react.extra_annotations.contains_key("mikebom:optional-derivation")`
- [X] T020 [P] [US3] Add unit test `berry_peer_optional_stays_peer_not_optional`; Berry variant of T019

### 5c. Integration fixture + test

- [X] T021 [US3] Create `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/fixtures/optional_dep/yarn-peer-optional/` with `package.json` declaring BOTH `peerDependencies: {"react": "^18"}` + `peerDependenciesMeta: {"react": {"optional": true}}` AND `dependenciesMeta: {"react": {"optional": true}}` (Berry-style optional; also drives the guard's exit condition); the root `dependencies` field should include a NON-root helper `dependencies: {"some-lib": "1.0.0"}` so that `some-lib` becomes the parent that declares react as peer-optional (mirroring m180's peer-optional fixture shape); hand-authored Berry-format `yarn.lock` with resolved entries for react + some-lib
- [X] T022 [US3] Add end-to-end integration test at `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/optional_dep_yarn_peer_precedence.rs`; scan the T021 fixture; assert SPDX 2.3 has `react` as source of `PROVIDED_DEPENDENCY_OF` (m178 semantic fires) NOT as source of `OPTIONAL_DEPENDENCY_OF`; assert CDX react component does NOT carry `mikebom:optional-derivation`; assert react's CDX `scope` is NOT `"excluded"` (lifecycle stays Runtime/None per FR-005)

## Phase 6: Polish & Cross-Cutting Concerns

- [X] T023 Update `/Users/mlieberman/Projects/mikebom/docs/reference/reading-a-mikebom-sbom.md` — extend the m180 `mikebom:optional-derivation` subsection to note yarn v1 + Berry now emit the `"npm-optional-dependencies"` value; add a short paragraph explaining that yarn's peer-precedence guard sources from `package.json` (rather than lockfile flags like npm/pnpm) because yarn.lock doesn't carry `peer: true`
- [X] T024 Regenerate CDX 1.6 goldens: `MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo +stable test --workspace`; expect ZERO drift on all existing goldens (m181 affects no existing yarn regression fixture) per SC-004
- [X] T025 Regenerate SPDX 2.3 goldens: `MIKEBOM_UPDATE_SPDX_GOLDENS=1 cargo +stable test --workspace`; expect ZERO drift on all existing goldens; NO decrement in `*_DEPENDENCY_OF` edge counts on any fixture per SC-003
- [X] T026 Regenerate SPDX 3 goldens: `MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo +stable test --workspace`; expect ZERO drift on any fixture per SC-005
- [X] T027 Run SPDX 3 conformance validator: `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 cargo +stable test --workspace` — confirm every emitted SPDX 3 document passes `spdx3-validate==0.0.5`
- [X] T028 Run parity CI: `cargo +stable test --workspace -- parity_symmetric_equal` — confirm C122 (`mikebom:optional-derivation`) shows `SymmetricEqual` polarity for the new yarn fixtures (SC-009 gate)
- [X] T029 Run walker audit allowlist check locally: `grep -rEn "fn walk[_(]" mikebom-cli/src/scan_fs/ | sed 's/:[0-9]*:/:/' | sort -u > /tmp/walk-actual.txt && diff /tmp/walk-actual.txt mikebom-cli/src/scan_fs/walk.audit-allowlist.txt` — m181 should introduce NO new walker functions (all changes are yarn parsing, no filesystem walking)
- [X] T030 Run the mandatory pre-PR gate: `/Users/mlieberman/Projects/mikebom/scripts/pre-pr.sh` — MUST report `>>> all pre-PR checks passed.` before commit
- [X] T031 Verify the m181 quickstart: manually validate the yarn-specific consumer flow jq recipe from `/Users/mlieberman/Projects/mikebom/specs/181-yarn-optional-dep/quickstart.md` against the newly-regenerated CDX + SPDX 2.3 goldens for the T010 yarn v1 fixture; confirm the recipe returns the correct PURL

## Dependencies

- T001 → T002 → T003 (Setup + Foundational) MUST complete before any US phase.
- **Phase 3 (US1)**: T004 → T005 → T006/T007/T008/T009 (parallel — all in the same test module, so serialize commits) → T010 (fixture) → T011 (integration test requires fixture + reader changes).
- **Phase 4 (US2)**: T012 (requires T005's `build_entry` extension) → T013/T014/T015 (parallel unit tests) → T016 (fixture) → T017 (integration test).
- **Phase 5 (US3)**: T018 (dead-code marker removal; independent) → T019/T020 (parallel unit tests) → T021 (fixture) → T022 (integration test). T018 can land any time after T003; the guard usage in T004+T012 already references `peer_optional::is_peer_optional`, and the `#[allow(dead_code)]` removal is a no-op change once yarn imports the helper.
- **Phase 6 polish**: T023 (docs) can run any time after Phase 3-5 lands. T024/T025/T026 golden regens in sequence. T027/T028 parallel. T029/T030/T031 sequential pre-commit gates.

## Parallel Execution Examples

**Phase 2 setup**: `T001 → T002 → T003`.

**Phase 3+4+5 kickoff** (after T003 lands): all three user stories share `build_entry` extension (T005). Sequencing:
- T004 (parse_v1 refactor) can start in parallel with T012 (parse_berry refactor); both depend on T005's build_entry signature being agreed
- T005 (build_entry) must land first (or at least be co-committed)
- Unit tests within a phase colocate in the same test module — parallel authoring, single commit
- Fixture creation (T010/T016/T021) all in different dirs → parallel
- Integration tests (T011/T017/T022) in three separate files → parallel

**Phase 6 polish**: golden regens T024→T025→T026 in sequence; T027+T028 in parallel; T029→T030→T031 sequential pre-commit gates.

## Implementation Strategy

**MVP scope (this milestone)**: US1 + US2 + US3 + polish = 31 tasks. All ship in one PR. Zero split alternatives make sense given single-file scope + converging patterns.

**Recommended flow**:
1. Land T001-T003 (setup + package.json plumbing).
2. Land T004+T005 (parse_v1 refactor + build_entry extension) in one commit — these are intertwined via the shared build_entry signature.
3. Land T006-T009 (v1 unit tests) in one commit.
4. Land T010+T011 (v1 fixture + integration test) in one commit.
5. Land T012+T013+T014+T015 (Berry work + tests) in one commit.
6. Land T016+T017 (Berry fixture + integration test) in one commit.
7. Land T018 (dead-code marker removal — trivial cleanup).
8. Land T019+T020+T021+T022 (peer-precedence work + fixture + integration test) in one commit.
9. Polish phase.

The above cadence produces ~7-9 commits on the branch — small, reviewable slices.

**Fallback (if implementation surprises)** — per research.md Decision 6: if yarn Berry `dependenciesMeta` cross-reference reveals unexpected complexity (e.g., some Berry projects put the field somewhere non-canonical), US2 can defer to m182; US1 + US3 (US3 covers v1 case) ship in this PR. The `#[allow(dead_code)]` marker removal (T018) still lands because US3-v1 uses the helper.

## Success Criteria Coverage

| SC | Gate | Task(s) |
|----|------|---------|
| SC-001 (yarn v1 CDX/SPDX 2.3 filter parity) | US1 delivery | T011, T031 |
| SC-002 (yarn Berry CDX/SPDX 2.3 filter parity) | US2 delivery | T017 |
| SC-003 (no `*_DEPENDENCY_OF` decrement) | Existing golden regen | T025 |
| SC-004 (CDX zero-drift un-touched) | Existing golden regen | T024 |
| SC-005 (SPDX 3 zero-drift) | Existing golden regen | T026 |
| SC-006 (basic-mode zero typed edges) | Dedicated basic-mode integration test | T011b |
| SC-007 (peer-precedence preserved) | US3 delivery | T022 |
| SC-008 (m106/m159 preservation) | Regression via full test suite | T030 (pre-PR gate) |
| SC-009 (annotation byte-identity 3 formats) | Parity CI | T028 |
