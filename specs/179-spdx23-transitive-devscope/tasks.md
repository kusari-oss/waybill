# Tasks: Unified Optional-Dependency Classification (m179)

**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md) · **Research**: [research.md](./research.md) · **Data model**: [data-model.md](./data-model.md)

**Delivery slice** (per plan.md Decision 4): US1 (pico flagship fix) + US2 (research artifact + docs) + US3 (Cargo) + core-model change + one SPDX 2.3 emitter arm. US4 (npm/yarn/pnpm) is deferred to m180; US5 (pip) to m181; US6 (Maven/Gradle) to m182; US7 (Erlang normalization) to m183. Each will get its own tasks.md.

## Phase 1: Setup

- [X] T001 Verify current branch is `179-spdx23-transitive-devscope` and working tree is clean at `/Users/mlieberman/Projects/mikebom` — `git status --short` MUST return empty output before m179 tasks begin
- [X] T002 Run baseline pre-PR gate at `/Users/mlieberman/Projects/mikebom/scripts/pre-pr.sh` and confirm it passes clean on the m178-descended tree before any m179 changes; this establishes the "before" baseline for SC-003 through SC-006 zero-drift gates

## Phase 2: Foundational (Blocking — required by both US1 and US3)

**Purpose**: Add the three new enum variants + extend the classifier dispatch table + wire the derivation-annotation emission across all three format emitters + register the parity catalog row. Every US phase downstream depends on this substrate.

### 2a. Core-model enum extensions

- [X] T003 [P] Extend `LifecycleScope` enum in `/Users/mlieberman/Projects/mikebom/mikebom-common/src/resolution.rs:370` with a new `Optional` variant; update `as_str()` at line 382 to return `"optional"`; add three unit tests to the `#[cfg(test)] mod tests` block: `lifecycle_scope_optional_serde_roundtrip` (JSON `"optional"`), `lifecycle_scope_optional_is_non_runtime` (returns `true`), `lifecycle_scope_optional_as_str` (returns `"optional"`), and `lifecycle_scope_legacy_dev_excludes_optional` (`lifecycle_scope_is_legacy_dev(&Some(Optional))` returns `false` per data-model.md §1.1)
- [X] T004 [P] Extend `RelationshipType` enum in `/Users/mlieberman/Projects/mikebom/mikebom-common/src/resolution.rs:487` with a new `OptionalDependsOn` variant; add unit test `relationship_type_optional_serde_roundtrip` (JSON `"optional_depends_on"`)
- [X] T005 [P] Extend `SpdxRelationshipType` enum in `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/generate/spdx/relationships.rs:34` with a new `OptionalDependencyOf` variant; update the `Display` impl (or equivalent `as_str()` method) to return the wire value `"OPTIONAL_DEPENDENCY_OF"`; add unit test `spdx_relationship_type_optional_wire_value`

### 2b. Classifier dispatch extension

- [X] T006 Extend `apply_lifecycle_scope_to_edges` at `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/mod.rs:1261` per data-model.md §2's dispatch table: (a) add `LifecycleScope::Optional → RelationshipType::OptionalDependsOn` to the existing match arm at line 1281-1287; (b) add a NEW second-pass block after line 1288 that scans components with `build_inclusion == Some(BuildInclusion::NotNeeded)` AND `lifecycle_scope == None`, then rewrites every `DependsOn` edge whose target is that component to `TestDependsOn` (US1 flagship semantic per Q1 answer to spec.md); wrap the new pass in a `tracing::info!` with a `not_needed_rewrites` count. Add THREE unit tests: (1) `optional_dispatch_rewrites_depends_on_to_optional_depends_on` for the Optional path; (2) `not_needed_fallthrough_rewrites_depends_on_to_test_depends_on` for the m112 fallthrough path (US1 semantic); (3) `precedence_optional_wins_over_not_needed` per FR-14 — synthesize a component with BOTH `lifecycle_scope = Some(LifecycleScope::Optional)` AND `build_inclusion = Some(BuildInclusion::NotNeeded)` and assert the incoming `DependsOn` edge is rewritten to `OptionalDependsOn` (NOT `TestDependsOn` — the Optional path runs first, the fallthrough is guarded on `lifecycle_scope = None`).
- [X] T007 Extend the SPDX 2.3 classifier's Full-mode match arm at `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/generate/spdx/relationships.rs:264-278` with a new arm `(Full, RelationshipType::OptionalDependsOn) => (to_id, from_id, SpdxRelationshipType::OptionalDependencyOf)` (reversed-direction convention per FR-008); position it alongside the existing `DevDependsOn`/`BuildDependsOn`/`TestDependsOn` arms; the Basic-mode catch-all continues to swallow it via the existing `(Basic, _) => DependsOn` arm; add unit tests `optional_depends_on_reverses_to_optional_dependency_of` (Full) and `optional_depends_on_collapses_to_depends_on_in_basic_mode` (Basic)
- [X] T008 Confirm SPDX 3 classifier at `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/generate/spdx/v3_relationships.rs:96-105` correctly falls through `RelationshipType::OptionalDependsOn` to `None` (no `lifecycleScope` parameter) via the existing `_ => None` catch-all arm; add a unit test `optional_depends_on_emits_no_lifecycle_scope_on_spdx3` that verifies emitted SPDX 3 relationship carries no `lifecycleScope` field

### 2c. Derivation annotation emission

- [X] T009 Add `mikebom:optional-derivation` component-level property emission to CDX 1.6 in `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/generate/cyclonedx/builder.rs`; site: colocate with the existing `mikebom:build-inclusion-derivation` emission block near `builder.rs:842-857`; read the value from `component.extra_annotations.get("mikebom:optional-derivation")` and emit as `component.properties[]` entry `{"name": "mikebom:optional-derivation", "value": "<string>"}`
- [X] T010 Add `mikebom:optional-derivation` `Package.annotations[]` emission to SPDX 2.3 in `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/generate/spdx/annotations.rs`; site: colocate with the existing `mikebom:build-inclusion-derivation` emission near line 243; wrap the value in the `MikebomAnnotationCommentV1` envelope (see contracts/mikebom-optional-derivation.md for exact JSON shape)
- [X] T011 Add `mikebom:optional-derivation` `spdx:Annotation` node emission to SPDX 3.0.1 in `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/generate/spdx/v3_annotations.rs`; site: colocate with the existing `mikebom:build-inclusion-derivation` emission near line 250; wrap the value in the same `MikebomAnnotationCommentV1` envelope in the `spdx:statement` field

### 2d. Parity catalog registration

- [X] T012 [P] Add CDX extractor for `mikebom:optional-derivation` in `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/parity/extractors/cdx.rs`; declare `cdx_anno!(c122_cdx, "mikebom:optional-derivation", component);` following the existing C61 (`mikebom:build-inclusion-derivation`) pattern at line 669-673
- [X] T013 [P] Add SPDX 2.3 extractor for `mikebom:optional-derivation` in `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/parity/extractors/spdx2.rs`; declare `spdx23_anno!(c122_spdx23, "mikebom:optional-derivation", component);` following the existing C61 pattern
- [X] T014 [P] Add SPDX 3 extractor for `mikebom:optional-derivation` in `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/parity/extractors/spdx3.rs`; declare `spdx3_anno!(c122_spdx3, "mikebom:optional-derivation", component);` following the existing C61 pattern
- [X] T015 Register the C122 row in the central catalog list at `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/parity/extractors/mod.rs`; insert the `ParityExtractor { row_id: "C122", label: "mikebom:optional-derivation", cdx: c122_cdx, spdx23: c122_spdx23, spdx3: c122_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false }` entry after the C121 row; include a header comment `// C122 — mikebom:optional-derivation (milestone 179). Records which ecosystem-reader mechanism populated the LifecycleScope::Optional classification. KEEP-BOTH polarity: native SPDX 2.3 OPTIONAL_DEPENDENCY_OF is primary; annotation carries derivation source.`

**Foundational checkpoint**: `cargo +stable clippy --workspace --all-targets -- -D warnings` MUST pass clean after T003–T015. All new tests MUST pass under `cargo +stable test --workspace`.

## Phase 3: User Story 1 — Pico flagship filter-parity fix (P1)

**Goal**: Close the reported pico bug — Go transitive test-only deps (m112 `build_inclusion = NotNeeded`) get emitted as SPDX 2.3 `TEST_DEPENDENCY_OF` instead of generic `DEPENDS_ON`, so consumers filtering via the SPDX 2.3 typed-verb set catch the same 23 components CDX catches via `scope: "excluded"`.

**Independent Test**: Scan a Go fixture where `go mod why` classifies at least one transitive as NotNeeded; count CDX `scope: "excluded"` PURLs and count SPDX 2.3 PURLs that appear as source-side of `TEST_DEPENDENCY_OF` / `DEV_DEPENDENCY_OF` / `BUILD_DEPENDENCY_OF` / `OPTIONAL_DEPENDENCY_OF`; both counts MUST match.

### 3a. Fixture + integration test scaffolding

- [X] T016 [US1] Create `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/optional_dep_classification.rs` — new integration test file for all m179 US1/US3 tests; add module docs + a `scan_helper` fn that runs `mikebom sbom scan --path <fixture>` and returns parsed CDX + SPDX 2.3 + SPDX 3 JSON `Value`s
- [X] T017 [US1] Add test `pico_filter_parity_yaml_v3_case` to `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/optional_dep_classification.rs`; use the existing `transitive_parity/golang` fixture (or extend it if check.v1 is not covered — see T018); implement the SC-001+SC-002 assertion per contracts/pico-filter-parity.md test signature (CDX-excluded PURL set == SPDX-typed-source PURL set)

### 3b. Fixture extension (if needed)

- [X] T018 [US1] Verify `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/fixtures/transitive_parity/golang/` (m083 fixture cache) contains a Go module whose `go.mod` transitively pulls in yaml.v3 → check.v1 (the flagship pico case); if the fixture is missing this shape, add a minimal `go.mod` + `go.sum` covering it; the fixture MUST cause m112's `go mod why` classifier to emit `build_inclusion = NotNeeded` on at least one transitive; document the expected NotNeeded count in a fixture-README

### 3c. Verify Phase 2 dispatch delivers US1

- [X] T019 [US1] Add test `not_needed_transitive_emits_test_dependency_of` to `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/mod.rs` `#[cfg(test)]` block; construct a synthetic `Vec<Relationship> + Vec<ResolvedComponent>` where a target has `build_inclusion = Some(BuildInclusion::NotNeeded)` and `lifecycle_scope = None`; call `apply_lifecycle_scope_to_edges` and assert the incoming `DependsOn` edge was rewritten to `TestDependsOn` per Phase 2 T006

## Phase 4: User Story 2 — Ecosystem survey + docs (P1)

**Goal**: Ship the research artifact (already at `specs/179-spdx23-transitive-devscope/research.md` — 28-row survey table) AND the consumer-facing docs updates so downstream ingestors know the new SPDX 2.3 semantic exists.

**Independent Test**: A reviewer can walk `docs/reference/sbom-format-mapping.md`'s new C-row + `docs/reference/reading-a-mikebom-sbom.md`'s new consumer-flow section and successfully filter a mikebom-produced SPDX 2.3 document via the documented jq recipe.

- [X] T020 [US2] Update `/Users/mlieberman/Projects/mikebom/docs/reference/sbom-format-mapping.md` — add a new C-row for `OPTIONAL_DEPENDENCY_OF` (SPDX 2.3 native) + `mikebom:optional-derivation` (annotation supplement) under the KEEP-BOTH polarity established by m178; row content: name = `OPTIONAL_DEPENDENCY_OF (m179)`, CDX = `component.scope == "excluded"` (auto via `is_non_runtime()`), SPDX 2.3 = `OPTIONAL_DEPENDENCY_OF` native relationship (reversed direction), SPDX 3 = `mikebom:optional-derivation` annotation only (no native enum value at spec 3.0.1)
- [X] T021 [US2] Update `/Users/mlieberman/Projects/mikebom/docs/reference/reading-a-mikebom-sbom.md` — add a new subsection under the "Filtering by scope" section covering: (a) the CDX `scope: "excluded"` recipe from quickstart.md, (b) the SPDX 2.3 typed-dep-scope-source recipe including the new `OPTIONAL_DEPENDENCY_OF` verb, (c) a note that under `--spdx2-relationship-compat=basic` the SPDX 2.3 recipe returns empty (m228 escape hatch), (d) an explanation of what the `mikebom:optional-derivation` annotation adds beyond the native signal

## Phase 5: User Story 3 — Cargo `optional = true` classifier (P2)

**Goal**: The Cargo reader detects `optional = true` in `[dependencies]` and populates `LifecycleScope::Optional` + `mikebom:optional-derivation = "cargo-optional-true"` on the target component, so SPDX 2.3 emits `OPTIONAL_DEPENDENCY_OF` and CDX emits `scope: "excluded"` automatically via FR-006.

**Independent Test**: Scan a Rust fixture whose `Cargo.toml` has `foo = { version = "1", optional = true }`; verify SPDX 2.3 contains `foo OPTIONAL_DEPENDENCY_OF my-app`, CDX contains `foo` with `scope: "excluded"`, and both formats carry the `mikebom:optional-derivation` annotation with value `cargo-optional-true`.

### 5a. Cargo reader extension

- [X] T022 [US3] Extend the Cargo reader at `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/cargo.rs` — when parsing `[dependencies]` table entries, detect `optional = true` per data-model.md §5 template; on match, set `entry.lifecycle_scope = Some(mikebom_common::resolution::LifecycleScope::Optional)` AND insert `entry.extra_annotations.insert("mikebom:optional-derivation".into(), serde_json::Value::String("cargo-optional-true".into()));`; skip the classifier entirely when the entry is already under `[dev-dependencies]` / `[build-dependencies]` (m052 precedence wins per FR-015)

### 5b. Cargo reader unit test

- [X] T023 [P] [US3] Add unit test `cargo_optional_true_sets_lifecycle_scope_optional` in `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/cargo.rs` `#[cfg(test)]` block; test input is a Cargo.toml with `bar = { version = "1", optional = true }`; assert `entry.lifecycle_scope == Some(LifecycleScope::Optional)` and the annotation is set
- [X] T024 [P] [US3] Add unit test `cargo_dev_dependencies_with_optional_stays_development` in the same test block; test input is a Cargo.toml with `[dev-dependencies] baz = { version = "1", optional = true }`; assert `entry.lifecycle_scope == Some(LifecycleScope::Development)` (m052 precedence) and the `mikebom:optional-derivation` annotation is NOT set

### 5c. Cargo integration test

- [X] T025 [US3] Add fixture `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/fixtures/optional_dep/cargo/` containing a minimal Rust project: `Cargo.toml` with `[package]` + `[dependencies] serde = "1"` + `[dependencies] foo = { version = "1", optional = true }` + `[features] foo-support = ["dep:foo"]`; `src/lib.rs` with a trivial function; do NOT include `Cargo.lock` (the reader doesn't need it for the manifest-based optional-flag path)
- [X] T026 [US3] Add integration test `cargo_optional_dep_end_to_end` to `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/optional_dep_classification.rs` (created in T016); scan the T025 fixture; assert CDX has `foo` with `scope: "excluded"` and `mikebom:optional-derivation` property = `cargo-optional-true`; assert SPDX 2.3 has `foo OPTIONAL_DEPENDENCY_OF my-app` relationship + matching annotation; assert SPDX 3 has `foo` component with `mikebom:optional-derivation` annotation (no native `lifecycleScope` per FR-017)
- [X] T027 [US3] Add integration test `cargo_optional_dep_basic_mode_collapses` to the same file; scan the T025 fixture with `--spdx2-relationship-compat=basic`; assert zero `OPTIONAL_DEPENDENCY_OF` edges in the emitted SPDX 2.3 (m228 escape hatch per FR-003 + SC-006); assert the `mikebom:optional-derivation` annotation IS still present (annotation is orthogonal to the relationship-type flag)

## Phase 6: Polish & Cross-Cutting

- [X] T028 Regenerate CDX 1.6 goldens: run `MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo +stable test --workspace` at repo root; review the diff to confirm ADDITIVE-ONLY changes — new `mikebom:optional-derivation` properties on the new Cargo fixture; ZERO drift on all other CDX goldens (SC-004 gate)
- [X] T029 Regenerate SPDX 2.3 goldens: run `MIKEBOM_UPDATE_SPDX_GOLDENS=1 cargo +stable test --workspace`; review the diff to confirm: (a) new `OPTIONAL_DEPENDENCY_OF` edges on the new Cargo fixture (US3), (b) new `TEST_DEPENDENCY_OF` edges on the golang transitive_parity fixture from previously-untyped m112 NotNeeded transitives (US1), (c) new `mikebom:optional-derivation` annotations on the new Cargo fixture, (d) ZERO decrement in existing `DEV_DEPENDENCY_OF` / `BUILD_DEPENDENCY_OF` / `TEST_DEPENDENCY_OF` edge counts on ANY fixture (SC-003 gate)
- [X] T030 Regenerate SPDX 3 goldens: run `MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo +stable test --workspace`; review the diff to confirm: (a) new `mikebom:optional-derivation` annotations on the new Cargo fixture, (b) ZERO drift in typed relationships or `lifecycleScope` values on any fixture — SPDX 3 emission is annotation-only for the new signal per FR-017 (SC-005 gate)
- [X] T031 Run SPDX 3 conformance validator: `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 cargo +stable test --workspace` — confirm every emitted SPDX 3 document passes JPEWdev `spdx3-validate==0.0.5` (m078 conformance gate); the new `mikebom:optional-derivation` annotation MUST NOT introduce spec-conformance issues (annotations are open-schema per SPDX 3)
- [X] T032 Run parity CI: `cargo +stable test --workspace -- parity_symmetric_equal` — confirm C122 (`mikebom:optional-derivation`) shows `SymmetricEqual` polarity holds for every fixture that exercises it (SC-008 gate)
- [X] T033 Run the walker audit allowlist check locally: `cd /Users/mlieberman/Projects/mikebom && grep -rEn "fn walk[_(]" mikebom-cli/src/scan_fs/ | grep -v 'walk\.audit-allowlist\.txt' | sort > /tmp/walk-actual.txt && diff /tmp/walk-actual.txt mikebom-cli/src/scan_fs/walk.audit-allowlist.txt` — m179 should introduce NO new walker functions (the Cargo reader change is manifest-parsing, not walking); if diff is non-empty, adjust allowlist per m117 line-stable convention
- [X] T034 Run the mandatory pre-PR gate: `/Users/mlieberman/Projects/mikebom/scripts/pre-pr.sh` — MUST report `>>> all pre-PR checks passed.` before the commit is prepared
- [X] T035 Verify the m179 quickstart: manually run the consumer-flow jq recipes from `/Users/mlieberman/Projects/mikebom/specs/179-spdx23-transitive-devscope/quickstart.md` against the newly-regenerated CDX + SPDX 2.3 goldens for the Cargo fixture; confirm both recipes return the same sorted PURL set (contract enforcement at contracts/pico-filter-parity.md)

## Dependencies

- T001–T002 (Setup) must complete before any other work.
- T003, T004, T005 (foundational enum extensions) run in parallel — different files/lines.
- T006 requires T003 + T004 (needs both new enum variants).
- T007 requires T005 + T006 (needs the SPDX enum + the internal RelationshipType variant).
- T008 requires T004 (needs `RelationshipType::OptionalDependsOn`).
- T009, T010, T011 (annotation emission per-format) run in parallel — different files, no interdependencies.
- T012, T013, T014 (parity extractors) run in parallel — different files.
- T015 requires T012 + T013 + T014 (imports the identifiers they declare).
- **Phase 2 checkpoint**: All of T003–T015 must complete before any Phase 3/4/5 tasks.
- Phase 3 (US1): T016 must complete before T017. T018 is independent of both (fixture verification/extension). T019 is independent (unit test in different file — can run in parallel with T016/T017 once T006 lands).
- Phase 4 (US2): T020 and T021 are independent — can run in parallel with Phase 3.
- Phase 5 (US3): T022 must complete before T023/T024 (unit tests exercise reader changes). T025 is independent (fixture creation — can run alongside T022). T026 requires T022 + T025 + T016 (integration test infra). T027 requires T026.
- Phase 6 polish: T028/T029/T030 run in sequence (each regenerates one format's goldens; the diff review is manual). T031/T032 can run in parallel after T030. T033/T034/T035 are the pre-commit gates — sequential.

## Parallel Execution Examples

**Phase 2 kickoff (all foundational enum work)**:
```
Launch T003, T004, T005 in parallel — three different enum extensions in different files
Then T006, T007, T008 (depend on the above)
Then T009, T010, T011 in parallel (annotation emission per format)
Then T012, T013, T014 in parallel (extractor registrations)
Then T015 (catalog list registration)
```

**Phase 3+4 parallelism** (after Phase 2 lands):
```
Launch T016 → T017 → T019 (US1 test chain) in sequence
In parallel: T018 (fixture verification), T020 (mapping doc), T021 (reading-guide doc)
```

**Phase 5 parallelism** (after T022 lands):
```
Launch T023, T024 in parallel (unit tests — different match cases in same file, so caveat: colocate in one commit)
Launch T025 in parallel with T022 (fixture creation — independent)
Then T026, T027 in sequence (integration tests share fixture)
```

**Phase 6 polish**:
```
T028 → T029 → T030 (golden regen sequence — one format per pass)
Then T031, T032 in parallel (validator + parity CI)
Then T033 → T034 → T035 (commit prep gates)
```

## Implementation Strategy

**MVP scope (this milestone, m179)**: US1 + US2 + US3 + core-model + polish = 35 tasks. All ship in a single PR against the `179-spdx23-transitive-devscope` branch.

**Recommended follow-up cadence** (each is its own milestone with its own tasks.md):
- **m180 (npm/yarn/pnpm)**: US4 — extend npm reader to detect `optionalDependencies` in package.json + propagate through package-lock.json, yarn.lock, pnpm-lock.yaml. Estimated 8-10 tasks. Guard: `peerDependenciesMeta.<name>.optional = true` MUST NOT reclassify — m178 `PROVIDED_DEPENDENCY_OF` wins.
- **m181 (pip)**: US5 — extend pip readers (pyproject.rs / setup_py.rs / setup_cfg.rs / uv_lock.rs) to detect extras. Estimated 6-8 tasks.
- **m182 (Maven + Gradle)**: US6 — Maven `<optional>true</optional>` + Gradle `compileOnly` classification. Estimated 6-8 tasks. Gradle scoping caveat: pin to `compileOnly` only for m182; annotationProcessor/other build-only configurations are already m052 `Build`.
- **m183 (Erlang normalization)**: US7 — populate `LifecycleScope::Optional` from the existing `optional_applications` detection in erlang.rs; KEEP the m141 `mikebom:erlang-app-dep-kind` annotation with byte-identical value. Estimated 3-5 tasks.

**Rationale for the split**: US1 closes the reported pico bug — the flagship user-facing fix — with the smallest possible change surface. US3 (Cargo) is the second-most-scanned ecosystem after Go and has the cleanest test signal, so delivering US1+US3 together validates the core-model design against two ecosystems (Go via NotNeeded fallthrough + Cargo via Optional dispatch) before extending to more. Each subsequent milestone (m180-m183) is a per-ecosystem-family delivery that can regress-test independently.

## Success Criteria Coverage

| SC | Gate | Task(s) |
|----|------|---------|
| SC-001 | 23=23 pico case | T017 (integration test) + T028/T029 (golden regen confirms count) |
| SC-002 | CDX excluded set == SPDX 2.3 typed-source set | T017 + T035 |
| SC-003 | No decrement in existing `*_DEPENDENCY_OF` counts | T029 (SPDX 2.3 regen review) |
| SC-004 | Zero drift on CDX goldens for un-touched fixtures | T028 (CDX regen review) |
| SC-005 | Zero drift on SPDX 3 goldens (typed relationships) | T030 (SPDX 3 regen review — annotation additions OK, typed relationships must be untouched) |
| SC-006 | Basic-mode zero new typed edges | T027 (Cargo basic-mode integration test) + T029 |
| SC-007 | Ecosystem survey covers all supported ecosystems | Already delivered by research.md (T020 references it) |
| SC-008 | `mikebom:optional-derivation` byte-identical across all 3 formats | T032 (parity CI) |
