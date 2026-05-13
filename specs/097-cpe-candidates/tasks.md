---
description: "Task list for milestone 097 — CPE candidate emission for binary-identified pkg:generic/<lib>@<version> components"
---

# Tasks: CPE candidate emission for binary-identified components

**Input**: Design documents from `/Users/mlieberman/Projects/mikebom/specs/097-cpe-candidates/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/cpe-emission-contracts.md, quickstart.md

**Tests**: Included. Unit tests in `cpe.rs::tests` cover Contracts 1-5; one integration test covers the SC-007 negative-control bound.

**Organization**: All three user stories converge on a single-file delta to `mikebom-cli/src/generate/cpe.rs`. US1 (P1) is the headline CPE-emission behavior. US2 (P2) is the *maintainability shape* of US1's mapping table — same code change, different framing. US3 (P2) is *automatic* via the existing `cpe.rs:25-28` empty-version fast-return; only a verification test is needed.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: User story this task belongs to (US1–US3)
- File paths are workspace-relative.

## Path Conventions

Production code: `mikebom-cli/src/generate/cpe.rs` (single-file delta per plan.md). Integration test: `mikebom-cli/tests/cpe_binary_id.rs` (NEW). Zero changes to any other production file (FR-008).

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Verify environment + confirm preconditions before touching production code.

- [X] T001 Confirm working branch is `097-cpe-candidates`. Run `git status` + `git log -1 --oneline`; verify branch was created by `/speckit-specify` and main is at post-PR-#203 (alpha.31 release) or later.
- [X] T002 Confirm baseline pre-PR gate passes. Run `./scripts/pre-pr.sh` once on the unchanged tree; expect `>>> all pre-PR checks passed.` Isolates any post-edit failure as introduced by milestone 097.
- [X] T003 Audit existing `cpe.rs` infrastructure per research §1. Run:
    ```bash
    grep -n 'fn synthesize_cpes\|match ecosystem\|return Vec::new()' mikebom-cli/src/generate/cpe.rs | head -5
    grep -n 'component\.cpes\|c\.cpes' mikebom-cli/src/generate/cyclonedx/builder.rs mikebom-cli/src/generate/spdx/packages.rs mikebom-cli/src/generate/spdx/v3_external_ids.rs | head -5
    ```
    Expected: `synthesize_cpes` defined; `match ecosystem` block present with a `_ => return Vec::new();` catch-all that the new `"generic"` arm will go *before*; all three emission files already consume `component.cpes`. Confirms zero emission-side changes needed.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: No shared infrastructure across all three user stories — the mapping table addition lives in US1's phase (T004) because the maintainability *shape* of that table IS US2's implementation. US3 is satisfied by the existing empty-version fast-return at `cpe.rs:25-28`; no new code path.

(No tasks in this phase — file-level convergence between US1 / US2 / US3.)

**Checkpoint**: US1, US2, US3 share the same edit to `cpe.rs`. Implementation order: T004 (table) → T005 (arm) → tests. US2 is satisfied as a side-effect of T004's maintainability requirements; US3 is satisfied as a side-effect of the existing empty-version fast-return (no new code path).

---

## Phase 3: User Story 1 — Operator runs Trivy against a mikebom SBOM and CVEs land (Priority: P1) 🎯 MVP

**Goal**: For every binary-extracted `pkg:generic/<lib>@<version>` component whose `<lib>` is in the v1 mapping table, emit a canonical CPE 2.3 string on CDX `component.cpe`, SPDX 2.3 `cpe23Type` external ref, and SPDX 3 `Software:cpe` external-id. 10 libraries in v1: openssl / zlib / sqlite / curl / pcre / pcre2 / gnutls / libressl / llvm / openjdk.

**Independent Test**: synthesize an SBOM containing `pkg:generic/openssl@3.0.13`; inspect the CDX JSON; confirm `components[?(@.purl=='pkg:generic/openssl@3.0.13')].cpe == 'cpe:2.3:a:openssl:openssl:3.0.13:*:*:*:*:*:*:*'`. (Toolchain-graceful end-to-end check: feed the SBOM to a current Trivy binary and confirm the OpenSSL 3.0.13 CVE list appears; skip if `trivy` unavailable.)

### Implementation for User Story 1

- [X] T004 [US1] Add the `GENERIC_LIBRARY_CPES` const table to `mikebom-cli/src/generate/cpe.rs` per `data-model.md §cpe.rs — extension shape`. 10 rows, sorted alphabetically by library_slug for diff-friendliness per FR-002. Include the in-source NVD-citation `//` comment per row (research §2). The table goes above `synthesize_cpes` near the existing `use` statements.
- [X] T005 [US1] Add the `"generic"` ecosystem arm to the `match ecosystem` block inside `synthesize_cpes` in `mikebom-cli/src/generate/cpe.rs`. Insert immediately BEFORE the existing `_ => { return Vec::new(); }` catch-all. Arm body per `data-model.md §cpe.rs — extension shape`: lookup by library_slug → `Option<&[(vendor, product)]>`; missing → empty Vec (FR-003 silent-skip); special-case `openjdk` to strip build-suffix before `format_cpe()`; emit one CPE per mapping pair.

### Tests for User Story 1

- [X] T006 [P] [US1] Add unit test `generic_openssl_emits_canonical_cpe` to `mikebom-cli/src/generate/cpe.rs::tests` per `data-model.md`. Asserts `synthesize_cpes(pkg:generic/openssl@3.0.13)` returns `["cpe:2.3:a:openssl:openssl:3.0.13:*:*:*:*:*:*:*"]`.
- [X] T007 [P] [US1] Add unit test `generic_curl_emits_dual_candidates` to `mikebom-cli/src/generate/cpe.rs::tests`. Asserts multi-vendor emission: `pkg:generic/curl@8.4.0` → `[haxx:curl, curl:curl]` (both pairs present, declaration order preserved).
- [X] T008 [P] [US1] Add unit test `generic_openjdk_strips_build_suffix` to `mikebom-cli/src/generate/cpe.rs::tests`. Asserts `pkg:generic/openjdk@21.0.1+12` → `["cpe:2.3:a:oracle:openjdk:21.0.1:*:*:*:*:*:*:*"]` (suffix stripped; PURL unchanged).
- [X] T009 [P] [US1] Add unit test `composite_evidence_emits_single_cpe` to `mikebom-cli/src/generate/cpe.rs::tests` covering FR-005 / SC-004. Constructs a single `ResolvedComponent` with `purl = pkg:generic/openssl@3.0.13` (representing the milestone-096 Q1 composite-merge output where both version-string + symbol-fingerprint matched), calls `synthesize_cpes(c)`, asserts `len() == 1` and `cpes[0] == "cpe:2.3:a:openssl:openssl:3.0.13:*:*:*:*:*:*:*"`. Verifies that milestone-096's Q1 merge correctly produces ONE PackageDbEntry → ONE CPE field downstream — no duplicate emission for the symbol-fingerprint half of the evidence trail.
- [X] T010 [US1] Verify Contract 1 from `contracts/cpe-emission-contracts.md`. Run:
    ```bash
    cargo +stable test -p mikebom --bin mikebom \
        --no-fail-fast generic_openssl_emits_canonical_cpe \
        generic_curl_emits_dual_candidates \
        generic_openjdk_strips_build_suffix \
        composite_evidence_emits_single_cpe 2>&1 | grep "test result:"
    # Expected: ok. 4 passed.
    ```

**Checkpoint**: US1 complete. MVP win lands — Trivy/Grype/DT can now match advisories on binary-extracted OpenSSL components.

---

## Phase 4: User Story 2 — Maintainer extends the CPE vendor/product table (Priority: P2)

**Goal**: The mapping table is structured so adding a new library is a one-line PR. Table is a single in-source `const`, alphabetically sorted, with per-row NVD-citation comments. Build-time test asserts the table covers every curated library slug from the milestone-096 + earlier version-string scanner.

**Independent Test**: insert a new mapping row in `GENERIC_LIBRARY_CPES`; recompile; confirm a unit test calling `synthesize_cpes(pkg:generic/<new-slug>@1.2.3)` returns the expected CPE.

**Note**: T004 (the table itself) is shared with US1 because the maintainability *shape* of US1's table IS the US2 implementation. Phase 4 only adds the well-formedness checks that lock the maintainability contract in.

### Tests for User Story 2

- [X] T011 [P] [US2] Add unit test `mappings_alphabetically_sorted` to `mikebom-cli/src/generate/cpe.rs::tests`. Walks `GENERIC_LIBRARY_CPES` and asserts `.windows(2).all(|w| w[0].0 < w[1].0)` to keep diffs friendly per FR-002. This is a build-time gate: if a new row breaks alphabetical order, the test fails and the maintainer reorders.
- [X] T012 [P] [US2] Add unit test `mappings_all_emit_valid_cpe23` to `mikebom-cli/src/generate/cpe.rs::tests` covering SC-002. Iterates every row in `GENERIC_LIBRARY_CPES`; for each `(library_slug, vendors)` pair builds a synthetic `ResolvedComponent` with `purl = format!("pkg:generic/{slug}@1.2.3")`; calls `synthesize_cpes`; asserts the returned Vec has length ≥1 and every emitted CPE string starts with `"cpe:2.3:a:"` and contains exactly 12 unescaped colon separators (CPE 2.3 has 13 segments). Locks the FR-006 "syntactically valid CPE 2.3" invariant at build time across the entire table — protects against a future maintainer adding a row whose vendor/product string breaks `format_cpe()` shape.
- [X] T013 [P] [US2] Add unit test `mappings_cover_all_curated_libraries` to `mikebom-cli/src/generate/cpe.rs::tests`. Walks `version_strings::CuratedLibrary` variants (via `slug()`), checks each against `GENERIC_LIBRARY_CPES` keys, and asserts every slug is either present in the table OR present in a documented-omission allowlist (`["boringssl"]` per spec). Catches the "scanner-team-added-a-library-but-forgot-the-CPE-row" regression at build time. **Implementation note (A1)**: `version_strings::CuratedLibrary` is `pub` and `slug()` is `pub fn` per audit; reach it via `crate::scan_fs::binary::version_strings::CuratedLibrary`. If module-visibility blocks the import at test time, fall back to a hardcoded local mirror of the slug list with a `//` comment pointing at `version_strings.rs` as the source of truth.
- [X] T014 [US2] Verify Contract 5 from `contracts/cpe-emission-contracts.md`. Run:
    ```bash
    cargo +stable test -p mikebom --bin mikebom \
        --no-fail-fast mappings_alphabetically_sorted \
        mappings_all_emit_valid_cpe23 \
        mappings_cover_all_curated_libraries 2>&1 | grep "test result:"
    # Expected: ok. 3 passed.
    ```

**Checkpoint**: US2 complete. Maintainability contract locked in via build-time tests.

---

## Phase 5: User Story 3 — Symbol-fingerprint-only binary's SBOM withholds the CPE (Priority: P2)

**Goal**: Components without a captured version (`pkg:generic/<library>` shape from milestone-096 symbol-fingerprint, or SQLite source-id-only edge case) emit NO CPE. Wildcard-version CPEs would over-match every NVD record for the vendor:product pair.

**Independent Test**: synthesize a component with empty `version` field; call `synthesize_cpes`; expect empty Vec.

**Note**: this US is *automatic* via the existing `cpe.rs:25-28` empty-version fast-return path (no new code). T015's test plus the regression of the existing `empty_version_returns_empty` test (already passing) cover the invariant.

### Tests for User Story 3

- [X] T015 [P] [US3] Add unit test `generic_symbol_fingerprint_only_emits_no_cpe` to `mikebom-cli/src/generate/cpe.rs::tests`. Constructs a component with `pkg:generic/openssl@dummy`, clears its `version` field, calls `synthesize_cpes`, asserts empty Vec returned. Documents in the test body the FR-004 + symbol-fingerprint-only / SQLite-source-id-only inheritance.
- [X] T016 [P] [US3] Rename existing `unknown_ecosystem_returns_empty` → `generic_unknown_library_returns_empty` in `mikebom-cli/src/generate/cpe.rs::tests` to reflect post-097 semantics. Existing assertion (`pkg:generic/weird@1.0.0` → empty Vec) is preserved — `weird` is not in the new `GENERIC_LIBRARY_CPES` table, so the test still validates FR-003 silent-skip behavior.
- [X] T017 [US3] Verify Contracts 2 and 3 from `contracts/cpe-emission-contracts.md`. Run:
    ```bash
    cargo +stable test -p mikebom --bin mikebom \
        --no-fail-fast generic_symbol_fingerprint_only_emits_no_cpe \
        generic_unknown_library_returns_empty \
        empty_version_returns_empty 2>&1 | grep "test result:"
    # Expected: ok. 3 passed.
    ```

**Checkpoint**: US3 complete. Symbol-fingerprint-only suppression verified; existing empty-version + missing-table-entry paths covered by tests.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Integration test for the SC-007 spurious-emission bound; final pre-PR gate; diff-scope guardrails.

- [X] T018 [P] Create `mikebom-cli/tests/cpe_binary_id.rs` per `data-model.md §cpe_binary_id.rs — NEW integration test`. The test (`mikebom_self_scan_emits_no_spurious_openssl_cpe`):
    1. Copies the mikebom binary itself into a temp dir
    2. Runs `mikebom sbom scan --path <tempdir> --output <file> --no-deep-hash`
    3. Asserts the emitted CDX JSON contains NO `cpe:2.3:a:openssl:openssl:` substring (mikebom uses rustls, not OpenSSL)
    Negative control for SC-007 — guards against the milestone-096 binary scanner or the milestone-097 CPE table firing spuriously on mikebom's own bytes.
- [X] T019 Verify Contract 6 — diff scope guardrails. Run:
    ```bash
    # No new Cargo deps (FR-007):
    git diff --name-only main | grep -E '^Cargo\.(lock|toml)$' | wc -l
    # Expected: 0

    # Production code outside generate/cpe.rs:
    git diff --name-only main | grep -E '^mikebom-cli/src/' \
      | grep -vE '^mikebom-cli/src/generate/cpe\.rs$' \
      | wc -l
    # Expected: 0

    # Golden regen scope (FR-009 / SC-006):
    git diff --stat mikebom-cli/tests/fixtures/golden/ | tail -1
    # Expected: empty (no goldens regenerated — research §5 forecast)

    # Diff scope allowlist:
    git diff --name-only main | sort
    # Expected only:
    #   mikebom-cli/src/generate/cpe.rs
    #   mikebom-cli/tests/cpe_binary_id.rs
    #   specs/097-cpe-candidates/...
    #   CLAUDE.md
    ```
- [X] T020 Run the mandatory pre-PR gate per Contract 7. Run `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 ./scripts/pre-pr.sh`. Expect: `>>> all pre-PR checks passed.` with zero clippy warnings and zero test failures across the workspace. The new test file (`cpe_binary_id.rs`) reports its 1 test passing; the 8 new unit tests in `cpe.rs::tests` all pass; the SPDX 3 conformance validator passes against the unchanged goldens.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies. Start immediately.
- **Foundational (Phase 2)**: No tasks (file-level convergence — the mapping table addition is part of US1's edit).
- **US1 (Phase 3, P1, MVP)**: Independent. Touches `cpe.rs` only.
- **US2 (Phase 4, P2)**: Requires T004 (the table) from US1 to exist. T011/T013 are test-only additions.
- **US3 (Phase 5, P2)**: Independent at the code level — relies on the existing empty-version fast-return, not the new arm. T015/T016 are test-only additions.
- **Polish (Phase 6)**: Requires US1+US2+US3 implementation tasks complete. T018 integration test + T019 diff audit + T020 pre-PR gate.

### User Story Dependencies

- **US1 (P1)**: Independent at file level. Touches `cpe.rs` (T004 + T005).
- **US2 (P2)**: Depends on US1's table existing (T004). Adds well-formedness tests only.
- **US3 (P2)**: Independent — relies on existing code path, not the new arm.

### Within Each User Story

- US1: T004 + T005 are sequential (same file, T005 references T004's table). T006+T007+T008+T009 are parallel-safe (independent unit-test functions). T010 verifies after T006-T009.
- US2: T011 + T012 + T013 are parallel-safe (independent unit-test functions). T014 verifies after T011-T013.
- US3: T015 + T016 are parallel-safe. T017 verifies after T015-T016.

### Parallel Opportunities

- T006 / T007 / T008 / T009 / T011 / T012 / T013 / T015 / T016 — 9 parallel-safe unit tests across all three stories (different test functions, no in-file conflicts since all live in the same `cpe.rs::tests` module). Strong fan-out potential for an agent-parallelism path.
- T018 (integration test file creation) is parallel-safe with any of the unit-test tasks.

---

## Parallel Example: Phase 3-5 (US1 + US2 + US3 tests)

```bash
# Implementation tasks (sequential — same file):
Task: "Add GENERIC_LIBRARY_CPES table to cpe.rs (T004)"
Task: "Add 'generic' ecosystem arm to synthesize_cpes (T005)"

# Test tasks (parallel — independent test functions):
Task: "Add unit test generic_openssl_emits_canonical_cpe (T006)"
Task: "Add unit test generic_curl_emits_dual_candidates (T007)"
Task: "Add unit test generic_openjdk_strips_build_suffix (T008)"
Task: "Add unit test composite_evidence_emits_single_cpe (T009)"
Task: "Add unit test mappings_alphabetically_sorted (T011)"
Task: "Add unit test mappings_all_emit_valid_cpe23 (T012)"
Task: "Add unit test mappings_cover_all_curated_libraries (T013)"
Task: "Add unit test generic_symbol_fingerprint_only_emits_no_cpe (T015)"
Task: "Rename unknown_ecosystem_returns_empty test (T016)"
Task: "Create cpe_binary_id.rs integration test (T018)"
```

After T004 + T005 land, all test additions can land in any order.

---

## Implementation Strategy

### MVP First (US1 only)

The user's stated payoff is "CVE matching for binary-extracted OpenSSL". MVP path:

1. Phase 1: Setup (T001-T003)
2. Phase 3: US1 (T004-T010) — table + arm + 4 unit tests + verification
3. Phase 6 partial: T020 (pre-PR gate)
4. **STOP and VALIDATE**: feed an SBOM containing `pkg:generic/openssl@3.0.13` to Trivy, confirm CVEs appear.

US2 + US3 layer on after MVP-validation. The full milestone delivers all three stories in a single PR — small surface (~30 lines of new code, 8 new tests, 1 integration test).

### Incremental Delivery (recommended)

Single PR shipping all three stories — the single-file delta + parallel-safe tests make this the right size. Total estimated time: ~1 dev-hour.

### Single-Developer Strategy

1. T001-T003 (setup, ~5 min)
2. T004-T010 (US1, ~25 min — table + arm + 4 tests + verification)
3. T011-T014 (US2, ~15 min — well-formedness + all-valid + coverage tests + verification)
4. T015-T017 (US3, ~10 min — empty-version tests + verification)
5. T018-T020 (Polish, ~10 min — integration test + pre-PR gate + diff audit)

Total: ~55 minutes single-developer focus. Heavily parallel across the 7 unit-test tasks with an agent or multiple developers.

---

## Notes

- [P] markers = different test functions OR different files with no shared edit-dependency.
- [Story] label maps task to the user story for traceability.
- All three signal paths (US1 emit, US2 maintainability, US3 suppression) converge on the same `cpe.rs` file — but the *code surface* is fully captured by T004+T005. US2 and US3 are test-only stories layered on top of the same implementation.
- The mapping-completeness test (T013) is a tripwire for milestone-098+ — when a future milestone adds a new library to `version_strings::CuratedLibrary`, the test fails and forces the maintainer to add the CPE row in the same PR.
- Commit boundary suggestion: single commit (US1+US2+US3+Polish in one PR) per the incremental-delivery guidance. Surface is small enough that splitting adds noise.
- Pre-PR gate (T020) MUST run with `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1` per CLAUDE.md SBOM-spec-touching-changes rule. CPE arrays on SPDX 3 are standards-native; conformance validator already accepts them.
- Avoid: extending the v1 mapping table during this milestone (e.g., adding libpng, freetype). New libraries land in milestone-098+ as the version-string scanner expands. v1's purpose is the *mechanism*; the table grows on its own cadence.
