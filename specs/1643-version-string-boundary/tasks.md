# Tasks: An embedded version string is found when a byte other than NUL precedes it (#1568, #1641)

**Input**: `specs/1643-version-string-boundary/` (spec.md, plan.md, research.md, data-model.md, contracts/wire.md, quickstart.md)

**Tests**: requested by the spec (SC-002, SC-003, SC-004, SC-005). Every case in `contracts/wire.md` whose `main` column differs from its expected result is written at the log-only commit and must fail there before the change that fixes it. The others are guards that pass throughout.

**Paths**:
- `VS` = `waybill-cli/src/scan_fs/binary/version_strings.rs`
- `BM` = `waybill-cli/src/scan_fs/binary/mod.rs` (`prepare_held`, the call site at line ~1107)
- `IT` = `waybill-cli/tests/version_string_boundary.rs` (new)
- `IL` = `waybill-cli/tests/common/installed_lib.rs` (`elf_shared_lib`, `scan`, `components`, `write`; unchanged)
- `MS` = `specs/1643-version-string-boundary/measurements/`
- `FR` = the TheRock fixture repository, `kusari-sandbox/waybill-fixture-therock-sdk`
- Research decisions are R1–R9. Cases are `contracts/wire.md`'s.

## Phase 1: Setup

- [X] T001 `main`'s binary is already built at `83657af2` and saved as the scratchpad's `waybill-main` (R1). Rename it `waybill-main-83657af2` and record the full SHA in `MS/control.md`. Create `IT` with `mod common;` and helpers built on `IL`:
  - `lib_dir(bytes, file_name) -> TempDir`: writes `elf_shared_lib(bytes)` as the named file in a fresh temp directory;
  - `scan3(dir)`: all three formats in one invocation, honouring `WAYBILL_VERSION_BOUNDARY_BIN` so a case can run against the saved `main` binary;
  - `scan_with_log(dir, filter) -> (docs, stderr)`: `scan3` with `RUST_LOG=<filter>` set and stderr captured, for C-1 and C-2;
  - `curated(doc, slug) -> Vec<String>`: sorted PURLs of components named `slug`, searched recursively (CycloneDX components nest);
  - `parity(dir)`: runs `waybill sbom parity-check` on the three documents and asserts agreement;
  - `unique_bom_refs(doc)`.
- [X] T002 B-1 (FR-006): one library holding NUL-preceded zlib, expat and ncurses banners together. Generate goldens `waybill-cli/tests/fixtures/version_string_boundary/b1-golden/{cdx,spdx-2.3,spdx-3}.json` with the saved `main` binary (`WAYBILL_UPDATE_VERSION_BOUNDARY_GOLDENS`). Assert byte-identity with per-run fields masked as `waybill-cli/tests/requirements_owner.rs` masks them. It passes at `main`.

## Phase 2: Foundational (log-only census)

Output does not change in this phase.

- [X] T003 In `VS`, add the following. `match_prefix` does not call any of them yet.
  - **`Boundary`** (`Anchored`, `NulOnly`) with `accepts(self, region, pos)` (data-model).
  - **`CuratedLibrary::boundary`**, an exhaustive `match` with no wildcard arm (R3). Its table is FR-001's.
  - **`is_identifier(b: u8) -> bool`** for `[A-Za-z0-9_]`.
  - **`census(region) -> Vec<CensusHit>`** (R5). Apply every pattern's prefix and its existing version parser at every position, with no boundary test. Record:
    - `preceding` (`start`, `nul`, `whitespace`, `other_non_identifier` or `identifier`). For zlib's leading-space form, this is the byte before the space.
    - `accepted_main`, from `main_rule_accepts`: a frozen copy of `main`'s rule as `match_prefix` applies it today, in its own function, so it still means `main` after T007;
    - `accepted_new`, by FR-002/FR-003;
    - 24 bytes of context each side, hex-encoded.

    Reuse the parsers; do not copy them.

  In `BM::prepare_held`, when `tracing::enabled!(target: "waybill::version_census", tracing::Level::DEBUG)`, emit one `tracing::debug!(target: "waybill::version_census", path = %path.display(), hit = <JSON string>)` per hit.

  **Unit tests in `VS`:** `census` reports
  - `nul` and `accepted_main=true` for a NUL-preceded ncurses banner;
  - `other_non_identifier`, `accepted_main=false` and `accepted_new=true` behind `-`;
  - `whitespace` and `accepted_new=true` behind a space;
  - `identifier` and `accepted_new=false` behind `x`;
  - for OpenSSL behind `-`: `accepted_new=false`.

  Every existing suite passes unchanged: `cargo test -p waybill version_strings`, `--test installed_lib_us3`, `--test python_owned_binaries` and `--test orphan_explanations`. B-1 passes too.
- [X] T004 C-1, the census positive control, in `IT`. Scan a library holding `-ncurses 6.6.20251230\0` with `RUST_LOG=waybill::version_census=debug`. Assert exactly one event with `library=ncurses`, `preceding=other_non_identifier`, `accepted_main=false` and `accepted_new=true`. C-2: the same with a space before the banner gives exactly one event with `preceding=whitespace`. Match on the raw field text, written so that Debug-escaped quotes cannot make it pass vacuously; spec 1642's T004 needle failed that way.
- [X] T005 **Census at the log-only commit** (SC-001, R5, R6).
  - **Write the census tooling in `FR`.** Add `census/versions.py` (stdlib, run as `python3 -I -B`, ANSI-stripped) and `.github/workflows/census-versions.yml`.
    - The script reads only the events; it never re-scans bytes.
    - **Matrix:** the full SDK tarball, downloaded and size-checked exactly as `measure-full.yml` does, plus the 18 quality-corpus targets at the pins in `census-owners.yml`.
    - **Tally per target:**
      - hits per library, by `preceding`;
      - every hit where `accepted_new != accepted_main`, with path, library, version and both contexts, whitespace-preceded hits listed separately;
      - every binary that would gain or change a curated version;
      - any binary that would newly carry two versions of one library.
    - **Each job uploads** its documents, its log and `refs.txt`.
  - **Dispatch** with `--ref main -f waybill_ref=<full sha>`. Enumerate every job's conclusion, and read the scanned SHA from each `refs.txt`.
  - **Public corpus** at the same commit, `regen_goldens=true`. 0 differing files are expected, which confirms the commit is log-only. Enumerate every job.
  - **Judge each new match** from its contexts: is it the library's own banner (the literal followed by its version and terminator in a string table) or a mention inside other text?
  - **Record** everything in `MS/census.md`. No prediction is written before the runs.

**Checkpoint**: census recorded. Every whitespace-preceded new match is judged individually (a sentence mention is the likeliest false positive). Any false positive on an anchored pattern moves that pattern to `NulOnly` (FR-005); amend spec.md FR-001, `contracts/wire.md` and this file before Phase 3. A binary that newly carries two versions of one library is traced before Phase 3.

## Phase 3: User Story 1: a bundled zlib or ncurses is versioned whatever byte precedes its banner (P1) 🎯 MVP

**Goal**: FR-001, FR-002, FR-004. **Independent test**: Z-1 to Z-3, Z-6, N-1 to N-3, E-1, E-2, and T007's unit tests.

- [X] T006 [US1] Write Z-1, Z-2, Z-3, Z-6, N-1, N-2, N-3, E-1 and E-2 in `IT`, each asserting the curated PURL, `parity` and `unique_bom_refs`. File names are `libwaybill-fixture-z.so.1`, `libwaybill-fixture-ncurses.so.6` and `libwaybill-fixture-expat.so.1`.

  **Run them at the log-only commit.** Z-2, Z-3, N-2, N-3 and E-2 must fail on their stated assertion; Z-1, Z-6, N-1 and E-1 must pass. Record each case's result in `MS/control.md`. If a case expected to fail passes, its control is wrong: stop and rebuild the case before T007.
- [X] T007 [US1] (Implementation amendment: at T003, `match_prefix` became `pattern_at`, which finds the pattern starting at a position, followed by the boundary rule, so that the census reuses the parsers. T007 swapped `main_rule_accepts` for `new_rule_accepts`. After SC-006 failed, `match_prefix` narrows the patterns by the preceding byte first (`measurements/therock.md`). There is no `at_boundary` early return any more; the outcome is the one described below.) Wire the rule in `VS::match_prefix` (R3, R4):
  - **expat and ncurses** move above the `if !at_boundary { return None; }` early return, each gated by `CuratedLibrary::X.boundary().accepts(region, pos)`.
  - **zlib**'s `after_one_space` changes `region[pos - 2] == 0` to `!is_identifier(region[pos - 2])`. The no-space form keeps `at_boundary`.
  - **The twelve `NulOnly` patterns** stay below the early return, unchanged.
  - **Update the module doc table** in `VS` with a boundary column.

  **Unit tests in `VS`:**
  - each anchored pattern behind `\0`, `-`, `\xff`, ` ` and `/` matches;
  - behind `x`, `9` and `_` it does not;
  - at region offset 0 it matches;
  - **census agrees with the scanner (I1):** for every fixture banner and every preceding byte above, the census hits with `accepted_new=true` are exactly `scan()`'s matches;
  - **the R3 iteration test:** a test-only `fn sample(lib: CuratedLibrary) -> &'static [u8]` (exhaustive `match`) gives each variant's canonical banner. For every `CuratedLibrary::iter()` variant, the banner behind NUL matches, and behind `-` it matches exactly when `boundary()` is `Anchored`.
- [X] T008 [US1] Run the US1 cases and T007's unit tests until they pass. Also run the existing #1525/#1526 tests in `VS`, `installed_lib_us3`, `python_owned_binaries`, `orphan_explanations`, `generate::cpe`'s `mappings_cover_all_curated_libraries` and B-1. An existing assertion whose expectation changes is recorded with its reason in the commit message. None may be loosened otherwise.

## Phase 4: User Story 2: a library name inside other text is still not a match (P1)

**Goal**: FR-002 (identifier rejection), FR-003. **Independent test**: Z-4, Z-5, Z-7, N-4, E-3, U-1, U-2.

- [X] T009 [US2] Write Z-4, Z-5, Z-7, N-4, E-3, U-1 and U-2 in `IT`. These are guards: they pass at the log-only commit, and are run there and recorded in `MS/control.md`. Each asserts the absence of the versioned PURL, not of every component with that name: the symbol fingerprint may still report an unversioned `zlib` (`contracts/wire.md`).
- [X] T010 [US2] Run T009's cases after T007 until they pass, with T007's unit tests for the other eleven `NulOnly` patterns.

## Phase 5: User Story 3: nothing else changes (P2)

**Goal**: FR-006, FR-007. **Independent test**: B-1, B-2.

- [X] T011 [US3] Run B-1 and B-2 (`installed_lib_us3`, `python_owned_binaries`, `orphan_explanations`, the #1525/#1526 unit tests) unchanged after T007. Confirm with `git diff origin/main -- docs/reference/sbom-format-mapping.md waybill-cli/src/parity` that no catalog row or extractor changed (FR-007).

## Phase 6: Polish and cross-cutting

- [X] T012 [P] Docs:
  - **`docs/reference/binary-identity.md` § Embedded-version-string detection.** Its sentence "anchored at a NUL boundary" becomes false with this change, so replace it with the two rules and the three anchored libraries. The paragraph's library list ("11 … libraries") omits PHP, Node.js, expat and ncurses; bring it to the 15 variants `CuratedLibrary` has, since the paragraph is being rewritten.
  - **`CHANGELOG.md` `## [Unreleased]`:** a Fixed entry (#1568, #1641).
- [X] T013 Spec record: add the census figures to `research.md` R5/R6 and resolve FR-005 against them. No figure is written before T005.
- [X] T014 Mutations (SC-003, R8): `MS/mutate.py`, modelled on `specs/1642-requirements-owner-editable/measurements/mutate.py`.
  - Run M1–M7, each against the test named in R8's table.
  - Back up and restore with `cp`, rebuild after the last restore, and use a `^error\[E` guard so that a mutation that fails to compile is reported as such, not as killed.
  - Record in `MS/mutations.md`.
- [X] T015 Re-run `MS/probe.sh` at the wired commit on the same Nix-built libraries (R1). Expect `00`, `2d` and `ff` to be versioned for both libraries, and `78` unchanged. Run 3 times each, and append to `MS/probe.md`.
- [X] T016 Gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with no `+toolchain`. Record `GATE_RESULT` and every `test result:` count in `MS/gate-log.md`. (`target/` was cleaned after spec 1642, so the first run rebuilds everything.)
- [X] T017 Public corpus at the final commit (SC-005): `regen_goldens=true`. Enumerate the jobs and confirm the scanned SHA. This diff is the public corpus's census (it has no T005 prediction): judge each changed component as T005 judges a new match.
  - `cmp` every regenerated golden against the committed one.
  - Trace every changed component to one binary and one anchored pattern, and check it against T005's census.
  - Commit any changed goldens from the artifact.
  - Record in `MS/corpus.md`.
- [X] T018 TheRock and the quality corpus (SC-005, SC-006).
  - **TheRock:** `measure-full.yml` with `--ref main -f waybill_ref=<branch full sha> -f base_ref=<main full sha>`.
    - Confirm both SHAs from the `base <sha>` and `branch <sha>` lines of the "Build both" step.
    - Compare the documents with per-run fields masked, and trace every changed component to one binary and one anchored pattern, against T005's census.
    - Component counts must be equal, apart from new versioned curated components that the census predicts.
    - Record the median of at least 3 runs per arm, and the ratio, which must be at most 1.05.
  - **Quality corpus:** dispatch `census-versions.yml` at the wired commit and compare its documents with T005's, which are the log-only control. Trace every difference.
  - Record in `MS/therock.md` and `MS/corpus.md`.
- [X] T019 Verify each task's claim against the code and measurements before ticking it. Confirm `git diff --quiet origin/main -- Cargo.lock '*/Cargo.toml' Cargo.toml` (zero new dependencies). Open the PR with "Closes #1568" and "Refs #1641, #1525, #1526, #1543", ending with the attribution footer. Merging is the maintainer's.

## Dependencies

- **Setup:** T001 → T002, with `main`'s binary saved before any source change.
- **Foundational:** T003 → T004 → T005. The census gates Phases 3 and 4.
- **US1:** T006 at the log-only commit, then T007 → T008.
- **US2:** T009 at the log-only commit (write it with T006), then T010 after T007.
- **US3:** T011 after T008 and T010.
- **Polish:**
  - T012 after T007;
  - T013 after T005;
  - T014 after T011;
  - T015 after T007;
  - T016 after T014;
  - T017 and T018 after T016;
  - T019 last.

## Parallel examples

- T006 and T009 (writing tests) together at the log-only commit, as separate test functions in `IT`.
- T012 alongside T014 and T015.
- T017 and T018 dispatched together.

## Implementation strategy

- **MVP:** Phases 1–3. The census is recorded, and vendor-built zlib, expat and ncurses are versioned whatever byte precedes their banners.
- **Then:**
  - Phase 4 confirms the guards against mentions;
  - Phase 5 confirms nothing else moved;
  - Phase 6 carries the corpus and TheRock evidence and the timing.
