---
description: "Task list for milestone 096 — identify-unknown-binaries enrichment (embedded version strings + packer detection + symbol fingerprinting)"
---

# Tasks: Identify-unknown-binaries enrichment

**Input**: Design documents from `/Users/mlieberman/Projects/mikebom/specs/096-binary-id-enrich/`
**Prerequisites**: plan.md, spec.md (with Q1/Q2/Q3 clarifications), research.md, data-model.md, contracts/, quickstart.md

**Tests**: Included. Three integration test files (one per user story) plus the existing pre-PR gate.

**Organization**: Tasks grouped by user story. US1 (P1, headline static-link identification) is the MVP increment. US2 (P2, packer transparency) requires the new C12 parity-catalog row and golden regen (additive). US3 (P2, symbol-fingerprint sibling) integrates with US1 via the composite-evidence aggregator per Clarification Q1.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: User story this task belongs to (US1–US3)
- File paths are workspace-relative.

## Path Conventions

Production code under `mikebom-cli/src/scan_fs/binary/` (extends the milestone-004 module) + `mikebom-cli/src/parity/extractors/` (new C12 row) + `mikebom-cli/src/generate/{cyclonedx,spdx}/` (property emission). Three new test files under `mikebom-cli/tests/`. Zero changes outside these directories (FR-008).

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Verify environment + confirm preconditions before touching production code.

- [X] T001 Confirm working branch is `096-binary-id-enrich`. Run `git status` + `git log -1 --oneline`; verify branch was created by `/speckit.specify` and main is at post-PR-#201 state (milestone-094 merge) or later.
- [X] T002 Confirm baseline pre-PR gate passes. Run `./scripts/pre-pr.sh` once on the unchanged tree; expect `>>> all pre-PR checks passed.` Isolates any post-edit failure as introduced by milestone 096.
- [X] T003 Confirm preconditions. Run `grep -E '^regex = ' mikebom-cli/Cargo.toml` → expect `regex = "1"` (workspace dep already available — FR-007 satisfied). Run `ls mikebom-cli/src/scan_fs/binary/version_strings.rs mikebom-cli/src/scan_fs/binary/packer.rs` → expect both files exist (milestone-004 stubs to extend).
- [X] T004 Confirm the existing parity-catalog C10/C11 rows for `mikebom:binary-class` + `mikebom:binary-stripped` are in place. Run `grep -nE 'C10.*binary-class|C11.*binary-stripped' mikebom-cli/src/parity/extractors/mod.rs` → expect 2 matches. The new C12 row (FR-005 in plan terms — the `mikebom:binary-packer` row) will mirror their shape.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: No shared infrastructure across all three user stories — each story has its own extraction module + integration test. The parity-catalog C12 row is specific to US2 (packer detection). The composite-evidence aggregator in `binary/mod.rs::read()` is the integration point for US1+US3 and is added during US3 (it depends on both being implemented).

(No tasks in this phase — file-level independence between US1 / US2 / US3.)

**Checkpoint**: US1, US2, US3 can begin in any order. Ship-order recommendation: US1 (P1) → US2 (P2 packer) → US3 (P2 symbol-fingerprint), because US3's composite-evidence aggregator depends on US1 having produced the version-string component shape that US3 merges into.

---

## Phase 3: User Story 1 — Operator scans a stripped binary that statically links OpenSSL and sees an OpenSSL component (Priority: P1) 🎯 MVP

**Goal**: Extract embedded version strings from binary `.rodata` / `__cstring` / `.rdata` sections per the 5-pattern v1 starter set (OpenSSL, zlib, libcurl, sqlite, libxml2). Emit `pkg:generic/<name>@<version>` components with `confidence = 0.6` and `mikebom:identification-method = embedded-version-string`.

**Independent Test**: build a stripped C binary that statically links OpenSSL 3.x; scan with `target/release/mikebom sbom scan --path <fixture-dir>`; expect a `pkg:generic/openssl@<version>` component with the milestone-096 evidence fields. Toolchain-graceful-skip when `cc` or `openssl-dev` unavailable.

### Implementation for User Story 1

- [X] T005 [P] [US1] Extend `mikebom-cli/src/scan_fs/binary/version_strings.rs` per `data-model.md §version_strings.rs`. Add the `VERSION_STRING_PATTERNS` const table (5 rows: openssl / zlib / libcurl / sqlite / libxml2 with the regex anchors from research.md §1). Implement `pub fn extract_embedded_versions(bytes: &[u8]) -> Vec<EmbeddedVersionMatch>` that walks each pattern via `regex::bytes::Regex` and returns a struct containing `library_name`, `version`, source-offset for evidence. Use lowercase library names. Emit a single match per pattern hit (don't dedupe within-binary; cross-binary dedup happens in `linkage::dedup_globally`).
- [X] T006 [US1] Wire `version_strings::extract_embedded_versions` into the binary-scan flow at `mikebom-cli/src/scan_fs/binary/mod.rs::read()`. For each match, construct a `PackageDbEntry` per `data-model.md §version_strings.rs` output spec: `purl = pkg:generic/<name>@<version>`, `type = library`, `evidence.identity[].technique = binary-analysis`, `evidence.identity[].confidence = 0.6`, `properties[]` includes `mikebom:identification-method = embedded-version-string`, `evidence.occurrences[]` lists the binary file path. Append to the components vector that the existing dedup pass consumes (strict-PURL-equality merge per Q3).
- [X] T007 [P] [US1] Create `mikebom-cli/tests/binary_embedded_version_strings.rs` with at least 2 tests: (a) `extracts_openssl_3_from_statically_linked_binary` — builds a fixture (or skips if `cc` / openssl-dev absent), scans, asserts a `pkg:generic/openssl@3.*.*` component with the right evidence shape; (b) `no_emission_when_no_pattern_matches` — scans a known-binary-with-no-static-linkage fixture and asserts zero `pkg:generic/openssl|zlib|libcurl|sqlite|libxml2@...` components emitted. Toolchain-graceful-skip pattern: `eprintln!("skipping — fixture unbuildable: cc not found")` + `return` if dependencies missing.
- [ ] T008 [P] [US1] Build the OpenSSL-static fixture under `mikebom-cli/tests/fixtures/binaries/elf/openssl-static/` (stay-set per milestone-090). Provide a `Makefile` or `build.sh` that compiles `main.c` with static OpenSSL + strips. The compiled binary is checked in as the test fixture; the build script is for reproducibility (re-builds during dev when openssl-dev is bumped). Use the C source from `quickstart.md` Recipe 4a.
- [ ] T009 [US1] Verify Contract 1 from `contracts/binary-id-contracts.md`. Run:
    ```bash
    cargo +stable test -p mikebom --test binary_embedded_version_strings 2>&1 | grep -E "test result:"
    # Expected: ok. 2 passed (or 1 passed + 1 ignored if toolchain absent).

    target/release/mikebom --offline sbom scan \
        --path mikebom-cli/tests/fixtures/binaries/elf/openssl-static/ \
        --format cyclonedx-json --output /tmp/us1.cdx.json --no-deep-hash
    jq '.components[] | select(.purl | startswith("pkg:generic/openssl@"))' /tmp/us1.cdx.json
    ```
    Expect one component with the right `name`, `version`, `purl`, `evidence.identity[].confidence = 0.6`, and `properties[]` containing `mikebom:identification-method = embedded-version-string`.

**Checkpoint**: US1 complete. MVP win lands — operators scanning unknown binaries with embedded OpenSSL (or zlib/libcurl/sqlite/libxml2) see the embedded component in the SBOM.

---

## Phase 4: User Story 2 — Operator scans a packed binary and sees a transparency flag (Priority: P2)

**Goal**: Detect UPX-packed binaries via two-signal approach (Section A: ELF/PE `UPX0`+`UPX1` section names; Signal B: universal `UPX!` magic-bytes scan). Always emit `mikebom:binary-packer = <name|none>` property on file-level binary components per Clarification Q2.

**Independent Test**: take any small ELF binary, run `upx --best <bin>`, scan with mikebom. Expect `mikebom:binary-packer = upx` property on the file-level binary component. Run a separate scan on an UNpacked binary; expect `mikebom:binary-packer = none`.

### Implementation for User Story 2

- [X] T010 [P] [US2] Extend `mikebom-cli/src/scan_fs/binary/packer.rs` per `data-model.md §packer.rs`. Add the `PACKER_SIGNATURES` table (UPX-only in v1) and `pub fn detect_packer(file: &object::read::File<'_>, bytes: &[u8]) -> Option<&'static str>`. Implement BOTH signals: Signal A (section-name heuristic — iterate `file.sections()` filtering on `name().starts_with("UPX")` with ≥2 distinct UPX-prefixed sections required for ELF/PE), Signal B (`memchr::memmem::find(bytes, b"UPX!").is_some()` universal magic scan). Either signal firing returns `Some("upx")`.
- [X] T011 [US2] Wire ALWAYS-emit packer property at `mikebom-cli/src/scan_fs/binary/mod.rs::read()` per Clarification Q2. On the file-level binary `PackageDbEntry` (the existing one carrying `mikebom:binary-class` + `mikebom:binary-stripped`), append a property `mikebom:binary-packer` whose value is either `detect_packer(...)` result OR the literal `"none"` if `detect_packer` returns `None`. Property is present on EVERY file-level binary component regardless of pack state.
- [ ] T012 [P] [US2] Add the parity-catalog C12 row. Edit `mikebom-cli/src/parity/extractors/mod.rs` and add the C12 entry to the `EXTRACTORS` table immediately after C11 (the `mikebom:binary-stripped` row at line ~141). Use `data-model.md §Parity-catalog C12 row` body verbatim: `row_id: "C12", label: "mikebom:binary-packer", cdx: c12_cdx, spdx23: c12_spdx23, spdx3: c12_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false`.
- [ ] T013 [P] [US2] Add the per-format C12 extractor stubs. Edit (in parallel — different files):
    - `mikebom-cli/src/parity/extractors/cdx.rs`: add `cdx_anno!(c12_cdx, "mikebom:binary-packer", component);` near the existing C10/C11 macro invocations.
    - `mikebom-cli/src/parity/extractors/spdx2.rs`: add `spdx23_anno!(c12_spdx23, "mikebom:binary-packer", component);` near C10/C11.
    - `mikebom-cli/src/parity/extractors/spdx3.rs`: add `spdx3_anno!(c12_spdx3, "mikebom:binary-packer", component);` near C10/C11.
- [ ] T014 [P] [US2] Wire `mikebom:binary-packer` property emission in the CDX + SPDX 2.3 + SPDX 3 generators. The generator side typically iterates component properties already; verify the existing emission flow picks up the new property automatically (it should, since `mikebom:binary-class` + `mikebom:binary-stripped` go through the same path). If a per-format generator has an explicit allowlist of mikebom-properties to emit (rather than emit-all), add `mikebom:binary-packer` to it. Touch ONLY the property-emission code — no other generator changes.
- [ ] T015 [P] [US2] Update `docs/reference/sbom-format-mapping.md` to add the C12 row. Match the table format used for C10 + C11. Body per `data-model.md §docs/reference/sbom-format-mapping.md`: include the Constitution-V audit justification ("no native packer-status field in any of CDX 1.6 / SPDX 2.3 / SPDX 3 schemas").
- [X] T016 [P] [US2] Create `mikebom-cli/tests/binary_packer_detection.rs` with at least 3 tests: (a) `unpacked_binary_emits_packer_none` — scan any unpacked ELF (e.g., mikebom itself in `target/release/`), assert `mikebom:binary-packer = none`; (b) `upx_packed_binary_detected_via_section_names` — scan a UPX-packed ELF fixture, assert `mikebom:binary-packer = upx`; (c) `upx_packed_binary_detected_via_magic_bytes` — same fixture but verify Signal B alone fires (e.g., scan with section table mutated to remove `UPX0`/`UPX1`, OR use a Mach-O packed fixture for which only Signal B applies). Toolchain-graceful-skip if `upx` not installed.
- [ ] T017 [P] [US2] Build the UPX-packed fixture under `mikebom-cli/tests/fixtures/binaries/elf/upx-packed/` per `quickstart.md` Recipe 4b. Use `cp target/release/mikebom <fixture>/sample && upx --best <fixture>/sample`. Build script gracefully skips if `upx` not on PATH.
- [X] T018 [US2] Verify Contract 2 + Contract 6 from `contracts/binary-id-contracts.md`. Run:
    ```bash
    cargo +stable test -p mikebom --test binary_packer_detection 2>&1 | grep -E "test result:"
    cargo +stable test -p mikebom --test sbom_format_mapping_coverage 2>&1 | tail -3
    # Both expected: passes.

    grep -n 'C12.*mikebom:binary-packer' mikebom-cli/src/parity/extractors/mod.rs   # 1 match
    grep -n 'c12_cdx\|c12_spdx23\|c12_spdx3' mikebom-cli/src/parity/extractors/   # 3 matches
    grep -nE '^\| C12' docs/reference/sbom-format-mapping.md   # 1 match
    ```
    All four greps MUST find their expected counts.

**Checkpoint**: US2 complete. Packer-transparency property present on every file-level binary component. Parity-catalog C12 row registered + documented; format-mapping doc updated.

---

## Phase 5: User Story 3 — Operator scans a binary that exports a known library's symbols and sees a fingerprint match (Priority: P2)

**Goal**: For ELF binaries with a `.dynsym` symbol table, fingerprint against the 3-library × 10-symbol v1 starter set (OpenSSL, zlib, libcurl); match fires when ≥8 of 10 symbols present. Emit `pkg:generic/<name>` (no version) with `confidence = 0.4`. When the same library ALSO triggered an embedded-version-string match on the same binary, MERGE into one composite-evidence component per Clarification Q1.

**Independent Test**: build (or pre-bake) a fixture that exports OpenSSL's symbol set without the embedded version string; scan with mikebom; expect `pkg:generic/openssl` (no version) with `evidence.identity[].confidence = 0.4` and `mikebom:identification-method = symbol-fingerprint`. Plus: scan the openssl-static fixture from US1 (which has BOTH version string AND symbols) and expect a SINGLE component with TWO `evidence.identity[]` entries.

### Implementation for User Story 3

- [X] T019 [P] [US3] Create `mikebom-cli/src/scan_fs/binary/symbol_fingerprint.rs` per `data-model.md §symbol_fingerprint.rs`. Add the `FINGERPRINTS` table (3 rows: openssl / zlib / libcurl with 10 symbols each, 8/10 threshold per research.md §3). Implement `pub fn match_symbols(file: &object::read::File<'_>) -> Vec<SymbolFingerprintMatch>` that walks the ELF `.dynsym` exports (via `object::read::elf::ElfFile::dynamic_symbols()` or equivalent), builds a `HashSet<&str>` of exported symbol names, then iterates `FINGERPRINTS` and emits a match for any library where ≥`required_symbol_count` of its `symbols` appear in the set. Each match carries `library_name`, `matched_count`, `total_count`. ELF-only in v1 (PE/Mach-O symbol fingerprinting deferred per spec Out-of-Scope).
- [X] T020 [US3] Wire `symbol_fingerprint::match_symbols` into `mikebom-cli/src/scan_fs/binary/mod.rs::read()` AND implement the composite-evidence merge per Clarification Q1. Pseudocode per `quickstart.md` Recipe 2:
    1. Collect `version_hits` (from US1's T006 wiring) and `symbol_hits` (new).
    2. Build a per-library `HashMap<String, PackageDbEntry>`, keyed by lowercase library name.
    3. For each version_hit: insert/update entry with PURL = `pkg:generic/<lib>@<version>` and append the embedded-version-string `evidence.identity[]` entry (confidence 0.6).
    4. For each symbol_hit: if the library already has an entry from step 3, APPEND a symbol-fingerprint `evidence.identity[]` entry to it (composite evidence — same component, two evidence entries). If no entry exists, create one with PURL = `pkg:generic/<lib>` (no version) and the symbol-fingerprint evidence.
    5. Flatten the map into a Vec; append to the components vector that `linkage::dedup_globally` consumes.
    
    Add `mikebom:fingerprint-symbols-matched = <matched>/<total>` property on symbol-fingerprint-only components for transparency.
- [X] T021 [P] [US3] Create `mikebom-cli/tests/binary_symbol_fingerprint.rs` with at least 3 tests: (a) `symbol_only_match_emits_openssl_without_version` — scan a fixture that exports OpenSSL's symbol set but has the version string stripped/missing, assert `pkg:generic/openssl` component with `confidence = 0.4` and `mikebom:identification-method = symbol-fingerprint`; (b) `composite_evidence_when_both_techniques_match` — scan the US1 openssl-static fixture, assert ONE component with TWO `evidence.identity[]` entries (versions-string + symbol-fingerprint); (c) `no_match_when_under_threshold` — scan a fixture that exports only 5 of OpenSSL's 10 symbols (under the 8/10 threshold), assert zero `pkg:generic/openssl` components from this technique.
- [ ] T022 [P] [US3] Build the symbol-only-fingerprint fixture under `mikebom-cli/tests/fixtures/binaries/elf/openssl-symbols-only/` per `quickstart.md` Recipe 4c. Pragmatic approach: build the same `main.c` as US1 but post-process the binary to zero out the `OpenSSL ` version-string bytes in `.rodata` via a small build-time helper script (`sed`-style binary patch or `objcopy --add-section`). Toolchain-graceful-skip if the build helper fails.
- [X] T023 [US3] Verify Contract 3 + Contract 4 (composite evidence Q1) + Contract 5 (strict-PURL-equality dedup Q3) from `contracts/binary-id-contracts.md`. Run:
    ```bash
    cargo +stable test -p mikebom --test binary_symbol_fingerprint 2>&1 | grep -E "test result:"
    # Expected: ok. 3 passed (or fewer if toolchain absent — graceful skip).

    target/release/mikebom --offline sbom scan \
        --path mikebom-cli/tests/fixtures/binaries/elf/openssl-static/ \
        --format cyclonedx-json --output /tmp/us3-composite.cdx.json --no-deep-hash
    jq '.components[] | select(.purl | startswith("pkg:generic/openssl@")) | .evidence.identity | length' /tmp/us3-composite.cdx.json
    # Expected: 2 (composite evidence per Q1)
    ```

**Checkpoint**: US3 complete. All three signal channels emit; composite-evidence aggregation works; strict-PURL-equality dedup preserves version-specificity.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Regenerate any goldens whose file-level binary components gained the new `mikebom:binary-packer = none` property (additive per FR-009); audit diff scope; final pre-PR gate.

- [ ] T024 Regenerate goldens for any fixture that contains binary components. The always-emit-`binary-packer` convention per Q2 adds the property to every file-level binary component → existing goldens that include polyglot / rpm-image / binary fixtures will regenerate to include the new property. Run:
    ```bash
    MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo +stable test -p mikebom --test cdx_regression 2>&1 | tail -5
    MIKEBOM_UPDATE_SPDX_GOLDENS=1 cargo +stable test -p mikebom --test spdx_regression 2>&1 | tail -5
    MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo +stable test -p mikebom --test spdx3_regression 2>&1 | tail -5
    ```
    Expected: tests pass on first run AFTER regen. The diff under `mikebom-cli/tests/fixtures/golden/` should be limited to (a) `mikebom:binary-packer = none` property additions on existing file-level binary components and (b) new `pkg:generic/<lib>@<version>` or `pkg:generic/<lib>` components if any existing fixture happens to trigger an embedded-version-string OR symbol-fingerprint match. Per SC-007, the spurious-match component count across the 9 existing ecosystem fixtures MUST be ≤1.
- [ ] T025 Audit golden diff scope per FR-009 + SC-007. Run:
    ```bash
    # Verify all changes are additive (new properties / new components — no PURL changes on existing components, no relationship-graph changes):
    git diff mikebom-cli/tests/fixtures/golden/ | head -60

    # Per SC-007: count NEW spurious version-string / symbol-fingerprint component additions across the 9 existing ecosystem fixtures. MUST be ≤1:
    git diff mikebom-cli/tests/fixtures/golden/ | grep -E '^\+.*"pkg:generic/(openssl|zlib|libcurl|sqlite|libxml2)' | wc -l
    # Expected: ≤1
    ```
    If ANY non-additive change appears, OR the spurious-match count exceeds 1, STOP and investigate (likely a pattern that needs a narrower anchor, or a fixture that genuinely statically links the library — verify which case applies before accepting the diff).
- [X] T026 Verify Contract 7 — diff scope guard. Run:
    ```bash
    # Allowed paths only (Cargo.lock/toml not expected to change per FR-007):
    git diff --name-only main | grep -vE '^(mikebom-cli/src/scan_fs/binary/.+\.rs|mikebom-cli/src/parity/extractors/.+\.rs|mikebom-cli/src/generate/(cyclonedx|spdx)/.+\.rs|mikebom-cli/tests/binary_(embedded_version_strings|packer_detection|symbol_fingerprint)\.rs|mikebom-cli/tests/fixtures/(binaries|golden)/.+|docs/reference/sbom-format-mapping\.md|specs/096-binary-id-enrich/.+|CLAUDE\.md)$' | grep -v '^$' | wc -l
    # Expected: 0

    # No Cargo.lock/toml change (FR-007):
    git diff --name-only main | grep -E '^Cargo\.(lock|toml)$' | wc -l
    # Expected: 0
    ```
- [X] T027 Run the mandatory pre-PR gate per Contract 8. Run `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 ./scripts/pre-pr.sh`. Expect: `>>> all pre-PR checks passed.` with zero clippy warnings and zero test failures across the workspace. The three new test files report passing or graceful-skip results; the SPDX 3 conformance validator passes against the regenerated goldens.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies. Start immediately.
- **Foundational (Phase 2)**: None — file-level independence.
- **US1 (Phase 3, P1, MVP)**: Independent. Touches `version_strings.rs` + `binary/mod.rs::read()` integration + 1 new test file + 1 new fixture.
- **US2 (Phase 4, P2)**: Independent at file level. Touches `packer.rs`, `parity/extractors/{mod,cdx,spdx2,spdx3}.rs`, generators, docs, 1 new test, 1 new fixture.
- **US3 (Phase 5, P2)**: Soft-depends on US1 having shipped — the composite-evidence aggregator in `binary/mod.rs::read()` (T020) requires both `version_hits` and `symbol_hits` collectors to exist. Implementation-order: US1 then US3 (in the same PR), or stage US1 in commit-A then US3 in commit-B.
- **Polish (Phase 6)**: Depends on US1+US2+US3 being complete. Golden regen is the catch-all for the additive property addition.

### User Story Dependencies

- **US1 (P1)**: Independent at file + spec level.
- **US2 (P2)**: Independent at all levels — own module, own parity-catalog row, own integration test.
- **US3 (P2)**: Soft-depends on US1's `version_hits` data being available in `binary/mod.rs::read()` so the composite-evidence merge has both signals to combine. In practice, this means T020 (US3 wiring) requires T006 (US1 wiring) to have landed.

### Within Each User Story

- US1: T005 + T007 + T008 are parallel-safe (different files); T006 follows T005 (same module). T009 verifies after T006-T008.
- US2: T010 + T012 + T013 + T015 + T016 + T017 are all parallel-safe (different files). T011 follows T010 (same module). T014 is a generator touch and may need light coordination with T011. T018 verifies after T010-T017.
- US3: T019 + T021 + T022 are parallel-safe (different files). T020 follows T019 AND requires US1's T006 to have landed. T023 verifies after T019-T022.

### Parallel Opportunities

- T005 / T007 / T008 / T010 / T012 / T013 / T015 / T016 / T017 / T019 / T021 / T022 — 12 parallel tasks across all three stories' implementation + test + fixture phases. Strong fan-out potential for multi-agent or multi-developer execution.

---

## Parallel Example: Phase 3–5 (US1 + US2 + US3 implementation)

```bash
# Implementation tasks across all three stories — different files, no in-PR conflicts:
Task: "Extend version_strings.rs with v1 pattern table (T005)"
Task: "Create binary_embedded_version_strings.rs test (T007)"
Task: "Build openssl-static fixture (T008)"
Task: "Extend packer.rs with UPX signature scan (T010)"
Task: "Add C12 row to parity/extractors/mod.rs (T012)"
Task: "Add c12_cdx / c12_spdx23 / c12_spdx3 extractors (T013)"
Task: "Update docs/reference/sbom-format-mapping.md C12 row (T015)"
Task: "Create binary_packer_detection.rs test (T016)"
Task: "Build upx-packed fixture (T017)"
Task: "Create symbol_fingerprint.rs (T019)"
Task: "Create binary_symbol_fingerprint.rs test (T021)"
Task: "Build openssl-symbols-only fixture (T022)"
```

After T005 + T010 + T019 complete, the integration tasks (T006, T011, T020) follow sequentially in `binary/mod.rs::read()`.

---

## Implementation Strategy

### MVP First (US1 only)

The user's stated core ask is "what's inside a random binary I don't know about" — that's US1's static-link identification. MVP path:

1. Phase 1: Setup (T001–T004)
2. Phase 3: US1 (T005–T009) — embedded version-string extraction
3. Phase 6 partial: T024 (golden regen — only the version-string-component additions, no packer property since US2 not yet done) + T027 (pre-PR gate)
4. **STOP and VALIDATE**: scan a real binary that statically links OpenSSL, confirm `pkg:generic/openssl@<version>` appears.

US2 + US3 layer on after MVP-validation. The full milestone delivers all three signal channels.

### Incremental Delivery (recommended)

Single PR shipping all three stories — the file-level independence + the small total surface (~10 files modified, 3 new test files, 3 new fixtures) make a single PR the right size. Total estimated time: ~1.5 dev-days per the spec's Notes section.

### Single-Developer Strategy

1. T001–T004 (setup, ~5 min)
2. T005–T009 (US1, ~3 hours — version-string regex + integration + fixture + tests)
3. T010–T018 (US2, ~3 hours — packer + parity-catalog C12 + per-format extractors + property emission + tests)
4. T019–T023 (US3, ~3 hours — symbol-fingerprint + composite-evidence merge + tests)
5. T024–T027 (Polish, ~30 min — golden regen + diff audit + pre-PR gate)

Total: ~10 hours single-developer focus. Heavily parallel across the three stories with multiple developers or agents.

---

## Notes

- [P] markers = different files OR different sections within the same file with no implicit dependency.
- [Story] label maps task to specific user story for traceability.
- All three signal channels emit through the same `PackageDbEntry` shape — no new schema-level changes; just new component/property values flowing through existing CDX/SPDX emission paths.
- Composite-evidence merging (Q1) and strict-PURL-equality dedup (Q3) are implemented in `binary/mod.rs::read()` after US1 + US3 both wire their collectors. Implementation-order matters: US1's wiring (T006) must land before US3's composite-merge step (T020).
- The always-emit `mikebom:binary-packer` property (Q2) will cause golden regen for any existing fixture that includes a binary component — that's expected and additive per FR-009; T024 + T025 handle this.
- Toolchain-graceful-skip pattern: when `cc`, `openssl-dev`, `upx`, etc. are missing, tests print a skip reason + return early. Matches the milestone-078 `spdx3-validate` deferred-toolchain convention.
- Pre-PR gate (T027) MUST run with `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1` per CLAUDE.md SBOM-spec-touching-changes rule.
- Commit boundary suggestion: one commit per phase (4–5 commits total) OR squash to a single PR-level commit at merge time.
- Avoid: tuning the v1 pattern/symbol/threshold values in this milestone. Future milestones extend the catalog; SC-007's ≤1 spurious-match bound is the gate on the v1 choices.
