# Tasks: TheRock's last two unexplained components

**Input**: `specs/1543-therock-last-orphans/` — spec.md, plan.md, research.md (R1–R7), data-model.md, contracts/wire.md (W-1..W-8, L-1..L-11), measurements/trace.md

**Tests**: The spec requires them. SC-007 calls for a test with teeth for each rule, and SC-004 for format agreement and repeatability. Within each story, tests are written first and must fail on `main` before the fix.

**Format**: `[ID] [P?] [Story] Description`. `[P]` marks a different file with no dependency on an incomplete task.

## Phase 1: Setup

- [X] T001 Create `waybill-cli/tests/scan_therock_last_orphans.rs` (file-scope `#![cfg(unix)]`), including the corpus harness by `#[path]` as `waybill-cli/tests/scan_determinism.rs` does. Add helpers:
  - `scan(dir, flags) -> [cdx, spdx23, spdx3]`;
  - `assert_explained(docs)` (I7 via the harness's `probe_i7_on_document` logic);
  - `assert_agree(docs, rows)` (via `row_all_formats`);
  - `assert_repeatable(dir, flags, 20)` (via `masked_difference`);
  - `subject(cdx)`, `targets_of(cdx, ref)`, `prop(cdx, purl, key)`.
- [X] T002 [P] Add fixture builders to the same file, all `waybill-fixture-*`, ported from `measurements/workspace-roots.sh`:
  - workspace builders: `uv_workspace`, `bun_workspace`, `kts_workspace`;
  - `vendored_record(dir, name, version, lib: Lib::{Present, Absent, Dvc})`, which writes `<dir>/lnx/lib/cmake/<name>/<name>ConfigVersion.cmake`, plus `lnx/lib/lib<name>.a` or `lib<name>.a.dvc`;
  - `cmake_file(path, body)`.

## Phase 2: Foundational

- [X] T003 Add `workspace-root` (confidence 0.95) to C69's `<name>` set in `docs/reference/sbom-format-mapping.md`, with a sentence naming spec 1543's rung. Register catalog row **C235** `waybill:cmake-link-conditions` in `docs/reference/sbom-format-mapping.md`. It has C232's shape, normalisation and Principle V audit, and is present iff C122 = `cmake-conditional-link`. Add the C122 value `cmake-conditional-link` to C122's row. Add the C235 extractors beside C232's in `waybill-cli/src/parity/extractors/{mod.rs,cdx.rs,spdx2.rs,spdx3.rs}`. The row and its extractors land in the same commit (the lib-target gate).
- [X] T004 Add `pub(crate) const ANN_CMAKE_LINK_CONDITIONS` and `pub(crate) const DERIVATION_CMAKE_CONDITIONAL_LINK` beside `ANN_CMAKE_FETCH_CONDITIONS` in `waybill-cli/src/scan_fs/package_db/cmake/fetches.rs`, or a shared `cmake/mod`-level module if the constants are used outside `fetches.rs`. Run `cargo build --all-targets`.

**Checkpoint**: the gate is green with no behaviour change. Running `cargo test -p waybill --lib parity` passes.

## Phase 3: User Story 1 — A subproject's workspace root hangs off the document root (P1)

**Goal**: no synthesized workspace root is stranded. At the scan root, it is the subject (R1, R2).

**Independent test**: `workspace-roots.sh` prints no empty `incoming=` cell. W-1..W-8 pass.

### Tests (write first; they must fail on `main`)

- [X] T005 [P] [US1] Add W-1, W-2 and W-3 to `waybill-cli/tests/scan_therock_last_orphans.rs`. Each is a workspace in `proj/` under a Cargo main module, scanned with `--root-name`. Assert:
  - the root → `pkg:generic/<ws>`;
  - `<ws>` → each member;
  - the members' root edges are still present;
  - no `foreign-build-system` on `<ws>`;
  - `assert_explained`, `assert_agree(C45, C104, C105)`, and `assert_repeatable(…, 20)`.
- [X] T006 [P] [US1] Add W-4, W-5 and W-6 to the same file. Each workspace is at the scan root, with no flags. Assert the subject is `pkg:generic/<ws>`, C69 = `workspace-root` in all three formats, the members are reachable through it, `assert_explained`, and `assert_repeatable(…, 20)`.
- [X] T007 [P] [US1] Add W-7 (uv and bun at one root: neither promoted, both anchored) and W-8 (the workspace root is already a relationship target: no extra root edge) to the same file. Each also calls `assert_repeatable(…, 20)`.
- [X] T008 [P] [US1] Add a unit test in `waybill-cli/src/generate/root_selector.rs` for the rung with zero, one, and two workspace roots at the scan root, and with one in a subdirectory, each with and without an operator override.

### Implementation

- [X] T009 [US1] R1: in `waybill-cli/src/scan_fs/mod.rs`, where `declared_root_anchors` is computed (≈ line 1662), append the PURL of every component that meets both conditions:
  - its `waybill:component-role` is `workspace-root`;
  - no relationship has it as its `to`.

  Sort and deduplicate as the existing anchors are. Comment with the trace's 7-of-12 finding.
- [X] T010 [US1] R2: in `waybill-cli/src/generate/root_selector.rs`, add the rung as a function beside `evaluated_nix_package`, called after the operator-override and image rungs and before the main-module ladder. If exactly one component has role `workspace-root` and its first `waybill:source-files` entry contains no `/`, select it as `MainModule(idx)`, with a new `RootSelectionHeuristic::WorkspaceRoot` variant (`as_str` `workspace-root`, confidence 0.95) and the main modules as `losers`. No caller changes: the rung reads only the components.
- [X] T011 [US1] Update the existing expectations this changes, after reading each failure. A Kotlin-at-root subject moves from a placeholder to `pkg:generic/<rootProject.name>@0.0.0`. Check `waybill-cli/tests/scan_kotlin_dsl.rs`, `scan_kmp_polyglot.rs`, `dart_edge_cases.rs`, and unit tests in `pip/uv_lock.rs` and `npm/bun_lock.rs`. Record each changed assertion and its cause in `specs/1543-therock-last-orphans/measurements/audit.md`.
- [X] T012 [US1] Rerun `specs/1543-therock-last-orphans/measurements/workspace-roots.sh` on the branch binary. Save the output as `measurements/workspace-roots.branch.out`. Expected: no empty `incoming=` cell, and the three `atroot plain` subjects are the `pkg:generic/` roots.

**Checkpoint**: T005–T008 pass; US1 is independently shippable.

## Phase 4: User Story 2 — A vendored library linked by path is a dependency (P1)

**Goal**: link-directory consumption of an installed-library record gives an edge from the linking project. It carries the conditions it is linked under, including conditions open at an `include()` site (R3–R6).

**Independent test**: L-1..L-11 pass, and CMake output with no link correlation is byte-identical to `main`.

### Tests (write first; they must fail on `main`)

- [X] T013 [P] [US2] Add L-1, L-2, L-3 and L-4 to `waybill-cli/tests/scan_therock_last_orphans.rs`. They cover conditional and unconditional links, and the library present, absent, or a DVC pointer only, with the record name differing from the link name. Assert:
  - the edge source;
  - the scope (CycloneDX `scope`, SPDX 2.3 `OPTIONAL_DEPENDENCY_OF`);
  - C122;
  - C235 via `row_all_formats`;
  - `assert_explained`;
  - `assert_repeatable(…, 20)`.
- [X] T014 [P] [US2] Add L-5 and L-6 (nothing invented: compare the component and relationship sets with a scan that has no link lines) L-11 (`link_directories` directory scope) and L-12 (nested prefixes: only the innermost record is linked) to the same file. Each also calls `assert_repeatable(…, 20)`.
- [X] T015 [P] [US2] Add L-7 (the link in `CMakeLists_wsl.txt`, included under `if (WIN_SDK)`, giving C235 `["WIN_SDK"]`) and L-10 (`include(FetchContent)` ignored; an include cycle terminates) to the same file. Each also calls `assert_repeatable(…, 20)`.
- [X] T016 [P] [US2] Add L-8 (the linking file inside a git submodule directory, using the submodule fixture helper from spec 1530's tests: the edge is from the submodule; no owner gives a root anchor) and L-9 (conditions union; one unconditional link makes the record required with no C235) to the same file. Each also calls `assert_repeatable(…, 20)`.
- [X] T017 [P] [US2] Add unit tests in the new `waybill-cli/src/scan_fs/package_db/cmake/links.rs`:
  - plain-item filtering (`::`, `/`, `$<`, `${`, `-l…`, keywords);
  - `target_link_directories` with `BEFORE` and scope keywords;
  - target-token pairing;
  - variable substitution with `CMAKE_CURRENT_SOURCE_DIR` vs `CMAKE_CURRENT_LIST_DIR` in an included file;
  - the include depth bound.

### Implementation

- [X] T018 [US2] R3/R4 parsing: create `waybill-cli/src/scan_fs/package_db/cmake/links.rs` with `LinkDeclaration { file, directory, name, conditions }` and `read_links(file, scan_root) -> Vec<LinkDeclaration>`. It reuses `installed_lib::cmake_version::{commands_checked, strip_comments}` and `cmake/blocks.rs::BlockStack`, follows `include()` per R3 with the include site's open blocks seeded, and normalises conditions exactly as C232 does (share the helper from `fetches.rs`). Declare the module in `waybill-cli/src/scan_fs/package_db/cmake.rs`.
- [X] T019 [US2] Collect link declarations in `cmake.rs::read_with_downloads` over the walked CMake files, the same set fetches use. Return them through the reader's coverage, as `FetchCoverage` is, and carry them to `ScanResult` in `waybill-cli/src/scan_fs/mod.rs`.
- [X] T020 [US2] R4/R5: create `waybill-cli/src/scan_fs/cmake_link_attach.rs`, mirroring `cmake_fetch_attach.rs`. For each final component with `waybill:source-mechanism` `cmake-package-version` or `pkg-config`:
  - compute its prefix with `installed_lib::location::prefix_of` on its source file;
  - match declarations per R4, including the DVC pointer and the longest-prefix rule;
  - emit `owner_of(file)` → record edges (provenance `cmake-link-declaration`), or root anchors;
  - set the scope, C122 and C235 per R5, with spec 1538's merge (Test outranks Optional; required if any link is unconditional).

  It returns a `DeclaredAttachment` plus per-record scope changes. Call it at two sites in `scan_fs/mod.rs`, as fetches are:
  - **(a)** in the owner block (≈1550), after `cmake_fetch_attach`: apply the scope, C122 and C235 to `components`, and extend `relationships` with its edges, before `apply_lifecycle_scope_to_edges` (≈1554);
  - **(b)** at the `declared_root_anchors` computation (≈1665): extend the anchors with its `root_anchors`, computed from the final components.
- [X] T021 [US2] Document links and includes in `docs/ecosystems.md`, in the CMake section beside the `cmake-conditional` table: the match rule, owner, scope, C235, the DVC rule, and that fetches are not yet read through `include()`.

**Checkpoint**: T013–T017 pass; US2 is independently shippable.

## Phase 5: User Story 3 — TheRock's source tree has no unexplained component (P2)

**Goal**: SC-002, SC-003 and SC-006 on TheRock, measured in CI only.

- [X] T022 [US3] Push the branch, then dispatch `measure-source.yml` (≥5 interleaved pairs) and `measure-full.yml` in `kusari-sandbox/waybill-fixture-therock-sdk`, with `--ref main -f waybill_ref=<sha> -f base_ref=main`.
- [X] T023 [US3] Probe the branch documents with `probe_i7_on_document` and `row_all_formats`. Record in `specs/1543-therock-last-orphans/measurements/therock.md`:
  - the unexplained count (expect 0);
  - `tensile` reachable;
  - `wkmi`'s edge source (`rocm-systems`), scope and C235 `["WIN_SDK"]` in all three formats;
  - the repeated-scan comparison;
  - the median times for branch and `main` (expect ≤ 1.1×);
  - the built SDK unchanged.

  Trace every other component-set difference from `main`.

## Phase 6: Polish & cross-cutting

- [X] T024 [P] Write a mutation script `specs/1543-therock-last-orphans/measurements/mutate.py` and runner `mutate-run.sh` (as in spec 1542). Mutate each of:
  - the anchor;
  - the subject rung;
  - the C69 heuristic (returned as `RepoRoot`);
  - the include-site seeding;
  - the longest-prefix rule;
  - the prefix test;
  - the name/file test;
  - the DVC match;
  - the owner lookup (forced to root);
  - the scope rule.

  Record which test fails for each in `measurements/mutations.md`. Revert with `cp`, never `git checkout`.
- [X] T025 [P] File the follow-up issue for reading fetches, `find_package` and downloads through `include()` (research R3, trace §2), and link it in `research.md`.
- [X] T026 Dispatch `public-corpus.yml` with `repeat=2` at the branch and at `main`, and the quality corpus against `main`. Trace every golden or score movement in `measurements/audit.md` (expect no golden change, per trace §1).
- [X] T027 Add a `CHANGELOG.md` entry (#1601, #1602) and a line in `docs/reference/sbom-invariants.md` if the I7 wording names workspace roots.
- [X] T028 Run the gate alone: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`. Record the per-target pass counts in `measurements/gate-log.md`.

## Dependencies & execution order

- **Setup (T001–T002)** goes first.
- **Foundational (T003–T004)** blocks US2 only. US1 can start after Setup.
- **US1 (T005–T012)** and **US2 (T013–T021)** are independent. They touch different files, except `scan_fs/mod.rs`, which both edit near line 1662, so T009 and T020 are sequential.
- **US3 (T022–T023)** needs US1 and US2.
- **Polish:**
  - T024 needs US1 and US2;
  - T026 needs both stories;
  - T028 is last.

### Parallel examples

- **US1:** T005, T006, T007 and T008 together, then T009 → T010 → T011 → T012.
- **US2:** T013, T014, T015, T016 and T017 together, then T018 → T019 → T020 → T021.
- **Across stories:** T024 and T025 together.

## Implementation strategy

1. **MVP is US1:** it closes #1601 alone, with no corpus exposure.
2. **US2** closes #1602.
3. **US3** proves both on TheRock in one CI round.
4. **Polish** comes last, and the gate before opening the PR.
