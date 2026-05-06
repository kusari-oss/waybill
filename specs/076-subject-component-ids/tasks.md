---
description: "Task list for milestone 076 — subject identifier scheme + per-component user-defined identifiers"
---

# Tasks: Subject identifier scheme + per-component user-defined identifiers

**Input**: Design documents from `/specs/076-subject-component-ids/`
**Prerequisites**: plan.md, spec.md (with /speckit.clarify integration), research.md, data-model.md, contracts/{subject-identifier,per-component-id}.md, quickstart.md

**Tests**: Spec references SC-001 through SC-009 plus the test matrices in both contract documents. Test tasks are included.

**Organization**: Four user stories. US1 (build-tier subject auto-detect) and US3 (cross-tier handshake) are P1 — the headline value. US4 (per-component identifiers) is also P1 — the second deliverable. US2 (manual `--subject-hash` on source/image tiers) is P2. Phases group by what blocks what; both deliverables share the foundational identifier-substrate work.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different files, no incomplete-task dependencies)
- **[Story]**: US1 / US2 / US3 / US4 (user-story phase tasks only)
- File paths are absolute or repository-relative

## Path Conventions

Single workspace; all 076 changes inside `mikebom-cli` plus `docs/reference/identifiers.md`. One small new module file (`component_id.rs`); no new crates.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Pre-flight reconnaissance before touching code.

- [X] T001 Audit four explicit deliverables and capture each in this PR's commit message or a checked-in scratchpad. T010 / T013 / T014 / T015 depend on the named outputs. (a) **Existing build-tier fixtures**: enumerate any tempdir or git-fixture-based test that exercises `mikebom trace run` end-to-end against an in-toto subject set. Per milestone 074's T001, the answer is likely "none" since `mikebom trace run` requires Linux + eBPF + privileges; confirm. (b) **Trace's in-process subject-set field name**: identify the exact struct + field on the in-process state object (post `super::scan::execute` completion) that holds `Vec<Subject>` (or whatever the in-toto subject collection is typed as). Document the type path (e.g., `mikebom::trace::AttestationBuilder.subjects: &Vec<Subject>`) so T010 can read it directly. (c) **Per-format component-emission sites**: identify the exact file path + function name where each format builds the per-component arrays — CDX `components[].properties[]` (likely `mikebom-cli/src/generate/cyclonedx/components.rs` or `mikebom-cli/src/generate/cyclonedx/mod.rs`), SPDX 2.3 `Package.externalRefs[]` (likely `mikebom-cli/src/generate/spdx/packages.rs`), SPDX 3 `Element.externalIdentifier[]` (likely `mikebom-cli/src/generate/spdx/v3_*.rs`). T013 / T014 / T015 each plug into one of these sites. (d) **Pre-existing per-component entries**: enumerate any pre-existing properties / externalRefs / externalIdentifier entries the existing emitters produce (e.g., `mikebom:not-linked` from milestone 049, `mikebom:shade-relocation` from milestone 009, the `purl` externalRef in SPDX 2.3). T013/T014/T015 must preserve these at their current array positions per research §6 — the audit produces the regression-test surface for that constraint.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Add the new types, helpers, flags, and `ScanArtifacts` field that both deliverables depend on. After this phase the production code is sanitization-aware but no story-specific tests have been written.

**⚠️ CRITICAL**: Both user-story tracks (subject + per-component) depend on this phase.

- [X] T002 Add `BuiltinScheme::Subject` variant to `mikebom-cli/src/binding/identifiers/mod.rs`. Update `from_scheme_name` (around `mod.rs:161`) to map `"subject"` → `Some(Self::Subject)`. Update `cdx_external_reference_type` (around `mod.rs:178`) to return `"attestation"` for `Self::Subject` per research §1. `spdx23_reference_category` already returns `"PERSISTENT-ID"` uniformly; no change needed. Update existing exhaustive-match call sites; the known sites identified at planning time are: (i) `cdx_external_reference_type` itself, (ii) `from_scheme_name` itself, (iii) the per-scheme test `builtin_scheme_cdx_external_reference_type_per_scheme` around `mod.rs:565`, (iv) any milestone 072/073/074/075 call site that pattern-matches on `BuiltinScheme` (`grep -rn 'BuiltinScheme::' mikebom-cli/src/` to enumerate). The compile-time exhaustiveness check catches any missed match arm. Add unit test asserting `BuiltinScheme::Subject.cdx_external_reference_type() == "attestation"` and `BuiltinScheme::Subject.spdx23_reference_category() == "PERSISTENT-ID"`.

- [X] T003 Add `pub fn validate_subject(value: &str) -> Result<(), IdentifierError>` to `mikebom-cli/src/binding/identifiers/validators.rs`. Behavior per research §4: accept `^(sha256:[0-9a-f]{64}|sha512:[0-9a-f]{128})$` exactly; reject uppercase hex, mixed-case hex, whitespace, missing algo prefix, prefix-only-no-hex, wrong-length hex, other algos. Return `IdentifierError::BuiltinValidation` on rejection so soft-fail-to-`UserDefined` triggers per FR-005. Wire `validate_subject` into the existing `validate_for_scheme` dispatcher so `BuiltinScheme::Subject` routes through it. Add 8 unit tests covering: valid sha256, valid sha512, uppercase rejection, missing prefix, wrong-length sha256 (63/65 chars), wrong-length sha512 (127/129 chars), unknown algo (`sha1:`), whitespace.

- [X] T004 Add `pub fn subject_identifiers_from_attestation_subjects(subjects: &[Subject]) -> Vec<Identifier>` to `mikebom-cli/src/binding/identifiers/auto_detect.rs`. Implementation per data-model.md "Functions": iterate subjects in input order (already lexically sorted per witness-v0.1); for each, extract `sha256` digest from the digest map (key lookup); on absent sha256, log `tracing::info!` with subject name + available algo list and skip per FR-002 + 2026-05-06 clarification; on present sha256, construct `Identifier` with scheme `subject`, value `sha256:<hex>`, kind `Builtin(Subject)` (after `validate_for_scheme` round-trip; the value should always validate but the call provides defense-in-depth), source_label `"auto-detected from build-tier in-toto subject `<name>`"`. Apply VR-076-001 + VR-076-002. Add unit tests for: single-subject sha256 happy path, multi-subject lexical order, subject-without-sha256 skip + log, subject with both sha256 and sha512 emits sha256-only, empty subject set returns empty vec.

- [X] T005 [P] Create new module `mikebom-cli/src/binding/identifiers/component_id.rs`. Implement `pub struct ComponentIdentifierFlag { selector_purl: String, scheme: SchemeName, value: IdentifierValue }`, `pub enum ComponentIdentifierFlagError` per data-model.md, `pub fn parse(raw: &str) -> Result<Self, ComponentIdentifierFlagError>` per contracts/per-component-id.md, and `pub fn parse_component_id_flag(raw: &str) -> Result<ComponentIdentifierFlag, String>` (the clap `value_parser` adapter). The `parse` function: split on FIRST `=`, reject empty LHS, split RHS on FIRST `:`, reject empty scheme, reject empty value, validate scheme via `SchemeName::new` (uses 073's regex), reject built-in scheme names (`repo`, `git`, `image`, `attestation`, `subject`) per FR-009. Apply VR-076-003. Add `pub mod component_id;` to `mikebom-cli/src/binding/identifiers/mod.rs`. Add 10 unit tests for parse-error paths + happy paths.

- [X] T006 Extend `ScanArtifacts` in `mikebom-cli/src/generate/mod.rs` with new field `pub component_identifiers: Vec<ComponentIdentifierFlag>`. Update existing struct-literal call sites (look for `ScanArtifacts { ... }` constructions) to pass `component_identifiers: vec![]` for back-compat. Update derive/impl blocks if needed. Add a brief module-level doc comment referencing data-model.md.

- [X] T007 Add `#[arg(long = "subject-hash", value_name = "ALGO:HEX", action = clap::ArgAction::Append)] pub subject_hash: Vec<String>` and `#[arg(long = "component-id", value_name = "PURL=SCHEME:VALUE", action = clap::ArgAction::Append, value_parser = component_id::parse_component_id_flag)] pub component_id: Vec<ComponentIdentifierFlag>` to `ScanArgs` in `mikebom-cli/src/cli/scan_cmd.rs`. Help text per contracts/{subject-identifier,per-component-id}.md "Help-text shape" sections. In the call site that constructs `ScanArtifacts`, build `subject:` `Identifier`s from `args.subject_hash` (reuse `Identifier::parse(format!("subject:{val}"))`-equivalent path; soft-fail per 073 contract on validation failure) and merge into the resolution pipeline; pass `args.component_id` directly to `ScanArtifacts.component_identifiers`.

- [X] T008 Add the same two flags to `RunArgs` in `mikebom-cli/src/cli/run.rs`. Wire them into the existing identifier-resolution flow at `run.rs::execute` (the `assembled_ids` block from milestone 074). Manual `--subject-hash` values become `Identifier`s appended after milestone-074's auto-detected `repo:`/`git:` entries. The build-tier auto-detect from the trace's subject set is wired separately in T010 — this task only handles flag plumbing.

**Checkpoint**: production code compiles. All foundational types + helpers + flag plumbing in place. No integration tests yet; no auto-detect wired. Both user-story tracks can proceed.

---

## Phase 3: User Story 1 — Build-tier auto-detects `subject:` from trace output (Priority: P1)

**Goal**: `mikebom trace run -- ./build.sh` produces a build SBOM body with `subject:sha256:<hex>` identifiers for each in-toto attestation subject — no manual flag required.

**Independent Test**: Build a tempdir fixture with a synthetic in-toto subject set (the test calls `auto_detect_build_tier_identifiers` or its successor directly, since `mikebom trace run` requires eBPF). Assert the resulting `Vec<Identifier>` contains a `subject:` identifier per subject with sha256, and skips subjects without sha256.

### Tests for User Story 1

- [X] T009 [US1] Create new integration test file `mikebom-cli/tests/identifiers_subject_and_component.rs` with: (a) a synthetic-subject-set fixture builder helper (constructs `Vec<Subject>` matching the in-toto witness-v0.1 shape — name + digest map); (b) test `build_tier_autodetects_subject_from_in_toto_subjects` (US1 §1: one subject with sha256 → one `subject:` identifier); (c) test `build_tier_autodetect_emits_one_subject_per_in_toto_subject` (US1 §2: 3 subjects → 3 identifiers in lexical order); (d) test `build_tier_autodetect_skips_subject_without_sha256` (US1 §3 + 2026-05-06 clarification: subject with only sha512 in digest map → no identifier emitted; info-log captured); (e) test `build_tier_autodetect_emits_sha256_only_when_multi_digest` (research-§4-aligned: subject with both sha256 AND sha512 → only `subject:sha256:...` emits); (f) test `build_tier_autodetect_empty_subject_set` (no subjects → empty vec). All tests guarded with `#[cfg_attr(test, allow(clippy::unwrap_used))]`.

### Implementation for User Story 1

- [X] T010 [US1] Wire build-tier subject auto-detect in `mikebom-cli/src/cli/run.rs::execute`. After `super::scan::execute(scan_args).await?` completes (which is when the in-toto attestation subject set has been collected by the trace pipeline), read the subject set from the in-process state (T001's audit identified the exact field) and call `subject_identifiers_from_attestation_subjects(subjects)`. Merge the resulting `Vec<Identifier>` into the existing `assembled_ids` flow per data-model.md "Updated `auto_detect_build_tier_identifiers` flow": `repo:` first, then `git:`, then auto-detected `subject:` entries (in witness-v0.1 lexical order), then manual `--subject-hash` from T008, then existing manual `--repo` / `--git-ref` etc. Verify all T009 tests pass.

**Checkpoint**: US1 passes. Build-tier `mikebom trace run` (when invoked in a real eBPF-capable environment) emits `subject:` identifiers in the SBOM body automatically.

---

## Phase 4: User Story 2 — Source-tier and image-tier accept manual `--subject-hash` (Priority: P2)

**Goal**: Operators on source-tier and image-tier scans can attach `subject:` identifiers manually via `--subject-hash`. Repeatable, augments rather than overrides auto-detected entries.

**Independent Test**: Run `mikebom sbom scan --path . --subject-hash sha256:<hex> --output out.cdx.json` and verify the emitted SBOM contains `subject:sha256:<hex>` in its identifier set.

### Tests for User Story 2

- [X] T011 [US2] Add to `mikebom-cli/tests/identifiers_subject_and_component.rs`: (a) `manual_subject_hash_flag_works_on_source_tier` — `mikebom sbom scan --path` against a tempdir with `--subject-hash sha256:abc...`; assert emitted CDX `metadata.component.externalReferences[]` contains the entry with type `attestation` and url `sha256:abc...`. (b) `manual_subject_hash_flag_repeatable` — pass `--subject-hash` twice with different values; assert both appear in the emitted SBOM in supply order. (c) `subject_value_validation_soft_fails_to_user_defined` — pass `--subject-hash banana`; assert the value rides through under user-defined namespace per FR-005; the scan exits 0. (d) `subject_identifier_emits_in_all_three_formats` — same scan with `--subject-hash`, emit CDX + SPDX 2.3 + SPDX 3; assert all three carry the value in the per-format carrier (CDX externalReferences[type:attestation], SPDX 2.3 externalRefs[PERSISTENT-ID][referenceType:subject], SPDX 3 externalIdentifier[type:subject]). (e) `manual_subject_hash_flag_works_on_image_tier` — `mikebom sbom scan --image <fixture-tar> --subject-hash sha256:def...`; assert `subject:sha256:def...` appears alongside the auto-detected `image:` identifier in the emitted SBOM. Verifies FR-003's "on `mikebom sbom scan` and `mikebom trace run`" coverage extends to image-tier scans, not just source-tier.

**Checkpoint**: US2 passes. Manual `--subject-hash` works on source-tier; per-format wire mapping correct.

---

## Phase 5: User Story 3 — Cross-tier digest handshake by string match (Priority: P1)

**Goal**: An external SBOM-store consumer holding a build SBOM with `subject:sha256:X` and an image SBOM listing a component with `hashes[].sha256 == X` can correlate them by string match alone — no mikebom-side resolver.

**Independent Test**: Construct a tempdir fixture with: a build SBOM (synthetic, with a known `subject:sha256:X` identifier), an image SBOM (synthetic, with one component whose hash equals X). Write a small jq harness in the test that performs the lookup and asserts the correlation succeeds.

### Tests for User Story 3

- [X] T012 [US3] Add to `mikebom-cli/tests/identifiers_subject_and_component.rs`: `cross_tier_handshake_image_digest_matches_build_subject` — build a fixture with two synthetic SBOMs sharing a hash; run a jq-shaped extraction (in Rust, using `serde_json::Value` to navigate) and assert that for each image-SBOM `components[].hashes[].sha256 == X`, the build SBOM with `subject:sha256:X` is found. The test does not call mikebom; it tests that the wire format mikebom emits is consumable by string-match correlation. Verifies SC-002.

**Checkpoint**: US3 passes. The cross-tier digest handshake is end-to-end-testable without mikebom infrastructure.

---

## Phase 6: User Story 4 — Per-component user-defined identifier attachment (Priority: P1)

**Goal**: Operators can attach user-defined identifiers (e.g., `kusari-id:asset-foo-prod-v2`) to specific components via `--component-id <PURL>=<scheme>:<value>`. The identifier emits in standards-native per-component carriers across all three formats.

**Independent Test**: Run `mikebom sbom scan --path . --component-id "pkg:cargo/serde@1.0.0=kusari-id:asset-foo" --output out.cdx.json`. Verify the matching component in the emitted CDX has `properties[]` entry `{name: "kusari-id", value: "asset-foo"}`. Verify zero matches produces a warn + scan-success.

### Implementation for User Story 4

- [X] T013 [US4] CDX emission: in `mikebom-cli/src/generate/cyclonedx/` (the components-emission module identified in T001's audit), after the `components[]` array is built, iterate `scan_artifacts.component_identifiers`. For each flag, find matching components by byte-equality of `purl` field per research §5. Append a new property `{name: scheme, value: value}` to each matching component's `properties[]` (preserving pre-existing entries at their original positions). After processing all flags, lexical-sort the NEW per-component entries by `(name, value)` per research §6. After the loop, call `tracing::warn!` for any flag whose selector_purl matched zero components (FR-010). Implementation must preserve existing milestone-073/074/075 byte-identity goldens (verify via existing parity-check golden suite).

- [X] T014 [US4] [P] SPDX 2.3 emission: in `mikebom-cli/src/generate/spdx/packages.rs`, mirror T013's logic — match by PURL, append `{referenceCategory: "PERSISTENT-ID", referenceType: scheme, referenceLocator: value}` entries to `Package.externalRefs[]`, lexical-sort new entries, warn on zero match.

- [X] T015 [US4] [P] SPDX 3 emission: in `mikebom-cli/src/generate/spdx/v3_*.rs` (the package/element module identified in T001's audit), mirror T013's logic — match by PURL, append `{type: scheme, identifier: value}` entries to `Element.externalIdentifier[]`, lexical-sort new entries, warn on zero match.

### Tests for User Story 4

- [X] T016 [US4] Add to `mikebom-cli/tests/identifiers_subject_and_component.rs`: (a) `component_id_attaches_to_matching_component_cdx` — `--component-id "pkg:cargo/serde@1.0.0=kusari-id:asset-foo"` against a project with `serde@1.0.0`; assert CDX `components[]` matching component has `properties[{name:"kusari-id",value:"asset-foo"}]`; non-matching components unchanged. (b) `component_id_attaches_to_matching_component_spdx23` — same scan emits SPDX 2.3; assert `Package.externalRefs[{referenceCategory:"PERSISTENT-ID",referenceType:"kusari-id",referenceLocator:"asset-foo"}]`. (c) `component_id_attaches_to_matching_component_spdx3` — same scan emits SPDX 3; assert `Element.externalIdentifier[{type:"kusari-id",identifier:"asset-foo"}]`. (d) `component_id_warns_on_zero_match` — `--component-id` with non-matching PURL; assert scan exits 0; assert warn-level log contains the unmatched selector. (e) `component_id_attaches_to_all_matching_when_multiple` — fixture with 2 components having same PURL different bom-refs; assert BOTH receive the identifier per FR-011. (f) `component_id_rejects_builtin_scheme_at_parse` — `--component-id "pkg:cargo/foo@1.0=subject:sha256:abc"`; assert clap parse error mentioning "reserved" or similar; non-zero exit. (g) `component_id_rejects_malformed_input_at_parse` — try several malformed inputs (no `=`, empty PURL, no `:`, empty scheme, empty value); assert each fails at parse with clear error. (h) `component_id_lexical_order_within_new_entries` — pass two `--component-id` flags matching same PURL with schemes `zzz:foo` and `aaa:bar`; assert emitted properties order has `aaa` first, `zzz` second per research §6. (i) `component_id_preserves_existing_properties` — fixture with a component carrying pre-existing `mikebom:not-linked=true` property; pass `--component-id` adding a new entry; assert pre-existing property at its original position; new entry appended after. (j) `component_id_deterministic_across_reruns` — invoke `mikebom sbom scan` twice with identical `--component-id` flags against the same fixture; emit CDX + SPDX 2.3 + SPDX 3 from each run; assert byte-equality of every emitted file across the two runs. Verifies SC-004 (deterministic re-emission) — analogous to milestone 074's `build_tier_autodetect_deterministic_across_reruns` test. Tests guarded with `#[cfg_attr(test, allow(clippy::unwrap_used))]`.

**Checkpoint**: US4 passes. Per-component user-defined identifiers emit in all three formats; matching, broadcast, ordering, and parse-error behavior all verified.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [X] T017 [P] Update `docs/reference/identifiers.md`: add a new section "Subject identifier scheme (`subject:`)" documenting the value form (`sha256:<hex>` or `sha512:<hex>`), the build-tier auto-detect path, the `--subject-hash` manual flag, and the per-format wire mapping (CDX externalReferences[type:attestation], SPDX 2.3 externalRefs[PERSISTENT-ID][referenceType:subject], SPDX 3 externalIdentifier[type:subject]). Add a separate section "Per-component user-defined identifiers" documenting the `--component-id` flag, the byte-equality PURL matching rule, the per-format carriers (CDX properties[], SPDX 2.3 externalRefs[PERSISTENT-ID], SPDX 3 externalIdentifier[]), the built-in scheme rejection rule, and the zero-match warn behavior. Cross-link to quickstart.md Recipes 1–5.

- [X] T018 [P] Update `mikebom-cli/src/parity/extractors/`: add new catalog row(s) for `subject:` identifier extraction (per-format carriers from contracts/subject-identifier.md) and per-component identifier extraction (per-format carriers from contracts/per-component-id.md). Use the existing `Directionality::SymmetricEqual` for both rows since the carriers are symmetric across CDX/SPDX 2.3/SPDX 3 (modulo per-format native field naming). Document the new rows in `parity/extractors/mod.rs`'s catalog comment block.

- [X] T019 Run pre-PR gate per CLAUDE.md: (a) `cargo +stable clippy --workspace --all-targets -- -D warnings` zero warnings; (b) `cargo +stable test --workspace` every target reports `0 failed`. Convenience: `./scripts/pre-pr.sh`. A failing per-crate `cargo test -p mikebom` does NOT discharge this requirement.

- [X] T020 Manually validate quickstart.md recipes 1, 2, 3, 4, 5 end-to-end against a real local build of milestone 076 (Recipe 1 requires Linux + eBPF; on macOS, the pre-trace flag-wiring portion can still be exercised). Confirm log-line phrasing matches contracts/{subject-identifier,per-component-id}.md exactly. Confirm jq snippets in the recipes work against actual emitted SBOMs.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: T001 has no dependencies; survey task.
- **Phase 2 (Foundational)**: T002 → T003 → T004 sequential (same file, building up). T005 [P] independent (different file). T006 depends on T005 (uses `ComponentIdentifierFlag` type). T007 depends on T002, T005, T006. T008 depends on T002, T004, T005, T006 (uses `subject_identifiers_from_attestation_subjects` and `ComponentIdentifierFlag`).
- **Phase 3 (US1)**: T009 (test file creation) before T010 (wiring) per TDD; T010 depends on T004 (Phase 2).
- **Phase 4 (US2)**: T011 depends on T007 (flag wiring), can run in parallel with Phase 3.
- **Phase 5 (US3)**: T012 depends on US1 implementation (T010) since US3 verifies the build-tier emission output.
- **Phase 6 (US4)**: T013 / T014 [P] / T015 [P] are different files (CDX emitter vs SPDX 2.3 emitter vs SPDX 3 emitter); they share no state and can run in parallel. T016 depends on all three implementation tasks.
- **Phase 7 (Polish)**: T017 [P] (docs) parallel with T018 [P] (parity catalog); T019 (pre-PR gate) depends on Phases 1-6 complete; T020 depends on T019 (need a clean build to smoke-test).

### Parallel Opportunities

- T005 [P] in Phase 2: independent of T002-T004 (different file).
- T011 in Phase 4: parallel with T010 (different concern; different test functions).
- T014 [P] and T015 [P] in Phase 6: parallel with each other and with T013 (three independent format emitters).
- T017 [P] and T018 [P] in Phase 7: parallel docs/catalog work.

### Within Each User Story

- US1 / US3 share the build-tier code path; US3 depends on US1 wiring.
- US2 / US4 are independent of each other; both depend on Phase 2 flag plumbing.

---

## Parallel Example: Phase 2 (Foundational)

```bash
# Sequential — same file (auto_detect.rs / mod.rs / validators.rs intermixed):
Task: "T002 Add BuiltinScheme::Subject variant in mod.rs"
Task: "T003 Add validate_subject in validators.rs"
Task: "T004 Add subject_identifiers_from_attestation_subjects in auto_detect.rs"

# Parallel with the above — different file:
Task: "T005 Create component_id.rs new module"

# After T002 + T005:
Task: "T006 Extend ScanArtifacts with component_identifiers field"

# After T002 + T005 + T006:
Task: "T007 Add flags to ScanArgs in scan_cmd.rs"
Task: "T008 Add flags to RunArgs in run.rs"
```

## Parallel Example: Phase 6 (US4 implementation)

```bash
# All three are different files, run in parallel:
Task: "T013 [US4] CDX per-component emission"
Task: "T014 [US4] [P] SPDX 2.3 per-component emission"
Task: "T015 [US4] [P] SPDX 3 per-component emission"

# After all three complete:
Task: "T016 [US4] Add per-component integration tests across all 3 formats"
```

---

## Implementation Strategy

### MVP First (Phases 1-3 = US1 alone)

1. Phase 1 setup (T001).
2. Phase 2 foundational (T002–T008).
3. Phase 3 US1 (T009 + T010).
4. **STOP and VALIDATE**: at this checkpoint, build-tier subject auto-detect works end-to-end. The wire-format works for the cross-tier handshake (US3) even without US3-specific tests. Per-component identifiers (US4) are not yet wired into format emitters but flag plumbing exists.
5. Continue to Phases 4-7 to complete the milestone.

### Incremental Delivery

The milestone is small enough (estimated <1 week) that a single PR covering Phases 1-7 is the natural shape. Splitting the subject-identifier half (Phases 2-5) from the per-component half (Phase 6) is technically possible but creates a transient state where one operator-visible feature ships in alpha-X without the other; recommend single PR.

### Parallel Team Strategy

With multiple developers:

1. Developer A: Phase 2 T002–T004 in `auto_detect.rs` / `mod.rs` / `validators.rs`.
2. Developer B: Phase 2 T005 (new `component_id.rs` module) in parallel.
3. Developer A or C: Phase 2 T006 / T007 / T008 (sequential after foundations).
4. Developer A: Phase 3 T009 + T010 (US1 test + wiring).
5. Developer B: Phase 6 T013 (CDX emitter), Developer C: Phase 6 T014 (SPDX 2.3 emitter), Developer D: Phase 6 T015 (SPDX 3 emitter) — three-way parallel.
6. Anyone: Phase 4 T011, Phase 5 T012, Phase 6 T016, Phase 7 T017–T020.

Single developer fits comfortably in <1 week. Parallel staffing is overkill for this size.

---

## Notes

- [P] = different files, no incomplete-task dependencies.
- All four user stories share Phase 2 production code. Test surface splits by user story; per-format emitter splits by format.
- Per CLAUDE.md: pre-PR gate REQUIRES both `cargo +stable clippy --workspace --all-targets -- -D warnings` clean AND `cargo +stable test --workspace` clean. Cite both in the PR description.
- Tests in `identifiers_subject_and_component.rs` MUST guard their `mod tests` items with `#[cfg_attr(test, allow(clippy::unwrap_used))]` per CLAUDE.md.
- Total estimated tasks: 20. Total estimated effort: <1 person-week.
