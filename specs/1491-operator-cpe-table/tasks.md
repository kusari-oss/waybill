# Tasks: Operator-supplied CPEs, and the curated CPE table as data

**Input**: Design documents from `specs/1491-operator-cpe-table/`.
**Prerequisites**: plan.md, spec.md, research.md (R1–R10), data-model.md, contracts/cli.md, contracts/wire.md, quickstart.md.

**Tests**: Requested. SC-007 asks for a mutation-checked test per rule, which is the project's practice (constitution VII and IX).

**Paths**: Code is under `waybill-cli/src/` unless stated. Tests are in `waybill-cli/tests/`.

**Delivery** (FR-017): two stacked PRs.
- **PR A** is Phases 1–3 (US4).
- **PR B** is Phases 4–8 (US1–US3). It starts by merging PR A's exact head.

Story order follows delivery, not priority. US4 (P2) ships first, because US1–US3 reuse its schema and parser.

## Format: `[ID] [P?] [Story] Description`

---

# PR A — the curated table as data

## Phase 1: Setup

- [X] T001 Record the branch base, `git rev-parse HEAD`, in `specs/1491-operator-cpe-table/measurements/base.txt`. Re-run `specs/1491-operator-cpe-table/measurements/evidence-attribution.py`, and confirm the output is byte-identical to `evidence-attribution-2026-11-13.tsv` with summary `with_exact_evidence=93 without=2` (research R1).

## Phase 2: Foundational — schema and parser (blocks US4 and PR B)

- [X] T002 Create `generate/cpe_table.rs` with the data-model types. Register it in `generate/mod.rs`.
  - Types: `CpeTable { row: Vec<CpeRow> }`, `CpeRow { key, pair: Vec<CpePair> }` and `CpePair { vendor, product, dictionary_entries: Option<u32>, observed: Option<String>, dictionary_evidence: Option<String>, title_evidence: Option<String> }`.
  - Use `#[serde(deny_unknown_fields)]`.
  - Add `pub(crate) fn parse(text: &str, origin: Origin) -> Result<CpeTable, CpeTableError>`, where `Origin` is `BuiltIn` or `Operator(PathBuf)`. It validates:
    - TOML syntax;
    - unique keys;
    - each key is a versionless PURL, parsed with `Purl::new`, and rejected if it has a version, qualifiers or subpath;
    - each pair forms a valid `Cpe` via `format_cpe(vendor, product, "1.0")`;
    - for `BuiltIn` only, all four evidence fields are present.
  - `CpeTableError` names the row index, the key and the reason, and its message is the one `contracts/cli.md` specifies.
- [X] T003 Add unit tests in `generate/cpe_table.rs`, under `#[cfg(test)] #[cfg_attr(test, allow(clippy::unwrap_used))]`:
  - valid built-in and operator documents parse;
  - each validation failure in data-model § "Validation failures" is rejected, with the row named: an unknown field, a duplicate key, a versioned key, a qualifier key, a wildcard vendor, an empty product, and a missing evidence field in a built-in row;
  - an operator row without evidence fields is accepted.

## Phase 3: User Story 4 — a maintainer edits the table as data (Priority: P2, PR A)

**Goal**: The 82 rows and 95 observations live in `generate/cpe_table.toml`. The tests read that file, and output does not change.

**Independent Test**: Every corpus golden is byte-identical (FR-014). Deleting an observation, or breaking an evidence path, fails a named test.

### Tests

- [X] T004 [P] [US4] In `generate/cpe.rs` tests, rewrite the table tests so they read the parsed built-in table. Keep each test's control assertions.
  - The tests: `mappings_alphabetically_sorted` (by key), `mappings_all_emit_valid_cpe23`, `mappings_cover_all_curated_libraries` (`OMITTED` stays a test constant), `m1375_no_duplicate_slug`, `every_table_pair_exists_in_the_nvd_dictionary` and `no_dictionary_observation_is_stale`.
  - `m1375_every_product_has_a_committed_title_query` reads each pair's `title_evidence` path, relative to `CARGO_MANIFEST_DIR/..`, and drops the six-file `concat!` (research R2).
- [X] T005 [P] [US4] Add three tests in `generate/cpe.rs`.
  - `m1491_every_evidence_path_exists`: for every pair, both evidence files exist.
  - `m1491_every_count_matches_its_evidence`:
    - every `dictionary_entries` is greater than 0;
    - it equals the `dictionary-entries` column of the census row matching `product` and `vendor`, read by header name, skipping `#` lines (research R1, "How to read the census");
    - **control:** the census parses to at least 95 rows, and `bash`/`gnu` is found.
  - `m1491_shared_pairs_agree`: `python:python`, the only pair two rows share, carries identical evidence in both. (`nodejs:node.js` was listed here in error: it appears once.)
- [X] T006 [P] [US4] Add `m1491_the_table_carries_no_vulnerability_data` in `generate/cpe.rs`. The built-in TOML has no key matching `/cve|advis|vuln/i`. Comment lines are excluded (research R10).
- [X] T007 [P] [US4] Add `the_builtin_table_parses` in `generate/cpe_table.rs`. The embedded file parses with `Origin::BuiltIn`, and has 82 rows and 96 pair occurrences. (The plan said 97; that count included a pair quoted in a comment. Only `python:python` is shared.)

### Implementation

- [X] T008 [US4] Generate `generate/cpe_table.toml` from the current `GENERIC_LIBRARY_CPES` and `NVD_DICTIONARY_OBSERVED`.
  - **Rows:** one `[[row]]` per slug, `key = "pkg:generic/<slug>"`, pairs in the existing order.
  - **Dictionary evidence:** `dictionary_evidence = "specs/1375-generic-cpe-table/measurements/nvd-vendor-cves-2026-11-13.tsv"` and `observed = "2026-11-13"` for every pair.
  - **Counts:** from `NVD_DICTIONARY_OBSERVED`, except `openssl:openssl` 387 and `libssh:libssh` 58 (R1).
  - **Title evidence:** the one of the six title files that contains `=== <product> (`. If more than one does, take the latest dated.
  - **Comments:** carry every row's rationale comment from `cpe.rs` over as TOML comments above the row. That includes the #1490 multi-vendor note and the brotli and bzip2 notes.
  - Write the generator to `specs/1491-operator-cpe-table/measurements/generate-cpe-table.py` (Python 3 stdlib, run as `python3 -I -B`) and commit it, so the conversion can be reproduced.
- [X] T009 [US4] In `generate/cpe_table.rs`, embed the file with `include_str!("cpe_table.toml")` and parse it once into a `std::sync::OnceLock`.
  - Provide a `HashMap<&str, &[pair]>` lookup by slug (the key's name segment).
  - On a parse error, log ERROR once and return an empty table. Do not `unwrap` (R3).
- [X] T010 [US4] In `generate/cpe.rs`:
  - delete `GENERIC_LIBRARY_CPES` and `NVD_DICTIONARY_OBSERVED`;
  - make the `"generic"` arm look up `component.name` in the built-in table;
  - keep the `openjdk` truncation and every other special case in code (R4).
  - Move each table-header doc comment that explains a rule to the module doc. Those are the boringssl rationale and the dictionary-versus-advisory note. Leave the per-row rationale in the TOML.
- [X] T011 [US4] **As built:** `scripts/probe-cpe-dictionary.sh` reads the TOML's pair lines by pattern, because the local `python3` has no `tomllib` (it needs 3.11). The pair set it yields equals the old table's: 95 pairs, and all 82 rows keep their order. Original task text: Update `secondary_cpes` and every other reader of the deleted constants. Find them with `git grep -n 'GENERIC_LIBRARY_CPES\|NVD_DICTIONARY_OBSERVED' -- waybill-cli scripts docs`, read the full output, and leave zero hits outside `specs/`. Update `scripts/probe-cpe-dictionary.sh`'s header and the pair source it reads, so it points at the TOML.
- [X] T012 [US4] Mutation checks. **Observed** (failed tests per mutation): swap vendor → 2 (`m1375_the_vendor_is_nvds…`, `m1491_every_count_matches…`); drop entries → 18 (built-in fails to parse, so the table is empty); missing evidence → 2; wrong count → 1; `cves = 1` → 19, including `m1491_the_table_carries_no_vulnerability_data`; out of order → 1. All reverted with `cp`, and the file is byte-identical afterwards.
  Original task text: Each must fail a named test; revert each with `cp`, never `git checkout`. Record the results in the commit message.
  1. Swap one row's primary vendor.
  2. Delete one pair's `dictionary_entries`.
  3. Point one `dictionary_evidence` at a missing file.
  4. Set one count to a value the census does not hold.
  5. Add `cves = 1` to a row.
  6. Move a row out of order.
- [X] T013 [US4] Wording-only updates:
  - `docs/reference/sbom-format-mapping.md`: C209's note and any line naming the deleted constants now say where the curated table lives;
  - a CHANGELOG `Changed` entry: "the curated CPE table is data".
- [X] T014 [US4] Run the gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`. Record `GATE_EXIT` and the totals.
- [X] T015 **Observed:** corpus run 43505314970 at `9a87a623`: 158 passed, 0 failed, 21 targets. PR A is #1505. [US4] Dispatch the non-regenerating corpus check:

  ```
  gh workflow run public-corpus.yml --ref 1491-operator-cpe-table -f branch=1491-operator-cpe-table -f regen_goldens=false
  ```

  Confirm every target is identical, by count of targets and not by job colour (FR-014, SC-005). Open PR A with the AI-drafting footer, no tool comparisons and no company names.

**Checkpoint, PR A**: the gate is green, the corpus is identical, and the mutations are recorded.

---

# PR B — the operator surfaces

## Phase 4: Foundational for PR B (blocks US1–US3)

- [X] T016 Create branch `1491-operator-cpes` from PR A's head (FR-017). If PR A moves afterwards, merge its new exact head into PR B. Never rebase.
- [X] T017 Create `generate/cpe_source.rs`:
  - move `CpeSource` and `cpe_source` from `scan_fs/package_db/nix/closure/identifiers.rs`, and add `Operator(OperatorSurface { RootCpe, Supplement, CpeMap })`;
  - add the internal marker key constant `ANN_OPERATOR_CPE = "waybill:operator-cpe"`;
  - add the six comment and method strings from `contracts/wire.md`;
  - resolution order: marker, then C212 with a `drv`/`name-version` basis, then the curated marker.

  Re-export from `identifiers.rs`, so the existing call sites compile unchanged. Update the three emitter call sites (`cyclonedx/evidence.rs:241`, `spdx/packages.rs:454,463`, `spdx/v3_external_ids.rs:78`) to use the operator method and comment strings.
- [X] T018 Register `waybill:operator-cpe` in `generate/root_selector.rs::is_internal_emission_key`, with a unit test beside the existing ones.
- [X] T019 Extend C212's value builder in `identifiers.rs` (`provenance`, `provenance_basis`) with the basis values `purl`, `cpe-map` and `root-cpe`. Unit-test that each round-trips, and that the existing `drv`/`name-version` output is byte-identical.
- [X] T020 Make the later passes in `cli/scan_cmd.rs` operator-aware (R5):
  - the m1377 `replaceable` check (~line 7536) treats a marked component as not replaceable, instead of reading `waybill:source-tier`;
  - `record_closure_cpe_sources` skips marked components.

  Unit-test both beside the existing m1377 FR-013 tests (~line 9679).

**Checkpoint**: compiles. With no operator input, output is unchanged: the gate and the in-repo goldens show it.

## Phase 5: User Story 2 — an operator's CPE beats a synthesized guess (Priority: P1)

**Goal**: A supplement CPE replaces the synthesized one (FR-003).

**Independent Test**: A supplemented `deb` component emits the operator's CPE, with `overrode` (SC-002).

### Tests

- [X] T021 [P] [US2] Unit tests in `supplement/conflict.rs`:
  - a collision with synthesized CPEs → the operator's list wins, the conflict record is kept with developer-wins, no `waybill:declared-cpe` is written, the marker is set, and C212 is `{"basis":"purl","overrode":...}`;
  - an identical CPE → no `overrode`, and the marker is still set;
  - a malformed supplement CPE → dropped with a WARN, and the scanner's CPEs are kept;
  - a supplement CPE whose version differs from the component's → emitted as given, with a WARN naming the component (spec edge case).

  Update the existing tests that assert scanner-wins for `cpe`. List each one in the commit message.
- [X] T022 **As built:** the `pkg:generic` curated case uses milestone 1377's closure fixture (`openssl`), because no non-closure fixture yields a curated `pkg:generic` component. The deb case uses a dpkg root; pypi is not separately exercised, because the collision path is ecosystem-agnostic and the deb, cargo and generic cases cover it. [P] [US2] Integration tests in the new file `waybill-cli/tests/operator_cpe.rs`, using `supplement_cdx_integration.rs`'s harness and synthetic fixtures. Cover a supplemented `deb`, `pypi` and curated `pkg:generic` component, checked in all three formats:
  - the primary CPE;
  - the method and comment `CPE supplied by the operator with --supplement-cdx`;
  - C212;
  - C212 parity across the three formats.
- [X] T023 **As built:** this lives in `tests/operator_cpe.rs` (`s1491_us2_a_supplement_cpe_replaces_a_nixpkgs_declared_one`). It exposed a gap: a supplement lands before the closure, so `overrode` was never recorded. `note_displaced` at the closure merge fixes it. Milestone 1377's `an_operator_supplied_cpe_carries_no_closure_provenance` and `m1377_an_operator_cpe_equal_to_the_curated_one_is_kept` now assert the spec 1491 contract: operator provenance, plus C212 with `overrode`. [P] [US2] Closure case in `waybill-cli/tests/closure_declared_cpe.rs`: a supplement CPE on a member with a nixpkgs-declared CPE → the operator's wins, and C212's `overrode` is the declared CPE (US2 scenario 2).

### Implementation

- [X] T024 [US2] In `supplement/conflict.rs`:
  - move `ConflictField::Cpe` to developer-wins, and update the module header's partition comment;
  - on a CPE collision, validate with `Cpe::parse` (WARN and drop on error), then replace `scanner.cpes`, set the marker, and set C212.
- [X] T025 [US2] In `supplement/merge.rs::synthesize_resolved`: solo entries with valid CPEs get the marker and C212 `{"basis":"purl"}`.
- [X] T026 [US2] In `supplement/parser.rs`: when `metadata.component` carries `cpe`, log a WARN that it is ignored and names `--root-cpe` (contracts/cli.md).

## Phase 6: User Story 1 — the operator names the subject's CPE (Priority: P1)

**Goal**: `--root-cpe` sets the subject's CPEs in all three formats (FR-001, FR-002).

**Independent Test**: Under SC-001, the subject carries the CPE in 3 of 3 formats; without the flag, output is unchanged.

### Tests

- [X] T027 [P] [US1] Tests in `waybill-cli/tests/operator_cpe.rs`:
  - a synthetic subject (`--root-name`) gets the primary, plus the secondary from a second flag, with the root-cpe method and comment in all three formats and no C212;
  - a main-module subject gets the operator CPE, the displaced primary as C212 `overrode` with basis `root-cpe`, and C212 parity, including CycloneDX `metadata.component` (R7);
  - a malformed or wildcard value exits non-zero with no output file;
  - with `--split`, clap rejects the run;
  - a `--root-cpe` version that differs from the main-module subject's version → emitted, with a WARN.

### Implementation

- [X] T028 [US1] In `cli/scan_cmd.rs`, add `--root-cpe` (`ArgAction::Append`, `conflicts_with = "split"`). Parse each value with `Cpe::parse` before scanning, and turn a failure into an `anyhow` usage error. Thread the values into `RootComponentOverride.cpes` in `generate/mod.rs`.
- [X] T029 **As built:** `apply_root_cpes` in `scan_cmd.rs` selects the subject. A main module gets the CPEs on its component; an override subject passes them to the synthetic-root builders. A placeholder or Maven-coordinate subject is a usage error, and FR-001 is amended to say so: on a placeholder scan SPDX 2.3 roots at the single top-level component, so the CPE would land on a dependency. [US1] In `generate/mod.rs`, add `subject_cpes(main_module, &RootComponentOverride) -> (Vec<Cpe>, Option<Cpe>)`, returning the CPEs and `overrode`. Apply it once:
  - **main-module subject:** replace the component's CPEs before emission, and set the marker and C212;
  - **synthetic subject:** used by `cyclonedx/metadata.rs:~1307`, `spdx/document.rs:~1037` and `spdx/v3_document.rs:~1062`, which emit the primary, the secondaries and the operator comment in each format's existing CPE carriers.
- [X] T030 **As built:** the CycloneDX parity extractor already walks `metadata.component`; what was missing was the property itself. It is propagated for operator-sourced C212 only. A closure subject's nixpkgs C212 already reaches CycloneDX through its `components[]` twin, and copying it would change closure goldens (FR-016). [US1] In `cyclonedx/metadata.rs`, propagate C212 to `metadata.component.properties` for a main-module subject, beside `waybill:package-shape`. If T027's parity case still fails, fix `parity/extractors/cdx.rs::cdx_property_values` to read `metadata.component`, and record which of the two fixed it (R7).

## Phase 7: User Story 3 — the operator extends the table for their estate (Priority: P2)

**Goal**: `--cpe-map` applies versionless-PURL rows at every version (FR-004, FR-005).

**Independent Test**: Under SC-003, two versions of one package both carry the mapped CPE.

### Tests

- [X] T031 [P] [US3] Tests in `waybill-cli/tests/operator_cpe.rs`:
  - one row matching a package at two versions;
  - a row that overrides a built-in `pkg:generic` row, with `overrode`;
  - a row for `pkg:generic/boringssl`, a name the built-in table deliberately omits → used;
  - a map row and a supplement CPE on one component → the supplement wins;
  - a map row over a nixpkgs-declared CPE → the map wins, with `overrode`;
  - an unused row → no output change, and the INFO line is logged;
  - every malformed-file case → a non-zero exit with no output (US3 scenario 4).

  Each checks all three formats, the `--cpe-map` comment, and C212 basis `cpe-map`.

### Implementation

- [X] T032 [US3] In `cli/scan_cmd.rs`, add `--cpe-map <PATH>`. Read and parse it with `cpe_table::parse(_, Origin::Operator(path))` before scanning, and turn an error into a usage error.
- [X] T033 [US3] In `cli/scan_cmd.rs`, add `apply_cpe_map(&mut components, &CpeTable)`, called right after `fill_missing_cpes` (~line 5129) and before `record_closure_cpe_sources`.
  - It skips marked components, and components with an empty version.
  - A component matches when its PURL type, namespace and name equal a key.
  - On a match, it builds the list with `format_cpe` from the component's version, records `overrode` against the current primary, and sets the marker and C212 `cpe-map`.
  - It emits one INFO summary: rows, rows matched, components changed, unused rows.

## Phase 8: Polish (PR B)

- [X] T034 [P] Widen the C212 row in `docs/reference/sbom-format-mapping.md` per `contracts/wire.md`. Add `--root-cpe` and `--cpe-map` to `docs/user-guide/cli-reference.md` and `docs/reference/identifiers.md`, alongside the root-override flags. Add a CHANGELOG entry, kept non-adjacent to PR A's.
- [X] T035 **Observed** (failed tests per mutation): scanner wins `cpe` 5; no marker check 3; map beats supplement 1; no `overrode` 6; Operator below Curated 3; wildcard `--root-cpe` 1; versioned map key 2. Each reverted with `cp` and verified with `cmp`. Mutation checks. Each must fail a named test; revert each with `cp` and record it in the commit message.
  1. Restore scanner-wins for `cpe`.
  2. Drop the marker check in the m1377 `replaceable` check.
  3. Make `apply_cpe_map` run before the supplement check.
  4. Drop the `overrode` computation.
  5. Swap `Operator` below `Curated` in `cpe_source`.
  6. Accept a wildcard vendor in `--root-cpe`.
  7. Accept a versioned key in `--cpe-map`.
- [ ] T036 Run the gate, then the non-regenerating corpus check at PR B's head. Every target must be identical (FR-016). Open PR B with the footer, recording the gate totals and the corpus run ID.

---

## Dependencies & order

- **PR A:** T001 → T002 → T003. Then T004–T007 [P] alongside T008. Then T009 → T010 → T011 → T012 → T013 → T014 → T015.
- **PR B** starts from PR A's head (T016). T017 → T018 → T019 → T020 block every PR B story.
- **Stories in PR B:**
  - US2 comes first: the marker semantics are proven on the existing surface.
  - US1 is independent of US2 apart from Phase 4.
  - US3 depends on Phase 4 and on PR A's parser.
  - US1 and US3 both edit `scan_cmd.rs`, so they run in sequence.
- **Polish** comes last.

## Parallel opportunities

- T004, T005, T006, T007: separate test functions, written together.
- T021, T022, T023: three different test files.
- T027 and T031 add to the same new test file. Write them in sequence, but run them together.

## Implementation strategy

- **PR A is the MVP**, and can ship alone. The goldens show no change, and the table becomes editable data.
- **PR B's MVP is US2:** the defect fix on an existing surface. US1 and US3 then add the two new surfaces.
- **Golden churn:** none in either PR. The corpus check runs non-regenerating at each head as the proof.
