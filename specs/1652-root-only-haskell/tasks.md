# Tasks: `root-only` keeps the packages the root builds, and records what it dropped

**Input**: `specs/1652-root-only-haskell/` — plan.md, spec.md, research.md (R1–R9), data-model.md, contracts/{scope-filter,wire}.md, quickstart.md, measurements/.
**Tests**: requested (SC-001 to SC-006, mutations). Each story's tests are written first and must fail before its implementation.

**Rules that apply to every task:**
- Fixtures use `waybill-fixture-*` names only.
- No new Cargo dependencies.
- Never run `cargo fmt`.
- Mutations are reverted with `cp` from a backup, then rebuilt.
- Python runs as `python3 -I -B`.
- Never pipe a check through `head`.
- Test modules carry `#[cfg_attr(test, allow(clippy::unwrap_used))]`.

## Phase 1: Setup

- [ ] T001 Commit the six trees from `specs/1652-root-only-haskell/measurements/make-fixtures.sh` (f1–f6) as static fixtures under `waybill-cli/tests/fixtures/project_discovery_haskell/{f1-json-root,f2-hpack-root-chain,f3-cabal-project,f4-cabal-project-json,f5-stack,f6-npm-path}/`, byte-identical to the script's output. Verify with `diff -r` against a fresh `make-fixtures.sh` run.
- [ ] T002 Create `waybill-cli/tests/project_discovery_haskell.rs`, with `#![cfg(unix)]` at file scope and a module doc naming spec 1652 and the contracts. Add helpers, following `tests/project_discovery_scope.rs`:
  - `scan(tree, mode, format) -> (Value, String)`: `--offline`, isolated `HOME`, `NO_COLOR=1`, with the combined log.
  - `scan_all_formats(tree, mode)`: CycloneDX, SPDX 2.3 and SPDX 3 written to a tempdir.
  - `parity_check(dir)`: runs `waybill sbom parity-check` and asserts no gap.
  - Extractors for hackage and npm PURLs, CycloneDX `dependencies` edges, a document property by name, and "fell back" (the log contains `found zero root-level manifests`).

## Phase 2: Foundational

- [ ] T003 Add `ProjectDiscoveryReport.dropped_main_modules: Vec<String>` in `waybill-cli/src/generate/project_discovery/mod.rs`. Remove `Copy` from its derives, and update `all_default()` and the #1045 early return to an empty `Vec`. In `waybill-cli/src/generate/project_discovery/filter.rs`, after Step 4:
  - compute the sorted PURLs of `all_roots` absent from `reachable`;
  - set `nested_projects_ignored = dropped_main_modules.len()` (research R3).

  Fix any compile fallout in `waybill-cli/src/cli/scan_cmd.rs`. This blocks US3, and it changes no emitted byte.
- [ ] T004 Unit tests in `filter.rs`'s test module:
  - `dropped_set_excludes_main_modules_reached_through_edges`: a root depending on a nested main module, plus an independent one. Expect a drop of exactly 1 and `nested_projects_ignored == 1`. This is the R3 defect, which reported 2.
  - Invariants I2 and I3 from `contracts/scope-filter.md` over every existing filter test fixture.
  - `cycle_between_nested_main_modules_terminates_and_keeps_both`: root → `a`, `a` ↔ `b`. Both are kept and the call returns.
  - `nested_chain_unreached_from_root_drops_whole_chain`: `x` → `y`, with nothing reaching `x`. Both are dropped and both are in `dropped_main_modules`.

  Run `cargo test -p waybill --lib project_discovery` and confirm that `tests/project_discovery_scope.rs::us1_info_log_carries_mode_and_counts` still sees `nested_projects_ignored=2`, since its nested projects are unreached.

## Phase 3: User Story 1 — A package the root builds stays in the root's SBOM (P1)

**Goal:** a root package is recognised as root-level whatever its manifest is called, and nested main modules it reaches through edges are kept, under `root-only` and `strict` (FR-002, research R1).

**Independent test:** f2 under `root-only` keeps root, `a`, `c` and `base`, and drops `d`. It does not fall back.

- [ ] T005 [P] [US1] Write the failing integration tests in `waybill-cli/tests/project_discovery_haskell.rs`:
  - `us1_f2_root_only_keeps_chain_drops_independent`: no fallback; C140 = `root-only`; hackage PURLs are exactly {root, `a`, `c`, `base`}; edges root→`a` and `a`→`c` present; `d` absent.
  - `us1_f2_strict_keeps_chain`: the same under `strict`, with C140 = `strict`.
  - `us1_f6_npm_nested_by_path_kept`: `pkg:npm/waybill-fixture-lib@0.2.0` and the root's edge to it are present under both modes, and so is the versionless declared component `pkg:npm/waybill-fixture-lib` (research R2).
  - `us1_unlisted_manifest_name_is_root_level`: a tempdir tree with a root `pubspec.yaml` (`name: waybill_fixture_root`, `version: 1.0.0`) and a nested `package.json` (`waybill-fixture-nested`). Under `root-only` it does not fall back, and the nested npm project is dropped. This is research R1's stated consequence for every ecosystem; if the dart reader's root main module turns out not to carry a manifest-file evidence path, pick another manifest from outside `is_manifest_basename` and say which in the test's comment.

  Each test scans all three formats with `scan_all_formats`, asserts the same PURL set in each, and runs `parity_check` (SC-001).

  Confirm the f2 tests fail on the current binary, by falling back.
- [ ] T006 [P] [US1] Write the failing unit tests for `root_dir` in `waybill-cli/src/generate/project_discovery/mod.rs`'s test module, using a tempdir:
  - `source_dir` empty gives empty;
  - `source_dir` `package.yaml` (a file) gives empty;
  - `local-packages/a/a.cabal` (a file) gives `local-packages/a`;
  - a directory `crates/api` gives itself;
  - a non-existent path gives itself.

  Also `is_root_level` is true for a root whose `source_dir` is `package.yaml` at the scan root.
- [ ] T007 [US1] Implement `pub(crate) fn root_dir(root: &SubprojectRoot, scan_root: &Path) -> PathBuf` in `waybill-cli/src/generate/project_discovery/mod.rs`: the parent when `scan_root.join(&root.source_dir).is_file()`, else `source_dir`.
  - Use it in `is_root_level`.
  - In `filter.rs`, seed `in_scope_dirs` from `dir_key(&root_dir(r, scan_root))` instead of `dir_key(&r.source_dir)`.
  - Do **not** edit `waybill-cli/src/generate/split.rs` (FR-007, #1691).
  - Doc comment: why the directory is decided here and not in `is_manifest_basename`.
- [ ] T008 [US1] Run T005 and T006 until green, then `cargo test -p waybill --test project_discovery_scope --test go_per_mainmod_scope` to confirm nothing regressed.

## Phase 4: User Story 2 — A package the root project declares is part of it (P1)

**Goal:** a root `cabal.project` or `stack.yaml` `packages:` makes its packages in-scope roots under `root-only`, including when there is no root package (FR-003, Clarifications 1 and 2, research R4).

**Independent test:** f3 under `root-only` keeps `a`, `b`, `base` and `text`, and drops `tools/e`, without falling back.

- [ ] T009 [P] [US2] Write the failing integration tests in `waybill-cli/tests/project_discovery_haskell.rs`:
  - `us2_f3_cabal_project_sets_scope_without_root_package`: no fallback; hackage = {`a`, `b`, `base`, `text`}; `e` absent.
  - `us2_f4_cabal_project_members_join_json_root`: root-only keeps the npm root, `a`, `b`, `base` and `text`, and drops `e`.
  - `us2_f5_stack_yaml_sets_scope`: like f3.
  - `us2_strict_ignores_declarations`: f3 and f5 under `strict` fall back; f4 under `strict` drops `a`, `b` and `e`.

  Each test scans all three formats with `scan_all_formats`, asserts the same PURL set in each, and runs `parity_check` (SC-001).
- [ ] T010 [P] [US2] Write the failing unit tests in `waybill-cli/src/scan_fs/package_db/haskell.rs`'s test module for `resolve_root_declared_members`, using tempdir trees of `waybill-fixture-*` `package.yaml` and `.cabal` files:
  - `cabal.project` with `packages: local-packages/*`; `./a ./b`; `code/*/*.cabal`; `*/*.cabal`; `.`; the leading-comma form `    api-types\n  , api\n  , sdk` (hs-opentelemetry); `optional-packages:`;
  - a `cabal.project` with no `packages:`, which gives empty;
  - entries matching nothing, which give empty;
  - a nested `cabal.project`, which is ignored (root only);
  - `stack.yaml` with `packages: [a, ./b/]`; with no `packages:`, which gives `["."]` and so no nested member; with both files, the union;
  - output sorted, unique, `/`-separated.
- [ ] T011 [US2] In `waybill-cli/src/scan_fs/package_db/haskell.rs`:
  - add `packages: Option<Vec<String>>` to `StackYaml`;
  - add `fn resolve_root_declared_members(rootfs: &Path, cabal_project_paths: &[PathBuf], stack_yaml_paths: &[PathBuf], package_dirs: &[PathBuf]) -> Vec<String>`, per data-model.md. It uses only the files whose parent is `rootfs`, and globset with `literal_separator(true)`. Commas are separators. Normalise entries with a small helper; leave `parse_cabal_project_packages` and `cabal_files_declared_by` and their tests unmodified;
  - add `pub root_declared_member_dirs: Vec<String>` to `HaskellParseSummary`, filled in `finalize` from the directories of the discovered `package.yaml` and `*.cabal` paths.

  The field is **never rendered**: check every emitter that reads `HaskellParseSummary`.

  If `finalize` can return `None` while the resolver is non-empty, carry the directories without turning a `None` summary into `Some`. A `None`→`Some` change would alter `all` output (milestone 895's note). Record which case holds in a code comment, and pin it with a unit test.
- [ ] T012 [US2] Add the parameter `declared_member_dirs: &[String]` to `apply_scope_filter` in `waybill-cli/src/generate/project_discovery/filter.rs`:
  - when `mode.follows_workspace_members()`, extend `in_scope_roots` with every root whose `dir_key(&root_dir(..))` is in the set, deduplicated by PURL;
  - leave Step 1b and the #1045 early return unchanged.

  Then:
  - pass the summary's directories from `waybill-cli/src/cli/scan_cmd.rs` (empty when no summary);
  - update every existing call site and unit test to pass `&[]`;
  - add unit tests for I4 (`strict` ignores the set) and I5 (unmatched directories are a no-op).
- [ ] T013 [US2] Run T009 and T010 until green.

## Phase 5: User Story 3 — The document says what `root-only` dropped (P1)

**Goal:** C244, the `project-discovery-scope` reason code (with `compositions` agreeing), and C200's `manifests-dropped-by-scope`, in all three formats with parity (FR-004 to FR-006, contracts/wire.md).

**Independent test (the positive control, SC-003):** f1 under `root-only` carries C244 with count 2 and both hackage PURLs, C104 `partial` with the new reason, and C200's key listing both manifests.

- [ ] T014 [P] [US3] Write the failing integration tests in `waybill-cli/tests/project_discovery_haskell.rs`:
  - `us3_f1_positive_control_records_drop`: in all three formats, C244 equals exactly `{"count":2,"main-modules":["pkg:hackage/waybill-fixture-a@0.1.0","pkg:hackage/waybill-fixture-b@0.1.0"],"reason":"project-discovery-scope"}`; C104 is `partial`; C105 contains `project-discovery-scope: 2 project(s) dropped from hackage`; C200 `manifests-dropped-by-scope` has the two nested `package.yaml` entries, each also in `manifests-read`; and `parity_check` passes.
  - `us3_cdx_compositions_not_complete_for_dropped_ecosystem`: on f1, no `compositions[]` record claims hackage `complete`.
  - `us3_f6_no_drop_no_record`: no C244; no `project-discovery-scope` in C105.
  - `us3_f2_drop_one`: C244 count 1 naming `d`.
  - `us3_symlinked_root_c200_matches`: scan f1 through a symlink to the fixture in a tempdir; C200's key still lists both manifests (research R7).
  - `us3_all_mode_has_none_of_it`: f1 under `all` has no C140, no C244, no `project-discovery-scope`, and no `manifests-dropped-by-scope`.
- [ ] T015 [US3] Add `pub struct ProjectDiscoveryDrop { pub main_modules: Vec<String>, pub ecosystems: Vec<String> }` in `waybill-cli/src/generate/project_discovery/mod.rs`, with a constructor from the report that returns `None` when there is no drop. Ecosystems come from `Purl::ecosystem()`, sorted and unique.
  - Add `pub project_discovery_dropped: Option<ProjectDiscoveryDrop>` to `ScanArtifacts` in `waybill-cli/src/generate/mod.rs`.
  - Set `None` at every other `ScanArtifacts` construction site: grep `project_discovery_mode: None`.
  - Build it in `waybill-cli/src/cli/scan_cmd.rs` on the filtered branch only, and thread it to the CycloneDX builder as C140 is threaded (`with_project_discovery_mode`).
- [ ] T016 [US3] Emit C244 in `waybill-cli/src/generate/cyclonedx/metadata.rs`, `waybill-cli/src/generate/spdx/annotations.rs` and `waybill-cli/src/generate/spdx/v3_annotations.rs`, beside C140. The value is the canonical JSON string per contracts/wire.md, built with `serde_json::json!`, whose map sorts keys. In the same change:
  - add extractors `c244_cdx`, `c244_spdx23` and `c244_spdx3` in `waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs`, with the document-scope annotation macros C140 uses;
  - register them in `waybill-cli/src/parity/extractors/mod.rs` as `SymmetricEqual`, `order_sensitive: false`;
  - add the C244 row to `docs/reference/sbom-format-mapping.md`, with its Principle V audit from research R5.

  The row and its extractor land together, because the gate is in the lib target.
- [ ] T017 [US3] Add `ReasonCode::ProjectDiscoveryScope { count: usize, ecosystems: Vec<String> }` in `waybill-cli/src/generate/graph_completeness/reason_codes.rs`. Its rendering is `project-discovery-scope: {count} project(s) dropped from {ecosystems joined ", "}`, with a unit test, and a test that the detail contains no `;`.
- [ ] T018 [US3] Thread the drop into completeness:
  - Add a parameter `scope_drop: Option<&ProjectDiscoveryDrop>` to `compute_graph_completeness` (`waybill-cli/src/generate/graph_completeness/mod.rs`) and to `for_document` (`document.rs`).
  - On the empty-set path, return `partial` with the code instead of `trivially_complete()`, keeping `manifest_unread_code`'s `unknown` precedence when both fire.
  - On the classified path, push it after the `manifest_unread_code` push.
  - Update all four callers to pass the artifacts' drop: `cyclonedx/builder.rs`, `spdx/document.rs`, `spdx/v3_document.rs` and `orphan_reason.rs`. Update the existing unit tests to pass `None`.
  - Unit tests: an empty set with a drop gives `partial` plus the code; an empty set with a drop **and** an unread manifest gives `unknown` with both codes, `manifest-unread` first (contracts/wire.md); a non-empty complete graph with a drop gives `partial`; no drop leaves results unchanged.
- [ ] T019 [US3] Add an explicit `ReasonCode::ProjectDiscoveryScope { ecosystems, .. }` arm to `degraded_ecosystems` in `waybill-cli/src/generate/cyclonedx/compositions.rs`, extending with `ecosystems`, plus a unit test beside `a_degraded_ecosystem_is_unknown_even_when_reachable`.
- [ ] T020 [US3] Add `pub(crate) fn amend_scope_dropped(value: &mut serde_json::Value, dropped: &[String])` in `waybill-cli/src/scan_fs/package_db/haskell/hpack.rs`. It inserts a sorted `manifests-dropped-by-scope` only when non-empty; every entry must be present in `manifests-read`; nothing else is touched. Unit tests cover: empty is a no-op, giving a byte-identical `to_string()`; and a subset.

  In `waybill-cli/src/cli/scan_cmd.rs`, on the filtered branch:
  - map each dropped `pkg:hackage` main module's evidence paths to the `manifests-read` entry equal to the reader root joined with the path, trying both the canonical and the raw root;
  - apply it to `haskell_parse_summary.hpack_read`.

  Update C200's row text in `docs/reference/sbom-format-mapping.md` with the key.
- [ ] T021 [US3] Run T014 to T020's tests until green. Then run `cargo test -p waybill --test project_discovery_scope --test go_per_mainmod_scope --test sbom_format_mapping_coverage --test holistic_parity`, plus `cargo test -p waybill --lib parity`, because the catalog-and-extractor gate is in the lib target.

## Phase 6: User Story 4 — Nothing else changes (P2)

**Goal:** `all` is byte-identical, `--split` is unchanged, and #1045's fall-back holds (FR-007, SC-004).

- [ ] T022 [P] [US4] Integration test `us4_no_root_and_no_declaration_falls_back` in `waybill-cli/tests/project_discovery_haskell.rs`: a tempdir tree with only `nested/x/package.yaml` (`waybill-fixture-x`). Under `root-only`, it falls back, with no C140, no C244 and output equal in components to `all`.
- [ ] T023 [US4] Byte-identity measurement, recorded in `specs/1652-root-only-haskell/measurements/after.md`:
  - build `main` (`git worktree` at `main` into the scratchpad) and the branch;
  - scan f1–f6 with `all`, and with `--split=workspace` and `--split=directory` (with `--output-dir`);
  - mask the serial, timestamps and `waybill_version`/`generated_at`, then `diff -r`.

  Expect zero differences. Any difference is a defect to fix, not to document.

## Phase 7: Polish & cross-cutting

- [ ] T024 [P] Update `docs/reference/project-discovery.md`:
  - root detection by directory (Haskell roots now count);
  - a Haskell row in the per-ecosystem table (root `cabal.project`/`stack.yaml` `packages:` makes packages in-scope roots under `root-only`; not under `strict`);
  - edges across nested main modules, under both modes;
  - C244 and the reason code, with jq recipes;
  - the corrected meaning of `nested_projects_ignored`;
  - the fall-back condition.
- [ ] T025 [P] Update C105's row in `docs/reference/sbom-format-mapping.md`: list all 11 codes, including `manifest-unread` (m1273, previously missing) and `project-discovery-scope`, with its detail template.
- [ ] T026 [P] Add a CHANGELOG entry under Unreleased in `CHANGELOG.md`:
  - root-level detection for every manifest name;
  - Haskell root declarations;
  - C244;
  - the new reason code, as a closed-vocabulary addition;
  - C200's key;
  - the log count fix.

  Refs #1676, #1691.
- [ ] T027 Run the mutations (research R9), each via a `cp` backup, rebuild, the targeted test (which must fail), then `cp` back and rebuild. Record each in `specs/1652-root-only-haskell/measurements/after.md` with the failing test's name:
  - `root_dir` returns `source_dir`;
  - `split::project_for_root` skips nested main modules (never committed): `us1_f2_root_only_keeps_chain_drops_independent` and `us1_f2_strict_keeps_chain` must both fail, proving the tests watch reachability and not only root detection (SC-006);
  - declared roots not added (comment out the extend), then once with only `cabal.project` resolution removed, and once with only `stack.yaml` resolution removed;
  - declared roots applied under `strict`;
  - `root_dir` special-cased to Haskell file names only (`package.yaml`, `*.cabal`): `us1_unlisted_manifest_name_is_root_level` must fail;
  - `dropped_main_modules` left empty;
  - the reason code not pushed on the empty path, and separately on the classified path;
  - the `compositions` arm removed;
  - the C200 key not inserted;
  - C244 not emitted in SPDX 3 only, which parity must catch.
- [ ] T028 SC-005: rerun `measurements/run-corpus`-equivalent scans (the 12 targets in `measurements/baseline.md`, same pins, `--offline`) with the branch binary. Add the "after" table to `specs/1652-root-only-haskell/measurements/after.md`, with each target's C244 count, and compare against the R9 prediction (7 of 8 Haskell targets stop falling back; ogmios does not). Explain any mismatch from the code before going on.

  FR-001 also asks for the targets with neither a root package nor a root declaration: copy `hpack-ogmios` to the scratchpad, add a root `package.json` (`{"name":"waybill-fixture-root-js","version":"1.0.0"}`), scan it under `root-only` and `strict` with the `main` and branch binaries, and record the components kept and the C244 count.
- [ ] T029 Run `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh` (no `+toolchain`). Confirm `GATE_RESULT=pass`, the expected line, and every per-target `test result: ok. N passed; 0 failed` line. Enumerate any `---- … stdout ----` before calling it green.
- [ ] T030 Dispatch a verification corpus run: `gh workflow run public-corpus.yml -R kusari-oss/waybill --ref main -f branch=1652-root-only-haskell`. Expect every golden byte-identical (no target uses `--project-discovery`). Enumerate every job's conclusion.
- [ ] T031 Mark `specs/1652-root-only-haskell/checklists/requirements.md` and these tasks complete, verifying each task's claim against the diff. Open the PR with `Refs #1676` and the public footer.

## Dependencies

- **Setup** (T001, T002) comes before everything.
- **T003 and T004** (the drop set) block US3.
- **US1** (T005–T008) blocks US2, because `root_dir` is used by declared-root matching, and US3, because the f2 drop needs root detection.
- **US2** (T009–T013) blocks US3's f3–f5 expectations. US3's f1 positive control depends only on US1.
- **US4** (T022, T023) comes after US1 to US3.
- **Polish** comes last. T027 and T028 come before T029, and T029 before T030 and T031.

## Parallel opportunities

- T005 ∥ T006 (different files).
- T009 ∥ T010.
- T014 ∥ T017 (the test file and `reason_codes.rs`).
- T024 ∥ T025 ∥ T026.

`filter.rs` (T003, T007, T012) and `scan_cmd.rs` (T012, T015, T020) are edited in sequence.

## Implementation strategy

- **MVP = US1.** Recognising Haskell roots makes `root-only` act on the Haskell targets that have a root package (haskell-aeson, haskell-language-server), and keeps the packages the root reaches.
- **Then US2.** Declared members make the declaration-only layout work.
- **Then US3.** It makes every drop visible.
- **US4 and Polish** prove nothing else moved.
