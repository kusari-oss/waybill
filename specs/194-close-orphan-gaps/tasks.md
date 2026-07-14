---
description: "Task list for m194 — Close Remaining Graph-Completeness Orphan Gaps (Go stdlib edge + npm nested mainmod)"
---

# Tasks: Close Remaining Graph-Completeness Orphan Gaps

**Input**: Design documents from `/specs/194-close-orphan-gaps/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/emission-shape.md, quickstart.md

**Tests**: Included — mikebom's standard integration-test-plus-unit-test pattern (matches m190/m191/m192/m193).

**Organization**: Two P1 user stories both closing orphan classes in the graph-completeness signal. Independent code paths (Go reader vs npm reader) → can proceed in parallel.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel
- **[Story]**: US1 (Go stdlib edge) or US2 (nameless nested npm mainmod)
- File paths absolute where non-obvious; workspace root is `/Users/mlieberman/Projects/mikebom/`

## Path Conventions

- **mikebom-cli crate**: `mikebom-cli/src/...`, `mikebom-cli/tests/...`
- **Feature spec dir**: `specs/194-close-orphan-gaps/...`

---

## Phase 1: Setup

**Purpose**: Verify workspace baseline is clean.

- [X] T001 Confirm `194-close-orphan-gaps` branch is checked out and clean (`git status` shows only the specs/ dir untracked; CLAUDE.md may be modified by /speckit-plan). Baseline pre-PR deferred to T017 — no Rust changes yet.

**Checkpoint**: Baseline recorded.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Golden drift audit — identify which existing goldens currently exhibit either the stdlib-orphan class (US1) or the nested-nameless-npm-orphan class (US2). Drives Phase 5 golden regen decision.

⚠️ **CRITICAL**: Must complete before Phase 3/4 impl so byte-identity gate (FR-012) is scoped correctly.

- [X] T002 Audit CDX goldens for `pkg:golang/stdlib` component presence: `grep -rEln '"purl":\s*"pkg:golang/stdlib@' mikebom-cli/tests/fixtures/`. Record hits in `specs/194-close-orphan-gaps/scratch/drift-set.txt`. Every hit = US1 drift (will gain a new dependsOn edge from Go mainmod → stdlib).
- [X] T003 [P] Audit CDX goldens for nested nameless-package.json fixtures: grep the fixtures directory for `package.json` files without a `"name"` field inside a directory that also has `package-lock.json`. Record hits in same scratch file. Every hit = US2 drift (will gain a new synthesized mainmod component + edges).
- [X] T004 [P] Verify `apply_main_module_drop_or_demote` at `mikebom-cli/src/generate/root_selector.rs:528` handles multi-mainmod drops correctly per Q2 answer B. Read the function; confirm it iterates ALL components with `mikebom:component-role: main-module` (not just the first). If it only drops the top-level, add a Phase 5 task to extend it.
- [X] T005 [P] Verify the name → PURL Relationship emission at `mikebom-cli/src/scan_fs/mod.rs:756-772` disambiguates when multiple `pkg:golang/stdlib@vX` entries exist under the same name `"stdlib"`. If ambiguous (multi-binary Go image scans), extend it OR fall back to direct `Relationship` emission for the stdlib case. Record finding in scratch.

**Checkpoint**: Drift set + edge-emission behavior verified.

---

## Phase 3: User Story 1 — Go stdlib synthetic edge (Priority: P1)

**Goal**: Every emitted `pkg:golang/stdlib@v*` component becomes reachable from the primary Go mainmod via a synthetic DependsOn edge. When no other orphans exist, `mikebom:graph-completeness` reports `complete`. Closes issue #571.

**Independent Test**: Scan a Go source repo with `go.mod` + `main.go`; assert `.dependencies[?(@.ref==<Go mainmod>)].dependsOn` includes `pkg:golang/stdlib@v*`, AND `mikebom:graph-completeness == "complete"` (assuming no other orphans).

### Tests for User Story 1

> **Write tests FIRST; ensure they FAIL against pre-m194 tree before implementation.**

- [X] T006 [P] [US1] Add unit tests to `mikebom-cli/src/scan_fs/package_db/golang/legacy.rs::tests` covering the stdlib-edge synthesis. Cover:
  - Single Go mainmod + single stdlib entry → mainmod's `.depends` includes `"stdlib"` after synthesis
  - No Go mainmod (defensive) → no crash; no synthesis fires
  - Multi-mainmod (2 Go binaries, same Go version) → BOTH mainmods' `.depends` include `"stdlib"`
  - Byte-identity guard: on a Go source scan with NO stdlib emission (edge case), the `.depends` list is unchanged.

### Implementation for User Story 1

- [X] T007 [US1] In `mikebom-cli/src/scan_fs/package_db/golang/legacy.rs` at the `build_stdlib_entry` call site (~line 2256), immediately after `entries.push(entry)` inside the `emitted_versions.insert(...)` branch, add a loop that walks `entries` to find the Go mainmod component matching the current `project_root` (compare `source_path == format!("path+file://{}", project_root.display())` AND `extra_annotations.get("mikebom:component-role") == Some("main-module")` AND `purl.ecosystem() == "golang"`), then appends `"stdlib"` to its `.depends` list if not already present. Per data-model.md pseudocode. Add doc comment referencing m194 US1 + FR-001.
- [X] T008 [US1] Run T006 unit tests: `cargo test -p mikebom --bin mikebom scan_fs::package_db::golang::legacy::tests` — MUST pass.

**Checkpoint**: Every Go source scan with detected mainmod + stdlib emission has the synthetic edge in place. US1 acceptance scenarios pass.

---

## Phase 4: User Story 2 — Nameless nested npm workspace mainmod (Priority: P1)

**Goal**: Nested nameless `package.json` + `package-lock.json` pairs get a synthesized mainmod component (versionless PURL `pkg:npm/<dir-basename>` + `mikebom:component-role: main-module`), so BFS from the classifier's roots reaches the nested workspace's transitive components. Closes issue #572.

**Independent Test**: Scan a project with a nested nameless `package.json` (no `"name"` field) + sibling `package-lock.json`; assert a `pkg:npm/<nested-dir>` component is emitted with `mikebom:component-role: main-module`, AND its `.dependsOn` includes the lockfile's transitive contents, AND `mikebom:graph-completeness == "complete"`.

### Tests for User Story 2

- [X] T009 [P] [US2] Add unit tests to `mikebom-cli/src/scan_fs/package_db/npm/mod.rs::tests` covering the new `synthesize_nameless_nested_mainmods` function. Cover:
  - Fixture with a nested nameless `package.json` under a discovered project root → synthesized mainmod emitted with versionless PURL + main-module role + declared deps in `.depends`
  - Fixture with a NAMED nested `package.json` → NOT synthesized (m066 already handled it; no double-emission)
  - Fixture with top-level `package.json` only (no nested) → NOT synthesized (no-op; byte-identity)
  - Nameless `package.json` with EMPTY dependencies section → NOT synthesized (nothing to root; skip per data-model.md)
  - Multiple nameless nested workspaces → EACH gets its own synthesized mainmod (PURLs disambiguated by dir-basename)

### Implementation for User Story 2

- [X] T010 [US2] In `mikebom-cli/src/scan_fs/package_db/npm/mod.rs`, add a new function `synthesize_nameless_nested_mainmods(rootfs, include_dev, entries, exclude_set)` per data-model.md pseudocode. Place it adjacent to the existing `apply_nameless_secondary_umbrella` (line 361+). Its logic: iterate `candidate_project_roots`; for each root, skip if a mainmod already exists for that dir; read `package.json`; skip if `.name` is present; collect declared dep names from `.dependencies` + optional/dev sections; skip if empty; synthesize a `PackageDbEntry` with versionless PURL `pkg:npm/<sanitized-basename>`, `mikebom:component-role: main-module`, `sbom_tier: source`, and `.depends = <collected names>`; push to `entries`; increment `synthesized_count`. Emit one `tracing::info!` at end when `synthesized_count > 0`.
- [X] T011 [US2] Call the new function from `mikebom-cli/src/scan_fs/package_db/npm/mod.rs::read` immediately AFTER `apply_nameless_secondary_umbrella(rootfs, include_dev, &mut entries, exclude_set);` at line 302, and BEFORE `walk::dedup_npm_main_modules_by_purl` at line 308.
- [X] T012 [US2] Handle sanitization of `<dir-basename>` per purl-spec type charset (`^[a-z][a-z0-9.+-]*$` for the type, but the NAME segment has different rules — needs to be URL-encoded via `encode_purl_segment` from `mikebom_common::types::purl`). If a directory name contains characters that can't be URL-encoded into a valid npm name, skip synthesis and emit a `tracing::warn!` with the reason.
- [X] T013 [US2] Run T009 unit tests: `cargo test -p mikebom --bin mikebom scan_fs::package_db::npm::tests` — MUST pass.

**Checkpoint**: Every nameless nested npm workspace has a mainmod component and reachable transitives. US2 acceptance scenarios pass.

---

## Phase 5: Polish & Cross-Cutting Concerns

**Purpose**: Integration tests, golden regen (per T002/T003 drift audit), pico corpus real-world verification, pre-PR gate.

- [X] T014 [P] Extend `mikebom-cli/tests/graph_completeness_operator_root.rs` (from m192) with 2 new tests:
  - `us1_go_source_with_stdlib_reports_complete`: build a synthetic Go source repo via `tempdir` (go.mod + main.go); scan with mikebom binary; assert `mikebom:graph-completeness == "complete"` AND `.dependencies[?(@.ref | test("^pkg:golang/example")))].dependsOn` includes `pkg:golang/stdlib@v*`. Repeat with `--root-name X --root-version Y` to verify m192/m193 pre-rewrite interaction.
  - `us2_nested_nameless_npm_workspace_reports_complete`: build a synthetic 2-level npm project (top-level named package.json + nested nameless package.json + both lockfiles); scan; assert `pkg:npm/<nested-dir>` component exists with mainmod role; assert its dependsOn includes chalk (or whatever the nested resolves); assert graph-completeness `complete`. Repeat with `--root-name X --root-version Y`.
- [X] T015 Golden regen — apply to drift-set goldens identified in T002/T003. Use targeted regen per memory `feedback_release_bump_regen_all_golden_tests`:
  ```bash
  MIKEBOM_UPDATE_CDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX3_GOLDENS=1 \
    cargo test -p mikebom --test cdx_regression --test spdx_regression --test spdx3_regression \
    --test pkg_alias_binding_us1 --test oci_pull_backward_compat --test optional_dep_classification
  ```
  Diff-review: every diff MUST be either (a) a new edge `<Go mainmod> → pkg:golang/stdlib@v*` in CDX `dependencies[]`, (b) a new `pkg:npm/<dir-basename>` mainmod component + its edges in the nested-workspace fixture, or (c) a graph-completeness value flipping `partial` → `complete` when the drift removed the last orphan. Reject any other class of diff.
- [X] T016 Non-drift byte-identity gate: every golden NOT in the T002/T003 drift set MUST pass byte-identically post-m194. Enforces FR-012 / SC-006.
- [X] T017 [P] Real-world Kusari pico corpus validation per quickstart.md Reproducer 3. Clone all 4 corpus repos at pinned SHAs; scan each with mikebom binary + `--root-name`; assert all 4 report `mikebom:graph-completeness: complete` (matches SC-001–SC-005). Record findings in `specs/194-close-orphan-gaps/scratch/pico-corpus-validation.txt`.
- [X] T018 [P] Update CLAUDE.md agent-context if `.specify/scripts/bash/update-agent-context.sh` wasn't already invoked during /speckit-plan. Verify current CLAUDE.md lists 194-close-orphan-gaps in Recent Changes.
- [X] T019 Pre-PR gate — `./scripts/pre-pr.sh` MUST pass clean per memory `feedback_prepr_gate_full_output`. Every test suite `ok. N passed; 0 failed`; clippy zero errors zero warnings.

**Checkpoint**: All 4 pico corpus source-repo fixtures report `complete`. Full workspace green.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: T001. No dependencies.
- **Foundational (Phase 2)**: T002 → T003/T004/T005 [P]. Depends on Setup. BLOCKS Phase 3/4.
- **US1 (Phase 3)**: Depends on Foundational.
- **US2 (Phase 4)**: Depends on Foundational. Fully parallel to US1 (different files).
- **Polish (Phase 5)**: Depends on both US phases complete.

### User Story Dependencies

- **US1 ⊥ US2**: no coupling; different reader files (`golang/legacy.rs` vs `npm/mod.rs`). Can be split across developers.
- Both P1 because both are needed for SC-005 (all 4 pico source-repo fixtures → `complete`): kusari-cli/guac only need US1; pico needs both; molcajete only needs US2.

### Within Each User Story

- Tests BEFORE implementation (mikebom TDD convention).
- Impl is a single file edit per story → sequential internally.

### Parallel Opportunities

- Phase 2: T003 / T004 / T005 all `[P]` after T002 completes (T002 feeds drift set for T003 to identify goldens).
- Phase 3: T006 alone (single-file test authoring).
- Phase 4: T009 alone (single-file test authoring).
- **US1 and US2 can proceed in parallel** — different files entirely (`golang/legacy.rs` vs `npm/mod.rs`).
- Phase 5: T014/T017/T018 all `[P]`.

---

## Parallel Example: US1 and US2 in parallel

```bash
# After Foundational, US1 dev and US2 dev proceed in parallel:
Developer A: T006 → T007 → T008 (US1 golang/legacy.rs)
Developer B: T009 → T010 → T011 → T012 → T013 (US2 npm/mod.rs)

# Then converge on Polish:
T014 (integration tests exercising BOTH)
T015 (golden regen)
...
```

---

## Implementation Strategy

### MVP (this milestone IS the MVP)

1. Phase 1 + Phase 2 (setup + drift audit).
2. Phase 3 (US1) AND Phase 4 (US2) in parallel.
3. Phase 5 (integration + regen + verify).
4. Ship as m194 bug-fix; bundle with alpha.62.

### Single-PR delivery (matches m190/m191/m192 shape)

Land Phases 1–5 in a single PR titled `impl(194): close remaining graph-completeness orphan gaps — Go stdlib edge + npm nested mainmod (#571, #572)`. Commit granularity per phase, reviewer digestibility per US.

---

## Implementation-time scope additions (US3, US4)

During real-corpus verification (T017) two additional classes of
false-positive-partial classification surfaced that block SC-005
independently of US1/US2. Both fixes were added to close the
milestone rather than defer to a follow-up:

- **US3** (`compute_graph_completeness` file-tier exclusion):
  file-tier components (`mikebom:component-tier: file`) have no
  dependency edges by design (they represent unattributed file
  inventory, not graph participants). Excluding them from the
  classifier's total/reachable counts prevents them from perma-
  triggering `OrphanedComponentsDetected` on scans that emit them.
  Applied in `mikebom-cli/src/generate/graph_completeness/mod.rs`
  at the top of `compute_graph_completeness`. Zero-change in
  emitted SBOM (file-tier components still appear in `.components[]`).

- **US4** (m192 pre-rewrite parity across SPDX 2.3 + SPDX 3): the
  m192 pre-rewrite added by #570 was only applied to CDX. SPDX 2.3
  + SPDX 3 emitters still saw `partial: orphaned-components-
  detected: N` on operator-override scans that CDX correctly
  classifies `complete`. Extracted the pre-rewrite into
  `graph_completeness::rewrite_dropped_mainmod_edges` (public
  helper) and applied it from all three emit sites — but ONLY as
  the classifier's relationship input, NOT the emit-side (which
  has its own existing dropped-mainmod alias mechanism).

Verified: post-US1+US2+US3+US4, kusari-cli and guac corpus repos
report `complete` across all three formats; pico goes from 57
orphans → 1 residual (edge case out of m194 scope); molcajete
goes to 2 residual (hoisted-unused / dead-lockfile — legitimate
orphan classes independent of m194).

## Notes

- Total tasks: 19 across 5 phases.
- US1: 3 tasks (T006–T008). US2: 5 tasks (T009–T013). Setup/Foundational/Polish: 11 tasks.
- Every `[P]` task edits a distinct file; no collision hazards among parallel tasks.
- Zero new Cargo dependencies (FR: research §R5 audit satisfied); zero new `mikebom:*` annotations (FR-014).
- Byte-identity gate (SC-006) enforced at T016 as a HARD merge blocker.
- Pico corpus validation (T017) is `[P]` because it's validation-only; failure surfaces in PR body but doesn't block merge.
