---
description: "Task list for milestone 164 — pnpm v9 multi-version edge disambiguation"
---

# Tasks: pnpm v9 multi-version edge disambiguation

**Input**: Design documents from `/specs/164-pnpm-multi-version-edges/`
**Prerequisites**: plan.md (required), spec.md (required), research.md, data-model.md, contracts/README.md, quickstart.md

**Tests**: INCLUDED. SC-007 requires ≥8 unit tests; SC-008 requires a new integration test. All test surfaces are load-bearing SC evidence and MUST land alongside the implementation.

**Organization**: Tasks grouped by 3 user stories from spec.md (US1 P1 MVP correct-version edge resolution, US2 P2 pnpm v6/v7 byte-identity, US3 P3 non-pnpm-v9 byte-identity). US1 is the load-bearing MVP. Total: 23 tasks (2 setup + 6 foundational + 8 US1 + 1 US2 + 2 US3 + 4 polish).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Different files, no dependencies on incomplete tasks — parallelizable.
- **[Story]**: US1 / US2 / US3 for user-story tasks. Setup, Foundational, Polish have NO story label.

## Path Conventions

Single-project workspace layout per plan.md §Project Structure. All paths absolute-relative to repo root `/Users/mlieberman/Projects/mikebom/`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Verify prerequisites + baseline current state before making changes.

- [X] T001 Verify the current pnpm-lock parser matches the research.md R1 pinpoint: `grep -n '_canon_ver' mikebom-cli/src/scan_fs/package_db/npm/pnpm_lock.rs` — MUST show line 80 discarding the version. If not present (upstream drift), abort and re-baseline research.md R1.
- [X] T002 Verify existing pnpm-lock tests baseline: `cargo +stable test --bin mikebom scan_fs::package_db::npm::pnpm_lock` — MUST pass clean before starting. Records the pre-164 test-count baseline for the T021 pre-PR gate diff.

**Checkpoint**: Codebase state confirmed. Ready for foundational implementation.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Implement the parser fix + supporting infrastructure. All user stories depend on this phase.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T003 Extend `collect_pnpm_dep_names` signature at `mikebom-cli/src/scan_fs/package_db/npm/pnpm_lock.rs:46-90` per data-model.md E1. Add 3 parameters: `emit_versioned: bool`, `versioned_counter: Option<&mut usize>`, `warn_counter: Option<&mut usize>`. Preserve return type `Vec<String>`. In the body: replace the `_canon_ver` discard with `canon_ver`; branch on `emit_versioned` — if true AND non-empty version: push `format!("{canon_name} {canon_ver}")` and increment `versioned_counter`; if true AND empty version: `tracing::warn!` per FR-008 + increment `warn_counter` + push bare `canon_name`; if false: push bare `canon_name`. Keep the existing dedup + sort at function end.
- [X] T004 Extend `build_snapshots_lookup` signature at `pnpm_lock.rs:101-126` per data-model.md E2. Add `&mut usize` counters for `versioned_counter` + `warn_counter`. Update the single `collect_pnpm_dep_names` call at line 122 to pass `emit_versioned=true, Some(versioned_counter), Some(warn_counter)`.
- [X] T005 Update the v6/v7 inline call to `collect_pnpm_dep_names` at `pnpm_lock.rs:262` per data-model.md E3 to pass `emit_versioned=false, None, None`. This preserves User Story 2 byte-identity guard for pnpm v6/v7 lockfiles.
- [X] T006 Add tally locals + extend info log at `pnpm_lock.rs` per data-model.md E4 + E5. Near the top of `parse_pnpm_lock`: `let mut multi_version_disambiguated_count: usize = 0; let mut malformed_key_warn_count: usize = 0;`. Thread these into the `build_snapshots_lookup` call. Extend the existing `tracing::info!("pnpm-lock parsed")` at `pnpm_lock.rs:373-377` with two new fields: `multi_version_disambiguated_count`, `malformed_key_warn_count`. Grep-friendly per FR-009 + milestone-157/158/159/160/161/162/163 observability convention.
- [X] T007 Update `rewrite_dep_names` in `mikebom-cli/src/scan_fs/package_db/npm/alias_mapping.rs` per data-model.md E6 + research.md R7. Split each input on FIRST space to separate `name` from optional `version`; look up `name` in `alias_map`; if match: emit `format!("{aliased_name} {version}")` (preserving version) or bare `aliased_name` (if no version); if no match: pass through unchanged. This preserves milestone-159 alias composition when milestone-164 puts versions into `depends`.
- [X] T007a Unit test `t007a_rewrite_dep_names_preserves_version` in `mikebom-cli/src/scan_fs/package_db/npm/alias_mapping.rs` mod tests block. Verify three cases per FR-003 alias-composition contract: (a) bare `"foo"` + alias-map hit `foo → @real/foo` → output `"@real/foo"` (pre-164 behavior preserved when no version); (b) versioned `"foo 1.2.3"` + same alias-map hit → output `"@real/foo 1.2.3"` (version preserved through alias substitution); (c) versioned `"baz 4.5.6"` + no alias-map hit → output `"baz 4.5.6"` unchanged (passthrough). Closes the FR-003 test gap surfaced during analyze — without this, a subtle milestone-159 regression when milestone-164 lands would only surface at podman-desktop-audit time, not at unit-test time.

**Checkpoint**: `cargo +stable check -p mikebom` succeeds. `cargo +stable clippy --workspace --all-targets -- -D warnings` clean. All existing tests continue to pass EXCEPT the pnpm v9 tests that expected pre-164 bare-name emission (Phase 3 tests will cover those).

---

## Phase 3: User Story 1 - Correct-version edge resolution on pnpm v9 (Priority: P1) 🎯 MVP

**Goal**: Verify the milestone-087-style disambiguation-key emission produces correct edge targets on pnpm v9 lockfiles with multi-version cases.

**Independent Test**: Synthesize a minimal pnpm-lock v9 fixture with two versions of the same package and two parents each declaring a different version. Assert both parent edges resolve to their correct version-specific PURLs, both versions are BFS-reachable, and zero multi-version orphans emerge.

### Tests for User Story 1

- [X] T008 [P] [US1] Unit test `t008_collect_pnpm_dep_names_emit_versioned_true_produces_versioned` in `mikebom-cli/src/scan_fs/package_db/npm/pnpm_lock.rs` mod tests block. Synthesize a minimal `serde_yaml::Mapping` with a `dependencies:` sub-mapping containing `foo: 1.2.3(peer@4.5.6)`. Call `collect_pnpm_dep_names(&tbl, &mut aliases, "/test", true, None, None)`. Assert `deps == ["foo 1.2.3"]` per SC-007 (a).
- [X] T009 [P] [US1] Unit test `t009_collect_pnpm_dep_names_emit_versioned_false_preserves_bare_name` in `pnpm_lock.rs` mod tests. Same input as T008 but `emit_versioned=false`. Assert `deps == ["foo"]` (pre-164 bare-name form) per SC-007 (b) + US2 regression guard.
- [X] T010 [P] [US1] Unit test `t010_collect_pnpm_dep_names_empty_version_falls_back_with_warn` in `pnpm_lock.rs` mod tests. **Pre-condition**: first verify `parse_pnpm_key`'s return semantics for edge inputs (`"foo@"`, `"foo"`, `"@scope/foo@"`). Two possible outcomes: **(a) `parse_pnpm_key` returns `None` on empty-version keys**: then FR-008's WARN branch fires only on parser bugs. Mark T010 as defensive-only: verify the WARN branch via direct fault injection (temporarily pass a wrapper closure that returns `Some(("foo", ""))` to the branch under test, OR extract the WARN branch logic into a helper `fn emit_dep_with_disambiguation(deps: &mut Vec<String>, name: &str, ver: &str, versioned_counter: &mut usize, warn_counter: &mut usize)` and unit-test that helper directly with `ver = ""`). Assert `deps == ["foo"]` AND `warn_counter == 1`. **(b) `parse_pnpm_key` returns `Some(("foo", ""))`**: then construct a lockfile-shaped input that triggers this and assert the full `collect_pnpm_dep_names` code path. Either way, T010 verifies the FR-008 fallback contract; the fragility framing goes away once the parse_pnpm_key behavior is nailed down.
- [X] T011 [P] [US1] Unit test `t011_build_snapshots_lookup_emits_versioned_for_v9` in `pnpm_lock.rs` mod tests. Synthesize a v9 `snapshots:` YAML section with one entry containing a dep. Call `build_snapshots_lookup` with fresh counters. Assert the returned `HashMap` values contain versioned-form strings per SC-007 (b).
- [X] T012 [P] [US1] Unit test `t012_parse_pnpm_lock_multi_version_edges_resolve_correctly` in `pnpm_lock.rs` mod tests. Synthesize a v9 pnpm-lock YAML with two versions of `foo` (`1.0.0` and `2.0.0`) + two parent packages each declaring one version via `snapshots:`. Call `parse_pnpm_lock`. Assert (a) both `PackageDbEntry` for `foo@1.0.0` and `foo@2.0.0` emitted; (b) parent 1's `depends` contains `"foo 1.0.0"`; (c) parent 2's `depends` contains `"foo 2.0.0"` per SC-007 (f).
- [X] T013 [P] [US1] Unit test `t013_parse_pnpm_lock_purl_never_includes_peer_dep_suffix` in `pnpm_lock.rs` mod tests. Synthesize a v9 pnpm-lock with a peer-dep-suffixed key like `foo@1.0.0(bar@2.0.0)`. Parse. Assert emitted `PackageDbEntry.purl.as_str()` == `"pkg:npm/foo@1.0.0"` — NEVER contains `(` per FR-005 + SC-007 (h).
- [X] T014 [P] [US1] Unit test `t014_peer_dependencies_handling_unchanged_after_164` in `pnpm_lock.rs` mod tests. Synthesize a v9 snapshot with a `peerDependencies:` block. Parse. Assert that peer-dep handling matches pre-164 behavior (existing milestone-147 semantic) per FR-010 + SC-007 (g).
- [X] T015 [US1] Integration test at `mikebom-cli/tests/pnpm_multi_version.rs` per SC-008 — synthesize a tempdir with:
  - `<tempdir>/pnpm-workspace.yaml` — declaring `packages: ['packages/*']`.
  - `<tempdir>/package.json` — root workspace declaring `name: monorepo-root`.
  - `<tempdir>/pnpm-lock.yaml` v9 — packages: `foo@1.0.0` + `foo@2.0.0` (both real registry entries); snapshots: `parent-a@1.0.0` depending on `foo: 1.0.0`, `parent-b@1.0.0` depending on `foo: 2.0.0`; importers section declaring both parents at workspace roots.
  - `<tempdir>/packages/consumer-a/package.json` — declaring `dependencies: {parent-a: "^1.0.0"}`.
  - `<tempdir>/packages/consumer-b/package.json` — declaring `dependencies: {parent-b: "^1.0.0"}`.

  Invoke the release binary via `env!("CARGO_BIN_EXE_mikebom")`, parse emitted CDX, assert per SC-008:
  1. Both `pkg:npm/foo@1.0.0` and `pkg:npm/foo@2.0.0` present in `components[]`.
  2. `pkg:npm/parent-a@1.0.0`'s `dependsOn` contains `pkg:npm/foo@1.0.0` (NOT `2.0.0`).
  3. `pkg:npm/parent-b@1.0.0`'s `dependsOn` contains `pkg:npm/foo@2.0.0` (NOT `1.0.0`).
  4. BFS from `metadata.component` reaches BOTH `foo@1.0.0` AND `foo@2.0.0` — zero multi-version orphans.
  5. **Milestone-163 invariants preserved** (SC-004): `[.components[].purl | select(test("^pkg:npm/[^@]+@$"))] | length == 0` (zero empty-version PURLs) AND `[.dependencies[].dependsOn[] | select(test("^pkg:npm/[^@]+@$"))] | length == 0` (zero phantom edges). Catches accidental milestone-163 drift at MVP-test time; without this assertion, an m164 regression breaking m163 invariants would only surface if an unrelated m163 golden test happens to fail — not guaranteed for the synthesized fixture territory.

**Checkpoint**: US1 is fully functional. Running `cargo +stable test --test pnpm_multi_version` verifies the end-to-end fix on a synthesized fixture. All 7 unit tests + 1 integration test pass.

---

## Phase 4: User Story 2 - pnpm v6/v7 lockfile format byte-identity (Priority: P2)

**Goal**: Verify pnpm v6/v7 lockfile emission is byte-identical to pre-164.

**Independent Test**: Regenerate any existing pnpm v6/v7 fixture. Diff against pre-164. Zero diff bytes.

### Tests for User Story 2

- [X] T016 [P] [US2] Unit test `t016_v6_v7_inline_path_emits_bare_names` in `pnpm_lock.rs` mod tests. Synthesize a v6/v7 pnpm-lock YAML (has `packages:` but no `snapshots:`) with a dep. Parse. Assert emitted `PackageDbEntry.depends` contains bare `["foo"]` (NOT `["foo 1.2.3"]`). Confirms FR-002 v6/v7 byte-identity path.

**Checkpoint**: US2 is fully functional. Existing pnpm v6/v7 golden tests continue passing (verified by full workspace test run in T018).

---

## Phase 5: User Story 3 - Non-pnpm-v9 scans byte-identical to pre-164 (Priority: P3)

**Goal**: Regression guard. Verify the milestone-090 non-pnpm-v9 goldens (10 non-pnpm ecosystems + the npm-with-package-lock fixture × 3 formats = 33 files) are byte-identical to pre-164.

**Independent Test**: `cargo +stable test --workspace --no-fail-fast` — every existing golden test that operates on non-pnpm-v9 fixtures MUST pass unchanged.

### Golden verification for User Story 3

- [X] T017 [US3] Run `cargo +stable test --workspace --no-fail-fast` after Phase 2+3+4 land. Every existing golden test on non-pnpm-v9 fixtures (10 non-pnpm ecosystems + `npm` fixture using `package-lock.json`) MUST pass unchanged. Any golden diff on those 33 files indicates an emission-leak bug that needs fixing before proceeding.
- [X] T018 [US3] If existing pnpm-v9 fixtures exist AND contain multi-version cases, their goldens will drift deliberately. Regenerate via `MIKEBOM_UPDATE_CDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo test` and manually inspect the diff. Verify the change is limited to: edge targets in `dependencies[].dependsOn[]` shifting from wrong-version to correct-version PURLs. NO new components, NO new annotations. If no pnpm-v9 multi-version fixture exists, this task is a no-op.

**Checkpoint**: SC-003 byte-identity verified. 33 non-pnpm-v9 goldens unchanged.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Documentation, CHANGELOG, optional real-testbed audit, pre-PR gate, empirical closure.

- [X] T019 [P] Add `CHANGELOG.md` entry per SC-009 documenting: (a) motivation (2026-07-05 podman-desktop re-measurement + root-cause pinpoint at `pnpm_lock.rs:80`), (b) fix summary (thread version through `collect_pnpm_dep_names` via `emit_versioned` param), (c) empirical impact — pre/post SC-001/SC-002 numbers on podman-desktop (77.4% → target ≥93% BFS; 435 → ≤30 multi-version orphans; measured via T015 synthesized fixture; T020 opportunistic real audit), (d) consumer jq recipe from contracts/README.md, (e) note that no new annotations, parity rows, or CLI flags were added — reuses existing `name_to_purl` mechanism per FR-007 + Constitution Principle V.
- [X] T020 Optional SC-010 real-testbed audit test at `mikebom-cli/tests/pnpm_multi_version_audit.rs` per research.md R5 — gated behind `MIKEBOM_PNPM_MULTIVER_AUDIT=1` env var. If a cached copy of `podman-desktop` is available (via `MIKEBOM_FIXTURES_DIR`), invoke the release binary + assert (a) multi-version orphans ≤ 30; (b) BFS reachability ≥ 93%. NOT blocking for the PR. Matches milestone-160 T033 + milestone-161 T040 + milestone-162 T034 + milestone-163 T037 fixture-gated audit pattern.
- [X] T021 Run `./scripts/pre-pr.sh` — both `cargo +stable clippy --workspace --all-targets -- -D warnings` and `cargo +stable test --workspace --no-fail-fast` MUST pass clean (SC-006). Any failure blocks PR opening.
- [X] T022 Include `implements milestone 164` in the impl PR body per SC-011 (milestone 164 has no upstream GitHub issue — it's an empirical follow-up to milestone 163's podman-desktop measurement). PR body should also document: (a) empirical measurement table (pre/post podman-desktop numbers), (b) SC-001+SC-002 verified via T015 synthesized fixture, (c) real podman-desktop verification is T020 opt-in (parallels milestone-160 T033 + milestone-161 T040 + milestone-162 T034 + milestone-163 T037 pattern), (d) delivers +15pp of milestone-158's ≥99% aspirational target for the npm ecosystem; remaining ~7pp gap belongs to future milestone 165 (platform-optional bindings + truly-isolated orphans).

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No prerequisites. Baseline verification.
- **Phase 2 (Foundational)**: Depends on Phase 1. Parser signature + call-site updates + alias-rewrite update + alias-rewrite unit test + FR-009 log emission. Blocks US1/US2/US3.
- **Phase 3 (US1)**: Depends on Phase 2. 7 unit tests + SC-008 integration test.
- **Phase 4 (US2)**: Depends on Phase 2 (needs T005's `emit_versioned=false` at the v6/v7 site). 1 test.
- **Phase 5 (US3)**: Depends on Phases 2+3+4. Byte-identity verification via full workspace test.
- **Phase 6 (Polish)**: Depends on Phases 1–5.

### Within Each User Story

- **US1**: T008-T014 (7 unit tests, all in `pnpm_lock.rs`, non-conflicting append-only) → T015 (integration test).
- **US2**: T016 (1 unit test).
- **US3**: T017 (test-run + inspect) → T018 (regenerate + inspect diff if applicable).

### Parallel Opportunities

Genuine `[P]` (different files, no dependencies):

- **Phase 3 T008-T014** — 7 unit tests all in the same file (`pnpm_lock.rs`) BUT non-conflicting append-only fn additions; can be authored in parallel.
- **Phase 4 T016** — 1 unit test in the same file, non-conflicting with Phase 3 tests.
- **Phase 6 T019** — CHANGELOG update in a different file from everything else.

**Foundational tasks T003-T007 are strictly sequential** — each touches `pnpm_lock.rs` or `alias_mapping.rs` and depends on the prior task's compilation state. **T007a** is parallelizable with US1 tests (different file: `alias_mapping.rs`) once T007 lands.

---

## Parallel Example: Phase 3 unit tests

```bash
# T008-T014 all append tests to the same #[cfg(test)] mod tests block in pnpm_lock.rs
# but are non-conflicting fn definitions — safe to author in parallel.
Task: "Add unit test t008_collect_pnpm_dep_names_emit_versioned_true_produces_versioned in pnpm_lock.rs"
Task: "Add unit test t009_collect_pnpm_dep_names_emit_versioned_false_preserves_bare_name in pnpm_lock.rs"
Task: "Add unit test t010_collect_pnpm_dep_names_empty_version_falls_back_with_warn in pnpm_lock.rs"
Task: "Add unit test t011_build_snapshots_lookup_emits_versioned_for_v9 in pnpm_lock.rs"
Task: "Add unit test t012_parse_pnpm_lock_multi_version_edges_resolve_correctly in pnpm_lock.rs"
Task: "Add unit test t013_parse_pnpm_lock_purl_never_includes_peer_dep_suffix in pnpm_lock.rs"
Task: "Add unit test t014_peer_dependencies_handling_unchanged_after_164 in pnpm_lock.rs"

# T015 depends on T003-T007 completing (integration test invokes the release binary).
```

---

## Implementation Strategy

### MVP scope

**US1 alone is the MVP.** Delivers the observable-bug fix (correct-version edge resolution + zero multi-version orphans) via the T015 fully-controlled synthesized fixture. US2 + US3 are regression guards.

Ship order:

1. Phase 1 (Setup) — 5 min. Baseline verification.
2. Phase 2 (Foundational) — 1 sitting. Parser + call-sites + alias rewrite + info log. Small diff (~40 lines).
3. Phase 3 (US1) — 1 sitting. 7 unit tests + integration test.
4. **STOP + VALIDATE**: Run T015 integration test. Iterate if failures.
5. Phase 4 (US2) — 15 min. 1 unit test.
6. Phase 5 (US3) — 30 min. Full workspace test + golden diff inspection.
7. Phase 6 (Polish) — 30 min. CHANGELOG + optional audit + pre-PR gate + PR body.

### Total effort

~23 tasks. Estimated 1-2 focused sessions total. Smaller than milestone 162 (34 tasks) and much smaller than milestone 163 (40 tasks).

### Empirical revision escape hatch

Per spec.md Assumption bullet, if T015/T020 investigation reveals corner cases that cap the improvement below 93%, SC-002 target may be revised inline per the milestone-156/157/158/159/160/161/162/163 empirical-revision pattern. The T015 fully-controlled synthesized fixture provides 100% BFS reachability as the achievable floor.

### Parallel team strategy

Single-contributor scope. If needed:

- Contributor A: Phase 1 → Phase 2 → Phase 3 US1 core (T015) — the load-bearing path.
- Contributor B: Phase 3 unit tests (T008-T014, in parallel with A after T003 lands) + Phase 4 US2 test + Phase 6 CHANGELOG.

---

## Notes

- All test tasks are load-bearing SC evidence (SC-007 requires ≥8 unit tests; SC-008 requires the integration test). Skipping tests fails milestone acceptance.
- No new Cargo dependencies. No new annotations. No new parity-catalog rows. No new CLI flags.
- Constitution Principle IV (`no .unwrap()` in production): all new code follows the milestone-055/091/160/161/162/163 pattern with `Option<&mut _>` counters + `Some(...)/None` matching.
- Constitution Principle V (standards-native precedence): reuses existing `name_to_purl` disambiguation mechanism at `scan_fs/mod.rs:519-525` per FR-007 + research.md R1.
- Delivers +15pp toward milestone-158's ≥99% BFS-reachability aspirational target for the npm ecosystem. Remaining ~7pp gap (platform-optional bindings + truly-isolated orphans) is out of scope; future milestone 165.
- Empirical baseline (2026-07-05) is pinned to live `github.com/podman-desktop/podman-desktop`. Numbers may drift with upstream commits; re-measure at implementation time and adjust SC-001/SC-002 targets if the pre-164 baseline shifted materially.
