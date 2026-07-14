---
description: "Task list for m192 — graph-completeness partial-value fix for operator-supplied roots"
---

# Tasks: Fix Graph-Completeness Over-Firing on Operator-Supplied Roots

**Input**: Design documents from `/specs/192-operator-root-completeness/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/classifier-input.md, quickstart.md

**Tests**: Included — mikebom's standard integration-test-plus-unit-test pattern (matches m190/m191).

**Organization**: Single user story (US1 — CI SBOM generation with `--root-name` reports `complete` when the graph is complete). Small scope → 13 tasks across 4 phases.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story (US1 only for this milestone)
- File paths absolute where non-obvious; workspace root is `/Users/mlieberman/Projects/mikebom/`

## Path Conventions

- **mikebom-cli crate**: `mikebom-cli/src/…`, `mikebom-cli/tests/…`
- **Feature spec dir**: `specs/192-operator-root-completeness/…`

---

## Phase 1: Setup

**Purpose**: Verify baseline is clean so any regression signal in later phases is unambiguous.

- [X] T001 Confirm `192-operator-root-completeness` branch is checked out and clean (`git status` shows only the specs/ directory as untracked; CLAUDE.md may show auto-updated by /speckit-plan). Deferred baseline pre-PR to T013 — no Rust changes yet so baseline equals m191's post-merge state (243 test suites passing).

**Checkpoint**: Baseline recorded; workspace clean.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Golden-drift audit per research §R3 — identify which existing goldens currently emit `partial` due to the false-positive path AND which are unaffected. Drives Phase 4's golden-regen decision.

⚠️ **CRITICAL**: Must complete before Phase 3 impl so the byte-identity gate (SC-004) is scoped correctly.

- [X] T002 Audit every existing golden CDX file for the current `mikebom:graph-completeness` value: `for f in mikebom-cli/tests/fixtures/golden/cyclonedx/*.cdx.json; do echo "=== $(basename $f) ==="; jq -r '.metadata.properties[]? | select(.name | test("graph-completeness")) | "\(.name): \(.value)"' "$f"; done`. Record findings in `specs/192-operator-root-completeness/scratch/golden-drift.txt`. Any golden currently reporting `partial: multi-ecosystem-partial-root: ...` is expected to flip to `complete` post-m192 — those goldens need regen. Every golden reporting `complete` today MUST remain `complete` (byte-identity).
- [X] T003 [P] Audit SPDX 2.3 + SPDX 3 golden shapes for the same annotation. Grep for `mikebom:graph-completeness` inside SPDX Annotation comments + SPDX 3 annotation graph elements. Record findings in the same scratch file.

**Checkpoint**: Drift set captured; regen scope known.

---

## Phase 3: User Story 1 — CI SBOM generation reports `complete` on operator-supplied roots (Priority: P1)

**Goal**: When operators pass `--root-name X --root-version Y` (with optional `--root-purl-type <eco>`) against a well-formed source repo, `mikebom:graph-completeness` reports `complete`. When the underlying graph has real orphans, the classifier STILL fires `OrphanedComponentsDetected` (real signal preserved).

**Independent Test**: Scan a synthetic Go source repo with `--root-name X --root-version Y --format cyclonedx-json`. Assert `.metadata.properties[?(@.name=="mikebom:graph-completeness")].value == "complete"` and `mikebom:graph-completeness-reason` annotation is absent. Repeat across SPDX 2.3 + SPDX 3.

### Tests for User Story 1

> **Write tests FIRST; ensure they FAIL against the pre-m192 tree before implementation.**

- [X] T004 [P] [US1] Add unit tests for `build_ecosystem_root_set` in `mikebom-cli/src/generate/graph_completeness/bfs.rs::tests` covering the 4 fixture shapes from `contracts/classifier-input.md`:
  - **Fixture O1** (operator-override, single-ecosystem Go components) → `ecosystems_without_root == []`, `roots` contains `target_ref`, `per_ecosystem_root` has `golang → target_ref`
  - **Fixture O2** (operator-override, mixed golang+npm+pypi) → `ecosystems_without_root == []`, `per_ecosystem_root` has entries for all 3 ecosystems all pointing at `target_ref`, INFO log fires with `synthesized_ecosystems_count = 3`
  - **Fixture O3** (operator-override + `--root-purl-type golang` shaped `target_ref = pkg:golang/svc@1.0` with Go+npm components) → synthesis SKIPS golang (already covered by operator's PURL), fires for npm only, `synthesized_ecosystems_count = 1`
  - **Fixture N** (native-root MainModule) → synthesis block does NOT execute, byte-identity to pre-m192 output, NO INFO log emitted
- [X] T005 [P] [US1] Add unit test in the same test module for the "real orphan still detected" case per FR-007 / contract: Fixture O1 modified to include a synthetic orphan component with no edges → downstream classifier MUST STILL emit `OrphanedComponentsDetected`. This test lives in `mod.rs::tests` since it invokes `compute_graph_completeness` end-to-end, not just `build_ecosystem_root_set`.
- [X] T006 [P] [US1] Add integration test file `mikebom-cli/tests/graph_completeness_operator_root.rs` covering the 5 US1 acceptance scenarios per quickstart.md Reproducers 1-4:
  - Go source repo with `--root-name X --root-version Y` → `mikebom:graph-completeness == "complete"` in all 3 formats
  - Mixed Go+npm source repo with `--root-name X --root-version Y --root-purl-type golang` → `complete` (no duplicate golang root)
  - Native-root scan (no `--root-name`) → byte-identical to pre-m192 (reproducer 4 covers this)
  - Real-orphan integration case: fabricate a scenario where mikebom emits a component with no edges → assert `partial` with `orphaned-components-detected` reason (fix preserves real gaps)

### Implementation for User Story 1

- [X] T007 [US1] Extend `build_ecosystem_root_set` signature at `mikebom-cli/src/generate/graph_completeness/bfs.rs:73` to accept a new `target_ref: &str` parameter. Update the docstring to reference m192 + FR-001. Add the standard `#[cfg_attr(test, allow(clippy::unwrap_used))]` guard on the co-located test module per Constitution Principle IV / codebase convention.
- [X] T008 [US1] Implement the synthesis pass per data-model.md::Function::build_ecosystem_root_set::Extended behavior. Place the block AFTER the existing per-ecosystem-root loop (currently ends at line 116) and BEFORE `ecosystems_without_root` computation (currently at line 118). Include:
  - `is_native_root` guard via `matches!(selection.subject, ResolvedRootSubject::MainModule(_))`
  - `operator_root_ecosystem` extraction via `Purl::new(target_ref).ok().map(|p| p.ecosystem().to_string()).filter(|e| e != "generic")`
  - Per-ecosystem loop over `components` inserting `(eco, target_ref.to_string())` into `per_ecosystem_root` when not already present AND not matching `operator_root_ecosystem`
  - Increment `synthesized_count` on each insertion
  - Emit `tracing::info!` when `synthesized_count > 0` per FR-009 / Q1 answer A
- [X] T009 [US1] Update the caller at `mikebom-cli/src/generate/graph_completeness/mod.rs:156` to pass `target_ref` through: change `bfs::build_ecosystem_root_set(components, selection)` → `bfs::build_ecosystem_root_set(components, selection, target_ref)`. This is the one-line wiring change; `target_ref` is already in scope as `compute_graph_completeness`'s `target_ref: &str` parameter.
- [X] T010 [US1] Run T004 + T005 unit tests locally: `cargo test -p mikebom --bin mikebom generate::graph_completeness::bfs::tests` and `cargo test -p mikebom --bin mikebom generate::graph_completeness::tests`. MUST all pass.
- [X] T011 [US1] Run T006 integration test: `cargo test -p mikebom --test graph_completeness_operator_root`. MUST all pass. Also manually reproduce quickstart.md Reproducer 1 to confirm the wire-shape flip end-to-end.

**Checkpoint**: All operator-override scans that were incorrectly reporting `partial: multi-ecosystem-partial-root` now report `complete`. Native-root path byte-identical. Real orphans still detected via `OrphanedComponentsDetected`.

---

## Phase 4: Polish & Cross-Cutting Concerns

**Purpose**: Golden regen (if any), cross-format verification, pre-PR gate.

- [X] T012 Golden regen — apply to any drift-set goldens identified in T002/T003. Use the "targeted regen" approach per memory `feedback_release_bump_regen_all_golden_tests`:
  ```bash
  MIKEBOM_UPDATE_CDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX3_GOLDENS=1 \
    cargo test -p mikebom --test cdx_regression --test spdx_regression --test spdx3_regression \
    --test pkg_alias_binding_us1 --test oci_pull_backward_compat --test optional_dep_classification
  ```
  Diff-review the resulting changes: every diff MUST be either (a) `"partial"` → `"complete"` on the `mikebom:graph-completeness` annotation, (b) removal of `mikebom:graph-completeness-reason` annotation entry with `multi-ecosystem-partial-root` value, or (c) trivial reordering caused by removed annotations. Reject any other class of diff. If T002/T003 found ZERO drift-set goldens, this task is a no-op (native root selection already picks per-ecosystem roots correctly on the existing golden fixtures).
- [X] T013 Pre-PR gate — run `./scripts/pre-pr.sh` and confirm BOTH commands pass clean per memory `feedback_prepr_gate_full_output`. Every test suite MUST report `ok. N passed; 0 failed`; clippy MUST report zero errors AND zero warnings.

**Checkpoint**: Full workspace clippy clean, full workspace test suite green, any drift-set goldens regenerated with explainable diffs, non-drift goldens byte-identical.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: T001. No dependencies.
- **Foundational (Phase 2)**: T002 (T003 [P]). Depends on Setup. BLOCKS Phase 3.
- **US1 (Phase 3)**: Depends on Foundational.
- **Polish (Phase 4)**: Depends on US1.

### Within User Story 1

- Tests BEFORE implementation (matches mikebom's TDD-flavored convention).
- T007 → T008 → T009: sequential (all edit `bfs.rs` or its call site).
- T010 → T011: sequential (T010 confirms unit correctness before integration test firing).

### Parallel Opportunities

- Phase 2: T002 must complete first (feeds drift-set input for T003); T003 can then run.
- Phase 3: T004, T005, T006 all `[P]` — parallel test authoring across the three test targets.
- Phase 3: T007–T009 sequential (same file).
- Phase 4: nothing parallel (small polish set).

Single-PR delivery is the natural shape — 13 tasks in one commit graph.

---

## Parallel Example: User Story 1

```bash
# All test-authoring tasks in parallel:
Task: "T004 Add unit tests for build_ecosystem_root_set — 4 fixture shapes"
Task: "T005 Add unit test for real-orphan detection in mod.rs::tests"
Task: "T006 Add integration test file graph_completeness_operator_root.rs"

# After Foundational lands, sequential impl:
Task: "T007 Extend build_ecosystem_root_set signature"
Task: "T008 Implement synthesis pass"
Task: "T009 Wire target_ref through call site in mod.rs"

# Then verify:
Task: "T010 Run unit tests"
Task: "T011 Run integration tests + manual reproducer"
```

---

## Implementation Strategy

### MVP (this milestone IS the MVP — single P1 story)

1. Complete Phase 1 + Phase 2 (baseline + drift audit).
2. Complete Phase 3 (US1: tests + impl + verify).
3. Complete Phase 4 (regen + pre-PR).
4. Ship as a bug-fix milestone; may be included in the next release cut alongside m190/m191 (alpha.62 or similar).

### Delivery shape

Single PR titled `impl(192): graph-completeness partial-value fix for operator-supplied roots (--root-name)`. Small enough for review-in-one-pass; matches the m189 bug-fix PR shape.

---

## Notes

- Total tasks: 13 across 4 phases.
- Single user story: 8 tasks (T004–T011). Setup/Foundational/Polish: 5 tasks.
- Every `[P]` task edits a distinct file; no file-collision hazards among parallel tasks.
- Zero new Cargo dependencies; zero new `mikebom:*` annotations (FR-008).
- Byte-identity gate (SC-004) is enforced by the `is_native_root` guard in T008 — proven by Fixture N unit test in T004 + regression suite in T013.
