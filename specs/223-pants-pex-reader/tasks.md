---
description: "Task list for feature 223 (Pants pex-lockfile reader)"
---

# Tasks: Pants pex-lockfile reader

**Input**: Design documents from `/specs/223-pants-pex-reader/`
**Prerequisites**: plan.md ✅, spec.md ✅ (3 user stories), research.md ✅ (5 items), data-model.md ✅ (4 Deserialize types + 1 config), contracts/pex-lockfile-schema.md ✅, quickstart.md ✅

**Tests**: Tests ARE included — this feature ships new file-format parsing whose correctness is only auditable via test coverage. Every user story has both unit + integration tests. This matches the pattern of every other reader shipped since milestone 002.

**Organization**: Tasks grouped by user story to enable independent implementation, testing, and shipping.

## Format: `[TaskID] [P?] [Story?] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1, US2, US3)
- File paths are absolute or repo-relative from `/Users/mlieberman/Projects/mikebom`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Prepare the module directory + shared PyPI-name-normalization helper.

- [X] T001 Created module directory `waybill-cli/src/scan_fs/package_db/pants/` with 4 stub files (`mod.rs`, `lockfile.rs`, `config.rs`, `resolve_classifier.rs`), each carrying a `//! Milestone 223: <purpose>` doc-comment.
- [X] T002 Registered `pub mod pants;` in `waybill-cli/src/scan_fs/package_db/mod.rs:45` (alphabetical between `opkg` and `pip`). Compile clean.
- [X] T003 Helper **already exists** at `waybill-cli/src/scan_fs/package_db/pip/mod.rs:72` as `pub(crate) fn normalize_pypi_name_for_purl(name: &str) -> String { name.replace('_', "-").to_lowercase() }`. Same-crate visibility is sufficient for pants module; no extraction work needed. Consumers in T014/T015 use `super::pip::normalize_pypi_name_for_purl` verbatim. Note: helper does NOT normalize `.` → `-` (research.md §R3 assumed PEP 503 collapsing but pip reader deliberately preserves dots per its doc-comment for packageurl-python parity). Pants reader inherits this behavior.

**Checkpoint**: Empty pants module registered, shared PyPI-normalizer helper available.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Define the Pex-lockfile + pants.toml Deserialize types + the `ArtifactSourceType` enum so all three user stories can consume them.

**⚠️ CRITICAL**: US1/US2/US3 all depend on these types.

- [X] T004 Added `PexLockfile`, `LockedResolve`, `LockedRequirement`, `Artifact` in `waybill-cli/src/scan_fs/package_db/pants/lockfile.rs` per data-model.md.
- [X] T005 Added `ArtifactSourceType` enum + `from_url` + `as_annotation_str` in `pants/lockfile.rs` + 6 unit tests (all green).
- [X] T006 Added `PantsConfig` + `PythonSection` + `parse(bytes) -> Option<PantsConfig>` in `pants/config.rs` + 3 unit tests (all green).
- [X] T007 Added `DEV_RESOLVE_NAMES` allowlist + `classify_resolve` in `pants/resolve_classifier.rs`. **Correction from tasks.md draft**: the enum variant is `LifecycleScope::Development` (not `Dev` as written) per `waybill-common/src/resolution.rs:377`. 4 unit tests green (default→Runtime, mypy→Development, case-insensitive, unknown→Runtime).

**Checkpoint**: Deserialize types + source-type dispatcher + config parser + resolve classifier all compile with green unit tests. Ready to wire into the reader orchestrator.

---

## Phase 3: User Story 1 — Scan a Pants Python repo and emit Python components (Priority: P1) 🎯 MVP

**Goal**: `waybill sbom scan` against a Pants Python repo emits one `pkg:pypi/*` (or `pkg:generic/*`) component per locked distribution, with correct hashes, dependencies, `waybill:pants-resolve` annotation, and lifecycle-scope tagging.

**Independent Test**: Run the scan against `waybill-cli/tests/fixtures/pants_pex/minimal_python/`. Assert the emitted CDX has 3 components with `pkg:pypi/waybill-fixture-{a,b,c}@1.0.0` PURLs, sha256 hashes matching the fixture lockfile, and `waybill:pants-resolve=default` on each.

### Tests for User Story 1

- [X] T008 [P] [US1] Create synthetic fixture `waybill-cli/tests/fixtures/pants_pex/minimal_python/3rdparty/python/default.lock` per research.md §R5 fixture 1: valid Pex 2.10.0 lockfile with 3 synthetic entries (`waybill-fixture-a`, `-b`, `-c` all at `1.0.0`), each with 1 PyPI-shape artifact URL + a sha256 hash. Include one `requires_dists` edge (`waybill-fixture-b` depends on `waybill-fixture-a`) to exercise FR-003. See contracts/pex-lockfile-schema.md for exact JSON shape.
- [X] T009 [P] [US1] Create synthetic fixture `waybill-cli/tests/fixtures/pants_pex/multi_resolve/3rdparty/python/{default,mypy,pytest}.lock` per research.md §R5 fixture 2: 3 lockfiles, 2 entries each. Different fixture package names per resolve (`waybill-fixture-runtime-{a,b}` in default, `waybill-fixture-typing-{a,b}` in mypy, `waybill-fixture-testing-{a,b}` in pytest). Exercises FR-008 lifecycle-scope tagging.
- [X] T010 [P] [US1] Create integration test file `waybill-cli/tests/pants_pex_reader.rs` with 3 initial `#[test]` functions (bodies filled in T011/T012/T013):
  - `us1_minimal_python_lockfile_emits_3_pypi_components`
  - `us1_multi_resolve_tags_scope_per_allowlist`
  - `us1_requires_dists_edges_produce_dependsOn`
  Import helpers `bin()`, `run_scan()` mirroring `waybill-cli/tests/cisa_2026_signing.rs` patterns.
- [X] T011 [US1] Implement `us1_minimal_python_lockfile_emits_3_pypi_components` in `waybill-cli/tests/pants_pex_reader.rs`. Uses fixture from T008. **Emits BOTH formats in one scan invocation** (`--format cyclonedx-json --output <tmp>/us1.cdx.json --format spdx-2.3-json --output <tmp>/us1.spdx.json`) — per SC-001's explicit CDX+SPDX claim. Assert: (a) exit 0; (b) CDX contains exactly 3 components with `pkg:pypi/waybill-fixture-{a,b,c}@1.0.0` PURLs, each with 1 sha256 hash in `hashes[]` and `waybill:pants-resolve=default` in `properties[]`; (c) SPDX 2.3 output contains exactly 3 `packages[]` entries with `externalRefs` carrying the matching `pkg:pypi/*` PURLs, each with `checksums[]` containing 1 SHA256 entry, and 3 corresponding `annotations[]` carrying the `waybill:pants-resolve=default` value.
- [X] T012 [US1] Implement `us1_multi_resolve_tags_scope_per_allowlist` in `waybill-cli/tests/pants_pex_reader.rs`. Uses fixture from T009. Assert: 6 total components; the 2 from `default.lock` have `waybill:lifecycle-scope=runtime` (or absent — matches Runtime default); the 2 from `mypy.lock` have `waybill:lifecycle-scope=dev`; the 2 from `pytest.lock` have `waybill:lifecycle-scope=dev`. Every component has its correct `waybill:pants-resolve=<name>` annotation.
- [X] T013 [US1] Implement `us1_requires_dists_edges_produce_dependsOn` in `waybill-cli/tests/pants_pex_reader.rs`. Uses fixture from T008. Assert: the CDX `dependencies[]` array has an edge from `pkg:pypi/waybill-fixture-b@1.0.0` → `pkg:pypi/waybill-fixture-a@1.0.0` (per the `requires_dists` in T008's fixture).

### Implementation for User Story 1

- [X] T014 [US1] In `waybill-cli/src/scan_fs/package_db/pants/lockfile.rs`, add `pub(crate) fn parse(bytes: &[u8]) -> Option<PexLockfile>` returning `None` on any serde_json error OR when `pex_version` doesn't match `^2\.` (per contracts §"Fail-open behavior boundaries"). Log WARN with file-agnostic reason on failure (caller adds the path). Add 4 unit tests: valid Pex 2.10 → Some; missing `pex_version` → None + WARN; `pex_version="1.5"` → None + WARN; garbage bytes → None + WARN.
- [X] T015 [US1] In `waybill-cli/src/scan_fs/package_db/pants/lockfile.rs`, add module-private helper `pub(crate) fn locked_req_to_entry(req: &LockedRequirement, lockfile_path: &Path, resolve_name: &str) -> Option<PackageDbEntry>` per data-model.md field-mapping table. Returns None + WARN on: empty `project_name`, empty `version`, PURL construction failure. Consumes: PyPI-name normalizer from T003, `ArtifactSourceType::from_url` from T005, `classify_resolve` from T007. Emits `PackageDbEntry` with the 4 annotation keys documented in data-model.md §"extra_annotations mapping". Extracts PEP 508 project names from `requires_dists[]` for the `depends` field (strip everything after any of `<`, `>`, `=`, `~`, `!`, `[`, `;`). Add 3 unit tests: happy-path PyPI entry; git-URL entry → `pkg:generic/*` + `waybill:source-*` annotations; entry with empty `version` → None + WARN.
- [X] T016 [US1] In `waybill-cli/src/scan_fs/package_db/pants/mod.rs`, implement `pub fn read(scan_root: &Path) -> Vec<PackageDbEntry>`. Steps per contracts §"Discovery + orchestration data flow":
  1. If `scan_root/pants.toml` exists: parse via `config::parse()`.
  2. Enumerate candidate lockfile paths: default glob `3rdparty/python/*.lock` (use `walkdir` or manual `read_dir` — check pip reader for convention) + optional `pants.toml` `[python].lockfile` path.
  3. For each candidate: read bytes, `lockfile::parse()` → skip on None with WARN naming file.
  4. For each valid `PexLockfile`: for each `LockedResolve.locked_requirements`: call `locked_req_to_entry()` with resolve name from lockfile filename stem (e.g., `default.lock` → `"default"`).
  5. Emit FR-010 INFO log with `lockfiles_discovered`, `lockfiles_parsed_ok`, `lockfiles_skipped_corrupt`, `components_emitted` structured fields.
  6. Return accumulated `Vec<PackageDbEntry>`.
- [X] T017 [US1] Wire the new reader into `waybill-cli/src/scan_fs/package_db/mod.rs::read_all()` dispatcher. Add a `pants::read(scan_root)` call alongside the existing per-ecosystem calls; extend the aggregated result vector. Verify against existing reader-order convention (grep `pip::read\|cargo::read` to find the sibling calls).
- [X] T017a [US1] Add integration test `us1_fr010_info_log_emits_all_four_structured_fields` to `waybill-cli/tests/pants_pex_reader.rs` (per finding C1 from `/speckit-analyze`). Uses fixture from T008. Subprocess invocation with `RUST_LOG=info` env; capture combined stderr; assert stderr contains ALL FOUR structured field names (`lockfiles_discovered=`, `lockfiles_parsed_ok=`, `lockfiles_skipped_corrupt=`, `components_emitted=`) with non-negative-integer values. Regression guard for FR-010 + Principle X (Transparency).

**Checkpoint**: Run `cargo +stable test -p waybill --test pants_pex_reader`. Expect T011 + T012 + T013 + T017a all green. `waybill sbom scan --path waybill-cli/tests/fixtures/pants_pex/minimal_python/ --format cyclonedx-json --output /tmp/us1.cdx.json` produces the expected 3 components. **MVP shippable at this point.**

---

## Phase 4: User Story 2 — Dedup against `requirements.txt` (Priority: P2)

**Goal**: When both a Pex lockfile and a `requirements.txt` list the same package, the SBOM contains exactly one component sourced from the (authoritative) lockfile.

**Independent Test**: Run scan against `waybill-cli/tests/fixtures/pants_pex/with_requirements_txt/`. Assert the CDX contains exactly ONE `pkg:pypi/waybill-fixture-shared@1.0.0`, sourced from the lockfile.

### Tests for User Story 2

- [X] T018 [P] [US2] Create synthetic fixture `waybill-cli/tests/fixtures/pants_pex/with_requirements_txt/` per research.md §R5 fixture 4: `3rdparty/python/default.lock` with `waybill-fixture-shared@1.0.0` (with sha256 hash) + `requirements.txt` at root listing `waybill-fixture-shared==1.0.0`.
- [X] T019 [P] [US2] Add integration test `us2_lockfile_dedups_against_requirements_txt` to `waybill-cli/tests/pants_pex_reader.rs`. Uses fixture from T018. Assert: exit 0; CDX contains exactly 1 component with PURL `pkg:pypi/waybill-fixture-shared@1.0.0`; that component has 1 sha256 hash (came from lockfile — requirements.txt entries carry none); `waybill:also-detected-via` annotation contains the requirements.txt path.

### Implementation for User Story 2

- [X] T020 [US2] No new production code in this phase — dedup is entirely handled by the existing m191 reconciler at `waybill-cli/src/resolve/reconciler.rs`. Verify: run T019 first as a regression check. If it passes without any reconciler change, no additional work. If it fails: the pants reader entry doesn't have `hashes.len() > 0` OR `sbom_tier="source"` set correctly — root-cause via test failure, fix in `pants::locked_req_to_entry` (T015) not the reconciler.

**Checkpoint**: T019 green. US2 done with zero production LOC beyond US1's existing implementation.

---

## Phase 5: User Story 3 — `pants.toml` custom lockfile path (Priority: P3)

**Goal**: When `pants.toml` declares `[python].lockfile = "..."` at a non-default path, waybill discovers lockfiles at that path.

**Independent Test**: Run scan against `waybill-cli/tests/fixtures/pants_pex/pants_toml_custom_path/`. Assert waybill discovers `build-support/py.lock` (NOT `3rdparty/python/default.lock`, which doesn't exist).

### Tests for User Story 3

- [X] T021 [P] [US3] Create synthetic fixture `waybill-cli/tests/fixtures/pants_pex/pants_toml_custom_path/` per research.md §R5 fixture 3: `pants.toml` declaring `[python] lockfile = "build-support/py.lock"` + `build-support/py.lock` file with 2 synthetic entries. NO file at `3rdparty/python/`.
- [X] T022 [P] [US3] Add integration test `us3_pants_toml_custom_path_discovery` to `waybill-cli/tests/pants_pex_reader.rs`. Uses fixture from T021. Assert: exit 0; CDX contains 2 Python components; INFO log (via subprocess `RUST_LOG=info` capture) shows `lockfiles_discovered=1` with the `build-support/py.lock` path in the discovery trace.
- [X] T023 [P] [US3] Add integration test `us3_missing_pants_toml_falls_back_to_default_glob` to `waybill-cli/tests/pants_pex_reader.rs`. Uses the US1 fixture (`minimal_python/`) which has no `pants.toml`. Assert: exit 0; 3 components still discovered from default glob. Regression guard for FR-004's fallback contract.
- [X] T024 [P] [US3] Add integration test `us3_malformed_pants_toml_falls_back_gracefully` to `waybill-cli/tests/pants_pex_reader.rs`. Uses a new fixture `waybill-cli/tests/fixtures/pants_pex/malformed_pants_toml/`: `pants.toml` containing `not = valid = toml =` (garbage) + `3rdparty/python/default.lock` with 1 entry. Assert: exit 0 (no scan-abort); 1 component discovered from default glob; WARN in log naming `pants.toml`.

### Implementation for User Story 3

- [X] T025 [US3] T016 already implements the `pants.toml` discovery per contracts §"Discovery + orchestration data flow" step 1 + 2. Verify via T022/T023/T024 passing. If T022 fails: config path resolution is wrong — check that the `pants.toml`-declared path is resolved relative to `scan_root` (not CWD or the pants.toml's own dir). If T024 fails: `config::parse()` isn't swallowing errors correctly — revisit T006 return-type.

**Checkpoint**: T022 + T023 + T024 green.

---

## Phase 6: Edge cases + non-PyPI + corruption coverage (Cross-cutting)

**Purpose**: Cover the Edge Cases + Q2-A non-PyPI + SC-005 corruption behavior surface exhaustively.

- [X] T026 [P] Create synthetic fixture `waybill-cli/tests/fixtures/pants_pex/non_pypi_entries/3rdparty/python/default.lock` per research.md §R5 fixture 5: 4 lockfile entries — 1 PyPI-hosted (`waybill-fixture-normal`), 1 git-URL (`waybill-fixture-git` with `url: "git+https://example.test/waybill-fixture-git.git@abc123"`), 1 direct-URL (`waybill-fixture-url` with `url: "https://mirror.example.test/wheels/waybill_fixture_url-1.0.0-py3-none-any.whl"`), 1 file:// path (`waybill-fixture-local` with `url: "file:///opt/wheels/waybill_fixture_local-1.0.0.whl"`).
- [X] T027 [P] Add integration test `non_pypi_entries_emit_pkg_generic_with_source_annotations` to `waybill-cli/tests/pants_pex_reader.rs`. Uses T026 fixture. Assert: exit 0; 4 components total; `waybill-fixture-normal` is `pkg:pypi/*`; the other 3 are `pkg:generic/*` each with `waybill:source-url` + `waybill:source-type=git|url|local` annotations matching the fixture's URL prefix.
- [X] T028 [P] Create synthetic fixture `waybill-cli/tests/fixtures/pants_pex/corrupt_lockfile/3rdparty/python/default.lock` per research.md §R5 fixture 6: intentionally truncated JSON (opens with `{"pex_version": "2.10.0", "locked_resolves": [{"locked_req` — deliberately unterminated).
- [X] T029 [P] Add integration test `corrupt_lockfile_produces_warn_and_continues` to `waybill-cli/tests/pants_pex_reader.rs`. Uses T028 fixture. Assert: exit 0 (fail-open per SC-005); zero Python components emitted; subprocess stderr contains WARN with the corrupt file's path.
- [X] T030 [P] Add integration test `no_pants_no_lockfiles_byte_identical_output` to `waybill-cli/tests/pants_pex_reader.rs`. Uses a fixture that is 100% NOT a Pants repo (any existing non-Python fixture from `waybill-cli/tests/fixtures/` — reuse, don't create new). Assert: exit 0; SBOM output byte-identical to pre-feature-223 baseline. Regression guard for FR-007 / SC-003.

---

## Phase 7: Parity catalog + extractor entries (m071 gate)

**Purpose**: Add the required rows to `docs/reference/sbom-format-mapping.md` + matching extractors in `parity/extractors/mod.rs` per memory `feedback_sbom_format_mapping_extractor_gate`. Without this, the `every_catalog_row_has_an_extractor` + `holistic_parity` tests fail.

- [X] T031 Add 3 new rows to `docs/reference/sbom-format-mapping.md` — one per new `waybill:*` annotation key: `waybill:pants-resolve`, `waybill:source-url`, `waybill:source-type`. Assign row IDs following the existing sequential C-row convention (grep the doc for the highest current C-row ID + increment). Each row cites: description, CDX property path (`components[].properties[]`), SPDX 2.3 annotation shape, SPDX 3 annotation shape, and directionality (`SymmetricEqual` — all three formats emit identically).
- [X] T032 Add matching extractor entries to `waybill-cli/src/parity/extractors/mod.rs::EXTRACTORS` — one per row from T031, following the existing extractor pattern (grep for an existing single-value extractor like `extract_lifecycle_scope` for the template). Each extractor pulls the annotation value from CDX `components[].properties[]` / SPDX `annotations[]` and returns it for cross-format parity checks.
- [X] T033 Run `cargo +stable test -p waybill --test parity_holistic` (or the matching test binary — verify via `ls waybill-cli/tests/ | grep parity`) to confirm the parity gate passes. Failure signature: `every_catalog_row_has_an_extractor` or `holistic_parity` fail — root-cause via which specific row is missing its extractor.

---

## Phase 8: Docs + memory

- [X] T034 [P] Update `docs/ecosystems.md` (if it has a supported-ecosystems table per pre-spec Explore-agent survey) — add a row for "Pants (Python)" with lockfile-format `3rdparty/python/*.lock` (pex) + dep-graph support "Full via requires_dists". Cross-link to `specs/223-pants-pex-reader/quickstart.md`.
- [X] T035 [P] Update `README.md` supported-ecosystems section — add "Pants Python (pex-lockfile)" to the ecosystems table. Keep the addition minimal — one row.
- [X] T036 [P] Add memory entry `/Users/mlieberman/.claude/projects/-Users-mlieberman-Projects-mikebom/memory/reference_pants_pex_reader.md` documenting: the pex-lockfile format shape (link to research.md §R1), the dev-resolve allowlist (link to §R2), the m071 parity-extractor gate implications, and the follow-up milestones (coursier + BUILD file + eBPF trace). Add corresponding line to `MEMORY.md` index.

---

## Phase 9: Pre-PR gate

- [X] T037 Run `./scripts/pre-pr.sh`. Confirm: (a) `cargo +stable clippy --workspace --all-targets -- -D warnings` exit 0, zero warnings; (b) `cargo +stable test --workspace --no-fail-fast` — every suite reports `ok. N passed; 0 failed`. Report per-target counts per memory `feedback_prepr_gate_full_output`.
- [X] T038 Verify no unintended goldens changed: `git status waybill-cli/tests/fixtures/` MUST show only additions under `pants_pex/` — no modifications to any existing golden. Any modification indicates a leaked side-effect on other readers.
- [X] T039 Run `cargo tree -p waybill --target x86_64-unknown-linux-gnu -e normal | grep -Ei 'openssl-sys|libz-sys|aws-lc-rs|aws-lc-sys|native-tls|mbedtls-sys|tough'` — expect zero output (Constitution Principle I regression guard).
- [X] T040 Locally walk `specs/223-pants-pex-reader/quickstart.md` §1 and §2 end-to-end against a real Pants Python repo (or the largest fixture from Phase 3 as substitute). Confirm the FR-010 INFO log line appears with the expected structured fields.

---

## Dependencies & Execution Order

### Phase dependencies

- **Setup (Phase 1)**: T001 → T002 → T003. T003 is `[P]` relative to T001/T002 only if T003's grep confirms no shared-file conflict (`pip/` directory work); otherwise sequential.
- **Foundational (Phase 2)**: T004–T007 all `[P]` w.r.t. each other (4 different files: lockfile.rs, config.rs, resolve_classifier.rs). All 4 must complete before US1 starts.
- **US1 (Phase 3)**: T008–T013 `[P]` (fixture creation + test scaffolding — different files). T014 → T015 (T015 consumes T014's types + T003's helper + T005's dispatcher + T007's classifier). T016 depends on T014 + T015. T017 depends on T016. Tests T011/T012/T013/T017a depend on T017 (need the reader wired) + their respective fixtures (T008/T009).
- **US2 (Phase 4)**: T018 + T019 `[P]`. T020 is a verification step — no code required in the happy case.
- **US3 (Phase 5)**: T021 + T022 + T023 + T024 all `[P]` (different fixtures + different tests). T025 verification only.
- **Phase 6 (edge cases)**: T026 + T027 + T028 + T029 + T030 all `[P]` (different files, no ordering).
- **Phase 7 (parity)**: T031 → T032 → T033 sequential (each depends on the prior).
- **Phase 8 (docs)**: T034 + T035 + T036 all `[P]`.
- **Phase 9 (pre-PR gate)**: T037 → T038 → T039 → T040 sequential.

### Story dependencies (visualized)

```text
Phase 1 (Setup) ──> Phase 2 (Foundational) ──> Phase 3 (US1 MVP) ──> Phase 6 (edge cases)
                                                    │                     │
                                                    ├──> Phase 4 (US2)    │
                                                    └──> Phase 5 (US3)    │
                                                                          │
                                          Phase 7 (parity) <──────────────┤
                                                                          │
                                          Phase 8 (docs)   <──────────────┤
                                                                          │
                                          Phase 9 (pre-PR gate) <─────────┘
```

### Parallel opportunities

- Phase 2: T004 + T005 + T006 + T007 in parallel (4 different files).
- Phase 3 (setup half): T008 + T009 + T010 in parallel.
- Phase 6: all 5 tasks in parallel.
- Phase 8: all 3 tasks in parallel.

---

## Implementation Strategy

### MVP first (US1 only)

1. Complete Phase 1 (Setup — module skeleton + shared helper).
2. Complete Phase 2 (types + config + classifier).
3. Complete Phase 3 (US1 — reader + orchestrator + 3 integration tests).
4. **STOP + VALIDATE**: `cargo +stable test -p waybill --test pants_pex_reader us1_` should be all-green.
5. Deploy / demo the MVP: `waybill sbom scan` now covers Pants Python repos with default lockfile layout.

### Incremental delivery after MVP

- Add Phase 4 (US2 dedup) — 1 fixture + 1 test + verification only.
- Add Phase 5 (US3 pants.toml discovery) — 1 fixture + 3 tests + verification only.
- Add Phase 6 (edge-case coverage) — hardens SC-005 fail-open behavior.
- Add Phase 7 (parity catalog + extractors) — REQUIRED before PR (m071 gate).
- Add Phase 8 (docs polish).
- Add Phase 9 (pre-PR gate).

Estimated total effort: **~2 focused work-days** — 1 day for MVP through US1 checkpoint, 1 day for US2/US3/edge/parity/docs/gate.

### Rollback strategy

Each phase is independently revertible via `git checkout HEAD -- <phase-files>`. The reader is opt-in (activates only when Pex lockfiles are present); no CLI surface changes; no existing goldens modified per SC-003. If Phase 3 lands but Phase 7 (parity) doesn't complete before merge deadline, ship without T031–T033 and file a follow-up — the reader works standalone but the parity gate stays broken until closed.

---

## Notes

- All fixtures use synthetic package names (`waybill-fixture-*`) per memory `feedback_fixture_synthetic_package_names`. Never real PyPI coordinates.
- `[P]` tasks touch different files with no ordering dependency.
- Every US phase is independently shippable: US1 alone is a valid MVP; US2 alone is a bugfix ("dedup broken with lockfile"); US3 alone is a bugfix ("custom lockfile path missed").
- Phase 7 (parity catalog) is a hard requirement pre-merge — the m071 test gate will otherwise fail CI. Never skip.
- Test framework: `cargo test` (existing pattern). No new test frameworks. Fixture files < 1 KB each; total addition < 20 KB.
- Estimated production LOC: ~500 total (T004+T005+T006+T007 ≈ 200 LOC; T014+T015 ≈ 200 LOC; T016+T017 ≈ 100 LOC). Test LOC: ~320 (T017a adds ~20 LOC). Fixture LOC (JSON + TOML): ~200.
