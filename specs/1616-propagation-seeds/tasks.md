# Tasks: Scope propagation protects every runtime project, not one per ecosystem

**Input**: `specs/1616-propagation-seeds/` (spec.md, plan.md, research.md, data-model.md, contracts/wire.md, quickstart.md)

**Tests**: requested (spec SC-006, plan Testing). Each story's tests are written first and must fail before its implementation.

**Paths**:
- `SP` = `waybill-cli/src/resolve/scope_propagation.rs`
- `SC` = `waybill-cli/src/cli/scan_cmd.rs`
- `IT` = `waybill-cli/tests/propagation_seeds.rs`
- `FX` = `waybill-cli/tests/fixtures/propagation_seeds/`

## Phase 1: Setup

- [X] T001 Confirm the premises at `main` and record them in `specs/1616-propagation-seeds/measurements/trace.md`:
  - (a) when `propagate_non_runtime_scope` runs, every main module carries `waybill:component-role` = `main-module`, with and without `--root-name` (research R1). Check with a throwaway debug assertion or log at the call site in `SC`, then remove it.
  - (b) in a two-project tree, which component `select_root` makes the primary root, and which pip main module `component_root_seeds` then seeds (`pick_ecosystem_top`, `generate/graph_completeness/bfs.rs`). Record both. The fixture must make `a` the seeded project and `b` the unseeded one.
  - (c) `runtime_reachable` admits a non-runtime seed as immune (research R2).
- [X] T002 Build the P-case fixture under `FX`, extending spec 1269's uv fixture (`waybill-cli/tests/fixtures/scope_propagation/uv/`) to two pip projects:
  - `a/`, the seeded one per T001(b): a `pyproject.toml` + `uv.lock` with a PEP 735 dev group holding `waybill-fixture-devtool`, which depends on `waybill-fixture-shipped` and `waybill-fixture-devonly`;
  - `b/`: declares `waybill-fixture-shipped` at runtime, which depends on `waybill-fixture-shipped-dep`.

  Also build the P-4 (non-runtime project) and P-5 (project under `.github/actions/x/`) variants as separate trees. Use synthetic names only.
- [X] T003 Create `IT` with helpers:
  - `scan` asserts C42 and C62 across the three formats via `row_all_formats`;
  - `scope_of`, `c62_of`, and `assert_required` / `assert_transitive(name, scope)`;
  - `assert_repeatable`, as in `waybill-cli/tests/scan_nested_req_scope.rs`.

## Phase 2: Foundational

- [X] T004 Write P-7, the byte-identity control, in `IT`. The tree is `FX/ctl/`, spec 1269's single-project uv shape with one main module. Generate its golden in `FX/ctl-golden/` from **`main`'s** binary, before any source change, and compare masked output (FR-008).

**Checkpoint**: P-7 passes at `main`.

## Phase 3: User Story 1 — every project's runtime dependencies stay runtime (P1) 🎯 MVP

**Goal**: FR-001, FR-003, FR-005: runtime main modules not already seeded join Phase A's seeds.

**Independent Test**: P-1, P-2, P-3 and P-6 pass.

- [X] T005 [P] [US1] Unit tests in `SP`'s test module:
  - an added seed protects its runtime closure;
  - with an empty added set the result is identical to today's, for every existing test input;
  - an added seed does not traverse an inferred-edge or nix build-graph edge (FR-003);
  - an assigned component's scope is unchanged where it is still assigned (research R4).
- [X] T006 [P] [US1] Integration cases P-1, P-2, P-3, P-6, P-9 and P-10 in `IT`.
  - P-1's `shipped`, P-2, P-9 and P-10 must fail at `main`.
  - P-1's `a-runtime`, P-3 and P-6 pass there, as guards.
- [X] T007 [US1] Implement in `SP`: `propagate_non_runtime_scope` takes `added_seeds: &HashSet<String>`, and Phase A runs `runtime_reachable` from `seeds ∪ added_seeds`. `ecosystems_with_root`, the FR-012 gate and `adj` are unchanged. Update every existing call in `SP`'s tests to pass an empty set.
- [X] T008 [US1] Implement in `SC`: next to the existing call, compute `added_seeds` as every component with `waybill:component-role` = `main-module` whose PURL is not in `seeds` and that passes T009's runtime test, then pass it. `component_root_seeds` is not changed (research R3).
- [X] T009 [US1] Implement the runtime test for an added seed (research R2) as a function in `SP`, used by T008. It requires both:
  - `lifecycle_scope` is `None` or `Runtime`;
  - not every evidence source file has a listed dot-directory ancestor, using `project_roots::listed_ancestor_class` the way `scan_fs/ci_scope.rs::class_of` does, relative to the scan roots.
- [X] T010 [US1] Run T005, T006, P-7, `scope_propagation_ecosystems` and `cargo test -p waybill --bins scope_propagation::` until they pass.

**Checkpoint**: US1 complete.

## Phase 4: User Story 2 — a non-runtime project protects nothing (P1)

**Goal**: FR-002: a main module that is non-runtime, by its own scope or by a dot-directory location, is not a seed.

**Independent Test**: P-4 and P-5 pass.

- [X] T011 [P] [US2] Unit tests for T009's function: `Development`, `Test` and `Optional` main modules, all-dot-directory and mixed-location main modules.
- [X] T012 [US2] Integration cases P-4 and P-5 in `IT`. With T009's test temporarily replaced by "every main module", confirm that P-4 and P-5 fail; then restore the test. This is the positive control that P-4 and P-5 can fail.

## Phase 5: User Story 3 — the record shows what the wider protection did (P3)

**Goal**: FR-006, `immune_by_added_seeds`.

**Independent Test**: P-8 passes.

- [X] T013 [US3] Unit test in `SP`: with base and added seeds, `immune_by_added_seeds` counts only the components Phase B reached that are unscoped, not compiler-supplied, and immune only through the added seeds.
- [X] T014 [US3] Implement in `SP`:
  - compute `runtime_reachable` from `seeds` alone, and again from `seeds ∪ added_seeds`;
  - Phase B counts reached components that are in the second set and not the first;
  - add the field to `PropagationSummary` and to both INFO lines (the "assigned nothing" one too).
- [X] T015 [US3] P-8 in `IT`: capture the scan's stderr with `RUST_LOG=info` and assert `immune_by_added_seeds=1` on the propagation line. Try `RUST_LOG=info` on the scan's stderr first. If the propagation line is not in it, assert through T013 instead and record in `trace.md` why the line was not visible.

## Phase 6: Polish & cross-cutting

- [X] T016 [P] P-rep in `IT`: P-1 to P-5 in one tree, scanned 20 times, identical each time.
- [X] T017 [P] Docs: in `docs/reference/sbom-format-mapping.md`, C62's description of `transitive` says which roots protect a component (the primary root, one main module per ecosystem, and every other runtime main module), and why graph completeness keeps its own roots.
- [X] T018 [P] Add a `CHANGELOG.md` entry under Unreleased / Fixed.
- [X] T019 Mutations, in `specs/1616-propagation-seeds/measurements/{mutate.py,mutate-run.sh,mutations.md}`, adapted from spec 1615's runner: restore with `cp`, rebuild after the last one, and use the `^error\[E…\]` compile pattern. One each:
  - (M1) empty added seeds;
  - (M2) non-runtime main modules seeded;
  - (M3) dot-directory main modules seeded;
  - (M4) the added seeds *replace* `seeds` in Phase A instead of joining them (FR-005: P-1's `a-runtime` must turn red). Widening `ecosystems_with_root` is not used: every added seed's ecosystem already has a seed, so that mutation could not change behaviour;
  - (M5) the count includes components the base seeds already protect;
  - (M6) added seeds traverse excluded edges, using a separate adjacency without the exclusion.

  Each must turn at least one test red, with 0 compile errors.
- [ ] T020 Gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`. Record `GATE_RESULT`, the ok and non-ok counts, passed/failed, failure blocks, and an empty `git diff --stat main -- '*Cargo.toml' Cargo.lock` in `specs/1616-propagation-seeds/measurements/gate-log.md`. Trace any repository golden that changes, component by component.
- [ ] T021 Push. Dispatch two TheRock measure-source runs against `main` with full SHAs. In `specs/1616-propagation-seeds/measurements/therock.md`, record:
  - scan time (SC-005);
  - the 11 packages (SC-001);
  - every component whose C42 or C62 changes, each traced to its protecting added seed (SC-002); nothing may gain a non-runtime scope (SC-003);
  - parity, duplicates, unexplained orphans and runner identity (SC-004).
- [ ] T022 Dispatch the public corpus with `regen_goldens=true`, and the quality corpus at the branch with a `main` control. Trace every golden change and every changed quality document (locally cached targets, as in spec 1615) to a protecting seed. Commit the regenerated goldens. Record all of it in `specs/1616-propagation-seeds/measurements/corpus.md`.
- [ ] T023 Remove spec 1615's exception: edit `specs/1615-nested-req-scope/spec.md` FR-007, SC-002 and the clarification, and `measurements/therock.md`, citing T021's measurement (FR-007).
- [ ] T024 Open the PR, with a body citing T019–T023, `Closes #1618` and the attribution line. Do not merge.

## Dependencies

- T001 → T002 → T003 → T004 (P-7's golden is made from `main` before any source change).
- **US1** (T005–T010) before **US2** (T011–T012), since T009 is introduced in US1. Tests are written in parallel within each story.
- **US3** (T013–T015) needs T007.
- **Polish:** T016–T018 in parallel, after US1–US3. Then T019 → T020 → T021 ∥ T022 → T023 → T024.

## Parallel examples

- T005 and T006, in different files.
- T016, T017 and T018.

## Implementation strategy

**MVP is US1** (T001–T010): it restores the 11 TheRock packages. US2 guards against over-protection and is part of the same release; it shares T009 with US1. US3 is a diagnostic.
