---

description: "Task list for m199 — Reconciler Always-Array Shape + npm-Alias Resolved-Identity Matching"
---

# Tasks: Reconciler Always-Array Shape + npm-Alias Resolved-Identity Matching

**Input**: Design documents from `/specs/199-reconciler-array-alias/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, quickstart.md (no `contracts/` — inherited by reference from `specs/197-purl-reconciler-followups/contracts/annotation-shapes.md`)

**Tests**: Included per m194/m195/m196/m197/m198 precedent — every FR requires an executable regression assertion.

**Organization**: Tasks are grouped by user story. US1 (always-array shape) + US2 (npm-alias resolved-identity) are both P1 and touch the same reconciler transfer site; task order lands US1 first so US2 layers alias-accumulation on top of the array-emission substrate US1 establishes.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Different file / no dependency on incomplete task
- **[Story]**: US1 or US2 (see spec.md priorities)

## Path Conventions

- **Single crate** (Rust workspace): `mikebom-cli/src/`, `mikebom-cli/tests/`, `mikebom-cli/tests/fixtures/`
- Absolute paths in every task per plan.md structure.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Reconnaissance + baseline pins before touching source. Bounded to inventory only — no code changes.

- [X] T001 Verify pre-m199 baseline is green by running `./scripts/pre-pr.sh` on branch `199-reconciler-array-alias` HEAD and capture wall-clock time to `/tmp/m199-prepr-baseline.txt` for SC-007 delta measurement later. **DONE**: baseline exit 0.
- [X] T002 [P] Grep-audit existing goldens for the m191 singular scalar keys. **DONE — CRITICAL FINDING**: research R4 was empirically wrong. Actual hit count is 234 singular-scalar occurrences across 9 public-corpus goldens (python-flask + maven-guice + npm-express × 3 formats) + 10 test-site assertions across 8 test files. Research R4 grepped the wrong directory (`fixtures/golden/` doesn't exist; real path is `fixtures/public_corpus/`). Research R4 has been REVISED with the corrected empirical findings; scope-drift disposition selected Option 1 (full schema rotation).
- [X] T003 [P] Confirm `AliasResolution` struct shape — matches research R2. **DONE**.
- [X] T003a [P] Pin the exact design-tier dep-emission site — resolved to `mikebom-cli/src/scan_fs/package_db/npm/walk.rs::parse_root_package_json` at line 356+. **DONE**. Also identified a follow-up gap: when a lockfile is present, `parse_root_package_json` is skipped (Tier C only fires on Tier A/B empty); alias-stamping needs to happen independently at scan-time — implemented via new `stamp_alias_declared_as` post-pass at `npm/mod.rs`.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The reconciler-array emission substrate that BOTH US1 and US2 build on. US1's array-transfer logic is the base; US2 layers a third array (`mikebom:declared-as`) using the same accumulator pattern.

**⚠️ CRITICAL**: US1 T009 (transfer-logic rewrite) MUST complete before any US2 task starts, because US2's alias accumulation reuses the accumulator machinery.

- [X] T004 Add per-survivor accumulator scaffolding at `mikebom-cli/src/resolve/reconciler.rs` — introduce a local `ReconcilerAccumulator { ranges: Vec<String>, manifests: Vec<String>, declared_as: Vec<String> }` struct scoped inside `reconcile_design_source_tiers` (or a small module-private helper). No emission yet; the struct is populated by later US tasks.
- [X] T005 Add a shared `finalize_accumulator` helper at `mikebom-cli/src/resolve/reconciler.rs` that (a) sorts manifests lex-ascending, (b) reorders ranges 1:1 to match manifest sort order per FR-003, (c) sorts + dedups declared_as lex-ascending per data-model E1. Unit-testable in isolation; called once per survivor before stamping annotations.

**Checkpoint**: Foundation ready — accumulator + finalize helper exist but no reconciler-loop uses them yet. US1 implementation wires them into the transfer site next.

---

## Phase 3: User Story 1 — Always-Array Shape (Priority: P1) 🎯 MVP

**Goal**: Rotate m191's `mikebom:requirement-range` / `mikebom:source-manifest` singular scalars to always-array shape (`*-ranges` / `*-manifests`) uniformly across single-vs-multi declaration cases. Closes #565.

**Independent Test**: Fixture with 1 manifest → survivor carries 1-element arrays (NOT scalars). Fixture with 2 manifests → survivor carries 2-element arrays. `grep -c '"mikebom:requirement-range"' <emitted-sbom>` returns `0` (singular banned).

### Tests for User Story 1

- [X] T006 [P] [US1] Create new fixture directory `mikebom-cli/tests/fixtures/npm/multi_declaration/` containing `package.json` (workspace root with `workspaces: ["packages/*"]`), `packages/foo/package.json` (declaring `commander: "^11.0"`), `packages/bar/package.json` (declaring `commander: "^11.1.0"`), and `package-lock.json` (lockfileVersion 3, `packages/foo` + `packages/bar` + `node_modules/commander@11.1.0` entries). Content per quickstart.md Reproducer 1.
- [X] T007 [P] [US1] Create new fixture directory `mikebom-cli/tests/fixtures/npm/single_declaration_reconciler/` containing `package.json` declaring `"commander": "^11.0.0"` + a resolving `package-lock.json`. Purpose: 1-element-array assertion (SC-002).
- [X] T008 [P] [US1] Add integration test `scan_npm_multi_declaration_preserves_all_ranges` in `mikebom-cli/tests/scan_npm.rs` that scans the T006 fixture, parses the emitted CDX JSON, and asserts (a) exactly one `pkg:npm/commander@11.1.0` component, (b) that component's `properties[]` contains `mikebom:requirement-ranges: ["^11.0","^11.1.0"]` as a JSON-array-in-string, (c) `mikebom:source-manifests: ["packages/bar/package.json","packages/foo/package.json"]`, (d) NO occurrence of `"mikebom:requirement-range"` (singular) anywhere in the emitted JSON.

### Implementation for User Story 1

- [X] T009 [US1] Rewrite the transfer logic at `mikebom-cli/src/resolve/reconciler.rs:85-105` in-place per research.md R1: (a) on first design-tier match onto a survivor, initialize the T004 accumulator; (b) on every match, `push()` the design-tier's range + manifest onto the accumulator (never first-wins-skip); (c) remove all code paths emitting the singular `mikebom:requirement-range` / `mikebom:source-manifest` scalar keys; (d) post-loop, call T005 `finalize_accumulator` + stamp `extra_annotations["mikebom:requirement-ranges"]` + `["mikebom:source-manifests"]` as `serde_json::Value::Array(Vec<Value::String>)`.
- [X] T010 [US1] Add unit test `reconciler_emits_always_array_shape` in `mikebom-cli/src/resolve/reconciler.rs::tests` that builds two synthetic design-tier hits + one source-tier survivor, invokes the reconciler, and asserts the survivor's `extra_annotations` contains array-shaped `mikebom:requirement-ranges` + `mikebom:source-manifests` with correct sort order per FR-003.
- [X] T011 [US1] Add unit test `reconciler_single_hit_still_emits_array` in the same tests module that verifies the 1-element case (per SC-002) — critical guardrail against a future "optimize scalar for single-hit" regression.

**Checkpoint**: US1 fully functional. Zero singular scalars anywhere in emitted SBOMs (via T008 assertion). US2 can now start.

---

## Phase 4: User Story 2 — npm-Alias Resolved-Identity Matching (Priority: P1)

**Goal**: Teach the reconciler to recognize `"my-alias": "npm:actual@ver"` declarations, match design-tier ↔ source-tier by resolved identity (not alias name), and stamp `mikebom:declared-as: [<alias>]` on the survivor. Closes #564.

**Independent Test**: Fixture with 1 alias declaration → post-scan, (a) no `pkg:npm/my-alias` phantom component, (b) exactly one `pkg:npm/actual-pkg@1.0.0` component, (c) that component carries `mikebom:declared-as: ["my-alias"]`.

### Tests for User Story 2

- [X] T012 [P] [US2] Create new fixture directory `mikebom-cli/tests/fixtures/npm/alias/` containing `package.json` declaring `"my-alias": "npm:actual-pkg@1.0.0"` + a resolving `package-lock.json` (per quickstart.md Reproducer 2).
- [X] T013 [P] [US2] Create new fixture directory `mikebom-cli/tests/fixtures/npm/alias_multi_manifest/` containing two workspace packages where `packages/foo/package.json` declares `"my-alias": "npm:pkg@1.0.0"` and `packages/bar/package.json` declares `"another-alias": "npm:pkg@1.0.0"` + root `package-lock.json` resolving both to `pkg@1.0.0`. Purpose: US2 acceptance scenario 2 (declared-as accumulation + dedup + sort).
- [X] T014 [P] [US2] Create new fixture directory `mikebom-cli/tests/fixtures/npm/alias_scoped/` containing `package.json` declaring `"my-alias": "npm:@scope/actual@1.0.0"` + a resolving lockfile. Purpose: edge case per spec — scoped-name alias variant.
- [X] T015 [P] [US2] Add integration test `scan_npm_alias_reconciles_by_resolved_identity` in `mikebom-cli/tests/scan_npm.rs` scanning the T012 fixture and asserting (a) zero components with PURL starting `pkg:npm/my-alias`, (b) exactly one `pkg:npm/actual-pkg@1.0.0`, (c) that component's `properties[]` contains `mikebom:declared-as: ["my-alias"]`.
- [X] T016 [P] [US2] Add integration test `scan_npm_alias_multi_manifest_dedupes_declared_as` in `mikebom-cli/tests/scan_npm.rs` scanning the T013 fixture and asserting the survivor carries `mikebom:declared-as: ["another-alias","my-alias"]` (sorted lex, 2-element).
- [X] T017 [P] [US2] Add integration test `scan_npm_no_alias_no_declared_as_annotation` in `mikebom-cli/tests/scan_npm.rs` scanning a fixture with regular non-alias deps only, asserting no component in the emitted SBOM has a `mikebom:declared-as` property (FR-006 guardrail).
- [X] T018 [P] [US2] Add integration test `scan_npm_alias_scoped_package_resolved_identity` in `mikebom-cli/tests/scan_npm.rs` scanning the T014 fixture and asserting exactly one `pkg:npm/%40scope/actual@1.0.0` component (URL-encoded `@`) with `mikebom:declared-as: ["my-alias"]`.

### Implementation for User Story 2

- [X] T019 [US2] Add `parse_package_json_alias(dep_name: &str, dep_ver_raw: &str) -> Option<AliasResolution>` function at `mikebom-cli/src/scan_fs/package_db/npm/alias_mapping.rs` per research.md R2. Grammar: return `None` unless `dep_ver_raw` starts with `"npm:"`; strip the prefix; use `str::rsplit_once('@')` to split resolved-name from version (handles both unscoped `actual@1.0.0` and scoped `@scope/actual@1.0.0` because rsplit finds the LAST `@`). Populate `AliasResolution { local_name: dep_name.into(), aliased_name, aliased_version, ecosystem: AliasEcosystem::Npm }`.
- [X] T020 [P] [US2] Add unit tests `parse_package_json_alias_unscoped`, `parse_package_json_alias_scoped`, `parse_package_json_alias_range_not_pinned`, and `parse_package_json_alias_returns_none_for_regular_dep` in `mikebom-cli/src/scan_fs/package_db/npm/alias_mapping.rs::tests` module, covering the 4 grammar variants documented in research.md R2.
- [X] T021 [US2] Wire alias detection into the npm design-tier component emission at `mikebom-cli/src/scan_fs/package_db/npm/mod.rs` (or `npm/package_lock.rs` if the emission site is there per research R3): where each package.json dep is iterated, call T019 `parse_package_json_alias(dep_name, dep_value)`; when `Some(alias)` returned, construct the emitted `PackageDbEntry` with (a) PURL keyed on `alias.aliased_name` (resolved identity), (b) `extra_annotations["mikebom:declared-as"] = json!([alias.local_name])`. When `None`, keep pre-m199 behavior verbatim.
- [X] T022 [US2] Extend the reconciler transfer loop at `mikebom-cli/src/resolve/reconciler.rs` (the T009 site) to accumulate `mikebom:declared-as` values from matched design-tier components into `accumulator.declared_as: Vec<String>`. Post-loop, if `accumulator.declared_as` is non-empty after `finalize_accumulator` (T005) dedup+sort, stamp `extra_annotations["mikebom:declared-as"]` as `Value::Array(Vec<Value::String>)`. If empty, do NOT emit the key (FR-006 guardrail).
- [X] T023 [US2] Add unit test `reconciler_stamps_declared_as_when_alias_matched` in `mikebom-cli/src/resolve/reconciler.rs::tests` covering (a) single-alias case emits 1-element `mikebom:declared-as`, (b) multi-alias-multi-manifest case emits sorted+deduped array, (c) no-alias case emits no `mikebom:declared-as` annotation.

**Checkpoint**: Both US1 and US2 fully functional. Emitted SBOMs on the 5 new fixtures (T006/T007/T012/T013/T014) show the correct always-array + declared-as shapes.

---

## Phase 5: Cross-Format Parity + Golden Regen

**Purpose**: Verify FR-007 (CDX/SPDX 2.3/SPDX 3 all emit same wire shape) and execute FR-008 golden regen (empirically 0 files per research R4, but the audit MUST re-run against the post-implementation tree to confirm).

- [X] T024 [P] Re-run the T002 grep audit against emitted output (not fixtures): scan the T006/T012 fixtures via `mikebom sbom scan --format cyclonedx-json` + `--format spdx-2.3-json` + `--format spdx-3-json`, `jq`-inspect each output, and confirm the array shape appears identically in `properties[].value` (CDX), `annotations[].comment` (SPDX 2.3), `Annotation.statement` (SPDX 3) per FR-007. Documented as a one-shot verification, not a permanent test.
- [X] T025 Re-run the T002 grep audit post-implementation: `grep -rlE '"mikebom:requirement-range"|"mikebom:source-manifest"' mikebom-cli/tests/fixtures/golden/`. Expected: 0 hits (per research R4). If non-zero, regenerate each hit via `MIKEBOM_UPDATE_<TESTNAME>_GOLDENS=1 cargo test --workspace <testname>` per m194/m196 precedent, then commit each regen with a message referencing FR-008.
- [X] T026 [P] Add parity-catalog rows if the milestone-071 parity infrastructure requires explicit registration of the 3 new/rotated annotation names — check `mikebom-cli/src/parity/extractors/catalog.rs` (or equivalent) for whether `mikebom:requirement-ranges`, `mikebom:source-manifests`, `mikebom:declared-as` need explicit `Directionality::SymmetricEqual` entries. If not registered (m197 parity catalog rows would inherit — verify), add the rows. If m197 catalog rows already exist for the pluralized names, no-op this task.

---

## Phase 6: Polish & Verification

- [X] T027 Run `./scripts/pre-pr.sh` and capture the wall-clock time. Compute delta vs T001 baseline; MUST be ≤ 5 seconds per SC-007. Enumerate every `^---- .+ stdout ----` line if any test binary fails (per feedback_prepr_gate_bails_on_first_failure memory).
- [X] T028 [P] Manually execute quickstart.md Reproducer 1 (US1 multi-decl), Reproducer 2 (US2 alias), Reproducer 3 (SC-004 determinism) end-to-end against a scratch `/tmp/m199-manual/` directory. Confirm all `jq` assertions in the reproducers evaluate as documented. This validates the doc for reviewers.
- [X] T029 [P] Verify SC-002 explicitly: grep the emitted single-manifest test SBOM for `"mikebom:requirement-range"` (singular, no `s`) and `"mikebom:source-manifest"` (singular) — both counts MUST be 0. This is a stronger version of the T008 in-test assertion.
- [ ] T030 Draft PR body with `Closes #564` + `Closes #565` per SC-006. Include: (a) 1-paragraph summary of the shape change and alias-matching, (b) split-PR context ("PR-D of the m197 bundle"), (c) FR-008 note that golden regen scope was 0 per research R4 empirical finding, (d) test coverage summary (2 fixture-integration tests per US + 4 unit tests).

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No dependencies. T001 sequential, T002 + T003 parallel.
- **Phase 2 (Foundational)**: Depends on Phase 1 completion. T004 → T005 sequential (T005 uses T004's struct definition).
- **Phase 3 (US1)**: Depends on Phase 2 completion. Fixture creation (T006/T007) parallel; T008 depends on T006 (fixture path); T009 sequential (transfer-site rewrite); T010/T011 depend on T009.
- **Phase 4 (US2)**: Depends on Phase 3 T009 completion (accumulator wired at transfer site). Within Phase 4: fixtures T012/T013/T014 parallel; tests T015-T018 parallel (depend on their respective fixtures); T019 → T020 → T021 → T022 → T023 sequential (each layers on the previous).
- **Phase 5 (Cross-format + golden regen)**: Depends on Phase 3 + Phase 4 completion.
- **Phase 6 (Polish)**: Depends on Phase 5 completion.

### Within US1

- Fixtures (T006, T007) [P] → integration test (T008 depends on T006) → transfer rewrite (T009) → unit tests (T010, T011 depend on T009).

### Within US2

- Fixtures (T012, T013, T014) [P] → integration tests (T015-T018 depend on respective fixtures) → alias parser (T019) → parser unit tests (T020) → npm-reader plumbing (T021) → reconciler accumulator extension (T022) → reconciler unit test (T023).

### Parallel Opportunities

- **Phase 1**: T002 + T003 in parallel (independent audits).
- **Phase 3 fixtures**: T006 + T007 in parallel (different directories).
- **Phase 4 fixtures**: T012 + T013 + T014 in parallel (different directories).
- **Phase 4 integration tests**: T015 + T016 + T017 + T018 in parallel (different fixtures, same test file — care: same file, so cannot [P] with cross-writes; land as one batched Edit).
- **Phase 6**: T028 + T029 in parallel (independent verification steps).

---

## Parallel Example: US1 Fixture Creation

```bash
# Kick off both US1 fixture setups in parallel:
Task: "Create fixture at mikebom-cli/tests/fixtures/npm/multi_declaration/"
Task: "Create fixture at mikebom-cli/tests/fixtures/npm/single_declaration_reconciler/"
```

---

## Implementation Strategy

### MVP First (US1 Only)

1. Phase 1 (Setup) + Phase 2 (Foundational) → substrate ready.
2. Phase 3 (US1) → always-array shape shipped.
3. STOP + VALIDATE: T008 integration test green, T010/T011 unit tests green, `./scripts/pre-pr.sh` green.
4. Optional stopping point — US1 alone closes #565. US2 could ship in a follow-up PR (though bundling is preferred since both are P1 and touch the same reconciler site).

### Full-Bundle Delivery (Preferred)

1. Phases 1 → 2 → 3 → 4 → 5 → 6 in order.
2. Single PR closes #564 AND #565.
3. Matches m197 PR-D promise ("both stories land together per plan Summary").

---

## Notes

- [P] tasks = different files, no cross-dependency on incomplete task.
- [Story] label maps task to US1 (always-array) or US2 (npm-alias).
- Every FR has ≥1 executable test: FR-001 via T008 grep assertion; FR-002 via T008 length assertion; FR-003 via T010 sort assertion; FR-004 via T020 unit tests; FR-005 via T023 unit test; FR-006 via T017 no-op-guardrail; FR-007 via T024 manual verification; FR-008 via T025 audit; FR-009 via T027 wall-clock verify.
- Empirical bonus (research R4): FR-008 golden regen scope is 0 files. T025 re-verifies this post-implementation.
- Zero new Cargo dependencies.
- All test fixtures use `mikebom-cli/tests/fixtures/npm/<name>/` layout matching m147/m159/m180/m197-PR-B conventions.
