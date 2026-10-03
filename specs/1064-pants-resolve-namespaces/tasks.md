---
description: "Task list for 1064 — Pants resolves owned and named across both language namespaces"
---

# Tasks: Pants resolves are owned and named across both language namespaces

**Input**: Design documents from `/specs/1064-pants-resolve-namespaces/`
**Prerequisites**: plan.md, spec.md, research.md (R1–R7), data-model.md, contracts/resolve-ownership.md, contracts/anchor-identity.md, quickstart.md

**Tests**: Included. Every user story in the spec defines an independent test, and the success criteria (SC-001–SC-006) are verified by crate tests and corpus goldens.

**Organization**: Grouped by user story, in the plan's delivery order: anchor identity first (Foundational), then naming (US3), then JVM anchors (US1). Naming precedes anchors because `pants-example-jvm`, the reference case in SC-001, configures no `[jvm.resolves]`. Without US3 its resolve is discovered rather than declared and gets no owning component.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: US1–US4 map to the spec's user stories
- Paths are relative to the repository root

---

## Phase 1: Setup

**Purpose**: Fixtures shared by several stories. Synthetic coordinates only (`waybill-fixture-*`, `dev.waybill.fixture`).

- [X] T001 [P] Create fixture `waybill-cli/tests/fixtures/pants_coursier_jvm/implicit_default/`: a `pants.toml` with a `[jvm]` table (no `[jvm.resolves]`), plus `3rdparty/jvm/default.lock` whose metadata header lists `dev.waybill.fixture:app:1.0.0` in `generated_with_requirements`, with entries for `app@1.0.0 → lib@1.0.0` and `lib@1.0.0` in the coursier coord-table shape used by `pants_coursier_jvm/two_versions_two_resolves/`
- [X] T002 [P] Create fixture `waybill-cli/tests/fixtures/pants_coursier_jvm/tool_lockfile/`: a `pants.toml` with `[jvm.resolves] main = "3rdparty/jvm/main.lock"` and `[junit] lockfile = "3rdparty/jvm/testing.lock"` (a name the heuristic does not match), plus both lockfiles with one `dev.waybill.fixture` entry each and `generated_with_requirements` naming it
- [X] T003 [P] Create fixture `waybill-cli/tests/fixtures/pants_pex/implicit_default_python/`: a `pants.toml` with `[python] enable_resolves = true` and no `resolves` key, plus `3rdparty/python/default.lock` (pex JSON with `requirements: ["waybill-fixture-core==1.0.0"]` and one locked requirement), modelled on `pants_pex/dotted_requirement_name/`
- [X] T004 [P] Create fixture `waybill-cli/tests/fixtures/pants_coursier_jvm/missing_top_level/`: `[jvm.resolves] main = "3rdparty/jvm/main.lock"`, with a lockfile whose `generated_with_requirements` names `dev.waybill.fixture:present:1.0.0` and `dev.waybill.fixture:absent:1.0.0`, where only `present` has an entry

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Namespace-qualified anchor identity (R1), the merged qualified ownership statement (R4), and the declaration model both readers share (data-model `Declaration`).

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T005 (satisfied by the existing `pants_resolve::qualify`, already tested) Add `pub(crate) fn qualified_name(ns: LanguageNamespace, name: &str) -> String` returning `<namespace>:<name>` (the C163 form, reusing `write_namespace`'s wire strings) to `waybill-cli/src/scan_fs/package_db/pants_resolve.rs`, with unit tests for both namespaces
- [X] T006 Add a `Declaration` enum (`Configured`, `PantsDefault`, `ToolLockfile`, `Discovered`) with an `is_declared()` accessor to `waybill-cli/src/scan_fs/package_db/pants_resolve.rs`, per data-model.md, with the precedence `Configured > ToolLockfile > PantsDefault > Discovered` encoded in a `fn stronger(self, other)` helper and unit-tested
- [X] T007 Carry a `Declaration` on each discovered lockfile in both readers: `DiscoveredLockfile` in `waybill-cli/src/scan_fs/package_db/pants_jvm/mod.rs` and in `waybill-cli/src/scan_fs/package_db/pants/mod.rs`. Set `Configured` for `[<lang>.resolves]` keys and `Discovered` for glob-only finds; keep behaviour identical, with the Python reader still anchoring exactly its `Configured` set
- [X] T008 Change the Python anchor PURL in `waybill-cli/src/scan_fs/package_db/pants/lockfile.rs::resolve_component_entry` from `pkg:generic/<name>` to `pkg:generic/<name>?pants-namespace=python` per `contracts/anchor-identity.md`; keep `name` and every annotation unchanged
- [X] T009 Make the resolve-mode anchor lookup in `waybill-cli/src/generate/split.rs` (around line 340, `c.purl.name() == resolve && c.purl.ecosystem() == "generic"`) also require the component's `waybill:pants-resolve-namespace` to equal the group's namespace (read through `pants_resolve::read_namespace`), per anchor-identity invariant 3
- [X] T010 Add a `PantsResolveSummary` merge in `waybill-cli/src/scan_fs/package_db/pants/mod.rs`: a constructor taking per-namespace parts (namespace, declared names, discovered names, weak count, unanchored count) that qualifies names with `qualified_name`, lexically sorts and deduplicates the lists, and sums the counts; `as_wire_value` keeps the four keys of `contracts/resolve-ownership.md`
- [X] T011 Route the Python reader's summary through the T010 merge at `waybill-cli/src/scan_fs/package_db/mod.rs:1811`, so that Python-only repositories emit qualified names (`python:<name>`) while the statement stays absent when no lockfile is found (FR-014)
- [X] T012 Update existing assertions on the Python anchor PURL and C161 value to the qualified forms in `waybill-cli/tests/pants_resolve_anchoring_m868.rs`, `waybill-cli/tests/pants_resolve_ownership.rs`, `waybill-cli/tests/pants_pex_reader.rs`, `waybill-cli/tests/pants_split_identity.rs`, `waybill-cli/tests/pants_split_identity_absence.rs` and `waybill-cli/tests/pants_split_resolve.rs`; every changed expectation must be the contract form, not whatever the code now prints
  - *Also, found by the T042 gate:* `scan_pants_m672.rs` asserted an unqualified anchor PURL. Separately, fixture `pants_discovered_resolves` (m868/m912) was meant to be a repository with no declared resolves, but it had `pants.toml` plus `3rdparty/python/default.lock`, which Pants itself declares as `python-default` (R2). Its `default.lock` was renamed to `app.lock` so the fixture stays all-discovered, and expectations in `pants_resolve_ownership.rs`, `pants_split_identity.rs`, `pants_split_identity_formats.rs` and `pants_split_resolve.rs` were renamed `default` → `app`. The lexical order is unchanged, so their counts and order assertions mean what they did before.
- [X] T013 Update the #925 layer-1 invariant `resolve-anchor-reaches-pants` in `waybill-cli/tests/corpus_harness_195/layer1_assertions.rs` to match `pkg:generic/pants-2.31?pants-namespace=python`

**Checkpoint**: Python behaviour is unchanged except identity and name qualification. `./scripts/pre-pr.sh` is green before Phase 3.

---

## Phase 3: User Story 3 — Pants built-in defaults are named as Pants names them (Priority: P2)

**Goal**: With `pants.toml` present and no `resolves` table for a language, the default-path lockfile is `jvm-default` / `python-default` with declaration `PantsDefault` (FR-007, R2). Done before US1 so that US1's anchors cover `pants-example-jvm` (SC-001).

**Independent Test**: Scan `implicit_default` (T001) and `implicit_default_python` (T003). Membership reads `jvm-default` / `python-default`, the statement lists them under `declared`, and the Python one has an anchor (the JVM anchor follows in US1). Fixtures without `pants.toml` (`pants_pex/multi_resolve`) keep stem names.

### Tests for User Story 3

- [X] T014 [P] [US3] Add test `unconfigured_jvm_default_is_named_jvm_default_and_declared` in `waybill-cli/tests/pants_coursier_jvm_reader.rs` against fixture `implicit_default`: membership `["jvm-default"]`, and C161 lists `jvm:jvm-default` under `declared`
- [X] T015 [P] [US3] Add test `unconfigured_python_default_is_named_python_default_and_declared` in `waybill-cli/tests/pants_pex_reader.rs` against fixture `implicit_default_python`: membership `["python-default"]`, `declared` includes `python:python-default`, and the anchor `pkg:generic/python-default?pants-namespace=python` exists
- [X] T016 [P] [US3] Add test `no_pants_toml_keeps_stem_names` in `waybill-cli/tests/pants_pex_reader.rs`: `pants_pex/multi_resolve` still reports `{"declared":[],"discovered":["python:default","python:mypy","python:pytest"],…}`. Names are qualified per T011; nothing else changes
- [X] T017 [P] [US3] Add test `explicit_resolves_table_disables_the_builtin_default` in `waybill-cli/tests/pants_coursier_jvm_reader.rs`: with `[jvm.resolves]` configured and an extra `3rdparty/jvm/default.lock`, that lockfile stays `discovered` as `jvm:default`

### Implementation for User Story 3

- [X] T018 [P] [US3] (implemented once, as `pants_resolve::builtin_default_applies`, which reads the raw `pants.toml`; an inline `resolves = {}` counts as configured) Expose "language `resolves` table absent" from both config parsers: `waybill-cli/src/scan_fs/package_db/pants_jvm/config.rs` (distinguish an absent `[jvm.resolves]` from an empty one) and `waybill-cli/src/scan_fs/package_db/pants/config.rs` (the same for `[python].resolves`)
- [X] T019 [US3] Apply R2 in `waybill-cli/src/scan_fs/package_db/pants_jvm/mod.rs::discover_lockfiles`: when `pants.toml` exists and `[jvm.resolves]` is absent, `3rdparty/jvm/default.lock` gets the name `jvm-default` and declaration `PantsDefault`; build the JVM summary part from declarations (declared vs discovered names, counts) and merge it via T010 (statement only; anchors come in US1)
- [X] T020 [US3] Apply R2 in `waybill-cli/src/scan_fs/package_db/pants/mod.rs` (`discover_lockfiles` plus the `declared_resolve_names` computation near line 442): when `pants.toml` exists and `[python].resolves` is absent, `3rdparty/python/default.lock` gets the name `python-default` and declaration `PantsDefault`, is counted declared, and is anchored
- [X] T021 [US3] Thread the JVM summary part from `pants_jvm::finalize` (`waybill-cli/src/scan_fs/package_db/mod.rs:2826-2832`, the shared-walker pilot) into the T010 merge alongside the Python part; change `pants_jvm::read`/`finalize` to return `(Vec<PackageDbEntry>, Option<summary part>)` and update every caller (`grep -rn "pants_jvm::read\|pants_jvm::finalize" waybill-cli/`)

**Checkpoint**: Default names are Pants-correct in both namespaces, and JVM resolves appear in C161.

---

## Phase 4: User Story 1 — A JVM Pants repository states and anchors its resolves (Priority: P1) 🎯 MVP

**Goal**: Every declared JVM resolve (`Configured`, `PantsDefault`, and later `ToolLockfile`) gets an owning component wired to its declared top-level requirements.

**Independent Test**: Scan a JVM-only repository. The statement is present with `jvm:` names, there is one anchor per declared resolve depending on exactly its `generated_with_requirements` packages, and the root's direct-dependency counts are equal across CycloneDX, SPDX 2.3 and SPDX 3.

### Tests for User Story 1

- [X] T022 [P] [US1] Add test `jvm_declared_resolve_gets_an_anchor_wired_to_its_top_levels` in `waybill-cli/tests/pants_coursier_jvm_reader.rs` against fixture `two_versions_two_resolves`: two anchors `pkg:generic/java17?pants-namespace=jvm` and `pkg:generic/java21?pants-namespace=jvm`, each depending on exactly its own `app@N`
- [X] T023 [P] [US1] Add test `jvm_only_repository_carries_an_ownership_statement` in `waybill-cli/tests/pants_coursier_jvm_reader.rs`: C161 equals `{"declared":["jvm:java17","jvm:java21"],"discovered":[],"unanchored_lockfiles":0,"weak_classification":2}` in all three formats
- [X] T024 [P] [US1] Add test `jvm_root_edges_agree_across_formats` in `waybill-cli/tests/pants_coursier_jvm_reader.rs`, emitting all three formats and asserting equal root out-edge counts (FR-013) using the counting approach of `corpus_harness_195::layer1_assertions` (`cdx_root_out_edges` and its SPDX siblings)
- [X] T025 [P] [US1] Add a test in `waybill-cli/tests/pants_coursier_jvm_reader.rs` with a lockfile whose `generated_with_requirements` is empty: the anchor exists with no `depends`, and the resolve is still listed (spec edge case)
- [X] T026 [P] [US1] Add test `top_level_missing_from_its_lockfile_is_dropped_not_rewired` in `waybill-cli/tests/pants_coursier_jvm_reader.rs` against fixture `missing_top_level`: the anchor depends on `present` only, no edge reaches any other resolve's package, and `waybill:unresolved-declared-dep` on the anchor names `dev.waybill.fixture:absent` (spec edge case)
- [X] T027 [P] [US1] Add test `implicit_default_jvm_resolve_is_anchored` in `waybill-cli/tests/pants_coursier_jvm_reader.rs` against fixture `implicit_default`: anchor `pkg:generic/jvm-default?pants-namespace=jvm` depends on `app@1.0.0` (the SC-001 shape)

### Implementation for User Story 1

- [X] T028 [US1] Stop discarding `PantsMetadata::generated_with_requirements` in `waybill-cli/src/scan_fs/package_db/pants_jvm/lockfile.rs` (the `let _ = …` at line 226); expose it on the parsed lockfile and add `fn top_level_coordinates(&self) -> Vec<String>` returning `group:artifact` (text before the first `,`, first two `:` fields), sorted and deduplicated, with unit tests including the `,url=…,jar=…` suffix form
- [X] T029 [US1] Add `pub(crate) fn resolve_component_entry` to `waybill-cli/src/scan_fs/package_db/pants_jvm/lockfile.rs`, mirroring `pants/lockfile.rs::resolve_component_entry`: PURL `pkg:generic/<name>?pants-namespace=jvm`, empty version, `depends` = `top_level_coordinates()`, `depends_ecosystem = Some("maven")`, annotations `waybill:component-kind = lockfile-resolve`, `waybill:pants-resolve = [<name>]`, `waybill:pants-resolve-namespace = jvm`, and `waybill:resolve-classification-source`, with `lifecycle_scope` from `pants_jvm::resolve_classifier`
- [X] T030 [US1] In `waybill-cli/src/scan_fs/package_db/pants_jvm/mod.rs::read`, emit the T029 anchor for every lockfile whose declaration `is_declared()`, and none for `Discovered`
- [X] T031 [US1] Verify research R6 on `waybill-cli/tests/fixtures/pants_namespace_collision/` and a local scan of `enragedginger/pants_backend_clojure` @ `e068ffb`: the root reaches every JVM anchor in all three formats with no change to the emitters. If it does not, stop and record the finding in `specs/1064-pants-resolve-namespaces/research.md` R6 before changing an emitter

**Checkpoint**: Configured and Pants-default JVM resolves have a statement and owned packages. US1 is independently verifiable through T022–T027.

---

## Phase 5: User Story 2 — Polyglot statement covers both namespaces without ambiguity (Priority: P1)

**Goal**: Same-named resolves across namespaces stay distinct everywhere: statement, anchors and split. Its test needs US1's JVM anchors.

**Independent Test**: Scan `pants_namespace_collision`. There are three distinct anchors (two named `default`), the statement lists `jvm:default`, `python:default` and `python:lint`, and `--split=resolve` still produces one document per qualified resolve with each document's anchor in its own namespace.

### Tests for User Story 2

- [X] T032 [P] [US2] Add test `same_named_resolves_get_distinct_anchors` in `waybill-cli/tests/pants_namespace_split.rs`: unsplit scan of `pants_namespace_collision` has anchors `pkg:generic/default?pants-namespace=python`, `pkg:generic/default?pants-namespace=jvm` and `pkg:generic/lint?pants-namespace=python`, each owning only its own namespace's packages
- [X] T033 [P] [US2] Add test `ownership_statement_lists_both_defaults` in `waybill-cli/tests/pants_namespace_split.rs`: C161 equals the `pants_namespace_collision` example in `contracts/resolve-ownership.md`
- [X] T034 [P] [US2] Extend the existing split tests in `waybill-cli/tests/pants_namespace_split.rs` to assert that each `--split=resolve` document's main-module is the anchor of that document's namespace (exercises T009), that filenames are unchanged, and that every split document's C161 is the repository-wide value (FR-010)

### Implementation for User Story 2

- [X] T035 [US2] Confirm that dedup keeps the two `default` anchors apart now that their PURLs differ by qualifier: find the dedup key in `waybill-cli/src/scan_fs/mod.rs` (`grep -n "fn dedup\|dedup_key\|pants-resolve-namespace" waybill-cli/src/scan_fs/mod.rs`, the m922 namespace-aware merge); if any key strips qualifiers, key it on the full PURL and add the case to T032
  - *Done:* `resolve/deduplicator.rs::deduplicate` grouped on `(ecosystem, name, version, parent_purl)`, which merged the two `default` anchors. It now also keys on `pants_resolve::anchor_namespace` (the `pants-namespace` qualifier only, `None` for every other PURL). Keying on the whole PURL would split qualifier-only variants such as deb `?arch=` and break FR-014.

**Checkpoint**: The collision fixture is correct in the unsplit and split outputs.

---

## Phase 6: User Story 4 — JVM tool lockfiles are classified by their declaration (Priority: P3)

**Goal**: A `[<scope>].lockfile` path naming a discovered JVM lockfile declares a tool resolve: named after the scope, anchored, `Development` scope, classification source `Declared` (R5).

**Independent Test**: Scan `tool_lockfile` (T002). The `testing.lock` packages are development-scope under resolve `junit`, the resolve is declared and anchored, and the weak-classification count is 1 (only `main`).

### Tests for User Story 4

- [X] T036 [P] [US4] Add test `tool_lockfile_is_declared_development_scope` in `waybill-cli/tests/pants_coursier_jvm_reader.rs` against fixture `tool_lockfile`: every `testing.lock` package has lifecycle scope development and membership `["junit"]`, the anchor `pkg:generic/junit?pants-namespace=jvm` exists, and C161 equals `{"declared":["jvm:junit","jvm:main"],"discovered":[],"unanchored_lockfiles":0,"weak_classification":1}`
- [X] T037 [P] [US4] Add unit tests in `waybill-cli/src/scan_fs/package_db/pants_jvm/config.rs` showing that a `lockfile = "<default>"` value and a path that doesn't exist both declare nothing

### Implementation for User Story 4

- [X] T038 [US4] Parse every `pants.toml` table other than `jvm` and `python` for a string `lockfile` key in `waybill-cli/src/scan_fs/package_db/pants_jvm/config.rs`, returning `(scope, path)` pairs (keep the parser fail-open per FR-004)
- [X] T039 [US4] In `waybill-cli/src/scan_fs/package_db/pants_jvm/mod.rs`, match those paths against discovered lockfiles (resolved against `scan_root`, compared as paths) and combine declarations with T006's `stronger`. A `Discovered` or `PantsDefault` match takes the scope as its name and becomes `ToolLockfile`; a `Configured` match keeps its name and gains declared-by-tool classification
- [X] T040 [US4] Add `classify_resolve_with_source(name, declared_by_tool)` to `waybill-cli/src/scan_fs/package_db/pants_jvm/resolve_classifier.rs`, returning `(Development, Declared)` when declared by a tool and `(classify_resolve(name), HeuristicOrDefault)` otherwise, mirroring `pants/resolve_classifier.rs:64-75`; use it for both package entries and anchors
- [X] T041 [P] [US4] Add test `configured_resolve_also_declared_by_a_tool_keeps_its_name` in `waybill-cli/tests/pants_coursier_jvm_reader.rs`: `[jvm.resolves] tests = "3rdparty/jvm/tests.lock"` plus `[junit] lockfile = "3rdparty/jvm/tests.lock"` gives membership `["tests"]`, development scope, and classification source `declared` (spec edge case)

**Checkpoint**: All four stories are functional and verified by their own tests.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [X] T042 Run `./scripts/pre-pr.sh`, save the log to a file, and confirm `EXIT=0`, the `>>> all pre-PR checks passed.` line, and zero non-`0 failed` result lines
- [ ] T043 Regenerate public-corpus goldens per `docs/development/refreshing-corpus-goldens.md`: two `regen_goldens=true` runs on the branch, `diff -r` identical, then `xtask corpus-diff` against committed goldens. Expect changes only in `pants-example-python`, `pants-example-django`, `pants-example-jvm` and `pants-clojure-polyglot`, and attribute each category to R1–R5; the other 13 targets must show `no semantic change` (SC-004)
- [ ] T044 Install the goldens (`rsync -a --delete`), then in `waybill-cli/tests/corpus_harness_195/layer1_assertions.rs` remove `pants-clojure-polyglot` from `KNOWN_SPDX3_ROOT_EDGE_DIVERGENCE` and its pin in `known_spdx3_divergence_is_still_present` if its counts now agree (SC-002, FR-013). `pants-example-python` and `pants-example-django` stay on the list (#1022). If the clojure counts don't agree, record the measured counts and the reason in research.md R6
- [X] T045 Add a layer-1 invariant for `pants-example-jvm` in `waybill-cli/tests/corpus_harness_195/layer1_assertions.rs` (`pants_example_jvm_layer1`): C161 equals `{"declared":["jvm:jvm-default"],"discovered":[],"unanchored_lockfiles":0,"weak_classification":1}`, and the anchor `pkg:generic/jvm-default?pants-namespace=jvm` exists (SC-001)
- [X] T046 Add a C161 invariant to `pants_clojure_polyglot_layer1` in `waybill-cli/tests/corpus_harness_195/layer1_assertions.rs`: `declared` equals `["jvm:java17","jvm:java21","python:pants-2.30","python:pants-2.31"]` (SC-002)
- [ ] T047 [P] Run the in-repo golden regeneration sweep (`WAYBILL_UPDATE_CDX_GOLDENS=1 WAYBILL_UPDATE_SPDX_GOLDENS=1 WAYBILL_UPDATE_SPDX3_GOLDENS=1` over the six golden-writing suites) and confirm the rewritten files are byte-identical (R7)
- [X] T048 [P] Add an Unreleased entry to `CHANGELOG.md` describing the consumer-visible changes (FR-015): qualified C161 names, JVM statement and anchors, anchor PURL qualifier, `jvm-default` / `python-default` naming, and the supersession of m912 SC-006
- [X] T049 [P] Update the C161 description in the parity catalogue documentation (the file that documents catalogue rows, found via `grep -rn "C161" docs/`) to v2 per `contracts/resolve-ownership.md`, and the anchor description wherever `docs/` documents `lockfile-resolve` components
- [ ] T050 Run `specs/1064-pants-resolve-namespaces/quickstart.md` end to end and record the actual outputs next to its expected ones
- [ ] T051 Post on #1106 the final anchor-identity form as shipped, and close #924 by linking the merged PR

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: none. T001–T004 are parallel.
- **Foundational (Phase 2)**: T005 → T006 → T007; T005 → T010 → T011; T008 → T009; T012–T013 after T008/T011. **Blocks all stories.**
- **US3 (Phase 3)**: after Phase 2. T018 → T019 → T021; T018 → T020.
- **US1 (Phase 4)**: after US3, since T030 anchors every `is_declared()` lockfile, including `PantsDefault` from T019. T028 → T029 → T030 → T031.
- **US2 (Phase 5)**: after US1, because the collision fixture's JVM `default` needs a JVM anchor to test distinctness.
- **US4 (Phase 6)**: after US1 (anchors) and T006 (precedence).
- **Polish (Phase 7)**: after every story targeted for this delivery. T043 → T044 → T045, T046.

### Within Each User Story

Tests are written first and fail against the pre-change code, then the implementation lands, then the checkpoint is re-verified.

### Parallel Opportunities

- T001–T004 (fixtures in different directories).
- Within US3: T014–T017 drafted together; T018's two config files.
- Within US1: T022–T027 (one file, independent test functions).
- T047, T048 and T049 in Polish.

---

## Parallel Example: User Story 1

```bash
# Draft all US1 tests together (they fail until T028–T030 land):
Task: "T022 jvm_declared_resolve_gets_an_anchor_wired_to_its_top_levels"
Task: "T023 jvm_only_repository_carries_an_ownership_statement"
Task: "T024 jvm_root_edges_agree_across_formats"
Task: "T025 empty generated_with_requirements"
Task: "T026 top_level_missing_from_its_lockfile_is_dropped_not_rewired"
Task: "T027 implicit_default_jvm_resolve_is_anchored"
```

---

## Implementation Strategy

### MVP (Phases 1–4)

Setup → Foundational (identity, qualified statement, declarations) → US3 (Pants default names) → US1 (JVM anchors). That delivers #924's core *including* the reference case: `pants-example-jvm` (no `[jvm.resolves]`) gets `jvm:jvm-default`, declared and anchored (SC-001). Identity is already qualified, so nothing collides. Validate with T014–T017, T022–T027, and the gate.

### Incremental Delivery

1. Phases 1–2 as one PR. Python anchors qualify and C161 names qualify; corpus goldens change for the Python Pants targets only.
2. US3 + US1 as one PR. JVM statement, default names and JVM anchors land together, so `pants-example-jvm`'s goldens change once.
3. US2 as one PR (collision verified, dedup confirmed).
4. US4 as one PR (tool lockfiles).
5. Polish runs inside each PR for its own corpus delta (regenerate twice, attribute), with T045–T046 in PR 2 and T048–T051 in the last one.

Each PR regenerates goldens through the CI lane rather than locally (rule zero of the corpus guide).
