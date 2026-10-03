---
description: "Task list for 1064 — Pants resolves owned and named across both language namespaces"
---

# Tasks: Pants resolves are owned and named across both language namespaces

**Input**: Design documents from `/specs/1064-pants-resolve-namespaces/`
**Prerequisites**: plan.md, spec.md, research.md (R1–R7), data-model.md, contracts/resolve-ownership.md, contracts/anchor-identity.md, quickstart.md

**Tests**: Included. Every user story in the spec defines an independent test, and the success criteria (SC-001–SC-006) are verified by crate tests and corpus goldens.

**Organization**: Grouped by user story. Anchor identity (R1) and the shared qualified-name merge are foundational: User Story 1's JVM anchors would merge with Python anchors in a collision repository without them (spec US2 rationale; plan "Delivery order").

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: US1–US4 map to the spec's user stories
- Paths are relative to the repository root

---

## Phase 1: Setup

**Purpose**: Fixtures shared by several stories. Synthetic coordinates only (`waybill-fixture-*`, `dev.waybill.fixture`).

- [ ] T001 [P] Create fixture `waybill-cli/tests/fixtures/pants_coursier_jvm/implicit_default/`: a `pants.toml` with a `[jvm]` table (no `[jvm.resolves]`), plus `3rdparty/jvm/default.lock` whose metadata header lists `dev.waybill.fixture:app:1.0.0` in `generated_with_requirements`, with entries for `app@1.0.0 → lib@1.0.0` and `lib@1.0.0` in the coursier coord-table shape used by `pants_coursier_jvm/two_versions_two_resolves/`
- [ ] T002 [P] Create fixture `waybill-cli/tests/fixtures/pants_coursier_jvm/tool_lockfile/`: a `pants.toml` with `[jvm.resolves] main = "3rdparty/jvm/main.lock"` and `[junit] lockfile = "3rdparty/jvm/testing.lock"` (a name the heuristic does not match), plus both lockfiles with one `dev.waybill.fixture` entry each and `generated_with_requirements` naming it
- [ ] T003 [P] Create fixture `waybill-cli/tests/fixtures/pants_pex/implicit_default_python/`: a `pants.toml` with `[python] enable_resolves = true` and no `resolves` key, plus `3rdparty/python/default.lock` (pex JSON with `requirements: ["waybill-fixture-core==1.0.0"]` and one locked requirement), modelled on `pants_pex/dotted_requirement_name/`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Namespace-qualified anchor identity (R1) and the merged, qualified ownership statement (R4) that every story builds on.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [ ] T004 Add `pub(crate) fn qualified_name(ns: LanguageNamespace, name: &str) -> String` returning `<namespace>:<name>` (the C163 form, reusing `write_namespace`'s wire strings) to `waybill-cli/src/scan_fs/package_db/pants_resolve.rs`, with unit tests for both namespaces
- [ ] T005 Change the Python anchor PURL in `waybill-cli/src/scan_fs/package_db/pants/lockfile.rs::resolve_component_entry` from `pkg:generic/<name>` to `pkg:generic/<name>?pants-namespace=python` per `contracts/anchor-identity.md`; keep `name` and every annotation unchanged
- [ ] T006 Make the resolve-mode anchor lookup in `waybill-cli/src/generate/split.rs` (around line 340, `c.purl.name() == resolve && c.purl.ecosystem() == "generic"`) also require the component's `waybill:pants-resolve-namespace` to equal the group's namespace (read through `pants_resolve::read_namespace`), per anchor-identity invariant 3
- [ ] T007 Add a `PantsResolveSummary` merge in `waybill-cli/src/scan_fs/package_db/pants/mod.rs`: a constructor taking per-namespace parts (namespace, declared names, discovered names, weak count, unanchored count) that qualifies names with `qualified_name`, lexically sorts and deduplicates the lists, and sums the counts; `as_wire_value` keeps the four keys of `contracts/resolve-ownership.md`
- [ ] T008 Route the Python reader's summary through the T007 merge at `waybill-cli/src/scan_fs/package_db/mod.rs:1811`, so that Python-only repositories emit qualified names (`python:<name>`) while the statement stays absent when no lockfile is found (FR-014)
- [ ] T009 Update existing assertions on the Python anchor PURL and C161 value to the qualified forms in `waybill-cli/tests/pants_resolve_anchoring_m868.rs`, `waybill-cli/tests/pants_resolve_ownership.rs`, `waybill-cli/tests/pants_pex_reader.rs`, `waybill-cli/tests/pants_split_identity.rs`, `waybill-cli/tests/pants_split_identity_absence.rs` and `waybill-cli/tests/pants_split_resolve.rs`; every changed expectation must be the contract form, not whatever the code now prints
- [ ] T010 Update the #925 layer-1 invariant `resolve-anchor-reaches-pants` in `waybill-cli/tests/corpus_harness_195/layer1_assertions.rs` to match `pkg:generic/pants-2.31?pants-namespace=python`

**Checkpoint**: Python behaviour is unchanged except identity and name qualification. `./scripts/pre-pr.sh` is green before Phase 3.

---

## Phase 3: User Story 1 — A JVM Pants repository states and anchors its resolves (Priority: P1) 🎯 MVP

**Goal**: Declared JVM resolves get owning components wired to their declared top-level requirements, and JVM resolves contribute to the one repository-wide ownership statement.

**Independent Test**: Scan a JVM-only repository with `[jvm.resolves]`. The statement is present with `jvm:` names, one anchor exists per declared resolve depending on exactly its `generated_with_requirements` packages, and the root's direct-dependency counts are equal across CycloneDX, SPDX 2.3 and SPDX 3.

### Tests for User Story 1

- [ ] T011 [P] [US1] Add test `jvm_declared_resolve_gets_an_anchor_wired_to_its_top_levels` in `waybill-cli/tests/pants_coursier_jvm_reader.rs` against fixture `two_versions_two_resolves`: two anchors `pkg:generic/java17?pants-namespace=jvm` and `pkg:generic/java21?pants-namespace=jvm`, each depending on exactly its own `app@N`
- [ ] T012 [P] [US1] Add test `jvm_only_repository_carries_an_ownership_statement` in `waybill-cli/tests/pants_coursier_jvm_reader.rs`: C161 equals `{"declared":["jvm:java17","jvm:java21"],"discovered":[],"unanchored_lockfiles":0,"weak_classification":2}` in all three formats
- [ ] T013 [P] [US1] Add test `jvm_root_edges_agree_across_formats` in `waybill-cli/tests/pants_coursier_jvm_reader.rs`, emitting all three formats and asserting equal root out-edge counts (FR-013) using the counting approach of `corpus_harness_195::layer1_assertions` (`cdx_root_out_edges` and its SPDX siblings)
- [ ] T014 [P] [US1] Add a test in `waybill-cli/tests/pants_coursier_jvm_reader.rs` with a lockfile whose `generated_with_requirements` is empty: the anchor exists with no `depends`, and the resolve is still listed (spec edge case)

### Implementation for User Story 1

- [ ] T015 [US1] Stop discarding `PantsMetadata::generated_with_requirements` in `waybill-cli/src/scan_fs/package_db/pants_jvm/lockfile.rs` (the `let _ = …` at line 226); expose it on the parsed lockfile and add `fn top_level_coordinates(&self) -> Vec<String>` returning `group:artifact` (text before the first `,`, first two `:` fields), sorted and deduplicated, with unit tests including the `,url=…,jar=…` suffix form
- [ ] T016 [US1] Add `pub(crate) fn resolve_component_entry` to `waybill-cli/src/scan_fs/package_db/pants_jvm/lockfile.rs`, mirroring `pants/lockfile.rs::resolve_component_entry`: PURL `pkg:generic/<name>?pants-namespace=jvm`, empty version, `depends` = `top_level_coordinates()`, `depends_ecosystem = Some("maven")`, annotations `waybill:component-kind = lockfile-resolve`, `waybill:pants-resolve = [<name>]`, `waybill:pants-resolve-namespace = jvm`, and `waybill:resolve-classification-source`, with `lifecycle_scope` from `pants_jvm::resolve_classifier`
- [ ] T017 [US1] In `waybill-cli/src/scan_fs/package_db/pants_jvm/mod.rs`, record for each discovered lockfile whether `[jvm.resolves]` declared it (`DiscoveredLockfile` gains a declaration field per data-model `Declaration`); emit the T016 anchor only for declared resolves; build the JVM `PantsResolveSummary` part (declared/discovered names, weak count, unanchored count); change `read`/`finalize` to return `(Vec<PackageDbEntry>, Option<summary part>)`, with `None` when no lockfile was found
- [ ] T018 [US1] Thread the JVM summary part from `pants_jvm::finalize` (`waybill-cli/src/scan_fs/package_db/mod.rs:2826-2832`, the shared-walker pilot) into the T007 merge alongside the Python part, so one statement covers both namespaces
- [ ] T019 [US1] Verify research R6 on `waybill-cli/tests/fixtures/pants_namespace_collision/` and a local scan of `enragedginger/pants_backend_clojure` @ `e068ffb`: the root reaches every JVM anchor in all three formats with no change to the emitters. If it does not, stop and record the finding in `specs/1064-pants-resolve-namespaces/research.md` R6 before changing an emitter

**Checkpoint**: A JVM-only repository has a statement and owned packages. US1 is independently verifiable through T011–T014.

---

## Phase 4: User Story 2 — Polyglot statement covers both namespaces without ambiguity (Priority: P1)

**Goal**: Same-named resolves across namespaces stay distinct everywhere: statement, anchors and split.

**Independent Test**: Scan `pants_namespace_collision`. There are three distinct anchors (two named `default`), the statement lists `jvm:default`, `python:default` and `python:lint`, and `--split=resolve` still produces one document per qualified resolve with each document's anchor in its own namespace.

### Tests for User Story 2

- [ ] T020 [P] [US2] Add test `same_named_resolves_get_distinct_anchors` in `waybill-cli/tests/pants_namespace_split.rs`: unsplit scan of `pants_namespace_collision` has anchors `pkg:generic/default?pants-namespace=python`, `pkg:generic/default?pants-namespace=jvm` and `pkg:generic/lint?pants-namespace=python`, each owning only its own namespace's packages
- [ ] T021 [P] [US2] Add test `ownership_statement_lists_both_defaults` in `waybill-cli/tests/pants_namespace_split.rs`: C161 equals the `pants_namespace_collision` example in `contracts/resolve-ownership.md`
- [ ] T022 [P] [US2] Extend the existing split tests in `waybill-cli/tests/pants_namespace_split.rs` to assert that each `--split=resolve` document's main-module is the anchor of that document's namespace (exercises T006), that filenames are unchanged, and that every split document's C161 is the repository-wide value (FR-010)

### Implementation for User Story 2

- [ ] T023 [US2] Confirm that dedup (`waybill-cli/src/scan_fs/mod.rs`, the m922 namespace-aware merge) keeps the two `default` anchors apart now that their PURLs differ by qualifier; if any dedup key strips qualifiers, key it on the full PURL and add the case to T020

**Checkpoint**: The collision fixture is correct in the unsplit and split outputs.

---

## Phase 5: User Story 3 — Pants built-in defaults are named as Pants names them (Priority: P2)

**Goal**: With `pants.toml` present and no `resolves` table for a language, the default-path lockfile is `jvm-default` / `python-default` and declared (R2).

**Independent Test**: Scan `implicit_default` (T001) and `implicit_default_python` (T003). Membership reads `jvm-default` / `python-default`, the statement lists them under `declared`, and an anchor exists. Fixtures without `pants.toml` (`pants_pex/multi_resolve`) are byte-identical to before.

### Tests for User Story 3

- [ ] T024 [P] [US3] Add test `unconfigured_jvm_default_is_named_jvm_default_and_declared` in `waybill-cli/tests/pants_coursier_jvm_reader.rs` against fixture `implicit_default`
- [ ] T025 [P] [US3] Add test `unconfigured_python_default_is_named_python_default_and_declared` in `waybill-cli/tests/pants_pex_reader.rs` against fixture `implicit_default_python`
- [ ] T026 [P] [US3] Add test `no_pants_toml_keeps_stem_names` in `waybill-cli/tests/pants_pex_reader.rs`: `pants_pex/multi_resolve` still reports `{"declared":[],"discovered":["python:default","python:mypy","python:pytest"],…}`. Names are qualified per T008; nothing else changes
- [ ] T027 [P] [US3] Add test `explicit_resolves_table_disables_the_builtin_default` in `waybill-cli/tests/pants_coursier_jvm_reader.rs`: with `[jvm.resolves]` configured and an extra `3rdparty/jvm/default.lock`, that lockfile stays `discovered` as `jvm:default`

### Implementation for User Story 3

- [ ] T028 [P] [US3] Expose "language `resolves` table absent" from both config parsers: `waybill-cli/src/scan_fs/package_db/pants_jvm/config.rs` (distinguish an absent `[jvm.resolves]` from an empty one) and `waybill-cli/src/scan_fs/package_db/pants/config.rs` (the same for `[python].resolves`)
- [ ] T029 [US3] Apply R2 in `waybill-cli/src/scan_fs/package_db/pants_jvm/mod.rs::discover_lockfiles`: when `pants.toml` exists and `[jvm.resolves]` is absent, `3rdparty/jvm/default.lock` gets the name `jvm-default` and the `PantsDefault` declaration
- [ ] T030 [US3] Apply R2 in `waybill-cli/src/scan_fs/package_db/pants/mod.rs` (`discover_lockfiles` plus the `declared_resolve_names` computation near line 442): when `pants.toml` exists and `[python].resolves` is absent, `3rdparty/python/default.lock` gets the name `python-default`, is added to the declared set, and is anchored

**Checkpoint**: `pants-example-jvm`'s shape (no `[jvm.resolves]`) yields `jvm:jvm-default`, declared and anchored.

---

## Phase 6: User Story 4 — JVM tool lockfiles are classified by their declaration (Priority: P3)

**Goal**: A `[<scope>].lockfile` path naming a discovered JVM lockfile declares a tool resolve: named after the scope, anchored, `Development` scope, classification source `Declared` (R5).

**Independent Test**: Scan `tool_lockfile` (T002). The `testing.lock` packages are development-scope under resolve `junit`, the resolve is declared and anchored, and the weak-classification count is 1 (only `main`).

### Tests for User Story 4

- [ ] T031 [P] [US4] Add test `tool_lockfile_is_declared_development_scope` in `waybill-cli/tests/pants_coursier_jvm_reader.rs` against fixture `tool_lockfile`: every `testing.lock` package has lifecycle scope development and membership `["junit"]`, the anchor `pkg:generic/junit?pants-namespace=jvm` exists, and C161 equals `{"declared":["jvm:junit","jvm:main"],"discovered":[],"unanchored_lockfiles":0,"weak_classification":1}`
- [ ] T032 [P] [US4] Add unit tests in `waybill-cli/src/scan_fs/package_db/pants_jvm/config.rs` showing that a `lockfile = "<default>"` value and a path that doesn't exist both declare nothing

### Implementation for User Story 4

- [ ] T033 [US4] Parse every `pants.toml` table other than `jvm` and `python` for a string `lockfile` key in `waybill-cli/src/scan_fs/package_db/pants_jvm/config.rs`, returning `(scope, path)` pairs (keep the parser fail-open per FR-004)
- [ ] T034 [US4] In `waybill-cli/src/scan_fs/package_db/pants_jvm/mod.rs`, match those paths against discovered lockfiles (resolved against `scan_root`, compared as paths). A match not already `Configured` takes the scope as its name and the `ToolLockfile` declaration; a `Configured` match keeps its name and gains declared-by-tool classification (data-model precedence)
- [ ] T035 [US4] Add `classify_resolve_with_source(name, declared_by_tool)` to `waybill-cli/src/scan_fs/package_db/pants_jvm/resolve_classifier.rs`, returning `(Development, Declared)` when declared by a tool and `(classify_resolve(name), HeuristicOrDefault)` otherwise, mirroring `pants/resolve_classifier.rs:64-75`; use it for both package entries and anchors

**Checkpoint**: All four stories are functional and verified by their own tests.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [ ] T036 Run `./scripts/pre-pr.sh`, save the log to a file, and confirm `EXIT=0`, the `>>> all pre-PR checks passed.` line, and zero non-`0 failed` result lines
- [ ] T037 Regenerate public-corpus goldens per `docs/development/refreshing-corpus-goldens.md`: two `regen_goldens=true` runs on the branch, `diff -r` identical, then `xtask corpus-diff` against committed goldens. Expect changes only in `pants-example-python`, `pants-example-django`, `pants-example-jvm` and `pants-clojure-polyglot`, and attribute each category to R1–R5; the other 13 targets must show `no semantic change` (SC-004)
- [ ] T038 Install the goldens (`rsync -a --delete`), then in `waybill-cli/tests/corpus_harness_195/layer1_assertions.rs` remove `pants-clojure-polyglot` from `KNOWN_SPDX3_ROOT_EDGE_DIVERGENCE` and its pin in `known_spdx3_divergence_is_still_present` if its counts now agree (SC-002); if they don't, record the measured counts and the reason in research.md R6
- [ ] T039 [P] Run the in-repo golden regeneration sweep (`WAYBILL_UPDATE_CDX_GOLDENS=1 WAYBILL_UPDATE_SPDX_GOLDENS=1 WAYBILL_UPDATE_SPDX3_GOLDENS=1` over the six golden-writing suites) and confirm the rewritten files are byte-identical (R7)
- [ ] T040 [P] Add an Unreleased entry to `CHANGELOG.md` describing the consumer-visible changes (FR-015): qualified C161 names, JVM statement and anchors, anchor PURL qualifier, `jvm-default` / `python-default` naming, and the supersession of m912 SC-006
- [ ] T041 [P] Update the C161 description in the parity catalogue documentation (the file that documents catalogue rows, found via `grep -rn "C161" docs/`) to v2 per `contracts/resolve-ownership.md`, and the anchor description wherever `docs/` documents `lockfile-resolve` components
- [ ] T042 Run `specs/1064-pants-resolve-namespaces/quickstart.md` end to end and record the actual outputs next to its expected ones
- [ ] T043 Post on #1106 the final anchor-identity form as shipped, and close #924 by linking the merged PR

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: none. T001–T003 are parallel.
- **Foundational (Phase 2)**: depends on Setup only for fixture availability in later tests; T004 → T007 → T008; T005 → T006; T009–T010 after T005/T008. **Blocks all stories.**
- **US1 (Phase 3)**: after Phase 2. T015 → T016 → T017 → T018 → T019.
- **US2 (Phase 4)**: after Phase 2 and after T016/T017, because the collision fixture's JVM `default` needs a JVM anchor to test distinctness.
- **US3 (Phase 5)**: after Phase 2 (T029 needs the T017 declaration field; T030 is independent of US1).
- **US4 (Phase 6)**: after T017 (declaration field) and T016 (anchors).
- **Polish (Phase 7)**: after every story targeted for this delivery.

### Within Each User Story

Tests are written first and fail against the pre-change code, then the implementation lands, then the checkpoint is re-verified.

### Parallel Opportunities

- T001, T002 and T003 (fixtures in different directories).
- Within US1: T011–T014 (same file, but independent test functions, so they can be drafted together and committed once).
- T028's two config files.
- T039, T040 and T041 in Polish.

---

## Parallel Example: User Story 1

```bash
# Draft all US1 tests together (they fail until T015–T018 land):
Task: "T011 jvm_declared_resolve_gets_an_anchor_wired_to_its_top_levels in waybill-cli/tests/pants_coursier_jvm_reader.rs"
Task: "T012 jvm_only_repository_carries_an_ownership_statement in waybill-cli/tests/pants_coursier_jvm_reader.rs"
Task: "T013 jvm_root_edges_agree_across_formats in waybill-cli/tests/pants_coursier_jvm_reader.rs"
Task: "T014 empty generated_with_requirements case in waybill-cli/tests/pants_coursier_jvm_reader.rs"
```

---

## Implementation Strategy

### MVP (Phases 1–3)

Setup → Foundational (identity plus qualified statement) → US1. That delivers the #924 core: JVM repositories get a statement and owned packages, and nothing collides because identity is already qualified. Validate with T011–T014 and the gate.

### Incremental Delivery

1. Phases 1–2 as one PR. Python anchors qualify and C161 names qualify; corpus goldens change for the Python Pants targets only.
2. US1 + US2 as one PR (JVM anchors and statement, collision verified).
3. US3 as one PR (Pants default names).
4. US4 as one PR (tool lockfiles).
5. Polish runs inside each PR for its own corpus delta (regenerate twice, attribute), with T040–T043 in the last one.

Each PR regenerates goldens through the CI lane rather than locally (rule zero of the corpus guide).
