# Tasks: Maven + Gradle optional-dependency classification (m184)

**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md) · **Research**: [research.md](./research.md) · **Data model**: [data-model.md](./data-model.md)

**Delivery slice** (per plan.md): both USs ship in one PR. Per-format independence (no shared code between the two readers) — either US could ship alone but the pattern is well-established (m179+ family). Estimated ~26 tasks across 5 phases.

**Zero new production Cargo dependencies** — reuses m179's `LifecycleScope::Optional`, m180's `apply_lifecycle_scope_to_edges`, and the C122 parity extractor infrastructure verbatim. C122 docstring at `parity/extractors/cdx.rs:866` requires ZERO edit — both new values (`maven-optional-element`, `gradle-compile-only`) are already pre-committed as placeholders since m179.

## Phase 1: Setup

- [X] T001 Verify current branch is `184-maven-gradle-optional` and working tree is clean at `/Users/mlieberman/Projects/mikebom`; confirm base is main HEAD post-m183 merge (commit `f7f48a5` / `impl(183): pip / poetry / uv optional-dependency classification (#537)`)
- [X] T002 Verify the m179/m180/m181/m183 helpers m184 will reuse exist and compile: `grep -n 'apply_lifecycle_scope_to_edges\|LifecycleScope::Optional' /Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/mod.rs` — expect the classifier at line 1261+ and the caller at line 805; also verify the C122 catalog row at `mikebom-cli/src/parity/extractors/mod.rs:545`; confirm the C122 docstring at `mikebom-cli/src/parity/extractors/cdx.rs:866` already lists `maven-optional-element` + `gradle-compile-only` as expected values via `grep -n 'maven-optional-element\|gradle-compile-only' /Users/mlieberman/Projects/mikebom/mikebom-cli/src/parity/extractors/cdx.rs` — expect two matches

## Phase 2: User Story 1 — Maven `<optional>true</optional>` classification (P1)

**Goal**: `<dependency>` blocks with `<optional>true</optional>` in `pom.xml` classify as `LifecycleScope::Optional` instead of the current silent-mis-classification as `Runtime`. Scope-derived classifications (Test / Build via `<scope>` element) win over optional per Decision 2.

**Independent Test**: Scan a Maven project with at least one `<dependency>` having `<optional>true</optional>` and no explicit `<scope>` (or `<scope>compile</scope>`). Verify (a) target component gets `Optional` scope + `mikebom:optional-derivation = "maven-optional-element"`, (b) CDX emits `scope: "excluded"`, (c) SPDX 2.3 emits `OPTIONAL_DEPENDENCY_OF`.

### 2a. Struct extension + parser handler

- [X] T003 [US1] Extend the `PomDependency` struct at `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/maven.rs:578` with a new field `pub optional: bool`. Update the docstring above the struct (if any) to note the m184 origin. Do NOT default it via `#[derive(Default)]` — every construction site needs to be audited explicitly per T004
- [X] T004 [US1] Audit + update every `PomDependency { .. }` construction site to initialize `optional: false` (the default). Grep pattern: `grep -rn 'PomDependency\s*{' /Users/mlieberman/Projects/mikebom/mikebom-cli/src/` — expect ONE production site at `maven.rs:798` (parse_pom_xml) + potentially other test-fixture sites throughout the test suite. Each site MUST initialize the new field
- [X] T005 [US1] Extend `parse_pom_xml` walker at `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/maven.rs:689` with an `<optional>` element handler. Analogous to the existing `<scope>` / `<type>` handlers at lines 761-768: add a local `let mut dep_optional: Option<String> = None;` near the other `dep_*` locals, add `"optional" => dep_optional = Some(current_text.clone()),` to the match arm inside the `if parent == "dependency"` block, and at the `if popped == "dependency"` block (line 786), initialize the new field as `optional: dep_optional.take().map(|v| v.eq_ignore_ascii_case("true")).unwrap_or(false)`. Also clear `dep_optional = None;` in the else-branch fallback at line 813 (matches the pattern used for `dep_v`, `dep_scope`, `dep_type`)

### 2b. Classifier extension

- [X] T006 [US1] Modify `pom_dep_to_entry` at `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/maven.rs:2347` per data-model.md §3 US1 code block. Full transformation:
    1. Replace the single line `lifecycle_scope: lifecycle_scope_from_maven(dep.scope.as_deref()),` at line 2399 with a `let (lifecycle_scope, is_m184_optional) = match (base_scope, dep.optional) { ... };` computation matching the data-model.md decision matrix.
    2. Add a `mut extra_annotations` block before the `Some(PackageDbEntry {` construction: initialize as `Default::default()`, then if `is_m184_optional`, insert `mikebom:optional-derivation = "maven-optional-element"`.
    3. Change the existing `extra_annotations: Default::default(),` line (line 2420) to `extra_annotations,` to consume the new variable.

  Implementation guidance: mirror the m183 US1 poetry.rs pattern (annotation-at-push-time, no shared helper). The scope-wins-over-optional precedence (Decision 2) falls out naturally from the match structure: Test / Build match arms return `(scope, false)` — no annotation

### 2c. Unit tests

- [X] T007 [US1] Add 2 parser-level unit tests to `maven.rs::tests` (colocate with existing `parse_pom_xml` tests): `parse_pom_xml_extracts_optional_true` (inline pom.xml with `<optional>true</optional>` — verify `PomDependency.optional == true`), `parse_pom_xml_optional_false_or_absent_stays_false` (inline pom.xml with `<optional>false</optional>` on one dep, no `<optional>` on another — both `PomDependency.optional == false`). Reuse the existing `parse_pom_xml(bytes)` invocation pattern from tests around `maven.rs:2233` / `maven.rs:2518` / `maven.rs:3078`
- [X] T008 [US1] Add 5 classifier-level unit tests to `maven.rs::tests` covering `pom_dep_to_entry`: `pom_dep_to_entry_optional_true_default_scope_classifies_as_optional` (US1 acceptance 1+2), `pom_dep_to_entry_optional_true_scope_test_stays_test` (US1 acceptance 4 + Decision 2 test-wins pin — verifies annotation is NOT emitted), `pom_dep_to_entry_optional_true_scope_provided_stays_build` (Decision 2 provided-wins pin), `pom_dep_to_entry_optional_false_stays_runtime` (US1 acceptance 3 regression pin), `pom_dep_to_entry_optional_absent_stays_runtime` (regression pin). Each test constructs a `PomDependency` value directly + calls `pom_dep_to_entry(&dep, &doc, source_path, include_dev, cache)` and asserts on the returned `PackageDbEntry.lifecycle_scope` + `extra_annotations`. Use `PomXmlDocument::default()` for the doc arg and `None` for the cache arg where applicable

## Phase 3: User Story 2 — Gradle `compileOnly` classification (P1)

**Goal**: `gradle.lockfile` entries with the compile-only shape (`*compileClasspath` present + `*runtimeClasspath` absent) classify as `LifecycleScope::Optional`. `buildscript-gradle.lockfile` entries preserve the existing `LifecycleScope::Build` classification per Decision 2 buildscript-wins.

**Independent Test**: Scan a Gradle project whose `gradle.lockfile` has at least one entry `org.example:lombok:1.18.30=compileClasspath,testCompileClasspath` (or similar compile-only shape). Verify (a) target gets `Optional` scope + `mikebom:optional-derivation = "gradle-compile-only"` + preserves the existing `mikebom:gradle-configurations` annotation, (b) CDX emits `scope: "excluded"`, (c) SPDX 2.3 emits `OPTIONAL_DEPENDENCY_OF`.

### 3a. Shape-detection helper

- [X] T009 [US2] Add the `is_compile_only_shape` helper to `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/gradle/lockfile.rs` per data-model.md §3 US2 code block. Signature: `fn is_compile_only_shape(configs: &str) -> bool`. Suffix-check both `compileClasspath` and `runtimeClasspath` per Decision 3. Place adjacent to the existing `BUILDSCRIPT_FILENAME` const near line 33 for locality with other module-level helpers
- [X] T010 [US2] Add 5 unit tests for `is_compile_only_shape` to the existing `gradle/lockfile.rs::tests` module (find via `grep -n '#\[cfg(test)\]\|^mod tests' /Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/gradle/lockfile.rs`): `is_compile_only_shape_detects_compile_only` (input `"compileClasspath,testCompileClasspath"` → true), `is_compile_only_shape_rejects_compile_and_runtime` (input `"compileClasspath,runtimeClasspath"` → false), `is_compile_only_shape_rejects_runtime_only` (input `"runtimeClasspath,testRuntimeClasspath"` → false), `is_compile_only_shape_detects_test_compile_only` (input `"testCompileClasspath"` alone → true per Decision 3 suffix-match), `is_compile_only_shape_detects_source_set_variants` (input `"main_compileClasspath,debug_compileClasspath"` → true — custom source set names)

### 3b. Classifier extension in `read_gradle_lockfile`

- [X] T011 [US2] Modify `read_gradle_lockfile` at `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/gradle/lockfile.rs:38` per data-model.md §3 US2 code block. Change the `let lifecycle_scope = if is_buildscript { Some(LifecycleScope::Build) } else { None };` at lines 56-60 to consult `is_compile_only_shape` when not-buildscript:
    ```rust
    let lifecycle_scope = if is_buildscript {
        Some(LifecycleScope::Build)
    } else if is_compile_only_shape(configs_value) {
        Some(LifecycleScope::Optional)
    } else {
        None
    };
    ```

  Also insert the derivation annotation into `extra_annotations` at line 118 (where `mikebom:gradle-configurations` is inserted) — ONLY when the shape check fires AND `!is_buildscript` (per Decision 2 buildscript-wins guard). Order: `mikebom:gradle-configurations` first (preserves pre-m184 shape for consumers), `mikebom:optional-derivation` second when applicable

  Note: `configs_value` is computed AFTER the `lifecycle_scope` decision in the current code flow. Refactor so the `is_compile_only_shape` check consumes `configs.trim()` — the raw string parsed off the `=`-split at line 71. Verify `cargo +stable check -p mikebom` compiles clean after this edit

### 3c. Classifier unit tests

- [X] T012 [US2] Add 3 classifier-level unit tests to `gradle/lockfile.rs::tests`: `read_gradle_lockfile_compile_only_classifies_as_optional` (US2 acceptance 1+2 end-to-end — write a temp `gradle.lockfile` with `com.example:lombok:1.18.30=compileClasspath,testCompileClasspath` + verify emitted entry has `LifecycleScope::Optional` + both annotations), `read_gradle_lockfile_buildscript_compile_only_stays_build` (US2 acceptance 5 + Decision 2 buildscript-wins pin — same shape but the file is `buildscript-gradle.lockfile` → classification stays `Build`, NO derivation annotation), `read_gradle_lockfile_runtime_stays_none` (regression pin: `compileClasspath,runtimeClasspath` shape stays pre-m184 unchanged — `lifecycle_scope == None`, NO derivation annotation)

  Use the existing `read_gradle_lockfile(path)` API + `tempfile::tempdir()` seeding pattern to mirror the m106 US1 `configurations_recorded_in_annotation` test at ~line 260 for setup consistency

## Phase 4: Polish & Cross-Cutting Concerns

### 4a. Integration fixtures

- [~] T013 [P] **Deferred to follow-up milestone**: external fixture directories (maven-optional + gradle-compile-only) are hosted in the sibling `mikebom-test-fixtures` repo per project memory `project_test_fixture_stayset`. Adding new fixture directories requires a cross-repo PR, deferred here per the m183 precedent. **Coverage replaced by**: (a) the m184 unit tests at `maven.rs::tests` (7 tests: 2 parser + 5 classifier), `gradle/lockfile.rs::tests` (8 tests: 5 helper + 3 classifier); (b) the existing maven/gradle fixture goldens' regen (T014-T016) — expected byte-identical drift unless the existing fixtures happen to contain m184 signals (`<optional>true</optional>` in the maven pom or `*compileClasspath` without `*runtimeClasspath` in the gradle lockfile), which the classifier will surface as additive-only changes
- [~] T014 [P] **Deferred** — same rationale as T013. SC-002 gradle filter-parity signal is covered at unit-test level by `read_gradle_lockfile_compile_only_classifies_as_optional`

### 4b. Golden regeneration (SC-003, SC-004, SC-005)

- [X] T015 Regenerate CDX 1.6 goldens: `MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo +stable test --workspace`. Expected drift: (a) additive changes on `maven.cdx.json` ONLY IF the existing fixture at `mikebom-cli/tests/fixtures/` (external repo) happens to include `<optional>true</optional>` on any `<dependency>` block — check the existing pom.xml fixture content; (b) additive changes on `bazel.cdx.json` if it contains any `<optional>` (bazel uses different toolchain semantics — unlikely to fire); (c) additive changes on any gradle-lockfile fixture that happens to include `*compileClasspath`-without-`*runtimeClasspath` entries; (d) ZERO drift on every non-Maven / non-Gradle golden (per SC-004). Verify (d) via `git diff --stat mikebom-cli/tests/fixtures/golden/cyclonedx/` post-regen — non-Java files MUST show `0 changed`
- [X] T016 Regenerate SPDX 2.3 goldens: `MIKEBOM_UPDATE_SPDX_GOLDENS=1 cargo +stable test --workspace`. Expected drift: net-INCREMENT in `*_DEPENDENCY_OF` counts on maven / gradle goldens IF the underlying fixtures contain m184 signals; NET-DECREMENT MUST be zero on any golden per SC-003. Verify via `git diff` inspection
- [X] T017 Regenerate SPDX 3.0.1 goldens: `MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo +stable test --workspace`. Expected drift: additive changes on maven / gradle goldens IF underlying fixtures contain m184 signals; ZERO drift on non-Java goldens per FR-011 / SC-005

### 4c. Documentation

- [X] T018 Update `/Users/mlieberman/Projects/mikebom/docs/reference/reading-a-mikebom-sbom.md` — add `maven-optional-element` and `gradle-compile-only` entries to the derivation-value list, with the same shape as m183's `pip-optional-dependencies` entry (which added a sub-bulleted list covering three sources). Since Maven and Gradle emit DISTINCT values (per Decision 1), each gets its own paragraph:
    - `maven-optional-element` — Maven `<dependency><optional>true</optional></dependency>` in `pom.xml` (POM 4.0.0 spec: transitive-exposure control)
    - `gradle-compile-only` — Gradle `compileOnly` deps (compile-only shape in `gradle.lockfile`: `*compileClasspath` present + `*runtimeClasspath` absent)

  Update the "Milestone" attribution paragraph to note m184 covers Maven + Gradle. Deferred Erlang/sbt lines shift to m185+/m186+ attribution

### 4d. Verification gates

- [X] T019 Run walker-audit allow-list check locally per project memory `feedback_walker_audit_local_check`: use the exact bash block from the m183 T021 body (grep + suffix-strip + diff against `mikebom-cli/src/scan_fs/walk.audit-allowlist.txt`) — m184 introduces ZERO new walker functions (all changes are Maven pom-parser + gradle-lockfile classifier + one new module-level helper `is_compile_only_shape`); expected exit 0
- [X] T020 Run the mandatory pre-PR gate: `/Users/mlieberman/Projects/mikebom/scripts/pre-pr.sh` — MUST report `>>> all pre-PR checks passed.` before commit. Per project memory `feedback_prepr_gate_full_output`, capture the per-target `N passed; 0 failed` lines from the output as verification evidence

### 4e. SC-008 C122 parity verification

- [X] T021 [P] After T015-T017 land, verify SC-008: `mikebom:optional-derivation` values appear byte-identically across all three format goldens. Command: `for val in maven-optional-element gradle-compile-only; do echo "=== $val ==="; for fmt in cyclonedx/*.cdx.json spdx-2.3/*.spdx.json spdx-3/*.spdx3.json; do count=$(grep -c "\"$val\"" "/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/fixtures/golden/$fmt" 2>/dev/null || echo "0"); echo "$fmt: $count"; done; done`. For each value, the per-fixture per-format count MUST be equal across all three formats (CDX / SPDX 2.3 / SPDX 3) — proves C122 SymmetricEqual propagation. Absent from all three is also fine (means no fixture exercised the signal); mismatched counts (present in one but not others) is a failure

### 4f. FR-011 backward-compat pin

- [X] T022 [P] After T015-T017 land, verify FR-011: `git diff --stat mikebom-cli/tests/fixtures/golden/` — every non-Maven / non-Gradle golden MUST show `0 changed`. Non-Java fixtures MUST be byte-identical to pre-m184 (SC-004 regression guard). If any non-Java golden shows drift, investigate immediately — likely indicates the m184 classifier is incorrectly firing outside the intended scope.
    - **FR-008 sub-check** (per /speckit-analyze R2 finding U1): add a targeted unit test in `maven.rs::tests` and/or `gradle/lockfile.rs::tests` that scans a fixture (inline pom.xml or gradle.lockfile respectively) containing at least one m184-Optional-classified entry with `include_dev=false` passed through and asserts the Optional-classified target is FILTERED at emit time via m179's `is_non_runtime()` extension. Mirrors the m183 T024 R1 remediation pattern. This closes the /speckit-analyze U1 gap where FR-008 previously had no explicit test at the m184 layer. Placement suggestion: co-locate one test each in `pom_dep_to_entry_*` tests (US1) and `read_gradle_lockfile_*` tests (US2) — or, alternatively, an end-to-end integration test via `Command::new(env!("CARGO_BIN_EXE_mikebom"))` seeded with a tempdir fixture is equally acceptable

### 4g. Filter-parity gates (SC-001, SC-002)

- [X] T023 [P] Verify SC-001 set-equality for any maven fixture that exercised m184 (post-T015-T016). Extract the SET of PURLs marked `scope: "excluded"` in the maven CDX golden AND the SET of PURLs appearing as source-side of `OPTIONAL_DEPENDENCY_OF` / `TEST_DEPENDENCY_OF` / `BUILD_DEPENDENCY_OF` in the maven SPDX 2.3 golden. Set equality gate per m179/m180/m181/m183 SC-001. Skip this task if no maven golden shows drift post-regen (means no fixture exercised m184; unit-test level coverage in T008 is sufficient)
- [X] T024 [P] Same set-equality verification for any gradle-lockfile fixture that exercised m184 (post-T015-T016). Skip if no gradle golden shows drift; unit-test level coverage in T012 is sufficient

### 4h. SC-006 basic-mode preservation

- [X] T025 [P] After T015-T017 land, verify SC-006 by running `cargo +stable test -p mikebom --test optional_dep_classification` — the existing m179 test suite covers basic-mode collapse for any component with `LifecycleScope::Optional`, inherited by m184 without new code. Expected: existing tests pass; no new test added because m184 doesn't touch emission code paths

### 4i. FR-013 zero-new-dep verification

- [X] T026 [P] Verify FR-013 (zero new production Cargo dependencies) explicitly per the m183 T029 pattern. Command: `git stash && cargo tree -p mikebom | wc -l > /tmp/m184-tree-pre.txt && git stash pop && cargo tree -p mikebom | wc -l > /tmp/m184-tree-post.txt && diff /tmp/m184-tree-pre.txt /tmp/m184-tree-post.txt`. Expected: identical line counts. If nonzero delta, investigate the added dep — expected to be zero because m184 only touches source files inside `mikebom-cli/src/scan_fs/package_db/{maven,gradle}/`; no `Cargo.toml` edit is proposed in any m184 task

## Dependencies

- **T001 → T002** (Setup) MUST complete before any other work.
- **T003 → T004** (US1 struct + audit) — sequential because T004 depends on T003's field addition.
- **T003 → T005** (US1 struct + parser) — T005 populates the field T003 added.
- **T003 → T006** (US1 struct + classifier) — T006 consumes the field T003 added.
- **T005 → T007** (US1 parser + parser tests) — sequential.
- **T006 → T008** (US1 classifier + classifier tests) — sequential.
- **T009 → T010** (US2 helper + helper tests) — sequential.
- **T009 → T011** (US2 helper + classifier wiring) — sequential (classifier consumes helper).
- **T011 → T012** (US2 classifier + classifier tests) — sequential.
- **T013, T014** (integration fixtures) — deferred; no task-graph dependency.
- **T015 → T016 → T017** (golden regens — sequential per project convention).
- **T018** (docs) — independent, can land any time after T006 + T011.
- **T019** (walker audit) — independent, can run any time.
- **T020** (pre-PR gate) — requires ALL preceding tasks to have landed.
- **T021, T022, T023, T024, T025, T026** (verification gates) — after T015-T017.

## Parallel Execution Examples

**Phase 2 (US1) can run entirely in parallel with Phase 3 (US2) — no shared code**:
- US1 series: T003 → T004 → T005 → T006 → T007 → T008 (all touch `maven.rs`)
- US2 series: T009 → T010 → T011 → T012 (all touch `gradle/lockfile.rs`)
- The two series are file-independent and can be developed simultaneously by different implementers or in parallel commits

**Phase 4 polish**:
- T015 → T016 → T017 (golden regens) — sequential
- T018 (docs), T019 (walker audit) — parallel with each other
- T021-T026 (verification gates) — parallel with each other; must run AFTER T015-T017

## Implementation Strategy

**MVP scope (this milestone)**: Both USs + polish = 26 tasks. Both ship in one PR per plan.md. Per-format independence means either US could ship alone — but the pattern is established (m179+ cadence), so single-PR bundle is the target.

**Recommended commit cadence** — ~5 small commits on the branch:
1. T001-T002 (setup)
2. T003-T008 (US1 Maven — struct + parser + classifier + 7 tests; grouped because they cumulatively edit `maven.rs`)
3. T009-T012 (US2 Gradle — helper + classifier + 8 tests; grouped because they cumulatively edit `gradle/lockfile.rs`)
4. T015-T018 (polish: goldens + docs)
5. T019-T026 (verification + pre-PR)

**Fallback** (if implementation surprises arise): US1 and US2 can land in separate commits or even separate PRs. Per-format independence makes the split trivial.

## Success Criteria Coverage

| SC | Gate | Task(s) |
|----|------|---------|
| SC-001 (Maven filter-parity) | US1 delivery + set-equality verification | T008 (unit), T015, T016, T023 (post-regen) |
| SC-002 (Gradle filter-parity) | US2 delivery + set-equality verification | T012 (unit), T015, T016, T024 (post-regen) |
| SC-003 (net-decrement zero) | Golden regen verification | T016, T022 |
| SC-004 (non-Java CDX byte-identity) | Golden regen + FR-011 pin | T015, T022 |
| SC-005 (non-Java SPDX 3 byte-identity) | Golden regen + FR-011 pin | T017, T022 |
| SC-006 (basic-mode collapse) | Inherited from m179 test infra | T025 |
| SC-007 (existing tests continue) | Pre-PR gate | T020 |
| SC-008 (C122 byte-identity across formats) | Cross-format grep | T021 |
| SC-009 (zero new Cargo dep) | `cargo tree` line-count diff | T026 |
| FR-008 (`--include-dev=false` filters Optional) | Extended T022 assertion | T022 (post-analyze R2 update) |
