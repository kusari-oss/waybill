---
description: "Task list for milestone 077 — operator-overridable root component name and version"
---

# Tasks: Operator-overridable root component name and version

**Input**: Design documents from `/specs/077-root-component-override/`
**Prerequisites**: plan.md, spec.md (with /speckit.clarify integration), research.md, data-model.md, contracts/root-component-override.md, quickstart.md

**Tests**: Spec references SC-001 through SC-010 plus the test matrix in contracts/root-component-override.md. Test tasks are included.

**Organization**: Three user stories. US1 (P1) is the headline source-tier override; US2 + US3 (P2) are tier coverage + manifest-drop verification. Phases group by what blocks what; all per-format wiring lives in foundational because all three stories share it.

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: parallelizable (different files, no incomplete-task dependencies)
- **[Story]**: US1 / US2 / US3 (user-story phase tasks only)
- File paths are absolute or repository-relative

## Path Conventions

Single workspace; all 077 changes inside `mikebom-cli` plus `docs/reference/identifiers.md`. No new modules; no new crates.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Pre-flight reconnaissance before touching code.

- [X] T001 Audit four explicit deliverables and capture each in this PR's commit message or a checked-in scratchpad. Other phases depend on the named outputs. (a) **All `ScanArtifacts` construction call sites**: enumerate every `ScanArtifacts { ... }` struct-literal construction (including in tests) so the new `root_override` field can be added with `..Default::default()` syntax where possible, and explicit `root_override: RootComponentOverride::default()` where necessary. Likely sites identified at planning: `cli/scan_cmd.rs`, `cli/run.rs`, `cli/generate.rs` (back-compat field update only — NO new flags on `GenerateArgs` per research §6), `generate/spdx/{document.rs,relationships.rs,mod.rs}`, `generate/openvex/mod.rs`, plus per-format module test fixtures. (b) **Existing per-format root-component construction lines**: confirm `cyclonedx/builder.rs:163-228` (build), `cyclonedx/metadata.rs:42` (build_metadata + the PURL-format string at metadata.rs:215), `spdx/document.rs:34-56` + `:456`, `spdx/v3_document.rs:150,367,376-390,435`. T007/T008/T009 each plug into one. (c) **Main-module detection property**: confirm the property name + value used to identify manifest-derived main-module components (per planning: `mikebom:component-role = main-module`); document the exact field path (`properties[].name` and `properties[].value`) for the filter logic. (d) **PURL emission encoding behavior baseline**: cat the existing PURL emission lines (e.g., `cyclonedx/metadata.rs:215`) to confirm whether they apply any encoding today (per planning: no — they're plain `format!()` calls). Establishes the baseline that T004's new `percent_encode_purl_name` must preserve for non-override cases (which means: non-override paths use unchanged emission; override paths route through the new helper).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Add the new types, helpers, flags, and per-format wiring that all three user stories depend on. After this phase the override flow is functionally complete; story phases add tests and verify edge cases.

**⚠️ CRITICAL**: All three user-story tracks depend on this phase.

- [X] T002 Add `pub struct RootComponentOverride { pub name: Option<String>, pub version: Option<String> }` with `#[derive(Debug, Clone, Default)]` and an `is_active(&self) -> bool` method to `mikebom-cli/src/generate/mod.rs`. Add new field `pub root_override: RootComponentOverride` to `ScanArtifacts`. Update existing `ScanArtifacts` constructions per T001(a) audit — use `..Default::default()` where possible; otherwise add explicit `root_override: RootComponentOverride::default()`. Compile-time check finds any missed sites. **Add 4 unit tests for `is_active()`**: default-constructed → false; name-only set → true; version-only set → true; both set → true.

- [X] T003 Add `fn validate_root_field(value: &str, flag_name: &str) -> Result<String, String>` to `mikebom-cli/src/cli/scan_cmd.rs` (or a small shared module if T005/T006 prefer to share). Behavior per research §4: reject empty, ASCII whitespace, control characters (`\x00`–`\x1F`, `\x7F`), `?`, and `#`. Verbose error messages identifying the offending character + position per the research §4 templates. Add 8 unit tests: valid simple name, valid npm-scoped name, valid version with dots, empty rejection, whitespace rejection, control-char rejection, `?` rejection, `#` rejection.

- [X] T004 [P] Add `fn percent_encode_purl_name(s: &str) -> String` to `mikebom-cli/src/generate/cyclonedx/metadata.rs` (or shared helper module, depending on T001(b) findings — both `metadata.rs:215` and `spdx/v3_document.rs:376` need access). Implementation per research §1: RFC 3986 §2.3 unreserved-character preservation (`[A-Za-z0-9._~-]`); UTF-8-aware percent-encoding for everything else. Add 5 unit tests: ASCII unreserved (passthrough), ASCII reserved (`@`, `/`, etc. encoded), UTF-8 multi-byte (e.g., emoji), empty string (returns empty), all-encoded (e.g., `?#@/` → all four encoded).

- [X] T005 Add `#[arg(long = "root-name", value_name = "NAME", value_parser = ...)]` and `#[arg(long = "root-version", value_name = "VERSION", value_parser = ...)]` flags to `ScanArgs` in `mikebom-cli/src/cli/scan_cmd.rs`. Both `Option<String>`. Both use `validate_root_field` from T003 as their value_parser. At the call site that constructs `ScanArtifacts`, populate `root_override` with `RootComponentOverride { name: args.root_name.clone(), version: args.root_version.clone() }`. Help text per contracts/root-component-override.md "Help-text shape".

- [X] T006 [P] Add the same two flags to `RunArgs` in `mikebom-cli/src/cli/run.rs`. Mirror T005's plumbing. Different file from T005, so [P].

- [X] T007 Wire override into the CDX builder. In `mikebom-cli/src/generate/cyclonedx/builder.rs`, modify the `build()` method (around line 163-228) to: (a) compute effective `target_name` and `target_version` from `self.root_override` if active, else fall through to the existing derivation; (b) when `root_override.is_active()`, filter the `components` slice BEFORE calling `build_components`, dropping entries where `properties[].name == "mikebom:component-role"` and `properties[].value == "main-module"` (per T001(c) confirmation); (c) emit a `tracing::info!` per the contract's logging table when override fires + a separate `tracing::info!` per dropped main-module component. In `mikebom-cli/src/generate/cyclonedx/metadata.rs:42`, modify `build_metadata` to use `percent_encode_purl_name` for the PURL `name` segment construction at line 215 ONLY when override is active (otherwise unchanged for back-compat).

- [X] T008 [P] Wire override into the SPDX 2.3 builder. In `mikebom-cli/src/generate/spdx/document.rs` (around lines 34-56 + 456) and any per-package emission site that consumes `target_name`/`target_version`, mirror T007's logic: use override values when active; filter manifest-derived main-module entries from `packages[]`; use `percent_encode_purl_name` for the override-path PURL in `Package.externalRefs`. Different file from T007 + T009, so [P]. Adds same logging as T007.

- [X] T009 [P] Wire override into the SPDX 3 builder. In `mikebom-cli/src/generate/spdx/v3_document.rs` (around lines 150, 367, 376-390, 435), mirror T007's logic: use override values when active; filter manifest-derived main-module entries from `elements[]`; use `percent_encode_purl_name` for the override-path PURL at line 376; SPDXID hash derives from the override values when active so it stays deterministic. Different file from T007 + T008, so [P]. Adds same logging as T007.

**Checkpoint**: production code is override-aware across all three formats. Validation, helpers, and flag plumbing in place. Both per-format wiring and the manifest-drop logic implemented. All three user-story phases can now proceed in parallel.

---

## Phase 3: User Story 1 — Source-tier override on arbitrary directories (Priority: P1)

**Goal**: `mikebom sbom scan --path /opt/builds/abc123-snapshot --root-name widget-svc --root-version 1.2.3` produces an SBOM whose root component carries the operator-supplied name+version, with `bom-ref`/`purl`/`cpe` derived from those values automatically.

**Independent Test**: Run the headline command against an empty tempdir; assert the emitted CDX `metadata.component` has the expected name/version/derived fields per contracts/root-component-override.md "Both flags set" example.

### Tests for User Story 1

- [X] T010 [US1] Create new integration test file `mikebom-cli/tests/identifiers_root_component_override.rs` with: tempdir-based fixture builder helper (similar pattern to milestone-076's identifiers_subject_and_component.rs), plus tests:
  - (a) `root_name_and_version_override_on_arbitrary_dir` — both flags set, assert `metadata.component.{name,version,bom-ref,purl,cpe}` per contracts/root-component-override.md "Both flags set" expected output.
  - (b) `only_root_name_supplied_uses_default_version` — `--root-name widget-svc` alone; assert name overridden + version stays `0.0.0`.
  - (c) `only_root_version_supplied_uses_basename_name` — `--root-version 1.2.3` alone; assert version overridden + name stays basename.
  - (d) `no_flags_emits_basename_derived_name` — neither flag passed against an arbitrary tempdir (e.g., `/tmp/abc-xyz-123`); assert positive identities: `metadata.component.name == "abc-xyz-123"`, `version == "0.0.0"`, `bom-ref == "abc-xyz-123@0.0.0"`, `purl == "pkg:generic/abc-xyz-123@0.0.0"`. Verifies the no-flag path produces the documented auto-derived defaults. The broader "byte-identical to alpha.17" guarantee from SC-002 is enforced transitively by the existing parity-check golden suite that T014 (pre-PR gate) runs.
  - (e) `validation_rejects_empty_name`, `validation_rejects_whitespace_name`, `validation_rejects_control_char_name`, `validation_rejects_question_mark_name`, `validation_rejects_hash_name` — each invokes `mikebom sbom scan --root-name <bad-value>`, asserts non-zero exit and clap-shaped error message containing the diagnostic text from T003's validator.
  - (f) `npm_scoped_name_url_encoded_in_purl` — `--root-name "@acme/widget-svc"` succeeds, emitted PURL is `pkg:generic/%40acme%2Fwidget-svc@<version>`, emitted `metadata.component.name` is the verbatim `@acme/widget-svc`.
  - (g) `override_emits_in_all_three_formats` — same scan emits CDX + SPDX 2.3 + SPDX 3; assert all three carry the override values in their respective root-element fields. Verifies SC-005.
  - (h) `override_deterministic_across_reruns` — invoke the same scan twice with identical flags; assert byte-equality of all three format outputs across the two runs. Verifies SC-009.

  Tests guarded with `#[cfg_attr(test, allow(clippy::unwrap_used))]` per CLAUDE.md.

**Checkpoint**: US1 passes. Source-tier override works end-to-end with all derived fields, validation, npm-scope encoding, multi-format emission, and determinism verified.

---

## Phase 4: User Story 2 — Override on image-tier and build-tier scans (Priority: P2)

**Goal**: The same `--root-name`/`--root-version` flags work on `mikebom sbom scan --image` and `mikebom trace run`. Auto-detected `image:` / `repo:` / `git:` / `subject:` identifiers are unaffected — they're separate slots in `externalReferences[]`.

**Independent Test**: Run `mikebom sbom scan --image alpine:3.19 --root-name widget-svc-base --root-version 1.0.0`; assert root component reflects the override AND the auto-detected `image:` identifier is still present.

### Tests for User Story 2

- [X] T011 [US2] Add to `mikebom-cli/tests/identifiers_root_component_override.rs`:
  - (a) `override_on_image_tier_scan` — `mikebom sbom scan --image <fixture-tar> --root-name widget-svc-image --root-version 1.2.3`; assert root component overridden AND auto-detected `image:` identifier (milestone 073) still present in `externalReferences[]`. Verifies US2 §1.
  - (b) `override_on_trace_run_build_tier` — construct a synthetic `ScanArtifacts` with `root_override` populated AND `generation_context = GenerationContext::BuildTimeTrace`, plus a small fixture component set including a build-tier `repo:`/`git:`/`subject:` identifier slot (mirroring milestone-074's pattern of testing emission logic with synthetic inputs since `mikebom trace run` requires Linux+eBPF which standard CI lanes don't have). Pass to the per-format builders (CDX `build()`, SPDX 2.3 `document.rs` builder, SPDX 3 `v3_document.rs` builder) and assert: (i) the emitted root component reflects the override values per FR-001/FR-002/FR-004; (ii) auto-detected document-level identifiers from milestones 073/074/076 in `externalReferences[]` are unaffected (separate slot — orthogonal). Verifies US2 §2 + FR-011.

**Checkpoint**: US2 passes. Override flows through image-tier and build-tier paths without disturbing identifier auto-detection from earlier milestones.

---

## Phase 5: User Story 3 — Override + manifest-driven main-module precedence (Priority: P2)

**Goal**: When the override is set on a manifest-driven scan (Cargo, npm, pip, gem, Maven, Go), the manifest-derived main-module component is dropped entirely from the emitted SBOM (clean replacement per Q2 clarification). When the override is NOT set, the manifest's main-module is preserved unchanged (no regression).

**Independent Test**: Scan an existing Cargo fixture (with `[package].name = "<known-name>"`) once with `--root-name override-name` and once without. Assert the override-set run drops the cargo PURL from `components[]`; the no-flag run preserves it.

### Tests for User Story 3

- [X] T012 [US3] Add to `mikebom-cli/tests/identifiers_root_component_override.rs`:
  - (a) `override_drops_manifest_main_module_cargo` — run scan with override against an existing Cargo test fixture (use one of the project's existing cargo fixtures so we don't have to construct a new one); assert `metadata.component.name == "widget-svc"` AND `components[]` has zero entries with PURL matching the cargo `[package].name@version`. Plus assert one `tracing::info!` was emitted naming the dropped PURL (or fall back to runtime side-effect verification if log capture is awkward — note the limitation in a code comment, same pattern as milestone 075's T011c). Verifies SC-006.
  - (b) `no_override_preserves_manifest_main_module` — same Cargo fixture without flags; assert `components[]` still contains the cargo main-module entry. Verifies FR-009 / SC-002 (no regression for the no-flag case).
  - (c) `override_orthogonal_to_other_identifier_flags` — run scan with `--root-name widget-svc --root-version 1.2.3 --repo git@github.com:acme/widget-svc.git --subject-hash sha256:abc1234567890abcdef1234567890abcdef1234567890abcdef1234567890ab --component-id "pkg:cargo/serde@1.0.0=kusari-id:asset-foo"`; assert ALL of the following coexist independently in the emitted SBOM: (i) root component reflects override; (ii) auto-detected `repo:` identifier from milestone 073 present in `externalReferences[]`; (iii) manual `subject:` identifier from milestone 076 present in `externalReferences[]`; (iv) per-component `kusari-id:asset-foo` identifier from milestone 076 present in the matching component's `properties[]` (if a `pkg:cargo/serde@1.0.0` component exists in the fixture). Verifies FR-011 across multiple identifier surfaces (072/073/074/076).

**Checkpoint**: US3 passes. Manifest-drop clean-replacement behavior verified on a real Cargo fixture; no-flag preservation verified; orthogonality with milestone-076 identifier flags verified.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [X] T013 [P] Update `docs/reference/identifiers.md`: add a new section "Root component override (`--root-name` / `--root-version`)" covering the two flags, the validation rule (whitespace/control/`?`/`#` rejected), the manifest-drop clean-replacement behavior + reference to GitHub issue #151 for the demote-to-library follow-up, and the per-format wire-mapping table from contracts/root-component-override.md. Add the operator recipes from quickstart.md as inline examples (Recipes 1, 3, 4 — the headline source-tier, the Cargo manifest-drop, the image-tier).

- [X] T014 Run pre-PR gate per CLAUDE.md: (a) `cargo +stable clippy --workspace --all-targets -- -D warnings` zero warnings; (b) `cargo +stable test --workspace` every target reports `ok. N passed; 0 failed`. Convenience: `./scripts/pre-pr.sh`. The pre-PR gate also transitively verifies SC-005 / SC-002 / SC-010 (existing milestone-073/074/075/076 byte-identity goldens stay byte-identical) since the parity-check golden suite is part of the workspace test set.

- [X] T015 Manually validate quickstart.md recipes 1, 2, 3, 4, 5 + the npm-scoped name + validation-error recipes end-to-end against a real local build of milestone 077. Confirm log-line phrasing matches contracts/root-component-override.md exactly. Confirm jq snippets in the recipes work against actual emitted SBOMs. Time the override flow with `time` to confirm <1ms additional overhead per scan (informal smoke check on the SC mentioned in plan.md performance goals).

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: T001 has no dependencies; survey task.
- **Phase 2 (Foundational)**: T002 → T005 sequential within scan_cmd.rs (T005 uses T003's validator + T002's struct). T003 must precede T005 + T006. T004 [P] independent of T002/T003 (different file). T006 [P] (different file from T005). T007 needs T002 + T004; T008 [P] / T009 [P] each need T002 + T004 (different files from T007 and from each other). Order: T002 → (T003 || T004 [P]) → (T005 || T006 [P]) → (T007 || T008 [P] || T009 [P]).
- **Phase 3 (US1)**: T010 depends on Phase 2 complete (uses the new flags + the new validator + the per-format wiring).
- **Phase 4 (US2)**: T011 depends on Phase 2 complete; can run in parallel with Phase 3 (different test functions in shared file — sequential within file but the implementation tasks are done).
- **Phase 5 (US3)**: T012 depends on Phase 2 complete; can run in parallel with Phases 3 + 4 (different test functions).
- **Phase 6 (Polish)**: T013 [P] (docs) parallel with T014 (gate); T014 depends on Phases 1-5 complete; T015 depends on T014 (need a clean build to smoke-test).

### Parallel Opportunities

- T004 [P] (helper) + T003 (validator) — different files in Phase 2.
- T006 [P] (RunArgs flags) + T005 (ScanArgs flags) — different files in Phase 2.
- T007 + T008 [P] + T009 [P] — three different per-format builder files in Phase 2.
- T011 / T012 / T013 — independent test functions / docs editing across Phases 4 / 5 / 6.

### Within Each User Story

- All three stories share the Phase 2 wiring; they differ only in test scope (US1 = source-tier coverage, US2 = image/build-tier, US3 = manifest-drop).

---

## Parallel Example: Phase 2 (Foundational)

```bash
# Sequential start (T002 must land first):
Task: "T002 RootComponentOverride struct + ScanArtifacts field"

# Then parallel pair (different files):
Task: "T003 validate_root_field in scan_cmd.rs"
Task: "T004 [P] percent_encode_purl_name in metadata.rs"

# Then parallel pair (different files):
Task: "T005 ScanArgs flags in scan_cmd.rs"
Task: "T006 [P] RunArgs flags in run.rs"

# Then three-way parallel (three different per-format builder files):
Task: "T007 CDX builder wiring in cyclonedx/builder.rs + metadata.rs"
Task: "T008 [P] SPDX 2.3 builder wiring in spdx/document.rs"
Task: "T009 [P] SPDX 3 builder wiring in spdx/v3_document.rs"
```

---

## Implementation Strategy

### MVP First (Phases 1-3 = US1)

1. Phase 1 setup (T001 audit).
2. Phase 2 foundational (T002–T009): types, validator, helpers, flag plumbing, all three per-format builders wired.
3. Phase 3 US1 (T010): integration tests covering source-tier override, validation, npm-scoped names, multi-format emission, determinism.
4. **STOP and VALIDATE**: at this checkpoint the headline use case (the one the user surfaced in their original message) works end-to-end. US2 + US3 verify additional tier coverage and the manifest-drop edge case but don't add new user-visible capability beyond US1.
5. Continue to Phases 4-6 to complete the milestone.

### Incremental Delivery

Single PR. Milestone is small (<2 days estimated). Splitting US1 from US2/US3 would create transient state where the source-tier path ships but the manifest-drop verification doesn't — risk of someone hitting the manifest case in alpha.17 and being surprised. Recommend single PR.

### Parallel Team Strategy

Single developer + reviewer. Three-way parallelism on T007/T008/T009 is real if you have three developers but overkill for one-person shipping. Sequential is fine.

---

## Notes

- [P] = different files, no incomplete-task dependencies.
- All three user stories share the Phase 2 wiring. Test surface splits by user story.
- Per CLAUDE.md: pre-PR gate REQUIRES both `cargo +stable clippy --workspace --all-targets -- -D warnings` clean AND `cargo +stable test --workspace` clean. Cite both in the PR description.
- Tests in `identifiers_root_component_override.rs` MUST guard their `mod tests` items with `#[cfg_attr(test, allow(clippy::unwrap_used))]` per CLAUDE.md.
- Total estimated tasks: 15. Total estimated effort: <2 person-days.
