---

description: "Task list for m216 — emit main-module component for Gemfile-only Ruby applications (fixes waybill#629 gap from m215 real-world validation)"
---

# Tasks: Emit main-module for Gemfile-only Ruby applications

**Input**: Design documents from `/specs/216-gemfile-main-module/`
**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/), [quickstart.md](./quickstart.md)

**Tests**: Test tasks INCLUDED. The user stories are verification perspectives on shared infrastructure; the tests are what prove each perspective is satisfied. Integration tests + reproducer validation dominate.

**Organization**: 2 user stories. US1 (P1 split-mode) is the discovered gap that motivated the feature. US2 (P2 single-SBOM root selection) is a bonus fix from the same reader change; the same code path serves both.

**Solo-dev sequencing note**: Both user stories share the SAME foundational reader change (walker + builder). Once Foundational (Phase 2) completes, US1 and US2 diverge only in the tests they add. Recommended order: US1 → US2 (US1 is the P1 gap; US2 tests slot in easily after).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story (US1, US2)
- File paths absolute-from-repo-root

---

## Phase 1: Setup

**Purpose**: Sanity checks + branch verification + reference-code reading.

- [X] T001 Verify branch `216-gemfile-main-module` is checked out and up-to-date with `main` post-#630 merge. Confirm HEAD is the plan-phase commit via `git log -1 --oneline`.
- [X] T002 Verify pre-feature single-SBOM emit path is green: `cargo test -p waybill --test cdx_regression --test spdx_regression --test spdx3_regression`. Should print `11 passed 0 failed` × 3 suites. The m216 reader change must not regress these — SC-004 gate.
- [X] T003 Read `waybill-cli/src/scan_fs/package_db/gem.rs:1128-1288` (m069 gemspec-loop implementation reference). Note the exact code shape of `find_top_level_gemspecs` + `build_gem_main_module_entry` + the dispatch loop at `read()` line 1049+ — the m216 addition mirrors this structure with an application-loop appended after the existing gemspec-loop.
- [X] T004 Read `waybill-cli/src/scan_fs/package_db/golang.rs` and locate the `git describe` helper (m053). Note function signature + subprocess-timeout pattern — the m216 version-fallback ladder (research R3) reuses this pattern. If the helper is private to `golang.rs`, note whether extraction to a shared `scan_fs::git_describe` module is needed for m216's reuse.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The core walker + builder + version-fallback ladder + m215-slug re-export. Every user story depends on these. NO user-story work can begin until Phase 2 completes.

**⚠️ CRITICAL**: `cargo check -p waybill --all-targets` MUST pass at Phase 2 completion.

- [X] T005 Re-export the m215 slug helper for cross-module use. Change `waybill-cli/src/generate/split.rs::subject_slug` visibility from `pub(crate)` to `pub(crate)` if not already, AND re-export via `waybill-cli/src/generate/mod.rs` OR use `crate::generate::split::subject_slug` directly from `gem.rs`. Verify by adding a temporary `use crate::generate::split::subject_slug;` in `gem.rs` and running `cargo check -p waybill`.
- [X] T006 [P] Add helper `fn git_describe_version(dir: &Path) -> Option<String>` in `waybill-cli/src/scan_fs/package_db/gem.rs` OR extract from `golang.rs` if it already exists there. Signature: takes a directory path, returns `Some(stdout-trimmed)` on success + non-empty output, `None` otherwise. 2-second subprocess timeout using `std::process::Command` + `std::thread::spawn` + `std::sync::mpsc` (same pattern as `golang.rs::run_go_mod_graph` at approximately line 81-158). Unit test in `gem.rs::tests` covering: (a) returns Some on a tagged git repo, (b) returns None on a non-git dir (tempdir), (c) returns None on empty stdout.
- [X] T007 Add walker `fn find_top_level_gemfiles(rootfs: &Path) -> Vec<PathBuf>` in `waybill-cli/src/scan_fs/package_db/gem.rs` per contracts/application-main-module.md emission predicate + research R1. Signature mirrors `find_top_level_gemspecs` at gem.rs:1153. Excludes `vendor/`, `gems/`, `specifications/`, `.bundle/`. FR-007 gemspec-wins guard: skip any directory whose `read_dir()` contains a `*.gemspec` file. Result sorted lex by path. Uses `MAX_GEMSPEC_WALK_DEPTH` (existing const). Uses `scan_fs::walk::safe_walk` (m114 helper — matches `find_top_level_gemspecs` convention).
- [X] T008 Add builder `fn build_gem_application_main_module_entry(gemfile_path: &Path, scan_root: &Path) -> Option<PackageDbEntry>` in `waybill-cli/src/scan_fs/package_db/gem.rs` per data-model.md E1. Constructs `PackageDbEntry` with `purl = pkg:generic/<slug>@<version>` (slug from T005's helper applied to directory basename; version from T006's git-describe ladder falling back to `0.0.0-unknown`), `extra_annotations = {waybill:component-role: main-module, waybill:package-shape: application}`, `evidence.source_file_paths = [<Gemfile path relative to scan_root>]`, `parent_purl = None`, `sbom_tier = Some("source")`. Every other field matches `build_gem_main_module_entry` (gem.rs:1202) defaults. Returns `None` when the slug is empty (pathological case per R2 skip pattern).
- [X] T009 [P] Add unit tests in `waybill-cli/src/scan_fs/package_db/gem.rs::tests`:
  - `find_top_level_gemfiles_walks_gemfile_only_dirs` — tempdir with `Gemfile` + `Gemfile.lock` no gemspec; walker returns exactly one path.
  - `find_top_level_gemfiles_skips_gemspec_carrying_dirs` — tempdir with BOTH `Gemfile` AND `*.gemspec`; walker returns empty (FR-007 gemspec-wins).
  - `find_top_level_gemfiles_skips_vendor_gems_bundle` — tempdir with `Gemfile` under `vendor/`, `gems/`, `.bundle/`; walker returns empty for each.
  - `build_gem_application_main_module_purl_is_pkg_generic` — construct on a tempdir named `my-service` with a `Gemfile`; assert PURL starts with `pkg:generic/my-service@`.
  - `build_gem_application_main_module_has_package_shape_annotation` — assert `extra_annotations.get("waybill:package-shape") == Some("application")`.
  - `build_gem_application_main_module_falls_back_to_unknown_version` — non-git tempdir; assert version segment == `0.0.0-unknown`.
  - `build_gem_application_main_module_applies_m215_slug_rules` — tempdir named with uppercase + non-ASCII chars; assert PURL name is sanitized (lowercase, ASCII).

**Checkpoint**: `cargo check -p waybill --all-targets` clean; walker + builder implemented + unit-tested but NOT yet wired into the dispatch loop.

---

## Phase 3: User Story 1 — Split-mode surfaces Ruby applications as sub-SBOMs (Priority: P1) 🎯 MVP

**Goal**: `waybill sbom scan --split --output-dir <dir>` on a repo with N Gemfile-only Ruby applications emits N new sub-SBOMs. On the ~/Projects/iac reproducer, 37 sub-SBOMs (was 34) — the 3 new ones correspond to `common-infra/`, `app-infra/`, `archives/gcp/`. SC-001.

**Independent Test**: On a fixture directory with 2 sibling Gemfile-only sub-dirs + at least one non-Ruby subproject, `waybill sbom scan --split --output-dir <tmp>` emits ≥ 3 sub-SBOMs; the 2 Ruby-app sub-SBOMs use the `<slug>.generic.cdx.json` naming convention and the split manifest lists them with `root_purl` starting with `pkg:generic/`.

### Implementation for User Story 1

- [X] T010 [US1] Wire the application-loop into `gem::read()` in `waybill-cli/src/scan_fs/package_db/gem.rs` per research R5. Append IMMEDIATELY AFTER the existing m069 gemspec-loop (approximately line 1090 after `dedup_gem_main_modules_by_purl`). For each path returned by `find_top_level_gemfiles(rootfs)`, call `build_gem_application_main_module_entry(path, rootfs)`, and push into `out`. Increment `main_modules_emitted` counter for structured-log parity with the gemspec-loop. FR-007 is guaranteed by T007's walker predicate (no PURL-based dedup needed since gem vs generic types can't collide).

### Test tasks for User Story 1

- [X] T011 [P] [US1] Author fixture at `waybill-cli/tests/fixtures/gemfile_application/` per research R7: minimal `Gemfile` (declares 2 deps — `rack`, `json`) + `Gemfile.lock` (hand-authored, mimicking bundler output shape with the 2 deps + one transitive; ~30 lines total). NO gemspec file, NO Rakefile, NO bin/, NO .ruby-version. Keep the fixture < 50 lines total for reviewability.
- [X] T012 [US1] Create new integration test file `waybill-cli/tests/gemfile_main_module.rs` with scenario `gemfile_only_dir_emits_pkg_generic_main_module` per research R8: uses the fixture from T011; invokes `waybill sbom scan --path <fixture> --format cyclonedx-json` via `Command` in an isolated `$HOME` (matches `scan_split_basic.rs::run_split_scan` env-isolation pattern); asserts `metadata.component.purl` starts with `pkg:generic/gemfile_application@`; asserts `metadata.component.properties[]` contains BOTH `{waybill:component-role: main-module}` AND `{waybill:package-shape: application}`.
- [X] T013 [US1] Extend `waybill-cli/tests/gemfile_main_module.rs` with scenario `iac_reproducer_pattern_split_mode` per research R8: author a mini-monorepo tempdir (using `std::fs::write` at test setup — not committed as a fixture) containing 2 sibling Gemfile-only subdirs + 1 npm subdir (for control). Invoke `waybill sbom scan --path <tempdir> --split --output-dir <out> --format cyclonedx-json` via Command. Assert: 3 total sub-SBOMs; 2 Ruby-app sub-SBOMs have filenames matching `<slug>.generic.cdx.json`; manifest lists all 3 with correct `root_purl` types; the 2 Ruby-app root_purls start with `pkg:generic/`.
- [X] T014 [US1] Extend the same integration test file with scenario `gemfile_without_lock_still_emits_main_module` per FR-006. Author a tempdir with just `Gemfile` (no `Gemfile.lock`). Invoke single-SBOM scan. Assert: `metadata.component.purl` still starts with `pkg:generic/`; `waybill:package-shape = application` still present; the `components[]` list has 0 or few entries (no lock → no transitives) but the main-module IS emitted.
- [X] T014a [US1] Extend `waybill-cli/tests/gemfile_main_module.rs` with scenario `workspaces_detected_annotation_includes_ruby_apps` (verifies SC-005 per C3 analyze finding). Reuses the mini-monorepo tempdir pattern from T013 (2 sibling Gemfile-only subdirs + 1 npm subdir). Invoke SINGLE-SBOM scan (not split — the `waybill:workspaces-detected` annotation is emitted on the whole-repo SBOM per `scan_cmd.rs:3605`). Assert: `.metadata.properties[]` contains a `waybill:workspaces-detected` property whose value (JSON-array-in-string) includes the paths of both Ruby-app subdirs alongside the npm subdir.
- [X] T014b [US1] Extend `waybill-cli/tests/gemfile_main_module.rs` with scenario `ruby_app_sub_sbom_passes_split_manifest_v1_schema` (verifies SC-006 per C4 analyze finding). Reuses T013's tempdir + split-mode invocation. Load the emitted `split-manifest.json` and validate against `waybill-cli/contracts/split-manifest-v1.schema.json` using the existing `jsonschema = "0.46"` dev-dep (reuse the loader pattern from `waybill-cli/tests/split_manifest_schema.rs`). Fails if the Ruby-app entries drift from the v1 schema (e.g., missing required fields, wrong types, invalid `files{}` map keys).

**Checkpoint**: `cargo test -p waybill --test gemfile_main_module` passes green with 5 scenarios (3 original + 2 remediation). Ruby-application sub-SBOMs surface end-to-end.

---

## Phase 4: User Story 2 — Single-SBOM scan gets meaningful root component (Priority: P2)

**Goal**: On a Ruby application (Gemfile-only, no `--split`), the emitted SBOM's `metadata.component.purl` identifies the app instead of falling through to m127's `synthetic-placeholder`. SC-002 + SC-003.

**Independent Test**: On a Gemfile-only fixture, `waybill sbom scan --path <fixture>` (no `--split`) emits an SBOM whose `waybill:root-selection-heuristic` annotation reports `repo-root-main-module` (NOT `synthetic-placeholder`), and `metadata.component.purl` matches the application main-module.

### Implementation for User Story 2

Nothing new — US2 uses the same emission path as US1. This phase is test-only.

### Test tasks for User Story 2

- [X] T015 [US2] Extend `waybill-cli/tests/gemfile_main_module.rs` with scenario `single_sbom_scan_promotes_gemfile_app_over_synthetic_placeholder` per SC-002 + SC-003. Uses the fixture from T011. Invoke single-SBOM scan (no `--split`). Assert: `.metadata.properties[]` contains `waybill:root-selection-heuristic` with the inner `heuristic` field matching `"repo-root-main-module"` (not `"synthetic-placeholder"`); `metadata.component.purl` matches `pkg:generic/gemfile_application@`.
- [X] T016 [US2] Extend the same integration test file with scenario `gemspec_present_wins_over_gemfile` per FR-007. Setup: create a tempdir with BOTH a `Gemfile` AND a synthetic `.gemspec` file (minimal — just `Gem::Specification.new do |s|; s.name = "pubgem"; s.version = "1.0.0"; end`) + a matching `Gemfile.lock`. Invoke single-SBOM scan. Assert: EXACTLY ONE main-module in the output; its PURL is `pkg:gem/pubgem@1.0.0` (NOT `pkg:generic/`); no application main-module is emitted for the same directory.

**Checkpoint**: All 7 integration test scenarios in `gemfile_main_module.rs` pass green (T012 + T013 + T014 + T014a + T014b + T015 + T016).

---

## Phase 5: Polish + docs + PR

**Purpose**: Docs update per Constitution Principle V, pre-PR gate, PR open.

- [X] T017 [P] Add ONE row to `docs/reference/sbom-format-mapping.md` for `waybill:package-shape` per research R9. Format matches existing rows: annotation name, format-native equivalent per CDX/SPDX-2.3/SPDX-3 (all "N/A — parity-bridging annotation" for now), rationale line citing FR-002's purl-spec resolution. Keep to one row; do not embed a value-vocabulary table (that's a nice-to-have for a follow-up if the vocab grows).
- [X] T018 Pre-PR gate per CLAUDE.md: `./scripts/pre-pr.sh` — `cargo +stable clippy --workspace --all-targets -- -D warnings` (zero warnings) + `cargo +stable test --workspace` (every suite `ok. N passed; 0 failed`). If the m053 git-describe helper extraction (T006) introduces any dead-code warnings in `golang.rs`, guard with `#[cfg_attr(...)]` per m212 counter-code precedent.
- [X] T019 Verify the m214 CI grep gate stays green: run the local mirror `BADHITS=$(grep -rE '\bmikebom\b' waybill-cli/src waybill-common/src waybill-ebpf/src xtask/src Cargo.toml waybill-cli/Cargo.toml waybill-common/Cargo.toml waybill-ebpf/Cargo.toml xtask/Cargo.toml Dockerfile.ebpf-test scripts 2>/dev/null | grep -v '^Binary file' | grep -vE 'mikebom-test-fixtures' || true)`; expects zero output.
- [X] T020 Real-world reproducer validation per SC-001: rerun `waybill sbom scan --path ~/Projects/iac --split --output-dir /tmp/iac-m216 --format cyclonedx-json --offline` on the same monorepo that motivated this feature. Assert: `jq '.entries | length' /tmp/iac-m216/split-manifest.json` returns `37` (was 34); the 3 new entries have `subproject_id` values `common-infra.generic`, `app-infra.generic`, and `gcp.generic` (m215 slug is the DIRECTORY BASENAME per FR-003 — so `archives/gcp/Gemfile`'s parent dir basename is `gcp`, NOT `archives-gcp`); each of the 3 new sub-SBOMs' `metadata.component.purl` starts with `pkg:generic/` and each carries the `waybill:package-shape = "application"` property.
- [X] T021 Push branch `git push origin 216-gemfile-main-module`.
- [X] T022 Open PR against `main` titled `impl(216): emit main-module for Gemfile-only Ruby applications (closes #629)`. PR body includes: (a) summary + link to spec/plan + link to closing issue #629, (b) Test Plan enumerating the 7 integration test scenarios (T012 happy path + T013 reproducer pattern + T014 no-lock + T014a workspaces-detected + T014b schema validation + T015 heuristic promotion + T016 gemspec-precedence) + pre-PR gate + real-world reproducer assertion + m214 grep gate, (c) migration/backward-compat note ("emission is additive; existing gemspec + non-Ruby scans byte-identical per SC-004"), (d) note that the follow-up interop doc [waybill#627] applies unchanged.

### Final gates

- [ ] T023 CI-side verification: all 20 CI checks (linux-x86_64 default + ebpf-tracing, macOS, Windows, Kusari Inspector, 15 rootfs/language scanners) MUST pass. Merge blocked until all green. Watch for the pre-existing podman env-var race documented in `reference_podman_test_flake.md` memory — if that specific test fails, rerun the failed lane once before treating as a real regression.

---

## Dependencies & Execution Order

### Phase dependencies

- **Setup (Phase 1)**: no dependencies — starts immediately. T001-T004 all reads/verification, parallelizable within Phase 1.
- **Foundational (Phase 2)**: depends on Setup. T005 (slug re-export) must complete before T008. T006 (git-describe helper) parallelizable with T005 + T007. T007 (walker) + T008 (builder) sequential within gem.rs. T009 (unit tests) parallelizable but conceptually last within Phase 2.
- **US1 (Phase 3)**: depends on Foundational. T010 (dispatch wire-up) is the critical path. T011 (fixture) parallelizable with T010. T012-T014 integration tests sequential (all in same file).
- **US2 (Phase 4)**: depends on Foundational + T011 (uses the fixture). No production-code work; only tests T015 + T016 sequential in the same file.
- **Polish (Phase 5)**: depends on all preceding phases. T017 (docs) parallelizable. T018-T023 sequential release-prep steps.

### Cross-story parallelism

- After Phase 2 completes: T010 (US1 wire-up) is the critical path; T011 (US1 fixture authoring) can start in parallel.
- T012 || T015 || T016 (three integration test scenarios in the same file — technically same-file, so sequential, but the underlying assertions are independent).
- T017 (docs) can start any time after Phase 2 (it doesn't depend on tests passing).

### Within each user story

- Foundational modules (T005-T009) written FIRST — walker + builder before wire-up.
- Wire-up (T010) sequential in `gem.rs::read()`.
- Fixture (T011) parallelizable with wire-up.
- Integration tests (T012-T016) after wire-up + fixture exist.

---

## Parallel Example: Foundational Phase

```bash
# After T005 (slug re-export) completes, launch T006 + T007 in parallel:
Task: "Add git_describe_version helper in waybill-cli/src/scan_fs/package_db/gem.rs (or extract from golang.rs)"    # T006
Task: "Add find_top_level_gemfiles walker in waybill-cli/src/scan_fs/package_db/gem.rs"                              # T007

# T009 (unit tests) parallelizable across T006/T007/T008 targets (all in gem.rs::tests, but conceptually independent):
Task: "Add unit tests for walker + builder + git-describe helper in gem.rs::tests"                                   # T009
```

---

## Implementation Strategy

### MVP (US1 P1 only)

1. Complete Phase 1 (Setup).
2. Complete Phase 2 (Foundational — walker + builder + helpers).
3. Complete Phase 3 (US1 — dispatch wire-up + fixture + 3 integration test scenarios).
4. **STOP + VALIDATE**: run `waybill sbom scan --split` on `~/Projects/iac` and count sub-SBOMs. Expect 37 (was 34). This is the earliest mergeable point that closes waybill#629.

### Full delivery (US1 + US2)

5. Complete Phase 4 (US2 — 2 additional test scenarios for single-SBOM root selection + gemspec-precedence).
6. Complete Phase 5 (Polish + PR).

### Solo-dev sequencing (recommended)

T001 → T002 → T003 → T004 → T005 → T006 → T007 → T008 → T009 → T010 → T011 → T012 → T013 → T014 → T014a → T014b → T015 → T016 → T017 → T018 → T019 → T020 → T021 → T022 → T023.

(All tasks sequential — this milestone is small enough that the parallelism overhead exceeds the benefit; strict sequential execution is the simplest path.)

---

## Notes

- [P] tasks = different files, no dependencies.
- No golden regeneration is needed for this milestone. Pre-feature gemspec-only fixtures continue to produce byte-identical output (FR-009, FR-010, SC-004). Any golden drift on a non-Ruby-application fixture is a regression, not an expected change.
- Fixture authoring (T011) is small (~30 lines total) — hand-authored `Gemfile.lock` mimics bundler output shape without requiring a Ruby runtime in CI.
- The m053 `git describe` helper (T006) may need extraction to a shared `scan_fs::git_describe` module if it's currently private to `golang.rs`. Design decision deferred to T006 execution — if extraction is > 20 LOC, do it; if it's smaller, inline a copy in `gem.rs` and file a follow-up for consolidation.
- **Constitution Principle V audit**: the new `waybill:package-shape` annotation MUST be documented in `docs/reference/sbom-format-mapping.md` (T017) with a parity-bridging justification citing FR-002's resolution. Reviewers will reject the PR without this.
- **Podman test flake caveat** (T023): the `podman_source::discover_storage_root_falls_back_to_default_when_no_config` test occasionally fails on CI due to a pre-existing env-var race. See `reference_podman_test_flake.md` memory for the diagnostic playbook. Rerun the failed CI job once before treating as a real regression.
