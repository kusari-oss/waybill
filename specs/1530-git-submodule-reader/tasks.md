# Tasks: Git submodules as components

**Input**: Design documents from `specs/1530-git-submodule-reader/`
**Prerequisites**: plan.md, spec.md, research.md (R1–R15), data-model.md, contracts/wire.md, quickstart.md

**Tests**: Requested. SC-001 to SC-010 are each asserted by a test, and the project's mutation practice applies (plan step 7). Test tasks precede the code they cover and must be seen failing first.

**Organization**: By user story.
- **US1** (P1): pin every submodule, even on a fresh clone. It ships alone as the MVP.
- **US2** (P1): count only real gitlinks, recurse into nested submodules, and report skips in C222.
- **US3** (P2): merge with other readers only on an identical PURL.

## Standing rules for every task

- **Base.** `main` at `fbaf3a6d` (v0.16.1 development).
- **Fixtures** are git repositories built at test time in a `tempfile::tempdir()`, from local `file://` repositories, with `protocol.file.allow=always` set on the command line, `GIT_CONFIG_NOSYSTEM=1` and `HOME` pointed at the tempdir. No test clones a remote. TheRock is never cloned or scanned locally (SC-003 is a CI step, T045). Package names are synthetic (`waybill-fixture-*`).
- **Gitlinks without a real submodule repository** can be made with `git update-index --add --cacheinfo 160000,<40-hex>,<path>` followed by a commit. Use this wherever a test only needs the gitlink.
- **The reader never runs an index-reading git command** (`ls-files`, `status`, `diff`, `describe --dirty`). Research R1 measured those running a repository's `core.fsmonitor`.
- **Revert mutations with `cp`**, never `git checkout`.
- **Never pipe a check through `head`.** Count with `wc -l` or write to a file.
- **Ask the binary.** Assertions read emitted documents, never a re-parse of `.gitmodules` by the test.
- **The gate:** `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`. No `+toolchain`, and never `cargo fmt`. Run `cargo clippy --workspace --all-targets -- -D warnings` before every gate (a dead-code `read()` cost one gate in #1521).
- **Test modules** that unwrap carry `#[cfg_attr(test, allow(clippy::unwrap_used))]`. Integration tests that need a Unix-only shell or `chmod` use file-scope `#![cfg(unix)]`.
- **Commit trailer:** `Assisted-by: Claude:claude-opus-5-5`.

---

## Phase 1: Setup (re-verify research before building on it)

**Purpose**: Confirm the research claims still hold on this base, before any code depends on them.

- [X] T001 Re-run research R6's check. Run `grep -rnE 'depends(\.push|:).*"pkg:|depends\.push\(.*purl' waybill-cli/src/scan_fs --include='*.rs'` and confirm it returns no production line. Record the command and its full output in `specs/1530-git-submodule-reader/measurements/r6-depends-grep.txt`.
- [X] T002 Re-run `specs/1530-git-submodule-reader/measurements/probe/git-safety.sh "$(mktemp -d)"`. Diff its R1, R2 and R3 lines against `measurements/git-safety.txt`, after replacing the scratch path with `<scratch>`, and record any difference in `measurements/git-safety-recheck.txt`. Stop and report if `ls-tree` now runs the payload, or if `ls-files` no longer does: the positive control must still fire.
- [X] T003 Create the module skeleton `waybill-cli/src/scan_fs/package_db/git_submodule/{mod.rs,gitmodules.rs,gitlinks.rs,identity.rs}`, with module docs citing spec 1530 and research R1. Add `pub const GIT_SUBMODULE: ReaderId = ReaderId::new("git-submodule");` beside `CONAN` in `waybill-cli/src/scan_fs/walk_registry/mod.rs`, and the `mod git_submodule;` declaration in `waybill-cli/src/scan_fs/package_db/mod.rs`. It must compile with no behaviour change.

---

## Phase 2: Foundational (shared plumbing, each a no-op without submodules)

**Purpose**: The shared changes research R5, R6, R9, R15 and R16 require. Each lands with a test, and the phase ends with a zero-churn control.

- [X] T004 [P] Generalise `run_bounded` in `waybill-cli/src/scan_fs/package_db/nix/eval/invoke.rs` (research R15). Add `pub(crate) fn run_bounded_program(program: &str, args: &[&str], envs: &[(&str, &str)], budget: Duration) -> Result<BoundedOutput, DegradationReason>`. Make `run_bounded(args, budget)` call it with `"nix"` and no env, so the Nix tier is unchanged. The existing Nix eval tests must pass untouched.
- [X] T005 [P] Create `waybill-cli/src/scan_fs/package_db/forge.rs` with `pub(crate) fn forge_type(host: &str) -> Option<&'static str>`, mapping `github.com`→`github`, `gitlab.com`→`gitlab`, `bitbucket.org`→`bitbucket` and `codeberg.org`→`codeberg` (research R9). Replace the `match host { … }` in `waybill-cli/src/scan_fs/package_db/yocto/recipe.rs::parse_git_src_uri` with a call to it. Every existing Yocto test and golden must stay byte-identical.
- [X] T006 [P] Add exact-PURL `depends` resolution in `waybill-cli/src/scan_fs/mod.rs`, in the `for dep_name in &entry.depends` loop (research R6). Before the name lookup, a `dep_name` starting with `pkg:` that equals an emitted component's PURL produces the `DependsOn` edge directly. Build a `HashSet` of emitted PURLs once, beside `name_to_purl`. Add unit tests in the same file:
  - a `pkg:` entry resolves across PURL types;
  - a `pkg:` entry that matches no component stays unresolved;
  - a plain name still resolves exactly as before.
- [X] T007 [P] Add a reader-contributed root-anchor channel in `waybill-cli/src/supplement/mod.rs` (research R5):
  - a second thread-local, `READER_ROOT_ANCHORS`, with `pub(crate) fn install_reader_root_anchors(Vec<String>) -> ReaderAnchorGuard`, whose guard clears it on drop;
  - `current_root_anchors()` returns the sorted, deduplicated union of the supplement anchors and the reader anchors.

  All three emitter call sites (`generate/cyclonedx/dependencies.rs`, `generate/spdx/document.rs`, `generate/spdx/v3_document.rs`) already call `current_root_anchors()` and need no change. Add a unit test of the union and of the empty case.
- [X] T008 [P] Lowercase the CMake reader's `pkg:github` and `pkg:bitbucket` PURLs (FR-015, research R16). In `waybill-cli/src/scan_fs/package_db/cmake.rs::parse_fetch_block`, lowercase the owner and repository captured by `github_re` before formatting, and do the same for any `bitbucket.org` handling. Add a unit test: `GIT_REPOSITORY https://github.com/ROCm/Foo.git GIT_TAG v1` gives `pkg:github/rocm/foo@v1`. The `golden/*/cmake.*` goldens must not change (`google/googletest` is already lowercase), and T009 checks that.
- [X] T009 Control checkpoint. Run clippy, then the gate. It must pass, and `git status --short -- waybill-cli/tests/fixtures | wc -l` must print `0`, because no golden may change from the foundational work. Commit Phase 1 and 2.

**Checkpoint**: The plumbing is in place and byte-identical. User stories can start.

---

## Phase 3: User Story 1 — every pinned submodule becomes a component, even on a fresh clone (Priority: P1) 🎯 MVP

**Goal**: One component per gitlink, pinned to the gitlink's commit, with forge PURL types, C55 and C57, anchored to the root, and with no repository-chosen program run.

**Independent Test**: quickstart §1 and §4. The #1515 reproduction gives one component in both the checked-out and the fresh-clone scan, and a hostile `core.fsmonitor` never runs.

### Tests for User Story 1 (write first, see them fail)

- [X] T010 [P] [US1] Create the shared test helper `waybill-cli/tests/common/git_fixture.rs`:
  - `fn git(dir, args)`, with isolated `HOME` and `GIT_CONFIG_NOSYSTEM=1`;
  - `fn repo_with_submodule(...) -> (TempDir, super_path, dep_commit)`;
  - `fn fresh_clone(...)`;
  - `fn add_gitlink(repo, path, commit)`, through `update-index --cacheinfo`.

  Register it the way the other `tests/common/*` helpers are.
- [X] T011 [P] [US1] Write unit tests in `waybill-cli/src/scan_fs/package_db/git_submodule/gitmodules.rs` for the `.gitmodules` parser (research R7):
  - quoted values with `\"`;
  - case-insensitive keys;
  - `#` and `;` comments;
  - two sections;
  - a path that is absolute or contains `..`, rejected as `unsafe-path-or-url`;
  - a URL beginning with `-`, rejected;
  - a section with no `path`, ignored with a diagnostic.
- [X] T012 [P] [US1] Write unit tests in `waybill-cli/src/scan_fs/package_db/git_submodule/identity.rs` for URL → PURL (FR-004, contracts/wire.md table):
  - `https`, `.git`, scp `git@github.com:o/r.git` and `ssh://git@github.com/o/r`, each to `pkg:github/o/r@<c>`;
  - casing (FR-004, research R16): `github.com/ROCm/Foo` and `bitbucket.org/Org/Repo` are lowercased to `pkg:github/rocm/foo@<c>` and `pkg:bitbucket/org/repo@<c>`, while `gitlab.com/Org/Repo` and `codeberg.org/Org/Repo` keep their case;
  - the same forms for `gitlab.com`, `bitbucket.org` and `codeberg.org`;
  - `gitlab.com/g/sub/p` falls back to generic;
  - another host gives `pkg:generic/<last>@<c>?vcs_url=git+<url>`;
  - `https://user:token@host/x.git` loses the credentials;
  - a relative `../dep.git`, both with and without an `origin` URL (research R8);
  - no URL gives `pkg:generic/<path-basename>@<c>`.
- [X] T013 [P] [US1] Write SC-001 as `waybill-cli/tests/git_submodule_reader.rs::sc001_fresh_clone_pins_the_gitlink_commit`. Use T010's helper. Scan the checked-out super-project and its fresh clone with the built binary (`CARGO_BIN_EXE_waybill`, `--offline`), and assert in CycloneDX:
  - exactly one component with `waybill:source-mechanism: git-submodule` in each scan;
  - the same PURL in both scans;
  - its version equals `git -C dep rev-parse HEAD`;
  - the root's `dependsOn` contains it.
- [X] T014 [P] [US1] Write SC-010 in `waybill-cli/tests/git_submodule_reader.rs::sc010_repository_config_runs_nothing`. Set `core.fsmonitor`, `core.pager` and `core.hooksPath` in the scanned super-project to a script that touches a marker. Scan, and assert the marker file is absent and the component is still emitted. Positive control in the same test: running `git ls-files --stage` directly in that repository does create the marker. Without the control the test could pass vacuously.
- [X] T015 [P] [US1] Write SC-004 in `waybill-cli/tests/git_submodule_reader.rs::sc004_credentials_never_emitted`. Use a submodule URL `https://waybill-fixture-user:waybill-fixture-token@example.invalid/waybill-fixture-dep.git`, built with `add_gitlink` and a hand-written `.gitmodules`. Assert that no output byte in any of the three formats contains `waybill-fixture-token` or `waybill-fixture-user`.
- [X] T016 [P] [US1] Write SC-008 in `waybill-cli/tests/git_submodule_reader.rs::sc008_build_reference_classification`. Three gitlinks:
  - `third_party/waybill-fixture-a`, used by `add_subdirectory(third_party/waybill-fixture-a)` in the root `CMakeLists.txt`;
  - `third_party/waybill-fixture-b`, used by `find_package(waybill-fixture-b)`;
  - `third_party/waybill-fixture-c`, referenced by neither.

  Assert C57 is `declared-and-used`, `declared-and-used` and `declared-only` respectively, in all three formats.

### Implementation for User Story 1

- [X] T017 [US1] Implement the parser in `waybill-cli/src/scan_fs/package_db/git_submodule/gitmodules.rs`, to data-model.md's `SubmoduleDeclaration` and research R7: `pub(crate) fn parse(text: &str, declared_in: &Path) -> (Vec<SubmoduleDeclaration>, Vec<Skip>)`. It must not use `git config -f`. T011 passes.
- [X] T018 [US1] Implement `waybill-cli/src/scan_fs/package_db/git_submodule/gitlinks.rs` (research R1, R2, R15). `pub(crate) fn read_gitlinks(repo: &Path, budget: Duration) -> Result<Vec<Gitlink>, GitlinkError>`:
  - Run `run_bounded_program("git", ["-C", repo, "-c", "core.fsmonitor=false", "-c", "core.hooksPath=/dev/null", "--no-pager", "ls-tree", "-r", "-z", "--full-tree", "HEAD"], envs [("GIT_TERMINAL_PROMPT","0"), ("GIT_OPTIONAL_LOCKS","0")], 30s)`.
  - Keep `160000 commit` entries only.
  - Map errors to the C222 reasons: `ToolAbsent` or timeout → `git-unavailable`; stderr containing `detected dubious ownership` → `git-refused`.
  - Assert at the call site that no argument derived from `.gitmodules` reaches argv.
  - Add `pub(crate) fn origin_url(repo)`, using `git config --get remote.origin.url` (research R8), with the same flags.
  - Add `pub(crate) fn is_repo_toplevel(dir)`, using `rev-parse --show-toplevel`, comparing canonicalized paths.
- [X] T019 [US1] Implement `waybill-cli/src/scan_fs/package_db/git_submodule/identity.rs`, to FR-004 and research R8 and R9. Use `forge::forge_type` (T005) and `crate::identifiers::sanitize::sanitize_userinfo` (milestone 075) before any use of the URL, and add scp and `ssh://` parsing. T012 passes.
- [X] T020 [US1] Implement C57 in `waybill-cli/src/scan_fs/package_db/cmake.rs` (FR-008, research R10):
  - remove `#[allow(dead_code)]` from `collect_find_package_targets` and `collect_into`, which are now used;
  - add `pub fn collect_cmake_path_references(scan_root: &Path) -> BTreeSet<PathBuf>`, covering `add_subdirectory(<dir>)` and the `SOURCE_DIR <dir>` of `ExternalProject_Add` and `FetchContent_Declare`, resolved against each file's directory after substituting `${CMAKE_CURRENT_SOURCE_DIR}`, `${CMAKE_CURRENT_LIST_DIR}`, `${PROJECT_SOURCE_DIR}` and `${CMAKE_SOURCE_DIR}`, and skipping anything that still holds `${`;
  - add `pub(crate) fn build_reference(submodule_path, targets, path_refs) -> &'static str`.
- [X] T021 [US1] Implement `waybill-cli/src/scan_fs/package_db/git_submodule/mod.rs`:
  - `registration()`: walker pattern `**/.gitmodules`, with `on_file` recording each path in a `Mutex<GitSubmoduleMarker { gitmodules: BTreeSet<PathBuf> }>`, modelled on `conan.rs`.
  - `extract_marker`.
  - `finalize(marker, scan_root) -> (Vec<PackageDbEntry>, Option<SubmoduleCoverage>)`: for each `.gitmodules` whose directory is a repository top level, `read_gitlinks`, join with the declarations by path, and emit one entry per gitlink.
  - Each entry has PURL from T019, version = commit, `sbom_tier: "source"`, `waybill:source-mechanism: git-submodule`, `waybill:build-reference` from T020, and `source_path` = the `.gitmodules` file.
  - It never reads a repository above `scan_root` (research R14).
  - It returns no entries and `None` when the marker is empty, so it costs nothing (research R4).
- [X] T022 [US1] Register and finalize the reader in `waybill-cli/src/scan_fs/package_db/mod.rs`, beside the conan registration (`register("conan", conan::registration())`) and its finalize block (`.find(|r| r.reader_id == ReaderId::CONAN)`).
- [X] T023 [US1] Anchor top-level submodules to the root, in `waybill-cli/src/cli/scan_cmd.rs`, next to (not inside) the supplement branch that calls `crate::supplement::install(...)`. Compute the anchors: the PURLs of components whose `waybill:source-mechanism` is `git-submodule`, or that carry `waybill:build-reference` (C57, which only this reader emits; it marks a submodule FR-009 merged into another reader's component, since C56 is not emitted, #1558), and that are not the target of a `DependsOn` from another such component. Install them with `install_reader_root_anchors` (T007), holding the guard for the emission's lifetime. This must hold whether or not a supplement is active.
- [X] T024 [US1] Run T011–T016. All pass. Then run the mutations, each reverted with `cp` and each recorded in `specs/1530-git-submodule-reader/measurements/mutations.md` with the failing test's name:
  - swap `ls-tree … HEAD` for `ls-files --stage` (T014 must fail);
  - take the commit from the submodule checkout's `HEAD` instead of the gitlink (T013 must fail on the fresh clone);
  - skip `sanitize_userinfo` (T015 must fail);
  - skip the anchor install (T013's root assertion must fail).

**Checkpoint**: US1 works alone. Run quickstart §1 and §4 by hand.

---

## Phase 4: User Story 2 — only real submodules are counted, including nested ones (Priority: P1)

**Goal**: One component per gitlink, never per `.gitmodules` line. Recurse into initialised submodules. Report every skip by reason in C222.

**Independent Test**: quickstart §2 and §3, plus SC-002 and SC-009.

### Tests for User Story 2 (write first, see them fail)

- [X] T025 [P] [US2] Write SC-002 in `waybill-cli/tests/git_submodule_reader.rs::sc002_declarations_without_gitlinks_are_skipped`. The super-project's top-level `.gitmodules` declares three paths, `projects/a/third_party/x`, `projects/a/third_party/y` and `projects/a/third_party/z`, and only the first two have gitlinks. A plain directory `projects/a/` (not a repository) carries its own `.gitmodules` declaring `third_party/x`, the same gitlink, plus `third_party/w`, which has none. This is #1515's monorepo and vendored shape (FR-001). Assert:
  - two components;
  - C222 `{"pinned":2, …, "no-gitlink":2, "no-repository-data":0, …}` (`z` from the top level, `w` from the vendored file);
  - `x` is emitted once.
- [X] T026 [P] [US2] Write `waybill-cli/tests/git_submodule_reader.rs::nested_submodules_follow_initialised_only`. It also covers the spec edge case of a checkout that differs from its gitlink: moving the initialised submodule to another commit without committing keeps the gitlink's commit as the version and logs a WARN. Assert on stderr.
  - an initialised submodule that itself pins `waybill-fixture-inner` gives an inner component whose version is the inner repository's gitlink;
  - the inner component's edge comes from the outer component, not from the root;
  - after `git submodule deinit`, the inner component is absent.
- [X] T027 [P] [US2] Write `waybill-cli/tests/git_submodule_reader.rs::gitlink_without_declaration`. A gitlink added with `add_gitlink` has no `.gitmodules` entry, while another submodule does. Assert `pkg:generic/<basename>@<commit>` with no `vcs_url`, and C222 `no-url: 1`, with that gitlink also counted in `pinned`.
- [X] T028 [P] [US2] Write SC-009 in `waybill-cli/tests/git_submodule_reader.rs::sc009_no_repository_data`. Remove `.git` from a fresh clone and scan. Assert zero submodule components and C222 `{"pinned":0,…"no-repository-data":<entry count>…}`, identical in all three formats. Rescan the same tree with `.git` present and assert every entry is pinned.
- [X] T029 [P] [US2] Write `waybill-cli/tests/git_submodule_reader.rs::git_refused_and_git_unavailable`. First assert that `GIT_TEST_ASSUME_DIFFERENT_OWNER=1 git -C <repo> rev-parse` exits 128 on this host's git. If it does not, the git is too old to simulate the case: skip the `git-refused` half with a printed reason, so it never passes vacuously. Then:
  - with `GIT_TEST_ASSUME_DIFFERENT_OWNER=1` set on the waybill process, C222 shows `git-refused`;
  - with `PATH` set to an empty tempdir, C222 shows `git-unavailable`;
  - in both cases there are zero submodule components, and the scan exits 0.

### Implementation for User Story 2

- [X] T030 [US2] Implement recursion and the per-gitlink join in `waybill-cli/src/scan_fs/package_db/git_submodule/mod.rs` (FR-001, FR-002, FR-006, data-model transitions):
  - a `.gitmodules` below its repository's top level is joined against the enclosing repository's gitlinks under its directory prefix, and only one with no enclosing repository at or below the scan root is `no-repository-data`;
  - deduplicate repositories by canonical top level;
  - for an initialised gitlink, read its own `.gitmodules` and gitlinks;
  - set the outer entry's `depends` to the inner entries' canonical PURLs (T006);
  - collapse duplicate declarations of one path, first URL in path order, and warn when the URLs disagree.
- [X] T031 [US2] Implement `SubmoduleCoverage` in `waybill-cli/src/scan_fs/package_db/git_submodule/mod.rs`, to data-model.md:
  - a closed `SkipReason` enum with `as_wire()` for the six reasons;
  - `fn wire(&self) -> String`, a sorted-key JSON object with all six reasons always present;
  - one INFO summary log;
  - a WARN for each skip, naming the `.gitmodules` file (contracts/wire.md, Diagnostics).
- [X] T032 [US2] Carry the coverage to the emitters the way `nix_closure_summary` already travels:
  - add `pub git_submodule_coverage: Option<…>` to `ScanArtifacts` in `waybill-cli/src/generate/mod.rs`;
  - set it in `waybill-cli/src/cli/scan_cmd.rs` from T021's `finalize` result, routed through `scan_fs`'s return value;
  - emit it as `waybill:git-submodule-coverage` in `waybill-cli/src/generate/cyclonedx/metadata.rs` (beside the C184 property), `waybill-cli/src/generate/spdx/annotations.rs` and `waybill-cli/src/generate/spdx/v3_annotations.rs` (beside `ANN_NIX_CLOSURE`).

  It is absent when `None`.
- [X] T033 [US2] Add catalog row **C222** `waybill:git-submodule-coverage` to `docs/reference/sbom-format-mapping.md` (contracts/wire.md, C222 section), and add the extractors `c222_cdx`, `c222_spdx23` and `c222_spdx3` with the `cdx_anno!` / `spdx23_anno!` / `spdx3_anno!` document macros in `waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs`. Register a `ParityExtractor { row_id: "C222", … }` in `waybill-cli/src/parity/extractors/mod.rs`. Both land in the same commit: the catalog gate is in the lib target. `sbom_format_mapping_coverage` and `every_catalog_row_has_an_extractor` must pass.
- [X] T034 [US2] Cross-format parity on fixture output (SC-006, FR-014). In `waybill-cli/tests/git_submodule_reader.rs::sc006_cross_format_parity`, scan the T025 and T026 trees once with `--format cyclonedx-json,spdx-2.3-json,spdx-3-json`. Run the C55, C57 and C222 extractors from `waybill_cli::parity::extractors`, the same functions the parity suite uses, over each output. Assert per-component equality across the three formats for C55 and C57, and document equality for C222. C56 is out of scope (#1558). Use `waybill sbom parity-check` instead if the extractors aren't reachable from an integration test.
- [X] T035 [US2] Run T025–T029. All pass. Mutation: emit one component per declaration instead of per gitlink, and T025 must fail. Record it in `measurements/mutations.md`. Revert with `cp`.

**Checkpoint**: US1 and US2 both work. Quickstart §1–§4 pass.

---

## Phase 5: User Story 3 — merges only on an identical PURL (Priority: P2)

**Goal**: FR-009. A submodule merges with another reader's component only when the canonical PURLs are identical, under milestone-105 precedence plus C56.

**Independent Test**: The un-ignored `dedup_collision` test and the merging fixture (SC-005).

- [X] T036 [US3] Rewrite `dedup_collision_emits_also_detected_via_after_us6` in `waybill-cli/tests/dedup_precedence_determinism.rs` and remove its `#[ignore]`:
  - copy `tests/fixtures/golden_inputs/dedup_collision/` into a tempdir, `git init` it, and add the gitlink `third_party/abseil` with `add_gitlink` and a fixed commit;
  - assert two components: `pkg:conan/abseil@20240722.0` with mechanism `conan-recipe`, and `pkg:github/abseil/abseil-cpp@<commit>` with mechanism `git-submodule` (FR-009). C56 is not asserted: it is never emitted until #1558.

  Update the fixture's `README.md`, removing the "post-US6 canonicalization" expectation, and delete its stale `.git/modules/abseil/HEAD` description.
- [X] T037 [US3] Add `waybill-cli/tests/git_submodule_reader.rs::sc011_submodule_and_fetchcontent_same_commit_merge`, which is SC-011. It moved to that file at implementation for its three-format scan helper. Use a gitlink `third_party/waybill-fixture-Mixed` with URL `https://github.com/ROCm/waybill-fixture-Mixed.git` at commit `<C>`, and a `FetchContent_Declare(waybill-fixture-mixed GIT_REPOSITORY https://github.com/ROCm/waybill-fixture-Mixed.git GIT_TAG <C>)`. Assert:
  - one component `pkg:github/rocm/waybill-fixture-mixed@<C>`, with mechanism `cmake-fetchcontent-git` and the submodule's C57, in all three formats (C56 deferred to #1558);
  - the root depends on it (FR-010, research R5).
- [X] T038 [US3] Determinism: scan each of the T036 and T037 trees 10 times and assert byte-identical outputs. Extend the existing `dedup_collision_scans_deterministically_today` pattern.

**Checkpoint**: All three stories work.

---

## Phase 6: Polish & cross-cutting

- [X] T039 [P] Amend the C57 row in `docs/reference/sbom-format-mapping.md` to name both signals: `find_package` name, and `add_subdirectory` / `SOURCE_DIR` path. Note that #1516 limits it to the files `discover_cmake_files` returns.
- [X] T040 [P] Add a CHANGELOG entry under `## [Unreleased]` in `CHANGELOG.md`, not adjacent to another branch's unreleased entry. Cover:
  - submodules as components pinned to the gitlink commit;
  - forge PURL types;
  - C57 path references;
  - C222;
  - that `git` runs with config-executed programs disabled.
- [X] T041 Run quickstart §1–§5 end to end with the release build, and record the outputs in `specs/1530-git-submodule-reader/measurements/quickstart-run.txt`.
- [X] T042 Run clippy, then the gate. Record the positive signal and the `test result: ok` count. Never pipe through `head`.
- [X] T043 Corpus goldens (SC-007, research R13):
  - push the branch and dispatch `public-corpus.yml` with `regen_goldens=true`;
  - download the artifact and run a normalised, sorted diff against the committed goldens;
  - assert that exactly the three `public_corpus/haskell-language-server/*` files differ, each only by gaining the C222 record with `pinned: 0`;
  - commit them with `measurements/golden-review.md`. Any other difference is a defect to explain before committing.
- [X] T044 TheRock (SC-003), in CI only. Add a branch-only workflow step that:
  - clones `ROCm/TheRock` at `b1cae390cc745283dce25581415353f9bc906162` without submodules;
  - records `git ls-tree -r HEAD | awk '$1=="160000"' | wc -l`;
  - scans with the branch binary and records the count of `git-submodule` components and the C222 record.

  Upload both. Remove the step before the PR. Record the run ID and the counts in `measurements/therock.md`. The counts must be equal, with `pinned` equal to the gitlink count.
- [ ] T045 Close-out:
  - verify every task's claim against the code and the measurements, not the checkbox;
  - confirm `measurements/` holds the probe, its outputs, the mutation log, the golden review and the TheRock record;
  - add the #1545 cataloger notes from `plan.md` to the PR description.

---

## Dependencies & execution order

- **Phase 1 → Phase 2 → stories.** T009 is the gate between them.
- **US1 (T010–T024)** has no story dependency. US2 (T025–T035) extends `git_submodule/mod.rs` from T021 and needs T006 for nested edges. US3 (T036–T038) needs US1's emission.
- **Within each story**, tests come first and must fail, then the implementation, then mutations.
- **Polish:** T043 and T044 need US1 and US2 complete. T039 and T040 can run any time after T033.

## Parallel opportunities

- **Phase 2:** T004–T008 touch different files.
- **US1 tests:** T010–T016, in different files or in independent test functions. T017, T019 and T020 can follow in parallel once their tests exist. T018 depends on T004.
- **US2 tests:** T025–T029.
- **Polish:** T039 and T040.

## Implementation strategy

- **MVP = US1.** Submodules appear, pinned and safe, with root edges and C57. This alone closes #1515's reproduction (SC-001).
- **US2** makes counts exact and skips visible (C222). It is needed before any corpus or TheRock measurement means anything.
- **US3** settles the merge rule and retires the `#[ignore]`d milestone-105 test.
- **One PR** is planned. If review prefers smaller PRs, stack them in the order Phase 2 → US1 → US2 → US3+polish, each targeting `main`, with CHANGELOG entries not adjacent.
