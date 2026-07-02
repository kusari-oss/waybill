---
description: "Task list for milestone 153 — SPDX 2.3 §10.1 conformance for LicenseRef-* (closes issue #485)"
---

# Tasks: SPDX 2.3 §10.1 conformance — milestone 153

**Input**: Design documents from `/specs/153-spdx-license-refs-conformance/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/sweep-api.md ✓, quickstart.md ✓

**Tests**: Tests are part of the implementation per the milestone-152 + milestone-478 convention (inline `#[cfg(test)] mod tests` in the affected Rust file). Spec SC-006 enumerates ≥6 new unit tests; research.md §R9 lists the canonical 10. No separate test-only phase.

**Organization**: Tasks are grouped by user story. The primary deliverable is one Rust file (`mikebom-cli/src/generate/spdx/document.rs`) plus a conditional second file (`mikebom-cli/src/generate/spdx/v3_licenses.rs`, gated on the SPDX 3 investigation outcome from US3). `[P]` markers indicate "no semantic dependency" rather than physical-parallel file edits.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: No semantic dependency on other tasks in the same phase
- **[Story]**: Which user story this task belongs to (US1, US2, US3)
- File paths are exact; the primary deliverable is `mikebom-cli/src/generate/spdx/document.rs` unless noted

## Path Conventions

- **Primary Rust deliverable**: `mikebom-cli/src/generate/spdx/document.rs`
- **Conditional Rust deliverable**: `mikebom-cli/src/generate/spdx/v3_licenses.rs` (only if US3 investigation concludes SPDX 3 needs equivalent work)
- **CHANGELOG**: `CHANGELOG.md`
- **Authoring artifacts** (not shipped publicly): `specs/153-spdx-license-refs-conformance/*.md`
- **Untouched references** (READ-ONLY): `docs/reference/sbom-format-mapping.md`, `mikebom-common/`, `mikebom-ebpf/`, `mikebom-cli/src/generate/cyclonedx/`, `mikebom-cli/src/scan_fs/`

---

## Phase 1: Setup

**Purpose**: Verify baseline + confirm integration site is unchanged since planning.

- [X] T001 Verify pre-PR baseline on `main` by running `./scripts/pre-pr.sh` from the repo root; confirm the documented `sbomqs_parity::sbomqs_spdx_score_meets_or_beats_cdx_across_ecosystems` env-only failure is the ONLY pre-existing failure permitted in SC-005. (May be skipped if the last recent pre-PR run — e.g., milestone 152 close-out — already established the baseline; T024 is the definitive gate for THIS milestone's work.)
- [X] T002 [P] Open `mikebom-cli/src/generate/spdx/document.rs` and confirm the exact line numbers: (a) `SpdxExtractedLicensingInfo` struct definition (planning-time: lines 204-211); (b) `has_extracted_licensing_infos` field in the document envelope (line 187); (c) the `build_packages` call site at ~line 352-353; (d) the `#[cfg(test)] mod tests` block (if present) or the file's tail for adding a new test module. Record actual line numbers for use in later tasks. **Additionally** (per analysis remediation A3): grep `mikebom-cli/src/generate/spdx/packages.rs` for the `SpdxPackage` struct definition and record the exact Rust field names for the three license-carrying fields (planning-time assumption: `license_declared` / `license_concluded` / `license_info_from_files`; verify actual names before T006 authoring so the sweep helper compiles cleanly on the first pass).
- [X] T003 [P] Confirm `spdx = "0.10"` and `regex = "1"` are both direct deps of `mikebom-cli/Cargo.toml` (added in milestones 013 + 152). Confirm `std::sync::OnceLock` availability at the workspace's Rust MSRV (stable, present since 1.70).

---

## Phase 2: Foundational

**Purpose**: Add the module-level constants + regex helper that every US1 test + implementation task depends on.

**⚠️ CRITICAL**: T004 + T005 must complete before US1's main helper (T007) is added, because both the helper AND its unit tests reference `PLACEHOLDER_EXTRACTED_TEXT` + `license_ref_regex`.

- [X] T004 Add the `PLACEHOLDER_EXTRACTED_TEXT: &str` module-level const to `mikebom-cli/src/generate/spdx/document.rs` immediately after the `SpdxExtractedLicensingInfo` struct definition (line ~211). Value MUST be the byte-exact string documented in spec FR-004 (per Clarifications Q1 wire contract): `"License text not extracted by mikebom. Consult the original package (e.g., /usr/share/licenses/<name>/ on Debian/RPM, or upstream project source) for the full text."` — implemented via a multi-line raw string literal to keep source-code readability without changing the emitted value. Include the doc-comment from data-model.md §2 explaining the `<name>` literal-token convention + the consumer jq recipe for pattern-matching on the prefix.
- [X] T005 Add the `license_ref_regex()` helper + `LICENSE_REF_REGEX: OnceLock<Regex>` static to `mikebom-cli/src/generate/spdx/document.rs` immediately after `PLACEHOLDER_EXTRACTED_TEXT`. Pattern: `(?:^|[^:])(LicenseRef-[a-zA-Z0-9.-]+)` per data-model.md §3. Include the doc-comment explaining the non-capturing DocumentRef-prefix filter + the capture-group-1 semantics.

**Checkpoint**: Phase 2 complete — foundation for US1 in place.

---

## Phase 3: User Story 1 — SPDX 2.3 consumer sees §10.1-conformant output (Priority: P1) 🎯 MVP

**Goal**: After this US ships, every emitted mikebom SPDX 2.3 document that contains any `LicenseRef-*` inline in a package's license field has a matching top-level `hasExtractedLicensingInfos[]` entry per §10.1. Closes the issue-#485 conformance gap.

**Independent Test**: Per quickstart.md Scenario 1 — manual operator-cadence verification against the Yocto testbed. Plus 8 inline unit tests covering single/compound/dedup/cross-field cases per research.md §R9.

### Implementation for User Story 1

- [X] T006 [US1] Add the `sweep_extracted_license_refs(packages: &[SpdxPackage], existing: Vec<SpdxExtractedLicensingInfo>) -> Vec<SpdxExtractedLicensingInfo>` helper function to `mikebom-cli/src/generate/spdx/document.rs` immediately after `license_ref_regex()`. Implementation per data-model.md §1 + contracts/sweep-api.md Contracts 1-8: seed `BTreeMap<String, SpdxExtractedLicensingInfo>` from `existing` (existing entries win); iterate `packages`, for each iterate the 3 license-carrying fields (`license_declared` / `license_concluded` / `license_info_from_files`), run `license_ref_regex().captures_iter(field_value)`, extract capture-group-1 matches, insert into map ONLY if key absent, sanitize `name` via `.strip_prefix("LicenseRef-").unwrap_or(match)` fallback; finally `map.into_values().collect()` + sort by `license_id` for determinism. Include comprehensive doc-comment naming the SPDX 2.3 §10.1 spec citation + the milestone-012 coexistence rule.
- [X] T007 [US1] Wire the sweep at the integration site in `mikebom-cli/src/generate/spdx/document.rs:352-353` (post-`build_packages`). Change from `let (packages, has_extracted_licensing_infos) = super::packages::build_packages(...);` to the version in data-model.md §4 — add the `let has_extracted_licensing_infos = sweep_extracted_license_refs(&packages, has_extracted_licensing_infos);` reassignment immediately after the tuple destructure. Include a 3-line comment naming the milestone-153 + issue #485.
- [X] T008 [P] [US1] Add unit test `sweep_single_package_single_licenseref` to the `document.rs` test module — assert a single `SpdxPackage` with `license_declared = "LicenseRef-PD"` produces exactly 1 entry with `licenseId = "LicenseRef-PD"`, `name = "PD"`, `extractedText = PLACEHOLDER_EXTRACTED_TEXT`. Covers US1 A2 (liblzma5 case).
- [X] T009 [P] [US1] Add unit test `sweep_single_package_compound_licenseref` — assert input `licenseDeclared = "GPL-2.0-only AND LicenseRef-bzip2-1.0.4"` produces 1 entry for `LicenseRef-bzip2-1.0.4` (only the LicenseRef- portion; the GPL-2.0-only substring is NOT extracted). Covers US1 A1 (busybox case).
- [X] T010 [P] [US1] Add unit test `sweep_dedup_across_multiple_packages` — construct 4 synthetic `SpdxPackage`s all with `licenseDeclared = "GPL-2.0-only AND LicenseRef-bzip2-1.0.4"` (busybox-family scenario); assert the returned Vec contains exactly 1 entry for `LicenseRef-bzip2-1.0.4`. Covers US1 A3 (dedup).
- [X] T011 [P] [US1] Add unit test `sweep_covers_licenseConcluded_field` — assert a package with LicenseRef ONLY in `license_concluded` (not `license_declared`) still gets a matching entry. Covers US1 A5 (cross-field, concluded).
- [X] T012 [P] [US1] Add unit test `sweep_covers_licenseInfoFromFiles_field` — assert a package with LicenseRef in `license_info_from_files` gets a matching entry. Covers US1 A5 (cross-field, files).
- [X] T013 [P] [US1] Add unit test `sweep_dedup_with_milestone_012_entry` — pass an `existing` Vec containing `SpdxExtractedLicensingInfo { license_id: "LicenseRef-x", extracted_text: "REAL EXTRACTED TEXT", name: "mikebom-extracted-license" }` and a package referencing `LicenseRef-x`; assert the returned Vec has 1 entry with `extracted_text = "REAL EXTRACTED TEXT"` (milestone-012 wins over placeholder per FR-005). Covers US1 A6.
- [X] T014 [P] [US1] Add unit test `sweep_ignores_document_ref_prefixed` — assert a package with `license_declared = "MIT AND DocumentRef-external:LicenseRef-foo"` produces ZERO entries (regex non-capturing prefix filters DocumentRef-prefixed compound). Covers Edge Case per spec.
- [X] T015 [P] [US1] Add unit test `sweep_covers_nested_compound_structure` — assert input `licenseDeclared = "MIT AND LicenseRef-foo OR (LicenseRef-bar AND Apache-2.0)"` produces exactly 2 entries (`LicenseRef-foo`, `LicenseRef-bar`) regardless of operator/paren surroundings. Covers Edge Case per spec.

**Checkpoint**: §3 produces the SC-001 fix + comprehensive unit coverage. The 3 issue-#485 reference LicenseRefs are provably handled by tests T008–T010; edge cases by T014 + T015.

---

## Phase 4: User Story 2 — Byte-identical happy path when no LicenseRef is present (Priority: P2)

**Goal**: After this US ships, mikebom is provably safe — the sweep is a strict no-op on scans that don't need it. SC-002 verified via 2 additional unit tests + the existing milestone-090 golden test infrastructure.

**Independent Test**: Unit tests + `cargo test --workspace` running the existing SPDX 2.3 golden tests against milestone-090 fixtures (cargo/npm/go/pip). No fixture regeneration.

### Implementation for User Story 2

- [X] T016 [P] [US2] Add unit test `sweep_no_licenserefs_returns_empty_vec` — construct synthetic `SpdxPackage`s where every license field is `"MIT AND Apache-2.0"` (canonical SPDX ids only); pass `existing = vec![]`; assert the returned Vec is empty (`.is_empty()`). This validates the empty-in-empty-out invariant per FR-006 + Contract 7. Since the document envelope's `#[serde(skip_serializing_if = "Vec::is_empty")]` handles the JSON omission, an empty Vec here guarantees byte-identity for happy-path scans.
- [X] T017 [P] [US2] Add unit test `placeholder_text_matches_wire_contract` — assert `PLACEHOLDER_EXTRACTED_TEXT` equals the exact byte-string documented in spec FR-004 + Clarifications Q1. This locks the wire contract at compile time; any future accidental edit to the const triggers this test.

**Checkpoint**: §4 produces the SC-002 + SC-003 safeguards. Happy-path byte-identity is now compile-time-locked (test #17 catches placeholder-drift; test #16 catches sweep-fires-on-empty regression).

---

## Phase 5: User Story 3 — SPDX 3 sanity-check (Priority: P3)

**Goal**: Determine whether SPDX 3.0.1 requires equivalent `licensing_CustomLicense` graph elements per unique LicenseRef; either apply the fix (FR-009 Option A) or document that SPDX 3 doesn't need it (FR-009 Option B). Empirical determination via `spdx3-validate==0.0.5`.

**Independent Test**: Per quickstart.md Scenario 4 — run `spdx3-validate` against a mikebom-emitted SPDX 3 for the issue-#485 testbed; document the outcome in the PR description.

### Implementation for User Story 3

- [X] T018 [US3] Emit a small synthetic SPDX 3.0.1 document from the current mikebom (without the T019+ changes) that contains at least one package with an inline `LicenseRef-*` in its `simplelicensing_licenseExpression` value. Route: (a) if an existing test fixture triggers milestone-152's LicenseRef injection, reuse it; (b) else synthesize a minimal 1-package SPDX 3 doc inline in a test function. Save to a temp path.
- [X] T019 [US3] Run `.venv/spdx3-validate/bin/spdx3-validate <the-emitted-doc>` against the T018 output. Capture the validator's stdout + exit code. Two outcomes to distinguish:
  - **Outcome A** — validator reports "undefined LicenseRef" or "unresolved license reference" or exits non-zero due to license-reference errors → proceed to T020 (implement `sweep_custom_licenses` in `v3_licenses.rs`).
  - **Outcome B** — validator reports no LicenseRef-related errors (exit 0 or unrelated warnings only) → skip T020; proceed to T021 to document the finding.
- [X] T020 [US3, CONDITIONAL] IF T019 == Outcome A: implement `sweep_custom_licenses` in `mikebom-cli/src/generate/spdx/v3_licenses.rs` per data-model.md §5 + contracts/sweep-api.md Contract 10. **STATUS: SKIPPED per T019 Outcome B** — `spdx3-validate==0.0.5` returned exit 0 (schema + SHACL both pass) on a synthetic SPDX 3.0.1 document with inline `LicenseRef-bzip2-1.0.4` in `simplelicensing_licenseExpression` WITHOUT any `licensing_CustomLicense` element. SPDX 3.0.1's license-reference model does not require the equivalent emission; the SPDX 3 emitter is conformant as-is. No code change to `v3_licenses.rs`. See `pr-description.md` "SPDX 3 investigation output" section for full validator output.
- [X] T021 [US3] Re-run T019 with the T020 fix applied (if T020 fired) OR document the T019 Outcome B result. Record the final `spdx3-validate` output in `specs/153-spdx-license-refs-conformance/pr-description.md` under a "SPDX 3 investigation outcome" heading, citing the validator's output as evidence per FR-009 + SC-004.

**Checkpoint**: §5 closes the FR-008/FR-009 investigation with an empirically-grounded answer. The PR description states unambiguously which outcome fired and why.

---

## Phase 6: Polish & cross-cutting

**Purpose**: CHANGELOG entry + audit scenarios + pre-PR gate + PR description.

- [X] T022 [P] Add the milestone-153 CHANGELOG.md entry under `## [Unreleased]` per research.md §R8 + SC-008 — single subsection (`### <heading>`) documenting: (a) the SPDX 2.3 §10.1 conformance fix + issue #485 reference; (b) the byte-exact locked placeholder text (verbatim, in a fenced code block so consumers can grep-diff); (c) the milestone-012 hash-fallback coexistence rule (dedup, not replace); (d) the SPDX 3 investigation outcome from T021 (either "applied" with brief description or "not required" with `spdx3-validate` evidence). **Precheck**: run `head -20 CHANGELOG.md` first to confirm `## [Unreleased]` heading exists (per milestone-152's A3 remediation precedent).
- [X] T023 [P] Run quickstart.md Scenarios 5 + 6 (SC-006 test count + SC-005 pre-PR gate): (a) grep the new test function names in `document.rs` to confirm ≥6 milestone-153 tests present; (b) run `./scripts/pre-pr.sh` and confirm same-as-pre-153 baseline (only the documented `sbomqs_parity` env-only flake permitted).
- [X] T024 [P] Run quickstart.md Scenario 7 (SC-007 wire-format guard): assert `git diff main --name-only -- docs/ mikebom-common/ mikebom-ebpf/ mikebom-cli/src/generate/cyclonedx/` returns empty. Confirm the ONLY changed Rust files are `mikebom-cli/src/generate/spdx/document.rs` (always) + `mikebom-cli/src/generate/spdx/v3_licenses.rs` (only if T020 fired).
- [X] T025 [P] Run quickstart.md Scenario 2 (SC-002 byte-identity regression): `cargo +stable test --workspace` MUST show every milestone-090 golden test passing. Any SPDX 2.3 golden failure → SC-002 regression; investigate (likely cause: unintended non-empty Vec return from the sweep on a happy-path scan).
- [X] T026 Run quickstart.md Scenario 8 (SC-008 CHANGELOG presence): `sed -n '/^## \[Unreleased\]/,/^## \[v/p' CHANGELOG.md | grep -A1 "hasExtractedLicensingInfos\|§10\.1\|issue #485"` MUST return the T022 entry.
- [X] T027 Draft the PR description at `specs/153-spdx-license-refs-conformance/pr-description.md` with sections: Summary, Closes #485, Changes (the primary `document.rs` + optional `v3_licenses.rs` + CHANGELOG), Verification (SC-001 through SC-008 results — SC-001 + SC-003 + SC-004 are manual operator-cadence per quickstart.md Scenarios 1 + 3 + 4; SC-002 / SC-005 / SC-006 / SC-007 / SC-008 are automated). **SC-003 explicit coupling** (per analysis remediation A1): under a dedicated "SC-003 strict SPDX 2.3 validator" heading in the PR description, include the validator invocation (e.g., `docker run --rm -v /tmp/mikebom-m153:/data spdx/spdx-tools spdx-tools --validate /data/core-image-minimal.spdx.json`) + expected output ("no undefined LicenseRef- reference errors"). SC-003 is verified alongside SC-001 during the same Yocto testbed run — one testbed pass exercises both. Include the SPDX 3 investigation outcome from T021 with `spdx3-validate` output as evidence, Constitution check citing plan.md POST-DESIGN, Reviewer-cadence operator instructions.

**Final checkpoint**: Milestone 153 is shippable. Mark all tasks in this file complete in the PR.

---

## Dependencies & Execution Order

### Phase dependencies

```text
Phase 1 (Setup)
  └─> Phase 2 (Foundational)
        └─> Phase 3 (US1) ─┐
                           ├─> Phase 6 (Polish)
            Phase 4 (US2) ─┤
                           │
            Phase 5 (US3) ─┘   (US1/US2/US3 can run in parallel after Phase 2)
```

### Within-phase parallelism

- **Phase 1**: T001 sequential; T002 + T003 [P] in parallel after T001.
- **Phase 2**: T004 → T005 sequential (T005 references T004's file-region proximity but no code dependency; sequential authoring avoids merge conflicts on the same file).
- **Phase 3 (US1)**: T006 → T007 sequential (T007 wires T006's helper); T008–T015 [P] in parallel after T007.
- **Phase 4 (US2)**: T016 + T017 [P] parallel after Phase 2 completes.
- **Phase 5 (US3)**: T018 → T019 sequential; T020 conditional on T019 outcome; T021 sequential after T020 (or after T019 if T020 skipped).
- **Phase 6 (Polish)**: T022 + T023 + T024 + T025 + T026 [P] parallel; T027 sequential at end.

### Cross-US independence

US1 (the sweep fix) is the MVP. US2 (byte-identity safeguards) is a small additive regression guard. US3 (SPDX 3 investigation) is empirical work whose outcome may or may not require code — the T019 validator run decides. All three USs touch DIFFERENT concerns:
- US1 → `document.rs` core logic + tests #1-8
- US2 → `document.rs` tests #9-10
- US3 → `v3_licenses.rs` (conditionally) + PR description

Reviewers can verify each US independently. The MVP is US1 alone; US2 + US3 are cheap adds shipping in the same milestone for coherence.

## Implementation strategy

### MVP scope

The MVP is **US1 alone** (the SPDX 2.3 §10.1 fix). Shipping US1 closes issue #485. US2 (idempotency guards) is the regression-safety layer; US3 (SPDX 3) is empirical investigation. All three ship together as ~200 LOC + ~10 tests + 1 CHANGELOG — small enough for a single sitting.

### Per-task time estimate

- Phase 1 (T001–T003): ~5 min (baseline verification + line-number confirmation)
- Phase 2 (T004–T005): ~15 min (const + regex helper)
- Phase 3 (US1, T006–T015): ~90 min (sweep helper ~30 LOC + wiring 3 lines + 8 unit tests)
- Phase 4 (US2, T016–T017): ~10 min (2 unit tests)
- Phase 5 (US3, T018–T021): ~30 min (synthesize SPDX 3 doc + run validator + document; conditional T020 adds ~30 min if it fires)
- Phase 6 (Polish, T022–T027): ~30 min (CHANGELOG + audits + pre-PR + PR description)

**Total**: ~3 hours (Outcome B — no SPDX 3 fix), ~3.5 hours (Outcome A — SPDX 3 sibling helper implemented). Single-sitting feasible.
