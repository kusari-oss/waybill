---

description: "Task list for milestone 131 — Quality metadata backfill (PE/CLR Phase B + license backfill + supplier URLs)"

---

# Tasks: Quality metadata backfill (milestone 131)

**Input**: Design documents from `/specs/131-quality-metadata-backfill/`
**Prerequisites**: plan.md, spec.md, research.md, contracts/annotation-schema.md, quickstart.md

**Tests**: REQUIRED. The spec's user story acceptance scenarios are Given/When/Then; SC-001..006
are verified end-to-end via the audit-image scorecard comparison. Inline unit tests for new helpers.

**Organization**: Three sequential PRs per milestone-130 cadence — US1 → US2 → US3. US1 is the
largest single piece (~300 LOC ECMA-335 CustomAttribute walking); US2 + US3 are ~150 LOC each.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to ([US1], [US2], [US3])

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Confirm milestone 130 fully landed on main; baseline existing tests.

- [ ] T001 Run `git rev-parse --abbrev-ref HEAD` and confirm `131-quality-metadata-backfill`. Run `git log -5 --oneline main` and confirm the three milestone-130 PRs (cargo-auditable gate fix #371, maven nested-JAR #372, PE/CLR US3 Phase A #373) are all merged.
- [ ] T002 [P] Run `cargo +stable build -p mikebom` baseline. Confirm clean.
- [ ] T003 [P] Run `cargo +stable test -p mikebom --bin mikebom nuget::pe_clr maven::tests cargo_auditable` and confirm milestone-130 tests pass.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: None. All three user stories are independent and parallelizable.

**Checkpoint**: User story implementation can begin in parallel.

---

## Phase 3: User Story 1 — PE/CLR Phase B `CustomAttribute` walking (Priority: P1) 🎯 MVP

**Goal**: Walk the CustomAttribute table (token 0x0C) in `pe_clr.rs` to extract
`AssemblyInformationalVersion` + `AssemblyFileVersion` strings. PURL version routes through the
milestone-129 Q3 ladder. Resolves all 373 VERSION_MISMATCH cases on the audit image.

**Independent Test**: Run `mikebom sbom scan --image mcr.microsoft.com/dotnet/runtime:8.0-alpine`;
confirm `Microsoft.AspNetCore` component emits with `@8.0.27-servicing.26230.7` (InformationalVersion)
instead of `@8.0.0.0` (AssemblyVersion 4-tuple).

### Data model

- [ ] T004 [US1] In `mikebom-cli/src/scan_fs/package_db/nuget/pe_clr.rs`, extend the `ManagedAssembly` struct with two new fields per data-model.md: `informational_version: Option<String>` and `file_version: Option<String>`. Defaults to `None`. Update the `Debug, Clone` derivations to cover them.

### CustomAttribute parser

- [ ] T005 [US1] Add a helper `compute_row_size_extended(token, widths, rows)` that extends the existing milestone-130 `compute_row_size` to include `CustomAttribute` (token 0x0C). The CustomAttribute row layout: `Parent` (HasCustomAttribute coded index, 5-bit tag, 21 referenced tables) + `Type` (CustomAttributeType coded index, 3-bit tag, 5 referenced tables) + `Value` (#Blob index). Width derivation per research R3.
- [ ] T006 [US1] Add helper `parse_compressed_int(bytes: &[u8], offset: usize) -> Option<(u32, usize)>` decoding the ECMA-335 §II.24.2.4 compressed integer format: high bit 0 = 1-byte (length <128); high two bits 10 = 2-byte (length <16384); high three bits 110 = 4-byte. Returns the decoded value + bytes consumed.
- [ ] T007 [US1] Add helper `decode_serstring(blob: &[u8], offset: usize) -> Option<(Option<String>, usize)>` per research R2: read the compressed-int length; `0xFF` byte → null string (returns `(None, 1)`); otherwise read `length` UTF-8 bytes into a `String`. Returns `(Some(s), consumed)` or `(None, 1)` for null.
- [ ] T008 [US1] Add helper `read_typeref_name(tables: &[u8], headers: &TableHeaders, strings: &[u8], row_index: u32) -> Option<&str>` that reads TypeRef table row `row_index`'s `TypeName` column (`#Strings` index) and resolves to the heap-stored type name. Use existing milestone-130 `read_string_heap`.
- [ ] T009 [US1] Add helper `resolve_customattribute_type_name(row_index: u32, tag: u8, tables, headers, strings) -> Option<String>` that, given a CustomAttributeType coded index (tag bits + table-row bits), dispatches to MemberRef (tag=3) or MethodDef (tag=2). For MemberRef: read the row's `Class` column (a MemberRefParent coded index — usually TypeRef tag=1), resolve through to TypeRef, return TypeName. For MethodDef: return `None` (defining assembly's own attribute, not relevant).
- [ ] T010 [US1] Add helper `walk_custom_attributes(tables, headers, strings, blob_heap) -> Vec<(String, Option<String>)>` that walks every CustomAttribute table row. For each: decode `Parent` (HasCustomAttribute coded index — filter to rows where the parent is the Assembly table); resolve `Type` via `resolve_customattribute_type_name`; if the resolved type name is `"AssemblyInformationalVersionAttribute"` or `"AssemblyFileVersionAttribute"`, decode the `Value` blob (compressed-int length-prefixed, prolog `01 00`, then `decode_serstring` for the string argument). Returns a vector of `(type_name, version_string)` tuples.
- [ ] T011 [US1] In `parse_tables_stream`, after the existing Assembly row 0 extraction, call `walk_custom_attributes`. Populate the new `ManagedAssembly.informational_version` / `.file_version` fields based on matched rows. Multi-row Assembly tables (rare) — first match wins.

### PURL version routing

- [ ] T012 [US1] Add method `ManagedAssembly::purl_version_with_ladder()` that returns the version string per FR-008 ladder: `informational_version.clone().or_else(|| file_version.clone()).unwrap_or_else(|| self.version.to_string())`. Replace the existing `version.to_string()` call site in `AssemblyAccumulator::absorb` to use the new method.
- [ ] T013 [US1] Per FR-009, the `Purl::new` constructor handles version percent-encoding (e.g. `+` → `%2B`) automatically — verify via a unit test that builds a PURL from an `InformationalVersion` containing `+`. Document in the test that the existing constructor's behavior satisfies FR-009.
- [ ] T014 [US1] In `AssemblyAccumulator::flatten`, populate the `mikebom:assembly-version-informational` and `mikebom:assembly-version-file` annotations from the accumulated representative's new fields, when present. The existing `mikebom:assembly-version-runtime` (4-tuple) emission stays unchanged per FR-010.

### Unit tests

- [ ] T015 [US1] Add unit tests inside `pe_clr.rs`: (a) `parse_compressed_int` for 1/2/4-byte cases + invalid prefix; (b) `decode_serstring` for short string, null (0xFF), and empty; (c) `purl_version_with_ladder` for all three rungs (Informational present, only File, only 4-tuple); (d) PURL construction with a `+` in the version round-trips correctly.

### US1 verification

- [ ] T016 [US1] Run `cargo +stable test -p mikebom --bin mikebom nuget::pe_clr` and confirm all new + existing tests pass.
- [ ] T017 [US1] Build release; scan the audit image with US1 only; compare via `sbom-comparison --format summary /tmp/us1.cdx.json ~/Downloads/remediation-planner-syft-image-sbom.json`. Assert VERSION_MISMATCH count drops from 373 to <20 (SC-002).

**Checkpoint**: US1 fully functional. SC-002 verifiable.

---

## Phase 4: User Story 2 — License coverage backfill (Priority: P2)

**Goal**: Add license metadata to the three new reader paths from milestone 130.

**Independent Test**: After US2, run `sbom-comparison --format summary` on the audit image; assert
License Coverage moves from 1/5 to ≥3/5.

### US2a — PE/CLR LICENSE.txt probe

- [ ] T018 [US2] In `mikebom-cli/src/scan_fs/package_db/nuget/pe_clr.rs`, add a helper `probe_license_file(dll_path: &Path, max_depth: u8) -> Option<(Vec<u8>, PathBuf)>` per research R4. Walks up to 3 levels above `dll_path`'s parent, looking for case-insensitive `LICENSE`, `LICENSE.txt`, `LICENSE.md`, `COPYING`, `COPYING.txt`. Returns the first match's first 4 KB of bytes + the file path. Returns `None` if no match.
- [ ] T019 [US2] Add a helper `fingerprint_license(bytes: &[u8]) -> Option<&'static str>` per FR-013 that matches the first 4 KB against canonical opening-text patterns of common SPDX licenses: `"Apache License"` → `"Apache-2.0"`; `"MIT License"` OR `"Permission is hereby granted, free of charge"` → `"MIT"`; `"BSD 3-Clause"` OR (`"Redistribution"` AND 3-clause text) → `"BSD-3-Clause"`; `"BSD 2-Clause"` → `"BSD-2-Clause"`; `"GNU General Public License"` + `"version 3"` → `"GPL-3.0"`; same + `"version 2"` → `"GPL-2.0"`. Returns `None` for non-matching texts (signals the C97 fallback path).
- [ ] T020 [US2] In the `AssemblyAccumulator::absorb` call site (`read()` function), after a successful managed-assembly parse, call `probe_license_file(path, 3)`. When `Some((bytes, file_path))`: call `fingerprint_license(&bytes)`. If `Some(spdx_id)`: set `PackageDbEntry.licenses` to a one-element `Vec<SpdxExpression>` via `SpdxExpression::try_canonical(spdx_id)`; emit `mikebom:license-source = "package-dir"`. If `None`: compute `sha256(&bytes)`, emit `mikebom:license-source = "package-dir-unrecognized"` + `mikebom:license-text-sha256 = <hex>` (C97). When the outer `probe_license_file` returns `None`: emit `mikebom:license-source = "package-dir-no-license"` per FR-015. Plumb the result through the accumulator's emission path (add `license_id: Option<&'static str>` + `license_sha256: Option<String>` fields to `AccumulatedAssembly`).

### US2b — Nested-JAR license propagation

- [ ] T021 [US2] In `mikebom-cli/src/scan_fs/package_db/maven.rs`, locate the nested-walker's emission site at `extract_nested_meta`. The existing top-level path uses `parse_pom_xml(pom_xml_bytes).licenses` — currently this output is discarded in the nested path. Add a step that extracts `parse_pom_xml(bytes).licenses` for each nested entry's `pom.xml`, canonicalizes each via `SpdxExpression::try_canonical`, and serializes the resulting `Vec<SpdxExpression>` as a JSON value under the existing `EmbeddedMavenMeta.extra_annotations["mikebom:nested-licenses"]` key (consistent with the milestone-130 `extra_annotations` plumbing pattern — no new struct field on `EmbeddedMavenMeta`).
- [ ] T022 [US2] Update `jar_pom_to_entry` to consume the new `mikebom:nested-licenses` annotation when present, deserialize it back to `Vec<SpdxExpression>`, and populate `PackageDbEntry.licenses` accordingly. Add `mikebom:license-source = "pom-xml"` annotation when at least one license was extracted.

### US2c — cargo-auditable registry-required annotation

- [ ] T023 [US2] In `mikebom-cli/src/scan_fs/binary/entry.rs::cargo_auditable_packages_to_entries`, for each `packages[]` entry: check the `source` field. When `source == "crates-io"` OR `source.starts_with("registry+https://")`, add `extra_annotations.insert("mikebom:license-source", "registry-required")` to the emitted `PackageDbEntry`. For `source == "local"` / `git+...` / `unknown`, no annotation is added.

### Unit tests (US2)

- [ ] T024 [US2] Add a unit test in `pe_clr.rs`: build a `TempDir` containing `<tmp>/pkgs/Microsoft.AspNetCore.App.Ref/8.0.0/ref/net8.0/Microsoft.AspNetCore.dll` AND `<tmp>/pkgs/Microsoft.AspNetCore.App.Ref/8.0.0/LICENSE.TXT` (case-mixed name) with synthetic text. Call `probe_license_file(dll_path, 3)`; assert returns `Some((text, path))` where `text` matches the file contents.
- [ ] T025 [US2] Add a unit test in `pe_clr.rs`: build the same `TempDir` WITHOUT the LICENSE file. Assert `probe_license_file` returns `None`.
- [ ] T026 [US2] Add a unit test in `maven.rs`: build a synthetic nested JAR carrying a `pom.xml` declaring `<licenses><license><name>Apache License 2.0</name></license></licenses>`. Use the milestone-130 `walk_jar_maven_meta`; assert the resulting nested `EmbeddedMavenMeta` carries the license annotation OR the new licenses field.

### US2 verification

- [ ] T027 [US2] Run `cargo +stable test -p mikebom --bin mikebom nuget::pe_clr maven::tests cargo_auditable` — confirm all new + existing tests pass.
- [ ] T028 [US2] Build release; scan audit image with US1 + US2; verify License Coverage scorecard moves to ≥3/5 (SC-003).

**Checkpoint**: US2 fully functional. SC-003 verifiable.

---

## Phase 5: User Story 3 — Supplier external-reference URL synthesis (Priority: P3)

**Goal**: Synthesize canonical registry URLs (`crates.io`, `nuget.org`, `search.maven.org`) for
the new components.

**Independent Test**: After US3, run `sbom-comparison --format summary`; assert Supplier
Attribution moves from 2/5 to ≥3/5.

### Implementation

- [ ] T029 [US3] In `mikebom-cli/src/scan_fs/mod.rs::supplier_from_purl`, add a `pkg:cargo/<name>@<version>` branch: emit `ExternalReference { ref_type: "website", url: format!("https://crates.io/crates/{name}/{version}") }`. Both segments URL-encoded via the existing `urlencoding::encode` (already in dep closure) or the `Purl::name` / `.version` accessors which return decoded strings.
- [ ] T030 [US3] In the same function, add a `pkg:nuget/<name>@<version>` branch: emit `url = format!("https://www.nuget.org/packages/{name}/{version}")`.
- [ ] T031 [US3] In the same function, add a `pkg:maven/<g>/<a>@<v>` branch: emit `url = format!("https://search.maven.org/artifact/{group}/{artifact}/{version}/jar")`. Gate emission on the component carrying `mikebom:source-mechanism = "maven-jar-nested"` annotation (use existing extra_annotations probe).
- [ ] T032 [US3] In `mikebom-cli/src/scan_fs/binary/entry.rs::cargo_auditable_packages_to_entries`, for each entry whose `source` matches `^git\+(https?://[^#]+?)(\.git)?(#[a-f0-9]+)?$` (compile a `regex::Regex` lazily via `once_cell::sync::Lazy` or `std::sync::OnceLock`), extract the captured URL group (strip trailing `.git` and any `#<rev>` fragment) and add `extra_annotations.insert("mikebom:cargo-vcs-source-url", "<url>")`. This is the catalogued C98 plumbing annotation; per contracts/annotation-schema.md C98, the annotation preserves the provenance signal that the URL came from the build-time cargo-auditable source declaration (not from PURL-heuristic guessing).
- [ ] T033 [US3] In `scan_fs/mod.rs::supplier_from_purl` (or wherever the per-component external-refs are computed), check for the `mikebom:cargo-vcs-source-url` annotation. When present, emit an additional `ExternalReference { ref_type: "vcs", url: <annotation value> }`. The native CDX `externalReferences[]` entry is the wire-format primary; the C98 annotation continues to ride alongside on the component as the provenance audit trail.

### Unit tests (US3)

- [ ] T034 [US3] In `scan_fs/mod.rs::supplier_tests`, add tests for: cargo PURL → crates.io URL; nuget PURL → nuget.org URL; maven PURL with `maven-jar-nested` source-mechanism annotation → search.maven.org URL; maven PURL without that annotation → no synthetic URL (top-level path's existing handling preserved).
- [ ] T035 [US3] In `binary/entry.rs::tests`, add a test for the `git+https://github.com/X/Y.git#<rev>` source-field parse: assert the captured URL is `https://github.com/X/Y` (sans `.git`, sans `#<rev>`).

### US3 verification

- [ ] T036 [US3] Run `cargo +stable test -p mikebom` — confirm all tests pass.
- [ ] T037 [US3] Build release; scan audit image with all three USs landed; run `sbom-comparison --format summary`. Assert: Version Accuracy = 5/5; License Coverage ≥ 3/5; Supplier Attribution ≥ 3/5; OVERALL ≥ 3.0 (mikebom leads syft by ≥ 0.5).

**Checkpoint**: US3 fully functional. SC-001 + SC-004 verifiable.

---

## Phase 6: Polish & Cross-Cutting Verification

- [ ] T038 Catalogue C96 `mikebom:license-source`, C97 `mikebom:license-text-sha256`, and C98 `mikebom:cargo-vcs-source-url` in `docs/reference/sbom-format-mapping.md` per contracts/annotation-schema.md. Grep the file for the highest existing C-NN; pin C96..C98 (or next-available range).
- [ ] T039 [P] Register C96, C97, C98 as `cdx_anno!` entries in `mikebom-cli/src/parity/extractors/cdx.rs`.
- [ ] T040 [P] Register C96, C97, C98 as `spdx23_anno!` entries in `spdx2.rs`.
- [ ] T041 [P] Register C96, C97, C98 as `spdx3_anno!` entries in `spdx3.rs`.
- [ ] T042 Register C96, C97, C98 as `ParityExtractor` slice entries in `mikebom-cli/src/parity/extractors/mod.rs` with matching `use` imports. Confirm shape tests pass (`extractors_table_is_sorted_by_row_id`, `every_catalog_row_has_an_extractor`).
- [ ] T043 Run SC-005 byte-identity verification: `./scripts/regen-goldens.sh` produces zero `.cdx.json` / `.spdx.json` churn.
- [ ] T044 Run SC-006 performance verification: time the audit-image scan pre vs post; assert wall-clock growth <30%.
- [ ] T045 Update `CHANGELOG.md` `[Unreleased]` section with the milestone-131 entry. Document the three USs + the new C96 annotation + the audit-image scorecard transformation (2.4 → ≥3.0).
- [ ] T046 Run the pre-PR gate: `./scripts/pre-pr.sh` confirms `>>> all pre-PR checks passed.` Fix any clippy lints surfaced.
- [ ] T047 Commit + push the `131-quality-metadata-backfill` branch.
- [ ] T048 Open PR via `gh pr create` with the summary referencing the post-131 scorecard expectations + the milestone-131 PR template.

---

## Dependency Graph

```text
Phase 1 (Setup) ───→ Phase 2 (empty) ───┬──→ Phase 3 (US1, P1) ──┐
                                        │                         ├──→ Phase 6 (Polish)
                                        ├──→ Phase 4 (US2, P2) ──┤
                                        │                         │
                                        └──→ Phase 5 (US3, P3) ──┘
```

Phases 3/4/5 are **independent**.

## Parallel Execution Opportunities

**Within Phase 1**: T002 + T003 in parallel.

**Within Phase 3 (US1)**: T005..T011 are sequential (single-file dependencies); T015 (unit tests) parallel
with implementation completion. T016 + T017 sequential.

**Within Phase 4 (US2)**: T018..T020 (US2a) ‖ T021..T022 (US2b) ‖ T023 (US2c) — three independent
sub-tracks. T024 ‖ T025 ‖ T026 (three unit tests in different files).

**Within Phase 5 (US3)**: T029..T033 sequential within same file (`scan_fs/mod.rs` + `binary/entry.rs`).
T034 ‖ T035.

**Within Phase 6**: T039 ‖ T040 ‖ T041 (three extractor files).

**Across stories**: after Phase 2 checkpoint, US1 ‖ US2 ‖ US3 can run on separate branches.

## Implementation Strategy

**Recommended cadence: three sequential PRs.**

- **PR 1: US1 only** — ~300 LOC CustomAttribute walking. Resolves 373 VERSION_MISMATCH cases (SC-002).
- **PR 2: US2 only** — ~150 LOC across three file modifications. License Coverage 1/5 → ≥3/5 (SC-003).
- **PR 3: US3 only** — ~100 LOC URL-synthesis heuristics. Supplier Attribution 2/5 → ≥3/5 (SC-004).

Alternatively bundle all three in one PR if confidence is high after US1 lands locally — the
spec/plan support this. The split is the **safer default**.

**MVP scope: US1 alone.** Single largest scorecard regression fix.

**Risk callouts**:

- T010 (`walk_custom_attributes`) is the most complex single task — CustomAttribute coded-index
  resolution through MemberRef → TypeRef → #Strings is mechanical but tedious. Time-box at 1 day;
  if it slips, the fallback is to add `pelite = "0.10"` as a dependency (burning the "zero new
  Cargo deps" constraint). Decision point: document in tasks.md as a comment if the fallback is
  exercised.
- T021 (nested-JAR license propagation) requires touching the milestone-130 walker's per-entry
  emission path. Verify byte-identity preservation (top-level JAR emissions unchanged) via the
  existing milestone-009 maven tests.
- T037 (end-to-end scorecard verification) depends on all three USs landing. If only US1 ships,
  run T037 with just SC-001/SC-002 + acknowledge SC-003/SC-004 deferred.
