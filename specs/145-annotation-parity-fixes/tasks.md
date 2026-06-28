---
description: "Task list for milestone 145 — annotation-emission parity fixes from sbom-conformance audit (2026-06-26)"
---

# Tasks: milestone 145 — annotation-emission parity fixes

**Input**: Design documents from `/specs/145-annotation-parity-fixes/`
**Prerequisites**: plan.md ✅, spec.md ✅, research.md ✅, data-model.md ✅, contracts/ ✅, quickstart.md ✅

**Tests**: Spec mandates tests via SC-002 + SC-005 + SC-009 + SC-010. Test tasks are included inline alongside implementation tasks (per the project's existing Rust convention of in-file `#[cfg(test)] mod tests`).

**Organization**: Tasks are grouped by user story. The three stories are largely independent — US1 touches `file_tier/mod.rs`, US2 touches `spdx/v3_annotations.rs`, US3 touches the source-files dedup at 4 sites. No foundational types or signature plumbing required (zero new types per data-model.md).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Maps to user story from spec.md (US1, US2, US3)
- Paths are absolute under `/Users/mlieberman/Projects/mikebom/`

## Path Conventions

All paths are absolute under repository root `/Users/mlieberman/Projects/mikebom/`. The single Cargo crate touched is `mikebom-cli/`.

---

## Phase 1: Setup

**Purpose**: Verify baseline before any code change.

- [X] T001 Confirm baseline pre-PR gate is green on branch `145-annotation-parity-fixes`. Run `./scripts/pre-pr.sh` from repo root. Expected: clippy `--workspace --all-targets -- -D warnings` clean; `cargo test --workspace` passes except for the pre-existing local-environment `sbomqs_parity::sbomqs_spdx_score_meets_or_beats_cdx_across_ecosystems` failure documented in milestone-144 T001. If anything ELSE fails, halt and investigate before proceeding.

---

## Phase 2: Foundational

**Purpose**: None required for this milestone. All three user stories are independent and touch disjoint code (US1 → file-tier reader, US2 → SPDX 3 emitter, US3 → source-files dedup sites). The parity catalog rows C18 / C42 / C92 already exist from milestone 071. Zero new types, zero new Cargo dependencies (per data-model.md).

(No tasks in this phase. Proceed directly to Phase 3.)

---

## Phase 3: User Story 1 - `mikebom:file-paths` is emitted as a native JSON array (Priority: P1) 🎯 MVP

**Goal**: Replace the `serde_json::to_string` round-trip at `file_tier/mod.rs:233` so `extra_annotations["mikebom:file-paths"]` holds a `Value::Array` instead of `Value::String`.

**Independent Test**: Build a synthetic file-tier component with one path, call `into_resolved_component()`, assert the `mikebom:file-paths` value is `Value::Array`. Then scan any image-fixture, emit SPDX 2.3, and grep for `"value":"["` followed by `"` — should be zero matches (the value is now a native array, not a quoted-string-of-array).

### Implementation for User Story 1

- [X] T002 [US1] Edit `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/file_tier/mod.rs` line 232-234. Replace:

  ```rust
  if let Ok(file_paths_json) = serde_json::to_string(&paths_str) {
      extra_annotations.insert(FILE_PATHS_KEY.to_string(), json!(file_paths_json));
  }
  ```

  with:

  ```rust
  extra_annotations.insert(FILE_PATHS_KEY.to_string(), json!(paths_str));
  ```

  Per research §A, the `if let Ok(...)` defensive shape is unnecessary because `serde_json::to_string(&Vec<String>)` always succeeds; dropping it is both shorter and correct. Also update the doc-comment at line 190 from "`mikebom:file-paths = <JSON-encoded-sorted-array>`" to "`mikebom:file-paths = <sorted JSON array of paths>` (native array, not JSON-string-encoded; milestone 145 US1)".

- [X] T003 [US1] Update the existing unit test at `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/file_tier/mod.rs:398-410` (the test that parses `mikebom:file-paths` via `serde_json::from_str`). Change the parse logic from `serde_json::from_str(fp).expect("file-paths JSON parses")` to direct array iteration. Concrete shape:

  ```rust
  let fp = c.extra_annotations.get("mikebom:file-paths")
      .expect("file-paths annotation present");
  let parsed: Vec<String> = fp.as_array()
      .expect("file-paths is array (milestone 145 US1)")
      .iter()
      .map(|v| v.as_str().expect("path is string").to_string())
      .collect();
  ```

  Preserve every other assertion in that test (sort order, cap behavior, truncation flag).

- [X] T004 [P] [US1] New unit test in the same `#[cfg(test)] mod tests` block at `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/file_tier/mod.rs`. Test name: `mikebom_file_paths_is_native_array_not_stringified`. Constructs a synthetic file-tier component with one path, calls `into_resolved_component()`, asserts `extra_annotations["mikebom:file-paths"].is_array()` is `true` (NOT `is_string()`). Covers SC-002 directly.

- [X] T005 [US1] Refresh affected SPDX 2.3 + SPDX 3 byte-identity goldens that contain file-tier components. Run:

  ```bash
  MIKEBOM_UPDATE_SPDX_GOLDENS=1 cargo test --test spdx_regression
  MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo test --test spdx3_regression
  ```

  Verify (via `git diff --stat mikebom-cli/tests/fixtures/golden/`) that diffs are limited to the file-paths shape change. Expected diff pattern per affected line: `"value":"[\\\"...\\\"]"` → `"value":[\"...\"]`. Reject any unrelated golden drift. CDX goldens are NOT expected to change (per contracts/file-paths-shape.md — CDX `properties[].value` is spec-typed `xs:string` so wire bytes stay identical).

**Checkpoint**: After Phase 3, US1 is fully functional. `cargo +stable test --workspace` passes; new test fires green; SPDX 2.3 + SPDX 3 goldens reflect the native-array shape. Manual smoke: scan any image-fixture and confirm no `"value":"[" → quoted-string-of-array entries in the SPDX outputs.

---

## Phase 4: User Story 2 - `mikebom:lifecycle-scope` emitted in SPDX 3 (Priority: P1)

**Goal**: Add a new emission branch in `spdx/v3_annotations.rs` mirroring the SPDX 2.3 sibling at `annotations.rs:227-236`. Emit for `Development`/`Build`/`Test` scopes; omit for `Runtime` and `None`.

**Independent Test**: Construct a synthetic `ResolvedComponent` with `lifecycle_scope = Some(LifecycleScope::Development)`, run it through the SPDX 3 annotation builder, assert the resulting JSON-LD contains `mikebom:lifecycle-scope` with value `"development"`. Construct a second with `Some(Runtime)` and assert NO `mikebom:lifecycle-scope` annotation is present.

### Implementation for User Story 2

- [X] T006 [US2] Add the new emission branch to `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/generate/spdx/v3_annotations.rs`. Insert after the existing `mikebom:raw-version` push (around line 263-265). Use the exact pattern from `contracts/lifecycle-scope-emission.md` §Insertion location:

  ```rust
  // C42 mikebom:lifecycle-scope — parity-bridging annotation mirroring
  // the SPDX 2.3 sibling at annotations.rs:227-236 (milestone 145 US2).
  // CDX and SPDX 2.3 both emit this annotation for non-Runtime scopes;
  // SPDX 3 was the pre-145 outlier.
  if let Some(ref scope) = c.lifecycle_scope {
      use mikebom_common::resolution::LifecycleScope;
      let s = match scope {
          LifecycleScope::Development => Some("development"),
          LifecycleScope::Build => Some("build"),
          LifecycleScope::Test => Some("test"),
          LifecycleScope::Runtime => None,
      };
      if let Some(s) = s {
          push(out, "mikebom:lifecycle-scope", json!(s));
      }
  }
  ```

  Verify the `push` helper and `out` parameter names match the existing pattern at this site (use the same names the surrounding code uses).

- [X] T007 [P] [US2] New unit test in `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/generate/spdx/v3_annotations.rs#[cfg(test)] mod tests` (create the test module if it doesn't exist, with `#[cfg_attr(test, allow(clippy::unwrap_used))]` per the existing project convention). Test name: `spdx3_lifecycle_scope_development_emits`. Constructs a synthetic `ResolvedComponent` with `lifecycle_scope = Some(LifecycleScope::Development)`, runs the SPDX 3 annotation builder, asserts the emitted annotations include `mikebom:lifecycle-scope` with value `"development"`. Covers SC-005 (first half).

- [X] T008 [P] [US2] New unit test in the same `mod tests`. Test name: `spdx3_lifecycle_scope_runtime_omitted`. Constructs `lifecycle_scope = Some(LifecycleScope::Runtime)`, runs the builder, asserts NO `mikebom:lifecycle-scope` annotation is present in the output. Covers SC-005 (second half) + FR-006.

- [X] T009 [P] [US2] New unit test in the same `mod tests`. Test name: `spdx3_lifecycle_scope_none_omitted`. Constructs `lifecycle_scope = None`, runs the builder, asserts NO `mikebom:lifecycle-scope` annotation is present. Covers FR-007.

- [X] T010 [US2] Refresh affected SPDX 3 byte-identity goldens. Run:

  ```bash
  MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo test --test spdx3_regression
  ```

  Verify (via `git diff --stat mikebom-cli/tests/fixtures/golden/spdx-3/`) that diffs are limited to NEW `mikebom:lifecycle-scope` annotations on non-Runtime-scoped components in fixtures that have them (likely `npm.spdx3.json` and similar). CDX and SPDX 2.3 goldens are NOT expected to change.

**Checkpoint**: After Phase 4, US2 is fully functional. The three new unit tests fire green; SPDX 3 goldens for fixtures with dev/build/test-scoped components reflect the new annotation. Manual smoke: scan `node-dev-vs-prod` (or equivalent npm dev-dep fixture), emit SPDX 3, count `mikebom:lifecycle-scope=development` annotations; should match the CDX count.

---

## Phase 5: User Story 3 - `mikebom:source-files` cross-format value parity (Priority: P2)

**Goal**: Eliminate the double-emission of `mikebom:source-files` (field-derived + Maven-stamped via `extra_annotations`) so all three emitters produce byte-equivalent values per component on the same scan.

**Independent Test**: Scan the `polyglot-builder-image` fixture (or equivalent), emit both CDX and SPDX 3, extract `mikebom:source-files` per Maven dep, diff. Post-fix diff MUST be empty.

### Implementation for User Story 3

- [X] T011 [US3] Reproduce the per-emitter drift on the `polyglot-builder-image` fixture. Locate the fixture path (likely under `mikebom-cli/tests/fixtures/` or fetched via the milestone-090 cache). Scan it with `cargo run -p mikebom -- sbom scan --path <fixture> --format cyclonedx-json --format spdx-3-json --output cyclonedx-json=/tmp/m.cdx.json --output spdx-3-json=/tmp/m.spdx3.json`. Inspect the emitted SBOMs to confirm the double-emission: each affected Maven component carries `mikebom:source-files` TWICE in at least one format (one entry from the field, one from `extra_annotations`). Document the reproduction in a new section of `/Users/mlieberman/Projects/mikebom/specs/145-annotation-parity-fixes/research.md` named "§C.1 — fixture reproduction (milestone 145 implement phase)". Record: which fixture, which 1-3 example component PURLs exhibit the drift, the exact two values that surface, and which entry the parity-extractor picks per format.

- [X] T012 [US3] Based on T011's reproduction, **choose between Option 1 (emitter-side dedup guard) and Option 2 (Maven reader stops stamping)** per contracts/source-files-dedup.md. Default to applying BOTH (defense-in-depth) unless T011 surfaces a reason one alone is sufficient. Record the choice in research §C.1.

- [X] T013 [US3] **Option 1 work** (emitter-side guard, if chosen). Add a helper `fn is_field_owned_annotation_key(key: &str) -> bool` returning `true` for `"mikebom:source-files"` at a shared location accessible from all three emitters — recommend `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/generate/root_selector.rs` (next to the existing `is_internal_emission_key` helper). Then update the three `extra_annotations` iteration sites:
  - `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/generate/cyclonedx/builder.rs:1086-1098`
  - `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/generate/spdx/annotations.rs:371`
  - `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/generate/spdx/v3_annotations.rs:332-337`

  At each, extend the existing `is_internal_emission_key` skip condition to ALSO skip `is_field_owned_annotation_key(key)`. Pattern:

  ```rust
  for (key, value) in &c.extra_annotations {
      if root_selector::is_internal_emission_key(key)
          || root_selector::is_field_owned_annotation_key(key) {
          continue;
      }
      // existing emission ...
  }
  ```

- [X] T014 [US3] **Option 2 work** (Maven reader stop-stamping, if chosen). In `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/maven.rs:2244`, EITHER (a) drop the `mikebom:source-files` key stamping entirely (the field-derived emission already covers it via the orchestrator's `entry.source_path` normalization), OR (b) rename the key to `mikebom:source-files-nested-url` to preserve the JAR-URL `!`-syntax value under a non-colliding annotation key. Recommend (b) for information preservation. Update the doc-comment on the nested-JAR reader function (lines 1310-1320) to reflect the new key name.

- [X] T015 [US3] Add doc-comments at the field-setter sites preventing recurrence. At `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/mod.rs:198` and `:636` (both invocations of `normalize_sbom_path_relative` populating `source_file_paths`), add the comment from contracts/source-files-dedup.md §Doc-comment additions:

  ```rust
  // NOTE (milestone 145): `mikebom:source-files` has TWO emission sources —
  // this field (canonical) AND `extra_annotations["mikebom:source-files"]`
  // (legacy, dedup'd at emit time). DO NOT stamp the latter from a new
  // reader; if you need to carry per-reader source provenance, use a
  // distinct key like `mikebom:<reader>-source-url`.
  ```

  Also add the same comment at `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/maven.rs:2244` (whether T014 picks option (a) or (b), the comment serves future readers).

- [X] T016 [P] [US3] New integration test in `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/source_files_parity_md145.rs`. Test name: `mikebom_source_files_byte_equivalent_across_emitters_for_maven_nested_jar`. Constructs a synthetic Maven component with BOTH `evidence.source_file_paths = ["root/.m2/.../foo.jar"]` AND `extra_annotations["mikebom:source-files"] = "root/.m2/.../foo.jar!META-INF/MANIFEST.MF"` (pre-145 double-source shape). Runs the component through each of the three emitters (CDX builder, SPDX 2.3 annotation builder, SPDX 3 annotation builder — use whichever public-test-friendly helpers exist; if none, expose `pub(crate)` shims for testing). Extracts the `mikebom:source-files` value(s) from each emitter's output. Asserts:
  1. ONLY ONE `mikebom:source-files` entry per format (the dedup invariant from contracts/source-files-dedup.md).
  2. The surviving entry's VALUE is byte-equivalent across all three formats.

- [X] T017 [P] [US3] If T014 chose option (b) — renamed key to `mikebom:source-files-nested-url` — add a unit test in `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/scan_fs/package_db/maven.rs#[cfg(test)] mod tests` (or co-located) asserting that nested-JAR detection still emits the JAR-URL `!`-notation value, just under the new key. Test name: `maven_nested_jar_url_emitted_under_renamed_key_md145`.

- [X] T018 [US3] Refresh affected goldens. The scope depends on whether the affected Maven fixtures appear in the existing golden-test set. Run all three update vars and inspect:

  ```bash
  MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo test --test cdx_regression
  MIKEBOM_UPDATE_SPDX_GOLDENS=1 cargo test --test spdx_regression
  MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo test --test spdx3_regression
  ```

  Verify (via `git diff --stat`) that diffs are limited to (a) suppression of the duplicate `mikebom:source-files` entry per Maven component, plus (b) IF T014 chose option (b), the new `mikebom:source-files-nested-url` entries on affected Maven components. Reject any unrelated drift.

**Checkpoint**: After Phase 5, US3 is fully functional. The scan-and-diff verification from the story's Independent Test produces an empty diff between CDX and SPDX 3 `mikebom:source-files` values on Maven deps. The new integration test fires green. Manual smoke per quickstart §Scenario 3.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Validation, doc updates, harness re-run preparation, pre-PR gate, commit.

- [X] T019 [P] Refresh the parity-catalog test expectations if they hardcoded the pre-145 finding patterns. Likely sites: tests under `mikebom-cli/tests/` that exercise the C18/C42/C92 parity rows. Grep first: `grep -rn 'C18\|C42\|C92\|mikebom:file-paths\|mikebom:lifecycle-scope\|mikebom:source-files' mikebom-cli/tests/`. If any existing tests assert on the pre-145 shape, update them.

- [X] T020 Run quickstart.md verification scenarios locally:
  1. Scenario 1: `mikebom:file-paths` shape verification — pick any image-like fixture, scan with `--format spdx-2.3-json`, confirm the jq query returns `"array"` not `"string"`.
  2. Scenario 2: `mikebom:lifecycle-scope` SPDX 3 emission — if `node-dev-vs-prod` or similar npm dev-dep fixture is accessible, scan it with `--format cyclonedx-json --format spdx-3-json`, confirm dev-component counts match between the two formats.
  3. Scenario 3: `mikebom:source-files` parity on Maven deps — if `polyglot-builder-image` is accessible, scan with both formats, confirm empty diff per the documented jq pipeline.

  Document which scenarios passed locally and which require operator-supplied fixtures.

- [X] T021 Run mandatory pre-PR gate per Constitution Development Workflow + memory `feedback_prepr_gate_full_output.md`: `./scripts/pre-pr.sh` from repo root. Both clippy + test steps MUST pass clean (except the pre-existing `sbomqs_parity` env-only failure documented in milestone-144 T001 — CI will validate). If any OTHER test fails, scan the FULL output (do NOT grep on `^test result: FAILED` — that filter is known to drop multi-test-suite summaries). Covers SC-010.

- [ ] T022 Commit the milestone-145 changes. Per the project's spec-→plan-→tasks-→impl convention from milestone-134/144, suggested commit chain:
  - `spec(145): annotation-emission parity fixes from sbom-conformance audit (2026-06-26)` — spec.md + checklists/requirements.md
  - `plan(145): annotation parity fixes — plan + research + data-model + contracts + quickstart` — plan + research + data-model + contracts/ + quickstart + CLAUDE.md
  - `tasks(145): N tasks across M phases for annotation parity fixes` — tasks.md
  - `impl(145): mikebom:file-paths native-array shape + mikebom:lifecycle-scope SPDX 3 emission + mikebom:source-files dedup` — code files + golden refreshes

  Do NOT commit until T021 passes clean. Use `git add <specific paths>` (never `-A`); each commit ends with the standard `Co-Authored-By` trailer.

- [X] T023 [P] Per spec SC-001 / SC-004 / SC-008 (harness-finding-count verification, operator cadence): document in the PR description that the operator should re-run the sbom-conformance harness against pre-/post-145 builds to confirm the cumulative ≥3,424 CFI finding reduction (3,112 file-paths + 261 lifecycle-scope + 51 source-files). The harness is NOT a CI gate (per research §D); the in-tree tests added by T004 / T007 / T008 / T009 / T016 are the CI-binding signal.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No dependencies. Verifies baseline.
- **Phase 2 (Foundational)**: EMPTY (no foundational work required for this milestone).
- **Phase 3 (US1)**: Depends on Phase 1. Independent of US2/US3.
- **Phase 4 (US2)**: Depends on Phase 1. Independent of US1/US3.
- **Phase 5 (US3)**: Depends on Phase 1. Includes a reproduce-first step (T011) that gates the choice of fix path (T012 → T013/T014).
- **Phase 6 (Polish)**: Depends on US1+US2+US3 being functionally complete (or whichever subset is being shipped as MVP).

### User Story Dependencies

- **US1 (P1, MVP)**: Standalone after Phase 1. Delivers the largest single audit-finding reduction (3,112).
- **US2 (P1)**: Standalone after Phase 1. Delivers 261 findings.
- **US3 (P2)**: Standalone after Phase 1. Requires reproduce-first step (T011) before fix can land (T012-T018). 51 findings.

### Within Each User Story

- T002 (US1 code) MUST land before T003 (US1 test update) — but T004 (new US1 test) can land in parallel with T002 since it's a different test function.
- T006 (US2 emission code) MUST land before T007/T008/T009 (US2 tests) — but the tests don't conflict with each other and can land in parallel after T006.
- T011 (US3 reproduce) MUST land before T012 (decision), which gates T013/T014/T015. T016/T017 are the tests and can land in parallel with the code once T011 + T012 settle.

### Parallel Opportunities

- Within US1: T004 [P] runs independently of T002.
- Within US2: T007/T008/T009 [P] all run in parallel after T006.
- Within US3: T016/T017 [P] run in parallel after T013/T014 land.
- Across phases (single-developer feasibility): one developer typically works through Phase 3 → 4 → 5 sequentially. A multi-developer team could split: Dev A → US1 (Phase 3), Dev B → US2 (Phase 4), Dev C → US3 (Phase 5). The polish phase (T019-T023) is sequential at the end.

---

## Parallel Example: US2

```bash
# After T006 (code change) lands sequentially:
Task T007: spdx3_lifecycle_scope_development_emits — new unit test
Task T008: spdx3_lifecycle_scope_runtime_omitted — new unit test
Task T009: spdx3_lifecycle_scope_none_omitted — new unit test
```

(All three tests are in the same `mod tests` block; they can be added in one editor pass and don't conflict with each other.)

---

## Implementation Strategy

### MVP First (US1 only — biggest ROI by an order of magnitude)

1. Complete Phase 1: T001 baseline check.
2. Complete Phase 3: T002 → T003 → T004 → T005 — the file-paths shape fix lands.
3. **STOP and VALIDATE**: `./scripts/pre-pr.sh` clean. Quickstart §Scenario 1 confirms native-array shape.
4. This alone is a shippable PR if US2/US3 need to be sequenced separately. Clears 3,112 of the 3,424 audit findings (91% of the cleanup ROI).

### Incremental (recommended for this milestone)

1. Phase 1 (T001) baseline.
2. Phase 3 (US1) — file-paths shape fix lands.
3. Phase 4 (US2) — SPDX 3 lifecycle-scope emission lands.
4. Phase 5 (US3) — source-files dedup lands (after reproduce-first step).
5. Phase 6 (T019-T023) — polish + commit.

All in a single PR is the intended shape per the spec's framing of all three as one milestone (audit-derived cleanup).

### Single-developer Note

This milestone is small enough that one developer can work through all three stories in one session. The [P] markers exist primarily to signal "no cross-file write conflict" for the tests within each story.

---

## Notes

- Tests live in-file under `#[cfg(test)] mod tests` per the project's existing convention. The one out-of-source test is the US3 integration test (T016) at `mikebom-cli/tests/source_files_parity_md145.rs`.
- The `#[cfg_attr(test, allow(clippy::unwrap_used))]` convention applies to any new `mod tests` block per Constitution Principle IV.
- Memory `feedback_prepr_gate_full_output.md` is directly relevant: when verifying T021, scan the FULL output rather than greping on `^test result: FAILED`.
- Memory `feedback_dont_dismiss_test_failures.md` is relevant if any new test failures surface: verify reproducibility before calling anything "pre-existing flake".
- The commit-message convention (T022) follows the milestone-134/144 precedent: `spec(145):` / `plan(145):` / `tasks(145):` / `impl(145):`.
- Harness re-run (T023) is operator-cadence, NOT CI-gating. The in-tree tests (T004 / T007 / T008 / T009 / T016) are the CI-binding signal.
