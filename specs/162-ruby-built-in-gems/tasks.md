---
description: "Task list for milestone 162 — Ruby built-in gem edges surfaced as SBOM components"
---

# Tasks: Ruby built-in gem edges surfaced as SBOM components

**Input**: Design documents from `/specs/162-ruby-built-in-gems/`
**Prerequisites**: plan.md (required), spec.md (required), research.md, data-model.md, contracts/annotations.md, quickstart.md

**Tests**: INCLUDED. SC-009 requires ≥10 unit tests; SC-010 requires a new integration test. All test surfaces are load-bearing SC evidence and MUST land alongside the implementation.

**Organization**: Tasks are grouped by the 3 user stories from spec.md (US1 P1 edge-fix, US2 P2 synthetic vs real distinguishability, US3 P3 byte-identity guard). US1 is the load-bearing MVP.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Different files, no dependencies on incomplete tasks — parallelizable.
- **[Story]**: US1 / US2 / US3 for user-story tasks. Setup, Foundational, Polish have NO story label.

## Path Conventions

Single-project workspace layout per plan.md §Project Structure. All paths absolute-relative to repo root `/Users/mlieberman/Projects/mikebom/`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: New Rust types + allowlist const + parser extension for the requirement-string preservation.

- [X] T001 Add `RUBY_BUILT_IN_GEMS: &[&str]` const array to `mikebom-cli/src/scan_fs/package_db/gem.rs` per data-model.md E1 + research.md R2 — union of Ruby 3.2/3.3/3.4 stable-release `Gem::default_gems` outputs, 57 entries in alphabetical order. Add a doc-comment naming the review cadence (~annual per Ruby release) + FR-006 reference.
- [X] T002 Add `is_ruby_built_in_gem(name: &str) -> bool` module-level helper function performing an O(N) linear scan against `RUBY_BUILT_IN_GEMS` per data-model.md E1 in `mikebom-cli/src/scan_fs/package_db/gem.rs`.
- [X] T003 Add NEW `GemDep` struct with `name: String` + `requirement: String` fields per data-model.md E2 in `mikebom-cli/src/scan_fs/package_db/gem.rs`. Update `GemSpec.depends: Vec<String>` → `Vec<GemDep>` in the same edit.
- [X] T004 Change parser at `gem.rs::parse_gemfile_lock` (near line 256) to preserve the version-constraint clause per quickstart.md §3 — split each indent-6 line on whitespace into `(name, raw_req)`; strip `(` and `)` from raw_req; construct `GemDep { name, requirement }` and push to `spec.depends`. **Pre-count** existing construction sites to know the migration scope: run `grep -n 'GemSpec {' mikebom-cli/src/scan_fs/package_db/gem.rs | wc -l` — expect ~10-30 test-side sites (verified during implementation). Update every such site to construct `GemDep { name, requirement: String::new() }` values in the `depends` field, plus adapt any callsite that iterates `spec.depends` for edge construction (e.g., `spec_to_entry`) to extract `.name` via `.iter().map(|d| &d.name)`.
- [X] T004a Verify SC-003 pre-implementation risk: run `grep -iE '^\s+(bundler|bigdecimal|csv|json|logger|openssl|psych|stringio|uri|yaml)\s' <path-to-milestone-090-gem-fixture-Gemfile.lock>` (fixture cache path: `~/.cache/mikebom/fixtures/<pinned-sha>/gem-source-project/Gemfile.lock`). Zero matches → the milestone-090 `gem` fixture goldens will NOT change (SC-003 satisfied for all 33 goldens). Non-zero matches → the `gem` fixture goldens WILL change; document expected diff shape (added C113/C114 annotations only, no edge or component changes) inline in T029. This is a knowledge-only task — no code change; result informs T029 expectations.
- [X] T005 Add NEW `SyntheticGemKind` enum with single variant `RubyBuiltIn` + `as_wire_str(&self) -> &'static str` method returning `"ruby"` per data-model.md E3 in `mikebom-cli/src/scan_fs/package_db/gem.rs`.

**Checkpoint**: Types compile. `cargo +stable check -p mikebom` succeeds. Downstream `spec.depends` consumers (edge construction in `spec_to_entry`) adapted to `.iter().map(|d| &d.name)`. No behavior change yet — synthetic emission helper not yet wired.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Emission helper + parity catalog registration + docs mapping row. All user stories depend on this.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T006 Add NEW `append_synthetic_built_in_gems()` helper function per data-model.md E4 in `mikebom-cli/src/scan_fs/package_db/gem.rs`. Function signature: `fn append_synthetic_built_in_gems(out: &mut Vec<PackageDbEntry>, emitted_names: &HashSet<String>, source_path: &str, specs: &[GemSpec])`. Implements FR-002 (allowlist gate), FR-003 (versionless PURL), FR-004 (real-gem-precedence via `emitted_names` check), FR-005 (requirement annotation), R4 (multi-source union → JSON array).
- [X] T007 [P] Register C113 + C114 (`mikebom:synthetic-built-in`, `mikebom:built-in-requirement`, both component-scope) `cdx_anno!` invocations per contracts/annotations.md §Parity catalog integration in `mikebom-cli/src/parity/extractors/cdx.rs`.
- [X] T008 [P] Register C113 + C114 `spdx23_anno!` invocations per contracts/annotations.md §Parity catalog integration in `mikebom-cli/src/parity/extractors/spdx2.rs`.
- [X] T009 [P] Register C113 + C114 `spdx3_anno!` invocations per contracts/annotations.md §Parity catalog integration in `mikebom-cli/src/parity/extractors/spdx3.rs`.
- [X] T010 Add 2 `ParityExtractor` entries (C113 + C114, both `Directionality::SymmetricEqual`, `order_sensitive: false`) adjacent to the existing C112 block in `mikebom-cli/src/parity/extractors/mod.rs` AND add `c113_cdx`/`c113_spdx23`/`c113_spdx3` + `c114_cdx`/`c114_spdx23`/`c114_spdx3` to the 3 import lines.
- [X] T011 Add C113 + C114 rows to `docs/reference/sbom-format-mapping.md` per contracts/annotations.md §C113 + §C114 wire-format sections — needed to satisfy the `every_mikebom_emitted_field_has_a_map_row` test in `mikebom-cli/tests/sbom_format_mapping_coverage.rs`.

**Checkpoint**: `cargo +stable clippy --workspace --all-targets -- -D warnings` clean. Parity registration + docs-mapping rows in place but synthetic emission not yet wired (T012 in Phase 3 wires it into `gem::read()`).

---

## Phase 3: User Story 1 - test-rails bundler-audit → bundler edge visible (Priority: P1) 🎯 MVP

**Goal**: Wire the synthetic emission helper into `gem::read()`. The `bundler-audit → bundler` edge from the milestone-157 audit MUST now appear in the emitted SBOM as a real edge to a synthetic `pkg:gem/bundler` component.

**Independent Test**: Scan a synthesized Gemfile.lock (via T024 integration test) referencing `bundler-audit@0.9.3` which declares `bundler (>= 1.2.0)` as a dep. Assert:

- Emitted SBOM has `pkg:gem/bundler-audit@0.9.3` (real) AND `pkg:gem/bundler` (synthetic, versionless).
- Synthetic `bundler` carries `mikebom:synthetic-built-in = "ruby"` + `mikebom:built-in-requirement = ">= 1.2.0"`.
- `bundler-audit@0.9.3.dependsOn` includes the `pkg:gem/bundler` reference.

### Synthetic emission wiring for User Story 1

- [X] T012 [US1] Wire the T006 `append_synthetic_built_in_gems()` helper into `gem::read()` at `mikebom-cli/src/scan_fs/package_db/gem.rs` per quickstart.md §4 — build `emitted_names: HashSet<String>` from `out.iter()` filtering on `pkg:gem/` PURLs, then invoke `append_synthetic_built_in_gems(&mut out, &emitted_names, &source_path, &doc.specs)` after the existing per-spec emission loop. Emit an info-level `tracing::info!` log line `"gem built-in synthetic components emitted"` with fields `count` (usize — number of synthetic components emitted this Gemfile.lock) AND `built_in_names` (comma-separated list of allowlist gem names emitted as synthetic, e.g. `"bundler,csv"`), per FR-011.

### Tests for User Story 1

- [X] T013 [P] [US1] Unit test: `is_ruby_built_in_gem("bundler")` returns `true` per SC-009 (a) in `mikebom-cli/src/scan_fs/package_db/gem.rs`.
- [X] T014 [P] [US1] Unit test: `is_ruby_built_in_gem("thor")` returns `false` (thor is a real gem, not built-in) per SC-009 (b) in `mikebom-cli/src/scan_fs/package_db/gem.rs`.
- [X] T015 [P] [US1] Unit test: `is_ruby_built_in_gem("csv")` returns `true` — verifies coverage of a Ruby 3.4-introduced built-in that older projects still reference in `mikebom-cli/src/scan_fs/package_db/gem.rs`.
- [X] T016 [P] [US1] Unit test: parser preserves version-constraint clause — synthesize a Gemfile.lock body with `bundler (>= 1.2.0)` line at indent 6; call `parse_gemfile_lock`; assert the parsed `GemSpec.depends[0]` has `name = "bundler"` AND `requirement = ">= 1.2.0"` (parens stripped) per SC-009 (f) in `mikebom-cli/src/scan_fs/package_db/gem.rs`.
- [X] T017 [P] [US1] Unit test: `append_synthetic_built_in_gems` emits a versionless PURL — synthesize a `GemSpec` with a `bundler` dep; call the helper; assert the emitted entry's PURL is exactly `pkg:gem/bundler` (no `@` symbol) per SC-009 (d) + FR-003 in `mikebom-cli/src/scan_fs/package_db/gem.rs`.
- [X] T018 [P] [US1] Unit test: `append_synthetic_built_in_gems` emits `mikebom:synthetic-built-in = "ruby"` annotation per SC-009 (e) in `mikebom-cli/src/scan_fs/package_db/gem.rs`.
- [X] T019 [P] [US1] Unit test: `append_synthetic_built_in_gems` emits `mikebom:built-in-requirement = ">= 1.2.0"` annotation when the source spec declared the constraint per SC-009 (f) in `mikebom-cli/src/scan_fs/package_db/gem.rs`.
- [X] T020 [P] [US1] Unit test: FR-004 real-gem-precedence — synthesize an `emitted_names` set containing `"bundler"` (as if a real `pkg:gem/bundler@X.Y.Z` was already emitted from GEM/specs); call the helper with a source spec declaring `bundler` dep; assert NO synthetic entry is appended per SC-009 (g) in `mikebom-cli/src/scan_fs/package_db/gem.rs`.
- [X] T021 [P] [US1] Unit test: multi-source dedup — synthesize 3 specs each declaring `bundler` as a dep; call the helper; assert exactly ONE synthetic `pkg:gem/bundler` entry appended (not 3) per SC-009 (h) in `mikebom-cli/src/scan_fs/package_db/gem.rs`.
- [X] T022 [P] [US1] Unit test: multi-source requirement union — synthesize 2 specs where one declares `bundler (>= 1.2.0)` and the other declares `bundler (>= 2.0.0)`; call the helper; assert the synthetic entry's `mikebom:built-in-requirement` value is a JSON-array-encoded string `["\">= 1.2.0\"", "\">= 2.0.0\""]` (sorted, deduplicated) per R4 in `mikebom-cli/src/scan_fs/package_db/gem.rs`.
- [X] T023 [P] [US1] Unit test: FR-008 non-allowlist dropped-target — synthesize a spec declaring `some-unknown-gem` as a dep (not in `RUBY_BUILT_IN_GEMS`); call the helper; assert NO synthetic entry is appended (drop-behavior preserved for non-built-in dangling targets) per SC-009 (j) in `mikebom-cli/src/scan_fs/package_db/gem.rs`.
- [X] T024 [US1] Integration test at `mikebom-cli/tests/ruby_built_in_gems.rs` per SC-010 — synthesize a tempdir with a minimal Ruby project containing a `Gemfile.lock` with `bundler-audit (0.9.3)` in GEM/specs declaring `bundler (>= 1.2.0)` + `thor (~> 1.0)` as deps + `thor (1.4.0)` in GEM/specs. Invoke the release binary via `env!("CARGO_BIN_EXE_mikebom")`, parse the emitted CDX. Assert (a) `pkg:gem/bundler-audit@0.9.3` present as a real component; (b) `pkg:gem/thor@1.4.0` present as a real component; (c) `pkg:gem/bundler` (versionless) present as a synthetic component with `mikebom:synthetic-built-in = "ruby"` + `mikebom:built-in-requirement = ">= 1.2.0"`; (d) `dependencies[]` array shows `bundler-audit@0.9.3 → thor@1.4.0` AND `bundler-audit@0.9.3 → pkg:gem/bundler`.

**Checkpoint**: US1 is fully functional. Running `cargo +stable test --test ruby_built_in_gems` verifies end-to-end that the `bundler-audit → bundler` edge is now present as a real dependency to a synthetic component. All 11 unit tests pass. SC-002 spot-check achieved via T024.

---

## Phase 4: User Story 2 - Synthetic vs real gem distinguishability (Priority: P2)

**Goal**: Verify the dual invariant from SC-004 — every synthetic component has versionless PURL + `mikebom:synthetic-built-in` annotation; every real component has `@version` PURL + no annotation.

**Independent Test**: For every emitted SBOM containing ≥1 `pkg:gem/` component, iterate all components and assert (a) synthetic iff versionless PURL iff annotation present; (b) real iff `@version` PURL iff annotation absent. No false positives on real components claiming synthetic status; no false negatives on synthetic components missing the annotation.

### Tests for User Story 2

- [X] T025 [P] [US2] Unit test: synthetic component has no `@version` in PURL — extend the T017 test to explicitly assert `!entry.purl.as_str().contains('@')` per SC-004 in `mikebom-cli/src/scan_fs/package_db/gem.rs`.
- [X] T026 [P] [US2] Unit test: real GEM/specs entry has `@version` in PURL AND does NOT carry the synthetic-built-in annotation — call `spec_to_entry` with a real `bundler-audit@0.9.3` spec; assert `entry.purl.as_str().contains("@0.9.3")` AND `!entry.extra_annotations.contains_key("mikebom:synthetic-built-in")` in `mikebom-cli/src/scan_fs/package_db/gem.rs`.
- [X] T027 [P] [US2] Unit test: dual invariant is bidirectional — synthesize a fixture with 2 real + 1 synthetic gem components; iterate all `pkg:gem/*` entries; assert for each: `entry.purl.contains('@')` XOR `entry.extra_annotations.contains_key("mikebom:synthetic-built-in")` — either versioned-no-annotation OR versionless-with-annotation, never mixed per SC-004 in `mikebom-cli/src/scan_fs/package_db/gem.rs`.
- [X] T028 [US2] Extend T024 integration test with US2 assertions — after asserting the synthetic + real components exist, iterate all `pkg:gem/*` components and verify the SC-004 dual invariant (versionless PURL iff annotation present).

**Checkpoint**: US2 is fully functional. SC-004 dual invariant verified in both unit and integration tests.

---

## Phase 5: User Story 3 - Non-Ruby scans byte-identical to pre-162 (Priority: P3)

**Goal**: Regression guard. Verify the milestone-090 non-`gem` goldens (10 ecosystems × 3 formats = 30 files) are byte-identical to pre-162.

**Independent Test**: `git diff <pre-162-sha> HEAD -- mikebom-cli/tests/fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/{apk,bazel,cargo,cmake,deb,golang,maven,npm,pip,rpm}.*` produces zero output.

### Golden verification for User Story 3

- [X] T029 [US3] Run `cargo +stable test --workspace --no-fail-fast` after Phase 3+4 land. Inspect the diff for any golden that changed. Expected: zero changes on the 10 non-`gem` goldens. The `gem` fixture golden MAY change if its `Gemfile.lock` references any allowlist gem names — if so, regenerate via `MIKEBOM_UPDATE_CDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo test` and manually inspect the diff to confirm only new C113/C114 annotations appear.
- [X] T030 [US3] Verify SC-003 dual-side byte-identity: `git diff HEAD~ -- mikebom-cli/tests/fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/{apk,bazel,cargo,cmake,deb,golang,maven,npm,pip,rpm}.*` produces exactly ZERO changed lines. Any diff on those 30 goldens indicates an emission-leak bug that needs fixing before proceeding.

**Checkpoint**: SC-003 byte-identity verified. The `gem` fixture may or may not change depending on its Gemfile.lock content.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Documentation, CHANGELOG, pre-PR gate, issue closure.

- [X] T031 [P] Add `CHANGELOG.md` entry per SC-011 documenting: (a) motivation (issue #496 + milestone-157 Round-2 audit), (b) fix summary (allowlist + synthetic emission), (c) new annotation vocab table (C113/C114), (d) empirical impact — pre/post SC-001 numbers on test-rails (99.20% → 100%), (e) consumer jq recipe from contracts/annotations.md, (f) Q1-Q2 clarification bullets.
- [X] T032 [P] Verify T011 `docs/reference/sbom-format-mapping.md` C113/C114 rows match the final wire shape after implementation — no-op if T011 already captured the correct shape.
- [X] T033 Run `./scripts/pre-pr.sh` — both `cargo +stable clippy --workspace --all-targets -- -D warnings` and `cargo +stable test --workspace --no-fail-fast` MUST pass clean (SC-008). Any failure blocks PR opening.
- [ ] T034 Optional SC-001 audit test at `mikebom-cli/tests/gem_built_in_audit.rs` per research.md R5 — gated behind `MIKEBOM_GEM_BUILT_IN_AUDIT=1` env var. If a cached copy of `test-rails`'s `Gemfile.lock` is available, invoke the release binary against it, compute the per-source edge set against ground truth, and assert 100% edge-match. This is OPPORTUNISTIC — the test is skippable if no cached fixture. NOT blocking for the PR. **SC-001 CI verification note**: real-`test-rails` verification of the 99.20% → 100% claim depends on this test running with the fixture available (parallels milestone-160 T033 + milestone-161 T040 fixture-gated audit tests). For this milestone's impl PR, the T024 synthesized fixture provides fully-controlled ground-truth verification of the fix mechanics; the T034 real-world audit is follow-on when the fixture is cached. The PR body per T035 MUST document this: SC-001 fix mechanics verified via T024; SC-001 test-rails-specific claim verified when T034 runs with fixture available.
- [ ] T035 Include `closes #496` in the impl PR body per SC-013 so merging the PR auto-closes the tracking issue.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No prerequisites. Types + parser change + helpers land first.
- **Phase 2 (Foundational)**: Depends on Phase 1. Emission helper + parity catalog + docs mapping. Blocks US1/US2/US3.
- **Phase 3 (US1)**: Depends on Phase 2. Wire the emission helper into `gem::read()` + 11 unit tests + SC-002 integration test.
- **Phase 4 (US2)**: Depends on Phase 3 (needs synthetic entries emitted to test the dual invariant). 4 tests.
- **Phase 5 (US3)**: Depends on Phase 3+4 completion. Byte-identity verification.
- **Phase 6 (Polish)**: Depends on Phases 1-5 completion.

### Within Each User Story

- **US1**: T012 (emission wiring) → T013-T023 (unit tests, mostly parallel) → T024 (integration test).
- **US2**: T025-T027 (unit tests, parallel) → T028 (extend integration test).
- **US3**: T029 (test-run + inspect) → T030 (byte-identity assertion).

### Parallel Opportunities

Genuine `[P]` (different files, no dependencies):

- **Phase 2 T007/T008/T009** — parity registration across 3 different files (cdx.rs, spdx2.rs, spdx3.rs).
- **Phase 3 T013-T023** — 11 unit tests all in the same file (`gem.rs`) BUT non-conflicting append-only fn additions; can be authored in parallel.
- **Phase 4 T025-T027** — 3 unit tests in the same file, non-conflicting.
- **Phase 6 T031/T032** — CHANGELOG + docs updates in different files.

---

## Parallel Example: Phase 2 parity registration

```bash
# T007 + T008 + T009 all edit DIFFERENT files:
Task: "Register C113/C114 cdx_anno! invocations in mikebom-cli/src/parity/extractors/cdx.rs"
Task: "Register C113/C114 spdx23_anno! invocations in mikebom-cli/src/parity/extractors/spdx2.rs"
Task: "Register C113/C114 spdx3_anno! invocations in mikebom-cli/src/parity/extractors/spdx3.rs"

# T010 depends on T007-T009 completing (mod.rs registration references the extractor fns defined by T007-T009).
```

---

## Implementation Strategy

### MVP scope

**US1 alone is the MVP.** Delivers the observable-bug fix (bundler-audit → bundler edge visible). US2's dual-invariant check is a P2 completeness signal; US3 is a regression guard.

Ship order:

1. Phase 1 (Setup) — 1 sitting. Types + parser change + allowlist const.
2. Phase 2 (Foundational) — 1 sitting. Emission helper + parity registration + docs mapping.
3. Phase 3 (US1) — 1 sitting. Emission wiring + 11 unit tests + integration test.
4. **STOP + VALIDATE**: Run T024 integration test. Iterate if failures.
5. Phase 4 (US2) — 1 sitting.
6. Phase 5 (US3) — 1 sitting. Should be trivially green — SC-003 predicts zero changes to non-`gem` goldens.
7. Phase 6 (Polish) — 1 sitting.

### Total effort

~35 tasks. Estimated 3-4 focused sessions total. Significantly smaller than milestones 160 + 161's 51 + 61 tasks respectively — no empirical investigation loop needed.

### Parallel team strategy

With 2 contributors:

- Contributor A: Phase 1 → Phase 2 → Phase 3 US1 core (T012 + T024) — the load-bearing path.
- Contributor B: Phase 3 unit tests (T013-T023, in parallel with A) + Phase 4 US2 tests + Phase 6 docs.

---

## Notes

- All test tasks are load-bearing SC evidence (SC-009 requires ≥10 unit tests; SC-010 requires the integration test). Skipping tests fails the milestone acceptance.
- Unlike milestones 160 + 161, NO empirical investigation is needed. The fix shape is fully specified at plan time.
- Preserve milestone-051's dev-scope classification behavior unchanged. Synthetic built-in components inherit the lifecycle scope of the source's declaration (typically Runtime).
- Constitution Principle IV (`no .unwrap()` in production): all new code follows the milestone-055/091/160/161 pattern with `anyhow::Result` + `?` propagation.
- No new Cargo dependencies (spec Assumption §6). The allowlist is a `const &[&str]` literal; the union logic uses `std::collections::BTreeMap` + `BTreeSet` (already imported in gem.rs).
- Per the closed value vocab for C113 (`"ruby"` only in this milestone): if a future milestone extends to other language runtimes, discuss vocab extension at PR review; don't unilaterally add codes.
