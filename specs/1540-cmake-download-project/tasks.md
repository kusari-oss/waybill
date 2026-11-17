# Tasks: CMake fetches made through DownloadProject's `download_project()`

**Input**: `specs/1540-cmake-download-project/` (`spec.md`, `plan.md`, `research.md`, `data-model.md`, `contracts/wire.md`, `quickstart.md`, `measurements/`)

**Tests are required.** SC-007 asks for a contract test, and a mutation that fails it, for recognition, the keyword split, naming and accounting. Each story's tests are written first, and must fail at `main` before the code that passes them.

**Conventions:**
- Synthetic `waybill-fixture-*` names, and a DownloadProject-shaped module written for the tests, not copied from upstream.
- No new Cargo dependency (FR-011).
- TheRock in CI only; single public files may be read through the GitHub API.
- Revert mutations with `cp`, never `git checkout`.
- Gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, run alone, no `+toolchain`.
- Commits carry `Assisted-by: Claude:claude-opus-5-5`.

## Phase 1: Setup

- [X] T001 Census TheRock's `download_project` definitions and calls in CI: `census/download_project.py` and a step in `.github/workflows/census-fetches.yml` in `kusari-sandbox/waybill-fixture-therock-sdk` (`fecd9b4`, run 44137827351). Recorded in `specs/1540-cmake-download-project/measurements/census.md`, with `census-download-project.{json,py}` and `census-refs.txt` (SC-001).
- [X] T002 Probe what a `download_project()` call fetches with CMake 4.4.5, on a synthetic DownloadProject-shaped module with localhost tarballs: `specs/1540-cmake-download-project/measurements/cmake-semantics/probe.sh`, with `results.txt` and `cmake-version.txt` (D1–D6; research R1–R3).
- [X] T003 File the FR-009 follow-up for branch-name tags across all CMake git fetches: #1598 (research R6).

## Phase 2: Foundational (blocks every story)

- [X] T004 Write unit tests for keyword-list expansion in the `#[cfg(test)]` module of `waybill-cli/src/scan_fs/package_db/cmake/wrappers.rs`. They must fail before T005.
  - **A list held in a variable resolves:** a wrapper body with `set(one URL NAME)`, `cmake_parse_arguments(W "" "${one}" "" ${ARGN})` and `FetchContent_Declare(${W_NAME} URL ${W_URL})`, called with `NAME n URL u`, binds `n` / `URL u`.
  - **An empty list keeps its position:** `set(multi "")` as the multi-value argument does not shift the one-value list.
  - **A list set under `if()` is unresolved:** the call returns `Err(Unbound::UnknownKeywords)`.
  - **A list appended with `list(APPEND …)` is unresolved.**
  - **The `PARSE_ARGV` form** resolves its lists the same way.
  - **Integration, D-18:** add D-18 to `waybill-cli/tests/cmake_fetch_wrappers.rs`: a spec 1539 wrapper with `set(one URL NAME)` and `cmake_parse_arguments(W "" "${one}" "" ${ARGN})`. It must fail before T005: without expansion, nothing is parsed and no component is emitted.
- [X] T005 Implement keyword-list expansion in `Scope::evaluate` in `waybill-cli/src/scan_fs/package_db/cmake/wrappers.rs` (data-model "Keyword lists", research R2).
  - **Expansion:** expand each of `cmake_parse_arguments`' three keyword-list arguments in the scope as **one string**, joining a known list with `;`. Then split on `;`. Do not splice them as separate arguments.
  - **Unresolved lists:** a list still containing `${` returns `Err(Unbound::UnknownKeywords)`. Add the variant.
  - **Accounting:** confirm that `waybill-cli/src/scan_fs/package_db/cmake/fetches.rs` counts the new variant in `unresolved_identity`. It handles `Unbound` with `if e == Unbound::Ambiguous`, so no edit is expected.
  - **The other argument positions** (the prefix, `PARSE_ARGV <n>`, the trailing input) are unchanged.

**Checkpoint:** T004 passes. Spec 1539's suites (`cmake_fetch_wrappers` W-1 to W-26, the 18 unit tests) and spec 1538's `cmake_nested_fetches` pass unchanged.

## Phase 3: User Story 1 — a dependency fetched with `download_project()` is a component (P1) 🎯 MVP

**Goal:** every `download_project()` call through a DownloadProject-shaped definition binds as the `ExternalProject_Add` it generates, named `<PROJ>`, with the copy's own keywords.
**Independent test:** contract cases D-1 to D-14, D-16, D-17 and D-19, plus I-1, I-2 and I-7. D-18 is T004's.

- [X] T006 [US1] Add a helper and contract cases to `waybill-cli/tests/cmake_fetch_wrappers.rs`.
  - **Helper:** `dp_module(extra_keyword: &str, guard: Option<&str>) -> String` writes a DownloadProject-shaped `cmake/DownloadProject.cmake`, as in `contracts/wire.md`'s description of "DP": keyword lists in variables, `cmake_parse_arguments(DL_ARGS …)`, defaults, `configure_file()` of `DownloadProject.CMakeLists.cmake.in`, and two `execute_process()` calls. `extra_keyword` adds to `oneValueArgs` (D-8), and `guard` puts `set(oneValueArgs …)` under an `if()` (D-9).
  - **Cases:** D-1 to D-14, D-16, D-17 and D-19, as written in `contracts/wire.md`, in all three formats, checking PURL, version, mechanism, scope, C122, C232, C18 and C231 `wrappers`.
  - **Not yet:** C234 assertions are US2's (T010). Leave a `// C234: T010` marker at each.
  - **Prove they bite:** run against the reader after T005, before T007. Record which fail (expected: all but D-10, D-12 and D-19, which are controls).
  - **Done:** before T007, the 3 controls passed (D-10, D-12, D-19), D-18 passed (T005), and the other 13 failed. As written in the contract, D-4 and D-8 could not bite, because forwarding a DownloadProject keyword does not change the identity. Both now put the keyword before `URL`: consumed, it takes `URL` as its value; forwarded, the fetch reads `URL URL …` and names another version.
- [X] T007 [US1] Recognise DownloadProject in `Definitions::collect`, in `waybill-cli/src/scan_fs/package_db/cmake/wrappers.rs` (data-model "DownloadProject definition", research R1).
  - **The shape check:** for a definition whose lowercased name is `download_project`, before the wrapper fixed point:
    - find a `cmake_parse_arguments` with prefix `DL_ARGS` whose one-value list, resolved with T005's expansion over the body's unconditional literal `set()`s, contains `PROJ`;
    - find a `configure_file()` whose first argument's last `/`-segment is `DownloadProject.CMakeLists.cmake.in`.
  - **The synthesized call:** if both hold, replace that `configure_file()` body command with `ExternalProject_Add("${DL_ARGS_PROJ}" ${DL_ARGS_UNPARSED_ARGUMENTS})`, with no `if()` conditions. The fixed point then finds it as a wrapper with shapes `ParsedVariable` and `Unparsed`, and R2's identity check finds no identity.
  - **Unit tests:**
    - shaped → wrapper;
    - no `DL_ARGS` → not;
    - `PROJ` missing from the list → not;
    - another template name → not;
    - a function named `download_project` with neither → not;
    - for each of TheRock's three keyword lists (upstream, `+STATUS_MSG`, `−BUILD_PROJECT`), a call passing the listed keywords and a GitHub repository binds to exactly `["<PROJ>", "GIT_REPOSITORY", …, "GIT_TAG", …]`.
  - **Changed at implementation:**
    - A call with no `PROJ` must not bind with its next argument as the name, as `URL` would be. CMake keeps a quoted empty name, but the tokeniser does not record quoting. So a DownloadProject definition records `name_var = DL_ARGS_PROJ`, and binding returns `Unbound::MissingName` when it is unset.
    - A keyword list that cannot be resolved still passes the shape check, so binding reports it as `UnknownKeywords` (D-9), rather than the call going unread.
- [X] T008 [US1] Run T006's cases, and fix what fails in `waybill-cli/src/scan_fs/package_db/cmake/wrappers.rs` or `fetches.rs`. Record each fix and its cause in this task's text; "none" is a valid answer, stated as such. Expected to pass without extra code:
  - D-3's `<PROJ>` naming, from the synthesized name;
  - D-7's missing `PROJ`, where the unset local expands to nothing and gives no identity.
  - **Found:** nothing beyond T007's two changes. All 18 D cases passed once T007 was in.
- [X] T009 [US1] **The FR-008 control.** These pass unchanged, with no golden regenerated:
  - `cmake_fetch_wrappers` W-1 to W-26;
  - `cmake_nested_fetches` C-1 to C-15;
  - the CMake regression goldens (`cdx_regression`, `spdx_regression`, `spdx3_regression`).
  - **Done:** `cmake_nested_fetches` 22/22, `cmake_fetch_wrappers` W-1 to W-26 within 46/46, `cdx_regression` 11/11, `spdx_regression` 11/11, `spdx3_regression` 11/11. No golden regenerated; clippy clean.

**Checkpoint:** US1's cases pass; spec 1538 and 1539's cases and the goldens are unchanged.

## Phase 4: User Story 2 — the SBOM says how it was found (P2)

**Goal:** C234 names `download_project` on every component a call stands for, and on no other.
**Independent test:** I-8 on every case; D-15's chain is `[["add_dep","download_project"]]`.

- [X] T010 [US2] In `waybill-cli/tests/cmake_fetch_wrappers.rs`, replace T006's `// C234: T010` markers with assertions in all three formats: `[["download_project"]]` on D-1 to D-5, D-8, D-13, D-14, D-16 and D-17. Then add:
  - **D-15:** a project function `add_dep(name)` calling `download_project(PROJ ${name} ${ARGN})`, chain `[["add_dep","download_project"]]`;
  - **D-11's ambiguity count;**
  - **I-8**, in `waybill-cli/tests/common/cmake_fetch.rs`: a chain ending in `download_project` appears only where a `download_project` call is among the component's declarations. Approximate this per fixture by asserting no direct-only component carries it.
  - **Done:** every component-emitting D case asserts C234. D-15 checks the chain `[["add_dep","download_project"]]` with `wrappers` 2/1/0. I-8 is a fixture with a direct fetch beside a `download_project` call (`i8_…`), rather than a general invariant: which declarations a component stands for is not on the wire.
- [X] T011 [US2] Update C234's row in `docs/reference/sbom-format-mapping.md`: a chain may end in `download_project`, DownloadProject's function, recognised by name and shape (spec 1540). The value shape is unchanged, so the extractors need no change. The catalog gates (`sbom_format_mapping_coverage`, `mapping_doc_bidirectional`, `holistic_parity`) must still pass.

**Checkpoint:** every D case passes, with I-1 to I-8.

## Phase 5: User Story 3 — TheRock's `download_project` calls are measured in CI (P2)

**Goal:** SC-002 to SC-005 on TheRock.
**Independent test:** `census-fetches` and `measure-source` at the branch, against `main`.

- [ ] T012 [US3] Push the branch, and dispatch `census-fetches` in the fixture repository with `--ref main -f waybill_ref=1540-cmake-download-project -f base_ref=main`. Record in `specs/1540-cmake-download-project/measurements/therock.md`:
  - **C231 at both refs,** against `census.md`'s prediction: `read` +8, `wrappers.calls` +8, `wrappers.definitions` +9, no new unresolved. Explain every difference.
  - **SC-002:** each of the 8 calls, and what it became: a component (PURL, version, mechanism, scope, C234), or a merge (into which component, from C233).
  - **SC-003:** no PURL containing `${`, `DL_ARGS` or `-download`.
  - **SC-004:** V1 and V2.
  - **SC-008:** C234 naming `download_project` in all three formats.
  - **I-1:** the root's direct targets agree in the three formats (`specs/1537-requirements-root-attach/measurements/root_targets.py`).
  - **The sparse-tree scan times.**
- [ ] T013 [US3] Dispatch `measure-source` with the branch against `main` (interleaved: a warm-up, then three alternating rounds). Record the medians and their ratio in `measurements/therock.md` (SC-005, ≤ 1.5×).

## Phase 6: Polish and cross-cutting

- [X] T014 Mutations: `specs/1540-cmake-download-project/measurements/mutate.py` and `mutate-run.sh`, in spec 1539's pattern, recorded in `measurements/mutations.md`. At least these, each caught by a named test:
  - each shape condition removed in turn: the `DL_ARGS` prefix, `PROJ` in the list, the template name (×3);
  - the name check made case-sensitive;
  - the synthesized call named `${DL_ARGS_PROJ}-download`;
  - `${DL_ARGS_UNPARSED_ARGUMENTS}` dropped from it;
  - the synthesized call given the template's `if(${DL_ARGS_BUILD_PROJECT})` condition;
  - keyword lists read literally again (T005 reverted);
  - keyword lists spliced as arguments instead of expanded as one string;
  - `UnknownKeywords` treated as bound with an empty list;
  - `UnknownKeywords` not counted in `unresolved_identity`.
- [ ] T015 Corpora in CI. Record in `measurements/corpus.md`:
  - **Public corpus:** dispatch `public-corpus.yml` at the branch, and a `main` control at the same time. Expected: identical, or each change traced to a `download_project` call or a wrapper whose keyword lists are variables (T005).
  - **Quality corpus:** dispatch at the branch with a `main` control, and trace every movement PURL by PURL.
- [ ] T016 [P] Docs:
  - **`docs/ecosystems.md`, "Fetches made through wrappers":** DownloadProject recognised by name and shape; keywords from each copy; the `<PROJ>` name; the branch-tag gap (#1598).
  - **`CHANGELOG.md` `[Unreleased]`:** Fixed #1594, with TheRock's numbers from T012.
- [ ] T017 Run the gate alone and record it in `specs/1540-cmake-download-project/measurements/gate-log.md`:
  - the ok-line count, 0 failed, and the expected line found;
  - `cmake_fetch_wrappers` N/N and `cmake_nested_fetches` N/N;
  - no Cargo changes.
- [ ] T018 Open the PR against `main`.
  - **Title:** `feat(cmake): download_project() fetches (DownloadProject) (#1594)`.
  - **Body:** census, probe, TheRock before and after, corpora, mutations, the T005 change to spec 1539's binder, and `Refs #1598`. End with the attribution line; no session links.
  - **After it lands:** tick #1594 on tracker #1542.

## Dependencies

- **Setup** is done.
- **Foundational:** T004 → T005, which blocks everything.
- **US1:** T006 (tests first) → T007 → T008 → T009.
- **US2:** T010 needs T007; T011 can go beside it.
- **US3:** T012 and T013 need US1 and US2 pushed.
- **Polish:** T014 needs all stories. T015 needs the branch pushed. T016 can go after T012. T017 goes before T018.

## Parallel examples

- T011 (the docs row) and T016 (ecosystems, CHANGELOG) touch only documents, and can be written while T010's tests run.
- T012, T013 and T015 are CI dispatches, and run concurrently. Run nothing that compiles locally beside the gate.

## Implementation strategy

1. **MVP = Foundational + US1:** TheRock's 8 calls bind with each copy's keywords, and spec 1538 and 1539's behaviour is unchanged.
2. **US2** in the same PR: C234 is the evidence that a binding went through `download_project`.
3. **US3 and Polish** close the measurement loop before the PR is opened.
