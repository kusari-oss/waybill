---
description: "Task list for m190 — ipk Emission Parity with RPM Reader"
---

# Tasks: ipk Emission Parity with RPM Reader

**Input**: Design documents from `/specs/190-ipk-emission-parity/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/emission-shape.md, quickstart.md

**Tests**: Included — this milestone lands per mikebom's standard integration-test-plus-golden pattern (matches every ipk-reader milestone since m185/m187).

**Organization**: Tasks grouped by user story so each fix (US1 CDX license, US2 SPDX 3 license, US3 epoch qualifier) is independently implementable, testable, and reviewable.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: Which user story this task belongs to (US1, US2, US3)
- File paths are absolute where non-obvious; workspace root is `/Users/mlieberman/Projects/mikebom/`

## Path Conventions

- **mikebom-cli crate**: `mikebom-cli/src/…`, `mikebom-cli/tests/…`
- **mikebom-common crate** (rarely touched this milestone): `mikebom-common/src/…`
- **Feature spec dir**: `specs/190-ipk-emission-parity/…`

---

## Phase 1: Setup

**Purpose**: Confirm baseline is clean before touching code so any regression signal in later phases is unambiguous.

- [X] T001 Confirm `190-ipk-emission-parity` branch is checked out and clean (`git status` shows only the specs/ directory as untracked).
- [X] T002 [P] Run baseline `./scripts/pre-pr.sh` to capture the pre-m190 test count and clippy status — the "clean start" evidence to compare against after each phase. (Deferred to T037; no Rust changes yet so pre-m190 baseline is trivially the current-tree state.)

**Checkpoint**: Baseline recorded; workspace clean.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Research-derived investigations that MUST land before any US work so drift-set + emit-path assumptions are locked in.

⚠️ **CRITICAL**: No US work can begin until this phase is complete.

- [X] T003 Audit existing golden fixtures for the m190 drift set per research §R8: results in `specs/190-ipk-emission-parity/scratch/drift-set.txt`. **Zero hits** for both raw BitBake operators and inline-epoch PURLs in existing goldens. Phase 6 T032 regen is effectively a no-op.
- [X] T004 [P] Verify current SPDX 3 emitter behavior for ipk components: confirmed via code inspection of `spdx/v3_licenses.rs:61` — empty `licenses` Vec → no `LicenseExpression` element emitted. Compound-license inputs currently fail canonicalization → lenient fallback stores raw string → `LicenseExpression` DOES get emitted, but with a value that is NOT SPDX-canonical (has raw `&`/`|`). The R1 fix will canonicalize it.
- [X] T005 [P] SPDX 2.3 empty-license behavior: confirmed at `packages.rs:245-247` — empty Vec → `SpdxLicenseField::NoAssertion` → serializes to `"licenseDeclared": "NOASSERTION"`. Matches Q3 answer B.
- [X] T006 [P] CDX 1.6 empty-license behavior: confirmed at `builder.rs:937` — `if !final_licenses.is_empty()` guard means empty → `.licenses` field omitted entirely. Matches Q3 answer B.

**Checkpoint**: Drift set captured; emit-path assumptions locked in.

---

## Phase 3: User Story 1 — CDX License Operator Normalization (Priority: P1) 🎯 MVP

**Goal**: BitBake `&`/`|`/`&&`/`||` operators in ipk `License:` fields are normalized to SPDX `AND`/`OR` before reaching any emitter, so CDX 1.6 `components[].licenses[].expression` (and by side effect SPDX 2.3 `licenseDeclared` and SPDX 3 `simplelicensing_LicenseExpression`) all carry SPDX-canonical values. Closes issue #550.

**Independent Test**: Scan a synthetic ipk with `License: GPL-2.0-only & MIT`. Assert the CDX `.components[].licenses[].expression` is `GPL-2.0-only AND MIT`, contains no raw `&` outside quoted operand names, and the SPDX 2.3 `.licenseDeclared` for the same component is the same canonical string (proves cross-format parity per US1 acceptance scenario #4).

### Tests for User Story 1

> **Write tests FIRST; ensure they FAIL against the pre-m190 tree before implementation.**

- [X] T007 [P] [US1] Add unit tests for `normalize_bitbake_license_operators` in `mikebom-cli/src/scan_fs/package_db/ipk_file.rs` (co-located `#[cfg(test)] mod tests` block). Cover: single `&`, single `|`, double `&&`, double `||`, mixed whitespace (`MIT&&Apache-2.0`, `MIT && Apache-2.0`, `MIT  &&  Apache-2.0`), nested grouping (`(A & B) | C`), idempotence (calling twice equals once), literal-in-string preservation (though License fields don't have quoted strings — sanity test only).
- [X] T008 [P] [US1] Create synthetic ipk fixtures for compound-license tests in `mikebom-cli/tests/fixtures/ipk_m190/`. Files:
  - `mit_only.ipk` (License: `MIT` — byte-identity baseline)
  - `bitbake_and.ipk` (License: `GPL-2.0-only & MIT`)
  - `bitbake_or.ipk` (License: `MIT | Apache-2.0`)
  - `bitbake_double_ops.ipk` (License: `MIT && Apache-2.0 || BSD-2-Clause`)
  - `bitbake_grouped.ipk` (License: `(GPL-2.0-only & MIT) | Apache-2.0`)
  - `vendor_license.ipk` (License: `SomeVendorLicense`)
  - `empty_license.ipk` (License field missing)
  Reuse the m187 ar-format fixture-builder helper (or synthesize at test-init time from string literals if the m187 helper is not exposed for reuse).
- [X] T009 [P] [US1] Add integration test `mikebom-cli/tests/ipk_license_parity.rs` with a helper `assert_cdx_expression_matches(fixture, expected)` that scans a fixture with `--format cyclonedx-json` and asserts the `.components[?(@.name=='<pkg>')].licenses[0].expression` value. Include cases for each fixture from T008 (except empty-license, deferred to T023).

### Implementation for User Story 1

- [X] T010 [US1] Implement `normalize_bitbake_license_operators(raw: &str) -> String` in `mikebom-cli/src/scan_fs/package_db/ipk_file.rs`. Per data-model.md — 4 sequential `str::replace` calls in long-form-first order (`&&`, `||`, `&`, `|`), each substituting to ` AND `/` OR `. Add a `//` doc-comment referencing spec §Q1 + research §R1 explaining the ordering invariant.
- [X] T011 [US1] Wire `normalize_bitbake_license_operators` into the license parsing at `mikebom-cli/src/scan_fs/package_db/ipk_file.rs:824-840`. Replace the `raw` argument to `SpdxExpression::try_canonical` with `&normalized_raw`. Ensure the m152 LicenseRef fallback continues to receive the normalized string (so vendor operands still LicenseRef-encode consistently across formats).
- [X] T012 [US1] Run T007's unit tests locally: `cargo test -p mikebom --lib ipk_file::tests::` — MUST pass.
- [X] T013 [US1] Run T009's integration tests locally: `cargo test -p mikebom --test ipk_license_parity` — MUST pass. If any fixture from T008 fails (raw operators still leak through), the wiring in T011 is incomplete.

**Checkpoint**: Compound-license CDX `.expression` values are SPDX-canonical for every T008 fixture. US1 acceptance scenarios #1, #2, #3, #4, #5 all pass. Cross-format parity for CDX vs SPDX 2.3 confirmed via T013's helper.

---

## Phase 4: User Story 2 — SPDX 3 License Emission (Priority: P1)

**Goal**: SPDX 3 `software_Package` elements derived from ipks with non-empty license fields carry `simplelicensing_LicenseExpression` (or `simplelicensing_CustomLicense` for vendor operands) elements, wired via `hasDeclaredLicense` relationship. Closes issue #551.

**Independent Test**: Scan a synthetic ipk with `License: GPL-2.0-only & MIT` and produce SPDX 3 output. Assert `.["@graph"][?(@.type=='simplelicensing_LicenseExpression')].simplelicensing_licenseExpression == 'GPL-2.0-only AND MIT'` and that the ipk's `software_Package` element has a `hasDeclaredLicense` relationship linking to it.

**Dependency note**: Per research §R2, US2 is likely resolved transitively by the US1 fix (T010–T013). US2 tests here confirm that hypothesis. If tests fail, US2 escalates to a routing-bug investigation.

### Tests for User Story 2

- [X] T014 [P] [US2] Add integration test `mikebom-cli/tests/ipk_license_parity.rs::spdx3_emits_license_expression_for_compound_license` — scan the `bitbake_and.ipk` fixture from T008 with `--format spdx-3-json`, assert THREE conditions (per FR-006 relationship coverage): (a) `[.["@graph"][] | select(.type == "simplelicensing_LicenseExpression")] | length >= 1` AND at least one such element has `simplelicensing_licenseExpression == "GPL-2.0-only AND MIT"`; (b) `[.["@graph"][] | select(.type == "software_Package" and .name == "bitbake-and-fixture")]` returns exactly one package IRI; (c) `[.["@graph"][] | select(.type == "Relationship" and .relationshipType == "hasDeclaredLicense" and .from == "<package-iri-from-b>")]` is non-empty AND its `to[0]` matches the `spdxId` of the LicenseExpression from (a). This is the direct #551 regression gate PLUS the FR-006 relationship coverage.
- [X] T015 [P] [US2] Add integration test `mikebom-cli/tests/ipk_license_parity.rs::spdx3_emits_custom_license_for_vendor_operand` — scan the `vendor_license.ipk` fixture with `--format spdx-3-json`, assert at least one `simplelicensing_CustomLicense` element is emitted (byproduct of the existing m154 sweep at `v3_licenses.rs::sweep_custom_licenses`).
- [X] T016 [P] [US2] Add cross-format parity assertion in `mikebom-cli/tests/ipk_license_parity.rs::cross_format_license_equality` — for the `bitbake_and.ipk` fixture, assert CDX `.components[X].licenses[0].expression`, SPDX 2.3 `.packages[X].licenseDeclared`, and SPDX 3 `simplelicensing_licenseExpression` all canonicalize to the same value via `SpdxExpression::try_canonical`. Verifies FR-013.
- [X] T017 [P] [US2] Extend the existing `spdx3-validate` gate to cover the `bitbake_and.ipk` fixture. Find the current spdx3-validate CI hook (likely `mikebom-cli/tests/spdx3_regression.rs` or an integration harness); add a fixture case that runs `.venv/spdx3-validate/bin/spdx3-validate <output>` on the m190 compound-license SBOM and asserts exit 0.

### Implementation for User Story 2

- [X] T018 [US2] Run T014, T015, T016 locally — likely PASS transitively via the US1 fix. If they PASS, US2 requires no additional code change; proceed to T019.
- [X] T019 [US2] If T014/T015/T016 FAIL despite the US1 fix: investigate per research §R2. Grep the pipeline for `ResolvedComponent.licenses` population between `mikebom-cli/src/scan_fs/package_db/ipk_file.rs::parse_control_stanza` and `mikebom-cli/src/generate/spdx/v3_licenses.rs::build_license_elements_and_relationships`. Verify `package_iri_by_purl` includes ipk PURLs. Fix the discovered routing gap; document the finding as a new `## Investigation Findings` section in research.md.
- [X] T020 [US2] Run T017's spdx3-validate assertion — MUST exit 0 with zero conformance errors.

**Checkpoint**: SPDX 3 output for every non-empty-license ipk fixture carries a `simplelicensing_LicenseExpression` (or `simplelicensing_CustomLicense`) element. spdx3-validate accepts the output. US2 acceptance scenarios #1, #2, #3, #4, #5 all pass.

---

## Phase 5: User Story 3 — ipk PURL Epoch Qualifier (Priority: P2)

**Goal**: ipks with non-zero epoch prefixes (`Version: <digits>:<rest>`) emit PURLs with `?epoch=<N>` qualifiers and store the naked version in `.version` — no inline `<digits>:` remnant anywhere. Closes issue #552.

**Independent Test**: Scan a synthetic ipk with `Version: 1:2.0-r0`. Assert the emitted PURL contains `&epoch=1` (positioned alphabetically after `arch=`), `.version == "2.0-r0"`, and the PURL for the same package emitted by CDX + SPDX 2.3 + SPDX 3 is byte-identical.

### Tests for User Story 3

- [X] T021 [P] [US3] Add unit tests for `parse_opkg_version_with_epoch` in `mikebom-cli/src/scan_fs/package_db/ipk_file.rs` (`#[cfg(test)] mod tests`). Cover: no prefix (`2.0-r0`), non-zero prefix (`1:2.0-r0`), zero prefix (`0:1.0-r0`), multi-colon (`1:2.0-r0:beta`), non-digit prefix (`abc:1.0-r0`), overflow-guard (`999999999999999:2.0-r0` — must not panic; returns `None`), empty string, whitespace only.
- [X] T022 [P] [US3] Create synthetic ipk fixtures for epoch tests in `mikebom-cli/tests/fixtures/ipk_m190/`:
  - `epoch_positive.ipk` — filename `epoch-fix_1:6.4-r0_all.ipk`, control `Version: 1:6.4-r0`.
  - `epoch_zero.ipk` — filename `epoch-fix_0:1.0-r0_all.ipk`, control `Version: 0:1.0-r0`.
  - `epoch_none.ipk` — filename `epoch-fix_2.0-r0_all.ipk`, control `Version: 2.0-r0` (byte-identity baseline).
  - `epoch_control_only.ipk` — filename lacks epoch (`test_2.0-r0_all.ipk`), control has `Version: 3:2.0-r0` (FR-012 control-wins case).
  - `epoch_filename_only.ipk` — filename `legacy_5:1.0-r0_all.ipk` (pre-2015 opkg-build style), control-file **missing entirely** so the reader takes the filename-fallback path per the `ipk_file.rs` module docstring. Exercises FR-012's filename-source branch. Expected: emitted PURL carries `&epoch=5`, `.version == "1.0-r0"`.
- [X] T023 [P] [US3] Add integration test `mikebom-cli/tests/ipk_epoch_purl.rs` with assertions:
  - `epoch_positive.ipk` produces CDX PURL `pkg:opkg/epoch-fix@6.4-r0?arch=all&epoch=1`, `.version == "6.4-r0"`.
  - `epoch_zero.ipk` produces CDX PURL `pkg:opkg/epoch-fix@1.0-r0?arch=all`, `.version == "1.0-r0"` (no `?epoch=0`).
  - `epoch_none.ipk` produces the pre-m190 baseline PURL byte-identically (FR-011 / SC-006).
  - `epoch_control_only.ipk` produces PURL with `&epoch=3` (control-file takes precedence over filename per FR-012).
  - `epoch_filename_only.ipk` produces PURL with `&epoch=5` and `.version == "1.0-r0"` (FR-012 filename-source branch — closes the legacy-fallback coverage gap identified in speckit-analyze).
  - Cross-format PURL parity: CDX `.purl` == SPDX 2.3 `externalRefs[?(@.referenceType=='purl')].referenceLocator` == SPDX 3 `software_packageUrl` for every fixture.

### Implementation for User Story 3

- [X] T024 [US3] Implement `parse_opkg_version_with_epoch(raw: &str) -> (Option<u32>, String)` in `mikebom-cli/src/scan_fs/package_db/ipk_file.rs`. Per data-model.md — regex `^(\d+):(.*)$` via `std::sync::OnceLock<regex::Regex>`. Doc-comment referencing spec FR-008/FR-009/FR-010 + research §R4.
- [X] T025 [US3] Add `epoch: Option<u32>` field to the parsed control record struct in `mikebom-cli/src/scan_fs/package_db/ipk_file.rs`. Initialize to `None` in every existing constructor site so the file compiles cleanly at this task boundary.
- [X] T026 [US3] Extend `build_opkg_purl` signature in `mikebom-cli/src/scan_fs/package_db/ipk_file.rs:1087+` to accept `epoch: Option<u32>`. Append `&epoch=<N>` qualifier when `epoch == Some(v) && v != 0`, positioned alphabetically per purl-spec §5.6 (after `arch=` and `distro=` if present; alphabetical is `arch < distro < epoch`). Add a co-located unit test asserting alphabetical qualifier ordering.
- [X] T027 [US3] Wire `parse_opkg_version_with_epoch` into `parse_control_stanza` / `assemble_entry` (whichever function currently builds the parsed record from the raw `Version:` field): call the new parser, store both the epoch and the naked-version, pass both into `build_opkg_purl`. Verify at every existing `build_opkg_purl(...)` call site that the argument list is updated.
- [X] T028 [US3] Update every existing caller of `build_opkg_purl` to pass `epoch: None` when epoch isn't relevant (e.g., filename-fallback paths that don't parse the control file) so the type-checker enforces intentional-vs-accidental omission.
- [X] T029 [US3] Run T021's unit tests and T023's integration tests locally: `cargo test -p mikebom ipk_epoch_purl` and `cargo test -p mikebom --lib ipk_file::tests::parse_opkg_version_with_epoch` — MUST pass.

**Checkpoint**: All ipks with non-zero epoch prefixes emit PURLs with `&epoch=N` qualifiers; all no-epoch ipks preserve byte-identity. US3 acceptance scenarios #1, #2, #3, #4 all pass.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Cross-milestone verification, golden regen, real-world validation, docs.

- [X] T030 [P] Empty-license behavior verification per Q3 answer B (research §R3). Scan `empty_license.ipk` (T008) with all three formats; assert:
  - SPDX 2.3 `.packages[].licenseDeclared == "NOASSERTION"`,
  - CDX 1.6 `.components[].licenses` omitted OR `[]`,
  - SPDX 3 emits `software_Package` element but no `hasDeclaredLicense` relationship AND no `simplelicensing_LicenseExpression` for it.
  If SPDX 2.3 or CDX diverge from this, extend the ipk reader / emitter to align — file scope as sub-task inside T030. Add integration test coverage in `mikebom-cli/tests/ipk_license_parity.rs::empty_license_uses_format_idiomatic_absent_marker`.
- [X] T031 [P] Cross-parity extractor catalog check: grep `mikebom-cli/src/parity/extractors/` for any existing catalog row that touches CDX `licenses.expression` OR SPDX 3 `simplelicensing_LicenseExpression`. If a row exists and its Directionality is `SymmetricEqual`, verify the m190 fixtures pass the parity extractor test (existing test in `mikebom-cli/tests/parity_catalog.rs` or similar). If NO row exists, this is expected — the license values are inherently symmetric via `SpdxExpression::try_canonical`, no new catalog row needed.
- [X] T032 Regenerate the drift-set goldens identified in T003. Use the "targeted regen" approach per memory `feedback_release_bump_regen_all_golden_tests`:
  ```bash
  MIKEBOM_UPDATE_CDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX3_GOLDENS=1 \
    cargo test -p mikebom --test cdx_regression --test spdx_regression --test spdx3_regression \
    --test pkg_alias_binding_us1 --test oci_pull_backward_compat --test optional_dep_classification
  ```
  Diff-review the resulting changes: every diff MUST be either (a) `&`/`|` → `AND`/`OR`, (b) `<digits>:<version>` → `<version>?epoch=<digits>`, or (c) a consequential canonicalization side-effect. Reject any other class of diff — investigate before committing.
- [X] T033 Non-drift byte-identity gate — run the full test suite; every golden test NOT in the drift set (T003) MUST pass byte-identically. This is SC-006's enforcement. If any non-drift golden fails, investigate before merge.
- [X] T034 [P] Real-world Yocto validation per quickstart.md Reproducer 2 — deferred (no Yocto build tree locally available; opt-in per spec). SC-005 acceptance falls to consumer validation post-release. — if a Yocto build tree is available locally (or accessible via a pinned image), run mikebom against it with all three formats and confirm:
  - No raw `&`/`|` operators in any CDX license expression;
  - SPDX 3 emits at least one `simplelicensing_LicenseExpression` element for each non-empty-license ipk (issue #551 signal);
  - At least one PURL includes `&epoch=<N>` (netbase on core-image-minimal is the canonical example);
  - spdx3-validate accepts the SPDX 3 output.
  Record findings in `specs/190-ipk-emission-parity/scratch/real-world-validation.txt`.
- [X] T035 File follow-up issues for dpkg + apk epoch audit (per Q4 answer A / assumption bullet in spec). Draft two GitHub issue bodies (do NOT open until the user approves in the merge PR):
  - "sbom scan (dpkg): audit for inline-epoch PURL bug (m190 follow-up to #552)"
  - "sbom scan (apk): audit for inline-epoch PURL bug (m190 follow-up to #552)"
  Store drafts in `specs/190-ipk-emission-parity/scratch/followup-issues.md`.
- [X] T036 [P] Update the CLAUDE.md agent-context "Active Technologies" and "Recent Changes" sections to reflect m190. If the auto-updater at `.specify/scripts/bash/update-agent-context.sh` was already invoked during `/speckit-plan`, verify the current CLAUDE.md lists 190-ipk-emission-parity in the Recent Changes section.
- [X] T037 Pre-PR gate — completed. `./scripts/pre-pr.sh` exit 0. Clippy clean (`--workspace --all-targets -- -D warnings` — zero errors, zero warnings). Tests: 241 suites `ok. N passed; 0 failed`. Log at `/tmp/m190_pre_pr.log`. — run `./scripts/pre-pr.sh` (from workspace root) and confirm BOTH commands pass clean per memory `feedback_prepr_gate_full_output`. Every test suite MUST report `ok. N passed; 0 failed`; clippy MUST report zero errors AND zero warnings. Post the per-suite summary in the eventual PR body, not a failure-grep result.

**Checkpoint**: Full workspace clippy clean, full workspace test suite green, real-world Yocto smoke-test passing, drift-set goldens regenerated with explainable diffs, non-drift goldens byte-identical.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: T001, T002. No dependencies. T002 is preflight baseline.
- **Foundational (Phase 2)**: T003–T006. Depends on Setup. BLOCKS all US phases.
- **US1 (Phase 3)**: Depends on Foundational. `Independent`: does NOT depend on US2 or US3.
- **US2 (Phase 4)**: Depends on Foundational AND on US1 landing (specifically T010–T011: the R1 preprocessing pass is a prerequisite for US2's tests to pass transitively).
- **US3 (Phase 5)**: Depends on Foundational ONLY. Fully parallel to US1 and US2 — different code path (version parser vs license normalizer), different fixtures.
- **Polish (Phase 6)**: Depends on all three US phases complete.

### User Story Dependencies

- **US1 (P1) → US2 (P1)**: US2's happy-path tests (T014–T016) pass transitively via US1's normalization fix. US2 is not "blocked" by US1 in a hard-dependency sense but is empirically coupled — schedule US1 first for the smoothest US2 verification.
- **US1 (P1) ⊥ US3 (P2)**: No coupling. Different fixtures, different reader-code paths, different emitter fields.
- **US2 (P1) ⊥ US3 (P2)**: No coupling. Different concerns.

### Within Each User Story

- Tests BEFORE implementation (matches mikebom's standard TDD approach for ipk-reader milestones).
- Unit tests before integration tests.
- Fixture creation ([P] tasks marked so; can run in parallel with unit-test authoring).

### Parallel Opportunities

- Phase 2 T003 must complete before T004/T005/T006 (they consume the drift set for context), but T004/T005/T006 can run in parallel with each other.
- Phase 3 T007, T008, T009 all `[P]` — parallel test authoring.
- Phase 3 T010 → T011 sequential (both edit `ipk_file.rs`).
- Phase 4 T014, T015, T016, T017 all `[P]` — parallel test authoring.
- Phase 5 T021, T022, T023 all `[P]` — parallel test authoring.
- Phase 5 T024, T025, T026 edit the same file (`ipk_file.rs`) — sequential.
- Phase 5 T027, T028 depend on T024–T026 landing.
- Phase 6 T030, T031, T034, T036 all `[P]`.

Different developers CAN split US1, US2, US3 across three branches after Phase 2 lands. Recommended pattern: land US1 → US2 → US3 as three commits on the same PR (matches every recent ipk-reader milestone's PR shape) so the CI-drift-check happens once at the end.

---

## Parallel Example: User Story 1

```bash
# All test-authoring tasks in parallel:
Task: "T007 Add unit tests for normalize_bitbake_license_operators"
Task: "T008 Create synthetic ipk fixtures for compound-license tests"
Task: "T009 Add integration test scaffold ipk_license_parity.rs"

# After T010–T011 (sequential impl):
Task: "T012 Run unit tests"
Task: "T013 Run integration tests"
```

---

## Implementation Strategy

### MVP (User Story 1 Only)

1. Complete Phase 1: Setup (T001–T002).
2. Complete Phase 2: Foundational (T003–T006). Drift set captured.
3. Complete Phase 3: US1 (T007–T013). CDX license normalization landed.
4. **STOP and VALIDATE**: run `./scripts/pre-pr.sh` clean; scan a compound-license fixture manually per quickstart.md Reproducer 1 Assertion 1. Ship as a #550-only fix if #551 + #552 need more time.

### Incremental Delivery (recommended)

1. Phase 1 + 2 → foundation ready.
2. Phase 3 (US1) → validate → ship #550 fix (partial milestone) OR continue.
3. Phase 4 (US2) → validate → ship #550 + #551 fixes together.
4. Phase 5 (US3) → validate → ship all three fixes as m190 alpha.61.
5. Phase 6 → docs + golden regen + real-world verification.

### Single-PR Delivery (matches every recent ipk milestone)

Land Phases 1–6 in a single PR titled "impl(190): ipk emission parity — CDX license normalization + SPDX 3 license emission + epoch qualifier". Commit granularity per phase, reviewer digestibility per US.

---

## Notes

- Total tasks: 37 across 6 phases.
- US1: 7 tasks (T007–T013). US2: 7 tasks (T014–T020). US3: 9 tasks (T021–T029). Setup/Foundational/Polish: 14 tasks.
- Every `[P]` task edits a distinct file or a distinct #[cfg(test)] block; no file-collision hazards among parallel tasks.
- Every task has an exact file path (or fixture directory) so no follow-up scoping needed.
- Zero new Cargo dependencies (FR: research §R9 audit satisfied); zero new `mikebom:*` annotations (FR-015).
- Byte-identity gate (SC-006) is enforced at T033 as a HARD blocker for merge.
- Real-world smoke (T034) is `[P]` because it's a validation-only step; failure does NOT block merge but MUST be surfaced in the PR body.
