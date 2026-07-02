---
description: "Task list for milestone 154 — SPDX 3 simplelicensing_CustomLicense for LicenseRef-* (closes issue #487)"
---

# Tasks: SPDX 3 `simplelicensing_CustomLicense` for LicenseRef-* — milestone 154

**Input**: Design documents from `/specs/154-spdx3-custom-licenses/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/sweep-api.md ✓, quickstart.md ✓

**Tests**: Tests are part of the implementation per the milestone-152/153/478 convention (inline `#[cfg(test)] mod tests` in the affected Rust file). Spec SC-006 enumerates ≥5 new unit tests; research.md §R6 lists the canonical 6 (5 required + 1 bonus cross-format identity test). No separate test-only phase.

**Organization**: Tasks are grouped by user story. The primary deliverable is one Rust file (`mikebom-cli/src/generate/spdx/v3_licenses.rs`) plus a 1-line visibility change in `mikebom-cli/src/generate/spdx/document.rs` and a ~3-line wiring change in `mikebom-cli/src/generate/spdx/v3_document.rs`. `[P]` markers indicate "no semantic dependency" rather than physical-parallel file edits.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: No semantic dependency on other tasks in the same phase
- **[Story]**: Which user story this task belongs to (US1, US2)
- File paths are exact

## Path Conventions

- **Primary Rust deliverable**: `mikebom-cli/src/generate/spdx/v3_licenses.rs`
- **Const visibility change**: `mikebom-cli/src/generate/spdx/document.rs` (1 line)
- **Wiring change**: `mikebom-cli/src/generate/spdx/v3_document.rs` (~3 lines)
- **CHANGELOG**: `CHANGELOG.md`
- **Authoring artifacts**: `specs/154-spdx3-custom-licenses/*.md`
- **Untouched references** (READ-ONLY): `docs/reference/sbom-format-mapping.md`, `mikebom-common/`, `mikebom-ebpf/`, `mikebom-cli/src/generate/cyclonedx/`, `mikebom-cli/src/scan_fs/`, and every other file in `mikebom-cli/src/generate/spdx/` besides the 3 listed above

---

## Phase 1: Setup

**Purpose**: Verify baseline + confirm integration site line numbers + confirm reusable milestone-153 assets.

- [X] T001 Verify pre-PR baseline on `main` by running `./scripts/pre-pr.sh` from the repo root; confirm the documented `sbomqs_parity::sbomqs_spdx_score_meets_or_beats_cdx_across_ecosystems` env-only failure is the ONLY pre-existing failure permitted in SC-005. (May be skipped if the last recent pre-PR run — e.g., milestone 153 close-out — already established the baseline; T023 is the definitive gate for THIS milestone's work.)
- [X] T002 [P] Open `mikebom-cli/src/generate/spdx/v3_licenses.rs` and confirm the exact line numbers: (a) end of `build_license_elements_and_relationships` function (planning-time: line ~118); (b) `element_iri_for` helper (planning-time: line ~126); (c) `#[cfg(test)] mod tests` block (add if absent — the file may not have inline tests yet). Record actual line numbers for T007 authoring.
- [X] T003 [P] Open `mikebom-cli/src/generate/spdx/document.rs` and confirm the exact line number of `const PLACEHOLDER_EXTRACTED_TEXT` declaration (planning-time: line ~228 — added in milestone 153). Record for T004's visibility change.
- [X] T004 [P] Open `mikebom-cli/src/generate/spdx/v3_document.rs` and confirm the exact line numbers of the `build_license_elements_and_relationships` call site (planning-time: lines 581-587) and the immediately-following `for elem in license_elements` push loop (planning-time: lines 588-590). Record for T009 wiring.

---

## Phase 2: Foundational

**Purpose**: Promote the milestone-153 placeholder const to `pub(crate)` + add the regex helper to `v3_licenses.rs`. Both are prerequisites for US1's sweep helper (T007).

**⚠️ CRITICAL**: T005 + T006 must complete before US1's sweep helper (T007) is added.

- [X] T005 Change `const PLACEHOLDER_EXTRACTED_TEXT: &str = ...` to `pub(crate) const PLACEHOLDER_EXTRACTED_TEXT: &str = ...` in `mikebom-cli/src/generate/spdx/document.rs` at the line identified in T003. The string VALUE MUST remain byte-identical to milestone 153 — only the visibility modifier changes. This single-source-of-truth promotion is the mechanical enforcement of FR-010 (cross-format placeholder identity). No other edit to `document.rs` in this task.
- [X] T006 Add the `license_ref_regex()` helper + `LICENSE_REF_REGEX: OnceLock<Regex>` static to `mikebom-cli/src/generate/spdx/v3_licenses.rs` immediately after the existing `element_iri_for` helper (line identified in T002). Pattern: `(?:^|[^:])(LicenseRef-[a-zA-Z0-9.-]+)` per data-model.md §2 — BYTE-IDENTICAL to milestone-153's `document.rs::license_ref_regex()` per research.md §R3 lockstep invariant. Include a doc comment naming milestone 153's helper as the source-of-truth reference AND stating the drift-warning ("any future change to milestone 153's regex MUST be mirrored here and vice versa").

**Checkpoint**: Phase 2 complete — foundational assets in place. US1 can build the sweep on top.

---

## Phase 3: User Story 1 — Cross-format symmetry for LicenseRef resolution (Priority: P1) 🎯 MVP

**Goal**: After this US ships, every emitted mikebom SPDX 3 document that contains any `LicenseRef-*` inline in a `simplelicensing_LicenseExpression` has a matching `simplelicensing_CustomLicense` graph element. The SPDX 2.3 side (milestone 153) and the SPDX 3 side (this milestone) describe the same LicenseRef set with byte-identical placeholder text.

**Independent Test**: Per quickstart.md Scenario 1 — manual operator-cadence verification against the Yocto testbed with 4 jq assertions covering name set, cross-format set equality, cross-format placeholder identity, and IRI scheme. Plus 5 inline unit tests covering single/compound/dedup/DocumentRef-exclusion/cross-format-identity per research.md §R6.

### Implementation for User Story 1

- [X] T007 [US1] Add the `sweep_custom_licenses(license_expression_elements: &[Value], doc_iri: &str, creation_info_id: &str) -> Vec<Value>` helper function to `mikebom-cli/src/generate/spdx/v3_licenses.rs` immediately after `license_ref_regex()`. Implementation per data-model.md §1 + contracts/sweep-api.md Contracts 1-4: iterate over `license_expression_elements`, tolerating (no-op on) entries with `type != "simplelicensing_LicenseExpression"`; for matching entries, extract capture-group-1 matches of `license_ref_regex()` from the `simplelicensing_licenseExpression` string; strip the `LicenseRef-` prefix to derive the idstring; dedup by idstring via `BTreeMap<String, Value>`; for each distinct idstring construct a `simplelicensing_CustomLicense` JSON element with 5 fields (type / spdxId / creationInfo / name / simplelicensing_licenseText) per data-model.md §3; the `spdxId` is `format!("{doc_iri}/licenseref/{idstring}")` per Clarifications Q1; the `simplelicensing_licenseText` is `PLACEHOLDER_EXTRACTED_TEXT.to_string()`; return `map.into_values().collect()` (already sorted lex by idstring via BTreeMap key ordering, equivalent to sort-by-spdxId). **Also add** (per analysis remediation A2): the import `use super::document::PLACEHOLDER_EXTRACTED_TEXT;` at the top of `v3_licenses.rs` alongside the existing `use mikebom_common::...` + `use serde_json::{json, Value};` imports (required for the helper to compile — folded into T007 so imports + helper land in one atomic authoring pass).
- [X] T008 [P] [US1] Verification step (per analysis remediation A2 — T008 folded into T007 for the actual authoring): confirm the `use super::document::PLACEHOLDER_EXTRACTED_TEXT;` import is present at the top of `mikebom-cli/src/generate/spdx/v3_licenses.rs` after T007 lands. Simple grep check: `grep -n "use super::document::PLACEHOLDER_EXTRACTED_TEXT" mikebom-cli/src/generate/spdx/v3_licenses.rs` returns 1 match. Ensures the imported const is single-sourced from milestone 153's `document.rs` per FR-018.
- [X] T009 [US1] Wire the sweep at the integration site in `mikebom-cli/src/generate/spdx/v3_document.rs` immediately after the `build_license_elements_and_relationships` call site (line identified in T004) per data-model.md §5. Add 4-line block: (1) 3-line comment naming milestone 154 + issue #487; (2) `let custom_license_elements = super::v3_licenses::sweep_custom_licenses(&license_elements, &doc_iri, CREATION_INFO_ID);`; (3) preserve the existing `for elem in license_elements { graph.push(elem); }` loop unchanged; (4) add `for elem in custom_license_elements { graph.push(elem); }` immediately after it. Net delta: 4 lines → ~13 lines. Existing `license_elements` push loop and `build_license_elements_and_relationships` call site both UNCHANGED besides the added lines.
- [X] T010 [P] [US1] Add unit test `sweep_custom_licenses_single_expression_single_licenseref` to the `v3_licenses.rs` test module. Assert: input `[serde_json::json!({"type": "simplelicensing_LicenseExpression", "simplelicensing_licenseExpression": "LicenseRef-PD"})]` with `doc_iri = "https://example.com/doc"` and `creation_info_id = "_:creation-info"` produces exactly 1 output element with `type = "simplelicensing_CustomLicense"`, `spdxId = "https://example.com/doc/licenseref/PD"`, `name = "PD"`, `simplelicensing_licenseText = PLACEHOLDER_EXTRACTED_TEXT`, `creationInfo = "_:creation-info"`. Covers US1 A2 (liblzma5 case).
- [X] T011 [P] [US1] Add unit test `sweep_custom_licenses_compound_expression` — assert input `simplelicensing_licenseExpression = "GPL-2.0-only AND LicenseRef-bzip2-1.0.4"` produces exactly 1 element for `LicenseRef-bzip2-1.0.4` with `spdxId = "https://example.com/doc/licenseref/bzip2-1.0.4"` and `name = "bzip2-1.0.4"` (the GPL-2.0-only substring is NOT extracted — it's a bare SPDX id, not a LicenseRef). Covers US1 A1 (busybox case).
- [X] T012 [P] [US1] Add unit test `sweep_custom_licenses_dedup_across_expressions` — construct 4 synthetic elements all with `simplelicensing_licenseExpression = "GPL-2.0-only AND LicenseRef-bzip2-1.0.4"` (busybox-family scenario); assert the returned Vec contains exactly 1 element for `bzip2-1.0.4` (dedup by idstring). Covers US1 A3.
- [X] T012a [P] [US1] Add unit test `sweep_custom_licenses_nested_compound_structure` (per analysis remediation A1 — closes the FR-005 unit-test gap). Assert input `simplelicensing_licenseExpression = "MIT AND LicenseRef-foo OR (LicenseRef-bar AND Apache-2.0)"` produces exactly 2 elements: one for `LicenseRef-foo`, one for `LicenseRef-bar`. Verify BTreeMap lex-ordering: returned Vec[0] is `LicenseRef-bar` (idstring `bar` sorts before `foo`), Vec[1] is `LicenseRef-foo`. Both extracted regardless of operator (AND/OR) surroundings and paren nesting. Covers FR-005 (nested compound with multiple distinct LicenseRefs — parallel to milestone-153's `sweep_covers_nested_compound_structure` test).
- [X] T013 [P] [US1] Add unit test `sweep_custom_licenses_ignores_document_ref_prefixed` — assert input with `simplelicensing_licenseExpression = "MIT AND DocumentRef-external:LicenseRef-foo"` produces zero output elements (regex non-capturing prefix filters DocumentRef-prefixed compound). Covers Edge Case per spec.
- [X] T014 [P] [US1] Add unit test `cross_format_placeholder_identity` (the "bonus" test per research.md §R6). Assert `super::document::PLACEHOLDER_EXTRACTED_TEXT` starts with the byte-exact prefix `"License text not extracted by mikebom."` (per Clarifications Q1 wire contract from milestone 153). This test doubles as a compile-time regression guard: any accidental change to the const value in `document.rs` trips this test AND milestone-153's own `placeholder_text_matches_wire_contract` test simultaneously. Locks FR-010 (cross-format placeholder identity).

**Checkpoint**: §3 produces the SC-001 cross-format symmetry fix + comprehensive unit coverage. The 3 issue-#487 reference LicenseRefs are provably handled by tests T010–T012; nested compound structure by T012a (per A1 remediation); DocumentRef exclusion by T013; wire-contract identity locked by T014.

---

## Phase 4: User Story 2 — Byte-identical happy path when no LicenseRef is present (Priority: P2)

**Goal**: After this US ships, mikebom is provably safe — the sweep is a strict no-op on scans that don't emit any LicenseRef-*. SC-002 verified via 1 unit test + the existing SPDX 3 golden test infrastructure.

**Independent Test**: Unit test + the existing SPDX 3 golden tests. No fixture regeneration should be needed.

### Implementation for User Story 2

- [X] T015 [P] [US2] Add unit test `sweep_custom_licenses_no_licenserefs_returns_empty` to the `v3_licenses.rs` test module — construct synthetic `simplelicensing_LicenseExpression` elements where every `simplelicensing_licenseExpression` is a canonical SPDX id (e.g., `"MIT"`, `"Apache-2.0 AND GPL-2.0-only"`); assert the returned Vec is empty (`.is_empty()`). This validates the empty-in-empty-out invariant per FR-007 + Contract 6. Combined with the wiring at T009 (push each returned element onto `@graph`), an empty return means zero new `simplelicensing_CustomLicense` elements appear in `@graph` — byte-identity preserved for happy-path scans that never trigger milestone-152's LicenseRef fallback.

**Checkpoint**: §4 produces the SC-002 safeguard. The sweep is compile-time-locked to no-op behavior on happy-path input.

---

## Phase 5: Polish & cross-cutting

**Purpose**: CHANGELOG entry + audit scenarios + pre-PR gate + PR description.

- [X] T016 [P] Add the milestone-154 CHANGELOG.md entry under `## [Unreleased]`, immediately above milestone-153's entry (chronological within the section — 154 later than 153 but shipped in the same release cadence). Content per research.md §R7: (a) SPDX 3 symmetry fix + issue #487 reference; (b) the byte-identical cross-format placeholder guarantee (invariant with milestone 153, mechanically enforced via `pub(crate)` visibility promotion); (c) the IRI scheme `{doc_iri}/licenseref/{idstring}` from Clarifications Q1; (d) a cross-format jq recipe consumers can use to verify parity between the SPDX 2.3 and SPDX 3 outputs of the same scan. **Precheck** (per milestone-152/153 A3 remediation pattern): run `head -20 CHANGELOG.md` first to confirm the `## [Unreleased]` heading exists.
- [X] T017 [P] Run quickstart.md Scenario 6 (SC-006 test count audit): `grep -cE "^\s+fn sweep_custom_licenses_" mikebom-cli/src/generate/spdx/v3_licenses.rs` MUST return ≥6 (5 required by SC-006 floor + 1 added per analysis remediation A1's `sweep_custom_licenses_nested_compound_structure`). Plus the bonus `cross_format_placeholder_identity` test (T014) brings the total to 7 milestone-154-added tests.
- [X] T018 [P] Run quickstart.md Scenario 7 (SC-007 wire-format guard): assert `git diff main --name-only -- docs/ mikebom-common/ mikebom-ebpf/ mikebom-cli/src/generate/cyclonedx/ mikebom-cli/src/scan_fs/` returns empty. Assert `git diff main -- mikebom-cli/src/generate/spdx/document.rs | grep -E "^\+" | grep -v "pub(crate)"` returns only the `+++ b/` header line (i.e., the ONLY change to `document.rs` is the `pub(crate)` visibility modifier — the string VALUE unchanged, no other edits per FR-018). This is the mechanical FR-012 + FR-018 enforcement check.
- [X] T019 [P] Run quickstart.md Scenario 2 (SC-002 byte-identity regression): `cargo +stable test --workspace` MUST show every SPDX 3 golden test passing unchanged. Any failure → SC-002 regression (sweep produced a non-empty Vec for a scan that should have had none — likely a bug in T007's regex application to non-matching element types).
- [X] T020 [P] Run quickstart.md Scenario 3 (SC-003 `spdx3-validate` continues to pass): `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 cargo +stable test --workspace --test spdx3_conformance` MUST show all tests passing including `every_existing_golden_passes_validator`.
- [X] T021 Run `./scripts/pre-pr.sh` per SC-005. Confirm clippy clean + all new tests pass + only the documented `sbomqs_parity` env-only flake failed.
- [X] T022 Run quickstart.md Scenario 8 (SC-008 CHANGELOG entry present): `sed -n '/^## \[Unreleased\]/,/^## \[v/p' CHANGELOG.md | grep -A2 "simplelicensing_CustomLicense\|SPDX 3.*symmetry\|closes #487"` MUST return the T016 entry.
- [X] T023 Draft the PR description at `specs/154-spdx3-custom-licenses/pr-description.md` with sections: Summary, Closes #487, Changes (the 3-file Rust diff + CHANGELOG), Verification (SC-001 through SC-008 results — SC-001 is manual operator-cadence per quickstart.md Scenario 1; SC-002/SC-003/SC-004/SC-005/SC-006/SC-007/SC-008 are automated), the cross-format symmetry guarantee (SPDX 2.3 + SPDX 3 outputs now define the same LicenseRef set with byte-identical placeholders), Constitution check citing plan.md POST-DESIGN, Reviewer-cadence operator instructions with the 4 jq assertions from quickstart Scenario 1.

**Final checkpoint**: Milestone 154 is shippable. Mark all tasks in this file complete in the PR.

---

## Dependencies & Execution Order

### Phase dependencies

```text
Phase 1 (Setup)
  └─> Phase 2 (Foundational)
        └─> Phase 3 (US1) ─┐
                           ├─> Phase 5 (Polish)
            Phase 4 (US2) ─┘   (US1 and US2 can run in parallel after Phase 2)
```

### Within-phase parallelism

- **Phase 1**: T001 sequential; T002 + T003 + T004 [P] in parallel after T001.
- **Phase 2**: T005 + T006 [P] parallel (touch different files).
- **Phase 3 (US1)**: T007 → T008 sequential (T008 is a no-op verification if T007's import was added inline); T009 sequential after T007 (wiring depends on the helper existing); T010–T014 [P] in parallel after T009.
- **Phase 4 (US2)**: T015 [P] parallel with any Phase 3 test task.
- **Phase 5 (Polish)**: T016 + T017 + T018 + T019 + T020 [P] parallel; T021 sequential after; T022 sequential after T016; T023 sequential at end.

### Cross-US independence

US1 (the sweep + cross-format symmetry) is the MVP. US2 (byte-identity safeguards) is a small additive regression guard. Both touch DIFFERENT test cases in the SAME file (`v3_licenses.rs`) but no semantic conflict. Reviewers can verify each US independently — US1's 5 tests exercise the sweep behavior; US2's 1 test exercises the empty-input invariant.

## Implementation strategy

### MVP scope

The MVP is **US1 alone** (the cross-format symmetry fix). Shipping US1 closes issue #487. US2 (byte-identity guards) is the regression-safety layer. Both ship together as ~90 LOC + ~6 tests + 1 CHANGELOG — smallest single-sitting change since milestone 478.

### Per-task time estimate

- Phase 1 (T001–T004): ~5 min (baseline + line-number confirmation)
- Phase 2 (T005–T006): ~10 min (const visibility change + regex helper duplication)
- Phase 3 (US1, T007–T014): ~60 min (sweep helper + wiring + 5 unit tests)
- Phase 4 (US2, T015): ~5 min (1 unit test)
- Phase 5 (Polish, T016–T023): ~30 min (CHANGELOG + 5 automated audits + pre-PR + PR description)

**Total**: ~2 hours single-sitting. Smallest milestone since the milestone-478 hot-fix.
