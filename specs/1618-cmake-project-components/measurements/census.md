# T001 census

**Run**: `kusari-sandbox/waybill-fixture-therock-sdk` `census-projects` run 44746588322, fixture commit `14bf7c4`, script `census/projects.py`. All 4 jobs `success`.

**Trees**:
- TheRock `b1cae390`, with submodules (60 submodules);
- the quality-corpus targets at their pinned commits, without submodules, as the quality corpus clones them:
  - `cmake-nlohmann-json` `65ee6845`;
  - `python-pytorch` `2b3ec348`;
  - `cpp-mongo` `41a57524`.

A measuring script, not a model of waybill's reader. Its "reached" signal follows literal `add_subdirectory()` only.

## (a) Declarations and versions

| Tree | Declarations | Distinct names | literal | variable | of which resolved by a same-file `set()` | expression | none |
|---|---|---|---|---|---|---|---|
| TheRock | 811 | 650 | 165 | 60 | 15 | 6 | 580 |
| nlohmann-json | 17 | 6 | 1 | 0 | 0 | 0 | 16 |
| pytorch | 20 | 19 | 0 | 0 | 0 | 0 | 20 |
| mongo | 3 | 3 | 2 | 0 | 0 | 0 | 1 |

## (b) Searched names with a project declaration (SC-001's population)

| Tree | Searched names | With a declaration |
|---|---|---|
| TheRock | 266 | **81** |
| nlohmann-json | 3 | 1 |
| pytorch | 51 | 1 |
| mongo | 12 | 1 |

#1616's figure of 63 of 225 came from `census/includes.py` on the same commit. That script counts only names it classes as `first-party-project`, over different file and call filters. This census counts every literal first argument of `find_package`, `pkg_check_modules` and `pkg_search_module` in every `CMakeLists.txt` and `*.cmake` file. The two methods are not reconciled here. SC-001 is measured from waybill's own output in T026, not from either census.

## (c) Directory names (R8)

| Name | TheRock projects caught | Others |
|---|---|---|
| `tests` | 251 | nlohmann 8 |
| `test` | 67 | pytorch 8 |
| `examples` | 93 | |
| `samples` | 75 | |
| `example` | 14 | |
| `benchmarks` / `benchmark` / `bench` | 4 / 3 / 3 | nlohmann `benchmarks` 1 |

**Example and sample names.** All 70 projects caught by these names whose own names don't say example, sample, bench, demo, test or tutorial were inspected. They are sample and example programs: rocdecode `samples/*`, rocprofiler-systems `examples/*`, hip-tests `samples/*`. One is a vendored library inside an example (`examples/lulesh/external/kokkos`), which is built only for that example. **No first-party library is caught. Nothing is removed.**

**Missing name, added.** Projects under documentation directories and caught by no listed name:
- nlohmann-json: 8, `docs/mkdocs/docs/integration/*`, the `json_example` projects;
- TheRock: 19, the same vendored copies plus `caliper/doc`.

Spec 1615 treats `doc`, `docs` and `documentation` as non-runtime for pip, so **R8's example list gains `doc`, `docs` and `documentation`**. The C62 value `source-example-directory` covers example, sample, benchmark and documentation directories.

**Final lists:**
- Test (`TEST_DIRS`, unchanged): `test`, `tests`, `unittests`.
- Development: `example`, `examples`, `sample`, `samples`, `benchmark`, `benchmarks`, `bench`, `doc`, `docs`, `documentation`.

## (d) Candidate vendored signals

| Tree | path-segment | submodule-boundary | licence-file | not-reached |
|---|---|---|---|---|
| TheRock | 230 | 775 | 145 | 795 |
| nlohmann-json | 0 | 0 | 1 | 16 |
| pytorch | 3 | 0 | 2 | 14 |
| mongo | 3 | 0 | 2 | 0 |

`not-reached` holds for 795 of TheRock's 811 projects. TheRock builds its subprojects through its own functions, not `add_subdirectory`, so C239 will mark nearly every TheRock project. That is accurate, but it carries little information there (research R9).

## (e) Names in more than one directory

- **TheRock:** 63, of which only 2 have differing versions. Most are versionless test and example projects: `json_example` ×18, `dummyimport` ×15, `gtest` ×11, `gmock` ×10, `fmt-test` ×8. Under R5 each name with no versioned declaration is one versionless component, with every declaring file in its evidence.
- **Others:** nlohmann-json 2, pytorch 1.

## (f) Identity collisions (TheRock only; the others have none)

- **fetch (11):** `benchmark`, `cereal`, `composable_kernel`, `fmt`, `gtest`, `nlohmann_json`, `rocm-cmake`, `rocm-device-libs`, …
- **an installed `ConfigVersion` file (7):** `clang`, `dyninst`, `libhipcxx`, `lld`, `llvm`, `rocm`, `tensile`.
- **a submodule directory's name (21):** `caliper`, `cereal`, `dyninst`, `elfio`, `ffts`, `fin`, `fmt`, `glog`, …

Per FR-005, each of these is the existing component, and the project's file joins its evidence.
