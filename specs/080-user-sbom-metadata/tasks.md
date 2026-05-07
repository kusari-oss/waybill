---
description: "Task list for milestone 080 — user-provided SBOM metadata"
---

# Tasks: User-provided SBOM metadata

**Input**: Design documents from `/specs/080-user-sbom-metadata/`
**Prerequisites**: plan.md, spec.md (with /speckit.clarify Q1 + Q2 integrated), research.md (with CDX 1.6 schema audit confirming native annotation support), data-model.md, contracts/user-sbom-metadata.md, quickstart.md

**Tests**: Spec references SC-001 through SC-011 plus the 17-test integration matrix in contracts/user-sbom-metadata.md. Test tasks are included.

**Organization**: Four user stories. US1 (P1) `--creator`; US2 (P1) `--metadata-comment` + `--annotator`/`--annotation-comment`; US3 (P2) `--scan-target-name`; US4 (P2) `--metadata-file` sidecar. All four ship in one PR per the spec assumptions section. The four stories share Phase 2's foundational module (`binding/user_metadata/`) + CLI parsing scaffold + CDX 1.6 schema fixture.

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: parallelizable (different files, no incomplete-task dependencies)
- **[Story]**: US1 / US2 / US3 / US4 (user-story phase tasks only)
- File paths are absolute or repository-relative

## Path Conventions

Single workspace. Bulk of milestone work is in one new module (`mikebom-cli/src/binding/user_metadata/` with 4 files), CLI flag definitions on two existing subcommands (`scan_cmd.rs`, `run.rs`), three existing format-emission code paths (`cyclonedx/metadata.rs`, `spdx/document.rs`, `spdx/v3_document.rs`), and one new integration-test file (`mikebom-cli/tests/sbom_user_metadata.rs`). Reuses milestone 078's `scripts/install-spdx3-validate.sh` + CI gate as-is. No new Cargo dependencies. No CI workflow updates.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Capture pre-implementation findings; vendor the CDX 1.6 schema fixture for the audit confirmation; verify the milestone-078 validator setup is intact.

- [ ] T001 Audit + fixture-vendor pass. (a) **CDX 1.6 schema vendor**: download `https://cyclonedx.org/schema/bom-1.6.schema.json` to `mikebom-cli/tests/fixtures/schemas/cyclonedx-1.6.json` (mirror the SPDX schema fixtures pattern from milestones 011 + 012 + 078). Confirm the audit findings from research §1: `bom.annotations[]` exists at the document level with required fields `subjects, annotator, timestamp, text`; `metadata.tools` supports the new `tools.components[]` shape; `metadata.authors[]` accepts `organizationalContact` objects; `metadata.manufacturer` is single-valued. (b) **Existing emission call sites**: re-confirm via `grep -n` (i) CDX `metadata.tools` emission in `mikebom-cli/src/generate/cyclonedx/metadata.rs` for the `--creator Tool:` landing, (ii) SPDX 2.3 `creationInfo.creators` emission in `mikebom-cli/src/generate/spdx/document.rs` line ~131, (iii) SPDX 3 `CreationInfo.createdBy` emission in `mikebom-cli/src/generate/spdx/v3_document.rs` line ~163 (extends milestone 078's wire shape). (c) **Validator binary still installed**: `.venv/spdx3-validate/bin/spdx3-validate --version` reports `0.0.5` (the milestone-078 pin); if not, run `bash scripts/install-spdx3-validate.sh`.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Build the new `user_metadata` module with all four data types, the CLI flag definitions on both subcommands, and the per-format emission wiring. After this phase, the production-side CLI surface is in place; story phases verify via tests.

**⚠️ CRITICAL**: All four user-story tracks depend on this phase.

- [ ] T002 Create `mikebom-cli/src/binding/user_metadata/mod.rs` with the public `UserMetadata` struct + `merge_file_and_flags` function signature per data-model.md. Register the new module in `mikebom-cli/src/binding/mod.rs` (or wherever `binding/identifiers/` is registered — mirror that pattern). Define the `BuildUserMetadataError` enum via `thiserror::Error` with variants for `ConflictError { field, file_value, flag_value }`, `ParseCreatorError`, `AnnotatorPairCountMismatch { annotator_count, comment_count }`, `MetadataFileIo { path, source }`, `MetadataFileParseError`. Empty function bodies (`todo!()`) acceptable — T003 fills them in.

- [ ] T003 Implement `mikebom-cli/src/binding/user_metadata/creator.rs`: `Creator { kind: CreatorKind, name: String }` newtype + `CreatorKind` enum `{Tool, Organization, Person}` + `parse_creator_str("Type: Name") -> Result<Creator, ParseCreatorError>`. Validation per VR-080-001 + VR-080-002: case-sensitive Type prefix, non-empty Name, no control characters, whitespace between `:` and Name trimmed. Add unit tests in a `#[cfg(test)] mod tests` block (`#[cfg_attr(test, allow(clippy::unwrap_used))]` per CLAUDE.md): valid inputs, invalid prefix (`Bot:`, `Service:`), empty Name, leading/trailing whitespace, control characters in Name.

- [ ] T004 Implement `mikebom-cli/src/binding/user_metadata/annotation.rs`: `Annotation { annotator: Creator, comment: String, timestamp: chrono::DateTime<chrono::Utc> }` + `validate_annotator_pairs(annotator: &[String], comment: &[String]) -> Result<(), AnnotatorPairCountMismatch>` per VR-080-003 (length-equality check). Add unit tests: equal-length pairs OK; mismatched lengths error with crisp diagnostic.

- [ ] T005 Implement `mikebom-cli/src/binding/user_metadata/metadata_file.rs`: `MetadataFile` + `MetadataFileAnnotator` structs both with `#[derive(Deserialize)]` + `#[serde(deny_unknown_fields)]` per VR-080-004 + research §4. Field names per research §4 (snake_case): `creators`, `annotators`, `metadata_comment`, `scan_target_name`. Add `pub fn load_metadata_file(path: &Path) -> Result<MetadataFile, BuildUserMetadataError>` that opens + reads + parses with `serde_json::from_reader`, returning `MetadataFileIo` or `MetadataFileParseError` on failures. Add unit tests: valid file loads correctly; unknown top-level field rejected; unknown field in `annotators[]` rejected; malformed JSON returns parse error with line+column.

- [ ] T006 Implement `mikebom-cli/src/binding/user_metadata/mod.rs::merge_file_and_flags` per data-model.md "Merge semantics" section. Concat order: file_creators + flag_creators (file first per research §6). For `metadata_comment` and `scan_target_name`, fail with `ConflictError` if BOTH file AND flag are `Some` (VR-080-005). The `emission_timestamp` parameter sets all annotations' `timestamp` field uniformly. Add unit tests: file-only inputs; flag-only inputs; both file and flags merge for arrays; conflict on single-valued fields fails; empty inputs return empty `UserMetadata::default()`.

- [ ] T007 Add the five new clap flags to `mikebom-cli/src/cli/scan_cmd.rs`'s `Scan` struct via derive: `creator: Vec<String>` (`ArgAction::Append`), `annotator: Vec<String>` (`ArgAction::Append`), `annotation_comment: Vec<String>` (`ArgAction::Append`), `metadata_comment: Option<String>`, `scan_target_name: Option<String>`, `metadata_file: Option<PathBuf>`. Add help text per the contract; for `--scan-target-name` include the precedence note from research §5 ("`--root-name` takes precedence on CDX `metadata.component.name` when both are passed"). At command-handler entry point, walk `std::env::args()` once and call `validate_annotator_pairs_strict_interleaving` (a helper that uses raw arg ordering for the early UX-friendly error per research §3) BEFORE dispatching to `merge_file_and_flags`.

- [ ] T008 Symmetric flag set on `mikebom-cli/src/cli/run.rs`'s `Run` struct (the `mikebom trace run` subcommand). Same six clap fields as T007. Same arg-vector walk for the strict-interleaving check. Both subcommand handlers delegate to a shared helper in `binding/user_metadata` for the parse + merge step (avoid copy-pasting between scan_cmd and run).

- [ ] T009 Wire `UserMetadata` into the CDX 1.6 emission path in `mikebom-cli/src/generate/cyclonedx/metadata.rs::build_metadata`. Per research §1 + §2: (a) iterate `user_metadata.creators` and dispatch by `CreatorKind` — `Tool` → append entry to `metadata.tools.components[]` with `name` + `type: "application"`; `Organization` (1st) → set `metadata.manufacturer = { name }`, (subsequent Organizations) → emit stderr warning + append to `bom.annotations[]` with `annotator.organization`; `Person` → append `{ name }` to `metadata.authors[]`. (b) For `user_metadata.metadata_comment`, append to `bom.annotations[]` with `annotator.organization.name = "mikebom contributors"`, `subjects = [<root-bom-ref>]`, `text = comment`, `timestamp = emission-time`. (c) For each `user_metadata.annotations[]` entry, append to `bom.annotations[]` with `annotator.<organization|individual|component>` set per `Creator.kind`, `subjects = [<root-bom-ref>]`, `text = comment`, `timestamp = emission-time`. (d) For `user_metadata.scan_target_name`, set `metadata.component.name` ONLY IF milestone 077's `--root-name` is NOT also set (`--root-name` wins per research §5; emit a stderr warning when both are set). Sort key for the `tools.components` and `bom.annotations` arrays MUST preserve insertion order (file-first-then-flags); no alphabetical sort.

- [ ] T010 Wire `UserMetadata` into the SPDX 2.3 emission path in `mikebom-cli/src/generate/spdx/document.rs`. Per research §2: (a) append each `user_metadata.creators` entry to `creationInfo.creators[]` formatted as `"<Type>: <Name>"` (use `CreatorKind::spdx_prefix()`); (b) set `creationInfo.comment = user_metadata.metadata_comment` if `Some`; (c) for each `user_metadata.annotations[]` entry, append to `annotations[]` with the SPDX 2.3 shape `{annotator: "<Type>: <Name>", annotationDate: emission-time, annotationType: "OTHER", comment}`; (d) set top-level `name = user_metadata.scan_target_name` if `Some` (this is the document-level name field; INDEPENDENT of `--root-name` which targets the root Package, per research §5 — both flags honored independently in SPDX 2.3).

- [ ] T011 Wire `UserMetadata` into the SPDX 3 emission path in `mikebom-cli/src/generate/spdx/v3_document.rs`. Per research §2 + data-model.md "SPDX 3 IRI scheme": (a) for each `user_metadata.creators` entry, add a new `Tool`/`Organization`/`Person` element to `@graph` with deterministic spdxId `<doc_iri>/<kind>/<slug>-<hash16>` where `<hash16>` is BASE32(SHA-256(`<kind>:<name>`))[..16] — produced by reusing the existing `hash_prefix(input, 16)` helper at `mikebom-cli/src/generate/spdx/v3_document.rs:664` for consistency with the milestone-078 IRI patterns (synthesized component IRIs at lines 553 + 621 also use 16 chars). Reference Tool elements from `CreationInfo.createdUsing[]`; reference Organization + Person elements from `CreationInfo.createdBy[]` (extends the milestone-078 `mikebom contributors` Organization pattern). (b) For `metadata_comment`, add a new `Annotation` element with `subject = <spdxDocument-iri>`, `annotationType = "other"`, `statement = comment`. (c) For each `user_metadata.annotations[]` entry, add a new `Annotation` element with the same shape; the annotator references the corresponding Agent element (added if not already present from `--creator`). (d) Set `software_Sbom.name = user_metadata.scan_target_name` if `Some` (independent of `--root-name` per research §5). Sort `@graph` per the existing v3_document.rs ordering convention (lines 30+); new elements slot into the appropriate sections (Tool/Org/Person before CreationInfo; Annotations after the SpdxDocument). Sort key extension: include the new elements in the existing dedup-correctness invariant.

---

## Phase 3: User Story 1 — `--creator` records automation provenance (Priority: P1)

**Goal**: post-fix emission of any scan with `--creator "Tool: my-pipeline"` (or repeatable form) lands the new entries in the format-appropriate native field of all three formats. Auto-populated mikebom entries preserved alongside.

**Independent Test**: emit a fresh SBOM with `--creator "Tool: my-pipeline"`; assert CDX `metadata.tools.components[]` contains the new entry; assert SPDX 2.3 `creationInfo.creators[]` contains `"Tool: my-pipeline"`; assert SPDX 3 `@graph` contains a new `Tool` element referenced from `CreationInfo.createdUsing[]`.

### Tests for User Story 1

- [ ] T012 [US1] Create `mikebom-cli/tests/sbom_user_metadata.rs` with module-level helpers (a `run_scan_with_flags` helper that invokes `mikebom sbom scan` against a synthetic tempdir with the flag set under test, then reads + parses each emitted format) and the first 3 tests: `creator_lands_in_all_three_formats` (US1 §1, single Tool creator → asserts on CDX `metadata.tools.components`, SPDX 2.3 `creationInfo.creators`, SPDX 3 `Tool` element); `multi_creator_appends_additively` (US1 §2, two `--creator` flags → both visible alongside auto-populated mikebom entry); `creator_type_routing_per_format` (US1 §3, one of each Tool/Organization/Person → routes correctly per research §2 table). All tests use `#[cfg_attr(test, allow(clippy::unwrap_used))]` per CLAUDE.md.

**Checkpoint**: US1 passes. Operators can replace `jq` post-processing for the dominant `--creator` use case.

---

## Phase 4: User Story 2 — `--metadata-comment` + `--annotator`/`--annotation-comment` record context (Priority: P1)

**Goal**: post-fix emission with `--metadata-comment` and/or paired `--annotator`/`--annotation-comment` flags lands the values at standards-native locations in all three formats. Multi-annotation positional pairing per Q1 clarification.

**Independent Test**: emit fresh SBOMs with `--metadata-comment "Release v1.0.0"` and `--annotator "Tool: T" --annotation-comment "C"`; assert each format's native field carries the expected value.

### Tests for User Story 2

- [ ] T013 [US2] Add to `mikebom-cli/tests/sbom_user_metadata.rs`: tests `metadata_comment_lands_in_all_three` (US2 §1-3, SC-002; SPDX 2.3 `creationInfo.comment`, SPDX 3 `Annotation` of type OTHER, CDX 1.6 `bom.annotations[]` with mikebom-contributors annotator); `annotator_pair_emits_annotation` (US2 §4, SC-003; assert each format's annotation slot carries the expected annotator + comment + emission-time timestamp); `multi_annotator_positional_pairing` (Q1 clarification; `--annotator A --annotation-comment X --annotator B --annotation-comment Y` produces two annotations); `annotator_without_comment_fails` (US2 §5; `--annotator A` alone OR `--annotator A --annotation-comment X --annotator B` fails parsing with the AnnotatorPairCountMismatch error message).

**Checkpoint**: US2 passes. Document-level context flags work alongside US1's creator flags.

---

## Phase 5: User Story 3 — `--scan-target-name` overrides auto-derived document name (Priority: P2)

**Goal**: post-fix emission with `--scan-target-name "myproject"` sets the document/Sbom-level name in all three formats. Interaction with milestone-077's `--root-name` documented + tested per research §5.

**Independent Test**: emit fresh SBOMs with `--scan-target-name "foo"` (alone) and with `--scan-target-name "foo" --root-name "bar"` (both); assert per-format precedence rules from research §5.

### Tests for User Story 3

- [ ] T014 [US3] Add to `mikebom-cli/tests/sbom_user_metadata.rs`: tests `scan_target_name_overrides_default` (US3 §1-3, SC-004; assert all three formats' document-level name = `"foo"`); `scan_target_name_root_name_precedence` (US3 + research §5; `--scan-target-name "S" --root-name "R"` → CDX `metadata.component.name == "R"` (root wins, stderr warning emitted), SPDX 2.3 document-level `name == "S"` AND root Package `name == "R"` (different fields, both honored), SPDX 3 `software_Sbom.name == "S"` AND root `software_Package.name == "R"`).

**Checkpoint**: US3 passes. The `--scan-target-name` extension to milestone 077's `--root-name` is operator-tested.

---

## Phase 6: User Story 4 — `--metadata-file` sidecar JSON input (Priority: P2)

**Goal**: post-fix emission with `--metadata-file meta.json` is byte-identical to the equivalent flag invocation. Schema validation rejects malformed input. File + flag merge per FR-006.

**Independent Test**: emit fresh SBOM with `--metadata-file meta.json` containing the equivalent of `--creator X --metadata-comment Y`; byte-compare against the same SBOM emitted with `--creator X --metadata-comment Y` as flags.

### Tests for User Story 4

- [ ] T015 [US4] Add to `mikebom-cli/tests/sbom_user_metadata.rs`: tests `metadata_file_loads_correctly` (US4 §1, SC-005; emit SBOM via `--metadata-file` containing all four field types; assert each lands at the correct format-native location); `metadata_file_unknown_field_fails` (US4 §3; file with `creator` (singular, typo) → fails with `unknown field "creator", expected one of [creators, annotators, metadata_comment, scan_target_name]`); `metadata_file_malformed_json_fails` (US4 §4; file with truncated JSON → fails with line+column error); `file_and_flags_merge_arrays` (US4 §2; file `creators: ["A"]` + `--creator "B"` → SBOM contains BOTH A and B, in that order per research §6); `file_and_flag_conflict_on_singular_fails` (FR-006 conflict; file `metadata_comment: "X"` + `--metadata-comment "Y"` → fails with the `ConflictError` message naming both sources).

**Checkpoint**: US4 passes. The sidecar JSON path is operator-tested.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [ ] T016 Add to `mikebom-cli/tests/sbom_user_metadata.rs`: `determinism_byte_identical_across_runs` (FR-009 + SC-009; same flag inputs + same scan inputs across two re-runs → byte-identical SBOM); `spdx3_conformance_with_full_metadata` (SC-008; emit SPDX 3 SBOM with all five flag families populated; shell out to milestone-078's `run_validator` helper; assert zero violations including the new Annotation + Tool/Organization/Person elements); `cdx_native_annotations_emit_correctly` (Q2 audit confirmation; CDX SBOM has no `mikebom:invocation-comment` or `mikebom:annotation` properties — confirms full native parity per Phase 0 §1, parity-bridge fallback NOT triggered); `schema_validation_passes_with_full_metadata_per_format` (FR-010 + SC-007; emit fresh CDX 1.6 + SPDX 2.3 + SPDX 3 SBOMs with all five flag families populated; validate each against its respective schema fixture — CDX 1.6 against the new `mikebom-cli/tests/fixtures/schemas/cyclonedx-1.6.json` from T001; SPDX 2.3 against the existing `spdx-2.3.json`; SPDX 3 against the existing `spdx-3.0.1.json`. Reuses the validation helper pattern from `spdx_schema_validation` / `spdx3_schema_validation` test targets — grep for the existing pattern at implementation time and mirror it. Asserts no JSON Schema violations).

- [ ] T017 [P] Update `docs/reference/identifiers.md`: add a small cross-reference from milestone 077's `--root-name` section to the new `--scan-target-name` flag explaining the precedence (per research §5). Optionally add a brief note about milestone 080's flag set in the SPDX 2.3 + SPDX 3 wire-mapping sections; this milestone's full operator-facing doc lives in `quickstart.md`. **REQUIRED**: add an audit-record entry to `docs/reference/sbom-format-mapping.md` per Constitution Principle V — the entry documents the positive Phase 0 §1 audit outcome ("milestone 080 audited CDX 1.6 against `bom.annotations[]` for `--metadata-comment` and `--annotator`/`--annotation-comment` landing slots; confirmed native CDX 1.6 support; no `mikebom:` parity bridge introduced"). The entry is positive proof that the Principle V audit ran and concluded "native fields suffice" — preserves the audit trail durably for future milestone reviewers. Format follows the existing sbom-format-mapping.md row conventions.

- [ ] T018 Run pre-PR gate per CLAUDE.md: (a) confirm validator installed via `bash scripts/install-spdx3-validate.sh` (idempotent); (b) export `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1`; (c) run `./scripts/pre-pr.sh`. Both `cargo +stable clippy --workspace --all-targets -- -D warnings` (zero warnings) AND `cargo +stable test --workspace` (every target reports `0 failed`) must pass. The `sbom_user_metadata` target must report all-green (~17 new tests). Critically: `cdx_regression`, `spdx_regression`, AND `spdx3_regression` test targets MUST pass WITHOUT their respective `MIKEBOM_UPDATE_*_GOLDENS` env vars — confirms the milestone's promise that pre-flag invocations stay byte-identical (no incidental golden churn). Also verify `spdx3_conformance` (15 tests from milestones 078 + 079) still passes.

- [ ] T019 Manually validate quickstart.md recipes 1-6 end-to-end against a real local build of milestone 080. (a) Recipe 1 — execute the post-fix native-CLI replacement of the issue body's CNCF-style `jq` recipe; verify the output SPDX 2.3 SBOM matches the post-`jq` output the operator was producing today. (b) Recipes 2-5 — exercise each flag family + the per-format inspection commands; verify operator-visible output. (c) Recipe 6 — confirm the pre-PR gate behavior is unchanged from milestone 078 / 079 (graceful-skip-when-missing on local dev without Python). **(d) Deliberate-regression smoke per the milestone-078 / 079 pattern**: in a scratch commit, change `parse_creator_str` to accept any prefix (remove the validation); run `cargo test --test sbom_user_metadata creator_type_routing_per_format`; verify the test fails with a crisp routing-mismatch assertion (since `Bot:` would now route to the default routing path which is undefined). Restore the fix; re-run the gate clean.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: T001 has no in-milestone dependencies; survey + fixture-vendor task. Should land before T002+ begin so the audit findings are checked-in evidence.
- **Phase 2 (Foundational)**: T002 (skeleton) → T003+T004+T005 (parallel — different files in `binding/user_metadata/`) → T006 (depends on T003-T005 — uses their public surfaces) → T007 + T008 (parallel — different files; both depend on T006) → T009 + T010 + T011 (parallel — different format-emission files; all depend on T006). Sequential within each format file.
- **Phase 3 (US1)**: T012 depends on T009 + T010 + T011 (all three format wirings in place).
- **Phase 4 (US2)**: T013 depends on T009 + T010 + T011; same test file as T012 so file-serial.
- **Phase 5 (US3)**: T014 depends on T009 + T010 + T011; same test file so file-serial.
- **Phase 6 (US4)**: T015 depends on T005 (file loader) + T009 + T010 + T011; same test file so file-serial.
- **Phase 7 (Polish)**: T016 depends on T012-T015 (test file infrastructure); T017 [P] independent (different file); T018 depends on Phases 1-7 production + test code complete; T019 depends on T018 (need a clean build to smoke-test).

### Parallel Opportunities

- **T003 + T004 + T005** [parallel] — three different files in `binding/user_metadata/`; only the module skeleton (T002) blocks them.
- **T007 + T008** [parallel] — two different CLI files (`scan_cmd.rs`, `run.rs`).
- **T009 + T010 + T011** [parallel] — three different format-emission files. The sort-key extension across these three is the shared invariant; each file independently handles its own emission-side changes.
- **T017** [P] (docs) parallel with T012-T016 + T018/T019 — different file (`docs/reference/identifiers.md`).

### Within Each User Story

- US1 / US2 / US3 / US4 share Phase 2 production code. Test surface splits by US but lives in the shared `sbom_user_metadata.rs` file — sequential within file, but tests are independent functions.

---

## Parallel Example: Phase 2 Foundational

```bash
# Sequential: skeleton must land first
Task: "T002 Create user_metadata module skeleton + error enum"

# Parallel: three different files
Task: "T003 Implement creator.rs with parse_creator_str + Creator type"
Task: "T004 [P] Implement annotation.rs with Annotation + validate_annotator_pairs"
Task: "T005 [P] Implement metadata_file.rs with MetadataFile deserialize + load_metadata_file"

# Sequential: depends on T003-T005's public surfaces
Task: "T006 Implement merge_file_and_flags in mod.rs"

# Parallel: two different CLI files
Task: "T007 Add 5 flags to scan_cmd.rs Scan struct"
Task: "T008 [P] Add symmetric flag set to run.rs Run struct"

# Parallel: three different format-emission files
Task: "T009 Wire UserMetadata into cyclonedx/metadata.rs"
Task: "T010 [P] Wire UserMetadata into spdx/document.rs"
Task: "T011 [P] Wire UserMetadata into spdx/v3_document.rs"
```

---

## Implementation Strategy

### MVP First (Phases 1-3 = US1)

1. Phase 1 setup (T001).
2. Phase 2 foundational (T002 → T003-T005 [P] → T006 → T007-T008 [P] → T009-T011 [P]) — module + CLI flags + per-format wiring.
3. Phase 3 US1 (T012) — `--creator` integration test across all three formats.
4. **STOP and VALIDATE**: at this checkpoint, the issue body's CNCF-style `jq --creator` use case is fully native. Operators can adopt `--creator` immediately for the dominant pain point. US2-US4 add depth but US1 alone is shippable.
5. Continue to Phases 4-7.

### Incremental Delivery

Single PR. The four user stories are tightly coupled: they share the `binding/user_metadata/` module + the `merge_file_and_flags` aggregator + the per-format emission wiring. Splitting by US would require maintaining intermediate states (e.g., merge function exists but only handles `creators`) — more work than shipping all four together.

### Parallel Team Strategy

For a multi-developer team: T003/T004/T005 are independently parallelizable in Phase 2 (separate files in the new module). T009/T010/T011 are independently parallelizable for the three format wirings. Test tasks (T012-T015) are file-serial in `sbom_user_metadata.rs` but logically independent — one developer can write all five test groups sequentially in a single editing pass.

---

## Notes

- [P] = different files, no incomplete-task dependencies.
- All four user stories share Phase 2 wiring.
- Per CLAUDE.md: pre-PR gate REQUIRES both `cargo +stable clippy --workspace --all-targets -- -D warnings` clean AND `cargo +stable test --workspace` clean. Cite both in the PR description.
- Tests in `sbom_user_metadata.rs` MUST guard their `mod tests` items (and any unit-test modules) with `#[cfg_attr(test, allow(clippy::unwrap_used))]` per CLAUDE.md.
- New module `binding/user_metadata/` follows the milestone-073/074/075/076/077 `binding/identifiers/` convention.
- No new `Cargo.toml` deps. `chrono` is already in the workspace closure.
- No CI workflow updates. Reuses milestone-078's conformance gate; the new tests live in a separate test binary that CI's existing `cargo test --workspace` invocation picks up automatically.
- Validator pin stays at `spdx3-validate==0.0.5`. No bump.
- **Critical regression-test contract**: the milestone's promise is that pre-flag invocations produce byte-identical SBOMs to alpha.20. T018 verifies by running the three regression tests WITHOUT their `MIKEBOM_UPDATE_*_GOLDENS` env vars; if any goldens require regen, the production code is leaking the new emission paths to flag-less invocations and needs fixing.
- Total estimated tasks: 19. Total estimated effort: 2-3 person-days.
