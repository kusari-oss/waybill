---
description: "Task list for milestone 098 — compiler/linker version extraction for build provenance"
---

# Tasks: Compiler/linker version extraction

**Input**: Design documents from `/Users/mlieberman/Projects/mikebom/specs/098-compiler-version-extract/`
**Prerequisites**: plan.md, spec.md (with Clarifications), research.md, data-model.md, contracts/build-provenance-contracts.md, quickstart.md

**Tests**: Included. Per-format unit tests in `elf.rs`, `macho.rs`, `pe.rs` modules (per SC-006); one optional integration test in `mikebom-cli/tests/binary_build_provenance.rs`.

**Organization**: Three user stories each extend ONE per-format reader (`elf.rs` / `macho.rs` / `pe.rs`). All three converge on the same `BinaryScan` struct in `entry.rs` (Phase 2 foundational task) but produce independent properties. US1 (P1, ELF `.comment`) is the broadest-coverage MVP; US2 + US3 layer on after.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: User story this task belongs to (US1–US3)
- File paths are workspace-relative.

## Path Conventions

Production code under `mikebom-cli/src/scan_fs/binary/` (extends milestone-023/024/028 per-format readers) + `mikebom-cli/src/parity/extractors/` (4 new catalog rows) + `docs/reference/sbom-format-mapping.md` (4 new rows). One optional new test file. Zero changes outside these paths (FR-006).

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Verify environment + confirm preconditions before touching production code.

- [X] T001 Confirm working branch is `098-compiler-version-extract`. Run `git status` + `git log -1 --oneline`; verify branch was created by `/speckit-specify` and main is at post-PR-#204 (milestone-097 merge) or later.
- [X] T002 Confirm baseline pre-PR gate passes. Run `./scripts/pre-pr.sh` once on the unchanged tree; expect `>>> all pre-PR checks passed.` Isolates any post-edit failure as introduced by milestone 098.
- [X] T003 Audit existing per-format readers + verify `object` crate accessors per research §1-§4. Run:
    ```bash
    grep -n 'fn parse_min_os_version\|LC_BUILD_VERSION\|for_each_load_command' mikebom-cli/src/scan_fs/binary/macho.rs | head -5
    grep -n 'fn parse_pe_identity\|major_linker_version\|MajorLinkerVersion' mikebom-cli/src/scan_fs/binary/pe.rs | head -5
    grep -n 'fn parse_note_package_public\|fn parse_gnu_build_id\|fn parse_debuglink\|section_by_name_bytes' mikebom-cli/src/scan_fs/binary/elf.rs mikebom-cli/src/scan_fs/binary/scan.rs | head -10
    grep -nE 'row_id: "C[0-9]+"' mikebom-cli/src/parity/extractors/mod.rs | tail -3
    ```
    Expected: `parse_min_os_version` present at `macho.rs:209`; `for_each_load_command` helper available; `parse_pe_identity` present at `pe.rs:54`; `parse_note_package_public` + `parse_gnu_build_id` + `parse_debuglink` present in `elf.rs`; `section_by_name_bytes(b".note.package")` precedent for `.comment` access. Highest existing parity row should be **C15** (`mikebom:binary-packed`) — confirms next-available is C16.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Extend the `BinaryScan` struct with three new fields shared by all three user stories. Forward-declare the `MachoBuildVersion` struct stub so all three fields can land in this single foundational task. Also extend the `fake_binary_scan` test helper in `entry.rs::tests`. This is the single blocking prerequisite for US1/US2/US3 implementation — Phase 2 truly blocks all three stories.

- [X] T004 Extend `BinaryScan` struct in `mikebom-cli/src/scan_fs/binary/entry.rs` per `data-model.md §entry.rs — BinaryScan + emission helpers`. Two-step:
    1. **Stub** the `MachoBuildVersion` type in `mikebom-cli/src/scan_fs/binary/macho.rs`. Single line near the top of the module:
        ```rust
        /// Filled in by T016 (US2). The fields are added here as
        /// pub stubs so T004 can plumb the BinaryScan field
        /// without an ordering dependency on US2.
        #[derive(Debug, Clone, Default)]
        pub struct MachoBuildVersion {
            pub platform: String,
            pub min_os: String,
            pub sdk: String,
            pub tools: Vec<(String, String)>,
        }
        ```
    2. **Add 3 new fields** to `BinaryScan`:
        ```rust
        pub comment_stamps: Vec<String>,
        pub macho_build_version: Option<macho::MachoBuildVersion>,
        pub pe_linker_version: Option<String>,
        ```
    Update the `fake_binary_scan` test helper at `entry.rs:898` to include all three new fields with default values (`Vec::new()`, `None`, `None`).

**Checkpoint**: After T004, `BinaryScan` has all three fields and the `MachoBuildVersion` struct shape is defined. US1, US2, US3 can proceed in any order — T016's contribution is filling in the `parse_build_version_full` parser, not changing the struct shape.

---

## Phase 3: User Story 1 — ELF `.comment` extraction (Priority: P1) 🎯 MVP

**Goal**: Every scanned ELF binary with a `.comment` section emits a `mikebom:elf-compiler-stamps` property on the file-level binary component. JSON array of NUL-delimited entries, within-binary deduped, per-entry 4 KiB cap, total 64 KiB cap with truncation marker.

**Independent Test**: scan `/bin/ls` (or any compiled ELF on the host); inspect the emitted CDX SBOM; confirm `components[?(@.purl contains 'ls')].properties[?(@.name=='mikebom:elf-compiler-stamps')]` exists with ≥1 entry. Toolchain-graceful-skip when no system ELF is available (e.g., scanning from macOS without a Linux fixture).

### Implementation for User Story 1

- [X] T005 [US1] Implement `parse_comment_section(data: &[u8]) -> Vec<String>` in `mikebom-cli/src/scan_fs/binary/elf.rs` per `research.md §2` + `data-model.md §elf.rs — extension`. NUL-delimited splitter with `HashSet`-based within-section dedup, per-entry 4 KiB cap (truncated entries get `" ... (truncated)"` suffix), total 64 KiB cap (subsequent entries replaced by final `"... (truncated)"` marker). Lossy-UTF-8 decode via `String::from_utf8_lossy`.
- [X] T006 [US1] Wire ELF `.comment` extraction in `mikebom-cli/src/scan_fs/binary/scan.rs` inside `scan_binary` (around line 158, alongside the existing `build_id` / `runpath` / `debuglink` extraction). Set `comment_stamps` only for `class == "elf"`; non-ELF gets `Vec::new()`. Add `comment_stamps` to the `BinaryScan` initializer block at scan.rs:223.
- [X] T007 [US1] Extend `build_elf_identity_annotations` in `mikebom-cli/src/scan_fs/binary/entry.rs` to emit `mikebom:elf-compiler-stamps` when `scan.comment_stamps` is non-empty. Per `data-model.md §entry.rs`: `bag.insert("mikebom:elf-compiler-stamps", serde_json::json!(scan.comment_stamps))`.

### Tests for User Story 1

- [X] T008 [P] [US1] Add unit test `parse_comment_section_single_stamp` to `mikebom-cli/src/scan_fs/binary/elf.rs::tests`. Synthetic `.comment` bytes: `b"GCC: (Debian 12.2.0-14) 12.2.0\0"`. Assert returned Vec has 1 entry matching the verbatim string (NUL stripped).
- [X] T009 [P] [US1] Add unit test `parse_comment_section_multi_toolchain_dedup` to `elf.rs::tests`. Synthetic bytes with GCC stamp + clang stamp + duplicate GCC stamp (NUL-separated). Assert returned Vec has 2 unique entries in first-occurrence order.
- [X] T010 [P] [US1] Add unit test `parse_comment_section_oversize_truncation` to `elf.rs::tests`. Synthetic entry of 5 KiB. Assert returned Vec has 1 entry of length ≤4 KiB + ` ... (truncated)` suffix.
- [X] T011 [P] [US1] Add unit test `parse_comment_section_total_cap` to `elf.rs::tests`. Synthesize 200 unique entries of ~500 bytes each. Assert returned Vec has the first N that fit within 64 KiB cap, followed by a final `"... (truncated)"` marker entry.
- [X] T012 [P] [US1] Add unit test `parse_comment_section_empty_section` to `elf.rs::tests`. Synthetic bytes of all NULs (`b"\0\0\0\0\0"`). Assert returned Vec is empty.
- [X] T013 [P] [US1] Add unit test `parse_comment_section_non_utf8` to `elf.rs::tests`. Synthetic entry containing `0xFF` byte. Assert returned Vec has 1 entry containing the replacement character `\u{FFFD}` (lossy decode per Edge Case US1#4).

### Parity catalog + verification for User Story 1

- [X] T014 [US1] Add C16 parity-catalog row + per-format extractors for `mikebom:elf-compiler-stamps`. Touch 4 files per `data-model.md §Parity-catalog rows`:
    - `mikebom-cli/src/parity/extractors/mod.rs`: append `ParityExtractor { row_id: "C16", label: "mikebom:elf-compiler-stamps", cdx: c16_cdx, spdx23: c16_spdx23, spdx3: c16_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: true }` after the existing C15 row.
    - `mikebom-cli/src/parity/extractors/cdx.rs`: add `cdx_anno!(c16_cdx, "mikebom:elf-compiler-stamps", component);` near the existing C10/C11/C15 macro invocations.
    - `mikebom-cli/src/parity/extractors/spdx2.rs`: add `spdx23_anno!(c16_spdx23, "mikebom:elf-compiler-stamps", component);`.
    - `mikebom-cli/src/parity/extractors/spdx3.rs`: add `spdx3_anno!(c16_spdx3, "mikebom:elf-compiler-stamps", component);`.
- [X] T015 [US1] Verify Contracts 1-3 from `contracts/build-provenance-contracts.md`. Run:
    ```bash
    cargo +stable test -p mikebom --bin mikebom \
        --no-fail-fast parse_comment_section_single_stamp \
        parse_comment_section_multi_toolchain_dedup \
        parse_comment_section_oversize_truncation \
        parse_comment_section_total_cap \
        parse_comment_section_empty_section \
        parse_comment_section_non_utf8 2>&1 | grep "test result:"
    # Expected: ok. 6 passed.

    grep -n 'C16.*mikebom:elf-compiler-stamps' mikebom-cli/src/parity/extractors/mod.rs
    # Expected: 1 match.
    ```

**Checkpoint**: US1 complete. MVP win — every scanned ELF binary now emits its `.comment` stamps as build-tier provenance.

---

## Phase 4: User Story 2 — Mach-O `LC_BUILD_VERSION` SDK + tools extraction (Priority: P2)

**Goal**: Every scanned Mach-O binary with an `LC_BUILD_VERSION` load command emits `mikebom:macho-build-version` (platform/min_os/sdk JSON object) + `mikebom:macho-build-tools` (tools JSON array) on the file-level component. Fat Mach-O uses the first slice (milestone-024 convention).

**Independent Test**: scan any recent (Xcode 14+) macOS binary; inspect the emitted CDX SBOM; confirm `mikebom:macho-build-version.platform == "macos"` and `mikebom:macho-build-tools` contains ≥1 entry with `tool ∈ {clang, ld}`. Toolchain-graceful-skip when scanning from non-macOS hosts.

### Implementation for User Story 2

- [X] T016 [US2] Implement `parse_build_version_full(bytes: &[u8]) -> Option<MachoBuildVersion>` + `tool_name(id: u32) -> Option<&'static str>` helper in `mikebom-cli/src/scan_fs/binary/macho.rs` per `research.md §3` + `data-model.md §macho.rs — extension`. The `MachoBuildVersion` struct stub already lives in `macho.rs` from T004; T016 fills in the parser. Defensive parsing of `ntools` (FR-008 + Edge Case): stop at first record that runs off `cmdsize`; emit `tracing::warn!` and return partial result. Unknown platform IDs → `"unknown-<id>"`; unknown tool IDs → `"unknown-<id>"`.
- [X] T017 [US2] Wire Mach-O extraction in `mikebom-cli/src/scan_fs/binary/scan.rs` for BOTH flat-Mach-O (`scan_binary` Mach-O arm) and fat-Mach-O (`scan_fat_macho` — extract from first slice only per milestone-024 first-slice convention). Set `macho_build_version` only for `class == "macho"`; non-Mach-O gets `None`.
- [X] T018 [US2] Extend `build_macho_identity_annotations` in `entry.rs` to emit `mikebom:macho-build-version` (JSON object) + `mikebom:macho-build-tools` (JSON array) when `scan.macho_build_version` is `Some(...)`. Skip the `build-tools` property when the tools list is empty (don't emit empty array). Per `data-model.md §entry.rs`.

### Tests for User Story 2

- [X] T019 [P] [US2] Add unit test `parse_build_version_full_happy_path` to `macho.rs::tests`. Synthesize a minimal Mach-O with one `LC_BUILD_VERSION` load command carrying `platform=macos, minos=14.0, sdk=14.4, ntools=2 (clang 1500.0.40.1, ld 1015.0.0)`. Assert returned `MachoBuildVersion` matches the expected struct.
- [X] T020 [P] [US2] Add unit test `parse_build_version_full_unknown_platform` to `macho.rs::tests`. Synthesize a Mach-O with `LC_BUILD_VERSION` carrying `platform=9999` (unknown). Assert returned `platform` field is `"unknown-9999"`.
- [X] T021 [P] [US2] Add unit test `parse_build_version_full_malformed_ntools` to `macho.rs::tests`. Synthesize a Mach-O with `LC_BUILD_VERSION` declaring `ntools=5` but `cmdsize` only fits 2 records. Assert returned `MachoBuildVersion.tools` has length 2 (defensive partial-parse stops at first bad record).
- [X] T022 [P] [US2] Add unit test `parse_build_version_full_missing_command` to `macho.rs::tests`. Synthesize a Mach-O with NO `LC_BUILD_VERSION` (e.g., only `LC_VERSION_MIN_MACOSX`). Assert `parse_build_version_full` returns `None`.
- [X] T022a [P] [US2] Add unit test `platform_name_covers_all_documented_platforms` to `macho.rs::tests` (analyze C2). Asserts the `platform_name` lookup table maps all 5 SC-002-documented platform IDs to expected strings: `platform_name(PLATFORM_MACOS) == Some("macos")`, `platform_name(PLATFORM_IOS) == Some("ios")`, `platform_name(PLATFORM_TVOS) == Some("tvos")`, `platform_name(PLATFORM_WATCHOS) == Some("watchos")`, `platform_name(PLATFORM_XROS) == Some("xros")`. Guards SC-002's platform-enum bound against future table edits. ~5 lines.

### Parity catalog + verification for User Story 2

- [X] T023 [US2] Add C17 + C18 parity-catalog rows + per-format extractors for `mikebom:macho-build-version` and `mikebom:macho-build-tools`. Mirror T014's pattern across `parity/extractors/{mod,cdx,spdx2,spdx3}.rs`. C17 is `order_sensitive: false` (scalar object); C18 is `order_sensitive: true` (tools declaration order matters).
- [X] T024 [US2] Verify Contracts 4 + 5 from `contracts/build-provenance-contracts.md`. Run:
    ```bash
    cargo +stable test -p mikebom --bin mikebom \
        --no-fail-fast parse_build_version_full_happy_path \
        parse_build_version_full_unknown_platform \
        parse_build_version_full_malformed_ntools \
        parse_build_version_full_missing_command \
        platform_name_covers_all_documented_platforms 2>&1 | grep "test result:"
    # Expected: ok. 5 passed.

    grep -nE 'C1[78].*mikebom:macho-build-(version|tools)' mikebom-cli/src/parity/extractors/mod.rs
    # Expected: 2 matches.
    ```

**Checkpoint**: US2 complete. Mach-O binaries from Xcode 14+ now expose their full LC_BUILD_VERSION record.

---

## Phase 5: User Story 3 — PE linker-version always-emit (Priority: P2)

**Goal**: Every scanned PE binary emits `mikebom:pe-linker-version = "<major>.<minor>"` on the file-level component. Always-emit per FR-003 (matches milestone-096 Q2 convention for `mikebom:binary-packed`); zeroed value `"0.0"` is valid and informative (signals packer/obfuscator).

**Independent Test**: scan a Windows PE binary (e.g., `notepad.exe` or a UPX-packed sample); inspect the emitted CDX SBOM; confirm `mikebom:pe-linker-version` matches `^\d+\.\d+$`. Toolchain-graceful-skip when no PE fixture is available.

### Implementation for User Story 3

- [X] T025 [US3] Implement `parse_linker_version<Pe: ImageNtHeaders>(file: &PeFile<'a, Pe, &'a [u8]>) -> String` in `mikebom-cli/src/scan_fs/binary/pe.rs` per `research.md §4` + `data-model.md §pe.rs — extension`. Reads `file.nt_headers().optional_header().major_linker_version()` + `minor_linker_version()` and formats as `"<major>.<minor>"`. Extend `parse_pe_identity` to also return the linker-version string — either by widening the existing tuple from 3-tuple to 4-tuple OR by refactoring to a named struct (implementer's choice; minimal-diff path is the tuple widening).
- [X] T026 [US3] Wire PE extraction in `mikebom-cli/src/scan_fs/binary/scan.rs` (PE arm). The existing `parse_pe_identity` call returns the new 4-tuple; destructure to also capture `pe_linker_version`. Set `pe_linker_version = Some(linker_version)` only for `class == "pe"`; non-PE gets `None`.
- [X] T027 [US3] Extend `build_pe_identity_annotations` in `entry.rs` to emit `mikebom:pe-linker-version` when `scan.pe_linker_version` is `Some(...)`. Per `data-model.md §entry.rs`.

### Tests for User Story 3

- [X] T028 [P] [US3] Extend the existing `parse_pe_identity_64bit_with_codeview` test in `pe.rs::tests` (currently at `pe.rs:400`) to ALSO assert that the returned tuple's 4th element matches the linker-version bytes synthesized into the fixture (e.g., MajorLinkerVersion=14, MinorLinkerVersion=36 → `"14.36"`). May require regenerating the synthetic fixture's optional-header bytes.
- [X] T029 [P] [US3] Add new unit test `parse_pe_identity_zeroed_linker_version` to `pe.rs::tests`. Synthesize a PE binary with `MajorLinkerVersion = 0, MinorLinkerVersion = 0`. Assert returned linker-version is `"0.0"` (always-emit guarantee per FR-003).

### Parity catalog + verification for User Story 3

- [X] T030 [US3] Add C19 parity-catalog row + per-format extractors for `mikebom:pe-linker-version`. Mirror T014's pattern across `parity/extractors/{mod,cdx,spdx2,spdx3}.rs`. `order_sensitive: false` (scalar string value).
- [X] T031 [US3] Verify Contract 6 from `contracts/build-provenance-contracts.md`. Run:
    ```bash
    cargo +stable test -p mikebom --bin mikebom \
        --no-fail-fast parse_pe_identity_64bit_with_codeview \
        parse_pe_identity_zeroed_linker_version 2>&1 | grep "test result:"
    # Expected: ok. 2 passed.

    grep -n 'C19.*mikebom:pe-linker-version' mikebom-cli/src/parity/extractors/mod.rs
    # Expected: 1 match.
    ```

**Checkpoint**: US3 complete. PE binaries always emit their linker version; zeroed values transparency-flag packer/obfuscator behavior.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: docs/reference/sbom-format-mapping.md update (4 new rows); optional integration test; diff-scope audit; pre-PR gate.

- [X] T032 [P] Update `docs/reference/sbom-format-mapping.md` with 4 new rows (C16-C19). Match the table format used for C10/C11/C15. Body per `data-model.md §docs/reference/sbom-format-mapping.md`: each row carries the Constitution-V audit note "No native field" with reference to research §1.
- [X] T033 [P] **REQUIRED** Create `mikebom-cli/tests/binary_build_provenance.rs` per `data-model.md §Integration test`. Closes SC-001 + SC-002 end-to-end emission coverage gap (analyze C1). Self-scan integration test:
    1. Copies the mikebom binary itself into a temp dir.
    2. Runs `mikebom sbom scan --path <tempdir> --output <file> --no-deep-hash`.
    3. Per host OS:
        - Linux: assert `mikebom:elf-compiler-stamps` has ≥1 entry on the file-level binary component (don't lock prefix — rustc's stamp format varies, and statically-linked C deps may also contribute GCC stamps).
        - macOS: assert `mikebom:macho-build-version.platform == "macos"` AND `mikebom:macho-build-tools` has ≥1 entry on the file-level binary component.
        - Windows / other: assert `mikebom:pe-linker-version` matches `^\d+\.\d+$` (always-emit guarantee).
    4. Per-host graceful-skip: if `find_file_level(&sbom)` returns `None` (no file-level binary component emitted — e.g., the host's `mikebom` binary was claimed by a package-db reader during a containerized test run), `eprintln!` skip reason + `return`. Matches the milestone-096 `binary_id_enrich.rs::unpacked_binary_emits_binary_packed_none` always-runs-with-skip-fallback pattern.
- [X] T034 Verify Contract 9 — diff scope guardrails. Run:
    ```bash
    # No new Cargo deps (FR-005 / SC-008):
    git diff --name-only main | grep -E '^Cargo\.(lock|toml)$' | wc -l
    # Expected: 0

    # Production code outside scan_fs/binary/ + parity/extractors/:
    git diff --name-only main | grep -E '^mikebom-cli/src/' \
      | grep -vE '^mikebom-cli/src/scan_fs/binary/' \
      | grep -vE '^mikebom-cli/src/parity/extractors/' \
      | wc -l
    # Expected: 0

    # Golden regen scope (FR-009 / SC-007):
    git diff --stat mikebom-cli/tests/fixtures/golden/ | tail -1
    # Expected: empty OR ≤ a handful of additive changes (no PURL or relationship changes)

    # Diff scope allowlist:
    git diff --name-only main | sort
    # Expected only:
    #   CLAUDE.md
    #   docs/reference/sbom-format-mapping.md
    #   mikebom-cli/src/parity/extractors/{cdx,mod,spdx2,spdx3}.rs
    #   mikebom-cli/src/scan_fs/binary/{elf,entry,macho,pe,scan}.rs
    #   mikebom-cli/tests/binary_build_provenance.rs   (NEW, optional)
    #   specs/098-compiler-version-extract/...
    ```
- [X] T035 Run the mandatory pre-PR gate per Contract 10. Run `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 ./scripts/pre-pr.sh`. Expect: `>>> all pre-PR checks passed.` with zero clippy warnings and zero test failures across the workspace. The 12 new unit tests (6 ELF + 4 Mach-O + 2 PE) all pass; the optional integration test passes or graceful-skips per host OS. The SPDX 3 conformance validator passes against the regenerated goldens.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies. Start immediately.
- **Foundational (Phase 2)**: T004 extends the `BinaryScan` struct with all three new fields (using a forward-declared `MachoBuildVersion` stub) — true blocking prerequisite for US1 / US2 / US3.
- **US1 (Phase 3, P1, MVP)**: Independent at file level. Depends on T004 for the `comment_stamps` field.
- **US2 (Phase 4, P2)**: Independent at file level. Depends on T004 for the `macho_build_version` field + the `MachoBuildVersion` struct stub. T016 fills in `parse_build_version_full`.
- **US3 (Phase 5, P2)**: Independent at file level. Depends on T004 for the `pe_linker_version` field.
- **Polish (Phase 6)**: Requires US1+US2+US3 implementation complete (catalog rows attach to property-emitting code).

### User Story Dependencies

- **US1 (P1)**: Independent at file level. Touches `elf.rs` (impl + 6 tests) + `entry.rs` (annotation) + `scan.rs` (wire) + 4 parity-catalog files.
- **US2 (P2)**: Independent at file level. Touches `macho.rs` (impl + 4 tests) + `entry.rs` (annotation + Mach-O BinaryScan field) + `scan.rs` (wire flat + fat) + 4 parity-catalog files.
- **US3 (P2)**: Independent at file level. Touches `pe.rs` (impl + 2 tests) + `entry.rs` (annotation) + `scan.rs` (wire) + 4 parity-catalog files.

### Within Each User Story

- US1: T005 (impl) → T006 (wire) → T007 (annotation). T008-T013 (6 unit tests) are parallel-safe. T014 (catalog rows) is parallel-safe with the tests. T015 verifies after T008-T014.
- US2: T016 (impl + struct field) → T017 (wire) → T018 (annotation). T019-T022 (4 unit tests) are parallel-safe. T023 (catalog rows) is parallel-safe. T024 verifies after T019-T023.
- US3: T025 (impl) → T026 (wire) → T027 (annotation). T028-T029 (2 unit tests) are parallel-safe. T030 (catalog row) is parallel-safe. T031 verifies after T028-T030.

### Parallel Opportunities

- T008 / T009 / T010 / T011 / T012 / T013 / T019 / T020 / T021 / T022 / T022a / T028 / T029 — 13 parallel-safe unit tests across all three stories (different test functions in different module-tests trees).
- T014 / T023 / T030 — parity-catalog row additions (different files within `parity/extractors/`). Could conflict on `mod.rs` if landed concurrently (different rows in the same `EXTRACTORS` const); recommend sequential or careful merge.
- T032 / T033 — docs update + optional integration test creation, both parallel-safe with each other and with most test tasks.

---

## Parallel Example: Phase 3-5 unit tests

```bash
# All unit-test additions can run in parallel (independent test functions):
Task: "Add unit test parse_comment_section_single_stamp (T008)"
Task: "Add unit test parse_comment_section_multi_toolchain_dedup (T009)"
Task: "Add unit test parse_comment_section_oversize_truncation (T010)"
Task: "Add unit test parse_comment_section_total_cap (T011)"
Task: "Add unit test parse_comment_section_empty_section (T012)"
Task: "Add unit test parse_comment_section_non_utf8 (T013)"
Task: "Add unit test parse_build_version_full_happy_path (T019)"
Task: "Add unit test parse_build_version_full_unknown_platform (T020)"
Task: "Add unit test parse_build_version_full_malformed_ntools (T021)"
Task: "Add unit test parse_build_version_full_missing_command (T022)"
Task: "Extend parse_pe_identity_64bit_with_codeview to assert linker-version (T028)"
Task: "Add unit test parse_pe_identity_zeroed_linker_version (T029)"
```

After T005 + T006 + T007 + T016 + T017 + T018 + T025 + T026 + T027 land, all test additions can land in any order.

---

## Implementation Strategy

### MVP First (US1 only)

The user's stated motivation is build-tier provenance for stripped binaries. ELF `.comment` covers the broadest population (every Linux distro binary). MVP path:

1. Phase 1: Setup (T001-T003)
2. Phase 2: Foundational — partial T004 (just `comment_stamps` field; defer Mach-O field to US2)
3. Phase 3: US1 (T005-T015) — impl + 6 unit tests + C16 catalog row + verify
4. Phase 6 partial: T032 (docs row for C16) + T035 (pre-PR gate)
5. **STOP and VALIDATE**: scan a real binary on Linux, confirm `mikebom:elf-compiler-stamps` appears.

US2 + US3 layer on after MVP-validation. The full milestone delivers all three stories in a single PR.

### Incremental Delivery (recommended)

Single PR shipping all three stories — the per-format independence + parallel-safe tests make this the right size. Total estimated time: ~2 dev-hours.

### Single-Developer Strategy

1. T001-T003 (setup, ~5 min)
2. T004 (foundational struct field, ~5 min)
3. T005-T015 (US1, ~45 min — parser + wire + annotation + 6 tests + catalog row + verify)
4. T016-T024 (US2, ~50 min — parser + wire flat/fat + annotation + 4 tests + 2 catalog rows + verify)
5. T025-T031 (US3, ~25 min — parse_linker_version + wire + annotation + 2 tests + catalog row + verify)
6. T032-T035 (Polish, ~15 min — docs + optional integration + diff audit + pre-PR gate)

Total: ~2 hours 25 min single-developer focus. Heavily parallel across the 12 unit-test tasks with multiple developers or agents.

---

## Notes

- [P] markers = different test functions OR different files with no shared edit-dependency.
- [Story] label maps task to user story for traceability.
- T004 adds all 3 new `BinaryScan` fields in one go, using a forward-declared `MachoBuildVersion` struct stub in `macho.rs`. T016 fills in the parser implementation without touching the struct shape. This keeps Phase 2 as the true foundational blocker for all three user stories.
- Catalog row IDs C16-C19 assume the highest existing is C15 (`mikebom:binary-packed`). Implementer verifies in T003 via `grep -nE 'row_id: "C[0-9]+"' mikebom-cli/src/parity/extractors/mod.rs | tail -3`. If higher rows exist (e.g., concurrent milestone work landed them), shift the new IDs forward.
- Backward compat with milestone 024 `mikebom:macho-min-os` (flat string) is preserved by design — the new `mikebom:macho-build-version` (structured) is ADDITIVE; both properties emit when `LC_BUILD_VERSION` is present. Operators keying on the old flat property continue to work.
- Pre-PR gate (T035) MUST run with `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1` per CLAUDE.md SBOM-spec-touching-changes rule. The 4 new annotations flow through the same emission path as every other `mikebom:*` annotation since milestone 004 — SPDX 3 conformance validator already accepts them.
- Commit boundary suggestion: single commit (US1+US2+US3+Polish in one PR). Surface is small enough (~150 production lines + ~80 test lines) that splitting adds noise.
- Avoid: implementing DWARF debug-info extraction, compiler-CPE candidate emission, or `.comment` sub-parsing in this milestone. Those are out-of-scope per spec — defer to future milestones if signal emerges.
