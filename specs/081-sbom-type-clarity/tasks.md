---
description: "Task list for milestone 081 — SBOM-type signaling clarity"
---

# Tasks: SBOM-type signaling clarity

**Input**: Design documents from `/specs/081-sbom-type-clarity/`
**Prerequisites**: plan.md, spec.md (with /speckit.clarify Q1 integrated), research.md (with the per-format audit confirming CDX clean / SPDX 2.3 clean / SPDX 3 native field gap), data-model.md, contracts/sbom-type-signaling.md, quickstart.md

**Tests**: Spec references SC-001 through SC-009 plus the 11-test integration matrix in contracts/sbom-type-signaling.md. Test tasks are included.

**Organization**: Four user stories, two with material code work. US1 (P1) operator-facing docs; US2 (P1) audit deliverable as research.md; US3 (P2) `--sbom-type` operator-assert flag + SPDX 3 native field wiring (audit confirmed promotion is required); US4 (P3) `runtime` tier auto-detection — DEFERRED to a separate GitHub issue per research §3 (eBPF observes builds, not runtime; the `--sbom-type` flag still accepts `runtime` as a vocab value for operator self-assertion). All shipping work in one PR.

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: parallelizable (different files, no incomplete-task dependencies)
- **[Story]**: US1 / US2 / US3 (user-story phase tasks only)
- File paths are absolute or repository-relative

## Path Conventions

Single workspace. Production work touches: ONE existing helper module (`mikebom-cli/src/generate/lifecycle_phases.rs`) for the `SbomType` enum + 2 helper functions; ONE existing emission file (`mikebom-cli/src/generate/spdx/v3_document.rs`) for the SPDX 3 native field wiring; TWO existing CLI files (`scan_cmd.rs`, `run.rs`) for the `--sbom-type` flag. No new modules. No new crates. No new dependencies. Reuses milestone 078's conformance gate as-is.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Capture pre-implementation findings; verify the milestone-078 validator setup is intact; file the deferred-work follow-up issue.

- [ ] T001 Audit + setup pass. (a) **Validator binary still installed**: `.venv/spdx3-validate/bin/spdx3-validate --version` reports `0.0.5` (the milestone-078 pin); if not, run `bash scripts/install-spdx3-validate.sh`. (b) **File deferred-work GitHub issue** for the `runtime` tier auto-detection per research §3: title `"Runtime SBOM auto-detection mode (separate from build-tier eBPF trace)"`, body explaining mikebom's eBPF observes builds not runtime per CISA semantics, and that this milestone's `--sbom-type runtime` flag accepts the value but auto-detection is future work requiring a real runtime-observation mode. Capture the issue number; reference it in the milestone PR description. (c) **Re-confirm existing emission call sites**: grep `mikebom-cli/src/generate/spdx/v3_document.rs` for the SpdxDocument construction site (where the new `software_sbomType[]` field will land — likely near the existing `comment` emission that milestone 047 wired). Confirm `lifecycle_phases.rs::aggregate_phases` signature so T002's new `aggregate_spdx3_sbom_types` can mirror it.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Extend `lifecycle_phases.rs` with the `SbomType` enum + 2 helper functions; wire CLI flag plumbing through both subcommands. After this phase, the production-side helpers are in place and the per-format wirings (Phase 3) plug in.

**⚠️ CRITICAL**: All US1 + US2 + US3 tracks depend on this phase.

- [ ] T002 Extend `mikebom-cli/src/generate/lifecycle_phases.rs` with ALL lifecycle_phases.rs changes for this milestone (consolidated into one pass to avoid touching the file twice across phases): (a) `SbomType` enum (6 variants `Design`/`Source`/`Build`/`Analyzed`/`Deployed`/`Runtime`) + `as_spdx3_iri` / `as_str` / `parse_str` methods + `ParseSbomTypeError` (`thiserror::Error` derive) per data-model.md; (b) `tier_to_spdx3_sbomtype_iri(tier: &str) -> Option<&'static str>` mirroring the existing `tier_to_phase` pattern at line 33; (c) `aggregate_spdx3_sbom_types(components, override_assertion: Option<SbomType>) -> Vec<&'static str>` mirroring `aggregate_phases` at line 49 — same `BTreeSet`-backed lexicographic ordering; (d) **EXTEND the existing `aggregate_phases` signature** at line 49 to also accept `override_assertion: Option<SbomType>` — when `Some(SbomType::Build)` etc., return single-element Vec with the corresponding CDX phase via the equivalence table; when `None`, retain existing milestone-047 aggregation behavior. Per VR-081-001 through VR-081-003. The single-pass scope keeps this file's diff bounded and removes the same-file constraint between Phase 2 and Phase 5.

  **Pre-pin the override-propagation field name** (per analyze H1): add `pub sbom_type_override: Option<crate::generate::lifecycle_phases::SbomType>` field to `mikebom-cli/src/generate/ScanArtifacts<'_>` struct (mirror the milestone-080 `user_metadata` field's location — grep `pub user_metadata:` in `mikebom-cli/src/generate/mod.rs` to find the canonical spot, add the new field next to it). All emission code paths consume the override via this field. T004 + T005 + T009 + T010 reference this exact field name.

- [ ] T003 Add unit tests in `mikebom-cli/src/generate/lifecycle_phases.rs`'s existing `#[cfg(test)] mod tests` block (or extend it) with `#[cfg_attr(test, allow(clippy::unwrap_used))]` per CLAUDE.md: (a) `sbom_type_parse_str_accepts_six_vocab_values` — table-driven over the 6 valid inputs; (b) `sbom_type_parse_str_rejects_invalid_value` — assert `ParseSbomTypeError` for at least 3 invalid inputs (`"Build"` (case mismatch), `"foobar"`, `""`); (c) `tier_to_spdx3_sbomtype_iri_returns_correct_iri_for_each_tier` — table-driven over the 6 tier values; (d) `tier_to_spdx3_sbomtype_iri_returns_none_for_unknown_tier`; (e) `aggregate_spdx3_sbom_types_with_override_returns_single_element_vec` — pass `Some(SbomType::Build)` + components with mixed tiers → assert single-element output regardless of components; (f) `aggregate_spdx3_sbom_types_without_override_aggregates_lex_sorted` — pass `None` + components with `["build", "source"]` tiers → assert output is `["spdx:Software/SbomType/build", "spdx:Software/SbomType/source"]` (lex order).

- [ ] T004 Add the `--sbom-type` flag to `mikebom-cli/src/cli/scan_cmd.rs`'s `Scan` struct via clap derive: `pub sbom_type: Option<SbomType>` with `value_parser = mikebom::generate::lifecycle_phases::SbomType::parse_str`. Help text per the contract: `"Override the auto-detected SBOM type with an operator-asserted CISA SBOM Type. Valid values: design, source, build, analyzed, deployed, runtime. Document-level only — per-component sbom-tier annotations preserve auto-detected values."`. Plumb the parsed `Option<SbomType>` through `scan_cmd.rs::execute` to set `ScanArtifacts.sbom_type_override` (the field T002 added) at the construction site.

- [ ] T005 Symmetric flag set on `mikebom-cli/src/cli/run.rs`'s `Run` struct (the `mikebom trace run` subcommand). Same `pub sbom_type: Option<SbomType>` field with the same `value_parser`. Plumb through to set `ScanArtifacts.sbom_type_override` at the trace-tier emission path's ScanArtifacts construction site. Both subcommands set the same field; no copy-paste of parsing logic.

---

## Phase 3: User Story 1 — Operator-facing SBOM-type docs (Priority: P1)

**Goal**: An operator inspecting a mikebom-emitted SBOM can identify the CISA SBOM Type via `docs/reference/sbom-types.md` in under 60 seconds. The doc covers all three formats with `jq` recipes + the four-column equivalence table per Q1 mixed-tier transparency.

**Independent Test**: take any of the three formats from a milestone-080 alpha.21 demo SBOM (or a fresh emission); follow the doc's per-format `jq` recipe; confirm the SBOM type identification matches the actual data lineage.

### Implementation for User Story 1

- [ ] T006 [US1] Create `docs/reference/sbom-types.md` per FR-001. Sections: (a) **Overview** — CISA SBOM Types framework citation (April 2023 PDF link) + the 6 types with one-line definitions each; (b) **Per-format field-position table** — CDX `metadata.lifecycles[].phase`, SPDX 2.3 `creationInfo.comment` parse-and-translate, SPDX 3 `software_Sbom.software_sbomType[]`; (c) **Per-format `jq` recipes** — copy-pasteable for each format from quickstart.md Recipe 1; (d) **The four-column equivalence table** — CISA Type ↔ mikebom tier ↔ CDX phase ↔ SPDX 3 SbomType IRI per research §2; (e) **Mixed-type SBOM presentation** — per Q1 clarification: docs treat multi-element `lifecycles[]` / `software_sbomType[]` as "Mixed-type SBOM" spanning multiple CISA types; operators wanting single-type assertion are pointed at `--sbom-type` (US3); (f) **Empty-SBOM handling** — absence of signal documented; (g) **mikebom version mismatch** — note that milestone 047 introduced lifecycle aggregation; pre-047 SBOMs lack the signal entirely; milestone 081 added SPDX 3 native-field promotion; (h) **CDX-vs-CISA naming-case caveat** — CDX uses `pre-build`/`post-build`/`operations`; CISA uses Title Case (`Source`/`Analyzed`/`Deployed`); the equivalence table column normalizes to lowercase to match emission. Length target: 200–300 lines for thoroughness without bloat.

- [ ] T007 [US1] Discoverability cross-references for the new `docs/reference/sbom-types.md` per FR-007 + SC-009: (a) **identifiers.md**: add a short subsection (perhaps §6.4 or wherever the existing reference document has natural cross-reference slots) titled "SBOM types and lifecycle phases" with one paragraph explaining that mikebom signals SBOM types via three different format-native fields and pointing readers to `docs/reference/sbom-types.md` for the full reference. (b) **README**: add a single sentence cross-reference to the project README's "What mikebom emits" section (or equivalent — grep the README for the section that describes the three formats; add the reference inline). The README cross-reference is REQUIRED per FR-007 + SC-009; an operator searching the README for "SBOM type" must surface the deeper reference.

**Checkpoint**: US1 passes. Operators can identify SBOM types from any mikebom-emitted format via the new doc.

---

## Phase 4: User Story 2 — Audit deliverable (Priority: P1)

**Goal**: The Phase 0 audit per research.md §1 produces a per-format gap list operators + maintainers can verify against the format specs. SPDX 3's native-field gap is explicitly named + tied to the milestone's code work in US3.

**Independent Test**: a maintainer reading `research.md` §1 can cross-check one claim per format against the spec (CDX 1.6 schema for `lifecycles[]`, SPDX 2.3 spec for absence of native enum, SPDX 3 schema for `software_SbomType` enum) and find each claim accurate.

### Implementation for User Story 2

- [ ] T008 [US2] Already done by /speckit.plan Phase 0 — `research.md` §1 IS the audit deliverable. The audit ran during /speckit.plan setup; this task is the verification step. (a) Confirm `research.md` §1 cites the SPDX 3 schema fixture path (`mikebom-cli/tests/fixtures/schemas/spdx-3.0.1.json`). (b) Confirm `research.md` §2 has the four-column equivalence table accurate per the actual format specs. (c) Add a per-format conclusion line to each audit-record entry in `docs/reference/sbom-format-mapping.md` Section I (the milestone-080 audit-record appendix): one new row for milestone 081 documenting the audit outcome — CDX clean (no change), SPDX 2.3 escape clause (no native field), SPDX 3 native field promotion from `comment`-aggregation to `software_Sbom.software_sbomType[]`.

**Checkpoint**: US2 passes. The audit is verifiable + the audit-record is durable.

---

## Phase 5: User Story 3 — `--sbom-type` operator-assert flag + SPDX 3 native field wiring (Priority: P2)

**Goal**: SPDX 3 emission gains the native `software_Sbom.software_sbomType[]` array field from the same per-component tier aggregation that already feeds CDX `metadata.lifecycles[]`. The `--sbom-type` flag (when asserted) overrides aggregation in all three formats while preserving per-component tier annotations.

**Independent Test**: emit a fresh SPDX 3 SBOM via `mikebom sbom scan --path .`; assert `software_Sbom.software_sbomType[]` is populated with the aggregated tier IRIs. Then re-emit with `--sbom-type build`; assert all three formats reflect the operator-asserted single-type override; assert per-component `mikebom:sbom-tier` annotations preserve auto-detected values.

### Implementation for User Story 3

- [ ] T009 [US3] Wire `software_Sbom.software_sbomType[]` emission in `mikebom-cli/src/generate/spdx/v3_document.rs` at the SpdxDocument construction site. Call `aggregate_spdx3_sbom_types(components, scan_artifacts.sbom_type_override)` (or whichever field T004/T005 named). When the returned Vec is non-empty, add `"software_sbomType": <Vec<&str>>` to the SpdxDocument JSON object. When empty (no components carry tiers, or tiers don't map to known IRIs), OMIT the field entirely (matches the milestone-047 `metadata_omits_lifecycles_when_no_tiers_present` pattern at `cyclonedx/metadata.rs:808`). Per VR-081-004.

- [ ] T010 [US3] Update CDX + SPDX 2.3 call sites to pass `scan_artifacts.sbom_type_override` through to the (already-extended-by-T002) `aggregate_phases(components, override)` helper: (a) CDX `cyclonedx/metadata.rs:73` — change the existing call to pass the override; (b) SPDX 2.3 `spdx/document.rs` — wherever the comment aggregation references `aggregate_phases`, pass the override. T002 already added the `override_assertion` parameter to `aggregate_phases`'s signature; T010 only updates the call sites.

- [ ] T011 [US3] Create `mikebom-cli/tests/sbom_type_signaling.rs` with module-level helpers (a `run_scan` helper that invokes `mikebom sbom scan` against a synthetic tempdir with the flag set under test, then reads + parses each emitted format) and the integration tests per the contract test matrix: `spdx3_sbomtype_emitted_natively_for_source_tier`, `spdx3_sbomtype_emitted_natively_for_build_tier`, `spdx3_sbomtype_aggregates_mixed_tiers` (US1 §3, Q1 mixed-tier — 2-element array sorted lex), `cdx_lifecycles_unchanged_from_milestone_047` (regression smoke — same fixture, post-fix output's `metadata.lifecycles[]` matches a pre-recorded expected value), `spdx2_comment_aggregation_unchanged` (regression smoke — same), `sbom_type_flag_overrides_spdx3_native` (US3 §1 — `--sbom-type build` → `software_sbomType: ["spdx:Software/SbomType/build"]`), `sbom_type_flag_overrides_cdx_lifecycles` (US3 §1 CDX path — `--sbom-type build` → `metadata.lifecycles: [{phase: "build"}]`), `sbom_type_flag_preserves_per_component_tiers` (US3 §2, SC-005 — assert per-component `mikebom:sbom-tier` annotations are unchanged from auto-detection when `--sbom-type` is asserted). All tests use `#[cfg_attr(test, allow(clippy::unwrap_used))]` per CLAUDE.md.

- [ ] T012 [US3] Add to `mikebom-cli/tests/sbom_type_signaling.rs`: `sbom_type_invalid_value_fails_parse` (US3 §3, SC-006 — `--sbom-type foobar` fails with the documented error message); `sbom_type_runtime_value_accepted` (per analyze C1 — `--sbom-type runtime` parses successfully + emits `software_sbomType: ["spdx:Software/SbomType/runtime"]` regardless of per-component tiers; verifies the deferred-auto-detection vocab acceptance per the SC-007 rewrite); `spdx3_conformance_with_native_sbomtype` (FR-010 + milestone-078 SHACL gate — emit fresh SPDX 3 SBOM with the new field populated; shell out to `run_validator` from `mikebom-cli/tests/spdx3_conformance.rs`; assert zero violations including the new `software_sbomType[]` field); `schema_validation_passes_per_format` (FR-010 — emit fresh CDX 1.6 + SPDX 2.3 + SPDX 3 SBOMs; validate each against its respective schema fixture per the milestone-080 pattern; assert no violations).

**Checkpoint**: US3 passes. The SPDX 3 native field is wired + the operator-assert flag works across all three formats.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [ ] T013 Regenerate the 9 SPDX 3 byte-identity goldens. Run `MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo test --test spdx3_regression` (per the established pattern from milestone 078). Per-fixture expected diff: +1 array field (`software_sbomType[]`) on the SpdxDocument element. Verify by spot-check on at least 3 fixtures (`apk.spdx3.json`, `cargo.spdx3.json`, `golang.spdx3.json`) that the diff is bounded to the new field; no other structural changes. Critically: `cdx_regression` and `spdx_regression` MUST pass WITHOUT their `MIKEBOM_UPDATE_*_GOLDENS` env vars (CDX 1.6 + SPDX 2.3 emission unchanged per FR-006).

- [ ] T014 Run pre-PR gate per CLAUDE.md: (a) confirm validator installed via `bash scripts/install-spdx3-validate.sh` (idempotent); (b) export `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1`; (c) run `./scripts/pre-pr.sh`. Both `cargo +stable clippy --workspace --all-targets -- -D warnings` (zero warnings) AND `cargo +stable test --workspace` (every target reports `0 failed`) must pass. The `sbom_type_signaling` target must report all-green (~11 tests). The `spdx3_conformance` target must continue to pass (the new `software_sbomType[]` field is spec-conformant per the schema audit — milestone 078's SHACL gate accepts it). Capture the per-target "ok. N passed; 0 failed" lines for the PR description.

- [ ] T015 Manually validate quickstart.md recipes 1-5 end-to-end against a real local build. (a) Recipe 1 — confirm each per-format `jq` query produces the expected output on a fresh source-tier scan. (b) Recipe 2 — confirm the four-column equivalence table is operationally correct by emitting a build-tier scan + manually translating each format's value through the table. (c) Recipe 3 — emit with `--sbom-type build`; confirm single-type override visible in all three formats; confirm per-component tiers unchanged. (d) Recipe 4 — emit a synthetic mixed-tier scan (e.g., the `polyglot-monorepo` fixture); confirm `software_sbomType[]` is multi-element and the docs treat it as "Mixed-type SBOM" rather than collapsing. (e) Recipe 5 — confirm the pre-PR gate behavior is unchanged. **Plus a deliberate-regression smoke per the milestone-078 / 079 / 080 pattern**: in a scratch commit, change `tier_to_spdx3_sbomtype_iri` to return `None` for `"build"` (simulate a regression dropping the IRI mapping); run `cargo test --test sbom_type_signaling spdx3_sbomtype_emitted_natively_for_build_tier`; capture the failure showing the test catches the regression. Restore via `git stash pop`; re-run gate clean.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: T001 has no in-milestone dependencies; survey + issue-filing.
- **Phase 2 (Foundational)**: T002 (skeleton + 2 helpers) → T003 (unit tests for the helpers; depends on T002) → T004 + T005 (parallel — different CLI files; both depend on T002 for the `SbomType` type).
- **Phase 3 (US1)**: T006 + T007 don't strictly depend on production code (the docs describe behavior that's been wired since milestone 047 + the new SPDX 3 field that lands in Phase 5). T006/T007 CAN run in parallel with Phase 2 if needed but the docs are easier to write AFTER Phase 5 lands so the per-format wire-shape diffs are concrete.
- **Phase 4 (US2)**: T008 verifies + propagates the existing audit; doesn't depend on production code.
- **Phase 5 (US3)**: T009 + T010 (parallel — different files: v3_document.rs / metadata.rs+spdx/document.rs) both depend on T002. T011 depends on T009 + T010 (production wiring in place). T012 depends on T011 (same test file).
- **Phase 6 (Polish)**: T013 depends on Phase 5 complete (production code emits the new field; goldens regenerate); T014 depends on T013 (need clean goldens to gate); T015 depends on T014 (need clean build to smoke-test).

### Parallel Opportunities

- **T004 + T005** [parallel] — two different CLI files (`scan_cmd.rs`, `run.rs`).
- **T009 + T010** [parallel] — different format-emission files.
- **T006 + T007** [P, polish-adjacent] — docs files, parallel with everything Phase 2/3/4-onward.

### Within Each User Story

- US1 (T006 + T007) entirely docs; no production code dependencies.
- US3 (T009 + T010 + T011 + T012) shares the new test file `sbom_type_signaling.rs` — sequential within file but tests are independent functions.

---

## Parallel Example: Phase 2 Foundational

```bash
# Sequential: helpers + unit tests
Task: "T002 Extend lifecycle_phases.rs with SbomType + 2 helpers"
Task: "T003 Unit tests for SbomType + helpers"

# Parallel: two different CLI files
Task: "T004 [US3-foundational] Add --sbom-type to scan_cmd.rs"
Task: "T005 [P] [US3-foundational] Add --sbom-type to run.rs"

# (Phase 5 runs after Phase 2)
# Parallel: two different emission files
Task: "T009 [US3] Wire software_sbomType[] in v3_document.rs"
Task: "T010 [P] [US3] Extend aggregate_phases override + CDX/SPDX 2.3 call sites"
```

---

## Implementation Strategy

### MVP First (Phases 1-3 = US1 docs only)

1. Phase 1 setup (T001).
2. Phase 3 US1 docs (T006 + T007 [P]) — operator-facing reference doc.
3. **STOP and VALIDATE**: at this checkpoint, the existing milestone-047 lifecycle infrastructure becomes operator-visible. Operators can identify SBOM types from any mikebom-emitted format via the new doc, even before the SPDX 3 native field promotion lands.
4. Continue to Phases 2 + 4 + 5 + 6 for the full milestone.

### Incremental Delivery

Single PR. The audit (US2) + docs (US1) + SPDX 3 native field wiring (US3) are tightly coupled — splitting would create a transient state where the docs cite SPDX 3 native field behavior that hasn't shipped, OR the production code ships without the docs explaining what landed.

### Parallel Team Strategy

Single developer + reviewer fits comfortably. T002/T003/T004/T005/T009/T010 are independently parallelizable but the milestone is small enough (~80 LOC production + ~150 LOC test) that one person sequencing through is fine.

---

## Notes

- [P] = different files, no incomplete-task dependencies.
- US1/US2/US3 all ship in this PR. US4 (`runtime` tier auto-detection) is DEFERRED to a separate GitHub issue per research §3 (T001 files it).
- Per CLAUDE.md: pre-PR gate REQUIRES both `cargo +stable clippy --workspace --all-targets -- -D warnings` clean AND `cargo +stable test --workspace` clean.
- Tests in `sbom_type_signaling.rs` MUST guard `mod tests` items with `#[cfg_attr(test, allow(clippy::unwrap_used))]` per CLAUDE.md.
- New unit tests in `lifecycle_phases.rs`'s existing `mod tests` block follow the same guard convention.
- No new `Cargo.toml` deps. No CI workflow updates.
- Validator pin stays at `spdx3-validate==0.0.5`.
- **Critical regression-test contract**: CDX 1.6 + SPDX 2.3 byte-identity goldens stay byte-identical (no emission change for those formats). Only SPDX 3 goldens regenerate, and the per-fixture diff is bounded to the new `software_sbomType[]` field. T013 verifies. T014 confirms via the pre-PR gate.
- Total estimated tasks: 15. Total estimated effort: 1-2 person-days.
