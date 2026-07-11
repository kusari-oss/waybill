# Tasks: pip / poetry / uv optional-dependency classification (m183)

**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md) · **Research**: [research.md](./research.md) · **Data model**: [data-model.md](./data-model.md)

**Delivery slice** (per plan.md): all 3 USs ship in one PR. Shared post-pass infrastructure + converging patterns across the three code sites. Estimated ~28 tasks across 6 phases.

**Zero new production Cargo dependencies** — reuses m179's `LifecycleScope::Optional`, m180's `apply_lifecycle_scope_to_edges`, and the C122 parity extractor infrastructure verbatim.

## Phase 1: Setup

- [X] T001 Verify current branch is `183-pip-extras-optional` and working tree is clean at `/Users/mlieberman/Projects/mikebom`; confirm base is main HEAD post-alpha.58 (commit `fc6f021` / m182+m181 merged)
- [X] T002 Verify the m179/m180/m181 helpers m183 will reuse exist and compile: `grep -n 'apply_lifecycle_scope_to_edges\|LifecycleScope::Optional' /Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/mod.rs` — expect the classifier at line 1261+ and the caller at line 805; also verify the C122 catalog row at `mikebom-cli/src/parity/extractors/mod.rs:545`

## Phase 2: Foundational (Blocking — required by US1 + US2 + US3)

**Purpose**: Add the shared post-pass infrastructure + fix the pre-existing docstring placeholder. US2 introduces the pip-dispatcher post-pass that US3 also uses.

### 2a. Fix C122 catalog docstring (research Decision 1 follow-up)

- [X] T003 Update the docstring at `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/parity/extractors/cdx.rs:866` — replace the placeholder `pip-extras-require` with `pip-optional-dependencies` per contracts/derivation-value-set.md. Zero runtime behavior change; the docstring lists expected value-set entries for the C122 extractor. Confirm no other file references `pip-extras-require` via `grep -rn 'pip-extras-require' /Users/mlieberman/Projects/mikebom/mikebom-cli/src/` — expect zero matches after this edit

### 2b. Add the pip-dispatcher post-pass helper

- [X] T004 Add a private helper `apply_optional_derivation_annotation` to `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/pip/mod.rs` that takes `entries: &mut Vec<PackageDbEntry>` and `optional_names: &HashSet<String>` and, for each entry where `optional_names.contains(&entry.name) AND entry.lifecycle_scope.is_none()`, sets `entry.lifecycle_scope = Some(LifecycleScope::Optional)` + inserts `mikebom:optional-derivation = "pip-optional-dependencies"` into `entry.extra_annotations`. Signature per data-model.md §2 US2 code block. Colocate 2 unit tests in the pip/mod.rs tests module: `apply_annotation_marks_matching_and_unclassified_entries`, `apply_annotation_skips_already_classified_entries` (Decision 3 lockfile-precedence enforcement)

**Foundational checkpoint**: `cargo +stable clippy --workspace --all-targets -- -D warnings` MUST pass clean after T003-T004. The new helper is dead code at this point (no caller yet) but the `#[allow(dead_code)]` marker is NOT needed because T009 wires it in during US2.

## Phase 3: User Story 1 — poetry.lock `optional = true` classification (P1)

**Goal**: `poetry.lock` `[[package]]` entries with `optional = true` (non-dev) classify as `LifecycleScope::Optional` instead of the current silent-mis-classification as `Runtime`. Dev classification wins over optional per Decision 2.

**Independent Test**: Scan a poetry-managed project with at least one `optional = true, category = "main"` package. Verify (a) target component gets `Optional` scope + derivation annotation, (b) CDX emits `scope: "excluded"`, (c) SPDX 2.3 emits `OPTIONAL_DEPENDENCY_OF`.

### 3a. Reader classifier extension

- [X] T005 [US1] Add the `poetry_is_optional` helper to `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/pip/poetry.rs` per data-model.md §2 US1: `fn poetry_is_optional(tbl: &toml::value::Table) -> bool` returning `tbl.get("optional").and_then(|v| v.as_bool()).unwrap_or(false)`. Place adjacent to the existing `poetry_is_dev` helper (around line 147+)
- [X] T006 [US1] Modify the classifier at `poetry.rs:67` from a 2-arm match on `poetry_is_dev` to the 3-arm decision matrix from contracts/classifier-decision-matrix.md §US1. Full table:
    - `poetry_is_dev = Some(true)` → `LifecycleScope::Development` (dev wins per Decision 2 — do NOT emit the annotation)
    - `poetry_is_dev = Some(false) AND poetry_is_optional = true` → `LifecycleScope::Optional` + insert `mikebom:optional-derivation = "pip-optional-dependencies"` into the entry's `extra_annotations`
    - `poetry_is_dev = Some(false) AND poetry_is_optional = false` → `LifecycleScope::Runtime` (unchanged)
    - `poetry_is_dev = None AND poetry_is_optional = true` → `LifecycleScope::Optional` + annotation
    - `poetry_is_dev = None AND poetry_is_optional = false` → `None` (unchanged)

  Implementation guidance: extend the match block to compute `let is_dev = poetry_is_dev(tbl); let is_optional = poetry_is_optional(tbl);` and dispatch through nested match. Also update the `include_dev` skip guard (line 74) — the guard already matches on `Development`, so it correctly still filters dev+optional combos

- [X] T007 [US1] Add 5 unit tests to `poetry.rs` tests module (colocated with the existing tests around line 178+): `optional_true_non_dev_classifies_as_optional`, `optional_true_annotation_carries_pip_optional_dependencies`, `dev_classified_package_still_dev_ignoring_optional_flag` (US1 acceptance 4 + Decision 2 pin), `optional_false_stays_runtime` (regression pin), `optional_field_absent_stays_runtime` (regression pin). Reuse the existing `include_dev=true` invocation pattern from surrounding tests. Each new fixture uses inline TOML matching the shape from data-model.md §2

## Phase 4: User Story 2 — pyproject.toml `[project.optional-dependencies]` (P1)

**Goal**: PEP 621's `[project.optional-dependencies].<extra>` deps classify as `LifecycleScope::Optional` on the resulting graph edges. Currently the main-module extractor flattens both `[project.dependencies]` and `[project.optional-dependencies]` into one `depends: Vec<String>` — extras-only deps emit as false-positive Runtime edges.

**Independent Test**: Scan a project with `pyproject.toml` declaring `[project.optional-dependencies].dev = ["pytest"]` and `[project.dependencies] = ["requests"]`, no poetry / uv lockfile. Verify pytest→`Optional`, requests→`Runtime`, C122 filter-parity holds.

### 4a. Extractor helper

- [X] T008 [US2] Add the `optional_deps_from_pyproject` helper to `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/pip/mod.rs` per data-model.md §2 US2 signature: `fn optional_deps_from_pyproject(project_table: &toml::Value) -> HashSet<String>`. Walk `[project.optional-dependencies].*` tables, apply the same `take_first_token` helper already used at pip/mod.rs:461 for name extraction (PEP 508 splitting), collect into a HashSet, then subtract the `regular_direct_deps` set (built from `[project.dependencies]` via the same take_first_token) — diamond-shape Runtime-wins per FR-005. Colocate 3 unit tests: `optional_deps_from_pyproject_extracts_names` (single extra + multi extra), `optional_deps_diamond_shape_runtime_wins` (name in both project.dependencies and project.optional-dependencies.<extra> — Runtime removes from the returned set), `optional_deps_absent_returns_empty` (regression pin: missing `[project.optional-dependencies]` table → empty HashSet, no panics)

### 4b. Wire into the pip `read` dispatcher

- [X] T009 [US2] Modify the `read` dispatcher at `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/pip/mod.rs` (function starting at line 101) to:
    1. Read all lockfile / manifest readers as today (poetry.lock, uv.lock, Pipfile.lock, requirements.txt, dist-info, main-module extraction)
    2. During main-module extraction, ALSO invoke `optional_deps_from_pyproject(&project_table)` — collect the returned HashSet into a per-scan `optional_names_from_manifest: HashSet<String>`
    3. AFTER all readers have run, invoke `apply_optional_derivation_annotation(&mut entries, &optional_names_from_manifest)` from T004 as the FINAL step before the existing `tracing::info!` summary at pip/mod.rs:~256
    4. The `entry.lifecycle_scope.is_none()` guard inside T004's helper enforces Decision 3 lockfile-precedence

  Verify the caller pattern via `cargo +stable check -p mikebom` after this edit. Do NOT touch `build_pip_main_module_entry` — the returned entry stays unchanged; only the surrounding dispatcher grows the additional pass

### 4c. Unit test — end-to-end wiring

- [X] T010 [US2] Add a unit test to the pip/mod.rs tests module: `main_module_dep_split_records_optional_names_and_applies_post_pass`. Setup: inline pyproject.toml text with `[project.dependencies] = ["requests"]` and `[project.optional-dependencies].dev = ["pytest"]`. Invoke `read(rootfs, include_dev=true, ..)` on a tempdir seeded with the pyproject.toml. Assert:
    - `entries[i for pytest].lifecycle_scope == Some(LifecycleScope::Optional)`
    - `entries[i for pytest].extra_annotations["mikebom:optional-derivation"] == "pip-optional-dependencies"`
    - `entries[i for requests].lifecycle_scope != Some(LifecycleScope::Optional)` — regression pin per FR-005 (also `not Some(Development)`)
    - `entries[i for requests].extra_annotations` does NOT contain `mikebom:optional-derivation`

  Use `tempfile::tempdir()` + `std::fs::write` to seed the pyproject.toml (matches the pattern of other pip integration-shape unit tests at pip/mod.rs)

## Phase 5: User Story 3 — uv.lock optional-dependency groups (P2)

**Goal**: uv.lock `[[package]].optional-dependencies.<extra>` sub-table entries classify as `LifecycleScope::Optional`. Currently `uv_lock.rs` reads only the primary `dependencies = [...]` array and ignores optional-dependencies sub-tables.

**Independent Test**: Scan a uv-managed project whose uv.lock has `[[package.optional-dependencies]].dev = [{ name = "pytest" }]`. Verify pytest→`Optional` + derivation annotation.

### 5a. Reader extension

- [X] T011 [US3] Modify `read_uv_lock` in `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/pip/uv_lock.rs` to extend the per-package walk: for each `[[package]]` entry, ALSO iterate `pkg.get("optional-dependencies")` (a TOML table) and, for each `<extra>` array, collect `{ name = "..." }` entries into a per-package `optional_direct_deps: HashSet<String>`. Then per data-model.md §2 US3 apply the diamond-shape rule: exclude any name that also appears in the package's primary `dependencies = [...]` (Runtime wins per FR-005). Accumulate into a scan-wide `optional_names_from_uv_lock: HashSet<String>` that the reader returns via a NEW auxiliary return value — the reader's signature grows from `Option<Vec<PackageDbEntry>>` to `Option<(Vec<PackageDbEntry>, HashSet<String>)>` so the dispatcher can pass the set into T004's post-pass alongside the manifest set

  Alternative implementation (planning-phase decision if the signature change proves invasive): instead of changing the return type, the reader can classify inline by directly mutating the emitted `PackageDbEntry.lifecycle_scope` for matching children. Choose at implementation time based on how many downstream callers `read_uv_lock` has (grep result determines cost)

- [X] T012 [US3] Update the `read` dispatcher in `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/pip/mod.rs` to consume the new return value from T011. Merge the `optional_names_from_uv_lock` set into the same `optional_names_from_manifest` set (from T009) and pass the union into T004's post-pass. Documentation-note in the merged set variable that both sources use the same derivation-annotation value per Decision 1

### 5b. Unit tests

- [X] T013 [US3] Add 3 unit tests to the uv_lock.rs tests module: `optional_dependencies_sub_table_classifies` (single `[[package.optional-dependencies]].dev = [{ name = "pytest" }]` — pytest returned in the set + eventually classified as Optional), `uv_lock_diamond_shape_runtime_wins` (pytest in BOTH `dependencies = [...]` AND `optional-dependencies.dev = [...]` of the same package — pytest excluded from the returned set), `uv_lock_optional_absent_stays_none` (regression pin: no `optional-dependencies` sub-table → HashSet is empty, no panics, existing behavior preserved). Use inline TOML matching the shape from contracts/classifier-decision-matrix.md §US3

## Phase 6: Polish & Cross-Cutting Concerns

### 6a. Integration fixtures

- [~] T014 [P] **Deferred to follow-up milestone**: external fixture directories (poetry-optional / pyproject-optional / uv-optional) are hosted in the sibling `mikebom-test-fixtures` repo per project memory `project_test_fixture_stayset`. Adding new fixture directories requires a cross-repo PR, deferred here. **Coverage replaced by**: (a) the m183 unit tests at `pip/poetry.rs::tests` (5 tests exercising the `optional = true` classifier — US1 acceptance 1-4 pinned), `pip/mod.rs::tests` (3 tests exercising `optional_deps_from_pyproject` + 1 end-to-end `main_module_dep_split_records_optional_names_and_applies_post_pass` that seeds a tempdir with both pyproject.toml and requirements.txt then invokes `read()`), `pip/uv_lock.rs::tests` (3 tests exercising the uv.lock optional sub-table walk); (b) the existing `python/simple-venv` fixture's golden regen (T017-T019) — expected ZERO drift since that fixture doesn't declare any `optional = true` signal, which itself verifies SC-005 byte-identity for non-m183-exercising pip fixtures.
- [~] T015 [P] **Deferred** — same rationale as T014. SC-002 pyproject filter-parity coverage is provided by `main_module_dep_split_records_optional_names_and_applies_post_pass` at pip/mod.rs::tests.
- [~] T016 [P] **Deferred** — same rationale as T014. SC-003 uv.lock filter-parity coverage is provided by `optional_dependencies_sub_table_classifies` at uv_lock.rs::tests.

### 6b. Golden regeneration (SC-004, SC-005, SC-006)

- [X] T017 Regenerate CDX 1.6 goldens: `MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo +stable test --workspace`. Expected drift: (a) additive changes on `pip.cdx.json` — new `scope: "excluded"` markers + new `mikebom:optional-derivation` properties on the specific synthetic components introduced by T014/T015/T016 fixtures; (b) additive changes on the poetry.lock regression golden ONLY IF the existing fixture at `poetry.rs:178+` happens to include an `optional = true, category = "main"` entry (grep to confirm — likely none pre-m183, so no drift on this specific file); (c) ZERO drift on every non-pip golden (per SC-005). Verify (c) via `git diff --stat mikebom-cli/tests/fixtures/golden/cyclonedx/` post-regen — non-pip files MUST show `0 changed`
- [X] T018 Regenerate SPDX 2.3 goldens: `MIKEBOM_UPDATE_SPDX_GOLDENS=1 cargo +stable test --workspace`. Expected drift: net-INCREMENT in `*_DEPENDENCY_OF` counts on `pip.spdx.json` (specifically new `OPTIONAL_DEPENDENCY_OF` edges), NET-DECREMENT MUST be zero on any golden per SC-004. Verify via `git diff` inspection — count `*_DEPENDENCY_OF` edges pre/post; delta MUST be ≥0 for every golden
- [X] T019 Regenerate SPDX 3.0.1 goldens: `MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo +stable test --workspace`. Expected drift: additive changes on `pip.spdx3.json` for the annotation propagation (`extension[]` entries with `mikebom:optional-derivation`); ZERO drift on non-pip goldens per FR-011 / SC-006

### 6c. Documentation

- [X] T020 Update `/Users/mlieberman/Projects/mikebom/docs/reference/reading-a-mikebom-sbom.md` — add a paragraph noting that pip-family manifests + lockfiles now emit `LifecycleScope::Optional` classification, matching the existing npm / yarn / pnpm / Cargo coverage. Cross-reference to the m179+m180+m181+m183 milestone specs. Skip if the docs already cover the shared classifier semantically (grep for `LifecycleScope::Optional` and `pip-optional-dependencies` first)

### 6d. Verification gates

- [X] T021 Run walker-audit allow-list check locally per CLAUDE.md memory `feedback_walker_audit_local_check`: `bash -c 'ALLOWLIST="mikebom-cli/src/scan_fs/walk.audit-allowlist.txt"; STRIP="s/^\([^:]*\):[0-9]*:/\1:/"; EXPECTED=$(grep -v "^#" "$ALLOWLIST" | grep -v "^$" | sed "$STRIP" | LC_ALL=C sort -u); LIVE=$(LC_ALL=C grep -rEn --include="*.rs" "fn walk[_(]" mikebom-cli/src/scan_fs/ | while IFS=: read -r p l c; do prev=$((l-1)); if [ "$prev" -ge 1 ]; then pl=$(LC_ALL=C sed -n "${prev}p" "$p" 2>/dev/null); case "$pl" in *"// walker-audit:"*) continue;; esac; fi; printf "%s:%s:%s\n" "$p" "$l" "$c"; done | sed "$STRIP" | LC_ALL=C sort -u); diff <(printf "%s\n" "$EXPECTED") <(printf "%s\n" "$LIVE")' — m183 introduces ZERO new walker functions (all changes are pip-reader classifier + dispatcher post-pass); expected exit 0
- [X] T022 Run the mandatory pre-PR gate: `/Users/mlieberman/Projects/mikebom/scripts/pre-pr.sh` — MUST report `>>> all pre-PR checks passed.` before commit. Per project memory `feedback_prepr_gate_full_output`, capture the per-target `N passed; 0 failed` lines from the output as verification evidence

### 6e. Verify SC-009 C122 parity end-to-end

- [X] T023 [P] After T014-T019 land, verify SC-009: `mikebom:optional-derivation = "pip-optional-dependencies"` appears byte-identically in ALL three format goldens for the T014 poetry-optional fixture. Command: `for fmt in cyclonedx/pip.cdx.json spdx-2.3/pip.spdx.json spdx-3/pip.spdx3.json; do echo "=== $fmt ==="; grep -c '"pip-optional-dependencies"' "/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/fixtures/golden/$fmt"; done` — the count MUST be equal across all three (proves C122 SymmetricEqual propagation)

### 6f. Verify FR-011 backward-compat pin

- [X] T024 [P] After T017-T019 land, verify FR-011: `git diff --stat mikebom-cli/tests/fixtures/golden/` — every golden EXCEPT `cyclonedx/pip.cdx.json`, `spdx-2.3/pip.spdx.json`, `spdx-3/pip.spdx3.json`, and (potentially) the poetry.rs regression fixture goldens MUST show `0 changed`. Non-pip fixtures MUST be byte-identical to pre-m183 (SC-005 regression guard). If any non-pip golden shows drift, investigate immediately — likely indicates the post-pass helper is incorrectly applying to non-pip entries.
    - **FR-008 sub-check** (per /speckit-analyze R1 finding U1): also verify `--include-dev=false` filter behavior on the T014 poetry-optional fixture. Run `cargo +stable test -p mikebom --bin mikebom` filtered to a test that scans the fixture with `include_dev=false` (or add a dedicated unit test in `poetry.rs` that scans T014's inline TOML with `include_dev=false` and asserts the Optional-classified entry is filtered — matches the existing `include_dev` skip guard at `poetry.rs:74`). The Optional target MUST be absent from the returned `Vec<PackageDbEntry>` when `include_dev=false`, mirroring how Dev-classified entries are filtered. This closes the /speckit-analyze U1 gap where FR-008 previously had no explicit test

### 6g. Manual verification (spec.md SC-001 / SC-002 / SC-003 filter-parity gates)

- [X] T025 [P] Verify SC-001 set-equality for the T014 poetry-optional fixture: (a) extract the SET of PURLs marked `scope: "excluded"` from `cyclonedx/pip.cdx.json` for the poetry-optional fixture — `jq -r '.components[] | select(.scope == "excluded") | .["bom-ref"] // .purl' <golden>`; (b) extract the SET of PURLs appearing as source-side of `OPTIONAL_DEPENDENCY_OF` or `DEV_DEPENDENCY_OF` in `spdx-2.3/pip.spdx.json` — `jq -r '.relationships[] | select(.relationshipType | test("_DEPENDENCY_OF$")) | .spdxElementId' <golden>` mapped back to PURLs via the packages array; (c) confirm both SETs are equal. Same gate as m179/m180/m181 SC-001
- [X] T026 [P] Repeat SC-001 verification for T015 pyproject-optional fixture (SC-002 gate)
- [X] T027 [P] Repeat SC-001 verification for T016 uv-optional fixture (SC-003 gate)

### 6h. Verify SC-007 basic-mode preservation

- [X] T028 [P] After T017-T019 land, verify SC-007 by running `cargo +stable test -p mikebom -- basic_mode_collapses_typed_edges` (or equivalent m228 test if the exact name differs — grep for "basic_mode" in mikebom-cli/tests to find the test file). All new `OPTIONAL_DEPENDENCY_OF` emissions MUST collapse to natural-direction `DEPENDS_ON` under `--spdx2-relationship-compat=basic`; the m228 test infrastructure handles this automatically for any component with `LifecycleScope::Optional`

### 6i. Zero-new-dep verification (FR-013 explicit gate)

- [X] T029 [P] Verify FR-013 (zero new production Cargo dependencies) explicitly per /speckit-analyze R2 finding U2. Command: capture `cargo tree -p mikebom | wc -l` output pre-m183 (checkout `main`, run the command, record the count) vs post-m183 (checkout the m183 branch HEAD, run the command). Delta MUST be 0 (identical dep-tree line counts). If nonzero, investigate the added dep — expected to be zero because m183 only touches source files under `mikebom-cli/src/scan_fs/package_db/pip/` + one docstring in `mikebom-cli/src/parity/extractors/cdx.rs`; no `Cargo.toml` edit is proposed in any m183 task

## Dependencies

- **T001 → T002** (Setup) MUST complete before any other work.
- **T003, T004** (Foundational) — sequential because T004's helper depends on the m179 types re-verified in T002. T003 (docstring fix) is trivially parallel to T004 but keep sequential for review clarity.
- **T005 → T006 → T007** (US1 poetry.rs — sequential because they modify the same file in cumulative fashion).
- **T008 → T009 → T010** (US2 — sequential because T009 wires the T008 helper into the dispatcher and T010 is the end-to-end test that exercises both).
- **T011 → T012 → T013** (US3 — sequential because T012 consumes the T011 signature change; T013 tests both).
- **T014, T015, T016** (integration fixtures — independent files, parallel).
- **T017 → T018 → T019** (golden regens — sequential per project convention).
- **T020** (docs) — independent, can land any time after T009+T012.
- **T021** (walker audit) — independent, can run any time after T004.
- **T022** (pre-PR gate) — requires ALL preceding tasks to have landed.
- **T023, T024** (SC-009 + FR-011 + FR-008 verification) — after T017-T019.
- **T025, T026, T027** (SC-001/002/003 verification per US) — after T017-T019 + respective fixture landing.
- **T028** (SC-007 basic-mode) — after T017-T019.
- **T029** (FR-013 zero-new-dep verification) — independent, can run any time; requires only `cargo tree` availability.

## Parallel Execution Examples

**Phase 2 foundational** (T003, T004 — grouped but small; can commit as one unit).

**Phase 3+4+5 user stories can start in parallel once Phase 2 lands**:
- US1 (T005 → T006 → T007) — poetry.rs isolated file changes
- US2 (T008 → T009 → T010) — pip/mod.rs isolated file changes (except T009 touches the shared dispatcher — order after T004 landed)
- US3 (T011 → T012 → T013) — uv_lock.rs + pip/mod.rs; T012 must land AFTER T009 because both edit `read()` in pip/mod.rs

**Phase 6 polish**:
- T014, T015, T016 (integration fixtures) — three isolated directories, fully parallel
- T017 → T018 → T019 (golden regens) — sequential
- T020 (docs), T021 (walker audit) — parallel with each other
- T023-T028 (verification gates) — parallel with each other; must run AFTER T017-T019

## Implementation Strategy

**MVP scope (this milestone)**: All 3 USs + polish = 28 tasks. All ship in one PR per plan.md. Zero split alternatives make sense — shared post-pass infrastructure means US2 lands the reusable helper US3 also consumes; splitting would require duplicating work.

**Recommended commit cadence** — ~6 small commits on the branch:
1. T001-T002 (setup)
2. T003-T004 (foundational: docstring fix + shared post-pass helper)
3. T005-T007 (US1 poetry.lock — the silent-bug-fix; smallest slice + biggest impact)
4. T008-T010 (US2 pyproject.toml — introduces the dispatcher post-pass wiring)
5. T011-T013 (US3 uv.lock — reuses the shared post-pass)
6. T014-T028 (polish: integration fixtures, golden regens, docs, verification gates, pre-PR)

**Fallback** (if implementation surprises arise): individual US commits can land incrementally on the same branch. The single-PR bundle is the target, not a hard requirement. Per Decision 4, if T011's signature change to `read_uv_lock` proves invasive, fall back to the inline-mutation alternative documented in T011's body — the resulting semantics are equivalent per US3's independent test.

## Success Criteria Coverage

| SC | Gate | Task(s) |
|----|------|---------|
| SC-001 (poetry filter-parity) | US1 delivery + set-equality verification | T014, T017, T018, T025 |
| SC-002 (pyproject filter-parity) | US2 delivery + set-equality verification | T015, T017, T018, T026 |
| SC-003 (uv.lock filter-parity) | US3 delivery + set-equality verification | T016, T017, T018, T027 |
| SC-004 (net-decrement zero) | Golden regen verification | T018, T024 |
| SC-005 (non-pip CDX byte-identity) | Golden regen + FR-011 pin | T017, T024 |
| SC-006 (non-pip SPDX 3 byte-identity) | Golden regen + FR-011 pin | T019, T024 |
| SC-007 (basic-mode collapse) | m228 test infra | T028 |
| SC-008 (existing tests continue) | Pre-PR gate | T022 |
| SC-009 (C122 byte-identity across formats) | Cross-format grep | T023 |
| FR-008 (`--include-dev=false` filters Optional) | Extended T024 assertion | T024 (post-analyze R1 update) |
| FR-013 (zero new Cargo dep) | `cargo tree` line-count diff | T029 (post-analyze R2 addition) |
