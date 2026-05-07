---
description: "Task list for milestone 079 — SPDX 3 externalIdentifierType controlled-vocabulary conformance"
---

# Tasks: SPDX 3 externalIdentifierType controlled-vocabulary conformance

**Input**: Design documents from `/specs/079-spdx3-id-vocab/`
**Prerequisites**: plan.md, spec.md (with /speckit.clarify Q1 + Q2 integrated), research.md (with schema-validated mapping table), data-model.md, contracts/spdx3-id-vocab-mapping.md, quickstart.md

**Tests**: Spec references SC-001 through SC-008 plus the 9-test integration matrix in contracts/spdx3-id-vocab-mapping.md. Test tasks are included.

**Organization**: Three user stories. US1 (P1) covers the auto-detected + build-tier identifier paths (image / repo / git / subject / attestation); US2 (P2) covers user-defined `--component-id <PURL>=<SCHEME>:<VALUE>` invocations; US3 (P2) hardens the milestone-078 CI gate to cover the broader identifier surface. All three ship in one PR per the spec assumptions section.

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: parallelizable (different files, no incomplete-task dependencies)
- **[Story]**: US1 / US2 / US3 (user-story phase tasks only)
- File paths are absolute or repository-relative

## Path Conventions

Single workspace; bulk of milestone work is in one new module (`mikebom-cli/src/generate/spdx/v3_id_type_map.rs`) plus minimal touch-ups at two existing emission call sites + extensions to the milestone-078 integration test file. No new shell helpers, no CI workflow updates, no new Cargo dependencies. Reuses milestone 078's `scripts/install-spdx3-validate.sh` + `MIKEBOM_REQUIRE_SPDX3_VALIDATOR` env var as-is.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Capture pre-implementation findings; confirm the milestone-078 validator setup is intact.

- [ ] T001 Audit two pre-implementation deliverables and capture each in this PR's commit message or a checked-in scratchpad. (a) **Validator binary still installed at the expected path**: confirm `.venv/spdx3-validate/bin/spdx3-validate --version` reports `0.0.5` (the milestone-078 pin); if not, run `bash scripts/install-spdx3-validate.sh` to restore. (b) **Exact emission call sites in v3_document.rs and v3_packages.rs**: re-confirm via `grep -n 'externalIdentifierType' mikebom-cli/src/generate/spdx/`; expected sites are `v3_document.rs:309` (document-level) and `v3_packages.rs:170` (per-package). T004 modifies these. Note any nearby call-site shifts since milestone 078 landed.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Ship the production-side mapping helper + wire it into both emission call sites + extend the sort key. After this phase, the SPDX 3 emission code path produces controlled-vocabulary-conformant output for every identifier source; story phases verify via tests.

**⚠️ CRITICAL**: All three user-story tracks depend on this phase.

- [ ] T002 Create `mikebom-cli/src/generate/spdx/v3_id_type_map.rs` per data-model.md. (a) Define `pub(crate) enum SpdxIdType` with 11 variants (`Other`, `Cve`, `Swhid`, `SecurityOther`, `Cpe23`, `PackageUrl`, `Gitoid`, `Cpe22`, `UrlScheme`, `Email`, `Swid`); add `pub fn as_str(self) -> &'static str` returning the literal vocab string per the SPDX 3 schema. (b) Define `pub(crate) struct MappingResult { pub vocab_type: SpdxIdType, pub comment: Option<String> }`. (c) Implement `pub(crate) fn map_scheme_to_vocab(scheme: &SchemeName, value: &str) -> MappingResult` per the per-scheme table in research.md §1: short-circuit when scheme name IS a vocab value; check `is_git_sha(value)` for `git:` scheme; default `Other` with `Some(format!("original-scheme: {}", scheme.as_str()))`. (d) Implement `fn is_git_sha(value: &str) -> bool` using `std::sync::OnceLock<regex::Regex>` compiled to `^[0-9a-f]{40}$`. Register the new module in `mikebom-cli/src/generate/spdx/mod.rs`.

- [ ] T003 Add to `mikebom-cli/src/generate/spdx/v3_id_type_map.rs`: table-driven unit tests `id_type_mapping_unit_table` covering every (scheme, value) pair from research.md §1 (9 rows) and `git_sha_detected_as_gitoid` covering the regex boundary cases per research §2 (40-char hex SHA → Gitoid; `git+https://...` URL → Other; abbreviated 7-char SHA → Other; 64-char hex SHA-256 → Other). Tests live in a `#[cfg(test)] mod tests { ... }` block guarded with `#[cfg_attr(test, allow(clippy::unwrap_used))]` per CLAUDE.md.

- [ ] T004 Wire the new mapping into `mikebom-cli/src/generate/spdx/v3_document.rs` (around line 309) AND `mikebom-cli/src/generate/spdx/v3_packages.rs` (around line 170). At each call site: (a) replace `"externalIdentifierType": id.scheme.as_str()` with `"externalIdentifierType": map_scheme_to_vocab(&id.scheme, &id.value).vocab_type.as_str()`. (b) When `MappingResult.comment` is `Some(text)`, add `"comment": text` to the externalIdentifier JSON object. When `None`, do NOT add a `comment` field (no empty string, no null). Use `serde_json::Map::insert` conditionally. Add `use crate::generate::spdx::v3_id_type_map::map_scheme_to_vocab;` at file top. Both call sites use the SAME helper — no copy-paste of mapping logic.

- [ ] T005 Update the sort-key in `mikebom-cli/src/generate/spdx/v3_external_ids.rs` (or wherever the per-component `externalIdentifier[]` array gets sorted) per research.md §4: extend from `(externalIdentifierType, identifier)` to `(externalIdentifierType, identifier, comment.unwrap_or(""))`. Verify by inspection that the document-level externalIdentifier array (constructed in `v3_document.rs:309`-region) uses the same sort key; if it has its own sort, mirror the extension. Per VR-079-006: two array entries with identical `(type, identifier, comment)` triples are genuine duplicates and dedup to one entry.

---

## Phase 3: User Story 1 — Auto-detected + build-tier identifier conformance (Priority: P1)

**Goal**: post-fix emission of any image-tier scan with RepoTags, source-tier scan inside a git repository, or build-tier trace with subjects/attestations passes `spdx3-validate` with zero `externalIdentifierType` violations. Original mikebom scheme name is recoverable from the `comment` field on each affected `externalIdentifier` element.

**Independent Test**: emit a fresh SPDX 3 SBOM via image scan / source-with-git scan / build-tier emission; assert the validator reports zero violations AND the `comment` field surfaces the original scheme verbatim.

### Tests for User Story 1

- [ ] T006 [US1] Add to `mikebom-cli/tests/spdx3_conformance.rs`: test `image_tier_with_repo_tags_passes_validator` — construct a synthetic image scan target with NON-empty RepoTags (per the issue #154 reproduction recipe; e.g., `vec!["registry.example.com/img:tag".to_string()]`); emit fresh SPDX 3 via `mikebom sbom scan --image <synthetic>`; call the existing milestone-078 `run_validator` helper; assert zero `Violation of type` markers in stderr; additionally assert the emitted `externalIdentifier[]` array contains an entry with `externalIdentifierType == "other"` AND `comment == "original-scheme: image"`. Replaces the milestone-078 dodge (which used empty RepoTags) — this milestone fixes the underlying issue so the dodge is no longer needed.

- [ ] T007 [US1] Add to `mikebom-cli/tests/spdx3_conformance.rs`: test `source_tier_in_git_repo_passes_validator` — construct a tempdir, run `git init` + `git remote add origin https://example.com/foo/bar.git` + create one commit so milestone-074's `git_rev_parse_head` succeeds; emit fresh SPDX 3 via `mikebom sbom scan --path <tempdir>`; call `run_validator`; assert zero violations; additionally assert the emitted `externalIdentifier[]` contains entries with: (a) `externalIdentifierType == "other"` + `comment == "original-scheme: repo"` for the remote URL, (b) `externalIdentifierType == "gitoid"` (NO `comment`) for the git SHA. Verifies both the FR-002 mapping for `repo:` and the FR-004 gitoid detection for `git:` SHAs in one test against a realistic source-tier path.

- [ ] T008 [US1] Add to `mikebom-cli/tests/spdx3_conformance.rs`: test `build_tier_with_subjects_passes_validator` — construct a synthetic `ScanArtifacts` with `GenerationContext::BuildTimeTrace` carrying at least one `subject:` and one `attestation:` identifier (mirror the milestone-076/077 synthetic-build-tier helper pattern from `triple_format_perf.rs`); pass to per-format builders directly; call `run_validator` against the emitted SPDX 3; assert zero violations; assert `externalIdentifier[]` contains entries with `comment == "original-scheme: subject"` and `comment == "original-scheme: attestation"` respectively.

**Checkpoint**: US1 passes. The dominant operator paths (image scan, source-with-git scan, build-tier trace) all produce SPDX 3 SBOMs that pass the validator with zero `externalIdentifierType` violations + supplementary metadata recoverability.

---

## Phase 4: User Story 2 — User-defined `--component-id` conformance (Priority: P2)

**Goal**: post-fix emission with `--component-id <PURL>=<non-vocab-SCHEME>:<VALUE>` invocations passes the validator zero-error; the user-supplied scheme name is recoverable from `comment`. Vocab-named `--component-id <PURL>=<vocab-SCHEME>:<VALUE>` invocations (e.g., `--component-id <PURL>=cve:CVE-1234`) pass through verbatim with no `comment` (no info loss).

**Independent Test**: emit fresh SPDX 3 with `--component-id <PURL>=jira:PROJ-1234` AND `--component-id <PURL>=cve:CVE-2024-1234`; assert validator zero-error; assert the first emits `externalIdentifierType: "other"` + `comment: "original-scheme: jira"`, the second emits `externalIdentifierType: "cve"` + no `comment`.

### Tests for User Story 2

- [ ] T009 [US2] Add to `mikebom-cli/tests/spdx3_conformance.rs`: test `user_defined_scheme_passes_validator` — invoke `mikebom sbom scan --path <tempdir> --component-id pkg:cargo/foo@1.0=jira:PROJ-1234 --component-id pkg:cargo/foo@1.0=cve:CVE-2024-1234` (or pick a PURL that matches a component the scan actually emits — the worker should pick a PURL from a manifest in `<tempdir>` so the `--component-id` selector matches; otherwise the flag warns "zero-match selector" and the test won't exercise the mapping path); call `run_validator`; assert zero violations; assert the JIRA identifier emits as `(type=other, comment="original-scheme: jira")` and the CVE identifier emits as `(type=cve, no comment)`. Validates both branches of FR-003 + the vocab-name short-circuit from research §1's table row 7. Per `mikebom-cli/src/binding/identifiers/component_id.rs:52`, the parser rejects the five built-in scheme names (`repo`/`git`/`image`/`attestation`/`subject`) at flag parse time, so user-defined schemes are guaranteed not to overlap with US1's auto-detect / build-tier paths.

**Checkpoint**: US2 passes. User-defined schemes — both vocab-named and non-vocab-named — emit conformantly with operator-recoverable original-scheme metadata.

---

## Phase 5: User Story 3 — CI gate hardening (Priority: P2)

**Goal**: the milestone-078 CI gate now fails on any future PR that introduces a new mikebom scheme without a vocab mapping. The `original_scheme_recoverable_from_comment` test guarantees that even when the mapping is correct, info preservation isn't accidentally dropped (e.g., emitted as empty string vs. omitted entirely).

**Independent Test**: deliberate-regression smoke (per spec.md US3 §2 + SC-008) — temporarily remove a scheme from the mapping function; verify the gate fails.

### Tests for User Story 3

- [ ] T010 [US3] Add to `mikebom-cli/tests/spdx3_conformance.rs`: test `original_scheme_recoverable_from_comment` — for each of the 5 built-in non-vocab schemes (`image`, `repo`, `git` with non-SHA value, `subject`, `attestation`) AND one user-defined non-vocab scheme (`jira`), assert that the `comment` field on the emitted externalIdentifier element starts with the literal string `"original-scheme: "` followed by the original scheme name verbatim. Use a single helper that takes (scheme_name, expected_value) and asserts via JSON-LD `@graph` traversal. Verifies SC-005 + VR-079-002 + VR-079-003 (no comment for vocab-mapped schemes).

---

## Phase 6: Polish & Cross-Cutting Concerns

- [ ] T011 Update `docs/reference/identifiers.md`: add a brief subsection in the SPDX 3 wire-mapping section that documents the per-scheme mapping table from contracts/spdx3-id-vocab-mapping.md (the same table operators see in quickstart.md Recipe 4). One-line note: "this corrects the alpha.16-alpha.19 emission shape per milestone 079 (closes GitHub issue #154)." Optionally cross-reference milestone 078's identifiers.md §6.3.1 entry so readers see the two milestones' fixes side-by-side.

- [ ] T012 Run pre-PR gate per CLAUDE.md: (a) confirm validator installed via `bash scripts/install-spdx3-validate.sh` (idempotent — does nothing if already installed); (b) export `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1` so the conformance test fails on absent binary; (c) run `./scripts/pre-pr.sh`. Both `cargo +stable clippy --workspace --all-targets -- -D warnings` (zero warnings) AND `cargo +stable test --workspace` (every target reports `0 failed`) must pass. The `spdx3_conformance` target must report all-green specifically (now ~17 tests: 10 from milestone 078 + 7 new from this milestone). Critically: `cdx_regression` and `spdx_regression` targets MUST pass WITHOUT their respective `MIKEBOM_UPDATE_*_GOLDENS` env vars — confirms FR-006 + FR-011 (CDX 1.6 + SPDX 2.3 byte-identity preservation). Also verify `spdx3_regression` passes WITHOUT `MIKEBOM_UPDATE_SPDX3_GOLDENS=1` because none of the 9 source-tier ecosystem fixtures exercise the new mapping path (per FR-007 + research §5).

- [ ] T013 Manually validate quickstart.md recipes 1-5 end-to-end against a real local build of milestone 079. Specifically: Recipe 1 (jq queries surface the new `comment` fields); Recipe 2 (validator passes against a fresh image scan); Recipe 3 (jq filter by `original-scheme:` prefix recovers identifiers correctly); Recipe 4 (per-scheme mapping reference table is accurate by spot-checking 3 rows); Recipe 5 (graceful-skip behavior unchanged from milestone 078). **(d) Deliberate-regression smoke per SC-008**: in a scratch commit, temporarily comment-out the `git` → `Gitoid` short-circuit branch in `map_scheme_to_vocab` so `git:` SHA values emit as `other`; run `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 cargo test --test spdx3_conformance`; verify `source_tier_in_git_repo_passes_validator` fails with the validator's stderr captured (will report a spurious `(other, comment="original-scheme: git")` emission instead of expected `gitoid`). Restore the fix (`git restore` the scratch revert) before opening the PR. Document the captured stderr snippet in the PR description as evidence that the gate works as designed.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: T001 has no dependencies; survey task. Should land before T002/T004 begin so the call sites are confirmed.
- **Phase 2 (Foundational)**: T002 has no in-milestone deps (creates the new module). T003 depends on T002 (tests the helper). T004 depends on T002 (uses the helper at the call sites). T005 depends on T004 (sort-key extension is in the same code-area as T004's call-site changes). Ordering: T002 → T003 → T004 → T005 (sequential within Phase 2; no [P] within Phase 2 because all touch the SPDX 3 emission area and T002's helper is the dependency root).
- **Phase 3 (US1)**: T006 + T007 + T008 all depend on T004 + T005 (production fixes in place); they are independent of each other but live in the same test file (`spdx3_conformance.rs`), so they're "logically parallel" but file-serial. Worker should write all three in one editing pass.
- **Phase 4 (US2)**: T009 depends on T004 + T005; same file as T006-T008 so file-serial.
- **Phase 5 (US3)**: T010 depends on T004 + T005; same file as T006-T009 so file-serial.
- **Phase 6 (Polish)**: T011 [P] (docs) parallel with everything else — different file. T012 depends on Phases 1-5 complete. T013 depends on T012 (need a clean build to smoke-test).

### Parallel Opportunities

- T011 [P] (docs) parallel with T006-T010 + T012/T013 — different file (`docs/reference/identifiers.md`), no dependency on the test code.

### Within Each User Story

- US1 / US2 / US3 share Phase 2 production code. Test surface splits by US but lives in the shared `spdx3_conformance.rs` file — sequential within file, but tests are independent functions.

---

## Parallel Example: Phase 6 (Polish)

```bash
# Sequential — pre-PR gate must run after all production + test code lands:
Task: "T012 Run pre-PR gate"
Task: "T013 Quickstart smoke + deliberate-regression"

# Parallel with the above — different file:
Task: "T011 [P] Update docs/reference/identifiers.md with mapping table"
```

---

## Implementation Strategy

### MVP First (Phases 1-3 = US1)

1. Phase 1 setup (T001).
2. Phase 2 foundational (T002, T003, T004, T005) — production fix + unit-tested mapping function + call-site wiring + sort-key extension.
3. Phase 3 US1 (T006, T007, T008) — integration tests for the dominant operator paths.
4. **STOP and VALIDATE**: at this checkpoint, the user-reported issue #154 is fixed for the auto-detected + build-tier identifier paths. The user-defined `--component-id` path also works correctly (uses the same helper) but isn't independently tested yet — that's US2.
5. Continue to Phases 4-6.

### Incremental Delivery

Single PR. The fix is small and tightly bounded — splitting US1 from US2/US3 would create transient state where the auto-detect path ships fixed but user-defined `--component-id` invocations have no test coverage proving they also work. Better to ship together; the contract test in T010 covers all six scheme types in one assertion sweep.

### Parallel Team Strategy

Single developer + reviewer fits this milestone comfortably. The mapping helper (T002) is small enough that splitting Phase 2 across multiple developers would add coordination overhead without saving meaningful time. Two-way parallelism (test-author-A: T006-T010 vs production-author-B: T002-T005) is possible but overkill for a ~10-task milestone.

---

## Notes

- [P] = different files, no incomplete-task dependencies.
- All three user stories share Phase 2 wiring.
- Per CLAUDE.md: pre-PR gate REQUIRES both `cargo +stable clippy --workspace --all-targets -- -D warnings` clean AND `cargo +stable test --workspace` clean. Cite both in the PR description.
- Tests in `spdx3_conformance.rs` MUST guard their `mod tests` items with `#[cfg_attr(test, allow(clippy::unwrap_used))]` per CLAUDE.md.
- New `v3_id_type_map.rs` module's tests MUST also use the same guard.
- No new `Cargo.toml` deps. `regex` is already in the dependency closure.
- No CI workflow updates. The milestone-078 gate automatically picks up the new tests because they live in the same `spdx3_conformance.rs` test binary.
- Validator pin stays at `spdx3-validate==0.0.5`. No bump.
- Total estimated tasks: 13. Total estimated effort: 1–2 person-days (smaller than 078 because no validator integration to set up + no CI workflow update + no shell helper to write).
