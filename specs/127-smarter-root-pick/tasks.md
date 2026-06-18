# Tasks: Smarter root component selection for polyglot + multi-module Go workspace scans

**Input**: Design documents from `/specs/127-smarter-root-pick/`
**Prerequisites**: plan.md, spec.md (with Clarifications), research.md, data-model.md, contracts/cli-behavior.md, contracts/annotation-schema.md, quickstart.md

**Tests**: Included. Three new integration-test targets are spec-mandatory per SC-001/SC-002/SC-005, plus the byte-identity regression target gates SC-003.

**Organization**: Tasks grouped by user story (US1 = #367 multi-module workspace, US2 = #366 polyglot, US3 = transparency annotation). Setup + Foundational phases land the shared scaffolding (`is_workspace_root` annotation, `root_selector.rs` module, ecosystem priority constant). Each user-story phase is independently testable.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Different files, no dependencies on incomplete tasks — safe to run in parallel.
- **[Story]**: User-story label on Phase 3+ tasks (US1 / US2 / US3).

## Path Conventions

Mikebom three-crate workspace (`mikebom-cli/`, `mikebom-common/`, `mikebom-ebpf/`). All changes in `mikebom-cli/` per plan.md "Structure Decision". Integration tests in `mikebom-cli/tests/`. Fixtures in `mikebom-cli/tests/fixtures/root_selection/`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Stand up the new module + fixture directory before any logic lands.

- [X] T001 Create the new module file `mikebom-cli/src/generate/root_selector.rs` with the `RootSelectionHeuristic` enum stub (variants per data-model.md, `name()` returning the stable string per contracts/annotation-schema.md, `confidence()` returning the fixed `f64` per the same table), and wire it into `mikebom-cli/src/generate/mod.rs` via `pub mod root_selector;`.
- [ ] T002 [P] Create the fixture directory `mikebom-cli/tests/fixtures/root_selection/` with subdirectories `multi_module_go_workspace/`, `polyglot_go_maven_npm/`, `go_subdir_no_root_module/`, and `cargo_workspace/`. Each subdir gets a placeholder `.gitkeep` for now.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Set the new `mikebom:is-workspace-root` annotation on every ecosystem's main-module emission path AND implement the FR-012 Maven `scan_target_coord` dedup. These must precede US1/US2/US3 because every later test asserts on this annotation being set correctly.

⚠️ **CRITICAL**: All US-phase tasks depend on these. No US-phase work begins until Phase 2 is complete.

> **Stage-1 implementation deviation (post-/speckit-implement)**: tasks T003–T008 (six per-ecosystem reader edits) collapsed into a single consolidated pass in `scan_fs/mod.rs::scan_path` after all readers complete, leveraging the existing `mikebom:source-files` annotation that every main-module emitter already populates. Same observable behavior, much smaller blast radius. The function `tag_main_modules_with_workspace_root` is staged in `scan_fs/mod.rs` but NOT YET called from Stage 1 — calling it would add a `mikebom:is-workspace-root: <bool>` annotation to every main-module, which the unfiltered SPDX 3 emitter currently serializes (SPDX 3 content-hashed IDs cascade → 33-golden byte-identity breaks). Stage 2 adds the filter at the per-format emission sites AND the tagging call site simultaneously.

- [ ] T003-T008 (CONSOLIDATED, STAGE-2 ACTIVATION REQUIRED) Add `mikebom:is-workspace-root` annotation to every main-module-tagged component via a single post-readers pass in `mikebom-cli/src/scan_fs/mod.rs::scan_path`. Compute `is_workspace_root = canonicalize(scan_root) == canonicalize(component.source_files[0].parent())`. The function `tag_main_modules_with_workspace_root` exists; the activation requires Stage 2's per-format emission filter to land first.
- [X] T009 In `mikebom-cli/src/scan_fs/mod.rs::scan_path`, after all per-ecosystem readers return AND before returning to the CLI, implement FR-012: when a Maven main-module exists whose PURL matches `scan_target_coord`, set `scan_target_coord = None`. Add a `tracing::debug!` log line naming the suppressed coord.
- [X] T010 In `mikebom-cli/src/generate/root_selector.rs`, define the `RootSelectionResult` struct + `ResolvedRootSubject` enum.
- [X] T011 In `mikebom-cli/src/generate/root_selector.rs`, define the `ECOSYSTEM_PRIORITY: &[&str]` constant.
- [X] T012 In `mikebom-cli/src/generate/root_selector.rs`, implement `pub fn select_root(...)`. (Per stage-1 refactor: `scan_path: &Path` parameter dropped — `is_workspace_root` is read from a component annotation set in `scan_fs/mod.rs`, so the selector doesn't need the path. Otherwise per contracts/cli-behavior.md.)
- [X] T013 In `mikebom-cli/src/generate/root_selector.rs`, add unit tests for `select_root()`. (9 tests landed: single-main-module fast path, override always wins, repo-root tiebreaker, ecosystem-priority, LCP picks unique, LCP no winner falls through, no-main-modules + Maven coord, confidence values, heuristic names. Symlink dedup test deferred to Stage 2 alongside the actual is_workspace_root tagging.)

**Checkpoint**: At end of Phase 2 the annotation pipeline + selector module compile, all unit tests pass, and `cargo +stable clippy --workspace --all-targets -- -D warnings` is clean. NO emitter wiring yet; existing alpha.48 SBOMs continue to emit byte-identically.

---

## Phase 3: User Story 1 (P1) — Multi-module Go workspace picks the repo-root module as SBOM subject

**Goal**: SC-001 — `mikebom sbom scan --path .` on a multi-module Go workspace (otel-collector shape) produces an SBOM whose root identifies the repo-root module, not an alphabetic leaf.

**Independent Test**: `cargo +stable test --workspace --test root_selection_us1_multi_module_workspace` passes. The test scans the `multi_module_go_workspace/` fixture (3 nested `go.mod` files with one at the fixture root) and asserts the emitted CDX `metadata.component.purl`, SPDX 2.3 `documentDescribes`'s root package PURL, and SPDX 3 `rootElement`'s `software_packageUrl` all carry the root-module's PURL.

### Implementation for US1

- [ ] T014 [US1] Build the `multi_module_go_workspace/` fixture at `mikebom-cli/tests/fixtures/root_selection/multi_module_go_workspace/`. Layout: `go.mod` at root with `module example.com/otelshape`, plus `cmd/builder/go.mod` (`module example.com/otelshape/cmd/builder`) and `pkg/configprovider/go.mod` (`module example.com/otelshape/pkg/configprovider`). Each `go.mod` has a minimal `go 1.22` directive and zero requires.
- [ ] T015 [US1] Wire the new selector into the CDX `metadata.component` emission at `mikebom-cli/src/generate/cyclonedx/metadata.rs:269-309`. Replace the existing inline ladder with a single call to `root_selector::select_root(...)`. Use the returned `subject` to populate `metadata.component.name` / `.version` / `.purl` / `.bom-ref` exactly as today's correct-path output would.
- [ ] T016 [US1] Wire the new selector into the SPDX 2.3 `documentDescribes` selection at `mikebom-cli/src/generate/spdx/document.rs::root_id` (and any related `synthesize_root_with_override`-adjacent code paths). Replace inline selection with `root_selector::select_root(...)`.
- [ ] T017 [US1] Wire the new selector into the SPDX 3 `rootElement` selection at `mikebom-cli/src/generate/spdx/v3_document.rs` (around the existing milestone-053 single-main-module promotion logic). Replace inline selection with `root_selector::select_root(...)`.
- [ ] T018 [US1] Add the integration test at `mikebom-cli/tests/root_selection_us1_multi_module_workspace.rs`. Scan the `multi_module_go_workspace/` fixture, assert CDX `metadata.component.purl == "pkg:golang/example.com/otelshape@v0.0.0-unknown"` (no git tag → fallback version is fine for the test), assert SPDX 2.3 `documentDescribes`-resolved package's `externalRefs[purl]` matches, assert SPDX 3 root element's `software_packageUrl` matches. Use the `tempfile::tempdir()` + `Command::new(env!("CARGO_BIN_EXE_mikebom"))` pattern from `identifiers_root_purl_control.rs`.

**Checkpoint**: At end of Phase 3 the otel-collector SC-001 fixture passes end-to-end. SC-005 (cross-format consistency) verified for the US1 fixture. Other user stories not yet integrated, but US1 is independently shippable as an MVP slice.

---

## Phase 4: User Story 2 (P1) — Polyglot repo prefers Go main-module over Maven/npm sub-projects

**Goal**: SC-002 — `mikebom sbom scan --path .` on a polyglot repo (argo-workflows shape: Go at root + Maven sub-project + npm sub-project) produces an SBOM whose root identifies the Go main-module.

**Independent Test**: `cargo +stable test --workspace --test root_selection_us2_polyglot` passes. The test scans the `polyglot_go_maven_npm/` fixture and asserts the Go main-module is selected as the SBOM root across all three formats.

### Implementation for US2

- [ ] T019 [P] [US2] Build the `polyglot_go_maven_npm/` fixture at `mikebom-cli/tests/fixtures/root_selection/polyglot_go_maven_npm/`. Layout: `go.mod` at root (`module example.com/polyglot`), `java-client/pom.xml` (minimal valid pom with `<groupId>example.com</groupId>` `<artifactId>polyglot-java-tests</artifactId>` `<version>0.0.0</version>`), `ui/package.json` (minimal valid `{"name": "polyglot-ui", "version": "1.0.0"}`).
- [ ] T020 [P] [US2] Add the integration test at `mikebom-cli/tests/root_selection_us2_polyglot.rs`. Scan the `polyglot_go_maven_npm/` fixture, assert across all three formats that the root is `pkg:golang/example.com/polyglot@v0.0.0-unknown`. Verify that the maven `pom.xml` AND npm `package.json` are still emitted as `components[]` entries (not dropped) — the feature changes selection, not detection.
- [ ] T020a [US2] Add an override-wins regression test as a second test function in `mikebom-cli/tests/root_selection_us2_polyglot.rs`. Scan the `polyglot_go_maven_npm/` fixture with `--root-name "polyglot-overridden" --root-purl-type "generic" --root-version "9.9.9"` and assert: (a) CDX `metadata.component.purl == "pkg:generic/polyglot-overridden@9.9.9"`, (b) SPDX 2.3 root package's `externalRefs[purl]` matches, (c) SPDX 3 root element's `software_packageUrl` matches, (d) NO `mikebom:root-selection-heuristic` annotation is emitted in any of the three formats (per FR-006 + FR-008: operator override suppresses the heuristic annotation). Implements SC-006.
- [ ] T021 [US2] Build the `go_subdir_no_root_module/` edge-case fixture at `mikebom-cli/tests/fixtures/root_selection/go_subdir_no_root_module/`. Layout: no root-level `go.mod`; instead `services/api/go.mod` (`module example.com/sub/services/api`) and `services/worker/go.mod` (`module example.com/sub/services/worker`). LCP of the two manifest paths is `services/`, which doesn't match any go.mod, so the LCP tiebreaker fails and the ladder falls through to placeholder.
- [ ] T022 [US2] Extend `mikebom-cli/tests/root_selection_us2_polyglot.rs` with a test case for the `go_subdir_no_root_module/` fixture. Assert: (a) no `is_workspace_root: true` main-module exists, (b) LCP produces no clear winner, (c) the ladder emits the `pkg:generic/<fixture-basename>@0.0.0` placeholder, (d) the `mikebom:root-selection-heuristic` annotation carries `"heuristic": "synthetic-placeholder"`, `"confidence": 0.30`, (e) stderr contains the FR-007 warning naming both loser PURLs.

**Checkpoint**: At end of Phase 4 the argo-workflows SC-002 and the LCP-edge-case scenarios both pass. US1 + US2 can ship together as the two-bug-class fix; the codepath is the same selector module.

---

## Phase 5: User Story 3 (P2) — Transparency annotation + warning emission

**Goal**: SC-004 + SC-007 — every scan whose root selection used a tiebreaker carries the document-scope `mikebom:root-selection-heuristic` annotation with both the heuristic name AND confidence value; every fall-through past a detected main-module emits the FR-007 warning naming losers.

**Independent Test**: `cargo +stable test --workspace --test root_selection_us3_heuristic_annotation` passes. The test reuses the US1 and US2 fixtures and asserts on (a) the annotation JSON shape per contracts/annotation-schema.md, (b) the stderr warning text shape per contracts/cli-behavior.md, (c) byte-identity preservation on single-main-module fixtures.

### Implementation for US3

- [ ] T023 [P] [US3] In `mikebom-cli/src/generate/cyclonedx/metadata.rs`, emit the `mikebom:root-selection-heuristic` document-scope annotation as a `metadata.properties[]` entry per contracts/annotation-schema.md when `RootSelectionResult.heuristic.is_some()`. Suppress when `heuristic.is_none()` (fast path OR override). Use the existing `mikebom-annotation/v1` envelope helper (look for prior art at the milestone-119 `mikebom:supplement-cdx` emit site for the canonical JSON object shape).
- [ ] T024 [P] [US3] In `mikebom-cli/src/generate/spdx/document.rs`, emit the SPDX 2.3 document-level `annotations[]` entry with `annotationType: "OTHER"` and the same JSON envelope per contracts/annotation-schema.md when `heuristic.is_some()`.
- [ ] T025 [P] [US3] In `mikebom-cli/src/generate/spdx/v3_document.rs`, emit the SPDX 3 top-level annotation per contracts/annotation-schema.md when `heuristic.is_some()`.
- [ ] T026 [US3] In `mikebom-cli/src/cli/scan_cmd.rs` at the scan-end summary point (near the existing `tracing::info!("scan complete components=…")` line), emit the FR-007 warning whenever `RootSelectionResult.heuristic.is_some()` AND `losers.is_empty() == false`. The warning text shape per contracts/cli-behavior.md: `WARN  mikebom::generate::root_selector: root-component selected via "<heuristic-name>" heuristic (confidence <value>); operator override recommended for deterministic identity` plus the structured fields `selected`, `losers`, `hint`.
- [ ] T027 [US3] Add the integration test at `mikebom-cli/tests/root_selection_us3_heuristic_annotation.rs`. Covers: (a) JSON shape conformance to the contracts/annotation-schema.md envelope across all three formats, (b) confidence values match the data-model.md table for each heuristic, (c) the stderr warning text shape matches contracts/cli-behavior.md, (d) `cargo_workspace/` fixture scan (T028 below) produces zero annotation churn (count==1 fast path for a single workspace root with N members).
- [ ] T028 [US3] Build the `cargo_workspace/` fixture at `mikebom-cli/tests/fixtures/root_selection/cargo_workspace/`. Layout: root `Cargo.toml` with `[workspace] members = ["alpha", "beta"]` and `[package] name = "cargo-shape" version = "0.1.0"`; `alpha/Cargo.toml` (`name = "alpha" version = "0.1.0"`); `beta/Cargo.toml` (`name = "beta" version = "0.1.0"`). Tests the FR-003 ecosystem-priority case for an all-cargo polyglot (or the repo-root fast path if the workspace root counts as the single main-module — verify with the existing milestone-064 cargo behavior).

**Checkpoint**: At end of Phase 5 SC-004, SC-005, and SC-007 verified end-to-end. The full feature is functionally complete.

---

## Phase 6: Byte-identity regression + cross-cutting polish

**Purpose**: Prove SC-003 (zero regression on the 33 alpha.48 goldens), wire FR-011 (`--bind-to-source` follows the new root), update documentation, and ship the parity-catalog C-row.

- [ ] T029 Add the integration test at `mikebom-cli/tests/root_selection_byte_identity.rs`. Iterate every fixture under `mikebom-cli/tests/fixtures/golden/`, run the scan, and assert that the emitted CDX / SPDX 2.3 / SPDX 3 outputs are byte-identical to the committed golden. NO `MIKEBOM_UPDATE_*` env vars set. Implements SC-003 as a dedicated regression target so US1/US2 contributors can iterate against it without polluting the main regression suites.
- [ ] T030 In `mikebom-cli/src/cli/scan_cmd.rs`, locate the `--bind-to-source` wire-up code path (around the existing `bind_source_ctx` variable). Replace whatever produces the current `SourceDocumentBinding.subject` with `root_selector::select_root(...)`-derived subject. Per FR-011: the binding follows the new heuristic; no freeze-old-behavior shim.
- [ ] T031 [P] Add the new C-row to `docs/reference/sbom-format-mapping.md` (the milestone-005 / milestone-071 parity catalog). Use the next free integer per research R7 (working assumption C69; verify against the catalog at edit time). Carry the full Principle V audit narrative: native-field surveys done in research R1, parity-bridge justification.
- [ ] T032 [P] Update `docs/user-guide/cli-reference.md`'s `mikebom sbom scan` section with a new subsection documenting the new behavior: when the heuristic fires, the annotation + warning + the override-recommendation hint. Cross-link to issues #366 and #367 for the bug context.
- [ ] T033 [P] Update `CHANGELOG.md`'s `[Unreleased]` section with a behavior-change entry describing: (a) what changed (smarter root selection on polyglot + multi-module workspaces), (b) what's preserved byte-identically (single-main-module fast path; all 33 alpha.48 goldens), (c) the `--bind-to-source` behavior change footnote for argo-workflows + otel-collector operators, (d) closes #366 and #367.
- [ ] T034 Run `./scripts/pre-pr.sh` (per CLAUDE.md mandatory pre-PR gate) and verify both `cargo +stable clippy --workspace --all-targets -- -D warnings` and `cargo +stable test --workspace` pass clean. If `clippy::unwrap_used` fires on any new test code, add the `#[cfg_attr(test, allow(clippy::unwrap_used))]` guard per the existing convention.
- [ ] T035 Run `./scripts/regen-goldens.sh` (per milestone-126 wrapper) and verify `git status` shows NO golden churn. Any churn means SC-003 was broken during integration — debug before merging. Implements the milestone-126 lesson: a workspace-wide regen is the canonical zero-regression gate.

---

## Dependencies & Execution Order

```text
Phase 1 (Setup): T001, T002 [P]
   ↓
Phase 2 (Foundational): T003..T008 [P] (six per-ecosystem annotations)
                        → T009 (Maven dedup, depends on T008)
                        → T010, T011 [P] (selector skeleton)
                        → T012 (selector logic, depends on T003..T011)
                        → T013 (selector unit tests, depends on T012)
   ↓
Phase 3 (US1): T014 [P] (fixture, independent)
               → T015, T016, T017 [P] (three emitter wirings, independent of each other; depend on T012)
               → T018 (US1 integration test, depends on T014..T017)
   ↓
Phase 4 (US2): T019 [P] (fixture, independent)
               → T020, T020a [P] (US2 integration tests, depend on T019 + T015..T017)
               → T021 [P] (edge fixture, independent)
               → T022 (LCP edge test, depends on T021)
   ↓
Phase 5 (US3): T023, T024, T025 [P] (three annotation emit wirings, independent; depend on T015..T017 having landed)
               → T026 (warning emit, depends on T012)
               → T028 [P] (cargo fixture, independent)
               → T027 (US3 integration test, depends on T023..T026 + T028)
   ↓
Phase 6 (Polish):
   T029 (byte-identity regression test, depends on all of Phase 3–5)
   T030 (--bind-to-source wire, depends on T012)
   T031, T032, T033 [P] (docs, independent)
   T034 (pre-PR gate, depends on everything compile-affecting)
   T035 (golden regen sanity, depends on T034 passing)
```

**Story dependencies**: US1 and US2 are independent slices of the same selector logic — once Phase 2 completes, each can land separately if desired (e.g., US1 first as the MVP, US2 in a follow-up PR). US3 depends on US1+US2's emitter wirings landing, then layers the annotation + warning on top.

## Parallel execution opportunities

**Phase 2 fan-out (T003..T008)**: Six per-ecosystem readers, each independent. With a fan-out of six concurrent edits, this phase compresses to one person-task-duration instead of six.

**Phase 3 emitter wiring (T015, T016, T017)**: Three format emitters, each independent. Same fan-out story.

**Phase 5 annotation emitters (T023, T024, T025)**: Three format emitters, each independent.

**Phase 6 docs (T031, T032, T033)**: Three different markdown files, no overlap.

## Shipping strategy

This feature ships as ONE coordinated PR covering all three user stories plus Phase 6 polish. The MVP-split temptation (US1 alone for the otel-collector fix, US2 + US3 later) is REJECTED on Constitution Principle X grounds: shipping US1 without US3 means the new heuristic fires silently — no annotation, no warning — on every multi-main-module scan. The behavior change becomes invisible to operators, which violates Principle X's mandate that "transparency metadata MUST inform the consumer of the limitation."

The selector module (`generate/root_selector.rs`) is a single source of truth; the annotation + warning emission (US3) is the inseparable transparency layer over the selector's behavior change. They land together or not at all.

Phase 6's byte-identity gate (T029 + T035) verifies that the SC-003 zero-regression guarantee holds across all 33 alpha.48 goldens before merge. Phase 6's `--bind-to-source` wiring (T030) closes the FR-011 cross-tier binding semantics gap. The full PR is reviewable as one diff thanks to the per-phase ordering: a reviewer can walk Foundational → US1 → US2 → US3 → Polish linearly and check the contract at each boundary.

## Format validation

All 35 tasks above strictly follow `- [ ] T### [P?] [Story?] Description with file path`. Setup (T001..T002) and Foundational (T003..T013) and Polish (T029..T035) phases carry NO `[Story]` label. US-phase tasks (T014..T028) all carry `[US1]`, `[US2]`, or `[US3]`. Every task description names at least one absolute or workspace-relative file path.
